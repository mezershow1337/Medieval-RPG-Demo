//! Иконки предметов. Рисуются кодом при запуске (своя простая «векторная» графика),
//! поэтому не нужны файлы картинок и нет чужих ассетов.
//!
//! Координаты фигур — в клетках инвентаря: у меча 1×3 холст 1 клетка в ширину и 3 в высоту.

use crate::items::{ITEM_BASES, ItemBaseId};
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

/// Клинок вертикально: острие сверху. `cx` — середина, от `top` до `bottom`.
fn blade(c: &mut Canvas, cx: f32, top: f32, bottom: f32, half: f32) {
    c.poly(&[(cx, top), (cx + half, top + half * 2.5), (cx + half, bottom), (cx - half, bottom), (cx - half, top + half * 2.5)], STEEL);
    c.poly(&[(cx, top), (cx + half, top + half * 2.5), (cx + half, bottom), (cx, bottom)], STEEL_LIGHT);
    c.line(cx, top + half * 3.0, cx, bottom - 0.05, half * 0.35, STEEL_DARK);
}

fn sword(c: &mut Canvas, len: f32, guard_y: f32) {
    let cx = 0.5;
    blade(c, cx, 0.12, guard_y, 0.11);
    c.rect(cx - 0.34, guard_y, cx + 0.34, guard_y + 0.1, GOLD_DARK);
    c.rect(cx - 0.32, guard_y + 0.01, cx + 0.32, guard_y + 0.06, GOLD);
    c.rect(cx - 0.07, guard_y + 0.1, cx + 0.07, len - 0.2, LEATHER_DARK);
    for i in 0..4 {
        let y = guard_y + 0.15 + i as f32 * 0.09;
        if y < len - 0.22 {
            c.line(cx - 0.07, y, cx + 0.07, y + 0.05, 0.025, LEATHER);
        }
    }
    c.circle(cx, len - 0.14, 0.11, GOLD_DARK);
    c.circle(cx - 0.02, len - 0.16, 0.06, GOLD);
}

fn tunic(c: &mut Canvas, body: Rgba, dark: Rgba) {
    // Торс с рукавами на холсте 2×3.
    c.poly(&[(0.55, 0.3), (1.45, 0.3), (1.95, 0.75), (1.75, 1.35), (1.5, 1.1), (1.5, 2.75), (0.5, 2.75), (0.5, 1.1), (0.25, 1.35), (0.05, 0.75)], body);
    c.poly(&[(0.8, 0.3), (1.2, 0.3), (1.0, 0.6)], dark);
    c.line(0.5, 2.7, 1.5, 2.7, 0.08, dark);
}

