//! Панорама дальних гор.
//!
//! Горы на горизонте — это луч по карте высот (десятки шагов, в каждом — несколько слоёв шума)
//! и ещё склон, снег и дымка в найденной точке. Считать так каждую точку неба и каждое отражение
//! в воде в каждом кадре дорого, а горы не меняются: меняется только свет на них, и тот медленно.
//! Поэтому горы рисуются отдельно, в длинную картинку-панораму (`shaders/mountain_map.wesl`:
//! по горизонтали — азимут в сторону суши, по вертикали — высота над горизонтом), понемногу —
//! несколько столбцов за кадр. Купол неба и море эту картинку только читают.

use crate::graphics::{SkyDome, SkyMaterial};
use crate::sky::SkyUniform;
use bevy::asset::embedded_asset;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{CameraOutputMode, ClearColorConfig, RenderTarget, Viewport};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};

/// Слой отрисовки, в котором живут только лист и камера панорамы.
const LAYER: usize = 6;
/// За сколько секунд панорама перерисовывается целиком (свет на горах меняется медленно).
const CYCLE_SECS: f32 = 8.0;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct MountainMapMaterial {
    #[uniform(0)]
    pub sky: SkyUniform,
    /// xy — размер панорамы в точках.
    #[uniform(1)]
    pub view: Vec4,
    /// Откуда смотрим (`MountainView` в шейдере).
    #[uniform(2)]
    pub eye: MountainView,
}

/// Горы считаются от игрока: ближние предгорья — настоящая земля (`level/mountains.rs`), и
/// панорама продолжает их только дальше края прорисовки (или края карты).
#[derive(Clone, Copy, Debug, Default, bevy::render::render_resource::ShaderType)]
pub struct MountainView {
    /// xyz — глаз, w — с какого расстояния начинать луч (там стоит купол неба).
    pub eye: Vec4,
    /// Прямоугольник нарисованной земли: xmin, xmax, zmin, zmax.
    pub edge: Vec4,
    /// Дымка у ближних гор: полная до x, сходит к y (м).
    pub fade: Vec4,
}

impl Material2d for MountainMapMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/mountain_map.wesl".into()
    }
}

#[derive(Resource)]
pub struct MountainMap {
    image: Handle<Image>,
    cam: Entity,
    material: Handle<MountainMapMaterial>,
    size: UVec2,
    /// Сколько столбцов текущего круга уже отдано на отрисовку.
    col: u32,
    /// Панорама дорисована хотя бы раз.
    ready: bool,
    /// Откуда начат текущий круг (если игрок ушёл далеко — рисуем заново и быстро).
    from: Vec3,
}

pub struct MountainMapPlugin;

