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
use bevy::tasks::{AsyncComputeTaskPool, Task};

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
///
/// Картинок у моделей сотни (люди, одежда, оружие, все предметы), и приходят они пачками по
/// сотне-две за кадр: считать мипы прямо в кадре — заморозка на секунду. Поэтому пикселы
/// уходят в фоновую задачу, а картинку до конца расчёта не отдаём видеокарте (без
/// `RENDER_WORLD` её не извлекают): модель появляется на долю секунды позже, уже с мипами.
fn mip_model_images(mut events: MessageReader<AssetEvent<Image>>, server: Res<AssetServer>, mut images: ResMut<Assets<Image>>, mut pending: Local<Vec<(AssetId<Image>, bool, Task<Image>)>>) {
    for ev in events.read() {
        let AssetEvent::Added { id } = ev else { continue };
        let from_models = server.get_path(*id).is_some_and(|p| p.path().starts_with("models") || p.path().starts_with("textures/library"));
        if !from_models || pending.iter().any(|p| p.0 == *id) {
            continue;
        }
        let Some(img) = images.get(*id) else { continue };
        if img.texture_descriptor.mip_level_count > 1 || img.data.is_none() {
            continue;
        }
        let Some(mut img) = images.get_mut(*id) else { continue };
        let repeat = !matches!(&img.sampler, ImageSampler::Descriptor(d) if d.address_mode_u == ImageAddressMode::ClampToEdge);
        // Пикселы забираем в задачу, остальное (формат, размер) — копия.
        let data = img.data.take();
        let mut work = img.clone();
        work.data = data;
        img.asset_usage = RenderAssetUsages::MAIN_WORLD;
        let cap = crate::settings::startup().texture_cap();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            // Карты нормалей и шероховатости из glTF не отличить по имени (метки вида «Texture3»),
            // поэтому линейные картинки просто усредняются: нормали вдали чуть сглаживаются — это и нужно.
            generate_mips(&mut work, false);
            // Настройка «Текстуры»: лишние верхние уровни не грузим в видеопамять.
            cap_size(&mut work, cap);
            work
        });
        pending.push((*id, repeat, task));
    }
    pending.retain_mut(|(id, repeat, task)| {
        if !task.is_finished() {
            return true;
        }
        let work = bevy::tasks::block_on(task);
        let Some(mut img) = images.get_mut(*id) else { return false };
        img.data = work.data;
        img.texture_descriptor.size = work.texture_descriptor.size;
        img.texture_descriptor.mip_level_count = work.texture_descriptor.mip_level_count;
        img.sampler = quality_sampler(*repeat);
        // Теперь — в видеокарту, а копию в оперативной памяти после загрузки отпускаем.
        img.asset_usage = RenderAssetUsages::RENDER_WORLD;
        false
    });
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

/// Перевод линейной яркости в sRGB-байт. Не формулой (`powf` на каждый канал — самое дорогое
/// в мипах), а по порогам: байт `k` начинается там, где формула с округлением даёт `k` (середина
/// между соседними байтами). Результат тот же, что у формулы.
struct SrgbEncoder {
    /// `t[k]` — наименьшая линейная яркость, которая округляется до байта `k` (`t[0]` и `t[256]` — заглушки).
    t: [f32; 257],
    /// Байт для яркости `i / 4096`: отсюда до ответа не больше пары шагов по порогам.
    start: [u8; 4097],
}

impl SrgbEncoder {
    fn get() -> &'static SrgbEncoder {
        use std::sync::OnceLock;
        static TABLE: OnceLock<SrgbEncoder> = OnceLock::new();
        TABLE.get_or_init(|| {
            let mut t = [f32::NEG_INFINITY; 257];
            for (k, v) in t.iter_mut().enumerate().take(256).skip(1) {
                let c = (k as f64 - 0.5) / 255.0;
                *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } as f32;
            }
            t[256] = f32::INFINITY;
            let mut start = [0u8; 4097];
            for (i, s) in start.iter_mut().enumerate() {
                *s = (t[1..256].partition_point(|&x| x <= i as f32 / 4096.0)) as u8;
            }
            SrgbEncoder { t, start }
        })
    }

    #[inline]
    fn encode(&self, l: f32) -> u8 {
        let mut b = self.start[((l.max(0.0) * 4096.0) as usize).min(4096)] as usize;
        while l >= self.t[b + 1] {
            b += 1;
        }
        b as u8
    }
}

