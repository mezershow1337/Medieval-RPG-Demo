//! Село Святого Фоки — греческое село херсонской хоры на плато к югу от города, в получасе
//! ходьбы от главных ворот. Поселение земледельцев, каких на Гераклейском полуострове было
//! много: дворы с каменными домами под черепицей вдоль сельской улицы, площадь с колодцем
//! и старым дубом, однонефная церковь св. Фоки (покровителя садовников и мореходов, его чтили
//! по всему Понту), корчма, гончарная печь, ток, пасека, огороды, пашня со снопами и загон.
//!
//! Улица села идёт с запада на восток по `VILLAGE.z`; на западе в неё входит ответвление
//! главной дороги, на востоке тропа уходит к дороге от южных ворот.

use super::*;
use super::kit::Side;
use crate::level::places::PHOCAS_VILLAGE as VILLAGE;

/// Полуширина сельской улицы (по ней же идёт дорога).
const STREET: f32 = 3.0;

/// Площадь у колодца.
pub fn square() -> Rect {
    Rect::new(VILLAGE.x - 11.0, VILLAGE.z - 9.0, VILLAGE.x + 11.0, VILLAGE.z + 9.0)
}

/// Корчма на южной стороне площади (по наружным граням стен).
pub fn tavern_rect() -> Rect {
    Rect::new(VILLAGE.x - 6.5, VILLAGE.z + 11.0, VILLAGE.x + 6.5, VILLAGE.z + 19.0)
}

/// Площадка под корчмой (с навесом перед входом).
fn tavern_pad() -> Rect {
    Rect::new(VILLAGE.x - 7.5, VILLAGE.z + 8.6, VILLAGE.x + 7.5, VILLAGE.z + 20.0)
}

pub fn tavern_floor() -> f32 {
    terrace_levels(tavern_pad()).1
}

/// Где стоит корчмарка: за стойкой у задней стены, лицом ко входу.
pub fn tavern_point() -> Vec3 {
    let r = tavern_rect();
    Vec3::new(r.cx() + 2.6, tavern_floor(), r.z1 - 1.9)
}

/// Прилавок старосты у площади: середина стола и куда он обращён (к улице).
pub fn stall_counter() -> (Vec3, Vec3) {
    let x = VILLAGE.x + 6.0;
    let z = VILLAGE.z - 5.6;
    (Vec3::new(x, ground_y(x, z), z), Vec3::Z)
}

/// Где стоит староста: за прилавком.
pub fn stall_point() -> Vec3 {
    let (c, f) = stall_counter();
    let p = c - f * 1.05;
    Vec3::new(p.x, ground_y(p.x, p.z), p.z)
}

/// Двор гончара на восточном краю села.
pub fn pottery_rect() -> Rect {
    Rect::new(VILLAGE.x + 40.0, VILLAGE.z + 5.0, VILLAGE.x + 56.0, VILLAGE.z + 19.0)
}

/// Навес гончара с кругом (по столбам).
fn potter_shed() -> Rect {
    let r = pottery_rect();
    Rect::new(r.x1 - 7.5, r.z0 + 1.0, r.x1 - 1.0, r.z0 + 6.5)
}

/// Где стоит гончар: у круга под навесом, со стороны улицы.
pub fn potter_point() -> Vec3 {
    let sh = potter_shed();
    Vec3::new(sh.cx() + 1.75, terrace_levels(sh.inset(-0.6)).1, sh.cz() - 1.1)
}

/// Дворы села: участок (по наружным граням ограды) и сторона улицы (`N` — двор выходит на север).
pub fn plots() -> Vec<(Rect, Side)> {
    let c = VILLAGE;
    let north = |x0: f32| (Rect::new(c.x + x0, c.z - STREET - 1.5 - 13.0, c.x + x0 + 10.0, c.z - STREET - 1.5), Side::S);
    let south = |x0: f32| (Rect::new(c.x + x0, c.z + STREET + 1.5, c.x + x0 + 10.0, c.z + STREET + 1.5 + 13.0), Side::N);
    vec![
        north(-49.0),
        north(-37.0),
        north(-25.0),
        north(14.0),
        north(26.0),
        south(-49.0),
        south(-37.0),
        south(-25.0),
        south(14.0),
        south(26.0),
    ]
}

/// Какой двор пустует и продаётся.
const VACANT: usize = 6;

/// Дороги села: ответвление от главной дороги и тропа на восток к дороге от южных ворот.
pub fn village_roads() -> Vec<Vec<Vec2>> {
    let c = VILLAGE;
    let v = Vec2::new;
    vec![vec![v(-110.4, 285.0), v(-80.0, 291.0), v(c.x - 62.0, c.z), v(c.x + 60.0, c.z), v(c.x + 92.0, c.z - 6.0), v(150.0, 275.0), v(192.0, 262.0)]]
}

/// Где в округе не растут деревья (село, огороды, пашня).
pub fn clearing() -> (Vec2, f32) {
    (Vec2::new(VILLAGE.x - 2.0, VILLAGE.z + 8.0), 74.0)
}

pub fn village(b: &mut Builder) {
    let mut rng = Rng::new(0xF0CA);
    square_and_well(b, &mut rng);
    // Церковь св. Фоки — за площадью, входом на запад.
    memorial_chapel(b, Vec2::new(VILLAGE.x - 1.5, VILLAGE.z - 18.0));
    tavern(b, &mut rng);
    for (k, (plot, front)) in plots().into_iter().enumerate() {
        farmhouse(b, plot, front, k, &mut rng);
    }
    pottery(b, &mut rng);
    threshing_floor(b);
    apiary(b, &mut rng);
    gardens(b, &mut rng);
    fields(b, &mut rng);
    sheep_fold(b);
    vineyard(b, &mut rng);
    greenery(b, &mut rng);
}

