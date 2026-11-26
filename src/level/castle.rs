//! Готический замок (первый уровень). Включается переменной окружения `MEDIEVAL_LEVEL=castle`.

use super::*;
use geom::{WallFrame, arch_height};

/// Высота крепостной стены.
pub const WALL_HEIGHT: f32 = 12.0;

// Границы основных частей замка.
const CURTAIN_IN: f32 = 45.0; // внутренняя грань восточной/западной стены
const NORTH_IN: f32 = -67.0; // внутренняя грань северной стены
const SOUTH_IN: f32 = 43.0; // внутренняя грань южной стены
const INNER_WALL: (f32, f32) = (-8.0, -5.0); // внутренняя стена (z)
const HALL_FRONT: f32 = -20.0; // фасад собора (наружная грань)
const HALL_BACK: f32 = -65.0; // северная стена собора (наружная грань)
const HALL_HALF: f32 = 13.0; // полуширина собора по наружным стенам
const NAVE_HALF: f32 = 6.5; // полуширина нефа по наружным граням его стен
/// Линии колонн нефа (z).
const COLUMN_Z: [f32; 6] = [-27.0, -33.0, -39.0, -45.0, -51.0, -57.0];
/// Двери из собора в боковые проходы, в клуатр и казармы.
const SIDE_DOOR_Z: f32 = -42.0;

pub fn castle_layout() -> Layout {
    let mut b = Builder::new();

    ground_and_curtain(&mut b);
    gatehouse(&mut b);
    courtyard(&mut b);
    inner_wall(&mut b);
    cathedral(&mut b);
    cloister(&mut b);
    barracks(&mut b);
    surroundings(&mut b);

    b.finish()
}

fn ground_and_curtain(b: &mut Builder) {
    let h = WALL_HEIGHT;
    b.solid([-900.0, -1.0, -900.0], [900.0, 0.0, 900.0], Mat::Grass);
    // Двор и площадь мощёные.
    b.decor([-CURTAIN_IN, 0.0, INNER_WALL.1], [CURTAIN_IN, 0.02, SOUTH_IN], Mat::Cobble);
    b.decor([-CURTAIN_IN, 0.0, -17.0], [CURTAIN_IN, 0.02, INNER_WALL.0], Mat::Cobble);
    b.decor([-20.0, 0.0, -67.0], [20.0, 0.02, -17.0], Mat::Cobble);
    b.decor([-CURTAIN_IN, 0.0, NORTH_IN], [CURTAIN_IN, 0.02, -63.0], Mat::Cobble);

    // Крепостные стены (толщина 3, по верху можно ходить).
    b.solid([-48.0, 0.0, -70.0], [48.0, h, NORTH_IN], Mat::Stone);
    b.solid([-48.0, 0.0, NORTH_IN], [-CURTAIN_IN, h, SOUTH_IN], Mat::Stone);
    b.solid([CURTAIN_IN, 0.0, NORTH_IN], [48.0, h, SOUTH_IN], Mat::Stone);
    b.solid([-48.0, 0.0, SOUTH_IN], [-8.0, h, 46.0], Mat::Stone);
    b.solid([8.0, 0.0, SOUTH_IN], [48.0, h, 46.0], Mat::Stone);

    // Зубцы по внешнему краю и низкий парапет по внутреннему.
    b.merlons_x(-41.0, 41.0, h, -70.0, -69.4);
    b.merlons_x(-41.0, -10.0, h, 45.4, 46.0);
    b.merlons_x(10.0, 41.0, h, 45.4, 46.0);
    for (x0, x1) in [(-48.0, -47.4), (47.4, 48.0)] {
        b.merlons_z(-62.0, -18.0, h, x0, x1);
        b.merlons_z(-6.0, 38.0, h, x0, x1);
    }
    // Карниз-пояс под зубцами снаружи.
    let t = b.mesh(Mat::Trim);
    t.cuboid(Vec3::new(-48.3, h - 0.6, -70.3), Vec3::new(48.3, h - 0.2, -69.9));
    t.cuboid(Vec3::new(-48.3, h - 0.6, 45.9), Vec3::new(48.3, h - 0.2, 46.3));
    t.cuboid(Vec3::new(-48.3, h - 0.6, -70.3), Vec3::new(-47.9, h - 0.2, 46.3));
    t.cuboid(Vec3::new(47.9, h - 0.6, -70.3), Vec3::new(48.3, h - 0.2, 46.3));

    // Контрфорсы снаружи стен.
    let fx = WallFrame { along_x: false };
    let fz = WallFrame { along_x: true };
    for z in [-50.0, -30.0, 8.0, 26.0] {
        b.buttress(fx, z, -48.0, -1.0, 2.2, 10.0, 1.6);
        b.buttress(fx, z, 48.0, 1.0, 2.2, 10.0, 1.6);
    }
    for x in [-32.0, -16.0, 16.0, 32.0] {
        b.buttress(fz, x, -70.0, -1.0, 2.2, 10.0, 1.6);
    }
    for x in [-30.0, -18.0, 18.0, 30.0] {
        b.buttress(fz, x, 46.0, 1.0, 2.2, 10.0, 1.6);
    }

    // Угловые и средние башни.
    for (cx, cz) in [(-46.5, -68.5), (46.5, -68.5), (-46.5, 44.5), (46.5, 44.5)] {
        b.tower(cx, cz, 5.5, 20.0, 15.0);
    }
    for cx in [-46.5, 46.5] {
        b.tower(cx, -12.0, 4.5, 17.0, 12.0);
    }

    // Лестницы на стены у южной стены (30 ступенек по 0.4 м = 12 м).
    b.stairs_x(-10.0, -1.0, 40.5, SOUTH_IN, 30);
    b.stairs_x(10.0, 1.0, 40.5, SOUTH_IN, 30);
    // Факелы на стенах двора.
    for z in [-0.0, 16.0, 30.0] {
        b.torch(-CURTAIN_IN + 0.35, 3.2, z, Vec3::X);
        b.torch(CURTAIN_IN - 0.35, 3.2, z, Vec3::NEG_X);
    }
}

fn gatehouse(b: &mut Builder) {
    let (z0, z1) = (40.0, 50.0);
    let top = 16.0;
    let (w, spring, rise) = (5.0, 3.4, 3.0);
    let apex = spring + arch_height(0.0, w, rise);
    b.solid([-8.0, 0.0, z0], [-w / 2.0, top, z1], Mat::DarkStone);
    b.solid([w / 2.0, 0.0, z0], [8.0, top, z1], Mat::DarkStone);
    b.solid([-w / 2.0, apex, z0], [w / 2.0, top, z1], Mat::DarkStone);
    let f = WallFrame { along_x: true };
    for (d0, d1) in [(z1 - 0.6, z1), (z0, z0 + 0.6)] {
        let mut infill = std::mem::take(b.mesh(Mat::DarkStone));
        let mut trim = std::mem::take(b.mesh(Mat::Trim));
        geom::arch_infill(&mut infill, &mut trim, f, 0.0, w, spring, rise, apex, d0, d1);
        *b.mesh(Mat::DarkStone) = infill;
        *b.mesh(Mat::Trim) = trim;
    }
    // Свод проезда.
    {
        let mut web = std::mem::take(b.mesh(Mat::DarkStone));
        let mut ribs = std::mem::take(b.mesh(Mat::Trim));
        for k in 0..2 {
            let za = z0 + 0.6 + k as f32 * 4.4;
            geom::rib_vault(&mut web, &mut ribs, Vec2::new(-w / 2.0, za), Vec2::new(w / 2.0, za + 4.4), spring, apex - spring);
        }
        *b.mesh(Mat::DarkStone) = web;
        *b.mesh(Mat::Trim) = ribs;
    }
    // Поднятая решётка.
    let m = b.mesh(Mat::Iron);
    for i in 0..9 {
        let x = -2.2 + i as f32 * 0.55;
        m.cuboid(Vec3::new(x - 0.05, apex - 1.3, 47.9), Vec3::new(x + 0.05, apex, 48.1));
    }
    for y in [apex - 1.1, apex - 0.5] {
        m.cuboid(Vec3::new(-2.4, y, 47.92), Vec3::new(2.4, y + 0.08, 48.08));
    }
    // Зубцы и турели.
    b.merlons_x(-8.0, 8.0, top, z1 - 0.6, z1);
    b.merlons_x(-8.0, 8.0, top, z0, z0 + 0.6);
    for x in [-7.5, 7.5] {
        b.turret(x, z1 + 0.6, 2.1, top + 3.0, 9.0);
    }
    // Пояса и окно над воротами.
    b.mesh(Mat::Trim).cuboid(Vec3::new(-8.0, 9.0, z1 - 0.05), Vec3::new(8.0, 9.35, z1 + 0.2));
    let (pts, uvs) = geom::lancet_outline(f, 0.0, z1 + 0.03, 1.6, 10.5, 12.8, 1.0);
    b.glass.push(GlassPane { kind: GlassKind::Lancet, pts, uvs, normal: Vec3::Z });
    b.banner(f, -4.6, z1 + 0.05, 1.0, 3.0, 8.2, 1.6);
    b.banner(f, 4.6, z1 + 0.05, 1.0, 3.0, 8.2, 1.6);
    b.torch(-3.0, 3.0, z1 + 0.35, Vec3::Z);
    b.torch(3.0, 3.0, z1 + 0.35, Vec3::Z);
    b.torch(-2.15, 2.8, 45.0, Vec3::X);
    b.torch(2.15, 2.8, 45.0, Vec3::NEG_X);
    b.torch(-3.0, 3.0, z0 - 0.35, Vec3::NEG_Z);
    b.torch(3.0, 3.0, z0 - 0.35, Vec3::NEG_Z);
}

