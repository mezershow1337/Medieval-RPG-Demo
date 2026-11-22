//! Простая собственная физика персонажа: коробки (AABB) против статичных коробок уровня.
//! Используется и клиентом (движение), и сервером (проверка, что игрок не в стене).

use bevy::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

const EPS: f32 = 0.001;

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min: min.min(max), max: min.max(max) }
    }

    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min.x < other.max.x - EPS
            && self.max.x > other.min.x + EPS
            && self.min.y < other.max.y - EPS
            && self.max.y > other.min.y + EPS
            && self.min.z < other.max.z - EPS
            && self.max.z > other.min.z + EPS
    }
}

/// Все твёрдые коробки уровня.
#[derive(Resource, Default, Clone)]
pub struct LevelColliders(pub Vec<Aabb>);

/// Размеры тела персонажа. Позиция персонажа = точка у ног (центр по X/Z, низ по Y).
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub half_width: f32,
    pub height: f32,
}

impl Body {
    pub const STANDING: Body = Body { half_width: 0.3, height: 1.8 };
    pub const CROUCHING: Body = Body { half_width: 0.3, height: 1.2 };

    pub fn aabb_at(&self, feet: Vec3) -> Aabb {
        Aabb {
            min: feet - Vec3::new(self.half_width, 0.0, self.half_width),
            max: feet + Vec3::new(self.half_width, self.height, self.half_width),
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct MoveResult {
    pub grounded: bool,
    pub hit_ceiling: bool,
    pub blocked_x: bool,
    pub blocked_z: bool,
}

/// Максимальная высота ступеньки, на которую персонаж зашагивает сам.
pub const STEP_HEIGHT: f32 = 0.5;

impl LevelColliders {
    pub fn body_is_free(&self, body: &Body, feet: Vec3) -> bool {
        let a = body.aabb_at(feet);
        !self.0.iter().any(|c| c.overlaps(&a))
    }

    /// Двигает тело на `delta` с обработкой столкновений и автоматическим подъёмом на ступеньки.
    pub fn move_body(&self, body: &Body, feet: &mut Vec3, delta: Vec3, was_grounded: bool) -> MoveResult {
        let mut result = MoveResult::default();
        // Разбиваем движение на мелкие шаги, чтобы не пролетать сквозь тонкие стены.
        let max_component = delta.abs().max_element();
        let steps = ((max_component / 0.2).ceil() as usize).clamp(1, 64);
        let step = delta / steps as f32;
        let mut grounded = was_grounded;
        for _ in 0..steps {
            for axis in [0usize, 2] {
                if step[axis] == 0.0 {
                    continue;
                }
                if self.move_horizontal(body, feet, axis, step[axis], grounded) {
                    if axis == 0 {
                        result.blocked_x = true;
                    } else {
                        result.blocked_z = true;
                    }
                }
            }
            if step.y != 0.0 {
                let (ground, ceiling) = self.move_vertical(body, feet, step.y);
                grounded = ground;
                result.grounded |= ground;
                result.hit_ceiling |= ceiling;
            }
        }
        result
    }

    /// Возвращает true, если движение по оси упёрлось в стену.
    fn move_horizontal(&self, body: &Body, feet: &mut Vec3, axis: usize, d: f32, grounded: bool) -> bool {
        let mut moved = *feet;
        moved[axis] += d;
        if self.body_is_free(body, moved) {
            *feet = moved;
            return false;
        }
        // Пробуем зашагнуть на ступеньку.
        if grounded {
            let mut lifted = moved;
            lifted.y += STEP_HEIGHT;
            let mut start_lifted = *feet;
            start_lifted.y += STEP_HEIGHT;
            if self.body_is_free(body, start_lifted) && self.body_is_free(body, lifted) {
                // Опускаем на поверхность ступеньки.
                let (_g, _c) = self.move_vertical(body, &mut lifted, -STEP_HEIGHT);
                *feet = lifted;
                return false;
            }
        }
        // Упираемся вплотную в препятствие.
        let a = body.aabb_at(moved);
        let mut limit = moved[axis];
        for c in self.0.iter().filter(|c| c.overlaps(&a)) {
            if d > 0.0 {
                limit = limit.min(c.min[axis] - body.half_width - EPS);
            } else {
                limit = limit.max(c.max[axis] + body.half_width + EPS);
            }
        }
        // Не даём «отскочить» назад дальше исходной точки.
        let new = if d > 0.0 { limit.max(feet[axis]).min(moved[axis]) } else { limit.min(feet[axis]).max(moved[axis]) };
        let mut candidate = *feet;
        candidate[axis] = new;
        if self.body_is_free(body, candidate) {
            *feet = candidate;
        }
        true
    }

    /// Возвращает (стоит_на_земле, ударился_головой).
    fn move_vertical(&self, body: &Body, feet: &mut Vec3, d: f32) -> (bool, bool) {
        let mut moved = *feet;
        moved.y += d;
        let a = body.aabb_at(moved);
        let hits: Vec<&Aabb> = self.0.iter().filter(|c| c.overlaps(&a)).collect();
        if hits.is_empty() {
            *feet = moved;
            return (false, false);
        }
        if d < 0.0 {
            let top = hits.iter().map(|c| c.max.y).fold(f32::MIN, f32::max);
            feet.y = top.min(feet.y);
            (true, false)
        } else {
            let bottom = hits.iter().map(|c| c.min.y).fold(f32::MAX, f32::min);
            feet.y = (bottom - body.height - EPS).max(feet.y);
            (false, true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> LevelColliders {
        LevelColliders(vec![Aabb::new(Vec3::new(-50.0, -1.0, -50.0), Vec3::new(50.0, 0.0, 50.0))])
    }

    #[test]
    fn stands_on_floor() {
        let level = floor();
        let mut feet = Vec3::new(0.0, 1.0, 0.0);
        let mut grounded = false;
        for _ in 0..200 {
            grounded = level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.0, -0.05, 0.0), grounded).grounded;
        }
        assert!(grounded);
        assert!(feet.y.abs() < 0.01, "feet.y = {}", feet.y);
    }

    #[test]
    fn blocked_by_wall() {
        let mut level = floor();
        level.0.push(Aabb::new(Vec3::new(2.0, 0.0, -5.0), Vec3::new(3.0, 3.0, 5.0)));
        let mut feet = Vec3::ZERO;
        for _ in 0..100 {
            level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.01, 0.0), true);
        }
        assert!(feet.x <= 2.0 - 0.3 + 0.01 && feet.x > 1.5, "feet.x = {}", feet.x);
    }

    #[test]
    fn climbs_step() {
        let mut level = floor();
        level.0.push(Aabb::new(Vec3::new(2.0, 0.0, -5.0), Vec3::new(10.0, 0.4, 5.0)));
        let mut feet = Vec3::ZERO;
        let mut grounded = true;
        for _ in 0..100 {
            grounded = level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.05, 0.0), grounded).grounded;
        }
        assert!(feet.x > 5.0 && (feet.y - 0.4).abs() < 0.01, "feet = {feet}");
    }

    #[test]
    fn does_not_climb_high_wall() {
        let mut level = floor();
        level.0.push(Aabb::new(Vec3::new(2.0, 0.0, -5.0), Vec3::new(10.0, 1.0, 5.0)));
        let mut feet = Vec3::ZERO;
        let mut grounded = true;
        for _ in 0..100 {
            grounded = level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.05, 0.0), grounded).grounded;
        }
        assert!(feet.x < 2.0, "feet = {feet}");
    }
}
