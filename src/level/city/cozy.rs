//! Уют: мелочи, без которых дом выглядит макетом, и вещи хозяев, которые можно взять.
//!
//! Что лежало и висело в доме горожанина Херсонеса VI в. (по находкам в усадьбах Херсонеса,
//! Сард, Анемурия, домам Сирии и Египта того же времени):
//! * на столе — краснолаковые миски (понтийские и фокейские), кружки, стеклянные кубки на
//!   ножке, хлеб, сыр, сушёные смоквы, глиняный светильник; у зажиточных — бронзовый кувшин
//!   и серебряные ложки (такие ложки — частая находка в кладах VI в.);
//! * на стенных полках — посуда, светильники, ампулы святого Мины (паломнические флаконы
//!   из Египта — их находят по всему Причерноморью), шкатулки;
//! * на стенах — тканые шерстяные завесы с полосами-клавами и квадратами-табулами (как ткани
//!   из египетских погребений), крест; цоколь горницы расписан красной охрой;
//! * в спальне — костяные гребни, бронзовые зеркальца, веретёна, сандалии у ложа, ночной горшок;
//! * у детей — деревянные лошадки на колёсиках, бабки и кости;
//! * в кухне — котлы и сковороды на колышках, связки рыбы, дрова у очага, ступа-мортарий.
//!
//! Вещи хозяев (настоящие предметы) кладутся через [`lay`]: в жилом доме это
//! [`crate::level::HouseItemSpec`], а в обставляемом доме игрока или вне дома — просто меш
//! той же формы, чтобы и там было уютно.

use super::interior::{Room, axis, inward, oil_lamp};
use super::kit::{self, Rect, Rng, Side};
use super::*;
use crate::items::ItemBaseId;

// ---------------------------------------------------------------------------
// Вещи хозяев
// ---------------------------------------------------------------------------

pub const LAMP: &str = "Глиняный светильник";
pub const BRONZE_LAMP: &str = "Бронзовый светильник";
pub const BOWL: &str = "Краснолаковая миска";
pub const CUP: &str = "Глиняная кружка";
pub const GOBLET: &str = "Стеклянный кубок";
pub const EWER: &str = "Бронзовый кувшин";
pub const SPOON: &str = "Серебряная ложка";
pub const KNIFE: &str = "Кухонный нож";
pub const BREAD: &str = "Каравай хлеба";
pub const CHEESE: &str = "Головка сыра";
pub const FIGS: &str = "Сушёные смоквы";
pub const COMB: &str = "Костяной гребень";
pub const MIRROR: &str = "Бронзовое зеркальце";
pub const AMPULLA: &str = "Ампула святого Мины";
pub const ENKOLPION: &str = "Энколпион";
pub const DICE: &str = "Игральные кости";
pub const HORSE: &str = "Деревянная лошадка";
pub const SPINDLE: &str = "Веретено с пряслицем";
pub const YARN: &str = "Моток пряжи";
pub const SHEARS: &str = "Железные ножницы";
pub const CASKET: &str = "Деревянная шкатулка";
pub const TABULA: &str = "Доска для тавлей";

