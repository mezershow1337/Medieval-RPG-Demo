//! Оборонительные стены Херсонеса: куртины из крупных известняковых блоков с зубцами,
//! прямоугольные башни, круглые угловые башни, башня Зенона, ворота, протейхизма.

use super::kit::{self, Op};
use super::*;
use crate::collision::Aabb;
use std::f32::consts::TAU;

const TOWER_ALONG: f32 = 9.0;
const TOWER_OUT: f32 = 5.5;
const TOWER_H: f32 = 12.0;
const GATE_W: f32 = 4.5;
/// Протейхизма: ось и параметры.
pub const PROTEICHISMA_Z: f32 = 182.0;
const PROT_T: f32 = 2.2;
const PROT_H: f32 = 5.0;
const PROT_X: (f32, f32) = (-152.0, 262.0);

/// Начало и конец лестницы на стену у главных ворот.
pub fn gate_stairs_start() -> (Vec3, Vec3) {
    let x = main_gate_x() + 9.0;
    let z = SOUTH_WALL_Z - WALL_T / 2.0 - 0.8;
    (Vec3::new(x - 1.0, 0.0, z), Vec3::new(x + 13.2, 0.0, z))
}

pub fn fortifications(b: &mut Builder) {
    let ring = terrain::CITY_RING;
    let n = ring.len();
    for i in 0..n {
        let a = Vec2::from(ring[i]);
        let c = Vec2::from(ring[(i + 1) % n]);
        b.at((a.x + c.x) / 2.0, (a.y + c.y) / 2.0);
        b.vary(i * 4);
        if a.x == EAST_WALL_X && c.x == EAST_WALL_X {
            east_wall(b, a, c);
        } else if a.y == SOUTH_WALL_Z && c.y == SOUTH_WALL_Z {
            south_wall(b, a, c);
        } else {
            diagonal_curtain(b, a, c);
        }
    }
    // Круглые башни на углах (кроме юго-восточного — там башня Зенона).
    for (i, &(x, z)) in ring.iter().enumerate() {
        b.at(x, z);
        b.vary(i * 2 + 1);
        if (x, z) == (EAST_WALL_X, SOUTH_WALL_Z) {
            zeno_tower(b, Vec2::new(x, z));
        } else {
            round_tower(b, Vec2::new(x, z), 5.5, TOWER_H);
        }
    }
    b.variant = 0;
    proteichisma(b);
    stairs_to_walls(b);
}

/// Отрезки стены между башнями: башня (шириной [`TOWER_ALONG`], середина в `towers`) сама
/// закрывает свой кусок, так что прясла по обе стороны от неё не налегают друг на друга и
/// могут быть из разных вариантов кладки.
fn spans(s0: f32, s1: f32, towers: &[f32]) -> Vec<(f32, f32)> {
    let mut t: Vec<f32> = towers.iter().copied().filter(|t| *t - TOWER_ALONG / 2.0 > s0 + 0.5 && *t + TOWER_ALONG / 2.0 < s1 - 0.5).collect();
    t.sort_by(|a, b| a.total_cmp(b));
    let mut out = Vec::new();
    let mut from = s0;
    for t in t {
        out.push((from, t - TOWER_ALONG / 2.0));
        from = t + TOWER_ALONG / 2.0;
    }
    out.push((from, s1));
    out
}

fn outward(a: Vec2, c: Vec2) -> Vec2 {
    let d = c - a;
    Vec2::new(d.y, -d.x).normalize()
}

