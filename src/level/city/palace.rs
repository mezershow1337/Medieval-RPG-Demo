//! Претория комита — резиденция правителя Херсона в западной части цитадели.
//!
//! Так строили наместники поздней империи (претории в Гортине, Аполлонии, дворец дукса в
//! Дура-Европос): перистильный двор с канцелярией и казной, за ним — базиликальный зал
//! приёмов с апсидой, где на возвышении стоит трон под золотой конхой и образ августа, а
//! позади — частные покои (спальни, триклиний со стибадием, кухня) и сад у крепостной стены.
//!
//! План (мировые координаты, цитадель — `CITADEL`):
//! - перистиль `PERI` 29 × 24 м: мраморная колоннада вокруг двора с фонтаном; на север —
//!   скрипторий хартулария, архив и казна сакеллария; на запад — домовая церковь (ораторий) и
//!   келья пресвитера; на восток — вход с караульной, канцелярия трибуна, покой препозита; на
//!   юг — проход к залу, покой трибуна и людская;
//! - переулок `LANE` между двором и залом (боковой вход в зал);
//! - зал приёмов `HALL` 19 × 13 м, высотой 9 м, с двумя колоннадами, апсидой Ø 13 м и
//!   пропилеем из четырёх колонн на плац цитадели;
//! - покои `WING` за залом: спальня комита, покой госпожи, триклиний, кухня;
//! - сад `GARDEN` вдоль южной стены: кипарисы, розы, мраморная чаша, скамья.
//!
//! Места, где бывают комит и его двор (трон, стол нотария, места в триклинии…), — [`seat`];
//! распорядок по ним ведёт `court.rs`, мебель вокруг мест не ставится.

use super::interior::{self, extras};
use super::kit::{self, Op, Rect, Rng, Side};
use super::public::{column, polycandelon};
use super::*;
use crate::collision::Aabb;
use crate::court::Courtier;
use crate::items::LootTableId as L;
use crate::level::Role;
use std::f32::consts::{FRAC_PI_2, PI};

/// Перистильный корпус (наружные грани стен).
pub const PERI: Rect = Rect::new(187.0, 106.5, 216.0, 130.5);
/// Глубина корпусов вокруг двора.
const PERI_DEPTH: f32 = 6.0;
/// Высота стен перистиля.
const PERI_H: f32 = 5.0;
/// Переулок между двором и залом.
pub const LANE: Rect = Rect::new(187.0, 130.5, 216.0, 134.2);
/// Зал приёмов: внутренние грани стен (апсида — к западу от `x0`).
pub const HALL: Rect = Rect::new(196.0, 135.0, 215.2, 148.0);
const HALL_T: f32 = 0.8;
const HALL_H: f32 = 9.0;
const APSE_R: f32 = 6.5;
/// Колоннады зала (оси) и высота колонн.
const COL_Z: [f32; 2] = [137.6, 145.4];
const COL_H: f32 = 5.0;
/// Возвышение (трибунал) в апсиде.
const DAIS: Rect = Rect::new(190.9, 138.2, 196.4, 144.8);
/// Ниже 0,4 м: выше этого навигация жителей считает возвышение лавкой, а не полом.
const DAIS_H: f32 = 0.38;
/// Частные покои (наружные грани; северная стена у восточной части — стена зала).
pub const WING: Rect = Rect::new(187.0, 148.8, 216.0, 158.5);
const WING_H: f32 = 5.0;
/// Перегородки покоев (оси): спальня | покой госпожи | триклиний | кухня.
const WING_SPLIT: [f32; 3] = [195.0, 202.0, 211.0];
/// Сад комита у южной стены цитадели.
pub const GARDEN: Rect = Rect::new(187.0, 158.5, 216.5, 164.25);
/// Пропилей перед залом.
const PROPYLON: Rect = Rect::new(216.02, 135.6, 221.2, 147.4);
/// Высота пола пропилея (мраморная площадка).
const PROPYLON_H: f32 = 0.16;

/// Середина апсиды (центр полукруга на линии западной стены зала).
pub fn apse_center() -> Vec3 {
    Vec3::new(HALL.x0, 0.0, HALL.cz())
}

/// Куда смотрит человек, обращённый лицом по `dir`.
fn face(dir: Vec3) -> f32 {
    crate::daily_life::yaw_to(Vec3::ZERO, dir)
}

/// Места двора: где сидит, стоит, пишет, молится, обедает каждый из приближённых.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Seat {
    /// Трон на возвышении в апсиде.
    Throne,
    /// Стол нотария у возвышения (хартуларий записывает прошения).
    NotaryDesk,
    /// У возвышения справа: сакелларий во время приёма.
    HallTreasurer,
    /// На возвышении слева от трона: трибун.
    HallTribune,
    /// У входа в зал: препозит впускает просителей.
    HallUsher,
    /// Проситель перед троном (протевон курии).
    HallPetition,
    /// Ещё двое просителей.
    HallPetitioner(u8),
    /// Северный портик двора: утренние донесения (комит и хартуларий).
    PorticoNorthA,
    PorticoNorthB,
    /// Южный портик: вечерний разговор (хартуларий и сакелларий).
    PorticoSouthA,
    PorticoSouthB,
    /// Восточный портик: протевон ждёт приёма.
    PorticoEast,
    /// Скрипторий: стол хартулария.
    ScriptoriumDesk,
    /// Архив: полки со свитками.
    ArchiveShelf,
    /// Казна: стол сакеллария и место против него.
    TreasuryDesk,
    TreasuryTalkA,
    TreasuryTalkB,
    /// Ораторий: пресвитер у престола и молящиеся.
    ChapelAltar,
    ChapelPray(u8),
    /// Келья пресвитера: аналой.
    Lectern,
    /// Канцелярия трибуна.
    TribuneTable,
    /// Триклиний: места на стибадии (0 — почётное, комита) и слуга у стола.
    Triclinium(u8),
    TricliniumServe,
    /// Покой госпожи: ткацкий стан.
    Loom,
    /// Кухня: очаг.
    KitchenHearth,
    /// Кухня: разговор у стола (препозит и кухарка).
    KitchenTalkA,
    KitchenTalkB,
    /// Сад: прогулка (комит и госпожа), скамья, игра мальчика.
    GardenA,
    GardenB,
    GardenPlay,
    /// Пост палатной стражи у пропилея (0 — северный, 1 — южный).
    Propylon(u8),
}

/// Где место и куда там смотрят.
pub fn seat(s: Seat) -> (Vec3, f32) {
    let v = |x: f32, z: f32| Vec3::new(x, 0.0, z);
    let (p, dir) = match s {
        Seat::Throne => (Vec3::new(193.65, DAIS_H, 141.5), Vec3::X),
        Seat::NotaryDesk => (v(198.4, 137.25), Vec3::NEG_Z),
        Seat::HallTreasurer => (v(197.9, 145.1), Vec3::new(-1.0, 0.0, -0.6)),
        Seat::HallTribune => (Vec3::new(195.3, DAIS_H, 138.7), Vec3::new(1.0, 0.0, 0.3)),
        Seat::HallUsher => (v(213.6, 139.2), Vec3::NEG_X),
        Seat::HallPetition => (v(200.3, 141.5), Vec3::NEG_X),
        Seat::HallPetitioner(k) => (v(202.2, if k == 0 { 139.9 } else { 143.1 }), Vec3::NEG_X),
        Seat::PorticoNorthA => (v(199.0, 113.6), Vec3::X),
        Seat::PorticoNorthB => (v(200.4, 113.6), Vec3::NEG_X),
        Seat::PorticoSouthA => (v(202.8, 123.4), Vec3::X),
        Seat::PorticoSouthB => (v(204.2, 123.4), Vec3::NEG_X),
        Seat::PorticoEast => (v(208.9, 117.2), Vec3::NEG_X),
        Seat::ScriptoriumDesk => (v(194.9, 108.75), Vec3::NEG_Z),
        Seat::ArchiveShelf => (v(201.5, 108.5), Vec3::NEG_Z),
        Seat::TreasuryDesk => (v(207.1, 110.9), Vec3::NEG_Z),
        Seat::TreasuryTalkA => (v(205.2, 110.9), Vec3::X),
        Seat::TreasuryTalkB => (v(206.5, 110.9), Vec3::NEG_X),
        Seat::ChapelAltar => (v(189.5, 115.5), Vec3::NEG_X),
        Seat::ChapelPray(k) => match k {
            0 => (v(190.75, 115.1), Vec3::NEG_X),
            1 => (v(190.75, 116.2), Vec3::NEG_X),
            2 => (v(191.7, 114.4), Vec3::NEG_X),
            _ => (v(191.7, 116.6), Vec3::NEG_X),
        },
        Seat::Lectern => (v(189.7, 120.0), Vec3::NEG_X),
        Seat::TribuneTable => (v(212.6, 115.0), Vec3::NEG_Z),
        Seat::Triclinium(k) => {
            // Стибадий — полукруглое ложе вокруг круглого стола, открытое к двери (на север).
            let c = triclinium_table();
            let a = [FRAC_PI_2, FRAC_PI_2 + 0.62, FRAC_PI_2 - 0.62, FRAC_PI_2 + 1.2, FRAC_PI_2 - 1.2][k.min(4) as usize];
            let p = c + Vec3::new(a.cos(), 0.0, a.sin()) * 1.5;
            (p, c - p)
        }
        Seat::TricliniumServe => (v(206.5, 151.7), Vec3::Z),
        Seat::Loom => (v(200.6, 153.3), Vec3::X),
        Seat::KitchenHearth => (v(213.6, 156.0), Vec3::new(0.6, 0.0, 1.0)),
        Seat::KitchenTalkA => (v(212.4, 155.2), Vec3::X),
        Seat::KitchenTalkB => (v(213.7, 155.2), Vec3::NEG_X),
        Seat::GardenA => (v(201.4, 161.0), Vec3::X),
        Seat::GardenB => (v(202.7, 161.0), Vec3::NEG_X),
        Seat::GardenPlay => (v(195.5, 161.2), Vec3::new(0.3, 0.0, 1.0)),
        Seat::Propylon(k) => (Vec3::new(217.25, PROPYLON_H, if k == 0 { 139.3 } else { 143.7 }), Vec3::X),
    };
    (p, face(dir))
}

/// Все места (чтобы мебель их не загораживала).
pub(crate) fn all_seats() -> Vec<Seat> {
    let mut v = vec![
        Seat::Throne,
        Seat::NotaryDesk,
        Seat::HallTreasurer,
        Seat::HallTribune,
        Seat::HallUsher,
        Seat::HallPetition,
        Seat::PorticoNorthA,
        Seat::PorticoNorthB,
        Seat::PorticoSouthA,
        Seat::PorticoSouthB,
        Seat::PorticoEast,
        Seat::ScriptoriumDesk,
        Seat::ArchiveShelf,
        Seat::TreasuryDesk,
        Seat::TreasuryTalkA,
        Seat::TreasuryTalkB,
        Seat::ChapelAltar,
        Seat::Lectern,
        Seat::TribuneTable,
        Seat::TricliniumServe,
        Seat::Loom,
        Seat::KitchenHearth,
        Seat::KitchenTalkA,
        Seat::KitchenTalkB,
        Seat::GardenA,
        Seat::GardenB,
        Seat::GardenPlay,
    ];
    v.extend((0..2).map(Seat::HallPetitioner));
    v.extend((0..4).map(Seat::ChapelPray));
    v.extend((0..5).map(Seat::Triclinium));
    v.extend((0..2).map(Seat::Propylon));
    v
}

fn triclinium_table() -> Vec3 {
    Vec3::new(206.5, 0.0, 154.0)
}

/// Где живёт (стоит дома, рядом с постелью) каждый из двора. Ночные стражи и гости «дома» не
/// видны: стража сменяется у пропилея, протевон и просители приходят с переулка.
pub fn court_posts() -> Vec<(Courtier, Vec3, f32)> {
    let v = |x: f32, z: f32| Vec3::new(x, 0.0, z);
    vec![
        (Courtier::Comes, v(191.9, 152.2), face(Vec3::NEG_X)),
        (Courtier::Wife, v(198.0, 152.4), face(Vec3::Z)),
        (Courtier::Son, v(199.7, 154.9), face(Vec3::Z)),
        (Courtier::Chartularius, v(196.6, 110.4), face(Vec3::NEG_Z)),
        (Courtier::Sacellarius, v(205.3, 111.6), face(Vec3::NEG_Z)),
        (Courtier::Tribune, v(196.4, 126.4), face(Vec3::Z)),
        (Courtier::Presbyter, v(191.1, 121.6), face(Vec3::NEG_X)),
        (Courtier::Praepositus, v(212.7, 123.4), face(Vec3::NEG_X)),
        (Courtier::Cook, v(214.3, 154.0), face(Vec3::NEG_X)),
        (Courtier::Guard(0), seat(Seat::Propylon(0)).0, seat(Seat::Propylon(0)).1),
        (Courtier::Guard(1), seat(Seat::Propylon(1)).0, seat(Seat::Propylon(1)).1),
        (Courtier::Guard(2), seat(Seat::Propylon(0)).0 + Vec3::new(0.0, 0.0, 0.5), seat(Seat::Propylon(0)).1),
        (Courtier::Guard(3), seat(Seat::Propylon(1)).0 - Vec3::new(0.0, 0.0, 0.5), seat(Seat::Propylon(1)).1),
        (Courtier::Protevon, v(212.0, 132.3), face(Vec3::NEG_X)),
        (Courtier::Petitioner(0), v(209.0, 132.0), face(Vec3::NEG_X)),
        (Courtier::Petitioner(1), v(206.0, 132.6), face(Vec3::NEG_X)),
    ]
}

