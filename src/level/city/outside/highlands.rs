//! Предгорья к югу от плато (рельеф — `level/mountains.rs`): то, ради чего туда идти.
//!
//! В 510 году горы за Херсоном — земля крымских готов (Прокопий зовёт её Дори): пастбища на
//! открытых вершинах, леса из дуба пушистого и крымской сосны, известняковые уступы с гротами.
//! - **тропы** от рудника вверх, по пологим местам (ищутся по рельефу), к кошу и к гроту;
//! - **пастуший кош** на высокой поляне: каменная хижина под соломой, загон из сухой кладки,
//!   кострище, поилка у ключа, пастухи-готы;
//! - **грот** в обрыве куэсты — логово разбойников: костёр, лежанки, их добро в сундуке;
//! - **лес по высотам**: внизу дуб и можжевельник с кустами шибляка, выше — сосна; на крутом —
//!   выходы камня; на вершинах — открытые поляны;
//! - **горные травы**: чабрец и зверобой на полянах, шиповник по опушкам, окопник и мята у ключа.

use super::*;
use crate::level::mountains::{self, MOUNT_L, WORLD_SOUTH};
use std::sync::OnceLock;

/// Где что стоит (ищется по рельефу один раз: рельеф задан формулой, места выходят те же
/// на сервере и у игроков).
pub struct Sites {
    /// Середина коша (поляна).
    pub camp: Vec2,
    /// Ключ под кошем: поилка, мята и окопник.
    pub spring: Vec2,
    pub grotto: Grotto,
}

/// Грот в обрыве: `foot` — подошва обрыва посередине устья, `dir` — вглубь скалы (по плану),
/// `floor` — высота пола.
#[derive(Clone, Copy, Debug)]
pub struct Grotto {
    pub foot: Vec2,
    pub dir: Vec2,
    pub floor: f32,
}

/// Глубина грота (от подошвы обрыва до дальней стены), м.
const GROTTO_LEN: f32 = 15.0;

impl Grotto {
    fn side(&self) -> Vec2 {
        Vec2::new(-self.dir.y, self.dir.x)
    }
    /// Точка грота: `t` — вглубь от подошвы, `s` — вбок.
    pub fn at(&self, t: f32, s: f32) -> Vec2 {
        self.foot + self.dir * t + self.side() * s
    }
    /// (вглубь, вбок) для точки плана.
    fn local(&self, p: Vec2) -> (f32, f32) {
        let d = p - self.foot;
        (d.dot(self.dir), d.dot(self.side()))
    }
    /// Полуширина прохода на глубине `t` (у устья уже, в середине — зал).
    fn half_w(t: f32) -> f32 {
        let hall = terrain::smoothstep(2.0, 6.0, t) * (1.0 - terrain::smoothstep(11.0, GROTTO_LEN, t));
        1.9 + 1.7 * hall
    }
    /// Высота свода над полом на глубине `t`.
    fn height(t: f32) -> f32 {
        let hall = terrain::smoothstep(1.0, 5.0, t) * (1.0 - terrain::smoothstep(11.5, GROTTO_LEN, t));
        2.9 + 1.3 * hall
    }
    /// Внутри ли полости (по плану, с запасом `pad`).
    fn inside(&self, p: Vec2, pad: f32) -> bool {
        let (t, s) = self.local(p);
        t > -3.0 && t < GROTTO_LEN && s.abs() < Self::half_w(t.max(0.0)) + pad
    }
    /// Вход в грот (снаружи, у устья).
    pub fn mouth(&self) -> Vec3 {
        let p = self.at(-2.0, 0.0);
        Vec3::new(p.x, cell_top(p).max(self.floor), p.y)
    }
    /// Середина зала.
    pub fn hall(&self) -> Vec3 {
        let p = self.at(8.0, 0.0);
        Vec3::new(p.x, self.floor, p.y)
    }
}

/// Верх тверди у грота в точке плана: столбы 1×1 м по целым метрам, верх — по самому высокому
/// углу (см. `grotto`).
fn cell_top(p: Vec2) -> f32 {
    let (x, z) = (p.x.floor(), p.y.floor());
    [(x, z), (x + 1.0, z), (x, z + 1.0), (x + 1.0, z + 1.0)].into_iter().map(|(x, z)| terrain::plain_y(x, z)).fold(f32::MIN, f32::max)
}

fn slope_at(p: Vec2) -> Vec2 {
    let e = 2.0;
    Vec2::new(ground_y(p.x + e, p.y) - ground_y(p.x - e, p.y), ground_y(p.x, p.y + e) - ground_y(p.x, p.y - e)) / (2.0 * e)
}

pub fn sites() -> &'static Sites {
    static S: OnceLock<Sites> = OnceLock::new();
    S.get_or_init(|| {
        let camp = find_camp();
        let spring = find_spring(camp);
        let grotto = find_grotto(camp);
        Sites { camp, spring, grotto }
    })
}

/// Кош — на самой высокой ровной поляне в середине гор (не у края карты).
fn find_camp() -> Vec2 {
    let mut best = (f32::MIN, Vec2::new(0.0, 2100.0));
    let mut z = 1750.0;
    while z < WORLD_SOUTH - 260.0 {
        let mut x = -650.0;
        while x < 650.0 {
            let c = Vec2::new(x, z);
            let y = ground_y(x, z);
            // Ровно в круге 24 м: перепад не больше 3 м.
            let ring = (0..12).map(|k| {
                let a = k as f32 / 12.0 * TAU;
                ground_y(x + a.cos() * 24.0, z + a.sin() * 24.0)
            });
            let (lo, hi) = ring.fold((y, y), |(lo, hi), v| (lo.min(v), hi.max(v)));
            if hi - lo < 3.0 {
                // Выше — лучше, но и поближе к середине карты.
                let score = y - c.x.abs() * 0.08;
                if score > best.0 {
                    best = (score, c);
                }
            }
            x += 12.0;
        }
        z += 12.0;
    }
    best.1
}

