//! Двери: каждая дверь уровня открывается и закрывается на «E», состояние общее для всех игроков.
//!
//! Где стоят двери, знают и сервер, и клиент: уровень строится одинаково (`level::Layout::doors`),
//! номер двери — её индекс. Коллизия закрытой створки живёт в `LevelColliders` (флаг открытости
//! меняется без перестройки сетки), поэтому двери останавливают и игроков, и NPC, и стрелы.
//! Сервер — источник правды: клиент просит «открой/закрой», сервер проверяет расстояние и что
//! в проёме никто не стоит, и рассылает новое состояние.

use crate::collision::{Body, LevelColliders};
use crate::level::DoorSpec;
use crate::protocol::{decode, encode};
use bevy::prelude::*;
use bevy_renet::{RenetClient, RenetServer, RenetServerEvent, renet::ServerEvent};
use serde::{Deserialize, Serialize};

/// Канал renet для дверей (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_DOOR: u8 = 8;

/// Дотянуться до двери можно с этого расстояния (от глаз до проёма, метры).
pub const DOOR_REACH: f32 = 2.6;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DoorClientMessage {
    /// Открыть закрытую или закрыть открытую.
    Toggle { id: u32 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DoorServerMessage {
    /// Все двери при подключении: бит на дверь (1 — открыта).
    All(Vec<u8>),
    Set { id: u32, open: bool },
}

pub fn pack(level: &LevelColliders) -> Vec<u8> {
    let n = level.door_count();
    let mut bits = vec![0u8; n.div_ceil(8)];
    for id in 0..n {
        if level.door_open(id) {
            bits[id / 8] |= 1 << (id % 8);
        }
    }
    bits
}

pub fn unpack(level: &LevelColliders, bits: &[u8]) {
    for id in 0..level.door_count() {
        let open = bits.get(id / 8).is_some_and(|b| b & (1 << (id % 8)) != 0);
        level.set_door_open(id, open);
    }
}

/// Расстояние от точки до проёма двери (до коробки закрытой створки).
pub fn distance_to_door(level: &LevelColliders, id: usize, p: Vec3) -> f32 {
    let Some(b) = level.door_box(id) else { return f32::MAX };
    let q = p.clamp(b.min, b.max);
    p.distance(q)
}

// ---------------------------------------------------------------------------
// Сервер
// ---------------------------------------------------------------------------

pub struct DoorServerPlugin;

impl Plugin for DoorServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_connection).add_systems(Update, receive_door_messages);
    }
}

fn on_connection(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, level: Res<LevelColliders>) {
    if let ServerEvent::ClientConnected { client_id } = &event.0 {
        server.send_message(*client_id, CH_DOOR, encode(&DoorServerMessage::All(pack(&level))));
    }
}

fn receive_door_messages(mut server: ResMut<RenetServer>, game: Res<crate::server::GameWorld>, level: Res<LevelColliders>, mut locks: Option<ResMut<crate::towns_server::Locks>>, shutters: Option<Res<crate::shop_hours::ShopShutters>>) {
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_DOOR) {
            let Some(DoorClientMessage::Toggle { id }) = decode::<DoorClientMessage>(&bytes) else { continue };
            let id = id as usize;
            let Some(actor) = game.actors.get(&pid).filter(|a| a.alive) else { continue };
            if id >= level.door_count() {
                continue;
            }
            let eye = actor.pose.feet + Vec3::Y * 1.6;
            // Небольшой запас на задержку сети.
            if distance_to_door(&level, id, eye) > DOOR_REACH + 0.8 {
                continue;
            }
            // Чужой дом заперт, недостроенный — без дверей.
            if let Some(locks) = locks.as_mut()
                && !locks.door(id as u32, pid)
            {
                server.send_message(pid, CH_DOOR, encode(&DoorServerMessage::Set { id: id as u32, open: level.door_open(id) }));
                continue;
            }
            // Ставни лавки на ночь заперты снаружи (изнутри выйти можно).
            if let Some(Err(why)) = shutters.as_ref().map(|s| s.may_toggle(id as u32, actor.pose.feet)) {
                if let Some(locks) = locks.as_mut() {
                    locks.denied.push((pid, why));
                }
                server.send_message(pid, CH_DOOR, encode(&DoorServerMessage::Set { id: id as u32, open: level.door_open(id) }));
                continue;
            }
            let open = !level.door_open(id);
            if !open {
                // Не захлопываем дверь на ком-то, кто стоит в проёме.
                let b = level.door_box(id).unwrap();
                let blocked = game.actors.values().any(|a| a.alive && Body::STANDING.aabb_at(a.pose.feet).overlaps(&b));
                if blocked {
                    server.send_message(pid, CH_DOOR, encode(&DoorServerMessage::Set { id: id as u32, open: true }));
                    continue;
                }
            }
            level.set_door_open(id, open);
            server.broadcast_message(CH_DOOR, encode(&DoorServerMessage::Set { id: id as u32, open }));
        }
    }
}

