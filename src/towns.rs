//! Поселения Таврики около 510 года живут сами по себе.
//!
//! Каждую игровую неделю (по умолчанию 4 минуты реального времени, `MRPG_WEEK_SECS`) у каждого
//! поселения меняются население, запасы, казна и настроение, по дорогам и морю идут обозы и
//! корабли, случаются неурожаи, набеги, мор, пожары, праздники. Из запасов выходят цены: в
//! Херсоне они двигают цены у торговцев (`market_mult`), пополнение их полок (`supply_mult`)
//! и стоимость домов.
//!
//! # Откуда цифры
//!
//! * Херсон VI века — город тысяч на десять жителей, живёт морем: солёная рыба (рыбозасолочные
//!   цистерны в каждом квартале), соль с озёр, вино своих клеров. Своего хлеба мало — зерно
//!   везут готы Дори, Боспор и корабли из-за моря; поэтому в городе большие хорреи.
//! * Готы страны Дори (Прокопий: «готы, числом три тысячи»), их сёла в долинах у Херсона — хлеб,
//!   лес, шкуры, мёд и воск.
//! * Аланы предгорий и гунно-булгарские кочевники — скот и шкуры; хлеб они выменивают,
//!   а в голодный год берут набегом.
//! * Морская торговля по Понту идёт с апреля по октябрь: зимой «море закрыто».
//! * Таможня — октава (1/8 стоимости товара), её брали в портах империи в VI веке.
//!
//! Всё в этом файле — чистая логика без Bevy: её легко проверять тестами и сохранять на диск.

use crate::items::Rng;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Время
// ---------------------------------------------------------------------------

/// Сколько секунд реального времени длится игровая неделя.
pub fn week_secs() -> f64 {
    std::env::var("MRPG_WEEK_SECS").ok().and_then(|s| s.parse::<f64>().ok()).filter(|s| *s >= 1.0).unwrap_or(240.0)
}

pub const WEEKS_PER_YEAR: u32 = 52;

/// Отсчёт начинается с 1 сентября 510 года: с сентября в империи начинался год (индикт).
pub const START_YEAR: u32 = 510;

const MONTHS: [&str; 12] = ["сентябрь", "октябрь", "ноябрь", "декабрь", "январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август"];
const MONTHS_GEN: [&str; 12] = ["сентября", "октября", "ноября", "декабря", "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа"];

/// Месяц недели: 0 — сентябрь, 11 — август.
pub fn month_of(week: u32) -> usize {
    let day = (week % WEEKS_PER_YEAR) as f32 * 7.0 + 3.0;
    ((day / (365.0 / 12.0)) as usize).min(11)
}

/// Год от Рождества Христова (новый год — с января).
pub fn year_of(week: u32) -> u32 {
    let years = week / WEEKS_PER_YEAR;
    START_YEAR + years + if month_of(week) >= 4 { 1 } else { 0 }
}

/// «вторая неделя октября 510 г.»
pub fn date_text(week: u32) -> String {
    let m = month_of(week);
    let day = (week % WEEKS_PER_YEAR) as f32 * 7.0;
    let in_month = day - m as f32 * (365.0 / 12.0);
    let n = ((in_month / 7.0) as usize).min(3);
    let ord = ["первая", "вторая", "третья", "последняя"][n];
    format!("{ord} неделя {} {} г.", MONTHS_GEN[m], year_of(week))
}

pub fn month_name(week: u32) -> &'static str {
    MONTHS[month_of(week)]
}

/// Открыто ли море (с апреля по октябрь).
pub fn sea_open(week: u32) -> bool {
    !matches!(month_of(week), 2..=6)
}

/// Зима: стройки идут медленнее, кочевники голоднее.
pub fn winter(week: u32) -> bool {
    matches!(month_of(week), 3..=5)
}

// ---------------------------------------------------------------------------
// Товары
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Good {
    Grain,
    Fish,
    Salt,
    Wine,
    Oil,
    Cloth,
    Hides,
    Iron,
    Timber,
    Imports,
}

pub const GOODS: [Good; 10] = [Good::Grain, Good::Fish, Good::Salt, Good::Wine, Good::Oil, Good::Cloth, Good::Hides, Good::Iron, Good::Timber, Good::Imports];
pub const NG: usize = GOODS.len();

impl Good {
    pub fn name(self) -> &'static str {
        match self {
            Good::Grain => "Зерно",
            Good::Fish => "Солёная рыба",
            Good::Salt => "Соль",
            Good::Wine => "Вино",
            Good::Oil => "Масло",
            Good::Cloth => "Ткани",
            Good::Hides => "Скот и шкуры",
            Good::Iron => "Железо",
            Good::Timber => "Лес",
            Good::Imports => "Заморский товар",
        }
    }
    /// Во что обходится воз (груз) в Херсоне, солиды: для таможни и казны.
    pub fn value(self) -> f32 {
        match self {
            Good::Grain => 0.6,
            Good::Fish => 0.8,
            Good::Salt => 0.3,
            Good::Wine => 1.0,
            Good::Oil => 1.4,
            Good::Cloth => 2.0,
            Good::Hides => 0.9,
            Good::Iron => 1.6,
            Good::Timber => 0.4,
            Good::Imports => 4.0,
        }
    }
    /// Портится ли за неделю (доля).
    fn spoil(self) -> f32 {
        match self {
            Good::Grain => 0.004,
            Good::Fish => 0.006,
            Good::Wine | Good::Oil => 0.002,
            Good::Hides => 0.004,
            _ => 0.001,
        }
    }
    fn idx(self) -> usize {
        self as usize
    }
}

// ---------------------------------------------------------------------------
// Поселения
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TownId {
    Herson,
    GothVillage,
    Doros,
    AlanCamp,
    SteppeCamp,
    Bosporus,
    Sinope,
}

pub const TOWNS: [TownId; 7] = [TownId::Herson, TownId::GothVillage, TownId::Doros, TownId::AlanCamp, TownId::SteppeCamp, TownId::Bosporus, TownId::Sinope];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TownKind {
    City,
    Village,
    Camp,
    /// Далеко за морем: на карте нет, приходят корабли.
    Overseas,
}

pub struct TownDef {
    pub name: &'static str,
    pub about: &'static str,
    pub kind: TownKind,
    /// Место на карте (`level::places`), если поселение есть на уровне.
    pub place: Option<&'static str>,
    pub pop: f32,
    /// Сколько семей можно поселить (дворы, юрты, шатры).
    pub homes: f32,
    /// Сколько человек в среднем живёт в одном дворе.
    pub per_home: f32,
    /// Производство на 1000 жителей в неделю (в среднем за год), по `GOODS`.
    pub make: [f32; NG],
    /// Потребление на 1000 жителей в неделю.
    pub eat: [f32; NG],
    /// Стены (0..1): насколько трудно взять набегом.
    pub walls: f32,
}

//                       Зерно Рыба Соль Вино Масло Ткани Скот Желез Лес Замор
const EAT_CITY: [f32; NG] = [10.0, 3.0, 1.0, 3.0, 1.5, 1.0, 1.0, 0.5, 3.0, 0.3];
const EAT_VILLAGE: [f32; NG] = [10.0, 1.0, 1.0, 1.5, 0.3, 0.8, 0.5, 0.3, 3.0, 0.05];
const EAT_STEPPE: [f32; NG] = [5.0, 0.3, 1.0, 1.5, 0.1, 1.5, 2.0, 0.6, 0.6, 0.1];

