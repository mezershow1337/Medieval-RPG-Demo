//! Карта объёмных облаков на всё небо.
//!
//! Объёмные облака — это луч сквозь слой облаков с десятками шагов на каждую точку неба; считать
//! так всё небо в каждом кадре дорого и шумно. Поэтому небо с облаками рисуется отдельно, в
//! картинку-«рыбий глаз» (`shaders/cloud_map.wesl`), и понемногу: за кадр — полоска в несколько
//! десятков строк, вся карта — примерно за секунду. Купол неба (`shaders/sky.wesl`) эту картинку
//! только читает. Картинок две: одну видно, вторая дорисовывается; готовая становится видимой.
//! Облака плывут и между обновлениями: купол сдвигает взгляд на путь, пройденный ветром с
//! момента снимка карты (и на путь самой камеры).

use crate::graphics::{SkyDome, SkyMaterial};
use crate::sky::SkyUniform;
use bevy::asset::embedded_asset;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{CameraOutputMode, ClearColorConfig, RenderTarget, Viewport};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};

/// Слой отрисовки, в котором живут только лист и камеры карты облаков.
const LAYER: usize = 5;
/// За сколько секунд карта перерисовывается целиком.
const CYCLE_SECS: f32 = 1.0;
/// Не больше этой доли карты за кадр (при низкой частоте кадров карта обновляется дольше,
/// зато кадр не становится ещё тяжелее).
const MAX_PART: u32 = 24;

/// Как `CloudView` в cloud_map.wesl.
#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub struct CloudView {
    /// xyz — откуда снимается карта.
    pub origin: Vec4,
    /// xy — на сколько метров ветер унёс облака, z — номер круга, w — сторона карты в точках.
    pub wind: Vec4,
    /// x — шагов луча, y — шагов к солнцу.
    pub quality: Vec4,
}

/// Как `CloudDome` в sky.wesl.
#[derive(ShaderType, Clone, Copy, Debug, Default, Reflect)]
pub struct CloudDome {
    /// xyz — откуда снята видимая карта, w — 1, если она готова.
    pub origin: Vec4,
    /// xy — на сколько метров ветер унёс облака с момента снимка, z — 1, если готова панорама
    /// гор (её ведёт mountain_map.rs).
    pub drift: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct CloudMapMaterial {
    #[uniform(0)]
    pub sky: SkyUniform,
    /// Шум формы облаков (`cloud_noise.rs`).
    #[texture(1)]
    #[sampler(2)]
    pub noise: Handle<Image>,
    #[uniform(3)]
    pub view: CloudView,
}

impl Material2d for CloudMapMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/cloud_map.wesl".into()
    }
}

#[derive(Resource)]
pub struct CloudMap {
    images: [Handle<Image>; 2],
    cams: [Entity; 2],
    material: Handle<CloudMapMaterial>,
    size: u32,
    /// Какая из двух картинок сейчас дорисовывается.
    back: usize,
    /// Сколько её строк уже отдано на отрисовку.
    row: u32,
    cycle: u32,
    /// Видимая картинка дорисована хотя бы раз.
    ready: bool,
    /// Откуда и при каком сносе ветром снята каждая картинка.
    shots: [(Vec3, Vec2); 2],
    wind: Vec2,
}

/// Шагов луча и шагов к солнцу по настройке эффектов (на низких — прежние плоские облака).
fn quality(effects: u8) -> Vec2 {
    match effects {
        0 => Vec2::ZERO,
        1 => Vec2::new(64.0, 5.0),
        2 => Vec2::new(96.0, 6.0),
        _ => Vec2::new(128.0, 6.0),
    }
}

pub struct CloudMapPlugin;

impl Plugin for CloudMapPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/cloud_map.wesl");
        app.add_plugins(Material2dPlugin::<CloudMapMaterial>::default()).add_systems(Startup, setup).add_systems(Update, drive);
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<CloudMapMaterial>>) {
    // Сторона карты: на высоких эффектах точка карты — около 0,09° неба.
    // `MRPG_CLOUD_MAP=512` — меньше (для снимков на программном рендере).
    let size = std::env::var("MRPG_CLOUD_MAP").ok().and_then(|s| s.parse().ok()).unwrap_or(if crate::settings::startup().effects >= 2 { 2048u32 } else { 1024 }).clamp(256, 4096);
    let target = |images: &mut Assets<Image>| {
        let mut img = Image::new_target_texture(size, size, TextureFormat::Rgba8UnormSrgb, None);
        img.sampler = ImageSampler::linear();
        images.add(img)
    };
    let pair = [target(&mut images), target(&mut images)];
    let material = materials.add(CloudMapMaterial { sky: SkyUniform::default(), noise: images.add(crate::cloud_noise::cloud_noise_image()), view: CloudView::default() });
    // Лист во весь вид камеры: шейдер сам знает по точке картинки, куда она смотрит в небе.
    commands.spawn((Mesh2d(meshes.add(Rectangle::new(20000.0, 20000.0))), MeshMaterial2d(material.clone()), RenderLayers::layer(LAYER), NoFrustumCulling));
    let cams = [0, 1].map(|i| {
        commands
            .spawn((
                Camera2d,
                Camera {
                    is_active: false,
                    order: -20 + i as isize,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    // Полоска пишется в картинку как есть; остальные строки не трогаем.
                    output_mode: CameraOutputMode::Write { blend_state: None, clear_color: ClearColorConfig::None },
                    ..default()
                },
                RenderTarget::Image(pair[i].clone().into()),
                Msaa::Off,
                RenderLayers::layer(LAYER),
            ))
            .id()
    });
    commands.insert_resource(CloudMap { images: pair, cams, material, size, back: 0, row: 0, cycle: 0, ready: false, shots: [(Vec3::ZERO, Vec2::ZERO); 2], wind: Vec2::ZERO });
}

