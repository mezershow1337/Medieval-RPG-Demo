//! Замок-заготовка из простых блоков (greybox). Позже блоки заменим моделями из Blender.
//!
//! Оси: X — восток, Z — юг (ворота на юге), Y — вверх. 1 единица = 1 метр.

use crate::collision::{Aabb, LevelColliders};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mat {
    Grass,
    Cobble,
    Stone,
    DarkStone,
    Floor,
    Wood,
    Banner,
    Gold,
}

#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub aabb: Aabb,
    pub mat: Mat,
    pub solid: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Torch {
    pub pos: Vec3,
}

pub struct Layout {
    pub blocks: Vec<Block>,
    pub torches: Vec<Torch>,
}

struct Builder {
    blocks: Vec<Block>,
    torches: Vec<Torch>,
}

impl Builder {
    fn solid(&mut self, min: [f32; 3], max: [f32; 3], mat: Mat) {
        self.blocks.push(Block { aabb: Aabb::new(Vec3::from(min), Vec3::from(max)), mat, solid: true });
    }
    fn decor(&mut self, min: [f32; 3], max: [f32; 3], mat: Mat) {
        self.blocks.push(Block { aabb: Aabb::new(Vec3::from(min), Vec3::from(max)), mat, solid: false });
    }
    fn torch(&mut self, x: f32, y: f32, z: f32) {
        self.torches.push(Torch { pos: Vec3::new(x, y, z) });
    }

    /// Зубцы вдоль линии от a до b (по X или по Z) на высоте y.
    fn merlons_x(&mut self, x0: f32, x1: f32, y: f32, z0: f32, z1: f32) {
        let mut x = x0;
        while x + 1.0 <= x1 {
            self.solid([x, y, z0], [x + 1.0, y + 1.4, z1], Mat::Stone);
            x += 2.0;
        }
    }
    fn merlons_z(&mut self, z0: f32, z1: f32, y: f32, x0: f32, x1: f32) {
        let mut z = z0;
        while z + 1.0 <= z1 {
            self.solid([x0, y, z], [x1, y + 1.4, z + 1.0], Mat::Stone);
            z += 2.0;
        }
    }

    /// Лестница из ступенек 0.4 м вдоль оси Z (от z_start в сторону dir).
    fn stairs_z(&mut self, x0: f32, x1: f32, z_start: f32, dir: f32, count: usize) {
        for i in 0..count {
            let top = 0.4 * (i as f32 + 1.0);
            let a = z_start + dir * 0.6 * i as f32;
            let b = z_start + dir * 0.6 * (i as f32 + 1.0);
            self.solid([x0, 0.0, a.min(b)], [x1, top, a.max(b)], Mat::Stone);
        }
    }

    fn crate_at(&mut self, x: f32, z: f32, size: f32) {
        self.solid([x - size / 2.0, 0.0, z - size / 2.0], [x + size / 2.0, size, z + size / 2.0], Mat::Wood);
    }
}

pub const WALL_HEIGHT: f32 = 10.0;

