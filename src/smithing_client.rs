//! Клиент кузницы: окно у горна (B), ковка молотом по участкам поковки, цвет каления,
//! мехи (пробел), закалка, отпуск, точило, литьё, плавка руды, ремонт.
//!
//! Сервер считает честно; клиент между снимками сам шагает ту же модель, чтобы металл
//! остывал и разгорался плавно. Удар: «маятник» качается над наковальней — чем ближе
//! к середине в миг щелчка, тем точнее удар. ЛКМ по участку — лёгкий молот, ПКМ — тяжёлый.
#![allow(deprecated)] // Interaction

use crate::inventory_ui::UiFonts;
use crate::items::{Item, ItemUid, Quality};
use crate::loot_client::LootState;
use crate::lore::{Lore, Skill, Technique};
use crate::lore_client::MyKnowledge;
use crate::player::LocalPlayer;
use crate::protocol::{decode, encode};
use crate::smithing::*;
use crate::smithing_net::*;
use crate::smithing_quick::{self as quick, Mark, QuickForge};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy_renet::RenetClient;
use std::collections::HashMap;

pub struct SmithingClientPlugin;

impl Plugin for SmithingClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SmithState::default())
            .add_systems(Startup, setup_smith_ui.after(crate::inventory_ui::setup_fonts))
            .add_plugins(crate::smithing_fx::SmithFxPlugin)
            .add_systems(Update, (receive_smith, smith_input, smith_buttons, predict, rebuild_panel, update_live, update_prompt).chain().in_set(SmithSet))
            .add_systems(Update, shot_quick_forge.before(SmithSet));
    }
}

/// Окно и предсказание кузницы (эффекты рисуются после).
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct SmithSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tab {
    /// Ковка на удачу: простое дело без горы материалов.
    Quick,
    Forge,
    Cast,
    Other,
    /// Верстак: древки и стрелы.
    Fletch,
}

#[derive(Resource)]
pub struct SmithState {
    pub stations: Vec<StationView>,
    fires: HashMap<u32, (f32, f32)>,
    /// Ближайший горн, у которого стоим.
    pub near: Option<u32>,
    pub open: bool,
    pub work: Option<WorkView>,
    /// Предсказанная работа (шагается каждый кадр).
    pred: Option<Job>,
    bellows: bool,
    /// Мехи качают пробелом в такт: до какого времени дует последний качок и когда он был.
    pump_until: f32,
    last_pump: f32,
    /// Как удался последний качок 0..1.
    pump_q: f32,
    swing_t: f32,
    last_strike: f32,
    tab: Tab,
    steel: bool,
    metals: [u8; 5],
    ore: u8,
    charcoal: u8,
    rev: u64,
    /// Раздел списка ковки.
    fam: Option<crate::arsenal::Fam>,
    built: Option<(u64, u64, u64, bool)>,
    /// Искры, пар, звон — разбирает `smithing_fx`.
    pub(crate) fx_queue: Vec<(u32, u64, Fx)>,
    /// Чужие раскалённые поковки и тигли.
    others: Vec<PieceView>,
    /// Свой номер клиента.
    pub(crate) me: u64,
    /// Клейма мастера и заказ (ковка на удачу).
    guild: GuildView,
}

impl Default for SmithState {
    fn default() -> Self {
        Self {
            stations: Vec::new(),
            fires: HashMap::new(),
            near: None,
            open: false,
            work: None,
            pred: None,
            bellows: false,
            pump_until: 0.0,
            last_pump: -10.0,
            pump_q: 0.0,
            swing_t: 0.0,
            last_strike: -10.0,
            tab: Tab::Quick,
            steel: true,
            metals: [3, 2, 0, 0, 0],
            ore: 6,
            charcoal: 10,
            rev: 1,
            fam: Some(crate::arsenal::Fam::Sword),
            built: None,
            fx_queue: Vec::new(),
            others: Vec::new(),
            me: 0,
            guild: GuildView::default(),
        }
    }
}

impl SmithState {
    pub(crate) fn station(&self, id: u32) -> Option<&StationView> {
        self.stations.iter().find(|s| s.id == id)
    }
    /// Показ для снимков: чужая поковка и огонь без сервера.
    pub(crate) fn demo(&mut self, station: u32, piece: Option<PieceView>, blast: f32) {
        if self.stations.is_empty() {
            // Без сервера: горны с карты, номера как у сервера.
            self.stations = crate::level::forges()
                .iter()
                .enumerate()
                .map(|(i, f)| StationView { id: i as u32 + 1, name: f.name.to_string(), kind: f.kind, fire: f.fire.into(), anvil: f.anvil.into(), trough: f.trough.into(), grind: f.grind.into() })
                .collect();
        }
        self.fires.insert(station, (300.0, blast));
        self.others = piece.into_iter().collect();
    }
    /// Огонь горна: (секунды угля, дутьё).
    pub(crate) fn fire(&self, id: u32) -> Option<(f32, f32)> {
        self.fires.get(&id).copied()
    }
    /// Все поковки у горнов: своя — из предсказания (плавнее), чужие — с сервера.
    pub(crate) fn all_pieces(&self) -> Vec<PieceView> {
        let mut v: Vec<PieceView> = self.others.iter().filter(|p| p.player != self.me).cloned().collect();
        if let (Some(w), Some(job)) = (self.work.as_ref(), self.pred.as_ref())
            && let Some(p) = PieceView::of(self.me, w.station, job)
        {
            v.push(p);
        }
        v
    }
}

fn send(client: &mut RenetClient, msg: &SmithClientMessage) {
    client.send_message(CH_SMITH, encode(msg));
}

