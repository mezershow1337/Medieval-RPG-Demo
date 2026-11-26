//! Лавки главной улицы: в каждой комнате-лавке первого этажа (`level::ShopSpec`) за каменным
//! прилавком стоит продавец со своим ремеслом — горшечник, хлебник, медник, торговец полотном…
//! Товар лежит на прилавке настоящими вещами (`trade_shelves.rs`), торгуют обычным окном лавки.
//!
//! Лавки работают днём: к вечеру продавец запирает ставни и уходит домой, товар убирает,
//! утром открывает снова (`shop_hours.rs`). Часы у каждой лавки свои, около 6–20 ч;
//! корчма по закону эпарха закрывается во второй час ночи (около 21 ч).
//!
//! Продавцы получают номера [`MerchantId`] следом за торговцами из `trade::MERCHANTS`;
//! кто где стоит — чистая функция от уровня, одинаковая у сервера и клиентов.

use crate::factions::Faction;
use crate::level::{ShopKind, ShopSpec};
use crate::trade::{Category, Look, MERCHANTS, MerchantDef, MerchantId, MoneyPolicy, StallBox, StockDef};
use bevy::prelude::*;

/// Чем торгует лавка.
pub struct Wares {
    /// Короткий ключ ремесла (для ключа продавца и тестов).
    pub key: &'static str,
    /// Ремесло — подпись под именем.
    pub title: &'static str,
    pub greeting: &'static str,
    pub stock: &'static [StockDef],
    pub buys: &'static [(Category, f32)],
    /// Цвет туники (у каждого ремесла свой).
    pub tunic: [f32; 3],
    /// Сколько денег в кассе (нуммии).
    pub cash: u32,
}

const fn s(item: &'static str, min: u16, max: u16, markup: f32) -> StockDef {
    StockDef { item, min, max, markup }
}

/// Нуммиев в солиде (касса лавок — в солидах).
const SOLIDUS: u32 = 7200;

