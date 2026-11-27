//! За стенами: дороги по плато Гераклейского полуострова, некрополь у южных ворот,
//! клер (усадьба с виноградником и башней), родник в балке, готское селение,
//! стан гунно-булгарских кочевников и аланский стан; можжевельник, сосна и дуб,
//! известняковые выходы и заросли лекарственных трав.
//!
//! Земля здесь холмистая: всё ставится по [`terrain::ground_y`], постройки — на террасы.

use super::kit::{self, Op, Rect, Rng};
use super::*;
use super::interior::extras;
use crate::items::LootTableId as L;
use crate::level::{PlantKind, Role};
use crate::collision::Aabb;
use crate::level::places::{self, ALAN_CAMP, GOTH_VILLAGE, KLEROS_FARM, NECROPOLIS, STEPPE_CAMP};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use terrain::{balka_x, ground_y};

pub(crate) mod mines;
pub(crate) mod fishing;
pub(crate) mod village;

const ROAD_W: f32 = 4.0;

fn g(x: f32, z: f32) -> Vec3 {
    Vec3::new(x, ground_y(x, z), z)
}

pub fn outside(b: &mut Builder) {
    let roads = roads();
    for r in &roads {
        road(b, r);
    }
    milestone(b);
    necropolis(b, &roads);
    kleros(b);
    spring(b);
    goth_village(b);
    goth_smithy(b);
    steppe_camp(b);
    alan_camp(b);
    camp_forge(b, steppe_forge_rect(), "Горн кочевого кузнеца");
    camp_forge(b, alan_forge_rect(), "Горн аланского кузнеца");
    brigand_camps(b);
    herbs(b);
    mines::mines(b);
    fishing::fishing_hut(b);
    wilderness(b, &roads);
    // Село — последним: его усадьба на продажу идёт в конце списка (сохранения не сдвигаются).
    village::village(b);
}

// ---------------- дороги ----------------

/// Дороги: от главных ворот — на запад к готам и на юг к кочевникам (мимо родника);
/// от южных ворот — через некрополь, вдоль бухты, к аланам.
pub fn roads() -> Vec<Vec<Vec2>> {
    let gx = main_gate_x();
    let sx = south_gate_x();
    let fork = Vec2::new(-112.0, 340.0);
    let v = Vec2::new;
    let mut all = vec![
        vec![v(gx, 183.4), v(gx, 200.0), v(-110.0, 262.0), fork, v(-200.0, 420.0), v(-330.0, 482.0), v(GOTH_VILLAGE.x + 24.0, GOTH_VILLAGE.z + 2.0)],
        vec![fork, v(-20.0, 455.0), v(30.0, 506.0), v(balka_x(548.0), 548.0), v(120.0, 600.0), v(STEPPE_CAMP.x - 20.0, STEPPE_CAMP.z - 4.0)],
        vec![v(sx, 183.4), v(sx, 200.0), v(185.0, 240.0), v(205.0, 300.0), v(260.0, 400.0), v(330.0, 478.0), v(420.0, 600.0), v(ALAN_CAMP.x - 19.0, ALAN_CAMP.z - 8.0)],
    ];
    // Тропы к руднику и к стану углежогов.
    all.extend(mines::mine_roads());
    all.extend(village::village_roads());
    all
}

fn seg_distance(p: Vec2, a: Vec2, c: Vec2) -> f32 {
    let t = ((p - a).dot(c - a) / (c - a).length_squared()).clamp(0.0, 1.0);
    p.distance(a + (c - a) * t)
}

/// Расстояние до ближайшей дороги (дороги считаются один раз).
pub fn road_distance_at(p: Vec2) -> f32 {
    static ROADS: std::sync::OnceLock<Vec<Vec<Vec2>>> = std::sync::OnceLock::new();
    road_distance(ROADS.get_or_init(roads), p)
}

fn road_distance(roads: &[Vec<Vec2>], p: Vec2) -> f32 {
    roads.iter().flat_map(|r| r.windows(2).map(move |w| seg_distance(p, w[0], w[1]))).fold(f32::MAX, f32::min)
}

/// Грунтовая дорога, лежащая по рельефу (без коллизии — по ней идут по земле).
fn road(b: &mut Builder, pts: &[Vec2]) {
    let lift = |p: Vec2| Vec3::new(p.x, ground_y(p.x, p.y) + 0.1, p.y);
    for w in pts.windows(2) {
        let (a, c) = (w[0], w[1]);
        let len = a.distance(c);
        let n = (len / 4.0).ceil() as usize;
        let dir = (c - a) / len;
        let side = Vec2::new(-dir.y, dir.x) * (ROAD_W / 2.0);
        for k in 0..n {
            let p0 = a + (c - a) * (k as f32 / n as f32);
            let p1 = a + (c - a) * ((k + 1) as f32 / n as f32);
            b.at(p0.x, p0.y);
            b.mesh(Mat::Earth).quad([lift(p0 - side), lift(p1 - side), lift(p1 + side), lift(p0 + side)], Vec3::Y);
        }
    }
    for p in &pts[1..pts.len() - 1] {
        b.at(p.x, p.y);
        b.mesh(Mat::Earth).disc(lift(*p) - Vec3::Y * 0.01, ROAD_W / 2.0, 12, true);
    }
}

/// Мильный столб у главных ворот (римский обычай: «от Херсона — миля первая»).
fn milestone(b: &mut Builder) {
    let p = g(main_gate_x() + 4.0, 214.0);
    b.at(p.x, p.z);
    b.mesh(Mat::Limestone).lathe(p - Vec3::Y * 0.3, &[(0.32, 0.0), (0.32, 0.5), (0.26, 0.55), (0.24, 1.9), (0.27, 2.0), (0.0, 2.0)], 12);
    kit::hidden(b, p + Vec3::new(-0.3, -0.3, -0.3), p + Vec3::new(0.3, 1.7, 0.3));
}

// ---------------- террасы ----------------

/// Ровная площадка под постройку: ступенчатая подпорная кладка, верх — пол постройки.
/// Возвращает высоту пола.
fn terrace(b: &mut Builder, r: Rect, mat: Mat) -> f32 {
    let (lo, y) = terrace_levels(r);
    b.at(r.cx(), r.cz());
    b.solid([r.x0, lo - 0.8, r.z0], [r.x1, y, r.z1], mat);
    // Ступени вокруг, чтобы на террасу можно было подняться с любой стороны.
    let levels = (((y - lo) / 0.4).ceil() as usize).saturating_sub(1).min(5);
    for i in 1..=levels {
        let e = 0.6 * i as f32;
        b.solid([r.x0 - e, lo - 0.8, r.z0 - e], [r.x1 + e, y - 0.4 * i as f32, r.z1 + e], mat);
    }
    y
}

/// Самая низкая точка земли под площадкой и высота её пола.
fn terrace_levels(r: Rect) -> (f32, f32) {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    let nx = (r.w() / 2.0).ceil() as usize;
    let nz = (r.d() / 2.0).ceil() as usize;
    for i in 0..=nx {
        for j in 0..=nz {
            let y = ground_y(r.x0 + r.w() * i as f32 / nx as f32, r.z0 + r.d() * j as f32 / nz as f32);
            lo = lo.min(y);
            hi = hi.max(y);
        }
    }
    (lo, hi + 0.08)
}

/// Двор усадьбы клера (по наружным граням ограды).
pub fn kleros_yard() -> Rect {
    Rect::new(KLEROS_FARM.x, KLEROS_FARM.z, KLEROS_FARM.x + 22.0, KLEROS_FARM.z + 16.0)
}

/// Высота пола во дворе усадьбы клера.
pub fn kleros_floor() -> f32 {
    terrace_levels(kleros_yard().inset(-0.6)).1
}

/// Двускатная кровля из камыша/соломы над прямоугольником (фронтоны — из `gable`).
pub fn thatch_roof(b: &mut Builder, r: Rect, eave: f32, rise: f32, ridge_x: bool, over: f32, gable: Mat) {
    let ro = Rect::new(r.x0 - over, r.z0 - over, r.x1 + over, r.z1 + over);
    let half = if ridge_x { r.d() / 2.0 } else { r.w() / 2.0 };
    let e = eave - rise / half * over;
    let top = eave + rise;
    b.roof(Mat::Thatch, |m| m.gable_roof_slopes(Vec2::new(ro.x0, ro.z0), Vec2::new(ro.x1, ro.z1), e, top, ridge_x));
    let mut under = crate::level::geom::MeshBuilder::default();
    under.gable_roof_slopes(Vec2::new(ro.x0, ro.z0), Vec2::new(ro.x1, ro.z1), e - 0.25, top - 0.25, ridge_x);
    b.mesh(Mat::Thatch).append(under.flipped());
    // Кровля — толстый пласт камыша, а не лист: торцы над фронтонами закрыты срезом снопов,
    // по свесу — округлый валик, конёк — толстый жгут, прижатый жердями (по ним можно ступить).
    let th = 0.25;
    {
        let m = b.mesh(Mat::Thatch);
        let (lo, hi, c) = if ridge_x { (ro.z0, ro.z1, r.cz()) } else { (ro.x0, ro.x1, r.cx()) };
        let (e0, e1) = if ridge_x { (ro.x0, ro.x1) } else { (ro.z0, ro.z1) };
        let pt = |along: f32, across: f32, y: f32| if ridge_x { Vec3::new(along, y, across) } else { Vec3::new(across, y, along) };
        for (end, sgn) in [(e0, -1.0f32), (e1, 1.0)] {
            let out = pt(sgn, 0.0, 0.0) - pt(0.0, 0.0, 0.0);
            for side in [lo, hi] {
                m.quad([pt(end, side, e), pt(end, c, top), pt(end, c, top - th), pt(end, side, e - th)], out);
            }
        }
    }
    b.roof(Mat::Thatch, |m| {
        if ridge_x {
            for z in [ro.z0, ro.z1] {
                m.ridge_pipe(Vec3::new(ro.x0, e - 0.13, z), Vec3::new(ro.x1, e - 0.13, z), 0.14, 6);
                m.beam(Vec3::new(ro.x0, e - 0.19, z), Vec3::new(ro.x1, e - 0.19, z), 0.14, 0.06, Vec3::Y);
            }
            m.ridge_pipe(Vec3::new(ro.x0 - 0.04, top - 0.1, r.cz()), Vec3::new(ro.x1 + 0.04, top - 0.1, r.cz()), 0.26, 7);
        } else {
            for x in [ro.x0, ro.x1] {
                m.ridge_pipe(Vec3::new(x, e - 0.13, ro.z0), Vec3::new(x, e - 0.13, ro.z1), 0.14, 6);
                m.beam(Vec3::new(x, e - 0.19, ro.z0), Vec3::new(x, e - 0.19, ro.z1), 0.14, 0.06, Vec3::Y);
            }
            m.ridge_pipe(Vec3::new(r.cx(), top - 0.1, ro.z0 - 0.04), Vec3::new(r.cx(), top - 0.1, ro.z1 + 0.04), 0.26, 7);
        }
    });
    // Жерди-прижимы поперёк конька (каждые ~1.1 м) и стропила под свесом.
    {
        let m = b.mesh(Mat::Wood);
        let len = if ridge_x { ro.w() } else { ro.d() };
        let n = (len / 1.1).round().max(2.0) as usize;
        let slope = Vec2::new(half + over, top - e).normalize();
        for i in 0..=n {
            let t = i as f32 / n as f32;
            for sgn in [-1.0f32, 1.0] {
                let (a, c) = if ridge_x {
                    let x = ro.x0 + 0.15 + (len - 0.3) * t;
                    (Vec3::new(x, top + 0.2, r.cz()), Vec3::new(x, top + 0.2 - slope.y * 0.75, r.cz() + sgn * slope.x * 0.75))
                } else {
                    let z = ro.z0 + 0.15 + (len - 0.3) * t;
                    (Vec3::new(r.cx(), top + 0.2, z), Vec3::new(r.cx() + sgn * slope.x * 0.75, top + 0.2 - slope.y * 0.75, z))
                };
                m.beam(a, c, 0.025, 0.025, Vec3::Y);
            }
        }
    }
    let m = b.mesh(gable);
    if ridge_x {
        for (x, s) in [(r.x0, -1.0f32), (r.x1, 1.0)] {
            m.tri(Vec3::new(x, eave, r.z0), Vec3::new(x, eave, r.z1), Vec3::new(x, top, r.cz()), Vec3::X * s);
        }
    } else {
        for (z, s) in [(r.z0, -1.0f32), (r.z1, 1.0)] {
            m.tri(Vec3::new(r.x0, eave, z), Vec3::new(r.x1, eave, z), Vec3::new(r.cx(), top, z), Vec3::Z * s);
        }
    }
}