/// Площадка под постройку, как [`terrace`], но с частыми низкими ступенями (по 0,3 м):
/// на склоне нижние ступени уходят в землю, и подъём остаётся по силам с любой стороны.
fn pad(b: &mut Builder, r: Rect, mat: Mat) -> f32 {
    let (lo, y) = terrace_levels(r);
    b.at(r.cx(), r.cz());
    b.solid([r.x0, lo - 0.8, r.z0], [r.x1, y, r.z1], mat);
    let levels = (((y - lo) / 0.3).ceil() as usize).saturating_sub(1).min(12);
    // Ступени — утоптанная земля откоса (кладка — только сама площадка).
    for i in 1..=levels {
        let e = 0.45 * i as f32;
        b.solid([r.x0 - e, lo - 0.8, r.z0 - e], [r.x1 + e, y - 0.3 * i as f32, r.z1 + e], Mat::Earth);
    }
    y
}

// ---------------- зелень ----------------

/// Сад, кипарисы у церкви, кусты по межам и вдоль дороги, деревья вокруг села:
/// село стоит в зелени, а не на голом плато.
fn greenery(b: &mut Builder, rng: &mut Rng) {
    let c = VILLAGE;
    // Фруктовый сад за церковью (груши, яблони — лиственный вид) рядами.
    for i in 0..6 {
        for j in 0..3 {
            let x = c.x - 13.0 + i as f32 * 5.0 + rng.range(-0.6, 0.6);
            let z = c.z - 30.0 - j as f32 * 5.5 + rng.range(-0.6, 0.6);
            b.at(x, z);
            kit::oak(b, g(x, z) - Vec3::Y * 0.1, rng.range(4.0, 5.5), rng);
        }
    }
    // Кипарисы по сторонам от входа в церковь.
    for dz in [-3.6f32, 3.6] {
        let p = g(c.x - 9.0, c.z - 18.0 + dz);
        b.at(p.x, p.z);
        kit::cypress(b, p - Vec3::Y * 0.1, 8.5);
    }
    // Деревья и кусты в просветах между дворами вдоль улицы.
    for x in [-38.0f32, -26.0, 13.0, 25.0, 37.0] {
        for (z, k) in [(-(STREET + 2.4), 0), (STREET + 2.4, 1)] {
            let p = g(c.x + x, c.z + z);
            b.at(p.x, p.z);
            if (x as i32 + k).rem_euclid(2) == 0 {
                kit::fig_tree(b, p - Vec3::Y * 0.05, rng.range(3.5, 4.5), rng);
            } else {
                kit::shrub(b, p - Vec3::Y * 0.05, rng.range(1.2, 1.8));
            }
        }
    }
    // Кусты по межам пашни.
    let mut x = c.x - 56.0;
    while x <= c.x + 40.0 {
        let mut z = c.z + 25.0;
        while z < c.z + 50.0 {
            if rng.chance(0.45) {
                let p = g(x + rng.range(-0.3, 0.3), z);
                b.at(p.x, p.z);
                kit::shrub(b, p - Vec3::Y * 0.05, rng.range(0.9, 1.6));
            }
            z += rng.range(4.0, 7.0);
        }
        x += 12.0;
    }
    // Вдоль дороги в село — дикие груши и шиповник.
    let road = &village_roads()[0];
    for w in road.windows(2) {
        let (a, e) = (w[0], w[1]);
        let len = a.distance(e);
        let dir = (e - a) / len;
        let side = Vec2::new(-dir.y, dir.x);
        let mut t = 6.0;
        while t < len - 4.0 {
            let q = a + dir * t;
            let inside = (q.x - c.x).abs() < 62.0 && (q.y - c.z).abs() < 25.0;
            if !inside {
                let s = if rng.chance(0.5) { 1.0 } else { -1.0 };
                let p = q + side * s * rng.range(4.5, 7.0);
                b.at(p.x, p.y);
                if rng.chance(0.35) {
                    kit::oak(b, g(p.x, p.y) - Vec3::Y * 0.1, rng.range(5.0, 7.5), rng);
                } else {
                    kit::shrub(b, g(p.x, p.y) - Vec3::Y * 0.05, rng.range(1.0, 1.9));
                }
            }
            t += rng.range(7.0, 13.0);
        }
    }
    // Кольцо деревьев вокруг села: дубы, можжевельник, кусты — кроме пашни, дорог и построек.
    let roads = super::roads();
    let fields = Rect::new(c.x - 58.0, c.z + 21.0, c.x + 42.0, c.z + 53.0);
    for k in 0..90 {
        let a = k as f32 / 90.0 * TAU + rng.range(-0.03, 0.03);
        let r = rng.range(1.0, 1.35);
        let p = Vec2::new(c.x + 5.0 + a.cos() * 78.0 * r, c.z + 4.0 + a.sin() * 54.0 * r);
        let in_fields = p.x > fields.x0 && p.x < fields.x1 && p.y > fields.z0 && p.y < fields.z1;
        if in_fields || super::road_distance(&roads, p) < 6.0 || rng.chance(0.25) {
            continue;
        }
        b.at(p.x, p.y);
        let q = g(p.x, p.y) - Vec3::Y * 0.1;
        let roll = rng.f();
        if roll < 0.35 {
            kit::oak(b, q, rng.range(6.0, 9.5), rng);
        } else if roll < 0.65 {
            kit::juniper(b, q, rng.range(2.5, 4.5), rng);
        } else {
            kit::shrub(b, q, rng.range(1.0, 2.0));
        }
        if rng.chance(0.5) {
            let s = Vec2::new(p.x + rng.range(-3.0, 3.0), p.y + rng.range(-3.0, 3.0));
            kit::shrub(b, g(s.x, s.y) - Vec3::Y * 0.05, rng.range(0.8, 1.5));
        }
    }
}

// ---------------- площадь ----------------