/// Роль жителя для каждого из двора (от неё — фракция, добыча, поведение в драке).
pub fn role_of(c: Courtier) -> Role {
    match c {
        Courtier::Comes | Courtier::Tribune => Role::Officer,
        Courtier::Wife => Role::Housewife,
        Courtier::Son => Role::Child,
        Courtier::Chartularius | Courtier::Sacellarius | Courtier::Praepositus => Role::Noble,
        Courtier::Presbyter => Role::Priest,
        Courtier::Cook => Role::Cook,
        Courtier::Guard(_) => Role::Soldier,
        Courtier::Protevon => Role::Householder,
        Courtier::Petitioner(_) => Role::Householder,
    }
}

// ---------------------------------------------------------------------------------------------
// Постройка
// ---------------------------------------------------------------------------------------------

/// Вся претория: двор, переулок, зал, покои, сад, жители двора.
pub(in crate::level) fn praetorium(b: &mut Builder) {
    for s in all_seats() {
        b.keep_clear.push(seat(s).0);
    }
    let mut rng = Rng::new(4881);
    b.at(PERI.cx(), PERI.cz());
    peristyle_court(b, &mut rng);
    b.at(HALL.cx(), HALL.cz());
    lane(b);
    hall(b, &mut rng);
    propylon(b);
    b.at(WING.cx(), WING.cz());
    wing(b, &mut rng);
    garden(b, &mut rng);
    for (c, pos, yaw) in court_posts() {
        b.resident(pos, yaw, role_of(c));
    }
}

/// Мраморная колоннада вокруг двора, скаты портика, мощение и сад с фонтаном.
fn colonnade(b: &mut Builder, court: Rect) -> Rect {
    let cr = court.inset(2.2);
    let ch = 3.6;
    let nx = (cr.w() / 3.0).round().max(1.0) as usize;
    let nz = (cr.d() / 3.0).round().max(1.0) as usize;
    let mut cols = Vec::new();
    for k in 0..=nx {
        let x = cr.x0 + cr.w() * k as f32 / nx as f32;
        cols.push(Vec3::new(x, 0.0, cr.z0));
        cols.push(Vec3::new(x, 0.0, cr.z1));
    }
    for k in 1..nz {
        let z = cr.z0 + cr.d() * k as f32 / nz as f32;
        cols.push(Vec3::new(cr.x0, 0.0, z));
        cols.push(Vec3::new(cr.x1, 0.0, z));
    }
    for p in cols {
        column(b, p, ch, 0.21, Mat::Marble);
    }
    // Мраморный архитрав с карнизом.
    let t = b.mesh(Mat::Marble);
    for (a, c) in [((cr.x0, cr.z0), (cr.x1, cr.z0)), ((cr.x0, cr.z1), (cr.x1, cr.z1)), ((cr.x0, cr.z0), (cr.x0, cr.z1)), ((cr.x1, cr.z0), (cr.x1, cr.z1))] {
        t.beam(Vec3::new(a.0, ch + 0.2, a.1), Vec3::new(c.0, ch + 0.2, c.1), 0.22, 0.2, Vec3::Y);
        t.beam(Vec3::new(a.0, ch + 0.44, a.1), Vec3::new(c.0, ch + 0.44, c.1), 0.28, 0.04, Vec3::Y);
    }
    for (side, r) in [
        (Side::N, Rect::new(court.x0, court.z0, court.x1, cr.z0)),
        (Side::S, Rect::new(court.x0, cr.z1, court.x1, court.z1)),
        (Side::W, Rect::new(court.x0, cr.z0, cr.x0, cr.z1)),
        (Side::E, Rect::new(cr.x1, cr.z0, court.x1, cr.z1)),
    ] {
        kit::lean_roof(b, r, side, PERI_H - 0.2, ch + 0.5, 0.45);
    }
    // Пол портиков — мраморные плиты, двор — мощёная дорожка вокруг сада.
    b.decor([court.x0, 0.0, court.z0], [court.x1, 0.025, cr.z0], Mat::Marble);
    b.decor([court.x0, 0.0, cr.z1], [court.x1, 0.025, court.z1], Mat::Marble);
    b.decor([court.x0, 0.0, cr.z0], [cr.x0, 0.025, cr.z1], Mat::Marble);
    b.decor([cr.x1, 0.0, cr.z0], [court.x1, 0.025, cr.z1], Mat::Marble);
    cr
}

/// Фонтан: восьмигранная мраморная чаша с вазой и струёй.
fn fountain(b: &mut Builder, p: Vec3, r: f32) {
    b.mesh(Mat::Marble).lathe(p, &[(0.0, 0.0), (r, 0.0), (r, 0.5), (r + 0.12, 0.55), (r + 0.12, 0.62), (r - 0.12, 0.62), (r - 0.12, 0.25)], 8);
    b.mesh(Mat::Water).disc(p + Vec3::Y * 0.48, r - 0.12, 8, true);
    b.mesh(Mat::Marble).lathe(p, &[(0.18, 0.0), (0.14, 0.9), (0.12, 1.0), (0.45, 1.15), (0.5, 1.22), (0.38, 1.24), (0.1, 1.2)], 12);
    b.mesh(Mat::Water).disc(p + Vec3::Y * 1.2, 0.36, 10, true);
    b.mesh(Mat::Bronze).lathe(p + Vec3::Y * 1.2, &[(0.05, 0.0), (0.03, 0.18), (0.06, 0.22), (0.0, 0.3)], 8);
    b.invisible.push(Aabb::new(p - Vec3::new(r + 0.1, 0.0, r + 0.1), p + Vec3::new(r + 0.1, 0.62, r + 0.1)));
}

/// Цветник: бордюр из известняка, земля, низкие кусты и розы.
fn flower_bed(b: &mut Builder, r: Rect, rng: &mut Rng) {
    let m = b.mesh(Mat::Limestone);
    m.cuboid(Vec3::new(r.x0, 0.0, r.z0), Vec3::new(r.x1, 0.22, r.z0 + 0.1));
    m.cuboid(Vec3::new(r.x0, 0.0, r.z1 - 0.1), Vec3::new(r.x1, 0.22, r.z1));
    m.cuboid(Vec3::new(r.x0, 0.0, r.z0), Vec3::new(r.x0 + 0.1, 0.22, r.z1));
    m.cuboid(Vec3::new(r.x1 - 0.1, 0.0, r.z0), Vec3::new(r.x1, 0.22, r.z1));
    b.decor([r.x0 + 0.1, 0.0, r.z0 + 0.1], [r.x1 - 0.1, 0.16, r.z1 - 0.1], Mat::Earth);
    let n = ((r.w() * r.d()) / 0.32).ceil() as usize;
    for _ in 0..n {
        let p = Vec3::new(rng.range(r.x0 + 0.3, r.x1 - 0.3), 0.16, rng.range(r.z0 + 0.3, r.z1 - 0.3));
        if rng.chance(0.55) {
            kit::shrub(b, p, rng.range(0.45, 0.8));
        } else {
            rose(b, p, rng);
        }
    }
    b.invisible.push(Aabb::new(r.min3(0.0), r.max3(0.5)));
}

/// Розовый куст: зелень и цветки (красные и белые розы садов Константинополя).
fn rose(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    let h = rng.range(0.4, 0.65);
    b.mesh(Mat::HerbGreen).lathe(p, &[(0.0, 0.0), (0.12, 0.05), (0.24, h * 0.55), (0.18, h * 0.9), (0.0, h)], 7);
    let petal = if rng.chance(0.6) { Mat::PetalRed } else if rng.chance(0.5) { Mat::PetalPink } else { Mat::PetalWhite };
    for k in 0..6 {
        let a = k as f32 * 1.05 + rng.range(0.0, 0.5);
        let q = p + Vec3::new(a.cos() * 0.17, h * rng.range(0.55, 0.95), a.sin() * 0.17);
        b.mesh(petal).lathe(q, &[(0.0, 0.0), (0.045, 0.02), (0.05, 0.05), (0.0, 0.065)], 6);
    }
}

fn peristyle_court(b: &mut Builder, rng: &mut Rng) {
    b.loot(L::Wealthy);
    let entrances = [(Side::E, PERI.cz()), (Side::S, PERI.cx())];
    let peri = super::public::peristyle(b, PERI, PERI_DEPTH, PERI_H, Mat::OpusMixtum, &entrances, [3, 3, 2, 3], false);
    let court = peri.court;
    let cr = colonnade(b, court);
    fountain(b, court.center(0.0), 1.35);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let c = court.center(0.0) + Vec3::new(sx * 3.6, 0.0, sz * 1.9);
        flower_bed(b, Rect::new(c.x - 1.3, c.z - 0.65, c.x + 1.3, c.z + 0.65), rng);
    }
    for (x, z) in [(cr.x0 + 0.9, cr.z0 + 0.9), (cr.x1 - 0.9, cr.z0 + 0.9), (cr.x0 + 0.9, cr.z1 - 0.9), (cr.x1 - 0.9, cr.z1 - 0.9)] {
        kit::cypress(b, Vec3::new(x, 0.0, z), 3.6);
    }
    b.decor([cr.x0, 0.0, cr.z0], [cr.x1, 0.02, cr.z1], Mat::Paving);
    // Лампы портиков (ночью двор не тонет во тьме).
    for (x, z) in [(court.x0 + 1.1, court.z0 + 1.1), (court.x1 - 1.1, court.z1 - 1.1)] {
        interior::lampstand(b, Vec3::new(x, 0.0, z), true);
    }
    let all = [Side::N, Side::S, Side::W, Side::E];
    for (side, room) in &peri.rooms {
        let c = room.center(0.0);
        let mid = |a: f32| (room.x0 + room.x1) / 2.0 - a;
        match side {
            Side::N if (c.x - 195.8).abs() < 1.0 => scriptorium(b, *room, rng),
            Side::N if (c.x - 201.5).abs() < 1.0 => archive(b, *room, rng),
            Side::N if (c.x - 207.2).abs() < 1.0 => treasury(b, *room),
            Side::W if c.z < 118.5 => chapel(b, *room),
            Side::W => presbyter_cell(b, *room, rng),
            Side::E if c.z < 116.6 => tribune_office(b, *room, rng),
            Side::E if c.z < 120.6 => vestibule(b, *room),
            Side::E => {
                let bed = Rect::new(room.x1 - 0.95, room.z0 + 0.1, room.x1 - 0.05, room.z0 + 2.0);
                interior::bed(b, bed, rng);
                interior::rug(b, Rect::new(room.x0 + 0.8, room.z0 + 0.6, room.x0 + 2.6, room.z1 - 0.6), Mat::Rug);
                extras(b, *room, L::Wealthy, &["Шкаф препозита", "Ларь с ключами"], &[], rng);
                interior::lampstand(b, Vec3::new(room.x0 + 0.35, 0.0, room.z1 - 0.35), false);
            }
            Side::S if (c.x - 195.8).abs() < 1.0 => {
                // Покой трибуна: ложе, стойка с парадной кольчугой, сундук.
                interior::bed(b, Rect::new(room.x0 + 0.1, room.z1 - 1.0, room.x0 + 2.0, room.z1 - 0.1), rng);
                armour_stand(b, Vec3::new(room.x1 - 0.5, 0.0, room.z1 - 0.5), false);
                extras(b, *room, L::Barracks, &["Сундук трибуна"], &[], rng);
            }
            Side::S if (c.x - 201.5).abs() < 1.0 => {
                // Проход к залу: светильники, ковровая дорожка, завеса.
                interior::rug(b, Rect::new(c.x - 0.6, room.z0 + 0.2, c.x + 0.6, room.z1 - 0.2), Mat::Rug);
                for x in [room.x0 + 0.4, room.x1 - 0.4] {
                    interior::lampstand(b, Vec3::new(x, 0.0, c.z), x < c.x);
                }
                super::cozy::wall_hanging(b, Vec3::new(room.x0 + 0.02, 2.0, c.z), Vec3::X, 1.6, 1.6, rng);
                super::cozy::wall_hanging(b, Vec3::new(room.x1 - 0.02, 2.0, c.z), Vec3::NEG_X, 1.6, 1.6, rng);
            }
            Side::S if (c.x - 207.2).abs() < 1.0 => {
                // Людская: ложа слуг вдоль южной стены, корзины.
                super::public::beds_along_edge(b, *room, true, rng);
                let people = [(Role::Servant, c + Vec3::new(-1.0, 0.0, -0.6), c), (Role::Servant, c + Vec3::new(1.2, 0.0, -0.6), c)];
                extras(b, Rect::new(room.x0, room.z0, room.x1, room.z1 - 2.3), L::Household, &["Корзина", "Сундук слуг"], &people, rng);
            }
            _ => {
                // Глухие угловые кладовые.
                let _ = mid(0.0);
                extras(b, *room, L::Storehouse, &[], &[], rng);
                for k in 0..3 {
                    kit::amphora(b, Vec3::new(room.x0 + 0.5 + k as f32 * 0.6, 0.0, room.z0 + 0.5), 1.0, k);
                }
            }
        }
        let _ = &all;
    }
}

