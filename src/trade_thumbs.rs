//! Миниатюры товаров для строк окна торговли (клетка 30 px).
//!
//! Иконка инвентаря рисуется по клеткам предмета: у копья 1×6 клеток это картинка 128×768.
//! В квадратной клетке строки она сжималась до 4×26 px — древко и наконечник становились уже
//! пикселя, а без мип-уровней видеокарта брала редкие отсчёты и предмет пропадал совсем
//! (пустые клетки у контоса, менавлона, пик; от плюмбаты — одна чёрточка).
//!
//! Поэтому для строк собираем свою миниатюру на процессоре, один раз на вид предмета:
//! * длинный предмет кладём по диагонали клетки (вершиной вверх-вправо), обрезав пустые поля;
//!   у очень длинного (копья, пики) оставляем широкий конец — наконечник или рукоять с гардой;
//! * уменьшаем усреднением всех попавших пикселей в линейном цвете (как мип-уровень), так что
//!   тонкие детали не исчезают, а становятся полупрозрачными.
//!
//! Остальные предметы остаются в своей рамке, только уменьшены честнее (без «лесенки»).
//! Предметы с временной плоской иконкой (без 3D-модели) не трогаем: они нарисованы толстыми линиями.

use crate::icons::ItemIcons;
use crate::items::ItemBaseId;
use bevy::asset::RenderAssetUsages;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Сторона миниатюры в пикселях: вдвое больше клетки строки (26 px) — запас под масштаб экрана.
pub const THUMB_PX: u32 = 52;
/// С какого соотношения сторон (по непрозрачной части) предмет кладём по диагонали
/// (мечи, ножи, булавы, луки, копья; склянки и топоры остаются как есть).
const DIAGONAL_FROM: f32 = 4.0;
/// Предмет длиннее стольких ширин (копья, пики, дротики) показываем не целиком...
const CROP_FROM: f32 = 14.0;
/// ...а куском у широкого конца такой длины (в ширинах): наконечник с частью древка.
const CROP_TO: f32 = 10.0;
/// Прозрачность, ниже которой пиксель считается пустым полем.
const ALPHA_EMPTY: u8 = 8;

pub struct TradeThumbsPlugin;

impl Plugin for TradeThumbsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemThumbs>().add_systems(PostUpdate, apply_thumbs.before(bevy::ui::UiSystems::Prepare));
        if std::env::var("MRPG_SHOT_UI").is_ok_and(|s| s.starts_with("trade:")) {
            app.add_systems(Update, shot_open_trade);
        }
    }
}

/// Снимок окна торговли: `MRPG_SHOT_UI=trade:<ключ торговца>` (вместе с `MRPG_SHOT`) — как только
/// игрок появился и торговцы пришли с сервера, заговорить с этим торговцем (как по «E»).
fn shot_open_trade(
    state: Res<crate::trade_client::TradeState>,
    player: Query<(), With<crate::player::LocalPlayer>>,
    client: Option<ResMut<bevy_renet::RenetClient>>,
    mut asked: Local<u32>,
) {
    let (Some(mut client), false) = (client, state.merchants.is_empty() || player.is_empty()) else { return };
    // Спрашиваем каждые 20 кадров, пока окно не открылось (до переноса к торговцу сервер откажет).
    if state.view.is_some() {
        return;
    }
    *asked += 1;
    if *asked % 20 != 1 {
        return;
    }
    let key = std::env::var("MRPG_SHOT_UI").unwrap_or_default();
    if let Some(id) = crate::trade::merchant_by_key(key.trim_start_matches("trade:")) {
        crate::trade_client::send(&mut client, &crate::trade_net::TradeClientMessage::Open { merchant: id });
    }
}

/// Иконка в строке торговли: подменим её миниатюрой, когда исходная картинка загрузится.
#[derive(Component)]
pub struct RowIcon {
    pub base: ItemBaseId,
    /// Сторона клетки под иконку (px).
    pub side: f32,
}

/// Готовые миниатюры по виду предмета (`None` — у предмета своя плоская иконка, оставляем её).
#[derive(Resource, Default)]
pub struct ItemThumbs(HashMap<ItemBaseId, Option<Handle<Image>>>);

