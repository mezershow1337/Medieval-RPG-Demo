//! Клиент: зрелищные эффекты алхимии и травничества.
//!
//! Пока кто-то варит, у всех на виду стоит его сосуд: на очаге, на жаровне, на столе или
//! на костерке прямо на земле. Над горшком идёт пар, кипящее варево пузырится и дрожит,
//! из носика алембика в приёмник капает дистиллят, тигель раскаляется и сыплет искрами,
//! в ступке ходит пестик и пылит. Чем кончилось — тоже видно: клуб пара, бурый чад,
//! брызги через край, осколки лопнувшего стекла, огненный шар со столбом копоти,
//! жёлто-зелёное облако ядовитых паров, листья из-под рук травника.
//!
//! Частицы — мягкие «пушинки» (квадраты с размытым кругом, повёрнутые к камере) и мелкие
//! тела (искры, капли, осколки). Затухание — сменой материала по ступеням прозрачности.

use crate::brewing::{Heat, Vessel};
use crate::graphics::{Billboard, FlameMaterial, Flicker};
use crate::lore_net::{BurstFx, WorkView};
use crate::items::Rng;
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::HashMap;

/// Больше этого частиц разом не держим (слабые машины).
const MAX_PARTICLES: usize = 900;
/// Ступеней затухания у каждого облачного материала.
const FADE_STEPS: usize = 6;

pub struct AlchemyFxPlugin;

impl Plugin for AlchemyFxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FxQueue::default())
            .insert_resource(Works::default())
            .insert_resource(FxRng(Rng::from_time()))
            .add_systems(Startup, setup_fx_assets)
            .add_systems(Update, (apply_queue, light_braziers, emit, update_particles, face_camera, animate_rigs, fade_flashes).chain());
    }
}

/// Сюда сеть складывает начатые/законченные работы и разовые эффекты.
#[derive(Resource, Default)]
pub struct FxQueue {
    pub works: Vec<(u64, Option<WorkView>)>,
    pub bursts: Vec<(Vec3, BurstFx)>,
}

/// Чей сосуд где стоит.
#[derive(Resource, Default)]
struct Works(HashMap<u64, Entity>);

#[derive(Resource)]
struct FxRng(Rng);

/// Угли жаровни: сюда эффекты сами добавят язык пламени, искры и дымок.
#[derive(Component)]
pub struct BrazierFire {
    /// Размер жаровни (настольная меньше напольной).
    pub scale: f32,
}