/// Ремёсла обычных лавок: соседние лавки получают разные (см. [`wares_for`]).
pub const COMMON_WARES: &[Wares] = &[
    Wares {
        key: "potter",
        title: "горшечник",
        greeting: "Горшки, миски, кружки, светильники — обожжены на совесть, не текут. Стукни — звенят.",
        stock: &[s("Глиняный горшок", 3, 8, 1.0), s("Глиняная кружка", 4, 10, 1.0), s("Краснолаковая миска", 3, 8, 1.05), s("Глиняный светильник", 3, 8, 1.0)],
        buys: &[(Category::Vessel, 0.8)],
        tunic: [0.62, 0.42, 0.3],
        cash: SOLIDUS,
    },
    Wares {
        key: "baker",
        title: "хлебник",
        greeting: "Хлеб утренний, ещё тёплый. Сыр с хоры, смоквы, мёд — всё к столу.",
        stock: &[s("Каравай хлеба", 6, 14, 1.0), s("Головка сыра", 2, 5, 1.05), s("Сушёные смоквы", 4, 10, 1.05), s("Горшок мёда", 1, 3, 1.1)],
        buys: &[(Category::Food, 0.8)],
        tunic: [0.85, 0.8, 0.68],
        cash: SOLIDUS,
    },
    Wares {
        key: "fishmonger",
        title: "рыботорговец",
        greeting: "Хамса с ночного лова, кефаль, барабулька. Солёная — на дорогу, свежая — на сковороду.",
        stock: &[s("Хамса", 6, 16, 1.0), s("Ставрида", 3, 8, 1.0), s("Барабулька", 3, 8, 1.0), s("Кефаль", 1, 4, 1.0), s("Солёная рыба", 4, 10, 1.0), s("Мешок соли", 1, 3, 1.05)],
        buys: &[(Category::Food, 0.85)],
        tunic: [0.4, 0.5, 0.55],
        cash: SOLIDUS,
    },
    Wares {
        key: "vintner",
        title: "виноторговец",
        greeting: "Вино с херсонских виноградников и оливковое масло из-за моря. Пробуй, не стесняйся.",
        stock: &[s("Кувшин вина", 3, 8, 1.0), s("Кувшин оливкового масла", 2, 6, 1.1), s("Винный уксус", 2, 5, 1.0), s("Глиняная кружка", 2, 6, 1.0)],
        buys: &[(Category::WineOil, 0.85), (Category::Vessel, 0.5)],
        tunic: [0.55, 0.22, 0.25],
        cash: SOLIDUS * 2,
    },
    Wares {
        key: "draper",
        title: "торговец полотном",
        greeting: "Лён, шерсть, пряжа. На рубаху, на плащ, на бинты — отмерю сколько скажешь.",
        stock: &[s("Отрез льна", 2, 6, 1.0), s("Отрез шерсти", 2, 5, 1.0), s("Моток пряжи", 4, 10, 1.0), s("Веретено с пряслицем", 1, 3, 1.0), s("Железные ножницы", 1, 2, 1.1), s("Бинт", 4, 10, 1.0)],
        buys: &[(Category::Textile, 0.85)],
        tunic: [0.32, 0.38, 0.6],
        cash: SOLIDUS * 2,
    },
    Wares {
        key: "tanner",
        title: "кожевник",
        greeting: "Овчина, войлок, сапоги, перчатки. Кожа своей выделки — дубил сам, на дубовой коре.",
        stock: &[s("Овчина", 2, 5, 1.0), s("Войлок", 2, 4, 1.0), s("Сырая шкура", 1, 3, 1.0), s("Кожаные сапоги", 1, 2, 1.0), s("Кожаные перчатки", 1, 3, 1.0)],
        buys: &[(Category::Hide, 0.9)],
        tunic: [0.45, 0.33, 0.22],
        cash: SOLIDUS * 2,
    },
    Wares {
        key: "coppersmith",
        title: "медник",
        greeting: "Котелки, кувшины, светильники, весы — медь и бронза, лужёные. Старую медь беру в лом.",
        stock: &[s("Медный котелок", 1, 3, 1.0), s("Бронзовый кувшин", 1, 3, 1.0), s("Бронзовый светильник", 1, 3, 1.0), s("Весы с гирями", 1, 2, 1.0), s("Ступка с пестиком", 1, 2, 1.0)],
        buys: &[(Category::Vessel, 0.75), (Category::Material, 0.8)],
        tunic: [0.6, 0.45, 0.25],
        cash: SOLIDUS * 3,
    },
    Wares {
        key: "haberdasher",
        title: "торговец мелочным товаром",
        greeting: "Гребни, зеркальца, пряжки, фибулы. Подарок жене, невесте или себе — найдётся.",
        stock: &[s("Костяной гребень", 2, 5, 1.0), s("Бронзовое зеркальце", 1, 3, 1.05), s("Бронзовая фибула", 2, 4, 1.0), s("Бронзовая пряжка", 2, 5, 1.0), s("Серебряная ложка", 1, 2, 1.1), s("Деревянная шкатулка", 1, 2, 1.0)],
        buys: &[(Category::Jewelry, 0.8)],
        tunic: [0.5, 0.4, 0.55],
        cash: SOLIDUS * 3,
    },
    Wares {
        key: "spicer",
        title: "торговец пряностями",
        greeting: "Перец из Индии, корица, шафран, мастика с Хиоса. Дорого — а ты понюхай.",
        stock: &[s("Перец", 2, 6, 1.15), s("Шафран", 1, 3, 1.2), s("Корица", 2, 5, 1.15), s("Хиосская мастика", 2, 5, 1.15), s("Ладан", 2, 5, 1.1), s("Мирра", 1, 3, 1.2)],
        buys: &[(Category::Import, 0.85)],
        tunic: [0.7, 0.55, 0.2],
        cash: SOLIDUS * 4,
    },
    Wares {
        key: "chandler",
        title: "свечник и мыловар",
        greeting: "Воск, мыло, щёлок, масло для светильников. Без меня в городе темно и грязно.",
        stock: &[s("Воск", 4, 10, 1.0), s("Мыло", 3, 8, 1.0), s("Щёлок", 1, 4, 1.0), s("Глиняный светильник", 2, 6, 1.0), s("Кувшин оливкового масла", 1, 4, 1.1)],
        buys: &[(Category::Material, 0.8), (Category::Food, 0.6)],
        tunic: [0.75, 0.7, 0.5],
        cash: SOLIDUS,
    },
    Wares {
        key: "cutler",
        title: "ножовщик",
        greeting: "Ножи, ножницы, топоры, заступы. Сталь звонкая, точу при тебе.",
        stock: &[s("Кухонный нож", 2, 5, 1.0), s("Кинжал", 1, 2, 1.05), s("Железные ножницы", 1, 3, 1.0), s("Плотничий топор", 1, 2, 1.0), s("Заступ", 1, 2, 1.0), s("Топор дровосека", 1, 2, 1.0)],
        buys: &[(Category::Weapon, 0.6), (Category::Material, 0.75)],
        tunic: [0.35, 0.35, 0.38],
        cash: SOLIDUS * 3,
    },
    Wares {
        key: "glazier",
        title: "торговец стеклом",
        greeting: "Стекло из мастерской у порта: чаши, кубки, флаконы для снадобий. Бери осторожно.",
        stock: &[s("Стеклянная чаша", 1, 3, 1.05), s("Стеклянный кубок", 2, 4, 1.05), s("Пустой флакон", 3, 8, 1.0)],
        buys: &[(Category::Vessel, 0.75)],
        tunic: [0.4, 0.58, 0.55],
        cash: SOLIDUS * 2,
    },
    Wares {
        key: "devotional",
        title: "торговец святынями",
        greeting: "Кресты, энколпионы, ампулы святого Мины, ладан. Освящены в базилике — не сомневайся.",
        stock: &[s("Нательный крест", 1, 3, 1.0), s("Энколпион", 1, 2, 1.1), s("Ампула святого Мины", 1, 3, 1.0), s("Ладан", 2, 5, 1.05), s("Псалтирь", 0, 1, 1.1)],
        buys: &[(Category::Jewelry, 0.7)],
        tunic: [0.3, 0.28, 0.3],
        cash: SOLIDUS * 2,
    },
    Wares {
        key: "toymaker",
        title: "игрушечник",
        greeting: "Лошадки детям, кости и тавлеи отцам. Играть — не грех, если не на последнее.",
        stock: &[s("Деревянная лошадка", 1, 3, 1.0), s("Игральные кости", 3, 8, 1.0), s("Доска для тавлей", 1, 2, 1.0), s("Деревянная шкатулка", 1, 2, 1.0)],
        buys: &[(Category::Jewelry, 0.5)],
        tunic: [0.6, 0.55, 0.3],
        cash: SOLIDUS,
    },
    Wares {
        key: "tackle",
        title: "торговец снастями",
        greeting: "Удочки, крючки, грузила, наживка. Рыба в бухте сама не поймается.",
        stock: &[s("Тростниковая удочка", 1, 2, 1.0), s("Бронзовые крючки", 5, 15, 1.0), s("Свинцовые грузила", 4, 10, 1.0), s("Морские черви", 6, 20, 1.0), s("Креветки", 4, 12, 1.0)],
        buys: &[(Category::Food, 0.6)],
        tunic: [0.45, 0.52, 0.42],
        cash: SOLIDUS,
    },
    Wares {
        key: "herbalist",
        title: "зеленщик-травник",
        greeting: "Сушёные травы с хоры: шалфей, мята, чабрец, шиповник. От кашля, от живота, к чаю.",
        stock: &[s("Сушёный шалфей", 2, 6, 1.0), s("Сушёная мята", 2, 6, 1.0), s("Сушёный чабрец", 2, 6, 1.0), s("Сушёный шиповник", 2, 6, 1.0), s("Ягоды можжевельника", 2, 5, 1.0)],
        buys: &[(Category::Herb, 0.85)],
        tunic: [0.42, 0.55, 0.35],
        cash: SOLIDUS,
    },
];

