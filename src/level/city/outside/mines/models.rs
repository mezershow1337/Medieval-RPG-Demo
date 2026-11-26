//! Модели рудника на рудной горке: штольня в скале с рудной жилой, крепь из круглого леса,
//! устье с воротником, отвесный слоистый уступ, куски руды и породы, шурф с воротом, землянка.
//!
//! Всё — своя процедурная геометрия: скала — сетка, смещённая объёмным шумом (с плавными
//! нормалями, фото скалы ложится трипланарно), брёвна — кривоватые сбежистые цилиндры с
//! развёрткой вдоль ствола (`Mat::Timber`: волокна идут вдоль бревна, на торцах — кольца).

use super::*;
use std::f32::consts::PI;
use terrain::smoothstep;
use crate::level::DoorSpec;

/// Полуширина выработки у пола и высота свода (в свету, до шума).
pub(super) const ADIT_W: f32 = 1.3;
pub(super) const ADIT_RISE: f32 = 2.6;
/// Насколько устье выступает из уступа (первое кольцо скалы).
const FRONT: f32 = 0.45;

// ---------------------------------------------------------------------------
// Шум и простые тела
// ---------------------------------------------------------------------------

/// Объёмный шум ~0..1 (среднее ~0.47) из двух плоских fbm.
pub(super) fn noise3(p: Vec3, seed: u32) -> f32 {
    (terrain::fbm(p.x + p.y * 0.37, p.z - p.y * 0.61, seed) + terrain::fbm(p.y * 1.13 + p.z * 0.31, p.x * 0.71 - p.z * 0.23, seed + 7)) * 0.5
}

