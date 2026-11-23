//! Сгенерировано tools/textures/build_surface_textures.py — не править руками.
//! Фотоматериалы ambientCG (CC0): assets/textures/surface/<слой>_albedo.jpg и _nrm.jpg.

/// Слой фотоматериала: имя файла, id на ambientCG, метров на тайл, средний линейный цвет.
pub struct Layer {
    pub name: &'static str,
    pub source: &'static str,
    pub meters: f32,
    pub mean: [f32; 3],
}

pub const LAYERS: &[Layer] = &[
    // 0 — тёсаный известняк рядами
    Layer { name: "ashlar", source: "Bricks109A", meters: 2.60, mean: [0.2326, 0.1692, 0.0938] },
    // 1 — бутовая кладка из рваного камня
    Layer { name: "rubble", source: "Bricks102", meters: 2.40, mean: [0.2241, 0.1835, 0.1122] },
    // 2 — старая известковая штукатурка
    Layer { name: "plaster", source: "Plaster007", meters: 2.50, mean: [0.4996, 0.4941, 0.4532] },
    // 3 — глиняная черепица
    Layer { name: "roof", source: "RoofingTiles014B", meters: 2.20, mean: [0.2131, 0.0926, 0.0532] },
    // 4 — грубые старые доски
    Layer { name: "wood", source: "Planks039", meters: 2.40, mean: [0.4729, 0.3091, 0.1390] },
    // 5 — тёмное дерево без швов
    Layer { name: "beam", source: "Wood060", meters: 1.60, mean: [0.1417, 0.0614, 0.0318] },
    // 6 — утоптанная сухая земля
    Layer { name: "earth", source: "Ground109", meters: 3.00, mean: [0.1922, 0.1459, 0.0840] },
    // 7 — трава
    Layer { name: "grass", source: "Grass004", meters: 2.50, mean: [0.1123, 0.1424, 0.0304] },
    // 8 — сухая степная почва
    Layer { name: "soil", source: "Ground102", meters: 3.50, mean: [0.2668, 0.1841, 0.0693] },
    // 9 — слоистая скала
    Layer { name: "rock", source: "Rock026", meters: 4.00, mean: [0.3804, 0.3811, 0.3698] },
    // 10 — булыжная мостовая
    Layer { name: "cobble", source: "PavingStones138", meters: 2.40, mean: [0.3193, 0.2875, 0.1799] },
    // 11 — известняковые плиты пола
    Layer { name: "flagstone", source: "Tiles143", meters: 3.40, mean: [0.7020, 0.4622, 0.2719] },
    // 12 — мрамор
    Layer { name: "marble", source: "Marble014", meters: 2.00, mean: [0.6383, 0.5532, 0.4422] },
    // 13 — старый кирпич (для плинфы)
    Layer { name: "brick", source: "Bricks097", meters: 1.60, mean: [0.2359, 0.1551, 0.0970] },
    // 14 — грубая шерстяная ткань
    Layer { name: "wool", source: "Fabric045", meters: 0.60, mean: [0.5422, 0.5370, 0.5253] },
    // 15 — кованое железо
    Layer { name: "iron", source: "Metal052C", meters: 1.20, mean: [0.5123, 0.5291, 0.5284] },
    // 16 — бронза/медь с патиной
    Layer { name: "bronze", source: "Metal047B", meters: 1.20, mean: [0.1576, 0.0852, 0.0559] },
    // 17 — золото
    Layer { name: "gold", source: "Metal048C", meters: 1.00, mean: [0.9468, 0.7365, 0.3171] },
    // 18 — береговая галька
    Layer { name: "gravel", source: "Gravel041", meters: 2.00, mean: [0.2675, 0.2581, 0.1830] },
];

pub const L_ASHLAR: u32 = 0;
pub const L_RUBBLE: u32 = 1;
pub const L_PLASTER: u32 = 2;
pub const L_ROOF: u32 = 3;
pub const L_WOOD: u32 = 4;
pub const L_BEAM: u32 = 5;
pub const L_EARTH: u32 = 6;
pub const L_GRASS: u32 = 7;
pub const L_SOIL: u32 = 8;
pub const L_ROCK: u32 = 9;
pub const L_COBBLE: u32 = 10;
pub const L_FLAGSTONE: u32 = 11;
pub const L_MARBLE: u32 = 12;
pub const L_BRICK: u32 = 13;
pub const L_WOOL: u32 = 14;
pub const L_IRON: u32 = 15;
pub const L_BRONZE: u32 = 16;
pub const L_GOLD: u32 = 17;
pub const L_GRAVEL: u32 = 18;