/// Площадь: утоптанная земля, колодец под воротом с поилкой, старый дуб, скамьи.
fn square_and_well(b: &mut Builder, rng: &mut Rng) {
    let s = square();
    // Утоптанная земля площади (по рельефу, полосами по 2 м).
    let n = (s.w() / 2.0) as usize;
    for i in 0..n {
        for j in 0..(s.d() / 2.0) as usize {
            let (x0, z0) = (s.x0 + i as f32 * 2.0, s.z0 + j as f32 * 2.0);
            let (x1, z1) = (x0 + 2.0, z0 + 2.0);
            b.at(x0, z0);
            let lift = |x: f32, z: f32| Vec3::new(x, ground_y(x, z) + 0.06, z);
            b.mesh(Mat::Earth).quad([lift(x0, z0), lift(x1, z0), lift(x1, z1), lift(x0, z1)], Vec3::Y);
        }
    }
    // Колодец: каменный венец, двускатный навесик на столбах, ворот с ведром.
    let w = g(VILLAGE.x - 3.5, VILLAGE.z + 5.5);
    b.at(w.x, w.z);
    let w = w - Vec3::Y * 0.1;
    b.mesh(Mat::Limestone).lathe(w, &[(0.95, 0.0), (0.95, 0.75), (1.02, 0.8), (1.02, 0.9), (0.7, 0.9), (0.7, 0.3)], 16);
    b.mesh(Mat::Water).disc(w + Vec3::Y * 0.25, 0.7, 14, true);
    kit::hidden(b, w - Vec3::new(0.9, 0.0, 0.9), w + Vec3::new(0.9, 0.9, 0.9));
    {
        let m = b.mesh(Mat::Wood);
        for dx in [-1.1f32, 1.1] {
            m.cuboid(w + Vec3::new(dx - 0.08, 0.0, -0.08), w + Vec3::new(dx + 0.08, 2.1, 0.08));
        }
        // Ворот с рукоятью и верёвка.
        m.beam(w + Vec3::new(-1.15, 1.2, 0.0), w + Vec3::new(1.15, 1.2, 0.0), 0.09, 0.09, Vec3::Y);
        m.beam(w + Vec3::new(1.2, 1.2, 0.0), w + Vec3::new(1.2, 1.2, 0.35), 0.03, 0.03, Vec3::Y);
        m.beam(w + Vec3::new(1.2, 1.2, 0.35), w + Vec3::new(1.4, 1.2, 0.35), 0.03, 0.03, Vec3::Y);
    }
    b.mesh(Mat::Canvas).beam(w + Vec3::new(0.0, 1.15, 0.0), w + Vec3::new(0.0, 0.55, 0.0), 0.015, 0.015, Vec3::X);
    kit::bucket(b, w + Vec3::new(0.0, 0.92, 0.0));
    for dx in [-1.1f32, 1.1] {
        kit::hidden(b, w + Vec3::new(dx - 0.1, 0.0, -0.1), w + Vec3::new(dx + 0.1, 2.1, 0.1));
    }
    kit::gable_roof(b, Rect::new(w.x - 1.3, w.z - 0.75, w.x + 1.3, w.z + 0.75), w.y + 2.1, 0.6, true, 0.15, Mat::Wood);
    // Каменная поилка для скота рядом.
    let t = g(VILLAGE.x - 7.0, VILLAGE.z + 6.5);
    b.solid([t.x - 1.4, t.y - 0.3, t.z - 0.38], [t.x + 1.4, t.y + 0.5, t.z + 0.38], Mat::Limestone);
    b.mesh(Mat::Water).cuboid(Vec3::new(t.x - 1.25, t.y + 0.5, t.z - 0.26), Vec3::new(t.x + 1.25, t.y + 0.51, t.z + 0.26));
    // Старый дуб у церкви: под ним сходятся на сход.
    let o = g(VILLAGE.x - 7.5, VILLAGE.z - 6.0) - Vec3::Y * 0.1;
    kit::oak(b, o, 11.0, rng);
    for (dx, dz) in [(1.8f32, 0.0f32), (-1.8, 0.0)] {
        let p = g(o.x + dx, o.z + dz);
        b.placed(p, |t| kit::bench(t, Rect::new(-0.2, -0.8, 0.2, 0.8)));
    }
    // Прилавок старосты: стол с товаром под холщовым навесом.
    let (c, f) = stall_counter();
    b.at(c.x, c.z);
    b.placed(c, |t| {
        kit::table(t, Rect::new(-1.2, -0.4, 1.2, 0.4), 0.85);
        kit::awning_of(t, Vec3::new(-1.6, 0.0, -1.7), Vec3::new(1.6, 0.0, -1.7), f, 2.6, 2.4, Mat::Canvas);
        let mut rr = Rng::new(77);
        for k in 0..3 {
            super::super::interior::basket(t, Vec3::new(-0.75 + k as f32 * 0.75, 0.85, 0.0), 0.22, 0.25, [Mat::PetalYellow, Mat::Bread, Mat::HerbGreen][k], None);
        }
        kit::amphora(t, Vec3::new(1.7, 0.0, -0.4), 0.9, 1);
        kit::amphora(t, Vec3::new(1.75, 0.0, 0.1), 0.9, 2);
        super::super::interior::sacks(t, Rect::new(-2.4, -1.3, -1.6, -0.5), &mut rr);
    });
}

// ---------------- корчма ----------------