pub const DEFS: [TownDef; 7] = [
    TownDef {
        name: "Херсон",
        about: "Форпост империи: порт, базилики, засолочные цистерны и хорреи. Своего хлеба мало — живёт морем и торговлей.",
        kind: TownKind::City,
        place: Some("agora"),
        pop: 9200.0,
        homes: 1330.0,
        per_home: 7.0,
        make: [3.0, 9.5, 4.0, 4.0, 0.0, 1.3, 0.4, 0.5, 0.2, 0.0],
        eat: EAT_CITY,
        walls: 0.9,
    },
    TownDef {
        name: "Готское село",
        about: "Селение федератов-готов в долине у Херсона: пашни, ток, загон, мазанки под соломой.",
        kind: TownKind::Village,
        place: Some("goth_village"),
        pop: 260.0,
        homes: 44.0,
        per_home: 6.0,
        make: [24.0, 0.0, 0.0, 3.0, 0.0, 1.2, 4.0, 0.2, 5.0, 0.0],
        eat: EAT_VILLAGE,
        walls: 0.1,
    },
    TownDef {
        name: "Дори",
        about: "Горная твердыня готов Таврики. Хлеб в долинах, лес на склонах, мёд и воск.",
        kind: TownKind::City,
        place: None,
        pop: 2600.0,
        homes: 450.0,
        per_home: 6.0,
        make: [16.0, 0.0, 0.0, 4.0, 0.0, 1.2, 3.0, 0.9, 10.0, 0.0],
        eat: EAT_VILLAGE,
        walls: 0.8,
    },
    TownDef {
        name: "Аланский стан",
        about: "Аланы предгорий: табуны, кузнецы, войлочные шатры. Хлеб выменивают у готов.",
        kind: TownKind::Camp,
        place: Some("alan_camp"),
        pop: 380.0,
        homes: 70.0,
        per_home: 6.0,
        make: [5.0, 0.0, 0.0, 0.0, 0.0, 0.6, 12.0, 1.4, 1.0, 0.0],
        eat: EAT_STEPPE,
        walls: 0.15,
    },
    TownDef {
        name: "Кочевье",
        about: "Гунно-булгарская орда у края степи: юрты, кибитки, табуны. Голодной зимой ходит в набеги.",
        kind: TownKind::Camp,
        place: Some("steppe_camp"),
        pop: 650.0,
        homes: 120.0,
        per_home: 6.0,
        make: [0.5, 0.0, 0.0, 0.0, 0.0, 0.4, 16.0, 0.3, 0.2, 0.0],
        eat: EAT_STEPPE,
        walls: 0.0,
    },
    TownDef {
        name: "Боспор",
        about: "Древний город у пролива: хлеб с полей Керченского полуострова, рыба, соль. Под рукой гуннов.",
        kind: TownKind::City,
        place: None,
        pop: 6000.0,
        homes: 900.0,
        per_home: 7.0,
        make: [15.0, 8.0, 3.5, 3.0, 0.0, 1.0, 1.5, 0.5, 0.4, 0.0],
        eat: EAT_CITY,
        walls: 0.7,
    },
    TownDef {
        name: "Синопа и Заморье",
        about: "Порты южного берега Понта и Константинополь: масло, вино, ткани, железо, пряности, книги.",
        kind: TownKind::Overseas,
        place: None,
        pop: 20000.0,
        homes: 2900.0,
        per_home: 7.0,
        make: [11.0, 3.0, 0.5, 6.0, 4.5, 3.0, 0.4, 1.5, 4.0, 2.0],
        eat: EAT_CITY,
        walls: 1.0,
    },
];

impl TownId {
    pub fn def(self) -> &'static TownDef {
        &DEFS[self as usize]
    }
    pub fn idx(self) -> usize {
        self as usize
    }
    /// Поселение на карте по ключу места торговца (`MerchantDef::place`).
    pub fn of_place(place: &str) -> TownId {
        match place {
            "goth_village" => TownId::GothVillage,
            "alan_camp" => TownId::AlanCamp,
            "steppe_camp" => TownId::SteppeCamp,
            _ => TownId::Herson,
        }
    }
}

// ---------------------------------------------------------------------------
// Дороги и море
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    Road,
    Steppe,
    Sea,
}

pub struct Route {
    pub a: TownId,
    pub b: TownId,
    pub kind: RouteKind,
    /// Сколько грузов за неделю (возов или корабельных грузов).
    pub cap: f32,
    /// Опасность пути за неделю: разбойники, шторм.
    pub danger: f32,
    pub name: &'static str,
}

pub const ROUTES: [Route; 11] = [
    Route { a: TownId::Herson, b: TownId::GothVillage, kind: RouteKind::Road, cap: 12.0, danger: 0.01, name: "дорога на готское село" },
    Route { a: TownId::Herson, b: TownId::Doros, kind: RouteKind::Road, cap: 22.0, danger: 0.03, name: "горная дорога в Дори" },
    Route { a: TownId::Herson, b: TownId::AlanCamp, kind: RouteKind::Road, cap: 10.0, danger: 0.04, name: "тропа к аланам" },
    Route { a: TownId::Herson, b: TownId::SteppeCamp, kind: RouteKind::Steppe, cap: 12.0, danger: 0.06, name: "степной шлях" },
    Route { a: TownId::Herson, b: TownId::Bosporus, kind: RouteKind::Sea, cap: 55.0, danger: 0.04, name: "морем на Боспор" },
    Route { a: TownId::Herson, b: TownId::Sinope, kind: RouteKind::Sea, cap: 90.0, danger: 0.05, name: "через Понт на Синопу" },
    Route { a: TownId::GothVillage, b: TownId::Doros, kind: RouteKind::Road, cap: 8.0, danger: 0.01, name: "долиной в Дори" },
    Route { a: TownId::Doros, b: TownId::AlanCamp, kind: RouteKind::Road, cap: 8.0, danger: 0.03, name: "предгорьями к аланам" },
    Route { a: TownId::AlanCamp, b: TownId::SteppeCamp, kind: RouteKind::Steppe, cap: 8.0, danger: 0.03, name: "степью к кочевью" },
    Route { a: TownId::SteppeCamp, b: TownId::Bosporus, kind: RouteKind::Steppe, cap: 14.0, danger: 0.04, name: "степью на Боспор" },
    Route { a: TownId::Bosporus, b: TownId::Sinope, kind: RouteKind::Sea, cap: 30.0, danger: 0.05, name: "морем с Боспора" },
];

// ---------------------------------------------------------------------------
// Состояние поселения
// ---------------------------------------------------------------------------

/// Что строят.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    /// Новые дворы. В Херсоне — на месте сгоревшей усадьбы (`site`), это видно на карте.
    Houses,
    /// Починка стен и ворот после набега.
    Walls,
    /// Мастерские (больше своих тканей и железа).
    Workshops,
}

