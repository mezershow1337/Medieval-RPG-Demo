//! Строительный набор Херсонеса: стены с проёмами, черепичные крыши, лестницы,
//! утварь (амфоры, пифосы, столы, ложа), деревья.

use super::super::geom::{self, MeshBuilder, WallFrame, arch_height};
use super::super::{Builder, DoorSpec, Mat, PlantKind};
use crate::collision::Aabb;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};

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
        socle(b, f, cursor, gl, y0, d0, d1, mat);
        if group.iter().all(|o| matches!(o, Op::Window { .. })) {
            socle(b, f, gl, gr, y0, d0, d1, mat);
        }
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
    socle(b, f, cursor, s1, y0, d0, d1, mat);
}

/// Цоколь: ряд тёсаных известняковых блоков у земли, чуть шире кладки стены
/// (так строили в Херсонесе: бут на глине стоит на цоколе из тёсаного камня).
fn socle(b: &mut Builder, f: WallFrame, s0: f32, s1: f32, y0: f32, d0: f32, d1: f32, mat: Mat) {
    if s1 - s0 < 0.05 || y0.abs() > 0.3 || !matches!(mat, Mat::Rubble | Mat::Plaster | Mat::OpusMixtum | Mat::Signinum) {
        return;
    }
    let (a, c) = f.bounds((s0, s1), (y0 - 0.15, y0 + 0.42), (d0 - 0.05, d1 + 0.05));
    b.mesh(Mat::Limestone).cuboid(a, c);
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
            // Каменные откосы по бокам проёма (как у дверей), чуть выступают из стены.
            for sd in [l, r] {
                let (a, c) = f.bounds((sd - 0.1, sd + 0.1), (sill, top), (d0 - 0.025, d1 + 0.025));
                b.mesh(trim).cuboid(a, c);
            }
            // Деревянная коробка окна и решётка посередине толщины.
            let m = b.mesh(Mat::Wood);
            for (s0, s1, v0, v1) in [(l + 0.1, l + 0.16, sill, top), (r - 0.16, r - 0.1, sill, top), (l + 0.1, r - 0.1, sill, sill + 0.05), (l + 0.1, r - 0.1, top - 0.05, top)] {
                let (a, c) = f.bounds((s0, s1), (v0, v1), (dm - 0.05, dm + 0.05));
                m.cuboid(a, c);
            }
            let bars = ((r - l) / 0.18).floor().max(1.0) as usize;
            for k in 1..=bars {
                let s = l + (r - l) * k as f32 / (bars + 1) as f32;
                let (a, c) = f.bounds((s - 0.02, s + 0.02), (sill, top), (dm - 0.02, dm + 0.02));
                m.cuboid(a, c);
            }
            let (a, c) = f.bounds((l, r), ((sill + top) / 2.0 - 0.02, (sill + top) / 2.0 + 0.02), (dm - 0.02, dm + 0.02));
            m.cuboid(a, c);
            b.invisible.push(Aabb::new(f.p(l, sill, dm - 0.05), f.p(r, top, dm + 0.05)));
            b.windows.push(Aabb::new(f.p(l, sill, dm - 0.01), f.p(r, top, dm + 0.01)));
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
    b.roof(Mat::Tile, |m| m.gable_roof_slopes(Vec2::new(ro.x0, ro.z0), Vec2::new(ro.x1, ro.z1), e, top, ridge_x));
    {
        let m = b.mesh(Mat::Wood);
        let mut under = MeshBuilder::default();
        under.gable_roof_slopes(Vec2::new(ro.x0, ro.z0), Vec2::new(ro.x1, ro.z1), e - ROOF_T, top - ROOF_T, ridge_x);
        m.append(under.flipped());
    }
    // Толщина кровли по краям: торцы черепицы на карнизах и на свесах у фронтонов.
    // Точки — в осях крыши: `al` вдоль конька, `ac` поперёк.
    let w = |al: f32, y: f32, ac: f32| if ridge_x { Vec3::new(al, y, ac) } else { Vec3::new(ac, y, al) };
    let (a0, a1, c0, c1, cm) = if ridge_x { (ro.x0, ro.x1, ro.z0, ro.z1, r.cz()) } else { (ro.z0, ro.z1, ro.x0, ro.x1, r.cx()) };
    let slope = Vec2::new((c1 - c0) / 2.0, top - e).length();
    roof_edges(b.mesh(Mat::Tile), &w, (a0, a1), [(c0, -1.0), (c1, 1.0)], e, top, cm, slope);
    // Концы стропил под свесом карниза: брусья через 0,7 м от стены до края кровли.
    if over > 0.15 {
        let (cw0, cw1) = if ridge_x { (r.z0, r.z1) } else { (r.x0, r.x1) };
        let (aw0, aw1) = if ridge_x { (r.x0, r.x1) } else { (r.z0, r.z1) };
        let under = ROOF_T + 0.055;
        let m = b.mesh(Mat::Wood);
        let mut al = aw0 + 0.25;
        while al < aw1 - 0.1 {
            for (cw, ce, s) in [(cw0, c0, 1.0f32), (cw1, c1, -1.0)] {
                m.beam(w(al, eave - under, cw + s * 0.02), w(al, e - under, ce + s * 0.04), 0.04, 0.055, Vec3::Y);
            }
            al += 0.7;
        }
    }
    // Калиптеры — настоящие полукруглые желоба поверх стыков тегул, от конька до карниза через
    // каждые полметра (там же, где они нарисованы в текстуре): скат ребристый, карниз волнистый.
    let mut al = a0 + CALYPTER_STEP;
    while al < a1 - 0.1 {
        for ce in [c0, c1] {
            // Только вид: коллизию ската дают плиты кровли (b.roof), желоба в неё не входят.
            b.mesh(Mat::Tile).ridge_pipe(w(al, top - 0.02, cm), w(al, e - 0.005, ce), CALYPTER_R, 5);
        }
        al += CALYPTER_STEP;
    }
    // Конёк — ряд полукруглых калиптеров на растворе.
    b.roof(Mat::Tile, |m| m.ridge_pipe(w(a0 - 0.04, top - 0.03, cm), w(a1 + 0.04, top - 0.03, cm), 0.13, 8));
    // Фронтоны — с обеих сторон: изнутри дома на их месте иначе видно небо.
    let m = b.mesh(gable_mat);
    if ridge_x {
        for (x, s) in [(r.x0, -1.0f32), (r.x1, 1.0)] {
            for f in [s, -s] {
                m.tri(Vec3::new(x, eave, r.z0), Vec3::new(x, eave, r.z1), Vec3::new(x, top, r.cz()), Vec3::X * f);
            }
        }
    } else {
        for (z, s) in [(r.z0, -1.0f32), (r.z1, 1.0)] {
            for f in [s, -s] {
                m.tri(Vec3::new(r.x0, eave, z), Vec3::new(r.x1, eave, z), Vec3::new(r.cx(), top, z), Vec3::Z * f);
            }
        }
    }
}