// ---------------- некрополь ----------------

/// Некрополь за южными воротами: христианские плитовые могилы (головой на запад) со стелами-
/// крестами, известняковые саркофаги, склепы с дромосом, маленькая поминальная церковь.
fn necropolis(b: &mut Builder, roads: &[Vec<Vec2>]) {
    let c = Vec2::new(NECROPOLIS.x, NECROPOLIS.z);
    let area = Rect::new(166.0, 206.0, 290.0, 300.0);
    let chapel_c = Vec2::new(245.0, 268.0);
    memorial_chapel(b, chapel_c);
    let mut rng = Rng::new(510);
    let mut z = area.z0 + 1.5;
    while z < area.z1 {
        let mut x = area.x0 + rng.range(0.0, 2.0);
        while x < area.x1 {
            let p = Vec2::new(x + rng.range(-0.5, 0.5), z + rng.range(-0.6, 0.6));
            x += rng.range(3.4, 4.6);
            let near_chapel = (p.x - chapel_c.x).abs() < 10.0 && (p.y - chapel_c.y).abs() < 7.5;
            if near_chapel || road_distance(roads, p) < ROAD_W / 2.0 + 2.5 || p.distance(c) < 8.0 || ground_y(p.x, p.y) < WATER_Y + 1.5 || rng.chance(0.3) {
                continue;
            }
            b.at(p.x, p.y);
            let roll = rng.f();
            if roll < 0.62 {
                slab_grave(b, p, &mut rng);
            } else if roll < 0.86 {
                sarcophagus(b, p, &mut rng);
            } else if roll < 0.94 {
                crypt(b, p);
            } else {
                stele(b, g(p.x, p.y), rng.chance(0.7));
            }
        }
        z += rng.range(3.6, 4.6);
    }
}

/// Стела над могилой: плита с рельефным крестом (или старая языческая — гладкая, с фронтоном).
fn stele(b: &mut Builder, p: Vec3, cross: bool) {
    let m = b.mesh(Mat::Limestone);
    m.cuboid(p + Vec3::new(-0.08, -0.3, -0.3), p + Vec3::new(0.08, 0.95, 0.3));
    if cross {
        m.cuboid(p + Vec3::new(0.08, 0.35, -0.04), p + Vec3::new(0.11, 0.85, 0.04));
        m.cuboid(p + Vec3::new(0.08, 0.62, -0.17), p + Vec3::new(0.11, 0.7, 0.17));
    } else {
        m.tri(p + Vec3::new(0.0, 0.95, -0.3), p + Vec3::new(0.0, 0.95, 0.3), p + Vec3::new(0.0, 1.15, 0.0), Vec3::X);
        m.tri(p + Vec3::new(0.0, 0.95, -0.3), p + Vec3::new(0.0, 0.95, 0.3), p + Vec3::new(0.0, 1.15, 0.0), -Vec3::X);
    }
    kit::hidden(b, p + Vec3::new(-0.1, -0.3, -0.3), p + Vec3::new(0.1, 0.95, 0.3));
}

/// Плитовая могила: перекрыта известняковыми плитами, обложена камнем; стела — в головах (на западе).
fn slab_grave(b: &mut Builder, p: Vec2, rng: &mut Rng) {
    let y = ground_y(p.x, p.y);
    let base = Vec3::new(p.x, y, p.y);
    let m = b.mesh(Mat::Rubble);
    m.cuboid(base + Vec3::new(-1.15, -0.3, -0.5), base + Vec3::new(1.15, 0.12, 0.5));
    let m = b.mesh(Mat::Limestone);
    let n = 3;
    for k in 0..n {
        let x0 = -1.0 + 2.0 * k as f32 / n as f32 + 0.02;
        let x1 = -1.0 + 2.0 * (k + 1) as f32 / n as f32 - 0.02;
        let lift = rng.range(0.0, 0.04);
        m.cuboid(base + Vec3::new(x0, 0.0, -0.38), base + Vec3::new(x1, 0.2 + lift, 0.38));
    }
    if rng.chance(0.75) {
        stele(b, g(p.x - 1.3, p.y), rng.chance(0.85));
    }
}

/// Саркофаг с двускатной крышкой и угловыми акротериями.
fn sarcophagus(b: &mut Builder, p: Vec2, rng: &mut Rng) {
    let y = ground_y(p.x, p.y);
    let (hx, hz) = (1.15, 0.48);
    let lean = rng.range(-0.04, 0.04);
    b.solid([p.x - hx, y - 0.4, p.y - hz], [p.x + hx, y + 0.8, p.y + hz], Mat::Limestone);
    let m = b.mesh(Mat::Limestone);
    m.cuboid(Vec3::new(p.x - hx - 0.06, y - 0.4, p.y - hz - 0.06), Vec3::new(p.x + hx + 0.06, y + 0.12, p.y + hz + 0.06));
    let lid_y = y + 0.8;
    m.gable_roof(Vec2::new(p.x - hx - 0.05, p.y - hz - 0.05 + lean), Vec2::new(p.x + hx + 0.05, p.y + hz + 0.05 + lean), lid_y, lid_y + 0.32, true);
    m.cuboid(Vec3::new(p.x - hx - 0.05, lid_y - 0.08, p.y - hz - 0.05), Vec3::new(p.x + hx + 0.05, lid_y, p.y + hz + 0.05));
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let q = Vec3::new(p.x + sx * (hx - 0.05), lid_y, p.y + sz * (hz - 0.02));
        m.cuboid(q - Vec3::new(0.11, 0.0, 0.06), q + Vec3::new(0.11, 0.24, 0.06));
    }
    // Табула для надписи на длинной стороне.
    m.cuboid(Vec3::new(p.x - 0.45, y + 0.25, p.y + hz), Vec3::new(p.x + 0.45, y + 0.62, p.y + hz + 0.03));
}

/// Склеп: сводчатая камера под земляным курганчиком; вход с востока закрыт плитой.
fn crypt(b: &mut Builder, p: Vec2) {
    let y = ground_y(p.x, p.y);
    let base = Vec3::new(p.x, y, p.y);
    b.solid([p.x - 1.6, y - 0.6, p.y - 1.3], [p.x + 1.2, y + 1.2, p.y + 1.3], Mat::Rubble);
    // Невысокий земляной холмик над камерой.
    let mound = [(2.6, -0.3), (2.3, 0.2), (1.8, 0.75), (1.1, 1.15), (0.4, 1.32), (0.0, 1.35)];
    b.mesh(Mat::Terrain).lathe(base - Vec3::new(0.2, 0.0, 0.0), &mound, 14);
    // Фасад-портал с плитой-закладом.
    let f = base + Vec3::new(1.2, 0.0, 0.0);
    let m = b.mesh(Mat::Limestone);
    m.cuboid(f + Vec3::new(0.0, -0.3, -0.9), f + Vec3::new(0.35, 1.45, -0.55));
    m.cuboid(f + Vec3::new(0.0, -0.3, 0.55), f + Vec3::new(0.35, 1.45, 0.9));
    m.cuboid(f + Vec3::new(-0.02, 1.25, -1.0), f + Vec3::new(0.4, 1.55, 1.0));
    m.cuboid(f + Vec3::new(0.05, -0.2, -0.6), f + Vec3::new(0.2, 1.25, 0.6));
    b.mesh(Mat::Coals).quad([f + Vec3::new(0.21, 1.1, -0.45), f + Vec3::new(0.21, 1.1, 0.45), f + Vec3::new(0.21, 1.24, 0.45), f + Vec3::new(0.21, 1.24, -0.45)], Vec3::X);
    kit::hidden(b, f + Vec3::new(0.0, -0.3, -1.0), f + Vec3::new(0.4, 1.55, 1.0));
}