/// Ключ — в ближайшей к кошу ложбине ниже его (где сходится вода): самое низкое место кольца
/// 60–110 м вокруг, на пологом.
fn find_spring(camp: Vec2) -> Vec2 {
    let mut best = (f32::MAX, camp + Vec2::new(70.0, 0.0));
    for r in [60.0f32, 75.0, 90.0, 110.0] {
        for k in 0..32 {
            let a = k as f32 / 32.0 * TAU;
            let p = camp + Vec2::new(a.cos(), a.sin()) * r;
            if slope_at(p).length() > 0.25 || p.y > WORLD_SOUTH - 80.0 {
                continue;
            }
            let y = ground_y(p.x, p.y);
            if y < best.0 {
                best = (y, p);
            }
        }
    }
    best.1
}

/// Грот — в обрыве куэсты: под ним площадка, над сводом — не меньше 6 м камня на всю глубину.
/// Из подходящих — ближайший к кошу (идти недалеко, но и не в самом коше).
fn find_grotto(camp: Vec2) -> Grotto {
    let mut best: Option<(f32, Grotto)> = None;
    let mut z = 1400.0;
    while z < WORLD_SOUTH - 120.0 {
        let mut x = -760.0;
        while x < 760.0 {
            let p = Vec2::new(x, z);
            x += 9.0;
            let gr = slope_at(p);
            if gr.length() < 1.1 {
                continue;
            }
            // Вверх по склону — вглубь скалы; вниз — к подошве.
            let dir = gr.normalize();
            let mut foot = p;
            let mut found = false;
            for _ in 0..90 {
                foot -= dir;
                if slope_at(foot).length() < 0.3 {
                    found = true;
                    break;
                }
            }
            if !found {
                continue;
            }
            let floor = ground_y(foot.x, foot.y);
            // Подошва — где склон начинает круто расти: устье прямо в стене, без навеса.
            for _ in 0..40 {
                let q = foot + dir * 0.5;
                if ground_y(q.x, q.y) > floor + 0.8 {
                    break;
                }
                foot = q;
            }
            let g = Grotto { foot, dir, floor };
            // Площадка перед устьем — ровная, по бокам устья — скала.
            let apron = (0..6).all(|k| {
                let q = g.at(-2.0 - k as f32 * 2.0, 0.0);
                (ground_y(q.x, q.y) - floor).abs() < 2.2
            });
            // Над сводом камень толщеет вглубь: у устья — полтора метра, дальше — до шести.
            let rock = [2.0f32, 4.0, 6.0, 9.0, 12.0, GROTTO_LEN].iter().all(|&t| {
                [-1.0f32, 0.0, 1.0].iter().all(|&s| {
                    let q = g.at(t, s * (Grotto::half_w(t) + 1.0));
                    ground_y(q.x, q.y) > floor + Grotto::height(t) + (1.5 + t * 0.6).min(6.0)
                })
            });
            let inside = (-850.0..850.0).contains(&foot.x) && foot.y < WORLD_SOUTH - 70.0 && g.at(GROTTO_LEN + 8.0, 0.0).y < WORLD_SOUTH - 20.0;
            if !(apron && rock && inside) {
                continue;
            }
            let d = foot.distance(camp);
            let score = (d - 380.0).abs();
            if best.as_ref().is_none_or(|(s, _)| score < *s) {
                best = Some((score, g));
            }
        }
        z += 9.0;
    }
    best.map(|b| b.1).unwrap_or(Grotto { foot: camp + Vec2::new(200.0, 0.0), dir: MOUNT_L, floor: ground_y(camp.x + 200.0, camp.y) })
}

// ---------------------------------------------------------------------------
// Тропы
// ---------------------------------------------------------------------------

/// Тропы в горы: от рудника к кошу, от коша к гроту. Идут по пологим местам: путь ищется по
/// сетке рельефа, крутой подъём дорог, обрыв непроходим.
pub fn trails() -> &'static [Vec<Vec2>] {
    static T: OnceLock<Vec<Vec<Vec2>>> = OnceLock::new();
    T.get_or_init(|| {
        let s = sites();
        let (mx, fz) = terrain::MINE_FACE;
        let start = Vec2::new(mx + 30.0, fz + 40.0);
        let mut v = Vec::new();
        let camp_in = s.camp + (start - s.camp).normalize() * 20.0;
        if let Some(t) = find_path(start, camp_in) {
            v.push(t);
        }
        let mouth = s.grotto.at(-7.0, 0.0);
        let from = s.camp + (mouth - s.camp).normalize() * 20.0;
        if let Some(t) = find_path(from, mouth) {
            v.push(t);
        }
        v
    })
}

/// Тропы, где они начинаются у дорог (для `roads`): от рудничной дороги к началу тропы.
pub fn trail_links() -> Vec<Vec<Vec2>> {
    let (mx, fz) = terrain::MINE_FACE;
    vec![vec![Vec2::new(mx - 4.0, fz - 30.0), Vec2::new(mx + 22.0, fz - 22.0), Vec2::new(mx + 30.0, fz + 40.0)]]
}

