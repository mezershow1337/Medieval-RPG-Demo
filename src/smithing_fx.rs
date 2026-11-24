//! Зрелище кузницы: раскалённая поковка в клещах, молот, искры и окалина, пар и бульканье
//! в корыте, вспышка масла, сноп искр с точила, струя бронзы из тигля, дым и жар горна.
//! Звуки синтезируются здесь же — без чужих записей.
//!
//! Всё это только вид: сервер шлёт [`Fx`] и [`PieceView`], клиент рисует.

use crate::smithing::*;
use crate::smithing_client::SmithState;
use crate::smithing_net::*;
use bevy::audio::{AddAudioSource, Decodable, Sample, Source, Volume};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;

pub struct SmithFxPlugin;

impl Plugin for SmithFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Synth>()
            .init_resource::<FxRng>()
            .init_resource::<DipsOut>()
            .add_systems(Startup, setup_fx)
            .add_systems(
                Update,
                showcase.run_if(|| std::env::var("MRPG_SMITH_SHOWCASE").is_ok()),
            )
            .add_systems(
                Update,
                demo.run_if(|| std::env::var("MRPG_SMITH_DEMO").is_ok())
                    .before(play_fx)
                    .after(crate::smithing_client::SmithSet),
            )
            .add_systems(
                Update,
                (play_fx, forge_fires, pieces, dips, hammers, particles)
                    .chain()
                    .after(crate::smithing_client::SmithSet),
            );
    }
}

/// Сколько частиц может жить разом (чтобы толпа у горна не уронила кадр).
const MAX_PARTICLES: usize = 900;
/// Дальше этого (м) не видно и не слышно мелочей.
const FX_RANGE: f32 = 45.0;

// ---------------------------------------------------------------------------
// Звук
// ---------------------------------------------------------------------------

const RATE: u32 = 44_100;

/// Синтезированный звук (моно, 44.1 кГц).
#[derive(Asset, TypePath, Clone)]
pub struct Synth {
    data: Arc<[f32]>,
}

pub struct SynthDecoder {
    data: Arc<[f32]>,
    pos: usize,
}

impl Iterator for SynthDecoder {
    type Item = Sample;
    fn next(&mut self) -> Option<Sample> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v as Sample)
    }
}

impl Source for SynthDecoder {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.data.len() - self.pos)
    }
    fn channels(&self) -> NonZero<u16> {
        NonZero::new(1).unwrap()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.data.len() as f32 / RATE as f32,
        ))
    }
}

impl Decodable for Synth {
    type Decoder = SynthDecoder;
    fn decoder(&self) -> SynthDecoder {
        SynthDecoder {
            data: self.data.clone(),
            pos: 0,
        }
    }
}

/// Простой шум без внешних крейтов.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn buffer(secs: f32) -> Vec<f32> {
    vec![0.0; (secs * RATE as f32) as usize]
}

fn normalize(mut v: Vec<f32>, peak: f32) -> Synth {
    let m = v.iter().fold(0.0f32, |a, x| a.max(x.abs())).max(1e-6);
    for x in &mut v {
        *x *= peak / m;
    }
    // Мягкие края, чтобы не щёлкало.
    let n = v.len();
    for i in 0..64.min(n) {
        v[i] *= i as f32 / 64.0;
        v[n - 1 - i] *= i as f32 / 64.0;
    }
    Synth { data: v.into() }
}

/// Звон наковальни: негармонические призвуки толстой стальной плиты, щелчок удара.
pub fn synth_clang(base: f32, ring: f32, hot: f32, seed: u32) -> Synth {
    let mut v = buffer(ring + 0.1);
    let mut n = Noise(seed | 1);
    let partials = [
        (1.0, 1.0, 1.0),
        (2.76, 0.55, 1.4),
        (5.40, 0.35, 2.2),
        (8.93, 0.22, 3.1),
        (13.3, 0.12, 4.0),
    ];
    for (i, s) in v.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let mut x = 0.0;
        for (k, a, d) in partials {
            // Горячее железо глушит звон — удар глуше и короче.
            let decay = (-t * d * (1.0 + hot * 2.5) / ring).exp();
            x += (TAU * base * k * t).sin() * a * decay;
        }
        let click = n.next() * (-t * 90.0).exp() * 0.9;
        let thud = (TAU * 70.0 * t).sin() * (-t * 18.0).exp() * (0.4 + hot);
        *s = x * (1.0 - hot * 0.55) + click + thud;
    }
    normalize(v, 0.85)
}

/// Шипение закалки: шум, в начале взрыв пара, дальше — бульканье пузырей.
pub fn synth_sizzle(secs: f32, oil: bool, seed: u32) -> Synth {
    let mut v = buffer(secs);
    let mut n = Noise(seed | 1);
    let (mut hp, mut lp) = (0.0f32, 0.0f32);
    let mut prev = 0.0;
    // Пузыри: короткие поднимающиеся тона.
    let mut bubbles: Vec<(f32, f32, f32)> = Vec::new();
    let mut t_next = 0.05;
    let mut rng = Noise(seed.wrapping_mul(747796405) | 1);
    while t_next < secs * 0.95 {
        let f = 260.0 + (rng.next() * 0.5 + 0.5) * 520.0;
        bubbles.push((t_next, f, 0.3 + (rng.next() * 0.5 + 0.5) * 0.7));
        t_next += 0.025 + (rng.next() * 0.5 + 0.5) * if oil { 0.12 } else { 0.07 };
    }
    for (i, s) in v.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let w = n.next();
        hp = w - prev + hp * 0.6; // верхи — шипение
        prev = w;
        lp += (w - lp) * 0.08; // низ — рёв и плеск
        let env = (t / 0.03).min(1.0) * (-t * 2.2 / secs).exp();
        let burst = (-t * 6.0).exp();
        let crackle = if n.next() > 0.985 {
            n.next() * 3.0
        } else {
            0.0
        };
        let hiss = if oil {
            lp * 1.6 + hp * 0.25
        } else {
            hp * 0.9 + lp * 0.5
        };
        *s = (hiss + crackle * 0.3) * env + lp * burst * 1.2;
        for &(b0, f, a) in &bubbles {
            let dt = t - b0;
            if (0.0..0.04).contains(&dt) {
                let ff = f * (1.0 + dt * 18.0);
                *s += (TAU * ff * dt).sin() * (-dt * 90.0).exp() * a * 0.5 * (1.0 - t / secs);
            }
        }
    }
    normalize(v, 0.7)
}

/// Точило: шуршание с визгом.
pub fn synth_grind(seed: u32) -> Synth {
    let secs = 0.45;
    let mut v = buffer(secs);
    let mut n = Noise(seed | 1);
    let mut bp = (0.0f32, 0.0f32);
    for (i, s) in v.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let w = n.next();
        // Полосовой фильтр около 3 кГц.
        let f = 0.45;
        bp.0 += f * (w - bp.0 - bp.1 * 0.3);
        bp.1 += f * bp.0;
        let env = (t / 0.05).min(1.0) * ((secs - t) / 0.12).clamp(0.0, 1.0);
        let whine = (TAU * (2400.0 + 300.0 * (t * 30.0).sin()) * t).sin() * 0.15;
        *s = (bp.0 * 0.8 + whine) * env * (0.8 + 0.2 * (t * 47.0).sin());
    }
    normalize(v, 0.45)
}

/// Треск лопнувшего металла.
pub fn synth_crack(seed: u32) -> Synth {
    let mut v = buffer(0.35);
    let mut n = Noise(seed | 1);
    for (i, s) in v.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        *s = n.next() * (-t * 60.0).exp()
            + (TAU * 1700.0 * t).sin() * (-t * 25.0).exp() * 0.5
            + (TAU * 95.0 * t).sin() * (-t * 12.0).exp() * 0.4;
    }
    normalize(v, 0.8)
}