/// Бревно от `a` до `c` (радиус у комля `r0`, у вершины `r1`): чуть кривое, неровное, с торцами.
/// Развёртка: u — вдоль ствола (м), v — по окружности (м); торцы — u = 1000 + доля радиуса.
pub(super) fn log(b: &mut Builder, a: Vec3, c: Vec3, r0: f32, r1: f32, seed: u32) {
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
    let center = |t: f32| a + d * (len * t) + bow * (PI * t).sin();
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

/// Кусок камня (руды, породы): неровный многогранник, приплюснутый, со сглаженными рёбрами
/// (нормали вершин — между гранью и радиусом: не «кристалл», а окатанный обломок).
pub(super) fn chunk(b: &mut Builder, mat: Mat, c: Vec3, size: f32, rng: &mut Rng) {
    let (seg, rows) = (7usize, 5usize);
    let yaw = rng.range(0.0, TAU);
    let flat = rng.range(0.5, 0.8);
    let stretch = rng.range(0.8, 1.35);
    let scale = Vec3::new(stretch, flat, 1.0);
    let (sy, cy) = yaw.sin_cos();
    let rot = |v: Vec3| Vec3::new(v.x * cy - v.z * sy, v.y, v.x * sy + v.z * cy);
    let mut pts = vec![vec![Vec3::ZERO; seg]; rows + 1];
    let mut nrm = vec![vec![Vec3::Y; seg]; rows + 1];
    for i in 0..=rows {
        let lat = -PI / 2.0 + PI * i as f32 / rows as f32;
        for k in 0..seg {
            let lon = TAU * k as f32 / seg as f32 + if i % 2 == 0 { 0.0 } else { 0.45 };
            // Радиус каждой вершины свой (рубленые грани), полюса срезаны.
            let r = size * rng.range(0.78, 1.1) * if i == 0 || i == rows { 0.45 } else { 1.0 };
            let unit = Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            pts[i][k] = c + rot(unit * scale * r);
            nrm[i][k] = rot(unit / scale).normalize();
        }
    }
    let m = b.mesh(mat);
    for i in 0..rows {
        for k in 0..seg {
            let q = [(i, k), (i + 1, k), (i + 1, (k + 1) % seg), (i, (k + 1) % seg)];
            let face = (pts[q[1].0][q[1].1] - pts[q[0].0][q[0].1]).cross(pts[q[2].0][q[2].1] - pts[q[0].0][q[0].1]).normalize_or_zero();
            let ids: Vec<u32> = q.iter().map(|&(a, cc)| {
                let n = (nrm[a][cc] * 0.55 + face * 0.45).normalize_or(nrm[a][cc]);
                let p = pts[a][cc];
                m.vert(p, n, Vec2::new(p.x + p.z, p.y))
            }).collect();
            // Обход: (вверх по широте) × (по долготе) смотрит наружу — так же, как нормаль грани.
            m.index_tri(ids[0], ids[1], ids[2]);
            m.index_tri(ids[0], ids[2], ids[3]);
        }
    }
}

/// Насыпь кусков: невысокий конус-ложе и куски по нему (крупные — у подножия, мелочь — сверху).
pub(super) fn heap(b: &mut Builder, p: Vec3, r: f32, h: f32, mat: Mat, rng: &mut Rng) {
    // Ложе: тот же материал, неровный конус с плавными нормалями.
    let segs = 14;
    let rings = 4;
    // Под пустой породой — земля с мелочью (гладкая «скала» куполом смотрелась покрывалом).
    let m = b.mesh(if mat == Mat::Bedrock { Mat::Earth } else { mat });
    let mut grid: Vec<Vec<Vec3>> = Vec::new();
    for i in 0..=rings {
        let f = i as f32 / rings as f32;
        let row = (0..segs)
            .map(|k| {
                let a = TAU * k as f32 / segs as f32;
                let rr = r * f * (0.85 + 0.3 * noise3(p + Vec3::new(a.cos(), 0.0, a.sin()) * 2.0, 131));
                let yy = h * (1.0 - f * f) * 0.9 * (0.85 + 0.3 * noise3(p * 1.7 + Vec3::new(a.cos(), f, a.sin()), 137));
                p + Vec3::new(a.cos() * rr, yy - 0.04, a.sin() * rr)
            })
            .collect::<Vec<_>>();
        grid.push(row);
    }
    for i in 0..rings {
        for k in 0..segs {
            let (a0, a1, b0, b1) = (grid[i][k], grid[i][(k + 1) % segs], grid[i + 1][k], grid[i + 1][(k + 1) % segs]);
            let n = (b1 - a0).cross(b0 - a1);
            let n = if n.y < 0.0 { -n } else { n };
            m.quad([a0, b0, b1, a1], n);
        }
    }
    let n = (8.0 + r * r * 18.0).min(if mat == Mat::Bedrock { 140.0 } else { 60.0 }) as usize;
    for k in 0..n {
        let a = rng.range(0.0, TAU);
        let f = rng.range(0.0, 1.0f32).sqrt();
        let top = h * (1.0 - f * f) * 0.9;
        let s = rng.range(0.05, 0.13) * (r / 0.7).clamp(0.8, 1.8) * if k % 6 == 0 { 1.7 } else { 1.0 } * (1.15 - f * 0.4);
        let cc = p + Vec3::new(a.cos() * f * r, top - s * 0.25, a.sin() * f * r);
        chunk(b, mat, cc, s, rng);
    }
}

// ---------------------------------------------------------------------------
// Штольня
// ---------------------------------------------------------------------------

/// Сечение выработки (x от оси, y от пола): от правого края под полом через свод к левому.
fn profile() -> Vec<Vec2> {
    let k = 20;
    let mut v = vec![Vec2::new(ADIT_W + 0.15, -0.3)];
    for i in 0..=k {
        let a = PI * i as f32 / k as f32;
        let c = a.cos();
        v.push(Vec2::new(ADIT_W * c.signum() * c.abs().powf(0.5), ADIT_RISE * a.sin().max(0.0).powf(0.4)));
    }
    v.push(Vec2::new(-ADIT_W - 0.15, -0.3));
    v
}

/// Наружная нормаль сечения в точке `k`.
fn profile_normal(p: &[Vec2], k: usize) -> Vec2 {
    let a = p[k.saturating_sub(1)];
    let c = p[(k + 1).min(p.len() - 1)];
    let t = (c - a).normalize_or(Vec2::X);
    // Контур идёт против часовой (справа через верх налево): наружу — вправо от хода.
    Vec2::new(t.y, -t.x)
}

/// Половина ширины выработки на высоте `h` над полом (без шума).
fn half_width_at(h: f32) -> f32 {
    if h <= 0.0 {
        return ADIT_W;
    }
    if h >= ADIT_RISE {
        return 0.0;
    }
    let s = (h / ADIT_RISE).powf(1.0 / 0.4);
    ADIT_W * (1.0 - s * s).max(0.0).sqrt().sqrt()
}

/// Смещение стены выработки по нормали (м): крупные бугры, мелкая рваность, слои.
fn rock_disp(p: Vec3, seed: u32) -> f32 {
    let big = noise3(p * 0.55, seed) - 0.47;
    let mid = noise3(p * 1.7, seed + 6) - 0.47;
    let fine = noise3(p * 4.3, seed + 12) - 0.47;
    let bed = ((p.y * 2.4 + noise3(p * 0.3, seed + 18) * 7.0).sin()).max(-0.3) * 0.05;
    (big * 0.75 + mid * 0.32 + fine * 0.1 + bed).clamp(-0.15, 0.4)
}

/// Рудная жила: полоса бурого железняка, полого падающая вдоль выработки.
fn in_vein(p: Vec3, floor: f32, fz: f32) -> bool {
    let h = p.y - floor;
    let mid = 1.15 + 0.4 * ((p.z - fz) * 0.31 + 1.0).sin() + (noise3(p * 0.4, 81) - 0.47) * 1.3;
    let thick = 0.3 + (noise3(p * 0.9, 83) - 0.47) * 0.7;
    h > 0.15 && (h - mid).abs() < thick
}

/// Ровная плоскость на сетке точек: нормали по соседям (сглаженные), квадраты в два материала.
fn grid_mesh(b: &mut Builder, pts: &[Vec<Vec3>], inside: impl Fn(Vec3) -> Vec3, pick: impl Fn(Vec3) -> Mat, skip: impl Fn(usize, usize) -> bool) {
    let (nj, nk) = (pts.len(), pts[0].len());
    let normal = |j: usize, k: usize| {
        let du = pts[j][(k + 1).min(nk - 1)] - pts[j][k.saturating_sub(1)];
        let dv = pts[(j + 1).min(nj - 1)][k] - pts[j.saturating_sub(1)][k];
        let n = du.cross(dv).normalize_or_zero();
        let want = inside(pts[j][k]);
        if n.dot(want) < 0.0 { -n } else { n }
    };
    let norms: Vec<Vec<Vec3>> = (0..nj).map(|j| (0..nk).map(|k| normal(j, k)).collect()).collect();
    for j in 0..nj - 1 {
        for k in 0..nk - 1 {
            if skip(j, k) {
                continue;
            }
            let q = [(j, k), (j, k + 1), (j + 1, k + 1), (j + 1, k)];
            // Лицом туда же, куда нормаль.
            let geo = (pts[q[1].0][q[1].1] - pts[q[0].0][q[0].1]).cross(pts[q[2].0][q[2].1] - pts[q[0].0][q[0].1]);
            let nn = norms[q[0].0][q[0].1] + norms[q[2].0][q[2].1];
            let tris = if geo.dot(nn) >= 0.0 { [[0, 1, 2], [0, 2, 3]] } else { [[0, 2, 1], [0, 3, 2]] };
            // Материал — по треугольнику: граница жилы идёт ступенькой по диагоналям, а не клетками.
            for t in tris {
                let c = t.iter().map(|&i| pts[q[i].0][q[i].1]).sum::<Vec3>() / 3.0;
                let m = b.mesh(pick(c));
                let ids: Vec<u32> = t.iter().map(|&i| { let (a, cc) = q[i]; m.vert(pts[a][cc], norms[a][cc], Vec2::new(pts[a][cc].x + pts[a][cc].z, pts[a][cc].y)) }).collect();
                m.index_tri(ids[0], ids[1], ids[2]);
            }
        }
    }
}

/// Скальная выработка: стены и свод одной трубой (без щелей), забой, воротник вокруг устья.
pub(super) fn adit_rock(b: &mut Builder, mx: f32, y: f32, fz: f32, end: f32) {
    let prof = profile();
    let z0 = fz - FRONT;
    let nj = ((end - z0) / 0.3).ceil() as usize;
    let ring = |z: f32| -> Vec<Vec3> {
        let fade = smoothstep(z0, fz + 0.9, z);
        prof.iter()
            .enumerate()
            .map(|(k, q)| {
                let base = Vec3::new(mx + q.x, y + q.y, z);
                let n = profile_normal(&prof, k);
                base + Vec3::new(n.x, n.y, 0.0) * rock_disp(base, 61) * fade
            })
            .collect()
    };
    let rings: Vec<Vec<Vec3>> = (0..=nj).map(|j| ring(z0 + (end - z0) * j as f32 / nj as f32)).collect();
    let axis = |p: Vec3| Vec3::new(mx, y + 1.2, p.z) - p;
    grid_mesh(b, &rings, axis, |c| if in_vein(c, y, fz) { Mat::Limonite } else { Mat::Bedrock }, |_, _| false);

    // Забой: веер от середины к последнему кольцу, вогнутый, рваный, почти весь в руде.
    // Кольцо замкнуто (последняя точка — снова первая), иначе под забоем у пола — щель.
    let mut last = rings.last().unwrap().clone();
    last.push(last[0]);
    let ctr = Vec3::new(mx, y + 1.15, end);
    let nr = 5;
    let fan: Vec<Vec<Vec3>> = (0..=nr)
        .map(|i| {
            let f = i as f32 / nr as f32;
            last.iter()
                .map(|&rim| {
                    let p = ctr.lerp(rim, f);
                    if i == nr {
                        return rim;
                    }
                    let bulge = (0.22 + (noise3(p * 1.4, 91) - 0.47) * 0.7 + (noise3(p * 3.7, 95) - 0.47) * 0.2) * (1.0 - f * f);
                    Vec3::new(p.x, p.y, end + bulge)
                })
                .collect()
        })
        .collect();
    grid_mesh(b, &fan, |_| Vec3::NEG_Z, |c| if in_vein(c, y, fz) || (c.y - y - 1.1).abs() < 0.75 { Mat::Limonite } else { Mat::Bedrock }, |_, _| false);

    // Воротник: от первого кольца наружу, к плоскости уступа — каменная губа вокруг устья.
    let first = rings[0].clone();
    let lip: Vec<Vec<Vec3>> = [0.0f32, 0.4, 0.9]
        .iter()
        .enumerate()
        .map(|(i, off)| {
            first
                .iter()
                .enumerate()
                .map(|(k, p)| {
                    if i == 0 {
                        return *p;
                    }
                    let n = profile_normal(&prof, k);
                    let q = *p + Vec3::new(n.x, n.y, 0.0) * *off;
                    let jag = (noise3(q * 1.3, 97) - 0.47) * 0.35;
                    Vec3::new(q.x + n.x * jag, q.y + n.y * jag, if i == 1 { fz - FRONT * 0.75 } else { face_z(mx, y, fz, q.x, q.y) - 0.04 })
                })
                .collect()
        })
        .collect();
    grid_mesh(b, &lip, |_| Vec3::NEG_Z, |_| Mat::Bedrock, |_, _| false);
}

/// Насколько уступ выдаётся из плоскости `fz` к площадке (м) в точке (x, y): пласты-полки,
/// бугры; у устья, у выходов руды внизу и по краям — почти плоско.
fn face_out(mx: f32, y: f32, fz: f32, x: f32, yy: f32) -> f32 {
    let p = Vec3::new(x, yy, fz);
    let h = yy - y;
    let big = noise3(p * 0.32, 101) - 0.47;
    let mid = noise3(p * 1.1, 103) - 0.47;
    let strata = (h * 1.25 + noise3(p * 0.22, 107) * 3.0).fract();
    let ledge = smoothstep(0.0, 0.12, strata) * (1.0 - smoothstep(0.75, 1.0, strata)) * 0.22;
    let mut d = 0.35 + big * 1.4 + mid * 0.45 + ledge;
    // У устья — ровнее (там воротник и оклад).
    let open = (x - mx).abs() - half_width_at(h.max(0.0)) - 0.9;
    let near_open = if h < ADIT_RISE + 1.2 { smoothstep(0.0, 1.6, open) } else { smoothstep(0.0, 1.6, h - ADIT_RISE - 1.2).max(smoothstep(0.0, 1.6, open)) };
    d *= near_open;
    // Внизу у выходов руды — к ним подходят забойщики.
    if h < 2.2 {
        let vein = 1.0 - smoothstep(1.2, 2.6, ((x - mx).abs() - 6.0).abs());
        d = d.min(0.45 - vein * 0.25);
    }
    d.clamp(-0.1, 1.1)
}

pub(super) fn face_z(mx: f32, y: f32, fz: f32, x: f32, yy: f32) -> f32 {
    fz - face_out(mx, y, fz, x, yy)
}

/// Отвесный уступ: слоистая скала сеткой, с проёмом устья и выходами бурого железняка.
/// `brow(x)` — высота бровки (там уступ сходится с поверхностью горки), `x0..x1` — края выемки.
pub(super) fn face_cliff(b: &mut Builder, mx: f32, y: f32, fz: f32, x0: f32, x1: f32, brow: impl Fn(f32) -> f32) {
    let nx = ((x1 - x0) / 0.45).ceil() as usize;
    let ny = 14;
    let pts: Vec<Vec<Vec3>> = (0..=ny)
        .map(|j| {
            let v = j as f32 / ny as f32;
            (0..=nx)
                .map(|i| {
                    let x = x0 + (x1 - x0) * i as f32 / nx as f32;
                    let top = brow(x);
                    let yy = (y - 0.3) + (top - (y - 0.3)) * v;
                    // К краям и к бровке смещение гаснет: там уступ стыкуется с рельефом.
                    let edge = smoothstep(0.0, 1.8, (x - x0).min(x1 - x)) * (1.0 - smoothstep(0.82, 1.0, v));
                    Vec3::new(x, yy, fz - face_out(mx, y, fz, x, yy) * edge)
                })
                .collect()
        })
        .collect();
    let skip = |j: usize, i: usize| {
        let c = (pts[j][i] + pts[j + 1][i + 1]) / 2.0;
        let h = c.y - y;
        h < ADIT_RISE + 0.45 && (c.x - mx).abs() < half_width_at(h.max(0.0)) + 0.45
    };
    let pick = |c: Vec3| {
        let h = c.y - y;
        let side = (c.x - mx).abs();
        let vein = (side - 6.0).abs() < 1.6 + (noise3(c * 0.8, 111) - 0.47) * 2.0 && h < 3.0 + (noise3(c * 0.5, 113) - 0.47) * 2.0;
        // У самого устья жила уходит в скалу: на уступе — только по бокам (желваки — отдельно).
        if vein { Mat::Limonite } else { Mat::Bedrock }
    };
    grid_mesh(b, &pts, |_| Vec3::NEG_Z, pick, skip);
}

/// Одна рама крепи (дверной оклад): две стойки с наклоном внутрь, верхняк, затяжка сверху.
fn timber_set(b: &mut Builder, mx: f32, y: f32, z: f32, heavy: bool, seed: u32) {
    let mut rng = Rng::new(seed);
    let (rp, rc) = if heavy { (0.13, 0.14) } else { (0.1, 0.11) };
    let cap_y = y + 2.18 + rng.range(-0.03, 0.03);
    let jz = rng.range(-0.04, 0.04);
    for sx in [-1.0f32, 1.0] {
        let foot = Vec3::new(mx + sx * (1.12 + rng.range(-0.03, 0.03)), y - 0.05, z + jz + rng.range(-0.03, 0.03));
        let top = Vec3::new(mx + sx * 0.98, cap_y - rc * 0.8, z + jz);
        log(b, foot, top, rp, rp * 0.9, seed + sx as u32 * 3 + 5);
        // Камень-подкладка под стойкой.
        chunk(b, Mat::Bedrock, foot + Vec3::new(sx * 0.05, 0.02, 0.0), 0.12, &mut rng);
    }
    log(b, Vec3::new(mx - 1.28, cap_y, z + jz), Vec3::new(mx + 1.28, cap_y + rng.range(-0.03, 0.03), z + jz), rc, rc * 0.92, seed + 11);
    // Клинья-распорки между верхняком и сводом.
    for sx in [-0.55f32, 0.6] {
        let p = Vec3::new(mx + sx + rng.range(-0.1, 0.1), cap_y + rc + 0.06, z + jz);
        log(b, p - Vec3::Y * 0.05, p + Vec3::Y * rng.range(0.12, 0.22), 0.05, 0.045, seed + 21 + (sx * 10.0) as u32);
    }
}

/// Крепь штольни: тяжёлый оклад устья, частые рамы и сплошная затяжка в слабой породе у устья,
/// дальше — реже, затяжка жердями только по своду. Последние метры у забоя — без крепи.
pub(super) fn adit_timber(b: &mut Builder, mx: f32, y: f32, fz: f32, end: f32) {
    let mut sets = vec![fz - 0.3, fz + 0.55, fz + 1.4, fz + 2.25];
    let mut z = fz + 3.6;
    while z < end - 1.6 {
        sets.push(z);
        z += 1.35;
    }
    for (i, &sz) in sets.iter().enumerate() {
        timber_set(b, mx, y, sz, i == 0, 4100 + i as u32 * 31);
    }
    let mut rng = Rng::new(4177);
    // Затяжка свода: жерди поверх верхняков, от рамы к раме.
    for w in sets.windows(2) {
        let (za, zb) = (w[0], w[1]);
        let dense = zb < fz + 2.5;
        let n = if dense { 9 } else { 4 };
        for k in 0..n {
            if !dense && rng.chance(0.25) {
                continue;
            }
            let x = mx - 0.95 + 1.9 * (k as f32 + rng.range(0.25, 0.75)) / n as f32;
            let yy = y + 2.18 + 0.14 + 0.055;
            log(b, Vec3::new(x, yy + rng.range(-0.02, 0.02), za - 0.12), Vec3::new(x + rng.range(-0.06, 0.06), yy + rng.range(-0.02, 0.02), zb + 0.12), 0.05, 0.045, 4300 + k as u32 + (za * 10.0) as u32);
        }
        // Бока у устья: горизонтальные жерди за стойками (снаружи уступа — не нужно).
        if dense && za > fz {
            for sx in [-1.0f32, 1.0] {
                for k in 0..4 {
                    let yy = y + 0.35 + k as f32 * 0.5 + rng.range(-0.05, 0.05);
                    // За стойками (они клонятся внутрь, ~6 см на метр).
                    let x = mx + sx * (1.3 - (yy - y).min(2.0) * 0.065);
                    log(b, Vec3::new(x, yy, za - 0.1), Vec3::new(x, yy + rng.range(-0.03, 0.03), zb + 0.1), 0.045, 0.045, 4500 + k as u32 + (za * 10.0) as u32 + (sx * 3.0) as u32);
                }
            }
        }
    }
}

/// Оклад устья снаружи: стойки, верхняк, козырёк из коротких брёвен в склон, бутовая
/// подпорная кладка по бокам и плаха-порог.
pub(super) fn portal(b: &mut Builder, mx: f32, y: f32, fz: f32) {
    let z = fz - FRONT - 0.18;
    let mut rng = Rng::new(4711);
    for sx in [-1.0f32, 1.0] {
        log(b, Vec3::new(mx + sx * 1.32, y - 0.1, z), Vec3::new(mx + sx * 1.2, y + 2.42, z + 0.02), 0.16, 0.14, 4720 + sx as u32);
    }
    let cap = y + 2.56;
    log(b, Vec3::new(mx - 1.85, cap, z), Vec3::new(mx + 1.85, cap + 0.03, z + 0.03), 0.17, 0.15, 4731);
    // Козырёк: ряд брёвен на верхняке, концы уходят в склон над устьем.
    for k in 0..9 {
        let x = mx - 1.6 + 3.2 * k as f32 / 8.0 + rng.range(-0.04, 0.04);
        log(b, Vec3::new(x, cap + 0.2, z - 0.55 - rng.range(0.0, 0.2)), Vec3::new(x + rng.range(-0.05, 0.05), cap + 0.3, fz + 0.9), 0.085, 0.075, 4740 + k);
    }
    // Камни, придавившие козырёк, и навал земли поверх.
    for k in 0..6 {
        let p = Vec3::new(mx + rng.range(-1.4, 1.4), cap + 0.42, z + rng.range(-0.2, 0.6));
        chunk(b, Mat::Bedrock, p, rng.range(0.13, 0.22) + k as f32 * 0.005, &mut rng);
    }
    // Глыбы у подножия по бокам устья: удерживают осыпь, прячут стык оклада со скалой.
    for (dx, sc, v) in [(-1.85f32, 0.75f32, 7u32), (1.9, 0.85, 19), (-2.5, 0.55, 33), (2.6, 0.6, 41)] {
        b.plant(PlantKind::Rock, Vec3::new(mx + dx, y - 0.12, z + 0.25 + rng.range(-0.1, 0.15)), sc, rng.range(0.0, TAU), v);
    }
    // Порог-плаха поперёк устья.
    log(b, Vec3::new(mx - 1.15, y + 0.06, z + 0.25), Vec3::new(mx + 1.15, y + 0.07, z + 0.27), 0.09, 0.09, 4790);
}

/// Пол штольни: каменная мелочь у стен, лужица у забоя.
pub(super) fn adit_floor(b: &mut Builder, mx: f32, y: f32, fz: f32, end: f32) {
    let mut rng = Rng::new(4801);
    let mut z = fz;
    while z < end - 0.3 {
        for sx in [-1.0f32, 1.0] {
            if rng.chance(0.7) {
                let p = Vec3::new(mx + sx * rng.range(0.85, 1.15), y + 0.02, z + rng.range(-0.2, 0.2));
                chunk(b, if rng.chance(0.3) { Mat::Limonite } else { Mat::Bedrock }, p, rng.range(0.05, 0.13), &mut rng);
            }
        }
        z += rng.range(0.35, 0.7);
    }
    // Мелочь по середине, втоптанная.
    for _ in 0..30 {
        let p = Vec3::new(mx + rng.range(-0.8, 0.8), y + 0.005, rng.range(fz, end - 0.4));
        chunk(b, Mat::Bedrock, p, rng.range(0.025, 0.05), &mut rng);
    }
}

/// Глиняный светильник на каменном уступе в стене: камень-полка и плошка с фитилём.
pub(super) fn wall_lamp(b: &mut Builder, p: Vec3, out: Vec3, rng: &mut Rng) {
    chunk(b, Mat::Bedrock, p - Vec3::Y * 0.08 - out * 0.05, 0.16, rng);
    b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.045, 0.0), (0.07, 0.025), (0.065, 0.045), (0.035, 0.05), (0.0, 0.035)], 10);
    // Носик.
    b.mesh(Mat::Ceramic).beam(p + out * 0.05 + Vec3::Y * 0.025, p + out * 0.1 + Vec3::Y * 0.03, 0.018, 0.012, Vec3::Y);
}