#[allow(clippy::too_many_arguments)]
fn drive(
    time: Res<Time>,
    settings: Res<crate::settings::Settings>,
    map: Option<ResMut<CloudMap>>,
    mut materials: ResMut<Assets<CloudMapMaterial>>,
    mut skies: ResMut<Assets<SkyMaterial>>,
    mut waters: ResMut<Assets<crate::sky::WaterMaterial>>,
    dome: Query<&MeshMaterial3d<SkyMaterial>, With<SkyDome>>,
    views: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    mut cams: Query<&mut Camera, Without<Camera3d>>,
) {
    let Some(mut map) = map else { return };
    let q = quality(settings.effects);
    let on = q.x > 0.0 && crate::level::active() == crate::level::LevelKind::Chersonesus;
    let sky = crate::graphics::mood().sky_uniform();
    // Облака плывут по ветру погоды (extra.zw — ветер, м/с).
    map.wind += (Vec2::new(8.0, 2.8) + Vec2::new(sky.extra.z, sky.extra.w) * 3.0) * time.delta_secs();
    if !on {
        for e in map.cams {
            if let Ok(mut c) = cams.get_mut(e) {
                c.is_active = false;
            }
        }
        if map.ready || map.row > 0 {
            map.ready = false;
            map.row = 0;
            for h in &dome {
                if let Some(mut mat) = skies.get_mut(&h.0) {
                    mat.dome.origin.w = 0.0;
                }
            }
            for (_, w) in waters.iter_mut() {
                w.extension.dome.origin.w = 0.0;
            }
        }
        return;
    }
    let eye = views.iter().find(|(_, c)| c.is_active).or_else(|| views.iter().next()).map_or(Vec3::ZERO, |(t, _)| t.translation());
    // Картинка дорисована в прошлом кадре — показываем её и начинаем другую.
    if map.row >= map.size {
        map.ready = true;
        map.back = 1 - map.back;
        map.row = 0;
        map.cycle += 1;
    }
    let (size, back) = (map.size, map.back);
    if map.row == 0 {
        // Весь круг карта рисуется с одной точки и при одном положении облаков — иначе полоски
        // не сошлись бы друг с другом.
        map.shots[back] = (eye, map.wind);
        if let Some(mut m) = materials.get_mut(&map.material) {
            m.sky = sky;
            m.view = CloudView { origin: eye.extend(0.0), wind: Vec4::new(map.wind.x, map.wind.y, map.cycle as f32, size as f32), quality: q.extend(0.0).extend(0.0) };
        }
    }
    // Первую карту рисуем быстро (небо ждёт), дальше — понемногу.
    let rows = if map.ready { ((size as f32 * time.delta_secs() / CYCLE_SECS).ceil() as u32).clamp(1, size.div_ceil(MAX_PART)) } else { size.div_ceil(16) }.min(size - map.row);
    for (i, e) in map.cams.into_iter().enumerate() {
        let Ok(mut c) = cams.get_mut(e) else { continue };
        c.is_active = i == back;
        if i == back {
            c.viewport = Some(Viewport { physical_position: UVec2::new(0, map.row), physical_size: UVec2::new(size, rows), ..default() });
        }
    }
    map.row += rows;
    let front = 1 - back;
    let (origin, wind) = map.shots[front];
    let drift = map.wind - wind;
    for h in &dome {
        if let Some(mut mat) = skies.get_mut(&h.0) {
            if mat.clouds != map.images[front] {
                mat.clouds = map.images[front].clone();
            }
            mat.dome = CloudDome { origin: origin.extend(if map.ready { 1.0 } else { 0.0 }), drift: Vec4::new(drift.x, drift.y, mat.dome.drift.z, 0.0) };
        }
    }
    // Те же облака отражаются в воде.
    for (_, w) in waters.iter_mut() {
        if w.extension.clouds.as_ref() != Some(&map.images[front]) {
            w.extension.clouds = Some(map.images[front].clone());
        }
        w.extension.dome = CloudDome { origin: origin.extend(if map.ready { 1.0 } else { 0.0 }), drift: Vec4::new(drift.x, drift.y, w.extension.dome.drift.z, 0.0) };
    }
}
