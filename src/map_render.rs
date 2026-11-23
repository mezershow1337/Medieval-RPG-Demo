//! Карта местности, нарисованная «от руки» на пергаменте: растр строится на процессоре
//! по той же геометрии, из которой собран уровень (`level::Layout`), поэтому карта всегда
//! совпадает с миром — новые дома, стены и улицы появляются на ней сами.
//!
//! Как рисуем: сверху «смотрим» на уровень (самая высокая поверхность в каждой точке и её
//! материал), затем переводим это в приёмы рукописной карты: море — размытая ярь-медянка
//! со штриховкой волн вдоль берега, черепичные кровли — киноварно-охристой отмывкой,
//! кладка — серо-бурой, стены крепости — густыми чернилами, деревья — точками, рельеф —
//! лёгкой светотенью. Сверху — неровности пергамента, пятна и затемнённые края.
//! Травы на карте не отличить от земли: что где растёт, узнают из книг и от учителей.

use crate::level::{Layout, Mat};
use bevy::prelude::*;

/// Какой участок мира покрывает карта (мировые X и Z; север — −Z — вверху карты).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapBounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl MapBounds {
    /// Мировая точка -> доля карты (0..1 по ширине, 0..1 по высоте сверху вниз).
    pub fn to_uv(&self, x: f32, z: f32) -> Vec2 {
        Vec2::new((x - self.min.x) / (self.max.x - self.min.x), (z - self.min.y) / (self.max.y - self.min.y))
    }
    pub fn from_uv(&self, uv: Vec2) -> Vec2 {
        self.min + uv * (self.max - self.min)
    }
    pub fn size(&self) -> f32 {
        self.max.x - self.min.x
    }
}

/// Готовый растр: RGBA8, `size × size`.
pub struct MapRaster {
    pub size: usize,
    pub rgba: Vec<u8>,
    pub bounds: MapBounds,
}

/// Что лежит сверху в этой точке (для выбора краски).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Class {
    Ground = 0,
    Sea,
    Paving,
    Roof,
    Thatch,
    Masonry,
    Wood,
    Tree,
    Pool,
}

fn classify(m: Mat) -> Option<Class> {
    use Mat::*;
    Some(match m {
        Tile | Roof => Class::Roof,
        Thatch | Felt | Canvas => Class::Thatch,
        Limestone | Stone | DarkStone | Rubble | Plaster | OpusMixtum | Brick | Marble | Trim | Signinum => Class::Masonry,
        Paving | Cobble | Floor | Mosaic | Earth => Class::Paving,
        Wood => Class::Wood,
        Leaves | Conifer => Class::Tree,
        Water => Class::Pool,
        // Земля, трава и травы (что именно растёт — секрет знающих), мелкая утварь — не рисуем.
        _ => return None,
    })
}

/// Границы карты: всё построенное и все именованные места с полями вокруг, квадратом.
pub fn bounds_for(layout: &Layout, extra: &[Vec3]) -> MapBounds {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    let mut add = |p: Vec2| {
        lo = lo.min(p);
        hi = hi.max(p);
    };
    for b in &layout.blocks {
        let s = b.aabb.max - b.aabb.min;
        if s.x > 400.0 || s.z > 400.0 {
            continue;
        }
        add(Vec2::new(b.aabb.min.x, b.aabb.min.z));
        add(Vec2::new(b.aabb.max.x, b.aabb.max.z));
    }
    for p in extra {
        add(Vec2::new(p.x, p.z));
    }
    if lo.x > hi.x {
        return MapBounds { min: Vec2::splat(-50.0), max: Vec2::splat(50.0) };
    }
    let margin = if layout.ground.is_some() { 110.0 } else { 14.0 };
    let c = (lo + hi) / 2.0;
    let half = ((hi - lo).max_element() / 2.0 + margin).max(40.0);
    MapBounds { min: c - half, max: c + half }
}

// ---------------------------------------------------------------------------
// Шум и смешивание
// ---------------------------------------------------------------------------

fn hash(ix: i32, iz: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343) ^ (iz as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xff_ffff) as f32 / 0xff_ffff as f32
}

fn vnoise(x: f32, z: f32, seed: u32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - ix as f32, z - iz as f32);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let a = hash(ix, iz, seed) + (hash(ix + 1, iz, seed) - hash(ix, iz, seed)) * sx;
    let b = hash(ix, iz + 1, seed) + (hash(ix + 1, iz + 1, seed) - hash(ix, iz + 1, seed)) * sx;
    a + (b - a) * sz
}