// ---------------------------------------------------------------------------
// Инструмент и утварь
// ---------------------------------------------------------------------------

/// Кайло: рукоять из круглого черенка, железная голова с острым и тупым концом.
pub(super) fn pick(b: &mut Builder, foot: Vec3, top: Vec3, side: Vec3) {
    let d = (top - foot).normalize();
    log(b, foot, top + d * 0.03, 0.019, 0.017, (foot.x * 31.0 + foot.z * 7.0) as i32 as u32);
    let head = top - d * 0.06;
    let s = (side - d * side.dot(d)).normalize_or(Vec3::X);
    let m = b.mesh(Mat::Iron);
    // Проух вокруг рукояти.
    m.beam(head - d * 0.045, head + d * 0.045, 0.032, 0.032, s);
    // Два рога, сужаются к концам и чуть загнуты вниз (к рукояти).
    let horns = [(1.0f32, 0.26f32, 0.012f32), (-1.0, 0.2, 0.022)];
    for (sg, len, tip) in horns {
        let pts: Vec<Vec3> = (0..=4).map(|k| {
            let t = k as f32 / 4.0;
            head + s * sg * len * t - d * (t * t * 0.06)
        }).collect();
        for k in 0..4 {
            let t = k as f32 / 4.0;
            let w = 0.028 + (tip - 0.028) * t;
            m.beam(pts[k], pts[k + 1], w, w * 0.85, d);
        }
    }
}

