//! Именованные места Херсонеса и его округи — точки для других систем игры:
//! торговцы встают у `place("agora")`, учителя — в дидаскалионе, лекари — в ксеноне,
//! травы растут в `herb_patches()`, враги и стражи стоят на `npc_posts()`.
//!
//! Все точки — на полу/земле (y — высота пола), проходимы для стоящего человека;
//! это проверяют тесты города. Пока активен старый уровень-замок, мест нет.

// Часть полей и функций — для систем, которые подключат их позже (обучение, алхимия, задания).
#![allow(dead_code)]

use super::city::{self, crafts, houses, kit::Side, outside::mines, port, public};
pub use super::city::outside::mines::{MineSite, PileSpot, WorkSpots};
use super::terrain::{self, ground_y};
use super::{LevelKind, active};
use bevy::prelude::*;
use std::sync::OnceLock;

/// Что это за место (для торговли, обучения, заданий).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    Apothecary,
    Hospital,
    School,
    Library,
    Bookseller,
    Agora,
    PortMarket,
    Customs,
    Warehouse,
    Glassworks,
    Pottery,
    Smithy,
    Winery,
    FishSalting,
    Thermae,
    Basilica,
    Baptistery,
    Church,
    Cistern,
    Gate,
    Garrison,
    Praetorium,
    Theatre,
    Necropolis,
    Tavern,
    ExilesHouse,
    HerbGarden,
    Quay,
    SteppeCamp,
    GothVillage,
    /// Греческое село хоры (село Святого Фоки).
    Village,
    AlanCamp,
    Spring,
    Vineyard,
    /// Очаг, на котором можно варить отвары.
    Hearth,
    /// Рудник: штольня, шурф, кострища, горн.
    Mine,
    /// Место, где копают болотную руду.
    BogOre,
    /// Стан углежогов в лесу.
    CharcoalCamp,
    ClayPit,
    Beach,
    Quarry,
}

#[derive(Clone, Copy, Debug)]
pub struct Place {
    /// Ключ: латиница, snake_case (`"apothecary_agora"`).
    pub id: &'static str,
    /// Русское название для интерфейса.
    pub name: &'static str,
    /// Как место называли сами херсониты (греческое слово).
    pub greek: &'static str,
    pub kind: PlaceKind,
    /// Точка на полу, где стоит хозяин места (торговец, учитель, лекарь).
    pub pos: Vec3,
    /// Примерный радиус места.
    pub radius: f32,
}

/// Все места активного уровня.
pub fn places() -> &'static [Place] {
    static CITY: OnceLock<Vec<Place>> = OnceLock::new();
    match active() {
        LevelKind::Chersonesus => CITY.get_or_init(city_places),
        LevelKind::Castle => &[],
    }
}

pub fn place(id: &str) -> Option<&'static Place> {
    places().iter().find(|p| p.id == id)
}

pub fn place_pos(id: &str) -> Option<Vec3> {
    place(id).map(|p| p.pos)
}

fn on_ground(x: f32, z: f32) -> Vec3 {
    Vec3::new(x, ground_y(x, z), z)
}

// ---------------- места за стенами ----------------

pub const NECROPOLIS: Vec3 = Vec3::new(215.0, 0.0, 250.0);
pub const KLEROS_FARM: Vec3 = Vec3::new(-178.0, 0.0, 300.0);
pub const STEPPE_CAMP: Vec3 = Vec3::new(200.0, 0.0, 640.0);
pub const GOTH_VILLAGE: Vec3 = Vec3::new(-465.0, 0.0, 515.0);
/// Село Святого Фоки на плато к югу от города (середина площади у колодца).
pub const PHOCAS_VILLAGE: Vec3 = Vec3::new(15.0, 0.0, 300.0);
pub const ALAN_CAMP: Vec3 = Vec3::new(500.0, 0.0, 685.0);
/// Широта родника в балке (x — по дну балки, см. [`terrain::balka_x`]).
pub const SPRING_Z: f32 = 530.0;

pub fn spring_pos() -> Vec3 {
    on_ground(terrain::balka_x(SPRING_Z), SPRING_Z)
}

