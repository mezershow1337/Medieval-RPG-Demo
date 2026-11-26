//! Суда Карантинной бухты — свои модели по образцу находок VI–VII вв. (Яссыада, Дор, Йеникапы):
//! - навикулярий-торговец: пузатый корпус ~20 м, одна мачта с латинским парусом, два рулевых
//!   весла, камбуз с черепичной кровлей на корме, амфоры на палубе;
//! - каботажник (кондура) поменьше — ходит вдоль Таврики до Боспора;
//! - дромон: длинная узкая гребная галера флота с шпороном вместо тарана, двумя мачтами,
//!   щитами по борту и вёслами, вынутыми из воды на стоянке.
//!
//! Корпус — гладкая оболочка с плавными нормалями (обшивка вгладь), поверх — привальные брусья,
//! окрашенный пояс, концы бимсов, штевни с завитком; палуба, по которой можно ходить.

use super::kit;
use super::*;
use crate::collision::Aabb;
use crate::level::geom::MeshBuilder;

/// Что с парусом.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sail {
    /// Подобран к рее.
    Furled,
    /// Отдан сушиться: висит треугольником, чуть выгнут ветром.
    Loose,
}

/// Мачта: где стоит (0 — корма, 1 — нос), высота над палубой, длина реи.
#[derive(Clone, Copy)]
pub struct Mast {
    pub t: f32,
    pub h: f32,
    pub yard: f32,
}

pub struct Ship {
    pub len: f32,
    pub beam: f32,
    /// От киля до планширя в середине.
    pub depth: f32,
    /// Осадка: насколько киль ниже воды.
    pub draft: f32,
    /// Подъём бортов к оконечностям.
    pub sheer: f32,
    /// Острота носа (больше — острее).
    pub bow: f32,
    pub masts: &'static [Mast],
    pub sail: Sail,
    /// Вёсел на борт (0 — парусник).
    pub oars: usize,
    pub galley: bool,
    /// Шпорон (надводный таран) у дромона.
    pub spur: bool,
    /// Щиты по борту.
    pub shields: bool,
    /// Окрашенный пояс под планширем.
    pub stripe: Mat,
    /// Рядов амфор на палубе.
    pub cargo: usize,
    /// Якорь отдан (канат уходит в воду) или висит у носа.
    pub anchored: bool,
}

pub const NAVICULARIUS: Ship = Ship {
    len: 20.0,
    beam: 5.4,
    depth: 3.0,
    draft: 1.45,
    sheer: 0.9,
    bow: 1.8,
    masts: &[Mast { t: 0.6, h: 11.0, yard: 15.0 }],
    sail: Sail::Furled,
    oars: 0,
    galley: true,
    spur: false,
    shields: false,
    stripe: Mat::Ochre,
    cargo: 5,
    anchored: false,
};

pub const ALEXANDRIAN: Ship = Ship {
    len: 24.0,
    beam: 6.4,
    depth: 3.3,
    draft: 1.7,
    sheer: 1.1,
    bow: 1.6,
    masts: &[Mast { t: 0.62, h: 13.0, yard: 18.0 }, Mast { t: 0.9, h: 6.0, yard: 7.5 }],
    sail: Sail::Loose,
    oars: 0,
    galley: true,
    spur: false,
    shields: false,
    stripe: Mat::Cloth,
    cargo: 7,
    anchored: false,
};

pub const KONDURA: Ship = Ship {
    len: 13.0,
    beam: 3.8,
    depth: 2.3,
    draft: 1.0,
    sheer: 0.6,
    bow: 2.0,
    masts: &[Mast { t: 0.58, h: 8.0, yard: 10.0 }],
    sail: Sail::Furled,
    oars: 4,
    galley: false,
    spur: false,
    shields: false,
    stripe: Mat::Rug,
    cargo: 2,
    anchored: false,
};

pub const DROMON: Ship = Ship {
    len: 31.0,
    beam: 4.6,
    depth: 2.4,
    draft: 1.0,
    sheer: 0.8,
    bow: 2.6,
    masts: &[Mast { t: 0.66, h: 11.0, yard: 15.0 }, Mast { t: 0.36, h: 8.5, yard: 11.0 }],
    sail: Sail::Furled,
    oars: 12,
    galley: false,
    spur: true,
    shields: true,
    stripe: Mat::Banner,
    cargo: 0,
    anchored: true,
};

/// Что нужно знать о поставленном судне: высота палубы, точки у бортов в середине.
pub struct Moored {
    pub deck_y: f32,
    /// Середина палубы.
    pub mid: Vec3,
}

/// Корабельные координаты: `t` — от кормы (0) к носу (1), `u` — от киля (0) к планширю (1), `sd` — борт.
struct Frame<'a> {
    s: &'a Ship,
    o: Vec3,
    fwd: Vec3,
    side: Vec3,
    keel: f32,
}

