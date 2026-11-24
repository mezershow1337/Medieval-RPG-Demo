//! Клиент: восковая табличка и счётная доска перед глазами (письмо и счёт, см. work.rs).
//!
//! Пока учитесь буквам (`Drill::Trace`), пишете слово (`Drill::Scribe`) или считаете
//! камешками (`Drill::Count`), внизу экрана — табличка: тёмный воск в деревянной рамке,
//! бледный образец буквы, который учитель процарапал, ваши светлые черты и кончик стиля.
//! Для счёта — доска с тремя столбцами (Η — сотни, Δ — десятки, Ι — единицы, как на
//! саламинской доске) и камешками; загаданное число выведено греческими буквами-цифрами.
//!
//! Картинка рисуется на процессоре в одну текстуру (`Image`) каждый кадр — она маленькая.

use crate::inventory_ui::UiFonts;
use crate::letters::{self, Glyph};
use crate::work::{Drill, Meter};
use crate::work_client::MyWork;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub struct TabletViewPlugin;

impl Plugin for TabletViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_tablet.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, draw_tablet.after(crate::work_client::WorkSet));
    }
}

const W: usize = 560;
const H: usize = 300;
const FRAME: usize = 16;

#[derive(Component)]
struct TabletView;

#[derive(Resource)]
struct TabletImage {
    handle: Handle<Image>,
    /// Воск и рамка без букв (рисуются один раз).
    wax: Vec<u8>,
    /// Доска для счёта без камешков.
    board: Vec<u8>,
}

/// Подписи поверх таблички.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Label {
    /// Крупно: имя буквы или загаданное число.
    Big,
    /// Мельче: слово и перевод, подсказка по цифрам.
    Small,
    /// Столбцы доски: сотни, десятки, единицы.
    Column(usize),
}

fn setup_tablet(mut commands: Commands, mut images: ResMut<Assets<Image>>, fonts: Res<UiFonts>) {
    let wax = background(false);
    let board = background(true);
    let image = Image::new(Extent3d { width: W as u32, height: H as u32, depth_or_array_layers: 1 }, TextureDimension::D2, wax.clone(), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::all());
    let handle = images.add(image);
    commands.insert_resource(TabletImage { handle: handle.clone(), wax, board });
    let text = |size: f32, bold: bool| {
        (
            Text::new(""),
            TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: bevy::text::FontSize::Px(size), ..default() },
            TextColor(Color::srgb(0.95, 0.88, 0.7)),
            TextShadow { offset: Vec2::new(1.0, 1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.85) },
        )
    };
    commands
        .spawn((
            TabletView,
            crate::civil_client::ShotKeep,
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                left: percent(50),
                margin: UiRect::left(px(-(W as f32) / 2.0)),
                bottom: px(110),
                width: px(W as f32),
                height: px(H as f32),
                ..default()
            },
            ImageNode::new(handle),
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.6), px(0), px(6), px(2), px(18)),
        ))
        .with_children(|p| {
            let abs = |l: f32, t: f32, w: f32| Node { position_type: PositionType::Absolute, left: px(l), top: px(t), width: px(w), ..default() };
            p.spawn((Label::Big, text(26.0, true), abs(0.0, 0.0, 240.0)));
            p.spawn((Label::Small, text(15.0, false), abs(0.0, 0.0, 240.0)));
            for k in 0..3 {
                p.spawn((Label::Column(k), text(14.0, true), abs(0.0, 0.0, 150.0), TextLayout::justify(Justify::Center)));
            }
        });
}

// ---------------------------------------------------------------------------
// Растр
// ---------------------------------------------------------------------------

type Rgb = [f32; 3];

fn hash(x: usize, y: usize) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

fn to_u8(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Рамка из дерева и воск (или светлая доска для счёта) с лёгким зерном.
fn background(board: bool) -> Vec<u8> {
    let mut out = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let edge = x.min(W - 1 - x).min(y).min(H - 1 - y);
            let n = hash(x, y);
            let c: Rgb = if edge < FRAME {
                // Дерево: волокна вдоль длинной стороны, тёмный кант снаружи и у воска.
                let grain = (y as f32 * 0.55 + (x as f32 * 0.021).sin() * 6.0).sin() * 0.5 + 0.5;
                let shade = if edge < 2 || edge >= FRAME - 2 { 0.62 } else { 1.0 };
                let k = (0.82 + 0.18 * grain + 0.06 * (n - 0.5)) * shade;
                [0.46 * k, 0.30 * k, 0.16 * k]
            } else if board {
                // Доска для счёта: светлое дерево, потёртое посередине.
                let grain = (x as f32 * 0.08 + (y as f32 * 0.05).sin() * 3.0).sin() * 0.5 + 0.5;
                let k = 0.9 + 0.07 * grain + 0.05 * (n - 0.5);
                [0.64 * k, 0.52 * k, 0.36 * k]
            } else {
                // Воск: тёмный, неровный, у рамки темнее.
                let vign = ((edge - FRAME) as f32 / 30.0).min(1.0);
                let k = 0.78 + 0.22 * vign + 0.08 * (n - 0.5);
                [0.25 * k, 0.16 * k, 0.075 * k]
            };
            let i = (y * W + x) * 4;
            out[i] = to_u8(c[0]);
            out[i + 1] = to_u8(c[1]);
            out[i + 2] = to_u8(c[2]);
            out[i + 3] = 255;
        }
    }
    out
}

