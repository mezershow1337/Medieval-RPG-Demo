//! Откуда в Херсоне руда, уголь, глина, песок и камень (см. `mining.rs`):
//! - **рудник** на рудной горке в предгорьях: штольня с крепью в северном уступе, выходы
//!   бурого железняка по уступу, шурф с воротом наверху, отвал, площадка разборки руды,
//!   обжиговые кострища, сыродутный горн, землянка рудокопов;
//! - **рыжая мочажина** в западной балке у готского селения: болотная руда под дёрном;
//! - **стан углежогов** в дубовом лесу на горах: шалаш, угольные площадки, штабеля поленьев,
//!   делянка с поваленными стволами;
//! - **глиняный карьер** в восточной балке, **песок** на берегу Песочной бухты,
//!   **каменоломня** за Карантинной бухтой (как Инкерманские — известняк для города).
//!
//! Здесь же — точки для систем игры: места добычи, кучи, места работников.

use super::*;
use crate::mining::{PileKind, SiteKind};
use std::sync::OnceLock;
use terrain::MINE_FACE;

// ---------------------------------------------------------------------------
// Где что
// ---------------------------------------------------------------------------

/// Стан углежогов (середина поляны).
pub const CHARCOAL_CAMP: Vec2 = Vec2::new(-560.0, 715.0);
/// Мочажина с болотной рудой: z — широта, x — по дну западной балки.
pub const BOG_Z: (f32, f32) = (440.0, 476.0);
pub const CLAY_PIT: Vec2 = Vec2::new(101.0, 250.0);
pub const SAND_BEACH: Vec2 = Vec2::new(-421.0, 203.0);
pub const QUARRY: Vec2 = Vec2::new(terrain::QUARRY_FACE.0, terrain::QUARRY_FACE.1);

/// Ось западной балки (к Песочной бухте) на широте z.
pub fn west_ravine_x(z: f32) -> f32 {
    -480.0 + (z * 0.008).sin() * 40.0 - 0.25 * z
}

/// Длина штольни от устья до забоя.
pub const ADIT_LEN: f32 = 12.0;
/// Полуширина и высота выработки (в свету).
const ADIT_HALF: f32 = 1.1;
const ADIT_H: f32 = 2.3;

/// Дыра в рельефе над штольней (кратна клетке рельефа 6 м).
fn mine_hole() -> Rect {
    let (mx, fz) = MINE_FACE;
    Rect::new(mx - 9.0, fz - 6.0, mx + 9.0, fz + 18.0)
}

/// Пол площадки перед уступом и в штольне.
pub fn mine_floor() -> f32 {
    let (mx, fz) = MINE_FACE;
    ground_y(mx, fz - 6.0)
}

/// Середина рудника (площадка перед штольней).
pub fn mine_yard() -> Vec3 {
    let (mx, fz) = MINE_FACE;
    Vec3::new(mx, mine_floor(), fz - 3.0)
}

/// Высота поверхности в дыре над штольней: перед уступом — площадка, за ним — горка
/// (уступ отвесный).
fn inner_y(x: f32, z: f32) -> f32 {
    let (mx, fz) = MINE_FACE;
    let hole = mine_hole();
    if z < fz {
        return mine_floor();
    }
    // Уступ высотой ~4,5 м над устьем; дальше склон поднимается к вершине горки.
    let crest = mine_floor() + ADIT_H + 2.2;
    let k = ((z - fz) / 9.0).clamp(0.0, 1.0);
    let slope = ground_y(x, z.max(fz + 9.0));
    let designed = crest + (slope - crest) * k * k * (3.0 - 2.0 * k);
    // К краям дыры поверхность сходится с рельефом — без отвесных стенок по бокам.
    let edge = (x - hole.x0).min(hole.x1 - x).min(hole.z1 - z);
    let e = (edge / 4.0).clamp(0.0, 1.0);
    let e = e * e * (3.0 - 2.0 * e);
    let natural = ground_y(x, z);
    // Ниже пола площадки не опускаемся: иначе меж уступом и площадкой — щель.
    let y = (natural + (designed - natural) * e).max(mine_floor());
    // Над штольней — всегда твердь не тоньше метра.
    if (x - mx).abs() < ADIT_HALF + 1.0 && z < fz + ADIT_LEN + 1.0 { y.max(mine_floor() + ADIT_H + 1.0) } else { y }
}

/// Шурф с воротом наверху горки.
pub fn shaft_pos() -> Vec3 {
    let (mx, fz) = MINE_FACE;
    g(mx + 13.0, fz + 14.0)
}

/// Обжиговые кострища рудника и сыродутный горн.
fn roast_pits() -> [Vec3; 2] {
    let (mx, fz) = MINE_FACE;
    [g(mx - 15.0, fz - 15.0), g(mx - 7.5, fz - 19.0)]
}

fn bloomery_rect() -> Rect {
    let (mx, fz) = MINE_FACE;
    Rect::new(mx + 4.0, fz - 25.0, mx + 10.0, fz - 19.5)
}

fn hut_rect() -> Rect {
    let (mx, fz) = MINE_FACE;
    Rect::new(mx - 25.0, fz - 10.0, mx - 20.0, fz - 5.5)
}

/// Угольные площадки стана.
fn clamp_spots() -> [Vec3; 3] {
    let c = CHARCOAL_CAMP;
    [g(c.x - 10.0, c.y + 5.0), g(c.x + 9.0, c.y + 8.0), g(c.x + 2.0, c.y - 10.0)]
}

/// Поваленные стволы на делянке: (комель, направление ствола).
fn felled() -> [(Vec2, f32); 4] {
    let c = CHARCOAL_CAMP;
    [(c + Vec2::new(-22.0, -9.0), 0.4), (c + Vec2::new(19.0, -14.0), 2.2), (c + Vec2::new(23.0, 12.0), -0.7), (c + Vec2::new(-6.0, 24.0), 1.3)]
}

fn bog_spots() -> [Vec3; 4] {
    [0.0f32, 0.3, 0.62, 0.9].map(|t| {
        let z = BOG_Z.0 + (BOG_Z.1 - BOG_Z.0) * t;
        let dx = [2.0, -3.0, 3.5, -1.5][(t * 3.4) as usize % 4];
        g(west_ravine_x(z) + dx, z)
    })
}

/// Место добычи: что, где, насколько широко.
#[derive(Clone, Copy, Debug)]
pub struct MineSite {
    pub id: u32,
    pub kind: SiteKind,
    pub pos: Vec3,
    pub radius: f32,
}

/// Куча (угольная или обжиговая) — место, где её складывают.
#[derive(Clone, Copy, Debug)]
pub struct PileSpot {
    pub id: u32,
    pub kind: PileKind,
    pub pos: Vec3,
}

pub fn mine_sites() -> &'static [MineSite] {
    static S: OnceLock<Vec<MineSite>> = OnceLock::new();
    S.get_or_init(|| {
        let (mx, fz) = MINE_FACE;
        let y = mine_floor();
        let mut v: Vec<(SiteKind, Vec3, f32)> = Vec::new();
        for p in bog_spots() {
            v.push((SiteKind::BogOre, p, 2.2));
        }
        v.push((SiteKind::Outcrop, Vec3::new(mx - 6.0, y, fz - 0.6), 1.6));
        v.push((SiteKind::Outcrop, Vec3::new(mx + 6.0, y, fz - 0.6), 1.6));
        v.push((SiteKind::Adit, Vec3::new(mx, y, fz + ADIT_LEN - 0.6), 1.3));
        v.push((SiteKind::Shaft, shaft_pos() + Vec3::new(0.0, 0.0, -1.3), 1.2));
        for (root, a) in felled() {
            let mid = root + Vec2::new(a.cos(), a.sin()) * 2.5;
            v.push((SiteKind::Woodlot, g(mid.x, mid.y), 2.2));
        }
        v.push((SiteKind::ClayPit, g(CLAY_PIT.x - 2.5, CLAY_PIT.y), 2.0));
        v.push((SiteKind::ClayPit, g(CLAY_PIT.x + 3.0, CLAY_PIT.y + 2.5), 2.0));
        v.push((SiteKind::Sand, g(SAND_BEACH.x, SAND_BEACH.y), 3.0));
        v.push((SiteKind::Sand, g(SAND_BEACH.x - 2.0, SAND_BEACH.y + 9.0), 3.0));
        v.push((SiteKind::Quarry, g(QUARRY.x - 3.0, QUARRY.y - 2.2), 1.6));
        v.push((SiteKind::Quarry, g(QUARRY.x + 3.5, QUARRY.y - 2.2), 1.6));
        v.into_iter().enumerate().map(|(i, (kind, pos, radius))| MineSite { id: i as u32 + 1, kind, pos, radius }).collect()
    })
}