#[cfg(test)]
fn lin_to_srgb(l: f32) -> u8 {
    SrgbEncoder::get().encode(l)
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
    let (w, h, layers) = (size.width as usize, size.height as usize, size.depth_or_array_layers as usize);
    let Some(data) = img.data.take() else { return };
    let layer_bytes = w * h * 4;
    if data.len() != layer_bytes * layers {
        img.data = Some(data);
        return;
    }
    let levels = mip_count(w as u32, h as u32);
    let dims: Vec<(usize, usize)> = (0..levels).map(|l| ((w >> l).max(1), (h >> l).max(1))).collect();
    let per_layer: usize = dims.iter().map(|(x, y)| x * y * 4).sum();
    // Один слой (картинки моделей) — уровни дописываются прямо за исходными пикселями, без копии.
    let mut out = if layers == 1 {
        let mut d = data;
        d.resize(per_layer, 0);
        d
    } else {
        let mut d = vec![0u8; per_layer * layers];
        for layer in 0..layers {
            d[layer * per_layer..layer * per_layer + layer_bytes].copy_from_slice(&data[layer * layer_bytes..(layer + 1) * layer_bytes]);
        }
        d
    };
    for chain in out.chunks_exact_mut(per_layer) {
        let mut at = 0;
        for l in 1..levels as usize {
            let (pw, ph) = dims[l - 1];
            let (done, rest) = chain.split_at_mut(at + pw * ph * 4);
            let (nw, nh) = dims[l];
            downsample_into(&done[at..], pw, ph, srgb, normal, &mut rest[..nw * nh * 4]);
            at += pw * ph * 4;
        }
    }
    img.data = Some(out);
    img.texture_descriptor.mip_level_count = levels;
}

/// Уменьшение вдвое фильтром 2×2 (для проверок — в новый буфер).
#[cfg(test)]
fn downsample(src: &[u8], w: u32, h: u32, srgb: bool, normal: bool) -> Vec<u8> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut dst = vec![0u8; (nw * nh * 4) as usize];
    downsample_into(src, w as usize, h as usize, srgb, normal, &mut dst);
    dst
}

/// Уменьшение вдвое фильтром 2×2: `src` (w×h) -> `dst` (w/2 × h/2, не меньше 1).
fn downsample_into(src: &[u8], w: usize, h: usize, srgb: bool, normal: bool, dst: &mut [u8]) {
    match (normal, srgb) {
        (true, _) => reduce(src, w, h, dst, normal_pixel),
        (false, true) => {
            let (lut, enc) = (srgb_to_lin_table(), SrgbEncoder::get());
            reduce(src, w, h, dst, |p, out| {
                // Цвет sRGB усредняем как свет (в линейном виде), прозрачность — как число.
                for c in 0..3 {
                    out[c] = enc.encode((lut[p[0][c] as usize] + lut[p[1][c] as usize] + lut[p[2][c] as usize] + lut[p[3][c] as usize]) * 0.25);
                }
                out[3] = ((p[0][3] as u32 + p[1][3] as u32 + p[2][3] as u32 + p[3][3] as u32 + 2) / 4) as u8;
            })
        }
        (false, false) => reduce(src, w, h, dst, |p, out| {
            for c in 0..4 {
                out[c] = ((p[0][c] as u32 + p[1][c] as u32 + p[2][c] as u32 + p[3][c] as u32 + 2) / 4) as u8;
            }
        }),
    }
}

