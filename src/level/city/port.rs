//! Порт в Карантинной бухте: каменная набережная за восточной стеной, деревянные причалы,
//! торговый корабль (по образцу затонувших судов VI–VII вв.: ~20 м, одна мачта с латинским
//! парусом, два рулевых весла, камбуз с черепичной крышей), рыбацкие лодки, кран, грузы;
//! внутри стен — торг (эмпорий), таможня (коммеркиарий) и склады-горреумы.

use super::kit::{self, Op, Rect, Rng, Side};
use super::public::{column, peristyle, stall};
use super::*;
use super::ships;
use super::interior::{self, extras};
use crate::items::LootTableId as L;
use crate::level::Role;
use crate::collision::Aabb;
use std::f32::consts::FRAC_PI_2;

pub const QUAY_TOP: f32 = 0.0;
pub const PIER_Z: [f32; 3] = [-70.0, 30.0, 110.0];
const PIER_LEN: f32 = 38.0;
const PIER_W: f32 = 4.0;
/// Сходни: причал, сторона (−1 — к меньшему z), отступ от края набережной.
const GANGWAYS: [(f32, f32, f32); 4] = [(PIER_Z[1], -1.0, 16.0), (PIER_Z[1], 1.0, 18.0), (PIER_Z[0], 1.0, 18.0), (PIER_Z[2], -1.0, 17.0)];

pub fn port(b: &mut Builder) {
    quay(b);
    market(b);
    customs(b);
    horrea(b);
}

