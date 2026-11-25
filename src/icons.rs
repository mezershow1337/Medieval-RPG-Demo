//! 3D-вид предметов: иконки инвентаря и модели, которые лежат на земле.
//!
//! Обе вещи получаются из одной 3D-модели предмета, которую строит
//! `tools/models/build_items.py` (Blender, всё процедурное — без чужих ассетов):
//! * `assets/icons/items/<ключ>.png` — рендер модели (Cycles) для инвентаря;
//! * `assets/models/items/<ключ>.glb` — та же модель с запечёнными текстурами,
//!   уже в позе «лежит на земле» (низ на y = 0).
//!
//! Как добавить предмет: описать модель в `build_items.py` (с названием из items.rs) и запустить
//! скрипт с её ключом — он сам допишет пару «название → ключ» в `icons/art.rs` ([`ITEM_ART`]). Пока у предмета нет модели,
//! в инвентаре он рисуется временной плоской иконкой (`flat.rs`), а на земле — холщовым мешочком
//! ([`FALLBACK_ART`]).

use crate::items::{ITEM_BASES, ItemBaseId};
use bevy::prelude::*;

/// Временные плоские иконки для предметов без 3D-модели.
mod flat;

/// Пикселей на клетку инвентаря в файлах иконок (в игре клетка меньше — запас под 4K).
#[allow(dead_code)] // используется в тестах и пригодится интерфейсу
pub const ICON_PX_PER_CELL: u32 = 128;

/// Название из `ITEM_BASES` -> ключ модели (файл создаёт `build_items.py`).
mod art;
pub use art::ITEM_ART;

/// Ключ модели -> половинные размеры её коробки (файл создаёт `item_bounds.py`), для физики предметов.
mod shapes;
pub use shapes::ITEM_SHAPES;
/// Ключ модели -> где ось рукояти относительно центра коробки (файл создаёт `item_grips.py`).
mod grips;

/// Сдвиг оси рукояти/топорища от центра коробки модели по X и Y (у топора лезвие сбоку,
/// и топорище не в центре коробки).
pub fn grip_axis_offset(base: ItemBaseId) -> [f32; 2] {
    let key = art_key(base);
    grips::ITEM_GRIPS.iter().find(|(k, _)| *k == key).map(|(_, o)| *o).unwrap_or([0.0, 0.0])
}

/// Половинные размеры коробки, в которую вписана 3D-модель предмета (низ модели — на y = 0).
pub fn model_half_extents(base: ItemBaseId) -> [f32; 3] {
    let key = art_key(base);
    ITEM_SHAPES.iter().find(|(k, _)| *k == key).map(|(_, h)| *h).unwrap_or_else(|| {
        // Модели ещё нет и размеров нет: оцениваем по клеткам инвентаря (клетка ≈ 9 см).
        let (w, h) = base.base().size;
        [0.045 * w as f32, 0.04, 0.045 * h as f32]
    })
}

/// Модель для предметов, у которых своей пока нет.
pub const FALLBACK_ART: &str = "sack";

/// Ключ 3D-модели предмета.
pub fn art_key(base: ItemBaseId) -> &'static str {
    own_art(base).unwrap_or(FALLBACK_ART)
}

/// Своя 3D-модель предмета, если она уже есть.
pub fn own_art(base: ItemBaseId) -> Option<&'static str> {
    let name = base.base().name;
    ITEM_ART.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
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
///
/// Модель грузится, когда её впервые попросили: моделей сотни, у каждой по три картинки 1024²,
/// а в мире и на людях их нужно от силы несколько десятков. Загрузка всех разом стоила
/// гигабайт памяти и десяток секунд фоновой работы на старте.
#[derive(Resource)]
pub struct ItemModels {
    server: AssetServer,
    models: Vec<std::sync::OnceLock<Handle<WorldAsset>>>,
}

impl ItemModels {
    pub fn get(&self, base: ItemBaseId) -> Handle<WorldAsset> {
        self.models[base.0 as usize].get_or_init(|| self.server.load(GltfAssetLabel::Scene(0).from_asset(model_path(art_key(base))))).clone()
    }
}

pub fn setup_icons(mut commands: Commands, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    let icons = (0..ITEM_BASES.len())
        .map(|i| {
            let base = ItemBaseId(i as u16);
            match own_art(base) {
                Some(key) => assets.load(icon_path(key)),
                None => images.add(flat::image(base)),
            }
        })
        .collect();
    commands.insert_resource(ItemIcons(icons));
}

pub fn setup_models(mut commands: Commands, assets: Res<AssetServer>) {
    let models = (0..ITEM_BASES.len()).map(|_| std::sync::OnceLock::new()).collect();
    commands.insert_resource(ItemModels { server: assets.clone(), models });
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
    fn items_without_art_still_have_icons() {
        // Новые предметы без модели получают мешочек, но список здесь подскажет, что её нет.
        let missing: Vec<_> = ITEM_BASES.iter().enumerate().filter(|(i, _)| own_art(ItemBaseId(*i as u16)).is_none()).map(|(_, b)| b.name).collect();
        if !missing.is_empty() {
            eprintln!("Предметы без своей 3D-модели (временная плоская иконка): {missing:?}");
        }
        // Плоская иконка у них должна рисоваться.
        for i in 0..ITEM_BASES.len() {
            let base = ItemBaseId(i as u16);
            if own_art(base).is_none() {
                let (w, h, data) = flat::render_icon(base);
                assert!(w > 0 && h > 0 && data.chunks(4).any(|p| p[3] > 128), "пустая иконка «{}»", base.base().name);
            }
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