/// Что кладётся на стол или полку: вещь хозяев или просто утварь (меш).
#[derive(Clone, Copy, Debug)]
pub enum Ware {
    /// Настоящий предмет: имя вида и сколько штук.
    Item(&'static str, u16),
    /// Кувшин высотой `h` (меш).
    Jug(f32),
    /// Стопка мисок.
    Bowls,
    /// Горшок с крышкой.
    Pot,
    /// Плоская корзинка с плодами.
    Fruit(Mat),
}

/// Вещь хозяев по одной штуке.
pub fn one(name: &'static str) -> Ware {
    Ware::Item(name, 1)
}

/// Половинные размеры модели предмета (как у его физического тела).
fn half(name: &str) -> Vec3 {
    let h = crate::icons::model_half_extents(ItemBaseId::by_name(name));
    Vec3::from_array(h).max(Vec3::splat(0.008))
}

/// Место на поверхности: (вдоль, поперёк, поворот так, чтобы длинная сторона легла вдоль).
fn footprint(w: Ware, along_x: bool) -> (f32, f32, f32) {
    match w {
        Ware::Item(name, _) => {
            let h = half(name);
            let (long, short) = (h.x.max(h.z), h.x.min(h.z));
            // Длинная сторона модели — по X: вдоль X-поверхности поворот 0, иначе 90°.
            let model_x = h.x >= h.z;
            let yaw = if model_x == along_x { 0.0 } else { std::f32::consts::FRAC_PI_2 };
            (long * 2.2 + 0.02, short * 2.2 + 0.02, yaw)
        }
        Ware::Jug(h) => (h * 0.75, h * 0.75, 0.0),
        Ware::Bowls => (0.24, 0.24, 0.0),
        Ware::Pot => (0.24, 0.24, 0.0),
        Ware::Fruit(_) => (0.34, 0.3, 0.0),
    }
}

/// Разложить вещи по поверхности `f` на высоте `y` (стол, полка, крышка сундука) в ряд вдоль
/// длинной стороны; что не влезло — не кладётся. Возвращает, сколько легло.
pub fn lay(b: &mut Builder, f: Rect, y: f32, wares: &[Ware], rng: &mut Rng) -> usize {
    let along_x = f.w() >= f.d();
    let (len, depth) = if along_x { (f.w(), f.d()) } else { (f.d(), f.w()) };
    let start = if along_x { f.x0 } else { f.z0 };
    let mut s = rng.range(0.02, 0.1);
    let mut placed = 0;
    for &w in wares {
        let (a, c, yaw) = footprint(w, along_x);
        if c > depth + 0.02 || s + a > len - 0.02 {
            continue;
        }
        let spare = (depth - c).max(0.0) / 2.0;
        let off = if spare > 0.0 { rng.range(-spare, spare) } else { 0.0 };
        let t = start + s + a / 2.0;
        let p = if along_x { Vec3::new(t, y, f.cz() + off) } else { Vec3::new(f.cx() + off, y, t) };
        // Чуть вкривь и иногда задом наперёд: вещи кладут руками, а не по линейке.
        let yaw = yaw + rng.range(-0.18, 0.18) + if rng.chance(0.5) { std::f32::consts::PI } else { 0.0 };
        place(b, p, w, yaw, rng);
        s += a + rng.range(0.03, 0.12);
        placed += 1;
    }
    placed
}

/// Положить одну вещь (низом в `p`).
pub fn place(b: &mut Builder, p: Vec3, w: Ware, yaw: f32, rng: &mut Rng) {
    match w {
        Ware::Item(name, count) => {
            if b.house.is_some() {
                b.house_item(p, yaw, name, count);
            } else {
                stand_in(b, p, name);
            }
        }
        Ware::Jug(h) => kit::jug(b, p, h, Mat::Ceramic),
        Ware::Bowls => bowl_stack(b, p, 3 + rng.int(3)),
        Ware::Pot => lidded_pot(b, p),
        Ware::Fruit(m) => fruit_tray(b, p, m, rng),
    }
}

/// Вещь хозяев на полу (амфора, лошадка у ложа), если в доме; иначе — меш.
pub fn put(b: &mut Builder, p: Vec3, name: &'static str, count: u16, rng: &mut Rng) {
    let yaw = rng.range(-std::f32::consts::PI, std::f32::consts::PI);
    place(b, p, Ware::Item(name, count), yaw, rng);
}

/// Меш вместо настоящей вещи (в доме игрока или вне жилого дома).
fn stand_in(b: &mut Builder, p: Vec3, name: &str) {
    let n = name.to_lowercase();
    if n.contains("миска") || n.contains("чаша") {
        let mat = if n.contains("стекл") { Mat::Glass } else { Mat::Ceramic };
        b.mesh(mat).lathe(p, &[(0.0, 0.0), (0.045, 0.0), (0.09, 0.045), (0.085, 0.05), (0.04, 0.012), (0.0, 0.012)], 12);
    } else if n.contains("кружка") || n.contains("кубок") {
        let mat = if n.contains("кубок") { Mat::Glass } else { Mat::Ceramic };
        b.mesh(mat).lathe(p, &[(0.0, 0.0), (0.03, 0.0), (0.04, 0.05), (0.045, 0.1), (0.04, 0.1), (0.0, 0.02)], 10);
    } else if n.contains("светильник") {
        oil_lamp(b, p);
    } else if n.contains("хлеб") {
        loaf(b, p);
    } else if n.contains("сыр") {
        b.mesh(Mat::PetalWhite).lathe(p, &[(0.0, 0.0), (0.08, 0.0), (0.09, 0.03), (0.08, 0.06), (0.0, 0.06)], 12);
    } else if n.contains("кувшин") || n.contains("горшок") {
        kit::jug(b, p, 0.24, if n.contains("бронз") || n.contains("медн") { Mat::Bronze } else { Mat::Ceramic });
    } else if n.contains("смоквы") {
        fruit_tray(b, p, Mat::Bread, &mut Rng::new((p.x * 97.0 + p.z * 13.0) as u32));
    }
}

// ---------------------------------------------------------------------------
// Мелкая утварь (меши)
// ---------------------------------------------------------------------------

/// Круглый хлеб с надрезами.
pub fn loaf(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Bread).lathe(p, &[(0.0, 0.0), (0.08, 0.0), (0.095, 0.025), (0.08, 0.06), (0.04, 0.075), (0.0, 0.078)], 12);
}

/// Стопка краснолаковых мисок.
pub fn bowl_stack(b: &mut Builder, p: Vec3, n: usize) {
    let m = b.mesh(Mat::Ceramic);
    for k in 0..n {
        let q = p + Vec3::Y * (k as f32 * 0.018);
        m.lathe(q, &[(0.0, 0.0), (0.05, 0.0), (0.1, 0.04), (0.095, 0.045), (0.045, 0.01), (0.0, 0.01)], 12);
    }
}

