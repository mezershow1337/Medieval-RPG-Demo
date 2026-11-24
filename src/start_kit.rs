//! Стартовый набор («нищий старт»): игрок при создании персонажа выбирает простое оружие и
//! доспех — по два варианта каждого вида. Сверх того всем дают бинт, винный уксус для промывки
//! ран и пять фоллисов.
//!
//! Выбор хранится в settings.txt (`kit = ...`), едет на сервер в профиле подключения
//! (`estates::pack_profile`), и сервер выдаёт по нему вещи при входе и после смерти.

use crate::items::Quality;
use serde::{Deserialize, Serialize};

/// Вариант: предмет (или ничего) и его качество.
pub type Choice = (Option<&'static str>, Quality);

const C: Quality = Quality::Common;
const P: Quality = Quality::Poor;

/// Оружие: по два простых образца каждого вида (мечи, кинжалы, копья, топоры, булавы).
pub const WEAPONS: [(Choice, &str); 10] = [
    ((Some("Спата"), C), "Меч: спата"),
    ((Some("Семиспата"), C), "Меч: семиспата"),
    ((Some("Кинжал"), C), "Кинжал"),
    ((Some("Боевой нож"), C), "Кинжал: боевой нож"),
    ((Some("Пехотное копьё"), C), "Копьё пехотное"),
    ((Some("Ланкия"), C), "Копьё: ланкия"),
    ((Some("Боевой топор"), C), "Топор боевой"),
    ((Some("Плотничий топор"), C), "Топор плотничий"),
    ((Some("Булава"), C), "Булава"),
    ((Some("Окованная палица"), C), "Палица окованная"),
];
pub const SHIELDS: [(Choice, &str); 3] = [
    ((Some("Круглый щит"), C), "Круглый щит"),
    ((Some("Овальный щит"), P), "Овальный щит"),
    ((None, C), "Без щита"),
];
pub const HELMETS: [(Choice, &str); 2] = [
    ((Some("Гребенчатый шлем"), C), "Гребенчатый шлем"),
    ((Some("Спангенхельм"), P), "Спангенхельм (ветхий)"),
];
pub const CHESTS: [(Choice, &str); 2] = [
    ((Some("Стёганая туника"), C), "Стёганая туника"),
    ((Some("Кольчуга"), P), "Кольчуга (ветхая)"),
];
pub const HANDS: [(Choice, &str); 2] = [
    ((Some("Кожаные перчатки"), C), "Кожаные перчатки"),
    ((Some("Ламеллярные наручи"), P), "Наручи (ветхие)"),
];
pub const LEGS: [(Choice, &str); 2] = [
    ((None, C), "Без поножей"),
    ((Some("Железные поножи"), P), "Поножи (ветхие)"),
];

/// Что дают всем сверх выбора: обувь, бинт, уксус для промывки ран и пять фоллисов.
pub const ALWAYS: [(&str, Quality, u16); 4] = [
    ("Кожаные сапоги", P, 1),
    ("Бинт", C, 1),
    ("Винный уксус", C, 1),
    ("Фоллис", C, 5),
];

/// Что можно выбрать в стартовом наборе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KitField {
    Weapon,
    Shield,
    Helmet,
    Chest,
    Hands,
    Legs,
}

impl KitField {
    pub const ALL: [KitField; 6] = [
        KitField::Weapon,
        KitField::Shield,
        KitField::Helmet,
        KitField::Chest,
        KitField::Hands,
        KitField::Legs,
    ];

    pub fn title(self) -> &'static str {
        match self {
            KitField::Weapon => "Оружие",
            KitField::Shield => "Щит",
            KitField::Helmet => "Шлем",
            KitField::Chest => "Доспех",
            KitField::Hands => "Руки",
            KitField::Legs => "Ноги",
        }
    }

    fn options(self) -> &'static [(Choice, &'static str)] {
        match self {
            KitField::Weapon => &WEAPONS,
            KitField::Shield => &SHIELDS,
            KitField::Helmet => &HELMETS,
            KitField::Chest => &CHESTS,
            KitField::Hands => &HANDS,
            KitField::Legs => &LEGS,
        }
    }
}

/// Выбранный набор: номера вариантов в списках выше.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct StartKit {
    pub weapon: u8,
    pub shield: u8,
    pub helmet: u8,
    pub chest: u8,
    pub hands: u8,
    pub legs: u8,
}

impl StartKit {
    fn slot(&mut self, f: KitField) -> &mut u8 {
        match f {
            KitField::Weapon => &mut self.weapon,
            KitField::Shield => &mut self.shield,
            KitField::Helmet => &mut self.helmet,
            KitField::Chest => &mut self.chest,
            KitField::Hands => &mut self.hands,
            KitField::Legs => &mut self.legs,
        }
    }

