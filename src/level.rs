//! Уровни: общий сборщик (коробки-коллизии, декоративные меши, свет), выбор уровня и его показ.
//!
//! Оси: X, Z — горизонталь, Y — вверх. 1 единица = 1 метр.
//! Уровень по умолчанию — Херсонес около 510 г. (`city`). Старый готический замок (`castle`)
//! включается переменной окружения `MEDIEVAL_LEVEL=castle` (одинаково на сервере и у клиентов).
//!
//! Коллизии — простые коробки (AABB) плюс карта высот холмов. Сложные декоративные детали
//! (арки, своды, крыши, колонны) рисуются отдельными мешами и в физике не участвуют.

// Построение геометрии естественно принимает много чисел (границы, высоты, толщины).
#![allow(clippy::too_many_arguments)]

mod castle;
mod city;
mod geom;
pub mod places;
mod terrain;

pub use places::*;

use crate::collision::{Aabb, HeightField, LevelColliders};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
pub use geom::MeshBuilder;
use geom::{WallFrame, arch_height};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mat {
    Grass,
    Cobble,
    /// Светлая тёсаная кладка.
    Stone,
    /// Тёмная кладка (башни, собор).
    DarkStone,
    /// Каменные плиты пола.
    Floor,
    Wood,
    Banner,
    Gold,
    /// Резной камень: колонны, обрамления, нервюры.
    Trim,
    /// Сланцевая черепица.
    Roof,
    Iron,
    /// Тлеющие угли горна.
    Coals,
    Water,
    /// Известняковые плиты мостовой.
    Paving,
    Leaves,
    // ---- Херсонес ----
    /// Местный известняк, крупные тёсаные блоки (крепостные стены, общественные здания).
    Limestone,
    /// Бутовая кладка жилых домов.
    Rubble,
    /// Побелённая штукатурка.
    Plaster,
    /// Керамическая черепица (тегулы и калиптеры).
    Tile,
    /// Плинфа (тонкий кирпич): печи, своды терм.
    Brick,
    /// Смешанная кладка: камень с поясами плинфы.
    OpusMixtum,
    /// Проконнесский мрамор: колонны, капители, алтарная преграда.
    Marble,
    /// Мозаичный пол.
    Mosaic,
    /// Утоптанная земля со щебнем: улицы, дворы.
    Earth,
    /// Рельеф за стенами: сухая трава, почва, известняковые выходы.
    Terrain,
    /// Розовая гидравлическая штукатурка с толчёной керамикой (цемянка): цистерны, бассейны.
    Signinum,
    /// Небелёный лён: навесы, паруса.
    Canvas,
    /// Войлок кочевых юрт.
    Felt,
    /// Шерсть, крашенная мареной: ковры, покрывала.
    Rug,
    /// Шерсть, крашенная вайдой: покрывала, занавеси.
    Cloth,
    /// Соломенная/камышовая кровля.
    Thatch,
    /// Обожжённая глина: амфоры, пифосы, горшки.
    Ceramic,
    Bronze,
    /// Хвоя можжевельника, кипариса, сосны.
    Conifer,
    HerbGreen,
    HerbSilver,
    PetalYellow,
    PetalPurple,
    PetalWhite,
    PetalRed,
    PetalPink,
}

#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub aabb: Aabb,
    pub mat: Mat,
    pub solid: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Torch {
    /// Положение пламени.
    pub pos: Vec3,
    /// Направление от стены (куда смотрит факел). Ноль — факел на столбе.
    pub out: Vec3,
}

/// Дополнительный источник света (люстры, горн).
#[derive(Clone, Copy, Debug)]
pub struct Lamp {
    pub pos: Vec3,
    pub color: Color,
    pub intensity: f32,
    pub range: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlassKind {
    Lancet,
    Rose,
}

/// Витраж: плоский многоугольник с UV 0..1, виден с обеих сторон.
#[derive(Clone, Debug)]
pub struct GlassPane {
    pub kind: GlassKind,
    pub pts: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub normal: Vec3,
}

/// Дверь в проёме стены. Номер двери — её индекс в [`Layout::doors`] (одинаков у сервера и клиентов,
/// потому что уровень строится одинаково).
#[derive(Clone, Copy, Debug)]
pub struct DoorSpec {
    /// Середина порога (по толщине стены).
    pub center: Vec3,
    /// Стена идёт вдоль X (иначе — вдоль Z).
    pub along_x: bool,
    pub width: f32,
    pub height: f32,
    /// Толщина стены: створка открывается внутрь проёма и прижимается к откосу.
    pub wall_t: f32,
    /// Двустворчатая (ворота, ставни лавки).
    pub double: bool,
    /// Открыта при запуске сервера.
    pub open: bool,
}

impl DoorSpec {
    /// Коробка закрытой створки (по середине толщины стены).
    pub fn closed_box(&self) -> Aabb {
        let (hw, ht) = (self.width / 2.0, 0.07);
        let (dx, dz) = if self.along_x { (hw, ht) } else { (ht, hw) };
        Aabb::new(self.center - Vec3::new(dx, 0.0, dz), self.center + Vec3::new(dx, self.height, dz))
    }
}

/// Сундук, шкаф или корзина, которые можно обыскать (содержимое бросает сервер).
#[derive(Clone, Copy, Debug)]
pub struct ContainerSpec {
    pub pos: Vec3,
    pub name: &'static str,
    pub table: crate::items::LootTableId,
}

/// Кто живёт или работает в помещении (рисуется клиентом, стоит на месте).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Householder,
    Housewife,
    Servant,
    Child,
    Weaver,
    Cook,
    Craftsman,
    Shopkeeper,
    Priest,
    Monk,
    Physician,
    Patient,
    Scholar,
    Student,
    Soldier,
    Officer,
    Bather,
    Sailor,
    Noble,
    Farmer,
}

