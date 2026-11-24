//! Medieval RPG — прототип.
//!
//! Запуск:
//!   cargo run                     — главное меню в окне игры (Создать / Подключиться / Настройки)
//!   cargo run -- host             — сразу хост: сервер + игра на этом компьютере
//!   cargo run -- join 192.168.1.5 — сразу подключиться к другу (порт по умолчанию 5000)
//!   cargo run -- server           — только сервер, без окна
//!   cargo run -- bot 127.0.0.1 Имя  — бот-товарищ (ходит и ставит метку) для проверки карты

// Готовая сборка для Windows — без чёрного окна консоли (меню теперь в самой игре).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod alchemy_fx;
mod appearance;
mod archery;
mod arsenal;
mod audio;
mod bot;
mod brewing;
mod brewing_client;
mod civil_client;
mod civil_server;
mod civilians;
mod client;
mod collision;
mod combat;
mod console;
mod console_net;
mod console_server;
mod consumables;
mod daily_life;
mod daylight;
mod doors;
mod economy;
mod estates;
mod factions;
mod fight;
mod gear_look;
mod graphics;
mod guide;
mod health_ui;
mod hints;
mod home_items;
mod hotbar;
mod hud;
mod icons;
mod inventory;
mod inventory_ui;
mod items;
mod knowledge;
mod launcher;
mod letters;
mod level;
mod life_client;
mod loot_client;
mod loot_net;
mod loot_server;
mod lore;
mod lore_client;
mod lore_net;
mod map;
mod map_render;
mod medicine;
mod medicine_client;
mod medicine_server;
mod menu;
mod minimap;
mod mining;
mod mining_client;
mod mining_net;
mod mining_server;
mod night_lights;
mod npc;
mod player;
mod props;
mod props_client;
mod props_server;
mod protocol;
mod residents;
mod rig;
mod school;
mod school_client;
mod server;
mod settings;
mod skill_book;
mod skill_tree;
mod skills_server;
mod sky;
mod smithing;
mod smithing_client;
mod smithing_fx;
mod smithing_net;
mod smithing_server;
mod smoke;
mod start_kit;
mod surface_layers;
mod tablet_view;
mod textures;
mod towns;
mod towns_client;
mod towns_server;
mod trade;
mod trade_client;
mod trade_net;
mod trade_server;
mod trade_ui;
mod ui_theme;
mod vegetation;
mod weight;
mod work;
mod work_client;
mod world_clock;
mod worn;

use protocol::DEFAULT_PORT;
use std::net::SocketAddr;

fn parse_addr(s: &str) -> SocketAddr {
    launcher::resolve(s).unwrap_or_else(|e| panic!("не понимаю адрес сервера: {e}"))
}

fn main() {
    // Загрузка больших glTF (serde_json) в пуле ввода-вывода Bevy уходит глубоко в стек, и
    // стандартных 2 МБ на поток не хватает: игра падала с «stack overflow» при запуске.
    if std::env::var_os("RUST_MIN_STACK").is_none() {
        // SAFETY: других потоков ещё нет — переменную окружения никто не читает одновременно.
        unsafe { std::env::set_var("RUST_MIN_STACK", (16 * 1024 * 1024).to_string()) };
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    launcher::install_crash_report();
    if !args.is_empty() {
        launcher::attach_parent_console();
    }
    match args.first().map(String::as_str) {
        Some("server") => {
            let port = args
                .get(1)
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            let socket = server::bind_socket(port).expect("порт занят");
            server::run(socket, true);
        }
        Some("bot") => {
            let addr = parse_addr(args.get(1).map(String::as_str).unwrap_or("127.0.0.1"));
            bot::run(
                addr,
                args.get(2).cloned().unwrap_or_else(|| "Бот".to_string()),
            );
        }
        Some("join") => {
            let addr = parse_addr(args.get(1).map(String::as_str).unwrap_or("127.0.0.1"));
            client::run(menu::Start::Join(addr));
        }
        Some("host") => client::run(menu::Start::Host(
            args.get(1)
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT),
        )),
        None => client::run(menu::Start::Menu),
        Some(other) => {
            eprintln!("Неизвестная команда «{other}». Используйте: host | join <адрес> | server");
            std::process::exit(2);
        }
    }
}