/// Книжная лавка у агоры.
pub const BOOKSELLER: Wares = Wares {
    key: "stationer",
    title: "книжная лавка, писчий товар",
    greeting: "Пергамент, чернила, перья, таблички. Псалтирь для дома — переписана чисто.",
    stock: &[s("Тетрадь пергамента", 4, 12, 1.0), s("Чернила", 2, 6, 1.0), s("Тростниковое перо", 4, 12, 1.0), s("Восковая табличка", 2, 5, 1.0), s("Псалтирь", 1, 2, 1.0), s("Обрывок травника", 0, 1, 1.0)],
    buys: &[(Category::Book, 0.9)],
    tunic: [0.35, 0.3, 0.45],
    cash: SOLIDUS * 3,
};

/// Корчма у главной улицы.
pub const TAVERN: Wares = Wares {
    key: "taverner",
    title: "корчмарь",
    greeting: "Вино, хлеб, сыр, рыбка — садись, путник. Во второй час ночи закрываю: закон эпарха.",
    stock: &[s("Кувшин вина", 3, 8, 1.1), s("Каравай хлеба", 3, 8, 1.1), s("Головка сыра", 1, 4, 1.1), s("Солёная рыба", 2, 6, 1.1), s("Глиняная кружка", 2, 6, 1.0)],
    buys: &[(Category::WineOil, 0.8), (Category::Food, 0.7)],
    tunic: [0.5, 0.3, 0.2],
    cash: SOLIDUS * 2,
};

