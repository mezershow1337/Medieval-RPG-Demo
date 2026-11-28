//! Общественные здания: агора, епископская базилика с баптистерием, ксенон (больница),
//! дидаскалион (школа), дом знатных ссыльных, термы, заброшенный театр, резервуар водопровода,
//! малая западная церковь и цитадель гарнизона.

use super::kit::{self, Op, Rect, Rng, Side};
use super::*;
use super::interior::{self, extras};
use crate::items::LootTableId as L;
use crate::level::Role;
use crate::collision::Aabb;
use std::f32::consts::{FRAC_PI_2, PI};

// ---------------- общие детали ----------------

/// Колонна: база, ствол с энтазисом, капитель (импостная — для мрамора, дорическая — для известняка).
pub fn column(b: &mut Builder, p: Vec3, h: f32, r: f32, mat: Mat) {
    let m = b.mesh(mat);
    m.lathe(p, &[(r * 1.45, 0.0), (r * 1.45, 0.18), (r * 1.25, 0.24), (r * 1.15, 0.32)], 14);
    let shaft_top = h - r * 1.6;
    m.lathe(p + Vec3::Y * 0.32, &[(r, 0.0), (r * 1.03, (shaft_top - 0.32) * 0.35), (r * 0.86, shaft_top - 0.32)], 14);
    let top = p + Vec3::Y * shaft_top;
    m.lathe(top, &[(r * 0.86, 0.0), (r * 0.95, 0.08), (r * 1.3, r * 0.9)], 14);
    m.cuboid(top + Vec3::new(-r * 1.4, r * 0.9, -r * 1.4), top + Vec3::new(r * 1.4, r * 1.6, r * 1.4));
    b.invisible.push(Aabb::new(p - Vec3::new(r * 1.1, 0.0, r * 1.1), p + Vec3::new(r * 1.1, h, r * 1.1)));
}

/// Поликандило — бронзовый круг с подвесными стеклянными лампадами.
pub fn polycandelon(b: &mut Builder, c: Vec3, top: f32) {
    let r = 0.7;
    let ring: Vec<Vec3> = (0..=12).map(|i| {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        c + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
    }).collect();
    let m = b.mesh(Mat::Bronze);
    m.beam_path(&ring, 0.025, 0.025, Vec3::Y);
    for k in 0..3 {
        let a = k as f32 * 2.094;
        m.beam(c + Vec3::new(a.cos() * r, 0.0, a.sin() * r), c + Vec3::Y * 1.0, 0.01, 0.01, Vec3::X);
    }
    m.beam(c + Vec3::Y, Vec3::new(c.x, top, c.z), 0.015, 0.015, Vec3::X);
    for k in 0..6 {
        let a = (k as f32 + 0.5) / 6.0 * std::f32::consts::TAU;
        let p = c + Vec3::new(a.cos() * r, -0.12, a.sin() * r);
        b.mesh(Mat::Coals).lathe(p, &[(0.0, 0.0), (0.05, 0.04), (0.06, 0.12), (0.0, 0.12)], 6);
    }
    b.lamp(c - Vec3::Y * 0.1, Color::srgb(1.0, 0.72, 0.42), 60_000.0, 16.0);
}

/// Комнаты по сторонам перистильного двора.
pub struct Peri {
    pub court: Rect,
    pub rooms: Vec<(Side, Rect)>,
}

/// Перистильное здание: наружные стены с входами, корпуса комнат вокруг двора,
/// колоннада-портик во дворе, черепичные скаты внутрь.
/// `entrances` — (сторона, координата вдоль стены). `n` — число комнат на сторонах N, S, W, E.
pub fn peristyle(b: &mut Builder, outer: Rect, depth: f32, h: f32, mat: Mat, entrances: &[(Side, f32)], n: [usize; 4], portico: bool) -> Peri {
    let court = outer.inset(depth);
    let ot = 0.7;
    let it = 0.5;
    // Наружные стены.
    let door = |side: Side| -> Vec<Op> { entrances.iter().filter(|(s, _)| *s == side).map(|&(_, x)| Op::Door { s: x, w: 2.0, h: 3.0 }).collect() };
    kit::wall_x(b, outer.x0, outer.x1, outer.z0 + ot / 2.0, ot, h, &door(Side::N), mat);
    kit::wall_x(b, outer.x0, outer.x1, outer.z1 - ot / 2.0, ot, h, &door(Side::S), mat);
    kit::wall_z(b, outer.z0 + ot, outer.z1 - ot, outer.x0 + ot / 2.0, ot, h, &door(Side::W), mat);
    kit::wall_z(b, outer.z0 + ot, outer.z1 - ot, outer.x1 - ot / 2.0, ot, h, &door(Side::E), mat);
    for (x, z) in [(outer.x0, outer.z0), (outer.x1, outer.z0), (outer.x0, outer.z1), (outer.x1, outer.z1)] {
        let sx = if x > outer.cx() { -1.0 } else { 1.0 };
        let sz = if z > outer.cz() { -1.0 } else { 1.0 };
        kit::quoins(b, x + sx * ot / 2.0, z + sz * ot / 2.0, h, ot);
    }
    let mut rooms = Vec::new();
    // Северный и южный корпуса — на всю ширину; западный и восточный — между ними.
    let ranges = [
        (Side::N, Rect::new(outer.x0 + ot, outer.z0 + ot, outer.x1 - ot, court.z0), n[0]),
        (Side::S, Rect::new(outer.x0 + ot, court.z1, outer.x1 - ot, outer.z1 - ot), n[1]),
        (Side::W, Rect::new(outer.x0 + ot, court.z0, court.x0, court.z1), n[2]),
        (Side::E, Rect::new(court.x1, court.z0, outer.x1 - ot, court.z1), n[3]),
    ];
    for (side, r, count) in ranges {
        if count == 0 {
            continue;
        }
        let along_x = side.along_x();
        let (s0, s1) = if along_x { (r.x0, r.x1) } else { (r.z0, r.z1) };
        // Комнаты N/S корпусов не заходят в углы над W/E корпусами — там глухие кладовые.
        let (c0, c1) = if along_x { (court.x0, court.x1) } else { (s0, s1) };
        let mut bounds = vec![s0];
        let step = (c1 - c0) / count as f32;
        for k in 0..=count {
            let s = c0 + step * k as f32;
            if s > s0 + 0.5 && s < s1 - 0.5 {
                bounds.push(s);
            }
        }
        bounds.push(s1);
        bounds.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        let mut ops = Vec::new();
        for w in bounds.windows(2) {
            let (a, c) = (w[0], w[1]);
            let room = if along_x { Rect::new(a, r.z0, c, r.z1) } else { Rect::new(r.x0, a, r.x1, c) };
            let mid = (a + c) / 2.0;
            let reaches_court = if along_x { mid > court.x0 && mid < court.x1 } else { true };
            if reaches_court {
                ops.push(Op::Door { s: mid, w: 1.3, h: 2.6 });
            }
            rooms.push((side, room.inset(it / 2.0)));
        }
        // Перегородки.
        for &s in &bounds[1..bounds.len() - 1] {
            if along_x {
                kit::wall_z(b, r.z0, r.z1, s, it, h, &[], mat);
            } else {
                kit::wall_x(b, r.x0, r.x1, s, it, h, &[], mat);
            }
        }
        // Стена во двор.
        match side {
            Side::N => kit::wall_x(b, court.x0 - it / 2.0, court.x1 + it / 2.0, court.z0 - it / 2.0, it, h, &ops, mat),
            Side::S => kit::wall_x(b, court.x0 - it / 2.0, court.x1 + it / 2.0, court.z1 + it / 2.0, it, h, &ops, mat),
            Side::W => kit::wall_z(b, court.z0, court.z1, court.x0 - it / 2.0, it, h, &ops, mat),
            Side::E => kit::wall_z(b, court.z0, court.z1, court.x1 + it / 2.0, it, h, &ops, mat),
        }
        // Крыша корпуса — скат во двор.
        let roof_r = match side {
            Side::N => Rect::new(outer.x0, outer.z0, outer.x1, court.z0),
            Side::S => Rect::new(outer.x0, court.z1, outer.x1, outer.z1),
            Side::W => Rect::new(outer.x0, court.z0, court.x0, court.z1),
            Side::E => Rect::new(court.x1, court.z0, outer.x1, court.z1),
        };
        kit::lean_roof(b, roof_r, side, h + 1.2, h + 0.05, 0.3);
        // Наружная стена и торцы — до черепицы. Торцы северного и южного корпусов стоят на
        // наружных стенах, западного и восточного — над низким краем соседнего ската.
        let ends = if along_x {
            [(outer.x0, outer.x0 + ot, h), (outer.x1 - ot, outer.x1, h)]
        } else {
            [(court.z0 - 0.25, court.z0 + 0.2, h), (court.z1 - 0.2, court.z1 + 0.25, h)]
        };
        kit::lean_fill(b, roof_r, side, h + 1.2, h + 0.05, ot, h, &ends, mat);
    }
    if portico {
        let inset = 2.6;
        let cr = court.inset(inset);
        let ch = 3.4;
        let mut cols = Vec::new();
        let nx = (cr.w() / 3.0).round().max(1.0) as usize;
        let nz = (cr.d() / 3.0).round().max(1.0) as usize;
        for k in 0..=nx {
            let x = cr.x0 + cr.w() * k as f32 / nx as f32;
            cols.push(Vec3::new(x, 0.0, cr.z0));
            cols.push(Vec3::new(x, 0.0, cr.z1));
        }
        for k in 1..nz {
            let z = cr.z0 + cr.d() * k as f32 / nz as f32;
            cols.push(Vec3::new(cr.x0, 0.0, z));
            cols.push(Vec3::new(cr.x1, 0.0, z));
        }
        for p in cols {
            column(b, p, ch, 0.2, Mat::Limestone);
        }
        // Архитрав и скаты портика.
        let t = b.mesh(Mat::Wood);
        for (a, c) in [((cr.x0, cr.z0), (cr.x1, cr.z0)), ((cr.x0, cr.z1), (cr.x1, cr.z1)), ((cr.x0, cr.z0), (cr.x0, cr.z1)), ((cr.x1, cr.z0), (cr.x1, cr.z1))] {
            t.beam(Vec3::new(a.0, ch + 0.15, a.1), Vec3::new(c.0, ch + 0.15, c.1), 0.18, 0.15, Vec3::Y);
        }
        for (side, r) in [
            (Side::N, Rect::new(court.x0, court.z0, court.x1, cr.z0)),
            (Side::S, Rect::new(court.x0, cr.z1, court.x1, court.z1)),
            (Side::W, Rect::new(court.x0, cr.z0, cr.x0, cr.z1)),
            (Side::E, Rect::new(cr.x1, cr.z0, court.x1, cr.z1)),
        ] {
            kit::lean_roof(b, r, side, h - 0.2, ch + 0.3, 0.4);
        }
    }
    Peri { court, rooms }
}

/// Ряд коек вдоль длинной стороны комнаты (палата ксенона, казарма).
pub fn beds_along(b: &mut Builder, r: Rect, rng: &mut Rng) {
    beds_along_edge(b, r, false, rng);
}

/// Койки у дальней стены: `at_max` — у стороны с большей координатой (z1 или x1).
pub fn beds_along_edge(b: &mut Builder, r: Rect, at_max: bool, rng: &mut Rng) {
    let along_x = r.w() > r.d();
    let r = if !at_max { r } else if along_x { Rect::new(r.x0, r.z1 - 2.2, r.x1, r.z1) } else { Rect::new(r.x1 - 2.2, r.z0, r.x1, r.z1) };
    let len = if along_x { r.w() } else { r.d() };
    let n = ((len - 0.4) / 1.3).floor() as usize;
    for k in 0..n {
        let s = 0.4 + k as f32 * 1.3;
        let bed = if along_x { Rect::new(r.x0 + s, r.z0 + 0.15, r.x0 + s + 0.85, r.z0 + 2.05) } else { Rect::new(r.x0 + 0.15, r.z0 + s, r.x0 + 2.05, r.z0 + s + 0.85) };
        kit::bed(b, bed);
        if rng.chance(0.4) {
            let p = if along_x { Vec3::new(bed.cx(), 0.0, bed.z1 + 0.25) } else { Vec3::new(bed.x1 + 0.25, 0.0, bed.cz()) };
            kit::jug(b, p, 0.25, Mat::Ceramic);
        }
    }
}

// ---------------- агора ----------------

pub fn agora_rect() -> Rect {
    block(16, -1).union(&block(17, -1))
}

