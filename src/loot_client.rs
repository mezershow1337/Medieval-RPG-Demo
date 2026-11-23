//! Клиент: тела и предметы на земле, наведение и «E», приём состояния инвентаря от сервера.

use crate::client::CorpseMarker;
use crate::collision::LevelColliders;
use crate::inventory::Inventory;
use crate::inventory_ui::{InventoryUiPlugin, UiFonts};
use crate::icons::ItemModels;
use crate::items::{ItemBaseId, Quality};
use crate::loot_net::*;
use crate::player::PlayerCamera;
use crate::protocol::{decode, encode};
use bevy::prelude::*;
use bevy_renet::{RenetClient, client_connected};
use std::collections::HashMap;

pub struct LootClientPlugin;

impl Plugin for LootClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InventoryUiPlugin)
            .insert_resource(LootState::default())
            .insert_resource(LootIndex::default())
            .insert_resource(PendingCorpses::default())
            .add_systems(Startup, (setup_loot_assets, crate::icons::setup_models, setup_prompt, setup_use_bar).chain().after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (receive_loot, attach_corpse_views, interact_input).chain().run_if(client_connected))
            .add_plugins(crate::props_client::PropsClientPlugin)
            .add_systems(Update, (aim_target, update_prompt, update_notices, update_use_bar).chain());
    }
}

/// Открытое тело.
pub struct OpenContainer {
    pub id: LootId,
    pub title: String,
    pub inv: Inventory,
}

#[derive(Resource, Default)]
pub struct LootState {
    /// Свой инвентарь (приходит с сервера).
    pub inventory: Option<Inventory>,
    pub container: Option<OpenContainer>,
    /// Открыто ли окно инвентаря.
    pub ui_open: bool,
    /// На что смотрит игрок (тело или предмет в пределах досягаемости).
    pub target: Option<LootId>,
    /// Сообщения игроку и время их появления.
    pub notices: Vec<(String, f32)>,
    /// Что сейчас применяем: подпись, время начала, длительность.
    pub using: Option<(String, f32, f32)>,
    /// Растёт при каждом изменении инвентаря или тела (окно перестраивается).
    pub rev: u64,
}

impl LootState {
    pub fn notify(&mut self, text: impl Into<String>, now: f32) {
        self.notices.push((text.into(), now));
        if self.notices.len() > 4 {
            self.notices.remove(0);
        }
    }
}

pub fn send(client: &mut RenetClient, msg: &LootClientMessage) {
    client.send_message(CH_LOOT, encode(msg));
}

#[derive(Resource, Default)]
pub struct LootIndex(pub HashMap<LootId, Entity>);

/// Сведения о телах боевой системы, к которым ещё не привязали обыск.
#[derive(Resource, Default)]
struct PendingCorpses(HashMap<LootId, CorpseView>);

/// Тело боевой системы (`CorpseMarker`) получает возможность обыска.
fn attach_corpse_views(mut commands: Commands, mut pending: ResMut<PendingCorpses>, markers: Query<(Entity, &CorpseMarker), Without<CorpseView>>) {
    for (e, marker) in &markers {
        if let Some(view) = pending.0.remove(&marker.corpse_id) {
            commands.entity(e).insert(view);
        }
    }
}

#[derive(Component, Clone)]
pub struct CorpseView {
    pub id: LootId,
    pub name: String,
    pub empty: bool,
}

#[derive(Component)]
pub struct GroundView {
    pub id: LootId,
    pub base: ItemBaseId,
    pub quality: Quality,
    pub count: u16,
    /// Половинные размеры тела (для прицела и подсветки).
    pub half: Vec3,
    /// Товар на прилавке этого торговца.
    pub shelf: Option<crate::trade::MerchantId>,
}

#[derive(Resource)]
struct LootAssets {
    corpse_body: Handle<Mesh>,
    corpse_head: Handle<Mesh>,
    blood: Handle<Mesh>,
    corpse_mat: Handle<StandardMaterial>,
    corpse_empty_mat: Handle<StandardMaterial>,
    blood_mat: Handle<StandardMaterial>,
    /// Светящийся круг под ценной вещью и его материалы по качеству.
    halo: Handle<Mesh>,
    halo_mat: Vec<Handle<StandardMaterial>>,
    beam: Handle<Mesh>,
    beam_mat: Vec<Handle<StandardMaterial>>,
}