impl Frame<'_> {
    fn half(&self, t: f32, u: f32) -> f32 {
        let e = 2.0 * t - 1.0;
        let ex = if e > 0.0 { e.powf(self.s.bow) } else { (-e).powf(2.3) };
        let full = (1.0 - ex).max(0.0).sqrt();
        self.s.beam / 2.0 * full * u.max(0.0).powf(0.5)
    }

    fn y(&self, t: f32, u: f32) -> f32 {
        let e = 2.0 * t - 1.0;
        // Киль чуть поднимается к оконечностям, борт — сильнее (седловатость).
        let rocker = 0.25 * self.s.depth * e.powi(4) * (1.0 - u);
        self.keel + self.s.depth * u + self.s.sheer * e * e * u * u + rocker
    }

    fn pt(&self, t: f32, u: f32, sd: f32) -> Vec3 {
        let x = (t - 0.5) * self.s.len;
        self.o + self.fwd * x + self.side * (self.half(t, u) * sd) + Vec3::Y * self.y(t, u)
    }

    /// Наружу от борта в точке (горизонтально).
    fn out(&self, sd: f32) -> Vec3 {
        self.side * sd
    }
}

const DECK_U: f32 = 0.84;

/// Поставить судно: `at` — середина по воде (y — уровень воды), `yaw` — куда смотрит нос.
pub fn ship(b: &mut Builder, s: &Ship, at: Vec3, yaw: f32) -> Moored {
    let (sn, cs) = yaw.sin_cos();
    let fwd = Vec3::new(cs, 0.0, sn);
    let side = Vec3::new(-sn, 0.0, cs);
    let o = Vec3::new(at.x, 0.0, at.z);
    let f = Frame { s, o, fwd, side, keel: at.y - s.draft };
    b.at(at.x, at.z);

    // ---- Корпус: гладкая обшивка с двух сторон (снаружи и изнутри — видно с палубы).
    let (nt, nu) = (36usize, 10usize);
    for sd in [-1.0f32, 1.0] {
        let g: Vec<Vec<Vec3>> = (0..=nt).map(|i| (0..=nu).map(|j| f.pt(i as f32 / nt as f32, j as f32 / nu as f32, sd)).collect()).collect();
        let out = f.out(sd);
        smooth_grid(b.mesh(Mat::Wood), &g, |p, i| {
            // Снаружи — от диаметральной плоскости; у днища — вниз.
            let t = i as f32 / nt as f32;
            let c = f.o + f.fwd * ((t - 0.5) * s.len) + Vec3::Y * p.y;
            ((p - c).normalize_or_zero() + out * 0.3 + Vec3::NEG_Y * 0.2).normalize_or_zero()
        });
    }

    // ---- Окрашенный пояс под планширем и привальные брусья.
    for sd in [-1.0f32, 1.0] {
        let out = f.out(sd);
        let band = b.mesh(s.stripe);
        let n = 30;
        for i in 0..n {
            let (t0, t1) = (0.04 + 0.92 * i as f32 / n as f32, 0.04 + 0.92 * (i + 1) as f32 / n as f32);
            let lift = |p: Vec3| p + out * 0.025;
            let q = [lift(f.pt(t0, 0.86, sd)), lift(f.pt(t1, 0.86, sd)), lift(f.pt(t1, 0.95, sd)), lift(f.pt(t0, 0.95, sd))];
            band.quad(q, out);
        }
        for (u, hw) in [(0.52f32, 0.07f32), (0.72, 0.08), (1.0, 0.09)] {
            let pts: Vec<Vec3> = (0..=24).map(|i| f.pt(0.02 + 0.96 * i as f32 / 24.0, u, sd) + out * 0.05 + Vec3::Y * if u >= 1.0 { 0.04 } else { 0.0 }).collect();
            b.mesh(Mat::Timber).beam_path(&pts, hw, 0.06, out);
        }
    }

    // ---- Киль, форштевень и ахтерштевень с завитком.
    let keel: Vec<Vec3> = (0..=16).map(|i| f.pt(i as f32 / 16.0, 0.0, 0.0) - Vec3::Y * 0.08).collect();
    b.mesh(Mat::Timber).beam_path(&keel, 0.12, 0.12, Vec3::Y);
    let bow_top = f.pt(1.0, 1.0, 0.0);
    let mut stem: Vec<Vec3> = (0..=8).map(|j| f.pt(1.0, j as f32 / 8.0, 0.0) + fwd * 0.05).collect();
    stem.push(bow_top + fwd * 0.35 + Vec3::Y * 0.7);
    stem.push(bow_top + fwd * 0.25 + Vec3::Y * 1.15);
    stem.push(bow_top + fwd * 0.0 + Vec3::Y * 1.3);
    b.mesh(Mat::Wood).beam_path(&stem, 0.13, 0.13, side);
    let st_top = f.pt(0.0, 1.0, 0.0);
    let mut stern: Vec<Vec3> = (0..=8).map(|j| f.pt(0.0, j as f32 / 8.0, 0.0) - fwd * 0.05).collect();
    for k in 1..=7 {
        // Завиток: дуга вверх и внутрь.
        let a = k as f32 / 7.0 * 2.6;
        stern.push(st_top - fwd * (0.9 * a.sin()) + Vec3::Y * (0.9 * (1.0 - a.cos()) + 0.3 * a));
    }
    b.mesh(Mat::Wood).beam_path(&stern, 0.13, 0.12, side);

    // ---- Палуба (по седловатости), концы бимсов, люк.
    let deck_at = |t: f32| f.y(t, DECK_U);
    {
        let deck = b.mesh(Mat::Wood);
        let n = 24;
        for i in 0..n {
            let (t0, t1) = (0.03 + 0.94 * i as f32 / n as f32, 0.03 + 0.94 * (i + 1) as f32 / n as f32);
            let q = [f.pt(t0, DECK_U, -1.0), f.pt(t1, DECK_U, -1.0), f.pt(t1, DECK_U, 1.0), f.pt(t0, DECK_U, 1.0)];
            deck.quad(q, Vec3::Y);
        }
    }
    let nb = (s.len / 2.2) as usize;
    for k in 1..nb {
        let t = 0.12 + 0.76 * k as f32 / nb as f32;
        let a = f.pt(t, 0.7, -1.0) - side * 0.28;
        let c = f.pt(t, 0.7, 1.0) + side * 0.28;
        b.mesh(Mat::Timber).beam(a, c, 0.09, 0.09, fwd);
    }
    let mid_t = 0.47;
    let mid = f.o + fwd * ((mid_t - 0.5) * s.len) + Vec3::Y * deck_at(mid_t);
    if s.cargo > 0 {
        b.mesh(Mat::Timber).obox(mid + Vec3::Y * 0.12, Vec3::new(1.1, 0.12, 0.9), yaw);
        b.mesh(Mat::Wood).obox(mid + Vec3::Y * 0.2, Vec3::new(0.95, 0.06, 0.75), yaw);
    }

    // ---- Камбуз на корме под черепицей (Яссыада).
    if s.galley {
        let t = 0.17;
        let g = f.o + fwd * ((t - 0.5) * s.len) + Vec3::Y * deck_at(t);
        let hw = (f.half(t, DECK_U) - 0.3).min(1.6);
        b.mesh(Mat::Wood).obox(g + Vec3::Y * 0.95, Vec3::new(1.4, 0.95, hw), yaw);
        // Двускатная крыша.
        let r = b.mesh(Mat::Tile);
        for sd in [-1.0f32, 1.0] {
            let e0 = g + Vec3::Y * 1.85 + side * sd * (hw + 0.25);
            let ridge = g + Vec3::Y * 2.45;
            let a = fwd * 1.65;
            r.quad([e0 - a, e0 + a, ridge + a, ridge - a], Vec3::Y + side * sd);
            r.quad([e0 - a, e0 + a, ridge + a, ridge - a], -(Vec3::Y + side * sd));
        }
        // Фронтоны.
        for e in [-1.0f32, 1.0] {
            let c = g + fwd * (1.4 * e);
            b.mesh(Mat::Wood).tri(c + Vec3::Y * 1.85 - side * hw, c + Vec3::Y * 1.85 + side * hw, c + Vec3::Y * 2.4, fwd * e);
        }
        // Дверца к мачте и печная труба.
        b.mesh(Mat::Timber).obox(g + fwd * 1.42 + Vec3::Y * 0.75, Vec3::new(0.03, 0.75, 0.42), yaw);
        b.mesh(Mat::Ceramic).lathe(g - fwd * 0.6 + side * 0.6 + Vec3::Y * 2.0, &[(0.14, 0.0), (0.12, 0.7), (0.15, 0.75), (0.0, 0.76)], 8);
        let gb = (g - fwd * 1.4 - side * hw, g + fwd * 1.4 + side * hw);
        let (lo, hi) = (gb.0.min(gb.1), gb.0.max(gb.1));
        b.invisible.push(Aabb::new(Vec3::new(lo.x, deck_at(t), lo.z), Vec3::new(hi.x, deck_at(t) + 1.9, hi.z)));
    }

    // ---- Мачты, реи, паруса, снасти.
    for (k, m) in s.masts.iter().enumerate() {
        let base = f.o + fwd * ((m.t - 0.5) * s.len) + Vec3::Y * deck_at(m.t);
        let r0 = 0.12 + m.h * 0.008;
        b.mesh(Mat::Wood).lathe(base - Vec3::Y * 0.4, &[(r0, 0.0), (r0 * 0.75, m.h + 0.4), (r0 * 0.5, m.h + 0.6), (0.0, m.h + 0.65)], 10);
        let top = base + Vec3::Y * m.h;
        // Топ мачты: блок-калкет.
        b.mesh(Mat::Timber).obox(top - Vec3::Y * 0.3, Vec3::new(0.16, 0.25, 0.16), yaw);
        // Латинская рея: нок у кормы высоко, галс у носа низко.
        let ang = 0.42f32;
        let peak_dir = (-fwd * ang.cos() + Vec3::Y * ang.sin()).normalize();
        let yc = top - Vec3::Y * 0.7;
        let peak = yc + peak_dir * m.yard * 0.58;
        let mut tack = yc - peak_dir * m.yard * 0.42;
        let tack_min = deck_at(m.t.min(0.98)) + 1.4;
        if tack.y < tack_min {
            tack.y = tack_min;
        }
        let yard = b.mesh(Mat::Wood);
        // Рея из двух сращенных брусьев (утончается к концам).
        let ymid = (peak + tack) / 2.0;
        yard.beam(tack, ymid, 0.06, 0.06, side);
        yard.beam(ymid, peak, 0.07, 0.07, side);
        // Фал от топа к рее.
        b.mesh(Mat::Leather).beam(top, yc + side * 0.12, 0.018, 0.018, fwd);
        match s.sail {
            Sail::Furled => {
                let d = (peak - tack).normalize();
                let c = b.mesh(Mat::Canvas);
                let pts: Vec<Vec3> = (0..=10)
                    .map(|i| {
                        let a = i as f32 / 10.0;
                        // Скатка толще посередине, провисает между сезнями.
                        let sag = 0.08 * ((a * 10.0) * std::f32::consts::PI).sin().abs();
                        tack + d * 0.4 + (peak - tack - d * 0.8) * a - Vec3::Y * (0.22 + sag)
                    })
                    .collect();
                c.beam_path(&pts, 0.17, 0.22, side);
                // Сезни поверх скатки.
                for i in 1..10 {
                    let p = pts[i];
                    b.mesh(Mat::Leather).beam(p + Vec3::Y * 0.26, p - Vec3::Y * 0.26, 0.2, 0.02, d);
                }
            }
            Sail::Loose => {
                let t_clew = (m.t - 0.22 * (m.yard / s.len)).max(0.08);
                let clew = f.o + fwd * ((t_clew - 0.5) * s.len) + Vec3::Y * (deck_at(t_clew) + 1.8) + side * 0.4;
                let billow = side * (0.06 * m.yard);
                sail_cloth(b.mesh(Mat::Canvas), tack - Vec3::Y * 0.1, peak - Vec3::Y * 0.1, clew, billow);
                // Шкот к борту.
                let cleat = f.pt(t_clew, 1.0, 1.0);
                b.mesh(Mat::Leather).beam(clew, cleat, 0.015, 0.015, fwd);
            }
        }
        // Ванты к бортам, штаг к носу.
        let sh_t = (m.t - 0.04).max(0.05);
        for sd in [-1.0f32, 1.0] {
            for dt in [0.0f32, 0.05] {
                b.mesh(Mat::Leather).beam(top - Vec3::Y * 0.2, f.pt(sh_t - dt, 1.0, sd) + side * sd * 0.08, 0.014, 0.014, fwd);
            }
        }
        if k == 0 {
            b.mesh(Mat::Leather).beam(top - Vec3::Y * 0.2, bow_top + Vec3::Y * 0.6, 0.014, 0.014, side);
        }
        // Вымпел на топе.
        let p0 = top + Vec3::Y * 0.6;
        b.mesh(Mat::Banner).tri(p0, p0 - Vec3::Y * 0.5, p0 - fwd * 1.6 + side * 0.15 - Vec3::Y * 0.2, side);
        b.mesh(Mat::Banner).tri(p0, p0 - Vec3::Y * 0.5, p0 - fwd * 1.6 + side * 0.15 - Vec3::Y * 0.2, -side);
    }

    // ---- Рулевые вёсла у кормы на брусе-подзоре.
    for sd in [-1.0f32, 1.0] {
        let t = 0.1;
        let pivot = f.pt(t, 1.0, sd) + side * sd * 0.35 + Vec3::Y * 0.3;
        let blade = Vec3::new(pivot.x, f.keel + 0.2, pivot.z) - fwd * 1.4 + side * sd * 0.25;
        b.mesh(Mat::Timber).beam(f.pt(t, 0.95, sd), pivot + side * sd * 0.1, 0.1, 0.1, fwd);
        let wood = b.mesh(Mat::Wood);
        wood.beam(pivot + Vec3::Y * 0.9 + fwd * 0.3, blade, 0.07, 0.07, side);
        let d = (blade - pivot).normalize();
        let bc = blade - d * 0.7;
        let w = fwd.cross(Vec3::Y).cross(d).normalize_or_zero();
        let wd = if w.dot(fwd) < 0.0 { -w } else { w };
        wood.quad([bc - d * 0.8 - wd * 0.05, bc + d * 0.8 - wd * 0.05, bc + d * 0.8 + wd * 0.5, bc - d * 0.8 + wd * 0.5], side);
        wood.quad([bc - d * 0.8 - wd * 0.05, bc + d * 0.8 - wd * 0.05, bc + d * 0.8 + wd * 0.5, bc - d * 0.8 + wd * 0.5], -side);
        // Румпель внутрь.
        b.mesh(Mat::Wood).beam(pivot + Vec3::Y * 0.8 + fwd * 0.28, pivot + Vec3::Y * 0.9 + fwd * 0.3 - side * sd * 1.1, 0.035, 0.035, Vec3::Y);
    }

    // ---- Вёсла: вынуты, лежат на уключинах лопастями над водой.
    if s.oars > 0 {
        for sd in [-1.0f32, 1.0] {
            for i in 0..s.oars {
                let t = 0.24 + 0.5 * i as f32 / (s.oars.max(2) - 1) as f32;
                let port = f.pt(t, 0.9, sd) + side * sd * 0.06;
                let reach = 2.2 + s.beam * 0.45;
                let tip = port + side * sd * reach - Vec3::Y * (port.y - at.y - 0.25) - fwd * 0.5;
                let inner = port - (tip - port).normalize() * 1.2;
                let w = b.mesh(Mat::Wood);
                w.beam(inner, tip, 0.035, 0.035, fwd);
                let d = (tip - port).normalize();
                let bl = tip - d * 0.55;
                w.quad([bl - d * 0.55 - fwd * 0.1, bl + d * 0.55 - fwd * 0.1, bl + d * 0.55 + fwd * 0.1, bl - d * 0.55 + fwd * 0.1], Vec3::Y);
                w.quad([bl - d * 0.55 - fwd * 0.1, bl + d * 0.55 - fwd * 0.1, bl + d * 0.55 + fwd * 0.1, bl - d * 0.55 + fwd * 0.1], Vec3::NEG_Y);
            }
        }
    }

    // ---- Шпорон дромона: окованный брус над водой.
    if s.spur {
        let root = f.pt(0.985, 0.55, 0.0);
        let tip = root + fwd * 3.4 + Vec3::Y * 0.35;
        b.mesh(Mat::Timber).beam(root - fwd * 0.6, tip, 0.13, 0.16, side);
        b.mesh(Mat::Bronze).beam(tip - (tip - root).normalize() * 0.6, tip + (tip - root).normalize() * 0.12, 0.15, 0.18, side);
        // Носовая площадка для лучников.
        let t = 0.9;
        let c = f.o + fwd * ((t - 0.5) * s.len) + Vec3::Y * deck_at(t);
        let hw = f.half(t, DECK_U) * 0.85;
        b.mesh(Mat::Wood).obox(c + Vec3::Y * 1.0, Vec3::new(1.1, 0.06, hw), yaw);
        for (a, z) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let p = c + fwd * (a * 0.95) + side * (z * (hw - 0.1));
            b.mesh(Mat::Timber).beam(p, p + Vec3::Y * 1.0, 0.06, 0.06, fwd);
        }
        b.mesh(Mat::Wood).obox(c + Vec3::Y * 1.45, Vec3::new(1.1, 0.4, 0.03), yaw);
    }

    // ---- Щиты по бортам.
    if s.shields {
        let n = (s.len / 1.6) as usize;
        for sd in [-1.0f32, 1.0] {
            for i in 0..n {
                let t = 0.18 + 0.64 * i as f32 / (n - 1) as f32;
                let c = f.pt(t, 0.98, sd) + side * sd * 0.12;
                let mat = if i % 2 == 0 { Mat::Banner } else { Mat::Cloth };
                let mut pts = Vec::new();
                let mut uvs = Vec::new();
                for k in 0..14 {
                    let a = k as f32 / 14.0 * std::f32::consts::TAU;
                    pts.push(c + fwd * (0.42 * a.cos()) + Vec3::Y * (0.42 * a.sin()));
                    uvs.push(Vec2::new(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()));
                }
                b.mesh(mat).polygon(&pts, &uvs, side * sd);
                b.mesh(Mat::Bronze).lathe(c + side * sd * 0.02, &[(0.0, 0.0)], 4);
                let boss = c + side * sd * 0.06;
                b.mesh(Mat::Bronze).obox(boss, Vec3::new(0.08, 0.08, 0.05), yaw);
            }
        }
    }

    // ---- Якорь: железный, с деревянным штоком (тип Яссыады).
    {
        let sd = 1.0;
        let cat = f.pt(0.94, 1.0, sd) + side * 0.5 + Vec3::Y * 0.2;
        b.mesh(Mat::Timber).beam(f.pt(0.94, 1.0, sd) - side * 0.2, cat + side * 0.1, 0.08, 0.08, fwd);
        if s.anchored {
            // Канат уходит в воду наискосок вперёд.
            let into = Vec3::new(cat.x, at.y - 0.1, cat.z) + fwd * 4.5 + side * 1.5;
            b.mesh(Mat::Leather).beam(cat, into, 0.03, 0.03, side);
        } else {
            let shank_top = cat - Vec3::Y * 0.2;
            let crown = shank_top - Vec3::Y * 1.7;
            let iron = b.mesh(Mat::Iron);
            iron.beam(shank_top, crown, 0.05, 0.05, fwd);
            for a in [-1.0f32, 1.0] {
                iron.beam(crown, crown + fwd * (0.55 * a) + Vec3::Y * 0.5, 0.045, 0.045, side);
            }
            b.mesh(Mat::Timber).beam(shank_top - Vec3::Y * 0.2 - side * 0.6, shank_top - Vec3::Y * 0.2 + side * 0.6, 0.05, 0.05, fwd);
            b.mesh(Mat::Leather).beam(cat, shank_top, 0.02, 0.02, fwd);
        }
    }

    // ---- Груз: амфоры рядами вокруг люка, тюки.
    for r in 0..s.cargo {
        for k in 0..2 {
            let t = 0.3 + 0.045 * r as f32;
            if t > 0.56 {
                break;
            }
            let p = f.o + fwd * ((t - 0.5) * s.len) + side * (if k == 0 { -1.0 } else { 1.0 } * (f.half(t, DECK_U) * 0.55)) + Vec3::Y * deck_at(t);
            kit::amphora(b, p, 1.0, (r + k) as u32);
        }
    }
    if s.cargo > 0 {
        let t = 0.66 + 0.02;
        let c = f.o + fwd * ((t - 0.5) * s.len) + Vec3::Y * deck_at(t);
        for (i, (dx, dz)) in [(0.0f32, -0.6f32), (0.0, 0.5), (0.9, 0.0)].into_iter().enumerate() {
            let p = c + fwd * dx + side * dz + Vec3::Y * (0.3 + 0.05 * i as f32);
            b.mesh(Mat::Canvas).obox(p, Vec3::new(0.45, 0.3, 0.35), yaw + 0.2 * i as f32);
        }
    }

    // ---- Опора для ног: палуба — твёрдая (по участкам, чтобы у узких оконечностей не стоять на воздухе).
    for (t0, t1, shrink) in [(0.06f32, 0.2f32, 0.75f32), (0.2, 0.8, 1.0), (0.8, 0.93, 0.6)] {
        let tm = (t0 + t1) / 2.0;
        let hw = f.half(tm, DECK_U) * shrink;
        let top = deck_at(if tm < 0.5 { t1 } else { t0 }).max(deck_at(tm));
        let a = f.o + fwd * ((t0 - 0.5) * s.len) - side * hw;
        let c = f.o + fwd * ((t1 - 0.5) * s.len) + side * hw;
        let (lo, hi) = (a.min(c), a.max(c));
        b.invisible.push(Aabb::new(Vec3::new(lo.x, f.keel, lo.z), Vec3::new(hi.x, top, hi.z)));
    }

    hull_blockers(b, &f, |t| f.y(t, DECK_U));
    bilge_floor(b, &f, at.y);

    Moored { deck_y: deck_at(mid_t), mid }
}

