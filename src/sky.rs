//! Небо, море и свет неба.
//!
//! * Купол неба и море считают небо одной функцией (`shaders/sky_common.wesl`).
//! * Море — отдельный материал (`shaders/water.wesl`): отражение неба с облаками,
//!   солнечная дорожка, прозрачность и пена у берега по глубине сцены.
//! * Карта окружения: то же небо, посчитанное на процессоре в кубическую карту
//!   (`sky_environment`), один раз отфильтрованная там же для рассеянного света и отражений.
//!   От неё — холодный синеватый свет в тенях, тёплый со стороны заката и отражения
//!   неба на мокром камне, мраморе и металле.

use crate::graphics::Mood;
use bevy::asset::RenderAssetUsages;
use bevy::light::EnvironmentMapLight;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension};
use bevy::shader::ShaderRef;

/// Параметры неба (как `SkyParams` в sky_common.wesl).
#[derive(ShaderType, Clone, Copy, Debug, Default, Reflect)]
pub struct SkyUniform {
    pub sun: Vec4,
    pub horizon: Vec4,
    pub horizon_sun: Vec4,
    pub zenith: Vec4,
    /// Направление на луну, w — освещённая часть диска.
    pub moon: Vec4,
    /// x — ночь 0..1, y — ненастье 0..1, zw — ветер (м/с).
    pub extra: Vec4,
    /// Сумерки: rgb — коралловое небо над зарёй, w — пелена высококучевых облаков 0..1.
    pub glow: Vec4,
    /// Сумерки: rgb — сиреневое небо в стороне от солнца, w — сила подсветки облаков снизу.
    pub tint: Vec4,
}

// ---------- Купол неба ----------

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub sky: SkyUniform,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/sky.wesl".into()
    }
    // Купол смотрим изнутри: без этого его грани отсекались как задние и вместо неба был
    // ровный цвет очистки экрана.
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

// ---------- Море ----------

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExt>;

#[derive(ShaderType, Clone, Copy, Debug, Default, Reflect)]
pub struct WaterUniform {
    pub sky: SkyUniform,
    /// Цвет толщи (линейный), w — плотность, 1/м.
    pub deep: Vec4,
    /// Цвет мелководья, w — сила пены.
    pub shallow: Vec4,
    /// x — сила блика солнца, y — масштаб волн, z — есть ли глубина из предпрохода.
    pub params: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct WaterExt {
    #[uniform(100)]
    pub water: WaterUniform,
}

impl MaterialExtension for WaterExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/water.wesl".into()
    }
}

/// Материал моря: тёмная зеленовато-синяя вода Понта, бирюзовое мелководье над галькой.
pub fn water_material(m: &Mood) -> WaterMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.02, 0.06, 0.07),
            perceptual_roughness: 0.25,
            reflectance: 0.0,
            alpha_mode: AlphaMode::Blend,
            ..default()
        },
        extension: WaterExt {
            water: WaterUniform {
                sky: m.sky_uniform(),
                deep: Vec4::new(0.006, 0.028, 0.034, 0.55),
                shallow: Vec4::new(0.09, 0.2, 0.17, 1.0),
                params: Vec4::new(if m.day { 2.0 } else { 3.0 }, 1.0, 1.0, 0.0),
            },
        },
    }
}

// ---------- Небо на процессоре: та же формула, что в sky_common.wesl ----------

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn hash21(p: Vec2) -> f32 {
    let mut q = Vec2::new(fract(p.x * 123.34), fract(p.y * 456.21));
    let d = q.dot(q + Vec2::splat(45.32));
    q += Vec2::splat(d);
    fract(q.x * q.y)
}

fn vnoise(p: Vec2) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec2::splat(3.0) - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + Vec2::X);
    let c = hash21(i + Vec2::Y);
    let d = hash21(i + Vec2::ONE);
    let ab = a + (b - a) * u.x;
    let cd = c + (d - c) * u.x;
    ab + (cd - ab) * u.y
}

