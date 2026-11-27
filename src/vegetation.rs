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
    (PlantKind::Fir, "fir", 3),
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
    /// x — сила объёмного затенения кроны (тёмный низ и сердцевина, светлая макушка),
    /// y — крона-колонна (0 — шар, 1 — нормали от оси ствола); остальное — запас.
    #[uniform(100)]
    pub shade: Vec4,
}

impl MaterialExtension for FoliageExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/foliage.wesl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/foliage.wesl".into()
    }
    fn fragment_shader() -> ShaderRef {
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
    for a in ["broadleaf", "pine", "scale", "bough", "grass"] {
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
        PlantKind::Pine => ("pine", Color::linear_rgb(1.5, 1.45, 1.2), [0.92, 0.84, 0.84]),
        // Дуб пушистый: листья снизу опушённые — серовато-зелёная крона.
        PlantKind::Oak => ("broadleaf", Color::srgb(0.78, 0.78, 0.6), [0.9, 0.9, 0.9]),
        // Можжевельник высокий — сизый.
        PlantKind::Juniper => ("scale", Color::linear_rgb(1.9, 1.25, 1.2), [1.0, 0.85, 0.8]),
        PlantKind::Cypress => ("scale", Color::linear_rgb(0.86, 0.8, 0.72), [0.9, 0.8, 0.75]),
        // Инжир — светлая сочная листва, гладкая серая кора.
        PlantKind::Fig => ("broadleaf", Color::srgb(1.05, 1.12, 0.85), [1.25, 1.25, 1.25]),
        PlantKind::Shrub => ("broadleaf", Color::srgb(0.84, 0.76, 0.52), [0.9, 0.85, 0.8]),
        // Хвоя в бору тёмная, с сизым отливом; затенение кроны — в цвете вершин модели.
        PlantKind::Fir => ("bough", Color::srgb(0.72, 0.73, 0.77), [0.74, 0.72, 0.74]),
        PlantKind::Rock => ("broadleaf", Color::WHITE, [1.0, 1.0, 1.0]),
    }
}

/// Скругление нормалей листвы по кроне: (доля, высота середины кроны в единицах модели —
/// см. высоты пород в tools/vegetation/blender_trees.py). У конической сосны нормали уже
/// такие в самой модели.
fn crown_of(kind: PlantKind) -> Option<(f32, f32)> {
    match kind {
        PlantKind::Pine => Some((0.85, 8.0)),
        PlantKind::Oak => Some((0.85, 4.9)),
        PlantKind::Juniper => Some((0.8, 2.2)),
        PlantKind::Cypress => Some((0.7, 4.6)),
        PlantKind::Fig => Some((0.85, 2.8)),
        PlantKind::Shrub => Some((0.8, 0.8)),
        PlantKind::Fir | PlantKind::Rock => None,
    }
}

/// Сила объёмного затенения кроны (`FoliageExt::shade.x`). Шейдер считает крону шаром вокруг её
/// середины; у узкой колонны кипариса шара нет — с полной силой нижняя половина уходила в черноту.
fn shade_of(kind: PlantKind) -> f32 {
    match kind {
        PlantKind::Cypress => 0.45,
        PlantKind::Fir | PlantKind::Rock => 0.0,
        _ => 1.0,
    }
}

/// Насколько крона — колонна, а не шар (`FoliageExt::shade.y`): у кипариса нормали «от середины
/// кроны» ниже середины смотрели в землю, и вся нижняя половина дерева стояла без солнца —
/// чёрной, с резкой границей посередине. У колонны нормали идут от оси ствола.
fn column_of(kind: PlantKind) -> f32 {
    match kind {
        PlantKind::Cypress => 0.88,
        PlantKind::Juniper => 0.4,
        _ => 0.0,
    }
}

/// Добавка рассеянного света в кроне (`FoliageExt::dir.w`): свет неба проходит сквозь хвою и
/// листья, и теневой бок дерева тёмно-зелёный, а не чёрный. У тёмных хвойных добавка больше.
fn fill_of(kind: PlantKind) -> f32 {
    match kind {
        PlantKind::Fir => 1.6,
        PlantKind::Pine | PlantKind::Juniper | PlantKind::Cypress => 1.2,
        _ => 0.8,
    }
}

