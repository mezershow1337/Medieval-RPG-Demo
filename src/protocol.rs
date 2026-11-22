//! Общий сетевой протокол: сообщения между клиентом и сервером.

use crate::combat::{Action, HitOutcome, WeaponKind};
use bevy::math::Vec3;
use bevy_renet::renet::{ChannelConfig, ConnectionConfig, DefaultChannel, SendType};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Меняйте при несовместимых изменениях протокола, чтобы старые клиенты не подключались.
pub const PROTOCOL_ID: u64 = 0x4D45_4449_0003;
pub const DEFAULT_PORT: u16 = 5000;
pub const MAX_PLAYERS: usize = 16;

/// Сколько раз в секунду сервер рассылает состояние мира.
pub const SNAPSHOT_RATE_HZ: f64 = 30.0;

/// Канал для частых данных, которые не страшно потерять (позиции).
pub const CH_UNRELIABLE: DefaultChannel = DefaultChannel::Unreliable;
/// Канал для важных событий (вход, выход, коррекция позиции).
pub const CH_RELIABLE: DefaultChannel = DefaultChannel::ReliableOrdered;

pub fn connection_config() -> ConnectionConfig {
    let mut config = ConnectionConfig::default();
    // Канал 3 — добыча и инвентарь (см. loot_net.rs).
    let loot = ChannelConfig {
        channel_id: crate::loot_net::CH_LOOT,
        max_memory_usage_bytes: 5 * 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(loot.clone());
    config.client_channels_config.push(loot);
    config
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct NetPos(pub [f32; 3]);

impl From<Vec3> for NetPos {
    fn from(v: Vec3) -> Self {
        Self(v.to_array())
    }
}

impl From<NetPos> for Vec3 {
    fn from(p: NetPos) -> Self {
        Vec3::from_array(p.0)
    }
}

/// Клиент -> сервер.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    /// Состояние своего персонажа (ненадёжный канал). Сервер проверяет, что скорость правдоподобная.
    PlayerState { pos: NetPos, yaw: f32, pitch: f32, crouching: bool },
    /// Начать удар (надёжный канал).
    Attack { yaw: f32, pitch: f32 },
    /// Зажать / отпустить защиту (надёжный канал).
    Guard { on: bool, yaw: f32, pitch: f32 },
}

/// Кто это: игрок или враг какого типа.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActorKind {
    Player,
    /// Рыцарь с мечом и щитом.
    Swordsman,
    /// Тяжёлый рыцарь с двуручным мечом.
    HeavyKnight,
}

impl ActorKind {
    pub fn is_npc(self) -> bool {
        !matches!(self, ActorKind::Player)
    }
}

/// Сервер -> клиент (надёжный канал).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    Welcome { your_id: u64, spawn: NetPos },
    PlayerJoined { id: u64 },
    PlayerLeft { id: u64 },
    /// Сервер отклонил перемещение и возвращает игрока в эту точку.
    Correction { pos: NetPos },
    /// Результат попадания клинка.
    Hit { attacker: u64, target: u64, outcome: HitOutcome, at: NetPos },
    Died { id: u64, killer: Option<u64> },
    /// Ваш персонаж возрождён в этой точке.
    Respawned { pos: NetPos },
    /// На земле появилось тело (его можно будет обыскать).
    CorpseSpawned { corpse_id: u64, kind: ActorKind, pos: NetPos, yaw: f32 },
    CorpseRemoved { corpse_id: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActorSnapshot {
    pub id: u64,
    pub kind: ActorKind,
    pub pos: NetPos,
    pub yaw: f32,
    pub pitch: f32,
    pub crouching: bool,
    pub weapon: WeaponKind,
    pub action: Action,
    pub hp: f32,
    pub max_hp: f32,
    /// Защищается щитом (а не оружием).
    pub shield: bool,
}

/// Сервер -> клиент (ненадёжный канал), рассылается SNAPSHOT_RATE_HZ раз в секунду.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Snapshot {
    pub tick: u32,
    pub actors: Vec<ActorSnapshot>,
}

pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).expect("сериализация сообщения")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}
