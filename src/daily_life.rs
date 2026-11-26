//! Распорядок дня жителей Херсона: сон, утреня, работа, обед, рынок, термы, таверна, вечерня.
//!
//! Как это устроено:
//! * **Навигационная сетка** (`Nav`): клетки 0,5 м по полу города, где проходит человек
//!   (двери считаются открытыми, жители — нет). Строится один раз в фоновом потоке и общая
//!   для сервера и клиента в одном процессе.
//! * **Поля расстояний** (`Field`) до главных мест (агора, базилика, таверны, термы, порт…):
//!   житель идёт к месту «под горку» по полю, путь спрямляется.
//! * **План на сутки** (`DayPlans`): для каждого жителя по его роли — где и чем он занят
//!   в каждый момент. План — чистая функция от уровня и номера дня, поэтому сервер и все
//!   клиенты считают одно и то же и ничего не передают по сети, кроме часов
//!   (`world_clock.rs`). Далёких жителей никто не «симулирует»: положение получается из плана
//!   за пару сравнений, когда понадобится (LOD бесплатно).
//! * Ночью жители спят в постелях, в дождь прячутся по домам и тавернам, при драке рядом —
//!   приседают, закрыв голову, и отстают от своего распорядка ровно на это время.

use crate::collision::{Body, LevelColliders};
use crate::level::{Bed, Resident, Role};
use crate::world_clock::{DAY, TIME_SCALE, hash, rains_during, unit};
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------------------------------------
// Навигация
// ---------------------------------------------------------------------------------------------

/// Размер клетки навигационной сетки.
pub const CELL: f32 = 0.5;
/// Наибольший перепад пола между соседними клетками (ступенька).
const STEP: f32 = 0.42;
/// Тело для проверки прохода (чуть уже игрока, чтобы проходить в двери).
const NAV_BODY: Body = Body { half_width: 0.22, height: 1.7 };
const UNREACHED: u16 = u16::MAX;

/// Где можно ходить: высота пола в каждой клетке (NaN — нельзя).
pub struct Nav {
    x0: f32,
    z0: f32,
    nx: i32,
    nz: i32,
    floor: Vec<f32>,
}

const DIRS: [(i32, i32, u16); 8] = [(1, 0, 2), (-1, 0, 2), (0, 1, 2), (0, -1, 2), (1, 1, 3), (1, -1, 3), (-1, 1, 3), (-1, -1, 3)];

impl Nav {
    /// Сетка в прямоугольнике `min..max` (X/Z) по коллизиям уровня. Двери и жители не мешают.
    pub fn build(level: &LevelColliders, min: Vec2, max: Vec2) -> Nav {
        let nx = ((max.x - min.x) / CELL).ceil() as i32;
        let nz = ((max.y - min.y) / CELL).ceil() as i32;
        let mut floor = vec![f32::NAN; (nx * nz) as usize];
        for k in 0..nz {
            for i in 0..nx {
                let x = min.x + (i as f32 + 0.5) * CELL;
                let z = min.y + (k as f32 + 0.5) * CELL;
                if let Some(y) = standing_floor(level, x, z) {
                    floor[(k * nx + i) as usize] = y;
                }
            }
        }
        Nav { x0: min.x, z0: min.y, nx, nz, floor }
    }

    fn idx(&self, i: i32, k: i32) -> Option<usize> {
        (i >= 0 && k >= 0 && i < self.nx && k < self.nz).then(|| (k * self.nx + i) as usize)
    }

    fn ik(&self, c: usize) -> (i32, i32) {
        ((c as i32) % self.nx, (c as i32) / self.nx)
    }

    fn cell_at(&self, x: f32, z: f32) -> Option<usize> {
        self.idx(((x - self.x0) / CELL).floor() as i32, ((z - self.z0) / CELL).floor() as i32)
    }

    pub fn walkable(&self, c: usize) -> bool {
        !self.floor[c].is_nan()
    }

    fn point(&self, c: usize) -> Vec3 {
        let (i, k) = self.ik(c);
        Vec3::new(self.x0 + (i as f32 + 0.5) * CELL, self.floor[c], self.z0 + (k as f32 + 0.5) * CELL)
    }

    /// Соседи клетки, куда можно шагнуть (без срезания углов), и цена шага.
    fn neighbours(&self, c: usize) -> impl Iterator<Item = (usize, u16)> + '_ {
        let (i, k) = self.ik(c);
        let y = self.floor[c];
        DIRS.iter().filter_map(move |&(di, dk, cost)| {
            let n = self.idx(i + di, k + dk)?;
            let ok = |m: usize| !self.floor[m].is_nan() && (self.floor[m] - y).abs() <= STEP;
            if !ok(n) {
                return None;
            }
            if di != 0 && dk != 0 && !(self.idx(i + di, k).is_some_and(ok) && self.idx(i, k + dk).is_some_and(ok)) {
                return None;
            }
            Some((n, cost))
        })
    }

    /// Ближайшая проходимая клетка к точке (в радиусе `r` м), на похожей высоте.
    pub fn nearest(&self, p: Vec3, r: f32) -> Option<usize> {
        let (ci, ck) = (((p.x - self.x0) / CELL).floor() as i32, ((p.z - self.z0) / CELL).floor() as i32);
        let n = (r / CELL).ceil() as i32;
        let mut best: Option<(f32, usize)> = None;
        for dk in -n..=n {
            for di in -n..=n {
                let Some(c) = self.idx(ci + di, ck + dk) else { continue };
                if !self.walkable(c) || (self.floor[c] - p.y).abs() > 0.9 {
                    continue;
                }
                let d = (di * di + dk * dk) as f32;
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, c));
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// Можно ли пройти по прямой от `a` до `b` (для спрямления пути).
    fn straight(&self, a: Vec3, b: Vec3) -> bool {
        let d = Vec2::new(b.x - a.x, b.z - a.z);
        let steps = (d.length() / (CELL * 0.4)).ceil().max(1.0) as i32;
        let mut prev = a.y;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let (x, z) = (a.x + d.x * t, a.z + d.y * t);
            let Some(c) = self.cell_at(x, z) else { return false };
            let y = self.floor[c];
            if y.is_nan() || (y - prev).abs() > STEP {
                return false;
            }
            // Не срезаем угол стены: соседние клетки вокруг тоже должны быть проходимы.
            let (i, k) = self.ik(c);
            let fx = (x - self.x0) / CELL - i as f32;
            let fz = (z - self.z0) / CELL - k as f32;
            let si = if fx < 0.5 { -1 } else { 1 };
            let sk = if fz < 0.5 { -1 } else { 1 };
            for (di, dk) in [(si, 0), (0, sk), (si, sk)] {
                if self.idx(i + di, k + dk).is_none_or(|m| self.floor[m].is_nan()) {
                    return false;
                }
            }
            prev = y;
        }
        true
    }

    /// Путь для бегущего по делу (стражник на крик): A* с запасом 40 м вокруг.
    pub fn route(&self, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
        self.local_path(from, to, 40.0)
    }

    /// Путь между близкими точками: A* в прямоугольнике вокруг них (с запасом `margin` м).
    fn local_path(&self, from: Vec3, to: Vec3, margin: f32) -> Option<Vec<Vec3>> {
        use std::cmp::Reverse;
        let a = self.nearest(from, 1.6)?;
        let b = self.nearest(to, 1.6)?;
        let ((ai, ak), (bi, bk)) = (self.ik(a), self.ik(b));
        let m = (margin / CELL) as i32;
        let (i0, i1, k0, k1) = (ai.min(bi) - m, ai.max(bi) + m, ak.min(bk) - m, ak.max(bk) + m);
        let w = i1 - i0 + 1;
        let h = k1 - k0 + 1;
        if (w * h) as usize > 600_000 {
            return None;
        }
        let local = |c: usize| -> Option<usize> {
            let (i, k) = self.ik(c);
            (i >= i0 && i <= i1 && k >= k0 && k <= k1).then(|| ((k - k0) * w + (i - i0)) as usize)
        };
        let heur = |c: usize| {
            let (i, k) = self.ik(c);
            let (dx, dz) = ((i - bi).unsigned_abs(), (k - bk).unsigned_abs());
            2 * dx.max(dz) + dx.min(dz)
        };
        let mut g = vec![u32::MAX; (w * h) as usize];
        let mut prev = vec![u32::MAX; (w * h) as usize];
        let mut heap = std::collections::BinaryHeap::new();
        g[local(a)?] = 0;
        heap.push(Reverse((heur(a), a as u32)));
        while let Some(Reverse((_, c))) = heap.pop() {
            let c = c as usize;
            if c == b {
                break;
            }
            let gc = g[local(c)?];
            for (n, cost) in self.neighbours(c) {
                let Some(ln) = local(n) else { continue };
                let ng = gc + cost as u32;
                if ng < g[ln] {
                    g[ln] = ng;
                    prev[ln] = c as u32;
                    heap.push(Reverse((ng + heur(n), n as u32)));
                }
            }
        }
        if g[local(b)?] == u32::MAX {
            return None;
        }
        let mut cells = vec![b];
        let mut c = b;
        while c != a {
            c = prev[local(c)?] as usize;
            cells.push(c);
        }
        cells.reverse();
        let mut pts: Vec<Vec3> = std::iter::once(from).chain(cells.iter().map(|c| self.point(*c))).collect();
        pts.push(to);
        Some(self.smooth(&pts))
    }

    /// Поле расстояний до клеток `goals` (алгоритм Дейкстры с корзинами).
    pub fn field(&self, goals: &[usize]) -> Field {
        let mut dist = vec![UNREACHED; self.floor.len()];
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); 4];
        for &g in goals {
            if self.walkable(g) {
                dist[g] = 0;
                buckets[0].push(g as u32);
            }
        }
        let mut d: u32 = 0;
        let mut empty = 0;
        while empty < 4 {
            let b = (d % 4) as usize;
            let cur = std::mem::take(&mut buckets[b]);
            if cur.is_empty() {
                empty += 1;
                d += 1;
                continue;
            }
            empty = 0;
            for c in cur {
                let c = c as usize;
                if dist[c] as u32 != d {
                    continue;
                }
                for (n, cost) in self.neighbours(c) {
                    let nd = d + cost as u32;
                    if nd < dist[n] as u32 && nd < UNREACHED as u32 {
                        dist[n] = nd as u16;
                        buckets[(nd % 4) as usize].push(n as u32);
                    }
                }
            }
            d += 1;
        }
        Field { dist }
    }

    /// Путь от `from` вниз по полю до цели (точки по клеткам).
    fn descend(&self, field: &Field, from: usize) -> Option<Vec<usize>> {
        if field.dist[from] == UNREACHED {
            return None;
        }
        let mut path = vec![from];
        let mut c = from;
        while field.dist[c] > 0 {
            let next = self.neighbours(c).filter(|(n, _)| field.dist[*n] < field.dist[c]).min_by_key(|(n, cost)| (field.dist[*n], *cost))?;
            c = next.0;
            path.push(c);
            if path.len() > 20_000 {
                return None;
            }
        }
        Some(path)
    }

    /// Спрямление: из каждой точки идём к самой дальней, видимой напрямую.
    fn smooth(&self, pts: &[Vec3]) -> Vec<Vec3> {
        if pts.len() <= 2 {
            return pts.to_vec();
        }
        let mut out = vec![pts[0]];
        let mut i = 0;
        while i < pts.len() - 1 {
            // Двоичный поиск дальней видимой (с проверкой нескольких промежуточных).
            let mut j = (i + 1).max((pts.len() - 1).min(i + 60));
            while j > i + 1 && !self.straight(pts[i], pts[j]) {
                j = i + 1 + (j - i - 1) * 2 / 3;
            }
            out.push(pts[j]);
            i = j;
        }
        out
    }
}

/// Пол под человеком в точке: самая нижняя опора (земля или верх коробки не выше колена над
/// землёй), над которой человек помещается. Верхние этажи не считаются.
fn standing_floor(level: &LevelColliders, x: f32, z: f32) -> Option<f32> {
    // Над морем (причалы, набережная) мерило — уровень города, а не дно.
    let natural = crate::level::ground_y(x, z).max(if crate::level::ground_y(x, z) < crate::level::WATER_Y { 0.0 } else { f32::MIN });
    let hf = level.ground_height(x, z);
    // Выше 0,4 м над землёй — уже не пол, а лавка, ложе или стол.
    let probe = crate::collision::Aabb::new(Vec3::new(x - 0.01, natural - 1.5, z - 0.01), Vec3::new(x + 0.01, natural + 0.4, z + 0.01));
    let mut cands: Vec<f32> = level.boxes_touching(probe).map(|b| b.max.y).filter(|y| *y <= natural + 0.4).collect();
    if hf.is_finite() {
        cands.push(hf);
    }
    cands.sort_by(|a, b| a.total_cmp(b));
    for y in cands {
        // Под водой не ходят.
        if y < crate::level::WATER_Y + 0.25 {
            continue;
        }
        let inside = level.point_solid(Vec3::new(x, y + 0.02, z));
        if !inside && level.body_is_free(&NAV_BODY, Vec3::new(x, y + 0.03, z)) {
            return Some(y);
        }
    }
    None
}

