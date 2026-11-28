//! Сгенерировано tools/textures/build_surface_textures.py — не править руками.
//! Фотоматериалы ambientCG и Poly Haven (CC0), своя черепица и солома из Blender: assets/textures/surface/<слой>_albedo.jpg и _nrm.jpg.

/// Слой фотоматериала: имя файла, id на ambientCG или Poly Haven, метров на тайл, средний линейный цвет.
pub struct Layer {
    pub name: &'static str,
    pub source: &'static str,
    pub meters: f32,
    pub mean: [f32; 3],
}

pub const LAYERS: &[Layer] = &[
    // 0 — крупные тёсаные блоки известняка, выветренные, с лишайником (фотоскан Poly Haven)
    Layer { name: "ashlar", source: "large_sandstone_blocks_01", meters: 3.00, mean: [0.1955, 0.1598, 0.0974] },
    // 1 — бутовая кладка из рваного известняка на растворе (фотоскан Poly Haven)
    Layer { name: "rubble", source: "medieval_blocks_05", meters: 2.00, mean: [0.3079, 0.2013, 0.1142] },
    // 2 — старая известковая штукатурка в потёках (фотоскан Poly Haven)
    Layer { name: "plaster", source: "worn_mossy_plasterwall", meters: 1.80, mean: [0.2385, 0.2176, 0.1798] },
    // 3 — черепица VI века: тегулы и калиптеры (своя, Blender: tools/textures/blender_byz_roof.py, глина — из фотоскана)
    Layer { name: "roof", source: "MrpgByzRoof", meters: 1.50, mean: [0.1762, 0.0766, 0.0430] },
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
    // 19 — солома и камыш: ряды пучков стеблей внахлёст (своя, Blender: tools/textures/blender_thatch.py)
    Layer { name: "thatch", source: "MrpgThatch", meters: 1.50, mean: [0.2185, 0.1518, 0.0633] },
    // 20 — обожжённая глина: амфоры, пифосы, горшки, миски
    Layer { name: "ceramic", source: "Clay002", meters: 0.90, mean: [0.2990, 0.1198, 0.0648] },
    // 21 — дублёная кожа: мехи, ремни, сёдла, переплёты
    Layer { name: "leather", source: "Leather014", meters: 0.70, mean: [0.0800, 0.0378, 0.0331] },
    // 22 — небелёный лён: навесы, паруса, мешки, занавеси
    Layer { name: "linen", source: "Fabric061", meters: 0.50, mean: [0.2511, 0.2289, 0.2062] },
    // 23 — валяная шерсть: войлок юрт, кошмы, подстилки
    Layer { name: "felt", source: "Fabric034", meters: 0.70, mean: [0.3598, 0.3497, 0.3800] },
    // 24 — тканый шерстяной ковёр
    Layer { name: "rug", source: "Carpet015", meters: 1.10, mean: [0.3205, 0.1043, 0.0589] },
    // 25 — плетёнка из лозы: корзины, плетни, верши
    Layer { name: "wicker", source: "Wicker010B", meters: 0.45, mean: [0.1464, 0.1066, 0.0750] },
    // 26 — свежая известковая побелка: фасады жилых домов
    Layer { name: "limewash", source: "Plaster003", meters: 2.20, mean: [0.6870, 0.6629, 0.6407] },
    // 27 — черепица, вариант 2: другой обжиг, сколы, потёки и лишайник (blender_byz_roof.py, MRPG_SEED=1)
    Layer { name: "roof_b", source: "MrpgByzRoof1", meters: 1.50, mean: [0.1762, 0.0766, 0.0430] },
    // 28 — черепица, вариант 3 (blender_byz_roof.py, MRPG_SEED=2)
    Layer { name: "roof_c", source: "MrpgByzRoof2", meters: 1.50, mean: [0.1762, 0.0766, 0.0430] },
    // 29 — бутовая кладка, вариант 2: ряды помельче, камень ровнее (фотоскан Poly Haven)
    Layer { name: "rubble_b", source: "yellow_stone_wall", meters: 2.20, mean: [0.3079, 0.2013, 0.1142] },
    // 30 — бутовая кладка, вариант 3: плитняк на известковом растворе (фотоскан Poly Haven)
    Layer { name: "rubble_c", source: "castle_wall_varriation", meters: 2.40, mean: [0.3079, 0.2013, 0.1142] },
    // 31 — тёсаные блоки, вариант 2: рваная лицевая сторона, ровные ряды (фотоскан Poly Haven)
    Layer { name: "ashlar_b", source: "sandstone_blocks_05", meters: 3.40, mean: [0.1955, 0.1598, 0.0974] },
    // 32 — тёсаные блоки, вариант 3: гладкие, выветренные морем (фотоскан Poly Haven)
    Layer { name: "ashlar_c", source: "seaworn_sandstone_brick", meters: 3.40, mean: [0.1955, 0.1598, 0.0974] },
    // 33 — старая штукатурка, вариант 2: белёсая, в разводах
    Layer { name: "plaster_b", source: "Plaster007", meters: 2.20, mean: [0.2385, 0.2176, 0.1798] },
    // 34 — старая штукатурка, вариант 3: облупилась до камня (фотоскан Poly Haven)
    Layer { name: "plaster_c", source: "white_rough_plaster", meters: 1.60, mean: [0.2385, 0.2176, 0.1798] },
    // 35 — побелка, вариант 2: гладкая, в волосяных трещинах
    Layer { name: "limewash_b", source: "PaintedPlaster017", meters: 2.20, mean: [0.6879, 0.6639, 0.6405] },
    // 36 — побелка, вариант 3: с разводами от дождя
    Layer { name: "limewash_c", source: "Plaster007", meters: 2.80, mean: [0.6870, 0.6629, 0.6407] },
    // 37 — доски, вариант 2: узкие, обветренные (фотоскан Poly Haven)
    Layer { name: "wood_b", source: "weathered_planks", meters: 4.00, mean: [0.4729, 0.3092, 0.1390] },
    // 38 — доски, вариант 3: старые, с сучками (фотоскан Poly Haven)
    Layer { name: "wood_c", source: "old_planks_02", meters: 4.00, mean: [0.4729, 0.3091, 0.1390] },
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
pub const L_THATCH: u32 = 19;
pub const L_CERAMIC: u32 = 20;
pub const L_LEATHER: u32 = 21;
pub const L_LINEN: u32 = 22;
pub const L_FELT: u32 = 23;
pub const L_RUG: u32 = 24;
pub const L_WICKER: u32 = 25;
pub const L_LIMEWASH: u32 = 26;
pub const L_ROOF_B: u32 = 27;
pub const L_ROOF_C: u32 = 28;
pub const L_RUBBLE_B: u32 = 29;
pub const L_RUBBLE_C: u32 = 30;
pub const L_ASHLAR_B: u32 = 31;
pub const L_ASHLAR_C: u32 = 32;
pub const L_PLASTER_B: u32 = 33;
pub const L_PLASTER_C: u32 = 34;
pub const L_LIMEWASH_B: u32 = 35;
pub const L_LIMEWASH_C: u32 = 36;
pub const L_WOOD_B: u32 = 37;
pub const L_WOOD_C: u32 = 38;
