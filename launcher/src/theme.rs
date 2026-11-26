//! Тема лаунчера — те же текстуры и шрифты, что у окон игры (src/ui_theme.rs):
//! деревянная доска в бронзовой раме, кожа со строчкой, кожаные кнопки в бронзовом ободке,
//! унциал Ponomar для заголовков и Alegreya для текста. Всё встроено в exe.

use eframe::egui::{
    self, Color32, ColorImage, FontData, FontDefinitions, FontFamily, Mesh, Pos2, Rect, Response, Sense,
    Shape, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2,
};
use std::sync::Arc;

pub mod ink {
    use eframe::egui::Color32;
    pub const TITLE: Color32 = Color32::from_rgb(242, 209, 140);
    pub const TEXT: Color32 = Color32::from_rgb(230, 217, 189);
    pub const DIM: Color32 = Color32::from_rgb(168, 153, 128);
    pub const GOOD: Color32 = Color32::from_rgb(148, 214, 115);
    pub const BAD: Color32 = Color32::from_rgb(242, 115, 89);
    pub const GOLD: Color32 = Color32::from_rgb(245, 199, 97);
}

pub const TITLE_FONT: &str = "title";
pub const BOLD_FONT: &str = "bold";

pub struct Tex {
    pub handle: TextureHandle,
    pub size: Vec2,
}

pub struct Theme {
    pub wood: Tex,
    pub leather: Tex,
    /// normal, hover, pressed, disabled
    pub button: [Tex; 4],
    pub bar_frame: Tex,
    pub divider_line: Tex,
    pub divider_mid: Tex,
    pub vignette: Tex,
    pub wall: Tex,
}

macro_rules! ui_png {
    ($f:literal) => {
        include_bytes!(concat!("../../assets/ui/", $f))
    };
}

pub fn decode(bytes: &[u8]) -> ColorImage {
    let img = image::load_from_memory(bytes).expect("встроенная текстура").to_rgba8();
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

fn load(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Tex {
    let img = decode(bytes);
    let size = vec2(img.size[0] as f32, img.size[1] as f32);
    Tex { handle: ctx.load_texture(name, img, TextureOptions::LINEAR), size }
}

impl Theme {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            wood: load(ctx, "wood", ui_png!("panel_wood.png")),
            leather: load(ctx, "leather", ui_png!("panel_leather.png")),
            button: [
                load(ctx, "b0", ui_png!("button_normal.png")),
                load(ctx, "b1", ui_png!("button_hover.png")),
                load(ctx, "b2", ui_png!("button_pressed.png")),
                load(ctx, "b3", ui_png!("button_disabled.png")),
            ],
            bar_frame: load(ctx, "bar", ui_png!("bar_frame.png")),
            divider_line: load(ctx, "dl", ui_png!("divider_line.png")),
            divider_mid: load(ctx, "dm", ui_png!("divider_mid.png")),
            vignette: load(ctx, "vig", ui_png!("vignette.png")),
            // Тёсаный камень стен Херсонеса (фотоскан ambientCG, CC0) — фон окна.
            wall: load(ctx, "wall", include_bytes!("../../assets/textures/surface/ashlar_albedo.jpg")),
        }
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "alegreya", include_bytes!("../../assets/fonts/Alegreya-Regular.ttf"));
    add(&mut fonts, "alegreya_bold", include_bytes!("../../assets/fonts/Alegreya-Bold.ttf"));
    add(&mut fonts, "ponomar", include_bytes!("../../assets/fonts/Ponomar-Regular.ttf"));
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let chain = |first: &[&str]| {
        let mut v: Vec<String> = first.iter().map(|s| s.to_string()).collect();
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, chain(&["alegreya"]));
    fonts.families.insert(FontFamily::Name(BOLD_FONT.into()), chain(&["alegreya_bold"]));
    fonts.families.insert(FontFamily::Name(TITLE_FONT.into()), chain(&["ponomar", "alegreya"]));
    ctx.set_fonts(fonts);
}

pub fn font(size: f32) -> egui::FontId {
    egui::FontId::proportional(size)
}
pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(BOLD_FONT.into()))
}
pub fn title(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(TITLE_FONT.into()))
}

