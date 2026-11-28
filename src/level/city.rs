//! Херсонес (греч. Херсон) около 510 г. — форпост Восточной Римской империи в Таврике.
//!
//! Что взято из археологии (масштаб 1:1 по размерам кварталов, улиц, стен; кварталов меньше,
//! чем в настоящем городе, — около 2/3 его площади):
//! - античная регулярная («гипподамова») сетка: главная продольная улица ~6 м, поперечные ~3,5 м,
//!   продольные ~4 м; кварталы ~23 × 47 м, делятся на две усадьбы с дворами;
//! - стены из крупных известняковых блоков, 8–10 м высотой, до 4 м толщиной, башни 10–12 м;
//!   при Зеноне (488 г.) стены подновлены на таможенные сборы — круглая башня Зенона (Ø 19 м)
//!   в юго-восточном углу у Карантинной бухты; цитадель (~90 × 55 м) с казармами и претория;
//! - протейхизма — внешняя низкая стена перед южной (сухопутной) стороной с периболом между стенами;
//! - порт в Карантинной бухте, таможня; рыбозасолочные цистерны (цемянка), стекольные мастерские
//!   на северном берегу, гончарные печи у цитадели, винодельни-тарапаны;
//! - центральная площадь с базиликой епископа и ксеноном (больницей), заброшенный театр
//!   (зрелища запрещены, камень разбирают), термы, резервуар водопровода (28 × 13 м);
//! - некрополь за южными воротами, к бухте; за ним — плато Гераклейского полуострова.
//!
//! Оси: X — вдоль главной улицы (в натуре она идёт с юго-запада на северо-восток),
//! Z — поперёк: −Z к морю (северный берег), +Z к суше (южная стена). Бухта — на +X.

pub(crate) mod crafts;
pub(crate) mod estate;
pub(crate) mod houses;
pub(crate) mod cozy;
pub(crate) mod interior;
pub(crate) mod kit;
pub(crate) mod outside;
pub(crate) mod plan;
pub(crate) mod port;
pub(crate) mod public;
pub(crate) mod ships;
pub(crate) mod walls;

use super::terrain::{self, WATER_Y, WORLD_HALF};
use super::{Builder, Layout, Mat};
use bevy::prelude::*;
use kit::Rect;

/// Ширина квартала вдоль главной улицы.
pub const BX: f32 = 23.0;
/// Глубина квартала поперёк.
pub const BZ: f32 = 47.0;
pub const CROSS_W: f32 = 3.5;
pub const LONG_W: f32 = 4.0;
/// Полуширина главной улицы.
pub const MAIN_HALF: f32 = 3.0;
pub const X0: f32 = -265.0;
pub const NCOL: i32 = 20;
/// Ряды кварталов: −3..−1 к морю, 1..3 к суше.
pub const ROWS: [i32; 6] = [-3, -2, -1, 1, 2, 3];

/// Квартал (i — номер вдоль главной улицы с запада, j — ряд).
pub fn block(i: i32, j: i32) -> Rect {
    let x0 = X0 + i as f32 * (BX + CROSS_W);
    let (z0, z1) = if j > 0 {
        let z0 = MAIN_HALF + (j - 1) as f32 * (BZ + LONG_W);
        (z0, z0 + BZ)
    } else {
        let z1 = -MAIN_HALF - (-j - 1) as f32 * (BZ + LONG_W);
        (z1 - BZ, z1)
    };
    Rect::new(x0, z0, x0 + BX, z1)
}

/// Ось поперечной улицы к западу от квартала i.
pub fn cross_x(i: i32) -> f32 {
    X0 + i as f32 * (BX + CROSS_W) - CROSS_W / 2.0
}

