//! Сетевые сообщения торговли, фракций и обмена между игроками. Свой канал [`CH_TRADE`].

use crate::factions::{Faction, Reputation};
use crate::items::{Item, ItemUid};
use crate::protocol::NetPos;
use crate::trade::{Attention, Look, MerchantId, MoneyPolicy};
use serde::{Deserialize, Serialize};

/// Канал renet для торговли (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_TRADE: u8 = 7;

/// С какого расстояния можно говорить с торговцем (метры).
pub const TALK_RANGE: f32 = 3.5;
/// Обмен между игроками — на расстоянии вытянутой руки.
pub const P2P_RANGE: f32 = 4.0;
/// Во время урока или смены нельзя отходить от мастера дальше этого.
pub const LESSON_RANGE: f32 = 8.0;

/// Строка сделки: предмет и сколько штук из стопки.
pub type Line = (ItemUid, u16);

/// Клиент -> сервер.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TradeClientMessage {
    /// Заговорить с торговцем (E).
    Open {
        merchant: MerchantId,
    },
    Close,
    /// Сделка целиком: что берём с прилавка и что отдаём из своей сумки.
    Deal {
        buy: Vec<Line>,
        sell: Vec<Line>,
    },
    /// Стянуть вещь с прилавка (одну штуку).
    Steal {
        uid: ItemUid,
    },
    /// Заказать вещь у мастера (номер в списке заказов), половина платы вперёд.
    PlaceOrder {
        index: u8,
    },
    /// Забрать готовый заказ (доплатить остаток).
    CollectOrder {
        id: u64,
    },
    /// Начать урок или смену (номер в списке уроков).
    StartLesson {
        index: u8,
    },
    /// Бросить урок.
    StopLesson,
    /// Уплатить штраф (у казначея гарнизона).
    PayFine,
    /// Пожертвовать храму.
    Donate {
        nummi: u32,
    },
    /// Предложить обмен игроку (T, глядя на него).
    P2pRequest {
        target: u64,
    },
    /// Ответ на приглашение.
    P2pRespond {
        from: u64,
        accept: bool,
    },
    /// Своё предложение в обмене: вещи и деньги.
    P2pOffer {
        items: Vec<Line>,
        nummi: u32,
    },
    /// Согласен на обмен (или снять согласие).
    P2pConfirm(bool),
    P2pCancel,
}

/// Торговец в мире (рассылается при входе).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MerchantInfo {
    pub id: MerchantId,
    pub name: String,
    pub title: String,
    pub faction: Faction,
    pub pos: NetPos,
    pub yaw: f32,
    pub look: Look,
}

/// Товар на полке.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShelfItem {
    pub item: Item,
    /// Цена за штуку.
    pub price: u32,
    /// Можно ли стянуть (мелкая вещь).
    pub pocketable: bool,
}

/// Что торговец даст за вещь игрока.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Bid {
    pub uid: ItemUid,
    /// Цена за штуку; `None` — не берёт.
    pub price: Option<u32>,
    /// Вещь запрещено продавать этому торговцу (оружие варварам) — предупреждение в окне.
    pub contraband: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OrderOffer {
    pub index: u8,
    pub item: String,
    pub count: u16,
    pub price: u32,
    pub secs: f32,
    /// Почему нельзя заказать (мало уважения), или None.
    pub locked: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OrderStatus {
    pub id: u64,
    pub item: String,
    pub count: u16,
    /// Секунд до готовности (0 — готово).
    pub remaining: f32,
    /// Сколько доплатить при получении.
    pub due: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LessonOffer {
    pub index: u8,
    pub title: String,
    pub paid: bool,
    pub money: u32,
    pub secs: f32,
    pub locked: Option<String>,
}

/// Всё, что видно в окне торговца.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MerchantView {
    pub id: MerchantId,
    pub name: String,
    pub title: String,
    pub greeting: String,
    pub faction: Faction,
    pub rep: f32,
    pub money: MoneyPolicy,
    pub attention: Attention,
    /// Касса (сколько торговец может заплатить деньгами).
    pub cash: u32,
    pub shelf: Vec<ShelfItem>,
    pub bids: Vec<Bid>,
    pub orders: Vec<OrderOffer>,
    pub my_orders: Vec<OrderStatus>,
    pub lessons: Vec<LessonOffer>,
    pub fines: bool,
    pub donations: bool,
    /// Торговец отказывается торговать: почему.
    pub refuse: Option<String>,
}

/// Состояние обмена между игроками.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct P2pState {
    pub partner: u64,
    pub mine: Vec<Item>,
    pub my_nummi: u32,
    pub theirs: Vec<Item>,
    pub their_nummi: u32,
    pub i_confirmed: bool,
    pub they_confirmed: bool,
}

/// Сервер -> клиент.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TradeServerMessage {
    Merchants(Vec<MerchantInfo>),
    /// Окно торговца открыто или обновилось.
    View(MerchantView),
    Closed,
    /// Репутация у фракций и сумма штрафа к уплате (0 — не в розыске).
    Standing {
        rep: Reputation,
        fine: u32,
    },
    /// Начался урок/смена: подпись и длительность.
    LessonStarted {
        title: String,
        secs: f32,
    },
    LessonFinished {
        ok: bool,
    },
    /// Короткая реплика торговца в окне («Держи, носи на здоровье»).
    Say(String),
    P2pInvite {
        from: u64,
    },
    P2p(P2pState),
    P2pClosed {
        reason: String,
    },
}
