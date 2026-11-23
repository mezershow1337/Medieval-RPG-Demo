//! Клиент торговли: торговцы в мире (фигуры и прилавки), наведение и клавиши,
//! приём сообщений сервера. Окна рисует `trade_ui.rs`.
//!
//! Клавиши:
//! - E, глядя на торговца — заговорить (окно торговли);
//! - T, глядя на другого игрока — предложить обмен; Y / N — принять или отказать;
//! - J — репутация у фракций и розыск;
//! - X — бросить урок или смену.

use crate::client::RemoteActor;
use crate::factions::Reputation;
use crate::inventory_ui::UiFonts;
use crate::loot_client::LootState;
use crate::player::PlayerCamera;
use crate::protocol::{ActorKind, decode, encode};
use crate::rig::{Beard, FootGear, Hair, HeadGear, LegGear, NeedsRig, Outfit, RigPose, Tunic};
use crate::trade::MerchantId;
use crate::trade_net::*;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::{RenetClient, client_connected};

pub struct TradeClientPlugin;

impl Plugin for TradeClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TradeState::default())
            .add_plugins(crate::trade_ui::TradeUiPlugin)
            .add_systems(
                Startup,
                setup_trade_hud.after(crate::inventory_ui::setup_fonts),
            )
            .add_systems(
                Update,
                (receive_trade, trade_keys).chain().run_if(client_connected),
            )
            .add_systems(Update, (aim_merchants, idle_merchants, update_trade_hud))
            .add_systems(PostUpdate, free_cursor_while_trading);
    }
}

/// На кого смотрит игрок.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aim {
    Merchant(MerchantId),
    Player(u64),
}

#[derive(Resource, Default)]
pub struct TradeState {
    pub merchants: Vec<MerchantInfo>,
    /// Открытое окно торговца.
    pub view: Option<MerchantView>,
    /// Когда пришло окно (для отсчёта времени заказов).
    pub view_at: f32,
    /// Корзина: что берём с прилавка и что отдаём (номер предмета, штук).
    pub basket_buy: Vec<Line>,
    pub basket_sell: Vec<Line>,
    /// Последняя реплика торговца.
    pub said: Option<String>,
    pub rep: Option<Reputation>,
    pub fine: u32,
    pub rep_open: bool,
    /// Урок или смена: подпись, начало, длительность.
    pub lesson: Option<(String, f32, f32)>,
    /// Приглашение к обмену: от кого, когда.
    pub invite: Option<(u64, f32)>,
    pub p2p: Option<P2pState>,
    pub aim: Option<Aim>,
    /// Растёт при изменениях — окно перестраивается.
    pub rev: u64,
}

impl TradeState {
    pub fn window_open(&self) -> bool {
        self.view.is_some() || self.rep_open || self.p2p.is_some()
    }

    pub fn merchant(&self, id: MerchantId) -> Option<&MerchantInfo> {
        self.merchants.iter().find(|m| m.id == id)
    }
}

pub fn send(client: &mut RenetClient, msg: &TradeClientMessage) {
    client.send_message(CH_TRADE, encode(msg));
}

#[derive(Component)]
pub struct MerchantNpc {
    pub id: MerchantId,
    /// Фаза «дыхания», чтобы торговцы не стояли как статуи синхронно.
    phase: f32,
}

pub fn outfit_for(info: &MerchantInfo) -> Outfit {
    use crate::factions::Faction;
    let c = |v: [f32; 3]| Color::srgb(v[0], v[1], v[2]);
    // Облик по народу: ромеи в туниках, готы с длинными волосами, аланы и кочевники в кафтанах.
    let base = Outfit::for_actor(ActorKind::Player, info.id.0 as u64);
    let (tunic, hair, head) = match info.faction {
        Faction::Goths => (Tunic::Gothic, Hair::Long, HeadGear::Spangenhelm),
        Faction::Alans => (Tunic::Kaftan, Hair::Short, HeadGear::Conical),
        Faction::Nomads => (Tunic::Kaftan, Hair::Shaved, HeadGear::FeltCap),
        _ => (Tunic::Byzantine, Hair::Short, HeadGear::Spangenhelm),
    };
    Outfit {
        head: if info.look.helmet { head } else { HeadGear::Bare },
        tunic,
        hair,
        beard: if info.id.0.is_multiple_of(3) { Beard::Short } else { Beard::Full },
        legs: if info.id.0.is_multiple_of(2) { LegGear::Wrapped } else { LegGear::Trousers },
        feet: if info.look.helmet { FootGear::Boots } else { FootGear::Shoes },
        tunic_color: c(info.look.tunic),
        trousers: c(info.look.trousers),
        ..base
    }
}