/// Скрипторий хартулария: стол с кодексом, ложе, шкаф с грамотами, полка с кодексами.
fn scriptorium(b: &mut Builder, room: Rect, rng: &mut Rng) {
    let (p, _) = seat(Seat::ScriptoriumDesk);
    interior::desk_codex(b, Rect::new(p.x - 0.75, room.z0 + 0.05, p.x + 0.75, room.z0 + 0.75));
    inkwell(b, Vec3::new(p.x + 0.45, 0.8, room.z0 + 0.45));
    interior::bed(b, Rect::new(room.x0 + 0.1, room.z1 - 2.0, room.x0 + 1.0, room.z1 - 0.1), rng);
    interior::cupboard(b, Rect::new(room.x1 - 0.55, room.z0 + 0.4, room.x1 - 0.05, room.z0 + 1.4), Side::E, "Шкаф с грамотами", rng);
    codex_shelf(b, Rect::new(room.x1 - 0.45, room.z0 + 1.7, room.x1 - 0.05, room.z0 + 3.1), rng);
    interior::lampstand(b, Vec3::new(p.x - 1.0, 0.0, p.z + 0.3), true);
    interior::rug(b, Rect::new(room.x0 + 1.3, room.z0 + 1.3, room.x0 + 3.2, room.z1 - 0.4), Mat::Rug);
}

/// Чернильница и тростниковое перо.
fn inkwell(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Bronze).lathe(p, &[(0.0, 0.0), (0.045, 0.0), (0.05, 0.05), (0.03, 0.07), (0.0, 0.07)], 8);
    b.mesh(Mat::Wood).beam(p + Vec3::new(0.01, 0.05, 0.0), p + Vec3::new(0.12, 0.22, 0.06), 0.006, 0.006, Vec3::Y);
}

/// Полка с кодексами в кожаных переплётах (стоят и лежат стопками).
fn codex_shelf(b: &mut Builder, r: Rect, rng: &mut Rng) {
    let along_x = r.w() > r.d();
    b.invisible.push(Aabb::new(r.min3(0.0), r.max3(1.9)));
    let m = b.mesh(Mat::Wood);
    for k in 0..4 {
        let y = 0.1 + k as f32 * 0.55;
        m.cuboid(r.min3(y), r.max3(y + 0.04));
    }
    for (x, z) in [(r.x0, r.z0), (r.x1 - 0.04, r.z0), (r.x0, r.z1 - 0.04), (r.x1 - 0.04, r.z1 - 0.04)] {
        m.cuboid(Vec3::new(x, 0.0, z), Vec3::new(x + 0.04, 1.85, z + 0.04));
    }
    let len = if along_x { r.w() } else { r.d() };
    for k in 0..4 {
        let y = 0.14 + k as f32 * 0.55;
        let mut s = 0.06;
        while s < len - 0.12 {
            let w = rng.range(0.05, 0.09);
            let h = rng.range(0.26, 0.34);
            let d = (if along_x { r.d() } else { r.w() }) * 0.8;
            let mat = if rng.chance(0.7) { Mat::Leather } else if rng.chance(0.5) { Mat::Rug } else { Mat::Cloth };
            let (a, c) = if along_x {
                (Vec3::new(r.x0 + s, y, r.cz() - d / 2.0), Vec3::new(r.x0 + s + w, y + h, r.cz() + d / 2.0))
            } else {
                (Vec3::new(r.cx() - d / 2.0, y, r.z0 + s), Vec3::new(r.cx() + d / 2.0, y + h, r.z0 + s + w))
            };
            b.mesh(mat).cuboid(a, c);
            // Золочёный обрез у богатых списков.
            if mat != Mat::Leather && rng.chance(0.5) {
                b.mesh(Mat::Gold).cuboid(a + Vec3::Y * (h * 0.45), Vec3::new(c.x, a.y + h * 0.5, c.z));
            }
            s += w + rng.range(0.005, 0.02);
            if rng.chance(0.12) {
                s += 0.12;
            }
        }
    }
}

/// Стойка со свитками (ячейки-«голубятни», в каждой — свёрнутые свитки).
fn scroll_rack(b: &mut Builder, r: Rect, rng: &mut Rng) {
    let along_x = r.w() > r.d();
    let len = if along_x { r.w() } else { r.d() };
    b.invisible.push(Aabb::new(r.min3(0.0), r.max3(2.0)));
    let m = b.mesh(Mat::Wood);
    m.cuboid(r.min3(0.0), r.max3(0.08));
    m.cuboid(r.min3(1.95), r.max3(2.0));
    let cells = (len / 0.45).floor().max(1.0) as usize;
    for k in 0..=cells {
        let s = len * k as f32 / cells as f32;
        let (a, c) = if along_x { (Vec3::new(r.x0 + s - 0.02, 0.0, r.z0), Vec3::new(r.x0 + s + 0.02, 2.0, r.z1)) } else { (Vec3::new(r.x0, 0.0, r.z0 + s - 0.02), Vec3::new(r.x1, 2.0, r.z0 + s + 0.02)) };
        m.cuboid(a, c);
    }
    for lvl in 0..4 {
        let y = 0.08 + lvl as f32 * 0.47;
        b.mesh(Mat::Wood).cuboid(r.min3(y), r.max3(y + 0.03));
        for k in 0..cells {
            let s0 = len * k as f32 / cells as f32;
            let n = 3 + rng.int(4);
            for q in 0..n {
                let rr = rng.range(0.035, 0.05);
                let off = s0 + 0.08 + (q % 3) as f32 * 0.11;
                let yy = y + 0.03 + rr + (q / 3) as f32 * 0.1;
                let d = if along_x { r.d() } else { r.w() };
                let (a, c) = if along_x { (Vec3::new(r.x0 + off, yy, r.z0 + 0.03), Vec3::new(r.x0 + off, yy, r.z0 + d - 0.03)) } else { (Vec3::new(r.x0 + 0.03, yy, r.z0 + off), Vec3::new(r.x0 + d - 0.03, yy, r.z0 + off)) };
                let mat = if rng.chance(0.8) { Mat::Canvas } else { Mat::Leather };
                b.mesh(mat).beam(a, c, rr, rr, Vec3::Y);
                // Ярлычок (силлибос) на конце свитка.
                if rng.chance(0.4) {
                    let tip = if along_x { a + Vec3::new(-0.01, -rr, 0.0) } else { a + Vec3::new(0.0, -rr, -0.01) };
                    b.mesh(Mat::Ochre).cuboid(tip - Vec3::new(0.012, 0.06, 0.012), tip + Vec3::new(0.012, 0.0, 0.012));
                }
            }
        }
    }
}

/// Архив (табуларий): стойки со свитками у стен, стол с раскрытым кодексом, лари с донесениями.
fn archive(b: &mut Builder, room: Rect, rng: &mut Rng) {
    scroll_rack(b, Rect::new(room.x0 + 0.3, room.z0 + 0.05, room.x1 - 0.3, room.z0 + 0.45), rng);
    scroll_rack(b, Rect::new(room.x0 + 0.05, room.z0 + 0.8, room.x0 + 0.45, room.z1 - 1.4), rng);
    scroll_rack(b, Rect::new(room.x1 - 0.45, room.z0 + 0.8, room.x1 - 0.05, room.z1 - 1.4), rng);
    let t = Rect::new(room.cx() - 0.7, room.cz() + 0.2, room.cx() + 0.7, room.cz() + 0.95);
    kit::table(b, t, 0.82);
    open_codex(b, t.center(0.82));
    b.loot(L::Library);
    for x in [room.x0 + 0.5, room.x1 - 1.5] {
        interior::chest(b, Rect::new(x, room.z1 - 0.65, x + 1.0, room.z1 - 0.05), "Ларь с донесениями");
    }
    b.loot(L::Wealthy);
    interior::lampstand(b, Vec3::new(room.cx() + 1.1, 0.0, room.cz() + 0.6), true);
}

/// Раскрытый кодекс на столе.
fn open_codex(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Leather).cuboid(p + Vec3::new(-0.32, 0.0, -0.21), p + Vec3::new(0.32, 0.02, 0.21));
    let pg = b.mesh(Mat::Canvas);
    pg.quad([p + Vec3::new(-0.3, 0.025, -0.19), p + Vec3::new(-0.01, 0.045, -0.19), p + Vec3::new(-0.01, 0.045, 0.19), p + Vec3::new(-0.3, 0.025, 0.19)], Vec3::Y);
    pg.quad([p + Vec3::new(0.01, 0.045, -0.19), p + Vec3::new(0.3, 0.025, -0.19), p + Vec3::new(0.3, 0.025, 0.19), p + Vec3::new(0.01, 0.045, 0.19)], Vec3::Y);
    // Красные строки заглавий (рубрики).
    for k in 0..3 {
        let z = -0.14 + k as f32 * 0.035;
        b.mesh(Mat::Ochre).cuboid(p + Vec3::new(-0.27, 0.046, z), p + Vec3::new(-0.06, 0.048, z + 0.012));
    }
}

/// Казна сакеллария: окованные сундуки, стол с весами и столбиками монет, ложе.
fn treasury(b: &mut Builder, room: Rect) {
    b.loot(L::Wealthy);
    for k in 0..3 {
        let x = room.x0 + 0.45 + k as f32 * 1.4;
        strongbox(b, Rect::new(x, room.z0 + 0.1, x + 1.15, room.z0 + 0.75));
    }
    let (p, _) = seat(Seat::TreasuryDesk);
    let t = Rect::new(p.x - 0.8, p.z - 1.45, p.x + 0.8, p.z - 0.55);
    kit::table(b, t, 0.8);
    // Весы-безмен с чашами и стопки монет: золотые солиды и медные фоллисы с «М».
    let top = t.center(0.8);
    let m = b.mesh(Mat::Bronze);
    m.beam(top + Vec3::new(-0.35, 0.0, 0.0), top + Vec3::new(-0.35, 0.42, 0.0), 0.012, 0.012, Vec3::X);
    m.beam(top + Vec3::new(-0.6, 0.42, 0.0), top + Vec3::new(-0.1, 0.42, 0.0), 0.008, 0.008, Vec3::Y);
    for dx in [-0.6, -0.1] {
        m.lathe(top + Vec3::new(dx, 0.18, 0.0), &[(0.0, 0.0), (0.09, 0.01), (0.1, 0.03), (0.0, 0.03)], 10);
        m.beam(top + Vec3::new(dx, 0.21, 0.0), top + Vec3::new(dx, 0.42, 0.0), 0.003, 0.003, Vec3::X);
    }
    for k in 0..5 {
        let q = top + Vec3::new(0.1 + k as f32 * 0.1, 0.0, -0.15 + (k % 2) as f32 * 0.25);
        let n = 3 + k % 4;
        let mat = if k % 2 == 0 { Mat::Gold } else { Mat::Bronze };
        b.mesh(mat).lathe(q, &[(0.0, 0.0), (0.022, 0.0), (0.022, 0.006 * n as f32), (0.0, 0.006 * n as f32)], 10);
    }
    b.mesh(Mat::Leather).lathe(top + Vec3::new(0.55, 0.0, 0.2), &[(0.0, 0.0), (0.09, 0.02), (0.1, 0.1), (0.04, 0.16), (0.06, 0.19), (0.0, 0.19)], 9);
    interior::bed(b, Rect::new(room.x1 - 0.95, room.z1 - 2.4, room.x1 - 0.05, room.z1 - 0.5), &mut Rng::new(77));
    interior::lampstand(b, Vec3::new(t.x1 + 0.35, 0.0, t.z0 + 0.1), true);
}

/// Окованный железом сундук казны с замком (обыскивается — но стража смотрит).
fn strongbox(b: &mut Builder, f: Rect) {
    let c = f.center(0.0);
    let (hx, hz) = (f.w() / 2.0, f.d() / 2.0);
    b.solid([c.x - hx, 0.0, c.z - hz], [c.x + hx, 0.62, c.z + hz], Mat::Wood);
    let m = b.mesh(Mat::Iron);
    for y in [0.08, 0.5] {
        m.cuboid(Vec3::new(c.x - hx - 0.01, y, c.z - hz - 0.01), Vec3::new(c.x + hx + 0.01, y + 0.05, c.z + hz + 0.01));
    }
    for s in [-0.6f32, 0.0, 0.6] {
        let x = c.x + s * hx;
        m.cuboid(Vec3::new(x - 0.03, 0.0, c.z - hz - 0.012), Vec3::new(x + 0.03, 0.63, c.z + hz + 0.012));
    }
    m.cuboid(Vec3::new(c.x - 0.06, 0.34, c.z + hz), Vec3::new(c.x + 0.06, 0.48, c.z + hz + 0.04));
    b.container(c + Vec3::Y * 0.8, "Казна претории");
}