impl ProjectKind {
    pub fn name(self) -> &'static str {
        match self {
            ProjectKind::Houses => "новые дома",
            ProjectKind::Walls => "починка стен",
            ProjectKind::Workshops => "мастерские",
        }
    }
    /// Полная цена, солиды (каменщики, плотники, известь, черепица).
    fn cost(self, kind: TownKind) -> f32 {
        let k = if kind == TownKind::City { 1.0 } else { 0.35 };
        k * match self {
            ProjectKind::Houses => 14.0,
            ProjectKind::Walls => 30.0,
            ProjectKind::Workshops => 18.0,
        }
    }
    /// Сколько недель строят при хорошей погоде и полном снабжении.
    fn weeks(self) -> f32 {
        match self {
            ProjectKind::Houses => 10.0,
            ProjectKind::Walls => 12.0,
            ProjectKind::Workshops => 8.0,
        }
    }
    /// Лес на всю стройку (грузы).
    fn timber(self) -> f32 {
        match self {
            ProjectKind::Houses => 8.0,
            ProjectKind::Walls => 4.0,
            ProjectKind::Workshops => 6.0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Project {
    pub kind: ProjectKind,
    pub progress: f32,
    /// Номер стройплощадки в Херсоне (видна на карте).
    pub site: Option<u16>,
    /// Сколько ещё недель простоя (нет леса, денег, мор).
    #[serde(default)]
    pub stalled: u8,
}

/// Временные беды и радости.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Неурожай этого года: зерно родится хуже (множитель).
    BadHarvest(f32),
    /// Мор: здоровье падает, люди умирают, торговля с городом замирает.
    Plague,
    /// После набега: дороги опасны, люди боятся.
    Raided,
    /// Праздник или ярмарка.
    Feast,
    /// Падёж скота.
    Murrain,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ActiveEffect {
    pub effect: Effect,
    pub until: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TownState {
    pub id: TownId,
    pub pop: f32,
    pub homes: f32,
    pub stock: [f32; NG],
    /// Казна (солиды).
    pub treasury: f32,
    /// Настроение 0..1.
    pub mood: f32,
    /// Здоровье 0..1 (мор).
    pub health: f32,
    /// Стены и стража 0..1.
    pub defense: f32,
    /// Мастерские: множитель тканей и железа.
    pub crafts: f32,
    pub projects: Vec<Project>,
    pub effects: Vec<ActiveEffect>,
    /// Голод прошлой недели (0 — сыты, 1 — есть нечего).
    pub hunger: f32,
    /// Рождено, умерло, пришло и ушло за последний год (для вестей).
    #[serde(default)]
    pub year_births: f32,
    #[serde(default)]
    pub year_deaths: f32,
    #[serde(default)]
    pub built_homes: u32,
}

impl TownState {
    pub fn new(id: TownId) -> Self {
        let d = id.def();
        let mut stock = [0.0; NG];
        for g in GOODS {
            // Запасов — примерно на два-три месяца; хлеба — только что с тока (отсчёт с сентября).
            stock[g.idx()] = target(d, d.pop, g) * if g == Good::Grain { 1.3 } else { 0.9 };
            if g == Good::Grain && d.kind != TownKind::Overseas {
                // Сентябрь: хорреи и ямы полны до будущего лета.
                stock[g.idx()] = stock[g.idx()].max(d.eat[g.idx()] * d.pop / 1000.0 * 50.0);
            }
        }
        TownState {
            id,
            pop: d.pop,
            homes: d.homes,
            stock,
            treasury: if d.kind == TownKind::City { 60.0 } else { 8.0 },
            mood: 0.62,
            health: 1.0,
            defense: d.walls,
            crafts: 1.0,
            projects: Vec::new(),
            effects: Vec::new(),
            hunger: 0.0,
            year_births: 0.0,
            year_deaths: 0.0,
            built_homes: 0,
        }
    }

    pub fn has(&self, e: fn(&Effect) -> bool) -> bool {
        self.effects.iter().any(|a| e(&a.effect))
    }

    pub fn plague(&self) -> bool {
        self.has(|e| matches!(e, Effect::Plague))
    }

    pub fn raided(&self) -> bool {
        self.has(|e| matches!(e, Effect::Raided))
    }

    fn harvest_mult(&self) -> f32 {
        self.effects.iter().filter_map(|a| if let Effect::BadHarvest(m) = a.effect { Some(m) } else { None }).fold(1.0, f32::min)
    }

    /// Цена товара относительно обычной (1 — как всегда, 2 — вдвое дороже).
    pub fn price(&self, g: Good) -> f32 {
        let d = self.id.def();
        let t = target(d, self.pop, g);
        ((t + 1.0) / (self.stock[g.idx()] + 1.0)).sqrt().clamp(0.45, 3.0)
    }

    /// Сколько народу помещается.
    pub fn capacity(&self) -> f32 {
        self.homes * self.id.def().per_home
    }

    /// Теснота: >1 — живут друг у друга на головах.
    pub fn crowding(&self) -> f32 {
        self.pop / self.capacity().max(1.0)
    }
}

/// Обычный запас товара (на 12 недель своего потребления, у производителей — на 4 недели выработки).
fn target(d: &TownDef, pop: f32, g: Good) -> f32 {
    let k = pop / 1000.0;
    // Хлеб и вино родятся раз в год — их держат в хорреях и пифосах на полгода вперёд.
    let weeks = match g {
        // Город живёт подвозом и держит хлеба месяца на четыре.
        Good::Grain if d.kind == TownKind::City => 16.0,
        Good::Grain | Good::Wine => 26.0,
        _ => 12.0,
    };
    (d.eat[g.idx()] * k * weeks).max(d.make[g.idx()] * k * 4.0).max(1.0)
}

/// Сколько недель до следующей жатвы (июль).
fn weeks_to_harvest(week: u32) -> f32 {
    let w = week % WEEKS_PER_YEAR;
    let h = month_start(10);
    ((h + WEEKS_PER_YEAR - w) % WEEKS_PER_YEAR) as f32
}

// ---------------------------------------------------------------------------
// Вести
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewsKind {
    Trade,
    Ship,
    Raid,
    Plague,
    Harvest,
    Fire,
    Feast,
    Build,
    People,
    Treasury,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct News {
    pub week: u32,
    pub town: TownId,
    pub kind: NewsKind,
    pub text: String,
    /// Важная весть — её выкрикивает глашатай всем игрокам.
    pub loud: bool,
}

/// Что из случившегося за неделю должен сделать сервер (то, что видно в игре).
#[derive(Debug, Clone, PartialEq)]
pub enum WorldCue {
    /// Набег на поселение на карте: поставить налётчиков.
    Raid { town: TownId, raiders: u32 },
    /// Стройплощадка Херсона перешла на новую стадию.
    SiteStage { site: u16, stage: u8 },
    /// Новый дом в Херсоне достроен: выставить на продажу.
    HouseBuilt { site: u16 },
    /// Мор в Херсоне начался или кончился.
    Plague { town: TownId, on: bool },
}

// ---------------------------------------------------------------------------
// Мир
// ---------------------------------------------------------------------------

/// Стадии стройки на площадке: 0 — пепелище, 1 — фундамент, 2 — стены, 3 — кровля, 4 — готово.
pub const SITE_DONE: u8 = 4;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Towns {
    pub week: u32,
    pub towns: Vec<TownState>,
    /// Стадия каждой стройплощадки Херсона.
    pub sites: Vec<u8>,
    pub news: Vec<News>,
    /// Сколько грузов прошло по каждому пути за последнюю неделю (для вестей).
    #[serde(default)]
    pub flows: Vec<f32>,
    #[serde(skip, default = "default_rng")]
    rng: Rng,
}

fn default_rng() -> Rng {
    Rng::from_time()
}

const NEWS_KEEP: usize = 60;

impl Towns {
    pub fn new(sites: usize, seed: u64) -> Self {
        Towns { week: 0, towns: TOWNS.iter().map(|t| TownState::new(*t)).collect(), sites: vec![0; sites], news: Vec::new(), flows: vec![0.0; ROUTES.len()], rng: Rng::new(seed) }
    }

    /// После загрузки с диска: число стройплощадок могло поменяться вместе с картой.
    pub fn fit_sites(&mut self, sites: usize) {
        self.sites.resize(sites, 0);
        for t in &mut self.towns {
            t.projects.retain(|p| p.site.is_none_or(|s| (s as usize) < sites));
        }
        self.flows.resize(ROUTES.len(), 0.0);
        if self.towns.len() != TOWNS.len() {
            let old = std::mem::take(&mut self.towns);
            self.towns = TOWNS.iter().map(|id| old.iter().find(|t| t.id == *id).cloned().unwrap_or_else(|| TownState::new(*id))).collect();
        }
    }

    pub fn reseed(&mut self, seed: u64) {
        self.rng = Rng::new(seed);
    }

    pub fn town(&self, id: TownId) -> &TownState {
        &self.towns[id.idx()]
    }

    pub fn town_mut(&mut self, id: TownId) -> &mut TownState {
        &mut self.towns[id.idx()]
    }

    fn say(&mut self, town: TownId, kind: NewsKind, loud: bool, text: impl Into<String>) {
        self.news.push(News { week: self.week, town, kind, text: text.into(), loud });
        if self.news.len() > NEWS_KEEP {
            let extra = self.news.len() - NEWS_KEEP;
            self.news.drain(..extra);
        }
    }

    /// Прожить одну неделю. Возвращает, что надо показать в мире.
    pub fn tick_week(&mut self) -> Vec<WorldCue> {
        let mut cues = Vec::new();
        let week = self.week;
        self.calendar_events(&mut cues);
        for i in 0..self.towns.len() {
            self.produce(i);
        }
        self.trade();
        for i in 0..self.towns.len() {
            self.consume_and_live(i);
        }
        self.random_events(&mut cues);
        for i in 0..self.towns.len() {
            self.build(i, &mut cues);
        }
        // Кончившиеся беды.
        for i in 0..self.towns.len() {
            let id = self.towns[i].id;
            let ended: Vec<Effect> = self.towns[i].effects.iter().filter(|a| a.until <= week).map(|a| a.effect).collect();
            self.towns[i].effects.retain(|a| a.until > week);
            for e in ended {
                match e {
                    Effect::Plague => {
                        self.say(id, NewsKind::Plague, true, format!("{}: мор отступил. Живые служат благодарственный молебен.", id.def().name));
                        cues.push(WorldCue::Plague { town: id, on: false });
                    }
                    Effect::Raided => {}
                    _ => {}
                }
            }
        }
        if (week + 1).is_multiple_of(WEEKS_PER_YEAR) {
            for t in &mut self.towns {
                t.year_births = 0.0;
                t.year_deaths = 0.0;
            }
        }
        self.week += 1;
        cues
    }

    fn produce(&mut self, i: usize) {
        let week = self.week;
        let m = month_of(week);
        let t = &mut self.towns[i];
        let d = t.id.def();
        let k = t.pop / 1000.0;
        let work = (0.35 + 0.65 * t.health) * (0.7 + 0.3 * t.mood) * if t.raided() { 0.6 } else { 1.0 };
        let harvest = t.harvest_mult();
        let murrain = if t.has(|e| matches!(e, Effect::Murrain)) { 0.4 } else { 1.0 };
        for g in GOODS {
            // Урожай зерна — июль-август, вина — сентябрь-октябрь; рыба — больше осенью.
            let season = match g {
                Good::Grain if d.kind != TownKind::Overseas => {
                    if matches!(m, 10 | 11) {
                        WEEKS_PER_YEAR as f32 / 9.0
                    } else {
                        0.0
                    }
                }
                Good::Wine if d.kind != TownKind::Overseas => {
                    if matches!(m, 0 | 1) {
                        WEEKS_PER_YEAR as f32 / 9.0
                    } else {
                        0.0
                    }
                }
                Good::Fish => {
                    if matches!(m, 1..=3) {
                        1.6
                    } else if matches!(m, 5 | 6) {
                        0.5
                    } else {
                        1.0
                    }
                }
                _ => 1.0,
            };
            let mut amount = d.make[g.idx()] * k * season * work;
            match g {
                Good::Grain => amount *= harvest,
                Good::Hides => amount *= murrain,
                Good::Cloth | Good::Iron => amount *= t.crafts,
                _ => {}
            }
            let s = &mut t.stock[g.idx()];
            *s = (*s + amount) * (1.0 - g.spoil());
        }
    }

    /// Обозы и корабли: товар идёт туда, где дороже.
    fn trade(&mut self) {
        let week = self.week;
        let sea = sea_open(week);
        let mut lines: Vec<(TownId, NewsKind, String)> = Vec::new();
        for (ri, r) in ROUTES.iter().enumerate() {
            self.flows[ri] = 0.0;
            if r.kind == RouteKind::Sea && !sea {
                continue;
            }
            let (ia, ib) = (r.a.idx(), r.b.idx());
            // Мор: в зачумлённый город не ездят, и оттуда не выпускают.
            let plague = self.towns[ia].plague() || self.towns[ib].plague();
            let raided = self.towns[ia].raided() || self.towns[ib].raided();
            let mut cap = r.cap * if plague { 0.25 } else { 1.0 } * if raided { 0.5 } else { 1.0 };
            // Шторм или разбойники.
            let danger = r.danger * if raided { 2.5 } else { 1.0 } * if winter(week) && r.kind == RouteKind::Steppe { 1.5 } else { 1.0 };
            let lost = self.rng.f32() < danger;
            // Выгоднее всего — где разница в ценах больше.
            let mut wants: Vec<(f32, Good, bool)> = GOODS
                .iter()
                .map(|&g| {
                    let pa = self.towns[ia].price(g);
                    let pb = self.towns[ib].price(g);
                    if pb > pa { (pb / pa, g, true) } else { (pa / pb, g, false) }
                })
                .filter(|(gap, _, _)| *gap > 1.12)
                .collect();
            wants.sort_by(|a, b| b.0.total_cmp(&a.0));
            let mut cargo: Vec<(Good, f32)> = Vec::new();
            let mut forward = 0.0;
            for (gap, g, a_to_b) in wants {
                if cap < 0.05 {
                    break;
                }
                let (from, to) = if a_to_b { (ia, ib) } else { (ib, ia) };
                let fd = self.towns[from].id.def();
                // Хлеб себе оставляют до новой жатвы.
                let farmers = fd.make[Good::Grain.idx()] >= fd.eat[Good::Grain.idx()] * 0.5;
                let keep = if g == Good::Grain && farmers && fd.kind != TownKind::Overseas { weeks_to_harvest(week).max(4.0) + 2.0 } else { 4.0 };
                let reserve = fd.eat[g.idx()] * self.towns[from].pop / 1000.0 * keep;
                let spare = (self.towns[from].stock[g.idx()] - reserve).max(0.0);
                let q = (cap * ((gap - 1.0) / 1.5).min(1.0)).min(spare);
                if q < 0.05 {
                    continue;
                }
                cap -= q;
                self.towns[from].stock[g.idx()] -= q;
                if !lost {
                    self.towns[to].stock[g.idx()] += q;
                    // Таможня Херсона — октава со всего, что ввезли и вывезли.
                    if self.towns[to].id == TownId::Herson || self.towns[from].id == TownId::Herson {
                        self.towns[TownId::Herson.idx()].treasury += q * g.value() / 8.0;
                    }
                    // Купцы с обеих сторон наживаются.
                    self.towns[from].treasury += q * g.value() * 0.03;
                }
                if a_to_b {
                    forward += q;
                }
                cargo.push((g, q));
            }
            let total: f32 = cargo.iter().map(|c| c.1).sum();
            self.flows[ri] = if lost { 0.0 } else { total };
            if total < 1.0 {
                continue;
            }
            cargo.sort_by(|a, b| b.1.total_cmp(&a.1));
            let names: Vec<String> = cargo.iter().take(3).map(|(g, _)| g.name().to_lowercase()).collect();
            let goods = names.join(", ");
            let herson_side = r.a == TownId::Herson || r.b == TownId::Herson;
            if lost {
                let text = match r.kind {
                    RouteKind::Sea => format!("Шторм на Понте: корабль ({}) не дошёл. Товар пропал: {goods}.", r.name),
                    _ => format!("Разбойники разграбили обоз ({}). Пропало: {goods}.", r.name),
                };
                lines.push((if herson_side { TownId::Herson } else { r.a }, NewsKind::Trade, text));
            } else if herson_side && total > 6.0 && self.rng.f32() < 0.35 {
                let other = if r.a == TownId::Herson { r.b } else { r.a };
                let inbound = (r.a == TownId::Herson) != (forward > total / 2.0);
                let text = match (r.kind, inbound) {
                    (RouteKind::Sea, true) => format!("В Карантинную бухту вошёл корабль из {}: {goods}.", from_name(other)),
                    (RouteKind::Sea, false) => format!("Корабль ушёл в {}: везёт {goods}.", to_name(other)),
                    (_, true) => format!("Пришёл обоз из {}: {goods}.", from_name(other)),
                    (_, false) => format!("Обоз ушёл в {}: везёт {goods}.", to_name(other)),
                };
                lines.push((TownId::Herson, if r.kind == RouteKind::Sea { NewsKind::Ship } else { NewsKind::Trade }, text));
            }
        }
        for (town, kind, text) in lines {
            self.say(town, kind, false, text);
        }
    }

    fn consume_and_live(&mut self, i: usize) {
        let id = self.towns[i].id;
        let d = id.def();
        let feast = self.towns[i].has(|e| matches!(e, Effect::Feast));
        let plague = self.towns[i].plague();
        let t = &mut self.towns[i];
        let k = t.pop / 1000.0;
        let cold = if winter(self.week) { 1.3 } else { 1.0 };
        // Еда: хлеб, рыба подменяет часть хлеба.
        let mut short = [0.0f32; NG];
        for g in GOODS {
            let need = d.eat[g.idx()] * k * if g == Good::Timber { cold } else { 1.0 };
            let have = t.stock[g.idx()];
            let take = need.min(have);
            t.stock[g.idx()] -= take;
            short[g.idx()] = if need > 0.0 { 1.0 - take / need } else { 0.0 };
        }
        let grain_need = d.eat[Good::Grain.idx()].max(0.1);
        let fish_need = d.eat[Good::Fish.idx()];
        let hunger = ((short[Good::Grain.idx()] * grain_need + short[Good::Fish.idx()] * fish_need * 0.5) / (grain_need + fish_need * 0.5)).clamp(0.0, 1.0);
        // Если рыба в избытке, она кормит вместо хлеба.
        let hunger = if hunger > 0.0 && t.stock[Good::Fish.idx()] > 1.0 {
            let fill = (hunger * grain_need * k).min(t.stock[Good::Fish.idx()] * 0.5);
            t.stock[Good::Fish.idx()] -= fill;
            (hunger - fill / (grain_need * k).max(0.01)).max(0.0)
        } else {
            hunger
        };
        t.hunger = hunger;
        let comfort = 1.0 - (short[Good::Wine.idx()] + short[Good::Oil.idx()] + short[Good::Cloth.idx()] + short[Good::Timber.idx()]) / 4.0;

        // Люди: рождаются, умирают, приходят и уходят.
        let fertility = if d.kind == TownKind::City || d.kind == TownKind::Overseas { 0.038 } else { 0.046 };
        let births = t.pop * fertility / 52.0 * (1.0 - hunger * 0.7);
        let deaths = t.pop * (0.031 / 52.0 + hunger * 0.012 + (1.0 - t.health) * 0.018);
        let crowd = t.crowding();
        let room = (t.capacity() - t.pop).max(0.0);
        let mut migrate = 0.0;
        if d.kind != TownKind::Overseas {
            // В города тянутся из сёл и из-за моря — если есть где жить и что есть.
            if d.kind == TownKind::City && t.mood > 0.55 && hunger < 0.1 && !plague {
                migrate += room.min(t.pop * 0.0006 * (t.mood - 0.5) * 4.0);
            }
            if hunger > 0.15 {
                migrate -= t.pop * hunger * 0.01;
            }
            if crowd > 1.08 {
                migrate -= t.pop * (crowd - 1.08) * 0.05;
            }
        }
        t.pop = (t.pop + births - deaths + migrate).max(20.0);
        t.year_births += births;
        t.year_deaths += deaths;

        // Настроение.
        let mut target = 0.45 + 0.25 * comfort - 0.6 * hunger - 0.4 * (1.0 - t.health) - 0.2 * (crowd - 1.0).max(0.0);
        if feast {
            target += 0.2;
        }
        if t.raided() {
            target -= 0.15;
        }
        target += (t.treasury / 200.0).clamp(-0.05, 0.08);
        t.mood += (target.clamp(0.05, 0.95) - t.mood) * 0.25;

        // Здоровье: мор валит людей, потом город медленно оправляется.
        if plague {
            t.health = (t.health - 0.05).max(0.55);
        } else {
            t.health = (t.health + 0.04).min(1.0);
        }
        // Казна: подати и расходы города.
        let taxes = k * 1.0 * (0.6 + 0.4 * t.mood) * (1.0 - hunger * 0.5);
        let upkeep = k * 0.75;
        // Лишнее курия тратит на мостовые, водопровод, раздачи хлеба и храмы.
        let works = (t.treasury - 120.0).max(0.0) * 0.05;
        if works > 0.5 {
            t.mood = (t.mood + 0.005).min(1.0);
        }
        t.treasury = (t.treasury + taxes - upkeep - works).max(-50.0);
        // Стены понемногу ветшают.
        t.defense = (t.defense - 0.001).max(d.walls * 0.3);
    }

    fn calendar_events(&mut self, cues: &mut Vec<WorldCue>) {
        let week = self.week;
        let w = week % WEEKS_PER_YEAR;
        // Праздники Херсона (неделя от 1 сентября): св. Климент Римский, сосланный в Херсон, —
        // конец ноября; Рождество, Богоявление, Пасха (в среднем — середина апреля).
        let feasts: [(u32, &str); 4] = [
            (12, "Память святого Климента, епископа Римского, сосланного в Херсон: крестный ход к его гробнице, раздача хлеба"),
            (16, "Рождество Христово: литургия в базилике, город гуляет"),
            (18, "Богоявление: водосвятие в бухте, ярмарка у ворот"),
            (32, "Пасха: по всему городу бьют в деревянные била, крестный ход, разговляются всем городом"),
        ];
        for (fw, text) in feasts {
            if w == fw {
                let until = week + 1;
                for t in self.towns.iter_mut().filter(|t| matches!(t.id, TownId::Herson | TownId::GothVillage | TownId::Doros)) {
                    t.effects.push(ActiveEffect { effect: Effect::Feast, until });
                }
                self.say(TownId::Herson, NewsKind::Feast, true, text);
            }
        }
        // Весной из столицы приходит жалованье гарнизону (руга) — на него оживает торговля.
        if w == 36 {
            let h = self.town_mut(TownId::Herson);
            h.treasury += 45.0;
            self.say(TownId::Herson, NewsKind::Treasury, true, "Из Константинополя пришёл корабль с жалованьем гарнизону. В лавках оживление.");
        }
        // Открытие и закрытие навигации.
        if w == month_start(7) {
            self.say(TownId::Herson, NewsKind::Ship, true, "Море открылось: навикулярии снаряжают корабли за Понт.");
        }
        if w == month_start(2) {
            self.say(TownId::Herson, NewsKind::Ship, false, "Море закрылось до весны: зимой корабли не ходят. Цены на привозное поползут вверх.");
        }
        // Перед жатвой решается, каким будет урожай.
        if w == month_start(9) {
            for i in 0..self.towns.len() {
                let id = self.towns[i].id;
                let d = id.def();
                if d.make[Good::Grain.idx()] < 5.0 || d.kind == TownKind::Overseas {
                    continue;
                }
                let roll = self.rng.f32();
                if roll < 0.14 {
                    let locusts = self.rng.f32() < 0.3;
                    let m = self.rng.range_f32(0.3, 0.6);
                    self.towns[i].effects.push(ActiveEffect { effect: Effect::BadHarvest(m), until: week + 14 });
                    let why = if locusts { "саранча поела хлеба" } else { "засуха спалила колос" };
                    self.say(id, NewsKind::Harvest, true, format!("{}: {why} — урожай будет вполовину. Хлеб подорожает.", d.name));
                } else if roll > 0.85 {
                    self.towns[i].effects.push(ActiveEffect { effect: Effect::BadHarvest(1.3), until: week + 14 });
                    self.say(id, NewsKind::Harvest, false, format!("{}: хлеба стоят стеной — урожай обильный.", d.name));
                }
            }
        }
        let _ = cues;
    }

    fn random_events(&mut self, cues: &mut Vec<WorldCue>) {
        let week = self.week;
        // Набеги: голодная степь (зимой — особенно) идёт за хлебом и скотом.
        let steppe = self.town(TownId::SteppeCamp).clone();
        let alans_hungry = self.town(TownId::AlanCamp).hunger;
        let raid_chance = 0.012 + steppe.hunger * 0.08 + if winter(week) { 0.015 } else { 0.0 } + if steppe.has(|e| matches!(e, Effect::Murrain)) { 0.03 } else { 0.0 } + alans_hungry * 0.02;
        if self.rng.f32() < raid_chance {
            // Цели: селения без стен — чаще, Херсон — только его клеры за стенами.
            let targets = [(TownId::GothVillage, 3.0), (TownId::AlanCamp, 1.0), (TownId::Herson, 1.2), (TownId::Doros, 0.6), (TownId::Bosporus, 0.4)];
            if let Some(ti) = self.rng.weighted(&targets) {
                let target = targets[ti].0;
                self.raid(TownId::SteppeCamp, target, cues);
            }
        }
        for i in 0..self.towns.len() {
            let id = self.towns[i].id;
            let d = id.def();
            // Мор: чаще в голод и в портах.
            let t = &self.towns[i];
            if !t.plague() {
                let port = matches!(id, TownId::Herson | TownId::Bosporus | TownId::Sinope);
                let p = 0.0015 + t.hunger * 0.01 + t.crowding().max(1.0) * 0.0 + if port && sea_open(week) { 0.0015 } else { 0.0 } + (t.crowding() - 1.0).max(0.0) * 0.02;
                if self.rng.f32() < p {
                    let weeks = self.rng.range_u32(4, 9);
                    self.towns[i].effects.push(ActiveEffect { effect: Effect::Plague, until: week + weeks });
                    let how = if port { "занесли с корабля" } else { "пришла из степи" };
                    self.say(id, NewsKind::Plague, true, format!("{}: мор — горячка, {how}. Ксенон полон, травы и снадобья в цене.", d.name));
                    cues.push(WorldCue::Plague { town: id, on: true });
                }
            }
            // Пожар.
            let fire = match d.kind {
                TownKind::City => 0.008,
                TownKind::Village | TownKind::Camp => 0.006,
                TownKind::Overseas => 0.0,
            } * if winter(week) { 1.6 } else { 1.0 };
            if self.rng.f32() < fire {
                let lost = (self.towns[i].homes * self.rng.range_f32(0.004, 0.02)).max(1.0).round();
                let t = &mut self.towns[i];
                t.homes = (t.homes - lost).max(1.0);
                t.stock[Good::Timber.idx()] *= 0.8;
                t.stock[Good::Cloth.idx()] *= 0.9;
                t.mood = (t.mood - 0.05).max(0.0);
                let what = if d.kind == TownKind::City { "в ремесленном квартале" } else { "в селении" };
                let n = lost as u32;
                self.say(id, NewsKind::Fire, d.kind == TownKind::City, format!("{}: пожар {what}, сгорело {n} {}. Погорельцы ищут, где жить.", d.name, crate::economy::plural(n, "двор", "двора", "дворов")));
            }
            // Падёж скота у скотоводов.
            if matches!(id, TownId::SteppeCamp | TownId::AlanCamp) && self.rng.f32() < 0.006 && !self.towns[i].has(|e| matches!(e, Effect::Murrain)) {
                self.towns[i].effects.push(ActiveEffect { effect: Effect::Murrain, until: week + 8 });
                self.say(id, NewsKind::Harvest, false, format!("{}: падёж скота. Шкуры подорожают, а степь станет голодной.", d.name));
            }
        }
        // Ярмарка у готов после жатвы.
        if week % WEEKS_PER_YEAR == month_start(1) {
            let until = week + 1;
            self.town_mut(TownId::GothVillage).effects.push(ActiveEffect { effect: Effect::Feast, until });
            self.say(TownId::GothVillage, NewsKind::Feast, true, "Осенняя ярмарка в готском селе: хлеб нового урожая, мёд, воск, скот.");
        }
    }

    /// Набег `from` на `target`.
    fn raid(&mut self, from: TownId, target: TownId, cues: &mut Vec<WorldCue>) {
        let week = self.week;
        let defense = self.town(target).defense;
        let strength = self.rng.range_f32(0.4, 1.0) * (1.0 - defense * 0.8);
        let t = self.town_mut(target);
        let took_grain = t.stock[Good::Grain.idx()] * 0.35 * strength;
        let took_hides = t.stock[Good::Hides.idx()] * 0.45 * strength;
        t.stock[Good::Grain.idx()] -= took_grain;
        t.stock[Good::Hides.idx()] -= took_hides;
        // В Херсоне за стены не войти: горят клеры и усадьбы на склонах.
        let city = target == TownId::Herson;
        let captives = (t.pop * if city { 0.002 } else { 0.015 } * strength).round();
        let burned = (t.homes * if city { 0.0 } else { 0.05 } * strength).round();
        t.pop = (t.pop - captives).max(20.0);
        t.homes = (t.homes - burned).max(1.0);
        t.mood = (t.mood - 0.12).max(0.0);
        t.defense = (t.defense - 0.15 * strength).max(0.0);
        t.effects.push(ActiveEffect { effect: Effect::Raided, until: week + 3 });
        let r = self.town_mut(from);
        r.stock[Good::Grain.idx()] += took_grain;
        r.stock[Good::Hides.idx()] += took_hides;
        let text = if city {
            format!("Кочевники налетели на клеры под Херсоном: угнали скот, увели {} {}. Гарнизон закрыл ворота.", captives as u32, crate::economy::plural(captives as u32, "человека", "человек", "человек"))
        } else {
            format!("Набег кочевников на {}: сожжено {} {}, уведено в полон {}. Хлеб и скот угнали в степь.", name_acc(target), burned as u32, crate::economy::plural(burned as u32, "двор", "двора", "дворов"), captives as u32)
        };
        self.say(target, NewsKind::Raid, true, text);
        if target.def().place.is_some() {
            let raiders = (2.0 + strength * 4.0).round() as u32;
            cues.push(WorldCue::Raid { town: target, raiders });
        }
    }

    /// Стройки: начать, вести, закончить.
    fn build(&mut self, i: usize, cues: &mut Vec<WorldCue>) {
        let week = self.week;
        let id = self.towns[i].id;
        let d = id.def();
        if d.kind == TownKind::Overseas {
            // За морем строятся сами: важна только вместимость.
            let t = &mut self.towns[i];
            if t.crowding() > 0.95 {
                t.homes += t.homes * 0.002;
            }
            return;
        }
        // Начать новую стройку.
        let (crowd, treasury, plague, defense, projects, raided) = {
            let t = &self.towns[i];
            (t.crowding(), t.treasury, t.plague(), t.defense, t.projects.clone(), t.raided())
        };
        if projects.len() < 2 && !plague && !raided {
            let mut want = None;
            if defense < d.walls * 0.75 && !projects.iter().any(|p| p.kind == ProjectKind::Walls) {
                want = Some(ProjectKind::Walls);
            } else if crowd > 0.9 && !projects.iter().any(|p| p.kind == ProjectKind::Houses) {
                want = Some(ProjectKind::Houses);
            } else if treasury > 120.0 && self.towns[i].crafts < 1.4 && !projects.iter().any(|p| p.kind == ProjectKind::Workshops) {
                want = Some(ProjectKind::Workshops);
            }
            // Сёла и станы строят сами, толокой: им нужен только лес.
            let city = d.kind == TownKind::City;
            if let Some(kind) = want
                && (!city || treasury > kind.cost(d.kind) * 0.3)
            {
                let site = if id == TownId::Herson && kind == ProjectKind::Houses { self.free_site() } else { None };
                self.towns[i].projects.push(Project { kind, progress: 0.0, site, stalled: 0 });
                let text = match (kind, site) {
                    (ProjectKind::Houses, Some(_)) => "Курия наняла каменщиков: на месте сгоревшей усадьбы разбирают завал и кладут новый дом.".to_string(),
                    (ProjectKind::Houses, None) => format!("{}: строят новые дворы — народу стало тесно.", d.name),
                    (ProjectKind::Walls, _) => format!("{}: чинят стены и ворота после набега.", d.name),
                    (ProjectKind::Workshops, _) => format!("{}: открывают новые мастерские — ткацкие и кузни.", d.name),
                };
                self.say(id, NewsKind::Build, site.is_some(), text);
                if let Some(s) = site {
                    self.sites[s as usize] = 1;
                    cues.push(WorldCue::SiteStage { site: s, stage: 1 });
                }
            }
        }
        // Вести стройки.
        let mut done = Vec::new();
        let n = self.towns[i].projects.len();
        for pi in 0..n {
            let (kind, site) = {
                let p = &self.towns[i].projects[pi];
                (p.kind, p.site)
            };
            let t = &mut self.towns[i];
            let step = 1.0 / kind.weeks();
            let timber_need = kind.timber() * step;
            let money_need = kind.cost(d.kind) * step;
            let season = if winter(week) { 0.5 } else { 1.0 };
            let workers = (0.4 + 0.6 * t.health) * season;
            let ok_timber = t.stock[Good::Timber.idx()] >= timber_need;
            let ok_money = d.kind != TownKind::City || t.treasury >= money_need * 0.5;
            if !ok_timber || !ok_money || t.plague() {
                t.projects[pi].stalled = t.projects[pi].stalled.saturating_add(1);
                continue;
            }
            t.stock[Good::Timber.idx()] -= timber_need * workers;
            if d.kind == TownKind::City {
                t.treasury -= money_need * workers;
            }
            let p = &mut t.projects[pi];
            p.stalled = 0;
            p.progress = (p.progress + step * workers).min(1.0);
            if let Some(s) = site {
                let stage = (1 + (p.progress * 3.0) as u8).min(3);
                let stage = if p.progress >= 1.0 { SITE_DONE } else { stage };
                if self.sites[s as usize] != stage {
                    self.sites[s as usize] = stage;
                    cues.push(WorldCue::SiteStage { site: s, stage });
                }
            }
            if self.towns[i].projects[pi].progress >= 1.0 {
                done.push(pi);
            }
        }
        for &pi in done.iter().rev() {
            let p = self.towns[i].projects.remove(pi);
            let t = &mut self.towns[i];
            match p.kind {
                ProjectKind::Houses => {
                    let add = match d.kind {
                        TownKind::City => 8.0,
                        _ => 3.0,
                    };
                    t.homes += add;
                    t.built_homes += add as u32;
                    t.mood = (t.mood + 0.03).min(1.0);
                    let text = if let Some(s) = p.site {
                        cues.push(WorldCue::HouseBuilt { site: s });
                        "Достроена новая усадьба на месте пепелища. Табуларий курии выставил её на продажу.".to_string()
                    } else {
                        format!("{}: достроили {} новых дворов.", d.name, add as u32)
                    };
                    self.say(id, NewsKind::Build, p.site.is_some(), text);
                }
                ProjectKind::Walls => {
                    t.defense = d.walls;
                    self.say(id, NewsKind::Build, false, format!("{}: стены и ворота починили.", d.name));
                }
                ProjectKind::Workshops => {
                    t.crafts += 0.15;
                    self.say(id, NewsKind::Build, false, format!("{}: новые мастерские заработали — больше своих тканей и железа.", d.name));
                }
            }
        }
        // Долго стоящая стройка: курия жалуется.
        let stalled = self.towns[i].projects.iter().any(|p| p.stalled == 6);
        if stalled {
            let why = if self.towns[i].stock[Good::Timber.idx()] < 1.0 { "нет леса" } else { "нет денег в казне" };
            self.say(id, NewsKind::Build, false, format!("{}: стройка встала — {why}.", d.name));
        }
    }

    fn free_site(&self) -> Option<u16> {
        let busy: Vec<u16> = self.town(TownId::Herson).projects.iter().filter_map(|p| p.site).collect();
        self.sites.iter().enumerate().find(|(i, s)| **s == 0 && !busy.contains(&(*i as u16))).map(|(i, _)| i as u16)
    }

    /// Главные вести (новые сверху).
    pub fn latest(&self, n: usize) -> Vec<&News> {
        self.news.iter().rev().take(n).collect()
    }
}

fn month_start(m: usize) -> u32 {
    ((m as f32 * 365.0 / 12.0) / 7.0).ceil() as u32
}

fn from_name(t: TownId) -> &'static str {
    match t {
        TownId::Herson => "Херсона",
        TownId::GothVillage => "готского села",
        TownId::Doros => "Дори",
        TownId::AlanCamp => "аланского стана",
        TownId::SteppeCamp => "кочевья",
        TownId::Bosporus => "Боспора",
        TownId::Sinope => "Синопы",
    }
}

fn to_name(t: TownId) -> &'static str {
    match t {
        TownId::Herson => "Херсон",
        TownId::GothVillage => "готское село",
        TownId::Doros => "Дори",
        TownId::AlanCamp => "аланский стан",
        TownId::SteppeCamp => "кочевье",
        TownId::Bosporus => "Боспор",
        TownId::Sinope => "Синопу",
    }
}

fn name_acc(t: TownId) -> &'static str {
    to_name(t)
}

// ---------------------------------------------------------------------------
// Рынок Херсона и торговцы
// ---------------------------------------------------------------------------

use crate::trade::Category;

/// Из каких товаров складывается цена вида вещей у торговцев.
fn category_goods(c: Category) -> &'static [(Good, f32)] {
    match c {
        Category::Food => &[(Good::Fish, 0.45), (Good::Grain, 0.35), (Good::Salt, 0.2)],
        Category::WineOil => &[(Good::Wine, 0.5), (Good::Oil, 0.5)],
        Category::Textile => &[(Good::Cloth, 1.0)],
        Category::Hide | Category::SteppeGood => &[(Good::Hides, 1.0)],
        Category::Weapon | Category::Armor => &[(Good::Iron, 0.6)],
        Category::Material => &[(Good::Timber, 0.5), (Good::Iron, 0.3)],
        Category::Import | Category::Dye | Category::Gem | Category::Book => &[(Good::Imports, 1.0)],
        Category::Vessel => &[(Good::Imports, 0.3)],
        _ => &[],
    }
}