/// Расстояния (в половинках клетки) до цели.
pub struct Field {
    dist: Vec<u16>,
}

impl Field {
    fn reached(&self, c: usize) -> bool {
        self.dist[c] != UNREACHED
    }

    /// Примерно в метрах.
    fn meters(&self, c: usize) -> f32 {
        self.dist[c] as f32 * CELL * 0.5
    }
}

// ---------------------------------------------------------------------------------------------
// Места, куда ходят жители
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dest {
    Agora,
    Basilica,
    ChurchWest,
    Tavern,
    TavernPort,
    Thermae,
    Quay,
    Horrea,
    PortMarket,
    MainGate,
    PortGate,
    SouthGate,
    Garrison,
    Reservoir,
    ApothecaryAgora,
    Library,
}

impl Dest {
    pub const ALL: [Dest; 16] = [
        Dest::Agora,
        Dest::Basilica,
        Dest::ChurchWest,
        Dest::Tavern,
        Dest::TavernPort,
        Dest::Thermae,
        Dest::Quay,
        Dest::Horrea,
        Dest::PortMarket,
        Dest::MainGate,
        Dest::PortGate,
        Dest::SouthGate,
        Dest::Garrison,
        Dest::Reservoir,
        Dest::ApothecaryAgora,
        Dest::Library,
    ];

    pub fn place_id(self) -> &'static str {
        match self {
            Dest::Agora => "agora",
            Dest::Basilica => "basilica",
            Dest::ChurchWest => "church_west",
            Dest::Tavern => "tavern",
            Dest::TavernPort => "tavern_port",
            Dest::Thermae => "thermae",
            Dest::Quay => "quay",
            Dest::Horrea => "horrea",
            Dest::PortMarket => "port_market",
            Dest::MainGate => "main_gate",
            Dest::PortGate => "port_gate",
            Dest::SouthGate => "south_gate",
            Dest::Garrison => "garrison",
            Dest::Reservoir => "reservoir",
            Dest::ApothecaryAgora => "apothecary_agora",
            Dest::Library => "library",
        }
    }

    /// Насколько далеко от точки места раскидывать людей.
    fn spread(self) -> f32 {
        match self {
            Dest::Agora | Dest::PortMarket => 16.0,
            Dest::Quay => 30.0,
            Dest::Horrea | Dest::Thermae | Dest::Basilica => 9.0,
            Dest::MainGate | Dest::PortGate | Dest::SouthGate => 6.0,
            _ => 6.0,
        }
    }
}

/// Место для одного-двух человек: точка, взгляд и точка собеседника (лицом к лицу).
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub pos: Vec3,
    pub yaw: f32,
    pub partner: Vec3,
}

pub fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    if d.x * d.x + d.z * d.z < 1e-6 { 0.0 } else { (-d.x).atan2(-d.z) }
}

/// Направление взгляда по `yaw`.
pub fn forward(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

// ---------------------------------------------------------------------------------------------
// Занятия
// ---------------------------------------------------------------------------------------------

/// Чем человек занят (по этому клиент выбирает позу и предмет в руках).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Act {
    /// Спит в постели.
    Sleep,
    /// Ушёл в дом/наверх — не виден.
    Away,
    Idle,
    /// Работа, склонившись над делом (верстак, гончарный круг, кухня).
    Work,
    /// Кует молотом у наковальни.
    Hammer,
    /// Мешает в котле.
    Stir,
    /// Ткёт у станка.
    Weave,
    /// Пишет, читает.
    Write,
    /// Учит, наставляет (жесты рукой).
    Teach,
    /// Лечит: присел над больным.
    Treat,
    /// Лежит (больной).
    Lie,
    /// Молится, воздев руки (оранта).
    Pray,
    /// Служит: священник у алтаря.
    Preach,
    /// Разговаривает с соседом.
    Talk,
    /// Разглядывает товар на рынке.
    Browse,
    /// Раскладывает товар.
    Arrange,
    Eat,
    Drink,
    /// Несёт груз (амфору, мешок, кувшин).
    Carry,
    /// Удит рыбу с набережной.
    Fish,
    /// Метёт двор.
    Sweep,
    /// Стоит на посту.
    Guard,
    /// Мотыжит землю.
    Hoe,
    /// Моется в термах (стоит в тепле без дела).
    Bathe,
    /// Присел у сундука, пифоса или полки: достаёт или укладывает припасы.
    Fetch,
    /// Присел у очага: подкладывает дрова, ворошит угли.
    Tend,
    /// У горна: держит заготовку клещами в углях и качает мехи.
    Stoke,
    /// Закаляет поковку в корыте.
    Quench,
    /// Присел и закрыл голову: рядом драка.
    Cower,
}

impl Act {
    /// Как сказать, чем человек занят («куёт», «молится»).
    pub fn name(self) -> &'static str {
        match self {
            Act::Sleep => "спит",
            Act::Away => "дома",
            Act::Idle => "отдыхает",
            Act::Work => "работает",
            Act::Hammer => "куёт",
            Act::Stir => "стряпает",
            Act::Weave => "ткёт",
            Act::Write => "пишет",
            Act::Teach => "учит",
            Act::Treat => "лечит",
            Act::Lie => "лежит",
            Act::Pray => "молится",
            Act::Preach => "служит",
            Act::Talk => "беседует",
            Act::Browse => "приценивается",
            Act::Arrange => "раскладывает товар",
            Act::Eat => "ест",
            Act::Drink => "пьёт вино",
            Act::Carry => "несёт груз",
            Act::Fish => "удит рыбу",
            Act::Sweep => "метёт",
            Act::Guard => "на посту",
            Act::Hoe => "мотыжит",
            Act::Bathe => "в банях",
            Act::Fetch => "достаёт припасы",
            Act::Tend => "подкладывает дрова",
            Act::Stoke => "греет заготовку в горне",
            Act::Quench => "закаляет поковку",
            Act::Cower => "прячется",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// План на сутки
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Stay {
    pub t0: f64,
    #[allow(dead_code)] // конец пребывания: для проверок плана
    pub t1: f64,
    pub pos: Vec3,
    pub yaw: f32,
    pub act: Act,
    /// Собеседник (номер жителя).
    pub partner: Option<u16>,
    /// Лежит в постели: её середина (`pos` — место у постели).
    pub lie: Option<Vec3>,
    /// Дела по соседству, на которые житель отходит от основного (к горну, в кладовую…).
    pub rounds: Option<Arc<Rounds>>,
}

#[derive(Clone, Debug)]
pub struct Walk {
    pub t0: f64,
    pub t1: f64,
    pub pts: Vec<Vec3>,
    /// Накопленная длина пути до каждой точки.
    pub cum: Vec<f32>,
    /// Несёт груз.
    pub carry: bool,
    /// Идёт патрулём (медленнее, с копьём).
    pub patrol: bool,
}

#[derive(Clone, Debug)]
pub enum Seg {
    Stay(Stay),
    Walk(Walk),
}

impl Seg {
    fn t0(&self) -> f64 {
        match self {
            Seg::Stay(s) => s.t0,
            Seg::Walk(w) => w.t0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub segs: Vec<Seg>,
}

/// Где житель и что делает в момент `t` (секунды от полуночи).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcState {
    pub pos: Vec3,
    pub yaw: f32,
    pub act: Act,
    /// Идёт (и сколько прошёл — для фазы шага).
    pub walking: bool,
    pub walked: f32,
    pub carry: bool,
    pub patrol: bool,
    pub partner: Option<u16>,
    /// Лежит в постели: её середина.
    pub lie: Option<Vec3>,
}

impl Plan {
    pub fn state(&self, t: f64) -> Option<NpcState> {
        if self.segs.is_empty() {
            return None;
        }
        let i = self.segs.partition_point(|s| s.t0() <= t).saturating_sub(1);
        Some(match &self.segs[i] {
            Seg::Stay(s) => {
                let base = NpcState { pos: s.pos, yaw: s.yaw, act: s.act, walking: false, walked: 0.0, carry: false, patrol: false, partner: s.partner, lie: s.lie };
                s.rounds.as_ref().and_then(|r| r.state(s, t, &base)).unwrap_or(base)
            }
            Seg::Walk(w) => {
                let total = *w.cum.last().unwrap_or(&0.0);
                let k = ((t - w.t0) / (w.t1 - w.t0).max(1e-3)).clamp(0.0, 1.0) as f32;
                let d = total * k;
                let (pos, a, b) = along(&w.pts, &w.cum, d);
                NpcState { pos, yaw: yaw_to(a, b), act: if w.carry { Act::Carry } else { Act::Idle }, walking: k < 1.0, walked: d, carry: w.carry, patrol: w.patrol, partner: None, lie: None }
            }
        })
    }

}

/// Точка на ломаной `pts` в `d` метрах от начала (`cum` — накопленные длины) и её отрезок.
fn along(pts: &[Vec3], cum: &[f32], d: f32) -> (Vec3, Vec3, Vec3) {
    if pts.len() < 2 {
        let p = pts.first().copied().unwrap_or(Vec3::ZERO);
        return (p, p, p);
    }
    let j = cum.partition_point(|c| *c <= d).clamp(1, pts.len() - 1);
    let (a, b) = (pts[j - 1], pts[j]);
    let seg = (cum[j] - cum[j - 1]).max(1e-4);
    (a.lerp(b, ((d - cum[j - 1]) / seg).clamp(0.0, 1.0)), a, b)
}

/// Чем занят по дороге: несёт ли что-то и что делает руками (метёт на ходу, несёт поковку).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Way {
    pub carry: bool,
    pub act: Act,
}

impl Way {
    const EMPTY: Way = Way { carry: false, act: Act::Idle };
    const LOADED: Way = Way { carry: true, act: Act::Carry };
}

/// Отлучка от рабочего места: дойти до `pts.last()`, сделать там `act` и вернуться той же дорогой.
#[derive(Clone, Debug, PartialEq)]
pub struct Errand {
    pub pts: Vec<Vec3>,
    pub cum: Vec<f32>,
    /// Куда смотрит на месте.
    pub yaw: f32,
    pub act: Act,
    /// Сколько настоящих секунд занят на месте.
    pub secs: f32,
    /// Скорость шага, м за настоящую секунду.
    pub speed: f32,
    pub there: Way,
    pub back: Way,
}

impl Errand {
    fn len(&self) -> f32 {
        *self.cum.last().unwrap_or(&0.0)
    }

    /// Сколько настоящих секунд на всю отлучку.
    fn secs_total(&self) -> f32 {
        2.0 * self.len() / self.speed + self.secs
    }
}

/// Дела по соседству с основным: время делится на отрезки по `slot` настоящих секунд;
/// в конце отрезка житель отлучается по одному из дел (выбор — от номера отрезка), в начале —
/// за основным делом. Всё — чистая функция времени: сервер и клиенты видят одно и то же.
#[derive(Clone, Debug, PartialEq)]
pub struct Rounds {
    pub errands: Vec<Errand>,
    pub slot: f32,
    pub seed: u64,
}

impl Rounds {
    fn state(&self, s: &Stay, t: f64, base: &NpcState) -> Option<NpcState> {
        if self.errands.is_empty() || t < s.t0 {
            return None;
        }
        let real = ((t - s.t0) / TIME_SCALE) as f32;
        let n = (real / self.slot).floor();
        // Отлучка не должна застать уход дальше по плану: последний неполный отрезок — дома.
        if s.t0 + ((n + 1.0) * self.slot) as f64 * TIME_SCALE > s.t1 {
            return None;
        }
        let h = hash(self.seed, n as u64);
        // Каждый пятый отрезок — целиком за основным делом.
        if h % 5 == 0 {
            return None;
        }
        let e = &self.errands[((h / 5) % self.errands.len() as u64) as usize];
        let total = e.secs_total();
        let start = self.slot - total;
        let k = real - n * self.slot - start;
        if start < self.slot * 0.3 || k < 0.0 {
            return None;
        }
        let len = e.len();
        let walk = len / e.speed;
        let mut st = NpcState { partner: None, lie: None, ..*base };
        if k < walk {
            let d = k * e.speed;
            let (pos, a, b) = along(&e.pts, &e.cum, d);
            st = NpcState { pos, yaw: yaw_to(a, b), act: e.there.act, walking: true, walked: d, carry: e.there.carry, ..st };
        } else if k < walk + e.secs {
            st = NpcState { pos: *e.pts.last()?, yaw: e.yaw, act: e.act, walking: false, walked: 0.0, carry: false, ..st };
        } else {
            let back = ((k - walk - e.secs) * e.speed).min(len);
            let (pos, a, b) = along(&e.pts, &e.cum, len - back);
            st = NpcState { pos, yaw: yaw_to(b, a), act: e.back.act, walking: true, walked: len + back, carry: e.back.carry, ..st };
        }
        Some(st)
    }
}

/// Планы всех жителей на один день.
pub struct DayPlans {
    pub day: u32,
    pub people: Vec<Plan>,
}

// ---------------------------------------------------------------------------------------------
// Мир жителей: сетка, поля, постели, планы
// ---------------------------------------------------------------------------------------------

/// Всё неизменное, что нужно для планов (строится один раз в фоне).
pub struct Town {
    pub nav: Nav,
    pub residents: Vec<Resident>,
    /// Клетка дома (якоря) каждого жителя, если он на сетке.
    home_cell: Vec<Option<usize>>,
    /// Постель каждого жителя.
    bed: Vec<Option<Bed>>,
    /// Наковальня рядом (кузнецы).
    anvil: Vec<Option<(Vec3, Vec3)>>,
    /// Ремесло, если не по роли (кузнец у горна, носильщик, рыбак).
    job: Vec<Job>,
    fields: HashMap<Dest, Field>,
    spots: HashMap<Dest, Vec<Spot>>,
    /// Места для удочки: у края набережной лицом к воде.
    fishing: Vec<Spot>,
    /// Места на улицах и во дворах (поболтать с соседями у дома, детям — поиграть).
    street: Vec<Spot>,
    /// Ближайшее к дому жителя место на улице.
    street_of: Vec<Option<usize>>,
    /// Места по соседству с рабочим местом каждого жителя: кладовая, очаг, горн, корыто.
    stations: Vec<Vec<Station>>,
}

/// Что стоит у места, куда житель отходит от основного дела.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Сундук, пифос, полка с припасами.
    Store,
    Hearth,
    /// Угли кузнечного горна.
    Fire,
    /// Корыто для закалки.
    Trough,
    /// Пара шагов в сторону (пройтись, подмести дальше, оглядеться).
    Pace,
}

/// Место по соседству: откуда идти (`from` — рабочее место), где встать и куда смотреть.
#[derive(Clone, Debug)]
struct Station {
    from: Vec3,
    kind: Kind,
    yaw: f32,
    pts: Vec<Vec3>,
    cum: Vec<f32>,
}

/// Проходимая клетка у предмета в `target` на полу высоты `floor_y`: не вплотную и не дальше `reach` м.
fn stand_near(nav: &Nav, target: Vec3, floor_y: f32, reach: f32) -> Option<Vec3> {
    let (ci, ck) = (((target.x - nav.x0) / CELL).floor() as i32, ((target.z - nav.z0) / CELL).floor() as i32);
    let n = (reach / CELL).ceil() as i32;
    let mut best: Option<(f32, usize)> = None;
    for dk in -n..=n {
        for di in -n..=n {
            let Some(c) = nav.idx(ci + di, ck + dk) else { continue };
            if !nav.walkable(c) || (nav.floor[c] - floor_y).abs() > 0.45 {
                continue;
            }
            let p = nav.point(c);
            let d = Vec2::new(p.x - target.x, p.z - target.z).length();
            if !(0.45..=reach).contains(&d) {
                continue;
            }
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| nav.point(c))
}

/// Дорога от рабочего места `from` к месту `stand` (короткая, без обходов через соседний дом).
fn errand_path(nav: &Nav, from: Vec3, stand: Vec3, max_len: f32) -> Option<(Vec<Vec3>, Vec<f32>)> {
    let direct = from.distance(stand);
    if !(0.3..=max_len).contains(&direct) {
        return None;
    }
    let pts = if nav.straight(from, stand) { vec![from, stand] } else { nav.local_path(from, stand, 3.0)? };
    let mut cum = vec![0.0];
    for w in pts.windows(2) {
        cum.push(cum.last().unwrap() + w[0].distance(w[1]));
    }
    let len = *cum.last().unwrap();
    (len <= max_len && len <= direct * 1.8 + 1.5).then_some((pts, cum))
}

impl Station {
    fn new(nav: &Nav, from: Vec3, target: Vec3, kind: Kind) -> Option<Station> {
        // Угли горна — в глубине кладки: к ним тянутся клещами с края.
        let reach = if kind == Kind::Fire { 2.3 } else { 1.6 };
        let stand = stand_near(nav, target, from.y, reach)?;
        // К горну — хоть шаг; к остальному ближе метра не «ходят»: и так под рукой.
        if kind != Kind::Fire && from.distance(stand) < 0.9 {
            return None;
        }
        let (pts, cum) = errand_path(nav, from, stand, 12.0)?;
        Some(Station { from, kind, yaw: yaw_to(stand, target), pts, cum })
    }

    /// Пара шагов в сторону: открытая клетка в 2–3,5 м по прямой.
    fn pace(nav: &Nav, from: Vec3, seed: u64) -> Option<Station> {
        (0..12).find_map(|k| {
            let a = (unit(seed, k) + k as f32 / 12.0) * std::f32::consts::TAU;
            let r = 2.0 + 1.5 * unit(seed, k + 50);
            let q = from + Vec3::new(a.cos(), 0.0, a.sin()) * r;
            let c = nav.cell_at(q.x, q.z).filter(|c| nav.walkable(*c) && (nav.floor[*c] - from.y).abs() < 0.3)?;
            let q = nav.point(c);
            nav.straight(from, q).then(|| {
                let d = from.distance(q);
                Station { from, kind: Kind::Pace, yaw: yaw_to(from, q), pts: vec![from, q], cum: vec![0.0, d] }
            })
        })
    }
}

static TOWN: OnceLock<Town> = OnceLock::new();
static STARTED: OnceLock<()> = OnceLock::new();
static PLANS: Mutex<Vec<Arc<DayPlans>>> = Mutex::new(Vec::new());
static PLANNING: Mutex<Option<u32>> = Mutex::new(None);

/// Готов ли город (сетка и поля посчитаны).
pub fn town() -> Option<&'static Town> {
    TOWN.get()
}

/// Запускает постройку города в фоне (один раз на процесс). Только для Херсонеса.
pub fn warm_up() {
    if crate::level::active() != crate::level::LevelKind::Chersonesus || STARTED.set(()).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("daily-life".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let started = std::time::Instant::now();
            let town = Town::build(crate::level::shared().clone());
            info!("Распорядок жителей: город готов за {:.1} с", started.elapsed().as_secs_f32());
            let _ = TOWN.set(town);
        })
        .ok();
}

