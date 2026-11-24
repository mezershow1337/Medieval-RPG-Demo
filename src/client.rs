//! Клиент: графика, подключение к серверу, отображение других бойцов и тел.

use crate::combat::{self, Action, Fighter, WeaponKind};
use crate::fight::{self, CombatFeedback, LocalFighter};
use crate::player::{LocalPlayer, PlayerPlugin, spawn_local_player};
use crate::protocol::*;
use crate::gear_look::{Gear, ShieldLook};
use crate::rig::{self, Blade, CharacterModels, NeedsRig, NeedsShield, NeedsWeapon, Outfit, PoseInput, RigPose, ShieldKind};
use crate::{graphics, hud, level, worn};
use bevy::prelude::*;
use bevy_renet::netcode::{NetcodeClientPlugin, NetcodeErrorEvent};
use bevy_renet::{RenetClient, RenetClientPlugin, client_connected};
use std::collections::HashMap;
use std::net::SocketAddr;

/// Состояние подключения для интерфейса.
#[derive(Resource, Default)]
pub struct NetState {
    pub my_id: Option<u64>,
    pub server_addr: Option<SocketAddr>,
    pub error: Option<String>,
}

/// Другой боец (игрок или враг), которого мы видим по данным сервера.
#[derive(Component)]
pub struct RemoteActor {
    pub id: u64,
    pub kind: ActorKind,
    pub fighter: Fighter,
    pub pitch: f32,
    target_pos: Vec3,
    target_yaw: f32,
    crouching: bool,
    /// Для анимации: фаза и сила шага, доля приседа, прошлое положение.
    walk_phase: f32,
    /// Сглаженная скорость, м/с.
    speed: f32,
    walk: f32,
    crouch: f32,
    last_pos: Vec3,
    /// Вид клинка (см. `ActorSnapshot::finish`).
    finish: u8,
    /// Видимые доспехи и щит (у игроков) и то, во что модель одета сейчас.
    gear: Gear,
    shown_gear: Gear,
    /// Сглаженное положение оружия (рукоять, направление) — без рывков между фазами удара.
    weapon_smooth: Option<(Vec3, Vec3)>,
}

/// Скелет с телом (дочерняя сущность бойца или тела на земле).
#[derive(Component)]
struct BodyRig;

/// Тело падает навзничь после смерти.
#[derive(Component)]
struct CorpseFall {
    t: f32,
    seed: u64,
}

/// Меч в руке другого бойца.
#[derive(Component)]
struct RemoteSword;

/// Щит в руке.
#[derive(Component)]
struct RemoteShield;

/// Какое оружие сейчас показано (лучник меняет лук на нож): вид и предмет со своей моделью.
#[derive(Component)]
struct ShownWeapon(WeaponKind, Option<u16>);

/// Какой предмет у бойца в руке (у игроков; у врагов — общая модель по виду оружия).
fn held_item(r: &RemoteActor) -> Option<u16> {
    r.gear.main_hand.filter(|_| !r.gear.holstered && r.fighter.weapon != WeaponKind::Fists)
}

/// Летящая стрела: двигаем её сами между снимками.
#[derive(Component)]
pub struct ArrowVisual {
    vel: Vec3,
}

#[derive(Resource, Default)]
struct ArrowIndex(HashMap<u64, Entity>);

#[derive(Resource)]
struct ArrowAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// Тело на земле.
#[derive(Component)]
#[allow(dead_code)] // используется окном обыска (тред про инвентарь)
pub struct CorpseMarker {
    pub corpse_id: u64,
    pub kind: ActorKind,
}

/// Ники игроков (присылает сервер). Для NPC имён нет — у них название по виду.
#[derive(Resource, Default)]
pub struct PlayerNames(pub HashMap<u64, String>);

impl PlayerNames {
    /// Ник игрока или запасное «Игрок 123», пока имя не пришло.
    pub fn of(&self, id: u64) -> String {
        self.0.get(&id).cloned().unwrap_or_else(|| format!("Игрок {}", id % 1000))
    }
}

#[derive(Resource, Default)]
pub struct RemoteIndex(pub HashMap<u64, Entity>);

#[derive(Resource, Default)]
pub struct CorpseIndex(pub HashMap<u64, Entity>);

#[derive(Resource)]
struct SendTimer(Timer);

/// Общие материалы персонажей. Сами модели — в `rig::CharacterModels` (загружаются из glb).
#[derive(Resource, Clone)]
pub struct CharacterAssets {
    /// Клинок, пока модели не загрузились.
    pub steel: Handle<StandardMaterial>,
    /// Светящийся клинок: этот удар нельзя парировать.
    pub blade_red: Handle<StandardMaterial>,
    /// Красное свечение вдоль клинка на ответном ударе (поверх любой модели оружия):
    /// цилиндр высотой 1 по оси Y.
    pub glow_mesh: Handle<Mesh>,
    pub glow: Handle<StandardMaterial>,
}

/// Свечение рипоста: стоит рядом с оружием и повторяет его положение.
#[derive(Component)]
pub struct RiposteGlow;

pub fn riposte_glow_bundle(assets: &CharacterAssets) -> impl Bundle {
    (RiposteGlow, Mesh3d(assets.glow_mesh.clone()), MeshMaterial3d(assets.glow.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster)
}

/// Каждому новому бойцу — своё свечение рипоста.
fn attach_riposte_glow(mut commands: Commands, assets: Res<CharacterAssets>, added: Query<Entity, Added<RemoteActor>>) {
    for e in &added {
        commands.entity(e).with_children(|p| {
            p.spawn(riposte_glow_bundle(&assets));
        });
    }
}

/// Положение свечения вдоль клинка оружия `weapon`, стоящего в `sword`. `None` — у оружия
/// нет клинка (кулаки, лук).
pub fn riposte_glow_transform(sword: &Transform, weapon: WeaponKind) -> Option<Transform> {
    if weapon == WeaponKind::Fists || weapon.ranged() {
        return None;
    }
    let (from, to) = (0.08, weapon.stats().reach);
    let len = (to - from).max(0.1);
    Some(Transform { translation: sword.transform_point(Vec3::Y * (from + len * 0.5)), rotation: sword.rotation, scale: Vec3::new(1.0, len, 1.0) })
}

impl CharacterAssets {
    /// Обычный материал клинка (пока модели не загружены — простая сталь).
    #[allow(dead_code)]
    pub fn blade(&self, models: Option<&CharacterModels>) -> Handle<StandardMaterial> {
        models.map(|m| m.blade.clone()).unwrap_or_else(|| self.steel.clone())
    }
}

pub fn run(start: crate::menu::Start) {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Херсонес, 510 — Medieval RPG".into(), present_mode: bevy::window::PresentMode::AutoVsync, ..default() }),
            ..default()
        }))
        .add_plugins((RenetClientPlugin, NetcodeClientPlugin, PlayerPlugin, fight::FightPlugin, hud::HudPlugin, graphics::GraphicsPlugin, crate::textures::TexturesPlugin))
        .add_plugins(rig::RigPlugin)
        .add_plugins(crate::vegetation::VegetationPlugin)
        .add_plugins(crate::loot_client::LootClientPlugin)
        .add_plugins(crate::doors::DoorClientPlugin)
        .add_plugins(crate::map::MapClientPlugin)
        .add_plugins(crate::residents::ResidentsPlugin)
        .add_plugins(crate::world_clock::ClockClientPlugin)
        .add_plugins(crate::life_client::LifeClientPlugin)
        .add_plugins(crate::night_lights::NightLightsPlugin)
        .add_plugins(crate::daylight::DaylightPlugin)
        .add_plugins(crate::civil_client::CivilClientPlugin)
        .add_plugins(crate::trade_client::TradeClientPlugin)
        .add_plugins((crate::lore_client::LoreClientPlugin, crate::lore_client::DevShotPlugin, crate::medicine_client::MedicineClientPlugin, crate::brewing_client::BrewingClientPlugin, crate::alchemy_fx::AlchemyFxPlugin))
        .add_plugins(crate::smithing_client::SmithingClientPlugin)
        .add_plugins(crate::audio::GameAudioPlugin)
        .add_plugins(crate::towns_client::TownClientPlugin)
        .add_plugins(crate::ui_theme::UiThemePlugin)
        .add_plugins((crate::skill_book::SkillBookPlugin, crate::hints::HintsPlugin))
        .add_plugins(crate::work_client::WorkClientPlugin)
        .add_plugins(crate::console::ConsolePlugin)
        .add_systems(Update, (hide_ui_for_shots, mute_player_camera_for_shots).run_if(|| std::env::var("MRPG_CAM").is_ok()))
        .add_systems(Update, shot_tour.run_if(|| std::env::var("MRPG_TOUR").is_ok()))
        // Клиент без соединения: транспорт появится, когда игрок выберет в меню «Создать» или «Подключиться».
        .insert_resource(RenetClient::new(connection_config()))
        .insert_resource(NetState::default())
        .insert_resource(start)
        .add_plugins((crate::settings::SettingsPlugin, crate::menu::MenuPlugin))
        .insert_resource(level::level_colliders())
        .insert_resource(RemoteIndex::default())
        .insert_resource(PlayerNames::default())
        .insert_resource(CorpseIndex::default())
        .insert_resource(ArrowIndex::default())
        .insert_resource(SendTimer(Timer::from_seconds(1.0 / 30.0, TimerMode::Repeating)))
        .insert_resource(ClearColor(Color::srgb(0.55, 0.5, 0.48)))
        .insert_resource(GlobalAmbientLight { color: Color::srgb(0.7, 0.72, 0.85), brightness: 250.0, ..default() })
        .add_observer(on_netcode_error)
        .add_systems(Startup, (level::spawn_level, setup_character_assets, spawn_waiting_camera))
        .add_systems(
            Update,
            ((receive_reliable, receive_snapshots).chain(), send_state, animate_remote).run_if(client_connected).before(fight::FightSet).in_set(rig::PoseSet),
        )
        .add_systems(Update, animate_corpses.in_set(rig::PoseSet))
        .add_systems(Update, (fly_arrows, refresh_remote_weapons, refresh_remote_gear, worn::attach_worn, attach_riposte_glow))
        .add_systems(Update, gear_showcase.run_if(|| std::env::var("MRPG_GEAR_SHOWCASE").is_ok()))
        .add_systems(Update, pose_showcase.in_set(rig::PoseSet).run_if(|| std::env::var("MRPG_GEAR_SHOWCASE").is_ok()))
        .add_systems(Update, (people_showcase, pose_people).chain().in_set(rig::PoseSet).run_if(|| std::env::var("MRPG_PEOPLE_SHOWCASE").is_ok()))
        .add_systems(Update, duel_showcase.run_if(|| std::env::var("MRPG_DUEL_SHOWCASE").is_ok()))
        .add_systems(
            Update,
            (freeze_showcase, animate_remote).chain().in_set(rig::PoseSet).run_if(|| std::env::var("MRPG_DUEL_SHOWCASE").is_ok()).run_if(not(client_connected)),
        )
        .add_systems(Update, hide_ui_for_shots.run_if(|| std::env::var("MRPG_DUEL_SHOWCASE").is_ok() && std::env::var("MRPG_CAM").is_err()))
        .run();
}