/// Путь по сетке 6 м (A*): цена шага растёт с уклоном, круче 0,42 — нельзя.
fn find_path(from: Vec2, to: Vec2) -> Option<Vec<Vec2>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    const C: f32 = 6.0;
    let (x0, x1) = (-870.0f32, 870.0f32);
    let (z0, z1) = (560.0f32, WORLD_SOUTH - 40.0);
    let nx = ((x1 - x0) / C) as i32 + 1;
    let nz = ((z1 - z0) / C) as i32 + 1;
    let idx = |i: i32, k: i32| (k * nx + i) as usize;
    let pos = |i: i32, k: i32| Vec2::new(x0 + i as f32 * C, z0 + k as f32 * C);
    let hgt: Vec<f32> = (0..nz).flat_map(|k| (0..nx).map(move |i| (i, k))).map(|(i, k)| {
        let p = pos(i, k);
        ground_y(p.x, p.y)
    }).collect();
    let cell = |p: Vec2| (((p.x - x0) / C).round() as i32, ((p.y - z0) / C).round() as i32);
    let (si, sk) = cell(from);
    let (ti, tk) = cell(to);
    let n = (nx * nz) as usize;
    let mut cost = vec![f32::MAX; n];
    let mut prev = vec![u32::MAX; n];
    let mut open = BinaryHeap::new();
    let key = |f: f32| Reverse((f * 16.0) as u64);
    cost[idx(si, sk)] = 0.0;
    open.push((key(0.0), si, sk));
    let goal = pos(ti, tk);
    while let Some((_, i, k)) = open.pop() {
        if (i, k) == (ti, tk) {
            break;
        }
        let c0 = cost[idx(i, k)];
        let h0 = hgt[idx(i, k)];
        for (di, dk) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (j, m) = (i + di, k + dk);
            if j < 0 || m < 0 || j >= nx || m >= nz {
                continue;
            }
            let len = C * ((di * di + dk * dk) as f32).sqrt();
            let dh = hgt[idx(j, m)] - h0;
            let grade = dh.abs() / len;
            if grade > 0.42 || hgt[idx(j, m)] < WATER_Y + 1.0 {
                continue;
            }
            let c = c0 + len * (1.0 + 30.0 * grade * grade);
            if c < cost[idx(j, m)] {
                cost[idx(j, m)] = c;
                prev[idx(j, m)] = idx(i, k) as u32;
                open.push((key(c + pos(j, m).distance(goal)), j, m));
            }
        }
    }
    if cost[idx(ti, tk)] == f32::MAX {
        return None;
    }
    let mut cells = vec![idx(ti, tk)];
    while let Some(&c) = cells.last() {
        let p = prev[c];
        if p == u32::MAX {
            break;
        }
        cells.push(p as usize);
    }
    cells.reverse();
    let pts: Vec<Vec2> = cells.iter().map(|&c| pos(c as i32 % nx, c as i32 / nx)).collect();
    // Прямые куски — одним отрезком (до 3 м в сторону); ступеньки сетки сглаживаются.
    let mut out = simplify(&pts, 3.0);
    // Ступеньки сетки и мелкие зигзаги — плавной дугой (срезание углов Чайкина).
    for _ in 0..2 {
        let mut s = vec![out[0]];
        for w in out.windows(2) {
            s.push(w[0].lerp(w[1], 0.25));
            s.push(w[0].lerp(w[1], 0.75));
        }
        s.push(*out.last().unwrap());
        out = s;
    }
    out = simplify(&out, 0.6);
    *out.first_mut().unwrap() = from;
    *out.last_mut().unwrap() = to;
    Some(out)
}

/// Упрощение ломаной (Дуглас — Пекер).
fn simplify(pts: &[Vec2], tol: f32) -> Vec<Vec2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, c) = (pts[0], *pts.last().unwrap());
    let (mut far, mut at) = (0.0f32, 0);
    for (k, p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
        let d = super::seg_distance(*p, a, c);
        if d > far {
            far = d;
            at = k;
        }
    }
    if far <= tol {
        return vec![a, c];
    }
    let mut l = simplify(&pts[..=at], tol);
    let r = simplify(&pts[at..], tol);
    l.pop();
    l.extend(r);
    l
}

/// Поляны, где не растёт лес (кош, ключ, площадка у грота).
pub fn clearings() -> Vec<(Vec2, f32)> {
    let s = sites();
    vec![(s.camp, 34.0), (s.spring, 9.0), (s.grotto.at(-6.0, 0.0), 14.0)]
}

// ---------------------------------------------------------------------------
// Для систем игры: травы, люди, места
// ---------------------------------------------------------------------------

/// Заросли горных трав (латинский род, точка, радиус).
pub fn herb_spots() -> Vec<(&'static str, Vec2, f32)> {
    let s = sites();
    let c = s.camp;
    let g = s.grotto;
    vec![
        ("thymus", c + Vec2::new(30.0, 22.0), 8.0),
        ("thymus", c + Vec2::new(-38.0, -14.0), 7.0),
        ("hypericum", c + Vec2::new(-20.0, 34.0), 6.0),
        ("achillea", c + Vec2::new(36.0, -20.0), 6.0),
        ("rosa", c + Vec2::new(-46.0, 30.0), 5.0),
        ("symphytum", s.spring + Vec2::new(5.0, 3.0), 4.0),
        ("mentha", s.spring + Vec2::new(-4.0, -4.0), 3.5),
        ("juniperus", g.at(-12.0, 9.0), 7.0),
        ("salvia", g.at(-14.0, -10.0), 6.0),
        ("quercus", (c + g.foot) * 0.5 + Vec2::new(14.0, 0.0), 9.0),
    ]
}

/// Посты: два пастуха-гота у коша, разбойники у грота и в нём.
pub fn posts() -> Vec<(Vec3, crate::level::NpcGroup, f32, f32)> {
    use crate::level::NpcGroup::*;
    let s = sites();
    let g = s.grotto;
    let on = |p: Vec2| Vec3::new(p.x, ground_y(p.x, p.y), p.y);
    // Лицом наружу от грота (yaw: 0 — на −Z).
    let out = -g.dir;
    let yaw = (-out.x).atan2(-out.y);
    let fl = |p: Vec2| Vec3::new(p.x, g.floor, p.y);
    let top = |p: Vec2| Vec3::new(p.x, cell_top(p).max(g.floor), p.y);
    vec![
        (on(s.camp + Vec2::new(8.0, -9.0)), Goths, 0.0, 14.0),
        (on(s.camp + Vec2::new(-12.0, 6.0)), Goths, std::f32::consts::PI, 10.0),
        (top(g.at(-4.0, -2.5)), Brigands, yaw, 6.0),
        (top(g.at(-3.0, 2.8)), Brigands, yaw, 6.0),
        (fl(g.at(7.5, 1.5)), Brigands, yaw, 2.0),
    ]
}

// ---------------------------------------------------------------------------
// Постройка
// ---------------------------------------------------------------------------