/// Повёрнутый параллелепипед: рисуется точно, коллизия — коробками по 2 м вдоль.
fn oriented_solid(b: &mut Builder, a: Vec2, c: Vec2, d0: f32, d1: f32, y0: f32, y1: f32, mat: Mat) {
    let dir = (c - a).normalize();
    let nrm = Vec2::new(dir.y, -dir.x);
    let len = a.distance(c);
    let mid = (a + c) / 2.0 + nrm * (d0 + d1) / 2.0;
    let yaw = dir.y.atan2(dir.x);
    b.mesh(mat).obox(Vec3::new(mid.x, (y0 + y1) / 2.0, mid.y), Vec3::new(len / 2.0, (y1 - y0) / 2.0, (d1 - d0) / 2.0), yaw);
    let pieces = (len / 2.0).ceil() as usize;
    for k in 0..pieces {
        let t0 = k as f32 / pieces as f32 * len;
        let t1 = (k + 1) as f32 / pieces as f32 * len;
        let corners = [a + dir * t0 + nrm * d0, a + dir * t1 + nrm * d0, a + dir * t0 + nrm * d1, a + dir * t1 + nrm * d1];
        let mn = corners.iter().fold(Vec2::splat(f32::MAX), |m, p| m.min(*p));
        let mx = corners.iter().fold(Vec2::splat(f32::MIN), |m, p| m.max(*p));
        // Чуть сжимаем по диагонали, чтобы «ступеньки» не торчали за кладку.
        let shrink = ((mx - mn) - Vec2::splat((t1 - t0).min(d1 - d0))).max(Vec2::ZERO) * 0.15;
        b.invisible.push(Aabb::new(Vec3::new(mn.x + shrink.x, y0, mn.y + shrink.y), Vec3::new(mx.x - shrink.x, y1, mx.y - shrink.y)));
    }
}

/// Зубцы по наружному краю и пояс-карниз.
pub(super) fn battlements(b: &mut Builder, a: Vec2, c: Vec2, top: f32, half_t: f32) {
    let dir = (c - a).normalize();
    let nrm = Vec2::new(dir.y, -dir.x);
    let len = a.distance(c);
    let yaw = dir.y.atan2(dir.x);
    // Парапет-коллизия.
    oriented_solid_invisible(b, a, c, half_t - 0.6, half_t, top, top + 1.5);
    let n = (len / 2.4).floor() as usize;
    for k in 0..n {
        let t = (k as f32 + 0.5) / n as f32 * len;
        let p = a + dir * t + nrm * (half_t - 0.3);
        b.mesh(Mat::Limestone).obox(Vec3::new(p.x, top + 0.75, p.y), Vec3::new(0.65, 0.75, 0.3), yaw);
    }
    // Низкий бруствер между зубцами.
    let p = (a + c) / 2.0 + nrm * (half_t - 0.3);
    b.mesh(Mat::Limestone).obox(Vec3::new(p.x, top + 0.35, p.y), Vec3::new(len / 2.0, 0.35, 0.3), yaw);
    let p = (a + c) / 2.0 + nrm * (half_t + 0.05);
    b.mesh(Mat::Limestone).obox(Vec3::new(p.x, top - 0.25, p.y), Vec3::new(len / 2.0, 0.18, 0.12), yaw);
}

fn oriented_solid_invisible(b: &mut Builder, a: Vec2, c: Vec2, d0: f32, d1: f32, y0: f32, y1: f32) {
    let mut tmp = Builder::new();
    oriented_solid(&mut tmp, a, c, d0, d1, y0, y1, Mat::Limestone);
    b.invisible.extend(tmp.invisible);
}

fn diagonal_curtain(b: &mut Builder, a: Vec2, c: Vec2) {
    let h = WALL_T / 2.0;
    // Башни через ~45 м; у каждого прясла между ними и у каждой башни свой вариант кладки.
    let len = a.distance(c);
    let n = (len / 45.0).round() as usize;
    let dir = (c - a).normalize();
    let out = outward(a, c);
    let towers: Vec<f32> = (1..n).map(|k| len * k as f32 / n as f32).collect();
    let v0 = b.variant as usize;
    for (k, (t0, t1)) in spans(0.0, len, &towers).into_iter().enumerate() {
        b.vary(v0 + k * 2);
        let (pa, pc) = (a + dir * t0, a + dir * t1);
        oriented_solid(b, pa, pc, -h, h, 0.0, WALL_H, Mat::Limestone);
        battlements(b, pa, pc, WALL_H, h);
    }
    for (k, t) in towers.into_iter().enumerate() {
        b.vary(v0 + k * 2 + 1);
        rect_tower(b, a + dir * t, dir, out);
    }
}