fn apply_thumbs(
    mut commands: Commands,
    mut thumbs: ResMut<ItemThumbs>,
    icons: Option<Res<ItemIcons>>,
    mut images: ResMut<Assets<Image>>,
    mut rows: Query<(Entity, &RowIcon, &mut ImageNode, &mut Node)>,
) {
    let Some(icons) = icons else { return };
    for (e, row, mut image, mut node) in &mut rows {
        let thumb = match thumbs.0.get(&row.base) {
            Some(t) => t.clone(),
            None => {
                if crate::icons::own_art(row.base).is_none() {
                    thumbs.0.insert(row.base, None);
                    None
                } else {
                    // Исходная иконка ещё грузится — попробуем в следующем кадре.
                    let Some(src) = images.get(&icons.get(row.base)) else { continue };
                    let made = thumb_image(src).map(|img| images.add(img));
                    thumbs.0.insert(row.base, made.clone());
                    made
                }
            }
        };
        if let Some(h) = thumb {
            image.image = h;
            node.width = Val::Px(row.side);
            node.height = Val::Px(row.side);
        }
        commands.entity(e).remove::<RowIcon>();
    }
}

/// Миниатюра из загруженной иконки (нужны пиксели RGBA8 на процессоре).
fn thumb_image(src: &Image) -> Option<Image> {
    let fmt = src.texture_descriptor.format;
    if !matches!(fmt, TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm) {
        return None;
    }
    let data = src.data.as_ref()?;
    let (w, h) = (src.width(), src.height());
    if data.len() != (w * h * 4) as usize {
        return None;
    }
    let px = make_thumb(data, w, h, THUMB_PX);
    Some(Image::new(
        Extent3d { width: THUMB_PX, height: THUMB_PX, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px,
        fmt,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

/// Как кладём исходную картинку в квадрат: центр взятой части в исходнике, масштаб и поворот.
#[derive(Debug, Clone, Copy)]
struct Placement {
    center: Vec2,
    /// Пикселей миниатюры на пиксель исходника.
    scale: f32,
    /// Положить по диагонали (вершина исходника — вверх-вправо).
    diagonal: bool,
    /// Взятая часть исходника: min и max (остальное прозрачно).
    min: Vec2,
    max: Vec2,
}

fn opaque(src: &[u8], w: u32, x: u32, y: u32) -> bool {
    src[((y * w + x) * 4 + 3) as usize] > ALPHA_EMPTY
}

fn place(src: &[u8], w: u32, h: u32, side: u32) -> Placement {
    let s = side as f32;
    let whole = Placement { center: Vec2::new(w as f32, h as f32) * 0.5, scale: (s / w as f32).min(s / h as f32), diagonal: false, min: Vec2::ZERO, max: Vec2::new(w as f32, h as f32) };
    // Непрозрачная рамка.
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            if opaque(src, w, x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return whole;
    }
    let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
    // Лежачие длинные предметы (шире, чем выше) в иконках не встречаются: их оставляем как есть.
    if bh < bw * DIAGONAL_FROM {
        return whole;
    }
    // Очень длинный: берём кусок у широкого конца (где наконечник или гарда).
    let len = if bh > bw * CROP_FROM { bw * CROP_TO } else { bh };
    let (mut top, mut bottom) = (y0 as f32, y1 as f32);
    if len < bh {
        let widest = (y0..y1).max_by_key(|&y| (x0..x1).filter(|&x| opaque(src, w, x, y)).count()).unwrap_or(y0);
        if ((widest - y0) as f32) < bh * 0.5 {
            bottom = top + len;
        } else {
            top = bottom - len;
        }
    }
    let center = Vec2::new((x0 + x1) as f32 * 0.5, (top + bottom) * 0.5);
    // Повёрнутый на 45° прямоугольник bw × len занимает по каждой оси (bw + len) / √2; поле — 1 px.
    let scale = (s - 2.0) * std::f32::consts::SQRT_2 / (bw + len);
    Placement { center, scale, diagonal: true, min: Vec2::new(x0 as f32, top), max: Vec2::new(x1 as f32, bottom) }
}

/// Квадратная миниатюра `side`×`side` (RGBA8, sRGB) из картинки `w`×`h`.
pub fn make_thumb(src: &[u8], w: u32, h: u32, side: u32) -> Vec<u8> {
    let lut: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
    let p = place(src, w, h, side);
    // Подотсчётов на сторону пикселя: по 4 на каждый попавший в него пиксель исходника (не больше 16).
    let n = ((4.0 / p.scale).ceil() as u32).clamp(2, 16);
    let half = side as f32 * 0.5;
    let k = std::f32::consts::FRAC_1_SQRT_2;
    let mut out = vec![0u8; (side * side * 4) as usize];
    for oy in 0..side {
        for ox in 0..side {
            let mut acc = [0.0f32; 4];
            for sy in 0..n {
                for sx in 0..n {
                    let q = Vec2::new(ox as f32 + (sx as f32 + 0.5) / n as f32, oy as f32 + (sy as f32 + 0.5) / n as f32) - Vec2::splat(half);
                    let d = q / p.scale;
                    // Обратный поворот на 45°: «вверх» исходника смотрит вверх-вправо.
                    let r = if p.diagonal { Vec2::new(k * (d.x + d.y), k * (d.y - d.x)) } else { d };
                    let at = p.center + r;
                    if at.x < p.min.x || at.y < p.min.y || at.x >= p.max.x || at.y >= p.max.y {
                        continue;
                    }
                    let i = ((at.y as u32 * w + at.x as u32) * 4) as usize;
                    let a = src[i + 3] as f32 / 255.0;
                    acc[0] += lut[src[i] as usize] * a;
                    acc[1] += lut[src[i + 1] as usize] * a;
                    acc[2] += lut[src[i + 2] as usize] * a;
                    acc[3] += a;
                }
            }
            if acc[3] <= 0.0 {
                continue;
            }
            let o = ((oy * side + ox) * 4) as usize;
            for c in 0..3 {
                out[o + c] = (linear_to_srgb(acc[c] / acc[3]) * 255.0 + 0.5) as u8;
            }
            out[o + 3] = (acc[3] / (n * n) as f32 * 255.0 + 0.5).min(255.0) as u8;
        }
    }
    out
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};

    fn load(key: &str) -> (Vec<u8>, u32, u32) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(crate::icons::icon_path(key));
        let bytes = std::fs::read(&path).unwrap();
        let img = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).unwrap();
        let img = thumb_image(&img).expect("иконка не RGBA8");
        let (w, h) = (img.width(), img.height());
        (img.data.unwrap(), w, h)
    }

    /// Сколько пикселей миниатюры заметно непрозрачны.
    fn visible(px: &[u8]) -> usize {
        px.chunks(4).filter(|p| p[3] > 96).count()
    }

    #[test]
    fn long_weapons_are_visible_in_trade_rows() {
        // Раньше в клетке строки от копья оставалось ~5 непрозрачных пикселей на 26×26.
        for key in ["kontos", "menavlon", "pike", "rhomb_spear", "plumbata", "spatha"] {
            let (px, w, h) = load(key);
            assert_eq!((w, h), (THUMB_PX, THUMB_PX));
            let n = visible(&px);
            assert!(n > 120, "{key}: в миниатюре видно всего {n} пикселей");
        }
    }

    #[test]
    fn spear_lies_diagonally_with_its_head_up_right() {
        let (px, w, _) = load("kontos");
        let at = |x: u32, y: u32| px[((y * w + x) * 4 + 3) as usize];
        let (mut ur, mut ul) = (0u32, 0u32);
        for y in 0..w / 2 {
            for x in 0..w / 2 {
                ul += at(x, y) as u32;
                ur += at(x + w / 2, y) as u32;
            }
        }
        assert!(ur > ul * 4, "наконечник должен быть вверху справа: {ur} против {ul}");
    }

    #[test]
    fn compact_items_keep_their_frame() {
        // Квадратная иконка не поворачивается и не обрезается: только честно уменьшается.
        let src: Vec<u8> = (0..128 * 128).flat_map(|i| if (i / 128) % 2 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] }).collect();
        let p = place(&src, 128, 128, THUMB_PX);
        assert!(!p.diagonal);
        let out = make_thumb(&src, 128, 128, THUMB_PX);
        // Полосы в 1 пиксель усредняются в ровный серый, а не в «лесенку» из чёрного и белого.
        for c in out.chunks(4) {
            assert_eq!(c[3], 255);
            assert!((150..=220).contains(&c[0]), "{}", c[0]);
        }
    }
}