/// Прилавок: стол, навес на столбах и немного товара. За стенами — расстеленный войлок и тюки.
fn spawn_stall(
    p: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    info: &MerchantInfo,
) {
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.25, 0.15),
        perceptual_roughness: 0.9,
        ..default()
    });
    let tunic = info.look.tunic;
    let cloth = materials.add(StandardMaterial {
        base_color: Color::srgb(
            tunic[0] * 0.8 + 0.15,
            tunic[1] * 0.8 + 0.12,
            tunic[2] * 0.8 + 0.1,
        ),
        perceptual_roughness: 1.0,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let clay = materials.add(StandardMaterial {
        base_color: Color::srgb(0.66, 0.38, 0.24),
        perceptual_roughness: 0.85,
        ..default()
    });
    // Прилавок (торговец смотрит в −Z): те же столешницы, что у сервера, — на них лежит товар.
    for b in &info.stall {
        let c = Vec3::from_array(b.center);
        let h = Vec3::from_array(b.half);
        if b.felt {
            p.spawn((Mesh3d(meshes.add(Cuboid::new(h.x * 2.0, h.y * 2.0, h.z * 2.0))), MeshMaterial3d(cloth.clone()), Transform::from_translation(c)));
            continue;
        }
        // Столешница и ножки.
        let top = 0.04;
        p.spawn((
            Mesh3d(meshes.add(Cuboid::new(h.x * 2.0, top, h.z * 2.0))),
            MeshMaterial3d(wood.clone()),
            Transform::from_translation(c + Vec3::Y * (h.y - top * 0.5)),
        ));
        // Передняя доска закрывает низ (как у настоящих лавок), сзади — ножки.
        p.spawn((
            Mesh3d(meshes.add(Cuboid::new(h.x * 2.0, h.y * 2.0 - top, 0.03))),
            MeshMaterial3d(wood.clone()),
            Transform::from_translation(c + Vec3::new(0.0, -top * 0.5, -h.z + 0.015)),
        ));
        for x in [-h.x + 0.05, h.x - 0.05] {
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.06, h.y * 2.0 - top, 0.06))),
                MeshMaterial3d(wood.clone()),
                Transform::from_translation(c + Vec3::new(x, -top * 0.5, h.z - 0.05)),
            ));
        }
    }
    if info.faction.outside_walls() {
        // Тюки с остальным товаром — за спиной торговца.
        for (i, x) in [-0.6f32, 0.0, 0.6].into_iter().enumerate() {
            let size = 0.32 + i as f32 * 0.05;
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(size, size * 0.7, size * 1.2))),
                MeshMaterial3d(wood.clone()),
                Transform::from_xyz(x, size * 0.35, 0.8),
            ));
        }
        let _ = clay;
        return;
    }
    // Навес.
    for (x, z) in [
        (-1.05f32, -1.35f32),
        (1.05, -1.35),
        (-1.05, 0.45),
        (1.05, 0.45),
    ] {
        p.spawn((
            Mesh3d(meshes.add(Cylinder::new(0.05, 2.4))),
            MeshMaterial3d(wood.clone()),
            Transform::from_xyz(x, 1.2, z),
        ));
    }
    p.spawn((
        Mesh3d(meshes.add(Cuboid::new(2.4, 0.03, 2.1))),
        MeshMaterial3d(cloth),
        Transform::from_xyz(0.0, 2.42, -0.45).with_rotation(Quat::from_rotation_x(-0.12)),
    ));
    let _ = clay;
}