/// Кухонный горшок с крышкой.
pub fn lidded_pot(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.06, 0.0), (0.1, 0.07), (0.095, 0.13), (0.08, 0.15), (0.085, 0.16)], 12);
    b.mesh(Mat::Ceramic).lathe(p + Vec3::Y * 0.155, &[(0.0, 0.03), (0.012, 0.03), (0.015, 0.02), (0.085, 0.0), (0.0, 0.0)], 10);
}

/// Плоская корзинка с плодами (смоквы, яблоки, виноград, лук).
pub fn fruit_tray(b: &mut Builder, p: Vec3, fruit: Mat, rng: &mut Rng) {
    b.mesh(Mat::Thatch).lathe(p, &[(0.0, 0.0), (0.12, 0.0), (0.15, 0.06), (0.14, 0.065), (0.11, 0.01)], 10);
    let m = b.mesh(fruit);
    for k in 0..7 {
        let a = k as f32 * 2.4 + rng.range(0.0, 0.5);
        let r = if k == 0 { 0.0 } else { rng.range(0.04, 0.09) };
        let q = p + Vec3::new(a.cos() * r, 0.035 + if k == 0 { 0.03 } else { 0.0 }, a.sin() * r);
        m.lathe(q, &[(0.0, 0.0), (0.025, 0.006), (0.032, 0.025), (0.02, 0.045), (0.0, 0.05)], 6);
    }
}

/// Сандалии у ложа (подошвы с ремешками).
pub fn sandals(b: &mut Builder, p: Vec3, dir: Vec3) {
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    let m = b.mesh(Mat::Leather);
    for s in [-1.0f32, 1.0] {
        let c = p + side * (s * 0.07) + dir * (s * 0.03);
        m.beam(c - dir * 0.12, c + dir * 0.12, 0.045, 0.008, Vec3::Y);
        m.beam(c - side * 0.045 + dir * 0.03 + Vec3::Y * 0.02, c + side * 0.045 + dir * 0.03 + Vec3::Y * 0.02, 0.01, 0.006, Vec3::Y);
    }
}

/// Ночной горшок.
pub fn chamber_pot(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.09, 0.0), (0.13, 0.1), (0.12, 0.17), (0.14, 0.19), (0.12, 0.19)], 12);
}

/// Сложенные шерстяные одеяла (на крышке сундука).
pub fn folded_blankets(b: &mut Builder, p: Vec3, along_x: bool, rng: &mut Rng) {
    let (hx, hz) = if along_x { (0.3, 0.2) } else { (0.2, 0.3) };
    let mut y = p.y;
    for k in 0..(2 + rng.int(2)) {
        let mat = [Mat::Rug, Mat::Cloth, Mat::Canvas, Mat::Felt][(k + rng.int(4)) % 4];
        let t = rng.range(0.04, 0.06);
        let j = Vec3::new(rng.range(-0.02, 0.02), 0.0, rng.range(-0.02, 0.02));
        b.mesh(mat).cuboid(Vec3::new(p.x - hx, y, p.z - hz) + j, Vec3::new(p.x + hx, y + t, p.z + hz) + j);
        y += t;
    }
}

/// Валики и подушки на скамье или ложе (`f` — сиденье, `y` — его верх).
pub fn cushions(b: &mut Builder, f: Rect, y: f32, rng: &mut Rng) {
    let along_x = f.w() > f.d();
    let len = if along_x { f.w() } else { f.d() };
    let n = ((len / 0.55) as usize).max(1);
    for k in 0..n {
        if rng.chance(0.25) {
            continue;
        }
        let t = (k as f32 + 0.5) / n as f32;
        let c = if along_x { Vec3::new(f.x0 + f.w() * t, y, f.cz()) } else { Vec3::new(f.cx(), y, f.z0 + f.d() * t) };
        let mat = [Mat::Rug, Mat::Cloth, Mat::PetalYellow, Mat::Canvas][rng.int(4)];
        let (hx, hz) = if along_x { (0.2, 0.15) } else { (0.15, 0.2) };
        b.mesh(mat).cuboid(c - Vec3::new(hx, 0.0, hz), c + Vec3::new(hx, 0.09, hz));
        // Кисточки по углам.
        let tassel = b.mesh(Mat::Cloth);
        for (sx, sz) in [(-1.0, -1.0), (1.0, 1.0)] {
            let q = c + Vec3::new(sx * hx, 0.05, sz * hz);
            tassel.cuboid(q - Vec3::splat(0.012), q + Vec3::splat(0.012));
        }
    }
}

