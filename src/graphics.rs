//! Графика: собственные шейдеры (процедурный камень, витражи, огонь, небо),
//! пост-обработка (HDR, блум, SSAO, TAA, объёмный туман с лучами солнца) и атмосфера.
//!
//! F9 — переключить «тяжёлые» эффекты (объёмный туман и SSAO), если игра тормозит.

use crate::level::Mat;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfigBuilder, FogVolume, NotShadowCaster, NotShadowReceiver, ShadowFilteringMethod, VolumetricFog, VolumetricLight};
use bevy::mesh::Indices;
use bevy::pbr::{DistanceFog, ExtendedMaterial, FogFalloff, MaterialExtension, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, PrimitiveTopology};
use bevy::camera::Hdr;
use bevy::light::EnvironmentMapLight;
use bevy::post_process::effect_stack::Vignette;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
pub use crate::sky::{SkyMaterial, SkyUniform};
use bevy::shader::ShaderRef;

/// Направление на солнце в замке (закатное солнце с запада-юго-запада).
pub const SUN_DIR: Vec3 = Vec3::new(-0.78, 0.42, 0.40);

/// Освещение уровня. Небо, дымка и туман согласованы: туман Bevy вдали даёт ровно цвет
/// горизонта неба (`horizon` + `horizon_sun` в сторону солнца), поэтому стены и море
/// растворяются в небе без шва.
#[derive(Clone, Copy)]
pub struct Mood {
    pub sun: Vec3,
    pub sun_color: Color,
    pub lux: f32,
    /// Равномерная подсветка (в дополнение к свету неба из карты окружения).
    pub ambient: (Color, f32),
    /// Горизонт со стороны от солнца (линейный, в единицах экрана) — он же цвет дымки.
    pub horizon: Vec3,
    /// Добавка к горизонту в сторону солнца и показатель её резкости.
    pub horizon_sun: (Vec3, f32),
    pub zenith: Vec3,
    /// Облачность 0..1.
    pub clouds: f32,
    /// Цвета поглощения и рассеяния дымки (см. `FogFalloff::from_visibility_colors`).
    pub fog_far: (Color, Color),
    pub visibility: f32,
    pub shadow_distance: f32,
    pub day: bool,
    /// Яркость неба как источника света (кд/м² на единицу цвета неба).
    pub sky_light: f32,
}

impl Mood {
    pub fn fog(&self) -> Color {
        Color::linear_rgb(self.horizon.x, self.horizon.y, self.horizon.z)
    }
    pub fn sky_uniform(&self) -> SkyUniform {
        SkyUniform {
            sun: self.sun.normalize().extend(if self.day { 1.0 } else { 0.0 }),
            horizon: self.horizon.extend(self.clouds),
            horizon_sun: self.horizon_sun.0.extend(self.horizon_sun.1),
            zenith: self.zenith.extend(0.0),
        }
    }
}

/// Время суток в Херсонесе: `MRPG_TIME=day` — ясный полдень, иначе золотой час.
fn golden_hour() -> bool {
    !std::env::var("MRPG_TIME").is_ok_and(|t| t == "day")
}