#[derive(Clone, Copy, Debug)]
pub struct Resident {
    pub pos: Vec3,
    pub yaw: f32,
    pub role: Role,
}

pub struct Layout {
    pub doors: Vec<DoorSpec>,
    pub containers: Vec<ContainerSpec>,
    pub residents: Vec<Resident>,
    pub blocks: Vec<Block>,
    pub torches: Vec<Torch>,
    pub lamps: Vec<Lamp>,
    pub glass: Vec<GlassPane>,
    /// Декоративная геометрия по материалам и участкам карты (без коллизий).
    pub decor: HashMap<(Mat, Chunk), MeshBuilder>,
    /// Невидимые преграды (стёкла окон и т. п.).
    pub invisible: Vec<Aabb>,
    /// Карта высот земли (если есть) и уровень воды.
    pub ground: Option<HeightField>,
    pub water_y: Option<f32>,
    /// Очаги (для варки отваров).
    #[allow(dead_code)] // читает `places::hearths` для алхимии
    pub hearths: Vec<Vec3>,
    /// Кузнечные и сыродутные горны (для кузнечного дела).
    pub forges: Vec<ForgeSpec>,
}

/// Рабочее место кузнеца: где огонь и где стоять у наковальни.
#[derive(Clone, Copy, Debug)]
pub struct ForgeSpec {
    pub name: &'static str,
    pub kind: crate::smithing_net::StationKind,
    /// Середина углей (свет и жар).
    pub fire: Vec3,
    /// Наковальня (у сыродутного горна — место у мехов), отсюда работают.
    pub anvil: Vec3,
}

/// Участок карты 64×64 м: меши режутся по участкам, чтобы невидимое не рисовалось.
pub type Chunk = (i16, i16);
pub const CHUNK: f32 = 64.0;

pub fn chunk_of(p: Vec3) -> Chunk {
    ((p.x / CHUNK).floor() as i16, (p.z / CHUNK).floor() as i16)
}

/// Проём в стене: дверь (sill = низ стены) или окно (sill выше, со стеклом).
#[derive(Clone, Copy, Debug)]
struct Opening {
    s: f32,
    width: f32,
    sill: f32,
    spring: f32,
    rise: f32,
    glass: bool,
}

impl Opening {
    fn door(s: f32, width: f32, spring: f32, rise: f32) -> Self {
        Self { s, width, sill: f32::NEG_INFINITY, spring, rise, glass: false }
    }
    fn window(s: f32, width: f32, sill: f32, spring: f32, rise: f32) -> Self {
        Self { s, width, sill, spring, rise, glass: true }
    }
    fn apex(&self) -> f32 {
        self.spring + arch_height(0.0, self.width, self.rise)
    }
}

struct Builder {
    doors: Vec<DoorSpec>,
    containers: Vec<ContainerSpec>,
    residents: Vec<Resident>,
    /// Какую добычу кладут в сундуки, которые сейчас ставятся (см. [`Builder::loot`]).
    loot: (crate::items::LootTableId, &'static str),
    /// Точки, где стоят торговцы, учителя и стража: обстановка их не занимает.
    keep_clear: Vec<Vec3>,
    blocks: Vec<Block>,
    torches: Vec<Torch>,
    lamps: Vec<Lamp>,
    glass: Vec<GlassPane>,
    decor: HashMap<(Mat, Chunk), MeshBuilder>,
    invisible: Vec<Aabb>,
    ground: Option<HeightField>,
    water_y: Option<f32>,
    hearths: Vec<Vec3>,
    forges: Vec<ForgeSpec>,
    /// Где земля карты высот вырезана (свои полы из коробок).
    holes: Vec<(Vec2, Vec2)>,
    /// Участок, куда сейчас складываются декоративные меши (см. [`Builder::at`]).
    chunk: Chunk,
}

impl Builder {
    fn new() -> Self {
        Self {
            doors: Vec::new(),
            containers: Vec::new(),
            residents: Vec::new(),
            loot: (crate::items::LootTableId::Household, "Сундук"),
            keep_clear: Vec::new(),
            blocks: Vec::new(),
            torches: Vec::new(),
            lamps: Vec::new(),
            glass: Vec::new(),
            decor: HashMap::new(),
            invisible: Vec::new(),
            ground: None,
            water_y: None,
            hearths: Vec::new(),
            forges: Vec::new(),
            holes: Vec::new(),
            chunk: (0, 0),
        }
    }

