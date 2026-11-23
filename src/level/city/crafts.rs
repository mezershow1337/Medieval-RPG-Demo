//! Ремёсла: рыбозасолочные цистерны, стекольная мастерская, гончарные печи, кузница, винодельня.

use super::kit::{self, Op, Rect, Rng, Side};
use super::public::{column, floor_around, snap_hole};
use super::*;
use super::interior::extras;
use crate::items::LootTableId as L;
use crate::level::Role;
use crate::collision::Aabb;

/// Ванна-цистерна в земле: стенки и дно из цемянки, ступени в углу, вода (рассол).
fn sunk_tank(b: &mut Builder, r: Rect, depth: f32, water: f32) {
    let t = 0.5;
    b.solid([r.x0 - t, -depth - 0.4, r.z0 - t], [r.x1 + t, -depth, r.z1 + t], Mat::Signinum);
    for (min, max) in [
        ([r.x0 - t, -depth, r.z0 - t], [r.x1 + t, 0.12, r.z0]),
        ([r.x0 - t, -depth, r.z1], [r.x1 + t, 0.12, r.z1 + t]),
        ([r.x0 - t, -depth, r.z0], [r.x0, 0.12, r.z1]),
        ([r.x1, -depth, r.z0], [r.x1 + t, 0.12, r.z1]),
    ] {
        b.solid(min, max, Mat::Signinum);
    }
    // Ступени, чтобы выбраться (по ним спускались чистить цистерну).
    let n = (depth / 0.45).ceil() as usize;
    for k in 0..n {
        let top = -depth + 0.45 * (k + 1) as f32;
        b.solid([r.x0, -depth, r.z0 + k as f32 * 0.4], [r.x0 + 0.9, top.min(0.0), r.z0 + (k + 1) as f32 * 0.4], Mat::Limestone);
    }
    b.decor([r.x0, -depth, r.z0], [r.x1, -water, r.z1], Mat::Water);
}

