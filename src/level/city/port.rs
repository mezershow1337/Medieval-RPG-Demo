//! Порт в Карантинной бухте: каменная набережная за восточной стеной, деревянные причалы,
//! торговый корабль (по образцу затонувших судов VI–VII вв.: ~20 м, одна мачта с латинским
//! парусом, два рулевых весла, камбуз с черепичной крышей), рыбацкие лодки, кран, грузы;
//! внутри стен — торг (эмпорий), таможня (коммеркиарий) и склады-горреумы.

use super::kit::{self, Op, Rect, Rng, Side};
use super::public::{column, peristyle, stall};
use super::*;
use super::interior::{self, extras};
use crate::items::LootTableId as L;
use crate::level::Role;
use crate::collision::Aabb;
use std::f32::consts::FRAC_PI_2;

pub const QUAY_TOP: f32 = 0.0;
pub const PIER_Z: [f32; 3] = [-70.0, 30.0, 110.0];
const PIER_LEN: f32 = 38.0;
const PIER_W: f32 = 4.0;

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
        b.invisible.push(Aabb::new(Vec3::new(r.x0, 0.0, r.z0 - 0.3), Vec3::new(r.x1, 1.3, r.z0)));
        b.invisible.push(Aabb::new(Vec3::new(r.x0, 0.0, r.z1), Vec3::new(r.x1, 1.3, r.z1 + 0.3)));
        b.invisible.push(Aabb::new(Vec3::new(r.x1, 0.0, r.z0), Vec3::new(r.x1 + 0.3, 1.3, r.z1)));
        b.torch(r.x1 - 0.5, 1.6, r.z0 + 0.3, Vec3::ZERO);
    }
    // Торговый корабль у среднего причала и лодки.
    b.at(edge + 20.0, PIER_Z[1]);
    merchant_ship(b, Vec3::new(edge + 19.0, -1.2, PIER_Z[1] - PIER_W / 2.0 - 3.4), 0.0);
    b.at(edge + 20.0, PIER_Z[0]);
    boat(b, Vec3::new(edge + 12.0, -1.2, PIER_Z[0] + PIER_W / 2.0 + 1.6), 0.05);
    boat(b, Vec3::new(edge + 24.0, -1.2, PIER_Z[0] - PIER_W / 2.0 - 1.6), -0.08);
    b.at(edge + 20.0, PIER_Z[2]);
    boat(b, Vec3::new(edge + 16.0, -1.2, PIER_Z[2] + PIER_W / 2.0 + 1.6), 0.0);
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