pub fn agora(b: &mut Builder) {
    let a = agora_rect();
    b.at(a.cx(), a.cz());
    let mut rng = Rng::new(404);
    b.decor([a.x0, 0.0, a.z0], [a.x1, 0.035, a.z1], Mat::Paving);
    // Северная стоя: задняя стена, колоннада, черепичный скат.
    let back_z = a.z0 + 0.35;
    let col_z = a.z0 + 7.0;
    kit::wall_x(b, a.x0, a.x1, back_z, 0.7, 6.0, &[Op::Door { s: a.cx(), w: 2.4, h: 3.2 }], Mat::Limestone);
    let n = 16;
    for k in 0..=n {
        let x = a.x0 + 1.0 + (a.w() - 2.0) * k as f32 / n as f32;
        column(b, Vec3::new(x, 0.0, col_z), 5.0, 0.28, Mat::Limestone);
    }
    b.mesh(Mat::Limestone).cuboid(Vec3::new(a.x0 + 0.5, 5.0, col_z - 0.45), Vec3::new(a.x1 - 0.5, 5.6, col_z + 0.45));
    kit::lean_roof(b, Rect::new(a.x0, a.z0, a.x1, col_z + 0.4), Side::N, 7.0, 5.6, 0.6);
    kit::lean_fill(b, Rect::new(a.x0, a.z0, a.x1, col_z + 0.4), Side::N, 7.0, 5.6, 0.7, 6.0, &[], Mat::Limestone);
    // Западная стоя.
    let back_x = a.x0 + 0.35;
    let col_x = a.x0 + 6.5;
    kit::wall_z(b, col_z + 0.5, a.z1, back_x, 0.7, 6.0, &[], Mat::Limestone);
    for k in 0..=9 {
        let z = col_z + 3.0 + (a.z1 - col_z - 4.0) * k as f32 / 9.0;
        column(b, Vec3::new(col_x, 0.0, z), 5.0, 0.28, Mat::Limestone);
    }
    b.mesh(Mat::Limestone).cuboid(Vec3::new(col_x - 0.45, 5.0, col_z + 0.5), Vec3::new(col_x + 0.45, 5.6, a.z1 - 0.5));
    kit::lean_roof(b, Rect::new(a.x0, col_z + 0.5, col_x + 0.4, a.z1), Side::W, 7.0, 5.6, 0.6);
    // Торец западной стои над скатом северной — иначе в углу видно под крышу.
    kit::lean_fill(b, Rect::new(a.x0, col_z + 0.5, col_x + 0.4, a.z1), Side::W, 7.0, 5.6, 0.7, 6.0, &[(col_z + 0.25, col_z + 0.7, 5.6)], Mat::Limestone);
    // Скамьи в стое, светильники.
    for k in 0..5 {
        let x = a.x0 + 8.0 + k as f32 * 8.0;
        kit::bench(b, Rect::new(x - 1.0, back_z + 0.5, x + 1.0, back_z + 0.9));
    }
    for k in 0..4 {
        let x = a.x0 + 6.0 + k as f32 * 12.0;
        b.torch(x, 3.4, col_z - 0.4, Vec3::NEG_Z);
    }
    // Фонтан-водоразборник (вода из водопровода): каменная чаша и столб с бронзовым носиком.
    let f = Vec3::new(a.x1 - 12.0, 0.0, a.cz() - 2.0);
    b.solid([f.x - 2.0, 0.0, f.z - 1.2], [f.x + 2.0, 0.8, f.z - 0.9], Mat::Limestone);
    b.solid([f.x - 2.0, 0.0, f.z + 0.9], [f.x + 2.0, 0.8, f.z + 1.2], Mat::Limestone);
    b.solid([f.x - 2.0, 0.0, f.z - 0.9], [f.x - 1.7, 0.8, f.z + 0.9], Mat::Limestone);
    b.solid([f.x + 1.7, 0.0, f.z - 0.9], [f.x + 2.0, 0.8, f.z + 0.9], Mat::Limestone);
    b.decor([f.x - 1.7, 0.0, f.z - 0.9], [f.x + 1.7, 0.62, f.z + 0.9], Mat::Water);
    b.solid([f.x + 2.0, 0.0, f.z - 0.4], [f.x + 2.8, 2.2, f.z + 0.4], Mat::Marble);
    b.mesh(Mat::Bronze).beam(Vec3::new(f.x + 2.0, 1.6, f.z), Vec3::new(f.x + 1.5, 1.45, f.z), 0.04, 0.04, Vec3::Y);
    // Колонна с крестом.
    let c = Vec3::new(a.cx() + 2.0, 0.0, a.cz() - 6.0);
    b.solid([c.x - 1.0, 0.0, c.z - 1.0], [c.x + 1.0, 1.0, c.z + 1.0], Mat::Limestone);
    column(b, c + Vec3::Y, 6.0, 0.3, Mat::Marble);
    let top = c + Vec3::Y * 7.0;
    b.mesh(Mat::Bronze).cuboid(top + Vec3::new(-0.06, 0.0, -0.06), top + Vec3::new(0.06, 1.3, 0.06));
    b.mesh(Mat::Bronze).cuboid(top + Vec3::new(-0.45, 0.75, -0.06), top + Vec3::new(0.45, 0.87, 0.06));
    // Торговые ряды: столы под навесами с товаром. Первые лотки занимают торговцы
    // (стол, навес и товар у них свои), остальные стоят с товаром сами по себе.
    let staffed = super::super::places::staffed_stalls("agora");
    for (k, (p, facing)) in agora_stalls().into_iter().enumerate().skip(staffed) {
        stall(b, p, facing, k, &mut rng);
    }
}

/// Лотки агоры: середина стола и куда он обращён (к покупателям). Первый ряд — вдоль северной
/// стои лицом на площадь, второй — вдоль западной стои лицом на восток, последний — у главной улицы.
pub fn agora_stalls() -> [(Vec3, Vec3); 6] {
    let a = agora_rect();
    let at = |x: f32, z: f32| Vec3::new(a.x0 + x, 0.0, a.z0 + z);
    [
        (at(12.0, 13.0), Vec3::Z),
        (at(20.0, 13.0), Vec3::Z),
        (at(32.0, 13.0), Vec3::Z),
        (at(12.0, 21.0), Vec3::X),
        (at(12.0, 29.0), Vec3::X),
        (at(40.0, 31.0), Vec3::Z),
    ]
}

/// Торговый лоток: стол, полотняный навес на четырёх жердях, товар. `facing` — куда обращён
/// стол (к покупателям, одно из ±X/±Z); продавец стоит с другой стороны, под высоким краем навеса.
pub fn stall(b: &mut Builder, p: Vec3, facing: Vec3, kind: usize, rng: &mut Rng) {
    let right = Vec3::new(facing.z, 0.0, -facing.x);
    // Точка в осях лотка: `u` — вдоль стола, `v` — к покупателю.
    let w = |u: f32, y: f32, v: f32| p + right * u + facing * v + Vec3::Y * y;
    let (c0, c1) = (w(-1.2, 0.0, -0.5), w(1.2, 0.0, 0.5));
    let t = Rect::new(c0.x.min(c1.x), c0.z.min(c1.z), c0.x.max(c1.x), c0.z.max(c1.z));
    kit::table(b, t, 0.85);
    let m = b.mesh(Mat::Wood);
    for (u, v) in [(-1.4, -1.1), (1.4, -1.1), (-1.4, 0.8), (1.4, 0.8)] {
        m.beam(w(u, 0.0, v), w(u, 2.4, v), 0.035, 0.035, right);
    }
    let q = [w(-1.55, 2.5, -1.25), w(1.55, 2.5, -1.25), w(1.55, 2.25, 0.95), w(-1.55, 2.25, 0.95)];
    b.roof(Mat::Canvas, |c| {
        c.quad(q, Vec3::Y);
        c.quad(q, Vec3::NEG_Y);
    });
    let top = p + Vec3::Y * 0.85;
    // Товар раскладывается вдоль стола.
    let along = |u: f32, y: f32, v: f32| right * u + facing * v + Vec3::Y * y;
    match kind % 4 {
        0 => {
            for k in 0..5 {
                kit::jug(b, top + along(-0.9 + k as f32 * 0.45, 0.0, 0.0), rng.range(0.15, 0.3), Mat::Ceramic);
            }
        }
        1 => {
            // Рыба (солёная и вяленая).
            for k in 0..8 {
                let q = top + along(-0.9 + k as f32 * 0.25, 0.04, rng.range(-0.2, 0.2));
                b.mesh(Mat::Iron).beam(q - facing * 0.15, q + facing * 0.15, 0.04, 0.025, Vec3::Y);
            }
        }
        2 => {
            // Ткани в рулонах.
            for k in 0..4 {
                let q = top + along(-0.8 + k as f32 * 0.5, 0.1, 0.0);
                b.mesh(if k % 2 == 0 { Mat::Canvas } else { Mat::Banner }).beam(q - facing * 0.4, q + facing * 0.4, 0.1, 0.1, Vec3::Y);
            }
        }
        _ => {
            // Корзины с зерном и бобами.
            for k in 0..3 {
                let q = top + along(-0.7 + k as f32 * 0.7, 0.0, 0.0);
                b.mesh(Mat::Wicker).lathe(q, &[(0.15, 0.0), (0.25, 0.22), (0.0, 0.22)], 10);
                b.mesh(Mat::Earth).disc(q + Vec3::Y * 0.2, 0.23, 10, true);
            }
        }
    }
}

// ---------------- епископская базилика ----------------

/// Базилика: атрий на западе, нартекс, трёхнефный наос, апсида на востоке (+X).
pub struct BasilicaPlan {
    pub atrium: Rect,
    pub naos: Rect,
    pub nave_z: (f32, f32),
    pub apse_x: f32,
    pub baptistery: Rect,
}

pub fn basilica_plan() -> BasilicaPlan {
    let r = block(18, -1).union(&block(19, -1));
    let zc = r.cz();
    BasilicaPlan {
        atrium: Rect::new(r.x0 + 1.0, zc - 10.0, r.x0 + 17.0, zc + 10.0),
        naos: Rect::new(r.x0 + 21.0, zc - 9.0, r.x0 + 43.0, zc + 9.0),
        nave_z: (zc - 4.5, zc + 4.5),
        apse_x: r.x0 + 43.0,
        baptistery: Rect::new(r.x0 + 28.0, r.z0 + 3.0, r.x0 + 36.0, r.z0 + 11.0),
    }
}