struct Canvas<'a> {
    px: &'a mut [u8],
}

impl Canvas<'_> {
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if x < FRAME as i32 || y < FRAME as i32 || x >= (W - FRAME) as i32 || y >= (H - FRAME) as i32 || a <= 0.0 {
            return;
        }
        let i = (y as usize * W + x as usize) * 4;
        for k in 0..3 {
            let old = self.px[i + k] as f32 / 255.0;
            self.px[i + k] = to_u8(old + (c[k] - old) * a.min(1.0));
        }
    }

    /// Круг с мягким краем.
    fn disc(&mut self, p: Vec2, r: f32, c: Rgb, a: f32) {
        let (x0, x1) = ((p.x - r - 1.0) as i32, (p.x + r + 1.0) as i32);
        let (y0, y1) = ((p.y - r - 1.0) as i32, (p.y + r + 1.0) as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(p);
                let cover = (r + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    self.blend(x, y, c, a * cover);
                }
            }
        }
    }

    /// Отрезок толщиной `w` (капсула).
    fn line(&mut self, a: Vec2, b: Vec2, w: f32, c: Rgb, alpha: f32) {
        let r = w / 2.0;
        let (x0, x1) = ((a.x.min(b.x) - r - 1.0) as i32, (a.x.max(b.x) + r + 1.0) as i32);
        let (y0, y1) = ((a.y.min(b.y) - r - 1.0) as i32, (a.y.max(b.y) + r + 1.0) as i32);
        let ab = b - a;
        let len2 = ab.length_squared().max(1e-6);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
                let d = p.distance(a + ab * t);
                let cover = (r + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    self.blend(x, y, c, alpha * cover);
                }
            }
        }
    }

    fn polyline(&mut self, pts: &[Vec2], w: f32, c: Rgb, a: f32) {
        if pts.len() == 1 {
            self.disc(pts[0], w / 2.0, c, a);
        }
        for s in pts.windows(2) {
            self.line(s[0], s[1], w, c, a);
        }
    }

    fn rect_outline(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: f32) {
        let (p0, p1, p2, p3) = (Vec2::new(x, y), Vec2::new(x + w, y), Vec2::new(x + w, y + h), Vec2::new(x, y + h));
        for (s, e) in [(p0, p1), (p1, p2), (p2, p3), (p3, p0)] {
            self.line(s, e, 1.5, c, a);
        }
    }
}

/// Клетка буквы на табличке: точка буквы (0..1, y вверх) -> пиксель.
#[derive(Clone, Copy)]
struct Cell {
    x: f32,
    y: f32,
    size: f32,
}

impl Cell {
    fn at(&self, p: Vec2) -> Vec2 {
        Vec2::new(self.x + p.x * self.size, self.y + (1.0 - p.y) * self.size)
    }
}

const MODEL: Rgb = [0.55, 0.43, 0.25];
const SCRATCH: Rgb = [0.86, 0.74, 0.5];
const NIB: Rgb = [0.75, 0.16, 0.08];
const GOAL: Rgb = [0.55, 0.85, 0.4];

/// Образец буквы (бледно), пройденное — ярче.
fn draw_model(c: &mut Canvas, g: &Glyph, cell: Cell, w: f32, a: f32) {
    for s in &g.strokes {
        let pts: Vec<Vec2> = s.iter().map(|p| cell.at(*p)).collect();
        c.polyline(&pts, w, MODEL, a);
    }
}

/// Процарапанное учеником.
fn draw_scratch(c: &mut Canvas, lines: &[Vec<Vec2>], cell: Cell, w: f32) {
    for l in lines {
        let pts: Vec<Vec2> = l.iter().map(|p| cell.at(*p)).collect();
        // Борозда: тёмный край и светлое дно.
        c.polyline(&pts, w + 2.0, [0.12, 0.07, 0.03], 0.55);
        c.polyline(&pts, w, SCRATCH, 0.95);
    }
}

