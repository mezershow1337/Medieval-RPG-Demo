//! Варка: травы, вода, огонь и время. Честная модель без «рецептов из воздуха».
//!
//! Игрок кладёт в сосуд вещества (каждый предмет весит сколько-то драхм), наливает воду
//! (в котилах), выбирает огонь и время. Сервер смотрит, на какое снадобье похожа смесь,
//! и насколько точно соблюдены пропорции древних врачей. Знание рецепта не нужно, чтобы
//! получилось: тот, кто нашёл настоящий рецепт (Гален: воск к маслу как 1 к 4), сварит
//! и без уроков. Знание даёт подсказки на экране, а навык — точность (качество).
//!
//! Времена сжаты: час варки — несколько секунд, сутки брожения — меньше минуты.
//!
//! Три этапа, как у Зосимы и врачей:
//! 1. Подготовка сырья (холодная): травы сушат на решётке в тени (или у слабого огня),
//!    корни, смолы и минералы толкут в ступке, сок отжимают через лён. Свежая трава для
//!    отвара слабее сушёной, неистолчённый корень отдаёт мало, сушёную траву на рану не кладут.
//! 2. Варка и томление: основа решает, что выйдет — вода (отвар), вино (настойка), уксус
//!    (вытяжка), масло и жир (мазь); водяная баня Марии не даёт пригореть. Огонь во время
//!    работы гуляет: его раздувают мехами и прикрывают заслонкой ([`FireLog`]); пережжённые
//!    травы дают одну горечь.
//! 3. Перегонка: купол алембика греется — его остужают мокрой тряпицей, иначе пар рвёт стекло.

use crate::lore::{Knowledge, Lore, Technique};
use crate::medicine::Toxin;
use serde::{Deserialize, Serialize};

/// Котила (κοτύλη) — около четверти литра; по весу воды примерно 80 драхм.
pub const DRACHMS_PER_KOTYLE: f32 = 80.0;
/// Секунд реального времени на час варки и предел для долгих занятий.
pub const SECS_PER_HOUR: f32 = 3.0;
pub const MAX_SECS: f32 = 45.0;
/// Сколько воды можно налить в горшок.
pub const MAX_WATER: u8 = 12;

/// Готовое варево — это порча.
pub const SPOILED: &str = "Испорченное варево";
pub const FLASK: &str = "Пустой флакон";
/// Лён: процедить, отжать сок, остудить купол мокрой тряпицей.
pub const CLOTH: &str = "Отрез льна";

/// В каком виде сырьё.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    /// Как собрали или купили (свежая трава, целый корень, кусок смолы).
    Raw,
    /// Высушено на решётке.
    Dried,
    /// Истолчено в ступке.
    Ground,
}

/// Сушёные травы: (что получилось, из чего).
pub const DRIED: &[(&str, &str)] = &[
    ("Сушёный шалфей", "Шалфей"),
    ("Сушёный чабрец", "Чабрец"),
    ("Сушёный зверобой", "Зверобой"),
    ("Сушёный подорожник", "Подорожник"),
    ("Сушёный тысячелистник", "Тысячелистник"),
    ("Сушёный просвирник", "Просвирник"),
    ("Сушёная мята", "Мята"),
    ("Сушёная полынь", "Полынь"),
    ("Сушёный шиповник", "Шиповник"),
];

/// Толчёное в ступке: (что получилось, из чего).
pub const GROUND: &[(&str, &str)] = &[
    ("Толчёный корень девясила", "Корень девясила"),
    ("Толчёный корень окопника", "Корень окопника"),
    ("Толчёный корень марены", "Корень марены"),
    ("Толчёный солодковый корень", "Солодковый корень"),
    ("Толчёная мирра", "Мирра"),
    ("Толчёный ладан", "Ладан"),
    ("Толчёные галлы", "Дубовые галлы"),
    ("Толчёная сера", "Сера"),
    ("Толчёная киноварь", "Киноварь"),
];

/// Какое это сырьё на самом деле и в каком виде: «Сушёная мята» → («Мята», Dried).
pub fn canonical(name: &str) -> (&str, Form) {
    if let Some((_, raw)) = DRIED.iter().find(|(d, _)| *d == name) {
        return (raw, Form::Dried);
    }
    if let Some((_, raw)) = GROUND.iter().find(|(g, _)| *g == name) {
        return (raw, Form::Ground);
    }
    (name, Form::Raw)
}

/// Листья и цветы, которые сушат.
fn leafy(raw: &str) -> bool {
    DRIED.iter().any(|(_, r)| *r == raw)
}

/// Корни, смолы и минералы, которые толкут.
fn grindable(raw: &str) -> bool {
    GROUND.iter().any(|(_, r)| *r == raw)
}

/// Сколько сырья по рецепту в этом предмете (драхмы свежего/целого): пучок сушёной травы —
/// та же горсть, только без воды.
pub fn equiv_mass(name: &str) -> Option<f32> {
    let (raw, form) = canonical(name);
    if form == Form::Raw {
        mass(name)
    } else {
        mass(raw)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Heat {
    /// Без огня: настой, брожение, растирание.
    None,
    /// Слабый: томить, водяная баня.
    Low,
    /// Средний: кипеть.
    Medium,
    /// Сильный: бурно кипеть, топить жир.
    High,
}

impl Heat {
    pub const ALL: [Heat; 4] = [Heat::None, Heat::Low, Heat::Medium, Heat::High];

    pub fn name(self) -> &'static str {
        match self {
            Heat::None => "Без огня",
            Heat::Low => "Слабый",
            Heat::Medium => "Средний",
            Heat::High => "Сильный",
        }
    }

    /// Сколько котил воды выкипает за час.
    fn boil_off(self) -> f32 {
        match self {
            Heat::None => 0.0,
            Heat::Low => 0.6,
            Heat::Medium => 1.6,
            Heat::High => 3.0,
        }
    }
}

/// Сосуд.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vessel {
    /// Глиняный горшок: дёшев, кислоту терпит, но лопается на сильном огне всухую.
    ClayPot,
    /// Медный котелок: прочен, но уксус и вино разъедают медь — выйдет ядовитая зелень.
    CopperKettle,
    /// Ступка с пестиком: растирать, без огня.
    Mortar,
    /// Алембик (кукурбита с амбиком): перегонка. Стеклянный — лопается от рывка жара.
    Alembic,
    /// Стеклянная реторта: перегонка, ещё капризнее к сильному огню.
    Retort,
    /// Керотакис Зосима: закрытый прибор для возгонки — паров наружу выходит меньше.
    Kerotakis,
    /// Тигель: обжиг на сильном огне.
    Crucible,
    /// Плетёная решётка: сушить травы в тени или у слабого огня.
    DryingTray,
}

impl Vessel {
    pub fn from_item(name: &str) -> Option<Vessel> {
        match name {
            "Глиняный горшок" => Some(Vessel::ClayPot),
            "Медный котелок" => Some(Vessel::CopperKettle),
            "Ступка с пестиком" => Some(Vessel::Mortar),
            "Алембик" => Some(Vessel::Alembic),
            "Стеклянная реторта" => Some(Vessel::Retort),
            "Керотакис" => Some(Vessel::Kerotakis),
            "Тигель" => Some(Vessel::Crucible),
            "Сушильная решётка" => Some(Vessel::DryingTray),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Vessel::ClayPot => "Глиняный горшок",
            Vessel::CopperKettle => "Медный котелок",
            Vessel::Mortar => "Ступка с пестиком",
            Vessel::Alembic => "Алембик",
            Vessel::Retort => "Стеклянная реторта",
            Vessel::Kerotakis => "Керотакис",
            Vessel::Crucible => "Тигель",
            Vessel::DryingTray => "Сушильная решётка",
        }
    }

    /// Стекло: лопается от сильного огня.
    pub fn glass(self) -> bool {
        matches!(self, Vessel::Alembic | Vessel::Retort)
    }

    /// Перегонный прибор (воды не льют, пары собираются в приёмнике).
    pub fn still(self) -> bool {
        matches!(self, Vessel::Alembic | Vessel::Retort | Vessel::Kerotakis)
    }

    /// Нужна ли вода в этом сосуде вообще.
    pub fn takes_water(self) -> bool {
        matches!(self, Vessel::ClayPot | Vessel::CopperKettle)
    }
}

/// Вес одного предмета в драхмах (драхма ≈ 3,4 г). None — в сосуд не кладут.
pub fn mass(item: &str) -> Option<f32> {
    match canonical(item) {
        // Свежая горсть на три четверти — вода.
        (raw, Form::Dried) => return mass(raw).map(|m| m * 0.25),
        (raw, Form::Ground) => return mass(raw),
        _ => {}
    }
    Some(match item {
        "Маковый сок" => 2.0,
        // Свежая трава: горсть.
        "Шалфей"
        | "Чабрец"
        | "Зверобой"
        | "Подорожник"
        | "Тысячелистник"
        | "Просвирник"
        | "Мята"
        | "Полынь"
        | "Белена"
        | "Вайда"
        | "Шиповник" => 8.0,
        "Маковые коробочки" | "Ягоды можжевельника" | "Дубовые галлы" => {
            6.0
        }
        "Корень марены"
        | "Корень окопника"
        | "Корень девясила"
        | "Солодковый корень"
        | "Ревень понтийский" => 10.0,
        // Привозное — кусочки и щепоти.
        "Мирра" | "Ладан" | "Камедь" | "Медный купорос" => 4.0,
        "Хиосская мастика" | "Корица" => 3.0,
        "Перец" | "Опий" => 2.0,
        "Шафран" => 1.0,
        "Квасцы" | "Натр" => 6.0,
        "Сера" => 8.0,
        // Жиры, мёд, жидкости.
        "Воск" => 25.0,
        "Бараний жир" | "Древесная зола" => 30.0,
        "Горшок мёда" => 60.0,
        "Кувшин оливкового масла" => 100.0,
        "Кувшин вина" => 120.0,
        "Винный уксус" | "Щёлок" => 20.0,
        // Алхимия и краски.
        "Сосновая смола" | "Негашёная известь" | "Медный лом" => {
            15.0
        }
        "Киноварь" | "Свинцовые белила" => 8.0,
        "Свинец" => 20.0,
        "Нефть" => 30.0,
        _ => return None,
    })
}

