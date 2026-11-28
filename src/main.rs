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
mod start_kit;
mod audio;
mod bot;
mod brewing;
mod brewing_client;
mod civil_client;
mod civil_server;
mod civilians;
mod cloud_map;
mod mountain_map;
mod cloud_noise;
mod client;
mod collision;
mod combat;
mod console;
mod console_net;
mod console_server;
mod consumables;
mod doors;
mod economy;
mod estates;
mod factions;
mod fight;
mod fishing;
mod fishing_client;
mod fishing_net;
mod fishing_server;
mod guide;
mod hints;
mod gear_look;
mod home_items;
mod hotbar;
mod graphics;
mod hud;
mod icons;
mod inventory;
mod inventory_ui;
mod items;
mod knowledge;
mod launcher;
mod level;
mod menu;
mod mining;
mod mining_client;
mod mining_net;
mod mining_server;
mod loot_client;
mod loot_net;
mod loot_server;
mod letters;
mod school;
mod school_client;
mod tablet_view;
mod lore;
mod lore_client;
mod lore_net;
mod map;
mod minimap;
mod map_render;
mod medicine;
mod medicine_server;
mod medicine_client;
mod health_ui;
mod npc;
mod player;
mod props;
mod rep_demo;
mod props_client;
mod props_server;
mod protocol;
mod residents;
mod rig;
mod self_body;
mod server;
mod settings;
mod shop_hours;
mod shops;
mod skill_book;
mod skill_tree;
mod skills_server;
mod archery;
mod arsenal;
mod smithing;
mod smithing_client;
mod smithing_fx;
mod smithing_net;
mod smithing_quick;
mod smithing_server;
mod sky;
mod smoke;
mod static_bounds;
mod surface_layers;
mod textures;
mod towns;
mod towns_client;
mod towns_server;
mod trade;
mod trade_client;
mod trade_net;
mod trade_server;
mod trade_thumbs;
mod trade_ui;
mod ui_cursor;
mod ui_theme;
mod vegetation;
mod sea_life;
mod swim;
mod water;
mod weight;
mod work;
mod work_client;
mod worn;
mod world_clock;
mod daily_life;
mod daylight;
mod life_client;
mod night_lights;

use protocol::DEFAULT_PORT;
use std::net::SocketAddr;

fn parse_addr(s: &str) -> SocketAddr {
    launcher::resolve(s).unwrap_or_else(|e| panic!("не понимаю адрес сервера: {e}"))
}

/// Вид мира сверху: вода, рельеф, город, дороги и растения точками. 2 м на пиксель,
/// от −900 до 900 м по обеим осям (север сверху, как на миникарте: +z вниз).
fn dump_map(path: &str) {
    use level::PlantKind as K;
    const N: usize = 900;
    // По Z — до южного края гор.
    let nz = ((900.0 + level::mountains::MESH_SOUTH) / 2.0) as usize;
    let mut px = vec![0u8; N * nz * 3];
    for j in 0..nz {
        for i in 0..N {
            let (x, z) = (i as f32 * 2.0 - 900.0, j as f32 * 2.0 - 900.0);
            let y = level::ground_y(x, z);
            let c = if y < level::WATER_Y {
                [40, 70, 120]
            } else if level::inside_city(x, z) {
                [170, 150, 120]
            } else if level::road_distance(x, z) < 2.6 {
                [120, 95, 60]
            } else {
                // Горы — тенью склона, чтобы были видны гривы и уступы.
                let (dx, dz) = (level::ground_y(x + 2.0, z) - y, level::ground_y(x, z + 2.0) - y);
                let shade = (1.0 - (dx + dz) * 0.35).clamp(0.4, 1.4);
                let k = ((60.0 + y.min(35.0) * 4.0 + (y - 35.0).max(0.0) * 0.25) * shade).clamp(30.0, 230.0) as u8;
                [k, k.saturating_add(25), k / 2 + 20]
            };
            px[(j * N + i) * 3..][..3].copy_from_slice(&c);
        }
    }
    for p in &level::layout().plants {
        let c = match p.kind {
            K::Fir => [0, 60, 20],
            K::Pine => [20, 110, 30],
            K::Oak => [230, 200, 40],
            K::Juniper | K::Cypress => [0, 130, 130],
            K::Rock => [255, 255, 255],
            K::Fig => [200, 60, 160],
            _ => continue,
        };
        let (i, j) = (((p.pos.x + 900.0) / 2.0) as i32, ((p.pos.z + 900.0) / 2.0) as i32);
        if i >= 0 && j >= 0 && (i as usize) < N && (j as usize) < nz {
            px[(j as usize * N + i as usize) * 3..][..3].copy_from_slice(&c);
        }
    }
    // По три растения каждого вида, ближайших к заданной точке (`medieval_rpg map файл x z`).
    let at: Vec<f32> = std::env::args().skip(3).filter_map(|a| a.parse().ok()).collect();
    if let [x, z] = at[..] {
        for kind in [K::Pine, K::Oak, K::Juniper, K::Cypress, K::Fig, K::Shrub, K::Fir, K::Rock] {
            let mut near: Vec<_> = level::layout().plants.iter().filter(|p| p.kind == kind).map(|p| (((p.pos.x - x).powi(2) + (p.pos.z - z).powi(2)).sqrt(), p.pos, p.height)).collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (d, pos, h) in near.into_iter().take(3) {
                println!("{kind:?}: {:.0},{:.1},{:.0} высота {h:.1} (до точки {d:.0} м)", pos.x, pos.y, pos.z);
            }
        }
    }
    let mut out = format!("P6\n{N} {nz}\n255\n").into_bytes();
    out.extend_from_slice(&px);
    std::fs::write(path, out).expect("не записать карту");
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
        // Карта мира сверху в файл PPM (для подбора точек съёмки): `medieval_rpg map файл.ppm`.
        Some("map") => dump_map(args.get(1).map(String::as_str).unwrap_or("map.ppm")),
        Some("server") => {
            let port = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
            let socket = server::bind_socket(port).expect("порт занят");
            server::run(socket, true);
        }
        Some("bot") => {
            let addr = parse_addr(args.get(1).map(String::as_str).unwrap_or("127.0.0.1"));
            bot::run(addr, args.get(2).cloned().unwrap_or_else(|| "Бот".to_string()));
        }
        Some("join") => {
            let addr = parse_addr(args.get(1).map(String::as_str).unwrap_or("127.0.0.1"));
            client::run(menu::Start::Join(addr));
        }
        Some("host") => client::run(menu::Start::Host(args.get(1).and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT))),
        None => client::run(menu::Start::Menu),
        Some(other) => {
            eprintln!("Неизвестная команда «{other}». Используйте: host | join <адрес> | server");
            std::process::exit(2);
        }
    }
}
