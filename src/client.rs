//! Клиент: графика, подключение к серверу, отображение других бойцов и тел.

use crate::combat::{self, Action, Fighter, WeaponKind};
use crate::fight::{self, CombatFeedback, LocalFighter};
use crate::player::{LocalPlayer, PlayerPlugin, spawn_local_player};
use crate::protocol::*;
use crate::{graphics, hud, level};
use bevy::prelude::*;
use bevy_renet::netcode::{ClientAuthentication, NetcodeClientPlugin, NetcodeClientTransport, NetcodeErrorEvent};
use bevy_renet::{RenetClient, RenetClientPlugin, client_connected};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::SystemTime;

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
    #[allow(dead_code)] // понадобится для обыска и прицеливания
    pub id: u64,
    pub kind: ActorKind,
    pub fighter: Fighter,
    pub pitch: f32,
    target_pos: Vec3,
    target_yaw: f32,
    crouching: bool,
}

/// Часть модели, которая сжимается при приседании и наклоняется при оглушении.
#[derive(Component)]
struct RemoteBody;

/// Меч в руке другого бойца.
#[derive(Component)]
struct RemoteSword;

/// Щит в руке рыцаря.
#[derive(Component)]
struct RemoteShield;

/// Лезвие меча (меняет материал, когда удар нельзя парировать).
#[derive(Component)]
pub struct Blade;

/// Тело на земле.
#[derive(Component)]
#[allow(dead_code)] // используется окном обыска (тред про инвентарь)
pub struct CorpseMarker {
    pub corpse_id: u64,
    pub kind: ActorKind,
}

#[derive(Resource, Default)]
pub struct RemoteIndex(pub HashMap<u64, Entity>);

#[derive(Resource, Default)]
struct CorpseIndex(HashMap<u64, Entity>);

#[derive(Resource)]
struct SendTimer(Timer);

#[derive(Resource, Clone)]
pub struct CharacterAssets {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    helmet: Handle<Mesh>,
    visor: Handle<Mesh>,
    shield: Handle<Mesh>,
    pub blade_short: Handle<Mesh>,
    pub blade_long: Handle<Mesh>,
    pub guard: Handle<Mesh>,
    pub grip: Handle<Mesh>,
    pub steel: Handle<StandardMaterial>,
    pub blade_red: Handle<StandardMaterial>,
    pub leather: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    swordsman_cloth: Handle<StandardMaterial>,
    heavy_cloth: Handle<StandardMaterial>,
    shield_paint: Handle<StandardMaterial>,
}

pub fn run(server_addr: SocketAddr) {
    let client_id = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let socket = UdpSocket::bind("0.0.0.0:0").expect("не удалось открыть сокет клиента");
    let auth = ClientAuthentication::Unsecure { protocol_id: PROTOCOL_ID, client_id, server_addr, user_data: None };
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let transport = NetcodeClientTransport::new(current_time, auth, socket).expect("не удалось создать подключение");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Medieval RPG — прототип".into(), ..default() }),
            ..default()
        }))
        .add_plugins((RenetClientPlugin, NetcodeClientPlugin, PlayerPlugin, fight::FightPlugin, hud::HudPlugin, graphics::GraphicsPlugin))
        .add_plugins(crate::loot_client::LootClientPlugin)
        .insert_resource(RenetClient::new(connection_config()))
        .insert_resource(transport)
        .insert_resource(NetState { server_addr: Some(server_addr), ..default() })
        .insert_resource(level::level_colliders())
        .insert_resource(RemoteIndex::default())
        .insert_resource(CorpseIndex::default())
        .insert_resource(SendTimer(Timer::from_seconds(1.0 / 30.0, TimerMode::Repeating)))
        .insert_resource(ClearColor(Color::srgb(0.55, 0.5, 0.48)))
        .insert_resource(GlobalAmbientLight { color: Color::srgb(0.7, 0.72, 0.85), brightness: 250.0, ..default() })
        .add_observer(on_netcode_error)
        .add_systems(Startup, (level::spawn_level, setup_character_assets, spawn_waiting_camera))
        .add_systems(
            Update,
            ((receive_reliable, receive_snapshots).chain(), send_state, animate_remote).run_if(client_connected).before(fight::FightSet),
        )
        .run();
}

