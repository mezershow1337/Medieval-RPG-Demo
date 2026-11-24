//! Физика предметов: каждый предмет в мире — твёрдое тело-коробка со своей массой.
//!
//! Свой маленький движок (без сторонних библиотек), общий для сервера и клиента:
//! * тело — повёрнутая коробка по размерам 3D-модели (`icons::model_half_extents`) и массой
//!   по весу предмета (`weight.rs`), с тензором инерции коробки: длинное копьё крутится
//!   иначе, чем горшок;
//! * столкновения — точки на поверхности коробки (углы, рёбра, грани) против земли (карта высот),
//!   коробок уровня, прилавков ([`Obb`]) и людей, а также друг против друга;
//! * ответ — импульсы с трением и небольшой упругостью, подшаги (4 на кадр 60 Гц) против
//!   «пролетания», сон для лежащих тел (лежащие ничего не стоят);
//! * удержание в руках ([`Hold`]) — пружина с ограниченной силой: лёгкое летит за взглядом,
//!   тяжёлое отстаёт и проседает, а слишком тяжёлое (больше ~35 кг) поднять нельзя — только волочь.
//!
//! Сервер — единственный источник правды (`props_server.rs`), клиент рисует присланное
//! и предсказывает только свой предмет в руках (`props_client.rs`).

use crate::collision::{Aabb, LevelColliders};
use crate::items::ItemBaseId;
use bevy::math::{Mat3, Quat, Vec3};

pub const GRAVITY: f32 = 9.81;
/// Подшагов на кадр (кадр сервера — 1/60 с).
pub const SUBSTEPS: usize = 4;
pub const TICK: f32 = 1.0 / 60.0;
const MAX_SPEED: f32 = 14.0;
const MAX_SPIN: f32 = 30.0;
const SLEEP_SPEED: f32 = 0.06;
const SLEEP_SPIN: f32 = 0.25;
const SLEEP_AFTER: f32 = 0.45;
const SLOP: f32 = 0.002;
const FRICTION: f32 = 0.6;
const RESTITUTION: f32 = 0.18;
/// Самая тонкая сторона тела для физики (монета тоньше, но так она не проваливается).
const MIN_HALF: f32 = 0.008;

/// Дальность, с которой можно схватить предмет (м, от глаз до центра).
pub const GRAB_RANGE: f32 = 2.6;
/// Ближе и дальше этого держать нельзя (колесо мыши).
pub const HOLD_MIN: f32 = 0.7;
pub const HOLD_MAX: f32 = 2.2;
/// Сколько килограммов можно поднять (дальше — только волочь по земле).
pub const LIFT_KG: f32 = 30.0;

/// Повёрнутая коробка.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    pub center: Vec3,
    pub rot: Quat,
    pub half: Vec3,
}

impl Obb {
    /// Точка внутри: (нормаль наружу, глубина).
    pub fn penetration(&self, p: Vec3) -> Option<(Vec3, f32)> {
        let l = self.rot.inverse() * (p - self.center);
        let d = self.half - l.abs();
        if d.min_element() <= 0.0 {
            return None;
        }
        let axis = if d.x < d.y && d.x < d.z {
            0
        } else if d.y < d.z {
            1
        } else {
            2
        };
        let mut n = Vec3::ZERO;
        n[axis] = if l[axis] < 0.0 { -1.0 } else { 1.0 };
        Some((self.rot * n, d[axis]))
    }

    /// Луч: расстояние до входа в коробку.
    pub fn ray(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let inv = self.rot.inverse();
        let o = inv * (origin - self.center);
        let d = inv * dir;
        let (mut t0, mut t1) = (0.0f32, f32::MAX);
        for i in 0..3 {
            if d[i].abs() < 1e-8 {
                if o[i].abs() > self.half[i] {
                    return None;
                }
                continue;
            }
            let a = (-self.half[i] - o[i]) / d[i];
            let b = (self.half[i] - o[i]) / d[i];
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
            if t0 > t1 {
                return None;
            }
        }
        Some(t0)
    }