pub fn pile_spots() -> &'static [PileSpot] {
    static S: OnceLock<Vec<PileSpot>> = OnceLock::new();
    S.get_or_init(|| {
        let mut v: Vec<(PileKind, Vec3)> = Vec::new();
        for p in clamp_spots() {
            v.push((PileKind::Clamp, p));
        }
        for p in roast_pits() {
            v.push((PileKind::Roast, p));
        }
        // Кострище у сыродутного горна готов: обжигать болотную руду.
        let gb = goth_bloomery_rect();
        v.push((PileKind::Roast, g(gb.x1 + 3.0, gb.z1 + 1.5)));
        v.into_iter().enumerate().map(|(i, (kind, pos))| PileSpot { id: i as u32 + 1, kind, pos }).collect()
    })
}

/// Где работают и живут люди рудника и стана (для распорядка дня).
pub struct WorkSpots {
    /// Забой, выходы, ворот, разборка руды, отвал, кострище, у горна.
    pub face: Vec3,
    pub outcrops: [Vec3; 2],
    pub windlass: Vec3,
    pub sorting: Vec3,
    pub spoil: Vec3,
    pub roast: Vec3,
    pub bloomery: Vec3,
    pub mine_fire: Vec3,
    pub mine_hut: Vec3,
    pub clamps: [Vec3; 3],
    pub woodlot: [Vec3; 2],
    pub stack: Vec3,
    pub camp_fire: Vec3,
    pub camp_hut: Vec3,
    pub bog: [Vec3; 2],
}

pub fn work_spots() -> WorkSpots {
    let (mx, fz) = MINE_FACE;
    let y = mine_floor();
    let c = CHARCOAL_CAMP;
    let br = bloomery_rect();
    let h = hut_rect();
    let f = felled();
    let bog = bog_spots();
    let fell = |k: usize| {
        let (root, a) = f[k];
        let p = root + Vec2::new(a.cos(), a.sin()) * 2.5 + Vec2::new(-a.sin(), a.cos()) * 0.9;
        g(p.x, p.y)
    };
    let cs = clamp_spots();
    WorkSpots {
        face: Vec3::new(mx, y, fz + ADIT_LEN - 0.9),
        outcrops: [Vec3::new(mx - 6.0, y, fz - 0.9), Vec3::new(mx + 6.0, y, fz - 0.9)],
        windlass: shaft_pos() + Vec3::new(-1.1, 0.0, 0.0),
        sorting: g(mx - 8.0, fz - 7.0),
        spoil: g(mx + 8.0, fz - 7.0),
        roast: roast_pits()[0] + Vec3::new(1.7, 0.0, 0.0),
        bloomery: Vec3::new(br.cx() - 1.0, super::smith_floor(br), br.cz() + 1.6),
        mine_fire: g(mx - 15.0, fz - 4.5),
        mine_hut: g(h.cx() + 3.2, h.cz()),
        clamps: [cs[0] + Vec3::new(3.6, 0.0, 0.0), cs[1] + Vec3::new(-3.6, 0.0, 0.0), cs[2] + Vec3::new(0.0, 0.0, 3.6)].map(|p| g(p.x, p.z)),
        woodlot: [fell(0), fell(2)],
        stack: g(c.x + 4.0, c.y + 1.5),
        camp_fire: g(c.x - 3.0, c.y - 3.5),
        camp_hut: g(c.x - 9.0, c.y - 6.0),
        bog: [bog[1] + Vec3::new(1.0, 0.0, 0.0), bog[2] + Vec3::new(-1.0, 0.0, 0.0)].map(|p| g(p.x, p.z)),
    }
}

/// Поляны, где не растут деревья (в `wilderness`).
pub fn clearings() -> Vec<(Vec2, f32)> {
    let (mx, fz) = MINE_FACE;
    let bz = (BOG_Z.0 + BOG_Z.1) / 2.0;
    vec![
        (Vec2::new(mx - 2.0, fz - 6.0), 34.0),
        (Vec2::new(mx + 12.0, fz + 16.0), 10.0),
        (CHARCOAL_CAMP, 19.0),
        (Vec2::new(west_ravine_x(bz), bz), 26.0),
        (CLAY_PIT, 12.0),
        (QUARRY, 16.0),
    ]
}

/// Тропы к руднику и стану углежогов (от дороги к готам).
pub fn mine_roads() -> Vec<Vec<Vec2>> {
    let (mx, fz) = MINE_FACE;
    let v = Vec2::new;
    vec![
        vec![v(-200.0, 420.0), v(-218.0, 520.0), v(-236.0, 610.0), v(mx - 4.0, fz - 30.0)],
        vec![v(GOTH_VILLAGE.x + 4.0, GOTH_VILLAGE.z + 26.0), v(-500.0, 600.0), v(-540.0, 680.0), v(CHARCOAL_CAMP.x + 6.0, CHARCOAL_CAMP.y - 16.0)],
    ]
}

// ---------------------------------------------------------------------------
// Постройка
// ---------------------------------------------------------------------------

pub fn mines(b: &mut Builder) {
    mine(b);
    bog(b);
    charcoal_camp(b);
    clay_pit(b);
    sand(b);
    quarry(b);
    // Кострище у горна готов.
    let p = pile_spots().last().unwrap().pos;
    b.at(p.x, p.z);
    roast_pit(b, p);
}

/// Плоский круг, лежащий по рельефу (каждая вершина — на земле).
fn ground_disc(b: &mut Builder, mat: Mat, c: Vec3, r: f32, lift: f32) {
    let n = 20;
    let rings = ((r / 1.2).ceil() as usize).max(3);
    let m = b.mesh(mat);
    let at = |rr: f32, a: f32| {
        let (x, z) = (c.x + a.cos() * rr, c.z + a.sin() * rr);
        Vec3::new(x, ground_y(x, z) + lift, z)
    };
    for ring in 0..rings {
        let (r0, r1) = (r * ring as f32 / rings as f32, r * (ring + 1) as f32 / rings as f32);
        for k in 0..n {
            let (a0, a1) = (k as f32 / n as f32 * TAU, (k + 1) as f32 / n as f32 * TAU);
            if ring == 0 {
                m.tri(at(0.0, 0.0), at(r1, a1), at(r1, a0), Vec3::Y);
            } else {
                m.quad([at(r0, a0), at(r0, a1), at(r1, a1), at(r1, a0)], Vec3::Y);
            }
        }
    }
}

/// Вертикальная полоса стены от `lo(t)` до `hi(t)` вдоль отрезка `a → c` (по горизонтали).
fn strip(b: &mut Builder, mat: Mat, a: Vec2, c: Vec2, lo: impl Fn(Vec2) -> f32, hi: impl Fn(Vec2) -> f32, facing: Vec3, step: f32) {
    let n = (a.distance(c) / step).ceil().max(1.0) as usize;
    let m = b.mesh(mat);
    for k in 0..n {
        let p0 = a + (c - a) * (k as f32 / n as f32);
        let p1 = a + (c - a) * ((k + 1) as f32 / n as f32);
        let (l0, l1, h0, h1) = (lo(p0), lo(p1), hi(p0), hi(p1));
        if h0 - l0 < 0.02 && h1 - l1 < 0.02 {
            continue;
        }
        m.quad([Vec3::new(p0.x, l0, p0.y), Vec3::new(p1.x, l1, p1.y), Vec3::new(p1.x, h1.max(l1), p1.y), Vec3::new(p0.x, h0.max(l0), p0.y)], facing);
    }
}