fn courtyard(b: &mut Builder) {
    // Фонтан в центре двора.
    let (fx, fz) = (0.0, 14.0);
    b.solid([fx - 1.8, 0.0, fz - 1.8], [fx + 1.8, 0.8, fz + 1.8], Mat::Trim);
    let m = b.mesh(Mat::Trim);
    m.prism(Vec3::new(fx, 0.0, fz), 2.8, 0.8, 8);
    m.prism(Vec3::new(fx, 0.8, fz), 2.95, 0.12, 8);
    m.prism(Vec3::new(fx, 0.8, fz), 0.45, 2.4, 8);
    m.prism(Vec3::new(fx, 2.2, fz), 1.1, 0.25, 8);
    m.prism(Vec3::new(fx, 2.45, fz), 0.3, 1.2, 8);
    b.pinnacle(Vec3::new(fx, 3.65, fz), 0.35, 2.2);
    b.mesh(Mat::Water).prism(Vec3::new(fx, 0.62, fz), 2.55, 0.1, 8);
    b.solid([fx - 0.29, 0.8, fz - 0.29], [fx + 0.29, 3.6, fz + 0.29], Mat::Trim);

    // Колодец.
    let (wx, wz) = (-20.0, 30.0);
    b.solid([wx - 1.5, 0.0, wz - 1.5], [wx + 1.5, 1.0, wz + 1.5], Mat::Stone);
    b.mesh(Mat::Water).cuboid(Vec3::new(wx - 1.1, 0.95, wz - 1.1), Vec3::new(wx + 1.1, 1.01, wz + 1.1));
    b.solid([wx - 1.5, 1.0, wz - 0.2], [wx - 1.2, 3.0, wz + 0.2], Mat::Wood);
    b.solid([wx + 1.2, 1.0, wz - 0.2], [wx + 1.5, 3.0, wz + 0.2], Mat::Wood);
    b.roof(Mat::Roof, |m| m.gable_roof(Vec2::new(wx - 1.9, wz - 1.4), Vec2::new(wx + 1.9, wz + 1.4), 2.9, 4.0, true));
    b.mesh(Mat::Wood).beam(Vec3::new(wx - 1.3, 2.4, wz), Vec3::new(wx + 1.3, 2.4, wz), 0.1, 0.1, Vec3::Y);

    // Фонарные столбы вдоль главной дороги.
    for z in [32.0, 22.0, 4.0] {
        b.lamp_post(-5.0, z);
        b.lamp_post(5.0, z);
    }

    // Конюшня у восточной стены.
    let (sx0, sx1, sz0, sz1) = (37.0, CURTAIN_IN, 6.0, 26.0);
    for z in [sz0, 11.0, 16.0, 21.0, sz1 - 0.3] {
        b.solid([sx0, 0.0, z], [sx0 + 0.3, 4.0, z + 0.3], Mat::Wood);
    }
    b.solid([sx0, 4.0, sz0], [sx1, 4.3, sz1], Mat::Wood);
    b.roof(Mat::Roof, |m| m.lean_roof([Vec3::new(sx1, 6.0, sz0 - 0.4), Vec3::new(sx1, 6.0, sz1 + 0.4)], [Vec3::new(sx0 - 0.8, 4.3, sz0 - 0.4), Vec3::new(sx0 - 0.8, 4.3, sz1 + 0.4)]));
    for z in [11.0, 16.0, 21.0] {
        b.solid([sx0 + 3.0, 0.0, z], [sx1, 1.5, z + 0.2], Mat::Wood);
    }
    for (x, z) in [(43.0, 8.0), (43.0, 13.5), (43.5, 18.5)] {
        b.solid([x - 0.6, 0.0, z - 0.6], [x + 0.6, 1.0, z + 0.6], Mat::Wood); // сено/кормушки
    }
    b.torch(sx0 + 0.6, 3.0, 16.0, Vec3::X);

    // Кузница у юго-восточного угла.
    let (kx0, kx1, kz0, kz1) = (30.0, 40.0, 29.0, 38.0);
    for (x, z) in [(kx0, kz0), (kx1 - 0.3, kz0), (kx0, kz1 - 0.3), (kx1 - 0.3, kz1 - 0.3)] {
        b.solid([x, 0.0, z], [x + 0.3, 4.0, z + 0.3], Mat::Wood);
    }
    b.solid([kx0, 4.0, kz0], [kx1, 4.3, kz1], Mat::Wood);
    b.roof(Mat::Roof, |m| m.gable_roof(Vec2::new(kx0 - 0.5, kz0 - 0.5), Vec2::new(kx1 + 0.5, kz1 + 0.5), 4.3, 6.5, true));
    b.solid([36.0, 0.0, 31.0], [39.5, 1.1, 35.0], Mat::Stone); // горн
    b.mesh(Mat::Coals).cuboid(Vec3::new(36.4, 1.1, 31.4), Vec3::new(39.1, 1.18, 34.6));
    b.solid([38.2, 1.1, 34.2], [39.4, 6.8, 35.0], Mat::Stone); // труба
    b.lamp(Vec3::new(37.7, 1.8, 33.0), Color::srgb(1.0, 0.45, 0.15), 160_000.0, 14.0);
    b.solid([33.0, 0.0, 32.6], [33.6, 0.6, 33.4], Mat::Wood); // колода
    b.solid([32.8, 0.6, 32.7], [33.8, 0.9, 33.3], Mat::Iron); // наковальня
    b.forges.push(super::ForgeSpec { name: "Кузница замка", kind: crate::smithing_net::StationKind::Forge, fire: Vec3::new(37.7, 1.15, 33.0), anvil: Vec3::new(33.3, 0.0, 34.0), trough: Vec3::new(31.0, 1.05, 36.5), grind: Vec3::new(32.0, 0.75, 31.5) });
    b.barrel_at(31.0, 36.5);
    b.barrel_at(31.0, 30.5);

    // Тренировочная площадка на западе: ограда и манекены.
    let (tx0, tx1, tz0, tz1) = (-40.0, -24.0, 4.0, 22.0);
    let mut z = tz0;
    while z <= tz1 {
        b.solid([tx0, 0.0, z], [tx0 + 0.2, 1.1, z + 0.2], Mat::Wood);
        b.solid([tx1, 0.0, z], [tx1 + 0.2, 1.1, z + 0.2], Mat::Wood);
        z += 2.0;
    }
    b.decor([tx0, 0.85, tz0], [tx0 + 0.2, 0.95, tz1 + 0.2], Mat::Wood);
    b.decor([tx1, 0.85, tz0], [tx1 + 0.2, 0.95, tz1 + 0.2], Mat::Wood);
    b.decor([tx0, 0.0, tz0], [tx1, 0.025, tz1], Mat::Grass);
    for (x, z) in [(-36.0, 8.0), (-32.0, 8.0), (-28.0, 8.0), (-34.0, 15.0), (-30.0, 15.0)] {
        b.solid([x - 0.12, 0.0, z - 0.12], [x + 0.12, 1.9, z + 0.12], Mat::Wood);
        b.decor([x - 0.6, 1.35, z - 0.08], [x + 0.6, 1.5, z + 0.08], Mat::Wood);
        b.decor([x - 0.25, 1.55, z - 0.25], [x + 0.25, 1.95, z + 0.25], Mat::Banner);
    }
    b.lamp_post(-32.0, tz0 - 1.0);

    // Ящики, бочки, телега.
    for (x, z, s) in [(-22.0, 38.0, 1.0), (-20.9, 38.2, 0.8), (-22.0, 36.9, 0.9), (20.0, 20.0, 1.2), (21.3, 20.5, 0.9), (-40.0, 30.0, 1.1), (-41.2, 30.4, 0.9)] {
        b.crate_at(x, z, s);
    }
    for (x, z) in [(22.0, 18.6), (23.0, 19.4), (-40.0, 32.0), (40.0, -2.0), (41.0, -2.8)] {
        b.barrel_at(x, z);
    }
    // Ящики у дороги.
    for (x, z, sz) in [(14.0, 24.0, 1.0), (15.2, 24.2, 0.8), (14.5, 25.2, 0.9)] {
        b.crate_at(x, z, sz);
    }
}