    fn finish(self) -> Layout {
        Layout {
            doors: self.doors,
            containers: self.containers,
            residents: self.residents,
            blocks: self.blocks,
            torches: self.torches,
            lamps: self.lamps,
            glass: self.glass,
            decor: self.decor,
            invisible: self.invisible,
            ground: self.ground,
            water_y: self.water_y,
            hearths: self.hearths,
            forges: self.forges,
        }
    }

    /// Строит `f` в локальных координатах (пол на y = 0) и переносит всё построенное на `offset`:
    /// так постройки за стенами ставятся на террасу на склоне.
    fn placed(&mut self, offset: Vec3, f: impl FnOnce(&mut Builder)) {
        self.placed_in(offset, chunk_of(offset), f);
    }

    /// Как [`Builder::placed`], но декор идёт в участок `chunk` (для подъёма на этаж выше).
    fn placed_in(&mut self, offset: Vec3, chunk: Chunk, f: impl FnOnce(&mut Builder)) {
        let mut t = Builder::new();
        t.loot = self.loot;
        f(&mut t);
        let shift = |a: Aabb| Aabb::new(a.min + offset, a.max + offset);
        self.blocks.extend(t.blocks.into_iter().map(|b| Block { aabb: shift(b.aabb), ..b }));
        self.invisible.extend(t.invisible.into_iter().map(shift));
        self.torches.extend(t.torches.into_iter().map(|x| Torch { pos: x.pos + offset, ..x }));
        self.lamps.extend(t.lamps.into_iter().map(|x| Lamp { pos: x.pos + offset, ..x }));
        self.hearths.extend(t.hearths.into_iter().map(|p| p + offset));
        self.forges.extend(t.forges.into_iter().map(|f| ForgeSpec { fire: f.fire + offset, anvil: f.anvil + offset, ..f }));
        self.doors.extend(t.doors.into_iter().map(|d| DoorSpec { center: d.center + offset, ..d }));
        self.containers.extend(t.containers.into_iter().map(|c| ContainerSpec { pos: c.pos + offset, ..c }));
        self.residents.extend(t.residents.into_iter().map(|r| Resident { pos: r.pos + offset, ..r }));
        for mut g in t.glass {
            g.pts.iter_mut().for_each(|p| *p += offset);
            self.glass.push(g);
        }
        for ((mat, _), mut mb) in t.decor {
            mb.translate(offset);
            self.decor.entry((mat, chunk)).or_default().append(mb);
        }
    }

    /// Что лежит в сундуках, которые ставятся дальше: таблица добычи и подпись по умолчанию.
    fn loot(&mut self, table: crate::items::LootTableId) {
        self.loot.0 = table;
    }

    fn door(&mut self, d: DoorSpec) {
        self.doors.push(d);
    }

    /// Обыскиваемое место (сундук, шкаф, корзина) по текущей таблице добычи.
    fn container(&mut self, pos: Vec3, name: &'static str) {
        self.containers.push(ContainerSpec { pos, name, table: self.loot.0 });
    }

    /// Житель стоит в точке `pos` лицом по `yaw` (тело — невидимая коробка, чтобы сквозь него не ходили).
    fn resident(&mut self, pos: Vec3, yaw: f32, role: Role) {
        self.residents.push(Resident { pos, yaw, role });
        self.invisible.push(Aabb::new(pos - Vec3::new(0.25, 0.0, 0.25), pos + Vec3::new(0.25, 1.75, 0.25)));
    }

    /// Дальнейшие декоративные меши относятся к участку карты вокруг точки (x, z).
    fn at(&mut self, x: f32, z: f32) {
        self.chunk = chunk_of(Vec3::new(x, 0.0, z));
    }

    fn solid(&mut self, min: [f32; 3], max: [f32; 3], mat: Mat) {
        self.blocks.push(Block { aabb: Aabb::new(Vec3::from(min), Vec3::from(max)), mat, solid: true });
    }
    fn solid_v(&mut self, (min, max): (Vec3, Vec3), mat: Mat) {
        if (max - min).min_element() > 0.001 {
            self.blocks.push(Block { aabb: Aabb::new(min, max), mat, solid: true });
        }
    }
    fn decor(&mut self, min: [f32; 3], max: [f32; 3], mat: Mat) {
        self.blocks.push(Block { aabb: Aabb::new(Vec3::from(min), Vec3::from(max)), mat, solid: false });
    }
    fn mesh(&mut self, mat: Mat) -> &mut MeshBuilder {
        self.decor.entry((mat, self.chunk)).or_default()
    }
    fn invisible(&mut self, (min, max): (Vec3, Vec3)) {
        self.invisible.push(Aabb::new(min, max));
    }
    fn torch(&mut self, x: f32, y: f32, z: f32, out: Vec3) {
        self.torches.push(Torch { pos: Vec3::new(x, y, z), out });
    }
    fn lamp(&mut self, pos: Vec3, color: Color, intensity: f32, range: f32) {
        self.lamps.push(Lamp { pos, color, intensity, range });
    }