fn fbm(p: Vec2) -> f32 {
    let (mut v, mut a, mut q) = (0.0, 0.5, p);
    for _ in 0..5 {
        v += a * vnoise(q);
        q = q * 2.02 + Vec2::new(5.3, 1.7);
        a *= 0.5;
    }
    v
}

fn fbm3(p: Vec2) -> f32 {
    vnoise(p) * 0.57 + vnoise(p * 2.03 + Vec2::splat(3.1)) * 0.29 + vnoise(p * 4.1 + Vec2::splat(7.7)) * 0.14
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn norm2(v: Vec2) -> Vec2 {
    (v + Vec2::new(1e-5, 0.0)).normalize()
}

/// Свет низкого солнца на нижней кромке облаков и доходит ли он (как `twilight_light`
/// в sky_common.wesl): у солнца золото и оранжевый, выше малиновый и розовый, вдали от
/// солнца — тень Земли.
fn twilight_light(dir: Vec3, sun: Vec3, e: f32, reach: f32) -> (Vec3, f32) {
    let cos_az = norm2(Vec2::new(dir.x, dir.z)).dot(norm2(Vec2::new(sun.x, sun.z)));
    let local = sun.y.clamp(-1.0, 1.0).asin() + (reach / e.max(0.02) * cos_az).clamp(-0.1, 0.1);
    let lit = smoothstep(-0.022, -0.008, local);
    let k = ((local + 0.012) / 0.312).clamp(0.0, 1.0);
    let mut c = Vec3::new(1.0, 0.04, 0.09).lerp(Vec3::new(1.0, 0.25, 0.07), smoothstep(0.06, 0.2, k));
    c = c.lerp(Vec3::new(1.0, 0.6, 0.25), smoothstep(0.18, 0.42, k));
    c = c.lerp(Vec3::new(1.0, 0.9, 0.75), smoothstep(0.4, 1.0, k));
    (c * ((0.8 + 0.2 * smoothstep(0.0, 0.4, k)) * lit), lit)
}

/// Гряды пелены высококучевых (как `DECK_ROT` в sky_common.wesl).
const DECK_ROT: Vec2 = Vec2::new(0.17, -0.985);

/// Цвет неба в направлении `dir` (без дисков солнца и луны — в отражениях их даёт свет).
/// Та же формула, что `sky_radiance` в sky_common.wesl (облака упрощены: без сноса ветром,
/// перистых и золотой кромки).
pub fn sky_radiance(dir: Vec3, s: &SkyUniform) -> Vec3 {
    let dir = dir.normalize();
    let sun = s.sun.truncate().normalize();
    let d = s.sun.w.clamp(0.0, 1.0);
    let n = s.extra.x.clamp(0.0, 1.0);
    let oc = s.extra.y.clamp(0.0, 1.0);
    let e = dir.y;
    let h = e.clamp(0.0, 1.0);
    let sd = dir.dot(sun).max(0.0);
    let flat = Vec3::new(dir.x, 0.0, dir.z) + Vec3::new(1e-5, 0.0, 0.0);
    let hor = s.horizon.truncate() + s.horizon_sun.truncate() * flat.normalize().dot(sun).max(0.0).powf(s.horizon_sun.w);
    let s2 = norm2(Vec2::new(sun.x, sun.z));
    let d2 = norm2(Vec2::new(dir.x, dir.z));
    let toward = (d2.dot(s2) * 0.5 + 0.5).clamp(0.0, 1.0);
    let zen = s.zenith.truncate();
    let tint = s.tint.truncate();
    let tw_mid = tint.lerp(s.glow.truncate(), toward.powf(2.5) * (1.0 - smoothstep(0.03, 0.2, h)));
    let mut mid = tw_mid.lerp(Vec3::new(0.26, 0.46, 0.86), d);
    mid = mid.lerp(hor.lerp(zen, 0.5), n);
    mid = mid.lerp(hor * 1.04, oc * 0.7);
    let mut col = hor.lerp(mid, smoothstep(0.0, 0.10 + (0.16 - 0.10) * d, h));
    col = col.lerp(zen.lerp(hor * 1.02, oc * 0.6), smoothstep(0.12, 0.5 + (0.85 - 0.5) * d, h));
    let el = sun.y;
    let veil = (1.0 - n) * (1.0 - 0.8 * oc);
    let zarya = smoothstep(-0.1, -0.035, el) * (1.0 - smoothstep(0.03, 0.12, el)) * veil;
    let zarya_c = Vec3::new(1.4, 0.25, 0.04).lerp(Vec3::new(2.2, 0.75, 0.08), smoothstep(-0.07, -0.02, el));
    col += zarya_c * (toward.powi(6) * (1.0 - smoothstep(0.0, 0.012, h)) * zarya);
    let low = smoothstep(-0.05, 0.05, el);
    let halo_gold = (Vec3::new(1.6, 0.45, 0.12).lerp(Vec3::new(2.2, 1.15, 0.45), smoothstep(0.0, 0.1, el)) * sd.powf(7.0) * 0.55 + Vec3::new(3.5, 2.0, 0.9) * sd.powf(90.0) * 0.9) * low;
    let halo_day = Vec3::new(1.0, 0.9, 0.7) * sd.powf(8.0) * 0.5;
    col += halo_gold.lerp(halo_day, d) * veil;
    let moon = s.moon.truncate().normalize_or_zero();
    let md = dir.dot(moon).max(0.0);
    let moon_up = smoothstep(-0.03, 0.05, moon.y) * s.moon.w * n;
    col += Vec3::new(0.10, 0.12, 0.16) * md.powf(60.0) * moon_up * (1.0 - 0.6 * oc);
    if e > 0.0 {
        let p = Vec2::new(dir.x, dir.z) / (e + 0.07) * 0.75;
        let w = Vec2::new(fbm3(p * 0.6 + Vec2::splat(3.1)), fbm3(p * 0.6 + Vec2::splat(7.7)));
        let q = p + (w - Vec2::splat(0.5)) * 1.1;
        let dn = fbm(q * 1.3);
        let k = s.horizon.w;
        let thr = 0.66 + (0.44 - 0.66) * k.min(1.0) - 0.22 * smoothstep(0.7, 1.0, k) - 0.3 * oc;
        let cover = smoothstep(thr, thr + 0.16, dn).max(oc * 0.93) * smoothstep(0.0, 0.07, e);
        let d_sun = fbm((q + s2 * 0.16) * 1.3);
        let lit = ((dn - d_sun) * 5.0 + 0.45).clamp(0.0, 1.0);
        let thin = 1.0 - smoothstep(thr + 0.05, thr + 0.3, dn);
        let tw_amb = tint * 0.75;
        let moon_lit = smoothstep(-0.02, 0.1, moon.y) * s.moon.w;
        let fade = (1.0 - smoothstep(0.0, 0.22, e)) * 0.55;
        // Кучевые.
        let day_c = Vec3::new(0.62, 0.66, 0.76).lerp(Vec3::new(1.32, 1.3, 1.26), lit * 0.7 + thin * 0.3);
        let (low_c, low_lit) = twilight_light(dir, sun, e, 0.001);
        let tw_c = tw_amb * ((0.9 - 0.3 * lit) * (1.0 - 0.8 * low_lit)) + low_c * s.tint.w * (0.45 + 0.55 * lit * (0.35 + 0.65 * thin));
        let mut c = tw_c.lerp(day_c, d);
        let grey = Vec3::new(0.34, 0.35, 0.38).lerp(Vec3::new(0.62, 0.64, 0.68), lit * 0.6 + thin * 0.4) * (0.55 + 0.45 * d);
        c = c.lerp(grey, oc * 0.8);
        let night_c = Vec3::new(0.010, 0.012, 0.020).lerp(Vec3::new(0.045, 0.052, 0.075), lit) * (0.6 + 0.8 * moon_lit);
        c = c.lerp(night_c, n);
        c = c.lerp(hor, fade);
        // Пелена высококучевых.
        let (mut deck, mut cd) = (0.0, Vec3::ZERO);
        if s.glow.w > 0.01 {
            let pd = Vec2::new(dir.x, dir.z) / (e + 0.035) * 0.9;
            let r = Vec2::new(pd.x * DECK_ROT.x + pd.y * DECK_ROT.y, -pd.x * DECK_ROT.y + pd.y * DECK_ROT.x) * Vec2::new(0.7, 1.35);
            let dd = fbm(r * 1.6 + w * 1.17 + Vec2::new(11.0, 3.0)) * 0.7 + fbm3(r * 6.0 + w * 1.8) * 0.3;
            let dthr = 0.62 + (0.26 - 0.62) * s.glow.w;
            deck = smoothstep(dthr, dthr + 0.14, dd) * smoothstep(0.0, 0.05, e) * (s.glow.w * 3.0).min(1.0) * (1.0 - 0.45 * d) * (1.0 - oc * 0.5);
            let thick = smoothstep(dthr + 0.1, dthr + 0.3, dd);
            let (hi_c, hi_lit) = twilight_light(dir, sun, e, 0.0015);
            let sunny = smoothstep(0.36, 0.62, w.y);
            let glow_k = hi_lit * s.tint.w * 1.25 * (1.1 - 0.85 * thick) * (0.4 + 0.9 * sunny);
            let tw_d = tw_amb * ((1.0 - 0.25 * thick) * (1.0 - 0.8 * hi_lit * (1.0 - 0.6 * thick) * sunny)) + hi_c * (2.0 * (1.0 - (-glow_k / 2.0).exp()) / hi_lit.max(1e-3));
            let day_d = Vec3::new(1.15, 1.15, 1.18).lerp(Vec3::new(0.75, 0.78, 0.86), thick) + Vec3::new(0.5, 0.45, 0.35) * sd.powf(10.0) * (1.0 - thick);
            cd = tw_d.lerp(day_d, d).lerp(grey, oc * 0.8);
            let night_d = Vec3::new(0.045, 0.052, 0.075).lerp(Vec3::new(0.010, 0.012, 0.020), thick) * (0.6 + 0.8 * moon_lit);
            cd = cd.lerp(night_d, n).lerp(hor, fade * 0.5);
        }
        let back = deck * (1.0 - cover);
        let total = cover + back;
        if total > 1e-4 {
            col = col.lerp((c * cover + cd * back) / total, total.min(1.0));
        }
    }
    if e < 0.0 {
        // Снизу — земля и море в дымке: темнее горизонта.
        col = hor * 0.35;
    }
    col
}

fn f32_to_f16(v: f32) -> u16 {
    let x = v.max(0.0).min(65000.0).to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32 - 127 + 15;
    let mant = x & 0x7f_ffff;
    if exp <= 0 {
        return sign;
    }
    if exp >= 31 {
        return sign | 0x7bff;
    }
    sign | ((exp as u16) << 10) | ((mant >> 13) as u16)
}

/// Направление для текселя грани кубической карты (порядок граней +X −X +Y −Y +Z −Z,
/// v растёт вниз). Bevy читает карту окружения с перевёрнутой осью Z — учитываем.
fn cube_dir(face: usize, u: f32, v: f32) -> Vec3 {
    let c = match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    };
    Vec3::new(c.x, c.y, -c.z).normalize()
}