/// Шаг и полуширина калиптеров (как в текстуре черепицы: колонна 0,5 м).
const CALYPTER_STEP: f32 = 0.5;
const CALYPTER_R: f32 = 0.085;

/// Толщина кровли: тегула с калиптером над обрешёткой.
const ROOF_T: f32 = 0.11;

/// Торцы кровли двускатной крыши: полосы черепицы на карнизах (последние сантиметры развёртки
/// загнуты вниз — тон и стыки те же, что у нижнего ряда) и на свесах у фронтонов (крайний
/// калиптер). `w` переводит (вдоль конька, высота, поперёк) в мир.
fn roof_edges(t: &mut MeshBuilder, w: &dyn Fn(f32, f32, f32) -> Vec3, (a0, a1): (f32, f32), eaves: [(f32, f32); 2], e: f32, top: f32, cm: f32, slope: f32) {
    let l = a1 - a0;
    let lip = 0.12;
    for (ce, s) in eaves {
        t.quad_uv(
            [w(a0, e, ce), w(a1, e, ce), w(a1, e - ROOF_T, ce), w(a0, e - ROOF_T, ce)],
            [Vec2::new(0.0, slope), Vec2::new(l, slope), Vec2::new(l, slope - lip), Vec2::new(0.0, slope - lip)],
            w(0.0, 0.0, s),
        );
        for (ae, u0, u1, out) in [(a0, 0.0, lip, -1.0f32), (a1, l, l - lip, 1.0)] {
            t.quad_uv(
                [w(ae, e, ce), w(ae, top, cm), w(ae, top - ROOF_T, cm), w(ae, e - ROOF_T, ce)],
                [Vec2::new(u0, slope), Vec2::new(u0, 0.0), Vec2::new(u1, 0.0), Vec2::new(u1, slope)],
                w(out, 0.0, 0.0),
            );
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
    let span = if high.along_x() { r.d() } else { r.w() };
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
    b.roof(Mat::Tile, |m| m.lean_roof(hi, lo));
    {
        // Калиптеры по скату (см. двускатную крышу).
        let ax = (hi[1] - hi[0]).normalize_or_zero();
        let n = ((hi[1] - hi[0]).length() / CALYPTER_STEP) as usize;
        for k in 1..n {
            let d = ax * (k as f32 * CALYPTER_STEP);
            b.mesh(Mat::Tile).ridge_pipe(hi[0] + d - Vec3::Y * 0.01, lo[0] + d - Vec3::Y * 0.005, CALYPTER_R, 5);
        }
    }
    let down = Vec3::Y * -ROOF_T;
    let mut under = MeshBuilder::default();
    under.lean_roof([hi[0] + down, hi[1] + down], [lo[0] + down, lo[1] + down]);
    b.mesh(Mat::Wood).append(under.flipped());
    // Торцы черепицы: на карнизе и на боковых свесах (как у двускатной).
    let along = (hi[1] - hi[0]).length();
    let slope_len = (lo[0] - hi[0]).length();
    let out = (lo[0] - hi[0]).with_y(0.0).normalize_or_zero();
    let ax = (hi[1] - hi[0]).normalize_or_zero();
    let lip = 0.12;
    let t = b.mesh(Mat::Tile);
    t.quad_uv(
        [lo[0], lo[1], lo[1] + down, lo[0] + down],
        [Vec2::new(0.0, slope_len), Vec2::new(along, slope_len), Vec2::new(along, slope_len - lip), Vec2::new(0.0, slope_len - lip)],
        out,
    );
    for (h, l, u0, u1, f) in [(hi[0], lo[0], 0.0, lip, -ax), (hi[1], lo[1], along, along - lip, ax)] {
        t.quad_uv([h, l, l + down, h + down], [Vec2::new(u0, 0.0), Vec2::new(u0, slope_len), Vec2::new(u1, slope_len), Vec2::new(u1, 0.0)], f);
    }
    // И у высокого края: без торца между черепицей и настилом видна щель на просвет.
    t.quad_uv([hi[0], hi[1], hi[1] + down, hi[0] + down], [Vec2::new(0.0, lip), Vec2::new(along, lip), Vec2::new(along, 0.0), Vec2::new(0.0, 0.0)], -out);
    // Треугольные торцы между стеной и скатом закрываем кладкой — делает вызывающий (стены выше),
    // а здесь — концы стропил под карнизом.
    let m = b.mesh(Mat::Wood);
    let inward = -out * 0.18 + Vec3::Y * -(ROOF_T + 0.05);
    m.beam(lo[0] + inward, lo[1] + inward, 0.05, 0.05, Vec3::Y);
    // И сами концы стропил под свесом, через 0,7 м.
    let sd = (lo[0] - hi[0]).normalize_or_zero();
    let downb = Vec3::Y * -(ROOF_T + 0.055);
    let n = (along / 0.7) as usize;
    for k in 0..=n {
        let d = ax * (0.2 + k as f32 * 0.7).min(along - 0.1);
        m.beam(lo[0] + d - sd * (over + 0.15) + downb, lo[0] + d - sd * 0.04 + downb, 0.04, 0.055, Vec3::Y);
    }
}

/// Насколько кладка под скатом заходит в толщу кровли: выше настила, но ниже черепицы.
const FILL_LIFT: f32 = 0.04;

/// Кладка под односкатной крышей (те же `r`, `high`, `y_high`, `y_low`, что у [`lean_roof`]):
/// стены доводятся от своего верха до низа кровли. Без неё между стеной и черепицей остаётся
/// щель до метра и крыша будто висит над домом.
/// Стена под высоким краем стоит внутри `r` толщиной `t` (0 — её нет) и сверху кончается на
/// `wall_top`. `ends` — торцевые стены: (от, до) по оси вдоль высокого края и высота их верха;
/// над ними кладка идёт от высокой стены до места, где скат опускается на стену.
/// Верх кладки повторяет скат (тупая стена-коробка или пробила бы черепицу, или оставила щель).
pub fn lean_fill(b: &mut Builder, r: Rect, high: Side, y_high: f32, y_low: f32, t: f32, wall_top: f32, ends: &[(f32, f32, f32)], mat: Mat) {
    let span = if high.along_x() { r.d() } else { r.w() };
    let slope = (y_high - y_low) / span;
    // Верх кладки на расстоянии `s` от высокой стороны внутрь.
    let top = |s: f32| y_high - ROOF_T + FILL_LIFT - slope * s;
    // (вдоль, высота, поперёк от высокой стороны) → мир.
    let w = |al: f32, y: f32, s: f32| match high {
        Side::N => Vec3::new(al, y, r.z0 + s),
        Side::S => Vec3::new(al, y, r.z1 - s),
        Side::W => Vec3::new(r.x0 + s, y, al),
        Side::E => Vec3::new(r.x1 - s, y, al),
    };
    let along = if high.along_x() { Vec3::X } else { Vec3::Z };
    let m = b.mesh(mat);
    // Сечение (поперёк, высота) — выпуклый многоугольник против часовой; вытягиваем его вдоль от `l0` до `l1`.
    let prism = |m: &mut MeshBuilder, l0: f32, l1: f32, prof: &[(f32, f32)]| {
        let at = |l: f32| prof.iter().map(|&(s, y)| w(l, y, s)).collect::<Vec<_>>();
        let uv: Vec<Vec2> = prof.iter().map(|&(s, y)| Vec2::new(s, -y)).collect();
        m.polygon(&at(l0), &uv, -along);
        m.polygon(&at(l1), &uv, along);
        for i in 0..prof.len() {
            let (p, q) = (prof[i], prof[(i + 1) % prof.len()]);
            // Наружу от сечения — перпендикуляр к ребру; снизу кладка стоит на стене, там граней нет.
            let e = Vec2::new(q.0 - p.0, q.1 - p.1);
            if e.length() < 1e-4 || (p.1 == q.1 && e.x > 0.0) {
                continue;
            }
            let n = w(0.0, -e.x, e.y) - w(0.0, 0.0, 0.0);
            let len = e.length();
            m.quad_uv([w(l0, p.1, p.0), w(l1, p.1, p.0), w(l1, q.1, q.0), w(l0, q.1, q.0)], [Vec2::ZERO, Vec2::new(l1 - l0, 0.0), Vec2::new(l1 - l0, len), Vec2::new(0.0, len)], n);
        }
    };
    let (a0, a1) = if high.along_x() { (r.x0, r.x1) } else { (r.z0, r.z1) };
    // Высокая стена: трапеция от её верха до ската.
    if t > 0.0 && top(t) > wall_top + 0.005 {
        prism(m, a0, a1, &[(0.0, wall_top), (t, wall_top), (t, top(t)), (0.0, top(0.0))]);
    }
    // Торцы: треугольник от высокой стены до места, где скат садится на стену (у самого низа —
    // трапеция, если стена торца ниже низкого края крыши).
    for &(l0, l1, y0) in ends {
        let s_end = ((top(0.0) - y0) / slope).min(span);
        if s_end <= t + 0.01 || top(t) <= y0 + 0.005 {
            continue;
        }
        let prof: Vec<(f32, f32)> = if top(s_end) > y0 + 0.005 { vec![(t, y0), (s_end, y0), (s_end, top(s_end)), (t, top(t))] } else { vec![(t, y0), (s_end, y0), (t, top(t))] };
        prism(m, l0, l1, &prof);
    }
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
    let pts = [Vec3::new(r.x0, eave, r.z0), Vec3::new(r.x1, eave, r.z0), Vec3::new(r.x1, eave, r.z1), Vec3::new(r.x0, eave, r.z1)];
    let apex = c + Vec3::Y * rise;
    b.roof(Mat::Tile, |m| {
        for i in 0..4 {
            let (a, bb) = (pts[i], pts[(i + 1) % 4]);
            let out = ((a + bb) / 2.0 - c).normalize() + Vec3::Y * 0.5;
            let w = a.distance(bb);
            let sl = ((a + bb) / 2.0).distance(apex);
            m.quad_uv([a, bb, apex, apex], [Vec2::new(0.0, sl), Vec2::new(w, sl), Vec2::new(w / 2.0, 0.0), Vec2::new(w / 2.0, 0.0)], out);
        }
    });
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
    m.lathe(p, &scaled, 16);
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
    b.mesh(Mat::Ceramic).lathe(base, &prof, 20);
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
    b.mesh(mat).lathe(p, &prof, 12);
}

/// Невидимая коллизия (форму рисует декоративный меш).
pub fn hidden(b: &mut Builder, min: Vec3, max: Vec3) {
    b.invisible.push(Aabb::new(min, max));
}

/// Стол на четырёх ножках: столешница из отдельных досок, царги под ней, проножки.
pub fn table(b: &mut Builder, r: Rect, h: f32) {
    b.invisible.push(Aabb::new(r.min3(0.0), r.max3(h)));
    let m = b.mesh(Mat::Wood);
    let along_x = r.w() > r.d();
    // Доски идут вдоль длинной стороны; между ними щели, толщина чуть гуляет.
    let width = if along_x { r.d() } else { r.w() };
    let n = ((width / 0.17).round() as usize).max(2);
    let pw = width / n as f32;
    for i in 0..n {
        let (a, c) = (i as f32 * pw + 0.004, (i + 1) as f32 * pw - 0.004);
        let dy = ((i * 7 + 3) % 5) as f32 * 0.0015;
        let end = ((i * 5 + 1) % 4) as f32 * 0.006;
        if along_x {
            m.cuboid(Vec3::new(r.x0 + end, h - 0.045, r.z0 + a), Vec3::new(r.x1 - end * 0.5, h - dy, r.z0 + c));
        } else {
            m.cuboid(Vec3::new(r.x0 + a, h - 0.045, r.z0 + end), Vec3::new(r.x0 + c, h - dy, r.z1 - end * 0.5));
        }
    }
    let leg = 0.045;
    let ins = 0.09;
    for (x, z) in [(r.x0 + ins, r.z0 + ins), (r.x1 - ins, r.z0 + ins), (r.x0 + ins, r.z1 - ins), (r.x1 - ins, r.z1 - ins)] {
        m.cuboid(Vec3::new(x - leg, 0.0, z - leg), Vec3::new(x + leg, h - 0.045, z + leg));
    }
    // Царги под столешницей и проножки у пола.
    for (y0, y1, t) in [(h - 0.14, h - 0.045, 0.018), (h * 0.22, h * 0.22 + 0.05, 0.02)] {
        if r.w() > 0.4 {
            for z in [r.z0 + ins, r.z1 - ins] {
                m.cuboid(Vec3::new(r.x0 + ins, y0, z - t), Vec3::new(r.x1 - ins, y1, z + t));
            }
        }
        if r.d() > 0.4 {
            for x in [r.x0 + ins, r.x1 - ins] {
                m.cuboid(Vec3::new(x - t, y0, r.z0 + ins), Vec3::new(x + t, y1, r.z1 - ins));
            }
        }
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
    let along = if r.w() > r.d() { Vec3::X } else { Vec3::Z };
    b.bed(r.center(0.62), along);
}

/// Сундук.
/// Сундук, который можно обыскать (добыча — по текущей таблице сборщика).
pub fn chest(b: &mut Builder, p: Vec3, along_x: bool) {
    chest_body(b, p, along_x);
    b.container(p + Vec3::Y * 0.7, "Сундук");
}

/// Только сам сундук (без добычи): ящик на ножках, выступающая крышка, железные полосы,
/// петли и накладка замка.
pub fn chest_body(b: &mut Builder, p: Vec3, along_x: bool) {
    let (hx, hz) = if along_x { (0.5, 0.3) } else { (0.3, 0.5) };
    hidden(b, Vec3::new(p.x - hx, 0.0, p.z - hz), Vec3::new(p.x + hx, 0.57, p.z + hz));
    let m = b.mesh(Mat::Wood);
    // Ножки-бруски и корпус.
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let c = Vec3::new(p.x + sx * (hx - 0.05), 0.0, p.z + sz * (hz - 0.05));
        m.cuboid(c - Vec3::new(0.05, 0.0, 0.05), c + Vec3::new(0.05, 0.06, 0.05));
    }
    m.cuboid(Vec3::new(p.x - hx + 0.012, 0.04, p.z - hz + 0.012), Vec3::new(p.x + hx - 0.012, 0.44, p.z + hz - 0.012));
    // Крышка: шире корпуса, с двускатной «спинкой».
    m.cuboid(Vec3::new(p.x - hx, 0.445, p.z - hz), Vec3::new(p.x + hx, 0.52, p.z + hz));
    let (lx, lz) = if along_x { (hx, hz * 0.55) } else { (hx * 0.55, hz) };
    m.cuboid(Vec3::new(p.x - lx, 0.52, p.z - lz), Vec3::new(p.x + lx, 0.57, p.z + lz));
    let i = b.mesh(Mat::Iron);
    let e = 0.006;
    // Полосы поперёк сундука (через корпус и крышку), петли и замок.
    for s in [-0.62f32, 0.62] {
        if along_x {
            let x = p.x + s * hx;
            i.cuboid(Vec3::new(x - 0.025, 0.04, p.z - hz + 0.012 - e), Vec3::new(x + 0.025, 0.44, p.z + hz - 0.012 + e));
            i.cuboid(Vec3::new(x - 0.025, 0.445, p.z - hz - e), Vec3::new(x + 0.025, 0.52 + e, p.z + hz + e));
        } else {
            let z = p.z + s * hz;
            i.cuboid(Vec3::new(p.x - hx + 0.012 - e, 0.04, z - 0.025), Vec3::new(p.x + hx - 0.012 + e, 0.44, z + 0.025));
            i.cuboid(Vec3::new(p.x - hx - e, 0.445, z - 0.025), Vec3::new(p.x + hx + e, 0.52 + e, z + 0.025));
        }
    }
    for s in [-1.0f32, 1.0] {
        if along_x {
            let z = p.z + s * hz;
            i.cuboid(Vec3::new(p.x - 0.045, 0.33, z - 0.012 - e), Vec3::new(p.x + 0.045, 0.5, z + e));
        } else {
            let x = p.x + s * hx;
            i.cuboid(Vec3::new(x - 0.012 - e, 0.33, p.z - 0.045), Vec3::new(x + e, 0.5, p.z + 0.045));
        }
    }
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

/// Бревно от `a` до `c` (радиус у комля `r0`, у вершины `r1`): чуть кривое, неровное, с торцами.
/// Развёртка: u — вдоль ствола (м), v — по окружности (м); торцы — u = 1000 + доля радиуса.
pub fn log(b: &mut Builder, a: Vec3, c: Vec3, r0: f32, r1: f32, seed: u32) {
    let len = a.distance(c);
    if len < 0.02 {
        return;
    }
    let d = (c - a) / len;
    let up = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let s = d.cross(up).normalize();
    let w = s.cross(d);
    let mut rng = Rng::new(seed.wrapping_mul(7919).wrapping_add(17));
    let ba = rng.range(0.0, TAU);
    let bow = (s * ba.cos() + w * ba.sin()) * len * rng.range(0.005, 0.014);
    let phase = rng.range(0.0, 10.0);
    let knot = rng.range(0.03, 0.06);
    let sides = if r0.max(r1) > 0.075 { 11 } else { 7 };
    let n = ((len / 0.4).ceil() as usize).max(2);
    let circ = TAU * (r0 + r1) * 0.5;
    let ring_r = |t: f32, k: usize| {
        let r = r0 + (r1 - r0) * t;
        let ang = k as f32 / sides as f32 * TAU;
        r * (1.0 + knot * (ang * 2.0 + phase + t * len * 2.3).sin() * (t * len * 1.7 + phase).cos() + 0.03 * (t * len * 4.1 + phase).sin())
    };
    let center = |t: f32| a + d * (len * t) + bow * (std::f32::consts::PI * t).sin();
    let m = b.mesh(Mat::Timber);
    let mut rings: Vec<Vec<u32>> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let ctr = center(t);
        let ring: Vec<u32> = (0..=sides)
            .map(|k| {
                let ang = k as f32 / sides as f32 * TAU;
                let dir = s * ang.cos() + w * ang.sin();
                m.vert(ctr + dir * ring_r(t, k % sides), dir, Vec2::new(t * len, k as f32 / sides as f32 * circ))
            })
            .collect();
        rings.push(ring);
    }
    for i in 0..n {
        for k in 0..sides {
            let (a0, a1, b0, b1) = (rings[i][k], rings[i][k + 1], rings[i + 1][k], rings[i + 1][k + 1]);
            m.index_tri(a0, b0, b1);
            m.index_tri(a0, b1, a1);
        }
    }
    // Торцы: плоские срезы с кольцами.
    for (t, out) in [(0.0f32, -d), (1.0, d)] {
        let ctr = center(t);
        let r = r0 + (r1 - r0) * t;
        let cv = m.vert(ctr, out, Vec2::new(1000.0, r));
        let rim: Vec<u32> = (0..=sides)
            .map(|k| {
                let ang = k as f32 / sides as f32 * TAU;
                let dir = s * ang.cos() + w * ang.sin();
                m.vert(ctr + dir * ring_r(t, k % sides), out, Vec2::new(1001.0, r))
            })
            .collect();
        for k in 0..sides {
            if t == 0.0 {
                m.index_tri(cv, rim[k], rim[k + 1]);
            } else {
                m.index_tri(cv, rim[k + 1], rim[k]);
            }
        }
    }
}

/// Штабель дров: настоящие круглые поленья с торцами, уложенные «в шахматку».
pub fn woodpile(b: &mut Builder, r: Rect, h: f32) {
    hidden(b, r.min3(0.0), r.max3(h));
    let along_x = r.w() > r.d();
    let (len, depth) = if along_x { (r.w(), r.d()) } else { (r.d(), r.w()) };
    let mut rng = Rng::new((r.x0 * 37.0 + r.z0 * 91.0) as i32 as u32 ^ 0x51ED);
    let rr = 0.075;
    let rows = ((h - rr) / (rr * 1.74)).floor().max(0.0) as usize + 1;
    for j in 0..rows {
        let y = rr + j as f32 * rr * 1.74;
        let n = (len / (rr * 2.0)) as usize - (j % 2);
        let off = (len - n as f32 * rr * 2.0) * 0.5 + rr;
        for i in 0..n {
            // Верхний ряд неполный — как у начатой поленницы.
            if j + 1 == rows && rows > 1 && rng.chance(0.3) {
                continue;
            }
            let t = off + i as f32 * rr * 2.0 + rng.range(-0.008, 0.008);
            let rad = rr * rng.range(0.78, 1.0);
            let yy = y - (rr - rad) + rng.range(0.0, 0.006);
            let (o0, o1) = (rng.range(-0.07, 0.03), rng.range(-0.03, 0.07));
            let (a, c) = if along_x {
                (Vec3::new(r.x0 + t, yy, r.z0 + o0), Vec3::new(r.x0 + t + rng.range(-0.01, 0.01), yy, r.z0 + depth + o1))
            } else {
                (Vec3::new(r.x0 + o0, yy, r.z0 + t), Vec3::new(r.x0 + depth + o1, yy, r.z0 + t + rng.range(-0.01, 0.01)))
            };
            log(b, a, c, rad, rad * rng.range(0.86, 0.98), rng.f().to_bits());
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
    awning_of(b, a, c, out, depth, h, Mat::Canvas);
}

/// Навес из полотна `cloth` (некрашеный холст, крашенная мареной или вайдой шерсть).
pub fn awning_of(b: &mut Builder, a: Vec3, c: Vec3, out: Vec3, depth: f32, h: f32, cloth: Mat) {
    let low = h - 0.5;
    let p = [a + Vec3::Y * h, c + Vec3::Y * h, c + out * depth + Vec3::Y * low, a + out * depth + Vec3::Y * low];
    // Полотно натянуто — по нему, как по крыше, не провалиться.
    b.roof(cloth, |m| {
        m.quad(p, Vec3::Y + out * 0.3);
        m.quad(p, -Vec3::Y - out * 0.3);
    });
    let m = b.mesh(cloth);
    // Подзор: полотно свисает с переднего края полосой с вырезами-«языками».
    let drop = Vec3::Y * -0.24;
    let edge = p[2] - p[3];
    let n = ((edge.length() / 0.45).round() as usize).max(2);
    for k in 0..n {
        let a = p[3] + edge * (k as f32 / n as f32);
        let c = p[3] + edge * ((k + 1) as f32 / n as f32);
        let mid = (a + c) * 0.5 + drop * 1.25;
        let quad = [a, c, c + drop, a + drop];
        m.quad(quad, out);
        m.quad([quad[1], quad[0], quad[3], quad[2]], -out);
        m.tri(a + drop, c + drop, mid, out);
        m.tri(a + drop, c + drop, mid, -out);
    }
    let w = b.mesh(Mat::Wood);
    for q in [p[2], p[3]] {
        w.beam(Vec3::new(q.x, 0.0, q.z), q, 0.04, 0.04, Vec3::X);
    }
    // Жердь по переднему краю, на ней натянуто полотно.
    w.beam(p[3] + out * 0.02, p[2] + out * 0.02, 0.03, 0.03, Vec3::Y);
}

// ---------------- растения ----------------


/// Поворот и вариант модели дерева. Берёт из `rng` ровно `draws` чисел — столько же, сколько
/// брали прежние деревья из примитивов, чтобы остальная планировка (сады, дворы, жители) не сдвинулась.
fn look_of(rng: &mut Rng, draws: usize) -> (f32, u32) {
    let (a, c) = (rng.f(), rng.f());
    for _ in 2..draws {
        rng.f();
    }
    (a * TAU, (c * 16.0) as u32 % 16)
}

/// Инжир (смоковница) — обычное дерево херсонесских дворов.
pub fn fig_tree(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let (yaw, v) = look_of(rng, 16);
    b.plant(PlantKind::Fig, p, h, yaw, v);
    b.invisible.push(Aabb::new(p - Vec3::new(0.18, 0.0, 0.18), p + Vec3::new(0.18, h * 0.5, 0.18)));
}

/// Кипарис.
pub fn cypress(b: &mut Builder, p: Vec3, h: f32) {
    let v = ((p.x * 7.3 + p.z * 3.1).abs() as u32) % 16;
    b.plant(PlantKind::Cypress, p, h, (p.x * 0.37 + p.z * 0.11) % TAU, v);
    b.invisible.push(Aabb::new(p - Vec3::new(0.3, 0.0, 0.3), p + Vec3::new(0.3, h * 0.6, 0.3)));
}

/// Можжевельник высокий (Juniperus excelsa) — кустарник или невысокое дерево крымских склонов.
pub fn juniper(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let n = 3 + rng.int(3);
    let (yaw, v) = look_of(rng, 3 * n);
    b.plant(PlantKind::Juniper, p, h, yaw, v);
    b.invisible.push(Aabb::new(p - Vec3::new(0.25, 0.0, 0.25), p + Vec3::new(0.25, h * 0.5, 0.25)));
}

/// Крымская сосна.
pub fn pine(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let (yaw, v) = look_of(rng, 3);
    // В бору сосны стройные, конусом, с хвоей до земли (три из четырёх); старые одиночные —
    // зонтиком. Вид берём из номера варианта — лишних чисел из `rng` не тянем.
    if v % 4 != 0 {
        b.plant(PlantKind::Fir, p, h * 1.3, yaw, v / 4 + v);
    } else {
        b.plant(PlantKind::Pine, p, h, yaw, v);
    }
    b.invisible.push(Aabb::new(p - Vec3::new(0.25, 0.0, 0.25), p + Vec3::new(0.25, h * 0.6, 0.25)));
}

/// Дуб пушистый (предгорья).
pub fn oak(b: &mut Builder, p: Vec3, h: f32, rng: &mut Rng) {
    let (yaw, v) = look_of(rng, 17);
    b.plant(PlantKind::Oak, p, h, yaw, v);
    b.invisible.push(Aabb::new(p - Vec3::new(0.3, 0.0, 0.3), p + Vec3::new(0.3, h * 0.5, 0.3)));
}

/// Кочан в розетке листьев (капуста, свёкла, салат): округлая голова, вокруг неё три листа
/// чашей и пять отогнутых наружу, разной длины. `turn` — поворот розетки, у каждого кочана свой.
/// Треугольников ~270: огородов много, а кадр должен держать 60 FPS.
pub fn cabbage(b: &mut Builder, mat: Mat, p: Vec3, s: f32, tall: f32, turn: f32) {
    let m = b.mesh(mat);
    let (r, h) = (0.125 * s, 0.2 * s * tall);
    let head: Vec<(f32, f32)> = (0..=5)
        .map(|k| {
            let t = k as f32 / 5.0 * std::f32::consts::PI;
            (r * t.sin().max(0.0), 0.025 * s + h * 0.5 * (1.0 - t.cos()))
        })
        .collect();
    m.lathe(p, &head, 8);
    // Листья чашей: прижаты к голове, верх чуть отходит.
    for k in 0..3 {
        let a = turn + 0.5 + k as f32 * TAU / 3.0;
        let cup = [(0.05 * s, 0.02 * s), (0.15 * s, 0.09 * s * tall), (0.165 * s, 0.19 * s * tall), (0.14 * s, 0.26 * s * tall)];
        // Кончик листа уже основания: с ровным срезом лист выглядел бумажной лопастью.
        for inward in [false, true] {
            m.lathe_arc(p, &cup[..3], a - 0.75, a + 0.75, 2, inward);
            m.lathe_arc(p, &cup[2..], a - 0.42, a + 0.42, 2, inward);
        }
    }
    // Наружные листья: лежат розеткой, через один короче и ниже.
    for k in 0..5 {
        let f = ((turn * 7.3 + k as f32 * 2.1).sin() * 0.5 + 0.5) * 0.3;
        let a = turn + k as f32 * TAU / 5.0 + f;
        let (len, lift) = if k % 2 == 0 { (1.0 + f * 0.4, 1.0) } else { (0.82, 0.55 + f) };
        let leaf = [(0.05 * s, 0.012), (0.16 * s * len, 0.035 * s * lift), (0.25 * s * len, 0.1 * s * lift * tall), (0.31 * s * len, 0.19 * s * lift * tall)];
        for inward in [false, true] {
            m.lathe_arc(p, &leaf[..3], a - 0.5, a + 0.5, 2, inward);
            m.lathe_arc(p, &leaf[2..], a - 0.27, a + 0.27, 1, inward);
        }
    }
}

/// Перья лука-порея и чеснока: пучок узких листьев веером, кончики отгибаются.
pub fn leek(b: &mut Builder, mat: Mat, p: Vec3, s: f32, turn: f32) {
    let m = b.mesh(mat);
    for k in 0..5 {
        let f = (turn * 5.1 + k as f32 * 1.7).sin() * 0.5 + 0.5;
        let a = turn + k as f32 * TAU / 5.0 + f * 0.6;
        let h = (0.3 + 0.16 * f) * s;
        let blade = [(0.006, 0.0), (0.03 * s, h * 0.45), (0.075 * s, h * 0.85), (0.14 * s, h * (0.98 - 0.12 * f))];
        m.lathe_arc(p, &blade, a - 0.22, a + 0.22, 1, false);
        m.lathe_arc(p, &blade, a - 0.22, a + 0.22, 1, true);
    }
}

/// Кустик зелени (мята, шалфей, салат): низкая пышная розетка листьев в два яруса.
pub fn herb(b: &mut Builder, mat: Mat, p: Vec3, s: f32, turn: f32) {
    let m = b.mesh(mat);
    for k in 0..7 {
        let f = (turn * 4.3 + k as f32 * 2.3).sin() * 0.5 + 0.5;
        let a = turn + k as f32 * TAU / 7.0 + f * 0.4;
        let up = if k % 2 == 0 { 1.0 } else { 1.7 };
        let reach = if k % 2 == 0 { 1.0 } else { 0.6 };
        let leaf = [(0.015, 0.01), (0.08 * s * reach, 0.05 * s * up), (0.15 * s * reach, 0.09 * s * up), (0.2 * s * reach, (0.08 + 0.03 * f) * s * up)];
        m.lathe_arc(p, &leaf, a - 0.36, a + 0.36, 1, false);
        m.lathe_arc(p, &leaf, a - 0.36, a + 0.36, 1, true);
    }
}

/// Куст шибляка (шиповник, боярышник, держидерево) — без коллизии.
/// Поворот и вариант — от положения, `rng` не трогаем (куст встал на место прежних «холмиков»).
pub fn shrub(b: &mut Builder, p: Vec3, h: f32) {
    let k = (p.x * 12.9898 + p.z * 78.233).sin() * 43758.547;
    let f = k - k.floor();
    b.plant(PlantKind::Shrub, p, h, f * TAU, (f * 997.0) as u32 % 16);
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
    let along = out;
    anvil(b, a, side, out);
    // Вёдра, окалина у горна, прутки.
    bucket(b, a - side * 0.7 + out * 0.3);
    let mut r = Rng::new((o.x * 13.0 + o.z * 7.0) as i32 as u32);
    scrap(b, o + out * 1.3 - side * 1.1, &mut r);
    if !field {
        bar_stock(b, o - out * (hw + 0.1) + side * 1.3, out, side);
    }
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

/// Наковальня на дубовой колоде: широкое основание, перехват, рабочая плита и короткий рог
/// (поздняя античность — блочная наковальня с небольшим клювом). `along` — вдоль плиты.
pub fn anvil(b: &mut Builder, a: Vec3, along: Vec3, side: Vec3) {
    b.mesh(Mat::Wood).lathe(a, &[(0.34, 0.0), (0.31, 0.06), (0.3, 0.5), (0.27, 0.55), (0.0, 0.55)], 12);
    // Железные обручи на колоде.
    for y in [0.08, 0.45] {
        b.mesh(Mat::Iron).lathe(a + Vec3::Y * y, &[(0.315, 0.0), (0.315, 0.035), (0.0, 0.035)], 12);
    }
    let y0 = a + Vec3::Y * 0.55;
    let m = b.mesh(Mat::Iron);
    m.beam(y0 - along * 0.17, y0 + along * 0.17, 0.13, 0.06, side);
    m.beam(y0 + Vec3::Y * 0.06 - along * 0.11, y0 + Vec3::Y * 0.06 + along * 0.11, 0.08, 0.07, side);
    m.beam(y0 + Vec3::Y * 0.16 - along * 0.26, y0 + Vec3::Y * 0.16 + along * 0.2, 0.12, 0.055, side);
    // Клюв.
    m.beam(y0 + Vec3::Y * 0.17 + along * 0.2, y0 + Vec3::Y * 0.2 + along * 0.4, 0.06, 0.035, side);
    m.beam(y0 + Vec3::Y * 0.2 + along * 0.4, y0 + Vec3::Y * 0.23 + along * 0.48, 0.02, 0.015, side);
    hidden(b, a - Vec3::new(0.38, 0.0, 0.38), a + Vec3::new(0.38, 0.8, 0.38));
}

/// Молот: рукоять и железный боёк. `p` — конец рукояти, `dir` — к бойку.
pub fn hammer(b: &mut Builder, p: Vec3, dir: Vec3, up: Vec3, len: f32, head: f32) {
    let top = p + dir * len;
    b.mesh(Mat::Wood).beam(p, top, 0.018, 0.022, up);
    b.mesh(Mat::Iron).beam(top - up * head * 0.5, top + up * head * 0.5, 0.034, 0.034, dir);
}

/// Клещи: две сведённые губки и расходящиеся рукояти.
pub fn tongs(b: &mut Builder, jaw: Vec3, dir: Vec3, open: Vec3, len: f32) {
    let pivot = jaw + dir * 0.12;
    let m = b.mesh(Mat::Iron);
    for s in [-1.0f32, 1.0] {
        m.beam(jaw + open * 0.015 * s, pivot - open * 0.015 * s, 0.014, 0.018, open);
        m.beam(pivot, pivot + dir * len + open * 0.06 * s, 0.011, 0.011, open);
    }
}

/// Доска с инструментом на стене: клещи, молоты, напильник. `p` — середина доски у стены,
/// `n` — от стены в комнату, `along` — вдоль стены.
pub fn tool_rack(b: &mut Builder, p: Vec3, n: Vec3, along: Vec3) {
    b.mesh(Mat::Wood).beam(p - along * 0.8 + n * 0.02, p + along * 0.8 + n * 0.02, 0.2, 0.02, n);
    // Колышки.
    for k in 0..7 {
        let q = p + along * (-0.69 + k as f32 * 0.23) + Vec3::Y * 0.12 + n * 0.04;
        b.mesh(Mat::Wood).beam(q, q + n * 0.08, 0.01, 0.01, Vec3::Y);
        let hang = q + n * 0.07;
        match k {
            0 | 2 | 5 => tongs(b, hang - Vec3::Y * 0.62, Vec3::Y, along, 0.42),
            1 | 4 => hammer(b, hang - Vec3::Y * 0.5, Vec3::Y, n, 0.42, 0.16),
            3 => b.mesh(Mat::Iron).beam(hang, hang - Vec3::Y * 0.4, 0.012, 0.004, n),
            _ => hammer(b, hang - Vec3::Y * 0.36, Vec3::Y, n, 0.3, 0.1),
        }
    }
}

/// Прутки и полосы железа, прислонённые к стене. `p` — у основания стены, `n` — от стены.
pub fn bar_stock(b: &mut Builder, p: Vec3, n: Vec3, along: Vec3) {
    let m = b.mesh(Mat::Iron);
    for k in 0..8 {
        let o = along * (k as f32 * 0.06 - 0.21);
        let h = 1.1 + (k % 3) as f32 * 0.15;
        let foot = p + o + n * (0.28 + (k % 2) as f32 * 0.05);
        let w = if k % 3 == 0 { 0.018 } else { 0.01 };
        m.beam(foot, p + o + n * 0.03 + Vec3::Y * h, w, 0.008, n);
    }
}

/// Готовый товар кузнеца на верстаке: наконечники копий, ножи, топор, гвозди.
pub fn smith_goods(b: &mut Builder, top: Vec3, along: Vec3, depth: Vec3) {
    let m = b.mesh(Mat::Iron);
    for k in 0..3 {
        let c = top + along * (-0.55 + k as f32 * 0.2) + Vec3::Y * 0.01;
        m.beam(c - depth * 0.18, c + depth * 0.12, 0.012, 0.006, Vec3::Y);
        m.beam(c + depth * 0.12, c + depth * 0.25, 0.03, 0.005, Vec3::Y);
    }
    for k in 0..2 {
        let c = top + along * (0.1 + k as f32 * 0.12) + Vec3::Y * 0.01;
        b.mesh(Mat::Iron).beam(c - depth * 0.12, c + depth * 0.14, 0.016, 0.004, Vec3::Y);
        b.mesh(Mat::Wood).beam(c - depth * 0.22, c - depth * 0.12, 0.012, 0.012, Vec3::Y);
    }
    // Топор.
    let c = top + along * 0.5 + Vec3::Y * 0.02;
    b.mesh(Mat::Wood).beam(c - depth * 0.25, c + depth * 0.2, 0.015, 0.015, Vec3::Y);
    b.mesh(Mat::Iron).beam(c + depth * 0.16 - along * 0.02, c + depth * 0.16 + along * 0.12, 0.03, 0.04, Vec3::Y);
    // Горка гвоздей.
    b.mesh(Mat::Iron).lathe(top + along * 0.32 - depth * 0.15, &[(0.07, 0.0), (0.0, 0.04)], 7);
}

/// Деревянное ведро с водой.
pub fn bucket(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Wood).lathe(p, &[(0.0, 0.0), (0.15, 0.0), (0.18, 0.32), (0.16, 0.32), (0.13, 0.03), (0.0, 0.03)], 10);
    b.mesh(Mat::Iron).lathe(p + Vec3::Y * 0.24, &[(0.176, 0.0), (0.176, 0.03), (0.0, 0.03)], 10);
    b.mesh(Mat::Water).disc(p + Vec3::Y * 0.26, 0.16, 10, true);
}

/// Куча окалины и обрезков у горна.
pub fn scrap(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    b.mesh(Mat::DarkStone).lathe(p, &[(0.3, 0.0), (0.15, 0.08), (0.0, 0.1)], 9);
    let m = b.mesh(Mat::Iron);
    for _ in 0..6 {
        let a = rng.range(0.0, TAU);
        let c = p + Vec3::new(a.cos(), 0.0, a.sin()) * rng.range(0.0, 0.35) + Vec3::Y * 0.06;
        let d = Vec3::new(rng.range(-1.0, 1.0), 0.1, rng.range(-1.0, 1.0)).normalize() * rng.range(0.1, 0.3);
        m.beam(c - d, c + d, 0.012, 0.008, Vec3::Y);
    }
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