/// Во сколько раз цены вида вещей в поселении отличаются от обычных (и у продавца, и у скупщика).
pub fn market_mult(t: &TownState, c: Category) -> f32 {
    let mut m = 1.0;
    for &(g, w) in category_goods(c) {
        m *= 1.0 + (t.price(g) - 1.0) * 0.6 * w;
    }
    // Мор: снадобья и травы нарасхват.
    if matches!(c, Category::Remedy | Category::Herb) {
        m *= 1.0 + (1.0 - t.health) * 1.6;
    }
    // Голод и страх: всё дорожает понемногу.
    m *= 1.0 + t.hunger * 0.25;
    m.clamp(0.6, 2.4)
}

/// Сколько товара привозят на полки (1 — как обычно).
pub fn supply_mult(t: &TownState, c: Category) -> f32 {
    let goods = category_goods(c);
    if goods.is_empty() {
        return 1.0;
    }
    let mut p = 0.0;
    let mut w = 0.0;
    for &(g, k) in goods {
        p += t.price(g) * k;
        w += k;
    }
    (w / p).clamp(0.25, 1.4)
}

// ---------------------------------------------------------------------------
// Что видно игроку
// ---------------------------------------------------------------------------

/// Сводка поселения для окна «Вести».
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TownView {
    pub id: TownId,
    pub pop: u32,
    pub homes: u32,
    pub mood: f32,
    pub health: f32,
    pub hunger: f32,
    pub defense: f32,
    pub treasury: f32,
    /// Цены товаров (1 — обычные).
    pub prices: [f32; NG],
    pub projects: Vec<(ProjectKind, f32)>,
    pub troubles: Vec<String>,
}

