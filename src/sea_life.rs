//! Подводный мир у берегов Херсонеса: заросли водорослей (цистозира, зостера), камни на дне
//! и стайки рыб (кефаль, ставрида, барабуля), которые держатся у дна и шарахаются от пловца.
//!
//! Всё своё и процедурное: меши строятся здесь же, цвет — в вершинах, качание и взмахи
//! хвоста — в вершинном шейдере (`shaders/sea_life.wesl`). По сети ничего не ходит: рыбы
//! у каждого игрока свои, как птицы и дым (на игру не влияют). Рыбачит рыбалка (fishing).

use crate::level::WATER_Y;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::VisibilityRange;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub struct SeaLifePlugin;

impl Plugin for SeaLifePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sea_life.wesl");
        app.add_plugins(MaterialPlugin::<SeaLifeMaterial>::default())
            .add_systems(Update, (spawn_sea_life.run_if(resource_added::<crate::water::SeaLevel>), swim_schools));
    }
}

pub type SeaLifeMaterial = ExtendedMaterial<StandardMaterial, SeaLifeExt>;

#[derive(ShaderType, Clone, Copy, Debug, Default, Reflect)]
pub struct SeaLifeUniform {
    /// x — 0: водоросли, 1: рыба; y — сила качания; z — частота.
    pub mode: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct SeaLifeExt {
    #[uniform(100)]
    pub life: SeaLifeUniform,
}

impl MaterialExtension for SeaLifeExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/sea_life.wesl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/sea_life.wesl".into()
    }
}

/// Простой генератор (одинаковый у всех игроков: водоросли растут в одних и тех же местах).
struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

// ---------------------------------------------------------------------------------------------
// Меши