/// Домовая церковь (ораторий): престол, крест, образа, лампады, мраморный пол.
fn chapel(b: &mut Builder, room: Rect) {
    b.decor([room.x0, 0.0, room.z0], [room.x1, 0.03, room.z1], Mat::Mosaic);
    let c = room.center(0.0);
    let alt = Vec3::new(room.x0 + 0.55, 0.0, c.z);
    b.solid([alt.x - 0.35, 0.0, alt.z - 0.55], [alt.x + 0.35, 1.0, alt.z + 0.55], Mat::Marble);
    b.mesh(Mat::Banner).cuboid(Vec3::new(alt.x - 0.37, 0.55, alt.z - 0.57), Vec3::new(alt.x + 0.37, 1.02, alt.z + 0.57));
    // Напрестольный крест и Евангелие в золотом окладе.
    let g = b.mesh(Mat::Gold);
    g.cuboid(Vec3::new(alt.x - 0.02, 1.0, alt.z - 0.02), Vec3::new(alt.x + 0.02, 1.42, alt.z + 0.02));
    g.cuboid(Vec3::new(alt.x - 0.02, 1.27, alt.z - 0.13), Vec3::new(alt.x + 0.02, 1.31, alt.z + 0.13));
    g.cuboid(Vec3::new(alt.x - 0.12, 1.0, alt.z + 0.22), Vec3::new(alt.x + 0.12, 1.05, alt.z + 0.4));
    // Образа на стене за престолом и по бокам.
    for (dz, h) in [(0.0, 2.2), (-1.35, 1.7), (1.35, 1.7)] {
        icon_panel(b, Vec3::new(room.x0 + 0.02, h, c.z + dz), Vec3::X, if dz == 0.0 { 0.8 } else { 0.55 });
    }
    // Лампады на цепях перед образами.
    for dz in [-0.6, 0.6] {
        let p = Vec3::new(room.x0 + 0.9, 2.1, c.z + dz);
        b.mesh(Mat::Bronze).beam(p, Vec3::new(p.x, PERI_H - 0.1, p.z), 0.006, 0.006, Vec3::X);
        b.mesh(Mat::Glass).lathe(p - Vec3::Y * 0.12, &[(0.0, 0.0), (0.06, 0.03), (0.07, 0.1), (0.05, 0.12)], 8);
    }
    b.lamp(Vec3::new(room.x0 + 1.0, 2.0, c.z), Color::srgb(1.0, 0.7, 0.4), 4_000.0, 6.0);
    // Скамья у стены для молящихся.
    b.solid([room.x1 - 0.5, 0.0, room.z0 + 0.2], [room.x1 - 0.1, 0.45, room.z0 + 1.8], Mat::Wood);
    b.solid([room.x1 - 0.5, 0.0, room.z1 - 1.8], [room.x1 - 0.1, 0.45, room.z1 - 0.2], Mat::Wood);
    b.loot(L::Church);
    interior::chest(b, Rect::new(room.x0 + 0.1, room.z0 + 0.1, room.x0 + 0.7, room.z0 + 1.0), "Ризничий ларь");
    b.loot(L::Wealthy);
}

/// Образ на доске в золочёной раме: лик, нимб, красный мафорий или синий гиматий.
fn icon_panel(b: &mut Builder, p: Vec3, out: Vec3, h: f32) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let w = h * 0.72;
    let g = b.mesh(Mat::Gold);
    g.cuboid(p - side * (w / 2.0) - Vec3::Y * (h / 2.0), p + side * (w / 2.0) + Vec3::Y * (h / 2.0) + out * 0.04);
    let f = p + out * 0.045;
    b.mesh(Mat::Wood).cuboid(f - side * (w / 2.0 - 0.05) - Vec3::Y * (h / 2.0 - 0.05), f + side * (w / 2.0 - 0.05) + Vec3::Y * (h / 2.0 - 0.05) + out * 0.004);
    let face_c = f + Vec3::Y * (h * 0.18) + out * 0.006;
    // Нимб.
    b.mesh(Mat::Gold).cuboid(face_c - side * (w * 0.24) - Vec3::Y * (w * 0.24), face_c + side * (w * 0.24) + Vec3::Y * (w * 0.24) + out * 0.003);
    b.mesh(Mat::Ochre).cuboid(face_c - side * (w * 0.12) - Vec3::Y * (w * 0.15) + out * 0.004, face_c + side * (w * 0.12) + Vec3::Y * (w * 0.15) + out * 0.007);
    let robe = if h > 0.7 { Mat::Banner } else { Mat::Cloth };
    let rb = f - Vec3::Y * (h * 0.18) + out * 0.006;
    b.mesh(robe).cuboid(rb - side * (w * 0.33) - Vec3::Y * (h * 0.3), rb + side * (w * 0.33) + Vec3::Y * (h * 0.12) + out * 0.006);
}

/// Келья пресвитера: ложе, аналой с книгой, сундук, образ.
fn presbyter_cell(b: &mut Builder, room: Rect, rng: &mut Rng) {
    interior::bed(b, Rect::new(room.x0 + 0.1, room.z1 - 2.0, room.x0 + 1.0, room.z1 - 0.1), rng);
    let (p, _) = seat(Seat::Lectern);
    let l = Vec3::new(p.x - 0.85, 0.0, p.z);
    let m = b.mesh(Mat::Wood);
    m.lathe(l, &[(0.18, 0.0), (0.18, 0.05), (0.05, 0.08), (0.04, 1.05)], 8);
    m.cuboid(l + Vec3::new(-0.2, 1.05, -0.25), l + Vec3::new(0.2, 1.1, 0.25));
    open_codex(b, l + Vec3::Y * 1.11);
    kit::hidden(b, l - Vec3::new(0.22, 0.0, 0.27), l + Vec3::new(0.22, 1.1, 0.27));
    icon_panel(b, Vec3::new(room.x0 + 0.02, 1.8, room.z0 + 1.2), Vec3::X, 0.55);
    b.loot(L::Church);
    interior::chest(b, Rect::new(room.x1 - 0.65, room.z0 + 0.2, room.x1 - 0.05, room.z0 + 1.2), "Сундук пресвитера");
    b.loot(L::Wealthy);
    interior::lampstand(b, Vec3::new(room.x0 + 0.4, 0.0, room.z0 + 0.35), true);
}

/// Канцелярия трибуна: стойка с оружием, стол с восковыми табличками и картой, сундук.
fn tribune_office(b: &mut Builder, room: Rect, rng: &mut Rng) {
    let (p, _) = seat(Seat::TribuneTable);
    let t = Rect::new(p.x - 0.8, room.z0 + 0.1, p.x + 0.8, room.z0 + 0.85);
    kit::table(b, t, 0.8);
    // Карта Таврики на пергаменте и таблички.
    b.mesh(Mat::Canvas).cuboid(Vec3::new(t.x0 + 0.15, 0.8, t.z0 + 0.1), Vec3::new(t.x0 + 0.95, 0.81, t.z1 - 0.1));
    b.mesh(Mat::Ochre).cuboid(Vec3::new(t.x0 + 0.3, 0.81, t.z0 + 0.3), Vec3::new(t.x0 + 0.7, 0.812, t.z0 + 0.34));
    b.mesh(Mat::Wood).cuboid(Vec3::new(t.x1 - 0.5, 0.8, t.z0 + 0.2), Vec3::new(t.x1 - 0.2, 0.82, t.z0 + 0.42));
    interior::weapon_rack(b, Rect::new(room.x1 - 0.45, room.z0 + 0.9, room.x1 - 0.05, room.z1 - 0.3), Side::E);
    b.loot(L::Barracks);
    interior::chest(b, Rect::new(room.x0 + 0.1, room.z1 - 0.65, room.x0 + 1.1, room.z1 - 0.05), "Ларь с донесениями");
    b.loot(L::Wealthy);
    interior::lampstand(b, Vec3::new(t.x0 - 0.35, 0.0, t.z1 + 0.2), true);
    let _ = rng;
}

/// Вход с караульной: скамьи просителей вдоль стен, проход между дверями свободен.
fn vestibule(b: &mut Builder, room: Rect) {
    b.decor([room.x0, 0.0, room.z0], [room.x1, 0.03, room.z1], Mat::Marble);
    for z in [room.z0 + 0.05, room.z1 - 0.5] {
        b.solid([room.x0 + 0.6, 0.0, z], [room.x1 - 0.6, 0.45, z + 0.45], Mat::Marble);
    }
    interior::lampstand(b, Vec3::new(room.x0 + 0.35, 0.0, room.z0 + 0.35), true);
}

/// Парадная кольчуга и шлем на крестовине (у комита — с позолотой).
fn armour_stand(b: &mut Builder, p: Vec3, gilded: bool) {
    b.mesh(Mat::Wood).cuboid(p - Vec3::new(0.04, 0.0, 0.04), p + Vec3::new(0.04, 1.7, 0.04));
    b.mesh(Mat::Wood).cuboid(p + Vec3::new(-0.32, 1.38, -0.03), p + Vec3::new(0.32, 1.44, 0.03));
    b.mesh(Mat::Iron).lathe(p + Vec3::Y * 0.7, &[(0.0, 0.0), (0.27, 0.0), (0.3, 0.35), (0.26, 0.6), (0.32, 0.72), (0.1, 0.78), (0.0, 0.78)], 10);
    let helm = if gilded { Mat::Gold } else { Mat::Iron };
    b.mesh(helm).lathe(p + Vec3::Y * 1.55, &[(0.0, 0.0), (0.14, 0.0), (0.145, 0.12), (0.1, 0.24), (0.0, 0.27)], 10);
    b.mesh(Mat::Banner).beam(p + Vec3::new(0.0, 1.82, -0.1), p + Vec3::new(0.0, 1.84, 0.12), 0.015, 0.05, Vec3::Y);
    kit::hidden(b, p - Vec3::new(0.34, 0.0, 0.3), p + Vec3::new(0.34, 1.8, 0.3));
}

/// Переулок между двором и залом: мостовая, кипарисы у стены.
fn lane(b: &mut Builder) {
    b.decor([LANE.x0, 0.0, LANE.z0], [LANE.x1, 0.025, LANE.z1], Mat::Paving);
    for x in [189.0, 192.5] {
        kit::cypress(b, Vec3::new(x, 0.0, LANE.cz()), 4.5);
    }
}