pub fn highlands(b: &mut Builder, roads: &[Vec<Vec2>]) {
    camp(b);
    spring(b);
    grotto(b);
    forest(b, roads);
}

/// Тропа: натоптанная лента по рельефу.
pub fn trail(b: &mut Builder, pts: &[Vec2]) {
    super::mines::path_ribbon(b, pts, 1.6);
}

// ---------------- кош ----------------

fn camp(b: &mut Builder) {
    let c = sites().camp;
    let mut rng = Rng::new(7301);
    // Хижина пастухов: сухая кладка, низкая дверь на юг (от ветра с плато), солома на жердях.
    let hut = Rect::new(c.x - 6.5, c.y - 12.0, c.x + 0.5, c.y - 6.5);
    let y = terrace(b, hut.inset(-1.0), Mat::Rubble);
    b.placed(Vec3::new(hut.cx(), y, hut.cz()), |t| {
        let r = Rect::new(-hut.w() / 2.0, -hut.d() / 2.0, hut.w() / 2.0, hut.d() / 2.0);
        let (h, wt) = (2.0, 0.6);
        let door = [Op::Door { s: 0.0, w: 1.0, h: 1.75 }];
        let none: [Op; 0] = [];
        kit::wall_x(t, r.x0, r.x1, r.z0 + wt / 2.0, wt, h, &none, Mat::Rubble);
        kit::wall_x(t, r.x0, r.x1, r.z1 - wt / 2.0, wt, h, &door, Mat::Rubble);
        kit::wall_z(t, r.z0 + wt, r.z1 - wt, r.x0 + wt / 2.0, wt, h, &none, Mat::Rubble);
        kit::wall_z(t, r.z0 + wt, r.z1 - wt, r.x1 - wt / 2.0, wt, h, &none, Mat::Rubble);
        thatch_roof(t, r, h, 1.6, true, 0.5, Mat::Rubble);
        t.decor([r.x0 + wt, 0.0, r.z0 + wt], [r.x1 - wt, 0.03, r.z1 - wt], Mat::Earth);
        // Внутри: очаг, лежанки на нарах, ларь, кадка для брынзы, сыр сушится на полке.
        kit::hearth(t, Vec3::new(0.4, 0.0, 0.2));
        let mut rr = Rng::new(7302);
        super::super::interior::bed(t, Rect::new(r.x0 + wt + 0.05, r.z0 + wt + 0.05, r.x0 + wt + 2.0, r.z0 + wt + 0.95), &mut rr);
        super::super::interior::bed(t, Rect::new(r.x1 - wt - 2.0, r.z0 + wt + 0.05, r.x1 - wt - 0.05, r.z0 + wt + 0.95), &mut rr);
        t.loot(L::Household);
        super::super::interior::chest(t, Rect::new(r.x0 + wt + 0.1, r.z1 - wt - 0.75, r.x0 + wt + 1.1, r.z1 - wt - 0.15), "Ларь пастухов");
        kit::pithos(t, Vec3::new(r.x1 - wt - 0.5, 0.0, r.z1 - wt - 0.55), 0.9, 0.15);
        let shelf = Rect::new(r.x1 - wt - 1.6, -0.2, r.x1 - wt - 0.05, 0.25);
        t.decor(shelf.min3(1.2).to_array(), shelf.max3(1.25).to_array(), Mat::Wood);
        for k in 0..4 {
            let p = Vec3::new(shelf.x0 + 0.2 + k as f32 * 0.38, 1.25, shelf.cz());
            t.mesh(Mat::Limestone).lathe(p, &[(0.0, 0.0), (0.14, 0.0), (0.15, 0.05), (0.13, 0.1), (0.0, 0.11)], 10);
        }
    });
    // Загон для овец: круг сухой кладки с проходом, внутри — стог.
    let pen = c + Vec2::new(14.0, 4.0);
    let r = 9.0;
    let n = 14;
    for k in 0..n {
        if k == 9 || k == 10 {
            continue;
        }
        let a0 = k as f32 / n as f32 * TAU;
        let a1 = (k + 1) as f32 / n as f32 * TAU;
        b.at(pen.x, pen.y);
        low_wall(b, pen + Vec2::from_angle(a0) * r, pen + Vec2::from_angle(a1) * r);
    }
    let hay = g(pen.x + 3.5, pen.y - 3.0);
    b.roof(Mat::Thatch, |m| m.lathe(hay - Vec3::Y * 0.2, &[(1.2, 0.0), (1.3, 0.7), (0.9, 1.6), (0.0, 2.1)], 10));
    kit::hidden(b, hay - Vec3::new(0.9, 0.2, 0.9), hay + Vec3::new(0.9, 1.8, 0.9));
    // Плетёные «косары» вдоль стенки: ясли для сена.
    let manger = g(pen.x - 5.0, pen.y + 2.0);
    b.decor((manger + Vec3::new(-0.3, 0.0, -1.6)).to_array(), (manger + Vec3::new(0.3, 0.7, 1.6)).to_array(), Mat::Wood);
    // Кострище с брёвнами-сиденьями, поленница, котёл на треноге.
    let fire = g(c.x - 2.0, c.y + 3.0);
    campfire(b, fire);
    let tri = fire;
    {
        let m = b.mesh(Mat::Wood);
        for k in 0..3 {
            let a = k as f32 / 3.0 * TAU + 0.3;
            m.beam(tri + Vec3::new(a.cos() * 0.9, -0.05, a.sin() * 0.9), tri + Vec3::Y * 1.7, 0.03, 0.03, Vec3::Y);
        }
    }
    b.mesh(Mat::Iron).lathe(tri + Vec3::Y * 0.65, &[(0.0, 0.0), (0.22, 0.02), (0.3, 0.2), (0.28, 0.42), (0.26, 0.44)], 12);
    let wp = g(c.x - 9.0, c.y - 4.0);
    b.placed(wp, |t| kit::woodpile(t, Rect::new(-1.3, -0.4, 1.3, 0.4), 0.85));
    // Пастушьи посохи у стены хижины.
    let staff = g(hut.x1 + 0.3, hut.z1 - 0.8);
    for k in 0..2 {
        let f = staff + Vec3::new(0.0, 0.0, k as f32 * 0.35);
        b.mesh(Mat::Wood).beam(f, f + Vec3::new(0.25, 1.5, 0.0), 0.022, 0.022, Vec3::X);
    }
    // Камни-сиденья и редкие валуны по краю поляны.
    for _ in 0..7 {
        let a = rng.range(0.0, TAU);
        let p = c + Vec2::from_angle(a) * rng.range(25.0, 32.0);
        super::rock(b, p, rng.range(0.6, 1.4), &mut rng);
    }
}

