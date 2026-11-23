//! Предметы: базовые типы, качество, случайные характеристики и таблицы добычи.
//!
//! Чистые данные без графики и сети: этим модулем пользуются и сервер (бросок добычи),
//! и клиент (подсказки, цвета). Всё случайное идёт через [`Rng`], чтобы результат
//! можно было повторить по зерну.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Генератор случайных чисел
// ---------------------------------------------------------------------------

/// Маленький быстрый генератор (xorshift64*). Своих зависимостей не тянем.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Self::new(nanos)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Число в [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.f32()
    }

    /// Целое в [min, max] включительно.
    pub fn range_u32(&mut self, min: u32, max: u32) -> u32 {
        if max <= min {
            return min;
        }
        min + (self.next_u64() % (max - min + 1) as u64) as u32
    }

    /// Индекс по весам. Пустой список или нулевые веса дают None.
    pub fn weighted<T>(&mut self, items: &[(T, f32)]) -> Option<usize> {
        let total: f32 = items.iter().map(|(_, w)| w.max(0.0)).sum();
        if total <= 0.0 {
            return None;
        }
        let mut roll = self.f32() * total;
        for (i, (_, w)) in items.iter().enumerate() {
            roll -= w.max(0.0);
            if roll < 0.0 {
                return Some(i);
            }
        }
        Some(items.len() - 1)
    }
}