/// Глухой рёв: вспыхнувшее масло, бульканье расплава, дутьё.
pub fn synth_roar(secs: f32, smooth: f32, seed: u32) -> Synth {
    let mut v = buffer(secs);
    let mut n = Noise(seed | 1);
    let (mut a, mut b) = (0.0f32, 0.0f32);
    for (i, s) in v.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        a += (n.next() - a) * smooth;
        b += (a - b) * smooth;
        let env = (t / (secs * 0.25)).min(1.0) * ((secs - t) / (secs * 0.4)).clamp(0.0, 1.0);
        *s = b * env;
    }
    normalize(v, 0.6)
}

/// Бульканье расплава в тигле и струи в форме.
pub fn synth_pour(seed: u32) -> Synth {
    let secs = 1.2;
    let mut v = synth_roar(secs, 0.05, seed).data.to_vec();
    let mut rng = Noise(seed | 3);
    let mut t0 = 0.0;
    while t0 < secs - 0.1 {
        let f = 140.0 + (rng.next() * 0.5 + 0.5) * 260.0;
        let i0 = (t0 * RATE as f32) as usize;
        for j in 0..(0.06 * RATE as f32) as usize {
            let dt = j as f32 / RATE as f32;
            if let Some(s) = v.get_mut(i0 + j) {
                *s += (TAU * f * (1.0 + dt * 8.0) * dt).sin() * (-dt * 45.0).exp() * 0.5;
            }
        }
        t0 += 0.05 + (rng.next() * 0.5 + 0.5) * 0.12;
    }
    normalize(v, 0.55)
}

#[derive(Resource)]
struct FxAssets {
    cube: Handle<Mesh>,
    ball: Handle<Mesh>,
    cyl: Handle<Mesh>,
    spark_hot: Handle<StandardMaterial>,
    spark_white: Handle<StandardMaterial>,
    spark_scale: Handle<StandardMaterial>,
    ember: Handle<StandardMaterial>,
    flame: Handle<StandardMaterial>,
    droplet: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    clay: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    clang: [Handle<Synth>; 4],
    thud: Handle<Synth>,
    sizzle: [Handle<Synth>; 2],
    sizzle_short: Handle<Synth>,
    oil: Handle<Synth>,
    grind: [Handle<Synth>; 2],
    crack: Handle<Synth>,
    pour: Handle<Synth>,
    bellows: Handle<Synth>,
}

#[derive(Resource)]
struct FxRng(Noise);

impl Default for FxRng {
    fn default() -> Self {
        Self(Noise(0x9E37_79B9))
    }
}

impl FxRng {
    fn f(&mut self) -> f32 {
        self.0.next() * 0.5 + 0.5
    }
    fn s(&mut self) -> f32 {
        self.0.next()
    }
    fn dir(&mut self) -> Vec3 {
        Vec3::new(self.s(), self.s(), self.s()).normalize_or(Vec3::Y)
    }
}

fn glow(
    materials: &mut Assets<StandardMaterial>,
    c: [f32; 3],
    k: f32,
    alpha: AlphaMode,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgb(c[0], c[1], c[2]),
        emissive: LinearRgba::rgb(c[0], c[1], c[2]) * k,
        unlit: true,
        alpha_mode: alpha,
        ..default()
    })
}

fn setup_fx(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut synths: ResMut<Assets<Synth>>,
) {
    let solid = |c: Color, metal: f32, rough: f32| StandardMaterial {
        base_color: c,
        metallic: metal,
        perceptual_roughness: rough,
        ..default()
    };
    commands.insert_resource(FxAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap()),
        cyl: meshes.add(Cylinder::new(0.5, 1.0)),
        spark_hot: glow(&mut materials, [1.0, 0.62, 0.18], 14.0, AlphaMode::Opaque),
        spark_white: glow(&mut materials, [1.0, 0.9, 0.62], 22.0, AlphaMode::Opaque),
        spark_scale: glow(&mut materials, [0.9, 0.32, 0.08], 6.0, AlphaMode::Opaque),
        ember: glow(&mut materials, [1.0, 0.42, 0.1], 9.0, AlphaMode::Opaque),
        flame: glow(&mut materials, [1.0, 0.55, 0.12], 6.0, AlphaMode::Add),
        droplet: materials.add(StandardMaterial {
            base_color: Color::srgba(0.75, 0.82, 0.85, 0.7),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.1,
            ..default()
        }),
        iron: materials.add(solid(Color::srgb(0.13, 0.12, 0.12), 0.8, 0.55)),
        wood: materials.add(solid(Color::srgb(0.36, 0.24, 0.14), 0.0, 0.85)),
        clay: materials.add(solid(Color::srgb(0.48, 0.32, 0.22), 0.0, 0.95)),
        stone: materials.add(solid(Color::srgb(0.42, 0.4, 0.37), 0.0, 0.95)),
        clang: [
            synths.add(synth_clang(820.0, 0.9, 0.0, 11)),
            synths.add(synth_clang(760.0, 0.7, 0.0, 12)),
            synths.add(synth_clang(560.0, 0.5, 0.6, 13)),
            synths.add(synth_clang(500.0, 0.4, 0.8, 14)),
        ],
        thud: synths.add(synth_clang(380.0, 0.25, 1.0, 15)),
        sizzle: [
            synths.add(synth_sizzle(2.4, false, 21)),
            synths.add(synth_sizzle(2.0, false, 22)),
        ],
        sizzle_short: synths.add(synth_sizzle(0.7, false, 23)),
        oil: synths.add(synth_sizzle(2.2, true, 24)),
        grind: [synths.add(synth_grind(31)), synths.add(synth_grind(32))],
        crack: synths.add(synth_crack(41)),
        pour: synths.add(synth_pour(51)),
        bellows: synths.add(synth_roar(0.9, 0.02, 61)),
    });
}

fn play(
    commands: &mut Commands,
    sound: &Handle<Synth>,
    at: Vec3,
    ear: Vec3,
    gain: f32,
    speed: f32,
) {
    let d = at.distance(ear);
    if d > FX_RANGE {
        return;
    }
    // Громкость по расстоянию, панораму и каналы громкости ведёт `crate::audio`.
    let v = gain / (1.0 + d * 0.25);
    let emitter = crate::audio::Emitter {
        at: Some(at),
        follow: None,
        gain,
        near: 4.0,
        range: FX_RANGE,
        channel: crate::audio::Channel::Sfx,
        fade: 1.0,
    };
    commands.spawn((
        AudioPlayer(sound.clone()),
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(v))
            .with_speed(speed)
            .with_spatial(true),
        Transform::from_translation(ear + (at - ear).normalize_or(Vec3::NEG_Z)),
        emitter,
    ));
}

// ---------------------------------------------------------------------------
// Частицы
// ---------------------------------------------------------------------------

#[derive(Component)]
struct Particle {
    vel: Vec3,
    age: f32,
    life: f32,
    gravity: f32,
    drag: f32,
    /// Размер в начале и в конце жизни.
    size: (f32, f32),
    /// Своя прозрачность (пар и дым тают), иначе — общий материал.
    fade: Option<(Handle<StandardMaterial>, f32)>,
    /// Искра вытягивается по скорости.
    streak: bool,
    /// Упала на пол — гаснет и не скачет.
    floor: f32,
}

impl Particle {
    fn new(vel: Vec3, life: f32, size: f32) -> Self {
        Self {
            vel,
            age: 0.0,
            life,
            gravity: 9.8,
            drag: 0.4,
            size: (size, size * 0.3),
            fade: None,
            streak: true,
            floor: -1000.0,
        }
    }
}

#[derive(Default)]
struct Spawn {
    list: Vec<(Particle, Handle<Mesh>, Handle<StandardMaterial>, Vec3)>,
}