/// Поминальная церковь (мартирий) — однонефная, с апсидой на восток (+X), черепичная кровля.
fn memorial_chapel(b: &mut Builder, c: Vec2) {
    let r = Rect::new(c.x - 4.5, c.y - 3.0, c.x + 3.5, c.y + 3.0);
    let y = terrace(b, Rect::new(r.x0 - 1.0, r.z0 - 1.0, r.x1 + 3.4, r.z1 + 1.0), Mat::Rubble);
    b.placed(Vec3::new(c.x, y, c.y), |t| {
        let r = Rect::new(-4.5, -3.0, 3.5, 3.0);
        let h = 4.2;
        kit::wall_z(t, r.z0, r.z1, r.x0 + 0.3, 0.6, h, &[Op::Door { s: 0.0, w: 1.4, h: 2.4 }], Mat::OpusMixtum);
        kit::wall_x(t, r.x0, r.x1, r.z0 + 0.3, 0.6, h, &[Op::Window { s: 0.0, w: 0.6, sill: 2.4, top: 3.4 }], Mat::OpusMixtum);
        kit::wall_x(t, r.x0, r.x1, r.z1 - 0.3, 0.6, h, &[Op::Window { s: 0.0, w: 0.6, sill: 2.4, top: 3.4 }], Mat::OpusMixtum);
        // Восточная стена с проёмом в апсиду.
        kit::wall_z(t, r.z0, r.z1, r.x1 - 0.3, 0.6, h, &[Op::Arch { s: 0.0, w: 3.0, spring: 2.6 }], Mat::OpusMixtum);
        let ap = Vec3::new(r.x1, 0.0, 0.0);
        let prof = [(2.1, 0.0), (2.1, h - 0.6)];
        t.mesh(Mat::OpusMixtum).lathe_arc(ap, &prof, -FRAC_PI_2, FRAC_PI_2, 8, false);
        t.mesh(Mat::Plaster).lathe_arc(ap, &[(1.5, 0.0), (1.5, h - 0.6)], -FRAC_PI_2, FRAC_PI_2, 8, true);
        // Конха изнутри оштукатурена: иначе из апсиды видна изнанка черепицы.
        t.mesh(Mat::Plaster).dome(ap + Vec3::Y * (h - 0.6), 2.0, -FRAC_PI_2, FRAC_PI_2, 8, 0.0);
        t.roof(Mat::Tile, |m| m.dome(ap + Vec3::Y * (h - 0.6), 2.1, -FRAC_PI_2, FRAC_PI_2, 8, 0.1));
        for k in 0..6 {
            let a = -FRAC_PI_2 + PI * (k as f32 + 0.5) / 6.0;
            let q = ap + Vec3::new(a.cos() * 1.8, 0.0, a.sin() * 1.8);
            kit::hidden(t, q - Vec3::new(0.35, 0.0, 0.35), q + Vec3::new(0.35, h, 0.35));
        }
        kit::gable_roof(t, r, h, 1.8, true, 0.4, Mat::OpusMixtum);
        t.decor([r.x0 + 0.6, 0.0, r.z0 + 0.6], [r.x1 + 1.4, 0.03, r.z1 - 0.6], Mat::Paving);
        // Престол и лампада.
        t.solid([r.x1 + 0.2, 0.0, -0.45], [r.x1 + 1.0, 0.95, 0.45], Mat::Marble);
        t.lamp(Vec3::new(r.x1 - 0.5, 2.6, 0.0), Color::srgb(1.0, 0.7, 0.4), 4_000.0, 6.0);
        // Гробница мученика — у северной стены.
        t.solid([-2.5, 0.0, r.z0 + 0.7], [0.0, 0.7, r.z0 + 1.5], Mat::Marble);
    });
}

// ---------------- клер ----------------

/// Клер — надел херсонесского земледельца: виноградники за низкими каменными оградами
/// и усадьба с оборонительной башней, давильней, пифосами и жилым домом.
fn kleros(b: &mut Builder) {
    // Виноградники: участки по 40 × 28 м.
    let mut rng = Rng::new(2050);
    for (px, pz) in [(-262.0f32, 232.0f32), (-218.0, 232.0), (-174.0, 232.0), (-262.0, 264.0), (-218.0, 264.0)] {
        let plot = Rect::new(px, pz, px + 40.0, pz + 28.0);
        b.at(plot.cx(), plot.cz());
        low_wall(b, Vec2::new(plot.x0, plot.z0), Vec2::new(plot.x1, plot.z0));
        low_wall(b, Vec2::new(plot.x0, plot.z1), Vec2::new(plot.x1 - 4.0, plot.z1));
        low_wall(b, Vec2::new(plot.x0, plot.z0), Vec2::new(plot.x0, plot.z1));
        low_wall(b, Vec2::new(plot.x1, plot.z0), Vec2::new(plot.x1, plot.z1));
        let mut z = plot.z0 + 2.0;
        while z < plot.z1 - 1.5 {
            let mut x = plot.x0 + 2.0;
            while x < plot.x1 - 2.0 {
                let x1 = (x + 9.0).min(plot.x1 - 2.0);
                kit::vine_row(b, g(x, z), g(x1, z), rng.range(1.1, 1.4));
                x = x1 + 1.5;
            }
            z += 2.4;
        }
    }
    // Усадьба.
    let f = KLEROS_FARM;
    let y = terrace(b, kleros_yard().inset(-0.6), Mat::Rubble);
    b.placed(Vec3::new(f.x, y, f.z), |t| {
        let mut rng = Rng::new(301);
        let r = Rect::new(0.0, 0.0, 22.0, 16.0);
        // Ограда двора с воротами на север, к дороге.
        kit::wall_x(t, r.x0, r.x1, r.z0 + 0.3, 0.6, 2.2, &[Op::Door { s: 14.0, w: 2.6, h: 2.2 }], Mat::Rubble);
        kit::wall_x(t, r.x0, r.x1, r.z1 - 0.3, 0.6, 2.2, &[], Mat::Rubble);
        kit::wall_z(t, r.z0 + 0.6, r.z1 - 0.6, r.x1 - 0.3, 0.6, 2.2, &[], Mat::Rubble);
        kit::wall_z(t, r.z0 + 0.6, r.z1 - 0.6, r.x0 + 0.3, 0.6, 2.2, &[], Mat::Rubble);
        // Башня в северо-западном углу (убежище от набегов).
        let tw = Rect::new(0.6, 0.6, 6.6, 6.6);
        kit::wall_x(t, tw.x0, tw.x1, tw.z0 + 0.4, 0.8, 8.0, &[Op::Window { s: 3.6, w: 0.5, sill: 5.0, top: 5.9 }], Mat::Limestone);
        kit::wall_x(t, tw.x0, tw.x1, tw.z1 - 0.4, 0.8, 8.0, &[Op::Door { s: 3.6, w: 1.1, h: 2.1 }], Mat::Limestone);
        kit::wall_z(t, tw.z0 + 0.8, tw.z1 - 0.8, tw.x0 + 0.4, 0.8, 8.0, &[], Mat::Limestone);
        kit::wall_z(t, tw.z0 + 0.8, tw.z1 - 0.8, tw.x1 - 0.4, 0.8, 8.0, &[Op::Window { s: 3.6, w: 0.5, sill: 5.0, top: 5.9 }], Mat::Limestone);
        kit::quoins(t, tw.x1 - 0.4, tw.z1 - 0.4, 8.0, 0.8);
        kit::hip_roof(t, tw.inset(-0.3), 8.0, 1.6);
        for k in 0..3 {
            kit::pithos(t, Vec3::new(2.0 + k as f32 * 1.4, 0.0, 2.3), 1.4, 0.3);
        }
        // Жилой дом вдоль южной ограды: две комнаты, навес.
        let h = Rect::new(8.0, 10.0, 21.4, 15.4);
        kit::wall_x(t, h.x0, h.x1, h.z0 + 0.3, 0.6, 3.2, &[Op::Door { s: 11.0, w: 1.2, h: 2.2 }, Op::Door { s: 18.0, w: 1.2, h: 2.2 }, Op::Window { s: 14.5, w: 0.7, sill: 1.3, top: 2.1 }], Mat::Rubble);
        kit::wall_z(t, h.z0 + 0.6, h.z1, h.x0 + 0.3, 0.6, 3.2, &[], Mat::Rubble);
        kit::wall_z(t, h.z0 + 0.6, h.z1, 15.0, 0.5, 3.2, &[], Mat::Rubble);
        kit::lean_roof(t, Rect::new(h.x0, h.z0, h.x1 + 0.6, h.z1 + 0.6), kit::Side::S, 4.0, 3.2, 0.5);
        // Высокий край — над оградой двора: ограда и торцы доведены до черепицы.
        kit::lean_fill(t, Rect::new(h.x0, h.z0, h.x1 + 0.6, h.z1 + 0.6), kit::Side::S, 4.0, 3.2, 0.6, 2.2, &[(h.x0, h.x0 + 0.6, 3.2), (h.x1, h.x1 + 0.6, 2.2)], Mat::Rubble);
        kit::hearth(t, Vec3::new(10.5, 0.0, 13.6));
        kit::bed(t, Rect::new(12.6, 13.4, 14.5, 14.6));
        kit::loom(t, Vec3::new(18.0, 0.0, 14.4), true);
        t.loot(L::Household);
        kit::chest(t, Vec3::new(20.4, 0.0, 12.4), false);
        let mut rr = Rng::new(31);
        extras(t, Rect::new(8.6, 10.6, 14.75, 15.4), L::Kitchen, &["Корзина с припасами"], &[(Role::Housewife, Vec3::new(11.5, 0.0, 12.6), Vec3::new(10.5, 0.0, 13.6))], &mut rr);
        extras(t, Rect::new(1.0, 7.5, 7.5, 9.5), L::Household, &[], &[(Role::Farmer, Vec3::new(5.0, 0.0, 8.5), Vec3::new(10.0, 0.0, 3.5))], &mut rr);
        // Давильня, пифосы для сусла, ручная мельница, поленница.
        kit::wine_press(t, Rect::new(9.0, 2.0, 12.0, 5.0));
        for k in 0..4 {
            kit::pithos(t, Vec3::new(16.5 + k as f32 * 1.2, 0.0, 2.0), 1.3, 0.6);
        }
        kit::hand_mill(t, Vec3::new(4.0, 0.0, 9.5));
        kit::woodpile(t, Rect::new(1.0, 13.5, 6.0, 15.2), 1.0);
        kit::cistern_mouth(t, Vec3::new(4.0, 0.0, 12.0));
        kit::fig_tree(t, Vec3::new(18.0, 0.0, 7.5), 4.5, &mut rng);
        for k in 0..5 {
            kit::amphora(t, Vec3::new(7.5 + k as f32 * 0.45, 0.0, 9.2), 1.0, 1);
        }
        t.decor([0.6, 0.0, 0.6], [21.4, 0.03, 15.4], Mat::Earth);
    });
}