impl Plugin for MountainMapPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/mountain_map.wesl");
        app.add_plugins(Material2dPlugin::<MountainMapMaterial>::default()).add_systems(Startup, setup).add_systems(Update, drive);
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<MountainMapMaterial>>) {
    // Точка панорамы — около 0,05° (на высоких эффектах): не крупнее точки экрана.
    // `MRPG_CLOUD_MAP` (для снимков на программном рендере) уменьшает и её.
    let small = std::env::var("MRPG_CLOUD_MAP").ok().and_then(|s| s.parse::<u32>().ok()).is_some_and(|s| s <= 1024);
    let width = if small || crate::settings::startup().effects < 2 { 2048u32 } else { 4096 };
    // По высоте панорама охватывает 0,86 рад (из предгорий главная гряда встаёт высоко).
    let size = UVec2::new(width, width / 6);
    let mut img = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    img.sampler = ImageSampler::linear();
    let image = images.add(img);
    let material = materials.add(MountainMapMaterial { sky: SkyUniform::default(), view: Vec4::new(size.x as f32, size.y as f32, 0.0, 0.0), eye: MountainView::default() });
    commands.spawn((Mesh2d(meshes.add(Rectangle::new(20000.0, 20000.0))), MeshMaterial2d(material.clone()), RenderLayers::layer(LAYER), NoFrustumCulling));
    let cam = commands
        .spawn((
            Camera2d,
            Camera {
                is_active: false,
                order: -22,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                // Столбцы пишутся в картинку как есть; остальные не трогаем.
                output_mode: CameraOutputMode::Write { blend_state: None, clear_color: ClearColorConfig::None },
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            Msaa::Off,
            RenderLayers::layer(LAYER),
        ))
        .id();
    commands.insert_resource(MountainMap { image, cam, material, size, col: 0, ready: false, from: Vec3::splat(f32::MAX) });
}

fn drive(
    time: Res<Time>,
    map: Option<ResMut<MountainMap>>,
    mut materials: ResMut<Assets<MountainMapMaterial>>,
    mut skies: ResMut<Assets<SkyMaterial>>,
    mut waters: ResMut<Assets<crate::sky::WaterMaterial>>,
    dome: Query<&MeshMaterial3d<SkyMaterial>, With<SkyDome>>,
    mut cams: Query<&mut Camera, Without<Camera3d>>,
    eyes: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    settings: Res<crate::settings::Settings>,
) {
    let Some(mut map) = map else { return };
    let on = crate::level::active() == crate::level::LevelKind::Chersonesus;
    if on {
        if map.col >= map.size.x {
            map.ready = true;
            map.col = 0;
        }
        let eye = crate::graphics::active_camera(&eyes).map(|c| c.translation()).unwrap_or(Vec3::ZERO);
        // Ушёл далеко (новое место, телепорт) — старая панорама врёт: прячем её и рисуем заново.
        if eye.distance(map.from) > 150.0 {
            map.ready = false;
            map.col = 0;
            map.from = eye;
        }
        // Свет и дымка — на начало круга (за круг они почти не меняются).
        if map.col == 0
            && let Some(mut m) = materials.get_mut(&map.material)
        {
            m.sky = crate::graphics::mood().sky_uniform();
        }
        // Глаз — каждый кадр (столбцы, нарисованные в разных кадрах, сходятся на шагах игрока).
        if let Some(mut m) = materials.get_mut(&map.material) {
            m.eye = view_from(eye, &settings);
        }
        // Первую панораму рисуем быстро (небо ждёт), дальше — понемногу.
        let cols = if map.ready { ((map.size.x as f32 * time.delta_secs() / CYCLE_SECS).ceil() as u32).clamp(1, map.size.x / 64) } else { map.size.x / 16 }.min(map.size.x - map.col);
        if let Ok(mut c) = cams.get_mut(map.cam) {
            c.is_active = true;
            c.viewport = Some(Viewport { physical_position: UVec2::new(map.col, 0), physical_size: UVec2::new(cols, map.size.y), ..default() });
        }
        map.col += cols;
    } else {
        if let Ok(mut c) = cams.get_mut(map.cam) {
            c.is_active = false;
        }
        map.ready = false;
        map.col = 0;
    }
    // Купол и море читают готовую панораму (drift.z — готова ли она).
    let flag = if on && map.ready { 1.0 } else { 0.0 };
    for h in &dome {
        if let Some(sky) = skies.get(&h.0)
            && (sky.dome.drift.z != flag || sky.mountains != map.image)
            && let Some(mut sky) = skies.get_mut(&h.0)
        {
            sky.mountains = map.image.clone();
            sky.dome.drift.z = flag;
        }
    }
    let stale: Vec<_> = waters.iter().filter(|(_, w)| w.extension.dome.drift.z != flag || w.extension.mountains.as_ref() != Some(&map.image)).map(|(id, _)| id).collect();
    for id in stale {
        if let Some(mut w) = waters.get_mut(id) {
            w.extension.mountains = Some(map.image.clone());
            w.extension.dome.drift.z = flag;
        }
    }
}

/// Вид на горы из точки `eye`: луч начинается у купола неба (за ним уже не рисует камера) или
/// раньше — где кончается земля карты.
fn view_from(eye: Vec3, settings: &crate::settings::Settings) -> MountainView {
    let dome = crate::graphics::sky_dome_radius(settings);
    let half = crate::level::WORLD_HALF;
    MountainView {
        eye: eye.extend(dome),
        edge: Vec4::new(-half, half, -half, crate::level::mountains::MESH_SOUTH),
        fade: Vec4::new(dome, dome + 1400.0, 0.0, 0.0),
    }
}
