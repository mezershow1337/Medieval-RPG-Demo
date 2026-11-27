//! Общий сетевой протокол: сообщения между клиентом и сервером.

use crate::combat::{Action, HitOutcome, Swing, WeaponKind};
use bevy::math::Vec3;
use bevy_renet::renet::{ChannelConfig, ConnectionConfig, DefaultChannel, SendType};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Меняйте при несовместимых изменениях протокола, чтобы старые клиенты не подключались.
pub const PROTOCOL_ID: u64 = 0x4D45_4449_0033;
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
    // Канал 7 — торговля, фракции и обмен между игроками (см. trade_net.rs).
    let trade = ChannelConfig {
        channel_id: crate::trade_net::CH_TRADE,
        max_memory_usage_bytes: 5 * 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(trade.clone());
    config.client_channels_config.push(trade);
    // Канал 4 — знания, чтение, сбор трав, уроки (см. lore_net.rs).
    let lore = ChannelConfig {
        channel_id: crate::lore_net::CH_LORE,
        max_memory_usage_bytes: 2 * 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(lore.clone());
    config.client_channels_config.push(lore);
    // Канал 8 — двери (см. doors.rs).
    let doors = ChannelConfig {
        channel_id: crate::doors::CH_DOOR,
        max_memory_usage_bytes: 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(doors.clone());
    config.client_channels_config.push(doors);
    // Канал 9 — положения движущихся предметов (см. props_server.rs), терять не страшно.
    let props = ChannelConfig {
        channel_id: crate::loot_net::CH_PROPS,
        max_memory_usage_bytes: 2 * 1024 * 1024,
        send_type: SendType::Unreliable,
    };
    config.server_channels_config.push(props.clone());
    config.client_channels_config.push(props);
    // Канал 10 — кузницы (см. smithing_net.rs).
    let smith = ChannelConfig {
        channel_id: crate::smithing_net::CH_SMITH,
        max_memory_usage_bytes: 2 * 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(smith.clone());
    config.client_channels_config.push(smith);
    // Канал 11 — консоль разработчика (см. console_net.rs).
    let console = ChannelConfig {
        channel_id: crate::console_net::CH_CONSOLE,
        max_memory_usage_bytes: 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(console.clone());
    config.client_channels_config.push(console);
    // Канал 12 — карта: имена игроков и метки (см. map.rs).
    let map = ChannelConfig {
        channel_id: crate::map::CH_MAP,
        max_memory_usage_bytes: 512 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(map.clone());
    config.client_channels_config.push(map);
    // Канал 13 — города, вести и свои дома (см. estates.rs).
    let town = ChannelConfig {
        channel_id: crate::estates::CH_TOWN,
        max_memory_usage_bytes: 2 * 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(town.clone());
    config.client_channels_config.push(town);
    // Канал 14 — рудники, углежоги, кучи и места добычи (см. mining_net.rs).
    let mine = ChannelConfig {
        channel_id: crate::mining_net::CH_MINE,
        max_memory_usage_bytes: 1024 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(mine.clone());
    config.client_channels_config.push(mine);
    // Канал 15 — рыбалка: заброс, поклёвка, вываживание (см. fishing_net.rs).
    let fish = ChannelConfig {
        channel_id: crate::fishing_net::CH_FISH,
        max_memory_usage_bytes: 256 * 1024,
        send_type: SendType::ReliableOrdered { resend_time: std::time::Duration::from_millis(300) },
    };
    config.server_channels_config.push(fish.clone());
    config.client_channels_config.push(fish);
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
    PlayerState { pos: NetPos, yaw: f32, pitch: f32, crouching: bool, sprinting: bool },
    /// Начать удар (надёжный канал). `swing` — направление, выбранное мышью
    /// (`None` — по серии). Во время замаха — морф, после парирования — рипост.
    Attack { yaw: f32, pitch: f32, swing: Option<Swing> },
    /// Финт: бросить свой замах.
    Feint,
    /// Зажать / отпустить защиту (надёжный канал).
    Guard { on: bool, yaw: f32, pitch: f32 },
    /// Убрать оружие и драться кулаками / снова взять оружие (хотбар).
    Holster { on: bool },
    /// Ответ стражнику, который вас остановил.
    Law(crate::civilians::LawAnswer),
}

/// Кто это: игрок или враг какого типа. Люди Херсонеса и его округи около 510 г.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActorKind {
    Player,
    /// Стратиот гарнизона: спата и большой овальный щит, кольчуга, шлем-спангенхельм.
    Stratiotes,
    /// Букелларий: тяжёлый воин из личной дружины, ламеллярный доспех, спата и овальный щит.
    Bucellarius,
    /// Крымский гот: бородовидный топор и круглый щит, почти без доспеха.
    GothWarrior,
    /// Алан: копьё-контос двумя руками, ламеллярный доспех.
    AlanLancer,
    /// Гунно-булгарский кочевник: составной лук, вблизи — нож-сакс.
    HunArcher,
    /// Горожанин, схвативший что под руку попало (топор, вилы, тесак), когда поднялся шум.
    Townsman,
}

impl ActorKind {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ENEMIES: [ActorKind; 5] = [ActorKind::Stratiotes, ActorKind::Bucellarius, ActorKind::GothWarrior, ActorKind::AlanLancer, ActorKind::HunArcher];

    pub fn is_npc(self) -> bool {
        !matches!(self, ActorKind::Player)
    }
    /// Могут ли эти двое задеть друг друга оружием. Люди мира между собой не дерутся;
    /// на кого из игроков нападать, решает ИИ по репутации фракции (см. `npc::Brain`).
    pub fn hostile(self, other: ActorKind) -> bool {
        self == ActorKind::Player || other == ActorKind::Player
    }
    /// Название для интерфейса.
    pub fn name_ru(self) -> &'static str {
        match self {
            ActorKind::Player => "Игрок",
            ActorKind::Stratiotes => "Стратиот",
            ActorKind::Bucellarius => "Букелларий",
            ActorKind::GothWarrior => "Готский воин",
            ActorKind::AlanLancer => "Аланский копейщик",
            ActorKind::HunArcher => "Кочевник-лучник",
            ActorKind::Townsman => "Горожанин",
        }
    }
}

/// Сервер -> клиент (надёжный канал).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    /// Вы в игре: ваш номер, где появились и куда смотрите (`yaw`, как у `LocalPlayer`).
    Welcome { your_id: u64, spawn: NetPos, yaw: f32 },
    PlayerJoined { id: u64 },
    PlayerLeft { id: u64 },
    /// Сервер отклонил перемещение и возвращает игрока в эту точку.
    Correction { pos: NetPos },
    /// Результат попадания клинка.
    Hit { attacker: u64, target: u64, outcome: HitOutcome, at: NetPos },
    Died { id: u64, killer: Option<u64> },
    /// Ваш персонаж возрождён в этой точке.
    Respawned { pos: NetPos, yaw: f32 },
    /// На земле появилось тело (его можно будет обыскать).
    CorpseSpawned { corpse_id: u64, kind: ActorKind, pos: NetPos, yaw: f32, gear: crate::gear_look::Gear },
    CorpseRemoved { corpse_id: u64 },
    /// Ранили мирного человека.
    CivilianHit { civ: crate::civilians::Civ, at: NetPos, amount: f32, head: bool },
    /// Мирный человек погиб; тело (если ещё лежит): id для обыска, точка и поворот.
    CivilianDead { civ: crate::civilians::Civ, corpse: Option<(u64, NetPos, f32)> },
    /// На место погибшего пришёл другой человек (наследник лавки, новый жилец).
    CivilianBack { civ: crate::civilians::Civ },
    /// Закон: свидетели, стража, карцер.
    Law(crate::civilians::LawMessage),
    /// Часы мира: игровые секунды от полуночи первого дня (см. `world_clock.rs`).
    Clock { game: f64 },
    /// Имя игрока (ник из главного меню). Приходит всем при входе игрока и новичку — про всех.
    PlayerName { id: u64, name: String },
}

/// Ник игрока: не длиннее 24 символов, без управляющих символов.
pub fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(24).collect::<String>().trim().to_string()
}

/// Строка снимка про одного бойца. Снимок у каждого клиента свой (см. `server::send_snapshots`):
/// в нём игроки и те, кто рядом, а облик и свои силы — только когда они нужны.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActorSnapshot {
    pub id: u64,
    pub kind: ActorKind,
    pub pos: NetPos,
    /// Поворот и наклон взгляда: по сети 2 байта вместо 4 (см. `angle16`).
    #[serde(with = "angle16")]
    pub yaw: f32,
    #[serde(with = "angle16")]
    pub pitch: f32,
    pub crouching: bool,
    pub weapon: WeaponKind,
    pub action: Action,
    pub hp: f32,
    pub max_hp: f32,
    /// Защищается щитом (а не оружием).
    pub shield: bool,
    /// Множитель скорости ударов (экипировка).
    pub action_speed: f32,
    /// Вид клинка в руке: 0 — ржавый и грубый, 255 — полированный (см. `Item::look`).
    pub finish: u8,
    /// Видимые доспехи и щит (у игроков), чтобы модель была одета как в инвентаре.
    /// Приходят, когда боец только что попал в снимок, когда вещи сменились и раз в 2 с
    /// (на случай потери пакетов); `None` — облик прежний.
    pub gear: Option<crate::gear_look::Gear>,
    /// Только в своей строке: силы и скорость ходьбы своего персонажа.
    pub own: Option<OwnState>,
}

/// То, что нужно только самому игроку (полоска сил, скорость ходьбы).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct OwnState {
    pub stamina: f32,
    pub max_stamina: f32,
    /// Множитель скорости ходьбы (экипировка и вес доспеха).
    pub move_mult: f32,
}

/// Угол в 2 байтах (шаг около 0,0001 рад). Поворот заворачивается в −π..π
/// (модели поворачиваются через кватернион, им всё равно), наклон и так в пределах ±1,6.
mod angle16 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::f32::consts::{PI, TAU};

    const SCALE: f32 = 32767.0 / PI;

    pub fn to_bits(a: f32) -> [u8; 2] {
        let wrapped = if a.is_finite() { (a + PI).rem_euclid(TAU) - PI } else { 0.0 };
        ((wrapped * SCALE).round().clamp(-32767.0, 32767.0) as i16).to_le_bytes()
    }

    pub fn from_bits(b: [u8; 2]) -> f32 {
        i16::from_le_bytes(b) as f32 / SCALE
    }

    pub fn serialize<S: Serializer>(a: &f32, s: S) -> Result<S::Ok, S::Error> {
        to_bits(*a).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        Ok(from_bits(<[u8; 2]>::deserialize(d)?))
    }
}

