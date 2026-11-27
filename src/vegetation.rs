//! Растительность и камни Таврики: деревья и кусты-модели с листвой-карточками и LOD,
//! фотосканы известняковых глыб и трава у ног игрока. Всё качается на ветру
//! (shaders/foliage.wesl), далёкие деревья — «импосторы» (две скрещённые плоскости).
//!
//! Модели собираются в Blender: tools/vegetation/blender_trees.py (деревья),
//! tools/vegetation/blender_rocks.py (сканы Poly Haven, CC0). Где стоят деревья и камни,
//! решает раскладка уровня ([`crate::level::Plant`]); траву клиент сеет сам вокруг камеры.
//!
//! `MRPG_GRASS=0` выключает траву, `MRPG_GRASS=0.5` — вдвое реже.

use crate::level::{Plant, PlantKind};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{Gltf, GltfMesh};
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use std::collections::HashMap;

pub struct VegetationPlugin;

impl Plugin for VegetationPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/foliage.wesl");
        app.add_plugins(MaterialPlugin::<FoliageMaterial>::default())
            .init_resource::<Grass>()
            .add_systems(Startup, load_vegetation)
            .add_systems(Update, (spawn_plants, update_wind, update_grass));
    }
}

/// Множитель цвета коры (как BARK_TINT в blender_trees.py): Bark014 слишком светлый.
const BARK_TINT: [f32; 3] = [0.42, 0.36, 0.31];

/// Виды деревьев: файл модели (`models/vegetation/<id>_<n>.glb`) и число вариантов.
const TREES: &[(PlantKind, &str, u32)] = &[
    (PlantKind::Pine, "pine", 3),
    (PlantKind::Oak, "oak", 3),
    (PlantKind::Juniper, "juniper", 3),
    (PlantKind::Cypress, "cypress", 2),
    (PlantKind::Fig, "fig", 2),
    (PlantKind::Shrub, "shrub", 3),
];

/// Фотосканы камней (Poly Haven, CC0), `models/rocks/<id>.glb`.
const ROCKS: &[&str] = &["rock_07", "rock_09", "stone_01", "namaqualand_boulder_02", "rock_face_01"];

// ---------------------------------------------------------------------------------------------
// Материал листвы с ветром

pub type FoliageMaterial = ExtendedMaterial<StandardMaterial, FoliageExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct FoliageExt {
    /// x — не используется (время шейдер берёт из `globals`), y — сила ветра, z — дрожь листвы, w — запас.
    #[uniform(100)]
    pub wind: Vec4,
    /// xy — направление ветра, z — гибкость (1 — листва, 0.3 — кора), w — запас.
    #[uniform(100)]
    pub dir: Vec4,
}

impl MaterialExtension for FoliageExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/foliage.wesl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/foliage.wesl".into()
    }
}