fn inner_wall(b: &mut Builder) {
    let (d0, d1) = INNER_WALL;
    let f = WallFrame { along_x: true };
    let h = 10.0;
    let openings = [Opening::door(0.0, 6.0, 3.6, 3.4), Opening::door(-24.0, 1.8, 2.3, 1.2), Opening::door(24.0, 1.8, 2.3, 1.2)];
    b.wall(f, -CURTAIN_IN, CURTAIN_IN, 0.0, h, d0, d1, &openings, Mat::Stone);
    b.merlons_x(-44.0, -9.0, h, d1 - 0.6, d1);
    b.merlons_x(9.0, 44.0, h, d1 - 0.6, d1);
    b.mesh(Mat::Trim).cuboid(Vec3::new(-CURTAIN_IN, h - 0.5, d1 - 0.05), Vec3::new(CURTAIN_IN, h - 0.15, d1 + 0.25));
    // Надвратные башни.
    for x in [-5.5, 5.5] {
        b.solid([x - 2.5, 0.0, d0 - 1.0], [x + 2.5, 14.0, d1 + 1.0], Mat::DarkStone);
        b.merlons_x(x - 2.5, x + 2.5, 14.0, d1 + 0.4, d1 + 1.0);
        b.roof(Mat::Roof, |m| m.spire(Vec3::new(x, 14.0, (d0 + d1) / 2.0), 2.6, 9.0, 4));
        b.mesh(Mat::Gold).prism(Vec3::new(x, 22.8, (d0 + d1) / 2.0), 0.1, 1.0, 4);
        let (pts, uvs) = geom::lancet_outline(f, x, d1 + 1.03, 0.9, 7.0, 8.8, 0.6);
        b.glass.push(GlassPane { kind: GlassKind::Lancet, pts, uvs, normal: Vec3::Z });
    }
    let fz = WallFrame { along_x: true };
    for x in [-36.0, -14.0, 14.0, 36.0] {
        b.buttress(fz, x, d1, 1.0, 1.6, 8.0, 1.2);
    }
    b.banner(f, -5.5, d1 + 1.05, 1.0, 3.5, 10.0, 2.0);
    b.banner(f, 5.5, d1 + 1.05, 1.0, 3.5, 10.0, 2.0);
    for x in [-3.6, 3.6] {
        b.torch(x, 3.2, d1 + 0.35, Vec3::Z);
        b.torch(x, 3.2, d0 - 0.35, Vec3::NEG_Z);
    }
    for x in [-24.0, 24.0] {
        b.torch(x + 1.5, 2.6, d1 + 0.35, Vec3::Z);
    }
}