/// Обходит 2×2-блоки `src` и пишет по пикселю в `dst` (`pixel` получает четыре RGBA-пикселя блока).
#[inline(always)]
fn reduce(src: &[u8], w: usize, h: usize, dst: &mut [u8], pixel: impl Fn([&[u8]; 4], &mut [u8])) {
    let nw = (w / 2).max(1);
    for (y, row) in dst.chunks_exact_mut(nw * 4).enumerate() {
        let r0 = &src[(y * 2).min(h - 1) * w * 4..][..w * 4];
        let r1 = &src[(y * 2 + 1).min(h - 1) * w * 4..][..w * 4];
        if w >= 2 {
            // Пары столбцов 2x, 2x+1 (у нечётной ширины последний столбец отпадает).
            for ((a, b), out) in r0.chunks_exact(8).zip(r1.chunks_exact(8)).zip(row.chunks_exact_mut(4)) {
                pixel([&a[..4], &a[4..], &b[..4], &b[4..]], out);
            }
        } else {
            pixel([r0, r0, r1, r1], row);
        }
    }
}

/// Пиксель карты нормалей: xy из [0,1] -> [-1,1], z восстанавливаем; длина сокращается
/// на неровных участках, это честно уменьшает рельеф вдали.
fn normal_pixel(p: [&[u8]; 4], out: &mut [u8]) {
    let mut acc = [0.0f32; 4];
    for q in p {
        for c in 0..4 {
            acc[c] += q[c] as f32 / 255.0;
        }
    }
    for v in &mut acc {
        *v *= 0.25;
    }
    let nx = acc[0] * 2.0 - 1.0;
    let ny = acc[1] * 2.0 - 1.0;
    let nz = (acc[2] * 2.0 - 1.0).max(0.05);
    let l = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-4);
    acc[0] = nx / l * 0.5 + 0.5;
    acc[1] = ny / l * 0.5 + 0.5;
    acc[2] = nz / l * 0.5 + 0.5;
    for c in 0..4 {
        out[c] = (acc[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
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

    /// Прежний расчёт мипа (формулой, через `powf`) — для сверки быстрого.
    fn downsample_by_formula(src: &[u8], w: u32, h: u32, srgb: bool) -> Vec<u8> {
        let lin = |v: u8| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        let to_srgb = |l: f32| {
            let c = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
            (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
        };
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut dst = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = ((((y * 2 + dy).min(h - 1)) * w + (x * 2 + dx).min(w - 1)) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += if srgb && c < 3 { lin(src[i + c]) } else { src[i + c] as f32 / 255.0 };
                    }
                }
                for c in 0..4 {
                    let v = acc[c] * 0.25;
                    dst[((y * nw + x) * 4) as usize + c] = if srgb && c < 3 { to_srgb(v) } else { (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 };
                }
            }
        }
        dst
    }

    #[test]
    fn fast_mips_match_the_formula() {
        // Шумная картинка нечётного размера: края, тёмные и светлые тона.
        let (w, h) = (37u32, 21u32);
        let mut seed = 0x1234_5678u32;
        let src: Vec<u8> = (0..w * h * 4)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed >> 24) as u8
            })
            .collect();
        for srgb in [true, false] {
            let fast = downsample(&src, w, h, srgb, false);
            let slow = downsample_by_formula(&src, w, h, srgb);
            assert_eq!(fast.len(), slow.len());
            let off: Vec<(u8, u8)> = fast.iter().zip(&slow).filter(|(a, b)| a != b).map(|(a, b)| (*a, *b)).collect();
            // Расходиться может только округление на самой границе между байтами.
            assert!(off.iter().all(|(a, b)| a.abs_diff(*b) == 1), "{off:?}");
            assert!(off.len() * 200 < fast.len(), "srgb {srgb}: {} из {}", off.len(), fast.len());
        }
        // Порог за порогом: байт из таблицы совпадает с формулой.
        for k in 0..=255u8 {
            let c = k as f32 / 255.0;
            let l = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            assert_eq!(lin_to_srgb(l), k);
        }
        assert_eq!(lin_to_srgb(0.0), 0);
        assert_eq!(lin_to_srgb(1.0), 255);
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