/// Рыбозасолочный комплекс: ряды цистерн (рыбу засыпали солью слоями), навес на столбах,
/// склад амфор для солёной рыбы, вешала для вяления, кучи соли.
pub fn fish_salting(b: &mut Builder, r: Rect, variant: u32) {
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(900 + variant);
    let cols = if r.w() > 30.0 { 4 } else { 3 };
    let (tw, td, gap) = (3.6, 3.0, 0.6);
    let gw = cols as f32 * tw + (cols + 1) as f32 * gap;
    let gd = 2.0 * td + 3.0 * gap;
    let g = Rect::new(r.cx() - gw / 2.0, r.z0 + 8.0, r.cx() + gw / 2.0, r.z0 + 8.0 + gd);
    let hole = snap_hole(g.inset(-0.5));
    b.holes.push((Vec2::new(hole.x0, hole.z0), Vec2::new(hole.x1, hole.z1)));
    floor_around(b, hole, g);
    for row in 0..2 {
        for c in 0..cols {
            let x0 = g.x0 + gap + c as f32 * (tw + gap);
            let z0 = g.z0 + gap + row as f32 * (td + gap);
            sunk_tank(b, Rect::new(x0, z0, x0 + tw, z0 + td), 2.4, 0.6);
        }
    }
    // Навес над цистернами.
    let sh = g.inset(-1.0);
    for (x, z) in [(sh.x0, sh.z0), (sh.x1, sh.z0), (sh.x0, sh.z1), (sh.x1, sh.z1), (sh.cx(), sh.z0), (sh.cx(), sh.z1)] {
        b.solid([x - 0.15, 0.0, z - 0.15], [x + 0.15, 3.6, z + 0.15], Mat::Wood);
    }
    kit::gable_roof(b, sh, 3.6, 1.6, true, 0.3, Mat::Wood);
    // Склад: амфоры штабелями, кучи соли.
    let st = Rect::new(r.x0 + 1.0, g.z1 + 4.0, r.x0 + 12.0, (g.z1 + 13.0).min(r.z1 - 1.0));
    // Дверь — к цистернам; если там вплотную городская стена — с востока.
    let door = [Op::Door { s: st.cx(), w: 1.6, h: 2.5 }];
    let side_door = [Op::Door { s: st.cz(), w: 1.6, h: 2.5 }];
    let to_wall = edge_distance(st.cx(), st.z0 - 1.0) < 4.0;
    let none: [Op; 0] = [];
    kit::wall_x(b, st.x0, st.x1, st.z0 + 0.3, 0.6, 3.4, if to_wall { &none } else { &door }, Mat::Rubble);
    kit::wall_x(b, st.x0, st.x1, st.z1 - 0.3, 0.6, 3.4, &[], Mat::Rubble);
    kit::wall_z(b, st.z0 + 0.6, st.z1 - 0.6, st.x0 + 0.3, 0.6, 3.4, &[], Mat::Rubble);
    kit::wall_z(b, st.z0 + 0.6, st.z1 - 0.6, st.x1 - 0.3, 0.6, 3.4, if to_wall { &side_door } else { &none }, Mat::Rubble);
    kit::gable_roof(b, st, 3.4, 1.4, true, 0.4, Mat::Rubble);
    for row in 0..3 {
        for k in 0..12 {
            let p = Vec3::new(st.x0 + 1.0 + k as f32 * 0.38, 0.17 + row as f32 * 0.3, st.z1 - 1.2);
            kit::amphora_lying(b, p, false, 1.0);
        }
    }
    b.invisible.push(Aabb::new(Vec3::new(st.x0 + 0.7, 0.0, st.z1 - 1.6), Vec3::new(st.x0 + 5.5, 1.0, st.z1 - 0.6)));
    for k in 0..2 {
        let p = Vec3::new(r.x1 - 3.0 - k as f32 * 3.5, 0.0, g.z1 + 5.0);
        b.mesh(Mat::Plaster).lathe(p, &[(1.3, 0.0), (0.9, 0.6), (0.0, 1.1)], 12);
        kit::hidden(b, p - Vec3::new(0.9, 0.0, 0.9), p + Vec3::new(0.9, 0.8, 0.9));
    }
    // Вешала для рыбы.
    for k in 0..3 {
        let z = g.z0 - 3.0 - k as f32 * 1.6;
        if z < r.z0 + 0.5 {
            break;
        }
        let (x0, x1) = (g.x0, g.x0 + 8.0);
        let m = b.mesh(Mat::Wood);
        for x in [x0, x1] {
            m.beam(Vec3::new(x, 0.0, z), Vec3::new(x, 1.9, z), 0.04, 0.04, Vec3::X);
        }
        m.beam(Vec3::new(x0, 1.85, z), Vec3::new(x1, 1.85, z), 0.025, 0.025, Vec3::Y);
        let n = 22;
        for i in 0..n {
            let x = x0 + 0.2 + i as f32 * (x1 - x0 - 0.4) / n as f32;
            b.mesh(Mat::Iron).beam(Vec3::new(x, 1.8, z), Vec3::new(x, 1.45 + rng.range(-0.05, 0.05), z), 0.035, 0.012, Vec3::X);
        }
    }
    // Ограда комплекса со стороны улиц.
    kit::wall_x(b, r.x0, r.x1, r.z0 + 0.3, 0.6, 2.6, &[Op::Door { s: r.cx() + 5.0, w: 2.4, h: 2.4 }], Mat::Rubble);
    kit::wall_x(b, r.x0, r.x1, r.z1 - 0.3, 0.6, 2.6, &[Op::Door { s: r.cx(), w: 2.4, h: 2.4 }], Mat::Rubble);
    extras(b, r.inset(1.0), L::Storehouse, &["Корзина с рыбой"], &[(Role::Craftsman, r.center(0.0) + Vec3::new(-3.0, 0.0, 2.0), r.center(0.0)), (Role::Craftsman, r.center(0.0) + Vec3::new(4.0, 0.0, -2.0), r.center(0.0))], &mut rng);
}

pub fn fish_salting_point() -> Vec3 {
    let r = block(18, -3).union(&block(19, -3));
    Vec3::new(r.cx(), 0.0, r.z0 + 5.0)
}

// ---------------- стекольная мастерская ----------------