pub fn basilica(b: &mut Builder) {
    let p = basilica_plan();
    let n = p.naos;
    let (nz0, nz1) = p.nave_z;
    b.at(n.cx(), n.cz());
    let wall_mat = Mat::OpusMixtum;
    let aisle_h = 6.0;
    let nave_h = 11.0;
    let t = 0.8;
    // Полы: мозаика в наосе и нартексе, мрамор на алтарном возвышении.
    b.decor([p.atrium.x1 + 0.0, 0.0, n.z0], [n.x1, 0.03, n.z1], Mat::Mosaic);
    b.decor([p.atrium.x0, 0.0, p.atrium.z0], [p.atrium.x1, 0.03, p.atrium.z1], Mat::Paving);
    // Боковые стены нефов с окнами и дверями (в баптистерий — с севера).
    let bx = p.baptistery.cx();
    let windows: Vec<Op> = (0..5).map(|k| Op::Window { s: n.x0 + 3.0 + k as f32 * 4.2, w: 1.0, sill: 2.6, top: 4.4 }).collect();
    let mut north = windows.clone();
    north.retain(|o| !matches!(o, Op::Window { s, .. } if (*s - bx).abs() < 2.0));
    north.push(Op::Door { s: bx, w: 1.4, h: 2.6 });
    kit::wall_x(b, n.x0, n.x1, n.z0 + t / 2.0, t, aisle_h, &north, wall_mat);
    let mut south = windows.clone();
    south.push(Op::Door { s: n.x0 + 4.0 + 6.3, w: 1.4, h: 2.6 });
    south.retain(|o| !matches!(o, Op::Window { s, .. } if (*s - (n.x0 + 10.3)).abs() < 1.5));
    kit::wall_x(b, n.x0, n.x1, n.z1 - t / 2.0, t, aisle_h, &south, wall_mat);
    // Западная стена наоса (из нартекса): три двери.
    let nar = Rect::new(p.atrium.x1, n.z0, n.x0, n.z1);
    let west_ops = [Op::Arch { s: n.cz(), w: 2.4, spring: 2.8 }, Op::Door { s: (n.z0 + nz0) / 2.0, w: 1.4, h: 2.6 }, Op::Door { s: (n.z1 + nz1) / 2.0, w: 1.4, h: 2.6 }];
    kit::wall(b, kit::fz(), n.z0, n.z1, n.x0 - t / 2.0, n.x0 + t / 2.0, 0.0, aisle_h, &west_ops, wall_mat, Mat::Limestone);
    kit::wall(b, kit::fz(), nz0, nz1, n.x0 - t / 2.0, n.x0 + t / 2.0, aisle_h, nave_h, &[Op::Window { s: n.cz(), w: 1.6, sill: 1.2, top: 3.6 }], wall_mat, Mat::Limestone);
    // Нартекс: наружная стена в атрий с тремя арками.
    let nar_ops = [Op::Arch { s: n.cz(), w: 2.6, spring: 2.6 }, Op::Arch { s: n.cz() - 6.0, w: 1.8, spring: 2.2 }, Op::Arch { s: n.cz() + 6.0, w: 1.8, spring: 2.2 }];
    kit::wall(b, kit::fz(), n.z0, n.z1, nar.x0 + 0.35, nar.x0 + 1.05, 0.0, 5.0, &nar_ops, wall_mat, Mat::Limestone);
    kit::lean_roof(b, Rect::new(nar.x0 + 0.35, n.z0, n.x0, n.z1), Side::E, 6.6, 5.0, 0.4);
    // Восточные стены боковых нефов.
    kit::wall_z(b, n.z0, nz0, n.x1 - t / 2.0, t, aisle_h, &[], wall_mat);
    kit::wall_z(b, nz1, n.z1, n.x1 - t / 2.0, t, aisle_h, &[], wall_mat);
    // Колоннады нефа и стены над ними (аркады с окнами второго света).
    let ncol = 6;
    let step = (n.w() - 2.0) / ncol as f32;
    for z in [nz0, nz1] {
        let mut ops = Vec::new();
        for k in 0..ncol {
            let x = n.x0 + 1.0 + step * (k as f32 + 0.5);
            ops.push(Op::Arch { s: x, w: step - 0.7, spring: 0.0 });
            ops.push(Op::Window { s: x, w: 1.0, sill: 2.6, top: 4.4 });
        }
        // Нижний ярус — колонны; стену начинаем с пят арок.
        let arches: Vec<Op> = ops.iter().copied().filter(|o| matches!(o, Op::Arch { .. })).collect();
        kit::wall(b, kit::fx(), n.x0, n.x1, z - 0.35, z + 0.35, 5.0, 7.6, &arches, wall_mat, Mat::Marble);
        let wins: Vec<Op> = ops.iter().copied().filter(|o| matches!(o, Op::Window { .. })).map(|o| match o {
            Op::Window { s, w, .. } => Op::Window { s, w, sill: 0.5, top: 2.4 },
            o => o,
        }).collect();
        kit::wall(b, kit::fx(), n.x0, n.x1, z - 0.35, z + 0.35, 7.6, nave_h, &wins, wall_mat, Mat::Limestone);
        for k in 0..=ncol {
            let x = n.x0 + 1.0 + step * k as f32;
            column(b, Vec3::new(x, 0.0, z), 5.0, 0.3, Mat::Marble);
        }
    }
    // Апсида: полукруглая стена и конха.
    let ac = Vec3::new(p.apse_x, 0.0, n.cz());
    let ar = (nz1 - nz0) / 2.0;
    b.mesh(wall_mat).lathe_arc(ac, &[(ar + t, 0.0), (ar + t, 8.0)], -FRAC_PI_2, FRAC_PI_2, 14, false);
    b.mesh(Mat::Plaster).lathe_arc(ac, &[(ar, 0.0), (ar, 8.0)], -FRAC_PI_2, FRAC_PI_2, 14, true);
    b.mesh(wall_mat).lathe_arc(ac + Vec3::Y * 8.0, &[(ar + t, 0.0), (ar, 0.0)], -FRAC_PI_2, FRAC_PI_2, 14, false);
    b.mesh(Mat::Plaster).dome(ac + Vec3::Y * 8.0, ar, -FRAC_PI_2, FRAC_PI_2, 12, 0.0);
    b.roof(Mat::Tile, |m| m.dome(ac + Vec3::Y * 8.0, ar + t + 0.1, -FRAC_PI_2, FRAC_PI_2, 12, 0.05));
    for k in 0..8 {
        let a = -FRAC_PI_2 + PI * (k as f32 + 0.5) / 8.0;
        let q = ac + Vec3::new(a.cos() * (ar + t / 2.0), 0.0, a.sin() * (ar + t / 2.0));
        b.invisible.push(Aabb::new(q - Vec3::new(0.7, 0.0, 0.7), q + Vec3::new(0.7, 8.0, 0.7)));
    }
    // Стена над триумфальной аркой.
    kit::wall(b, kit::fz(), nz0 - 0.4, nz1 + 0.4, p.apse_x - 0.4, p.apse_x + 0.4, 0.0, nave_h, &[Op::Arch { s: n.cz(), w: 2.0 * ar - 0.2, spring: 5.2 }], wall_mat, Mat::Marble);
    // Синтрон (ступенчатая скамья клира) и кафедра епископа.
    for k in 0..3 {
        let r0 = ar - 0.9 - k as f32 * 0.7;
        b.mesh(Mat::Marble).lathe_arc(ac, &[(r0, 0.0), (r0, 0.45 * (k + 1) as f32), (r0 + 0.7, 0.45 * (k + 1) as f32)], -FRAC_PI_2 + 0.2, FRAC_PI_2 - 0.2, 12, true);
    }
    b.solid([ac.x + ar - 1.2, 0.0, ac.z - 0.5], [ac.x + ar - 0.2, 1.5, ac.z + 0.5], Mat::Marble);
    // Алтарь под киворием.
    let alt = Vec3::new(p.apse_x - 2.5, 0.0, n.cz());
    b.solid([alt.x - 4.0, 0.0, nz0 + 0.4], [p.apse_x + ar - 1.3, 0.35, nz1 - 0.4], Mat::Marble);
    b.solid([alt.x - 0.6, 0.0, alt.z - 0.9], [alt.x + 0.6, 1.35, alt.z + 0.9], Mat::Marble);
    for (dx, dz) in [(-1.2, -1.3), (1.2, -1.3), (-1.2, 1.3), (1.2, 1.3)] {
        column(b, alt + Vec3::new(dx, 0.35, dz), 3.0, 0.12, Mat::Marble);
    }
    b.mesh(Mat::Marble).cuboid(alt + Vec3::new(-1.5, 3.35, -1.6), alt + Vec3::new(1.5, 3.7, 1.6));
    b.roof(Mat::Tile, |m| m.spire(alt + Vec3::Y * 3.7, 2.0, 1.4, 4));
    // Алтарная преграда (темплон): низкие мраморные плиты на столбиках, проход посередине.
    let tx = alt.x - 4.0;
    for (z0, z1) in [(nz0 + 0.4, n.cz() - 1.0), (n.cz() + 1.0, nz1 - 0.4)] {
        b.solid([tx - 0.08, 0.0, z0], [tx + 0.08, 1.0, z1], Mat::Marble);
        let k = ((z1 - z0) / 1.2) as usize;
        for i in 0..=k {
            let z = z0 + (z1 - z0) * i as f32 / k.max(1) as f32;
            b.mesh(Mat::Marble).cuboid(Vec3::new(tx - 0.13, 0.0, z - 0.1), Vec3::new(tx + 0.13, 1.25, z + 0.1));
        }
    }
    // Амвон посреди нефа: площадка на столбиках с лестницей.
    let am = Vec3::new(n.x0 + 9.0, 0.0, n.cz());
    b.solid([am.x - 1.0, 0.0, am.z - 0.8], [am.x + 1.0, 1.6, am.z + 0.8], Mat::Marble);
    kit::stairs(b, Vec3::new(am.x - 1.0 - 2.4, 0.0, am.z), Vec3::X, 0.9, 1.6, Mat::Marble);
    b.mesh(Mat::Marble).cuboid(Vec3::new(am.x - 1.0, 1.6, am.z - 0.8), Vec3::new(am.x + 1.0, 2.6, am.z - 0.7));
    b.mesh(Mat::Marble).cuboid(Vec3::new(am.x - 1.0, 1.6, am.z + 0.7), Vec3::new(am.x + 1.0, 2.6, am.z + 0.8));
    // Крыши: неф — двускатная, боковые нефы — односкатные.
    kit::gable_roof(b, Rect::new(n.x0 - 0.4, nz0 - 0.35, p.apse_x + 0.4, nz1 + 0.35), nave_h, 2.6, true, 0.6, wall_mat);
    kit::lean_roof(b, Rect::new(n.x0, n.z0, n.x1, nz0 - 0.35), Side::S, 7.3, aisle_h, 0.6);
    kit::lean_roof(b, Rect::new(n.x0, nz1 + 0.35, n.x1, n.z1), Side::N, 7.3, aisle_h, 0.6);
    // Потолок нефа — деревянный, кессонами (балки).
    let m = b.mesh(Mat::Wood);
    m.cuboid(Vec3::new(n.x0, nave_h - 0.1, nz0 + 0.35), Vec3::new(p.apse_x, nave_h, nz1 - 0.35));
    let beams = 11;
    for k in 0..=beams {
        let x = n.x0 + (p.apse_x - n.x0) * k as f32 / beams as f32;
        m.cuboid(Vec3::new(x - 0.12, nave_h - 0.4, nz0 + 0.35), Vec3::new(x + 0.12, nave_h - 0.1, nz1 - 0.35));
    }
    // Свет: поликандила над нефом и лампада в апсиде.
    for k in 0..3 {
        polycandelon(b, Vec3::new(n.x0 + 5.0 + k as f32 * 6.5, 4.0, n.cz()), nave_h - 0.4);
    }
    b.lamp(alt + Vec3::Y * 2.5, Color::srgb(1.0, 0.75, 0.45), 25_000.0, 10.0);
    {
        let mut rng = Rng::new(1919);
        // У алтаря — священник, в боковых нефах — ризничьи лари, у входа — монах.
        b.resident(alt - Vec3::X * 1.6, -std::f32::consts::FRAC_PI_2, Role::Priest);
        b.loot(L::Church);
        for z in [n.z0 + 1.0, n.z1 - 1.0] {
            let f = Rect::new(p.apse_x - 6.0, z - 0.3, p.apse_x - 5.0, z + 0.3);
            interior::chest(b, f, "Ризничий ларь");
            interior::lampstand(b, Vec3::new(p.apse_x - 7.0, 0.0, z), false);
        }
        b.resident(Vec3::new(n.x0 + 2.5, 0.0, n.z0 + 1.2), std::f32::consts::PI, Role::Monk);
        let _ = &mut rng;
    }
    // Атрий: портики с трёх сторон и фиал (фонтан для омовения).
    let at = p.atrium;
    kit::wall_x(b, at.x0, nar.x0 + 0.35, at.z0 + 0.35, 0.7, 5.0, &[], Mat::Limestone);
    kit::wall_x(b, at.x0, nar.x0 + 0.35, at.z1 - 0.35, 0.7, 5.0, &[], Mat::Limestone);
    kit::wall(b, kit::fz(), at.z0 + 0.7, at.z1 - 0.7, at.x0, at.x0 + 0.7, 0.0, 5.0, &[Op::Arch { s: at.cz(), w: 3.0, spring: 2.8 }], Mat::Limestone, Mat::Limestone);
    for k in 0..5 {
        let x = at.x0 + 3.0 + k as f32 * 3.0;
        column(b, Vec3::new(x, 0.0, at.z0 + 3.5), 3.6, 0.22, Mat::Marble);
        column(b, Vec3::new(x, 0.0, at.z1 - 3.5), 3.6, 0.22, Mat::Marble);
    }
    kit::lean_roof(b, Rect::new(at.x0, at.z0, nar.x0, at.z0 + 3.9), Side::N, 5.4, 3.9, 0.4);
    kit::lean_roof(b, Rect::new(at.x0, at.z1 - 3.9, nar.x0, at.z1), Side::S, 5.4, 3.9, 0.4);
    kit::lean_fill(b, Rect::new(at.x0, at.z0, nar.x0, at.z0 + 3.9), Side::N, 5.4, 3.9, 0.7, 5.0, &[(at.x0, at.x0 + 0.7, 5.0)], Mat::Limestone);
    kit::lean_fill(b, Rect::new(at.x0, at.z1 - 3.9, nar.x0, at.z1), Side::S, 5.4, 3.9, 0.7, 5.0, &[(at.x0, at.x0 + 0.7, 5.0)], Mat::Limestone);
    let ph = Vec3::new(at.cx() + 1.0, 0.0, at.cz());
    b.mesh(Mat::Marble).lathe(ph, &[(0.0, 0.0), (1.1, 0.0), (1.1, 0.7), (1.25, 0.75), (1.25, 0.85), (0.95, 0.85), (0.95, 0.3)], 18);
    b.mesh(Mat::Water).disc(ph + Vec3::Y * 0.65, 0.95, 16, true);
    b.mesh(Mat::Marble).lathe(ph, &[(0.15, 0.0), (0.15, 1.3), (0.3, 1.35), (0.0, 1.5)], 10);
    b.invisible.push(Aabb::new(ph - Vec3::new(1.1, 0.0, 1.1), ph + Vec3::new(1.1, 0.85, 1.1)));
    // Баптистерий: квадрат с куполом на барабане, внутри — крестообразная купель.
    let bp = p.baptistery;
    b.at(bp.cx(), bp.cz());
    kit::wall_x(b, bp.x0, bp.x1, bp.z0 + 0.35, 0.7, 5.5, &[Op::Window { s: bp.cx(), w: 0.8, sill: 3.0, top: 4.4 }], wall_mat);
    kit::wall_x(b, bp.x0, bp.x1, bp.z1 - 0.35, 0.7, 5.5, &[Op::Door { s: bp.cx(), w: 1.4, h: 2.6 }], wall_mat);
    kit::wall_z(b, bp.z0 + 0.7, bp.z1 - 0.7, bp.x0 + 0.35, 0.7, 5.5, &[Op::Window { s: bp.cz(), w: 0.8, sill: 3.0, top: 4.4 }], wall_mat);
    kit::wall_z(b, bp.z0 + 0.7, bp.z1 - 0.7, bp.x1 - 0.35, 0.7, 5.5, &[Op::Window { s: bp.cz(), w: 0.8, sill: 3.0, top: 4.4 }], wall_mat);
    // Переход из нефа к баптистерию.
    kit::wall_z(b, bp.z1, n.z0, bp.cx() - 1.2, 0.5, 3.4, &[], wall_mat);
    kit::wall_z(b, bp.z1, n.z0, bp.cx() + 1.2, 0.5, 3.4, &[], wall_mat);
    b.roof(Mat::Tile, |m| m.gable_roof_slopes(Vec2::new(bp.cx() - 1.6, bp.z1), Vec2::new(bp.cx() + 1.6, n.z0), 3.4, 4.2, false));
    let dc = bp.center(5.5);
    b.mesh(wall_mat).lathe(dc, &[(3.0, 0.0), (3.0, 1.2)], 16);
    b.mesh(Mat::Plaster).dome(dc + Vec3::Y * 1.2, 2.9, 0.0, std::f32::consts::TAU, 16, 0.0);
    b.roof(Mat::Tile, |m| m.dome(dc + Vec3::Y * 1.2, 3.1, 0.0, std::f32::consts::TAU, 16, 0.0));
    b.roof(Mat::Tile, |m| m.cuboid(Vec3::new(bp.x0 - 0.2, 5.5, bp.z0 - 0.2), Vec3::new(bp.x1 + 0.2, 5.7, bp.z1 + 0.2)));
    b.decor([bp.x0 + 0.7, 0.0, bp.z0 + 0.7], [bp.x1 - 0.7, 0.03, bp.z1 - 0.7], Mat::Marble);
    let c = bp.center(0.0);
    for (min, max) in [(Vec3::new(-1.4, 0.0, -0.5), Vec3::new(1.4, 0.5, 0.5)), (Vec3::new(-0.5, 0.0, -1.4), Vec3::new(0.5, 0.5, 1.4))] {
        b.solid((c + min).to_array(), (c + max).to_array(), Mat::Marble);
    }
    b.decor([c.x - 1.2, 0.5, c.z - 0.3], [c.x + 1.2, 0.52, c.z + 0.3], Mat::Water);
    b.decor([c.x - 0.3, 0.5, c.z - 1.2], [c.x + 0.3, 0.52, c.z + 1.2], Mat::Water);
    b.lamp(c + Vec3::Y * 3.0, Color::srgb(1.0, 0.75, 0.45), 10_000.0, 7.0);
}

// ---------------- ксенон и епископия ----------------

pub fn xenon_rect() -> Rect {
    let r = block(18, -2).union(&block(19, -2));
    Rect::new(r.x0 + 2.0, r.z0 + 2.0, r.x1 - 2.0, r.z1 - 1.0)
}