/// Вещество в рецепте: одно из имён и доля по весу (без воды).
struct Part {
    any_of: &'static [&'static str],
    share: f32,
    tol: f32,
    /// Без него рецепт всё равно узнаётся, но хуже.
    optional: bool,
}

const fn part(any_of: &'static [&'static str], share: f32, tol: f32) -> Part {
    Part {
        any_of,
        share,
        tol,
        optional: false,
    }
}

const fn opt(any_of: &'static [&'static str], share: f32, tol: f32) -> Part {
    Part {
        any_of,
        share,
        tol,
        optional: true,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum VesselNeed {
    Pot,
    Mortar,
    /// Алембик, реторта или керотакис.
    Still,
    Crucible,
    /// Сушильная решётка.
    Tray,
}

/// Как делают снадобье (по Диоскориду, Галену, Орибасию; доли округлены).
pub struct Recipe {
    pub product: &'static str,
    pub technique: Technique,
    vessel: VesselNeed,
    parts: &'static [Part],
    /// Вода по весу к сухому, (идеал, допуск). (0, 0) — воды быть не должно.
    water: (f32, f32),
    heat: &'static [Heat],
    /// Сколько воды должно выкипеть (доля), (идеал, допуск). Только если есть вода и огонь.
    reduce: (f32, f32),
    /// Часы: (меньше нельзя, лучше всего, больше — портится; 0 — не портится).
    hours: (f32, f32, f32),
    /// Сколько выходит: на котилу оставшейся жидкости или на предмет основы.
    yield_per: Yield,
    /// Кислое: в меди даёт ядовитую зелень.
    acidic: bool,
    /// Подсказка для знающего приём (то, что написано в книгах).
    pub hint: &'static str,
    /// Ядовитые пары при работе (доз), если не в закрытом приборе.
    fumes: Option<(Toxin, f32)>,
    /// Готовое — сам сосуд (зажигательный горшок).
    consumes_vessel: bool,
    /// С этого огня смесь вспыхивает.
    ignites: Option<Heat>,
}

#[derive(Clone, Copy, Debug)]
enum Yield {
    /// Флаконов на котилу оставшейся жидкости.
    PerKotyle(f32),
    /// Штук на столько-то драхм смеси.
    PerDrachms(f32),
}

impl Recipe {
    /// Жидкое — разливают во флаконы.
    pub fn bottled(&self) -> bool {
        !is_grinding(self)
            && self.vessel != VesselNeed::Tray
            && !matches!(
                self.product,
                "Кровоостанавливающая припарка"
                    | "Мыло"
                    | "Жаровня"
                    | "Ярь-медянка"
                    | "Свинцовые белила"
                    | "Сурик"
                    | "Мареновый лак"
                    | "Вайдовая синь"
                    | "Зажигательный горшок"
            )
    }
}

const HERB_DECOCTION_HOURS: (f32, f32, f32) = (0.5, 1.0, 3.0);

/// Сушка на решётке: горсть свежей травы — пучок сушёной.
macro_rules! dry {
    ($product:literal, $raw:literal) => {
        Recipe {
            product: $product,
            technique: Technique::Gathering,
            vessel: VesselNeed::Tray,
            parts: &[part(&[$raw], 1.0, 0.0)],
            water: (0.0, 0.0),
            heat: &[Heat::None, Heat::Low],
            reduce: (0.0, 1.0),
            hours: (8.0, 12.0, 0.0),
            yield_per: Yield::PerDrachms(8.0),
            acidic: false,
            hint: "Сушить на решётке в тени полсуток или у слабого огня часа три-четыре. На сильном огне трава сгорит.",
            fumes: None,
            consumes_vessel: false,
            ignites: None,
        }
    };
}

/// Толчение в ступке: корни, смолы, минералы — в порошок.
macro_rules! grind {
    ($product:literal, $raw:literal, $mass:literal) => {
        Recipe {
            product: $product,
            technique: Technique::Dosage,
            vessel: VesselNeed::Mortar,
            parts: &[part(&[$raw], 1.0, 0.0)],
            water: (0.0, 0.0),
            heat: &[Heat::None],
            reduce: (0.0, 1.0),
            hours: (0.2, 0.5, 0.0),
            yield_per: Yield::PerDrachms($mass),
            acidic: false,
            hint: "Корни, смолы и минералы толочь в ступке в мелкий порошок: чем мельче, тем полнее отдают при варке.",
            fumes: None,
            consumes_vessel: false,
            ignites: None,
        }
    };
}

pub static RECIPES: &[Recipe] = &[
    Recipe {
        product: "Отвар шалфея",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Шалфей"], 1.0, 0.0)],
        water: (20.0, 10.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.6, 0.25),
        hours: HERB_DECOCTION_HOURS,
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Отвар: горсть травы на две котилы воды, томить на слабом огне около часа, уварить наполовину-на две трети.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Отвар чабреца",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Чабрец"], 1.0, 0.0)],
        water: (20.0, 10.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.6, 0.25),
        hours: HERB_DECOCTION_HOURS,
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Чабрец варят так же, как шалфей.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Маковый отвар",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Маковые коробочки"], 1.0, 0.0)],
        water: (15.0, 8.0),
        heat: &[Heat::Low],
        reduce: (0.5, 0.25),
        hours: (1.0, 2.0, 4.0),
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Маковые головки томить на слабом огне часа два; чем гуще, тем сильнее — а значит, опаснее.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Оксимель",
        technique: Technique::Oxymel,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Горшок мёда"], 0.75, 0.12),
            part(&["Винный уксус"], 0.25, 0.12),
        ],
        water: (1.0, 1.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.5, 0.4),
        hours: (0.5, 1.0, 3.0),
        yield_per: Yield::PerDrachms(30.0),
        acidic: true,
        hint: "Оксимель: мёда втрое больше, чем уксуса, немного воды, уварить до сиропа. Только в глине — уксус ест медь.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Зверобойное масло",
        technique: Technique::Maceration,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Зверобой"], 0.14, 0.08),
            part(&["Кувшин оливкового масла"], 0.86, 0.08),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::None, Heat::Low],
        reduce: (0.0, 1.0),
        hours: (2.0, 6.0, 0.0),
        yield_per: Yield::PerDrachms(50.0),
        acidic: false,
        hint: "Цветы зверобоя в оливковом масле: две горсти на кувшин, томить на водяной бане несколько часов, пока масло не покраснеет. Воды не лить.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Ранозаживляющая мазь",
        technique: Technique::Salve,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Воск"], 0.2, 0.07),
            part(&["Кувшин оливкового масла", "Бараний жир"], 0.77, 0.1),
            opt(&["Мирра", "Ладан"], 0.03, 0.03),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::Low],
        reduce: (0.0, 1.0),
        hours: (0.3, 1.0, 3.0),
        yield_per: Yield::PerDrachms(40.0),
        acidic: false,
        hint: "Керат Галена: воск и масло как один к четырём, растопить на слабом огне; для ран добавить щепоть мирры или ладана.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Кровоостанавливающая припарка",
        technique: Technique::Poultice,
        vessel: VesselNeed::Mortar,
        parts: &[
            opt(&["Тысячелистник"], 0.5, 0.35),
            opt(&["Подорожник"], 0.5, 0.35),
            opt(&["Квасцы"], 0.08, 0.08),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (0.1, 0.25, 0.0),
        yield_per: Yield::PerDrachms(8.0),
        acidic: false,
        hint: "Растереть в ступке тысячелистник с подорожником, можно с щепотью квасцов.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Винный уксус",
        technique: Technique::Fermentation,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Кувшин вина"], 1.0, 0.0)],
        water: (0.0, 0.3),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (24.0, 72.0, 0.0),
        yield_per: Yield::PerDrachms(20.0),
        acidic: true,
        hint: "Вино в открытом горшке в тепле, без огня; через несколько дней скиснет в уксус.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Щёлок",
        technique: Technique::Soap,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Древесная зола"], 1.0, 0.0)],
        water: (3.0, 2.0),
        heat: &[Heat::None, Heat::Low],
        reduce: (0.0, 1.0),
        hours: (2.0, 6.0, 0.0),
        yield_per: Yield::PerKotyle(1.0),
        acidic: false,
        hint: "Золу залить водой (по весу втрое больше), настоять полдня и слить щёлок.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Мыло",
        technique: Technique::Soap,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Щёлок"], 0.5, 0.2),
            part(&["Бараний жир", "Кувшин оливкового масла"], 0.5, 0.2),
        ],
        water: (0.0, 0.5),
        heat: &[Heat::Medium, Heat::High],
        reduce: (0.0, 1.0),
        hours: (1.0, 3.0, 8.0),
        yield_per: Yield::PerDrachms(25.0),
        acidic: false,
        hint: "Щёлок с жиром поровну по весу, варить на сильном огне, пока не загустеет.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Чернила",
        technique: Technique::Ink,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Дубовые галлы"], 0.55, 0.15),
            part(&["Медный купорос"], 0.2, 0.1),
            part(&["Камедь"], 0.25, 0.12),
        ],
        water: (3.0, 2.0),
        heat: &[Heat::None, Heat::Low],
        reduce: (0.0, 1.0),
        hours: (1.0, 6.0, 0.0),
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Толчёные галлы настоять в воде, добавить купорос и камедь: галлов вдвое больше камеди.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Териак",
        technique: Technique::Theriac,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Опий"], 0.05, 0.03),
            part(&["Мирра"], 0.05, 0.04),
            part(&["Ягоды можжевельника"], 0.07, 0.05),
            part(&["Корица"], 0.04, 0.03),
            opt(&["Шафран"], 0.015, 0.015),
            part(&["Перец"], 0.03, 0.025),
            part(&["Горшок мёда"], 0.72, 0.12),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::Low],
        reduce: (0.0, 1.0),
        hours: (4.0, 12.0, 0.0),
        yield_per: Yield::PerDrachms(20.0),
        acidic: false,
        hint: "Териак Андромаха по Галену: опий, мирра, можжевельник, корица, перец и шафран растереть и замесить на мёде (мёда втрое больше по весу, чем порошков), долго томить.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    // ----- Краски и пигменты (Феофраст, Плиний, папирусы красильщиков) -----
    Recipe {
        product: "Ярь-медянка",
        technique: Technique::Fermentation,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Медный лом"], 0.45, 0.25),
            part(&["Винный уксус"], 0.55, 0.25),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (48.0, 72.0, 0.0),
        yield_per: Yield::PerDrachms(12.0),
        acidic: true,
        hint: "Медные пластины над крепким уксусом в закрытом горшке, без огня; через несколько дней соскрести зелень.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Свинцовые белила",
        technique: Technique::Calcination,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Свинец"], 0.5, 0.25),
            part(&["Винный уксус"], 0.5, 0.25),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (48.0, 72.0, 0.0),
        yield_per: Yield::PerDrachms(12.0),
        acidic: true,
        hint: "Свинец над уксусом в закрытом горшке, в тепле без огня; белый налёт соскрести — это белила.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Сурик",
        technique: Technique::Calcination,
        vessel: VesselNeed::Crucible,
        parts: &[part(&["Свинцовые белила"], 1.0, 0.0)],
        water: (0.0, 0.0),
        heat: &[Heat::High],
        reduce: (0.0, 1.0),
        hours: (2.0, 4.0, 10.0),
        yield_per: Yield::PerDrachms(8.0),
        acidic: false,
        hint: "Белила, обожжённые в тигле на сильном огне, краснеют и становятся суриком.",
        fumes: Some((Toxin::Lead, 0.25)),
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Мареновый лак",
        technique: Technique::Mordanting,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Корень марены"], 0.6, 0.2),
            part(&["Квасцы"], 0.28, 0.15),
            opt(&["Натр", "Щёлок"], 0.12, 0.1),
        ],
        water: (4.0, 3.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.0, 1.0),
        hours: (1.0, 2.0, 5.0),
        yield_per: Yield::PerDrachms(10.0),
        acidic: false,
        hint: "Толчёный корень марены варить с квасцами, добавить натра: красная краска осядет на дно.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Вайдовая синь",
        technique: Technique::VatDyeing,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Вайда"], 0.7, 0.2),
            part(&["Щёлок", "Древесная зола"], 0.3, 0.2),
        ],
        water: (4.0, 3.0),
        heat: &[Heat::Low],
        reduce: (0.0, 1.0),
        hours: (48.0, 96.0, 200.0),
        yield_per: Yield::PerDrachms(10.0),
        acidic: false,
        hint: "Листья вайды в щелочном чане держать в тепле несколько дней, пока не забродят и не пойдёт синь.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    // ----- Перегонка и возгонка (Диоскорид, Зосим) -----
    Recipe {
        product: "Серная вода",
        technique: Technique::Distillation,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Сера"], 0.4, 0.2),
            part(&["Негашёная известь"], 0.6, 0.2),
        ],
        water: (10.0, 7.0),
        heat: &[Heat::Medium],
        reduce: (0.4, 0.3),
        hours: (1.0, 2.0, 4.0),
        yield_per: Yield::PerKotyle(1.0),
        acidic: false,
        hint: "«Божественная вода» Зосима: толчёную серу с известью кипятить в воде, пока не станет красно-жёлтой. Смердит тухлыми яйцами — работать на ветру.",
        fumes: Some((Toxin::Caustic, 0.3)),
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Смоляное масло",
        technique: Technique::Distillation,
        vessel: VesselNeed::Still,
        parts: &[part(&["Сосновая смола"], 1.0, 0.0)],
        water: (0.0, 0.0),
        heat: &[Heat::Medium],
        reduce: (0.0, 1.0),
        hours: (1.0, 2.0, 5.0),
        yield_per: Yield::PerDrachms(15.0),
        acidic: false,
        hint: "Смолу греть ровным средним огнём под амбиком: пары стекают в приёмник маслом.",
        fumes: None,
        consumes_vessel: false,
        ignites: Some(Heat::High),
    },
    Recipe {
        product: "Ртуть",
        technique: Technique::Sublimation,
        vessel: VesselNeed::Still,
        parts: &[part(&["Киноварь"], 1.0, 0.0)],
        water: (0.0, 0.0),
        heat: &[Heat::Medium, Heat::High],
        reduce: (0.0, 1.0),
        hours: (1.0, 2.0, 6.0),
        yield_per: Yield::PerDrachms(10.0),
        acidic: false,
        hint: "Диоскорид: киноварь в железной ложке под амбиком, обмазанным глиной, угли снизу; осевшую на крышке сажу соскрести — это ртуть. Пары смертельны.",
        fumes: Some((Toxin::Mercury, 0.9)),
        consumes_vessel: false,
        ignites: None,
    },
    // ----- Огонь (Эней Тактик) -----
    Recipe {
        product: "Зажигательный горшок",
        technique: Technique::Incendiary,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Сосновая смола"], 0.6, 0.25),
            part(&["Сера"], 0.18, 0.12),
            opt(&["Ладан"], 0.08, 0.08),
            opt(&["Нефть"], 0.3, 0.3),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::Low],
        reduce: (0.0, 1.0),
        hours: (0.3, 0.5, 2.0),
        yield_per: Yield::PerDrachms(1000.0),
        acidic: false,
        hint: "Эней Тактик: смолу, серу и толчёный ладан растопить на слабом огне прямо в горшке, добавить нефти; огонь сильнее слабого — вспыхнет.",
        fumes: None,
        consumes_vessel: true,
        ignites: Some(Heat::Medium),
    },
    Recipe {
        product: "Отвар полыни",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Полынь"], 1.0, 0.0)],
        water: (20.0, 10.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.6, 0.25),
        hours: HERB_DECOCTION_HOURS,
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Полынь варят как прочие отвары; горька, но гонит яд и глистов.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Мятный отвар",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Мята"], 1.0, 0.0)],
        water: (20.0, 10.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.6, 0.25),
        hours: HERB_DECOCTION_HOURS,
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Мяту томить недолго, на слабом огне: унимает тошноту.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Отвар шиповника",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Шиповник"], 1.0, 0.0)],
        water: (20.0, 10.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.6, 0.25),
        hours: HERB_DECOCTION_HOURS,
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Плоды шиповника варить около часа: укрепляет и бодрит.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Отвар девясила",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Корень девясила"], 1.0, 0.0)],
        water: (15.0, 8.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.6, 0.25),
        hours: HERB_DECOCTION_HOURS,
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Корень девясила варить дольше травы: от кашля и жара.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Жаровня",
        technique: Technique::Calcination,
        vessel: VesselNeed::Crucible,
        parts: &[part(&["Медный лом"], 1.0, 0.0)],
        water: (0.0, 0.0),
        heat: &[Heat::High],
        reduce: (0.0, 1.0),
        hours: (2.0, 3.0, 8.0),
        yield_per: Yield::PerDrachms(60.0),
        acidic: false,
        hint: "Медный лом расплавить в тигле на сильном огне и отлить чашу жаровни (на неё — четыре куска лома).",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    // --- Подготовка сырья ---
    dry!("Сушёный шалфей", "Шалфей"),
    dry!("Сушёный чабрец", "Чабрец"),
    dry!("Сушёный зверобой", "Зверобой"),
    dry!("Сушёный подорожник", "Подорожник"),
    dry!("Сушёный тысячелистник", "Тысячелистник"),
    dry!("Сушёный просвирник", "Просвирник"),
    dry!("Сушёная мята", "Мята"),
    dry!("Сушёная полынь", "Полынь"),
    dry!("Сушёный шиповник", "Шиповник"),
    grind!("Толчёный корень девясила", "Корень девясила", 10.0),
    grind!("Толчёный корень окопника", "Корень окопника", 10.0),
    grind!("Толчёный корень марены", "Корень марены", 10.0),
    grind!("Толчёный солодковый корень", "Солодковый корень", 10.0),
    grind!("Толчёная мирра", "Мирра", 4.0),
    grind!("Толчёный ладан", "Ладан", 4.0),
    grind!("Толчёные галлы", "Дубовые галлы", 6.0),
    grind!("Толчёная сера", "Сера", 8.0),
    grind!("Толчёная киноварь", "Киноварь", 8.0),
    Recipe {
        product: "Сок подорожника",
        technique: Technique::Poultice,
        vessel: VesselNeed::Mortar,
        parts: &[part(&["Подорожник"], 1.0, 0.0)],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (0.4, 0.6, 0.0),
        yield_per: Yield::PerDrachms(16.0),
        acidic: false,
        hint: "Сок: свежий подорожник толочь с полчаса, потом отжать через лён (нужен отрез льна). Две горсти — флакон. Останавливает кровь.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Маковый сок",
        technique: Technique::Dosage,
        vessel: VesselNeed::Mortar,
        parts: &[part(&["Маковые коробочки"], 1.0, 0.0)],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (0.4, 0.6, 0.0),
        yield_per: Yield::PerDrachms(18.0),
        acidic: false,
        hint: "Меконий (Диоскорид): коробочки с листьями истолочь, отжать через лён. Слабее опия, но давать по каплям.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    // --- Основы: вино, уксус, жир ---
    Recipe {
        product: "Полынное вино",
        technique: Technique::Maceration,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Полынь"], 0.07, 0.045),
            part(&["Кувшин вина"], 0.93, 0.05),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (12.0, 36.0, 0.0),
        yield_per: Yield::PerDrachms(60.0),
        acidic: false,
        hint: "Апсинтит (Диоскорид, V): горсть полыни, лучше сушёной, на кувшин вина, настаивать сутки-двое без огня. Вино вытягивает больше воды и хранится дольше.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Вино с миррой",
        technique: Technique::WoundWashing,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Мирра"], 0.035, 0.025),
            part(&["Кувшин вина"], 0.965, 0.03),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (2.0, 6.0, 0.0),
        yield_per: Yield::PerDrachms(60.0),
        acidic: false,
        hint: "Для промывания ран: кусочек толчёной мирры на кувшин вина, настоять несколько часов.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Чабрецовый уксус",
        technique: Technique::Maceration,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Чабрец"], 0.3, 0.15),
            part(&["Винный уксус"], 0.7, 0.15),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (6.0, 12.0, 0.0),
        yield_per: Yield::PerDrachms(14.0),
        acidic: true,
        hint: "Уксус вытягивает из трав то, чего не берёт вода: чабрец в уксусе полсуток. В меди не держать — позеленеет. Промывать раны.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Мазь из окопника",
        technique: Technique::Salve,
        vessel: VesselNeed::Pot,
        parts: &[
            part(&["Корень окопника"], 0.25, 0.12),
            part(&["Бараний жир"], 0.75, 0.12),
        ],
        water: (0.0, 0.0),
        heat: &[Heat::Low],
        reduce: (0.0, 1.0),
        hours: (1.0, 2.0, 5.0),
        yield_per: Yield::PerDrachms(10.0),
        acidic: false,
        hint: "Толчёный корень окопника в топлёном бараньем жиру, часа два на слабом огне или на водяной бане. Сращивает раны.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
    Recipe {
        product: "Отвар ревеня",
        technique: Technique::Decoction,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Ревень понтийский"], 1.0, 0.0)],
        water: (16.0, 9.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.5, 0.3),
        hours: HERB_DECOCTION_HOURS,
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Ра понтийский (Диоскорид, III): корень на две котилы воды, уварить наполовину. Слабит и выгоняет дурное из чрева.",
        fumes: None,
        consumes_vessel: false,
        ignites: None,
    },
];

/// Что положено в сосуд.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BrewSpec {
    /// (имя предмета, сколько штук).
    pub items: Vec<(String, u16)>,
    /// Котил воды.
    pub water: u8,
    pub heat: Heat,
    pub hours: f32,
    /// Водяная баня Марии: сосуд стоит в котле с кипящей водой — жар не выше кипения.
    #[serde(default)]
    pub bath: bool,
}