pub fn glassworks_rect() -> Rect {
    block(12, -3)
}

/// Стеклоделы V–VI вв. на северном берегу: купольная печь с рабочими окнами, печь для отжига,
/// стол с трубками-выдувалками, полки с сосудами, бой стекла на переплавку.
pub fn glassworks(b: &mut Builder) {
    let r = glassworks_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(4545);
    let shed = Rect::new(r.x0 + 2.0, r.z0 + 6.0, r.x1 - 2.0, r.z0 + 20.0);
    // Мастерская открыта с юга (жар): навес на столбах, задняя стена.
    kit::wall_x(b, shed.x0, shed.x1, shed.z0 + 0.3, 0.6, 4.0, &[], Mat::Rubble);
    kit::wall_z(b, shed.z0 + 0.6, shed.z1, shed.x0 + 0.3, 0.6, 4.0, &[], Mat::Rubble);
    kit::wall_z(b, shed.z0 + 0.6, shed.z1, shed.x1 - 0.3, 0.6, 4.0, &[], Mat::Rubble);
    for k in 0..4 {
        let x = shed.x0 + 1.0 + k as f32 * (shed.w() - 2.0) / 3.0;
        column(b, Vec3::new(x, 0.0, shed.z1 - 0.3), 3.6, 0.2, Mat::Limestone);
    }
    kit::lean_roof(b, shed, Side::N, 5.0, 3.7, 0.5);
    // Главная печь: купол из плинфы с тремя светящимися рабочими окнами.
    let f = Vec3::new(shed.cx() - 2.5, 0.0, shed.cz() - 1.0);
    let prof: Vec<(f32, f32)> = (0..=8).map(|k| {
        let a = k as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
        (1.7 * a.cos(), 0.9 + 1.5 * a.sin())
    }).collect();
    let mut full = vec![(1.75, 0.0), (1.75, 0.9)];
    full.extend(prof);
    b.mesh(Mat::Brick).lathe(f, &full, 16);
    kit::hidden(b, f - Vec3::new(1.4, 0.0, 1.4), f + Vec3::new(1.4, 2.2, 1.4));
    for k in 0..3 {
        let a = 0.6 + k as f32 * 1.0;
        let q = f + Vec3::new(a.cos() * 1.72, 1.1, a.sin() * 1.72);
        b.mesh(Mat::Coals).obox(q, Vec3::new(0.05, 0.2, 0.18), a);
    }
    b.lamp(f + Vec3::new(1.2, 1.2, 1.6), Color::srgb(1.0, 0.5, 0.18), 30_000.0, 9.0);
    // Печь для отжига.
    let a = Vec3::new(shed.cx() + 3.5, 0.0, shed.cz() - 2.0);
    b.solid([a.x - 1.2, 0.0, a.z - 0.8], [a.x + 1.2, 1.3, a.z + 0.8], Mat::Brick);
    b.mesh(Mat::Brick).barrel_vault(Vec2::new(a.x - 1.2, a.z - 0.8), Vec2::new(a.x + 1.2, a.z + 0.8), 1.3, true, 0.12);
    b.mesh(Mat::Coals).cuboid(Vec3::new(a.x - 0.4, 0.4, a.z + 0.79), Vec3::new(a.x + 0.4, 0.8, a.z + 0.82));
    // Верстак с выдувными трубками и готовыми сосудами.
    let t = Rect::new(shed.x0 + 1.5, shed.z1 - 3.0, shed.x0 + 4.5, shed.z1 - 2.0);
    kit::table(b, t, 0.85);
    for k in 0..4 {
        let p = Vec3::new(t.x0 + 0.3 + k as f32 * 0.7, 0.9, t.cz());
        b.mesh(Mat::Iron).beam(p - Vec3::Z * 0.7, p + Vec3::Z * 0.7, 0.015, 0.015, Vec3::Y);
    }
    for k in 0..6 {
        kit::jug(b, Vec3::new(t.x0 + 0.2 + k as f32 * 0.5, 0.85, t.z0 + 0.2), rng.range(0.12, 0.22), Mat::Water);
    }
    kit::shelf(b, Rect::new(shed.x1 - 0.95, shed.z0 + 1.0, shed.x1 - 0.6, shed.z1 - 2.0), 3, &mut rng, Mat::Water);
    // Бой стекла на переплавку и мешки с песком и золой.
    let c = Vec3::new(shed.x0 + 2.0, 0.0, shed.z0 + 2.0);
    b.mesh(Mat::Water).lathe(c, &[(0.8, 0.0), (0.0, 0.45)], 9);
    for k in 0..3 {
        b.solid([shed.x0 + 3.5 + k as f32 * 0.9, 0.0, shed.z0 + 1.0], [shed.x0 + 4.2 + k as f32 * 0.9, 0.7, shed.z0 + 1.6], Mat::Canvas);
    }
    // Стеклодув у печи, подмастерье у стола; ящик с бракованным стеклом и инструментом.
    extras(b, shed.inset(0.6), L::Workshop, &["Ящик стеклодува", "Корзина"], &[(Role::Craftsman, f + Vec3::new(2.4, 0.0, 0.0), f), (Role::Craftsman, t.center(0.0) + Vec3::Z * -1.0, t.center(0.0))], &mut rng);
}