/// Ксенон (странноприимный дом и больница при епископии): палаты вокруг двора с аптекарским
/// огородом, кухня, комната врача (ятрейон), кладовая лекарств.
pub fn episcopium(b: &mut Builder) {
    let r = xenon_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(77);
    let peri = peristyle(b, r, 8.0, 4.4, Mat::Plaster, &[(Side::S, r.cx())], [3, 3, 2, 2], true);
    for (side, room) in &peri.rooms {
        match side {
            Side::N => beds_along_edge(b, *room, false, &mut rng),
            // Через среднюю южную комнату входят с улицы — это сени со скамьями для ждущих.
            Side::S if room.x0 < r.cx() && room.x1 > r.cx() => {
                kit::bench(b, Rect::new(room.x0 + 0.2, room.z0 + 0.6, room.x0 + 0.6, room.z1 - 0.6));
                kit::bench(b, Rect::new(room.x1 - 0.6, room.z0 + 0.6, room.x1 - 0.2, room.z1 - 0.6));
            }
            Side::S => beds_along_edge(b, *room, true, &mut rng),
            Side::W => {
                // Ятрейон: стол врача, полки со свитками и инструментами.
                let c = room.center(0.0);
                kit::table(b, Rect::new(c.x - 0.9, c.z - 0.5, c.x + 0.9, c.z + 0.5), 0.8);
                kit::shelf(b, Rect::new(room.x0 + 0.1, room.z0 + 0.3, room.x0 + 0.45, room.z1 - 0.3), 3, &mut rng, Mat::Canvas);
                b.lamp(c + Vec3::Y * 2.6, Color::srgb(1.0, 0.75, 0.45), 6_000.0, 7.0);
            }
            Side::E => {
                // Кладовая лекарств и кухня с очагом.
                kit::shelf(b, Rect::new(room.x1 - 0.45, room.z0 + 0.3, room.x1 - 0.1, room.z1 - 0.3), 3, &mut rng, Mat::Ceramic);
                kit::hearth(b, room.center(0.0) - Vec3::X * 0.8);
            }
        }
    }
    // Огород лекарственных трав во дворе: грядки и колодец.
    let c = peri.court.inset(3.4);
    for k in 0..4 {
        let z = c.z0 + 1.0 + k as f32 * (c.d() - 2.0) / 3.0;
        let bed = Rect::new(c.x0 + 1.0, z - 0.5, c.x1 - 1.0, z + 0.5);
        b.mesh(Mat::Earth).cuboid(bed.min3(0.0), bed.max3(0.2));
        let mats = [Mat::HerbSilver, Mat::HerbGreen, Mat::PetalYellow, Mat::PetalPurple];
        let n = (bed.w() / 0.6) as usize;
        for i in 0..n {
            let p = Vec3::new(bed.x0 + 0.3 + i as f32 * 0.6, 0.2, bed.cz());
            b.mesh(Mat::HerbGreen).lathe(p, &[(0.0, 0.0), (0.2, 0.1), (0.14, 0.3), (0.0, 0.35)], 6);
            b.mesh(mats[k]).lathe(p + Vec3::Y * 0.32, &[(0.0, 0.0), (0.06, 0.03), (0.0, 0.07)], 5);
        }
    }
    kit::cistern_mouth(b, Vec3::new(peri.court.cx(), 0.0, c.z1 + 1.0));
    // Люди и шкафы: монахи-параболаны в палатах, врач в ятрейоне, кухарка, больные у входа.
    let (mut ward, mut east) = (0, 0);
    for (side, room) in &peri.rooms {
        let c = room.center(0.0);
        match side {
            Side::N | Side::S if !(room.x0 < r.cx() && room.x1 > r.cx()) => {
                let role = if ward % 2 == 0 { Role::Monk } else { Role::Patient };
                extras(b, *room, L::Apothecary, if ward % 3 == 0 { &["Корзина с бинтами"] } else { &[] }, &[(role, c, c + Vec3::Z)], &mut rng);
                interior::lampstand(b, Vec3::new(room.x0 + 0.4, 0.0, room.cz()), false);
                ward += 1;
            }
            Side::N | Side::S => extras(b, *room, L::Household, &[], &[(Role::Patient, c + Vec3::X, c), (Role::Patient, c - Vec3::X * 1.2, c)], &mut rng),
            Side::W => extras(b, *room, L::Apothecary, &["Шкаф с лекарствами", "Ларь врача"], &[(Role::Physician, c + Vec3::Z * 1.0, c)], &mut rng),
            Side::E => {
                if east == 0 {
                    extras(b, *room, L::Apothecary, &["Шкаф с лекарствами"], &[(Role::Monk, c, c - Vec3::X)], &mut rng);
                } else {
                    extras(b, *room, L::Kitchen, &["Корзина с припасами"], &[(Role::Cook, c + Vec3::X, c - Vec3::X)], &mut rng);
                }
                east += 1;
            }
        }
    }
    for side in [Side::N, Side::S] {
        let z = if side == Side::N { peri.court.z0 - 4.0 } else { peri.court.z1 + 4.0 };
        b.lamp(Vec3::new(r.cx() - 6.0, 2.8, z), Color::srgb(1.0, 0.72, 0.45), 6_000.0, 8.0);
        b.lamp(Vec3::new(r.cx() + 6.0, 2.8, z), Color::srgb(1.0, 0.72, 0.45), 6_000.0, 8.0);
    }
}

/// Точка в южной палате ксенона (для механик больницы).
pub fn xenon_ward() -> Vec3 {
    let r = xenon_rect();
    Vec3::new(r.cx() + 2.0, 0.0, r.z1 - 4.2)
}

// ---------------- дидаскалион ----------------

pub fn didaskaleion_rect() -> Rect {
    block(15, -1).inset(0.5)
}

/// Школа (грамматика, риторика и медицина по Галену и Диоскориду): аудитория-экзедра со скамьями
/// и кафедрой учителя, книгохранилище со шкафами для свитков и кодексов.
pub fn didaskaleion(b: &mut Builder) {
    let r = didaskaleion_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(15);
    // С улицы — в вестибюль на всю ширину южного корпуса, из него — во двор.
    let peri = peristyle(b, r, 7.5, 4.6, Mat::Plaster, &[(Side::S, r.cx())], [1, 1, 2, 2], true);
    for (k, (side, room)) in peri.rooms.iter().enumerate() {
        match side {
            Side::N => {
                // Класс грамматиста: кафедра на помосте, ряды скамей лицом к ней (ученики сидят
                // с табличками на коленях), счётная доска, полка со свитками, светильник,
                // расписанная стена и балочный потолок.
                let c = room.center(0.0);
                b.solid([c.x - 0.6, 0.0, room.z0 + 0.3], [c.x + 0.6, 0.5, room.z0 + 1.2], Mat::Limestone);
                interior::chair(b, Vec3::new(c.x, 0.5, room.z0 + 0.75), Vec3::NEG_Z);
                for row in 0..3 {
                    let z = room.z0 + 2.4 + row as f32 * 1.2;
                    if z > room.z1 - 0.5 {
                        break;
                    }
                    for (x0, x1) in [(room.x0 + 1.0, c.x - 1.0), (c.x + 1.0, room.x1 - 1.0)] {
                        school_bench(b, Rect::new(x0, z, x1, z + 0.4));
                        // Свободные таблички и стили на краю скамьи.
                        if (row + (x0 > c.x) as usize) % 2 == 0 {
                            wax_tablet(b, Vec3::new(x1 - 0.3, 0.45, z + 0.2), 0.2);
                        }
                    }
                }
                classroom(b, *room, c, &mut rng);
            }
            Side::W | Side::E if k % 2 == 0 => {
                // Книгохранилище: шкафы-армарии со свитками у наружной стены и у торцов.
                let sh = if *side == Side::W { Rect::new(room.x0 + 0.1, room.z0 + 0.3, room.x0 + 0.5, room.z1 - 0.3) } else { Rect::new(room.x1 - 0.5, room.z0 + 0.3, room.x1 - 0.1, room.z1 - 0.3) };
                kit::shelf(b, sh, 4, &mut rng, Mat::Canvas);
                for z in [room.z0 + 0.1, room.z1 - 0.5] {
                    kit::shelf(b, Rect::new(room.x0 + 0.7, z, room.x1 - 0.7, z + 0.4), 4, &mut rng, Mat::Canvas);
                }
                let c = room.center(0.0);
                kit::table(b, Rect::new(c.x - 0.5, c.z - 0.8, c.x + 0.5, c.z + 0.8), 0.8);
            }
            // Вестибюль — проходной: скамьи для ждущих у стен.
            Side::S => {
                kit::bench(b, Rect::new(room.x0 + 0.3, room.z0 + 0.8, room.x0 + 0.7, room.z1 - 0.8));
                kit::bench(b, Rect::new(room.x1 - 0.7, room.z0 + 0.8, room.x1 - 0.3, room.z1 - 0.8));
            }
            _ => {
                let c = room.center(0.0);
                kit::table(b, Rect::new(c.x - 0.8, c.z - 0.4, c.x + 0.8, c.z + 0.4), 0.75);
            }
        }
    }
    b.decor([peri.court.x0, 0.0, peri.court.z0], [peri.court.x1, 0.03, peri.court.z1], Mat::Paving);
    for (k, (side, room)) in peri.rooms.iter().enumerate() {
        let c = room.center(0.0);
        match side {
            Side::N => {
                let cat = Vec3::new(c.x + 1.0, 0.0, room.z0 + 1.6);
                // Ученики сидят на скамьях (по двое в ряду), лицом к учителю.
                let seat = |i: usize| {
                    let row = (i / 2) as f32;
                    let x = if i % 2 == 0 { (room.x0 + 1.0 + c.x - 1.0) / 2.0 } else { (c.x + 1.0 + room.x1 - 1.0) / 2.0 };
                    Vec3::new(x, 0.0, room.z0 + 2.65 + row * 1.2)
                };
                let students: Vec<(Role, Vec3, Vec3)> = (0..4).map(|i| (Role::Student, seat(i), seat(i) - Vec3::Z)).collect();
                extras(b, *room, L::Library, &[], &[(Role::Scholar, cat, cat + Vec3::Z)], &mut rng);
                extras(b, *room, L::Library, &[], &students, &mut rng);
            }
            Side::W | Side::E if k % 2 == 0 => extras(b, *room, L::Library, &["Шкаф со свитками", "Ларь с кодексами"], &[(Role::Scholar, c + Vec3::Z * 1.3, c)], &mut rng),
            _ => {
                let mut room_l = interior::Room::new(b, room.inset(0.05));
                if let Some((f, _)) = room_l.take(1.2, 0.7, &[Side::N, Side::S, Side::W, Side::E], &mut rng) {
                    interior::desk_codex(b, f);
                }
                extras(b, *room, L::Library, &["Сундук"], &[(Role::Student, c, c + Vec3::X)], &mut rng);
            }
        }
    }
}

/// Школьная скамья без столкновений (ученики сидят прямо на ней).
fn school_bench(b: &mut Builder, r: Rect) {
    let h = 0.45;
    let m = b.mesh(Mat::Wood);
    m.cuboid(r.min3(h - 0.06), r.max3(h));
    for (x, z) in [(r.x0 + 0.08, r.z0 + 0.08), (r.x1 - 0.08, r.z0 + 0.08), (r.x0 + 0.08, r.z1 - 0.08), (r.x1 - 0.08, r.z1 - 0.08)] {
        m.cuboid(Vec3::new(x - 0.04, 0.0, z - 0.04), Vec3::new(x + 0.04, h - 0.06, z + 0.04));
    }
}

/// Восковая табличка со стилем, лежит на `p` (верх поверхности); `turn` — поворот.
fn wax_tablet(b: &mut Builder, p: Vec3, turn: f32) {
    let rot = Quat::from_rotation_y(turn);
    let (w, d) = (0.13, 0.09);
    let corners = |hw: f32, hd: f32, y0: f32, y1: f32| {
        let a = p + rot * Vec3::new(-hw, y0, -hd);
        let c = p + rot * Vec3::new(hw, y1, hd);
        (a.min(c), a.max(c))
    };
    let (a, c) = corners(w, d, 0.0, 0.018);
    b.mesh(Mat::Wood).cuboid(a, c);
    let (a, c) = corners(w - 0.015, d - 0.015, 0.018, 0.022);
    b.mesh(Mat::DarkStone).cuboid(a, c);
    let s0 = p + rot * Vec3::new(-0.1, 0.03, d + 0.02);
    let s1 = p + rot * Vec3::new(0.1, 0.03, d + 0.03);
    b.mesh(Mat::Bronze).beam(s0, s1, 0.005, 0.005, Vec3::Y);
}

/// Убранство класса: потолок с балками, расписанный пояс по стенам, счётная доска,
/// полка со свитками, светильники.
fn classroom(b: &mut Builder, room: Rect, c: Vec3, rng: &mut Rng) {
    // Потолок: дощатый настил на балках (поперёк класса).
    let top = 4.3;
    b.decor([room.x0, top, room.z0], [room.x1, top + 0.06, room.z1], Mat::Wood);
    let n = (room.w() / 1.1).floor().max(1.0) as usize;
    for k in 0..=n {
        let x = room.x0 + 0.1 + k as f32 * (room.w() - 0.2) / n as f32;
        b.decor([x - 0.08, top - 0.2, room.z0], [x + 0.08, top, room.z1], Mat::Wood);
    }
    // Стены: красный цокольный пояс (opus signinum) и полоса росписи над ним.
    let walls = [
        ([room.x0, 0.0, room.z0], [room.x1, 0.0, room.z0 + 0.03]),
        ([room.x0, 0.0, room.z0], [room.x0 + 0.03, 0.0, room.z1]),
        ([room.x1 - 0.03, 0.0, room.z0], [room.x1, 0.0, room.z1]),
    ];
    for (lo, hi) in walls {
        b.decor([lo[0], 0.05, lo[2]], [hi[0], 0.95, hi[2]], Mat::Signinum);
        b.decor([lo[0], 0.95, lo[2]], [hi[0], 1.07, hi[2]], Mat::Banner);
        b.decor([lo[0], 2.9, lo[2]], [hi[0], 3.0, hi[2]], Mat::Banner);
    }
    // Счётная доска (абак) на столике слева от кафедры: камешки в трёх столбцах.
    let ab = Rect::new(c.x - 2.6, room.z0 + 0.5, c.x - 1.4, room.z0 + 1.1);
    kit::table(b, ab, 0.8);
    let o = ab.center(0.8);
    b.mesh(Mat::Wood).cuboid(o + Vec3::new(-0.5, 0.0, -0.22), o + Vec3::new(0.5, 0.025, 0.22));
    for col in 0..3 {
        let x = o.x - 0.3 + col as f32 * 0.3;
        b.mesh(Mat::Iron).cuboid(Vec3::new(x - 0.004, o.y + 0.025, o.z - 0.2), Vec3::new(x + 0.004, o.y + 0.03, o.z + 0.2));
        for i in 0..(2 + rng.range(0.0, 6.0) as usize) {
            let q = Vec3::new(x - 0.06 + (i % 4) as f32 * 0.035 + 0.02, o.y + 0.03, o.z + 0.15 - (i / 4) as f32 * 0.04);
            b.mesh(Mat::Marble).cuboid(q - Vec3::new(0.012, 0.0, 0.012), q + Vec3::new(0.012, 0.014, 0.012));
        }
    }
    // Табличка и свиток учителя на кафедральном столике справа.
    let desk = Rect::new(c.x + 1.4, room.z0 + 0.4, c.x + 2.4, room.z0 + 1.0);
    kit::table(b, desk, 0.8);
    wax_tablet(b, desk.center(0.8) + Vec3::new(-0.2, 0.0, 0.0), -0.3);
    b.mesh(Mat::Canvas).beam(desk.center(0.83) + Vec3::new(0.12, 0.0, -0.1), desk.center(0.83) + Vec3::new(0.38, 0.0, 0.05), 0.035, 0.035, Vec3::Y);
    // Полка со свитками и табличками у боковой стены.
    kit::shelf(b, Rect::new(room.x0 + 0.1, room.z0 + 0.4, room.x0 + 0.5, room.z0 + 1.9), 3, rng, Mat::Canvas);
    // Светильники: на подставке у кафедры и подвесной над скамьями.
    interior::lampstand(b, Vec3::new(room.x1 - 0.5, 0.0, room.z0 + 0.6), true);
    b.lamp(c + Vec3::Y * 3.0, Color::srgb(1.0, 0.75, 0.45), 8_000.0, 9.0);
    interior::oil_lamp(b, Vec3::new(c.x, top - 0.9, c.z));
    b.mesh(Mat::Iron).beam(Vec3::new(c.x, top - 0.86, c.z), Vec3::new(c.x, top - 0.2, c.z), 0.006, 0.006, Vec3::X);
}