/// Цитадель в северо-западном углу, на скале над морем (внутренние грани стен).
pub const CITADEL: Rect = Rect::new(-298.25, -158.0, -222.0, -114.0);
/// Главные ворота (на южной стене) и южные ворота к некрополю.
pub fn main_gate_x() -> f32 {
    cross_x(6)
}
pub fn south_gate_x() -> f32 {
    cross_x(16)
}
/// Южная стена (ось кладки).
pub const SOUTH_WALL_Z: f32 = 166.0;
/// Восточная стена над бухтой (ось кладки).
pub const EAST_WALL_X: f32 = 285.0;
pub const WALL_T: f32 = 3.5;
pub const WALL_H: f32 = 9.0;

/// Что стоит в квартале.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Houses,
    Agora,
    Basilica,
    Episcopium,
    FishSalting,
    Glassworks,
    Didaskaleion,
    ExilesHouse,
    Thermae,
    Theatre,
    PortMarket,
    Customs,
    Horrea,
    Citadel,
    Pottery,
    Smithy,
    Winery,
    Reservoir,
    WestChurch,
    /// Пустырь, огороды (кварталы у стен, не влезающие целиком).
    Garden,
}

pub fn block_use(i: i32, j: i32) -> Use {
    use Use::*;
    match (i, j) {
        (16 | 17, -1) => Agora,
        (18 | 19, -1) => Basilica,
        (18 | 19, -2) => Episcopium,
        (18 | 19, -3) | (8, -3) => FishSalting,
        (12, -3) => Glassworks,
        (15, -1) => Didaskaleion,
        (13, -2) => ExilesHouse,
        (12 | 13, 1) => Thermae,
        (14 | 15, 2 | 3) => Theatre,
        (18, 1) => PortMarket,
        (19, 1) => Customs,
        (17..=19, 2) => Horrea,
        (0 | 1, -3) => Citadel,
        (16, 3) => Pottery,
        (5, 2) => Smithy,
        (9, 2) => Winery,
        (8, 3) => Reservoir,
        (1, -1) => WestChurch,
        _ => {
            if block_fits(i, j) {
                Houses
            } else {
                Garden
            }
        }
    }
}

/// Квартал целиком внутри стен с запасом на улицу вдоль стены.
pub fn block_fits(i: i32, j: i32) -> bool {
    let r = block(i, j);
    [(r.x0, r.z0), (r.x1, r.z0), (r.x0, r.z1), (r.x1, r.z1)].iter().all(|&(x, z)| terrain::inside_city(x, z) && edge_distance(x, z) > 7.0)
}

/// Расстояние до ближайшего участка городской стены.
pub fn edge_distance(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let ring = &terrain::CITY_RING;
    (0..ring.len())
        .map(|i| {
            let a = Vec2::from(ring[i]);
            let b = Vec2::from(ring[(i + 1) % ring.len()]);
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            p.distance(a + (b - a) * t)
        })
        .fold(f32::MAX, f32::min)
}

