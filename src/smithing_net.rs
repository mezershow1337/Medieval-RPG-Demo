//! Сетевые сообщения кузницы. Свой канал [`CH_SMITH`].

use crate::items::{Item, ItemUid};
use crate::protocol::NetPos;
use crate::smithing::{CastMetal, CastPiece, Casting, Forging, Pattern, Quench, Soak, Spot};
use crate::smithing_quick::{Mark, Marks, Order, QuickForge};
use serde::{Deserialize, Serialize};

/// Номер канала renet (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_SMITH: u8 = 10;

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
    /// Поверхность воды в корыте.
    pub trough: NetPos,
    /// Точильный круг.
    pub grind: NetPos,
}

/// Зрелищное событие у горна: его видят и слышат все рядом.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Fx {
    /// Удар молота; `temp` — жар поковки (окалина летит с горячего).
    Strike { heavy: bool, temp: f32 },
    /// Клинок опустили в корыто (или в масло).
    Quench { medium: Quench, temp: f32, cracked: bool },
    /// Касание точила.
    Grind,
    /// Металл горит в горне: сноп искр.
    Burn,
    Crack,
    /// Струя металла из тигля пошла / прервалась.
    Pour(bool),
    /// Сняли шлак с расплава.
    Skim,
    /// Разбили форму.
    OpenMould,
    /// Вещь готова.
    Done,
}

/// Что лежит в горне или на наковальне у любого кузнеца — чтобы и другие видели раскалённое железо.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PieceView {
    pub player: u64,
    pub station: u32,
    pub spot: Spot,
    pub temp: f32,
    /// Что куют (None — тигель с металлом).
    pub pattern: Option<Pattern>,
    /// Насколько уже вытянуто 0..1.
    pub drawn: f32,
    pub pouring: bool,
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
    /// Верстак у кузни: древки или сборка стрел.
    Fletch(crate::archery::Fletch),
    /// Ковка на удачу: заготовка из любого железа, бить, пока не страшно.
    Quick { pattern: Pattern },
}

/// Работа у горна.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Job {
    Forge(Forging),
    Cast(Casting),
    Soak(Soak),
    /// Ремонт: вещь лежит у горна, `progress` 0..1.
    Repair { item: Item, progress: f32 },
    /// Стрельное дело на верстаке. `acc` — средняя аккуратность, `head` — качество наконечников.
    Fletch { what: crate::archery::Fletch, progress: f32, acc: f32, strokes: u16, head: crate::items::Quality, sinew: bool },
    /// Ковка на удачу ([`crate::smithing_quick`]).
    Quick(QuickForge),
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
    /// Перевернуть поковку клещами на другую грань.
    Flip,
    Quench(Quench),
    Grind { acc: f32 },
    /// Снять шлак с расплава.
    Skim,
    /// Поставить форму греться у горна.
    WarmMould(bool),
    Pour(bool),
    OpenMould,
    /// Собрать вещь (`fitting` — с бронзовым навершием). В ковке на удачу — снять с наковальни.
    Finish { fitting: bool },
    Abandon,
    /// Выменять клеймо мастера на готовые вещи.
    BuyMark(Mark),
    /// Сдать вещь в заказ мастера.
    Deliver,
    /// Попросить у мастера другой заказ.
    NewOrder,
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
    Fx { station: u32, player: u64, fx: Fx },
    /// Все раскалённые поковки и тигли (несколько раз в секунду, пока что-то есть).
    Pieces(Vec<PieceView>),
    /// Свои клейма мастера и заказ.
    Guild(GuildView),
}

/// Что мастер кузни знает об игроке: клейма, заказ, сколько сдано.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct GuildView {
    pub marks: Marks,
    pub order: Option<Order>,
    pub delivered: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WorkView {
    pub station: u32,
    pub job: Job,
    pub bellows: bool,
}

impl PieceView {
    /// Что видно у горна от этой работы (ремонт и плавка — без поковки в клещах).
    pub fn of(player: u64, station: u32, job: &Job) -> Option<Self> {
        match job {
            Job::Forge(f) => Some(Self { player, station, spot: f.spot, temp: f.temp, pattern: Some(f.pattern), drawn: f.drawn(), pouring: false }),
            Job::Cast(c) if !c.opened => Some(Self { player, station, spot: c.spot, temp: c.temp, pattern: None, drawn: c.poured, pouring: c.pouring }),
            Job::Quick(q) if !q.shattered => Some(Self { player, station, spot: Spot::Anvil, temp: q.temp(), pattern: Some(q.pattern), drawn: q.drawn(), pouring: false }),
            _ => None,
        }
    }
}