fn on_netcode_error(event: On<NetcodeErrorEvent>, mut state: ResMut<NetState>) {
    error!("Ошибка сети: {}", event.0);
    state.error = Some(event.0.to_string());
}

/// Камера, пока сервер не прислал точку появления.
#[derive(Component)]
pub struct WaitingCamera;

/// Снимки уровня (MRPG_CAM): прячем интерфейс.
fn hide_ui_for_shots(mut roots: Query<&mut Visibility, (With<Node>, Without<ChildOf>)>) {
    for mut v in &mut roots {
        *v = Visibility::Hidden;
    }
}

/// Снимки уровня (MRPG_CAM): камера игрока не рисует — кадр снимает камера ожидания.
fn mute_player_camera_for_shots(mut cams: Query<&mut Camera, With<crate::player::PlayerCamera>>) {
    for mut c in &mut cams {
        c.is_active = false;
    }
}

/// Обход для снимков уровня (вместе с `MRPG_CAM`): `MRPG_TOUR="имя:x,y,z,tx,ty,tz;…"` — камера
/// ожидания по очереди встаёт в каждую точку и сохраняет `MRPG_TOUR_DIR/имя.png` (первый снимок —
/// через `MRPG_TOUR_FRAMES` кадров, пока всё грузится, следующие — через 8), потом игра выходит.
/// Одна загрузка уровня на много снимков: на программном рендере это минуты и гигабайты.
/// `имя@часы:…` — снимок в другое время суток (`@75.5` — 03:30 четвёртых суток).
fn shot_tour(mut commands: Commands, mut cams: Query<&mut Transform, With<WaitingCamera>>, mut state: Local<(usize, u32)>, mut exit: MessageWriter<AppExit>) {
    let tour = std::env::var("MRPG_TOUR").unwrap_or_default();
    let views: Vec<(&str, Option<f64>, Transform)> = tour
        .split(';')
        .filter_map(|v| {
            let (name, cam) = v.split_once(':')?;
            let (name, at) = match name.split_once('@') {
                Some((n, h)) => (n, h.trim().parse::<f64>().ok().map(|h| h * 3600.0)),
                None => (name, None),
            };
            let f: Vec<f32> = cam.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            (f.len() == 6).then(|| (name.trim(), at, Transform::from_xyz(f[0], f[1], f[2]).looking_at(Vec3::new(f[3], f[4], f[5]), Vec3::Y)))
        })
        .collect();
    let first: u32 = std::env::var("MRPG_TOUR_FRAMES").ok().and_then(|s| s.parse().ok()).unwrap_or(30);
    let dir = std::env::var("MRPG_TOUR_DIR").unwrap_or_else(|_| ".".into());
    let (i, n) = &mut *state;
    *n += 1;
    let Some((name, at, tf)) = views.get(*i) else {
        if *n > 12 {
            exit.write(AppExit::Success);
        }
        return;
    };
    for mut c in &mut cams {
        *c = *tf;
    }
    if at.is_some() {
        crate::graphics::set_shot_time(*at);
    }
    // Сменилось время — ждём дольше: свет неба пересчитывается в фоне.
    let changed = *i > 0 && at.is_some() && views[*i - 1].1 != *at;
    if *n >= if *i == 0 { first } else if changed { 16 } else { 8 } {
        info!("обход: снимок {name}");
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(format!("{dir}/{name}.png")));
        *i += 1;
        *n = 0;
    }
}

fn spawn_waiting_camera(mut commands: Commands) {
    // Для снимков уровня: MRPG_CAM="x,y,z,tx,ty,tz" — откуда и куда смотрит камера ожидания.
    // Высота с префиксом «g» отсчитывается от земли: "g1.7" — на уровне глаз.
    // Или "@место,dx,dy,dz" — смотреть на точку из `places()` (например "@smithy,-6,2.5,4").
    let cam = std::env::var("MRPG_CAM").ok().and_then(|s| {
        if let Some(rest) = s.strip_prefix('@') {
            let t: Vec<&str> = rest.split(',').map(str::trim).collect();
            let p = crate::level::place_pos(t[0])?;
            let d: Vec<f32> = t[1..].iter().filter_map(|v| v.parse().ok()).collect();
            let off = if d.len() == 3 { Vec3::new(d[0], d[1], d[2]) } else { Vec3::new(-6.0, 2.5, 4.0) };
            let at = p + Vec3::Y * 1.2;
            info!("камера снимка: {} -> {at}", p + off);
            return Some(Transform::from_translation(p + off).looking_at(at, Vec3::Y));
        }
        let t: Vec<&str> = s.split(',').map(str::trim).collect();
        let v: Vec<f32> = t.iter().filter_map(|t| t.trim_start_matches('g').parse().ok()).collect();
        if v.len() != 6 {
            return None;
        }
        let y = |i: usize, x: f32, z: f32| if t[i].starts_with('g') { crate::level::ground_y(x, z) + v[i] } else { v[i] };
        let (eye, at) = (Vec3::new(v[0], y(1, v[0], v[2]), v[2]), Vec3::new(v[3], y(4, v[3], v[5]), v[5]));
        info!("камера снимка: {eye} -> {at}");
        Some(Transform::from_translation(eye).looking_at(at, Vec3::Y))
    });
    let tf = cam.unwrap_or_else(|| {
        let p = crate::level::spawn_point(0);
        Transform::from_translation(p + Vec3::new(-40.0, 30.0, 60.0)).looking_at(p + Vec3::Y * 4.0, Vec3::Y)
    });
    commands.spawn((WaitingCamera, Camera3d::default(), Projection::Perspective(PerspectiveProjection { fov: 70f32.to_radians(), ..default() }), tf));
}