// ---------------- гончарная мастерская ----------------

pub fn pottery_rect() -> Rect {
    block(16, 3)
}

/// Гончарный квартал у цитадели: две круглые горновые печи (топка снизу, обжигательная камера
/// сверху), гончарный круг под навесом, сушильные стеллажи, яма с глиной, готовые амфоры.
pub fn pottery(b: &mut Builder) {
    let r = pottery_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(1616);
    for (k, z) in [r.z0 + 8.0, r.z0 + 20.0].into_iter().enumerate() {
        let p = Vec3::new(r.x0 + 7.0, 0.0, z);
        kiln(b, p, k == 0);
    }
    // Навес с гончарным кругом.
    let sh = Rect::new(r.x1 - 9.0, r.z0 + 4.0, r.x1 - 1.0, r.z0 + 14.0);
    for (x, z) in [(sh.x0, sh.z0), (sh.x1, sh.z0), (sh.x0, sh.z1), (sh.x1, sh.z1)] {
        b.solid([x - 0.12, 0.0, z - 0.12], [x + 0.12, 3.0, z + 0.12], Mat::Wood);
    }
    kit::gable_roof(b, sh, 3.0, 1.2, false, 0.3, Mat::Wood);
    let w = sh.center(0.0);
    b.mesh(Mat::Wood).lathe(w, &[(0.45, 0.0), (0.45, 0.08), (0.06, 0.1), (0.06, 0.75), (0.25, 0.75), (0.25, 0.8)], 12);
    b.mesh(Mat::Earth).lathe(w + Vec3::Y * 0.8, &[(0.0, 0.0), (0.12, 0.0), (0.15, 0.12), (0.08, 0.25), (0.0, 0.25)], 10);
    kit::hidden(b, w - Vec3::new(0.45, 0.0, 0.45), w + Vec3::new(0.45, 0.8, 0.45));
    kit::bench(b, Rect::new(w.x + 0.6, w.z - 0.3, w.x + 1.0, w.z + 0.3));
    // Сушильные стеллажи с сырыми горшками.
    for k in 0..2 {
        let sr = Rect::new(sh.x0 + 0.5, sh.z1 + 2.0 + k as f32 * 2.0, sh.x1 - 0.5, sh.z1 + 2.4 + k as f32 * 2.0);
        kit::shelf(b, sr, 3, &mut rng, Mat::Earth);
    }
    // Яма с глиной (куча) и вода.
    let cl = Vec3::new(r.cx(), 0.0, r.z1 - 8.0);
    b.mesh(Mat::Earth).lathe(cl, &[(2.2, 0.0), (1.6, 0.5), (0.0, 0.7)], 12);
    kit::hidden(b, cl - Vec3::new(1.6, 0.0, 1.6), cl + Vec3::new(1.6, 0.5, 1.6));
    // Готовая продукция.
    for row in 0..2 {
        for k in 0..10 {
            kit::amphora(b, Vec3::new(r.x0 + 2.0 + k as f32 * 0.45, 0.0, r.z1 - 2.0 - row as f32 * 0.5), 1.0, 1);
        }
    }
    kit::woodpile(b, Rect::new(r.x0 + 12.0, r.z0 + 2.0, r.x0 + 15.0, r.z0 + 3.0), 1.2);
    kit::wall_x(b, r.x0, r.x1, r.z1 - 0.3, 0.6, 2.4, &[], Mat::Rubble);
    extras(b, sh.inset(0.3), L::Workshop, &["Ящик гончара"], &[(Role::Craftsman, w + Vec3::new(0.0, 0.0, 1.2), w)], &mut rng);
    extras(b, r.inset(1.0), L::Kitchen, &[], &[(Role::Craftsman, cl + Vec3::new(2.0, 0.0, 0.0), cl)], &mut rng);
}

