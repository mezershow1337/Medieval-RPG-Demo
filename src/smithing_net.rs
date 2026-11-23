//! Сетевые сообщения кузницы. Свой канал [`CH_SMITH`].

use crate::items::{Item, ItemUid};
use crate::protocol::NetPos;
use crate::smithing::{CastMetal, CastPiece, Casting, Forging, Pattern, Quench, Soak, Spot};
use serde::{Deserialize, Serialize};

/// Номер канала renet (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_SMITH: u8 = 9;

/// Дальше этого (метры от наковальни) у горна не поработать.
pub const SMITH_REACH: f32 = 4.0;

/// Что за рабочее место.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StationKind {
    /// Горн с мехами, наковальня, корыто, точило.
    Forge,
    /// Сыродутный горн: глиняная шахта для плавки руды.
    Bloomery,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StationView {
    pub id: u32,
    pub name: String,
    pub kind: StationKind,
    pub fire: NetPos,
    pub anvil: NetPos,
}

/// Что игрок начинает делать.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Plan {
    /// Ковать из полос (или крицы, или лома) из своей сумки.
    Forge { pattern: Pattern, metal: Vec<ItemUid> },
    /// Литьё: сколько штук какого металла положить в тигель.
    Cast { piece: CastPiece, metals: Vec<(CastMetal, u8)> },
    /// Заложить сыродутный горн.
    Smelt { ore: u8, charcoal: u8 },
    /// Цементация железной полосы.
    Carburize { bar: ItemUid },
    /// Выправить и наточить свою вещь.
    Repair { uid: ItemUid },
}

/// Работа у горна.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Job {
    Forge(Forging),
    Cast(Casting),
    Soak(Soak),
    /// Ремонт: вещь лежит у горна, `progress` 0..1.
    Repair { item: Item, progress: f32 },
}

/// Клиент -> сервер.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SmithClientMessage {
    Begin { station: u32, plan: Plan },
    /// Подсыпать угля в горн (одна мера из сумки).
    AddFuel { station: u32 },
    /// Качать меха (держать) или отпустить.
    Bellows(bool),
    /// Переложить поковку или тигель.
    Move(Spot),
    Strike { zone: u8, heavy: bool, acc: f32 },
    Quench(Quench),
    Grind { acc: f32 },
    /// Снять шлак с расплава.
    Skim,
    /// Поставить форму греться у горна.
    WarmMould(bool),
    Pour(bool),
    OpenMould,
    /// Собрать вещь (`fitting` — с бронзовым навершием).
    Finish { fitting: bool },
    Abandon,
}

/// Сервер -> клиент.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SmithServerMessage {
    Stations(Vec<StationView>),
    /// Сколько угля в горне (секунды) и сила дутья 0..1.
    Fire { station: u32, fuel: f32, blast: f32 },
    /// Своя работа (None — нет).
    Work(Option<WorkView>),
    Notice(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WorkView {
    pub station: u32,
    pub job: Job,
    pub bellows: bool,
}