/// Где и чем работают (кроме самого сосуда).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workplace {
    /// Алхимический стол рядом.
    pub table: bool,
    /// Есть лён: процедить, отжать сок, остудить купол.
    pub cloth: bool,
    /// Есть второй горшок или котёл под водяную баню.
    pub outer_pot: bool,
    /// Есть гончарная глина — обмазать стыки перегонного прибора (лют).
    pub clay: bool,
    /// Как вёлся огонь (None — ровно, как задано).
    pub fire: Option<FireLog>,
    /// Как работали руками 0..1: растирали, мешали, качали мехи (None — не считали).
    pub work: Option<f32>,
}

/// Как на самом деле горел огонь за работу: доли времени, когда жар был заметно выше и
/// ниже заданного, и сильнейший перегрев купола перегонного прибора (1 — давление рвёт стекло).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FireLog {
    pub over: f32,
    pub under: f32,
    pub dome_peak: f32,
}

impl BrewSpec {
    pub fn real_secs(&self) -> f32 {
        (self.hours * SECS_PER_HOUR).clamp(2.0, MAX_SECS)
    }

    fn dry_mass(&self) -> f32 {
        self.items
            .iter()
            .map(|(n, c)| equiv_mass(n).unwrap_or(0.0) * *c as f32)
            .sum()
    }
}

