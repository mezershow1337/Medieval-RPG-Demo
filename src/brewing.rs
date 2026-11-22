//! Варка: травы, вода, огонь и время. Честная модель без «рецептов из воздуха».
//!
//! Игрок кладёт в сосуд вещества (каждый предмет весит сколько-то драхм), наливает воду
//! (в котилах), выбирает огонь и время. Сервер смотрит, на какое снадобье похожа смесь,
//! и насколько точно соблюдены пропорции древних врачей. Знание рецепта не нужно, чтобы
//! получилось: тот, кто нашёл настоящий рецепт (Гален: воск к маслу как 1 к 4), сварит
//! и без уроков. Знание даёт подсказки на экране, а навык — точность (качество).
//!
//! Времена сжаты: час варки — несколько секунд, сутки брожения — меньше минуты.

use crate::lore::{Knowledge, Lore, Technique};
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
}

impl Vessel {
    pub fn from_item(name: &str) -> Option<Vessel> {
        match name {
            "Глиняный горшок" => Some(Vessel::ClayPot),
            "Медный котелок" => Some(Vessel::CopperKettle),
            "Ступка с пестиком" => Some(Vessel::Mortar),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Vessel::ClayPot => "Глиняный горшок",
            Vessel::CopperKettle => "Медный котелок",
            Vessel::Mortar => "Ступка с пестиком",
        }
    }
}

/// Вес одного предмета в драхмах (драхма ≈ 3,4 г). None — в сосуд не кладут.
pub fn mass(item: &str) -> Option<f32> {
    Some(match item {
        // Свежая трава: горсть.
        "Шалфей" | "Чабрец" | "Зверобой" | "Подорожник" | "Тысячелистник" | "Просвирник" | "Мята" | "Полынь" | "Белена" | "Вайда" | "Шиповник" => 8.0,
        "Маковые коробочки" | "Ягоды можжевельника" | "Дубовые галлы" => 6.0,
        "Корень марены" | "Корень окопника" | "Корень девясила" | "Солодковый корень" | "Ревень понтийский" => 10.0,
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
    Part { any_of, share, tol, optional: false }
}

const fn opt(any_of: &'static [&'static str], share: f32, tol: f32) -> Part {
    Part { any_of, share, tol, optional: true }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum VesselNeed {
    Pot,
    Mortar,
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
        !matches!(self.product, "Кровоостанавливающая припарка" | "Мыло")
    }
}

const HERB_DECOCTION_HOURS: (f32, f32, f32) = (0.5, 1.0, 3.0);

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
    },
    Recipe {
        product: "Оксимель",
        technique: Technique::Oxymel,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Горшок мёда"], 0.75, 0.12), part(&["Винный уксус"], 0.25, 0.12)],
        water: (1.0, 1.0),
        heat: &[Heat::Low, Heat::Medium],
        reduce: (0.5, 0.4),
        hours: (0.5, 1.0, 3.0),
        yield_per: Yield::PerDrachms(30.0),
        acidic: true,
        hint: "Оксимель: мёда втрое больше, чем уксуса, немного воды, уварить до сиропа. Только в глине — уксус ест медь.",
    },
    Recipe {
        product: "Зверобойное масло",
        technique: Technique::Maceration,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Зверобой"], 0.14, 0.08), part(&["Кувшин оливкового масла"], 0.86, 0.08)],
        water: (0.0, 0.0),
        heat: &[Heat::None, Heat::Low],
        reduce: (0.0, 1.0),
        hours: (2.0, 6.0, 0.0),
        yield_per: Yield::PerDrachms(50.0),
        acidic: false,
        hint: "Цветы зверобоя в оливковом масле: две горсти на кувшин, томить на водяной бане несколько часов, пока масло не покраснеет. Воды не лить.",
    },
    Recipe {
        product: "Ранозаживляющая мазь",
        technique: Technique::Salve,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Воск"], 0.2, 0.07), part(&["Кувшин оливкового масла", "Бараний жир"], 0.77, 0.1), opt(&["Мирра", "Ладан"], 0.03, 0.03)],
        water: (0.0, 0.0),
        heat: &[Heat::Low],
        reduce: (0.0, 1.0),
        hours: (0.3, 1.0, 3.0),
        yield_per: Yield::PerDrachms(40.0),
        acidic: false,
        hint: "Керат Галена: воск и масло как один к четырём, растопить на слабом огне; для ран добавить щепоть мирры или ладана.",
    },
    Recipe {
        product: "Кровоостанавливающая припарка",
        technique: Technique::Poultice,
        vessel: VesselNeed::Mortar,
        parts: &[opt(&["Тысячелистник"], 0.5, 0.35), opt(&["Подорожник"], 0.5, 0.35), opt(&["Квасцы"], 0.08, 0.08)],
        water: (0.0, 0.0),
        heat: &[Heat::None],
        reduce: (0.0, 1.0),
        hours: (0.1, 0.25, 0.0),
        yield_per: Yield::PerDrachms(8.0),
        acidic: false,
        hint: "Растереть в ступке тысячелистник с подорожником, можно с щепотью квасцов.",
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
    },
    Recipe {
        product: "Мыло",
        technique: Technique::Soap,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Щёлок"], 0.5, 0.2), part(&["Бараний жир", "Кувшин оливкового масла"], 0.5, 0.2)],
        water: (0.0, 0.5),
        heat: &[Heat::Medium, Heat::High],
        reduce: (0.0, 1.0),
        hours: (1.0, 3.0, 8.0),
        yield_per: Yield::PerDrachms(25.0),
        acidic: false,
        hint: "Щёлок с жиром поровну по весу, варить на сильном огне, пока не загустеет.",
    },
    Recipe {
        product: "Чернила",
        technique: Technique::Ink,
        vessel: VesselNeed::Pot,
        parts: &[part(&["Дубовые галлы"], 0.55, 0.15), part(&["Медный купорос"], 0.2, 0.1), part(&["Камедь"], 0.25, 0.12)],
        water: (3.0, 2.0),
        heat: &[Heat::None, Heat::Low],
        reduce: (0.0, 1.0),
        hours: (1.0, 6.0, 0.0),
        yield_per: Yield::PerKotyle(2.0),
        acidic: false,
        hint: "Толчёные галлы настоять в воде, добавить купорос и камедь: галлов вдвое больше камеди.",
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
}