fn quality_color(q: Quality) -> Color {
    let [r, g, b] = q.color();
    Color::srgb(r, g, b)
}

fn setup_loot_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let glow_mat = |materials: &mut Assets<StandardMaterial>, q: Quality, alpha: f32, power: f32| {
        let c = quality_color(q).to_linear();
        materials.add(StandardMaterial {
            base_color: quality_color(q).with_alpha(alpha),
            emissive: LinearRgba::rgb(c.red * power, c.green * power, c.blue * power),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        })
    };
    let halo_mat = Quality::ALL.iter().map(|q| glow_mat(&mut materials, *q, 0.12, [0.0, 0.0, 0.6, 1.0, 1.6, 2.4][q.index()])).collect();
    let beam_mat = Quality::ALL.iter().map(|q| glow_mat(&mut materials, *q, 0.25, 3.0)).collect();
    commands.insert_resource(LootAssets {
        corpse_body: meshes.add(Capsule3d::new(0.28, 1.0)),
        corpse_head: meshes.add(Sphere::new(0.2)),
        blood: meshes.add(Cylinder::new(0.75, 0.01)),
        corpse_mat: materials.add(StandardMaterial { base_color: Color::srgb(0.42, 0.42, 0.45), metallic: 0.6, perceptual_roughness: 0.45, ..default() }),
        corpse_empty_mat: materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.24, 0.24), perceptual_roughness: 0.9, ..default() }),
        blood_mat: materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.02, 0.02), perceptual_roughness: 0.3, ..default() }),
        halo: meshes.add(Circle::new(0.32)),
        halo_mat,
        beam: meshes.add(Cylinder::new(0.02, 1.6)),
        beam_mat,
    });
}

/// Предмет на земле: его 3D-модель (та же, что на иконке) и подсветка по качеству.
#[allow(clippy::too_many_arguments)]
fn spawn_item_model(p: &mut ChildSpawnerCommands, assets: &LootAssets, models: &ItemModels, base: ItemBaseId, quality: Quality, offset: Vec3, shelf: bool, finish: u8) {
    let mut m = p.spawn((WorldAssetRoot(models.get(base)), Transform::from_translation(offset)));
    if finish < 160 {
        m.insert(crate::rig::LookTint(finish));
    }
    // Товар на прилавке не светится: его и так видно, а луч над столом выглядел бы странно.
    if shelf {
        return;
    }
    // Добротные и лучше слегка светятся снизу, редкие — ещё и лучом, чтобы их было видно издалека.
    if quality >= Quality::Uncommon {
        p.spawn((
            Mesh3d(assets.halo.clone()),
            MeshMaterial3d(assets.halo_mat[quality.index()].clone()),
            Transform::from_translation(offset + Vec3::Y * 0.004).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            bevy::light::NotShadowCaster,
        ));
    }
    if quality >= Quality::Rare {
        p.spawn((Mesh3d(assets.beam.clone()), MeshMaterial3d(assets.beam_mat[quality.index()].clone()), Transform::from_translation(offset + Vec3::Y * 0.8), bevy::light::NotShadowCaster));
    }
}