fn on_netcode_error(event: On<NetcodeErrorEvent>, mut state: ResMut<NetState>) {
    error!("Ошибка сети: {}", event.0);
    state.error = Some(event.0.to_string());
}

/// Камера, пока сервер не прислал точку появления.
#[derive(Component)]
struct WaitingCamera;

fn spawn_waiting_camera(mut commands: Commands) {
    commands.spawn((WaitingCamera, Camera3d::default(), Transform::from_xyz(0.0, 25.0, 70.0).looking_at(Vec3::new(0.0, 5.0, 0.0), Vec3::Y)));
}

fn setup_character_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let steel = materials.add(StandardMaterial { base_color: Color::srgb(0.72, 0.74, 0.78), metallic: 0.9, perceptual_roughness: 0.3, ..default() });
    commands.insert_resource(CharacterAssets {
        body: meshes.add(Capsule3d::new(0.3, 0.9)),
        head: meshes.add(Sphere::new(0.2)),
        helmet: meshes.add(Cylinder::new(0.23, 0.36)),
        visor: meshes.add(Cuboid::new(0.3, 0.05, 0.1)),
        shield: meshes.add(Cuboid::new(0.6, 0.8, 0.06)),
        blade_short: meshes.add(Cuboid::new(0.05, WeaponKind::ArmingSword.stats().reach, 0.012)),
        blade_long: meshes.add(Cuboid::new(0.07, WeaponKind::Greatsword.stats().reach, 0.015)),
        guard: meshes.add(Cuboid::new(0.24, 0.035, 0.04)),
        grip: meshes.add(Cuboid::new(0.035, 0.2, 0.035)),
        steel: steel.clone(),
        blade_red: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.2, 0.1),
            emissive: LinearRgba::rgb(6.0, 0.6, 0.2),
            ..default()
        }),
        leather: materials.add(Color::srgb(0.25, 0.15, 0.08)),
        dark: materials.add(Color::srgb(0.08, 0.08, 0.08)),
        swordsman_cloth: materials.add(StandardMaterial { base_color: Color::srgb(0.12, 0.2, 0.45), perceptual_roughness: 0.9, ..default() }),
        heavy_cloth: materials.add(StandardMaterial { base_color: Color::srgb(0.4, 0.05, 0.05), perceptual_roughness: 0.9, ..default() }),
        shield_paint: materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.8, 0.65), perceptual_roughness: 0.7, ..default() }),
    });
}

fn color_for(id: u64) -> Color {
    let hue = (id.wrapping_mul(2654435761) % 360) as f32;
    Color::hsl(hue, 0.6, 0.5)
}

/// Создаёт меч: корень в рукояти, клинок вдоль +Y.
pub fn spawn_sword(parent: &mut ChildSpawnerCommands, assets: &CharacterAssets, weapon: WeaponKind, marker: impl Bundle) {
    let reach = weapon.stats().reach;
    let blade = if weapon == WeaponKind::Greatsword { assets.blade_long.clone() } else { assets.blade_short.clone() };
    parent.spawn((marker, Transform::default(), Visibility::default())).with_children(|s| {
        s.spawn((Blade, Mesh3d(blade), MeshMaterial3d(assets.steel.clone()), Transform::from_xyz(0.0, reach / 2.0 + 0.02, 0.0)));
        s.spawn((Mesh3d(assets.guard.clone()), MeshMaterial3d(assets.steel.clone()), Transform::default()));
        s.spawn((Mesh3d(assets.grip.clone()), MeshMaterial3d(assets.leather.clone()), Transform::from_xyz(0.0, -0.11, 0.0)));
    });
}

