//! Кованое оружие Херсонеса около 510 г.: 74 вида сверх старых шести (мечи, саксы и ножи,
//! топоры, булавы и кистени, копья и дротики) и наконечники стрел.
//!
//! Сама таблица — в `arsenal_table.rs` (её пишет `tools/gen_arsenal.py` из `tools/arsenal_data.py`).
//! Здесь — как семейство куют и как им бьются.

use crate::combat::WeaponKind;
use crate::items::ItemBaseId;
use crate::lore::Technique;
use std::sync::OnceLock;

include!("arsenal_table.rs");

impl Fam {
    /// Как этим бьются (анимации и темп — по образцу старых шести видов).
    pub fn kind(self) -> WeaponKind {
        match self {
            Fam::Sword => WeaponKind::Spatha,
            Fam::Seax => WeaponKind::Seax,
            // Тяжёлое и короткое — по-топорному: замах, сильный удар, слабое парирование.
            Fam::Axe | Fam::Mace => WeaponKind::Axe,
            Fam::Spear => WeaponKind::Spear,
            Fam::Head => WeaponKind::Spatha,
        }
    }

    /// Участки поковки на экране кузни.
    pub fn segments(self) -> &'static [&'static str] {
        match self {
            Fam::Sword => &["хвостовик", "пята", "дол", "дол", "сильная часть", "острие"],
            Fam::Seax => &["хвостовик", "обух", "клинок", "острие"],
            Fam::Axe => &["обух", "проух", "щёки", "лезвие"],
            Fam::Mace => &["втулка", "навершие", "рёбра"],
            Fam::Spear => &["втулка", "шейка", "перо", "острие"],
            Fam::Head => &["черешки", "перья", "острия"],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Fam::Sword => "Мечи",
            Fam::Seax => "Саксы и ножи",
            Fam::Axe => "Топоры",
            Fam::Mace => "Булавы и кистени",
            Fam::Spear => "Копья и дротики",
            Fam::Head => "Наконечники стрел",
        }
    }

    pub const ALL: [Fam; 6] = [Fam::Sword, Fam::Seax, Fam::Axe, Fam::Mace, Fam::Spear, Fam::Head];
}

/// Номер в [`ARMS`] по виду предмета.
pub fn of_base(base: ItemBaseId) -> Option<&'static Arm> {
    static MAP: OnceLock<Vec<Option<u16>>> = OnceLock::new();
    let map = MAP.get_or_init(|| {
        crate::items::ITEM_BASES.iter().map(|b| ARMS.iter().position(|a| a.name == b.name).map(|i| i as u16)).collect()
    });
    map.get(base.0 as usize).copied().flatten().map(|i| &ARMS[i as usize])
}

pub fn find(name: &str) -> Option<&'static Arm> {
    ARMS.iter().find(|a| a.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_arm_is_an_item_with_matching_size_and_price() {
        assert!(ARMS.len() >= 74, "{}", ARMS.len());
        for a in ARMS {
            let base = ItemBaseId::find(a.name).unwrap_or_else(|| panic!("нет предмета {}", a.name));
            assert!(of_base(base).is_some_and(|x| x.name == a.name));
            assert!(a.price > 0 && a.kg > 0.0, "{}", a.name);
            assert!(a.metal >= 1, "{}", a.name);
            if a.fam != Fam::Head {
                assert_eq!(base.base().slot, Some(crate::items::EquipSlot::MainHand), "{}", a.name);
                assert_eq!(WeaponKind::from_item_name(a.name), a.fam.kind(), "{}", a.name);
            }
            for (part, _) in a.parts {
                assert!(ItemBaseId::find(part).is_some(), "{}: нет детали {part}", a.name);
            }
        }
    }

    #[test]
    fn eighty_weapons_in_total() {
        let weapons = crate::items::ITEM_BASES.iter().filter(|b| b.slot == Some(crate::items::EquipSlot::MainHand)).count();
        assert!(weapons >= 80, "{weapons}");
    }
}
