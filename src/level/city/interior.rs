//! Обстановка комнат: мебель, утварь, мелочи, сундуки (с добычей) и жители.
//!
//! Что стояло в доме херсонесита VI в. (по раскопкам усадеб Херсонеса и византийских домов
//! Сирии и Малой Азии): ложа с тюфяками и шерстяными покрывалами, сундуки и стенные шкафы,
//! треногие табуреты, низкие столы, глиняные светильники на бронзовых подставках, пифосы
//! и амфоры в кладовых, очаг или переносная жаровня на кухне, ручные жернова, ткацкий
//! станок с грузилами, корзины. Иконы в домах ещё редки — в комнате висит деревянный крест.
//!
//! Раскладка простая: каждая вещь прижимается к стене ([`Room::take`]) и не встаёт
//! в проходы перед дверями (двери к этому времени уже построены и известны сборщику).

use super::cozy::{self, Ware, one};
use super::kit::{self, Rect, Rng, Side};
use super::*;
use crate::collision::Aabb;
use crate::items::LootTableId;
use crate::level::{DoorSpec, Role};

/// Комната, в которую расставляется обстановка.
pub struct Room {
    pub r: Rect,
    used: Vec<Rect>,
    pub(super) doors: Vec<DoorSpec>,
    /// Занятые места на стенах (что висит выше мебели).
    walls: Vec<Rect>,
}

impl Room {
    /// `r` — внутренние грани стен комнаты (пол на y = 0 в координатах сборщика).
    pub fn new(b: &Builder, r: Rect) -> Self {
        let near = r.inset(-1.6);
        let doors = b
            .doors
            .iter()
            .filter(|d| d.center.y.abs() < 1.0 && d.center.x > near.x0 && d.center.x < near.x1 && d.center.z > near.z0 && d.center.z < near.z1)
            .copied()
            .collect();
        // Что уже стоит в комнате (колонны, печи, столы): выше ступеньки и ниже головы.
        let used = b
            .blocks
            .iter()
            .filter(|k| k.solid)
            .map(|k| k.aabb)
            .chain(b.invisible.iter().copied())
            .filter(|a| a.max.y > 0.45 && a.min.y < 1.8 && a.min.x < r.x1 && a.max.x > r.x0 && a.min.z < r.z1 && a.max.z > r.z0)
            .map(|a| Rect::new(a.min.x, a.min.z, a.max.x, a.max.z))
            // Стены вокруг комнаты не считаем (комната задана по их граням).
            .filter(|f| !(f.w() > r.w() * 0.9 || f.d() > r.d() * 0.9))
            .chain(b.keep_clear.iter().filter(|p| p.y.abs() < 1.0).map(|p| Rect::new(p.x - 0.8, p.z - 0.8, p.x + 0.8, p.z + 0.8)))
            .collect();
        Room { r, used, doors, walls: Vec::new() }
    }

    /// Свободна ли площадка: внутри комнаты, не на занятом месте и не в проходе у двери.
    pub fn free(&self, f: Rect) -> bool {
        let r = self.r;
        if f.x0 < r.x0 - 0.01 || f.z0 < r.z0 - 0.01 || f.x1 > r.x1 + 0.01 || f.z1 > r.z1 + 0.01 {
            return false;
        }
        if self.used.iter().any(|u| overlaps(&u.inset(-0.05), &f)) {
            return false;
        }
        self.doors.iter().all(|d| !overlaps(&door_zone(d), &f))
    }

    /// Отметить место занятым (то, что поставлено мимо [`Room::take`]).
    pub fn reserve(&mut self, f: Rect) {
        self.used.push(f);
    }

    /// Площадка `len` (вдоль стены) × `depth` (от стены) у одной из стен `sides` (по порядку
    /// предпочтения). Возвращает площадку и стену, к которой она прижата.
    pub fn take(&mut self, len: f32, depth: f32, sides: &[Side], rng: &mut Rng) -> Option<(Rect, Side)> {
        for &side in sides {
            let r = self.r;
            let (s0, s1) = if side.along_x() { (r.x0, r.x1) } else { (r.z0, r.z1) };
            let mut options = Vec::new();
            let mut s = s0 + 0.05;
            while s + len <= s1 - 0.05 {
                let f = match side {
                    Side::N => Rect::new(s, r.z0, s + len, r.z0 + depth),
                    Side::S => Rect::new(s, r.z1 - depth, s + len, r.z1),
                    Side::W => Rect::new(r.x0, s, r.x0 + depth, s + len),
                    Side::E => Rect::new(r.x1 - depth, s, r.x1, s + len),
                };
                if self.free(f) {
                    options.push(f);
                }
                s += 0.2;
            }
            if !options.is_empty() {
                // Чаще — в углу (у края стены), иначе — где придётся.
                let f = if rng.chance(0.5) { options[0] } else if rng.chance(0.5) { *options.last().unwrap() } else { options[rng.int(options.len())] };
                self.used.push(f);
                return Some((f, side));
            }
        }
        None
    }

    /// Место на стене длиной `len` для того, что висит выше мебели (одежда, связки, светильник):
    /// не над дверью и не там, где уже что-то висит. Возвращает точку на грани стены (y = 0)
    /// и стену.
    pub fn wall_spot(&mut self, len: f32, sides: &[Side], rng: &mut Rng) -> Option<(Vec3, Side)> {
        let mut options = Vec::new();
        for &side in sides {
            let r = self.r;
            let (s0, s1) = if side.along_x() { (r.x0, r.x1) } else { (r.z0, r.z1) };
            let mut s = s0 + 0.15;
            while s + len <= s1 - 0.15 {
                let f = match side {
                    Side::N => Rect::new(s, r.z0, s + len, r.z0 + 0.15),
                    Side::S => Rect::new(s, r.z1 - 0.15, s + len, r.z1),
                    Side::W => Rect::new(r.x0, s, r.x0 + 0.15, s + len),
                    Side::E => Rect::new(r.x1 - 0.15, s, r.x1, s + len),
                };
                // Тонкие полосы у стен в `used` — то, что там уже висит (ковры, колышки, полки).
                let hung = self.used.iter().any(|u| u.w().min(u.d()) <= 0.2 && overlaps(&u.inset(-0.1), &f));
                if !hung && !self.walls.iter().any(|u| overlaps(&u.inset(-0.15), &f)) && self.doors.iter().all(|d| !overlaps(&door_zone(d), &f)) {
                    options.push((f, side));
                }
                s += 0.25;
            }
        }
        if options.is_empty() {
            return None;
        }
        let (f, side) = options[rng.int(options.len())];
        self.walls.push(f);
        let c = f.center(0.0);
        let at = match side {
            Side::N => Vec3::new(c.x, 0.0, self.r.z0),
            Side::S => Vec3::new(c.x, 0.0, self.r.z1),
            Side::W => Vec3::new(self.r.x0, 0.0, c.z),
            Side::E => Vec3::new(self.r.x1, 0.0, c.z),
        };
        Some((at, side))
    }

    /// Место для человека (0,6 × 0,6 м), ближайшее к `want`.
    pub fn spot(&self, want: Vec3) -> Option<Vec3> {
        let mut best: Option<(f32, Vec3)> = None;
        let r = self.r.inset(0.45);
        let mut x = r.x0;
        while x <= r.x1 {
            let mut z = r.z0;
            while z <= r.z1 {
                let f = Rect::new(x - 0.35, z - 0.35, x + 0.35, z + 0.35);
                if self.free(f) {
                    let p = Vec3::new(x, 0.0, z);
                    let d = p.distance_squared(Vec3::new(want.x, 0.0, want.z));
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, p));
                    }
                }
                z += 0.25;
            }
            x += 0.25;
        }
        best.map(|(_, p)| p)
    }

    /// Поставить жителя рядом с `want`, лицом к точке `look`.
    pub fn resident(&mut self, b: &mut Builder, want: Vec3, look: Vec3, role: Role) -> bool {
        let Some(p) = self.spot(want) else { return false };
        let d = look - p;
        let yaw = if d.length_squared() > 0.01 { (-d.x).atan2(-d.z) } else { 0.0 };
        b.resident(p, yaw, role);
        self.used.push(Rect::new(p.x - 0.3, p.z - 0.3, p.x + 0.3, p.z + 0.3));
        true
    }
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.z0 < b.z1 && a.z1 > b.z0
}