fn cathedral(b: &mut Builder) {
    let fz = WallFrame { along_x: true };
    let fx = WallFrame { along_x: false };
    let (front_in, back_in) = (HALL_FRONT - 1.0, HALL_BACK + 1.0); // внутренние грани торцов
    let aisle_h = 9.0;
    let nave_h = 16.0;
    let ridge = 24.0;

    // Пол.
    b.decor([-HALL_HALF + 1.0, 0.0, back_in], [HALL_HALF - 1.0, 0.03, front_in], Mat::Floor);

    // --- Фасад (юг) ---
    let portal = Opening::door(0.0, 5.0, 4.2, 4.0);
    let side_portals = [Opening::door(-9.5, 2.6, 3.0, 1.8), Opening::door(9.5, 2.6, 3.0, 1.8)];
    b.wall(fz, -HALL_HALF, -NAVE_HALF, 0.0, aisle_h, front_in, HALL_FRONT, &side_portals, Mat::DarkStone);
    b.wall(fz, NAVE_HALF, HALL_HALF, 0.0, aisle_h, front_in, HALL_FRONT, &side_portals, Mat::DarkStone);
    b.wall(fz, -NAVE_HALF, NAVE_HALF, 0.0, nave_h, front_in, HALL_FRONT, &[portal], Mat::DarkStone);
    b.gable(fz, -NAVE_HALF - 0.5, NAVE_HALF + 0.5, nave_h, ridge, front_in, HALL_FRONT, Mat::DarkStone);
    // Ступени перед порталом.
    b.solid([-4.0, 0.0, HALL_FRONT], [4.0, 0.2, HALL_FRONT + 1.2], Mat::Trim);
    // Портал: углублённые рёбра (архивольты) и тимпан.
    let pa = portal.apex();
    for k in 1..=3 {
        let grow = k as f32 * 0.45;
        let path: Vec<Vec3> = (0..=20)
            .map(|i| {
                let s = -(portal.width / 2.0 + grow) + (portal.width + 2.0 * grow) * i as f32 / 20.0;
                Vec3::new(s, portal.spring + arch_height(s, portal.width + 2.0 * grow, portal.rise + grow), HALL_FRONT + 0.15)
            })
            .collect();
        b.mesh(Mat::Trim).beam_path(&path, 0.16, 0.14, Vec3::Z);
        for s in [-(portal.width / 2.0 + grow), portal.width / 2.0 + grow] {
            b.mesh(Mat::Trim).prism(Vec3::new(s, 0.0, HALL_FRONT + 0.15), 0.17, portal.spring, 8);
        }
    }
    // Щипец-вимперг над порталом.
    b.mesh(Mat::Trim).beam(Vec3::new(-4.2, pa - 0.6, HALL_FRONT + 0.2), Vec3::new(0.0, pa + 2.6, HALL_FRONT + 0.2), 0.18, 0.2, Vec3::Z);
    b.mesh(Mat::Trim).beam(Vec3::new(0.0, pa + 2.6, HALL_FRONT + 0.2), Vec3::new(4.2, pa - 0.6, HALL_FRONT + 0.2), 0.18, 0.2, Vec3::Z);
    b.pinnacle(Vec3::new(0.0, pa + 2.6, HALL_FRONT + 0.2), 0.25, 1.6);
    // Роза над порталом — с обеих сторон стены.
    let rose_c = Vec3::new(0.0, 12.6, 0.0);
    let rose_r = 3.0;
    for (z, n) in [(HALL_FRONT + 0.04, Vec3::Z), (front_in - 0.04, Vec3::NEG_Z)] {
        let pts: Vec<Vec3> = (0..32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                Vec3::new(rose_c.x + a.cos() * rose_r, rose_c.y + a.sin() * rose_r, z)
            })
            .collect();
        let uvs = (0..32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                Vec2::new(0.5 + a.cos() * 0.5, 0.5 - a.sin() * 0.5)
            })
            .collect();
        b.glass.push(GlassPane { kind: GlassKind::Rose, pts, uvs, normal: n });
        let ring: Vec<Vec3> = (0..=32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                Vec3::new(rose_c.x + a.cos() * (rose_r + 0.15), rose_c.y + a.sin() * (rose_r + 0.15), z + n.z * 0.1)
            })
            .collect();
        b.mesh(Mat::Trim).beam_path(&ring, 0.25, 0.22, Vec3::Z);
    }
    b.lamp(Vec3::new(0.0, 12.6, front_in - 1.5), Color::srgb(0.55, 0.45, 1.0), 25_000.0, 10.0);
    // Галерея королей — ряд ниш над порталом.
    for i in 0..7 {
        let x = -5.4 + i as f32 * 1.8;
        b.mesh(Mat::Trim).prism(Vec3::new(x, 9.0, HALL_FRONT + 0.15), 0.12, 0.9, 6);
    }
    b.mesh(Mat::Trim).cuboid(Vec3::new(-6.5, 8.8, HALL_FRONT), Vec3::new(6.5, 9.0, HALL_FRONT + 0.35));
    b.mesh(Mat::Trim).cuboid(Vec3::new(-6.5, 9.9, HALL_FRONT), Vec3::new(6.5, 10.1, HALL_FRONT + 0.35));

    // Башни фасада.
    for sx in [-1.0f32, 1.0] {
        let cx = sx * 15.5;
        let (x0, x1) = (cx - 2.5, cx + 2.5);
        let (z0, z1) = (HALL_FRONT - 5.0, HALL_FRONT + 4.0);
        b.solid([x0, 0.0, z0], [x1, 28.0, z1], Mat::DarkStone);
        for y in [9.0, 16.0, 24.0] {
            b.mesh(Mat::Trim).cuboid(Vec3::new(x0 - 0.15, y - 0.35, z0 - 0.15), Vec3::new(x1 + 0.15, y, z1 + 0.15));
        }
        // Угловые контрфорсы башни.
        for (bx, bz) in [(x0 + 0.7, z1), (x1 - 0.7, z1)] {
            b.solid([bx - 0.7, 0.0, bz - 0.7], [bx + 0.7, 22.0, bz + 1.2], Mat::DarkStone);
            b.pinnacle(Vec3::new(bx, 22.0, bz + 0.25), 0.55, 4.0);
        }
        // Окна-ланцеты на фасаде башни.
        for (y, h) in [(11.0, 3.2), (18.0, 4.0)] {
            for dx in [-0.8, 0.8] {
                let (pts, uvs) = geom::lancet_outline(fz, cx + dx, z1 + 0.03, 1.0, y, y + h, 0.7);
                b.glass.push(GlassPane { kind: GlassKind::Lancet, pts, uvs, normal: Vec3::Z });
            }
        }
        // Звонница и высокий шпиль.
        b.mesh(Mat::DarkStone).prism(Vec3::new(cx, 28.0, (z0 + z1) / 2.0), 2.4, 3.5, 8);
        b.mesh(Mat::Trim).prism(Vec3::new(cx, 31.5, (z0 + z1) / 2.0), 2.7, 0.4, 8);
        b.roof(Mat::Roof, |m| m.spire(Vec3::new(cx, 31.9, (z0 + z1) / 2.0), 2.6, 18.0, 8));
        b.mesh(Mat::Gold).prism(Vec3::new(cx, 49.6, (z0 + z1) / 2.0), 0.15, 2.0, 4);
        b.mesh(Mat::Gold).cuboid(Vec3::new(cx - 0.7, 50.8, (z0 + z1) / 2.0 - 0.08), Vec3::new(cx + 0.7, 51.0, (z0 + z1) / 2.0 + 0.08));
        for (px, pz) in [(x0, z0), (x1, z0), (x0, z1), (x1, z1)] {
            b.pinnacle(Vec3::new(px, 28.0, pz), 0.5, 4.5);
        }
        b.torch(sx * 11.6, 3.2, HALL_FRONT + 0.35, Vec3::Z);
    }

    // --- Северная стена с тремя высокими окнами ---
    let apse_windows = [Opening::window(-3.2, 2.0, 3.5, 11.0, 2.4), Opening::window(0.0, 2.2, 3.0, 11.8, 2.6), Opening::window(3.2, 2.0, 3.5, 11.0, 2.4)];
    let aisle_end_windows = [Opening::window(-9.5, 2.0, 2.0, 5.2, 1.5), Opening::window(9.5, 2.0, 2.0, 5.2, 1.5)];
    b.wall(fz, -HALL_HALF, -NAVE_HALF, 0.0, aisle_h, HALL_BACK, back_in, &aisle_end_windows, Mat::DarkStone);
    b.wall(fz, NAVE_HALF, HALL_HALF, 0.0, aisle_h, HALL_BACK, back_in, &aisle_end_windows, Mat::DarkStone);
    b.wall(fz, -NAVE_HALF, NAVE_HALF, 0.0, nave_h, HALL_BACK, back_in, &apse_windows, Mat::DarkStone);
    b.gable(fz, -NAVE_HALF - 0.5, NAVE_HALF + 0.5, nave_h, ridge, HALL_BACK, back_in, Mat::DarkStone);

    // --- Боковые стены нефов: окна между колоннами, по двери в каждом ---
    let bay_centers: Vec<f32> = std::iter::once(front_in).chain(COLUMN_Z).chain(std::iter::once(back_in)).collect::<Vec<_>>().windows(2).map(|w| (w[0] + w[1]) / 2.0).collect();
    let mut side_openings: Vec<Opening> = bay_centers
        .iter()
        .filter(|z| (**z - SIDE_DOOR_Z).abs() > 2.0)
        .map(|&z| Opening::window(z, 2.4, 1.8, 5.0, 1.8))
        .collect();
    side_openings.push(Opening::door(SIDE_DOOR_Z, 2.4, 2.6, 1.4));
    for sx in [-1.0f32, 1.0] {
        let (d0, d1) = if sx < 0.0 { (-HALL_HALF, -HALL_HALF + 1.0) } else { (HALL_HALF - 1.0, HALL_HALF) };
        b.wall(fx, back_in, front_in, 0.0, aisle_h, d0, d1, &side_openings, Mat::DarkStone);
        b.mesh(Mat::Trim).cuboid(Vec3::new(d0.min(d1) - 0.2, aisle_h - 0.4, back_in), Vec3::new(d0.max(d1) + 0.2, aisle_h, front_in));
        // Контрфорсы с аркбутанами.
        let face = if sx < 0.0 { -HALL_HALF } else { HALL_HALF };
        for &z in COLUMN_Z.iter() {
            b.buttress(fx, z, face, sx, 2.6, 12.5, 1.2);
            geom::flying_buttress(b.mesh(Mat::Trim), Vec3::new(sx * (HALL_HALF + 1.0), 13.0, z), Vec3::new(sx * (NAVE_HALF + 0.1), 15.0, z), 0.4);
        }
        // Факелы внутри боковых нефов.
        for &z in COLUMN_Z.iter() {
            b.torch(sx * (HALL_HALF - 1.35), 3.3, z, Vec3::new(-sx, 0.0, 0.0));
        }
    }

    // --- Аркада нефа и верхний ряд окон ---
    let mut edges = vec![front_in];
    for &z in COLUMN_Z.iter() {
        edges.push(z + 0.7);
        edges.push(z - 0.7);
    }
    edges.push(back_in);
    let arcade: Vec<Opening> = edges
        .chunks(2)
        .map(|c| {
            let w = (c[0] - c[1]).abs();
            Opening::door((c[0] + c[1]) / 2.0, w, 5.6, (w / 2.0).max(3.0))
        })
        .collect();
    let clerestory: Vec<Opening> = bay_centers.iter().map(|&z| Opening::window(z, 2.4, 10.6, 13.2, 1.6)).collect();
    for sx in [-1.0f32, 1.0] {
        let (d0, d1) = if sx < 0.0 { (-NAVE_HALF, -NAVE_HALF + 1.0) } else { (NAVE_HALF - 1.0, NAVE_HALF) };
        b.wall(fx, back_in, front_in, 0.0, 10.0, d0, d1, &arcade, Mat::DarkStone);
        b.wall(fx, back_in, front_in, 10.0, nave_h, d0, d1, &clerestory, Mat::DarkStone);
        // Колонны аркады (декор поверх опор).
        for &z in COLUMN_Z.iter() {
            b.column(sx * (NAVE_HALF - 0.5), z, 0.8, 5.6);
            // Служебные колонки до пят свода нефа.
            b.mesh(Mat::Trim).prism(Vec3::new(sx * (NAVE_HALF - 1.12), 5.6, z), 0.16, 4.4, 6);
        }
        b.mesh(Mat::Trim).cuboid(Vec3::new(sx * (NAVE_HALF - 1.0) - 0.25, 9.6, back_in), Vec3::new(sx * (NAVE_HALF - 1.0) + 0.25, 9.85, front_in));
        // Крыши боковых нефов.
        let (xi, xo) = (sx * NAVE_HALF, sx * (HALL_HALF + 0.4));
        b.roof(Mat::Roof, |m| m.lean_roof([Vec3::new(xi, 10.3, back_in - 1.4), Vec3::new(xi, 10.3, front_in + 1.4)], [Vec3::new(xo, aisle_h, back_in - 1.4), Vec3::new(xo, aisle_h, front_in + 1.4)]));
        // Знамёна на опорах аркады.
        for &z in COLUMN_Z.iter().skip(1).step_by(2) {
            b.banner(fx, z, sx * (NAVE_HALF - 1.2), -sx, 3.0, 5.3, 1.0);
        }
    }

    // --- Своды ---
    {
        let mut web = std::mem::take(b.mesh(Mat::Stone));
        let mut ribs = std::mem::take(b.mesh(Mat::Trim));
        let lines: Vec<f32> = std::iter::once(front_in).chain(COLUMN_Z).chain(std::iter::once(back_in)).collect();
        for w in lines.windows(2) {
            let (za, zb) = (w[1], w[0]);
            geom::rib_vault(&mut web, &mut ribs, Vec2::new(-NAVE_HALF + 1.0, za), Vec2::new(NAVE_HALF - 1.0, zb), 10.0, 6.0);
            for sx in [-1.0f32, 1.0] {
                let (xa, xb) = if sx < 0.0 { (-HALL_HALF + 1.0, -NAVE_HALF) } else { (NAVE_HALF, HALL_HALF - 1.0) };
                geom::rib_vault(&mut web, &mut ribs, Vec2::new(xa, za), Vec2::new(xb, zb), 5.6, 3.0);
            }
        }
        *b.mesh(Mat::Stone) = web;
        *b.mesh(Mat::Trim) = ribs;
    }
    // Крыша нефа.
    b.roof(Mat::Roof, |m| m.gable_roof(Vec2::new(-NAVE_HALF - 0.6, HALL_BACK - 0.3), Vec2::new(NAVE_HALF + 0.6, HALL_FRONT + 0.3), nave_h, ridge, false));
    // Шпиль-флеш над средокрестием.
    b.roof(Mat::Roof, |m| m.spire(Vec3::new(0.0, ridge - 1.0, -42.0), 1.6, 12.0, 8));
    b.mesh(Mat::Trim).prism(Vec3::new(0.0, ridge - 3.0, -42.0), 1.5, 2.0, 8);
    b.mesh(Mat::Gold).prism(Vec3::new(0.0, ridge + 10.8, -42.0), 0.1, 1.4, 4);
    // Карнизы нефа.
    for sx in [-1.0f32, 1.0] {
        b.mesh(Mat::Trim).cuboid(Vec3::new(sx * NAVE_HALF - 0.35, nave_h - 0.4, HALL_BACK), Vec3::new(sx * NAVE_HALF + 0.35, nave_h, HALL_FRONT));
    }

    // --- Хоры и трон ---
    let (cz0, cz1) = (back_in, COLUMN_Z[5] - 0.7);
    b.solid([-NAVE_HALF + 1.0, 0.0, cz0], [NAVE_HALF - 1.0, 0.4, cz1], Mat::Trim);
    b.solid([-3.0, 0.4, cz0], [3.0, 0.8, cz0 + 3.0], Mat::Trim);
    b.decor([-NAVE_HALF + 1.0, 0.4, cz0], [NAVE_HALF - 1.0, 0.42, cz1], Mat::Banner);
    // Трон.
    let tz = cz0 + 1.2;
    b.solid([-0.8, 0.8, tz - 0.6], [0.8, 1.35, tz + 0.6], Mat::Gold);
    b.solid([-0.8, 0.8, tz - 0.75], [0.8, 4.0, tz - 0.55], Mat::Gold);
    b.mesh(Mat::Gold).spire(Vec3::new(0.0, 4.0, tz - 0.65), 0.55, 1.3, 4);
    for x in [-0.8, 0.8] {
        b.mesh(Mat::Gold).prism(Vec3::new(x, 0.8, tz - 0.65), 0.12, 3.6, 6);
        b.mesh(Mat::Gold).spire(Vec3::new(x, 4.4, tz - 0.65), 0.15, 0.6, 4);
    }
    b.decor([-0.7, 1.35, tz - 0.5], [0.7, 1.42, tz + 0.5], Mat::Banner);
    // Большое знамя за троном.
    b.banner(fz, 0.0, back_in + 0.05, 1.0, 1.2, 3.2, 2.6);
    // Длинные столы в боковых нефах.
    for x in [-9.5, 9.5] {
        b.solid([x - 0.6, 0.75, -38.0], [x + 0.6, 0.9, -24.0], Mat::Wood);
        for z in [-37.5, -31.0, -24.5] {
            b.solid([x - 0.5, 0.0, z - 0.1], [x + 0.5, 0.75, z + 0.1], Mat::Wood);
        }
        for dx in [-1.0, 1.0] {
            b.solid([x + dx - 0.2, 0.0, -38.0], [x + dx + 0.2, 0.45, -24.0], Mat::Wood);
        }
    }
    // Священник у хоров, монах у колонн.
    {
        use super::city::interior::extras;
        let mut rng = super::city::kit::Rng::new(808);
        let r = super::city::kit::Rect::new(-NAVE_HALF + 1.0, cz1 + 0.5, NAVE_HALF - 1.0, cz1 + 6.0);
        extras(b, r, crate::items::LootTableId::Church, &["Ризничий ларь"], &[(Role::Priest, Vec3::new(0.0, 0.0, cz1 + 1.5), Vec3::new(0.0, 0.0, 0.0)), (Role::Monk, Vec3::new(4.0, 0.0, cz1 + 4.0), Vec3::new(0.0, 0.0, cz1))], &mut rng);
    }
    // Люстры над нефом.
    for z in [-30.0, -42.0, -54.0] {
        b.chandelier(Vec3::new(0.0, 6.5, z), 15.6);
    }
    for z in [-24.0, -48.0] {
        b.lamp(Vec3::new(0.0, 4.0, z), Color::srgb(1.0, 0.7, 0.4), 40_000.0, 14.0);
    }
    // Канделябры у хоров.
    for x in [-4.0, 4.0] {
        b.solid([x - 0.2, 0.4, cz1 - 0.6], [x + 0.2, 2.2, cz1 - 0.2], Mat::Iron);
        b.torch(x, 2.45, cz1 - 0.4, Vec3::ZERO);
    }
}

