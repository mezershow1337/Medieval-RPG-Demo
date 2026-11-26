//! Рельеф вокруг Херсонеса: плато Гераклейского полуострова с балками, берег моря,
//! Карантинная бухта к востоку от города и Песочная — к западу.
//!
//! Высота считается функцией (одинаково на сервере и клиенте), затем запекается в сетку
//! [`HeightField`] с шагом 6 м: по ней ходят персонажи и строится меш земли.

use super::{CHUNK, Chunk, Mat, chunk_of};
use crate::collision::HeightField;
use bevy::prelude::*;
use std::collections::HashMap;

use super::geom::MeshBuilder;

/// Уровень моря.
pub const WATER_Y: f32 = -1.4;
/// Край карты (±), дальше — невидимая стена.
pub const WORLD_HALF: f32 = 900.0;
pub const CELL: f32 = 6.0;

/// Стена города (по оси кладки), по часовой стрелке при взгляде сверху (X вправо, Z вниз).
/// Север (−Z) — открытое море, восток (+X) — Карантинная бухта, юг (+Z) — суша.
pub const CITY_RING: [(f32, f32); 14] = [
    (-300.0, -50.0),
    (-282.0, -128.0),
    (-232.0, -160.0),
    (-120.0, -163.0),
    (0.0, -166.0),
    (120.0, -164.0),
    (215.0, -161.0),
    (268.0, -148.0),
    (285.0, -118.0),
    (285.0, 166.0),
    (-150.0, 166.0),
    (-212.0, 150.0),
    (-268.0, 104.0),
    (-302.0, 30.0),
];

/// Западный край набережной Карантинной бухты (камень набережной до этой линии).
pub const QUAY_EDGE: f32 = 300.0;
/// Участок бухты с каменной набережной (по Z).
pub const QUAY_Z: (f32, f32) = (-150.0, 172.0);

pub fn inside_city(x: f32, z: f32) -> bool {
    let mut inside = false;
    let n = CITY_RING.len();
    for i in 0..n {
        let (xi, zi) = CITY_RING[i];
        let (xj, zj) = CITY_RING[(i + n - 1) % n];
        if (zi > z) != (zj > z) && x < (xj - xi) * (z - zi) / (zj - zi) + xi {
            inside = !inside;
        }
    }
    inside
}

/// Расстояние до стены города (0 внутри).
pub fn city_distance(x: f32, z: f32) -> f32 {
    if inside_city(x, z) {
        return 0.0;
    }
    let p = Vec2::new(x, z);
    let n = CITY_RING.len();
    (0..n)
        .map(|i| {
            let a = Vec2::from(CITY_RING[i]);
            let b = Vec2::from(CITY_RING[(i + 1) % n]);
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            p.distance(a + (b - a) * t)
        })
        .fold(f32::MAX, f32::min)
}

fn hash(ix: i32, iz: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(374761393) ^ (iz as u32).wrapping_mul(668265263) ^ seed.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0xFFFFFF) as f32 / 16777216.0
}

fn vnoise(x: f32, z: f32, seed: u32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - ix as f32, z - iz as f32);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let a = hash(ix, iz, seed);
    let b = hash(ix + 1, iz, seed);
    let c = hash(ix, iz + 1, seed);
    let d = hash(ix + 1, iz + 1, seed);
    (a + (b - a) * ux) * (1.0 - uz) + (c + (d - c) * ux) * uz
}

pub fn fbm(x: f32, z: f32, seed: u32) -> f32 {
    let mut v = 0.0;
    let mut a = 0.5;
    let (mut x, mut z) = (x, z);
    for o in 0..4 {
        v += a * vnoise(x, z, seed + o);
        x = x * 2.03 + 1.7;
        z = z * 2.03 + 9.2;
        a *= 0.5;
    }
    v
}

/// Береговая линия: > 0 — суша (примерное расстояние до воды), < 0 — вода.
pub fn coast(x: f32, z: f32) -> f32 {
    // Открытое море на севере.
    let north = z - (-176.0 + 5.0 * (x * 0.021).sin() + 3.0 * (x * 0.067 + 1.0).sin());
    // Песочная бухта на западе: берег уходит на запад к югу.
    let west_line = if z < 60.0 { -316.0 + 6.0 * (z * 0.03).sin() } else { -316.0 - (z - 60.0) * 0.8 };
    let west = x - west_line;
    // Карантинная бухта: длинный узкий залив с севера на юг.
    let bay_w = if z < QUAY_Z.1 { QUAY_EDGE } else { QUAY_EDGE + (z - QUAY_Z.1) * 0.18 + 6.0 * ((z - QUAY_Z.1) * 0.02).sin() };
    let bay_e = 470.0 + 10.0 * (z * 0.015).sin() - (z - 220.0).max(0.0) * 0.55;
    let bay_end = 440.0;
    let bay = (bay_w - x).max(x - bay_e).max(z - bay_end);
    north.min(west).min(bay)
}

