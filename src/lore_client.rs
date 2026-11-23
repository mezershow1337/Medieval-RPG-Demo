//! Клиент: свои знания, кусты трав и временные мастера в мире, окно знаний (K).
//!
//! Управление:
//! - E у куста — собрать; E у мастера — платный урок; R у мастера — поработать подмастерьем.
//! - F или ПКМ по книге в сумке — читать; по лекарству — на рану или внутрь.
//!   Шаг в сторону или удар прерывают занятие.
//! - F по жаровне или столу в сумке — поставить перед собой; E у поставленной — забрать,
//!   R у жаровни — собрать на ней алхимический стол (нужны весы и ступка).
//! - K — окно знаний.

use crate::inventory_ui::UiFonts;
use crate::items::{Item, ItemBaseId};
use crate::loot_client::LootState;
use crate::lore::*;
use crate::lore_net::*;
use crate::player::PlayerCamera;
use crate::protocol::{decode, encode};
use bevy::prelude::*;
use bevy_renet::{RenetClient, client_connected};
use std::collections::HashMap;

pub struct LoreClientPlugin;

impl Plugin for LoreClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MyKnowledge::default())
            .insert_resource(MyTending::default())
            .insert_resource(LoreView::default())
            .add_systems(Startup, (setup_lore_assets, setup_lore_ui).after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (receive_lore, lore_input).chain().run_if(client_connected))
            .add_systems(Update, (aim_lore_target, update_lore_prompt, update_patch_looks, update_knowledge_window, update_tending_hud).chain());
    }
}

/// Что знает свой персонаж (приходит с сервера).
#[derive(Resource, Default)]
pub struct MyKnowledge(pub Knowledge);

impl MyKnowledge {
    pub fn item_name(&self, item: &Item) -> String {
        self.0.display_name(item)
    }
}

/// Огонь под своим сосудом (пока варим).
#[derive(Resource, Default)]
pub struct MyTending(pub Option<TendView>);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LoreTarget {
    Patch(u32),
    Master(u32),
    Station(u32),
}

#[derive(Resource, Default)]
pub struct LoreView {
    patches: HashMap<u32, (PatchView, Entity)>,
    masters: HashMap<u32, (MasterView, Entity)>,
    stations: HashMap<u32, (StationView, Entity)>,
    pub target: Option<LoreTarget>,
    window_open: bool,
    /// Окно знаний перестраивается, когда это число меняется.
    rev: u64,
    built_rev: u64,
}

pub fn send(client: &mut RenetClient, msg: &LoreClientMessage) {
    client.send_message(CH_LORE, encode(msg));
}

/// Обработать «использовать» для книг и лекарств. true — обработано.
pub fn try_use(client: &mut RenetClient, item: &Item, body: Option<&crate::medicine::Body>) -> bool {
    if Book::from_item(item.base).is_some() {
        send(client, &LoreClientMessage::Read { uid: item.uid });
        return true;
    }
    let name = item.info().name;
    if StationKind::from_item(name).is_some() {
        send(client, &LoreClientMessage::PlaceStation { uid: item.uid });
        return true;
    }
    if crate::medicine::is_remedy(name) {
        // Бинт, когда перевязывать нечего, — обычный расходник (лечит здоровье).
        let healthy = crate::medicine::Body::default();
        if item.base.consumable().is_some() && crate::medicine::plan(body.unwrap_or(&healthy), name).is_none() {
            return false;
        }
        send(client, &LoreClientMessage::Treat { uid: item.uid });
        return true;
    }
    false
}

/// Можно ли «использовать» предмет (для подсказок и ПКМ в сумке).
pub fn usable(base: ItemBaseId) -> bool {
    let name = base.base().name;
    Book::from_item(base).is_some() || crate::medicine::is_remedy(name) || crate::brewing::Vessel::from_item(name).is_some() || StationKind::from_item(name).is_some()
}

// ---------------------------------------------------------------------------
// Модели
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct LoreAssets {
    leaf: Handle<Mesh>,
    flower: Handle<Mesh>,
    stem: Handle<Mesh>,
    leaf_mat: Handle<StandardMaterial>,
    flower_mats: HashMap<Substance, Handle<StandardMaterial>>,
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    robes: HashMap<Teacher, Handle<StandardMaterial>>,
    bronze: Handle<StandardMaterial>,
    coals: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    cylinder: Handle<Mesh>,
    cube: Handle<Mesh>,
    ball: Handle<Mesh>,
}

#[derive(Component)]
struct PatchModel {
    id: u32,
}

#[derive(Component)]
struct PatchBloom;

/// Цвет цветка или плода (по настоящему растению).
fn bloom_color(s: Substance) -> Color {
    use Substance::*;
    match s {
        Sage => Color::srgb(0.55, 0.45, 0.8),
        Thyme => Color::srgb(0.75, 0.5, 0.8),
        StJohnsWort => Color::srgb(0.98, 0.82, 0.15),
        Henbane => Color::srgb(0.75, 0.7, 0.35),
        Juniper => Color::srgb(0.2, 0.25, 0.45),
        Wormwood => Color::srgb(0.75, 0.78, 0.72),
        Yarrow => Color::srgb(0.95, 0.94, 0.9),
        Plantain => Color::srgb(0.45, 0.5, 0.3),
        RoseHip => Color::srgb(0.85, 0.15, 0.1),
        Mallow => Color::srgb(0.9, 0.55, 0.75),
        Poppy => Color::srgb(0.6, 0.65, 0.45),
        Mint => Color::srgb(0.8, 0.7, 0.9),
        Madder => Color::srgb(0.85, 0.85, 0.4),
        Woad => Color::srgb(0.95, 0.85, 0.2),
        OakGall => Color::srgb(0.6, 0.5, 0.3),
        Comfrey => Color::srgb(0.55, 0.35, 0.65),
        Elecampane => Color::srgb(0.98, 0.8, 0.1),
        _ => Color::srgb(0.6, 0.6, 0.6),
    }
}