fn cloister(b: &mut Builder) {
    let fz = WallFrame { along_x: true };
    let fx = WallFrame { along_x: false };
    let (x0, x1, z0, z1) = (-CURTAIN_IN, -20.0, -63.0, -17.0);
    let h = 7.0;
    // Наружные стены.
    b.wall(fz, x0, x1, 0.0, h, z1 - 1.0, z1, &[Opening::door(-31.0, 2.4, 2.6, 1.4)], Mat::Stone);
    b.wall(fz, x0, x1, 0.0, h, z0 - 1.0, z0, &[], Mat::Stone);
    b.wall(fx, z0 - 1.0, z1, 0.0, h, x1 - 1.0, x1, &[Opening::door(SIDE_DOOR_Z, 2.4, 2.6, 1.4)], Mat::Stone);
    // Аркада галереи вокруг сада.
    let walk = 3.5;
    let (gx0, gx1, gz0, gz1) = (x0 + walk, x1 - 1.0 - walk, z0 + walk, z1 - 1.0 - walk);
    let ah = 4.6;
    let arches = |a: f32, bnd: f32| -> Vec<Opening> {
        let n = ((bnd - a) / 3.0).floor().max(1.0) as usize;
        let step = (bnd - a) / n as f32;
        (0..n).map(|i| Opening::door(a + step * (i as f32 + 0.5), step - 0.6, 2.4, 1.3)).collect()
    };
    b.wall(fz, gx0, gx1, 0.0, ah, gz1 - 0.6, gz1, &arches(gx0, gx1), Mat::Trim);
    b.wall(fz, gx0, gx1, 0.0, ah, gz0, gz0 + 0.6, &arches(gx0, gx1), Mat::Trim);
    b.wall(fx, gz0, gz1, 0.0, ah, gx0, gx0 + 0.6, &arches(gz0, gz1), Mat::Trim);
    b.wall(fx, gz0, gz1, 0.0, ah, gx1 - 0.6, gx1, &arches(gz0, gz1), Mat::Trim);
    // Перекрытия галерей и крыши.
    let slab = |b: &mut Builder, min: [f32; 3], max: [f32; 3]| b.solid(min, max, Mat::Wood);
    slab(b, [x0, ah, z0], [x1 - 1.0, ah + 0.35, gz0 + 0.6]);
    slab(b, [x0, ah, gz1 - 0.6], [x1 - 1.0, ah + 0.35, z1 - 1.0]);
    slab(b, [x0, ah, gz0], [gx0 + 0.6, ah + 0.35, gz1]);
    slab(b, [gx1 - 0.6, ah, gz0], [x1 - 1.0, ah + 0.35, gz1]);
    b.roof(Mat::Roof, |r| {
        r.lean_roof([Vec3::new(x0, h - 0.3, z0), Vec3::new(x1 - 1.0, h - 0.3, z0)], [Vec3::new(x0, ah + 0.35, gz0 + 0.9), Vec3::new(x1 - 1.0, ah + 0.35, gz0 + 0.9)]);
        r.lean_roof([Vec3::new(x0, h - 0.3, z1 - 1.0), Vec3::new(x1 - 1.0, h - 0.3, z1 - 1.0)], [Vec3::new(x0, ah + 0.35, gz1 - 0.9), Vec3::new(x1 - 1.0, ah + 0.35, gz1 - 0.9)]);
        r.lean_roof([Vec3::new(x0, h - 0.3, gz0), Vec3::new(x0, h - 0.3, gz1)], [Vec3::new(gx0 + 0.9, ah + 0.35, gz0), Vec3::new(gx0 + 0.9, ah + 0.35, gz1)]);
        r.lean_roof([Vec3::new(x1 - 1.0, h - 0.3, gz0), Vec3::new(x1 - 1.0, h - 0.3, gz1)], [Vec3::new(gx1 - 0.9, ah + 0.35, gz0), Vec3::new(gx1 - 0.9, ah + 0.35, gz1)]);
    });
    // Балки потолка галерей.
    let mut z = z0 + 1.5;
    while z < z1 - 1.0 {
        b.mesh(Mat::Wood).cuboid(Vec3::new(x0, ah - 0.25, z - 0.12), Vec3::new(gx0, ah, z + 0.12));
        b.mesh(Mat::Wood).cuboid(Vec3::new(gx1, ah - 0.25, z - 0.12), Vec3::new(x1 - 1.0, ah, z + 0.12));
        z += 3.0;
    }
    let mut x = x0 + 1.5;
    while x < x1 - 1.0 {
        b.mesh(Mat::Wood).cuboid(Vec3::new(x - 0.12, ah - 0.25, z0), Vec3::new(x + 0.12, ah, gz0));
        b.mesh(Mat::Wood).cuboid(Vec3::new(x - 0.12, ah - 0.25, gz1), Vec3::new(x + 0.12, ah, z1 - 1.0));
        x += 3.0;
    }
    // Пол галерей и сад.
    b.decor([x0, 0.0, z0], [x1 - 1.0, 0.03, z1 - 1.0], Mat::Floor);
    b.decor([gx0 + 0.6, 0.0, gz0 + 0.6], [gx1 - 0.6, 0.05, gz1 - 0.6], Mat::Grass);
    // Колодец-фонтан в центре сада и кипарисы.
    let (cx, cz) = ((gx0 + gx1) / 2.0, (gz0 + gz1) / 2.0);
    b.solid([cx - 1.35, 0.0, cz - 1.35], [cx + 1.35, 0.7, cz + 1.35], Mat::Trim);
    b.mesh(Mat::Trim).prism(Vec3::new(cx, 0.0, cz), 2.1, 0.7, 8);
    b.mesh(Mat::Water).prism(Vec3::new(cx, 0.55, cz), 1.9, 0.1, 8);
    b.solid([cx - 0.2, 0.7, cz - 0.2], [cx + 0.2, 2.6, cz + 0.2], Mat::Trim);
    b.mesh(Mat::Trim).prism(Vec3::new(cx, 0.7, cz), 0.3, 1.9, 8);
    b.pinnacle(Vec3::new(cx, 2.6, cz), 0.3, 1.8);
    for (tx, tz) in [(gx0 + 3.0, gz0 + 4.0), (gx1 - 3.0, gz0 + 4.0), (gx0 + 3.0, gz1 - 4.0), (gx1 - 3.0, gz1 - 4.0), (cx, gz0 + 9.0), (cx, gz1 - 9.0)] {
        cypress(b, tx, tz, 5.5);
    }
    // Факелы в галереях.
    for (tx, tz, out) in [(x0 + 0.35, -30.0, Vec3::X), (x0 + 0.35, -50.0, Vec3::X), (x1 - 1.35, -28.0, Vec3::NEG_X), (x1 - 1.35, -55.0, Vec3::NEG_X), (-38.0, z0 + 0.35, Vec3::Z), (-25.0, z1 - 1.35, Vec3::NEG_Z)] {
        b.torch(tx, 3.0, tz, out);
    }
}