fn quay(b: &mut Builder) {
    let edge = terrain::QUAY_EDGE;
    let (z0, z1) = terrain::QUAY_Z;
    let mut rng = Rng::new(285);
    let mut z = z0;
    while z < z1 {
        let zz = (z + 32.0).min(z1);
        b.at(edge - 6.0, (z + zz) / 2.0);
        // Лицевая стенка набережной из крупных блоков и мощение.
        b.solid([edge - 1.2, -6.0, z], [edge, QUAY_TOP, zz], Mat::Limestone);
        b.decor([EAST_WALL_X + WALL_T / 2.0, QUAY_TOP, z], [edge - 1.2, QUAY_TOP + 0.04, zz], Mat::Paving);
        b.mesh(Mat::Limestone).cuboid(Vec3::new(edge - 1.3, QUAY_TOP, z), Vec3::new(edge + 0.05, QUAY_TOP + 0.2, zz));
        // Невидимое ограждение края (кроме выходов на причалы).
        let mut s = z;
        for &pz in PIER_Z.iter().filter(|&&pz| pz > z - PIER_W && pz < zz + PIER_W) {
            if pz - PIER_W / 2.0 > s {
                b.invisible.push(Aabb::new(Vec3::new(edge - 0.1, QUAY_TOP, s), Vec3::new(edge + 0.3, QUAY_TOP + 1.3, pz - PIER_W / 2.0)));
            }
            s = pz + PIER_W / 2.0;
        }
        if zz > s {
            b.invisible.push(Aabb::new(Vec3::new(edge - 0.1, QUAY_TOP, s), Vec3::new(edge + 0.3, QUAY_TOP + 1.3, zz)));
        }
        z = zz;
    }
    // Тумбы для швартовки.
    let mut z = z0 + 6.0;
    while z < z1 - 4.0 {
        let p = Vec3::new(edge - 0.6, QUAY_TOP, z);
        b.mesh(Mat::Limestone).lathe(p, &[(0.25, 0.0), (0.22, 0.6), (0.3, 0.7), (0.0, 0.75)], 10);
        kit::hidden(b, p - Vec3::new(0.25, 0.0, 0.25), p + Vec3::new(0.25, 0.75, 0.25));
        z += 14.0;
    }
    // Причалы на сваях.
    for &pz in &PIER_Z {
        b.at(edge + PIER_LEN / 2.0, pz);
        let r = Rect::new(edge, pz - PIER_W / 2.0, edge + PIER_LEN, pz + PIER_W / 2.0);
        b.solid([r.x0, QUAY_TOP - 0.25, r.z0], [r.x1, QUAY_TOP, r.z1], Mat::Wood);
        let m = b.mesh(Mat::Wood);
        let mut x = r.x0 + 2.0;
        while x < r.x1 {
            for zz in [r.z0 + 0.2, r.z1 - 0.2] {
                m.lathe(Vec3::new(x, -5.0, zz), &[(0.15, 0.0), (0.15, 5.4), (0.0, 5.45)], 6);
            }
            x += 3.0;
        }
        // Ограждение краёв причала — с проходами к сходням.
        for (dir, z0, z1) in [(-1.0f32, r.z0 - 0.3, r.z0), (1.0, r.z1, r.z1 + 0.3)] {
            let mut x = r.x0;
            for gx in GANGWAYS.iter().filter(|g| g.0 == pz && g.1 == dir).map(|g| edge + g.2) {
                b.invisible.push(Aabb::new(Vec3::new(x, 0.0, z0), Vec3::new(gx - 0.45, 1.3, z1)));
                x = gx + 0.45;
            }
            b.invisible.push(Aabb::new(Vec3::new(x, 0.0, z0), Vec3::new(r.x1, 1.3, z1)));
        }
        b.invisible.push(Aabb::new(Vec3::new(r.x1, 0.0, r.z0), Vec3::new(r.x1 + 0.3, 1.3, r.z1)));
        b.torch(r.x1 - 0.5, 1.6, r.z0 + 0.3, Vec3::ZERO);
    }
    moored_ships(b, edge);
    // Грузовая стрела у портовых ворот.
    b.at(edge - 4.0, 14.0);
    crane(b, Vec3::new(edge - 2.5, QUAY_TOP, 14.0));
    // Грузы на набережной: штабели амфор, тюки, сети.
    for (k, z) in [-40.0, -12.0, 22.0, 52.0, 80.0, 128.0].into_iter().enumerate() {
        b.at(edge - 6.0, z);
        let x0 = edge - 8.5;
        match k % 3 {
            0 => {
                for row in 0..3 {
                    for i in 0..8 - row * 2 {
                        let p = Vec3::new(x0 + 0.5 + i as f32 * 0.38 + row as f32 * 0.38, 0.17 + row as f32 * 0.3, z);
                        kit::amphora_lying(b, p, false, 1.0);
                    }
                }
                kit::hidden(b, Vec3::new(x0 + 0.2, 0.0, z - 0.4), Vec3::new(x0 + 3.6, 0.9, z + 0.4));
            }
            1 => {
                for i in 0..4 {
                    let p = Vec3::new(x0 + i as f32 * 1.0, 0.0, z + rng.range(-0.3, 0.3));
                    b.solid([p.x, 0.0, p.z - 0.45], [p.x + 0.9, 0.7, p.z + 0.45], Mat::Canvas);
                }
            }
            _ => {
                // Сети сушатся на жердях.
                let m = b.mesh(Mat::Wood);
                for x in [x0, x0 + 4.0] {
                    m.beam(Vec3::new(x, 0.0, z), Vec3::new(x, 2.0, z), 0.04, 0.04, Vec3::X);
                }
                let c = b.mesh(Mat::Canvas);
                let q = [Vec3::new(x0, 1.95, z), Vec3::new(x0 + 4.0, 1.95, z), Vec3::new(x0 + 4.0, 0.6, z + 0.1), Vec3::new(x0, 0.6, z + 0.1)];
                c.quad(q, Vec3::Z);
                c.quad(q, Vec3::NEG_Z);
            }
        }
    }
}