/// Проход у двери: проём и по метру с каждой стороны стены.
fn door_zone(d: &DoorSpec) -> Rect {
    let along = d.width / 2.0 + 0.3;
    let across = d.wall_t / 2.0 + 1.1;
    if d.along_x {
        Rect::new(d.center.x - along, d.center.z - across, d.center.x + along, d.center.z + across)
    } else {
        Rect::new(d.center.x - across, d.center.z - along, d.center.x + across, d.center.z + along)
    }
}

/// Направление от стены внутрь комнаты.
pub(super) fn inward(side: Side) -> Vec3 {
    -side.out()
}

// ---------------------------------------------------------------------------
// Мебель и утварь
// ---------------------------------------------------------------------------

/// Ложе с тюфяком, шерстяным покрывалом и валиком-подушкой.
pub fn bed(b: &mut Builder, f: Rect, rng: &mut Rng) {
    kit::bed(b, f);
    let along_x = f.w() > f.d();
    let cover = if rng.chance(0.5) { Mat::Rug } else { Mat::Cloth };
    let (c0, c1) = if along_x { (Rect::new(f.x0 + f.w() * 0.32, f.z0 + 0.02, f.x1 - 0.02, f.z1 - 0.02), Rect::new(f.x0 + 0.08, f.z0 + 0.12, f.x0 + 0.42, f.z1 - 0.12)) } else { (Rect::new(f.x0 + 0.02, f.z0 + f.d() * 0.32, f.x1 - 0.02, f.z1 - 0.02), Rect::new(f.x0 + 0.12, f.z0 + 0.08, f.x1 - 0.12, f.z0 + 0.42)) };
    b.mesh(cover).cuboid(c0.min3(0.6), c0.max3(0.66));
    // Свисающий край покрывала.
    if along_x {
        b.mesh(cover).cuboid(Vec3::new(c0.x0, 0.3, c0.z1), Vec3::new(c0.x1, 0.66, c0.z1 + 0.03));
    } else {
        b.mesh(cover).cuboid(Vec3::new(c0.x1, 0.3, c0.z0), Vec3::new(c0.x1 + 0.03, 0.66, c0.z1));
    }
    b.mesh(Mat::Canvas).cuboid(c1.min3(0.6), c1.max3(0.74));
}

/// Сундук у стены (обыскивается). `name` — подпись при обыске.
pub fn chest(b: &mut Builder, f: Rect, name: &'static str) {
    let along_x = f.w() > f.d();
    let c = f.center(0.0);
    kit::chest_body(b, c, along_x);
    b.container(c + Vec3::Y * 0.7, name);
}

/// Стенной шкаф (армарий) с двумя створками и посудой наверху; обыскивается.
pub fn cupboard(b: &mut Builder, f: Rect, side: Side, name: &'static str, rng: &mut Rng) {
    let h = 1.6;
    b.solid([f.x0, 0.0, f.z0], [f.x1, h, f.z1], Mat::Wood);
    let n = inward(side);
    let front = f.center(0.0) + n * (if side.along_x() { f.d() } else { f.w() } / 2.0 + 0.01);
    let along = if side.along_x() { Vec3::X } else { Vec3::Z };
    let len = if side.along_x() { f.w() } else { f.d() };
    let m = b.mesh(Mat::Wood);
    // Рамка створок и карниз.
    m.cuboid(front - along * (len / 2.0) + Vec3::Y * (h - 0.08), front + along * (len / 2.0) + n * 0.05 + Vec3::Y * (h + 0.04));
    m.cuboid(front - along * 0.02 + Vec3::Y * 0.1, front + along * 0.02 + n * 0.02 + Vec3::Y * (h - 0.1));
    let iron = b.mesh(Mat::Iron);
    for s in [-1.0f32, 1.0] {
        let k = front + along * s * 0.08 + Vec3::Y * 0.85;
        iron.cuboid(k - Vec3::splat(0.025), k + Vec3::splat(0.025) + n * 0.02);
    }
    let top = f.center(h);
    for k in 0..2 {
        if rng.chance(0.7) {
            kit::jug(b, top + along * (k as f32 - 0.5) * len * 0.5, rng.range(0.18, 0.3), Mat::Ceramic);
        }
    }
    b.container(f.center(0.0) + n * (if side.along_x() { f.d() } else { f.w() } / 2.0 + 0.25) + Vec3::Y * 1.0, name);
}

/// Плетёная корзина (может быть обыскиваемой).
pub fn basket(b: &mut Builder, p: Vec3, r: f32, h: f32, fill: Mat, name: Option<&'static str>) {
    b.mesh(Mat::Thatch).lathe(p, &[(0.0, 0.0), (r * 0.8, 0.0), (r, h * 0.8), (r * 1.05, h), (r * 0.95, h), (r * 0.85, h * 0.85)], 10);
    b.mesh(fill).lathe(p + Vec3::Y * (h * 0.8), &[(0.0, 0.0), (r * 0.85, 0.0), (r * 0.6, h * 0.2), (0.0, h * 0.25)], 8);
    kit::hidden(b, p - Vec3::new(r, 0.0, r), p + Vec3::new(r, h, r));
    if let Some(name) = name {
        b.container(p + Vec3::Y * (h + 0.15), name);
    }
}

/// Треногий табурет.
pub fn stool(b: &mut Builder, p: Vec3) {
    let m = b.mesh(Mat::Wood);
    m.lathe(p + Vec3::Y * 0.42, &[(0.0, 0.0), (0.18, 0.0), (0.18, 0.05), (0.0, 0.05)], 10);
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        let foot = p + Vec3::new(a.cos() * 0.17, 0.0, a.sin() * 0.17);
        m.beam(foot, p + Vec3::new(a.cos() * 0.1, 0.42, a.sin() * 0.1), 0.02, 0.02, Vec3::Y);
    }
}

/// Кресло со спинкой (у хозяина, учителя, командира). `back` — сторона, где спинка.
pub fn chair(b: &mut Builder, p: Vec3, back: Vec3) {
    let side = Vec3::new(back.z, 0.0, back.x);
    let m = b.mesh(Mat::Wood);
    m.cuboid(p - side * 0.25 - back * 0.22 + Vec3::Y * 0.44, p + side * 0.25 + back * 0.22 + Vec3::Y * 0.5);
    for (s, d) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let q = p + side * (s * 0.21) + back * (d * 0.18);
        m.cuboid(q - Vec3::new(0.025, 0.0, 0.025), q + Vec3::new(0.025, 0.44, 0.025));
    }
    let bb = p + back * 0.2;
    m.cuboid(bb - side * 0.25 + Vec3::Y * 0.5, bb + side * 0.25 + back * 0.04 + Vec3::Y * 1.05);
    b.mesh(Mat::Rug).cuboid(p - side * 0.22 - back * 0.2 + Vec3::Y * 0.5, p + side * 0.22 + back * 0.18 + Vec3::Y * 0.55);
    kit::hidden(b, p - Vec3::new(0.25, 0.0, 0.25), p + Vec3::new(0.25, 0.5, 0.25));
}

/// Бронзовая подставка с глиняным светильником; `light` — с настоящим светом.
pub fn lampstand(b: &mut Builder, p: Vec3, light: bool) {
    let m = b.mesh(Mat::Bronze);
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0 + 0.3;
        m.beam(p + Vec3::new(a.cos() * 0.16, 0.0, a.sin() * 0.16), p + Vec3::Y * 0.15, 0.012, 0.012, Vec3::Y);
    }
    m.lathe(p + Vec3::Y * 0.12, &[(0.02, 0.0), (0.018, 1.2), (0.09, 1.22), (0.09, 1.25), (0.0, 1.26)], 8);
    oil_lamp(b, p + Vec3::Y * 1.38);
    if light {
        b.lamp(p + Vec3::Y * 1.55, Color::srgb(1.0, 0.68, 0.38), 2_500.0, 5.5);
    }
}