/// План на день `day`, если уже посчитан; иначе ставит его в очередь (в фоне).
pub fn plans(day: u32) -> Option<Arc<DayPlans>> {
    let town = town()?;
    if let Some(p) = PLANS.lock().ok()?.iter().find(|p| p.day == day) {
        return Some(p.clone());
    }
    request(town, day);
    None
}

/// Заранее считает план на день (например, завтрашний — вечером).
pub fn prefetch(day: u32) {
    if let Some(town) = town()
        && !PLANS.lock().is_ok_and(|p| p.iter().any(|p| p.day == day))
    {
        request(town, day);
    }
}

fn request(town: &'static Town, day: u32) {
    let Ok(mut busy) = PLANNING.lock() else { return };
    if busy.is_some() {
        return;
    }
    *busy = Some(day);
    std::thread::Builder::new()
        .name("day-plan".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let plans = Arc::new(town.plan_day(day));
            if let Ok(mut all) = PLANS.lock() {
                all.retain(|p| p.day + 1 >= day && p.day <= day + 1);
                all.push(plans);
            }
            if let Ok(mut b) = PLANNING.lock() {
                *b = None;
            }
        })
        .ok();
}

/// Границы сетки: весь город и порт (по точкам жителей и мест), с запасом.
fn nav_bounds(residents: &[Resident]) -> (Vec2, Vec2) {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    let add = |p: Vec3, min: &mut Vec2, max: &mut Vec2| {
        *min = min.min(Vec2::new(p.x, p.z));
        *max = max.max(Vec2::new(p.x, p.z));
    };
    for d in Dest::ALL {
        if let Some(p) = crate::level::places::place_pos(d.place_id()) {
            add(p, &mut min, &mut max);
        }
    }
    let (a, b) = (min, max);
    for r in residents {
        // Жители за стенами (усадьбы, деревни) остаются у себя: сетку на всю округу не строим.
        let inside = crate::level::inside_city(r.pos.x, r.pos.z) || (r.pos.x > a.x - 40.0 && r.pos.x < b.x + 40.0 && r.pos.z > a.y - 40.0 && r.pos.z < b.y + 40.0);
        if inside {
            add(r.pos, &mut min, &mut max);
        }
    }
    (min - Vec2::splat(30.0), max + Vec2::splat(30.0))
}