fn robe_color(t: Teacher) -> Color {
    match t {
        Teacher::Scribe => Color::srgb(0.12, 0.11, 0.12),
        Teacher::Grammarian => Color::srgb(0.85, 0.82, 0.72),
        Teacher::Rhizotomos => Color::srgb(0.42, 0.38, 0.25),
        Teacher::Wisewoman => Color::srgb(0.45, 0.25, 0.2),
        Teacher::Iatros => Color::srgb(0.8, 0.78, 0.7),
        Teacher::Apothecary => Color::srgb(0.35, 0.45, 0.3),
        Teacher::Alchemist => Color::srgb(0.25, 0.2, 0.35),
        Teacher::Dyer => Color::srgb(0.55, 0.15, 0.2),
        Teacher::Glassblower | Teacher::Potter => Color::srgb(0.5, 0.35, 0.22),
        Teacher::SteppeHealer => Color::srgb(0.5, 0.4, 0.3),
    }
}

fn setup_lore_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let flower_mats = Substance::ALL.into_iter().filter(|s| s.is_wild()).map(|s| (s, materials.add(StandardMaterial { base_color: bloom_color(s), perceptual_roughness: 0.8, ..default() }))).collect();
    let robes = Teacher::ALL.into_iter().map(|t| (t, materials.add(StandardMaterial { base_color: robe_color(t), perceptual_roughness: 0.95, ..default() }))).collect();
    commands.insert_resource(LoreAssets {
        leaf: meshes.add(Cuboid::new(0.07, 0.32, 0.015)),
        flower: meshes.add(Sphere::new(0.035)),
        stem: meshes.add(Cylinder::new(0.008, 0.4)),
        leaf_mat: materials.add(StandardMaterial { base_color: Color::srgb(0.22, 0.42, 0.18), perceptual_roughness: 0.9, double_sided: true, cull_mode: None, ..default() }),
        flower_mats,
        body: meshes.add(Capsule3d::new(0.26, 1.05)),
        head: meshes.add(Sphere::new(0.13)),
        skin: materials.add(StandardMaterial { base_color: Color::srgb(0.72, 0.55, 0.42), perceptual_roughness: 0.8, ..default() }),
        robes,
        bronze: materials.add(StandardMaterial { base_color: Color::srgb(0.55, 0.36, 0.18), metallic: 0.85, perceptual_roughness: 0.42, ..default() }),
        coals: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.06, 0.04),
            emissive: LinearRgba::rgb(2.6, 0.7, 0.12),
            perceptual_roughness: 1.0,
            ..default()
        }),
        wood: materials.add(StandardMaterial { base_color: Color::srgb(0.27, 0.18, 0.11), perceptual_roughness: 0.92, ..default() }),
        iron: materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.24, 0.23), metallic: 0.7, perceptual_roughness: 0.55, ..default() }),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0)),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(0.5)),
    });
}