/// Глиняный светильник с огоньком.
pub fn oil_lamp(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.05, 0.0), (0.06, 0.02), (0.05, 0.035), (0.0, 0.04)], 8);
    b.mesh(Mat::Ceramic).cuboid(p + Vec3::new(0.04, 0.0, -0.015), p + Vec3::new(0.09, 0.03, 0.015));
    b.mesh(Mat::Coals).lathe(p + Vec3::new(0.085, 0.03, 0.0), &[(0.0, 0.0), (0.01, 0.01), (0.0, 0.04)], 4);
}

/// Ковёр или циновка.
pub fn rug(b: &mut Builder, f: Rect, mat: Mat) {
    b.mesh(mat).cuboid(f.min3(0.0), f.max3(0.02));
    if mat == Mat::Rug {
        // Кайма.
        let i = f.inset(0.12);
        b.mesh(Mat::Cloth).cuboid(Vec3::new(i.x0, 0.0, i.z0), Vec3::new(i.x1, 0.022, i.z0 + 0.06));
        b.mesh(Mat::Cloth).cuboid(Vec3::new(i.x0, 0.0, i.z1 - 0.06), Vec3::new(i.x1, 0.022, i.z1));
    }
}

/// Деревянный крест на стене (на высоте `y`), `out` — от стены в комнату.
pub fn wall_cross(b: &mut Builder, p: Vec3, out: Vec3) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let m = b.mesh(Mat::Wood);
    m.cuboid(p - side * 0.03 + Vec3::Y * -0.3, p + side * 0.03 + out * 0.04 + Vec3::Y * 0.3);
    m.cuboid(p - side * 0.2 + Vec3::Y * 0.08, p + side * 0.2 + out * 0.04 + Vec3::Y * 0.14);
}

/// Колышки с плащом и сумкой на стене.
pub fn pegs(b: &mut Builder, p: Vec3, out: Vec3, mat: Mat) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    b.mesh(Mat::Wood).cuboid(p - side * 0.5 - Vec3::Y * 0.03, p + side * 0.5 + out * 0.08 + Vec3::Y * 0.03);
    let c = b.mesh(mat);
    c.cuboid(p - side * 0.35 + out * 0.03 - Vec3::Y * 1.0, p - side * 0.0 + out * 0.09);
    b.mesh(Mat::Felt).cuboid(p + side * 0.15 + out * 0.03 - Vec3::Y * 0.4, p + side * 0.4 + out * 0.14);
}

/// Связки чеснока, лука и сушёных трав под потолком.
pub fn hanging_bunches(b: &mut Builder, a: Vec3, c: Vec3, n: usize) {
    b.mesh(Mat::Wood).beam(a, c, 0.025, 0.025, Vec3::Y);
    for k in 0..n {
        let q = a + (c - a) * ((k as f32 + 0.5) / n as f32);
        let mat = [Mat::HerbGreen, Mat::Plaster, Mat::HerbSilver, Mat::PetalYellow][k % 4];
        b.mesh(Mat::Canvas).beam(q, q - Vec3::Y * 0.25, 0.006, 0.006, Vec3::X);
        b.mesh(mat).lathe(q - Vec3::Y * 0.55, &[(0.0, 0.0), (0.07, 0.08), (0.05, 0.25), (0.0, 0.3)], 6);
    }
}

/// Тренога с медным котлом над очагом.
pub fn tripod_cauldron(b: &mut Builder, p: Vec3) {
    let m = b.mesh(Mat::Iron);
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        m.beam(p + Vec3::new(a.cos() * 0.55, 0.0, a.sin() * 0.55), p + Vec3::Y * 1.1, 0.015, 0.015, Vec3::Y);
    }
    m.beam(p + Vec3::Y * 1.1, p + Vec3::Y * 0.75, 0.008, 0.008, Vec3::X);
    b.mesh(Mat::Bronze).lathe(p + Vec3::Y * 0.4, &[(0.0, 0.0), (0.2, 0.03), (0.26, 0.18), (0.24, 0.34), (0.22, 0.34), (0.22, 0.3)], 12);
}

/// Мешки с зерном, мукой или шерстью.
pub fn sacks(b: &mut Builder, f: Rect, rng: &mut Rng) {
    let along_x = f.w() > f.d();
    let n = ((if along_x { f.w() } else { f.d() }) / 0.55).floor().max(1.0) as usize;
    for k in 0..n {
        let t = (k as f32 + 0.5) / n as f32;
        let p = if along_x { Vec3::new(f.x0 + f.w() * t, 0.0, f.cz()) } else { Vec3::new(f.cx(), 0.0, f.z0 + f.d() * t) };
        let h = rng.range(0.55, 0.8);
        b.mesh(Mat::Canvas).lathe(p, &[(0.0, 0.0), (0.2, 0.0), (0.26, h * 0.4), (0.22, h * 0.85), (0.1, h), (0.0, h)], 8);
    }
    b.invisible.push(Aabb::new(f.min3(0.0), f.max3(0.7)));
}

/// Глиняная гидрия (вода) с черпаком.
pub fn hydria(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.12, 0.0), (0.22, 0.2), (0.24, 0.4), (0.16, 0.58), (0.09, 0.64), (0.11, 0.7), (0.0, 0.7)], 12);
    kit::hidden(b, p - Vec3::new(0.22, 0.0, 0.22), p + Vec3::new(0.22, 0.7, 0.22));
}

/// Посуда на столе: миски, кружки, хлеб, кувшин.
pub fn table_setting(b: &mut Builder, f: Rect, y: f32, rng: &mut Rng) {
    let along_x = f.w() > f.d();
    let len = if along_x { f.w() } else { f.d() };
    let n = (len / 0.45).floor().max(1.0) as usize;
    for k in 0..n {
        let t = (k as f32 + 0.5) / n as f32;
        let p = if along_x { Vec3::new(f.x0 + f.w() * t, y, f.cz() + if k % 2 == 0 { -0.12 } else { 0.12 }) } else { Vec3::new(f.cx() + if k % 2 == 0 { -0.12 } else { 0.12 }, y, f.z0 + f.d() * t) };
        match rng.int(4) {
            0 => b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.06, 0.0), (0.11, 0.04), (0.1, 0.045), (0.0, 0.02)], 10),
            1 => kit::jug(b, p, 0.12, Mat::Ceramic),
            2 => b.mesh(Mat::Thatch).lathe(p, &[(0.0, 0.0), (0.1, 0.0), (0.1, 0.04), (0.06, 0.08), (0.0, 0.09)], 8),
            _ => kit::jug(b, p, 0.24, Mat::Ceramic),
        }
    }
    oil_lamp(b, f.center(y));
}

/// Верстак: толстая доска на козлах, инструмент, стружка.
pub fn workbench(b: &mut Builder, f: Rect, tools: Mat) {
    kit::table(b, f, 0.85);
    let along_x = f.w() > f.d();
    let len = if along_x { f.w() } else { f.d() };
    let m = b.mesh(tools);
    for k in 0..4 {
        let t = (k as f32 + 0.5) / 4.0 * 0.8 + 0.1;
        let p = if along_x { Vec3::new(f.x0 + f.w() * t, 0.85, f.cz()) } else { Vec3::new(f.cx(), 0.85, f.z0 + f.d() * t) };
        let dir = if along_x { Vec3::Z } else { Vec3::X };
        m.beam(p - dir * 0.12, p + dir * 0.12, 0.015, 0.01, Vec3::Y);
    }
    let _ = len;
    b.mesh(Mat::Thatch).cuboid(f.center(0.0) - Vec3::new(0.3, 0.0, 0.3), f.center(0.0) + Vec3::new(0.3, 0.03, 0.3));
}

/// Пюпитр-стол с раскрытым кодексом, чернильницей и стилями.
pub fn desk_codex(b: &mut Builder, f: Rect) {
    kit::table(b, f, 0.8);
    let c = f.center(0.8);
    let m = b.mesh(Mat::Wood);
    m.cuboid(c + Vec3::new(-0.2, 0.0, -0.15), c + Vec3::new(0.2, 0.02, 0.15));
    b.mesh(Mat::Canvas).cuboid(c + Vec3::new(-0.18, 0.02, -0.13), c + Vec3::new(0.18, 0.045, 0.13));
    b.mesh(Mat::Bronze).lathe(c + Vec3::new(0.3, 0.0, 0.1), &[(0.0, 0.0), (0.035, 0.0), (0.035, 0.05), (0.01, 0.06)], 8);
    b.mesh(Mat::Wood).beam(c + Vec3::new(0.25, 0.01, -0.1), c + Vec3::new(0.4, 0.01, -0.05), 0.006, 0.006, Vec3::Y);
}

