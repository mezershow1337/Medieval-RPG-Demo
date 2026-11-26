//! Рыбалка глазами игрока (в духе RDR2, но проще):
//!
//! 1. Удочка в правой руке (хотбар). **Зажать ЛКМ** — замах (чем дольше, тем дальше),
//!    отпустить — заброс туда, куда смотрите. **B** — сменить наживку (черви / креветки).
//! 2. Поплавок на воде покачивается; рыба сперва пробует наживку — поплавок вздрагивает.
//!    Когда он **уходит под воду** — **ЛКМ**, подсечь (на это чуть больше секунды).
//! 3. Вываживание: **держать ЛКМ** — подматывать лесу, отпустить — дать рыбе слабину.
//!    Полоска показывает натяжение: в красном долго держать нельзя — леса лопнет; совсем
//!    без натяжения рыба сойдёт с крючка. Когда рыба бросается в сторону — **ведите удочку
//!    (камеру) в другую сторону**: натяжение меньше, подмотка быстрее.
//! 4. **ПКМ** — смотать удочку.
//!
//! Сервер решает, кто клюнул и когда (fishing_server.rs); клиент рисует поплавок, лесу,
//! рыбу на крючке и полоску натяжения.

use crate::audio::Sfx;
use crate::fight::LocalFighter;
use crate::fishing::{self, Bait, Fight, FightEnd, FightSpec};
use crate::fishing_net::*;
use crate::items::ItemBaseId;
use crate::loot_client::LootState;
use crate::player::PlayerCamera;
use crate::protocol::{decode, encode};
use crate::ui_theme::{Skin, UiTheme, ink};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::{RenetClient, client_connected};

pub struct FishingClientPlugin;

impl Plugin for FishingClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Fishing::default())
            .insert_resource(FishingHands::default())
            .insert_resource(DemoInput::default())
            .insert_resource(RodView::default())
            .add_systems(Startup, (setup_fish_assets, setup_fish_ui.after(crate::ui_theme::setup_fonts)))
            .add_systems(Update, receive_fish.run_if(client_connected))
            .add_systems(Update, (fish_input, tick_fishing, draw_fishing, update_fish_ui).chain().after(receive_fish));
        if demo().is_some() {
            app.add_systems(Update, demo_driver.before(fish_input));
        }
    }
}

/// Удочка в руке: ЛКМ и ПКМ — для рыбалки, а не для удара (fight.rs смотрит сюда).
#[derive(Resource, Default)]
pub struct FishingHands(pub bool);

/// Как держать удочку от первого лица (её ставит fight.rs): `raise` 0..1 — от «к воде» до
/// «вскинута над головой», `side` — увести вбок. fight.rs пишет назад `tip` — вершину
/// удочки в осях камеры, от неё и идёт леса.
#[derive(Resource, Default)]
pub struct RodView {
    pub raise: f32,
    pub side: f32,
    pub tip: Option<Vec3>,
}

/// Где вершина у модели удочки (от хвата, вдоль оси), м.
pub const ROD_TIP: f32 = 2.7;

#[derive(Debug, Clone, Default)]
enum Phase {
    #[default]
    Idle,
    /// Замах: сила 0..1.
    Charging { power: f32 },
    /// Поплавок летит.
    Flying { t: f32, dur: f32, from: Vec3, to: Vec3, water: bool },
    /// Ждём ответа сервера на заброс.
    Sent,
    /// Поплавок на воде: `t` — с заброса.
    Waiting { t: f32, bite_in: f32, nibbles: Vec<f32>, spec: FightSpec, line: f32, reel: f32 },
    /// Подсекли, ждём, на крючке ли рыба.
    Hooking { spec: FightSpec, line: f32, reel: f32 },
    Fighting { fight: Fight, reel: f32, dir: Vec3, origin: Vec3, offset: f32 },
    /// Рыба вытащена, ждём, что скажет сервер; потом висит на леске.
    Landed { t: f32 },
    /// Сматываем пустую лесу.
    Retrieve { t: f32, from: Vec3 },
}