pub fn castle_layout() -> Layout {
    let mut b = Builder { blocks: Vec::new(), torches: Vec::new() };

    // --- Земля и двор ---
    b.solid([-150.0, -1.0, -150.0], [150.0, 0.0, 150.0], Mat::Grass);
    b.decor([-28.0, 0.0, -28.0], [28.0, 0.02, 28.0], Mat::Cobble);
    // Дорога к воротам.
    b.decor([-3.0, 0.0, 30.0], [3.0, 0.02, 70.0], Mat::Cobble);

    // --- Крепостные стены (60 x 60, толщина 2) ---
    let h = WALL_HEIGHT;
    b.solid([-30.0, 0.0, -30.0], [30.0, h, -28.0], Mat::Stone); // север
    b.solid([-30.0, 0.0, -28.0], [-28.0, h, 28.0], Mat::Stone); // запад
    b.solid([28.0, 0.0, -28.0], [30.0, h, 28.0], Mat::Stone); // восток
    b.solid([-30.0, 0.0, 28.0], [-3.0, h, 30.0], Mat::Stone); // юг, слева от ворот
    b.solid([3.0, 0.0, 28.0], [30.0, h, 30.0], Mat::Stone); // юг, справа от ворот
    b.solid([-3.0, 6.0, 28.0], [3.0, h, 30.0], Mat::Stone); // арка над воротами

    // Деревянные галереи с внутренней стороны стен (по ним ходят наверху).
    let gy = h - 0.3;
    b.solid([-26.0, gy, -28.0], [26.0, h, -26.5], Mat::Wood);
    b.solid([-28.0, gy, -26.0], [-26.5, h, 26.0], Mat::Wood);
    b.solid([26.5, gy, -26.0], [28.0, h, 26.0], Mat::Wood);
    b.solid([-26.0, gy, 26.5], [26.0, h, 28.0], Mat::Wood);

    // Зубцы по внешнему краю стен.
    b.merlons_x(-26.0, 26.0, h, -30.0, -29.4);
    b.merlons_x(-26.0, 26.0, h, 29.4, 30.0);
    b.merlons_z(-26.0, 26.0, h, -30.0, -29.4);
    b.merlons_z(-26.0, 26.0, h, 29.4, 30.0);

    // Лестницы на стены (западная и восточная), 25 ступенек по 0.4 м.
    b.stairs_z(-26.5, -24.0, 22.0, -1.0, 25);
    b.stairs_z(24.0, 26.5, 22.0, -1.0, 25);

    // --- Угловые башни ---
    for (cx, cz) in [(-30.0, -30.0), (30.0, -30.0), (-30.0, 30.0), (30.0, 30.0)] {
        b.solid([cx - 4.0, 0.0, cz - 4.0], [cx + 4.0, 16.0, cz + 4.0], Mat::DarkStone);
        b.merlons_x(cx - 4.0, cx + 4.0, 16.0, cz - 4.0, cz - 3.4);
        b.merlons_x(cx - 4.0, cx + 4.0, 16.0, cz + 3.4, cz + 4.0);
        b.merlons_z(cx - 3.0, cx + 3.0, 16.0, cx - 4.0, cx - 3.4);
        b.merlons_z(cx - 3.0, cx + 3.0, 16.0, cx + 3.4, cx + 4.0);
    }

    // --- Надвратные башни ---
    b.solid([-7.0, 0.0, 28.0], [-3.0, 13.0, 34.0], Mat::DarkStone);
    b.solid([3.0, 0.0, 28.0], [7.0, 13.0, 34.0], Mat::DarkStone);
    b.solid([-3.0, 7.0, 30.0], [3.0, 13.0, 34.0], Mat::DarkStone);
    b.decor([-3.0, 6.2, 33.6], [3.0, 7.0, 33.8], Mat::Wood); // поднятая решётка
    b.torch(-3.3, 3.2, 35.0);
    b.torch(3.3, 3.2, 35.0);
    b.torch(-2.7, 3.2, 27.6);
    b.torch(2.7, 3.2, 27.6);

    // --- Донжон (главная башня с тронным залом) ---
    let kh = 14.0;
    b.solid([-14.0, 0.0, -28.0], [14.0, kh, -26.5], Mat::DarkStone); // задняя стена
    b.solid([-14.0, 0.0, -26.5], [-12.5, kh, -10.5], Mat::DarkStone); // запад
    b.solid([12.5, 0.0, -26.5], [14.0, kh, -10.5], Mat::DarkStone); // восток
    b.solid([-14.0, 0.0, -10.5], [-2.5, kh, -9.0], Mat::DarkStone); // фасад слева
    b.solid([2.5, 0.0, -10.5], [14.0, kh, -9.0], Mat::DarkStone); // фасад справа
    b.solid([-2.5, 5.0, -10.5], [2.5, kh, -9.0], Mat::DarkStone); // над дверью
    b.solid([-14.0, kh, -28.0], [14.0, kh + 1.0, -9.0], Mat::DarkStone); // крыша
    b.merlons_x(-14.0, 14.0, kh + 1.0, -9.6, -9.0);
    b.merlons_x(-14.0, 14.0, kh + 1.0, -28.0, -27.4);
    b.decor([-12.5, 0.0, -26.5], [12.5, 0.03, -10.5], Mat::Floor);
    b.torch(-3.0, 3.5, -8.6);
    b.torch(3.0, 3.5, -8.6);

    // Колонны тронного зала.
    for z in [-13.5, -17.5, -21.5] {
        for x in [-5.0, 5.0] {
            b.solid([x - 0.5, 0.0, z - 0.5], [x + 0.5, kh, z + 0.5], Mat::Stone);
        }
    }
    // Помост и трон.
    b.solid([-4.0, 0.0, -26.5], [4.0, 0.45, -23.5], Mat::Stone);
    b.solid([-0.75, 0.45, -26.3], [0.75, 1.0, -25.2], Mat::Gold);
    b.solid([-0.75, 0.45, -26.5], [0.75, 3.0, -26.2], Mat::Gold);
    // Длинные столы.
    b.solid([-10.0, 0.0, -23.0], [-8.8, 0.9, -12.5], Mat::Wood);
    b.solid([8.8, 0.0, -23.0], [10.0, 0.9, -12.5], Mat::Wood);
    // Знамёна.
    for x in [-9.0, -3.0, 3.0, 9.0] {
        b.decor([x - 0.8, 4.0, -26.45], [x + 0.8, 9.0, -26.35], Mat::Banner);
    }
    for z in [-14.0, -19.0, -24.0] {
        b.torch(-12.1, 3.5, z);
        b.torch(12.1, 3.5, z);
    }

    // --- Двор ---
    // Колодец.
    b.solid([10.0, 0.0, 6.0], [13.0, 1.0, 6.4], Mat::Stone);
    b.solid([10.0, 0.0, 8.6], [13.0, 1.0, 9.0], Mat::Stone);
    b.solid([10.0, 0.0, 6.4], [10.4, 1.0, 8.6], Mat::Stone);
    b.solid([12.6, 0.0, 6.4], [13.0, 1.0, 8.6], Mat::Stone);
    b.solid([10.0, 0.0, 7.3], [10.3, 3.0, 7.7], Mat::Wood);
    b.solid([12.7, 0.0, 7.3], [13.0, 3.0, 7.7], Mat::Wood);
    b.solid([9.6, 3.0, 6.0], [13.4, 3.3, 9.0], Mat::Wood);
    b.torch(11.5, 2.6, 5.7);

    // Навес-конюшня у восточной стены.
    b.solid([18.0, 4.0, -6.0], [28.0, 4.3, 10.0], Mat::Wood);
    for z in [-6.0, -0.7, 4.6, 9.7] {
        b.solid([18.0, 0.0, z], [18.3, 4.0, z + 0.3], Mat::Wood);
    }
    for z in [-1.0, 4.0] {
        b.solid([22.0, 0.0, z], [28.0, 1.4, z + 0.2], Mat::Wood); // перегородки стойл
    }
    b.torch(18.6, 3.0, 2.0);

    // Тренировочная площадка с ограждением на западе.
    for i in 0..9 {
        let z = -4.0 + i as f32 * 1.5;
        b.solid([-20.0, 0.0, z], [-19.8, 1.1, z + 0.2], Mat::Wood);
        b.solid([-8.0, 0.0, z], [-7.8, 1.1, z + 0.2], Mat::Wood);
    }
    b.decor([-20.0, 0.9, -4.0], [-19.8, 1.0, 8.2], Mat::Wood);
    b.decor([-8.0, 0.9, -4.0], [-7.8, 1.0, 8.2], Mat::Wood);
    b.torch(-14.0, 2.5, -4.5);

    // Ящики и бочки.
    for (x, z, s) in [(-22.0, 18.0, 1.0), (-21.0, 18.2, 0.8), (-22.0, 19.1, 0.9), (20.0, 20.0, 1.2), (21.3, 20.5, 0.9), (-5.0, 14.0, 1.0), (6.5, -5.0, 1.0)] {
        b.crate_at(x, z, s);
    }

    Layout { blocks: b.blocks, torches: b.torches }
}