impl Town {
    pub fn build(layout: crate::level::Layout) -> Town {
        let residents = layout.residents.clone();
        let beds = layout.beds.clone();
        let anvils: Vec<Vec3> = layout.forges.iter().map(|f| f.anvil).collect();
        let forges = layout.forges.clone();
        let stores: Vec<Vec3> = layout.containers.iter().map(|c| c.pos).collect();
        let hearths = layout.hearths.clone();
        let (min, max) = nav_bounds(&residents);
        let level = crate::level::colliders(layout);
        level.set_all_doors(true);
        for i in 0..residents.len() {
            level.set_body_gone(i, true);
        }
        let nav = Nav::build(&level, min, max);

        let mut dest_cells = HashMap::new();
        let mut fields = HashMap::new();
        for d in Dest::ALL {
            let Some(p) = crate::level::places::place_pos(d.place_id()) else { continue };
            let Some(c) = nav.nearest(p, 4.0) else { continue };
            dest_cells.insert(d, c);
            fields.insert(d, nav.field(&[c]));
        }
        // Главная площадь — мерило связности: дом, откуда не дойти до агоры, считается «вне сетки».
        let home_cell: Vec<Option<usize>> = residents
            .iter()
            .map(|r| nav.nearest(r.pos, 1.2).filter(|c| fields.get(&Dest::Agora).is_none_or(|f| f.reached(*c))))
            .collect();

        // Постель: ближайшая к дому на том же этаже (не дальше 9 м), по одной на человека.
        let mut taken = vec![false; beds.len()];
        let mut bed = vec![None; residents.len()];
        let mut order: Vec<usize> = (0..residents.len()).collect();
        order.sort_by_key(|&i| hash(i as u64, 0xBED));
        for i in order {
            let r = &residents[i];
            let best = beds
                .iter()
                .enumerate()
                .filter(|(j, b)| !taken[*j] && (b.pos.y - 0.62 - r.pos.y).abs() < 0.8 && Vec2::new(b.pos.x - r.pos.x, b.pos.z - r.pos.z).length() < 9.0)
                .min_by(|a, b| a.1.pos.distance_squared(r.pos).total_cmp(&b.1.pos.distance_squared(r.pos)));
            if let Some((j, b)) = best {
                taken[j] = true;
                bed[i] = Some(*b);
            }
        }
        // У каждого горна в городе — мастер с клещами и молотобоец напротив (из ближайших мужчин).
        let mut anvil: Vec<Option<(Vec3, Vec3)>> = vec![None; residents.len()];
        let mut job = vec![Job::Default; residents.len()];
        for a in anvils {
            let sides: Vec<Vec3> = (0..8)
                .filter_map(|k| {
                    let ang = k as f32 * std::f32::consts::FRAC_PI_4;
                    let q = a + Vec3::new(ang.cos(), 0.0, ang.sin()) * 0.8;
                    nav.cell_at(q.x, q.z).filter(|c| nav.walkable(*c) && (nav.floor[*c] - a.y).abs() < 0.3 && fields.get(&Dest::Agora).is_some_and(|f| f.reached(*c))).map(|c| nav.point(c))
                })
                .collect();
            let Some(&first) = sides.first() else { continue };
            let second = sides.iter().copied().max_by(|x, y| x.distance(first).total_cmp(&y.distance(first))).filter(|s| s.distance(first) > 1.0);
            let mut men: Vec<(f32, usize)> = residents
                .iter()
                .enumerate()
                .filter(|(i, r)| matches!(r.role, Role::Craftsman | Role::Householder) && home_cell[*i].is_some() && job[*i] == Job::Default && r.pos.distance(a) < 45.0)
                .map(|(i, r)| (r.pos.distance(a), i))
                .collect();
            men.sort_by(|x, y| x.0.total_cmp(&y.0));
            for ((_, i), side) in men.into_iter().zip([Some(first), second]) {
                let Some(side) = side else { break };
                anvil[i] = Some((side, a));
                job[i] = Job::Smith;
            }
        }
        // Мужчины у порта: каждый четвёртый — носильщик, каждый четвёртый — рыбак.
        if let (Some(fq), Some(fh)) = (fields.get(&Dest::Quay), fields.get(&Dest::Horrea)) {
            for (i, r) in residents.iter().enumerate() {
                let Some(c) = home_cell[i] else { continue };
                if r.role != Role::Householder || job[i] != Job::Default || !fq.reached(c) || fq.meters(c) > 260.0 {
                    continue;
                }
                job[i] = match hash(i as u64, 0x9047) % 4 {
                    0 if fh.reached(c) => Job::Porter,
                    1 => Job::Fisher,
                    _ => Job::Default,
                };
            }
        }

        let mut spots = HashMap::new();
        for d in Dest::ALL {
            let (Some(f), Some(&c)) = (fields.get(&d), dest_cells.get(&d)) else { continue };
            spots.insert(d, make_spots(&nav, f, c, d));
        }
        let fishing = fishing_spots(&nav, &fields, &dest_cells);
        let street = street_spots(&nav, fields.get(&Dest::Agora));
        let street_of = residents
            .iter()
            .zip(&home_cell)
            .map(|(r, c)| {
                c.as_ref()?;
                let mut near: Vec<(f32, usize)> = street.iter().enumerate().map(|(k, s)| (s.pos.distance(r.pos), k)).filter(|(d, k)| *d < 26.0 && (street[*k].pos.y - r.pos.y).abs() < 1.0).collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                near.into_iter().take(3).map(|(_, k)| k).find(|k| nav.local_path(r.pos, street[*k].pos, 10.0).is_some())
            })
            .collect();
        // Места по соседству: ближайшие сундуки и пифосы, очаг, пара шагов в сторону; у кузнецов —
        // угли горна и корыто для закалки.
        let stations = residents
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut v = Vec::new();
                if home_cell[i].is_none() {
                    return v;
                }
                let home = r.pos;
                let near = |pts: &[Vec3], max: f32| -> Vec<Vec3> {
                    let mut c: Vec<(f32, Vec3)> = pts.iter().map(|p| (Vec2::new(p.x - home.x, p.z - home.z).length(), *p)).filter(|(d, p)| *d < max && (p.y - home.y).abs() < 1.4).collect();
                    c.sort_by(|a, b| a.0.total_cmp(&b.0));
                    c.into_iter().map(|(_, p)| p).collect()
                };
                v.extend(near(&stores, 9.0).into_iter().filter_map(|p| Station::new(&nav, home, p, Kind::Store)).take(2));
                v.extend(near(&hearths, 9.0).into_iter().filter_map(|p| Station::new(&nav, home, p, Kind::Hearth)).take(1));
                v.extend(Station::pace(&nav, home, hash(i as u64, 0x9ACE)));
                if let Some((side, a)) = anvil[i]
                    && let Some(f) = forges.iter().find(|f| f.anvil.distance(a) < 0.05)
                {
                    // Заготовку греет тот из двоих, кто стоит ближе к горну: второй не ходит сквозь него.
                    let mate = anvil.iter().enumerate().any(|(j, x)| j != i && x.is_some_and(|(s2, a2)| a2.distance(a) < 0.05 && s2.distance(f.fire) < side.distance(f.fire)));
                    if !mate {
                        v.extend(Station::new(&nav, side, f.fire, Kind::Fire));
                    }
                    v.extend(Station::new(&nav, side, f.trough, Kind::Trough));
                    v.extend(near(&stores, 9.0).into_iter().filter_map(|p| Station::new(&nav, side, p, Kind::Store)).take(1));
                }
                v
            })
            .collect();
        Town { nav, residents, home_cell, bed, anvil, job, fields, spots, fishing, street, street_of, stations }
    }

    /// Чем житель занят по соседству с основным делом `act` на месте `pos` (у места `dest`).
    fn rounds_for(&self, i: usize, act: Act, pos: Vec3, dest: Option<Dest>, day: u32, k: usize) -> Option<Arc<Rounds>> {
        use Act::*;
        let role = self.residents[i].role;
        let here = |kind: Kind| self.stations[i].iter().filter(move |s| s.kind == kind && s.from.distance(pos) < 0.05);
        let go = |s: &Station, act: Act, secs: f32, there: Way, back: Way, speed: f32| Errand { pts: s.pts.clone(), cum: s.cum.clone(), yaw: s.yaw, act, secs, speed, there, back };
        let tongs = Way { carry: false, act: Stoke };
        let sweep = Way { carry: false, act: Sweep };
        let mut v: Vec<Errand> = Vec::new();
        match act {
            Hammer => {
                v.extend(here(Kind::Fire).map(|s| go(s, Stoke, 10.0, Way::EMPTY, tongs, 1.1)));
                v.extend(here(Kind::Fire).map(|s| go(s, Stoke, 7.0, Way::EMPTY, tongs, 1.1)));
                v.extend(here(Kind::Trough).map(|s| go(s, Quench, 5.0, tongs, Way::EMPTY, 1.1)));
                v.extend(here(Kind::Store).map(|s| go(s, Fetch, 4.0, Way::EMPTY, Way::LOADED, 1.1)));
            }
            Work | Weave | Write | Treat if dest.is_none() && role != Role::Student => {
                v.extend(here(Kind::Store).map(|s| go(s, Fetch, 5.0, Way::EMPTY, if act == Work { Way::LOADED } else { Way::EMPTY }, 1.1)));
                if matches!(act, Work) {
                    v.extend(here(Kind::Hearth).map(|s| go(s, Tend, 6.0, Way::EMPTY, Way::EMPTY, 1.1)));
                }
            }
            Stir if dest.is_none() => {
                v.extend(here(Kind::Hearth).map(|s| go(s, Stir, 18.0, Way::EMPTY, Way::EMPTY, 1.0)));
                v.extend(here(Kind::Hearth).map(|s| go(s, Stir, 24.0, Way::EMPTY, Way::EMPTY, 1.0)));
                v.extend(here(Kind::Hearth).map(|s| go(s, Tend, 6.0, Way::EMPTY, Way::EMPTY, 1.0)));
                v.extend(here(Kind::Store).map(|s| go(s, Fetch, 4.0, Way::EMPTY, Way::LOADED, 1.0)));
            }
            Arrange if dest.is_none() => {
                v.extend(here(Kind::Store).map(|s| go(s, Fetch, 4.0, Way::EMPTY, Way::LOADED, 1.1)));
                v.extend(here(Kind::Pace).map(|s| go(s, Arrange, 8.0, Way::EMPTY, Way::EMPTY, 1.0)));
            }
            Sweep if dest.is_none() => {
                // Метёт на ходу: медленно до края двора и обратно.
                v.extend(here(Kind::Pace).map(|s| go(s, Sweep, 5.0, sweep, sweep, 0.45)));
                v.extend(here(Kind::Store).map(|s| go(s, Fetch, 4.0, Way::EMPTY, Way::LOADED, 1.0)));
            }
            Idle if dest.is_none() && role == Role::Child => {
                // Дети не сидят на месте: бегают туда-сюда.
                v.extend(here(Kind::Pace).map(|s| go(s, Idle, 4.0, Way::EMPTY, Way::EMPTY, 2.4)));
            }
            Idle if dest.is_none() && role != Role::Patient => {
                v.extend(here(Kind::Store).map(|s| go(s, Fetch, 4.0, Way::EMPTY, Way::EMPTY, 1.0)));
                v.extend(here(Kind::Hearth).map(|s| go(s, Tend, 5.0, Way::EMPTY, Way::EMPTY, 1.0)));
                v.extend(here(Kind::Pace).map(|s| go(s, Idle, 7.0, Way::EMPTY, Way::EMPTY, 0.9)));
            }
            Guard => {
                // Часовой не стоит столбом: проходит пару шагов вдоль поста и возвращается.
                let st = Station::pace(&self.nav, pos, hash(i as u64, 0x6A7D ^ k as u64));
                v.extend(st.iter().map(|s| go(s, Guard, 8.0, Way::EMPTY, Way::EMPTY, 0.8)));
            }
            Browse if dest.is_some() => {
                // На рынке переходит от прилавка к прилавку (к соседним местам площади).
                let spots = dest.and_then(|d| self.spots.get(&d)).map(Vec::as_slice).unwrap_or(&[]);
                let mut near: Vec<&Spot> = spots.iter().filter(|s| (3.0..10.0).contains(&s.pos.distance(pos)) && (s.pos.y - pos.y).abs() < 0.4).collect();
                near.sort_by_key(|s| hash(day as u64 ^ (i as u64) << 8, (s.pos.x * 4.0) as i64 as u64));
                for sp in near.into_iter().take(2) {
                    if let Some((pts, cum)) = errand_path(&self.nav, pos, sp.pos, 11.0) {
                        v.push(Errand { pts, cum, yaw: sp.yaw, act: Browse, secs: 14.0, speed: 1.0, there: Way::EMPTY, back: Way::EMPTY });
                    }
                }
            }
            _ => {}
        }
        if v.is_empty() {
            return None;
        }
        let longest = v.iter().map(Errand::secs_total).fold(0.0, f32::max);
        let slot = (40.0 + 30.0 * unit(i as u64, 0x5107 ^ k as u64)).max(longest / 0.6);
        Some(Arc::new(Rounds { errands: v, slot, seed: hash(i as u64 ^ (day as u64) << 24, k as u64) }))
    }

    /// Путь от точки `from` к месту `d` (через поле), заканчивая в `to` (место у цели).
    fn path_to(&self, from: Vec3, d: Dest, to: Vec3) -> Option<Vec<Vec3>> {
        let f = self.fields.get(&d)?;
        let start = self.nav.nearest(from, 2.0).filter(|c| f.reached(*c))?;
        let cells = self.nav.descend(f, start)?;
        let mut pts: Vec<Vec3> = std::iter::once(from).chain(cells.iter().map(|c| self.nav.point(*c))).collect();
        // Последний участок: от центра места к своему месту — по полю этого места обратно не
        // спустишься, поэтому спрямляем с конца: ищем самую раннюю точку пути, откуда видно `to`.
        let cut = (0..pts.len()).rev().take(400).filter(|&k| self.nav.straight(pts[k], to)).last();
        if let Some(k) = cut {
            pts.truncate(k + 1);
        }
        pts.push(to);
        Some(self.nav.smooth(&pts))
    }

    /// Путь между двумя точками, где цель — место `d` или дом (тогда — обратный путь из дома).
    fn route(&self, from: Vec3, from_dest: Option<Dest>, to: Vec3, to_dest: Option<Dest>) -> Option<Vec<Vec3>> {
        if from.distance(to) < 0.05 {
            return Some(vec![from, to]);
        }
        if self.nav.straight(from, to) && from.distance(to) < 30.0 {
            return Some(vec![from, to]);
        }
        if from.distance(to) < 60.0
            && let Some(p) = self.nav.local_path(from, to, 12.0)
        {
            return Some(p);
        }
        match (from_dest, to_dest) {
            (_, Some(d)) => self.path_to(from, d, to),
            (Some(d), None) => {
                let mut p = self.path_to(to, d, from)?;
                p.reverse();
                Some(p)
            }
            (None, None) => self.nav.local_path(from, to, 30.0),
        }
    }

    /// Секунды игрового времени на путь длиной `len` метров.
    fn walk_time(len: f32, speed: f32) -> f64 {
        (len / speed) as f64 * TIME_SCALE
    }

    /// План на день для всех жителей.
    pub fn plan_day(&self, day: u32) -> DayPlans {
        let wishes: Vec<Vec<Wish>> = (0..self.residents.len()).map(|i| self.wishes(i, day)).collect();
        let places = self.allocate(day, &wishes);
        let people = (0..self.residents.len()).map(|i| self.build_plan(i, day, &wishes[i], &places[i])).collect();
        DayPlans { day, people }
    }

    /// Распределяет места: собеседники встают парами лицом к лицу, остальные — порознь.
    fn allocate(&self, day: u32, wishes: &[Vec<Wish>]) -> Vec<Vec<Option<(Spot, Option<u16>)>>> {
        let mut out: Vec<Vec<Option<(Spot, Option<u16>)>>> = wishes.iter().map(|w| vec![None; w.len()]).collect();
        // (место) -> посетители: (время прихода, ухода, житель, номер желания)
        let mut visits: HashMap<Key, Vec<(f32, f32, usize, usize)>> = HashMap::new();
        for (i, ws) in wishes.iter().enumerate() {
            for (k, w) in ws.iter().enumerate() {
                let key = match w.target {
                    Target::Dest(d) => Key::Dest(d),
                    Target::Street => match self.street_of[i] {
                        Some(s) => Key::Street(s),
                        None => continue,
                    },
                    _ => continue,
                };
                let end = ws.get(k + 1).map_or(24.0, |n| n.at);
                visits.entry(key).or_default().push((w.at, end, i, k));
            }
        }
        let mut visits: Vec<(Key, Vec<(f32, f32, usize, usize)>)> = visits.into_iter().collect();
        visits.sort_by_key(|(k, _)| *k);
        for (key, mut v) in visits {
            let near: Vec<Spot>;
            let (spots, d) = match key {
                Key::Dest(d) => match self.spots.get(&d).filter(|s| !s.is_empty()) {
                    Some(s) => (s.as_slice(), d as u64),
                    None => continue,
                },
                Key::Street(k) => {
                    // Своё место и соседние (не дальше 14 м): у одного дома тесно.
                    let c = self.street[k].pos;
                    let mut n: Vec<(f32, Spot)> = self.street.iter().map(|s| (s.pos.distance(c), *s)).filter(|(d, _)| *d < 14.0).collect();
                    n.sort_by(|a, b| a.0.total_cmp(&b.0));
                    near = n.into_iter().map(|(_, s)| s).collect();
                    (near.as_slice(), 1000 + k as u64)
                }
            };
            v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
            // Место занято до (часов): [одиночное место / первое в паре, второе в паре].
            let mut busy = vec![[0.0f32; 2]; spots.len()];
            // Ждущий собеседника: (место, житель, номер желания, до какого часа там будет).
            let mut waiting: Vec<(usize, usize, usize, f32)> = Vec::new();
            let start = if matches!(key, Key::Street(_)) { 0 } else { (hash(day as u64, d) % spots.len() as u64) as usize };
            for (at, end, i, k) in v {
                let talk = wishes[i][k].act == Act::Talk;
                if talk && let Some(w) = waiting.iter().position(|(_, j, _, until)| *j != i && *until > at + 0.3) {
                    let (s, j, kj, until) = waiting.remove(w);
                    busy[s][1] = end;
                    busy[s][0] = busy[s][0].max(until);
                    let spot = spots[s];
                    out[i][k] = Some((Spot { pos: spot.partner, yaw: yaw_to(spot.partner, spot.pos), partner: spot.pos }, Some(j as u16)));
                    if let Some((sp, _)) = out[j][kj] {
                        out[j][kj] = Some((sp, Some(i as u16)));
                    }
                    continue;
                }
                let free = (0..spots.len()).map(|n| (start + n) % spots.len()).find(|&s| busy[s][0] <= at && busy[s][1] <= at);
                let s = free.unwrap_or_else(|| (start + i) % spots.len());
                busy[s][0] = end;
                out[i][k] = Some((spots[s], None));
                if talk {
                    waiting.push((s, i, k, end));
                }
            }
        }
        out
    }

    fn build_plan(&self, i: usize, day: u32, wishes: &[Wish], places: &[Option<(Spot, Option<u16>)>]) -> Plan {
        let r = &self.residents[i];
        let mut segs = Vec::new();
        let home = r.pos;
        let Some(_) = self.home_cell[i] else {
            // Вне сетки (верхний этаж, усадьба за стенами): занятия на месте, ночью — в доме.
            for (k, w) in wishes.iter().enumerate() {
                let t0 = if k == 0 { 0.0 } else { w.at as f64 * 3600.0 };
                let t1 = wishes.get(k + 1).map_or(DAY, |n| n.at as f64 * 3600.0);
                if t1 <= t0 {
                    continue;
                }
                let act = match w.target {
                    Target::Home | Target::Bed => w.act,
                    _ => Act::Idle,
                };
                let (pos, yaw, act, lie) = match (act, self.bed[i]) {
                    (Act::Sleep | Act::Lie, Some(b)) => (home, yaw_to(Vec3::ZERO, b.along), act, Some(b.pos)),
                    (Act::Sleep, None) => (home, r.yaw, Act::Away, None),
                    _ => (home, r.yaw, if act == Act::Lie { Act::Idle } else { act }, None),
                };
                segs.push(Seg::Stay(Stay { t0, t1, pos, yaw, act, partner: None, lie, rounds: None }));
            }
            return Plan { segs };
        };
        let speed = match r.role {
            Role::Child => 1.55,
            Role::Noble | Role::Priest | Role::Patient => 1.05,
            _ => 1.25,
        };
        // Сейчас: где стоит и у какого места.
        let first_bed = matches!(wishes.first().map(|w| w.act), Some(Act::Sleep));
        let (mut at_pos, mut at_dest): (Vec3, Option<Dest>) = (if first_bed { self.bed_spot(i).0 } else { home }, None);
        // Лежит ли в постели на этом шаге (середина ложа).
        let mut lie: Option<Vec3>;
        let mut t = 0.0f64;
        for (k, w) in wishes.iter().enumerate() {
            let want_t = if k == 0 { 0.0 } else { w.at as f64 * 3600.0 };
            let next_t = wishes.get(k + 1).map_or(DAY, |n| n.at as f64 * 3600.0);
            // Цель.
            lie = None;
            let (pos, yaw, dest, partner, act) = match w.target {
                Target::Home => (home, r.yaw, None, None, w.act),
                Target::Bed => {
                    let (p, y, act, l) = self.bed_spot(i);
                    lie = l;
                    (p, y, None, None, if w.act == Act::Sleep { act } else if l.is_some() { w.act } else { Act::Idle })
                }
                Target::Street => match places.get(k).copied().flatten() {
                    Some((s, partner)) => (s.pos, s.yaw, None, partner, if w.act == Act::Talk && partner.is_none() { Act::Idle } else { w.act }),
                    None => (home, r.yaw, None, None, Act::Idle),
                },
                Target::Anvil => match self.anvil[i] {
                    Some((p, a)) => (p, yaw_to(p, a), None, None, Act::Hammer),
                    None => (home, r.yaw, None, None, Act::Work),
                },
                Target::Fishing => match self.fishing.get((hash(i as u64, 0xF15) % self.fishing.len().max(1) as u64) as usize) {
                    Some(s) => (s.pos, s.yaw, Some(Dest::Quay), None, Act::Fish),
                    None => (home, r.yaw, None, None, Act::Idle),
                },
                Target::Dest(d) => match places.get(k).copied().flatten() {
                    Some((s, partner)) => (s.pos, s.yaw, Some(d), partner, if w.act == Act::Talk && partner.is_none() { Act::Idle } else { w.act }),
                    None => (home, r.yaw, None, None, Act::Idle),
                },
            };
            let start = want_t.max(t);
            if start >= DAY - 60.0 && k + 1 < wishes.len() {
                continue;
            }
            // Дорога.
            if pos.distance(at_pos) > 0.3 {
                let path = self.route(at_pos, at_dest, pos, dest).or_else(|| self.route(at_pos, at_dest, home, None).map(|_| vec![at_pos, pos]));
                let Some(path) = path else {
                    continue;
                };
                let mut cum = vec![0.0];
                for p in path.windows(2) {
                    cum.push(cum.last().unwrap() + p[0].distance(p[1]));
                }
                let len = *cum.last().unwrap();
                let patrol = w.act == Act::Guard && dest.is_some();
                let mut t1 = start + Self::walk_time(len, if patrol { speed * 0.85 } else { speed });
                // Домой к ночи успевают любой ценой (а то в полночь план начнётся сначала).
                // Дорога домой тоже: дальше по плану только сон дома, а затянувшийся обход
                // иначе застал бы солдата в полночь посреди улицы.
                if (k + 1 == wishes.len() || pos.distance(home) < 1.0) && t1 > DAY - 120.0 {
                    t1 = (DAY - 120.0).max(start + 1.0);
                }
                segs.push(Seg::Walk(Walk { t0: start, t1, pts: path, cum, carry: w.carry, patrol }));
                t = t1;
            } else {
                t = start;
            }
            let t1 = next_t.max(t + 1.0).min(DAY);
            if t1 > t {
                let rounds = if lie.is_some() || partner.is_some() { None } else { self.rounds_for(i, act, pos, dest, day, k) };
                // Котёл — на очаге: мешать ходят туда, а на своём месте режут и месят.
                let act = if act == Act::Stir && rounds.as_ref().is_some_and(|r| r.errands.iter().any(|e| e.act == Act::Stir)) { Act::Work } else { act };
                segs.push(Seg::Stay(Stay { t0: t, t1, pos, yaw, act, partner, lie, rounds }));
                t = t1;
            }
            at_pos = pos;
            at_dest = dest;
            if t >= DAY {
                break;
            }
        }
        if let Some(Seg::Walk(_)) = segs.first() {
            // День всегда начинается на месте (на случай, если первым шагом оказалась дорога).
            segs.insert(0, Seg::Stay(Stay { t0: 0.0, t1: 0.0, pos: home, yaw: r.yaw, act: Act::Idle, partner: None, lie: None, rounds: None }));
        }
        Plan { segs }
    }

    /// Где житель спит: место у постели и сама постель (лёжа) или дом (ушёл в комнаты, не виден).
    fn bed_spot(&self, i: usize) -> (Vec3, f32, Act, Option<Vec3>) {
        let r = &self.residents[i];
        if let Some(b) = self.bed[i]
            && let Some(c) = self.nav.nearest(b.pos - Vec3::Y * 0.62, 1.4)
        {
            let side = self.nav.point(c);
            if self.nav.local_path(r.pos, side, 6.0).is_some() {
                return (side, yaw_to(Vec3::ZERO, b.along), Act::Sleep, Some(b.pos));
            }
        }
        (r.pos, r.yaw, Act::Away, None)
    }

    /// Насколько далеко (м) от дома жителя место `d` (для выбора, куда идти).
    fn dist_home(&self, i: usize, d: Dest) -> f32 {
        match (self.home_cell[i], self.fields.get(&d)) {
            (Some(c), Some(f)) if f.reached(c) => f.meters(c),
            _ => f32::INFINITY,
        }
    }

    /// Ближайшее из мест.
    fn nearest_of(&self, i: usize, ds: &[Dest]) -> Option<Dest> {
        ds.iter().copied().filter(|d| self.dist_home(i, *d).is_finite()).min_by(|a, b| self.dist_home(i, *a).total_cmp(&self.dist_home(i, *b)))
    }
}

