//! Фракции Херсона и его округи около 510 года и репутация игрока у каждой.
//!
//! Внутри стен — имперский город: гарнизон (баллистарии, о которых говорит надпись
//! императора Зенона 488 г., и стратиоты), граждане (курия, ремесленники, лавочники),
//! церковь с епископом и ксеноном (больницей), заморские купцы и знатные ссыльные.
//! За стенами — крымские готы (союзники-федераты из страны Дори), аланы предгорий
//! и кочевые гунно-булгарские орды степи. В порту — своё «дно»: контрабандисты и скупщики краденого.
//!
//! Репутация от −100 до 100. Изменение у одной фракции частично передаётся другим
//! (помог готам — гарнизон доволен союзником, кочевники — нет).
//!
//! Как вернуть доброе имя: отработать смену у мастера (+10–15, по качеству работы),
//! общественные работы в гарнизоне (+10–20) и уплатить штраф — тогда город прощает
//! половину вражды: −100 → −50.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Faction {
    Garrison,
    Citizens,
    Church,
    Merchants,
    Exiles,
    Goths,
    Alans,
    Nomads,
    Underworld,
}

impl Faction {
    pub const ALL: [Faction; 9] = [
        Faction::Garrison,
        Faction::Citizens,
        Faction::Church,
        Faction::Merchants,
        Faction::Exiles,
        Faction::Goths,
        Faction::Alans,
        Faction::Nomads,
        Faction::Underworld,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Faction::Garrison => "Гарнизон Херсона",
            Faction::Citizens => "Граждане Херсона",
            Faction::Church => "Церковь",
            Faction::Merchants => "Заморские купцы",
            Faction::Exiles => "Знатные ссыльные",
            Faction::Goths => "Крымские готы",
            Faction::Alans => "Аланы",
            Faction::Nomads => "Гунно-булгарские кочевники",
            Faction::Underworld => "Портовое дно",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Faction::Garrison => {
                "Баллистарии и стратиоты на жалованье империи. Стерегут стены, укреплённые при Зеноне, \
                 ловят воров и взимают штрафы. Не любят тех, кто продаёт оружие варварам."
            }
            Faction::Citizens => {
                "Курия, ремесленники и лавочники. Стеклодувы, гончары, медники, рыбозасольщики."
            }
            Faction::Church => {
                "Епископ Херсона, клир и монастырский ксенон, где лечат больных. Ценит милостыню и помощь страждущим."
            }
            Faction::Merchants => {
                "Навикулярии из Константинополя, Синопы и Сирии. Привозят вино, масло, стекло, пряности и книги."
            }
            Faction::Exiles => {
                "Опальные сановники и клирики, сосланные из столицы. Скучают по роскоши и платят за неё щедро."
            }
            Faction::Goths => {
                "Готы страны Дори в горах — союзники империи. Меняют шкуры, мёд и воск на вино, ткани и стекло."
            }
            Faction::Alans => {
                "Аланы предгорий. Медь не берут — только золото или товар. Ценят стекло, вино и лекарства."
            }
            Faction::Nomads => {
                "Кочевые орды степи. Меняют степные травы, жир и шкуры на вино, ткани и снадобья. \
                 Чужакам не доверяют; медь им ни к чему, золото — другое дело."
            }
            Faction::Underworld => {
                "Контрабандисты и скупщики краденого портовой таверны. Вопросов не задают, но и честных цен не дают."
            }
        }
    }

    /// Живёт за городскими стенами (суд и штрафы гарнизона там не действуют).
    pub fn outside_walls(self) -> bool {
        matches!(self, Faction::Goths | Faction::Alans | Faction::Nomads)
    }

    /// Подчиняется городским властям: при розыске такие лавки не торгуют с игроком.
    pub fn city_law(self) -> bool {
        matches!(
            self,
            Faction::Garrison | Faction::Citizens | Faction::Church | Faction::Merchants
        )
    }

    /// Начальное отношение к пришлому греку-христианину.
    pub fn initial_reputation(self) -> f32 {
        match self {
            Faction::Church => 5.0,
            Faction::Alans => -10.0,
            Faction::Nomads => -20.0,
            Faction::Underworld => -15.0,
            _ => 0.0,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Какая доля изменения репутации у `from` передаётся фракции `to`.
pub fn spillover(from: Faction, to: Faction) -> f32 {
    use Faction::*;
    match (from, to) {
        (Garrison, Citizens) => 0.3,
        (Garrison, Church) => 0.2,
        (Garrison, Goths) => 0.15,
        (Garrison, Nomads) => -0.2,
        (Garrison, Underworld) => -0.4,
        (Garrison, Exiles) => -0.1,
        (Citizens, Garrison) => 0.2,
        (Citizens, Merchants) => 0.3,
        (Citizens, Church) => 0.2,
        (Citizens, Underworld) => -0.2,
        (Church, Citizens) => 0.2,
        (Church, Garrison) => 0.1,
        (Merchants, Citizens) => 0.2,
        (Exiles, Garrison) => -0.1,
        (Goths, Garrison) => 0.1,
        (Goths, Alans) => 0.1,
        (Goths, Nomads) => -0.3,
        (Alans, Goths) => 0.1,
        (Alans, Nomads) => -0.1,
        (Nomads, Garrison) => -0.2,
        (Nomads, Goths) => -0.2,
        (Nomads, Alans) => -0.1,
        (Underworld, Garrison) => -0.3,
        (Underworld, Citizens) => -0.1,
        _ => 0.0,
    }
}

/// Отношение фракции к игроку словами.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Standing {
    Hostile,
    Unfriendly,
    Neutral,
    Friendly,
    Respected,
    Honored,
}

impl Standing {
    pub fn of(rep: f32) -> Standing {
        match rep {
            r if r <= -60.0 => Standing::Hostile,
            r if r <= -20.0 => Standing::Unfriendly,
            r if r < 20.0 => Standing::Neutral,
            r if r < 50.0 => Standing::Friendly,
            r if r < 80.0 => Standing::Respected,
            _ => Standing::Honored,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Standing::Hostile => "Вражда",
            Standing::Unfriendly => "Неприязнь",
            Standing::Neutral => "Безразличие",
            Standing::Friendly => "Расположение",
            Standing::Respected => "Уважение",
            Standing::Honored => "Почёт",
        }
    }

    pub fn color(self) -> [f32; 3] {
        match self {
            Standing::Hostile => [0.9, 0.25, 0.2],
            Standing::Unfriendly => [0.9, 0.55, 0.3],
            Standing::Neutral => [0.8, 0.78, 0.72],
            Standing::Friendly => [0.55, 0.85, 0.45],
            Standing::Respected => [0.4, 0.8, 0.95],
            Standing::Honored => [0.95, 0.8, 0.35],
        }
    }

    /// Во сколько раз дороже торговец продаёт игроку.
    pub fn sell_markup(self) -> f32 {
        match self {
            Standing::Hostile => 1.6,
            Standing::Unfriendly => 1.25,
            Standing::Neutral => 1.0,
            Standing::Friendly => 0.94,
            Standing::Respected => 0.88,
            Standing::Honored => 0.82,
        }
    }

    /// Во сколько раз щедрее торговец покупает у игрока.
    pub fn buy_bonus(self) -> f32 {
        match self {
            Standing::Hostile => 0.6,
            Standing::Unfriendly => 0.8,
            Standing::Neutral => 1.0,
            Standing::Friendly => 1.06,
            Standing::Respected => 1.12,
            Standing::Honored => 1.18,
        }
    }
}

/// Репутация одного игрока у всех фракций.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Reputation(pub [f32; 9]);

impl Default for Reputation {
    fn default() -> Self {
        let mut r = [0.0; 9];
        for f in Faction::ALL {
            r[f.index()] = f.initial_reputation();
        }
        Self(r)
    }
}

impl Reputation {
    pub fn get(&self, f: Faction) -> f32 {
        self.0[f.index()]
    }

    pub fn standing(&self, f: Faction) -> Standing {
        Standing::of(self.get(f))
    }

    /// Изменить репутацию у фракции; часть изменения уходит её союзникам и врагам.
    pub fn change(&mut self, f: Faction, delta: f32) {
        for other in Faction::ALL {
            let k = if other == f { 1.0 } else { spillover(f, other) };
            if k != 0.0 {
                let v = &mut self.0[other.index()];
                *v = (*v + delta * k).clamp(-100.0, 100.0);
            }
        }
    }

    /// Штраф уплачен: город прощает половину вражды у каждой городской фракции,
    /// у которой игрок в минусе (−100 → −50). Без отголосков у других фракций.
    /// Возвращает, у кого и как изменилось (фракция, было, стало).
    pub fn forgive_half(&mut self) -> Vec<(Faction, f32, f32)> {
        let mut out = Vec::new();
        for f in Faction::ALL.into_iter().filter(|f| f.city_law()) {
            let v = &mut self.0[f.index()];
            if *v < 0.0 {
                let before = *v;
                *v *= 0.5;
                out.push((f, before, *v));
            }
        }
        out
    }
}

/// Сколько репутации даёт смена у мастера: 10 за кое-как, 15 за отличную работу.
pub fn shift_reward(quality: f32) -> f32 {
    10.0 + 5.0 * quality.clamp(0.0, 1.0)
}

/// Общественные работы для гарнизона (носить камень на стены, чистить ров): 10–20.
pub fn public_works_reward(quality: f32) -> f32 {
    10.0 + 10.0 * quality.clamp(0.0, 1.0)
}

/// Сколько репутации даёт смена у мастера этой фракции.
pub fn work_reward(faction: Faction, quality: f32) -> f32 {
    if faction == Faction::Garrison {
        public_works_reward(quality)
    } else {
        shift_reward(quality)
    }
}

/// Подпись к смене в окне торговца: сколько она прибавит к репутации.
pub fn work_reward_label(faction: Faction) -> &'static str {
    if faction == Faction::Garrison {
        "репутация +10–20"
    } else {
        "репутация +10–15"
    }
}