/// Суда у причалов и на рейде; сходни с причалов на палубы.
fn moored_ships(b: &mut Builder, edge: f32) {
    let w = terrain::WATER_Y;
    let gap = 0.7;
    // Средний причал: с юга — навикулярий из Синопы, с севера — большой александриец с отданным парусом.
    let nav = ships::NAVICULARIUS;
    let z = PIER_Z[1] - PIER_W / 2.0 - gap - nav.beam / 2.0;
    let m = ships::ship(b, &nav, Vec3::new(edge + 19.0, w, z), 0.0);
    board(b, &m, PIER_Z[1] - PIER_W / 2.0, -1.0);
    let alx = ships::ALEXANDRIAN;
    let z = PIER_Z[1] + PIER_W / 2.0 + gap + alx.beam / 2.0;
    let m = ships::ship(b, &alx, Vec3::new(edge + 21.0, w, z), 0.0);
    board(b, &m, PIER_Z[1] + PIER_W / 2.0, 1.0);
    // Южный причал: кондура с Боспора и лодки.
    let kon = ships::KONDURA;
    let z = PIER_Z[0] + PIER_W / 2.0 + gap + kon.beam / 2.0;
    let m = ships::ship(b, &kon, Vec3::new(edge + 21.0, w, z), 0.0);
    board(b, &m, PIER_Z[0] + PIER_W / 2.0, 1.0);
    ships::skiff(b, Vec3::new(edge + 6.0, w, PIER_Z[0] + PIER_W / 2.0 + 1.6), 0.05);
    ships::skiff(b, Vec3::new(edge + 24.0, w, PIER_Z[0] - PIER_W / 2.0 - 1.4), -0.06);
    // Северный причал: лазский купец из Трапезунта с подобранным парусом и лодка.
    let laz = ships::Ship { len: 17.0, beam: 4.8, depth: 2.7, draft: 1.25, stripe: Mat::Cloth, cargo: 3, ..ships::NAVICULARIUS };
    let z = PIER_Z[2] - PIER_W / 2.0 - gap - laz.beam / 2.0;
    let m = ships::ship(b, &laz, Vec3::new(edge + 20.0, w, z), 0.0);
    board(b, &m, PIER_Z[2] - PIER_W / 2.0, -1.0);
    ships::skiff(b, Vec3::new(edge + 16.0, w, PIER_Z[2] + PIER_W / 2.0 + 1.5), 0.0);
    // Моряки на причалах у своих судов: чинят сети, перекатывают грузы (дальше — по распорядку дня).
    for &(pz, dir, gx) in &GANGWAYS {
        for dx in [-6.0f32, 7.0] {
            let p = Vec3::new(edge + gx + dx, QUAY_TOP, pz + dir * 1.1);
            let look = Vec3::new(edge + gx, QUAY_TOP, pz + dir * 3.0);
            let d = look - p;
            b.resident(p, (-d.x).atan2(-d.z), Role::Sailor);
        }
    }
    // На рейде: дромон флота на якоре, вдоль набережной, и рыбаки в лодках.
    ships::ship(b, &ships::DROMON, Vec3::new(edge + 62.0, w, 62.0), FRAC_PI_2);
    ships::skiff(b, Vec3::new(edge + 48.0, w, -28.0), 0.9);
    ships::skiff(b, Vec3::new(edge + 55.0, w, 150.0), -2.2);
}

/// Сходни с края причала `pier_z` на палубу судна, которое лежит по `dir` от причала.
fn board(b: &mut Builder, m: &ships::Moored, pier_z: f32, dir: f32) {
    let x = GANGWAYS.iter().find(|g| g.0 == pier_z.round() - dir * (PIER_W / 2.0) && g.1 == dir).map_or(m.mid.x - 3.0, |g| terrain::QUAY_EDGE + g.2);
    let from = Vec3::new(x, QUAY_TOP, pier_z - dir * 0.6);
    let to = Vec3::new(x, m.deck_y + 0.45, pier_z + dir * 1.5);
    ships::gangway(b, from, to);
}