/// Заступ: черенок, деревянная лопасть, оковка-железко по нижнему краю.
pub(super) fn spade(b: &mut Builder, foot: Vec3, top: Vec3, side: Vec3) {
    let d = (top - foot).normalize();
    let s = (side - d * side.dot(d)).normalize_or(Vec3::X);
    let face = s.cross(d).normalize_or(Vec3::Z);
    log(b, foot + d * 0.36, top, 0.018, 0.017, (foot.x * 13.0 + foot.z * 3.0) as i32 as u32);
    // Поперечная ручка-костыль.
    log(b, top - s * 0.09, top + s * 0.09, 0.016, 0.016, (foot.z * 13.0) as i32 as u32);
    // Лопасть: чуть сужается книзу и закруглена.
    let m = b.mesh(Mat::Wood);
    let rows = 5;
    let pts: Vec<(Vec3, f32)> = (0..=rows).map(|k| {
        let t = k as f32 / rows as f32;
        (foot + d * (0.06 + t * 0.32), 0.095 + 0.03 * (t * PI * 0.8).sin() - (1.0 - t).powi(3) * 0.03)
    }).collect();
    for k in 0..rows {
        let ((a, wa), (c, wc)) = (pts[k], pts[k + 1]);
        let th = 0.012;
        m.quad([a - s * wa + face * th, a + s * wa + face * th, c + s * wc + face * th, c - s * wc + face * th], face);
        m.quad([a - s * wa - face * th, a + s * wa - face * th, c + s * wc - face * th, c - s * wc - face * th], -face);
        m.quad([a - s * wa + face * th, a - s * wa - face * th, c - s * wc - face * th, c - s * wc + face * th], -s);
        m.quad([a + s * wa + face * th, a + s * wa - face * th, c + s * wc - face * th, c + s * wc + face * th], s);
    }
    let (a, wa) = pts[rows];
    m.quad([a - s * wa + face * 0.012, a + s * wa + face * 0.012, a + s * wa - face * 0.012, a - s * wa - face * 0.012], d);
    // Железко: U-образная оковка по низу лопасти.
    let w0 = pts[0].1;
    let base = pts[0].0;
    let m = b.mesh(Mat::Iron);
    m.beam(base - s * (w0 + 0.005) - d * 0.03, base + s * (w0 + 0.005) - d * 0.03, 0.05, 0.016, face);
    for sg in [-1.0f32, 1.0] {
        m.beam(base + s * sg * (w0 + 0.003) - d * 0.06, base + s * sg * (w0 + 0.01) + d * 0.1, 0.012, 0.016, face);
    }
}

/// Бадья: клёпки по кругу, два железных обруча, дужка.
pub(super) fn bucket(b: &mut Builder, p: Vec3, r: f32, h: f32, fill: Option<Mat>) {
    let n = 14;
    for k in 0..n {
        let a0 = TAU * k as f32 / n as f32;
        let a1 = TAU * (k as f32 + 0.93) / n as f32;
        let bot = |a: f32| p + Vec3::new(a.cos() * r * 0.86, 0.0, a.sin() * r * 0.86);
        let top = |a: f32| p + Vec3::new(a.cos() * r, h, a.sin() * r);
        let am = (a0 + a1) / 2.0;
        let out = Vec3::new(am.cos(), 0.12, am.sin());
        b.mesh(Mat::Wood).quad([bot(a0), bot(a1), top(a1), top(a0)], out);
        b.mesh(Mat::Wood).quad([bot(a0), bot(a1), top(a1), top(a0)], -out);
    }
    b.mesh(Mat::Wood).disc(p + Vec3::Y * 0.04, r * 0.88, n, true);
    for t in [0.2f32, 0.78] {
        let rr = r * (0.86 + 0.14 * t) + 0.006;
        b.mesh(Mat::Iron).lathe(p + Vec3::Y * (h * t - 0.025), &[(rr, 0.0), (rr + 0.004, 0.025), (rr, 0.05)], n);
    }
    if let Some(m) = fill {
        b.mesh(m).lathe(p + Vec3::Y * (h * 0.82), &[(r * 0.97, 0.0), (r * 0.6, h * 0.18), (0.0, h * 0.24)], 10);
    }
    // Дужка.
    let pts: Vec<Vec3> = (0..=8).map(|k| {
        let a = PI * k as f32 / 8.0;
        p + Vec3::new(a.cos() * (r + 0.01), h + a.sin() * r * 0.9, 0.0)
    }).collect();
    b.mesh(Mat::Iron).beam_path(&pts, 0.007, 0.007, Vec3::Z);
    kit::hidden(b, p - Vec3::new(r, 0.0, r), p + Vec3::new(r, h, r));
}