    /// Описанная коробка по осям мира.
    pub fn aabb(&self) -> Aabb {
        let m = Mat3::from_quat(self.rot);
        let e = Vec3::new(
            m.x_axis.x.abs() * self.half.x + m.y_axis.x.abs() * self.half.y + m.z_axis.x.abs() * self.half.z,
            m.x_axis.y.abs() * self.half.x + m.y_axis.y.abs() * self.half.y + m.z_axis.y.abs() * self.half.z,
            m.x_axis.z.abs() * self.half.x + m.y_axis.z.abs() * self.half.y + m.z_axis.z.abs() * self.half.z,
        );
        Aabb::new(self.center - e, self.center + e)
    }
}

/// Точка внутри коробки уровня: (нормаль наружу, глубина) по ближайшей грани.
fn aabb_penetration(b: &Aabb, p: Vec3) -> Option<(Vec3, f32)> {
    let lo = p - b.min;
    let hi = b.max - p;
    if lo.min_element() <= 0.0 || hi.min_element() <= 0.0 {
        return None;
    }
    let cands = [(lo.x, Vec3::NEG_X), (hi.x, Vec3::X), (lo.y, Vec3::NEG_Y), (hi.y, Vec3::Y), (lo.z, Vec3::NEG_Z), (hi.z, Vec3::Z)];
    let (d, n) = cands.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
    Some((n, d))
}

/// Кто-то из людей: предметы отталкиваются от тела и получают его скорость.
#[derive(Clone, Copy, Debug)]
pub struct Mover {
    pub aabb: Aabb,
    pub vel: Vec3,
}

/// Неподвижный мир для физики предметов.
pub struct Env<'a> {
    pub level: &'a LevelColliders,
    /// Прилавки и столы торговцев (их нет в коробках уровня).
    pub statics: &'a [Obb],
    pub movers: &'a [Mover],
}

/// Предмет в руках: куда тянуть и как повернуть.
#[derive(Clone, Copy, Debug)]
pub struct Hold {
    pub target: Vec3,
    pub target_rot: Quat,
    /// Предельная сила рук (Н).
    pub max_force: f32,
}

/// Сила рук: поднять `LIFT_KG` с запасом на рывок; сила с вещей добавляет по 0,4 кг.
pub fn hand_force(strength: f32) -> f32 {
    (LIFT_KG + strength.max(-20.0) * 0.4) * GRAVITY * 1.15
}

/// Мгновенная скорость броска (м/с) для массы: толчок ~9 Н·с, но не быстрее 11 м/с.
pub fn throw_speed(mass: f32, strength: f32) -> f32 {
    ((9.0 + strength.max(0.0) * 0.15) / mass.max(0.05)).min(11.0)
}

#[derive(Clone, Debug)]
pub struct PropBody {
    /// Центр коробки.
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// Угловая скорость (мир, рад/с).
    pub ang: Vec3,
    pub half: Vec3,
    pub mass: f32,
    inv_inertia: Vec3,
    pub asleep: bool,
    still: f32,
    /// Касается чего-нибудь (стоит, лежит, упирается).
    pub touching: bool,
    points: Vec<Vec3>,
}

/// Точки на поверхности коробки: углы, рёбра и грани через ~12 см.
fn surface_points(half: Vec3) -> Vec<Vec3> {
    let n = |len: f32, step: f32| ((len / step).ceil() as usize).clamp(1, 8);
    let (nx, ny, nz) = (n(half.x * 2.0, 0.12), n(half.y * 2.0, 0.12), n(half.z * 2.0, 0.12));
    let mut pts = Vec::new();
    for ix in 0..=nx {
        for iy in 0..=ny {
            for iz in 0..=nz {
                let on_x = ix == 0 || ix == nx;
                let on_y = iy == 0 || iy == ny;
                let on_z = iz == 0 || iz == nz;
                // Только поверхность (хотя бы одна координата на грани).
                if !(on_x || on_y || on_z) {
                    continue;
                }
                let f = |i: usize, n: usize| i as f32 / n as f32 * 2.0 - 1.0;
                pts.push(Vec3::new(f(ix, nx) * half.x, f(iy, ny) * half.y, f(iz, nz) * half.z));
            }
        }
    }
    pts
}