/// Грузовая стрела: две наклонные ноги, растяжка, блок с канатом и подвешенной амфорой в сетке.
fn crane(b: &mut Builder, base: Vec3) {
    let m = b.mesh(Mat::Wood);
    let top = base + Vec3::new(3.4, 7.0, 0.0);
    for dz in [-1.6f32, 1.6] {
        m.beam(base + Vec3::new(0.0, 0.0, dz), top, 0.14, 0.14, Vec3::Y);
    }
    m.beam(top, base + Vec3::new(-4.0, 0.0, 0.0), 0.03, 0.03, Vec3::Z);
    m.beam(top, top - Vec3::Y * 4.0, 0.015, 0.015, Vec3::X);
    kit::amphora(b, top - Vec3::Y * 4.7, 1.0, 2);
    // Ворот у основания.
    let w = base + Vec3::new(-1.2, 0.6, 0.0);
    b.mesh(Mat::Wood).beam(w - Vec3::Z * 0.8, w + Vec3::Z * 0.8, 0.18, 0.18, Vec3::Y);
    for dz in [-0.9f32, 0.9] {
        b.solid([w.x - 0.15, 0.0, w.z + dz - 0.08], [w.x + 0.15, 0.8, w.z + dz + 0.08], Mat::Wood);
    }
    for dz in [-1.6f32, 1.6] {
        kit::hidden(b, base + Vec3::new(-0.2, 0.0, dz - 0.2), base + Vec3::new(0.2, 1.5, dz + 0.2));
    }
}

// ---------------- эмпорий, таможня, склады ----------------

pub fn market_rect() -> Rect {
    block(18, 1).inset(0.5)
}

/// Портовый торг: лавки вокруг двора с лотками; в одной из комнат — корчма для моряков.
fn market(b: &mut Builder) {
    let r = market_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(1818);
    let peri = peristyle(b, r, 6.0, 4.2, Mat::Rubble, &[(Side::N, r.cx()), (Side::E, r.cz())], [1, 2, 3, 3], false);
    b.decor([peri.court.x0, 0.0, peri.court.z0], [peri.court.x1, 0.03, peri.court.z1], Mat::Paving);
    let staffed = super::super::places::staffed_stalls("port_market");
    for (k, (p, facing)) in market_stalls().into_iter().enumerate().skip(staffed) {
        stall(b, p, facing, k + 1, &mut rng);
    }
    for (side, room) in &peri.rooms {
        match side {
            Side::S => {
                // Корчма: столы, скамьи, амфоры с вином, очаг.
                let c = room.center(0.0);
                kit::table(b, Rect::new(c.x - 1.0, c.z - 0.4, c.x + 1.0, c.z + 0.4), 0.8);
                kit::bench(b, Rect::new(c.x - 1.0, c.z + 0.7, c.x + 1.0, c.z + 1.1));
                kit::hearth(b, Vec3::new(room.x1 - 1.0, 0.0, room.cz()));
                for k in 0..4 {
                    kit::amphora(b, Vec3::new(room.x0 + 0.4 + k as f32 * 0.45, 0.0, room.z1 - 0.35), 1.0, k as u32);
                }
                b.lamp(c + Vec3::Y * 2.6, Color::srgb(1.0, 0.7, 0.4), 7_000.0, 7.0);
            }
            _ => {
                for k in 0..3 {
                    kit::amphora(b, Vec3::new(room.x0 + 0.5 + k as f32 * 0.45, 0.0, room.z0 + 0.4), 1.0, rng.int(3) as u32);
                }
            }
        }
    }
    // Лавочники в лавках, моряки в корчме.
    for (k, (side, room)) in peri.rooms.iter().enumerate() {
        let c = room.center(0.0);
        let court = peri.court.center(0.0);
        if *side == Side::S {
            extras(b, *room, L::Storehouse, &["Ларь корчмаря"], &[(Role::Sailor, c + Vec3::X * 1.6, c), (Role::Sailor, c - Vec3::X * 1.8, c)], &mut rng);
        } else {
            let mut rr = interior::Room::new(b, room.inset(0.05));
            if let Some((f, _)) = rr.take(1.2, 0.6, &[Side::N, Side::S, Side::W, Side::E], &mut rng) {
                interior::sacks(b, f, &mut rng);
            }
            let keeper: Vec<(Role, Vec3, Vec3)> = if k % 2 == 0 { vec![(Role::Shopkeeper, c, court)] } else { vec![] };
            extras(b, *room, L::Storehouse, &["Корзина с товаром", "Ларь"], &keeper, &mut rng);
        }
    }
}

pub fn tavern_port_point() -> Vec3 {
    // Южный корпус торга, сразу за дверью из двора (см. `market`).
    let c = market_rect().inset(6.0);
    Vec3::new(c.x0 + 2.5, 0.0, c.z1 + 0.9)
}