// ---------------- ключ ----------------

fn spring(b: &mut Builder) {
    let s = sites().spring;
    let p = g(s.x, s.y);
    b.at(p.x, p.z);
    // Выдолбленная колода-поилка, в неё по жёлобу течёт ключ из-под камня.
    let dir = (sites().camp - s).normalize();
    let side = Vec2::new(-dir.y, dir.x);
    let a = p + Vec3::new(side.x, 0.0, side.y) * 1.6;
    let c = p - Vec3::new(side.x, 0.0, side.y) * 1.6;
    let yb = ground_y(a.x, a.z).min(ground_y(c.x, c.z));
    let lo = a.min(c) - Vec3::new(0.35, 0.0, 0.35);
    let hi = a.max(c) + Vec3::new(0.35, 0.0, 0.35);
    b.solid([lo.x, yb - 0.3, lo.z], [hi.x, yb + 0.45, hi.z], Mat::Wood);
    b.mesh(Mat::Water).disc(Vec3::new(p.x, yb + 0.46, p.z), 0.4, 10, true);
    let stone = g(s.x + dir.x * 1.6, s.y + dir.y * 1.6);
    b.plant(PlantKind::Rock, stone - Vec3::Y * 0.3, 1.5, 0.8, 21);
    let spout = stone + Vec3::new(-dir.x * 0.6, 0.55, -dir.y * 0.6);
    b.mesh(Mat::Wood).beam(spout, Vec3::new(p.x, yb + 0.7, p.z) + Vec3::new(dir.x, 0.0, dir.y) * 0.3, 0.06, 0.03, Vec3::Y);
    b.mesh(Mat::Water).beam(Vec3::new(p.x, yb + 0.68, p.z) + Vec3::new(dir.x, 0.0, dir.y) * 0.3, Vec3::new(p.x, yb + 0.46, p.z), 0.025, 0.025, Vec3::X);
}

// ---------------- грот ----------------