/// Стойка с копьями и щитом.
pub fn weapon_rack(b: &mut Builder, f: Rect, side: Side) {
    let n = inward(side);
    let along = if side.along_x() { Vec3::X } else { Vec3::Z };
    let len = if side.along_x() { f.w() } else { f.d() };
    let c = f.center(0.0);
    let m = b.mesh(Mat::Wood);
    m.beam(c - along * len * 0.45 + Vec3::Y * 1.2, c + along * len * 0.45 + Vec3::Y * 1.2, 0.03, 0.03, Vec3::Y);
    m.beam(c - along * len * 0.45 + Vec3::Y * 0.2, c + along * len * 0.45 + Vec3::Y * 0.2, 0.03, 0.03, Vec3::Y);
    let k = (len / 0.25) as usize;
    for i in 0..k {
        let q = c - along * len * 0.4 + along * (i as f32 * 0.25);
        b.mesh(Mat::Wood).beam(q, q + Vec3::Y * 2.4 - n * 0.1, 0.015, 0.015, Vec3::X);
        b.mesh(Mat::Iron).spire(q + Vec3::Y * 2.4 - n * 0.1, 0.035, 0.22, 4);
    }
    kit::hidden(b, f.min3(0.0), f.max3(2.4));
}

/// Гончарный круг (ножной).
pub fn potter_wheel(b: &mut Builder, p: Vec3) {
    let m = b.mesh(Mat::Wood);
    m.lathe(p, &[(0.0, 0.0), (0.4, 0.0), (0.4, 0.08), (0.0, 0.08)], 14);
    m.beam(p + Vec3::Y * 0.08, p + Vec3::Y * 0.7, 0.03, 0.03, Vec3::X);
    m.lathe(p + Vec3::Y * 0.7, &[(0.0, 0.0), (0.22, 0.0), (0.22, 0.04), (0.0, 0.04)], 12);
    b.mesh(Mat::Ceramic).lathe(p + Vec3::Y * 0.74, &[(0.0, 0.0), (0.1, 0.0), (0.12, 0.12), (0.08, 0.22), (0.09, 0.25)], 10);
    kit::hidden(b, p - Vec3::new(0.4, 0.0, 0.4), p + Vec3::new(0.4, 0.8, 0.4));
}

/// Жаровня (переносной очаг из меди) с углями.
pub fn brazier(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Bronze).lathe(p, &[(0.08, 0.0), (0.05, 0.35), (0.28, 0.45), (0.3, 0.55), (0.0, 0.5)], 12);
    b.mesh(Mat::Coals).disc(p + Vec3::Y * 0.54, 0.26, 10, true);
    kit::hidden(b, p - Vec3::new(0.28, 0.0, 0.28), p + Vec3::new(0.28, 0.55, 0.28));
}

/// Бельевая верёвка через двор с простынями и туникой.
pub fn washing_line(b: &mut Builder, a: Vec3, c: Vec3, rng: &mut Rng) {
    let m = b.mesh(Mat::Wood);
    for q in [a, c] {
        m.beam(q, q + Vec3::Y * 2.2, 0.04, 0.04, Vec3::X);
    }
    b.mesh(Mat::Canvas).beam(a + Vec3::Y * 2.1, c + Vec3::Y * 2.1, 0.006, 0.006, Vec3::Y);
    let len = a.distance(c);
    let dir = (c - a) / len;
    let mut s = 0.4;
    while s < len - 0.8 {
        let w = rng.range(0.5, 1.1);
        let p0 = a + dir * s + Vec3::Y * 2.1;
        let p1 = p0 + dir * w;
        let mat = [Mat::Canvas, Mat::Cloth, Mat::Canvas, Mat::Rug][rng.int(4)];
        let drop = rng.range(0.6, 1.1);
        let n = Vec3::new(-dir.z, 0.0, dir.x);
        b.mesh(mat).quad([p0, p1, p1 - Vec3::Y * drop, p0 - Vec3::Y * drop], n);
        b.mesh(mat).quad([p1, p0, p0 - Vec3::Y * drop, p1 - Vec3::Y * drop], -n);
        s += w + 0.3;
    }
}

/// Деревянная лохань (стирка, мытьё).
pub fn tub(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Wood).lathe(p, &[(0.0, 0.0), (0.35, 0.0), (0.42, 0.4), (0.38, 0.4), (0.32, 0.06)], 12);
    b.mesh(Mat::Water).disc(p + Vec3::Y * 0.3, 0.38, 10, true);
    kit::hidden(b, p - Vec3::new(0.4, 0.0, 0.4), p + Vec3::new(0.4, 0.4, 0.4));
}

// ---------------------------------------------------------------------------
// Комнаты
// ---------------------------------------------------------------------------

/// Чем занята комната.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Kitchen,
    Bedroom,
    Storeroom,
    Weaving,
    /// Главная комната: стол, кресло хозяина, шкаф.
    Living,
    Workshop(Trade),
    /// Спальня наверху: несколько лож, сундуки.
    Loft,
}

/// Домашнее ремесло (мастерская в доме).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trade {
    Cobbler,
    Carpenter,
    Potter,
    Netmaker,
    Metalworker,
    Dyer,
}

impl Trade {
    pub fn pick(rng: &mut Rng) -> Self {
        [Trade::Cobbler, Trade::Carpenter, Trade::Potter, Trade::Netmaker, Trade::Metalworker, Trade::Dyer][rng.int(6)]
    }
}

/// Кто в доме и где (заполняет [`furnish`]).
pub struct Household {
    /// Поставлено ли уже хозяйке, хозяину, ремесленнику.
    pub wife: bool,
    pub master: bool,
    pub child: bool,
    pub craft: bool,
}

impl Household {
    pub fn new() -> Self {
        Household { wife: false, master: false, child: false, craft: false }
    }
    /// Никого не ставить (горница, служебные помещения).
    pub fn full() -> Self {
        Household { wife: true, master: true, child: true, craft: true }
    }
}