fn pick_tool(b: &mut Builder, foot: Vec3, top: Vec3, side: Vec3) {
    b.mesh(Mat::Wood).beam(foot, top, 0.022, 0.022, side);
    let d = (top - foot).normalize();
    let head = top - d * 0.04;
    b.mesh(Mat::Iron).beam(head - side * 0.26 + d * 0.03, head + side * 0.24 - d * 0.02, 0.025, 0.035, d);
}

fn spade_tool(b: &mut Builder, foot: Vec3, top: Vec3, side: Vec3) {
    let d = (top - foot).normalize();
    b.mesh(Mat::Wood).beam(foot + d * 0.32, top, 0.02, 0.02, side);
    b.mesh(Mat::Wood).beam(foot + d * 0.05, foot + d * 0.34, 0.11, 0.015, side.cross(d).normalize_or(Vec3::X));
    b.mesh(Mat::Iron).beam(foot, foot + d * 0.07, 0.115, 0.018, side.cross(d).normalize_or(Vec3::X));
}

/// Плетёная корзина (заплечная), сверху — руда или уголь.
fn basket(b: &mut Builder, p: Vec3, r: f32, h: f32, fill: Option<Mat>) {
    b.mesh(Mat::Thatch).lathe(p, &[(r * 0.75, 0.0), (r, h * 0.5), (r * 1.05, h), (r * 0.98, h), (r * 0.9, h * 0.6), (r * 0.65, 0.04)], 10);
    if let Some(m) = fill {
        b.mesh(m).lathe(p + Vec3::Y * (h * 0.85), &[(r * 0.98, 0.0), (r * 0.6, h * 0.25), (0.0, h * 0.32)], 9);
    }
    kit::hidden(b, p - Vec3::new(r, 0.0, r), p + Vec3::new(r, h, r));
}

/// Кучка кусков (руда, порода, глина): низкое ложе и навал угловатых кусков по нему.
fn lumps(b: &mut Builder, p: Vec3, r: f32, h: f32, mat: Mat, rng: &mut Rng) {
    mound(b, p - Vec3::Y * 0.03, r * 0.9, h * 0.55, mat);
    let n = (10.0 + r * 14.0) as usize;
    for k in 0..n {
        let a = rng.range(0.0, TAU);
        let d = r * rng.range(0.0, 1.0f32).sqrt() * 1.05;
        // Ближе к середине — выше (навал конусом).
        let top = h * (1.0 - (d / r).powf(1.6)).max(0.0);
        let s = rng.range(0.06, 0.15) * (r / 0.7).clamp(0.7, 1.5) * if k % 5 == 0 { 1.6 } else { 1.0 };
        let c = Vec3::new(p.x + a.cos() * d, p.y + top * rng.range(0.55, 0.95), p.z + a.sin() * d);
        // Крупные глыбы пустой породы — сканы камней, мелочь — угловатые куски.
        if mat == Mat::Bedrock && k % 3 == 0 {
            b.plant(PlantKind::Rock, c - Vec3::Y * s, s * 3.2, rng.range(0.0, TAU), rng.int(64) as u32);
            continue;
        }
        let s = if mat == Mat::Bedrock { s * 0.7 } else { s };
        b.mesh(mat).obox(c, Vec3::new(s, s * rng.range(0.55, 0.85), s * rng.range(0.7, 1.0)), rng.range(0.0, TAU));
    }
}

/// Обжиговое кострище: кольцо камней, зольное ложе.
fn roast_pit(b: &mut Builder, p: Vec3) {
    let mut rng = Rng::new((p.x * 3.0 + p.z) as i32 as u32);
    // Зольное ложе (почерневшая земля) и кольцо из полевых камней.
    b.mesh(Mat::Earth).disc(p + Vec3::Y * 0.04, 1.6, 16, true);
    for k in 0..12 {
        let a = k as f32 / 12.0 * TAU + rng.range(-0.12, 0.12);
        let c = p + Vec3::new(a.cos() * 1.8, -0.12, a.sin() * 1.8);
        b.plant(PlantKind::Rock, c, rng.range(0.32, 0.5), rng.range(0.0, TAU), rng.int(64) as u32);
    }
    // Рыжая окалина вокруг.
    b.mesh(Mat::Limonite).disc(p + Vec3::new(1.9, 0.03, 0.6), 0.5, 8, true);
}