#[derive(Resource)]
pub struct Fishing {
    phase: Phase,
    bait: Bait,
    /// Поплавок (центр, на воде).
    bobber: Vec3,
    /// Насколько поплавок притоплен (0 — лежит, 1 — под водой).
    dip: f32,
    /// Кончик удилища.
    tip: Vec3,
    /// Подсказка-строка и до какого времени показывать «итог» (поймали, сорвалась…).
    result: Option<(String, f32)>,
    /// Пойманная рыба (вид, вес) — висит на леске.
    caught: Option<(ItemBaseId, f32)>,
    /// Время (для покачивания).
    clock: f32,
    /// Куда игрок ведёт удочку: сглаженное движение мыши по горизонтали.
    lead: f32,
    /// Последняя поклёвка-проба (для ряби).
    ripple: f32,
}

impl Default for Fishing {
    fn default() -> Self {
        Fishing { phase: Phase::Idle, bait: Bait::Worms, bobber: Vec3::ZERO, dip: 0.0, tip: Vec3::ZERO, result: None, caught: None, clock: 0.0, lead: 0.0, ripple: -10.0 }
    }
}

impl Fishing {
    fn active(&self) -> bool {
        !matches!(self.phase, Phase::Idle | Phase::Charging { .. })
    }

    fn say(&mut self, text: impl Into<String>, secs: f32) {
        let text = text.into();
        debug!("рыбалка: {text}");
        self.result = Some((text, self.clock + secs));
    }
}

/// `MRPG_FISH_DEMO=wait|bite|fight|caught` — для снимков: игрок сам забрасывает и вываживает.
fn demo() -> Option<String> {
    std::env::var("MRPG_FISH_DEMO").ok()
}

fn send(client: &mut RenetClient, msg: &FishClientMessage) {
    client.send_message(CH_FISH, encode(msg));
}