fn receive_smith(
    mut client: Option<ResMut<RenetClient>>,
    transport: Option<Res<bevy_renet::netcode::NetcodeClientTransport>>,
    mut state: ResMut<SmithState>,
    mut loot: ResMut<LootState>,
    time: Res<Time>,
) {
    if let Some(t) = transport {
        state.me = t.client_id();
    }
    let Some(client) = client.as_mut() else { return };
    while let Some(bytes) = client.receive_message(CH_SMITH) {
        let Some(msg) = decode::<SmithServerMessage>(&bytes) else { continue };
        match msg {
            SmithServerMessage::Stations(v) => {
                state.stations = v;
                state.rev += 1;
            }
            SmithServerMessage::Fire { station, fuel, blast } => {
                state.fires.insert(station, (fuel, blast));
            }
            SmithServerMessage::Work(w) => {
                // Ковка на удачу меняется только от ударов — окно перестраиваем после каждого.
                let quick = matches!(w.as_ref().map(|x| &x.job), Some(Job::Quick(_)));
                let changed = quick || state.work.as_ref().map(|x| std::mem::discriminant(&x.job)) != w.as_ref().map(|x| std::mem::discriminant(&x.job)) || state.work.is_some() != w.is_some();
                state.pred = w.as_ref().map(|w| w.job.clone());
                state.work = w;
                if changed {
                    state.rev += 1;
                }
            }
            SmithServerMessage::Notice(text) => {
                let now = time.elapsed_secs();
                loot.notify(text, now);
            }
            SmithServerMessage::Fx { station, player, fx } => state.fx_queue.push((station, player, fx)),
            SmithServerMessage::Pieces(v) => state.others = v,
            SmithServerMessage::Guild(g) => {
                state.guild = g;
                state.rev += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ввод
// ---------------------------------------------------------------------------

fn smith_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<SmithState>,
    mut loot: ResMut<LootState>,
    mut client: Option<ResMut<RenetClient>>,
    player: Query<&Transform, With<LocalPlayer>>,
    knowledge: Res<MyKnowledge>,
    time: Res<Time>,
) {
    let Ok(tf) = player.single() else { return };
    let feet = tf.translation;
    let near = state
        .stations
        .iter()
        .map(|s| (s.id, Vec3::from(s.anvil)))
        .filter(|(_, a)| Vec2::new(a.x - feet.x, a.z - feet.z).length() <= SMITH_REACH - 0.3 && (a.y - feet.y).abs() < 3.0)
        .min_by(|a, b| a.1.distance(feet).total_cmp(&b.1.distance(feet)))
        .map(|(id, _)| id);
    if state.near != near {
        state.near = near;
        state.rev += 1;
    }
    if state.open && near.is_none() {
        state.open = false;
        state.rev += 1;
    }
    if keys.just_pressed(KeyCode::KeyB) && (near.is_some() || state.open) {
        state.open = !state.open;
        loot.ui_open = state.open;
        loot.rev += 1;
        state.rev += 1;
    }
    if state.open && (keys.just_pressed(KeyCode::Escape) || !loot.ui_open) {
        state.open = false;
        loot.ui_open = false;
        loot.rev += 1;
        state.rev += 1;
    }
    state.swing_t += time.delta_secs();
    let Some(client) = client.as_mut() else { return };
    // Мехи: качать пробелом в такт. Каждый качок дует, пока мехи выдыхают; держать
    // пробел бесполезно, частить тоже — мехи не успевают набрать воздух.
    let now = time.elapsed_secs();
    let quick = matches!(state.pred, Some(Job::Quick(_)));
    if state.open && quick && keys.just_pressed(KeyCode::Space) {
        quick_strike(&mut state, client, &knowledge, now);
    }
    if state.open && state.work.is_some() && !quick && keys.just_pressed(KeyCode::Space) {
        let gap = now - state.last_pump;
        state.last_pump = now;
        let q = if gap > 3.0 { 0.6 } else if gap < 0.4 { 0.0 } else { (1.0 - (gap / PUMP_PERIOD).ln().abs() / 2.2f32.ln()).clamp(0.0, 1.0) };
        state.pump_q = q;
        if q > 0.0 {
            state.pump_until = state.pump_until.max(now + 0.35 + 0.65 * q);
        }
    }
    let want = state.open && state.work.is_some() && now < state.pump_until;
    if want != state.bellows {
        state.bellows = want;
        send(client, &SmithClientMessage::Bellows(want));
    }
    if !state.open {
        return;
    }
    // Перевернуть поковку клещами на другую грань.
    if keys.just_pressed(KeyCode::KeyR) && matches!(state.pred, Some(Job::Forge(_))) {
        send(client, &SmithClientMessage::Flip);
    }
    if keys.just_pressed(KeyCode::KeyF)
        && let Some(job) = &state.pred
    {
        let spot = match job {
            Job::Forge(f) => Some(if f.spot == Spot::Fire { Spot::Anvil } else { Spot::Fire }),
            Job::Cast(c) => Some(if c.spot == Spot::Fire { Spot::Anvil } else { Spot::Fire }),
            _ => None,
        };
        if let Some(s) = spot {
            send(client, &SmithClientMessage::Move(s));
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Act {
    Close,
    Tab(Tab),
    /// Раздел списка ковки: None — заготовки и доспехи.
    Fam(Option<crate::arsenal::Fam>),
    Steel,
    Begin(Plan),
    Fuel,
    Move(Spot),
    Quench(Quench),
    Flip,
    Finish(bool),
    Abandon,
    Skim,
    Warm(bool),
    Metal(usize, i8),
    Ore(i8),
    Coal(i8),
    OpenMould,
    /// Ковка на удачу: удар молотом.
    QuickStrike,
    BuyMark(Mark),
    Deliver,
    NewOrder,
}

#[derive(Component)]
struct SmithButton(Act);

/// Участок поковки: щелчок — удар по нему.
#[derive(Component)]
struct Segment(u8);

/// Точило: щелчок — проход.
#[derive(Component)]
struct GrindPad;

/// Кнопка «Лить» (держать).
#[derive(Component)]
struct PourButton;

#[allow(clippy::too_many_arguments)]
/// Ровный такт мехов, секунд на качок.
const PUMP_PERIOD: f32 = 0.9;

fn smith_buttons(
    buttons: Query<(&Interaction, &SmithButton), Changed<Interaction>>,
    segments: Query<(&Interaction, &Segment)>,
    pads: Query<&Interaction, With<GrindPad>>,
    pour: Query<&Interaction, With<PourButton>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<SmithState>,
    mut loot: ResMut<LootState>,
    knowledge: Res<MyKnowledge>,
    time: Res<Time>,
    mut pouring: Local<bool>,
    mut client: Option<ResMut<RenetClient>>,
) {
    let Some(client) = client.as_mut() else { return };
    if !state.open {
        return;
    }
    let skill = knowledge.0.skill(Skill::Smithing);
    let now = time.elapsed_secs();
    // Удар молотом по участку.
    for (i, seg) in &segments {
        if !matches!(i, Interaction::Hovered | Interaction::Pressed) {
            continue;
        }
        let heavy = mouse.just_pressed(MouseButton::Right);
        if !(mouse.just_pressed(MouseButton::Left) || heavy) {
            continue;
        }
        if now - state.last_strike < STRIKE_GAP[heavy as usize] {
            continue;
        }
        state.last_strike = now;
        let acc = accuracy(swing_pos(state.swing_t, heavy), skill);
        send(client, &SmithClientMessage::Strike { zone: seg.0, heavy, acc });
        // Маятник начинает новый замах.
        state.swing_t = SWING_PERIOD[heavy as usize] * 0.25;
    }
    for i in &pads {
        if *i == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) && now - state.last_strike > 0.35 {
            state.last_strike = now;
            let acc = accuracy(swing_pos(state.swing_t, false), skill);
            send(client, &SmithClientMessage::Grind { acc });
        }
    }
    let held = pour.iter().any(|i| *i == Interaction::Pressed) && mouse.pressed(MouseButton::Left);
    if held != *pouring {
        *pouring = held;
        send(client, &SmithClientMessage::Pour(held));
    }
    for (i, b) in &buttons {
        if *i != Interaction::Pressed {
            continue;
        }
        let station = state.near.or(state.work.as_ref().map(|w| w.station)).unwrap_or(0);
        match b.0.clone() {
            Act::Close => {
                state.open = false;
                loot.ui_open = false;
                loot.rev += 1;
            }
            Act::Tab(t) => state.tab = t,
            Act::Fam(f) => {
                state.fam = f;
                state.rev += 1;
            }
            Act::Steel => state.steel = !state.steel,
            Act::Begin(plan) => send(client, &SmithClientMessage::Begin { station, plan }),
            Act::Fuel => send(client, &SmithClientMessage::AddFuel { station }),
            Act::Move(s) => send(client, &SmithClientMessage::Move(s)),
            Act::Quench(q) => send(client, &SmithClientMessage::Quench(q)),
            Act::Flip => send(client, &SmithClientMessage::Flip),
            Act::Finish(f) => send(client, &SmithClientMessage::Finish { fitting: f }),
            Act::Abandon => send(client, &SmithClientMessage::Abandon),
            Act::Skim => send(client, &SmithClientMessage::Skim),
            Act::Warm(on) => send(client, &SmithClientMessage::WarmMould(on)),
            Act::OpenMould => send(client, &SmithClientMessage::OpenMould),
            Act::Metal(k, d) => state.metals[k] = (state.metals[k] as i16 + d as i16).clamp(0, 12) as u8,
            Act::Ore(d) => state.ore = (state.ore as i16 + d as i16).clamp(3, 12) as u8,
            Act::Coal(d) => state.charcoal = (state.charcoal as i16 + d as i16).clamp(0, 20) as u8,
            Act::QuickStrike => quick_strike(&mut state, client, &knowledge, now),
            Act::BuyMark(m) => send(client, &SmithClientMessage::BuyMark(m)),
            Act::Deliver => send(client, &SmithClientMessage::Deliver),
            Act::NewOrder => send(client, &SmithClientMessage::NewOrder),
        }
        state.rev += 1;
    }
}

/// Между снимками сервера металл остывает и греется у нас.
fn predict(time: Res<Time>, mut state: ResMut<SmithState>) {
    let dt = time.delta_secs().min(0.1);
    let Some(w) = state.work.clone() else { return };
    let (fuel, blast) = state.fires.get(&w.station).copied().unwrap_or((100.0, 0.0));
    let blast = step_blast(blast, state.bellows, dt);
    if let Some(f) = state.fires.get_mut(&w.station) {
        f.1 = blast;
    }
    let fire = fire_temp(fuel, blast);
    let bellows = state.bellows;
    match state.pred.as_mut() {
        Some(Job::Forge(f)) => {
            f.step(dt, fire);
        }
        Some(Job::Cast(c)) => {
            c.step(dt, fire);
        }
        Some(Job::Soak(s)) => {
            s.bellows = bellows;
            s.step(dt, fuel > 0.0);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Окно
// ---------------------------------------------------------------------------

use crate::ui_theme::{Skin, ThemeButton, ink, theme};

const BORDER: Color = Color::srgba(0.55, 0.42, 0.24, 0.7);
const TITLE: Color = ink::TITLE;
const TEXT: Color = ink::TEXT;
const DIM: Color = ink::DIM;
const BAD: Color = ink::BAD;

#[derive(Component)]
struct SmithPanel;
#[derive(Component)]
struct SmithPrompt;
/// Живые части окна (обновляются каждый кадр).
#[derive(Component)]
enum Live {
    Swing,
    Sweet,
    HeatSwatch,
    HeatText,
    Fuel,
    Blast,
    SegFill(u8),
    SegText(u8),
    Status,
    Progress,
}

fn setup_smith_ui(mut commands: Commands, fonts: Res<UiFonts>) {
    commands.spawn((
        SmithPanel,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            left: px(12),
            top: px(50),
            width: px(600),
            max_height: percent(92),
            flex_direction: FlexDirection::Column,
            padding: Skin::Wood.padding(),
            row_gap: px(5),
            // Длинный список рецептов — прокрутка колесом.
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition::default(),
        crate::ui_theme::ScrollWheel,
        theme().skin(Skin::Wood),
        GlobalZIndex(31),
        Interaction::default(),
        FocusPolicy::Block,
    ));
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), top: percent(62), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            crate::ui_theme::theme().plaque(p).with_children(|p| {
                p.spawn((
                    SmithPrompt,
                    Text::new(""),
                    TextFont {
                        font: fonts.bold.clone().into(),
                        font_size: bevy::text::FontSize::Px(18.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.8, 0.5)),
                    TextLayout::justify(Justify::Center),
                    TextShadow::default(),
                    crate::ui_theme::Plaque,
                ));
            });
        });
}

fn label(fonts: &UiFonts, s: impl Into<String>, size: f32, color: Color, bold: bool) -> impl Bundle {
    (
        Text::new(s),
        TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: bevy::text::FontSize::Px(size), ..default() },
        TextColor(color),
    )
}

fn button(p: &mut ChildSpawnerCommands, fonts: &UiFonts, text: &str, act: Act, on: bool) {
    let _ = fonts;
    theme().toggle_button(p, text, (SmithButton(act), FocusPolicy::Block), on);
}

fn row<'a>(p: &'a mut ChildSpawnerCommands<'_>) -> EntityCommands<'a> {
    p.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(5), row_gap: px(4), ..default() })
}

fn have(loot: &LootState, name: &str) -> u32 {
    loot.inventory.as_ref().map_or(0, |inv| inv.grid.items.iter().filter(|p| p.item.info().name == name).map(|p| p.item.count as u32).sum())
}

/// Руды всех сортов: обожжённой, горной, болотной.
fn have_ore(loot: &LootState) -> u32 {
    crate::mining::OreGrade::ALL.iter().map(|g| have(loot, g.item())).sum()
}

/// Какие полосы взять: лучшие сначала, сталь — одна на лезвие (если просили).
fn pick_bars(loot: &LootState, pattern: Pattern, steel: bool) -> Vec<ItemUid> {
    let Some(inv) = loot.inventory.as_ref() else { return vec![] };
    let info = pattern.info();
    let mut items: Vec<&Item> = inv.grid.items.iter().map(|p| &p.item).collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.quality));
    let names: &[&str] = match pattern {
        Pattern::Bloom => &[BLOOM],
        Pattern::ScrapWeld => &[SCRAP],
        _ => &[IRON, STEEL],
    };
    let mut out = Vec::new();
    let mut left = info.metal as i32;
    if steel && info.harden {
        if let Some(s) = items.iter().find(|i| i.info().name == STEEL) {
            out.push(s.uid);
            left -= 1;
        }
    }
    for i in items.iter().filter(|i| names.contains(&i.info().name)) {
        if i.info().name == STEEL && out.contains(&i.uid) {
            continue;
        }
        if left <= 0 {
            break;
        }
        // Без стали — сначала железо.
        if !steel && i.info().name == STEEL && items.iter().any(|j| j.info().name == IRON && !out.contains(&j.uid)) {
            continue;
        }
        out.push(i.uid);
        left -= i.count as i32;
    }
    if left > 0 && !steel {
        for i in items.iter().filter(|i| i.info().name == STEEL) {
            if out.contains(&i.uid) {
                continue;
            }
            if left <= 0 {
                break;
            }
            out.push(i.uid);
            left -= i.count as i32;
        }
    }
    out
}