/// Обставить комнату по назначению. Ставит жителя, если `people` ещё нужен такой.
pub fn furnish(b: &mut Builder, r: Rect, kind: Kind, people: &mut Household, rng: &mut Rng) {
    let r = r.inset(0.05);
    if r.w() < 1.7 || r.d() < 1.7 {
        return;
    }
    let mut room = Room::new(b, r);
    let long = if r.w() > r.d() { [Side::N, Side::S, Side::W, Side::E] } else { [Side::W, Side::E, Side::N, Side::S] };
    let short = if r.w() > r.d() { [Side::W, Side::E, Side::N, Side::S] } else { [Side::N, Side::S, Side::W, Side::E] };
    let all = [Side::N, Side::E, Side::S, Side::W];
    let c = r.center(0.0);
    let wealth = b.wealth;
    match kind {
        Kind::Kitchen => {
            b.loot(LootTableId::Kitchen);
            // Очаг у стены, над ним — тренога с котлом, рядом — поленница.
            if let Some((f, s)) = room.take(1.3, 1.3, &short, rng) {
                kit::hearth(b, f.center(0.0));
                if rng.chance(0.6) {
                    tripod_cauldron(b, f.center(0.0));
                } else {
                    // Горшки греются на краю очага.
                    cozy::lidded_pot(b, f.center(0.2) + axis(s) * 0.35);
                    kit::jug(b, f.center(0.2) - axis(s) * 0.38, 0.2, Mat::Ceramic);
                }
                b.lamp(f.center(0.6), Color::srgb(1.0, 0.5, 0.2), 3_000.0, 5.0);
                let wood = f.center(0.0) + axis(s) * 1.05;
                let wf = Rect::new(wood.x - 0.3, wood.z - 0.3, wood.x + 0.3, wood.z + 0.3);
                if room.free(wf) {
                    room.reserve(wf);
                    cozy::firewood(b, wf, rng);
                }
            }
            if let Some((f, s)) = room.take(1.6, 0.35, &long, rng) {
                let mut rr = Rng::new(rng.int(9999) as u32);
                kit::shelf(b, f, 2, &mut rr, Mat::Ceramic);
                hanging_bunches(b, f.center(2.5) + inward(s) * 0.25 - axis(s) * 0.7, f.center(2.5) + inward(s) * 0.25 + axis(s) * 0.7, 5);
            }
            // Полка с посудой и припасами (вещи хозяев).
            let herb = ["Сушёный чабрец", "Сушёная мята", "Сушёный шалфей"][rng.int(3)];
            let mut upper = vec![Ware::Bowls, one(cozy::CUP), Ware::Jug(0.22), one(cozy::LAMP), one(herb)];
            let mut lower = vec![one("Глиняный горшок"), Ware::Pot, one(cozy::BOWL)];
            if rng.chance(0.5) {
                lower.push(one("Горшок мёда"));
            }
            if rng.chance(0.4) {
                upper.push(one("Кувшин оливкового масла"));
            }
            cozy::shuffle(&mut upper, rng);
            cozy::shelf_unit(b, &mut room, &long, 1.4, &upper, &lower, rng);
            // Колышки с утварью.
            if let Some((f, s)) = room.take(1.3, 0.12, &all, rng) {
                cozy::utensil_rail(b, f.center(1.75) - inward(s) * 0.06, inward(s), rng);
            }
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                kit::hand_mill(b, f.center(0.0));
            }
            if let Some((f, _)) = room.take(0.5, 0.5, &all, rng) {
                hydria(b, f.center(0.0));
            }
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                basket(b, f.center(0.0), 0.25, 0.4, Mat::HerbGreen, Some("Корзина с припасами"));
            }
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                cozy::amphora_ring(b, f.center(0.0), rng);
            }
            // Рабочий стол: нож, хлеб, рыба, ступка.
            if let Some((f, _)) = room.take(1.2, 0.55, &all, rng) {
                kit::table(b, f, 0.75);
                let mut w = vec![one(cozy::KNIFE), one(cozy::BREAD), one(cozy::BOWL), Ware::Fruit([Mat::PetalYellow, Mat::PetalRed, Mat::HerbGreen][rng.int(3)])];
                if rng.chance(0.6) {
                    w.push(one("Солёная рыба"));
                }
                if rng.chance(0.4) {
                    w.push(one("Ступка с пестиком"));
                }
                if rng.chance(0.3) {
                    w.push(one(cozy::CHEESE));
                }
                cozy::shuffle(&mut w, rng);
                cozy::lay(b, f.inset(0.05), 0.75, &w, rng);
            }
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                cozy::mortarium(b, f.center(0.0));
            }
            if rng.chance(0.6)
                && let Some((f, _)) = room.take(0.45, 0.45, &all, rng)
            {
                cozy::chopping_block(b, f.center(0.0));
            }
            if r.w().max(r.d()) > 3.4 {
                let s = long[rng.int(2)];
                let p = wall_point(r, s, 2.45) + inward(s) * 0.35;
                cozy::hanging_fish(b, p - axis(s) * 0.6, p + axis(s) * 0.6, 4);
            }
            if !people.wife {
                people.wife = room.resident(b, c, c + Vec3::X, if rng.chance(0.3) { Role::Cook } else { Role::Housewife });
            }
        }
        Kind::Bedroom => {
            b.loot(LootTableId::Household);
            let mut beds = Vec::new();
            if let Some((f, s)) = room.take(2.0, 0.95, &long, rng) {
                bed(b, f, rng);
                beds.push((f, s));
            }
            if r.w().max(r.d()) > 4.2
                && let Some((f, s)) = room.take(2.0, 0.95, &long, rng)
            {
                bed(b, f, rng);
                beds.push((f, s));
            }
            // Сандалии у ложа и ночной горшок.
            for (f, s) in &beds {
                let edge = f.center(0.0) + inward(*s) * ((if s.along_x() { f.d() } else { f.w() }) / 2.0 + 0.2);
                if rng.chance(0.7) {
                    cozy::sandals(b, edge + axis(*s) * 0.3, inward(*s));
                }
                if rng.chance(0.35) {
                    cozy::chamber_pot(b, f.center(0.0) - axis(*s) * ((if s.along_x() { f.w() } else { f.d() }) / 2.0 - 0.25));
                }
            }
            if let Some((f, _)) = room.take(1.0, 0.6, &all, rng) {
                chest(b, f, "Сундук");
                let along_x = f.w() > f.d();
                if rng.chance(0.5) {
                    cozy::folded_blankets(b, f.center(0.55), along_x, rng);
                } else {
                    let mut w = vec![one(cozy::COMB), one(cozy::SPINDLE)];
                    if rng.chance(0.4) {
                        w.push(one(cozy::CASKET));
                    }
                    cozy::lay(b, f.inset(0.08), 0.55, &w, rng);
                }
            }
            // Столик у ложа: светильник, гребень, зеркальце, украшения.
            if let Some((f, _)) = room.take(0.55, 0.42, &all, rng) {
                kit::table(b, f, 0.55);
                let mut w = vec![one(cozy::LAMP), one(cozy::COMB)];
                if wealth >= 1 && rng.chance(0.55) {
                    w.push(one(cozy::MIRROR));
                }
                if rng.chance(0.3) {
                    w.push(one(cozy::AMPULLA));
                }
                if wealth >= 2 && rng.chance(0.5) {
                    w.push(one(["Серебряный перстень", "Бронзовая фибула", cozy::ENKOLPION][rng.int(3)]));
                } else if rng.chance(0.2) {
                    w.push(one(["Нательный крест", "Бронзовая фибула", "Бронзовая пряжка"][rng.int(3)]));
                }
                cozy::lay(b, f.inset(0.03), 0.55, &w, rng);
            }
            if rng.chance(0.45)
                && let Some((f, _)) = room.take(0.5, 0.5, &all, rng)
            {
                cozy::wash_stand(b, f.center(0.0), wealth >= 2);
            }
            if let Some((f, _)) = room.take(0.4, 0.4, &all, rng) {
                lampstand(b, f.center(0.0), false);
            }
            if let Some((f, _)) = room.take(0.4, 0.4, &all, rng) {
                stool(b, f.center(0.0));
            }
            if let Some((f, s)) = room.take(1.0, 0.15, &all, rng) {
                pegs(b, f.center(1.9) - inward(s) * 0.07, inward(s), if rng.chance(0.5) { Mat::Cloth } else { Mat::Felt });
            }
            if rng.chance(0.45) {
                let mut upper = vec![Ware::Jug(0.2), one(cozy::AMPULLA)];
                if rng.chance(0.5) {
                    upper.push(one(cozy::CASKET));
                }
                if rng.chance(0.4) {
                    upper.push(one(cozy::YARN));
                }
                cozy::shelf_unit(b, &mut room, &all, 1.0, &upper, &[], rng);
            }
            // Завеса над ложем.
            if let Some((f, s)) = beds.first().copied().filter(|_| rng.chance(0.55)) {
                let p = f.center(1.55) - inward(s) * ((if s.along_x() { f.d() } else { f.w() }) / 2.0 - 0.02);
                cozy::wall_hanging(b, p, inward(s), 1.3, 0.9, rng);
            } else if let Some(s) = all.get(rng.int(4)) {
                let p = wall_point(r, *s, 2.2);
                wall_cross(b, p, inward(*s));
            }
            let cr = r.inset(0.9);
            if cr.w() > 0.8 && cr.d() > 0.8 {
                rug(b, cr, if rng.chance(0.5) { Mat::Rug } else { Mat::Thatch });
            }
            // Детский угол: лошадка, кости, мяч; у самых маленьких — люлька.
            if rng.chance(0.45)
                && let Some((f, _)) = room.take(0.7, 0.6, &all, rng)
            {
                cozy::toys(b, f.center(0.0) - Vec3::new(0.15, 0.0, 0.1), rng);
            }
            if rng.chance(0.15)
                && let Some((f, _)) = room.take(0.9, 0.55, &all, rng)
            {
                cozy::cradle(b, f);
            }
            if !people.child && rng.chance(0.25) {
                people.child = room.resident(b, c, c + Vec3::Z, Role::Child);
            }
        }
        Kind::Storeroom => {
            b.loot(LootTableId::Kitchen);
            for _ in 0..3 {
                if let Some((f, _)) = room.take(0.9, 0.9, &all, rng) {
                    kit::pithos(b, f.center(0.0), rng.range(1.0, 1.3), 0.2);
                }
            }
            if let Some((f, _)) = room.take(1.8, 0.6, &long, rng) {
                sacks(b, f, rng);
            }
            if let Some((f, _)) = room.take(1.4, 0.5, &all, rng) {
                for k in 0..3 {
                    kit::amphora(b, f.center(0.0) + axis_of(&f) * (k as f32 - 1.0) * 0.45, 1.0, rng.int(3) as u32);
                }
            }
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                basket(b, f.center(0.0), 0.28, 0.45, Mat::PetalYellow, Some("Корзина"));
            }
            if let Some((f, _)) = room.take(1.0, 0.6, &all, rng) {
                b.loot(LootTableId::Storehouse);
                chest(b, f, "Ларь");
            }
            // Помост с припасами: мёд, воск, масло, рыба, соль.
            if let Some((f, _)) = room.take(1.5, 0.45, &all, rng) {
                kit::bench(b, f);
                let mut w = vec![one(["Кувшин вина", "Кувшин оливкового масла"][rng.int(2)]), one("Солёная рыба"), one(cozy::FIGS)];
                if rng.chance(0.5) {
                    w.push(one("Горшок мёда"));
                }
                if rng.chance(0.4) {
                    w.push(one("Воск"));
                }
                if rng.chance(0.35) {
                    w.push(one("Мешок соли"));
                }
                cozy::shuffle(&mut w, rng);
                cozy::lay(b, f.inset(0.03), 0.45, &w, rng);
            }
            let s = long[rng.int(2)];
            let p = wall_point(r, s, 2.4) + inward(s) * 0.4;
            if rng.chance(0.5) {
                cozy::hanging_fish(b, p - axis(s) * 0.7, p + axis(s) * 0.7, 5);
            } else {
                hanging_bunches(b, p - axis(s) * 0.7, p + axis(s) * 0.7, 5);
            }
        }
        Kind::Weaving => {
            b.loot(LootTableId::Household);
            if let Some((f, s)) = room.take(1.9, 0.6, &long, rng) {
                kit::loom(b, f.center(0.0) - inward(s) * 0.2, s.along_x());
                let p = f.center(0.0) + inward(s) * 1.0;
                if room.free(Rect::new(p.x - 0.25, p.z - 0.25, p.x + 0.25, p.z + 0.25)) {
                    stool(b, p);
                }
            }
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                basket(b, f.center(0.0), 0.28, 0.4, Mat::Felt, Some("Корзина с шерстью"));
            }
            if let Some((f, _)) = room.take(0.5, 0.5, &all, rng) {
                basket(b, f.center(0.0), 0.2, 0.3, Mat::Canvas, None);
            }
            if let Some((f, _)) = room.take(1.0, 0.6, &all, rng) {
                chest(b, f, "Сундук с тканями");
            }
            // Скамья с пряжей, веретёнами и ножницами.
            if let Some((f, _)) = room.take(1.3, 0.4, &all, rng) {
                kit::bench(b, f);
                let mut w = vec![one(cozy::YARN), one(cozy::SPINDLE), one(cozy::YARN), one(cozy::SHEARS)];
                if rng.chance(0.5) {
                    w.push(one(cozy::SPINDLE));
                }
                if rng.chance(0.3) {
                    w.push(one(["Отрез шерсти", "Отрез льна"][rng.int(2)]));
                }
                cozy::shuffle(&mut w, rng);
                cozy::lay(b, f.inset(0.03), 0.45, &w, rng);
            }
            if let Some((f, s)) = room.take(1.3, 0.15, &all, rng) {
                cozy::skein_pole(b, f.center(1.8) - inward(s) * 0.07, inward(s), rng);
            }
            if !people.wife || rng.chance(0.4) {
                people.wife |= room.resident(b, c, c - Vec3::X, Role::Weaver);
            }
        }
        Kind::Living => {
            b.loot(LootTableId::Household);
            cozy::painted_socle(b, &room, if wealth >= 2 { 1.1 } else { 0.85 });
            if let Some((f, s)) = room.take(1.0, 0.55, &long, rng) {
                cupboard(b, f, s, "Шкаф", rng);
            }
            // Стол посередине, если комната позволяет, иначе — у стены.
            let t = Rect::new(c.x - 0.7, c.z - 0.4, c.x + 0.7, c.z + 0.4);
            let t = if r.w() < r.d() { Rect::new(c.x - 0.4, c.z - 0.7, c.x + 0.4, c.z + 0.7) } else { t };
            let table = if room.free(t.inset(-0.6)) {
                room.reserve(t.inset(-0.3));
                Some(t)
            } else {
                room.take(1.2, 0.6, &all, rng).map(|(f, _)| f)
            };
            if let Some(t) = table {
                kit::table(b, t, 0.72);
                // Льняная скатерть и всё, что на столе.
                let cloth = t.inset(0.1);
                b.mesh(if rng.chance(0.6) { Mat::PetalWhite } else { Mat::Canvas }).cuboid(cloth.min3(0.72), cloth.max3(0.725));
                let w = cozy::table_wares(wealth, rng);
                cozy::lay(b, t.inset(0.06), 0.725, &w, rng);
                for (k, s) in [Side::N, Side::S, Side::W, Side::E].into_iter().enumerate() {
                    let p = t.center(0.0) + s.out() * (if s.along_x() { t.d() } else { t.w() } / 2.0 + 0.35);
                    let foot = Rect::new(p.x - 0.2, p.z - 0.2, p.x + 0.2, p.z + 0.2);
                    if room.free(foot) && (k < 2 || rng.chance(0.5)) {
                        if k == 0 { chair(b, p, s.out()) } else { stool(b, p) }
                        room.reserve(foot);
                    }
                }
            }
            if let Some((f, _)) = room.take(0.4, 0.4, &all, rng) {
                lampstand(b, f.center(0.0), rng.chance(0.5));
            }
            if let Some((f, _)) = room.take(1.4, 0.4, &long, rng) {
                kit::bench(b, f);
                cozy::cushions(b, f.inset(0.04), 0.45, rng);
            }
            // Полка: светильник, ампула, шкатулка, посуда.
            let mut upper = vec![Ware::Jug(0.24), one(cozy::AMPULLA), Ware::Bowls];
            if wealth >= 1 {
                upper.push(one(cozy::BRONZE_LAMP));
            }
            if rng.chance(0.45) {
                upper.push(one(cozy::CASKET));
            }
            if wealth >= 2 && rng.chance(0.6) {
                upper.push(one("Стеклянная чаша"));
            }
            let mut lower = vec![one(cozy::BOWL), one(cozy::CUP), Ware::Jug(0.3), one(cozy::LAMP)];
            if rng.chance(0.35) {
                lower.push(one("Кувшин вина"));
            }
            cozy::shuffle(&mut upper, rng);
            cozy::shuffle(&mut lower, rng);
            cozy::shelf_unit(b, &mut room, &all, 1.4, &upper, &lower, rng);
            // Столик с доской для тавлей.
            if rng.chance(0.4)
                && let Some((f, _)) = room.take(0.7, 0.6, &all, rng)
            {
                kit::table(b, f, 0.6);
                cozy::lay(b, f.inset(0.04), 0.6, &[one(cozy::TABULA), one(cozy::DICE)], rng);
            }
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                brazier(b, f.center(0.0));
            }
            // Завеса на стене, крест, у зажиточных — икона с лампадкой.
            if let Some((f, s)) = room.take(1.4, 0.1, &all, rng) {
                cozy::wall_hanging(b, f.center(1.7) - inward(s) * 0.05, inward(s), 1.3, 1.1, rng);
            }
            let s = all[rng.int(4)];
            if wealth >= 2 && rng.chance(0.6) {
                cozy::wall_icon(b, wall_point(r, s, 1.9), inward(s));
            } else {
                wall_cross(b, wall_point(r, s, 2.3), inward(s));
            }
            if rng.chance(0.5) {
                let s = all[(rng.int(4) + 1) % 4];
                cozy::bracket_lamp(b, wall_point(r, s, 2.0) + axis(s) * 0.9, inward(s));
            }
            if !people.master {
                people.master = room.resident(b, c + Vec3::new(0.6, 0.0, 0.6), c, Role::Householder);
            }
        }
        Kind::Workshop(trade) => {
            b.loot(LootTableId::Workshop);
            let tools = if matches!(trade, Trade::Metalworker) { Mat::Iron } else { Mat::Bronze };
            let mut bench_at = None;
            if let Some((f, s)) = room.take(1.8, 0.7, &long, rng) {
                workbench(b, f, tools);
                bench_at = Some((f, s));
            }
            // Что сделано и чем работают: настоящие вещи на полке мастерской.
            let goods: Vec<Ware> = match trade {
                Trade::Cobbler => vec![one("Кожаные сапоги"), one(cozy::SHEARS), one("Кожаные сапоги"), one("Сырая шкура")],
                Trade::Carpenter => vec![one("Плотничий топор"), one("Ясеневая заготовка"), one(cozy::HORSE), one(cozy::CASKET)],
                Trade::Potter => vec![one(cozy::BOWL), one(cozy::LAMP), one(cozy::CUP), one(cozy::BOWL), one(cozy::LAMP), one("Глиняный горшок")],
                Trade::Netmaker => vec![Ware::Item("Свинцовые грузила", 6), one("Отрез льна"), Ware::Item("Свинцовые грузила", 4)],
                Trade::Metalworker => vec![one("Бронзовая пряжка"), one("Бронзовая фибула"), one("Бронзовый лом"), one("Медный лом"), one(cozy::BRONZE_LAMP)],
                Trade::Dyer => vec![one("Корень марены"), one("Вайда"), one("Квасцы"), one(cozy::YARN), one(cozy::YARN)],
            };
            let (upper, lower) = goods.split_at(goods.len() / 2);
            cozy::shelf_unit(b, &mut room, &all, 1.5, upper, lower, rng);
            if let Some((f, s)) = room.take(1.2, 0.12, &all, rng) {
                cozy::tool_board(b, f.center(1.6) - inward(s) * 0.06, inward(s), tools, rng);
            }
            match trade {
                Trade::Cobbler => {
                    if let Some((f, _)) = room.take(1.2, 0.35, &all, rng) {
                        // Колодки и готовая обувь на полке.
                        let mut rr = Rng::new(rng.int(999) as u32);
                        kit::shelf(b, f, 2, &mut rr, Mat::Leather);
                    }
                    if let Some((f, _)) = room.take(0.9, 0.9, &all, rng) {
                        b.mesh(Mat::Leather).cuboid(f.min3(0.0), f.max3(0.25));
                        kit::hidden(b, f.min3(0.0), f.max3(0.25));
                    }
                }
                Trade::Carpenter => {
                    if let Some((f, _)) = room.take(1.6, 0.5, &long, rng) {
                        kit::woodpile(b, f, 0.6);
                    }
                    if let Some((f, _)) = room.take(0.5, 0.5, &all, rng) {
                        // Чурбак для рубки с топором.
                        b.solid([f.x0 + 0.05, 0.0, f.z0 + 0.05], [f.x1 - 0.05, 0.5, f.z1 - 0.05], Mat::Wood);
                        b.mesh(Mat::Iron).beam(f.center(0.5), f.center(0.85) + Vec3::X * 0.1, 0.02, 0.06, Vec3::Z);
                    }
                }
                Trade::Potter => {
                    if let Some((f, _)) = room.take(0.9, 0.9, &all, rng) {
                        potter_wheel(b, f.center(0.0));
                    }
                    if let Some((f, _)) = room.take(1.4, 0.35, &all, rng) {
                        let mut rr = Rng::new(rng.int(999) as u32);
                        kit::shelf(b, f, 3, &mut rr, Mat::Ceramic);
                    }
                }
                Trade::Netmaker => {
                    if let Some((f, s)) = room.take(1.8, 0.2, &all, rng) {
                        let a = f.center(0.3) - axis(s) * 0.8;
                        let m = b.mesh(Mat::Canvas);
                        for k in 0..6 {
                            m.beam(a + axis(s) * (k as f32 * 0.32), a + axis(s) * (k as f32 * 0.32) + Vec3::Y * 1.6, 0.004, 0.004, inward(s));
                        }
                        for k in 0..6 {
                            let y = 0.3 + k as f32 * 0.27;
                            m.beam(a + Vec3::Y * y, a + axis(s) * 1.6 + Vec3::Y * y, 0.004, 0.004, Vec3::Y);
                        }
                    }
                    if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                        basket(b, f.center(0.0), 0.28, 0.35, Mat::Canvas, None);
                    }
                }
                Trade::Metalworker => {
                    if let Some((f, _)) = room.take(0.8, 0.8, &all, rng) {
                        brazier(b, f.center(0.0));
                        b.lamp(f.center(0.8), Color::srgb(1.0, 0.45, 0.15), 2_500.0, 4.0);
                    }
                    if let Some((f, _)) = room.take(0.5, 0.5, &all, rng) {
                        b.solid([f.x0 + 0.05, 0.0, f.z0 + 0.05], [f.x1 - 0.05, 0.55, f.z1 - 0.05], Mat::Wood);
                        b.mesh(Mat::Iron).cuboid(f.center(0.55) - Vec3::new(0.15, 0.0, 0.07), f.center(0.55) + Vec3::new(0.15, 0.12, 0.07));
                    }
                }
                Trade::Dyer => {
                    for mat in [Mat::Rug, Mat::Cloth, Mat::PetalYellow] {
                        if let Some((f, _)) = room.take(0.8, 0.8, &all, rng) {
                            let p = f.center(0.0);
                            b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.3, 0.0), (0.36, 0.55), (0.32, 0.58)], 12);
                            b.mesh(mat).disc(p + Vec3::Y * 0.5, 0.31, 10, true);
                            kit::hidden(b, p - Vec3::new(0.35, 0.0, 0.35), p + Vec3::new(0.35, 0.6, 0.35));
                        }
                    }
                    if let Some((f, s)) = room.take(2.0, 0.1, &all, rng) {
                        let a = f.center(2.3) + inward(s) * 0.5 - axis(s) * 1.0;
                        washing_line(b, a, a + axis(s) * 2.0, rng);
                    }
                }
            }
            if let Some((f, _)) = room.take(0.9, 0.55, &all, rng) {
                chest(b, f, "Ящик с инструментом");
            }
            if let Some((f, _)) = room.take(0.4, 0.4, &all, rng) {
                stool(b, f.center(0.0));
            }
            if rng.chance(0.5)
                && let Some((f, s)) = room.take(0.4, 0.4, &all, rng)
            {
                cozy::broom(b, f.center(0.0) - inward(s) * 0.15, inward(s));
            }
            if !people.craft {
                let (want, look) = match bench_at {
                    Some((f, s)) => (f.center(0.0) + inward(s) * 0.9, f.center(0.0)),
                    None => (c, c + Vec3::X),
                };
                people.craft = room.resident(b, want, look, Role::Craftsman);
            }
        }
        Kind::Loft => {
            b.loot(LootTableId::Household);
            let n = ((r.w().max(r.d())) / 3.2) as usize;
            for _ in 0..n.max(1) {
                if let Some((f, s)) = room.take(2.0, 0.95, &long, rng) {
                    bed(b, f, rng);
                    if rng.chance(0.5) {
                        let edge = f.center(0.0) + inward(s) * ((if s.along_x() { f.d() } else { f.w() }) / 2.0 + 0.2);
                        cozy::sandals(b, edge + axis(s) * 0.3, inward(s));
                    }
                }
                if rng.chance(0.6)
                    && let Some((f, _)) = room.take(1.0, 0.6, &long, rng)
                {
                    chest(b, f, "Сундук");
                    if rng.chance(0.5) {
                        cozy::folded_blankets(b, f.center(0.55), f.w() > f.d(), rng);
                    } else {
                        let w = [one(cozy::COMB), one([cozy::SPINDLE, cozy::LAMP, cozy::MIRROR][rng.int(3)])];
                        cozy::lay(b, f.inset(0.08), 0.55, &w, rng);
                    }
                }
            }
            if let Some((f, s)) = room.take(1.0, 0.55, &all, rng) {
                cupboard(b, f, s, "Шкаф", rng);
            }
            if rng.chance(0.5)
                && let Some((f, s)) = room.take(1.9, 0.6, &long, rng)
            {
                kit::loom(b, f.center(0.0) - inward(s) * 0.2, s.along_x());
                if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                    basket(b, f.center(0.0), 0.25, 0.35, Mat::Felt, Some("Корзина с пряжей"));
                }
            }
            for _ in 0..2 {
                if let Some((f, _)) = room.take(0.4, 0.4, &all, rng) {
                    stool(b, f.center(0.0));
                }
            }
            if let Some((f, s)) = room.take(1.0, 0.15, &all, rng) {
                pegs(b, f.center(1.9) - inward(s) * 0.07, inward(s), Mat::Cloth);
            }
            if rng.chance(0.5)
                && let Some((f, s)) = room.take(1.3, 0.1, &all, rng)
            {
                cozy::wall_hanging(b, f.center(1.6) - inward(s) * 0.05, inward(s), 1.2, 0.9, rng);
            }
            if rng.chance(0.4)
                && let Some((f, _)) = room.take(0.7, 0.6, &all, rng)
            {
                cozy::toys(b, f.center(0.0) - Vec3::new(0.15, 0.0, 0.1), rng);
            }
            let k = (r.w().max(r.d()) / 4.0) as usize;
            for i in 0..k {
                let t = (i as f32 + 0.5) / k as f32;
                let p = if r.w() > r.d() { Vec3::new(r.x0 + r.w() * t, 0.0, r.cz()) } else { Vec3::new(r.cx(), 0.0, r.z0 + r.d() * t) };
                let f = Rect::new(p.x - 0.7, p.z - 0.5, p.x + 0.7, p.z + 0.5);
                if room.free(f) && rng.chance(0.6) {
                    rug(b, f, Mat::Rug);
                }
            }
        }
    }
    // Второй проход: крупное у стен и на стенах, чтобы с порога комната выглядела жилой.
    cozy::dress(b, &mut room, kind, wealth, rng);
}