/// Тканая шерстяная завеса на стене: полотно, две полосы-клавы, квадраты-табулы и кайма.
/// `p` — середина на стене, `out` — от стены в комнату.
pub fn wall_hanging(b: &mut Builder, p: Vec3, out: Vec3, w: f32, h: f32, rng: &mut Rng) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let p = p - out * 0.035;
    let ground = [Mat::Canvas, Mat::Canvas, Mat::PetalWhite, Mat::Rug][rng.int(4)];
    let ink = if ground == Mat::Rug { Mat::PetalYellow } else { [Mat::Rug, Mat::Cloth, Mat::PetalPurple][rng.int(3)] };
    let lo = p - side * (w / 2.0) - Vec3::Y * (h / 2.0);
    let hi = p + side * (w / 2.0) + Vec3::Y * (h / 2.0);
    b.mesh(ground).cuboid(lo, hi + out * 0.012);
    let front = out * 0.016;
    // Кайма сверху и снизу.
    for y in [-h / 2.0 + 0.06, h / 2.0 - 0.1] {
        b.mesh(ink).cuboid(p - side * (w / 2.0 - 0.04) + Vec3::Y * y, p + side * (w / 2.0 - 0.04) + Vec3::Y * (y + 0.05) + front);
    }
    // Клавы — вертикальные полосы.
    for s in [-0.28f32, 0.28] {
        let c = p + side * (s * w);
        b.mesh(ink).cuboid(c - side * 0.025 - Vec3::Y * (h / 2.0 - 0.14), c + side * 0.025 + Vec3::Y * (h / 2.0 - 0.16) + front);
    }
    // Табула посередине (с внутренним квадратом).
    let t = 0.11f32.min(w * 0.12);
    b.mesh(ink).cuboid(p - side * t - Vec3::Y * t, p + side * t + Vec3::Y * t + front);
    let i = t * 0.5;
    b.mesh(ground).cuboid(p - side * i - Vec3::Y * i + front, p + side * i + Vec3::Y * i + out * 0.02);
    // Бахрома снизу и палка сверху.
    let fringe = b.mesh(ink);
    let n = (w / 0.06) as usize;
    for k in 0..n {
        let q = lo + side * ((k as f32 + 0.5) * w / n as f32);
        fringe.cuboid(q - side * 0.006 - Vec3::Y * 0.07, q + side * 0.006 + out * 0.01);
    }
    b.mesh(Mat::Wood).beam(p - side * (w / 2.0 + 0.08) + Vec3::Y * (h / 2.0 + 0.02) + out * 0.02, p + side * (w / 2.0 + 0.08) + Vec3::Y * (h / 2.0 + 0.02) + out * 0.02, 0.015, 0.015, out);
}

/// Цоколь, расписанный красной охрой, с тёмной полосой поверху — вдоль всех стен комнаты,
/// кроме дверных проёмов. `r` — комната (как в `furnish`, отступ 5 см от стен).
pub fn painted_socle(b: &mut Builder, room: &Room, height: f32) {
    let wall = room.r.inset(-0.05);
    let t = 0.012;
    for side in [Side::N, Side::S, Side::W, Side::E] {
        let along_x = side.along_x();
        let (s0, s1) = if along_x { (wall.x0, wall.x1) } else { (wall.z0, wall.z1) };
        let at = match side {
            Side::N => wall.z0,
            Side::S => wall.z1,
            Side::W => wall.x0,
            Side::E => wall.x1,
        };
        // Проёмы на этой стене.
        let mut gaps: Vec<(f32, f32)> = room
            .doors
            .iter()
            .filter(|d| d.along_x == along_x && ((if along_x { d.center.z } else { d.center.x }) - at).abs() < 0.8)
            .map(|d| {
                let c = if along_x { d.center.x } else { d.center.z };
                (c - d.width / 2.0 - 0.04, c + d.width / 2.0 + 0.04)
            })
            .collect();
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut spans = Vec::new();
        let mut s = s0;
        for (g0, g1) in gaps {
            if g0 > s {
                spans.push((s, g0));
            }
            s = s.max(g1);
        }
        if s < s1 {
            spans.push((s, s1));
        }
        let n = inward(side);
        for (a, c) in spans {
            if c - a < 0.1 {
                continue;
            }
            let (p0, p1) = if along_x { (Vec3::new(a, 0.0, at), Vec3::new(c, 0.0, at)) } else { (Vec3::new(at, 0.0, a), Vec3::new(at, 0.0, c)) };
            let lo = p0.min(p1 + n * t);
            let hi = p0.max(p1 + n * t);
            b.mesh(Mat::Ochre).cuboid(lo + Vec3::Y * 0.02, hi + Vec3::Y * height);
            let lo2 = p0.min(p1 + n * (t + 0.004));
            let hi2 = p0.max(p1 + n * (t + 0.004));
            b.mesh(Mat::Rug).cuboid(lo2 + Vec3::Y * height, hi2 + Vec3::Y * (height + 0.035));
        }
    }
}