/// Зал приёмов (аула): колоннады, апсида с золотой конхой, трибунал с троном, образ августа.
fn hall(b: &mut Builder, rng: &mut Rng) {
    let h = HALL;
    let t = HALL_T;
    let wall = Mat::OpusMixtum;
    let ac = apse_center();
    // Полы: мраморные плиты, по оси — мозаичный ковёр с каймой.
    b.decor([h.x0, 0.0, h.z0], [h.x1, 0.025, h.z1], Mat::Marble);
    b.decor([h.x0 + 1.6, 0.025, COL_Z[0] + 0.9], [h.x1 - 1.2, 0.04, COL_Z[1] - 0.9], Mat::Mosaic);
    let k = b.mesh(Mat::Ochre);
    for z in [COL_Z[0] + 0.75, COL_Z[1] - 0.9] {
        k.cuboid(Vec3::new(h.x0 + 1.45, 0.025, z), Vec3::new(h.x1 - 1.05, 0.042, z + 0.15));
    }
    // Стены: внизу — окна на переулок (с юга за стеной покои), двери; вверху — окна второго
    // света (с юга — над кровлей покоев), над главными дверями — три окна.
    let side_door_x = PERI.cx();
    let tricl_door_x = triclinium_table().x;
    let xs = [199.5, 203.0, 206.5, 210.0, 213.4];
    let low: Vec<f32> = xs.iter().copied().filter(|x| (x - side_door_x).abs() > 1.6).collect();
    let mut north: Vec<Op> = low.iter().map(|&s| Op::Window { s, w: 1.0, sill: 2.4, top: 4.3 }).collect();
    north.push(Op::Door { s: side_door_x, w: 1.4, h: 2.7 });
    let upper: Vec<Op> = xs.iter().map(|&s| Op::Window { s, w: 1.0, sill: 1.3, top: 3.1 }).collect();
    kit::wall(b, kit::fx(), h.x0, h.x1 + t, h.z0 - t, h.z0, 0.0, 5.4, &north, wall, Mat::Marble);
    kit::wall(b, kit::fx(), h.x0, h.x1 + t, h.z0 - t, h.z0, 5.4, HALL_H, &upper, wall, Mat::Marble);
    kit::wall(b, kit::fx(), h.x0, h.x1 + t, h.z1, h.z1 + t, 0.0, 5.4, &[Op::Door { s: tricl_door_x, w: 1.4, h: 2.7 }], wall, Mat::Marble);
    kit::wall(b, kit::fx(), h.x0, h.x1 + t, h.z1, h.z1 + t, 5.4, HALL_H, &upper, wall, Mat::Marble);
    kit::wall(b, kit::fz(), h.z0, h.z1, h.x1, h.x1 + t, 0.0, 5.4, &[Op::Door { s: h.cz(), w: 2.6, h: 4.2 }], wall, Mat::Marble);
    let east_up = [h.cz() - 3.6, h.cz(), h.cz() + 3.6];
    let upper_e: Vec<Op> = east_up.iter().map(|&s| Op::Window { s, w: 1.1, sill: 0.6, top: 2.6 }).collect();
    kit::wall(b, kit::fz(), h.z0, h.z1, h.x1, h.x1 + t, 5.4, HALL_H, &upper_e, wall, Mat::Marble);
    // Изнутри стены оштукатурены: цоколь под красный мрамор, белёное поле, расписной фриз.
    let mut holes_n: Vec<(f32, f32, f32, f32)> = low.iter().map(|&x| (x - 0.66, x + 0.66, 2.24, 4.46)).collect();
    holes_n.push((side_door_x - 0.88, side_door_x + 0.88, 0.0, 2.95));
    let ups: Vec<(f32, f32, f32, f32)> = xs.iter().map(|&x| (x - 0.66, x + 0.66, 6.54, 8.66)).collect();
    holes_n.extend(ups.iter().copied());
    let mut holes_s = ups.clone();
    holes_s.push((tricl_door_x - 0.88, tricl_door_x + 0.88, 0.0, 2.95));
    let mut holes_e: Vec<(f32, f32, f32, f32)> = east_up.iter().map(|&z| (z - 0.72, z + 0.72, 5.84, 8.16)).collect();
    holes_e.push((h.cz() - 1.48, h.cz() + 1.48, 0.0, 4.45));
    skin(b, true, h.x0 + 0.4, h.x1, h.z0, 1.0, &holes_n);
    skin(b, true, h.x0 + 0.4, h.x1, h.z1, -1.0, &holes_s);
    skin(b, false, h.z0, h.z1, h.x1, -1.0, &holes_e);
    // Колоннады: проконнесский мрамор, архитрав с карнизом — две колоннады-экрана вдоль зала.
    for z in COL_Z {
        for k in 0..5 {
            let x = 199.5 + k as f32 * 3.5;
            column(b, Vec3::new(x, 0.0, z), COL_H, 0.26, Mat::Marble);
        }
        let m = b.mesh(Mat::Marble);
        m.cuboid(Vec3::new(h.x0 + 1.5, COL_H, z - 0.3), Vec3::new(h.x1, COL_H + 0.45, z + 0.3));
        m.cuboid(Vec3::new(h.x0 + 1.5, COL_H + 0.45, z - 0.38), Vec3::new(h.x1, COL_H + 0.55, z + 0.38));
        // Фриз архитрава — полоса красного порфира (охрой) и золотой поясок.
        b.mesh(Mat::Ochre).cuboid(Vec3::new(h.x0 + 1.5, COL_H + 0.15, z - 0.31), Vec3::new(h.x1, COL_H + 0.3, z + 0.31));
        b.mesh(Mat::Gold).cuboid(Vec3::new(h.x0 + 1.5, COL_H + 0.42, z - 0.32), Vec3::new(h.x1, COL_H + 0.45, z + 0.32));
    }
    // Над средним нефом — кессонный потолок, над залом — двускатная черепичная крыша.
    let m = b.mesh(Mat::Wood);
    m.cuboid(Vec3::new(h.x0, HALL_H - 0.1, COL_Z[0]), Vec3::new(h.x1, HALL_H, COL_Z[1]));
    for k in 0..=10 {
        let x = h.x0 + h.w() * k as f32 / 10.0;
        m.cuboid(Vec3::new(x - 0.1, HALL_H - 0.4, COL_Z[0]), Vec3::new(x + 0.1, HALL_H - 0.1, COL_Z[1]));
    }
    for z in [COL_Z[0] + 2.6, h.cz(), COL_Z[1] - 2.6] {
        m.cuboid(Vec3::new(h.x0, HALL_H - 0.34, z - 0.08), Vec3::new(h.x1, HALL_H - 0.1, z + 0.08));
    }
    kit::gable_roof(b, Rect::new(h.x0, h.z0 - t, h.x1 + t, h.z1 + t), HALL_H, 2.8, true, 0.5, wall);
    // Апсида: стена, изнутри — штукатурка, над ней — конха, выложенная золотой смальтой.
    let ar = APSE_R;
    let ah = 4.5;
    b.mesh(wall).lathe_arc(ac, &[(ar + t, 0.0), (ar + t, ah)], FRAC_PI_2, PI + FRAC_PI_2, 16, false);
    b.mesh(Mat::Plaster).lathe_arc(ac, &[(ar, 0.0), (ar, ah)], FRAC_PI_2, PI + FRAC_PI_2, 16, true);
    b.mesh(Mat::Ochre).lathe_arc(ac, &[(ar - 0.005, 0.0), (ar - 0.005, 1.1)], FRAC_PI_2, PI + FRAC_PI_2, 16, true);
    b.mesh(wall).lathe_arc(ac + Vec3::Y * ah, &[(ar + t, 0.0), (ar, 0.0)], FRAC_PI_2, PI + FRAC_PI_2, 16, false);
    b.mesh(Mat::Gold).dome(ac + Vec3::Y * ah, ar, FRAC_PI_2, PI + FRAC_PI_2, 14, 0.0);
    b.roof(Mat::Tile, |m| m.dome(ac + Vec3::Y * ah, ar + t + 0.1, FRAC_PI_2, PI + FRAC_PI_2, 14, 0.05));
    // Карниз конхи — мраморная полоса.
    b.mesh(Mat::Marble).lathe_arc(ac + Vec3::Y * (ah - 0.25), &[(ar - 0.12, 0.0), (ar - 0.12, 0.25)], FRAC_PI_2, PI + FRAC_PI_2, 16, true);
    for k in 0..10 {
        let a = FRAC_PI_2 + PI * (k as f32 + 0.5) / 10.0;
        let q = ac + Vec3::new(a.cos() * (ar + t / 2.0), 0.0, a.sin() * (ar + t / 2.0));
        b.invisible.push(Aabb::new(q - Vec3::new(0.65, 0.0, 0.65), q + Vec3::new(0.65, ah, 0.65)));
    }
    // Стена с аркой апсиды (триумфальная арка).
    kit::wall(b, kit::fz(), h.z0 - t, h.z1 + t, h.x0 - 0.4, h.x0 + 0.4, 0.0, HALL_H, &[Op::Arch { s: h.cz(), w: 2.0 * ar - 0.4, spring: ah }], wall, Mat::Marble);
    // Трибунал: мраморное возвышение и две ступени.
    let d = DAIS;
    b.solid([d.x0, 0.0, d.z0], [d.x1, DAIS_H, d.z1], Mat::Marble);
    b.solid([d.x1, 0.0, d.z0 + 0.4], [d.x1 + 0.6, 0.25, d.z1 - 0.4], Mat::Marble);
    b.solid([d.x1 + 0.6, 0.0, d.z0 + 0.8], [d.x1 + 1.2, 0.12, d.z1 - 0.8], Mat::Marble);
    b.mesh(Mat::Banner).cuboid(Vec3::new(d.x0 + 0.4, DAIS_H, ac.z - 1.6), Vec3::new(d.x1 - 0.05, DAIS_H + 0.02, ac.z + 1.6));
    throne(b);
    emperor_portrait(b, Vec3::new(ac.x - ar + 0.02, 3.4, ac.z));
    for dz in [-2.4f32, 2.4] {
        labarum(b, Vec3::new(d.x0 + 1.3, DAIS_H, ac.z + dz));
        candelabrum(b, Vec3::new(d.x1 - 0.5, DAIS_H, ac.z + dz * 0.73));
    }
    // Завесы (вела) по сторонам арки, подхваченные шнурами.
    for z in [h.z0 + 0.15, h.z1 - 0.15] {
        let m = b.mesh(Mat::Banner);
        m.cuboid(Vec3::new(h.x0 + 0.45, 0.6, z - 0.04), Vec3::new(h.x0 + 1.25, 6.8, z + 0.04));
        b.mesh(Mat::Gold).cuboid(Vec3::new(h.x0 + 0.42, 2.2, z - 0.06), Vec3::new(h.x0 + 1.28, 2.32, z + 0.06));
    }
    // Стол нотария с кодексом и чернильницей, мраморные скамьи (субселлии) вдоль стен.
    let (np, _) = seat(Seat::NotaryDesk);
    let nt = Rect::new(np.x - 0.6, h.z0 + 0.35, np.x + 0.6, np.z - 0.42);
    kit::table(b, nt, 0.82);
    open_codex(b, nt.center(0.82));
    inkwell(b, Vec3::new(nt.x1 - 0.15, 0.82, nt.cz()));
    for (z0, z1) in [(h.z0 + 0.05, h.z0 + 0.55), (h.z1 - 0.55, h.z1 - 0.05)] {
        for (x0, x1) in [(201.0, 205.2), (207.8, 212.0)] {
            if z0 < h.cz() && x0 < 202.0 {
                continue; // у боковой двери скамьи нет
            }
            b.solid([x0, 0.0, z0], [x1, 0.45, z1], Mat::Marble);
        }
    }
    // Свет: два поликандила над средним нефом, светильники у трибунала, лампа в апсиде.
    for x in [202.5, 209.5] {
        polycandelon(b, Vec3::new(x, 4.6, h.cz()), HALL_H - 0.4);
    }
    b.lamp(ac + Vec3::new(-2.8, 3.6, 0.0), Color::srgb(1.0, 0.74, 0.45), 14_000.0, 9.0);
    let _ = rng;
}

/// Отделка внутренней грани стены (тонкие плиты на 1 см от неё) с вырезами под проёмы.
/// `along_x` — стена вдоль X на линии `d` (иначе вдоль Z), `inward` — знак нормали в комнату,
/// `holes` — (s0, s1, y0, y1) проёмов. Полосы: цоколь, белёное поле, фриз, поле до потолка.
fn skin(b: &mut Builder, along_x: bool, s0: f32, s1: f32, d: f32, inward: f32, holes: &[(f32, f32, f32, f32)]) {
    let bands = [(0.03, 1.1, Mat::Ochre), (1.1, 5.0, Mat::Plaster), (5.0, 5.35, Mat::Ochre), (5.35, HALL_H - 0.1, Mat::Plaster)];
    skin_bands(b, along_x, s0, s1, d, inward, holes, &bands);
}

/// Отделка стены полосами (снизу вверх: до какой высоты какой материал), с проёмами
/// `holes` = (от, до, низ, верх) вдоль стены.
fn skin_bands(b: &mut Builder, along_x: bool, s0: f32, s1: f32, d: f32, inward: f32, holes: &[(f32, f32, f32, f32)], bands: &[(f32, f32, Mat)]) {
    let (dn, df) = if inward > 0.0 { (d + 0.004, d + 0.014) } else { (d - 0.014, d - 0.004) };
    let quad = |b: &mut Builder, a0: f32, a1: f32, y0: f32, y1: f32, mat: Mat| {
        if a1 - a0 < 0.01 || y1 - y0 < 0.01 {
            return;
        }
        let (min, max) = if along_x { (Vec3::new(a0, y0, dn), Vec3::new(a1, y1, df)) } else { (Vec3::new(dn, y0, a0), Vec3::new(df, y1, a1)) };
        b.mesh(mat).cuboid(min, max);
    };
    for &(y0, y1, mat) in bands {
        let mut cuts: Vec<(f32, f32, f32, f32)> = holes.iter().copied().filter(|h| h.2 < y1 && h.3 > y0).collect();
        cuts.sort_by(|a, c| a.0.total_cmp(&c.0));
        let mut s = s0;
        for (h0, h1, hy0, hy1) in cuts {
            quad(b, s, h0.max(s), y0, y1, mat);
            // Над и под проёмом в пределах полосы.
            quad(b, h0.max(s), h1, y0, hy0.max(y0).min(y1), mat);
            quad(b, h0.max(s), h1, hy1.min(y1).max(y0), y1, mat);
            s = s.max(h1);
        }
        quad(b, s, s1, y0, y1, mat);
    }
}

