//! Клиент: графика, подключение к серверу, отображение других бойцов и тел.

use crate::combat::{self, Action, Fighter, WeaponKind};
use crate::fight::{self, CombatFeedback, LocalFighter};
use crate::player::{LocalPlayer, PlayerPlugin, spawn_local_player};
use crate::protocol::*;
use crate::rig::{self, CharacterModels, NeedsRig, NeedsShield, NeedsWeapon, Outfit, PoseInput, RigPose};
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
    /// Для анимации: фаза и сила шага, доля приседа, прошлое положение.
    walk_phase: f32,
    walk: f32,
    crouch: f32,
    last_pos: Vec3,
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

/// Щит в руке рыцаря.
#[derive(Component)]
struct RemoteShield;

/// Лезвие меча (меняет материал, когда удар нельзя парировать).
#[derive(Component, Clone)]
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

/// Общие материалы персонажей. Сами модели — в `rig::CharacterModels` (загружаются из glb).
#[derive(Resource, Clone)]
pub struct CharacterAssets {
    /// Клинок, пока модели не загрузились.
    pub steel: Handle<StandardMaterial>,
    /// Светящийся клинок: этот удар нельзя парировать.
    pub blade_red: Handle<StandardMaterial>,
}

impl CharacterAssets {
    /// Обычный материал клинка.
    pub fn blade(&self, models: Option<&CharacterModels>) -> Handle<StandardMaterial> {
        models.map(|m| m.blade.clone()).unwrap_or_else(|| self.steel.clone())
    }
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
        .add_plugins(rig::RigPlugin)
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
            ((receive_reliable, receive_snapshots).chain(), send_state, animate_remote).run_if(client_connected).before(fight::FightSet).in_set(rig::PoseSet),
        )
        .add_systems(Update, animate_corpses.in_set(rig::PoseSet))
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

fn setup_character_assets(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(CharacterAssets {
        steel: materials.add(StandardMaterial { base_color: Color::srgb(0.72, 0.74, 0.78), metallic: 0.9, perceptual_roughness: 0.3, ..default() }),
        blade_red: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.2, 0.1),
            emissive: LinearRgba::rgb(6.0, 0.6, 0.2),
            ..default()
        }),
    });
}

/// Где рука держит оружие: (правая, левая) — центр кулака в координатах оружия.
fn weapon_grips(weapon: WeaponKind) -> (Vec3, Option<Vec3>) {
    match weapon {
        WeaponKind::ArmingSword => (Vec3::new(0.0, -0.06, 0.0), None),
        WeaponKind::Greatsword => (Vec3::new(0.0, -0.07, 0.0), Some(Vec3::new(0.0, -0.19, 0.0))),
    }
}

/// Где левая рука держит щит (ремни на обратной стороне).
const SHIELD_GRIP: Vec3 = Vec3::new(0.0, -0.02, 0.07);

fn spawn_remote(commands: &mut Commands, index: &mut RemoteIndex, snap: &ActorSnapshot) {
    let pos: Vec3 = snap.pos.into();
    let mut fighter = Fighter::new(snap.weapon, snap.max_hp, snap.kind == ActorKind::Swordsman);
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
                crouch: if snap.crouching { 1.0 } else { 0.0 },
                last_pos: pos,
            },
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(snap.yaw)),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((BodyRig, NeedsRig(Outfit::for_actor(snap.kind, snap.id)), Transform::default(), Visibility::default()));
            p.spawn((RemoteSword, NeedsWeapon(snap.weapon), Transform::from_xyz(0.3, 1.0, -0.4), Visibility::default()));
            if snap.kind == ActorKind::Swordsman {
                p.spawn((RemoteShield, NeedsShield, Transform::from_xyz(-0.42, 1.05, -0.1), Visibility::default()));
            }
        })
        .id();
    index.0.insert(snap.id, entity);
}

