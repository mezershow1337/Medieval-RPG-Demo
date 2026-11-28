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
mod crowd_lod;
mod rig;
mod saves;
mod saves_server;
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
    // `MRPG_MAP="x,z,полуширина"` — крупный план куска карты (по умолчанию — вся карта).
    let view: Vec<f32> = std::env::var("MRPG_MAP").ok().map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect()).unwrap_or_default();
    let (cx, cz, half) = if let [x, z, h] = view[..] { (x, z, h) } else { (0.0, 0.0, 900.0) };
    let step = half * 2.0 / N as f32;
    let world = |i: usize, j: usize| (cx - half + i as f32 * step, cz - half + j as f32 * step);
    let pixel = |x: f32, z: f32| (((x - cx + half) / step) as i32, ((z - cz + half) / step) as i32);
    let mut px = vec![0u8; N * N * 3];
    for j in 0..N {
        for i in 0..N {
            let (x, z) = world(i, j);
            let y = level::ground_y(x, z);
            // Свет с северо-запада: склоны читаются.
            let shade = (1.0 + ((level::ground_y(x + 2.0, z + 2.0) - level::ground_y(x - 2.0, z - 2.0)) * -0.22)).clamp(0.35, 1.6);
            let c = if y < level::WATER_Y {
                [40.0, 70.0, 120.0]
            } else if level::inside_city(x, z) {
                [170.0, 150.0, 120.0]
            } else if level::road_distance(x, z) < 2.6 {
                [150.0 * shade, 120.0 * shade, 80.0 * shade]
            } else {
                let k = (95.0 + y * 1.6).clamp(40.0, 200.0) * shade;
                [k, k + 25.0, k / 2.0 + 20.0]
            };
            px[(j * N + i) * 3..][..3].copy_from_slice(&c.map(|v| v.clamp(0.0, 255.0) as u8));
        }
    }
    // Постройки: стены — тёмные, крыши — красные (чем выше, тем светлее).
    let mut paint = |a: &crate::collision::Aabb, c: [u8; 3]| {
        let (i0, j0) = pixel(a.min.x, a.min.z);
        let (i1, j1) = pixel(a.max.x, a.max.z);
        for j in j0.max(0)..=j1.min(N as i32 - 1) {
            for i in i0.max(0)..=i1.min(N as i32 - 1) {
                px[(j as usize * N + i as usize) * 3..][..3].copy_from_slice(&c);
            }
        }
    };
    let layout = level::layout();
    for b in layout.blocks.iter().filter(|b| b.solid && b.aabb.max.y - b.aabb.min.y > 1.6) {
        paint(&b.aabb, [70, 60, 55]);
    }
    for r in &layout.roofs {
        let k = ((r.aabb.max.y - level::ground_y(r.aabb.min.x, r.aabb.min.z)) * 9.0).clamp(0.0, 110.0) as u8;
        paint(&r.aabb, [120 + k, 50 + k / 2, 40 + k / 3]);
    }
    for p in &layout.plants {
        let c = match p.kind {
            K::Fir => [0, 60, 20],
            K::Pine => [20, 110, 30],
            K::Oak => [230, 200, 40],
            K::Juniper | K::Cypress => [0, 130, 130],
            K::Rock => [255, 255, 255],
            K::Fig => [200, 60, 160],
            _ => continue,
        };
        let (i, j) = pixel(p.pos.x, p.pos.z);
        if i >= 0 && j >= 0 && (i as usize) < N && (j as usize) < N {
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
    let mut out = format!("P6\n{N} {N}\n255\n").into_bytes();
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
        Some("host") => {
            let port = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
            // `host 5000 --load 3` — начать с сохранения (так игра перезапускается из «Загрузить игру»).
            let load = args.iter().position(|a| a == "--load").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok());
            client::run(match load {
                Some(slot) => menu::Start::Load(port, slot),
                None => menu::Start::Host(port),
            })
        }
        None => client::run(menu::Start::Menu),
        Some(other) => {
            eprintln!("Неизвестная команда «{other}». Используйте: host | join <адрес> | server");
            std::process::exit(2);
        }
    }
}
