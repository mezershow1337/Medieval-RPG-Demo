//! Временные плоские иконки, нарисованные кодом, — только для предметов, у которых
//! ещё нет 3D-модели (см. `ITEM_ART` в icons.rs). Когда модели появятся у всех, модуль уйдёт.
//!
//! Координаты фигур — в клетках инвентаря: у меча 1×3 холст 1 клетка в ширину и 3 в высоту.

#![allow(dead_code)] // часть цветов и фигур нужна только trade_icons/lore_icons

use crate::items::ItemBaseId;

#[path = "../trade_icons.rs"]
mod trade_icons;
/// Травы, вещества, книги и посуда.
#[path = "lore_icons.rs"]
mod lore_icons;
use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Пикселей на клетку в готовой иконке.
pub const PX_PER_CELL: u32 = 48;
/// Рисуем вдвое крупнее и уменьшаем — так края получаются гладкими.
const SUPERSAMPLE: u32 = 2;

type Rgba = [f32; 4];

const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

const STEEL: Rgba = rgb(178, 184, 194);
const STEEL_LIGHT: Rgba = rgb(225, 230, 238);
const STEEL_DARK: Rgba = rgb(104, 110, 122);
const IRON: Rgba = rgb(80, 82, 90);
const WOOD: Rgba = rgb(122, 82, 46);
const WOOD_DARK: Rgba = rgb(84, 54, 30);
const LEATHER: Rgba = rgb(132, 86, 52);
const LEATHER_DARK: Rgba = rgb(92, 58, 34);
const CLOTH: Rgba = rgb(196, 176, 132);
const CLOTH_DARK: Rgba = rgb(150, 130, 92);
const GOLD: Rgba = rgb(232, 184, 64);
const GOLD_DARK: Rgba = rgb(170, 122, 30);
const RED: Rgba = rgb(170, 30, 34);
const RED_LIGHT: Rgba = rgb(230, 70, 70);
const WHITE: Rgba = rgb(236, 232, 222);
const SILVER: Rgba = rgb(200, 204, 212);
const GLASS: Rgba = [0.75, 0.85, 0.9, 0.55];
const OUTLINE: Rgba = rgb(20, 16, 14);

/// Холст в «клетках» с растеризацией фигур.
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    scale: f32,
    px: Vec<Rgba>,
}

impl Canvas {
    fn new(cells_w: u8, cells_h: u8) -> Self {
        let scale = (PX_PER_CELL * SUPERSAMPLE) as f32;
        let (w, h) = (cells_w as u32 * PX_PER_CELL * SUPERSAMPLE, cells_h as u32 * PX_PER_CELL * SUPERSAMPLE);
        Self { w, h, scale, px: vec![[0.0; 4]; (w * h) as usize] }
    }

    fn blend(&mut self, x: u32, y: u32, c: Rgba) {
        let p = &mut self.px[(y * self.w + x) as usize];
        let a = c[3];
        for i in 0..3 {
            p[i] = p[i] * (1.0 - a) + c[i] * a;
        }
        p[3] = p[3] + a * (1.0 - p[3]);
    }