/// Преступления, за которые в городе наказывают.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Crime {
    /// Пойман за руку при краже (furtum manifestum): по римскому праву — штраф в четверную цену.
    TheftCaught,
    /// Продал краденое, и вещь узнали (furtum nec manifestum): двойная цена.
    StolenGoodsRecognized,
    /// Продажа оружия варварам — запрещена законом (Кодекс Юстиниана 4.41).
    ArmsToBarbarians,
}

impl Crime {
    /// Во сколько раз штраф больше цены вещи.
    pub fn fine_multiplier(self) -> u32 {
        match self {
            Crime::TheftCaught => 4,
            Crime::StolenGoodsRecognized => 2,
            Crime::ArmsToBarbarians => 2,
        }
    }

    #[allow(dead_code)] // для подсказок и стражи
    pub fn name(self) -> &'static str {
        match self {
            Crime::TheftCaught => "кража (пойман с поличным)",
            Crime::StolenGoodsRecognized => "сбыт краденого",
            Crime::ArmsToBarbarians => "продажа оружия варварам",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_tiers() {
        assert_eq!(Standing::of(0.0), Standing::Neutral);
        assert_eq!(Standing::of(-70.0), Standing::Hostile);
        assert_eq!(Standing::of(85.0), Standing::Honored);
        assert!(Standing::Honored.sell_markup() < Standing::Neutral.sell_markup());
    }

    #[test]
    fn helping_goths_annoys_nomads() {
        let mut r = Reputation::default();
        let nomads = r.get(Faction::Nomads);
        r.change(Faction::Goths, 20.0);
        assert!(r.get(Faction::Goths) > 19.0);
        assert!(r.get(Faction::Nomads) < nomads);
        assert!(r.get(Faction::Garrison) > 0.0);
    }

    #[test]
    fn paying_fine_forgives_half() {
        let mut r = Reputation::default();
        r.0[Faction::Garrison.index()] = -100.0;
        r.0[Faction::Citizens.index()] = -30.0;
        r.0[Faction::Merchants.index()] = 10.0;
        r.0[Faction::Underworld.index()] = -40.0;
        let changed = r.forgive_half();
        assert_eq!(r.get(Faction::Garrison), -50.0);
        assert_eq!(r.get(Faction::Citizens), -15.0);
        // Кто не в минусе — без изменений; «дно» штрафов гарнизона не прощает.
        assert_eq!(r.get(Faction::Merchants), 10.0);
        assert_eq!(r.get(Faction::Underworld), -40.0);
        assert_eq!(changed.len(), 2);
        // Второй штраф — снова половина: −50 → −25.
        r.forgive_half();
        assert_eq!(r.get(Faction::Garrison), -25.0);
    }

    #[test]
    fn work_gives_reputation() {
        assert_eq!(shift_reward(0.0), 10.0);
        assert_eq!(shift_reward(1.0), 15.0);
        assert_eq!(public_works_reward(0.0), 10.0);
        assert_eq!(public_works_reward(1.0), 20.0);
        assert_eq!(work_reward(Faction::Citizens, 0.5), 12.5);
        assert_eq!(work_reward(Faction::Garrison, 0.5), 15.0);
        // −50 у гарнизона после штрафа: несколько дней общественных работ — и снова безразличие.
        let mut r = Reputation::default();
        r.0[Faction::Garrison.index()] = -50.0;
        for _ in 0..3 {
            r.change(Faction::Garrison, public_works_reward(0.7));
        }
        assert_eq!(r.standing(Faction::Garrison), Standing::Neutral);
    }

    #[test]
    fn reputation_is_clamped() {
        let mut r = Reputation::default();
        r.change(Faction::Church, 500.0);
        assert_eq!(r.get(Faction::Church), 100.0);
    }
}