#[allow(clippy::too_many_arguments)]
fn receive_loot(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut state: ResMut<LootState>,
    mut index: ResMut<LootIndex>,
    assets: Res<LootAssets>,
    models: Res<ItemModels>,
    mut corpses: Query<(&mut CorpseView, Option<&Children>)>,
    mut pending: ResMut<PendingCorpses>,
    mut mats: Query<&mut MeshMaterial3d<StandardMaterial>>,
    time: Res<Time>,
    mut grab: ResMut<crate::props_client::PropGrab>,
    mut motions: Query<&mut crate::props_client::PropMotion>,
    net: Option<Res<crate::client::NetState>>,
) {
    let me = net.as_ref().and_then(|n| n.my_id);
    let now = time.elapsed_secs();
    while let Some(bytes) = client.receive_message(CH_LOOT) {
        let Some(msg) = decode::<LootServerMessage>(&bytes) else { continue };
        match msg {
            LootServerMessage::Inventory(inv) => {
                state.inventory = Some(inv);
                state.rev += 1;
            }
            LootServerMessage::ContainerOpened { id, title, inv } => {
                state.container = Some(OpenContainer { id, title, inv });
                state.ui_open = true;
                state.rev += 1;
            }
            LootServerMessage::ContainerUpdated { id, inv } => {
                if let Some(c) = state.container.as_mut().filter(|c| c.id == id) {
                    c.inv = inv;
                    state.rev += 1;
                }
            }
            LootServerMessage::ContainerClosed => {
                state.container = None;
                state.rev += 1;
            }
            LootServerMessage::CorpseSpawned { id, pos, yaw, name, empty, own_model } => {
                if id >= CONTAINER_ID_BASE {
                    // Сундук уровня: его рисует уровень, здесь — только точка для наведения.
                    let e = commands.spawn((CorpseView { id, name, empty }, Transform::from_translation(pos.into()))).id();
                    index.0.insert(id, e);
                    continue;
                }
                if !own_model {
                    // Модель рисует боевая система — обыск привяжем к её телу.
                    pending.0.insert(id, CorpseView { id, name, empty });
                    continue;
                }
                let pos: Vec3 = pos.into();
                let mat = if empty { assets.corpse_empty_mat.clone() } else { assets.corpse_mat.clone() };
                let e = commands
                    .spawn((CorpseView { id, name, empty }, Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()))
                    .with_children(|p| {
                        // Лежит на спине вдоль оси Z.
                        p.spawn((
                            Mesh3d(assets.corpse_body.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(0.0, 0.28, 0.1).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                        ));
                        p.spawn((Mesh3d(assets.corpse_head.clone()), MeshMaterial3d(mat), Transform::from_xyz(0.0, 0.22, -0.75)));
                        p.spawn((Mesh3d(assets.blood.clone()), MeshMaterial3d(assets.blood_mat.clone()), Transform::from_xyz(0.1, 0.006, -0.3)));
                    })
                    .id();
                index.0.insert(id, e);
            }
            LootServerMessage::CorpseEmpty { id, empty } => {
                if let Some(p) = pending.0.get_mut(&id) {
                    p.empty = empty;
                }
                for (mut view, _) in &mut corpses {
                    if view.id == id {
                        view.empty = empty;
                    }
                }
                // Своя модель: тусклый цвет у обысканного тела.
                let Some(e) = index.0.get(&id) else { continue };
                let Ok((_, Some(children))) = corpses.get_mut(*e) else { continue };
                let mat = if empty { assets.corpse_empty_mat.clone() } else { assets.corpse_mat.clone() };
                // Тело и голова — первые два ребёнка.
                for child in children.iter().take(2) {
                    if let Ok(mut m) = mats.get_mut(child) {
                        m.0 = mat.clone();
                    }
                }
            }
            LootServerMessage::GroundSpawned { id, pos, rot, vel, base, quality, count, asleep, shelf, finish } => {
                let pos: Vec3 = pos.into();
                let rot = Quat::from_array(rot).normalize();
                let body = crate::props::PropBody::for_item(base, count, pos, rot);
                let (half, offset) = (body.half, body.model_offset());
                // Тот же предмет пересоздаётся (украли с прилавка, часть стопки подобрали): не прыгаем.
                let start = index.0.get(&id).and_then(|e| motions.get(*e).ok()).map(|_| pos).unwrap_or(pos);
                let e = commands
                    .spawn((
                        GroundView { id, base, quality, count, half, shelf: shelf.map(|s| s.merchant) },
                        crate::props_client::PropMotion { pos, rot, vel: vel.into(), at: now, asleep },
                        Transform::from_translation(start).with_rotation(rot),
                        Visibility::default(),
                    ))
                    .with_children(|p| spawn_item_model(p, &assets, &models, base, quality, offset, shelf.is_some(), finish))
                    .id();
                if let Some(old) = index.0.insert(id, e) {
                    commands.entity(old).despawn();
                }
            }
            LootServerMessage::PropRest { id, pos, rot } => {
                if let Some(mut m) = index.0.get(&id).and_then(|e| motions.get_mut(*e).ok()) {
                    m.pos = pos.into();
                    m.rot = Quat::from_array(rot).normalize();
                    m.vel = Vec3::ZERO;
                    m.asleep = true;
                    m.at = now;
                }
            }
            LootServerMessage::PropHeld { id, holder } => grab.set_holder(id, holder, me),
            LootServerMessage::ShelfInfo { id, price, stock, merchant, attention, pocketable, refuse } => {
                grab.shelf_info.insert(id, crate::props_client::ShelfInfoData { price, stock, merchant, attention, pocketable, refuse });
            }
            LootServerMessage::Removed { id } => {
                grab.forget(id);
                pending.0.remove(&id);
                if let Some(e) = index.0.remove(&id) {
                    commands.entity(e).despawn();
                }
            }
            LootServerMessage::Notice(text) => state.notify(text, now),
            LootServerMessage::UseStarted { verb, duration } => state.using = Some((verb, now, duration)),
            LootServerMessage::UseFinished { ok } => {
                if !ok && state.using.is_some() {
                    state.notify("Прервано", now);
                }
                state.using = None;
            }
        }
    }
}

/// На что смотрит игрок: ближайшее тело или предмет на линии взгляда.
fn aim_target(
    mut state: ResMut<LootState>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    corpses: Query<(&CorpseView, &Transform)>,
    ground: Query<(&GroundView, &Transform)>,
    level: Res<LevelColliders>,
    grab: Res<crate::props_client::PropGrab>,
) {
    state.target = None;
    if state.ui_open {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let origin = cam.translation();
    let dir = cam.forward().as_vec3();
    let mut best: Option<(f32, LootId)> = None;
    let mut test = |center: Vec3, radius: f32, id: LootId| {
        let to = center - origin;
        let along = to.dot(dir);
        if along < 0.0 || along > INTERACT_RANGE + 1.0 {
            return;
        }
        let miss = (to - dir * along).length();
        if miss <= radius {
            // Предметы приоритетнее тел (они меньше и лежат рядом с ними).
            let score = along + miss;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, id));
            }
        }
    };
    // Предметы: точный луч по их коробке (чуть больше, чтобы мелочь было легко поймать).
    let mut item: Option<(f32, LootId)> = None;
    for (g, t) in &ground {
        if grab.holding == Some(g.id) {
            continue;
        }
        let obb = crate::props::Obb { center: t.translation, rot: t.rotation, half: g.half + Vec3::splat(0.03) };
        if let Some(d) = obb.ray(origin, dir).filter(|d| *d <= INTERACT_RANGE + 0.5)
            && item.is_none_or(|(b, _)| d < b)
        {
            item = Some((d, g.id));
        }
    }
    if let Some((d, id)) = item
        && crate::combat::line_of_sight(&level, origin, origin + dir * (d - 0.05).max(0.0))
    {
        state.target = Some(id);
        return;
    }
    for (c, t) in &corpses {
        if c.id >= CONTAINER_ID_BASE {
            // Сундук уровня: точка — над крышкой; сквозь стены и закрытые двери не открыть.
            let close = (t.translation - origin).length_squared() < (INTERACT_RANGE + 1.0).powi(2);
            if close && crate::combat::line_of_sight(&level, origin, t.translation) {
                test(t.translation, 0.5, c.id);
            }
        } else {
            test(t.translation + Vec3::Y * 0.3, 0.9, c.id);
        }
    }
    state.target = best.map(|(_, id)| id);
}