fn spawn_corpse(commands: &mut Commands, index: &mut CorpseIndex, corpse_id: u64, kind: ActorKind, pos: Vec3, yaw: f32, animate: bool) {
    let weapon = crate::npc::fighter_for(kind).weapon;
    let entity = commands
        .spawn((CorpseMarker { corpse_id, kind }, Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()))
        .with_children(|c| {
            // Лежит на спине; упавшее оружие и щит — рядом.
            let t = if animate { 0.0 } else { 1.0 };
            c.spawn((BodyRig, CorpseFall { t, seed: corpse_id }, NeedsRig(Outfit::for_actor(kind, corpse_id)), rig::fall_transform(t), Visibility::default()));
            c.spawn((
                NeedsWeapon(weapon),
                Transform::from_xyz(0.5, 0.025, 0.35).with_rotation(Quat::from_rotation_y(0.35) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                Visibility::default(),
            ));
            if kind == ActorKind::Swordsman {
                c.spawn((
                    NeedsShield,
                    Transform::from_xyz(-0.6, 0.06, 0.55).with_rotation(Quat::from_rotation_y(-0.5) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    Visibility::default(),
                ));
            }
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
    time: Res<Time>,
    mut welcomed_at: Local<Option<f32>>,
) {
    while let Some(bytes) = client.receive_message(CH_RELIABLE) {
        let Some(msg) = decode::<ServerMessage>(&bytes) else { continue };
        match msg {
            ServerMessage::Welcome { your_id, spawn } => {
                info!("Подключились к серверу, наш id {your_id}");
                state.my_id = Some(your_id);
                state.error = None;
                *welcomed_at = Some(time.elapsed_secs());
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
                    // Тела, которые уже лежали, когда мы вошли, не падают заново.
                    let fresh = welcomed_at.is_some_and(|t| time.elapsed_secs() - t > 1.5);
                    spawn_corpse(&mut commands, &mut corpses, corpse_id, kind, pos.into(), yaw, fresh);
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
            None if !index.0.contains_key(&a.id) => spawn_remote(&mut commands, &mut index, a),
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate_remote(
    time: Res<Time>,
    assets: Res<CharacterAssets>,
    models: Option<Res<CharacterModels>>,
    mut remotes: Query<(&mut RemoteActor, &mut Transform, &Children), (Without<RemoteSword>, Without<RemoteShield>)>,
    mut swords: Query<(&mut Transform, &Children), (With<RemoteSword>, Without<RemoteActor>, Without<RemoteShield>)>,
    mut shields: Query<&mut Transform, (With<RemoteShield>, Without<RemoteActor>, Without<RemoteSword>)>,
    mut rigs: Query<&mut RigPose, With<BodyRig>>,
    mut blades: Query<&mut MeshMaterial3d<StandardMaterial>, With<Blade>>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let t = 1.0 - (-15.0 * dt).exp();
    let blade_mat = assets.blade(models.as_deref());
    for (mut r, mut transform, children) in &mut remotes {
        r.fighter.tick(dt);
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
        r.walk_phase += back * flat.length() / 1.5 * std::f32::consts::TAU;
        let walk_target = (speed / crate::player::WALK_SPEED).min(1.2);
        r.walk += (walk_target - r.walk) * (1.0 - (-8.0 * dt).exp());
        let crouch_target = if r.crouching { 1.0 } else { 0.0 };
        r.crouch += (crouch_target - r.crouch) * (1.0 - (-10.0 * dt).exp());

        let eye = 1.65 - 0.6 * r.crouch;
        let aim = Quat::from_rotation_x(r.pitch * 0.6);
        let (hilt, dir) = combat::weapon_pose(&r.fighter.action, r.fighter.weapon, now);
        let unparryable = matches!(r.fighter.action, Action::Windup { unparryable: true, .. } | Action::Strike { unparryable: true, .. });
        let (twist, lean) = match r.fighter.action {
            Action::Windup { swing, .. } | Action::Strike { swing, .. } | Action::Recovery { swing, .. } => {
                let lean = if swing == combat::Swing::Overhead { -0.25 * (aim * dir).z.abs().min(1.0) } else { -0.05 };
                (-(aim * dir).x * 0.55, lean)
            }
            Action::Stagger { t } | Action::BlockRecoil { t } => (0.0, 0.32 + (now * 25.0).sin() * 0.05 * (1.0 - t).max(0.0)),
            Action::Flinch { .. } => (0.1, 0.15),
            Action::Parry { .. } | Action::ParryRecovery { .. } => (0.2, 0.05),
            _ => (0.0, 0.0),
        };

        let mut sword_entity = None;
        let mut shield_entity = None;
        let mut rig_entity = None;
        for child in children.iter() {
            if swords.contains(child) {
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
            let (pos, rot) = if matches!(r.fighter.action, Action::Block) {
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
        let mut input = PoseInput {
            crouch: r.crouch,
            walk_phase: r.walk_phase,
            walk: r.walk,
            pitch: r.pitch,
            twist,
            lean,
            time: now + r.id as f32,
            right_grip: Some(sword_tf.transform_point(right)),
            left_grip: left.map(|l| sword_tf.transform_point(l)).or(shield_tf.map(|s| s.transform_point(SHIELD_GRIP))),
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
        if let Some(e) = sword_entity
            && let Ok((mut sword, blade_children)) = swords.get_mut(e)
        {
            *sword = sword_tf;
            for bc in blade_children.iter() {
                if let Ok(mut m) = blades.get_mut(bc) {
                    m.0 = if unparryable { assets.blade_red.clone() } else { blade_mat.clone() };
                }
            }
        }
    }
}