/// Фрактальный шум 0..1.
fn fbm(x: f32, z: f32, seed: u32, octaves: u32) -> f32 {
    let (mut v, mut a, mut norm) = (0.0, 0.5, 0.0);
    let (mut x, mut z) = (x, z);
    for o in 0..octaves {
        v += a * vnoise(x, z, seed + o * 17);
        norm += a;
        x = x * 2.07 + 3.1;
        z = z * 2.07 + 7.7;
        a *= 0.5;
    }
    v / norm
}

type Rgb = [f32; 3];

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn mul(a: Rgb, k: f32) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// Краски (sRGB 0..1), как у переписчиков: железо-галловые чернила, ярь-медянка, киноварь, охра.
const PARCHMENT: Rgb = [0.86, 0.78, 0.60];
const PARCHMENT_DARK: Rgb = [0.70, 0.58, 0.40];
const INK: Rgb = [0.22, 0.14, 0.08];
const SEA: Rgb = [0.50, 0.60, 0.52];
const SEA_DEEP: Rgb = [0.40, 0.51, 0.46];
const ROOF: Rgb = [0.72, 0.40, 0.25];
const THATCH: Rgb = [0.72, 0.60, 0.36];
const MASONRY: Rgb = [0.64, 0.58, 0.47];
const WALL: Rgb = [0.38, 0.30, 0.22];
const WOOD: Rgb = [0.55, 0.42, 0.28];
const TREE: Rgb = [0.38, 0.44, 0.27];
const PAVING: Rgb = [0.90, 0.84, 0.68];

// ---------------------------------------------------------------------------
// Растеризация
// ---------------------------------------------------------------------------

struct Top {
    n: usize,
    h: Vec<f32>,
    class: Vec<u8>,
    /// Насколько высоко поверхность над землёй (для стен крепости и теней).
    rise: Vec<f32>,
}

impl Top {
    fn put(&mut self, i: usize, h: f32, c: Class) {
        if h > self.h[i] {
            self.h[i] = h;
            self.class[i] = c as u8;
        }
    }
}

/// Плавная высота земли (без сетки 6 м), если уровень её знает: по ней берег на карте
/// рисуется ровной линией, а не ступеньками.
pub fn smooth_ground() -> Option<fn(f32, f32) -> f32> {
    (crate::level::active() == crate::level::LevelKind::Chersonesus).then_some(crate::level::height_at as fn(f32, f32) -> f32)
}