pub fn customs_point() -> Vec3 {
    let c = customs_rect().inset(6.5);
    Vec3::new(c.cx(), 0.0, c.z0 + 3.0)
}

/// Лотки портового торга: середина стола и куда он обращён. Двор узкий и длинный, поэтому
/// лотки стоят у западной и восточной стен лицом друг к другу — посередине остаётся проход
/// от северных ворот к корчме; ставятся между дверями лавок, чтобы их не загораживать.
pub fn market_stalls() -> [(Vec3, Vec3); 4] {
    let c = market_rect().inset(6.0);
    let (z0, z1) = (c.z0 + c.d() / 3.0, c.z0 + c.d() * 2.0 / 3.0);
    let w = c.x0 + 1.95;
    let e = c.x1 - 1.95;
    [
        (Vec3::new(w, 0.0, z0), Vec3::X),
        (Vec3::new(e, 0.0, z0), Vec3::NEG_X),
        (Vec3::new(w, 0.0, z1), Vec3::X),
        (Vec3::new(e, 0.0, z1), Vec3::NEG_X),
    ]
}

/// Прилавки навклиров на набережной у корней причалов, спиной к морю, лицом к городу:
/// александриец, лаз и боспорец — каждый у своего корабля. Отступы по Z — между тумбами.
pub fn quay_stalls() -> [(Vec3, Vec3); 3] {
    let x = terrain::QUAY_EDGE - 1.0 - super::super::places::SELLER_BEHIND;
    [
        (Vec3::new(x, QUAY_TOP, PIER_Z[1] + 4.5), Vec3::NEG_X),
        (Vec3::new(x, QUAY_TOP, PIER_Z[2] - 5.5), Vec3::NEG_X),
        (Vec3::new(x, QUAY_TOP, PIER_Z[0] + 5.0), Vec3::NEG_X),
    ]
}

pub fn market_point() -> Vec3 {
    market_rect().inset(6.0).center(0.0)
}

pub fn customs_rect() -> Rect {
    block(19, 1).inset(0.5)
}

/// Таможня (коммеркиарий): здесь взимали пошлину, на которую при Зеноне чинили стены.
/// Стол мытаря, безмен-весы, свинцовые печати, сложенные товары.
fn customs(b: &mut Builder) {
    let r = customs_rect();
    b.at(r.cx(), r.cz());
    let mut rng = Rng::new(1919);
    let peri = peristyle(b, r, 6.5, 4.6, Mat::OpusMixtum, &[(Side::E, r.z0 + 10.0), (Side::N, r.cx())], [1, 2, 2, 2], false);
    b.decor([peri.court.x0, 0.0, peri.court.z0], [peri.court.x1, 0.03, peri.court.z1], Mat::Paving);
    let court = peri.court;
    if let Some((_, room)) = peri.rooms.iter().find(|(s, r)| *s == Side::N && r.cx() > court.x0 && r.cx() < court.x1) {
        let c = room.center(0.0);
        kit::table(b, Rect::new(c.x - 1.2, c.z - 0.5, c.x + 1.2, c.z + 0.5), 0.85);
        b.loot(L::Storehouse);
        kit::chest(b, Vec3::new(c.x + 2.0, 0.0, room.z0 + 0.5), true);
        b.lamp(c + Vec3::Y * 2.6, Color::srgb(1.0, 0.72, 0.45), 6_000.0, 7.0);
        interior::desk_codex(b, Rect::new(c.x - 2.6, room.z0 + 0.3, c.x - 1.6, room.z0 + 1.0));
        extras(b, *room, L::Wealthy, &["Ларь с пошлиной"], &[(Role::Officer, c + Vec3::Z * 1.0, c)], &mut rng);
    }
    // Безмен на треноге посреди двора.
    let s = peri.court.center(0.0);
    let m = b.mesh(Mat::Wood);
    for k in 0..3 {
        let a = k as f32 * 2.094;
        m.beam(Vec3::new(s.x + a.cos() * 1.0, 0.0, s.z + a.sin() * 1.0), s + Vec3::Y * 2.6, 0.05, 0.05, Vec3::Y);
    }
    b.mesh(Mat::Bronze).beam(s + Vec3::new(-0.8, 2.2, 0.0), s + Vec3::new(1.0, 2.25, 0.0), 0.025, 0.025, Vec3::Y);
    b.mesh(Mat::Bronze).lathe(s + Vec3::new(0.9, 1.6, 0.0), &[(0.0, 0.0), (0.12, 0.05), (0.1, 0.25), (0.0, 0.3)], 8);
    kit::hidden(b, s - Vec3::new(0.3, 0.0, 0.3), s + Vec3::new(0.3, 2.5, 0.3));
    for k in 0..5 {
        let p = Vec3::new(peri.court.x0 + 1.0, 0.0, peri.court.z0 + 1.0 + k as f32 * 1.0);
        b.solid([p.x, 0.0, p.z], [p.x + 0.9, 0.6 + rng.range(0.0, 0.4), p.z + 0.8], Mat::Canvas);
    }
}

