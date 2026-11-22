//! Модели персонажей и их процедурная анимация.
//!
//! Модели лежат в `assets/models/characters.glb` (их собирает `tools/models/build_characters.py`).
//! Тело — набор жёстких деталей на «костях» (таз, торс, голова, руки, ноги), поверх — броня
//! по слотам экипировки. Анимация считается кодом: руки тянутся к рукояти меча и щиту (IK),
//! ноги шагают по земле, торс поворачивается за ударом, при оглушении откидывается назад,
//! после смерти тело падает навзничь.

use crate::combat::WeaponKind;
use crate::protocol::ActorKind;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh};
use bevy::pbr::gltf::standard_material_from_gltf_material;
use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

pub const MODEL_PATH: &str = "models/characters.glb";

// ---------------------------------------------------------------------------------------------
// Кости

pub const BONES: usize = 15;

/// Номера костей (индексы в массивах ниже).
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

/// Имя кости в glb (детали называются "<имя>.<слой>").
const MESH_NAME: [&str; BONES] = [
    "pelvis", "torso", "head", "upper_arm_r", "forearm_r", "hand_r", "upper_arm_l", "forearm_l", "hand_l", "thigh_r", "shin_r", "foot_r", "thigh_l", "shin_l", "foot_l",
];

/// Положение сустава относительно родителя в покое (как JOINTS в build_characters.py).
const REST: [Vec3; BONES] = [
    Vec3::new(0.0, PELVIS_HEIGHT, 0.0),
    Vec3::new(0.0, 0.06, 0.0),
    Vec3::new(0.0, 0.46, 0.0),
    Vec3::new(0.2, 0.38, 0.0),
    Vec3::new(0.0, -UPPER_ARM, 0.0),
    Vec3::new(0.0, -FOREARM, 0.0),
    Vec3::new(-0.2, 0.38, 0.0),
    Vec3::new(0.0, -UPPER_ARM, 0.0),
    Vec3::new(0.0, -FOREARM, 0.0),
    Vec3::new(0.1, -0.04, 0.0),
    Vec3::new(0.0, -THIGH, 0.0),
    Vec3::new(0.0, -SHIN, 0.0),
    Vec3::new(-0.1, -0.04, 0.0),
    Vec3::new(0.0, -THIGH, 0.0),
    Vec3::new(0.0, -SHIN, 0.0),
];

const PELVIS_HEIGHT: f32 = 0.98;
const UPPER_ARM: f32 = 0.29;
const FOREARM: f32 = 0.26;
const THIGH: f32 = 0.44;
const SHIN: f32 = 0.42;
const ANKLE_HEIGHT: f32 = 0.08;
/// От запястья до центра кулака (там, где рукоять).
const FIST: f32 = 0.055;

// ---------------------------------------------------------------------------------------------
// Снаряжение по слотам