    /// Зубцы вдоль X на высоте y.
    fn merlons_x(&mut self, x0: f32, x1: f32, y: f32, z0: f32, z1: f32) {
        let mut x = x0;
        while x + 1.0 <= x1 + 0.01 {
            self.solid([x, y, z0], [x + 1.0, y + 1.5, z1], Mat::Stone);
            x += 2.0;
        }
    }
    fn merlons_z(&mut self, z0: f32, z1: f32, y: f32, x0: f32, x1: f32) {
        let mut z = z0;
        while z + 1.0 <= z1 + 0.01 {
            self.solid([x0, y, z], [x1, y + 1.5, z + 1.0], Mat::Stone);
            z += 2.0;
        }
    }

    /// Лестница из ступенек 0.4 м вдоль X (от x_start в сторону dir).
    fn stairs_x(&mut self, x_start: f32, dir: f32, z0: f32, z1: f32, count: usize) {
        for i in 0..count {
            let top = 0.4 * (i as f32 + 1.0);
            let a = x_start + dir * 0.6 * i as f32;
            let b = x_start + dir * 0.6 * (i as f32 + 1.0);
            self.solid([a.min(b), 0.0, z0], [a.max(b), top, z1], Mat::Stone);
        }
    }

    fn crate_at(&mut self, x: f32, z: f32, size: f32) {
        self.solid([x - size / 2.0, 0.0, z - size / 2.0], [x + size / 2.0, size, z + size / 2.0], Mat::Wood);
        // Окованные рёбра.
        let m = self.mesh(Mat::Iron);
        let h = size / 2.0 + 0.01;
        m.cuboid(Vec3::new(x - h, size * 0.15, z - h), Vec3::new(x + h, size * 0.22, z + h));
        m.cuboid(Vec3::new(x - h, size * 0.78, z - h), Vec3::new(x + h, size * 0.85, z + h));
    }

    fn barrel_at(&mut self, x: f32, z: f32) {
        self.solid([x - 0.4, 0.0, z - 0.4], [x + 0.4, 1.1, z + 0.4], Mat::Wood);
        let m = self.mesh(Mat::Iron);
        for y in [0.15, 0.85] {
            m.prism(Vec3::new(x, y, z), 0.47, 0.08, 8);
        }
    }

    /// Стена с проёмами. `f` задаёт ориентацию; s — вдоль стены, d — толщина.
    fn wall(&mut self, f: WallFrame, s0: f32, s1: f32, y0: f32, y1: f32, d0: f32, d1: f32, openings: &[Opening], mat: Mat) {
        let mut ops: Vec<Opening> = openings.iter().copied().filter(|o| o.s > s0 && o.s < s1).collect();
        ops.sort_by(|a, b| a.s.total_cmp(&b.s));
        let mut cursor = s0;
        let dm = (d0 + d1) / 2.0;
        for o in ops {
            let (l, r) = (o.s - o.width / 2.0, o.s + o.width / 2.0);
            self.solid_v(f.bounds((cursor, l), (y0, y1), (d0, d1)), mat);
            let apex = o.apex();
            let sill = o.sill.max(y0);
            if sill > y0 {
                self.solid_v(f.bounds((l, r), (y0, sill), (d0, d1)), mat);
                // Подоконник.
                let (a, b) = f.bounds((l - 0.15, r + 0.15), (sill - 0.12, sill), (d0 - 0.12, d1 + 0.12));
                self.mesh(Mat::Trim).cuboid(a, b);
            }
            if apex < y1 {
                self.solid_v(f.bounds((l, r), (apex, y1), (d0, d1)), mat);
            }
            // Коллизия в зоне арки: по краям проёма.
            let q = o.width / 4.0;
            let band_bottom = o.spring + arch_height(q, o.width, o.rise);
            self.solid_v(f.bounds((l, l + q * 0.6), (band_bottom, apex.min(y1)), (d0, d1)), mat);
            self.solid_v(f.bounds((r - q * 0.6, r), (band_bottom, apex.min(y1)), (d0, d1)), mat);
            {
                let mut infill = std::mem::take(self.mesh(mat));
                let mut trim = std::mem::take(self.mesh(Mat::Trim));
                geom::arch_infill(&mut infill, &mut trim, f, o.s, o.width, o.spring, o.rise, apex.min(y1), d0, d1);
                // Откосы проёма ниже пяты арки — обрамление.
                for s in [l, r] {
                    let (a, b) = f.bounds((s - 0.12, s + 0.12), (sill, o.spring), (d0 - 0.06, d1 + 0.06));
                    trim.cuboid(a, b);
                }
                *self.mesh(mat) = infill;
                *self.mesh(Mat::Trim) = trim;
            }
            // Дверь в проёме (широкие порталы и арки нефа остаются открытыми проходами).
            if !o.glass && o.width <= 3.0 && o.sill < y0 + 0.01 {
                self.door(DoorSpec { center: f.p(o.s, y0, dm), along_x: f.along_x, width: o.width, height: o.spring - y0, wall_t: d1 - d0, double: o.width > 2.0, open: false });
            }
            if o.glass {
                let (pts, uvs) = geom::lancet_outline(f, o.s, dm, o.width, sill, o.spring, o.rise);
                self.glass.push(GlassPane { kind: GlassKind::Lancet, pts, uvs, normal: f.d_axis() });
                self.invisible(f.bounds((l, r), (sill, apex), (dm - 0.1, dm + 0.1)));
                // Средник для широких окон.
                if o.width > 1.9 {
                    let (a, b) = f.bounds((o.s - 0.07, o.s + 0.07), (sill, apex - 0.2), (dm - 0.12, dm + 0.12));
                    self.mesh(Mat::Trim).cuboid(a, b);
                }
            }
            cursor = r;
        }
        self.solid_v(f.bounds((cursor, s1), (y0, y1), (d0, d1)), mat);
    }