pub fn mood() -> Mood {
    match crate::level::active() {
        crate::level::LevelKind::Castle => Mood {
            sun: SUN_DIR,
            sun_color: Color::srgb(1.0, 0.84, 0.66),
            lux: 11_000.0,
            ambient: (Color::srgb(0.62, 0.62, 0.8), 200.0),
            horizon: Vec3::new(0.44, 0.36, 0.46),
            horizon_sun: (Vec3::new(0.8, 0.22, 0.0), 6.0),
            zenith: Vec3::new(0.09, 0.11, 0.27),
            clouds: 0.5,
            fog_far: (Color::srgb(0.36, 0.36, 0.46), Color::srgb(0.62, 0.56, 0.60)),
            visibility: 380.0,
            shadow_distance: 160.0,
            day: false,
            sky_light: 700.0,
        },
        crate::level::LevelKind::Chersonesus if !golden_hour() => Mood {
            sun: Vec3::new(-0.72, 0.62, 0.22),
            sun_color: Color::srgb(1.0, 0.93, 0.8),
            lux: 15_000.0,
            ambient: (Color::srgb(0.6, 0.7, 0.9), 250.0),
            horizon: Vec3::new(0.62, 0.72, 0.86),
            horizon_sun: (Vec3::new(0.35, 0.28, 0.15), 8.0),
            zenith: Vec3::new(0.07, 0.2, 0.62),
            clouds: 0.45,
            fog_far: (Color::srgb(0.55, 0.64, 0.78), Color::srgb(0.80, 0.78, 0.72)),
            visibility: 1100.0,
            shadow_distance: 220.0,
            day: true,
            sky_light: 900.0,
        },
        // Золотой час: солнце (≈10° над горизонтом) садится в море на северо-западе (−X, −Z).
        // Длинные тёплые тени, золотая дорожка на воде, облака подсвечены снизу; тени —
        // холодные от синего неба, вдали — тёплая дымка, горы Таврики в розовом отсвете.
        crate::level::LevelKind::Chersonesus => Mood {
            sun: Vec3::new(-0.80, 0.175, -0.42),
            sun_color: Color::srgb(1.0, 0.8, 0.6),
            lux: 9_500.0,
            ambient: (Color::srgb(0.55, 0.58, 0.8), 110.0),
            horizon: Vec3::new(0.56, 0.48, 0.54),
            horizon_sun: (Vec3::new(1.5, 0.78, 0.24), 5.0),
            zenith: Vec3::new(0.11, 0.2, 0.46),
            clouds: 0.55,
            fog_far: (Color::srgb(0.5, 0.5, 0.56), Color::srgb(0.8, 0.77, 0.74)),
            visibility: 1600.0,
            shadow_distance: 240.0,
            day: false,
            sky_light: 950.0,
        },
    }
}

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/surface.wesl");
        embedded_asset!(app, "shaders/glass.wesl");
        embedded_asset!(app, "shaders/flame.wesl");
        embedded_asset!(app, "shaders/sky.wesl");
        embedded_asset!(app, "shaders/water.wesl");
        embedded_asset!(app, "shaders/smoke.wesl");
        // Общая функция неба — модуль, который импортируют небо, море и дым.
        bevy::shader::load_shader_library!(app, "shaders/sky_common.wesl");
        app.add_plugins((
            MaterialPlugin::<SurfaceMaterial>::default(),
            MaterialPlugin::<GlassMaterial>::default(),
            MaterialPlugin::<FlameMaterial>::default(),
            MaterialPlugin::<SkyMaterial>::default(),
            MaterialPlugin::<crate::sky::WaterMaterial>::default(),
            MaterialPlugin::<crate::smoke::SmokeMaterial>::default(),
        ))
        .insert_resource(GraphicsSettings { heavy: true })
        .add_observer(setup_camera)
        .add_systems(Startup, setup_environment)
        .add_systems(Update, (billboard_flames, flicker_lights, cull_far_lights, follow_sky, toggle_heavy, attach_sky_light));
    }

    fn finish(&self, app: &mut App) {
        // Фотоматериалы нужны раньше, чем уровень создаст свои материалы.
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let tex = load_surface_textures(&mut images);
        if tex.is_none() {
            warn!("Фотоматериалы не загружены — остаются процедурные поверхности");
        }
        let _ = SURFACE_TEXTURES.set(tex);
    }
}

#[derive(Resource)]
pub struct GraphicsSettings {
    pub heavy: bool,
}

// ---------- Камень, дерево, черепица и т. д. ----------

pub type SurfaceMaterial = ExtendedMaterial<StandardMaterial, SurfaceExt>;

/// Параметры поверхности (см. shaders/surface.wesl): процедурный узор + фотоматериал.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct SurfaceExt {
    /// Второй цвет: шов кладки, волокна дерева, тёмная плитка и т. п.
    #[uniform(100)]
    pub color_b: Vec4,
    /// x — узор, y — масштаб, z — разброс цвета, w — сила рельефа.
    #[uniform(100)]
    pub params: Vec4,
    /// x — мох, y — грязь у земли, z — свечение (угли), w — запас.
    #[uniform(100)]
    pub params2: Vec4,
    /// Фотоматериал: x — слой на горизонтальных гранях (−1 — без фото), y — 1/метров на тайл,
    /// z — слой на вертикальных гранях, w — 1/метров на тайл для него.
    #[uniform(100)]
    pub photo: Vec4,
    /// rgb — средний линейный цвет слоя x, w — насколько перекрашивать фото в цвет материала (0..1).
    #[uniform(100)]
    pub photo_mean: Vec4,
    /// rgb — средний линейный цвет слоя z, w — сила рельефа нормалей фото.
    #[uniform(100)]
    pub photo_mean_b: Vec4,
    /// Цвет фото (массив слоёв, sRGB).
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub albedo: Option<Handle<Image>>,
    /// Нормаль (RG) и шероховатость (B).
    #[texture(103, dimension = "2d_array")]
    pub nrm: Option<Handle<Image>>,
}

impl MaterialExtension for SurfaceExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/surface.wesl".into()
    }
}

// Номера узоров в шейдере.
const P_ASHLAR: f32 = 0.0;
const P_FLAGSTONE: f32 = 1.0;
const P_WOOD: f32 = 2.0;
const P_COBBLE: f32 = 3.0;
const P_GRASS: f32 = 4.0;
const P_CLOTH: f32 = 5.0;
const P_SLATE: f32 = 6.0;
const P_METAL: f32 = 7.0;
const P_WATER: f32 = 8.0;
const P_COALS: f32 = 9.0;
const P_LEAVES: f32 = 10.0;
const P_RUBBLE: f32 = 11.0;
const P_TILE: f32 = 12.0;
const P_PLASTER: f32 = 13.0;
const P_BRICK: f32 = 14.0;
const P_MIXTUM: f32 = 15.0;
const P_MARBLE: f32 = 16.0;
const P_MOSAIC: f32 = 17.0;
const P_EARTH: f32 = 18.0;
const P_TERRAIN: f32 = 19.0;
const P_PLAIN: f32 = 30.0;