fn draw(base: ItemBaseId, c: &mut Canvas) {
    let name = base.base().name;
    match name {
        "Одноручный меч" => sword(c, 3.0, 2.0),
        "Длинный меч" => sword(c, 4.0, 2.75),
        "Кинжал" => {
            blade(c, 0.5, 0.12, 1.25, 0.1);
            c.rect(0.25, 1.25, 0.75, 1.33, GOLD_DARK);
            c.rect(0.43, 1.33, 0.57, 1.8, LEATHER_DARK);
            c.circle(0.5, 1.85, 0.08, GOLD);
        }
        "Булава" => {
            c.rect(0.44, 0.7, 0.56, 1.88, WOOD);
            c.rect(0.42, 1.55, 0.58, 1.88, LEATHER_DARK);
            for (dx, dy) in [(-0.3, 0.0), (0.3, 0.0), (0.0, -0.3), (-0.21, -0.21), (0.21, -0.21), (-0.21, 0.21), (0.21, 0.21)] {
                c.poly(&[(0.5 + dx * 0.4, 0.45 + dy * 0.4), (0.5 + dx * 1.25, 0.45 + dy * 1.25), (0.5 + dx * 0.5 - dy * 0.3, 0.45 + dy * 0.5 + dx * 0.3)], STEEL_DARK);
            }
            c.circle(0.5, 0.45, 0.26, STEEL);
            c.circle(0.44, 0.39, 0.1, STEEL_LIGHT);
        }
        "Боевой топор" => {
            c.line(1.2, 0.2, 0.9, 2.85, 0.13, WOOD);
            c.line(1.17, 0.45, 1.13, 0.85, 0.15, IRON);
            c.poly(&[(1.12, 0.45), (0.35, 0.1), (0.15, 0.75), (0.35, 1.3), (1.1, 0.9)], STEEL);
            c.poly(&[(0.35, 0.1), (0.15, 0.75), (0.35, 1.3), (0.45, 0.7)], STEEL_LIGHT);
            c.rect(0.85, 2.4, 1.05, 2.85, LEATHER_DARK);
        }
        "Алебарда" => {
            c.rect(0.45, 0.6, 0.55, 4.9, WOOD);
            c.poly(&[(0.5, 0.05), (0.58, 0.6), (0.42, 0.6)], STEEL_LIGHT);
            c.poly(&[(0.55, 0.65), (0.95, 0.55), (0.98, 1.25), (0.55, 1.1)], STEEL);
            c.poly(&[(0.45, 0.7), (0.12, 0.85), (0.45, 0.95)], STEEL_DARK);
            for y in [2.2, 3.4] {
                c.rect(0.42, y, 0.58, y + 0.12, IRON);
            }
        }
        "Каплевидный щит" => {
            c.poly(&[(0.15, 0.2), (1.85, 0.2), (1.8, 1.4), (1.0, 2.85), (0.2, 1.4)], WOOD_DARK);
            c.poly(&[(0.28, 0.32), (1.72, 0.32), (1.68, 1.38), (1.0, 2.62), (0.32, 1.38)], WHITE);
            c.rect(0.88, 0.32, 1.12, 2.45, RED);
            c.rect(0.3, 0.85, 1.7, 1.08, RED);
        }
        "Баклер" => {
            c.circle(1.0, 1.0, 0.85, STEEL_DARK);
            c.circle(1.0, 1.0, 0.75, STEEL);
            c.ring(1.0, 1.0, 0.5, 0.56, STEEL_DARK);
            c.circle(1.0, 1.0, 0.25, STEEL_LIGHT);
            c.circle(0.93, 0.93, 0.09, WHITE);
        }
        "Топфхельм" => {
            c.rect(0.3, 0.25, 1.7, 1.75, STEEL);
            c.rect(1.0, 0.25, 1.7, 1.75, STEEL_DARK);
            c.rect(0.3, 0.72, 1.7, 0.84, OUTLINE);
            c.rect(0.95, 0.25, 1.05, 1.75, STEEL_LIGHT);
            for (x, y) in [(0.55, 1.2), (0.7, 1.35), (1.3, 1.2), (1.45, 1.35)] {
                c.circle(x, y, 0.04, OUTLINE);
            }
        }
        "Шапель" => {
            c.poly(&[(0.5, 1.05), (0.6, 0.45), (1.0, 0.25), (1.4, 0.45), (1.5, 1.05)], STEEL);
            c.poly(&[(1.0, 0.25), (1.4, 0.45), (1.5, 1.05), (1.0, 1.05)], STEEL_DARK);
            c.poly(&[(0.1, 1.05), (1.9, 1.05), (1.7, 1.35), (0.3, 1.35)], STEEL_LIGHT);
            c.rect(0.55, 1.35, 1.45, 1.75, LEATHER_DARK);
        }
        "Стёганый гамбезон" => {
            tunic(c, CLOTH, CLOTH_DARK);
            for i in 0..6 {
                let y = 1.0 + i as f32 * 0.28;
                c.line(0.55, y, 1.45, y, 0.03, CLOTH_DARK);
            }
            c.line(1.0, 0.6, 1.0, 2.7, 0.03, CLOTH_DARK);
        }
        "Кольчужный хауберк" => {
            tunic(c, STEEL_DARK, IRON);
            let mut y = 0.45;
            while y < 2.65 {
                let mut x = 0.2;
                while x < 1.85 {
                    c.ring(x, y, 0.035, 0.06, STEEL_LIGHT);
                    x += 0.14;
                }
                y += 0.13;
            }
            tunic_mask(c);
        }
        "Латная кираса" => {
            c.poly(&[(0.35, 0.35), (1.65, 0.35), (1.75, 1.2), (1.5, 2.3), (1.0, 2.6), (0.5, 2.3), (0.25, 1.2)], STEEL);
            c.poly(&[(1.0, 0.35), (1.65, 0.35), (1.75, 1.2), (1.5, 2.3), (1.0, 2.6)], STEEL_DARK);
            c.line(1.0, 0.45, 1.0, 2.5, 0.06, STEEL_LIGHT);
            c.poly(&[(0.35, 0.35), (0.75, 0.35), (0.25, 0.9)], STEEL_LIGHT);
            c.rect(0.4, 2.35, 1.6, 2.5, LEATHER_DARK);
        }
        "Латные рукавицы" | "Кожаные перчатки" => {
            let (main, dark) = if name.starts_with("Латные") { (STEEL, STEEL_DARK) } else { (LEATHER, LEATHER_DARK) };
            c.rect(0.55, 1.15, 1.45, 1.85, dark);
            c.poly(&[(0.5, 1.2), (0.45, 0.55), (1.5, 0.55), (1.5, 1.2)], main);
            for i in 0..4 {
                let x = 0.55 + i as f32 * 0.24;
                c.rect(x, 0.2, x + 0.19, 0.6, main);
                c.line(x, 0.42, x + 0.19, 0.42, 0.03, dark);
            }
            c.poly(&[(1.5, 0.75), (1.85, 0.6), (1.9, 0.8), (1.5, 1.1)], main);
        }
        "Латные поножи" => {
            for x in [0.35, 1.05] {
                c.rect(x, 0.2, x + 0.6, 2.4, STEEL);
                c.rect(x + 0.3, 0.2, x + 0.6, 2.4, STEEL_DARK);
                c.circle(x + 0.3, 1.25, 0.22, STEEL_LIGHT);
                c.rect(x - 0.05, 2.4, x + 0.75, 2.8, IRON);
            }
        }
        "Кожаные сапоги" => {
            c.poly(&[(0.55, 0.2), (1.15, 0.2), (1.15, 1.3), (1.85, 1.45), (1.85, 1.8), (0.55, 1.8)], LEATHER);
            c.rect(0.5, 0.2, 1.2, 0.4, LEATHER_DARK);
            c.rect(0.55, 1.68, 1.85, 1.8, LEATHER_DARK);
        }
        "Серебряное кольцо" => {
            c.ring(0.5, 0.58, 0.2, 0.3, SILVER);
            c.circle(0.5, 0.26, 0.13, RED);
            c.circle(0.46, 0.22, 0.05, RED_LIGHT);
        }
        "Амулет с гербом" => {
            c.ring(0.5, 0.35, 0.24, 0.28, GOLD_DARK);
            c.poly(&[(0.28, 0.5), (0.72, 0.5), (0.7, 0.72), (0.5, 0.92), (0.3, 0.72)], GOLD);
            c.rect(0.46, 0.55, 0.54, 0.82, RED);
        }
        "Золотые монеты" => {
            for (x, y) in [(0.36, 0.72), (0.64, 0.72), (0.5, 0.55), (0.5, 0.36)] {
                c.circle(x, y, 0.2, GOLD_DARK);
                c.circle(x, y - 0.02, 0.16, GOLD);
                c.ring(x, y - 0.02, 0.09, 0.11, GOLD_DARK);
            }
        }
        "Бинт" => {
            c.circle(0.5, 0.48, 0.32, WHITE);
            c.ring(0.5, 0.48, 0.12, 0.17, CLOTH_DARK);
            c.circle(0.5, 0.48, 0.08, CLOTH);
            c.poly(&[(0.78, 0.6), (0.92, 0.85), (0.72, 0.92), (0.6, 0.72)], WHITE);
            c.line(0.25, 0.3, 0.75, 0.3, 0.04, RED_LIGHT);
        }
        "Целебное зелье" => {
            c.rect(0.4, 0.15, 0.6, 0.35, WOOD);
            c.rect(0.42, 0.35, 0.58, 0.6, GLASS);
            c.circle(0.5, 1.35, 0.38, GLASS);
            c.circle(0.5, 1.4, 0.32, RED);
            c.circle(0.4, 1.3, 0.08, RED_LIGHT);
        }
        "Рубин" => {
            c.poly(&[(0.2, 0.4), (0.35, 0.2), (0.65, 0.2), (0.8, 0.4), (0.5, 0.85)], RED);
            c.poly(&[(0.35, 0.2), (0.65, 0.2), (0.6, 0.4), (0.4, 0.4)], RED_LIGHT);
            c.poly(&[(0.2, 0.4), (0.4, 0.4), (0.5, 0.85)], [0.5, 0.05, 0.08, 1.0]);
        }
        _ => {
            c.circle(0.5, 0.5, 0.3, STEEL);
        }
    }
    c.outline();
}