/// Сходни с пирса на палубу: доска с набитыми планками и перилами-верёвкой.
pub fn gangway(b: &mut Builder, from: Vec3, to: Vec3) {
    let d = to - from;
    let flat = Vec3::new(d.x, 0.0, d.z).normalize_or_zero();
    let across = Vec3::Y.cross(flat).normalize_or_zero();
    let m = b.mesh(Mat::Wood);
    m.beam(from + Vec3::Y * 0.05, to + Vec3::Y * 0.05, 0.3, 0.04, across);
    let n = (d.length() / 0.4) as usize;
    let t = b.mesh(Mat::Timber);
    for i in 1..n {
        let p = from + d * (i as f32 / n as f32) + Vec3::Y * 0.1;
        t.beam(p - across * 0.28, p + across * 0.28, 0.025, 0.02, flat);
    }
    for sd in [-1.0f32, 1.0] {
        let a = from + across * (0.3 * sd);
        let c = to + across * (0.3 * sd);
        let t = b.mesh(Mat::Timber);
        t.beam(a, a + Vec3::Y * 1.0, 0.03, 0.03, flat);
        t.beam(c, c + Vec3::Y * 1.0, 0.03, 0.03, flat);
        b.mesh(Mat::Leather).beam(a + Vec3::Y * 0.95, c + Vec3::Y * 0.95, 0.012, 0.012, Vec3::Y);
    }
    // Ступенька под ногой посередине — проход по сходням без провала в щель.
    let (lo, hi) = ((from - across * 0.32).min(to + across * 0.32), (from - across * 0.32).max(to + across * 0.32));
    let lo2 = lo.min((from + across * 0.32).min(to - across * 0.32));
    let hi2 = hi.max((from + across * 0.32).max(to - across * 0.32));
    b.invisible.push(Aabb::new(Vec3::new(lo2.x, from.y.min(to.y) - 1.5, lo2.z), Vec3::new(hi2.x, (from.y + to.y) / 2.0, hi2.z)));
}

