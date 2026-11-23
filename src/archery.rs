//! Луки и стрелы: из чего собирают стрелу, чем она бьёт, сколько стоит.
//!
//! Цепочка: кузнец куёт наконечники (arsenal.rs, семейство «Наконечники стрел»), из ясеневой
//! заготовки строгают древки, к ним — три маховых пера и рыбий клей (или жилы), наконечник
//! насаживают на черешок. Сборка идёт на верстаке у кузни (smithing_server: `Plan::Fletch`).
//! Лук игрока тратит по стреле на выстрел (server.rs); у стрелков-NPC колчан бесконечный.

use crate::items::{Item, Quality};

/// Вид стрелы. `head` — наконечник, из которого её собирают.
#[derive(Debug)]
pub struct ArrowType {
    pub name: &'static str,
    pub head: &'static str,
    pub desc: &'static str,
}

/// Порядок не менять: номер вида уходит в `Fletch::Arrows`.
pub const ARROWS: &[ArrowType] = &[
    ArrowType {
        name: "Стрелы",
        head: "Наконечники стрел",
        desc: "Листовидный черешковый наконечник на ясеневом древке. Обычная стрела охотника и лучника.",
    },
    ArrowType {
        name: "Бронебойные стрелы",
        head: "Бронебойные наконечники",
        desc: "Узкое гранёное шило: раздвигает кольца кольчуги и пробивает пластины. Рана небольшая.",
    },
    ArrowType {
        name: "Трёхлопастные стрелы",
        head: "Трёхлопастные наконечники",
        desc: "Гуннский трёхлопастный наконечник: три режущих пера, тяжёлая рана, неплохо берёт и доспех.",
    },
    ArrowType {
        name: "Стрелы-срезни",
        head: "Срезни",
        desc: "Широкая лопатка: страшные раны и кровотечение по голому телу, но от доспеха отскакивает.",
    },
];

pub const SHAFTS: &str = "Древки стрел";
pub const FEATHERS: &str = "Маховые перья";
pub const GLUE: &str = "Рыбий клей";
pub const SINEW: &str = "Сухожилия";
pub const ASH: &str = "Ясеневая заготовка";

/// Из одной ясеневой заготовки выходит столько древков.
pub const SHAFTS_PER_BLANK: u16 = 10;
/// Стрел в одной сборке.
pub const BATCH: u16 = 10;
/// Перьев на стрелу.
pub const FEATHERS_PER_ARROW: u16 = 3;

/// Луки: название, урон, врождённая быстрота (%).
pub const BOWS: &[(&str, f32, f32)] = &[
    ("Простой лук", 26.0, 0.0),
    ("Гуннский составной лук", 36.0, -8.0),
];

/// Что делают на верстаке.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fletch {
    /// Расколоть заготовку и выстрогать древки.
    Shafts,
    /// Собрать десяток стрел вида `ARROWS[i]`.
    Arrows(u8),
}

impl Fletch {
    pub fn all() -> Vec<Fletch> {
        let mut v = vec![Fletch::Shafts];
        v.extend((0..ARROWS.len() as u8).map(Fletch::Arrows));
        v
    }

    /// Что выходит и сколько.
    pub fn product(self) -> (&'static str, u16) {
        match self {
            Fletch::Shafts => (SHAFTS, SHAFTS_PER_BLANK),
            Fletch::Arrows(i) => (ARROWS[i as usize].name, BATCH),
        }
    }

    /// Что уходит (кроме клея или жил — их берут что есть).
    pub fn inputs(self) -> Vec<(&'static str, u16)> {
        match self {
            Fletch::Shafts => vec![(ASH, 1)],
            Fletch::Arrows(i) => vec![
                (ARROWS[i as usize].head, BATCH),
                (SHAFTS, BATCH),
                (FEATHERS, BATCH * FEATHERS_PER_ARROW),
            ],
        }
    }

    pub fn needs_glue(self) -> bool {
        matches!(self, Fletch::Arrows(_))
    }