fn mine(b: &mut Builder) {
    let (mx, fz) = MINE_FACE;
    let y = mine_floor();
    let hole = mine_hole();
    b.holes.push((Vec2::new(hole.x0, hole.z0), Vec2::new(hole.x1, hole.z1)));
    b.at(mx, fz);
    let mut rng = Rng::new(5101);
    // Площадка перед уступом и пол штольни.
    b.solid([hole.x0, y - 1.0, hole.z0], [hole.x1, y, fz], Mat::Earth);
    b.solid([mx - ADIT_HALF, y - 1.0, fz], [mx + ADIT_HALF, y, fz + ADIT_LEN], Mat::Earth);
    // Поверхность горки над штольней (меш) и твердь под ней (невидимые столбы).
    let step = 1.5;
    let (x0, x1) = (hole.x0, hole.x1);
    let nz = ((hole.z1 - fz) / step).round() as usize;
    let nx = ((x1 - x0) / step).round() as usize;
    for i in 0..nx {
        for j in 0..nz {
            let (xa, xb) = (x0 + (x1 - x0) * i as f32 / nx as f32, x0 + (x1 - x0) * (i + 1) as f32 / nx as f32);
            let (za, zb) = (fz + (hole.z1 - fz) * j as f32 / nz as f32, fz + (hole.z1 - fz) * (j + 1) as f32 / nz as f32);
            let c = [Vec3::new(xa, inner_y(xa, za), za), Vec3::new(xa, inner_y(xa, zb), zb), Vec3::new(xb, inner_y(xb, zb), zb), Vec3::new(xb, inner_y(xb, za), za)];
            b.mesh(Mat::Terrain).quad(c, Vec3::Y);
        }
    }
    // Столбы тверди: слева и справа от штольни — до поверхности; над штольнёй — от свода.
    let cols = [(x0, mx - ADIT_HALF, y - 1.0), (mx - ADIT_HALF, mx + ADIT_HALF, y + ADIT_H + 0.05), (mx + ADIT_HALF, x1, y - 1.0)];
    for (ca, cb, bottom) in cols {
        let n = ((cb - ca) / step).ceil().max(1.0) as usize;
        for i in 0..n {
            let (xa, xb) = (ca + (cb - ca) * i as f32 / n as f32, ca + (cb - ca) * (i + 1) as f32 / n as f32);
            for j in 0..nz {
                let (za, zb) = (fz + (hole.z1 - fz) * j as f32 / nz as f32, fz + (hole.z1 - fz) * (j + 1) as f32 / nz as f32);
                // За забоем над штольней — сплошная твердь до пола.
                let bot = if bottom > y && za >= fz + ADIT_LEN - 0.01 { y - 1.0 } else { bottom };
                let top = [inner_y(xa, za), inner_y(xa, zb), inner_y(xb, za), inner_y(xb, zb)].into_iter().fold(f32::MIN, f32::max);
                b.invisible.push(Aabb::new(Vec3::new(xa, bot, za), Vec3::new(xb, top, zb)));
            }
        }
    }
    // Отвесный уступ (с устьем штольни) и бока горки над дырой.
    let face_lo = |_: Vec2| y;
    let face_hi = |p: Vec2| inner_y(p.x, fz + 0.01);
    strip(b, Mat::Bedrock, Vec2::new(x0, fz), Vec2::new(mx - ADIT_HALF - 0.2, fz), face_lo, face_hi, Vec3::NEG_Z, 0.75);
    strip(b, Mat::Bedrock, Vec2::new(mx + ADIT_HALF + 0.2, fz), Vec2::new(x1, fz), face_lo, face_hi, Vec3::NEG_Z, 0.75);
    strip(b, Mat::Bedrock, Vec2::new(mx - ADIT_HALF - 0.2, fz), Vec2::new(mx + ADIT_HALF + 0.2, fz), |_| y + ADIT_H + 0.2, face_hi, Vec3::NEG_Z, 0.75);
    // Стенки дыры смотрят внутрь выемки (на площадку), иначе снаружи видна щель в небо.
    for (x, n) in [(x0, Vec3::X), (x1, Vec3::NEG_X)] {
        let lo = |p: Vec2| ground_y(p.x, p.y).min(inner_y(p.x, p.y));
        let hi = |p: Vec2| ground_y(p.x, p.y).max(inner_y(p.x, p.y));
        strip(b, Mat::Bedrock, Vec2::new(x, hole.z0), Vec2::new(x, hole.z1), lo, hi, n, 0.75);
        b.invisible.push(Aabb::new(Vec3::new(x - 0.05, y - 1.0, hole.z0), Vec3::new(x + 0.05, y, fz)));
    }
    strip(b, Mat::Earth, Vec2::new(x0, hole.z0), Vec2::new(x1, hole.z0), |_| y - 0.2, |p| ground_y(p.x, p.y).max(y - 0.2), Vec3::Z, 0.75);
    // Глыбы у подножия уступа и по углам — скрывают стыки.
    for (dx, dz, s) in [(-8.6, 0.6, 1.9), (8.4, 0.9, 2.1), (-3.6, 0.3, 1.1), (3.9, 0.4, 1.2), (-9.2, 7.0, 2.4), (9.1, 6.5, 2.2), (-6.0, 3.5, 1.4), (6.4, 4.0, 1.5)] {
        let c = Vec3::new(mx + dx, if dz > 1.0 { inner_y(mx + dx, fz + dz) - s * 0.5 } else { y - s * 0.25 }, fz + dz);
        b.plant(PlantKind::Rock, c, s, rng.range(0.0, TAU), rng.int(64) as u32);
    }
    // Уступ не ровная стена: навал глыб у подножия и по бровке, кроме устья.
    let mut x = hole.x0 + 0.8;
    while x < hole.x1 - 0.8 {
        if (x - mx).abs() > ADIT_HALF + 1.3 {
            let top = inner_y(x, fz + 0.01);
            let s = rng.range(1.3, 2.3);
            // Жилу (выходы руды) не заслоняем.
            let vein = ((x - mx).abs() - 6.0).abs() < 2.2;
            if !vein {
                b.plant(PlantKind::Rock, Vec3::new(x, y - s * 0.35, fz + rng.range(0.1, 0.6)), s, rng.range(0.0, TAU), rng.int(64) as u32);
            }
            if top - y > 2.5 {
                let s2 = rng.range(1.2, 2.0);
                b.plant(PlantKind::Rock, Vec3::new(x + rng.range(-0.5, 0.5), top - s2 * 0.55, fz + rng.range(0.3, 0.9)), s2, rng.range(0.0, TAU), rng.int(64) as u32);
            }
        } else {
            // Над устьем — бровка из камней поменьше.
            let top = inner_y(x, fz + 0.01);
            b.plant(PlantKind::Rock, Vec3::new(x, top - 0.45, fz + 0.7), rng.range(0.8, 1.1), rng.range(0.0, TAU), rng.int(64) as u32);
        }
        x += rng.range(1.6, 2.4);
    }
    // Жила бурого железняка в уступе: охристые желваки по обе стороны устья.
    for side in [-1.0f32, 1.0] {
        for k in 0..9 {
            let x = mx + side * rng.range(4.2, 7.8);
            let yy = y + rng.range(0.25, 2.3);
            let s = rng.range(0.12, 0.32);
            b.mesh(Mat::Limonite).obox(Vec3::new(x, yy, fz - s * 0.55 + k as f32 * 0.001), Vec3::new(s * 1.4, s, s * 1.1), rng.range(-0.5, 0.5));
        }
        // Отбитая руда у подножия.
        lumps(b, Vec3::new(mx + side * 6.0, y, fz - 1.1), 0.55, 0.35, Mat::Limonite, &mut rng);
    }
    // Штольня: стены, свод, забой — рваная порода.
    let (l, r) = (mx - ADIT_HALF, mx + ADIT_HALF);
    let end = fz + ADIT_LEN;
    let m = b.mesh(Mat::Bedrock);
    m.quad([Vec3::new(l, y, end), Vec3::new(l, y, fz), Vec3::new(l, y + ADIT_H, fz), Vec3::new(l, y + ADIT_H, end)], Vec3::X);
    m.quad([Vec3::new(r, y, fz), Vec3::new(r, y, end), Vec3::new(r, y + ADIT_H, end), Vec3::new(r, y + ADIT_H, fz)], Vec3::NEG_X);
    m.quad([Vec3::new(l, y + ADIT_H, fz), Vec3::new(r, y + ADIT_H, fz), Vec3::new(r, y + ADIT_H, end), Vec3::new(l, y + ADIT_H, end)], Vec3::NEG_Y);
    b.mesh(Mat::Limonite).quad([Vec3::new(l, y, end), Vec3::new(r, y, end), Vec3::new(r, y + ADIT_H, end), Vec3::new(l, y + ADIT_H, end)], Vec3::NEG_Z);
    // Выступы породы на стенах и в забое.
    for k in 0..26 {
        let zz = rng.range(fz + 0.3, end - 0.2);
        let (x, n) = if k % 2 == 0 { (l, 1.0) } else { (r, -1.0) };
        let s = rng.range(0.08, 0.2);
        let mat = if rng.chance(0.3) { Mat::Limonite } else { Mat::Bedrock };
        b.mesh(mat).obox(Vec3::new(x + n * s * 0.35, rng.range(y + 0.2, y + ADIT_H - 0.3), zz), Vec3::new(s * 0.6, s, s * 1.4), rng.range(-0.3, 0.3));
    }
    for _ in 0..10 {
        let s = rng.range(0.1, 0.24);
        b.mesh(Mat::Limonite).obox(Vec3::new(rng.range(l + 0.2, r - 0.2), rng.range(y + 0.3, y + 2.0), end - s * 0.3), Vec3::new(s * 1.2, s, s * 0.7), rng.range(-0.5, 0.5));
    }
    lumps(b, Vec3::new(mx + 0.4, y, end - 0.7), 0.5, 0.3, Mat::Limonite, &mut rng);
    // Крепь: каждые 1,2 м — две стойки и верхняк, между окладами — затяжка из жердей.
    let wood = b.mesh(Mat::Wood);
    let mut zz = fz + 0.5;
    let mut sets = Vec::new();
    while zz < end - 0.8 {
        sets.push(zz);
        for sx in [l + 0.14, r - 0.14] {
            wood.beam(Vec3::new(sx, y, zz), Vec3::new(sx + (mx - sx).signum() * 0.04, y + ADIT_H - 0.12, zz), 0.09, 0.09, Vec3::Z);
        }
        wood.beam(Vec3::new(l + 0.02, y + ADIT_H - 0.11, zz), Vec3::new(r - 0.02, y + ADIT_H - 0.11, zz), 0.1, 0.1, Vec3::Y);
        zz += 1.2;
    }
    let (z_a, z_b) = (fz + 0.3, sets.last().copied().unwrap_or(end) + 0.2);
    for k in 0..6 {
        let x = l + 0.2 + (r - l - 0.4) * k as f32 / 5.0;
        wood.beam(Vec3::new(x, y + ADIT_H - 0.03, z_a), Vec3::new(x, y + ADIT_H - 0.03, z_b), 0.035, 0.03, Vec3::Y);
    }
    for (sx, n) in [(l + 0.03, 1.0f32), (r - 0.03, -1.0)] {
        for k in 0..4 {
            let yy = y + 0.35 + k as f32 * 0.5;
            wood.beam(Vec3::new(sx, yy, z_a), Vec3::new(sx, yy, z_b), 0.03, 0.035, Vec3::X * n);
        }
    }
    // Устье: тяжёлый оклад с козырьком.
    let p = b.mesh(Mat::Wood);
    for sx in [l - 0.18, r + 0.18] {
        p.beam(Vec3::new(sx, y - 0.1, fz - 0.15), Vec3::new(sx, y + ADIT_H + 0.25, fz - 0.15), 0.15, 0.15, Vec3::Z);
    }
    p.beam(Vec3::new(l - 0.5, y + ADIT_H + 0.3, fz - 0.15), Vec3::new(r + 0.5, y + ADIT_H + 0.3, fz - 0.15), 0.17, 0.16, Vec3::Y);
    for k in 0..7 {
        let x = l - 0.45 + (r - l + 0.9) * k as f32 / 6.0;
        p.beam(Vec3::new(x, y + ADIT_H + 0.48, fz + 0.4), Vec3::new(x + rng.range(-0.03, 0.03), y + ADIT_H + 0.18, fz - 0.95), 0.1, 0.03, Vec3::Y);
    }
    kit::hidden(b, Vec3::new(l - 0.35, y, fz - 0.32), Vec3::new(l, y + ADIT_H + 0.4, fz));
    kit::hidden(b, Vec3::new(r, y, fz - 0.32), Vec3::new(r + 0.35, y + ADIT_H + 0.4, fz));
    b.torch(r + 0.42, y + 1.75, fz - 0.4, Vec3::NEG_Z);
    // Глиняный светильник на уступе в забое.
    let lamp = Vec3::new(r - 0.25, y + 1.45, end - 0.6);
    b.mesh(Mat::Rubble).cuboid(lamp - Vec3::new(0.16, 0.08, 0.16), lamp + Vec3::new(0.1, 0.0, 0.16));
    b.mesh(Mat::Ceramic).lathe(lamp, &[(0.0, 0.0), (0.07, 0.0), (0.08, 0.03), (0.05, 0.05), (0.0, 0.05)], 8);
    b.lamp(lamp + Vec3::new(-0.3, 0.25, 0.0), Color::srgb(1.0, 0.62, 0.3), 2_500.0, 7.0);
    b.lamp(Vec3::new(mx, y + 2.0, fz + 4.5), Color::srgb(1.0, 0.6, 0.3), 1_200.0, 5.0);

    // ---- площадка перед уступом ----
    // Отвал пустой породы под горку.
    let sp = g(mx + 11.0, fz - 8.0);
    b.at(sp.x, sp.z);
    lumps(b, sp - Vec3::Y * 0.2, 4.6, 2.4, Mat::Bedrock, &mut rng);
    lumps(b, sp + Vec3::new(-3.4, -0.05, -3.0), 1.3, 0.7, Mat::Bedrock, &mut rng);
    for _ in 0..7 {
        let a = rng.range(0.0, TAU);
        let d = rng.range(1.0, 4.6);
        let c = Vec3::new(sp.x + a.cos() * d, 0.0, sp.z + a.sin() * d);
        let hh = (2.45 * (1.0 - d / 5.0)).max(0.0);
        b.plant(PlantKind::Rock, Vec3::new(c.x, sp.y + hh - 0.5, c.z), rng.range(0.5, 0.9), rng.range(0.0, TAU), rng.int(64) as u32);
    }
    kit::hidden(b, sp - Vec3::new(3.0, 0.3, 3.0), sp + Vec3::new(3.0, 1.6, 3.0));
    // Разборка руды: плита, кучи руды и породы, корзины.
    let so = g(mx - 8.0, fz - 7.0);
    b.at(so.x, so.z);
    b.mesh(Mat::Limestone).obox(so + Vec3::new(0.0, 0.18, -1.0), Vec3::new(0.7, 0.2, 0.5), 0.2);
    kit::hidden(b, so + Vec3::new(-0.7, 0.0, -1.5), so + Vec3::new(0.7, 0.38, -0.5));
    lumps(b, so + Vec3::new(-1.6, -0.05, -0.4), 0.9, 0.55, Mat::Limonite, &mut rng);
    lumps(b, so + Vec3::new(1.7, -0.05, -0.2), 0.8, 0.45, Mat::Rubble, &mut rng);
    basket(b, so + Vec3::new(-0.6, 0.0, 0.9), 0.28, 0.55, Some(Mat::Limonite));
    basket(b, so + Vec3::new(0.3, 0.0, 1.2), 0.28, 0.55, None);
    // Инструмент у уступа: кайла и заступы.
    let tr = Vec3::new(mx - 2.6, y, fz - 0.35);
    pick_tool(b, tr, tr + Vec3::new(0.1, 1.0, 0.25), Vec3::X);
    pick_tool(b, tr + Vec3::new(0.35, 0.0, 0.0), tr + Vec3::new(0.42, 1.0, 0.26), Vec3::X);
    spade_tool(b, tr + Vec3::new(-0.45, 0.0, -0.05), tr + Vec3::new(-0.38, 1.25, 0.25), Vec3::X);
    // Обжиговые кострища, поленница.
    for p in roast_pits() {
        b.at(p.x, p.z);
        roast_pit(b, p);
    }
    let rp = roast_pits()[0];
    b.at(rp.x, rp.z);
    lumps(b, rp + Vec3::new(-2.4, -0.05, 1.4), 0.7, 0.4, Mat::Limonite, &mut rng);
    let wp = g(mx - 19.0, fz - 17.0);
    b.placed(wp, |t| kit::woodpile(t, Rect::new(-1.4, -0.4, 1.4, 0.4), 0.9));
    // Сыродутный горн рудника.
    let br = bloomery_rect();
    let fy = terrace(b, br.inset(-0.6), Mat::Rubble);
    b.placed(Vec3::new(br.cx(), fy, br.cz()), |t| {
        t.decor([-3.0, 0.0, -2.75], [3.0, 0.03, 2.75], Mat::Earth);
        kit::bloomery(t, "Сыродутный горн рудника", Vec3::new(-1.0, 0.0, 0.0), Vec3::Z);
    });
    // Землянка рудокопов: сруб на треть в земле, кровля из дёрна по жердям.
    let h = hut_rect();
    let hy = terrace(b, h.inset(-0.5), Mat::Rubble);
    let (w, d) = (h.w(), h.d());
    b.placed(Vec3::new(h.cx(), hy, h.cz()), move |t| {
        let l = Rect::new(-w / 2.0, -d / 2.0, w / 2.0, d / 2.0);
        let ht = 1.6;
        let door = [kit::Op::Door { s: 0.0, w: 0.9, h: 1.5 }];
        kit::wall_x(t, l.x0, l.x1, l.z0 + 0.15, 0.3, ht, &[], Mat::Wood);
        kit::wall_x(t, l.x0, l.x1, l.z1 - 0.15, 0.3, ht, &[], Mat::Wood);
        kit::wall_z(t, l.z0 + 0.3, l.z1 - 0.3, l.x0 + 0.15, 0.3, ht, &[], Mat::Wood);
        kit::wall_z(t, l.z0 + 0.3, l.z1 - 0.3, l.x1 - 0.15, 0.3, ht, &door, Mat::Wood);
        thatch_roof(t, l, ht, 1.2, true, 0.35, Mat::Wood);
        t.decor([l.x0 + 0.3, 0.0, l.z0 + 0.3], [l.x1 - 0.3, 0.03, l.z1 - 0.3], Mat::Earth);
        kit::bed(t, Rect::new(l.x0 + 0.35, l.z0 + 0.35, l.x0 + 1.15, l.z1 - 0.35));
        kit::hearth(t, Vec3::new(l.x1 - 1.2, 0.0, l.z0 + 0.9));
    });
    campfire(b, g(mx - 15.0, fz - 4.5));
    // Шурф с воротом наверху горки.
    shaft(b, shaft_pos(), &mut rng);
}

