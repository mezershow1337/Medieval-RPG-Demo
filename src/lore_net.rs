//! Сетевые сообщения знаний: чтение, сбор трав, уроки. Свой канал [`CH_LORE`].

use crate::items::ItemUid;
use crate::lore::{Knowledge, Substance, Teacher};
use crate::brewing::{BrewSpec, Heat, Vessel};
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
    /// Радиус заросли: собирать можно, стоя в ней.
    pub radius: f32,
}

/// Номера одиночных кустиков в поле начинаются отсюда (меньшие — заросли города).
pub const WILD_PATCH_BASE: u32 = 1000;

/// Рабочее место, поставленное игроком на землю.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StationKind {
    /// Медная жаровня на треноге: огонь где угодно (жжёт древесный уголь).
    Brazier,
    /// Алхимический стол: жаровня, весы, ступка, подставка под алембик.
    Table,
}

impl StationKind {
    pub fn item(self) -> &'static str {
        match self {
            StationKind::Brazier => "Жаровня",
            StationKind::Table => "Алхимический стол",
        }
    }

    pub fn from_item(name: &str) -> Option<Self> {
        match name {
            "Жаровня" => Some(StationKind::Brazier),
            "Алхимический стол" => Some(StationKind::Table),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StationView {
    pub id: u32,
    pub kind: StationKind,
    pub pos: NetPos,
    pub yaw: f32,
}

/// Кто-то работает у огня: где стоит сосуд и что в нём творится (для пара, дыма, капель).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct WorkView {
    pub vessel: Vessel,
    pub heat: Heat,
    /// Где стоит сосуд (дно).
    pub pos: NetPos,
    pub yaw: f32,
    /// Огонь разведён прямо под сосудом (нет ни очага, ни жаровни рядом).
    pub own_fire: bool,
}

/// Чем кончилось дело — разовый эффект для всех вокруг.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurstFx {
    /// Получилось: клуб пара.
    Done,
    /// Бурда: бурый чад.
    Spoiled,
    /// Кипело через край: брызги и пар.
    BoilOver,
    /// Сосуд лопнул: осколки (стекло или глина).
    Shatter { glass: bool },
    /// Смесь вспыхнула: огненный шар и чёрный дым.
    Ignition,
    /// Едкие или ядовитые пары.
    Fumes,
    /// Траву сорвали: листья и пыль.
    Gathered,
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
    /// Варить в сосуде из своей сумки.
    Brew { vessel: ItemUid, spec: BrewSpec },
    /// Поставить жаровню или стол из сумки перед собой.
    PlaceStation { uid: ItemUid },
    /// Забрать поставленное обратно в сумку.
    TakeStation { id: u32 },
    /// Собрать на жаровне алхимический стол (нужны весы и ступка).
    UpgradeStation { id: u32 },
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
    /// Одиночный кустик вырос на новом месте (или его сорвали: charges 0).
    PatchMoved { id: u32, pos: NetPos, charges: u8 },
    Masters(Vec<MasterView>),
    /// Все поставленные жаровни и столы (после каждого изменения).
    Stations(Vec<StationView>),
    /// Игрок начал (Some) или закончил (None) работу у сосуда.
    Work { player: u64, work: Option<WorkView> },
    /// Разовый эффект в точке.
    Burst { pos: NetPos, fx: BurstFx },
    /// Началось занятие: подпись, сколько уже прошло и сколько всего (секунды).
    Activity { verb: String, done: f32, total: f32 },
    ActivityEnded,
    Notice(String),
    /// Раны, яды и прочее (раз в полсекунды, пока не здоров; пустое — выздоровел).
    Body(Body),
}
