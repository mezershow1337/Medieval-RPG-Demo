//! Клиент: предметы в мире двигаются по снимкам сервера, а свой предмет в руках — сразу,
//! по тому же физическому движку (`props.rs`), с мягкой подтяжкой к серверу.
//!
//! Управление (как в Skyrim): навёл прицел на предмет и **зажал ЛКМ** — предмет в руках и
//! тянется за взглядом со своей физикой; **отпустил** — летит дальше сам; **ПКМ** — бросок;
//! **колесо** — ближе/дальше. Удар мечом при этом не происходит: ЛКМ по предмету — захват,
//! мимо предмета (или во время замаха) — удар.

use crate::fight::LocalFighter;
use crate::loot_client::{GroundView, LootState, send};
use crate::loot_net::*;
use crate::player::{LocalPlayer, PlayerCamera};
use crate::props::*;
use crate::props_server::PropStatics;
use crate::protocol::decode;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::{RenetClient, client_connected};
use std::collections::HashMap;

pub struct PropsClientPlugin;

impl Plugin for PropsClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PropGrab::default())
            .insert_resource(PropStatics::default())
            .add_systems(Startup, setup_highlight)
            .add_systems(Update, receive_snapshots.run_if(client_connected))
            .add_systems(Update, (grab_input, predict_held, smooth_props, update_highlight).chain().after(receive_snapshots));
    }
}

/// Ответ сервера о товаре на прилавке.
#[derive(Clone, Debug)]
pub struct ShelfInfoData {
    pub price: u32,
    pub stock: u16,
    pub merchant: String,
    pub attention: String,
    pub pocketable: bool,
    pub refuse: Option<String>,
}

/// Что игрок делает с предметами руками.
#[derive(Resource, Default)]
pub struct PropGrab {
    /// Предмет под прицелом, который можно схватить прямо сейчас.
    pub aim: Option<LootId>,
    pub holding: Option<LootId>,
    pub dist: f32,
    /// Кто что держит (по данным сервера).
    pub holders: HashMap<LootId, u64>,
    pub shelf_info: HashMap<LootId, ShelfInfoData>,
    asked: HashMap<LootId, f32>,
    /// Своё предсказание предмета в руках.
    sim: Option<(LootId, PropBody, f32, Quat)>,
    acc: f32,
}

impl PropGrab {
    /// ЛКМ сейчас — захват, а не удар.
    pub fn blocks_attack(&self) -> bool {
        self.aim.is_some() || self.holding.is_some()
    }

    /// ПКМ сейчас — бросок, а не защита.
    pub fn blocks_guard(&self) -> bool {
        self.holding.is_some()
    }

    /// Пришло от сервера, кто держит предмет.
    pub fn set_holder(&mut self, id: LootId, holder: Option<u64>, me: Option<u64>) {
        match holder {
            Some(h) => {
                self.holders.insert(id, h);
            }
            None => {
                self.holders.remove(&id);
            }
        }
        if self.holding == Some(id) && holder.is_some_and(|h| Some(h) != me) || (self.holding == Some(id) && holder.is_none()) {
            self.holding = None;
            self.sim = None;
        }
    }

    pub fn forget(&mut self, id: LootId) {
        self.holders.remove(&id);
        self.shelf_info.remove(&id);
        self.asked.remove(&id);
        if self.holding == Some(id) {
            self.holding = None;
            self.sim = None;
        }
    }

    /// Спросить цену товара (не чаще раза в 3 с).
    pub fn ask_price(&mut self, client: &mut RenetClient, id: LootId, now: f32) {
        if self.asked.get(&id).is_some_and(|t| now - t < 3.0) {
            return;
        }
        self.asked.insert(id, now);
        send(client, &LootClientMessage::ShelfQuery { id });
    }
}

/// Куда движется предмет по данным сервера.
#[derive(Component)]
pub struct PropMotion {
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// Когда пришёл снимок.
    pub at: f32,
    pub asleep: bool,
}

fn receive_snapshots(
    mut client: ResMut<RenetClient>,
    time: Res<Time>,
    index: Res<crate::loot_client::LootIndex>,
    mut motions: Query<&mut PropMotion>,
) {
    let now = time.elapsed_secs();
    while let Some(bytes) = client.receive_message(CH_PROPS) {
        let Some(PropsSnapshot(list)) = decode::<PropsSnapshot>(&bytes) else { continue };
        for s in list {
            let Some(e) = index.0.get(&s.id) else { continue };
            let Ok(mut m) = motions.get_mut(*e) else { continue };
            m.pos = Vec3::from_array(s.pos);
            m.rot = s.rotation();
            m.vel = Vec3::from_array(s.vel);
            m.at = now;
            m.asleep = false;
        }
    }
}

/// Плавно ведём предметы к серверному положению (с упреждением по скорости).
fn smooth_props(time: Res<Time>, grab: Res<PropGrab>, mut items: Query<(&GroundView, &PropMotion, &mut Transform)>) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let k = 1.0 - (-dt * 18.0).exp();
    for (g, m, mut t) in &mut items {
        if grab.sim.as_ref().is_some_and(|s| s.0 == g.id) {
            continue;
        }
        let ahead = if m.asleep { 0.0 } else { (now - m.at).clamp(0.0, 0.1) };
        let target = m.pos + m.vel * ahead;
        if t.translation.distance(target) > 3.0 {
            t.translation = target;
            t.rotation = m.rot;
            continue;
        }
        t.translation = t.translation.lerp(target, k);
        t.rotation = t.rotation.slerp(m.rot, k);
    }
}

fn cursor_locked(c: &CursorOptions) -> bool {
    c.grab_mode != CursorGrabMode::None
}

