//! Как выглядит надетое: сервер собирает из экипировки игрока короткое описание видимых вещей
//! (`Gear`) и рассылает его в снимках, клиент по нему одевает модель (`rig::Outfit`).
//!
//! Вид вещи определяется по названию предмета, поэтому кованые в кузнице шлемы и доспехи
//! (у них свои названия) тоже видны: «…шлем» — шлем, «кольчуг…» — кольчуга и т. д.

use crate::inventory::Inventory;
use crate::items::{EquipSlot, ItemBaseId};
use serde::{Deserialize, Serialize};

/// Видимые вещи бойца: номер вида предмета (`ItemBaseId`) по слотам. Шлем, торс, руки,
/// ноги, ступни и левая рука (щит). У врагов пусто — их одевает `Outfit::for_actor`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Gear {
    pub head: Option<u16>,
    pub chest: Option<u16>,
    pub hands: Option<u16>,
    pub legs: Option<u16>,
    pub feet: Option<u16>,
    pub off_hand: Option<u16>,
    /// Оружие в правой руке (свой вид у каждого из 80 видов) и убрано ли оно (на поясе / за спиной).
    pub main_hand: Option<u16>,
    pub holstered: bool,
    /// Шея (нательный крест) и перстень.
    pub neck: Option<u16>,
    pub ring: Option<u16>,
    /// Стрелы в сумке — колчан у пояса.
    pub quiver: Option<u16>,
    /// Облик игрока (причёска, борода, кожа, сложение) — из профиля подключения.
    pub look: crate::appearance::Look,
}

impl Gear {
    pub fn from_inventory(inv: &Inventory) -> Self {
        let id = |slot| inv.equipped(slot).map(|i| i.base.0);
        Gear {
            head: id(EquipSlot::Head),
            chest: id(EquipSlot::Chest),
            hands: id(EquipSlot::Hands),
            legs: id(EquipSlot::Legs),
            feet: id(EquipSlot::Feet),
            off_hand: id(EquipSlot::OffHand),
            main_hand: id(EquipSlot::MainHand),
            holstered: false,
            neck: id(EquipSlot::Necklace),
            ring: id(EquipSlot::Ring),
            quiver: inv
                .all_items()
                .find(|i| crate::archery::arrow_type(i.info().name).is_some())
                .map(|i| i.base.0),
            look: Default::default(),
        }
    }

    pub fn helmet(&self) -> Option<Helmet> {
        Helmet::from_name(name(self.head)?)
    }
    pub fn body(&self) -> Option<BodyPiece> {
        BodyPiece::from_name(name(self.chest)?)
    }
    pub fn arms(&self) -> Option<ArmPiece> {
        ArmPiece::from_name(name(self.hands)?)
    }
    pub fn greaves(&self) -> bool {
        name(self.legs).is_some_and(|n| has(n, &["понож", "наголен"]))
    }
    pub fn boots(&self) -> bool {
        self.feet.is_some()
    }
    pub fn shield(&self) -> Option<ShieldLook> {
        let n = name(self.off_hand)?;
        has(n, &["щит"]).then(|| {
            if has(n, &["кругл"]) {
                ShieldLook::Round
            } else {
                ShieldLook::Oval
            }
        })
    }
}

fn name(id: Option<u16>) -> Option<&'static str> {
    let id = id? as usize;
    crate::items::ITEM_BASES
        .get(id)
        .map(|_| ItemBaseId(id as u16).base().name)
}

fn has(name: &str, words: &[&str]) -> bool {
    let n = name.to_lowercase();
    words.iter().any(|w| n.contains(w))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Helmet {
    /// Спангенхельм — сегментный купол с нащёчниками.
    Spangenhelm,
    /// Гребенчатый шлем позднеримского типа: две половины, соединённые гребнем.
    Ridge,
    /// Высокий конический шлем.
    Conical,
    /// Войлочная или кожаная шапка.
    Cap,
}

impl Helmet {
    pub fn from_name(n: &str) -> Option<Self> {
        if has(n, &["спанген"]) {
            Some(Helmet::Spangenhelm)
        } else if has(n, &["гребен"]) {
            Some(Helmet::Ridge)
        } else if has(n, &["шапк", "колпак", "войлок"]) {
            Some(Helmet::Cap)
        } else if has(n, &["шлем", "шелом"]) {
            Some(Helmet::Conical)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyPiece {
    /// Стёганая туника (поддоспешник из многих слоёв льна).
    Gambeson,
    Mail,
    Lamellar,
}

impl BodyPiece {
    pub fn from_name(n: &str) -> Option<Self> {
        if has(n, &["кольчуг"]) {
            Some(BodyPiece::Mail)
        } else if has(n, &["ламелл", "панцир", "чешу"]) {
            Some(BodyPiece::Lamellar)
        } else if has(n, &["стёган", "стеган", "тегиляй", "поддоспеш"])
        {
            Some(BodyPiece::Gambeson)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmPiece {
    Gloves,
    /// Наручи из пластин поверх рукава.
    Vambraces,
}

impl ArmPiece {
    pub fn from_name(n: &str) -> Option<Self> {
        if has(n, &["наруч"]) {
            Some(ArmPiece::Vambraces)
        } else if has(n, &["перчат", "рукавиц"]) {
            Some(ArmPiece::Gloves)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShieldLook {
    Oval,
    Round,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{ITEM_BASES, ItemShape};

    #[test]
    fn every_wearable_has_a_look() {
        for (i, b) in ITEM_BASES.iter().enumerate() {
            let g = |slot| Some(i as u16).filter(|_| b.slot == Some(slot));
            let gear = Gear {
                head: g(EquipSlot::Head),
                chest: g(EquipSlot::Chest),
                hands: g(EquipSlot::Hands),
                legs: g(EquipSlot::Legs),
                feet: g(EquipSlot::Feet),
                off_hand: g(EquipSlot::OffHand),
                ..Default::default()
            };
            match b.slot {
                Some(EquipSlot::Head) => assert!(gear.helmet().is_some(), "{}", b.name),
                Some(EquipSlot::Chest) => assert!(gear.body().is_some(), "{}", b.name),
                Some(EquipSlot::Hands) => assert!(gear.arms().is_some(), "{}", b.name),
                Some(EquipSlot::Legs) => assert!(gear.greaves(), "{}", b.name),
                Some(EquipSlot::Feet) => assert!(gear.boots(), "{}", b.name),
                Some(EquipSlot::OffHand) if b.shape == ItemShape::Shield => {
                    assert!(gear.shield().is_some(), "{}", b.name)
                }
                _ => {}
            }
        }
    }

    #[test]
    fn names_map_to_looks() {
        assert_eq!(Helmet::from_name("Спангенхельм"), Some(Helmet::Spangenhelm));
        assert_eq!(Helmet::from_name("Гребенчатый шлем"), Some(Helmet::Ridge));
        assert_eq!(
            BodyPiece::from_name("Стёганая туника"),
            Some(BodyPiece::Gambeson)
        );
        assert_eq!(
            BodyPiece::from_name("Ламеллярный панцирь"),
            Some(BodyPiece::Lamellar)
        );
        assert_eq!(
            ArmPiece::from_name("Ламеллярные наручи"),
            Some(ArmPiece::Vambraces)
        );
        assert_eq!(
            ArmPiece::from_name("Кожаные перчатки"),
            Some(ArmPiece::Gloves)
        );
    }
}