impl BrewSpec {
    pub fn real_secs(&self) -> f32 {
        (self.hours * SECS_PER_HOUR).clamp(2.0, MAX_SECS)
    }

    fn dry_mass(&self) -> f32 {
        self.items.iter().map(|(n, c)| mass(n).unwrap_or(0.0) * *c as f32).sum()
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
}

/// Насколько значение похоже на идеал: 1 — точно, 0 — за допуском.
fn closeness(value: f32, ideal: f32, tol: f32) -> f32 {
    if tol <= 0.0 {
        return if (value - ideal).abs() < 1e-3 { 1.0 } else { 0.0 };
    }
    (1.0 - ((value - ideal) / tol).abs()).max(0.0)
}

/// Подходит ли смесь к рецепту и насколько (0 — нет).
fn match_recipe(r: &Recipe, spec: &BrewSpec, vessel: Vessel) -> (f32, Vec<&'static str>) {
    let mut notes = Vec::new();
    let dry = spec.dry_mass();
    if dry <= 0.0 {
        return (0.0, notes);
    }
    let vessel_ok = match r.vessel {
        VesselNeed::Pot => vessel != Vessel::Mortar,
        VesselNeed::Mortar => vessel == Vessel::Mortar,
    };
    if !vessel_ok {
        return (0.0, notes);
    }
    // Всё, что положено, должно быть из рецепта.
    let in_recipe = |n: &str| r.parts.iter().any(|p| p.any_of.contains(&n));
    let foreign: f32 = spec.items.iter().filter(|(n, _)| !in_recipe(n)).map(|(n, c)| mass(n).unwrap_or(0.0) * *c as f32).sum();
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
        let m: f32 = spec.items.iter().filter(|(n, _)| p.any_of.contains(&n.as_str())).map(|(n, c)| mass(n).unwrap_or(0.0) * *c as f32).sum();
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
            score *= 0.5 + 0.5 * c;
        }
    }
    if !any_present {
        return (0.0, notes);
    }
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
            notes.push(if water / dry > wi { "слишком жидко" } else { "слишком густо" });
            score *= 0.3;
        } else {
            score *= 0.4 + 0.6 * c;
        }
    }
    // Огонь.
    if !r.heat.contains(&spec.heat) {
        notes.push(if spec.heat > r.heat[r.heat.len() - 1] { "огонь слишком сильный" } else { "не тот огонь" });
        score *= 0.35;
    }
    // Время.
    let (min, best, max) = r.hours;
    if spec.hours < min {
        notes.push("не довели до конца");
        score *= 0.4 * spec.hours / min;
    } else if max > 0.0 && spec.hours > max {
        notes.push("передержали");
        score *= 0.5;
    } else {
        let c = if spec.hours <= best { 0.7 + 0.3 * (spec.hours - min) / (best - min).max(0.01) } else { 1.0 };
        score *= c;
    }
    // Уваривание.
    if water > 0.0 && spec.heat != Heat::None && r.reduce.1 < 1.0 {
        let gone = (spec.heat.boil_off() * spec.hours / spec.water as f32).min(1.0);
        let c = closeness(gone, r.reduce.0, r.reduce.1);
        if c <= 0.0 {
            notes.push(if gone > r.reduce.0 { "переварили" } else { "недоварили" });
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
pub fn brew(spec: &BrewSpec, vessel: Vessel, k: &Knowledge, flasks: u16) -> BrewResult {
    let mut result = BrewResult { product: None, flasks_used: 0, vessel_broken: false, burn: 0.0, text: String::new(), technique: None };
    let water = spec.water as f32;
    let left = (water - spec.heat.boil_off() * spec.hours).max(0.0);
    // Всухую на огне: пригорает, глина лопается.
    let dry_fire = spec.heat >= Heat::Medium && (water == 0.0 || left <= 0.0);
    let only_fat = spec.items.iter().all(|(n, _)| matches!(n.as_str(), "Воск" | "Бараний жир" | "Кувшин оливкового масла" | "Щёлок"));
    if vessel == Vessel::Mortar && spec.heat != Heat::None {
        result.text = "Ступку на огонь не ставят".into();
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
    if spec.heat == Heat::High && water >= 4.0 && spec.hours > 0.5 {
        // Бурное кипение через край.
        result.burn = 0.15;
    }
    let best = RECIPES.iter().map(|r| (r, match_recipe(r, spec, vessel))).max_by(|a, b| a.1.0.total_cmp(&b.1.0));
    let Some((recipe, (score, notes))) = best.filter(|(_, (s, _))| *s > 0.0) else {
        result.product = Some((SPOILED, 1, 0));
        result.text = "Вышла мутная бурда, ни на что не похожая".into();
        return result;
    };
    result.technique = Some(recipe.technique);
    if recipe.acidic && vessel == Vessel::CopperKettle {
        result.product = Some((SPOILED, 1, 0));
        result.text = "Кислое разъело медь: варево позеленело, пить его — яд".into();
        return result;
    }
    let Some(q) = quality_index(score, k.skill(recipe.technique.skill())) else {
        result.product = Some((SPOILED, 1, 0));
        result.text = format!("Похоже на «{}», но не вышло: {}", recipe.product, if notes.is_empty() { "всё не так".into() } else { notes.join(", ") });
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
    result.product = Some((recipe.product, count, q));
    let mut text = format!("Готово: {} ×{}", recipe.product, count);
    if !notes.is_empty() {
        text.push_str(&format!(" ({})", notes.join(", ")));
    }
    result.text = text;
    result
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

    fn spec(items: &[(&str, u16)], water: u8, heat: Heat, hours: f32) -> BrewSpec {
        BrewSpec { items: items.iter().map(|(n, c)| (n.to_string(), *c)).collect(), water, heat, hours }
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
        for n in [SPOILED, FLASK, "Глиняный горшок", "Медный котелок", "Ступка с пестиком"] {
            assert!(ItemBaseId::find(n).is_some(), "{n}");
        }
    }

    #[test]
    fn real_recipe_works_without_knowledge() {
        // Гален: воск к маслу 1:4, на слабом огне — и неуч сварит мазь.
        let k = Knowledge::default();
        let r = brew(&spec(&[("Воск", 1), ("Кувшин оливкового масла", 1), ("Мирра", 1)], 0, Heat::Low, 1.0), Vessel::ClayPot, &k, 5);
        let (name, n, q) = r.product.expect("что-то вышло");
        assert_eq!(name, "Ранозаживляющая мазь", "{}", r.text);
        assert!(n >= 2);
        assert!(q >= 2, "качество {q}: {}", r.text);
    }

    #[test]
    fn wrong_proportions_fail() {
        let k = Knowledge::default();
        // Воска больше, чем масла.
        let r = brew(&spec(&[("Воск", 4), ("Кувшин оливкового масла", 1)], 0, Heat::Low, 1.0), Vessel::ClayPot, &k, 5);
        assert_eq!(r.product.map(|p| p.0), Some(SPOILED), "{}", r.text);
    }

    #[test]
    fn decoction_needs_water_and_gentle_fire() {
        let k = Knowledge::default();
        let good = brew(&spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5), Vessel::ClayPot, &k, 5);
        assert_eq!(good.product.map(|p| p.0), Some("Отвар шалфея"), "{}", good.text);
        // Без воды на сильном огне глина лопается.
        let burnt = brew(&spec(&[("Шалфей", 2)], 0, Heat::High, 1.0), Vessel::ClayPot, &k, 5);
        assert!(burnt.vessel_broken);
        // Выкипело всё.
        let dry = brew(&spec(&[("Шалфей", 2)], 2, Heat::High, 2.0), Vessel::ClayPot, &k, 5);
        assert_eq!(dry.product.map(|p| p.0), Some(SPOILED));
    }

    #[test]
    fn vinegar_eats_copper() {
        let k = Knowledge::default();
        let r = brew(&spec(&[("Горшок мёда", 1), ("Винный уксус", 1)], 1, Heat::Low, 1.0), Vessel::CopperKettle, &k, 5);
        assert_eq!(r.product.map(|p| p.0), Some(SPOILED), "{}", r.text);
        let r = brew(&spec(&[("Горшок мёда", 1), ("Винный уксус", 1)], 1, Heat::Low, 1.0), Vessel::ClayPot, &k, 5);
        assert_eq!(r.product.map(|p| p.0), Some("Оксимель"), "{}", r.text);
    }

    #[test]
    fn liquids_need_flasks() {
        let k = Knowledge::default();
        let r = brew(&spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5), Vessel::ClayPot, &k, 0);
        assert!(r.product.is_none());
        let r = brew(&spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5), Vessel::ClayPot, &k, 1);
        assert_eq!(r.product.map(|p| p.1), Some(1));
        assert_eq!(r.flasks_used, 1);
    }

    #[test]
    fn poultice_in_mortar() {
        let k = Knowledge::default();
        let r = brew(&spec(&[("Тысячелистник", 1), ("Подорожник", 1)], 0, Heat::None, 0.25), Vessel::Mortar, &k, 0);
        assert_eq!(r.product.map(|p| p.0), Some("Кровоостанавливающая припарка"), "{}", r.text);
    }

    #[test]
    fn skill_improves_quality() {
        let novice = Knowledge::default();
        let mut master = Knowledge::default();
        for _ in 0..400 {
            master.practice(Skill::Herbalism, 1.0);
        }
        let s = spec(&[("Шалфей", 2)], 4, Heat::Low, 1.5);
        let a = brew(&s, Vessel::ClayPot, &novice, 5).product.unwrap().2;
        let b = brew(&s, Vessel::ClayPot, &master, 5).product.unwrap().2;
        assert!(b > a, "{a} {b}");
    }

    #[test]
    fn theriac_and_vinegar() {
        let k = Knowledge::default();
        let t = brew(
            &spec(&[("Опий", 2), ("Мирра", 1), ("Ягоды можжевельника", 1), ("Корица", 1), ("Шафран", 1), ("Перец", 1), ("Горшок мёда", 1)], 0, Heat::Low, 12.0),
            Vessel::ClayPot,
            &k,
            5,
        );
        assert_eq!(t.product.map(|p| p.0), Some("Териак"), "{}", t.text);
        let v = brew(&spec(&[("Кувшин вина", 1)], 0, Heat::None, 72.0), Vessel::ClayPot, &k, 10);
        assert_eq!(v.product.map(|p| p.0), Some("Винный уксус"), "{}", v.text);
    }
}