    /// Выбранный вариант поля.
    pub fn choice(&self, f: KitField) -> (Choice, &'static str) {
        let opts = f.options();
        let mut copy = *self;
        let i = *copy.slot(f) as usize % opts.len();
        opts[i]
    }

    /// Следующий/предыдущий вариант (по кругу).
    pub fn step(&mut self, f: KitField, d: i32) {
        let n = f.options().len() as i32;
        let v = self.slot(f);
        *v = (*v as i32 + d).rem_euclid(n) as u8;
    }

    /// Все предметы набора: (название, качество, количество).
    pub fn items(&self) -> Vec<(&'static str, Quality, u16)> {
        let mut out: Vec<_> = KitField::ALL
            .iter()
            .filter_map(|f| self.choice(*f).0.0.map(|n| (n, self.choice(*f).0.1, 1)))
            .collect();
        out.extend(ALWAYS);
        out
    }

    pub fn pack(&self) -> [u8; 6] {
        [
            self.weapon,
            self.shield,
            self.helmet,
            self.chest,
            self.hands,
            self.legs,
        ]
    }

    /// Из байтов профиля; чужие и испорченные номера приводятся в допустимые.
    pub fn unpack(b: &[u8]) -> Self {
        let get =
            |i: usize, f: KitField| b.get(i).map(|v| *v % f.options().len() as u8).unwrap_or(0);
        StartKit {
            weapon: get(0, KitField::Weapon),
            shield: get(1, KitField::Shield),
            helmet: get(2, KitField::Helmet),
            chest: get(3, KitField::Chest),
            hands: get(4, KitField::Hands),
            legs: get(5, KitField::Legs),
        }
    }

    /// Строка для settings.txt: номера через запятую.
    pub fn to_text(&self) -> String {
        self.pack()
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn from_text(s: &str) -> Option<Self> {
        let v: Vec<u8> = s
            .split(',')
            .map(|x| x.trim().parse().ok())
            .collect::<Option<_>>()?;
        (v.len() == 6).then(|| Self::unpack(&v))
    }

    /// Вид персонажа в этом наборе — для предпросмотра в меню.
    pub fn gear(&self, look: crate::appearance::Look) -> crate::gear_look::Gear {
        let id = |f: KitField| {
            self.choice(f)
                .0
                .0
                .and_then(crate::items::ItemBaseId::find)
                .map(|b| b.0)
        };
        crate::gear_look::Gear {
            head: id(KitField::Helmet),
            chest: id(KitField::Chest),
            hands: id(KitField::Hands),
            legs: id(KitField::Legs),
            feet: crate::items::ItemBaseId::find(ALWAYS[0].0).map(|b| b.0),
            off_hand: id(KitField::Shield),
            main_hand: id(KitField::Weapon),
            look,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kit_item_exists() {
        for f in KitField::ALL {
            for ((name, _), _) in f.options() {
                if let Some(n) = name {
                    assert!(
                        crate::items::ItemBaseId::find(n).is_some(),
                        "нет предмета «{n}»"
                    );
                }
            }
        }
        for (n, _, _) in ALWAYS {
            assert!(
                crate::items::ItemBaseId::find(n).is_some(),
                "нет предмета «{n}»"
            );
        }
    }

    #[test]
    fn two_choices_of_each_weapon_and_armour() {
        assert_eq!(WEAPONS.len(), 10);
        for f in [
            KitField::Helmet,
            KitField::Chest,
            KitField::Hands,
            KitField::Legs,
        ] {
            assert_eq!(f.options().len(), 2, "{f:?}");
        }
    }

    #[test]
    fn round_trip_and_stepping() {
        let mut k = StartKit::default();
        k.step(KitField::Weapon, -1);
        assert_eq!(k.weapon as usize, WEAPONS.len() - 1);
        k.step(KitField::Shield, 2);
        k.step(KitField::Chest, 1);
        assert_eq!(StartKit::unpack(&k.pack()), k);
        assert_eq!(StartKit::from_text(&k.to_text()), Some(k));
        assert_eq!(StartKit::unpack(&[200, 200, 200, 200, 200, 200]).weapon, 0);
        // Всегда есть бинт, уксус и пять фоллисов.
        let items = k.items();
        assert!(items.contains(&("Бинт", Quality::Common, 1)));
        assert!(items.contains(&("Винный уксус", Quality::Common, 1)));
        assert!(items.contains(&("Фоллис", Quality::Common, 5)));
        assert!(items.iter().any(|i| i.0 == "Кольчуга"));
        assert!(!items.iter().any(|i| i.0.contains("щит")), "без щита");
    }
}