pub fn town_view(t: &TownState) -> TownView {
    let mut troubles = Vec::new();
    for a in &t.effects {
        let s = match a.effect {
            Effect::BadHarvest(m) if m < 1.0 => "неурожай",
            Effect::BadHarvest(_) => "богатый урожай",
            Effect::Plague => "мор",
            Effect::Raided => "после набега",
            Effect::Feast => "праздник",
            Effect::Murrain => "падёж скота",
        };
        if !troubles.contains(&s.to_string()) {
            troubles.push(s.to_string());
        }
    }
    if t.hunger > 0.2 {
        troubles.push("голод".into());
    } else if t.hunger > 0.05 {
        troubles.push("хлеба не хватает".into());
    }
    if t.crowding() > 1.02 {
        troubles.push("теснота".into());
    }
    let mut prices = [1.0; NG];
    for g in GOODS {
        prices[g.idx()] = t.price(g);
    }
    TownView {
        id: t.id,
        pop: t.pop.round() as u32,
        homes: t.homes.round() as u32,
        mood: t.mood,
        health: t.health,
        hunger: t.hunger,
        defense: t.defense,
        treasury: t.treasury,
        prices,
        projects: t.projects.iter().map(|p| (p.kind, p.progress)).collect(),
        troubles,
    }
}