/// Девять частей: углы как есть, края и середина повторяются плиткой (как TextureSlicer в игре).
pub fn nine_slice(painter: &egui::Painter, tex: &Tex, rect: Rect, border: f32, tile: bool, tint: Color32) {
    let mut mesh = Mesh::with_texture(tex.handle.id());
    let ts = tex.size;
    let b = border.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let xs = [rect.min.x, rect.min.x + b, rect.max.x - b, rect.max.x];
    let ys = [rect.min.y, rect.min.y + b, rect.max.y - b, rect.max.y];
    let us = [0.0, border / ts.x, 1.0 - border / ts.x, 1.0];
    let vs = [0.0, border / ts.y, 1.0 - border / ts.y, 1.0];
    for j in 0..3 {
        for i in 0..3 {
            let dst = Rect::from_min_max(pos2(xs[i], ys[j]), pos2(xs[i + 1], ys[j + 1]));
            let uv = Rect::from_min_max(pos2(us[i], vs[j]), pos2(us[i + 1], vs[j + 1]));
            if dst.width() <= 0.0 || dst.height() <= 0.0 {
                continue;
            }
            // Сколько пикселей текстуры у этой части; углы не тянем, остальное — плиткой.
            let src = vec2(uv.width() * ts.x, uv.height() * ts.y);
            let tile_x = tile && i == 1;
            let tile_y = tile && j == 1;
            tiled(&mut mesh, dst, uv, src, tile_x, tile_y, tint);
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn tiled(mesh: &mut Mesh, dst: Rect, uv: Rect, src: Vec2, tx: bool, ty: bool, tint: Color32) {
    let step_x = if tx { src.x.max(1.0) } else { dst.width() };
    let step_y = if ty { src.y.max(1.0) } else { dst.height() };
    let mut y = dst.min.y;
    while y < dst.max.y - 0.01 {
        let h = step_y.min(dst.max.y - y);
        let mut x = dst.min.x;
        while x < dst.max.x - 0.01 {
            let w = step_x.min(dst.max.x - x);
            let u1 = if tx { uv.min.x + uv.width() * (w / step_x) } else { uv.max.x };
            let v1 = if ty { uv.min.y + uv.height() * (h / step_y) } else { uv.max.y };
            mesh.add_rect_with_uv(
                Rect::from_min_size(pos2(x, y), vec2(w, h)),
                Rect::from_min_max(uv.min, pos2(u1, v1)),
                tint,
            );
            x += w;
        }
        y += h;
    }
}

/// Фон: камень плиткой, притушенный, с виньеткой по краям.
pub fn backdrop(painter: &egui::Painter, th: &Theme, rect: Rect) {
    let mut mesh = Mesh::with_texture(th.wall.handle.id());
    // Текстура 1024 px, на экране плитка 512 — камень мелче и чётче, без размытия.
    tiled(&mut mesh, rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Vec2::splat(512.0), true, true, Color32::from_gray(92));
    painter.add(Shape::mesh(mesh));
    painter.image(th.vignette.handle.id(), rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    painter.image(th.vignette.handle.id(), rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::from_white_alpha(140));
}

/// Разделитель с бронзовым крестом посередине.
pub fn divider(ui: &mut Ui, th: &Theme, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
    let p = ui.painter();
    let mid = Rect::from_center_size(rect.center(), vec2(48.0, 24.0));
    for (a, b) in [(rect.min.x, mid.min.x), (mid.max.x, rect.max.x)] {
        let line = Rect::from_min_max(pos2(a, rect.center().y - 6.0), pos2(b, rect.center().y + 6.0));
        let mut mesh = Mesh::with_texture(th.divider_line.handle.id());
        tiled(&mut mesh, line, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), vec2(64.0, 12.0), true, false, Color32::WHITE);
        p.add(Shape::mesh(mesh));
    }
    p.image(th.divider_mid.handle.id(), mid, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
}

/// Кожаная кнопка в бронзовом ободке.
pub fn button(ui: &mut Ui, th: &Theme, text: &str, size: Vec2, font_size: f32, enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let idx = if !enabled {
        3
    } else if resp.is_pointer_button_down_on() {
        2
    } else if resp.hovered() {
        1
    } else {
        0
    };
    let p = ui.painter();
    nine_slice(p, &th.button[idx], rect, 14.0, false, Color32::WHITE);
    let color = if enabled { ink::TITLE } else { Color32::from_rgb(140, 135, 128) };
    let c = rect.center() + if idx == 2 { vec2(0.0, 1.5) } else { Vec2::ZERO };
    p.text(c + vec2(1.5, 2.0), egui::Align2::CENTER_CENTER, text, bold(font_size), Color32::from_black_alpha(190));
    p.text(c, egui::Align2::CENTER_CENTER, text, bold(font_size), color);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Полоса загрузки в бронзовой рамке.
pub fn bar(ui: &mut Ui, th: &Theme, width: f32, frac: Option<f32>, time: f64) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    let p = ui.painter();
    let inner = rect.shrink2(vec2(7.0, 7.0));
    p.rect_filled(inner, 2.0, Color32::from_rgb(22, 15, 10));
    let fill = |r: Rect| {
        let mut m = Mesh::default();
        let top = Color32::from_rgb(232, 178, 84);
        let bot = Color32::from_rgb(150, 92, 34);
        let i = m.vertices.len() as u32;
        m.colored_vertex(r.left_top(), top);
        m.colored_vertex(r.right_top(), top);
        m.colored_vertex(r.right_bottom(), bot);
        m.colored_vertex(r.left_bottom(), bot);
        m.add_triangle(i, i + 1, i + 2);
        m.add_triangle(i, i + 2, i + 3);
        p.add(Shape::mesh(m));
    };
    match frac {
        Some(f) => {
            let f = f.clamp(0.0, 1.0);
            if f > 0.0 {
                fill(Rect::from_min_size(inner.min, vec2(inner.width() * f, inner.height())));
            }
        }
        None => {
            // Неизвестно сколько — бегущий отрезок.
            let w = inner.width() * 0.25;
            let t = (time * 0.6).fract() as f32;
            let x0 = inner.min.x - w + (inner.width() + w) * t;
            let r = Rect::from_min_max(pos2(x0.max(inner.min.x), inner.min.y), pos2((x0 + w).min(inner.max.x), inner.max.y));
            if r.width() > 0.0 {
                fill(r);
            }
        }
    }
    nine_slice(p, &th.bar_frame, rect, 10.0, false, Color32::WHITE);
}

/// Текст с тенью, как заголовки в игре.
pub fn shadow_text(p: &egui::Painter, pos: Pos2, align: egui::Align2, text: &str, font: egui::FontId, color: Color32) {
    p.text(pos + vec2(2.0, 2.5), align, text, font.clone(), Color32::from_black_alpha(200));
    p.text(pos, align, text, font, color);
}