/// Низкая ограда из бута (по колено), по рельефу участками по 4 м.
fn low_wall(b: &mut Builder, a: Vec2, c: Vec2) {
    let len = a.distance(c);
    let n = (len / 4.0).ceil() as usize;
    for k in 0..n {
        let p0 = a + (c - a) * (k as f32 / n as f32);
        let p1 = a + (c - a) * ((k + 1) as f32 / n as f32);
        let y0 = ground_y(p0.x, p0.y).min(ground_y(p1.x, p1.y));
        let y1 = ground_y(p0.x, p0.y).max(ground_y(p1.x, p1.y));
        let (min, max) = (p0.min(p1) - Vec2::splat(0.3), p0.max(p1) + Vec2::splat(0.3));
        b.solid([min.x, y0 - 0.4, min.y], [max.x, y1 + 0.45, max.y], Mat::Rubble);
    }
}

// ---------------- родник ----------------

/// Родник в балке, обложенный камнем: чаша, водоразборная стенка с желобом, поилка для скота.
fn spring(b: &mut Builder) {
    let p = places::spring_pos();
    b.at(p.x, p.z);
    b.mesh(Mat::Limestone).lathe(p - Vec3::Y * 0.3, &[(1.7, 0.0), (1.7, 0.75), (1.4, 0.75), (1.4, 0.4)], 16);
    b.mesh(Mat::Water).disc(p + Vec3::Y * 0.3, 1.42, 16, true);
    for k in 0..8 {
        let a = k as f32 / 8.0 * TAU;
        let q = p + Vec3::new(a.cos() * 1.55, 0.0, a.sin() * 1.55);
        kit::hidden(b, q - Vec3::new(0.35, 0.3, 0.35), q + Vec3::new(0.35, 0.45, 0.35));
    }
    // Стенка с каменным носиком с запада (выше по склону балки).
    let w = p + Vec3::new(-2.2, 0.0, 0.0);
    b.solid([w.x - 0.4, w.y - 0.4, w.z - 1.4], [w.x + 0.2, w.y + 1.5, w.z + 1.4], Mat::Limestone);
    b.mesh(Mat::Limestone).cuboid(w + Vec3::new(0.2, 0.9, -0.08), w + Vec3::new(0.7, 1.0, 0.08));
    b.mesh(Mat::Water).beam(w + Vec3::new(0.7, 0.98, 0.0), w + Vec3::new(0.75, 0.3, 0.0), 0.03, 0.03, Vec3::X);
    // Поилка.
    let t = p + Vec3::new(0.0, 0.0, -3.0);
    b.solid([t.x - 1.5, t.y - 0.3, t.z - 0.4], [t.x + 1.5, t.y + 0.5, t.z + 0.4], Mat::Limestone);
    b.mesh(Mat::Water).cuboid(Vec3::new(t.x - 1.35, t.y + 0.5, t.z - 0.28), Vec3::new(t.x + 1.35, t.y + 0.51, t.z + 0.28));
    let mut rng = Rng::new(530);
    for k in 0..4 {
        let a = rng.range(0.0, TAU);
        let q = g(p.x + a.cos() * rng.range(4.0, 7.0), p.z + a.sin() * rng.range(4.0, 7.0));
        if k % 2 == 0 {
            willow(b, q, rng.range(5.0, 7.0), &mut rng);
        }
    }
}

/// Ива у воды: настоящее дерево с кроной из листвы (модель лиственного дерева), а не
/// прежние гранёные «шары» на столбе.
fn willow(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let (yaw, v) = (rng.range(0.0, TAU), (rng.range(0.0, 3.0)) as u32);
    b.plant(PlantKind::Oak, p - Vec3::Y * 0.15, h * 1.35, yaw, v);
    kit::hidden(b, p - Vec3::new(0.3, 0.2, 0.3), p + Vec3::new(0.3, h * 0.5, 0.3));
}

// ---------------- готское селение ----------------

/// Селение крымских готов (федераты империи, христиане): каменные дома под камышом
/// на террасах склона, загон для скота, круглый ток для обмолота, деревянный крест.
fn goth_village(b: &mut Builder) {
    let c = GOTH_VILLAGE;
    let mut rng = Rng::new(488);
    for (k, (dx, dz)) in [(-15.0f32, -11.0f32), (2.0, -17.0), (17.0, -9.0), (15.0, 12.0), (-3.0, 17.0), (-19.0, 8.0)].into_iter().enumerate() {
        let p = Vec2::new(c.x + dx, c.z + dz);
        // Дверь — к центру селения.
        let to_c = -Vec2::new(dx, dz);
        let front = if to_c.x.abs() > to_c.y.abs() {
            if to_c.x > 0.0 { kit::Side::E } else { kit::Side::W }
        } else if to_c.y > 0.0 {
            kit::Side::S
        } else {
            kit::Side::N
        };
        goth_house(b, p, front, k, &mut rng);
    }
    // Ток — мощёный круг для обмолота, у центра.
    let tp = g(c.x - 2.0, c.z - 1.0);
    b.at(tp.x, tp.z);
    b.mesh(Mat::Paving).disc(tp + Vec3::Y * 0.08, 4.0, 18, true);
    b.mesh(Mat::Rubble).lathe(tp - Vec3::Y * 0.2, &[(4.3, 0.0), (4.3, 0.22), (4.0, 0.25)], 18);
    // Крест у дороги.
    let cr = g(c.x + 9.0, c.z + 1.0);
    let m = b.mesh(Mat::Wood);
    m.cuboid(cr + Vec3::new(-0.1, -0.3, -0.1), cr + Vec3::new(0.1, 3.0, 0.1));
    m.cuboid(cr + Vec3::new(-0.08, 2.1, -0.6), cr + Vec3::new(0.08, 2.3, 0.6));
    kit::hidden(b, cr + Vec3::new(-0.12, -0.3, -0.12), cr + Vec3::new(0.12, 3.0, 0.12));
    // Загон.
    let pen = Rect::new(c.x + 26.0, c.z - 8.0, c.x + 40.0, c.z + 6.0);
    b.at(pen.cx(), pen.cz());
    low_wall(b, Vec2::new(pen.x0, pen.z0), Vec2::new(pen.x1, pen.z0));
    low_wall(b, Vec2::new(pen.x1, pen.z0), Vec2::new(pen.x1, pen.z1));
    low_wall(b, Vec2::new(pen.x0, pen.z1), Vec2::new(pen.x1, pen.z1));
    low_wall(b, Vec2::new(pen.x0, pen.z0), Vec2::new(pen.x0, pen.z0 + 5.0));
    low_wall(b, Vec2::new(pen.x0, pen.z1 - 5.0), Vec2::new(pen.x0, pen.z1));
    let hay = g(pen.x1 - 3.0, pen.z1 - 3.0);
    b.roof(Mat::Thatch, |m| m.lathe(hay - Vec3::Y * 0.2, &[(1.3, 0.0), (1.4, 0.8), (1.0, 1.8), (0.0, 2.4)], 10));
    kit::hidden(b, hay - Vec3::new(1.0, 0.2, 1.0), hay + Vec3::new(1.0, 2.0, 1.0));
}