fn hammersley(i: u32, n: u32) -> Vec2 {
    Vec2::new(i as f32 / n as f32, i.reverse_bits() as f32 * 2.328_306_4e-10)
}

/// Базис касательной плоскости к `n`.
fn frame(n: Vec3) -> (Vec3, Vec3) {
    let up = if n.y.abs() < 0.999 { Vec3::Y } else { Vec3::X };
    let t = up.cross(n).normalize();
    (t, n.cross(t))
}

/// Отражение неба для шероховатости `rough` (GGX, выборка по значимости, N = V = R).
#[cfg(test)]
fn prefilter_specular(n: Vec3, rough: f32, s: &SkyUniform, samples: u32) -> Vec3 {
    prefilter_with(n, rough, samples, |d| sky_radiance(d, s))
}

/// Что видно в отражениях: сверху небо, у горизонта — стены и крыши города (тёплый
/// известняк в свете неба), снизу — мостовая. Голое небо во всех направлениях делало
/// металл однотонно-синим: клинок отражал синеву даже «боком», где в жизни были бы дома.
fn surround_radiance(dir: Vec3, s: &SkyUniform, stone: Vec3) -> Vec3 {
    let e = dir.normalize().y;
    let c = if e < 0.0 {
        stone * 0.85
    } else {
        let walls = (1.0 - smoothstep(0.0, 0.16, e)) * 0.5;
        sky_radiance(dir, s).lerp(stone, walls)
    };
    // Чуть приглушаем цвет: в отражениях смешано много всего.
    let l = c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    c.lerp(Vec3::splat(l), 0.15)
}