fn on_quay(x: f32, z: f32) -> bool {
    x > 280.0 && x < QUAY_EDGE + 30.0 && z > QUAY_Z.0 && z < QUAY_Z.1
}

/// Ось восточной балки (пересохшее русло к Карантинной бухте) на широте z.
/// По её дну сочится вода: родник, мята, окопник, девясил.
pub fn balka_x(z: f32) -> f32 {
    40.0 + (z * 0.008).sin() * 40.0 + 0.1 * z
}

/// Рудная горка в предгорьях к югу от города: невысокий холм с крутым северным уступом,
/// в который ведут штольню (уступ — по линии `z`, середина — `x`).
pub const MINE_FACE: (f32, f32) = (-249.0, 702.0);

/// Насколько рудная горка поднимает землю (м): неровный округлый холм с центром к югу от уступа,
/// с размытым краем и буграми; уступ со штольней — срез его северного склона.
fn knoll(x: f32, z: f32) -> f32 {
    knoll_shape(x, z).0
}

/// (подъём, «внутренность» 0..1) рудной горки — для рельефа и для выходов породы на склонах.
pub fn knoll_shape(x: f32, z: f32) -> (f32, f32) {
    let (mx, fz) = MINE_FACE;
    let (dx, dz) = (x - mx, z - (fz + 16.0));
    let dist = (dx * dx * 0.8 + dz * dz).sqrt();
    if dist > 60.0 {
        return (0.0, 0.0);
    }
    // Край холма не круглый: радиус гуляет от 26 до 37 м.
    let r = 26.0 + fbm(x * 0.022 + 3.0, z * 0.022 - 7.0, 41) * 14.0;
    let d = dist / r;
    let inside = 1.0 - smoothstep(0.15, 1.0, d);
    // Бугры и промоины по склонам; на вершине — пологая седловина.
    let bumps = (fbm(x * 0.11, z * 0.11, 43) - 0.47) * 2.4 * inside * (1.0 - inside * 0.5);
    let gully = (-(((dx + dz * 0.4 - 14.0) / 3.5).powi(2))).exp() * smoothstep(0.35, 0.8, d) * inside * 2.2;
    // К северу (к площадке перед уступом) горка обрывается быстро: площадка — на исходной земле.
    // Край обрыва вне выемки рваный.
    let wob = (fbm(x * 0.06 + 11.0, z * 0.06, 57) - 0.47) * 12.0 * smoothstep(10.0, 16.0, dx.abs());
    let north = smoothstep(fz - 7.0 + wob, fz + 3.0 + wob, z);
    let inside = inside * north;
    ((9.5 * inside + bumps * north - gully).max(0.0), inside)
}

/// Каменоломня у Карантинной бухты: известняковый взлобок, срезанный уступами с севера.
pub const QUARRY_FACE: (f32, f32) = (452.0, 300.0);

/// Насколько взлобок каменоломни поднимает землю (м).
fn quarry_rise(x: f32, z: f32) -> f32 {
    let (qx, qz) = QUARRY_FACE;
    if (x - qx).abs() > 50.0 || (z - qz).abs() > 60.0 {
        return 0.0;
    }
    // Край взлобка неровный, кроме самого среза (|x - qx| < 9).
    let wob = (fbm(x * 0.05, z * 0.05, 53) - 0.47) * 10.0;
    let side = 1.0 - smoothstep(11.0 + wob, 27.0 + wob, (x - qx).abs());
    let cut = smoothstep(9.0, 14.0, (x - qx).abs());
    let north = smoothstep(qz - 1.0 - cut * 6.0, qz + 6.0, z);
    let south = 1.0 - smoothstep(qz + 14.0 + wob, qz + 34.0 + wob, z);
    let bumps = (fbm(x * 0.13, z * 0.13, 59) - 0.47) * 1.4;
    ((4.0 + bumps * cut) * side * north * south).max(0.0)
}

/// Высота суши без учёта моря.
fn land(x: f32, z: f32) -> f32 {
    let d = city_distance(x, z);
    let k = smoothstep(20.0, 240.0, d);
    // Плато поднимается к югу, к предгорьям.
    let rise = ((z - 160.0) / 700.0).clamp(0.0, 1.0).powf(1.3) * 34.0;
    let hills = fbm(x * 0.006, z * 0.006, 11) * 22.0 + fbm(x * 0.02, z * 0.02, 5) * 5.0;
    // Балки — пересохшие русла, идущие к морю и к бухтам.
    let ravine = |cx: f32, k1: f32, w: f32| {
        let line = cx + (z * 0.008).sin() * 40.0 + k1 * z;
        let dd = (x - line).abs();
        (-(dd / w).powi(2)).exp()
    };
    let east = (-((x - balka_x(z)) / 26.0).powi(2)).exp();
    let r = ravine(-480.0, -0.25, 30.0) * 12.0 + east * smoothstep(320.0, 420.0, z) * 10.0;
    let l = k * (hills + rise - r - 4.0 + knoll(x, z) + quarry_rise(x, z)).max(-k * 2.0) + (1.0 - k) * 0.0;
    village_flat(x, z, l)
}

