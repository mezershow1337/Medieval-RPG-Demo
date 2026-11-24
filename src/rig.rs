//! Модели людей и их процедурная анимация.
//!
//! Модели лежат в `assets/models/humans.glb` (их собирает `tools/models/build_humans.py` из
//! базового тела MakeHuman, CC0). Тело и одежда — скинованные меши на скелете из 15 костей
//! (таз, торс, голова, руки, ноги); шлем, бармица и прочее надеваются слоями по снаряжению.
//! Анимация считается кодом: кулаки ложатся на рукоять оружия и ручку щита (IK с поворотом кисти
//! по оси рукояти), ноги шагают по земле, торс поворачивается за ударом, при оглушении
//! откидывается назад, после смерти тело падает навзничь.

use crate::appearance::{Folk, Look, Shape};
use crate::combat::WeaponKind;
use crate::protocol::ActorKind;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh, GltfSkin};
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::pbr::gltf::standard_material_from_gltf_material;
use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

#[path = "rig_dims.rs"]
mod dims;
use dims::{
    ANKLE_HEIGHT, FOREARM, GRIP_AXIS_L, GRIP_AXIS_R, GRIP_L, GRIP_R, REST, SHIN, THIGH, UPPER_ARM,
};

pub const MODEL_PATH: &str = "models/humans.glb";
/// Доспехи игроков сверх тех, что носят враги (поддоспешник, перчатки, наручи, поножи,
/// гребенчатый шлем): собирает `tools/models/build_armor.py` на том же скелете.
pub const ARMOR_PATH: &str = "models/armor.glb";

// ---------------------------------------------------------------------------------------------
// Кости

pub const BONES: usize = 15;

/// Номера костей (индексы в массивах ниже; тот же порядок у суставов скина в glb).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Bone {
    Pelvis,
    Torso,
    Head,
    UpperArmR,
    ForearmR,
    HandR,
    UpperArmL,
    ForearmL,
    HandL,
    ThighR,
    ShinR,
    FootR,
    ThighL,
    ShinL,
    FootL,
}

const PARENT: [Option<usize>; BONES] = [
    None,
    Some(0),
    Some(1),
    Some(1),
    Some(3),
    Some(4),
    Some(1),
    Some(6),
    Some(7),
    Some(0),
    Some(9),
    Some(10),
    Some(0),
    Some(12),
    Some(13),
];

const PELVIS_HEIGHT: f32 = REST[0].y;

