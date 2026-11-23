//! Строительный набор Херсонеса: стены с проёмами, черепичные крыши, лестницы,
//! утварь (амфоры, пифосы, столы, ложа), деревья.

use super::super::geom::{self, MeshBuilder, WallFrame, arch_height};
use super::super::{Builder, DoorSpec, Mat};
use crate::collision::Aabb;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Простой детерминированный генератор случайных чисел (одинаково на сервере и клиенте).
#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        let mut r = Rng(seed.wrapping_mul(2654435761).wrapping_add(0x9E3779B9));
        r.f();
        r
    }
    pub fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
    pub fn int(&mut self, n: usize) -> usize {
        ((self.f() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

/// Прямоугольник на плане (X, Z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub z0: f32,
    pub x1: f32,
    pub z1: f32,
}

impl Rect {
    pub const fn new(x0: f32, z0: f32, x1: f32, z1: f32) -> Self {
        Rect { x0, z0, x1, z1 }
    }
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn d(&self) -> f32 {
        self.z1 - self.z0
    }
    pub fn cx(&self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
    pub fn cz(&self) -> f32 {
        (self.z0 + self.z1) / 2.0
    }
    pub fn center(&self, y: f32) -> Vec3 {
        Vec3::new(self.cx(), y, self.cz())
    }
    pub fn inset(&self, m: f32) -> Rect {
        Rect::new(self.x0 + m, self.z0 + m, self.x1 - m, self.z1 - m)
    }
    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(self.x0.min(o.x0), self.z0.min(o.z0), self.x1.max(o.x1), self.z1.max(o.z1))
    }
    pub fn min3(&self, y: f32) -> Vec3 {
        Vec3::new(self.x0, y, self.z0)
    }
    pub fn max3(&self, y: f32) -> Vec3 {
        Vec3::new(self.x1, y, self.z1)
    }
}

/// Проём в стене.
#[derive(Clone, Copy, Debug)]
pub enum Op {
    /// Дверь с деревянной перемычкой: центр `s`, ширина `w`, высота `h`.
    Door { s: f32, w: f32, h: f32 },
    /// Окно: подоконник `sill`, верх `top`. С деревянной решёткой; не проходимо.
    Window { s: f32, w: f32, sill: f32, top: f32 },
    /// Полукруглая арка (проход): пята арки на высоте `spring`.
    Arch { s: f32, w: f32, spring: f32 },
    /// Дверь на верхний этаж: проём от `sill` до `top` (с дверью).
    Upper { s: f32, w: f32, sill: f32, top: f32 },
}

impl Op {
    /// Просвет проёма по высоте (от и до) в стене от `y0` до `y1`.
    fn gap(&self, y0: f32, y1: f32) -> (f32, f32) {
        match *self {
            Op::Door { h, .. } => (y0, (y0 + h).min(y1)),
            Op::Window { sill, top, .. } | Op::Upper { sill, top, .. } => ((y0 + sill).min(y1), (y0 + top).min(y1)),
            Op::Arch { w, spring, .. } => (y0, (y0 + spring + w / 2.0).min(y1)),
        }
    }
    fn s(&self) -> f32 {
        match *self {
            Op::Door { s, .. } | Op::Window { s, .. } | Op::Arch { s, .. } | Op::Upper { s, .. } => s,
        }
    }
    fn w(&self) -> f32 {
        match *self {
            Op::Door { w, .. } | Op::Window { w, .. } | Op::Arch { w, .. } | Op::Upper { w, .. } => w,
        }
    }
}

pub fn fx() -> WallFrame {
    WallFrame { along_x: true }
}
pub fn fz() -> WallFrame {
    WallFrame { along_x: false }
}

/// Стена от `s0` до `s1` (толщина `d0..d1`, высота `y0..y1`) с проёмами.
/// `trim` — материал откосов и перемычек каменных проёмов (обычно известняк).
pub fn wall(b: &mut Builder, f: WallFrame, s0: f32, s1: f32, d0: f32, d1: f32, y0: f32, y1: f32, ops: &[Op], mat: Mat, trim: Mat) {
    let mut ops: Vec<Op> = ops.iter().copied().filter(|o| o.s() > s0 && o.s() < s1).collect();
    ops.sort_by(|a, b| a.s().total_cmp(&b.s()));
    let mut cursor = s0;
    let dm = (d0 + d1) / 2.0;
    let mut i = 0;
    while i < ops.len() {
        // Проёмы, стоящие друг над другом (дверь внизу, окно второго этажа над ней), — одной группой.
        let mut group = vec![ops[i]];
        let (mut gl, mut gr) = (ops[i].s() - ops[i].w() / 2.0, ops[i].s() + ops[i].w() / 2.0);
        while i + 1 < ops.len() && ops[i + 1].s() - ops[i + 1].w() / 2.0 < gr {
            i += 1;
            let o = ops[i];
            gl = gl.min(o.s() - o.w() / 2.0);
            gr = gr.max(o.s() + o.w() / 2.0);
            group.push(o);
        }
        i += 1;
        b.solid_v(f.bounds((cursor, gl), (y0, y1), (d0, d1)), mat);
        if group.len() == 1 {
            opening(b, f, group[0], gl, gr, y0, y1, d0, d1, dm, mat, trim);
        } else {
            // Сплошная кладка между просветами, обрамления — от каждого проёма.
            let mut gaps: Vec<(f32, f32)> = group.iter().map(|o| o.gap(y0, y1)).collect();
            gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut y = y0;
            for (lo, hi) in gaps {
                b.solid_v(f.bounds((gl, gr), (y, lo), (d0, d1)), mat);
                y = y.max(hi);
            }
            b.solid_v(f.bounds((gl, gr), (y, y1), (d0, d1)), mat);
            for o in group {
                let (l, r) = (o.s() - o.w() / 2.0, o.s() + o.w() / 2.0);
                let n = b.blocks.len();
                opening(b, f, o, l, r, y0, y1, d0, d1, dm, mat, trim);
                // Кладку вокруг уже положили выше.
                let mut k = n;
                while k < b.blocks.len() {
                    if b.blocks[k].solid {
                        b.blocks.remove(k);
                    } else {
                        k += 1;
                    }
                }
            }
        }
        cursor = gr;
    }
    b.solid_v(f.bounds((cursor, s1), (y0, y1), (d0, d1)), mat);
}

/// Один проём: кладка над и под ним, обрамление, решётка, дверь.
fn opening(b: &mut Builder, f: WallFrame, o: Op, l: f32, r: f32, y0: f32, y1: f32, d0: f32, d1: f32, dm: f32, mat: Mat, trim: Mat) {
    match o {
        Op::Door { h, w, .. } => {
            let top = (y0 + h).min(y1);
            // Створка: двустворчатые ворота и ставни лавок (широкие — открыты днём).
            b.door(DoorSpec { center: f.p(o.s(), y0, dm), along_x: f.along_x, width: w, height: top - y0, wall_t: d1 - d0, double: w > 1.9, open: w > 2.5 });
            b.solid_v(f.bounds((l, r), (top, y1), (d0, d1)), mat);
            // Деревянная перемычка и каменные откосы.
            let (a, c) = f.bounds((l - 0.25, r + 0.25), (top - 0.02, top + 0.22), (d0 - 0.03, d1 + 0.03));
            b.mesh(Mat::Wood).cuboid(a, c);
            for s in [l, r] {
                let (a, c) = f.bounds((s - 0.16, s + 0.16), (y0, top), (d0 - 0.02, d1 + 0.02));
                b.mesh(trim).cuboid(a, c);
            }
            // Порог.
            let (a, c) = f.bounds((l, r), (y0, y0 + 0.06), (d0, d1));
            b.mesh(trim).cuboid(a, c);
        }
        Op::Window { sill, top, .. } => {
            let (sill, top) = ((y0 + sill).min(y1), (y0 + top).min(y1));
            b.solid_v(f.bounds((l, r), (y0, sill), (d0, d1)), mat);
            b.solid_v(f.bounds((l, r), (top, y1), (d0, d1)), mat);
            let (a, c) = f.bounds((l - 0.12, r + 0.12), (sill - 0.08, sill), (d0 - 0.06, d1 + 0.06));
            b.mesh(trim).cuboid(a, c);
            let (a, c) = f.bounds((l - 0.15, r + 0.15), (top, top + 0.16), (d0 - 0.03, d1 + 0.03));
            b.mesh(Mat::Wood).cuboid(a, c);
            // Деревянная решётка посередине толщины.
            let m = b.mesh(Mat::Wood);
            let bars = ((r - l) / 0.18).floor().max(1.0) as usize;
            for k in 1..=bars {
                let s = l + (r - l) * k as f32 / (bars + 1) as f32;
                let (a, c) = f.bounds((s - 0.02, s + 0.02), (sill, top), (dm - 0.02, dm + 0.02));
                m.cuboid(a, c);
            }
            let (a, c) = f.bounds((l, r), ((sill + top) / 2.0 - 0.02, (sill + top) / 2.0 + 0.02), (dm - 0.02, dm + 0.02));
            m.cuboid(a, c);
            b.invisible.push(Aabb::new(f.p(l, sill, dm - 0.05), f.p(r, top, dm + 0.05)));
        }
        Op::Upper { sill, top, w, .. } => {
            let (sill, top) = ((y0 + sill).min(y1), (y0 + top).min(y1));
            b.solid_v(f.bounds((l, r), (y0, sill), (d0, d1)), mat);
            b.solid_v(f.bounds((l, r), (top, y1), (d0, d1)), mat);
            let (a, c) = f.bounds((l - 0.25, r + 0.25), (top - 0.02, top + 0.22), (d0 - 0.03, d1 + 0.03));
            b.mesh(Mat::Wood).cuboid(a, c);
            for s in [l, r] {
                let (a, c) = f.bounds((s - 0.16, s + 0.16), (sill, top), (d0 - 0.02, d1 + 0.02));
                b.mesh(trim).cuboid(a, c);
            }
            b.door(DoorSpec { center: f.p(o.s(), sill, dm), along_x: f.along_x, width: w, height: top - sill, wall_t: d1 - d0, double: false, open: false });
        }
        Op::Arch { w, spring, .. } => {
            let apex = (y0 + spring + w / 2.0).min(y1);
            b.solid_v(f.bounds((l, r), (apex, y1), (d0, d1)), mat);
            let q = w / 4.0;
            let band = y0 + spring + arch_height(q, w, w / 2.0);
            b.solid_v(f.bounds((l, l + q * 0.6), (band, apex), (d0, d1)), mat);
            b.solid_v(f.bounds((r - q * 0.6, r), (band, apex), (d0, d1)), mat);
            let mut infill = std::mem::take(b.mesh(mat));
            let mut tr = std::mem::take(b.mesh(trim));
            geom::arch_infill(&mut infill, &mut tr, f, o.s(), w, y0 + spring, w / 2.0, apex, d0, d1);
            for s in [l, r] {
                let (a, c) = f.bounds((s - 0.14, s + 0.14), (y0, y0 + spring), (d0 - 0.04, d1 + 0.04));
                tr.cuboid(a, c);
                // Импост.
                let (a, c) = f.bounds((s - 0.25, s + 0.25), (y0 + spring - 0.2, y0 + spring), (d0 - 0.1, d1 + 0.1));
                tr.cuboid(a, c);
            }
            *b.mesh(mat) = infill;
            *b.mesh(trim) = tr;
        }
    }
}

/// Стена вдоль X на линии `z` (толщина `t`, по центру).
pub fn wall_x(b: &mut Builder, x0: f32, x1: f32, z: f32, t: f32, h: f32, ops: &[Op], mat: Mat) {
    wall(b, fx(), x0, x1, z - t / 2.0, z + t / 2.0, 0.0, h, ops, mat, Mat::Limestone);
}
/// Стена вдоль Z на линии `x`.
pub fn wall_z(b: &mut Builder, z0: f32, z1: f32, x: f32, t: f32, h: f32, ops: &[Op], mat: Mat) {
    wall(b, fz(), z0, z1, x - t / 2.0, x + t / 2.0, 0.0, h, ops, mat, Mat::Limestone);
}

/// Угловые камни (крупные тёсаные блоки) на углу дома.
pub fn quoins(b: &mut Builder, x: f32, z: f32, h: f32, t: f32) {
    let m = b.mesh(Mat::Limestone);
    let mut y = 0.0;
    let mut k = 0;
    while y < h - 0.1 {
        let hh = 0.42f32.min(h - y);
        let (lx, lz) = if k % 2 == 0 { (0.9, 0.5) } else { (0.5, 0.9) };
        m.cuboid(Vec3::new(x - t / 2.0 - 0.02 - lx * 0.0, y, z - t / 2.0 - 0.02), Vec3::new(x - t / 2.0 + lx, y + hh, z - t / 2.0 + lz));
        y += hh;
        k += 1;
    }
}

/// Двускатная черепичная крыша над прямоугольником (по наружным граням стен) со свесом.
/// Фронтоны (треугольники) — из `gable_mat`. Снизу — дощатый настил.
pub fn gable_roof(b: &mut Builder, r: Rect, eave: f32, rise: f32, ridge_x: bool, over: f32, gable_mat: Mat) {
    let ro = Rect::new(r.x0 - over, r.z0 - over, r.x1 + over, r.z1 + over);
    let drop = rise / ((if ridge_x { r.d() } else { r.w() }) / 2.0) * over;
    let e = eave - drop;
    let top = eave + rise;
    {
        let m = b.mesh(Mat::Tile);
        m.gable_roof_slopes(Vec2::new(ro.x0, ro.z0), Vec2::new(ro.x1, ro.z1), e, top, ridge_x);
    }
    {
        let m = b.mesh(Mat::Wood);
        let mut under = MeshBuilder::default();
        under.gable_roof_slopes(Vec2::new(ro.x0, ro.z0), Vec2::new(ro.x1, ro.z1), e - 0.08, top - 0.08, ridge_x);
        m.append(under.flipped());
    }
    // Конёк — ряд калиптеров.
    let ridge = if ridge_x {
        (Vec3::new(ro.x0, top + 0.05, r.cz()), Vec3::new(ro.x1, top + 0.05, r.cz()))
    } else {
        (Vec3::new(r.cx(), top + 0.05, ro.z0), Vec3::new(r.cx(), top + 0.05, ro.z1))
    };
    b.mesh(Mat::Tile).beam(ridge.0, ridge.1, 0.13, 0.09, Vec3::Y);
    // Фронтоны.
    let m = b.mesh(gable_mat);
    if ridge_x {
        for (x, s) in [(r.x0, -1.0f32), (r.x1, 1.0)] {
            m.tri(Vec3::new(x, eave, r.z0), Vec3::new(x, eave, r.z1), Vec3::new(x, top, r.cz()), Vec3::X * s);
        }
    } else {
        for (z, s) in [(r.z0, -1.0f32), (r.z1, 1.0)] {
            m.tri(Vec3::new(r.x0, eave, z), Vec3::new(r.x1, eave, z), Vec3::new(r.cx(), top, z), Vec3::Z * s);
        }
    }
}

/// Сторона прямоугольника.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    N,
    S,
    W,
    E,
}

impl Side {
    pub fn out(&self) -> Vec3 {
        match self {
            Side::N => Vec3::NEG_Z,
            Side::S => Vec3::Z,
            Side::W => Vec3::NEG_X,
            Side::E => Vec3::X,
        }
    }
    pub fn along_x(&self) -> bool {
        matches!(self, Side::N | Side::S)
    }
}

/// Односкатная крыша: высокий край над стороной `high` прямоугольника, низкий — над противоположной.
pub fn lean_roof(b: &mut Builder, r: Rect, high: Side, y_high: f32, y_low: f32, over: f32) {
    let (span, along) = if high.along_x() { (r.d(), r.w()) } else { (r.w(), r.d()) };
    let slope = (y_high - y_low) / span;
    let yh = y_high + slope * 0.25;
    let yl = y_low - slope * over;
    let ro = Rect::new(r.x0 - 0.25, r.z0 - 0.25, r.x1 + 0.25, r.z1 + 0.25);
    let (hi, lo): ([Vec3; 2], [Vec3; 2]) = match high {
        Side::N => ([Vec3::new(ro.x0, yh, r.z0 - 0.25), Vec3::new(ro.x1, yh, r.z0 - 0.25)], [Vec3::new(ro.x0, yl, r.z1 + over), Vec3::new(ro.x1, yl, r.z1 + over)]),
        Side::S => ([Vec3::new(ro.x0, yh, r.z1 + 0.25), Vec3::new(ro.x1, yh, r.z1 + 0.25)], [Vec3::new(ro.x0, yl, r.z0 - over), Vec3::new(ro.x1, yl, r.z0 - over)]),
        Side::W => ([Vec3::new(r.x0 - 0.25, yh, ro.z0), Vec3::new(r.x0 - 0.25, yh, ro.z1)], [Vec3::new(r.x1 + over, yl, ro.z0), Vec3::new(r.x1 + over, yl, ro.z1)]),
        Side::E => ([Vec3::new(r.x1 + 0.25, yh, ro.z0), Vec3::new(r.x1 + 0.25, yh, ro.z1)], [Vec3::new(r.x0 - over, yl, ro.z0), Vec3::new(r.x0 - over, yl, ro.z1)]),
    };
    let _ = along;
    b.mesh(Mat::Tile).lean_roof(hi, lo);
    let down = Vec3::Y * -0.07;
    let mut under = MeshBuilder::default();
    under.lean_roof([hi[0] + down, hi[1] + down], [lo[0] + down, lo[1] + down]);
    b.mesh(Mat::Wood).append(under.flipped());
    // Треугольные торцы между стеной и скатом закрываем кладкой — делает вызывающий (стены выше),
    // а здесь — торцевые доски.
    let m = b.mesh(Mat::Wood);
    m.beam(lo[0] + Vec3::Y * -0.06, lo[1] + Vec3::Y * -0.06, 0.06, 0.06, Vec3::Y);
}

/// Треугольный кусок кладки под односкатной крышей на торце крыла.
pub fn lean_gable(b: &mut Builder, mat: Mat, a: Vec3, c: Vec3, rise_at_a: f32, facing: Vec3, t: f32) {
    let m = b.mesh(mat);
    let top = a + Vec3::Y * rise_at_a;
    for off in [facing * t / 2.0, -facing * t / 2.0] {
        m.tri(a + off, c + off, top + off, if off.dot(facing) > 0.0 { facing } else { -facing });
    }
    let up = (top - c).cross(facing);
    m.quad([c + facing * t / 2.0, top + facing * t / 2.0, top - facing * t / 2.0, c - facing * t / 2.0], if up.y > 0.0 { up } else { -up });
}

/// Пирамидальная черепичная крыша (башни).
pub fn hip_roof(b: &mut Builder, r: Rect, eave: f32, rise: f32) {
    let c = r.center(eave);
    let m = b.mesh(Mat::Tile);
    let pts = [Vec3::new(r.x0, eave, r.z0), Vec3::new(r.x1, eave, r.z0), Vec3::new(r.x1, eave, r.z1), Vec3::new(r.x0, eave, r.z1)];
    let apex = c + Vec3::Y * rise;
    for i in 0..4 {
        let (a, bb) = (pts[i], pts[(i + 1) % 4]);
        let out = ((a + bb) / 2.0 - c).normalize() + Vec3::Y * 0.5;
        let w = a.distance(bb);
        let sl = ((a + bb) / 2.0).distance(apex);
        m.quad_uv([a, bb, apex, apex], [Vec2::new(0.0, sl), Vec2::new(w, sl), Vec2::new(w / 2.0, 0.0), Vec2::new(w / 2.0, 0.0)], out);
    }
}

/// Лестница вдоль X или Z из ступеней 0.4 × 0.6 м, от `start` в сторону `dir`, до высоты `h`.
pub fn stairs(b: &mut Builder, start: Vec3, dir: Vec3, width: f32, h: f32, mat: Mat) {
    let n = (h / 0.4).ceil() as usize;
    let side = Vec3::new(dir.z, 0.0, -dir.x).abs() * (width / 2.0);
    for i in 0..n {
        let top = start.y + (0.4 * (i + 1) as f32).min(h);
        let a = start + dir * 0.6 * i as f32;
        let c = start + dir * 0.6 * (i + 1) as f32;
        let min = a.min(c) - side;
        let max = a.max(c) + side;
        b.solid([min.x, start.y, min.z], [max.x, top, max.z], mat);
    }
}

// ---------------- утварь ----------------

/// Амфора: тело вращения с ручками. `tilt` — наклон (лежит в штабеле ≈ π/2).
pub fn amphora(b: &mut Builder, p: Vec3, scale: f32, kind: u32) {
    // Профили по типам: 0 — LR2 (шаровидная, рифлёная), 1 — херсонесская узкогорлая, 2 — LR1 (цилиндрическая).
    let prof: &[(f32, f32)] = match kind % 3 {
        0 => &[(0.0, 0.0), (0.04, 0.01), (0.1, 0.06), (0.17, 0.16), (0.2, 0.3), (0.19, 0.42), (0.13, 0.52), (0.06, 0.57), (0.05, 0.66), (0.06, 0.68), (0.0, 0.69)],
        1 => &[(0.0, 0.0), (0.025, 0.0), (0.06, 0.08), (0.12, 0.22), (0.15, 0.38), (0.14, 0.5), (0.09, 0.6), (0.045, 0.66), (0.035, 0.8), (0.05, 0.82), (0.0, 0.83)],
        _ => &[(0.0, 0.0), (0.05, 0.0), (0.1, 0.05), (0.14, 0.15), (0.15, 0.4), (0.13, 0.55), (0.07, 0.62), (0.05, 0.7), (0.06, 0.72), (0.0, 0.73)],
    };
    let scaled: Vec<(f32, f32)> = prof.iter().map(|&(r, y)| (r * scale, y * scale)).collect();
    let m = b.mesh(Mat::Ceramic);
    m.lathe(p, &scaled, 10);
    let neck_y = scaled[scaled.len() - 3].1;
    let shoulder = scaled[scaled.len() / 2 + 1];
    for s in [-1.0f32, 1.0] {
        let a = p + Vec3::new(s * 0.05 * scale, neck_y - 0.02 * scale, 0.0);
        let mid = p + Vec3::new(s * (shoulder.0 * 0.75 + 0.03 * scale), neck_y, 0.0);
        let c = p + Vec3::new(s * shoulder.0 * 0.8, shoulder.1, 0.0);
        m.beam_path(&[a, mid, c], 0.012 * scale, 0.02 * scale, Vec3::Z);
    }
}

/// Лежащая амфора (в штабеле): упрощённое тело вдоль X или Z.
pub fn amphora_lying(b: &mut Builder, p: Vec3, along_x: bool, scale: f32) {
    let m = b.mesh(Mat::Ceramic);
    let len = 0.7 * scale;
    let dir = if along_x { Vec3::X } else { Vec3::Z };
    let r = 0.17 * scale;
    let segs: Vec<Vec3> = (0..=6).map(|k| p + dir * (k as f32 / 6.0 - 0.5) * len).collect();
    for (k, w) in segs.windows(2).enumerate() {
        let t = (k as f32 + 0.5) / 6.0;
        let rr = r * (1.0 - (2.0 * t - 0.8).powi(2) * 0.55).max(0.25);
        m.beam(w[0], w[1], rr, rr, Vec3::Y);
    }
}

/// Пифос — большой зерновой или винный сосуд. `buried` — врыт в землю на столько метров.
pub fn pithos(b: &mut Builder, p: Vec3, h: f32, buried: f32) {
    let r = h * 0.38;
    let prof = [(0.0, 0.0), (r * 0.45, 0.0), (r * 0.85, h * 0.18), (r, h * 0.48), (r * 0.88, h * 0.78), (r * 0.5, h * 0.95), (r * 0.52, h), (r * 0.42, h), (r * 0.4, h * 0.96)];
    let base = p - Vec3::Y * buried;
    b.mesh(Mat::Ceramic).lathe(base, &prof, 14);
    // Тёмное нутро.
    b.mesh(Mat::Coals).disc(base + Vec3::Y * (h * 0.955), r * 0.4, 10, true);
    let hh = h - buried;
    if hh > 0.3 {
        hidden(b, Vec3::new(p.x - r * 0.8, p.y, p.z - r * 0.8), Vec3::new(p.x + r * 0.8, p.y + hh, p.z + r * 0.8));
    }
}

/// Кувшин/горшок (мелкая посуда на полках и столах).
pub fn jug(b: &mut Builder, p: Vec3, h: f32, mat: Mat) {
    let r = h * 0.35;
    let prof = [(0.0, 0.0), (r * 0.6, 0.0), (r, h * 0.35), (r * 0.9, h * 0.65), (r * 0.45, h * 0.85), (r * 0.5, h), (0.0, h)];
    b.mesh(mat).lathe(p, &prof, 8);
}

/// Невидимая коллизия (форму рисует декоративный меш).
pub fn hidden(b: &mut Builder, min: Vec3, max: Vec3) {
    b.invisible.push(Aabb::new(min, max));
}

/// Стол на четырёх ножках.
pub fn table(b: &mut Builder, r: Rect, h: f32) {
    b.invisible.push(Aabb::new(r.min3(0.0), r.max3(h)));
    let m = b.mesh(Mat::Wood);
    m.cuboid(r.min3(h - 0.06), r.max3(h));
    for (x, z) in [(r.x0 + 0.08, r.z0 + 0.08), (r.x1 - 0.08, r.z0 + 0.08), (r.x0 + 0.08, r.z1 - 0.08), (r.x1 - 0.08, r.z1 - 0.08)] {
        m.cuboid(Vec3::new(x - 0.04, 0.0, z - 0.04), Vec3::new(x + 0.04, h - 0.06, z + 0.04));
    }
}

/// Скамья.
pub fn bench(b: &mut Builder, r: Rect) {
    table(b, r, 0.45);
}

/// Ложе (кровать) с соломенным тюфяком.
pub fn bed(b: &mut Builder, r: Rect) {
    table(b, r, 0.45);
    b.mesh(Mat::Canvas).cuboid(Vec3::new(r.x0 + 0.05, 0.45, r.z0 + 0.05), Vec3::new(r.x1 - 0.05, 0.6, r.z1 - 0.05));
}

/// Сундук.
/// Сундук, который можно обыскать (добыча — по текущей таблице сборщика).
pub fn chest(b: &mut Builder, p: Vec3, along_x: bool) {
    chest_body(b, p, along_x);
    b.container(p + Vec3::Y * 0.7, "Сундук");
}

/// Только сам сундук (без добычи).
pub fn chest_body(b: &mut Builder, p: Vec3, along_x: bool) {
    let (hx, hz) = if along_x { (0.5, 0.3) } else { (0.3, 0.5) };
    b.solid([p.x - hx, 0.0, p.z - hz], [p.x + hx, 0.55, p.z + hz], Mat::Wood);
    let m = b.mesh(Mat::Iron);
    m.cuboid(Vec3::new(p.x - hx - 0.01, 0.42, p.z - hz - 0.01), Vec3::new(p.x + hx + 0.01, 0.47, p.z + hz + 0.01));
}

/// Полка у стены с посудой: `r` — прямоугольник полки (узкий), `levels` ярусов.
pub fn shelf(b: &mut Builder, r: Rect, levels: usize, rng: &mut Rng, wares: Mat) {
    b.invisible.push(Aabb::new(r.min3(0.0), r.max3(0.4 + 0.5 * levels as f32)));
    let along_x = r.w() > r.d();
    for k in 0..levels {
        let y = 0.4 + 0.5 * k as f32;
        b.mesh(Mat::Wood).cuboid(r.min3(y - 0.04), r.max3(y));
        let len = if along_x { r.w() } else { r.d() };
        let n = (len / 0.22) as usize;
        for i in 0..n {
            if rng.chance(0.25) {
                continue;
            }
            let t = (i as f32 + 0.5) / n as f32;
            let p = if along_x { Vec3::new(r.x0 + r.w() * t, y, r.cz()) } else { Vec3::new(r.cx(), y, r.z0 + r.d() * t) };
            jug(b, p, rng.range(0.14, 0.3), wares);
        }
    }
    let m = b.mesh(Mat::Wood);
    for (x, z) in [(r.x0, r.z0), (r.x1, r.z1)] {
        m.cuboid(Vec3::new(x - 0.03, 0.0, z - 0.03), Vec3::new(x + 0.03, 0.4 + 0.5 * levels as f32, z + 0.03));
    }
}

/// Очаг: каменное кольцо с углями.
pub fn hearth(b: &mut Builder, p: Vec3) {
    b.hearths.push(p);
    let m = b.mesh(Mat::Rubble);
    m.lathe(p, &[(0.55, 0.0), (0.6, 0.12), (0.55, 0.22), (0.45, 0.22), (0.45, 0.1)], 10);
    b.mesh(Mat::Coals).disc(p + Vec3::Y * 0.1, 0.45, 10, true);
}

/// Хлебная печь — глиняный купол (как танур/подовая печь) с устьем.
pub fn bread_oven(b: &mut Builder, p: Vec3, r: f32) {
    hidden(b, Vec3::new(p.x - r * 0.75, 0.0, p.z - r * 0.75), Vec3::new(p.x + r * 0.75, r * 1.1, p.z + r * 0.75));
    let prof: Vec<(f32, f32)> = (0..=8).map(|k| {
        let a = k as f32 / 8.0 * FRAC_PI_2;
        (a.cos() * r, 0.35 + a.sin() * r * 0.85)
    }).collect();
    let mut full = vec![(r, 0.0)];
    full.extend(prof);
    b.mesh(Mat::Plaster).lathe(p, &full, 12);
    b.mesh(Mat::Coals).cuboid(p + Vec3::new(r - 0.05, 0.3, -0.2), p + Vec3::new(r + 0.02, 0.7, 0.2));
}

/// Устье цистерны: круглый каменный венец, внутри — тёмная вода.
pub fn cistern_mouth(b: &mut Builder, p: Vec3) {
    hidden(b, Vec3::new(p.x - 0.5, 0.0, p.z - 0.5), Vec3::new(p.x + 0.5, 0.65, p.z + 0.5));
    b.mesh(Mat::Limestone).lathe(p, &[(0.62, 0.0), (0.62, 0.55), (0.66, 0.6), (0.66, 0.68), (0.42, 0.68), (0.42, 0.3)], 14);
    b.mesh(Mat::Water).disc(p + Vec3::Y * 0.32, 0.42, 12, true);
    // Деревянный ворот с ведром.
    let m = b.mesh(Mat::Wood);
    m.cuboid(p + Vec3::new(-0.7, 0.0, -0.06), p + Vec3::new(-0.6, 1.5, 0.06));
    m.cuboid(p + Vec3::new(0.6, 0.0, -0.06), p + Vec3::new(0.7, 1.5, 0.06));
    m.beam(p + Vec3::new(-0.65, 1.35, 0.0), p + Vec3::new(0.65, 1.35, 0.0), 0.06, 0.06, Vec3::Y);
}

/// Давильня для винограда (тарапан): площадка с бортиком и сток в чан.
pub fn wine_press(b: &mut Builder, r: Rect) {
    let m = b.mesh(Mat::Signinum);
    m.cuboid(r.min3(0.0), Vec3::new(r.x1, 0.08, r.z1));
    b.solid([r.x0, 0.0, r.z0], [r.x1, 0.45, r.z0 + 0.25], Mat::Limestone);
    b.solid([r.x0, 0.0, r.z1 - 0.25], [r.x1, 0.45, r.z1], Mat::Limestone);
    b.solid([r.x0, 0.0, r.z0], [r.x0 + 0.25, 0.45, r.z1], Mat::Limestone);
    b.solid([r.x1 - 0.25, 0.0, r.z0], [r.x1, 0.45, r.z1], Mat::Limestone);
    // Чан для сусла рядом.
    let p = Vec3::new(r.x1 + 0.7, 0.0, r.cz());
    b.mesh(Mat::Signinum).lathe(p, &[(0.55, 0.0), (0.55, 0.5), (0.45, 0.5), (0.45, 0.1)], 12);
    b.mesh(Mat::Coals).disc(p + Vec3::Y * 0.12, 0.45, 12, true);
    b.invisible.push(Aabb::new(p - Vec3::new(0.5, 0.0, 0.5), p + Vec3::new(0.5, 0.5, 0.5)));
    // Желоб.
    b.mesh(Mat::Limestone).cuboid(Vec3::new(r.x1 - 0.1, 0.25, r.cz() - 0.08), Vec3::new(r.x1 + 0.3, 0.33, r.cz() + 0.08));
}

/// Ручная ротационная мельница: два жернова на подставке.
pub fn hand_mill(b: &mut Builder, p: Vec3) {
    hidden(b, Vec3::new(p.x - 0.35, 0.0, p.z - 0.35), Vec3::new(p.x + 0.35, 0.55, p.z + 0.35));
    let m = b.mesh(Mat::Limestone);
    m.lathe(p, &[(0.3, 0.0), (0.3, 0.3)], 10);
    m.lathe(p + Vec3::Y * 0.3, &[(0.0, 0.0), (0.33, 0.0), (0.33, 0.14), (0.28, 0.16), (0.05, 0.2)], 12);
    m.lathe(p + Vec3::Y * 0.46, &[(0.32, 0.0), (0.32, 0.12), (0.08, 0.14), (0.08, 0.06)], 12);
    b.mesh(Mat::Wood).beam(p + Vec3::new(0.22, 0.58, 0.0), p + Vec3::new(0.22, 0.85, 0.0), 0.025, 0.025, Vec3::X);
}

/// Штабель дров.
pub fn woodpile(b: &mut Builder, r: Rect, h: f32) {
    b.solid([r.x0, 0.0, r.z0], [r.x1, h, r.z1], Mat::Wood);
    let m = b.mesh(Mat::Wood);
    let along_x = r.w() > r.d();
    let n = ((if along_x { r.w() } else { r.d() }) / 0.14) as usize;
    for k in 0..n {
        let t = (k as f32 + 0.5) / n as f32;
        for y in [0.15, 0.4, 0.65] {
            if y > h {
                continue;
            }
            let (a, c) = if along_x {
                (Vec3::new(r.x0 + r.w() * t, y, r.z0 - 0.05), Vec3::new(r.x0 + r.w() * t, y, r.z1 + 0.05))
            } else {
                (Vec3::new(r.x0 - 0.05, y, r.z0 + r.d() * t), Vec3::new(r.x1 + 0.05, y, r.z0 + r.d() * t))
            };
            m.beam(a, c, 0.06, 0.06, Vec3::Y);
        }
    }
}

/// Ткацкий станок (вертикальный, с грузилами).
pub fn loom(b: &mut Builder, p: Vec3, along_x: bool) {
    let side = if along_x { Vec3::X } else { Vec3::Z };
    let m = b.mesh(Mat::Wood);
    for s in [-0.8f32, 0.8] {
        m.beam(p + side * s, p + side * s + Vec3::new(0.0, 1.9, 0.0) + Vec3::new(side.z, 0.0, side.x) * 0.3, 0.04, 0.04, Vec3::Y);
    }
    let top = p + Vec3::Y * 1.85 + Vec3::new(side.z, 0.0, side.x) * 0.28;
    m.beam(top - side * 0.9, top + side * 0.9, 0.04, 0.04, Vec3::Y);
    let cloth = b.mesh(Mat::Canvas);
    let lo = p + Vec3::Y * 0.7 + Vec3::new(side.z, 0.0, side.x) * 0.17;
    cloth.quad([lo - side * 0.7, lo + side * 0.7, top + side * 0.7 - Vec3::Y * 0.05, top - side * 0.7 - Vec3::Y * 0.05], Vec3::new(side.z, 0.3, side.x));
    let w = b.mesh(Mat::Ceramic);
    for k in 0..8 {
        let q = lo - side * 0.6 + side * (k as f32 * 0.17) - Vec3::Y * 0.2;
        w.lathe(q, &[(0.0, 0.0), (0.05, 0.02), (0.04, 0.1), (0.0, 0.12)], 6);
    }
    b.invisible.push(Aabb::new(p - side * 0.85 - Vec3::new(side.z, 0.0, side.x) * 0.1, p + side * 0.85 + Vec3::new(side.z, 1.9, side.x) * 0.4));
}

/// Навес из небелёного льна на жердях перед лавкой.
pub fn awning(b: &mut Builder, a: Vec3, c: Vec3, out: Vec3, depth: f32, h: f32) {
    let low = h - 0.5;
    let p = [a + Vec3::Y * h, c + Vec3::Y * h, c + out * depth + Vec3::Y * low, a + out * depth + Vec3::Y * low];
    let m = b.mesh(Mat::Canvas);
    m.quad(p, Vec3::Y + out * 0.3);
    m.quad(p, -Vec3::Y - out * 0.3);
    let w = b.mesh(Mat::Wood);
    for q in [p[2], p[3]] {
        w.beam(Vec3::new(q.x, 0.0, q.z), q, 0.04, 0.04, Vec3::X);
    }
}

// ---------------- растения ----------------

/// Крона: несколько сплюснутых «шаров» листвы.
fn canopy(b: &mut Builder, c: Vec3, r: f32, mat: Mat, rng: &mut Rng) {
    let n = 5;
    for k in 0..n {
        let a = k as f32 / n as f32 * TAU + rng.range(0.0, 1.0);
        let off = if k == 0 { Vec3::ZERO } else { Vec3::new(a.cos() * r * 0.55, rng.range(-0.25, 0.3) * r, a.sin() * r * 0.55) };
        let rr = r * if k == 0 { 0.8 } else { rng.range(0.5, 0.7) };
        let prof: Vec<(f32, f32)> = (0..=6).map(|i| {
            let t = -FRAC_PI_2 + PI * i as f32 / 6.0;
            (t.cos() * rr, t.sin() * rr * 0.75)
        }).collect();
        b.mesh(mat).lathe(c + off, &prof, 9);
    }
}

/// Инжир (смоковница) — обычное дерево херсонесских дворов.
pub fn fig_tree(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let m = b.mesh(Mat::Wood);
    m.lathe(p, &[(0.16, 0.0), (0.12, h * 0.45), (0.08, h * 0.55)], 7);
    for k in 0..3 {
        let a = k as f32 * 2.1 + rng.f();
        m.beam(p + Vec3::Y * h * 0.4, p + Vec3::new(a.cos() * h * 0.3, h * 0.7, a.sin() * h * 0.3), 0.06, 0.06, Vec3::Y);
    }
    canopy(b, p + Vec3::Y * h * 0.72, h * 0.42, Mat::Leaves, rng);
    b.invisible.push(Aabb::new(p - Vec3::new(0.18, 0.0, 0.18), p + Vec3::new(0.18, h * 0.5, 0.18)));
}

/// Кипарис.
pub fn cypress(b: &mut Builder, p: Vec3, h: f32) {
    b.mesh(Mat::Wood).lathe(p, &[(0.15, 0.0), (0.12, 0.8)], 6);
    let prof: Vec<(f32, f32)> = (0..=8).map(|i| {
        let t = i as f32 / 8.0;
        let r = (h * 0.13) * (1.0 - t).powf(0.8) * (0.5 + 1.6 * t.min(0.3));
        (r, 0.5 + t * (h - 0.5))
    }).collect();
    b.mesh(Mat::Conifer).lathe(p, &prof, 9);
    b.invisible.push(Aabb::new(p - Vec3::new(0.3, 0.0, 0.3), p + Vec3::new(0.3, h * 0.6, 0.3)));
}

/// Можжевельник высокий (Juniperus excelsa) — кустарник или невысокое дерево крымских склонов.
pub fn juniper(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let m = b.mesh(Mat::Wood);
    m.lathe(p, &[(0.1, 0.0), (0.06, h * 0.4)], 6);
    let n = 3 + rng.int(3);
    for k in 0..n {
        let a = rng.range(0.0, TAU);
        let y = h * rng.range(0.25, 0.7);
        let r = h * rng.range(0.18, 0.3) * (1.0 - y / h * 0.5);
        let c = p + Vec3::new(a.cos() * h * 0.12 * k as f32 / n as f32, y, a.sin() * h * 0.12 * k as f32 / n as f32);
        let prof: Vec<(f32, f32)> = (0..=5).map(|i| {
            let t = -FRAC_PI_2 + PI * i as f32 / 5.0;
            (t.cos() * r, t.sin() * r * 1.3)
        }).collect();
        b.mesh(Mat::Conifer).lathe(c, &prof, 7);
    }
    b.invisible.push(Aabb::new(p - Vec3::new(0.25, 0.0, 0.25), p + Vec3::new(0.25, h * 0.5, 0.25)));
}

/// Крымская сосна.
pub fn pine(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    b.mesh(Mat::Wood).lathe(p, &[(0.22, 0.0), (0.14, h * 0.75), (0.05, h * 0.9)], 7);
    for k in 0..3 {
        let y = h * (0.62 + k as f32 * 0.13);
        let r = h * (0.28 - k as f32 * 0.06);
        let prof: Vec<(f32, f32)> = (0..=5).map(|i| {
            let t = -FRAC_PI_2 + PI * i as f32 / 5.0;
            (t.cos() * r, t.sin() * r * 0.4)
        }).collect();
        let a = rng.range(0.0, TAU);
        b.mesh(Mat::Conifer).lathe(p + Vec3::new(a.cos() * 0.4, y, a.sin() * 0.4), &prof, 9);
    }
    b.invisible.push(Aabb::new(p - Vec3::new(0.25, 0.0, 0.25), p + Vec3::new(0.25, h * 0.6, 0.25)));
}

/// Дуб пушистый (предгорья).
pub fn oak(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let m = b.mesh(Mat::Wood);
    m.lathe(p, &[(0.3, 0.0), (0.22, h * 0.4), (0.15, h * 0.5)], 8);
    for k in 0..4 {
        let a = k as f32 * 1.6 + rng.f();
        m.beam(p + Vec3::Y * h * 0.38, p + Vec3::new(a.cos() * h * 0.32, h * 0.62, a.sin() * h * 0.32), 0.09, 0.09, Vec3::Y);
    }
    canopy(b, p + Vec3::Y * h * 0.68, h * 0.42, Mat::Leaves, rng);
    b.invisible.push(Aabb::new(p - Vec3::new(0.3, 0.0, 0.3), p + Vec3::new(0.3, h * 0.5, 0.3)));
}

/// Виноградная лоза на шпалере (беседка во дворе или ряд в винограднике).
pub fn vine_row(b: &mut Builder, a: Vec3, c: Vec3, h: f32) {
    let len = a.distance(c);
    let n = (len / 1.6).ceil() as usize;
    let dir = (c - a) / n as f32;
    let m = b.mesh(Mat::Wood);
    for k in 0..=n {
        let p = a + dir * k as f32;
        m.beam(p, p + Vec3::Y * h, 0.03, 0.03, Vec3::X);
    }
    m.beam(a + Vec3::Y * h, c + Vec3::Y * h, 0.01, 0.01, Vec3::Y);
    let leaves = b.mesh(Mat::Leaves);
    for k in 0..n {
        let p = a + dir * (k as f32 + 0.5) + Vec3::Y * (h * 0.7);
        leaves.lathe(p, &[(0.0, -0.35), (0.45, -0.1), (0.4, 0.25), (0.0, 0.35)], 6);
    }
}

// ---------------- кузнечный горн ----------------

/// Где стоят части кузницы относительно горна (`o` — пол у горна, `out` — к наковальне).
pub struct ForgeLayout {
    pub fire: Vec3,
    pub anvil: Vec3,
    pub trough: Vec3,
    pub grind: Vec3,
}

pub fn forge_layout(o: Vec3, out: Vec3) -> ForgeLayout {
    let side = Vec3::new(-out.z, 0.0, out.x);
    ForgeLayout { fire: o, anvil: o + out * 2.7, trough: o + out * 1.4 + side * 1.6, grind: o + out * 2.9 - side * 1.9 }
}

/// Полный набор кузнеца: горн (кирпичный под колпаком или полевой из камня и глины),
/// мехи, наковальня на колоде, корыто с водой, точило. Регистрирует рабочее место.
pub fn forge_set(b: &mut Builder, name: &'static str, o: Vec3, out: Vec3, field: bool) {
    let l = forge_layout(o, out);
    let side = Vec3::new(-out.z, 0.0, out.x);
    let h = if field { 0.55 } else { 0.9 };
    // Горн.
    let (mat, hw) = if field { (Mat::Rubble, 0.65) } else { (Mat::Brick, 0.85) };
    b.solid((o - Vec3::new(hw, 0.0, hw)).to_array(), (o + Vec3::new(hw, h, hw)).to_array(), mat);
    b.mesh(Mat::Coals).cuboid(o + Vec3::new(-hw + 0.25, h, -hw + 0.25), o + Vec3::new(hw - 0.25, h + 0.08, hw - 0.25));
    if !field {
        b.mesh(Mat::Brick).spire(o + Vec3::Y * 2.3, 1.0, 1.4, 4);
        b.mesh(Mat::Brick).prism(o + Vec3::Y * 3.4, 0.3, 1.4, 4);
    } else {
        // Глиняная обмазка с соплом для мехов.
        b.mesh(Mat::Earth).lathe(o + Vec3::Y * h, &[(hw, 0.0), (hw * 0.8, 0.18), (hw * 0.6, 0.2)], 10);
    }
    // Мехи (кожаные, с доской) сбоку, сопло — в угли.
    let m = o - side * (hw + 0.7) + Vec3::Y * (h * 0.6);
    b.mesh(if field { Mat::Felt } else { Mat::Canvas }).lathe(m - Vec3::Y * 0.2, &[(0.0, 0.0), (0.38, 0.08), (0.42, 0.25), (0.0, 0.42)], 8);
    b.mesh(Mat::Wood).beam(m + Vec3::Y * 0.1, o - side * (hw - 0.1) + Vec3::Y * (h * 0.8), 0.04, 0.04, Vec3::Y);
    hidden(b, m - Vec3::new(0.45, 0.6, 0.45), m + Vec3::new(0.45, 0.4, 0.45));
    // Наковальня на колоде.
    let a = l.anvil;
    b.mesh(Mat::Wood).lathe(a, &[(0.32, 0.0), (0.3, 0.55), (0.0, 0.55)], 10);
    let along = out;
    b.mesh(Mat::Iron).cuboid(a + Vec3::Y * 0.55 - along.abs() * 0.12 - side.abs() * 0.3, a + Vec3::Y * 0.8 + along.abs() * 0.12 + side.abs() * 0.3);
    b.mesh(Mat::Iron).spire(a + side * 0.3 + Vec3::Y * 0.7, 0.1, 0.25, 4);
    hidden(b, a - Vec3::new(0.38, 0.0, 0.38), a + Vec3::new(0.38, 0.8, 0.38));
    // Молоты и клещи на колоде.
    b.mesh(Mat::Wood).beam(a + Vec3::Y * 0.82 - side * 0.1, a + Vec3::Y * 0.82 - side * 0.1 + along * 0.35, 0.015, 0.015, Vec3::Y);
    b.mesh(Mat::Iron).cuboid(a + Vec3::Y * 0.8 - side * 0.16 - along * 0.05, a + Vec3::Y * 0.88 - side * 0.04 + along * 0.05);
    // Корыто для закалки.
    let t = l.trough;
    let half = along.abs() * 0.35 + side.abs() * 0.7 + Vec3::Y * 0.5;
    b.solid((t - half.with_y(0.0)).to_array(), (t + half).to_array(), Mat::Wood);
    b.decor((t - half.with_y(0.0) + Vec3::new(0.06, 0.5, 0.06)).to_array(), (t + half.with_y(0.51) - Vec3::new(0.06, 0.0, 0.06)).to_array(), Mat::Water);
    // Точило: круг на станке.
    let g = l.grind;
    for s in [-1.0f32, 1.0] {
        b.mesh(Mat::Wood).cuboid(g + side * (0.3 * s) - Vec3::new(0.05, 0.0, 0.05), g + side * (0.3 * s) + Vec3::new(0.05, 0.75, 0.05));
    }
    let c = g + Vec3::Y * 0.75;
    let mm = b.mesh(Mat::Stone);
    for k in 0..10 {
        let ang = k as f32 / 10.0 * TAU;
        let r = (out * ang.cos() + Vec3::Y * ang.sin()) * 0.38;
        mm.beam(c, c + r, 0.13, 0.06, side);
    }
    b.mesh(Mat::Wood).beam(c - side * 0.35, c + side * 0.45, 0.025, 0.025, Vec3::Y);
    hidden(b, g - Vec3::new(0.45, 0.0, 0.45), g + Vec3::new(0.45, 1.1, 0.45));
    b.lamp(o + Vec3::Y * (h + 0.8), Color::srgb(1.0, 0.45, 0.15), 16_000.0, 7.0);
    b.forges.push(crate::level::ForgeSpec { name, kind: crate::smithing_net::StationKind::Forge, fire: o + Vec3::Y * (h + 0.08), anvil: a, trough: l.trough + Vec3::Y * 0.51, grind: l.grind + Vec3::Y * 0.75 });
}

/// Сыродутный горн: глиняная шахта в рост человека, мехи у фурмы, куча шлака и руды.
/// `stand` — где стоит дующий мехами.
pub fn bloomery(b: &mut Builder, name: &'static str, p: Vec3, out: Vec3) {
    let side = Vec3::new(-out.z, 0.0, out.x);
    b.mesh(Mat::Earth).lathe(p, &[(0.75, 0.0), (0.72, 0.4), (0.55, 1.2), (0.36, 1.75), (0.26, 1.78), (0.24, 1.6)], 12);
    b.mesh(Mat::Coals).disc(p + Vec3::Y * 1.62, 0.25, 10, true);
    hidden(b, p - Vec3::new(0.7, 0.0, 0.7), p + Vec3::new(0.7, 1.8, 0.7));
    // Мехи у фурмы.
    let m = p + out * 1.25 + Vec3::Y * 0.3;
    b.mesh(Mat::Felt).lathe(m - Vec3::Y * 0.25, &[(0.0, 0.0), (0.36, 0.08), (0.4, 0.25), (0.0, 0.4)], 8);
    b.mesh(Mat::Earth).beam(m, p + out * 0.6 + Vec3::Y * 0.25, 0.06, 0.06, Vec3::Y);
    // Шлак и руда.
    b.mesh(Mat::DarkStone).lathe(p - side * 1.6 - out * 0.4, &[(0.7, 0.0), (0.0, 0.35)], 8);
    b.mesh(Mat::Rubble).lathe(p + side * 1.6 - out * 0.3, &[(0.55, 0.0), (0.0, 0.4)], 8);
    b.mesh(Mat::Coals).lathe(p + side * 1.5 + out * 1.3, &[(0.5, 0.0), (0.0, 0.3)], 8);
    b.lamp(p + Vec3::Y * 2.0, Color::srgb(1.0, 0.5, 0.2), 8_000.0, 6.0);
    b.forges.push(crate::level::ForgeSpec { name, kind: crate::smithing_net::StationKind::Bloomery, fire: p + Vec3::Y * 1.6, anvil: p + out * 1.9, trough: p + out * 1.9, grind: p + out * 1.9 });
}
