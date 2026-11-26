//! Море: сетка поверхности вокруг камеры, волны Герстнера (одна формула здесь и в
//! `shaders/water.wesl`), карта глубин для шейдера.
//!
//! Для других систем:
//! * [`surface_y`] — высота поверхности с волнами в точке (поплавок, пловец, лодки);
//! * [`is_water`] — есть ли здесь море (а не суша).

use crate::level::WATER_Y;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, VisibilityRange};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::ScreenSpaceTransmission;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::sync::atomic::{AtomicU32, Ordering};

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(camera_transmission)
            .add_systems(Update, (spawn_sea.run_if(resource_added::<SeaLevel>), sea_weather))
            .add_systems(PostUpdate, follow_camera.after(bevy::transform::TransformSystems::Propagate));
    }
}

/// Уровень моря на этой карте (кладёт `level::spawn_level`, если море есть).
#[derive(Resource, Clone, Copy)]
pub struct SeaLevel(pub f32);

/// Сетка моря (ходит за камерой).
#[derive(Component)]
struct SeaSurface;

/// Волны Герстнера: направление (x, z), длина волны, м, высота (амплитуда), м.
/// Те же числа — `WAVES` в water.wesl.
pub const WAVES: [[f32; 4]; 6] = [
    [0.28, 0.96, 31.0, 0.17],
    [-0.34, 0.94, 17.5, 0.10],
    [0.71, 0.70, 10.2, 0.06],
    [-0.80, 0.60, 6.1, 0.035],
    [0.10, -0.99, 3.7, 0.02],
    [0.93, 0.37, 2.3, 0.012],
];
/// Крутизна гребней (как `STEEP` в шейдере).
const STEEP: f32 = 0.62;
/// Половина стороны карты глубин (как `MAP_HALF` в шейдере) — весь мир.
const MAP_HALF: f32 = 900.0;
const MAP_SIZE: u32 = 1024;
/// Западный край набережной Карантинной бухты (`terrain::QUAY_EDGE`).
const QUAY_EDGE: f32 = 300.0;

/// Высота волн от ветра (общая для всего моря): f32 в битах, чтобы читать без ресурсов.
static WAVE_AMP: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0

pub fn wave_amp() -> f32 {
    f32::from_bits(WAVE_AMP.load(Ordering::Relaxed))
}

/// Высота волн от ветра, м/с: в штиль зыбь вполовину, в шторм — почти вдвое выше.
pub fn amp_for_wind(wind: f32) -> f32 {
    (0.55 + 0.085 * wind).clamp(0.5, 1.9)
}

/// Есть ли здесь море (а не суша).
pub fn is_water(x: f32, z: f32) -> bool {
    crate::level::ground_y(x, z) < WATER_Y - 0.05
}

/// Сила волн в точке: на мелководье они гаснут, в Карантинной бухте (за молом) — втрое ниже.
pub fn wave_scale(x: f32, z: f32) -> f32 {
    let depth = WATER_Y - crate::level::ground_y(x, z);
    let shallow = 0.2 + 0.8 * smoothstep(0.2, 6.0, depth);
    // Карантинная бухта: узкий залив к востоку от города.
    let bay = smoothstep(QUAY_EDGE - 30.0, QUAY_EDGE + 10.0, x) * smoothstep(-230.0, -130.0, z);
    // Песочная бухта на западе — чуть тише открытого моря.
    let sand = (1.0 - smoothstep(-330.0, -300.0, x)) * smoothstep(-100.0, 20.0, z);
    shallow * (1.0 - 0.68 * bay) * (1.0 - 0.3 * sand)
}

/// Высота поверхности моря с волнами, м (`t` — `Time::elapsed_secs_wrapped`, то же время,
/// что у шейдера). Без горизонтального сдвига гребней — для поплавка и пловца хватает.
pub fn surface_y(x: f32, z: f32, t: f32) -> f32 {
    WATER_Y + wave_height(x, z, t, wave_amp() * wave_scale(x, z))
}