impl Spawn {
    fn push(&mut self, p: Particle, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, at: Vec3) {
        self.list.push((p, mesh.clone(), mat.clone(), at));
    }
    fn flush(self, commands: &mut Commands, alive: usize) {
        let room = MAX_PARTICLES.saturating_sub(alive);
        for (p, mesh, mat, at) in self.list.into_iter().take(room) {
            let s = p.size.0;
            commands.spawn((
                Transform::from_translation(at).with_scale(Vec3::splat(s)),
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                NotShadowCaster,
                p,
            ));
        }
    }
}

/// Пар или дым: тающий шар со своим материалом.
fn puff(
    materials: &mut Assets<StandardMaterial>,
    c: [f32; 3],
    alpha: f32,
) -> (Handle<StandardMaterial>, f32) {
    let h = materials.add(StandardMaterial {
        base_color: Color::srgba(c[0], c[1], c[2], alpha),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    (h, alpha)
}

fn sparks(
    sp: &mut Spawn,
    a: &FxAssets,
    rng: &mut FxRng,
    at: Vec3,
    n: usize,
    speed: f32,
    up: f32,
    mat: &Handle<StandardMaterial>,
    floor: f32,
) {
    for _ in 0..n {
        let d = (rng.dir() + Vec3::Y * up).normalize_or(Vec3::Y);
        let mut p = Particle::new(
            d * speed * (0.4 + rng.f()),
            0.3 + rng.f() * 0.5,
            0.007 + rng.f() * 0.008,
        );
        p.floor = floor;
        sp.push(p, &a.cube, mat, at);
    }
}

fn steam(
    sp: &mut Spawn,
    a: &FxAssets,
    materials: &mut Assets<StandardMaterial>,
    rng: &mut FxRng,
    at: Vec3,
    n: usize,
    power: f32,
) {
    for _ in 0..n {
        let jitter = Vec3::new(rng.s() * 0.25, 0.0, rng.s() * 0.25);
        let vel = Vec3::new(rng.s() * 0.35, 0.9 + rng.f() * 1.4 * power, rng.s() * 0.35);
        let mut p = Particle::new(vel, 1.4 + rng.f() * 1.6, 0.12 + rng.f() * 0.1);
        p.gravity = -0.6;
        p.drag = 1.6;
        p.size = (p.size.0, 0.7 + rng.f() * 0.6 * power);
        p.streak = false;
        let g = 0.86 + rng.f() * 0.1;
        p.fade = Some(puff(
            materials,
            [g, g, g * 1.02],
            0.22 + 0.14 * power.min(1.0),
        ));
        let m = p.fade.as_ref().unwrap().0.clone();
        sp.push(p, &a.ball, &m, at + jitter);
    }
}

fn smoke(
    sp: &mut Spawn,
    a: &FxAssets,
    materials: &mut Assets<StandardMaterial>,
    rng: &mut FxRng,
    at: Vec3,
    n: usize,
    dark: f32,
    size: f32,
) {
    for _ in 0..n {
        let jitter = Vec3::new(rng.s() * 0.2, 0.0, rng.s() * 0.2);
        let vel = Vec3::new(rng.s() * 0.25 + 0.15, 0.6 + rng.f() * 0.6, rng.s() * 0.25);
        let mut p = Particle::new(vel, 3.0 + rng.f() * 2.5, 0.15 * size);
        p.gravity = -0.25;
        p.drag = 0.7;
        p.size = (0.15 * size, (0.9 + rng.f() * 0.8) * size);
        p.streak = false;
        let g = 0.32 - dark * 0.22 + rng.f() * 0.06;
        p.fade = Some(puff(materials, [g, g * 0.97, g * 0.94], 0.35));
        let m = p.fade.as_ref().unwrap().0.clone();
        sp.push(p, &a.ball, &m, at + jitter);
    }
}

fn particles(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.05);
    for (e, mut p, mut tf) in &mut q {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        let g = p.gravity;
        let drag = p.drag;
        p.vel.y -= g * dt;
        p.vel *= (1.0 - drag * dt).max(0.0);
        tf.translation += p.vel * dt;
        if tf.translation.y < p.floor {
            tf.translation.y = p.floor;
            p.vel = Vec3::ZERO;
            // Упавшая окалина тлеет недолго.
            p.life = p.life.min(p.age + 0.4);
        }
        let k = p.age / p.life;
        let s = p.size.0 + (p.size.1 - p.size.0) * k;
        if p.streak && p.vel.length_squared() > 0.5 {
            let len = (p.vel.length() * 0.6).clamp(1.0, 5.0);
            tf.rotation = Quat::from_rotation_arc(Vec3::Y, p.vel.normalize());
            tf.scale = Vec3::new(s * 0.6, s * len, s * 0.6);
        } else {
            tf.scale = Vec3::splat(s);
        }
        if let Some((h, a0)) = &p.fade
            && let Some(mut m) = materials.get_mut(h)
        {
            let a = a0 * (1.0 - k).powf(1.3) * (k * 6.0).min(1.0);
            m.base_color.set_alpha(a);
        }
    }
}

// ---------------------------------------------------------------------------
// События
// ---------------------------------------------------------------------------

/// Опускание в корыто: поковка игрока уходит в воду клинком вниз.
#[derive(Clone, Copy)]
struct Dip {
    t: f32,
    medium: Quench,
    temp: f32,
}

#[derive(Resource, Default)]
struct DipState(HashMap<u64, Dip>);

/// Молот над наковальней (летит вниз и пропадает).
#[derive(Component)]
struct Hammer {
    t: f32,
    heavy: bool,
    at: Vec3,
    side: Vec3,
}

/// Откуда слушаем и смотрим: активная камера (своя голова или камера ожидания).
fn ear(cams: &Query<(&GlobalTransform, &Camera), With<Camera3d>>) -> Vec3 {
    cams.iter()
        .find(|(_, c)| c.is_active)
        .map(|(t, _)| t.translation())
        .unwrap_or(Vec3::splat(1.0e6))
}

/// Направления у горна: `along` — от наковальни к горну, `side` — поперёк.
fn axes(st: &StationView) -> (Vec3, Vec3) {
    let d = Vec3::from(st.fire) - Vec3::from(st.anvil);
    let along = Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::X);
    (along, Vec3::new(-along.z, 0.0, along.x))
}

fn anvil_top(st: &StationView) -> Vec3 {
    Vec3::from(st.anvil) + Vec3::Y * 0.84
}