// ---------------------------------------------------------------------------
// Клиент
// ---------------------------------------------------------------------------

pub struct DoorClientPlugin;

impl Plugin for DoorClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DoorAim::default())
            .add_systems(Startup, setup_door_prompt.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (receive_doors, door_input).chain().run_if(bevy_renet::client_connected))
            .add_systems(Update, (aim_door, swing_doors, update_door_prompt).chain().in_set(DoorSystems));
    }
}

/// Наведение на двери (после наведения на жителей: у них своё «E»).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DoorSystems;

/// На какую дверь смотрит игрок.
#[derive(Resource, Default)]
pub struct DoorAim(pub Option<u32>);

/// Створка: поворачивается вокруг петли (сущность стоит на оси петли).
#[derive(Component)]
pub struct DoorLeaf {
    pub id: u32,
    /// Куда поворачивается при открытии (+1/−1 вокруг Y).
    pub sign: f32,
    pub angle: f32,
}

/// Сколько градусов открывается створка.
const OPEN_ANGLE: f32 = 1.75;

/// Клиент: ставит створки во все проёмы (вызывается при постройке уровня).
pub fn spawn_door_leaves(commands: &mut Commands, meshes: &mut Assets<Mesh>, wood: Handle<crate::graphics::SurfaceMaterial>, iron: Handle<crate::graphics::SurfaceMaterial>, doors: &[DoorSpec]) {
    // Одна створка 1 × 1 м (масштабом растягивается под проём): доски, две оковки, кольцо.
    let mut planks = crate::level::MeshBuilder::default();
    let mut metal = crate::level::MeshBuilder::default();
    let n = 5;
    for k in 0..n {
        let x0 = k as f32 / n as f32;
        let x1 = (k + 1) as f32 / n as f32 - 0.008;
        planks.cuboid(Vec3::new(x0, 0.0, -0.035), Vec3::new(x1, 1.0, 0.035));
    }
    // Поперечные планки с обеих сторон.
    for y in [0.15, 0.82] {
        planks.cuboid(Vec3::new(0.04, y, 0.035), Vec3::new(0.96, y + 0.07, 0.06));
        metal.cuboid(Vec3::new(0.0, y + 0.02, -0.05), Vec3::new(0.75, y + 0.05, -0.035));
    }
    metal.cuboid(Vec3::new(0.86, 0.46, -0.07), Vec3::new(0.9, 0.52, 0.07));
    let leaf_mesh = meshes.add(planks.build());
    let metal_mesh = meshes.add(metal.build());
    for (id, d) in doors.iter().enumerate() {
        let leaves: &[(f32, f32)] = if d.double { &[(-1.0, 0.5), (1.0, 0.5)] } else { &[(-1.0, 1.0)] };
        for &(side, frac) in leaves {
            // Ось вдоль стены и поперёк неё.
            let (along, across) = if d.along_x { (Vec3::X, Vec3::Z) } else { (Vec3::Z, Vec3::X) };
            let hinge = d.center + along * side * d.width / 2.0;
            let leaf_w = d.width * frac - 0.02;
            // Створка лежит от петли к середине проёма: локальный +X — от петли.
            let dir = -along * side;
            let base_yaw = (-dir.z).atan2(dir.x);
            let base = Quat::from_rotation_y(base_yaw);
            // Открывается внутрь той стороны, куда смотрит `across` (одинаково у всех дверей уровня).
            let sign = if (base * Vec3::Z).dot(across) > 0.0 { -1.0 } else { 1.0 };
            let angle = if d.open { OPEN_ANGLE } else { 0.0 };
            commands
                .spawn((
                    DoorLeaf { id: id as u32, sign, angle },
                    Transform::from_translation(hinge).with_rotation(base * Quat::from_rotation_y(sign * angle)),
                    Visibility::default(),
                ))
                .with_children(|p| {
                    let scale = Vec3::new(leaf_w, d.height - 0.03, 1.0);
                    p.spawn((Mesh3d(leaf_mesh.clone()), MeshMaterial3d(wood.clone()), Transform::from_scale(scale)));
                    p.spawn((Mesh3d(metal_mesh.clone()), MeshMaterial3d(iron.clone()), Transform::from_scale(scale)));
                });
        }
    }
}

fn receive_doors(mut client: ResMut<RenetClient>, level: Res<LevelColliders>) {
    while let Some(bytes) = client.receive_message(CH_DOOR) {
        match decode::<DoorServerMessage>(&bytes) {
            Some(DoorServerMessage::All(bits)) => unpack(&level, &bits),
            Some(DoorServerMessage::Set { id, open }) => level.set_door_open(id as usize, open),
            None => {}
        }
    }
}