/// Горновая печь: круглое тело, топочный канал с углями, обжигательная камера под куполом.
fn kiln(b: &mut Builder, p: Vec3, burning: bool) {
    let r = 1.8;
    let mut prof = vec![(r + 0.3, 0.0), (r, 0.3), (r, 1.8)];
    for k in 0..=6 {
        let a = k as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
        prof.push((r * a.cos().max(0.2), 1.8 + 1.4 * a.sin()));
    }
    b.mesh(Mat::Brick).lathe(p, &prof, 16);
    kit::hidden(b, p - Vec3::new(r * 0.85, 0.0, r * 0.85), p + Vec3::new(r * 0.85, 3.0, r * 0.85));
    // Топочный канал (префурний) с востока.
    b.solid([p.x + r - 0.2, 0.0, p.z - 0.6], [p.x + r + 1.6, 0.9, p.z - 0.45], Mat::Brick);
    b.solid([p.x + r - 0.2, 0.0, p.z + 0.45], [p.x + r + 1.6, 0.9, p.z + 0.6], Mat::Brick);
    if burning {
        b.mesh(Mat::Coals).cuboid(Vec3::new(p.x + r - 0.1, 0.0, p.z - 0.44), Vec3::new(p.x + r + 1.0, 0.25, p.z + 0.44));
        b.lamp(p + Vec3::new(r + 1.0, 0.6, 0.0), Color::srgb(1.0, 0.45, 0.15), 20_000.0, 7.0);
    }
}

// ---------------- кузница ----------------

pub fn smithy_rect() -> Rect {
    block(5, 2)
}