// ---------------------------------------------------------------------------
// Качество
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Quality {
    Poor,
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Quality {
    pub const ALL: [Quality; 6] = [Quality::Poor, Quality::Common, Quality::Uncommon, Quality::Rare, Quality::Epic, Quality::Legendary];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Poor => "Ветхое",
            Quality::Common => "Обычное",
            Quality::Uncommon => "Добротное",
            Quality::Rare => "Редкое",
            Quality::Epic => "Эпическое",
            Quality::Legendary => "Легендарное",
        }
    }

    /// Цвет рамки и названия (sRGB).
    pub fn color(self) -> [f32; 3] {
        match self {
            Quality::Poor => [0.55, 0.55, 0.55],
            Quality::Common => [0.92, 0.92, 0.88],
            Quality::Uncommon => [0.35, 0.85, 0.3],
            Quality::Rare => [0.3, 0.55, 1.0],
            Quality::Epic => [0.72, 0.35, 0.95],
            Quality::Legendary => [1.0, 0.62, 0.12],
        }
    }

    /// Множитель базовых характеристик предмета (урон, броня).
    pub fn stat_multiplier(self) -> f32 {
        match self {
            Quality::Poor => 0.8,
            Quality::Common => 1.0,
            Quality::Uncommon => 1.1,
            Quality::Rare => 1.2,
            Quality::Epic => 1.32,
            Quality::Legendary => 1.45,
        }
    }

    /// Сколько случайных свойств получает предмет.
    pub fn affix_count(self) -> usize {
        match self {
            Quality::Poor | Quality::Common => 0,
            Quality::Uncommon => 1,
            Quality::Rare => 2,
            Quality::Epic => 3,
            Quality::Legendary => 4,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

// ---------------------------------------------------------------------------
// Характеристики
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stat {
    /// Урон оружия (базовая характеристика оружия).
    WeaponDamage,
    /// Броня (базовая характеристика доспехов и щитов).
    Armor,
    /// Добавочный урон.
    BonusDamage,
    MaxHealth,
    Strength,
    Agility,
    /// % к скорости передвижения.
    MoveSpeed,
    /// % к скорости замаха и удара.
    ActionSpeed,
    /// Миллисекунды к окну парирования.
    ParryWindow,
    /// % снижения потери стойкости при блоке.
    BlockStability,
    /// % снижения урона от ударов в голову.
    HeadshotReduction,
    /// Удача: шанс найти вещи лучшего качества.
    Luck,
}

impl Stat {
    pub fn name(self) -> &'static str {
        match self {
            Stat::WeaponDamage => "Урон оружия",
            Stat::Armor => "Броня",
            Stat::BonusDamage => "Доп. урон",
            Stat::MaxHealth => "Здоровье",
            Stat::Strength => "Сила",
            Stat::Agility => "Ловкость",
            Stat::MoveSpeed => "Скорость бега",
            Stat::ActionSpeed => "Скорость ударов",
            Stat::ParryWindow => "Окно парирования",
            Stat::BlockStability => "Устойчивость блока",
            Stat::HeadshotReduction => "Защита головы",
            Stat::Luck => "Удача",
        }
    }

    /// Как показывать число.
    pub fn format(self, value: f32) -> String {
        match self {
            Stat::MoveSpeed | Stat::ActionSpeed | Stat::BlockStability | Stat::HeadshotReduction => format!("{value:+.1}%"),
            Stat::ParryWindow => format!("{value:+.0} мс"),
            Stat::WeaponDamage | Stat::Armor => format!("{value:.0}"),
            _ => format!("{value:+.0}"),
        }
    }
}

/// Сумма характеристик (например, всей надетой экипировки).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct StatTotals(pub Vec<(Stat, f32)>);

impl StatTotals {
    pub fn add(&mut self, stat: Stat, value: f32) {
        match self.0.iter_mut().find(|(s, _)| *s == stat) {
            Some((_, v)) => *v += value,
            None => self.0.push((stat, value)),
        }
    }

    pub fn get(&self, stat: Stat) -> f32 {
        self.0.iter().find(|(s, _)| *s == stat).map(|(_, v)| *v).unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------
// Слоты экипировки
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Head,
    Chest,
    Hands,
    Legs,
    Feet,
    MainHand,
    OffHand,
    Necklace,
    Ring,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 9] = [
        EquipSlot::Head,
        EquipSlot::Necklace,
        EquipSlot::Chest,
        EquipSlot::Hands,
        EquipSlot::Legs,
        EquipSlot::Feet,
        EquipSlot::Ring,
        EquipSlot::MainHand,
        EquipSlot::OffHand,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EquipSlot::Head => "Голова",
            EquipSlot::Chest => "Торс",
            EquipSlot::Hands => "Руки",
            EquipSlot::Legs => "Ноги",
            EquipSlot::Feet => "Ступни",
            EquipSlot::MainHand => "Правая рука",
            EquipSlot::OffHand => "Левая рука",
            EquipSlot::Necklace => "Шея",
            EquipSlot::Ring => "Кольцо",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Базовые предметы
// ---------------------------------------------------------------------------

/// Как предмет выглядит, когда лежит на земле.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemShape {
    Blade,
    Blunt,
    Polearm,
    Shield,
    Armor,
    Helmet,
    Small,
    Coins,
    Flask,
}

/// Описание вида предмета. Конкретный экземпляр — [`Item`].
#[derive(Debug, Clone, Copy)]
pub struct ItemBase {
    pub name: &'static str,
    /// Короткая подпись (сейчас в клетках иконки, подпись пригодится торговцу).
    #[allow(dead_code)]
    pub short: &'static str,
    pub slot: Option<EquipSlot>,
    /// Размер в клетках сетки (ширина, высота).
    pub size: (u8, u8),
    /// Основная характеристика и её значение при обычном качестве.
    pub base_stat: Option<(Stat, f32)>,
    /// Больше 1 — предметы складываются в стопку.
    pub max_stack: u16,
    /// Какие случайные свойства может получить.
    pub affix_pool: &'static [Stat],
    pub shape: ItemShape,
    /// Можно ли выпасть с качеством выше обычного (у монет и зелий нет качества).
    pub has_quality: bool,
}

const WEAPON_AFFIXES: &[Stat] = &[Stat::BonusDamage, Stat::Strength, Stat::Agility, Stat::ActionSpeed, Stat::ParryWindow];
const SHIELD_AFFIXES: &[Stat] = &[Stat::MaxHealth, Stat::Strength, Stat::BlockStability, Stat::ParryWindow];
const ARMOR_AFFIXES: &[Stat] = &[Stat::MaxHealth, Stat::Strength, Stat::Agility, Stat::MoveSpeed, Stat::Luck];
const HELMET_AFFIXES: &[Stat] = &[Stat::MaxHealth, Stat::HeadshotReduction, Stat::Strength, Stat::Luck];
const JEWEL_AFFIXES: &[Stat] = &[Stat::MaxHealth, Stat::Strength, Stat::Agility, Stat::MoveSpeed, Stat::ActionSpeed, Stat::Luck, Stat::BonusDamage];

macro_rules! base {
    ($name:expr, $short:expr, $slot:expr, $size:expr, $stat:expr, $stack:expr, $affixes:expr, $shape:expr, $quality:expr) => {
        ItemBase { name: $name, short: $short, slot: $slot, size: $size, base_stat: $stat, max_stack: $stack, affix_pool: $affixes, shape: $shape, has_quality: $quality }
    };
}

use EquipSlot as S;
use ItemShape as Sh;

/// Все виды предметов. Индекс в этом массиве — [`ItemBaseId`].
pub const ITEM_BASES: &[ItemBase] = &[
    // Оружие
    base!("Спата", "Спата", Some(S::MainHand), (1, 3), Some((Stat::WeaponDamage, 30.0)), 1, WEAPON_AFFIXES, Sh::Blade, true),
    base!("Кавалерийская спата", "Длинная спата", Some(S::MainHand), (1, 4), Some((Stat::WeaponDamage, 42.0)), 1, WEAPON_AFFIXES, Sh::Blade, true),
    base!("Булава", "Булава", Some(S::MainHand), (1, 2), Some((Stat::WeaponDamage, 34.0)), 1, WEAPON_AFFIXES, Sh::Blunt, true),
    base!("Боевой топор", "Топор", Some(S::MainHand), (2, 3), Some((Stat::WeaponDamage, 38.0)), 1, WEAPON_AFFIXES, Sh::Blunt, true),
    base!("Пехотное копьё", "Копьё", Some(S::MainHand), (1, 5), Some((Stat::WeaponDamage, 50.0)), 1, WEAPON_AFFIXES, Sh::Polearm, true),
    base!("Кинжал", "Кинжал", Some(S::MainHand), (1, 2), Some((Stat::WeaponDamage, 20.0)), 1, WEAPON_AFFIXES, Sh::Blade, true),
    // Щиты
    base!("Овальный щит", "Щит", Some(S::OffHand), (2, 3), Some((Stat::Armor, 20.0)), 1, SHIELD_AFFIXES, Sh::Shield, true),
    base!("Круглый щит", "Круглый щит", Some(S::OffHand), (2, 2), Some((Stat::Armor, 10.0)), 1, SHIELD_AFFIXES, Sh::Shield, true),
    // Шлемы
    base!("Спангенхельм", "Спангенхельм", Some(S::Head), (2, 2), Some((Stat::Armor, 18.0)), 1, HELMET_AFFIXES, Sh::Helmet, true),
    base!("Гребенчатый шлем", "Шлем", Some(S::Head), (2, 2), Some((Stat::Armor, 11.0)), 1, HELMET_AFFIXES, Sh::Helmet, true),
    // Торс
    base!("Стёганая туника", "Стёганка", Some(S::Chest), (2, 3), Some((Stat::Armor, 14.0)), 1, ARMOR_AFFIXES, Sh::Armor, true),
    base!("Кольчуга", "Кольчуга", Some(S::Chest), (2, 3), Some((Stat::Armor, 26.0)), 1, ARMOR_AFFIXES, Sh::Armor, true),
    base!("Ламеллярный панцирь", "Панцирь", Some(S::Chest), (2, 3), Some((Stat::Armor, 38.0)), 1, ARMOR_AFFIXES, Sh::Armor, true),
    // Руки, ноги
    base!("Ламеллярные наручи", "Наручи", Some(S::Hands), (2, 2), Some((Stat::Armor, 9.0)), 1, ARMOR_AFFIXES, Sh::Armor, true),
    base!("Кожаные перчатки", "Перчатки", Some(S::Hands), (2, 2), Some((Stat::Armor, 4.0)), 1, ARMOR_AFFIXES, Sh::Armor, true),
    base!("Железные поножи", "Поножи", Some(S::Legs), (2, 3), Some((Stat::Armor, 16.0)), 1, ARMOR_AFFIXES, Sh::Armor, true),
    base!("Кожаные сапоги", "Сапоги", Some(S::Feet), (2, 2), Some((Stat::Armor, 5.0)), 1, ARMOR_AFFIXES, Sh::Armor, true),
    // Украшения
    base!("Серебряный перстень", "Перстень", Some(S::Ring), (1, 1), None, 1, JEWEL_AFFIXES, Sh::Small, true),
    base!("Нательный крест", "Крест", Some(S::Necklace), (1, 1), None, 1, JEWEL_AFFIXES, Sh::Small, true),
    // Расходники и ценности
    // Золотой солид (номисма) Анастасия: 1/72 римского фунта золота, 4,5 г. См. economy.rs.
    base!("Солид", "Солид", None, (1, 1), None, 100, &[], Sh::Coins, false),
    base!("Бинт", "Бинт", None, (1, 1), None, 5, &[], Sh::Small, false),
    base!("Целебное зелье", "Зелье", None, (1, 2), None, 3, &[], Sh::Flask, false),
    base!("Рубин", "Рубин", None, (1, 1), None, 1, &[], Sh::Small, false),
    // --- Торговля (economy.rs, trade.rs): монеты реформы Анастасия 498 г. и товары Херсона ---
    base!("Тремисс", "Тремисс", None, (1, 1), None, 100, &[], Sh::Coins, false),
    base!("Фоллис", "Фоллис", None, (1, 1), None, 100, &[], Sh::Coins, false),
    base!("Декануммий", "Декан.", None, (1, 1), None, 100, &[], Sh::Coins, false),
    base!("Амфора вина", "Вино", None, (2, 3), None, 1, &[], Sh::Flask, false),
    base!("Амфора оливкового масла", "Масло", None, (2, 3), None, 1, &[], Sh::Flask, false),
    base!("Стеклянная чаша", "Чаша", None, (1, 1), None, 4, &[], Sh::Small, false),
    base!("Отрез льна", "Лён", None, (2, 2), None, 3, &[], Sh::Small, false),
    base!("Отрез шерсти", "Шерсть", None, (2, 2), None, 3, &[], Sh::Small, false),
    base!("Солёная рыба", "Рыба", None, (1, 2), None, 5, &[], Sh::Small, false),
    base!("Мешок соли", "Соль", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Сырая шкура", "Шкура", None, (2, 2), None, 3, &[], Sh::Small, false),
    base!("Овчина", "Овчина", None, (2, 2), None, 3, &[], Sh::Small, false),
    base!("Войлок", "Войлок", None, (2, 2), None, 3, &[], Sh::Small, false),
    base!("Воск", "Воск", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Горшок мёда", "Мёд", None, (1, 1), None, 5, &[], Sh::Flask, false),
    // -----------------------------------------------------------------------
    // Травничество, медицина и алхимия (данные о свойствах — в src/lore.rs).
    // Новые предметы добавляйте ниже; ищите всегда по имени.
    // -----------------------------------------------------------------------
    // Местные травы (собираются за стенами).
    base!("Шалфей", "Шалфей", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Чабрец", "Чабрец", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Зверобой", "Зверобой", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Белена", "Белена", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Ягоды можжевельника", "Можжевельник", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Полынь", "Полынь", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Тысячелистник", "Тысячелистник", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Подорожник", "Подорожник", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Шиповник", "Шиповник", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Просвирник", "Просвирник", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Маковые коробочки", "Мак", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Мята", "Мята", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Корень марены", "Марена", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Вайда", "Вайда", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Дубовые галлы", "Галлы", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Корень окопника", "Окопник", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Корень девясила", "Девясил", None, (1, 1), None, 10, &[], Sh::Small, false),
    // Степь (бартер у кочевников).
    base!("Солодковый корень", "Солодка", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Ревень понтийский", "Ревень", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Бараний жир", "Жир", None, (1, 1), None, 5, &[], Sh::Small, false),
    // Привозное (порт).
    base!("Мирра", "Мирра", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Ладан", "Ладан", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Сера", "Сера", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Квасцы", "Квасцы", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Киноварь", "Киноварь", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Опий", "Опий", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Перец", "Перец", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Шафран", "Шафран", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Корица", "Корица", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Хиосская мастика", "Мастика", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Нефть", "Нефть", None, (1, 2), None, 3, &[], Sh::Flask, false),
    base!("Натр", "Натр", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Камедь", "Камедь", None, (1, 1), None, 10, &[], Sh::Small, false),
    // Материалы.
    base!("Сосновая смола", "Смола", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Негашёная известь", "Известь", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Медный купорос", "Купорос", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Свинец", "Свинец", None, (1, 1), None, 5, &[], Sh::Small, false),
    base!("Медный лом", "Медь", None, (1, 1), None, 5, &[], Sh::Small, false),
    base!("Древесная зола", "Зола", None, (1, 1), None, 10, &[], Sh::Small, false),
    base!("Кувшин вина", "Вино", None, (1, 2), None, 2, &[], Sh::Flask, false),
    base!("Кувшин оливкового масла", "Масло", None, (1, 2), None, 2, &[], Sh::Flask, false),
    // Посуда и инструменты.
    base!("Ступка с пестиком", "Ступка", None, (2, 2), None, 1, &[], Sh::Blunt, false),
    base!("Глиняный горшок", "Горшок", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Медный котелок", "Котелок", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Весы с гирями", "Весы", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Пустой флакон", "Флакон", None, (1, 1), None, 5, &[], Sh::Flask, false),
    base!("Тигель", "Тигель", None, (1, 1), None, 1, &[], Sh::Small, false),
    base!("Алембик", "Алембик", None, (2, 3), None, 1, &[], Sh::Flask, false),
    base!("Стеклянная реторта", "Реторта", None, (2, 2), None, 1, &[], Sh::Flask, false),
    base!("Керотакис", "Керотакис", None, (2, 3), None, 1, &[], Sh::Small, false),
    // Тексты.
    base!("Обрывок травника", "Обрывок", None, (1, 1), None, 1, &[], Sh::Small, false),
    base!("Диоскорид. О лекарственных веществах", "Диоскорид", None, (1, 2), None, 1, &[], Sh::Small, false),
    base!("Иллюстрированный травник", "Травник", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Гиппократ. О язвах", "Гиппократ", None, (1, 2), None, 1, &[], Sh::Small, false),
    base!("Гален. О простых лекарствах", "Гален", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Гален. О противоядиях", "Гален", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Орибасий. Синопсис", "Орибасий", None, (2, 2), None, 1, &[], Sh::Small, false),
    base!("Зосим. О приборах и печах", "Зосим", None, (1, 2), None, 1, &[], Sh::Small, false),
    base!("Книга красильщика", "Красильщик", None, (1, 2), None, 1, &[], Sh::Small, false),
    base!("Эней Тактик. Об обороне городов", "Эней", None, (1, 2), None, 1, &[], Sh::Small, false),
    base!("Феофраст. О камнях", "Феофраст", None, (1, 2), None, 1, &[], Sh::Small, false),
    base!("Цельс. О медицине", "Цельс", None, (2, 2), None, 1, &[], Sh::Small, false),
    // Снадобья и продукты (качество = мастерство изготовителя).
    base!("Кровоостанавливающая припарка", "Припарка", None, (1, 1), None, 5, &[], Sh::Small, true),
    base!("Отвар шалфея", "Шалфей", None, (1, 2), None, 3, &[], Sh::Flask, true),
    base!("Отвар чабреца", "Чабрец", None, (1, 2), None, 3, &[], Sh::Flask, true),
    base!("Маковый отвар", "Мак", None, (1, 2), None, 3, &[], Sh::Flask, true),
    base!("Зверобойное масло", "Масло", None, (1, 2), None, 3, &[], Sh::Flask, true),
    base!("Ранозаживляющая мазь", "Мазь", None, (1, 1), None, 5, &[], Sh::Small, true),
    base!("Оксимель", "Оксимель", None, (1, 2), None, 3, &[], Sh::Flask, true),
    base!("Териак", "Териак", None, (1, 1), None, 3, &[], Sh::Small, true),
    base!("Винный уксус", "Уксус", None, (1, 2), None, 2, &[], Sh::Flask, true),
    base!("Ртуть", "Ртуть", None, (1, 1), None, 5, &[], Sh::Flask, false),
    base!("Серная вода", "Серн. вода", None, (1, 2), None, 3, &[], Sh::Flask, true),
    base!("Смоляное масло", "Смол. масло", None, (1, 2), None, 3, &[], Sh::Flask, true),
    base!("Ярь-медянка", "Ярь", None, (1, 1), None, 10, &[], Sh::Small, true),
    base!("Свинцовые белила", "Белила", None, (1, 1), None, 10, &[], Sh::Small, true),
    base!("Сурик", "Сурик", None, (1, 1), None, 10, &[], Sh::Small, true),
    base!("Мареновый лак", "Марена", None, (1, 1), None, 10, &[], Sh::Small, true),
    base!("Вайдовая синь", "Синь", None, (1, 1), None, 10, &[], Sh::Small, true),
    base!("Чернила", "Чернила", None, (1, 1), None, 5, &[], Sh::Flask, true),
    base!("Щёлок", "Щёлок", None, (1, 2), None, 3, &[], Sh::Flask, false),
    base!("Мыло", "Мыло", None, (1, 1), None, 5, &[], Sh::Small, true),
    base!("Зажигательный горшок", "Огонь", None, (2, 2), None, 1, &[], Sh::Small, true),
    // Неудачная варка (brewing.rs).
    base!("Испорченное варево", "Бурда", None, (1, 1), None, 3, &[], Sh::Flask, false),
];

/// Старое название → новое (предметы переименованы под VI век).
pub const RENAMED: &[(&str, &str)] = &[
    ("Одноручный меч", "Спата"),
    ("Длинный меч", "Кавалерийская спата"),
    ("Алебарда", "Пехотное копьё"),
    ("Каплевидный щит", "Овальный щит"),
    ("Баклер", "Круглый щит"),
    ("Топфхельм", "Спангенхельм"),
    ("Шапель", "Гребенчатый шлем"),
    ("Стёганый гамбезон", "Стёганая туника"),
    ("Кольчужный хауберк", "Кольчуга"),
    ("Латная кираса", "Ламеллярный панцирь"),
    ("Латные рукавицы", "Ламеллярные наручи"),
    ("Латные поножи", "Железные поножи"),
    ("Серебряное кольцо", "Серебряный перстень"),
    ("Амулет с гербом", "Нательный крест"),
];

/// Нынешнее название предмета по старому (или то же самое).
pub fn canonical_name(name: &str) -> &str {
    RENAMED.iter().find(|(old, _)| *old == name).map(|(_, new)| *new).unwrap_or(name)
}

/// Номер вида предмета в [`ITEM_BASES`].
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemBaseId(pub u16);

impl ItemBaseId {
    pub fn base(self) -> &'static ItemBase {
        &ITEM_BASES[self.0 as usize]
    }

    /// Вид предмета по имени, если такой есть (торговцы пропускают ещё не добавленные товары;
    /// другие части игры тоже не должны паниковать).
    pub fn find(name: &str) -> Option<ItemBaseId> {
        // Старые названия (до переноса действия в Херсонес 510 г.) продолжают работать.
        let name = canonical_name(name);
        ITEM_BASES.iter().position(|b| b.name == name).map(|i| ItemBaseId(i as u16))
    }

    pub fn by_name(name: &str) -> ItemBaseId {
        Self::find(name).unwrap_or_else(|| panic!("нет предмета «{name}»"))
    }
}

// ---------------------------------------------------------------------------
// Экземпляр предмета
// ---------------------------------------------------------------------------

/// Уникальный номер экземпляра предмета (выдаёт сервер).
pub type ItemUid = u64;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Item {
    pub uid: ItemUid,
    pub base: ItemBaseId,
    pub quality: Quality,
    /// Случайные свойства.
    pub affixes: Vec<(Stat, f32)>,
    pub count: u16,
}

impl Item {
    pub fn info(&self) -> &'static ItemBase {
        self.base.base()
    }

    pub fn size(&self) -> (u8, u8) {
        self.info().size
    }

    pub fn display_name(&self) -> String {
        let base = self.info();
        if self.count > 1 { format!("{} ×{}", base.name, self.count) } else { base.name.to_string() }
    }

    /// Основная характеристика с учётом качества.
    pub fn main_stat(&self) -> Option<(Stat, f32)> {
        self.info().base_stat.map(|(s, v)| (s, (v * self.quality.stat_multiplier()).round()))
    }

    /// Все характеристики предмета: основная + случайные.
    pub fn stats(&self) -> StatTotals {
        let mut totals = StatTotals::default();
        if let Some((s, v)) = self.main_stat() {
            totals.add(s, v);
        }
        for (s, v) in &self.affixes {
            totals.add(*s, *v);
        }
        totals
    }

    /// Справочная цена всей стопки в нуммиях (1 фоллис = 40 нуммиев, 1 солид = 7200).
    /// Настоящую цену у торговца считает `trade.rs`: она зависит от лавки, запаса и репутации.
    pub fn value(&self) -> u32 {
        crate::economy::item_value(self)
    }

    pub fn can_stack_with(&self, other: &Item) -> bool {
        self.base == other.base && self.info().max_stack > 1 && self.quality == other.quality
    }
}

/// Диапазон случайного свойства: значение при обычном качестве (мин, макс).
fn affix_range(stat: Stat) -> (f32, f32) {
    match stat {
        Stat::BonusDamage => (1.0, 4.0),
        Stat::MaxHealth => (4.0, 12.0),
        Stat::Strength | Stat::Agility => (1.0, 3.0),
        Stat::MoveSpeed => (1.0, 3.0),
        Stat::ActionSpeed => (1.5, 4.0),
        Stat::ParryWindow => (10.0, 30.0),
        Stat::BlockStability => (4.0, 10.0),
        Stat::HeadshotReduction => (4.0, 10.0),
        Stat::Luck => (5.0, 15.0),
        Stat::WeaponDamage | Stat::Armor => (0.0, 0.0),
    }
}

/// Выдаёт уникальные номера предметов.
#[derive(Debug, Default, Clone)]
pub struct UidGen(pub ItemUid);

impl UidGen {
    pub fn next(&mut self) -> ItemUid {
        self.0 += 1;
        self.0
    }
}

/// Создаёт предмет заданного вида и качества со случайными свойствами.
pub fn roll_item(base_id: ItemBaseId, quality: Quality, count: u16, rng: &mut Rng, uids: &mut UidGen) -> Item {
    let base = base_id.base();
    let quality = if base.has_quality { quality } else { Quality::Common };
    let mut pool: Vec<Stat> = base.affix_pool.to_vec();
    let mut affixes = Vec::new();
    for _ in 0..quality.affix_count().min(pool.len()) {
        let stat = pool.swap_remove((rng.next_u64() % pool.len() as u64) as usize);
        let (lo, hi) = affix_range(stat);
        // Чем выше качество, тем выше нижняя граница — легендарки не бывают «пустыми».
        let floor = (quality.index() as f32 - 2.0).max(0.0) * 0.12;
        let t = rng.range_f32(floor.min(0.9), 1.0);
        let value = (lo + (hi - lo) * t) * quality.stat_multiplier();
        let value = if stat == Stat::ParryWindow { value.round() } else { (value * 10.0).round() / 10.0 };
        affixes.push((stat, value));
    }
    Item { uid: uids.next(), base: base_id, quality, affixes, count: count.clamp(1, base.max_stack) }
}

// ---------------------------------------------------------------------------
// Расходники
// ---------------------------------------------------------------------------

/// Как действует расходник.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Consumable {
    /// Сколько секунд длится применение (удар или новое действие прерывают его).
    pub use_time: f32,
    /// Сколько здоровья лечит всего.
    pub heal: f32,
    /// 0 — лечит постепенно во время применения (бинт);
    /// больше 0 — лечит постепенно столько секунд после применения (зелье).
    pub heal_over: f32,
    /// Подпись полоски: «Перевязка», «Пьёте зелье».
    pub verb: &'static str,
}

impl ItemBaseId {
    pub fn consumable(self) -> Option<Consumable> {
        match self.base().name {
            "Бинт" => Some(Consumable { use_time: 3.0, heal: 30.0, heal_over: 0.0, verb: "Перевязка" }),
            "Целебное зелье" => Some(Consumable { use_time: 1.0, heal: 50.0, heal_over: 10.0, verb: "Пьёте зелье" }),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Таблицы добычи
// ---------------------------------------------------------------------------

/// Какая таблица добычи у противника или сундука.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LootTableId {
    /// Ополченец / пехотинец.
    Footman,
    /// Рыцарь.
    Knight,
    /// Капитан рыцарей (мини-босс).
    KnightCaptain,
    /// Сундук в замке.
    Chest,
    /// Сундук в доме горожанина: одежда, ткани, немного денег.
    Household,
    /// Кухонные корзины и полки: еда, масло, вино, посуда.
    Kitchen,
    /// Сундук богатого дома (ссыльные, претория).
    Wealthy,
    /// Шкаф аптекаря или врача.
    Apothecary,
    /// Ящик мастерской: материалы и инструменты.
    Workshop,
    /// Ризница церкви.
    Church,
    /// Сундук в казарме.
    Barracks,
    /// Шкаф со свитками (школа, книжная лавка).
    Library,
    /// Склад, таможня, корчма: товары в мешках и амфорах.
    Storehouse,
}

impl LootTableId {
    /// Все таблицы (для проверок).
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [LootTableId; 13] = [
        LootTableId::Footman,
        LootTableId::Knight,
        LootTableId::KnightCaptain,
        LootTableId::Chest,
        LootTableId::Household,
        LootTableId::Kitchen,
        LootTableId::Wealthy,
        LootTableId::Apothecary,
        LootTableId::Workshop,
        LootTableId::Church,
        LootTableId::Barracks,
        LootTableId::Library,
        LootTableId::Storehouse,
    ];
}

pub struct LootTable {
    /// Сколько вещей (кроме золота) выпадает: от и до.
    pub rolls: (u32, u32),
    /// Возможные вещи и их веса.
    pub entries: &'static [(&'static str, f32)],
    /// Веса качеств: Ветхое..Легендарное.
    pub quality_weights: [f32; 6],
    /// Деньги в нуммиях: от и до (0 — не выпадают). Выдаются монетами (economy::coins_for).
    pub gold: (u32, u32),
}

impl LootTableId {
    pub fn table(self) -> LootTable {
        match self {
            LootTableId::Footman => LootTable {
                rolls: (1, 2),
                entries: &[
                    ("Спата", 3.0),
                    ("Булава", 3.0),
                    ("Кинжал", 2.0),
                    ("Круглый щит", 2.0),
                    ("Гребенчатый шлем", 3.0),
                    ("Стёганая туника", 3.0),
                    ("Кожаные перчатки", 2.0),
                    ("Кожаные сапоги", 2.0),
                    ("Бинт", 4.0),
                    ("Обрывок травника", 1.0),
                    ("Подорожник", 1.0),
                ],
                quality_weights: [30.0, 45.0, 18.0, 6.0, 1.0, 0.0],
                // Пара фоллисов и мелочь: жалованье солдата — несколько солидов в год.
                gold: (60, 480),
            },
            LootTableId::Knight => LootTable {
                rolls: (2, 3),
                entries: &[
                    ("Спата", 3.0),
                    ("Кавалерийская спата", 3.0),
                    ("Боевой топор", 2.0),
                    ("Пехотное копьё", 1.5),
                    ("Овальный щит", 3.0),
                    ("Спангенхельм", 3.0),
                    ("Кольчуга", 3.0),
                    ("Ламеллярный панцирь", 1.5),
                    ("Ламеллярные наручи", 2.0),
                    ("Железные поножи", 2.0),
                    ("Серебряный перстень", 1.0),
                    ("Целебное зелье", 2.0),
                    ("Бинт", 2.0),
                    ("Обрывок травника", 1.0),
                    ("Гиппократ. О язвах", 0.3),
                    ("Эней Тактик. Об обороне городов", 0.2),
                ],
                quality_weights: [10.0, 40.0, 30.0, 14.0, 5.0, 1.0],
                gold: (400, 3200),
            },
            LootTableId::KnightCaptain => LootTable {
                rolls: (3, 4),
                entries: &[
                    ("Кавалерийская спата", 3.0),
                    ("Пехотное копьё", 2.0),
                    ("Овальный щит", 2.0),
                    ("Спангенхельм", 2.0),
                    ("Ламеллярный панцирь", 3.0),
                    ("Железные поножи", 2.0),
                    ("Ламеллярные наручи", 2.0),
                    ("Серебряный перстень", 2.0),
                    ("Нательный крест", 2.0),
                    ("Рубин", 1.0),
                    ("Целебное зелье", 2.0),
                    ("Цельс. О медицине", 0.4),
                    ("Эней Тактик. Об обороне городов", 0.4),
                    ("Мирра", 0.6),
                ],
                quality_weights: [0.0, 15.0, 35.0, 30.0, 15.0, 5.0],
                gold: (2400, 12000),
            },
            LootTableId::Chest => LootTable {
                rolls: (1, 3),
                entries: &[
                    ("Серебряный перстень", 2.0),
                    ("Нательный крест", 2.0),
                    ("Рубин", 2.0),
                    ("Целебное зелье", 3.0),
                    ("Бинт", 3.0),
                    ("Кинжал", 1.0),
                    ("Кавалерийская спата", 1.0),
                    // Книги и редкие вещества: свитки часто хранили в сундуках.
                    ("Обрывок травника", 2.0),
                    ("Диоскорид. О лекарственных веществах", 0.6),
                    ("Гиппократ. О язвах", 0.4),
                    ("Орибасий. Синопсис", 0.3),
                    ("Гален. О простых лекарствах", 0.3),
                    ("Гален. О противоядиях", 0.1),
                    ("Зосим. О приборах и печах", 0.2),
                    ("Феофраст. О камнях", 0.3),
                    ("Книга красильщика", 0.3),
                    ("Иллюстрированный травник", 0.15),
                    ("Мирра", 1.0),
                    ("Ладан", 1.0),
                    ("Пустой флакон", 1.0),
                ],
                quality_weights: [5.0, 35.0, 30.0, 20.0, 8.0, 2.0],
                gold: (1200, 9000),
            },
            LootTableId::Household => LootTable {
                rolls: (0, 3),
                entries: &[
                    ("Отрез льна", 3.0),
                    ("Отрез шерсти", 3.0),
                    ("Овчина", 1.0),
                    ("Стёганая туника", 1.0),
                    ("Кожаные сапоги", 1.0),
                    ("Нательный крест", 1.0),
                    ("Бинт", 2.0),
                    ("Воск", 1.5),
                    ("Горшок мёда", 1.0),
                    ("Кинжал", 0.5),
                    ("Серебряный перстень", 0.3),
                    ("Обрывок травника", 0.4),
                ],
                quality_weights: [35.0, 45.0, 15.0, 4.0, 1.0, 0.0],
                // Сбережения ремесленника: несколько фоллисов, редко — тремисс.
                gold: (0, 600),
            },
            LootTableId::Kitchen => LootTable {
                rolls: (1, 3),
                entries: &[
                    ("Солёная рыба", 4.0),
                    ("Кувшин вина", 3.0),
                    ("Кувшин оливкового масла", 2.0),
                    ("Горшок мёда", 1.5),
                    ("Мешок соли", 1.0),
                    ("Глиняный горшок", 2.0),
                    ("Медный котелок", 0.5),
                    ("Чабрец", 1.0),
                    ("Мята", 1.0),
                    ("Бараний жир", 1.0),
                ],
                quality_weights: [30.0, 55.0, 12.0, 3.0, 0.0, 0.0],
                gold: (0, 0),
            },
            LootTableId::Wealthy => LootTable {
                rolls: (2, 4),
                entries: &[
                    ("Серебряный перстень", 3.0),
                    ("Нательный крест", 2.0),
                    ("Рубин", 1.5),
                    ("Стеклянная чаша", 2.0),
                    ("Отрез льна", 2.0),
                    ("Шафран", 0.5),
                    ("Корица", 0.5),
                    ("Перец", 0.8),
                    ("Мирра", 0.8),
                    ("Ладан", 0.8),
                    ("Кавалерийская спата", 0.4),
                    ("Цельс. О медицине", 0.2),
                    ("Феофраст. О камнях", 0.3),
                    ("Эней Тактик. Об обороне городов", 0.2),
                ],
                quality_weights: [0.0, 25.0, 35.0, 25.0, 12.0, 3.0],
                gold: (2000, 14000),
            },
            LootTableId::Apothecary => LootTable {
                rolls: (2, 4),
                entries: &[
                    ("Пустой флакон", 3.0),
                    ("Бинт", 3.0),
                    ("Шалфей", 2.0),
                    ("Зверобой", 2.0),
                    ("Тысячелистник", 2.0),
                    ("Подорожник", 2.0),
                    ("Корень окопника", 1.5),
                    ("Корень девясила", 1.0),
                    ("Солодковый корень", 1.0),
                    ("Белена", 0.6),
                    ("Маковые коробочки", 0.6),
                    ("Мирра", 0.5),
                    ("Квасцы", 0.6),
                    ("Ступка с пестиком", 0.5),
                    ("Целебное зелье", 1.0),
                    ("Обрывок травника", 1.0),
                    ("Диоскорид. О лекарственных веществах", 0.2),
                ],
                quality_weights: [15.0, 50.0, 25.0, 8.0, 2.0, 0.0],
                gold: (0, 300),
            },
            LootTableId::Workshop => LootTable {
                rolls: (1, 3),
                entries: &[
                    ("Медный лом", 2.0),
                    ("Свинец", 2.0),
                    ("Древесная зола", 2.0),
                    ("Воск", 2.0),
                    ("Сосновая смола", 2.0),
                    ("Сырая шкура", 1.5),
                    ("Негашёная известь", 1.0),
                    ("Натр", 0.8),
                    ("Тигель", 0.8),
                    ("Глиняный горшок", 1.5),
                    ("Весы с гирями", 0.4),
                    ("Кинжал", 0.5),
                    ("Кожаные перчатки", 1.0),
                ],
                quality_weights: [25.0, 50.0, 20.0, 5.0, 0.0, 0.0],
                gold: (0, 240),
            },
            LootTableId::Church => LootTable {
                rolls: (1, 3),
                entries: &[
                    ("Воск", 4.0),
                    ("Ладан", 3.0),
                    ("Мирра", 1.0),
                    ("Нательный крест", 2.0),
                    ("Кувшин вина", 1.5),
                    ("Кувшин оливкового масла", 1.5),
                    ("Отрез льна", 1.5),
                    ("Бинт", 1.0),
                    ("Гиппократ. О язвах", 0.15),
                ],
                quality_weights: [5.0, 45.0, 35.0, 12.0, 3.0, 0.0],
                gold: (0, 900),
            },
            LootTableId::Barracks => LootTable {
                rolls: (1, 3),
                entries: &[
                    ("Спата", 2.0),
                    ("Кинжал", 2.0),
                    ("Пехотное копьё", 1.0),
                    ("Круглый щит", 1.0),
                    ("Гребенчатый шлем", 1.5),
                    ("Стёганая туника", 2.0),
                    ("Кожаные перчатки", 1.5),
                    ("Кожаные сапоги", 1.5),
                    ("Бинт", 3.0),
                    ("Солёная рыба", 1.5),
                    ("Кувшин вина", 1.0),
                ],
                quality_weights: [30.0, 50.0, 15.0, 5.0, 0.0, 0.0],
                gold: (30, 600),
            },
            LootTableId::Library => LootTable {
                rolls: (1, 2),
                entries: &[
                    ("Обрывок травника", 4.0),
                    ("Воск", 2.0),
                    ("Дубовые галлы", 1.5),
                    ("Медный купорос", 0.8),
                    ("Камедь", 1.0),
                    ("Диоскорид. О лекарственных веществах", 0.4),
                    ("Гиппократ. О язвах", 0.4),
                    ("Гален. О простых лекарствах", 0.3),
                    ("Орибасий. Синопсис", 0.3),
                    ("Феофраст. О камнях", 0.3),
                    ("Книга красильщика", 0.3),
                    ("Зосим. О приборах и печах", 0.15),
                    ("Эней Тактик. Об обороне городов", 0.2),
                    ("Цельс. О медицине", 0.15),
                    ("Иллюстрированный травник", 0.08),
                ],
                quality_weights: [10.0, 50.0, 28.0, 10.0, 2.0, 0.0],
                gold: (0, 0),
            },
            LootTableId::Storehouse => LootTable {
                rolls: (1, 3),
                entries: &[
                    ("Амфора вина", 3.0),
                    ("Амфора оливкового масла", 2.0),
                    ("Солёная рыба", 3.0),
                    ("Мешок соли", 2.0),
                    ("Отрез шерсти", 1.0),
                    ("Войлок", 1.0),
                    ("Овчина", 1.0),
                    ("Воск", 1.0),
                    ("Горшок мёда", 1.0),
                    ("Сера", 0.3),
                    ("Квасцы", 0.3),
                ],
                quality_weights: [25.0, 55.0, 15.0, 5.0, 0.0, 0.0],
                gold: (0, 0),
            },
        }
    }
}

/// Выбрасывает качество по весам. `luck` (из характеристики «Удача», в процентах)
/// сдвигает шансы к лучшему качеству.
pub fn roll_quality(weights: [f32; 6], luck: f32, rng: &mut Rng) -> Quality {
    let boost = 1.0 + luck.max(0.0) / 100.0;
    let list: Vec<(Quality, f32)> = Quality::ALL.iter().map(|q| (*q, weights[q.index()] * boost.powi(q.index() as i32))).collect();
    rng.weighted(&list).map(|i| list[i].0).unwrap_or(Quality::Common)
}

/// Бросок таблицы добычи: список предметов (золото — отдельной стопкой).
pub fn roll_loot(table: LootTableId, luck: f32, rng: &mut Rng, uids: &mut UidGen) -> Vec<Item> {
    let t = table.table();
    let mut items = Vec::new();
    let entries: Vec<(ItemBaseId, f32)> = t.entries.iter().map(|(n, w)| (ItemBaseId::by_name(n), *w)).collect();
    for _ in 0..rng.range_u32(t.rolls.0, t.rolls.1) {
        let Some(i) = rng.weighted(&entries) else { break };
        let base = entries[i].0;
        let quality = roll_quality(t.quality_weights, luck, rng);
        let count = if base.base().max_stack > 1 { rng.range_u32(1, base.base().max_stack.min(3) as u32) as u16 } else { 1 };
        items.push(roll_item(base, quality, count, rng, uids));
    }
    if t.gold.1 > 0 {
        let nummi = rng.range_u32(t.gold.0, t.gold.1);
        for (base, count) in crate::economy::coins_for(nummi) {
            items.push(roll_item(base, Quality::Common, count, rng, uids));
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_table_entries_exist() {
        for t in LootTableId::ALL {
            for (name, _) in t.table().entries {
                ItemBaseId::by_name(name);
            }
        }
    }

    #[test]
    fn old_names_still_resolve() {
        for (old, new) in RENAMED {
            assert_eq!(ItemBaseId::by_name(old), ItemBaseId::by_name(new));
        }
    }

    #[test]
    fn affix_count_follows_quality() {
        let mut rng = Rng::new(7);
        let mut uids = UidGen::default();
        let sword = ItemBaseId::by_name("Кавалерийская спата");
        for q in Quality::ALL {
            let item = roll_item(sword, q, 1, &mut rng, &mut uids);
            assert_eq!(item.affixes.len(), q.affix_count());
            assert!(item.main_stat().unwrap().1 > 0.0);
        }
    }

    #[test]
    fn luck_improves_quality() {
        let w = LootTableId::Knight.table().quality_weights;
        let avg = |luck: f32| {
            let mut rng = Rng::new(42);
            (0..5000).map(|_| roll_quality(w, luck, &mut rng).index() as f32).sum::<f32>() / 5000.0
        };
        assert!(avg(100.0) > avg(0.0));
    }

    #[test]
    fn loot_always_has_gold() {
        let mut rng = Rng::new(1);
        let mut uids = UidGen::default();
        let loot = roll_loot(LootTableId::Knight, 0.0, &mut rng, &mut uids);
        assert!(loot.iter().any(|i| i.info().shape == ItemShape::Coins));
        assert!(loot.len() >= 3);
    }
}