/// Корыто-лоток для руды: выдолблено из половины бревна.
pub(super) fn trough(b: &mut Builder, a: Vec3, c: Vec3, r: f32, fill: Option<Mat>, rng: &mut Rng) {
    let d = (c - a).normalize();
    let s = d.cross(Vec3::Y).normalize();
    let len = a.distance(c);
    let segs = 8;
    let m = b.mesh(Mat::Timber);
    // Наружная полутруба и внутренняя выемка.
    for (rad, inward) in [(r, false), (r * 0.78, true)] {
        let ids: Vec<(u32, u32)> = (0..=segs).map(|k| {
            let ang = PI * k as f32 / segs as f32;
            let dir = s * ang.cos() - Vec3::Y * ang.sin();
            let n = if inward { -dir } else { dir };
            let off = dir * rad + Vec3::Y * r;
            (m.vert(a + off, n, Vec2::new(0.0, ang * rad)), m.vert(c + off, n, Vec2::new(len, ang * rad)))
        }).collect();
        for k in 0..segs {
            let ((a0, b0), (a1, b1)) = (ids[k], ids[k + 1]);
            if inward {
                m.index_tri(a0, a1, b1);
                m.index_tri(a0, b1, b0);
            } else {
                m.index_tri(a0, b0, b1);
                m.index_tri(a0, b1, a1);
            }
        }
    }
    // Верхние кромки и торцы.
    for sg in [-1.0f32, 1.0] {
        let e = s * sg;
        let (o0, o1) = (e * r + Vec3::Y * r, e * r * 0.78 + Vec3::Y * r);
        m.quad([a + o0, c + o0, c + o1, a + o1], Vec3::Y);
    }
    for (p, out) in [(a, -d), (c, d)] {
        let pts: Vec<Vec3> = (0..=segs).map(|k| {
            let ang = PI * k as f32 / segs as f32;
            p + (s * ang.cos() - Vec3::Y * ang.sin()) * r + Vec3::Y * r
        }).collect();
        let ctr = p + Vec3::Y * r;
        for w in pts.windows(2) {
            m.tri(ctr, w[0], w[1], out);
        }
    }
    if let Some(fm) = fill {
        for _ in 0..(len * 14.0) as usize {
            let t = rng.range(0.08, 0.92);
            let q = a.lerp(c, t) + s * rng.range(-0.5, 0.5) * r + Vec3::Y * (r * 0.95);
            chunk(b, fm, q, rng.range(0.04, 0.075), rng);
        }
    }
    kit::hidden(b, a.min(c) - Vec3::new(r, 0.0, r), a.max(c) + Vec3::new(r, r * 1.6, r));
}

/// Гладкий цилиндр (верёвка, обруч, вал): без шума, с торцами.
fn rod(b: &mut Builder, mat: Mat, a: Vec3, c: Vec3, r: f32, sides: usize) {
    let len = a.distance(c);
    if len < 1e-3 {
        return;
    }
    let d = (c - a) / len;
    let up = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let s = d.cross(up).normalize();
    let w = s.cross(d);
    let m = b.mesh(mat);
    let ids: Vec<(u32, u32)> = (0..=sides)
        .map(|k| {
            let ang = TAU * k as f32 / sides as f32;
            let dir = s * ang.cos() + w * ang.sin();
            let v = ang * r;
            (m.vert(a + dir * r, dir, Vec2::new(0.0, v)), m.vert(c + dir * r, dir, Vec2::new(len, v)))
        })
        .collect();
    for k in 0..sides {
        let ((a0, b0), (a1, b1)) = (ids[k], ids[k + 1]);
        m.index_tri(a0, b0, b1);
        m.index_tri(a0, b1, a1);
    }
}

/// Нормаль рельефа в точке — как у мешей земли: в узлах сетки 6 м по соседним узлам,
/// между узлами — билинейно.
fn terrain_normal(x: f32, z: f32) -> Vec3 {
    let c = terrain::CELL;
    let node = |ix: f32, iz: f32| {
        let (nx, nz) = (ix * c, iz * c);
        Vec3::new(ground_y(nx - c, nz) - ground_y(nx + c, nz), 2.0 * c, ground_y(nx, nz - c) - ground_y(nx, nz + c)).normalize()
    };
    let (fx, fz) = (x / c, z / c);
    let (ix, iz) = (fx.floor(), fz.floor());
    let (tx, tz) = (fx - ix, fz - iz);
    let a = node(ix, iz).lerp(node(ix + 1.0, iz), tx);
    let b = node(ix, iz + 1.0).lerp(node(ix + 1.0, iz + 1.0), tx);
    a.lerp(b, tz).normalize()
}

/// Своя земля в вырезе рельефа `r` (кратном клетке 6 м): сетка с шагом `step` по высоте
/// `height(x, z)`, нормали — по наклону (как у рельефа, без граней), материал по точке,
/// клетки, где `skip`, пропускаются. Твердь — невидимыми столбами с шагом `col` (или пол
/// одной коробкой там, где `flat` даёт его высоту).
fn ground_patch(b: &mut Builder, r: Rect, step: f32, col: f32, height: impl Fn(f32, f32) -> f32, mat: impl Fn(Vec3) -> Mat, skip: impl Fn(Vec3) -> bool, flat: impl Fn(Vec3) -> bool) {
    b.holes.push((Vec2::new(r.x0, r.z0), Vec2::new(r.x1, r.z1)));
    b.at(r.cx(), r.cz());
    let (nx, nz) = ((r.w() / step).round() as usize, (r.d() / step).round() as usize);
    let at = |i: usize, j: usize| {
        let (x, z) = (r.x0 + r.w() * i as f32 / nx as f32, r.z0 + r.d() * j as f32 / nz as f32);
        Vec3::new(x, height(x, z), z)
    };
    let e = step * 0.5;
    // Там, где своя земля совпадает с рельефом, нормаль — как у рельефа (сглаженная по узлам
    // сетки 6 м), иначе на стыке с картой высот видна разница в освещении.
    let normal = |p: Vec3| {
        let own = Vec3::new(height(p.x - e, p.z) - height(p.x + e, p.z), 2.0 * e, height(p.x, p.z - e) - height(p.x, p.z + e)).normalize();
        let off = [(0.0, 0.0), (e, 0.0), (-e, 0.0), (0.0, e), (0.0, -e)].iter().map(|(dx, dz)| (height(p.x + dx, p.z + dz) - ground_y(p.x + dx, p.z + dz)).abs()).fold(0.0f32, f32::max);
        let k = smoothstep(0.01, 0.12, off);
        terrain_normal(p.x, p.z).lerp(own, k).normalize()
    };
    for i in 0..nx {
        for j in 0..nz {
            let q = [at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)];
            let c = (q[0] + q[2]) / 2.0;
            if skip(c) {
                continue;
            }
            // Материал — по треугольнику: граница двух земель идёт по диагоналям, не клетками.
            for t in [[0usize, 1, 2], [0, 2, 3]] {
                let m = b.mesh(mat((q[t[0]] + q[t[1]] + q[t[2]]) / 3.0));
                let ids: Vec<u32> = t.iter().map(|&i| m.vert(q[i], normal(q[i]), Vec2::new(q[i].x, q[i].z))).collect();
                m.index_tri(ids[0], ids[1], ids[2]);
            }
        }
    }
    let (cx, cz) = ((r.w() / col).round() as usize, (r.d() / col).round() as usize);
    for i in 0..cx {
        for j in 0..cz {
            let (xa, xb) = (r.x0 + r.w() * i as f32 / cx as f32, r.x0 + r.w() * (i + 1) as f32 / cx as f32);
            let (za, zb) = (r.z0 + r.d() * j as f32 / cz as f32, r.z0 + r.d() * (j + 1) as f32 / cz as f32);
            let c = Vec3::new((xa + xb) / 2.0, 0.0, (za + zb) / 2.0);
            if skip(c) || flat(c) {
                continue;
            }
            let hs = [height(xa, za), height(xa, zb), height(xb, za), height(xb, zb), height(c.x, c.z)];
            let top = hs.iter().copied().fold(f32::MIN, f32::max);
            let bot = hs.iter().copied().fold(f32::MAX, f32::min);
            // Верх столба — чуть ниже самой высокой точки: по склону не «спотыкаемся».
            b.invisible.push(Aabb::new(Vec3::new(xa, bot - 2.0, za), Vec3::new(xb, (top + hs[4]) * 0.5, zb)));
        }
    }
}

// ---------------------------------------------------------------------------
// Шурф
// ---------------------------------------------------------------------------

/// Шурф: земля вокруг (своя сетка — в рельефе здесь дыра на клетку 6×6 м), колодец в скале,
/// сруб-оклад из круглого леса, ворот на рогатках с валом, верёвкой и рукоятью, бадьи.
/// Ширина откоса, которым пол площадки сходится с рельефом у переднего края выреза.
const YARD_FADE: f32 = 2.4;