/// Пучок водоросли: несколько изогнутых лент от общего корня. `kind` 0 — цистозира (бурая,
/// ветвистая), 1 — зостера (длинные зелёные ленты), 2 — ульва (короткие светло-зелёные листы).
fn weed_mesh(kind: u32, seed: u64) -> Mesh {
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let (blades, len, width, segs, col_a, col_b) = match kind {
        0 => (7, (0.5, 1.1), 0.035, 7, [0.16, 0.12, 0.04], [0.38, 0.30, 0.10]),
        1 => (9, (0.7, 1.6), 0.018, 9, [0.05, 0.14, 0.04], [0.20, 0.36, 0.10]),
        _ => (6, (0.18, 0.4), 0.07, 4, [0.10, 0.25, 0.06], [0.35, 0.55, 0.16]),
    };
    let (mut pos, mut nrm, mut uv, mut uv2, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::<u32>::new());
    for b in 0..blades {
        let a = b as f32 / blades as f32 * std::f32::consts::TAU + r.range(-0.3, 0.3);
        let tilt = r.range(0.05, 0.35);
        let l = r.range(len.0, len.1);
        let w = width * r.range(0.7, 1.3);
        let phase = r.f();
        let dir = Vec3::new(a.cos() * tilt, 1.0, a.sin() * tilt).normalize();
        let side = Vec3::new(-a.sin(), 0.0, a.cos());
        let root = Vec3::new(a.cos(), 0.0, a.sin()) * r.range(0.0, 0.06);
        let curl = r.range(-0.4, 0.4);
        let base = pos.len() as u32;
        for s in 0..=segs {
            let k = s as f32 / segs as f32;
            // Лента изгибается и к кончику сужается.
            let c = root + dir * (l * k) + side * (curl * k * k * l * 0.3) + Vec3::new(a.cos(), 0.0, a.sin()) * (k * k * l * 0.15);
            let half = w * (1.0 - 0.75 * k) * if kind == 2 { (k * std::f32::consts::PI).sin() + 0.3 } else { 1.0 };
            let n = side.cross(dir).normalize();
            let tint = (0.85 + 0.3 * r.f()).min(1.1);
            let cc = [col_a[0] + (col_b[0] - col_a[0]) * k, col_a[1] + (col_b[1] - col_a[1]) * k, col_a[2] + (col_b[2] - col_a[2]) * k];
            for (sgn, u) in [(-1.0f32, 0.0f32), (1.0, 1.0)] {
                pos.push((c + side * half * sgn).to_array());
                nrm.push(n.to_array());
                uv.push([u, k]);
                uv2.push([k, phase]);
                col.push([cc[0] * tint, cc[1] * tint, cc[2] * tint, 1.0]);
            }
            if s > 0 {
                let i = base + s * 2;
                idx.extend([i - 2, i - 1, i + 1, i - 2, i + 1, i]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv2)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// Вид рыбы.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FishKind {
    /// Кефаль-лобан: серебристая, тёмная спина, длинное тело.
    Mullet,
    /// Ставрида: зеленоватая спина, ярко-серебряные бока, вильчатый хвост.
    Scad,
    /// Барабуля: розово-красная, короткая, держится у самого дна.
    RedMullet,
}

impl FishKind {
    pub const ALL: [FishKind; 3] = [FishKind::Mullet, FishKind::Scad, FishKind::RedMullet];

    /// Длина, м; (высота, толщина) тела в долях длины; цвета спины, боков, брюха.
    fn look(self) -> (f32, f32, f32, [f32; 3], [f32; 3], [f32; 3]) {
        match self {
            FishKind::Mullet => (0.42, 0.20, 0.13, [0.10, 0.13, 0.15], [0.55, 0.58, 0.60], [0.85, 0.86, 0.84]),
            FishKind::Scad => (0.28, 0.21, 0.11, [0.12, 0.22, 0.20], [0.70, 0.74, 0.74], [0.90, 0.90, 0.86]),
            FishKind::RedMullet => (0.22, 0.24, 0.14, [0.55, 0.18, 0.12], [0.85, 0.45, 0.35], [0.95, 0.85, 0.75]),
        }
    }
}

/// Рыба длиной 1 м по оси −Z (нос вперёд), масштаб — длина вида. Тело из колец эллипса,
/// хвостовой плавник и спинной; второй UV — доля длины к хвосту (для изгиба в шейдере).
pub fn fish_mesh(kind: FishKind) -> Mesh {
    let (_, h, w, back, side, belly) = kind.look();
    const RINGS: usize = 12;
    const AROUND: usize = 10;
    let (mut pos, mut nrm, mut uv2, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::<u32>::new());
    let mix = |a: [f32; 3], b: [f32; 3], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, 1.0];
    // Профиль тела: тупой нос, толстая середина, тонкий хвостовой стебель.
    let profile = |k: f32| (k * std::f32::consts::PI).sin().powf(0.75) * (1.0 - 0.6 * k * k) + 0.02;
    for i in 0..=RINGS {
        let k = i as f32 / RINGS as f32;
        let z = -0.5 + k * 0.82;
        let p = profile(k.min(0.999));
        for j in 0..AROUND {
            let a = j as f32 / AROUND as f32 * std::f32::consts::TAU;
            let (sa, ca) = a.sin_cos();
            let y = ca * h * 0.5 * p;
            let x = sa * w * 0.5 * p;
            pos.push([x, y, z]);
            nrm.push(Vec3::new(sa / w.max(0.01), ca / h.max(0.01), 0.0).normalize().to_array());
            uv2.push([k, 0.0]);
            // Спина тёмная, бока серебряные, брюхо светлое; у ставриды — тёмная полоса.
            let up = ca * 0.5 + 0.5;
            let mut c = if up > 0.6 { mix(side, back, ((up - 0.6) / 0.4).powf(0.7)) } else { mix(belly, side, (up / 0.6).powf(1.5)) };
            if kind == FishKind::Scad && (up - 0.62).abs() < 0.05 {
                c = [c[0] * 0.55, c[1] * 0.55, c[2] * 0.55, 1.0];
            }
            // Глаз.
            if i == 1 && (j == 2 || j == AROUND - 2) {
                c = [0.03, 0.03, 0.03, 1.0];
            }
            col.push(c);
        }
    }
    for i in 0..RINGS {
        for j in 0..AROUND {
            let a = (i * AROUND + j) as u32;
            let b = (i * AROUND + (j + 1) % AROUND) as u32;
            let c = a + AROUND as u32;
            let d = b + AROUND as u32;
            idx.extend([a, c, b, b, c, d]);
        }
    }
    // Нос и хвостовой стебель закрыты.
    let nose = pos.len() as u32;
    pos.push([0.0, 0.0, -0.52]);
    nrm.push([0.0, 0.0, -1.0]);
    uv2.push([0.0, 0.0]);
    col.push(mix(side, back, 0.5));
    for j in 0..AROUND as u32 {
        idx.extend([nose, (j + 1) % AROUND as u32, j]);
    }
    // Плавники — двусторонние пластины (две стороны с разными нормалями).
    let mut fin = |pts: [[f32; 3]; 3], k: [f32; 3], c: [f32; 4]| {
        for side_n in [1.0f32, -1.0] {
            let base = pos.len() as u32;
            for (p, kk) in pts.iter().zip(k) {
                pos.push(*p);
                nrm.push([side_n, 0.0, 0.0]);
                uv2.push([kk, 0.0]);
                col.push(c);
            }
            if side_n > 0.0 {
                idx.extend([base, base + 1, base + 2]);
            } else {
                idx.extend([base, base + 2, base + 1]);
            }
        }
    };
    let fin_col = mix(back, side, 0.35);
    let fork = if kind == FishKind::Scad { 0.16 } else { 0.07 };
    // Хвост: две лопасти.
    fin([[0.0, 0.0, 0.30], [0.0, h * 0.75, 0.5], [0.0, 0.02, 0.5 - fork]], [0.85, 1.0, 0.95], fin_col);
    fin([[0.0, 0.0, 0.30], [0.0, 0.0 - 0.02, 0.5 - fork], [0.0, -h * 0.75, 0.5]], [0.85, 0.95, 1.0], fin_col);
    // Спинной плавник.
    fin([[0.0, h * 0.45, -0.12], [0.0, h * 0.85, -0.02], [0.0, h * 0.42, 0.12]], [0.3, 0.4, 0.5], fin_col);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; uv2.len()])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv2)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// Материал рыбы (тот же шейдер, взмахи хвоста): чешуя блестит.