/// Летящая стрела.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ArrowSnapshot {
    pub id: u64,
    pub pos: NetPos,
    pub vel: NetPos,
}

/// Сервер -> клиент (ненадёжный канал), рассылается SNAPSHOT_RATE_HZ раз в секунду.
/// У каждого клиента свой: все игроки, а враги и стрелы — только поблизости.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Snapshot {
    pub tick: u32,
    pub actors: Vec<ActorSnapshot>,
    pub arrows: Vec<ArrowSnapshot>,
}

pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).expect("сериализация сообщения")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Феодор Херсонит "), "Феодор Херсонит");
        assert_eq!(clean_name(&"Ж".repeat(100)).chars().count(), 24);
        assert_eq!(clean_name("a\nb\tc"), "abc");
    }
}

#[cfg(test)]
mod snapshot_format_tests {
    use super::*;

    #[test]
    fn angles_survive_two_bytes() {
        use std::f32::consts::{PI, TAU};
        for a in [0.0f32, 0.5, -1.6, 1.6, 3.1, -3.1, PI - 1e-4, 10.0, -25.0] {
            let back = angle16::from_bits(angle16::to_bits(a));
            let diff = (back - a).rem_euclid(TAU);
            let diff = diff.min(TAU - diff);
            assert!(diff < 2e-4, "{a} -> {back}");
        }
        assert_eq!(angle16::from_bits(angle16::to_bits(f32::NAN)), 0.0);
        assert_eq!(angle16::from_bits(angle16::to_bits(f32::INFINITY)), 0.0);
    }

    #[test]
    fn enemy_line_is_small() {
        let a = ActorSnapshot {
            id: 1 << 62,
            kind: ActorKind::GothWarrior,
            pos: NetPos([120.5, 3.2, -310.0]),
            yaw: 2.0,
            pitch: -0.3,
            crouching: false,
            weapon: WeaponKind::Axe,
            action: Action::Idle,
            hp: 80.0,
            max_hp: 100.0,
            shield: true,
            action_speed: 1.0,
            finish: 120,
            gear: None,
            own: None,
        };
        let bytes = encode(&a);
        // Было 77 байт (облик и силы в каждой строке, углы по 4 байта).
        assert!(bytes.len() <= 46, "{} байт", bytes.len());
        let back: ActorSnapshot = decode(&bytes).unwrap();
        assert!((back.yaw - 2.0).abs() < 1e-3 && (back.pitch + 0.3).abs() < 1e-3);
        assert!(back.gear.is_none() && back.own.is_none());
    }
}