/// Места вокруг точки `c` для людей: открытые клетки, не ближе 2,2 м друг к другу.
fn make_spots(nav: &Nav, f: &Field, c: usize, d: Dest) -> Vec<Spot> {
    let center = nav.point(c);
    let r = d.spread();
    let n = (r / CELL).ceil() as i32;
    let (ci, ck) = nav.ik(c);
    let mut cand = Vec::new();
    for dk in -n..=n {
        for di in -n..=n {
            let Some(m) = nav.idx(ci + di, ck + dk) else { continue };
            let p = nav.point(m);
            if !nav.walkable(m) || !f.reached(m) || p.distance(center) > r || f.meters(m) > r * 2.0 {
                continue;
            }
            // Просторно вокруг (не у самой стены).
            let open = (-2..=2).all(|a| (-2..=2).all(|b| nav.idx(ci + di + a, ck + dk + b).is_some_and(|q| nav.walkable(q) && (nav.floor[q] - p.y).abs() < 0.3)));
            if open {
                cand.push(m);
            }
        }
    }
    cand.sort_by_key(|m| hash(*m as u64, d as u64));
    let mut out: Vec<Spot> = Vec::new();
    for m in cand {
        let p = nav.point(m);
        if out.iter().any(|s| s.pos.distance(p) < 2.3 || s.partner.distance(p) < 1.6) {
            continue;
        }
        // Собеседник — в 1,3 м, лицом друг к другу.
        let a = unit(m as u64, 7) * std::f32::consts::TAU;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        let q = p + dir * 1.3;
        let Some(qc) = nav.nearest(q, 0.3) else { continue };
        let q = nav.point(qc);
        if out.iter().any(|s| s.pos.distance(q) < 1.6 || s.partner.distance(q) < 1.6) || !nav.straight(p, q) {
            continue;
        }
        // Молятся лицом на восток (к алтарю), остальные — друг к другу.
        let yaw = if matches!(d, Dest::Basilica | Dest::ChurchWest) { yaw_to(Vec3::ZERO, Vec3::X) } else { yaw_to(p, q) };
        out.push(Spot { pos: p, yaw, partner: q });
        if out.len() >= 48 {
            break;
        }
    }
    out
}