/// Полка на кронштейнах у стены (две доски) с вещами; под ней — что-нибудь на полу.
/// Доски твёрдые: на них можно класть вещи, с них их можно уронить.
pub fn shelf_unit(b: &mut Builder, room: &mut Room, sides: &[Side], len: f32, upper: &[Ware], lower: &[Ware], rng: &mut Rng) -> bool {
    let Some((f, side)) = room.take(len, 0.28, sides, rng) else { return false };
    let n = inward(side);
    let along = axis(side);
    let mut boards = Vec::new();
    for (y, wares) in [(1.0f32, lower), (1.5, upper)] {
        if wares.is_empty() && y < 1.2 {
            continue;
        }
        b.solid([f.x0, y - 0.035, f.z0], [f.x1, y, f.z1], Mat::Wood);
        boards.push((y, wares));
        // Кронштейны.
        let m = b.mesh(Mat::Wood);
        for s in [-1.0f32, 1.0] {
            let wall = f.center(y - 0.035) - n * (if side.along_x() { f.d() } else { f.w() } / 2.0) + along * (s * (len / 2.0 - 0.12));
            m.beam(wall - Vec3::Y * 0.22, wall + n * 0.22, 0.015, 0.02, along);
        }
    }
    for (y, wares) in boards {
        lay(b, f.inset(0.02), y, wares, rng);
    }
    // Под полкой: амфора, корзина или пара кувшинов.
    let c = f.center(0.0);
    match rng.int(4) {
        0 => kit::amphora(b, c + along * (len * 0.25), 0.8, rng.int(3) as u32),
        1 => super::interior::basket(b, c - along * (len * 0.2), 0.13, 0.25, [Mat::PetalYellow, Mat::HerbGreen, Mat::Canvas][rng.int(3)], None),
        2 => {
            kit::jug(b, c - along * 0.25, 0.3, Mat::Ceramic);
            kit::jug(b, c + along * 0.05, 0.22, Mat::Ceramic);
        }
        _ => {}
    }
    true
}

/// Колышки с кухонной утварью на стене: сковорода, черпак, сито, котелок.
pub fn utensil_rail(b: &mut Builder, p: Vec3, out: Vec3, rng: &mut Rng) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let p = p - out * 0.03;
    b.mesh(Mat::Wood).cuboid(p - side * 0.6 - Vec3::Y * 0.03, p + side * 0.6 + out * 0.04 + Vec3::Y * 0.03);
    for k in 0..4 {
        let q = p + side * (-0.45 + k as f32 * 0.3) + out * 0.04;
        b.mesh(Mat::Wood).beam(q, q + out * 0.08 + Vec3::Y * 0.02, 0.01, 0.01, Vec3::Y);
        match (k + rng.int(4)) % 4 {
            // Сковорода с длинной ручкой (висит на ручке).
            0 => {
                b.mesh(Mat::Iron).beam(q + out * 0.05, q + out * 0.05 - Vec3::Y * 0.25, 0.012, 0.006, out);
                let c = q + out * 0.06 - Vec3::Y * 0.4;
                b.mesh(Mat::Iron).cuboid(c - side * 0.13 - Vec3::Y * 0.13, c + side * 0.13 + Vec3::Y * 0.13 + out * 0.03);
            }
            // Черпак.
            1 => {
                b.mesh(Mat::Bronze).beam(q + out * 0.05, q + out * 0.05 - Vec3::Y * 0.3, 0.008, 0.005, out);
                b.mesh(Mat::Bronze).lathe(q + out * 0.08 - Vec3::Y * 0.4, &[(0.0, 0.0), (0.05, 0.02), (0.06, 0.07), (0.0, 0.06)], 8);
            }
            // Сито (обечайка с сеткой).
            2 => {
                let c = q + out * 0.05 - Vec3::Y * 0.2;
                b.mesh(Mat::Wood).cuboid(c - side * 0.14 - Vec3::Y * 0.14, c + side * 0.14 + Vec3::Y * 0.14 + out * 0.05);
                b.mesh(Mat::Canvas).cuboid(c - side * 0.12 - Vec3::Y * 0.12 + out * 0.05, c + side * 0.12 + Vec3::Y * 0.12 + out * 0.052);
            }
            // Медный котелок на дужке.
            _ => {
                b.mesh(Mat::Iron).beam(q + out * 0.06, q + out * 0.12 - Vec3::Y * 0.12, 0.004, 0.004, side);
                b.mesh(Mat::Bronze).lathe(q + out * 0.14 - Vec3::Y * 0.32, &[(0.0, 0.0), (0.08, 0.01), (0.11, 0.1), (0.1, 0.18), (0.09, 0.18)], 10);
            }
        }
    }
}

/// Связка вяленой рыбы или колбас на шесте под потолком.
pub fn hanging_fish(b: &mut Builder, a: Vec3, c: Vec3, n: usize) {
    b.mesh(Mat::Wood).beam(a, c, 0.025, 0.025, Vec3::Y);
    for k in 0..n {
        let q = a + (c - a) * ((k as f32 + 0.5) / n as f32);
        b.mesh(Mat::Canvas).beam(q, q - Vec3::Y * 0.12, 0.004, 0.004, Vec3::X);
        // Рыба головой вниз: веретено и хвост.
        let m = b.mesh(Mat::HerbSilver);
        m.lathe(q - Vec3::Y * 0.5, &[(0.0, 0.0), (0.025, 0.06), (0.035, 0.2), (0.02, 0.33), (0.0, 0.36)], 6);
        m.beam(q - Vec3::Y * 0.12, q - Vec3::Y * 0.16, 0.035, 0.004, Vec3::X);
    }
}