fn metal_count(loot: &LootState, pattern: Pattern) -> u32 {
    match pattern {
        Pattern::Bloom => have(loot, BLOOM),
        Pattern::ScrapWeld => have(loot, SCRAP),
        _ => have(loot, IRON) + have(loot, STEEL),
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_panel(
    mut commands: Commands,
    mut dock: ResMut<crate::ui_theme::SideDock>,
    mut state: ResMut<SmithState>,
    loot: Res<LootState>,
    knowledge: Res<MyKnowledge>,
    fonts: Res<UiFonts>,
    mut panel: Query<(Entity, &mut Node), With<SmithPanel>>,
) {
    let Ok((entity, mut node)) = panel.single_mut() else {
        return;
    };
    let want = if state.open {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != want {
        if state.open {
            commands
                .entity(entity)
                .insert(crate::ui_theme::Appear::default());
        }
        node.display = want;
    }
    let w = if state.open { 600.0 } else { 0.0 };
    if dock.smith != w {
        dock.smith = w;
    }
    if !state.open {
        return;
    }
    let key = (state.rev, loot.rev, knowledge.0.known.len() as u64, state.work.is_some());
    if state.built == Some(key) {
        return;
    }
    state.built = Some(key);
    let k = &knowledge.0;
    let fonts = &*fonts;
    let st = &*state;
    let station = st.work.as_ref().map(|w| w.station).or(st.near).and_then(|id| st.station(id)).cloned();
    commands.entity(entity).despawn_children();
    commands.entity(entity).with_children(|p| {
        row(p).with_children(|r| {
            let title = station
                .as_ref()
                .map_or("Кузница".to_string(), |s| s.name.clone());
            r.spawn((
                theme().title(title, 26.0),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));
            r.spawn(label(
                fonts,
                format!("Кузнечное дело: {:.0}", k.skill(Skill::Smithing)),
                13.0,
                DIM,
                false,
            ));
            button(r, fonts, "×", Act::Close, false);
        });
        theme().divider(p);
        let forge = station
            .as_ref()
            .is_none_or(|s| s.kind == StationKind::Forge);
        // На удачу куют у горна мастера: уголь и мехи не нужны.
        let quick_view = matches!(st.pred, Some(Job::Quick(_))) || (st.pred.is_none() && st.tab == Tab::Quick);
        if forge && !quick_view {
            row(p).with_children(|r| {
                r.spawn((label(fonts, "", 13.0, TEXT, false), Live::Fuel));
                button(r, fonts, &format!("Подсыпать уголь ({})", have(&loot, CHARCOAL)), Act::Fuel, false);
                r.spawn(label(fonts, "Дутьё:", 13.0, DIM, false));
                bar(r, 90.0, Color::srgb(1.0, 0.5, 0.15), Live::Blast);
                r.spawn(label(fonts, "Пробел — качнуть мехи (в такт, раз в секунду)", 12.0, DIM, false));
            });
        }
        match st.pred.clone() {
            None => begin_menu(p, fonts, st, &loot, k, forge),
            Some(job) => work_panel(p, fonts, st, &loot, k, &job),
        }
    });
}

fn bar(r: &mut ChildSpawnerCommands, w: f32, color: Color, live: Live) {
    r.spawn((
        Node {
            width: px(w),
            height: px(16),
            padding: UiRect::axes(px(5), px(4)),
            ..default()
        },
        theme().bar_frame(),
    ))
    .with_children(|b| {
        b.spawn((
            Node {
                width: percent(0),
                height: percent(100),
                ..default()
            },
            BackgroundColor(color),
            live,
        ));
    });
}

fn begin_menu(p: &mut ChildSpawnerCommands, fonts: &UiFonts, st: &SmithState, loot: &LootState, k: &crate::lore::Knowledge, forge: bool) {
    if !forge {
        // Сыродутный горн.
        p.spawn(label(fonts, "Сыродутный горн: руду и уголь слоями, потом непрерывно дуть.", 14.0, TEXT, false));
        row(p).with_children(|r| {
            button(r, fonts, "−", Act::Ore(-1), false);
            r.spawn(label(fonts, format!("Руда: {} (есть {})", st.ore, have_ore(loot)), 14.0, TEXT, true));
            button(r, fonts, "+", Act::Ore(1), false);
            button(r, fonts, "−", Act::Coal(-1), false);
            r.spawn(label(fonts, format!("Уголь: {} (есть {})", st.charcoal, have(loot, CHARCOAL)), 14.0, TEXT, true));
            button(r, fonts, "+", Act::Coal(1), false);
        });
        let ok = have_ore(loot) >= st.ore as u32 && have(loot, CHARCOAL) >= st.charcoal as u32;
        button(p, fonts, "Заложить горн", Act::Begin(Plan::Smelt { ore: st.ore, charcoal: st.charcoal }), ok);
        if k.knows(Lore::Technique(Technique::Smelting)) {
            p.spawn(label(fonts, "Вы знаете: угля по весу примерно столько же, сколько руды; держать жёлто-белый жар, но не белый.", 13.0, DIM, false));
        }
        return;
    }
    row(p).with_children(|r| {
        button(r, fonts, "На удачу", Act::Tab(Tab::Quick), st.tab == Tab::Quick);
        button(r, fonts, "Ковка", Act::Tab(Tab::Forge), st.tab == Tab::Forge);
        button(r, fonts, "Литьё", Act::Tab(Tab::Cast), st.tab == Tab::Cast);
        button(r, fonts, "Цементация и ремонт", Act::Tab(Tab::Other), st.tab == Tab::Other);
        button(r, fonts, "Стрелы", Act::Tab(Tab::Fletch), st.tab == Tab::Fletch);
    });
    match st.tab {
        Tab::Quick => quick_menu(p, fonts, st, loot),
        Tab::Forge => {
            row(p).with_children(|r| {
                r.spawn(label(fonts, format!("Железных полос: {}, стальных: {}, лома: {}, криц: {}", have(loot, IRON), have(loot, STEEL), have(loot, SCRAP), have(loot, BLOOM)), 13.0, DIM, false));
                button(r, fonts, if st.steel { "Наварить сталь на лезвие: да" } else { "Наварить сталь: нет" }, Act::Steel, st.steel);
            });
            // Разделы: заготовки и доспехи, затем семейства оружия.
            row(p).with_children(|r| {
                button(r, fonts, "Заготовки и доспехи", Act::Fam(None), st.fam.is_none());
                for f in crate::arsenal::Fam::ALL {
                    button(r, fonts, f.name(), Act::Fam(Some(f)), st.fam == Some(f));
                }
            });
            for pat in Pattern::all().into_iter().filter(|p| p.fam() == st.fam) {
                let info = pat.info();
                let known = info.needs.is_none_or(|t| k.knows(Lore::Technique(t)));
                let metal = match pat {
                    Pattern::Bloom => "крица".to_string(),
                    Pattern::ScrapWeld => format!("{} лома", info.metal),
                    _ => format!("{} полос{}", info.metal, if info.metal == 1 { "а" } else { "ы" }),
                };
                let parts: Vec<String> = info.parts.iter().map(|(n, c)| if *c > 1 { format!("{} ×{c}", n.to_lowercase()) } else { n.to_lowercase() }).collect();
                let need = if parts.is_empty() { metal } else { format!("{metal}; при сборке: {}", parts.join(", ")) };
                let enough = metal_count(loot, pat) >= info.metal as u32;
                row(p).with_children(|r| {
                    if known {
                        button(r, fonts, pat.name(), Act::Begin(Plan::Forge { pattern: pat, metal: pick_bars(loot, pat, st.steel) }), enough);
                        r.spawn(label(fonts, need, 12.0, if enough { TEXT } else { DIM }, false));
                    } else {
                        r.spawn(label(fonts, format!("{} — не знаете как ({})", pat.name(), info.needs.map(|t| t.name()).unwrap_or("")), 12.0, DIM, false));
                    }
                });
            }
        }
        Tab::Cast => {
            p.spawn(label(fonts, format!("Тигель: {} · каменная форма: {} · воск: {} · глина: {}", have(loot, CRUCIBLE), have(loot, MOULD), have(loot, WAX), have(loot, CLAY)), 13.0, DIM, false));
            let mut cu = 0.0;
            let mut sn = 0.0;
            for (i, m) in CastMetal::ALL.into_iter().enumerate() {
                let d = m.drachms() * st.metals[i] as f32;
                match m {
                    CastMetal::Copper => cu += d,
                    CastMetal::Tin => sn += d,
                    CastMetal::Bronze => {
                        cu += d * 0.9;
                        sn += d * 0.1;
                    }
                    _ => {}
                }
                row(p).with_children(|r| {
                    button(r, fonts, "−", Act::Metal(i, -1), false);
                    r.spawn(label(fonts, format!("{}/{}", st.metals[i], have(loot, m.item())), 14.0, if st.metals[i] > 0 { TITLE } else { TEXT }, true));
                    button(r, fonts, "+", Act::Metal(i, 1), false);
                    r.spawn(label(fonts, format!("{} · {:.0} др. штука", m.item(), m.drachms()), 13.0, TEXT, false));
                });
            }
            let total: f32 = CastMetal::ALL.iter().enumerate().map(|(i, m)| m.drachms() * st.metals[i] as f32).sum();
            let tin = if cu + sn > 0.0 { format!(" · олова в бронзе {:.0}%", sn / (cu + sn) * 100.0) } else { String::new() };
            p.spawn(label(fonts, format!("В тигле: {total:.0} др.{tin}"), 13.0, DIM, false));
            let metals: Vec<(CastMetal, u8)> = CastMetal::ALL.iter().enumerate().filter(|(i, _)| st.metals[*i] > 0).map(|(i, m)| (*m, st.metals[i])).collect();
            for piece in CastPiece::ALL {
                let info = piece.info();
                let known = !info.lost_wax || k.knows(Lore::Technique(Technique::LostWax));
                row(p).with_children(|r| {
                    if known {
                        button(r, fonts, &format!("Лить: {}", info.product.to_lowercase()), Act::Begin(Plan::Cast { piece, metals: metals.clone() }), total >= info.drachms);
                        r.spawn(label(fonts, format!("{:.0} др., {}", info.drachms, if info.lost_wax { "по воску" } else { "в каменную форму" }), 12.0, DIM, false));
                    } else {
                        r.spawn(label(fonts, format!("{} — литьё по воску вам неизвестно", info.product), 12.0, DIM, false));
                    }
                });
            }
            if k.knows(Lore::Technique(Technique::Casting)) {
                p.spawn(label(fonts, "Вы знаете: бронза — девять частей меди на одну олова; лить жёлтым жаром в прогретую форму, ровно и без перерыва.", 13.0, DIM, false));
            }
        }
        Tab::Fletch => {
            use crate::archery::{FEATHERS, Fletch, GLUE, SHAFTS, SINEW};
            p.spawn(label(
                fonts,
                format!(
                    "Верстак у кузни. Древков: {} · перьев: {} · рыбьего клея: {} · сухожилий: {}",
                    have(loot, SHAFTS),
                    have(loot, FEATHERS),
                    have(loot, GLUE),
                    have(loot, SINEW)
                ),
                13.0,
                DIM,
                false,
            ));
            for what in Fletch::all() {
                let inputs = what.inputs();
                let mut enough = inputs.iter().all(|(n, c)| have(loot, n) >= *c as u32);
                let mut need: Vec<String> = inputs.iter().map(|(n, c)| format!("{} ×{c}", n.to_lowercase())).collect();
                if what.needs_glue() {
                    need.push("клей или жилы".into());
                    enough &= have(loot, GLUE) + have(loot, SINEW) > 0;
                }
                let (product, n) = what.product();
                row(p).with_children(|r| {
                    button(r, fonts, &format!("{} ×{n}", product), Act::Begin(Plan::Fletch(what)), enough);
                    r.spawn(label(fonts, need.join(", "), 12.0, if enough { TEXT } else { DIM }, false));
                });
            }
            p.spawn(label(
                fonts,
                "Наконечники куют здесь же (Ковка → Наконечники стрел). Лук тратит стрелы из сумки — ту стопку, что лежит первой.",
                12.0,
                DIM,
                false,
            ));
        }
        Tab::Other => {
            if let Some(inv) = loot.inventory.as_ref() {
                let bar = inv.grid.items.iter().find(|p| p.item.info().name == IRON).map(|p| p.item.uid);
                row(p).with_children(|r| {
                    button(r, fonts, "Цементировать железную полосу", Act::Begin(Plan::Carburize { bar: bar.unwrap_or(0) }), bar.is_some() && have(loot, CHARCOAL) >= 2 && have(loot, POT) >= 1);
                    r.spawn(label(fonts, "полоса, 2 угля, глиняный горшок", 12.0, DIM, false));
                });
                p.spawn(label(fonts, "Починить (выправить и наточить):", 14.0, TITLE, true));
                let mut any = false;
                for item in inv.all_items() {
                    if let Some((d, m)) = item.durability()
                        && d < m * 0.97
                    {
                        any = true;
                        row(p).with_children(|r| {
                            button(r, fonts, &format!("{} {:.0}/{:.0}", item.info().name, d, m), Act::Begin(Plan::Repair { uid: item.uid }), true);
                        });
                    }
                }
                if !any {
                    p.spawn(label(fonts, "Всё цело.", 13.0, DIM, false));
                }
            }
        }
    }
}

fn swing_bar(p: &mut ChildSpawnerCommands, fonts: &UiFonts) {
    swing_bar_hint(p, fonts, "ЛКМ — лёгкий молот, ПКМ — тяжёлый");
}

fn swing_bar_hint(p: &mut ChildSpawnerCommands, fonts: &UiFonts, hint: &str) {
    row(p).with_children(|r| {
        r.spawn(label(fonts, "Замах:", 13.0, DIM, false));
        r.spawn((
            Node {
                width: px(300),
                height: px(18),
                justify_content: JustifyContent::Center,
                ..default()
            },
            theme().bar_frame(),
        ))
        .with_children(|b| {
            b.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(30),
                    height: percent(100),
                    left: percent(35),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.5, 0.75, 0.35, 0.45)),
                Live::Sweet,
            ));
            b.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(4),
                    height: percent(100),
                    left: percent(50),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.95, 0.9, 0.7)),
                Live::Swing,
            ));
        });
        r.spawn(label(fonts, hint, 12.0, DIM, false));
    });
}

