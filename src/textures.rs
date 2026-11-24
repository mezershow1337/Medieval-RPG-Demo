//! Качество текстур: мип-уровни и анизотропная фильтрация.
//!
//! Bevy не строит мип-уровни сам: картинка из glTF (одежда, кожа, оружие, предметы)
//! загружается одним уровнем, и вдали она «рябит» и выглядит пиксельной кашей.
//! Здесь мы строим цепочку мип-уровней на процессоре (с гамма-коррекцией для sRGB,
//! с перенормировкой для карт нормалей) и включаем анизотропную фильтрацию ×16 —
//! поверхности под острым углом (пол, мостовая, стены вдоль улицы) остаются чёткими.
//!
//! Для фотоматериалов уровня (камень, дерево, земля…) см. `surface_textures`.

use bevy::asset::{AssetEvent, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

pub struct TexturesPlugin;

impl Plugin for TexturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, mip_model_images);
    }
}

/// Сэмплер с трилинейной и анизотропной фильтрацией.
pub fn quality_sampler(repeat: bool) -> ImageSampler {
    let address = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    })
}

/// Картинки моделей (assets/models/**) получают мип-уровни и анизотропию сразу после загрузки.
fn mip_model_images(mut events: MessageReader<AssetEvent<Image>>, server: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    for ev in events.read() {
        let AssetEvent::Added { id } = ev else { continue };
        let from_models = server.get_path(*id).is_some_and(|p| p.path().starts_with("models"));
        if !from_models {
            continue;
        }
        let Some(img) = images.get(*id) else { continue };
        if img.texture_descriptor.mip_level_count > 1 || img.data.is_none() {
            continue;
        }
        let Some(mut img) = images.get_mut(*id) else { continue };
        let repeat = !matches!(&img.sampler, ImageSampler::Descriptor(d) if d.address_mode_u == ImageAddressMode::ClampToEdge);
        // Карты нормалей и шероховатости из glTF не отличить по имени (метки вида «Texture3»),
        // поэтому линейные картинки просто усредняются: нормали вдали чуть сглаживаются — это и нужно.
        generate_mips(&mut img, false);
        // Настройка «Текстуры»: лишние верхние уровни не грузим в видеопамять.
        cap_size(&mut img, crate::settings::startup().texture_cap());
        img.sampler = quality_sampler(repeat);
        // Копия в оперативной памяти больше не нужна — после загрузки в видеокарту её отпускаем.
        img.asset_usage = RenderAssetUsages::RENDER_WORLD;
    }
}

/// Отбрасывает верхние мип-уровни, пока большая сторона не станет не больше `cap`
/// (0 — без ограничения). Картинка должна уже иметь мип-уровни (`generate_mips`).
pub fn cap_size(img: &mut Image, cap: u32) {
    if cap == 0 {
        return;
    }
    let size = img.texture_descriptor.size;
    let (w, h, layers) = (size.width, size.height, size.depth_or_array_layers as usize);
    let levels = img.texture_descriptor.mip_level_count;
    let mut drop = 0;
    while drop + 1 < levels && (w >> drop).max(h >> drop) > cap {
        drop += 1;
    }
    if drop == 0 {
        return;
    }
    let Some(data) = img.data.take() else { return };
    let level_bytes = |l: u32| (((w >> l).max(1)) * ((h >> l).max(1)) * 4) as usize;
    let per_layer: usize = (0..levels).map(level_bytes).sum();
    if data.len() != per_layer * layers {
        img.data = Some(data);
        return;
    }
    let skip: usize = (0..drop).map(level_bytes).sum();
    let mut out = Vec::with_capacity((per_layer - skip) * layers);
    for layer in 0..layers {
        out.extend_from_slice(&data[layer * per_layer + skip..(layer + 1) * per_layer]);
    }
    img.data = Some(out);
    img.texture_descriptor.size.width = (w >> drop).max(1);
    img.texture_descriptor.size.height = (h >> drop).max(1);
    img.texture_descriptor.mip_level_count = levels - drop;
}

// sRGB <-> линейное через таблицу (на мип-уровнях усредняем свет, а не байты).
fn srgb_to_lin_table() -> &'static [f32; 256] {
    use std::sync::OnceLock;
    static T: OnceLock<[f32; 256]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0.0; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}