fn cypress(b: &mut Builder, x: f32, z: f32, h: f32) {
    b.solid([x - 0.2, 0.0, z - 0.2], [x + 0.2, h * 0.9, z + 0.2], Mat::Wood);
    let m = b.mesh(Mat::Leaves);
    m.prism(Vec3::new(x, 0.8, z), 0.7, h * 0.35, 8);
    m.spire(Vec3::new(x, 0.8 + h * 0.35, z), 0.75, h * 0.65, 8);
    m.spire(Vec3::new(x, 0.6, z), 0.8, 0.25, 8);
}

fn barracks(b: &mut Builder) {
    let fz = WallFrame { along_x: true };
    let fx = WallFrame { along_x: false };
    let (x0, x1, z0, z1) = (20.0, CURTAIN_IN, -63.0, -17.0);
    let h = 7.0;
    let ceil = 5.0;
    let (cx0, cx1) = (30.0, 33.0); // центральный коридор
    let (cz0, cz1) = (SIDE_DOOR_Z - 1.5, SIDE_DOOR_Z + 1.5); // поперечный коридор
    b.wall(fz, x0, x1, 0.0, h, z1 - 1.0, z1, &[Opening::door((cx0 + cx1) / 2.0, 2.4, 2.6, 1.4)], Mat::Stone);
    b.wall(fz, x0, x1, 0.0, h, z0 - 1.0, z0, &[], Mat::Stone);
    b.wall(fx, z0 - 1.0, z1, 0.0, h, x0, x0 + 1.0, &[Opening::door(SIDE_DOOR_Z, 2.4, 2.6, 1.4)], Mat::Stone);
    // Окна на южном фасаде.
    for x in [22.0, 26.5, 37.0, 41.5] {
        let (pts, uvs) = geom::lancet_outline(fz, x, z1 + 0.03, 1.1, 2.4, 4.2, 0.8);
        b.glass.push(GlassPane { kind: GlassKind::Lancet, pts, uvs, normal: Vec3::Z });
    }
    // Потолок и крыша.
    b.solid([x0 + 1.0, ceil, z0], [x1, ceil + 0.4, z1 - 1.0], Mat::Wood);
    b.solid([x0, ceil + 0.4, z0 - 1.0], [x1, h, z1], Mat::Stone);
    b.roof(Mat::Roof, |m| m.gable_roof(Vec2::new(x0 - 0.5, z0 - 1.5), Vec2::new(x1, z1 + 0.5), h, 12.5, false));
    // Внутренние стены коридоров (толщина 0.4) с дверями в комнаты.
    let wall_h = ceil;
    let door = |s: f32| Opening::door(s, 1.6, 2.2, 1.0);
    let south_split = -29.0;
    let north_split = -53.5;
    let south_rooms = [door((cz1 + 0.4 + south_split) / 2.0), door((south_split + z1 - 1.0) / 2.0)];
    let north_rooms = [door((z0 + north_split) / 2.0), door((north_split + cz0 - 0.4) / 2.0)];
    for (d0, d1) in [(cx0 - 0.4, cx0), (cx1, cx1 + 0.4)] {
        b.wall(fx, cz1 + 0.4, z1 - 1.0, 0.0, wall_h, d0, d1, &south_rooms, Mat::Stone);
        b.wall(fx, z0, cz0 - 0.4, 0.0, wall_h, d0, d1, &north_rooms, Mat::Stone);
    }
    for (d0, d1) in [(cz0 - 0.4, cz0), (cz1, cz1 + 0.4)] {
        b.wall(fz, x0 + 1.0, cx0 - 0.4, 0.0, wall_h, d0, d1, &[], Mat::Stone);
        b.wall(fz, cx1 + 0.4, x1, 0.0, wall_h, d0, d1, &[], Mat::Stone);
    }
    // Перегородки между комнатами.
    for (xa, xb) in [(x0 + 1.0, cx0 - 0.4), (cx1 + 0.4, x1)] {
        b.solid([xa, 0.0, south_split - 0.2], [xb, wall_h, south_split + 0.2], Mat::Stone);
        b.solid([xa, 0.0, north_split - 0.2], [xb, wall_h, north_split + 0.2], Mat::Stone);
    }
    b.decor([x0 + 1.0, 0.0, z0], [x1, 0.03, z1 - 1.0], Mat::Floor);
    // Балки потолка.
    let mut z = z0 + 2.0;
    while z < z1 - 1.0 {
        b.mesh(Mat::Wood).cuboid(Vec3::new(x0 + 1.0, ceil - 0.3, z - 0.15), Vec3::new(x1, ceil, z + 0.15));
        z += 4.0;
    }
    // Факелы в коридорах.
    for z in [-22.0, -32.0, -50.0, -60.0] {
        b.torch(cx0 + 0.35, 2.8, z, Vec3::X);
        b.torch(cx1 - 0.35, 2.8, z + 3.0, Vec3::NEG_X);
    }
    b.torch(24.0, 2.8, cz0 + 0.35, Vec3::Z);
    b.torch(39.0, 2.8, cz1 - 0.35, Vec3::NEG_Z);

    // Комнаты: спальни с койками (юг), арсенал (северо-восток), трапезная (северо-запад), склад (юго-восток).
    for z in [-20.5, -23.5, -26.5] {
        b.solid([x0 + 1.2, 0.0, z - 0.5], [x0 + 3.2, 0.5, z + 0.5], Mat::Wood);
        b.decor([x0 + 1.25, 0.5, z - 0.45], [x0 + 2.2, 0.6, z + 0.45], Mat::Banner);
        b.solid([cx0 - 2.4, 0.0, z - 0.5], [cx0 - 0.4, 0.5, z + 0.5], Mat::Wood);
    }
    for z in [-31.5, -34.5, -37.5] {
        b.solid([x0 + 1.2, 0.0, z - 0.5], [x0 + 3.2, 0.5, z + 0.5], Mat::Wood);
        b.decor([x0 + 1.25, 0.5, z - 0.45], [x0 + 2.2, 0.6, z + 0.45], Mat::Banner);
    }
    b.lamp(Vec3::new(24.0, 3.5, -23.0), Color::srgb(1.0, 0.65, 0.35), 25_000.0, 9.0);
    b.lamp(Vec3::new(24.0, 3.5, -35.0), Color::srgb(1.0, 0.65, 0.35), 25_000.0, 9.0);
    // Склад.
    for (x, z, s) in [(36.0, -20.0, 1.2), (37.3, -20.2, 0.9), (36.2, -21.3, 1.0), (43.0, -27.0, 1.2), (42.8, -33.0, 1.0), (43.0, -38.5, 1.1)] {
        b.crate_at(x, z, s);
    }
    for (x, z) in [(39.0, -20.0), (40.0, -20.0), (43.5, -24.5), (35.0, -38.5)] {
        b.barrel_at(x, z);
    }
    b.lamp(Vec3::new(39.0, 3.5, -30.0), Color::srgb(1.0, 0.65, 0.35), 20_000.0, 9.0);
    // Трапезная.
    b.solid([21.0, 0.75, -61.0], [27.0, 0.9, -59.8], Mat::Wood);
    b.solid([21.0, 0.75, -50.0], [27.0, 0.9, -48.8], Mat::Wood);
    for z in [-61.6, -59.2, -50.6, -48.2] {
        b.solid([21.0, 0.0, z - 0.2], [27.0, 0.45, z + 0.2], Mat::Wood);
    }
    for z in [-60.4, -49.4] {
        for x in [21.5, 26.5] {
            b.solid([x - 0.1, 0.0, z - 0.4], [x + 0.1, 0.75, z + 0.4], Mat::Wood);
        }
    }
    b.lamp(Vec3::new(24.0, 3.8, -55.0), Color::srgb(1.0, 0.65, 0.35), 30_000.0, 10.0);
    b.lamp(Vec3::new(24.0, 3.8, -48.0), Color::srgb(1.0, 0.65, 0.35), 25_000.0, 9.0);
    // Арсенал: стойки с оружием.
    for z in [-46.0, -50.0, -57.0, -61.0] {
        b.solid([x1 - 0.6, 0.0, z - 1.2], [x1, 2.2, z + 1.2], Mat::Wood);
        let m = b.mesh(Mat::Iron);
        for k in 0..5 {
            let zz = z - 1.0 + k as f32 * 0.5;
            m.cuboid(Vec3::new(x1 - 0.8, 0.3, zz - 0.03), Vec3::new(x1 - 0.7, 2.0, zz + 0.03));
        }
    }
    b.solid([37.0, 0.0, -58.0], [40.0, 0.9, -56.5], Mat::Wood);
    b.lamp(Vec3::new(39.0, 3.5, -52.0), Color::srgb(1.0, 0.65, 0.35), 25_000.0, 10.0);
    b.lamp(Vec3::new(39.0, 3.5, -60.0), Color::srgb(1.0, 0.65, 0.35), 20_000.0, 9.0);

    // Обитатели и сундуки: стража в спальнях и арсенале, кашевар в трапезной, кладовщик.
    use super::city::interior::{self, extras};
    use crate::items::LootTableId as L;
    let mut rng = super::city::kit::Rng::new(606);
    let rect = |xa: f32, za: f32, xb: f32, zb: f32| super::city::kit::Rect::new(xa, za, xb, zb);
    let p = |x: f32, z: f32| Vec3::new(x, 0.0, z);
    extras(b, rect(x0 + 1.0, south_split + 0.2, cx0 - 0.4, z1 - 1.0), L::Barracks, &["Сундук стражника", "Сундук стражника"], &[(Role::Soldier, p(26.0, -22.0), p(24.0, -23.0))], &mut rng);
    extras(b, rect(x0 + 1.0, cz1 + 0.4, cx0 - 0.4, south_split - 0.2), L::Barracks, &["Сундук стражника"], &[(Role::Soldier, p(26.0, -34.0), p(22.0, -34.0))], &mut rng);
    extras(b, rect(cx1 + 0.4, cz1 + 0.4, x1, z1 - 1.0), L::Storehouse, &["Ларь", "Корзина"], &[(Role::Servant, p(39.0, -27.0), p(43.0, -27.0))], &mut rng);
    extras(b, rect(x0 + 1.0, z0, cx0 - 0.4, cz0 - 0.4), L::Kitchen, &["Корзина с припасами", "Шкаф с посудой"], &[(Role::Cook, p(28.0, -55.0), p(24.0, -55.0))], &mut rng);
    extras(b, rect(cx1 + 0.4, z0, x1, cz0 - 0.4), L::Barracks, &["Оружейный сундук"], &[(Role::Officer, p(38.5, -54.0), p(42.0, -54.0))], &mut rng);
    for (z, light) in [(-24.0, true), (-36.0, false)] {
        interior::lampstand(b, p(29.0, z), light);
    }
}