fn city_places() -> Vec<Place> {
    use PlaceKind::*;
    let gx = city::main_gate_x();
    let sx = city::south_gate_x();
    let a = public::agora_rect();
    let bp = public::basilica_plan();
    let xr = public::xenon_rect();
    let xc = xr.inset(8.0);
    let dr = public::didaskaleion_rect();
    let dc = dr.inset(7.5);
    let er = public::exiles_rect();
    let tr = public::thermae_rect();
    let rr = public::reservoir_rect();
    let wc = public::west_church_rect();
    let cit = city::CITADEL;
    let gw = crafts::glassworks_rect();
    let pt = crafts::pottery_rect();
    let sm = crafts::smithy_rect();
    let ar = public::armoury_rect();
    let gs = city::outside::goth_smithy_rect();
    let gb = city::outside::goth_bloomery_rect();
    let sf = city::outside::steppe_forge_rect();
    let af = city::outside::alan_forge_rect();
    let smith_point = |r: city::kit::Rect, dx: f32, dz: f32| Vec3::new(r.cx() + dx, city::outside::smith_floor(r), r.cz() + dz);
    let wn = crafts::winery_rect();
    let fw = city::block(8, -3);
    let hr = city::block(18, 2);
    let sp = spring_pos();
    let flat = |x: f32, z: f32| Vec3::new(x, 0.0, z);
    let p = |id, name, greek, kind, pos: Vec3, radius| Place { id, name, greek, kind, pos, radius };
    vec![
        p("agora", "Агора", "ἀγορά", Agora, flat(a.cx() - 6.0, a.cz() + 2.0), 22.0),
        p("basilica", "Епископская базилика", "βασιλική", Basilica, flat(bp.naos.x0 + 3.0, bp.naos.cz()), 14.0),
        p("baptistery", "Баптистерий", "βαπτιστήριον", Baptistery, flat(bp.baptistery.x0 + 1.2, bp.baptistery.cz()), 4.0),
        p("xenon", "Ксенон (больница)", "ξενών", Hospital, public::xenon_ward(), 8.0),
        p("xenon_garden", "Аптекарский огород ксенона", "κῆπος", HerbGarden, flat(xc.x0 + 1.7, xc.cz()), 6.0),
        p("iatreion", "Ятрейон (кабинет врача)", "ἰατρεῖον", Hospital, flat(xr.x0 + 4.35, xc.z0 + xc.d() / 4.0 + 1.3), 3.0),
        p("didaskaleion", "Дидаскалион (школа)", "διδασκαλεῖον", School, dc.center(0.0), 8.0),
        p("library", "Книгохранилище дидаскалиона", "βιβλιοθήκη", Library, flat(dr.x0 + 3.75, dc.z0 + dc.d() / 4.0 + 1.3), 3.0),
        p("bibliotheke", "Городская библиотека", "βιβλιοθήκη", Library, city::library::plan().vestibule.center(0.0), 12.0),
        p("bookseller", "Лавка книжника", "βιβλιοπωλεῖον", Bookseller, houses::shop_point(16, -2, Side::S), 2.5),
        p("apothecary_agora", "Аптека у агоры", "μυροπωλεῖον", Apothecary, houses::shop_point(17, -2, Side::S), 2.5),
        p("apothecary_port", "Аптека у порта", "πιγμεντάριον", Apothecary, houses::shop_point(17, 1, Side::N), 2.5),
        p("tavern", "Корчма у главной улицы", "καπηλεῖον", Tavern, houses::shop_point(7, 1, Side::N), 3.0),
        p("tavern_port", "Корчма портового торга", "καπηλεῖον", Tavern, port::tavern_port_point(), 3.0),
        p("exiles_house", "Дом знатных ссыльных", "οἶκος", ExilesHouse, flat(er.cx(), er.z0 + 3.0), 10.0),
        p("thermae", "Термы", "βαλανεῖον", Thermae, flat(tr.x0 + 3.0, tr.z0 + 3.0), 20.0),
        p("theatre", "Заброшенный театр", "θέατρον", Theatre, public::theatre_center(), 25.0),
        p("reservoir", "Резервуар водопровода", "δεξαμενή", Cistern, flat(rr.cx(), rr.z0 - 2.5), 15.0),
        p("garrison", "Цитадель, плац гарнизона", "κάστρον", Garrison, public::garrison_point(), 15.0),
        p("praetorium", "Претория", "πραιτώριον", Praetorium, flat(cit.x0 + 15.0, cit.z0 + 13.0), 8.0),
        p("main_gate", "Главные ворота", "πύλη", Gate, flat(gx + 3.0, 157.0), 6.0),
        p("south_gate", "Южные ворота", "πύλη", Gate, flat(sx, 157.0), 6.0),
        p("port_gate", "Портовые ворота", "πύλη", Gate, flat(276.0, -6.0), 5.0),
        p("port_market", "Портовый торг (эмпорий)", "ἐμπόριον", PortMarket, port::market_point(), 10.0),
        p("customs", "Таможня (коммеркиарий)", "κομμερκιάριον", Customs, port::customs_point(), 6.0),
        p("horrea", "Склады (горреи)", "ὅρρεα", Warehouse, flat(hr.cx(), hr.z0 - 2.0), 20.0),
        p("fish_salting", "Рыбозасолочные цистерны у базилики", "ταριχεία", FishSalting, crafts::fish_salting_point(), 15.0),
        p("fish_salting_west", "Рыбозасолочные цистерны у стены", "ταριχεία", FishSalting, flat(fw.cx(), fw.z0 + 5.0), 10.0),
        p("glassworks", "Стекольная мастерская", "ὑελουργεῖον", Glassworks, flat(gw.cx(), gw.z1 - 3.0), 10.0),
        p("pottery", "Гончарная мастерская", "κεραμεῖον", Pottery, flat(pt.cx(), pt.z0 + 3.0), 10.0),
        p("smithy", "Кузница", "χαλκεῖον", Smithy, flat(sm.cx() + 1.5, sm.z0 + 3.0), 8.0),
        p("iron_yard", "Угольный и железный двор", "σιδηροπωλεῖον", Warehouse, crafts::iron_yard_counter().0 - crafts::iron_yard_counter().1 * SELLER_BEHIND, 8.0),
        p("armoury", "Оружейная мастерская гарнизона", "φαβρίκα", Smithy, flat(ar.x0 + 7.0, ar.z0 + 6.5), 8.0),
        p("winery", "Винодельня (тарапан)", "ληνός", Winery, flat(wn.cx(), wn.z0 + 3.0), 10.0),
        p("church_west", "Западная церковь", "ἐκκλησία", Church, flat(wc.x0 + 4.0, wc.cz()), 8.0),
        p("quay", "Набережная Карантинной бухты", "λιμήν", Quay, flat(293.0, 60.0), 30.0),
        p("necropolis", "Некрополь", "νεκρόπολις", Necropolis, on_ground(NECROPOLIS.x, NECROPOLIS.z), 45.0),
        p("kleros", "Клер (усадьба с виноградником)", "κλῆρος", Vineyard, Vec3::new(KLEROS_FARM.x + 7.0, city::outside::kleros_floor(), KLEROS_FARM.z + 7.5), 60.0),
        p("spring", "Родник в балке", "κρήνη", Spring, on_ground(sp.x + 3.0, sp.z), 6.0),
        p("steppe_camp", "Стан кочевников", "—", SteppeCamp, on_ground(STEPPE_CAMP.x, STEPPE_CAMP.z + 3.0), 25.0),
        p("goth_village", "Готское селение", "—", GothVillage, on_ground(GOTH_VILLAGE.x, GOTH_VILLAGE.z), 30.0),
        p("alan_camp", "Аланский стан", "—", AlanCamp, on_ground(ALAN_CAMP.x, ALAN_CAMP.z + 3.0), 25.0),
        p("goth_smithy", "Кузница готов", "—", Smithy, smith_point(gs, 1.6, 1.4), 6.0),
        p("goth_bloomery", "Сыродутный горн готов", "—", Smithy, smith_point(gb, 1.6, -1.6), 4.0),
        p("phocas_village", "Село Святого Фоки", "κώμη", Village, city::outside::village::stall_point(), 56.0),
        p("phocas_tavern", "Корчма в селе Святого Фоки", "καπηλεῖον", Tavern, city::outside::village::tavern_point(), 4.0),
        p("phocas_pottery", "Сельский гончар", "κεραμεῖον", Pottery, city::outside::village::potter_point(), 6.0),
        p("steppe_forge", "Горн кочевого кузнеца", "—", Smithy, smith_point(sf, 1.6, 1.4), 4.0),
        p("alan_forge", "Горн аланского кузнеца", "—", Smithy, smith_point(af, 1.4, 1.3), 4.0),
        p("mine", "Рудник на рудной горке", "μέταλλον", Mine, mines::mine_yard() + Vec3::new(-4.5, 0.0, -2.0), 20.0),
        p("bog_ore", "Рыжая мочажина (болотная руда)", "—", BogOre, on_ground(mines::west_ravine_x(458.0) + 6.0, 458.0), 20.0),
        p("charcoal_camp", "Стан углежогов", "—", CharcoalCamp, on_ground(mines::CHARCOAL_CAMP.x - 1.0, mines::CHARCOAL_CAMP.y - 1.0), 18.0),
        p("clay_pit", "Глиняный карьер", "—", ClayPit, on_ground(mines::CLAY_PIT.x, mines::CLAY_PIT.y - 6.5), 8.0),
        p("sand_beach", "Песок у Песочной бухты", "—", Beach, on_ground(mines::SAND_BEACH.x + 2.0, mines::SAND_BEACH.y + 3.0), 10.0),
        p("quarry", "Каменоломня за бухтой", "λατομεῖον", Quarry, on_ground(mines::QUARRY.x, mines::QUARRY.y - 9.0), 14.0),
        p("fish_hut", "Рыбацкая лачуга у Песочной бухты", "καλύβη", Quay, city::outside::fishing::trader_point(), 8.0),
    ]
}