/// Сельская корчма: один зал с очагом и стойкой, пифосы с вином, навес со столами у входа.
fn tavern(b: &mut Builder, rng: &mut Rng) {
    let site = tavern_pad();
    let y = pad(b, site, Mat::Rubble);
    let r = tavern_rect();
    let o = Vec3::new(r.cx(), y, r.cz());
    let seed = rng.int(10_000) as u32;
    b.placed(o, move |t| {
        let mut rng = Rng::new(seed);
        let r = Rect::new(-6.5, -4.0, 6.5, 4.0);
        let (h, wt) = (3.4, 0.65);
        kit::wall_x(t, r.x0, r.x1, r.z0 + wt / 2.0, wt, h, &[Op::Door { s: -1.5, w: 1.4, h: 2.3 }, Op::Window { s: -4.4, w: 0.8, sill: 1.2, top: 2.1 }, Op::Window { s: 2.6, w: 0.8, sill: 1.2, top: 2.1 }], Mat::Rubble);
        kit::wall_x(t, r.x0, r.x1, r.z1 - wt / 2.0, wt, h, &[], Mat::Rubble);
        kit::wall_z(t, r.z0 + wt, r.z1 - wt, r.x0 + wt / 2.0, wt, h, &[Op::Window { s: 0.0, w: 0.7, sill: 1.3, top: 2.1 }], Mat::Rubble);
        kit::wall_z(t, r.z0 + wt, r.z1 - wt, r.x1 - wt / 2.0, wt, h, &[], Mat::Rubble);
        kit::quoins(t, r.x0 + wt / 2.0, r.z0 + wt / 2.0, h, wt);
        kit::quoins(t, r.x1 - wt / 2.0, r.z0 + wt / 2.0, h, wt);
        kit::gable_roof(t, r, h, 1.7, true, 0.45, Mat::Rubble);
        t.decor([r.x0 + wt, 0.0, r.z0 + wt], [r.x1 - wt, 0.04, r.z1 - wt], Mat::Earth);
        // Стойка корчмарки в восточной половине и пифосы за ней.
        kit::table(t, Rect::new(0.6, 0.9, 4.6, 1.5), 1.0);
        for k in 0..3 {
            kit::pithos(t, Vec3::new(1.2 + k as f32 * 1.4, 0.0, 3.0), 1.2, 0.25);
        }
        kit::amphora(t, Vec3::new(5.2, 0.0, 2.9), 1.0, 1);
        kit::amphora(t, Vec3::new(5.4, 0.0, 2.3), 1.0, 2);
        let mut cr = Rng::new(seed ^ 0x55);
        super::super::cozy::bowl_stack(t, Vec3::new(1.0, 1.0, 1.2), 4);
        kit::jug(t, Vec3::new(3.9, 1.0, 1.2), 0.35, Mat::Ceramic);
        super::super::cozy::loaf(t, Vec3::new(2.4, 1.0, 1.2));
        // Очаг у западной стены и котёл на треноге.
        let hp = Vec3::new(-5.0, 0.0, 2.2);
        kit::hearth(t, hp);
        super::super::interior::tripod_cauldron(t, hp);
        super::super::interior::hanging_bunches(t, Vec3::new(-5.6, 2.6, 0.0), Vec3::new(-5.6, 2.6, 2.8), 4);
        // Два стола со скамьями.
        for x in [-4.0f32, 1.2] {
            kit::table(t, Rect::new(x - 0.9, -1.6, x + 0.9, -0.8), 0.78);
            kit::bench(t, Rect::new(x - 0.9, -2.35, x + 0.9, -2.0));
            kit::bench(t, Rect::new(x - 0.9, -0.4, x + 0.9, -0.05));
            super::super::interior::table_setting(t, Rect::new(x - 0.9, -1.6, x + 0.9, -0.8), 0.78, &mut cr);
        }
        super::super::interior::lampstand(t, Vec3::new(-2.0, 0.0, 2.6), true);
        t.lamp(Vec3::new(2.6, 2.6, 0.0), Color::srgb(1.0, 0.72, 0.45), 9_000.0, 9.0);
        extras(t, Rect::new(-6.0, -0.5, -0.5, 3.4), L::Kitchen, &["Ларь корчмарки"], &[(Role::Cook, Vec3::new(-4.0, 0.0, 1.6), hp)], &mut rng);
        // Навес перед входом: столб, кровля из камыша, стол с лавкой.
        let a = Rect::new(-6.6, -5.5, 3.0, -4.0);
        for x in [a.x0 + 0.2, a.x1 - 0.2] {
            t.solid([x - 0.1, 0.0, a.z0 - 0.1], [x + 0.1, 2.4, a.z0 + 0.1], Mat::Wood);
        }
        t.mesh(Mat::Thatch).lean_roof([Vec3::new(a.x0 - 0.2, 2.75, -4.0), Vec3::new(a.x1 + 0.2, 2.75, -4.0)], [Vec3::new(a.x0 - 0.2, 2.35, a.z0 - 0.3), Vec3::new(a.x1 + 0.2, 2.35, a.z0 - 0.3)]);
        t.mesh(Mat::Wood).beam(Vec3::new(a.x0, 2.4, a.z0), Vec3::new(a.x1, 2.4, a.z0), 0.08, 0.08, Vec3::Y);
        kit::table(t, Rect::new(-5.6, -5.3, -3.6, -4.6), 0.78);
        kit::bench(t, Rect::new(-5.6, -5.9, -3.6, -5.6));
        kit::amphora(t, Vec3::new(2.2, 0.0, -4.4), 0.95, 1);
    });
    // Факел у двери: ночью площадь видно издалека.
    b.torch(o.x - 0.4, o.y + 2.2, r.z0 - 0.25, Vec3::NEG_Z);
}

// ---------------- дворы ----------------