/// Имена продавцов (у торговцев из `trade::MERCHANTS` — свои, не повторяются).
pub const NAMES: &[&str] = &[
    "Георгий", "Константин", "Василий", "Димитрий", "Николай", "Сергий", "Пётр", "Павел", "Фома", "Лука", "Марк", "Хрисанф", "Евфимий", "Мина", "Прокопий",
    "Пантелеимон", "Афанасий", "Кирилл", "Ипатий", "Исидор", "Неофит", "Онисим", "Пафнутий", "Симеон", "Трифон", "Фотий", "Феофан", "Евгений", "Аркадий",
    "Гликерий", "Дорофей", "Елевферий", "Ермолай", "Иларион", "Кирик", "Лаврентий", "Македоний", "Нектарий", "Орест", "Панкратий", "Родион", "Сосипатр",
    "Тимофей", "Филофей", "Анфим", "Галактион", "Диодор", "Евлампий", "Зосима", "Каллистрат", "Ксенофонт", "Мартирий", "Никандр", "Савватий", "Тарасий",
    "Феодот", "Фирс", "Акакий", "Авксентий", "Созонт", "Фока", "Леонтий", "Анастасий", "Варда", "Агафон", "Вонифатий", "Герман", "Досифей", "Евсевий",
    "Иерон", "Клеоник", "Мелетий", "Нил", "Паисий", "Полиевкт", "Сисиний", "Феофил", "Харалампий", "Аполлос", "Архипп", "Вавила", "Власий",
    "Гавриил", "Григорий", "Давид", "Евагрий", "Евдоким", "Евстратий", "Захария", "Иаков", "Игнатий", "Иосиф", "Кесарий", "Кондрат", "Косма", "Дамиан",
    "Лонгин", "Максим", "Мамант", "Модест", "Нифонт", "Пимен", "Платон", "Порфирий", "Руф", "Самуил", "Сильвестр", "Спиридон", "Филарет", "Флор", "Лавр",
];

/// Ремесло обычной лавки номер `k` (по порядку обычных лавок): шаг 7 по списку из 16 —
/// соседние лавки торгуют разным, а все ремёсла есть в городе.
pub fn wares_for(k: usize) -> &'static Wares {
    &COMMON_WARES[(k * 7) % COMMON_WARES.len()]
}

/// Часы работы лавки номер `shop` (часы суток: открывается, закрывается).
pub fn hours_of(shop: usize, kind: ShopKind) -> [f32; 2] {
    match kind {
        // Во второй час ночи корчмы запирали (Книга эпарха).
        ShopKind::Tavern => [7.0, 21.0],
        ShopKind::Apothecary => [7.0, 20.0],
        // Лавочники открываются с рассветом и запирают в сумерки, каждый чуть по-своему.
        _ => {
            let h = crate::world_clock::hash(shop as u64, 0x5409);
            [6.0 + 0.25 * (h % 4) as f32, 19.5 + 0.25 * ((h >> 8) % 3) as f32]
        }
    }
}

/// Открыта ли лавка в час `hour`.
pub fn is_open(hours: [f32; 2], hour: f32) -> bool {
    hour >= hours[0] && hour < hours[1]
}

/// «6:30–20:00».
pub fn hours_text(hours: [f32; 2]) -> String {
    let hm = |h: f32| {
        let m = (h * 60.0).round() as u32;
        format!("{}:{:02}", m / 60, m % 60)
    };
    format!("{}–{}", hm(hours[0]), hm(hours[1]))
}

/// Продавец лавки.
pub struct Seller {
    /// Номер лавки в `level::shops()`.
    pub shop: usize,
    #[allow(dead_code)] // номер торговца — для тестов и отладки
    pub id: MerchantId,
    pub def: MerchantDef,
    pub hours: [f32; 2],
}

/// Продавцы всех лавок уровня (в аптеках торгуют аптекари из `trade::MERCHANTS`).
pub fn sellers() -> &'static [Seller] {
    static ALL: std::sync::OnceLock<Vec<Seller>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| build_sellers(crate::level::shops()))
}