#[allow(clippy::too_many_arguments)]
fn receive_trade(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut state: ResMut<TradeState>,
    mut loot: ResMut<LootState>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    npcs: Query<Entity, With<MerchantNpc>>,
) {
    let now = time.elapsed_secs();
    while let Some(bytes) = client.receive_message(CH_TRADE) {
        let Some(msg) = decode::<TradeServerMessage>(&bytes) else {
            continue;
        };
        match msg {
            TradeServerMessage::Merchants(list) => {
                for e in &npcs {
                    commands.entity(e).despawn();
                }
                // Прилавки для физики своего предмета в руках.
                let statics = list.iter().flat_map(|i| crate::trade::stall_obbs(i.pos.into(), i.yaw, &i.stall)).collect();
                commands.insert_resource(crate::props_server::PropStatics(statics));
                for (i, info) in list.iter().enumerate() {
                    let pos: Vec3 = info.pos.into();
                    commands
                        .spawn((
                            MerchantNpc {
                                id: info.id,
                                phase: i as f32 * 1.7,
                            },
                            Transform::from_translation(pos)
                                .with_rotation(Quat::from_rotation_y(info.yaw)),
                            Visibility::default(),
                        ))
                        .with_children(|p| {
                            p.spawn((
                                NeedsRig(outfit_for(info)),
                                Transform::default(),
                                Visibility::default(),
                            ));
                            spawn_stall(p, &mut meshes, &mut materials, info);
                        });
                }
                state.merchants = list;
            }
            TradeServerMessage::View(view) => {
                // Корзина живёт, пока открыт тот же торговец; исчезнувшие товары убираем.
                if state.view.as_ref().map(|v| v.id) != Some(view.id) {
                    state.basket_buy.clear();
                    state.basket_sell.clear();
                    state.said = None;
                }
                state
                    .basket_buy
                    .retain(|(uid, _)| view.shelf.iter().any(|s| s.item.uid == *uid));
                state.basket_sell.retain(|(uid, _)| {
                    view.bids.iter().any(|b| b.uid == *uid && b.price.is_some())
                });
                if loot.ui_open {
                    loot.ui_open = false;
                    loot.rev += 1;
                }
                state.view = Some(view);
                state.view_at = now;
                state.rev += 1;
            }
            TradeServerMessage::Closed => {
                state.view = None;
                state.rev += 1;
            }
            TradeServerMessage::Standing { rep, fine } => {
                if fine > state.fine {
                    loot.notify(
                        format!("Вы в розыске: штраф {}", crate::economy::format_money(fine)),
                        now,
                    );
                }
                state.rep = Some(rep);
                state.fine = fine;
                state.rev += 1;
            }
            TradeServerMessage::LessonStarted { title, secs } => {
                state.lesson = Some((title, now, secs));
                // Урок идёт в мире: окно закрываем, чтобы можно было смотреть на мастера.
                if state.view.take().is_some() {
                    send(&mut client, &TradeClientMessage::Close);
                }
                state.rev += 1;
            }
            TradeServerMessage::LessonFinished { ok } => {
                if !ok && state.lesson.is_some() {
                    loot.notify("Урок прерван", now);
                }
                state.lesson = None;
            }
            TradeServerMessage::Say(text) => {
                loot.notify(text.clone(), now);
                state.said = Some(text);
                // Сделка прошла — корзину очищаем (сервер пришлёт новое окно следом).
                state.basket_buy.clear();
                state.basket_sell.clear();
                state.rev += 1;
            }
            TradeServerMessage::P2pInvite { from } => {
                state.invite = Some((from, now));
            }
            TradeServerMessage::P2p(p) => {
                if loot.ui_open {
                    loot.ui_open = false;
                    loot.rev += 1;
                }
                state.p2p = Some(p);
                state.rev += 1;
            }
            TradeServerMessage::P2pClosed { reason } => {
                state.p2p = None;
                loot.notify(reason, now);
                state.rev += 1;
            }
        }
    }
}