fn lin(c: Color) -> Vec4 {
    let l = c.to_linear();
    Vec4::new(l.red, l.green, l.blue, 1.0)
}

/// Материал для каждого типа поверхности уровня.
pub fn surface_material(mat: Mat) -> SurfaceMaterial {
    // (основной цвет, второй цвет, узор, масштаб, разброс, рельеф, шероховатость, металл, мох, грязь, свечение)
    let (a, b, pattern, scale, var, bump, rough, metal, moss, grime, glow) = match mat {
        Mat::Grass => (Color::srgb(0.13, 0.22, 0.08), Color::srgb(0.24, 0.26, 0.12), P_GRASS, 1.0, 0.5, 0.4, 0.95, 0.0, 0.0, 0.0, 0.0),
        Mat::Cobble => (Color::srgb(0.36, 0.34, 0.32), Color::srgb(0.12, 0.11, 0.10), P_COBBLE, 1.0, 0.35, 1.0, 0.85, 0.0, 0.35, 0.0, 0.0),
        Mat::Stone => (Color::srgb(0.52, 0.51, 0.49), Color::srgb(0.22, 0.21, 0.20), P_ASHLAR, 0.8, 0.25, 1.0, 0.88, 0.0, 0.5, 0.6, 0.0),
        Mat::DarkStone => (Color::srgb(0.38, 0.37, 0.38), Color::srgb(0.14, 0.13, 0.14), P_ASHLAR, 0.9, 0.25, 1.0, 0.85, 0.0, 0.4, 0.6, 0.0),
        Mat::Floor => (Color::srgb(0.42, 0.38, 0.34), Color::srgb(0.16, 0.14, 0.13), P_FLAGSTONE, 1.0, 0.3, 0.8, 0.55, 0.0, 0.0, 0.2, 0.0),
        Mat::Wood => (Color::srgb(0.34, 0.22, 0.12), Color::srgb(0.18, 0.11, 0.06), P_WOOD, 1.0, 0.35, 0.6, 0.75, 0.0, 0.0, 0.0, 0.0),
        Mat::Banner => (Color::srgb(0.50, 0.04, 0.05), Color::srgb(0.85, 0.62, 0.20), P_CLOTH, 1.0, 0.2, 0.3, 0.9, 0.0, 0.0, 0.0, 0.0),
        Mat::Gold => (Color::srgb(0.90, 0.68, 0.30), Color::srgb(0.55, 0.38, 0.12), P_METAL, 1.0, 0.2, 0.3, 0.3, 1.0, 0.0, 0.0, 0.0),
        Mat::Trim => (Color::srgb(0.62, 0.58, 0.52), Color::srgb(0.30, 0.28, 0.25), P_ASHLAR, 1.6, 0.15, 0.5, 0.8, 0.0, 0.3, 0.4, 0.0),
        Mat::Roof => (Color::srgb(0.20, 0.22, 0.27), Color::srgb(0.08, 0.09, 0.11), P_SLATE, 1.0, 0.3, 1.0, 0.6, 0.0, 0.25, 0.0, 0.0),
        Mat::Iron => (Color::srgb(0.22, 0.21, 0.20), Color::srgb(0.35, 0.20, 0.12), P_METAL, 1.0, 0.3, 0.3, 0.45, 0.9, 0.0, 0.0, 0.0),
        Mat::Coals => (Color::srgb(0.08, 0.05, 0.04), Color::srgb(1.0, 0.35, 0.08), P_COALS, 1.0, 0.5, 0.5, 0.9, 0.0, 0.0, 0.0, 6.0),
        Mat::Water => (Color::srgb(0.06, 0.13, 0.16), Color::srgb(0.10, 0.2, 0.22), P_WATER, 1.0, 0.0, 0.3, 0.05, 0.0, 0.0, 0.0, 0.0),
        Mat::Leaves => (Color::srgb(0.07, 0.15, 0.07), Color::srgb(0.16, 0.24, 0.09), P_LEAVES, 1.0, 0.4, 1.0, 0.9, 0.0, 0.0, 0.0, 0.0),
        Mat::Paving => (Color::srgb(0.66, 0.61, 0.52), Color::srgb(0.36, 0.33, 0.28), P_FLAGSTONE, 0.8, 0.3, 0.9, 0.8, 0.0, 0.1, 0.0, 0.0),
        Mat::Limestone => (Color::srgb(0.70, 0.64, 0.52), Color::srgb(0.42, 0.38, 0.31), P_ASHLAR, 0.62, 0.22, 1.0, 0.9, 0.0, 0.25, 0.55, 0.0),
        Mat::Rubble => (Color::srgb(0.62, 0.57, 0.47), Color::srgb(0.55, 0.50, 0.42), P_RUBBLE, 1.0, 0.35, 1.2, 0.92, 0.0, 0.15, 0.6, 0.0),
        Mat::Plaster => (Color::srgb(0.86, 0.83, 0.76), Color::srgb(0.66, 0.60, 0.50), P_PLASTER, 1.0, 0.18, 0.3, 0.9, 0.0, 0.05, 0.8, 0.0),
        Mat::Tile => (Color::srgb(0.56, 0.29, 0.18), Color::srgb(0.30, 0.15, 0.09), P_TILE, 1.0, 0.25, 1.2, 0.7, 0.0, 0.35, 0.0, 0.0),
        Mat::Brick => (Color::srgb(0.55, 0.25, 0.14), Color::srgb(0.68, 0.63, 0.55), P_BRICK, 1.0, 0.25, 0.8, 0.9, 0.0, 0.0, 0.5, 0.0),
        Mat::OpusMixtum => (Color::srgb(0.68, 0.63, 0.53), Color::srgb(0.62, 0.58, 0.50), P_MIXTUM, 1.0, 0.25, 1.0, 0.9, 0.0, 0.1, 0.55, 0.0),
        Mat::Marble => (Color::srgb(0.88, 0.87, 0.84), Color::srgb(0.52, 0.55, 0.60), P_MARBLE, 1.0, 0.1, 0.25, 0.3, 0.0, 0.0, 0.2, 0.0),
        Mat::Mosaic => (Color::srgb(0.85, 0.80, 0.70), Color::srgb(0.58, 0.18, 0.12), P_MOSAIC, 1.0, 0.1, 0.4, 0.55, 0.0, 0.0, 0.0, 0.0),
        Mat::Earth => (Color::srgb(0.48, 0.41, 0.32), Color::srgb(0.66, 0.62, 0.55), P_EARTH, 1.0, 0.3, 0.7, 0.95, 0.0, 0.0, 0.0, 0.0),
        Mat::Terrain => (Color::srgb(0.30, 0.33, 0.15), Color::srgb(0.60, 0.57, 0.50), P_TERRAIN, 1.0, 0.3, 0.8, 0.95, 0.0, 0.0, 0.0, 0.0),
        Mat::Signinum => (Color::srgb(0.68, 0.45, 0.38), Color::srgb(0.50, 0.33, 0.28), P_PLASTER, 1.6, 0.25, 0.4, 0.8, 0.0, 0.1, 0.6, 0.0),
        Mat::Canvas => (Color::srgb(0.78, 0.72, 0.60), Color::srgb(0.6, 0.55, 0.45), P_PLAIN, 1.0, 0.15, 0.2, 0.95, 0.0, 0.0, 0.0, 0.0),
        Mat::Felt => (Color::srgb(0.62, 0.56, 0.47), Color::srgb(0.4, 0.34, 0.28), P_PLAIN, 1.0, 0.25, 0.4, 1.0, 0.0, 0.0, 0.4, 0.0),
        // Шерсть, крашенная мареной (ковры, покрывала) и вайдой (покрывала, занавеси).
        Mat::Rug => (Color::srgb(0.5, 0.17, 0.12), Color::srgb(0.32, 0.1, 0.07), P_PLAIN, 1.0, 0.3, 0.4, 1.0, 0.0, 0.0, 0.3, 0.0),
        Mat::Cloth => (Color::srgb(0.24, 0.3, 0.46), Color::srgb(0.14, 0.18, 0.3), P_PLAIN, 1.0, 0.25, 0.3, 1.0, 0.0, 0.0, 0.2, 0.0),
        Mat::Thatch => (Color::srgb(0.55, 0.45, 0.28), Color::srgb(0.33, 0.27, 0.16), P_WOOD, 0.5, 0.4, 1.0, 1.0, 0.0, 0.3, 0.0, 0.0),
        Mat::Ceramic => (Color::srgb(0.66, 0.40, 0.26), Color::srgb(0.5, 0.3, 0.2), P_PLAIN, 1.0, 0.2, 0.15, 0.75, 0.0, 0.0, 0.3, 0.0),
        Mat::Bronze => (Color::srgb(0.55, 0.40, 0.22), Color::srgb(0.25, 0.42, 0.33), P_METAL, 1.0, 0.25, 0.3, 0.4, 0.9, 0.0, 0.0, 0.0),
        Mat::Conifer => (Color::srgb(0.08, 0.15, 0.10), Color::srgb(0.12, 0.2, 0.12), P_LEAVES, 1.6, 0.35, 1.0, 0.9, 0.0, 0.0, 0.0, 0.0),
        Mat::HerbGreen => (Color::srgb(0.16, 0.30, 0.10), Color::srgb(0.25, 0.36, 0.14), P_LEAVES, 3.0, 0.35, 0.6, 0.85, 0.0, 0.0, 0.0, 0.0),
        Mat::HerbSilver => (Color::srgb(0.42, 0.48, 0.40), Color::srgb(0.55, 0.58, 0.50), P_LEAVES, 3.0, 0.25, 0.6, 0.9, 0.0, 0.0, 0.0, 0.0),
        Mat::PetalYellow => (Color::srgb(0.95, 0.75, 0.10), Color::srgb(0.8, 0.6, 0.1), P_PLAIN, 1.0, 0.1, 0.0, 0.7, 0.0, 0.0, 0.0, 0.0),
        Mat::PetalPurple => (Color::srgb(0.50, 0.30, 0.65), Color::srgb(0.4, 0.2, 0.5), P_PLAIN, 1.0, 0.1, 0.0, 0.7, 0.0, 0.0, 0.0, 0.0),
        Mat::PetalWhite => (Color::srgb(0.92, 0.90, 0.84), Color::srgb(0.8, 0.8, 0.7), P_PLAIN, 1.0, 0.1, 0.0, 0.7, 0.0, 0.0, 0.0, 0.0),
        Mat::PetalRed => (Color::srgb(0.80, 0.08, 0.05), Color::srgb(0.6, 0.05, 0.03), P_PLAIN, 1.0, 0.1, 0.0, 0.7, 0.0, 0.0, 0.0, 0.0),
        Mat::PetalPink => (Color::srgb(0.85, 0.50, 0.65), Color::srgb(0.7, 0.4, 0.5), P_PLAIN, 1.0, 0.1, 0.0, 0.7, 0.0, 0.0, 0.0, 0.0),
    };
    let photo = photo_layers(mat);
    let tex = surface_textures();
    let has_photo = tex.is_some() && photo.is_some();
    let (top, side, tint, relief) = photo.unwrap_or((0, 0, 0.0, 0.0));
    let layer = |l: u32| &crate::surface_layers::LAYERS[l as usize];
    let mean = |l: u32| Vec3::from_array(layer(l).mean);
    ExtendedMaterial {
        base: StandardMaterial { base_color: a, perceptual_roughness: rough, metallic: metal, reflectance: 0.4, ..default() },
        extension: SurfaceExt {
            color_b: lin(b),
            params: Vec4::new(pattern, scale, var, bump),
            params2: Vec4::new(moss, grime, glow, 0.0),
            photo: if has_photo { Vec4::new(top as f32, 1.0 / layer(top).meters, side as f32, 1.0 / layer(side).meters) } else { Vec4::new(-1.0, 1.0, -1.0, 1.0) },
            // Воде вместо фото передаём солнце (для отражений неба и солнечной дорожки) и «день».
            photo_mean: if mat == Mat::Water { mood().sun.normalize().extend(0.0) } else { mean(top).extend(tint) },
            photo_mean_b: if mat == Mat::Water { Vec4::new(if mood().day { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0) } else { mean(side).extend(relief) },
            albedo: tex.map(|t| t.0.clone()),
            nrm: tex.map(|t| t.1.clone()),
        },
    }
}

/// Какие фотослои берёт материал: (горизонтальные грани, вертикальные, перекраска 0..1, сила нормалей).
/// `None` — остаётся чисто процедурным (мозаика, листва, вода, угли, знамёна, лепестки).
fn photo_layers(mat: Mat) -> Option<(u32, u32, f32, f32)> {
    use crate::surface_layers::*;
    Some(match mat {
        Mat::DarkStone => (L_ASHLAR, L_ASHLAR, 0.85, 1.0),
        // Местный известняк Херсонеса — тёплый серо-бежевый: оставляем фото почти как есть.
        Mat::Stone | Mat::Trim => (L_ASHLAR, L_ASHLAR, 0.5, 1.0),
        Mat::Limestone => (L_ASHLAR, L_ASHLAR, 0.75, 1.0),
        Mat::Floor => (L_FLAGSTONE, L_ASHLAR, 0.8, 0.8),
        Mat::Paving => (L_FLAGSTONE, L_ASHLAR, 0.6, 0.9),
        Mat::Cobble => (L_COBBLE, L_ASHLAR, 0.5, 1.0),
        Mat::Grass => (L_GRASS, L_SOIL, 0.55, 0.8),
        Mat::Rubble => (L_RUBBLE, L_RUBBLE, 0.6, 1.0),
        Mat::Plaster => (L_PLASTER, L_PLASTER, 0.9, 0.6),
        Mat::Signinum => (L_PLASTER, L_PLASTER, 1.0, 0.5),
        Mat::Brick => (L_BRICK, L_BRICK, 0.5, 1.0),
        Mat::OpusMixtum => (L_ASHLAR, L_BRICK, 0.7, 1.0),
        Mat::Marble => (L_MARBLE, L_MARBLE, 0.6, 0.4),
        Mat::Earth => (L_EARTH, L_EARTH, 0.6, 0.9),
        Mat::Terrain => (L_GRASS, L_ROCK, 0.5, 1.0),
        Mat::Wood => (L_WOOD, L_WOOD, 0.85, 0.9),
        // Своя черепица (запечена в Blender) уже нужного тона — перекрашиваем слегка.
        Mat::Tile => (L_ROOF, L_ROOF, 0.15, 1.0),
        Mat::Iron => (L_IRON, L_IRON, 0.9, 0.6),
        Mat::Bronze => (L_BRONZE, L_BRONZE, 0.5, 0.6),
        Mat::Gold => (L_GOLD, L_GOLD, 0.4, 0.5),
        Mat::Canvas | Mat::Felt | Mat::Rug | Mat::Cloth => (L_WOOL, L_WOOL, 1.0, 0.7),
        _ => return None,
    })
}

static SURFACE_TEXTURES: std::sync::OnceLock<Option<(Handle<Image>, Handle<Image>)>> = std::sync::OnceLock::new();

fn surface_textures() -> Option<&'static (Handle<Image>, Handle<Image>)> {
    SURFACE_TEXTURES.get().and_then(|t| t.as_ref())
}