/// Лодка-рыбница: обшивка гладкая, банки, вёсла на борту.
pub fn skiff(b: &mut Builder, at: Vec3, yaw: f32) {
    let s = Ship {
        len: 6.2,
        beam: 1.8,
        depth: 0.95,
        draft: 0.35,
        sheer: 0.3,
        bow: 1.6,
        masts: &[],
        sail: Sail::Furled,
        oars: 0,
        galley: false,
        spur: false,
        shields: false,
        stripe: Mat::Ochre,
        cargo: 0,
        anchored: false,
    };
    let (sn, cs) = yaw.sin_cos();
    let fwd = Vec3::new(cs, 0.0, sn);
    let side = Vec3::new(-sn, 0.0, cs);
    let o = Vec3::new(at.x, 0.0, at.z);
    let f = Frame { s: &s, o, fwd, side, keel: at.y - s.draft };
    b.at(at.x, at.z);
    let (nt, nu) = (18usize, 5usize);
    for sd in [-1.0f32, 1.0] {
        let g: Vec<Vec<Vec3>> = (0..=nt).map(|i| (0..=nu).map(|j| f.pt(i as f32 / nt as f32, j as f32 / nu as f32, sd)).collect()).collect();
        let out = f.out(sd);
        smooth_grid(b.mesh(Mat::Wood), &g, |p, i| {
            let t = i as f32 / nt as f32;
            let c = f.o + f.fwd * ((t - 0.5) * s.len) + Vec3::Y * p.y;
            ((p - c).normalize_or_zero() + out * 0.3 + Vec3::NEG_Y * 0.2).normalize_or_zero()
        });
        let pts: Vec<Vec3> = (0..=12).map(|i| f.pt(0.03 + 0.94 * i as f32 / 12.0, 1.0, sd) + Vec3::Y * 0.02).collect();
        b.mesh(Mat::Timber).beam_path(&pts, 0.04, 0.03, out);
        let band: Vec<Vec3> = (0..=12).map(|i| f.pt(0.05 + 0.9 * i as f32 / 12.0, 0.82, sd) + out * 0.03).collect();
        b.mesh(s.stripe).beam_path(&band, 0.07, 0.015, out);
    }
    // Банки и вёсла.
    for t in [0.35f32, 0.6] {
        let a = f.pt(t, 0.8, -1.0);
        let c = f.pt(t, 0.8, 1.0);
        b.mesh(Mat::Timber).beam(a, c, 0.11, 0.025, fwd);
    }
    for sd in [-1.0f32, 1.0] {
        let a = f.pt(0.3, 0.95, sd) + Vec3::Y * 0.08;
        let c = f.pt(0.78, 0.9, -sd) + Vec3::Y * 0.12;
        b.mesh(Mat::Wood).beam(a, c, 0.028, 0.028, Vec3::Y);
    }
    // Сеть и корзина на дне.
    let p = f.o + fwd * (-1.4) + Vec3::Y * (at.y + 0.06 + 0.12);
    b.mesh(Mat::Canvas).obox(p, Vec3::new(0.4, 0.12, 0.35), yaw + 0.3);
    let stem: Vec<Vec3> = (0..=4).map(|j| f.pt(1.0, j as f32 / 4.0, 0.0) + fwd * 0.03).chain([f.pt(1.0, 1.0, 0.0) + fwd * 0.12 + Vec3::Y * 0.25]).collect();
    b.mesh(Mat::Wood).beam_path(&stem, 0.05, 0.05, side);
    hull_blockers(b, &f, |_| at.y + 0.06);
    bilge_floor(b, &f, at.y);
}