fn build_sellers(shops: &[ShopSpec]) -> Vec<Seller> {
    let mut out = Vec::new();
    let mut common = 0;
    for (k, shop) in shops.iter().enumerate() {
        let wares: &'static Wares = match shop.kind {
            ShopKind::Apothecary => continue,
            ShopKind::Bookseller => &BOOKSELLER,
            ShopKind::Tavern => &TAVERN,
            ShopKind::Common => {
                common += 1;
                wares_for(common - 1)
            }
        };
        let n = out.len();
        let name = NAMES[n % NAMES.len()];
        let key: &'static str = Box::leak(format!("shop{k}_{}", wares.key).into_boxed_str());
        let def = MerchantDef {
            key,
            name,
            title: wares.title,
            faction: Faction::Citizens,
            place: "shop",
            greeting: wares.greeting,
            stock: wares.stock,
            buys: wares.buys,
            money: MoneyPolicy::Coins,
            vigilance: 0.5 + 0.1 * (k % 3) as f32,
            cash: wares.cash,
            orders: &[],
            lessons: &[],
            services: &[],
            look: Look { tunic: wares.tunic, trousers: [0.3, 0.27, 0.24], helmet: false },
        };
        out.push(Seller { shop: k, id: MerchantId((MERCHANTS.len() + n) as u16), def, hours: hours_of(k, shop.kind) });
    }
    out
}

/// Продавец лавки по номеру торговца (`None` — торговец не из лавок).
pub fn seller(id: MerchantId) -> Option<&'static Seller> {
    (id.0 as usize).checked_sub(MERCHANTS.len()).and_then(|k| sellers().get(k))
}

/// Часы работы торговца: продавцы лавок и аптекари в лавках у агоры и порта. Остальные
/// (лотки на агоре, купцы порта, мастера, станы за стенами) торгуют, пока в мире светло и темно.
pub fn merchant_hours(id: MerchantId) -> Option<[f32; 2]> {
    if let Some(s) = seller(id) {
        return Some(s.hours);
    }
    let def = id.def();
    matches!(def.place, "apothecary_agora" | "apothecary_port").then(|| hours_of(0, ShopKind::Apothecary))
}