/// Куст: пучок листьев и цветков. Детерминированно по номеру, чтобы у всех одинаково.
fn spawn_patch(commands: &mut Commands, assets: &LoreAssets, p: &PatchView) -> Entity {
    let pos: Vec3 = p.pos.into();
    let mut rng = crate::items::Rng::new(p.id as u64 * 7919 + 13);
    // Одиночный кустик в поле — поменьше заросли.
    let scale = if p.id >= WILD_PATCH_BASE { 0.7 } else { 1.0 };
    let shown = if p.id >= WILD_PATCH_BASE && p.charges == 0 { Visibility::Hidden } else { Visibility::default() };
    commands
        .spawn((PatchModel { id: p.id }, Transform::from_translation(pos).with_scale(Vec3::splat(scale)), shown))
        .with_children(|c| {
            for i in 0..14 {
                let a = i as f32 * 2.4 + rng.f32();
                let r = rng.range_f32(0.03, 0.28);
                let tilt = rng.range_f32(0.15, 0.7);
                let h = rng.range_f32(0.6, 1.15);
                c.spawn((
                    Mesh3d(assets.leaf.clone()),
                    MeshMaterial3d(assets.leaf_mat.clone()),
                    Transform::from_xyz(a.cos() * r, 0.14 * h, a.sin() * r)
                        .with_rotation(Quat::from_rotation_y(-a) * Quat::from_rotation_x(tilt))
                        .with_scale(Vec3::new(1.0, h, 1.0)),
                ));
            }
            let Some(mat) = assets.flower_mats.get(&p.substance) else { return };
            for i in 0..7 {
                let a = i as f32 * 0.9 + rng.f32();
                let r = rng.range_f32(0.0, 0.22);
                let h = rng.range_f32(0.32, 0.5);
                c.spawn((Mesh3d(assets.stem.clone()), MeshMaterial3d(assets.leaf_mat.clone()), Transform::from_xyz(a.cos() * r, h * 0.5, a.sin() * r).with_scale(Vec3::new(1.0, h / 0.4, 1.0))));
                c.spawn((PatchBloom, Mesh3d(assets.flower.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(a.cos() * r, h, a.sin() * r)));
            }
        })
        .id()
}

/// Жаровня — медная чаша с углями на железной треноге; стол — дощатый, с жаровней, весами,
/// ступкой и подставкой под алембик. Всё из простых тел, свои модели.
fn spawn_station(commands: &mut Commands, assets: &LoreAssets, s: &StationView) -> Entity {
    let pos: Vec3 = s.pos.into();
    let part = |mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform| (Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t);
    let brazier = |c: &mut ChildSpawnerCommands, base: Vec3, k: f32| {
        // Ножки треноги, чуть врозь.
        for i in 0..3 {
            let a = i as f32 * std::f32::consts::TAU / 3.0;
            let off = Vec3::new(a.cos(), 0.0, a.sin()) * 0.17 * k;
            c.spawn(part(
                &assets.cylinder,
                &assets.iron,
                Transform::from_translation(base + off + Vec3::Y * 0.2 * k)
                    .with_rotation(Quat::from_axis_angle(Vec3::new(-a.sin(), 0.0, a.cos()), 0.22))
                    .with_scale(Vec3::new(0.03 * k, 0.42 * k, 0.03 * k)),
            ));
        }
        c.spawn(part(&assets.cylinder, &assets.bronze, Transform::from_translation(base + Vec3::Y * 0.44 * k).with_scale(Vec3::new(0.46 * k, 0.1 * k, 0.46 * k))));
        c.spawn(part(&assets.cylinder, &assets.bronze, Transform::from_translation(base + Vec3::Y * 0.38 * k).with_scale(Vec3::new(0.3 * k, 0.06 * k, 0.3 * k))));
        // Угли горкой.
        for (x, z, r) in [(0.0, 0.0, 0.13), (0.08, 0.05, 0.09), (-0.07, 0.06, 0.1), (0.02, -0.08, 0.09), (-0.06, -0.05, 0.08)] {
            c.spawn(part(&assets.ball, &assets.coals, Transform::from_translation(base + Vec3::new(x, 0.48, z) * k).with_scale(Vec3::new(r, r * 0.55, r) * k)));
        }
        c.spawn((
            PointLight { color: Color::srgb(1.0, 0.55, 0.25), intensity: 9000.0, range: 4.0, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(base + Vec3::Y * 0.7 * k),
            crate::graphics::Flicker { base: 9000.0, phase: base.x * 3.1 + k },
        ));
        // Пламя, искры и дымок над углями добавят эффекты.
        c.spawn((crate::alchemy_fx::BrazierFire { scale: k }, Transform::from_translation(base + Vec3::Y * 0.5 * k), Visibility::default()));
    };
    commands
        .spawn((Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(s.yaw)), Visibility::default()))
        .with_children(|c| match s.kind {
            StationKind::Brazier => brazier(c, Vec3::ZERO, 1.0),
            StationKind::Table => {
                let top = 0.82;
                c.spawn(part(&assets.cube, &assets.wood, Transform::from_xyz(0.0, top, 0.0).with_scale(Vec3::new(1.5, 0.06, 0.72))));
                for (x, z) in [(-0.68, -0.3), (0.68, -0.3), (-0.68, 0.3), (0.68, 0.3)] {
                    c.spawn(part(&assets.cube, &assets.wood, Transform::from_xyz(x, top * 0.5, z).with_scale(Vec3::new(0.07, top, 0.07))));
                }
                // Нижняя полка с кувшинами.
                c.spawn(part(&assets.cube, &assets.wood, Transform::from_xyz(0.0, 0.18, 0.0).with_scale(Vec3::new(1.36, 0.04, 0.6))));
                c.spawn(part(&assets.cylinder, &assets.bronze, Transform::from_xyz(-0.4, 0.3, 0.05).with_scale(Vec3::new(0.18, 0.22, 0.18))));
                // Жаровня на краю стола, меньше напольной.
                brazier(c, Vec3::new(0.45, top + 0.03, 0.0), 0.55);
                // Подставка под алембик над жаровней: железное кольцо на двух стойках.
                for z in [-0.17, 0.17] {
                    c.spawn(part(&assets.cylinder, &assets.iron, Transform::from_xyz(0.45, top + 0.3, z).with_scale(Vec3::new(0.02, 0.55, 0.02))));
                }
                c.spawn(part(&assets.cube, &assets.iron, Transform::from_xyz(0.45, top + 0.56, 0.0).with_scale(Vec3::new(0.02, 0.02, 0.36))));
                // Весы: стойка, коромысло, две чашки.
                c.spawn(part(&assets.cylinder, &assets.bronze, Transform::from_xyz(-0.25, top + 0.17, -0.12).with_scale(Vec3::new(0.02, 0.32, 0.02))));
                c.spawn(part(&assets.cube, &assets.bronze, Transform::from_xyz(-0.25, top + 0.32, -0.12).with_scale(Vec3::new(0.34, 0.012, 0.012))));
                for x in [-0.41, -0.09] {
                    c.spawn(part(&assets.cylinder, &assets.bronze, Transform::from_xyz(x, top + 0.14, -0.12).with_scale(Vec3::new(0.1, 0.012, 0.1))));
                }
                // Ступка с пестиком.
                c.spawn(part(&assets.cylinder, &assets.iron, Transform::from_xyz(-0.55, top + 0.07, 0.15).with_scale(Vec3::new(0.12, 0.1, 0.12))));
                c.spawn(part(&assets.cylinder, &assets.wood, Transform::from_xyz(-0.53, top + 0.15, 0.15).with_rotation(Quat::from_rotation_z(0.4)).with_scale(Vec3::new(0.022, 0.2, 0.022))));
            }
        })
        .id()
}

fn spawn_master(commands: &mut Commands, assets: &LoreAssets, m: &MasterView) -> Entity {
    let pos: Vec3 = m.pos.into();
    commands
        .spawn((Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(m.yaw)), Visibility::default()))
        .with_children(|c| {
            c.spawn((Mesh3d(assets.body.clone()), MeshMaterial3d(assets.robes[&m.teacher].clone()), Transform::from_xyz(0.0, 0.79, 0.0)));
            c.spawn((Mesh3d(assets.head.clone()), MeshMaterial3d(assets.skin.clone()), Transform::from_xyz(0.0, 1.72, 0.0)));
        })
        .id()
}

