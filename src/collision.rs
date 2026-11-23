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

/// Карта высот земли (холмы за стенами). Сетка с билинейной интерполяцией.
/// Всё, что ниже поверхности, — твёрдое.
#[derive(Clone, Debug)]
pub struct HeightField {
    /// Мировые X/Z угла сетки (индекс 0, 0).
    pub origin: Vec2,
    pub cell: f32,
    pub nx: usize,
    pub nz: usize,
    /// Высоты узлов, строка за строкой по Z: `data[iz * nx + ix]`.
    pub data: Vec<f32>,
    /// Прямоугольники (min, max по X/Z), где земли нет: там уровень строит свой пол из коробок.
    pub holes: Vec<(Vec2, Vec2)>,
}

impl HeightField {
    pub fn node(&self, ix: usize, iz: usize) -> f32 {
        self.data[iz.min(self.nz - 1) * self.nx + ix.min(self.nx - 1)]
    }

    pub fn in_hole(&self, x: f32, z: f32) -> bool {
        self.holes.iter().any(|(a, b)| x > a.x && x < b.x && z > a.y && z < b.y)
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        if self.in_hole(x, z) {
            return f32::NEG_INFINITY;
        }
        let fx = ((x - self.origin.x) / self.cell).clamp(0.0, (self.nx - 1) as f32 - 0.001);
        let fz = ((z - self.origin.y) / self.cell).clamp(0.0, (self.nz - 1) as f32 - 0.001);
        let (ix, iz) = (fx as usize, fz as usize);
        let (tx, tz) = (fx - ix as f32, fz - iz as f32);
        let a = self.node(ix, iz) * (1.0 - tx) + self.node(ix + 1, iz) * tx;
        let b = self.node(ix, iz + 1) * (1.0 - tx) + self.node(ix + 1, iz + 1) * tx;
        a * (1.0 - tz) + b * tz
    }
}

/// Сетка-ускоритель: какие коробки лежат в каждой клетке 8×8 м.
#[derive(Clone, Default)]
struct Grid {
    origin: Vec2,
    nx: i32,
    nz: i32,
    cells: Vec<Vec<u32>>,
    /// Очень большие коробки (пол на весь мир) проверяются всегда.
    big: Vec<u32>,
}

const GRID_CELL: f32 = 8.0;

impl Grid {
    fn build(boxes: &[Aabb]) -> Self {
        let mut min = Vec2::splat(f32::MAX);
        let mut max = Vec2::splat(f32::MIN);
        let is_big = |b: &Aabb| (b.max.x - b.min.x) * (b.max.z - b.min.z) > 64.0 * 64.0;
        for b in boxes.iter().filter(|b| !is_big(b)) {
            min = min.min(Vec2::new(b.min.x, b.min.z));
            max = max.max(Vec2::new(b.max.x, b.max.z));
        }
        if min.x > max.x {
            return Grid { big: (0..boxes.len() as u32).collect(), ..default() };
        }
        let nx = ((max.x - min.x) / GRID_CELL).ceil() as i32 + 1;
        let nz = ((max.y - min.y) / GRID_CELL).ceil() as i32 + 1;
        let mut grid = Grid { origin: min, nx, nz, cells: vec![Vec::new(); (nx * nz) as usize], big: Vec::new() };
        for (i, b) in boxes.iter().enumerate() {
            if is_big(b) {
                grid.big.push(i as u32);
                continue;
            }
            let (x0, z0, x1, z1) = grid.range(b);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    grid.cells[(z * nx + x) as usize].push(i as u32);
                }
            }
        }
        grid
    }

    fn range(&self, b: &Aabb) -> (i32, i32, i32, i32) {
        let c = |v: f32, o: f32, n: i32| (((v - o) / GRID_CELL).floor() as i32).clamp(0, n - 1);
        (c(b.min.x, self.origin.x, self.nx), c(b.min.z, self.origin.y, self.nz), c(b.max.x, self.origin.x, self.nx), c(b.max.z, self.origin.y, self.nz))
    }
}

/// Все твёрдые коробки уровня (поле `.0`) плюс ускоритель поиска и карта высот.
/// Создавать через [`LevelColliders::new`]: после создания коробки не меняются.
#[derive(Resource, Default, Clone)]
pub struct LevelColliders(pub Vec<Aabb>, Accel);

#[derive(Clone, Default)]
struct Accel {
    grid: Grid,
    ground: Option<std::sync::Arc<HeightField>>,
    /// Двери: их коробки лежат в конце `.0` (с индекса `first`), открытая дверь не мешает.
    /// Флаги общие для всех копий ресурса и меняются без `&mut` (сервер и клиент держат
    /// по своему `LevelColliders`, состояние приходит по сети).
    doors: std::sync::Arc<DoorFlags>,
}