/// Кузница: горн с мехами, наковальня на колоде, корыто для закалки, древесный уголь.
pub fn smithy(b: &mut Builder) {
    let r = smithy_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(55);
    // Мастерская выходит на поперечную улицу к главным воротам (восточная сторона квартала).
    let w = Rect::new(r.x1 - 12.0, r.z0 + 2.0, r.x1, r.z0 + 14.0);
    kit::wall_x(b, w.x0, w.x1, w.z0 + 0.3, 0.6, 4.0, &[], Mat::Rubble);
    kit::wall_x(b, w.x0, w.x1, w.z1 - 0.3, 0.6, 4.0, &[Op::Door { s: w.x0 + 3.0, w: 1.2, h: 2.3 }], Mat::Rubble);
    kit::wall_z(b, w.z0 + 0.6, w.z1 - 0.6, w.x0 + 0.3, 0.6, 4.0, &[], Mat::Rubble);
    // Восточная сторона открыта: два столба держат крышу.
    for z in [w.z0 + 4.0, w.z1 - 4.0] {
        b.solid([w.x1 - 0.5, 0.0, z - 0.2], [w.x1 - 0.1, 3.9, z + 0.2], Mat::Wood);
    }
    b.mesh(Mat::Wood).cuboid(Vec3::new(w.x1 - 0.5, 3.9, w.z0), Vec3::new(w.x1 - 0.1, 4.2, w.z1));
    kit::lean_roof(b, w, Side::W, 5.2, 4.1, 0.6);
    let f = Vec3::new(w.x0 + 2.0, 0.0, w.cz());
    b.solid([f.x - 1.0, 0.0, f.z - 1.2], [f.x + 1.0, 1.0, f.z + 1.2], Mat::Brick);
    b.mesh(Mat::Coals).cuboid(Vec3::new(f.x - 0.6, 1.0, f.z - 0.6), Vec3::new(f.x + 0.6, 1.08, f.z + 0.6));
    // Вытяжной колпак.
    b.mesh(Mat::Brick).spire(f + Vec3::Y * 2.4, 1.2, 1.6, 4);
    b.mesh(Mat::Brick).prism(f + Vec3::Y * 3.6, 0.35, 1.6, 4);
    // Меха.
    let m = Vec3::new(f.x - 0.2, 0.9, f.z + 1.8);
    b.mesh(Mat::Canvas).lathe(m, &[(0.0, 0.0), (0.4, 0.1), (0.45, 0.3), (0.0, 0.45)], 8);
    b.lamp(f + Vec3::new(1.0, 1.6, 0.0), Color::srgb(1.0, 0.45, 0.15), 18_000.0, 7.0);
    // Наковальня на колоде, корыто.
    let a = Vec3::new(f.x + 3.0, 0.0, f.z);
    b.mesh(Mat::Wood).lathe(a, &[(0.32, 0.0), (0.3, 0.55), (0.0, 0.55)], 10);
    b.mesh(Mat::Iron).cuboid(a + Vec3::new(-0.3, 0.55, -0.12), a + Vec3::new(0.3, 0.8, 0.12));
    b.mesh(Mat::Iron).spire(a + Vec3::new(0.3, 0.7, 0.0), 0.1, 0.25, 4);
    kit::hidden(b, a - Vec3::new(0.35, 0.0, 0.35), a + Vec3::new(0.35, 0.8, 0.35));
    b.forges.push(crate::level::ForgeSpec { name: "Кузница Филиппа", kind: crate::smithing_net::StationKind::Forge, fire: f + Vec3::Y * 1.08, anvil: a });
    let q = Rect::new(f.x + 2.0, w.z0 + 1.0, f.x + 4.0, w.z0 + 1.7);
    b.solid([q.x0, 0.0, q.z0], [q.x1, 0.6, q.z1], Mat::Wood);
    b.decor([q.x0 + 0.08, 0.5, q.z0 + 0.08], [q.x1 - 0.08, 0.55, q.z1 - 0.08], Mat::Water);
    // Инструмент на стене и уголь.
    for k in 0..6 {
        let x = w.x0 + 4.0 + k as f32 * 0.35;
        b.mesh(Mat::Iron).beam(Vec3::new(x, 1.0, w.z0 + 0.65), Vec3::new(x, 1.9, w.z0 + 0.65), 0.02, 0.02, Vec3::Z);
    }
    let c = Vec3::new(w.x0 + 2.0, 0.0, w.z1 - 1.8);
    b.mesh(Mat::Coals).lathe(c, &[(1.0, 0.0), (0.0, 0.6)], 10);
    kit::hidden(b, c - Vec3::new(0.7, 0.0, 0.7), c + Vec3::new(0.7, 0.4, 0.7));
    // Остальная часть квартала — жилая усадьба кузнеца (простая) и двор с металлоломом.
    let _ = &mut rng;
    for k in 0..4 {
        b.mesh(Mat::Iron).beam(Vec3::new(w.x0 - 3.0 + k as f32 * 0.4, 0.05, w.z1 + 2.0), Vec3::new(w.x0 - 2.0 + k as f32 * 0.4, 0.05, w.z1 + 3.0), 0.04, 0.04, Vec3::Y);
    }
    extras(b, w.inset(0.5), L::Workshop, &["Ящик кузнеца"], &[(Role::Craftsman, a + Vec3::new(0.0, 0.0, 1.0), a), (Role::Child, f + Vec3::new(0.0, 0.0, -1.6), f)], &mut rng);
}

// ---------------- винодельня ----------------

pub fn winery_rect() -> Rect {
    block(9, 2)
}