fn swing_doors(time: Res<Time>, level: Res<LevelColliders>, mut leaves: Query<(&mut DoorLeaf, &mut Transform)>) {
    let step = time.delta_secs() * 2.6;
    for (mut leaf, mut t) in &mut leaves {
        let target = if level.door_open(leaf.id as usize) { OPEN_ANGLE } else { 0.0 };
        if (leaf.angle - target).abs() < 1e-4 {
            continue;
        }
        let delta = (target - leaf.angle).clamp(-step, step);
        leaf.angle += delta;
        t.rotate_local_y(leaf.sign * delta);
    }
}

#[allow(clippy::too_many_arguments)]
fn aim_door(
    mut aim: ResMut<DoorAim>,
    level: Res<LevelColliders>,
    loot: Res<crate::loot_client::LootState>,
    trade: Res<crate::trade_client::TradeState>,
    lore: Res<crate::lore_client::LoreView>,
    resident: Res<crate::residents::ResidentAim>,
    camera: Query<&GlobalTransform, With<crate::player::PlayerCamera>>,
) {
    aim.0 = None;
    // Тела, вещи, торговцы и травы важнее: у них своё «E».
    if loot.ui_open || loot.target.is_some() || trade.window_open() || trade.aim.is_some() || lore.target.is_some() || resident.0.is_some() {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let origin = cam.translation();
    let dir = cam.forward().as_vec3();
    let mut best: Option<(f32, u32)> = None;
    for id in 0..level.door_count() {
        let Some(b) = level.door_box(id) else { continue };
        // Быстрый отсев по расстоянию.
        let c = (b.min + b.max) / 2.0;
        if Vec2::new(c.x - origin.x, c.z - origin.z).length_squared() > 36.0 {
            continue;
        }
        // Луч против коробки проёма, чуть утолщённой (за открытую створку тоже можно взяться).
        let pad = Vec3::new(0.25, 0.0, 0.25);
        if let Some(t) = ray_box(origin, dir, b.min - pad, b.max + pad)
            && t <= DOOR_REACH
            && best.is_none_or(|(bt, _)| t < bt)
        {
            best = Some((t, id as u32));
        }
    }
    aim.0 = best.map(|(_, id)| id);
}

/// Пересечение луча с коробкой: расстояние до входа (или 0, если начало внутри).
fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inv = dir.recip();
    let t0 = (min - origin) * inv;
    let t1 = (max - origin) * inv;
    let near = t0.min(t1).max_element();
    let far = t0.max(t1).min_element();
    (far >= near.max(0.0)).then_some(near.max(0.0))
}

fn door_input(keys: Res<ButtonInput<KeyCode>>, aim: Res<DoorAim>, mut client: ResMut<RenetClient>, loot: Res<crate::loot_client::LootState>) {
    if loot.ui_open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyE)
        && let Some(id) = aim.0
    {
        client.send_message(CH_DOOR, encode(&DoorClientMessage::Toggle { id }));
    }
}

#[derive(Component)]
struct DoorPrompt;

fn setup_door_prompt(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>) {
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), top: percent(56), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            crate::ui_theme::theme().plaque(p).with_children(|p| {
                p.spawn((
                    DoorPrompt,
                    Text::new(""),
                    TextFont {
                        font: fonts.bold.clone().into(),
                        font_size: bevy::text::FontSize::Px(20.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    TextShadow::default(),
                    crate::ui_theme::Plaque,
                ));
            });
        });
}

fn update_door_prompt(aim: Res<DoorAim>, level: Res<LevelColliders>, mut text: Single<&mut Text, With<DoorPrompt>>) {
    let line = match aim.0 {
        Some(id) if level.door_open(id as usize) => "[E] Закрыть дверь",
        Some(_) => "[E] Открыть дверь",
        None => "",
    };
    if text.0 != line {
        text.0 = line.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_round_trip() {
        let layout = crate::level::layout();
        assert!(layout.doors.len() > 300, "мало дверей: {}", layout.doors.len());
        let level = crate::level::colliders(layout);
        for id in (0..level.door_count()).step_by(3) {
            level.set_door_open(id, !level.door_open(id));
        }
        let bits = pack(&level);
        let other = crate::level::level_colliders();
        unpack(&other, &bits);
        for id in 0..level.door_count() {
            assert_eq!(level.door_open(id), other.door_open(id));
        }
    }

    #[test]
    fn ray_hits_box() {
        assert!(ray_box(Vec3::ZERO, Vec3::X, Vec3::new(1.0, -1.0, -1.0), Vec3::new(2.0, 1.0, 1.0)).is_some_and(|t| (t - 1.0).abs() < 1e-4));
        assert!(ray_box(Vec3::ZERO, -Vec3::X, Vec3::new(1.0, -1.0, -1.0), Vec3::new(2.0, 1.0, 1.0)).is_none());
    }
}