/// Склады-горреумы между главной улицей и цитаделью.
fn horrea(b: &mut Builder) {
    for i in 17..=19 {
        let r = block(i, 2).inset(0.5);
        b.at(r.cx(), r.cz());
        let mut rng = Rng::new(i as u32 * 31);
        kit::wall_x(b, r.x0, r.x1, r.z0 + 0.4, 0.8, 5.0, &[Op::Door { s: r.cx(), w: 2.6, h: 3.2 }], Mat::Limestone);
        kit::wall_x(b, r.x0, r.x1, r.z1 - 0.4, 0.8, 5.0, &[Op::Door { s: r.cx(), w: 2.6, h: 3.2 }], Mat::Limestone);
        kit::wall_z(b, r.z0 + 0.8, r.z1 - 0.8, r.x0 + 0.4, 0.8, 5.0, &[Op::Window { s: r.cz() - 8.0, w: 0.6, sill: 3.4, top: 4.4 }, Op::Window { s: r.cz() + 8.0, w: 0.6, sill: 3.4, top: 4.4 }], Mat::Limestone);
        kit::wall_z(b, r.z0 + 0.8, r.z1 - 0.8, r.x1 - 0.4, 0.8, 5.0, &[Op::Window { s: r.cz() - 8.0, w: 0.6, sill: 3.4, top: 4.4 }, Op::Window { s: r.cz() + 8.0, w: 0.6, sill: 3.4, top: 4.4 }], Mat::Limestone);
        kit::gable_roof(b, r, 5.0, 3.0, false, 0.5, Mat::Limestone);
        // Средний ряд столбов под коньковую балку.
        for k in 0..5 {
            let z = r.z0 + 6.0 + k as f32 * (r.d() - 12.0) / 4.0;
            column(b, Vec3::new(r.cx(), 0.0, z), 5.5, 0.22, Mat::Wood);
        }
        // Пифосы и амфоры рядами вдоль стен, проход посередине.
        for k in 0..9 {
            let z = r.z0 + 4.0 + k as f32 * 4.4;
            if z > r.z1 - 3.0 {
                break;
            }
            kit::pithos(b, Vec3::new(r.x0 + 1.8, 0.0, z), 1.5, 0.0);
            if rng.chance(0.7) {
                kit::pithos(b, Vec3::new(r.x1 - 1.8, 0.0, z), 1.5, 0.0);
            } else {
                for q in 0..3 {
                    kit::amphora(b, Vec3::new(r.x1 - 1.2 - q as f32 * 0.45, 0.0, z), 1.0, q as u32);
                }
            }
        }
        b.lamp(r.center(3.5), Color::srgb(1.0, 0.72, 0.45), 4_000.0, 8.0);
        let roles: Vec<(Role, Vec3, Vec3)> = if i == 18 { vec![(Role::Servant, r.center(0.0) + Vec3::new(-1.0, 0.0, 3.0), r.center(0.0))] } else { vec![] };
        extras(b, r.inset(0.85), L::Storehouse, &["Ларь", "Корзина", "Ларь"], &roles, &mut rng);
    }
}