#[allow(clippy::too_many_arguments)]
fn play_fx(
    mut commands: Commands,
    mut state: ResMut<SmithState>,
    a: Res<FxAssets>,
    mut rng: ResMut<FxRng>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut dips: Local<DipState>,
    player: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    alive: Query<(), With<Particle>>,
    time: Res<Time>,
    mut out_dips: ResMut<DipsOut>,
) {
    let ear = ear(&player);
    let mut sp = Spawn::default();
    let queue = std::mem::take(&mut state.fx_queue);
    for (sid, pid, fx) in queue {
        let Some(st) = state.station(sid).cloned() else {
            continue;
        };
        let top = anvil_top(&st);
        if top.distance(ear) > FX_RANGE * 1.5 {
            continue;
        }
        let (along, side) = axes(&st);
        let floor = Vec3::from(st.anvil).y + 0.02;
        let seed = (time.elapsed_secs() * 1000.0) as usize;
        match fx {
            Fx::Strike { heavy, temp } => {
                commands
                    .spawn((
                        Hammer {
                            t: 0.0,
                            heavy,
                            at: top + Vec3::Y * 0.04,
                            side,
                        },
                        Transform::from_translation(top),
                        Visibility::default(),
                    ))
                    .with_children(|c| {
                        // Рукоять и боёк.
                        c.spawn((
                            Mesh3d(a.cube.clone()),
                            MeshMaterial3d(a.wood.clone()),
                            Transform::from_xyz(0.0, 0.0, 0.2)
                                .with_scale(Vec3::new(0.035, 0.035, 0.42)),
                        ));
                        let head = if heavy {
                            Vec3::new(0.085, 0.085, 0.2)
                        } else {
                            Vec3::new(0.06, 0.06, 0.13)
                        };
                        c.spawn((
                            Mesh3d(a.cube.clone()),
                            MeshMaterial3d(a.iron.clone()),
                            Transform::from_xyz(0.0, 0.0, 0.0)
                                .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
                                .with_scale(head),
                        ));
                    });
                let hot = ((temp - 650.0) / 500.0).clamp(0.0, 1.0);
                if temp > 600.0 {
                    // Окалина брызжет с раскалённого железа.
                    let n = (4.0 + hot * 16.0 * if heavy { 1.6 } else { 1.0 }) as usize;
                    sparks(
                        &mut sp,
                        &a,
                        &mut rng,
                        top + Vec3::Y * 0.05,
                        n,
                        3.2 + hot * 2.0,
                        0.6,
                        if hot > 0.6 {
                            &a.spark_white
                        } else {
                            &a.spark_scale
                        },
                        floor,
                    );
                    sparks(
                        &mut sp,
                        &a,
                        &mut rng,
                        top + Vec3::Y * 0.05,
                        n / 2,
                        1.8,
                        0.2,
                        &a.spark_hot,
                        floor,
                    );
                } else if temp < 100.0 {
                    sparks(
                        &mut sp,
                        &a,
                        &mut rng,
                        top + Vec3::Y * 0.05,
                        2,
                        2.0,
                        0.5,
                        &a.spark_white,
                        floor,
                    );
                }
                let i = if temp > 600.0 { 2 + seed % 2 } else { seed % 2 };
                let pitch = 0.94 + rng.f() * 0.12 - if heavy { 0.12 } else { 0.0 };
                let snd = if heavy && temp > 950.0 {
                    &a.thud
                } else {
                    &a.clang[i]
                };
                play(
                    &mut commands,
                    snd,
                    top,
                    ear,
                    if heavy { 1.0 } else { 0.75 },
                    pitch,
                );
            }
            Fx::Quench {
                medium,
                temp,
                cracked,
            } => {
                dips.0.insert(
                    pid,
                    Dip {
                        t: 0.0,
                        medium,
                        temp,
                    },
                );
                let water = Vec3::from(st.trough);
                let hot = ((temp - 300.0) / 600.0).clamp(0.0, 1.0);
                if medium == Quench::Oil {
                    play(&mut commands, &a.oil, water, ear, 0.9, 1.0);
                    play(&mut commands, &a.bellows, water, ear, 0.6 * hot, 0.7);
                } else if hot > 0.05 {
                    play(
                        &mut commands,
                        &a.sizzle[seed % 2],
                        water,
                        ear,
                        0.5 + hot * 0.6,
                        0.95 + rng.f() * 0.1,
                    );
                } else {
                    play(&mut commands, &a.sizzle_short, water, ear, 0.25, 1.2);
                }
                if cracked {
                    play(&mut commands, &a.crack, water, ear, 0.9, 1.0);
                }
                // Брызги при погружении.
                for _ in 0..14 {
                    let v = Vec3::new(rng.s() * 1.2, 1.6 + rng.f() * 1.6, rng.s() * 1.2);
                    let mut p = Particle::new(v, 0.6, 0.02 + rng.f() * 0.015);
                    p.floor = water.y - 0.05;
                    p.streak = false;
                    sp.push(p, &a.ball, &a.droplet, water + Vec3::Y * 0.02);
                }
            }
            Fx::Grind => {
                let g = Vec3::from(st.grind);
                // Круг крутится от себя — искры летят веером вниз и вбок.
                let r = 0.38;
                let contact = g + Vec3::Y * r * 0.7 - along * r * 0.7;
                for _ in 0..22 {
                    let d = (-along * 1.0 - Vec3::Y * (0.3 + rng.f() * 0.5)
                        + side * rng.s() * 0.35)
                        .normalize();
                    let mut p = Particle::new(
                        d * (3.0 + rng.f() * 3.0),
                        0.25 + rng.f() * 0.45,
                        0.01 + rng.f() * 0.008,
                    );
                    p.floor = floor;
                    p.drag = 0.8;
                    sp.push(
                        p,
                        &a.cube,
                        if rng.f() > 0.5 {
                            &a.spark_white
                        } else {
                            &a.spark_hot
                        },
                        contact,
                    );
                }
                play(
                    &mut commands,
                    &a.grind[seed % 2],
                    g,
                    ear,
                    0.7,
                    0.9 + rng.f() * 0.2,
                );
            }
            Fx::Burn => {
                let f = Vec3::from(st.fire);
                for _ in 0..28 {
                    let d = (Vec3::Y * 1.4 + rng.dir() * 0.6).normalize();
                    let mut p = Particle::new(
                        d * (2.0 + rng.f() * 3.0),
                        0.5 + rng.f() * 0.9,
                        0.012 + rng.f() * 0.01,
                    );
                    p.gravity = 2.0;
                    sp.push(p, &a.cube, &a.spark_white, f + Vec3::Y * 0.1);
                }
                play(&mut commands, &a.sizzle_short, f, ear, 0.35, 1.6);
            }
            Fx::Crack => {
                play(&mut commands, &a.crack, top, ear, 1.0, 0.9 + rng.f() * 0.2);
                sparks(
                    &mut sp,
                    &a,
                    &mut rng,
                    top,
                    6,
                    1.5,
                    0.3,
                    &a.spark_scale,
                    floor,
                );
            }
            Fx::Pour(true) => {
                play(&mut commands, &a.pour, top, ear, 0.8, 1.0);
            }
            Fx::Pour(false) => {}
            Fx::Skim => {
                let f = Vec3::from(st.fire);
                smoke(
                    &mut sp,
                    &a,
                    &mut materials,
                    &mut rng,
                    f + Vec3::Y * 0.2,
                    4,
                    0.6,
                    0.5,
                );
                sparks(
                    &mut sp,
                    &a,
                    &mut rng,
                    f + Vec3::Y * 0.15,
                    8,
                    1.2,
                    1.0,
                    &a.spark_hot,
                    floor,
                );
            }
            Fx::OpenMould => {
                play(&mut commands, &a.crack, top, ear, 0.6, 0.6);
                for _ in 0..16 {
                    let mut p =
                        Particle::new(rng.dir() * 1.5 + Vec3::Y * 1.2, 0.9, 0.03 + rng.f() * 0.03);
                    p.floor = floor;
                    p.streak = false;
                    sp.push(p, &a.cube, &a.clay, top - Vec3::Y * 0.3);
                }
                smoke(
                    &mut sp,
                    &a,
                    &mut materials,
                    &mut rng,
                    top - Vec3::Y * 0.3,
                    5,
                    0.3,
                    0.6,
                );
            }
            Fx::Done => {
                play(&mut commands, &a.clang[0], top, ear, 0.5, 1.25);
            }
        }
    }
    // Пар идёт, пока клинок в воде; масло горит и чадит.
    let dt = time.delta_secs().min(0.05);
    let stations = state.stations.clone();
    let station_of: HashMap<u64, u32> = state
        .all_pieces()
        .iter()
        .map(|p| (p.player, p.station))
        .collect();
    dips.0.retain(|pid, d| {
        d.t += dt;
        let Some(st) = station_of
            .get(pid)
            .and_then(|s| stations.iter().find(|x| x.id == *s))
        else {
            return d.t < 2.0;
        };
        let water = Vec3::from(st.trough);
        let hot = ((d.temp - 250.0) / 650.0).clamp(0.0, 1.0) * (1.0 - d.t / 2.4).max(0.0);
        if hot > 0.0 && d.t > 0.25 {
            let n = (hot * 55.0 * dt).ceil() as usize;
            match d.medium {
                Quench::Oil => {
                    for _ in 0..n {
                        let mut p = Particle::new(
                            Vec3::new(rng.s() * 0.2, 0.8 + rng.f() * 1.2, rng.s() * 0.2),
                            0.35 + rng.f() * 0.4,
                            0.08 + rng.f() * 0.08,
                        );
                        p.gravity = -2.0;
                        p.streak = false;
                        p.size = (p.size.0, 0.02);
                        sp.push(
                            p,
                            &a.ball,
                            &a.flame,
                            water + Vec3::new(rng.s() * 0.2, 0.05, rng.s() * 0.2),
                        );
                    }
                    smoke(
                        &mut sp,
                        &a,
                        &mut materials,
                        &mut rng,
                        water + Vec3::Y * 0.3,
                        n.div_ceil(2),
                        1.0,
                        0.7,
                    );
                }
                _ => {
                    steam(
                        &mut sp,
                        &a,
                        &mut materials,
                        &mut rng,
                        water + Vec3::Y * 0.05,
                        n,
                        hot * 1.3,
                    );
                    // Пузыри лопаются у клинка.
                    if rng.f() < hot {
                        let mut p = Particle::new(Vec3::Y * 0.3, 0.25, 0.03 + rng.f() * 0.03);
                        p.gravity = 0.0;
                        p.streak = false;
                        p.size = (0.02, p.size.0 * 1.5);
                        sp.push(
                            p,
                            &a.ball,
                            &a.droplet,
                            water + Vec3::new(rng.s() * 0.12, 0.0, rng.s() * 0.12),
                        );
                    }
                }
            }
        }
        d.t < 2.6
    });
    out_dips.0 = dips.0.iter().map(|(k, d)| (*k, d.t)).collect();
    sp.flush(&mut commands, alive.iter().count());
}

