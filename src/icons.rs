//! 3D-вид предметов: иконки инвентаря и модели, которые лежат на земле.
//!
//! Обе вещи получаются из одной 3D-модели предмета, которую строит
//! `tools/models/build_items.py` (Blender, всё процедурное — без чужих ассетов):
//! * `assets/icons/items/<ключ>.png` — рендер модели (Cycles) для инвентаря;
//! * `assets/models/items/<ключ>.glb` — та же модель с запечёнными текстурами,
//!   уже в позе «лежит на земле» (низ на y = 0).
//!
//! Как добавить предмет: описать модель в `build_items.py`, запустить скрипт с её ключом
//! и вписать пару «название из items.rs → ключ» в [`ITEM_ART`]. Предмет без записи
//! показывается как холщовый мешочек ([`FALLBACK_ART`]).

use crate::items::{ITEM_BASES, ItemBaseId};
use bevy::prelude::*;

/// Пикселей на клетку инвентаря в файлах иконок (в игре клетка меньше — запас под 4K).
#[allow(dead_code)] // используется в тестах и пригодится интерфейсу
pub const ICON_PX_PER_CELL: u32 = 128;

/// Ключ модели для каждого вида предмета (название — как в `ITEM_BASES`).
pub const ITEM_ART: &[(&str, &str)] = &[
    ("Спата", "spatha"),
    ("Кавалерийская спата", "cavalry_spatha"),
    ("Булава", "mace"),
    ("Боевой топор", "axe"),
    ("Пехотное копьё", "spear"),
    ("Кинжал", "dagger"),
    ("Овальный щит", "shield_oval"),
    ("Круглый щит", "shield_round"),
    ("Спангенхельм", "spangenhelm"),
    ("Гребенчатый шлем", "ridge_helmet"),
    ("Стёганая туника", "padded_tunic"),
    ("Кольчуга", "mail_shirt"),
    ("Ламеллярный панцирь", "lamellar"),
    ("Ламеллярные наручи", "vambraces"),
    ("Кожаные перчатки", "gloves"),
    ("Железные поножи", "greaves"),
    ("Кожаные сапоги", "boots"),
    ("Серебряный перстень", "signet_ring"),
    ("Нательный крест", "pectoral_cross"),
    ("Золотые монеты", "coins"),
    ("Бинт", "bandage"),
    ("Целебное зелье", "flask"),
    ("Рубин", "ruby"),
];

/// Модель для предметов, у которых своей пока нет.
pub const FALLBACK_ART: &str = "sack";

/// Ключ 3D-модели предмета.
pub fn art_key(base: ItemBaseId) -> &'static str {
    let name = base.base().name;
    ITEM_ART.iter().find(|(n, _)| *n == name).map(|(_, k)| *k).unwrap_or(FALLBACK_ART)
}

pub fn icon_path(key: &str) -> String {
    format!("icons/items/{key}.png")
}

pub fn model_path(key: &str) -> String {
    format!("models/items/{key}.glb")
}

/// Иконки всех предметов по номеру вида.
#[derive(Resource)]
pub struct ItemIcons(pub Vec<Handle<Image>>);

impl ItemIcons {
    pub fn get(&self, base: ItemBaseId) -> Handle<Image> {
        self.0[base.0 as usize].clone()
    }
}

/// 3D-модели предметов на земле по номеру вида.
#[derive(Resource)]
pub struct ItemModels(pub Vec<Handle<WorldAsset>>);

impl ItemModels {
    pub fn get(&self, base: ItemBaseId) -> Handle<WorldAsset> {
        self.0[base.0 as usize].clone()
    }
}

pub fn setup_icons(mut commands: Commands, assets: Res<AssetServer>) {
    let ids = || (0..ITEM_BASES.len()).map(|i| art_key(ItemBaseId(i as u16)));
    let icons = ids().map(|k| assets.load(icon_path(k))).collect();
    commands.insert_resource(ItemIcons(icons));
}

pub fn setup_models(mut commands: Commands, assets: Res<AssetServer>) {
    let models = (0..ITEM_BASES.len()).map(|i| assets.load(GltfAssetLabel::Scene(0).from_asset(model_path(art_key(ItemBaseId(i as u16)))))).collect();
    commands.insert_resource(ItemModels(models));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Размер PNG из заголовка IHDR.
    fn png_size(path: &Path) -> (u32, u32) {
        let data = std::fs::read(path).unwrap_or_else(|_| panic!("нет файла {}", path.display()));
        assert_eq!(&data[1..4], b"PNG", "{} — не PNG", path.display());
        let w = u32::from_be_bytes(data[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(data[20..24].try_into().unwrap());
        (w, h)
    }

    #[test]
    fn art_names_match_items() {
        for (name, _) in ITEM_ART {
            assert!(ITEM_BASES.iter().any(|b| b.name == *name), "в ITEM_ART есть «{name}», а такого предмета нет");
        }
    }

    #[test]
    fn every_item_has_own_art() {
        // Новые предметы без модели получают мешочек, но список здесь подскажет, что её нет.
        let missing: Vec<_> = ITEM_BASES.iter().enumerate().filter(|(i, _)| art_key(ItemBaseId(*i as u16)) == FALLBACK_ART).map(|(_, b)| b.name).collect();
        if !missing.is_empty() {
            eprintln!("Предметы без своей 3D-модели (показываются мешочком): {missing:?}");
        }
    }

    #[test]
    fn art_files_exist_and_icons_fit_cells() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let check = |key: &str, size: (u8, u8)| {
            assert!(root.join(model_path(key)).exists(), "нет модели {}", model_path(key));
            let (w, h) = png_size(&root.join(icon_path(key)));
            assert_eq!((w, h), (size.0 as u32 * ICON_PX_PER_CELL, size.1 as u32 * ICON_PX_PER_CELL), "иконка {key} не по размеру клеток предмета");
        };
        for (name, key) in ITEM_ART {
            let base = ITEM_BASES.iter().find(|b| b.name == *name).unwrap();
            check(key, base.size);
        }
        check(FALLBACK_ART, (1, 1));
    }
}