pub fn city_layout() -> Layout {
    let mut b = Builder::new();
    // Где стоят торговцы, учителя и стража (несколько торговцев одного места — через 2,4 м).
    for p in super::places::places() {
        for dx in [0.0, 2.4, -2.4] {
            b.keep_clear.push(p.pos + Vec3::X * dx);
        }
    }
    b.keep_clear.extend(super::places::npc_posts().iter().map(|p| p.pos));

    walls::fortifications(&mut b);
    streets(&mut b);

    for i in 0..NCOL {
        for j in ROWS {
            let r = block(i, j);
            b.at(r.cx(), r.cz());
            let seed = (i as u32 * 131 + (j + 4) as u32 * 7919) ^ 0x5eed;
            // По-старому (две усадьбы на квартал) строятся только кварталы с именованными лавками;
            // остальная жилая застройка — по плану (см. `plan`).
            if block_use(i, j) == Use::Houses && plan::classic(i, j) {
                houses::block_houses(&mut b, i, j, seed);
            }
        }
    }
    houses::plan_houses(&mut b);
    b.variant = 0;
    // Общественные здания и мастерские: у каждого свой вариант кладки, черепицы и штукатурки.
    b.vary(2);
    public::agora(&mut b);
    b.vary(6);
    public::basilica(&mut b);
    b.vary(10);
    public::episcopium(&mut b);
    b.vary(14);
    public::didaskaleion(&mut b);
    b.vary(18);
    public::exiles_house(&mut b);
    b.vary(22);
    public::thermae(&mut b);
    b.vary(26);
    public::theatre(&mut b);
    b.vary(30);
    public::reservoir(&mut b);
    b.vary(34);
    public::west_church(&mut b);
    b.vary(38);
    public::citadel(&mut b);
    b.vary(42);
    crafts::fish_salting(&mut b, block(18, -3).union(&block(19, -3)), 1);
    b.vary(46);
    crafts::fish_salting(&mut b, block(8, -3), 2);
    b.vary(50);
    crafts::glassworks(&mut b);
    b.vary(54);
    crafts::pottery(&mut b);
    b.vary(58);
    crafts::smithy(&mut b);
    b.vary(62);
    crafts::winery(&mut b);
    b.variant = 0;
    port::port(&mut b);
    outside::outside(&mut b);

    // Край карты. На юге он дальше — за предгорьями (`mountains::WORLD_SOUTH`), и стены там
    // выше: склоны поднимаются на сотни метров.
    let e = WORLD_HALF - 4.0;
    let s = super::mountains::WORLD_SOUTH - 4.0;
    for (min, max) in [
        (Vec3::new(-e - 2.0, -20.0, -e - 2.0), Vec3::new(e + 2.0, 200.0, -e)),
        (Vec3::new(-e - 2.0, -20.0, s), Vec3::new(e + 2.0, 1500.0, s + 2.0)),
        (Vec3::new(-e - 2.0, -20.0, -e), Vec3::new(-e, 1500.0, s)),
        (Vec3::new(e, -20.0, -e), Vec3::new(e + 2.0, 1500.0, s)),
    ] {
        b.invisible.push(crate::collision::Aabb::new(min, max));
    }

    let holes = std::mem::take(&mut b.holes);
    b.ground = Some(terrain::heightfield(holes));
    b.water_y = Some(WATER_Y);
    let mut layout = b.finish();
    let lamps = door_lamps(&layout);
    layout.torches.extend(lamps);
    layout
}

/// Факелы у дверей лавок и домов на мощёной главной улице и агоре (у каждой второй двери):
/// ночью от них на мостовой тёплые пятна света. Снаружи — та сторона, где под ногами плиты.
fn door_lamps(layout: &Layout) -> Vec<super::Torch> {
    let paving: Vec<&super::Block> = layout.blocks.iter().filter(|b| b.mat == Mat::Paving).collect();
    let paved = |p: Vec3| paving.iter().any(|b| b.aabb.min.x <= p.x && p.x <= b.aabb.max.x && b.aabb.min.z <= p.z && p.z <= b.aabb.max.z && (b.aabb.max.y - p.y).abs() < 0.5);
    let mut out = Vec::new();
    for (i, d) in layout.doors.iter().enumerate() {
        if crate::world_clock::unit(i as u64, 0x1A4) > 0.5 || d.height < 1.8 {
            continue;
        }
        let n = if d.along_x { Vec3::Z } else { Vec3::X };
        let off = d.wall_t * 0.5 + 0.6;
        let dir = match (paved(d.center + n * off), paved(d.center - n * off)) {
            (true, false) => n,
            (false, true) => -n,
            _ => continue,
        };
        let side = if d.along_x { Vec3::X } else { Vec3::Z };
        let pos = d.center + dir * (d.wall_t * 0.5 + 0.22) + side * (d.width * 0.5 + 0.4) + Vec3::Y * 2.15;
        out.push(super::Torch { pos, out: dir });
    }
    out
}