fn interact_input(keys: Res<ButtonInput<KeyCode>>, mut client: ResMut<RenetClient>, mut state: ResMut<LootState>, time: Res<Time>, ground: Query<&GroundView>) {
    if state.ui_open {
        return;
    }
    let mut notices = Vec::new();
    if keys.just_pressed(KeyCode::KeyE)
        && let Some(id) = state.target
    {
        send(&mut client, &LootClientMessage::Interact { id });
    }
    // Быстрое лечение: 4 — бинт, 5 — зелье (первый найденный в сумке).
    for (key, name) in [(KeyCode::Digit4, "Бинт"), (KeyCode::Digit5, "Целебное зелье")] {
        if keys.just_pressed(key) {
            let base = ItemBaseId::by_name(name);
            match state.inventory.as_ref().and_then(|inv| inv.all_items().find(|i| i.base == base)) {
                Some(item) => send(&mut client, &LootClientMessage::Use { uid: item.uid }),
                None => notices.push(format!("Нет предмета: {name}")),
            }
        }
    }
    // «G» на товаре прилавка — стащить в сумку.
    if keys.just_pressed(KeyCode::KeyG)
        && let Some(id) = state.target
        && ground.iter().any(|g| g.id == id && g.shelf.is_some())
    {
        send(&mut client, &LootClientMessage::ShelfSteal { id });
    }
    if keys.just_pressed(KeyCode::F8) {
        send(&mut client, &LootClientMessage::DebugSpawnCorpse);
    }
    for n in notices {
        state.notify(n, time.elapsed_secs());
    }
}