/// Ось вдоль стены `s`.
pub(super) fn axis(s: Side) -> Vec3 {
    if s.along_x() { Vec3::X } else { Vec3::Z }
}

fn axis_of(f: &Rect) -> Vec3 {
    if f.w() > f.d() { Vec3::X } else { Vec3::Z }
}

/// Точка на стене комнаты (посередине стороны `s`) на высоте `y`.
fn wall_point(r: Rect, s: Side, y: f32) -> Vec3 {
    match s {
        Side::N => Vec3::new(r.cx() + 0.4, y, r.z0),
        Side::S => Vec3::new(r.cx() - 0.4, y, r.z1),
        Side::W => Vec3::new(r.x0, y, r.cz() + 0.4),
        Side::E => Vec3::new(r.x1, y, r.cz() - 0.4),
    }
}

/// Обставить лавку: прилавок уже стоит; добавить товар, мешки, корзины, весы и лавочника.
pub fn shop_extras(b: &mut Builder, r: Rect, taken: &[Rect], keeper: bool, rng: &mut Rng) {
    let mut room = Room::new(b, r.inset(0.05));
    for t in taken {
        room.reserve(t.inset(-0.1));
    }
    let counter = taken[0];
    let all = [Side::N, Side::E, Side::S, Side::W];
    b.loot(LootTableId::Storehouse);
    if let Some((f, _)) = room.take(1.2, 0.5, &all, rng) {
        sacks(b, f, rng);
    }
    if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
        basket(b, f.center(0.0), 0.26, 0.4, [Mat::PetalYellow, Mat::HerbGreen, Mat::Ceramic][rng.int(3)], Some("Корзина с товаром"));
    }
    if let Some((f, _)) = room.take(1.0, 0.6, &all, rng) {
        chest(b, f, "Ларь лавочника");
    }
    // Безмен на прилавке.
    let p = counter.center(1.0);
    b.mesh(Mat::Bronze).beam(p + Vec3::new(-0.25, 0.25, 0.0), p + Vec3::new(0.25, 0.27, 0.0), 0.01, 0.01, Vec3::Y);
    b.mesh(Mat::Bronze).lathe(p + Vec3::new(0.22, 0.02, 0.0), &[(0.0, 0.0), (0.07, 0.0), (0.07, 0.02)], 8);
    if keeper {
        let behind = r.center(0.0);
        room.resident(b, behind, counter.center(0.0), Role::Shopkeeper);
    }
}