/// Крестьянский двор: каменный дом из двух комнат под черепицей в глубине участка,
/// перед ним — двор за невысокой оградой с калиткой на улицу.
fn farmhouse(b: &mut Builder, plot: Rect, front: Side, k: usize, rng: &mut Rng) {
    let y = pad(b, plot.inset(-0.5), Mat::Rubble);
    let o = Vec3::new(plot.cx(), y, plot.cz());
    let seed = rng.int(100_000) as u32;
    let vacant = k == VACANT;
    b.at(o.x, o.z);
    b.placed(o, move |t| {
        let mut rng = Rng::new(seed);
        // s = +1: улица на +Z (двор на юге), s = −1: улица на −Z.
        let s = if front == Side::S { 1.0f32 } else { -1.0 };
        // u — глубина от улицы (0) к задней стене (13): z = s·(6.5 − u).
        let z = |u: f32| s * (6.5 - u);
        let rz = |x0: f32, u0: f32, x1: f32, u1: f32| Rect::new(x0, z(u0).min(z(u1)), x1, z(u0).max(z(u1)));
        let (h, wt) = (3.0, 0.6);
        let door_x = if k % 2 == 0 { -2.0 } else { 2.0 };
        let win_x = -door_x;
        // Дом: передняя стена (к двору) на u = 7, задняя — на 13.
        let house = rz(-4.6, 7.0, 4.6, 13.0);
        let ops_front = [Op::Door { s: door_x, w: 1.1, h: 2.1 }, Op::Window { s: win_x, w: 0.7, sill: 1.2, top: 2.0 }];
        kit::wall_x(t, house.x0, house.x1, z(7.0 + wt / 2.0), wt, h, &ops_front, Mat::Rubble);
        kit::wall_x(t, house.x0, house.x1, z(13.0 - wt / 2.0), wt, h, &[Op::Window { s: win_x, w: 0.5, sill: 1.5, top: 2.1 }], Mat::Rubble);
        let (za, zb) = (z(7.0 + wt).min(z(13.0 - wt)), z(7.0 + wt).max(z(13.0 - wt)));
        kit::wall_z(t, za, zb, house.x0 + wt / 2.0, wt, h, &[], Mat::Rubble);
        kit::wall_z(t, za, zb, house.x1 - wt / 2.0, wt, h, &[], Mat::Rubble);
        // Перегородка с дверью между кухней и спальней.
        let part_x = 0.0;
        let mid = (za + zb) / 2.0;
        kit::wall_z(t, za, zb, part_x, 0.45, h, &[Op::Door { s: mid, w: 1.0, h: 2.0 }], Mat::Rubble);
        kit::quoins(t, house.x0 + wt / 2.0, z(7.0 + wt / 2.0), h, wt);
        kit::quoins(t, house.x1 - wt / 2.0, z(7.0 + wt / 2.0), h, wt);
        kit::gable_roof(t, house, h, 1.5, true, 0.4, Mat::Rubble);
        let floor = Rect::new(house.x0 + wt, za, house.x1 - wt, zb);
        t.decor([floor.x0, 0.0, floor.z0], [floor.x1, 0.03, floor.z1], Mat::Earth);
        // Комната с дверью во двор — кухня, вторая — спальня.
        let (kitchen, bedroom) = {
            let west = Rect::new(floor.x0, floor.z0, part_x - 0.23, floor.z1);
            let east = Rect::new(part_x + 0.23, floor.z0, floor.x1, floor.z1);
            if door_x < 0.0 { (west, east) } else { (east, west) }
        };
        // Двор: ограда по пояс с калиткой посередине.
        let (yh, yt) = (1.4, 0.5);
        kit::wall_x(t, -5.0, -1.2, z(yt / 2.0), yt, yh, &[], Mat::Rubble);
        kit::wall_x(t, 1.2, 5.0, z(yt / 2.0), yt, yh, &[], Mat::Rubble);
        let (ya, yb) = (z(yt).min(z(7.0)), z(yt).max(z(7.0)));
        kit::wall_z(t, ya, yb, -5.0 + yt / 2.0, yt, yh, &[], Mat::Rubble);
        kit::wall_z(t, ya, yb, 5.0 - yt / 2.0, yt, yh, &[], Mat::Rubble);
        let yard = Rect::new(-5.0 + yt + 0.1, ya + 0.1, 5.0 - yt - 0.1, yb - 0.1);
        t.decor([yard.x0, 0.0, yard.z0], [yard.x1, 0.025, yard.z1], Mat::Earth);
        if vacant {
            // Хозяева перебрались в город: пустой дом с ларём — продаётся.
            let cz = (kitchen.z0 + kitchen.z1) / 2.0;
            super::super::interior::chest(t, Rect::new(kitchen.cx() - 0.5, cz - 0.3, kitchen.cx() + 0.5, cz + 0.3), "Ларь");
            let kinds = super::super::estate::GROUND_KINDS.to_vec();
            let rooms = vec![
                super::super::estate::RoomPlan { r: kitchen, floor: 0.0, kinds: kinds.clone() },
                super::super::estate::RoomPlan { r: bedroom, floor: 0.0, kinds },
            ];
            let entrance = Vec3::new(0.0, 0.0, z(-1.2));
            let (nd, nc) = (t.doors.len(), t.containers.len());
            super::super::estate::register(t, "Двор в селе Святого Фоки".into(), crate::towns::TownId::Phocas, Rect::new(-5.2, z(0.0).min(z(13.0)) - 0.2, 5.2, z(0.0).max(z(13.0)) + 0.2), h + 2.0, entrance, 0..nd, 0..nc, 0, rooms, false, None);
            return;
        }
        t.wealth = (k % 3 == 0) as u8;
        let mut people = super::super::interior::Household::new();
        super::super::interior::furnish(t, kitchen, super::super::interior::Kind::Kitchen, &mut people, &mut rng);
        super::super::interior::furnish(t, bedroom, super::super::interior::Kind::Bedroom, &mut people, &mut rng);
        if !people.master {
            let c = yard.center(0.0);
            extras(t, yard.inset(0.6), L::Household, &[], &[(Role::Farmer, c + Vec3::new(1.5, 0.0, 0.0), c)], &mut rng);
        }
        super::super::interior::yard_extras(t, yard, None, &mut people, &mut rng);
        // У двора — пифос под зерно или смоковница.
        if k % 3 == 1 {
            kit::fig_tree(t, Vec3::new(win_x * 1.4, 0.0, z(3.6)), 4.0, &mut rng);
        } else {
            kit::pithos(t, Vec3::new(win_x * 1.7, 0.0, z(5.8)), 1.3, 0.3);
        }
    });
}

// ---------------- гончар ----------------