/// Шурф: сруб-оклад над колодцем, ворот на двух рогатках, бадья на верёвке.
fn shaft(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    b.at(p.x, p.z);
    let w = 0.75;
    let m = b.mesh(Mat::Wood);
    for k in 0..4 {
        let yy = p.y - 0.1 + k as f32 * 0.17;
        let o = if k % 2 == 0 { 0.0 } else { 0.08 };
        m.beam(Vec3::new(p.x - w - 0.2, yy, p.z - w + o), Vec3::new(p.x + w + 0.2, yy, p.z - w + o), 0.08, 0.08, Vec3::Y);
        m.beam(Vec3::new(p.x - w - 0.2, yy, p.z + w - o), Vec3::new(p.x + w + 0.2, yy, p.z + w - o), 0.08, 0.08, Vec3::Y);
        m.beam(Vec3::new(p.x - w + o, yy + 0.08, p.z - w - 0.2), Vec3::new(p.x - w + o, yy + 0.08, p.z + w + 0.2), 0.08, 0.08, Vec3::Y);
        m.beam(Vec3::new(p.x + w - o, yy + 0.08, p.z - w - 0.2), Vec3::new(p.x + w - o, yy + 0.08, p.z + w + 0.2), 0.08, 0.08, Vec3::Y);
    }
    // Чёрный зев колодца.
    b.mesh(Mat::DarkStone).quad(
        [Vec3::new(p.x - w + 0.08, p.y - 0.5, p.z + w - 0.08), Vec3::new(p.x + w - 0.08, p.y - 0.5, p.z + w - 0.08), Vec3::new(p.x + w - 0.08, p.y - 0.5, p.z - w + 0.08), Vec3::new(p.x - w + 0.08, p.y - 0.5, p.z - w + 0.08)],
        Vec3::Y,
    );
    kit::hidden(b, p - Vec3::new(w + 0.2, 0.2, w + 0.2), p + Vec3::new(w + 0.2, 0.65, w + 0.2));
    // Рогатки и вал ворота (вдоль X), рукоять справа.
    let top = p.y + 1.05;
    let m = b.mesh(Mat::Wood);
    for sx in [-1.25f32, 1.25] {
        let base = Vec3::new(p.x + sx, p.y - 0.1, p.z);
        m.beam(base, Vec3::new(p.x + sx, top + 0.05, p.z), 0.08, 0.08, Vec3::Z);
        m.beam(Vec3::new(p.x + sx, top - 0.1, p.z), Vec3::new(p.x + sx, top + 0.22, p.z - 0.16), 0.04, 0.04, Vec3::X);
        m.beam(Vec3::new(p.x + sx, top - 0.1, p.z), Vec3::new(p.x + sx, top + 0.22, p.z + 0.16), 0.04, 0.04, Vec3::X);
    }
    let drum_a = Vec3::new(p.x - 1.35, top, p.z);
    let drum_b = Vec3::new(p.x + 1.35, top, p.z);
    b.mesh(Mat::Wood).beam(drum_a, drum_b, 0.13, 0.13, Vec3::Y);
    // Намотанная верёвка.
    b.mesh(Mat::Canvas).beam(Vec3::new(p.x - 0.35, top, p.z), Vec3::new(p.x + 0.25, top, p.z), 0.145, 0.145, Vec3::Y);
    // Рукоять-коленце.
    let m = b.mesh(Mat::Wood);
    m.beam(drum_b, drum_b + Vec3::X * 0.25, 0.03, 0.03, Vec3::Y);
    m.beam(drum_b + Vec3::X * 0.25, drum_b + Vec3::new(0.25, -0.4, 0.0), 0.03, 0.03, Vec3::X);
    m.beam(drum_b + Vec3::new(0.25, -0.4, 0.0), drum_b + Vec3::new(0.5, -0.4, 0.0), 0.028, 0.028, Vec3::Y);
    // Верёвка в колодец и бадья у края.
    b.mesh(Mat::Canvas).beam(Vec3::new(p.x, top - 0.13, p.z + 0.05), Vec3::new(p.x, p.y - 0.4, p.z + 0.05), 0.012, 0.012, Vec3::X);
    basket(b, p + Vec3::new(-1.7, 0.0, 0.7), 0.3, 0.45, Some(Mat::Limonite));
    lumps(b, p + Vec3::new(1.6, -0.05, 1.3), 0.7, 0.4, Mat::Limonite, rng);
}