/// Рисует карту уровня. `size` — сторона растра в пикселях.
pub fn render(layout: &Layout, bounds: MapBounds, size: usize) -> MapRaster {
    let smooth = smooth_ground();
    let n = size;
    let px = bounds.size() / n as f32;
    let to_px = |x: f32, z: f32| ((x - bounds.min.x) / px, (z - bounds.min.y) / px);
    let mut top = Top { n, h: vec![f32::NEG_INFINITY; n * n], class: vec![Class::Ground as u8; n * n], rise: vec![0.0; n * n] };

    // Коробки уровня: верхняя грань.
    for b in &layout.blocks {
        let Some(c) = classify(b.mat) else { continue };
        let s = b.aabb.max - b.aabb.min;
        if s.x > 400.0 || s.z > 400.0 {
            continue;
        }
        let (x0, z0) = to_px(b.aabb.min.x, b.aabb.min.z);
        let (x1, z1) = to_px(b.aabb.max.x, b.aabb.max.z);
        // Узкие стены (тоньше пикселя) всё равно оставляют след в одну точку.
        let (ix0, iz0) = (x0.round().max(0.0) as i64, z0.round().max(0.0) as i64);
        let (ix1, iz1) = ((x1.round() as i64).max(ix0 + 1).min(n as i64), (z1.round() as i64).max(iz0 + 1).min(n as i64));
        for iz in iz0..iz1 {
            for ix in ix0..ix1 {
                top.put(iz as usize * n + ix as usize, b.aabb.max.y, c);
            }
        }
    }
    // Декор (крыши, кладка, деревья): треугольники, видимые сверху.
    for ((mat, _), mb) in &layout.decor {
        let Some(c) = classify(*mat) else { continue };
        for [a, b, d] in mb.triangles() {
            raster_tri(&mut top, &to_px, [a, b, d], c);
        }
    }

    // Земля и море.
    let ground = |x: f32, z: f32| -> f32 {
        match &layout.ground {
            Some(g) => {
                let h = g.height(x, z);
                if h.is_finite() { h } else { f32::NEG_INFINITY }
            }
            None => f32::NEG_INFINITY,
        }
    };
    let water = layout.water_y;
    let mut gh = vec![0.0f32; n * n];
    let mut sea = vec![false; n * n];
    for iz in 0..n {
        for ix in 0..n {
            let i = iz * n + ix;
            let (x, z) = (bounds.min.x + (ix as f32 + 0.5) * px, bounds.min.y + (iz as f32 + 0.5) * px);
            let mut g = ground(x, z);
            if let Some(f) = smooth
                && g.is_finite()
            {
                g = f(x, z);
            }
            gh[i] = if g.is_finite() { g } else { top.h[i].max(0.0) };
            if top.h[i] < g + 0.08 {
                top.h[i] = g;
                top.class[i] = Class::Ground as u8;
            }
            if let Some(w) = water
                && top.h[i] < w
            {
                sea[i] = true;
                top.class[i] = Class::Sea as u8;
            }
            top.rise[i] = if top.h[i].is_finite() { top.h[i] - gh[i] } else { 0.0 };
            if !top.h[i].is_finite() {
                top.h[i] = gh[i];
            }
        }
    }

    // Расстояние от берега по морю (в пикселях) — для штриховки волн.
    let coast_dist = distance_field(n, &sea);

    let mut rgba = vec![0u8; n * n * 4];
    let at = |v: &Vec<f32>, ix: i64, iz: i64| v[(iz.clamp(0, n as i64 - 1) as usize) * n + ix.clamp(0, n as i64 - 1) as usize];
    let cls = |ix: i64, iz: i64| top.class[(iz.clamp(0, n as i64 - 1) as usize) * n + ix.clamp(0, n as i64 - 1) as usize];
    let m_per_px = px;
    for iz in 0..n {
        for ix in 0..n {
            let i = iz * n + ix;
            let (fx, fz) = (ix as f32, iz as f32);
            let (x, z) = (bounds.min.x + (fx + 0.5) * px, bounds.min.y + (fz + 0.5) * px);
            let (ixi, izi) = (ix as i64, iz as i64);

            // Пергамент: неровный тон, волокна, редкие пятна.
            let blotch = fbm(fx / 140.0, fz / 140.0, 11, 4);
            let fiber = fbm(fx / 3.0, fz / 22.0, 23, 2);
            let grain = hash(ix as i32, iz as i32, 5);
            let mut c = mix(PARCHMENT, PARCHMENT_DARK, (blotch - 0.35) * 0.9);
            c = mul(c, 0.96 + 0.05 * fiber + 0.025 * grain);

            let class = top.class[i];
            let rise = top.rise[i];
            let wobble = fbm(fx / 9.0, fz / 9.0, 41, 2) - 0.5;
            if class == Class::Sea as u8 {
                let d = coast_dist[i] * m_per_px;
                c = mix(c, mix(SEA, SEA_DEEP, (d / 220.0).min(1.0)), 0.55 + 0.1 * blotch);
                // Волны вдоль берега: чем дальше, тем реже и бледнее (как штриховка на картах).
                for (k, dist) in [7.0f32, 15.0, 25.0, 38.0].iter().enumerate() {
                    let w = (d + wobble * 3.0 - dist).abs();
                    let line = (1.0 - w / (0.9 * m_per_px.max(0.6))).clamp(0.0, 1.0);
                    c = mix(c, INK, line * (0.42 - 0.08 * k as f32));
                }
                // Редкие штрихи ряби в открытом море.
                if d > 50.0 {
                    let r = vnoise(fx / 5.0, fz / 1.4, 77);
                    let s = vnoise(fx / 37.0, fz / 29.0, 78);
                    if r > 0.82 && s > 0.62 {
                        c = mix(c, INK, 0.22);
                    }
                }
            } else {
                // Светотень рельефа (свет с северо-запада, как принято на картах).
                let hx = at(&gh, ixi + 1, izi) - at(&gh, ixi - 1, izi);
                let hz = at(&gh, ixi, izi + 1) - at(&gh, ixi, izi - 1);
                let shade = ((hx + hz) / (2.0 * m_per_px) * 1.6).clamp(-0.7, 0.7);
                c = mul(c, 1.0 - shade * 0.32);
                match class {
                    x if x == Class::Paving as u8 => c = mix(c, PAVING, 0.45),
                    x if x == Class::Roof as u8 => {
                        // Отмывка и штриховка по диагонали.
                        c = mix(c, ROOF, 0.62 + 0.08 * blotch);
                        if (ix + iz) % 4 == 0 {
                            c = mix(c, INK, 0.10);
                        }
                    }
                    x if x == Class::Thatch as u8 => c = mix(c, THATCH, 0.6),
                    x if x == Class::Wood as u8 => c = mix(c, WOOD, 0.55),
                    x if x == Class::Pool as u8 => c = mix(c, SEA, 0.75),
                    x if x == Class::Tree as u8 => {
                        let dots = vnoise(fx / 1.6, fz / 1.6, 91);
                        c = mix(c, TREE, 0.45 + 0.35 * (dots > 0.55) as u8 as f32);
                    }
                    x if x == Class::Masonry as u8 => {
                        if rise > 5.5 {
                            // Стены и башни крепости.
                            c = mix(c, WALL, 0.85);
                        } else {
                            c = mix(c, MASONRY, 0.7);
                        }
                    }
                    _ => {
                        // Земля: чуть зеленее в низинах, суше и светлее на склонах.
                        let veg = fbm(x / 60.0, z / 60.0, 3, 3);
                        c = mix(c, [0.74, 0.72, 0.52], (veg - 0.45) * 0.9);
                    }
                }
                // Тень от строений на юго-восток (тени делают план читаемым).
                let sx = (2.0 / m_per_px).round().max(1.0) as i64;
                let h_nw = at(&top.h, ixi - sx, izi - sx);
                if h_nw - top.h[i] > 1.5 {
                    c = mul(c, 0.84);
                }
            }

            // Чернильные контуры: перепад высот или смена «построено/не построено».
            let mut edge: f32 = 0.0;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let dh = (at(&top.h, ixi + dx, izi + dz) - top.h[i]).abs();
                // Под водой перепады дна не рисуем — только берег.
                if dh > 1.2 && class != Class::Sea as u8 && cls(ixi + dx, izi + dz) != Class::Sea as u8 {
                    edge = edge.max(((dh - 1.2) / 2.0).clamp(0.45, 0.9));
                }
                let other = cls(ixi + dx, izi + dz);
                if (other == Class::Sea as u8) != (class == Class::Sea as u8) {
                    edge = edge.max(0.85);
                }
            }
            c = mix(c, INK, edge * (0.85 + 0.15 * grain));

            // Края листа темнее, будто его долго носили в сумке.
            let (u, v) = (fx / n as f32, fz / n as f32);
            let e = u.min(v).min(1.0 - u).min(1.0 - v);
            let rim = (1.0 - e / 0.09).clamp(0.0, 1.0);
            let ragged = fbm(fx / 30.0, fz / 30.0, 99, 3);
            c = mix(c, PARCHMENT_DARK, rim * rim * (0.55 + 0.4 * ragged));
            // Пятна «ржавчины» на пергаменте.
            let fox = fbm(fx / 26.0, fz / 26.0, 131, 3);
            if fox > 0.74 {
                c = mul(c, 1.0 - (fox - 0.74) * 0.8);
            }

            let o = i * 4;
            for k in 0..3 {
                rgba[o + k] = (c[k].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            rgba[o + 3] = 255;
        }
    }
    MapRaster { size: n, rgba, bounds }
}

/// Треугольник сверху: заполняем пиксели, центр которых внутри; высота — интерполяция.
fn raster_tri(top: &mut Top, to_px: &impl Fn(f32, f32) -> (f32, f32), v: [Vec3; 3], c: Class) {
    let n = top.n as i64;
    let p: [(f32, f32); 3] = v.map(|q| to_px(q.x, q.z));
    let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
    if area.abs() < 1e-6 {
        return;
    }
    // Почти вертикальные грани (стенки) сверху не видны: их закрывает верх.
    let nrm = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
    if nrm.y.abs() < 0.2 {
        return;
    }
    let minx = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor().max(0.0) as i64;
    let maxx = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as i64).min(n - 1);
    let minz = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor().max(0.0) as i64;
    let maxz = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i64).min(n - 1);
    if minx > maxx || minz > maxz {
        return;
    }
    // Треугольник меньше пикселя: ставим одну точку (черепичная кровля из мелких деталей).
    if maxx - minx <= 1 && maxz - minz <= 1 {
        let cx = ((p[0].0 + p[1].0 + p[2].0) / 3.0) as i64;
        let cz = ((p[0].1 + p[1].1 + p[2].1) / 3.0) as i64;
        if (0..n).contains(&cx) && (0..n).contains(&cz) {
            top.put((cz * n + cx) as usize, (v[0].y + v[1].y + v[2].y) / 3.0, c);
        }
        return;
    }
    for iz in minz..=maxz {
        for ix in minx..=maxx {
            let (x, z) = (ix as f32 + 0.5, iz as f32 + 0.5);
            let w0 = ((p[1].0 - x) * (p[2].1 - z) - (p[2].0 - x) * (p[1].1 - z)) / area;
            let w1 = ((p[2].0 - x) * (p[0].1 - z) - (p[0].0 - x) * (p[2].1 - z)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let h = w0 * v[0].y + w1 * v[1].y + w2 * v[2].y;
            top.put((iz * n + ix) as usize, h, c);
        }
    }
}

