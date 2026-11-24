//! По чему ступает нога и есть ли над головой крыша — для шагов и окружения.
//!
//! Карту строим из раскладки уровня в отдельном потоке (уровень большой, а звук не должен
//! задерживать запуск); пока она не готова, шаги звучат как по утоптанной земле.

use crate::collision::HeightField;
use crate::level::Mat;
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Чем звучит шаг.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    Stone,
    Wood,
    Dirt,
    Grass,
    Cloth,
    Tile,
    Water,
}

impl Surface {
    pub fn step_sound(self) -> &'static str {
        match self {
            Surface::Stone => "step_stone",
            Surface::Wood => "step_wood",
            Surface::Dirt => "step_dirt",
            Surface::Grass => "step_grass",
            Surface::Cloth => "step_cloth",
            Surface::Tile => "step_tile",
            Surface::Water => "step_water",
        }
    }

    pub fn of(mat: Mat) -> Option<Surface> {
        use Mat::*;
        Some(match mat {
            Cobble | Stone | DarkStone | Floor | Trim | Paving | Limestone | Rubble | Plaster
            | Brick | OpusMixtum | Marble | Mosaic | Signinum | Gold | Iron | Bronze | Coals
            | Ochre | Glass | Bedrock | Limonite | Shingle => Surface::Stone,
            Wood | Bread => Surface::Wood,
            Earth | Soot => Surface::Dirt,
            Grass | Terrain | Leaves | Conifer | HerbGreen | HerbSilver | PetalYellow
            | PetalPurple | PetalWhite | PetalRed | PetalPink | Thatch => Surface::Grass,
            Rug | Cloth | Felt | Canvas | Banner | Leather => Surface::Cloth,
            Tile | Roof | Ceramic => Surface::Tile,
            Water => Surface::Water,
        })
    }
}

const CELL: f32 = 4.0;

/// Верх твёрдой коробки: по нему можно идти.
#[derive(Clone, Copy)]
struct Top {
    min: Vec2,
    max: Vec2,
    y: f32,
    surface: Surface,
}

/// Треугольник крыши в плане (для «под крышей ли мы»).
#[derive(Clone, Copy)]
struct RoofTri([Vec3; 3]);

pub struct SurfaceMap {
    tops: HashMap<(i32, i32), Vec<Top>>,
    roofs: HashMap<(i32, i32), Vec<RoofTri>>,
    /// Плиты и навесы над головой (потолки, своды) — тоже крыша.
    ceilings: HashMap<(i32, i32), Vec<Top>>,
    ground: Option<HeightField>,
    pub water_y: Option<f32>,
    /// Очаги и горны (треск огня).
    pub fires: Vec<Vec3>,
}

