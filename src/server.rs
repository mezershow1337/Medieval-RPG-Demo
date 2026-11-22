//! Сервер без графики. Хранит состояние игроков, проверяет их перемещения и рассылает снимки мира.

use crate::collision::{Body, LevelColliders};
use crate::level;
use crate::protocol::*;
use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy_renet::netcode::{NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig};
use bevy_renet::{RenetServer, RenetServerEvent, RenetServerPlugin, renet::ServerEvent};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::time::{Duration, SystemTime};

/// Сколько метров по горизонтали игрок может пройти за секунду (с запасом на лаги).
const MAX_HORIZONTAL_SPEED: f32 = 9.0;
const MAX_VERTICAL_SPEED: f32 = 40.0;

struct ServerPlayer {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
    crouching: bool,
    last_update: f64,
}

#[derive(Resource, Default)]
struct Players {
    map: HashMap<u64, ServerPlayer>,
    joined_count: usize,
}

#[derive(Resource)]
struct SnapshotTimer {
    timer: Timer,
    tick: u32,
}

pub fn bind_socket(port: u16) -> std::io::Result<UdpSocket> {
    UdpSocket::bind(("0.0.0.0", port))
}

/// Запускает сервер в текущем потоке (блокирует до выхода).
pub fn run(socket: UdpSocket, with_logging: bool) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))));
    if with_logging {
        app.add_plugins(bevy::log::LogPlugin::default());
    }
    let addr = socket.local_addr().expect("адрес сокета");
    let config = ServerConfig {
        current_time: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(),
        max_clients: MAX_PLAYERS,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![addr],
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = NetcodeServerTransport::new(config, socket).expect("не удалось запустить сервер");
    app.add_plugins((RenetServerPlugin, NetcodeServerPlugin))
        .insert_resource(RenetServer::new(connection_config()))
        .insert_resource(transport)
        .insert_resource(level::level_colliders())
        .insert_resource(Players::default())
        .insert_resource(SnapshotTimer { timer: Timer::from_seconds((1.0 / SNAPSHOT_RATE_HZ) as f32, TimerMode::Repeating), tick: 0 })
        .add_observer(on_server_event)
        .add_systems(Update, (receive_client_messages, send_snapshots).chain());
    info!("Сервер слушает {addr}");
    app.run();
}

fn on_server_event(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, mut players: ResMut<Players>, time: Res<Time>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let id = *client_id;
            let spawn = level::spawn_point(players.joined_count);
            players.joined_count += 1;
            info!("Игрок {id} подключился");
            server.send_message(id, CH_RELIABLE, encode(&ServerMessage::Welcome { your_id: id, spawn: spawn.into() }));
            server.broadcast_message_except(id, CH_RELIABLE, encode(&ServerMessage::PlayerJoined { id }));
            players.map.insert(id, ServerPlayer { pos: spawn, yaw: 0.0, pitch: 0.0, crouching: false, last_update: time.elapsed_secs_f64() });
        }
        ServerEvent::ClientDisconnected { client_id, reason } => {
            info!("Игрок {client_id} отключился: {reason}");
            players.map.remove(client_id);
            server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::PlayerLeft { id: *client_id }));
        }
    }
}

fn receive_client_messages(mut server: ResMut<RenetServer>, mut players: ResMut<Players>, level: Res<LevelColliders>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    for id in server.clients_id() {
        while let Some(bytes) = server.receive_message(id, CH_UNRELIABLE) {
            let Some(ClientMessage::PlayerState { pos, yaw, pitch, crouching }) = decode::<ClientMessage>(&bytes) else {
                continue;
            };
            let Some(player) = players.map.get_mut(&id) else { continue };
            let new_pos: Vec3 = pos.into();
            let dt = (now - player.last_update).max(1.0 / 60.0) as f32;
            let d = new_pos - player.pos;
            let horizontal = Vec2::new(d.x, d.z).length();
            let body = if crouching { Body::CROUCHING } else { Body::STANDING };
            let too_fast = horizontal > MAX_HORIZONTAL_SPEED * dt + 0.5 || d.y.abs() > MAX_VERTICAL_SPEED * dt + 0.6;
            let in_wall = !level.body_is_free(&body, new_pos);
            if !new_pos.is_finite() || too_fast || in_wall {
                server.send_message(id, CH_RELIABLE, encode(&ServerMessage::Correction { pos: player.pos.into() }));
                player.last_update = now;
                continue;
            }
            player.pos = new_pos;
            player.yaw = yaw;
            player.pitch = pitch.clamp(-1.6, 1.6);
            player.crouching = crouching;
            player.last_update = now;
        }
    }
}

fn send_snapshots(mut server: ResMut<RenetServer>, players: Res<Players>, mut timer: ResMut<SnapshotTimer>, time: Res<Time>) {
    if !timer.timer.tick(time.delta()).just_finished() {
        return;
    }
    timer.tick = timer.tick.wrapping_add(1);
    let snapshot = Snapshot {
        tick: timer.tick,
        players: players
            .map
            .iter()
            .map(|(id, p)| PlayerSnapshot { id: *id, pos: p.pos.into(), yaw: p.yaw, pitch: p.pitch, crouching: p.crouching })
            .collect(),
    };
    server.broadcast_message(CH_UNRELIABLE, encode(&snapshot));
}