/// Собирает массивы слоёв фотоматериалов (assets/textures/surface) с мип-уровнями.
fn load_surface_textures(images: &mut Assets<Image>) -> Option<(Handle<Image>, Handle<Image>)> {
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension};
    let dir = bevy::asset::io::file::FileAssetReader::get_base_path().join("assets/textures/surface");
    let layers = crate::surface_layers::LAYERS;
    let load = |suffix: &str, srgb: bool| -> Option<Image> {
        // Слои декодируем и уменьшаем параллельно — это ~40 картинок 1024².
        let decoded: Vec<Option<Image>> = std::thread::scope(|s| {
            let jobs: Vec<_> = layers
                .iter()
                .map(|l| {
                    let path = dir.join(format!("{}_{}.jpg", l.name, suffix));
                    s.spawn(move || {
                        let bytes = std::fs::read(&path).map_err(|e| warn!("нет текстуры {}: {e}", path.display())).ok()?;
                        let mut img = Image::from_buffer(&bytes, ImageType::Extension("jpg"), CompressedImageFormats::NONE, srgb, ImageSampler::Default, RenderAssetUsages::default()).ok()?;
                        crate::textures::generate_mips(&mut img, suffix == "nrm");
                        Some(img)
                    })
                })
                .collect();
            jobs.into_iter().map(|j| j.join().ok().flatten()).collect()
        });
        let decoded: Vec<Image> = decoded.into_iter().collect::<Option<_>>()?;
        let first = &decoded[0];
        let (w, h) = (first.width(), first.height());
        if decoded.iter().any(|i| i.width() != w || i.height() != h) {
            warn!("слои фотоматериалов разного размера");
            return None;
        }
        let levels = first.texture_descriptor.mip_level_count;
        let mut data = Vec::new();
        for img in &decoded {
            data.extend_from_slice(img.data.as_ref()?);
        }
        let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
        let mut arr = Image::new_uninit(Extent3d { width: w, height: h, depth_or_array_layers: decoded.len() as u32 }, TextureDimension::D2, format, RenderAssetUsages::RENDER_WORLD);
        arr.data = Some(data);
        arr.texture_descriptor.mip_level_count = levels;
        arr.sampler = crate::textures::quality_sampler(true);
        arr.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });
        Some(arr)
    };
    let started = std::time::Instant::now();
    let albedo = load("albedo", true)?;
    let nrm = load("nrm", false)?;
    info!("Фотоматериалы: {} слоёв за {:.2} с", layers.len(), started.elapsed().as_secs_f32());
    Some((images.add(albedo), images.add(nrm)))
}

