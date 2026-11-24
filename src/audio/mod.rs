//! Звук: шаги, бой, голоса, предметы, окружение и музыка.
//!
//! Все звуки синтезированы для игры (`tools/audio/build.py`), лежат в `assets/audio`.
//! Здесь — общий механизм:
//! * [`Sfx`] — сообщение «сыграть звук» (с позицией в мире или без неё, например интерфейс);
//! * свой расчёт громкости по расстоянию (закон 1/r с ближней зоной) и стереопанорама
//!   через пространственный звук Bevy: источник ставим на единичном расстоянии в нужном
//!   направлении, а громкость задаём сами — встроенное затухание 1/r² слишком резкое;
//! * каналы громкости: общая (`GlobalVolume`, её меняют настройки), музыка, эффекты, окружение.
//!
//! Звуки других игроков не передаются отдельно: каждый клиент сам слышит их шаги, удары,
//! стрелы и двери по тем же снимкам и событиям, по которым рисует их движения.

mod ambience;
pub mod bank_gen;
mod combat_sfx;
mod music;
mod steps;
pub mod surface;
mod world_sfx;

use bevy::audio::{PlaybackMode, SpatialListener, Volume};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;


pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Sfx>()
            .init_resource::<AudioSettings>()
            .init_resource::<SfxRng>()
            .init_resource::<Ears>()
            .add_systems(Startup, (load_bank, surface::start_building))
            .add_systems(Update, (attach_listener, play_sfx, update_emitters, reap_one_shots).chain().in_set(AudioSet))
            .configure_sets(Update, AudioSet.after(crate::fight::FightSet))
            .add_systems(Update, debug_log.run_if(|| std::env::var("MRPG_AUDIO_LOG").is_ok()))
            .add_plugins((steps::StepsPlugin, combat_sfx::CombatSfxPlugin, world_sfx::WorldSfxPlugin, ambience::AmbiencePlugin, music::MusicPlugin));
    }
}

/// Системы, которые превращают [`Sfx`] в звук (источники звуков пишут до них).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioSet;

/// Громкость каналов 0..1. Общая громкость — `GlobalVolume` (её задаёт меню настроек).
#[derive(Resource, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioSettings {
    pub music: f32,
    pub sfx: f32,
    pub ambient: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { music: 0.6, sfx: 0.9, ambient: 0.8 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    Sfx,
    Ambient,
    Music,
}

impl AudioSettings {
    pub fn channel(&self, c: Channel) -> f32 {
        match c {
            Channel::Sfx => self.sfx,
            Channel::Ambient => self.ambient,
            Channel::Music => self.music,
        }
        .clamp(0.0, 1.0)
    }
}

/// Загруженные звуки.
#[derive(Resource, Default)]
pub struct Bank {
    sfx: HashMap<&'static str, Vec<Handle<AudioSource>>>,
    pub ambient: HashMap<&'static str, Handle<AudioSource>>,
    pub music: HashMap<&'static str, Handle<AudioSource>>,
}

fn load_bank(mut commands: Commands, assets: Res<AssetServer>) {
    let mut bank = Bank::default();
    for &(name, n) in bank_gen::SFX_VARIANTS {
        let v = (1..=n).map(|i| assets.load(format!("audio/sfx/{name}_{i}.ogg"))).collect();
        bank.sfx.insert(name, v);
    }
    for &name in bank_gen::AMBIENT {
        bank.ambient.insert(name, assets.load(format!("audio/ambient/{name}.ogg")));
    }
    for &(name, _) in bank_gen::MUSIC {
        bank.music.insert(name, assets.load(format!("audio/music/{name}.ogg")));
    }
    commands.insert_resource(bank);
}

/// Простой генератор случайных чисел для разброса высоты и выбора вариантов.
#[derive(Resource)]
pub struct SfxRng(u64);

impl Default for SfxRng {
    fn default() -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Self(seed | 1)
    }
}