#[derive(Default)]
struct DoorFlags {
    first: usize,
    open: Vec<std::sync::atomic::AtomicBool>,
}

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
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(boxes: Vec<Aabb>, ground: Option<HeightField>) -> Self {
        Self::with_doors(boxes, ground, Vec::new())
    }

    /// Коробки уровня плюс двери (коробка закрытой створки и открыта ли она сначала).
    /// Номер двери — её индекс в `doors`.
    pub fn with_doors(mut boxes: Vec<Aabb>, ground: Option<HeightField>, doors: Vec<(Aabb, bool)>) -> Self {
        let first = boxes.len();
        let open = doors.iter().map(|(_, o)| std::sync::atomic::AtomicBool::new(*o)).collect();
        boxes.extend(doors.iter().map(|(a, _)| *a));
        let grid = Grid::build(&boxes);
        LevelColliders(boxes, Accel { grid, ground: ground.map(std::sync::Arc::new), doors: std::sync::Arc::new(DoorFlags { first, open }) })
    }

    pub fn door_count(&self) -> usize {
        self.1.doors.open.len()
    }

    pub fn door_open(&self, id: usize) -> bool {
        self.1.doors.open.get(id).is_some_and(|f| f.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn set_door_open(&self, id: usize, open: bool) {
        if let Some(f) = self.1.doors.open.get(id) {
            f.store(open, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Коробка закрытой двери.
    pub fn door_box(&self, id: usize) -> Option<Aabb> {
        (id < self.door_count()).then(|| self.0[self.1.doors.first + id])
    }

    /// Открывает или закрывает все двери сразу (для проверок проходимости).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_all_doors(&self, open: bool) {
        for id in 0..self.door_count() {
            self.set_door_open(id, open);
        }
    }

    /// Высота земли под точкой (минус бесконечность, если карты высот нет).
    pub fn ground_height(&self, x: f32, z: f32) -> f32 {
        self.1.ground.as_ref().map_or(f32::NEG_INFINITY, |g| g.height(x, z))
    }

    /// Коробки, которые могут пересекаться с `a`.
    fn near<'a>(&'a self, a: Aabb) -> impl Iterator<Item = &'a Aabb> + 'a {
        let g = &self.1.grid;
        let (x0, z0, x1, z1) = if g.cells.is_empty() { (0, 0, -1, -1) } else { g.range(&a) };
        let cells = (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| (z * g.nx + x) as usize));
        let doors = &self.1.doors;
        g.big
            .iter()
            .chain(cells.flat_map(move |c| g.cells[c].iter()))
            .filter(move |&&i| {
                let i = i as usize;
                i < doors.first || !doors.open.get(i - doors.first).is_some_and(|f| f.load(std::sync::atomic::Ordering::Relaxed))
            })
            .map(move |&i| &self.0[i as usize])
    }

    /// Коробки, в которые может попасть точка (быстрее, чем перебор всего `.0`).
    pub fn boxes_near(&self, p: Vec3) -> impl Iterator<Item = &Aabb> + '_ {
        let a = Aabb { min: p, max: p };
        self.near(a).filter(move |c| p.cmpgt(c.min).all() && p.cmplt(c.max).all())
    }

    /// Точка внутри твёрдого (стены или земли)? Для проверки линии видимости.
    pub fn point_solid(&self, p: Vec3) -> bool {
        if p.y < self.ground_height(p.x, p.z) {
            return true;
        }
        let a = Aabb { min: p, max: p };
        self.near(a).any(|c| p.cmpgt(c.min).all() && p.cmplt(c.max).all())
    }

    pub fn body_is_free(&self, body: &Body, feet: Vec3) -> bool {
        if feet.y < self.ground_height(feet.x, feet.z) - 0.01 {
            return false;
        }
        let a = body.aabb_at(feet);
        !self.near(a).any(|c| c.overlaps(&a))
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
            // По склону вниз: прижимаем к земле, чтобы не «подпрыгивать» на каждом шаге.
            if grounded && step.y <= 0.0 && self.1.ground.is_some() {
                let g = self.ground_height(feet.x, feet.z);
                if feet.y > g && feet.y - g < STEP_HEIGHT * 0.8 {
                    let snapped = Vec3::new(feet.x, g, feet.z);
                    // Если под ногами коробка (стоим на ступеньке), тело на уровне земли в неё упрётся.
                    if self.body_is_free(body, snapped) {
                        feet.y = g;
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
        for c in self.near(a).filter(|c| c.overlaps(&a)) {
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
        let hits: Vec<&Aabb> = self.near(a).filter(|c| c.overlaps(&a)).collect();
        let ground = self.ground_height(moved.x, moved.z);
        if hits.is_empty() && moved.y >= ground {
            *feet = moved;
            return (false, false);
        }
        if d < 0.0 {
            let top = hits.iter().map(|c| c.max.y).fold(ground, f32::max);
            feet.y = top.min(feet.y);
            (true, false)
        } else {
            if hits.is_empty() {
                // Под землёй и движемся вверх: просто выталкиваем на поверхность.
                feet.y = ground;
                return (true, false);
            }
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
        with(vec![])
    }

    fn with(extra: Vec<Aabb>) -> LevelColliders {
        let mut boxes = vec![Aabb::new(Vec3::new(-50.0, -1.0, -50.0), Vec3::new(50.0, 0.0, 50.0))];
        boxes.extend(extra);
        LevelColliders::new(boxes, None)
    }

    #[test]
    fn doors_block_only_when_closed() {
        let mut boxes = vec![Aabb::new(Vec3::new(-50.0, -1.0, -50.0), Vec3::new(50.0, 0.0, 50.0))];
        boxes.push(Aabb::new(Vec3::new(2.0, 0.0, -5.0), Vec3::new(2.2, 3.0, -0.6)));
        boxes.push(Aabb::new(Vec3::new(2.0, 0.0, 0.6), Vec3::new(2.2, 3.0, 5.0)));
        let level = LevelColliders::with_doors(boxes, None, vec![(Aabb::new(Vec3::new(2.05, 0.0, -0.6), Vec3::new(2.15, 2.2, 0.6)), false)]);
        let copy = level.clone();
        let walk = |level: &LevelColliders| {
            let mut feet = Vec3::ZERO;
            for _ in 0..100 {
                level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.01, 0.0), true);
            }
            feet.x
        };
        assert!(walk(&level) < 2.0);
        copy.set_door_open(0, true);
        assert!(level.door_open(0), "флаги общие у копий");
        assert!(walk(&level) > 5.0);
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
        let level = with(vec![Aabb::new(Vec3::new(2.0, 0.0, -5.0), Vec3::new(3.0, 3.0, 5.0))]);
        let mut feet = Vec3::ZERO;
        for _ in 0..100 {
            level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.01, 0.0), true);
        }
        assert!(feet.x <= 2.0 - 0.3 + 0.01 && feet.x > 1.5, "feet.x = {}", feet.x);
    }

    #[test]
    fn climbs_step() {
        let level = with(vec![Aabb::new(Vec3::new(2.0, 0.0, -5.0), Vec3::new(10.0, 0.4, 5.0))]);
        let mut feet = Vec3::ZERO;
        let mut grounded = true;
        for _ in 0..100 {
            grounded = level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.05, 0.0), grounded).grounded;
        }
        assert!(feet.x > 5.0 && (feet.y - 0.4).abs() < 0.01, "feet = {feet}");
    }

    #[test]
    fn does_not_climb_high_wall() {
        let level = with(vec![Aabb::new(Vec3::new(2.0, 0.0, -5.0), Vec3::new(10.0, 1.0, 5.0))]);
        let mut feet = Vec3::ZERO;
        let mut grounded = true;
        for _ in 0..100 {
            grounded = level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.05, 0.0), grounded).grounded;
        }
        assert!(feet.x < 2.0, "feet = {feet}");
    }

    #[test]
    fn walks_up_and_down_a_hill() {
        // Склон 1:4 вверх по X до высоты 8, потом вниз.
        let nx = 41;
        let data: Vec<f32> = (0..nx * nx).map(|i| {
            let x = (i % nx) as f32 * 2.0 - 40.0;
            (8.0 - x.abs() * 0.25).max(0.0)
        }).collect();
        let hf = HeightField { origin: Vec2::splat(-40.0), cell: 2.0, nx, nz: nx, data, holes: vec![] };
        let level = LevelColliders::new(vec![], Some(hf));
        let mut feet = Vec3::new(-35.0, 0.0, 0.0);
        let mut grounded = true;
        let mut max_y: f32 = 0.0;
        for _ in 0..700 {
            grounded = level.move_body(&Body::STANDING, &mut feet, Vec3::new(0.1, -0.05, 0.0), grounded).grounded;
            max_y = max_y.max(feet.y);
            assert!(feet.y >= level.ground_height(feet.x, feet.z) - 0.02, "под землёй: {feet}");
        }
        assert!(feet.x > 30.0, "не прошёл через холм: {feet}");
        assert!(max_y > 7.0, "не поднялся на холм: {max_y}");
        assert!(grounded && feet.y < 1.5, "не спустился: {feet}");
        assert!(level.point_solid(Vec3::new(0.0, 5.0, 0.0)));
        assert!(!level.point_solid(Vec3::new(0.0, 9.0, 0.0)));
    }
}
