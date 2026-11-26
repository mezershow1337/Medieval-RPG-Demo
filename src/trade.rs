//! Торговля: виды товаров, торговцы и мастера Херсона и округи, расчёт цен, бартер, кража.
//!
//! Чистые данные и формулы без сети и графики. Сервер (`trade_server.rs`) хранит запасы
//! лавок и выполняет сделки, клиент (`trade_client.rs`, `trade_ui.rs`) только показывает.
//!
//! Как устроены цены:
//! - справочная цена товара — `economy::base_price` (нуммии), умножается на качество;
//! - лавка продаёт с наценкой: своя продукция дешевле, привозное дороже; мало товара — дороже;
//! - лавка покупает только то, что ей нужно (`buys`), примерно за половину цены (перекупщику
//!   надо жить), и тем дешевле, чем больше такого товара у неё уже лежит;
//! - отношение фракции к игроку (`Standing`) двигает обе цены;
//! - за стенами — бартер: медь не берут или берут плохо, золото ценят, товар меряют своей мерой.

use crate::economy::{base_price, coin_value, is_gold_coin, quality_price, round_price};
use crate::factions::{Faction, Standing};
use crate::items::{ItemBaseId, ItemShape, Quality};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Виды товаров
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Weapon,
    Armor,
    Jewelry,
    Coin,
    /// Готовые снадобья, бинты, мази.
    Remedy,
    /// Местные травы.
    Herb,
    /// Степные травы и жир.
    SteppeGood,
    /// Заморские смолы, пряности, минералы.
    Import,
    /// Простые материалы: смола, известь, свинец, зола.
    Material,
    /// Посуда и приборы: стекло, глина, медь.
    Vessel,
    Book,
    /// Вино и масло.
    WineOil,
    Textile,
    /// Шкуры, овчина, войлок.
    Hide,
    /// Рыба, соль, мёд, воск.
    Food,
    /// Самоцветы.
    Gem,
    /// Краски и прочее ремесленное.
    Dye,
}

impl Category {
    /// Сколько видов товара (размер таблиц по `Category as usize`).
    pub const COUNT: usize = 17;

    #[allow(dead_code)] // для подсказок
    pub fn name(self) -> &'static str {
        match self {
            Category::Weapon => "оружие",
            Category::Armor => "доспехи",
            Category::Jewelry => "украшения",
            Category::Coin => "монеты",
            Category::Remedy => "снадобья",
            Category::Herb => "травы",
            Category::SteppeGood => "степные товары",
            Category::Import => "заморские снадобья и минералы",
            Category::Material => "материалы",
            Category::Vessel => "посуда и приборы",
            Category::Book => "книги и свитки",
            Category::WineOil => "вино и масло",
            Category::Textile => "ткани",
            Category::Hide => "шкуры и войлок",
            Category::Food => "рыба, соль, мёд, воск",
            Category::Gem => "самоцветы",
            Category::Dye => "краски",
        }
    }
}

const HERBS: &[&str] = &[
    "Шалфей",
    "Чабрец",
    "Зверобой",
    "Белена",
    "Ягоды можжевельника",
    "Полынь",
    "Тысячелистник",
    "Подорожник",
    "Шиповник",
    "Просвирник",
    "Маковые коробочки",
    "Мята",
    "Корень марены",
    "Вайда",
    "Дубовые галлы",
    "Корень окопника",
    "Корень девясила",
];
const STEPPE: &[&str] = &["Солодковый корень", "Ревень понтийский", "Бараний жир"];
const IMPORTS: &[&str] = &[
    "Мирра",
    "Ладан",
    "Сера",
    "Квасцы",
    "Киноварь",
    "Опий",
    "Перец",
    "Шафран",
    "Корица",
    "Хиосская мастика",
    "Нефть",
    "Натр",
    "Камедь",
    "Ртуть",
    "Олово",
];
const MATERIALS: &[&str] = &[
    "Сосновая смола",
    "Негашёная известь",
    "Медный купорос",
    "Свинец",
    "Медный лом",
    "Древесная зола",
    "Щёлок",
    // Кузнечное дело.
    "Железная руда",
    "Древесный уголь",
    "Крица",
    // Горное дело и углежжение.
    "Болотная руда",
    "Обожжённая руда",
    "Пустая порода",
    "Дрова",
    "Морской песок",
    "Известняковый блок",
    "Кайло рудокопа",
    "Заступ",
    "Топор дровосека",
    "Железная полоса",
    "Стальная полоса",
    "Железный лом",
    "Бронзовый лом",
    "Серебряный лом",
    "Чугунная корка",
    "Ясеневая заготовка",
    "Гончарная глина",
    "Каменная литейная форма",
    "Свинцовые грузила",
    "Древки стрел",
    "Маховые перья",
    "Рыбий клей",
    "Сухожилия",
];
const VESSELS: &[&str] = &[
    "Ступка с пестиком",
    "Глиняный горшок",
    "Медный котелок",
    "Весы с гирями",
    "Пустой флакон",
    "Тигель",
    "Алембик",
    "Стеклянная реторта",
    "Керотакис",
    "Стеклянная чаша",
    "Жаровня",
    "Алхимический стол",
    "Сушильная решётка",
    "Глиняный светильник",
    "Бронзовый светильник",
    "Краснолаковая миска",
    "Глиняная кружка",
    "Стеклянный кубок",
    "Бронзовый кувшин",
    "Кухонный нож",
    "Ампула святого Мины",
    "Игральные кости",
    "Деревянная лошадка",
    "Веретено с пряслицем",
    "Железные ножницы",
    "Деревянная шкатулка",
    "Доска для тавлей",
];
const DYES: &[&str] = &[
    "Ярь-медянка",
    "Свинцовые белила",
    "Сурик",
    "Мареновый лак",
    "Вайдовая синь",
    "Чернила",
    "Мыло",
];
const WINE_OIL: &[&str] = &[
    "Амфора вина",
    "Амфора оливкового масла",
    "Кувшин вина",
    "Кувшин оливкового масла",
    "Винный уксус",
];
const TEXTILES: &[&str] = &["Отрез льна", "Отрез шерсти", "Моток пряжи"];
const HIDES: &[&str] = &["Сырая шкура", "Овчина", "Войлок"];
const FOOD: &[&str] = &["Солёная рыба", "Мешок соли", "Воск", "Горшок мёда", "Каравай хлеба", "Головка сыра", "Сушёные смоквы"];
/// Серебро, кость и бронза для себя: ложки, гребни, зеркальца, энколпионы.
const JEWELS: &[&str] = &["Серебряная ложка", "Костяной гребень", "Бронзовое зеркальце", "Энколпион"];
/// Школьные книги и письменный прибор — товар книжника.
const WRITING: &[&str] = &[
    "Псалтирь",
    "Дионисий Фракиец. Искусство грамматики",
    "Никомах. Введение в арифметику",
    "Восковая табличка",
    "Тростниковое перо",
    "Тетрадь пергамента",
];

/// К какому виду товаров относится предмет.
pub fn category(base: ItemBaseId) -> Category {
    let info = base.base();
    let name = info.name;
    // Сушёное и толчёное — того же рода, что и сырьё.
    let raw = crate::brewing::canonical(name).0;
    let is = |list: &[&str]| list.contains(&name) || list.contains(&raw);
    if coin_value(base).is_some() {
        return Category::Coin;
    }
    if is(HERBS) {
        return Category::Herb;
    }
    if is(WRITING) {
        return Category::Book;
    }
    if is(STEPPE) {
        return Category::SteppeGood;
    }
    if is(IMPORTS) {
        return Category::Import;
    }
    if is(MATERIALS) {
        return Category::Material;
    }
    if is(VESSELS) {
        return Category::Vessel;
    }
    if is(DYES) {
        return Category::Dye;
    }
    if is(WINE_OIL) {
        return Category::WineOil;
    }
    if is(TEXTILES) {
        return Category::Textile;
    }
    if is(HIDES) {
        return Category::Hide;
    }
    if is(FOOD) {
        return Category::Food;
    }
    if is(JEWELS) {
        return Category::Jewelry;
    }
    if name == "Рубин" {
        return Category::Gem;
    }
    if name == "Наконечники стрел" || crate::arsenal::find(name).is_some() || crate::archery::is_arrow(name) || crate::archery::is_bow(name) {
        return Category::Weapon;
    }
    if name.starts_with("Бронзов") {
        return Category::Jewelry;
    }
    // Книги узнаём по автору или слову в названии: «Гален. О…», «Книга…», «…травник».
    if name.contains(". О")
        || name.contains("Книга")
        || name.contains("травник")
        || name.contains("Синопсис")
        || name.contains("Свиток")
        || name.contains("история")
        || name.contains("Исследование")
    {
        return Category::Book;
    }
    match (info.shape, info.slot) {
        (ItemShape::Blade | ItemShape::Blunt | ItemShape::Polearm, _) => Category::Weapon,
        (ItemShape::Shield | ItemShape::Armor | ItemShape::Helmet, _) => Category::Armor,
        (ItemShape::Small, Some(_)) => Category::Jewelry,
        _ => Category::Remedy,
    }
}