/// Пол площадки перед уступом: утоптанная земля с едва заметными буграми, у переднего края
/// выреза — пологим откосом в рельеф (без ступеньки и прямой кромки).
pub(super) fn yard_y(x: f32, z: f32) -> f32 {
    let (_, fz) = MINE_FACE;
    let hole = mine_hole();
    let y = mine_floor();
    // К краю — ровным откосом к высоте рельефа на самой кромке (внутри выреза рельеф
    // горбится к уступу, по нему вышел бы бугор).
    let edge = ground_y(x, hole.z0);
    let bump = (noise3(Vec3::new(x, 0.0, z) * 0.55, 171) - 0.47) * 0.07 * smoothstep(0.2, 1.2, fz - z);
    let t = smoothstep(0.0, YARD_FADE, z - hole.z0);
    edge + (y + bump - edge) * t
}

/// Своя земля площадки (клетки выреза перед уступом) и её твердь.
pub(super) fn yard(b: &mut Builder) {
    let (_, fz) = MINE_FACE;
    let hole = mine_hole();
    let y = mine_floor();
    let r = Rect::new(hole.x0, hole.z0, hole.x1, fz);
    // У кромки — дёрн рваной полосой, дальше — вытоптанная земля.
    let mat = move |p: Vec3| {
        let ragged = (noise3(Vec3::new(p.x, 0.0, p.z) * 0.9, 173) - 0.47) * 1.4;
        if p.z - hole.z0 + ragged < 0.45 { Mat::Terrain } else { Mat::Earth }
    };
    let flat = move |p: Vec3| p.z > hole.z0 + YARD_FADE + 0.1;
    ground_patch(b, r, 0.5, 0.75, yard_y, mat, |_| false, flat);
    b.invisible.push(Aabb::new(Vec3::new(r.x0, y - 1.0, hole.z0 + YARD_FADE + 0.1), Vec3::new(r.x1, y, fz)));
}

/// `p` — середина устья на земле (середина клетки рельефа).
pub(super) fn shaft(b: &mut Builder, p: Vec3, rng: &mut Rng) {
    let cell = 3.0;
    let w = 0.75;
    let open = move |c: Vec3| (c.x - p.x).abs() < w && (c.z - p.z).abs() < w;
    ground_patch(b, Rect::new(p.x - cell, p.z - cell, p.x + cell, p.z + cell), 0.75, 0.75, ground_y, |_| Mat::Terrain, open, |_| false);
    // Колодец: стены в скале до глубины ~5 м, сверху — венцы крепи, на дне — вода.
    let rim = |sx: f32, sz: f32| ground_y(p.x + sx * w, p.z + sz * w);
    let deep = p.y - 5.0;
    let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for k in 0..4 {
        let (a, c) = (corners[k], corners[(k + 1) % 4]);
        let (pa, pc) = (Vec3::new(p.x + a.0 * w, 0.0, p.z + a.1 * w), Vec3::new(p.x + c.0 * w, 0.0, p.z + c.1 * w));
        let inward = Vec3::new(-(a.0 + c.0), 0.0, -(a.1 + c.1)).normalize();
        let rows = 8;
        for r in 0..rows {
            let (t0, t1) = (r as f32 / rows as f32, (r + 1) as f32 / rows as f32);
            let ya = |t: f32, top: f32| top + (deep - top) * t;
            let quad = [
                Vec3::new(pa.x, ya(t0, rim(a.0, a.1)), pa.z),
                Vec3::new(pc.x, ya(t0, rim(c.0, c.1)), pc.z),
                Vec3::new(pc.x, ya(t1, rim(c.0, c.1)), pc.z),
                Vec3::new(pa.x, ya(t1, rim(a.0, a.1)), pa.z),
            ];
            b.mesh(if r < 2 { Mat::Earth } else { Mat::Bedrock }).quad(quad, inward);
        }
    }
    b.mesh(Mat::Water).quad([Vec3::new(p.x - w, deep + 0.3, p.z - w), Vec3::new(p.x + w, deep + 0.3, p.z - w), Vec3::new(p.x + w, deep + 0.3, p.z + w), Vec3::new(p.x - w, deep + 0.3, p.z + w)], Vec3::Y);
    // Венцы крепи внутри колодца (до полутора метров) — бревно к бревну.
    let ground = (0..4).map(|k| rim(corners[k].0, corners[k].1)).fold(f32::MAX, f32::min);
    let mut yy = ground - 0.12;
    let mut k = 0;
    while yy > ground - 1.6 {
        let o = w - 0.09;
        let (len, rr) = (w + 0.02, 0.085);
        if k % 2 == 0 {
            for sz in [-1.0f32, 1.0] {
                log(b, Vec3::new(p.x - len, yy, p.z + sz * o), Vec3::new(p.x + len, yy, p.z + sz * o), rr, rr * 0.95, 5200 + k);
            }
        } else {
            for sx in [-1.0f32, 1.0] {
                log(b, Vec3::new(p.x + sx * o, yy, p.z - len), Vec3::new(p.x + sx * o, yy, p.z + len), rr, rr * 0.95, 5300 + k);
            }
        }
        yy -= 0.17;
        k += 1;
    }
    // Сруб-оклад над землёй: четыре венца с выпусками на углах.
    let base = ground - 0.08;
    let o = w + 0.1;
    for k in 0..4u32 {
        let y = base + k as f32 * 0.16 + 0.1;
        let r = 0.1;
        if k % 2 == 0 {
            for sz in [-1.0f32, 1.0] {
                log(b, Vec3::new(p.x - o - 0.28, y, p.z + sz * o), Vec3::new(p.x + o + 0.3, y + rng.range(-0.02, 0.02), p.z + sz * o), r, r * 0.9, 5400 + k * 2 + (sz > 0.0) as u32);
            }
        } else {
            for sx in [-1.0f32, 1.0] {
                log(b, Vec3::new(p.x + sx * o, y, p.z - o - 0.3), Vec3::new(p.x + sx * o, y + rng.range(-0.02, 0.02), p.z + o + 0.28), r, r * 0.9, 5500 + k * 2 + (sx > 0.0) as u32);
            }
        }
    }
    let collar = base + 0.1 + 3.0 * 0.16 + 0.1;
    kit::hidden(b, Vec3::new(p.x - o - 0.15, ground - 0.3, p.z - o - 0.15), Vec3::new(p.x + o + 0.15, collar, p.z + o + 0.15));
    // Рогатки ворота и вал вдоль X, рукоять справа.
    let top = collar + 0.55;
    for sx in [-1.0f32, 1.0] {
        let x = p.x + sx * 1.3;
        let foot = Vec3::new(x, ground_y(x, p.z) - 0.2, p.z);
        log(b, foot, Vec3::new(x, top - 0.02, p.z), 0.09, 0.08, 5600 + (sx > 0.0) as u32);
        // Развилка: два сука вверх-в стороны, вал ложится между ними.
        for sz in [-1.0f32, 1.0] {
            log(b, Vec3::new(x, top - 0.12, p.z + sz * 0.02), Vec3::new(x + sx * 0.02, top + 0.2, p.z + sz * 0.17), 0.045, 0.035, 5610 + (sz > 0.0) as u32 + (sx > 0.0) as u32 * 2);
        }
        // Подпорки-укосины.
        log(b, Vec3::new(x + sx * 0.55, ground_y(x + sx * 0.55, p.z) - 0.05, p.z), Vec3::new(x + sx * 0.05, top - 0.45, p.z), 0.05, 0.045, 5620 + (sx > 0.0) as u32);
        kit::hidden(b, Vec3::new(x - 0.12, foot.y, p.z - 0.12), Vec3::new(x + 0.12, top + 0.2, p.z + 0.12));
    }
    let axle = top + 0.04;
    log(b, Vec3::new(p.x - 1.45, axle, p.z), Vec3::new(p.x + 1.45, axle, p.z), 0.12, 0.115, 5630);
    // Намотанная верёвка: витки по валу.
    for k in 0..12 {
        let x = p.x - 0.4 + k as f32 * 0.055;
        rod_ring(b, Vec3::new(x, axle, p.z), 0.135, 0.022);
    }
    // Рукоять-коленце: щёки из жердей и ручка.
    let e = Vec3::new(p.x + 1.45, axle, p.z);
    log(b, e, e + Vec3::X * 0.18, 0.04, 0.04, 5640);
    log(b, e + Vec3::X * 0.18, e + Vec3::new(0.18, -0.42, 0.0), 0.035, 0.035, 5641);
    log(b, e + Vec3::new(0.16, -0.42, 0.0), e + Vec3::new(0.45, -0.42, 0.0), 0.028, 0.028, 5642);
    // Верёвка в колодец, на ней — бадья с рудой у самого устья.
    rod(b, Mat::Felt, Vec3::new(p.x, axle - 0.13, p.z + 0.02), Vec3::new(p.x, collar - 0.1, p.z + 0.02), 0.013, 6);
    bucket(b, Vec3::new(p.x - 0.16, collar - 0.55, p.z + 0.02), 0.22, 0.38, Some(Mat::Limonite));
    // Вторая бадья и куча вынутой руды у сруба.
    bucket(b, Vec3::new(p.x - 1.75, ground_y(p.x - 1.75, p.z + 0.9), p.z + 0.9), 0.23, 0.4, None);
}