/// Прямоугольная башня, выступающая наружу.
pub(super) fn rect_tower(b: &mut Builder, p: Vec2, _dir: Vec2, out: Vec2) {
    // Вдоль стены — так, чтобы нормаль `oriented_solid` смотрела наружу.
    let dir = Vec2::new(-out.y, out.x);
    let a = p - dir * TOWER_ALONG / 2.0;
    let c = p + dir * TOWER_ALONG / 2.0;
    oriented_solid(b, a, c, -WALL_T / 2.0, WALL_T / 2.0 + TOWER_OUT, 0.0, TOWER_H, Mat::Limestone);
    let yaw = dir.y.atan2(dir.x);
    let center = p + out * (TOWER_OUT / 2.0 + WALL_T / 4.0);
    let half_d = (TOWER_OUT + WALL_T) / 2.0;
    // Карниз и зубцы по трём наружным сторонам.
    b.mesh(Mat::Limestone).obox(Vec3::new(center.x, TOWER_H - 0.25, center.y), Vec3::new(TOWER_ALONG / 2.0 + 0.15, 0.2, half_d + 0.15), yaw);
    for k in 0..4 {
        let t = (k as f32 + 0.5) / 4.0 - 0.5;
        let q = p + dir * t * TOWER_ALONG + out * (WALL_T / 2.0 + TOWER_OUT - 0.3);
        b.mesh(Mat::Limestone).obox(Vec3::new(q.x, TOWER_H + 0.7, q.y), Vec3::new(0.7, 0.7, 0.3), yaw);
    }
    for s in [-1.0f32, 1.0] {
        for k in 0..2 {
            let q = p + dir * s * (TOWER_ALONG / 2.0 - 0.3) + out * (WALL_T / 2.0 + 1.0 + k as f32 * 2.6);
            b.mesh(Mat::Limestone).obox(Vec3::new(q.x, TOWER_H + 0.7, q.y), Vec3::new(0.3, 0.7, 0.7), yaw);
        }
    }
    // Бойницы — тёмные щели.
    for y in [4.0, 7.5] {
        let q = p + out * (WALL_T / 2.0 + TOWER_OUT + 0.01);
        b.mesh(Mat::Coals).obox(Vec3::new(q.x, y, q.y), Vec3::new(0.12, 0.7, 0.02), yaw);
    }
}

pub(super) fn round_tower(b: &mut Builder, p: Vec2, r: f32, h: f32) {
    let base = Vec3::new(p.x, 0.0, p.y);
    b.mesh(Mat::Limestone).lathe(base, &[(r + 0.4, 0.0), (r + 0.2, 1.2), (r, 1.4), (r, h - 0.4), (r + 0.2, h - 0.3), (r + 0.2, h), (0.0, h)], 20);
    for k in 0..10 {
        let a = k as f32 / 10.0 * TAU;
        let q = base + Vec3::new(a.cos() * (r - 0.1), h + 0.7, a.sin() * (r - 0.1));
        b.mesh(Mat::Limestone).obox(q, Vec3::new(0.3, 0.7, 0.75), a + std::f32::consts::FRAC_PI_2);
    }
    // Коллизия: крест из двух коробок.
    let k = r * 0.86;
    let w = r * 0.5;
    b.invisible.push(Aabb::new(base - Vec3::new(k, 0.0, w), base + Vec3::new(k, h, w)));
    b.invisible.push(Aabb::new(base - Vec3::new(w, 0.0, k), base + Vec3::new(w, h, k)));
}

/// Башня Зенона: при Зеноне облицована вторым кольцом кладки и стала мощнейшей башней Таврики.
fn zeno_tower(b: &mut Builder, p: Vec2) {
    let r = 9.6;
    let h = 13.0;
    round_tower(b, p, r, h);
    // Шов между старым ядром и облицовкой Зенона и надпись (плита) с внутренней стороны.
    let base = Vec3::new(p.x, 0.0, p.y);
    b.mesh(Mat::Limestone).lathe(base, &[(r + 0.05, 6.0), (r + 0.05, 6.25)], 24);
    let plate = base + Vec3::new(-r * 0.71 - 0.05, 3.2, -r * 0.71 - 0.05);
    b.mesh(Mat::Marble).obox(plate, Vec3::new(0.9, 0.55, 0.08), std::f32::consts::FRAC_PI_4);
    b.torch(plate.x - 1.2, 3.4, plate.z - 0.2, Vec3::new(-0.7, 0.0, -0.7).normalize());
}