/// Чем кончилась варка.
#[derive(Debug, Clone, PartialEq)]
pub struct BrewResult {
    /// Что вышло: (имя, сколько, индекс качества 0..5).
    pub product: Option<(&'static str, u16, usize)>,
    /// Сколько пустых флаконов ушло.
    pub flasks_used: u16,
    /// Сосуд лопнул.
    pub vessel_broken: bool,
    /// Обжёгся (кипело через край).
    pub burn: f32,
    pub text: String,
    /// Какой приём практиковали (для роста навыка).
    pub technique: Option<Technique>,
    /// Надышался: (яд, доз).
    pub poison: Option<(Toxin, f32)>,
    /// Смесь вспыхнула (для огненной вспышки на экране).
    pub ignited: bool,
    /// Порезался осколками (тяжесть 0..1).
    pub cut: f32,
    /// Стыки обмазаны лютом (ушёл кусок глины).
    pub luted: bool,
    /// Сосуд ушёл в изделие.
    pub vessel_used: bool,
}

/// Насколько значение похоже на идеал: 1 — точно, 0 — за допуском.
fn closeness(value: f32, ideal: f32, tol: f32) -> f32 {
    if tol <= 0.0 {
        return if (value - ideal).abs() < 1e-3 {
            1.0
        } else {
            0.0
        };
    }
    (1.0 - ((value - ideal) / tol).abs()).max(0.0)
}

/// Подходит ли смесь к рецепту и насколько (0 — нет).
/// Сок отжимают через лён.
fn is_juice(r: &Recipe) -> bool {
    matches!(r.product, "Сок подорожника" | "Маковый сок")
}

/// Толчение сырья в ступке.
fn is_grinding(r: &Recipe) -> bool {
    GROUND.iter().any(|(g, _)| *g == r.product)
}

/// Травяные вытяжки: сушёное сырьё лучше свежего, пережжённое горчит.
fn herbal(r: &Recipe) -> bool {
    matches!(
        r.technique,
        Technique::Decoction
            | Technique::Maceration
            | Technique::Oxymel
            | Technique::Theriac
            | Technique::Salve
    )
}

/// Подходит ли смесь к рецепту и насколько (0 — нет). `heat` — жар, который на деле
/// получает сосуд (водяная баня — не выше слабого), `boil_mult` — сколько выкипает.
fn match_recipe_at(
    r: &Recipe,
    spec: &BrewSpec,
    vessel: Vessel,
    heat: Heat,
    boil_mult: f32,
) -> (f32, Vec<&'static str>) {
    let mut notes = Vec::new();
    let dry = spec.dry_mass();
    if dry <= 0.0 {
        return (0.0, notes);
    }
    let vessel_ok = match r.vessel {
        VesselNeed::Pot => matches!(vessel, Vessel::ClayPot | Vessel::CopperKettle),
        VesselNeed::Mortar => vessel == Vessel::Mortar,
        VesselNeed::Still => vessel.still(),
        VesselNeed::Crucible => vessel == Vessel::Crucible,
        VesselNeed::Tray => vessel == Vessel::DryingTray,
    };
    // Зажигательная смесь делается прямо в глиняном горшке, который потом и бросают.
    if !vessel_ok || (r.consumes_vessel && vessel != Vessel::ClayPot) {
        return (0.0, notes);
    }
    // Всё, что положено, должно быть из рецепта.
    let in_recipe = |n: &str| r.parts.iter().any(|p| p.any_of.contains(&canonical(n).0));
    let foreign: f32 = spec
        .items
        .iter()
        .filter(|(n, _)| !in_recipe(n))
        .map(|(n, c)| equiv_mass(n).unwrap_or(0.0) * *c as f32)
        .sum();
    let foreign_share = foreign / dry;
    if foreign_share > 0.25 {
        return (0.0, notes);
    }
    let mut score = 1.0 - foreign_share * 3.0;
    if foreign > 0.0 {
        notes.push("лишнее в смеси");
    }
    let mut any_present = false;
    for p in r.parts {
        let m: f32 = spec
            .items
            .iter()
            .filter(|(n, _)| p.any_of.contains(&canonical(n).0))
            .map(|(n, c)| equiv_mass(n).unwrap_or(0.0) * *c as f32)
            .sum();
        if m <= 0.0 {
            if p.optional {
                score *= 0.85;
                continue;
            }
            return (0.0, notes);
        }
        any_present = true;
        let share = m / dry;
        // Для необязательных — без жёсткого провала, только хуже.
        let c = closeness(share, p.share, p.tol.max(0.001));
        if r.parts.len() > 1 {
            if c <= 0.0 && !p.optional {
                return (0.0, notes);
            }
            score *= 0.6 + 0.4 * c;
        }
    }
    if !any_present {
        return (0.0, notes);
    }
    // Подготовка сырья.
    let mut prep = 1.0f32;
    for (n, _) in &spec.items {
        let (raw, form) = canonical(n);
        if r.vessel == VesselNeed::Tray || is_grinding(r) {
            // Сушат свежее, толкут целое.
            if form != Form::Raw {
                return (0.0, notes);
            }
            continue;
        }
        if is_juice(r) {
            if form == Form::Dried {
                return (0.0, notes);
            }
            continue;
        }
        let mut note = |k: f32, text: &'static str| {
            prep = prep.min(k);
            if !notes.contains(&text) {
                notes.push(text);
            }
        };
        if r.vessel == VesselNeed::Mortar && leafy(raw) && form == Form::Dried {
            note(0.7, "сухую траву на рану не кладут — её размачивают");
        } else if herbal(r) && leafy(raw) && form == Form::Raw {
            note(0.85, "из свежей травы слабее, чем из сушёной");
        }
        if grindable(raw) && form == Form::Raw && r.vessel != VesselNeed::Mortar {
            note(0.85, "не истолкли — отдало мало");
        }
    }
    score *= prep;
    // Вода.
    let water = spec.water as f32 * DRACHMS_PER_KOTYLE;
    let (wi, wt) = r.water;
    if wi == 0.0 && wt == 0.0 {
        if water > 0.0 {
            notes.push("вода испортила");
            score *= 0.2;
        }
    } else {
        let c = closeness(water / dry, wi, wt);
        if c <= 0.0 {
            notes.push(if water / dry > wi {
                "слишком жидко"
            } else {
                "слишком густо"
            });
            score *= 0.3;
        } else {
            score *= 0.5 + 0.5 * c;
        }
    }
    // Огонь.
    if !r.heat.contains(&heat) {
        notes.push(if heat > r.heat[r.heat.len() - 1] {
            "огонь слишком сильный"
        } else {
            "не тот огонь"
        });
        score *= 0.35;
    }
    // Время: у слабого огня трава сохнет втрое быстрее, чем в тени.
    let hours = if r.vessel == VesselNeed::Tray && heat == Heat::Low {
        spec.hours * 3.0
    } else {
        spec.hours
    };
    let (min, best, max) = r.hours;
    if hours < min {
        notes.push("не довели до конца");
        score *= 0.4 * hours / min;
    } else if max > 0.0 && hours > max {
        notes.push("передержали");
        score *= 0.5;
    } else {
        let c = if hours <= best {
            0.7 + 0.3 * (hours - min) / (best - min).max(0.01)
        } else {
            1.0
        };
        score *= c;
    }
    // Уваривание.
    if water > 0.0 && heat != Heat::None && r.reduce.1 < 1.0 {
        let gone = (heat.boil_off() * boil_mult * spec.hours / spec.water as f32).min(1.0);
        let c = closeness(gone, r.reduce.0, r.reduce.1);
        if c <= 0.0 {
            notes.push(if gone > r.reduce.0 {
                "переварили"
            } else {
                "недоварили"
            });
            score *= 0.5;
        } else {
            score *= 0.6 + 0.4 * c;
        }
    }
    (score.clamp(0.0, 1.0), notes)
}