#[derive(Component)]
struct UseBar;

#[derive(Component)]
struct UseBarFill;

#[derive(Component)]
struct UseBarText;

fn setup_use_bar(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn((
            UseBar,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                bottom: px(110),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4),
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn((
                UseBarText,
                Text::new(""),
                TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(17.0), ..default() },
                TextColor(Color::srgb(0.95, 0.9, 0.8)),
                TextShadow::default(),
            ));
            p.spawn((
                Node { width: px(260), height: px(10), border: UiRect::all(px(1)), ..default() },
                BorderColor::all(Color::srgb(0.45, 0.36, 0.22)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ))
            .with_children(|b| {
                b.spawn((UseBarFill, Node { width: percent(0), height: percent(100), ..default() }, BackgroundColor(Color::srgb(0.45, 0.8, 0.4))));
            });
        });
}

/// Полоска «Перевязка…» с ходом применения.
fn update_use_bar(
    time: Res<Time>,
    state: Res<LootState>,
    mut bar: Single<&mut Node, (With<UseBar>, Without<UseBarFill>)>,
    mut fill: Single<&mut Node, (With<UseBarFill>, Without<UseBar>)>,
    mut text: Single<&mut Text, With<UseBarText>>,
    work: Option<Res<crate::work_client::MyWork>>,
) {
    // Работу руками показывает своя шкала (work_client), полоска не нужна.
    if crate::work_client::hands_busy(work.as_deref()) {
        bar.display = Display::None;
        return;
    }
    match &state.using {
        Some((verb, start, duration)) => {
            let t = ((time.elapsed_secs() - start) / duration.max(0.01)).clamp(0.0, 1.0);
            bar.display = Display::Flex;
            fill.width = percent(t * 100.0);
            let line = format!("{verb}… (удар или парирование прервут)");
            if text.0 != line {
                text.0 = line;
            }
        }
        None => bar.display = Display::None,
    }
}

#[derive(Component)]
struct PromptText;

#[derive(Component)]
struct NoticeText;

fn setup_prompt(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(56),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                PromptText,
                Text::new(""),
                TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(20.0), ..default() },
                TextColor(Color::WHITE),
                TextShadow::default(),
            ));
        });
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            bottom: px(70),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                NoticeText,
                Text::new(""),
                TextFont { font: fonts.regular.clone().into(), font_size: bevy::text::FontSize::Px(20.0), ..default() },
                TextColor(Color::srgb(1.0, 0.85, 0.6)),
                TextLayout::justify(Justify::Center),
                TextShadow::default(),
            ));
        });
}