/// Место торговца у прилавка: где он стоит и куда смотрит (единичный вектор по горизонтали).
#[derive(Clone, Copy, Debug)]
pub struct StallSlot {
    pub pos: Vec3,
    pub facing: Vec3,
}

/// Сколько от середины стола до продавца за ним.
pub const SELLER_BEHIND: f32 = 0.95;

/// Готовые места для торговцев у места `id`, по порядку занятия. На агоре и портовом торге —
/// лотки под навесами (первые занимают торговцы, см. `staffed_stalls`, остальные стоят с товаром
/// сами по себе), в лавках — за прилавком лицом к двери на улицу. Пусто — место без прилавков.
pub fn stall_slots(id: &str) -> Vec<StallSlot> {
    if active() != LevelKind::Chersonesus {
        return Vec::new();
    }
    let behind = |(table, facing): (Vec3, Vec3)| StallSlot { pos: table - facing * SELLER_BEHIND, facing };
    match id {
        "agora" => public::agora_stalls().into_iter().map(behind).collect(),
        "port_market" => port::market_stalls().into_iter().map(behind).collect(),
        "quay" => port::quay_stalls().into_iter().map(behind).collect(),
        "apothecary_agora" => vec![behind(houses::shop_counter(17, -2, Side::S))],
        "apothecary_port" => vec![behind(houses::shop_counter(17, 1, Side::N))],
        "iron_yard" => vec![behind(crafts::iron_yard_counter())],
        _ => Vec::new(),
    }
}

