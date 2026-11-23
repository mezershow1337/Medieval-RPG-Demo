//! Модели людей и их процедурная анимация.
//!
//! Модели лежат в `assets/models/humans.glb` (их собирает `tools/models/build_humans.py` из
//! базового тела MakeHuman, CC0). Тело и одежда — скинованные меши на скелете из 15 костей
//! (таз, торс, голова, руки, ноги); шлем, бармица и прочее надеваются слоями по снаряжению.
//! Анимация считается кодом: кулаки ложатся на рукоять оружия и ручку щита (IK с поворотом кисти
//! по оси рукояти), ноги шагают по земле, торс поворачивается за ударом, при оглушении
//! откидывается назад, после смерти тело падает навзничь.

use crate::combat::WeaponKind;
use crate::protocol::ActorKind;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh, GltfSkin};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::pbr::gltf::standard_material_from_gltf_material;
use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

#[path = "rig_dims.rs"]
mod dims;
use dims::{ANKLE_HEIGHT, FOREARM, GRIP_AXIS_L, GRIP_AXIS_R, GRIP_L, GRIP_R, REST, SHIN, THIGH, UPPER_ARM};

pub const MODEL_PATH: &str = "models/humans.glb";

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

const PARENT: [Option<usize>; BONES] = [None, Some(0), Some(1), Some(1), Some(3), Some(4), Some(1), Some(6), Some(7), Some(0), Some(9), Some(10), Some(0), Some(12), Some(13)];

const PELVIS_HEIGHT: f32 = REST[0].y;

// ---------------------------------------------------------------------------------------------
// Снаряжение по слотам (одежда эпохи: Херсонес, 510 г.)