// ---------------------------------------------------------------------------
// Обучение (в связке с тредом знаний: knowledge.rs)
// ---------------------------------------------------------------------------

/// Кто учит. Совпадает по именам с `knowledge::Teacher`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeacherKind {
    Iatros,
    Rhizotomos,
    Apothecary,
    Glassblower,
    Potter,
    Dyer,
    Alchemist,
    Grammarian,
    SteppeHealer,
    Smith,
    Armourer,
    GothSmith,
    SteppeSmith,
    Copyist,
    Philosopher,
    Miner,
    CharcoalBurner,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonMode {
    /// Платный урок: игрок платит мастеру.
    Paid,
    /// Работа подмастерьем или в ксеноне: мастер платит гроши, учишься медленнее, но учишься.
    Apprentice,
}

pub struct LessonDef {
    pub title: &'static str,
    pub teacher: TeacherKind,
    pub mode: LessonMode,
    /// Сколько игровых часов обучения засчитывается.
    pub hours: f32,
    /// Сколько секунд игрок должен провести рядом с мастером.
    pub secs: f32,
    /// Плата за урок или жалованье за смену (нуммии).
    pub money: u32,
    pub min: Standing,
}

// ---------------------------------------------------------------------------
// Торговцы
// ---------------------------------------------------------------------------

pub struct StockDef {
    pub item: &'static str,
    pub min: u16,
    pub max: u16,
    /// Наценка этой лавки на товар (своё — около 1, привозное вглубь города — дороже).
    pub markup: f32,
}

const fn s(item: &'static str, min: u16, max: u16, markup: f32) -> StockDef {
    StockDef {
        item,
        min,
        max,
        markup,
    }
}

/// Работа на заказ: мастер делает вещь, половина платы вперёд.
pub struct OrderDef {
    pub item: &'static str,
    pub count: u16,
    /// Полная цена (нуммии).
    pub price: u32,
    /// Сколько секунд реального времени делается.
    pub secs: f32,
    pub min: Standing,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MoneyPolicy {
    /// Обычная городская торговля: всё в монетах, со сдачей.
    Coins,
    /// Мена. Торговец не платит деньгами; монеты принимает как товар: медь по `bronze`, золото по `gold`
    /// от номинала (0 — не берёт вовсе).
    Barter { bronze: f32, gold: f32 },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// Принимает штрафы (снимает розыск).
    Fines,
    /// Принимает пожертвования на храм и ксенон.
    Donations,
    /// Скупщик: берёт краденое без вопросов.
    Fence,
    /// Кузнец: правит и точит сбитое оружие и доспехи.
    Repair,
}

/// Внешний вид торговца (рисует клиент).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub tunic: [f32; 3],
    pub trousers: [f32; 3],
    /// Шапка/шлем.
    pub helmet: bool,
}

pub struct MerchantDef {
    /// Короткий ключ (для тестов, отладки и связи с квестами).
    #[allow(dead_code)]
    pub key: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub faction: Faction,
    /// Ключ места из `level::places()` (треда города).
    pub place: &'static str,
    pub greeting: &'static str,
    pub stock: &'static [StockDef],
    /// Что покупает и насколько охотно (множитель к справочной цене).
    pub buys: &'static [(Category, f32)],
    pub money: MoneyPolicy,
    /// Насколько зорко смотрит за прилавком (0..1).
    pub vigilance: f32,
    /// Сколько денег в кассе (нуммии): больше этого за раз не заплатит.
    pub cash: u32,
    pub orders: &'static [OrderDef],
    pub lessons: &'static [LessonDef],
    pub services: &'static [Service],
    pub look: Look,
}

impl MerchantDef {
    pub fn has(&self, s: Service) -> bool {
        self.services.contains(&s)
    }