/// Трон: мраморное основание, кресло с высокой полукруглой спинкой, позолота, багряная подушка,
/// подножие. Сиденье без коллизии — на нём сидит комит (и может сесть игрок).
fn throne(b: &mut Builder) {
    let (p, _) = seat(Seat::Throne);
    let base = Vec3::new(p.x - 0.1, DAIS_H, p.z);
    // Подиум под троном.
    b.mesh(Mat::Marble).cuboid(base + Vec3::new(-0.85, 0.0, -0.85), base + Vec3::new(0.65, 0.08, 0.85));
    let m = b.mesh(Mat::Wood);
    // Сиденье, ножки-львиные лапы (стилизованные) и подлокотники.
    m.cuboid(base + Vec3::new(-0.45, 0.4, -0.42), base + Vec3::new(0.38, 0.5, 0.42));
    for (dx, dz) in [(-0.4, -0.37), (0.33, -0.37), (-0.4, 0.37), (0.33, 0.37)] {
        m.cuboid(base + Vec3::new(dx - 0.05, 0.08, dz - 0.05), base + Vec3::new(dx + 0.05, 0.4, dz + 0.05));
    }
    for dz in [-0.44f32, 0.44] {
        m.cuboid(base + Vec3::new(-0.45, 0.5, dz - 0.05), base + Vec3::new(0.3, 0.56, dz + 0.05));
        m.cuboid(base + Vec3::new(0.24, 0.5, dz - 0.05), base + Vec3::new(0.32, 0.82, dz + 0.05));
        m.cuboid(base + Vec3::new(-0.45, 0.78, dz - 0.06), base + Vec3::new(0.34, 0.86, dz + 0.06));
    }
    let g = b.mesh(Mat::Gold);
    for dz in [-0.44f32, 0.44] {
        g.lathe(base + Vec3::new(0.3, 0.86, dz), &[(0.0, 0.0), (0.06, 0.0), (0.06, 0.03), (0.0, 0.09)], 8);
        g.cuboid(base + Vec3::new(-0.46, 0.49, dz - 0.06), base + Vec3::new(0.39, 0.51, dz + 0.06));
    }
    // Высокая спинка с полукруглым верхом: дерево, золочёный край и багряный бархат.
    let bx = base.x - 0.5;
    let back = b.mesh(Mat::Wood);
    back.cuboid(Vec3::new(bx - 0.06, base.y + 0.5, p.z - 0.45), Vec3::new(bx + 0.06, base.y + 1.75, p.z + 0.45));
    let top_c = Vec3::new(bx, base.y + 1.75, p.z);
    let arc: Vec<Vec3> = (0..=12).map(|k| {
        let a = PI * k as f32 / 12.0;
        top_c + Vec3::new(0.0, a.sin() * 0.45, a.cos() * 0.45)
    }).collect();
    b.mesh(Mat::Gold).beam_path(&arc, 0.05, 0.035, Vec3::X);
    let fan = b.mesh(Mat::Wood);
    for w in arc.windows(2) {
        fan.tri(top_c, w[0], w[1], Vec3::X);
        fan.tri(top_c, w[1], w[0], Vec3::NEG_X);
    }
    b.mesh(Mat::Banner).cuboid(Vec3::new(bx + 0.06, base.y + 0.58, p.z - 0.36), Vec3::new(bx + 0.08, base.y + 1.8, p.z + 0.36));
    b.mesh(Mat::Gold).cuboid(Vec3::new(bx + 0.07, base.y + 0.55, p.z - 0.4), Vec3::new(bx + 0.09, base.y + 0.6, p.z + 0.4));
    // Золотой крест-навершие и бусины по краю спинки.
    let g = b.mesh(Mat::Gold);
    g.cuboid(top_c + Vec3::new(-0.03, 0.42, -0.025), top_c + Vec3::new(0.03, 0.7, 0.025));
    g.cuboid(top_c + Vec3::new(-0.03, 0.56, -0.1), top_c + Vec3::new(0.03, 0.61, 0.1));
    for k in 0..7 {
        let z = p.z - 0.42 + k as f32 * 0.14;
        g.lathe(Vec3::new(bx + 0.07, base.y + 1.75, z), &[(0.0, -0.02), (0.025, 0.0), (0.0, 0.025)], 6);
    }
    // Подушка и подножие.
    b.mesh(Mat::Banner).cuboid(base + Vec3::new(-0.42, 0.5, -0.38), base + Vec3::new(0.36, 0.58, 0.38));
    b.mesh(Mat::Gold).cuboid(base + Vec3::new(-0.43, 0.555, -0.39), base + Vec3::new(0.37, 0.565, 0.39));
    b.mesh(Mat::Marble).cuboid(base + Vec3::new(0.55, 0.0, -0.4), base + Vec3::new(0.95, 0.14, 0.4));
    b.mesh(Mat::Rug).cuboid(base + Vec3::new(0.57, 0.14, -0.36), base + Vec3::new(0.93, 0.16, 0.36));
    // Коллизия — только спинка и подлокотники: на сиденье садятся.
    b.invisible.push(Aabb::new(Vec3::new(bx - 0.08, base.y, p.z - 0.5), Vec3::new(bx + 0.1, base.y + 2.2, p.z + 0.5)));
}

/// Лаврата — образ августа в золочёной раме: император в диадеме и пурпурной хламиде с
/// таблионом, над ним — хризма. Перед образом горят свечи: так в претории являли власть.
fn emperor_portrait(b: &mut Builder, p: Vec3) {
    let (w, h) = (1.5, 2.0);
    let g = b.mesh(Mat::Gold);
    g.cuboid(p + Vec3::new(0.0, -h / 2.0, -w / 2.0), p + Vec3::new(0.06, h / 2.0, w / 2.0));
    let f = p + Vec3::X * 0.065;
    b.mesh(Mat::Cloth).cuboid(f + Vec3::new(0.0, -h / 2.0 + 0.08, -w / 2.0 + 0.08), f + Vec3::new(0.006, h / 2.0 - 0.08, w / 2.0 - 0.08));
    // Фигура: пурпурная хламида, таблион, лик, диадема с жемчугом, нимб.
    let body = f + Vec3::new(0.008, -0.35, 0.0);
    b.mesh(Mat::Banner).cuboid(body + Vec3::new(0.0, -0.5, -0.42), body + Vec3::new(0.008, 0.45, 0.42));
    b.mesh(Mat::Gold).cuboid(body + Vec3::new(0.009, -0.05, 0.08), body + Vec3::new(0.013, 0.25, 0.34));
    b.mesh(Mat::Gold).lathe(body + Vec3::new(0.012, 0.38, -0.3), &[(0.0, -0.04), (0.05, 0.0), (0.0, 0.04)], 8);
    let head = f + Vec3::new(0.008, 0.33, 0.0);
    b.mesh(Mat::Gold).cuboid(head + Vec3::new(0.0, -0.27, -0.27), head + Vec3::new(0.004, 0.27, 0.27));
    b.mesh(Mat::Ochre).cuboid(head + Vec3::new(0.005, -0.17, -0.12), head + Vec3::new(0.012, 0.15, 0.12));
    b.mesh(Mat::PetalWhite).cuboid(head + Vec3::new(0.013, 0.08, -0.13), head + Vec3::new(0.02, 0.12, 0.13));
    // Хризма над головой.
    let c = f + Vec3::new(0.01, 0.82, 0.0);
    let g = b.mesh(Mat::Gold);
    g.beam(c + Vec3::new(0.0, -0.1, 0.0), c + Vec3::new(0.0, 0.1, 0.0), 0.006, 0.012, Vec3::X);
    g.beam(c + Vec3::new(0.0, -0.07, -0.07), c + Vec3::new(0.0, 0.07, 0.07), 0.006, 0.012, Vec3::X);
    g.beam(c + Vec3::new(0.0, -0.07, 0.07), c + Vec3::new(0.0, 0.07, -0.07), 0.006, 0.012, Vec3::X);
    g.lathe(c + Vec3::new(0.0, 0.06, 0.03), &[(0.0, -0.03), (0.03, 0.0), (0.0, 0.03)], 8);
}

/// Лабарум — военное знамя с хризмой: древко, перекладина, пурпурное полотнище с золотом.
fn labarum(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Wood).lathe(p, &[(0.035, 0.0), (0.03, 3.4)], 8);
    b.mesh(Mat::Marble).lathe(p, &[(0.22, 0.0), (0.2, 0.12), (0.1, 0.2), (0.06, 0.3)], 10);
    let top = p + Vec3::Y * 3.0;
    b.mesh(Mat::Gold).cuboid(top + Vec3::new(-0.02, -0.02, -0.45), top + Vec3::new(0.02, 0.02, 0.45));
    b.mesh(Mat::Banner).cuboid(top + Vec3::new(-0.01, -1.05, -0.42), top + Vec3::new(0.01, 0.0, 0.42));
    for k in 0..3 {
        let c = top + Vec3::new(0.012, -0.25 - k as f32 * 0.3, 0.0);
        b.mesh(Mat::Gold).lathe(c, &[(0.0, -0.06), (0.06, 0.0), (0.0, 0.06)], 8);
    }
    // Венок с хризмой на верху древка.
    let c = p + Vec3::Y * 3.55;
    let ring: Vec<Vec3> = (0..=14).map(|k| {
        let a = k as f32 / 14.0 * std::f32::consts::TAU;
        c + Vec3::new(0.0, a.sin() * 0.16, a.cos() * 0.16)
    }).collect();
    let g = b.mesh(Mat::Gold);
    g.beam_path(&ring, 0.02, 0.02, Vec3::X);
    g.beam(c - Vec3::Y * 0.14, c + Vec3::Y * 0.14, 0.012, 0.012, Vec3::X);
    g.beam(c + Vec3::new(0.0, -0.1, -0.1), c + Vec3::new(0.0, 0.1, 0.1), 0.012, 0.012, Vec3::X);
    g.beam(c + Vec3::new(0.0, -0.1, 0.1), c + Vec3::new(0.0, 0.1, -0.1), 0.012, 0.012, Vec3::X);
    b.mesh(Mat::Wood).lathe(p + Vec3::Y * 3.4, &[(0.03, 0.0), (0.03, 0.0), (0.012, 0.0)], 6);
    b.mesh(Mat::Gold).cuboid(p + Vec3::new(-0.015, 3.38, -0.015), p + Vec3::new(0.015, 3.42, 0.015));
    kit::hidden(b, p - Vec3::new(0.2, 0.0, 0.2), p + Vec3::new(0.2, 2.0, 0.2));
}

/// Бронзовый канделябр в рост человека с чашей-светильником.
fn candelabrum(b: &mut Builder, p: Vec3) {
    let m = b.mesh(Mat::Bronze);
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        m.beam(p + Vec3::new(a.cos() * 0.25, 0.0, a.sin() * 0.25), p + Vec3::Y * 0.25, 0.018, 0.018, Vec3::Y);
    }
    m.lathe(p + Vec3::Y * 0.2, &[(0.03, 0.0), (0.025, 0.8), (0.05, 0.85), (0.025, 0.9), (0.022, 1.5), (0.14, 1.58), (0.16, 1.64), (0.0, 1.62)], 10);
    b.mesh(Mat::Coals).lathe(p + Vec3::Y * 1.82, &[(0.0, 0.0), (0.05, 0.02), (0.0, 0.1)], 6);
    kit::hidden(b, p - Vec3::new(0.25, 0.0, 0.25), p + Vec3::new(0.25, 1.9, 0.25));
}

/// Пропилей: четыре мраморные колонны, фронтон, ступени к плацу, знамёна и светильники.
fn propylon(b: &mut Builder) {
    let pr = PROPYLON;
    let colx = pr.x1 - 0.6;
    b.solid([pr.x0, 0.0, pr.z0], [pr.x1, PROPYLON_H, pr.z1], Mat::Marble);
    for z in [137.0, 139.7, 143.3, 146.0] {
        column(b, Vec3::new(colx, 0.16, z), 5.2, 0.3, Mat::Marble);
    }
    // Пилястры-анты у стены и архитрав.
    let m = b.mesh(Mat::Marble);
    for z in [pr.z0 + 0.25, pr.z1 - 0.25] {
        m.cuboid(Vec3::new(pr.x0, 0.16, z - 0.3), Vec3::new(pr.x0 + 0.35, 5.36, z + 0.3));
    }
    m.cuboid(Vec3::new(pr.x0, 5.36, pr.z0), Vec3::new(pr.x1, 5.86, pr.z1));
    m.cuboid(Vec3::new(pr.x1 - 0.05, 5.86, pr.z0 - 0.1), Vec3::new(pr.x1 + 0.12, 6.0, pr.z1 + 0.1));
    // Фрона: мраморный фронтон с хризмой в венке.
    kit::gable_roof(b, Rect::new(pr.x0, pr.z0, pr.x1, pr.z1), 6.0, 1.7, true, 0.25, Mat::Marble);
    let c = Vec3::new(pr.x1 + 0.03, 6.65, pr.cz());
    let ring: Vec<Vec3> = (0..=16).map(|k| {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        c + Vec3::new(0.0, a.sin() * 0.42, a.cos() * 0.42)
    }).collect();
    let g = b.mesh(Mat::Gold);
    g.beam_path(&ring, 0.04, 0.03, Vec3::X);
    g.beam(c - Vec3::Y * 0.36, c + Vec3::Y * 0.36, 0.03, 0.025, Vec3::X);
    g.beam(c + Vec3::new(0.0, -0.26, -0.26), c + Vec3::new(0.0, 0.26, 0.26), 0.03, 0.025, Vec3::X);
    g.beam(c + Vec3::new(0.0, -0.26, 0.26), c + Vec3::new(0.0, 0.26, -0.26), 0.03, 0.025, Vec3::X);
    g.lathe(c + Vec3::new(0.0, 0.2, 0.09), &[(0.0, -0.09), (0.09, 0.0), (0.0, 0.09)], 10);
    // Ступени к плацу.
    b.solid([pr.x1, 0.0, pr.z0 + 0.6], [pr.x1 + 0.45, 0.08, pr.z1 - 0.6], Mat::Marble);
    // Знамёна на стене по сторонам дверей и факелы.
    for dz in [-2.2f32, 2.2] {
        let z = HALL.cz() + dz;
        let x = HALL.x1 + HALL_T + 0.03;
        b.mesh(Mat::Banner).cuboid(Vec3::new(x, 1.4, z - 0.45), Vec3::new(x + 0.03, 4.4, z + 0.45));
        b.mesh(Mat::Gold).cuboid(Vec3::new(x + 0.03, 3.3, z - 0.18), Vec3::new(x + 0.04, 3.66, z + 0.18));
        b.mesh(Mat::Gold).cuboid(Vec3::new(x - 0.02, 4.4, z - 0.5), Vec3::new(x + 0.06, 4.46, z + 0.5));
        b.torch(x + 0.05, 2.6, z + dz.signum() * 1.0, Vec3::X);
    }
}