/// Сколько секунд назад игрок опустил поковку в корыто (для позы клещей).
#[derive(Resource, Default)]
pub struct DipsOut(HashMap<u64, f32>);

fn hammers(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Hammer, &mut Transform)>,
) {
    for (e, mut h, mut tf) in &mut q {
        h.t += time.delta_secs();
        let up = if h.heavy { 0.55 } else { 0.35 };
        // Замах вверх, падение, отскок.
        let t = h.t;
        let (lift, tilt) = if t < 0.08 {
            let k = t / 0.08;
            (up * (1.0 - k * k), -0.9 * (1.0 - k))
        } else {
            let k = ((t - 0.08) / 0.25).min(1.0);
            (0.18 * (k * PI).sin(), 0.35 * (k * PI).sin())
        };
        let yaw = Quat::from_rotation_arc(Vec3::Z, h.side);
        tf.translation = h.at + Vec3::Y * (lift + 0.05) - h.side * 0.22;
        tf.rotation = yaw * Quat::from_rotation_x(tilt);
        if h.t > 0.4 {
            commands.entity(e).despawn();
        }
    }
}

// ---------------------------------------------------------------------------
// Горн: свет, угли, дым
// ---------------------------------------------------------------------------

#[derive(Component)]
struct FireFx {
    station: u32,
    kind: StationKind,
    phase: f32,
    acc: f32,
    smoke_acc: f32,
    /// Свой материал углей: разгорается от мехов.
    coal: Handle<StandardMaterial>,
}