// ---------------------------------------------------------------------------
// Ресурсы
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Cloud {
    Steam,
    Smoke,
    Soot,
    Toxic,
    Chad,
    Dust,
    Fire,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Bit {
    Spark,
    Glint,
    Drip,
    Spatter,
    Bubble,
    GlassShard,
    ClayShard,
    Leaf,
}

#[derive(Resource)]
struct FxAssets {
    quad: Handle<Mesh>,
    ball: Handle<Mesh>,
    chip: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    cube: Handle<Mesh>,
    flame_mesh: Handle<Mesh>,
    flame: Handle<FlameMaterial>,
    clouds: HashMap<Cloud, [Handle<StandardMaterial>; FADE_STEPS]>,
    bits: HashMap<Bit, Handle<StandardMaterial>>,
    clay: Handle<StandardMaterial>,
    copper: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    char_wood: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    brew: Handle<StandardMaterial>,
    distillate: Handle<StandardMaterial>,
    hot_crucible: Handle<StandardMaterial>,
    melt: Handle<StandardMaterial>,
}

/// Размытый круг с лёгкой клочковатостью — основа пара и дыма.
fn soft_puff_image() -> Image {
    const N: u32 = 64;
    let mut data = Vec::with_capacity((N * N * 4) as usize);
    let mut rng = Rng::new(4242);
    let noise: Vec<f32> = (0..64).map(|_| rng.f32()).collect();
    for y in 0..N {
        for x in 0..N {
            let dx = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let r = (dx * dx + dy * dy).sqrt();
            // Край «облачка» гуляет по углу.
            let a = dy.atan2(dx);
            let i = ((a / std::f32::consts::TAU + 0.5) * 16.0) as usize % 16;
            let wobble = 0.82 + 0.18 * (noise[i] * 0.6 + noise[(i + 1) % 16] * 0.4);
            let t = (1.0 - r / wobble).clamp(0.0, 1.0);
            let alpha = t * t * (3.0 - 2.0 * t);
            let grain = 0.85 + 0.15 * noise[((x * 7 + y * 13) % 64) as usize];
            data.extend_from_slice(&[255, 255, 255, (alpha * grain * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: N, height: N, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

fn setup_fx_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>, mut flames: ResMut<Assets<FlameMaterial>>) {
    let tex = images.add(soft_puff_image());
    let mut cloud = |c: Color, alpha: f32, add: bool| -> [Handle<StandardMaterial>; FADE_STEPS] {
        std::array::from_fn(|i| {
            let k = 1.0 - i as f32 / FADE_STEPS as f32;
            let base = if add { c.to_linear() * k } else { c.with_alpha(alpha * k).to_linear() };
            materials.add(StandardMaterial {
                base_color: base.into(),
                base_color_texture: Some(tex.clone()),
                alpha_mode: if add { AlphaMode::Add } else { AlphaMode::Blend },
                unlit: true,
                cull_mode: None,
                double_sided: true,
                ..default()
            })
        })
    };
    let clouds = HashMap::from([
        (Cloud::Steam, cloud(Color::srgb(0.97, 0.97, 0.99), 0.5, false)),
        (Cloud::Smoke, cloud(Color::srgb(0.42, 0.4, 0.38), 0.5, false)),
        (Cloud::Soot, cloud(Color::srgb(0.07, 0.06, 0.06), 0.78, false)),
        (Cloud::Toxic, cloud(Color::srgb(0.66, 0.72, 0.24), 0.5, false)),
        (Cloud::Chad, cloud(Color::srgb(0.36, 0.25, 0.14), 0.6, false)),
        (Cloud::Dust, cloud(Color::srgb(0.78, 0.7, 0.55), 0.4, false)),
        (Cloud::Fire, cloud(Color::linear_rgb(5.0, 1.6, 0.35), 1.0, true)),
    ]);
    let glow = |materials: &mut Assets<StandardMaterial>, c: Color, e: LinearRgba| materials.add(StandardMaterial { base_color: c, emissive: e, unlit: true, ..default() });
    let bits = HashMap::from([
        (Bit::Spark, glow(&mut materials, Color::srgb(1.0, 0.6, 0.2), LinearRgba::rgb(9.0, 3.2, 0.6))),
        (Bit::Glint, glow(&mut materials, Color::srgb(1.0, 0.95, 0.7), LinearRgba::rgb(5.0, 4.4, 2.4))),
        (
            Bit::Drip,
            materials.add(StandardMaterial { base_color: Color::srgba(0.78, 0.86, 0.9, 0.8), emissive: LinearRgba::rgb(0.25, 0.3, 0.32), perceptual_roughness: 0.05, alpha_mode: AlphaMode::Blend, ..default() }),
        ),
        (Bit::Spatter, materials.add(StandardMaterial { base_color: Color::srgb(0.32, 0.22, 0.1), perceptual_roughness: 0.1, ..default() })),
        (Bit::Bubble, materials.add(StandardMaterial { base_color: Color::srgba(0.95, 0.92, 0.85, 0.55), perceptual_roughness: 0.05, alpha_mode: AlphaMode::Blend, ..default() })),
        (
            Bit::GlassShard,
            materials.add(StandardMaterial { base_color: Color::srgba(0.8, 0.92, 0.88, 0.5), perceptual_roughness: 0.02, reflectance: 0.9, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() }),
        ),
        (Bit::ClayShard, materials.add(StandardMaterial { base_color: Color::srgb(0.55, 0.3, 0.18), perceptual_roughness: 0.95, ..default() })),
        (Bit::Leaf, materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.45, 0.17), perceptual_roughness: 0.8, cull_mode: None, double_sided: true, ..default() })),
    ]);
    let flat = |materials: &mut Assets<StandardMaterial>, c: Color, metal: f32, rough: f32| materials.add(StandardMaterial { base_color: c, metallic: metal, perceptual_roughness: rough, ..default() });
    commands.insert_resource(FxAssets {
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        ball: meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap()),
        chip: meshes.add(Cuboid::new(1.0, 0.25, 0.7)),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0)),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        flame_mesh: meshes.add(Rectangle::new(0.3, 0.5)),
        flame: flames.add(FlameMaterial::default()),
        clouds,
        bits,
        clay: flat(&mut materials, Color::srgb(0.58, 0.32, 0.19), 0.0, 0.9),
        copper: flat(&mut materials, Color::srgb(0.74, 0.42, 0.26), 0.9, 0.35),
        stone: flat(&mut materials, Color::srgb(0.55, 0.53, 0.5), 0.0, 0.85),
        wood: flat(&mut materials, Color::srgb(0.35, 0.24, 0.15), 0.0, 0.9),
        char_wood: materials.add(StandardMaterial { base_color: Color::srgb(0.08, 0.06, 0.05), emissive: LinearRgba::rgb(0.45, 0.09, 0.015), perceptual_roughness: 1.0, ..default() }),
        glass: materials.add(StandardMaterial {
            base_color: Color::srgba(0.72, 0.86, 0.82, 0.42),
            emissive: LinearRgba::rgb(0.04, 0.06, 0.06),
            perceptual_roughness: 0.03,
            reflectance: 0.8,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        brew: materials.add(StandardMaterial { base_color: Color::srgb(0.27, 0.2, 0.09), perceptual_roughness: 0.08, reflectance: 0.6, ..default() }),
        distillate: materials.add(StandardMaterial { base_color: Color::srgba(0.62, 0.5, 0.25, 0.75), perceptual_roughness: 0.05, alpha_mode: AlphaMode::Blend, ..default() }),
        hot_crucible: materials.add(StandardMaterial { base_color: Color::srgb(0.5, 0.25, 0.12), emissive: LinearRgba::rgb(2.4, 0.7, 0.12), perceptual_roughness: 0.8, ..default() }),
        melt: materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.7, 0.3), emissive: LinearRgba::rgb(8.0, 3.5, 0.8), unlit: true, ..default() }),
    });
}

// ---------------------------------------------------------------------------
// Частицы
// ---------------------------------------------------------------------------

#[derive(Component)]
struct Particle {
    vel: Vec3,
    /// Меньше нуля — ещё не появилась (отложенный дым после вспышки).
    age: f32,
    life: f32,
    size: (f32, f32),
    /// Сопротивление воздуха (доля скорости в секунду).
    drag: f32,
    /// Ускорение по вертикали: дым всплывает, капли падают.
    lift: f32,
    cloud: Option<Cloud>,
    step: usize,
    spin: Vec3,
}

/// Облачная частица смотрит в камеру.
#[derive(Component)]
struct FaceCamera;

struct P {
    pos: Vec3,
    vel: Vec3,
    life: f32,
    size: (f32, f32),
    drag: f32,
    lift: f32,
    delay: f32,
}