pub fn level_colliders() -> LevelColliders {
    LevelColliders(castle_layout().blocks.iter().filter(|b| b.solid).map(|b| b.aabb).collect())
}

/// Точки появления игроков во дворе возле ворот.
pub fn spawn_point(index: usize) -> Vec3 {
    let col = (index % 4) as f32;
    let row = (index / 4 % 4) as f32;
    Vec3::new(-3.0 + col * 2.0, 0.1, 20.0 + row * 1.5)
}

fn material_for(mat: Mat) -> StandardMaterial {
    let (color, roughness, emissive) = match mat {
        Mat::Grass => (Color::srgb(0.25, 0.36, 0.18), 0.95, LinearRgba::BLACK),
        Mat::Cobble => (Color::srgb(0.38, 0.36, 0.33), 0.9, LinearRgba::BLACK),
        Mat::Stone => (Color::srgb(0.55, 0.53, 0.5), 0.85, LinearRgba::BLACK),
        Mat::DarkStone => (Color::srgb(0.4, 0.39, 0.38), 0.85, LinearRgba::BLACK),
        Mat::Floor => (Color::srgb(0.32, 0.28, 0.25), 0.7, LinearRgba::BLACK),
        Mat::Wood => (Color::srgb(0.42, 0.29, 0.17), 0.8, LinearRgba::BLACK),
        Mat::Banner => (Color::srgb(0.55, 0.06, 0.06), 0.9, LinearRgba::BLACK),
        Mat::Gold => (Color::srgb(0.8, 0.62, 0.2), 0.35, LinearRgba::rgb(0.08, 0.05, 0.0)),
    };
    StandardMaterial { base_color: color, perceptual_roughness: roughness, emissive, ..default() }
}

/// Клиент: создаёт меши, материалы и свет для уровня.
pub fn spawn_level(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let layout = castle_layout();
    let mut mats = std::collections::HashMap::new();
    for block in &layout.blocks {
        let handle = mats.entry(block.mat).or_insert_with(|| materials.add(material_for(block.mat))).clone();
        let size = block.aabb.max - block.aabb.min;
        let center = (block.aabb.max + block.aabb.min) / 2.0;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(handle),
            Transform::from_translation(center),
        ));
    }

    let torch_mesh = meshes.add(Cuboid::new(0.15, 0.4, 0.15));
    let flame = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.6, 0.2),
        emissive: LinearRgba::rgb(12.0, 5.0, 1.0),
        ..default()
    });
    for torch in &layout.torches {
        commands.spawn((Mesh3d(torch_mesh.clone()), MeshMaterial3d(flame.clone()), Transform::from_translation(torch.pos)));
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.65, 0.35),
                intensity: 60_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(torch.pos + Vec3::Y * 0.3),
        ));
    }

    // Закатное солнце.
    commands.spawn((
        DirectionalLight { color: Color::srgb(1.0, 0.82, 0.6), illuminance: 6_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(60.0, 40.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