    /// Ступенчатый контрфорс у стены с пинаклем наверху.
    /// `face` — координата грани стены по d, `dir` — куда выступает (+1/−1).
    fn buttress(&mut self, f: WallFrame, s: f32, face: f32, dir: f32, depth: f32, height: f32, width: f32) {
        let tiers = [(1.0, 0.55), (0.75, 0.8), (0.5, 1.0)];
        let mut y = 0.0;
        for (dk, hk) in tiers {
            let top = height * hk;
            let d_out = face + dir * depth * dk;
            let (a, b) = f.bounds((s - width / 2.0, s + width / 2.0), (y, top), (face.min(d_out), face.max(d_out)));
            self.solid_v((a, b), Mat::DarkStone);
            // Слив-карниз.
            let (a2, b2) = f.bounds((s - width / 2.0 - 0.08, s + width / 2.0 + 0.08), (top - 0.15, top), (face.min(d_out + dir * 0.1), face.max(d_out + dir * 0.1)));
            self.mesh(Mat::Trim).cuboid(a2, b2);
            y = top;
        }
        let pc = f.p(s, height, face + dir * depth * 0.25);
        self.pinnacle(pc, width * 0.45, 3.2);
    }

    /// Пинакль: маленькая башенка со шпилем.
    fn pinnacle(&mut self, base: Vec3, r: f32, h: f32) {
        let m = self.mesh(Mat::Trim);
        m.prism(base, r, h * 0.35, 4);
        m.spire(base + Vec3::Y * h * 0.35, r * 1.1, h * 0.65, 4);
        m.prism(base + Vec3::Y * (h - 0.05), r * 0.25, 0.25, 4);
    }

    /// Квадратная башня со шпилем, зубцами и окнами-бойницами.
    fn tower(&mut self, cx: f32, cz: f32, half: f32, height: f32, spire_h: f32) {
        self.solid([cx - half, 0.0, cz - half], [cx + half, height, cz + half], Mat::DarkStone);
        // Пояс-карниз.
        self.mesh(Mat::Trim).cuboid(Vec3::new(cx - half - 0.2, height - 0.5, cz - half - 0.2), Vec3::new(cx + half + 0.2, height, cz + half + 0.2));
        self.mesh(Mat::Trim).cuboid(Vec3::new(cx - half - 0.15, 2.0, cz - half - 0.15), Vec3::new(cx + half + 0.15, 2.4, cz + half + 0.15));
        // Зубцы.
        self.merlons_x(cx - half, cx + half, height, cz - half, cz - half + 0.6);
        self.merlons_x(cx - half, cx + half, height, cz + half - 0.6, cz + half);
        self.merlons_z(cx - half + 1.0, cx + half - 1.0, height, cx - half, cx - half + 0.6);
        self.merlons_z(cx - half + 1.0, cx + half - 1.0, height, cx + half - 0.6, cx + half);
        // Восьмигранный шпиль с барабаном.
        let r = half * 0.72;
        self.mesh(Mat::DarkStone).prism(Vec3::new(cx, height, cz), r, 2.5, 8);
        self.mesh(Mat::Roof).spire(Vec3::new(cx, height + 2.5, cz), r * 1.08, spire_h, 8);
        self.mesh(Mat::Gold).prism(Vec3::new(cx, height + 2.5 + spire_h - 0.3, cz), 0.12, 1.6, 4);
        self.mesh(Mat::Gold).cuboid(Vec3::new(cx - 0.5, height + 2.5 + spire_h + 0.7, cz - 0.06), Vec3::new(cx + 0.5, height + 2.5 + spire_h + 0.85, cz + 0.06));
        // Пинакли по углам.
        for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            self.pinnacle(Vec3::new(cx + sx * (half - 0.4), height, cz + sz * (half - 0.4)), 0.35, 3.0);
        }
        // Узкие светящиеся окна на всех гранях.
        for (n, along_x) in [(Vec3::Z, true), (Vec3::NEG_Z, true), (Vec3::X, false), (Vec3::NEG_X, false)] {
            let f = WallFrame { along_x };
            let d = if along_x { cz + n.z * (half + 0.03) } else { cx + n.x * (half + 0.03) };
            let sc = if along_x { cx } else { cz };
            for y in [height * 0.45, height * 0.72] {
                let (pts, uvs) = geom::lancet_outline(f, sc, d, 0.8, y, y + 1.6, 0.6);
                self.glass.push(GlassPane { kind: GlassKind::Lancet, pts, uvs, normal: n });
                let (a, b) = f.bounds((sc - 0.6, sc + 0.6), (y - 0.15, y), (d - 0.1, d + 0.1));
                self.mesh(Mat::Trim).cuboid(a, b);
            }
        }
    }