fn foliage_material(base: StandardMaterial, kind: WindKind) -> FoliageMaterial {
    let (s, flutter, flex) = wind_of(kind);
    ExtendedMaterial { base, extension: FoliageExt { wind: Vec4::new(0.0, s, flutter, 0.0), dir: Vec4::new(0.8, 0.45, flex, 0.0), shade: Vec4::ZERO } }
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

/// Стволы в моделях гранёные (нормали по граням восьмиугольника): тонкому стволу это не вредит,
/// а толстый ствол ели вблизи выходил тёсаным брусом. Усредняем нормали вершин, стоящих в одной
/// точке, — ствол круглый.
fn smooth_bark(mesh: &mut Mesh) {
    let (Some(pos), Some(nrm)) = (mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).map(<[_]>::to_vec), mesh.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()).map(<[_]>::to_vec)) else { return };
    let key = |p: &[f32; 3]| [(p[0] * 2000.0).round() as i32, (p[1] * 2000.0).round() as i32, (p[2] * 2000.0).round() as i32];
    let mut sum: HashMap<[i32; 3], Vec3> = HashMap::new();
    for (p, n) in pos.iter().zip(&nrm) {
        *sum.entry(key(p)).or_default() += Vec3::from(*n);
    }
    let out: Vec<[f32; 3]> = pos.iter().zip(&nrm).map(|(p, n)| sum[&key(p)].try_normalize().unwrap_or(Vec3::from(*n)).to_array()).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, out);
}