/// Двор сельского гончара: горновая печь, круг под навесом, сушильные полки, глина и готовая посуда.
fn pottery(b: &mut Builder, rng: &mut Rng) {
    let r = pottery_rect();
    b.at(r.cx(), r.cz());
    // Печь — на земле у западного края двора, топкой на восток.
    let kp = g(r.x0 + 4.0, r.z0 + 8.5) - Vec3::Y * 0.15;
    b.placed(kp, |t| super::super::crafts::kiln(t, Vec3::ZERO, true));
    // Навес с кругом.
    let sh = potter_shed();
    let y = pad(b, sh.inset(-0.6), Mat::Rubble);
    let seed = rng.int(10_000) as u32;
    b.placed(Vec3::new(sh.cx(), y, sh.cz()), move |t| {
        let mut rng = Rng::new(seed);
        let sh = Rect::new(-3.25, -2.75, 3.25, 2.75);
        for (x, z) in [(sh.x0, sh.z0), (sh.x1, sh.z0), (sh.x0, sh.z1), (sh.x1, sh.z1)] {
            t.solid([x - 0.12, 0.0, z - 0.12], [x + 0.12, 2.8, z + 0.12], Mat::Wood);
        }
        thatch_roof(t, sh, 2.8, 1.2, true, 0.35, Mat::Wood);
        let w = Vec3::new(1.75, 0.0, 0.0);
        super::super::interior::potter_wheel(t, w);
        kit::bench(t, Rect::new(w.x - 0.2, 0.6, w.x + 0.2, 1.1));
        kit::shelf(t, Rect::new(sh.x0 + 0.3, sh.z1 - 0.6, sh.x0 + 3.0, sh.z1 - 0.2), 3, &mut rng, Mat::Earth);
        kit::shelf(t, Rect::new(sh.x0 + 0.3, sh.z0 + 0.2, sh.x0 + 2.6, sh.z0 + 0.6), 2, &mut rng, Mat::Ceramic);
        extras(t, Rect::new(sh.x0 + 0.4, -1.4, -0.2, 1.4), L::Workshop, &["Ящик гончара"], &[], &mut rng);
    });
    // Куча глины, корыто с водой, поленница и готовые горшки и амфоры.
    let cl = g(r.cx(), r.z1 - 3.0);
    b.mesh(Mat::Earth).lathe(cl - Vec3::Y * 0.2, &[(2.0, 0.0), (1.5, 0.5), (0.0, 0.75)], 12);
    kit::hidden(b, cl - Vec3::new(1.5, 0.2, 1.5), cl + Vec3::new(1.5, 0.5, 1.5));
    b.placed(g(r.x0 + 2.75, r.z1 - 1.6) - Vec3::Y * 0.1, |t| kit::woodpile(t, Rect::new(-1.75, -0.6, 1.75, 0.6), 1.1));
    for k in 0..8 {
        let p = g(r.x0 + 8.0 + (k % 4) as f32 * 0.5, r.z0 + 11.0 + (k / 4) as f32 * 0.5);
        kit::amphora(b, p - Vec3::Y * 0.05, 0.95, 1);
    }
    for k in 0..5 {
        let p = g(r.x0 + 8.0 + k as f32 * 0.55, r.z0 + 13.0);
        kit::jug(b, p - Vec3::Y * 0.03, 0.45, Mat::Ceramic);
    }
    // Низкая ограда двора с проходом с улицы.
    super::low_wall(b, Vec2::new(r.x0, r.z1), Vec2::new(r.x1, r.z1));
    super::low_wall(b, Vec2::new(r.x1, r.z0 + 3.0), Vec2::new(r.x1, r.z1));
    super::low_wall(b, Vec2::new(r.x0, r.z0 + 3.0), Vec2::new(r.x0, r.z1));
}

// ---------------- хозяйство ----------------

/// Ток: мощёный круг для обмолота с каменным катком и стогом соломы.
fn threshing_floor(b: &mut Builder) {
    let tp = g(VILLAGE.x + 47.0, VILLAGE.z - 14.0);
    b.at(tp.x, tp.z);
    b.mesh(Mat::Paving).disc(tp + Vec3::Y * 0.1, 5.0, 20, true);
    b.mesh(Mat::Rubble).lathe(tp - Vec3::Y * 0.3, &[(5.3, 0.0), (5.3, 0.3), (5.0, 0.34)], 20);
    // Каменный каток.
    let c = tp + Vec3::new(1.5, 0.45, 0.5);
    b.mesh(Mat::Limestone).beam(c - Vec3::X * 0.6, c + Vec3::X * 0.6, 0.32, 0.32, Vec3::Y);
    kit::hidden(b, c - Vec3::new(0.7, 0.45, 0.35), c + Vec3::new(0.7, 0.3, 0.35));
    // Солома по краю и стог.
    b.mesh(Mat::Thatch).disc(tp + Vec3::new(-1.0, 0.13, -1.0), 2.2, 12, true);
    haystack(b, g(VILLAGE.x + 55.0, VILLAGE.z - 19.0), 1.6);
}

fn haystack(b: &mut Builder, p: Vec3, r: f32) {
    b.at(p.x, p.z);
    b.mesh(Mat::Thatch).lathe(p - Vec3::Y * 0.2, &[(r, 0.0), (r * 1.08, r * 0.6), (r * 0.75, r * 1.3), (0.0, r * 1.75)], 12);
    b.mesh(Mat::Wood).beam(p + Vec3::Y * (r * 1.5), p + Vec3::Y * (r * 2.1), 0.04, 0.04, Vec3::X);
    kit::hidden(b, p - Vec3::new(r * 0.8, 0.2, r * 0.8), p + Vec3::new(r * 0.8, r * 1.5, r * 0.8));
}

/// Пасека за северными дворами: плетёные ульи-корзины на каменных подставках.
fn apiary(b: &mut Builder, rng: &mut Rng) {
    for k in 0..6 {
        let p = g(VILLAGE.x - 50.0 + k as f32 * 2.0, VILLAGE.z - 25.0 + rng.range(-0.3, 0.3));
        b.at(p.x, p.z);
        b.solid([p.x - 0.35, p.y - 0.3, p.z - 0.35], [p.x + 0.35, p.y + 0.35, p.z + 0.35], Mat::Rubble);
        let top = p + Vec3::Y * 0.35;
        b.mesh(Mat::Thatch).lathe(top, &[(0.3, 0.0), (0.36, 0.25), (0.33, 0.5), (0.22, 0.7), (0.0, 0.78)], 10);
        b.mesh(Mat::Earth).cuboid(top + Vec3::new(-0.06, 0.0, 0.28), top + Vec3::new(0.06, 0.06, 0.37));
        kit::hidden(b, top - Vec3::new(0.32, 0.0, 0.32), top + Vec3::new(0.32, 0.75, 0.32));
    }
}

