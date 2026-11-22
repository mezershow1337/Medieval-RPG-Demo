//! Medieval RPG — прототип.
//!
//! Запуск:
//!   cargo run                     — хост: сервер + игра на этом компьютере
//!   cargo run -- join 192.168.1.5 — подключиться к другу (порт по умолчанию 5000)
//!   cargo run -- server           — только сервер, без окна

mod client;
mod collision;
mod combat;
mod consumables;
mod fight;
mod graphics;
mod hud;
mod icons;
mod inventory;
mod inventory_ui;
mod items;
mod level;
mod loot_client;
mod loot_net;
mod loot_server;
mod npc;
mod player;
mod protocol;
mod rig;
mod server;

use protocol::DEFAULT_PORT;
use std::net::{SocketAddr, ToSocketAddrs};

fn parse_addr(s: &str) -> SocketAddr {
    let with_port = if s.contains(':') { s.to_string() } else { format!("{s}:{DEFAULT_PORT}") };
    with_port
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .unwrap_or_else(|| panic!("не понимаю адрес сервера: {s}"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("server") => {
            let port = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
            let socket = server::bind_socket(port).expect("порт занят");
            server::run(socket, true);
        }
        Some("join") => {
            let addr = parse_addr(args.get(1).map(String::as_str).unwrap_or("127.0.0.1"));
            client::run(addr);
        }
        None | Some("host") => {
            let port = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
            let socket = server::bind_socket(port).expect("порт занят: возможно, сервер уже запущен");
            std::thread::spawn(move || server::run(socket, false));
            client::run(SocketAddr::from(([127, 0, 0, 1], port)));
        }
        Some(other) => {
            eprintln!("Неизвестная команда «{other}». Используйте: host | join <адрес> | server");
            std::process::exit(2);
        }
    }
}