// ---------------- дом ссыльных ----------------

pub fn exiles_rect() -> Rect {
    block(13, -2).inset(0.4)
}

/// Богатый перистильный дом, где живут знатные ссыльные: мозаика, триклиний с ложами, фонтан.
pub fn exiles_house(b: &mut Builder) {
    let r = exiles_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(1313);
    b.loot(L::Wealthy);
    // Вход — в правую из двух южных комнат (между ними перегородка по оси дома).
    let peri = peristyle(b, r, 7.0, 5.0, Mat::Plaster, &[(Side::S, r.cx() + (r.w() - 14.0) / 4.0)], [1, 2, 3, 3], true);
    for (side, room) in &peri.rooms {
        b.decor([room.x0, 0.0, room.z0], [room.x1, 0.03, room.z1], Mat::Mosaic);
        match side {
            Side::N => {
                // Триклиний: три ложа буквой П вокруг стола.
                let c = room.center(0.0);
                kit::bed(b, Rect::new(c.x - 2.4, room.z0 + 0.3, c.x + 2.4, room.z0 + 1.2));
                kit::bed(b, Rect::new(c.x - 3.2, room.z0 + 1.4, c.x - 2.4, room.z1 - 0.6));
                kit::bed(b, Rect::new(c.x + 2.4, room.z0 + 1.4, c.x + 3.2, room.z1 - 0.6));
                kit::table(b, Rect::new(c.x - 0.7, c.z - 0.3, c.x + 0.7, c.z + 0.5), 0.55);
                polycandelon(b, c + Vec3::Y * 2.6, 4.9);
            }
            _ => {
                if rng.chance(0.5) {
                    kit::bed(b, Rect::new(room.x0 + 0.2, room.z0 + 0.2, room.x0 + 1.1, room.z0 + 2.1));
                    kit::chest(b, Vec3::new(room.x1 - 0.5, 0.0, room.z1 - 0.4), true);
                } else {
                    let mut rr = interior::Room::new(b, room.inset(0.05));
                    if let Some((f, _)) = rr.take(2.4, 0.35, &[Side::N, Side::S, Side::W, Side::E], &mut rng) {
                        kit::shelf(b, f, 3, &mut rng, Mat::Canvas);
                    }
                }
            }
        }
    }
    for (k, (side, room)) in peri.rooms.iter().enumerate() {
        let c = room.center(0.0);
        let mut rr = interior::Room::new(b, room.inset(0.05));
        if let Some((f, _)) = rr.take(0.4, 0.4, &[Side::N, Side::S, Side::W, Side::E], &mut rng) {
            interior::lampstand(b, f.center(0.0), k == 0);
        }
        match side {
            Side::N => extras(b, *room, L::Wealthy, &["Шкаф с серебром"], &[(Role::Noble, c + Vec3::Z * 0.4, c - Vec3::Z), (Role::Servant, c + Vec3::new(2.0, 0.0, 1.5), c)], &mut rng),
            _ if k % 3 == 1 => extras(b, *room, L::Wealthy, &["Шкаф"], &[(Role::Servant, c, c + Vec3::X)], &mut rng),
            _ if k % 3 == 2 => {
                if let Some((f, _)) = rr.take(1.2, 0.7, &[Side::N, Side::S, Side::W, Side::E], &mut rng) {
                    interior::desk_codex(b, f);
                }
                extras(b, *room, L::Library, &["Ларь со свитками"], &[], &mut rng);
            }
            _ => {
                let r2 = room.inset(0.9);
                if r2.w() > 0.8 && r2.d() > 0.8 {
                    interior::rug(b, r2, Mat::Rug);
                }
            }
        }
    }
    // Сад с фонтаном.
    let c = peri.court.center(0.0);
    b.mesh(Mat::Marble).lathe(c, &[(0.0, 0.0), (1.6, 0.0), (1.6, 0.5), (1.4, 0.5), (1.4, 0.15)], 18);
    b.mesh(Mat::Water).disc(c + Vec3::Y * 0.4, 1.4, 16, true);
    b.invisible.push(Aabb::new(c - Vec3::new(1.5, 0.0, 1.5), c + Vec3::new(1.5, 0.5, 1.5)));
    for (dx, dz) in [(-3.5, -2.5), (3.5, 2.5)] {
        kit::cypress(b, c + Vec3::new(dx, 0.0, dz), 6.0);
    }
}

// ---------------- термы ----------------

pub fn thermae_rect() -> Rect {
    block(12, 1).union(&block(13, 1))
}

/// Общественные бани: раздевальня (аподитерий), холодный зал с бассейном (фригидарий),
/// тёплый (тепидарий), горячий (кальдарий) с ванной, топка (префурний). Своды из плинфы.
pub fn thermae(b: &mut Builder) {
    let r = thermae_rect();
    b.at(r.cx(), r.cz());
    let mat = Mat::OpusMixtum;
    let z0 = r.z0 + 1.0;
    let z1 = z0 + 11.0;
    let h = 5.0;
    let xs = [r.x0 + 1.0, r.x0 + 14.0, r.x0 + 24.0, r.x0 + 32.0, r.x0 + 42.0, r.x1 - 1.0];
    // Наружные стены ряда залов.
    kit::wall_x(b, xs[0], xs[5], z0 + 0.4, 0.8, h, &[Op::Arch { s: xs[0] + 6.5, w: 2.4, spring: 2.6 }, Op::Window { s: xs[1] + 5.0, w: 1.0, sill: 1.8, top: 3.0 }, Op::Window { s: xs[3] + 5.0, w: 1.0, sill: 1.8, top: 3.0 }], mat);
    kit::wall_x(b, xs[0], xs[5], z1 - 0.4, 0.8, h, &[Op::Door { s: xs[0] + 6.5, w: 1.6, h: 2.6 }], mat);
    kit::wall_z(b, z0 + 0.8, z1 - 0.8, xs[0] + 0.4, 0.8, h, &[], mat);
    kit::wall_z(b, z0 + 0.8, z1 - 0.8, xs[5] - 0.4, 0.8, h, &[], mat);
    for &x in &xs[1..5] {
        kit::wall_z(b, z0 + 0.8, z1 - 0.8, x, 0.7, h, &[Op::Door { s: (z0 + z1) / 2.0, w: 1.3, h: 2.4 }], mat);
    }
    // Своды над каждым залом и черепичные кровли.
    for w in xs.windows(2) {
        let room = Rect::new(w[0] + 0.35, z0 + 0.8, w[1] - 0.35, z1 - 0.8);
        b.mesh(Mat::Plaster).barrel_vault(Vec2::new(room.x0, room.z0), Vec2::new(room.x1, room.z1), 3.4, true, 0.0);
        kit::gable_roof(b, Rect::new(w[0], z0, w[1], z1), h + 0.1, 4.5, true, 0.4, mat);
    }
    // Аподитерий: скамьи вдоль стен, ниши для одежды.
    let apo = Rect::new(xs[0] + 0.8, z0 + 0.8, xs[1] - 0.35, z1 - 0.8);
    kit::bench(b, Rect::new(apo.x0 + 0.1, apo.z0 + 3.0, apo.x0 + 0.6, apo.z1 - 0.2));
    kit::bench(b, Rect::new(apo.x0 + 2.0, apo.z1 - 0.6, apo.x1 - 0.2, apo.z1 - 0.1));
    b.decor([apo.x0, 0.0, apo.z0], [apo.x1, 0.03, apo.z1], Mat::Mosaic);
    // Фригидарий: холодный бассейн (бортик 0.6 м).
    let fr = Rect::new(xs[1] + 0.35, z0 + 0.8, xs[2] - 0.35, z1 - 0.8);
    pool(b, Rect::new(fr.x0 + 1.5, fr.z0 + 0.4, fr.x1 - 1.5, fr.z0 + 3.4));
    b.decor([fr.x0, 0.0, fr.z0], [fr.x1, 0.03, fr.z1], Mat::Marble);
    // Тепидарий и кальдарий: пол на гипокаусте приподнят на 0,4 м.
    for (k, w) in xs[2..5].windows(2).enumerate() {
        let room = Rect::new(w[0] + 0.35, z0 + 0.8, w[1] - 0.35, z1 - 0.8);
        b.solid([room.x0, 0.0, room.z0], [room.x1, 0.4, room.z1], Mat::Marble);
        if k == 1 {
            pool_raised(b, Rect::new(room.x0 + 1.0, room.z0 + 0.3, room.x1 - 1.0, room.z0 + 2.6), 0.4);
            b.lamp(room.center(3.0), Color::srgb(1.0, 0.7, 0.45), 8_000.0, 8.0);
        }
    }
    // Префурний: топка с углями под котлом.
    let pr = Rect::new(xs[4] + 0.35, z0 + 0.8, xs[5] - 0.8, z1 - 0.8);
    let f = pr.center(0.0);
    b.solid([f.x - 1.0, 0.0, f.z - 1.0], [f.x + 1.0, 1.0, f.z + 1.0], Mat::Brick);
    b.mesh(Mat::Coals).cuboid(Vec3::new(f.x - 1.01, 0.1, f.z - 0.4), Vec3::new(f.x - 0.95, 0.6, f.z + 0.4));
    b.mesh(Mat::Bronze).lathe(f + Vec3::Y * 1.0, &[(0.0, 0.0), (0.7, 0.05), (0.8, 0.5), (0.75, 0.9), (0.0, 0.95)], 14);
    b.lamp(f + Vec3::new(-1.4, 0.5, 0.0), Color::srgb(1.0, 0.45, 0.15), 15_000.0, 6.0);
    kit::woodpile(b, Rect::new(pr.x0 + 0.2, pr.z1 - 1.0, pr.x1 - 0.2, pr.z1 - 0.2), 1.0);
    b.lamp(apo.center(3.0), Color::srgb(1.0, 0.75, 0.45), 6_000.0, 8.0);
    b.lamp(fr.center(3.0), Color::srgb(1.0, 0.75, 0.45), 6_000.0, 8.0);
    let mut rng = Rng::new(1212);
    extras(b, apo, L::Household, &["Корзина с полотенцами", "Ларь банщика"], &[(Role::Bather, apo.center(0.0) + Vec3::X * 2.0, apo.center(0.0) - Vec3::X)], &mut rng);
    extras(b, pr, L::Workshop, &[], &[(Role::Servant, f + Vec3::new(-1.6, 0.0, 1.0), f)], &mut rng);
    extras(b, fr, L::Household, &[], &[(Role::Bather, fr.center(0.0) + Vec3::Z * 2.0, fr.center(0.0))], &mut rng);
    // Двор-палестра с портиком и колодцем за банями.
    let pal = Rect::new(r.x0 + 1.0, z1 + 1.0, r.x1 - 1.0, r.z1 - 1.0);
    b.decor([pal.x0, 0.0, pal.z0], [pal.x1, 0.03, pal.z1], Mat::Paving);
    kit::wall_x(b, pal.x0, pal.x1, pal.z1 - 0.3, 0.6, 3.2, &[], Mat::Rubble);
    kit::wall_z(b, z1, pal.z1 - 0.6, pal.x0 + 0.3, 0.6, 3.2, &[Op::Door { s: pal.cz(), w: 2.0, h: 2.6 }], Mat::Rubble);
    kit::wall_z(b, z1, pal.z1 - 0.6, pal.x1 - 0.3, 0.6, 3.2, &[Op::Door { s: pal.cz(), w: 2.0, h: 2.6 }], Mat::Rubble);
    for k in 0..8 {
        column(b, Vec3::new(pal.x0 + 4.0 + k as f32 * 5.8, 0.0, pal.z1 - 3.5), 3.4, 0.2, Mat::Limestone);
    }
    kit::lean_roof(b, Rect::new(pal.x0, pal.z1 - 3.9, pal.x1, pal.z1), Side::S, 4.4, 3.5, 0.3);
    kit::lean_fill(b, Rect::new(pal.x0, pal.z1 - 3.9, pal.x1, pal.z1), Side::S, 4.4, 3.5, 0.6, 3.2, &[(pal.x0, pal.x0 + 0.6, 3.2), (pal.x1 - 0.6, pal.x1, 3.2)], Mat::Rubble);
    kit::cistern_mouth(b, pal.center(0.0));
}

/// Бассейн с бортиком (стоит на полу, глубина условная).
fn pool(b: &mut Builder, r: Rect) {
    pool_raised(b, r, 0.0);
}

