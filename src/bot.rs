//! Бот-товарищ для проверки сети без второго компьютера: `medieval_rpg bot <адрес> [имя]`.
//!
//! Подключается как обычный игрок, но без окна и графики: называет себя, ходит взад-вперёд
//! от точки появления (с теми же столкновениями, что и у игрока) и ставит метку на карте
//! (`MRPG_MARK="x,z"`). Удобно смотреть, как видны товарищи на карте, компасе и над головой.

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
    let mut client = RenetClient::new(connection_config());
    let level: LevelColliders = crate::level::level_colliders();
    let mark = std::env::var("MRPG_MARK").ok().map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 2);

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
            }
            while let Some(bytes) = client.receive_message(CH_RELIABLE) {
                match decode::<ServerMessage>(&bytes) {
                    Some(ServerMessage::Welcome { spawn, .. }) | Some(ServerMessage::Respawned { pos: spawn }) => {
                        feet = Some(spawn.into());
                        home = spawn.into();
                        println!("Бот появился в {home}");
                    }
                    Some(ServerMessage::Correction { pos }) => feet = Some(pos.into()),
                    _ => {}
                }
            }
            for ch in [CH_UNRELIABLE as u8, crate::loot_net::CH_LOOT, crate::trade_net::CH_TRADE, crate::lore_net::CH_LORE, crate::doors::CH_DOOR, crate::loot_net::CH_PROPS, crate::smithing_net::CH_SMITH, CH_MAP] {
                while client.receive_message(ch).is_some() {}
            }
            if let Some(f) = feet.as_mut() {
                // Ходит по главной улице на 30 м на запад и обратно (шаг человека ~1,4 м/с).
                t += dt.as_secs_f32();
                let phase = (t * 1.4 / 30.0) % 2.0;
                let target = home + Vec3::new(-30.0 * if phase < 1.0 { phase } else { 2.0 - phase }, 0.0, 0.0);
                let mut d = target - *f;
                d.y = 0.0;
                let step = d.clamp_length_max(1.4 * dt.as_secs_f32());
                let r = level.move_body(&Body::STANDING, f, step + Vec3::Y * -4.0 * dt.as_secs_f32(), grounded);
                grounded = r.grounded;
                let yaw = if phase < 1.0 { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
                let msg = ClientMessage::PlayerState { pos: (*f).into(), yaw, pitch: 0.0, crouching: false, sprinting: false };
                client.send_message(CH_UNRELIABLE, encode(&msg));
            }
        }
        let _ = transport.send_packets(&mut client);
        std::thread::sleep(tick);
    }
}