/// Частные покои: спальня комита, покой госпожи с сыном, триклиний, кухня.
fn wing(b: &mut Builder, rng: &mut Rng) {
    let w = WING;
    let ot = 0.7;
    let it = 0.5;
    let inner_n = HALL.z1 + HALL_T;
    // Северная стена западной части (восточнее её роль играет стена зала).
    kit::wall_x(b, w.x0, HALL.x0 - 0.3, w.z0 + ot / 2.0, ot, WING_H, &[], Mat::OpusMixtum);
    let [s0, s1, s2] = WING_SPLIT;
    // Двери в сад у спальни и покоя госпожи.
    let garden_doors = [Op::Door { s: 191.0, w: 1.2, h: 2.5 }, Op::Door { s: 198.5, w: 1.2, h: 2.5 }];
    kit::wall_x(b, w.x0, w.x1, w.z1 - ot / 2.0, ot, WING_H, &garden_doors, Mat::OpusMixtum);
    kit::wall_z(b, w.z0, w.z1 - ot, w.x0 + ot / 2.0, ot, WING_H, &[], Mat::OpusMixtum);
    // Восточная стена: дверь на кухню (вход слуг с плаца).
    kit::wall_z(b, inner_n, w.z1 - ot, w.x1 - ot / 2.0, ot, WING_H, &[Op::Door { s: 152.8, w: 1.1, h: 2.4 }], Mat::OpusMixtum);
    for (x, door) in [(s0, 151.5), (s1, 151.5), (s2, 153.4)] {
        let z0 = if x < HALL.x0 { w.z0 + ot } else { inner_n };
        kit::wall_z(b, z0, w.z1 - ot, x, it, WING_H, &[Op::Door { s: door, w: 1.0, h: 2.4 }], Mat::OpusMixtum);
    }
    for (x, z) in [(w.x0, w.z1), (w.x1, w.z1)] {
        let sx = if x > w.cx() { -1.0 } else { 1.0 };
        kit::quoins(b, x + sx * ot / 2.0, z - ot / 2.0, WING_H, ot);
    }
    kit::lean_roof(b, w, Side::N, WING_H + 1.4, WING_H + 0.05, 0.35);
    kit::lean_fill(b, w, Side::N, WING_H + 1.4, WING_H + 0.05, ot, WING_H, &[(w.x0, w.x0 + ot, WING_H), (w.x1 - ot, w.x1, WING_H)], Mat::OpusMixtum);
    // Комнаты (внутренние грани).
    let z_in0 = |x0: f32| if x0 < HALL.x0 { w.z0 + ot } else { inner_n };
    let rooms = [
        Rect::new(w.x0 + ot, z_in0(w.x0), s0 - it / 2.0, w.z1 - ot),
        Rect::new(s0 + it / 2.0, z_in0(s0), s1 - it / 2.0, w.z1 - ot),
        Rect::new(s1 + it / 2.0, inner_n, s2 - it / 2.0, w.z1 - ot),
        Rect::new(s2 + it / 2.0, inner_n, w.x1 - ot, w.z1 - ot),
    ];
    // Стены покоев изнутри расписаны (двери — с наличниками).
    paint_room(b, rooms[0], &[(Side::E, 151.5, 1.36, 2.65), (Side::S, 191.0, 1.56, 2.75)]);
    paint_room(b, rooms[1], &[(Side::W, 151.5, 1.36, 2.65), (Side::E, 151.5, 1.36, 2.65), (Side::S, 198.5, 1.56, 2.75)]);
    paint_room(b, rooms[2], &[(Side::N, triclinium_table().x, 1.76, 2.95), (Side::W, 151.5, 1.36, 2.65), (Side::E, 153.4, 1.36, 2.65)]);
    cubiculum(b, rooms[0], rng);
    lady_chamber(b, rooms[1], rng);
    triclinium(b, rooms[2], rng);
    kitchen(b, rooms[3], rng);
}

/// Спальня комита: широкое ложе под покрывалом, сундук, умывальник, образ, парадные доспехи.
fn cubiculum(b: &mut Builder, r: Rect, rng: &mut Rng) {
    b.decor([r.x0, 0.0, r.z0], [r.x1, 0.03, r.z1], Mat::Mosaic);
    let bed = Rect::new(r.x0 + 0.15, r.z1 - 2.4, r.x0 + 1.65, r.z1 - 0.1);
    interior::bed(b, bed, rng);
    b.mesh(Mat::Banner).cuboid(Vec3::new(bed.x0 + 0.02, 0.66, bed.z0 + 0.6), Vec3::new(bed.x1 - 0.02, 0.69, bed.z1 - 0.02));
    // Резная спинка изголовья с позолотой.
    b.mesh(Mat::Wood).cuboid(Vec3::new(bed.x0, 0.0, bed.z1 - 0.08), Vec3::new(bed.x1, 1.25, bed.z1));
    b.mesh(Mat::Gold).cuboid(Vec3::new(bed.x0 - 0.01, 1.18, bed.z1 - 0.1), Vec3::new(bed.x1 + 0.01, 1.25, bed.z1 + 0.01));
    b.loot(L::Wealthy);
    interior::chest(b, Rect::new(r.x1 - 0.65, 153.0, r.x1 - 0.05, 154.1), "Сундук комита");
    super::cozy::wash_stand(b, Vec3::new(r.x1 - 0.4, 0.0, r.z1 - 0.45), true);
    icon_panel(b, Vec3::new(r.x0 + 0.05, 1.9, r.z0 + 1.4), Vec3::X, 0.6);
    armour_stand(b, Vec3::new(r.x1 - 0.55, 0.0, r.z0 + 0.6), true);
    super::cozy::wall_hanging(b, Vec3::new(r.x1 - 0.06, 2.0, r.cz() + 1.0), Vec3::NEG_X, 1.4, 1.5, rng);
    interior::rug(b, Rect::new(r.x0 + 2.0, r.z0 + 1.2, r.x1 - 1.2, r.z1 - 1.6), Mat::Rug);
    interior::lampstand(b, Vec3::new(bed.x1 + 0.35, 0.0, bed.z1 - 0.3), true);
    // Меч комита на стене.
    let sw = Vec3::new(r.x0 + 0.03, 1.6, r.z0 + 2.8);
    b.mesh(Mat::Iron).cuboid(sw + Vec3::new(0.0, -0.45, -0.025), sw + Vec3::new(0.03, 0.45, 0.025));
    b.mesh(Mat::Gold).cuboid(sw + Vec3::new(0.0, 0.45, -0.1), sw + Vec3::new(0.04, 0.49, 0.1));
    b.mesh(Mat::Leather).cuboid(sw + Vec3::new(0.0, 0.49, -0.02), sw + Vec3::new(0.035, 0.62, 0.02));
}

/// Стены покоя изнутри: цоколь под мрамор, белёное поле, красный фриз с полосой охры.
/// Двери — (сторона, где по стене, ширина проёма с наличником, высота).
fn paint_room(b: &mut Builder, r: Rect, doors: &[(Side, f32, f32, f32)]) {
    let bands = [(0.03, 0.95, Mat::Ochre), (0.95, 1.04, Mat::Marble), (1.04, 2.6, Mat::Plaster), (2.6, 2.82, Mat::Banner), (2.82, 2.9, Mat::Gold), (2.9, WING_H - 0.02, Mat::Plaster)];
    let holes = |side: Side| -> Vec<(f32, f32, f32, f32)> { doors.iter().filter(|d| d.0 == side).map(|d| (d.1 - d.2 / 2.0, d.1 + d.2 / 2.0, 0.0, d.3)).collect() };
    skin_bands(b, true, r.x0, r.x1, r.z0, 1.0, &holes(Side::N), &bands);
    skin_bands(b, true, r.x0, r.x1, r.z1, -1.0, &holes(Side::S), &bands);
    skin_bands(b, false, r.z0, r.z1, r.x0, 1.0, &holes(Side::W), &bands);
    skin_bands(b, false, r.z0, r.z1, r.x1, -1.0, &holes(Side::E), &bands);
}

/// Покой госпожи Евфимии: ткацкий стан с пурпурной шерстью, её ложе и постель сына, игрушки.
fn lady_chamber(b: &mut Builder, r: Rect, rng: &mut Rng) {
    b.decor([r.x0, 0.0, r.z0], [r.x1, 0.03, r.z1], Mat::Mosaic);
    let (lp, _) = seat(Seat::Loom);
    kit::loom(b, Vec3::new(lp.x + 0.75, 0.0, lp.z), false);
    for k in 0..3 {
        interior::basket(b, Vec3::new(lp.x + 0.2, 0.0, lp.z + 1.3 + k as f32 * 0.55), 0.22, 0.32, if k == 0 { Mat::Banner } else { Mat::Cloth }, None);
    }
    let bed = Rect::new(r.x0 + 0.15, r.z1 - 2.3, r.x0 + 1.5, r.z1 - 0.1);
    interior::bed(b, bed, rng);
    b.mesh(Mat::Cloth).cuboid(Vec3::new(bed.x0 + 0.02, 0.66, bed.z0 + 0.6), Vec3::new(bed.x1 - 0.02, 0.69, bed.z1 - 0.02));
    // Постель сына — поменьше, у восточной стены.
    interior::bed(b, Rect::new(r.x1 - 1.0, r.z1 - 1.75, r.x1 - 0.15, r.z1 - 0.1), rng);
    super::cozy::toys(b, Vec3::new(r.x1 - 1.4, 0.0, r.z1 - 1.0), rng);
    b.loot(L::Wealthy);
    interior::chest(b, Rect::new(r.x0 + 1.65, r.z1 - 0.65, r.x0 + 2.55, r.z1 - 0.05), "Сундук госпожи");
    super::cozy::wash_stand(b, Vec3::new(r.x0 + 0.4, 0.0, r.z0 + 0.5), true);
    icon_panel(b, Vec3::new(r.x0 + 0.05, 1.9, r.z0 + 3.8), Vec3::X, 0.55);
    super::cozy::wall_hanging(b, Vec3::new(199.0, 2.2, HALL.z1 + HALL_T + 0.06), Vec3::Z, 1.6, 1.4, rng);
    interior::rug(b, Rect::new(r.x0 + 1.9, r.z0 + 2.4, r.x1 - 1.4, r.z1 - 1.3), Mat::Rug);
    interior::lampstand(b, Vec3::new(bed.x1 + 0.3, 0.0, bed.z0 - 0.2), true);
}