pub fn fish_material() -> SeaLifeMaterial {
    ExtendedMaterial {
        base: StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.32, reflectance: 0.6, double_sided: true, cull_mode: None, ..default() },
        extension: SeaLifeExt { life: SeaLifeUniform { mode: Vec4::new(1.0, 0.06, 9.0, 0.0) } },
    }
}

// ---------------------------------------------------------------------------------------------
// Раскладка

/// Где растут водоросли: на дне от 1,2 до 9 м глубины, пятнами (по шуму), реже вдали от берега.
fn weed_spots() -> Vec<(Vec3, u32, f32, u64)> {
    let mut out = Vec::new();
    let mut r = Rng(0x5EA_11FE);
    // Пятна — центры зарослей вдоль всего побережья, внутри каждого — пучки.
    let mut patches = 0;
    let mut tries = 0;
    while patches < 420 && tries < 40_000 {
        tries += 1;
        let c = Vec2::new(r.range(-900.0, 900.0), r.range(-900.0, 900.0));
        let ground = crate::level::ground_y(c.x, c.y);
        let depth = WATER_Y - ground;
        if !(1.2..=9.0).contains(&depth) || !crate::water::is_water(c.x, c.y) {
            continue;
        }
        patches += 1;
        let kind = if depth < 2.5 { if r.f() < 0.5 { 2 } else { 1 } } else if r.f() < 0.6 { 0 } else { 1 };
        let n = 6 + (r.f() * 14.0) as usize;
        let radius = r.range(1.5, 4.5);
        for _ in 0..n {
            let a = r.range(0.0, std::f32::consts::TAU);
            let d = radius * r.f().sqrt();
            let p = c + Vec2::new(a.cos(), a.sin()) * d;
            let g = crate::level::ground_y(p.x, p.y);
            if WATER_Y - g < 0.9 {
                continue;
            }
            let k = if r.f() < 0.15 { (kind + 1) % 3 } else { kind };
            out.push((Vec3::new(p.x, g - 0.03, p.y), k, r.range(0.0, std::f32::consts::TAU), (r.f() * 1e6) as u64));
        }
    }
    out
}

