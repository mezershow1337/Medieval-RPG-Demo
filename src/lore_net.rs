//! Сетевые сообщения знаний: чтение, сбор трав, уроки. Свой канал [`CH_LORE`].

use crate::items::ItemUid;
use crate::lore::{Knowledge, Substance, Teacher};
use crate::medicine::Body;
use crate::protocol::NetPos;
use serde::{Deserialize, Serialize};

/// Номер канала renet (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_LORE: u8 = 4;

/// Дальше этого (метры от ног) не дотянуться до травы или мастера.
pub const LORE_REACH: f32 = 2.6;

/// Точка, где растёт трава.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PatchView {
    pub id: u32,
    pub substance: Substance,
    pub pos: NetPos,
    /// Сколько раз ещё можно собрать (0 — пусто, отрастает).
    pub charges: u8,
    /// Рисовать ли куст самим (false — его рисует уровень).
    pub draw: bool,
}

/// Временный мастер, пока в городе нет настоящих NPC.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MasterView {
    pub id: u32,
    pub teacher: Teacher,
    pub pos: NetPos,
    pub yaw: f32,
    /// Цена часа урока в нуммиях (см. economy.rs).
    pub price: u32,
    /// Сколько нуммиев платит подмастерью за час работы.
    pub wage: u32,
}

/// Клиент -> сервер.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LoreClientMessage {
    /// Читать книгу или свиток из своей сумки.
    Read { uid: ItemUid },
    /// Собрать траву.
    Gather { patch: u32 },
    /// Платный урок у мастера.
    PaidLesson { master: u32 },
    /// Поработать подмастерьем.
    Apprentice { master: u32 },
    /// Приложить к ране или выпить (лекарство, а то и яд).
    Treat { uid: ItemUid },
    /// Прервать текущее занятие.
    Stop,
}

/// Сервер -> клиент.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LoreServerMessage {
    /// Всё, что знает персонаж (после каждого изменения).
    Knowledge(Knowledge),
    Patches(Vec<PatchView>),
    PatchCharges { id: u32, charges: u8 },
    Masters(Vec<MasterView>),
    /// Началось занятие: подпись, сколько уже прошло и сколько всего (секунды).
    Activity { verb: String, done: f32, total: f32 },
    ActivityEnded,
    Notice(String),
    /// Раны, яды и прочее (раз в полсекунды, пока не здоров; пустое — выздоровел).
    Body(Body),
}