/// Грот в обрыве. Земля над ним вырезается «дырой» (как над штольней рудника): поверхность
/// в дыре — своя, по той же высоте, что вокруг, но без кусков, попавших в полость устья;
/// твердь — невидимые столбы, над залом — от свода вверх. Стены и свод — неровная труба из скалы.
fn grotto(b: &mut Builder) {
    let g = sites().grotto;
    let fl = g.floor;
    // Дыра: охват полости с запасом, по сетке рельефа (6 м).
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for t in [-4.0f32, GROTTO_LEN + 1.0] {
        for s in [-1.0f32, 1.0] {
            let p = g.at(t, s * (Grotto::half_w(7.0) + 1.0));
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let snap_lo = |v: f32| -WORLD_HALF + ((v + WORLD_HALF) / terrain::CELL).floor() * terrain::CELL;
    let snap_hi = |v: f32| -WORLD_HALF + ((v + WORLD_HALF) / terrain::CELL).ceil() * terrain::CELL;
    let hole = Rect::new(snap_lo(lo.x), snap_lo(lo.y), snap_hi(hi.x), snap_hi(hi.y));
    b.holes.push((Vec2::new(hole.x0, hole.z0), Vec2::new(hole.x1, hole.z1)));
    let surf = |x: f32, z: f32| terrain::plain_y(x, z);
    // Поверхность склона над гротом. Карта высот не умеет навесов, поэтому над полостью
    // склон поднят выше свода («крыша» устья), а у подошвы остаётся землёй площадки; грани
    // между тем и другим выброшены — это и есть устье. Нижнюю часть навеса даёт сама арка
    // (у устья она двусторонняя), стыки закрывают глыбы.
    #[derive(Clone, Copy, PartialEq)]
    enum V {
        Out,
        Low,
        Lifted,
    }
    let classify = |x: f32, z: f32| {
        let y = surf(x, z);
        let (t, s) = g.local(Vec2::new(x, z));
        if t > -3.0 && t < GROTTO_LEN + 0.5 && s.abs() < Grotto::half_w(t.clamp(0.0, GROTTO_LEN)) + 0.45 {
            if y < fl + 0.5 {
                (y, V::Low)
            } else {
                (y.max(fl + Grotto::height(t.clamp(0.0, GROTTO_LEN)) + 0.6), V::Lifted)
            }
        } else {
            (y, V::Out)
        }
    };
    let step = 0.75;
    let nx = (hole.w() / step).round() as usize;
    let nz = (hole.d() / step).round() as usize;
    b.at(hole.cx(), hole.cz());
    {
        let grid: Vec<(Vec3, V)> = (0..=nz)
            .flat_map(|j| (0..=nx).map(move |i| (i, j)))
            .map(|(i, j)| {
                let (x, z) = (hole.x0 + i as f32 * step, hole.z0 + j as f32 * step);
                let (y, v) = classify(x, z);
                (Vec3::new(x, y, z), v)
            })
            .collect();
        let at = |i: usize, j: usize| grid[j.min(nz) * (nx + 1) + i.min(nx)].0;
        let m = b.mesh(Mat::Terrain);
        let mut ids = Vec::with_capacity(grid.len());
        for j in 0..=nz {
            for i in 0..=nx {
                let p = at(i, j);
                let dx = at(i + 1, j) - at(i.saturating_sub(1), j);
                let dz = at(i, j + 1) - at(i, j.saturating_sub(1));
                let mut n = dz.cross(dx).normalize_or_zero();
                if n.y < 0.0 {
                    n = -n;
                }
                ids.push(m.vert(p, n, Vec2::new(p.x, p.z)));
            }
        }
        for j in 0..nz {
            for i in 0..nx {
                let c = [j * (nx + 1) + i, j * (nx + 1) + i + 1, (j + 1) * (nx + 1) + i + 1, (j + 1) * (nx + 1) + i];
                let low = c.iter().any(|&k| grid[k].1 == V::Low);
                let high = c.iter().any(|&k| grid[k].0.y > fl + 1.2);
                if low && high {
                    continue;
                }
                m.index_tri(ids[c[0]], ids[c[2]], ids[c[1]]);
                m.index_tri(ids[c[0]], ids[c[3]], ids[c[2]]);
            }
        }
    }
    // Твердь: столбы 1×1 м. В полости — пол и камень над сводом, вне её — до поверхности.
    let cs = 1.0;
    let (cx, cz) = ((hole.w() / cs).round() as usize, (hole.d() / cs).round() as usize);
    for j in 0..cz {
        for i in 0..cx {
            let (xa, za) = (hole.x0 + i as f32 * cs, hole.z0 + j as f32 * cs);
            let (xb, zb) = (xa + cs, za + cs);
            let top = [surf(xa, za), surf(xb, za), surf(xa, zb), surf(xb, zb)].into_iter().fold(f32::MIN, f32::max);
            let mid = Vec2::new(xa + cs / 2.0, za + cs / 2.0);
            let (t, s) = g.local(mid);
            let open = t > -0.5 && t < GROTTO_LEN - 0.4 && s.abs() + 0.35 < Grotto::half_w(t.max(0.0));
            if open {
                b.invisible.push(Aabb::new(Vec3::new(xa, fl - 1.0, za), Vec3::new(xb, fl, zb)));
                let roof = fl + Grotto::height(t.max(0.0));
                if top > roof {
                    b.invisible.push(Aabb::new(Vec3::new(xa, roof, za), Vec3::new(xb, top, zb)));
                }
            } else {
                b.invisible.push(Aabb::new(Vec3::new(xa, top.min(fl) - 1.0, za), Vec3::new(xb, top, zb)));
            }
        }
    }
    // Скала: кольца сечения через 1 м, каждое — арка из 12 точек с неровностью камня.
    let mut rings: Vec<f32> = (0..=((GROTTO_LEN + 2.5) as usize)).map(|k| -2.5 + k as f32).collect();
    // Последнее кольцо — ровно у дальней стены, без щели.
    if *rings.last().unwrap() < GROTTO_LEN - 0.01 {
        rings.push(GROTTO_LEN);
    }
    let arc = 12;
    let point = |t: f32, k: usize| {
        let a = k as f32 / arc as f32 * PI;
        let (w, h) = (Grotto::half_w(t.clamp(0.0, GROTTO_LEN)), Grotto::height(t.clamp(0.0, GROTTO_LEN)));
        let c = a.cos();
        let lat = c.signum() * c.abs().powf(0.55) * w;
        let y = a.sin().max(0.0).powf(0.7) * h;
        // Камень неровный: стены то выпирают, то уходят в ниши, свод — буграми.
        let q = g.at(t, lat);
        let bump = (terrain::fbm(q.x * 0.9 + y * 0.7, q.y * 0.9 - y * 0.5, 811) - 0.47) * 0.8;
        let p = g.at(t, lat * (1.0 + bump * 0.22));
        Vec3::new(p.x, fl + y * (1.0 + bump * 0.12) - 0.15 * (1.0 - a.sin()), p.y)
    };
    let center = |t: f32| {
        let p = g.at(t, 0.0);
        Vec3::new(p.x, fl + Grotto::height(t.clamp(0.0, GROTTO_LEN)) * 0.45, p.y)
    };
    {
        let m = b.mesh(Mat::Bedrock);
        for w in rings.windows(2) {
            for k in 0..arc {
                let v = [point(w[0], k), point(w[0], k + 1), point(w[1], k + 1), point(w[1], k)];
                let mid = (v[0] + v[2]) * 0.5;
                m.quad(v, center((w[0] + w[1]) * 0.5) - mid);
                // У устья свод может выходить из склона навесом — его видно и снаружи.
                if w[0] < 3.0 {
                    m.quad(v, mid - center((w[0] + w[1]) * 0.5));
                }
            }
        }
        // Дальняя стена.
        let t = GROTTO_LEN;
        let c = center(t) + Vec3::new(g.dir.x, 0.0, g.dir.y) * 0.4;
        for k in 0..arc {
            m.tri(point(t, k), point(t, k + 1), c, -Vec3::new(g.dir.x, 0.0, g.dir.y));
        }
        // И низ стены — от пола до середины.
        m.tri(point(t, arc), point(t, 0), c, -Vec3::new(g.dir.x, 0.0, g.dir.y));
    }
    // Пол: утоптанная земля с золой у костра.
    {
        let m = b.mesh(Mat::Earth);
        for w in rings.windows(2) {
            let (a, c) = (w[0].max(-1.5), w[1].max(-1.5));
            if c <= a {
                continue;
            }
            let (wa, wc) = (Grotto::half_w(a.max(0.0)) + 0.3, Grotto::half_w(c.max(0.0)) + 0.3);
            let p = |t: f32, s: f32| {
                let q = g.at(t, s);
                Vec3::new(q.x, fl + 0.02, q.y)
            };
            m.quad([p(a, -wa), p(a, wa), p(c, wc), p(c, -wc)], Vec3::Y);
        }
    }
    // Глыбы вокруг устья скрывают стык скалы с землёй склона.
    let mut rng = Rng::new(7311);
    for k in 0..9 {
        let a = k as f32 / 8.0 * PI;
        let w = Grotto::half_w(0.0) + 0.6;
        let h = Grotto::height(0.0) + 0.3;
        let s = a.cos() * w;
        let y = a.sin() * h;
        let p = g.at(-0.3 + rng.range(-0.3, 0.3), s);
        let size = rng.range(1.4, 2.2);
        b.plant(PlantKind::Rock, Vec3::new(p.x, fl + y - size * 0.45, p.y), size, rng.range(0.0, TAU), rng.int(64) as u32);
    }
    // И по бокам устья, где склон сходится с аркой.
    for (t, side) in [(-1.2f32, -1.0f32), (-1.2, 1.0), (0.6, -1.0), (0.6, 1.0), (2.0, -1.0), (2.0, 1.0)] {
        let p = g.at(t, side * (Grotto::half_w(t.max(0.0)) + rng.range(0.9, 1.4)));
        let size = rng.range(1.3, 2.0);
        b.plant(PlantKind::Rock, Vec3::new(p.x, surf(p.x, p.y).max(fl) - size * 0.3, p.y), size, rng.range(0.0, TAU), rng.int(64) as u32);
    }
    // Логово: кострище в зале, лежанки из соломы и шкур, сундук с добычей,
    // амфоры с краденым вином, кости и черепки.
    let hall = g.hall();
    let fire = Vec3::new(hall.x, fl, hall.z);
    kit::hearth(b, fire);
    b.lamp(fire + Vec3::Y * 0.9, Color::srgb(1.0, 0.58, 0.28), 6_000.0, 9.0);
    b.torch(fire.x, fire.y + 0.45, fire.z, Vec3::ZERO);
    let at = |t: f32, s: f32| {
        let p = g.at(t, s);
        Vec3::new(p.x, fl, p.y)
    };
    for (t, s) in [(10.5f32, -2.4f32), (12.5, 1.9), (6.2, 2.6)] {
        let c = at(t, s);
        b.mesh(Mat::Canvas).cuboid(c + Vec3::new(-0.7, 0.0, -0.7), c + Vec3::new(0.7, 0.18, 0.7));
        b.mesh(Mat::Rug).cuboid(c + Vec3::new(-0.55, 0.18, -0.5), c + Vec3::new(0.55, 0.22, 0.5));
    }
    b.loot(L::Wealthy);
    let ch = at(13.6, -0.8);
    kit::chest_body(b, ch, true);
    b.container(ch + Vec3::Y * 0.7, "Сундук разбойников");
    b.loot(L::Household);
    for k in 0..3 {
        kit::amphora_lying(b, at(11.0 + k as f32 * 0.6, 3.0) + Vec3::Y * 0.12, false, 1.0);
    }
    for k in 0..6 {
        let p = at(4.0 + rng.range(0.0, 8.0), rng.range(-2.0, 2.0)) + Vec3::Y * 0.03;
        let s = rng.range(0.05, 0.12);
        b.mesh(if k % 2 == 0 { Mat::Limestone } else { Mat::Ceramic }).cuboid(p - Vec3::new(s, 0.0, s * 0.4), p + Vec3::new(s, s * 0.5, s * 0.4));
    }
    // Факел у входа — видно издали, что в скале кто-то живёт.
    let m = at(0.4, Grotto::half_w(0.4) - 0.2);
    b.torch(m.x, fl + 1.7, m.z, Vec3::new(-g.side().x, 0.0, -g.side().y));
    b.keep_clear.push(g.mouth());
}

// ---------------- лес ----------------

/// Лес и камни на горах. Свой счётчик случайных чисел: зелень плато не сдвигается.
fn forest(b: &mut Builder, roads: &[Vec<Vec2>]) {
    let clear = clearings();
    let g = sites().grotto;
    let blocked = |p: Vec2, pad: f32| {
        p.x.abs() > WORLD_HALF - 12.0
            || p.y > mountains::MESH_SOUTH - 12.0
            || super::road_distance(roads, p) < 2.0 + pad
            || clear.iter().any(|(c, r)| p.distance(*c) < *r)
            || g.inside(p, 4.0)
    };
    let steep = |p: Vec2| {
        let y = ground_y(p.x, p.y);
        (ground_y(p.x + 1.5, p.y) - y).abs().max((ground_y(p.x, p.y + 1.5) - y).abs())
    };
    let mut rng = Rng::new(9311);
    let step = 17.0;
    let mut z = WORLD_HALF - 12.0;
    while z < mountains::MESH_SOUTH - 12.0 {
        let mut x = -WORLD_HALF + 10.0;
        while x < WORLD_HALF - 10.0 {
            let p = Vec2::new(x + rng.range(-7.0, 7.0), z + rng.range(-7.0, 7.0));
            x += step;
            let (roll, kind_roll, size) = (rng.f(), rng.f(), rng.f());
            let u = p.dot(MOUNT_L);
            // Ниже — плато со своим редколесьем (wilderness), горный лес — с подножия.
            if u < mountains::FOOT_U0 - 150.0 || blocked(p, 1.5) {
                continue;
            }
            let y = ground_y(p.x, p.y);
            let rise = steep(p);
            let density = terrain::fbm(p.x * 0.006 + 40.0, p.y * 0.006, 91);
            // На ровных макушках — поляны (яйлы), в лощинах и на склонах — лес.
            let open = terrain::smoothstep(0.08, 0.02, rise / 1.5) * terrain::smoothstep(260.0, 380.0, y);
            let chance = (((density - 0.3) * 2.4).clamp(0.15, 0.9)) * (1.0 - 0.85 * open) * terrain::smoothstep(mountains::FOOT_U0 - 150.0, mountains::FOOT_U0 + 200.0, u);
            if roll > chance {
                if rise > 1.2 && roll < chance + 0.25 {
                    super::rock(b, p, 0.9 + size * 2.0, &mut rng);
                }
                continue;
            }
            b.at(p.x, p.y);
            let high = terrain::smoothstep(180.0, 340.0, y);
            let trees = if density > 0.55 { 3 + rng.int(3) } else { 1 + rng.int(3) };
            for k in 0..trees {
                let tp = if k == 0 { p } else { p + Vec2::new(rng.range(-7.5, 7.5), rng.range(-7.5, 7.5)) };
                if k > 0 && (blocked(tp, 1.0) || tp.distance(p) < 2.5) {
                    continue;
                }
                let q = g_at(tp) - Vec3::Y * 0.12;
                let r = steep(tp);
                let roll = rng.f();
                if r > 1.3 {
                    // На обрыве держится только можжевельник.
                    kit::juniper(b, q - Vec3::Y * 0.3, rng.range(1.8, 3.5), &mut rng);
                } else if roll < 0.25 - 0.15 * high {
                    kit::juniper(b, q, rng.range(2.2, 4.5), &mut rng);
                } else if roll < 0.35 + 0.45 * high + 0.1 * kind_roll {
                    // Крымская сосна: выше и по гривам — сосновые боры.
                    kit::pine(b, q, rng.range(9.0, 16.0), &mut rng);
                } else {
                    // Дуб пушистый, ниже и в лощинах; на высоте — бук и граб (их изображает тот же лиственный вид).
                    kit::oak(b, q, rng.range(7.0, 13.0), &mut rng);
                }
            }
            for _ in 0..1 + rng.int(2) {
                let a = rng.range(0.0, TAU);
                let s = p + Vec2::from_angle(a) * rng.range(2.0, 6.0);
                if !blocked(s, 0.5) && steep(s) < 1.2 {
                    kit::shrub(b, g_at(s) - Vec3::Y * 0.05, rng.range(0.9, 2.2));
                }
            }
        }
        z += step;
    }
    // Выходы известняка на крутом: бровки куэст, гребни грив. Тоже свой счётчик.
    let mut rng = Rng::new(9312);
    let step = 7.0;
    let mut z = WORLD_HALF - 12.0;
    while z < mountains::MESH_SOUTH - 12.0 {
        let mut x = -WORLD_HALF + 12.0;
        while x < WORLD_HALF - 12.0 {
            let p = Vec2::new(x + rng.range(-3.0, 3.0), z + rng.range(-3.0, 3.0));
            x += step;
            let (roll, size) = (rng.f(), rng.f());
            if p.dot(MOUNT_L) < mountains::FOOT_U0 {
                continue;
            }
            let rise = steep(p);
            if roll > 0.55 || !(0.9..6.0).contains(&rise) || blocked(p, 0.5) {
                continue;
            }
            super::rock(b, p, 1.0 + size * size * 3.2 + rise.min(3.0) * 0.4, &mut rng);
        }
        z += step;
    }
}

fn g_at(p: Vec2) -> Vec3 {
    Vec3::new(p.x, ground_y(p.x, p.y), p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Кош — в горах и на ровном, грот — в скале, тропы ведут к обоим и нигде не лезут на кручу.
    #[test]
    fn trails_reach_camp_and_grotto_gently() {
        let s = sites();
        let camp_y = ground_y(s.camp.x, s.camp.y);
        println!("кош {:?} на {camp_y:.0} м, ключ {:?}, грот {:?} вглубь {:?} пол {:.0} м", s.camp, s.spring, s.grotto.foot, s.grotto.dir, s.grotto.floor);
        println!("тропы {:?}", trails());
        assert!(s.camp.dot(MOUNT_L) > mountains::FOOT_U0 + 400.0, "кош не в горах: {:?}", s.camp);
        assert!(camp_y > 150.0, "кош низко: {camp_y}");
        assert!(s.camp.y < WORLD_SOUTH - 100.0 && s.camp.x.abs() < 800.0);
        let t = trails();
        assert_eq!(t.len(), 2, "не нашлось троп");
        assert!(t[0].last().unwrap().distance(s.camp) < 30.0);
        assert!(t[1].last().unwrap().distance(s.grotto.foot) < 12.0);
        for trail in t {
            // Уклон — на шести метрах пути (шаг сетки, по которой искалась тропа): мелкие кочки
            // и камешки под ногами не в счёт.
            let mut pts = Vec::new();
            for w in trail.windows(2) {
                let n = (w[0].distance(w[1]) / 0.5).ceil().max(1.0) as usize;
                pts.extend((0..n).map(|k| w[0].lerp(w[1], k as f32 / n as f32)));
            }
            pts.push(*trail.last().unwrap());
            let mut worst = 0.0f32;
            let (mut i, mut j, mut along) = (0, 0, 0.0f32);
            while j + 1 < pts.len() {
                along += pts[j].distance(pts[j + 1]);
                j += 1;
                while along > 6.0 {
                    along -= pts[i].distance(pts[i + 1]);
                    i += 1;
                }
                if along > 4.0 {
                    let (a, c) = (pts[i], pts[j]);
                    worst = worst.max((ground_y(c.x, c.y) - ground_y(a.x, a.y)).abs() / along);
                }
            }
            println!("тропа: {} точек, самый крутой кусок {worst:.2}", trail.len());
            assert!(worst < 0.6, "тропа лезет на кручу: уклон {worst}");
        }
        // Над сводом грота — камень.
        let g = s.grotto;
        for t in [0.0f32, 1.0, 2.0, 3.0] {
            let p = g.at(t, 0.0);
            println!("над устьем на глубине {t}: {:.1} м над полом", ground_y(p.x, p.y) - g.floor);
        }
        assert!(s.grotto.foot.distance(s.camp + Vec2::new(200.0, 0.0)) > 1.0, "грот не нашёлся");
        for t in [2.0f32, 5.0, 10.0, 14.0] {
            let p = g.at(t, 0.0);
            assert!(ground_y(p.x, p.y) > g.floor + Grotto::height(t) + 1.0, "свод грота наружу на глубине {t}");
        }
    }

    /// Ни одной вершины NaN: меш с NaN получает рамку NaN и отсекается целиком (так однажды
    /// пропал весь свод грота — `sin(π)` чуть меньше нуля, а дробная степень от него — NaN).
    #[test]
    fn level_meshes_are_finite() {
        let layout = crate::level::layout();
        for ((mat, chunk), mb) in &layout.decor {
            assert!(mb.triangles().flatten().all(|p| p.is_finite()), "NaN в меше {mat:?} {chunk:?}");
        }
    }
}