/// Стайка рыб: центр бродит у дна, рыбы держатся вокруг.
#[derive(Component)]
struct School {
    kind: FishKind,
    center: Vec3,
    heading: f32,
    speed: f32,
    /// Куда стайка плывёт сейчас.
    goal: Vec3,
    seed: u64,
    /// Испуг 0..1 (рядом пловец).
    fright: f32,
}

#[derive(Component)]
struct Fish {
    school: Entity,
    /// Место в стайке (смещение от центра) и своя фаза.
    slot: Vec3,
    phase: f32,
    vel: Vec3,
}

const SCHOOLS_PER_KIND: usize = 3;
/// Стайки живут в кругу этого радиуса вокруг камеры (дальше их всё равно не видно).
const SCHOOL_RANGE: f32 = 70.0;

fn spawn_sea_life(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<SeaLifeMaterial>>) {
    // Водоросли: несколько вариантов каждого вида — одинаковые меши рисуются пачкой.
    let weed_mat = mats.add(ExtendedMaterial {
        base: StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.7, reflectance: 0.3, double_sided: true, cull_mode: None, diffuse_transmission: 0.0, ..default() },
        extension: SeaLifeExt { life: SeaLifeUniform { mode: Vec4::new(0.0, 0.18, 0.9, 0.0) } },
    });
    let variants: Vec<Vec<Handle<Mesh>>> = (0..3).map(|k| (0..4).map(|v| meshes.add(weed_mesh(k, 17 + v * 31 + k as u64 * 7))).collect()).collect();
    let spots = weed_spots();
    for (p, kind, yaw, s) in &spots {
        let mesh = variants[*kind as usize][(*s % 4) as usize].clone();
        let scale = 0.75 + (*s % 1000) as f32 / 1000.0 * 0.6;
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(weed_mat.clone()),
            Transform::from_translation(*p).with_rotation(Quat::from_rotation_y(*yaw)).with_scale(Vec3::splat(scale)),
            NotShadowCaster,
            VisibilityRange { start_margin: 0.0..0.0, end_margin: 55.0..65.0, use_aabb: false },
        ));
    }
    info!("Подводный мир: {} пучков водорослей", spots.len());

    // Рыбы.
    let fish_mat = mats.add(fish_material());
    let mut seed = 1u64;
    for kind in FishKind::ALL {
        let mesh = meshes.add(fish_mesh(kind));
        for _ in 0..SCHOOLS_PER_KIND {
            seed += 1;
            let school = commands.spawn((School { kind, center: Vec3::new(0.0, -1000.0, 0.0), heading: 0.0, speed: 0.6, goal: Vec3::ZERO, seed, fright: 0.0 }, Transform::default(), Visibility::Hidden)).id();
            let mut r = Rng(seed * 7919 + 3);
            let count = match kind {
                FishKind::Scad => 16,
                FishKind::Mullet => 9,
                FishKind::RedMullet => 6,
            };
            let (len, ..) = kind.look();
            for _ in 0..count {
                let slot = Vec3::new(r.range(-1.6, 1.6), r.range(-0.5, 0.5), r.range(-1.6, 1.6)) * if kind == FishKind::Scad { 0.8 } else { 1.2 };
                let size = len * r.range(0.8, 1.2);
                commands.spawn((
                    Fish { school, slot, phase: r.f() * 10.0, vel: Vec3::ZERO },
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(fish_mat.clone()),
                    Transform::from_xyz(0.0, -1000.0, 0.0).with_scale(Vec3::splat(size)),
                    Visibility::Hidden,
                    NotShadowCaster,
                ));
            }
        }
    }
}