/// Винодельня: две давильные площадки (тарапаны) с чанами для сусла, рычажный пресс,
/// подвал с рядами врытых пифосов-долиев.
pub fn winery(b: &mut Builder) {
    let r = winery_rect();
    b.at(r.cx(), r.cz());
    let hall = Rect::new(r.x0 + 1.0, r.z0 + 2.0, r.x1 - 1.0, r.z0 + 20.0);
    kit::wall_x(b, hall.x0, hall.x1, hall.z0 + 0.35, 0.7, 4.2, &[Op::Door { s: hall.cx(), w: 2.4, h: 2.8 }], Mat::Rubble);
    kit::wall_x(b, hall.x0, hall.x1, hall.z1 - 0.35, 0.7, 4.2, &[Op::Door { s: hall.cx(), w: 1.6, h: 2.5 }], Mat::Rubble);
    kit::wall_z(b, hall.z0 + 0.7, hall.z1 - 0.7, hall.x0 + 0.35, 0.7, 4.2, &[Op::Window { s: hall.cz(), w: 0.8, sill: 2.2, top: 3.0 }], Mat::Rubble);
    kit::wall_z(b, hall.z0 + 0.7, hall.z1 - 0.7, hall.x1 - 0.35, 0.7, 4.2, &[Op::Window { s: hall.cz(), w: 0.8, sill: 2.2, top: 3.0 }], Mat::Rubble);
    kit::gable_roof(b, hall, 4.2, 2.0, false, 0.5, Mat::Rubble);
    kit::wine_press(b, Rect::new(hall.x0 + 1.2, hall.z0 + 1.5, hall.x0 + 5.2, hall.z0 + 5.5));
    kit::wine_press(b, Rect::new(hall.x0 + 1.2, hall.z0 + 7.5, hall.x0 + 5.2, hall.z0 + 11.5));
    // Рычажный пресс: длинное бревно, один конец в стене, на другом — каменный груз.
    let p0 = Vec3::new(hall.x1 - 1.0, 1.4, hall.z0 + 4.0);
    let p1 = Vec3::new(hall.x1 - 9.0, 1.0, hall.z0 + 4.0);
    b.mesh(Mat::Wood).beam(p0, p1, 0.2, 0.2, Vec3::Y);
    for x in [hall.x1 - 3.0, hall.x1 - 3.6] {
        b.solid([x - 0.12, 0.0, p0.z - 0.5], [x + 0.12, 2.2, p0.z - 0.3], Mat::Wood);
        b.solid([x - 0.12, 0.0, p0.z + 0.3], [x + 0.12, 2.2, p0.z + 0.5], Mat::Wood);
    }
    let wt = Vec3::new(p1.x, 0.0, p1.z);
    b.mesh(Mat::Limestone).lathe(wt, &[(0.6, 0.0), (0.6, 0.7), (0.0, 0.7)], 12);
    kit::hidden(b, wt - Vec3::new(0.6, 0.0, 0.6), wt + Vec3::new(0.6, 0.7, 0.6));
    b.mesh(Mat::Wood).beam(wt + Vec3::Y * 0.7, p1, 0.03, 0.03, Vec3::X);
    // Ряды долиев, врытых по плечи.
    for row in 0..2 {
        for k in 0..5 {
            kit::pithos(b, Vec3::new(hall.x1 - 2.0 - k as f32 * 1.6, 0.0, hall.z1 - 2.0 - row as f32 * 1.8), 1.6, 1.0);
        }
    }
    b.lamp(hall.center(3.0), Color::srgb(1.0, 0.72, 0.45), 6_000.0, 9.0);
    // Во дворе — виноградник-шпалера и амфоры для отправки.
    for k in 0..4 {
        let z = hall.z1 + 3.0 + k as f32 * 2.5;
        if z > r.z1 - 2.0 {
            break;
        }
        kit::vine_row(b, Vec3::new(r.x0 + 2.0, 0.0, z), Vec3::new(r.x1 - 2.0, 0.0, z), 1.6);
    }
    let mut rng = Rng::new(2929);
    extras(b, hall.inset(0.8), L::Storehouse, &["Ларь винодела", "Корзина"], &[(Role::Craftsman, wt + Vec3::new(1.5, 0.0, 1.2), wt), (Role::Farmer, hall.center(0.0) + Vec3::Z * 3.0, hall.center(0.0))], &mut rng);
}