fn spawn_cloud(commands: &mut Commands, a: &FxAssets, kind: Cloud, p: P) {
    let mat = a.clouds[&kind][0].clone();
    commands.spawn((
        Particle { vel: p.vel, age: -p.delay, life: p.life, size: p.size, drag: p.drag, lift: p.lift, cloud: Some(kind), step: 0, spin: Vec3::ZERO },
        FaceCamera,
        Mesh3d(a.quad.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(p.pos).with_scale(Vec3::splat(if p.delay > 0.0 { 0.0 } else { p.size.0 })),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn spawn_bit(commands: &mut Commands, a: &FxAssets, kind: Bit, p: P, spin: Vec3) {
    let mesh = match kind {
        Bit::GlassShard | Bit::ClayShard | Bit::Leaf => a.chip.clone(),
        _ => a.ball.clone(),
    };
    commands.spawn((
        Particle { vel: p.vel, age: -p.delay, life: p.life, size: p.size, drag: p.drag, lift: p.lift, cloud: None, step: 0, spin },
        Mesh3d(mesh),
        MeshMaterial3d(a.bits[&kind].clone()),
        Transform::from_translation(p.pos).with_scale(Vec3::splat(if p.delay > 0.0 { 0.0 } else { p.size.0 })),
        NotShadowCaster,
    ));
}

fn update_particles(mut commands: Commands, time: Res<Time>, assets: Option<Res<FxAssets>>, mut q: Query<(Entity, &mut Particle, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>) {
    let Some(assets) = assets else { return };
    let dt = time.delta_secs().min(0.1);
    for (e, mut p, mut t, mut mat) in &mut q {
        p.age += dt;
        if p.age < 0.0 {
            continue;
        }
        let k = p.age / p.life;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y += p.lift * dt;
        let drag = (1.0 - p.drag * dt).max(0.0);
        p.vel *= drag;
        t.translation += p.vel * dt;
        // Облака быстро набухают и медленно расползаются; мелочь к концу уменьшается.
        let s = if p.cloud.is_some() { p.size.0 + (p.size.1 - p.size.0) * (1.0 - (1.0 - k).powi(2)) } else { p.size.0 + (p.size.1 - p.size.0) * k };
        t.scale = Vec3::splat(s.max(0.0));
        if p.spin != Vec3::ZERO {
            t.rotate(Quat::from_scaled_axis(p.spin * dt));
        }
        if let Some(c) = p.cloud {
            // Сначала проявляется, потом тает.
            let step = if k < 0.12 { ((0.12 - k) / 0.12 * 2.0) as usize } else { ((k - 0.12) / 0.88 * FADE_STEPS as f32) as usize };
            let step = step.min(FADE_STEPS - 1);
            if step != p.step {
                p.step = step;
                mat.0 = assets.clouds[&c][step].clone();
            }
        }
    }
}

fn face_camera(cameras: Query<&GlobalTransform, With<Camera3d>>, mut q: Query<&mut Transform, With<FaceCamera>>) {
    let Some(cam) = cameras.iter().next() else { return };
    let c = cam.translation();
    for mut t in &mut q {
        let d = c - t.translation;
        if d.length_squared() > 1e-4 {
            t.rotation = Transform::IDENTITY.looking_to(-d, Vec3::Y).rotation;
        }
    }
}

// ---------------------------------------------------------------------------
// Источники: пар над горшком, капли из алембика, искры жаровни
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Emit {
    /// Пар над жидкостью: радиус зеркала, сила.
    Steam { radius: f32, power: f32 },
    /// Пузыри на поверхности кипящего варева.
    Bubbles { radius: f32 },
    /// Капли из носика вниз, в приёмник (высота падения).
    Drips { fall: f32 },
    /// Струйка пара на стыке шлема и куба.
    Wisp,
    /// Искры с углей.
    Embers { radius: f32 },
    /// Тонкий дымок.
    Smoke { radius: f32 },
    /// Пыль из ступки.
    Dust,
    /// Жар над тиглем: искры и марево.
    Forge,
}

#[derive(Component)]
struct Emitter {
    kind: Emit,
    /// Частиц в секунду.
    rate: f32,
    acc: f32,
}

fn emitter(kind: Emit, rate: f32) -> Emitter {
    Emitter { kind, rate, acc: 0.0 }
}

fn emit(mut commands: Commands, time: Res<Time>, assets: Option<Res<FxAssets>>, mut rng: ResMut<FxRng>, mut emitters: Query<(&mut Emitter, &GlobalTransform, &InheritedVisibility)>, live: Query<(), With<Particle>>) {
    let Some(a) = assets else { return };
    let dt = time.delta_secs().min(0.1);
    let mut room = MAX_PARTICLES.saturating_sub(live.iter().count());
    let r = &mut rng.0;
    for (mut em, gt, vis) in &mut emitters {
        if !vis.get() {
            continue;
        }
        em.acc += em.rate * dt;
        let at = gt.translation();
        while em.acc >= 1.0 {
            em.acc -= 1.0;
            if room == 0 {
                continue;
            }
            room -= 1;
            let disc = |r: &mut Rng, radius: f32| {
                let a = r.f32() * std::f32::consts::TAU;
                let d = radius * r.f32().sqrt();
                Vec3::new(a.cos() * d, 0.0, a.sin() * d)
            };
            let jitter = |r: &mut Rng, s: f32| Vec3::new(r.range_f32(-s, s), 0.0, r.range_f32(-s, s));
            match em.kind {
                Emit::Steam { radius, power } => {
                    let p = P {
                        pos: at + disc(r, radius),
                        vel: Vec3::Y * r.range_f32(0.25, 0.5) * power + jitter(r, 0.08) + Vec3::new(0.05, 0.0, 0.02),
                        life: r.range_f32(1.4, 2.4) * power.sqrt(),
                        size: (radius * 0.8, r.range_f32(0.3, 0.5) * power.sqrt()),
                        drag: 0.35,
                        lift: 0.15,
                        delay: 0.0,
                    };
                    spawn_cloud(&mut commands, &a, Cloud::Steam, p);
                }
                Emit::Bubbles { radius } => {
                    let s = r.range_f32(0.012, 0.03);
                    let p = P { pos: at + disc(r, radius * 0.9), vel: Vec3::Y * 0.03, life: r.range_f32(0.25, 0.6), size: (s * 0.4, s), drag: 0.0, lift: 0.0, delay: 0.0 };
                    spawn_bit(&mut commands, &a, Bit::Bubble, p, Vec3::ZERO);
                }
                Emit::Drips { fall } => {
                    let life = (2.0 * fall / 9.8).sqrt();
                    let p = P { pos: at, vel: Vec3::ZERO, life, size: (0.012, 0.016), drag: 0.0, lift: -9.8, delay: 0.0 };
                    spawn_bit(&mut commands, &a, Bit::Drip, p, Vec3::ZERO);
                }
                Emit::Wisp => {
                    let p = P { pos: at + jitter(r, 0.02), vel: Vec3::Y * 0.2 + jitter(r, 0.1), life: r.range_f32(0.8, 1.4), size: (0.04, 0.18), drag: 0.5, lift: 0.1, delay: 0.0 };
                    spawn_cloud(&mut commands, &a, Cloud::Steam, p);
                }
                Emit::Embers { radius } => {
                    let p = P {
                        pos: at + disc(r, radius),
                        vel: Vec3::Y * r.range_f32(0.5, 1.3) + jitter(r, 0.25),
                        life: r.range_f32(0.6, 1.4),
                        size: (0.014, 0.004),
                        drag: 0.6,
                        lift: 0.3,
                        delay: 0.0,
                    };
                    spawn_bit(&mut commands, &a, Bit::Spark, p, Vec3::ZERO);
                }
                Emit::Smoke { radius } => {
                    let p = P {
                        pos: at + disc(r, radius) + Vec3::Y * 0.15,
                        vel: Vec3::Y * r.range_f32(0.3, 0.5) + jitter(r, 0.06) + Vec3::new(0.06, 0.0, 0.03),
                        life: r.range_f32(2.5, 4.0),
                        size: (0.08, r.range_f32(0.5, 0.8)),
                        drag: 0.2,
                        lift: 0.05,
                        delay: 0.0,
                    };
                    spawn_cloud(&mut commands, &a, Cloud::Smoke, p);
                }
                Emit::Dust => {
                    let p = P { pos: at + disc(r, 0.06), vel: Vec3::Y * r.range_f32(0.08, 0.2) + jitter(r, 0.12), life: r.range_f32(0.7, 1.2), size: (0.04, 0.16), drag: 1.2, lift: -0.05, delay: 0.0 };
                    spawn_cloud(&mut commands, &a, Cloud::Dust, p);
                }
                Emit::Forge => {
                    if r.f32() < 0.7 {
                        let p = P {
                            pos: at + disc(r, 0.04),
                            vel: Vec3::Y * r.range_f32(0.8, 2.0) + jitter(r, 0.6),
                            life: r.range_f32(0.4, 0.9),
                            size: (0.012, 0.003),
                            drag: 0.4,
                            lift: -1.5,
                            delay: 0.0,
                        };
                        spawn_bit(&mut commands, &a, Bit::Spark, p, Vec3::ZERO);
                    } else {
                        let p = P { pos: at, vel: Vec3::Y * 0.6 + jitter(r, 0.05), life: 1.2, size: (0.05, 0.3), drag: 0.3, lift: 0.1, delay: 0.0 };
                        spawn_cloud(&mut commands, &a, Cloud::Smoke, p);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Сосуды на огне
// ---------------------------------------------------------------------------

/// Пестик ходит по кругу.
#[derive(Component)]
struct Pestle;

/// Зеркало варева дрожит, когда кипит.
#[derive(Component)]
struct Simmer {
    base: f32,
    amp: f32,
}

/// Короткая вспышка света (лопнуло, вспыхнуло).
#[derive(Component)]
struct Flash {
    age: f32,
    life: f32,
    base: f32,
}

fn heat_power(h: Heat) -> f32 {
    match h {
        Heat::None => 0.0,
        Heat::Low => 0.6,
        Heat::Medium => 1.0,
        Heat::High => 1.6,
    }
}

fn apply_queue(mut commands: Commands, mut queue: ResMut<FxQueue>, mut works: ResMut<Works>, assets: Option<Res<FxAssets>>, mut rng: ResMut<FxRng>) {
    let Some(a) = assets else { return };
    if queue.works.is_empty() && queue.bursts.is_empty() {
        return;
    }
    for (player, work) in std::mem::take(&mut queue.works) {
        if let Some(e) = works.0.remove(&player) {
            commands.entity(e).try_despawn();
        }
        if let Some(w) = work {
            let e = spawn_rig(&mut commands, &a, &w);
            works.0.insert(player, e);
        }
    }
    for (pos, fx) in std::mem::take(&mut queue.bursts) {
        burst(&mut commands, &a, &mut rng.0, pos, fx);
    }
}

/// Сосуд с огнём, паром и прочим — у всех одинаковый.
fn spawn_rig(commands: &mut Commands, a: &FxAssets, w: &WorkView) -> Entity {
    let pos: Vec3 = w.pos.into();
    let fire = w.heat != Heat::None;
    let power = heat_power(w.heat);
    let part = |mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform| (Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t);
    commands
        .spawn((Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(w.yaw)), Visibility::default()))
        .with_children(|c| {
            // Свой костерок: камни, поленца, пламя и свет. Сосуд стоит на камнях.
            let lift = if w.own_fire && fire { 0.2 } else { 0.0 };
            if w.own_fire && fire {
                for i in 0..3 {
                    let ang = i as f32 * std::f32::consts::TAU / 3.0 + 0.3;
                    c.spawn(part(&a.ball, &a.stone, Transform::from_xyz(ang.cos() * 0.16, 0.08, ang.sin() * 0.16).with_scale(Vec3::new(0.14, 0.2, 0.14))));
                }
                for i in 0..4 {
                    let ang = i as f32 * 0.8;
                    c.spawn(part(
                        &a.cube,
                        &a.char_wood,
                        Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_y(ang)).with_scale(Vec3::new(0.32, 0.035, 0.035)),
                    ));
                }
                // Языки пламени выбиваются из-под сосуда между камнями.
                for i in 0..4 {
                    let ang = i as f32 * std::f32::consts::FRAC_PI_2 + 0.8;
                    let r = if i == 0 { 0.0 } else { 0.11 };
                    let k = 0.55 + 0.22 * power - if i == 0 { 0.0 } else { 0.12 };
                    c.spawn((
                        Mesh3d(a.flame_mesh.clone()),
                        MeshMaterial3d(a.flame.clone()),
                        Transform::from_xyz(ang.cos() * r, 0.25 * k, ang.sin() * r).with_scale(Vec3::splat(k)),
                        Billboard,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
                c.spawn((Transform::from_xyz(0.0, 0.05, 0.0), Visibility::default(), emitter(Emit::Embers { radius: 0.08 }, 3.0 * power)));
                let intensity = 6000.0 + 5000.0 * power;
                c.spawn((
                    PointLight { color: Color::srgb(1.0, 0.55, 0.25), intensity, range: 5.0, shadow_maps_enabled: false, ..default() },
                    Transform::from_xyz(0.0, 0.3, 0.0),
                    Flicker { base: intensity, phase: pos.x + pos.z },
                ));
            }
            let y0 = lift;
            match w.vessel {
                Vessel::ClayPot | Vessel::CopperKettle => {
                    let mat = if w.vessel == Vessel::ClayPot { &a.clay } else { &a.copper };
                    let (r, h) = if w.vessel == Vessel::ClayPot { (0.13, 0.2) } else { (0.14, 0.17) };
                    c.spawn(part(&a.ball, mat, Transform::from_xyz(0.0, y0 + h * 0.45, 0.0).with_scale(Vec3::new(r * 2.0, h * 0.95, r * 2.0))));
                    c.spawn(part(&a.cylinder, mat, Transform::from_xyz(0.0, y0 + h * 0.86, 0.0).with_scale(Vec3::new(r * 1.55, h * 0.28, r * 1.55))));
                    let surface = y0 + h * 0.97;
                    c.spawn((part(&a.cylinder, &a.brew, Transform::from_xyz(0.0, surface, 0.0).with_scale(Vec3::new(r * 1.45, 0.006, r * 1.45))), Simmer { base: surface, amp: 0.004 * power }));
                    if w.vessel == Vessel::CopperKettle {
                        // Дужка.
                        c.spawn(part(&a.cylinder, &a.copper, Transform::from_xyz(0.0, y0 + h * 1.35, 0.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.012, r * 2.1, 0.012))));
                    }
                    if fire {
                        c.spawn((Transform::from_xyz(0.0, surface + 0.02, 0.0), Visibility::default(), emitter(Emit::Steam { radius: r * 0.6, power }, 2.0 + 7.0 * power)));
                        if w.heat >= Heat::Medium {
                            c.spawn((Transform::from_xyz(0.0, surface + 0.005, 0.0), Visibility::default(), emitter(Emit::Bubbles { radius: r * 0.7 }, 8.0 * power)));
                        }
                    } else {
                        // Настой: редкие пузырьки.
                        c.spawn((Transform::from_xyz(0.0, surface + 0.005, 0.0), Visibility::default(), emitter(Emit::Bubbles { radius: r * 0.6 }, 0.8)));
                    }
                }
                Vessel::Mortar => {
                    c.spawn(part(&a.cylinder, &a.stone, Transform::from_xyz(0.0, y0 + 0.05, 0.0).with_scale(Vec3::new(0.2, 0.1, 0.2))));
                    c.spawn(part(&a.cylinder, &a.brew, Transform::from_xyz(0.0, y0 + 0.1, 0.0).with_scale(Vec3::new(0.15, 0.004, 0.15))));
                    c.spawn((Pestle, Transform::from_xyz(0.0, y0 + 0.1, 0.0), Visibility::default())).with_children(|p| {
                        p.spawn(part(&a.cylinder, &a.wood, Transform::from_xyz(0.0, 0.1, 0.0).with_scale(Vec3::new(0.032, 0.24, 0.032))));
                    });
                    c.spawn((Transform::from_xyz(0.0, y0 + 0.11, 0.0), Visibility::default(), emitter(Emit::Dust, 4.0)));
                }
                Vessel::Alembic | Vessel::Retort | Vessel::Kerotakis => {
                    let glass = w.vessel != Vessel::Kerotakis;
                    let body_mat = if glass { &a.glass } else { &a.copper };
                    // Куб (кукурбита) с варевом внутри.
                    c.spawn(part(&a.ball, body_mat, Transform::from_xyz(0.0, y0 + 0.11, 0.0).with_scale(Vec3::splat(0.23))));
                    if glass {
                        c.spawn((part(&a.ball, &a.brew, Transform::from_xyz(0.0, y0 + 0.07, 0.0).with_scale(Vec3::new(0.15, 0.1, 0.15))), Simmer { base: y0 + 0.07, amp: 0.003 * power }));
                    }
                    let (tip, recv) = if w.vessel == Vessel::Retort {
                        // Реторта: шея уходит вбок и вниз прямо в приёмник.
                        c.spawn(part(&a.cylinder, &a.glass, Transform::from_xyz(0.17, y0 + 0.17, 0.0).with_rotation(Quat::from_rotation_z(-1.15)).with_scale(Vec3::new(0.035, 0.3, 0.035))));
                        (Vec3::new(0.3, y0 + 0.1, 0.0), Vec3::new(0.34, 0.0, 0.0))
                    } else {
                        // Шлем-амбик с носиком.
                        c.spawn(part(&a.ball, body_mat, Transform::from_xyz(0.0, y0 + 0.23, 0.0).with_scale(Vec3::new(0.15, 0.11, 0.15))));
                        c.spawn(part(&a.cylinder, body_mat, Transform::from_xyz(0.17, y0 + 0.2, 0.0).with_rotation(Quat::from_rotation_z(-1.25)).with_scale(Vec3::new(0.022, 0.26, 0.022))));
                        (Vec3::new(0.29, y0 + 0.15, 0.0), Vec3::new(0.34, 0.0, 0.0))
                    };
                    // Приёмник: колба рядом, на земле или на столе.
                    let recv_pos = recv;
                    c.spawn(part(&a.ball, &a.glass, Transform::from_translation(recv_pos + Vec3::Y * 0.05).with_scale(Vec3::splat(0.11))));
                    c.spawn(part(&a.cylinder, &a.glass, Transform::from_translation(recv_pos + Vec3::Y * 0.12).with_scale(Vec3::new(0.035, 0.07, 0.035))));
                    c.spawn(part(&a.ball, &a.distillate, Transform::from_translation(recv_pos + Vec3::Y * 0.03).with_scale(Vec3::new(0.085, 0.05, 0.085))));
                    if fire {
                        let fall = (tip.y - (recv_pos.y + 0.06)).max(0.05);
                        c.spawn((Transform::from_translation(tip), Visibility::default(), emitter(Emit::Drips { fall }, 1.2 + 2.0 * power)));
                        // Керотакис закрыт: пар наружу не идёт.
                        if glass {
                            c.spawn((Transform::from_xyz(0.0, y0 + 0.2, 0.0), Visibility::default(), emitter(Emit::Wisp, 1.5 * power)));
                            c.spawn((Transform::from_xyz(0.0, y0 + 0.06, 0.0), Visibility::default(), emitter(Emit::Bubbles { radius: 0.05 }, 6.0 * power)));
                        }
                    }
                }
                Vessel::DryingTray => {
                    // Плетёная решётка на двух брусках, на ней разложены листья.
                    c.spawn(part(&a.cube, &a.wood, Transform::from_xyz(0.0, y0 + 0.08, 0.0).with_scale(Vec3::new(0.62, 0.025, 0.42))));
                    for x in [-0.25, 0.25] {
                        c.spawn(part(&a.cube, &a.wood, Transform::from_xyz(x, y0 + 0.035, 0.0).with_scale(Vec3::new(0.05, 0.07, 0.44))));
                    }
                    let mut r = Rng::new(pos.x.to_bits() as u64 ^ 0x5eed);
                    for _ in 0..22 {
                        let (x, z) = (r.range_f32(-0.27, 0.27), r.range_f32(-0.18, 0.18));
                        c.spawn(part(
                            &a.chip,
                            &a.bits[&Bit::Leaf],
                            Transform::from_xyz(x, y0 + 0.1, z).with_rotation(Quat::from_rotation_y(r.f32() * 6.3)).with_scale(Vec3::new(0.07, 0.02, 0.04)),
                        ));
                    }
                    if fire {
                        c.spawn((Transform::from_xyz(0.0, y0 + 0.12, 0.0), Visibility::default(), emitter(Emit::Wisp, 1.2)));
                    }
                }
                Vessel::Crucible => {
                    let mat = if fire { &a.hot_crucible } else { &a.clay };
                    c.spawn(part(&a.cylinder, mat, Transform::from_xyz(0.0, y0 + 0.06, 0.0).with_scale(Vec3::new(0.12, 0.12, 0.12))));
                    if fire {
                        c.spawn(part(&a.cylinder, &a.melt, Transform::from_xyz(0.0, y0 + 0.121, 0.0).with_scale(Vec3::new(0.1, 0.004, 0.1))));
                        c.spawn((Transform::from_xyz(0.0, y0 + 0.13, 0.0), Visibility::default(), emitter(Emit::Forge, 6.0 + 6.0 * power)));
                        let intensity = 3000.0 * power;
                        c.spawn((
                            PointLight { color: Color::srgb(1.0, 0.5, 0.2), intensity, range: 3.0, shadow_maps_enabled: false, ..default() },
                            Transform::from_xyz(0.0, y0 + 0.3, 0.0),
                            Flicker { base: intensity, phase: pos.z },
                        ));
                    }
                }
            }
        })
        .id()
}

fn animate_rigs(time: Res<Time>, mut pestles: Query<&mut Transform, (With<Pestle>, Without<Simmer>)>, mut simmers: Query<(&mut Transform, &Simmer), Without<Pestle>>) {
    let t = time.elapsed_secs();
    for mut p in &mut pestles {
        // Пестик наклонён и ходит по кругу, то нажимая, то отпуская.
        let a = t * 3.2;
        p.rotation = Quat::from_rotation_y(a) * Quat::from_rotation_z(0.35);
        p.translation.y = p.translation.y.clamp(0.0, 10.0);
        p.scale = Vec3::new(1.0, 1.0 + 0.04 * (t * 6.4).sin(), 1.0);
    }
    for (mut tr, s) in &mut simmers {
        if s.amp > 0.0 {
            tr.translation.y = s.base + s.amp * ((t * 17.0).sin() * 0.6 + (t * 29.0 + 1.3).sin() * 0.4);
        }
    }
}

/// Поставленным жаровням — пламя над углями, искры и дымок.
fn light_braziers(mut commands: Commands, assets: Option<Res<FxAssets>>, new: Query<(Entity, &BrazierFire), Added<BrazierFire>>) {
    let Some(a) = assets else { return };
    for (e, b) in &new {
        let k = b.scale;
        commands.entity(e).with_children(|c| {
            c.spawn((Mesh3d(a.flame_mesh.clone()), MeshMaterial3d(a.flame.clone()), Transform::from_xyz(0.0, 0.1 * k, 0.0).with_scale(Vec3::splat(0.55 * k)), Billboard, NotShadowCaster, NotShadowReceiver));
            c.spawn((Transform::default(), Visibility::default(), emitter(Emit::Embers { radius: 0.12 * k }, 4.0)));
            c.spawn((Transform::default(), Visibility::default(), emitter(Emit::Smoke { radius: 0.08 * k }, 0.8)));
        });
    }
}

fn fade_flashes(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Flash, &mut PointLight)>) {
    let dt = time.delta_secs().min(0.1);
    for (e, mut f, mut l) in &mut q {
        f.age += dt;
        if f.age >= f.life {
            commands.entity(e).despawn();
            continue;
        }
        let k = 1.0 - f.age / f.life;
        l.intensity = f.base * k * k;
    }
}

// ---------------------------------------------------------------------------
// Разовые эффекты
// ---------------------------------------------------------------------------

fn flash(commands: &mut Commands, pos: Vec3, color: Color, intensity: f32, range: f32, life: f32) {
    commands.spawn((PointLight { color, intensity, range, shadow_maps_enabled: false, ..default() }, Transform::from_translation(pos), Flash { age: 0.0, life, base: intensity }));
}

fn burst(commands: &mut Commands, a: &FxAssets, r: &mut Rng, pos: Vec3, fx: BurstFx) {
    let ball = |r: &mut Rng, speed: (f32, f32), up: f32| {
        let ang = r.f32() * std::f32::consts::TAU;
        let s = r.range_f32(speed.0, speed.1);
        Vec3::new(ang.cos() * s, r.range_f32(up * 0.5, up * 1.2), ang.sin() * s)
    };
    let spin = |r: &mut Rng| Vec3::new(r.range_f32(-14.0, 14.0), r.range_f32(-14.0, 14.0), r.range_f32(-14.0, 14.0));
    match fx {
        BurstFx::Done => {
            for _ in 0..14 {
                let p = P { pos: pos + Vec3::Y * 0.2, vel: ball(r, (0.1, 0.4), 0.8), life: r.range_f32(1.4, 2.2), size: (0.1, 0.5), drag: 0.8, lift: 0.2, delay: r.range_f32(0.0, 0.25) };
                spawn_cloud(commands, a, Cloud::Steam, p);
            }
            for _ in 0..8 {
                let p = P { pos: pos + Vec3::Y * 0.25, vel: ball(r, (0.05, 0.25), 0.5), life: r.range_f32(0.8, 1.4), size: (0.02, 0.0), drag: 1.0, lift: 0.0, delay: r.range_f32(0.0, 0.4) };
                spawn_bit(commands, a, Bit::Glint, p, Vec3::ZERO);
            }
        }
        BurstFx::Spoiled => {
            for _ in 0..16 {
                let p = P { pos: pos + Vec3::Y * 0.2, vel: ball(r, (0.05, 0.25), 0.4), life: r.range_f32(2.5, 4.0), size: (0.12, 0.7), drag: 0.4, lift: 0.08, delay: r.range_f32(0.0, 0.8) };
                spawn_cloud(commands, a, Cloud::Chad, p);
            }
        }
        BurstFx::BoilOver => {
            for _ in 0..22 {
                let p = P { pos: pos + Vec3::Y * 0.2, vel: ball(r, (0.4, 1.2), 1.8), life: r.range_f32(0.4, 0.7), size: (0.025, 0.015), drag: 0.2, lift: -9.8, delay: 0.0 };
                spawn_bit(commands, a, Bit::Spatter, p, Vec3::ZERO);
            }
            for _ in 0..16 {
                let p = P { pos: pos + Vec3::Y * 0.2, vel: ball(r, (0.3, 0.7), 1.0), life: r.range_f32(1.0, 1.8), size: (0.1, 0.55), drag: 1.2, lift: 0.2, delay: 0.0 };
                spawn_cloud(commands, a, Cloud::Steam, p);
            }
        }
        BurstFx::Shatter { glass } => {
            let kind = if glass { Bit::GlassShard } else { Bit::ClayShard };
            for _ in 0..26 {
                let s = r.range_f32(0.02, 0.06);
                let p = P { pos: pos + Vec3::Y * 0.12, vel: ball(r, (0.8, 2.6), 2.2), life: r.range_f32(0.7, 1.2), size: (s, s * 0.8), drag: 0.3, lift: -9.8, delay: 0.0 };
                spawn_bit(commands, a, kind, p, spin(r));
            }
            for _ in 0..14 {
                let p = P { pos: pos + Vec3::Y * 0.15, vel: ball(r, (0.6, 1.4), 0.8), life: r.range_f32(0.8, 1.6), size: (0.1, 0.6), drag: 2.0, lift: 0.3, delay: 0.0 };
                spawn_cloud(commands, a, Cloud::Steam, p);
            }
            for _ in 0..10 {
                let p = P { pos: pos + Vec3::Y * 0.1, vel: ball(r, (0.5, 1.6), 1.6), life: r.range_f32(0.3, 0.6), size: (0.014, 0.004), drag: 0.5, lift: -6.0, delay: 0.0 };
                spawn_bit(commands, a, Bit::Spark, p, Vec3::ZERO);
            }
            flash(commands, pos + Vec3::Y * 0.3, Color::srgb(1.0, 0.8, 0.6), 30_000.0, 6.0, 0.25);
        }
        BurstFx::Ignition => {
            // Огненный шар, искры, потом столб жирной копоти.
            for _ in 0..30 {
                let p = P { pos: pos + Vec3::Y * 0.2, vel: ball(r, (0.4, 1.6), 1.6), life: r.range_f32(0.45, 0.9), size: (0.15, r.range_f32(0.6, 1.0)), drag: 2.5, lift: 1.5, delay: r.range_f32(0.0, 0.15) };
                spawn_cloud(commands, a, Cloud::Fire, p);
            }
            for _ in 0..28 {
                let p = P { pos: pos + Vec3::Y * 0.25, vel: ball(r, (0.6, 2.5), 3.0), life: r.range_f32(0.6, 1.3), size: (0.016, 0.004), drag: 0.6, lift: -3.0, delay: 0.0 };
                spawn_bit(commands, a, Bit::Spark, p, Vec3::ZERO);
            }
            for _ in 0..34 {
                let p = P {
                    pos: pos + Vec3::Y * 0.5 + Vec3::new(r.range_f32(-0.2, 0.2), 0.0, r.range_f32(-0.2, 0.2)),
                    vel: Vec3::Y * r.range_f32(0.6, 1.3) + Vec3::new(r.range_f32(-0.15, 0.15), 0.0, r.range_f32(-0.15, 0.15)),
                    life: r.range_f32(3.0, 5.0),
                    size: (0.25, r.range_f32(1.0, 1.6)),
                    drag: 0.25,
                    lift: 0.1,
                    delay: r.range_f32(0.2, 2.5),
                };
                spawn_cloud(commands, a, Cloud::Soot, p);
            }
            flash(commands, pos + Vec3::Y * 0.6, Color::srgb(1.0, 0.55, 0.2), 160_000.0, 12.0, 0.9);
        }
        BurstFx::Fumes => {
            for _ in 0..30 {
                let p = P {
                    pos: pos + Vec3::Y * 0.3,
                    vel: ball(r, (0.1, 0.45), 0.25),
                    life: r.range_f32(5.0, 8.0),
                    size: (0.2, r.range_f32(1.0, 1.5)),
                    drag: 0.5,
                    lift: 0.02,
                    delay: r.range_f32(0.0, 1.5),
                };
                spawn_cloud(commands, a, Cloud::Toxic, p);
            }
        }
        BurstFx::Bellows => {
            for _ in 0..16 {
                let p = P { pos: pos + Vec3::Y * 0.05, vel: ball(r, (0.1, 0.5), 1.8), life: r.range_f32(0.5, 1.1), size: (0.016, 0.004), drag: 0.5, lift: 0.2, delay: 0.0 };
                spawn_bit(commands, a, Bit::Spark, p, Vec3::ZERO);
            }
            for _ in 0..4 {
                let p = P { pos: pos + Vec3::Y * 0.1, vel: ball(r, (0.05, 0.2), 0.8), life: r.range_f32(0.25, 0.45), size: (0.08, 0.3), drag: 2.0, lift: 1.0, delay: 0.0 };
                spawn_cloud(commands, a, Cloud::Fire, p);
            }
            flash(commands, pos + Vec3::Y * 0.3, Color::srgb(1.0, 0.6, 0.25), 12_000.0, 4.0, 0.35);
        }
        BurstFx::Hiss => {
            for _ in 0..12 {
                let p = P { pos, vel: ball(r, (0.3, 0.8), 0.6), life: r.range_f32(0.6, 1.1), size: (0.06, 0.32), drag: 2.5, lift: 0.4, delay: 0.0 };
                spawn_cloud(commands, a, Cloud::Steam, p);
            }
        }
        BurstFx::Gathered => {
            for _ in 0..12 {
                let s = r.range_f32(0.03, 0.06);
                let p = P { pos: pos + Vec3::Y * 0.3, vel: ball(r, (0.2, 0.8), 1.2), life: r.range_f32(0.8, 1.3), size: (s, s), drag: 1.5, lift: -2.5, delay: 0.0 };
                spawn_bit(commands, a, Bit::Leaf, p, spin(r) * 0.5);
            }
            for _ in 0..6 {
                let p = P { pos: pos + Vec3::Y * 0.1, vel: ball(r, (0.1, 0.4), 0.3), life: r.range_f32(0.8, 1.2), size: (0.08, 0.35), drag: 1.5, lift: 0.0, delay: 0.0 };
                spawn_cloud(commands, a, Cloud::Dust, p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puff_texture_is_soft_round() {
        let img = soft_puff_image();
        let data = img.data.as_ref().unwrap();
        let alpha = |x: usize, y: usize| data[(y * 64 + x) * 4 + 3];
        assert!(alpha(32, 32) > 200, "центр плотный");
        assert_eq!(alpha(0, 0), 0, "углы прозрачные");
        assert!(alpha(32, 32) > alpha(32, 50), "к краю тает");
    }
}