/// Во сколько раз притушить импостор, чтобы он совпал с живой кроной: у лиственных запечённая
/// картинка светлее кроны, у хвойных она уже выровнена (fix_impostors.py) — лишнее затемнение
/// превращало дальние можжевельники и кипарисы в чёрные кляксы.
fn imp_gain(kind: PlantKind) -> f32 {
    match kind {
        PlantKind::Fir => 0.75,
        PlantKind::Pine | PlantKind::Juniper | PlantKind::Cypress => 1.0,
        _ => 0.55,
    }
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
    mut meshes: ResMut<Assets<Mesh>>,
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
        let crown = crown_of(kind);
        let mut leaf = foliage_material(
            StandardMaterial {
                base_color: tint,
                base_color_texture: assets.atlases.get(atlas).cloned(),
                alpha_mode: AlphaMode::Mask(0.45),
                // Со скруглёнными нормалями изнанку листа не переворачиваем: нормаль уже
                // смотрит наружу кроны с обеих сторон карточки.
                double_sided: crown.is_none(),
                cull_mode: None,
                perceptual_roughness: 0.9,
                reflectance: 0.12,
                // Свет сквозь листву (контровое солнце просвечивает крону). Больше не ставим:
                // в Bevy пропускание отнимает столько же у отражённого света и крона темнеет.
                diffuse_transmission: 0.15,
                ..default()
            },
            WindKind::Leaves,
        );
        if let Some((round, center)) = crown {
            leaf.extension.wind.x = round;
            leaf.extension.wind.w = center;
            leaf.extension.shade.x = shade_of(kind);
            leaf.extension.shade.y = column_of(kind);
        }
        leaf.extension.dir.w = fill_of(kind);
        let leaf = foliage.add(leaf);
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
        for h in [&b0, &b1] {
            if let Some(mut mesh) = meshes.get_mut(h) {
                smooth_bark(&mut mesh);
            }
        }
        let height = [&b0, &l0].iter().filter_map(|h| meshes.get(*h)).filter_map(mesh_bounds).map(|(_, max)| max.y).fold(0.5f32, f32::max);
        let (_, tint, _) = look(*kind);
        let mut imp_m = foliage_material(
            StandardMaterial {
                // Запечённый импостор светлее живой кроны — вдали деревья выходили бледными «призраками».
                // (У хвойных импостер уже выровнен под затенённую крону — fix_impostors.py.)
                base_color: Color::from((tint.to_linear() * imp_gain(*kind)).with_alpha(1.0)),
                base_color_texture: assets.impostors.get(id).cloned(),
                alpha_mode: AlphaMode::Mask(0.45),
                // Изнанку не переворачиваем: нормали импостора смотрят вверх, и перевёрнутая
                // изнанка смотрела бы в землю — плоскость, видимая с тыла, выходила чёрной.
                double_sided: false,
                cull_mode: None,
                perceptual_roughness: 0.9,
                reflectance: 0.2,
                ..default()
            },
            WindKind::Leaves,
        );
        // Свет на импосторе — как на живой кроне: по «круглым» нормалям от середины кроны.
        if let Some((round, center)) = crown_of(*kind) {
            imp_m.extension.wind.x = round;
            imp_m.extension.wind.w = center;
            imp_m.extension.shade.x = shade_of(*kind);
            imp_m.extension.shade.y = column_of(*kind);
        }
        imp_m.extension.dir.w = fill_of(*kind);
        let imp_mat = foliage.add(imp_m);
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

    // Камни: материал из скана, приглушённый под серый местный известняк (в скане розоватые
    // и рыжие пятна — на сером откосе глыбы выходили «кирпичами»).
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
        sm.base_color = Color::srgb(0.66, 0.8, 0.84);
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
        // Высокая ель стройнее низкой: растёт вверх, а не вширь.
        let slim = if plant.kind == PlantKind::Fir { 1.0 - 0.25 * ((plant.height - 10.0) / 14.0).clamp(0.0, 1.0) } else { 1.0 };
        let t = Transform::from_translation(plant.pos).with_rotation(Quat::from_rotation_y(plant.yaw)).with_scale(Vec3::new(s * slim, s, s * slim));
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
    cameras: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    colliders: Option<Res<crate::collision::LevelColliders>>,
    assets: Option<Res<VegetationAssets>>,
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
    // Трава растёт вокруг той камеры, что сейчас рисует (для снимков это камера `MRPG_CAM`).
    let cam = cameras.iter().find(|(_, c)| c.is_active).or_else(|| cameras.iter().next()).map(|(t, _)| t);
    let (Some(cam), Some(colliders), Some(assets)) = (cam, colliders, assets) else { return };
    let Some(wind) = wind.as_mut() else { return };
    let material = grass
        .material
        .get_or_insert_with(|| {
            let h = foliage.add(foliage_material(
                StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: assets.atlases.get("grass").cloned(),
                    alpha_mode: AlphaMode::Mask(0.42),
                    // Обе стороны карточки светятся одинаково (нормаль вверх): без `double_sided`
                    // Bevy не переворачивает нормаль у изнанки, и половина пучков не чернеет.
                    double_sided: false,
                    cull_mode: None,
                    perceptual_roughness: 0.95,
                    reflectance: 0.08,
                    diffuse_transmission: 0.25,
                    ..default()
                },
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
    // Перемешиваем как следует: при слабом хеше пучки травы ложились рядами.
    let mut h = (x as u32).wrapping_mul(374761393).rotate_left(13) ^ (z as u32).wrapping_mul(668265263) ^ s.wrapping_mul(2246822519).rotate_left(7);
    h = (h ^ (h >> 16)).wrapping_mul(0x85eb_ca6b);
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    (h & 0xFFFFFF) as f32 / 16777216.0
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

/// Меш травы на участке: скрещённые карточки с пучками из атласа (луговой пучок, ковыль,
/// разнотравье, низкая дернина — tools/vegetation/build_grass_atlas.py). На одной карточке
/// десятки тонких травин, поэтому ковёр густой и мелкий, а треугольников на порядок меньше,
/// чем когда каждая травина была своей полоской.
fn grass_chunk(k: (i32, i32), dense: bool, colliders: &crate::collision::LevelColliders) -> Option<Mesh> {
    let density = grass_density();
    let step = if dense { 0.4 } else { 0.85 } / density.sqrt().max(0.2);
    let x0 = k.0 as f32 * GRASS_CHUNK;
    let z0 = k.1 as f32 * GRASS_CHUNK;
    let n = (GRASS_CHUNK / step).ceil() as i32;
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut uv0: Vec<[f32; 2]> = Vec::new();
    let mut uv1: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    // Оттенки (множители к цвету атласа): сочная зелень у воды, оливковый типчак, соломенный
    // ковыль на сухих местах, сизая полынь.
    let green = Vec3::new(0.56, 0.69, 0.5);
    let olive = Vec3::new(0.62, 0.66, 0.46);
    let straw = Vec3::new(0.9, 0.84, 0.5);
    let sage = Vec3::new(0.56, 0.65, 0.6);
    for iz in 0..n {
        for ix in 0..n {
            let gx = (x0 / step) as i32 + ix;
            let gz = (z0 / step) as i32 + iz;
            let x = x0 + (ix as f32 + hash2(gx, gz, 1)) * step;
            let z = z0 + (iz as f32 + hash2(gx, gz, 2)) * step;
            // Пятна: где-то густо, где-то проплешины с камнями.
            let patch = crate::level::fbm(x * 0.045, z * 0.045, 91);
            let keep = ((patch - 0.14) * 5.0).clamp(0.0, 1.0);
            if hash2(gx, gz, 3) > keep {
                continue;
            }
            let y = crate::level::ground_y(x, z);
            // Слишком круто — голый склон.
            let (sx, sz) = (crate::level::ground_y(x + 1.0, z) - y, crate::level::ground_y(x, z + 1.0) - y);
            let slope = sx.abs().max(sz.abs());
            // На откосе трава редеет постепенно: отдельные пучки держатся и на осыпи.
            if slope > 0.8 + 0.75 * hash2(gx, gz, 11) * hash2(gx, gz, 12) || !grass_allowed(x, z, y, colliders) {
                continue;
            }
            let mix = crate::level::fbm(x * 0.02 + 3.0, z * 0.02, 17);
            let dry = ((0.42 - mix) * 4.0).clamp(0.0, 1.0);
            let wet = (-(y - crate::level::WATER_Y - 2.0) * 0.3).clamp(0.0, 1.0);
            let lush = ((mix - 0.5) * 4.0).clamp(0.0, 1.0).max(wet);
            let tint = olive.lerp(green, lush).lerp(straw, dry * 0.7).lerp(sage, ((crate::level::fbm(x * 0.07, z * 0.07, 23) - 0.62) * 6.0).clamp(0.0, 0.6));
            // Какой пучок: под ногами больше всего низкой дернины, над ней — луговые пучки,
            // на сухих местах ковыль, изредка разнотравье с цветами.
            let roll = hash2(gx, gz, 4);
            let (cell, w, h, quads) = if roll < 0.05 + 0.05 * lush {
                (2usize, 0.55 + hash2(gx, gz, 6) * 0.3, 0.5 + hash2(gx, gz, 7) * 0.35, 2)
            } else if roll < 0.12 + 0.3 * dry {
                (1, 0.6 + hash2(gx, gz, 6) * 0.4, 0.5 + hash2(gx, gz, 7) * 0.4, 2)
            } else if roll < 0.5 + 0.1 * lush || keep < 0.7 || slope > 0.6 {
                // На проплешинах и откосах — только стоячие пучки: низкая дернина поодиночке
                // лежит «звёздами».
                (0, 0.55 + hash2(gx, gz, 6) * 0.4, (0.34 + hash2(gx, gz, 7) * 0.36) * (0.8 + 0.4 * keep), 2)
            } else {
                (3, 0.7 + hash2(gx, gz, 6) * 0.45, (0.2 + hash2(gx, gz, 7) * 0.16) * (0.85 + 0.3 * keep), if dense { 3 } else { 2 })
            };
            // Вдали пучков меньше — они крупнее, чтобы ковёр не редел.
            let (w, h) = if dense { (w, h) } else { (w * 1.7, h * 1.2) };
            let (cu, cv) = ((cell % 2) as f32 * 0.5, (cell / 2) as f32 * 0.5);
            let phase = hash2(gx, gz, 5);
            let yaw0 = hash2(gx, gz, 8) * std::f32::consts::TAU;
            let shade = 0.82 + hash2(gx, gz, 9) * 0.36;
                    // Общий тон: приглушённая зелень, без желтизны.
            let c = tint * shade * Vec3::new(0.45, 0.37, 0.67);
            for q in 0..quads {
                let a = yaw0 + q as f32 * std::f32::consts::PI / quads as f32 + (hash2(gx + q, gz - q, 10) - 0.5) * 0.5;
                let dir = Vec3::new(a.cos(), 0.0, a.sin());
                // Карточка слегка завалена назад: сверху ковёр не превращается в палочки.
                // Низкая дернина и дальние пучки завалены сильнее: сверху (со стены, с холма)
                // они закрывают землю, а не торчат «палочками».
                let lean = if cell == 3 { 2.6 } else if dense { 0.5 } else { 1.3 };
                let back = Vec3::new(-a.sin(), 0.0, a.cos()) * (hash2(gx - q, gz + q, 11) - 0.5) * lean * h;
                let flip = hash2(gx + 3 * q, gz, 12) < 0.5;
                // Полосы низкой дернины не сходятся в одной точке: сверху пучок выглядел
                // «звездой» из скрещённых лент, а так они лежат вразброс, как кочки.
                let off = if cell == 3 { Vec3::new(-a.sin(), 0.0, a.cos()) * (hash2(gx + 5 * q, gz - 2 * q, 13) - 0.5) * 0.7 } else { Vec3::ZERO };
                let start = pos.len() as u32;
                for row in 0..3 {
                    let t = row as f32 * 0.5;
                    for (side, u) in [(-0.5f32, 0.0f32), (0.5, 1.0)] {
                        let px = x + off.x + dir.x * side * w + back.x * t * t;
                        let pz = z + off.z + dir.z * side * w + back.z * t * t;
                        // Низ карточки идёт по нарисованной земле, по её треугольникам: посаженный
                        // по гладкой высоте или по наклону в центре пучок на крутом и неровном
                        // тонул краем в ребре треугольника, и земля срезала его прямой линией.
                        let gy = crate::level::mesh_y(px, pz);
                        // Комель пучка на картинке тёмный и плотный: на светлой земле (склон, осыпь,
                        // проплешина) он лежал тёмной полосой с прямым нижним краем — «ямкой» под
                        // каждым пучком. Самый низ прячем в землю, остальной комель высветляем.
                        let root = if cell == 3 { 0.16 } else { 0.11 };
                        pos.push([px, gy - 0.04 + h * (t - root * (1.0 - t)), pz]);
                        // Нормаль вверх — трава освещается мягко, как ковёр, а не как забор.
                        let nn = (Vec3::Y + Vec3::new(-a.sin(), 0.0, a.cos()) * 0.2).normalize();
                        nrm.push(nn.to_array());
                        let lift = 1.0 + (if cell == 3 { 0.75 } else { 0.5 }) * (1.0 - t) * (1.0 - t);
                        col.push([c.x * lift, c.y * lift, c.z * lift, 1.0]);
                        let uu = if flip { 1.0 - u } else { u };
                        uv0.push([cu + 0.006 + uu * 0.488, cv + 0.497 - t * 0.49]);
                        uv1.push([t.powf(1.5) * (h / 0.5).min(1.5), phase]);
                    }
                }
                for row in 0..2 {
                    let i = start + row * 2;
                    idx.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
                }
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
        for a in ["broadleaf", "pine", "scale", "bough", "grass"] {
            assert!(root.join(format!("textures/vegetation/{a}_atlas.png")).exists());
        }
    }

    /// Деревья заменили прежние «шары» листвы: на уровне есть все виды.
    #[test]
    fn city_has_plants_of_every_kind() {
        let layout = crate::level::layout();
        for kind in [PlantKind::Pine, PlantKind::Oak, PlantKind::Juniper, PlantKind::Cypress, PlantKind::Fig, PlantKind::Shrub, PlantKind::Fir, PlantKind::Rock] {
            assert!(layout.plants.iter().any(|p| p.kind == kind), "нет растений {kind:?}");
        }
    }
}