fn surroundings(b: &mut Builder) {
    // Дорога к воротам и кипарисы вдоль неё.
    b.decor([-3.0, 0.0, 50.0], [3.0, 0.02, 130.0], Mat::Cobble);
    for z in [62.0, 76.0, 90.0, 104.0] {
        cypress(b, -7.0, z, 7.0);
        cypress(b, 7.0, z + 4.0, 6.5);
    }
    // Камни и холмики вокруг стен.
    for (x, z, s) in [(-60.0, 20.0, 1.6), (-58.0, -40.0, 2.2), (62.0, -10.0, 1.8), (58.0, 30.0, 1.2), (30.0, 62.0, 2.0), (-35.0, 70.0, 1.5), (-20.0, -84.0, 2.4), (25.0, -82.0, 1.7)] {
        b.solid([x - s, 0.0, z - s * 0.8], [x + s, s * 0.7, z + s * 0.8], Mat::Stone);
    }
    // Лес кипарисов на горизонте.
    let mut seed = 17u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    for _ in 0..90 {
        let a = rnd() * std::f32::consts::TAU;
        let r = 75.0 + rnd() * 70.0;
        let (x, z) = (a.cos() * r, a.sin() * r - 10.0);
        if x.abs() < 14.0 && z > 40.0 {
            continue;
        }
        let h = 5.0 + rnd() * 6.0;
        cypress(b, x, z, h);
    }
}