/// Кольцо верёвки (виток) вокруг оси X.
fn rod_ring(b: &mut Builder, c: Vec3, r: f32, t: f32) {
    let n = 12;
    let pts: Vec<Vec3> = (0..=n).map(|k| {
        let a = TAU * k as f32 / n as f32;
        c + Vec3::new(0.0, a.cos() * r, a.sin() * r)
    }).collect();
    for w in pts.windows(2) {
        rod(b, Mat::Felt, w[0], w[1], t, 5);
    }
}

// ---------------------------------------------------------------------------
// Землянка рудокопов
// ---------------------------------------------------------------------------

/// Землянка: сруб из круглого леса (венцы с выпусками на углах), на треть в земле — с трёх
/// сторон присыпан валом; двускатная кровля по жердям, крытая дёрном; дверь в торце.
/// Вырез рельефа под землянку (кратный клетке 6 м) — с запасом под вал и прокоп к двери.
fn area_of(h: Rect) -> Rect {
    let cell = terrain::CELL;
    let snap = |v: f32, up: bool| if up { (v / cell).ceil() * cell } else { (v / cell).floor() * cell };
    Rect::new(snap(h.x0 - 1.8, false), snap(h.z0 - 1.8, false), snap(h.x1 + 3.0, true), snap(h.z1 + 1.8, true))
}

pub(super) fn dugout(b: &mut Builder, h: Rect, hy: f32) {
    let (w, d) = (h.w(), h.d());
    let c = Vec3::new(h.cx(), hy, h.cz());
    // Земля вокруг своя: пол внутри ниже склона, вал с трёх сторон, к двери (+X) — прокоп.
    let height = move |x: f32, z: f32| {
        let inside = x > h.x0 - 0.01 && x < h.x1 + 0.01 && z > h.z0 - 0.01 && z < h.z1 + 0.01;
        if inside {
            return hy;
        }
        let dx = (h.x0 - x).max(x - h.x1).max(0.0);
        let dz = (h.z0 - z).max(z - h.z1).max(0.0);
        let dist = (dx * dx + dz * dz).sqrt();
        let nat = ground_y(x, z);
        let berm = hy + 0.85 - (dist - 0.3).max(0.0) * 0.55 + (noise3(Vec3::new(x, 0.0, z) * 0.9, 151) - 0.47) * 0.25 * smoothstep(0.3, 1.0, dist);
        let mut y = nat.max(berm);
        if x > h.x1 - 0.01 {
            let lat = (z - h.cz()).abs();
            let trench = hy + (x - h.x1 - 0.9).max(0.0) * 0.45 + smoothstep(0.5, 1.5, lat) * 2.0;
            y = y.min(trench).max(hy);
        }
        // К краю выреза — ровно в рельеф, без уступа.
        let a = area_of(h);
        let edge = (x - a.x0).min(a.x1 - x).min(z - a.z0).min(a.z1 - z);
        nat + (y - nat) * smoothstep(0.0, 0.9, edge)
    };
    let area = area_of(h);
    let inside = move |p: Vec3| p.x > h.x0 && p.x < h.x1 && p.z > h.z0 && p.z < h.z1;
    let mat = move |p: Vec3| if inside(p) || (p.x > h.x1 && p.y < ground_y(p.x, p.z) - 0.08) { Mat::Earth } else { Mat::Terrain };
    ground_patch(b, area, 0.5, 0.75, height, mat, |_| false, inside);
    b.solid([h.x0, hy - 1.0, h.z0], [h.x1, hy, h.z1], Mat::Earth);
    b.placed(c, move |t| {
        let l = Rect::new(-w / 2.0, -d / 2.0, w / 2.0, d / 2.0);
        let r = 0.13;
        let ht = 1.6;
        let (dz0, dz1) = (-0.48, 0.48);
        let door_h = 1.5;
        let ix = l.x1 - 0.15;
        // Венцы: вдоль X и вдоль Z сдвинуты на полбревна (замок «в обло»).
        let mut k = 0u32;
        let mut y = r;
        while y < ht {
            let along_x = k % 2 == 0;
            let yy = y;
            if along_x {
                for z in [l.z0 + 0.15, l.z1 - 0.15] {
                    log(t, Vec3::new(l.x0 - 0.12, yy, z), Vec3::new(l.x1 + 0.12, yy + 0.01, z), r, r * 0.92, 6000 + k * 2 + (z > 0.0) as u32);
                }
            } else {
                let x = l.x0 + 0.15;
                log(t, Vec3::new(x, yy, l.z0 - 0.12), Vec3::new(x, yy + 0.01, l.z1 + 0.12), r, r * 0.92, 6100 + k);
                // Стена с дверью: ниже притолоки — два бревна по сторонам проёма.
                if yy < door_h {
                    log(t, Vec3::new(ix, yy, l.z0 - 0.12), Vec3::new(ix, yy, dz0 - 0.08), r, r * 0.95, 6200 + k);
                    log(t, Vec3::new(ix, yy, dz1 + 0.08), Vec3::new(ix, yy, l.z1 + 0.12), r, r * 0.95, 6300 + k);
                } else {
                    log(t, Vec3::new(ix, yy, l.z0 - 0.12), Vec3::new(ix, yy + 0.01, l.z1 + 0.12), r, r * 0.92, 6400 + k);
                }
            }
            y += r * 1.0;
            k += 1;
        }
        // Дверная коробка: косяки и притолока.
        for z in [dz0 - 0.06, dz1 + 0.06] {
            log(t, Vec3::new(ix + 0.02, 0.0, z), Vec3::new(ix + 0.02, door_h + 0.08, z), 0.08, 0.075, 6500 + (z > 0.0) as u32);
        }
        log(t, Vec3::new(ix + 0.02, door_h + 0.1, dz0 - 0.3), Vec3::new(ix + 0.02, door_h + 0.12, dz1 + 0.3), 0.1, 0.1, 6510);
        t.door(DoorSpec { center: Vec3::new(ix, 0.0, 0.0), along_x: false, width: dz1 - dz0, height: door_h, wall_t: 0.26, double: false, open: false });
        // Стены для столкновений (с проёмом двери).
        kit::hidden(t, Vec3::new(l.x0, 0.0, l.z0), Vec3::new(l.x1, ht, l.z0 + 0.28));
        kit::hidden(t, Vec3::new(l.x0, 0.0, l.z1 - 0.28), Vec3::new(l.x1, ht, l.z1));
        kit::hidden(t, Vec3::new(l.x0, 0.0, l.z0), Vec3::new(l.x0 + 0.28, ht, l.z1));
        kit::hidden(t, Vec3::new(ix - 0.14, 0.0, l.z0), Vec3::new(ix + 0.14, ht, dz0));
        kit::hidden(t, Vec3::new(ix - 0.14, 0.0, dz1), Vec3::new(ix + 0.14, ht, l.z1));
        kit::hidden(t, Vec3::new(ix - 0.14, door_h, dz0), Vec3::new(ix + 0.14, ht, dz1));
        // Кровля: конёк вдоль X, стропила-жерди, по ним слой дёрна; фронтоны из венцов.
        let ridge = ht + 1.25;
        let (ez, eave) = (d / 2.0 + 0.45, ht - 0.2);
        log(t, Vec3::new(l.x0 - 0.5, ridge, 0.0), Vec3::new(l.x1 + 0.5, ridge + 0.02, 0.0), 0.12, 0.11, 6600);
        let nraf = 7;
        for i in 0..nraf {
            let x = l.x0 - 0.3 + (w + 0.6) * i as f32 / (nraf - 1) as f32;
            for sz in [-1.0f32, 1.0] {
                log(t, Vec3::new(x, eave - 0.05, sz * ez), Vec3::new(x, ridge + 0.05, sz * 0.05), 0.06, 0.055, 6610 + i * 2 + (sz > 0.0) as u32);
            }
        }
        // Дёрн: толстый слой поверх стропил, по краям видна земля.
        let th = 0.16;
        let (xa, xb) = (l.x0 - 0.55, l.x1 + 0.55);
        for sz in [-1.0f32, 1.0] {
            let (top0, top1) = (Vec3::new(xa, ridge + 0.12 + th, 0.0), Vec3::new(xb, ridge + 0.12 + th, 0.0));
            let (lo0, lo1) = (Vec3::new(xa, eave + th - 0.02, sz * (ez + 0.08)), Vec3::new(xb, eave + th - 0.02, sz * (ez + 0.08)));
            let up = Vec3::new(0.0, 1.0, sz * 0.9);
            t.mesh(Mat::Terrain).quad([top0, top1, lo1, lo0], up);
            let dn = Vec3::Y * th;
            t.mesh(Mat::Earth).quad([lo0, lo1, lo1 - dn, lo0 - dn], Vec3::new(0.0, -0.2, sz));
            for (p0, p1, out) in [(top0, lo0, Vec3::NEG_X), (top1, lo1, Vec3::X)] {
                t.mesh(Mat::Earth).quad([p0, p1, p1 - dn, p0 - dn], out);
            }
            t.mesh(Mat::Earth).quad([lo0 - dn, lo1 - dn, top1 - dn, top0 - dn], Vec3::new(0.0, -1.0, -sz));
        }
        // Фронтоны: короткие брёвна, всё короче к коньку.
        for (x, seed) in [(l.x0 + 0.15, 6700u32), (ix, 6800)] {
            let mut yy = ht + r * 0.5;
            let mut k = 0;
            while yy < ridge - 0.1 {
                let half = (d / 2.0) * (ridge - yy) / (ridge - eave) - 0.05;
                if half > 0.15 {
                    log(t, Vec3::new(x, yy, -half), Vec3::new(x, yy, half), 0.11, 0.1, seed + k);
                }
                yy += 0.21;
                k += 1;
            }
        }
        // Внутри: лежанка, очаг.
        kit::bed(t, Rect::new(l.x0 + 0.35, l.z0 + 0.35, l.x0 + 1.15, l.z1 - 0.35));
        kit::hearth(t, Vec3::new(l.x1 - 1.2, 0.0, l.z0 + 0.9));
    });
}