fn pool_raised(b: &mut Builder, r: Rect, base: f32) {
    let h = base + 0.6;
    let t = 0.3;
    b.solid([r.x0, 0.0, r.z0], [r.x1, h, r.z0 + t], Mat::Marble);
    b.solid([r.x0, 0.0, r.z1 - t], [r.x1, h, r.z1], Mat::Marble);
    b.solid([r.x0, 0.0, r.z0 + t], [r.x0 + t, h, r.z1 - t], Mat::Marble);
    b.solid([r.x1 - t, 0.0, r.z0 + t], [r.x1, h, r.z1 - t], Mat::Marble);
    b.decor([r.x0 + t, 0.0, r.z0 + t], [r.x1 - t, h - 0.15, r.z1 - t], Mat::Water);
}

// ---------------- театр ----------------

pub fn theatre_center() -> Vec3 {
    let r = block(14, 2).union(&block(15, 3));
    Vec3::new(r.cx(), 0.0, r.z0 + 34.0)
}

/// Театр IV–III вв. до н. э. В 510 г. заброшен: зрелища запрещены, ряды разбирают на камень.
/// Орхестра Ø 23 м, около 20 рядов кавеи, южнее — груды снятых блоков.
pub fn theatre(b: &mut Builder) {
    let c = theatre_center();
    b.at(c.x, c.z);
    let mut rng = Rng::new(2023);
    let r_orch = 11.5;
    b.mesh(Mat::Paving).disc(c + Vec3::Y * 0.03, r_orch, 32, true);
    // Ряды сидений — полукруг к югу (+Z), обращён к морю.
    let rows = 14;
    let segs = 28;
    for k in 0..rows {
        let r0 = r_orch + 1.4 + k as f32 * 0.85;
        let r1 = r0 + 0.85;
        let top = 0.4 * (k + 1) as f32;
        for s in 0..segs {
            // Камень разобран: пропуски, больше всего в верхних рядах.
            let robbed = rng.chance(0.05 + 0.45 * (k as f32 / rows as f32).powi(2));
            if robbed && k > 2 {
                // Остаётся только забутовка ниже.
                continue;
            }
            let a0 = PI * s as f32 / segs as f32;
            let a1 = PI * (s + 1) as f32 / segs as f32;
            let am = (a0 + a1) / 2.0;
            let rm = (r0 + r1) / 2.0;
            let len = rm * (a1 - a0);
            let p = c + Vec3::new(am.cos() * rm, top / 2.0, am.sin() * rm);
            b.mesh(Mat::Limestone).obox(p, Vec3::new((r1 - r0) / 2.0, top / 2.0, len / 2.0 + 0.02), am);
            // Коллизия — коробка поменьше, чтобы ряды не «наплывали» друг на друга.
            let q = c + Vec3::new(am.cos() * (rm - 0.1), 0.0, am.sin() * (rm - 0.1));
            let hw = 0.32 * (am.cos().abs() * (r1 - r0) + am.sin().abs() * len * 0.6).max(0.3);
            let hd = 0.32 * (am.sin().abs() * (r1 - r0) + am.cos().abs() * len * 0.6).max(0.3);
            b.invisible.push(Aabb::new(q - Vec3::new(hw, 0.0, hd), q + Vec3::new(hw, top, hd)));
        }
    }
    // Подпорные стены (аналеммы) по краям кавеи.
    let rmax = r_orch + 1.4 + rows as f32 * 0.85;
    for sx in [-1.0f32, 1.0] {
        let (a, e) = (c.x + sx * (r_orch + 0.6), c.x + sx * (rmax - 0.3));
        b.solid([a.min(e), 0.0, c.z - 0.6], [a.max(e), 2.0, c.z - 0.1], Mat::Limestone);
    }
    // Остатки сцены (скены): низкие стены, обломки колонн.
    let sz = c.z - r_orch - 4.0;
    kit::wall_x(b, c.x - 14.0, c.x - 4.0, sz, 1.0, 3.5, &[], Mat::Limestone);
    kit::wall_x(b, c.x + 4.0, c.x + 11.0, sz, 1.0, 2.2, &[], Mat::Limestone);
    kit::wall_x(b, c.x - 1.5, c.x + 1.5, sz, 1.0, 1.1, &[], Mat::Limestone);
    for k in 0..4 {
        let x = c.x - 12.0 + k as f32 * 7.0;
        column(b, Vec3::new(x, 0.0, sz + 2.0), rng.range(1.5, 4.2), 0.3, Mat::Marble);
    }
    // Упавшие барабаны колонн.
    for k in 0..5 {
        let p = Vec3::new(c.x + rng.range(-12.0, 12.0), 0.35, sz + rng.range(2.5, 6.0));
        let dir = if k % 2 == 0 { Vec3::X } else { Vec3::Z };
        b.mesh(Mat::Marble).beam(p - dir * 0.5, p + dir * 0.5, 0.33, 0.33, Vec3::Y);
        b.invisible.push(Aabb::new(p - Vec3::new(0.5, 0.35, 0.5), p + Vec3::new(0.5, 0.35, 0.5)));
    }
    // За кавеей — штабели снятых блоков и тачка: камень уходит на церкви.
    for k in 0..6 {
        let p = Vec3::new(c.x - 15.0 + k as f32 * 6.0, 0.0, c.z + rmax + 6.0 + rng.range(0.0, 8.0));
        let h = 0.6 * (1 + rng.int(3)) as f32;
        b.solid([p.x - 1.3, 0.0, p.z - 0.7], [p.x + 1.3, h, p.z + 0.7], Mat::Limestone);
    }
}

// ---------------- резервуар водопровода ----------------

pub fn reservoir_rect() -> Rect {
    let r = block(8, 3);
    // Котлован 28 × 13.4 м; дыра в рельефе по сетке 6 м.
    Rect::new(r.cx() - 6.7, r.cz() - 14.0, r.cx() + 6.7, r.cz() + 14.0)
}

/// Резервуар (кастеллум) водопровода II–III вв., 28 × 13.4 м, глубина 3.75 м: стены облицованы
/// цемянкой, вокруг — каменный парапет. Вода приходит по керамическим трубам с юга.
pub fn reservoir(b: &mut Builder) {
    let res = reservoir_rect();
    b.at(res.cx(), res.cz());
    let hole = snap_hole(res.inset(-1.0));
    b.holes.push((Vec2::new(hole.x0, hole.z0), Vec2::new(hole.x1, hole.z1)));
    floor_around(b, hole, res);
    let depth = 3.75;
    let t = 0.8;
    // Стенки котлована и дно.
    b.solid([res.x0 - t, -depth - 0.5, res.z0 - t], [res.x1 + t, -depth, res.z1 + t], Mat::Signinum);
    for (min, max) in [
        ([res.x0 - t, -depth, res.z0 - t], [res.x1 + t, 0.0, res.z0]),
        ([res.x0 - t, -depth, res.z1], [res.x1 + t, 0.0, res.z1 + t]),
        ([res.x0 - t, -depth, res.z0], [res.x0, 0.0, res.z1]),
        ([res.x1, -depth, res.z0], [res.x1 + t, 0.0, res.z1]),
    ] {
        b.solid(min, max, Mat::Signinum);
    }
    b.decor([res.x0, -depth, res.z0], [res.x1, -1.1, res.z1], Mat::Water);
    // Парапет.
    let ph = 1.0;
    b.solid([res.x0 - t, 0.0, res.z0 - t], [res.x1 + t, ph, res.z0 - t + 0.5], Mat::Limestone);
    b.solid([res.x0 - t, 0.0, res.z1 + t - 0.5], [res.x1 + t, ph, res.z1 + t], Mat::Limestone);
    b.solid([res.x0 - t, 0.0, res.z0 - t + 0.5], [res.x0 - t + 0.5, ph, res.z1 + t - 0.5], Mat::Limestone);
    b.solid([res.x1 + t - 0.5, 0.0, res.z0 - t + 0.5], [res.x1 + t, ph, res.z1 + t - 0.5], Mat::Limestone);
    // Водопровод: крытый каменный жёлоб с керамической трубой от южной стены.
    let x = res.cx();
    b.mesh(Mat::Limestone).cuboid(Vec3::new(x - 0.4, 0.0, res.z1 + t), Vec3::new(x + 0.4, 0.3, SOUTH_WALL_Z - WALL_T / 2.0));
    b.mesh(Mat::Ceramic).beam(Vec3::new(x, 0.62, res.z1 + t - 0.4), Vec3::new(x, 0.62, res.z1 - 0.2), 0.1, 0.1, Vec3::Y);
}

/// Дыра в рельефе, выровненная по сетке рельефа (кратно 6 м).
pub fn snap_hole(r: Rect) -> Rect {
    let c = terrain::CELL;
    let o = -terrain::WORLD_HALF;
    let f = |v: f32| ((v - o) / c).floor() * c + o;
    let g = |v: f32| ((v - o) / c).ceil() * c + o;
    Rect::new(f(r.x0), f(r.z0), g(r.x1), g(r.z1))
}

/// Пол из коробок (верх на 0) в дыре рельефа вокруг внутреннего прямоугольника.
pub fn floor_around(b: &mut Builder, hole: Rect, inner: Rect) {
    for (min, max) in [
        ([hole.x0, -0.6, hole.z0], [hole.x1, 0.0, inner.z0]),
        ([hole.x0, -0.6, inner.z1], [hole.x1, 0.0, hole.z1]),
        ([hole.x0, -0.6, inner.z0], [inner.x0, 0.0, inner.z1]),
        ([inner.x1, -0.6, inner.z0], [hole.x1, 0.0, inner.z1]),
    ] {
        if max[0] - min[0] > 0.01 && max[2] - min[2] > 0.01 {
            b.solid(min, max, Mat::Earth);
        }
    }
}

// ---------------- малая западная церковь ----------------

pub fn west_church_rect() -> Rect {
    let r = block(1, -1);
    Rect::new(r.x0 + 3.0, r.cz() - 4.5, r.x0 + 19.0, r.cz() + 4.5)
}

/// Однонефная церковь с апсидой и кладбищем во дворе.
pub fn west_church(b: &mut Builder) {
    let r = west_church_rect();
    b.at(r.cx(), r.cz());
    let mat = Mat::Rubble;
    let t = 0.7;
    kit::wall_x(b, r.x0, r.x1, r.z0 + t / 2.0, t, 5.5, &[Op::Window { s: r.cx(), w: 0.8, sill: 3.0, top: 4.4 }], mat);
    kit::wall_x(b, r.x0, r.x1, r.z1 - t / 2.0, t, 5.5, &[Op::Window { s: r.cx(), w: 0.8, sill: 3.0, top: 4.4 }], mat);
    kit::wall_z(b, r.z0 + t, r.z1 - t, r.x0 + t / 2.0, t, 5.5, &[Op::Door { s: r.cz(), w: 1.5, h: 2.6 }], mat);
    kit::wall(b, kit::fz(), r.z0 + t, r.z1 - t, r.x1 - t / 2.0 - 0.3, r.x1 + 0.3 - t / 2.0, 0.0, 5.5, &[Op::Arch { s: r.cz(), w: 3.6, spring: 2.4 }], mat, Mat::Limestone);
    let ac = Vec3::new(r.x1, 0.0, r.cz());
    b.mesh(mat).lathe_arc(ac, &[(2.4, 0.0), (2.4, 4.5)], -FRAC_PI_2, FRAC_PI_2, 10, false);
    b.mesh(Mat::Plaster).lathe_arc(ac, &[(1.8, 0.0), (1.8, 4.5)], -FRAC_PI_2, FRAC_PI_2, 10, true);
    // Конха изнутри оштукатурена (как в базилике): иначе из апсиды видна изнанка черепицы.
    b.mesh(Mat::Plaster).dome(ac + Vec3::Y * 4.5, 2.4, -FRAC_PI_2, FRAC_PI_2, 10, 0.0);
    b.roof(Mat::Tile, |m| m.dome(ac + Vec3::Y * 4.5, 2.5, -FRAC_PI_2, FRAC_PI_2, 10, 0.0));
    for k in 0..5 {
        let a = -FRAC_PI_2 + PI * (k as f32 + 0.5) / 5.0;
        let q = ac + Vec3::new(a.cos() * 2.1, 0.0, a.sin() * 2.1);
        b.invisible.push(Aabb::new(q - Vec3::new(0.5, 0.0, 0.5), q + Vec3::new(0.5, 4.5, 0.5)));
    }
    kit::gable_roof(b, r, 5.5, 2.4, true, 0.5, mat);
    b.solid([r.x1 - 1.6, 0.0, r.cz() - 0.6], [r.x1 - 0.6, 1.1, r.cz() + 0.6], Mat::Limestone);
    b.lamp(Vec3::new(r.x1 - 2.0, 2.4, r.cz()), Color::srgb(1.0, 0.72, 0.45), 5_000.0, 6.0);
    let mut rng = Rng::new(11);
    let inner = r.inset(t + 0.05);
    for k in 0..3 {
        kit::bench(b, Rect::new(inner.x0 + 2.0 + k as f32 * 2.2, inner.z0 + 0.4, inner.x0 + 2.4 + k as f32 * 2.2, inner.cz() - 1.2));
        kit::bench(b, Rect::new(inner.x0 + 2.0 + k as f32 * 2.2, inner.cz() + 1.2, inner.x0 + 2.4 + k as f32 * 2.2, inner.z1 - 0.4));
    }
    let altar = Vec3::new(r.x1 - 1.1, 0.0, r.cz());
    extras(b, inner, L::Church, &["Ризничий ларь"], &[(Role::Priest, altar - Vec3::X * 1.6, altar)], &mut rng);
    for z in [inner.z0 + 0.4, inner.z1 - 0.4] {
        interior::lampstand(b, Vec3::new(inner.x1 - 2.5, 0.0, z), false);
    }
    // Могилы с каменными крестами у церкви.
    for k in 0..8 {
        let p = Vec3::new(r.x0 + 1.0 + (k % 4) as f32 * 4.0, 0.0, r.z1 + 4.0 + (k / 4) as f32 * 3.5);
        grave(b, p, &mut rng);
    }
}