// ---------- Витражи ----------

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GlassMaterial {
    /// x — вид (0 ланцет, 1 роза), y — яркость, z, w — запас.
    #[uniform(0)]
    pub params: Vec4,
}

impl Material for GlassMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/glass.wesl".into()
    }
}

pub fn glass_material(rose: bool) -> GlassMaterial {
    GlassMaterial { params: Vec4::new(if rose { 1.0 } else { 0.0 }, 2.6, 0.0, 0.0) }
}

// ---------- Пламя факелов ----------

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct FlameMaterial {
    #[uniform(0)]
    pub params: Vec4,
}

impl Material for FlameMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/flame.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
}

/// Пламя всегда повёрнуто к камере (по вертикальной оси).
#[derive(Component)]
pub struct Billboard;

/// Мерцание источника света.
#[derive(Component)]
pub struct Flicker {
    pub base: f32,
    pub phase: f32,
}

pub fn spawn_flame(commands: &mut Commands, mesh: Handle<Mesh>, material: Handle<FlameMaterial>, pos: Vec3, out: Vec3, phase: f32) {
    commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_translation(pos), Billboard, NotShadowCaster, NotShadowReceiver));
    // Свет чуть отодвинут от стены, чтобы стена освещалась ровнее.
    let intensity = 45_000.0;
    commands.spawn((
        PointLight { color: Color::srgb(1.0, 0.58, 0.28), intensity, range: 13.0, radius: 0.15, shadow_maps_enabled: false, ..default() },
        Transform::from_translation(pos + out * 0.35 + Vec3::Y * 0.1),
        Flicker { base: intensity, phase },
    ));
}