/// Сколько лотков места `id` занято торговцами (их столы и навесы ставит торговля).
pub fn staffed_stalls(id: &str) -> usize {
    crate::trade::MERCHANTS.iter().filter(|m| m.place == id).count().min(stall_slots(id).len())
}

/// Кузнечные и сыродутные горны активного уровня.
pub fn forges() -> &'static [super::ForgeSpec] {
    &super::shared().forges
}

/// Очаги, на которых можно варить (в домах, корчмах, кухне ксенона, станах).
pub fn hearths() -> &'static [Vec3] {
    &super::shared().hearths
}

/// Лавки в первых этажах домов активного уровня (номер лавки — индекс).
pub fn shops() -> &'static [super::ShopSpec] {
    &super::shared().shops
}

/// Жилые дома горожан активного уровня (номер дома — индекс).
pub fn houses() -> &'static [super::HouseSpec] {
    &super::shared().houses
}

/// В чьём доме точка (номер в [`houses`]): для «залез в чужой дом» и краж.
pub fn house_at(p: Vec3) -> Option<u32> {
    houses().iter().position(|h| h.contains(p)).map(|i| i as u32)
}

// ---------------- горное дело ----------------

/// Места добычи (руда, дрова, глина, песок, камень) — см. `mining.rs`.
pub fn mine_sites() -> &'static [MineSite] {
    match active() {
        LevelKind::Chersonesus => mines::mine_sites(),
        LevelKind::Castle => &[],
    }
}