/// Мочажина в западной балке: рыжая вода с плёнкой, кочки, копанки, кучи болотной руды.
fn bog(b: &mut Builder) {
    let mut rng = Rng::new(7713);
    let n = 9;
    for k in 0..n {
        let z = BOG_Z.0 + (BOG_Z.1 - BOG_Z.0) * (k as f32 + 0.5) / n as f32;
        let x = west_ravine_x(z) + rng.range(-5.0, 5.0);
        let p = g(x, z);
        b.at(p.x, p.z);
        let r = rng.range(1.3, 2.6);
        // Рыжий ил по краю и вода.
        ground_disc(b, Mat::Limonite, p, r + 0.6, 0.03);
        // Вода стоит по нижнему краю ямы.
        let low = (0..8).map(|k| { let a = k as f32 / 8.0 * TAU; ground_y(p.x + a.cos() * r * 0.6, p.z + a.sin() * r * 0.6) }).fold(p.y, f32::min);
        b.mesh(Mat::Water).disc(Vec3::new(p.x, low + 0.07, p.z), r * 0.8, 14, true);
        // Осока и рогоз по берегам.
        for _ in 0..6 {
            let a = rng.range(0.0, TAU);
            let c = g(p.x + a.cos() * (r + 0.6), p.z + a.sin() * (r + 0.6));
            stems(b, c, 7, rng.range(0.7, 1.3), 0.25, Mat::Thatch, if rng.chance(0.3) { 0.04 } else { 0.0 }, &mut rng);
        }
    }
    for (i, p) in bog_spots().into_iter().enumerate() {
        b.at(p.x, p.z);
        // Копанка: снятый дёрн, рыжее дно, куча комьев.
        ground_disc(b, Mat::Earth, p, 1.3, 0.05);
        ground_disc(b, Mat::Limonite, p, 0.8, 0.07);
        for k in 0..4 {
            let a = k as f32 / 4.0 * TAU + 0.5;
            b.mesh(Mat::Grass).obox(p + Vec3::new(a.cos() * 1.7, 0.12, a.sin() * 1.7), Vec3::new(0.3, 0.1, 0.22), a);
        }
        lumps(b, p + Vec3::new(1.8, 0.0, -0.8), 0.6, 0.35, Mat::Limonite, &mut rng);
        if i == 1 {
            spade_tool(b, p + Vec3::new(1.5, 0.15, -0.5), p + Vec3::new(1.8, 1.4, -0.4), Vec3::Z);
            basket(b, p + Vec3::new(-1.6, 0.0, 0.6), 0.28, 0.5, Some(Mat::Limonite));
        }
    }
    // Сушат болотную руду на плетне, прежде чем нести к горну.
    let bz = (BOG_Z.0 + BOG_Z.1) / 2.0;
    let d = g(west_ravine_x(bz) + 9.0, bz + 2.0);
    b.at(d.x, d.z);
    lumps(b, d, 1.1, 0.6, Mat::Limonite, &mut rng);
    lumps(b, d + Vec3::new(2.4, 0.0, 0.8), 0.8, 0.45, Mat::Limonite, &mut rng);
    kit::hidden(b, d - Vec3::new(0.8, 0.0, 0.8), d + Vec3::new(0.8, 0.5, 0.8));
}