fn close_windows(state: &mut TradeState, client: &mut RenetClient) {
    if state.view.take().is_some() {
        send(client, &TradeClientMessage::Close);
    }
    if state.p2p.is_some() {
        send(client, &TradeClientMessage::P2pCancel);
    }
    state.rep_open = false;
    state.rev += 1;
}

#[allow(clippy::too_many_arguments)]
fn trade_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut client: ResMut<RenetClient>,
    mut state: ResMut<TradeState>,
    loot: Res<LootState>,
    time: Res<Time>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let now = time.elapsed_secs();
    // Открылся инвентарь — окна торговли убираем.
    if loot.ui_open && state.window_open() {
        close_windows(&mut state, &mut client);
        return;
    }
    if state.window_open() {
        let close = keys.just_pressed(KeyCode::Escape)
            || (state.view.is_some() && keys.just_pressed(KeyCode::KeyE))
            || (state.rep_open && keys.just_pressed(KeyCode::KeyJ));
        if close {
            close_windows(&mut state, &mut client);
            if !keys.just_pressed(KeyCode::Escape) {
                cursor.grab_mode = CursorGrabMode::Locked;
                cursor.visible = false;
            }
        }
        return;
    }
    if loot.ui_open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        state.rep_open = true;
        state.rev += 1;
    }
    // Говорить с торговцем можно, если не смотрим на тело или вещь (их «E» важнее).
    if keys.just_pressed(KeyCode::KeyE)
        && loot.target.is_none()
        && let Some(Aim::Merchant(id)) = state.aim
    {
        send(&mut client, &TradeClientMessage::Open { merchant: id });
    }
    if keys.just_pressed(KeyCode::KeyT)
        && let Some(Aim::Player(id)) = state.aim
    {
        send(&mut client, &TradeClientMessage::P2pRequest { target: id });
    }
    if let Some((from, at)) = state.invite {
        if now - at > 30.0 {
            state.invite = None;
        } else if keys.just_pressed(KeyCode::KeyY) || keys.just_pressed(KeyCode::KeyN) {
            send(
                &mut client,
                &TradeClientMessage::P2pRespond {
                    from,
                    accept: keys.just_pressed(KeyCode::KeyY),
                },
            );
            state.invite = None;
        }
    }
    if keys.just_pressed(KeyCode::KeyX) && state.lesson.is_some() {
        send(&mut client, &TradeClientMessage::StopLesson);
    }
}

