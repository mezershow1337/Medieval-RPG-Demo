//! Сетевые сообщения горного дела и углежжения. Свой канал [`CH_MINE`] (сервер → клиент);
//! просьбы игрока идут по каналу знаний (`LoreClientMessage::Mine`), чтобы работа руками
//! шла тем же путём, что сбор трав и чтение.

use crate::mining::{Pile, PileKind, Smoke, Stage};
use serde::{Deserialize, Serialize};

/// Номер канала renet (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_MINE: u8 = 14;

/// Дальше этого (м) от кучи или места добычи не поработать.
pub const MINE_REACH: f32 = 3.2;

/// Что игрок делает у места добычи или у кучи.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MineAct {
    /// Копать, бить, колоть, крутить ворот (работа руками).
    Dig { site: u32 },
    /// Сложить поленья из сумки (работа руками).
    StackWood { pile: u32 },
    /// Насыпать руду на кострище (сначала обожжённую не берёт).
    LoadOre { pile: u32 },
    /// Укрыть угольную кучу листвой, дёрном и землёй (заступ, работа руками).
    Cover { pile: u32 },
    Light { pile: u32 },
    /// Открыть или закрыть отдушину.
    Vent { pile: u32, open: bool },
    /// Засыпать прогар землёй (работа руками).
    Patch { pile: u32 },
    /// Заглушить угольную кучу.
    Smother { pile: u32 },
    /// Ворошить руду на кострище (работа руками).
    Stir { pile: u32 },
    /// Подбросить дров в кострище.
    Feed { pile: u32 },
    /// Разобрать остывшую кучу (работа руками).
    Rake { pile: u32 },
}

/// Куча для всех: где, какая, что видно со стороны.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PileView {
    pub id: u32,
    pub kind: PileKind,
    pub pos: [f32; 3],
    pub stage: Stage,
    pub wood: u16,
    pub ore: u16,
    pub cover: f32,
    pub vents: u8,
    pub smoke: Smoke,
    /// Сила огня / тления 0..~1.5 (отсвет, пламя).
    pub heat: f32,
    pub cracks: f32,
    /// Полная модель (для подсказок рядом стоящему; знание решает, что из неё прочтут).
    pub pile: Pile,
}

/// Сервер -> клиент.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MineServerMessage {
    /// Сколько осталось в каждом месте добычи (по номеру места).
    Sites(Vec<(u32, u8)>),
    SiteCharges { id: u32, charges: u8 },
    Piles(Vec<PileView>),
    Pile(PileView),
}