/// Обрезает кольца кольчуги по форме рубахи.
fn tunic_mask(c: &mut Canvas) {
    let mut mask = Canvas::new(2, 3);
    tunic(&mut mask, WHITE, WHITE);
    for (p, m) in c.px.iter_mut().zip(mask.px.iter()) {
        if m[3] < 0.5 {
            *p = [0.0; 4];
        }
    }
}

/// Иконка одного вида предмета: (ширина, высота, RGBA).
pub fn render_icon(base: ItemBaseId) -> (u32, u32, Vec<u8>) {
    let (w, h) = base.base().size;
    let mut c = Canvas::new(w, h);
    draw(base, &mut c);
    c.to_rgba8()
}

/// Иконки всех предметов по номеру вида.
#[derive(Resource)]
pub struct ItemIcons(pub Vec<Handle<Image>>);

impl ItemIcons {
    pub fn get(&self, base: ItemBaseId) -> Handle<Image> {
        self.0[base.0 as usize].clone()
    }
}

pub fn setup_icons(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let handles = (0..ITEM_BASES.len())
        .map(|i| {
            let (w, h, data) = render_icon(ItemBaseId(i as u16));
            images.add(Image::new(
                Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                TextureDimension::D2,
                data,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            ))
        })
        .collect();
    commands.insert_resource(ItemIcons(handles));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_has_visible_icon() {
        for (i, base) in ITEM_BASES.iter().enumerate() {
            let (w, h, data) = render_icon(ItemBaseId(i as u16));
            assert_eq!((w, h), (base.size.0 as u32 * PX_PER_CELL, base.size.1 as u32 * PX_PER_CELL));
            let opaque = data.chunks(4).filter(|p| p[3] > 128).count();
            assert!(opaque > (w * h) as usize / 12, "иконка «{}» почти пустая", base.name);
            // ICON_DUMP=папка — сохранить иконки (формат PAM) для просмотра.
            if let Ok(dir) = std::env::var("ICON_DUMP") {
                let mut f = format!("P7\nWIDTH {w}\nHEIGHT {h}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n").into_bytes();
                f.extend_from_slice(&data);
                std::fs::write(format!("{dir}/{i:02}.pam"), f).unwrap();
            }
        }
    }
}