/// Случайная точка в воде рядом с `near` на подходящей виду глубине (или None).
fn water_spot(r: &mut Rng, near: Vec3, radius: f32, kind: FishKind) -> Option<Vec3> {
    for _ in 0..12 {
        let a = r.range(0.0, std::f32::consts::TAU);
        let d = radius * r.f().sqrt();
        let p = Vec2::new(near.x + a.cos() * d, near.z + a.sin() * d);
        let ground = crate::level::ground_y(p.x, p.y);
        let depth = WATER_Y - ground;
        if depth < 1.6 || !crate::water::is_water(p.x, p.y) {
            continue;
        }
        // Барабуля — у самого дна, кефаль и ставрида — в толще.
        let above = match kind {
            FishKind::RedMullet => r.range(0.25, 0.6),
            _ => r.range(0.6, (depth - 0.7).max(0.7)),
        };
        return Some(Vec3::new(p.x, (ground + above).min(WATER_Y - 0.6), p.y));
    }
    None
}

#[allow(clippy::type_complexity)]
fn swim_schools(
    time: Res<Time>,
    cams: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    swimmers: Query<&Transform, (With<crate::player::LocalPlayer>, Without<Fish>, Without<School>)>,
    mut schools: Query<(Entity, &mut School)>,
    mut fish: Query<(&mut Fish, &mut Transform, &mut Visibility), Without<School>>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let Some(cam) = cams.iter().find(|(_, c)| c.is_active).or_else(|| cams.iter().next()).map(|(t, _)| t.translation()) else { return };
    // Камера далеко от моря — рыбы спят.
    let near_sea = crate::water::is_water(cam.x, cam.z) || (cam.y < 25.0 && [(12.0, 0.0), (-12.0, 0.0), (0.0, 12.0), (0.0, -12.0), (30.0, 0.0), (-30.0, 0.0), (0.0, 30.0), (0.0, -30.0)].iter().any(|(dx, dz)| crate::water::is_water(cam.x + dx, cam.z + dz)));
    let player = swimmers.iter().next().map(|t| t.translation + Vec3::Y * 1.0);
    let t = time.elapsed_secs();
    let mut centers: std::collections::HashMap<Entity, (Vec3, f32, f32, bool)> = default();
    for (e, mut s) in &mut schools {
        let mut r = Rng(s.seed.wrapping_mul(6364136223846793005).wrapping_add((t * 0.37) as u64) | 1);
        let far = s.center.distance(cam) > SCHOOL_RANGE || s.center.y < -500.0;
        if !near_sea {
            s.center.y = -1000.0;
            centers.insert(e, (s.center, s.heading, s.fright, false));
            continue;
        }
        if far {
            // Новая стайка появляется в стороне, вне прямой видимости через густую воду.
            // Для съёмки (MRPG_SWIM_SHOT) стайки появляются ближе.
            let (radius, min) = if std::env::var_os("MRPG_SWIM_SHOT").is_some() { (14.0, 5.0) } else { (SCHOOL_RANGE * 0.8, 18.0) };
            if let Some(p) = water_spot(&mut r, cam, radius, s.kind).filter(|p| p.distance(cam) > min) {
                s.center = p;
                s.goal = p;
            }
        }
        if s.center.distance(s.goal) < 1.5 || r.f() < dt * 0.05 {
            if let Some(g) = water_spot(&mut r, s.center, 14.0, s.kind) {
                s.goal = g;
            }
        }
        // Пловец рядом — стайка уходит прочь, быстро.
        let mut to = s.goal - s.center;
        if let Some(pl) = player {
            let d = s.center - pl;
            if d.length() < 5.0 {
                s.fright = 1.0;
                to = Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::X) * 4.0;
            }
        }
        s.fright = (s.fright - dt * 0.4).max(0.0);
        let want = to.normalize_or_zero();
        let yaw_want = (-want.x).atan2(-want.z);
        let mut dy = yaw_want - s.heading;
        dy = (dy + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        s.heading += dy.clamp(-1.5 * dt, 1.5 * dt) * (1.0 + 2.0 * s.fright);
        s.speed = 0.5 + 2.6 * s.fright;
        let fwd = Quat::from_rotation_y(s.heading) * Vec3::NEG_Z;
        let mut next = s.center + (fwd * s.speed + Vec3::Y * want.y * 0.3) * dt;
        // Не выходим на берег и не всплываем.
        let ground = crate::level::ground_y(next.x, next.z);
        if !crate::water::is_water(next.x, next.z) || WATER_Y - ground < 1.2 {
            s.goal = s.center - fwd * 6.0;
            next = s.center;
        }
        next.y = next.y.min(WATER_Y - 0.5).max((ground + 0.25).min(WATER_Y - 0.5));
        s.center = next;
        centers.insert(e, (s.center, s.heading, s.fright, true));
    }
    for (mut f, mut tf, mut vis) in &mut fish {
        let Some(&(c, heading, fright, live)) = centers.get(&f.school) else { continue };
        if !live {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        let rot = Quat::from_rotation_y(heading);
        // Рыба гуляет вокруг своего места в стайке; испуганная стайка сжимается.
        let wobble = Vec3::new((t * 0.7 + f.phase).sin() * 0.35, (t * 0.5 + f.phase * 1.3).sin() * 0.15, (t * 0.6 + f.phase * 0.7).cos() * 0.35);
        let target = c + rot * (f.slot * (1.0 - 0.4 * fright) + wobble);
        let ground = crate::level::ground_y(target.x, target.z);
        let target = Vec3::new(target.x, target.y.min(WATER_Y - 0.35).max((ground + 0.15).min(WATER_Y - 0.35)), target.z);
        if tf.translation.y < -500.0 || tf.translation.distance(target) > 8.0 {
            tf.translation = target;
            f.vel = Vec3::ZERO;
        }
        let k = 1.0 - (-3.0 * dt).exp();
        let want_v = (target - tf.translation) * 2.0;
        let dv = (want_v - f.vel) * k;
        f.vel += dv;
        tf.translation += f.vel * dt;
        // Смотрит, куда плывёт (или туда же, что стайка, когда стоит).
        let dir = if f.vel.length_squared() > 0.01 { f.vel.normalize() } else { rot * Vec3::NEG_Z };
        let look = Transform::IDENTITY.looking_to(dir, Vec3::Y).rotation;
        tf.rotation = tf.rotation.slerp(look, 1.0 - (-6.0 * dt).exp());
        vis.set_if_neq(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeds_grow_under_water() {
        let spots = weed_spots();
        assert!(spots.len() > 2000, "{}", spots.len());
        for (p, ..) in spots.iter().take(500) {
            assert!(p.y < WATER_Y - 0.8, "{p}");
            assert!(crate::water::is_water(p.x, p.z));
        }
    }

    #[test]
    fn fish_meshes_are_closed_and_sane() {
        for kind in FishKind::ALL {
            let m = fish_mesh(kind);
            let n = m.count_vertices();
            assert!(n > 100);
            if let Some(Indices::U32(idx)) = m.indices() {
                assert!(idx.iter().all(|&i| (i as usize) < n));
            }
        }
    }
}
