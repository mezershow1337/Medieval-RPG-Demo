//! Сетевые сообщения рыбалки. Свой канал [`CH_FISH`] (надёжный, по порядку).
//!
//! Клиент сам рисует заброс, поплавок и вываживание (fishing_client.rs), а сервер решает,
//! кто клюнет и когда, и верит пойманной рыбе, только если по времени всё сходится
//! (fishing_server.rs).

use crate::fishing::{Bait, FightSpec};
use serde::{Deserialize, Serialize};

/// Номер канала renet. Настроен в `protocol::connection_config`.
pub const CH_FISH: u8 = 15;

/// Клиент -> сервер.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum FishClientMessage {
    /// Поплавок упал на воду в точке `at` (уровень воды), на крючке — `bait`.
    Cast { at: [f32; 3], bait: Bait },
    /// Подсечь (поплавок ушёл под воду).
    Hook,
    /// Рыба у ног — вытащили.
    Landed,
    /// Рыба ушла: `snapped` — леса лопнула (крючок пропал).
    Lost { snapped: bool },
    /// Смотать удочку без рыбы.
    Reel,
}

/// Сервер -> клиент.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum FishServerMessage {
    /// Заброс принят: через `bite_in` секунд клюнет по-настоящему, до того — поклёвки-пробы
    /// (секунды от заброса). `spec` — как рыба будет тянуть, `line` и `reel` — от удочки.
    Cast { bite_in: f32, nibbles: Vec<f32>, spec: FightSpec, line: f32, reel: f32 },
    /// Заброс не принят (нет крючка, наживки, воды…), удочку сматываем.
    Refused(String),
    /// Подсекли вовремя: рыба на крючке.
    Hooked,
    /// Подсекли не вовремя: рыба ушла (текст — почему).
    Missed(String),
    /// Пойманная рыба: название предмета и вес.
    Caught { name: String, kg: f32 },
    Notice(String),
}