/// Строки-линейки, как на школьных табличках.
fn draw_rules(c: &mut Canvas, cell: Cell) {
    for y in [0.0, 1.0] {
        let p = cell.at(Vec2::new(-0.15, y));
        let q = cell.at(Vec2::new(1.15, y));
        c.line(p, q, 1.0, [0.45, 0.33, 0.18], 0.6);
    }
}

fn draw_writing(c: &mut Canvas, m: &Meter) {
    let Some(g) = m.glyph() else { return };
    let big = match m.drill {
        Drill::Scribe => Cell { x: 52.0, y: 96.0, size: 170.0 },
        _ => Cell { x: 60.0, y: 46.0, size: 210.0 },
    };
    if m.drill == Drill::Scribe {
        // Слово целиком: сверху маленькими клетками.
        let n = m.letters.len().max(1);
        let s = (W as f32 - 2.0 * FRAME as f32 - 40.0) / n as f32;
        let s = s.min(54.0);
        let x0 = (W as f32 - s * n as f32) / 2.0;
        for (k, &li) in m.letters.iter().enumerate() {
            let cell = Cell { x: x0 + k as f32 * s + s * 0.12, y: 26.0 + s * 0.06, size: s * 0.76 };
            let gl = letters::glyph(li);
            if k == m.letter_i {
                c.rect_outline(x0 + k as f32 * s + 2.0, 24.0, s - 4.0, s, GOAL, 0.7);
            }
            if k < m.letter_i {
                draw_scratch(c, &m.scratch[k], cell, 2.0);
            } else {
                draw_model(c, gl, cell, 2.0, if k == m.letter_i { 0.8 } else { 0.45 });
            }
        }
    }
    draw_rules(c, big);
    // Образец: пройденные черты бледнее, текущая — ярче.
    for (k, s) in g.strokes.iter().enumerate() {
        let pts: Vec<Vec2> = s.iter().map(|p| big.at(*p)).collect();
        let a = if k == m.stroke_i { 0.85 } else if k < m.stroke_i { 0.3 } else { 0.55 };
        let w = if k == m.stroke_i { 7.0 } else { 6.0 };
        c.polyline(&pts, w, MODEL, a);
    }
    if let Some(lines) = m.scratch.get(m.letter_i) {
        draw_scratch(c, lines, big, 5.0);
    }
    // Куда вести дальше.
    if let Some(goal) = m.nib_goal() {
        c.disc(big.at(goal), 5.0, GOAL, 0.8);
    }
    // Допуск — бледное кольцо вокруг кончика стиля.
    let nib = big.at(m.nib);
    let tol = m.nib_tolerance() * big.size;
    for k in 0..48 {
        let a0 = k as f32 / 48.0 * std::f32::consts::TAU;
        if k % 2 == 0 {
            let a1 = a0 + std::f32::consts::TAU / 48.0;
            c.line(nib + Vec2::from_angle(a0) * tol, nib + Vec2::from_angle(a1) * tol, 1.2, [0.9, 0.8, 0.6], 0.35);
        }
    }
    c.disc(nib, 4.5, NIB, 1.0);
    c.disc(nib, 1.8, [1.0, 0.9, 0.7], 1.0);
}

/// Столбцы доски: середина по x.
fn column_x(k: usize) -> f32 {
    W as f32 * (0.5 + (k as f32 - 1.0) * 0.27)
}

fn draw_counting(c: &mut Canvas, m: &Meter) {
    let top = 118.0;
    let bottom = H as f32 - FRAME as f32 - 10.0;
    let first = if m.number < 100 { 1 } else { 0 };
    for k in 0..3 {
        let x = column_x(k);
        let active = k == m.col;
        if active {
            for dx in -60..=60 {
                c.line(Vec2::new(x + dx as f32, top), Vec2::new(x + dx as f32, bottom), 1.0, [0.95, 0.85, 0.55], 0.06);
            }
        }
        // Желоб столбца.
        for side in [-62.0, 62.0] {
            c.line(Vec2::new(x + side, top - 6.0), Vec2::new(x + side, bottom), 2.0, [0.33, 0.24, 0.14], 0.8);
        }
        // Камешки: по пять в ряд, снизу вверх.
        let n = m.pebbles[k] as usize;
        let dim = k < first;
        for i in 0..n {
            let (row, col) = (i / 5, i % 5);
            let p = Vec2::new(x - 44.0 + col as f32 * 22.0, bottom - 16.0 - row as f32 * 24.0);
            let shade = 0.85 + 0.15 * hash(i, k);
            c.disc(p + Vec2::new(1.5, 2.5), 10.0, [0.1, 0.07, 0.04], 0.45);
            c.disc(p, 10.0, [0.88 * shade, 0.86 * shade, 0.8 * shade], if dim { 0.4 } else { 1.0 });
            c.disc(p - Vec2::new(3.0, 3.0), 3.5, [1.0, 1.0, 0.98], 0.5);
        }
    }
    // Загаданное число — буквами-цифрами, как писал бы учитель.
    let nums = letters::numeral(m.number);
    let s = 50.0;
    let x0 = W as f32 - FRAME as f32 - 24.0 - s * nums.len() as f32;
    for (k, &g) in nums.iter().enumerate() {
        let cell = Cell { x: x0 + k as f32 * s, y: 30.0, size: s * 0.8 };
        let pts_w = 4.0;
        for st in &letters::glyph(g).strokes {
            let pts: Vec<Vec2> = st.iter().map(|p| cell.at(*p)).collect();
            c.polyline(&pts, pts_w, [0.2, 0.12, 0.06], 0.95);
        }
    }
    // Чёрточка над числом (так отличали цифры от слов).
    c.line(Vec2::new(x0, 24.0), Vec2::new(x0 + s * nums.len() as f32 - 8.0, 24.0), 2.5, [0.2, 0.12, 0.06], 0.9);
}

