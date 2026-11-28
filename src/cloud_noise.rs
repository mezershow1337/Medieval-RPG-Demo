//! Шум для объёмных облаков: одна бесшовная картинка 512², которую шейдер неба читает много раз
//! на луч (считать столько шума формулой в каждом шаге луча слишком дорого).
//!
//! Каналы (приём из облаков Horizon Zero Dawn — Перлин, «выеденный» Ворли):
//! * R — форма облаков: шум Перлина, смешанный с ячеистым (круглые клубы);
//! * G — ячеистый шум средней частоты: им выедаются края (клубы и клочья);
//! * B — ячеистый шум высокой частоты: мелкая рвань по кромке;
//! * A — плавный шум Перлина: карта «погоды» (где гряды облаков, где просветы) и пряди перистых.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub const SIZE: usize = 512;

fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ seed.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^ (h >> 16)
}

fn unit(x: i32, y: i32, seed: u32) -> f32 {
    (hash(x, y, seed) & 0xFFFFFF) as f32 / 16777216.0
}

/// Шум Перлина с периодом `period` клеток, примерно −1..1.
fn perlin(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let grad = |cx: i32, cy: i32, dx: f32, dy: f32| {
        let a = unit(cx.rem_euclid(period), cy.rem_euclid(period), seed) * std::f32::consts::TAU;
        a.cos() * dx + a.sin() * dy
    };
    let (u, v) = (fade(fx), fade(fy));
    let a = grad(ix, iy, fx, fy);
    let b = grad(ix + 1, iy, fx - 1.0, fy);
    let c = grad(ix, iy + 1, fx, fy - 1.0);
    let d = grad(ix + 1, iy + 1, fx - 1.0, fy - 1.0);
    (a + (b - a) * u + (c + (d - c) * u - (a + (b - a) * u)) * v) * 1.41
}

/// Ячеистый шум (Ворли) с периодом `period`: расстояние до ближайшей точки, 0..1.
fn worley(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let mut best = f32::MAX;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (cx, cy) = (ix + dx, iy + dy);
            let (wx, wy) = (cx.rem_euclid(period), cy.rem_euclid(period));
            let px = cx as f32 + unit(wx, wy, seed);
            let py = cy as f32 + unit(wx, wy, seed ^ 0x9E37);
            best = best.min((px - x).powi(2) + (py - y).powi(2));
        }
    }
    best.sqrt().min(1.0)
}

fn perlin_fbm(u: f32, v: f32, period: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut p) = (0.0, 0.5, 0.0, period);
    for o in 0..octaves {
        sum += amp * perlin(u * p as f32, v * p as f32, p, seed + o * 7);
        norm += amp;
        amp *= 0.5;
        p *= 2;
    }
    sum / norm
}

/// «Обратный» Ворли в три октавы: 1 в серединах клубов, 0 на их границах.
fn worley_fbm(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let w = |p: i32, s: u32| 1.0 - worley(u * p as f32, v * p as f32, p, s);
    w(period, seed) * 0.625 + w(period * 2, seed + 11) * 0.25 + w(period * 4, seed + 23) * 0.125
}

fn remap(x: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    c + (x - a) / (b - a) * (d - c)
}

/// Байты RGBA8 картинки шума (без мип-уровней).
pub fn cloud_noise_pixels() -> Vec<u8> {
    let n = SIZE;
    let mut data = vec![0u8; n * n * 4];
    let rows: Vec<&mut [u8]> = data.chunks_mut(n * 4).collect();
    let threads = std::thread::available_parallelism().map_or(4, |t| t.get()).min(8);
    let per = n.div_ceil(threads);
    std::thread::scope(|s| {
        let mut rows = rows;
        let mut y0 = 0;
        while !rows.is_empty() {
            let take = per.min(rows.len());
            let part: Vec<&mut [u8]> = rows.drain(..take).collect();
            let start = y0;
            y0 += take;
            s.spawn(move || {
                for (k, row) in part.into_iter().enumerate() {
                    let v = (start + k) as f32 / n as f32;
                    for (x, px) in row.chunks_exact_mut(4).enumerate() {
                        let u = x as f32 / n as f32;
                        let p = perlin_fbm(u, v, 6, 6, 1) * 0.5 + 0.5;
                        let w = worley_fbm(u, v, 6, 31);
                        // Перлин, поджатый снизу ячеистым шумом: кучевые с круглыми макушками.
                        let shape = remap(p, 0.0, 1.0, w * 0.55 - 0.1, 1.0);
                        let r = ((shape - 0.25) / 0.5).clamp(0.0, 1.0);
                        let g = worley_fbm(u, v, 8, 57);
                        let b = worley_fbm(u, v, 20, 83);
                        let a = ((perlin_fbm(u, v, 3, 5, 113) * 0.5 + 0.5 - 0.2) / 0.6).clamp(0.0, 1.0);
                        px.copy_from_slice(&[(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (a * 255.0) as u8]);
                    }
                }
            });
        }
    });
    data
}

/// Картинка шума для материала неба: бесшовная, с мип-уровнями (вдали читаются крупные).
pub fn cloud_noise_image() -> Image {
    let mut img = Image::new(Extent3d { width: SIZE as u32, height: SIZE as u32, depth_or_array_layers: 1 }, TextureDimension::D2, cloud_noise_pixels(), TextureFormat::Rgba8Unorm, RenderAssetUsages::RENDER_WORLD);
    crate::textures::generate_mips(&mut img, false);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Шум бесшовный (левый край продолжает правый) и занимает весь диапазон — иначе пороги
    /// облачности в шейдере не попадут в него и небо будет либо пустым, либо сплошным.
    #[test]
    fn noise_tiles_and_spans_range() {
        let px = cloud_noise_pixels();
        let at = |x: usize, y: usize, c: usize| px[(y * SIZE + x) * 4 + c] as f32 / 255.0;
        for c in 0..4 {
            let (mut lo, mut hi, mut sum) = (1.0f32, 0.0f32, 0.0f32);
            for y in (0..SIZE).step_by(3) {
                for x in (0..SIZE).step_by(3) {
                    let v = at(x, y, c);
                    lo = lo.min(v);
                    hi = hi.max(v);
                    sum += v;
                }
            }
            let mean = sum / ((SIZE / 3 + 1) * (SIZE / 3 + 1)) as f32;
            assert!(lo < 0.2 && hi > 0.85, "канал {c}: {lo}..{hi}");
            assert!((0.25..0.75).contains(&mean), "канал {c}: среднее {mean}");
            // Шов: разница через край не больше, чем между соседями внутри.
            let mut seam = 0.0f32;
            let mut inner = 0.0f32;
            for y in 0..SIZE {
                seam = seam.max((at(0, y, c) - at(SIZE - 1, y, c)).abs());
                inner = inner.max((at(200, y, c) - at(201, y, c)).abs());
            }
            assert!(seam <= inner * 2.0 + 0.03, "канал {c}: шов {seam}, внутри {inner}");
        }
    }

    /// Картинка для подбора порогов: `cargo test dump_cloud_noise -- --ignored`.
    #[test]
    #[ignore]
    fn dump_cloud_noise() {
        let px = cloud_noise_pixels();
        let mut out = format!("P6\n{} {}\n255\n", SIZE * 4, SIZE).into_bytes();
        for y in 0..SIZE {
            for c in 0..4 {
                for x in 0..SIZE {
                    let v = px[(y * SIZE + x) * 4 + c];
                    out.extend_from_slice(&[v, v, v]);
                }
            }
        }
        std::fs::write(std::env::temp_dir().join("cloud_noise.ppm"), out).unwrap();
    }
}