/// Где складывают угольные кучи и обжиговые кострища.
pub fn pile_spots() -> &'static [PileSpot] {
    match active() {
        LevelKind::Chersonesus => mines::pile_spots(),
        LevelKind::Castle => &[],
    }
}

/// Где работают рудокопы и углежоги (для распорядка дня).
pub fn mine_work_spots() -> Option<WorkSpots> {
    (active() == LevelKind::Chersonesus).then(mines::work_spots)
}

// ---------------- травы ----------------

/// Заросль дикого лекарственного растения. `herb` — латинский род (ключ для травников).
#[derive(Clone, Copy, Debug)]
pub struct HerbPatch {
    pub herb: &'static str,
    pub pos: Vec3,
    pub radius: f32,
}

/// Где что растёт в округе (по местообитаниям Гераклейского полуострова): шалфей и чабрец —
/// на каменистых склонах, полынь — в степи, белена и мальва — у дорог и жилья,
/// мята, окопник и девясил — по сырому дну балки у родника, шиповник — по опушкам.
pub fn herb_patches() -> &'static [HerbPatch] {
    static H: OnceLock<Vec<HerbPatch>> = OnceLock::new();
    if active() != LevelKind::Chersonesus {
        return &[];
    }
    H.get_or_init(|| {
        let wet = |z: f32, dx: f32| (terrain::balka_x(z) + dx, z);
        let list: [(&str, (f32, f32), f32); 36] = [
            ("salvia", (-60.0, 300.0), 7.0),
            ("salvia", (125.0, 380.0), 6.0),
            ("salvia", (-300.0, 425.0), 7.0),
            ("thymus", (40.0, 262.0), 8.0),
            ("thymus", (-225.0, 385.0), 6.0),
            ("thymus", (262.0, 335.0), 6.0),
            ("hypericum", (-150.0, 455.0), 6.0),
            ("hypericum", (300.0, 560.0), 6.0),
            ("hyoscyamus", (128.0, 212.0), 4.0),
            ("hyoscyamus", (-440.0, 545.0), 4.0),
            ("juniperus", (-380.0, 440.0), 8.0),
            ("juniperus", (385.0, 600.0), 8.0),
            ("artemisia", (0.0, 360.0), 9.0),
            ("artemisia", (-80.0, 520.0), 8.0),
            ("artemisia", (240.0, 470.0), 8.0),
            ("achillea", (-20.0, 232.0), 6.0),
            ("achillea", (180.0, 545.0), 6.0),
            ("plantago", (-100.0, 268.0), 5.0),
            ("plantago", (-150.0, 400.0), 5.0),
            ("rosa", (-250.0, 352.0), 5.0),
            ("rosa", (330.0, 650.0), 5.0),
            ("malva", (60.0, 215.0), 5.0),
            ("malva", (-470.0, 490.0), 4.0),
            ("papaver", (-122.0, 232.0), 6.0),
            ("papaver", (-205.0, 345.0), 6.0),
            ("mentha", wet(518.0, 2.0), 3.5),
            ("mentha", wet(600.0, 0.0), 3.5),
            ("rubia", (-170.0, 360.0), 5.0),
            ("isatis", (100.0, 300.0), 6.0),
            ("isatis", (-40.0, 600.0), 6.0),
            ("quercus", (-200.0, 640.0), 10.0),
            ("quercus", (300.0, 720.0), 10.0),
            ("symphytum", wet(470.0, 0.0), 4.0),
            ("symphytum", wet(640.0, 0.0), 4.0),
            ("inula", wet(560.0, 3.0), 4.0),
            ("inula", wet(545.0, -4.0), 3.0),
        ];
        list.iter().map(|&(herb, (x, z), radius)| HerbPatch { herb, pos: on_ground(x, z), radius }).collect()
    })
}