impl PropBody {
    pub fn new(half: Vec3, mass: f32, pos: Vec3, rot: Quat) -> Self {
        let half = half.max(Vec3::splat(MIN_HALF));
        let mass = mass.max(0.002);
        let s = half * 2.0;
        let i = Vec3::new(s.y * s.y + s.z * s.z, s.x * s.x + s.z * s.z, s.x * s.x + s.y * s.y) * (mass / 12.0);
        Self {
            pos,
            rot,
            vel: Vec3::ZERO,
            ang: Vec3::ZERO,
            half,
            mass,
            inv_inertia: Vec3::ONE / i,
            asleep: false,
            still: 0.0,
            touching: false,
            points: surface_points(half),
        }
    }

    /// Тело для предмета: размеры модели и вес одной штуки (стопка — тяжелее, но не больше).
    pub fn for_item(base: ItemBaseId, count: u16, pos: Vec3, rot: Quat) -> Self {
        let h = crate::icons::model_half_extents(base);
        let mass = crate::weight::unit_weight(base) * count.clamp(1, 20) as f32;
        Self::new(Vec3::from_array(h), mass, pos, rot)
    }

    /// Положение модели (её низ — в центре нижней грани коробки).
    pub fn model_offset(&self) -> Vec3 {
        Vec3::new(0.0, -self.half.y, 0.0)
    }

    pub fn obb(&self) -> Obb {
        Obb { center: self.pos, rot: self.rot, half: self.half }
    }

    pub fn wake(&mut self) {
        self.asleep = false;
        self.still = 0.0;
    }

    /// Положить и усыпить (на полке, после «поставил на место»).
    pub fn place(&mut self, pos: Vec3, rot: Quat) {
        self.pos = pos;
        self.rot = rot;
        self.vel = Vec3::ZERO;
        self.ang = Vec3::ZERO;
        self.asleep = true;
        self.still = 0.0;
    }

    fn inv_inertia_world(&self) -> Mat3 {
        let r = Mat3::from_quat(self.rot);
        r * Mat3::from_diagonal(self.inv_inertia) * r.transpose()
    }

    pub fn velocity_at(&self, p: Vec3) -> Vec3 {
        self.vel + self.ang.cross(p - self.pos)
    }

    /// «Мягкость» тела в точке вдоль направления (знаменатель импульса).
    fn effective(&self, iw: &Mat3, r: Vec3, n: Vec3) -> f32 {
        1.0 / self.mass + n.dot((*iw * r.cross(n)).cross(r))
    }

    fn apply_hold(&mut self, hold: &Hold, h: f32) {
        let to = hold.target - self.pos;
        let desired = (to * 12.0).clamp_length_max(7.0);
        let need = (desired - self.vel) / 0.06 + Vec3::Y * GRAVITY;
        let force = (need * self.mass).clamp_length_max(hold.max_force);
        self.vel += force / self.mass * h;
        // Поворот вслед за взглядом; тяжёлое поворачивается неохотно.
        let q = hold.target_rot * self.rot.inverse();
        let q = if q.w < 0.0 { -q } else { q };
        let (axis, angle) = q.to_axis_angle();
        let want = if angle.is_finite() && axis.is_finite() { axis * angle * 10.0 } else { Vec3::ZERO };
        let k = (hold.max_force / (self.mass * GRAVITY * 3.0)).clamp(0.05, 1.0);
        self.ang += (want - self.ang) * (h * 20.0 * k).min(1.0);
    }

