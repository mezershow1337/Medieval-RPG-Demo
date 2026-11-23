//! Деньги и цены Херсона около 510 года.
//!
//! # Монеты
//!
//! После денежной реформы императора Анастасия (498 г.) в ходу:
//! - **солид** (номисма) — золото, 1/72 римского фунта (≈4,5 г). Им платят жалованье, подати, крупные сделки;
//! - **тремисс** — золотая треть солида;
//! - **фоллис** — крупная медная монета с буквой «M» (40 нуммиев), повседневные расчёты;
//! - **декануммий** — медь с буквой «I» (10 нуммиев), мелочь.
//!
//! Курс в начале VI века — около 180 фоллисов за солид (7200 нуммиев), его и берём.
//! Все цены в коде — в нуммиях (`u32`), так считать проще и без дробей.
//!
//! Ориентиры (папирусы Египта VI века, Кодекс Юстиниана): поденщик зарабатывает
//! около 1/30 солида в день (≈6 фоллисов), солдат-лимитаней — несколько солидов в год,
//! хорошая кольчуга стоит пару солидов, переписанная книга — от одного до десятка солидов.

use crate::inventory::{Inventory, Target};
use crate::items::{ITEM_BASES, Item, ItemBaseId, ItemShape, Quality, UidGen};

pub const NUMMI_PER_FOLLIS: u32 = 40;
pub const NUMMI_PER_SOLIDUS: u32 = 7200;

/// Монеты от крупной к мелкой: имя и стоимость в нуммиях.
pub const DENOMINATIONS: [(&str, u32); 4] = [
    ("Солид", NUMMI_PER_SOLIDUS),
    ("Тремисс", 2400),
    ("Фоллис", NUMMI_PER_FOLLIS),
    ("Декануммий", 10),
];

/// Самая мелкая монета: цены округляются до неё.
pub const SMALLEST_COIN: u32 = 10;