/// Торговое судно: корпус из шпангоутов, палуба, камбуз, мачта с реей и подобранным парусом.
pub fn merchant_ship(b: &mut Builder, waterline: Vec3, yaw: f32) {
    let len = 20.0;
    let beam = 5.2;
    let depth = 2.6;
    let (s, c) = yaw.sin_cos();
    let fwd = Vec3::new(c, 0.0, s);
    let side = Vec3::new(-s, 0.0, c);
    let keel_y = waterline.y - 1.4;
    let n = 16;
    let m = 6;
    // Точка корпуса: t — от кормы (0) к носу (1), u — от киля (0) к планширю (1), sd — борт.
    let pt = |t: f32, u: f32, sd: f32| -> Vec3 {
        let x = (t - 0.5) * len;
        let fullness = (1.0 - (2.0 * t - 1.0).abs().powf(2.2)).max(0.0).sqrt();
        let half = beam / 2.0 * fullness * (u.powf(0.6)).min(1.0);
        let sheer = 0.6 * (2.0 * t - 1.0).powi(2);
        let y = keel_y + depth * u + sheer * u;
        waterline + fwd * x + side * (half * sd) + Vec3::Y * (y - waterline.y)
    };
    let hull = b.mesh(Mat::Wood);
    for sd in [-1.0f32, 1.0] {
        for i in 0..n {
            for j in 0..m {
                let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
                let (u0, u1) = (j as f32 / m as f32, (j + 1) as f32 / m as f32);
                let q = [pt(t0, u0, sd), pt(t1, u0, sd), pt(t1, u1, sd), pt(t0, u1, sd)];
                let out = side * sd + Vec3::NEG_Y * (1.0 - u0) * 0.5;
                hull.quad(q, out);
                hull.quad(q, -out);
            }
        }
    }
    // Палуба и планширь.
    let deck_y = keel_y + depth * 0.86;
    let deck = b.mesh(Mat::Wood);
    for i in 0..n {
        let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        let a0 = pt(t0, 0.86, -1.0);
        let a1 = pt(t1, 0.86, -1.0);
        let b1 = pt(t1, 0.86, 1.0);
        let b0 = pt(t0, 0.86, 1.0);
        let fix = |p: Vec3| Vec3::new(p.x, deck_y, p.z);
        deck.quad([fix(a0), fix(a1), fix(b1), fix(b0)], Vec3::Y);
    }
    // Штевни.
    let stem = waterline + fwd * (len / 2.0);
    deck.beam(stem + Vec3::Y * (keel_y - waterline.y), stem + Vec3::Y * (depth + 1.3) + fwd * 0.6, 0.12, 0.12, side);
    let stern = waterline - fwd * (len / 2.0);
    deck.beam(stern + Vec3::Y * (keel_y - waterline.y), stern + Vec3::Y * (depth + 1.4) - fwd * 0.8, 0.12, 0.12, side);
    // Мачта, рея (латинская — наискосок), подобранный парус.
    let mast = waterline + fwd * 2.0 + Vec3::Y * (deck_y - waterline.y);
    b.mesh(Mat::Wood).lathe(mast, &[(0.17, 0.0), (0.12, 10.0), (0.0, 10.2)], 8);
    let ya = mast + Vec3::Y * 9.0 + fwd * 6.5;
    let yb = mast + Vec3::Y * 3.2 - fwd * 6.0;
    b.mesh(Mat::Wood).beam(ya, yb, 0.08, 0.08, side);
    b.mesh(Mat::Canvas).beam(ya - Vec3::Y * 0.25 - (ya - yb).normalize() * 0.6, yb - Vec3::Y * 0.25 + (ya - yb).normalize() * 0.8, 0.22, 0.3, side);
    // Снасти.
    for p in [stem + Vec3::Y * (depth + 1.0), stern + Vec3::Y * (depth + 1.0), pt(0.6, 1.0, 1.0), pt(0.6, 1.0, -1.0)] {
        b.mesh(Mat::Wood).beam(mast + Vec3::Y * 9.8, p, 0.012, 0.012, Vec3::Y);
    }
    // Камбуз на корме с черепичной крышей.
    let g = waterline - fwd * 6.5 + Vec3::Y * (deck_y - waterline.y);
    let yaw_box = yaw;
    b.mesh(Mat::Wood).obox(g + Vec3::Y * 0.9, Vec3::new(1.6, 0.9, 1.5), yaw_box);
    b.mesh(Mat::Tile).obox(g + Vec3::Y * 1.9, Vec3::new(1.8, 0.12, 1.7), yaw_box);
    // Рулевые вёсла.
    for sd in [-1.0f32, 1.0] {
        let top = pt(0.06, 1.0, sd) + side * sd * 0.3 + Vec3::Y * 0.5;
        let blade = top - Vec3::Y * 3.6 - fwd * 1.2 + side * sd * 0.2;
        b.mesh(Mat::Wood).beam(top, blade, 0.08, 0.08, side);
        b.mesh(Mat::Wood).obox(blade + Vec3::Y * 0.6, Vec3::new(0.6, 0.7, 0.05), yaw);
    }
    // Груз на палубе: амфоры.
    for i in 0..5 {
        for k in 0..2 {
            let p = waterline + fwd * (-2.0 + i as f32 * 0.6) + side * (-0.4 + k as f32 * 0.8) + Vec3::Y * (deck_y - waterline.y);
            kit::amphora(b, p, 1.0, 0);
        }
    }
    // Корпус — преграда (коллизия по ватерлинии до палубы).
    let c0 = waterline - fwd * (len / 2.0) - side * (beam / 2.0);
    let c1 = waterline + fwd * (len / 2.0) + side * (beam / 2.0);
    b.invisible.push(Aabb::new(Vec3::new(c0.x.min(c1.x), keel_y, c0.z.min(c1.z)), Vec3::new(c0.x.max(c1.x), deck_y, c0.z.max(c1.z))));
}

/// Рыбацкая лодка с вёслами.
pub fn boat(b: &mut Builder, waterline: Vec3, yaw: f32) {
    let len = 6.0;
    let beam = 1.7;
    let (s, c) = yaw.sin_cos();
    let fwd = Vec3::new(c, 0.0, s);
    let side = Vec3::new(-s, 0.0, c);
    let pt = |t: f32, u: f32, sd: f32| -> Vec3 {
        let x = (t - 0.5) * len;
        let fullness = (1.0 - (2.0 * t - 1.0).abs().powf(2.0)).max(0.0).sqrt();
        let half = beam / 2.0 * fullness * u.powf(0.5);
        let y = -0.45 + 0.85 * u + 0.25 * (2.0 * t - 1.0).powi(2) * u;
        waterline + fwd * x + side * (half * sd) + Vec3::Y * y
    };
    let m = b.mesh(Mat::Wood);
    for sd in [-1.0f32, 1.0] {
        for i in 0..10 {
            for j in 0..3 {
                let (t0, t1) = (i as f32 / 10.0, (i + 1) as f32 / 10.0);
                let (u0, u1) = (j as f32 / 3.0, (j + 1) as f32 / 3.0);
                let q = [pt(t0, u0, sd), pt(t1, u0, sd), pt(t1, u1, sd), pt(t0, u1, sd)];
                m.quad(q, side * sd);
                m.quad(q, -side * sd);
            }
        }
    }
    for k in 0..2 {
        let p = waterline + fwd * (-0.8 + k as f32 * 1.6) + Vec3::Y * 0.2;
        m.beam(p - side * 0.75, p + side * 0.75, 0.12, 0.03, fwd);
    }
    for sd in [-1.0f32, 1.0] {
        let a = waterline + side * sd * 0.7 + Vec3::Y * 0.4;
        m.beam(a - fwd * 0.4, a + side * sd * 1.6 - Vec3::Y * 0.6 + fwd * 0.6, 0.03, 0.03, Vec3::Y);
    }
    let _ = FRAC_PI_2;
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
    for (k, p) in [(peri.court.x0 + 3.0, peri.court.z0 + 4.0), (peri.court.x1 - 3.0, peri.court.z0 + 4.0), (peri.court.x0 + 3.0, peri.court.z1 - 4.0), (peri.court.x1 - 3.0, peri.court.z1 - 4.0)].into_iter().enumerate() {
        stall(b, Vec3::new(p.0, 0.0, p.1), k + 1, &mut rng);
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