fn prefilter_with(n: Vec3, rough: f32, samples: u32, radiance: impl Fn(Vec3) -> Vec3) -> Vec3 {
    if rough < 1e-3 {
        return radiance(n);
    }
    let a = rough * rough;
    let (t, b) = frame(n);
    let (mut sum, mut w) = (Vec3::ZERO, 0.0);
    for i in 0..samples {
        let xi = hammersley(i, samples);
        let phi = std::f32::consts::TAU * xi.x;
        let cos_t = ((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y)).sqrt();
        let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
        let h = t * (sin_t * phi.cos()) + b * (sin_t * phi.sin()) + n * cos_t;
        let l = 2.0 * n.dot(h) * h - n;
        let nl = n.dot(l);
        if nl > 0.0 {
            sum += radiance(l) * nl;
            w += nl;
        }
    }
    sum / w.max(1e-4)
}

/// Рассеянный свет неба: средняя яркость по полусфере с весом косинуса.
fn prefilter_diffuse(n: Vec3, s: &SkyUniform, samples: u32) -> Vec3 {
    let (t, b) = frame(n);
    let mut sum = Vec3::ZERO;
    for i in 0..samples {
        let xi = hammersley(i, samples);
        let r = xi.y.sqrt();
        let phi = std::f32::consts::TAU * xi.x;
        let l = t * (r * phi.cos()) + b * (r * phi.sin()) + n * (1.0 - xi.y).max(0.0).sqrt();
        sum += sky_radiance(l, s);
    }
    sum / samples as f32
}