/// Индекс качества по точности (навык добавляет руку мастера).
fn quality_index(score: f32, skill: f32) -> Option<usize> {
    let s = score * (0.82 + 0.18 * skill / 100.0);
    let t = [0.3, 0.45, 0.6, 0.74, 0.86, 0.94];
    t.iter().rposition(|x| s >= *x)
}

/// Сварить. `flasks` — сколько пустых флаконов в сумке.
/// `roll` 0..1 — случай (лопнет ли стекло). `bench` — работа за алхимическим столом:
/// ровный жар жаровни и подставки для приборов (стекло лопается вдвое реже, точнее выходит).
pub fn brew(
    spec: &BrewSpec,
    vessel: Vessel,
    k: &Knowledge,
    flasks: u16,
    roll: f32,
    wp: &Workplace,
) -> BrewResult {
    let bench = wp.table;
    let mut result = BrewResult {
        product: None,
        flasks_used: 0,
        vessel_broken: false,
        burn: 0.0,
        text: String::new(),
        technique: None,
        poison: None,
        vessel_used: false,
        ignited: false,
        cut: 0.0,
        luted: false,
    };
    let bath = spec.bath;
    if bath {
        let err = if !matches!(vessel, Vessel::ClayPot | Vessel::CopperKettle) {
            Some("Водяная баня — для горшка или котелка")
        } else if !wp.outer_pot {
            Some("Для водяной бани нужен второй горшок или котелок с водой")
        } else if spec.heat == Heat::None {
            Some("Под водяной баней нужен огонь")
        } else {
            None
        };
        if let Some(e) = err {
            result.text = e.into();
            return result;
        }
    }
    // В бане сосуд не горячее кипящей воды, а выкипает из него мало.
    let heat = if bath { Heat::Low } else { spec.heat };
    let boil_mult = if bath { 0.3 } else { 1.0 };
    let water = spec.water as f32;
    let left = (water - heat.boil_off() * boil_mult * spec.hours).max(0.0);
    // Всухую на огне: пригорает, глина лопается.
    let dry_fire = !bath && spec.heat >= Heat::Medium && (water == 0.0 || left <= 0.0);
    if vessel == Vessel::DryingTray && spec.heat >= Heat::Medium {
        result.product = Some((SPOILED, 1, 0));
        result.text = "Над сильным огнём трава пересохла и обуглилась".into();
        return result;
    }
    // Лют (глиняная обмазка стыков) у знающего держит давление дольше и паров меньше.
    let luted = vessel.still() && wp.clay && k.knows(Lore::Technique(Technique::Distillation));
    result.luted = luted;
    let dome_limit = if luted { 1.35 } else { 1.0 };
    if vessel.glass()
        && let Some(f) = wp.fire
        && f.dome_peak >= dome_limit
    {
        result.vessel_broken = true;
        result.burn = 0.3;
        result.cut = 0.35;
        result.text = "Купол перегрелся, пар разорвал стекло: ожоги и порезы осколками".into();
        if spec.items.iter().any(|(n, _)| canonical(n).0 == "Киноварь") {
            result.poison = Some((Toxin::Mercury, 1.2));
            result.text.push_str(". Пары ртути!");
        }
        return result;
    }
    let only_fat = spec.items.iter().all(|(n, _)| {
        matches!(
            n.as_str(),
            "Воск" | "Бараний жир" | "Кувшин оливкового масла" | "Щёлок"
        )
    });
    if vessel == Vessel::Mortar && spec.heat != Heat::None {
        result.text = "Ступку на огонь не ставят".into();
        return result;
    }
    if !vessel.takes_water() && spec.water > 0 {
        result.text = format!("В {} воду не льют", vessel.name().to_lowercase());
        return result;
    }
    // Стекло боится сильного огня: знающий разогревает постепенно.
    if vessel.glass() && spec.heat == Heat::High && wp.fire.is_none() {
        let careful = k.knows(Lore::Technique(Technique::Distillation));
        let chance = if vessel == Vessel::Retort { 0.6 } else { 0.45 }
            * if careful { 0.5 } else { 1.0 }
            * if bench { 0.5 } else { 1.0 };
        if roll < chance {
            result.vessel_broken = true;
            result.burn = 0.25;
            result.text = "Стекло не выдержало жара и лопнуло, горячее брызнуло на руки".into();
            if spec.items.iter().any(|(n, _)| canonical(n).0 == "Киноварь") {
                result.poison = Some((Toxin::Mercury, 1.2));
                result.text.push_str(". Пары ртути!");
            }
            return result;
        }
    }
    // Тигель и перегонный прибор стоят на огне всухую — так и надо.
    let dry_fire = dry_fire && vessel.takes_water();
    let flammable = spec.items.iter().any(|(n, _)| {
        matches!(
            canonical(n).0,
            "Сосновая смола" | "Сера" | "Нефть" | "Ладан"
        )
    });
    if dry_fire && flammable {
        result.burn = 0.4;
        result.ignited = true;
        result.product = Some((SPOILED, 1, 0));
        result.text = "Смесь вспыхнула на сильном огне! Вы обожглись".into();
        return result;
    }
    if dry_fire && !only_fat {
        if vessel == Vessel::ClayPot && spec.heat == Heat::High {
            result.vessel_broken = true;
            result.text = "Вода выкипела, всё пригорело, и горшок лопнул".into();
        } else {
            result.text = "Вода выкипела, всё пригорело".into();
        }
        result.product = Some((SPOILED, 1, 0));
        return result;
    }
    if !bath && spec.heat == Heat::High && water >= 4.0 && spec.hours > 0.5 {
        // Бурное кипение через край.
        result.burn = 0.15;
    }
    let best = RECIPES
        .iter()
        .map(|r| (r, match_recipe_at(r, spec, vessel, heat, boil_mult)))
        .max_by(|a, b| a.1.0.total_cmp(&b.1.0));
    let Some((recipe, (score, mut notes))) = best.filter(|(_, (s, _))| *s > 0.0) else {
        // Знающий свойства трав мастер смешает мазь и без готового рецепта.
        if let Some(r) = free_salve(spec, vessel, heat, k, flasks) {
            return r;
        }
        result.product = Some((SPOILED, 1, 0));
        result.text = "Вышла мутная бурда, ни на что не похожая".into();
        return result;
    };
    if is_juice(recipe) && !wp.cloth {
        result.text = "Чтобы отжать сок, нужен отрез льна".into();
        return result;
    }
    result.technique = Some(recipe.technique);
    if let Some(at) = recipe.ignites
        && spec.heat >= at
    {
        result.burn = 0.4;
        result.ignited = true;
        result.product = Some((SPOILED, 1, 0));
        result.text = "Смесь вспыхнула! Всё сгорело, вы обожглись".into();
        if vessel == Vessel::ClayPot && recipe.consumes_vessel {
            result.vessel_broken = true;
        }
        return result;
    }
    let mut score = score;
    // Как на деле вёлся огонь.
    if let Some(f) = wp.fire
        && spec.heat != Heat::None
    {
        if f.over > 0.3 && !bath && herbal(recipe) {
            result.product = Some((SPOILED, 1, 0));
            result.text =
                "Огонь разгорелся слишком сильно: травы пригорели и дали одну горечь".into();
            return result;
        }
        if f.over > 0.3 && !bath {
            notes.push("жар гулял");
            score *= 0.75;
        }
        if f.under > 0.45 {
            notes.push("огонь то и дело угасал");
            score *= 0.6;
        }
    }
    // Руки: растёртое комками и неразмешанное выходит хуже.
    if let Some(w) = wp.work {
        let w = w.clamp(0.0, 1.0);
        if vessel == Vessel::Mortar {
            if w < 0.45 {
                notes.push("растёрто комками");
            }
            score = (score * (0.6 + 0.45 * w)).min(1.0);
        } else {
            if w < 0.4 {
                notes.push(if vessel.still() || vessel == Vessel::Crucible {
                    "мехи качали рывками"
                } else {
                    "мешали кое-как"
                });
            }
            score = (score * (0.82 + 0.22 * w)).min(1.0);
        }
    }
    // Жидкое с травой процеживают через лён.
    if recipe.bottled() && recipe.water.0 > 0.0 {
        if wp.cloth {
            score = (score * 1.05).min(1.0);
        } else {
            notes.push("не процедили — муть и осадок");
            score *= 0.92;
        }
    }
    if bath
        && matches!(
            recipe.technique,
            Technique::Salve | Technique::Maceration | Technique::Theriac | Technique::Oxymel
        )
    {
        score = (score * 1.1).min(1.0);
    }
    if let Some((t, d)) = recipe.fumes {
        // Закрытый керотакис и знание, что работать надо на ветру, берегут лёгкие.
        let mut closed = if vessel == Vessel::Kerotakis {
            0.3
        } else {
            1.0
        };
        if luted {
            closed *= 0.6;
        }
        if vessel == Vessel::Kerotakis
            && let Some(f) = wp.fire
            && f.dome_peak >= dome_limit
        {
            // Крышку сорвало паром.
            closed = 1.0;
            notes.push("крышку сорвало паром");
        }
        let knows = if k.knows(Lore::Technique(recipe.technique)) {
            0.35
        } else {
            1.0
        };
        result.poison = Some((t, d * closed * knows));
    }
    if recipe.acidic && vessel == Vessel::CopperKettle {
        result.product = Some((SPOILED, 1, 0));
        result.text = "Кислое разъело медь: варево позеленело, пить его — яд".into();
        return result;
    }
    let score = if bench { (score * 1.1).min(1.0) } else { score };
    let Some(q) = quality_index(score, k.skill(recipe.technique.skill())) else {
        result.product = Some((SPOILED, 1, 0));
        result.text = format!(
            "Похоже на «{}», но не вышло: {}",
            recipe.product,
            if notes.is_empty() {
                "всё не так".into()
            } else {
                notes.join(", ")
            }
        );
        return result;
    };
    let mut count = match recipe.yield_per {
        Yield::PerKotyle(n) => (left * n).round().max(1.0) as u16,
        Yield::PerDrachms(d) => (spec.dry_mass() / d).round().max(1.0) as u16,
    };
    if recipe.bottled() {
        if flasks == 0 {
            result.text = "Не во что разлить: нужны пустые флаконы".into();
            return result;
        }
        if flasks < count {
            count = flasks;
        }
        result.flasks_used = count;
    }
    if recipe.consumes_vessel {
        count = 1;
        result.vessel_used = true;
    }
    result.product = Some((recipe.product, count, q));
    let mut text = format!("Готово: {} ×{}", recipe.product, count);
    if !notes.is_empty() {
        text.push_str(&format!(" ({})", notes.join(", ")));
    }
    result.text = text;
    result
}