/// Стан углежогов: шалаш, угольные площадки, поленницы, поваленные стволы, кострище.
fn charcoal_camp(b: &mut Builder) {
    let c = CHARCOAL_CAMP;
    let mut rng = Rng::new(9431);
    // Шалаш из жердей, крытый корой и дёрном, — по обычаю углежогов.
    let hp = g(c.x - 9.0, c.y - 9.0);
    b.at(hp.x, hp.z);
    for k in 0..14 {
        let a = k as f32 / 14.0 * TAU;
        if (a - 1.0).abs() < 0.3 {
            continue;
        }
        let foot = hp + Vec3::new(a.cos() * 2.0, -0.1, a.sin() * 2.0);
        b.mesh(Mat::Wood).beam(foot, hp + Vec3::Y * 3.4, 0.05, 0.05, Vec3::Y);
    }
    b.mesh(Mat::Earth).lathe(hp - Vec3::Y * 0.05, &[(2.05, 0.0), (1.55, 1.0), (0.85, 2.2), (0.25, 3.0), (0.0, 3.05)], 12);
    b.mesh(Mat::Thatch).lathe(hp + Vec3::Y * 2.2, &[(0.9, 0.0), (0.3, 0.8), (0.0, 0.85)], 10);
    // Тёмный вход (лаз) к стану.
    let a: f32 = 1.0;
    let door = hp + Vec3::new(a.cos() * 1.95, 0.0, a.sin() * 1.95);
    let t = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.45;
    b.mesh(Mat::DarkStone).quad([door - t, door + t, door + t * 0.4 + Vec3::Y * 1.3, door - t * 0.4 + Vec3::Y * 1.3], Vec3::new(a.cos(), 0.0, a.sin()));
    kit::hidden(b, hp - Vec3::new(1.5, 0.1, 1.5), hp + Vec3::new(1.5, 2.6, 1.5));
    // Угольные площадки: чёрная земля, обугленные колья, лопаты.
    for p in clamp_spots() {
        b.at(p.x, p.z);
        ground_disc(b, Mat::Soot, p, 3.6, 0.03);
        b.mesh(Mat::Coals).disc(p + Vec3::new(2.9, 0.045, -2.0), 0.4, 8, true);
        for _ in 0..3 {
            let ang = rng.range(0.0, TAU);
            let q = p + Vec3::new(ang.cos(), 0.0, ang.sin()) * rng.range(3.9, 4.6);
            let q = g(q.x, q.z);
            b.mesh(Mat::Grass).obox(q, Vec3::new(0.2, 0.05, 0.14), ang);
        }
    }
    // Поленницы (полуторааршинные дубовые поленья) и козлы для пилки.
    for (dx, dz, along) in [(3.0f32, 1.0f32, true), (6.0, -3.0, false), (-1.0, 9.0, true)] {
        let p = g(c.x + dx, c.y + dz);
        let r = if along { Rect::new(-1.8, -0.45, 1.8, 0.45) } else { Rect::new(-0.45, -1.8, 0.45, 1.8) };
        b.placed(p - Vec3::Y * 0.05, move |t| kit::woodpile(t, r, 1.1));
    }
    let sh = g(c.x + 6.0, c.y + 4.5);
    b.at(sh.x, sh.z);
    let m = b.mesh(Mat::Wood);
    for sx in [-0.6f32, 0.6] {
        m.beam(sh + Vec3::new(sx - 0.3, 0.0, -0.3), sh + Vec3::new(sx + 0.3, 0.9, 0.3), 0.04, 0.04, Vec3::Y);
        m.beam(sh + Vec3::new(sx + 0.3, 0.0, -0.3), sh + Vec3::new(sx - 0.3, 0.9, 0.3), 0.04, 0.04, Vec3::Y);
    }
    m.beam(sh + Vec3::new(-1.1, 0.7, 0.0), sh + Vec3::new(1.2, 0.72, 0.05), 0.12, 0.12, Vec3::Y);
    kit::hidden(b, sh - Vec3::new(0.9, 0.0, 0.4), sh + Vec3::new(0.9, 0.85, 0.4));
    // Корзины с готовым углём, грабли и лестница (на кучу влезают пробивать отдушины).
    for (dx, dz) in [(-4.5f32, 3.0f32), (-5.2, 3.6), (-4.0, 3.9)] {
        basket(b, g(c.x + dx, c.y + dz), 0.32, 0.6, Some(Mat::Coals));
    }
    let lp = g(c.x - 2.0, c.y + 6.0);
    let m = b.mesh(Mat::Wood);
    for sx in [-0.22f32, 0.22] {
        m.beam(lp + Vec3::new(sx, 0.0, 0.0), lp + Vec3::new(sx, 2.6, 0.8), 0.03, 0.03, Vec3::X);
    }
    for k in 0..7 {
        let t = (k as f32 + 0.5) / 7.0;
        m.beam(lp + Vec3::new(-0.24, 2.6 * t, 0.8 * t), lp + Vec3::new(0.24, 2.6 * t, 0.8 * t), 0.02, 0.02, Vec3::Y);
    }
    let rk = g(c.x + 0.5, c.y + 5.5);
    b.mesh(Mat::Wood).beam(rk, rk + Vec3::new(0.3, 1.5, 0.5), 0.02, 0.02, Vec3::X);
    b.mesh(Mat::Wood).beam(rk + Vec3::new(-0.25, 0.0, -0.05), rk + Vec3::new(0.25, 0.0, 0.05), 0.02, 0.05, Vec3::Y);
    campfire(b, g(c.x - 3.0, c.y - 3.5));
    // Дубовый лес вокруг поляны (уголь жгут там, где рубят), просвет — к тропе.
    let road = Vec2::new(6.0, -16.0).normalize();
    let mut k = 0;
    while k < 70 {
        k += 1;
        let a = k as f32 * 2.399;
        let r = 27.0 + (k as f32 * 7.31).sin().abs() * 16.0;
        let d = Vec2::new(a.cos(), a.sin());
        if d.dot(road) > 0.93 {
            continue;
        }
        let q = c + d * r + Vec2::new(rng.range(-2.0, 2.0), rng.range(-2.0, 2.0));
        b.at(q.x, q.y);
        let p = g(q.x, q.y) - Vec3::Y * 0.1;
        if rng.chance(0.78) {
            kit::oak(b, p, rng.range(7.0, 11.0), &mut rng);
        } else {
            kit::pine(b, p, rng.range(9.0, 13.0), &mut rng);
        }
    }
    // Делянка: поваленные дубы с обрубленными сучьями, пни, щепа, колода для колки.
    for (root, a) in felled() {
        let dir = Vec2::new(a.cos(), a.sin());
        let p0 = g(root.x, root.y);
        let p1 = g(root.x + dir.x * 7.5, root.y + dir.y * 7.5);
        b.at(p0.x, p0.z);
        b.mesh(Mat::Wood).ridge_pipe(p0 + Vec3::Y * 0.3, p1 + Vec3::Y * 0.2, 0.3, 9);
        b.mesh(Mat::Wood).ridge_pipe(p1 + Vec3::Y * 0.2, p1 + Vec3::new(dir.x * 2.0, 0.15, dir.y * 2.0), 0.2, 7);
        kit::hidden(b, p0.min(p1) - Vec3::new(0.3, 0.0, 0.3), p0.max(p1) + Vec3::new(0.3, 0.55, 0.3));
        let stump = g(root.x - dir.x * 1.2, root.y - dir.y * 1.2);
        b.mesh(Mat::Wood).lathe(stump - Vec3::Y * 0.1, &[(0.42, 0.0), (0.36, 0.25), (0.34, 0.55), (0.0, 0.58)], 10);
        kit::hidden(b, stump - Vec3::new(0.35, 0.1, 0.35), stump + Vec3::new(0.35, 0.55, 0.35));
        let side = Vec2::new(-dir.y, dir.x);
        let ch = root + dir * 2.5 + side * 1.6;
        b.mesh(Mat::Thatch).disc(g(ch.x, ch.y) + Vec3::Y * 0.03, 1.0, 10, true);
        let blk = g(ch.x, ch.y);
        b.mesh(Mat::Wood).lathe(blk, &[(0.3, 0.0), (0.3, 0.5), (0.0, 0.52)], 10);
        kit::hidden(b, blk - Vec3::new(0.3, 0.0, 0.3), blk + Vec3::new(0.3, 0.5, 0.3));
        for _ in 0..3 {
            let q = ch + Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
            let qp = g(q.x, q.y) + Vec3::Y * 0.08;
            let ang = rng.range(0.0, TAU);
            b.mesh(Mat::Wood).beam(qp, qp + Vec3::new(ang.cos(), 0.0, ang.sin()) * 0.7, 0.07, 0.06, Vec3::Y);
        }
    }
}

