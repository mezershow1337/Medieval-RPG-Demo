//! Бот-товарищ для проверки сети без второго компьютера: `medieval_rpg bot <адрес> [имя]`.
//!
//! Подключается как обычный игрок, но без окна и графики: называет себя, ходит взад-вперёд
//! от точки появления (с теми же столкновениями, что и у игрока) и ставит метку на карте
//! (`MRPG_MARK="x,z"`). Удобно смотреть, как видны товарищи на карте, компасе и над головой.
//! `MRPG_BOT_WALK="0,0"` — стоять на месте; `MRPG_BOT_CONSOLE="tp 88 11 8"` — команды консоли
//! после входа (нужны читы для всех: `MEDIEVAL_CHEATS=1` у сервера) — например, поставить бота
//! на крышу для снимка со стороны.

use crate::collision::{Body, LevelColliders};
use crate::map::{CH_MAP, MapClientMessage};
use crate::protocol::*;
use bevy::math::Vec3;
use bevy_renet::netcode::{ClientAuthentication, NetcodeClientTransport};
use bevy_renet::renet::RenetClient;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime};

pub fn run(server_addr: SocketAddr, name: String) {
    let client_id = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let socket = UdpSocket::bind("0.0.0.0:0").expect("сокет бота");
    let auth = ClientAuthentication::Unsecure { protocol_id: PROTOCOL_ID, client_id, server_addr, user_data: None };
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let mut transport = NetcodeClientTransport::new(now, auth, socket).expect("подключение бота");
    let config = connection_config();
    let channels: Vec<u8> = config.server_channels_config.iter().map(|c| c.channel_id).filter(|c| *c != CH_RELIABLE as u8).collect();
    let mut client = RenetClient::new(config);
    let level: LevelColliders = crate::level::level_colliders();
    let mark = std::env::var("MRPG_MARK").ok().map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 2);

    // `MRPG_BOT_WALK="dx,dz"` — куда и насколько ходить от точки появления.
    let walk = std::env::var("MRPG_BOT_WALK")
        .ok()
        .map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>())
        .filter(|v| v.len() == 2)
        .map_or(Vec3::new(-30.0, 0.0, 0.0), |v| Vec3::new(v[0], 0.0, v[1]));
    let tick = Duration::from_secs_f64(1.0 / 30.0);
    let mut last = Instant::now();
    let mut feet: Option<Vec3> = None;
    let mut home = Vec3::ZERO;
    let mut said_hello = false;
    let mut t = 0.0f32;
    let mut grounded = true;
    println!("Бот «{name}» подключается к {server_addr}…");
    loop {
        let dt = last.elapsed();
        last = Instant::now();
        client.update(dt);
        if transport.update(dt, &mut client).is_err() {
            eprintln!("Бот: сервер недоступен");
            return;
        }
        if client.is_connected() {
            if !said_hello {
                said_hello = true;
                client.send_message(CH_MAP, encode(&MapClientMessage::Hello { name: name.clone() }));
                if let Some(v) = &mark {
                    client.send_message(CH_MAP, encode(&MapClientMessage::SetMark(Some([v[0], v[1]]))));
                }
                for line in std::env::var("MRPG_BOT_CONSOLE").unwrap_or_default().split(';').map(str::trim).filter(|l| !l.is_empty()) {
                    let msg = crate::console_net::ConsoleClientMessage::Command { line: line.to_string(), token: 0 };
                    client.send_message(crate::console_net::CH_CONSOLE, encode(&msg));
                }
            }
            while let Some(bytes) = client.receive_message(CH_RELIABLE) {
                match decode::<ServerMessage>(&bytes) {
                    Some(ServerMessage::Welcome { spawn, .. }) | Some(ServerMessage::Respawned { pos: spawn, .. }) => {
                        feet = Some(spawn.into());
                        home = spawn.into();
                        println!("Бот появился в {home}");
                    }
                    Some(ServerMessage::Correction { pos }) => feet = Some(pos.into()),
                    _ => {}
                }
            }
            // Остальное (снимки мира, добыча, торговля…) боту не нужно — просто вычитываем.
            for ch in &channels {
                while client.receive_message(*ch).is_some() {}
            }
            if let Some(f) = feet.as_mut() {
                // Ходит туда и обратно (шаг человека ~1,4 м/с): по умолчанию 30 м на запад по главной улице.
                t += dt.as_secs_f32();
                let len = walk.length().max(1.0);
                let phase = (t * 1.4 / len) % 2.0;
                let target = home + walk * if phase < 1.0 { phase } else { 2.0 - phase };
                let mut d = target - *f;
                d.y = 0.0;
                let step = if walk == Vec3::ZERO { Vec3::ZERO } else { d.clamp_length_max(1.4 * dt.as_secs_f32()) };
                let r = level.move_body(&Body::STANDING, f, step + Vec3::Y * -4.0 * dt.as_secs_f32(), grounded);
                grounded = r.grounded;
                let dir = if phase < 1.0 { walk } else { -walk };
                let yaw = (-dir.x).atan2(-dir.z);
                let msg = ClientMessage::PlayerState { pos: (*f).into(), yaw, pitch: 0.0, crouching: false, sprinting: false };
                client.send_message(CH_UNRELIABLE, encode(&msg));
            }
        }
        let _ = transport.send_packets(&mut client);
        std::thread::sleep(tick);
    }
}