    /// Восьмигранная башенка-турель (коллизия — квадрат).
    fn turret(&mut self, cx: f32, cz: f32, r: f32, height: f32, spire_h: f32) {
        let h = r * 0.65;
        self.solid([cx - h, 0.0, cz - h], [cx + h, height, cz + h], Mat::DarkStone);
        let m = self.mesh(Mat::DarkStone);
        m.prism(Vec3::new(cx, 0.0, cz), r, height, 8);
        self.mesh(Mat::Trim).prism(Vec3::new(cx, height - 0.4, cz), r + 0.2, 0.4, 8);
        self.mesh(Mat::Roof).spire(Vec3::new(cx, height, cz), r + 0.25, spire_h, 8);
        self.mesh(Mat::Gold).prism(Vec3::new(cx, height + spire_h - 0.2, cz), 0.1, 1.0, 4);
    }

    /// Колонна: база, восьмигранный ствол, капитель. Коллизия — квадрат.
    fn column(&mut self, x: f32, z: f32, r: f32, height: f32) {
        self.solid([x - r * 0.8, 0.0, z - r * 0.8], [x + r * 0.8, height, z + r * 0.8], Mat::Trim);
        let m = self.mesh(Mat::Trim);
        m.prism(Vec3::new(x, 0.0, z), r * 1.3, 0.5, 8);
        m.prism(Vec3::new(x, 0.5, z), r * 1.1, 0.2, 8);
        m.prism(Vec3::new(x, 0.0, z), r, height, 8);
        m.prism(Vec3::new(x, height - 0.6, z), r * 1.25, 0.3, 8);
        m.prism(Vec3::new(x, height - 0.3, z), r * 1.45, 0.3, 4);
        // Тонкие колонки-«пучок» по сторонам ствола.
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2;
            m.prism(Vec3::new(x + a.cos() * r, 0.5, z + a.sin() * r), r * 0.28, height - 1.1, 6);
        }
    }

    /// Деревянный столб со скобой и факелом наверху.
    fn lamp_post(&mut self, x: f32, z: f32) {
        self.solid([x - 0.1, 0.0, z - 0.1], [x + 0.1, 3.0, z + 0.1], Mat::Iron);
        let m = self.mesh(Mat::Iron);
        m.prism(Vec3::new(x, 2.9, z), 0.25, 0.12, 8);
        m.prism(Vec3::new(x, 3.0, z), 0.12, 0.25, 8);
        self.torch(x, 3.35, z, Vec3::ZERO);
    }

    /// Люстра: железное кольцо со свечами на цепях.
    fn chandelier(&mut self, c: Vec3, top: f32) {
        let r = 1.3;
        let ring: Vec<Vec3> = (0..=16)
            .map(|i| {
                let a = i as f32 / 16.0 * std::f32::consts::TAU;
                c + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
            })
            .collect();
        let m = self.mesh(Mat::Iron);
        m.beam_path(&ring, 0.05, 0.05, Vec3::Y);
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2;
            let p = c + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            m.beam(p, c + Vec3::Y * 1.6, 0.02, 0.02, Vec3::X);
        }
        m.beam(c + Vec3::Y * 1.6, Vec3::new(c.x, top, c.z), 0.03, 0.03, Vec3::X);
        let candles: Vec<Vec3> = (0..8)
            .map(|i| {
                let a = (i as f32 + 0.5) / 8.0 * std::f32::consts::TAU;
                c + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
            })
            .collect();
        for p in candles {
            self.mesh(Mat::Trim).prism(p, 0.05, 0.3, 6);
            self.mesh(Mat::Coals).prism(p + Vec3::Y * 0.3, 0.025, 0.08, 4);
        }
        self.lamp(c + Vec3::Y * 0.4, Color::srgb(1.0, 0.72, 0.42), 120_000.0, 22.0);
    }

    /// Знамя (ткань с узором по UV) на стене.
    fn banner(&mut self, f: WallFrame, s: f32, d: f32, out: f32, y0: f32, y1: f32, width: f32) {
        let m = self.mesh(Mat::Banner);
        let n = f.d_axis() * out;
        let p = |ss: f32, y: f32| f.p(s + ss, y, d);
        m.quad_uv(
            [p(-width / 2.0, y0), p(width / 2.0, y0), p(width / 2.0, y1), p(-width / 2.0, y1)],
            [Vec2::new(0.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 0.0)],
            n,
        );
        // Перекладина.
        let (a, b) = f.bounds((s - width / 2.0 - 0.15, s + width / 2.0 + 0.15), (y1, y1 + 0.1), (d - 0.06, d + 0.06));
        self.mesh(Mat::Wood).cuboid(a, b);
    }

    /// Треугольный щипец (фронтон) над стеной.
    fn gable(&mut self, f: WallFrame, s0: f32, s1: f32, y0: f32, y1: f32, d0: f32, d1: f32, mat: Mat) {
        let sm = (s0 + s1) / 2.0;
        let m = self.mesh(mat);
        for (d, sign) in [(d0, -1.0f32), (d1, 1.0)] {
            m.tri(f.p(s0, y0, d), f.p(s1, y0, d), f.p(sm, y1, d), f.d_axis() * sign);
        }
        for (sa, sb) in [(s0, sm), (sm, s1)] {
            let (ya, yb) = if sa == s0 { (y0, y1) } else { (y1, y0) };
            let up = Vec3::Y + f.s_axis() * if sa == s0 { -1.0 } else { 1.0 };
            m.quad([f.p(sa, ya, d0), f.p(sb, yb, d0), f.p(sb, yb, d1), f.p(sa, ya, d1)], up);
        }
        // Обрамление по скатам и крест-фиал на вершине.
        let t = self.mesh(Mat::Trim);
        let dd = (d0 + d1) / 2.0;
        t.beam(f.p(s0, y0 + 0.2, dd), f.p(sm, y1 + 0.2, dd), (d1 - d0) / 2.0 + 0.15, 0.2, f.d_axis());
        t.beam(f.p(sm, y1 + 0.2, dd), f.p(s1, y0 + 0.2, dd), (d1 - d0) / 2.0 + 0.15, 0.2, f.d_axis());
        self.pinnacle(f.p(sm, y1, dd), 0.4, 3.0);
    }
}