// ---------------- люди ----------------

/// Чьи люди стоят на посту. Разбойники (беглые рабы, дезертиры, изгои из готов и гуннов)
/// не принадлежат ни одной фракции и нападают на всех.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcGroup {
    Garrison,
    Goths,
    Alans,
    Nomads,
    Brigands,
}

impl NpcGroup {
    pub fn hostile(self) -> bool {
        self == NpcGroup::Brigands
    }
    pub fn faction(self) -> Option<crate::factions::Faction> {
        use crate::factions::Faction;
        match self {
            NpcGroup::Garrison => Some(Faction::Garrison),
            NpcGroup::Goths => Some(Faction::Goths),
            NpcGroup::Alans => Some(Faction::Alans),
            NpcGroup::Nomads => Some(Faction::Nomads),
            NpcGroup::Brigands => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NpcPost {
    pub pos: Vec3,
    pub group: NpcGroup,
    /// Куда смотрит (рад, как `yaw` игрока: 0 — на −Z).
    pub yaw: f32,
    /// Насколько далеко можно отходить от поста.
    pub wander: f32,
}

pub fn npc_posts() -> Vec<NpcPost> {
    if active() != LevelKind::Chersonesus {
        return Vec::new();
    }
    use NpcGroup::*;
    use std::f32::consts::PI;
    let gx = city::main_gate_x();
    let sx = city::south_gate_x();
    let flat = |x: f32, z: f32| Vec3::new(x, 0.0, z);
    let post = |pos: Vec3, group, yaw, wander| NpcPost { pos, group, yaw, wander };
    let mut v = vec![
        // Стража у ворот смотрит наружу (на +Z), у портовых — на бухту (+X).
        post(flat(gx - 3.0, 157.5), Garrison, PI, 2.0),
        post(flat(gx + 3.0, 157.5), Garrison, PI, 2.0),
        post(flat(sx - 3.0, 157.5), Garrison, PI, 2.0),
        post(flat(276.0, 3.0), Garrison, -PI / 2.0, 2.0),
        post(public::garrison_point() + Vec3::new(4.0, 0.0, 0.0), Garrison, 0.0, 10.0),
        post(flat(city::cross_x(19), 98.0), Garrison, 0.0, 2.0),
        post(flat(public::agora_rect().x1 - 4.0, -20.0), Garrison, PI / 2.0, 15.0),
        post(flat(gx + 20.0, 174.0), Garrison, 0.0, 30.0),
    ];
    for (x, z) in [(232.0, 330.0), (168.0, 388.0), (terrain::balka_x(452.0), 452.0), (-90.0, 362.0), (-20.0, 525.0), (300.0, 470.0)] {
        v.push(post(on_ground(x, z), Brigands, 0.0, 20.0));
    }
    for (c, group, n) in [(STEPPE_CAMP, Nomads, 4), (GOTH_VILLAGE, Goths, 4), (ALAN_CAMP, Alans, 4)] {
        for k in 0..n {
            let a = k as f32 / n as f32 * 2.0 * PI + 0.4;
            let (x, z) = (c.x + a.cos() * 6.0, c.z + a.sin() * 6.0);
            v.push(post(on_ground(x, z), group, (-a.cos()).atan2(-a.sin()), 12.0));
        }
    }
    v
}