/// Триклиний: стибадий (полукруглое ложе-сигма) вокруг круглого мраморного стола, посуда,
/// мозаичный пол, светильники, завеса.
fn triclinium(b: &mut Builder, r: Rect, rng: &mut Rng) {
    b.decor([r.x0, 0.0, r.z0], [r.x1, 0.03, r.z1], Mat::Mosaic);
    let c = triclinium_table();
    // Сигма: мягкое ложе по дуге с юга, открыто к двери. Без коллизии — на нём сидят.
    let (r0, r1) = (1.15, 2.2);
    let (a0, a1) = (0.1, PI - 0.1);
    let seg = 16;
    for k in 0..seg {
        let t0 = a0 + (a1 - a0) * k as f32 / seg as f32;
        let t1 = a0 + (a1 - a0) * (k + 1) as f32 / seg as f32;
        let p = |a: f32, rr: f32, y: f32| c + Vec3::new(a.cos() * rr, y, a.sin() * rr);
        let base = b.mesh(Mat::Wood);
        base.quad([p(t0, r0, 0.3), p(t1, r0, 0.3), p(t1, r0, 0.0), p(t0, r0, 0.0)], -Vec3::new(((t0 + t1) / 2.0).cos(), 0.0, ((t0 + t1) / 2.0).sin()));
        let cushion = b.mesh(Mat::Banner);
        cushion.quad([p(t0, r0, 0.45), p(t1, r0, 0.45), p(t1, r1, 0.45), p(t0, r1, 0.45)], Vec3::Y);
        cushion.quad([p(t0, r0, 0.45), p(t1, r0, 0.45), p(t1, r0, 0.3), p(t0, r0, 0.3)], -Vec3::new(((t0 + t1) / 2.0).cos(), 0.0, ((t0 + t1) / 2.0).sin()));
        // Валик-спинка по внешнему краю.
        let back = b.mesh(Mat::Rug);
        back.quad([p(t0, r1, 0.45), p(t1, r1, 0.45), p(t1, r1, 0.85), p(t0, r1, 0.85)], -Vec3::new(((t0 + t1) / 2.0).cos(), 0.0, ((t0 + t1) / 2.0).sin()));
        back.quad([p(t0, r1, 0.85), p(t1, r1, 0.85), p(t1, r1 + 0.2, 0.85), p(t0, r1 + 0.2, 0.85)], Vec3::Y);
        b.mesh(Mat::Wood).quad([p(t0, r1 + 0.2, 0.0), p(t1, r1 + 0.2, 0.0), p(t1, r1 + 0.2, 0.85), p(t0, r1 + 0.2, 0.85)], Vec3::new(((t0 + t1) / 2.0).cos(), 0.0, ((t0 + t1) / 2.0).sin()));
    }
    // За спинкой — коллизия (ложе нельзя пройти насквозь со спины).
    for k in 0..6 {
        let a = a0 + (a1 - a0) * (k as f32 + 0.5) / 6.0;
        let q = c + Vec3::new(a.cos(), 0.0, a.sin()) * (r1 + 0.1);
        b.invisible.push(Aabb::new(q - Vec3::new(0.3, 0.0, 0.3), q + Vec3::new(0.3, 0.85, 0.3)));
    }
    // Круглый стол на одной ножке-львиной лапе.
    let m = b.mesh(Mat::Marble);
    m.lathe(c, &[(0.0, 0.0), (0.3, 0.0), (0.12, 0.08), (0.08, 0.55), (0.65, 0.6), (0.68, 0.66), (0.0, 0.66)], 18);
    b.invisible.push(Aabb::new(c - Vec3::new(0.6, 0.0, 0.6), c + Vec3::new(0.6, 0.66, 0.6)));
    let top = c + Vec3::Y * 0.66;
    let wares = super::cozy::table_wares(2, rng);
    super::cozy::lay(b, Rect::new(top.x - 0.45, top.z - 0.45, top.x + 0.45, top.z + 0.45), top.y, &wares, rng);
    super::cozy::fruit_tray(b, top + Vec3::new(0.0, 0.0, 0.1), Mat::PetalPurple, rng);
    for dx in [-1.7f32, 1.7] {
        candelabrum(b, c + Vec3::new(dx * 1.45, 0.0, -1.1));
    }
    b.lamp(c + Vec3::new(0.0, 2.2, -0.6), Color::srgb(1.0, 0.72, 0.42), 6_000.0, 7.0);
    b.lamp(c + Vec3::new(0.0, 3.2, 1.2), Color::srgb(1.0, 0.7, 0.4), 4_000.0, 7.0);
    super::cozy::wall_hanging(b, Vec3::new(r.cx(), 2.3, r.z1 - 0.06), Vec3::NEG_Z, 2.2, 1.6, rng);
    // Буфет (абак) с серебром и стеклом у западной стены.
    let ab = Rect::new(r.x0 + 0.05, 154.2, r.x0 + 0.55, 155.8);
    b.solid([ab.x0, 0.0, ab.z0], [ab.x1, 0.95, ab.z1], Mat::Marble);
    for k in 0..4 {
        let p = Vec3::new(ab.cx(), 0.95, ab.z0 + 0.25 + k as f32 * 0.38);
        let mat = if k % 2 == 0 { Mat::Glass } else { Mat::Bronze };
        b.mesh(mat).lathe(p, &[(0.0, 0.0), (0.05, 0.0), (0.03, 0.08), (0.07, 0.18), (0.0, 0.18)], 10);
    }
    b.loot(L::Kitchen);
    interior::basket(b, Vec3::new(r.x1 - 0.4, 0.0, r.z0 + 0.5), 0.25, 0.4, Mat::Bread, Some("Корзина с хлебом"));
    b.loot(L::Wealthy);
}

/// Кухня: очаг с котлом, полки с посудой, амфоры, стол, ложе кухарки.
fn kitchen(b: &mut Builder, r: Rect, rng: &mut Rng) {
    let (hp, _) = seat(Seat::KitchenHearth);
    let hearth = Vec3::new(hp.x + 0.5, 0.0, r.z1 - 0.75);
    kit::hearth(b, hearth);
    interior::tripod_cauldron(b, hearth);
    b.invisible.push(Aabb::new(hearth - Vec3::new(0.55, 0.0, 0.55), hearth + Vec3::new(0.55, 0.6, 0.55)));
    b.lamp(hearth + Vec3::Y * 0.7, Color::srgb(1.0, 0.55, 0.25), 3_000.0, 5.0);
    interior::bed(b, Rect::new(r.x0 + 0.05, r.z0 + 0.05, r.x0 + 0.95, r.z0 + 1.9), rng);
    kit::shelf(b, Rect::new(r.x1 - 0.4, r.z0 + 0.2, r.x1 - 0.05, r.z0 + 1.8), 3, rng, Mat::Ceramic);
    let t = Rect::new(r.cx() - 0.5, r.z0 + 2.2, r.cx() + 0.6, r.z0 + 2.9);
    kit::table(b, t, 0.8);
    super::cozy::loaf(b, t.center(0.8) + Vec3::new(-0.2, 0.0, 0.0));
    super::cozy::mortarium(b, t.center(0.8) + Vec3::new(0.25, 0.0, 0.1));
    for k in 0..3 {
        kit::amphora(b, Vec3::new(r.x1 - 0.35, 0.0, r.z1 - 2.6 + k as f32 * 0.55), 1.0, k);
    }
    super::cozy::hanging_fish(b, Vec3::new(r.x0 + 0.15, 2.0, r.z0 + 2.3), Vec3::new(r.x0 + 0.15, 2.0, r.z0 + 3.6), 4);
    b.loot(L::Kitchen);
    interior::basket(b, Vec3::new(r.x0 + 0.45, 0.0, r.z1 - 0.45), 0.25, 0.4, Mat::Canvas, Some("Корзина с припасами"));
    b.loot(L::Wealthy);
}

/// Сад комита: дорожка, кипарисы у стены, цветники с розами, мраморная чаша, скамья, смоковница.
fn garden(b: &mut Builder, rng: &mut Rng) {
    let g = GARDEN;
    b.at(g.cx(), g.cz());
    // Восточная ограда с калиткой (во двор у казарм).
    kit::wall_z(b, g.z0, g.z1, g.x1 - 0.25, 0.5, 2.2, &[Op::Door { s: 161.3, w: 1.1, h: 2.0 }], Mat::Rubble);
    b.decor([g.x0 + 0.7, 0.0, 160.4], [g.x1 - 0.5, 0.025, 162.0], Mat::Paving);
    for k in 0..7 {
        kit::cypress(b, Vec3::new(189.2 + k as f32 * 4.1, 0.0, g.z1 - 0.8), rng.range(4.8, 6.2));
    }
    for (x0, x1) in [(187.8, 190.2), (192.0, 196.6), (205.5, 213.5)] {
        flower_bed(b, Rect::new(x0, g.z0 + 0.3, x1, g.z0 + 1.6), rng);
    }
    fountain(b, Vec3::new(205.0, 0.0, 161.2), 0.85);
    b.solid([196.3, 0.0, 162.45], [198.1, 0.45, 162.95], Mat::Marble);
    b.mesh(Mat::Marble).cuboid(Vec3::new(196.2, 0.45, 162.4), Vec3::new(198.2, 0.5, 163.0));
    kit::fig_tree(b, Vec3::new(209.5, 0.0, 162.9), 3.6, rng);
    interior::lampstand(b, Vec3::new(200.6, 0.0, 159.3), true);
    pergola(b, 188.6, 199.8, 160.25, 162.3, rng);
}

/// Пергола над дорожкой: столбы, балки и обрешётка, по которой вьётся виноград с гроздьями.
fn pergola(b: &mut Builder, x0: f32, x1: f32, z0: f32, z1: f32, rng: &mut Rng) {
    let h = 2.55;
    let n = ((x1 - x0) / 2.3).round().max(1.0) as usize;
    for k in 0..=n {
        let x = x0 + (x1 - x0) * k as f32 / n as f32;
        for z in [z0, z1] {
            b.solid([x - 0.07, 0.0, z - 0.07], [x + 0.07, h, z + 0.07], Mat::Wood);
        }
        b.mesh(Mat::Wood).cuboid(Vec3::new(x - 0.06, h, z0 - 0.35), Vec3::new(x + 0.06, h + 0.14, z1 + 0.35));
    }
    let m = b.mesh(Mat::Wood);
    for z in [z0, z1] {
        m.cuboid(Vec3::new(x0 - 0.3, h - 0.12, z - 0.06), Vec3::new(x1 + 0.3, h, z + 0.06));
    }
    for k in 0..6 {
        let z = z0 - 0.2 + (z1 - z0 + 0.4) * k as f32 / 5.0;
        m.cuboid(Vec3::new(x0 - 0.3, h + 0.14, z - 0.025), Vec3::new(x1 + 0.3, h + 0.19, z + 0.025));
    }
    // Листва лозы — неровными купами по обрешётке, грозди свисают между балками.
    let leaves = ((x1 - x0) * 7.0) as usize;
    for _ in 0..leaves {
        let p = Vec3::new(rng.range(x0 - 0.2, x1 + 0.2), h + 0.12 + rng.range(0.0, 0.2), rng.range(z0 - 0.3, z1 + 0.3));
        let r = rng.range(0.2, 0.38);
        let mat = if rng.chance(0.7) { Mat::Leaves } else { Mat::HerbGreen };
        b.mesh(mat).lathe(p, &[(0.0, -0.09), (r * 0.7, -0.06), (r, 0.02), (r * 0.85, 0.11), (r * 0.45, 0.17), (0.0, 0.19)], 9);
    }
    // Грозди: каждая — несколько ягод-шариков, книзу уже.
    for _ in 0..((x1 - x0) * 1.4) as usize {
        let top = Vec3::new(rng.range(x0 + 0.2, x1 - 0.2), h + 0.08, rng.range(z0 + 0.2, z1 - 0.2));
        b.mesh(Mat::Wood).beam(top, top - Vec3::Y * 0.06, 0.006, 0.006, Vec3::X);
        for k in 0..9 {
            let row = (k / 3) as f32;
            let a = k as f32 * 2.1 + rng.range(0.0, 0.6);
            let rr = 0.035 * (1.0 - row * 0.3);
            let q = top - Vec3::Y * (0.08 + row * 0.045) + Vec3::new(a.cos() * rr, 0.0, a.sin() * rr);
            b.mesh(Mat::PetalPurple).lathe(q - Vec3::Y * 0.022, &[(0.0, 0.0), (0.018, 0.008), (0.022, 0.022), (0.018, 0.036), (0.0, 0.044)], 6);
        }
    }
    // Плети свисают с краёв обрешётки.
    for _ in 0..((x1 - x0) * 1.6) as usize {
        let z = if rng.chance(0.5) { z0 - 0.25 } else { z1 + 0.25 };
        let p = Vec3::new(rng.range(x0, x1), h + 0.05, z);
        let len = rng.range(0.3, 0.75);
        b.mesh(Mat::Leaves).lathe(p - Vec3::Y * len, &[(0.0, 0.0), (0.09, 0.1), (0.14, len * 0.6), (0.1, len), (0.0, len + 0.04)], 6);
    }
    // Лоза у столбов.
    for k in [0, n] {
        let x = x0 + (x1 - x0) * k as f32 / n as f32;
        for z in [z0, z1] {
            b.mesh(Mat::Wood).beam(Vec3::new(x + 0.09, 0.0, z), Vec3::new(x + 0.12, h, z + 0.05), 0.03, 0.03, Vec3::X);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palace_fits_in_citadel() {
        let c = super::super::CITADEL;
        for r in [PERI, LANE, HALL, WING, GARDEN, PROPYLON] {
            assert!(r.x0 >= c.x0 && r.x1 <= c.x1 + 0.5 && r.z0 >= c.z0 && r.z1 <= c.z1 + 0.01, "{r:?} вне цитадели");
        }
        // Апсида не выходит за западную стену цитадели.
        assert!(apse_center().x - APSE_R - HALL_T > c.x0);
    }

    #[test]
    fn seats_are_distinct() {
        let s = all_seats();
        for (i, a) in s.iter().enumerate() {
            for b in &s[i + 1..] {
                assert!(seat(*a).0.distance(seat(*b).0) > 0.5, "{a:?} и {b:?} на одном месте");
            }
        }
    }
}