/// Какой уровень загружен.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelKind {
    /// Херсонес (Херсон) около 510 г. — по умолчанию.
    Chersonesus,
    /// Готический замок — `MEDIEVAL_LEVEL=castle`.
    Castle,
}

pub fn active() -> LevelKind {
    static KIND: std::sync::OnceLock<LevelKind> = std::sync::OnceLock::new();
    *KIND.get_or_init(|| match std::env::var("MEDIEVAL_LEVEL").as_deref() {
        Ok("castle") => LevelKind::Castle,
        _ => LevelKind::Chersonesus,
    })
}

/// Полная раскладка активного уровня.
pub fn layout() -> Layout {
    match active() {
        LevelKind::Chersonesus => city::city_layout(),
        LevelKind::Castle => castle::castle_layout(),
    }
}

/// Коллизии из раскладки.
pub fn colliders(layout: Layout) -> LevelColliders {
    let mut boxes: Vec<Aabb> = layout.blocks.iter().filter(|b| b.solid).map(|b| b.aabb).collect();
    boxes.extend(layout.invisible);
    let doors = layout.doors.iter().map(|d| (d.closed_box(), d.open)).collect();
    LevelColliders::with_doors(boxes, layout.ground, doors)
}

pub fn level_colliders() -> LevelColliders {
    colliders(layout())
}

/// Точки появления игроков.
pub fn spawn_point(index: usize) -> Vec3 {
    match active() {
        LevelKind::Chersonesus => city::spawn_point(index),
        LevelKind::Castle => castle::spawn_point(index),
    }
}

/// Где ставить враждебных NPC (открытые места, далеко от появления игроков).
/// Подробнее (с фракциями) — [`npc_posts`].
pub fn npc_spawn_hints() -> Vec<Vec3> {
    match active() {
        LevelKind::Chersonesus => places::npc_posts().into_iter().filter(|p| p.group.hostile()).map(|p| p.pos).collect(),
        LevelKind::Castle => castle::npc_spawn_hints(),
    }
}