    /// Один подшаг: силы, перемещение, столкновения с неподвижным миром.
    fn substep(&mut self, env: &Env, boxes: &[Aabb], hold: Option<&Hold>, h: f32) {
        self.vel.y -= GRAVITY * h;
        if let Some(hold) = hold {
            self.apply_hold(hold, h);
        }
        self.vel = self.vel.clamp_length_max(MAX_SPEED);
        self.ang = self.ang.clamp_length_max(MAX_SPIN);
        self.pos += self.vel * h;
        let spin = self.ang * h;
        if spin.length_squared() > 1e-12 {
            self.rot = (Quat::from_scaled_axis(spin) * self.rot).normalize();
        }

        // Точки, ушедшие в твёрдое.
        let mut contacts: Vec<(Vec3, Vec3, f32, Vec3)> = Vec::new();
        for lp in &self.points {
            let p = self.pos + self.rot * *lp;
            if let Some(c) = static_contact(env, boxes, p) {
                contacts.push((p, c.0, c.1, c.2));
            }
        }
        self.touching = !contacts.is_empty();
        if contacts.is_empty() {
            return;
        }
        // Выталкиваем по самой глубокой точке.
        let (_, n, depth, _) = contacts.iter().copied().max_by(|a, b| a.2.total_cmp(&b.2)).unwrap();
        if depth > SLOP {
            self.pos += n * (depth - SLOP) * 0.8;
        }
        // Импульсы: дважды по всем точкам (последовательно, как в большинстве движков).
        let iw = self.inv_inertia_world();
        for _ in 0..2 {
            for (p, n, depth, surf) in &contacts {
                let r = *p - self.pos;
                let v = self.velocity_at(*p) - *surf;
                let vn = v.dot(*n);
                let bias = ((depth - SLOP).max(0.0) * 0.15 / h).min(0.4);
                if vn >= bias {
                    continue;
                }
                let e = if vn < -1.2 { RESTITUTION } else { 0.0 };
                let jn = (-(1.0 + e) * vn + bias) / self.effective(&iw, r, *n);
                let jn = jn.max(0.0);
                self.vel += *n * jn / self.mass;
                self.ang += iw * r.cross(*n * jn);
                // Трение.
                let v = self.velocity_at(*p) - *surf;
                let vt = v - *n * v.dot(*n);
                let speed = vt.length();
                if speed > 1e-5 {
                    let t = vt / speed;
                    let jt = (speed / self.effective(&iw, r, t)).min(FRICTION * jn);
                    self.vel -= t * jt / self.mass;
                    self.ang -= iw * r.cross(t * jt);
                }
            }
        }
        // Трение качения: лежащее не крутится бесконечно.
        self.ang *= 1.0 - (3.0 * h).min(0.5);
    }

    /// Один кадр одиночного тела (без других предметов). Возвращает true, если тело уснуло.
    pub fn step(&mut self, env: &Env, hold: Option<&Hold>, dt: f32) -> bool {
        if self.asleep && hold.is_none() {
            return false;
        }
        self.asleep = false;
        let boxes = candidate_boxes(env.level, self, dt);
        let h = dt / SUBSTEPS as f32;
        for _ in 0..SUBSTEPS {
            self.substep(env, &boxes, hold, h);
        }
        self.update_sleep(hold.is_some(), dt)
    }

    fn update_sleep(&mut self, held: bool, dt: f32) -> bool {
        if !held && self.touching && self.vel.length() < SLEEP_SPEED && self.ang.length() < SLEEP_SPIN {
            self.still += dt;
            if self.still > SLEEP_AFTER {
                self.vel = Vec3::ZERO;
                self.ang = Vec3::ZERO;
                self.asleep = true;
                return true;
            }
        } else {
            self.still = 0.0;
        }
        false
    }
}

/// Коробки уровня, которых тело может коснуться за кадр.
fn candidate_boxes(level: &LevelColliders, b: &PropBody, dt: f32) -> Vec<Aabb> {
    let a = b.obb().aabb();
    let grow = Vec3::splat(b.vel.length() * dt + 0.05);
    level.boxes_touching(Aabb::new(a.min - grow, a.max + grow)).copied().collect()
}