    /// Закрашивает пиксели в рамке (в клетках), для которых `inside(x, y)` истинно.
    fn fill(&mut self, min: Vec2, max: Vec2, c: Rgba, inside: impl Fn(Vec2) -> bool) {
        let x0 = ((min.x * self.scale).floor().max(0.0)) as u32;
        let y0 = ((min.y * self.scale).floor().max(0.0)) as u32;
        let x1 = ((max.x * self.scale).ceil() as u32).min(self.w);
        let y1 = ((max.y * self.scale).ceil() as u32).min(self.h);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = Vec2::new((x as f32 + 0.5) / self.scale, (y as f32 + 0.5) / self.scale);
                if inside(p) {
                    self.blend(x, y, c);
                }
            }
        }
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Rgba) {
        self.fill(Vec2::new(x0, y0), Vec2::new(x1, y1), c, |_| true);
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let ctr = Vec2::new(cx, cy);
        self.fill(ctr - r, ctr + r, c, |p| p.distance(ctr) <= r);
    }

    fn ring(&mut self, cx: f32, cy: f32, r0: f32, r1: f32, c: Rgba) {
        let ctr = Vec2::new(cx, cy);
        self.fill(ctr - r1, ctr + r1, c, |p| (r0..=r1).contains(&p.distance(ctr)));
    }

    fn poly(&mut self, pts: &[(f32, f32)], c: Rgba) {
        let pts: Vec<Vec2> = pts.iter().map(|(x, y)| Vec2::new(*x, *y)).collect();
        let min = pts.iter().fold(Vec2::splat(f32::MAX), |a, b| a.min(*b));
        let max = pts.iter().fold(Vec2::splat(f32::MIN), |a, b| a.max(*b));
        self.fill(min, max, c, |p| {
            // Правило чётности.
            let mut inside = false;
            let mut j = pts.len() - 1;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[j]);
                if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                    inside = !inside;
                }
                j = i;
            }
            inside
        });
    }

    /// Толстая линия.
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgba) {
        let (a, b) = (Vec2::new(x0, y0), Vec2::new(x1, y1));
        let half = width / 2.0;
        self.fill(a.min(b) - half, a.max(b) + half, c, |p| {
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            p.distance(a + ab * t) <= half
        });
    }

    /// Тёмный контур вокруг нарисованного и лёгкая тень — как у иконок в RPG.
    fn outline(&mut self) {
        let thick = (SUPERSAMPLE * 2) as i32;
        let src = self.px.clone();
        let (w, h) = (self.w as i32, self.h as i32);
        for y in 0..h {
            for x in 0..w {
                if src[(y * w + x) as usize][3] > 0.5 {
                    continue;
                }
                let mut near = false;
                'search: for dy in -thick..=thick {
                    for dx in -thick..=thick {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx >= 0 && ny >= 0 && nx < w && ny < h && src[(ny * w + nx) as usize][3] > 0.5 {
                            near = true;
                            break 'search;
                        }
                    }
                }
                if near {
                    self.px[(y * w + x) as usize] = OUTLINE;
                }
            }
        }
    }

    /// Готовые байты RGBA (с уменьшением).
    fn to_rgba8(&self) -> (u32, u32, Vec<u8>) {
        let s = SUPERSAMPLE;
        let (w, h) = (self.w / s, self.h / s);
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 4];
                for sy in 0..s {
                    for sx in 0..s {
                        let p = self.px[((y * s + sy) * self.w + x * s + sx) as usize];
                        // Цвет усредняем с учётом прозрачности.
                        for i in 0..3 {
                            acc[i] += p[i] * p[3];
                        }
                        acc[3] += p[3];
                    }
                }
                let n = (s * s) as f32;
                let a = acc[3] / n;
                for i in 0..3 {
                    let c = if acc[3] > 0.0 { acc[i] / acc[3] } else { 0.0 };
                    out.push((c.clamp(0.0, 1.0) * 255.0) as u8);
                }
                out.push((a.clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
        (w, h, out)
    }
}

fn draw(base: ItemBaseId, c: &mut Canvas) {
    let name = base.base().name;
    match name {
        // Монеты и товары торговли рисует trade_icons.rs.
        _ if trade_icons::draw(name, c) => {}
        other => {
            if !lore_icons::draw(other, c) {
                c.circle(0.5, 0.5, 0.3, STEEL);
            }
        }
    }
    c.outline();
}

/// Иконка одного вида предмета: (ширина, высота, RGBA).
pub fn render_icon(base: ItemBaseId) -> (u32, u32, Vec<u8>) {
    let (w, h) = base.base().size;
    let mut c = Canvas::new(w, h);
    draw(base, &mut c);
    c.to_rgba8()
}


/// Готовая картинка для Bevy.
pub fn image(base: ItemBaseId) -> Image {
    let (w, h, data) = render_icon(base);
    Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}