/// Христианская могила: каменная плита и крест/стела.
pub fn grave(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    b.mesh(Mat::Limestone).cuboid(p + Vec3::new(-0.4, 0.0, -1.0), p + Vec3::new(0.4, 0.2, 1.0));
    let s = p + Vec3::new(0.0, 0.0, -1.05);
    if rng.chance(0.6) {
        b.mesh(Mat::Limestone).cuboid(s + Vec3::new(-0.07, 0.0, -0.07), s + Vec3::new(0.07, 1.0, 0.07));
        b.mesh(Mat::Limestone).cuboid(s + Vec3::new(-0.3, 0.65, -0.07), s + Vec3::new(0.3, 0.8, 0.07));
    } else {
        b.mesh(Mat::Limestone).cuboid(s + Vec3::new(-0.3, 0.0, -0.08), s + Vec3::new(0.3, 0.9, 0.08));
    }
    b.invisible.push(Aabb::new(s - Vec3::new(0.3, 0.0, 0.1), s + Vec3::new(0.3, 0.9, 0.1)));
}

// ---------------- цитадель ----------------

/// Ворота цитадели (x): на южной стене замка, против улицы от главной.
pub fn citadel_gate_x() -> f32 {
    cross_x(1)
}

/// Донжон — главная башня цитадели (по наружным граням стен), в самом углу над обрывом.
pub fn keep_rect() -> Rect {
    let c = CITADEL;
    Rect::new(c.x0 + 0.75, c.z0 + 1.0, c.x0 + 13.75, c.z0 + 14.0)
}

/// Высота этажа донжона и число перекрытий (последнее — боевая площадка).
const KEEP_FLOOR: f32 = 4.8;
const KEEP_LEVELS: usize = 4;

/// Боевая площадка донжона: середина и высота.
pub fn keep_top() -> Vec3 {
    keep_rect().center(KEEP_FLOOR * KEEP_LEVELS as f32)
}

fn praetorium_rect() -> Rect {
    let c = CITADEL;
    Rect::new(c.x0 + 18.0, c.z0 + 1.0, c.x0 + 42.0, c.z0 + 21.0)
}

/// Двор претории.
pub fn praetorium_point() -> Vec3 {
    praetorium_rect().center(0.0)
}

/// Цитадель (кастрон) в северо-западном углу города, на скале над морем: с севера и запада её
/// закрывают городские стены над обрывом, от города отделяют свои стены с воротами на юг.
/// Внутри — донжон (башня в углу, с неё видно всё море), претория (дом командира), казармы
/// вдоль восточной стены, зернохранилище, оружейная мастерская, плац с чучелом и цистерной.
pub fn citadel(b: &mut Builder) {
    let c = CITADEL;
    b.at(c.cx(), c.cz());
    let gate = citadel_gate_x();
    let t = WALL_T;
    let mut rng = Rng::new(488);
    // Южная и восточная стены цитадели (в высоту — как городские: по стенам можно обойти кругом).
    let (sz, ex) = (c.z1 + t / 2.0, c.x1 + t / 2.0);
    kit::wall(b, kit::fx(), c.x0, c.x1 + t, c.z1, c.z1 + t, 0.0, WALL_H, &[Op::Arch { s: gate, w: 4.0, spring: 3.4 }], Mat::Limestone, Mat::Limestone);
    kit::wall(b, kit::fz(), c.z0 - 3.5, c.z1, c.x1, c.x1 + t, 0.0, WALL_H, &[], Mat::Limestone, Mat::Limestone);
    for (a, e) in [(gate - 2.4, c.x0), (c.x1 + t, gate + 2.4)] {
        walls::battlements(b, Vec2::new(a.max(e), sz), Vec2::new(a.min(e), sz), WALL_H, t / 2.0);
    }
    walls::battlements(b, Vec2::new(ex, c.z0 - 3.5), Vec2::new(ex, c.z1), WALL_H, t / 2.0);
    // Воротные башни наружу, к городу, и круглая башня на углу.
    for s in [-1.0f32, 1.0] {
        walls::rect_tower(b, Vec2::new(gate + s * 7.2, sz), Vec2::X, Vec2::Y);
    }
    walls::round_tower(b, Vec2::new(ex, sz), 5.2, 12.5);
    walls::rect_tower(b, Vec2::new(ex, c.cz() - 4.0), Vec2::Y, Vec2::X);
    // Створки ворот открыты внутрь.
    for s in [-1.0f32, 1.0] {
        let leaf = Vec3::new(gate + s * 1.94, 2.3, c.z1 - 1.0);
        b.mesh(Mat::Wood).obox(leaf, Vec3::new(1.1, 2.3, 0.07), std::f32::consts::FRAC_PI_2);
        for y in [0.6, 2.3, 4.0] {
            b.mesh(Mat::Iron).obox(leaf + Vec3::new(-s * 0.02, y - 2.3, 0.0), Vec3::new(1.05, 0.06, 0.08), std::f32::consts::FRAC_PI_2);
        }
    }
    b.torch(gate - 3.0, 3.4, c.z1 + t + 0.3, Vec3::Z);
    b.torch(gate + 3.0, 3.4, c.z1 + t + 0.3, Vec3::Z);
    b.torch(gate - 3.0, 3.4, c.z1 - 0.3, Vec3::NEG_Z);
    // Лестница на стену — внутри, вдоль южной стены.
    kit::stairs(b, Vec3::new(gate - 21.0, 0.0, c.z1 - 0.8), Vec3::X, 1.6, WALL_H, Mat::Limestone);
    keep(b);

    // Претория.
    let pr = praetorium_rect();
    b.at(pr.cx(), pr.cz());
    b.loot(L::Wealthy);
    let peri = peristyle(b, pr, 6.0, 4.8, Mat::OpusMixtum, &[(Side::S, pr.cx() + 3.0)], [2, 2, 1, 1], true);
    b.decor([peri.court.x0, 0.0, peri.court.z0], [peri.court.x1, 0.03, peri.court.z1], Mat::Paving);
    for (side, room) in &peri.rooms {
        if *side == Side::N {
            kit::table(b, Rect::new(room.cx() - 1.0, room.cz() - 0.5, room.cx() + 1.0, room.cz() + 0.5), 0.8);
            kit::chest(b, Vec3::new(room.x0 + 0.6, 0.0, room.z0 + 0.5), true);
            b.lamp(room.center(2.6), Color::srgb(1.0, 0.72, 0.45), 6_000.0, 7.0);
        } else if rng.chance(0.5) {
            kit::bed(b, Rect::new(room.x0 + 0.2, room.z0 + 0.2, room.x0 + 1.1, room.z0 + 2.1));
        }
    }
    for (k, (side, room)) in peri.rooms.iter().enumerate() {
        let c = room.center(0.0);
        let mut rr = interior::Room::new(b, room.inset(0.05));
        let all = [Side::N, Side::S, Side::W, Side::E];
        match (side, k % 2) {
            (Side::N, 0) => extras(b, *room, L::Wealthy, &["Шкаф с бумагами"], &[(Role::Officer, c + Vec3::Z * 1.0, c)], &mut rng),
            (Side::N, _) => {
                if let Some((f, _)) = rr.take(1.2, 0.7, &all, &mut rng) {
                    interior::desk_codex(b, f);
                }
                extras(b, *room, L::Library, &["Ларь с донесениями"], &[(Role::Servant, c, c + Vec3::X)], &mut rng);
            }
            (Side::S, _) => {
                if let Some((f, s)) = rr.take(1.6, 0.4, &all, &mut rng) {
                    interior::weapon_rack(b, f, s);
                }
                extras(b, *room, L::Barracks, &["Сундук"], &[(Role::Soldier, c, c + Vec3::Z)], &mut rng);
            }
            _ => extras(b, *room, L::Kitchen, &["Корзина с припасами"], &[(Role::Cook, c, c + Vec3::X)], &mut rng),
        }
    }

    // Казармы вдоль восточной стены: длинный корпус с комнатами на 8 солдат, двери во двор.
    let ba = Rect::new(c.x1 - 9.0, c.z0 + 2.0, c.x1, c.z1 - 2.0);
    b.at(ba.cx(), ba.cz());
    let rooms = 4;
    let step = ba.d() / rooms as f32;
    kit::wall_z(b, ba.z0, ba.z1, ba.x0 + 0.3, 0.6, 3.8, &(0..rooms).map(|k| Op::Door { s: ba.z0 + step * (k as f32 + 0.5), w: 1.2, h: 2.4 }).collect::<Vec<_>>(), Mat::Rubble);
    for k in 0..=rooms {
        let z = (ba.z0 + k as f32 * step).clamp(ba.z0 + 0.3, ba.z1 - 0.3);
        kit::wall_x(b, ba.x0 + 0.6, ba.x1, z, if k == 0 || k == rooms { 0.6 } else { 0.45 }, 3.8, &[], Mat::Rubble);
    }
    for k in 0..rooms {
        let room = Rect::new(ba.x0 + 0.8, ba.z0 + k as f32 * step + 0.5, ba.x1 - 0.2, ba.z0 + (k + 1) as f32 * step - 0.5);
        // Нары вдоль задней стены, стойка с копьями.
        beds_along(b, Rect::new(room.x1 - 2.2, room.z0, room.x1, room.z1), &mut rng);
        for q in 0..4 {
            let z = room.z0 + 0.5 + q as f32 * 0.25;
            b.mesh(Mat::Wood).beam(Vec3::new(room.x0 + 0.35, 0.0, z), Vec3::new(room.x0 + 0.35, 2.6, z), 0.015, 0.015, Vec3::X);
            b.mesh(Mat::Iron).spire(Vec3::new(room.x0 + 0.35, 2.6, z), 0.04, 0.25, 4);
        }
        let rc = room.center(0.0);
        let soldiers: Vec<(Role, Vec3, Vec3)> = if k % 2 == 0 { vec![(Role::Soldier, rc, rc - Vec3::X)] } else { vec![] };
        let mut rr = interior::Room::new(b, room.inset(0.05));
        if let Some((f, _)) = rr.take(1.4, 0.7, &[Side::N, Side::S, Side::W], &mut rng) {
            kit::table(b, f, 0.75);
            interior::table_setting(b, f.inset(0.1), 0.75, &mut rng);
        }
        extras(b, room, L::Barracks, &["Сундук стратиота", "Сундук стратиота"], &soldiers, &mut rng);
    }
    kit::lean_roof(b, ba, Side::E, 5.0, 3.85, 0.5);
    // Высокий край — у стены цитадели; торцы — до черепицы.
    kit::lean_fill(b, ba, Side::E, 5.0, 3.85, 0.0, 0.0, &[(ba.z0, ba.z0 + 0.6, 3.8), (ba.z1 - 0.6, ba.z1, 3.8)], Mat::Rubble);

    // Горреум — зернохранилище.
    let ho = Rect::new(c.x0 + 45.0, c.z0 + 1.5, c.x0 + 62.0, c.z0 + 12.5);
    b.at(ho.cx(), ho.cz());
    kit::wall_x(b, ho.x0, ho.x1, ho.z0 + 0.4, 0.8, 5.0, &[], Mat::Limestone);
    kit::wall_x(b, ho.x0, ho.x1, ho.z1 - 0.4, 0.8, 5.0, &[Op::Door { s: ho.cx(), w: 2.2, h: 3.0 }], Mat::Limestone);
    kit::wall_z(b, ho.z0 + 0.8, ho.z1 - 0.8, ho.x0 + 0.4, 0.8, 5.0, &[], Mat::Limestone);
    kit::wall_z(b, ho.z0 + 0.8, ho.z1 - 0.8, ho.x1 - 0.4, 0.8, 5.0, &[], Mat::Limestone);
    kit::gable_roof(b, ho, 5.0, 2.2, true, 0.5, Mat::Limestone);
    for k in 0..5 {
        let p = Vec3::new(ho.x0 + 2.0 + k as f32 * 2.7, 0.0, ho.z0 + 2.0);
        b.solid([p.x - 0.5, 0.0, p.z - 0.4], [p.x + 0.5, 0.8, p.z + 0.4], Mat::Canvas);
        kit::pithos(b, p + Vec3::Z * 2.4, 1.5, 0.0);
    }
    extras(b, ho.inset(0.85), L::Storehouse, &["Ларь с зерном", "Корзина"], &[(Role::Servant, ho.center(0.0) + Vec3::Z * 2.0, ho.center(0.0))], &mut rng);

    // Плац: чучело для упражнений, цистерна, поленница, коновязь.
    let yard = garrison_point();
    b.at(yard.x, yard.z);
    let pell = yard + Vec3::new(5.0, 0.0, 2.0);
    b.solid([pell.x - 0.15, 0.0, pell.z - 0.15], [pell.x + 0.15, 1.9, pell.z + 0.15], Mat::Wood);
    b.mesh(Mat::Wood).beam(pell + Vec3::new(-0.55, 1.45, 0.0), pell + Vec3::new(0.55, 1.45, 0.0), 0.05, 0.05, Vec3::Y);
    kit::cistern_mouth(b, yard + Vec3::new(-12.0, 0.0, -3.0));
    b.torch(yard.x, 3.0, c.z1 - 0.3, Vec3::NEG_Z);
    let m = b.mesh(Mat::Wood);
    let rail = yard + Vec3::new(14.0, 0.0, 6.0);
    for dx in [0.0f32, 4.0] {
        m.beam(rail + Vec3::X * dx, rail + Vec3::new(dx, 1.1, 0.0), 0.06, 0.06, Vec3::X);
    }
    m.beam(rail + Vec3::new(-0.3, 1.05, 0.0), rail + Vec3::new(4.3, 1.05, 0.0), 0.05, 0.05, Vec3::Y);
    kit::hidden(b, rail - Vec3::new(0.1, 0.0, 0.1), rail + Vec3::new(4.1, 1.1, 0.1));

    // Оружейная мастерская гарнизона (фабрика): навес к югу от донжона, открыт на плац.
    let ar = armoury_rect();
    b.at(ar.cx(), ar.cz());
    kit::wall_x(b, ar.x0, ar.x1, ar.z0 + 0.3, 0.6, 4.2, &[], Mat::Rubble);
    // Боковые стены — ниже низкого края ската (иначе прорезают черепицу), выше — кладка по скату.
    kit::wall_z(b, ar.z0 + 0.6, ar.z1, ar.x0 + 0.3, 0.6, 3.75, &[], Mat::Rubble);
    kit::wall_z(b, ar.z0 + 0.6, ar.z1, ar.x1 - 0.3, 0.6, 3.75, &[], Mat::Rubble);
    for x in [ar.x0 + 5.5, ar.x1 - 5.5] {
        b.solid([x - 0.2, 0.0, ar.z1 - 0.5], [x + 0.2, 3.6, ar.z1 - 0.1], Mat::Wood);
    }
    b.mesh(Mat::Wood).cuboid(Vec3::new(ar.x0, 3.6, ar.z1 - 0.5), Vec3::new(ar.x1, 3.9, ar.z1 - 0.1));
    kit::lean_roof(b, ar, Side::N, 5.0, 3.9, 0.5);
    kit::lean_fill(b, ar, Side::N, 5.0, 3.9, 0.6, 4.2, &[(ar.x0, ar.x0 + 0.6, 3.75), (ar.x1 - 0.6, ar.x1, 3.75)], Mat::Rubble);
    kit::forge_set(b, "Оружейная гарнизона", Vec3::new(ar.x0 + 2.4, 0.0, ar.z0 + 3.0), Vec3::X, false);
    // Кольчуги на крестовинах и связки копий у стены.
    for k in 0..3 {
        let p = Vec3::new(ar.x1 - 1.2 - k as f32 * 1.4, 0.0, ar.z0 + 1.0);
        b.mesh(Mat::Wood).cuboid(p - Vec3::new(0.04, 0.0, 0.04), p + Vec3::new(0.04, 1.6, 0.04));
        b.mesh(Mat::Iron).lathe(p + Vec3::Y * 0.8, &[(0.0, 0.0), (0.32, 0.05), (0.3, 0.7), (0.22, 0.78), (0.0, 0.8)], 8);
        kit::hidden(b, p - Vec3::new(0.35, 0.0, 0.35), p + Vec3::new(0.35, 1.6, 0.35));
    }
    kit::woodpile(b, Rect::new(ar.x1 - 4.0, ar.z1 - 2.0, ar.x1 - 1.0, ar.z1 - 1.0), 0.8);
}