/// Самая глубокая точка касания с неподвижным миром: (нормаль, глубина, скорость поверхности).
fn static_contact(env: &Env, boxes: &[Aabb], p: Vec3) -> Option<(Vec3, f32, Vec3)> {
    let mut best: Option<(Vec3, f32, Vec3)> = None;
    let mut take = |n: Vec3, d: f32, v: Vec3| {
        if best.is_none_or(|b| d > b.1) {
            best = Some((n, d, v));
        }
    };
    let g = env.level.ground_height(p.x, p.z);
    if p.y < g {
        let e = 0.15;
        let gx = env.level.ground_height(p.x + e, p.z) - env.level.ground_height(p.x - e, p.z);
        let gz = env.level.ground_height(p.x, p.z + e) - env.level.ground_height(p.x, p.z - e);
        let n = if gx.is_finite() && gz.is_finite() { Vec3::new(-gx, 2.0 * e, -gz).normalize() } else { Vec3::Y };
        take(n, (g - p.y) * n.y, Vec3::ZERO);
    }
    for b in boxes {
        if let Some((n, d)) = aabb_penetration(b, p) {
            take(n, d, Vec3::ZERO);
        }
    }
    for o in env.statics {
        if let Some((n, d)) = o.penetration(p) {
            take(n, d, Vec3::ZERO);
        }
    }
    for m in env.movers {
        if let Some((mut n, d)) = aabb_penetration(&m.aabb, p) {
            // Человек отталкивает в сторону, а не вверх-вниз (иначе встаёт на предмет).
            if n.y.abs() > 0.5 {
                let c = (m.aabb.min + m.aabb.max) * 0.5;
                n = (p - c).with_y(0.0).normalize_or(Vec3::X);
            }
            take(n, d.min(0.05), m.vel);
        }
    }
    best
}

/// Столкновение двух тел (хотя бы одно не спит). Возвращает true, если было касание.
pub fn collide_pair(a: &mut PropBody, b: &mut PropBody, h: f32) -> bool {
    let d = a.pos - b.pos;
    let r = a.half.length() + b.half.length();
    if d.length_squared() > r * r {
        return false;
    }
    let mut hit = false;
    hit |= points_into(a, b, h);
    hit |= points_into(b, a, h);
    if hit {
        a.touching = true;
        b.touching = true;
    }
    hit
}

/// Точки `a` внутри `b`: толкаем оба тела.
fn points_into(a: &mut PropBody, b: &mut PropBody, h: f32) -> bool {
    let ob = b.obb();
    let contacts: Vec<(Vec3, Vec3, f32)> = a.points.iter().filter_map(|lp| {
        let p = a.pos + a.rot * *lp;
        ob.penetration(p).map(|(n, d)| (p, n, d))
    }).collect();
    if contacts.is_empty() {
        return false;
    }
    if a.asleep || b.asleep {
        a.wake();
        b.wake();
    }
    let (_, n, depth) = contacts.iter().copied().max_by(|x, y| x.2.total_cmp(&y.2)).unwrap();
    if depth > SLOP {
        let total = a.mass + b.mass;
        let push = n * (depth - SLOP) * 0.6;
        a.pos += push * (b.mass / total);
        b.pos -= push * (a.mass / total);
    }
    let (ia, ib) = (a.inv_inertia_world(), b.inv_inertia_world());
    for (p, n, _) in &contacts {
        let (ra, rb) = (*p - a.pos, *p - b.pos);
        let v = a.velocity_at(*p) - b.velocity_at(*p);
        let vn = v.dot(*n);
        if vn >= 0.0 {
            continue;
        }
        let k = a.effective(&ia, ra, *n) + b.effective(&ib, rb, *n);
        let e = if vn < -1.2 { RESTITUTION } else { 0.0 };
        let jn = -(1.0 + e) * vn / k;
        a.vel += *n * jn / a.mass;
        a.ang += ia * ra.cross(*n * jn);
        b.vel -= *n * jn / b.mass;
        b.ang -= ib * rb.cross(*n * jn);
        let v = a.velocity_at(*p) - b.velocity_at(*p);
        let vt = v - *n * v.dot(*n);
        let speed = vt.length();
        if speed > 1e-5 {
            let t = vt / speed;
            let kt = a.effective(&ia, ra, t) + b.effective(&ib, rb, t);
            let jt = (speed / kt).min(FRICTION * jn);
            a.vel -= t * jt / a.mass;
            a.ang -= ia * ra.cross(t * jt);
            b.vel += t * jt / b.mass;
            b.ang += ib * rb.cross(t * jt);
        }
    }
    let _ = h;
    true
}