const FATS: [&str; 3] = ["Воск", "Бараний жир", "Кувшин оливкового масла"];

/// Свойства сырья для мазей: (вяжущее, обеззараживающее, смягчающее, заживляющее).
fn salve_props(raw: &str) -> Option<[f32; 4]> {
    Some(match raw {
        "Подорожник" => [0.7, 0.15, 0.0, 0.4],
        "Тысячелистник" => [0.5, 0.1, 0.0, 0.1],
        "Дубовые галлы" => [0.8, 0.1, 0.0, 0.0],
        "Квасцы" => [0.6, 0.2, 0.0, 0.0],
        "Ладан" => [0.6, 0.35, 0.0, 0.1],
        "Шалфей" => [0.25, 0.25, 0.0, 0.0],
        "Мирра" => [0.0, 0.5, 0.0, 0.4],
        "Чабрец" => [0.0, 0.4, 0.0, 0.0],
        "Горшок мёда" => [0.0, 0.5, 0.3, 0.2],
        "Корень окопника" => [0.0, 0.0, 0.2, 0.9],
        "Зверобой" => [0.0, 0.2, 0.3, 0.3],
        "Просвирник" => [0.0, 0.1, 0.7, 0.0],
        _ => return None,
    })
}

/// Мастер, знающий свойства каждой травы в смеси, сам составит мазь: вяжущее с жиром —
/// вяжущая мазь, смягчающее — от ожогов, обеззараживающее и заживляющее — ранозаживляющая.
fn free_salve(
    spec: &BrewSpec,
    vessel: Vessel,
    heat: Heat,
    k: &Knowledge,
    flasks: u16,
) -> Option<BrewResult> {
    if !matches!(vessel, Vessel::ClayPot | Vessel::CopperKettle)
        || heat != Heat::Low
        || spec.water > 0
    {
        return None;
    }
    if !k.knows(Lore::Technique(Technique::Salve)) {
        return None;
    }
    let total = spec.dry_mass();
    let fat: f32 = spec
        .items
        .iter()
        .filter(|(n, _)| FATS.contains(&n.as_str()))
        .map(|(n, c)| equiv_mass(n).unwrap_or(0.0) * *c as f32)
        .sum();
    if total <= 0.0 || fat / total < 0.5 || fat >= total {
        return None;
    }
    let mut v = [0.0f32; 4];
    for (n, c) in spec
        .items
        .iter()
        .filter(|(n, _)| !FATS.contains(&n.as_str()))
    {
        let raw = canonical(n).0;
        let props = salve_props(raw)?;
        // Свойства надо знать, а не угадывать.
        if let Some(s) =
            crate::items::ItemBaseId::find(raw).and_then(crate::lore::Substance::from_item)
            && !k.knows(Lore::Properties(s))
        {
            return None;
        }
        let m = equiv_mass(n).unwrap_or(0.0) * *c as f32;
        for i in 0..4 {
            v[i] += props[i] * m;
        }
    }
    let sum: f32 = v.iter().sum();
    if sum <= 0.0 {
        return None;
    }
    let (styptic, soothe) = (v[0], v[2]);
    let rest = v[1] + v[3];
    let (product, top) = if styptic >= soothe && styptic >= rest {
        ("Вяжущая мазь", styptic)
    } else if soothe >= rest {
        ("Мазь от ожогов", soothe)
    } else {
        ("Ранозаживляющая мазь", rest)
    };
    let fat_ok = closeness(fat / total, 0.8, 0.2);
    let score = (0.55 + 0.45 * top / sum) * (0.6 + 0.4 * fat_ok);
    let mut result = BrewResult {
        product: None,
        flasks_used: 0,
        vessel_broken: false,
        burn: 0.0,
        text: String::new(),
        technique: Some(Technique::Salve),
        poison: None,
        vessel_used: false,
        ignited: false,
        cut: 0.0,
        luted: false,
    };
    let Some(q) = quality_index(score, k.skill(Technique::Salve.skill())) else {
        result.product = Some((SPOILED, 1, 0));
        result.text = "Жирная каша: свойства трав спорят друг с другом".into();
        return Some(result);
    };
    if flasks == 0 {
        result.text = "Не во что разлить: нужны пустые флаконы".into();
        return Some(result);
    }
    let count = ((total / 12.0).round().max(1.0) as u16).min(flasks);
    result.flasks_used = count;
    result.product = Some((product, count, q));
    result.text = format!("Вы составили мазь по свойствам трав: {product} ×{count}");
    Some(result)
}