/// Подъём поверхности волнами при данной высоте волн `amp`.
pub fn wave_height(x: f32, z: f32, t: f32, amp: f32) -> f32 {
    let mut h = 0.0;
    for (i, w) in WAVES.iter().enumerate() {
        let d = Vec2::new(w[0], w[1]).normalize();
        let k = std::f32::consts::TAU / w[2];
        let c = (9.81 / k).sqrt();
        let ph = k * d.dot(Vec2::new(x, z)) - c * k * t + i as f32 * 1.7;
        h += w[3] * amp * ph.sin();
    }
    h
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------------------------------------------------------------------------------------------
// Поверхность

/// Круговая сетка: у камеры шаг 0,35 м, дальше кольца редеют (до 2,5 км). Волны, которые
/// редкая сетка вдали не передаёт, шейдер гасит сам.
fn sea_mesh() -> Mesh {
    const SEGMENTS: u32 = 224;
    let mut radii = vec![0.0f32];
    let mut r = 0.0f32;
    while r < 2500.0 {
        let step = (r * std::f32::consts::TAU / SEGMENTS as f32).max(0.35);
        r += step;
        radii.push(r);
    }
    let mut pos = Vec::new();
    pos.push([0.0, 0.0, 0.0]);
    for &r in &radii[1..] {
        for s in 0..SEGMENTS {
            let a = s as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            pos.push([r * a.cos(), 0.0, r * a.sin()]);
        }
    }
    let rings = radii.len() as u32 - 1;
    let mut idx: Vec<u32> = Vec::new();
    let ring = |k: u32, s: u32| 1 + k * SEGMENTS + s % SEGMENTS;
    for s in 0..SEGMENTS {
        idx.extend([0, ring(0, s + 1), ring(0, s)]);
    }
    for k in 0..rings - 1 {
        for s in 0..SEGMENTS {
            let (a, b, c, d) = (ring(k, s), ring(k, s + 1), ring(k + 1, s), ring(k + 1, s + 1));
            idx.extend([a, b, d, a, d, c]);
        }
    }
    let n = pos.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; n])
        .with_inserted_indices(Indices::U32(idx))
}

/// Карта для шейдера на весь мир: r — глубина/16 м, g — сила волн, b — близость берега.
fn water_map() -> Image {
    let n = MAP_SIZE as usize;
    let mut data = vec![0u8; n * n * 4];
    for iz in 0..n {
        for ix in 0..n {
            let x = -MAP_HALF + (ix as f32 + 0.5) / n as f32 * 2.0 * MAP_HALF;
            let z = -MAP_HALF + (iz as f32 + 0.5) / n as f32 * 2.0 * MAP_HALF;
            let depth = (WATER_Y - crate::level::ground_y(x, z)).max(0.0);
            let i = (iz * n + ix) * 4;
            data[i] = ((depth / 16.0).clamp(0.0, 1.0) * 255.0).round() as u8;
            data[i + 1] = (wave_scale(x, z).clamp(0.0, 1.0) * 255.0).round() as u8;
            data[i + 2] = ((1.0 - depth / 3.0).clamp(0.0, 1.0) * 255.0).round() as u8;
            data[i + 3] = 255;
        }
    }
    let mut img = Image::new(Extent3d { width: MAP_SIZE, height: MAP_SIZE, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8Unorm, RenderAssetUsages::RENDER_WORLD);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    img
}

fn spawn_sea(mut commands: Commands, sea: Res<SeaLevel>, settings: Option<Res<crate::settings::Settings>>, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut mats: ResMut<Assets<crate::sky::WaterMaterial>>, mut surfaces: ResMut<Assets<crate::graphics::SurfaceMaterial>>) {
    let map = images.add(water_map());
    let full = settings.is_none_or(|s| s.water > 0);
    let mat = mats.add(crate::sky::water_material(&crate::graphics::mood(), Some(map), full));
    commands.spawn((SeaSurface, Mesh3d(meshes.add(sea_mesh())), MeshMaterial3d(mat), Transform::from_xyz(0.0, sea.0, 0.0), NotShadowCaster, NoFrustumCulling));
    // Дно глубже двух метров: земля уровня его не рисует (раньше вода была непрозрачной),
    // а сквозь новую воду его видно. Тот же материал рельефа — у воды это галька.
    let layout = crate::level::shared_full();
    if let Some(hf) = &layout.ground {
        let material = surfaces.add(crate::graphics::surface_material(crate::level::Mat::Terrain));
        for (center, mesh) in seabed_meshes(hf, sea.0) {
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::IDENTITY,
                NotShadowCaster,
                // Дальше ~300 м дно сквозь толщу уже не видно.
                VisibilityRange { start_margin: 0.0..0.0, end_margin: 300.0..330.0, use_aabb: true },
                Name::new(format!("Дно {center}")),
            ));
        }
    }
}

/// Участок дна (сторона, м).
const SEABED_CHUNK: f32 = 120.0;