fn spawn_remote(commands: &mut Commands, assets: &CharacterAssets, materials: &mut Assets<StandardMaterial>, index: &mut RemoteIndex, snap: &ActorSnapshot) {
    let pos: Vec3 = snap.pos.into();
    let cloth = match snap.kind {
        ActorKind::Player => materials.add(color_for(snap.id)),
        ActorKind::Swordsman => assets.swordsman_cloth.clone(),
        ActorKind::HeavyKnight => assets.heavy_cloth.clone(),
    };
    let scale = if snap.kind == ActorKind::HeavyKnight { 1.12 } else { 1.0 };
    let mut fighter = Fighter::new(snap.weapon, snap.max_hp, snap.kind == ActorKind::Swordsman);
    fighter.action = snap.action;
    fighter.hp = snap.hp;
    let entity = commands
        .spawn((
            RemoteActor { id: snap.id, kind: snap.kind, fighter, pitch: snap.pitch, target_pos: pos, target_yaw: snap.yaw, crouching: snap.crouching },
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(snap.yaw)),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((RemoteBody, Transform::from_scale(Vec3::splat(scale)), Visibility::default())).with_children(|b| {
                b.spawn((Mesh3d(assets.body.clone()), MeshMaterial3d(cloth.clone()), Transform::from_xyz(0.0, 0.75, 0.0)));
                if snap.kind == ActorKind::Player {
                    b.spawn((Mesh3d(assets.head.clone()), MeshMaterial3d(cloth.clone()), Transform::from_xyz(0.0, 1.6, 0.0)));
                } else {
                    // Латы: стальной шлем-ведро и плечи.
                    b.spawn((Mesh3d(assets.helmet.clone()), MeshMaterial3d(assets.steel.clone()), Transform::from_xyz(0.0, 1.6, 0.0)));
                    b.spawn((Mesh3d(assets.body.clone()), MeshMaterial3d(assets.steel.clone()), Transform::from_xyz(0.0, 1.05, 0.0).with_scale(Vec3::new(1.08, 0.35, 1.05))));
                }
                b.spawn((Mesh3d(assets.visor.clone()), MeshMaterial3d(assets.dark.clone()), Transform::from_xyz(0.0, 1.63, -0.2)));
            });
            spawn_sword(p, assets, snap.weapon, RemoteSword);
            if snap.kind == ActorKind::Swordsman {
                p.spawn((RemoteShield, Mesh3d(assets.shield.clone()), MeshMaterial3d(assets.shield_paint.clone()), Transform::from_xyz(-0.4, 1.1, -0.1)));
            }
        })
        .id();
    index.0.insert(snap.id, entity);
}