#[allow(clippy::type_complexity)]
fn draw_tablet(
    work: Res<MyWork>,
    tablet: Option<Res<TabletImage>>,
    mut images: ResMut<Assets<Image>>,
    mut view: Single<&mut Node, With<TabletView>>,
    mut labels: Query<(&Label, &mut Node, &mut Text, &mut TextColor), Without<TabletView>>,
) {
    let Some(m) = work.meter.as_ref().filter(|m| m.drill.tablet()) else {
        if view.display != Display::None {
            view.display = Display::None;
        }
        return;
    };
    view.display = Display::Flex;
    let Some(t) = tablet else { return };
    let Some(mut image) = images.get_mut(&t.handle) else { return };
    let base = if m.drill == Drill::Count { &t.board } else { &t.wax };
    let Some(data) = image.data.as_mut() else { return };
    data.copy_from_slice(base);
    let mut c = Canvas { px: data };
    match m.drill {
        Drill::Count => draw_counting(&mut c, m),
        _ => draw_writing(&mut c, m),
    }
    let g = m.glyph();
    for (label, mut node, mut text, mut color) in &mut labels {
        let mut set = |s: String, l: f32, t: f32, col: Color| {
            if text.0 != s {
                text.0 = s;
            }
            node.left = px(l);
            node.top = px(t);
            color.0 = col;
        };
        let light = Color::srgb(0.95, 0.88, 0.7);
        let ink = Color::srgb(0.22, 0.13, 0.06);
        match (m.drill, label) {
            (Drill::Trace, Label::Big) => set(g.map_or(String::new(), |g| format!("{}  {}", g.lower, g.name)), 300.0, 70.0, light),
            (Drill::Trace, Label::Small) => {
                let n = m.stroke_i + 1;
                let all = g.map_or(1, |g| g.strokes.len());
                set(format!("Черта {n} из {all}.\nВедите стиль от зелёной точки по бледному образцу."), 300.0, 120.0, light)
            }
            (Drill::Scribe, Label::Big) => {
                let w = letters::WORDS.get(m.word);
                set(w.map_or(String::new(), |w| w.1.to_string()), 262.0, 112.0, light)
            }
            (Drill::Scribe, Label::Small) => {
                let w = letters::WORDS.get(m.word);
                let now = g.map_or(String::new(), |g| format!("\nСейчас: {} ({})", g.lower, g.name));
                set(w.map_or(String::new(), |w| format!("«{}»{now}", w.2)), 262.0, 150.0, light)
            }
            (Drill::Count, Label::Big) => set(format!("Отложите: {}", letters::numeral_text(m.number)), 30.0, 26.0, ink),
            (Drill::Count, Label::Small) => {
                // Новичку учитель подсказывает, какой знак сколько значит.
                let s = if m.skill < 0.4 { format!("{}\nЛКМ — к следующему столбцу", letters::numeral_legend(m.number)) } else { "ЛКМ — к следующему столбцу".to_string() };
                set(s, 30.0, 62.0, ink)
            }
            (Drill::Count, Label::Column(k)) => {
                let name = ["Η · сотни", "Δ · десятки", "Ι · единицы"][*k];
                let col = if *k == m.col { Color::srgb(0.45, 0.12, 0.05) } else { ink };
                set(name.to_string(), column_x(*k) - 75.0, 96.0, col)
            }
            _ => set(String::new(), 0.0, 0.0, light),
        }
    }
}