#[allow(dead_code)] // «без шлема/перчаток» понадобится, когда экипировка из инвентаря будет видна
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadGear {
    Bare,
    KettleHat,
    NasalHelm,
    GreatHelm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChestGear {
    Gambeson,
    MailAndTabard,
    PlateAndTabard,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandGear {
    Bare,
    Gloves,
    Gauntlets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegGear {
    Cloth,
    Greaves,
    Plate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootGear {
    Boots,
    Sabatons,
}

/// Во что одет персонаж. Поля совпадают со слотами экипировки (голова, торс, руки, ноги, ступни).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outfit {
    pub head: HeadGear,
    pub chest: ChestGear,
    pub hands: HandGear,
    pub legs: LegGear,
    pub feet: FootGear,
    /// Цвет стёганки или сюрко (герб рода, цвет игрока).
    pub colors: Color,
    /// Цвет штанов.
    pub trousers: Color,
}

impl Outfit {
    pub fn for_actor(kind: ActorKind, id: u64) -> Self {
        match kind {
            ActorKind::Player => {
                // Небелёная стёганка с оттенком цвета игрока, чтобы игроков можно было различать.
                let hue = (id.wrapping_mul(2654435761) % 360) as f32;
                Outfit {
                    head: HeadGear::KettleHat,
                    chest: ChestGear::Gambeson,
                    hands: HandGear::Gloves,
                    legs: LegGear::Cloth,
                    feet: FootGear::Boots,
                    colors: Color::hsl(hue, 0.35, 0.62),
                    trousers: Color::srgb(0.24, 0.2, 0.16),
                }
            }
            // Временно на старых деталях, пока нет моделей эпохи.
            ActorKind::Stratiotes | ActorKind::GothWarrior | ActorKind::HunArcher => Outfit {
                head: HeadGear::NasalHelm,
                chest: ChestGear::MailAndTabard,
                hands: HandGear::Gauntlets,
                legs: LegGear::Greaves,
                feet: FootGear::Boots,
                colors: Color::srgb(0.13, 0.22, 0.55),
                trousers: Color::srgb(0.2, 0.18, 0.17),
            },
            ActorKind::Bucellarius | ActorKind::AlanLancer => Outfit {
                head: HeadGear::GreatHelm,
                chest: ChestGear::PlateAndTabard,
                hands: HandGear::Gauntlets,
                legs: LegGear::Plate,
                feet: FootGear::Sabatons,
                colors: Color::srgb(0.5, 0.07, 0.06),
                trousers: Color::srgb(0.15, 0.13, 0.12),
            },
        }
    }

    /// Слои (детали glb), надетые на кость.
    pub fn layers(&self, bone: usize) -> Vec<&'static str> {
        let mut l = Vec::new();
        match bone {
            0 => {
                l.push("body");
                match self.chest {
                    ChestGear::Gambeson => l.extend(["gambeson", "belt"]),
                    ChestGear::MailAndTabard => l.extend(["mail", "tabard", "belt"]),
                    ChestGear::PlateAndTabard => l.extend(["mail", "faulds", "surcoat"]),
                }
            }
            1 => {
                l.push("body");
                match self.chest {
                    ChestGear::Gambeson => {}
                    ChestGear::MailAndTabard => l.extend(["mail", "tabard"]),
                    ChestGear::PlateAndTabard => l.extend(["cuirass", "surcoat"]),
                }
            }
            2 => match self.head {
                HeadGear::Bare => l.push("body"),
                HeadGear::KettleHat => l.extend(["body", "kettle_hat"]),
                HeadGear::NasalHelm => l.extend(["body", "coif", "nasal_helm"]),
                // Под глухим шлемом голову не видно — не рисуем.
                HeadGear::GreatHelm => l.push("great_helm"),
            },
            3 | 6 => {
                l.push("body");
                match self.chest {
                    ChestGear::Gambeson => {}
                    ChestGear::MailAndTabard => l.push("mail"),
                    ChestGear::PlateAndTabard => l.extend(["pauldron", "rerebrace"]),
                }
            }
            4 | 7 => {
                l.push("body");
                match self.hands {
                    HandGear::Bare => {}
                    HandGear::Gloves => l.push("glove_cuff"),
                    HandGear::Gauntlets if self.chest == ChestGear::PlateAndTabard => l.push("vambrace"),
                    HandGear::Gauntlets => l.push("gauntlet_cuff"),
                }
            }
            5 | 8 => l.push(match self.hands {
                HandGear::Bare => "body",
                HandGear::Gloves => "glove",
                HandGear::Gauntlets => "gauntlet",
            }),
            9 | 12 => {
                l.push("body");
                if self.legs == LegGear::Plate {
                    l.push("cuisse");
                }
            }
            10 | 13 => {
                l.push("body");
                match self.legs {
                    LegGear::Cloth if self.feet == FootGear::Boots => l.push("boot_cuff"),
                    LegGear::Cloth => {}
                    LegGear::Greaves | LegGear::Plate => l.push("greave"),
                }
            }
            _ => l.push(match self.feet {
                FootGear::Boots => "boot",
                FootGear::Sabatons => "sabaton",
            }),
        }
        l
    }
}

// ---------------------------------------------------------------------------------------------
// Загрузка моделей

#[derive(Resource)]
struct ModelsHandle(Handle<Gltf>);

/// Детали из glb: меш + материал для каждого примитива.
/// Примитив детали: меш, материал и имя материала из glb.
type Primitive = (Handle<Mesh>, Handle<StandardMaterial>, String);

#[derive(Resource)]
pub struct CharacterModels {
    parts: HashMap<String, Vec<Primitive>>,
    tinted: HashMap<(String, [u8; 4]), Handle<StandardMaterial>>,
    pub blade: Handle<StandardMaterial>,
}

impl CharacterModels {
    fn has(&self, name: &str) -> bool {
        self.parts.contains_key(name)
    }

    /// Создаёт детали меша как дочерние сущности. Ткань красится в цвета наряда.
    fn spawn_part(&mut self, parent: &mut ChildSpawnerCommands, materials: &mut Assets<StandardMaterial>, name: &str, outfit: Option<&Outfit>, extra: impl Bundle + Clone) {
        let Some(prims) = self.parts.get(name).cloned() else {
            warn!("в {MODEL_PATH} нет детали {name}");
            return;
        };
        for (mesh, mat, mat_name) in prims {
            let tint = outfit.and_then(|o| match mat_name.as_str() {
                "gambeson" | "tabard" | "surcoat" => Some(o.colors),
                "cloth" => Some(o.trousers),
                _ => None,
            });
            let mat = match tint {
                Some(c) => self.tint(materials, &mat_name, &mat, c),
                None => mat,
            };
            parent.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), extra.clone()));
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

fn finish_models(
    mut commands: Commands,
    handle: Res<ModelsHandle>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !server.is_loaded_with_dependencies(&handle.0) {
        return;
    }
    let Some(gltf) = gltfs.get(&handle.0) else { return };
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
    info!("Модели персонажей загружены: {} деталей", parts.len());
    commands.insert_resource(CharacterModels { parts, tinted: HashMap::new(), blade });
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

/// Щит рыцаря.
#[derive(Component)]
pub struct NeedsShield;

/// Рука от первого лица (кости плеча, предплечья и кисти — прямые дети камеры).
#[derive(Component)]
pub struct NeedsViewArm(pub Option<Outfit>);

#[derive(Component)]
pub struct ViewArm {
    joints: [Entity; 3],
}

pub fn weapon_mesh(weapon: WeaponKind) -> &'static str {
    match weapon {
        // Временно на старых мешах, пока нет моделей эпохи.
        WeaponKind::Spatha | WeaponKind::Axe | WeaponKind::Seax | WeaponKind::Bow => "arming_sword",
        WeaponKind::Spear => "greatsword",
    }
}

#[allow(clippy::too_many_arguments)]
fn attach_rigs(
    mut commands: Commands,
    mut models: ResMut<CharacterModels>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    rigs: Query<(Entity, &NeedsRig)>,
    weapons: Query<(Entity, &NeedsWeapon)>,
    shields: Query<Entity, With<NeedsShield>>,
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
            commands.entity(joint).with_children(|j| {
                for layer in outfit.layers(bone) {
                    let name = format!("{}.{layer}", MESH_NAME[bone]);
                    if models.has(&name) {
                        models.spawn_part(j, &mut materials, &name, Some(&outfit), ());
                    }
                }
            });
            joints[bone] = joint;
        }
        commands.entity(e).remove::<NeedsRig>().insert((Rig { joints }, RigPose::default()));
    }
    for (e, need) in &weapons {
        let name = weapon_mesh(need.0);
        commands.entity(e).remove::<NeedsWeapon>().with_children(|w| {
            models.spawn_part(w, &mut materials, &format!("{name}.blade"), None, crate::client::Blade);
            models.spawn_part(w, &mut materials, &format!("{name}.hilt"), None, ());
        });
    }
    for e in &shields {
        commands.entity(e).remove::<NeedsShield>().with_children(|s| {
            models.spawn_part(s, &mut materials, "shield.body", None, ());
            models.spawn_part(s, &mut materials, "shield.boss", None, ());
        });
    }
    for (e, need) in &arms {
        let my_id = net.as_ref().and_then(|n| n.my_id).unwrap_or(0);
        let outfit = need.0.unwrap_or_else(|| Outfit::for_actor(ActorKind::Player, my_id));
        let mut joints = [Entity::PLACEHOLDER; 3];
        for (i, bone) in [Bone::UpperArmR, Bone::ForearmR, Bone::HandR].into_iter().enumerate() {
            let joint = commands.spawn((Transform::default(), Visibility::default())).id();
            commands.entity(e).add_child(joint);
            commands.entity(joint).with_children(|j| {
                for layer in outfit.layers(bone as usize) {
                    let name = format!("{}.{layer}", MESH_NAME[bone as usize]);
                    if models.has(&name) {
                        models.spawn_part(j, &mut materials, &name, Some(&outfit), ());
                    }
                }
            });
            joints[i] = joint;
        }
        commands.entity(e).remove::<NeedsViewArm>().insert(ViewArm { joints });
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

/// Рука от первого лица: (положение, поворот) плеча, локтя и кисти в пространстве камеры.
pub fn view_arm_pose(shoulder: Vec3, grip: Vec3) -> [(Vec3, Quat); 3] {
    // Плечо «подтягиваем» к рукояти: от первого лица его не видно, а меч всегда в руке.
    let reach = UPPER_ARM + FOREARM + FIST - 0.03;
    let to = grip - shoulder;
    let shoulder = if to.length() > reach { grip - to.normalize() * reach } else { shoulder };
    let (elbow, wrist) = reach_grip(shoulder, grip, Vec3::new(1.0, -1.0, 0.6));
    let up = (elbow - shoulder).normalize_or(Vec3::NEG_Y);
    let fore = (wrist - elbow).normalize_or(Vec3::NEG_Y);
    let q_up = bone_rot(up, fore, Vec3::NEG_Z);
    let q_fore = Quat::from_rotation_arc(up, fore) * q_up;
    [(shoulder, q_up), (elbow, q_fore), (wrist, q_fore)]
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

/// IK руки так, чтобы центр кулака (продолжение предплечья) оказался в `grip`.
fn reach_grip(shoulder: Vec3, grip: Vec3, pole: Vec3) -> (Vec3, Vec3) {
    let mut fore = (grip - shoulder).normalize_or(Vec3::NEG_Y);
    let mut joints = (shoulder, shoulder);
    for _ in 0..4 {
        joints = two_bone(shoulder, grip - fore * FIST, UPPER_ARM, FOREARM, pole);
        fore = (joints.1 - joints.0).normalize_or(fore);
    }
    joints
}

pub fn solve_pose(inp: &PoseInput) -> Solved {
    let mut world = [Quat::IDENTITY; BONES];
    let mut local = [Quat::IDENTITY; BONES];
    let walk = inp.walk.clamp(0.0, 1.0);
    let crouch = inp.crouch.clamp(0.0, 1.0);

    // Таз: проседает при приседе и покачивается при ходьбе.
    let bob = 0.025 * walk * (1.0 - (2.0 * inp.walk_phase).cos()) * 0.5;
    let pelvis = Vec3::new(0.0, PELVIS_HEIGHT - 0.5 * crouch - bob, 0.08 * crouch);
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
    for (side, target) in [inp.right_grip, inp.left_grip].into_iter().enumerate() {
        let (ua, fa, hd) = if side == 0 { (3, 4, 5) } else { (6, 7, 8) };
        let sign = if side == 0 { 1.0 } else { -1.0 };
        let shoulder = torso_pos + world[1] * REST[ua];
        match target {
            Some(grip) => {
                let pole = world[1] * Vec3::new(0.7 * sign, -0.8, 0.5);
                let (elbow, wrist) = reach_grip(shoulder, grip, pole);
                let up = (elbow - shoulder).normalize_or(Vec3::NEG_Y);
                let fore = (wrist - elbow).normalize_or(Vec3::NEG_Y);
                world[ua] = bone_rot(up, fore, world[1] * Vec3::NEG_Z);
                world[fa] = Quat::from_rotation_arc(up, fore) * world[ua];
                world[hd] = world[fa];
                grips[side] = wrist + fore * FIST;
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
                grips[side] = elbow + world[fa] * Vec3::new(0.0, -FOREARM - FIST, 0.0);
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
        app.add_systems(Startup, load_models)
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
        let s = solve_pose(&PoseInput { right_grip: Some(grip), ..default() });
        assert!(s.right_grip.distance(grip) < 0.02, "кулак в {:?}", s.right_grip);
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
    fn every_outfit_part_exists_in_model_list() {
        // Имена слоёв должны совпадать с build_characters.py.
        let known = [
            "body", "belt", "gambeson", "mail", "tabard", "surcoat", "faulds", "cuirass", "kettle_hat", "coif", "nasal_helm", "great_helm", "pauldron", "rerebrace", "glove_cuff", "vambrace",
            "gauntlet_cuff", "glove", "gauntlet", "cuisse", "boot_cuff", "greave", "boot", "sabaton",
        ];
        for kind in std::iter::once(ActorKind::Player).chain(ActorKind::ENEMIES) {
            let o = Outfit::for_actor(kind, 1);
            for bone in 0..BONES {
                for l in o.layers(bone) {
                    assert!(known.contains(&l), "{l}");
                }
            }
        }
    }
}