/// Мостовые: главная улица и площадь — известняковые плиты с крытым водостоком посередине;
/// остальные улицы — утоптанная земля со щебнем (рисуется рельефом).
fn streets(b: &mut Builder) {
    let x_start = block(0, 1).x0 - 30.0;
    let x_end = EAST_WALL_X - WALL_T / 2.0;
    let mut x = x_start;
    while x < x_end {
        let x1 = (x + 32.0).min(x_end);
        b.at((x + x1) / 2.0, 0.0);
        b.decor([x, 0.0, -MAIN_HALF], [x1, 0.03, MAIN_HALF], Mat::Paving);
        // Плиты, перекрывающие водосток, и бордюры.
        b.mesh(Mat::Limestone).cuboid(Vec3::new(x, 0.0, -0.45), Vec3::new(x1, 0.045, 0.45));
        for z in [-MAIN_HALF, MAIN_HALF] {
            b.mesh(Mat::Limestone).cuboid(Vec3::new(x, 0.0, z - 0.2), Vec3::new(x1, 0.1, z + 0.2));
        }
        x = x1;
    }
}

/// Точки появления игроков: агора, у восточного портика.
pub fn spawn_point(index: usize) -> Vec3 {
    let col = (index % 4) as f32;
    let row = (index / 4 % 4) as f32;
    let a = block(16, -1).union(&block(17, -1));
    Vec3::new(a.cx() - 3.0 + col * 2.0, 0.1, a.z1 - 12.0 + row * 1.5)
}

#[cfg(test)]
mod tests {
    use super::super::{colliders, places};
    use super::*;
    use crate::collision::{Body, LevelColliders};

    fn level() -> LevelColliders {
        colliders(city_layout())
    }

    fn free(level: &LevelColliders, p: Vec3) -> bool {
        level.body_is_free(&Body::STANDING, p)
    }

    #[test]
    fn street_doors_get_lamps_and_windows_are_known() {
        let layout = city_layout();
        let lamps = door_lamps(&layout);
        assert!(lamps.len() > 60, "факелов у дверей: {}", lamps.len());
        // Факел смотрит на улицу (горизонтально) и висит над человеческим ростом.
        for t in &lamps {
            assert!((t.out.length() - 1.0).abs() < 1e-3 && t.out.y == 0.0);
            assert!(t.pos.y > 1.8, "{:?}", t.pos);
        }
        assert!(layout.windows.len() > 100, "окон: {}", layout.windows.len());
    }

    /// Идёт по ломаной; возвращает конечную точку. Падает, если застрял.
    fn walk(level: &LevelColliders, path: &[Vec3]) -> Vec3 {
        let mut feet = path[0];
        let mut grounded = true;
        for target in &path[1..] {
            for _ in 0..20000 {
                let to = *target - feet;
                let flat = Vec3::new(to.x, 0.0, to.z);
                if flat.length() < 0.3 {
                    break;
                }
                let step = flat.normalize() * 0.15;
                grounded = level.move_body(&Body::STANDING, &mut feet, step + Vec3::new(0.0, -0.08, 0.0), grounded).grounded;
            }
            let to = *target - feet;
            assert!(Vec2::new(to.x, to.z).length() < 0.6, "застрял по пути к {target}: стою в {feet}");
        }
        feet
    }