// ---------------------------------------------------------------------------
// Сеть
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn receive_lore(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut knowledge: ResMut<MyKnowledge>,
    mut view: ResMut<LoreView>,
    mut loot: ResMut<LootState>,
    mut body: ResMut<crate::medicine_client::MyBody>,
    mut fx: ResMut<crate::alchemy_fx::FxQueue>,
    mut tending: ResMut<MyTending>,
    assets: Res<LoreAssets>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    while let Some(bytes) = client.receive_message(CH_LORE) {
        let Some(msg) = decode::<LoreServerMessage>(&bytes) else { continue };
        match msg {
            LoreServerMessage::Knowledge(k) => {
                knowledge.0 = k;
                view.rev += 1;
                // Имена трав в сумке могли поменяться.
                loot.rev += 1;
            }
            LoreServerMessage::Patches(list) => {
                for (_, (_, e)) in view.patches.drain() {
                    commands.entity(e).despawn();
                }
                for p in list {
                    let e = if p.draw { spawn_patch(&mut commands, &assets, &p) } else { commands.spawn(Transform::from_translation(p.pos.into())).id() };
                    view.patches.insert(p.id, (p, e));
                }
            }
            LoreServerMessage::PatchMoved { id, pos, charges } => {
                if let Some((p, e)) = view.patches.get_mut(&id) {
                    p.pos = pos;
                    p.charges = charges;
                    commands.entity(*e).insert(Transform::from_translation(pos.into()).with_scale(Vec3::splat(0.7)));
                }
            }
            LoreServerMessage::Stations(list) => {
                for (_, (_, e)) in view.stations.drain() {
                    commands.entity(e).despawn();
                }
                for s in list {
                    let e = spawn_station(&mut commands, &assets, &s);
                    view.stations.insert(s.id, (s, e));
                }
            }
            LoreServerMessage::Work { player, work } => fx.works.push((player, work)),
            LoreServerMessage::Burst { pos, fx: kind } => fx.bursts.push((pos.into(), kind)),
            LoreServerMessage::PatchCharges { id, charges } => {
                if let Some((p, _)) = view.patches.get_mut(&id) {
                    p.charges = charges;
                }
            }
            LoreServerMessage::Masters(list) => {
                for (_, (_, e)) in view.masters.drain() {
                    commands.entity(e).despawn();
                }
                for m in list {
                    let e = spawn_master(&mut commands, &assets, &m);
                    view.masters.insert(m.id, (m, e));
                }
            }
            LoreServerMessage::Activity { verb, done, total } => loot.using = Some((verb, now - done, total)),
            LoreServerMessage::ActivityEnded => {
                loot.using = None;
                tending.0 = None;
            }
            LoreServerMessage::Tending(t) => tending.0 = Some(t),
            LoreServerMessage::Notice(text) => loot.notify(text, now),
            LoreServerMessage::Body(b) => {
                if body.0 != b {
                    body.0 = b;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Наведение и ввод
// ---------------------------------------------------------------------------

fn aim_lore_target(mut view: ResMut<LoreView>, loot: Res<LootState>, camera: Query<&GlobalTransform, With<PlayerCamera>>) {
    let target = (|| {
        if loot.ui_open || loot.target.is_some() || view.window_open {
            return None;
        }
        let cam = camera.single().ok()?;
        let (origin, dir) = (cam.translation(), cam.forward().as_vec3());
        let mut best: Option<(f32, LoreTarget)> = None;
        let mut test = |center: Vec3, radius: f32, t: LoreTarget| {
            let to = center - origin;
            let along = to.dot(dir);
            if !(0.0..=LORE_REACH + 1.2).contains(&along) {
                return;
            }
            let miss = (to - dir * along).length();
            if miss <= radius && best.is_none_or(|(s, _)| along + miss < s) {
                best = Some((along + miss, t));
            }
        };
        let mut zone: Option<(f32, LoreTarget)> = None;
        for (id, (p, _)) in &view.patches {
            if p.radius > 1.0 {
                // Заросль в городе: собирать, стоя в ней (если не смотрим на что-то ещё).
                let d = Vec3::from(p.pos) - origin;
                let flat = Vec2::new(d.x, d.z).length();
                if flat <= p.radius + 0.5 && d.y.abs() < 3.5 && zone.is_none_or(|(s, _)| flat < s) {
                    zone = Some((flat, LoreTarget::Patch(*id)));
                }
            } else if p.charges > 0 || *id < WILD_PATCH_BASE {
                test(Vec3::from(p.pos) + Vec3::Y * 0.25, 0.5, LoreTarget::Patch(*id));
            }
        }
        for (id, (s, _)) in &view.stations {
            let (h, r) = if s.kind == StationKind::Table { (0.85, 0.8) } else { (0.45, 0.45) };
            test(Vec3::from(s.pos) + Vec3::Y * h, r, LoreTarget::Station(*id));
        }
        for (id, (m, _)) in &view.masters {
            test(Vec3::from(m.pos) + Vec3::Y * 1.1, 0.6, LoreTarget::Master(*id));
        }
        best.or(zone).map(|(_, t)| t)
    })();
    if view.target != target {
        view.target = target;
    }
}

fn lore_input(keys: Res<ButtonInput<KeyCode>>, mut client: ResMut<RenetClient>, mut view: ResMut<LoreView>, loot: Res<LootState>, tending: Res<MyTending>) {
    // Огонь под сосудом: 1 — мехи, 2 — заслонка, 3 — мокрая тряпица на купол.
    if loot.using.is_some() && tending.0.is_some() {
        for (key, action) in [(KeyCode::Digit1, TendAction::Bellows), (KeyCode::Digit2, TendAction::Damper), (KeyCode::Digit3, TendAction::CoolDome)] {
            if keys.just_pressed(key) {
                send(&mut client, &LoreClientMessage::Tend(action));
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyK) && !loot.ui_open {
        view.window_open = !view.window_open;
        view.rev += 1;
    }
    if view.window_open && keys.just_pressed(KeyCode::Escape) {
        view.window_open = false;
        view.rev += 1;
    }
    if loot.ui_open {
        return;
    }
    // Любое движение прервёт занятие на сервере, но так отклик быстрее.
    if loot.using.is_some() && [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::Space].iter().any(|k| keys.just_pressed(*k)) {
        send(&mut client, &LoreClientMessage::Stop);
    }
    match view.target {
        Some(LoreTarget::Patch(patch)) if keys.just_pressed(KeyCode::KeyE) => send(&mut client, &LoreClientMessage::Gather { patch }),
        Some(LoreTarget::Master(master)) if keys.just_pressed(KeyCode::KeyE) => send(&mut client, &LoreClientMessage::PaidLesson { master }),
        Some(LoreTarget::Master(master)) if keys.just_pressed(KeyCode::KeyR) => send(&mut client, &LoreClientMessage::Apprentice { master }),
        Some(LoreTarget::Station(id)) if keys.just_pressed(KeyCode::KeyE) => send(&mut client, &LoreClientMessage::TakeStation { id }),
        Some(LoreTarget::Station(id)) if keys.just_pressed(KeyCode::KeyR) => send(&mut client, &LoreClientMessage::UpgradeStation { id }),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Подсказка и окно знаний
// ---------------------------------------------------------------------------

#[derive(Component)]
struct LorePrompt;

#[derive(Component)]
struct KnowledgeWindow;

/// Строка про огонь под сосудом.
#[derive(Component)]
struct TendingHud;

/// Знающий видит жар и нагрев купола; новичок — только слышит кипение и видит дым.
fn tending_lines(t: &TendView, k: &Knowledge) -> (String, bool) {
    let mut out = Vec::new();
    let reads_fire = k.knows(Lore::Technique(Technique::Decoction)) || k.knows(Lore::Technique(Technique::Distillation)) || k.skill(Skill::Alchemy) >= 25.0;
    let diff = t.fire - t.target;
    let mut danger = false;
    if reads_fire {
        let bar: String = (0..12).map(|i| if (i as f32 + 0.5) * FIRE_SCALE / 12.0 <= t.fire { '▮' } else { '▯' }).collect();
        let want = ["угли", "слабый", "средний", "сильный"][(t.target.round() as usize).min(3)];
        out.push(format!("Жар {bar}  (нужен {want})"));
    }
    let cue = if t.bath {
        if diff < -0.75 { "Вода в котле перестала кипеть" } else { "Вода в котле кипит ровно — баня держит жар" }
    } else if diff < -0.75 {
        "Угли едва тлеют, варево затихло"
    } else if diff > 0.75 {
        danger = true;
        "Клокочет, тянет горелым, дым чернеет!"
    } else if t.target >= 2.5 {
        "Гудит сильный огонь, дым светлый"
    } else {
        "Ровно побулькивает, дымок светлый"
    };
    out.push(cue.to_string());
    if let Some(d) = t.dome {
        if k.knows(Lore::Technique(Technique::Distillation)) {
            let bar: String = (0..10).map(|i| if (i as f32 + 0.5) / 10.0 <= d { '▮' } else { '▯' }).collect();
            out.push(format!("Купол {bar}"));
        }
        let cue = match d {
            d if d < 0.4 => "Купол тёплый, капает ровно",
            d if d < 0.75 => "Купол горячий, стекло звенит",
            _ => {
                danger = true;
                "Купол раскалён, стекло потрескивает!"
            }
        };
        out.push(cue.to_string());
    }
    let mut keys = "[1] мехи   [2] заслонка".to_string();
    if t.dome.is_some() {
        keys.push_str("   [3] мокрая тряпица на купол");
    }
    out.push(keys);
    (out.join("\n"), danger)
}

/// Предел шкалы жара (раскалено сверх сильного).
const FIRE_SCALE: f32 = 3.6;

fn update_tending_hud(tending: Res<MyTending>, knowledge: Res<MyKnowledge>, loot: Res<LootState>, mut hud: Single<(&mut Text, &mut TextColor, &mut Node), With<TendingHud>>) {
    if !tending.is_changed() && !knowledge.is_changed() && !loot.is_changed() {
        return;
    }
    let (text, color, node) = &mut *hud;
    match tending.0.as_ref().filter(|_| loot.using.is_some()) {
        Some(t) => {
            let (line, danger) = tending_lines(t, &knowledge.0);
            text.0 = line;
            color.0 = if danger { Color::srgb(1.0, 0.45, 0.3) } else { Color::srgb(0.95, 0.88, 0.7) };
            node.display = Display::Flex;
        }
        None => node.display = Display::None,
    }
}

fn setup_lore_ui(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), bottom: px(150), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((
                TendingHud,
                Text::new(""),
                TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(17.0), ..default() },
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                TextShadow::default(),
                Node { display: Display::None, ..default() },
            ));
        });
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), top: percent(56), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((
                LorePrompt,
                Text::new(""),
                TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(20.0), ..default() },
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                TextShadow::default(),
            ));
        });
    commands.spawn((
        KnowledgeWindow,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            left: percent(8),
            top: percent(6),
            width: percent(84),
            max_height: percent(88),
            padding: UiRect::all(px(16)),
            column_gap: px(28),
            border: UiRect::all(px(2)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.09, 0.075, 0.06, 0.96)),
        BorderColor::all(Color::srgb(0.45, 0.36, 0.22)),
        GlobalZIndex(20),
    ));
}