fn spawn_corpse(commands: &mut Commands, assets: &CharacterAssets, index: &mut CorpseIndex, corpse_id: u64, kind: ActorKind, pos: Vec3, yaw: f32) {
    let cloth = match kind {
        ActorKind::Player => assets.leather.clone(),
        ActorKind::Swordsman => assets.swordsman_cloth.clone(),
        ActorKind::HeavyKnight => assets.heavy_cloth.clone(),
    };
    let entity = commands
        .spawn((CorpseMarker { corpse_id, kind }, Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()))
        .with_children(|c| {
            // Лежит на спине: тело повёрнуто на 90 градусов.
            c.spawn((Mesh3d(assets.body.clone()), MeshMaterial3d(cloth), Transform::from_xyz(0.0, 0.3, 0.4).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))));
            let head = if kind == ActorKind::Player { assets.head.clone() } else { assets.helmet.clone() };
            let head_mat = if kind == ActorKind::Player { assets.leather.clone() } else { assets.steel.clone() };
            c.spawn((Mesh3d(head), MeshMaterial3d(head_mat), Transform::from_xyz(0.0, 0.25, 1.25).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))));
        })
        .id();
    index.0.insert(corpse_id, entity);
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
) {
    while let Some(bytes) = client.receive_message(CH_RELIABLE) {
        let Some(msg) = decode::<ServerMessage>(&bytes) else { continue };
        match msg {
            ServerMessage::Welcome { your_id, spawn } => {
                info!("Подключились к серверу, наш id {your_id}");
                state.my_id = Some(your_id);
                state.error = None;
                for e in &waiting {
                    commands.entity(e).despawn();
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
            ServerMessage::CorpseSpawned { corpse_id, kind, pos, yaw } => {
                if !corpses.0.contains_key(&corpse_id) {
                    spawn_corpse(&mut commands, &assets, &mut corpses, corpse_id, kind, pos.into(), yaw);
                }
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
    assets: Res<CharacterAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut index: ResMut<RemoteIndex>,
    mut remotes: Query<&mut RemoteActor>,
    mut local: Query<&mut LocalFighter>,
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
                if std::mem::discriminant(&r.fighter.action) != std::mem::discriminant(&a.action) || action_time(&a.action) > action_time(&r.fighter.action) {
                    r.fighter.action = a.action;
                }
                r.fighter.hp = a.hp;
                r.fighter.max_hp = a.max_hp;
            }
            None if !index.0.contains_key(&a.id) => spawn_remote(&mut commands, &assets, &mut materials, &mut index, a),
            None => {}
        }
    }
    // Те, кого больше нет в снимке, ушли или погибли.
    let gone: Vec<u64> = index.0.keys().copied().filter(|id| !seen.contains(id)).collect();
    for id in gone {
        if let Some(e) = index.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }
}

fn action_time(a: &Action) -> f32 {
    match *a {
        Action::Windup { t, .. }
        | Action::Strike { t, .. }
        | Action::Recovery { t, .. }
        | Action::Parry { t }
        | Action::ParryRecovery { t }
        | Action::BlockRecoil { t }
        | Action::Stagger { t }
        | Action::Flinch { t } => t,
        Action::Idle | Action::Block | Action::Dead => 0.0,
    }
}

fn send_state(mut client: ResMut<RenetClient>, mut timer: ResMut<SendTimer>, time: Res<Time>, local: Query<(&LocalPlayer, &Transform)>) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let Ok((p, t)) = local.single() else { return };
    let msg = ClientMessage::PlayerState { pos: t.translation.into(), yaw: p.yaw, pitch: p.pitch, crouching: p.crouching };
    client.send_message(CH_UNRELIABLE, encode(&msg));
}

#[allow(clippy::type_complexity)]
fn animate_remote(
    time: Res<Time>,
    assets: Res<CharacterAssets>,
    mut remotes: Query<(&mut RemoteActor, &mut Transform, &Children), (Without<RemoteBody>, Without<RemoteSword>, Without<RemoteShield>)>,
    mut bodies: Query<&mut Transform, (With<RemoteBody>, Without<RemoteSword>, Without<RemoteShield>)>,
    mut swords: Query<(&mut Transform, &Children), (With<RemoteSword>, Without<RemoteBody>, Without<RemoteShield>)>,
    mut shields: Query<&mut Transform, (With<RemoteShield>, Without<RemoteBody>, Without<RemoteSword>)>,
    mut blades: Query<&mut MeshMaterial3d<StandardMaterial>, With<Blade>>,
) {
    let dt = time.delta_secs();
    let t = 1.0 - (-15.0 * dt).exp();
    for (mut r, mut transform, children) in &mut remotes {
        r.fighter.tick(dt);
        if transform.translation.distance(r.target_pos) > 5.0 {
            transform.translation = r.target_pos;
        } else {
            transform.translation = transform.translation.lerp(r.target_pos, t);
        }
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(r.target_yaw), t);

        let stagger = matches!(r.fighter.action, Action::Stagger { .. } | Action::BlockRecoil { .. });
        let eye = if r.crouching { 1.05 } else { 1.65 };
        let aim = Quat::from_rotation_x(r.pitch * 0.6);
        let (hilt, dir) = combat::weapon_pose(&r.fighter.action, r.fighter.weapon, time.elapsed_secs());
        let unparryable = matches!(r.fighter.action, Action::Windup { unparryable: true, .. } | Action::Strike { unparryable: true, .. });
        for child in children.iter() {
            if let Ok(mut body) = bodies.get_mut(child) {
                let target_scale = if r.crouching { 0.68 } else { 1.0 };
                let base = body.scale.x;
                body.scale.y += (target_scale * base - body.scale.y) * t;
                let tilt = if stagger { 0.3 } else { 0.0 };
                body.rotation = body.rotation.slerp(Quat::from_rotation_x(tilt), t);
            }
            if let Ok((mut sword, blade_children)) = swords.get_mut(child) {
                sword.translation = Vec3::Y * (eye - 0.25) + aim * hilt;
                sword.rotation = Quat::from_rotation_arc(Vec3::Y, aim * dir);
                for bc in blade_children.iter() {
                    if let Ok(mut m) = blades.get_mut(bc) {
                        m.0 = if unparryable { assets.blade_red.clone() } else { assets.steel.clone() };
                    }
                }
            }
            if let Ok(mut shield) = shields.get_mut(child) {
                let (pos, rot) = if matches!(r.fighter.action, Action::Block) {
                    (Vec3::new(-0.05, 1.25, -0.5), Quat::IDENTITY)
                } else {
                    (Vec3::new(-0.42, 1.05, -0.1), Quat::from_rotation_y(1.2))
                };
                shield.translation = shield.translation.lerp(pos, t * 1.5);
                shield.rotation = shield.rotation.slerp(rot, t * 1.5);
            }
        }
    }
}