/// Кубическая карта (порядок «слой, затем мип-уровни»), каждый тексель считает `texel(dir, mip)`.
fn cubemap(size: u32, mips: u32, texel: impl Fn(Vec3, u32) -> Vec3 + Sync) -> Image {
    let texel = &texel;
    let faces: Vec<Vec<u8>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..6)
            .map(|face| {
                scope.spawn(move || {
                    let mut out = Vec::new();
                    for mip in 0..mips {
                        let n = (size >> mip).max(1) as usize;
                        for y in 0..n {
                            for x in 0..n {
                                let u = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                                let v = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                                let c = texel(cube_dir(face, u, v), mip);
                                for ch in [c.x, c.y, c.z, 1.0] {
                                    out.extend_from_slice(&f32_to_f16(ch).to_le_bytes());
                                }
                            }
                        }
                    }
                    out
                })
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    let mut img = Image::new_uninit(Extent3d { width: size, height: size, depth_or_array_layers: 6 }, TextureDimension::D2, TextureFormat::Rgba16Float, RenderAssetUsages::RENDER_WORLD);
    img.data = Some(faces.concat());
    img.texture_descriptor.mip_level_count = mips;
    img.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    img
}

/// Карта окружения неба, отфильтрованная один раз на процессоре: рассеянный свет 32²
/// и отражения 128² с мип-уровнями по шероховатости (так Bevy и читает: уровень =
/// шероховатость × (уровней − 1)). Небо не меняется, поэтому считать его каждый кадр
/// на видеокарте незачем.
pub fn sky_environment(s: &SkyUniform) -> (Image, Image) {
    sky_environment_sized(s, 128, 1.0)
}

/// То же с размером отражений `spec` и долей выборок `quality` (для пересчёта на ходу,
/// когда меняется время суток и погода).
pub fn sky_environment_sized(s: &SkyUniform, spec: u32, quality: f32) -> (Image, Image) {
    let mips = spec.trailing_zeros() + 1;
    let q = |n: u32| ((n as f32 * quality) as u32).max(8);
    // Освещённый небом и солнцем известняк города — для отражений у горизонта и снизу.
    let stone = Vec3::new(0.62, 0.56, 0.46) * prefilter_diffuse(Vec3::Y, s, q(128)) * 1.5;
    let specular = cubemap(spec, mips, |d, mip| {
        let rough = mip as f32 / (mips - 1) as f32;
        prefilter_with(d, rough, if mip == 0 { 1 } else { q(24 + mip * 8) }, |l| surround_radiance(l, s, stone))
    });
    let diffuse = cubemap(if spec >= 128 { 32 } else { 16 }, 1, |d, _| prefilter_diffuse(d, s, q(96)));
    (diffuse, specular)
}

/// Свет неба для камер (создаётся один раз).
#[derive(Resource, Clone)]
pub struct SkyEnvironment {
    pub diffuse: Handle<Image>,
    pub specular: Handle<Image>,
}

pub fn sky_environment_light(env: &SkyEnvironment, m: &Mood) -> EnvironmentMapLight {
    EnvironmentMapLight { diffuse_map: env.diffuse.clone(), specular_map: env.specular.clone(), intensity: m.sky_light, ..default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_round_trip() {
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(0.5), 0x3800);
        assert_eq!(f32_to_f16(2.0), 0x4000);
        assert_eq!(f32_to_f16(0.0), 0);
    }

    /// На закате горизонт со стороны солнца ярче и теплее, чем с противоположной,
    /// а зенит — синее горизонта.
    #[test]
    fn golden_sky_is_warm_toward_the_sun() {
        let s = SkyUniform {
            sun: Vec3::new(-0.8, 0.175, -0.42).normalize().extend(0.0),
            horizon: Vec4::new(0.56, 0.48, 0.54, 0.0),
            horizon_sun: Vec4::new(1.5, 0.78, 0.24, 5.0),
            zenith: Vec4::new(0.11, 0.2, 0.46, 0.0),
            moon: Vec4::new(0.8, -0.2, 0.4, 0.5),
            extra: Vec4::ZERO,
            glow: Vec4::new(1.0, 0.66, 0.46, 0.0),
            tint: Vec4::new(0.34, 0.40, 0.66, 2.2),
        };
        let toward = sky_radiance(Vec3::new(-0.8, 0.02, -0.42), &s);
        let away = sky_radiance(Vec3::new(0.8, 0.02, 0.42), &s);
        let up = sky_radiance(Vec3::Y, &s);
        assert!(toward.x > away.x * 1.5 && toward.x > toward.z * 2.0, "{toward} {away}");
        assert!(up.z > up.x, "{up}");
    }

    #[test]
    fn cubemap_faces_point_outward() {
        // Центр грани +Y смотрит вверх, центр грани −Y — вниз; с учётом перевёрнутой Z
        // центр грани +Z смотрит в мировую −Z.
        assert!(cube_dir(2, 0.0, 0.0).y > 0.99);
        assert!(cube_dir(3, 0.0, 0.0).y < -0.99);
        assert!(cube_dir(4, 0.0, 0.0).z < -0.99);
        assert!(cube_dir(0, 0.0, 0.0).x > 0.99);
    }
}

#[cfg(test)]
mod env_tests {
    use super::*;

    /// Рассеянный свет сверху — от неба (синеватый), снизу — тёмный (земля).
    #[test]
    fn diffuse_light_comes_from_the_sky() {
        let s = crate::graphics::mood().sky_uniform();
        let up = prefilter_diffuse(Vec3::Y, &s, 64);
        let down = prefilter_diffuse(Vec3::NEG_Y, &s, 64);
        assert!(up.length() > down.length() * 1.5, "{up} {down}");
        // Шероховатое отражение — среднее по конусу: не ярче самого яркого направления.
        let toward_sun = s.sun.truncate().normalize();
        let blurry = prefilter_specular(toward_sun, 1.0, &s, 64);
        assert!(blurry.max_element() < sky_radiance(toward_sun, &s).max_element() * 1.5);
    }
}