fn update_lore_prompt(view: Res<LoreView>, knowledge: Res<MyKnowledge>, mut text: Single<&mut Text, With<LorePrompt>>) {
    let line = match view.target {
        Some(LoreTarget::Patch(id)) => view.patches.get(&id).map(|(p, _)| {
            let name = p.substance.item().map(|b| knowledge.0.item_name(b)).unwrap_or_default();
            if p.charges == 0 { format!("{name} (всё собрано, отрастает)") } else { format!("[E] Собрать: {name}") }
        }),
        Some(LoreTarget::Master(id)) => view.masters.get(&id).map(|(m, _)| {
            let next = knowledge.0.next_lesson(m.teacher);
            match next {
                Some(lore) => {
                    let mut s = format!("{}\n[E] Урок ({}): {}", m.teacher.name(), crate::economy::format_money(m.price), lore.name().to_lowercase());
                    let wage = if m.wage > 0 { format!(", платят {}", crate::economy::format_money(m.wage)) } else { String::new() };
                    s.push_str(&format!("\n[R] Работать подмастерьем{wage}"));
                    s
                }
                None => format!("{}: больше нечему учить", m.teacher.name()),
            }
        }),
        Some(LoreTarget::Station(id)) => view.stations.get(&id).map(|(s, _)| match s.kind {
            StationKind::Brazier => "Жаровня: варите рядом (нужен древесный уголь)\n[E] Забрать\n[R] Собрать алхимический стол (весы и ступка)".to_string(),
            StationKind::Table => "Алхимический стол: варите рядом (уголь для жаровни)\n[E] Забрать".to_string(),
        }),
        None => None,
    }
    .unwrap_or_default();
    if text.0 != line {
        text.0 = line;
    }
}