/// Кадр для набора тел: каждое — со своим удержанием (или без), плюс столкновения между ними.
/// Возвращает номера тел, которые уснули в этом кадре.
pub fn step_bodies(bodies: &mut [(&mut PropBody, Option<Hold>)], env: &Env, dt: f32) -> Vec<usize> {
    let awake: Vec<usize> = (0..bodies.len()).filter(|i| !bodies[*i].0.asleep || bodies[*i].1.is_some()).collect();
    if awake.is_empty() {
        return Vec::new();
    }
    let boxes: Vec<Vec<Aabb>> = awake.iter().map(|i| candidate_boxes(env.level, bodies[*i].0, dt)).collect();
    for i in &awake {
        bodies[*i].0.asleep = false;
    }
    // Соседи каждого бодрствующего: в домах тысячи спящих вещей, перебирать все на каждом
    // подшаге дорого. За кадр тело не уходит дальше запаса.
    let mut is_awake = vec![false; bodies.len()];
    for i in &awake {
        is_awake[*i] = true;
    }
    let near: Vec<Vec<usize>> = awake
        .iter()
        .map(|i| {
            let a = &bodies[*i].0;
            let reach = a.half.length() + a.vel.length() * dt + 0.5;
            (0..bodies.len())
                .filter(|j| {
                    let b = &bodies[*j].0;
                    let r = reach + b.half.length() + b.vel.length() * dt;
                    a.pos.distance_squared(b.pos) <= r * r
                })
                .collect()
        })
        .collect();
    let h = dt / SUBSTEPS as f32;
    for _ in 0..SUBSTEPS {
        for (k, i) in awake.iter().enumerate() {
            let (body, hold) = &mut bodies[*i];
            if !body.asleep {
                body.substep(env, &boxes[k], hold.as_ref(), h);
            }
        }
        // Пары: бодрствующее с любым соседним.
        for (k, i) in awake.iter().enumerate() {
            for &j in &near[k] {
                if j == *i || (j < *i && is_awake[j]) {
                    continue;
                }
                let (lo, hi) = if *i < j { (*i, j) } else { (j, *i) };
                let (left, right) = bodies.split_at_mut(hi);
                collide_pair(left[lo].0, right[0].0, h);
            }
        }
    }
    let mut slept = Vec::new();
    for (i, (body, hold)) in bodies.iter_mut().enumerate() {
        // Разбуженные соседями в этом кадре тоже проверяем.
        if !body.asleep && body.update_sleep(hold.is_some(), dt) {
            slept.push(i);
        }
    }
    slept
}

