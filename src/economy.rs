//! Деньги и цены Херсона около 510 года.
//!
//! # Монеты
//!
//! В ходу три монеты с простым десятичным курсом:
//! - **золотая монета** = 10 серебряных — жалованье, подати, дома, доспехи, книги;
//! - **серебряная монета** = 10 медных — плата за день работы, оружие попроще, снадобья;
//! - **медная монета** — хлеб, травы, баня, мелочь. Мельче медной монеты ничего нет.
//!
//! Все цены в коде — целые «доли» (`u32`): медная монета — 10 долей, серебряная — 100,
//! золотая — 1000. Так в прейскуранте остаются прежние числа, а сумма раскладывается на монеты без дробей.
//!
//! Ориентиры (папирусы Египта VI века, Кодекс Юстиниана): поденщик зарабатывает
//! около 2–3 серебряных в день, кольчуга стоит полтора десятка золотых, переписанная книга —
//! от семи до сорока золотых.
//!
//! До этих монет были солиды, тремиссы, фоллисы и декануммии. Они остались только в старых
//! сохранениях: [`legacy_coin_value`] переводит их в доли, и сундуки получают новые монеты
//! на ту же сумму.

use crate::inventory::{Inventory, Target};
use crate::items::{ITEM_BASES, Item, ItemBaseId, ItemShape, Quality, UidGen};

/// Медная монета, в долях.
pub const COPPER: u32 = 10;
/// Серебряная монета: 10 медных.
pub const SILVER: u32 = 10 * COPPER;
/// Золотая монета: 10 серебряных.
pub const GOLD: u32 = 10 * SILVER;

pub const GOLD_COIN: &str = "Золотая монета";
pub const SILVER_COIN: &str = "Серебряная монета";
pub const COPPER_COIN: &str = "Медная монета";

/// Монеты от крупной к мелкой: имя и стоимость в долях.
pub const DENOMINATIONS: [(&str, u32); 3] = [(GOLD_COIN, GOLD), (SILVER_COIN, SILVER), (COPPER_COIN, COPPER)];

/// Самая мелкая монета: цены округляются до неё.
pub const SMALLEST_COIN: u32 = COPPER;

/// Монеты прежней системы (солид = 7200 долей и т. д.): только для старых сохранений.
const LEGACY_COINS: [(&str, u32); 4] = [("Солид", 7200), ("Тремисс", 2400), ("Фоллис", 40), ("Декануммий", 10)];