/// Обитатели и сундуки общественного здания или мастерской: `chests` — подписи сундуков
/// (добыча — по `table`), `people` — кто стоит и где примерно (точка), лицом к `look`.
pub fn extras(b: &mut Builder, r: Rect, table: LootTableId, chests: &[&'static str], people: &[(Role, Vec3, Vec3)], rng: &mut Rng) {
    let mut room = Room::new(b, r);
    b.loot(table);
    let all = [Side::N, Side::E, Side::S, Side::W];
    for name in chests {
        if name.starts_with("Шкаф") {
            if let Some((f, s)) = room.take(1.0, 0.55, &all, rng) {
                cupboard(b, f, s, name, rng);
            }
        } else if name.starts_with("Корзина") {
            if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
                basket(b, f.center(0.0), 0.26, 0.4, Mat::Canvas, Some(name));
            }
        } else if let Some((f, _)) = room.take(1.0, 0.6, &all, rng) {
            chest(b, f, name);
        }
    }
    for &(role, want, look) in people {
        room.resident(b, want, look, role);
    }
}

/// Двор усадьбы: бельё, лохань, скамья, корзины; иногда ребёнок или хозяин во дворе.
pub fn yard_extras(b: &mut Builder, r: Rect, stair: Option<Rect>, people: &mut Household, rng: &mut Rng) {
    if r.w() < 4.0 || r.d() < 4.0 {
        return;
    }
    let mut room = Room::new(b, r);
    if let Some(s) = stair {
        room.reserve(s);
    }
    // Уже стоящее во дворе (цистерна, пифосы, дерево) — грубо: середина двора занята.
    room.reserve(Rect::new(r.cx() - 1.6, r.cz() - 1.6, r.cx() + 1.6, r.cz() + 1.6));
    let all = [Side::N, Side::E, Side::S, Side::W];
    if rng.chance(0.55)
        && let Some((f, s)) = room.take(3.2, 0.6, &all, rng)
    {
        let a = f.center(0.0) + inward(s) * 0.3 - axis(s) * 1.5;
        washing_line(b, a, a + axis(s) * 3.0, rng);
    }
    if rng.chance(0.6)
        && let Some((f, _)) = room.take(0.9, 0.9, &all, rng)
    {
        tub(b, f.center(0.0));
    }
    if let Some((f, _)) = room.take(1.4, 0.4, &all, rng) {
        kit::bench(b, f);
    }
    if let Some((f, _)) = room.take(0.6, 0.6, &all, rng) {
        basket(b, f.center(0.0), 0.25, 0.35, Mat::Thatch, None);
    }
    // Горшки с зеленью, поленница, метла, курятник — двор, в котором живут.
    if rng.chance(0.7)
        && let Some((f, _)) = room.take(1.6, 0.4, &all, rng)
    {
        cozy::herb_pots(b, f, rng);
    }
    if rng.chance(0.6)
        && let Some((f, _)) = room.take(1.4, 0.5, &all, rng)
    {
        cozy::firewood(b, f, rng);
    }
    if rng.chance(0.4)
        && let Some((f, _)) = room.take(1.0, 0.7, &all, rng)
    {
        cozy::hen_coop(b, f);
    }
    if let Some((f, s)) = room.take(0.4, 0.4, &all, rng) {
        cozy::broom(b, f.center(0.0) - inward(s) * 0.15, inward(s));
    }
    if !people.child && rng.chance(0.35) {
        let c = r.center(0.0) + Vec3::new(2.2, 0.0, 0.0);
        people.child = room.resident(b, c, r.center(0.0), Role::Child);
    }
}