/// Поленница у очага: колотые дрова и растопка.
pub fn firewood(b: &mut Builder, f: Rect, rng: &mut Rng) {
    let along_x = f.w() > f.d();
    let len = if along_x { f.w() } else { f.d() };
    let dir = if along_x { Vec3::Z } else { Vec3::X };
    let m = b.mesh(Mat::Wood);
    let n = (len / 0.11) as usize;
    for row in 0..3 {
        for k in 0..n.saturating_sub(row) {
            let t = (k as f32 + 0.5 + row as f32 * 0.5) * 0.11;
            let base = if along_x { Vec3::new(f.x0 + t, 0.05 + row as f32 * 0.09, f.cz()) } else { Vec3::new(f.cx(), 0.05 + row as f32 * 0.09, f.z0 + t) };
            let d = (if along_x { f.d() } else { f.w() }) * 0.45 + rng.range(-0.04, 0.04);
            m.beam(base - dir * d, base + dir * d, 0.045, 0.045, Vec3::Y);
        }
    }
    kit::hidden(b, f.min3(0.0), f.max3(0.3));
}

/// Мортарий (широкая ступа для растирания) на полу.
pub fn mortarium(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.12, 0.0), (0.24, 0.13), (0.28, 0.14), (0.26, 0.16), (0.2, 0.12), (0.0, 0.04)], 14);
}

/// Колода для рубки с тесаком.
pub fn chopping_block(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Wood).lathe(p, &[(0.0, 0.0), (0.2, 0.0), (0.21, 0.5), (0.0, 0.52)], 10);
    b.mesh(Mat::Iron).beam(p + Vec3::new(-0.05, 0.53, 0.0), p + Vec3::new(0.1, 0.62, 0.0), 0.04, 0.005, Vec3::Z);
    b.mesh(Mat::Wood).beam(p + Vec3::new(0.1, 0.62, 0.0), p + Vec3::new(0.22, 0.7, 0.0), 0.012, 0.012, Vec3::Z);
    kit::hidden(b, p - Vec3::new(0.2, 0.0, 0.2), p + Vec3::new(0.2, 0.52, 0.2));
}

/// Амфоры на деревянной подставке-кольце, слегка наклонно у стены.
pub fn amphora_ring(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    b.mesh(Mat::Wood).lathe(p + Vec3::Y * 0.3, &[(0.17, 0.0), (0.2, 0.0), (0.2, 0.05), (0.17, 0.05)], 10);
    let m = b.mesh(Mat::Wood);
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        m.beam(p + Vec3::new(a.cos() * 0.22, 0.0, a.sin() * 0.22), p + Vec3::new(a.cos() * 0.18, 0.33, a.sin() * 0.18), 0.015, 0.015, Vec3::Y);
    }
    kit::amphora(b, p + Vec3::Y * 0.12, 0.85, rng.int(3) as u32);
}

/// Мотки пряжи на шесте у стены и прялка-веретено, прислонённая рядом.
pub fn skein_pole(b: &mut Builder, p: Vec3, out: Vec3, rng: &mut Rng) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let p = p - out * 0.03;
    b.mesh(Mat::Wood).beam(p - side * 0.6 + out * 0.12, p + side * 0.6 + out * 0.12, 0.018, 0.018, Vec3::Y);
    for s in [-0.6f32, 0.6] {
        b.mesh(Mat::Wood).beam(p + side * s, p + side * s + out * 0.14, 0.015, 0.015, Vec3::Y);
    }
    for k in 0..5 {
        let q = p + side * (-0.48 + k as f32 * 0.24) + out * 0.12;
        let mat = [Mat::Rug, Mat::Cloth, Mat::PetalWhite, Mat::Felt, Mat::PetalYellow, Mat::PetalPurple][rng.int(6)];
        b.mesh(mat).beam(q - Vec3::Y * 0.02, q - Vec3::Y * 0.42, 0.05, 0.025, out);
    }
}