/// Куда тянуть предмет в руках: точка перед глазами.
pub fn hold_point(eye: Vec3, yaw: f32, pitch: f32, dist: f32) -> Vec3 {
    eye + Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Vec3::NEG_Z * dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::LevelColliders;

    fn floor() -> LevelColliders {
        LevelColliders::new(vec![Aabb::new(Vec3::new(-50.0, -1.0, -50.0), Vec3::new(50.0, 0.0, 50.0))], None)
    }

    fn run(body: &mut PropBody, level: &LevelColliders, statics: &[Obb], secs: f32) {
        let env = Env { level, statics, movers: &[] };
        for _ in 0..(secs / TICK) as usize {
            body.step(&env, None, TICK);
        }
    }

    #[test]
    fn falls_and_settles_flat() {
        let level = floor();
        let half = Vec3::new(0.05, 0.02, 0.45);
        let mut b = PropBody::new(half, 1.1, Vec3::new(0.0, 1.5, 0.0), Quat::from_rotation_x(0.6) * Quat::from_rotation_z(0.4));
        run(&mut b, &level, &[], 6.0);
        assert!(b.asleep, "тело должно уснуть: v={} w={}", b.vel, b.ang);
        // Плоская спата ложится плашмя: центр на половине толщины над полом.
        assert!((b.pos.y - half.y).abs() < 0.02, "высота {}", b.pos.y);
        let up = b.rot * Vec3::Y;
        assert!(up.y.abs() > 0.95, "должна лежать плашмя, up={up}");
    }

    #[test]
    fn lands_on_counter_and_on_slope() {
        let level = floor();
        let counter = Obb { center: Vec3::new(0.0, 0.45, 0.0), rot: Quat::from_rotation_y(0.7), half: Vec3::new(1.0, 0.45, 0.35) };
        let mut b = PropBody::new(Vec3::splat(0.06), 0.8, Vec3::new(0.1, 1.4, 0.05), Quat::IDENTITY);
        run(&mut b, &level, &[counter], 4.0);
        assert!(b.asleep);
        assert!((b.pos.y - 0.96).abs() < 0.02, "на прилавке: {}", b.pos.y);
    }

    #[test]
    fn thrown_item_does_not_pass_through_wall() {
        let mut boxes = floor().0;
        boxes.push(Aabb::new(Vec3::new(3.0, 0.0, -5.0), Vec3::new(3.25, 4.0, 5.0)));
        let level = LevelColliders::new(boxes, None);
        let mut b = PropBody::new(Vec3::new(0.0045, 0.006, 0.004), 0.009, Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY);
        b.vel = Vec3::new(13.0, 1.0, 0.0);
        run(&mut b, &level, &[], 4.0);
        assert!(b.pos.x < 3.0, "монета пролетела сквозь стену: {}", b.pos);
        assert!(b.pos.y > -0.05);
    }

    #[test]
    fn stack_rests_and_heavy_cannot_be_lifted() {
        let level = floor();
        let env = Env { level: &level, statics: &[], movers: &[] };
        let mut a = PropBody::new(Vec3::new(0.2, 0.1, 0.2), 5.0, Vec3::new(0.0, 0.1, 0.0), Quat::IDENTITY);
        let mut b = PropBody::new(Vec3::new(0.1, 0.05, 0.1), 1.0, Vec3::new(0.02, 0.5, 0.0), Quat::IDENTITY);
        for _ in 0..240 {
            let mut list = [(&mut a, None), (&mut b, None)];
            step_bodies(&mut list, &env, TICK);
        }
        assert!(b.pos.y > 0.2 && b.pos.y < 0.3, "лёгкое лежит на тяжёлом: {}", b.pos.y);

        // Лёгкое поднимается к точке перед глазами, амфору 40 кг не оторвать от земли.
        let eye_target = Vec3::new(0.0, 1.3, -1.0);
        let hold = Hold { target: eye_target, target_rot: Quat::IDENTITY, max_force: hand_force(0.0) };
        let mut light = PropBody::new(Vec3::splat(0.08), 1.0, Vec3::new(0.0, 0.08, -1.0), Quat::IDENTITY);
        let mut heavy = PropBody::new(Vec3::new(0.15, 0.3, 0.15), 40.0, Vec3::new(2.0, 0.3, -1.0), Quat::IDENTITY);
        let heavy_hold = Hold { target: Vec3::new(2.0, 1.3, -1.0), ..hold };
        for _ in 0..120 {
            light.step(&env, Some(&hold), TICK);
            heavy.step(&env, Some(&heavy_hold), TICK);
        }
        assert!(light.pos.distance(eye_target) < 0.1, "лёгкое в руке: {}", light.pos);
        assert!(heavy.pos.y < 0.45, "40 кг не поднять: {}", heavy.pos.y);
        // А тянуть по земле можно.
        let drag = Hold { target: Vec3::new(3.0, 0.3, -1.0), ..hold };
        let x0 = heavy.pos.x;
        for _ in 0..120 {
            heavy.step(&env, Some(&drag), TICK);
        }
        assert!(heavy.pos.x > x0 + 0.3, "волочится: {} -> {}", x0, heavy.pos.x);
    }

    #[test]
    fn ray_hits_rotated_box() {
        let o = Obb { center: Vec3::new(0.0, 0.0, -2.0), rot: Quat::from_rotation_y(0.8), half: Vec3::new(0.5, 0.1, 0.05) };
        assert!(o.ray(Vec3::ZERO, Vec3::NEG_Z).is_some());
        assert!(o.ray(Vec3::new(0.0, 0.5, 0.0), Vec3::NEG_Z).is_none());
        assert!(o.penetration(Vec3::new(0.0, 0.0, -2.0)).is_some());
    }

    #[test]
    fn every_item_has_a_body() {
        for i in 0..crate::items::ITEM_BASES.len() {
            let b = PropBody::for_item(ItemBaseId(i as u16), 1, Vec3::ZERO, Quat::IDENTITY);
            assert!(b.mass > 0.0 && b.half.min_element() >= MIN_HALF && b.half.max_element() < 2.0, "{}", crate::items::ITEM_BASES[i].name);
        }
    }
}