/// Прямая стена вдоль X или Z с воротами и башнями.
fn straight_wall(b: &mut Builder, a: Vec2, c: Vec2, gates: &[f32], towers: &[f32]) {
    let along_x = (c.y - a.y).abs() < 0.01;
    let out = outward(a, c);
    let (s0, s1) = if along_x { (a.x.min(c.x), a.x.max(c.x)) } else { (a.y.min(c.y), a.y.max(c.y)) };
    let line = if along_x { a.y } else { a.x };
    let f = if along_x { kit::fx() } else { kit::fz() };
    let ops: Vec<Op> = gates.iter().map(|&s| Op::Arch { s, w: GATE_W, spring: 3.6 }).collect();
    let o = if along_x { out.y } else { out.x };
    let p2 = |s: f32| if along_x { Vec2::new(s, line) } else { Vec2::new(line, s) };
    // Середины всех башен на этой стене: простых и воротных.
    let gate_tower = |g: f32, s: f32| g + s * (GATE_W / 2.0 + TOWER_ALONG / 2.0 + 0.3);
    let mut all_towers: Vec<f32> = towers.to_vec();
    all_towers.extend(gates.iter().flat_map(|&g| [gate_tower(g, -1.0), gate_tower(g, 1.0)]));
    let v0 = b.variant as usize;
    // Прясла между башнями — каждое своим вариантом кладки; зубцы — над пряслом, кроме ворот.
    for (k, (t0, t1)) in spans(s0, s1, &all_towers).into_iter().enumerate() {
        b.vary(v0 + k * 2);
        kit::wall(b, f, t0, t1, line - WALL_T / 2.0, line + WALL_T / 2.0, 0.0, WALL_H, &ops, Mat::Limestone, Mat::Limestone);
        let mut cuts: Vec<(f32, f32)> = gates.iter().filter(|&&g| g > t0 && g < t1).map(|&g| (g - GATE_W / 2.0 - 0.2, g + GATE_W / 2.0 + 0.2)).collect();
        cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut cur = t0;
        for (l, r) in cuts.iter().copied().chain(std::iter::once((t1, t1))) {
            if l > cur + 1.0 {
                let (pa, pc) = (p2(cur), p2(l));
                // Порядок концов такой, чтобы «наружу» указывало туда же, куда `out`.
                let (pa, pc) = if outward(pa, pc).dot(out) > 0.0 { (pa, pc) } else { (pc, pa) };
                battlements(b, pa, pc, WALL_H, WALL_T / 2.0);
            }
            cur = r;
        }
    }
    // Над воротами — площадка с зубцами и ворота: створки открыты внутрь.
    for &g in gates {
        let d_in = -o;
        for s in [-1.0f32, 1.0] {
            let hinge = p2(g + s * (GATE_W / 2.0 - 0.1));
            let hinge = Vec3::new(hinge.x, 0.0, hinge.y) + Vec3::new(out.x, 0.0, out.y) * (-WALL_T / 2.0 + 0.1);
            let leaf_dir = if along_x { Vec3::new(0.0, 0.0, d_in) } else { Vec3::new(d_in, 0.0, 0.0) };
            let leaf_side = if along_x { Vec3::new(s, 0.0, 0.0) } else { Vec3::new(0.0, 0.0, s) };
            let c = hinge + leaf_dir * 1.1 - leaf_side * 0.06 + Vec3::Y * 2.3;
            let yaw = if along_x { std::f32::consts::FRAC_PI_2 } else { 0.0 };
            b.mesh(Mat::Wood).obox(c, Vec3::new(1.1, 2.3, 0.07), yaw);
            for y in [0.6, 2.3, 4.0] {
                b.mesh(Mat::Iron).obox(c + Vec3::new(0.0, y - 2.3, 0.0) - leaf_side * 0.02, Vec3::new(1.05, 0.06, 0.08), yaw);
            }
        }
        // Факелы снаружи и внутри.
        for side in [1.0f32, -1.0] {
            let p = p2(g + 3.2) + out * side * (WALL_T / 2.0 + 0.3);
            b.torch(p.x, 3.4, p.y, Vec3::new(out.x, 0.0, out.y) * side);
        }
        // Воротные башни.
        for s in [-1.0f32, 1.0] {
            let p = p2(gate_tower(g, s));
            let dir = if along_x { Vec2::X } else { Vec2::Y };
            b.vary(v0 + 4 + (s > 0.0) as usize);
            rect_tower(b, p, dir, out);
        }
    }
    for (k, &t) in towers.iter().enumerate() {
        let dir = if along_x { Vec2::X } else { Vec2::Y };
        b.vary(v0 + k * 2 + 1);
        rect_tower(b, p2(t), dir, out);
    }
    b.variant = v0 as u8;
}