fn work_panel(p: &mut ChildSpawnerCommands, fonts: &UiFonts, st: &SmithState, loot: &LootState, k: &crate::lore::Knowledge, job: &Job) {
    if let Job::Quick(q) = job {
        quick_panel(p, fonts, st, loot, q);
        return;
    }
    // Цвет металла.
    row(p).with_children(|r| {
        r.spawn((Node { width: px(70), height: px(26), border: UiRect::all(px(1)), ..default() }, BorderColor::all(BORDER), BackgroundColor(Color::BLACK), Live::HeatSwatch));
        r.spawn((label(fonts, "", 14.0, TEXT, true), Live::HeatText));
    });
    p.spawn((label(fonts, "", 13.0, TEXT, false), Live::Status));
    let knows = |t: Technique| k.knows(Lore::Technique(t));
    match job {
        Job::Forge(f) => {
            let info = f.pattern.info();
            p.spawn(label(fonts, format!("{} · F — в горн / на наковальню", f.pattern.name()), 15.0, TITLE, true));
            swing_bar(p, fonts);
            // Поковка: участки — щёлкать по ним.
            p.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(3), height: px(120), align_items: AlignItems::FlexEnd, ..default() }, BackgroundColor(Color::srgb(0.1, 0.09, 0.08))))
                .with_children(|r| {
                    for (i, name) in info.segments.iter().enumerate() {
                        r.spawn((
                            Segment(i as u8),
                            Interaction::default(),
                            FocusPolicy::Block,
                            Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::FlexEnd, align_items: AlignItems::Center, flex_grow: 1.0, height: percent(100), border: UiRect::all(px(1)), ..default() },
                            BorderColor::all(Color::srgba(0.4, 0.33, 0.22, 0.6)),
                        ))
                        .with_children(|c| {
                            // Черта «как надо» на 1.0 (из 1.3).
                            c.spawn((Node { position_type: PositionType::Absolute, top: percent(100.0 - 100.0 / 1.3), width: percent(100), height: px(2), ..default() }, BackgroundColor(Color::srgba(0.9, 0.85, 0.6, 0.7))));
                            c.spawn((Node { width: percent(70), height: percent(0), ..default() }, BackgroundColor(Color::srgb(0.3, 0.3, 0.3)), Live::SegFill(i as u8)));
                            c.spawn((label(fonts, *name, 11.0, DIM, false), Node { position_type: PositionType::Absolute, bottom: px(2), ..default() }));
                            c.spawn((label(fonts, "", 11.0, TEXT, false), Live::SegText(i as u8), Node { position_type: PositionType::Absolute, top: px(2), ..default() }));
                        });
                    }
                });
            row(p).with_children(|r| {
                button(r, fonts, "В горн", Act::Move(Spot::Fire), f.spot == Spot::Fire);
                button(r, fonts, "На наковальню", Act::Move(Spot::Anvil), f.spot == Spot::Anvil);
                button(r, fonts, "Отпуск у края углей", Act::Move(Spot::Temper), f.spot == Spot::Temper);
                button(r, fonts, "В золу (отжиг)", Act::Move(Spot::Ashes), f.spot == Spot::Ashes);
                button(r, fonts, "На точило", Act::Move(Spot::Grindstone), f.spot == Spot::Grindstone);
                button(r, fonts, "Перевернуть (R)", Act::Flip, false);
            });
            if info.harden {
                row(p).with_children(|r| {
                    r.spawn(label(fonts, "Закалить:", 13.0, TEXT, true));
                    for q in Quench::ALL {
                        button(r, fonts, q.name(), Act::Quench(q), false);
                    }
                });
            }
            if f.spot == Spot::Grindstone {
                p.spawn((
                    GrindPad,
                    Interaction::default(),
                    FocusPolicy::Block,
                    Node {
                        padding: UiRect::all(px(10)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    theme().button_image(true),
                    ThemeButton {
                        enabled: true,
                        on: false,
                    },
                ))
                .with_children(|b| {
                    b.spawn(label(
                        fonts,
                        "Точило: щёлкайте в такт замаху",
                        14.0,
                        TEXT,
                        true,
                    ));
                });
            }
            row(p).with_children(|r| {
                button(r, fonts, "Собрать", Act::Finish(false), f.shaped());
                if info.kind == PieceKind::Weapon && have(loot, POMMEL) > 0 {
                    button(r, fonts, "Собрать с бронзовым навершием", Act::Finish(true), f.shaped());
                }
                button(r, fonts, "Бросить (в лом)", Act::Abandon, false);
            });
            // Подсказки знающему.
            let mut hints = Vec::new();
            if knows(Technique::Forging) {
                hints.push("Ковать от вишнёвого до жёлтого; тяжёлым тянуть, лёгким на неярком жару выглаживать.");
            }
            if info.weld && knows(Technique::ForgeWelding) {
                hints.push("Сначала сварить пакет: белый жар, тяжёлые удары.");
            }
            if info.harden && knows(Technique::Quenching) {
                hints.push("Калить с вишнёвого. Вода жёстче, масло мягче (кувшин масла), рассол (мешок соли) — жёстче воды.");
            }
            if info.harden && knows(Technique::Tempering) {
                hints.push(if info.temper_best < 260.0 { "Отпуск ножа и топора — до соломенного или бронзового." } else { "Отпуск клинка — до пурпурного или синего." });
            }
            for h in hints {
                p.spawn(label(fonts, format!("• {h}"), 12.0, DIM, false));
            }
            if !knows(Technique::Forging) {
                p.spawn(label(fonts, "Вы никогда не ковали: смотрите на цвет металла и пробуйте.", 12.0, DIM, false));
            }
        }
        Job::Cast(c) => {
            p.spawn(label(fonts, format!("Литьё: {} · F — тигель в горн / вынуть", c.piece.info().product), 15.0, TITLE, true));
            row(p).with_children(|r| {
                r.spawn(label(fonts, "Залито:", 13.0, DIM, false));
                bar(r, 200.0, Color::srgb(1.0, 0.7, 0.3), Live::Progress);
            });
            row(p).with_children(|r| {
                button(r, fonts, "Тигель в горн", Act::Move(Spot::Fire), c.spot == Spot::Fire);
                button(r, fonts, "Снять шлак", Act::Skim, false);
                button(
                    r,
                    fonts,
                    if c.mould_warm {
                        "Убрать форму от горна"
                    } else {
                        "Поставить форму греться"
                    },
                    Act::Warm(!c.mould_warm),
                    c.mould_warm,
                );
                r.spawn((
                    PourButton,
                    Interaction::default(),
                    FocusPolicy::Block,
                    Node {
                        padding: UiRect::axes(px(14), px(6)),
                        ..default()
                    },
                    theme().button_image(true),
                    ThemeButton {
                        enabled: true,
                        on: false,
                    },
                ))
                .with_children(|b| {
                    b.spawn(label(fonts, "ЛИТЬ (держать)", 13.0, TEXT, true));
                });
                button(r, fonts, "Раскрыть форму", Act::OpenMould, false);
            });
            if c.opened {
                swing_bar(p, fonts);
                p.spawn((
                    Segment(0),
                    Interaction::default(),
                    FocusPolicy::Block,
                    Node {
                        padding: UiRect::all(px(12)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    theme().button_image(true),
                    ThemeButton {
                        enabled: true,
                        on: false,
                    },
                ))
                .with_children(|b| {
                    b.spawn(label(
                        fonts,
                        "Отливка: чеканить и опиливать (щёлкать в такт)",
                        13.0,
                        TEXT,
                        true,
                    ));
                });
            }
            row(p).with_children(|r| {
                button(r, fonts, "Готово", Act::Finish(false), c.opened);
                button(r, fonts, "Бросить (в лом)", Act::Abandon, false);
            });
        }
        Job::Soak(s) => {
            let what = match s.kind {
                SoakKind::Bloomery { ore, charcoal, rich } => format!("Плавка: руды {ore} (богатость {rich}%), угля {charcoal}"),
                SoakKind::Carburize => "Цементация полосы".into(),
            };
            p.spawn(label(fonts, format!("{what} · качайте мехи пробелом в такт"), 15.0, TITLE, true));
            row(p).with_children(|r| {
                r.spawn(label(fonts, "Прошло:", 13.0, DIM, false));
                bar(r, 200.0, Color::srgb(0.8, 0.6, 0.3), Live::Progress);
                r.spawn(label(fonts, "Дутьё:", 13.0, DIM, false));
                bar(r, 90.0, Color::srgb(1.0, 0.5, 0.15), Live::Blast);
            });
            let (lo, hi) = s.window();
            let (cl, nl) = heat_color(lo);
            let (_, nh) = heat_color(hi);
            if knows(Technique::Smelting) || knows(Technique::Carburizing) {
                row(p).with_children(|r| {
                    r.spawn((Node { width: px(18), height: px(14), ..default() }, BackgroundColor(Color::srgb(cl[0], cl[1], cl[2]))));
                    r.spawn(label(fonts, format!("Нужный жар: от «{nl}» до «{nh}»"), 13.0, DIM, false));
                });
            }
            row(p).with_children(|r| {
                button(r, fonts, if matches!(s.kind, SoakKind::Bloomery { .. }) { "Разломать горн, вынуть крицу" } else { "Разбить горшок" }, Act::Finish(false), s.done());
                button(r, fonts, "Бросить", Act::Abandon, false);
            });
        }
        Job::Repair { item, .. } => {
            p.spawn(label(fonts, format!("Ремонт: {}", item.info().name), 15.0, TITLE, true));
            row(p).with_children(|r| {
                bar(r, 200.0, Color::srgb(0.6, 0.8, 0.5), Live::Progress);
            });
            swing_bar(p, fonts);
            p.spawn((
                GrindPad,
                Interaction::default(),
                FocusPolicy::Block,
                Node {
                    padding: UiRect::all(px(12)),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                theme().button_image(true),
                ThemeButton {
                    enabled: true,
                    on: false,
                },
            ))
            .with_children(|b| {
                b.spawn(label(
                    fonts,
                    "Править и точить: щёлкайте в такт",
                    14.0,
                    TEXT,
                    true,
                ));
            });
            button(p, fonts, "Бросить и забрать вещь", Act::Abandon, false);
        }
        Job::Fletch { what, .. } => {
            p.spawn(label(fonts, what.verb(), 15.0, TITLE, true));
            row(p).with_children(|r| {
                bar(r, 200.0, Color::srgb(0.75, 0.65, 0.4), Live::Progress);
            });
            swing_bar(p, fonts);
            let hint = match what {
                crate::archery::Fletch::Shafts => "Колоть, строгать и править древки: щёлкайте в такт",
                crate::archery::Fletch::Arrows(_) => "Насадить наконечник, вклеить перья, обмотать: щёлкайте в такт",
            };
            p.spawn((
                GrindPad,
                Interaction::default(),
                FocusPolicy::Block,
                Node {
                    padding: UiRect::all(px(12)),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                theme().button_image(true),
                ThemeButton {
                    enabled: true,
                    on: false,
                },
            ))
            .with_children(|b| {
                b.spawn(label(fonts, hint, 14.0, TEXT, true));
            });
            button(
                p,
                fonts,
                "Бросить (материал вернётся, клей — нет)",
                Act::Abandon,
                false,
            );
        }
        Job::Quick(_) => {}
    }
    let _ = st;
}

/// Живые части: маятник, цвет металла, участки, уголь.
fn update_live(state: Res<SmithState>, knowledge: Res<MyKnowledge>, mut nodes: Query<(&Live, Option<&mut Node>, Option<&mut BackgroundColor>, Option<&mut Text>, Option<&mut TextColor>)>) {
    if !state.open {
        return;
    }
    let k = &knowledge.0;
    let skill = k.skill(Skill::Smithing);
    let swing = swing_pos(state.swing_t, false);
    let (fuel, blast) = state.work.as_ref().map(|w| w.station).or(state.near).and_then(|id| state.fires.get(&id).copied()).unwrap_or((0.0, 0.0));
    let named = k.knows(Lore::Technique(Technique::Forging));
    let job = state.pred.as_ref();
    let temp = match job {
        Some(Job::Forge(f)) => f.temp,
        Some(Job::Cast(c)) => c.temp,
        Some(Job::Soak(s)) => s.temp,
        _ => AMBIENT,
    };
    let (col, name) = heat_color(temp);
    let col = Color::srgb(col[0], col[1], col[2]);
    for (live, node, bg, text, tc) in &mut nodes {
        match live {
            Live::Swing => {
                if let Some(mut n) = node {
                    n.left = percent(50.0 + swing * 48.0);
                }
            }
            Live::Sweet => {
                // Ширина «хорошего удара» меняется только с навыком — не раскладываем окно зря.
                if let Some(mut n) = node {
                    let w = sweet_width(skill) * 0.5 * 48.0;
                    let (left, width) = (percent(50.0 - w), percent(w * 2.0));
                    if n.left != left || n.width != width {
                        n.left = left;
                        n.width = width;
                    }
                }
            }
            Live::HeatSwatch => {
                if let Some(mut b) = bg {
                    // Закалённая и зачищенная сталь при отпуске — цвета побежалости.
                    b.set_if_neq(BackgroundColor(match job {
                        Some(Job::Forge(f)) if f.quenched && f.temp < 400.0 => {
                            let (c, _) = temper_color(f.temper.max(f.temp));
                            Color::srgb(c[0], c[1], c[2])
                        }
                        _ => col,
                    }));
                }
            }
            Live::HeatText => {
                if let Some(mut t) = text {
                    let s = match job {
                        Some(Job::Forge(f)) if f.quenched && f.temp < 400.0 => {
                            let shown = f.temper.max(f.temp);
                            if k.knows(Lore::Technique(Technique::Tempering)) { format!("Побежалость: {}", temper_color(shown).1) } else { "Закалено. Цвет на зачищенной стали меняется от жара".into() }
                        }
                        Some(Job::Cast(c)) => {
                            let state = if c.melt >= 1.0 { "расплав жидкий" } else if c.melt > 0.0 { "плавится" } else { "твёрдый" };
                            format!("{}{} · {state}, шлак {:.0}%, форма прогрета {:.0}%", if named { name } else { "металл" }, if c.spot == Spot::Fire { " (в горне)" } else { "" }, c.dross * 100.0, c.mould_heat * 100.0)
                        }
                        Some(Job::Forge(f)) => {
                            let where_ = match f.spot {
                                Spot::Fire => "в горне",
                                Spot::Anvil => "на наковальне",
                                Spot::Temper => "у края углей",
                                Spot::Ashes => "в золе",
                                Spot::Grindstone => "на точиле",
                            };
                            if named { format!("{} — {where_}", name) } else { format!("Светится вот так — {where_}") }
                        }
                        _ => if named { name.to_string() } else { String::new() },
                    };
                    if t.0 != s {
                        t.0 = s;
                    }
                }
            }
            Live::Fuel => {
                if let Some(mut t) = text {
                    let s = if fuel <= 0.0 { "Горн погас: нужен уголь".to_string() } else { format!("Угля в горне на {}", crate::lore::fmt_secs(fuel)) };
                    if t.0 != s {
                        t.0 = s;
                    }
                }
            }
            Live::Blast => {
                if let Some(mut n) = node {
                    let b = match job {
                        Some(Job::Soak(s)) => s.blast,
                        _ => blast,
                    };
                    let w = percent(b * 100.0);
                    if n.width != w {
                        n.width = w;
                    }
                }
            }
            Live::SegFill(i) => {
                if let (Some(mut n), Some(Job::Forge(f))) = (node, job) {
                    let s = f.shape.get(*i as usize).copied().unwrap_or(0.0);
                    let h = percent((s / 1.3 * 100.0).clamp(2.0, 100.0));
                    if n.height != h {
                        n.height = h;
                    }
                    if let Some(mut b) = bg {
                        b.set_if_neq(BackgroundColor(if temp > 480.0 { col } else { Color::srgb(0.32, 0.31, 0.3) }));
                    }
                }
            }
            Live::SegText(i) => {
                if let (Some(mut t), Some(Job::Forge(f))) = (text, job) {
                    let s = f.shape.get(*i as usize).copied().unwrap_or(0.0);
                    let v = if named { format!("{:.0}%", s * 100.0) } else { String::new() };
                    if t.0 != v {
                        t.0 = v;
                    }
                    if let Some(mut c) = tc {
                        c.set_if_neq(TextColor(if s > 1.2 { BAD } else { TEXT }));
                    }
                }
            }
            Live::Status => {
                if let Some(mut t) = text {
                    let s = match job {
                        Some(Job::Forge(f)) => {
                            let mut v = Vec::new();
                            if f.weld < 1.0 {
                                v.push(format!("пакет сварен на {:.0}%", f.weld * 100.0));
                            }
                            if f.ground > 0.0 {
                                v.push(format!("заточено {:.0}%", f.ground.min(1.0) * 100.0));
                            }
                            if f.cracks > 0.1 {
                                v.push("есть трещины".into());
                            }
                            if f.dents > 0.2 {
                                v.push("в забоинах".into());
                            }
                            if f.scale > 0.2 {
                                v.push("окалина".into());
                            }
                            if f.lean.abs() > crate::smithing::LEAN_SAFE * 0.7 {
                                v.push(if f.lean.abs() > crate::smithing::LEAN_SAFE { "заготовку ведёт — переверните (R)!".into() } else { "заготовку начинает вести".into() });
                            }
                            v.push(format!("ударов {}", f.strikes));
                            v.join(" · ")
                        }
                        Some(Job::Soak(s)) => format!("В нужном жару {:.0}% времени, осталось {}", s.steadiness() * 100.0, crate::lore::fmt_secs((s.total - s.elapsed).max(0.0))),
                        Some(Job::Cast(c)) => if c.opened { format!("Отделка {:.0}%", c.chased.min(1.0) * 100.0) } else if c.pouring { "Льётся…".into() } else { String::new() },
                        _ => String::new(),
                    };
                    if t.0 != s {
                        t.0 = s;
                    }
                }
            }
            Live::Progress => {
                if let Some(mut n) = node {
                    let v = match job {
                        Some(Job::Cast(c)) => c.poured,
                        Some(Job::Soak(s)) => s.elapsed / s.total,
                        Some(Job::Repair { progress, .. }) | Some(Job::Fletch { progress, .. }) => *progress,
                        _ => 0.0,
                    };
                    let w = percent(v.clamp(0.0, 1.0) * 100.0);
                    if n.width != w {
                        n.width = w;
                    }
                }
            }
        }
    }
}

fn update_prompt(state: Res<SmithState>, mut text: Single<&mut Text, With<SmithPrompt>>) {
    let line = match (state.open, state.near) {
        (false, Some(id)) => state.station(id).map(|s| {
            if state.work.as_ref().is_some_and(|w| w.station == id) { format!("[B] {} — продолжить работу", s.name) } else { format!("[B] {} — работать у горна", s.name) }
        }),
        _ => None,
    }
    .unwrap_or_default();
    if text.0 != line {
        text.0 = line;
    }
}

// ---------------------------------------------------------------------------
// Поковка в мире: светится в горне или на наковальне
// ---------------------------------------------------------------------------

/// Цвет каления для подписи в подсказках других окон.
#[allow(dead_code)]
pub fn quality_word(q: Quality) -> &'static str {
    q.name()
}

// ---------------------------------------------------------------------------
// Ковка на удачу
// ---------------------------------------------------------------------------

/// Удар молотом по заготовке: точность — по маятнику в миг нажатия.
fn quick_strike(state: &mut SmithState, client: &mut RenetClient, knowledge: &MyKnowledge, now: f32) {
    if now - state.last_strike < quick::QUICK_GAP {
        return;
    }
    if !matches!(&state.pred, Some(Job::Quick(q)) if !q.shattered) {
        return;
    }
    state.last_strike = now;
    let acc = accuracy(swing_pos(state.swing_t, false), knowledge.0.skill(Skill::Smithing));
    send(client, &SmithClientMessage::Strike { zone: 0, heavy: false, acc });
    // Маятник начинает новый замах.
    state.swing_t = SWING_PERIOD[0] * 0.25;
}

fn money(n: u32) -> String {
    crate::economy::format_money(n)
}

fn quality_color(q: Quality) -> Color {
    let c = q.color();
    Color::srgb(c[0], c[1], c[2])
}

/// Цена кованой вещи такого качества (вся стопка — у наконечников дюжина).
fn quick_price(p: Pattern, q: Quality) -> u32 {
    let info = p.info();
    let base = crate::items::ItemBaseId::find(info.product).map_or(0, crate::economy::base_price);
    crate::economy::round_price(base as f32 * crate::economy::quality_price(q)) * info.count.max(1) as u32
}

fn product_label(p: Pattern) -> String {
    let info = p.info();
    if info.count > 1 { format!("{} ×{}", info.product, info.count) } else { info.product.to_string() }
}

fn measures(n: u32) -> &'static str {
    crate::economy::plural(n, "мера", "меры", "мер")
}

/// Сколько в сумке кованых вещей не хуже `min` (только того вида, если `only`). Стопка — одна вещь,
/// как и считает мастер.
fn count_products(loot: &LootState, min: Quality, only: Option<&str>) -> u32 {
    loot.inventory.as_ref().map_or(0, |inv| {
        inv.grid
            .items
            .iter()
            .filter(|p| quick::is_quick_product(p.item.info().name) && p.item.quality >= min && only.is_none_or(|o| o == p.item.info().name))
            .count() as u32
    })
}

fn section(p: &mut ChildSpawnerCommands, fonts: &UiFonts, title: &str, about: &str) {
    theme().divider(p);
    p.spawn(label(fonts, title, 17.0, TITLE, true));
    if !about.is_empty() {
        p.spawn(label(fonts, about, 12.0, DIM, false));
    }
}

/// Вкладка «На удачу»: что сковать, заказ мастера, клейма.
fn quick_menu(p: &mut ChildSpawnerCommands, fonts: &UiFonts, st: &SmithState, loot: &LootState) {
    let units = quick::iron_units(|n| have(loot, n));
    p.spawn(label(
        fonts,
        "Заготовку греет мастер — вам только бить в такт. Удар поднимает цену вещи, но копит напряжение: следующий может расколоть металл. Снимите вещь вовремя.",
        13.0,
        TEXT,
        false,
    ));
    p.spawn(label(fonts, format!("Железа в сумке: {units} {} · полоса или крица — мера, лом и руда — две штуки на меру", measures(units)), 13.0, if units > 0 { ink::GOLD } else { BAD }, false));
    for pat in quick::QUICK_PATTERNS {
        let need = quick::iron_needed(pat) as u32;
        let ok = units >= need;
        row(p).with_children(|r| {
            r.spawn(Node { width: px(190), ..default() }).with_children(|c| {
                button(c, fonts, &product_label(pat), Act::Begin(Plan::Quick { pattern: pat }), ok);
            });
            r.spawn(label(fonts, format!("{need} {} · от {} до {}", measures(need), money(quick_price(pat, Quality::Poor)), money(quick_price(pat, Quality::Legendary))), 12.0, if ok { TEXT } else { DIM }, false));
        });
    }
    section(p, fonts, "Заказ мастера", "Мастер платит за заказ больше, чем дал бы торговец. Сдали — он придумает следующий, строже.");
    if let Some(o) = st.guild.order {
        let product = o.pattern.info().product;
        let has = count_products(loot, o.min, Some(product)) > 0;
        row(p).with_children(|r| {
            r.spawn(label(fonts, format!("{} не хуже «{}»", product_label(o.pattern), o.min.name().to_lowercase()), 14.0, quality_color(o.min), true));
            r.spawn(label(fonts, format!("плата {}", money(o.reward)), 14.0, ink::GOLD, true));
        });
        row(p).with_children(|r| {
            button(r, fonts, "Сдать заказ", Act::Deliver, has);
            button(r, fonts, "Другой заказ", Act::NewOrder, false);
            r.spawn(label(fonts, if has { "нужная вещь есть в сумке".to_string() } else { format!("сдано заказов: {}", st.guild.delivered) }, 12.0, if has { ink::GOOD } else { DIM }, false));
        });
    } else {
        p.spawn(label(fonts, "Мастер думает над заказом…", 12.0, DIM, false));
    }
    section(p, fonts, "Клейма мастера", "Мастер выбивает клеймо на вашем молоте в обмен на кованые вещи. Клейма действуют на каждую заготовку.");
    let m = &st.guild.marks;
    for mark in Mark::ALL {
        let (n, min) = mark.cost();
        let owned = mark.owned(m);
        let can = !owned && count_products(loot, min, None) >= n as u32;
        row(p).with_children(|r| {
            r.spawn(Node { width: px(165), ..default() }).with_children(|c| {
                let name = if mark == Mark::Ward { format!("{} {}/{}", mark.name(), m.wards, quick::WARDS_MAX) } else { mark.name().to_string() };
                c.spawn(label(fonts, name, 14.0, if owned { ink::GOOD } else { TEXT }, true));
            });
            let cost = if n == 1 { "цена: 1 любая кованая вещь".to_string() } else { format!("цена: {n} вещи не хуже «{}»", min.name().to_lowercase()) };
            r.spawn(Node { width: px(250), flex_direction: FlexDirection::Column, ..default() }).with_children(|c| {
                c.spawn(label(fonts, mark.about(), 12.0, TEXT, false));
                c.spawn(label(fonts, cost, 11.0, DIM, false));
            });
            if owned && mark != Mark::Ward {
                r.spawn(label(fonts, "есть", 13.0, ink::GOOD, true));
            } else {
                button(r, fonts, "Выменять", Act::BuyMark(mark), can);
            }
        });
    }
}

/// Заготовка на наковальне: цена, риск, раж, маятник и молот.
fn quick_panel(p: &mut ChildSpawnerCommands, fonts: &UiFonts, st: &SmithState, loot: &LootState, q: &QuickForge) {
    let _ = loot;
    let quality = q.quality();
    p.spawn(label(fonts, format!("{} · ковка на удачу · ударов {}", product_label(q.pattern), q.strikes), 16.0, TITLE, true));
    // Цена сейчас и путь до следующего качества.
    row(p).with_children(|r| {
        r.spawn(label(fonts, "Сейчас:", 14.0, DIM, false));
        r.spawn(label(fonts, quality.name(), 20.0, quality_color(quality), true));
        r.spawn(label(fonts, format!("цена около {}", money(quick_price(q.pattern, quality))), 15.0, ink::GOLD, true));
    });
    row(p).with_children(|r| {
        match q.next_quality() {
            Some((next, k)) => {
                r.spawn((label(fonts, format!("до «{}»", next.name().to_lowercase()), 13.0, DIM, false), Node { width: px(150), ..default() }));
                theme().bar(r, Node { width: px(300), height: px(18), ..default() }, k, quality_color(next), ());
                r.spawn(label(fonts, format!("{:.0}%", k * 100.0), 13.0, TEXT, false));
            }
            None => {
                r.spawn(label(fonts, "Выше не бывает — снимайте!", 14.0, quality_color(Quality::Legendary), true));
            }
        }
    });
    // Риск — или раж.
    row(p).with_children(|r| {
        if q.raging() {
            r.spawn(label(fonts, format!("РАЖ! Ещё {} {} — металл не треснет, цена растёт быстрее", q.rage, crate::economy::plural(q.rage as u32, "удар", "удара", "ударов")), 16.0, Color::srgb(1.0, 0.62, 0.2), true));
        } else {
            let risk = q.risk;
            let col = if risk < 0.12 { ink::GOOD } else if risk < 0.3 { ink::GOLD } else { BAD };
            r.spawn((label(fonts, "Риск трещины", 13.0, DIM, false), Node { width: px(150), ..default() }));
            theme().bar(r, Node { width: px(300), height: px(18), ..default() }, risk / quick::RISK_MAX, col, ());
            r.spawn(label(fonts, format!("{:.0}%", risk * 100.0), 15.0, col, true));
        }
    });
    row(p).with_children(|r| {
        if !q.raging() {
            // Серия точных ударов: зажжённые угольки — сколько уже есть.
            r.spawn(label(fonts, "Точных подряд до ража:", 13.0, DIM, false));
            for i in 0..q.rage_need() {
                let lit = i < q.streak;
                r.spawn((
                    Node { width: px(14), height: px(14), border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(7)), ..default() },
                    BorderColor::all(Color::srgb(1.0, 0.62, 0.2)),
                    BackgroundColor(if lit { Color::srgb(1.0, 0.62, 0.2) } else { Color::srgba(0.0, 0.0, 0.0, 0.35) }),
                ));
            }
        }
        if q.marks.wards > 0 {
            r.spawn(label(fonts, format!("Оберегов наковальни: {}", q.marks.wards), 13.0, ink::GOOD, false));
        }
    });
    swing_bar_hint(p, fonts, "бейте, когда черта в зелёной середине");
    p.spawn(Node { justify_content: JustifyContent::Center, padding: UiRect::vertical(px(4)), ..default() }).with_children(|c| {
        theme().button_sized(c, "Ударить молотом  (пробел)", 20.0, (SmithButton(Act::QuickStrike), FocusPolicy::Block), true).insert(Node {
            padding: UiRect::axes(px(28), px(10)),
            min_height: px(46),
            min_width: px(360),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        });
    });
    if let Some(h) = q.last {
        let col = match h {
            quick::Hit::Rage => Color::srgb(1.0, 0.62, 0.2),
            quick::Hit::Glancing | quick::Hit::RageLost => BAD,
            quick::Hit::Saved => ink::GOOD,
            _ => TEXT,
        };
        p.spawn(label(fonts, quick::hit_text(h), 13.0, col, false));
    }
    row(p).with_children(|r| {
        button(r, fonts, &format!("Снять с наковальни — {}", quality.name().to_lowercase()), Act::Finish(false), q.strikes > 0);
        button(r, fonts, "Бросить (в лом)", Act::Abandon, false);
    });
    if let Some(o) = st.guild.order {
        p.spawn(label(fonts, format!("Заказ мастера: {} не хуже «{}» — {}", product_label(o.pattern), o.min.name().to_lowercase(), money(o.reward)), 12.0, DIM, false));
    }
}