/// Точки появления игроков во дворе возле ворот.
pub fn spawn_point(index: usize) -> Vec3 {
    let col = (index % 4) as f32;
    let row = (index / 4 % 4) as f32;
    Vec3::new(-3.0 + col * 2.0, 0.1, 30.0 + row * 1.5)
}

/// Подсказки, где ставить врагов: открытые места (≥ 3 м свободного пола вокруг),
/// не ближе 12 м к точкам появления игроков.
/// Порядок: две точки во дворе у тренировочной площадки, одна у конюшни,
/// две в нефе собора и тяжёлый рыцарь перед хорами.
pub fn npc_spawn_hints() -> Vec<Vec3> {
    vec![
        Vec3::new(-18.0, 0.0, 10.0),
        Vec3::new(-14.0, 0.0, 18.0),
        Vec3::new(30.0, 0.0, 14.0),
        Vec3::new(-3.0, 0.0, -48.0),
        Vec3::new(3.0, 0.0, -48.0),
        Vec3::new(0.0, 0.0, -55.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::Body;

    fn free(level: &LevelColliders, p: Vec3) -> bool {
        level.body_is_free(&Body::STANDING, p)
    }

    #[test]
    fn spawn_points_are_free() {
        let level = colliders(castle_layout());
        level.set_all_doors(true);
        for i in 0..16 {
            assert!(free(&level, spawn_point(i)), "точка появления {i} в стене: {}", spawn_point(i));
        }
        for p in npc_spawn_hints() {
            assert!(free(&level, p + Vec3::Y * 0.1), "точка врага в стене: {p}");
            for s in spawn_points_sample() {
                assert!(p.distance(s) >= 12.0, "враг {p} слишком близко к появлению игрока {s}");
            }
        }
    }

    fn spawn_points_sample() -> Vec<Vec3> {
        (0..16).map(spawn_point).collect()
    }

    /// Проходы, которые должны оставаться открытыми: ворота, калитки, двери, коридоры.
    #[test]
    fn key_passages_are_open() {
        let level = colliders(castle_layout());
        level.set_all_doors(true);
        let checks = [
            ("проезд надвратной башни", Vec3::new(0.0, 0.05, 45.0)),
            ("внутренние ворота", Vec3::new(0.0, 0.05, -6.5)),
            ("западная калитка", Vec3::new(-24.0, 0.05, -6.5)),
            ("восточная калитка", Vec3::new(24.0, 0.05, -6.5)),
            ("портал собора", Vec3::new(0.0, 0.25, -20.5)),
            ("боковой портал собора", Vec3::new(-9.5, 0.05, -20.5)),
            ("неф", Vec3::new(0.0, 0.05, -40.0)),
            ("боковой неф", Vec3::new(-9.0, 0.05, -45.0)),
            ("аркада нефа", Vec3::new(-6.0, 0.05, -36.0)),
            ("западная дверь собора", Vec3::new(-12.5, 0.05, SIDE_DOOR_Z)),
            ("восточная дверь собора", Vec3::new(12.5, 0.05, SIDE_DOOR_Z)),
            ("западный проход", Vec3::new(-17.5, 0.05, -42.0)),
            ("устье западного прохода", Vec3::new(-19.0, 0.05, -17.0)),
            ("коридор за собором", Vec3::new(0.0, 0.05, -66.0)),
            ("дверь клуатра с площади", Vec3::new(-31.0, 0.05, -17.5)),
            ("дверь клуатра из прохода", Vec3::new(-20.5, 0.05, SIDE_DOOR_Z)),
            ("галерея клуатра", Vec3::new(-43.0, 0.05, -40.0)),
            ("сад клуатра", Vec3::new(-30.0, 0.05, -30.0)),
            ("дверь казарм", Vec3::new(31.5, 0.05, -17.5)),
            ("западная дверь казарм", Vec3::new(20.5, 0.05, SIDE_DOOR_Z)),
            ("коридор казарм", Vec3::new(31.5, 0.05, -50.0)),
            ("поперечный коридор казарм", Vec3::new(40.0, 0.05, SIDE_DOOR_Z)),
            ("верх стены", Vec3::new(-30.0, 12.05, 44.5)),
        ];
        for (name, p) in checks {
            assert!(free(&level, p), "закрыт проход: {name} ({p})");
        }
    }

    /// Можно дойти пешком от ворот до хоров и подняться по лестнице на стену.
    #[test]
    fn walk_gate_to_choir_and_up_the_stairs() {
        let level = colliders(castle_layout());
        level.set_all_doors(true);
        let walk = |path: &[Vec3]| {
            let mut feet = path[0];
            for target in &path[1..] {
                for _ in 0..4000 {
                    let to = *target - feet;
                    let flat = Vec3::new(to.x, 0.0, to.z);
                    if flat.length() < 0.3 {
                        break;
                    }
                    let step = flat.normalize() * 0.1;
                    level.move_body(&Body::STANDING, &mut feet, step + Vec3::new(0.0, -0.05, 0.0), true);
                }
                let to = *target - feet;
                assert!(Vec2::new(to.x, to.z).length() < 0.5, "застрял по пути к {target}: стою в {feet}");
            }
            feet
        };
        walk(&[
            Vec3::new(0.0, 0.0, 70.0),
            Vec3::new(0.0, 0.0, 30.0),
            Vec3::new(7.0, 0.0, 22.0),
            Vec3::new(7.0, 0.0, 2.0),
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::new(0.0, 0.0, -30.0),
            Vec3::new(-9.0, 0.0, -42.0),
            Vec3::new(-17.0, 0.0, -42.0),
            Vec3::new(-30.0, 0.0, -42.0),
        ]);
        let top = walk(&[Vec3::new(-9.0, 0.0, 41.7), Vec3::new(-27.6, 0.0, 41.7), Vec3::new(-27.6, 0.0, 44.5)]);
        assert!(top.y > 11.9, "не поднялся на стену: {top}");
    }
}