fn lin_to_srgb(l: f32) -> u8 {
    let c = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Число мип-уровней для размера.
pub fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// Строит полную цепочку мип-уровней для RGBA8-картинки (2D или массив слоёв).
/// `normal` — карта нормалей в каналах RG(B): вектор усредняется и перенормируется.
pub fn generate_mips(img: &mut Image, normal: bool) {
    let fmt = img.texture_descriptor.format;
    let srgb = match fmt {
        TextureFormat::Rgba8UnormSrgb => true,
        TextureFormat::Rgba8Unorm => false,
        _ => return,
    };
    let size = img.texture_descriptor.size;
    let (w, h, layers) = (size.width, size.height, size.depth_or_array_layers);
    let Some(data) = img.data.take() else { return };
    let layer_bytes = (w * h * 4) as usize;
    if data.len() != layer_bytes * layers as usize {
        img.data = Some(data);
        return;
    }
    let levels = mip_count(w, h);
    let mut out = Vec::with_capacity(data.len() * 4 / 3 + 64);
    for layer in 0..layers as usize {
        let base = &data[layer * layer_bytes..(layer + 1) * layer_bytes];
        out.extend_from_slice(base);
        let mut prev = base.to_vec();
        let (mut pw, mut ph) = (w, h);
        for _ in 1..levels {
            let next = downsample(&prev, pw, ph, srgb, normal);
            pw = (pw / 2).max(1);
            ph = (ph / 2).max(1);
            out.extend_from_slice(&next);
            prev = next;
        }
    }
    img.data = Some(out);
    img.texture_descriptor.mip_level_count = levels;
}

/// Уменьшение вдвое фильтром 2×2.
fn downsample(src: &[u8], w: u32, h: u32, srgb: bool, normal: bool) -> Vec<u8> {
    let lut = srgb_to_lin_table();
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut dst = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0.0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(w - 1);
                let sy = (y * 2 + dy).min(h - 1);
                let i = ((sy * w + sx) * 4) as usize;
                for c in 0..4 {
                    let v = src[i + c];
                    acc[c] += if srgb && c < 3 { lut[v as usize] } else { v as f32 / 255.0 };
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for v in &mut acc {
                *v *= 0.25;
            }
            if normal {
                // Нормаль: xy из [0,1] -> [-1,1], z восстанавливаем; длина сокращается
                // на неровных участках, это честно уменьшает рельеф вдали.
                let nx = acc[0] * 2.0 - 1.0;
                let ny = acc[1] * 2.0 - 1.0;
                let nz = (acc[2] * 2.0 - 1.0).max(0.05);
                let l = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-4);
                acc[0] = nx / l * 0.5 + 0.5;
                acc[1] = ny / l * 0.5 + 0.5;
                acc[2] = nz / l * 0.5 + 0.5;
            }
            for c in 0..4 {
                dst[o + c] = if srgb && c < 3 { lin_to_srgb(acc[c]) } else { (acc[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8 };
            }
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::{Extent3d, TextureDimension};

    #[test]
    fn mips_cover_every_layer() {
        let size = Extent3d { width: 8, height: 4, depth_or_array_layers: 3 };
        let mut img = Image::new(size, TextureDimension::D2, vec![200u8; 8 * 4 * 4 * 3], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        generate_mips(&mut img, false);
        assert_eq!(img.texture_descriptor.mip_level_count, 4);
        // 8x4 + 4x2 + 2x1 + 1x1 пикселей на слой.
        assert_eq!(img.data.as_ref().unwrap().len(), (32 + 8 + 2 + 1) * 4 * 3);
        // Ровный цвет не меняется на мип-уровнях.
        assert!(img.data.as_ref().unwrap().iter().all(|&v| v == 200));
    }

    #[test]
    fn cap_drops_top_levels_of_every_layer() {
        let size = Extent3d { width: 8, height: 8, depth_or_array_layers: 2 };
        let mut data = vec![10u8; 8 * 8 * 4 * 2];
        data[8 * 8 * 4..].fill(90);
        let mut img = Image::new(size, TextureDimension::D2, data, TextureFormat::Rgba8Unorm, RenderAssetUsages::default());
        generate_mips(&mut img, false);
        cap_size(&mut img, 2);
        assert_eq!(img.texture_descriptor.size.width, 2);
        assert_eq!(img.texture_descriptor.mip_level_count, 2);
        let d = img.data.as_ref().unwrap();
        assert_eq!(d.len(), (4 + 1) * 4 * 2);
        assert!(d[..20].iter().all(|&v| v == 10) && d[20..].iter().all(|&v| v == 90));
    }
}
