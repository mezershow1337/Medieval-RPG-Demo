//! Клиент: графика, подключение к серверу, отображение других игроков.

use crate::player::{LocalPlayer, PlayerPlugin, spawn_local_player};
use crate::protocol::*;
use crate::{hud, level};
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

/// Другой игрок, которого мы видим по данным сервера.
#[derive(Component)]
pub struct RemotePlayer {
    #[allow(dead_code)] // понадобится для боя (кто кого ударил)
    pub id: u64,
    target_pos: Vec3,
    target_yaw: f32,
    crouching: bool,
}

/// Часть модели другого игрока, которая сжимается при приседании.
#[derive(Component)]
struct RemoteBody;

#[derive(Resource, Default)]
struct RemoteIndex(HashMap<u64, Entity>);

#[derive(Resource)]
struct SendTimer(Timer);

#[derive(Resource, Clone)]
struct RemoteAssets {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    visor: Handle<Mesh>,
    visor_mat: Handle<StandardMaterial>,
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
        .add_plugins((RenetClientPlugin, NetcodeClientPlugin, PlayerPlugin, hud::HudPlugin))
        .insert_resource(RenetClient::new(connection_config()))
        .insert_resource(transport)
        .insert_resource(NetState { server_addr: Some(server_addr), ..default() })
        .insert_resource(level::level_colliders())
        .insert_resource(RemoteIndex::default())
        .insert_resource(SendTimer(Timer::from_seconds(1.0 / 30.0, TimerMode::Repeating)))
        .insert_resource(ClearColor(Color::srgb(0.55, 0.5, 0.48)))
        .insert_resource(GlobalAmbientLight { color: Color::srgb(0.7, 0.72, 0.85), brightness: 250.0, ..default() })
        .add_observer(on_netcode_error)
        .add_systems(Startup, (level::spawn_level, setup_remote_assets, spawn_waiting_camera))
        .add_systems(Update, (receive_reliable, receive_snapshots, send_state, interpolate_remote).run_if(client_connected))
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

fn setup_remote_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(RemoteAssets {
        body: meshes.add(Capsule3d::new(0.3, 0.9)),
        head: meshes.add(Sphere::new(0.22)),
        visor: meshes.add(Cuboid::new(0.3, 0.08, 0.1)),
        visor_mat: materials.add(Color::srgb(0.1, 0.1, 0.1)),
    });
}

fn color_for(id: u64) -> Color {
    let hue = (id.wrapping_mul(2654435761) % 360) as f32;
    Color::hsl(hue, 0.6, 0.5)
}

fn spawn_remote(
    commands: &mut Commands,
    assets: &RemoteAssets,
    materials: &mut Assets<StandardMaterial>,
    index: &mut RemoteIndex,
    id: u64,
    pos: Vec3,
    yaw: f32,
) {
    let mat = materials.add(color_for(id));
    let entity = commands
        .spawn((
            RemotePlayer { id, target_pos: pos, target_yaw: yaw, crouching: false },
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((RemoteBody, Transform::default(), Visibility::default())).with_children(|b| {
                b.spawn((Mesh3d(assets.body.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, 0.75, 0.0)));
                b.spawn((Mesh3d(assets.head.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, 1.6, 0.0)));
                // «Забрало» показывает, куда смотрит игрок.
                b.spawn((Mesh3d(assets.visor.clone()), MeshMaterial3d(assets.visor_mat.clone()), Transform::from_xyz(0.0, 1.63, -0.2)));
            });
        })
        .id();
    index.0.insert(id, entity);
}

fn receive_reliable(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut state: ResMut<NetState>,
    mut index: ResMut<RemoteIndex>,
    mut local: Query<(&mut LocalPlayer, &mut Transform)>,
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
                    spawn_local_player(&mut commands, spawn.into());
                }
            }
            ServerMessage::Correction { pos } => {
                for (mut p, mut t) in &mut local {
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
        }
    }
}

fn receive_snapshots(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    state: Res<NetState>,
    assets: Res<RemoteAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut index: ResMut<RemoteIndex>,
    mut remotes: Query<&mut RemotePlayer>,
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
    for p in snapshot.players.iter().filter(|p| p.id != my_id) {
        seen.push(p.id);
        match index.0.get(&p.id).and_then(|e| remotes.get_mut(*e).ok()) {
            Some(mut r) => {
                r.target_pos = p.pos.into();
                r.target_yaw = p.yaw;
                r.crouching = p.crouching;
            }
            None if !index.0.contains_key(&p.id) => {
                spawn_remote(&mut commands, &assets, &mut materials, &mut index, p.id, p.pos.into(), p.yaw);
            }
            None => {}
        }
    }
    // Те, кого больше нет в снимке, ушли.
    let gone: Vec<u64> = index.0.keys().copied().filter(|id| !seen.contains(id)).collect();
    for id in gone {
        if let Some(e) = index.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }
}

fn send_state(
    mut client: ResMut<RenetClient>,
    mut timer: ResMut<SendTimer>,
    time: Res<Time>,
    local: Query<(&LocalPlayer, &Transform)>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let Ok((p, t)) = local.single() else { return };
    let msg = ClientMessage::PlayerState { pos: t.translation.into(), yaw: p.yaw, pitch: p.pitch, crouching: p.crouching };
    client.send_message(CH_UNRELIABLE, encode(&msg));
}

fn interpolate_remote(
    time: Res<Time>,
    mut remotes: Query<(&RemotePlayer, &mut Transform, &Children), Without<RemoteBody>>,
    mut bodies: Query<&mut Transform, With<RemoteBody>>,
) {
    let t = 1.0 - (-15.0 * time.delta_secs()).exp();
    for (r, mut transform, children) in &mut remotes {
        if transform.translation.distance(r.target_pos) > 5.0 {
            transform.translation = r.target_pos;
        } else {
            transform.translation = transform.translation.lerp(r.target_pos, t);
        }
        let target_rot = Quat::from_rotation_y(r.target_yaw);
        transform.rotation = transform.rotation.slerp(target_rot, t);
        for child in children.iter() {
            if let Ok(mut body) = bodies.get_mut(child) {
                let target_scale = if r.crouching { 0.68 } else { 1.0 };
                body.scale.y += (target_scale - body.scale.y) * t;
            }
        }
    }
}