/// Рецепты, которые знает персонаж (по приёмам), — для подсказок у сосуда.
pub fn known_hints(k: &Knowledge) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    for r in RECIPES {
        if k.knows(Lore::Technique(r.technique)) && !out.iter().any(|(_, h)| *h == r.hint) {
            out.push((r.product, r.hint));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemBaseId;
    use crate::lore::Skill;

    fn wp(table: bool) -> Workplace {
        Workplace {
            table,
            cloth: true,
            outer_pot: true,
            clay: true,
            fire: None,
            work: None,
        }
    }

    fn spec(items: &[(&str, u16)], water: u8, heat: Heat, hours: f32) -> BrewSpec {
        BrewSpec {
            items: items.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
            water,
            heat,
            hours,
            bath: false,
        }
    }

    #[test]
    fn all_names_exist() {
        for r in RECIPES {
            assert!(ItemBaseId::find(r.product).is_some(), "{}", r.product);
            for p in r.parts {
                for n in p.any_of {
                    assert!(ItemBaseId::find(n).is_some(), "{n}");
                    assert!(mass(n).is_some(), "нет веса у {n}");
                }
            }
        }
        for n in [
            SPOILED,
            FLASK,
            "Глиняный горшок",
            "Медный котелок",
            "Ступка с пестиком",
        ] {
            assert!(ItemBaseId::find(n).is_some(), "{n}");
        }
    }

    #[test]
    fn real_recipe_works_without_knowledge() {
        // Гален: воск к маслу 1:4, на слабом огне — и неуч сварит мазь.
        let k = Knowledge::default();
        let r = brew(
            &spec(
                &[("Воск", 1), ("Кувшин оливкового масла", 1), ("Мирра", 1)],
                0,
                Heat::Low,
                1.0,
            ),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        let (name, n, q) = r.product.expect("что-то вышло");
        assert_eq!(name, "Ранозаживляющая мазь", "{}", r.text);
        assert!(n >= 2);
        assert!(q >= 2, "качество {q}: {}", r.text);
    }

    #[test]
    fn wrong_proportions_fail() {
        let k = Knowledge::default();
        // Воска больше, чем масла.
        let r = brew(
            &spec(
                &[("Воск", 4), ("Кувшин оливкового масла", 1)],
                0,
                Heat::Low,
                1.0,
            ),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(r.product.map(|p| p.0), Some(SPOILED), "{}", r.text);
    }

    #[test]
    fn decoction_needs_water_and_gentle_fire() {
        let k = Knowledge::default();
        let good = brew(
            &spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(
            good.product.map(|p| p.0),
            Some("Отвар шалфея"),
            "{}",
            good.text
        );
        // Без воды на сильном огне глина лопается.
        let burnt = brew(
            &spec(&[("Шалфей", 2)], 0, Heat::High, 1.0),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert!(burnt.vessel_broken);
        // Выкипело всё.
        let dry = brew(
            &spec(&[("Шалфей", 2)], 2, Heat::High, 2.0),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(dry.product.map(|p| p.0), Some(SPOILED));
    }

    #[test]
    fn vinegar_eats_copper() {
        let k = Knowledge::default();
        let r = brew(
            &spec(
                &[("Горшок мёда", 1), ("Винный уксус", 1)],
                1,
                Heat::Low,
                1.0,
            ),
            Vessel::CopperKettle,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(r.product.map(|p| p.0), Some(SPOILED), "{}", r.text);
        let r = brew(
            &spec(
                &[("Горшок мёда", 1), ("Винный уксус", 1)],
                1,
                Heat::Low,
                1.0,
            ),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(r.product.map(|p| p.0), Some("Оксимель"), "{}", r.text);
    }

    #[test]
    fn liquids_need_flasks() {
        let k = Knowledge::default();
        let r = brew(
            &spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5),
            Vessel::ClayPot,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert!(r.product.is_none());
        let r = brew(
            &spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5),
            Vessel::ClayPot,
            &k,
            1,
            0.9,
            &wp(false),
        );
        assert_eq!(r.product.map(|p| p.1), Some(1));
        assert_eq!(r.flasks_used, 1);
    }

    #[test]
    fn poultice_in_mortar() {
        let k = Knowledge::default();
        let r = brew(
            &spec(
                &[("Тысячелистник", 1), ("Подорожник", 1)],
                0,
                Heat::None,
                0.25,
            ),
            Vessel::Mortar,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(
            r.product.map(|p| p.0),
            Some("Кровоостанавливающая припарка"),
            "{}",
            r.text
        );
    }

    #[test]
    fn careless_grinding_gives_worse_poultice() {
        let mut k = Knowledge::default();
        for _ in 0..200 {
            k.practice(Skill::Herbalism, 1.0);
        }
        let s = spec(
            &[("Тысячелистник", 1), ("Подорожник", 1)],
            0,
            Heat::None,
            0.25,
        );
        let good = brew(
            &s,
            Vessel::Mortar,
            &k,
            0,
            0.9,
            &Workplace {
                work: Some(1.0),
                ..wp(false)
            },
        );
        let bad = brew(
            &s,
            Vessel::Mortar,
            &k,
            0,
            0.9,
            &Workplace {
                work: Some(0.1),
                ..wp(false)
            },
        );
        let (g, b) = (
            good.product.expect("вышло").2,
            bad.product.expect("вышло").2,
        );
        assert!(g > b, "ровно {g}, комками {b}");
        assert!(bad.text.contains("комками"), "{}", bad.text);
    }

    #[test]
    fn skill_improves_quality() {
        let novice = Knowledge::default();
        let mut master = Knowledge::default();
        for _ in 0..400 {
            master.practice(Skill::Herbalism, 1.0);
        }
        let s = spec(&[("Сушёный шалфей", 2)], 3, Heat::Low, 2.0);
        let a = brew(&s, Vessel::ClayPot, &novice, 5, 0.9, &wp(false))
            .product
            .unwrap()
            .2;
        let b = brew(&s, Vessel::ClayPot, &master, 5, 0.9, &wp(false))
            .product
            .unwrap()
            .2;
        assert!(b > a, "{a} {b}");
    }

    #[test]
    fn theriac_and_vinegar() {
        let k = Knowledge::default();
        let t = brew(
            &spec(
                &[
                    ("Опий", 2),
                    ("Мирра", 1),
                    ("Ягоды можжевельника", 1),
                    ("Корица", 1),
                    ("Шафран", 1),
                    ("Перец", 1),
                    ("Горшок мёда", 1),
                ],
                0,
                Heat::Low,
                12.0,
            ),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(t.product.map(|p| p.0), Some("Териак"), "{}", t.text);
        let v = brew(
            &spec(&[("Кувшин вина", 1)], 0, Heat::None, 72.0),
            Vessel::ClayPot,
            &k,
            10,
            0.9,
            &wp(false),
        );
        assert_eq!(v.product.map(|p| p.0), Some("Винный уксус"), "{}", v.text);
    }

    #[test]
    fn mercury_from_cinnabar_poisons_unless_careful() {
        let k = Knowledge::default();
        let s = spec(&[("Киноварь", 3)], 0, Heat::Medium, 2.0);
        let r = brew(&s, Vessel::Alembic, &k, 5, 0.9, &wp(false));
        assert_eq!(r.product.map(|p| p.0), Some("Ртуть"), "{}", r.text);
        let (_, d) = r.poison.expect("пары");
        let mut sage = Knowledge::default();
        sage.learn(Lore::Technique(Technique::Sublimation));
        let careful = brew(&s, Vessel::Kerotakis, &sage, 5, 0.9, &wp(false))
            .poison
            .unwrap()
            .1;
        assert!(careful < d * 0.2, "{careful} {d}");
    }

    #[test]
    fn glass_bursts_on_strong_fire() {
        let k = Knowledge::default();
        let s = spec(&[("Сосновая смола", 2)], 0, Heat::High, 1.0);
        let r = brew(&s, Vessel::Retort, &k, 5, 0.1, &wp(false));
        assert!(r.vessel_broken && r.burn > 0.0, "{}", r.text);
        // Ровный средний огонь — масло.
        let ok = brew(
            &spec(&[("Сосновая смола", 2)], 0, Heat::Medium, 2.0),
            Vessel::Alembic,
            &k,
            5,
            0.1,
            &wp(false),
        );
        assert_eq!(
            ok.product.map(|p| p.0),
            Some("Смоляное масло"),
            "{}",
            ok.text
        );
    }

    #[test]
    fn pigments_and_dyes() {
        let k = Knowledge::default();
        let lead = brew(
            &spec(&[("Свинец", 1), ("Винный уксус", 1)], 0, Heat::None, 72.0),
            Vessel::ClayPot,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(
            lead.product.map(|p| p.0),
            Some("Свинцовые белила"),
            "{}",
            lead.text
        );
        let red = brew(
            &spec(&[("Свинцовые белила", 3)], 0, Heat::High, 3.0),
            Vessel::Crucible,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(red.product.map(|p| p.0), Some("Сурик"), "{}", red.text);
        let verd = brew(
            &spec(
                &[("Медный лом", 1), ("Винный уксус", 1)],
                0,
                Heat::None,
                72.0,
            ),
            Vessel::ClayPot,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(
            verd.product.map(|p| p.0),
            Some("Ярь-медянка"),
            "{}",
            verd.text
        );
        let madder = brew(
            &spec(
                &[("Корень марены", 3), ("Квасцы", 2), ("Натр", 1)],
                2,
                Heat::Low,
                2.0,
            ),
            Vessel::ClayPot,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(
            madder.product.map(|p| p.0),
            Some("Мареновый лак"),
            "{}",
            madder.text
        );
    }

    #[test]
    fn incendiary_pot_ignites_on_strong_fire() {
        let k = Knowledge::default();
        let good = brew(
            &spec(
                &[("Сосновая смола", 2), ("Сера", 1), ("Ладан", 1)],
                0,
                Heat::Low,
                0.5,
            ),
            Vessel::ClayPot,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(
            good.product.map(|p| (p.0, p.1)),
            Some(("Зажигательный горшок", 1)),
            "{}",
            good.text
        );
        assert!(good.vessel_used);
        let bad = brew(
            &spec(&[("Сосновая смола", 2), ("Сера", 1)], 0, Heat::Medium, 0.5),
            Vessel::ClayPot,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert!(bad.burn > 0.0, "{}", bad.text);
    }

    #[test]
    fn sulfur_water_smells() {
        let k = Knowledge::default();
        let r = brew(
            &spec(
                &[("Толчёная сера", 1), ("Негашёная известь", 1)],
                4,
                Heat::Medium,
                1.0,
            ),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(r.product.map(|p| p.0), Some("Серная вода"), "{}", r.text);
        assert!(r.poison.is_some());
    }

    fn product(r: &BrewResult) -> Option<&'static str> {
        r.product.map(|p| p.0)
    }

    #[test]
    fn drying_and_grinding_prepare_raw_stuff() {
        let k = Knowledge::default();
        let r = brew(
            &spec(&[("Шалфей", 3)], 0, Heat::None, 12.0),
            Vessel::DryingTray,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(
            r.product.map(|p| (p.0, p.1)),
            Some(("Сушёный шалфей", 3)),
            "{}",
            r.text
        );
        // У слабого огня быстрее, на сильном — сгорит.
        let r = brew(
            &spec(&[("Мята", 2)], 0, Heat::Low, 4.0),
            Vessel::DryingTray,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(product(&r), Some("Сушёная мята"), "{}", r.text);
        let r = brew(
            &spec(&[("Мята", 2)], 0, Heat::High, 4.0),
            Vessel::DryingTray,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(product(&r), Some(SPOILED), "{}", r.text);
        let r = brew(
            &spec(&[("Корень девясила", 2)], 0, Heat::None, 0.5),
            Vessel::Mortar,
            &k,
            0,
            0.9,
            &wp(false),
        );
        assert_eq!(
            r.product.map(|p| (p.0, p.1)),
            Some(("Толчёный корень девясила", 2)),
            "{}",
            r.text
        );
        assert_eq!(canonical("Толчёная мирра"), ("Мирра", Form::Ground));
        assert_eq!(mass("Сушёный шалфей"), Some(2.0));
        assert_eq!(equiv_mass("Сушёный шалфей"), Some(8.0));
    }

    #[test]
    fn dried_herb_makes_better_decoction_than_fresh() {
        let k = Knowledge::default();
        let fresh = brew(
            &spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        let dried = brew(
            &spec(&[("Сушёный шалфей", 2)], 4, Heat::Low, 1.5),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        let (f, d) = (fresh.product.unwrap(), dried.product.unwrap());
        assert_eq!((f.0, d.0), ("Отвар шалфея", "Отвар шалфея"));
        assert!(d.2 >= f.2, "сушёное {} свежее {}: {}", d.2, f.2, fresh.text);
        assert!(fresh.text.contains("свежей"), "{}", fresh.text);
        // Неистолчённый корень отдаёт меньше.
        let whole = brew(
            &spec(&[("Корень девясила", 1)], 2, Heat::Low, 1.0),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert!(whole.text.contains("не истолкли"), "{}", whole.text);
        // Без льна — муть.
        let murky = brew(
            &spec(&[("Сушёный шалфей", 2)], 4, Heat::Low, 1.5),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &Workplace::default(),
        );
        assert!(murky.text.contains("не процедили"), "{}", murky.text);
    }

    #[test]
    fn juice_needs_linen() {
        let k = Knowledge::default();
        let s = spec(&[("Подорожник", 4)], 0, Heat::None, 0.5);
        let none = brew(&s, Vessel::Mortar, &k, 5, 0.9, &Workplace::default());
        assert_eq!(none.product, None, "{}", none.text);
        assert!(none.text.contains("льна"));
        let r = brew(&s, Vessel::Mortar, &k, 5, 0.9, &wp(false));
        assert_eq!(
            r.product.map(|p| (p.0, p.1)),
            Some(("Сок подорожника", 2)),
            "{}",
            r.text
        );
        // Короткое растирание — по-прежнему припарка.
        let p = brew(
            &spec(&[("Подорожник", 2)], 0, Heat::None, 0.25),
            Vessel::Mortar,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(
            product(&p),
            Some("Кровоостанавливающая припарка"),
            "{}",
            p.text
        );
    }

    #[test]
    fn bases_wine_vinegar_fat() {
        let k = Knowledge::default();
        let r = brew(
            &spec(
                &[("Сушёная полынь", 1), ("Кувшин вина", 1)],
                0,
                Heat::None,
                24.0,
            ),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(product(&r), Some("Полынное вино"), "{}", r.text);
        let r = brew(
            &spec(&[("Чабрец", 1), ("Винный уксус", 1)], 0, Heat::None, 12.0),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(product(&r), Some("Чабрецовый уксус"), "{}", r.text);
        let r = brew(
            &spec(
                &[("Толчёный корень окопника", 1), ("Бараний жир", 1)],
                0,
                Heat::Low,
                2.0,
            ),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(product(&r), Some("Мазь из окопника"), "{}", r.text);
        let r = brew(
            &spec(&[("Ревень понтийский", 1)], 2, Heat::Low, 1.0),
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &wp(false),
        );
        assert_eq!(product(&r), Some("Отвар ревеня"), "{}", r.text);
    }

    #[test]
    fn neglected_fire_burns_herbs_but_bath_saves() {
        let k = Knowledge::default();
        let hot = Workplace {
            fire: Some(FireLog {
                over: 0.5,
                under: 0.0,
                dome_peak: 0.0,
            }),
            ..wp(false)
        };
        let s = spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5);
        let r = brew(&s, Vessel::ClayPot, &k, 5, 0.9, &hot);
        assert_eq!(product(&r), Some(SPOILED), "{}", r.text);
        assert!(r.text.contains("горечь"));
        let bath = BrewSpec {
            bath: true,
            ..spec(
                &[
                    ("Воск", 1),
                    ("Кувшин оливкового масла", 1),
                    ("Толчёная мирра", 1),
                ],
                0,
                Heat::Medium,
                1.0,
            )
        };
        let r = brew(&bath, Vessel::ClayPot, &k, 5, 0.9, &hot);
        assert_eq!(product(&r), Some("Ранозаживляющая мазь"), "{}", r.text);
        // Без второго горшка бани нет.
        let r = brew(
            &bath,
            Vessel::ClayPot,
            &k,
            5,
            0.9,
            &Workplace {
                outer_pot: false,
                ..wp(false)
            },
        );
        assert_eq!(r.product, None, "{}", r.text);
        // Угасший огонь — хуже.
        let cold = Workplace {
            fire: Some(FireLog {
                over: 0.0,
                under: 0.7,
                dome_peak: 0.0,
            }),
            ..wp(false)
        };
        let r = brew(&s, Vessel::ClayPot, &k, 5, 0.9, &cold);
        assert!(r.text.contains("угасал"), "{}", r.text);
    }

    #[test]
    fn overheated_dome_bursts_glass_unless_luted() {
        let mut k = Knowledge::default();
        let s = spec(&[("Сосновая смола", 3)], 0, Heat::Medium, 4.0);
        let hot = Workplace {
            fire: Some(FireLog {
                over: 0.0,
                under: 0.0,
                dome_peak: 1.1,
            }),
            ..wp(false)
        };
        let r = brew(&s, Vessel::Alembic, &k, 5, 0.9, &hot);
        assert!(r.vessel_broken && r.cut > 0.0 && r.burn > 0.0, "{}", r.text);
        // Мастер обмазал стыки лютом — выдержит.
        k.learn(Lore::Technique(Technique::Distillation));
        let r = brew(&s, Vessel::Alembic, &k, 5, 0.9, &hot);
        assert!(!r.vessel_broken, "{}", r.text);
        // Остуженный купол и без знаний цел, даже на сильном огне.
        let cool = Workplace {
            fire: Some(FireLog {
                over: 0.0,
                under: 0.0,
                dome_peak: 0.5,
            }),
            ..wp(false)
        };
        let r = brew(
            &spec(&[("Сосновая смола", 3)], 0, Heat::High, 4.0),
            Vessel::Alembic,
            &Knowledge::default(),
            5,
            0.0,
            &cool,
        );
        assert!(!r.vessel_broken, "{}", r.text);
    }

    #[test]
    fn master_mixes_salve_by_properties() {
        use crate::lore::Substance;
        let s = spec(
            &[("Толчёные галлы", 1), ("Подорожник", 1), ("Бараний жир", 2)],
            0,
            Heat::Low,
            2.0,
        );
        let novice = Knowledge::default();
        assert_eq!(
            product(&brew(&s, Vessel::ClayPot, &novice, 5, 0.9, &wp(false))),
            Some(SPOILED)
        );
        let mut master = Knowledge::default();
        master.learn(Lore::Technique(Technique::Salve));
        for sub in [
            Substance::OakGall,
            Substance::Plantain,
            Substance::MuttonFat,
        ] {
            master.learn(Lore::Properties(sub));
        }
        let r = brew(&s, Vessel::ClayPot, &master, 5, 0.9, &wp(false));
        assert_eq!(product(&r), Some("Вяжущая мазь"), "{}", r.text);
        let s = spec(
            &[("Просвирник", 2), ("Кувшин оливкового масла", 1)],
            0,
            Heat::Low,
            2.0,
        );
        master.learn(Lore::Properties(Substance::Mallow));
        let r = brew(&s, Vessel::ClayPot, &master, 5, 0.9, &wp(false));
        assert_eq!(product(&r), Some("Мазь от ожогов"), "{}", r.text);
    }
}