fn forge_fires(
    mut commands: Commands,
    state: Res<SmithState>,
    a: Res<FxAssets>,
    time: Res<Time>,
    mut rng: ResMut<FxRng>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fires: Query<(&mut FireFx, &mut PointLight, &Transform)>,
    player: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    alive: Query<(), With<Particle>>,
    mut known: Local<usize>,
) {
    if *known != state.stations.len() {
        *known = state.stations.len();
        for st in &state.stations {
            let coal = materials.add(StandardMaterial {
                base_color: Color::srgb(0.12, 0.08, 0.06),
                perceptual_roughness: 0.95,
                ..default()
            });
            let fire = Vec3::from(st.fire);
            commands
                .spawn((
                    FireFx {
                        station: st.id,
                        kind: st.kind,
                        phase: st.id as f32 * 1.7,
                        acc: 0.0,
                        smoke_acc: 0.0,
                        coal: coal.clone(),
                    },
                    PointLight {
                        color: Color::srgb(1.0, 0.5, 0.18),
                        intensity: 0.0,
                        range: 9.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(fire + Vec3::Y * 0.35),
                    Visibility::default(),
                ))
                .with_children(|c| {
                    // Куча углей: тлеют и разгораются, когда качают мехи.
                    let r = if st.kind == StationKind::Bloomery {
                        0.2
                    } else {
                        0.58
                    };
                    let mut n = Noise(st.id * 7919 + 1);
                    for _ in 0..if st.kind == StationKind::Bloomery {
                        10
                    } else {
                        44
                    } {
                        let (ang, d) = (
                            (n.next() * 0.5 + 0.5) * TAU,
                            (n.next() * 0.5 + 0.5).sqrt() * r,
                        );
                        let s = 0.05 + (n.next() * 0.5 + 0.5) * 0.05;
                        let p = Vec3::new(ang.cos() * d, -0.35 + s * 0.3, ang.sin() * d);
                        c.spawn((
                            Mesh3d(a.cube.clone()),
                            MeshMaterial3d(coal.clone()),
                            Transform::from_translation(p)
                                .with_rotation(Quat::from_euler(
                                    EulerRot::XYZ,
                                    n.next(),
                                    n.next() * 3.0,
                                    n.next(),
                                ))
                                .with_scale(Vec3::splat(s)),
                            NotShadowCaster,
                        ));
                    }
                });
        }
    }
    let ear = ear(&player);
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();
    let mut sp = Spawn::default();
    for (mut f, mut light, tf) in &mut fires {
        let (fuel, blast) = state.fire(f.station).unwrap_or((0.0, 0.0));
        let lit = fuel > 0.0;
        let flicker =
            0.85 + 0.1 * (t * 13.0 + f.phase).sin() + 0.05 * (t * 31.0 + f.phase * 2.0).sin();
        let heat = if lit { 0.35 + blast * 0.9 } else { 0.0 };
        if let Some(mut m) = materials.get_mut(&f.coal) {
            m.emissive = LinearRgba::rgb(1.0, 0.32 + blast * 0.3, 0.06 + blast * 0.15)
                * (heat * heat * 5.0 * flicker);
        }
        light.intensity = 60_000.0 * heat * flicker;
        light.color = Color::srgb(1.0, 0.42 + blast * 0.25, 0.14 + blast * 0.2);
        if !lit || tf.translation.distance(ear) > FX_RANGE {
            continue;
        }
        let pos = tf.translation - Vec3::Y * 0.3;
        // Искорки углей летят вверх — гуще, когда качают мехи.
        f.acc += dt * (1.5 + blast * 30.0);
        while f.acc >= 1.0 {
            f.acc -= 1.0;
            let mut p = Particle::new(
                Vec3::new(
                    rng.s() * 0.4,
                    1.0 + rng.f() * 2.0 * (0.5 + blast),
                    rng.s() * 0.4,
                ),
                0.8 + rng.f() * 1.2,
                0.01 + rng.f() * 0.008,
            );
            p.gravity = -0.4;
            p.drag = 0.6;
            p.streak = false;
            sp.push(
                p,
                &a.cube,
                &a.ember,
                pos + Vec3::new(rng.s() * 0.3, 0.0, rng.s() * 0.3),
            );
        }
        // Дым: из шахты сыродутного горна валит всегда, у кузнечного — немного.
        let rate = if f.kind == StationKind::Bloomery {
            2.5 + blast * 2.0
        } else {
            0.4 + blast * 0.8
        };
        f.smoke_acc += dt * rate;
        if f.smoke_acc >= 1.0 {
            f.smoke_acc -= 1.0;
            let size = if f.kind == StationKind::Bloomery {
                0.9
            } else {
                0.6
            };
            smoke(
                &mut sp,
                &a,
                &mut materials,
                &mut rng,
                pos + Vec3::Y * 0.25,
                1,
                0.4,
                size,
            );
            // Над сыродутным горном пляшут языки пламени.
            if f.kind == StationKind::Bloomery {
                for _ in 0..3 {
                    let mut p = Particle::new(
                        Vec3::new(rng.s() * 0.1, 1.0 + blast * 1.5, rng.s() * 0.1),
                        0.3 + rng.f() * 0.3,
                        0.12 + rng.f() * 0.08,
                    );
                    p.gravity = -1.0;
                    p.streak = false;
                    p.size = (p.size.0, 0.02);
                    sp.push(p, &a.ball, &a.flame, pos + Vec3::Y * 0.1);
                }
            }
        }
    }
    sp.flush(&mut commands, alive.iter().count());
}

// ---------------------------------------------------------------------------
// Поковка в клещах
// ---------------------------------------------------------------------------

#[derive(Component)]
struct PieceFx {
    player: u64,
    seen: bool,
}

#[derive(Component)]
struct PieceBody;

#[derive(Component)]
struct PieceStream;

#[derive(Component)]
struct PieceMould;

/// Размер поковки: (длина, ширина, толщина) в начале и в конце.
fn piece_dims(p: Option<Pattern>, drawn: f32) -> Vec3 {
    let (start, end) = match p {
        None => return Vec3::new(0.14, 0.16, 0.14),
        Some(Pattern::Bloom) => (Vec3::new(0.16, 0.12, 0.12), Vec3::new(0.3, 0.05, 0.04)),
        Some(Pattern::ScrapWeld) => (Vec3::new(0.22, 0.05, 0.05), Vec3::new(0.4, 0.04, 0.02)),
        Some(Pattern::Knife) => (Vec3::new(0.12, 0.02, 0.012), Vec3::new(0.22, 0.03, 0.004)),
        Some(Pattern::Spatha | Pattern::CavalrySpatha) => {
            (Vec3::new(0.32, 0.03, 0.02), Vec3::new(0.82, 0.05, 0.006))
        }
        Some(Pattern::Axe) => (Vec3::new(0.14, 0.05, 0.03), Vec3::new(0.18, 0.12, 0.015)),
        Some(Pattern::Spear) => (Vec3::new(0.2, 0.03, 0.02), Vec3::new(0.38, 0.05, 0.008)),
        Some(Pattern::Mace) => (Vec3::new(0.1, 0.06, 0.06), Vec3::new(0.12, 0.08, 0.08)),
        Some(Pattern::Arrowheads) => (Vec3::new(0.18, 0.012, 0.012), Vec3::new(0.24, 0.02, 0.006)),
        Some(Pattern::Mail) => (Vec3::new(0.25, 0.012, 0.012), Vec3::new(0.35, 0.006, 0.006)),
        Some(Pattern::Arms(i)) => {
            use crate::arsenal::Fam;
            let a = &crate::arsenal::ARMS[(i as usize).min(crate::arsenal::ARMS.len() - 1)];
            let len = a.mass * 0.6;
            match a.fam {
                Fam::Sword => (
                    Vec3::new(len * 0.4, 0.03, 0.02),
                    Vec3::new(len, 0.05, 0.006),
                ),
                Fam::Seax => (
                    Vec3::new(0.12, 0.025, 0.015),
                    Vec3::new(0.25 + len * 0.2, 0.04, 0.006),
                ),
                Fam::Axe => (
                    Vec3::new(0.14, 0.05, 0.03),
                    Vec3::new(0.18, 0.12 + a.mass * 0.03, 0.015),
                ),
                Fam::Mace => (Vec3::new(0.1, 0.06, 0.06), Vec3::new(0.12, 0.08, 0.08)),
                Fam::Spear => (Vec3::new(0.18, 0.03, 0.02), Vec3::new(0.32, 0.045, 0.008)),
                Fam::Head => (Vec3::new(0.18, 0.012, 0.012), Vec3::new(0.24, 0.02, 0.006)),
            }
        }
        // Пластины доспеха, шлема, умбон щита.
        Some(_) => (Vec3::new(0.18, 0.12, 0.02), Vec3::new(0.24, 0.18, 0.004)),
    };
    start.lerp(end, drawn.clamp(0.0, 1.0))
}

fn piece_pose(
    st: &StationView,
    v: &PieceView,
    dims: Vec3,
    dip: Option<f32>,
    t: f32,
) -> (Vec3, Quat) {
    let (along, side) = axes(st);
    let fire = Vec3::from(st.fire);
    let flat = Quat::from_mat3(&Mat3::from_cols(side, Vec3::Y, -side.cross(Vec3::Y)));
    if let Some(dt) = dip
        && dt < 2.2
    {
        // Клинок вниз, в воду, и чуть покачать — так закаляют ровно.
        let water = Vec3::from(st.trough);
        let k = (dt / 0.3).min(1.0);
        let sway = (t * 7.0).sin() * 0.05 * (1.0 - dt / 2.2);
        let down = water + Vec3::Y * (0.35 - k * (0.35 + dims.x * 0.35)) + side * sway;
        let rot = Quat::from_rotation_arc(Vec3::X, -Vec3::Y) * Quat::from_rotation_x(sway);
        return (down, rot);
    }
    match v.spot {
        Spot::Fire => {
            let p = fire + along.cross(Vec3::Y) * 0.0 - along * (dims.x * 0.25) - Vec3::Y * 0.02;
            (
                p,
                Quat::from_rotation_arc(Vec3::X, -along) * Quat::from_rotation_x(0.2),
            )
        }
        Spot::Temper => (
            fire - along * (dims.x * 0.6 + 0.2) + Vec3::Y * 0.12,
            Quat::from_rotation_arc(Vec3::X, -along),
        ),
        Spot::Ashes => (
            Vec3::new(fire.x, Vec3::from(st.anvil).y + 0.04, fire.z) - along * 1.0 + side * 0.9,
            Quat::from_rotation_arc(Vec3::X, side),
        ),
        Spot::Grindstone => {
            let g = Vec3::from(st.grind);
            (
                g + Vec3::Y * 0.4 - along * 0.15,
                Quat::from_rotation_arc(Vec3::X, side) * Quat::from_rotation_x(0.3),
            )
        }
        Spot::Anvil => {
            let _ = flat;
            (
                anvil_top(st) + Vec3::Y * (dims.z * 0.5 + 0.005),
                Quat::from_rotation_arc(Vec3::X, side),
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn pieces(
    mut commands: Commands,
    state: Res<SmithState>,
    a: Res<FxAssets>,
    dips: Res<DipsOut>,
    time: Res<Time>,
    mut rng: ResMut<FxRng>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(
        Entity,
        &mut PieceFx,
        &mut Transform,
        &Children,
        &mut PointLight,
    )>,
    mut bodies: Query<
        (&mut Transform, &MeshMaterial3d<StandardMaterial>),
        (With<PieceBody>, Without<PieceFx>),
    >,
    mut streams: Query<
        (&mut Transform, &mut Visibility),
        (With<PieceStream>, Without<PieceFx>, Without<PieceBody>),
    >,
    player: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    alive: Query<(), With<Particle>>,
) {
    let list = state.all_pieces();
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();
    let ear = ear(&player);
    let mut sp = Spawn::default();
    let mut have: HashMap<u64, Entity> = HashMap::new();
    for (e, mut fx, _, _, _) in &mut q {
        fx.seen = false;
        have.insert(fx.player, e);
    }
    for v in &list {
        let Some(st) = state.station(v.station) else {
            continue;
        };
        if Vec3::from(st.anvil).distance(ear) > FX_RANGE * 2.0 {
            continue;
        }
        let dims = piece_dims(v.pattern, v.drawn);
        let (pos, rot) = piece_pose(st, v, dims, dips.0.get(&v.player).copied(), t);
        let Some(&e) = have.get(&v.player) else {
            spawn_piece(&mut commands, &a, &mut materials, v, pos, rot);
            continue;
        };
        let Ok((_, mut fx, mut tf, children, mut light)) = q.get_mut(e) else {
            continue;
        };
        fx.seen = true;
        // Клещи несут поковку, а не телепортируют её.
        let k = 1.0 - (-dt * 9.0).exp();
        tf.translation = tf.translation.lerp(pos, k);
        tf.rotation = tf.rotation.slerp(rot, k);
        let (c, _) = heat_color(v.temp);
        let g = ((v.temp - 480.0) / 800.0).clamp(0.0, 1.0);
        light.intensity = 9_000.0 * g * g * (0.9 + 0.1 * (t * 17.0).sin());
        light.color = Color::srgb(c[0].max(0.3), c[1].max(0.1), c[2].max(0.05));
        for ch in children.iter() {
            if let Ok((mut btf, mat)) = bodies.get_mut(ch) {
                if v.pattern.is_some() {
                    btf.scale = dims;
                    btf.translation = Vec3::new(dims.x * 0.5, 0.0, 0.0);
                }
                if let Some(mut m) = materials.get_mut(&mat.0) {
                    if v.pattern.is_some() {
                        m.base_color = Color::srgb(0.2 + c[0] * 0.3, 0.19 + c[1] * 0.2, 0.18);
                        m.emissive = LinearRgba::rgb(c[0], c[1], c[2]) * (g * g * 14.0);
                    } else {
                        // Расплав в тигле: светится зеркало металла.
                        let mg = ((v.temp - 700.0) / 500.0).clamp(0.0, 1.0);
                        m.base_color = Color::srgb(0.3 + c[0] * 0.5, 0.25 + c[1] * 0.4, 0.15);
                        m.emissive =
                            LinearRgba::rgb(c[0], c[1] * 0.9, c[2] * 0.6) * (mg * mg * 18.0);
                    }
                }
            }
            if let Ok((mut stf, mut vis)) = streams.get_mut(ch) {
                *vis = if v.pouring {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                stf.scale = Vec3::new(0.018 + (t * 40.0).sin().abs() * 0.006, 0.55, 0.018);
            }
        }
        // Остывая на наковальне, железо чуть дымит окалиной; расплав парит.
        if v.temp > 700.0 && rng.f() < dt * 3.0 {
            smoke(
                &mut sp,
                &a,
                &mut materials,
                &mut rng,
                tf.translation + Vec3::Y * 0.05,
                1,
                0.2,
                0.25,
            );
        }
        if v.pouring && rng.f() < dt * 25.0 {
            let mouth = anvil_top(st) - Vec3::Y * 0.25;
            sparks(
                &mut sp,
                &a,
                &mut rng,
                mouth,
                1,
                1.2,
                1.0,
                &a.spark_hot,
                Vec3::from(st.anvil).y + 0.02,
            );
            if rng.f() < 0.3 {
                smoke(&mut sp, &a, &mut materials, &mut rng, mouth, 1, 0.1, 0.35);
            }
        }
    }
    for (e, fx, _, _, _) in &q {
        if !fx.seen && !list.iter().any(|v| v.player == fx.player) {
            commands.entity(e).despawn();
        }
    }
    sp.flush(&mut commands, alive.iter().count());
}

fn spawn_piece(
    commands: &mut Commands,
    a: &FxAssets,
    materials: &mut Assets<StandardMaterial>,
    v: &PieceView,
    pos: Vec3,
    rot: Quat,
) {
    let body_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.24, 0.23),
        metallic: 0.6,
        perceptual_roughness: 0.55,
        ..default()
    });
    let dims = piece_dims(v.pattern, v.drawn);
    commands
        .spawn((
            PieceFx {
                player: v.player,
                seen: true,
            },
            Transform::from_translation(pos).with_rotation(rot),
            Visibility::default(),
            PointLight {
                color: Color::srgb(1.0, 0.45, 0.15),
                intensity: 0.0,
                range: 3.5,
                shadow_maps_enabled: false,
                ..default()
            },
        ))
        .with_children(|c| {
            if v.pattern.is_some() {
                // Поковка лежит вдоль +X от губок клещей.
                c.spawn((
                    PieceBody,
                    Mesh3d(a.cube.clone()),
                    MeshMaterial3d(body_mat),
                    Transform::from_xyz(dims.x * 0.5, 0.0, 0.0).with_scale(dims),
                ));
                // Клещи: две губки и рукояти, разведённые к руке.
                for s in [-1.0f32, 1.0] {
                    c.spawn((
                        Mesh3d(a.cube.clone()),
                        MeshMaterial3d(a.iron.clone()),
                        Transform::from_xyz(-0.28, 0.0, s * 0.018)
                            .with_rotation(Quat::from_rotation_y(s * 0.06))
                            .with_scale(Vec3::new(0.56, 0.014, 0.012)),
                    ));
                }
                c.spawn((
                    Mesh3d(a.cube.clone()),
                    MeshMaterial3d(a.iron.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.035, 0.03, 0.05)),
                ));
            } else {
                // Тигель из огнеупорной глины, металл светится в устье.
                c.spawn((
                    Mesh3d(a.cyl.clone()),
                    MeshMaterial3d(a.clay.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.13, 0.16, 0.13)),
                ));
                c.spawn((
                    PieceBody,
                    Mesh3d(a.cyl.clone()),
                    MeshMaterial3d(body_mat),
                    Transform::from_xyz(0.0, 0.081, 0.0).with_scale(Vec3::new(0.11, 0.004, 0.11)),
                ));
                // Струя в форму.
                c.spawn((
                    PieceStream,
                    Mesh3d(a.cyl.clone()),
                    MeshMaterial3d(a.spark_hot.clone()),
                    Transform::from_xyz(0.07, -0.2, 0.0).with_scale(Vec3::new(0.018, 0.55, 0.018)),
                    Visibility::Hidden,
                    NotShadowCaster,
                ));
                // Клещи-обхват тигля.
                c.spawn((
                    Mesh3d(a.cube.clone()),
                    MeshMaterial3d(a.iron.clone()),
                    Transform::from_xyz(-0.32, 0.02, 0.0).with_scale(Vec3::new(0.5, 0.016, 0.016)),
                ));
                c.spawn((
                    Mesh3d(a.cyl.clone()),
                    MeshMaterial3d(a.iron.clone()),
                    Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(0.145, 0.02, 0.145)),
                ));
            }
        });
    let _ = &a.stone;
}