/// Места на улицах и во дворах: открытые клетки через каждые ~7 м, откуда дойти до агоры.
fn street_spots(nav: &Nav, agora: Option<&Field>) -> Vec<Spot> {
    let Some(f) = agora else { return Vec::new() };
    let step = (7.0 / CELL) as i32;
    let mut out = Vec::new();
    let open = |i: i32, k: i32, y: f32| (-2..=2).all(|a| (-2..=2).all(|b| nav.idx(i + a, k + b).is_some_and(|q| nav.walkable(q) && (nav.floor[q] - y).abs() < 0.25)));
    for bk in (0..nav.nz).step_by(step as usize) {
        for bi in (0..nav.nx).step_by(step as usize) {
            // В квадрате ищем открытую клетку, начиная от середины.
            let mut found = None;
            'search: for r in 0..step / 2 {
                for (di, dk) in [(0, 0), (r, 0), (-r, 0), (0, r), (0, -r), (r, r), (-r, -r), (r, -r), (-r, r)] {
                    let (i, k) = (bi + step / 2 + di, bk + step / 2 + dk);
                    let Some(c) = nav.idx(i, k) else { continue };
                    if nav.walkable(c) && f.reached(c) && open(i, k, nav.floor[c]) {
                        found = Some(c);
                        break 'search;
                    }
                }
            }
            let Some(c) = found else { continue };
            let p = nav.point(c);
            let a = unit(c as u64, 11) * std::f32::consts::TAU;
            let q = p + Vec3::new(a.cos(), 0.0, a.sin()) * 1.3;
            let Some(qc) = nav.nearest(q, 0.3) else { continue };
            let q = nav.point(qc);
            if nav.straight(p, q) {
                out.push(Spot { pos: p, yaw: yaw_to(p, q), partner: q });
            }
        }
    }
    out
}