fn goth_house(b: &mut Builder, p: Vec2, front: kit::Side, k: usize, rng: &mut Rng) {
    // Одна мазанка пустует (хозяева ушли в горы) — её можно купить.
    let vacant = k == 4;
    let along_x = front.along_x();
    let (w, d) = if along_x { (7.4, 5.4) } else { (5.4, 7.4) };
    let r = Rect::new(p.x - w / 2.0, p.y - d / 2.0, p.x + w / 2.0, p.y + d / 2.0);
    let y = terrace(b, r.inset(-1.2), Mat::Rubble);
    let seed = rng.int(1000) as u32;
    b.placed(Vec3::new(p.x, y, p.y), move |t| {
        let mut rng = Rng::new(seed);
        let r = Rect::new(-w / 2.0, -d / 2.0, w / 2.0, d / 2.0);
        let h = 2.3;
        let wt = 0.65;
        let door = [Op::Door { s: 0.0, w: 1.1, h: 1.9 }];
        let none: [Op; 0] = [];
        let ops = |s: kit::Side| -> &[Op] { if s == front { &door } else { &none } };
        kit::wall_x(t, r.x0, r.x1, r.z0 + wt / 2.0, wt, h, ops(kit::Side::N), Mat::Rubble);
        kit::wall_x(t, r.x0, r.x1, r.z1 - wt / 2.0, wt, h, ops(kit::Side::S), Mat::Rubble);
        kit::wall_z(t, r.z0 + wt, r.z1 - wt, r.x0 + wt / 2.0, wt, h, ops(kit::Side::W), Mat::Rubble);
        kit::wall_z(t, r.z0 + wt, r.z1 - wt, r.x1 - wt / 2.0, wt, h, ops(kit::Side::E), Mat::Rubble);
        // Конёк — вдоль длинной стороны.
        thatch_roof(t, r, h, 1.9, along_x, 0.6, Mat::Rubble);
        t.decor([r.x0 + wt, 0.0, r.z0 + wt], [r.x1 - wt, 0.03, r.z1 - wt], Mat::Earth);
        // Внутри: очаг в центре, лежанка у дальней стены, пифос, ткацкий станок или жернов.
        let back = -front.out() * (if along_x { d } else { w } / 2.0 - wt - 0.6);
        kit::hearth(t, Vec3::new(0.0, 0.0, 0.0) + front.out() * 0.3);
        if vacant {
            // Хозяева ушли в Дори: пустая мазанка с очагом и ларём — продаётся.
            let side = Vec3::new(front.out().z, 0.0, front.out().x).abs();
            let c = back + side * 1.5;
            let half = side * 0.5 + (Vec3::ONE - side - Vec3::Y) * 0.3;
            super::interior::chest(t, Rect::new(c.x - half.x, c.z - half.z, c.x + half.x, c.z + half.z), "Ларь");
            let room = super::estate::RoomPlan { r: r.inset(wt + 0.05), floor: 0.0, kinds: vec![crate::level::FurnishKind::Bedroom, crate::level::FurnishKind::Storeroom, crate::level::FurnishKind::Weaving, crate::level::FurnishKind::Workshop] };
            let entrance = front.out() * ((if along_x { d } else { w }) / 2.0 + 1.3);
            let (nd, nc) = (t.doors.len(), t.containers.len());
            super::estate::register(t, "Мазанка в готском селе".into(), crate::towns::TownId::GothVillage, r.inset(-1.0), h + 2.0, entrance, 0..nd, 0..nc, 0, vec![room], false, None);
            return;
        }
        let side = Vec3::new(front.out().z, 0.0, front.out().x).abs();
        let bed_c = back + side * 1.4 * if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        let half = side * 0.95 + (Vec3::ONE - side - Vec3::Y) * 0.45;
        kit::bed(t, Rect::new(bed_c.x - half.x, bed_c.z - half.z, bed_c.x + half.x, bed_c.z + half.z));
        let corner = back - side * 1.8 * if k.is_multiple_of(2) { 1.0 } else { -1.0 };
        kit::pithos(t, corner, 1.1, 0.2);
        if k.is_multiple_of(3) {
            kit::loom(t, back * 0.2 - side * 1.6, !along_x);
        } else {
            kit::hand_mill(t, back * 0.1 + side * 1.9);
        }
        // Хозяин или хозяйка у очага, сундук с добром.
        let role = if k.is_multiple_of(2) { Role::Farmer } else { Role::Housewife };
        let home = front.out() * 0.3;
        extras(t, r.inset(wt + 0.05), L::Household, &["Сундук"], &[(role, home - side * 1.2, home)], &mut rng);
        // Перед домом — поленница или плетень.
        let out = front.out() * ((if along_x { d } else { w }) / 2.0 + 1.2);
        if rng.chance(0.6) {
            let c = out + side * 2.3;
            let hs = side * 1.0 + (Vec3::ONE - side - Vec3::Y) * 0.3;
            kit::woodpile(t, Rect::new(c.x - hs.x, c.z - hs.z, c.x + hs.x, c.z + hs.z), 0.9);
        }
    });
}

// ---------------- кочевники ----------------

/// Стан гунно-булгар: войлочные юрты дверями к костру, кибитки, коновязь.
fn steppe_camp(b: &mut Builder) {
    let c = STEPPE_CAMP;
    let mut rng = Rng::new(463);
    for k in 0..6 {
        let a = k as f32 / 6.0 * TAU + 0.3;
        let p = Vec2::new(c.x + a.cos() * 16.0, c.z + a.sin() * 16.0);
        yurt(b, p, 3.0 + rng.range(0.0, 0.6), a + PI, &mut rng);
    }
    for (k, a) in [0.85f32, 2.95, 5.05].into_iter().enumerate() {
        let p = Vec2::new(c.x + a.cos() * 24.0, c.z + a.sin() * 24.0);
        wagon(b, p, a + FRAC_PI_2, k % 2 == 0);
    }
    campfire(b, g(c.x, c.z - 1.5));
    tether_line(b, Vec2::new(c.x + 8.0, c.z + 27.0), Vec2::new(c.x + 22.0, c.z + 24.0));
}

/// Юрта: решётчатая стена и купол из войлока на жердях, дверной проём на `door_yaw`.
fn yurt(b: &mut Builder, p: Vec2, r: f32, door_yaw: f32, rng: &mut Rng) {
    let y = [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)].iter().map(|(dx, dz)| ground_y(p.x + dx, p.y + dz)).fold(f32::MIN, f32::max);
    let base = Vec3::new(p.x, y, p.y);
    b.at(p.x, p.y);
    let wall_h = 1.6;
    let door_w = 0.55;
    // Войлочная стена — дугой, с разрывом под дверь.
    let gap = door_w / r;
    let wall = [(r, -0.6), (r, wall_h)];
    b.mesh(Mat::Felt).lathe_arc(base, &wall, door_yaw + gap, door_yaw + TAU - gap, 18, false);
    b.mesh(Mat::Felt).lathe_arc(base, &[(r - 0.08, -0.6), (r - 0.08, wall_h)], door_yaw + gap, door_yaw + TAU - gap, 18, true);
    // Купол и дымовое кольцо (тюндюк).
    let roof: Vec<(f32, f32)> = (0..=6).map(|k| {
        let t = k as f32 / 6.0;
        (r + 0.1 - t * (r - 0.45), wall_h + (1.0 - (1.0 - t).powi(2)) * r * 0.55)
    }).collect();
    b.roof(Mat::Felt, |m| m.lathe(base, &roof, 18));
    b.mesh(Mat::Wood).lathe(base + Vec3::Y * (roof[6].1 - 0.05), &[(0.5, 0.0), (0.5, 0.15), (0.38, 0.15), (0.38, 0.0)], 10);
    // Обвязка волосяными арканами.
    for hh in [0.6, 1.3] {
        b.mesh(Mat::Canvas).lathe_arc(base, &[(r + 0.03, hh), (r + 0.03, hh + 0.06)], door_yaw + gap, door_yaw + TAU - gap, 18, false);
    }
    // Дверная рама.
    let dir = Vec3::new(door_yaw.cos(), 0.0, door_yaw.sin());
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    let dp = base + dir * r;
    let m = b.mesh(Mat::Wood);
    for s in [-1.0f32, 1.0] {
        m.beam(dp + side * s * door_w - Vec3::Y * 0.6, dp + side * s * door_w + Vec3::Y * 1.5, 0.05, 0.05, dir);
    }
    m.beam(dp - side * (door_w + 0.05) + Vec3::Y * 1.5, dp + side * (door_w + 0.05) + Vec3::Y * 1.5, 0.05, 0.07, Vec3::Y);
    // Пол — войлочные кошмы; очаг в центре.
    b.mesh(Mat::Felt).disc(base + Vec3::Y * 0.04, r - 0.1, 14, true);
    kit::hearth(b, base + Vec3::Y * 0.02);
    if rng.chance(0.5) {
        b.loot(L::Household);
        kit::chest(b, base - dir * (r - 0.7), true);
    }
    // Коллизия стены: кольцо коробок с разрывом у двери.
    let n = 14;
    for k in 0..n {
        let a = door_yaw + (k as f32 + 0.5) / n as f32 * TAU;
        let rel = (a - door_yaw).rem_euclid(TAU);
        if rel < 0.4 || rel > TAU - 0.4 {
            continue;
        }
        let q = base + Vec3::new(a.cos(), 0.0, a.sin()) * r;
        kit::hidden(b, q - Vec3::new(0.55, 0.6, 0.55), q + Vec3::new(0.55, wall_h + 0.6, 0.55));
    }
}

/// Двухколёсная кибитка с войлочным верхом на дугах.
fn wagon(b: &mut Builder, p: Vec2, yaw: f32, covered: bool) {
    let y = ground_y(p.x, p.y);
    let base = Vec3::new(p.x, y, p.y);
    b.at(p.x, p.y);
    let fwd = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let side = Vec3::new(-fwd.z, 0.0, fwd.x);
    let (hl, hw) = (1.6, 0.8);
    let bed_y = 1.0;
    let m = b.mesh(Mat::Wood);
    m.obox(base + Vec3::Y * bed_y, Vec3::new(hl, 0.06, hw), yaw);
    for s in [-1.0f32, 1.0] {
        m.obox(base + Vec3::Y * (bed_y + 0.25) + side * s * hw, Vec3::new(hl, 0.25, 0.04), yaw);
        m.obox(base + Vec3::Y * (bed_y + 0.25) + fwd * s * hl, Vec3::new(0.04, 0.25, hw), yaw);
        // Оглобли.
        m.beam(base + Vec3::Y * bed_y + fwd * hl + side * s * 0.5, base + Vec3::Y * 0.5 + fwd * (hl + 2.4) + side * s * 0.4, 0.05, 0.05, Vec3::Y);
    }
    // Колёса со спицами.
    let wr = 0.85;
    for s in [-1.0f32, 1.0] {
        let hub = base + Vec3::Y * wr + side * s * (hw + 0.12);
        let rim: Vec<Vec3> = (0..=12).map(|k| {
            let a = k as f32 / 12.0 * TAU;
            hub + (fwd * a.cos() + Vec3::Y * a.sin()) * wr
        }).collect();
        m.beam_path(&rim, 0.06, 0.05, side);
        for k in 0..6 {
            let a = k as f32 / 6.0 * TAU;
            m.beam(hub, hub + (fwd * a.cos() + Vec3::Y * a.sin()) * wr, 0.025, 0.025, side);
        }
    }
    m.beam(base + Vec3::Y * wr - side * (hw + 0.2), base + Vec3::Y * wr + side * (hw + 0.2), 0.06, 0.06, Vec3::Y);
    if covered {
        let arch: Vec<Vec3> = (0..=8).map(|k| {
            let a = k as f32 / 8.0 * PI;
            Vec3::Y * (bed_y + 0.5 + a.sin() * 1.0) + side * a.cos() * hw
        }).collect();
        b.roof(Mat::Felt, |felt| {
            for w in arch.windows(2) {
                let (a, c) = (w[0], w[1]);
                let out = ((a + c) / 2.0 - Vec3::Y * (bed_y + 0.5)).normalize();
                felt.quad([base + a - fwd * hl, base + c - fwd * hl, base + c + fwd * hl, base + a + fwd * hl], out);
                felt.quad([base + a - fwd * hl, base + c - fwd * hl, base + c + fwd * hl, base + a + fwd * hl], -out);
            }
        });
        b.mesh(Mat::Felt).polygon(&arch.iter().map(|q| base + *q + fwd * hl).collect::<Vec<_>>(), &arch.iter().map(|q| Vec2::new(q.x, q.y)).collect::<Vec<_>>(), fwd);
    }
    let ext = (fwd * hl).abs() + (side * (hw + 0.3)).abs();
    kit::hidden(b, base + Vec3::new(-ext.x, 0.0, -ext.z), base + Vec3::new(ext.x, bed_y + 0.6, ext.z));
}