/// Донжон: квадратная башня в четыре яруса с каменными лестницами вдоль стен и боевой
/// площадкой наверху. Такие башни-пирги ставили в крепостях при Анастасии и Юстиниане:
/// последний рубеж обороны, дозор и склад оружия.
fn keep(b: &mut Builder) {
    let k = keep_rect();
    b.at(k.cx(), k.cz());
    let t = 1.5;
    let fh = KEEP_FLOOR;
    let top = fh * KEEP_LEVELS as f32;
    let inn = k.inset(t);
    let lime = Mat::Limestone;
    // Цоколь с откосом и угловые камни.
    for (x, z) in [(k.x0, k.z0), (k.x1, k.z0), (k.x0, k.z1), (k.x1, k.z1)] {
        let (sx, sz) = (if x > k.cx() { -1.0 } else { 1.0 }, if z > k.cz() { -1.0 } else { 1.0 });
        kit::quoins(b, x + sx * 0.35, z + sz * 0.35, top, 0.7);
    }
    for level in 0..KEEP_LEVELS {
        let (y0, y1) = (level as f32 * fh, (level + 1) as f32 * fh);
        // Окна-бойницы: внизу узкие щели, выше — пошире, с подоконником на высоте груди.
        let slit = |s: f32| Op::Window { s, w: if level == 0 { 0.35 } else { 0.6 }, sill: if level == 0 { 2.2 } else { 1.2 }, top: if level == 0 { 3.4 } else { 2.6 } };
        let door = Op::Door { s: k.cx() + 2.5, w: 1.4, h: 2.5 };
        let south: Vec<Op> = if level == 0 { vec![door] } else { vec![slit(k.cx() - 2.0), slit(k.cx() + 2.0)] };
        kit::wall(b, kit::fx(), k.x0, k.x1, k.z0, k.z0 + t, y0, y1, &[slit(k.cx())], lime, lime);
        kit::wall(b, kit::fx(), k.x0, k.x1, k.z1 - t, k.z1, y0, y1, &south, lime, lime);
        kit::wall(b, kit::fz(), k.z0 + t, k.z1 - t, k.x0, k.x0 + t, y0, y1, &[slit(k.cz())], lime, lime);
        kit::wall(b, kit::fz(), k.z0 + t, k.z1 - t, k.x1 - t, k.x1, y0, y1, &[slit(k.cz())], lime, lime);
        // Пояс-карниз между ярусами.
        b.mesh(lime).cuboid(Vec3::new(k.x0 - 0.12, y1 - 0.3, k.z0 - 0.12), Vec3::new(k.x1 + 0.12, y1 - 0.08, k.z0));
        b.mesh(lime).cuboid(Vec3::new(k.x0 - 0.12, y1 - 0.3, k.z1), Vec3::new(k.x1 + 0.12, y1 - 0.08, k.z1 + 0.12));
        b.mesh(lime).cuboid(Vec3::new(k.x0 - 0.12, y1 - 0.3, k.z0), Vec3::new(k.x0, y1 - 0.08, k.z1));
        b.mesh(lime).cuboid(Vec3::new(k.x1, y1 - 0.3, k.z0), Vec3::new(k.x1 + 0.12, y1 - 0.08, k.z1));
        // Лестница на следующий ярус: по чётным — вдоль западной стены на север, по нечётным —
        // вдоль восточной на юг. Марш — 12 ступеней.
        let west = level % 2 == 0;
        let (sw, run) = (1.3, 7.2);
        let (start, dir, hole) = if west {
            (Vec3::new(inn.x0 + sw / 2.0, y0, inn.z1 - 0.8), Vec3::NEG_Z, Rect::new(inn.x0, inn.z1 - 0.8 - run, inn.x0 + sw + 0.1, inn.z1))
        } else {
            (Vec3::new(inn.x1 - sw / 2.0, y0, inn.z0 + 0.8), Vec3::Z, Rect::new(inn.x1 - sw - 0.1, inn.z0, inn.x1, inn.z0 + 0.8 + run))
        };
        kit::stairs(b, start, dir, sw, fh, lime);
        // Перекрытие над ярусом — дубовый настил на балках, с проёмом над маршем.
        let slab = if level + 1 == KEEP_LEVELS { lime } else { Mat::Wood };
        let (ya, yb) = (y1 - 0.28, y1);
        if west {
            b.solid([hole.x1, ya, inn.z0], [inn.x1, yb, inn.z1], slab);
            b.solid([inn.x0, ya, inn.z0], [hole.x1, yb, hole.z0], slab);
            b.solid([hole.x1, yb, hole.z0 + 1.4], [hole.x1 + 0.14, yb + 0.95, inn.z1], Mat::Wood);
        } else {
            b.solid([inn.x0, ya, inn.z0], [hole.x0, yb, inn.z1], slab);
            b.solid([hole.x0, ya, hole.z1], [inn.x1, yb, inn.z1], slab);
            b.solid([hole.x0 - 0.14, yb, inn.z0], [hole.x0, yb + 0.95, hole.z1 - 1.4], Mat::Wood);
        }
        if level + 1 < KEEP_LEVELS {
            let m = b.mesh(Mat::Wood);
            let mut x = inn.x0 + 2.2;
            while x < inn.x1 - 1.6 {
                m.beam(Vec3::new(x, ya - 0.14, inn.z0), Vec3::new(x, ya - 0.14, inn.z1), 0.1, 0.14, Vec3::Y);
                x += 2.2;
            }
        }
        b.lamp(Vec3::new(k.cx(), y0 + 3.2, k.cz()), Color::srgb(1.0, 0.7, 0.42), 9_000.0, 9.0);
    }
    // Что хранят в башне: внизу — припасы на осаду, выше — оружейная и караульня.
    let mut rng = Rng::new(0x51E6E);
    for q in 0..4 {
        kit::pithos(b, Vec3::new(inn.x1 - 1.0, 0.0, inn.z0 + 1.2 + q as f32 * 1.5), 1.45, 0.0);
    }
    interior::sacks(b, Rect::new(inn.x0 + 2.4, inn.z0 + 0.2, inn.x0 + 6.0, inn.z0 + 0.9), &mut rng);
    b.loot(L::Storehouse);
    kit::chest(b, Vec3::new(inn.cx() + 1.0, 0.0, inn.z0 + 0.6), true);
    b.placed_in(Vec3::Y * fh, crate::level::chunk_of(k.center(0.0)), |b| {
        interior::weapon_rack(b, Rect::new(inn.x0 + 2.6, inn.z0 + 0.05, inn.x0 + 4.4, inn.z0 + 0.5), Side::N);
        interior::weapon_rack(b, Rect::new(inn.x0 + 5.0, inn.z0 + 0.05, inn.x0 + 6.8, inn.z0 + 0.5), Side::N);
        b.loot(L::Barracks);
        kit::chest(b, Vec3::new(inn.x0 + 3.2, 0.0, inn.z1 - 0.6), true);
    });
    b.placed_in(Vec3::Y * fh * 2.0, crate::level::chunk_of(k.center(0.0)), |b| {
        kit::table(b, Rect::new(inn.cx() - 1.1, inn.cz() - 0.5, inn.cx() + 1.1, inn.cz() + 0.5), 0.8);
        kit::bench(b, Rect::new(inn.cx() - 1.1, inn.cz() + 0.8, inn.cx() + 1.1, inn.cz() + 1.2));
        kit::bed(b, Rect::new(inn.x0 + 2.0, inn.z0 + 0.15, inn.x0 + 3.0, inn.z0 + 2.1));
        kit::bed(b, Rect::new(inn.x0 + 3.6, inn.z0 + 0.15, inn.x0 + 4.6, inn.z0 + 2.1));
        b.loot(L::Barracks);
        kit::chest(b, Vec3::new(inn.x0 + 5.6, 0.0, inn.z0 + 0.6), true);
    });
    b.placed_in(Vec3::Y * fh * 3.0, crate::level::chunk_of(k.center(0.0)), |b| {
        b.loot(L::Wealthy);
        kit::chest(b, Vec3::new(inn.x0 + 3.0, 0.0, inn.z1 - 0.6), true);
        interior::desk_codex(b, Rect::new(inn.cx() - 0.6, inn.z0 + 0.2, inn.cx() + 0.6, inn.z0 + 0.9));
    });
    // Боевая площадка: зубцы по краю, жаровня сигнального огня.
    let m = b.mesh(lime);
    m.cuboid(Vec3::new(k.x0 - 0.2, top - 0.25, k.z0 - 0.2), Vec3::new(k.x1 + 0.2, top, k.z0 + 0.5));
    m.cuboid(Vec3::new(k.x0 - 0.2, top - 0.25, k.z1 - 0.5), Vec3::new(k.x1 + 0.2, top, k.z1 + 0.2));
    m.cuboid(Vec3::new(k.x0 - 0.2, top - 0.25, k.z0 + 0.5), Vec3::new(k.x0 + 0.5, top, k.z1 - 0.5));
    m.cuboid(Vec3::new(k.x1 - 0.5, top - 0.25, k.z0 + 0.5), Vec3::new(k.x1 + 0.2, top, k.z1 - 0.5));
    // Пол площадки над стенами и парапет.
    for (lo, hi) in [
        ([k.x0, top - 0.28, k.z0], [k.x1, top, k.z0 + t]),
        ([k.x0, top - 0.28, k.z1 - t], [k.x1, top, k.z1]),
        ([k.x0, top - 0.28, k.z0 + t], [k.x0 + t, top, k.z1 - t]),
        ([k.x1 - t, top - 0.28, k.z0 + t], [k.x1, top, k.z1 - t]),
    ] {
        b.solid(lo, hi, lime);
    }
    for (lo, hi) in [
        ([k.x0 - 0.2, top, k.z0 - 0.2], [k.x1 + 0.2, top + 0.9, k.z0 + 0.4]),
        ([k.x0 - 0.2, top, k.z1 - 0.4], [k.x1 + 0.2, top + 0.9, k.z1 + 0.2]),
        ([k.x0 - 0.2, top, k.z0 + 0.4], [k.x0 + 0.4, top + 0.9, k.z1 - 0.4]),
        ([k.x1 - 0.4, top, k.z0 + 0.4], [k.x1 + 0.2, top + 0.9, k.z1 - 0.4]),
    ] {
        b.solid(lo, hi, lime);
    }
    let n = 5;
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32;
        let (x, z) = (k.x0 + k.w() * f, k.z0 + k.d() * f);
        for (c, half) in [
            (Vec3::new(x, top + 1.6, k.z0 + 0.1), Vec3::new(0.75, 0.7, 0.3)),
            (Vec3::new(x, top + 1.6, k.z1 - 0.1), Vec3::new(0.75, 0.7, 0.3)),
            (Vec3::new(k.x0 + 0.1, top + 1.6, z), Vec3::new(0.3, 0.7, 0.75)),
            (Vec3::new(k.x1 - 0.1, top + 1.6, z), Vec3::new(0.3, 0.7, 0.75)),
        ] {
            b.mesh(lime).obox(c, half, 0.0);
        }
    }
    let fire = Vec3::new(k.cx() - 2.0, top, k.cz() + 2.0);
    b.mesh(Mat::Iron).lathe(fire, &[(0.08, 0.0), (0.06, 0.9), (0.42, 1.0), (0.48, 1.25), (0.4, 1.25), (0.36, 1.05), (0.0, 1.02)], 10);
    b.mesh(Mat::Coals).disc(fire + Vec3::Y * 1.2, 0.38, 10, true);
    kit::hidden(b, fire - Vec3::new(0.3, 0.0, 0.3), fire + Vec3::new(0.3, 1.25, 0.3));
    b.torch(fire.x, top + 1.45, fire.z, Vec3::ZERO);
}

/// Оружейная мастерская гарнизона в цитадели.
pub fn armoury_rect() -> Rect {
    let c = CITADEL;
    Rect::new(c.x0 + 0.25, c.z0 + 17.0, c.x0 + 16.25, c.z0 + 27.0)
}

/// Середина плаца цитадели.
pub fn garrison_point() -> Vec3 {
    Vec3::new(CITADEL.x0 + 40.0, 0.0, CITADEL.z1 - 10.0)
}