fn setup_character_assets(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>, mut meshes: ResMut<Assets<Mesh>>) {
    // Стрела: тонкое древко вдоль -Z длиной 75 см.
    commands.insert_resource(ArrowAssets {
        mesh: meshes.add(Cuboid::new(0.012, 0.012, 0.75)),
        material: materials.add(StandardMaterial { base_color: Color::srgb(0.45, 0.33, 0.2), perceptual_roughness: 0.8, ..default() }),
    });
    commands.insert_resource(CharacterAssets {
        steel: materials.add(StandardMaterial { base_color: Color::srgb(0.72, 0.74, 0.78), metallic: 0.9, perceptual_roughness: 0.3, ..default() }),
        blade_red: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.2, 0.1),
            emissive: LinearRgba::rgb(1.2, 0.15, 0.05),
            ..default()
        }),
        glow_mesh: meshes.add(Cylinder::new(0.022, 1.0)),
        glow: materials.add(StandardMaterial {
            base_color: Color::srgba(0.7, 0.12, 0.05, 0.22),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        }),
    });
}

/// Где рука держит оружие: (правая, левая) — центр кулака в координатах оружия.
pub fn weapon_grips(weapon: WeaponKind) -> (Vec3, Option<Vec3>) {
    match weapon {
        WeaponKind::Spatha | WeaponKind::Axe | WeaponKind::Seax => (Vec3::new(0.0, -0.06, 0.0), None),
        // Копьё: начало координат — передняя (левая) рука, правая сзади на древке.
        WeaponKind::Spear => (Vec3::new(0.0, -0.6, 0.0), Some(Vec3::ZERO)),
        // Лук держат за середину.
        WeaponKind::Bow => (Vec3::ZERO, None),
        // Кулак: «оружие» и есть кулак.
        WeaponKind::Fists => (Vec3::ZERO, None),
    }
}

/// Как одет боец: враги — по своему типу, игроки — по надетому в инвентаре.
pub fn outfit_of(kind: ActorKind, id: u64, gear: &Gear) -> Outfit {
    if kind == ActorKind::Townsman
        && let Some(i) = crate::civil_server::militia_resident(id)
        && let Some(role) = crate::residents::role_of(i)
    {
        return crate::residents::outfit(role, i as u64 + 17);
    }
    let base = Outfit::for_actor(kind, id);
    if kind == ActorKind::Player { base.with_gear(gear) } else { base }
}

/// Какой щит в руке: у игроков — надетый, у врагов — по типу (если он у них есть).
pub(crate) fn shield_of(kind: ActorKind, has_shield: bool, gear: &Gear) -> Option<ShieldKind> {
    if kind == ActorKind::Player {
        return gear.shield().map(|s| match s {
            ShieldLook::Oval => ShieldKind::Oval,
            ShieldLook::Round => ShieldKind::Round,
        });
    }
    has_shield.then(|| ShieldKind::for_actor(kind))
}

/// Щит на руке другого бойца (место — как в `animate_remote`).
fn spawn_remote_shield(p: &mut ChildSpawnerCommands, kind: ShieldKind) {
    p.spawn((RemoteShield, NeedsShield(kind), Transform::from_xyz(-0.42, 1.05, -0.1), Visibility::default()));
}

/// Где левая рука держит щит (ремни на обратной стороне).
const SHIELD_GRIP: Vec3 = Vec3::new(0.0, -0.02, 0.07);

fn spawn_remote(commands: &mut Commands, index: &mut RemoteIndex, snap: &ActorSnapshot) {
    let pos: Vec3 = snap.pos.into();
    let mut fighter = Fighter::new(snap.weapon, snap.max_hp, snap.shield);
    fighter.action = snap.action;
    fighter.hp = snap.hp;
    let entity = commands
        .spawn((
            RemoteActor {
                id: snap.id,
                kind: snap.kind,
                fighter,
                pitch: snap.pitch,
                target_pos: pos,
                target_yaw: snap.yaw,
                crouching: snap.crouching,
                walk_phase: 0.0,
                walk: 0.0,
                speed: 0.0,
                crouch: if snap.crouching { 1.0 } else { 0.0 },
                last_pos: pos,
                finish: snap.finish,
                gear: snap.gear,
                shown_gear: snap.gear,
                weapon_smooth: None,
            },
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(snap.yaw)),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((BodyRig, worn::Worn::new(worn::Extras::of(&snap.gear)), NeedsRig(outfit_of(snap.kind, snap.id, &snap.gear)), Transform::default(), Visibility::default()));
            p.spawn((RemoteSword, ShownWeapon(snap.weapon, None), NeedsWeapon(snap.weapon), Transform::from_xyz(0.3, 1.0, -0.4), Visibility::default()));
            if let Some(kind) = shield_of(snap.kind, snap.shield, &snap.gear) {
                spawn_remote_shield(p, kind);
            }
        })
        .id();
    index.0.insert(snap.id, entity);
}

#[allow(clippy::too_many_arguments)]
fn spawn_corpse(commands: &mut Commands, index: &mut CorpseIndex, corpse_id: u64, kind: ActorKind, pos: Vec3, yaw: f32, gear: &Gear, animate: bool) {
    let proto = crate::npc::fighter_for(kind);
    // Оружие и щит игрока остаются в его инвентаре (их забирают при обыске), рядом не кладём.
    let weapon = if kind == ActorKind::Player { WeaponKind::Fists } else { proto.weapon };
    let shield = if kind == ActorKind::Player { None } else { shield_of(kind, proto.has_shield, gear) };
    let entity = commands
        .spawn((CorpseMarker { corpse_id, kind }, Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()))
        .with_children(|c| {
            // Лежит на спине; упавшее оружие и щит — рядом.
            let t = if animate { 0.0 } else { 1.0 };
            c.spawn((BodyRig, worn::Worn::new(worn::Extras::of(gear)), CorpseFall { t, seed: corpse_id }, NeedsRig(outfit_of(kind, corpse_id, gear)), rig::fall_transform(t), Visibility::default()));
            c.spawn((
                NeedsWeapon(weapon),
                Transform::from_xyz(0.5, 0.025, 0.35).with_rotation(Quat::from_rotation_y(0.35) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                Visibility::default(),
            ));
            if let Some(shield) = shield {
                c.spawn((
                    NeedsShield(shield),
                    Transform::from_xyz(-0.6, 0.06, 0.55).with_rotation(Quat::from_rotation_y(-0.5) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    Visibility::default(),
                ));
            }
        })
        .id();
    index.0.insert(corpse_id, entity);
}

/// Тело мирного человека (в своей одежде, без оружия).
pub fn spawn_civ_corpse(commands: &mut Commands, index: &mut CorpseIndex, corpse_id: u64, outfit: Outfit, scale: f32, pos: Vec3, yaw: f32, animate: bool) {
    let entity = commands
        .spawn((
            CorpseMarker { corpse_id, kind: ActorKind::Player },
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)),
            Visibility::default(),
        ))
        .with_children(|c| {
            let t = if animate { 0.0 } else { 1.0 };
            c.spawn((BodyRig, CorpseFall { t, seed: corpse_id }, NeedsRig(outfit), rig::fall_transform(t), Visibility::default()));
        })
        .id();
    index.0.insert(corpse_id, entity);
}