/// Костёр: кольцо камней, угли, огонь и свет.
fn campfire(b: &mut Builder, p: Vec3) {
    b.at(p.x, p.z);
    kit::hearth(b, p);
    let m = b.mesh(Mat::Wood);
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + 0.4;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        m.beam(p + d * 0.45 + Vec3::Y * 0.1, p + d * 0.05 + Vec3::Y * 0.35, 0.05, 0.05, Vec3::Y);
    }
    b.torch(p.x, p.y + 0.55, p.z, Vec3::ZERO);
    b.lamp(p + Vec3::Y * 1.0, Color::srgb(1.0, 0.6, 0.3), 30_000.0, 14.0);
    // Сиденья-брёвна.
    for k in 0..3 {
        let a = k as f32 / 3.0 * TAU + 1.0;
        let c = p + Vec3::new(a.cos(), 0.0, a.sin()) * 2.6;
        let t = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.9;
        let (ya, yc) = (ground_y((c - t).x, (c - t).z), ground_y((c + t).x, (c + t).z));
        b.mesh(Mat::Wood).beam(Vec3::new((c - t).x, ya + 0.2, (c - t).z), Vec3::new((c + t).x, yc + 0.2, (c + t).z), 0.2, 0.2, Vec3::Y);
    }
}

/// Коновязь: столбы и натянутая верёвка.
fn tether_line(b: &mut Builder, a: Vec2, c: Vec2) {
    b.at(a.x, a.y);
    let n = 4;
    let mut tops = Vec::new();
    for k in 0..=n {
        let p = a + (c - a) * (k as f32 / n as f32);
        let q = g(p.x, p.y);
        b.mesh(Mat::Wood).beam(q - Vec3::Y * 0.3, q + Vec3::Y * 1.3, 0.07, 0.07, Vec3::X);
        kit::hidden(b, q - Vec3::new(0.1, 0.3, 0.1), q + Vec3::new(0.1, 1.3, 0.1));
        tops.push(q + Vec3::Y * 1.2);
    }
    b.mesh(Mat::Canvas).beam_path(&tops, 0.015, 0.015, Vec3::Y);
}

// ---------------- аланы ----------------

/// Аланский стан: кольцо повозок (вагенбург), конические шатры из шкур на жердях, коновязь.
fn alan_camp(b: &mut Builder) {
    let c = ALAN_CAMP;
    for k in 0..7 {
        let a = k as f32 / 7.0 * TAU + 0.2;
        // Разрыв в кольце — к дороге (на запад).
        if (a - PI).abs() < 0.5 {
            continue;
        }
        let p = Vec2::new(c.x + a.cos() * 17.0, c.z + a.sin() * 17.0);
        wagon(b, p, a + FRAC_PI_2, k % 3 != 0);
    }
    let mut rng = Rng::new(375);
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + 0.8;
        let p = Vec2::new(c.x + a.cos() * 10.5, c.z + a.sin() * 10.5);
        cone_tent(b, p, 2.4 + rng.range(0.0, 0.5), a + PI);
    }
    campfire(b, g(c.x, c.z - 1.5));
    tether_line(b, Vec2::new(c.x + 22.0, c.z - 12.0), Vec2::new(c.x + 26.0, c.z + 4.0));
}

/// Конический шатёр: жерди, покрышка из шкур (войлок/кожа) с откинутым входом.
fn cone_tent(b: &mut Builder, p: Vec2, r: f32, door_yaw: f32) {
    let y = ground_y(p.x, p.y);
    let base = Vec3::new(p.x, y, p.y);
    b.at(p.x, p.y);
    let h = r * 1.5;
    let gap = 0.35;
    // Покрышка — и крыша, и стена: внутри тесно ровно настолько, насколько она клонится.
    b.roof(Mat::Felt, |m| m.lathe_arc(base - Vec3::Y * 0.4, &[(r * 1.08, 0.0), (r, 0.4), (0.12, h)], door_yaw + gap, door_yaw + TAU - gap, 14, false));
    b.mesh(Mat::Felt).lathe_arc(base - Vec3::Y * 0.4, &[(r * 1.08 - 0.06, 0.0), (r - 0.06, 0.4), (0.06, h - 0.1)], door_yaw + gap, door_yaw + TAU - gap, 14, true);
    let m = b.mesh(Mat::Wood);
    for k in 0..7 {
        let a = k as f32 / 7.0 * TAU;
        m.beam(base + Vec3::new(a.cos() * r, 0.0, a.sin() * r), base + Vec3::Y * (h + 0.4) - Vec3::new(a.cos() * 0.1, 0.0, a.sin() * 0.1), 0.03, 0.03, Vec3::Y);
    }
    kit::hearth(b, base);
    let n = 10;
    for k in 0..n {
        let a = door_yaw + (k as f32 + 0.5) / n as f32 * TAU;
        let rel = (a - door_yaw).rem_euclid(TAU);
        if rel < 0.5 || rel > TAU - 0.5 {
            continue;
        }
        let q = base + Vec3::new(a.cos(), 0.0, a.sin()) * (r - 0.2);
        kit::hidden(b, q - Vec3::new(0.5, 0.4, 0.5), q + Vec3::new(0.5, 1.6, 0.5));
    }
}

// ---------------- разбойники ----------------

/// Логово разбойников в балке: шалаш из жердей и веток, костровище, награбленные амфоры.
fn brigand_camps(b: &mut Builder) {
    for (x, z) in [(-20.0f32, 525.0f32), (232.0, 330.0)] {
        let p = g(x + 5.0, z + 3.0);
        b.at(p.x, p.z);
        // Шалаш: наклонные жерди на коньковой перекладине, сверху ветки.
        let (l, w) = (3.0, 1.6);
        let m = b.mesh(Mat::Wood);
        m.beam(p + Vec3::new(-l / 2.0, 1.6, 0.0), p + Vec3::new(l / 2.0, 1.6, 0.0), 0.05, 0.05, Vec3::Y);
        for s in [-1.0f32, 1.0] {
            m.beam(p + Vec3::new(s * l / 2.0, -0.2, -0.3), p + Vec3::new(s * l / 2.0, 1.7, 0.0), 0.05, 0.05, Vec3::X);
        }
        let leaves = b.mesh(Mat::Conifer);
        leaves.quad([p + Vec3::new(-l / 2.0, 0.0, w), p + Vec3::new(l / 2.0, 0.0, w), p + Vec3::new(l / 2.0, 1.65, 0.0), p + Vec3::new(-l / 2.0, 1.65, 0.0)], Vec3::new(0.0, 1.0, 1.0));
        leaves.quad([p + Vec3::new(-l / 2.0, 0.0, w), p + Vec3::new(l / 2.0, 0.0, w), p + Vec3::new(l / 2.0, 1.65, 0.0), p + Vec3::new(-l / 2.0, 1.65, 0.0)], Vec3::new(0.0, -1.0, -1.0));
        kit::hidden(b, p + Vec3::new(-l / 2.0, -0.2, 0.6), p + Vec3::new(l / 2.0, 1.0, w));
        let f = g(x + 5.0, z - 1.0);
        kit::hearth(b, f);
        for k in 0..3 {
            kit::amphora_lying(b, g(x + 2.0 + k as f32 * 0.5, z + 4.5) + Vec3::Y * 0.12, false, 1.0);
        }
    }
}

// ---------------- травы ----------------

/// Заросли лекарственных трав (см. [`places::herb_patches`]): у каждого вида — свой облик.
fn herbs(b: &mut Builder) {
    for (i, h) in places::herb_patches().iter().enumerate() {
        let mut rng = Rng::new(9000 + i as u32 * 31);
        b.at(h.pos.x, h.pos.z);
        let n = match h.herb {
            "quercus" => 4,
            "juniperus" => 7,
            "rosa" => 6,
            _ => (h.radius * h.radius * 1.2) as usize + 6,
        };
        for _ in 0..n {
            let a = rng.range(0.0, TAU);
            let d = h.radius * rng.f().sqrt();
            let p = g(h.pos.x + a.cos() * d, h.pos.z + a.sin() * d) - Vec3::Y * 0.03;
            herb_plant(b, p, h.herb, &mut rng);
        }
    }
}

/// Пучок стеблей с соцветиями.
fn stems(b: &mut Builder, p: Vec3, n: usize, h: f32, spread: f32, flower: Mat, head: f32, rng: &mut Rng) {
    for _ in 0..n {
        let a = rng.range(0.0, TAU);
        let top = p + Vec3::new(a.cos() * spread, h * rng.range(0.75, 1.0), a.sin() * spread);
        b.mesh(Mat::HerbGreen).beam(p, top, 0.012, 0.012, Vec3::X);
        if head > 0.0 {
            b.mesh(flower).lathe(top - Vec3::Y * head * 0.5, &[(0.0, 0.0), (head, head * 0.4), (head * 0.8, head), (0.0, head * 1.1)], 5);
        }
    }
}

/// Кустик травы: пучок узких листьев, расходящихся от корня и поникающих на концах
/// (раньше был гладкий гранёный «холмик»). Раскладка листьев — от положения куста.
fn mound(b: &mut Builder, p: Vec3, r: f32, h: f32, mat: Mat) {
    let hash = |k: f32| {
        let v = ((p.x * 12.9898 + p.z * 78.233 + k * 37.719).sin() * 43758.547).abs();
        v - v.floor()
    };
    let m = b.mesh(mat);
    let n = 11;
    for k in 0..n {
        let f = k as f32;
        let a = (f + hash(f)) / n as f32 * TAU * 1.9;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        // Внутренние листья стоят круче и выше, наружные лежат шире.
        let out = 0.35 + 0.65 * (f / n as f32);
        let len = r * (0.75 + 0.5 * hash(f + 20.0)) * out;
        let top = h * (1.15 - 0.55 * out) * (0.8 + 0.4 * hash(f + 40.0));
        let root = p + dir * (r * 0.08);
        let mid = root + dir * (len * 0.65) + Vec3::Y * top;
        let tip = mid + dir * (len * 0.5) - Vec3::Y * (top * 0.3);
        let w = (r * 0.11).clamp(0.015, 0.05);
        let up = Vec3::Y.cross(dir).cross(mid - root).normalize_or(Vec3::Y);
        m.beam(root, mid, w, 0.006, up);
        m.beam(mid, tip, w * 0.6, 0.005, up);
    }
}