/// Обруч вокруг вертикали (обод корзины, кольцо): гладкая трубка радиуса `t`.
fn hoop(b: &mut Builder, mat: Mat, c: Vec3, r: f32, t: f32, n: usize) {
    let pts: Vec<Vec3> = (0..=n).map(|k| {
        let a = TAU * k as f32 / n as f32;
        c + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
    }).collect();
    for w in pts.windows(2) {
        rod(b, mat, w[0], w[1], t, 5);
    }
}

/// Плетёная корзина из лозы: стенки (волокна идут по кругу — ряды плетения), стояки снаружи,
/// толстый обод; сверху — руда или уголь горкой.
pub(super) fn basket(b: &mut Builder, p: Vec3, r: f32, h: f32, fill: Option<Mat>) {
    b.mesh(Mat::Timber).lathe(p, &[(r * 0.75, 0.0), (r * 0.95, h * 0.45), (r * 1.03, h), (r * 0.97, h), (r * 0.89, h * 0.55), (r * 0.66, 0.04)], 14);
    let n = 11;
    for k in 0..n {
        let a = TAU * (k as f32 + 0.5) / n as f32;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        let pts = [p + d * (r * 0.77), p + d * (r * 0.965) + Vec3::Y * (h * 0.45), p + d * (r * 1.045) + Vec3::Y * h];
        rod(b, Mat::Timber, pts[0], pts[1], 0.009, 5);
        rod(b, Mat::Timber, pts[1], pts[2], 0.009, 5);
    }
    hoop(b, Mat::Timber, p + Vec3::Y * h, r * 1.0, 0.017, 16);
    hoop(b, Mat::Timber, p + Vec3::Y * 0.03, r * 0.76, 0.014, 12);
    if let Some(m) = fill {
        let mut rng = Rng::new((p.x * 17.0 + p.z * 5.0) as i32 as u32);
        b.mesh(m).lathe(p + Vec3::Y * (h * 0.82), &[(r * 0.97, 0.0), (r * 0.6, h * 0.18), (0.0, h * 0.24)], 10);
        for _ in 0..7 {
            let a = rng.range(0.0, TAU);
            let d = r * rng.range(0.0, 0.7);
            chunk(b, m, p + Vec3::new(a.cos() * d, h * (0.95 + 0.12 * (1.0 - d / r)), a.sin() * d), r * rng.range(0.16, 0.26), &mut rng);
        }
    }
    kit::hidden(b, p - Vec3::new(r, 0.0, r), p + Vec3::new(r, h, r));
}

/// Костёр рудокопов: кольцо камней, зола, угли, шалашик из поленьев, огонь и свет; вокруг —
/// брёвна-сиденья и чурбак.
pub(super) fn fire_pit(b: &mut Builder, p: Vec3) {
    b.at(p.x, p.z);
    b.hearths.push(p);
    let mut rng = Rng::new((p.x * 7.0 + p.z * 3.0) as i32 as u32);
    b.mesh(Mat::Soot).disc(p + Vec3::Y * 0.025, 0.75, 14, true);
    for k in 0..11 {
        let a = TAU * k as f32 / 11.0 + rng.range(-0.1, 0.1);
        let (x, z) = (p.x + a.cos() * 0.85, p.z + a.sin() * 0.85);
        b.plant(PlantKind::Rock, Vec3::new(x, ground_y(x, z) - 0.015, z), rng.range(0.32, 0.45), rng.range(0.0, TAU), rng.int(64) as u32);
    }
    b.mesh(Mat::Coals).disc(p + Vec3::Y * 0.045, 0.38, 10, true);
    for k in 0..5 {
        let a = TAU * k as f32 / 5.0 + 0.3;
        let d = Vec3::new(a.cos(), 0.0, a.sin());
        log(b, p + d * 0.56 + Vec3::Y * 0.03, p + d * 0.07 + Vec3::Y * 0.56, 0.065, 0.045, 7000 + k);
    }
    // Обугленные головни с краю.
    for _ in 0..3 {
        let a = rng.range(0.0, TAU);
        let q = p + Vec3::new(a.cos() * 0.45, 0.06, a.sin() * 0.45);
        let t = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.18;
        rod(b, Mat::Soot, q - t, q + t, 0.035, 6);
    }
    b.torch(p.x, p.y + 0.5, p.z, Vec3::ZERO);
    b.lamp(p + Vec3::Y * 1.0, Color::srgb(1.0, 0.6, 0.3), 30_000.0, 14.0);
    kit::hidden(b, p - Vec3::new(0.75, 0.0, 0.75), p + Vec3::new(0.75, 0.3, 0.75));
    // Сиденья: бревно на камнях-подкладках и чурбак.
    for (k, ang) in [(0u32, 1.0f32), (1, 3.1)] {
        let c = p + Vec3::new(ang.cos(), 0.0, ang.sin()) * 2.2;
        let t = Vec3::new(-ang.sin(), 0.0, ang.cos()) * 0.95;
        let (a0, a1) = (c - t, c + t);
        let (y0, y1) = (ground_y(a0.x, a0.z), ground_y(a1.x, a1.z));
        log(b, Vec3::new(a0.x, y0 + 0.1, a0.z), Vec3::new(a1.x, y1 + 0.1, a1.z), 0.17, 0.15, 7100 + k);
        kit::hidden(b, Vec3::new(c.x - 0.5, y0.min(y1), c.z - 0.5), Vec3::new(c.x + 0.5, y0.max(y1) + 0.34, c.z + 0.5));
    }
    let st = p + Vec3::new(0.4f32.cos(), 0.0, 0.4f32.sin()) * 2.0;
    let sy = ground_y(st.x, st.z);
    log(b, Vec3::new(st.x, sy - 0.35, st.z), Vec3::new(st.x, sy + 0.42, st.z), 0.21, 0.2, 7150);
    kit::hidden(b, Vec3::new(st.x - 0.2, sy, st.z - 0.2), Vec3::new(st.x + 0.2, sy + 0.42, st.z + 0.2));
}