/// Пока открыто окно торговли, мышь свободна.
fn free_cursor_while_trading(
    state: Res<TradeState>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if state.window_open() && (cursor.grab_mode != CursorGrabMode::None || !cursor.visible) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// На кого смотрит игрок: торговец (до TALK_RANGE) или другой игрок (до P2P_RANGE).
fn aim_merchants(
    mut state: ResMut<TradeState>,
    loot: Res<LootState>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    npcs: Query<(&MerchantNpc, &GlobalTransform), Without<crate::civil_client::Fallen>>,
    players: Query<(&RemoteActor, &GlobalTransform)>,
) {
    let aim = (|| {
        if loot.ui_open || state.window_open() {
            return None;
        }
        let cam = camera.single().ok()?;
        let origin = cam.translation();
        let dir = cam.forward().as_vec3();
        let mut best: Option<(f32, Aim)> = None;
        let mut test = |center: Vec3, radius: f32, range: f32, aim: Aim| {
            let to = center - origin;
            let along = to.dot(dir);
            if along < 0.0 || along > range + 0.6 {
                return;
            }
            if (to - dir * along).length() <= radius && best.is_none_or(|(d, _)| along < d) {
                best = Some((along, aim));
            }
        };
        for (npc, t) in &npcs {
            test(
                t.translation() + Vec3::Y * 1.1,
                0.55,
                TALK_RANGE,
                Aim::Merchant(npc.id),
            );
        }
        for (actor, t) in &players {
            if actor.kind == ActorKind::Player {
                test(
                    t.translation() + Vec3::Y * 1.1,
                    0.5,
                    P2P_RANGE,
                    Aim::Player(actor.id),
                );
            }
        }
        best.map(|(_, a)| a)
    })();
    if state.aim != aim {
        state.aim = aim;
    }
}

/// Торговцы дышат, переминаются и время от времени поправляют товар.
fn idle_merchants(
    time: Res<Time>,
    npcs: Query<(&MerchantNpc, &Children)>,
    mut poses: Query<&mut RigPose>,
) {
    let t = time.elapsed_secs();
    for (npc, children) in &npcs {
        for child in children.iter() {
            let Ok(mut pose) = poses.get_mut(child) else {
                continue;
            };
            let solved = crate::rig::solve_pose(&crate::life_client::merchant_pose(t, npc.phase));
            *pose = solved.pose;
        }
    }
}

// ---------------------------------------------------------------------------
// Подсказки на экране: «[E] Торговать», приглашение к обмену, полоска урока
// ---------------------------------------------------------------------------

#[derive(Component)]
struct TradePrompt;

#[derive(Component)]
struct LessonBar;

#[derive(Component)]
struct LessonFill;

#[derive(Component)]
struct LessonText;

fn setup_trade_hud(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(60),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                TradePrompt,
                Text::new(""),
                TextFont {
                    font: fonts.bold.clone().into(),
                    font_size: bevy::text::FontSize::Px(19.0),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.88, 0.7)),
                TextLayout::justify(Justify::Center),
                TextShadow::default(),
            ));
        });
    commands
        .spawn((
            LessonBar,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                bottom: px(140),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4),
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn((
                LessonText,
                Text::new(""),
                TextFont {
                    font: fonts.bold.clone().into(),
                    font_size: bevy::text::FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.9, 0.8)),
                TextShadow::default(),
            ));
            p.spawn((
                Node {
                    width: px(320),
                    height: px(10),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.45, 0.36, 0.22)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ))
            .with_children(|b| {
                b.spawn((
                    LessonFill,
                    Node {
                        width: percent(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.85, 0.7, 0.35)),
                ));
            });
        });
}

#[allow(clippy::type_complexity)]
fn update_trade_hud(
    time: Res<Time>,
    state: Res<TradeState>,
    loot: Res<LootState>,
    mut prompt: Single<&mut Text, (With<TradePrompt>, Without<LessonText>)>,
    mut bar: Single<&mut Node, (With<LessonBar>, Without<LessonFill>)>,
    mut fill: Single<&mut Node, (With<LessonFill>, Without<LessonBar>)>,
    mut lesson_text: Single<&mut Text, (With<LessonText>, Without<TradePrompt>)>,
) {
    let now = time.elapsed_secs();
    let mut lines = Vec::new();
    if loot.target.is_none() && !loot.ui_open && !state.window_open() {
        match state.aim {
            Some(Aim::Merchant(id)) => {
                if let Some(m) = state.merchant(id) {
                    lines.push(format!("[E] Говорить: {} — {}", m.name, m.title));
                    let standing = state
                        .rep
                        .as_ref()
                        .map(|r| r.standing(m.faction).name())
                        .unwrap_or("");
                    lines.push(format!("{} · {}", m.faction.name(), standing));
                }
            }
            Some(Aim::Player(id)) => lines.push(format!("[T] Предложить обмен игроку {id}")),
            None => {}
        }
    }
    if let Some((from, at)) = state.invite
        && now - at <= 30.0
    {
        lines.push(format!(
            "Игрок {from} предлагает обмен: [Y] согласиться, [N] отказать"
        ));
    }
    if state.fine > 0 && !state.window_open() {
        lines.push(format!(
            "В розыске: штраф {} (уплатить казначею гарнизона)",
            crate::economy::format_money(state.fine)
        ));
    }
    let line = lines.join("\n");
    if prompt.0 != line {
        prompt.0 = line;
    }
    match &state.lesson {
        Some((title, start, secs)) => {
            let k = ((now - start) / secs.max(0.1)).clamp(0.0, 1.0);
            bar.display = Display::Flex;
            fill.width = percent(k * 100.0);
            let t = format!("{title}… не отходите от мастера · X — бросить");
            if lesson_text.0 != t {
                lesson_text.0 = t;
            }
        }
        None => bar.display = Display::None,
    }
}