/// Розетка прикорневых листьев (подорожник).
fn rosette(b: &mut Builder, p: Vec3, r: f32, rng: &mut Rng) {
    let m = b.mesh(Mat::HerbGreen);
    let n = 6;
    let a0 = rng.range(0.0, TAU);
    for k in 0..n {
        let a = a0 + k as f32 / n as f32 * TAU;
        m.obox(p + Vec3::new(a.cos() * r * 0.5, 0.03, a.sin() * r * 0.5), Vec3::new(r * 0.5, 0.01, r * 0.18), a);
    }
}

fn herb_plant(b: &mut Builder, p: Vec3, herb: &str, rng: &mut Rng) {
    let s = rng.range(0.75, 1.25);
    match herb {
        "salvia" => {
            mound(b, p, 0.35 * s, 0.45 * s, Mat::HerbSilver);
            stems(b, p + Vec3::Y * 0.3 * s, 5, 0.4 * s, 0.15, Mat::PetalPurple, 0.04, rng);
        }
        "thymus" => {
            mound(b, p, 0.28 * s, 0.12 * s, Mat::HerbGreen);
            b.mesh(Mat::PetalPink).lathe(p + Vec3::Y * 0.06 * s, &[(0.0, 0.0), (0.24 * s, 0.02), (0.18 * s, 0.07 * s), (0.0, 0.09 * s)], 7);
        }
        "hypericum" => stems(b, p, 6, 0.6 * s, 0.12, Mat::PetalYellow, 0.05, rng),
        "hyoscyamus" => {
            mound(b, p, 0.32 * s, 0.3 * s, Mat::HerbGreen);
            stems(b, p, 3, 0.7 * s, 0.1, Mat::PetalYellow, 0.06, rng);
        }
        "juniperus" => kit::juniper(b, p, rng.range(1.2, 2.8), rng),
        "artemisia" => {
            mound(b, p, 0.4 * s, 0.6 * s, Mat::HerbSilver);
            stems(b, p, 4, 0.75 * s, 0.2, Mat::HerbSilver, 0.05, rng);
        }
        "achillea" => {
            rosette(b, p, 0.18 * s, rng);
            for _ in 0..3 {
                let a = rng.range(0.0, TAU);
                let top = p + Vec3::new(a.cos() * 0.08, 0.55 * s, a.sin() * 0.08);
                b.mesh(Mat::HerbGreen).beam(p, top, 0.01, 0.01, Vec3::X);
                b.mesh(Mat::PetalWhite).lathe(top, &[(0.0, 0.0), (0.1, 0.02), (0.09, 0.04), (0.0, 0.045)], 7);
            }
        }
        "plantago" => {
            rosette(b, p, 0.22 * s, rng);
            stems(b, p, 3, 0.3 * s, 0.05, Mat::HerbGreen, 0.02, rng);
        }
        "rosa" => {
            let h = 1.3 * s;
            kit::shrub(b, p, h);
            for _ in 0..10 {
                let a = rng.range(0.0, TAU);
                let y = rng.range(0.3, 0.9) * h;
                let r = 0.8 * s * (1.0 - (y / h - 0.45).powi(2) * 1.6).max(0.3);
                b.mesh(Mat::PetalPink).lathe(p + Vec3::new(a.cos() * r, y, a.sin() * r), &[(0.0, 0.0), (0.05, 0.02), (0.0, 0.04)], 5);
            }
            kit::hidden(b, p - Vec3::new(0.5, 0.0, 0.5), p + Vec3::new(0.5, h * 0.8, 0.5));
        }
        "malva" => {
            mound(b, p, 0.25 * s, 0.25 * s, Mat::HerbGreen);
            stems(b, p, 4, 0.8 * s, 0.12, Mat::PetalPurple, 0.05, rng);
        }
        "papaver" => stems(b, p, 3, 0.5 * s, 0.08, Mat::PetalRed, 0.06, rng),
        "mentha" => {
            mound(b, p, 0.3 * s, 0.35 * s, Mat::HerbGreen);
            stems(b, p + Vec3::Y * 0.2, 4, 0.25 * s, 0.15, Mat::PetalPurple, 0.03, rng);
        }
        "rubia" => {
            mound(b, p, 0.5 * s, 0.25 * s, Mat::HerbGreen);
            stems(b, p + Vec3::Y * 0.15, 5, 0.15 * s, 0.35, Mat::PetalYellow, 0.02, rng);
        }
        "isatis" => {
            rosette(b, p, 0.2 * s, rng);
            stems(b, p, 4, 1.0 * s, 0.1, Mat::PetalYellow, 0.12, rng);
        }
        "quercus" => {
            if rng.chance(0.5) {
                kit::oak(b, p, rng.range(6.0, 9.0), rng);
            } else {
                kit::shrub(b, p, 1.6);
            }
        }
        "symphytum" => {
            mound(b, p, 0.45 * s, 0.6 * s, Mat::HerbGreen);
            stems(b, p + Vec3::Y * 0.3, 4, 0.4 * s, 0.2, Mat::PetalPurple, 0.04, rng);
        }
        "inula" => {
            mound(b, p, 0.4 * s, 0.6 * s, Mat::HerbGreen);
            stems(b, p, 3, 1.6 * s, 0.15, Mat::PetalYellow, 0.09, rng);
        }
        _ => mound(b, p, 0.3 * s, 0.3 * s, Mat::HerbGreen),
    }
}

// ---------------- лес, кусты, камни ----------------