/// Где встаёт продавец лавки, куда смотрит и где на прилавке его товар (в системе продавца).
pub fn seller_spot(shop: &ShopSpec) -> (Vec3, f32, Vec<StallBox>) {
    let pos = shop.seller + Vec3::Y * 0.1;
    let yaw = f32::atan2(-shop.facing.x, -shop.facing.z);
    let inv = Quat::from_rotation_y(yaw).inverse();
    let stall = shop
        .goods
        .iter()
        .flatten()
        .map(|(a, c)| {
            // Столешница: коробка от пола до верха прилавка (товар ляжет на её верх).
            let lo = a.with_y(shop.seller.y);
            let hi = c.with_y(a.y);
            let center = inv * ((lo + hi) / 2.0 - pos);
            let half = (inv * ((hi - lo) / 2.0)).abs();
            StallBox { center: center.to_array(), half: half.to_array(), felt: false, fixed: true }
        })
        .collect();
    (pos, yaw, stall)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemBaseId;

    #[test]
    fn wares_exist_and_differ() {
        let mut keys = std::collections::HashSet::new();
        for w in COMMON_WARES.iter().chain([&BOOKSELLER, &TAVERN]) {
            assert!(keys.insert(w.key), "повтор ремесла {}", w.key);
            assert!(!w.stock.is_empty() && !w.buys.is_empty(), "{}", w.key);
            for st in w.stock {
                assert!(ItemBaseId::find(st.item).is_some(), "{}: нет предмета «{}»", w.key, st.item);
                assert!(st.min <= st.max && st.max > 0, "{}: {}", w.key, st.item);
                assert!(crate::economy::base_price(ItemBaseId::by_name(st.item)) > 0, "{}: у «{}» нет цены", w.key, st.item);
            }
        }
        // Шаг по списку: соседи разные, все ремёсла в ходу.
        let n = COMMON_WARES.len();
        for k in 0..n {
            assert_ne!(wares_for(k).key, wares_for(k + 1).key);
        }
        let all: std::collections::HashSet<_> = (0..n).map(|k| wares_for(k).key).collect();
        assert_eq!(all.len(), n);
    }

    #[test]
    fn names_are_unique_and_not_taken() {
        let mut seen = std::collections::HashSet::new();
        for n in NAMES {
            assert!(seen.insert(*n), "повтор имени {n}");
            assert!(!MERCHANTS.iter().any(|m| m.name.split(['-', ' ']).any(|w| w == *n)), "имя {n} уже у торговца");
        }
    }

    #[test]
    fn hours_are_daytime() {
        for k in 0..50 {
            for kind in [ShopKind::Common, ShopKind::Tavern, ShopKind::Apothecary, ShopKind::Bookseller] {
                let h = hours_of(k, kind);
                assert!(h[0] >= 6.0 && h[0] <= 7.0 && h[1] >= 19.5 && h[1] <= 21.0, "{h:?}");
                assert!(is_open(h, 12.0) && !is_open(h, 23.0) && !is_open(h, 3.0) && !is_open(h, 5.5));
            }
        }
        assert_eq!(hours_text([6.5, 20.0]), "6:30–20:00");
    }

    /// Каждая лавка главной улицы (кроме аптек со своими аптекарями) — с продавцом;
    /// продавец стоит свободно за прилавком, товар ложится на прилавок, ставни есть.
    #[test]
    fn every_shop_has_a_seller_behind_the_counter() {
        let shops = crate::level::shops();
        assert!(shops.len() >= 20, "лавок {}", shops.len());
        let level = crate::level::city_colliders();
        let body = crate::collision::Body::STANDING;
        let sellers = sellers();
        for (k, shop) in shops.iter().enumerate() {
            assert!(shop.door.is_some(), "лавка {k}: нет ставней");
            let d = crate::level::shared().doors[shop.door.unwrap() as usize];
            assert!(d.double && d.open, "лавка {k}: ставни не двустворчатые или закрыты днём");
            if shop.kind == ShopKind::Apothecary {
                assert!(sellers.iter().all(|s| s.shop != k));
                continue;
            }
            let s = sellers.iter().find(|s| s.shop == k).unwrap_or_else(|| panic!("лавка {k} без продавца"));
            let (pos, yaw, stall) = seller_spot(shop);
            assert!(level.body_is_free(&body, pos), "лавка {k}: продавец в стене или в вещах ({pos})");
            assert!(shop.contains(pos, 0.0), "лавка {k}: продавец вне лавки");
            assert!(!stall.is_empty(), "лавка {k}: на прилавке нет места");
            // Столешница совпадает с верхом каменного прилавка, лежит перед продавцом.
            for t in crate::trade::stall_obbs(pos, yaw, &stall) {
                let a = t.aabb();
                assert!((a.max.y - shop.counter_max.y).abs() < 0.01, "лавка {k}: товар не на прилавке");
                assert!(a.min.x >= shop.counter_min.x - 0.01 && a.max.x <= shop.counter_max.x + 0.01, "лавка {k}");
                assert!(a.min.z >= shop.counter_min.z - 0.01 && a.max.z <= shop.counter_max.z + 0.01, "лавка {k}");
                let local = Quat::from_rotation_y(yaw).inverse() * (t.center - pos);
                assert!(local.z < -0.3, "лавка {k}: прилавок не перед продавцом");
            }
            // Покупатель встаёт перед прилавком и дотягивается до продавца, но не проходит насквозь.
            let forward = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            assert!(!level.body_is_free(&body, pos + forward * 0.95), "лавка {k}: сквозь прилавок проходят");
            let buyer = (30..=60).map(|i| i as f32 * 0.05).find(|d| level.body_is_free(&body, pos + forward * d));
            assert!(buyer.is_some_and(|d| d < crate::trade_net::TALK_RANGE), "лавка {k}: покупателю негде встать");
            let def = &s.def;
            assert_eq!(s.id.def().key, def.key);
            assert!(is_open(s.hours, 12.0) && !is_open(s.hours, 23.0));
        }
        // Имена продавцов не повторяются.
        let names: std::collections::HashSet<_> = sellers.iter().map(|s| s.def.name).collect();
        assert_eq!(names.len(), sellers.len(), "имён меньше, чем продавцов: {}", sellers.len());
    }
}

#[cfg(test)]
mod dump {
    /// Список лавок: `cargo test --release dump_shops -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dump_shops() {
        for s in super::sellers() {
            let shop = &crate::level::shops()[s.shop];
            println!("{:3} {:28} {:14} seller ({:7.2},{:5.2},{:7.2}) facing ({:4.1},{:4.1}) door {:?} hours {:?}", s.shop, s.def.title, s.def.name, shop.seller.x, shop.seller.y, shop.seller.z, shop.facing.x, shop.facing.z, shop.door, s.hours);
        }
    }
}