#[allow(dead_code)] // «с непокрытой головой» понадобится, когда экипировка из инвентаря будет видна
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadGear {
    Bare,
    /// Спангенхельм: купол из сегментов на бронзовом каркасе, нащёчники, бармица сзади.
    Spangenhelm,
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

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyArmor {
    None,
    /// Кольчуга до середины бедра с короткими рукавами.
    Mail,
    /// Ламеллярный доспех из железных пластин на кожаных шнурах.
    Lamellar,
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

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hair {
    Shaved,
    Short,
    Long,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beard {
    None,
    Short,
    Full,
}

/// Во что одет персонаж. Поля совпадают со слотами экипировки (голова, торс, ноги, ступни).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outfit {
    pub head: HeadGear,
    pub tunic: Tunic,
    pub armor: BodyArmor,
    pub legs: LegGear,
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
}

/// Небольшой разброс по номеру персонажа, чтобы одинаковые враги не были близнецами.
fn jitter(id: u64, salt: u64) -> f32 {
    let h = id.wrapping_add(salt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    ((h >> 40) % 1000) as f32 / 1000.0
}

impl Outfit {
    pub fn for_actor(kind: ActorKind, id: u64) -> Self {
        let j = |s| jitter(id, s);
        let brown = |k: f32| Color::srgb(0.12 + 0.25 * k, 0.08 + 0.15 * k, 0.05 + 0.08 * k);
        let skin_med = Color::srgb(0.97 + 0.03 * j(3), 0.93 + 0.05 * j(3), 0.9 + 0.06 * j(3));
        let base = Outfit {
            head: HeadGear::Bare,
            tunic: Tunic::Byzantine,
            armor: BodyArmor::None,
            legs: LegGear::Trousers,
            feet: FootGear::Boots,
            hair: Hair::Short,
            beard: if j(1) < 0.5 { Beard::Short } else { Beard::Full },
            tunic_color: Color::WHITE,
            trousers: Color::srgb(0.45, 0.4, 0.34),
            hair_color: brown(j(2) * 0.6),
            skin: skin_med,
        };
        match kind {
            ActorKind::Player => {
                // Некрашеная шерсть с лёгким оттенком цвета игрока.
                let hue = (id.wrapping_mul(2654435761) % 360) as f32;
                Outfit {
                    legs: LegGear::Wrapped,
                    feet: FootGear::Shoes,
                    beard: Beard::Short,
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
                hair: Hair::Long,
                beard: Beard::Full,
                tunic_color: Color::srgb(0.72 + 0.2 * j(4), 0.68 + 0.15 * j(4), 0.55),
                trousers: Color::srgb(0.5, 0.45, 0.36),
                hair_color: Color::srgb(0.55 + 0.3 * j(2), 0.42 + 0.25 * j(2), 0.25 + 0.12 * j(2)),
                skin: Color::srgb(1.0, 0.96, 0.95),
                ..base
            },
            ActorKind::AlanLancer => Outfit {
                head: HeadGear::Conical,
                tunic: Tunic::Kaftan,
                armor: BodyArmor::Lamellar,
                beard: Beard::Full,
                tunic_color: Color::srgb(0.6, 0.45, 0.3),
                trousers: Color::srgb(0.35, 0.3, 0.25),
                ..base
            },
            ActorKind::HunArcher => Outfit {
                head: HeadGear::FeltCap,
                tunic: Tunic::Kaftan,
                hair: Hair::Shaved,
                beard: if j(1) < 0.6 { Beard::Short } else { Beard::None },
                tunic_color: Color::srgb(0.55 + 0.2 * j(4), 0.5 + 0.15 * j(4), 0.42),
                trousers: Color::srgb(0.32, 0.27, 0.22),
                hair_color: Color::srgb(0.08, 0.06, 0.05),
                skin: Color::srgb(0.93, 0.86, 0.78),
                ..base
            },
        }
    }

    /// Детали glb (скинованные на весь скелет), из которых собран персонаж.
    pub fn layers(&self) -> Vec<&'static str> {
        let mut l = vec!["body.head", "body.hands", "eyes", "belt"];
        l.push(match self.tunic {
            Tunic::Byzantine => "tunic.byz",
            Tunic::Gothic => "tunic.goth",
            Tunic::Kaftan => "tunic.kaftan",
        });
        match self.armor {
            BodyArmor::None => {}
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
        // Под шлемом и шапкой волос не видно.
        if self.head == HeadGear::Bare {
            match self.hair {
                Hair::Shaved => {}
                Hair::Short => l.push("hair.short"),
                Hair::Long => l.push("hair.long"),
            }
        }
        match self.beard {
            Beard::None => {}
            Beard::Short => l.push("beard.short"),
            Beard::Full => l.push("beard.full"),
        }
        match self.head {
            HeadGear::Bare => {}
            HeadGear::Spangenhelm => l.push("helm.spangen"),
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
            "hair_short" | "hair_long" | "beard_short" | "beard_full" => Some(self.hair_color),
            "skin" => Some(self.skin),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Загрузка моделей

#[derive(Resource)]
struct ModelsHandle(Handle<Gltf>);

/// Примитив детали: меш, материал и имя материала из glb.
type Primitive = (Handle<Mesh>, Handle<StandardMaterial>, String);

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
    pub fn get(&mut self, materials: &mut Assets<StandardMaterial>, base: &Handle<StandardMaterial>, finish: u8) -> Handle<StandardMaterial> {
        let bucket = finish / 32;
        if bucket >= 5 {
            return base.clone();
        }
        if let Some(h) = self.0.get(&(base.id(), bucket)) {
            return h.clone();
        }
        let Some(mut m) = materials.get(base).cloned() else { return base.clone() };
        // 0 — бурая ржа и окалина, 4 — чуть тусклее модели.
        let k = 1.0 - bucket as f32 / 5.0;
        let c = m.base_color.to_srgba();
        let rust = Srgba::new(0.36, 0.25, 0.18, c.alpha);
        m.base_color = Color::Srgba(Srgba::new(c.red + (rust.red - c.red) * k, c.green + (rust.green - c.green) * k, c.blue + (rust.blue - c.blue) * k, c.alpha));
        m.metallic *= 1.0 - k * 0.6;
        m.perceptual_roughness = m.perceptual_roughness + (0.9 - m.perceptual_roughness) * k;
        let h = materials.add(m);
        self.0.insert((base.id(), bucket), h.clone());
        h
    }
}

/// Лезвие оружия: хранит свой обычный материал (на время ответного удара его подменяют).
#[derive(Component, Clone)]
pub struct Blade(pub Handle<StandardMaterial>);

impl CharacterModels {
    /// Создаёт примитивы детали как дочерние сущности. `skin` — суставы для скинованной детали.
    fn spawn_part(&mut self, parent: &mut ChildSpawnerCommands, materials: &mut Assets<StandardMaterial>, name: &str, outfit: Option<&Outfit>, skin: Option<&[Entity]>, blade: bool) {
        let Some(prims) = self.parts.get(name).cloned() else {
            warn!("в {MODEL_PATH} нет детали {name}");
            return;
        };
        for (mesh, mat, mat_name) in prims {
            let mat = match outfit.and_then(|o| o.tint(&mat_name)) {
                Some(c) => self.tint(materials, &mat_name, &mat, c),
                None => mat,
            };
            let mut e = parent.spawn((Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::default()));
            if let Some(joints) = skin {
                e.insert((SkinnedMesh { inverse_bindposes: self.bindposes.clone(), joints: joints.to_vec() }, NoFrustumCulling));
            }
            if blade {
                e.insert(Blade(mat));
            }
        }
    }

    fn tint(&mut self, materials: &mut Assets<StandardMaterial>, name: &str, base: &Handle<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
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
    commands.insert_resource(ModelsHandle(server.load(MODEL_PATH)));
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
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !server.is_loaded_with_dependencies(&handle.0) {
        return;
    }
    let Some(gltf) = gltfs.get(&handle.0) else { return };
    let Some(skin) = gltf.skins.first().and_then(|h| gltf_skins.get(h)) else {
        error!("в {MODEL_PATH} нет скелета");
        commands.remove_resource::<ModelsHandle>();
        return;
    };
    let mut by_handle: HashMap<AssetId<GltfMaterial>, (Handle<StandardMaterial>, String)> = HashMap::new();
    for (name, h) in &gltf.named_materials {
        if let Some(m) = gltf_materials.get(h) {
            let mut sm = standard_material_from_gltf_material(m);
            // Без карты окружения чистый металл выглядит почти чёрным — оставляем часть рассеянного света.
            sm.metallic *= 0.6;
            by_handle.insert(h.id(), (materials.add(sm), name.to_string()));
        }
    }
    let fallback = materials.add(Color::srgb(0.6, 0.6, 0.6));
    let mut parts = HashMap::new();
    for (name, h) in &gltf.named_meshes {
        let Some(mesh) = gltf_meshes.get(h) else { continue };
        let prims = mesh
            .primitives
            .iter()
            .map(|p| {
                let (mat, mat_name) = p.material.as_ref().and_then(|m| by_handle.get(&m.id()).cloned()).unwrap_or((fallback.clone(), String::new()));
                (p.mesh.clone(), mat, mat_name)
            })
            .collect();
        parts.insert(name.to_string(), prims);
    }
    let blade = by_handle.values().find(|(_, n)| n == "blade").map(|(h, _)| h.clone()).unwrap_or(fallback);
    info!("Модели людей загружены: {} деталей", parts.len());
    commands.insert_resource(CharacterModels { parts, tinted: HashMap::new(), bindposes: skin.inverse_bind_matrices.clone(), blade });
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
}

impl Default for RigPose {
    fn default() -> Self {
        Self { pelvis: REST[0], rot: [Quat::IDENTITY; BONES] }
    }
}

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
#[derive(Component)]
pub struct NeedsViewArm(pub Option<Outfit>);

#[derive(Component)]
pub struct ViewArm {
    joints: [Entity; 3],
}

pub fn weapon_mesh(weapon: WeaponKind) -> &'static str {
    match weapon {
        WeaponKind::Spatha => "spatha",
        WeaponKind::Axe => "axe",
        WeaponKind::Seax => "seax",
        WeaponKind::Spear => "spear",
        WeaponKind::Bow => "bow",
    }
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
            let joint = commands.spawn((Transform::from_translation(REST[bone]), Visibility::default())).id();
            commands.entity(parent).add_child(joint);
            joints[bone] = joint;
        }
        commands.entity(e).with_children(|p| {
            for layer in outfit.layers() {
                models.spawn_part(p, &mut materials, layer, Some(&outfit), Some(&joints), false);
            }
        });
        commands.entity(e).remove::<NeedsRig>().insert((Rig { joints }, RigPose::default()));
    }
    for (e, need) in &weapons {
        let name = weapon_mesh(need.0);
        commands.entity(e).remove::<NeedsWeapon>().with_children(|w| {
            models.spawn_part(w, &mut materials, &format!("{name}.blade"), None, None, true);
            models.spawn_part(w, &mut materials, &format!("{name}.hilt"), None, None, false);
        });
    }
    for (e, need) in &shields {
        let name = match need.0 {
            ShieldKind::Oval => "shield_oval",
            ShieldKind::Round => "shield_round",
        };
        commands.entity(e).remove::<NeedsShield>().with_children(|s| {
            models.spawn_part(s, &mut materials, &format!("{name}.body"), None, None, false);
            models.spawn_part(s, &mut materials, &format!("{name}.boss"), None, None, false);
        });
    }
    for (e, need) in &arms {
        let my_id = net.as_ref().and_then(|n| n.my_id).unwrap_or(0);
        let outfit = need.0.unwrap_or_else(|| Outfit::for_actor(ActorKind::Player, my_id));
        // Настоящие только плечо, предплечье и кисть правой руки; остальные кости — одна
        // сущность с нулевым масштабом у плеча, всё, что на них висит, схлопывается в точку.
        let mut arm = [Entity::PLACEHOLDER; 3];
        for i in 0..3 {
            let parent = if i == 0 { e } else { arm[i - 1] };
            let joint = commands.spawn((Transform::default(), Visibility::default())).id();
            commands.entity(parent).add_child(joint);
            arm[i] = joint;
        }
        let hidden = commands.spawn((Transform::from_scale(Vec3::ZERO), Visibility::default())).id();
        commands.entity(arm[0]).add_child(hidden);
        let mut joints = [hidden; BONES];
        joints[Bone::UpperArmR as usize] = arm[0];
        joints[Bone::ForearmR as usize] = arm[1];
        joints[Bone::HandR as usize] = arm[2];
        commands.entity(e).with_children(|p| {
            for layer in outfit.layers() {
                if layer.starts_with("body.hands") || layer.starts_with("tunic") || layer == "mail" {
                    models.spawn_part(p, &mut materials, layer, Some(&outfit), Some(&joints), false);
                }
            }
        });
        commands.entity(e).remove::<NeedsViewArm>().insert(ViewArm { joints: arm });
    }
}

fn apply_poses(rigs: Query<(&Rig, &RigPose), Changed<RigPose>>, mut joints: Query<&mut Transform>) {
    for (rig, pose) in &rigs {
        for bone in 0..BONES {
            if let Ok(mut t) = joints.get_mut(rig.joints[bone]) {
                t.translation = if bone == 0 { pose.pelvis } else { REST[bone] };
                t.rotation = pose.rot[bone];
            }
        }
    }
}

impl ViewArm {
    pub fn joints(&self) -> [Entity; 3] {
        self.joints
    }
}

/// Рука от первого лица: локальные (положение, поворот) плеча, локтя и кисти для цепочки
/// суставов под корнем руки (в пространстве камеры). `axis` — куда смотрит клинок.
pub fn view_arm_pose(shoulder: Vec3, grip: Vec3, axis: Vec3) -> [(Vec3, Quat); 3] {
    // Плечо «подтягиваем» к рукояти: от первого лица его не видно, а оружие всегда в руке.
    let reach = UPPER_ARM + FOREARM + GRIP_R.length() - 0.03;
    let to = grip - shoulder;
    let shoulder = if to.length() > reach { grip - to.normalize() * reach } else { shoulder };
    let arm = solve_arm(shoulder, Some((grip, axis)), Vec3::new(1.0, -1.0, 0.6), Vec3::NEG_Z, 0);
    let q_up = arm.rot[0];
    let q_fore = arm.rot[1];
    let q_hand = arm.rot[2];
    [
        (shoulder, q_up),
        (q_up.inverse() * (arm.elbow - shoulder), q_up.inverse() * q_fore),
        (q_fore.inverse() * (arm.wrist - arm.elbow), q_fore.inverse() * q_hand),
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
fn solve_arm(shoulder: Vec3, target: Option<(Vec3, Vec3)>, pole: Vec3, front: Vec3, side: usize) -> Arm {
    let (grip_c, hole) = if side == 0 { (GRIP_R, GRIP_AXIS_R) } else { (GRIP_L, GRIP_AXIS_L) };
    let Some((grip, axis)) = target else { unreachable!() };
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
    Arm { elbow, wrist, rot: [q_up, q_fore, hand], grip: wrist + hand * grip_c }
}

pub fn solve_pose(inp: &PoseInput) -> Solved {
    let mut world = [Quat::IDENTITY; BONES];
    let mut local = [Quat::IDENTITY; BONES];
    let walk = inp.walk.clamp(0.0, 1.0);
    let crouch = inp.crouch.clamp(0.0, 1.0);

    // Таз: проседает при приседе и покачивается при ходьбе.
    let bob = 0.025 * walk * (1.0 - (2.0 * inp.walk_phase).cos()) * 0.5;
    let pelvis = Vec3::new(0.0, PELVIS_HEIGHT - 0.5 * crouch - bob, REST[0].z + 0.08 * crouch);
    local[0] = Quat::from_rotation_y(-inp.twist * 0.3 + 0.08 * walk * inp.walk_phase.sin());
    world[0] = local[0];

    // Торс и голова.
    let breathe = (inp.time * 1.7).sin() * 0.012;
    local[1] = Quat::from_rotation_y(inp.twist * 0.7) * Quat::from_rotation_x(inp.lean - 0.35 * crouch + breathe);
    world[1] = world[0] * local[1];
    let head_pitch = (inp.pitch * 0.8 - (inp.lean - 0.35 * crouch)).clamp(-0.8, 0.7);
    local[2] = Quat::from_rotation_y(-inp.twist * 0.6) * Quat::from_rotation_x(head_pitch);
    world[2] = world[1] * local[2];

    let torso_pos = pelvis + world[0] * REST[1];
    let mut grips = [Vec3::ZERO; 2];
    // Руки: 0 — правая, 1 — левая.
    for (side, (target, axis)) in [(inp.right_grip, inp.right_axis), (inp.left_grip, inp.left_axis)].into_iter().enumerate() {
        let (ua, fa, hd) = if side == 0 { (3, 4, 5) } else { (6, 7, 8) };
        let sign = if side == 0 { 1.0 } else { -1.0 };
        let shoulder = torso_pos + world[1] * REST[ua];
        match target {
            Some(grip) => {
                let pole = world[1] * Vec3::new(0.7 * sign, -0.8, 0.5);
                // Без оси рукоять считается продолжением предплечья (кулак «смотрит» вперёд).
                let axis = axis.unwrap_or_else(|| (grip - shoulder).normalize_or(Vec3::NEG_Z).cross(world[1] * Vec3::X * sign).normalize_or(Vec3::Y));
                let arm = solve_arm(shoulder, Some((grip, axis)), pole, world[1] * Vec3::NEG_Z, side);
                world[ua] = arm.rot[0];
                world[fa] = arm.rot[1];
                world[hd] = arm.rot[2];
                grips[side] = arm.grip;
            }
            None => {
                // Свободная рука висит и качается в такт шагу.
                let swing = -0.35 * walk * (inp.walk_phase + side as f32 * PI).sin() * sign;
                local[ua] = Quat::from_rotation_x(swing + 0.1) * Quat::from_rotation_z(0.1 * sign);
                local[fa] = Quat::from_rotation_x(0.25 + 0.2 * walk);
                world[ua] = world[1] * local[ua];
                world[fa] = world[ua] * local[fa];
                world[hd] = world[fa];
                let elbow = shoulder + world[ua] * REST[fa];
                let wrist = elbow + world[fa] * REST[hd];
                grips[side] = wrist + world[hd] * if side == 0 { GRIP_R } else { GRIP_L };
            }
        }
    }

    // Ноги: стопы ставим на землю (IK), колени смотрят вперёд.
    for (side, (th, sh, ft)) in [(9, 10, 11), (12, 13, 14)].into_iter().enumerate() {
        let sign = if side == 0 { 1.0 } else { -1.0 };
        let phase = inp.walk_phase + side as f32 * PI;
        let hip = pelvis + world[0] * REST[th];
        let stride = -phase.sin() * 0.24 * walk;
        let lift = phase.cos().max(0.0) * 0.09 * walk;
        // При приседе одна нога выставлена вперёд, другая отставлена.
        let crouch_offset = crouch * if side == 0 { -0.18 } else { 0.2 };
        let ankle = Vec3::new(0.11 * sign, ANKLE_HEIGHT + lift, stride + crouch_offset);
        let pole = world[0] * Vec3::new(0.15 * sign, 0.0, -1.0);
        let (knee, ankle) = two_bone(hip, ankle, THIGH, SHIN, pole);
        let up = (knee - hip).normalize_or(Vec3::NEG_Y);
        let down = (ankle - knee).normalize_or(Vec3::NEG_Y);
        world[th] = bone_rot(up, -down, world[0] * Vec3::NEG_Z);
        world[sh] = Quat::from_rotation_arc(up, down) * world[th];
        // Стопы стоят ровно на земле, носки чуть врозь.
        world[ft] = Quat::from_rotation_y(-0.1 * sign);
    }

    // Мировые повороты -> локальные.
    for bone in 0..BONES {
        if let Some(p) = PARENT[bone] {
            local[bone] = (world[p].inverse() * world[bone]).normalize();
        }
    }
    Solved { pose: RigPose { pelvis, rot: local }, right_grip: grips[0], left_grip: grips[1] }
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
    RigPose { pelvis: REST[0], rot }
}

/// Смешивает две позы (k = 0 — первая, 1 — вторая).
pub fn blend(a: &RigPose, b: &RigPose, k: f32) -> RigPose {
    let mut rot = a.rot;
    for (r, t) in rot.iter_mut().zip(b.rot.iter()) {
        *r = r.slerp(*t, k);
    }
    RigPose { pelvis: a.pelvis.lerp(b.pelvis, k), rot }
}

/// Тело падает навзничь: доля 0..1 -> поворот и подъём корня скелета.
pub fn fall_transform(k: f32) -> Transform {
    // Ускоряющееся падение с небольшим отскоком в конце.
    let e = if k < 0.85 { (k / 0.85).powi(2) } else { 1.0 - ((k - 0.85) / 0.15 * PI).sin() * 0.04 };
    Transform::from_xyz(0.0, 0.13 * e.min(1.0), 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2 * e))
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
            .add_systems(Update, finish_models.run_if(resource_exists::<ModelsHandle>))
            .add_systems(Update, attach_rigs.run_if(resource_exists::<CharacterModels>).before(PoseSet))
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
        for axis in [None, Some(Vec3::Y), Some(Vec3::new(0.3, 0.5, -0.8).normalize())] {
            let s = solve_pose(&PoseInput { right_grip: Some(grip), right_axis: axis, ..default() });
            assert!(s.right_grip.distance(grip) < 0.02, "кулак в {:?} (ось {axis:?})", s.right_grip);
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
        let s = solve_pose(&PoseInput { right_grip: Some(grip), ..default() });
        assert!(s.right_grip.distance(grip) > 1.0);
        assert!(s.right_grip.z < -0.3);
    }

    #[test]
    fn bone_rot_identity() {
        let q = bone_rot(Vec3::NEG_Y, Vec3::NEG_Z, Vec3::NEG_Z);
        assert!(q.angle_between(Quat::IDENTITY) < 1e-4);
    }

    #[test]
    fn every_outfit_part_exists_in_model() {
        // Имена слоёв должны совпадать с build_humans.py — проверяем по самому glb.
        let glb = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", "models/humans.glb")).expect("нет humans.glb");
        let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let json = String::from_utf8_lossy(&glb[20..20 + len]).to_string();
        let mut names: Vec<String> = Outfit::for_actor(ActorKind::Player, 1).layers().iter().map(|s| s.to_string()).collect();
        for kind in ActorKind::ENEMIES {
            for id in 0..8 {
                names.extend(Outfit::for_actor(kind, id).layers().iter().map(|s| s.to_string()));
            }
        }
        for w in [WeaponKind::Spatha, WeaponKind::Axe, WeaponKind::Seax, WeaponKind::Spear, WeaponKind::Bow] {
            names.push(format!("{}.blade", weapon_mesh(w)));
            names.push(format!("{}.hilt", weapon_mesh(w)));
        }
        for s in ["shield_oval", "shield_round"] {
            names.push(format!("{s}.body"));
            names.push(format!("{s}.boss"));
        }
        for n in names {
            assert!(json.contains(&format!("\"name\":\"{n}\"")), "в humans.glb нет детали {n}");
        }
    }
}
