//! Процедурная геометрия: сборщик мешей и готические элементы
//! (стрельчатые арки, своды с нервюрами, шпили, крыши, окна).
//!
//! Все детали одного материала сливаются в один меш, чтобы сцена рисовалась быстро.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

/// Собирает треугольники с плоскими нормалями и UV.
#[derive(Default, Clone)]
pub struct MeshBuilder {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl MeshBuilder {
    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    /// Все треугольники (для расчётов на процессоре: над чем крыша и т. п.).
    pub fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.idx.chunks_exact(3).map(|t| [0, 1, 2].map(|k| Vec3::from_array(self.pos[t[k] as usize])))
    }

    /// Четырёхугольник. Порядок вершин переворачивается так, чтобы лицевая сторона смотрела по `facing`.
    pub fn quad_uv(&mut self, v: [Vec3; 4], uv: [Vec2; 4], facing: Vec3) {
        let mut n = (v[1] - v[0]).cross(v[2] - v[0]);
        if n.length_squared() < 1e-12 {
            n = (v[2] - v[0]).cross(v[3] - v[0]);
        }
        let flip = n.dot(facing) < 0.0;
        let n = if flip { -n } else { n }.normalize_or_zero();
        let base = self.pos.len() as u32;
        for i in 0..4 {
            self.pos.push(v[i].to_array());
            self.nrm.push(n.to_array());
            self.uv.push(uv[i].to_array());
        }
        if flip {
            self.idx.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        } else {
            self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    pub fn quad(&mut self, v: [Vec3; 4], facing: Vec3) {
        // UV в метрах по двум рёбрам — пригодится для узоров, завязанных на UV.
        let u = (v[1] - v[0]).length();
        let w = (v[3] - v[0]).length();
        self.quad_uv(v, [Vec2::ZERO, Vec2::new(u, 0.0), Vec2::new(u, w), Vec2::new(0.0, w)], facing);
    }

    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, facing: Vec3) {
        let mut n = (b - a).cross(c - a);
        let flip = n.dot(facing) < 0.0;
        if flip {
            n = -n;
        }
        let n = n.normalize_or_zero();
        let base = self.pos.len() as u32;
        for p in [a, b, c] {
            self.pos.push(p.to_array());
            self.nrm.push(n.to_array());
        }
        self.uv.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
        if flip {
            self.idx.extend_from_slice(&[base, base + 2, base + 1]);
        } else {
            self.idx.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }

    /// Плоский выпуклый многоугольник (веером), с UV.
    pub fn polygon(&mut self, pts: &[Vec3], uvs: &[Vec2], facing: Vec3) {
        if pts.len() < 3 {
            return;
        }
        let n = facing.normalize_or_zero();
        let base = self.pos.len() as u32;
        for (p, uv) in pts.iter().zip(uvs) {
            self.pos.push(p.to_array());
            self.nrm.push(n.to_array());
            self.uv.push(uv.to_array());
        }
        let geo = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        let flip = geo.dot(facing) < 0.0;
        for i in 1..pts.len() as u32 - 1 {
            if flip {
                self.idx.extend_from_slice(&[base, base + i + 1, base + i]);
            } else {
                self.idx.extend_from_slice(&[base, base + i, base + i + 1]);
            }
        }
    }

    /// Коробка по осям. UV — в метрах (для узоров по UV).
    pub fn cuboid(&mut self, min: Vec3, max: Vec3) {
        let c = (min + max) / 2.0;
        let h = (max - min) / 2.0;
        for axis in 0..3 {
            for sign in [-1.0f32, 1.0] {
                let mut n = Vec3::ZERO;
                n[axis] = sign;
                let (ua, va) = match axis {
                    0 => (2, 1),
                    1 => (0, 2),
                    _ => (0, 1),
                };
                let mut u = Vec3::ZERO;
                u[ua] = h[ua];
                let mut v = Vec3::ZERO;
                v[va] = h[va];
                let fc = c + n * h[axis];
                self.quad([fc - u - v, fc + u - v, fc + u + v, fc - u + v], n);
            }
        }
    }

    /// Призма вдоль отрезка a→b с прямоугольным сечением (для нервюр, обрамлений).
    pub fn beam(&mut self, a: Vec3, b: Vec3, half_w: f32, half_h: f32, up_hint: Vec3) {
        let dir = (b - a).normalize_or_zero();
        if dir == Vec3::ZERO {
            return;
        }
        let mut side = dir.cross(up_hint);
        if side.length_squared() < 1e-6 {
            side = dir.cross(Vec3::X);
        }
        let side = side.normalize() * half_w;
        let up = side.normalize().cross(dir).normalize() * half_h;
        let corners = |p: Vec3| [p - side - up, p + side - up, p + side + up, p - side + up];
        let ca = corners(a);
        let cb = corners(b);
        for i in 0..4 {
            let j = (i + 1) % 4;
            let mid = (ca[i] + ca[j]) / 2.0 - a;
            self.quad([ca[i], ca[j], cb[j], cb[i]], mid);
        }
    }

    /// Нервюра или обрамление вдоль ломаной.
    pub fn beam_path(&mut self, pts: &[Vec3], half_w: f32, half_h: f32, up_hint: Vec3) {
        for w in pts.windows(2) {
            // Чуть удлиняем сегменты, чтобы не было щелей на изгибах.
            let d = (w[1] - w[0]).normalize_or_zero() * half_h.min(half_w) * 0.5;
            self.beam(w[0] - d, w[1] + d, half_w, half_h, up_hint);
        }
    }

    /// Многогранная пирамида (шпиль). `sides` = 4 — квадратный, 8 — восьмигранный.
    pub fn spire(&mut self, base: Vec3, radius: f32, height: f32, sides: usize) {
        let apex = base + Vec3::Y * height;
        let offset = if sides == 4 { std::f32::consts::FRAC_PI_4 } else { 0.0 };
        let ring: Vec<Vec3> = (0..sides)
            .map(|i| {
                let a = offset + i as f32 / sides as f32 * std::f32::consts::TAU;
                base + Vec3::new(a.cos() * radius, 0.0, a.sin() * radius)
            })
            .collect();
        for i in 0..sides {
            let a = ring[i];
            let b = ring[(i + 1) % sides];
            let out = ((a + b) / 2.0 - base).normalize() + Vec3::Y * (radius / height);
            self.tri(a, b, apex, out);
        }
    }

    /// Восьмигранная призма (колонна, барабан башенки).
    pub fn prism(&mut self, base: Vec3, radius: f32, height: f32, sides: usize) {
        let offset = if sides == 4 { std::f32::consts::FRAC_PI_4 } else { std::f32::consts::PI / sides as f32 };
        let ring: Vec<Vec3> = (0..sides)
            .map(|i| {
                let a = offset + i as f32 / sides as f32 * std::f32::consts::TAU;
                Vec3::new(a.cos() * radius, 0.0, a.sin() * radius)
            })
            .collect();
        for i in 0..sides {
            let a = base + ring[i];
            let b = base + ring[(i + 1) % sides];
            let up = Vec3::Y * height;
            self.quad([a, b, b + up, a + up], (ring[i] + ring[(i + 1) % sides]) / 2.0);
        }
        let top: Vec<Vec3> = ring.iter().map(|p| base + *p + Vec3::Y * height).collect();
        let uvs: Vec<Vec2> = ring.iter().map(|p| Vec2::new(p.x, p.z)).collect();
        self.polygon(&top, &uvs, Vec3::Y);
    }

    /// Двускатная крыша над прямоугольником. Конёк идёт вдоль оси X (`ridge_x`) или Z.
    pub fn gable_roof(&mut self, min: Vec2, max: Vec2, eave_y: f32, ridge_y: f32, ridge_x: bool) {
        let (x0, z0, x1, z1) = (min.x, min.y, max.x, max.y);
        if ridge_x {
            let zm = (z0 + z1) / 2.0;
            let half = (z1 - z0) / 2.0;
            let slope = Vec2::new(half, ridge_y - eave_y).length();
            let l = x1 - x0;
            for (ze, s) in [(z0, -1.0f32), (z1, 1.0)] {
                self.quad_uv(
                    [Vec3::new(x0, eave_y, ze), Vec3::new(x1, eave_y, ze), Vec3::new(x1, ridge_y, zm), Vec3::new(x0, ridge_y, zm)],
                    [Vec2::new(0.0, slope), Vec2::new(l, slope), Vec2::new(l, 0.0), Vec2::new(0.0, 0.0)],
                    Vec3::new(0.0, half / (ridge_y - eave_y).max(0.01), s),
                );
            }
            for (xe, s) in [(x0, -1.0f32), (x1, 1.0)] {
                self.tri(Vec3::new(xe, eave_y, z0), Vec3::new(xe, eave_y, z1), Vec3::new(xe, ridge_y, zm), Vec3::X * s);
            }
        } else {
            let xm = (x0 + x1) / 2.0;
            let half = (x1 - x0) / 2.0;
            let slope = Vec2::new(half, ridge_y - eave_y).length();
            let l = z1 - z0;
            for (xe, s) in [(x0, -1.0f32), (x1, 1.0)] {
                self.quad_uv(
                    [Vec3::new(xe, eave_y, z0), Vec3::new(xe, eave_y, z1), Vec3::new(xm, ridge_y, z1), Vec3::new(xm, ridge_y, z0)],
                    [Vec2::new(0.0, slope), Vec2::new(l, slope), Vec2::new(l, 0.0), Vec2::new(0.0, 0.0)],
                    Vec3::new(s, half / (ridge_y - eave_y).max(0.01), 0.0),
                );
            }
            for (ze, s) in [(z0, -1.0f32), (z1, 1.0)] {
                self.tri(Vec3::new(x0, eave_y, ze), Vec3::new(x1, eave_y, ze), Vec3::new(xm, ridge_y, ze), Vec3::Z * s);
            }
        }
    }

    /// Только скаты двускатной крыши (без треугольных торцов).
    pub fn gable_roof_slopes(&mut self, min: Vec2, max: Vec2, eave_y: f32, ridge_y: f32, ridge_x: bool) {
        let mut tmp = MeshBuilder::default();
        tmp.gable_roof(min, max, eave_y, ridge_y, ridge_x);
        // Последние два треугольника (6 индексов, 6 вершин) — торцы; отрезаем их.
        let tri_verts = 6;
        tmp.pos.truncate(tmp.pos.len() - tri_verts);
        tmp.nrm.truncate(tmp.nrm.len() - tri_verts);
        tmp.uv.truncate(tmp.uv.len() - tri_verts);
        tmp.idx.truncate(tmp.idx.len() - 6);
        self.append(tmp);
    }

    /// Та же геометрия, но лицом в другую сторону (изнанка крыши, потолок).
    pub fn flipped(mut self) -> MeshBuilder {
        for n in &mut self.nrm {
            *n = [-n[0], -n[1], -n[2]];
        }
        for t in self.idx.chunks_mut(3) {
            t.swap(1, 2);
        }
        self
    }

    /// Односкатная крыша: верхний край над линией `high`, нижний — над `low` (обе вдоль одной оси).
    pub fn lean_roof(&mut self, high: [Vec3; 2], low: [Vec3; 2]) {
        let along = (high[1] - high[0]).length();
        let slope = (low[0] - high[0]).length();
        let facing = {
            let d = low[0] - high[0];
            Vec3::new(d.x, 0.0, d.z).normalize_or_zero() + Vec3::Y
        };
        self.quad_uv(
            [high[0], high[1], low[1], low[0]],
            [Vec2::new(0.0, 0.0), Vec2::new(along, 0.0), Vec2::new(along, slope), Vec2::new(0.0, slope)],
            facing,
        );
    }

    /// Коньковый ряд калиптеров: полутруба над отрезком a→b (радиус `r`, чуть приплюснута).
    /// UV — под фото черепицы: u в узкой полосе вокруг оси калиптера (0.5 м развёртки),
    /// v — вдоль конька, поэтому видны стыки и нахлёсты калиптеров через каждые полметра.
    pub fn ridge_pipe(&mut self, a: Vec3, b: Vec3, r: f32, segs: usize) {
        let len = (b - a).length();
        let dir = (b - a).normalize_or_zero();
        if len < 1e-4 || segs == 0 {
            return;
        }
        let side = dir.cross(Vec3::Y).normalize_or(Vec3::X);
        let ring = |ang: f32| (side * ang.cos() * r + Vec3::Y * ang.sin() * r * 0.75, (side * ang.cos() + Vec3::Y * ang.sin()).normalize());
        let mut ids = Vec::with_capacity((segs + 1) * 2);
        for i in 0..=segs {
            let ang = std::f32::consts::PI * i as f32 / segs as f32;
            let (o, n) = ring(ang);
            let u = 0.5 + ang.cos() * 0.1;
            ids.push((self.vert(a + o, n, Vec2::new(u, 0.0)), self.vert(b + o, n, Vec2::new(u, len))));
        }
        for i in 0..segs {
            let (a0, b0) = ids[i];
            let (a1, b1) = ids[i + 1];
            // Лицом наружу: от a к b вдоль `dir`, по дуге от `side` вверх.
            self.idx.extend_from_slice(&[a0, b0, b1, a0, b1, a1]);
        }
        // Торцы забиты раствором — полукруглые заглушки той же черепицы.
        for (p, out) in [(a, -dir), (b, dir)] {
            let c = self.vert(p, out, Vec2::new(0.5, 0.25));
            let rim: Vec<u32> = (0..=segs)
                .map(|i| {
                    let ang = std::f32::consts::PI * i as f32 / segs as f32;
                    self.vert(p + ring(ang).0, out, Vec2::new(0.5 + ang.cos() * 0.1, 0.25 + ang.sin() * 0.05))
                })
                .collect();
            for w in rim.windows(2) {
                if out.dot(dir) > 0.0 {
                    self.idx.extend_from_slice(&[c, w[1], w[0]]);
                } else {
                    self.idx.extend_from_slice(&[c, w[0], w[1]]);
                }
            }
        }
    }

    /// Вершина с явной нормалью (для гладких тел вращения и рельефа).
    pub fn vert(&mut self, p: Vec3, n: Vec3, uv: Vec2) -> u32 {
        self.pos.push(p.to_array());
        self.nrm.push(n.to_array());
        self.uv.push(uv.to_array());
        self.pos.len() as u32 - 1
    }

    pub fn index_tri(&mut self, a: u32, b: u32, c: u32) {
        self.idx.extend_from_slice(&[a, b, c]);
    }

    pub fn append(&mut self, other: MeshBuilder) {
        let base = self.pos.len() as u32;
        self.pos.extend(other.pos);
        self.nrm.extend(other.nrm);
        self.uv.extend(other.uv);
        self.idx.extend(other.idx.into_iter().map(|i| i + base));
    }

    /// Сдвигает всю геометрию.
    pub fn translate(&mut self, d: Vec3) {
        for p in &mut self.pos {
            *p = (Vec3::from(*p) + d).to_array();
        }
    }

    /// Тело вращения вокруг вертикали через `base`: профиль (радиус, высота) снизу вверх.
    /// Нормали сглажены, кроме резких изломов профиля (> 40°). Полный круг.
    pub fn lathe(&mut self, base: Vec3, profile: &[(f32, f32)], sides: usize) {
        self.lathe_arc(base, profile, 0.0, std::f32::consts::TAU, sides, false);
    }

    /// Часть тела вращения от угла `a0` до `a1` (угол 0 — +X, растёт к +Z).
    /// `inward` — лицевая сторона внутрь (апсида, чаша, свод изнутри).
    pub fn lathe_arc(&mut self, base: Vec3, profile: &[(f32, f32)], a0: f32, a1: f32, segs: usize, inward: bool) {
        if profile.len() < 2 || segs == 0 {
            return;
        }
        // Нормаль каждого отрезка профиля в плоскости (радиус, y).
        let seg_n: Vec<Vec2> = profile
            .windows(2)
            .map(|w| {
                let t = Vec2::new(w[1].0 - w[0].0, w[1].1 - w[0].1);
                Vec2::new(t.y, -t.x).normalize_or(Vec2::X)
            })
            .collect();
        let crease = 40f32.to_radians().cos();
        for (k, w) in profile.windows(2).enumerate() {
            let n_here = seg_n[k];
            let end_normal = |other: Option<&Vec2>| match other {
                Some(o) if o.dot(n_here) > crease => (n_here + *o).normalize(),
                _ => n_here,
            };
            let na = end_normal(if k > 0 { seg_n.get(k - 1) } else { None });
            let nb = end_normal(seg_n.get(k + 1));
            let ((ra, ya), (rb, yb)) = (w[0], w[1]);
            let len = Vec2::new(rb - ra, yb - ya).length();
            let mut ring = Vec::with_capacity((segs + 1) * 2);
            for i in 0..=segs {
                let a = a0 + (a1 - a0) * i as f32 / segs as f32;
                let dir = Vec3::new(a.cos(), 0.0, a.sin());
                let u = a * ra.max(rb).max(0.05);
                let sign = if inward { -1.0 } else { 1.0 };
                let nva = (dir * na.x + Vec3::Y * na.y) * sign;
                let nvb = (dir * nb.x + Vec3::Y * nb.y) * sign;
                ring.push(self.vert(base + dir * ra + Vec3::Y * ya, nva, Vec2::new(u, ya)));
                ring.push(self.vert(base + dir * rb + Vec3::Y * yb, nvb, Vec2::new(u, ya + len)));
            }
            for i in 0..segs {
                let (a, b, c, d) = (ring[i * 2], ring[i * 2 + 2], ring[i * 2 + 3], ring[i * 2 + 1]);
                // При обходе по углу вверх лицевая сторона смотрит наружу для порядка a, d, c / a, c, b.
                if inward {
                    self.idx.extend_from_slice(&[a, b, c, a, c, d]);
                } else {
                    self.idx.extend_from_slice(&[a, c, b, a, d, c]);
                }
            }
        }
    }

    /// Плоский диск (крышка, вода в цистерне).
    pub fn disc(&mut self, c: Vec3, r: f32, sides: usize, up: bool) {
        let n = if up { Vec3::Y } else { Vec3::NEG_Y };
        let center = self.vert(c, n, Vec2::new(c.x, c.z));
        let ring: Vec<u32> = (0..=sides)
            .map(|i| {
                let a = i as f32 / sides as f32 * std::f32::consts::TAU;
                let p = c + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                self.vert(p, n, Vec2::new(p.x, p.z))
            })
            .collect();
        for i in 0..sides {
            if up {
                self.idx.extend_from_slice(&[center, ring[i + 1], ring[i]]);
            } else {
                self.idx.extend_from_slice(&[center, ring[i], ring[i + 1]]);
            }
        }
    }

    /// Коробка, повёрнутая вокруг вертикали на `yaw` (радианы, от +X к +Z).
    pub fn obox(&mut self, center: Vec3, half: Vec3, yaw: f32) {
        let (s, c) = yaw.sin_cos();
        let ax = Vec3::new(c, 0.0, s) * half.x;
        let az = Vec3::new(-s, 0.0, c) * half.z;
        let ay = Vec3::Y * half.y;
        for (n, u, v) in [(ax, az, ay), (-ax, ay, az), (az, ay, ax), (-az, ax, ay), (ay, ax, az), (-ay, az, ax)] {
            let f = center + n;
            self.quad([f - u - v, f + u - v, f + u + v, f - u + v], n);
        }
    }

    /// Цилиндрический (полукруглый) свод над прямоугольником: ось вдоль X (`along_x`) или Z.
    /// Рисуется внутренняя сторона (интрадос) и наружная (экстрадос) толщиной `thick`.
    pub fn barrel_vault(&mut self, min: Vec2, max: Vec2, spring: f32, along_x: bool, thick: f32) {
        const N: usize = 16;
        let (span, len) = if along_x { (max.y - min.y, max.x - min.x) } else { (max.x - min.x, max.y - min.y) };
        let r = span / 2.0;
        let c = (min + max) / 2.0;
        let p = |k: usize, t: f32, rr: f32| {
            let a = std::f32::consts::PI * k as f32 / N as f32;
            let off = -a.cos() * rr;
            let y = spring + a.sin() * rr;
            if along_x { Vec3::new(min.x + len * t, y, c.y + off) } else { Vec3::new(c.x + off, y, min.y + len * t) }
        };
        let axis_c = |t: f32| if along_x { Vec3::new(min.x + len * t, spring, c.y) } else { Vec3::new(c.x, spring, min.y + len * t) };
        for k in 0..N {
            let inner = [p(k, 0.0, r), p(k + 1, 0.0, r), p(k + 1, 1.0, r), p(k, 1.0, r)];
            let mid = (inner[0] + inner[2]) / 2.0;
            self.quad(inner, axis_c(0.5) - mid);
            let outer = [p(k, 0.0, r + thick), p(k + 1, 0.0, r + thick), p(k + 1, 1.0, r + thick), p(k, 1.0, r + thick)];
            let mid = (outer[0] + outer[2]) / 2.0;
            self.quad(outer, mid - axis_c(0.5));
        }
    }

    /// Купол-полусфера (или её часть по углу `a0..a1` — конха апсиды), изнутри и снаружи.
    pub fn dome(&mut self, c: Vec3, r: f32, a0: f32, a1: f32, segs: usize, thick: f32) {
        const RINGS: usize = 8;
        let prof = |rr: f32| -> Vec<(f32, f32)> {
            (0..=RINGS)
                .map(|k| {
                    let t = k as f32 / RINGS as f32 * std::f32::consts::FRAC_PI_2;
                    (t.cos() * rr, t.sin() * rr)
                })
                .collect()
        };
        self.lathe_arc(c, &prof(r), a0, a1, segs, true);
        self.lathe_arc(c, &prof(r + thick), a0, a1, segs, false);
    }

    pub fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
            .with_inserted_indices(Indices::U32(self.idx))
    }
}

/// Профиль стрельчатой арки: высота над пятой арки в точке `s` (от центра пролёта).
/// `span` — ширина пролёта, `rise` — подъём (≥ span/2; span/2 — полукруглая арка).
pub fn arch_height(s: f32, span: f32, rise: f32) -> f32 {
    let half = span / 2.0;
    let rise = rise.max(half);
    let r = (rise * rise + half * half) / span;
    let d = s.abs() + r - half;
    (r * r - d * d).max(0.0).sqrt()
}

/// Локальная система координат стены: `s` — вдоль стены, `d` — поперёк (толщина), `y` — вверх.
#[derive(Clone, Copy, Debug)]
pub struct WallFrame {
    /// true: стена тянется вдоль X (толщина по Z), false: вдоль Z (толщина по X).
    pub along_x: bool,
}

impl WallFrame {
    pub fn p(&self, s: f32, y: f32, d: f32) -> Vec3 {
        if self.along_x { Vec3::new(s, y, d) } else { Vec3::new(d, y, s) }
    }
    pub fn d_axis(&self) -> Vec3 {
        if self.along_x { Vec3::Z } else { Vec3::X }
    }
    pub fn s_axis(&self) -> Vec3 {
        if self.along_x { Vec3::X } else { Vec3::Z }
    }
    /// Мировой AABB из локальных диапазонов.
    pub fn bounds(&self, s: (f32, f32), y: (f32, f32), d: (f32, f32)) -> (Vec3, Vec3) {
        let a = self.p(s.0, y.0, d.0);
        let b = self.p(s.1, y.1, d.1);
        (a.min(b), a.max(b))
    }
}

/// Заполнение стены над стрельчатой аркой (между дугой и прямоугольником до `top`),
/// плюс выступающий профиль-архивольт по дуге с обеих сторон стены.
pub fn arch_infill(mb: &mut MeshBuilder, trim: &mut MeshBuilder, f: WallFrame, sc: f32, span: f32, spring: f32, rise: f32, top: f32, d0: f32, d1: f32) {
    const N: usize = 16;
    let pts: Vec<(f32, f32)> = (0..=N)
        .map(|i| {
            let s = -span / 2.0 + span * i as f32 / N as f32;
            (s, spring + arch_height(s, span, rise))
        })
        .collect();
    let dm = (d0 + d1) / 2.0;
    for w in pts.windows(2) {
        let ((sa, ya), (sb, yb)) = (w[0], w[1]);
        for (d, sign) in [(d0, -1.0f32), (d1, 1.0)] {
            mb.quad([f.p(sc + sa, ya, d), f.p(sc + sb, yb, d), f.p(sc + sb, top, d), f.p(sc + sa, top, d)], f.d_axis() * sign);
        }
        // Внутренняя поверхность арки (смотрит вниз, к центру проёма).
        let hint = f.p(sc, spring, dm) - f.p(sc + (sa + sb) / 2.0, (ya + yb) / 2.0, dm);
        mb.quad([f.p(sc + sa, ya, d0), f.p(sc + sb, yb, d0), f.p(sc + sb, yb, d1), f.p(sc + sa, ya, d1)], hint);
    }
    // Архивольт.
    for d in [d0 - 0.06, d1 + 0.06] {
        let path: Vec<Vec3> = pts.iter().map(|&(s, y)| f.p(sc + s, y + 0.1, d)).collect();
        trim.beam_path(&path, 0.14, 0.12, f.d_axis());
    }
}

/// Контур окна-ланцета (прямоугольник со стрельчатым верхом) для стекла, с UV 0..1.
pub fn lancet_outline(f: WallFrame, sc: f32, d: f32, width: f32, sill: f32, spring: f32, rise: f32) -> (Vec<Vec3>, Vec<Vec2>) {
    const N: usize = 12;
    let apex = spring + arch_height(0.0, width, rise);
    let h = apex - sill;
    let mut pts = vec![(-width / 2.0, sill), (width / 2.0, sill)];
    for i in 0..=N {
        let s = width / 2.0 - width * i as f32 / N as f32;
        pts.push((s, spring + arch_height(s, width, rise)));
    }
    let world = pts.iter().map(|&(s, y)| f.p(sc + s, y, d)).collect();
    let uvs = pts.iter().map(|&(s, y)| Vec2::new(s / width + 0.5, 1.0 - (y - sill) / h)).collect();
    (world, uvs)
}

/// Крестовый стрельчатый свод над прямоугольной ячейкой, с нервюрами и замковым камнем.
/// Поверхность — верхняя огибающая двух пересекающихся стрельчатых цилиндров.
pub fn rib_vault(web: &mut MeshBuilder, ribs: &mut MeshBuilder, min: Vec2, max: Vec2, spring: f32, rise: f32) {
    const N: usize = 14;
    let (sx, sz) = (max.x - min.x, max.y - min.y);
    let c = (min + max) / 2.0;
    let height = |x: f32, z: f32| spring + arch_height(x - c.x, sx, rise).max(arch_height(z - c.y, sz, rise));
    let p = |i: usize, j: usize| {
        let x = min.x + sx * i as f32 / N as f32;
        let z = min.y + sz * j as f32 / N as f32;
        Vec3::new(x, height(x, z), z)
    };
    for i in 0..N {
        for j in 0..N {
            web.quad([p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1)], Vec3::NEG_Y);
        }
    }
    // Диагональные нервюры.
    const M: usize = 20;
    for (a, b) in [(min, max), (Vec2::new(min.x, max.y), Vec2::new(max.x, min.y))] {
        let path: Vec<Vec3> = (0..=M)
            .map(|k| {
                let t = k as f32 / M as f32;
                let q = a.lerp(b, t);
                Vec3::new(q.x, height(q.x, q.y) - 0.12, q.y)
            })
            .collect();
        ribs.beam_path(&path, 0.16, 0.16, Vec3::Y);
    }
    // Поперечные и стенные арки по краям ячейки.
    for z in [min.y, max.y] {
        let path: Vec<Vec3> = (0..=M).map(|k| {
            let x = min.x + sx * k as f32 / M as f32;
            Vec3::new(x, height(x, z) - 0.12, z)
        }).collect();
        ribs.beam_path(&path, 0.2, 0.18, Vec3::Z);
    }
    for x in [min.x, max.x] {
        let path: Vec<Vec3> = (0..=M).map(|k| {
            let z = min.y + sz * k as f32 / M as f32;
            Vec3::new(x, height(x, z) - 0.12, z)
        }).collect();
        ribs.beam_path(&path, 0.2, 0.18, Vec3::X);
    }
    // Замковый камень.
    let top = spring + rise;
    ribs.prism(Vec3::new(c.x, top - 0.45, c.y), 0.38, 0.4, 8);
}

/// Аркбутан: наклонный брус от вершины опоры (`pier`) к стене (`wall`),
/// под ним — четверть арки, которая начинается ниже на опоре и подходит к стене.
pub fn flying_buttress(mb: &mut MeshBuilder, pier: Vec3, wall: Vec3, half_w: f32) {
    const N: usize = 12;
    let top: Vec<Vec3> = (0..=N).map(|i| pier.lerp(wall, i as f32 / N as f32)).collect();
    mb.beam_path(&top, half_w, 0.25, Vec3::Y);
    let low_y = pier.y - 3.0;
    let end_y = wall.y - 0.5;
    let arc: Vec<Vec3> = (0..=N)
        .map(|i| {
            let t = i as f32 / N as f32;
            let mut p = pier.lerp(wall, t);
            p.y = low_y + (end_y - low_y) * (1.0 - (1.0 - t) * (1.0 - t)).sqrt();
            p
        })
        .collect();
    mb.beam_path(&arc, half_w, 0.22, Vec3::Y);
    // Стойки между аркой и брусом.
    for i in (2..N).step_by(3) {
        mb.beam(arc[i], top[i], half_w * 0.6, 0.08, (wall - pier).normalize());
    }
}