#[allow(clippy::too_many_arguments)]
fn grab_input(
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    client: Option<ResMut<RenetClient>>,
    mut grab: ResMut<PropGrab>,
    loot: Res<LootState>,
    trade: Option<Res<crate::trade_client::TradeState>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    local: Query<(&LocalPlayer, &LocalFighter)>,
    items: Query<(&GroundView, &Transform)>,
    index: Res<crate::loot_client::LootIndex>,
) {
    let Some(mut client) = client else { return };
    let busy = loot.ui_open || trade.is_some_and(|t| t.window_open()) || !cursor_locked(&cursor);
    let Ok(cam) = camera.single() else { return };
    let Ok((player, fighter)) = local.single() else { return };
    let idle = matches!(fighter.fighter.action, crate::combat::Action::Idle) && !fighter.fighter.is_dead();

    // Что под прицелом и можно ли это схватить.
    grab.aim = None;
    if !busy
        && idle
        && grab.holding.is_none()
        && let Some(id) = loot.target
        && let Some((g, t)) = index.0.get(&id).and_then(|e| items.get(*e).ok())
        && t.translation.distance(cam.translation()) <= GRAB_RANGE
        && !grab.holders.contains_key(&g.id)
    {
        grab.aim = Some(g.id);
    }

    if let Some(id) = grab.holding {
        let dead = fighter.fighter.is_dead();
        if busy || dead || mouse.just_released(MouseButton::Left) || !mouse.pressed(MouseButton::Left) {
            send(&mut client, &LootClientMessage::Release);
            grab.holding = None;
            grab.sim = None;
        } else if mouse.just_pressed(MouseButton::Right) {
            send(&mut client, &LootClientMessage::Throw);
            // Предскажем бросок, чтобы предмет сорвался сразу.
            if let Some((_, body, _, _)) = grab.sim.as_mut() {
                let dir = cam.forward().as_vec3();
                body.vel += dir * throw_speed(body.mass, 0.0);
            }
            grab.holding = None;
            grab.sim = None;
        } else if scroll.delta.y.abs() > 0.0 {
            grab.dist = (grab.dist + scroll.delta.y.signum() * 0.15).clamp(HOLD_MIN, HOLD_MAX);
            send(&mut client, &LootClientMessage::HoldDistance { dist: grab.dist });
        }
        let _ = id;
        return;
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some(id) = grab.aim
        && let Some((g, t)) = index.0.get(&id).and_then(|e| items.get(*e).ok())
    {
        let dist = t.translation.distance(cam.translation()).clamp(HOLD_MIN, HOLD_MAX);
        send(&mut client, &LootClientMessage::Grab { id, dist });
        grab.holding = Some(id);
        grab.dist = dist;
        let mut body = PropBody::for_item(g.base, g.count, t.translation, t.rotation);
        body.wake();
        grab.sim = Some((id, body, player.yaw, t.rotation));
        grab.acc = 0.0;
    }
}

/// Свой предмет в руках двигаем сами (без задержки сети) и мягко сверяем с сервером.
#[allow(clippy::too_many_arguments)]
fn predict_held(
    time: Res<Time>,
    mut grab: ResMut<PropGrab>,
    level: Res<crate::collision::LevelColliders>,
    statics: Res<PropStatics>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    local: Query<&LocalPlayer>,
    index: Res<crate::loot_client::LootIndex>,
    mut items: Query<(&PropMotion, &mut Transform)>,
) {
    let grab = &mut *grab;
    let Some((id, body, yaw0, rot0)) = grab.sim.as_mut() else { return };
    let (Ok(cam), Ok(player)) = (camera.single(), local.single()) else { return };
    let Some(Ok((m, mut t))) = index.0.get(id).map(|e| items.get_mut(*e)) else { return };
    let eye = cam.translation();
    let hold = Hold {
        target: eye + cam.forward().as_vec3() * grab.dist,
        target_rot: Quat::from_rotation_y(player.yaw - *yaw0) * *rot0,
        max_force: hand_force(0.0),
    };
    let env = Env { level: &level, statics: &statics.0, movers: &[] };
    grab.acc += time.delta_secs().min(0.1);
    while grab.acc >= TICK {
        grab.acc -= TICK;
        body.step(&env, Some(&hold), TICK);
    }
    // Сервер прав: подтягиваемся к нему понемногу (быстрее, если разошлись далеко).
    let off = m.pos.distance(body.pos);
    let k = if off > 1.0 { 0.5 } else { 0.06 };
    body.pos = body.pos.lerp(m.pos, k);
    body.rot = body.rot.slerp(m.rot, k * 0.5);
    t.translation = body.pos;
    t.rotation = body.rot;
}

#[derive(Component)]
struct Highlight;

fn setup_highlight(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        Highlight,
        Mesh3d(meshes.add(Cuboid::new(2.0, 2.0, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.8, 0.35, 0.10),
            emissive: LinearRgba::rgb(0.5, 0.35, 0.1),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
        bevy::light::NotShadowCaster,
    ));
}

/// Предмет под прицелом подсвечивается мягким золотистым «ореолом» (товар — всегда).
fn update_highlight(
    loot: Res<LootState>,
    grab: Res<PropGrab>,
    index: Res<crate::loot_client::LootIndex>,
    items: Query<(&GroundView, &Transform), Without<Highlight>>,
    mut hl: Single<(&mut Transform, &mut Visibility), With<Highlight>>,
) {
    let (ref mut t, ref mut vis) = *hl;
    let id = grab.holding.or(loot.target);
    let Some((g, it)) = id.and_then(|id| index.0.get(&id)).and_then(|e| items.get(*e).ok()) else {
        **vis = Visibility::Hidden;
        return;
    };
    **vis = Visibility::Visible;
    t.translation = it.translation;
    t.rotation = it.rotation;
    t.scale = g.half + Vec3::splat(0.015);
}