/// Снимок окна (`MRPG_SHOT_UI=quick`, `quick:rage`, `quick:risk`, `quick:menu`): окно «на удачу»
/// у горна без сервера — горн с карты, заготовка и заказ придуманы.
fn shot_quick_forge(
    mut commands: Commands,
    mut state: ResMut<SmithState>,
    mut loot: ResMut<LootState>,
    panel: Query<Entity, With<SmithPanel>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok(want) = std::env::var("MRPG_SHOT_UI") else {
        *done = true;
        return;
    };
    let Some(rest) = want.strip_prefix("quick") else {
        *done = true;
        return;
    };
    let Ok(entity) = panel.single() else { return };
    *done = true;
    // Снимок прячет окна без метки — это окно оставить (и вернуть, если уже спрятано).
    commands.entity(entity).insert((crate::civil_client::ShotKeep, Visibility::Visible));
    let sid: u32 = std::env::var("MRPG_SHOT_FORGE").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    state.demo(sid, None, 0.2);
    state.near = Some(sid);
    state.open = true;
    state.tab = Tab::Quick;
    loot.ui_open = true;
    // Сумка: руда с шахты, пара кованых вещей.
    let mut inv = crate::inventory::Inventory::new((10, 8));
    let mut uids = crate::items::UidGen(9000);
    for (name, q, n) in [(ORE, Quality::Common, 10), (IRON, Quality::Common, 1), ("Спата", Quality::Uncommon, 1), ("Кинжал", Quality::Common, 2)] {
        if let Some(base) = crate::items::ItemBaseId::find(name) {
            let _ = inv.place(crate::items::plain_item(base, q, n, &mut uids), crate::inventory::Target::Auto);
        }
    }
    loot.inventory = Some(inv);
    loot.rev += 1;
    let marks = quick::Marks { steady: true, wards: 1, ..Default::default() };
    state.guild = GuildView { marks, order: Some(quick::Order::new(Pattern::Spatha, Quality::Uncommon)), delivered: 2 };
    let mut q = QuickForge::new(Pattern::Spatha, 0.18, marks);
    match rest.trim_start_matches(':') {
        "menu" => {}
        // Низ вкладки: заказ и клейма мастера.
        "marks" => {
            commands.entity(entity).insert(ScrollPosition(Vec2::new(0.0, 330.0)));
        }
        mode => {
            // Удары с заданной точностью, пока не выйдет нужное состояние.
            let mut rng = crate::items::Rng::new(11);
            let plan: &[f32] = match mode {
                "rage" => &[0.6, 0.9, 0.95, 0.9, 0.85],
                "risk" => &[0.8, 0.5, 0.9, 0.4, 0.6, 0.5, 0.7, 0.5, 0.6, 0.55],
                _ => &[0.9, 0.6, 0.85, 0.5, 0.7, 0.8],
            };
            for a in plan {
                let before = q.clone();
                q.strike(*a, 20.0, &mut rng);
                if q.shattered {
                    // Для снимка — без раскола: тот же удар, но удачный.
                    q = before;
                    q.risk = 0.0;
                    q.strike(*a, 20.0, &mut rng);
                }
            }
            let job = Job::Quick(q);
            state.work = Some(WorkView { station: sid, job: job.clone(), bellows: false });
            state.pred = Some(job);
        }
    }
    state.rev += 1;
}