    /// Отладка: что мешает в точках из `CITY_PROBE` ("x,z;x,z") и где ближайшее свободное место.
    #[test]
    #[ignore]
    fn probe() {
        let level = level();
        let mut pts: Vec<(String, Vec3)> = places::places().iter().map(|p| (p.id.to_string(), p.pos)).collect();
        pts.extend(places::npc_posts().iter().map(|p| (format!("{:?}", p.group), p.pos)));
        if let Ok(s) = std::env::var("CITY_PROBE") {
            for q in s.split(';') {
                let v: Vec<f32> = q.split(',').map(|t| t.parse().unwrap()).collect();
                pts.push((q.to_string(), Vec3::new(v[0], v.get(2).copied().unwrap_or(0.0), v[1])));
            }
        }
        for (name, p) in pts {
            let p = p + Vec3::Y * 0.1;
            if free(&level, p) {
                continue;
            }
            let a = Body::STANDING.aabb_at(p);
            let hits: Vec<_> = level.0.iter().filter(|c| c.overlaps(&a)).map(|c| format!("[{:.2} {:.2} {:.2}]-[{:.2} {:.2} {:.2}]", c.min.x, c.min.y, c.min.z, c.max.x, c.max.y, c.max.z)).collect();
            let mut best = None;
            'search: for r in 1..40 {
                for k in 0..(r * 8) {
                    let ang = k as f32 / (r * 8) as f32 * std::f32::consts::TAU;
                    let q = p + Vec3::new(ang.cos(), 0.0, ang.sin()) * (r as f32 * 0.25);
                    let q = Vec3::new(q.x, level.ground_height(q.x, q.z).max(0.0) + 0.1, q.z);
                    if free(&level, q) {
                        best = Some(q);
                        break 'search;
                    }
                }
            }
            println!("{name} @ {p}: ground {:.2}; hits {hits:?}; nearest free {best:?}", level.ground_height(p.x, p.z));
        }
    }

    #[test]
    fn spawn_points_and_npc_posts_are_free() {
        let level = level();
        for i in 0..16 {
            assert!(free(&level, spawn_point(i)), "точка появления {i} в стене: {}", spawn_point(i));
        }
        for p in places::npc_posts() {
            let pos = p.pos + Vec3::Y * 0.1;
            assert!(free(&level, pos), "пост {:?} в стене: {pos}", p.group);
        }
        for p in super::super::npc_spawn_hints() {
            for i in 0..16 {
                assert!(p.distance(spawn_point(i)) >= 12.0, "враг {p} слишком близко к появлению");
            }
        }
    }

    #[test]
    fn places_are_reachable_floor() {
        let level = level();
        for p in places::places() {
            let pos = p.pos + Vec3::Y * 0.1;
            assert!(free(&level, pos), "место «{}» ({}) в стене: {pos}", p.name, p.id);
            assert!(places::place(p.id).is_some());
        }
        let mut ids: Vec<&str> = places::places().iter().map(|p| p.id).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(n, ids.len(), "повторяются id мест");
    }

    /// Заливка проходимого пола города от агоры (шаг 0,4 м, на уровне улиц): куда можно дойти.
    fn reachable_from_agora(level: &LevelColliders) -> impl Fn(Vec3) -> bool {
        let cell = 0.4;
        let (x0, z0) = (-305.0f32, -170.0f32);
        let (nx, nz) = ((610.0 / cell) as i32, (345.0 / cell) as i32);
        let at = |i: i32, k: i32| Vec3::new(x0 + i as f32 * cell, 0.1, z0 + k as f32 * cell);
        let idx = move |i: i32, k: i32| (k * nx + i) as usize;
        let mut seen = vec![false; (nx * nz) as usize];
        let start = spawn_point(0);
        let s = (((start.x - x0) / cell).round() as i32, ((start.z - z0) / cell).round() as i32);
        let mut queue = std::collections::VecDeque::from([s]);
        seen[idx(s.0, s.1)] = true;
        while let Some((i, k)) = queue.pop_front() {
            for (di, dk) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (a, c) = (i + di, k + dk);
                if a < 0 || c < 0 || a >= nx || c >= nz || seen[idx(a, c)] {
                    continue;
                }
                let p = at(a, c);
                if level.ground_height(p.x, p.z) < 0.3 && free(level, p) {
                    seen[idx(a, c)] = true;
                    queue.push_back((a, c));
                }
            }
        }
        move |p: Vec3| {
            let (i, k) = (((p.x - x0) / cell).round() as i32, ((p.z - z0) / cell).round() as i32);
            (-1..=1).any(|di| {
                (-1..=1).any(|dk| {
                    let (a, c) = (i + di, k + dk);
                    a >= 0 && c >= 0 && a < nx && c < nz && seen[idx(a, c)]
                })
            })
        }
    }

    /// Заливка проходимого пола города от агоры: к каждому месту
    /// внутри стен и на набережной можно дойти — лавки, кабинеты и мастерские не замурованы.
    #[test]
    fn places_inside_walls_are_reachable() {
        let level = level();
        // Двери открыты: проверяем, что комнаты не замурованы мебелью и жителями.
        level.set_all_doors(true);
        let reached = reachable_from_agora(&level);
        let mut lost = Vec::new();
        for p in places::places().iter().filter(|p| p.pos.y < 0.5 && p.pos.z < 166.0) {
            if !reached(p.pos) {
                lost.push(format!("{} {}", p.id, p.pos));
            }
        }
        for p in places::npc_posts().iter().filter(|p| p.pos.y < 0.5 && p.pos.z < 182.0) {
            if !reached(p.pos) {
                lost.push(format!("{:?} {}", p.group, p.pos));
            }
        }
        assert!(lost.is_empty(), "не дойти: {lost:?}");
    }

    /// Места появления: на открытой мостовой внутри стен, с простором вокруг, не у прилавков
    /// и не на постах стражи; до каждого можно дойти от агоры, и они разбросаны по городу.
    #[test]
    fn spawn_spots_are_open_streets_across_the_city() {
        let level = level();
        level.set_all_doors(true);
        let reached = reachable_from_agora(&level);
        let spots = crate::level::spawn_spots();
        assert!(spots.len() >= 12, "мест появления: {}", spots.len());
        let mut stalls: Vec<Vec3> = Vec::new();
        for id in ["agora", "port_market"] {
            for s in places::stall_slots(id) {
                stalls.push(s.pos);
                stalls.push(s.pos + s.facing * places::SELLER_BEHIND);
            }
        }
        let posts: Vec<Vec3> = places::npc_posts().iter().map(|p| p.pos).collect();
        let mut bad = Vec::new();
        for s in spots {
            let p = s.pos;
            let mut why = Vec::new();
            if !terrain::inside_city(p.x, p.z) {
                why.push("за стенами".to_string());
            }
            if !free(&level, p) {
                why.push("в стене".into());
            }
            // Простор: шаг в любую сторону свободен.
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                if !free(&level, p + Vec3::new(a.cos(), 0.0, a.sin()) * 1.2) {
                    why.push(format!("тесно на {}°", k * 45));
                }
            }
            if !reached(p) {
                why.push("не дойти от агоры".into());
            }
            if let Some(q) = stalls.iter().find(|q| q.xz().distance(p.xz()) < 2.5) {
                why.push(format!("у прилавка {q}"));
            }
            if let Some(q) = posts.iter().find(|q| q.xz().distance(p.xz()) < 2.0) {
                why.push(format!("на посту {q}"));
            }
            if !why.is_empty() {
                bad.push(format!("{} {}: {}", s.id, p, why.join(", ")));
            }
        }
        assert!(bad.is_empty(), "плохие места появления:\n{}", bad.join("\n"));
        for (i, a) in spots.iter().enumerate() {
            for b in &spots[i + 1..] {
                assert!(a.pos.xz().distance(b.pos.xz()) > 15.0, "{} и {} слишком близко", a.id, b.id);
                assert_ne!(a.id, b.id);
            }
        }
        // Город велик: места от западной стены до порта.
        let xs: Vec<f32> = spots.iter().map(|s| s.pos.x).collect();
        let span = xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!(span > 300.0, "места появления жмутся друг к другу: {span} м");
    }

    /// Выбор места: случайный, разный, подальше от других, точка всегда свободна.
    #[test]
    fn picking_spawns_spreads_players() {
        use crate::level::{SpawnChoice, pick_spawn};
        let level = level();
        let mut seed = 7u64;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let (id, p, _) = pick_spawn(&SpawnChoice::Random, &level, &[], None, &mut rand);
            assert!(free(&level, p), "{id} {p}");
            seen.insert(id);
        }
        assert!(seen.len() >= crate::level::spawn_spots().len() - 1, "выпадают не все места: {seen:?}");
        // Прошлое место не повторяется; рядом с другим игроком не ставим.
        let first = &crate::level::spawn_spots()[0];
        for _ in 0..50 {
            let (id, p, _) = pick_spawn(&SpawnChoice::Random, &level, &[first.pos], Some("main_middle"), &mut rand);
            assert_ne!(id, "main_middle");
            assert!(p.xz().distance(first.pos.xz()) > 12.0);
        }
        // Выбранное место — ровно оно.
        let (id, p, yaw) = pick_spawn(&SpawnChoice::At("port_market".into()), &level, &[], None, &mut rand);
        let spot = crate::level::spawn_spot("port_market").unwrap();
        assert_eq!(id, "port_market");
        assert!(p.distance(spot.pos) < 1.7 && yaw == spot.yaw);
    }

    #[test]
    fn herb_patches_on_land_outside_walls() {
        for h in places::herb_patches() {
            assert!(!terrain::inside_city(h.pos.x, h.pos.z), "трава {} внутри стен", h.herb);
            assert!(h.pos.y > WATER_Y + 0.3, "трава {} под водой: {}", h.herb, h.pos);
            assert!(crate::lore::Substance::from_latin(h.herb).is_some(), "трава {} неизвестна травнику", h.herb);
        }
    }

    /// От агоры по главной улице к главным воротам, через протейхизму наружу, к некрополю;
    /// обратно — в южные ворота и по главной улице к террасе над бухтой.
    #[test]
    fn walk_through_the_city() {
        let level = level();
        let gx = main_gate_x();
        let sx = south_gate_x();
        let a = spawn_point(0);
        walk(&level, &[
            a,
            Vec3::new(a.x, 0.0, 0.0),
            Vec3::new(gx, 0.0, 0.0),
            Vec3::new(gx, 0.0, 150.0),
            Vec3::new(gx, 0.0, 200.0),
        ]);
        walk(&level, &[Vec3::new(sx, 0.0, 200.0), Vec3::new(sx, 0.0, 150.0), Vec3::new(sx, 0.0, 0.0), Vec3::new(272.0, 0.0, 0.0)]);
    }

    /// От южных ворот — по дороге к карнизу над бухтой и по нему вниз, в порт, до причала.
    #[test]
    fn walk_down_to_the_port() {
        let level = level();
        let road = outside::port_road();
        let mut path: Vec<Vec3> = road.iter().map(|p| Vec3::new(p.x, terrain::ground_y(p.x, p.y), p.y)).collect();
        let pier = port::on_map(Vec3::new(terrain::QUAY_EDGE + 10.0, 0.0, port::PIER_Z[1]));
        path.push(port::on_map(Vec3::new(290.0, 0.0, port::PIER_Z[1])));
        path.push(pier);
        let end = walk(&level, &path);
        assert!((end.y - terrain::SHORE_Y).abs() < 0.3, "причал не на уровне берега: {end}");
        // Порт далеко от стен и под обрывом.
        assert!(terrain::city_distance(pier.x, pier.z) > 200.0);
        assert!(terrain::ground_y(212.0, pier.z) > terrain::SHORE_Y + 8.0, "над портом нет обрыва");
    }

    /// Город стоит на обрыве: под стенами с моря — десятки метров скалы, а с суши подход ровный.
    #[test]
    fn city_stands_on_a_cliff_with_land_approach_only() {
        for (x, z) in [(0.0, -230.0), (-200.0, -235.0), (-372.0, -100.0), (-360.0, 0.0), (330.0, -60.0), (330.0, 100.0)] {
            assert!(terrain::coast(x, z) < 0.0, "тут должно быть море: {x} {z}");
        }
        // Под северной, западной и восточной стенами земля держится на уровне города, дальше — обрыв.
        for (x, z, dx, dz) in [(0.0, -166.0, 0.0, -1.0), (-300.5, -20.0, -1.0, 0.0), (285.0, 0.0, 1.0, 0.0)] {
            let near = terrain::ground_y(x + dx * 7.0, z + dz * 7.0);
            assert!(near.abs() < 0.6, "у стены ({x}, {z}) земля не ровная: {near}");
            let far = (20..60).map(|k| terrain::ground_y(x + dx * k as f32, z + dz * k as f32)).fold(f32::MAX, f32::min);
            assert!(far < WATER_Y + 1.5, "за стеной ({x}, {z}) нет обрыва к морю: {far}");
        }
        // С юга — плато: от ворот можно уйти на сотни метров, не спускаясь.
        for z in [190.0, 230.0, 300.0] {
            assert!(terrain::ground_y(main_gate_x(), z) > -1.0, "к югу от ворот провал: z = {z}");
        }
        // Ворота только на южной стене.
        let level = level();
        for z in (-110..160).step_by(3) {
            assert!(!free(&level, Vec3::new(EAST_WALL_X, 0.1, z as f32)), "в восточной стене проход на z = {z}");
        }
    }

    #[test]
    fn climb_the_wall_by_the_main_gate() {
        let level = level();
        let s = walls::gate_stairs_start();
        let top = walk(&level, &[s.0, s.1, s.1 + Vec3::new(0.0, 0.0, 1.6)]);
        assert!(top.y > WALL_H - 0.1, "не поднялся на стену: {top}");
    }

    #[test]
    fn block_grid_is_consistent() {
        // Кварталы не пересекаются и не залезают на главную улицу.
        for i in 0..NCOL {
            for j in ROWS {
                let r = block(i, j);
                assert!(r.z0 >= MAIN_HALF - 0.01 || r.z1 <= -MAIN_HALF + 0.01);
                assert!((r.w() - BX).abs() < 0.01 && (r.d() - BZ).abs() < 0.01);
            }
        }
        assert!(block_fits(10, -3) && block_fits(10, 3));
    }
}