pub fn mood_word(m: f32) -> &'static str {
    match m {
        m if m > 0.75 => "ликует",
        m if m > 0.6 => "доволен",
        m if m > 0.45 => "спокоен",
        m if m > 0.3 => "ропщет",
        _ => "в отчаянии",
    }
}

/// «дешевле на 20%», «как обычно», «дороже вдвое».
pub fn price_word(p: f32) -> String {
    if p >= 1.9 {
        format!("дороже в {:.1} раза", p).replace(".0 ", " ").replace('.', ",")
    } else if p > 1.08 {
        format!("дороже на {}%", ((p - 1.0) * 100.0).round() as i32)
    } else if p < 0.92 {
        format!("дешевле на {}%", ((1.0 - p) * 100.0).round() as i32)
    } else {
        "как обычно".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_starts_in_september_510() {
        assert_eq!(month_of(0), 0);
        assert_eq!(year_of(0), 510);
        assert!(date_text(0).contains("сентября 510"), "{}", date_text(0));
        // Через 18 недель — январь уже 511 года.
        assert_eq!(month_of(18), 4);
        assert_eq!(year_of(18), 511);
        assert!(sea_open(0));
        assert!(!sea_open(20), "в январе море закрыто");
        assert!(sea_open(35), "в мае море открыто");
    }

    #[test]
    fn world_survives_ten_years() {
        let mut w = Towns::new(4, 7);
        let mut raids = 0;
        let mut plagues = 0;
        let mut built = 0;
        for _ in 0..WEEKS_PER_YEAR * 10 {
            for c in w.tick_week() {
                match c {
                    WorldCue::Raid { .. } => raids += 1,
                    WorldCue::Plague { on: true, .. } => plagues += 1,
                    WorldCue::HouseBuilt { .. } => built += 1,
                    _ => {}
                }
            }
            for t in &w.towns {
                assert!(t.pop.is_finite() && t.stock.iter().all(|s| s.is_finite() && *s >= -0.001), "{:?}", t);
            }
        }
        for t in &w.towns {
            let d = t.id.def();
            assert!(t.pop > d.pop * 0.5 && t.pop < d.pop * 2.0, "{}: {} жителей (было {})", d.name, t.pop, d.pop);
        }
        let h = w.town(TownId::Herson);
        assert!(h.price(Good::Grain) < 2.5, "Херсон голодает: цена хлеба {}", h.price(Good::Grain));
        assert!(raids > 0, "за десять лет ни одного набега");
        assert!(plagues <= 8, "слишком много моров: {plagues}");
        assert!(built > 0, "Херсон не строится");
        assert!(w.news.len() > 10);
    }

    #[test]
    fn grain_flows_into_herson() {
        let mut w = Towns::new(0, 3);
        w.town_mut(TownId::Herson).stock[Good::Grain.idx()] = 1.0;
        let before = w.town(TownId::Herson).price(Good::Grain);
        // Летом (дороги и море открыты).
        w.week = 40;
        for _ in 0..4 {
            w.tick_week();
        }
        let after = w.town(TownId::Herson).price(Good::Grain);
        assert!(after < before, "хлеб не подешевел: {before} -> {after}");
    }

    #[test]
    fn sea_is_closed_in_winter() {
        let mut w = Towns::new(0, 5);
        w.week = 20;
        w.tick_week();
        let sea: f32 = ROUTES.iter().zip(&w.flows).filter(|(r, _)| r.kind == RouteKind::Sea).map(|(_, f)| *f).sum();
        assert_eq!(sea, 0.0);
    }

    #[test]
    fn scarcity_raises_market_prices() {
        let mut t = TownState::new(TownId::Herson);
        let normal = market_mult(&t, Category::WineOil);
        t.stock[Good::Wine.idx()] = 0.0;
        t.stock[Good::Oil.idx()] = 0.0;
        let scarce = market_mult(&t, Category::WineOil);
        assert!(scarce > normal * 1.3, "{normal} -> {scarce}");
        assert!(supply_mult(&t, Category::WineOil) < 0.7);
        t.effects.push(ActiveEffect { effect: Effect::Plague, until: 99 });
        t.health = 0.6;
        assert!(market_mult(&t, Category::Remedy) > 1.5);
    }

    #[test]
    fn herson_site_is_built_in_stages() {
        let mut w = Towns::new(2, 11);
        let h = w.town_mut(TownId::Herson);
        h.pop = h.capacity() * 0.97;
        h.treasury = 200.0;
        h.stock[Good::Timber.idx()] = 200.0;
        w.week = 40;
        let mut stages = Vec::new();
        for _ in 0..60 {
            let h = w.town_mut(TownId::Herson);
            h.stock[Good::Timber.idx()] = h.stock[Good::Timber.idx()].max(50.0);
            h.treasury = h.treasury.max(100.0);
            h.effects.retain(|e| !matches!(e.effect, Effect::Plague | Effect::Raided));
            h.health = 1.0;
            for c in w.tick_week() {
                if let WorldCue::SiteStage { site: 0, stage } = c {
                    stages.push(stage);
                }
            }
        }
        assert_eq!(stages, vec![1, 2, 3, SITE_DONE], "стадии стройки: {stages:?}");
        assert_eq!(w.sites[0], SITE_DONE);
    }

    #[test]
    fn save_round_trip() {
        let mut w = Towns::new(3, 1);
        for _ in 0..30 {
            w.tick_week();
        }
        let json = serde_json::to_string(&w).unwrap();
        let back: Towns = serde_json::from_str(&json).unwrap();
        assert_eq!(back.week, w.week);
        assert_eq!(back.towns, w.towns);
        assert_eq!(back.sites, w.sites);
    }
}

#[cfg(test)]
mod balance {
    use super::*;

    /// Для настройки: `cargo test print_decade -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_decade() {
        for seed in [7u64, 1, 2] {
            let mut w = Towns::new(4, seed);
            for y in 0..10 {
                let mut raids = 0;
                let mut maxh = [0.0f32; 7];
                let mut b = [0.0f32; 7];
                let mut dd = [0.0f32; 7];
                for _ in 0..WEEKS_PER_YEAR {
                    raids += w.tick_week().iter().filter(|c| matches!(c, WorldCue::Raid { .. })).count();
                    for (i, t) in w.towns.iter().enumerate() {
                        maxh[i] = maxh[i].max(t.hunger);
                        if t.year_births > 0.0 {
                            b[i] = t.year_births;
                            dd[i] = t.year_deaths;
                        }
                    }
                }
                let line: Vec<String> = w.towns.iter().enumerate().map(|(i, t)| format!("{} {:.0} H{:.2} b{:.0} d{:.0} cr{:.2} g{:.2}", &t.id.def().name[..6.min(t.id.def().name.len())], t.pop, maxh[i], b[i], dd[i], t.crowding(), t.price(Good::Grain))).collect();
                println!("s{seed} y{y} raids{raids}: {}", line.join(" | "));
            }
        }
    }
}