/// Меши дна по участкам: клетки карты высот, которые пропускает рельеф уровня (все углы
/// глубже двух метров, см. `terrain::terrain_meshes`), — стык без щелей и без наложения.
fn seabed_meshes(hf: &crate::collision::HeightField, sea: f32) -> Vec<(Vec2, Mesh)> {
    let n = hf.nx;
    let pos = |ix: usize, iz: usize| Vec3::new(hf.origin.x + ix as f32 * hf.cell, hf.node(ix, iz), hf.origin.y + iz as f32 * hf.cell);
    let normal = |ix: usize, iz: usize| {
        let l = hf.node(ix.saturating_sub(1), iz);
        let r = hf.node((ix + 1).min(n - 1), iz);
        let d = hf.node(ix, iz.saturating_sub(1));
        let u = hf.node(ix, (iz + 1).min(hf.nz - 1));
        Vec3::new(l - r, 2.0 * hf.cell, d - u).normalize()
    };
    let mut chunks: std::collections::HashMap<(i32, i32), (crate::level::MeshBuilder, std::collections::HashMap<usize, u32>)> = default();
    for iz in 0..hf.nz - 1 {
        for ix in 0..n - 1 {
            let corners = [pos(ix, iz), pos(ix + 1, iz), pos(ix + 1, iz + 1), pos(ix, iz + 1)];
            if !corners.iter().all(|c| c.y < sea - 2.0) {
                continue;
            }
            let c = (corners[0] + corners[2]) / 2.0;
            let key = ((c.x / SEABED_CHUNK).floor() as i32, (c.z / SEABED_CHUNK).floor() as i32);
            let (mb, map) = chunks.entry(key).or_default();
            let mut v = [0u32; 4];
            for (k, (jx, jz)) in [(ix, iz), (ix + 1, iz), (ix + 1, iz + 1), (ix, iz + 1)].into_iter().enumerate() {
                v[k] = *map.entry(jz * n + jx).or_insert_with(|| {
                    let p = pos(jx, jz);
                    mb.vert(p, normal(jx, jz), Vec2::new(p.x, p.z))
                });
            }
            mb.index_tri(v[0], v[2], v[1]);
            mb.index_tri(v[0], v[3], v[2]);
        }
    }
    chunks
        .into_iter()
        .filter(|(_, (mb, _))| !mb.is_empty())
        .map(|((x, z), (mb, _))| (Vec2::new((x as f32 + 0.5) * SEABED_CHUNK, (z as f32 + 0.5) * SEABED_CHUNK), mb.build()))
        .collect()
}

/// Сетка ходит за камерой шагами по 0,35 м (как её внутренний шаг): волны не «плывут».
fn follow_camera(cameras: Query<(&GlobalTransform, &Camera), With<Camera3d>>, mut sea: Query<&mut Transform, With<SeaSurface>>) {
    let Some(cam) = cameras.iter().find(|(_, c)| c.is_active).or_else(|| cameras.iter().next()).map(|(t, _)| t.translation()) else { return };
    for mut tf in &mut sea {
        let snap = |v: f32| (v / 0.35).round() * 0.35;
        let want = Vec3::new(snap(cam.x), tf.translation.y, snap(cam.z));
        if tf.translation != want {
            tf.translation = want;
        }
    }
}

/// Высота волн по ветру (раз в несколько секунд, вместе со сменой погоды).
fn sea_weather(mood: Option<Res<crate::daylight::CurrentMood>>, mut mats: ResMut<Assets<crate::sky::WaterMaterial>>, mut shown: Local<f32>) {
    let Some(mood) = mood else { return };
    let amp = amp_for_wind(mood.0.wind.length());
    // Плавно: ветер меняется постепенно, волны за ним.
    let amp = if *shown == 0.0 { amp } else { *shown + (amp - *shown).clamp(-0.002, 0.002) };
    if (amp - *shown).abs() < 1e-4 {
        return;
    }
    *shown = amp;
    WAVE_AMP.store(amp.to_bits(), Ordering::Relaxed);
    let ids: Vec<_> = mats.ids().collect();
    for id in ids {
        if let Some(mut m) = mats.get_mut(id) {
            m.extension.water.waves.x = amp;
        }
    }
}

/// Камеры получают копию кадра после непрозрачного прохода: по ней море преломляет дно и
/// отражает город.
fn camera_transmission(event: On<Add<Camera3d>>, mut commands: Commands) {
    commands.entity(event.entity).insert(ScreenSpaceTransmission { steps: 1, ..default() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_stay_small_in_the_bay() {
        // У причалов Карантинной бухты корабли стоят ватерлинией на y ≈ −1,2: волна там
        // не выше ±0,15 м даже в сильный ветер.
        let amp = amp_for_wind(14.0);
        for z in [-100.0, 0.0, 100.0] {
            let s = wave_scale(305.0, z);
            let max: f32 = WAVES.iter().map(|w| w[3]).sum::<f32>() * amp * s;
            assert!(max < 0.2, "z = {z}: {max}");
        }
        // В открытом море — заметная зыбь.
        assert!(wave_scale(0.0, -400.0) > 0.9);
    }

    #[test]
    fn surface_moves_with_time() {
        let a = surface_y(10.0, -400.0, 0.0);
        let b = surface_y(10.0, -400.0, 1.3);
        assert!((a - b).abs() > 0.01);
        assert!((a - WATER_Y).abs() < 0.6);
        assert!(is_water(0.0, -400.0) && !is_water(0.0, 0.0));
    }
}