// ---------------------------------------------------------------------------------------------
// Снаряжение по слотам (одежда эпохи: Херсонес, 510 г.)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadGear {
    Bare,
    /// Спангенхельм: купол из сегментов на бронзовом каркасе, нащёчники, бармица сзади.
    Spangenhelm,
    /// Позднеримский гребенчатый шлем: две половины купола на гребне, нащёчники, назатыльник.
    Ridge,
    /// Высокий клёпаный конический шлем с наносником и бармицей кругом.
    Conical,
    /// Войлочная шапка кочевника с меховым отворотом.
    FeltCap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tunic {
    /// Ромейская шерстяная туника с клавами и орбикулами.
    Byzantine,
    /// Готская туника с тканой каймой.
    Gothic,
    /// Кафтан кочевника с запахом и меховой оторочкой.
    Kaftan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyArmor {
    None,
    /// Стёганый поддоспешник из многих слоёв льна, простёган ромбами.
    Gambeson,
    /// Кольчуга до середины бедра с короткими рукавами.
    Mail,
    /// Ламеллярный доспех из железных пластин на кожаных шнурах.
    Lamellar,
}

/// Что на руках.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmGear {
    Bare,
    /// Кожаные перчатки с крагами.
    Gloves,
    /// Ламеллярные наручи на предплечьях.
    Vambraces,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegGear {
    Trousers,
    /// Штаны и обмотки-фасции до колена.
    Wrapped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootGear {
    Shoes,
    Boots,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hair {
    Shaved,
    /// Ёжик под машинку.
    Cropped,
    Short,
    Long,
    /// Лысина ото лба до макушки, венчик волос (и тонзура у монахов).
    Balding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beard {
    None,
    /// Отросшая за несколько дней щетина.
    Stubble,
    /// Только усы.
    Moustache,
    Short,
    Full,
}

/// Во что одет персонаж. Поля совпадают со слотами экипировки (голова, торс, ноги, ступни).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outfit {
    pub head: HeadGear,
    pub tunic: Tunic,
    pub armor: BodyArmor,
    pub arms: ArmGear,
    pub legs: LegGear,
    /// Железные поножи поверх обмоток.
    pub greaves: bool,
    pub feet: FootGear,
    pub hair: Hair,
    pub beard: Beard,
    /// Цвет туники (у игроков — свой оттенок, чтобы их различать).
    pub tunic_color: Color,
    /// Цвет штанов.
    pub trousers: Color,
    pub hair_color: Color,
    /// Оттенок кожи (множитель к текстуре).
    pub skin: Color,
    /// Телосложение и черты лица (веса целей формы модели).
    pub shape: Shape,
}

/// Небольшой разброс по номеру персонажа, чтобы одинаковые враги не были близнецами.
fn jitter(id: u64, salt: u64) -> f32 {
    let h = id.wrapping_add(salt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    ((h >> 40) % 1000) as f32 / 1000.0
}

/// Народ бойца (от него облик: кожа, волосы, борода).
pub fn folk_of(kind: ActorKind) -> Folk {
    match kind {
        ActorKind::GothWarrior => Folk::Goths,
        ActorKind::AlanLancer => Folk::Alans,
        ActorKind::HunArcher => Folk::Nomads,
        _ => Folk::Romaioi,
    }
}

impl Outfit {
    pub fn for_actor(kind: ActorKind, id: u64) -> Self {
        let j = |s| jitter(id, s);
        let base = Outfit {
            head: HeadGear::Bare,
            tunic: Tunic::Byzantine,
            armor: BodyArmor::None,
            arms: ArmGear::Bare,
            legs: LegGear::Trousers,
            greaves: false,
            feet: FootGear::Boots,
            hair: Hair::Short,
            beard: Beard::Short,
            tunic_color: Color::WHITE,
            trousers: Color::srgb(0.45, 0.4, 0.34),
            hair_color: Color::WHITE,
            skin: Color::WHITE,
            shape: Shape::default(),
        }
        .with_look(Look::random(id, folk_of(kind)));
        match kind {
            ActorKind::Player => {
                // Некрашеная шерсть с лёгким оттенком цвета игрока.
                let hue = (id.wrapping_mul(2654435761) % 360) as f32;
                Outfit {
                    legs: LegGear::Wrapped,
                    feet: FootGear::Shoes,
                    tunic_color: Color::hsl(hue, 0.3, 0.82),
                    ..base
                }
            }
            ActorKind::Stratiotes => Outfit {
                head: HeadGear::Spangenhelm,
                armor: BodyArmor::Mail,
                tunic_color: Color::srgb(0.75, 0.25, 0.2),
                ..base
            },
            ActorKind::Bucellarius => Outfit {
                head: HeadGear::Conical,
                armor: BodyArmor::Lamellar,
                tunic_color: Color::srgb(0.35, 0.42, 0.6),
                trousers: Color::srgb(0.3, 0.27, 0.24),
                ..base
            },
            ActorKind::GothWarrior => Outfit {
                tunic: Tunic::Gothic,
                legs: LegGear::Wrapped,
                feet: FootGear::Shoes,
                tunic_color: Color::srgb(0.72 + 0.2 * j(4), 0.68 + 0.15 * j(4), 0.55),
                trousers: Color::srgb(0.5, 0.45, 0.36),
                ..base
            },
            ActorKind::AlanLancer => Outfit {
                head: HeadGear::Conical,
                tunic: Tunic::Kaftan,
                armor: BodyArmor::Lamellar,
                tunic_color: Color::srgb(0.6, 0.45, 0.3),
                trousers: Color::srgb(0.35, 0.3, 0.25),
                ..base
            },
            // Горожанина рисуют в одежде того жителя, кто схватился за оружие (`client::outfit_of`).
            ActorKind::Townsman => base,
            ActorKind::HunArcher => Outfit {
                head: HeadGear::FeltCap,
                tunic: Tunic::Kaftan,
                tunic_color: Color::srgb(0.55 + 0.2 * j(4), 0.5 + 0.15 * j(4), 0.42),
                trousers: Color::srgb(0.32, 0.27, 0.22),
                ..base
            },
        }
    }

    /// Причёска, борода, цвет волос и кожи, телосложение и лицо — из облика.
    pub fn with_look(mut self, look: Look) -> Self {
        self.hair = look.hair_style();
        self.beard = look.beard_style();
        self.hair_color = look.hair_tint();
        self.skin = look.skin_tint();
        self.shape = look.shape();
        self
    }

    /// Игрок, одетый в то, что надето у него в инвентаре.
    pub fn with_gear(mut self, gear: &crate::gear_look::Gear) -> Self {
        use crate::gear_look::{ArmPiece, BodyPiece, Helmet};
        if gear.look.set {
            self = self.with_look(gear.look);
        }
        self.head = match gear.helmet() {
            None => HeadGear::Bare,
            Some(Helmet::Spangenhelm) => HeadGear::Spangenhelm,
            Some(Helmet::Ridge) => HeadGear::Ridge,
            Some(Helmet::Conical) => HeadGear::Conical,
            Some(Helmet::Cap) => HeadGear::FeltCap,
        };
        self.armor = match gear.body() {
            None => BodyArmor::None,
            Some(BodyPiece::Gambeson) => BodyArmor::Gambeson,
            Some(BodyPiece::Mail) => BodyArmor::Mail,
            Some(BodyPiece::Lamellar) => BodyArmor::Lamellar,
        };
        self.arms = match gear.arms() {
            None => ArmGear::Bare,
            Some(ArmPiece::Gloves) => ArmGear::Gloves,
            Some(ArmPiece::Vambraces) => ArmGear::Vambraces,
        };
        self.greaves = gear.greaves();
        self.feet = if gear.boots() {
            FootGear::Boots
        } else {
            FootGear::Shoes
        };
        // Высокие сапоги обуваются на штаны, обмотки под ними не наматывают.
        if self.feet == FootGear::Boots {
            self.legs = LegGear::Trousers;
        }
        self
    }

    /// Детали glb (скинованные на весь скелет), из которых собран персонаж.
    pub fn layers(&self) -> Vec<&'static str> {
        let mut l = vec!["body.head", "body.hands", "eyes", "brows", "belt"];
        l.push(match self.tunic {
            Tunic::Byzantine => "tunic.byz",
            Tunic::Gothic => "tunic.goth",
            Tunic::Kaftan => "tunic.kaftan",
        });
        match self.armor {
            BodyArmor::None => {}
            BodyArmor::Gambeson => l.push("gambeson"),
            BodyArmor::Mail => l.push("mail"),
            BodyArmor::Lamellar => l.push("lamellar"),
        }
        l.push("trousers");
        if self.legs == LegGear::Wrapped {
            l.push("wraps");
        }
        l.push(match self.feet {
            FootGear::Shoes => "shoes",
            FootGear::Boots => "boots",
        });
        if self.greaves {
            l.push("greaves");
        }
        match self.arms {
            ArmGear::Bare => {}
            ArmGear::Gloves => l.push("gloves"),
            ArmGear::Vambraces => l.push("vambraces"),
        }
        // Под шлемом и шапкой волос не видно.
        if self.head == HeadGear::Bare {
            match self.hair {
                Hair::Shaved => {}
                Hair::Cropped => l.push("hair.cropped"),
                Hair::Short => l.push("hair.short"),
                Hair::Long => l.push("hair.long"),
                Hair::Balding => l.push("hair.balding"),
            }
        }
        match self.beard {
            Beard::None => {}
            Beard::Stubble => l.push("beard.stubble"),
            Beard::Moustache => l.push("beard.moustache"),
            Beard::Short => l.push("beard.short"),
            Beard::Full => l.push("beard.full"),
        }
        match self.head {
            HeadGear::Bare => {}
            HeadGear::Spangenhelm => l.push("helm.spangen"),
            HeadGear::Ridge => l.push("helm.ridge"),
            HeadGear::Conical => l.push("helm.conical"),
            HeadGear::FeltCap => l.push("cap.felt"),
        }
        l
    }

    /// Цвет, в который красится материал детали (None — как в текстуре).
    fn tint(&self, material: &str) -> Option<Color> {
        let base = material.split('.').next().unwrap_or(material);
        match base {
            "tunic_byz" | "tunic_goth" | "tunic_kaftan" => Some(self.tunic_color),
            "trousers" => Some(self.trousers),
            b if b.starts_with("hair_") || b.starts_with("beard_") => Some(self.hair_color),
            "skin" => Some(self.skin),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Загрузка моделей

#[derive(Resource)]
struct ModelsHandle(Vec<Handle<Gltf>>);

/// Примитив детали: меш, материал, имя материала из glb и имена целей формы меша (морфов).
type Primitive = (Handle<Mesh>, Handle<StandardMaterial>, String, Vec<String>);

#[derive(Resource)]
pub struct CharacterModels {
    parts: HashMap<String, Vec<Primitive>>,
    tinted: HashMap<(String, [u8; 4]), Handle<StandardMaterial>>,
    bindposes: Handle<SkinnedMeshInverseBindposes>,
    pub blade: Handle<StandardMaterial>,
}

/// Клинки разной выделки: грубый и сбитый темнее, в окалине и ржавчине; полированный блестит.
#[derive(Resource, Default)]
pub struct BladeTints(HashMap<(AssetId<StandardMaterial>, u8), Handle<StandardMaterial>>);

impl BladeTints {
    /// Материал клинка для вида `finish` (0..255). Хороший клинок — как в модели.
    pub fn get(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        base: &Handle<StandardMaterial>,
        finish: u8,
    ) -> Handle<StandardMaterial> {
        let bucket = finish / 32;
        if bucket >= 5 {
            return base.clone();
        }
        if let Some(h) = self.0.get(&(base.id(), bucket)) {
            return h.clone();
        }
        let Some(mut m) = materials.get(base).cloned() else {
            return base.clone();
        };
        // 0 — бурая ржа и окалина, 4 — чуть тусклее модели.
        // Дерево и кожа рукояти не ржавеют — только темнеют от грязи.
        let k = (1.0 - bucket as f32 / 5.0) * if m.metallic > 0.25 { 1.0 } else { 0.35 };
        let c = m.base_color.to_srgba();
        let rust = Srgba::new(0.36, 0.25, 0.18, c.alpha);
        m.base_color = Color::Srgba(Srgba::new(
            c.red + (rust.red - c.red) * k,
            c.green + (rust.green - c.green) * k,
            c.blue + (rust.blue - c.blue) * k,
            c.alpha,
        ));
        m.metallic *= 1.0 - k * 0.6;
        m.perceptual_roughness = m.perceptual_roughness + (0.9 - m.perceptual_roughness) * k;
        let h = materials.add(m);
        self.0.insert((base.id(), bucket), h.clone());
        h
    }
}

/// Модель вещи (на земле, на прилавке, в витрине) по выделке: `finish` как у `Item::look` × 255.
/// Когда модель загрузится, её материалы подменяются на ржавые и тусклые.
#[derive(Component, Clone, Copy)]
pub struct LookTint(pub u8);

#[derive(Component)]
struct LookTinted;

fn apply_look_tints(
    mut commands: Commands,
    roots: Query<(Entity, &LookTint), Without<LookTinted>>,
    children: Query<&Children>,
    mut mats: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut tints: ResMut<BladeTints>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root, tint) in &roots {
        let mut found = false;
        for e in children.iter_descendants(root) {
            if let Ok(mut m) = mats.get_mut(e) {
                found = true;
                m.0 = tints.get(&mut materials, &m.0, tint.0);
            }
        }
        // Модель ещё грузится — попробуем в следующем кадре.
        if found {
            commands.entity(root).insert(LookTinted);
        }
    }
}

/// Лезвие оружия: хранит свой обычный материал (на время ответного удара его подменяют).
#[derive(Component, Clone)]
pub struct Blade(pub Handle<StandardMaterial>);

impl CharacterModels {
    /// Создаёт примитивы детали как дочерние сущности. `skin` — суставы для скинованной детали.
    fn spawn_part(
        &mut self,
        parent: &mut ChildSpawnerCommands,
        materials: &mut Assets<StandardMaterial>,
        name: &str,
        outfit: Option<&Outfit>,
        skin: Option<&[Entity]>,
        blade: bool,
    ) {
        let Some(prims) = self.parts.get(name).cloned() else {
            warn!("в {MODEL_PATH} нет детали {name}");
            return;
        };
        for (mesh, mat, mat_name, targets) in prims {
            let mat = match outfit.and_then(|o| o.tint(&mat_name)) {
                Some(c) => self.tint(materials, &mat_name, &mat, c),
                None => mat,
            };
            let mut e = parent.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat.clone()),
                Transform::default(),
            ));
            // Телосложение и черты лица: веса целей формы по облику.
            if !targets.is_empty() {
                let shape = outfit.map(|o| o.shape).unwrap_or_default();
                e.insert(MeshMorphWeights::Value {
                    weights: targets.iter().map(|t| shape.weight(t)).collect(),
                });
            }
            if let Some(joints) = skin {
                e.insert((
                    SkinnedMesh {
                        inverse_bindposes: self.bindposes.clone(),
                        joints: joints.to_vec(),
                    },
                    NoFrustumCulling,
                ));
            }
            if blade {
                e.insert(Blade(mat));
            }
        }
    }

    fn tint(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        name: &str,
        base: &Handle<StandardMaterial>,
        color: Color,
    ) -> Handle<StandardMaterial> {
        let key = (name.to_string(), color.to_srgba().to_u8_array());
        if let Some(h) = self.tinted.get(&key) {
            return h.clone();
        }
        let mut m = materials.get(base).cloned().unwrap_or_default();
        m.base_color = color;
        let h = materials.add(m);
        self.tinted.insert(key, h.clone());
        h
    }
}

fn load_models(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(ModelsHandle(vec![
        server.load(MODEL_PATH),
        server.load(ARMOR_PATH),
    ]));
}

#[allow(clippy::too_many_arguments)]
fn finish_models(
    mut commands: Commands,
    handle: Res<ModelsHandle>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    gltf_skins: Res<Assets<GltfSkin>>,
    meshes: Res<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Файл доспехов необязателен: если его нет (не собран), загружаем только людей.
    let failed = |h: &Handle<Gltf>| {
        matches!(
            server.get_load_state(h),
            Some(bevy::asset::LoadState::Failed(_))
        )
    };
    if handle
        .0
        .iter()
        .any(|h| !server.is_loaded_with_dependencies(h) && !failed(h))
    {
        return;
    }
    let Some(gltf) = gltfs.get(&handle.0[0]) else {
        return;
    };
    let Some(skin) = gltf.skins.first().and_then(|h| gltf_skins.get(h)) else {
        error!("в {MODEL_PATH} нет скелета");
        commands.remove_resource::<ModelsHandle>();
        return;
    };
    let mut by_handle: HashMap<AssetId<GltfMaterial>, (Handle<StandardMaterial>, String)> =
        HashMap::new();
    let fallback = materials.add(Color::srgb(0.6, 0.6, 0.6));
    let mut parts = HashMap::new();
    for gltf in handle.0.iter().filter_map(|h| gltfs.get(h)) {
        for (name, h) in &gltf.named_materials {
            if let Some(m) = gltf_materials.get(h) {
                let mut sm = standard_material_from_gltf_material(m);
                // Без карты окружения чистый металл выглядит почти чёрным — оставляем часть рассеянного света.
                sm.metallic *= 0.6;
                by_handle.insert(h.id(), (materials.add(sm), name.to_string()));
            }
        }
        for (name, h) in &gltf.named_meshes {
            let Some(mesh) = gltf_meshes.get(h) else {
                continue;
            };
            let prims = mesh
                .primitives
                .iter()
                .map(|p| {
                    let (mat, mat_name) = p
                        .material
                        .as_ref()
                        .and_then(|m| by_handle.get(&m.id()).cloned())
                        .unwrap_or((fallback.clone(), String::new()));
                    let targets = meshes
                        .get(&p.mesh)
                        .filter(|m| m.try_has_morph_targets().unwrap_or(true))
                        .and_then(|m| m.try_morph_target_names().ok().flatten())
                        .map(|n| n.to_vec())
                        .unwrap_or_default();
                    (p.mesh.clone(), mat, mat_name, targets)
                })
                .collect();
            parts.insert(name.to_string(), prims);
        }
    }
    let blade = by_handle
        .values()
        .find(|(_, n)| n == "blade")
        .map(|(h, _)| h.clone())
        .unwrap_or(fallback);
    info!("Модели людей загружены: {} деталей", parts.len());
    commands.insert_resource(CharacterModels {
        parts,
        tinted: HashMap::new(),
        bindposes: skin.inverse_bind_matrices.clone(),
        blade,
    });
    commands.remove_resource::<ModelsHandle>();
}

// ---------------------------------------------------------------------------------------------
// Сущности рига

/// Корень скелета. Повесьте на сущность, и когда модели загрузятся, под ней появится тело.
#[derive(Component)]
pub struct NeedsRig(pub Outfit);

/// Готовый скелет: сущности суставов.
#[derive(Component)]
pub struct Rig {
    joints: [Entity; BONES],
}

/// Поза скелета (локальные повороты суставов и положение таза), пишется анимацией каждый кадр.
#[derive(Component, Clone, Copy, Debug)]
pub struct RigPose {
    pub pelvis: Vec3,
    pub rot: [Quat; BONES],
    /// Руки (правая, левая) висят свободно — у плотного телосложения их отводим от боков.
    pub free: [bool; 2],
}

impl Default for RigPose {
    fn default() -> Self {
        Self {
            pelvis: REST[0],
            rot: [Quat::IDENTITY; BONES],
            free: [false; 2],
        }
    }
}

/// Насколько плотное тело (вес цели `heavy`): свободные руки отводятся от боков.
#[derive(Component, Clone, Copy)]
pub struct RigBulk(pub f32);

/// Оружие: под сущностью появятся клинок (с меткой `Blade`) и рукоять.
#[derive(Component)]
pub struct NeedsWeapon(pub WeaponKind);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShieldKind {
    /// Большой овальный щит гарнизона с хризмой.
    Oval,
    /// Круглый готский щит из досок.
    Round,
}

impl ShieldKind {
    pub fn for_actor(kind: ActorKind) -> Self {
        match kind {
            ActorKind::GothWarrior => ShieldKind::Round,
            _ => ShieldKind::Oval,
        }
    }
}

/// Щит.
#[derive(Component)]
pub struct NeedsShield(pub ShieldKind);

/// Рука от первого лица (кости плеча, предплечья и кисти — прямые дети камеры).
/// Второе поле — левая рука (для оружия в двух руках и кулачной стойки).
#[derive(Component)]
pub struct NeedsViewArm(pub Option<Outfit>, pub bool);

#[derive(Component)]
pub struct ViewArm {
    joints: [Entity; 3],
    pub left: bool,
}

/// Имя модели оружия в glb (у кулаков модели нет).
pub fn weapon_mesh(weapon: WeaponKind) -> Option<&'static str> {
    Some(match weapon {
        WeaponKind::Spatha => "spatha",
        WeaponKind::Axe => "axe",
        WeaponKind::Seax => "seax",
        WeaponKind::Spear => "spear",
        WeaponKind::Bow => "bow",
        WeaponKind::Fists => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn attach_rigs(
    mut commands: Commands,
    mut models: ResMut<CharacterModels>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    rigs: Query<(Entity, &NeedsRig)>,
    weapons: Query<(Entity, &NeedsWeapon)>,
    shields: Query<(Entity, &NeedsShield)>,
    arms: Query<(Entity, &NeedsViewArm)>,
    net: Option<Res<crate::client::NetState>>,
) {
    for (e, need) in &rigs {
        let outfit = need.0;
        let mut joints = [Entity::PLACEHOLDER; BONES];
        for bone in 0..BONES {
            let parent = PARENT[bone].map(|p| joints[p]).unwrap_or(e);
            let joint = commands
                .spawn((
                    Transform::from_translation(REST[bone]),
                    Visibility::default(),
                ))
                .id();
            commands.entity(parent).add_child(joint);
            joints[bone] = joint;
        }
        commands.entity(e).with_children(|p| {
            for layer in outfit.layers() {
                models.spawn_part(
                    p,
                    &mut materials,
                    layer,
                    Some(&outfit),
                    Some(&joints),
                    false,
                );
            }
        });
        commands.entity(e).remove::<NeedsRig>().insert((
            Rig { joints },
            RigPose::default(),
            RigBulk(outfit.shape.heavy),
        ));
    }
    for (e, need) in &weapons {
        let Some(name) = weapon_mesh(need.0) else {
            commands.entity(e).remove::<NeedsWeapon>();
            continue;
        };
        commands
            .entity(e)
            .remove::<NeedsWeapon>()
            .with_children(|w| {
                models.spawn_part(
                    w,
                    &mut materials,
                    &format!("{name}.blade"),
                    None,
                    None,
                    true,
                );
                models.spawn_part(
                    w,
                    &mut materials,
                    &format!("{name}.hilt"),
                    None,
                    None,
                    false,
                );
            });
    }
    for (e, need) in &shields {
        let name = match need.0 {
            ShieldKind::Oval => "shield_oval",
            ShieldKind::Round => "shield_round",
        };
        commands
            .entity(e)
            .remove::<NeedsShield>()
            .with_children(|s| {
                models.spawn_part(
                    s,
                    &mut materials,
                    &format!("{name}.body"),
                    None,
                    None,
                    false,
                );
                models.spawn_part(
                    s,
                    &mut materials,
                    &format!("{name}.boss"),
                    None,
                    None,
                    false,
                );
            });
    }
    for (e, need) in &arms {
        let my_id = net.as_ref().and_then(|n| n.my_id).unwrap_or(0);
        let outfit = need
            .0
            .unwrap_or_else(|| Outfit::for_actor(ActorKind::Player, my_id));
        let left = need.1;
        // Настоящие только плечо, предплечье и кисть своей стороны; остальные кости — одна
        // сущность с нулевым масштабом у плеча, всё, что на них висит, схлопывается в точку.
        let mut arm = [Entity::PLACEHOLDER; 3];
        for i in 0..3 {
            let parent = if i == 0 { e } else { arm[i - 1] };
            let joint = commands
                .spawn((Transform::default(), Visibility::default()))
                .id();
            commands.entity(parent).add_child(joint);
            arm[i] = joint;
        }
        let hidden = commands
            .spawn((Transform::from_scale(Vec3::ZERO), Visibility::default()))
            .id();
        commands.entity(arm[0]).add_child(hidden);
        let mut joints = [hidden; BONES];
        let bones = if left {
            [Bone::UpperArmL, Bone::ForearmL, Bone::HandL]
        } else {
            [Bone::UpperArmR, Bone::ForearmR, Bone::HandR]
        };
        for (bone, joint) in bones.into_iter().zip(arm) {
            joints[bone as usize] = joint;
        }
        commands.entity(e).with_children(|p| {
            for layer in outfit.layers() {
                if layer.starts_with("body.hands")
                    || layer.starts_with("tunic")
                    || matches!(
                        layer,
                        "mail" | "lamellar" | "gambeson" | "gloves" | "vambraces"
                    )
                {
                    models.spawn_part(
                        p,
                        &mut materials,
                        layer,
                        Some(&outfit),
                        Some(&joints),
                        false,
                    );
                }
            }
        });
        commands
            .entity(e)
            .remove::<NeedsViewArm>()
            .insert(ViewArm { joints: arm, left });
    }
}

fn apply_poses(
    rigs: Query<(&Rig, &RigPose, Option<&RigBulk>), Changed<RigPose>>,
    mut joints: Query<&mut Transform>,
) {
    for (rig, pose, bulk) in &rigs {
        let bulk = bulk.map_or(0.0, |b| b.0);
        for bone in 0..BONES {
            if let Ok(mut t) = joints.get_mut(rig.joints[bone]) {
                t.translation = if bone == 0 { pose.pelvis } else { REST[bone] };
                t.rotation = pose.rot[bone];
                // Плотному телу свободные руки отводим в стороны, чтобы не уходили в бока.
                let side = match bone {
                    3 if pose.free[0] => 1.0,
                    6 if pose.free[1] => -1.0,
                    _ => 0.0,
                };
                if side != 0.0 && bulk > 0.0 {
                    t.rotation = Quat::from_rotation_z(0.13 * bulk * side) * t.rotation;
                }
            }
        }
    }
}

impl Rig {
    /// Сущность сустава: к ней можно прицепить вещь (ножны к тазу, крест к груди).
    pub fn joint(&self, bone: Bone) -> Entity {
        self.joints[bone as usize]
    }
}

impl ViewArm {
    pub fn joints(&self) -> [Entity; 3] {
        self.joints
    }
}

/// Рука от первого лица: локальные (положение, поворот) плеча, локтя и кисти для цепочки
/// суставов под корнем руки (в пространстве камеры). `axis` — куда смотрит клинок.
/// `left` — левая рука (плечо и локоть зеркально).
pub fn view_arm_pose(shoulder: Vec3, grip: Vec3, axis: Vec3, left: bool) -> [(Vec3, Quat); 3] {
    // Плечо «подтягиваем» к рукояти: от первого лица его не видно, а оружие всегда в руке.
    let reach = UPPER_ARM + FOREARM + GRIP_R.length() - 0.03;
    let to = grip - shoulder;
    let shoulder = if to.length() > reach {
        grip - to.normalize() * reach
    } else {
        shoulder
    };
    let (pole, side) = if left {
        (Vec3::new(-1.0, -1.0, 0.6), 1)
    } else {
        (Vec3::new(1.0, -1.0, 0.6), 0)
    };
    let arm = solve_arm(shoulder, Some((grip, axis)), pole, Vec3::NEG_Z, side);
    let q_up = arm.rot[0];
    let q_fore = arm.rot[1];
    let q_hand = arm.rot[2];
    [
        (shoulder, q_up),
        (
            q_up.inverse() * (arm.elbow - shoulder),
            q_up.inverse() * q_fore,
        ),
        (
            q_fore.inverse() * (arm.wrist - arm.elbow),
            q_fore.inverse() * q_hand,
        ),
    ]
}

// ---------------------------------------------------------------------------------------------
// Расчёт позы

/// Что делает персонаж — из этого считается поза.
#[derive(Clone, Copy, Debug, Default)]
pub struct PoseInput {
    /// 0 — стоит, 1 — присел.
    pub crouch: f32,
    /// Фаза шага (радианы) и сила шага 0..1.
    pub walk_phase: f32,
    pub walk: f32,
    /// Наклон взгляда.
    pub pitch: f32,
    /// Поворот торса вокруг вертикали (замах вправо — отрицательный).
    pub twist: f32,
    /// Наклон торса: + назад, - вперёд.
    pub lean: f32,
    /// Время, для дыхания.
    pub time: f32,
    /// Скорость, м/с: шаг или бег (0 — обычный шаг, если `walk` > 0). Фазу шага при этом ведут
    /// по `step_cycle(speed)`.
    pub speed: f32,
    /// Куда тянется правая/левая рука (центр кулака) в пространстве персонажа.
    pub right_grip: Option<Vec3>,
    pub left_grip: Option<Vec3>,
    /// Куда смотрит рукоять в кулаке (в сторону клинка; для щита — вдоль ручки).
    /// Если не задано, кисть продолжает предплечье.
    pub right_axis: Option<Vec3>,
    pub left_axis: Option<Vec3>,
}

/// Результат: поза и куда кулаки дотянулись на самом деле.
pub struct Solved {
    pub pose: RigPose,
    pub right_grip: Vec3,
    pub left_grip: Vec3,
}

/// Поворот кости: её ось -Y смотрит в `dir`, лицевая сторона (-Z) — по `front`.
fn bone_rot(dir: Vec3, front: Vec3, fallback: Vec3) -> Quat {
    let y = -dir.normalize_or(Vec3::NEG_Y);
    let mut f = front - dir * front.dot(dir);
    if f.length_squared() < 1e-4 {
        f = fallback - dir * fallback.dot(dir);
    }
    let z = -f.normalize_or(Vec3::Z);
    let x = y.cross(z).normalize_or(Vec3::X);
    let z = x.cross(y);
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

/// Двухзвенная IK: возвращает (средний сустав, конец).
pub fn two_bone(root: Vec3, target: Vec3, a: f32, b: f32, pole: Vec3) -> (Vec3, Vec3) {
    let to = target - root;
    let d = to.length().clamp((a - b).abs() + 1e-3, a + b - 1e-3);
    let dir = to.normalize_or(Vec3::NEG_Y);
    let end = root + dir * d;
    let cos_a = ((a * a + d * d - b * b) / (2.0 * a * d)).clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    let perp = (pole - dir * pole.dot(dir)).normalize_or(dir.any_orthonormal_vector());
    (root + dir * (a * cos_a) + perp * (a * sin_a), end)
}

/// Поворот кулака: отверстие кулака (ось `hole` в системе кисти, от указательного к мизинцу)
/// идёт против `axis` (клинок выходит со стороны большого пальца), а пальцы по возможности
/// продолжают предплечье `fore` — так запястье сгибается как можно меньше.
fn fist_rot(axis: Vec3, fore: Vec3, hole: Vec3) -> Quat {
    let e1 = hole.normalize();
    let e2 = (Vec3::NEG_Y - e1 * Vec3::NEG_Y.dot(e1)).normalize_or(Vec3::NEG_Y);
    let e3 = e1.cross(e2);
    let f1 = -axis.normalize_or(Vec3::Y);
    let mut f2 = fore - f1 * fore.dot(f1);
    if f2.length_squared() < 1e-4 {
        f2 = f1.any_orthonormal_vector();
    }
    let f2 = f2.normalize();
    let f3 = f1.cross(f2);
    let world = Mat3::from_cols(f1, f2, f3);
    let local = Mat3::from_cols(e1, e2, e3);
    Quat::from_mat3(&(world * local.transpose())).normalize()
}

/// Рука целиком: где локоть и запястье, и мировые повороты плеча, предплечья и кисти.
struct Arm {
    elbow: Vec3,
    wrist: Vec3,
    rot: [Quat; 3],
    grip: Vec3,
}

/// IK руки к рукояти. side 0 — правая, 1 — левая. `front` — «перёд» торса для плеча.
fn solve_arm(
    shoulder: Vec3,
    target: Option<(Vec3, Vec3)>,
    pole: Vec3,
    front: Vec3,
    side: usize,
) -> Arm {
    let (grip_c, hole) = if side == 0 {
        (GRIP_R, GRIP_AXIS_R)
    } else {
        (GRIP_L, GRIP_AXIS_L)
    };
    let Some((grip, axis)) = target else {
        unreachable!()
    };
    let mut fore = (grip - shoulder).normalize_or(Vec3::NEG_Y);
    let mut hand = fist_rot(axis, fore, hole);
    let mut joints = (shoulder, shoulder);
    for _ in 0..4 {
        let wrist = grip - hand * grip_c;
        joints = two_bone(shoulder, wrist, UPPER_ARM, FOREARM, pole);
        fore = (joints.1 - joints.0).normalize_or(fore);
        hand = fist_rot(axis, fore, hole);
    }
    let (elbow, wrist) = joints;
    let up = (elbow - shoulder).normalize_or(Vec3::NEG_Y);
    let q_up = bone_rot(up, fore, front);
    let q_fore = Quat::from_rotation_arc(up, fore) * q_up;
    Arm {
        elbow,
        wrist,
        rot: [q_up, q_fore, hand],
        grip: wrist + hand * grip_c,
    }
}

/// Походка: где стопа (вперёд-назад, подъём), наклон носка и доля опоры в цикле шага.
/// `u` — доля цикла 0..1 (0 — пятка этой ноги касается земли впереди).
#[derive(Debug)]
struct Step {
    /// Смещение голеностопа вдоль хода (-Z — вперёд) и вверх.
    z: f32,
    lift: f32,
    /// Наклон стопы: + носок вверх (удар пяткой), - носок вниз (толчок).
    pitch: f32,
}

/// Сколько метров пути на один цикл шага (два шага) при скорости `speed` м/с. По нему снаружи
/// ведут фазу: `walk_phase += путь / step_cycle(скорость) · 2π`. Шагом — 1,3 м, бегом шире.
pub fn step_cycle(speed: f32) -> f32 {
    1.3 + 0.32 * (speed - 1.4).max(0.0)
}

/// Насколько это бег, а не шаг (0..1).
fn run_share(speed: f32) -> f32 {
    smooth01((speed - 2.0) / 1.8)
}

/// Доля цикла, когда нога стоит на земле: шагом 0,6 (обе ноги на земле дважды за цикл),
/// бегом 0,32 (между шагами — полёт).
fn stance_share(speed: f32) -> f32 {
    0.6 - 0.28 * run_share(speed)
}

fn gait(u: f32, speed: f32) -> Step {
    let u = u.rem_euclid(1.0);
    let run = run_share(speed);
    let beta = stance_share(speed);
    // За опору тело проходит step_cycle·β м — столько же стопа отъезжает назад под телом, она не
    // скользит по земле. Опора чуть сдвинута назад: пятка ставится ближе, чем носок отталкивается.
    let half = 0.5 * step_cycle(speed) * beta;
    let back = 0.045 * (1.0 - run);
    if u < beta {
        let s = u / beta;
        // Опора: удар пяткой (носок поднят), перекат, отрыв пятки перед толчком. Бегом — на носок.
        let heel_off = smooth01((s - 0.68 + 0.3 * run) / (0.32 - 0.1 * run));
        let strike = 1.0 - smooth01(s / 0.14);
        Step {
            z: -half + 2.0 * half * s + back,
            lift: 0.09 * heel_off * heel_off + 0.018 * strike * (1.0 - run),
            pitch: 0.22 * strike * (1.0 - run) - 0.55 * heel_off,
        }
    } else {
        let t = (u - beta) / (1.0 - beta);
        // Перенос: стопа идёт вперёд по плавной кривой, носок от толчка поднимается к удару пяткой.
        // Бегом пятка подлетает высоко назад.
        let e = smooth01(t);
        // К концу переноса пятка опускается к высоте удара (носок ещё поднят).
        let lift = (t * PI).sin() * (0.085 + 0.16 * run)
            + 0.09 * (1.0 - smooth01(t / 0.3))
            + 0.018 * (1.0 - run) * smooth01((t - 0.6) / 0.4);
        let kick = 0.12 * run * (t * PI).sin() * (1.0 - t);
        Step {
            z: half - 2.0 * half * e + back + kick,
            lift,
            pitch: -0.55 + (0.77 - 0.22 * run) * smooth01((t - 0.1) / 0.9),
        }
    }
}

fn smooth01(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub fn solve_pose(inp: &PoseInput) -> Solved {
    let mut world = [Quat::IDENTITY; BONES];
    let mut local = [Quat::IDENTITY; BONES];
    let walk = inp.walk.clamp(0.0, 1.0);
    let idle = 1.0 - walk;
    // Скорость: если не задана — обычный шаг горожанина.
    let speed = if inp.speed > 0.0 { inp.speed } else { 1.3 };
    let run = run_share(speed) * walk;
    let crouch = inp.crouch.clamp(0.0, 1.0);
    let t = inp.time;
    // Доля цикла шага: правая пятка касается земли при u = 0, левая — при u = 0,5.
    let u = (inp.walk_phase / TAU).rem_euclid(1.0);
    let cyc = TAU * u;

    // Таз. Шагом: выше всего над опорной ногой, ниже всего, когда обе на земле; бегом наоборот —
    // ниже всего, когда нога пружинит в опоре, выше всего в полёте. Таз смещается к опорной ноге,
    // бедро ноги в переносе опускается, таз поворачивается за шагающей ногой.
    // Стоя: человек не замирает — медленно переносит вес с ноги на ногу.
    let stance_mid = 0.5 * stance_share(speed);
    let twice = (2.0 * (cyc - TAU * stance_mid)).cos();
    let bob = walk * (0.035 * (1.0 - run) * (0.5 - 0.5 * twice) + 0.05 * run * (0.5 + 0.5 * twice))
        + 0.05 * run;
    let sway_walk = (cyc - TAU * stance_mid).cos();
    let shift = (t * 0.31).sin() * 0.7 + (t * 0.53 + 1.3).sin() * 0.3;
    let pelvis_x =
        0.028 * walk * (1.0 - 0.5 * run) * sway_walk + 0.022 * idle * shift * (1.0 - crouch);
    let pelvis = Vec3::new(
        pelvis_x,
        PELVIS_HEIGHT - 0.5 * crouch - bob - 0.008 * idle * shift.abs(),
        REST[0].z + 0.08 * crouch,
    );
    let hip_roll = 0.045 * walk * sway_walk + 0.04 * idle * shift * (1.0 - crouch);
    let hip_yaw = (0.1 - 0.03 * run) * walk * cyc.cos();
    local[0] = Quat::from_rotation_y(-inp.twist * 0.3 + hip_yaw) * Quat::from_rotation_z(hip_roll);
    world[0] = local[0];

    // Торс: плечи поворачиваются навстречу тазу и держатся ровно, при ходьбе чуть вперёд.
    // Дыхание — грудь поднимается раз в 4 с. Стоя торс едва заметно гуляет.
    let breathe = (t * 1.6).sin() * 0.014;
    // Взгляд вниз/вверх частью уходит в торс: у скелета одна кость на шею и голову, и при сильном
    // наклоне одной головы шея рвётся из ворота.
    let look = (inp.pitch * 0.8).clamp(-1.1, 1.0);
    let chest = inp.lean - 0.35 * crouch + 0.3 * look - 0.06 * walk - 0.14 * run;
    let drift = idle * (0.035 * (t * 0.19 + 0.4).sin() + 0.02 * (t * 0.43).sin());
    local[1] = Quat::from_rotation_y(inp.twist * 0.7 - 1.7 * hip_yaw + drift)
        * Quat::from_rotation_z(-0.85 * hip_roll)
        * Quat::from_rotation_x(chest + breathe);
    world[1] = world[0] * local[1];
    // Голова держит взгляд: гасит качание плеч, а стоя изредка поглядывает по сторонам.
    let glance =
        idle * (0.09 * (t * 0.23).sin() + 0.05 * (t * 0.61 + 2.0).sin()) * (1.0 - crouch * 0.5);
    let nod = idle * 0.025 * (t * 0.37 + 0.8).sin();
    let head_pitch = (look - chest - breathe * 0.5 + nod).clamp(-0.5, 0.5);
    local[2] = Quat::from_rotation_y(-inp.twist * 0.6 + 0.7 * hip_yaw - drift + glance)
        * Quat::from_rotation_z(-0.15 * hip_roll)
        * Quat::from_rotation_x(head_pitch);
    world[2] = world[1] * local[2];

    let torso_pos = pelvis + world[0] * REST[1];
    let mut grips = [Vec3::ZERO; 2];
    let mut free = [false; 2];
    // Руки: 0 — правая, 1 — левая.
    for (side, (target, axis)) in [
        (inp.right_grip, inp.right_axis),
        (inp.left_grip, inp.left_axis),
    ]
    .into_iter()
    .enumerate()
    {
        let (ua, fa, hd) = if side == 0 { (3, 4, 5) } else { (6, 7, 8) };
        let sign = if side == 0 { 1.0 } else { -1.0 };
        let shoulder = torso_pos + world[1] * REST[ua];
        match target {
            Some(grip) => {
                let pole = world[1] * Vec3::new(0.7 * sign, -0.8, 0.5);
                // Без оси рукоять считается продолжением предплечья (кулак «смотрит» вперёд).
                let axis = axis.unwrap_or_else(|| {
                    (grip - shoulder)
                        .normalize_or(Vec3::NEG_Z)
                        .cross(world[1] * Vec3::X * sign)
                        .normalize_or(Vec3::Y)
                });
                let arm = solve_arm(
                    shoulder,
                    Some((grip, axis)),
                    pole,
                    world[1] * Vec3::NEG_Z,
                    side,
                );
                world[ua] = arm.rot[0];
                world[fa] = arm.rot[1];
                world[hd] = arm.rot[2];
                grips[side] = arm.grip;
            }
            None => {
                free[side] = true;
                // Свободная рука качается навстречу ноге своей стороны (правая вперёд, когда
                // впереди левая нога), локоть сгибается на махе вперёд; кисть отстаёт от предплечья.
                // Бегом руки согнуты в локтях почти под прямым углом и машут сильнее.
                let swing = -(0.42 + 0.25 * run) * walk * cyc.cos() * sign;
                let rest_hang = 0.06 + 0.03 * (t * 0.7 + side as f32).sin() * idle + 0.15 * run;
                local[ua] = Quat::from_rotation_x(swing + rest_hang)
                    * Quat::from_rotation_z((0.09 + 0.015 * breathe / 0.014) * sign);
                local[fa] = Quat::from_rotation_x(
                    0.22 + 0.12 * walk + 0.5 * swing.max(0.0) * (1.0 - run) + 1.15 * run,
                );
                local[hd] =
                    Quat::from_rotation_x(-0.12 * swing) * Quat::from_rotation_z(0.12 * sign);
                world[ua] = world[1] * local[ua];
                world[fa] = world[ua] * local[fa];
                world[hd] = world[fa] * local[hd];
                let elbow = shoulder + world[ua] * REST[fa];
                let wrist = elbow + world[fa] * REST[hd];
                grips[side] = wrist + world[hd] * if side == 0 { GRIP_R } else { GRIP_L };
            }
        }
    }

    // Ноги: стопы ставим на землю (IK), колени смотрят вперёд.
    for (side, (th, sh, ft)) in [(9, 10, 11), (12, 13, 14)].into_iter().enumerate() {
        let sign = if side == 0 { 1.0 } else { -1.0 };
        let hip = pelvis + world[0] * REST[th];
        let step = gait(u + side as f32 * 0.5, speed);
        // Стоя ноги чуть разные: одна немного впереди, носки врозь.
        let stand_z = if side == 0 { -0.035 } else { 0.03 };
        // При приседе одна нога выставлена вперёд, другая отставлена.
        let crouch_offset = crouch * if side == 0 { -0.18 } else { 0.2 };
        let z = step.z * walk + stand_z * idle + crouch_offset;
        let ankle = Vec3::new(
            (0.11 + 0.01 * idle) * sign,
            ANKLE_HEIGHT + step.lift * walk,
            z,
        );
        let pole = world[0] * Vec3::new(0.15 * sign, 0.0, -1.0);
        let (knee, ankle) = two_bone(hip, ankle, THIGH, SHIN, pole);
        let up = (knee - hip).normalize_or(Vec3::NEG_Y);
        let down = (ankle - knee).normalize_or(Vec3::NEG_Y);
        world[th] = bone_rot(up, -down, world[0] * Vec3::NEG_Z);
        world[sh] = Quat::from_rotation_arc(up, down) * world[th];
        // Стопа: носок чуть наружу, при ходьбе — перекат с пятки на носок.
        world[ft] = Quat::from_rotation_y(-(0.1 + 0.05 * idle) * sign)
            * Quat::from_rotation_x(step.pitch * walk);
    }

    // Мировые повороты -> локальные.
    for bone in 0..BONES {
        if let Some(p) = PARENT[bone] {
            local[bone] = (world[p].inverse() * world[bone]).normalize();
        }
    }
    Solved {
        pose: RigPose {
            pelvis,
            rot: local,
            free,
        },
        right_grip: grips[0],
        left_grip: grips[1],
    }
}

/// Поза тела на спине (для смерти): руки раскинуты, ноги чуть согнуты.
pub fn dead_pose(seed: u64) -> RigPose {
    let s = (seed % 7) as f32 / 7.0;
    let mut rot = [Quat::IDENTITY; BONES];
    rot[2] = Quat::from_rotation_y(0.5 - s) * Quat::from_rotation_x(0.2);
    rot[3] = Quat::from_rotation_z(0.9 + 0.4 * s) * Quat::from_rotation_x(0.2);
    rot[4] = Quat::from_rotation_x(0.5 * s);
    rot[6] = Quat::from_rotation_z(-1.2 + 0.3 * s) * Quat::from_rotation_x(-0.1);
    rot[7] = Quat::from_rotation_x(0.6);
    rot[9] = Quat::from_rotation_x(0.25) * Quat::from_rotation_z(0.1);
    rot[10] = Quat::from_rotation_x(-0.4);
    rot[11] = Quat::from_rotation_x(0.7);
    rot[12] = Quat::from_rotation_x(0.05 * s) * Quat::from_rotation_z(-0.15);
    rot[13] = Quat::from_rotation_x(-0.15);
    rot[14] = Quat::from_rotation_x(0.9);
    RigPose {
        pelvis: REST[0],
        rot,
        free: [false; 2],
    }
}

/// Смешивает две позы (k = 0 — первая, 1 — вторая).
pub fn blend(a: &RigPose, b: &RigPose, k: f32) -> RigPose {
    let mut rot = a.rot;
    for (r, t) in rot.iter_mut().zip(b.rot.iter()) {
        *r = r.slerp(*t, k);
    }
    RigPose {
        pelvis: a.pelvis.lerp(b.pelvis, k),
        rot,
        free: if k < 0.5 { a.free } else { b.free },
    }
}

/// Тело падает навзничь: доля 0..1 -> поворот и подъём корня скелета.
pub fn fall_transform(k: f32) -> Transform {
    // Ускоряющееся падение с небольшим отскоком в конце.
    let e = if k < 0.85 {
        (k / 0.85).powi(2)
    } else {
        1.0 - ((k - 0.85) / 0.15 * PI).sin() * 0.04
    };
    Transform::from_xyz(0.0, 0.13 * e.min(1.0), 0.0)
        .with_rotation(Quat::from_rotation_x(FRAC_PI_2 * e))
}

// ---------------------------------------------------------------------------------------------

pub struct RigPlugin;

/// Набор систем, которые пишут `RigPose`; применение позы идёт после них.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoseSet;

impl Plugin for RigPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BladeTints>()
            .add_systems(Startup, load_models)
            .add_systems(
                Update,
                finish_models.run_if(resource_exists::<ModelsHandle>),
            )
            .add_systems(PostUpdate, apply_look_tints)
            .add_systems(
                Update,
                attach_rigs
                    .run_if(resource_exists::<CharacterModels>)
                    .before(PoseSet),
            )
            .add_systems(Update, apply_poses.after(PoseSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_pose_stands_on_ground() {
        let s = solve_pose(&PoseInput::default());
        // Колени почти прямые, стопы на земле: таз не ниже исходного более чем на сантиметр.
        assert!((s.pose.pelvis.y - PELVIS_HEIGHT).abs() < 0.01);
        for r in s.pose.rot {
            assert!(r.is_normalized());
        }
    }

    #[test]
    fn hand_reaches_grip() {
        let grip = Vec3::new(0.25, 1.2, -0.35);
        for axis in [
            None,
            Some(Vec3::Y),
            Some(Vec3::new(0.3, 0.5, -0.8).normalize()),
        ] {
            let s = solve_pose(&PoseInput {
                right_grip: Some(grip),
                right_axis: axis,
                ..default()
            });
            assert!(
                s.right_grip.distance(grip) < 0.02,
                "кулак в {:?} (ось {axis:?})",
                s.right_grip
            );
        }
    }

    #[test]
    fn fist_hole_follows_weapon_axis() {
        let axis = Vec3::new(0.2, 0.9, -0.4).normalize();
        let q = fist_rot(axis, Vec3::new(0.0, -0.3, -1.0).normalize(), GRIP_AXIS_R);
        assert!((q * GRIP_AXIS_R).normalize().dot(-axis) > 0.999);
    }

    #[test]
    fn far_grip_is_clamped() {
        let grip = Vec3::new(0.3, 1.2, -2.0);
        let s = solve_pose(&PoseInput {
            right_grip: Some(grip),
            ..default()
        });
        assert!(s.right_grip.distance(grip) > 1.0);
        assert!(s.right_grip.z < -0.3);
    }

    #[test]
    fn walking_feet_do_not_slide() {
        // Пока стопа на земле, она отъезжает назад ровно на столько, сколько проходит тело
        // (цикл шага — 1,5 м пути): иначе ноги «скользят по льду».
        for speed in [1.3, 4.0, 6.5] {
            let beta = stance_share(speed);
            let a = gait(0.2 * beta, speed);
            let b = gait(0.6 * beta, speed);
            let body = step_cycle(speed) * (0.4 * beta);
            assert!(
                ((b.z - a.z) - body).abs() < 0.01,
                "стопа {} при пути тела {body}",
                b.z - a.z
            );
            // В переносе стопа поднята.
            assert!(gait(beta + 0.5 * (1.0 - beta), speed).lift > 0.05);
            // Цикл непрерывный: конец переноса совпадает с началом опоры.
            let end = gait(0.9999, speed);
            let start = gait(0.0, speed);
            assert!(
                (end.z - start.z).abs() < 0.01
                    && (end.lift - start.lift).abs() < 0.01
                    && (end.pitch - start.pitch).abs() < 0.02,
                "{speed}: {:?} {:?}",
                (end.z, end.lift, end.pitch),
                (start.z, start.lift, start.pitch)
            );
        }
        // Шагом в середине опоры стопа на земле и ровная.
        let mid = gait(0.4 * stance_share(1.3), 1.3);
        assert!(mid.lift < 1e-3 && mid.pitch.abs() < 0.05);
    }

    #[test]
    fn walking_legs_reach_the_ground() {
        // Шагом колено не выпрямляется до отказа: стопа достаёт до земли во всём цикле.
        for k in 0..40 {
            let u = k as f32 / 40.0;
            let s = solve_pose(&PoseInput {
                walk: 1.0,
                walk_phase: u * TAU,
                ..default()
            });
            let step = gait(u, 1.3);
            let hip = s.pose.pelvis + s.pose.rot[0] * REST[9];
            let ankle = Vec3::new(0.11, ANKLE_HEIGHT + step.lift, step.z);
            assert!(
                hip.distance(ankle) < THIGH + SHIN + 0.012,
                "u = {u}: {}",
                hip.distance(ankle)
            );
        }
    }

    #[test]
    fn walk_cycle_keeps_feet_on_ground_and_limbs_sane() {
        for speed in [0.0, 1.3, 4.0, 6.5] {
            for k in 0..32 {
                let s = solve_pose(&PoseInput {
                    walk: 1.0,
                    speed,
                    walk_phase: k as f32 / 32.0 * TAU,
                    time: k as f32 * 0.3,
                    ..default()
                });
                assert!(s.pose.rot.iter().all(|r| r.is_normalized()));
                // Таз не проваливается и не взлетает.
                assert!(
                    (s.pose.pelvis.y - PELVIS_HEIGHT).abs() < 0.12,
                    "{}",
                    s.pose.pelvis.y
                );
            }
        }
    }

    #[test]
    fn bone_rot_identity() {
        let q = bone_rot(Vec3::NEG_Y, Vec3::NEG_Z, Vec3::NEG_Z);
        assert!(q.angle_between(Quat::IDENTITY) < 1e-4);
    }

    #[test]
    fn every_outfit_part_exists_in_model() {
        // Имена слоёв должны совпадать с build_humans.py / build_armor.py — проверяем по самим glb.
        let json_of = |path: &str| {
            let glb = std::fs::read(format!("{}/assets/{path}", env!("CARGO_MANIFEST_DIR")))
                .unwrap_or_else(|_| panic!("нет {path}"));
            let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
            String::from_utf8_lossy(&glb[20..20 + len]).to_string()
        };
        let json = json_of(MODEL_PATH) + &json_of(ARMOR_PATH);
        let mut names: Vec<String> = Outfit::for_actor(ActorKind::Player, 1)
            .layers()
            .iter()
            .map(|s| s.to_string())
            .collect();
        for kind in ActorKind::ENEMIES {
            for id in 0..8 {
                names.extend(
                    Outfit::for_actor(kind, id)
                        .layers()
                        .iter()
                        .map(|s| s.to_string()),
                );
            }
        }
        // Любая причёска и борода из окна облика.
        for hair in 0..crate::appearance::HAIR_STYLES.len() {
            for beard in 0..crate::appearance::BEARD_STYLES.len() {
                let look = Look {
                    hair: hair as u8,
                    beard: beard as u8,
                    ..Look::PLAYER_DEFAULT
                };
                names.extend(
                    Outfit::for_actor(ActorKind::Player, 1)
                        .with_look(look)
                        .layers()
                        .iter()
                        .map(|s| s.to_string()),
                );
            }
        }
        // Игрок в любой надеваемой вещи.
        for (i, b) in crate::items::ITEM_BASES.iter().enumerate() {
            use crate::items::EquipSlot as S;
            let id = Some(i as u16);
            let gear = match b.slot {
                Some(S::Head) => crate::gear_look::Gear {
                    head: id,
                    ..default()
                },
                Some(S::Chest) => crate::gear_look::Gear {
                    chest: id,
                    ..default()
                },
                Some(S::Hands) => crate::gear_look::Gear {
                    hands: id,
                    ..default()
                },
                Some(S::Legs) => crate::gear_look::Gear {
                    legs: id,
                    ..default()
                },
                Some(S::Feet) => crate::gear_look::Gear {
                    feet: id,
                    ..default()
                },
                _ => continue,
            };
            names.extend(
                Outfit::for_actor(ActorKind::Player, 1)
                    .with_gear(&gear)
                    .layers()
                    .iter()
                    .map(|s| s.to_string()),
            );
        }
        for n in names {
            assert!(
                json.contains(&format!("\"name\":\"{n}\"")),
                "в humans.glb и armor.glb нет детали {n}"
            );
        }
    }
}