fn east_wall(b: &mut Builder, a: Vec2, c: Vec2) {
    let towers = [-95.0, -50.0, 45.0, 90.0, 130.0];
    // Ворот тут нет: под стеной обрыв к бухте. В город входят только с суши, с юга.
    straight_wall(b, a, c, &[], &towers);
}

fn south_wall(b: &mut Builder, a: Vec2, c: Vec2) {
    let gx = main_gate_x();
    let sx = south_gate_x();
    let mut towers = Vec::new();
    let mut x = -135.0;
    while x < 270.0 {
        if (x - gx).abs() > 16.0 && (x - sx).abs() > 16.0 {
            towers.push(x);
        }
        x += 42.0;
    }
    straight_wall(b, a, c, &[gx, sx], &towers);
}

/// Протейхизма — внешняя стена перед южной стороной; между стенами — перибол.
fn proteichisma(b: &mut Builder) {
    let z = PROTEICHISMA_Z;
    let gates = [main_gate_x(), south_gate_x()];
    b.at(0.0, z);
    let ops: Vec<Op> = gates.iter().map(|&s| Op::Arch { s, w: GATE_W, spring: 2.6 }).collect();
    kit::wall(b, kit::fx(), PROT_X.0, PROT_X.1, z - PROT_T / 2.0, z + PROT_T / 2.0, 0.0, PROT_H, &ops, Mat::Limestone, Mat::Limestone);
    let mut cur = PROT_X.0;
    for &g in &gates {
        battlements(b, Vec2::new(g - GATE_W / 2.0 - 0.3, z), Vec2::new(cur, z), PROT_H, PROT_T / 2.0);
        cur = g + GATE_W / 2.0 + 0.3;
    }
    battlements(b, Vec2::new(PROT_X.1, z), Vec2::new(cur, z), PROT_H, PROT_T / 2.0);
    // Поперечные стенки, замыкающие перибол.
    for x in [PROT_X.0, PROT_X.1] {
        b.at(x, z);
        b.solid([x - PROT_T / 2.0, 0.0, SOUTH_WALL_Z + WALL_T / 2.0], [x + PROT_T / 2.0, PROT_H, z - PROT_T / 2.0], Mat::Limestone);
    }
}

fn stairs_to_walls(b: &mut Builder) {
    let (s, _) = gate_stairs_start();
    b.at(s.x, s.z);
    kit::stairs(b, Vec3::new(s.x - 0.3 + 0.3, 0.0, s.z), Vec3::X, 1.6, WALL_H, Mat::Limestone);
    // У восточной стены, над бухтой.
    let x = EAST_WALL_X - WALL_T / 2.0 - 0.8;
    b.at(x, 12.0);
    kit::stairs(b, Vec3::new(x, 0.0, 12.0), Vec3::Z, 1.6, WALL_H, Mat::Limestone);
    // У северной стены, за стекольной мастерской.
    let r = block(11, -3);
    // Внутренняя грань северной стены здесь ≈ −163,6 (ось кладки идёт от (0, −166) к (120, −164)).
    let z = -162.75;
    b.at(r.cx(), z);
    kit::stairs(b, Vec3::new(r.x0, 0.0, z), Vec3::X, 1.6, WALL_H, Mat::Limestone);
}