fn cell(x: f32, z: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

fn cells(min: Vec2, max: Vec2) -> impl Iterator<Item = (i32, i32)> {
    let (a, b) = (cell(min.x, min.y), cell(max.x, max.y));
    (a.0..=b.0).flat_map(move |i| (a.1..=b.1).map(move |j| (i, j)))
}

impl SurfaceMap {
    pub fn build(layout: &crate::level::Layout) -> Self {
        let mut tops: HashMap<(i32, i32), Vec<Top>> = HashMap::new();
        let mut ceilings: HashMap<(i32, i32), Vec<Top>> = HashMap::new();
        for b in &layout.blocks {
            let Some(surface) = Surface::of(b.mat) else {
                continue;
            };
            let (min, max) = (
                Vec2::new(b.aabb.min.x, b.aabb.min.z),
                Vec2::new(b.aabb.max.x, b.aabb.max.z),
            );
            let size = max - min;
            // Гигантские плиты (пол на весь мир) — в отдельные ячейки не кладём, их заменяет земля.
            if size.x > 200.0 || size.y > 200.0 {
                continue;
            }
            let top = Top {
                min,
                max,
                y: b.aabb.max.y,
                surface,
            };
            for c in cells(min, max) {
                tops.entry(c).or_default().push(top);
            }
            // Широкие плоские плиты — перекрытия и потолки.
            if b.solid && size.x > 1.5 && size.y > 1.5 && b.aabb.max.y - b.aabb.min.y < 1.0 {
                let under = Top {
                    min,
                    max,
                    y: b.aabb.min.y,
                    surface,
                };
                for c in cells(min, max) {
                    ceilings.entry(c).or_default().push(under);
                }
            }
        }
        let mut roofs: HashMap<(i32, i32), Vec<RoofTri>> = HashMap::new();
        for ((mat, _), mb) in &layout.decor {
            if !matches!(mat, Mat::Tile | Mat::Roof | Mat::Thatch | Mat::Felt) {
                continue;
            }
            for t in mb.triangles() {
                let min = Vec2::new(
                    t[0].x.min(t[1].x).min(t[2].x),
                    t[0].z.min(t[1].z).min(t[2].z),
                );
                let max = Vec2::new(
                    t[0].x.max(t[1].x).max(t[2].x),
                    t[0].z.max(t[1].z).max(t[2].z),
                );
                if (max - min).max_element() > 80.0 {
                    continue;
                }
                for c in cells(min, max) {
                    roofs.entry(c).or_default().push(RoofTri(t));
                }
            }
        }
        let mut fires = layout.hearths.clone();
        fires.extend(layout.forges.iter().map(|f| f.fire));
        Self {
            tops,
            roofs,
            ceilings,
            ground: layout.ground.clone(),
            water_y: layout.water_y,
            fires,
        }
    }

    /// Высота земли (рельеф) в точке.
    pub fn ground_y(&self, x: f32, z: f32) -> Option<f32> {
        self.ground.as_ref().map(|g| g.height(x, z))
    }

    /// Море ли это (земля ниже уровня воды).
    pub fn is_sea(&self, x: f32, z: f32) -> bool {
        match (self.water_y, self.ground_y(x, z)) {
            (Some(w), Some(g)) => g < w - 0.3,
            _ => false,
        }
    }

    /// По чему стоит человек, чьи ноги в точке `feet`.
    pub fn surface_at(&self, feet: Vec3) -> Surface {
        let p = Vec2::new(feet.x, feet.z);
        let mut best: Option<Top> = None;
        if let Some(list) = self.tops.get(&cell(feet.x, feet.z)) {
            for t in list {
                if p.x >= t.min.x - 0.05
                    && p.x <= t.max.x + 0.05
                    && p.y >= t.min.y - 0.05
                    && p.y <= t.max.y + 0.05
                    && (t.y - feet.y).abs() < 0.3
                    && best.is_none_or(|b| t.y > b.y)
                {
                    best = Some(*t);
                }
            }
        }
        if let Some(b) = best {
            return b.surface;
        }
        if let Some(w) = self.water_y
            && feet.y < w + 0.15
        {
            return Surface::Water;
        }
        if crate::level::inside_city(feet.x, feet.z) {
            Surface::Dirt
        } else {
            Surface::Grass
        }
    }

    /// Есть ли над точкой крыша или перекрытие (в пределах 14 м).
    pub fn roofed(&self, head: Vec3) -> bool {
        let p = Vec2::new(head.x, head.z);
        let c = cell(head.x, head.z);
        if let Some(list) = self.ceilings.get(&c)
            && list.iter().any(|t| {
                p.x >= t.min.x
                    && p.x <= t.max.x
                    && p.y >= t.min.y
                    && p.y <= t.max.y
                    && t.y > head.y
                    && t.y < head.y + 14.0
            })
        {
            return true;
        }
        if let Some(list) = self.roofs.get(&c) {
            for RoofTri(t) in list {
                if let Some(y) = tri_height(t, p)
                    && y > head.y
                    && y < head.y + 14.0
                {
                    return true;
                }
            }
        }
        false
    }
}

/// Высота треугольника над точкой плана (если точка внутри его проекции).
fn tri_height(t: &[Vec3; 3], p: Vec2) -> Option<f32> {
    let (a, b, c) = (
        Vec2::new(t[0].x, t[0].z),
        Vec2::new(t[1].x, t[1].z),
        Vec2::new(t[2].x, t[2].z),
    );
    let d = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
    if d.abs() < 1e-6 {
        return None;
    }
    let l1 = ((b.y - c.y) * (p.x - c.x) + (c.x - b.x) * (p.y - c.y)) / d;
    let l2 = ((c.y - a.y) * (p.x - c.x) + (a.x - c.x) * (p.y - c.y)) / d;
    let l3 = 1.0 - l1 - l2;
    (l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0).then(|| l1 * t[0].y + l2 * t[1].y + l3 * t[2].y)
}

/// Карта поверхностей (появляется, когда поток её достроит).
#[derive(Resource, Clone, Default)]
pub struct Surfaces(pub Arc<OnceLock<SurfaceMap>>);

impl Surfaces {
    pub fn get(&self) -> Option<&SurfaceMap> {
        self.0.get()
    }
}

pub fn start_building(mut commands: Commands) {
    let slot = Surfaces::default();
    let target = slot.0.clone();
    std::thread::Builder::new()
        .name("audio-surfaces".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let layout = crate::level::layout();
            let _ = target.set(SurfaceMap::build(&layout));
        })
        .ok();
    commands.insert_resource(slot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_surfaces_make_sense() {
        let map = SurfaceMap::build(&crate::level::layout());
        // Точка появления — на улице или мостовой города, не в море.
        let p = crate::level::spawn_point(0);
        let s = map.surface_at(p);
        assert_ne!(s, Surface::Water, "у точки появления под ногами вода");
        // Где-то в городе есть крыши.
        let roofed = crate::level::places::places()
            .iter()
            .filter(|pl| map.roofed(pl.pos + Vec3::Y * 1.7))
            .count();
        assert!(roofed > 0, "ни одно место не под крышей");
    }

    #[test]
    fn triangle_height() {
        let t = [
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(4.0, 5.0, 0.0),
            Vec3::new(0.0, 9.0, 4.0),
        ];
        assert!((tri_height(&t, Vec2::new(1.0, 1.0)).unwrap() - 6.0).abs() < 1e-4);
        assert!(tri_height(&t, Vec2::new(5.0, 5.0)).is_none());
    }
}
