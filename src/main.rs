//! Medieval RPG — прототип.
//!
//! Запуск:
//!   cargo run                     — хост: сервер + игра на этом компьютере
//!   cargo run -- join 192.168.1.5 — подключиться к другу (порт по умолчанию 5000)
//!   cargo run -- server           — только сервер, без окна

mod alchemy_fx;
mod brewing;
mod brewing_client;
mod client;
mod collision;
mod combat;
mod consumables;
mod doors;
mod economy;
mod factions;
mod fight;
mod graphics;
mod hud;
mod icons;
mod inventory;
mod inventory_ui;
mod items;
mod knowledge;
mod launcher;
mod level;
mod loot_client;
mod loot_net;
mod loot_server;
mod lore;
mod lore_client;
mod lore_net;
mod medicine;
mod medicine_server;
mod medicine_client;
mod npc;
mod player;
mod props;
mod props_client;
mod props_server;
mod protocol;
mod residents;
mod rig;
mod server;
mod archery;
mod arsenal;
mod smithing;
mod smithing_client;
mod smithing_fx;
mod smithing_net;
mod smithing_server;
mod sky;
mod smoke;
mod surface_layers;
mod textures;
mod trade;
mod trade_client;
mod trade_net;
mod trade_server;
mod trade_ui;
mod weight;
mod work;
mod work_client;

use protocol::DEFAULT_PORT;
use std::net::SocketAddr;

fn parse_addr(s: &str) -> SocketAddr {
    launcher::resolve(s).unwrap_or_else(|e| panic!("не понимаю адрес сервера: {e}"))
}

/// Хост: сервер в отдельном потоке и окно игры, подключённое к нему же.
fn host(port: u16) {
    let socket = match server::bind_socket(port) {
        Ok(s) => s,
        Err(e) => panic!("порт {port} занят ({e}): возможно, игра или сервер уже запущены"),
    };
    std::thread::spawn(move || server::run(socket, false));
    client::run(SocketAddr::from(([127, 0, 0, 1], port)));
}

fn main() {
    // Загрузка больших glTF (serde_json) в пуле ввода-вывода Bevy уходит глубоко в стек, и
    // стандартных 2 МБ на поток не хватает: игра падала с «stack overflow» при запуске.
    if std::env::var_os("RUST_MIN_STACK").is_none() {
        // SAFETY: других потоков ещё нет — переменную окружения никто не читает одновременно.
        unsafe { std::env::set_var("RUST_MIN_STACK", (16 * 1024 * 1024).to_string()) };
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Готовая сборка, запущенная двойным щелчком: меню «Создать игру / Подключиться».
    if args.is_empty() && launcher::wanted() {
        launcher::keep_console_on_panic();
        match launcher::run_menu() {
            launcher::Choice::Host => host(DEFAULT_PORT),
            launcher::Choice::Join(addr) => client::run(addr),
        }
        return;
    }
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
            host(args.get(1).and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT));
        }
        Some(other) => {
            eprintln!("Неизвестная команда «{other}». Используйте: host | join <адрес> | server");
            std::process::exit(2);
        }
    }
}