/// Шибляк и редколесье плато: можжевельник высокий, крымская сосна на гребнях,
/// дуб пушистый ближе к предгорьям; известняковые выходы. Вокруг стен — чисто (обзор для стражи).
fn wilderness(b: &mut Builder, roads: &[Vec<Vec2>]) {
    let mut clear: Vec<(Vec2, f32)> = vec![
        (Vec2::new(NECROPOLIS.x + 10.0, NECROPOLIS.z + 2.0), 72.0),
        (Vec2::new(-197.0, 270.0), 80.0),
        (Vec2::new(STEPPE_CAMP.x, STEPPE_CAMP.z), 34.0),
        (Vec2::new(GOTH_VILLAGE.x + 8.0, GOTH_VILLAGE.z), 40.0),
        (Vec2::new(ALAN_CAMP.x, ALAN_CAMP.z), 34.0),
        (Vec2::new(places::spring_pos().x, places::spring_pos().z), 9.0),
    ];
    for p in places::npc_posts() {
        clear.push((Vec2::new(p.pos.x, p.pos.z), 6.0));
    }
    for h in places::herb_patches() {
        clear.push((Vec2::new(h.pos.x, h.pos.z), h.radius + 2.0));
    }
    // Рыбацкая хижина с причалом и подход к ней.
    clear.push((Vec2::new(fishing::HUT.cx(), fishing::HUT.cz()), 20.0));
    clear.extend(mines::clearings());
    clear.push(village::clearing());
    // Деревьям у стен нельзя (обзор для стражи), а низким кустам можно — с 14 м от стены.
    let blocked_low = |p: Vec2| {
        terrain::city_distance(p.x, p.y) < 14.0
            || p.x.abs() > WORLD_HALF - 12.0
            || p.y.abs() > WORLD_HALF - 12.0
            || ground_y(p.x, p.y) < WATER_Y + 1.2
            || road_distance(roads, p) < ROAD_W / 2.0 + 1.5
            || clear.iter().any(|(c, r)| p.distance(*c) < *r)
    };
    let blocked = |p: Vec2| {
        terrain::city_distance(p.x, p.y) < 40.0
            || p.x.abs() > WORLD_HALF - 12.0
            || p.y.abs() > WORLD_HALF - 12.0
            || ground_y(p.x, p.y) < WATER_Y + 1.6
            || road_distance(roads, p) < ROAD_W / 2.0 + 2.5
            || clear.iter().any(|(c, r)| p.distance(*c) < *r)
    };
    let mut rng = Rng::new(1783);
    // Зелени много: рощи сливаются в редколесье, между ними — одиночные деревья и кусты.
    let step = 15.0;
    let mut z = -WORLD_HALF + 10.0;
    while z < WORLD_HALF {
        let mut x = -WORLD_HALF + 10.0;
        while x < WORLD_HALF {
            let p = Vec2::new(x + rng.range(-6.5, 6.5), z + rng.range(-6.5, 6.5));
            x += step;
            let density = terrain::fbm(p.x * 0.007, p.y * 0.007, 77);
            let chance = ((density - 0.34) * 2.6).clamp(0.07, 0.92);
            if !rng.chance(chance) || blocked(p) {
                if blocked(p) {
                    // Полоса у стен и у берега: только кусты, зато чаще — не голая земля.
                    if !blocked_low(p) && rng.chance(0.45) {
                        b.at(p.x, p.y);
                        for _ in 0..1 + rng.int(3) {
                            let s = p + Vec2::new(rng.range(-3.5, 3.5), rng.range(-3.5, 3.5));
                            if !blocked_low(s) {
                                kit::shrub(b, g(s.x, s.y) - Vec3::Y * 0.05, rng.range(0.6, 1.5));
                            }
                        }
                    }
                    continue;
                }
                // На открытых местах — камни и куртины кустов.
                if rng.chance(0.11) {
                    rock(b, p, rng.range(0.5, 1.8), &mut rng);
                } else if rng.chance(0.27) {
                    b.at(p.x, p.y);
                    for _ in 0..1 + rng.int(2) {
                        let s = p + Vec2::new(rng.range(-2.5, 2.5), rng.range(-2.5, 2.5));
                        if !blocked(s) {
                            kit::shrub(b, g(s.x, s.y) - Vec3::Y * 0.05, rng.range(0.7, 1.8));
                        }
                    }
                }
                continue;
            }
            b.at(p.x, p.y);
            let up = ((p.y - 300.0) / 400.0).clamp(0.0, 1.0);
            // В густой роще деревья стоят по два-три.
            let grove = density > 0.55;
            let trees = if grove { 3 + rng.int(3) } else if density > 0.48 { 2 + rng.int(2) } else { 1 + rng.chance(0.25) as usize };
            for k in 0..trees {
                let tp = if k == 0 { p } else { p + Vec2::new(rng.range(-7.0, 7.0), rng.range(-7.0, 7.0)) };
                if k > 0 && (blocked(tp) || tp.distance(p) < 2.5) {
                    continue;
                }
                let q = g(tp.x, tp.y) - Vec3::Y * 0.1;
                let roll = rng.f();
                // На самом плато сосна редка (крымская сосна — дерево гор), больше можжевельника,
                // дуба пушистого и диких груш (их изображает тот же лиственный вид).
                if grove && roll > 0.9 {
                    // Тёмные свечи кипарисов среди бора.
                    kit::cypress(b, q, rng.range(7.0, 11.0));
                } else if roll < 0.5 - up * 0.3 {
                    kit::juniper(b, q, rng.range(2.0, 4.5), &mut rng);
                } else if roll < 0.6 - up * 0.1 + if grove { 0.2 } else { 0.0 } {
                    // В сердце рощи — сосновый бор: тёмная стена хвои.
                    kit::pine(b, q, rng.range(7.0, 13.0), &mut rng);
                } else {
                    kit::oak(b, q, rng.range(6.0, 10.0), &mut rng);
                }
            }
            // Подлесок: кусты шибляка вокруг деревьев.
            for _ in 0..1 + rng.int(3) {
                let a = rng.range(0.0, TAU);
                let d = rng.range(1.8, 5.0);
                let s = Vec2::new(p.x + a.cos() * d, p.y + a.sin() * d);
                if !blocked(s) {
                    kit::shrub(b, g(s.x, s.y) - Vec3::Y * 0.05, rng.range(0.9, 2.0));
                }
            }
        }
        z += step;
    }
    // Берега Карантинной бухты: хвойный бор стеной по кромке обрывов и валуны у самой воды.
    // Свой счётчик случайных чисел — остальная зелень от этого не сдвигается.
    let mut rng = Rng::new(9127);
    let near_water = |p: Vec2, r: f32| (0..8).any(|k| ground_y(p.x + (k as f32 * TAU / 8.0).cos() * r, p.y + (k as f32 * TAU / 8.0).sin() * r) < WATER_Y);
    let mut z = 140.0;
    while z < 480.0 {
        let mut x = 270.0;
        while x < 540.0 {
            let p = Vec2::new(x + rng.range(-3.0, 3.0), z + rng.range(-3.0, 3.0));
            x += 7.0;
            let (roll, size, yaw, variant) = (rng.f(), rng.f(), rng.range(0.0, TAU), rng.int(64) as u32);
            let y = ground_y(p.x, p.y);
            if y < WATER_Y - 0.3 {
                continue;
            }
            if y < WATER_Y + 1.7 {
                // Полоса прибоя: валуны, поодиночке и кучками.
                if roll < 0.2 && terrain::city_distance(p.x, p.y) > 14.0 && road_distance(roads, p) > ROAD_W / 2.0 + 1.5 && near_water(p, 4.0) {
                    rock(b, p, 0.7 + size * size * 2.4, &mut rng);
                }
                continue;
            }
            let chance = if near_water(p, 22.0) { 0.7 } else if near_water(p, 48.0) { 0.5 } else if near_water(p, 75.0) { 0.22 } else { 0.0 };
            // На отвесной скале деревья не растут.
            let steep = (ground_y(p.x + 1.5, p.y) - y).abs().max((ground_y(p.x, p.y + 1.5) - y).abs()) > 1.6;
            if roll >= chance || steep || blocked(p) {
                continue;
            }
            b.at(p.x, p.y);
            let q = g(p.x, p.y) - Vec3::Y * 0.15;
            let h = 9.0 + size * 8.5;
            b.plant(PlantKind::Fir, q, h, yaw, variant);
            b.invisible.push(Aabb::new(q - Vec3::new(0.25, 0.0, 0.25), q + Vec3::new(0.25, h * 0.5, 0.25)));
        }
        z += 7.0;
    }
    // Каменные выходы на крутых склонах по всему плато: глыбы и осыпь вместо голой земли.
    // Опять свой счётчик: деревья и кусты остаются на своих местах.
    let mut rng = Rng::new(4471);
    let mut z = -WORLD_HALF + 14.0;
    while z < WORLD_HALF - 14.0 {
        let mut x = -WORLD_HALF + 14.0;
        while x < WORLD_HALF - 14.0 {
            let p = Vec2::new(x + rng.range(-2.6, 2.6), z + rng.range(-2.6, 2.6));
            x += 5.5;
            let (roll, size) = (rng.f(), rng.f());
            let y = ground_y(p.x, p.y);
            let rise = (ground_y(p.x + 1.5, p.y) - y).abs().max((ground_y(p.x, p.y + 1.5) - y).abs());
            if roll > 0.42 || y < WATER_Y + 0.4 || !(0.75..3.2).contains(&rise) || terrain::city_distance(p.x, p.y) < 14.0 || road_distance(roads, p) < ROAD_W / 2.0 + 2.0 || clear.iter().any(|(c, r)| p.distance(*c) < *r) {
                continue;
            }
            rock(b, p, 0.5 + size * size * 2.2, &mut rng);
        }
        z += 5.5;
    }
}

/// Известняковый выход: фотоскан глыбы, наполовину в земле, иногда с камнями поменьше.
fn rock(b: &mut Builder, p: Vec2, size: f32, rng: &mut Rng) {
    let y = ground_y(p.x, p.y);
    b.at(p.x, p.y);
    let n = 1 + rng.int(3);
    for k in 0..n {
        let off = if k == 0 { Vec2::ZERO } else { Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * size * 1.2 };
        let s = size * if k == 0 { 1.0 } else { rng.range(0.3, 0.6) };
        let q = Vec2::new(p.x + off.x, p.y + off.y);
        // На откосе глыба сидит в склоне по нижнему краю, а не висит над ним.
        let r = s * 0.35;
        let low = [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)].iter().map(|(dx, dz)| ground_y(q.x + dx, q.y + dz)).fold(f32::MAX, f32::min);
        let c = Vec3::new(q.x, low - s * 0.2, q.y);
        b.plant(PlantKind::Rock, c, s, rng.range(0.0, TAU), rng.int(64) as u32);
        if s > 0.7 {
            let r = s * 0.45;
            b.invisible.push(Aabb::new(Vec3::new(c.x - r, y - 0.3, c.z - r), Vec3::new(c.x + r, c.y + s * 0.6, c.z + r)));
        }
    }
}

// ---------------- кузницы за стенами ----------------

/// Кузница готского селения — на западном краю, подальше от соломенных крыш.
pub fn goth_smithy_rect() -> Rect {
    let c = GOTH_VILLAGE;
    Rect::new(c.x - 38.0, c.z - 7.0, c.x - 29.0, c.z)
}

/// Сыродутный горн готов, рядом с кузницей.
pub fn goth_bloomery_rect() -> Rect {
    let c = GOTH_VILLAGE;
    Rect::new(c.x - 37.0, c.z + 4.5, c.x - 31.0, c.z + 10.0)
}

/// Полевой горн кочевников между юртами.
pub fn steppe_forge_rect() -> Rect {
    let c = STEPPE_CAMP;
    Rect::new(c.x + 3.4, c.z + 4.6, c.x + 8.8, c.z + 8.8)
}

/// Горн аланов внутри кольца повозок.
pub fn alan_forge_rect() -> Rect {
    let c = ALAN_CAMP;
    Rect::new(c.x - 2.6, c.z - 10.4, c.x + 2.6, c.z - 6.6)
}

/// Высота пола площадки кузницы за стенами.
pub fn smith_floor(r: Rect) -> f32 {
    terrace_levels(r.inset(-0.6)).1
}

fn goth_smithy(b: &mut Builder) {
    let r = goth_smithy_rect();
    let y = terrace(b, r.inset(-0.6), Mat::Rubble);
    let (w, d) = (r.w(), r.d());
    b.placed(Vec3::new(r.cx(), y, r.cz()), move |t| {
        let l = Rect::new(-w / 2.0, -d / 2.0, w / 2.0, d / 2.0);
        // Навес на столбах под камышом: в кузнице должно продувать.
        for (x, z) in [(l.x0, l.z0), (l.x1, l.z0), (l.x0, l.z1), (l.x1, l.z1)] {
            t.solid([x - 0.12, 0.0, z - 0.12], [x + 0.12, 2.5, z + 0.12], Mat::Wood);
        }
        thatch_roof(t, l, 2.5, 1.5, true, 0.5, Mat::Wood);
        t.decor([l.x0, 0.0, l.z0], [l.x1, 0.03, l.z1], Mat::Earth);
        kit::forge_set(t, "Готская кузница", Vec3::new(l.x0 + 1.8, 0.0, l.z0 + 2.2), Vec3::X, true);
        kit::woodpile(t, Rect::new(l.x1 - 2.6, l.z1 - 0.9, l.x1 - 0.3, l.z1 - 0.3), 0.9);
    });
    let rb = goth_bloomery_rect();
    let y = terrace(b, rb.inset(-0.6), Mat::Rubble);
    b.placed(Vec3::new(rb.cx(), y, rb.cz()), |t| {
        t.decor([-3.0, 0.0, -2.75], [3.0, 0.03, 2.75], Mat::Earth);
        kit::bloomery(t, "Сыродутный горн готов", Vec3::new(-1.0, 0.0, 0.0), Vec3::X);
    });
}

/// Полевой горн: камни и глина, кожаные мехи, наковальня на колоде.
fn camp_forge(b: &mut Builder, r: Rect, name: &'static str) {
    let y = terrace(b, r.inset(-0.6), Mat::Earth);
    let (w, d) = (r.w(), r.d());
    b.placed(Vec3::new(r.cx(), y, r.cz()), move |t| {
        let l = Rect::new(-w / 2.0, -d / 2.0, w / 2.0, d / 2.0);
        kit::forge_set(t, name, Vec3::new(l.x0 + 1.3, 0.0, l.cz() + 0.15), Vec3::X, true);
    });
}