/// Расстояние (в пикселях) от каждой точки моря до ближайшей суши: два прохода фаски 3-4.
fn distance_field(n: usize, sea: &[bool]) -> Vec<f32> {
    let big = 1e9f32;
    let mut d: Vec<f32> = sea.iter().map(|&s| if s { big } else { 0.0 }).collect();
    let (a, b) = (1.0f32, std::f32::consts::SQRT_2);
    for iz in 0..n {
        for ix in 0..n {
            let i = iz * n + ix;
            if d[i] == 0.0 {
                continue;
            }
            let mut v = d[i];
            if ix > 0 {
                v = v.min(d[i - 1] + a);
            }
            if iz > 0 {
                v = v.min(d[i - n] + a);
                if ix > 0 {
                    v = v.min(d[i - n - 1] + b);
                }
                if ix + 1 < n {
                    v = v.min(d[i - n + 1] + b);
                }
            }
            d[i] = v;
        }
    }
    for iz in (0..n).rev() {
        for ix in (0..n).rev() {
            let i = iz * n + ix;
            if d[i] == 0.0 {
                continue;
            }
            let mut v = d[i];
            if ix + 1 < n {
                v = v.min(d[i + 1] + a);
            }
            if iz + 1 < n {
                v = v.min(d[i + n] + a);
                if ix + 1 < n {
                    v = v.min(d[i + n + 1] + b);
                }
                if ix > 0 {
                    v = v.min(d[i + n - 1] + b);
                }
            }
            d[i] = v;
        }
    }
    // Если суши на карте нет вовсе — море «далеко от берега».
    for v in &mut d {
        if *v >= big {
            *v = 400.0;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_cover_city_and_camps() {
        let layout = crate::level::layout();
        let extra: Vec<Vec3> = crate::level::places().iter().map(|p| p.pos).collect();
        let b = bounds_for(&layout, &extra);
        for p in crate::level::places() {
            let uv = b.to_uv(p.pos.x, p.pos.z);
            assert!(uv.x > 0.02 && uv.x < 0.98 && uv.y > 0.02 && uv.y < 0.98, "{} вне карты: {uv}", p.id);
        }
        let back = b.from_uv(b.to_uv(12.0, -40.0));
        assert!((back - Vec2::new(12.0, -40.0)).length() < 1e-3);
    }

    /// Рисует карту целиком. С `MRPG_MAP_PNG=путь.png` сохраняет её для просмотра.
    #[test]
    fn renders_sea_land_and_roofs() {
        let layout = crate::level::layout();
        let extra: Vec<Vec3> = crate::level::places().iter().map(|p| p.pos).collect();
        let b = bounds_for(&layout, &extra);
        let size = std::env::var("MRPG_MAP_SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(512);
        let r = render(&layout, b, size);
        assert_eq!(r.rgba.len(), size * size * 4);
        // На карте Херсонеса есть и море (зеленовато-голубое), и черепичные крыши (рыжие).
        let px = |p: Vec3| {
            let uv = b.to_uv(p.x, p.z);
            let (ix, iz) = ((uv.x * size as f32) as usize, (uv.y * size as f32) as usize);
            let o = (iz * size + ix) * 4;
            [r.rgba[o], r.rgba[o + 1], r.rgba[o + 2]]
        };
        if crate::level::active() == crate::level::LevelKind::Chersonesus {
            let sea = px(Vec3::new(0.0, 0.0, -260.0));
            assert!(sea[1] > sea[0], "море должно быть зеленовато-голубым (ярь-медянка): {sea:?}");
            let reds = r.rgba.chunks_exact(4).filter(|c| c[0] as i32 - c[2] as i32 > 70).count();
            assert!(reds > size * size / 400, "на карте должны быть черепичные крыши: {reds}");
        }
        if let Ok(path) = std::env::var("MRPG_MAP_PNG") {
            let img = Image::new(
                bevy::render::render_resource::Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 },
                bevy::render::render_resource::TextureDimension::D2,
                r.rgba.clone(),
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::default(),
            );
            img.try_into_dynamic().unwrap().to_rgba8().save(path).unwrap();
        }
    }
}