/// Под селом Святого Фоки земля выровнена (селяне веками срывали бугры и засыпали ямы под дворы):
/// пологая площадка с лёгким уклоном к югу, края плавно сходят в плато.
fn village_flat(x: f32, z: f32, l: f32) -> f32 {
    let c = crate::level::places::PHOCAS_VILLAGE;
    let (cx, cz) = (c.x - 3.0, c.z - 2.0);
    let e = (((x - cx) / 80.0).powi(2) + ((z - cz) / 32.0).powi(2)).sqrt();
    if e > 1.7 {
        return l;
    }
    let w = 1.0 - smoothstep(1.0, 1.7, e);
    let flat = 7.0 + (z - cz) * 0.03 + (x - cx) * 0.004;
    l + (flat - l) * w
}

/// Аналитическая высота земли.
pub fn height_at(x: f32, z: f32) -> f32 {
    let s = coast(x, z);
    let quay = on_quay(x, z);
    let l = land(x, z);
    if s >= 0.0 {
        if quay {
            return l;
        }
        // Галечный пляж и скалы у воды.
        let t = smoothstep(0.0, 10.0, s);
        WATER_Y + 0.25 + (l - WATER_Y - 0.25) * t
    } else {
        let depth = if quay || (x > QUAY_EDGE && x < 480.0) { 4.5 } else { 0.4 };
        WATER_Y - depth - (-s * 0.12).min(7.0)
    }
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Карта высот на всю карту. `holes` — прямоугольники (кратные 6 м), где земли нет
/// (там стоят свои коробки: рыбозасолочные цистерны и т. п.).
pub fn heightfield(holes: Vec<(Vec2, Vec2)>) -> HeightField {
    let n = (WORLD_HALF * 2.0 / CELL) as usize + 1;
    let origin = Vec2::splat(-WORLD_HALF);
    let mut data = Vec::with_capacity(n * n);
    for iz in 0..n {
        for ix in 0..n {
            data.push(height_at(origin.x + ix as f32 * CELL, origin.y + iz as f32 * CELL));
        }
    }
    HeightField { origin, cell: CELL, nx: n, nz: n, data, holes }
}

/// Высота земли (по той же сетке, по которой ходят персонажи). Сетка считается один раз.
pub fn ground_y(x: f32, z: f32) -> f32 {
    static HF: std::sync::OnceLock<HeightField> = std::sync::OnceLock::new();
    HF.get_or_init(|| heightfield(Vec::new())).height(x, z)
}

/// Меши земли по участкам: внутри стен — утоптанная земля, снаружи — трава и камень.
/// Клетки глубоко под водой и в «дырах» пропускаются.
pub fn terrain_meshes(hf: &HeightField, out: &mut HashMap<(Mat, Chunk), MeshBuilder>) {
    let n = hf.nx;
    let pos = |ix: usize, iz: usize| Vec3::new(hf.origin.x + ix as f32 * hf.cell, hf.node(ix, iz), hf.origin.y + iz as f32 * hf.cell);
    let normal = |ix: usize, iz: usize| {
        let l = hf.node(ix.saturating_sub(1), iz);
        let r = hf.node((ix + 1).min(n - 1), iz);
        let d = hf.node(ix, iz.saturating_sub(1));
        let u = hf.node(ix, (iz + 1).min(n - 1));
        Vec3::new(l - r, 2.0 * hf.cell, d - u).normalize()
    };
    // Вершины одного участка переиспользуются: индекс узла -> индекс вершины в меше участка.
    let mut maps: HashMap<(Mat, Chunk), HashMap<usize, u32>> = HashMap::new();
    for iz in 0..n - 1 {
        for ix in 0..n - 1 {
            let corners = [pos(ix, iz), pos(ix + 1, iz), pos(ix + 1, iz + 1), pos(ix, iz + 1)];
            if corners.iter().all(|c| c.y < WATER_Y - 2.0) {
                continue;
            }
            let c = (corners[0] + corners[2]) / 2.0;
            if hf.in_hole(c.x, c.z) {
                continue;
            }
            let mat = if inside_city(c.x, c.z) { Mat::Earth } else { Mat::Terrain };
            let chunk = chunk_of(c);
            let key = (mat, chunk);
            let mb = out.entry(key).or_default();
            let map = maps.entry(key).or_default();
            let mut v = [0u32; 4];
            for (k, (jx, jz)) in [(ix, iz), (ix + 1, iz), (ix + 1, iz + 1), (ix, iz + 1)].into_iter().enumerate() {
                let id = jz * n + jx;
                v[k] = *map.entry(id).or_insert_with(|| {
                    let p = pos(jx, jz);
                    mb.vert(p, normal(jx, jz), Vec2::new(p.x, p.z))
                });
            }
            // Лицом вверх: обход против часовой при взгляде сверху (X вправо, Z вниз — это по часовой).
            mb.index_tri(v[0], v[2], v[1]);
            mb.index_tri(v[0], v[3], v[2]);
        }
    }
    let _ = CHUNK;
}