/// Стоимость одной монеты, если предмет — монета.
pub fn coin_value(base: ItemBaseId) -> Option<u32> {
    let name = base.base().name;
    DENOMINATIONS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

/// Золотая ли монета (кочевники и аланы берут только золото).
pub fn is_gold_coin(base: ItemBaseId) -> bool {
    coin_value(base).is_some_and(|v| v >= 2400)
}

/// Сколько денег в инвентаре (нуммии).
pub fn purse(inv: &Inventory) -> u32 {
    inv.all_items()
        .filter_map(|i| coin_value(i.base).map(|v| v * i.count as u32))
        .sum()
}

/// Разложить сумму на монеты (жадно, от крупных). Остаток меньше декануммия пропадает.
/// Возвращает стопки не больше `max_stack`.
pub fn coins_for(nummi: u32) -> Vec<(ItemBaseId, u16)> {
    let mut rest = nummi;
    let mut out = Vec::new();
    for (name, value) in DENOMINATIONS {
        let Some(base) = ItemBaseId::find(name) else {
            continue;
        };
        let mut n = rest / value;
        rest -= n * value;
        let max = base.base().max_stack.max(1) as u32;
        while n > 0 {
            let take = n.min(max);
            out.push((base, take as u16));
            n -= take;
        }
    }
    out
}

/// Создать монеты на сумму.
pub fn mint(nummi: u32, uids: &mut UidGen) -> Vec<Item> {
    coins_for(nummi)
        .into_iter()
        .map(|(base, count)| Item {
            uid: uids.next(),
            base,
            quality: Quality::Common,
            affixes: Vec::new(),
            count,
            make: None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyError {
    NotEnough,
    /// Сдача не помещается в сумку.
    NoSpace,
}

impl MoneyError {
    pub fn message(self) -> &'static str {
        match self {
            MoneyError::NotEnough => "Не хватает денег",
            MoneyError::NoSpace => "Сдачу некуда положить: в сумке нет места",
        }
    }
}

/// Положить деньги в инвентарь. При нехватке места ничего не меняется.
pub fn give_money(inv: &mut Inventory, nummi: u32, uids: &mut UidGen) -> Result<(), MoneyError> {
    let backup = inv.clone();
    for coin in mint(nummi, uids) {
        if inv.place(coin, Target::Auto).is_err() {
            *inv = backup;
            return Err(MoneyError::NoSpace);
        }
    }
    Ok(())
}

/// Заплатить сумму монетами из инвентаря.
///
/// Сначала платим без сдачи, от крупных монет к мелким (ничего не переплачивая),
/// а остаток покрываем одной монетой покрупнее и получаем сдачу — как на рынке.
/// При ошибке инвентарь не меняется.
pub fn take_money(inv: &mut Inventory, nummi: u32, uids: &mut UidGen) -> Result<(), MoneyError> {
    if nummi == 0 {
        return Ok(());
    }
    if purse(inv) < nummi {
        return Err(MoneyError::NotEnough);
    }
    let backup = inv.clone();
    let mut rest = nummi;
    for (name, value) in DENOMINATIONS {
        let Some(base) = ItemBaseId::find(name) else {
            continue;
        };
        while rest >= value {
            if !remove_coins(inv, base, 1) {
                break;
            }
            rest -= value;
        }
    }
    if rest > 0 {
        // Разменять самую мелкую из монет, что больше остатка.
        let coin = DENOMINATIONS
            .iter()
            .rev()
            .filter(|(_, v)| *v > rest)
            .find_map(|(n, v)| {
                let base = ItemBaseId::find(n)?;
                inv.all_items()
                    .any(|i| i.base == base)
                    .then_some((base, *v))
            });
        let Some((base, value)) = coin else {
            *inv = backup;
            return Err(MoneyError::NotEnough);
        };
        remove_coins(inv, base, 1);
        if let Err(e) = give_money(inv, value - rest, uids) {
            *inv = backup;
            return Err(e);
        }
    }
    Ok(())
}

/// Убрать `count` монет вида `base` (из любых стопок). false — столько нет.
fn remove_coins(inv: &mut Inventory, base: ItemBaseId, count: u16) -> bool {
    let have: u32 = inv
        .all_items()
        .filter(|i| i.base == base)
        .map(|i| i.count as u32)
        .sum();
    if have < count as u32 {
        return false;
    }
    let mut left = count;
    let uids: Vec<u64> = inv
        .all_items()
        .filter(|i| i.base == base)
        .map(|i| i.uid)
        .collect();
    for uid in uids {
        if left == 0 {
            break;
        }
        if let Some(p) = inv.grid.items.iter_mut().find(|p| p.item.uid == uid) {
            let take = left.min(p.item.count);
            p.item.count -= take;
            left -= take;
        }
    }
    inv.grid.items.retain(|p| p.item.count > 0);
    true
}

/// Сколько стоит одна штука товара обычного качества (справочная цена Херсона, нуммии).
pub fn base_price(base: ItemBaseId) -> u32 {
    if let Some(v) = coin_value(base) {
        return v;
    }
    let info = base.base();
    if let Some(a) = crate::arsenal::of_base(base) {
        return a.price;
    }
    if let Some(p) = PRICE_LIST
        .iter()
        .find(|(n, _)| *n == info.name)
        .map(|(_, p)| *p)
    {
        return p;
    }
    // Чего нет в списке: оцениваем по виду предмета.
    match (info.shape, info.slot) {
        (ItemShape::Blade | ItemShape::Blunt | ItemShape::Polearm, _) => 3600,
        (ItemShape::Shield, _) => 1200,
        (ItemShape::Armor | ItemShape::Helmet, _) => 2400,
        (ItemShape::Small, Some(_)) => 1600,
        (ItemShape::Flask, _) => 240,
        _ => 160,
    }
}

/// Множитель цены по качеству.
pub fn quality_price(q: Quality) -> f32 {
    [0.45, 1.0, 1.6, 2.6, 4.5, 8.0][q.index()]
}

/// Справочная цена стопки (как в подсказке инвентаря).
pub fn item_value(item: &Item) -> u32 {
    if let Some(v) = coin_value(item.base) {
        return v * item.count as u32;
    }
    round_price(base_price(item.base) as f32 * quality_price(item.quality))
        * item.count.max(1) as u32
}

/// Округлить до мелкой монеты (не меньше одного декануммия).
pub fn round_price(nummi: f32) -> u32 {
    ((nummi / SMALLEST_COIN as f32).round().max(1.0) as u32) * SMALLEST_COIN
}

/// Склонение: 1 солид, 2 солида, 5 солидов.
pub fn plural<'a>(n: u32, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let (n10, n100) = (n % 10, n % 100);
    if n10 == 1 && n100 != 11 {
        one
    } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
        few
    } else {
        many
    }
}

/// «2 солида 15 фоллисов», «3 фоллиса 20 нуммиев», «ничего».
pub fn format_money(nummi: u32) -> String {
    let sol = nummi / NUMMI_PER_SOLIDUS;
    let fol = nummi % NUMMI_PER_SOLIDUS / NUMMI_PER_FOLLIS;
    let num = nummi % NUMMI_PER_FOLLIS;
    let mut parts = Vec::new();
    if sol > 0 {
        parts.push(format!(
            "{sol} {}",
            plural(sol, "солид", "солида", "солидов")
        ));
    }
    if fol > 0 {
        parts.push(format!(
            "{fol} {}",
            plural(fol, "фоллис", "фоллиса", "фоллисов")
        ));
    }
    if num > 0 {
        parts.push(format!(
            "{num} {}",
            plural(num, "нуммий", "нуммия", "нуммиев")
        ));
    }
    if parts.is_empty() {
        "ничего".to_string()
    } else {
        parts.join(" ")
    }
}

/// Короткая запись для списков: «2 сол. 15 ф.», «20 н.».
pub fn format_money_short(nummi: u32) -> String {
    let sol = nummi / NUMMI_PER_SOLIDUS;
    let fol = nummi % NUMMI_PER_SOLIDUS / NUMMI_PER_FOLLIS;
    let num = nummi % NUMMI_PER_FOLLIS;
    let mut parts = Vec::new();
    if sol > 0 {
        parts.push(format!("{sol} сол."));
    }
    if fol > 0 {
        parts.push(format!("{fol} ф."));
    }
    if num > 0 {
        parts.push(format!("{num} н."));
    }
    if parts.is_empty() {
        "0".to_string()
    } else {
        parts.join(" ")
    }
}

/// Справочные цены за штуку, нуммии. Сюда же — предметы треда знаний и алхимии
/// (их имена из /mnt/project-files/alchemy/items-and-api.md); пока предмета нет в игре, строка просто не используется.
const PRICE_LIST: &[(&str, u32)] = &[
    // Оружие и доспехи. Железо в Херсоне привозное, кольчуга — целое состояние.
    ("Спата", 4800),
    ("Кавалерийская спата", 7200),
    ("Булава", 2400),
    ("Боевой топор", 1800),
    ("Пехотное копьё", 3600),
    ("Кинжал", 720),
    ("Овальный щит", 1440),
    ("Круглый щит", 720),
    ("Спангенхельм", 4800),
    ("Гребенчатый шлем", 2400),
    ("Стёганая туника", 1200),
    ("Кольчуга", 14400),
    ("Ламеллярный панцирь", 21600),
    ("Ламеллярные наручи", 3600),
    ("Кожаные перчатки", 240),
    ("Железные поножи", 4800),
    ("Кожаные сапоги", 400),
    ("Серебряный перстень", 1200),
    ("Нательный крест", 2400),
    ("Бинт", 40),
    // Кузнечное дело: железо и уголь местные или с Боспора, олово и серебро — заморские.
    ("Железная руда", 20),
    ("Крица", 140),
    ("Железная полоса", 200),
    ("Стальная полоса", 640),
    ("Железный лом", 60),
    ("Олово", 60),
    ("Бронзовый лом", 160),
    ("Серебряный лом", 400),
    ("Чугунная корка", 5),
    ("Ясеневая заготовка", 60),
    ("Гончарная глина", 10),
    ("Каменная литейная форма", 400),
    ("Наконечники стрел", 30),
    ("Бронзовая пряжка", 320),
    ("Бронзовая фибула", 360),
    ("Бронзовое навершие", 440),
    ("Свинцовые грузила", 15),
    ("Плиний. Естественная история", 9600),
    ("Целебное зелье", 600),
    ("Рубин", 14400),
    // Привоз из Константинополя, Синопы, Малой Азии (амфора LRA1 — около 25 литров).
    ("Амфора вина", 1600),
    ("Амфора оливкового масла", 2800),
    ("Стеклянная чаша", 200),
    ("Отрез льна", 1440),
    // Вывоз из Херсона.
    ("Отрез шерсти", 960),
    ("Солёная рыба", 60),
    ("Мешок соли", 160),
    ("Сырая шкура", 320),
    ("Овчина", 280),
    ("Войлок", 480),
    ("Воск", 120),
    ("Горшок мёда", 200),
    // Местные травы (собирают за стенами, стоят гроши).
    ("Шалфей", 30),
    ("Чабрец", 20),
    ("Зверобой", 30),
    ("Белена", 40),
    ("Ягоды можжевельника", 30),
    ("Полынь", 20),
    ("Тысячелистник", 20),
    ("Подорожник", 10),
    ("Шиповник", 20),
    ("Просвирник", 20),
    ("Маковые коробочки", 60),
    ("Мята", 20),
    ("Корень марены", 80),
    ("Вайда", 60),
    ("Дубовые галлы", 80),
    ("Корень окопника", 40),
    ("Корень девясила", 60),
    // Степь.
    ("Солодковый корень", 120),
    ("Ревень понтийский", 400),
    ("Бараний жир", 80),
    // Заморские снадобья и минералы (за малую меру).
    ("Мирра", 1200),
    ("Ладан", 800),
    ("Сера", 240),
    ("Квасцы", 320),
    ("Киноварь", 2400),
    ("Опий", 2000),
    ("Перец", 1800),
    ("Шафран", 2400),
    ("Корица", 3600),
    ("Хиосская мастика", 1440),
    ("Нефть", 240),
    ("Натр", 160),
    ("Камедь", 400),
    // Материалы.
    ("Сосновая смола", 60),
    ("Негашёная известь", 40),
    ("Медный купорос", 240),
    ("Свинец", 200),
    ("Медный лом", 160),
    ("Древесная зола", 10),
    ("Кувшин вина", 160),
    ("Кувшин оливкового масла", 280),
    // Посуда и инструменты. Алембик и реторта — штучная работа стеклодува на заказ.
    ("Ступка с пестиком", 400),
    ("Глиняный горшок", 40),
    ("Медный котелок", 1200),
    ("Весы с гирями", 2400),
    ("Пустой флакон", 80),
    ("Тигель", 160),
    ("Алембик", 4800),
    ("Стеклянная реторта", 3600),
    ("Керотакис", 3000),
    // Книги: переписчик берёт за кодекс солиды.
    ("Обрывок травника", 1200),
    ("Диоскорид. О лекарственных веществах", 28800),
    ("Иллюстрированный травник", 43200),
    ("Гиппократ. О язвах", 7200),
    ("Гален. О простых лекарствах", 21600),
    ("Гален. О противоядиях", 14400),
    ("Орибасий. Синопсис", 21600),
    ("Зосим. О приборах и печах", 36000),
    ("Книга красильщика", 7200),
    ("Эней Тактик. Об обороне городов", 10800),
    ("Феофраст. О камнях", 10800),
    ("Цельс. О медицине", 14400),
    // Снадобья и продукты.
    ("Кровоостанавливающая припарка", 120),
    ("Отвар шалфея", 80),
    ("Отвар чабреца", 80),
    ("Маковый отвар", 400),
    ("Зверобойное масло", 240),
    ("Ранозаживляющая мазь", 400),
    ("Оксимель", 160),
    ("Териак", 4800),
    ("Винный уксус", 80),
    ("Ртуть", 2400),
    ("Серная вода", 200),
    ("Смоляное масло", 200),
    ("Ярь-медянка", 480),
    ("Свинцовые белила", 400),
    ("Сурик", 480),
    ("Мареновый лак", 720),
    ("Вайдовая синь", 480),
    ("Чернила", 160),
    ("Щёлок", 40),
    ("Мыло", 120),
    ("Зажигательный горшок", 1200),
    ("Испорченное варево", 10),
    ("Отвар полыни", 80),
    ("Мятный отвар", 60),
    ("Отвар шиповника", 80),
    ("Отвар девясила", 100),
    // Медная жаровня на треноге и стол с жаровней, весами, ступкой и подставкой для алембика.
    ("Жаровня", 960),
    ("Алхимический стол", 7200 * 2),
    // Корзина угля из дубовых дров с Гераклейского плато; кусок — на час-другой жара.
    ("Древесный уголь", 6),
    // Подготовленное сырьё: сушёное дороже свежего (усохло вчетверо), толчёное — чуть дороже целого.
    ("Сушёный шалфей", (30) * 3),
    ("Сушёный чабрец", (20) * 3),
    ("Сушёный зверобой", (30) * 3),
    ("Сушёный подорожник", (10) * 3),
    ("Сушёный тысячелистник", (20) * 3),
    ("Сушёный просвирник", (20) * 3),
    ("Сушёная мята", (20) * 3),
    ("Сушёная полынь", (20) * 3),
    ("Сушёный шиповник", (20) * 3),
    ("Толчёный корень девясила", (60) * 6 / 5),
    ("Толчёный корень окопника", (40) * 6 / 5),
    ("Толчёный корень марены", (80) * 6 / 5),
    ("Толчёный солодковый корень", (120) * 6 / 5),
    ("Толчёная мирра", (1200) * 6 / 5),
    ("Толчёный ладан", (800) * 6 / 5),
    ("Толчёные галлы", (80) * 6 / 5),
    ("Толчёная сера", (240) * 6 / 5),
    ("Толчёная киноварь", (2400) * 6 / 5),
    ("Сок подорожника", 60),
    ("Маковый сок", 360),
    ("Полынное вино", 200),
    ("Чабрецовый уксус", 120),
    ("Мазь из окопника", 300),
    ("Сушильная решётка", 60),
    ("Вино с миррой", 260),
    ("Отвар ревеня", 120),
    ("Вяжущая мазь", 280),
    ("Мазь от ожогов", 280),
];

/// Все ли предметы игры имеют осмысленную цену (для тестов и отладки).
#[allow(dead_code)]
pub fn unpriced_items() -> Vec<&'static str> {
    ITEM_BASES
        .iter()
        .filter(|b| {
            !PRICE_LIST.iter().any(|(n, _)| *n == b.name)
                && crate::arsenal::find(b.name).is_none()
                && !DENOMINATIONS.iter().any(|(n, _)| *n == b.name)
        })
        .map(|b| b.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Inventory;

    fn wallet(coins: &[(&str, u16)]) -> (Inventory, UidGen) {
        let mut inv = Inventory::new((6, 4));
        let mut uids = UidGen::default();
        for (name, count) in coins {
            let item = Item {
                uid: uids.next(),
                base: ItemBaseId::by_name(name),
                quality: Quality::Common,
                affixes: vec![],
                count: *count,
                make: None,
            };
            inv.place(item, Target::Auto).unwrap();
        }
        (inv, uids)
    }

    #[test]
    fn change_is_given() {
        let (mut inv, mut uids) = wallet(&[("Солид", 1)]);
        take_money(&mut inv, 40 * 25, &mut uids).unwrap();
        assert_eq!(purse(&inv), 7200 - 1000);
        // Солид разменяли: остались тремиссы и фоллисы.
        assert!(!inv.all_items().any(|i| i.info().name == "Солид"));
    }

    #[test]
    fn pays_exact_without_breaking_gold() {
        let (mut inv, mut uids) = wallet(&[("Солид", 2), ("Фоллис", 30)]);
        take_money(&mut inv, 40 * 12, &mut uids).unwrap();
        assert_eq!(purse(&inv), 2 * 7200 + 18 * 40);
        assert_eq!(
            inv.all_items()
                .filter(|i| i.info().name == "Солид")
                .map(|i| i.count)
                .sum::<u16>(),
            2
        );
    }

    #[test]
    fn not_enough_money_changes_nothing() {
        let (mut inv, mut uids) = wallet(&[("Фоллис", 3)]);
        let before = inv.clone();
        assert_eq!(
            take_money(&mut inv, 200, &mut uids),
            Err(MoneyError::NotEnough)
        );
        assert_eq!(inv, before);
    }

    #[test]
    fn change_needs_room() {
        // Сумка 1×1 занята солидом: сдачу из разных монет положить некуда.
        let mut inv = Inventory::new((1, 1));
        let mut uids = UidGen::default();
        inv.place(
            Item {
                uid: uids.next(),
                base: ItemBaseId::by_name("Солид"),
                quality: Quality::Common,
                affixes: vec![],
                count: 1,
                make: None,
            },
            Target::Auto,
        )
        .unwrap();
        let before = inv.clone();
        assert_eq!(
            take_money(&mut inv, 2410, &mut uids),
            Err(MoneyError::NoSpace)
        );
        assert_eq!(inv, before);
    }

    #[test]
    fn coins_split_into_stacks() {
        let coins = coins_for(7200 * 3 + 40 * 250 + 30);
        let folles: u32 = coins
            .iter()
            .filter(|(b, _)| b.base().name == "Фоллис")
            .map(|(_, c)| *c as u32)
            .sum();
        assert!(coins.iter().all(|(b, c)| *c <= b.base().max_stack));
        // 250 фоллисов = 1 солид (180) + 1 тремисс (60) + 10 фоллисов.
        assert_eq!(folles, 10);
        assert_eq!(
            coins
                .iter()
                .map(|(b, c)| coin_value(*b).unwrap() * *c as u32)
                .sum::<u32>(),
            7200 * 3 + 40 * 250 + 30
        );
    }

    #[test]
    fn russian_money_words() {
        assert_eq!(format_money(7200 * 2 + 40 * 15), "2 солида 15 фоллисов");
        assert_eq!(format_money(40 + 20), "1 фоллис 20 нуммиев");
        assert_eq!(format_money(7200 * 21 + 40 * 3), "21 солид 3 фоллиса");
        assert_eq!(format_money(0), "ничего");
    }

    #[test]
    fn every_trade_good_is_priced() {
        // Свои товары и всё, что было до них, должны быть в прейскуранте.
        let missing = unpriced_items();
        for name in ["Амфора вина", "Солёная рыба", "Спата", "Бинт"]
        {
            assert!(!missing.contains(&name), "нет цены у «{name}»");
        }
        assert!(base_price(ItemBaseId::by_name("Солид")) == 7200);
    }
}