/// Сколько долей стоила монета прежней системы с таким названием (для старых сохранений).
pub fn legacy_coin_value(name: &str) -> Option<u32> {
    LEGACY_COINS.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

/// Стоимость одной монеты, если предмет — монета.
pub fn coin_value(base: ItemBaseId) -> Option<u32> {
    let name = base.base().name;
    DENOMINATIONS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

/// Золото или серебро: за стенами драгоценный металл ценят, а медь берут плохо или не берут вовсе.
pub fn is_precious_coin(base: ItemBaseId) -> bool {
    coin_value(base).is_some_and(|v| v >= SILVER)
}

/// Сколько денег в инвентаре (доли).
pub fn purse(inv: &Inventory) -> u32 {
    inv.all_items()
        .filter_map(|i| coin_value(i.base).map(|v| v * i.count as u32))
        .sum()
}

/// Разложить сумму на монеты (жадно, от крупных). Сумма сперва округляется до медной монеты
/// ([`whole_coins`]). Возвращает стопки не больше `max_stack`.
pub fn coins_for(nummi: u32) -> Vec<(ItemBaseId, u16)> {
    let mut rest = whole_coins(nummi);
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
/// Сумма округляется до медной монеты, как её и показывают ([`whole_coins`]).
/// При ошибке инвентарь не меняется.
pub fn take_money(inv: &mut Inventory, nummi: u32, uids: &mut UidGen) -> Result<(), MoneyError> {
    let nummi = whole_coins(nummi);
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

/// Сколько стоит одна штука товара обычного качества (справочная цена Херсона, в долях).
pub fn base_price(base: ItemBaseId) -> u32 {
    if let Some(v) = coin_value(base) {
        return v;
    }
    let info = base.base();
    if let Some(a) = crate::arsenal::of_base(base) {
        return a.price;
    }
    if let Some(p) = crate::fishing::price(info.name) {
        return p;
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

/// Округлить до мелкой монеты (не меньше одной медной).
pub fn round_price(nummi: f32) -> u32 {
    ((nummi / SMALLEST_COIN as f32).round().max(1.0) as u32) * SMALLEST_COIN
}

/// Сумма, которую можно отсчитать монетами: до ближайшей медной, но не меньше одной, если сумма не нулевая.
/// Так платят, получают и показывают одинаково.
pub fn whole_coins(nummi: u32) -> u32 {
    if nummi == 0 { 0 } else { round_price(nummi as f32) }
}

/// Склонение: 1 монета, 2 монеты, 5 монет.
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

/// Сколько золотых, серебряных и медных в сумме (как её отсчитают монетами).
pub fn split_coins(nummi: u32) -> (u32, u32, u32) {
    let n = whole_coins(nummi);
    (n / GOLD, n % GOLD / SILVER, n % SILVER / COPPER)
}

/// «2 золотых 3 серебряных 5 медных», «1 серебряный», «ничего».
pub fn format_money(nummi: u32) -> String {
    let (g, s, c) = split_coins(nummi);
    let mut parts = Vec::new();
    if g > 0 {
        parts.push(format!("{g} {}", plural(g, "золотой", "золотых", "золотых")));
    }
    if s > 0 {
        parts.push(format!("{s} {}", plural(s, "серебряный", "серебряных", "серебряных")));
    }
    if c > 0 {
        parts.push(format!("{c} {}", plural(c, "медный", "медных", "медных")));
    }
    if parts.is_empty() { "ничего".to_string() } else { parts.join(" ") }
}

/// Короткая запись для списков: «2 зол. 3 сер. 5 мед.», «4 мед.».
pub fn format_money_short(nummi: u32) -> String {
    let (g, s, c) = split_coins(nummi);
    let mut parts = Vec::new();
    if g > 0 {
        parts.push(format!("{g} зол."));
    }
    if s > 0 {
        parts.push(format!("{s} сер."));
    }
    if c > 0 {
        parts.push(format!("{c} мед."));
    }
    if parts.is_empty() { "0".to_string() } else { parts.join(" ") }
}

/// Справочные цены за штуку, в долях (медная монета — 10). Сюда же — предметы треда знаний и алхимии
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
    ("Лубок", 60),
    // Домашняя утварь: светильник и кружка — гроши, бронза и серебро — дороги.
    ("Глиняный светильник", 20),
    ("Бронзовый светильник", 720),
    ("Краснолаковая миска", 40),
    ("Глиняная кружка", 20),
    ("Стеклянный кубок", 160),
    ("Бронзовый кувшин", 1440),
    ("Серебряная ложка", 1200),
    ("Кухонный нож", 80),
    ("Каравай хлеба", 10),
    ("Головка сыра", 60),
    ("Сушёные смоквы", 30),
    ("Костяной гребень", 60),
    ("Бронзовое зеркальце", 480),
    ("Ампула святого Мины", 60),
    ("Энколпион", 600),
    ("Игральные кости", 20),
    ("Деревянная лошадка", 40),
    ("Веретено с пряслицем", 15),
    ("Моток пряжи", 120),
    ("Железные ножницы", 120),
    ("Деревянная шкатулка", 180),
    ("Доска для тавлей", 240),
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
    // Луки и стрелы: стрела — наконечник, древко, перья, клей и работа.
    ("Простой лук", 1800),
    ("Гуннский составной лук", 6000),
    ("Стрелы", 70),
    ("Бронебойные стрелы", 75),
    ("Трёхлопастные стрелы", 95),
    ("Стрелы-срезни", 80),
    ("Древки стрел", 10),
    ("Маховые перья", 2),
    ("Рыбий клей", 80),
    ("Сухожилия", 40),
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
    // Книги: переписчик берёт за кодекс десятки золотых.
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
    // Школьные книги и письменный прибор. Тетрадь — восемь листов пергамена: книга съедает десятки.
    ("Псалтирь", 7200),
    ("Дионисий Фракиец. Искусство грамматики", 4800),
    ("Никомах. Введение в арифметику", 7200),
    ("Восковая табличка", 160),
    ("Тростниковое перо", 20),
    ("Тетрадь пергамента", 240),
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
    // Горное дело и углежжение: болотная руда беднее горной, обожжённая — лучше; вязанка дров
    // с делянки; блок тёсаного известняка из каменоломни; песок для стекольщиков.
    ("Болотная руда", 12),
    ("Обожжённая руда", 32),
    ("Пустая порода", 1),
    ("Дрова", 4),
    ("Морской песок", 3),
    ("Известняковый блок", 60),
    ("Кайло рудокопа", 360),
    ("Заступ", 200),
    ("Топор дровосека", 320),
    ("Феофраст. Исследование о растениях", 10800),
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

    fn count(inv: &Inventory, name: &str) -> u32 {
        inv.all_items().filter(|i| i.info().name == name).map(|i| i.count as u32).sum()
    }

    #[test]
    fn exchange_rate_is_decimal() {
        assert_eq!(GOLD, 10 * SILVER);
        assert_eq!(SILVER, 10 * COPPER);
        for (name, value) in DENOMINATIONS {
            assert_eq!(coin_value(ItemBaseId::by_name(name)), Some(value), "{name}");
            assert_eq!(base_price(ItemBaseId::by_name(name)), value);
        }
        assert!(is_precious_coin(ItemBaseId::by_name(GOLD_COIN)));
        assert!(is_precious_coin(ItemBaseId::by_name(SILVER_COIN)));
        assert!(!is_precious_coin(ItemBaseId::by_name(COPPER_COIN)));
        // Старых монет в игре больше нет.
        for (old, _) in LEGACY_COINS {
            assert!(ItemBaseId::find(old).is_none(), "«{old}» ещё в таблице предметов");
            assert!(legacy_coin_value(old).is_some());
        }
        assert_eq!(legacy_coin_value("Солид"), Some(7200));
        assert_eq!(legacy_coin_value("Спата"), None);
    }

    #[test]
    fn change_is_given() {
        let (mut inv, mut uids) = wallet(&[(GOLD_COIN, 1)]);
        take_money(&mut inv, 250, &mut uids).unwrap();
        assert_eq!(purse(&inv), 750);
        // Золотой разменяли: сдача — 7 серебряных и 5 медных.
        assert_eq!(count(&inv, GOLD_COIN), 0);
        assert_eq!(count(&inv, SILVER_COIN), 7);
        assert_eq!(count(&inv, COPPER_COIN), 5);
    }

    #[test]
    fn pays_exact_without_breaking_gold() {
        let (mut inv, mut uids) = wallet(&[(GOLD_COIN, 2), (SILVER_COIN, 3), (COPPER_COIN, 30)]);
        take_money(&mut inv, 120, &mut uids).unwrap();
        assert_eq!(purse(&inv), 2 * GOLD + 3 * SILVER + 30 * COPPER - 120);
        assert_eq!(count(&inv, GOLD_COIN), 2);
    }

    #[test]
    fn odd_sums_are_paid_and_given_in_whole_copper() {
        // 14 долей — одна медная, 15 — две; ноль — ничего, но 3 доли — всё же медная.
        assert_eq!(whole_coins(14), 10);
        assert_eq!(whole_coins(15), 20);
        assert_eq!(whole_coins(3), 10);
        assert_eq!(whole_coins(0), 0);
        let (mut inv, mut uids) = wallet(&[(SILVER_COIN, 1)]);
        take_money(&mut inv, 14, &mut uids).unwrap();
        assert_eq!(purse(&inv), 90);
        give_money(&mut inv, 26, &mut uids).unwrap();
        assert_eq!(purse(&inv), 120);
        // Показывают ту же сумму, что отсчитают.
        assert_eq!(format_money(14), "1 медный");
        assert_eq!(format_money(26), "3 медных");
    }

    #[test]
    fn not_enough_money_changes_nothing() {
        let (mut inv, mut uids) = wallet(&[(SILVER_COIN, 1)]);
        let before = inv.clone();
        assert_eq!(take_money(&mut inv, 200, &mut uids), Err(MoneyError::NotEnough));
        assert_eq!(inv, before);
    }

    #[test]
    fn change_needs_room() {
        // Сумка 1×1 занята золотым: сдачу из разных монет положить некуда.
        let mut inv = Inventory::new((1, 1));
        let mut uids = UidGen::default();
        inv.place(
            Item {
                uid: uids.next(),
                base: ItemBaseId::by_name(GOLD_COIN),
                quality: Quality::Common,
                affixes: vec![],
                count: 1,
                make: None,
            },
            Target::Auto,
        )
        .unwrap();
        let before = inv.clone();
        assert_eq!(take_money(&mut inv, 410, &mut uids), Err(MoneyError::NoSpace));
        assert_eq!(inv, before);
    }

    #[test]
    fn coins_split_into_stacks() {
        let sum = 3 * GOLD + 250 * COPPER + 30;
        let coins = coins_for(sum);
        assert!(coins.iter().all(|(b, c)| *c <= b.base().max_stack));
        let of = |name: &str| coins.iter().filter(|(b, _)| b.base().name == name).map(|(_, c)| *c as u32).sum::<u32>();
        // 250 медных = 2 золотых + 5 серебряных; ещё 30 долей — 3 медных.
        assert_eq!((of(GOLD_COIN), of(SILVER_COIN), of(COPPER_COIN)), (5, 5, 3));
        assert_eq!(coins.iter().map(|(b, c)| coin_value(*b).unwrap() * *c as u32).sum::<u32>(), sum);
        // Много золота — несколько стопок по 100.
        let rich = coins_for(250 * GOLD);
        assert_eq!(rich.iter().filter(|(b, _)| b.base().name == GOLD_COIN).count(), 3);
    }

    #[test]
    fn russian_money_words() {
        assert_eq!(format_money(2 * GOLD + 3 * SILVER + 5 * COPPER), "2 золотых 3 серебряных 5 медных");
        assert_eq!(format_money(GOLD + SILVER + COPPER), "1 золотой 1 серебряный 1 медный");
        assert_eq!(format_money(21 * GOLD + 11 * COPPER), "21 золотой 1 серебряный 1 медный");
        assert_eq!(format_money(11 * GOLD + 4 * SILVER), "11 золотых 4 серебряных");
        assert_eq!(format_money(0), "ничего");
        assert_eq!(format_money_short(14_400), "14 зол. 4 сер.");
        assert_eq!(format_money_short(40), "4 мед.");
        assert_eq!(format_money_short(0), "0");
    }

    #[test]
    fn every_trade_good_is_priced() {
        // Свои товары и всё, что было до них, должны быть в прейскуранте.
        let missing = unpriced_items();
        for name in ["Амфора вина", "Солёная рыба", "Спата", "Бинт"] {
            assert!(!missing.contains(&name), "нет цены у «{name}»");
        }
        for (coin, _) in DENOMINATIONS {
            assert!(!missing.contains(&coin), "монета «{coin}» без цены");
        }
    }
}