/// Огороды за северными дворами (капуста, лук, бобы — грядками) за низкой оградой.
fn gardens(b: &mut Builder, rng: &mut Rng) {
    for (x0, x1) in [(-37.0f32, -16.0f32), (14.0, 35.0)] {
        let r = Rect::new(VILLAGE.x + x0, VILLAGE.z - 33.0, VILLAGE.x + x1, VILLAGE.z - 24.0);
        b.at(r.cx(), r.cz());
        super::low_wall(b, Vec2::new(r.x0, r.z0), Vec2::new(r.x1, r.z0));
        super::low_wall(b, Vec2::new(r.x0, r.z0), Vec2::new(r.x0, r.z1));
        super::low_wall(b, Vec2::new(r.x1, r.z0), Vec2::new(r.x1, r.z1));
        let mut z = r.z0 + 1.2;
        let mut row = 0;
        while z < r.z1 - 0.6 {
            // Грядка — вал земли; на нём кочаны или ботва.
            let a = g(r.x0 + 1.0, z);
            let c = g(r.x1 - 1.0, z);
            ridge(b, a, c, 0.45, 0.14);
            let mut x = r.x0 + 1.4;
            while x < r.x1 - 1.2 {
                let p = g(x, z) + Vec3::Y * 0.1;
                if row % 3 == 0 {
                    b.mesh(Mat::HerbGreen).lathe(p, &[(0.0, 0.0), (0.22, 0.05), (0.26, 0.18), (0.16, 0.3), (0.0, 0.33)], 7);
                } else if row % 3 == 1 {
                    b.mesh(Mat::HerbGreen).lathe(p, &[(0.0, 0.0), (0.06, 0.02), (0.02, 0.35), (0.0, 0.36)], 5);
                } else {
                    b.mesh(Mat::HerbSilver).lathe(p, &[(0.0, 0.0), (0.16, 0.06), (0.14, 0.32), (0.0, 0.4)], 6);
                }
                x += rng.range(0.55, 0.75);
            }
            z += 1.1;
            row += 1;
        }
    }
}

/// Вал земли от `a` до `c` (грядка, борозда), по рельефу кусками.
fn ridge(b: &mut Builder, a: Vec3, c: Vec3, half_w: f32, h: f32) {
    let n = ((a.xz().distance(c.xz())) / 4.0).ceil().max(1.0) as usize;
    let mut pts = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let p = a.lerp(c, k as f32 / n as f32);
        pts.push(Vec3::new(p.x, ground_y(p.x, p.z) + h * 0.4, p.z));
    }
    b.mesh(Mat::Earth).beam_path(&pts, half_w, h * 0.6, Vec3::Y);
}

/// Пашня к югу от села: после жатвы — борозды под озимые и копны снопов на сжатых полосах.
fn fields(b: &mut Builder, rng: &mut Rng) {
    let c = VILLAGE;
    // Полосы по 12 м: через одну — свежевспаханная (борозды), сжатая (стерня и копны).
    let (z0, z1) = (c.z + 24.0, c.z + 50.0);
    let mut x = c.x - 56.0;
    let mut k = 0;
    while x < c.x + 34.0 {
        let x1 = x + 12.0;
        b.at((x + x1) / 2.0, (z0 + z1) / 2.0);
        if k % 2 == 0 {
            // Пашня — тёмная разрыхлённая земля, борозды поперёк полосы.
            let lift = |px: f32, pz: f32| Vec3::new(px, ground_y(px, pz) + 0.05, pz);
            let mut z = z0;
            while z < z1 {
                let zz = (z + 4.0).min(z1);
                b.mesh(Mat::Earth).quad([lift(x + 0.4, z), lift(x1 - 0.4, z), lift(x1 - 0.4, zz), lift(x + 0.4, zz)], Vec3::Y);
                z = zz;
            }
            let mut xx = x + 0.9;
            while xx < x1 - 0.6 {
                ridge(b, g(xx, z0 + 0.4), g(xx, z1 - 0.4), 0.16, 0.12);
                xx += 0.8;
            }
        } else {
            // Стерня — светлая солома поверх земли.
            let lift = |px: f32, pz: f32| Vec3::new(px, ground_y(px, pz) + 0.05, pz);
            let mut z = z0;
            while z < z1 {
                let zz = (z + 4.0).min(z1);
                b.mesh(Mat::Thatch).quad([lift(x + 0.4, z), lift(x1 - 0.4, z), lift(x1 - 0.4, zz), lift(x + 0.4, zz)], Vec3::Y);
                z = zz;
            }
            // Копны: снопы, составленные шатром.
            let mut z = z0 + 3.0;
            while z < z1 - 2.0 {
                let p = g(x + rng.range(3.0, 9.0), z + rng.range(-0.8, 0.8));
                stook(b, p, rng);
                z += rng.range(5.0, 7.0);
            }
        }
        x = x1;
        k += 1;
    }
}

/// Копна: несколько снопов, приставленных друг к другу колосьями вверх.
fn stook(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    let n = 5 + rng.int(3);
    let yaw0 = rng.range(0.0, TAU);
    for i in 0..n {
        let a = yaw0 + i as f32 / n as f32 * TAU;
        let foot = p + Vec3::new(a.cos() * 0.45, -0.05, a.sin() * 0.45);
        let top = p + Vec3::Y * 1.15;
        b.mesh(Mat::Thatch).beam(foot, top, 0.13, 0.11, Vec3::Y);
        // Перевясло.
        b.mesh(Mat::Thatch).beam(foot.lerp(top, 0.55) - Vec3::Y * 0.05, foot.lerp(top, 0.55) + Vec3::Y * 0.05, 0.15, 0.13, Vec3::Y);
    }
    b.mesh(Mat::Thatch).lathe(p + Vec3::Y * 1.0, &[(0.0, 0.0), (0.3, 0.05), (0.0, 0.4)], 7);
    kit::hidden(b, p - Vec3::new(0.45, 0.1, 0.45), p + Vec3::new(0.45, 1.3, 0.45));
}