#[allow(clippy::too_many_arguments)]
fn update_prompt(
    state: Res<LootState>,
    mut grab: ResMut<crate::props_client::PropGrab>,
    client: Option<ResMut<RenetClient>>,
    time: Res<Time>,
    corpses: Query<&CorpseView>,
    ground: Query<&GroundView>,
    index: Res<LootIndex>,
    knowledge: Option<Res<crate::lore_client::MyKnowledge>>,
    mut text: Single<(&mut Text, &mut TextColor), With<PromptText>>,
) {
    let (ref mut t, ref mut color) = *text;
    let now = time.elapsed_secs();
    // В руках: подсказка, как отпустить и бросить.
    if let Some(g) = grab.holding.and_then(|id| index.0.get(&id)).and_then(|e| ground.get(*e).ok()) {
        let w = crate::weight::format_weight(crate::weight::unit_weight(g.base) * g.count.max(1) as f32);
        let line = format!("{} ({w}) — отпустите ЛКМ, чтобы положить · [ПКМ] бросить · колесо — ближе/дальше", g.base.base().name);
        if t.0 != line {
            t.0 = line;
        }
        color.0 = Color::srgb(0.95, 0.9, 0.75);
        return;
    }
    let corpse = state.target.and_then(|id| corpses.iter().find(|c| c.id == id));
    let entity = state.target.and_then(|id| index.0.get(&id).copied());
    let (line, c) = match entity {
        _ if corpse.is_some() => {
            let c = corpse.unwrap();
            let suffix = if c.empty { " (пусто)" } else { "" };
            let verb = if c.id >= CONTAINER_ID_BASE { "Открыть" } else { "Обыскать" };
            (format!("[E] {verb}: {}{suffix}", c.name), Color::WHITE)
        }
        Some(e) if ground.get(e).is_ok() => {
            let g = ground.get(e).unwrap();
            let base = g.base.base();
            let count = if g.count > 1 { format!(" ×{}", g.count) } else { String::new() };
            let quality = if base.has_quality { format!(" ({})", g.quality.name().to_lowercase()) } else { String::new() };
            // Незнакомую траву персонаж не может назвать.
            let name = knowledge.as_ref().map(|k| k.0.item_name(g.base)).unwrap_or_else(|| base.name.to_string());
            let weight = crate::weight::format_weight(crate::weight::unit_weight(g.base) * g.count.max(1) as f32);
            if g.shelf.is_some() {
                // Товар: цена для нас (спрашиваем сервер), сколько есть и чем занят торговец.
                if let Some(mut c) = client {
                    grab.ask_price(&mut c, g.id, now);
                }
                let line = match grab.shelf_info.get(&g.id) {
                    Some(info) => {
                        let price = crate::economy::format_money(info.price);
                        let stock = if info.stock > 1 { format!(", есть {} шт.", info.stock) } else { String::new() };
                        let steal = if info.pocketable { " · [G] стащить" } else { "" };
                        let head = format!("{name}{quality} — {price} ({weight}{stock})");
                        match &info.refuse {
                            Some(why) => format!("{head}\n{why}"),
                            None => format!("{head}\n[E] купить · [ЛКМ] взять в руки{steal}\n{} {}", info.merchant, info.attention),
                        }
                    }
                    None => format!("{name}{quality} — товар ({weight})\n[E] купить · [ЛКМ] взять в руки"),
                };
                (line, Color::srgb(1.0, 0.85, 0.45))
            } else {
                (format!("[E] Подобрать: {name}{count}{quality} ({weight}) · [ЛКМ] взять в руки"), quality_color(g.quality))
            }
        }
        _ => (String::new(), Color::WHITE),
    };
    if t.0 != line {
        t.0 = line;
    }
    color.0 = c;
}

fn update_notices(time: Res<Time>, mut state: ResMut<LootState>, mut text: Single<&mut Text, With<NoticeText>>) {
    let now = time.elapsed_secs();
    let state = state.bypass_change_detection();
    state.notices.retain(|(_, at)| now - at < 3.0);
    let line = state.notices.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join("\n");
    if text.0 != line {
        text.0 = line;
    }
}