    pub fn barter(&self) -> bool {
        matches!(self.money, MoneyPolicy::Barter { .. })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MerchantId(pub u16);

impl MerchantId {
    pub fn def(self) -> &'static MerchantDef {
        &MERCHANTS[self.0 as usize]
    }
}

const GREEK: Look = Look {
    tunic: [0.78, 0.72, 0.6],
    trousers: [0.35, 0.3, 0.25],
    helmet: false,
};

/// Все торговцы, мастера и учителя.
pub const MERCHANTS: &[MerchantDef] = &[
    // ---------------- Порт: заморские купцы ----------------
    MerchantDef {
        key: "eutychios",
        name: "Евтихий из Синопы",
        title: "навикулярий, вино и масло",
        faction: Faction::Merchants,
        place: "port_market",
        greeting: "Только что разгрузились: синопское и газское вино, масло из Киликии. Берёшь амфорой — уступлю.",
        stock: &[
            s("Амфора вина", 2, 6, 1.0),
            s("Амфора оливкового масла", 1, 4, 1.0),
            s("Кувшин вина", 3, 8, 1.1),
            s("Кувшин оливкового масла", 2, 6, 1.1),
            s("Стеклянная чаша", 2, 8, 1.0),
            s("Отрез льна", 1, 3, 1.0),
        ],
        // Назад корабль везёт солёную рыбу, соль, шкуры, воск и мёд.
        buys: &[
            (Category::Food, 1.25),
            (Category::Hide, 1.8),
            (Category::Textile, 0.8),
            (Category::WineOil, 0.7),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.55,
        cash: 7200 * 4,
        orders: &[],
        lessons: &[],
        services: &[],
        look: Look {
            tunic: [0.55, 0.32, 0.22],
            trousers: [0.3, 0.25, 0.2],
            helmet: false,
        },
    },
    MerchantDef {
        key: "abbakum",
        name: "Аввакум-сириец",
        title: "пряности, смолы и благовония",
        faction: Faction::Merchants,
        place: "port_market",
        greeting: "Мирра из Аравии, ладан, перец из Индии через Клисму. Дорого? Это море стоит дорого, друг.",
        stock: &[
            s("Мирра", 1, 4, 1.0),
            s("Ладан", 2, 6, 1.0),
            s("Перец", 1, 4, 1.0),
            s("Шафран", 0, 2, 1.0),
            s("Корица", 0, 2, 1.0),
            s("Хиосская мастика", 1, 3, 1.0),
            s("Камедь", 1, 4, 1.0),
            s("Опий", 0, 2, 1.1),
            s("Натр", 1, 4, 1.0),
        ],
        buys: &[
            (Category::SteppeGood, 1.9),
            (Category::Food, 0.9),
            (Category::Gem, 0.9),
            (Category::Import, 0.6),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.75,
        cash: 7200 * 6,
        orders: &[],
        lessons: &[],
        services: &[],
        look: Look {
            tunic: [0.35, 0.28, 0.5],
            trousers: [0.25, 0.2, 0.18],
            helmet: false,
        },
    },
    MerchantDef {
        key: "kallinikos",
        name: "Каллиник",
        title: "сера, квасцы, киноварь, нефть с Боспора",
        faction: Faction::Merchants,
        place: "port_market",
        greeting: "Квасцы для красильщиков, сера, свинец. Нефть с Тамани — держи подальше от огня.",
        stock: &[
            s("Сера", 2, 8, 1.0),
            s("Квасцы", 2, 6, 1.0),
            s("Киноварь", 0, 2, 1.0),
            s("Нефть", 1, 5, 1.0),
            s("Медный купорос", 1, 4, 1.0),
            s("Свинец", 1, 4, 1.0),
            s("Негашёная известь", 2, 6, 1.0),
            // Бурый железняк с Керченского полуострова и олово из Средиземноморья.
            s("Железная руда", 4, 12, 1.0),
            s("Олово", 2, 6, 1.0),
            s("Серебряный лом", 0, 3, 1.0),
        ],
        buys: &[
            (Category::Material, 0.9),
            (Category::Import, 0.7),
            (Category::Dye, 0.9),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.6,
        cash: 7200 * 3,
        orders: &[],
        lessons: &[],
        services: &[],
        look: GREEK,
    },
    // ---------------- Агора: граждане ----------------
    MerchantDef {
        key: "andronikos",
        name: "Андроник",
        title: "лавочник на агоре",
        faction: Faction::Citizens,
        place: "agora",
        greeting: "Соль, рыба, воск, горшки, полотно на бинты — всё, что нужно в хозяйстве.",
        stock: &[
            s("Бинт", 4, 12, 1.0),
            s("Отрез шерсти", 1, 3, 1.0),
            s("Солёная рыба", 4, 10, 1.0),
            s("Мешок соли", 1, 3, 1.0),
            s("Воск", 3, 8, 1.0),
            s("Горшок мёда", 1, 4, 1.0),
            s("Глиняный горшок", 2, 6, 1.0),
            s("Сушильная решётка", 1, 3, 1.0),
            s("Пустой флакон", 2, 6, 1.1),
            s("Сосновая смола", 2, 6, 1.0),
            s("Маховые перья", 20, 60, 1.0),
            s("Древесная зола", 3, 10, 1.0),
            s("Кувшин вина", 2, 5, 1.2),
            s("Винный уксус", 1, 4, 1.0),
        ],
        buys: &[
            (Category::Food, 0.9),
            (Category::Hide, 0.85),
            (Category::Textile, 0.85),
            (Category::Vessel, 0.7),
            (Category::Material, 0.8),
            (Category::Herb, 0.8),
            (Category::Remedy, 0.6),
            (Category::Weapon, 0.5),
            (Category::Armor, 0.5),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.45,
        cash: 7200 * 2,
        orders: &[],
        lessons: &[],
        services: &[],
        look: GREEK,
    },
    MerchantDef {
        key: "ioannes",
        name: "Иоанн-книжник",
        title: "переписчик и продавец книг",
        faction: Faction::Citizens,
        place: "agora",
        greeting: "Диоскорид, Гален, Гиппократ — переписаны моей рукой, без ошибок. Научу и письму, а умеющему писать дам работу: переписывать.",
        stock: &[
            s("Обрывок травника", 1, 2, 1.0),
            s("Гиппократ. О язвах", 0, 1, 1.0),
            s("Диоскорид. О лекарственных веществах", 0, 1, 1.0),
            s("Гален. О простых лекарствах", 0, 1, 1.0),
            s("Гален. О противоядиях", 0, 1, 1.0),
            s("Книга красильщика", 0, 1, 1.0),
            s("Эней Тактик. Об обороне городов", 0, 1, 1.0),
            s("Феофраст. О камнях", 0, 1, 1.0),
            s("Феофраст. Исследование о растениях", 0, 1, 1.0),
            s("Чернила", 2, 6, 1.0),
            s("Псалтирь", 1, 2, 1.0),
            s("Дионисий Фракиец. Искусство грамматики", 0, 1, 1.0),
            s("Никомах. Введение в арифметику", 0, 1, 1.0),
            s("Восковая табличка", 2, 5, 1.0),
            s("Тростниковое перо", 3, 10, 1.0),
            s("Тетрадь пергамента", 8, 30, 1.0),
        ],
        buys: &[(Category::Book, 1.0)],
        money: MoneyPolicy::Coins,
        vigilance: 0.85,
        cash: 7200 * 5,
        orders: &[
            OrderDef {
                item: "Орибасий. Синопсис",
                count: 1,
                price: 7200 * 3,
                secs: 900.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Иллюстрированный травник",
                count: 1,
                price: 7200 * 6,
                secs: 1500.0,
                min: Standing::Friendly,
            },
        ],
        lessons: &[
            LessonDef {
                title: "Урок письма у книгописца",
                teacher: TeacherKind::Copyist,
                mode: LessonMode::Paid,
                hours: 2.0,
                secs: 60.0,
                money: 40 * 6,
                min: Standing::Neutral,
            },
            LessonDef {
                title: "Работа в скриптории: линовать, растирать чернила, переписывать",
                teacher: TeacherKind::Copyist,
                mode: LessonMode::Apprentice,
                hours: 1.5,
                secs: 60.0,
                money: 40 * 2,
                min: Standing::Neutral,
            },
        ],
        services: &[],
        look: Look {
            tunic: [0.2, 0.24, 0.35],
            trousers: [0.25, 0.22, 0.2],
            helmet: false,
        },
    },
    MerchantDef {
        key: "stephanos",
        name: "Стефан-аптекарь",
        title: "аптека на агоре",
        faction: Faction::Citizens,
        place: "apothecary_agora",
        greeting: "Отвары, мази, припарки. Сам не умеешь — купи готовое; хочешь уметь — плати за науку.",
        stock: &[
            s("Бинт", 3, 8, 1.1),
            s("Лубок", 1, 3, 1.0),
            s("Кровоостанавливающая припарка", 1, 4, 1.0),
            s("Отвар шалфея", 1, 4, 1.0),
            s("Отвар чабреца", 1, 4, 1.0),
            s("Ранозаживляющая мазь", 0, 3, 1.0),
            s("Оксимель", 1, 3, 1.0),
            s("Маковый отвар", 0, 2, 1.0),
            s("Териак", 0, 1, 1.0),
            s("Пустой флакон", 2, 6, 1.0),
            s("Ступка с пестиком", 0, 1, 1.0),
        ],
        buys: &[
            (Category::Herb, 1.0),
            (Category::SteppeGood, 1.1),
            (Category::Remedy, 0.7),
            (Category::Import, 0.8),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.7,
        cash: 7200 * 2,
        orders: &[],
        lessons: &[LessonDef {
            title: "Урок аптекарского дела",
            teacher: TeacherKind::Apothecary,
            mode: LessonMode::Paid,
            hours: 2.0,
            secs: 75.0,
            money: 40 * 20,
            min: Standing::Neutral,
        }],
        services: &[],
        look: Look {
            tunic: [0.4, 0.45, 0.32],
            trousers: [0.3, 0.26, 0.2],
            helmet: false,
        },
    },
    MerchantDef {
        key: "zenon",
        name: "Зенон",
        title: "аптекарь у порта",
        faction: Faction::Citizens,
        place: "apothecary_port",
        greeting: "Моряки режутся и обжигаются — бинты и мази у меня всегда есть. Травы беру, если свежие.",
        stock: &[
            s("Бинт", 4, 10, 1.0),
            s("Кровоостанавливающая припарка", 1, 3, 1.0),
            s("Зверобойное масло", 1, 3, 1.0),
            s("Серная вода", 0, 2, 1.0),
            s("Винный уксус", 1, 4, 1.0),
            s("Целебное зелье", 0, 2, 1.2),
        ],
        buys: &[
            (Category::Herb, 0.95),
            (Category::Remedy, 0.65),
            (Category::SteppeGood, 1.0),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.55,
        cash: 7200,
        orders: &[],
        lessons: &[],
        services: &[],
        look: GREEK,
    },
    // ---------------- Мастерские ----------------
    MerchantDef {
        key: "eustathios",
        name: "Мастер Евстафий",
        title: "стеклодув",
        faction: Faction::Citizens,
        place: "glassworks",
        greeting: "Чаши и флаконы — с полки. Алембик или реторту выдую на заказ, но это работа не на час и не за медяки.",
        stock: &[
            s("Стеклянная чаша", 3, 10, 0.9),
            s("Пустой флакон", 4, 12, 0.9),
        ],
        buys: &[(Category::Material, 0.8), (Category::Vessel, 0.5)],
        money: MoneyPolicy::Coins,
        vigilance: 0.5,
        cash: 7200 * 2,
        orders: &[
            OrderDef {
                item: "Алембик",
                count: 1,
                price: 7200,
                secs: 600.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Стеклянная реторта",
                count: 1,
                price: 5400,
                secs: 480.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Пустой флакон",
                count: 6,
                price: 40 * 10,
                secs: 120.0,
                min: Standing::Neutral,
            },
        ],
        lessons: &[
            LessonDef {
                title: "Подмастерье: таскать песок и золу, смотреть у печи",
                teacher: TeacherKind::Glassblower,
                mode: LessonMode::Apprentice,
                hours: 4.0,
                secs: 120.0,
                money: 40 * 2,
                min: Standing::Neutral,
            },
            LessonDef {
                title: "Урок выдувания (платно)",
                teacher: TeacherKind::Glassblower,
                mode: LessonMode::Paid,
                hours: 3.0,
                secs: 75.0,
                money: 40 * 45,
                min: Standing::Friendly,
            },
        ],
        services: &[],
        look: Look {
            tunic: [0.45, 0.38, 0.3],
            trousers: [0.22, 0.2, 0.18],
            helmet: false,
        },
    },
    MerchantDef {
        key: "agapetos",
        name: "Гончар Агапит",
        title: "гончарная мастерская",
        faction: Faction::Citizens,
        place: "pottery",
        greeting: "Горшки, тигли, ступки. Тигель для огня — из особой глины, его тоже сделаю на заказ.",
        stock: &[
            s("Глиняный горшок", 4, 12, 0.9),
            s("Тигель", 1, 4, 0.9),
            s("Ступка с пестиком", 1, 2, 0.9),
            s("Гончарная глина", 4, 12, 0.9),
        ],
        buys: &[(Category::Material, 0.7)],
        money: MoneyPolicy::Coins,
        vigilance: 0.4,
        cash: 7200,
        orders: &[
            OrderDef {
                item: "Тигель",
                count: 3,
                price: 40 * 14,
                secs: 180.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Ступка с пестиком",
                count: 1,
                price: 40 * 12,
                secs: 240.0,
                min: Standing::Neutral,
            },
        ],
        lessons: &[LessonDef {
            title: "Подмастерье: месить глину, топить печь",
            teacher: TeacherKind::Potter,
            mode: LessonMode::Apprentice,
            hours: 4.0,
            secs: 120.0,
            money: 40 * 2,
            min: Standing::Neutral,
        }],
        services: &[],
        look: Look {
            tunic: [0.6, 0.45, 0.32],
            trousers: [0.3, 0.24, 0.18],
            helmet: false,
        },
    },
    MerchantDef {
        key: "philippos",
        name: "Филипп",
        title: "кузнец и медник",
        faction: Faction::Citizens,
        place: "smithy",
        greeting: "Котлы, весы, ножи, клинки. Железо — полосами, уголь — мерами. Горн свободен: работай, если умеешь, а нет — научу за плату или бери в подмастерья.",
        stock: &[
            s("Семиспата", 0, 1, 1.0),
            s("Солдатская спата", 0, 1, 1.0),
            s("Боевой нож", 1, 3, 1.0),
            s("Ланкия", 0, 2, 1.0),
            s("Дротик", 1, 4, 1.0),
            s("Плотничий топор", 0, 1, 1.0),
            s("Бронебойные наконечники", 10, 40, 1.0),
            s("Медный котелок", 1, 2, 1.0),
            s("Жаровня", 1, 2, 1.0),
            s("Древесный уголь", 8, 20, 1.0),
            s("Весы с гирями", 0, 1, 1.0),
            s("Кинжал", 1, 3, 1.0),
            s("Спата", 0, 1, 1.1),
            s("Кожаные перчатки", 0, 2, 1.0),
            s("Железная полоса", 2, 6, 1.0),
            s("Стальная полоса", 0, 2, 1.1),
            s("Железный лом", 2, 6, 1.0),
            s("Ясеневая заготовка", 1, 3, 1.0),
            s("Каменная литейная форма", 0, 1, 1.0),
            s("Бронзовый лом", 1, 4, 1.0),
            s("Олово", 1, 4, 1.15),
            s("Бронзовое навершие", 0, 2, 1.0),
            s("Наконечники стрел", 10, 30, 1.0),
            s("Стрелы", 10, 30, 1.1),
            s("Бронебойные стрелы", 0, 20, 1.1),
            s("Древки стрел", 10, 30, 1.0),
            s("Маховые перья", 30, 90, 1.0),
            s("Рыбий клей", 1, 3, 1.1),
            s("Простой лук", 0, 2, 1.1),
        ],
        buys: &[
            (Category::Material, 0.9),
            (Category::Weapon, 0.6),
            (Category::Armor, 0.55),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.55,
        cash: 7200 * 2,
        orders: &[
            OrderDef {
                item: "Керотакис",
                count: 1,
                price: 4400,
                secs: 480.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Медный котелок",
                count: 1,
                price: 1500,
                secs: 300.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Весы с гирями",
                count: 1,
                price: 3000,
                secs: 420.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Спата",
                count: 1,
                price: 6000,
                secs: 540.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Пехотное копьё",
                count: 1,
                price: 4200,
                secs: 360.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Боевой топор",
                count: 1,
                price: 2400,
                secs: 300.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Алхимический стол",
                count: 1,
                price: 7200 * 2 + 2400,
                secs: 600.0,
                min: Standing::Neutral,
            },
        ],
        lessons: &[
            LessonDef {
                title: "Урок у горна: ковка, сварка, закалка",
                teacher: TeacherKind::Smith,
                mode: LessonMode::Paid,
                hours: 1.5,
                secs: 90.0,
                money: 40 * 30,
                min: Standing::Neutral,
            },
            LessonDef {
                title: "Подмастерье: качать меха, бить кувалдой",
                teacher: TeacherKind::Smith,
                mode: LessonMode::Apprentice,
                hours: 4.0,
                secs: 120.0,
                money: 40 * 2,
                min: Standing::Neutral,
            },
        ],
        services: &[Service::Repair],
        look: Look {
            tunic: [0.3, 0.27, 0.24],
            trousers: [0.2, 0.18, 0.16],
            helmet: false,
        },
    },
    MerchantDef {
        key: "pamphilos",
        name: "Памфил",
        title: "красильщик",
        faction: Faction::Citizens,
        place: "agora",
        greeting: "Марена, вайда и квасцы — вот и вся тайна. Почти вся.",
        stock: &[
            s("Мареновый лак", 0, 2, 1.0),
            s("Вайдовая синь", 0, 2, 1.0),
            s("Квасцы", 1, 3, 1.2),
            s("Корень марены", 2, 6, 1.0),
            s("Вайда", 2, 6, 1.0),
        ],
        buys: &[
            (Category::Dye, 0.9),
            (Category::Herb, 0.7),
            (Category::Textile, 0.8),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.5,
        cash: 7200,
        orders: &[],
        lessons: &[LessonDef {
            title: "Подмастерье в красильне",
            teacher: TeacherKind::Dyer,
            mode: LessonMode::Apprentice,
            hours: 4.0,
            secs: 120.0,
            money: 40 * 2,
            min: Standing::Neutral,
        }],
        services: &[],
        look: Look {
            tunic: [0.42, 0.18, 0.25],
            trousers: [0.2, 0.18, 0.3],
            helmet: false,
        },
    },
    MerchantDef {
        key: "theodora",
        name: "Феодора",
        title: "рыбозасольщица",
        faction: Faction::Citizens,
        place: "fish_salting",
        greeting: "Хамса, кефаль, осётр — из цистерн, в рассоле по-херсонски. Корабли берут бочками.",
        stock: &[s("Солёная рыба", 8, 20, 0.85), s("Мешок соли", 2, 5, 0.9), s("Рыбий клей", 2, 6, 0.9)],
        buys: &[(Category::Food, 0.7)],
        money: MoneyPolicy::Coins,
        vigilance: 0.35,
        cash: 7200,
        orders: &[],
        lessons: &[],
        services: &[],
        look: Look {
            tunic: [0.5, 0.48, 0.42],
            trousers: [0.3, 0.28, 0.26],
            helmet: false,
        },
    },
    // ---------------- Церковь ----------------
    MerchantDef {
        key: "agapios",
        name: "Ятрос Агапий",
        title: "врач монастырского ксенона",
        faction: Faction::Church,
        place: "basilica",
        greeting: "В ксеноне всегда нужны руки. Поработай — увидишь больше, чем вычитаешь у Галена.",
        stock: &[s("Бинт", 2, 6, 1.0), s("Лубок", 1, 4, 1.0), s("Отвар шалфея", 0, 2, 1.0)],
        buys: &[(Category::Herb, 0.9), (Category::Remedy, 0.8)],
        money: MoneyPolicy::Coins,
        vigilance: 0.3,
        cash: 7200,
        orders: &[],
        lessons: &[
            LessonDef {
                title: "Смена в ксеноне: перевязки, отвары, уход за больными",
                teacher: TeacherKind::Iatros,
                mode: LessonMode::Apprentice,
                hours: 4.0,
                secs: 150.0,
                money: 40 * 3,
                min: Standing::Neutral,
            },
            LessonDef {
                title: "Урок врачевания у ятроса",
                teacher: TeacherKind::Iatros,
                mode: LessonMode::Paid,
                hours: 2.0,
                secs: 75.0,
                money: 40 * 30,
                min: Standing::Friendly,
            },
        ],
        services: &[Service::Donations],
        look: Look {
            tunic: [0.2, 0.18, 0.17],
            trousers: [0.15, 0.14, 0.13],
            helmet: false,
        },
    },
    MerchantDef {
        key: "chariton",
        name: "Харитон-ризотом",
        title: "сборщик трав",
        faction: Faction::Citizens,
        place: "main_gate",
        greeting: "Каждое утро за ворота, к балкам и на Гераклейский полуостров. Покажу, что рвать, а что обходить.",
        stock: &[
            s("Шалфей", 2, 6, 1.0),
            s("Чабрец", 2, 6, 1.0),
            s("Зверобой", 1, 5, 1.0),
            s("Подорожник", 3, 8, 1.0),
            s("Тысячелистник", 2, 5, 1.0),
            s("Полынь", 2, 5, 1.0),
            s("Сушёный шалфей", 1, 4, 1.0),
            s("Сушёная мята", 1, 4, 1.0),
            s("Сушёный зверобой", 0, 3, 1.0),
            s("Сушильная решётка", 1, 2, 1.0),
        ],
        buys: &[(Category::Herb, 0.8)],
        money: MoneyPolicy::Coins,
        vigilance: 0.4,
        cash: 2400,
        orders: &[],
        lessons: &[LessonDef {
            title: "Сбор трав с ризотомом за стенами",
            teacher: TeacherKind::Rhizotomos,
            mode: LessonMode::Apprentice,
            hours: 3.0,
            secs: 120.0,
            money: 40,
            min: Standing::Neutral,
        }],
        services: &[],
        look: Look {
            tunic: [0.45, 0.5, 0.35],
            trousers: [0.3, 0.28, 0.22],
            helmet: false,
        },
    },
    // ---------------- Гарнизон ----------------
    MerchantDef {
        key: "markianos",
        name: "Актуарий Маркиан",
        title: "казначей гарнизона",
        faction: Faction::Garrison,
        place: "garrison",
        greeting: "Списанное снаряжение продаю, трофеи покупаю. Штрафы — тоже ко мне.",
        stock: &[
            s("Спата", 1, 2, 1.0),
            s("Овальный щит", 1, 2, 1.0),
            s("Гребенчатый шлем", 1, 2, 1.0),
            s("Стёганая туника", 1, 3, 1.0),
            s("Кольчуга", 0, 1, 1.1),
            s("Кожаные сапоги", 1, 3, 1.0),
            s("Бинт", 3, 8, 1.0),
        ],
        buys: &[(Category::Weapon, 0.7), (Category::Armor, 0.7)],
        money: MoneyPolicy::Coins,
        vigilance: 0.9,
        cash: 7200 * 6,
        orders: &[],
        lessons: &[],
        services: &[Service::Fines],
        look: Look {
            tunic: [0.55, 0.12, 0.1],
            trousers: [0.2, 0.18, 0.17],
            helmet: true,
        },
    },
    // ---------------- Ссыльные ----------------
    MerchantDef {
        key: "apollinarios",
        name: "Флавий Аполлинарий",
        title: "опальный сенатор",
        faction: Faction::Exiles,
        place: "exiles_house",
        greeting: "Константинополь далеко, а жить надо. Продаю фамильное. Хорошее вино и книги куплю щедро.",
        stock: &[
            s("Серебряный перстень", 0, 2, 1.0),
            s("Нательный крест", 0, 1, 1.0),
            s("Рубин", 0, 1, 1.0),
            s("Цельс. О медицине", 0, 1, 1.0),
        ],
        buys: &[
            (Category::WineOil, 1.3),
            (Category::Import, 1.2),
            (Category::Book, 1.15),
            (Category::Textile, 1.1),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.4,
        cash: 7200 * 8,
        orders: &[],
        lessons: &[],
        services: &[],
        look: Look {
            tunic: [0.45, 0.12, 0.35],
            trousers: [0.25, 0.2, 0.25],
            helmet: false,
        },
    },
    MerchantDef {
        key: "olympios",
        name: "Философ Олимпий",
        title: "сосланный александрийский философ",
        faction: Faction::Exiles,
        place: "exiles_house",
        greeting: "Зосим писал о печах и приборах, но книга без рук ничего не даст. Учу немногих и недёшево: и числам по Никомаху, и грамматике.",
        stock: &[
            s("Зосим. О приборах и печах", 0, 1, 1.0),
            s("Ртуть", 0, 2, 1.1),
        ],
        buys: &[(Category::Import, 1.1), (Category::Book, 1.1)],
        money: MoneyPolicy::Coins,
        vigilance: 0.5,
        cash: 7200 * 3,
        orders: &[],
        lessons: &[
            LessonDef {
                title: "Беседа о перегонке и печах",
                teacher: TeacherKind::Alchemist,
                mode: LessonMode::Paid,
                hours: 3.0,
                secs: 90.0,
                money: 7200 / 2,
                min: Standing::Friendly,
            },
            LessonDef {
                title: "Беседа о числах и грамматике",
                teacher: TeacherKind::Philosopher,
                mode: LessonMode::Paid,
                hours: 3.0,
                secs: 90.0,
                money: 40 * 30,
                min: Standing::Neutral,
            },
        ],
        services: &[],
        look: Look {
            tunic: [0.85, 0.82, 0.75],
            trousers: [0.4, 0.38, 0.35],
            helmet: false,
        },
    },
    // ---------------- Портовое дно ----------------
    MerchantDef {
        key: "mardonios",
        name: "Мардоний",
        title: "хозяин портовой таверны",
        faction: Faction::Underworld,
        place: "tavern_port",
        greeting: "Откуда вещь — не моё дело. Цена — моё.",
        stock: &[
            s("Кувшин вина", 3, 8, 1.3),
            s("Кинжал", 1, 2, 1.2),
            s("Опий", 0, 2, 1.4),
            s("Нефть", 0, 2, 1.3),
            s("Зажигательный горшок", 0, 1, 1.5),
        ],
        buys: &[
            (Category::Weapon, 0.6),
            (Category::Armor, 0.55),
            (Category::Jewelry, 0.7),
            (Category::Gem, 0.7),
            (Category::Import, 0.7),
            (Category::Book, 0.5),
            (Category::WineOil, 0.7),
            (Category::Remedy, 0.6),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.8,
        cash: 7200 * 3,
        orders: &[],
        lessons: &[],
        services: &[Service::Fence],
        look: Look {
            tunic: [0.32, 0.25, 0.18],
            trousers: [0.18, 0.16, 0.14],
            helmet: false,
        },
    },
    MerchantDef {
        key: "vitalianos",
        name: "Виталиан",
        title: "мастер-оружейник гарнизонной фабрики",
        faction: Faction::Garrison,
        place: "armoury",
        greeting: "Здесь куют для стратиотов: клинки, кольчуги, шлемы. Чужому продам, если не враг империи. Кольчуге учат годами.",
        stock: &[
            s("Меч букеллария", 0, 1, 1.1),
            s("Контос", 0, 1, 1.1),
            s("Менавлон", 0, 2, 1.0),
            s("Плюмбата", 4, 12, 1.0),
            s("Пехотная пика", 0, 2, 1.0),
            s("Копьё с ромбическим пером", 0, 2, 1.0),
            s("Гранёная булава", 0, 1, 1.0),
            s("Бронебойные наконечники", 20, 60, 0.9),
            s("Кольчуга", 0, 1, 1.15),
            s("Спангенхельм", 0, 1, 1.1),
            s("Ламеллярные наручи", 0, 2, 1.05),
            s("Железные поножи", 0, 1, 1.05),
            s("Овальный щит", 1, 2, 1.0),
            s("Кавалерийская спата", 0, 1, 1.15),
            s("Наконечники стрел", 20, 60, 0.95),
            s("Стальная полоса", 1, 3, 1.05),
            s("Железная полоса", 2, 5, 1.0),
            s("Сырая шкура", 1, 3, 1.1),
        ],
        buys: &[
            (Category::Weapon, 0.55),
            (Category::Armor, 0.6),
            (Category::Material, 0.8),
        ],
        money: MoneyPolicy::Coins,
        vigilance: 0.85,
        cash: 7200 * 3,
        orders: &[
            OrderDef {
                item: "Кольчуга",
                count: 1,
                price: 18000,
                secs: 900.0,
                min: Standing::Friendly,
            },
            OrderDef {
                item: "Ламеллярный панцирь",
                count: 1,
                price: 26000,
                secs: 900.0,
                min: Standing::Friendly,
            },
            OrderDef {
                item: "Спангенхельм",
                count: 1,
                price: 6000,
                secs: 480.0,
                min: Standing::Neutral,
            },
            OrderDef {
                item: "Кавалерийская спата",
                count: 1,
                price: 9000,
                secs: 600.0,
                min: Standing::Friendly,
            },
        ],
        lessons: &[
            LessonDef {
                title: "Урок оружейника: доспехи, закалка клинка",
                teacher: TeacherKind::Armourer,
                mode: LessonMode::Paid,
                hours: 1.5,
                secs: 90.0,
                money: 40 * 60,
                min: Standing::Friendly,
            },
            LessonDef {
                title: "Подмастерье в фабрике: тянуть проволоку, клепать кольца",
                teacher: TeacherKind::Armourer,
                mode: LessonMode::Apprentice,
                hours: 4.0,
                secs: 120.0,
                money: 40 * 3,
                min: Standing::Neutral,
            },
        ],
        services: &[Service::Repair],
        look: Look {
            tunic: [0.42, 0.18, 0.15],
            trousers: [0.22, 0.2, 0.18],
            helmet: false,
        },
    },
    // ---------------- За стенами ----------------
    MerchantDef {
        key: "ansila",
        name: "Ансила",
        title: "готский кузнец",
        faction: Faction::Goths,
        place: "goth_smithy",
        greeting: "Руду возим из-за пролива, уголь жжём в горах. Сам плавлю, сам кую. Клинок из пакета — дольше, зато не сломается.",
        stock: &[
            s("Готская спата с кольцом", 0, 1, 1.0),
            s("Бородовидная секира", 0, 2, 0.9),
            s("Длинный сакс", 0, 2, 0.9),
            s("Крылатое копьё", 0, 2, 0.9),
            s("Боевой молот", 0, 1, 0.9),
            s("Древесный уголь", 8, 24, 0.75),
            s("Железная руда", 2, 8, 0.9),
            s("Крица", 0, 2, 0.85),
            s("Железная полоса", 1, 4, 0.9),
            s("Боевой топор", 0, 2, 0.9),
            s("Кинжал", 1, 2, 0.9),
            s("Круглый щит", 0, 1, 0.9),
            s("Ясеневая заготовка", 2, 4, 0.75),
        ],
        buys: &[
            (Category::WineOil, 1.4),
            (Category::Textile, 1.3),
            (Category::Material, 1.0),
            (Category::Weapon, 1.1),
        ],
        money: MoneyPolicy::Barter {
            bronze: 0.8,
            gold: 1.0,
        },
        vigilance: 0.55,
        cash: 0,
        orders: &[],
        lessons: &[
            LessonDef {
                title: "Ансила учит плавить руду и варить пакет",
                teacher: TeacherKind::GothSmith,
                mode: LessonMode::Paid,
                hours: 1.5,
                secs: 90.0,
                money: 40 * 24,
                min: Standing::Neutral,
            },
            LessonDef {
                title: "Подмастерье: жечь уголь, дуть мехами у горна",
                teacher: TeacherKind::GothSmith,
                mode: LessonMode::Apprentice,
                hours: 4.0,
                secs: 120.0,
                money: 40,
                min: Standing::Neutral,
            },
        ],
        services: &[Service::Repair],
        look: Look {
            tunic: [0.36, 0.3, 0.24],
            trousers: [0.24, 0.2, 0.17],
            helmet: false,
        },
    },
    MerchantDef {
        key: "theodosios",
        name: "Феодосий",
        title: "десятник рудника",
        faction: Faction::Citizens,
        place: "mine",
        greeting: "Руда тут бурая, железняк. Кто кайло держать умеет — бери поденщину у ворота. В забой без науки не пущу: крепь не знаешь — завалит.",
        stock: &[
            s("Железная руда", 6, 16, 1.0),
            s("Обожжённая руда", 2, 8, 1.0),
            s("Болотная руда", 0, 6, 0.9),
            s("Пустая порода", 0, 4, 1.0),
            s("Кайло рудокопа", 1, 2, 1.0),
            s("Заступ", 1, 2, 1.0),
            s("Дрова", 4, 12, 1.0),
            s("Известняковый блок", 0, 2, 1.0),
        ],
        buys: &[(Category::Material, 0.8), (Category::Food, 1.1), (Category::WineOil, 1.1)],
        money: MoneyPolicy::Coins,
        vigilance: 0.6,
        cash: 40 * 300,
        orders: &[],
        lessons: &[
            LessonDef {
                title: "Феодосий учит различать руду, обжигать её и крепить штольню",
                teacher: TeacherKind::Miner,
                mode: LessonMode::Paid,
                hours: 2.0,
                secs: 90.0,
                money: 40 * 12,
                min: Standing::Neutral,
            },
            LessonDef {
                title: "Подмастерье: таскать корзины с рудой из забоя",
                teacher: TeacherKind::Miner,
                mode: LessonMode::Apprentice,
                hours: 4.0,
                secs: 120.0,
                money: 40,
                min: Standing::Neutral,
            },
        ],
        services: &[],
        look: Look {
            tunic: [0.4, 0.33, 0.24],
            trousers: [0.26, 0.22, 0.18],
            helmet: false,
        },
    },
    MerchantDef {
        key: "wulfila",
        name: "Вульфила",
        title: "углежог",
        faction: Faction::Goths,
        place: "charcoal_camp",
        greeting: "Дуб да бук — лучший уголь. Куча горит неделю, глаз не сомкнёшь: прозевал дым — и вместо угля зола.",
        stock: &[
            s("Древесный уголь", 12, 40, 0.85),
            s("Дрова", 8, 24, 0.9),
            s("Топор дровосека", 1, 2, 1.0),
            s("Заступ", 0, 1, 1.0),
        ],
        buys: &[(Category::Food, 1.3), (Category::WineOil, 1.4), (Category::Textile, 1.2), (Category::Material, 0.8)],
        money: MoneyPolicy::Barter { bronze: 0.85, gold: 1.0 },
        vigilance: 0.4,
        cash: 0,
        orders: &[],
        lessons: &[
            LessonDef {
                title: "Вульфила учит класть и вести угольную кучу",
                teacher: TeacherKind::CharcoalBurner,
                mode: LessonMode::Paid,
                hours: 2.0,
                secs: 90.0,
                money: 40 * 8,
                min: Standing::Neutral,
            },
            LessonDef {
                title: "Подмастерье: носить дёрн и землю, колоть поленья",
                teacher: TeacherKind::CharcoalBurner,
                mode: LessonMode::Apprentice,
                hours: 4.0,
                secs: 120.0,
                money: 30,
                min: Standing::Neutral,
            },
        ],
        services: &[],
        look: Look {
            tunic: [0.25, 0.23, 0.2],
            trousers: [0.2, 0.18, 0.16],
            helmet: false,
        },
    },
    MerchantDef {
        key: "irnik",
        name: "Ирник",
        title: "кочевой кузнец",
        faction: Faction::Nomads,
        place: "steppe_forge",
        greeting: "Наконечники — сотнями, бляшки для поясов — литые. Горн мой лёгкий: снялись — и в путь.",
        stock: &[
            s("Степной изогнутый меч", 0, 1, 1.0),
            s("Гуннский кинжал", 0, 2, 0.9),
            s("Кистень", 0, 2, 0.9),
            s("Гирька на ремне", 1, 3, 0.8),
            s("Трёхлопастные наконечники", 20, 60, 0.8),
            s("Срезни", 10, 40, 0.8),
            s("Наконечники стрел", 30, 90, 0.8),
            s("Гуннский составной лук", 0, 2, 0.9),
            s("Трёхлопастные стрелы", 10, 30, 0.85),
            s("Стрелы", 10, 30, 0.85),
            s("Сухожилия", 3, 10, 0.8),
            s("Бронзовый лом", 2, 6, 0.85),
            s("Олово", 1, 3, 1.0),
            s("Бронзовая пряжка", 1, 3, 0.85),
            s("Кинжал", 0, 2, 0.9),
            s("Древесный уголь", 4, 10, 0.9),
        ],
        buys: &[
            (Category::WineOil, 1.6),
            (Category::Material, 1.1),
            (Category::Weapon, 1.4),
            (Category::Jewelry, 1.3),
        ],
        money: MoneyPolicy::Barter {
            bronze: 0.0,
            gold: 1.15,
        },
        vigilance: 0.7,
        cash: 0,
        orders: &[],
        lessons: &[LessonDef {
            title: "Ирник показывает литьё бляшек и наконечники",
            teacher: TeacherKind::SteppeSmith,
            mode: LessonMode::Paid,
            hours: 1.5,
            secs: 90.0,
            money: 2000,
            min: Standing::Neutral,
        }],
        services: &[Service::Repair],
        look: Look {
            tunic: [0.45, 0.38, 0.28],
            trousers: [0.3, 0.26, 0.2],
            helmet: true,
        },
    },
    MerchantDef {
        key: "kandak",
        name: "Кандак",
        title: "аланский кузнец-бронник",
        faction: Faction::Alans,
        place: "alan_forge",
        greeting: "Аланский всадник одет в железо с головы до коня. Чешую и поножи делаю сам. Учить чужака? Сначала стань другом.",
        stock: &[
            s("Аланский длинный меч", 0, 1, 1.0),
            s("Аланский топорик", 0, 2, 0.9),
            s("Клевец", 0, 1, 0.9),
            s("Аланское копьё", 0, 1, 0.9),
            s("Копьё-жало", 0, 2, 0.9),
            s("Железная полоса", 1, 3, 0.95),
            s("Наконечники стрел", 10, 40, 0.85),
            s("Ламеллярные наручи", 0, 1, 0.95),
            s("Железные поножи", 0, 1, 0.95),
            s("Спангенхельм", 0, 1, 1.0),
        ],
        buys: &[
            (Category::WineOil, 1.5),
            (Category::Material, 1.1),
            (Category::Armor, 1.3),
        ],
        money: MoneyPolicy::Barter {
            bronze: 0.0,
            gold: 1.1,
        },
        vigilance: 0.75,
        cash: 0,
        orders: &[],
        lessons: &[LessonDef {
            title: "Кандак учит доспешному делу",
            teacher: TeacherKind::Armourer,
            mode: LessonMode::Paid,
            hours: 1.5,
            secs: 90.0,
            money: 3000,
            min: Standing::Friendly,
        }],
        services: &[Service::Repair],
        look: Look {
            tunic: [0.32, 0.3, 0.26],
            trousers: [0.3, 0.26, 0.2],
            helmet: true,
        },
    },
    MerchantDef {
        key: "guntharis",
        name: "Гунтарис из Дори",
        title: "готский староста",
        faction: Faction::Goths,
        place: "goth_village",
        greeting: "Мёд, воск, овчины. Нам нужно вино, полотно и стекло. Медью тоже возьму, хоть и без радости.",
        stock: &[
            s("Горшок мёда", 2, 5, 0.8),
            s("Воск", 3, 8, 0.8),
            s("Овчина", 2, 5, 0.8),
            s("Сырая шкура", 2, 4, 0.85),
            s("Ягоды можжевельника", 2, 6, 0.8),
            s("Зверобой", 2, 5, 0.8),
            s("Бараний жир", 1, 4, 0.9),
        ],
        buys: &[
            (Category::WineOil, 1.5),
            (Category::Textile, 1.4),
            (Category::Vessel, 1.4),
            (Category::Food, 1.1),
            (Category::Remedy, 1.3),
            (Category::Weapon, 1.3),
            (Category::Armor, 1.2),
        ],
        money: MoneyPolicy::Barter {
            bronze: 0.8,
            gold: 1.0,
        },
        vigilance: 0.6,
        cash: 0,
        orders: &[],
        lessons: &[],
        services: &[],
        look: Look {
            tunic: [0.4, 0.3, 0.2],
            trousers: [0.25, 0.22, 0.2],
            helmet: false,
        },
    },
    MerchantDef {
        key: "saros",
        name: "Сарос",
        title: "аланский старейшина",
        faction: Faction::Alans,
        place: "alan_camp",
        greeting: "Медь оставь себе. Золото, стекло, вино, лекарства — тогда поговорим.",
        stock: &[
            s("Сырая шкура", 2, 5, 0.85),
            s("Войлок", 1, 4, 0.85),
            s("Бараний жир", 2, 5, 0.8),
            s("Солодковый корень", 1, 4, 0.9),
        ],
        buys: &[
            (Category::WineOil, 1.5),
            (Category::Vessel, 1.6),
            (Category::Textile, 1.5),
            (Category::Remedy, 1.4),
            (Category::Food, 1.2),
            (Category::Weapon, 1.5),
            (Category::Jewelry, 1.3),
        ],
        money: MoneyPolicy::Barter {
            bronze: 0.0,
            gold: 1.1,
        },
        vigilance: 0.7,
        cash: 0,
        orders: &[],
        lessons: &[],
        services: &[],
        look: Look {
            tunic: [0.3, 0.32, 0.25],
            trousers: [0.35, 0.28, 0.2],
            helmet: true,
        },
    },
    MerchantDef {
        key: "eskam",
        name: "Эскам",
        title: "торговец кочевой орды",
        faction: Faction::Nomads,
        place: "steppe_camp",
        greeting: "Ревень с Итиля, солодка, жир, шкуры. Вино давай, снадобья давай. Медяки — детям играть.",
        stock: &[
            s("Ревень понтийский", 1, 4, 0.8),
            s("Солодковый корень", 2, 6, 0.8),
            s("Бараний жир", 3, 8, 0.75),
            s("Овчина", 2, 5, 0.8),
            s("Войлок", 2, 4, 0.8),
            s("Сырая шкура", 2, 6, 0.8),
        ],
        buys: &[
            (Category::WineOil, 1.8),
            (Category::Remedy, 1.6),
            (Category::Textile, 1.6),
            (Category::Vessel, 1.5),
            (Category::Import, 1.4),
            (Category::Weapon, 1.6),
            (Category::Armor, 1.4),
            (Category::Jewelry, 1.4),
        ],
        money: MoneyPolicy::Barter {
            bronze: 0.0,
            gold: 1.15,
        },
        vigilance: 0.75,
        cash: 0,
        orders: &[],
        lessons: &[LessonDef {
            title: "Знахарь орды показывает степные снадобья",
            teacher: TeacherKind::SteppeHealer,
            mode: LessonMode::Paid,
            hours: 2.0,
            secs: 90.0,
            money: 2400,
            min: Standing::Friendly,
        }],
        services: &[],
        look: Look {
            tunic: [0.42, 0.36, 0.26],
            trousers: [0.3, 0.25, 0.18],
            helmet: true,
        },
    },
    // ---------------- Угольный и железный двор напротив кузницы ----------------
    MerchantDef {
        key: "nikephoros",
        name: "Никифор-угольщик",
        title: "уголь, крицы и железо для кузниц",
        faction: Faction::Citizens,
        place: "iron_yard",
        greeting: "Уголь дубовый, от готов из леса, крицы с рудника, прутки — хоть сейчас под молот. Филипп напротив берёт у меня, и ты бери: до горна десять шагов.",
        stock: &[
            s("Древесный уголь", 20, 60, 1.0),
            s("Железная полоса", 4, 12, 1.1),
            s("Стальная полоса", 1, 4, 1.15),
            s("Железный лом", 6, 20, 1.0),
            s("Крица", 2, 6, 1.1),
            s("Железная руда", 4, 12, 1.15),
        ],
        buys: &[(Category::Material, 0.75)],
        money: MoneyPolicy::Coins,
        vigilance: 0.6,
        cash: 40 * 400,
        orders: &[],
        lessons: &[],
        services: &[],
        look: Look {
            tunic: [0.3, 0.27, 0.24],
            trousers: [0.22, 0.2, 0.18],
            helmet: false,
        },
    },
];

#[allow(dead_code)]
pub fn merchant_by_key(key: &str) -> Option<MerchantId> {
    MERCHANTS
        .iter()
        .position(|m| m.key == key)
        .map(|i| MerchantId(i as u16))
}

// ---------------------------------------------------------------------------
// Цены
// ---------------------------------------------------------------------------

/// Цена одной штуки, по которой лавка продаёт игроку.
///
/// `have` — сколько штук сейчас на полке, `normal` — обычный запас: мало товара — дороже (до +30%),
/// много — дешевле (до −15%).
pub fn ask_price(
    def: &MerchantDef,
    base: ItemBaseId,
    quality: Quality,
    standing: Standing,
    have: u16,
    normal: u16,
) -> u32 {
    let markup = def
        .stock
        .iter()
        .find(|st| st.item == base.base().name)
        .map(|st| st.markup)
        .unwrap_or(1.2);
    let scarcity = if normal == 0 {
        1.0
    } else {
        (1.0 + 0.3 * (1.0 - have as f32 / normal as f32)).clamp(0.85, 1.3)
    };
    // В лавках за стенами своё отдают дешевле, а «городское» мерило не действует.
    let margin = if def.barter() { 1.0 } else { 1.15 };
    round_price(
        base_price(base) as f32
            * quality_price(quality)
            * markup
            * scarcity
            * margin
            * standing.sell_markup(),
    )
}

/// Цена одной штуки, по которой лавка купит у игрока. `None` — такое не берут.
/// `held` — сколько такого товара уже лежит у торговца (насыщение рынка).
pub fn bid_price(
    def: &MerchantDef,
    base: ItemBaseId,
    quality: Quality,
    standing: Standing,
    held: u16,
    stolen: bool,
) -> Option<u32> {
    let cat = category(base);
    if cat == Category::Coin {
        // Монеты — не товар, кроме мены за стенами.
        let MoneyPolicy::Barter { bronze, gold } = def.money else {
            return None;
        };
        let rate = if is_gold_coin(base) { gold } else { bronze };
        return (rate > 0.0).then(|| (coin_value(base).unwrap_or(0) as f32 * rate).round() as u32);
    }
    if stolen && def.has(Service::Fence) {
        // Скупщик берёт краденое за бесценок, зато не спрашивает.
        return Some(round_price(
            base_price(base) as f32 * quality_price(quality) * 0.4,
        ));
    }
    let want = def.buys.iter().find(|(c, _)| *c == cat).map(|(_, w)| *w)?;
    let margin = if def.barter() { 0.75 } else { 0.5 };
    let saturation = 1.0 / (1.0 + held as f32 / 12.0);
    Some(round_price(
        base_price(base) as f32
            * quality_price(quality)
            * want
            * margin
            * saturation.max(0.4)
            * standing.buy_bonus(),
    ))
}

// ---------------------------------------------------------------------------
// Прилавок
// ---------------------------------------------------------------------------

/// Столешница прилавка в системе торговца (он смотрит в −Z): центр и половинные размеры.
/// `felt` — войлок на земле (за стенами), иначе деревянный стол.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct StallBox {
    pub center: [f32; 3],
    pub half: [f32; 3],
    pub felt: bool,
}

/// Из чего состоит прилавок: в городе — стол перед торговцем (и боковой столик, если есть место),
/// за стенами — войлок на земле.
pub fn stall_layout(outside: bool, side_table: bool) -> Vec<StallBox> {
    if outside {
        return vec![StallBox { center: [0.0, 0.015, -1.15], half: [1.2, 0.015, 0.75], felt: true }];
    }
    let mut v = vec![StallBox { center: [0.0, 0.45, -0.95], half: [1.0, 0.45, 0.35], felt: false }];
    if side_table {
        v.push(StallBox { center: [1.65, 0.38, -0.6], half: [0.45, 0.38, 0.4], felt: false });
    }
    v
}

/// Столешницы прилавка в мире.
pub fn stall_obbs(pos: bevy::math::Vec3, yaw: f32, boxes: &[StallBox]) -> Vec<crate::props::Obb> {
    let rot = bevy::math::Quat::from_rotation_y(yaw);
    boxes
        .iter()
        .map(|b| crate::props::Obb { center: pos + rot * bevy::math::Vec3::from_array(b.center), rot, half: bevy::math::Vec3::from_array(b.half) })
        .collect()
}

/// Продажа оружия и доспехов варварам запрещена законом.
pub fn is_contraband_for(def: &MerchantDef, base: ItemBaseId) -> bool {
    def.faction.outside_walls() && matches!(category(base), Category::Weapon | Category::Armor)
}

// ---------------------------------------------------------------------------
// Кража
// ---------------------------------------------------------------------------

/// Чем занят торговец (с сервера, видно игроку).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attention {
    /// Смотрит на прилавок и на покупателя.
    Watching,
    /// Отвлёкся: номер подсказки в [`DISTRACTIONS`].
    Distracted(u8),
}

pub const DISTRACTIONS: [&str; 5] = [
    "отвернулся к полкам",
    "пересчитывает выручку",
    "торгуется с другим покупателем",
    "ушёл в глубь лавки за товаром",
    "кричит на подмастерья",
];

impl Attention {
    pub fn describe(self) -> String {
        match self {
            Attention::Watching => "смотрит на вас".to_string(),
            Attention::Distracted(i) => DISTRACTIONS[i as usize % DISTRACTIONS.len()].to_string(),
        }
    }
}

/// Условия попытки кражи.
pub struct TheftAttempt {
    pub vigilance: f32,
    pub attention: Attention,
    /// Сколько клеток занимает вещь (большое под одеждой не спрячешь).
    pub cells: u8,
    pub crouching: bool,
    /// Ловкость из экипировки.
    pub agility: f32,
    /// Сколько раз недавно уже крали у этого торговца (он настороже).
    pub recent_thefts: u32,
    /// Рядом стража.
    pub guards_near: bool,
}

/// Самая крупная вещь, которую можно стянуть с прилавка (клеток).
pub const MAX_STEAL_CELLS: u8 = 2;

/// Вероятность попасться (0..1).
pub fn theft_detection(a: &TheftAttempt) -> f32 {
    let attention = match a.attention {
        Attention::Watching => 1.0,
        Attention::Distracted(_) => 0.25,
    };
    let size = if a.cells <= 1 { 0.8 } else { 1.15 };
    // Присевший у чужого прилавка не незаметен, но руку видно хуже.
    let crouch = if a.crouching { 0.85 } else { 1.0 };
    let agility = (1.0 - a.agility * 0.03).clamp(0.6, 1.0);
    let wary = 0.15 * a.recent_thefts as f32;
    let guards = if a.guards_near { 0.15 } else { 0.0 };
    (a.vigilance * attention * size * crouch * agility + wary + guards).clamp(0.05, 0.95)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ITEM_BASES;

    #[test]
    fn merchants_are_consistent() {
        let mut keys = std::collections::HashSet::new();
        for m in MERCHANTS {
            assert!(keys.insert(m.key), "повтор ключа {}", m.key);
            for st in m.stock {
                assert!(st.min <= st.max, "{}: {}", m.key, st.item);
            }
            if m.barter() {
                assert!(m.orders.is_empty(), "за стенами заказов нет");
            }
        }
    }

    #[test]
    fn own_goods_exist() {
        // Товары этого модуля уже в игре; чужие (алхимия) появятся позже, их пропускаем.
        for name in [
            "Амфора вина",
            "Солёная рыба",
            "Овчина",
            "Войлок",
            "Фоллис",
            "Солид",
        ] {
            assert!(ItemBaseId::find(name).is_some(), "{name}");
        }
    }

    #[test]
    fn categories_cover_everything() {
        for (i, b) in ITEM_BASES.iter().enumerate() {
            let c = category(ItemBaseId(i as u16));
            if b.slot == Some(crate::items::EquipSlot::MainHand) {
                assert_eq!(c, Category::Weapon, "{}", b.name);
            }
        }
        assert_eq!(
            category(ItemBaseId::by_name("Амфора вина")),
            Category::WineOil
        );
        assert_eq!(category(ItemBaseId::by_name("Фоллис")), Category::Coin);
    }

    #[test]
    fn merchant_makes_profit() {
        let m = merchant_by_key("eutychios").unwrap().def();
        let wine = ItemBaseId::by_name("Амфора вина");
        let andronikos = merchant_by_key("andronikos").unwrap().def();
        let fish = ItemBaseId::by_name("Солёная рыба");
        let ask = ask_price(andronikos, fish, Quality::Common, Standing::Neutral, 6, 6);
        let bid = bid_price(
            andronikos,
            fish,
            Quality::Common,
            Standing::Neutral,
            0,
            false,
        )
        .unwrap();
        assert!(ask > bid, "перекупщик не должен терять: {ask} > {bid}");
        // Купец из Синопы рыбу берёт охотнее лавочника — её везут на юг.
        let export = bid_price(m, fish, Quality::Common, Standing::Neutral, 0, false).unwrap();
        assert!(export > bid);
        // Вино он не покупает дороже, чем продаёт.
        let wine_ask = ask_price(m, wine, Quality::Common, Standing::Neutral, 4, 4);
        assert!(
            bid_price(m, wine, Quality::Common, Standing::Neutral, 0, false).unwrap() < wine_ask
        );
    }

    #[test]
    fn steppe_pays_more_for_wine_than_port_sells() {
        // Ради этого и возят вино в степь.
        let port = merchant_by_key("eutychios").unwrap().def();
        let steppe = merchant_by_key("eskam").unwrap().def();
        let wine = ItemBaseId::by_name("Амфора вина");
        let buy = ask_price(port, wine, Quality::Common, Standing::Neutral, 4, 4);
        let sell = bid_price(steppe, wine, Quality::Common, Standing::Neutral, 0, false).unwrap();
        assert!(sell > buy, "в степи {sell}, в порту {buy}");
    }

    #[test]
    fn steppe_route_pays_off() {
        // Вино в степь, ревень обратно сирийцу: круг должен приносить доход.
        let port = merchant_by_key("eutychios").unwrap().def();
        let steppe = merchant_by_key("eskam").unwrap().def();
        let syrian = merchant_by_key("abbakum").unwrap().def();
        let (wine, rhubarb) = (
            ItemBaseId::by_name("Амфора вина"),
            ItemBaseId::find("Ревень понтийский"),
        );
        let Some(rhubarb) = rhubarb else { return };
        let spent = ask_price(port, wine, Quality::Common, Standing::Neutral, 4, 4) as f32;
        let credit =
            bid_price(steppe, wine, Quality::Common, Standing::Neutral, 0, false).unwrap() as f32;
        let got =
            credit / ask_price(steppe, rhubarb, Quality::Common, Standing::Neutral, 3, 3) as f32;
        let back = got
            * bid_price(
                syrian,
                rhubarb,
                Quality::Common,
                Standing::Neutral,
                0,
                false,
            )
            .unwrap() as f32;
        assert!(back > spent * 1.15, "потрачено {spent}, выручено {back}");
    }

    #[test]
    fn nomads_refuse_bronze() {
        let steppe = merchant_by_key("eskam").unwrap().def();
        assert_eq!(
            bid_price(
                steppe,
                ItemBaseId::by_name("Фоллис"),
                Quality::Common,
                Standing::Neutral,
                0,
                false
            ),
            None
        );
        assert!(
            bid_price(
                steppe,
                ItemBaseId::by_name("Солид"),
                Quality::Common,
                Standing::Neutral,
                0,
                false
            )
            .unwrap()
                > 7200
        );
        let city = merchant_by_key("andronikos").unwrap().def();
        assert_eq!(
            bid_price(
                city,
                ItemBaseId::by_name("Солид"),
                Quality::Common,
                Standing::Neutral,
                0,
                false
            ),
            None
        );
    }

    #[test]
    fn reputation_changes_prices() {
        let m = merchant_by_key("andronikos").unwrap().def();
        let b = ItemBaseId::by_name("Бинт");
        assert!(
            ask_price(m, b, Quality::Common, Standing::Honored, 5, 5)
                < ask_price(m, b, Quality::Common, Standing::Unfriendly, 5, 5)
        );
    }

    #[test]
    fn theft_is_easier_when_distracted() {
        let mut a = TheftAttempt {
            vigilance: 0.6,
            attention: Attention::Watching,
            cells: 1,
            crouching: false,
            agility: 0.0,
            recent_thefts: 0,
            guards_near: false,
        };
        let watched = theft_detection(&a);
        a.attention = Attention::Distracted(0);
        let distracted = theft_detection(&a);
        assert!(distracted < watched * 0.5);
        a.recent_thefts = 3;
        assert!(theft_detection(&a) > distracted);
    }

    #[test]
    fn arms_to_barbarians_is_contraband() {
        let steppe = merchant_by_key("eskam").unwrap().def();
        assert!(is_contraband_for(
            steppe,
            ItemBaseId::by_name("Спата")
        ));
        assert!(!is_contraband_for(
            steppe,
            ItemBaseId::by_name("Амфора вина")
        ));
        let garrison = merchant_by_key("markianos").unwrap().def();
        assert!(!is_contraband_for(
            garrison,
            ItemBaseId::by_name("Спата")
        ));
    }
}