#[derive(Resource)]
struct FishAssets {
    float: Handle<Mesh>,
    cork: Handle<StandardMaterial>,
    paint: Handle<StandardMaterial>,
    ring: Handle<Mesh>,
    ring_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Bobber;
#[derive(Component)]
struct BobberCap;
#[derive(Component)]
struct Ripple;
#[derive(Component)]
struct CaughtFish(ItemBaseId);

fn setup_fish_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Поплавок из пробки: веретено, верх выкрашен суриком, перо-антенна.
    let mut mb = crate::level::MeshBuilder::default();
    mb.lathe(Vec3::new(0.0, -0.05, 0.0), &[(0.0, 0.0), (0.016, 0.02), (0.024, 0.05), (0.018, 0.075), (0.0, 0.08)], 12);
    let float = meshes.add(mb.build());
    let cork = materials.add(StandardMaterial { base_color: Color::srgb(0.62, 0.46, 0.3), perceptual_roughness: 0.85, ..default() });
    let paint = materials.add(StandardMaterial { base_color: Color::srgb(0.72, 0.12, 0.08), perceptual_roughness: 0.5, ..default() });
    let ring = meshes.add(Annulus::new(0.9, 1.0));
    let ring_mat = materials.add(StandardMaterial { base_color: Color::srgba(0.9, 0.95, 1.0, 0.35), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
    commands.insert_resource(FishAssets { float, cork, paint, ring, ring_mat });
}

#[derive(Component)]
struct FishPanel;
#[derive(Component)]
struct FishHint;
#[derive(Component)]
struct FishBarBox;
#[derive(Component)]
struct FishBarFill;
#[derive(Component)]
struct FishBarMark;

fn setup_fish_ui(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((Node { position_type: PositionType::Absolute, bottom: px(118), width: percent(100), justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE))
        .with_children(|root| {
            root.spawn((
                FishPanel,
                Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(6), padding: Skin::Leather.padding(), min_width: px(360), display: Display::None, ..default() },
                theme.skin(Skin::Leather),
            ))
            .with_children(|p| {
                p.spawn((FishHint, theme.text("", 17.0, ink::TEXT), TextLayout::justify(Justify::Center)));
                p.spawn((FishBarBox, Node { width: px(320), height: px(16), display: Display::None, ..default() })).with_children(|b| {
                    theme.bar(b, Node { width: percent(100), height: percent(100), ..default() }, 0.3, ink::GOOD, FishBarFill);
                    // Предел лесы — риска на 85 %.
                    b.spawn((FishBarMark, Node { position_type: PositionType::Absolute, left: percent(85), top: px(-3), width: px(2), height: px(22), ..default() }, BackgroundColor(ink::GOLD)));
                });
            });
        });
}

/// Удочка в руке, если она там.
fn rod_in_hand(f: &LocalFighter) -> Option<&'static fishing::Rod> {
    if f.gear.holstered {
        return None;
    }
    f.gear.main_hand.and_then(|b| fishing::rod(ItemBaseId(b).base().name))
}

fn bait_count(loot: &LootState, bait: Bait) -> u32 {
    loot.inventory.as_ref().map(|inv| inv.all_items().filter(|i| i.info().name == bait.item()).map(|i| i.count as u32).sum()).unwrap_or(0)
}

/// Камера, от которой считаем кончик удилища и заброс.
fn eye(cams: &Query<&GlobalTransform, With<PlayerCamera>>) -> Option<GlobalTransform> {
    cams.iter().next().copied()
}

/// Кончик удилища: вперёд-вправо-вверх от глаз (удочку держат правой рукой, вершину приподнимают).
fn rod_tip(cam: &GlobalTransform, lift: f32) -> Vec3 {
    let (f, r, u) = (cam.forward().as_vec3(), cam.right().as_vec3(), cam.up().as_vec3());
    let flat = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
    cam.translation() + flat * 2.3 + r * 0.45 + Vec3::Y * (0.75 + lift) + u * 0.05
}

#[allow(clippy::too_many_arguments)]
fn receive_fish(mut client: ResMut<RenetClient>, mut state: ResMut<Fishing>, mut loot: ResMut<LootState>, time: Res<Time>, mut sfx: MessageWriter<Sfx>) {
    let now = time.elapsed_secs();
    while let Some(bytes) = client.receive_message(CH_FISH) {
        let Some(msg) = decode::<FishServerMessage>(&bytes) else { continue };
        match msg {
            FishServerMessage::Cast { bite_in, nibbles, spec, line, reel } => {
                if matches!(state.phase, Phase::Sent) {
                    state.phase = Phase::Waiting { t: 0.0, bite_in, nibbles, spec, line, reel };
                }
            }
            FishServerMessage::Refused(text) => {
                let from = state.bobber;
                state.phase = Phase::Retrieve { t: 0.0, from };
                state.say(text, 3.5);
            }
            FishServerMessage::Hooked => {
                if let Phase::Hooking { spec, line, reel } = state.phase.clone() {
                    let origin = state.tip;
                    let flat = Vec3::new(state.bobber.x - origin.x, 0.0, state.bobber.z - origin.z);
                    let dist = flat.length();
                    state.phase = Phase::Fighting { fight: Fight::new(spec, dist, line), reel, dir: flat.normalize_or_zero(), origin, offset: 0.0 };
                    sfx.write(Sfx::at("step_water", state.bobber).gain(1.2).pitch(0.8));
                }
            }
            FishServerMessage::Missed(text) => {
                let from = state.bobber;
                state.phase = Phase::Retrieve { t: 0.0, from };
                state.say(text, 3.5);
            }
            FishServerMessage::Caught { name, kg } => {
                if let Some(base) = ItemBaseId::find(&name) {
                    state.caught = Some((base, kg));
                }
                let about = fishing::FISH.iter().find(|f| f.name == name).map(|f| f.about).unwrap_or("");
                state.say(format!("Поймали: {name}, {}. {about}", fmt_kg(kg)), 6.0);
                loot.notify(format!("Улов: {name} ({})", fmt_kg(kg)), now);
                sfx.write(Sfx::ui("coin").gain(0.0));
            }
            FishServerMessage::Notice(text) => {
                state.say(text.clone(), 4.0);
                loot.notify(text, now);
            }
        }
    }
}

fn fmt_kg(kg: f32) -> String {
    if kg < 1.0 { format!("{} г", (kg * 1000.0).round() as u32) } else { format!("{:.1} кг", kg).replace('.', ",") }
}

#[allow(clippy::too_many_arguments)]
fn fish_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    client: Option<ResMut<RenetClient>>,
    local: Query<&LocalFighter>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    loot: Res<LootState>,
    mut state: ResMut<Fishing>,
    mut hands: ResMut<FishingHands>,
    mut demo_in: ResMut<DemoInput>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    let rod = local.iter().next().and_then(rod_in_hand);
    hands.0 = rod.is_some();
    let Some(rod) = rod else {
        if state.active() || matches!(state.phase, Phase::Charging { .. }) {
            if let Some(mut c) = client {
                send(&mut c, &FishClientMessage::Reel);
            }
            state.phase = Phase::Idle;
            state.caught = None;
        }
        return;
    };
    let demo = demo().is_some();
    let locked = cursor.grab_mode != CursorGrabMode::None || demo;
    let Some(mut client) = client else { return };
    let Some(cam) = eye(&cams) else { return };
    // Мышь / демо: что нажато.
    let (lmb_down, lmb_held, lmb_up, rmb) = if demo {
        let d = &mut *demo_in;
        let r = (d.press, d.hold, d.release, false);
        d.press = false;
        d.release = false;
        r
    } else if locked && !loot.ui_open {
        (mouse.just_pressed(MouseButton::Left), mouse.pressed(MouseButton::Left), mouse.just_released(MouseButton::Left), mouse.just_pressed(MouseButton::Right))
    } else {
        (false, false, false, false)
    };
    // Куда ведём удочку: поворот камеры за последние доли секунды.
    let k = (-dt / 0.35).exp();
    let mx = if demo { demo_in.lead * 40.0 } else { motion.delta.x };
    state.lead = (state.lead * k + mx * 0.02).clamp(-1.5, 1.5);
    if keys.just_pressed(KeyCode::KeyB) && !state.active() {
        let next = if state.bait == Bait::Worms { Bait::Shrimp } else { Bait::Worms };
        state.bait = next;
        state.say(format!("Наживка: {} ({})", next.item().to_lowercase(), bait_count(&loot, next)), 2.0);
    }
    if rmb && state.active() {
        send(&mut client, &FishClientMessage::Reel);
        let from = state.bobber;
        state.phase = Phase::Retrieve { t: 0.0, from };
        state.caught = None;
        return;
    }
    match &mut state.phase {
        Phase::Idle => {
            if lmb_down {
                if bait_count(&loot, state.bait) == 0 {
                    let other = if state.bait == Bait::Worms { Bait::Shrimp } else { Bait::Worms };
                    if bait_count(&loot, other) > 0 {
                        state.bait = other;
                    } else {
                        state.say("Нет наживки: черви и креветки — у рыбака у Песочной бухты", 3.5);
                        return;
                    }
                }
                state.caught = None;
                state.phase = Phase::Charging { power: 0.0 };
            }
        }
        Phase::Charging { power } => {
            *power = (*power + dt / 1.1).min(1.0);
            if lmb_up || !lmb_held {
                let p = *power;
                let f = cam.forward().as_vec3();
                let flat = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
                let feet = cam.translation() - Vec3::Y * 1.6;
                let dist = 3.0 + p * (rod.cast - 3.0);
                let mut to = feet + flat * dist;
                let depth = crate::fishing_server::water_depth(to.x, to.z);
                let water = depth > 0.25;
                to.y = if water { crate::level::WATER_Y } else { crate::level::ground_y(to.x, to.z) + 0.03 };
                let from = rod_tip(&cam, 0.6);
                state.phase = Phase::Flying { t: 0.0, dur: 0.45 + dist * 0.025, from, to, water };
                sfx.write(Sfx::at("swing", from).gain(0.6).pitch(1.3));
            }
        }
        Phase::Waiting { t, bite_in, .. } => {
            if lmb_down {
                let bit = *t >= *bite_in;
                if let Phase::Waiting { spec, line, reel, .. } = state.phase.clone() {
                    send(&mut client, &FishClientMessage::Hook);
                    state.phase = Phase::Hooking { spec, line, reel };
                    if !bit {
                        // Сервер скажет «рано».
                    }
                }
            }
        }
        Phase::Fighting { .. } => {
            // Ведёте удочку в сторону: вправо по экрану — плюс.
            let lead = state.lead.clamp(-1.0, 1.0);
            let end = match &mut state.phase {
                Phase::Fighting { .. } if demo && demo_in.freeze => None,
                Phase::Fighting { fight, reel, origin, .. } => {
                    let end = fight.step(dt, lmb_held, lead, *reel);
                    let moved = Vec3::new(cam.translation().x - origin.x, 0.0, cam.translation().z - origin.z).length();
                    if moved > 4.5 { Some(FightEnd::Snapped) } else { end }
                }
                _ => None,
            };
            if let Some(end) = end {
                match end {
                    FightEnd::Landed => {
                        send(&mut client, &FishClientMessage::Landed);
                        state.phase = Phase::Landed { t: 0.0 };
                        sfx.write(Sfx::at("step_water", state.bobber).gain(1.3).pitch(1.1));
                    }
                    FightEnd::Snapped => {
                        send(&mut client, &FishClientMessage::Lost { snapped: true });
                        let from = state.bobber;
                        state.phase = Phase::Retrieve { t: 0.0, from };
                        state.say("Леса лопнула! Рыба ушла вместе с крючком", 4.0);
                    }
                    FightEnd::Escaped => {
                        send(&mut client, &FishClientMessage::Lost { snapped: false });
                        let from = state.bobber;
                        state.phase = Phase::Retrieve { t: 0.0, from };
                        state.say("Рыба сошла с крючка", 3.5);
                    }
                }
            } else if lmb_held && (state.clock * 7.0).fract() < dt * 7.0 {
                sfx.write(Sfx::ui("gear").gain(0.25).pitch(1.6));
            }
        }
        Phase::Landed { t } => {
            if *t > 1.0 && lmb_down {
                state.phase = Phase::Idle;
                state.caught = None;
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn tick_fishing(
    mut commands: Commands,
    time: Res<Time>,
    real: Res<Time<Real>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
    client: Option<ResMut<RenetClient>>,
    mut state: ResMut<Fishing>,
    mut sfx: MessageWriter<Sfx>,
    assets: Option<Res<FishAssets>>,
    loot: Res<LootState>,
    mut rod: ResMut<RodView>,
) {
    let dt = time.delta_secs();
    state.clock += dt;
    let clock = state.clock;
    if let Some(cam) = eye(&cams) {
        let lift = match state.phase {
            Phase::Charging { power } => 0.6 + power * 0.5,
            Phase::Fighting { ref fight, .. } => 0.9 + fight.tension.min(1.3) * 0.4,
            Phase::Landed { .. } => 1.1,
            _ => 0.6,
        };
        let want = ((lift - 0.6) / 0.6).clamp(0.0, 1.0);
        let side = match state.phase {
            Phase::Fighting { ref fight, .. } => (state.lead * 0.6 - fight.side * 0.25).clamp(-1.0, 1.0),
            _ => 0.0,
        };
        let k = 1.0 - (-dt * 9.0).exp();
        rod.raise += (want - rod.raise) * k;
        rod.side += (side - rod.side) * k;
        state.tip = match rod.tip {
            Some(local) => cam.transform_point(local),
            None => rod_tip(&cam, lift),
        };
    }
    let near = eye(&cams).map(|c| {
        let f = c.forward().as_vec3();
        let flat = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
        c.translation() + flat * 1.05 + c.right().as_vec3() * 0.12 - Vec3::Y * 0.1
    });
    let wave = |p: Vec3, t: f32| 0.025 * (t * 1.7 + p.x * 0.4).sin() + 0.015 * (t * 2.3 + p.z * 0.5).sin();
    let mut ripple = false;
    let tip = state.tip;
    let bait = state.bait;
    match &mut state.phase {
        Phase::Idle | Phase::Charging { .. } | Phase::Sent | Phase::Hooking { .. } => {}
        Phase::Flying { t, dur, from, to, water } => {
            *t += dt;
            let u = (*t / *dur).min(1.0);
            let p = from.lerp(*to, u) + Vec3::Y * (4.0 * u * (1.0 - u) * (1.5 + from.distance(*to) * 0.08));
            let (done, wet, land) = (u >= 1.0, *water, *to);
            if done {
                state.bobber = land;
                if wet {
                    sfx.write(Sfx::at("step_water", land).gain(0.8).pitch(1.5));
                    ripple = true;
                    if let Some(mut c) = client {
                        send(&mut c, &FishClientMessage::Cast { at: land.to_array(), bait });
                        state.phase = Phase::Sent;
                    }
                } else {
                    state.say("Поплавок упал не в воду", 2.5);
                    state.phase = Phase::Retrieve { t: 0.0, from: land };
                }
            } else {
                state.bobber = p;
            }
        }
        Phase::Waiting { t, bite_in, nibbles, .. } => {
            // Поклёвку сервер считает по настоящим часам: если кадр подвис, игровые часы
            // отстают, а рыба — нет.
            let t0 = *t;
            *t += real.delta_secs();
            let t1 = *t;
            let mut dip: f32 = 0.0;
            for &n in nibbles.iter() {
                let a = t1 - n;
                if (0.0..0.35).contains(&a) {
                    dip = dip.max(0.35 * (a / 0.35 * std::f32::consts::PI).sin());
                }
                if t0 < n && t1 >= n {
                    ripple = true;
                }
            }
            let b = t1 - *bite_in;
            if b >= 0.0 && b < fishing::HOOK_WINDOW + 0.6 {
                dip = 1.0;
                if t0 < *bite_in {
                    ripple = true;
                    sfx.write(Sfx::at("step_water", state.bobber).gain(1.0).pitch(1.8));
                }
            } else if b >= fishing::HOOK_WINDOW + 0.6 {
                dip = 0.0;
            }
            state.dip += (dip - state.dip) * (1.0 - (-dt * 14.0).exp());
            let base = Vec3::new(state.bobber.x, crate::level::WATER_Y, state.bobber.z);
            state.bobber = base + Vec3::Y * (wave(base, clock) - state.dip * 0.12);
        }
        Phase::Fighting { fight, dir, origin, offset, .. } => {
            // Рыба уходит в сторону — поплавок (леса) уводит туда же.
            let side = Vec3::new(-dir.z, 0.0, dir.x);
            let want = fight.side * (fight.distance * 0.35).min(5.0);
            *offset += (want - *offset) * (1.0 - (-dt * 1.5).exp());
            let p = *origin + *dir * fight.distance + side * *offset;
            let burst = fight.burst > 0.0;
            let y = crate::level::WATER_Y - 0.06 + if burst { 0.05 * (clock * 25.0).sin() } else { 0.0 };
            state.bobber = Vec3::new(p.x, y, p.z);
            state.dip = 1.0;
            if burst && (clock * 3.0).fract() < dt * 3.0 {
                ripple = true;
            }
        }
        Phase::Landed { t } => {
            *t += dt;
            // Рыбу подтягивают к себе и держат перед глазами.
            let k = (*t / 0.6).min(1.0);
            let hang = near.unwrap_or(tip - Vec3::Y * 0.9);
            state.bobber = state.bobber.lerp(hang, k);
            state.dip = 0.0;
        }
        Phase::Retrieve { t, from } => {
            *t += dt;
            let u = (*t / 0.7).min(1.0);
            state.bobber = from.lerp(tip - Vec3::Y * 0.5, u) + Vec3::Y * (u * (1.0 - u) * 1.2);
            state.dip = 0.0;
            if u >= 1.0 {
                state.phase = Phase::Idle;
            }
        }
    }
    if ripple && let Some(a) = assets.as_ref() {
        state.ripple = clock;
        let p = Vec3::new(state.bobber.x, crate::level::WATER_Y + 0.02, state.bobber.z);
        commands.spawn((Ripple, Mesh3d(a.ring.clone()), MeshMaterial3d(a.ring_mat.clone()), Transform::from_translation(p).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(0.05)), bevy::light::NotShadowCaster));
    }
    let _ = loot;
}

#[allow(clippy::too_many_arguments)]
fn draw_fishing(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<Fishing>,
    assets: Option<Res<FishAssets>>,
    models: Option<Res<crate::icons::ItemModels>>,
    mut gizmos: Gizmos,
    mut bobbers: Query<(Entity, &mut Transform, &mut Visibility), (With<Bobber>, Without<Ripple>, Without<CaughtFish>)>,
    mut ripples: Query<(Entity, &mut Transform, &MeshMaterial3d<StandardMaterial>), (With<Ripple>, Without<Bobber>, Without<CaughtFish>)>,
    mut fish: Query<(Entity, &CaughtFish, &mut Transform), (Without<Bobber>, Without<Ripple>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cams: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    let Some(a) = assets else { return };
    let dt = time.delta_secs();
    // Рябь расходится и тает.
    for (e, mut tf, _) in &mut ripples {
        tf.scale += Vec3::splat(dt * 0.9);
        if tf.scale.x > 1.3 {
            commands.entity(e).despawn();
        }
    }
    if let Some(mut m) = materials.get_mut(&a.ring_mat) {
        m.base_color = Color::srgba(0.92, 0.96, 1.0, 0.3);
    }
    let showing = !matches!(state.phase, Phase::Idle | Phase::Charging { .. });
    let hanging = matches!(state.phase, Phase::Landed { .. }) && state.caught.is_some();
    // Поплавок.
    let pose = Transform::from_translation(state.bobber).with_rotation(Quat::from_rotation_z(state.dip * 0.5));
    if bobbers.is_empty() {
        commands
            .spawn((Bobber, Mesh3d(a.float.clone()), MeshMaterial3d(a.cork.clone()), pose, Visibility::Hidden, bevy::light::NotShadowCaster))
            .with_children(|b| {
                b.spawn((BobberCap, Mesh3d(a.float.clone()), MeshMaterial3d(a.paint.clone()), Transform::from_xyz(0.0, 0.045, 0.0).with_scale(Vec3::new(0.9, 0.5, 0.9))));
            });
    }
    for (_, mut tf, mut vis) in &mut bobbers {
        *tf = pose;
        *vis = if showing && !hanging { Visibility::Inherited } else { Visibility::Hidden };
    }
    // Пойманная рыба висит на леске.
    let want = if hanging { state.caught.map(|c| c.0) } else { None };
    for (e, f, mut tf) in &mut fish {
        if Some(f.0) != want {
            commands.entity(e).despawn();
        } else {
            // Модель лежит на боку рылом к −Z: ставим рылом вверх (висит на крючке за губу)
            // и боком к глазу, чуть покачивая.
            let swing = (time.elapsed_secs() * 2.2).sin() * 0.25;
            let to = eye(&cams).map(|c| c.translation() - state.bobber).unwrap_or(Vec3::Z);
            let face = to.x.atan2(to.z);
            let len = crate::icons::model_half_extents(f.0)[2];
            *tf = Transform::from_translation(state.bobber - Vec3::Y * len).with_rotation(Quat::from_rotation_y(face + swing) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
        }
    }
    if let (Some(b), Some(models)) = (want, models.as_ref())
        && fish.is_empty()
    {
        commands.spawn((CaughtFish(b), bevy::world_serialization::WorldAssetRoot(models.get(b)), Transform::from_translation(state.bobber)));
    }
    if !showing {
        return;
    }
    // Леса: от кончика удилища к поплавку, с провисом (натянутая — почти прямая).
    let tension = match &state.phase {
        Phase::Fighting { fight, .. } => fight.tension.min(1.0),
        Phase::Landed { .. } => 1.0,
        _ => 0.15,
    };
    let (p0, p1) = (state.tip, state.bobber + Vec3::Y * 0.03);
    let sag = p0.distance(p1) * 0.12 * (1.0 - tension);
    let pts: Vec<Vec3> = (0..=16).map(|i| {
        let u = i as f32 / 16.0;
        p0.lerp(p1, u) - Vec3::Y * (4.0 * u * (1.0 - u) * sag)
    }).collect();
    gizmos.linestrip(pts, Color::srgba(0.86, 0.84, 0.78, 0.85));
}

#[allow(clippy::type_complexity)]
fn update_fish_ui(
    state: Res<Fishing>,
    hands: Res<FishingHands>,
    loot: Res<LootState>,
    mut panel: Single<&mut Node, (With<FishPanel>, Without<FishBarBox>, Without<FishBarFill>)>,
    mut hint: Single<&mut Text, With<FishHint>>,
    mut bar: Single<&mut Node, (With<FishBarBox>, Without<FishPanel>, Without<FishBarFill>)>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), (With<FishBarFill>, Without<FishPanel>, Without<FishBarBox>)>,
) {
    let mut lines: Vec<String> = Vec::new();
    let mut bar_k: Option<(f32, Color)> = None;
    let bait = format!("{} ({})", state.bait.item().to_lowercase(), bait_count(&loot, state.bait));
    match &state.phase {
        Phase::Idle => {
            if hands.0 {
                lines.push(format!("[ЛКМ, зажать] Замах и заброс · [B] Наживка: {bait}"));
            }
        }
        Phase::Charging { power } => {
            lines.push("Отпустите ЛКМ — заброс".into());
            bar_k = Some((*power, ink::GOLD));
        }
        Phase::Flying { .. } | Phase::Sent => {}
        Phase::Waiting { t, bite_in, .. } => {
            if *t >= *bite_in && *t < *bite_in + fishing::HOOK_WINDOW + 0.6 {
                lines.push("Клюёт! [ЛКМ] Подсечь".into());
            } else {
                lines.push(format!("Ждите поклёвки… · [ЛКМ] Подсечь · [ПКМ] Смотать · {bait}"));
            }
        }
        Phase::Hooking { .. } => lines.push("Подсекаем…".into()),
        Phase::Fighting { fight, .. } => {
            let way = if fight.side > 0.3 {
                "рыба уходит вправо — ведите удочку влево"
            } else if fight.side < -0.3 {
                "рыба уходит влево — ведите удочку вправо"
            } else {
                "рыба тянет от берега"
            };
            lines.push(format!("[ЛКМ, держать] Подматывать · отпустить — дать слабину · {} м", fight.distance.round() as i32));
            lines.push(way.into());
            let k = (fight.tension_frac() / 1.18).clamp(0.0, 1.0);
            let c = if fight.danger() > 0.05 || k > 0.85 { ink::BAD } else if k < 0.08 { ink::DIM } else { ink::GOOD };
            bar_k = Some((k, c));
        }
        Phase::Landed { .. } => {
            if state.caught.is_some() {
                lines.push("[ЛКМ] Снять с крючка и забросить снова".into());
            }
        }
        Phase::Retrieve { .. } => {}
    }
    if let Some((text, until)) = &state.result
        && state.clock < *until
    {
        lines.insert(0, text.clone());
    }
    let show = !lines.is_empty() && (hands.0 || state.result.as_ref().is_some_and(|(_, u)| state.clock < *u));
    let disp = if show { Display::Flex } else { Display::None };
    if panel.display != disp {
        panel.display = disp;
    }
    let text = lines.join("\n");
    if hint.0 != text {
        hint.0 = text;
    }
    let bd = if bar_k.is_some() && show { Display::Flex } else { Display::None };
    if bar.display != bd {
        bar.display = bd;
    }
    if let Some((k, c)) = bar_k {
        fill.0.width = percent(k * 100.0);
        fill.1.0 = c;
    }
}

// ---------------------------------------------------------------------------
// Демонстрация для снимков (MRPG_FISH_DEMO)
// ---------------------------------------------------------------------------

/// Что «нажимает» демонстрация.
#[derive(Resource, Default)]
pub struct DemoInput {
    press: bool,
    hold: bool,
    release: bool,
    lead: f32,
    /// Замереть посреди вываживания (для снимка).
    freeze: bool,
}

/// Демонстрация: взять удочку в руку, забросить, подсечь, вываживать — до нужной стадии и замереть.
#[allow(clippy::too_many_arguments)]
fn demo_driver(
    time: Res<Time>,
    mut input: ResMut<DemoInput>,
    mut state: ResMut<Fishing>,
    loot: Res<LootState>,
    local: Query<&LocalFighter>,
    mut client: Option<ResMut<RenetClient>>,
    mut step: Local<(u32, f32)>,
) {
    let want = demo().unwrap_or_default();
    let t = time.elapsed_secs();
    let rod = local.iter().next().and_then(rod_in_hand);
    if rod.is_none() {
        // Удочку — в руку (её выдаёт MRPG_CONSOLE="give …").
        if step.1 + 1.0 < t
            && let (Some(c), Some(inv)) = (client.as_mut(), loot.inventory.as_ref())
            && let Some(item) = inv.all_items().find(|i| fishing::rod(i.info().name).is_some())
        {
            use crate::loot_net::{LootClientMessage, Owner};
            crate::loot_client::send(c, &LootClientMessage::Move { uid: item.uid, from: Owner::Me, to: Owner::Me, target: crate::inventory::Target::Equip(crate::items::EquipSlot::MainHand) });
            step.1 = t;
        }
        return;
    }
    if std::env::var("MRPG_FISH_DEMO_BAIT").as_deref() == Ok("shrimp") && !state.active() {
        state.bait = Bait::Shrimp;
    }
    let delay: f32 = std::env::var("MRPG_FISH_DEMO_AT").ok().and_then(|s| s.parse().ok()).unwrap_or(14.0);
    match &state.phase {
        Phase::Idle if t > delay && step.0 == 0 => {
            input.press = true;
            input.hold = true;
            step.0 = 1;
            step.1 = t;
        }
        Phase::Charging { power } if *power > 0.85 && want != "charge" => {
            input.hold = false;
            input.release = true;
        }
        Phase::Waiting { t: w, bite_in, .. } if want != "wait" && *w > *bite_in - 0.2 && step.0 == 1 => {
            if want != "bite" {
                input.press = true;
                step.0 = 2;
            }
        }
        Phase::Fighting { fight, .. } => {
            let stop = want == "fight" && fight.elapsed > 1.0;
            if stop || input.freeze {
                // Замираем: время для рыбы не идёт (снимок).
                input.hold = true;
                input.freeze = true;
            } else {
                input.hold = fight.tension_frac() < 0.8;
                input.lead = -fight.side;
            }
        }
        _ => {}
    }
    let _ = &mut client;
}