/// Пустые кусты «вянут»: цветы прячутся.
fn update_patch_looks(view: Res<LoreView>, mut patches: Query<(&PatchModel, &Children, &mut Visibility), Without<PatchBloom>>, mut blooms: Query<&mut Visibility, With<PatchBloom>>) {
    if !view.is_changed() {
        return;
    }
    for (pm, children, mut shown) in &mut patches {
        let full = view.patches.get(&pm.id).is_some_and(|(p, _)| p.charges > 0);
        // Сорванный кустик в поле исчезает совсем (вырастет в другом месте).
        if pm.id >= WILD_PATCH_BASE {
            let want = if full { Visibility::Inherited } else { Visibility::Hidden };
            if *shown != want {
                *shown = want;
            }
        }
        for c in children.iter() {
            if let Ok(mut v) = blooms.get_mut(c) {
                *v = if full { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

fn column(p: &mut ChildSpawnerCommands, fonts: &UiFonts, title: &str, lines: &[String]) {
    p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), flex_basis: percent(33), flex_grow: 1.0, ..default() }).with_children(|c| {
        c.spawn((
            Text::new(title),
            TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(19.0), ..default() },
            TextColor(Color::srgb(0.93, 0.83, 0.6)),
        ));
        let body = if lines.is_empty() { "—".to_string() } else { lines.join("\n") };
        c.spawn((
            Text::new(body),
            TextFont { font: fonts.regular.clone().into(), font_size: bevy::text::FontSize::Px(14.0), ..default() },
            TextColor(Color::srgb(0.86, 0.82, 0.75)),
        ));
    });
}

/// Строки окна знаний (отдельно — чтобы проверить тестом).
fn knowledge_lines(k: &Knowledge) -> [Vec<String>; 3] {
    let mut left = Vec::new();
    for s in Skill::ALL {
        left.push(format!("{}: {:.0}", s.name(), k.skill(s)));
    }
    left.push(String::new());
    for lang in [Language::Greek, Language::Latin] {
        let lore = Lore::Read(lang);
        let line = if k.knows(lore) {
            format!("Читаете: {}", lang.name())
        } else if let Some(p) = k.lessons.get(&lore) {
            format!("Учите грамоту ({}): {:.0}%", lang.name(), p * 100.0)
        } else {
            format!("Не читаете: {}", lang.name())
        };
        left.push(line);
    }
    let lessons: Vec<String> = k.lessons.iter().filter(|(l, _)| !matches!(l, Lore::Read(_))).map(|(l, p)| format!("{} — {:.0}%", l.name(), p * 100.0)).collect();
    if !lessons.is_empty() {
        left.push(String::new());
        left.push("Недоученное:".into());
        left.extend(lessons);
    }
    let books: Vec<String> = k.read.iter().map(|(t, p)| if *p >= 1.0 { format!("{} — прочитано", t.info().item) } else { format!("{} — {:.0}%", t.info().item, p * 100.0) }).collect();
    if !books.is_empty() {
        left.push(String::new());
        left.push("Книги:".into());
        left.extend(books);
    }

    let mut mid = Vec::new();
    for s in Substance::ALL {
        let id = k.identifies(s);
        let props = k.knows(Lore::Properties(s));
        if !(s.is_wild() && id) && !props {
            continue;
        }
        let info = s.info();
        let mut line = info.item.to_string();
        if s.is_wild() {
            line.push_str(&format!(" ({})", info.greek));
        }
        if props {
            line.push_str(&format!(" — {}", info.uses));
        } else {
            line.push_str(" — узнаёте, но свойства неизвестны");
        }
        mid.push(line);
    }

    let mut right = Vec::new();
    for t in Technique::ALL {
        if k.knows(Lore::Technique(t)) {
            right.push(format!("{} — {}", t.name(), t.description()));
        }
    }
    [left, mid, right]
}

fn update_knowledge_window(
    mut commands: Commands,
    mut view: ResMut<LoreView>,
    knowledge: Res<MyKnowledge>,
    fonts: Res<UiFonts>,
    mut window: Single<(Entity, &mut Node), With<KnowledgeWindow>>,
) {
    if view.built_rev == view.rev {
        return;
    }
    view.built_rev = view.rev;
    let (entity, ref mut node) = *window;
    node.display = if view.window_open { Display::Flex } else { Display::None };
    if !view.window_open {
        return;
    }
    let [left, mid, right] = knowledge_lines(&knowledge.0);
    commands.entity(entity).despawn_children().with_children(|p| {
        column(p, &fonts, "Навыки и грамота", &left);
        column(p, &fonts, "Травы и вещества", &mid);
        column(p, &fonts, "Приёмы (K — закрыть)", &right);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_window_lists_what_is_known() {
        let mut k = Knowledge::default();
        let [_, herbs, techniques] = knowledge_lines(&k);
        // Покупное без изученных свойств тоже не показываем.
        assert!(herbs.is_empty());
        assert!(techniques.is_empty());
        k.learn(Lore::Identify(Substance::Plantain));
        k.learn(Lore::Technique(Technique::Poultice));
        let [_, herbs, techniques] = knowledge_lines(&k);
        assert_eq!(herbs.len(), 1);
        assert!(herbs[0].contains("Подорожник"));
        assert_eq!(techniques.len(), 1);
    }
}

// ---------------------------------------------------------------------------
// Отладка: снимок экрана без рук (для проверки на сервере сборки)
// ---------------------------------------------------------------------------

/// `MRPG_SHOT=файл.png` — через `MRPG_SHOT_DELAY` секунд (по умолчанию 8) сохранить
/// снимок экрана и выйти. `MRPG_SHOT_UI=knowledge` — перед снимком открыть окно знаний,
/// `MRPG_SHOT_UI=brew` — открыть сумку и окно варки у горшка (нужен `MRPG_DEBUG_KIT=1`),
/// `MRPG_SHOT_UI=fx` — сосуды на огне и вспышки перед камерой, `MRPG_SHOT_PITCH=градусы` — наклон,
/// `MRPG_SHOT_UI=brazier|table` — поставить жаровню (и собрать стол), `MRPG_SHOT_YAW=градусы` —
/// повернуть камеру.
pub struct DevShotPlugin;

impl Plugin for DevShotPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("MRPG_SHOT").is_ok() {
            app.add_systems(Update, dev_shot);
        }
    }
}

fn dev_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut view: ResMut<LoreView>,
    mut player: Query<(&mut crate::player::LocalPlayer, &mut Transform)>,
    mut loot: ResMut<LootState>,
    mut brew: ResMut<crate::brewing_client::BrewState>,
    mut done: Local<u8>,
    mut placed: Local<u8>,
    mut client: Option<ResMut<RenetClient>>,
    mut fx: ResMut<crate::alchemy_fx::FxQueue>,
    mut exit: MessageWriter<AppExit>,
) {
    let delay: f32 = std::env::var("MRPG_SHOT_DELAY").ok().and_then(|s| s.parse().ok()).unwrap_or(8.0);
    let t = time.elapsed_secs();
    let ui = std::env::var("MRPG_SHOT_UI").unwrap_or_default();
    // `MRPG_SHOT_UI=fx` — выставить перед камерой сосуды на огне и показать вспышку и осколки.
    if ui == "fx"
        && let Some((p, tf)) = player.iter().next()
    {
        let fwd = Quat::from_rotation_y(p.yaw) * Vec3::NEG_Z;
        let right = Quat::from_rotation_y(p.yaw) * Vec3::X;
        let feet = Vec3::new(tf.translation.x, crate::level::ground_y(tf.translation.x, tf.translation.z).max(tf.translation.y - 1.7).min(tf.translation.y), tf.translation.z);
        let at = |side: f32, ahead: f32| feet + fwd * ahead + right * side;
        if *placed == 0 && t > delay - 6.0 {
            use crate::brewing::{Heat, Vessel};
            let rigs = [(Vessel::ClayPot, Heat::High, -0.85, 2.1, true), (Vessel::Alembic, Heat::Medium, -0.1, 2.4, true), (Vessel::Crucible, Heat::High, 0.6, 2.1, true), (Vessel::Mortar, Heat::None, 1.15, 1.9, false)];
            for (i, (vessel, heat, side, ahead, own_fire)) in rigs.into_iter().enumerate() {
                fx.works.push((900 + i as u64, Some(WorkView { vessel, heat, pos: at(side, ahead).into(), yaw: p.yaw + std::f32::consts::PI, own_fire })));
            }
            *placed = 1;
        }
        if *placed == 1 && t > delay - 0.7 {
            fx.bursts.push((at(2.2, 4.5), BurstFx::Ignition));
            *placed = 2;
        }
        if *placed == 2 && t > delay - 0.3 {
            fx.bursts.push((at(-1.6, 2.8), BurstFx::Shatter { glass: true }));
            *placed = 3;
        }
    }
    // `MRPG_SHOT_UI=tend` — поставить жаровню, сварить на ней тысячелистник и раздуть мехами.
    if ui == "tend"
        && let Some(client) = client.as_mut()
    {
        let find = |name: &str| loot.inventory.as_ref().and_then(|inv| inv.all_items().find(|i| i.info().name == name).map(|i| i.uid));
        if *placed == 0 && t > delay - 9.0 {
            if let Some(uid) = find("Жаровня") {
                send(client, &LoreClientMessage::PlaceStation { uid });
                *placed = 1;
            }
        }
        if *placed == 1 && t > delay - 6.0 {
            if let Some(vessel) = find("Глиняный горшок") {
                let spec = crate::brewing::BrewSpec { items: vec![("Тысячелистник".into(), 2)], water: 8, heat: crate::brewing::Heat::Low, hours: 15.0, bath: false };
                send(client, &LoreClientMessage::Brew { vessel, spec });
            }
            *placed = 2;
        }
        if *placed == 2 && t > delay - 1.5 {
            send(client, &LoreClientMessage::Tend(TendAction::Bellows));
            *placed = 3;
        }
    }
    // `MRPG_SHOT_UI=brazier|table` — поставить жаровню из набора (и собрать на ней стол).
    if (ui == "brazier" || ui == "table")
        && let Some(client) = client.as_mut()
    {
        if *placed == 0 && t > delay - 6.0 {
            let item = loot.inventory.as_ref().and_then(|inv| inv.all_items().find(|i| i.info().name == "Жаровня").cloned());
            if let Some(item) = item {
                send(client, &LoreClientMessage::PlaceStation { uid: item.uid });
                *placed = 1;
            }
        }
        if *placed == 1 && ui == "table" && t > delay - 3.5 {
            if let Some(id) = view.stations.keys().next().copied() {
                send(client, &LoreClientMessage::UpgradeStation { id });
            }
            *placed = 2;
        }
    }
    if *done == 0 && t > delay - 1.0 {
        if std::env::var("MRPG_SHOT_UI").as_deref() == Ok("brew") {
            loot.ui_open = true;
            loot.rev += 1;
            let pot = loot.inventory.as_ref().and_then(|inv| inv.all_items().find(|i| i.info().name == "Глиняный горшок").map(|i| i.uid));
            if let Some(uid) = pot {
                brew.open_vessel(uid, crate::brewing::Vessel::ClayPot);
            }
        }
        if std::env::var("MRPG_SHOT_UI").as_deref() == Ok("knowledge") {
            view.window_open = true;
            view.rev += 1;
        }
        if let Some(yaw) = std::env::var("MRPG_SHOT_YAW").ok().and_then(|s| s.parse::<f32>().ok()) {
            for (mut p, mut tf) in &mut player {
                if let Some(pitch) = std::env::var("MRPG_SHOT_PITCH").ok().and_then(|s| s.parse::<f32>().ok()) {
                    p.pitch = pitch.to_radians();
                }
                p.yaw = yaw.to_radians();
                tf.rotation = Quat::from_rotation_y(p.yaw);
            }
        }
        *done = 1;
    }
    if *done == 1 && t > delay {
        let path = std::env::var("MRPG_SHOT").unwrap();
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        *done = 2;
    }
    if *done == 2 && t > delay + 2.0 {
        exit.write(AppExit::Success);
    }
}