fn billboard_flames(cameras: Query<&GlobalTransform, With<Camera3d>>, mut flames: Query<&mut Transform, With<Billboard>>) {
    let Some(cam) = cameras.iter().next() else { return };
    let cam_pos = cam.translation();
    for mut t in &mut flames {
        let d = cam_pos - t.translation;
        let yaw = d.x.atan2(d.z);
        t.rotation = Quat::from_rotation_y(yaw);
    }
}

fn flicker_lights(time: Res<Time>, mut lights: Query<(&mut PointLight, &Flicker)>) {
    let t = time.elapsed_secs();
    for (mut light, f) in &mut lights {
        let p = f.phase;
        let n = (t * 9.1 + p).sin() * 0.5 + (t * 23.7 + p * 2.3).sin() * 0.3 + (t * 4.3 + p * 0.7).sin() * 0.2;
        light.intensity = f.base * (0.85 + 0.12 * n);
    }
}

/// Далёкие факелы и светильники выключаем: в городе их сотни, а освещают они лишь пару комнат.
fn cull_far_lights(time: Res<Time>, mut next: Local<f32>, cameras: Query<&GlobalTransform, With<Camera3d>>, mut lights: Query<(&GlobalTransform, &PointLight, &mut Visibility), With<Flicker>>) {
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + 0.25;
    let Some(cam) = cameras.iter().next() else { return };
    let c = cam.translation();
    for (t, light, mut vis) in &mut lights {
        let far = 35.0 + light.range * 2.0;
        let want = if t.translation().distance_squared(c) < far * far { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

// ---------- Небо ----------

#[derive(Component)]
struct SkyDome;

/// Сфера, вывернутая внутрь (камера внутри неё).
fn sky_mesh(radius: f32) -> Mesh {
    let (rings, segs) = (32usize, 64usize);
    let mut pos = Vec::new();
    let mut nrm = Vec::new();
    let mut uv = Vec::new();
    for r in 0..=rings {
        let phi = r as f32 / rings as f32 * std::f32::consts::PI;
        for s in 0..=segs {
            let th = s as f32 / segs as f32 * std::f32::consts::TAU;
            let d = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
            pos.push((d * radius).to_array());
            nrm.push((-d).to_array());
            uv.push([s as f32 / segs as f32, r as f32 / rings as f32]);
        }
    }
    let mut idx = Vec::new();
    let w = segs as u32 + 1;
    for r in 0..rings as u32 {
        for s in 0..segs as u32 {
            let (a, b, c, d) = (r * w + s, r * w + s + 1, (r + 1) * w + s, (r + 1) * w + s + 1);
            idx.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(idx))
}

fn setup_environment(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut skies: ResMut<Assets<SkyMaterial>>, mut images: ResMut<Assets<Image>>) {
    let m = mood();
    commands.insert_resource(ClearColor(m.fog()));
    // Свет неба: то же небо в кубической карте, отфильтрованной на процессоре один раз.
    let started = std::time::Instant::now();
    let (diffuse, specular) = crate::sky::sky_environment(&m.sky_uniform());
    info!("Карта окружения неба за {:.2} с", started.elapsed().as_secs_f32());
    commands.insert_resource(crate::sky::SkyEnvironment { diffuse: images.add(diffuse), specular: images.add(specular) });
    // Тени в 4096² на каскад: края теней от карнизов и колонн не «лесенкой».
    commands.insert_resource(bevy::light::DirectionalLightShadowMap { size: 4096 });
    commands.insert_resource(GlobalAmbientLight { color: m.ambient.0, brightness: m.ambient.1, ..default() });

    let sun = m.sun.normalize();
    commands.spawn((
        Mesh3d(meshes.add(sky_mesh(900.0))),
        MeshMaterial3d(skies.add(SkyMaterial { sky: m.sky_uniform() })),
        Transform::IDENTITY,
        SkyDome,
        NotShadowCaster,
        NotShadowReceiver,
    ));

    // Закатное солнце.
    commands.spawn((
        DirectionalLight { color: m.sun_color, illuminance: m.lux, shadow_maps_enabled: true, ..default() },
        Transform::from_translation(sun * 100.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 4, first_cascade_far_bound: 12.0, maximum_distance: m.shadow_distance, ..default() }.build(),
        VolumetricLight,
    ));
    // Лёгкая дымка: в ней видны лучи низкого солнца между домами, из окон и от факелов
    // (днём у моря её нет). В Херсонесе она гуще у земли — вечерний дым очагов и морская мгла.
    match (m.day, crate::level::active()) {
        (true, _) => {}
        (false, crate::level::LevelKind::Castle) => {
            commands.spawn((
                FogVolume { fog_color: Color::srgb(0.85, 0.8, 0.8), density_factor: 0.03, absorption: 0.0, scattering: 0.25, scattering_asymmetry: 0.6, ..default() },
                Transform::from_xyz(0.0, 20.0, 0.0).with_scale(Vec3::new(600.0, 40.0, 600.0)),
            ));
        }
        (false, crate::level::LevelKind::Chersonesus) => {
            commands.spawn((
                FogVolume { fog_color: Color::srgb(0.95, 0.9, 0.88), density_factor: 0.0012, absorption: 0.0, scattering: 0.3, scattering_asymmetry: 0.72, ..default() },
                Transform::from_xyz(0.0, 6.0, 0.0).with_scale(Vec3::new(1800.0, 14.0, 1800.0)),
            ));
        }
    }
}

fn follow_sky(cameras: Query<&GlobalTransform, With<Camera3d>>, mut sky: Query<&mut Transform, With<SkyDome>>) {
    let Some(cam) = cameras.iter().next() else { return };
    for mut t in &mut sky {
        t.translation = cam.translation();
    }
}

// ---------- Камера и пост-обработка ----------

fn setup_camera(event: On<Add<Camera3d>>, mut commands: Commands, settings: Res<GraphicsSettings>) {
    let m = mood();
    let mut e = commands.entity(event.entity);
    e.insert((
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom { intensity: if m.day { 0.16 } else { 0.2 }, ..Bloom::NATURAL },
        color_grading(&m),
        // Лёгкое затемнение углов кадра, как у объектива.
        Vignette { intensity: 0.32, radius: 0.85, smoothness: 3.0, ..default() },
        // Дальняя дымка того же цвета, что горизонт неба (см. `Mood`).
        DistanceFog {
            color: m.fog(),
            // Свечение дымки к солнцу — слабее, чем у неба: иначе город против солнца тонет в оранжевом.
            directional_light_color: Color::linear_rgba(m.horizon_sun.0.x * 0.55, m.horizon_sun.0.y * 0.55, m.horizon_sun.0.z * 0.55, 1.0),
            directional_light_exponent: m.horizon_sun.1,
            falloff: FogFalloff::from_visibility_colors(m.visibility, m.fog_far.0, m.fog_far.1),
        },
        Msaa::Off,
        TemporalAntiAliasing::default(),
        ShadowFilteringMethod::Temporal,
    ));
    if settings.heavy {
        e.insert(heavy_effects());
    }
}

/// Цветокоррекция: чуть больше контраста в средних тонах, насыщенные света (закатное
/// золото), чуть приглушённые тени — как на плёнке.
fn color_grading(m: &Mood) -> ColorGrading {
    let section = |saturation: f32, contrast: f32| ColorGradingSection { saturation, contrast, ..default() };
    ColorGrading {
        global: ColorGradingGlobal { exposure: if m.day { 0.3 } else { 0.2 }, post_saturation: 1.06, ..default() },
        shadows: section(0.92, 1.0),
        midtones: section(1.04, 1.08),
        highlights: section(1.1, 1.0),
    }
}

/// Свет неба (карта окружения) — на каждую 3D-камеру, как только карта готова.
fn attach_sky_light(mut commands: Commands, env: Option<Res<crate::sky::SkyEnvironment>>, cameras: Query<Entity, (With<Camera3d>, Without<EnvironmentMapLight>)>) {
    let Some(env) = env else { return };
    let m = mood();
    for e in &cameras {
        commands.entity(e).insert(crate::sky::sky_environment_light(&env, &m));
    }
}

fn heavy_effects() -> impl Bundle {
    (ScreenSpaceAmbientOcclusion::default(), VolumetricFog { ambient_intensity: 0.0, step_count: 48, jitter: 0.5, ..default() })
}

fn toggle_heavy(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<GraphicsSettings>, cameras: Query<Entity, With<Camera3d>>, mut commands: Commands) {
    if !keys.just_pressed(KeyCode::F9) {
        return;
    }
    settings.heavy = !settings.heavy;
    info!("Тяжёлые эффекты (SSAO, объёмный туман): {}", if settings.heavy { "вкл" } else { "выкл" });
    for e in &cameras {
        if settings.heavy {
            commands.entity(e).insert(heavy_effects());
        } else {
            commands.entity(e).remove::<(ScreenSpaceAmbientOcclusion, VolumetricFog)>();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::surface_layers::*;

    /// Шейдер рельефа берёт почву и гальку по номерам слоёв напрямую.
    #[test]
    fn terrain_layers_match_shader() {
        assert_eq!(L_SOIL, 8);
        assert_eq!(L_GRAVEL, 18);
    }

    #[test]
    fn every_surface_layer_file_exists() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/textures/surface");
        for l in LAYERS {
            for suffix in ["albedo", "nrm"] {
                let p = dir.join(format!("{}_{}.jpg", l.name, suffix));
                assert!(p.exists(), "нет {}", p.display());
            }
        }
    }
}