impl SfxRng {
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next_f32()
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
    pub fn index(&mut self, n: usize) -> usize {
        ((self.next_f32() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

// ---------------------------------------------------------------------------
// Сообщение «сыграть звук»
// ---------------------------------------------------------------------------

/// Сыграть разовый звук. `name` — из `bank_gen::SFX_VARIANTS` (вариант выбирается случайно).
#[derive(Message, Clone, Debug)]
pub struct Sfx {
    pub name: &'static str,
    /// Где звучит; `None` — «в голове» (интерфейс, свои вдохи).
    pub at: Option<Vec3>,
    /// Следовать за сущностью (шаги, голос), смещение — `at` относительно неё.
    pub follow: Option<Entity>,
    pub gain: f32,
    /// Множитель высоты (1 — как записано); к нему добавляется небольшой разброс.
    pub pitch: f32,
    pub spread: f32,
    /// Расстояние, на котором звук ещё почти в полную силу (м).
    pub near: f32,
    /// Дальше этого звук не слышен вовсе (м).
    pub range: f32,
    pub channel: Channel,
}

impl Sfx {
    pub fn at(name: &'static str, pos: Vec3) -> Self {
        Self { name, at: Some(pos), follow: None, gain: 1.0, pitch: 1.0, spread: 0.06, near: 1.5, range: 40.0, channel: Channel::Sfx }
    }
    pub fn ui(name: &'static str) -> Self {
        Self { at: None, ..Self::at(name, Vec3::ZERO) }
    }
    pub fn gain(mut self, g: f32) -> Self {
        self.gain = g;
        self
    }
    pub fn pitch(mut self, p: f32) -> Self {
        self.pitch = p;
        self
    }
    pub fn spread(mut self, s: f32) -> Self {
        self.spread = s;
        self
    }
    pub fn range(mut self, near: f32, range: f32) -> Self {
        self.near = near;
        self.range = range;
        self
    }
    pub fn channel(mut self, c: Channel) -> Self {
        self.channel = c;
        self
    }
}

/// Разовый звук и когда он начался: если устройство вывода не отдало звук (нет колонок),
/// Bevy его не удалит сам — убираем через 30 с.
#[derive(Component)]
struct OneShot(f32);

fn reap_one_shots(mut commands: Commands, time: Res<Time>, q: Query<(Entity, &OneShot)>) {
    let now = time.elapsed_secs();
    for (e, s) in &q {
        if now - s.0 > 30.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Звучащий источник: где он и как громко.
#[derive(Component, Clone)]
pub struct Emitter {
    /// Мировая точка (или смещение от `follow`); `None` — без позиции.
    pub at: Option<Vec3>,
    pub follow: Option<Entity>,
    pub gain: f32,
    pub near: f32,
    pub range: f32,
    pub channel: Channel,
    /// Доля громкости от плавного включения/выключения (петли окружения, музыка).
    pub fade: f32,
}

/// Где уши: камера игрока (или камера ожидания до входа в игру).
#[derive(Resource, Default, Clone, Copy)]
pub struct Ears {
    pub pos: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub valid: bool,
}

/// Громкость по расстоянию: ближняя зона без затухания, дальше — 1/r, у края — плавно в ноль.
pub fn distance_gain(d: f32, near: f32, range: f32) -> f32 {
    if d >= range {
        return 0.0;
    }
    let near = near.max(0.1);
    let inv = (near / d.max(near)).powf(0.9);
    let edge = ((range - d) / (range * 0.25)).clamp(0.0, 1.0);
    inv * edge * edge * (3.0 - 2.0 * edge)
}

const MAX_VOICES: usize = 48;

fn attach_listener(
    mut commands: Commands,
    cams: Query<(Entity, &GlobalTransform), (With<Camera3d>, Without<SpatialListener>)>,
    listeners: Query<(Entity, &GlobalTransform, Has<crate::player::PlayerCamera>), With<SpatialListener>>,
    player_cam: Query<(), With<crate::player::PlayerCamera>>,
    mut ears: ResMut<Ears>,
) {
    for (e, _) in &cams {
        // Слушает камера игрока; пока её нет — камера ожидания.
        if player_cam.get(e).is_ok() || listeners.is_empty() {
            commands.entity(e).insert(SpatialListener::new(0.4));
        }
    }
    let best = listeners.iter().max_by_key(|(_, _, is_player)| *is_player);
    if let Some((_, gt, _)) = best {
        ears.pos = gt.translation();
        ears.forward = gt.forward().as_vec3();
        ears.right = gt.right().as_vec3();
        ears.valid = true;
    }
    // Лишние слушатели (камера ожидания после входа) мешают Bevy — снимаем.
    if listeners.iter().count() > 1 {
        for (e, _, is_player) in &listeners {
            if !is_player {
                commands.entity(e).remove::<SpatialListener>();
            }
        }
    }
}

/// Точка «на единичном расстоянии» в направлении источника: Bevy по ней строит панораму.
fn virtual_pos(ears: &Ears, src: Vec3) -> Vec3 {
    let d = src - ears.pos;
    let dir = if d.length_squared() < 1e-4 { ears.forward } else { d.normalize() };
    ears.pos + dir
}

fn world_pos(e: &Emitter, follow: &Query<&GlobalTransform>) -> Option<Vec3> {
    let at = e.at?;
    match e.follow {
        Some(f) => follow.get(f).ok().map(|gt| gt.translation() + at),
        None => Some(at),
    }
}

#[allow(clippy::too_many_arguments)]
fn play_sfx(
    mut commands: Commands,
    mut msgs: MessageReader<Sfx>,
    bank: Option<Res<Bank>>,
    mut rng: ResMut<SfxRng>,
    ears: Res<Ears>,
    settings: Res<AudioSettings>,
    global: Res<GlobalVolume>,
    follow: Query<&GlobalTransform>,
    active: Query<(), With<Emitter>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    let Some(bank) = bank else {
        msgs.clear();
        return;
    };
    let mut voices = active.iter().count();
    for m in msgs.read() {
        let Some(list) = bank.sfx.get(m.name) else {
            warn!("нет звука «{}»", m.name);
            continue;
        };
        let em = Emitter { at: m.at, follow: m.follow, gain: m.gain, near: m.near, range: m.range, channel: m.channel, fade: 1.0 };
        let pos = world_pos(&em, &follow);
        let dist_gain = match (pos, ears.valid) {
            (Some(p), true) => distance_gain(p.distance(ears.pos), m.near, m.range),
            (Some(_), false) => 0.0,
            (None, _) => 1.0,
        };
        let vol = m.gain * dist_gain * settings.channel(m.channel) * global.volume.to_linear();
        if vol < 0.002 {
            continue;
        }
        // Слишком много звуков разом — тихие пропускаем.
        if voices >= MAX_VOICES && vol < 0.3 {
            continue;
        }
        voices += 1;
        let handle = list[rng.index(list.len())].clone();
        let speed = (m.pitch * (1.0 + rng.range(-m.spread, m.spread))).clamp(0.3, 3.0);
        let mut settings = PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(vol), speed, ..default() };
        let mut ent = commands.spawn((AudioPlayer(handle), em, OneShot(now)));
        if let Some(p) = pos {
            settings.spatial = true;
            ent.insert(Transform::from_translation(virtual_pos(&ears, p)));
        }
        ent.insert(settings);
    }
}

/// Каждый кадр: громкость по расстоянию и каналу, панорама по направлению.
#[allow(clippy::type_complexity)]
fn update_emitters(
    ears: Res<Ears>,
    settings: Res<AudioSettings>,
    global: Res<GlobalVolume>,
    follow: Query<&GlobalTransform>,
    mut q: Query<(&Emitter, Option<&mut Transform>, Option<&mut AudioSink>, Option<&mut bevy::audio::SpatialAudioSink>)>,
) {
    let master = global.volume.to_linear();
    for (e, tf, sink, spatial) in &mut q {
        let pos = world_pos(e, &follow);
        let dg = match pos {
            Some(p) if ears.valid => distance_gain(p.distance(ears.pos), e.near, e.range),
            Some(_) => 0.0,
            None => 1.0,
        };
        let vol = Volume::Linear(e.gain * e.fade * dg * settings.channel(e.channel) * master);
        if let (Some(p), Some(mut tf)) = (pos, tf) {
            let v = virtual_pos(&ears, p);
            if tf.translation.distance_squared(v) > 1e-6 {
                tf.translation = v;
            }
        }
        if let Some(mut s) = sink {
            s.set_volume(vol);
        }
        if let Some(mut s) = spatial {
            s.set_volume(vol);
        }
    }
}

/// Отладка (`MRPG_AUDIO_LOG=1`): раз в 2 с — где уши, что вокруг, что звучит.
fn debug_log(time: Res<Time>, fighters: Res<combat_sfx::Fighters>, ears: Res<Ears>, scene: Res<ambience::Scene>, dir: Res<music::MusicDirector>, q: Query<(&Emitter, Has<AudioSink>, Has<bevy::audio::SpatialAudioSink>)>, mut next: Local<f32>, mut heard: MessageReader<Sfx>) {
    let names: Vec<&str> = heard.read().map(|s| s.name).collect();
    let now = time.elapsed_secs();
    if !names.is_empty() {
        info!("звук: {names:?}");
    }
    if now < *next {
        return;
    }
    *next = now + 2.0;
    let playing = q.iter().filter(|(_, a, b)| *a || *b).count();
    let loops: Vec<String> = q.iter().filter(|(e, _, _)| e.channel != Channel::Sfx && e.fade > 0.01).map(|(e, _, _)| format!("{:?}:{:.2}", e.channel, e.fade)).collect();
    info!(
        "звук: бой {:.0} / враг {:.0} с назад; уши {:.0?}, море {:.2}, крыша {:.2}, город {:.2}, рынок {:.2}, храм {:.2}, высота {:.2}, место {:?}, время {:?}; музыка {:?}; источников {} (звучит {}), петли {:?}",
        now - fighters.last_fight, now - fighters.hostile_near, ears.pos, scene.sea, scene.indoor, scene.city, scene.market, scene.sacred, scene.height, scene.place, scene.time, dir.want, q.iter().count(), playing, loops
    );
}

/// Создать петлю (окружение, огонь, кипение): громкость задаёт `Emitter::fade`.
pub fn spawn_loop(commands: &mut Commands, handle: Handle<AudioSource>, emitter: Emitter, ears: &Ears) -> Entity {
    let spatial = emitter.at.is_some() && emitter.follow.is_none();
    let mut ent = commands.spawn((AudioPlayer(handle), PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), spatial, ..default() }));
    if let Some(p) = emitter.at.filter(|_| spatial) {
        ent.insert(Transform::from_translation(virtual_pos(ears, p)));
    }
    ent.insert(emitter);
    ent.id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_gain_falls_off() {
        assert!((distance_gain(0.5, 2.0, 40.0) - 1.0).abs() < 1e-4);
        assert!(distance_gain(10.0, 2.0, 40.0) < distance_gain(5.0, 2.0, 40.0));
        assert_eq!(distance_gain(40.0, 2.0, 40.0), 0.0);
        assert!(distance_gain(39.0, 2.0, 40.0) < 0.02);
    }

    #[test]
    fn every_sound_file_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/audio");
        for &(name, n) in bank_gen::SFX_VARIANTS {
            for i in 1..=n {
                let p = root.join(format!("sfx/{name}_{i}.ogg"));
                assert!(p.exists(), "нет файла {}", p.display());
            }
        }
        for name in bank_gen::AMBIENT {
            assert!(root.join(format!("ambient/{name}.ogg")).exists(), "нет петли {name}");
        }
        for (name, _) in bank_gen::MUSIC {
            assert!(root.join(format!("music/{name}.ogg")).exists(), "нет музыки {name}");
        }
    }
}