/// Икона на стене: доска с золотым фоном и ликом (у зажиточных; в VI в. домашние иконы ещё редки).
pub fn wall_icon(b: &mut Builder, p: Vec3, out: Vec3) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let p = p - out * 0.03;
    b.mesh(Mat::Wood).cuboid(p - side * 0.17 - Vec3::Y * 0.22, p + side * 0.17 + Vec3::Y * 0.22 + out * 0.03);
    b.mesh(Mat::Gold).cuboid(p - side * 0.14 - Vec3::Y * 0.19 + out * 0.03, p + side * 0.14 + Vec3::Y * 0.19 + out * 0.034);
    // Фигура святого: одеяние и лик.
    b.mesh(Mat::Rug).cuboid(p - side * 0.08 - Vec3::Y * 0.19 + out * 0.034, p + side * 0.08 + Vec3::Y * 0.05 + out * 0.037);
    b.mesh(Mat::Bread).cuboid(p - side * 0.035 + Vec3::Y * 0.06 + out * 0.034, p + side * 0.035 + Vec3::Y * 0.13 + out * 0.037);
    // Лампадка на цепочке перед иконой.
    let lamp = p + out * 0.18 - Vec3::Y * 0.32;
    b.mesh(Mat::Bronze).beam(p + out * 0.04 + Vec3::Y * 0.25, lamp + Vec3::Y * 0.1, 0.003, 0.003, side);
    b.mesh(Mat::Glass).lathe(lamp, &[(0.0, 0.0), (0.02, 0.01), (0.04, 0.06), (0.045, 0.09), (0.04, 0.09)], 8);
}

/// Бронзовый светильник на настенном кронштейне (висит на цепочке).
pub fn bracket_lamp(b: &mut Builder, p: Vec3, out: Vec3) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let p = p - out * 0.03;
    let m = b.mesh(Mat::Iron);
    m.beam(p, p + out * 0.3, 0.012, 0.012, side);
    m.beam(p - Vec3::Y * 0.2, p + out * 0.2, 0.008, 0.008, side);
    let hang = p + out * 0.28;
    b.mesh(Mat::Bronze).beam(hang, hang - Vec3::Y * 0.18, 0.003, 0.003, side);
    let lamp = hang - Vec3::Y * 0.2;
    b.mesh(Mat::Bronze).lathe(lamp, &[(0.0, 0.0), (0.04, 0.0), (0.05, 0.02), (0.035, 0.035), (0.0, 0.035)], 8);
    b.mesh(Mat::Coals).lathe(lamp + out * 0.06 + Vec3::Y * 0.03, &[(0.0, 0.0), (0.008, 0.01), (0.0, 0.035)], 4);
}

/// Деревянная колыбель-люлька на полозьях.
pub fn cradle(b: &mut Builder, f: Rect) {
    let along_x = f.w() > f.d();
    let c = f.center(0.0);
    let (hx, hz) = if along_x { (0.4, 0.22) } else { (0.22, 0.4) };
    let m = b.mesh(Mat::Wood);
    for s in [-1.0f32, 1.0] {
        let o = if along_x { Vec3::X * (hx - 0.06) * s } else { Vec3::Z * (hz - 0.06) * s };
        let across = if along_x { Vec3::Z * (hz + 0.04) } else { Vec3::X * (hx + 0.04) };
        m.beam(c + o - across + Vec3::Y * 0.05, c + o + across + Vec3::Y * 0.05, 0.025, 0.05, Vec3::Y);
    }
    m.cuboid(c + Vec3::new(-hx, 0.1, -hz), c + Vec3::new(hx, 0.14, hz));
    m.cuboid(c + Vec3::new(-hx, 0.14, -hz), c + Vec3::new(hx, 0.4, -hz + 0.025));
    m.cuboid(c + Vec3::new(-hx, 0.14, hz - 0.025), c + Vec3::new(hx, 0.4, hz));
    m.cuboid(c + Vec3::new(-hx, 0.14, -hz), c + Vec3::new(-hx + 0.025, 0.46, hz));
    m.cuboid(c + Vec3::new(hx - 0.025, 0.14, -hz), c + Vec3::new(hx, 0.46, hz));
    b.mesh(Mat::PetalWhite).cuboid(c + Vec3::new(-hx + 0.03, 0.14, -hz + 0.03), c + Vec3::new(hx - 0.03, 0.26, hz - 0.03));
    kit::hidden(b, f.min3(0.0), f.max3(0.4));
}

/// Таз с кувшином для умывания на треногой подставке.
pub fn wash_stand(b: &mut Builder, p: Vec3, rich: bool) {
    let m = b.mesh(Mat::Wood);
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        m.beam(p + Vec3::new(a.cos() * 0.2, 0.0, a.sin() * 0.2), p + Vec3::new(a.cos() * 0.15, 0.75, a.sin() * 0.15), 0.015, 0.015, Vec3::Y);
    }
    let mat = if rich { Mat::Bronze } else { Mat::Ceramic };
    b.mesh(mat).lathe(p + Vec3::Y * 0.72, &[(0.0, 0.0), (0.1, 0.0), (0.2, 0.08), (0.22, 0.09), (0.2, 0.09)], 14);
    b.mesh(Mat::Water).disc(p + Vec3::Y * 0.78, 0.18, 10, true);
    kit::hidden(b, p - Vec3::new(0.22, 0.0, 0.22), p + Vec3::new(0.22, 0.8, 0.22));
}