/// Края набережной у воды: стоят лицом к морю.
fn fishing_spots(nav: &Nav, fields: &HashMap<Dest, Field>, cells: &HashMap<Dest, usize>) -> Vec<Spot> {
    let (Some(f), Some(&c)) = (fields.get(&Dest::Quay), cells.get(&Dest::Quay)) else { return Vec::new() };
    let center = nav.point(c);
    let water = crate::level::WATER_Y;
    let mut out: Vec<Spot> = Vec::new();
    let n = (70.0 / CELL) as i32;
    let (ci, ck) = nav.ik(c);
    let mut cand = Vec::new();
    for dk in -n..=n {
        for di in -n..=n {
            let Some(m) = nav.idx(ci + di, ck + dk) else { continue };
            if !nav.walkable(m) || !f.reached(m) {
                continue;
            }
            let p = nav.point(m);
            for a in 0..8 {
                let ang = a as f32 * std::f32::consts::FRAC_PI_4;
                let dir = Vec3::new(ang.cos(), 0.0, ang.sin());
                // У края бывает бортик: вода в пределах пары метров.
                let deep = [1.2f32, 1.8, 2.4].iter().any(|d| {
                    let q = p + dir * *d;
                    crate::level::ground_y(q.x, q.z) < water && nav.cell_at(q.x, q.z).is_none_or(|qc| !nav.walkable(qc))
                }) && nav.cell_at(p.x + dir.x * 0.7, p.z + dir.z * 0.7).is_none_or(|qc| !f.reached(qc) || !nav.walkable(qc));
                let q = p + dir * 1.2;
                if deep && p.y > water + 0.3 {
                    cand.push((m, yaw_to(p, q), p.distance(center)));
                    break;
                }
            }
        }
    }
    cand.sort_by_key(|(m, _, _)| hash(*m as u64, 0xF154));
    for (m, yaw, _) in cand {
        let p = nav.point(m);
        if out.iter().all(|s| s.pos.distance(p) > 3.0) {
            out.push(Spot { pos: p, yaw, partner: p });
        }
        if out.len() >= 24 {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Распорядок по ролям
// ---------------------------------------------------------------------------------------------

/// Чем житель зарабатывает, если это не следует из роли.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    Default,
    Smith,
    Porter,
    Fisher,
}

/// Ключ места при раздаче мест: большое место города или место на улице у дома.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    Dest(Dest),
    Street(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Home,
    /// На улице у дома (с соседями).
    Street,
    Bed,
    Anvil,
    Fishing,
    Dest(Dest),
}

/// Желание: с часа `at` быть там-то и делать то-то.
#[derive(Clone, Copy, Debug)]
struct Wish {
    at: f32,
    target: Target,
    act: Act,
    carry: bool,
}

fn w(at: f32, target: Target, act: Act) -> Wish {
    Wish { at, target, act, carry: false }
}

fn carry(at: f32, target: Target) -> Wish {
    Wish { at, target, act: Act::Work, carry: true }
}

/// Чем занят на работе (по роли).
fn work_act(role: Role) -> Act {
    match role {
        Role::Weaver => Act::Weave,
        Role::Cook | Role::Housewife => Act::Stir,
        Role::Shopkeeper => Act::Arrange,
        Role::Scholar => Act::Teach,
        Role::Student => Act::Write,
        Role::Physician => Act::Treat,
        Role::Patient => Act::Lie,
        Role::Servant => Act::Sweep,
        Role::Soldier | Role::Officer => Act::Guard,
        Role::Priest => Act::Preach,
        Role::Monk => Act::Pray,
        Role::Farmer => Act::Hoe,
        Role::Householder | Role::Noble | Role::Child => Act::Idle,
        _ => Act::Work,
    }
}

impl Town {
    fn wishes(&self, i: usize, day: u32) -> Vec<Wish> {
        use Act::*;
        use Target::{Bed, Dest as D, Home};
        let r = &self.residents[i];
        let seed = i as u64 * 31 + 7;
        let j = |k: u64| (unit(seed ^ (day as u64) << 20, k) - 0.5) * 0.8; // ±0.4 ч
        let pick = |k: u64| unit(seed ^ ((day as u64) << 20), k + 100);
        let wake = 5.2 + 0.6 * pick(1);
        let sleep = 21.2 + 0.8 * pick(2);
        let rain = |a: f32, b: f32| rains_during(day, a, b);
        let church = self.nearest_of(i, &[Dest::Basilica, Dest::ChurchWest]);
        let tavern = self.nearest_of(i, &[Dest::Tavern, Dest::TavernPort]);
        let market = self.nearest_of(i, &[Dest::Agora, Dest::PortMarket]);
        let near = |d: Option<Dest>, max: f32| d.filter(|d| self.dist_home(i, *d) < max);
        let street = self.street_of[i].is_some();
        // Поболтать с соседями у дома (если есть где), иначе — дома.
        let doorstep = |at: f32| if street { w(at, Target::Street, Talk) } else { w(at, Home, Idle) };
        // Вечерний досуг (17–19.5): куда пойти.
        let leisure = |k: u64, from: f32, men: bool| -> Wish {
            let p = pick(k);
            let wet = rain(from, from + 2.5);
            if wet {
                return match near(tavern, 350.0) {
                    Some(t) if men && p < 0.5 => w(from, D(t), Drink),
                    _ => w(from, Home, Idle),
                };
            }
            // Ближе к дому: соседи у дверей (половина вечеров).
            if street && p > 0.55 {
                return doorstep(from);
            }
            if p < 0.35 {
                if let Some(m) = near(market, 350.0) {
                    return w(from, D(m), Talk);
                }
            } else if p < 0.6 && men {
                if let Some(t) = near(tavern, 350.0) {
                    return w(from, D(t), Drink);
                }
            } else if p < 0.75 && men {
                if self.dist_home(i, Dest::Thermae) < 300.0 {
                    return w(from, D(Dest::Thermae), Bathe);
                }
            } else if p < 0.92
                && let Some(c) = near(church, 300.0)
            {
                return w(from, D(c), Pray);
            }
            doorstep(from)
        };
        let devout_morning = |v: &mut Vec<Wish>| {
            if pick(3) < 0.22
                && let Some(c) = near(church, 300.0)
            {
                v.push(w(wake + 0.4, D(c), Pray));
            }
        };
        let men = !matches!(r.role, Role::Housewife | Role::Weaver | Role::Cook | Role::Child);
        let role = match self.job[i] {
            Job::Smith => Role::Craftsman,
            Job::Porter | Job::Fisher => Role::Sailor,
            Job::Default => r.role,
        };
        let work = work_act(role);
        let mut v = vec![w(0.0, Bed, Sleep)];
        match role {
            Role::Craftsman | Role::Weaver | Role::Cook | Role::Shopkeeper | Role::Bather | Role::Scholar | Role::Student => {
                v.push(w(wake, Home, Idle));
                devout_morning(&mut v);
                let at_work = if self.anvil[i].is_some() { Target::Anvil } else { Home };
                v.push(w(7.2 + j(4), at_work, work));
                v.push(w(12.0 + j(5), Home, Eat));
                if r.role == Role::Scholar && self.dist_home(i, Dest::Library) < 300.0 {
                    v.push(w(13.5, D(Dest::Library), Write));
                } else {
                    v.push(w(13.3 + j(6), at_work, work));
                }
                v.push(leisure(7, 17.0 + j(8), men));
                v.push(w(19.3 + j(9), Home, Eat));
            }
            Role::Physician => {
                v.push(w(wake, Home, Idle));
                v.push(w(7.0 + j(4), Home, Treat));
                if self.dist_home(i, Dest::ApothecaryAgora) < 300.0 {
                    v.push(w(10.5 + j(5), D(Dest::ApothecaryAgora), Browse));
                }
                v.push(w(12.0, Home, Eat));
                v.push(w(13.0 + j(6), Home, Treat));
                v.push(leisure(7, 17.5 + j(8), true));
                v.push(w(19.5, Home, Eat));
            }
            Role::Patient => {
                v.push(w(7.0, Bed, Lie));
            }
            Role::Householder | Role::Noble => {
                v.push(w(wake + 0.5, Home, Idle));
                devout_morning(&mut v);
                match near(market, 400.0) {
                    Some(m) if !rain(8.0, 10.5) => v.push(w(8.0 + j(4), D(m), if pick(10) < 0.6 { Talk } else { Browse })),
                    _ if !rain(8.0, 10.5) => v.push(doorstep(8.0 + j(4))),
                    _ => {}
                }
                v.push(w(10.5 + j(5), Home, Idle));
                v.push(w(12.3, Home, Eat));
                // Римская привычка: термы после полудня.
                if self.dist_home(i, Dest::Thermae) < 300.0 && pick(11) < 0.6 {
                    v.push(w(14.5 + j(6), D(Dest::Thermae), Bathe));
                } else if let Some(m) = near(market, 300.0).filter(|_| !rain(14.0, 16.5)) {
                    v.push(w(14.5 + j(6), D(m), Talk));
                }
                v.push(leisure(7, 16.8 + j(8), true));
                v.push(w(19.5 + j(9), Home, Eat));
            }
            Role::Housewife => {
                v.push(w(wake, Home, Stir));
                match near(market, 350.0) {
                    Some(m) if !rain(7.5, 9.5) => {
                        v.push(w(7.5 + j(4), D(m), Browse));
                        v.push(carry(9.3 + j(5), Home));
                    }
                    _ if !rain(7.5, 9.5) => v.push(doorstep(7.8 + j(4))),
                    _ => {}
                }
                v.push(w(9.6 + j(5), Home, Sweep));
                if self.dist_home(i, Dest::Reservoir) < 350.0 && !rain(10.5, 11.5) {
                    v.push(w(10.5 + j(6), D(Dest::Reservoir), Work));
                    v.push(carry(11.0 + j(6), Home));
                }
                v.push(w(11.6, Home, Stir));
                v.push(w(13.5, Home, Weave));
                v.push(leisure(7, 17.0 + j(8), false));
                v.push(w(18.6 + j(9), Home, Stir));
            }
            Role::Servant => {
                v.push(w(wake - 0.3, Home, Sweep));
                if let Some(m) = near(market, 300.0) {
                    v.push(w(8.0 + j(4), D(m), Browse));
                    v.push(carry(9.0 + j(4), Home));
                }
                v.push(w(10.0, Home, Sweep));
                if self.dist_home(i, Dest::Horrea) < 350.0 {
                    v.push(w(13.0 + j(5), D(Dest::Horrea), Work));
                    v.push(carry(13.6 + j(5), Home));
                }
                v.push(w(15.0, Home, Work));
                v.push(w(19.0, Home, Eat));
            }
            Role::Child => {
                v[0].at = 0.0;
                v.push(w(6.8 + j(1), Home, Idle));
                // Дети играют на улице с соседскими, постарше — бегают на агору.
                match near(market, 260.0) {
                    Some(m) if !rain(9.0, 11.5) && pick(13) < 0.4 => v.push(w(9.0 + j(4), D(m), Talk)),
                    _ if !rain(9.0, 11.5) => v.push(doorstep(9.0 + j(4))),
                    _ => {}
                }
                v.push(w(11.8, Home, Eat));
                v.push(w(13.0, Home, Idle));
                if !rain(14.5, 18.5) {
                    v.push(doorstep(14.5 + j(5)));
                    v.push(w(18.3, Home, Idle));
                }
                v.push(w(19.0, Home, Eat));
            }
            Role::Priest => {
                v.push(w(4.6, Home, Preach)); // утреня
                v.push(w(7.0, Home, Pray));
                if let Some(m) = near(market, 350.0).filter(|_| pick(12) < 0.4 && !rain(9.5, 11.0)) {
                    v.push(w(9.5 + j(4), D(m), Talk));
                }
                v.push(w(11.0, Home, Pray));
                v.push(w(12.5, Home, Eat));
                v.push(w(14.0, Home, Pray));
                v.push(w(17.0, Home, Preach)); // вечерня
                v.push(w(19.0, Home, Eat));
            }
            Role::Monk => {
                v.push(w(4.4, Home, Pray));
                v.push(w(7.0, Home, Sweep));
                v.push(w(9.0, Home, Pray));
                v.push(w(10.0, Home, Work));
                v.push(w(12.0, Home, Eat));
                v.push(w(13.5, Home, Work));
                v.push(w(17.0, Home, Pray));
                v.push(w(19.0, Home, Eat));
            }
            Role::Soldier => return self.soldier_day(i, day),
            Role::Officer => {
                v.push(w(wake, Home, Guard));
                for (h, d) in [(9.0, Dest::MainGate), (15.0, Dest::PortGate)] {
                    if self.dist_home(i, d) < 400.0 {
                        v.push(w(h + j(4), D(d), Guard));
                        v.push(w(h + 1.2, Home, Guard));
                    }
                }
                v.push(w(12.0, Home, Eat));
                v.sort_by(|a, b| a.at.total_cmp(&b.at));
                v.push(leisure(7, 18.0, true));
                v.push(w(19.8, Home, Eat));
            }
            Role::Sailor => {
                // Половина — носильщики (амфоры с набережной в хорреи), половина — рыбаки.
                let porter = self.job[i] == Job::Porter || (self.job[i] == Job::Default && hash(i as u64, 0x5A11) % 2 == 0);
                if porter && self.dist_home(i, Dest::Horrea) < 400.0 && self.dist_home(i, Dest::Quay) < 400.0 {
                    v.push(w(wake, Home, Idle));
                    let mut h = 6.5 + j(4).abs();
                    while h < 17.5 {
                        if (12.0..13.2).contains(&h) {
                            v.push(w(h, Home, Eat));
                            h = 13.2;
                            continue;
                        }
                        v.push(w(h, D(Dest::Quay), Work));
                        v.push(carry(h + 0.25, D(Dest::Horrea)));
                        h += 1.4;
                    }
                    v.push(w(h, Home, Idle));
                } else {
                    v.push(w(wake - 0.6, Target::Fishing, Fish));
                    if !rain(10.8, 13.0) {
                        v.push(w(10.8 + j(4), D(Dest::PortMarket), Browse));
                    }
                    v.push(w(13.0, Home, Work));
                    if !rain(16.0, 19.0) {
                        v.push(w(16.0 + j(5), Target::Fishing, Fish));
                    }
                }
                if let Some(t) = near(tavern, 300.0) {
                    v.push(w(19.0 + j(8), D(t), Drink));
                }
                v.push(w(21.0 + j(9), Home, Idle));
            }
            Role::Farmer => {
                v.push(w(wake, Home, Idle));
                v.push(w(6.0, Home, if rain(6.0, 19.0) { Idle } else { Hoe }));
                v.push(w(12.0, Home, Eat));
                v.push(w(13.0, Home, if rain(13.0, 19.0) { Idle } else { Hoe }));
                v.push(w(19.0, Home, Eat));
            }
        }
        // Сон — последнее за день: поздний «домой» (21:00 + разброс) не должен поднять с постели.
        v.retain(|x| x.at < sleep - 0.05);
        v.push(w(sleep, Bed, Sleep));
        v.sort_by(|a, b| a.at.total_cmp(&b.at));
        // Желания с одинаковым временем — оставляем последнее.
        v.dedup_by(|b, a| (b.at - a.at).abs() < 0.05 && {
            *a = *b;
            true
        });
        v
    }

    /// Солдат: смены по восемь часов, свободные часы — в казарме, иногда в таверне.
    fn soldier_day(&self, i: usize, day: u32) -> Vec<Wish> {
        use Act::*;
        use Target::{Bed, Dest as D, Home};
        let shift = (i + day as usize) % 3;
        let route = self.patrol_route(i);
        let mut v = Vec::new();
        let patrol = |v: &mut Vec<Wish>, from: f32, to: f32| {
            let mut h = from;
            let mut k = 0;
            while h < to - 0.3 && !route.is_empty() {
                v.push(w(h, D(route[k % route.len()]), Guard));
                h += 0.75;
                k += 1;
            }
            v.push(w(to.max(h), Home, Guard));
        };
        match shift {
            // Утренняя смена: 6–14.
            0 => {
                v.push(w(0.0, Bed, Sleep));
                v.push(w(5.4, Home, Eat));
                patrol(&mut v, 6.0, 14.0);
                v.push(w(14.3, Home, Eat));
                v.push(w(15.0, Home, Idle));
                if let Some(t) = self.nearest_of(i, &[Dest::Tavern, Dest::TavernPort]).filter(|t| self.dist_home(i, *t) < 300.0) {
                    v.push(w(18.0, D(t), Drink));
                }
                v.push(w(20.0, Home, Idle));
                v.push(w(21.3, Bed, Sleep));
            }
            // Дневная: 14–22.
            1 => {
                v.push(w(0.0, Bed, Sleep));
                v.push(w(7.0, Home, Idle));
                v.push(w(8.0, Home, Guard));
                v.push(w(12.5, Home, Eat));
                patrol(&mut v, 14.0, 21.5);
                v.push(w(22.0, Bed, Sleep));
            }
            // Ночная: 0–6 обход с факелом, днём сон, вечером — на часах в казарме.
            _ => {
                v.push(w(0.0, Home, Guard));
                patrol(&mut v, 0.2, 5.8);
                v.push(w(6.2, Home, Eat));
                v.push(w(7.0, Bed, Sleep));
                v.push(w(14.5, Home, Eat));
                v.push(w(15.3, Home, Idle));
                v.push(w(18.0, Home, Guard));
            }
        }
        v
    }

    /// Обход: ворота, агора, порт — каждому солдату со своего места.
    fn patrol_route(&self, i: usize) -> Vec<Dest> {
        let all = [Dest::MainGate, Dest::Agora, Dest::Basilica, Dest::PortGate, Dest::Quay, Dest::Horrea, Dest::SouthGate, Dest::Thermae];
        let mut r: Vec<Dest> = all.into_iter().filter(|d| self.dist_home(i, *d) < 500.0).collect();
        if r.is_empty() {
            return r;
        }
        let k = (hash(i as u64, 0x9A7) % r.len() as u64) as usize;
        r.rotate_left(k);
        r
    }
}

// ---------------------------------------------------------------------------------------------
// Испуг: драка рядом
// ---------------------------------------------------------------------------------------------

/// Рядом ударили человека: мирные вокруг приседают и закрывают голову (событие сети).
#[derive(Message, Clone, Copy, Debug)]
pub struct Alarm(pub Vec3);

/// Испуг жителей: пока прячутся, их день «стоит», и потом они отстают ровно на это время.
#[derive(Resource, Default)]
pub struct Fright {
    /// житель -> (с какого игрового времени, до какого)
    pub scared: HashMap<usize, (f64, f64)>,
    /// житель -> на сколько игровых секунд отстаёт от плана
    pub lag: HashMap<usize, f64>,
    pub day: u32,
}

/// Сколько прячутся после удара рядом (настоящие секунды).
const FRIGHT_SECS: f64 = 14.0;
const FRIGHT_RANGE: f32 = 16.0;

impl Fright {
    /// Удар в точке `at` в момент `now` (игровые секунды от полуночи).
    pub fn alarm(&mut self, plans: &DayPlans, at: Vec3, now: f64) {
        for (i, p) in plans.people.iter().enumerate() {
            let Some(s) = self.state(p, i, now) else { continue };
            if s.pos.distance(at) > FRIGHT_RANGE || matches!(s.act, Act::Sleep | Act::Away | Act::Lie) {
                continue;
            }
            let until = now + FRIGHT_SECS * TIME_SCALE;
            let e = self.scared.entry(i).or_insert((now, until));
            e.1 = until;
        }
    }

    /// Состояние жителя `i` с учётом испуга и отставания.
    pub fn state(&self, plan: &Plan, i: usize, now: f64) -> Option<NpcState> {
        // Обычно никто не напуган: не ищем каждого из сотен жителей в пустых таблицах.
        if self.scared.is_empty() && self.lag.is_empty() {
            return plan.state(now.max(0.0));
        }
        let lag = self.lag.get(&i).copied().unwrap_or(0.0);
        if let Some(&(from, until)) = self.scared.get(&i)
            && now >= from
            && now < until
        {
            let mut s = plan.state((from - lag).max(0.0))?;
            s.act = Act::Cower;
            s.walking = false;
            return Some(s);
        }
        plan.state((now - lag).max(0.0))
    }

    /// Испуг прошёл: время, что прятались, добавляется к отставанию. В полночь всё забывается.
    pub fn settle(&mut self, day: u32, now: f64) {
        if day != self.day {
            self.day = day;
            self.scared.clear();
            self.lag.clear();
            return;
        }
        let done: Vec<(usize, f64)> = self.scared.iter().filter(|(_, (_, until))| now >= *until).map(|(i, (from, until))| (*i, until - from)).collect();
        for (i, d) in done {
            self.scared.remove(&i);
            *self.lag.entry(i).or_default() += d;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Сервер: жители двигаются для боя, свидетелей и столкновений
// ---------------------------------------------------------------------------------------------

pub struct LifeServerPlugin;

impl Plugin for LifeServerPlugin {
    fn build(&self, app: &mut App) {
        warm_up();
        app.init_resource::<Fright>().add_systems(Update, move_residents_server.before(crate::civil_server::civil_tick));
    }
}

fn move_residents_server(clock: Res<crate::world_clock::WorldClock>, mut civil: ResMut<crate::civil_server::CivilWorld>, level: Res<LevelColliders>, mut fright: ResMut<Fright>) {
    let day = clock.day();
    let now = clock.of_day();
    if clock.hour() > 18.0 {
        prefetch(day + 1);
    }
    let Some(plans) = plans(day) else { return };
    for at in std::mem::take(&mut civil.alarms) {
        fright.alarm(&plans, at, now);
    }
    fright.settle(day, now);
    for (i, plan) in plans.people.iter().enumerate() {
        let Some(p) = civil.people.get_mut(i) else { break };
        // Схватился за оружие и бегает бойцом (`civil_server::rally`): им правит сервер боя.
        if p.role.is_none() || !p.alive() || p.rallied.is_some() {
            continue;
        }
        let Some(s) = fright.state(plan, i, now) else { continue };
        p.act = s.act;
        let anchor = town().map_or(p.pos, |t| t.residents[i].pos);
        p.pos = s.pos;
        p.yaw = s.yaw;
        // Тело-коробка стоит в доме: пока житель не там (или спит), она не мешает.
        // (Ставим каждый тик: так и вернувшийся на место погибшего сразу получает верное тело.)
        level.set_body_gone(i, s.pos.distance(anchor) > 0.4 || matches!(s.act, Act::Sleep | Act::Away));
    }
}

// ---------------------------------------------------------------------------------------------
// Проверки
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
pub fn test_town() -> &'static Town {
    static T: OnceLock<Town> = OnceLock::new();
    T.get_or_init(|| Town::build(crate::level::layout()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn town_stats_and_plans() {
        let t0 = std::time::Instant::now();
        let town = test_town();
        let walk = town.nav.floor.iter().filter(|f| !f.is_nan()).count();
        eprintln!("сетка {}x{} ({} проходимых) за {:.1} с", town.nav.nx, town.nav.nz, walk, t0.elapsed().as_secs_f32());
        let on = town.home_cell.iter().filter(|c| c.is_some()).count();
        let beds = town.bed.iter().filter(|b| b.is_some()).count();
        eprintln!("жителей {}, на сетке {}, с постелью {}, рыбацких мест {}", town.residents.len(), on, beds, town.fishing.len());
        let mut roles: HashMap<String, (usize, usize)> = HashMap::new();
        for (i, r) in town.residents.iter().enumerate() {
            let e = roles.entry(format!("{:?}", r.role)).or_default();
            e.0 += 1;
            e.1 += town.home_cell[i].is_some() as usize;
        }
        eprintln!("{roles:?}");
        for (d, s) in &town.spots {
            eprintln!("{d:?}: мест {}", s.len());
        }
        let t1 = std::time::Instant::now();
        let plans = town.plan_day(3);
        eprintln!("план дня за {:.2} с", t1.elapsed().as_secs_f32());
        // Больше половины жителей за день куда-то ходят.
        let walkers = plans.people.iter().filter(|p| p.segs.iter().any(|s| matches!(s, Seg::Walk(_)))).count();
        eprintln!("ходят {walkers}");
        assert!(walkers * 3 >= on, "ходят {walkers} из {on}");
        let talkers = plans.people.iter().filter(|p| p.segs.iter().any(|s| matches!(s, Seg::Stay(x) if x.partner.is_some()))).count();
        eprintln!("беседуют {talkers}");
        assert!(talkers >= 6);
    }

    #[test]
    fn plans_are_continuous_and_end_in_bed() {
        let town = test_town();
        let plans = town.plan_day(5);
        for (i, p) in plans.people.iter().enumerate() {
            let mut prev: Option<Vec3> = None;
            let mut t = 0.0;
            for s in &p.segs {
                let (t0, t1, a, b) = match s {
                    Seg::Stay(s) => (s.t0, s.t1, s.pos, s.pos),
                    Seg::Walk(w) => (w.t0, w.t1, w.pts[0], *w.pts.last().unwrap()),
                };
                assert!(t0 + 1e-6 >= t, "житель {i}: время идёт назад");
                assert!(t1 >= t0);
                if let Some(pp) = prev {
                    assert!(pp.distance(a) < 0.6, "житель {i} ({:?}) телепортируется: {pp} -> {a}", town.residents[i].role);
                }
                prev = Some(b);
                t = t1;
            }
            // К полуночи — там же, где начинал день.
            let start = p.state(0.0).unwrap().pos;
            let end = p.state(DAY - 1.0).unwrap().pos;
            assert!(start.distance(end) < 0.6, "житель {i} ({:?}) к ночи не вернулся: {start} / {end}", town.residents[i].role);
        }
    }

    #[test]
    fn same_plan_everywhere() {
        // Сервер и клиент считают план независимо — он обязан совпадать до бита.
        let town = test_town();
        let a = town.plan_day(9);
        let b = town.plan_day(9);
        for t in [3.0 * 3600.0, 8.4 * 3600.0, 13.1 * 3600.0, 18.2 * 3600.0] {
            for (pa, pb) in a.people.iter().zip(&b.people) {
                assert_eq!(pa.state(t), pb.state(t));
            }
        }
    }

    #[test]
    fn walkers_stay_on_walkable_ground() {
        let town = test_town();
        let plans = town.plan_day(2);
        let mut bad = Vec::new();
        for (i, p) in plans.people.iter().enumerate() {
            for s in &p.segs {
                let Seg::Walk(w) = s else { continue };
                let total = *w.cum.last().unwrap();
                let mut d = 0.0;
                while d < total {
                    let t = w.t0 + (w.t1 - w.t0) * (d / total) as f64;
                    let st = p.state(t).unwrap();
                    if town.nav.cell_at(st.pos.x, st.pos.z).is_none_or(|c| !town.nav.walkable(c)) && town.nav.nearest(st.pos, 0.6).is_none() {
                        bad.push((i, st.pos));
                    }
                    d += 0.5;
                }
            }
        }
        assert!(bad.len() < 20, "сквозь стены: {:?}", &bad[..bad.len().min(10)]);
    }

    #[test]
    fn people_step_away_on_errands_and_come_back() {
        // Рядом с игроком люди не стоят столбом: отходят к горну, в кладовую, к очагу,
        // метут двор на ходу — и возвращаются на рабочее место по проходимой земле.
        let town = test_town();
        let plans = town.plan_day(4);
        let with_rounds = plans.people.iter().filter(|p| p.segs.iter().any(|s| matches!(s, Seg::Stay(x) if x.rounds.is_some()))).count();
        let mut kinds: HashMap<Act, usize> = HashMap::new();
        let mut bad = Vec::new();
        for (i, p) in plans.people.iter().enumerate() {
            for s in &p.segs {
                let Seg::Stay(st) = s else { continue };
                let Some(r) = &st.rounds else { continue };
                for e in &r.errands {
                    *kinds.entry(e.act).or_default() += 1;
                    assert!(e.pts[0].distance(st.pos) < 0.05, "житель {i}: отлучка начинается не с рабочего места");
                    assert!(e.secs_total() <= r.slot * 0.7 + 1e-3, "житель {i}: отлучка {:?} не помещается в отрезок", e.act);
                    let mut d = 0.0;
                    while d < e.len() {
                        let (q, _, _) = along(&e.pts, &e.cum, d);
                        if town.nav.cell_at(q.x, q.z).is_none_or(|c| !town.nav.walkable(c)) && town.nav.nearest(q, 0.5).is_none() {
                            bad.push((i, q));
                        }
                        d += 0.4;
                    }
                }
                // К концу пребывания — на месте (дальше по плану идёт оттуда).
                let end = p.state(st.t1 - 1.0).unwrap();
                if st.t1 - st.t0 > 2.0 && end.pos.distance(st.pos) > 0.05 {
                    bad.push((i, end.pos));
                }
            }
        }
        eprintln!("с отлучками {with_rounds} из {}; дела: {kinds:?}", plans.people.len());
        assert!(with_rounds * 4 >= town.home_cell.iter().filter(|c| c.is_some()).count(), "отлучаются только {with_rounds}");
        assert!(kinds.contains_key(&Act::Stoke) && kinds.contains_key(&Act::Fetch), "{kinds:?}");
        assert!(bad.len() < 10, "сквозь стены или не вернулись: {:?}", &bad[..bad.len().min(10)]);
        // За час работы кузнец успевает и к горну, и обратно к наковальне.
        let smith = plans.people.iter().position(|p| p.state(10.0 * 3600.0).is_some_and(|s| s.act == Act::Hammer)).expect("кузнец за работой");
        let acts: std::collections::HashSet<Act> = (0..180).filter_map(|k| plans.people[smith].state(9.5 * 3600.0 + k as f64 * 20.0 * 1.0).map(|s| s.act)).collect();
        assert!(acts.contains(&Act::Hammer) && acts.contains(&Act::Stoke), "кузнец: {acts:?}");
    }

    #[test]
    fn smith_hammers_and_priest_serves() {
        let town = test_town();
        let plans = town.plan_day(4);
        let at = |act: Act, h: f32| plans.people.iter().filter(|p| p.state(h as f64 * 3600.0).is_some_and(|s| s.act == act)).count();
        eprintln!("в 10:00 куют {}, служат в 5:00 {}, спят в 2:00 {}, удят в 7:00 {}", at(Act::Hammer, 10.0), at(Act::Preach, 5.0), at(Act::Sleep, 2.0), at(Act::Fish, 7.0));
        assert!(at(Act::Sleep, 2.0) > town.residents.len() / 3);
        assert!(at(Act::Preach, 5.0) >= 1);
    }
}


#[cfg(test)]
mod shot_tests {
    use super::*;

    /// Подбирает виды для снимков: житель за делом и камера в 3 м перед ним.
    #[test]
    #[ignore]
    fn print_tour() {
        let town = test_town();
        let h: f32 = std::env::var("TOUR_HOUR").ok().and_then(|s| s.parse().ok()).unwrap_or(10.0);
        let day = (h / 24.0) as u32;
        let plans = town.plan_day(day);
        let t = (h % 24.0) as f64 * 3600.0;
        let mut out = Vec::new();
        let mut used = Vec::new();
        for act in [Act::Hammer, Act::Talk, Act::Carry, Act::Fish, Act::Sleep, Act::Preach, Act::Pray, Act::Stir, Act::Sweep, Act::Browse, Act::Drink, Act::Guard, Act::Weave, Act::Treat, Act::Lie, Act::Idle] {
            let mut n = 0;
            for (i, p) in plans.people.iter().enumerate() {
                let Some(s) = p.state(t) else { continue };
                let ok = if act == Act::Carry { s.carry } else { s.act == act && !s.walking };
                if !ok || used.iter().any(|u: &Vec3| u.distance(s.pos) < 15.0) {
                    continue;
                }
                let base = s.lie.unwrap_or(s.pos);
                let fwd = forward(s.yaw);
                let side = Vec3::new(fwd.z, 0.0, -fwd.x);
                let cands = [fwd * 3.2 + side * 1.2, fwd * 2.5 - side * 1.5, side * 3.0, -side * 3.0, fwd * 4.5];
                let Some(off) = cands.iter().find(|o| town.nav.nearest(s.pos + **o, 0.3).is_some() && town.nav.straight(s.pos, s.pos + **o)) else { continue };
                let cam = s.pos + *off + Vec3::Y * 1.75;
                used.push(s.pos);
                out.push(format!("{:?}{}_{}:{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}", act, n, i, cam.x, cam.y, cam.z, base.x, base.y + 1.1, base.z));
                n += 1;
                if n >= 2 {
                    break;
                }
            }
        }
        println!("TOUR={}", out.join(";"));
    }
}


#[cfg(test)]
mod errand_shots {
    use super::*;

    /// Виды для снимков отлучек: `cargo test errand_tour -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn errand_tour() {
        let town = test_town();
        let plans = town.plan_day(0);
        for want in [Act::Stoke, Act::Sweep, Act::Fetch, Act::Tend, Act::Browse, Act::Guard] {
            let mut n = 0;
            for (i, p) in plans.people.iter().enumerate() {
                if n >= 2 {
                    break;
                }
                for s in &p.segs {
                    let Seg::Stay(st) = s else { continue };
                    let Some(r) = &st.rounds else { continue };
                    if !(6.0 * 3600.0..16.0 * 3600.0).contains(&st.t0) || !r.errands.iter().any(|e| e.act == want) {
                        continue;
                    }
                    // Первая минута игрового времени, когда житель на месте отлучки с этим делом.
                    let t = (0..4000).map(|k| st.t0 + k as f64 * 20.0).find(|t| p.state(*t).is_some_and(|x| x.act == want && !x.walking));
                    let Some(t) = t else { continue };
                    let e = r.errands.iter().find(|e| e.act == want).unwrap();
                    let (a, b) = (st.pos, *e.pts.last().unwrap());
                    let mid = (a + b) * 0.5;
                    let fwd = (b - a).normalize_or(Vec3::X);
                    let side = Vec3::new(fwd.z, 0.0, -fwd.x);
                    let cam = [side * 4.5, -side * 4.5, side * 3.0 - fwd * 2.0, -side * 3.0 + fwd * 2.0].into_iter().map(|o| mid + o).find(|c| town.nav.nearest(*c, 0.3).is_some() && town.nav.straight(mid, *c));
                    let Some(cam) = cam else { continue };
                    println!("{want:?} житель {i} {:?} база {a} место {b} время {:.3} ч  TOUR {:.1},{:.1},{:.1},{:.1},{:.1},{:.1}", town.residents[i].role, t / 3600.0, cam.x, cam.y + 1.7, cam.z, mid.x, mid.y + 0.9, mid.z);
                    n += 1;
                    break;
                }
            }
        }
    }
}