/// Загон для овец на западном краю села, со стогом сена.
fn sheep_fold(b: &mut Builder) {
    let c = VILLAGE;
    let pen = Rect::new(c.x - 66.0, c.z - 22.0, c.x - 53.0, c.z - 8.0);
    b.at(pen.cx(), pen.cz());
    super::low_wall(b, Vec2::new(pen.x0, pen.z0), Vec2::new(pen.x1, pen.z0));
    super::low_wall(b, Vec2::new(pen.x0, pen.z0), Vec2::new(pen.x0, pen.z1));
    super::low_wall(b, Vec2::new(pen.x0, pen.z1), Vec2::new(pen.x1, pen.z1));
    super::low_wall(b, Vec2::new(pen.x1, pen.z0), Vec2::new(pen.x1, pen.z1 - 4.0));
    // Навес для ягнят у северной ограды.
    let sh = g(pen.cx(), pen.z0 + 1.5);
    b.placed(sh, |t| {
        for x in [-2.5f32, 2.5] {
            t.solid([x - 0.1, -0.2, 0.9], [x + 0.1, 1.9, 1.1], Mat::Wood);
        }
        t.mesh(Mat::Thatch).lean_roof([Vec3::new(-2.8, 2.2, -1.4), Vec3::new(2.8, 2.2, -1.4)], [Vec3::new(-2.8, 1.8, 1.3), Vec3::new(2.8, 1.8, 1.3)]);
        t.mesh(Mat::Wood).beam(Vec3::new(-2.6, 1.9, 1.0), Vec3::new(2.6, 1.9, 1.0), 0.07, 0.07, Vec3::Y);
    });
    haystack(b, g(pen.x0 + 3.0, pen.z1 - 3.0), 1.3);
}

/// Маленький виноградник за восточными дворами.
fn vineyard(b: &mut Builder, rng: &mut Rng) {
    let c = VILLAGE;
    let plot = Rect::new(c.x + 38.0, c.z - 34.0, c.x + 62.0, c.z - 22.0);
    b.at(plot.cx(), plot.cz());
    super::low_wall(b, Vec2::new(plot.x0, plot.z0), Vec2::new(plot.x1, plot.z0));
    super::low_wall(b, Vec2::new(plot.x1, plot.z0), Vec2::new(plot.x1, plot.z1));
    super::low_wall(b, Vec2::new(plot.x0, plot.z0), Vec2::new(plot.x0, plot.z1));
    let mut z = plot.z0 + 1.6;
    while z < plot.z1 - 1.0 {
        let mut x = plot.x0 + 1.5;
        while x < plot.x1 - 1.5 {
            let x1 = (x + 9.0).min(plot.x1 - 1.5);
            kit::vine_row(b, g(x, z), g(x1, z), rng.range(1.1, 1.4));
            x = x1 + 1.5;
        }
        z += 2.4;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{Body, LevelColliders};

    fn level() -> LevelColliders {
        crate::level::colliders(super::super::super::city_layout())
    }

    /// Идёт по ломаной по земле; падает, если застрял.
    fn walk(level: &LevelColliders, path: &[Vec3]) -> Vec3 {
        let mut feet = path[0];
        let mut grounded = true;
        for target in &path[1..] {
            for _ in 0..40000 {
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

    #[test]
    fn village_is_lived_in_and_has_a_house_for_sale() {
        let layout = super::super::super::city_layout();
        let near = |p: Vec3| p.xz().distance(VILLAGE.xz()) < 80.0;
        let people = layout.residents.iter().filter(|r| near(r.pos)).count();
        assert!(people >= 15, "жителей в селе: {people}");
        let doors = layout.doors.iter().filter(|d| near(d.center)).count();
        assert!(doors >= 15, "дверей в селе: {doors}");
        assert!(layout.estates.iter().any(|e| e.town == crate::towns::TownId::Phocas), "в селе нет двора на продажу");
        for id in ["phocas_village", "phocas_tavern", "phocas_pottery"] {
            assert!(crate::level::place(id).is_some(), "нет места {id}");
            assert!(crate::trade::MERCHANTS.iter().any(|m| m.place == id), "у места {id} нет торговца");
            assert_eq!(crate::towns::TownId::of_place(id), crate::towns::TownId::Phocas);
        }
    }

    /// От главных ворот по дороге в село, через площадь к корчме, к гончару и дальше
    /// тропой к дороге от южных ворот.
    #[test]
    fn walk_from_the_gate_to_the_village() {
        let level = level();
        level.set_all_doors(true);
        let on = |p: Vec2| Vec3::new(p.x, ground_y(p.x, p.y) + 0.1, p.y);
        let gx = super::super::super::main_gate_x();
        let road = &village_roads()[0];
        let mut path = vec![on(Vec2::new(gx, 200.0)), on(Vec2::new(-110.0, 262.0))];
        path.extend(road.iter().map(|p| on(*p)));
        walk(&level, &path);
        // С площади — в корчму к стойке и к гончарному кругу.
        let sq = on(Vec2::new(VILLAGE.x, VILLAGE.z));
        let t = tavern_rect();
        let door = on(Vec2::new(t.cx() - 1.5, VILLAGE.z + 8.0));
        let inside = |x: f32, z: f32| Vec3::new(t.cx() + x, tavern_floor() + 0.1, t.cz() + z);
        walk(&level, &[sq, door, inside(-1.5, -2.7), inside(-0.3, 0.3), inside(2.6, 0.3)]);
        let pot = potter_point();
        walk(&level, &[sq, on(Vec2::new(pot.x, VILLAGE.z)), pot + Vec3::new(0.0, 0.1, 0.0)]);
    }
}