/// Наклон тигля при заливке и форма у наковальни.
fn dips(
    mut commands: Commands,
    state: Res<SmithState>,
    a: Res<FxAssets>,
    mut q: Query<(&PieceFx, &mut Transform)>,
    moulds: Query<(Entity, &PieceMould)>,
    mut made: Local<Option<u32>>,
) {
    // Форма видна, пока своя или чужая отливка идёт у горна.
    let all = state.all_pieces();
    let casting: Vec<&PieceView> = all.iter().filter(|v| v.pattern.is_none()).collect();
    let want = casting.first().map(|v| v.station);
    if *made != want {
        for (e, _) in &moulds {
            commands.entity(e).despawn();
        }
        if let Some(sid) = want
            && let Some(st) = state.station(sid)
        {
            let (_, side) = axes(st);
            let at = anvil_top(st) - Vec3::Y * 0.55 + side * 0.0;
            commands.spawn((
                PieceMould,
                Mesh3d(a.cube.clone()),
                MeshMaterial3d(a.stone.clone()),
                Transform::from_translation(at).with_scale(Vec3::new(0.3, 0.12, 0.22)),
            ));
        }
        *made = want;
    }
    for (fx, mut tf) in &mut q {
        if let Some(v) = casting.iter().find(|v| v.player == fx.player)
            && v.pouring
            && let Some(st) = state.station(v.station)
        {
            let (_, side) = axes(st);
            tf.translation = anvil_top(st) - Vec3::Y * 0.05 - side * 0.05;
            tf.rotation = Quat::from_axis_angle(side.cross(Vec3::Y).normalize_or(Vec3::Z), 0.0)
                * Quat::from_rotation_arc(Vec3::X, side)
                * Quat::from_rotation_x(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_finite_and_loud_enough() {
        for s in [
            synth_clang(800.0, 0.9, 0.0, 1),
            synth_sizzle(2.0, false, 2),
            synth_sizzle(2.0, true, 3),
            synth_grind(4),
            synth_crack(5),
            synth_pour(6),
            synth_roar(0.9, 0.02, 7),
        ] {
            assert!(s.data.iter().all(|x| x.is_finite()));
            let peak = s.data.iter().fold(0.0f32, |a, x| a.max(x.abs()));
            assert!(peak > 0.3 && peak <= 1.0, "{peak}");
        }
    }

    #[test]
    fn blade_grows_as_it_is_drawn() {
        let a = piece_dims(Some(Pattern::Spatha), 0.0);
        let b = piece_dims(Some(Pattern::Spatha), 1.0);
        assert!(b.x > a.x * 2.0 && b.z < a.z);
    }
}

#[cfg(test)]
mod where_tests {
    #[test]
    #[ignore]
    fn print_forges() {
        for f in crate::level::forges() {
            println!(
                "{} fire {:?} anvil {:?} trough {:?} grind {:?}",
                f.name, f.fire, f.anvil, f.trough, f.grind
            );
        }
    }
}

/// `MRPG_SMITH_DEMO=<номер горна>[,фаза]`: кузнец-призрак куёт спату по кругу — для снимков эффектов.
/// Фаза (секунды цикла) крутит последние 2 с до неё: 2.5 — в горне, 5 — удары, 7.5 — пар.
fn demo(mut state: ResMut<SmithState>, time: Res<Time>, mut last: Local<f32>) {
    let arg = std::env::var("MRPG_SMITH_DEMO").unwrap_or_default();
    let mut it = arg.split(',');
    let sid: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(2);
    let hold: Option<f32> = it.next().and_then(|s| s.parse().ok());
    if state.station(sid).is_none() {
        state.demo(sid, None, 0.0);
        return;
    }
    let t = time.elapsed_secs();
    let c = hold
        .map(|h| h - 2.0 + (t * 0.5).fract() * 2.0)
        .unwrap_or(t % 10.0);
    let (spot, temp, blast) = match c {
        c if c < 3.0 => (Spot::Fire, 700.0 + c * 160.0, 1.0),
        c if c < 6.0 => (Spot::Anvil, 1180.0 - (c - 3.0) * 120.0, 0.3),
        c => (Spot::Anvil, (820.0 - (c - 6.0) * 400.0).max(60.0), 0.2),
    };
    let piece = PieceView {
        player: 777,
        station: sid,
        spot,
        temp,
        pattern: Some(Pattern::Spatha),
        drawn: 0.75,
        pouring: false,
    };
    state.demo(sid, Some(piece), blast);
    let prev = *last;
    *last = c;
    if (3.0..6.0).contains(&c) && (c * 2.0).floor() != (prev * 2.0).floor() {
        state.fx_queue.push((
            sid,
            777,
            Fx::Strike {
                heavy: (c * 2.0) as i32 % 3 == 0,
                temp,
            },
        ));
    }
    if c >= 6.0 && prev < 6.0 {
        state.fx_queue.push((
            sid,
            777,
            Fx::Quench {
                medium: Quench::Water,
                temp: 820.0,
                cracked: false,
            },
        ));
    }
}

/// `MRPG_SMITH_SHOWCASE=weapons|armour[,x,y,z]`: витрина для снимков — одни и те же вещи грубой,
/// средней и отличной работы. Ряды от камеры: грубая, средняя, отличная; по ширине — виды.
pub fn showcase(
    mut commands: Commands,
    models: Option<Res<crate::icons::ItemModels>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut done: Local<bool>,
) {
    let Some(models) = models else { return };
    if *done {
        return;
    }
    *done = true;
    let arg = std::env::var("MRPG_SMITH_SHOWCASE").unwrap_or_default();
    let mut it = arg.split(',');
    let mode = it.next().unwrap_or("weapons").trim().to_string();
    let v: Vec<f32> = it.filter_map(|s| s.trim().parse().ok()).collect();
    let o = if v.len() == 3 {
        Vec3::new(v[0], v[1], v[2])
    } else {
        Vec3::new(174.0, 0.0, -22.0)
    };
    let (kinds, dx, dz): (Vec<&str>, f32, f32) = if mode.starts_with("arm") {
        (
            vec![
                "Спангенхельм",
                "Гребенчатый шлем",
                "Кольчуга",
                "Ламеллярный панцирь",
                "Железные поножи",
            ],
            0.85,
            0.95,
        )
    } else if mode.starts_with("new") {
        (
            crate::arsenal::ARMS
                .iter()
                .filter(|a| a.fam == crate::arsenal::Fam::Sword)
                .map(|a| a.name)
                .take(12)
                .collect(),
            0.32,
            1.25,
        )
    } else if let Some(fam) = crate::arsenal::Fam::ALL
        .into_iter()
        .find(|f| format!("{f:?}").eq_ignore_ascii_case(&mode))
    {
        // Витрина одного семейства: всё в один ряд, хорошее качество.
        let dx = match fam {
            crate::arsenal::Fam::Axe | crate::arsenal::Fam::Mace => 0.34,
            crate::arsenal::Fam::Head => 0.16,
            _ => 0.26,
        };
        (
            crate::arsenal::ARMS
                .iter()
                .filter(|a| a.fam == fam)
                .map(|a| a.name)
                .collect(),
            dx,
            1.0,
        )
    } else if mode == "archery" {
        (
            vec![
                "Простой лук",
                "Гуннский составной лук",
                "Стрелы",
                "Бронебойные стрелы",
                "Трёхлопастные стрелы",
                "Стрелы-срезни",
                "Древки стрел",
                "Маховые перья",
                "Рыбий клей",
                "Сухожилия",
            ],
            0.3,
            1.0,
        )
    } else {
        (
            vec![
                "Спата",
                "Кавалерийская спата",
                "Кинжал",
                "Булава",
                "Боевой топор",
            ],
            0.42,
            1.3,
        )
    };
    let single = !(mode.starts_with("arm") || mode.starts_with("new") || mode.starts_with("weap"));
    let looks: &[u8] = if single { &[255] } else { &[12, 100, 255] };
    let w = dx * kinds.len() as f32 + 0.4;
    let d = dz * looks.len() as f32 + 0.2;
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(w, 0.04, d))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.14, 0.1),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_translation(
            o + Vec3::new(
                w * 0.5 - 0.2 - dx * 0.5 + dx * 0.5,
                0.02,
                d * 0.5 - 0.1 - dz * 0.5 + dz * 0.5,
            ),
        ),
    ));
    for (row, f) in looks.iter().enumerate() {
        for (i, name) in kinds.iter().enumerate() {
            let Some(base) = crate::items::ItemBaseId::find(name) else {
                continue;
            };
            let p = o + Vec3::new(i as f32 * dx, 0.05, row as f32 * dz);
            let mut e = commands.spawn((
                WorldAssetRoot(models.get(base)),
                Transform::from_translation(p),
            ));
            if *f < 160 {
                e.insert(crate::rig::LookTint(*f));
            }
        }
    }
}

#[cfg(test)]
mod where_agora {
    #[test]
    #[ignore]
    fn print_agora() {
        println!("AGORA {:?}", crate::level::place_pos("agora"));
    }
}