/// Падение тела: поза из стойки в лежачую, корень скелета заваливается назад.
fn animate_corpses(time: Res<Time>, mut corpses: Query<(&mut CorpseFall, &mut Transform, Option<&mut RigPose>)>) {
    const FALL_TIME: f32 = 0.8;
    for (mut fall, mut transform, pose) in &mut corpses {
        let Some(mut pose) = pose else { continue };
        if fall.t >= 1.0 && pose.pelvis.y > 0.0 && pose.rot[3] != Quat::IDENTITY {
            continue;
        }
        fall.t = (fall.t + time.delta_secs() / FALL_TIME).min(1.0);
        let k = fall.t;
        *transform = rig::fall_transform(k);
        let standing = rig::solve_pose(&PoseInput { time: time.elapsed_secs(), ..default() }).pose;
        *pose = rig::blend(&standing, &rig::dead_pose(fall.seed), (k * 1.3).min(1.0));
    }
}

#[allow(clippy::too_many_arguments)]
fn receive_reliable(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut state: ResMut<NetState>,
    mut index: ResMut<RemoteIndex>,
    mut corpses: ResMut<CorpseIndex>,
    assets: Res<CharacterAssets>,
    mut feedback: MessageWriter<CombatFeedback>,
    mut local: Query<(&mut LocalPlayer, &mut Transform, Option<&mut LocalFighter>)>,
    waiting: Query<Entity, With<WaitingCamera>>,
    mut names: ResMut<PlayerNames>,
    time: Res<Time>,
    mut welcomed_at: Local<Option<f32>>,
    mut civil: MessageWriter<crate::civil_client::CivilNet>,
    mut clock: MessageWriter<crate::world_clock::ClockSync>,
    mut alarm: MessageWriter<crate::daily_life::Alarm>,
) {
    while let Some(bytes) = client.receive_message(CH_RELIABLE) {
        let Some(msg) = decode::<ServerMessage>(&bytes) else { continue };
        match msg {
            ServerMessage::Welcome { your_id, spawn } => {
                info!("Подключились к серверу, наш id {your_id}");
                state.my_id = Some(your_id);
                state.error = None;
                *welcomed_at = Some(time.elapsed_secs());
                // Снимок (MRPG_CAM) в режиме host: камеру ожидания оставляем, чтобы видеть
                // торговцев и прочее, что приходит с сервера, с заданной точки.
                if std::env::var("MRPG_CAM").is_err() {
                    for e in &waiting {
                        commands.entity(e).despawn();
                    }
                }
                if local.is_empty() {
                    spawn_local_player(&mut commands, spawn.into(), &assets);
                }
            }
            ServerMessage::Correction { pos } => {
                for (mut p, mut t, _) in &mut local {
                    t.translation = pos.into();
                    p.velocity = Vec3::ZERO;
                }
            }
            ServerMessage::PlayerJoined { id } => info!("Игрок {id} вошёл в игру"),
            ServerMessage::PlayerName { id, name } => {
                info!("Игрок {id} — «{name}»");
                names.0.insert(id, name);
            }
            ServerMessage::PlayerLeft { id } => {
                if let Some(e) = index.0.remove(&id) {
                    commands.entity(e).despawn();
                }
            }
            ServerMessage::Hit { attacker, target, outcome, at } => {
                feedback.write(CombatFeedback::Hit { attacker, target, outcome, at: at.into() });
            }
            ServerMessage::Died { id, killer } => {
                if Some(id) == state.my_id {
                    for (_, _, f) in &mut local {
                        if let Some(mut f) = f {
                            f.fighter.action = Action::Dead;
                            f.fighter.hp = 0.0;
                        }
                    }
                } else if let Some(e) = index.0.remove(&id) {
                    commands.entity(e).despawn();
                }
                feedback.write(CombatFeedback::Died { id, killer });
            }
            ServerMessage::Respawned { pos } => {
                for (mut p, mut t, f) in &mut local {
                    t.translation = pos.into();
                    p.velocity = Vec3::ZERO;
                    if let Some(mut f) = f {
                        f.fighter = crate::npc::fighter_for(ActorKind::Player);
                    }
                }
            }
            ServerMessage::CorpseSpawned { corpse_id, kind, pos, yaw, gear } => {
                if !corpses.0.contains_key(&corpse_id) {
                    // Тела, которые уже лежали, когда мы вошли, не падают заново.
                    let fresh = welcomed_at.is_some_and(|t| time.elapsed_secs() - t > 1.5);
                    spawn_corpse(&mut commands, &mut corpses, corpse_id, kind, pos.into(), yaw, &gear, fresh);
                }
            }
            ServerMessage::CivilianHit { civ, at, amount, head } => {
                feedback.write(CombatFeedback::Hit { attacker: 0, target: u64::MAX, outcome: combat::HitOutcome::Damage { amount, killed: false, head }, at: at.into() });
                let _ = civ;
                civil.write(crate::civil_client::CivilNet::Hit);
                alarm.write(crate::daily_life::Alarm(at.into()));
            }
            ServerMessage::CivilianDead { civ, corpse } => {
                let fresh = welcomed_at.is_some_and(|t| time.elapsed_secs() - t > 1.5);
                civil.write(crate::civil_client::CivilNet::Dead { civ, corpse: corpse.map(|(id, p, y)| (id, p.into(), y)), fresh });
            }
            ServerMessage::CivilianBack { civ } => {
                civil.write(crate::civil_client::CivilNet::Back(civ));
            }
            ServerMessage::Law(msg) => {
                civil.write(crate::civil_client::CivilNet::Law(msg));
            }
            ServerMessage::Clock { game } => {
                clock.write(crate::world_clock::ClockSync(game));
            }
            ServerMessage::CorpseRemoved { corpse_id } => {
                if let Some(e) = corpses.0.remove(&corpse_id) {
                    commands.entity(e).despawn();
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn receive_snapshots(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    state: Res<NetState>,
    mut index: ResMut<RemoteIndex>,
    mut remotes: Query<&mut RemoteActor>,
    mut local: Query<&mut LocalFighter>,
    mut arrow_index: ResMut<ArrowIndex>,
    mut arrows: Query<(&mut ArrowVisual, &mut Transform)>,
    arrow_assets: Res<ArrowAssets>,
) {
    let mut latest: Option<Snapshot> = None;
    while let Some(bytes) = client.receive_message(CH_UNRELIABLE) {
        if let Some(s) = decode::<Snapshot>(&bytes) {
            // Пакеты могут прийти не по порядку — берём самый свежий.
            if latest.as_ref().is_none_or(|l| s.tick.wrapping_sub(l.tick) < u32::MAX / 2) {
                latest = Some(s);
            }
        }
    }
    let Some(snapshot) = latest else { return };
    let Some(my_id) = state.my_id else { return };
    let mut seen = Vec::new();
    for a in &snapshot.actors {
        if a.id == my_id {
            if let Ok(mut f) = local.single_mut() {
                fight::reconcile(&mut f, a);
            }
            continue;
        }
        seen.push(a.id);
        match index.0.get(&a.id).and_then(|e| remotes.get_mut(*e).ok()) {
            Some(mut r) => {
                r.target_pos = a.pos.into();
                r.target_yaw = a.yaw;
                r.pitch = a.pitch;
                r.crouching = a.crouching;
                // Берём действие с сервера, но не откатываем таймер назад внутри того же действия.
                if std::mem::discriminant(&r.fighter.action) != std::mem::discriminant(&a.action) || a.action.time() > r.fighter.action.time() {
                    r.fighter.action = a.action;
                }
                r.fighter.hp = a.hp;
                r.fighter.max_hp = a.max_hp;
                r.fighter.has_shield = a.shield;
                r.fighter.weapon = a.weapon;
                r.fighter.stamina = a.stamina;
                r.fighter.max_stamina = a.max_stamina;
                r.fighter.action_speed = a.action_speed;
                r.finish = a.finish;
                r.gear = a.gear;
            }
            None if !index.0.contains_key(&a.id) => spawn_remote(&mut commands, &mut index, a),
            None => {}
        }
    }
    // Стрелы: обновляем, создаём новые, убираем упавшие.
    let mut alive_arrows = Vec::with_capacity(snapshot.arrows.len());
    for a in &snapshot.arrows {
        alive_arrows.push(a.id);
        let (pos, vel): (Vec3, Vec3) = (a.pos.into(), a.vel.into());
        let rot = Transform::default().looking_to(vel.normalize_or(Vec3::NEG_Z), Vec3::Y).rotation;
        match arrow_index.0.get(&a.id).and_then(|e| arrows.get_mut(*e).ok()) {
            Some((mut v, mut t)) => {
                v.vel = vel;
                // Плавно подтягиваем к серверной точке.
                t.translation = t.translation.lerp(pos, 0.5);
            }
            None => {
                let e = commands
                    .spawn((ArrowVisual { vel }, Mesh3d(arrow_assets.mesh.clone()), MeshMaterial3d(arrow_assets.material.clone()), Transform::from_translation(pos).with_rotation(rot)))
                    .id();
                arrow_index.0.insert(a.id, e);
            }
        }
    }
    arrow_index.0.retain(|id, e| {
        let keep = alive_arrows.contains(id);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    // Те, кого больше нет в снимке, ушли или погибли.
    let gone: Vec<u64> = index.0.keys().copied().filter(|id| !seen.contains(id)).collect();
    for id in gone {
        if let Some(e) = index.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }
}

/// Стрелы летят по той же баллистике, что и на сервере.
fn fly_arrows(time: Res<Time>, mut arrows: Query<(&mut ArrowVisual, &mut Transform)>) {
    let dt = time.delta_secs().min(0.1);
    for (mut a, mut t) in &mut arrows {
        a.vel.y -= combat::ARROW_GRAVITY * dt;
        t.translation += a.vel * dt;
        if a.vel.length_squared() > 0.01 {
            t.look_to(a.vel, Vec3::Y);
        }
    }
}

/// Если боец сменил оружие (лучник взялся за нож), меняем модель в руке.
/// У игрока — модель именно того предмета, что у него в руке.
fn refresh_remote_weapons(
    mut commands: Commands,
    remotes: Query<(&RemoteActor, &Children)>,
    mut shown: Query<(Entity, &mut ShownWeapon)>,
    models: Option<Res<crate::icons::ItemModels>>,
) {
    for (r, children) in &remotes {
        let want = (r.fighter.weapon, held_item(r));
        for child in children.iter() {
            if let Ok((e, mut w)) = shown.get_mut(child)
                && (w.0, w.1) != want
            {
                (w.0, w.1) = want;
                worn::set_weapon_model(&mut commands, e, want.0, want.1.map(crate::items::ItemBaseId), r.finish, models.as_deref());
            }
        }
    }
}

/// `MRPG_GEAR_SHOWCASE=x,y,z` (или пусто — у агоры): для снимков ряд игроков в разных доспехах
/// и с разными вещами на теле, лицом к +Z; камера ожидания смотрит на них (если не задана `MRPG_CAM`).
fn gear_showcase(mut commands: Commands, mut cams: Query<&mut Transform, With<WaitingCamera>>, models: Option<Res<crate::icons::ItemModels>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Ok(mut cam) = cams.single_mut() else { return };
    *done = true;
    let v: Vec<f32> = std::env::var("MRPG_GEAR_SHOWCASE").unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let o = if v.len() == 3 { Vec3::new(v[0], v[1], v[2]) } else { crate::level::place_pos("agora").unwrap_or_default() + Vec3::new(4.0, 0.0, 4.0) };
    let id = |n: &str| crate::items::ItemBaseId::find(n).map(|b| b.0);
    // Слоты: 0 голова, 1 торс, 2 руки, 3 ноги, 4 ступни, 5 щит, 6 оружие в руке, 7 оружие убрано,
    // 8 шея, 9 перстень, 10 стрелы (колчан).
    let sets: [&[(&str, u8)]; 5] = [
        &[("Спата", 7), ("Нательный крест", 8), ("Серебряный перстень", 9)],
        &[("Стёганая туника", 1), ("Кожаные перчатки", 2), ("Кожаные сапоги", 4), ("Гуннский составной лук", 7), ("Стрелы", 10)],
        &[("Спангенхельм", 0), ("Кольчуга", 1), ("Кожаные перчатки", 2), ("Железные поножи", 3), ("Кожаные сапоги", 4), ("Круглый щит", 5), ("Готская узорчатая спата", 6)],
        &[("Гребенчатый шлем", 0), ("Ламеллярный панцирь", 1), ("Ламеллярные наручи", 2), ("Железные поножи", 3), ("Кожаные сапоги", 4), ("Контос", 7), ("Франциска", 6)],
        &[("Гребенчатый шлем", 0), ("Стёганая туника", 1), ("Ламеллярные наручи", 2), ("Кожаные сапоги", 4), ("Овальный щит", 5), ("Шипастая булава", 6)],
    ];
    info!("Витрина доспехов у {o}");
    if std::env::var("MRPG_CAM").is_err() {
        // `MRPG_GEAR_FOCUS=x` — крупно между фигурами (x — номер фигуры, можно дробный).
        *cam = match std::env::var("MRPG_GEAR_FOCUS").ok().and_then(|s| s.parse::<f32>().ok()) {
            Some(f) => Transform::from_translation(o + Vec3::new(f * 0.95, 1.45, 1.7)).looking_at(o + Vec3::new(f * 0.95, 1.1, 0.0), Vec3::Y),
            None => Transform::from_translation(o + Vec3::new(1.9, 1.35, 3.6)).looking_at(o + Vec3::new(1.9, 1.0, 0.0), Vec3::Y),
        };
    }
    for (i, set) in sets.iter().enumerate() {
        let mut g = Gear::default();
        let mut in_hand = None;
        for (name, slot) in set.iter() {
            let b = id(name);
            match slot {
                0 => g.head = b,
                1 => g.chest = b,
                2 => g.hands = b,
                3 => g.legs = b,
                4 => g.feet = b,
                5 => g.off_hand = b,
                6 => in_hand = b,
                7 => {
                    g.main_hand = b;
                    g.holstered = true;
                }
                8 => g.neck = b,
                9 => g.ring = b,
                _ => g.quiver = b,
            }
        }
        if in_hand.is_some() {
            (g.main_hand, g.holstered) = (in_hand, false);
        }
        let kind = in_hand.map_or(WeaponKind::Fists, |b| WeaponKind::from_item_name(crate::items::ItemBaseId(b).base().name));
        let p = o + Vec3::X * (i as f32 * 0.95);
        commands
            .spawn((Transform::from_translation(p).with_rotation(Quat::from_rotation_y(std::f32::consts::PI)), Visibility::default()))
            .with_children(|c| {
                c.spawn((BodyRig, ShowcasePose(kind), worn::Worn::new(worn::Extras::of(&g)), NeedsRig(outfit_of(ActorKind::Player, 7 + i as u64, &g)), Transform::default(), Visibility::default()));
                if let Some(kind) = shield_of(ActorKind::Player, false, &g) {
                    c.spawn((NeedsShield(kind), Transform::from_xyz(-0.36, 1.0, -0.12).with_rotation(Quat::from_rotation_y(1.2)), Visibility::default()));
                }
                let weapon = c.spawn((ShowcaseWeapon, Transform::default(), Visibility::default())).id();
                let mut cmds = c.commands();
                worn::set_weapon_model(&mut cmds, weapon, kind, in_hand.map(crate::items::ItemBaseId), 200, models.as_deref());
            });
    }
}

/// `MRPG_PEOPLE_SHOWCASE=x,y,z` (или пусто — у агоры): для снимков ряд горожан разного облика
/// (бороды, усы, бритые, кожа, сложение), лицом к +Z. `MRPG_PEOPLE_WALK=1` — шагают на месте,
/// `MRPG_PEOPLE_FOCUS=x` — камера крупно у фигуры x, `MRPG_PEOPLE_SEED` — другой набор людей,
/// `MRPG_PEOPLE_FACES=1` — все варианты лица подряд.
#[derive(Component)]
struct ShowcasePerson(usize);

fn people_showcase(mut commands: Commands, mut cams: Query<&mut Transform, With<WaitingCamera>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Ok(mut cam) = cams.single_mut() else { return };
    *done = true;
    let v: Vec<f32> = std::env::var("MRPG_PEOPLE_SHOWCASE").unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let o = if v.len() == 3 { Vec3::new(v[0], v[1], v[2]) } else { crate::level::place_pos("agora").unwrap_or_default() + Vec3::new(4.0, 0.0, 4.0) };
    let seed: u64 = std::env::var("MRPG_PEOPLE_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    use crate::level::Role;
    let roles = [Role::Householder, Role::Craftsman, Role::Sailor, Role::Noble, Role::Farmer, Role::Priest, Role::Shopkeeper, Role::Servant];
    const GAP: f32 = 0.85;
    if std::env::var("MRPG_CAM").is_err() {
        let mid = (roles.len() - 1) as f32 * 0.5;
        *cam = match std::env::var("MRPG_PEOPLE_FOCUS").ok().and_then(|s| s.parse::<f32>().ok()) {
            Some(f) => Transform::from_translation(o + Vec3::new(f * GAP, 1.6, 1.25)).looking_at(o + Vec3::new(f * GAP, 1.5, 0.0), Vec3::Y),
            None => Transform::from_translation(o + Vec3::new(mid * GAP, 1.3, 4.4)).looking_at(o + Vec3::new(mid * GAP, 0.95, 0.0), Vec3::Y),
        };
    }
    // `MRPG_PEOPLE_FACES=1` — вместо горожан бритые одинаковые люди со всеми вариантами лица подряд.
    // Число — с какого лица начать: тогда трое стоят дугой лицом к камере (крупный план).
    let faces = std::env::var("MRPG_PEOPLE_FACES").is_ok();
    let first: Option<usize> = std::env::var("MRPG_PEOPLE_FACES").ok().and_then(|s| s.parse().ok());
    let count = if faces { crate::appearance::FACES.len() } else { roles.len() };
    for i in 0..count {
        if let Some(f) = first {
            if i < f || i >= f + 3 {
                continue;
            }
            let k = (i - f) as f32 - 1.0;
            let look = crate::appearance::Look { face: i as u8, beard: 4, hair: 0, ..crate::appearance::Look::PLAYER_DEFAULT };
            let outfit = crate::residents::outfit(Role::Householder, 100).with_look(look);
            // Камера в точке o + (0, 1.6, 1.3) смотрит на −Z; люди на дуге радиусом 1,3 м.
            let eye = o + Vec3::new(0.0, 0.0, 1.3);
            let a = k * 0.36;
            let at = eye + Vec3::new(a.sin(), 0.0, -a.cos()) * 1.3;
            commands
                .spawn((Transform::from_translation(at).with_rotation(Quat::from_rotation_y(std::f32::consts::PI - a)), Visibility::default()))
                .with_children(|c| {
                    c.spawn((BodyRig, ShowcasePerson(i), NeedsRig(outfit), Transform::default(), Visibility::default()));
                });
            *cam = Transform::from_translation(eye + Vec3::Y * 1.62).looking_at(eye + Vec3::new(0.0, 1.55, -1.3), Vec3::Y);
            continue;
        }
        let outfit = if faces {
            let look = crate::appearance::Look { face: i as u8, beard: 4, hair: 1, ..crate::appearance::Look::PLAYER_DEFAULT };
            crate::residents::outfit(Role::Householder, 100).with_look(look)
        } else {
            crate::residents::outfit(roles[i], 100 + i as u64 * 7 + seed * 131)
        };
        commands
            .spawn((Transform::from_translation(o + Vec3::X * (i as f32 * GAP)).with_rotation(Quat::from_rotation_y(std::f32::consts::PI)), Visibility::default()))
            .with_children(|c| {
                c.spawn((BodyRig, ShowcasePerson(i), NeedsRig(outfit), Transform::default(), Visibility::default()));
            });
    }
}

fn pose_people(time: Res<Time>, mut rigs: Query<(&ShowcasePerson, &mut RigPose)>) {
    let walk = std::env::var("MRPG_PEOPLE_WALK").is_ok();
    // Для снимков походки: `MRPG_PEOPLE_PHASE` — доля цикла шага (одинаковая у всех фигур).
    let phase: Option<f32> = std::env::var("MRPG_PEOPLE_PHASE").ok().and_then(|s| s.parse().ok());
    let faces = std::env::var("MRPG_PEOPLE_FACES").is_ok();
    let now = time.elapsed_secs();
    for (p, mut pose) in &mut rigs {
        let k = p.0 as f32;
        let walk_phase = match phase {
            Some(u) => (u + k * 0.13) * std::f32::consts::TAU,
            None => now * 1.3 / rig::step_cycle(1.3) * std::f32::consts::TAU + k,
        };
        // Для сравнения лиц все стоят одинаково и смотрят прямо.
        let t = if faces { 0.0 } else { now + k * 3.7 };
        *pose = rig::solve_pose(&PoseInput { time: t, walk: if walk { 1.0 } else { 0.0 }, walk_phase, ..default() }).pose;
    }
}

/// Фигура витрины и вид оружия у неё в руке. `MRPG_GEAR_POSE=присед,взгляд,наклон` — поза
/// (для проверки одежды на изгибах).
#[derive(Component)]
struct ShowcasePose(WeaponKind);

#[derive(Component)]
struct ShowcaseWeapon;

fn pose_showcase(
    time: Res<Time>,
    figures: Query<&Children>,
    mut rigs: Query<(&ShowcasePose, &mut RigPose, &ChildOf)>,
    mut weapons: Query<&mut Transform, With<ShowcaseWeapon>>,
) {
    let v: Vec<f32> = std::env::var("MRPG_GEAR_POSE").unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let get = |i: usize| v.get(i).copied().unwrap_or(0.0);
    let now = time.elapsed_secs();
    for (fig, mut pose, parent) in &mut rigs {
        let (crouch, pitch) = (get(0), get(1));
        let eye = 1.65 - 0.6 * crouch;
        let aim = Quat::from_rotation_x(pitch * 0.6);
        let (hilt, dir) = combat::weapon_pose(&Action::Idle, fig.0, now);
        let sword = Transform::from_translation(Vec3::Y * (eye - 0.25) + aim * hilt).with_rotation(Quat::from_rotation_arc(Vec3::Y, aim * dir));
        let (right, left) = weapon_grips(fig.0);
        let armed = fig.0 != WeaponKind::Fists;
        let input = PoseInput {
            crouch,
            pitch,
            lean: get(2),
            time: now,
            right_grip: armed.then(|| sword.transform_point(right)),
            left_grip: left.filter(|_| armed).map(|l| sword.transform_point(l)),
            right_axis: armed.then(|| sword.rotation * Vec3::Y),
            left_axis: left.filter(|_| armed).map(|_| sword.rotation * Vec3::Y),
            ..Default::default()
        };
        *pose = rig::solve_pose(&input).pose;
        if let Ok(children) = figures.get(parent.parent()) {
            for c in children.iter() {
                if let Ok(mut t) = weapons.get_mut(c) {
                    *t = sword;
                }
            }
        }
    }
}

/// Застывшее действие фигуры на витрине боя.
#[derive(Component)]
struct Frozen(Action);

fn freeze_showcase(mut q: Query<(&mut RemoteActor, &Frozen)>) {
    for (mut r, f) in &mut q {
        r.fighter.action = f.0;
    }
}

/// `MRPG_DUEL_SHOWCASE=row|pairs` (место — `MRPG_DUEL_AT=x,y,z`, иначе у агоры): для снимков
/// застывшие бойцы в фазах боя. `row` — шестеро лицом к камере (замах сверху справа, удар
/// сверху, укол, окно парирования, отбит, рипост); `pairs` — три пары лицом друг к другу
/// (удар в парирование, отбитый под рипостом, укол в щит).
fn duel_showcase(mut commands: Commands, mut index: ResMut<RemoteIndex>, mut cams: Query<&mut Transform, With<WaitingCamera>>, mut done: Local<bool>) {
    use std::f32::consts::FRAC_PI_2;
    if *done {
        return;
    }
    let Ok(mut cam) = cams.single_mut() else { return };
    *done = true;
    let v: Vec<f32> = std::env::var("MRPG_DUEL_AT").unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let o = if v.len() == 3 { Vec3::new(v[0], v[1], v[2]) } else { crate::level::place_pos("agora").unwrap_or_default() + Vec3::new(4.0, 0.0, 4.0) };
    let mail = |n: &str| crate::items::ItemBaseId::find(n).map(|b| b.0);
    let gear = |shield: bool| Gear {
        head: mail("Спангенхельм"),
        chest: mail("Кольчуга"),
        hands: mail("Кожаные перчатки"),
        feet: mail("Кожаные сапоги"),
        off_hand: if shield { mail("Овальный щит") } else { None },
        ..default()
    };
    let mut spawn = |id: u64, kind: ActorKind, pos: Vec3, yaw: f32, weapon: WeaponKind, shield: bool, spec: &str| {
        let action = combat::showcase_action(spec, weapon).unwrap_or_default();
        let snap = ActorSnapshot {
            id,
            kind,
            pos: pos.into(),
            yaw,
            pitch: 0.0,
            crouching: false,
            weapon,
            action,
            hp: 100.0,
            max_hp: 100.0,
            shield,
            stamina: 100.0,
            max_stamina: 100.0,
            action_speed: 1.0,
            move_mult: 1.0,
            finish: 230,
            gear: if kind == ActorKind::Player { gear(shield) } else { Gear::default() },
        };
        spawn_remote(&mut commands, &mut index, &snap);
        if let Some(e) = index.0.get(&id) {
            commands.entity(*e).insert(Frozen(action));
        }
    };
    let layout = std::env::var("MRPG_DUEL_SHOWCASE").unwrap_or_default();
    if layout == "pairs" {
        let pairs: [(&str, ActorKind, WeaponKind, bool, &str, ActorKind, WeaponKind, bool); 3] = [
            ("strike:ur:0.4", ActorKind::Player, WeaponKind::Spatha, false, "guard:o:0.2", ActorKind::Player, WeaponKind::Spatha, false),
            ("recoil:r:0.15", ActorKind::GothWarrior, WeaponKind::Axe, false, "riposte:l:0.8", ActorKind::Player, WeaponKind::Spatha, false),
            ("strike:t:0.3", ActorKind::AlanLancer, WeaponKind::Spear, false, "guard:o:0.8", ActorKind::Stratiotes, WeaponKind::Spatha, true),
        ];
        for (i, (sa, ka, wa, sha, sb, kb, wb, shb)) in pairs.into_iter().enumerate() {
            let a = o + Vec3::X * (i as f32 * 3.6);
            let gap = if wa == WeaponKind::Spear { 2.0 } else { 1.25 };
            spawn(9000 + i as u64 * 2, ka, a, -FRAC_PI_2, wa, sha, sa);
            spawn(9001 + i as u64 * 2, kb, a + Vec3::X * gap, FRAC_PI_2, wb, shb, sb);
        }
        if std::env::var("MRPG_CAM").is_err() {
            let f: f32 = std::env::var("MRPG_DUEL_FOCUS").ok().and_then(|s| s.parse().ok()).unwrap_or(-1.0);
            *cam = if f >= 0.0 {
                let c = o + Vec3::new(f * 3.6 + 0.7, 1.15, 0.0);
                Transform::from_translation(c + Vec3::new(-0.6, 0.35, 2.6)).looking_at(c, Vec3::Y)
            } else {
                Transform::from_translation(o + Vec3::new(4.3, 1.9, 6.8)).looking_at(o + Vec3::new(4.3, 1.0, 0.0), Vec3::Y)
            };
        }
    } else {
        let row = ["windup:ur:0.95", "strike:o:0.4", "strike:t:0.6", "guard:o:0.2", "recoil:r:0.12", "riposte:l:0.85"];
        for (i, spec) in row.iter().enumerate() {
            spawn(9100 + i as u64, ActorKind::Player, o + Vec3::X * (i as f32 * 1.3), std::f32::consts::PI, WeaponKind::Spatha, false, spec);
        }
        if std::env::var("MRPG_CAM").is_err() {
            let f: f32 = std::env::var("MRPG_DUEL_FOCUS").ok().and_then(|s| s.parse().ok()).unwrap_or(-1.0);
            *cam = if f >= 0.0 {
                // Крупно одного бойца, чуть сбоку — чтобы клинок, смотрящий в камеру, был виден.
                let c = o + Vec3::new(f * 1.3, 1.25, 0.0);
                Transform::from_translation(c + Vec3::new(1.1, 0.25, 2.2)).looking_at(c, Vec3::Y)
            } else {
                Transform::from_translation(o + Vec3::new(3.25, 1.5, 5.2)).looking_at(o + Vec3::new(3.25, 1.05, 0.0), Vec3::Y)
            };
        }
    }
}

/// Другой игрок надел или снял доспех, щит — переодеваем модель.
fn refresh_remote_gear(
    mut commands: Commands,
    mut remotes: Query<(Entity, &mut RemoteActor, &Children)>,
    mut bodies: Query<&mut worn::Worn, With<BodyRig>>,
    shields: Query<(), With<RemoteShield>>,
    models: Option<Res<CharacterModels>>,
) {
    // Пока модели не загрузились, тело всё равно ещё не собрано — подождём.
    if models.is_none() {
        return;
    }
    for (e, mut r, children) in &mut remotes {
        if r.gear == r.shown_gear {
            continue;
        }
        let (old, new) = (r.shown_gear, r.gear);
        r.shown_gear = new;
        let outfit = outfit_of(r.kind, r.id, &new);
        let rebuild = outfit != outfit_of(r.kind, r.id, &old);
        for child in children.iter() {
            let Ok(mut worn) = bodies.get_mut(child) else { continue };
            worn.want = worn::Extras::of(&new);
            if rebuild {
                // Вещи висят на суставах и уходят вместе со скелетом.
                worn.reset();
                commands.entity(child).despawn_related::<Children>().remove::<rig::Rig>().insert(NeedsRig(outfit));
            }
        }
        let has = r.fighter.has_shield;
        if shield_of(r.kind, has, &new) != shield_of(r.kind, has, &old) {
            for child in children.iter().filter(|c| shields.contains(*c)) {
                commands.entity(child).despawn();
            }
            if let Some(kind) = shield_of(r.kind, has, &new) {
                commands.entity(e).with_children(|p| spawn_remote_shield(p, kind));
            }
        }
    }
}

fn send_state(mut client: ResMut<RenetClient>, mut timer: ResMut<SendTimer>, time: Res<Time>, local: Query<(&LocalPlayer, &Transform)>) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let Ok((p, t)) = local.single() else { return };
    let msg = ClientMessage::PlayerState { pos: t.translation.into(), yaw: p.yaw, pitch: p.pitch, crouching: p.crouching, sprinting: p.sprinting };
    client.send_message(CH_UNRELIABLE, encode(&msg));
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate_remote(
    time: Res<Time>,
    assets: Res<CharacterAssets>,
    models: Option<Res<CharacterModels>>,
    mut remotes: Query<(&mut RemoteActor, &mut Transform, &Children, Option<&Frozen>), (Without<RemoteSword>, Without<RemoteShield>)>,
    mut swords: Query<(&mut Transform, &Children), (With<RemoteSword>, Without<RemoteActor>, Without<RemoteShield>)>,
    mut shields: Query<&mut Transform, (With<RemoteShield>, Without<RemoteActor>, Without<RemoteSword>)>,
    mut glows: Query<(&mut Transform, &mut Visibility), (With<RiposteGlow>, Without<RemoteActor>, Without<RemoteSword>, Without<RemoteShield>)>,
    hints: Option<Res<crate::fight::CombatHints>>,
    mut rigs: Query<&mut RigPose, With<BodyRig>>,
    mut blades: Query<(&Blade, &mut MeshMaterial3d<StandardMaterial>)>,
    mut tints: ResMut<rig::BladeTints>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let t = 1.0 - (-15.0 * dt).exp();
    let _ = models;
    for (mut r, mut transform, children, frozen) in &mut remotes {
        // Застывший для снимка боец не живёт по таймеру: иначе замах сам переходит в удар.
        match frozen {
            Some(f) => r.fighter.action = f.0,
            None => { r.fighter.tick(dt); }
        }
        if transform.translation.distance(r.target_pos) > 5.0 {
            transform.translation = r.target_pos;
            r.last_pos = r.target_pos;
        } else {
            transform.translation = transform.translation.lerp(r.target_pos, t);
        }
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(r.target_yaw), t);

        // Шаг: фаза идёт по пройденному пути, назад — в обратную сторону.
        let moved = transform.translation - r.last_pos;
        r.last_pos = transform.translation;
        let flat = Vec3::new(moved.x, 0.0, moved.z);
        let local = transform.rotation.inverse() * flat;
        let speed = if dt > 0.0 { flat.length() / dt } else { 0.0 };
        let back = if local.z > 0.3 * flat.length() { -1.0 } else { 1.0 };
        // Скорость сглаживаем (снимки приходят рывками); шаг или бег выбирается по ней.
        r.speed += (speed.min(9.0) - r.speed) * (1.0 - (-6.0 * dt).exp());
        r.walk_phase += back * flat.length() / rig::step_cycle(r.speed) * std::f32::consts::TAU;
        // Вес походки: уже от медленного шага ноги идут полным шагом (иначе стопы скользят).
        let walk_target = (speed / 0.9).min(1.0);
        r.walk += (walk_target - r.walk) * (1.0 - (-8.0 * dt).exp());
        let crouch_target = if r.crouching { 1.0 } else { 0.0 };
        r.crouch += (crouch_target - r.crouch) * (1.0 - (-10.0 * dt).exp());

        let eye = 1.65 - 0.6 * r.crouch;
        let aim = Quat::from_rotation_x(r.pitch * 0.6);
        let (want_hilt, want_dir) = combat::weapon_pose(&r.fighter.action, r.fighter.weapon, now);
        // Сглаживаем смену фаз (комбо, отбитый удар, снимок сервера пришёл с опозданием).
        let k = 1.0 - (-30.0 * dt).exp();
        let (hilt, dir) = match r.weapon_smooth {
            Some((h, d)) if h.distance(want_hilt) < 0.8 => (h.lerp(want_hilt, k), d.lerp(want_dir, k).normalize_or(want_dir)),
            _ => (want_hilt, want_dir),
        };
        r.weapon_smooth = Some((hilt, dir));
        // Ответный удар после парирования подсвечиваем.
        let riposte = matches!(r.fighter.action, Action::Windup { riposte: true, .. } | Action::Strike { riposte: true, .. }) && hints.as_ref().is_none_or(|h| h.0);
        // Корпус работает вместе с клинком: на замахе торс закручен в сторону удара, в ударе
        // раскручивается, сверху — наклон вперёд, укол — выпад, отбитый — откинут назад.
        let d = aim * dir;
        let (twist, lean) = match r.fighter.action {
            Action::Windup { swing, .. } | Action::Strike { swing, .. } | Action::Recovery { swing, .. } => {
                let striking = matches!(r.fighter.action, Action::Strike { .. });
                let lean = match swing.class() {
                    combat::SwingClass::Overhead | combat::SwingClass::Diagonal => -0.25 * d.z.abs().min(1.0),
                    combat::SwingClass::Thrust if striking => -0.22,
                    combat::SwingClass::Rising => 0.04,
                    _ => -0.06,
                };
                (-d.x * 0.6, lean)
            }
            Action::Recoil { t, dur } => (0.12, 0.3 * (1.0 - t / dur).max(0.0) + (now * 25.0).sin() * 0.03 * (1.0 - t / dur).max(0.0)),
            Action::ParryRecovery { .. } => (0.08, 0.1),
            Action::Stagger { t, .. } => (0.0, 0.32 + (now * 25.0).sin() * 0.05 * (1.0 - t).max(0.0)),
            Action::GuardStagger { .. } => (0.15, 0.12),
            Action::Flinch { .. } => (0.1, 0.15),
            Action::Guard { .. } => (0.2, 0.05),
            _ => (0.0, 0.0),
        };

        let mut sword_entity = None;
        let mut shield_entity = None;
        let mut rig_entity = None;
        let mut glow_entity = None;
        for child in children.iter() {
            if glows.contains(child) {
                glow_entity = Some(child);
            } else if swords.contains(child) {
                sword_entity = Some(child);
            } else if shields.contains(child) {
                shield_entity = Some(child);
            } else if rigs.contains(child) {
                rig_entity = Some(child);
            }
        }

        // Меч и щит ставятся как раньше (по ним считается бой), руки тянутся к ним.
        let mut sword_tf = Transform::from_translation(Vec3::Y * (eye - 0.25) + aim * hilt).with_rotation(Quat::from_rotation_arc(Vec3::Y, aim * dir));
        let mut shield_tf = None;
        if let Some(e) = shield_entity
            && let Ok(mut shield) = shields.get_mut(e)
        {
            let (pos, rot) = if matches!(r.fighter.action, Action::Guard { .. } | Action::GuardStagger { .. }) {
                (Vec3::new(-0.05, 1.25, -0.5), Quat::IDENTITY)
            } else {
                (Vec3::new(-0.38, 1.0, -0.22), Quat::from_rotation_y(1.0))
            };
            let pos = pos - Vec3::Y * 0.6 * r.crouch;
            shield.translation = shield.translation.lerp(pos, t * 1.5);
            shield.rotation = shield.rotation.slerp(rot, t * 1.5);
            shield_tf = Some(*shield);
        }
        let (right, left) = weapon_grips(r.fighter.weapon);
        // Без оружия и без щита левый кулак тоже поднят (стойка кулачного бойца).
        let fist_l = (r.fighter.weapon == WeaponKind::Fists && shield_tf.is_none()).then(|| {
            let guard = matches!(r.fighter.action, Action::Windup { .. } | Action::Strike { .. } | Action::Recovery { .. });
            Vec3::Y * (eye - 0.25) + aim * if guard { Vec3::new(-0.16, -0.08, -0.28) } else { Vec3::new(-0.2, -0.18, -0.3) }
        });
        let mut input = PoseInput {
            crouch: r.crouch,
            walk_phase: r.walk_phase,
            walk: r.walk,
            speed: r.speed,
            pitch: r.pitch,
            twist,
            lean,
            time: now + r.id as f32,
            right_grip: Some(sword_tf.transform_point(right)),
            left_grip: left.map(|l| sword_tf.transform_point(l)).or(shield_tf.map(|s| s.transform_point(SHIELD_GRIP))).or(fist_l),
            // Кулак обхватывает рукоять/древко по оси оружия, щит — за горизонтальную ручку.
            right_axis: Some(sword_tf.rotation * Vec3::Y),
            left_axis: if left.is_some() { Some(sword_tf.rotation * Vec3::Y) } else { shield_tf.map(|s| s.rotation * Vec3::NEG_X).or(fist_l.map(|_| aim * Vec3::new(0.15, 0.25, -1.0).normalize())) },
        };
        let mut solved = rig::solve_pose(&input);
        // Если руки не дотягиваются до рукояти, меч придвигается к ним (только внешний вид,
        // бой считается по исходному положению).
        for _ in 0..4 {
            let mut miss = solved.right_grip - input.right_grip.unwrap_or_default();
            if left.is_some() {
                miss = (miss + solved.left_grip - input.left_grip.unwrap_or_default()) * 0.6;
            }
            if miss.length() < 0.005 {
                break;
            }
            sword_tf.translation += miss;
            input.right_grip = Some(sword_tf.transform_point(right));
            if let Some(l) = left {
                input.left_grip = Some(sword_tf.transform_point(l));
            }
            solved = rig::solve_pose(&input);
        }
        if let Some(e) = rig_entity
            && let Ok(mut pose) = rigs.get_mut(e)
        {
            *pose = solved.pose;
        }
        if let Some(e) = glow_entity
            && let Ok((mut gt, mut gv)) = glows.get_mut(e)
        {
            let glow = riposte.then(|| riposte_glow_transform(&sword_tf, r.fighter.weapon)).flatten();
            *gv = if glow.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            if let Some(g) = glow {
                *gt = g;
            }
        }
        if let Some(e) = sword_entity
            && let Ok((mut sword, blade_children)) = swords.get_mut(e)
        {
            *sword = sword_tf;
            for bc in blade_children.iter() {
                if let Ok((b, mut m)) = blades.get_mut(bc) {
                    m.0 = if riposte { assets.blade_red.clone() } else { tints.get(&mut materials, &b.0, r.finish) };
                }
            }
        }
    }
}