#[cfg(test)]
mod interior_tests {
    use super::*;
    use crate::collision::Body;

    /// Город обжит: двери, сундуки и жители есть, и мебель не загораживает двери.
    #[test]
    fn interiors_are_lived_in_and_doors_passable() {
        let layout = city_layout();
        assert!(layout.doors.len() > 1500, "дверей мало: {}", layout.doors.len());
        assert!(layout.containers.len() > 1500, "сундуков мало: {}", layout.containers.len());
        assert!(layout.residents.len() > 600, "жителей мало: {}", layout.residents.len());
        let doors = layout.doors.clone();
        let level = super::super::colliders(layout);
        level.set_all_doors(true);
        let mut blocked = Vec::new();
        for d in doors.iter().filter(|d| d.center.y < 0.5 && d.width < 2.5) {
            let across = if d.along_x { Vec3::Z } else { Vec3::X };
            // Сквозь проём и по полметра за ним с обеих сторон.
            for k in [-1.0f32, 0.0, 1.0] {
                let p = d.center + across * (k * (d.wall_t / 2.0 + 0.45)) + Vec3::Y * 0.1;
                // Низкую скамью перешагнуть можно.
                let free = level.body_is_free(&Body::STANDING, p) || level.body_is_free(&Body::STANDING, p + Vec3::Y * 0.5);
                if level.ground_height(p.x, p.z) < 0.3 && !free {
                    let a = Body::STANDING.aabb_at(p);
                    let hits: Vec<String> = level.0.iter().filter(|c| c.overlaps(&a)).map(|c| format!("[{:.2} {:.2} {:.2}]-[{:.2} {:.2} {:.2}]", c.min.x, c.min.y, c.min.z, c.max.x, c.max.y, c.max.z)).collect();
                    blocked.push(format!("{} ({k}) {hits:?}", d.center));
                    break;
                }
            }
        }
        assert!(blocked.len() < 3, "загорожено {} дверей: {:?}", blocked.len(), &blocked[..blocked.len().min(20)]);
    }
}