/// Клиент: создаёт меши, материалы и свет для уровня.
pub fn spawn_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut surfaces: ResMut<Assets<crate::graphics::SurfaceMaterial>>,
    mut glass_mats: ResMut<Assets<crate::graphics::GlassMaterial>>,
    mut flame_mats: ResMut<Assets<crate::graphics::FlameMaterial>>,
) {
    let mut layout = layout();

    // Все коробки и декор одного материала на одном участке — в один меш.
    for block in &layout.blocks {
        let size = block.aabb.max - block.aabb.min;
        // Огромные плиты (пол на весь мир) — отдельно, иначе их участок будет виден всегда.
        let chunk = if size.x > CHUNK * 2.0 || size.z > CHUNK * 2.0 { (i16::MAX, i16::MAX) } else { chunk_of((block.aabb.min + block.aabb.max) / 2.0) };
        layout.decor.entry((block.mat, chunk)).or_default().cuboid(block.aabb.min, block.aabb.max);
    }
    if let Some(ground) = &layout.ground {
        terrain::terrain_meshes(ground, &mut layout.decor);
    }
    if let Some(y) = layout.water_y {
        let mut mb = MeshBuilder::default();
        let r = 1200.0;
        mb.quad([Vec3::new(-r, y, -r), Vec3::new(r, y, -r), Vec3::new(r, y, r), Vec3::new(-r, y, r)], Vec3::Y);
        layout.decor.entry((Mat::Water, (i16::MAX, i16::MAX))).or_default().append(mb);
    }
    let mut decor: Vec<((Mat, Chunk), MeshBuilder)> = layout.decor.into_iter().collect();
    decor.sort_by_key(|(k, _)| *k);
    let mut materials: HashMap<Mat, Handle<crate::graphics::SurfaceMaterial>> = HashMap::new();
    for ((mat, _chunk), mb) in decor {
        if mb.is_empty() {
            continue;
        }
        let handle = materials.entry(mat).or_insert_with(|| surfaces.add(crate::graphics::surface_material(mat))).clone();
        let mut e = commands.spawn((Mesh3d(meshes.add(mb.build())), MeshMaterial3d(handle), Transform::IDENTITY));
        if matches!(mat, Mat::Coals | Mat::Water) {
            e.insert(NotShadowCaster);
        }
    }

    // Двери и жители.
    let wood = materials.entry(Mat::Wood).or_insert_with(|| surfaces.add(crate::graphics::surface_material(Mat::Wood))).clone();
    let iron = materials.entry(Mat::Iron).or_insert_with(|| surfaces.add(crate::graphics::surface_material(Mat::Iron))).clone();
    crate::doors::spawn_door_leaves(&mut commands, &mut meshes, wood, iron, &layout.doors);
    crate::residents::spawn_residents(&mut commands, &layout.residents);

    // Витражи (двусторонние, не отбрасывают тень — свет проходит в окна).
    for kind in [GlassKind::Lancet, GlassKind::Rose] {
        let mut mb = MeshBuilder::default();
        for pane in layout.glass.iter().filter(|p| p.kind == kind) {
            mb.polygon(&pane.pts, &pane.uvs, pane.normal);
            mb.polygon(&pane.pts, &pane.uvs, -pane.normal);
        }
        if mb.is_empty() {
            continue;
        }
        commands.spawn((
            Mesh3d(meshes.add(mb.build())),
            MeshMaterial3d(glass_mats.add(crate::graphics::glass_material(kind == GlassKind::Rose))),
            Transform::IDENTITY,
            NotShadowCaster,
        ));
    }

    // Факелы: железный держатель, пламя-билборд и мерцающий свет.
    let mut holders = MeshBuilder::default();
    for t in &layout.torches {
        let base = t.pos - Vec3::Y * 0.35;
        if t.out != Vec3::ZERO {
            let wall = base - t.out * 0.35;
            holders.beam(wall + Vec3::Y * -0.25, base, 0.03, 0.03, Vec3::Y);
            holders.cuboid(wall - Vec3::splat(0.08) - Vec3::Y * 0.3, wall + Vec3::splat(0.08) - Vec3::Y * 0.1);
        }
        holders.prism(base - Vec3::Y * 0.25, 0.05, 0.3, 6);
        holders.prism(base + Vec3::Y * 0.02, 0.1, 0.12, 6);
    }
    if !holders.is_empty() {
        commands.spawn((Mesh3d(meshes.add(holders.build())), MeshMaterial3d(surfaces.add(crate::graphics::surface_material(Mat::Iron)))));
    }
    let flame_mesh = meshes.add(Rectangle::new(0.42, 0.75));
    let flame = flame_mats.add(crate::graphics::FlameMaterial::default());
    for (i, t) in layout.torches.iter().enumerate() {
        crate::graphics::spawn_flame(&mut commands, flame_mesh.clone(), flame.clone(), t.pos + Vec3::Y * 0.12, t.out, i as f32 * 1.7);
    }
    for lamp in &layout.lamps {
        commands.spawn((
            PointLight { color: lamp.color, intensity: lamp.intensity, range: lamp.range, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(lamp.pos),
            crate::graphics::Flicker { base: lamp.intensity, phase: lamp.pos.x + lamp.pos.z },
        ));
    }
}