/// Настил в трюме чуть выше ватерлинии: вода не видна внутри открытого корпуса.
fn bilge_floor(b: &mut Builder, f: &Frame, water: f32) {
    let y = water + 0.06;
    // Для каждого шпангоута ищем высоту обвода `u`, на которой борт проходит на уровне `y`.
    let at = |t: f32| -> Option<(Vec3, Vec3)> {
        if f.y(t, 1.0) <= y || f.y(t, 0.0) >= y {
            return None;
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..16 {
            let mid = (lo + hi) / 2.0;
            if f.y(t, mid) < y {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let u = (lo + hi) / 2.0;
        let l = f.pt(t, u, -1.0);
        let r = f.pt(t, u, 1.0);
        Some((Vec3::new(l.x, y, l.z) + f.side * 0.03, Vec3::new(r.x, y, r.z) - f.side * 0.03))
    };
    let n = 24;
    let m = b.mesh(Mat::Timber);
    for i in 0..n {
        let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        if let (Some((l0, r0)), Some((l1, r1))) = (at(t0), at(t1)) {
            m.quad([l0, l1, r1, r0], Vec3::Y);
        }
    }
}

/// Корпус целиком, от носа до кормы: вплавь внутрь не заплыть. Сверху — по `top(t)`
/// (палуба судна или настил лодки), так что на нём можно стоять, выбравшись из воды.
fn hull_blockers(b: &mut Builder, f: &Frame, top: impl Fn(f32) -> f32) {
    let n = 12;
    for i in 0..n {
        let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        let hw = [t0, (t0 + t1) / 2.0, t1].into_iter().map(|t| f.half(t, 1.0)).fold(0.0f32, f32::max).max(0.25);
        let a = f.o + f.fwd * ((t0 - 0.5) * f.s.len) - f.side * hw;
        let c = f.o + f.fwd * ((t1 - 0.5) * f.s.len) + f.side * hw;
        let (lo, hi) = (a.min(c), a.max(c));
        b.invisible.push(Aabb::new(Vec3::new(lo.x, f.keel - 0.1, lo.z), Vec3::new(hi.x, top(t0).min(top(t1)), hi.z)));
    }
}

/// Гладкая сетка (строки по `t`, столбцы по `u`) с нормалями по соседям — с двух сторон.
/// `outward(p, i)` — примерно наружу (по нему ориентируются нормали и обход треугольников).
fn smooth_grid(m: &mut MeshBuilder, g: &[Vec<Vec3>], outward: impl Fn(Vec3, usize) -> Vec3) {
    let ni = g.len();
    let nj = g[0].len();
    // UV в метрах: вдоль корпуса и по обводу — доски обшивки идут вдоль.
    let mut along = vec![vec![0.0f32; nj]; ni];
    let mut girth = vec![vec![0.0f32; nj]; ni];
    for i in 0..ni {
        for j in 0..nj {
            if i > 0 {
                along[i][j] = along[i - 1][j] + g[i][j].distance(g[i - 1][j]);
            }
            if j > 0 {
                girth[i][j] = girth[i][j - 1] + g[i][j].distance(g[i][j - 1]);
            }
        }
    }
    for back in [false, true] {
        let mut ids = vec![vec![0u32; nj]; ni];
        for i in 0..ni {
            for j in 0..nj {
                let di = g[(i + 1).min(ni - 1)][j] - g[i.saturating_sub(1)][j];
                let dj = g[i][(j + 1).min(nj - 1)] - g[i][j.saturating_sub(1)];
                let hint = outward(g[i][j], i);
                let mut n = di.cross(dj).normalize_or_zero();
                if n == Vec3::ZERO {
                    n = hint;
                }
                if n.dot(hint) < 0.0 {
                    n = -n;
                }
                if back {
                    n = -n;
                }
                ids[i][j] = m.vert(g[i][j], n, Vec2::new(along[i][j] / 2.0, girth[i][j] / 2.0));
            }
        }
        for i in 0..ni - 1 {
            for j in 0..nj - 1 {
                let (a, b2, c, d) = (ids[i][j], ids[i + 1][j], ids[i + 1][j + 1], ids[i][j + 1]);
                let geo = (g[i + 1][j] - g[i][j]).cross(g[i + 1][j + 1] - g[i][j]);
                let want = outward(g[i][j], i) * if back { -1.0 } else { 1.0 };
                if geo.dot(want) >= 0.0 {
                    m.index_tri(a, b2, c);
                    m.index_tri(a, c, d);
                } else {
                    m.index_tri(a, c, b2);
                    m.index_tri(a, d, c);
                }
            }
        }
    }
}

/// Парус треугольником `tack`–`peak`–`clew`, выгнутый на `billow` посередине (с двух сторон).
/// Сетка общая для соседних треугольников: плавные нормали и сплошная развёртка ткани в метрах.
fn sail_cloth(m: &mut MeshBuilder, tack: Vec3, peak: Vec3, clew: Vec3, billow: Vec3) {
    let n = 12usize;
    let at = |i: usize, j: usize| -> Vec3 {
        // i — к пику, j — к шкотовому углу.
        let a = i as f32 / n as f32;
        let c = j as f32 / n as f32;
        let w0 = (1.0 - a - c).max(0.0);
        let bulge = (27.0 * w0 * a * c).min(1.0).sqrt();
        // Провис по нижней шкаторине.
        let foot = 0.25 * (4.0 * c * (1.0 - c)) * (1.0 - a);
        tack + (peak - tack) * a + (clew - tack) * c + billow * bulge - Vec3::Y * foot
    };
    let face = (peak - tack).cross(clew - tack).normalize_or_zero();
    let face = if face.dot(billow) < 0.0 { -face } else { face };
    let ua = (peak - tack).normalize_or_zero();
    let va = face.cross(ua).normalize_or_zero();
    let idx = |i: usize, j: usize| -> usize { i * (n + 1) + j };
    let mut pts = vec![Vec3::ZERO; (n + 1) * (n + 1)];
    for i in 0..=n {
        for j in 0..=n - i {
            pts[idx(i, j)] = at(i, j);
        }
    }
    let normal = |i: usize, j: usize| -> Vec3 {
        let p = pts[idx(i, j)];
        let pi = if i + 1 + j <= n { pts[idx(i + 1, j)] } else { p };
        let mi = if i > 0 { pts[idx(i - 1, j)] } else { p };
        let pj = if i + j + 1 <= n { pts[idx(i, j + 1)] } else { p };
        let mj = if j > 0 { pts[idx(i, j - 1)] } else { p };
        let nn = (pi - mi).cross(pj - mj).normalize_or_zero();
        if nn == Vec3::ZERO { face } else if nn.dot(face) < 0.0 { -nn } else { nn }
    };
    for back in [false, true] {
        let sg = if back { -1.0 } else { 1.0 };
        let mut ids = vec![0u32; (n + 1) * (n + 1)];
        for i in 0..=n {
            for j in 0..=n - i {
                let p = pts[idx(i, j)];
                let uv = Vec2::new((p - tack).dot(ua), (p - tack).dot(va)) / 2.0;
                ids[idx(i, j)] = m.vert(p, normal(i, j) * sg, uv);
            }
        }
        let mut tri = |a: usize, b2: usize, c: usize| {
            let geo = (pts[b2] - pts[a]).cross(pts[c] - pts[a]);
            if geo.dot(face * sg) >= 0.0 {
                m.index_tri(ids[a], ids[b2], ids[c]);
            } else {
                m.index_tri(ids[a], ids[c], ids[b2]);
            }
        };
        for i in 0..n {
            for j in 0..n - i {
                tri(idx(i, j), idx(i + 1, j), idx(i, j + 1));
                if i + j + 1 < n {
                    tri(idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
                }
            }
        }
    }
}