/// Глиняный карьер в восточной балке: выемка, отвалы, отмучивание глины в корыте.
fn clay_pit(b: &mut Builder) {
    let c = CLAY_PIT;
    let mut rng = Rng::new(3307);
    let p = g(c.x, c.y);
    b.at(p.x, p.z);
    b.mesh(Mat::Earth).disc(p + Vec3::new(0.0, 0.04, 1.0), 5.0, 18, true);
    b.mesh(Mat::Earth).lathe(p - Vec3::Y * 0.1, &[(6.2, 0.0), (6.0, 0.45), (5.4, 0.6), (5.1, 0.2)], 20);
    b.mesh(Mat::Water).disc(p + Vec3::new(1.5, 0.08, 2.4), 1.2, 12, true);
    lumps(b, g(c.x - 4.0, c.y - 3.0), 0.8, 0.5, Mat::Earth, &mut rng);
    lumps(b, g(c.x + 5.0, c.y - 2.0), 0.7, 0.45, Mat::Earth, &mut rng);
    // Корыто для отмучивания.
    let t = g(c.x - 4.5, c.y + 3.5);
    b.placed(t, |t| {
        t.solid([-1.0, 0.0, -0.35], [1.0, 0.45, 0.35], Mat::Wood);
        t.decor([-0.92, 0.3, -0.28], [0.92, 0.42, 0.28], Mat::Water);
    });
    spade_tool(b, p + Vec3::new(-1.5, 0.1, -2.6), p + Vec3::new(-1.3, 1.35, -2.4), Vec3::X);
    basket(b, g(c.x + 3.5, c.y + 4.0), 0.28, 0.5, Some(Mat::Earth));
}

/// Песок у Песочной бухты: насыпанная куча и мешки.
fn sand(b: &mut Builder) {
    let mut rng = Rng::new(4410);
    for (dx, dz) in [(0.0f32, 0.0f32), (-2.0, 9.0)] {
        let p = g(SAND_BEACH.x + dx, SAND_BEACH.y + dz);
        b.at(p.x, p.z);
        ground_disc(b, Mat::Shingle, p, 3.2, 0.03);
        mound(b, p + Vec3::new(1.6, -0.05, 1.0), 1.1, 0.55, Mat::Shingle);
    }
    let p = g(SAND_BEACH.x + 3.0, SAND_BEACH.y + 4.0);
    for k in 0..3 {
        let q = p + Vec3::new(k as f32 * 0.55, 0.0, rng.range(-0.2, 0.2));
        b.mesh(Mat::Canvas).lathe(q, &[(0.2, 0.0), (0.27, 0.2), (0.25, 0.45), (0.12, 0.55), (0.05, 0.6)], 8);
    }
    kit::hidden(b, p - Vec3::new(0.3, 0.0, 0.3), p + Vec3::new(1.4, 0.55, 0.3));
}

/// Каменоломня: ступенчатый забой в известняке, отколотые блоки, клинья, щебень, тренога.
fn quarry(b: &mut Builder) {
    let c = QUARRY;
    let mut rng = Rng::new(2207);
    let y = terrace_levels(Rect::new(c.x - 8.0, c.y - 6.0, c.x + 8.0, c.y + 4.0)).0;
    b.at(c.x, c.y);
    // Уступы забоя (к югу), верх — на уровне земли.
    let top = ground_y(c.x, c.y + 6.0).max(y + 2.4);
    for (k, (z0, h)) in [(0.0f32, 1.2f32), (1.4, 2.2), (2.8, top - y)].into_iter().enumerate() {
        let w = 7.5 - k as f32 * 0.4;
        b.solid([c.x - w, y - 0.5, c.y + z0], [c.x + w, y + h, c.y + 6.0], Mat::Bedrock);
    }
    // Срезанный склон: за уступами — земля к рельефу, по краям — глыбы.
    let (gb, gl, gr) = (ground_y(c.x, c.y + 14.0), ground_y(c.x - 10.5, c.y + 3.0), ground_y(c.x + 10.5, c.y + 3.0));
    let m = b.mesh(Mat::Terrain);
    m.quad([Vec3::new(c.x + 6.7, top, c.y + 6.0), Vec3::new(c.x - 6.7, top, c.y + 6.0), Vec3::new(c.x - 9.5, gb.min(top), c.y + 14.0), Vec3::new(c.x + 9.5, gb.min(top), c.y + 14.0)], Vec3::Y);
    m.quad([Vec3::new(c.x - 6.7, top, c.y + 6.0), Vec3::new(c.x - 6.7, top, c.y + 2.8), Vec3::new(c.x - 10.5, gl.min(top), c.y + 2.0), Vec3::new(c.x - 9.5, gb.min(top), c.y + 14.0)], Vec3::Y);
    m.quad([Vec3::new(c.x + 6.7, top, c.y + 2.8), Vec3::new(c.x + 6.7, top, c.y + 6.0), Vec3::new(c.x + 9.5, gb.min(top), c.y + 14.0), Vec3::new(c.x + 10.5, gr.min(top), c.y + 2.0)], Vec3::Y);
    for (dx, dz, s) in [(-7.6f32, 0.6f32, 1.6f32), (-8.2, 2.6, 2.0), (7.8, 0.8, 1.7), (8.3, 2.8, 2.1), (-3.0, 6.4, 1.2), (2.5, 6.6, 1.4), (-6.0, 7.5, 1.6), (6.2, 7.2, 1.5)] {
        b.plant(PlantKind::Rock, Vec3::new(c.x + dx, y + if dz > 5.0 { top - y - 0.5 } else { -0.2 }, c.y + dz), s, rng.range(0.0, TAU), rng.int(64) as u32);
    }
    // Следы клиньев на гранях блоков.
    for k in 0..14 {
        let x = c.x - 6.5 + k as f32;
        b.mesh(Mat::DarkStone).cuboid(Vec3::new(x, y + 1.0, c.y - 0.03), Vec3::new(x + 0.12, y + 1.15, c.y + 0.02));
    }
    // Отколотые блоки рядами и щебень.
    for i in 0..4 {
        for j in 0..2 {
            let p = Vec3::new(c.x - 5.0 + i as f32 * 1.6, y, c.y - 5.0 - j as f32 * 1.3);
            b.solid((p + Vec3::new(-0.6, 0.0, -0.45)).to_array(), (p + Vec3::new(0.6, rng.range(0.7, 0.9), 0.45)).to_array(), Mat::Limestone);
        }
    }
    lumps(b, Vec3::new(c.x + 6.5, y - 0.05, c.y - 4.0), 2.0, 1.0, Mat::Bedrock, &mut rng);
    ground_disc(b, Mat::Shingle, Vec3::new(c.x, y, c.y - 2.5), 6.0, 0.04);
    // Тренога для подъёма блоков.
    let t = Vec3::new(c.x + 3.0, y, c.y - 3.5);
    let apex = t + Vec3::Y * 3.6;
    for k in 0..3 {
        let a = k as f32 / 3.0 * TAU;
        b.mesh(Mat::Wood).beam(t + Vec3::new(a.cos() * 1.4, 0.0, a.sin() * 1.4), apex, 0.07, 0.07, Vec3::Y);
    }
    b.mesh(Mat::Canvas).beam(apex, t + Vec3::Y * 1.2, 0.015, 0.015, Vec3::X);
    pick_tool(b, Vec3::new(c.x - 1.0, y, c.y - 0.3), Vec3::new(c.x - 0.9, y + 1.0, c.y - 0.05), Vec3::X);
}