    /// Сколько ударов по верстаку (каждый «в такт» — от 0.03 до 0.1 работы).
    pub fn work(self) -> f32 {
        match self {
            Fletch::Shafts => 0.8,
            Fletch::Arrows(_) => 1.0,
        }
    }

    pub fn verb(self) -> String {
        match self {
            Fletch::Shafts => "Строгать древки".into(),
            Fletch::Arrows(i) => format!("Собрать: {}", ARROWS[i as usize].name.to_lowercase()),
        }
    }
}

/// Вид стрелы по названию предмета.
pub fn arrow_type(name: &str) -> Option<&'static ArrowType> {
    ARROWS.iter().find(|a| a.name == name)
}

pub fn is_arrow(name: &str) -> bool {
    arrow_type(name).is_some()
}

pub fn is_bow(name: &str) -> bool {
    BOWS.iter().any(|(n, ..)| *n == name)
}

/// Прибавка урона (%) и пробивание доспеха (%) от наконечника.
pub fn head_mods(t: &ArrowType) -> (f32, f32) {
    crate::arsenal::find(t.head).map_or((0.0, 0.0), |a| (a.dmg, a.speed))
}

/// Множитель урона от качества стрелы: прямое древко и ровное оперение летят точнее и бьют сильнее.
pub fn quality_mult(q: Quality) -> f32 {
    [0.85, 1.0, 1.05, 1.1, 1.15, 1.2][q.index()]
}

/// Стрела в полёте: урон до брони и пробивание (0.45 — доспех считается на 45% слабее,
/// −0.3 — на 30% крепче).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shot {
    pub damage_mult: f32,
    pub pierce: f32,
}

impl Shot {
    pub const PLAIN: Shot = Shot {
        damage_mult: 1.0,
        pierce: 0.0,
    };

    pub fn of(item: &Item) -> Shot {
        let Some(t) = arrow_type(item.info().name) else {
            return Shot::PLAIN;
        };
        let (dmg, pierce) = head_mods(t);
        Shot {
            damage_mult: (1.0 + dmg / 100.0) * quality_mult(item.quality),
            pierce: pierce / 100.0,
        }
    }
}

/// Качество собранной стрелы: наконечники, мастерство и аккуратность работы.
pub fn assembled_quality(head: Quality, skill: f32, acc: f32, sinew: bool) -> Quality {
    let head = crate::smithing::purity_of(head);
    let skill = (skill / 100.0).clamp(0.0, 1.0);
    let score =
        head * 0.45 + skill * 0.25 + acc.clamp(0.0, 1.0) * 0.3 - if sinew { 0.03 } else { 0.0 };
    crate::smithing::quality_of(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemBaseId;

    #[test]
    fn every_arrow_part_is_an_item() {
        for n in [SHAFTS, FEATHERS, GLUE, SINEW, ASH] {
            assert!(ItemBaseId::find(n).is_some(), "{n}");
        }
        for a in ARROWS {
            assert!(ItemBaseId::find(a.name).is_some(), "{}", a.name);
            assert!(ItemBaseId::find(a.head).is_some(), "{}", a.head);
        }
        for (b, ..) in BOWS {
            let base = ItemBaseId::find(b).unwrap();
            assert_eq!(
                crate::combat::WeaponKind::from_item_name(b),
                crate::combat::WeaponKind::Bow
            );
            assert_eq!(base.base().slot, Some(crate::items::EquipSlot::MainHand));
        }
    }

    #[test]
    fn bodkins_pierce_and_broadheads_cut() {
        let bodkin = head_mods(arrow_type("Бронебойные стрелы").unwrap());
        let broad = head_mods(arrow_type("Стрелы-срезни").unwrap());
        assert!(bodkin.1 > 30.0 && bodkin.0 <= 0.0);
        assert!(broad.0 > 20.0 && broad.1 < 0.0);
        assert!(
            assembled_quality(Quality::Rare, 60.0, 0.9, false).index()
                > assembled_quality(Quality::Poor, 0.0, 0.2, true).index()
        );
    }
}