/// Сила ветра для вида материала: (сила, дрожь, гибкость).
fn wind_of(kind: WindKind) -> (f32, f32, f32) {
    match kind {
        WindKind::Bark => (0.05, 0.0, 0.3),
        WindKind::Leaves => (0.12, 1.0, 1.0),
        WindKind::Grass => (0.16, 0.6, 1.0),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindKind {
    Bark,
    Leaves,
    Grass,
}

// ---------------------------------------------------------------------------------------------
// Загрузка

#[derive(Resource)]
struct VegetationAssets {
    trees: Vec<(PlantKind, String, Handle<Gltf>)>,
    rocks: Vec<(String, Handle<Gltf>)>,
    atlases: HashMap<&'static str, Handle<Image>>,
    bark: (Handle<Image>, Handle<Image>),
    impostors: HashMap<String, Handle<Image>>,
}

/// Материалы с ветром — им обновляются сила и направление ветра (см. [`update_wind`]).
#[derive(Resource, Default)]
struct WindMaterials(Vec<(Handle<FoliageMaterial>, WindKind)>);

/// Растения, которые раскладка уровня передала клиенту (кладёт `level::spawn_level`).
#[derive(Resource)]
pub struct PendingPlants(pub Vec<Plant>);

/// Читает картинку из assets/ сразу (без загрузчика), с мип-уровнями; у листвы — с сохранением
/// доли непрозрачных пикселей на мипах, чтобы крона вдали не редела.
fn load_image(images: &mut Assets<Image>, rel: &str, srgb: bool, foliage: bool) -> Handle<Image> {
    let path = bevy::asset::io::file::FileAssetReader::get_base_path().join("assets").join(rel);
    let ext = if rel.ends_with(".png") { "png" } else { "jpg" };
    let img = std::fs::read(&path)
        .map_err(|e| warn!("нет картинки {}: {e}", path.display()))
        .ok()
        .and_then(|bytes| Image::from_buffer(&bytes, ImageType::Extension(ext), CompressedImageFormats::NONE, srgb, ImageSampler::Default, RenderAssetUsages::RENDER_WORLD).ok());
    let Some(img) = img else {
        return images.add(Image::default());
    };
    let img = img.convert(if srgb { bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb } else { bevy::render::render_resource::TextureFormat::Rgba8Unorm }).unwrap_or(img.clone());
    let mut img = img;
    crate::textures::generate_mips(&mut img, false);
    if foliage {
        keep_alpha_coverage(&mut img, 0.45);
    }
    img.sampler = crate::textures::quality_sampler(!foliage);
    images.add(img)
}

/// На каждом мип-уровне подбирает множитель альфы так, чтобы доля пикселей выше `cutoff`
/// была как на исходной картинке (иначе листва на расстоянии становится прозрачной).
fn keep_alpha_coverage(img: &mut Image, cutoff: f32) {
    let size = img.texture_descriptor.size;
    let levels = img.texture_descriptor.mip_level_count;
    let Some(data) = img.data.as_mut() else { return };
    let coverage = |px: &[u8], scale: f32| px.chunks_exact(4).filter(|p| p[3] as f32 / 255.0 * scale > cutoff).count() as f32 / (px.len() / 4).max(1) as f32;
    let (mut w, mut h) = (size.width as usize, size.height as usize);
    let mut off = 0;
    let mut target = 0.0;
    for level in 0..levels {
        let len = w * h * 4;
        if off + len > data.len() {
            break;
        }
        let px = &mut data[off..off + len];
        if level == 0 {
            target = coverage(px, 1.0);
        } else {
            let (mut lo, mut hi) = (0.5f32, 4.0f32);
            for _ in 0..12 {
                let mid = (lo + hi) / 2.0;
                if coverage(px, mid) < target {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            for p in px.chunks_exact_mut(4) {
                p[3] = (p[3] as f32 * hi).min(255.0) as u8;
            }
        }
        off += len;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
}

fn load_vegetation(mut commands: Commands, server: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    if crate::level::active() != crate::level::LevelKind::Chersonesus {
        return;
    }
    let started = std::time::Instant::now();
    let mut trees = Vec::new();
    let mut impostors = HashMap::new();
    for &(kind, name, n) in TREES {
        for v in 0..n {
            let id = format!("{name}_{v}");
            trees.push((kind, id.clone(), server.load(format!("models/vegetation/{id}.glb"))));
            impostors.insert(id.clone(), load_image(&mut images, &format!("textures/vegetation/{id}_imp.png"), true, true));
        }
    }
    let rocks = ROCKS.iter().map(|r| (r.to_string(), server.load(format!("models/rocks/{r}.glb")))).collect();
    let mut atlases = HashMap::new();
    for a in ["broadleaf", "pine", "scale"] {
        atlases.insert(a, load_image(&mut images, &format!("textures/vegetation/{a}_atlas.png"), true, true));
    }
    let bark = (
        load_image(&mut images, "textures/vegetation/bark_albedo.jpg", true, false),
        load_image(&mut images, "textures/vegetation/bark_nrm.jpg", false, false),
    );
    info!("Растительность: текстуры за {:.2} с", started.elapsed().as_secs_f32());
    commands.insert_resource(VegetationAssets { trees, rocks, atlases, bark, impostors });
    commands.init_resource::<WindMaterials>();
}

// ---------------------------------------------------------------------------------------------
// Деревья, кусты, камни

/// Готовая модель: меши LOD и материалы.
struct TreeModel {
    kind: PlantKind,
    height: f32,
    lod0: (Handle<Mesh>, Handle<Mesh>),
    lod1: (Handle<Mesh>, Handle<Mesh>),
    imp: Handle<Mesh>,
    bark: Handle<FoliageMaterial>,
    leaf: Handle<FoliageMaterial>,
    imp_mat: Handle<FoliageMaterial>,
}

struct RockModel {
    size: f32,
    lod0: Handle<Mesh>,
    lod1: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
}

/// Листва и цвет коры по видам: (атлас, оттенок листвы, оттенок коры).
fn look(kind: PlantKind) -> (&'static str, Color, [f32; 3]) {
    match kind {
        // Хвоя крымской сосны тёмная, кора серо-бурая.
        PlantKind::Pine => ("pine", Color::srgb(0.95, 1.0, 0.9), [0.92, 0.84, 0.84]),
        // Дуб пушистый: листья снизу опушённые — серовато-зелёная крона.
        PlantKind::Oak => ("broadleaf", Color::srgb(0.88, 0.92, 0.8), [0.9, 0.9, 0.9]),
        // Можжевельник высокий — сизый.
        PlantKind::Juniper => ("scale", Color::srgb(0.95, 1.0, 1.0), [1.0, 0.85, 0.8]),
        PlantKind::Cypress => ("scale", Color::srgb(0.75, 0.85, 0.75), [0.9, 0.8, 0.75]),
        // Инжир — светлая сочная листва, гладкая серая кора.
        PlantKind::Fig => ("broadleaf", Color::srgb(1.05, 1.12, 0.85), [1.25, 1.25, 1.25]),
        PlantKind::Shrub => ("broadleaf", Color::srgb(1.05, 1.05, 0.85), [0.9, 0.85, 0.8]),
        PlantKind::Rock => ("broadleaf", Color::WHITE, [1.0, 1.0, 1.0]),
    }
}

fn foliage_material(base: StandardMaterial, kind: WindKind) -> FoliageMaterial {
    let (s, flutter, flex) = wind_of(kind);
    ExtendedMaterial { base, extension: FoliageExt { wind: Vec4::new(0.0, s, flutter, 0.0), dir: Vec4::new(0.8, 0.45, flex, 0.0) } }
}

fn mesh_named(gltf: &Gltf, gltf_meshes: &Assets<GltfMesh>, name: &str) -> Option<Handle<Mesh>> {
    gltf.named_meshes.get(name).and_then(|h| gltf_meshes.get(h)).and_then(|m| m.primitives.first()).map(|p| p.mesh.clone())
}

/// Габариты меша (min, max) по позициям вершин.
/// Наибольший размер скана камня (м), как он лежит в `models/rocks/<id>.glb`.
fn rock_scan_size(id: &str) -> f32 {
    match id {
        "rock_07" => 0.32,
        "rock_09" => 0.14,
        "stone_01" => 0.15,
        "namaqualand_boulder_02" => 2.52,
        "rock_face_01" => 4.95,
        _ => 1.0,
    }
}

fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for p in pos {
        min = min.min(Vec3::from(*p));
        max = max.max(Vec3::from(*p));
    }
    Some((min, max))
}

#[allow(clippy::too_many_arguments)]
fn spawn_plants(
    mut commands: Commands,
    pending: Option<Res<PendingPlants>>,
    assets: Option<Res<VegetationAssets>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<bevy::gltf::GltfMaterial>>,
    meshes: Res<Assets<Mesh>>,
    mut foliage: ResMut<Assets<FoliageMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut wind: Option<ResMut<WindMaterials>>,
) {
    let (Some(pending), Some(assets)) = (pending, assets) else { return };
    let failed = |h: &Handle<Gltf>| matches!(server.get_load_state(h), Some(bevy::asset::LoadState::Failed(_)));
    let all = assets.trees.iter().map(|t| &t.2).chain(assets.rocks.iter().map(|r| &r.1));
    if all.clone().any(|h| !server.is_loaded_with_dependencies(h) && !failed(h)) {
        return;
    }
    let Some(wind) = wind.as_mut() else { return };
    let started = std::time::Instant::now();

    // Материалы по видам.
    let mut bark_mats: HashMap<PlantKind, Handle<FoliageMaterial>> = HashMap::new();
    let mut leaf_mats: HashMap<PlantKind, Handle<FoliageMaterial>> = HashMap::new();
    for &(kind, _, _) in TREES {
        let (atlas, tint, bark_tint) = look(kind);
        let bark = foliage.add(foliage_material(
            StandardMaterial {
                base_color: Color::linear_rgb(BARK_TINT[0] * bark_tint[0], BARK_TINT[1] * bark_tint[1], BARK_TINT[2] * bark_tint[2]),
                base_color_texture: Some(assets.bark.0.clone()),
                normal_map_texture: Some(assets.bark.1.clone()),
                perceptual_roughness: 0.92,
                reflectance: 0.3,
                ..default()
            },
            WindKind::Bark,
        ));
        let leaf = foliage.add(foliage_material(
            StandardMaterial {
                base_color: tint,
                base_color_texture: assets.atlases.get(atlas).cloned(),
                alpha_mode: AlphaMode::Mask(0.45),
                double_sided: true,
                cull_mode: None,
                perceptual_roughness: 0.9,
                reflectance: 0.12,
                // Свет сквозь листву (контровое солнце просвечивает крону). Больше не ставим:
                // в Bevy пропускание отнимает столько же у отражённого света и крона темнеет.
                diffuse_transmission: 0.15,
                ..default()
            },
            WindKind::Leaves,
        ));
        wind.0.push((bark.clone(), WindKind::Bark));
        wind.0.push((leaf.clone(), WindKind::Leaves));
        bark_mats.insert(kind, bark);
        leaf_mats.insert(kind, leaf);
    }

    // Модели деревьев.
    let mut models: HashMap<PlantKind, Vec<TreeModel>> = HashMap::new();
    for (kind, id, h) in &assets.trees {
        let Some(gltf) = gltfs.get(h) else {
            warn!("нет модели дерева {id}");
            continue;
        };
        let m = |suffix: &str| mesh_named(gltf, &gltf_meshes, &format!("{id}_{suffix}"));
        let (Some(b0), Some(l0), Some(b1), Some(l1), Some(imp)) = (m("lod0_bark"), m("lod0_leaf"), m("lod1_bark"), m("lod1_leaf"), m("imp")) else {
            warn!("в модели {id} не все LOD");
            continue;
        };
        let height = [&b0, &l0].iter().filter_map(|h| meshes.get(*h)).filter_map(mesh_bounds).map(|(_, max)| max.y).fold(0.5f32, f32::max);
        let (_, tint, _) = look(*kind);
        let imp_mat = foliage.add(foliage_material(
            StandardMaterial {
                base_color: tint,
                base_color_texture: assets.impostors.get(id).cloned(),
                alpha_mode: AlphaMode::Mask(0.45),
                double_sided: true,
                cull_mode: None,
                perceptual_roughness: 0.9,
                reflectance: 0.2,
                ..default()
            },
            WindKind::Leaves,
        ));
        wind.0.push((imp_mat.clone(), WindKind::Leaves));
        models.entry(*kind).or_default().push(TreeModel {
            kind: *kind,
            height,
            lod0: (b0, l0),
            lod1: (b1, l1),
            imp,
            bark: bark_mats[kind].clone(),
            leaf: leaf_mats[kind].clone(),
            imp_mat,
        });
    }

    // Камни: материал из скана, чуть высветленный под местный известняк.
    let mut rocks: Vec<RockModel> = Vec::new();
    for (id, h) in &assets.rocks {
        let Some(gltf) = gltfs.get(h) else { continue };
        let (Some(l0), Some(l1)) = (gltf.named_meshes.get(format!("{id}_lod0").as_str()), gltf.named_meshes.get(format!("{id}_lod1").as_str())) else {
            warn!("в скане {id} нет LOD");
            continue;
        };
        let (Some(g0), Some(g1)) = (gltf_meshes.get(l0), gltf_meshes.get(l1)) else { continue };
        let (Some(p0), Some(p1)) = (g0.primitives.first(), g1.primitives.first()) else { continue };
        let mut sm = p0.material.as_ref().and_then(|m| gltf_materials.get(m)).map(bevy::pbr::gltf::standard_material_from_gltf_material).unwrap_or_default();
        sm.base_color = Color::srgb(0.92, 0.96, 1.0);
        // Сетка в главном мире бывает уже выгружена (только для рендера) — тогда берём размер
        // скана из таблицы, иначе камни выходят то с гальку, то со скалу.
        let size = meshes.get(&p0.mesh).and_then(mesh_bounds).map(|(a, b)| (b - a).max_element()).filter(|s| *s > 0.0).unwrap_or_else(|| rock_scan_size(id));
        debug!("скан камня {id}: {size:.2} м");
        rocks.push(RockModel { size, lod0: p0.mesh.clone(), lod1: p1.mesh.clone(), mat: standard.add(sm) });
    }

    let mut counts: HashMap<PlantKind, usize> = HashMap::new();
    for plant in &pending.0 {
        *counts.entry(plant.kind).or_default() += 1;
        if plant.kind == PlantKind::Rock {
            let Some(r) = rocks.get(plant.variant as usize % rocks.len().max(1)) else { continue };
            let s = plant.height / r.size;
            let t = Transform::from_translation(plant.pos).with_rotation(Quat::from_rotation_y(plant.yaw)).with_scale(Vec3::new(s, s * 0.8, s));
            let far = (120.0 + plant.height * 150.0).min(500.0);
            commands.spawn((Mesh3d(r.lod0.clone()), MeshMaterial3d(r.mat.clone()), t, VisibilityRange { start_margin: 0.0..0.0, end_margin: 40.0..50.0, use_aabb: false }));
            commands.spawn((Mesh3d(r.lod1.clone()), MeshMaterial3d(r.mat.clone()), t, VisibilityRange { start_margin: 40.0..50.0, end_margin: far..far + 20.0, use_aabb: false }));
            continue;
        }
        let Some(list) = models.get(&plant.kind).filter(|l| !l.is_empty()) else { continue };
        let m = &list[plant.variant as usize % list.len()];
        let s = plant.height / m.height;
        let t = Transform::from_translation(plant.pos).with_rotation(Quat::from_rotation_y(plant.yaw)).with_scale(Vec3::splat(s));
        // Дальности LOD растут с размером растения: кусты переходят на импостор раньше деревьев.
        let k = (plant.height / 8.0).clamp(0.35, 1.5);
        let (d0, d1, d2) = (40.0 * k, 120.0 * k, 900.0 * k.min(1.0));
        let near = VisibilityRange { start_margin: 0.0..0.0, end_margin: d0..d0 + 8.0, use_aabb: false };
        let mid = VisibilityRange { start_margin: d0..d0 + 8.0, end_margin: d1..d1 + 15.0, use_aabb: false };
        let far = VisibilityRange { start_margin: d1..d1 + 15.0, end_margin: d2..d2 + 30.0, use_aabb: false };
        commands.spawn((Mesh3d(m.lod0.0.clone()), MeshMaterial3d(m.bark.clone()), t, near.clone()));
        commands.spawn((Mesh3d(m.lod0.1.clone()), MeshMaterial3d(m.leaf.clone()), t, near));
        commands.spawn((Mesh3d(m.lod1.0.clone()), MeshMaterial3d(m.bark.clone()), t, mid.clone()));
        let mut leaf = commands.spawn((Mesh3d(m.lod1.1.clone()), MeshMaterial3d(m.leaf.clone()), t, mid));
        if m.kind == PlantKind::Shrub {
            leaf.insert(NotShadowCaster);
        }
        if m.kind == PlantKind::Shrub {
            // Кусты часто видны сверху (с холма, со стены), а импостер — плоский крест и сверху
            // превращается в тёмную кляксу. Вдали у куста остаётся облегчённая листва (~450 треугольников).
            commands.spawn((Mesh3d(m.lod1.1.clone()), MeshMaterial3d(m.leaf.clone()), t, far, NotShadowCaster));
        } else {
            commands.spawn((Mesh3d(m.imp.clone()), MeshMaterial3d(m.imp_mat.clone()), t, far, NotShadowCaster));
        }
    }
    info!("Растительность: {:?} за {:.2} с", counts, started.elapsed().as_secs_f32());
    commands.remove_resource::<PendingPlants>();
}

/// Сила и направление ветра в материалах листвы — только когда погода заметно изменилась
/// (или появились новые материалы). Время кадра шейдер берёт сам из `globals`: правка
/// материала заставляет Bevy заново готовить к отрисовке все меши с ним (~12 тыс.), и
/// раньше это случалось каждый кадр.
fn update_wind(clock: Option<Res<crate::world_clock::WorldClock>>, wind: Option<Res<WindMaterials>>, mut mats: ResMut<Assets<FoliageMaterial>>, mut shown: Local<Option<(f32, Vec2)>>) {
    let Some(wind) = wind else { return };
    // Сила и направление ветра — по погоде (в тихую погоду листва едва шевелится,
    // в ненастье деревья гнёт и треплет).
    let w = clock.map(|c| crate::world_clock::weather_at(crate::graphics::frozen_time().unwrap_or(c.game)).wind).unwrap_or(Vec2::new(3.0, 1.7));
    let speed = w.length();
    let gain = (0.35 + 0.16 * speed).clamp(0.3, 2.2);
    let dir = if speed > 0.1 { w / speed } else { Vec2::new(0.87, 0.49) };
    // Пороги грубые нарочно: ветер плавно меняется за игровые часы, а скачок силы на 5 % или
    // направления на ~4° глазу не виден — зато правка материалов случается раз в десятки секунд.
    let same = shown.is_some_and(|(g, d)| (g - gain).abs() < 0.05 && d.distance(dir) < 0.08);
    if same && !wind.is_changed() {
        return;
    }
    *shown = Some((gain, dir));
    for (h, kind) in &wind.0 {
        if let Some(mut m) = mats.get_mut(h) {
            m.extension.wind.y = wind_of(*kind).0 * gain;
            m.extension.dir.x = dir.x;
            m.extension.dir.y = dir.y;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Трава

/// Сторона участка травы, м.
const GRASS_CHUNK: f32 = 16.0;
/// До какого расстояния от камеры растёт трава.
const GRASS_RADIUS: f32 = 72.0;
/// Ближе этого — густая трава, дальше — редкая.
const GRASS_DENSE: f32 = 30.0;

#[derive(Resource, Default)]
struct Grass {
    chunks: HashMap<(i32, i32), (Entity, bool)>,
    material: Option<Handle<FoliageMaterial>>,
    next: f32,
}

fn grass_density() -> f32 {
    static D: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var("MRPG_GRASS").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0f32).clamp(0.0, 3.0))
}

#[allow(clippy::too_many_arguments)]
fn update_grass(
    mut commands: Commands,
    time: Res<Time>,
    mut grass: ResMut<Grass>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    colliders: Option<Res<crate::collision::LevelColliders>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut foliage: ResMut<Assets<FoliageMaterial>>,
    mut wind: Option<ResMut<WindMaterials>>,
) {
    if grass_density() <= 0.0 || crate::level::active() != crate::level::LevelKind::Chersonesus {
        return;
    }
    let now = time.elapsed_secs();
    if now < grass.next {
        return;
    }
    grass.next = now + 0.2;
    let (Some(cam), Some(colliders)) = (cameras.iter().next(), colliders) else { return };
    let Some(wind) = wind.as_mut() else { return };
    let material = grass
        .material
        .get_or_insert_with(|| {
            let h = foliage.add(foliage_material(
                StandardMaterial { base_color: Color::WHITE, double_sided: true, cull_mode: None, perceptual_roughness: 0.95, reflectance: 0.08, diffuse_transmission: 0.3, ..default() },
                WindKind::Grass,
            ));
            wind.0.push((h.clone(), WindKind::Grass));
            h
        })
        .clone();
    let c = cam.translation();
    let (cx, cz) = ((c.x / GRASS_CHUNK).floor() as i32, (c.z / GRASS_CHUNK).floor() as i32);
    let r = (GRASS_RADIUS / GRASS_CHUNK).ceil() as i32;
    let center = |k: (i32, i32)| Vec2::new((k.0 as f32 + 0.5) * GRASS_CHUNK, (k.1 as f32 + 0.5) * GRASS_CHUNK);
    let cam2 = Vec2::new(c.x, c.z);
    // Убираем дальние участки и те, у которых сменилась густота.
    let keys: Vec<_> = grass.chunks.keys().copied().collect();
    for k in keys {
        let d = center(k).distance(cam2);
        let dense = d < GRASS_DENSE;
        let (e, was_dense) = grass.chunks[&k];
        if d > GRASS_RADIUS + GRASS_CHUNK || (dense != was_dense && (d < GRASS_DENSE - 6.0 || d > GRASS_DENSE + 6.0)) {
            commands.entity(e).despawn();
            grass.chunks.remove(&k);
        }
    }
    // Строим недостающие, ближние — первыми; не больше нескольких за раз.
    let mut todo: Vec<((i32, i32), f32)> = Vec::new();
    for dz in -r..=r {
        for dx in -r..=r {
            let k = (cx + dx, cz + dz);
            let d = center(k).distance(cam2);
            if d <= GRASS_RADIUS && !grass.chunks.contains_key(&k) {
                todo.push((k, d));
            }
        }
    }
    todo.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (k, d) in todo.into_iter().take(3) {
        let dense = d < GRASS_DENSE;
        let mesh = grass_chunk(k, dense, &colliders);
        let e = match mesh {
            Some(m) => commands.spawn((Mesh3d(meshes.add(m)), MeshMaterial3d(material.clone()), Transform::IDENTITY, NotShadowCaster)).id(),
            None => commands.spawn(Transform::IDENTITY).id(),
        };
        grass.chunks.insert(k, (e, dense));
    }
}

fn hash2(x: i32, z: i32, s: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263) ^ s.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0xFFFFFF) as f32 / 16777216.0
}

fn srgb(r: f32, g: f32, b: f32) -> Vec3 {
    let l = Color::srgb(r, g, b).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

/// Можно ли тут расти траве: за стенами, не на дороге, не в воде, не под постройкой.
pub fn grass_allowed(x: f32, z: f32, y: f32, colliders: &crate::collision::LevelColliders) -> bool {
    if crate::level::inside_city(x, z) || crate::level::city_wall_distance(x, z) < 4.0 {
        return false;
    }
    if y < crate::level::WATER_Y + 0.7 || crate::level::road_distance(x, z) < 2.6 || crate::level::bare_ground(x, z) {
        return false;
    }
    // В вырезе рельефа (рудник, шурф, землянка) своя земля может лежать ниже карты высот:
    // трава растёт, только если прямо под ней твердь, иначе висела бы в воздухе (в землянке).
    if colliders.in_ground_hole(x, z) && !colliders.point_solid(Vec3::new(x, y - 0.08, z)) {
        return false;
    }
    !colliders.point_solid(Vec3::new(x, y + 0.06, z)) && !colliders.point_solid(Vec3::new(x, y + 0.6, z))
}

/// Меш травы на участке: пучки тонких изогнутых травинок (ковыль, типчак, полынь) с цветами.
fn grass_chunk(k: (i32, i32), dense: bool, colliders: &crate::collision::LevelColliders) -> Option<Mesh> {
    let density = grass_density();
    let step = if dense { 0.36 } else { 0.8 } / density.sqrt().max(0.2);
    let x0 = k.0 as f32 * GRASS_CHUNK;
    let z0 = k.1 as f32 * GRASS_CHUNK;
    let n = (GRASS_CHUNK / step).ceil() as i32;
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut uv0: Vec<[f32; 2]> = Vec::new();
    let mut uv1: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    // Цвета степи в конце лета: соломенный ковыль, оливковый типчак, сизая полынь, зелень у воды.
    // Тон подобран под фототекстуру земли — иначе трава читается тёмной щетиной на светлом склоне.
    let straw = srgb(0.36, 0.36, 0.20);
    let olive = srgb(0.21, 0.28, 0.12);
    let green = srgb(0.15, 0.24, 0.09);
    let sage = srgb(0.26, 0.31, 0.22);
    let flowers = [srgb(0.90, 0.75, 0.20), srgb(0.88, 0.86, 0.78), srgb(0.52, 0.40, 0.62), srgb(0.75, 0.18, 0.12)];
    for iz in 0..n {
        for ix in 0..n {
            let gx = (x0 / step) as i32 + ix;
            let gz = (z0 / step) as i32 + iz;
            let x = x0 + (ix as f32 + hash2(gx, gz, 1)) * step;
            let z = z0 + (iz as f32 + hash2(gx, gz, 2)) * step;
            // Пятна: где-то густо, где-то проплешины с камнями.
            let patch = crate::level::fbm(x * 0.045, z * 0.045, 91);
            let keep = ((patch - 0.2) * 3.6).clamp(0.0, 1.0);
            if hash2(gx, gz, 3) > keep {
                continue;
            }
            let y = crate::level::ground_y(x, z);
            // Слишком круто — голый склон.
            let slope = (crate::level::ground_y(x + 1.0, z) - y).abs().max((crate::level::ground_y(x, z + 1.0) - y).abs());
            if slope > 0.7 || !grass_allowed(x, z, y, colliders) {
                continue;
            }
            let mix = crate::level::fbm(x * 0.02 + 3.0, z * 0.02, 17);
            let wet = (-(y - crate::level::WATER_Y - 2.0) * 0.3).clamp(0.0, 1.0);
            let base_col = straw.lerp(olive, ((mix - 0.35) * 3.0).clamp(0.0, 1.0)).lerp(sage, ((0.4 - mix) * 4.0).clamp(0.0, 1.0)).lerp(green, wet * 0.7);
            let tall = 0.75 + keep * 0.5;
            let blades = if dense { 9 + (hash2(gx, gz, 4) * 8.0) as i32 } else { 7 + (hash2(gx, gz, 4) * 5.0) as i32 };
            let phase = hash2(gx, gz, 5);
            for b in 0..blades {
                let hb = |s: u32| hash2(gx * 13 + b, gz * 7 - b, s);
                let a = hb(6) * std::f32::consts::TAU;
                let r = hb(7).sqrt() * if dense { 0.26 } else { 0.5 };
                let bx = x + a.cos() * r;
                let bz = z + a.sin() * r;
                let h = (0.16 + hb(8) * hb(8) * 0.5) * tall * if dense { 1.0 } else { 1.15 };
                let w = if dense { 0.014 + hb(9) * 0.02 } else { 0.035 + hb(9) * 0.04 };
                let lean_dir = Vec2::new(a.cos(), a.sin()) * (0.15 + hb(10) * 0.45);
                let face = a + std::f32::consts::FRAC_PI_2 + (hb(11) - 0.5);
                let side = Vec3::new(face.cos(), 0.0, face.sin()) * w * 0.5;
                let tint = 0.8 + hb(12) * 0.35;
                // У корня темно: стебли затеняют друг друга — ковёр выглядит объёмным.
                let bottom = base_col * 0.38 * tint;
                let top = base_col.lerp(straw, hb(13) * 0.35) * (1.0 + hb(13) * 0.15) * tint;
                let start = pos.len() as u32;
                let segs = if dense { 3 } else { 2 };
                for s in 0..=segs {
                    let t = s as f32 / segs as f32;
                    let bend = Vec3::new(lean_dir.x, 0.0, lean_dir.y) * h * t * t;
                    let c = Vec3::new(bx, y - 0.03, bz) + Vec3::Y * h * t + bend;
                    let wd = 1.0 - t * 0.85;
                    for sign in [-1.0f32, 1.0] {
                        pos.push((c + side * sign * wd).to_array());
                        // Нормаль почти вверх — трава освещается мягко, без чёрных граней.
                        let n = (Vec3::Y * 0.85 + Vec3::new(-face.sin(), 0.0, face.cos()) * 0.25 * sign).normalize();
                        nrm.push(n.to_array());
                        // Лист сложен вдоль жилки: одна половина в тени, другая на свету.
                        let cc = bottom.lerp(top, t.powf(0.7)) * if sign < 0.0 { 0.78 } else { 1.1 };
                        col.push([cc.x, cc.y, cc.z, 1.0]);
                        uv0.push([(sign + 1.0) * 0.5, t]);
                        uv1.push([t.powf(1.5) * (h / 0.5).min(1.5), phase]);
                    }
                }
                for s in 0..segs {
                    let i = start + s * 2;
                    idx.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
                }
            }
            // Изредка — цветок (жёлтый девясил, белый тысячелистник, шалфей, мак).
            if dense && hash2(gx, gz, 14) < 0.02 {
                let fc = flowers[(hash2(gx, gz, 15) * 4.0) as usize % 4];
                let h = 0.35 + hash2(gx, gz, 16) * 0.3;
                let top = Vec3::new(x, y + h, z);
                let start = pos.len() as u32;
                for (dx, dz) in [(0.03f32, 0.0f32), (0.0, 0.03)] {
                    let s = pos.len() as u32;
                    for (px, py) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        pos.push((top + Vec3::new(dx * px, 0.03 * py * 0.6, dz * px)).to_array());
                        nrm.push([0.0, 1.0, 0.0]);
                        col.push([fc.x, fc.y, fc.z, 1.0]);
                        uv0.push([0.5, 1.0]);
                        uv1.push([1.0, phase]);
                    }
                    idx.extend_from_slice(&[s, s + 1, s + 2, s, s + 2, s + 3]);
                }
                let _ = start;
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv0)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
            .with_inserted_indices(Indices::U32(idx)),
    )
}

#[cfg(test)]
mod tests {
    /// Координаты мест и горнов для снимков: `cargo test print_places -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_places() {
        for p in crate::level::places() {
            println!("place {} {:?}", p.id, p.pos);
        }
        for f in &crate::level::layout().forges {
            println!("forge {} fire {:?} anvil {:?}", f.name, f.fire, f.anvil);
        }
    }

    use super::*;

    /// Все модели и текстуры растительности на месте.
    #[test]
    fn vegetation_assets_exist() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for &(_, name, n) in TREES {
            for v in 0..n {
                assert!(root.join(format!("models/vegetation/{name}_{v}.glb")).exists(), "нет {name}_{v}.glb");
                assert!(root.join(format!("textures/vegetation/{name}_{v}_imp.png")).exists(), "нет импостора {name}_{v}");
            }
        }
        for r in ROCKS {
            assert!(root.join(format!("models/rocks/{r}.glb")).exists(), "нет камня {r}");
        }
        for a in ["broadleaf", "pine", "scale"] {
            assert!(root.join(format!("textures/vegetation/{a}_atlas.png")).exists());
        }
    }

    /// Деревья заменили прежние «шары» листвы: на уровне есть все виды.
    #[test]
    fn city_has_plants_of_every_kind() {
        let layout = crate::level::layout();
        for kind in [PlantKind::Pine, PlantKind::Oak, PlantKind::Juniper, PlantKind::Cypress, PlantKind::Fig, PlantKind::Shrub, PlantKind::Rock] {
            assert!(layout.plants.iter().any(|p| p.kind == kind), "нет растений {kind:?}");
        }
    }
}