/// Метла из прутьев, прислонённая к стене.
pub fn broom(b: &mut Builder, p: Vec3, out: Vec3) {
    let top = p - out * 0.02 + Vec3::Y * 1.3;
    let foot = p + out * 0.3;
    b.mesh(Mat::Wood).beam(foot + Vec3::Y * 0.3, top, 0.014, 0.014, out);
    b.mesh(Mat::Thatch).beam(foot, foot + (top - foot).normalize() * 0.35, 0.09, 0.03, Vec3::Y);
}

/// Глиняные горшки с зеленью (базилик, мята, рута) у стены двора.
pub fn herb_pots(b: &mut Builder, f: Rect, rng: &mut Rng) {
    let along_x = f.w() > f.d();
    let len = if along_x { f.w() } else { f.d() };
    let n = (len / 0.4) as usize;
    for k in 0..n {
        let t = (k as f32 + 0.5) / n as f32;
        let p = if along_x { Vec3::new(f.x0 + f.w() * t, 0.0, f.cz()) } else { Vec3::new(f.cx(), 0.0, f.z0 + f.d() * t) };
        let r = rng.range(0.1, 0.16);
        b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (r * 0.7, 0.0), (r, r * 1.6), (r * 1.1, r * 1.7), (r * 0.95, r * 1.7)], 10);
        let leaf = [Mat::HerbGreen, Mat::HerbGreen, Mat::HerbSilver][rng.int(3)];
        b.mesh(leaf).lathe(p + Vec3::Y * (r * 1.5), &[(0.0, 0.0), (r * 1.1, 0.04), (r * 1.2, r), (r * 0.6, r * 1.7), (0.0, r * 1.9)], 7);
    }
    kit::hidden(b, f.min3(0.0), f.max3(0.4));
}

/// Детский угол: лошадка, бабки, мяч (вещи — через [`put`], остальное — мешами).
pub fn toys(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    put(b, p, HORSE, 1, rng);
    // Мяч из кожи, набитый шерстью.
    b.mesh(Mat::Leather).lathe(p + Vec3::new(0.3, 0.0, 0.15), &[(0.0, 0.0), (0.05, 0.015), (0.065, 0.06), (0.05, 0.11), (0.0, 0.12)], 8);
    if rng.chance(0.6) {
        put(b, p + Vec3::new(-0.25, 0.0, 0.2), DICE, 1, rng);
    }
}

/// Плетёный короб-курятник у стены двора.
pub fn hen_coop(b: &mut Builder, f: Rect) {
    b.mesh(Mat::Thatch).cuboid(f.min3(0.0), f.max3(0.6));
    b.mesh(Mat::Wood).cuboid(f.min3(0.6), f.max3(0.65));
    kit::hidden(b, f.min3(0.0), f.max3(0.65));
}

/// Доска с инструментом на стене мастерской (по ремеслу — разные силуэты).
pub fn tool_board(b: &mut Builder, p: Vec3, out: Vec3, tools: Mat, rng: &mut Rng) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let p = p - out * 0.03;
    b.mesh(Mat::Wood).cuboid(p - side * 0.55 - Vec3::Y * 0.35, p + side * 0.55 + Vec3::Y * 0.35 + out * 0.025);
    for k in 0..6 {
        let q = p + side * (-0.45 + k as f32 * 0.18) + out * 0.03 + Vec3::Y * 0.2;
        let len = rng.range(0.2, 0.42);
        b.mesh(Mat::Wood).beam(q, q - Vec3::Y * len * 0.6, 0.012, 0.01, out);
        b.mesh(tools).beam(q - Vec3::Y * len * 0.6, q - Vec3::Y * len, 0.02 + rng.range(0.0, 0.02), 0.006, out);
    }
}

/// Вещи хозяев по достатку: бедным — глина, зажиточным — стекло, бронза и серебро.
pub fn table_wares(wealth: u8, rng: &mut Rng) -> Vec<Ware> {
    let mut w = vec![one(BOWL), one(CUP), one(BREAD)];
    if rng.chance(0.6) {
        w.push(one(CUP));
    }
    if rng.chance(0.5) {
        w.push(one(CHEESE));
    }
    if rng.chance(0.55) {
        w.push(one(FIGS));
    }
    if rng.chance(0.4) {
        w.push(one("Кувшин вина"));
    }
    if wealth >= 1 && rng.chance(0.5) {
        w.push(one(GOBLET));
    }
    if wealth >= 2 {
        w.push(one(EWER));
        w.push(Ware::Item(SPOON, 1 + rng.int(3) as u16));
        if rng.chance(0.5) {
            w.push(one("Стеклянная чаша"));
        }
    }
    if rng.chance(0.35) {
        w.push(one(DICE));
    }
    if rng.chance(0.45) {
        let coins = if wealth >= 2 && rng.chance(0.4) { ("Декануммий", 2 + rng.int(4) as u16) } else { ("Фоллис", 3 + rng.int(10) as u16) };
        w.push(Ware::Item(coins.0, coins.1));
    }
    w.push(one(LAMP));
    shuffle(&mut w, rng);
    w
}

pub fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        let j = rng.int(i + 1);
        v.swap(i, j);
    }
}
