//! Где игрок появляется, когда входит в игру и после смерти.
//!
//! Херсон: дюжина с лишним открытых мест внутри стен — агора, главная улица, перекрёстки,
//! портовый торг, плац цитадели, площадь у резервуара… Каждый вход — случайное из них
//! (сервер выбирает, см. [`pick_spawn`]); потом игрок сможет выбирать место сам —
//! для этого уже есть [`SpawnChoice::At`].

use super::city::{self, BZ, LONG_W, MAIN_HALF, block, cross_x};
use super::{LevelKind, active};
use crate::collision::{Body, LevelColliders};
use bevy::prelude::*;
use std::sync::OnceLock;

/// Место появления: точка на улице и куда смотреть.
#[derive(Clone, Debug)]
pub struct SpawnSpot {
    /// Ключ (латиница) — по нему игрок потом будет выбирать место.
    pub id: &'static str,
    /// Как назвать место игроку.
    pub name: &'static str,
    /// Ноги игрока (на мостовой).
    pub pos: Vec3,
    /// Поворот вокруг Y, как `LocalPlayer::yaw` (0 — на север, к морю).
    pub yaw: f32,
}

/// Кто где хочет появиться.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // `At` — для будущего выбора места игроком.
pub enum SpawnChoice {
    /// Любое место города (пока всегда так).
    #[default]
    Random,
    /// Определённое место по `SpawnSpot::id`.
    At(String),
}

impl SpawnChoice {
    /// Для проверок: `MRPG_SPAWN=<id>` на сервере ставит всех в одно место.
    pub fn from_env() -> Self {
        match std::env::var("MRPG_SPAWN") {
            Ok(id) if !id.trim().is_empty() => SpawnChoice::At(id.trim().to_string()),
            _ => SpawnChoice::Random,
        }
    }
}

/// Поворот, при котором игрок смотрит вдоль `dir` (по горизонтали).
fn yaw_towards(dir: Vec2) -> f32 {
    (-dir.x).atan2(-dir.y)
}

/// Все места появления текущего уровня.
pub fn spawn_spots() -> &'static [SpawnSpot] {
    static CITY: OnceLock<Vec<SpawnSpot>> = OnceLock::new();
    static CASTLE: OnceLock<Vec<SpawnSpot>> = OnceLock::new();
    match active() {
        LevelKind::Chersonesus => CITY.get_or_init(city_spots),
        LevelKind::Castle => CASTLE.get_or_init(|| vec![SpawnSpot { id: "yard", name: "Двор замка", pos: super::castle::spawn_point(0), yaw: 0.0 }]),
    }
}

pub fn spawn_spot(id: &str) -> Option<&'static SpawnSpot> {
    spawn_spots().iter().find(|s| s.id == id)
}

fn city_spots() -> Vec<SpawnSpot> {
    let at = |x: f32, z: f32| Vec3::new(x, 0.1, z);
    let mut out = Vec::new();
    let mut add = |id, name, pos: Vec3, look: Vec2| out.push(SpawnSpot { id, name, pos, yaw: yaw_towards(look) });
    let east = Vec2::X;
    let west = Vec2::NEG_X;
    let north = Vec2::NEG_Y;
    let south = Vec2::Y;

    // Агора: у восточного портика (старая точка появления) и у западного края площади.
    let a = super::city::public::agora_rect();
    add("agora", "Агора", city::spawn_point(0), west);
    add("agora_west", "Агора, западный край", at(a.x0 + 4.0, a.z1 - 6.0), east);

    // Главная улица — с запада на восток, у водостока посередине (у краёв — портики и бордюры).
    let main = |i: i32| block(i, 1).cx();
    add("main_west", "Главная улица у западной стены", at(main(2), 0.9), east);
    add("main_tavern", "Главная улица у корчмы", at(main(6), 0.9), east);
    add("main_middle", "Главная улица, середина", at(main(10), -0.9), west);
    add("main_thermae", "Главная улица у терм", at(main(13), -0.9), east);
    add("main_east", "Главная улица у агоры", at(main(15), 0.9), east);

    // Перекрёстки поперечных улиц с продольными (между первым и вторым рядом кварталов).
    let long_land = MAIN_HALF + BZ + LONG_W / 2.0;
    let long_sea = -long_land;
    add("cross_sea_west", "Перекрёсток у моря, запад", at(cross_x(5), long_sea), south);
    add("cross_sea_east", "Перекрёсток у базилики", at(cross_x(12), long_sea), south);
    add("cross_land_west", "Перекрёсток у главных ворот", at(cross_x(6), long_land), north);
    add("cross_land_middle", "Перекрёсток у театра", at(cross_x(11), long_land), north);

    // Площади и дворы.
    let rr = super::city::public::reservoir_rect();
    add("reservoir", "Площадь у резервуара", at(rr.cx(), rr.z0 - 3.5), north);
    add("main_gate", "У главных ворот, изнутри", at(city::main_gate_x() + 8.0, 157.0), north);
    add("garrison", "Плац цитадели", super::city::public::garrison_point() + Vec3::new(-6.0, 0.1, 0.0), west);
    add("port_market", "Портовый торг", super::city::port::market_point() + Vec3::new(0.0, 0.1, 3.5), north);
    out
}

/// Сколько метров вокруг места можно разбросать игроков (чтобы двое не встали друг в друга).
const SCATTER: f32 = 1.6;
/// Ближе этого к другому игроку не ставим, если есть место свободнее.
const CROWD: f32 = 12.0;

/// Выбирает место появления: по желанию игрока или случайное, подальше от других игроков
/// и не там, где он появлялся прошлый раз. Возвращает точку (проверенную на стены) и поворот.
pub fn pick_spawn(choice: &SpawnChoice, level: &LevelColliders, others: &[Vec3], avoid: Option<&str>, mut rand: impl FnMut() -> f32) -> (&'static str, Vec3, f32) {
    let spots = spawn_spots();
    let wanted: Vec<&SpawnSpot> = match choice {
        SpawnChoice::At(id) => spots.iter().filter(|s| s.id == id).collect(),
        SpawnChoice::Random => Vec::new(),
    };
    let pool: Vec<&SpawnSpot> = if !wanted.is_empty() {
        wanted
    } else {
        let all: Vec<&SpawnSpot> = spots.iter().collect();
        let roomy = |s: &&SpawnSpot| others.iter().all(|o| o.xz().distance(s.pos.xz()) > CROWD);
        let fresh = |s: &&SpawnSpot| Some(s.id) != avoid;
        let best: Vec<&SpawnSpot> = all.iter().copied().filter(|s| roomy(s) && fresh(s)).collect();
        if !best.is_empty() {
            best
        } else {
            let fresh_only: Vec<&SpawnSpot> = all.iter().copied().filter(fresh).collect();
            if fresh_only.is_empty() { all } else { fresh_only }
        }
    };
    let spot = pool[((rand() * pool.len() as f32) as usize).min(pool.len() - 1)];
    // Чуть в сторону от точки, если там свободно и никто не стоит.
    for _ in 0..12 {
        let ang = rand() * std::f32::consts::TAU;
        let r = rand().sqrt() * SCATTER;
        let p = spot.pos + Vec3::new(ang.cos() * r, 0.0, ang.sin() * r);
        let clear = others.iter().all(|o| o.xz().distance(p.xz()) > 0.9);
        if clear && level.body_is_free(&Body::STANDING, p) {
            return (spot.id, p, spot.yaw);
        }
    }
    (spot.id, spot.pos, spot.yaw)
}
