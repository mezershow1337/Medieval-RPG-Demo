//! Клиент: работа руками от первого лица (см. work.rs).
//!
//! Пока идёт занятие с движением, мышь не крутит голову и не машет оружием — она
//! работает: водит пестик, мешает ложкой, качает мехи, бьёт молотом, ведёт указку по
//! строке, тянет повязку, повторяет движения мастера. Перед глазами — инструмент в руках,
//! сбоку — маленькая шкала с подсказкой. Каждый законченный мазок уходит на сервер.

use crate::inventory_ui::UiFonts;
use crate::loot_client::LootState;
use crate::player::PlayerCamera;
use crate::protocol::encode;
use crate::work::{Dir, Drill, Input, Meter};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;

pub struct WorkClientPlugin;

impl Plugin for WorkClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MyWork::default())
            .add_systems(Startup, (setup_work_hud.after(crate::inventory_ui::setup_fonts), setup_tool_assets))
            .add_systems(Update, (demo_work, drive_meter, update_work_hud, spawn_tool, animate_tool).chain().in_set(WorkSet));
    }
}

/// Системы работы руками (табличка рисуется после них).
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct WorkSet;

/// Через какое окно идёт работа: занятия знаний или урок/смена у торговца-мастера.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Via {
    #[default]
    Lore,
    Trade,
}

/// Своя текущая работа руками.
#[derive(Resource, Default)]
pub struct MyWork {
    pub meter: Option<Meter>,
    pub via: Via,
    /// Сколько сделано 0..1 (с сервера).
    pub progress: f32,
    pub verb: String,
    seed: u32,
    /// Только что взялись за работу: закрыть сумку и поймать мышь.
    fresh: bool,
    /// Показ для снимков (`MRPG_WORK_DEMO`): руки двигаются сами.
    demo: bool,
}

/// `MRPG_WORK_DEMO=grind|stir|pump|rhythm|track|tension|mimic|trace|scribe|count` — для снимков экрана: сразу
/// взяться за работу, и руки сами делают её (сервер ничего не знает).
fn demo_work(mut work: ResMut<MyWork>, mut done: Local<bool>, time: Res<Time>) {
    if *done || time.elapsed_secs() < 2.0 {
        return;
    }
    *done = true;
    let Ok(name) = std::env::var("MRPG_WORK_DEMO") else { return };
    let (drill, verb) = match name.as_str() {
        "grind" => (Drill::Grind, "Ступка: растираете"),
        "stir" => (Drill::Stir, "Глиняный горшок: варите"),
        "pump" => (Drill::Pump, "Алембик: перегоняете"),
        "rhythm" => (Drill::Rhythm, "Урок: ковка — бейте в такт с мастером"),
        "track" => (Drill::Track, "Чтение"),
        "tension" => (Drill::Tension, "Перевязываете рану"),
        "mimic" => (Drill::Mimic, "Урок: отвары — повторяйте за мастером"),
        "trace" => (Drill::Trace, "Урок: азбука — выводите буквы на воске"),
        "scribe" => (Drill::Scribe, "Урок: письмо — пишите слово на табличке"),
        "count" => (Drill::Count, "Урок: счёт — откладывайте камешки на абаке"),
        _ => return,
    };
    work.begin(Some(drill), 0.3, Via::Lore, verb, 0.0);
    work.demo = true;
}

/// Движения «сами собой» для показа: почти правильно, с лёгкой неровностью.
fn demo_input(m: &Meter, dt: f32) -> Input {
    let t = m.t + dt;
    let mut inp = Input { dt, ..Input::default() };
    match m.drill {
        Drill::Grind | Drill::Stir => {
            let rev = m.drill.period() * 1.1;
            let p = |t: f32| {
                let a = t / rev * std::f32::consts::TAU;
                Vec2::new(a.cos(), a.sin()) * 0.85
            };
            let d = p(t) - if m.t <= 0.0 { Vec2::ZERO } else { p(m.t) };
            inp.delta = Vec2::new(d.x, -d.y) * 160.0;
        }
        Drill::Pump => {
            let w = std::f32::consts::TAU / 1.1;
            let v = -(t * w).sin() * w * 1.2 * dt;
            inp.delta.y = -v / (1.4 / 160.0);
        }
        Drill::Rhythm => inp.lmb_pressed = m.swing().abs() < 0.08,
        Drill::Track => inp.delta.x = ((m.target - m.cursor) * 0.25 + 0.002) * 320.0,
        Drill::Tension => inp.lmb_held = m.tension < m.tension_center() + 0.03,
        Drill::Trace | Drill::Scribe => {
            // Рука идёт за образцом, чуть дрожа.
            if let Some(goal) = m.nib_goal() {
                let wobble = Vec2::new((t * 7.0).sin(), (t * 5.3).cos()) * 0.01;
                let d = (goal + wobble - m.nib).clamp_length_max(0.55 * dt);
                inp.delta = Vec2::new(d.x, -d.y) * crate::work::NIB_PX;
            }
        }
        Drill::Count => {
            let want = m.digits();
            if m.col < 3 {
                if m.pebbles[m.col] < want[m.col] {
                    inp.delta.y = -crate::work::PEBBLE_PX * 4.0 * dt;
                } else if (t * 1.2) as i32 != (m.t * 1.2) as i32 {
                    inp.lmb_pressed = true;
                }
            }
        }
        Drill::Mimic => {
            if !m.showing && (t * 2.5) as i32 != (m.t * 2.5) as i32 {
                let v = m.seq.get(m.seq_i).map(|d| d.vec()).unwrap_or_default() * 120.0;
                inp.delta = Vec2::new(v.x, -v.y);
            }
        }
    }
    inp
}

impl MyWork {
    /// Началось занятие. Без движения (`drill` None) — просто ждём, как раньше.
    pub fn begin(&mut self, drill: Option<Drill>, skill: f32, via: Via, verb: &str, progress: f32) {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) ^ (progress.to_bits() | 0x9E37);
        self.meter = drill.map(|d| Meter::new(d, skill, self.seed));
        self.via = via;
        self.verb = verb.to_string();
        self.progress = progress.clamp(0.0, 1.0);
        self.fresh = self.meter.is_some();
    }

    /// Занятие кончилось (только если оно шло через это же окно).
    pub fn end(&mut self, via: Via) {
        if self.via == via {
            self.meter = None;
        }
    }

    /// Руки заняты работой: мышь не крутит голову и не бьёт.
    pub fn busy(&self) -> bool {
        self.meter.is_some()
    }
}

/// Для других модулей: заняты ли руки работой.
pub fn hands_busy(work: Option<&MyWork>) -> bool {
    work.is_some_and(|w| w.busy())
}

#[allow(clippy::too_many_arguments)]
fn drive_meter(
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    console: Option<Res<crate::console::Console>>,
    mut loot: ResMut<LootState>,
    trade: Option<Res<crate::trade_client::TradeState>>,
    mut work: ResMut<MyWork>,
    mut client: Option<ResMut<RenetClient>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // Взялись за работу (например, из окна варки): сумку закрыть, мышь — в руки.
    if work.fresh {
        work.fresh = false;
        if loot.ui_open {
            loot.ui_open = false;
            loot.rev += 1;
        }
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    let via = work.via;
    if work.meter.is_none() {
        return;
    }
    // Открыто окно — мышь у окна, работа стоит.
    if loot.ui_open || trade.is_some_and(|t| t.window_open()) {
        return;
    }
    // Стрелки — вместо мыши (кому мышью неудобно). В открытой консоли стрелки — её.
    let typing = console.is_some_and(|c| c.open);
    let held = |k: KeyCode| if keys.pressed(k) && !typing { 1.0 } else { 0.0 };
    let arrows = Vec2::new(
        held(KeyCode::ArrowRight) - held(KeyCode::ArrowLeft),
        held(KeyCode::ArrowUp) - held(KeyCode::ArrowDown),
    );
    let arrow = [
        (KeyCode::ArrowUp, Dir::Up),
        (KeyCode::ArrowDown, Dir::Down),
        (KeyCode::ArrowLeft, Dir::Left),
        (KeyCode::ArrowRight, Dir::Right),
    ]
    .into_iter()
    .find(|(k, _)| keys.just_pressed(*k) && !typing)
    .map(|(_, d)| d);
    let enter =
        !typing && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter));
    let mut inp = Input {
        dt: time.delta_secs(),
        delta: motion.delta,
        lmb_pressed: mouse.just_pressed(MouseButton::Left),
        lmb_held: mouse.pressed(MouseButton::Left),
        arrows,
        arrow,
        enter,
    };
    if work.demo {
        inp = demo_input(work.meter.as_ref().expect("есть"), inp.dt);
    }
    let Some(m) = work.meter.as_mut() else { return };
    let Some(stroke) = m.update(inp) else { return };
    let Some(client) = client.as_mut() else { return };
    match via {
        Via::Lore => crate::lore_client::send(client, &crate::lore_net::LoreClientMessage::Stroke { q: stroke.q }),
        Via::Trade => client.send_message(crate::trade_net::CH_TRADE, encode(&crate::trade_net::TradeClientMessage::Stroke { q: stroke.q })),
    }
}

// ---------------------------------------------------------------------------
// Шкала сбоку
// ---------------------------------------------------------------------------

const GAUGE_W: f32 = 300.0;
const GAUGE_H: f32 = 130.0;

#[derive(Component)]
struct WorkHud;

/// Поле шкалы (у письма и счёта его нет — там табличка).
#[derive(Component)]
struct GaugeBox;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Title,
    Hint,
    Note,
    Track,
    Zone,
    Fill,
    Ring,
    Ghost,
    Dot,
    Arrows,
    Bar,
}

fn text(fonts: &UiFonts, size: f32, bold: bool) -> (Text, TextFont, TextColor, TextShadow) {
    (
        Text::new(""),
        TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: bevy::text::FontSize::Px(size), ..default() },
        TextColor(Color::srgb(0.95, 0.9, 0.78)),
        TextShadow::default(),
    )
}

fn setup_work_hud(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn((
            WorkHud,
            crate::civil_client::ShotKeep,
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                right: px(36),
                bottom: px(150),
                width: px(GAUGE_W + 32.0),
                padding: crate::ui_theme::Skin::Leather.padding(),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
            crate::ui_theme::theme().skin(crate::ui_theme::Skin::Leather),
        ))
        .with_children(|p| {
            p.spawn((Part::Title, text(&fonts, 16.0, true)));
            // Сделано.
            p.spawn((Node { width: px(GAUGE_W), height: px(6), ..default() }, BackgroundColor(Color::srgba(0.2, 0.17, 0.12, 0.9))))
                .with_children(|b| {
                    b.spawn((Part::Bar, Node { width: percent(0), height: percent(100), ..default() }, BackgroundColor(Color::srgb(0.85, 0.68, 0.32))));
                });
            p.spawn((GaugeBox, Node { width: px(GAUGE_W), height: px(GAUGE_H), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25))))
                .with_children(|g| {
                    let abs = || Node { position_type: PositionType::Absolute, ..default() };
                    g.spawn((Part::Track, abs(), BackgroundColor(Color::srgba(0.32, 0.27, 0.2, 0.9))));
                    g.spawn((Part::Zone, abs(), BackgroundColor(Color::srgba(0.55, 0.8, 0.4, 0.45))));
                    g.spawn((Part::Fill, abs(), BackgroundColor(Color::srgb(0.9, 0.85, 0.7))));
                    g.spawn((Part::Ring, Node { border: UiRect::all(px(2)), border_radius: BorderRadius::MAX, ..abs() }, BorderColor::all(Color::srgba(0.75, 0.65, 0.45, 0.7))));
                    g.spawn((Part::Ghost, Node { border_radius: BorderRadius::MAX, ..abs() }, BackgroundColor(Color::srgba(0.6, 0.85, 0.5, 0.55))));
                    g.spawn((Part::Dot, Node { border_radius: BorderRadius::MAX, ..abs() }, BackgroundColor(Color::srgb(1.0, 0.93, 0.75))));
                    g.spawn((Part::Arrows, text(&fonts, 20.0, true), Node { position_type: PositionType::Absolute, width: percent(100), top: px(50), justify_content: JustifyContent::Center, ..default() }, TextLayout::justify(Justify::Center)));
                });
            p.spawn((Part::Hint, text(&fonts, 13.0, false)));
            p.spawn((Part::Note, text(&fonts, 15.0, true)));
        });
}

/// Где и какого размера часть шкалы (None — спрятать).
struct Placed {
    rect: Option<(f32, f32, f32, f32)>,
}

fn place(node: &mut Node, p: Placed) {
    match p.rect {
        Some((l, t, w, h)) => {
            node.display = Display::Flex;
            node.left = px(l);
            node.top = px(t);
            node.width = px(w);
            node.height = px(h);
        }
        None => node.display = Display::None,
    }
}

fn some(l: f32, t: f32, w: f32, h: f32) -> Placed {
    Placed { rect: Some((l, t, w, h)) }
}

const NONE: Placed = Placed { rect: None };

fn dot_at(cx: f32, cy: f32, r: f32) -> Placed {
    some(cx - r, cy - r, 2.0 * r, 2.0 * r)
}

#[allow(clippy::type_complexity)]
fn update_work_hud(
    work: Res<MyWork>,
    knowledge: Option<Res<crate::lore_client::MyKnowledge>>,
    mut hud: Single<&mut Node, (With<WorkHud>, Without<GaugeBox>)>,
    mut gauge_box: Single<&mut Node, (With<GaugeBox>, Without<WorkHud>)>,
    mut parts: Query<(&Part, &mut Node, Option<&mut Text>, Option<&mut TextColor>, Option<&mut BackgroundColor>), (Without<WorkHud>, Without<GaugeBox>)>,
) {
    let _ = knowledge;
    // Без работы шкала спрятана — и узел не трогаем, иначе интерфейс раскладывается каждый кадр.
    let Some(m) = work.meter.as_ref() else {
        if hud.display != Display::None {
            hud.display = Display::None;
        }
        return;
    };
    if hud.display != Display::Flex {
        hud.display = Display::Flex;
    }
    let shown = if m.drill.tablet() { Display::None } else { Display::Flex };
    if gauge_box.display != shown {
        gauge_box.display = shown;
    }
    let (w, h) = (GAUGE_W, GAUGE_H);
    let (cx, cy) = (w / 2.0, h / 2.0);
    // Кольцо для кругов: радиус 0.9 по шкале.
    let rr = 52.0;
    let ideal = m.drill.period();
    for (part, mut node, txt, color, bg) in &mut parts {
        let set_text = |s: String, c: Option<Color>| {
            if let Some(mut t) = txt
                && t.0 != s
            {
                t.0 = s;
            }
            if let (Some(mut tc), Some(c)) = (color, c) {
                tc.0 = c;
            }
        };
        match part {
            Part::Title => set_text(work.verb.clone(), None),
            Part::Hint => set_text(m.drill.hint().to_string(), None),
            Part::Note => {
                let fresh = m.t - m.last_t < 1.6;
                match m.last.as_ref().filter(|_| fresh) {
                    Some(s) => {
                        let c = if s.q >= 0.7 {
                            Color::srgb(0.6, 0.95, 0.5)
                        } else if s.q >= 0.4 {
                            Color::srgb(0.95, 0.85, 0.45)
                        } else {
                            Color::srgb(1.0, 0.5, 0.38)
                        };
                        set_text(s.note.to_string(), Some(c));
                    }
                    None => set_text(String::new(), None),
                }
            }
            Part::Bar => node.width = percent(work.progress.clamp(0.0, 1.0) * 100.0),
            Part::Arrows => {
                if m.drill != Drill::Mimic {
                    set_text(String::new(), None);
                    continue;
                }
                let line = match m.shown() {
                    None if m.lead_in() => "смотрите на мастера…".to_string(),
                    Some(i) => m
                        .seq
                        .iter()
                        .enumerate()
                        .map(|(k, d)| {
                            if k == i {
                                d.arrow().to_uppercase()
                            } else if k < i {
                                d.arrow().to_string()
                            } else {
                                "·".into()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    None if m.showing => m
                        .seq
                        .iter()
                        .map(|d| d.arrow())
                        .collect::<Vec<_>>()
                        .join(" "),
                    None => m
                        .seq
                        .iter()
                        .enumerate()
                        .map(|(k, _)| match m.seq_ok.get(k) {
                            Some(true) => "+".to_string(),
                            Some(false) => "×".to_string(),
                            None => "?".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                let c = if m.showing {
                    Color::srgb(0.95, 0.85, 0.55)
                } else {
                    Color::srgb(0.8, 0.9, 1.0)
                };
                let c = if m.showing { Color::srgb(0.95, 0.85, 0.55) } else { Color::srgb(0.8, 0.9, 1.0) };
                set_text(line, Some(c));
            }
            _ => {
                let p = gauge(*part, m, w, h, cx, cy, rr, ideal);
                place(&mut node, p);
                if *part == Part::Fill
                    && let Some(mut bg) = bg
                {
                    // Натяжение у предела краснеет.
                    let hot = m.drill == Drill::Tension && m.tension > 0.85;
                    bg.0 = if hot { Color::srgb(1.0, 0.4, 0.3) } else { Color::srgb(0.9, 0.85, 0.7) };
                }
            }
        }
    }
}

/// Геометрия частей шкалы для каждого движения.
#[allow(clippy::too_many_arguments)]
fn gauge(part: Part, m: &Meter, w: f32, h: f32, cx: f32, cy: f32, rr: f32, ideal: f32) -> Placed {
    let bar_y = cy - 9.0;
    match (m.drill, part) {
        (Drill::Grind | Drill::Stir, Part::Ring) => dot_at(cx, cy, rr),
        (Drill::Grind | Drill::Stir, Part::Ghost) => {
            // Призрак показывает нужную скорость круга.
            let a = m.t / ideal * std::f32::consts::TAU;
            dot_at(cx + a.cos() * rr, cy - a.sin() * rr, 6.0)
        }
        (Drill::Grind | Drill::Stir, Part::Dot) => dot_at(cx + m.pos.x / 0.9 * rr, cy - m.pos.y / 0.9 * rr, 8.0),
        (Drill::Pump, Part::Track) => some(cx - 12.0, 8.0, 24.0, h - 16.0),
        (Drill::Pump, Part::Zone) => some(cx - 30.0, 8.0, 60.0, 4.0),
        (Drill::Pump, Part::Fill) => some(cx - 30.0, h - 12.0, 60.0, 4.0),
        (Drill::Pump, Part::Ghost) => {
            let y = (m.t / ideal * std::f32::consts::TAU).cos();
            dot_at(cx + 34.0, cy - y * (h / 2.0 - 12.0), 6.0)
        }
        (Drill::Pump, Part::Dot) => some(cx - 16.0, cy - m.pos.y * (h / 2.0 - 12.0) - 6.0, 32.0, 12.0),
        (Drill::Rhythm, Part::Track) => some(0.0, bar_y, w, 18.0),
        (Drill::Rhythm, Part::Zone) => {
            let z = m.zone() * w / 2.0;
            some(cx - z, bar_y, 2.0 * z, 18.0)
        }
        (Drill::Rhythm, Part::Dot) => some(cx + m.swing() * (w / 2.0 - 3.0) - 3.0, bar_y - 6.0, 6.0, 30.0),
        (Drill::Track, Part::Track) => some(0.0, bar_y, w, 18.0),
        (Drill::Track, Part::Zone) => {
            let z = m.zone() * w;
            some(m.target * w - z, bar_y, 2.0 * z, 18.0)
        }
        (Drill::Track, Part::Dot) => some(m.cursor * w - 2.0, bar_y - 8.0, 4.0, 34.0),
        (Drill::Tension, Part::Track) => some(0.0, bar_y, w, 18.0),
        (Drill::Tension, Part::Zone) => {
            let z = m.zone() * w;
            some(m.tension_center() * w - z, bar_y, 2.0 * z, 18.0)
        }
        (Drill::Tension, Part::Fill) => some(0.0, bar_y + 5.0, m.tension.min(1.0) * w, 8.0),
        _ => NONE,
    }
}

// ---------------------------------------------------------------------------
// Инструмент в руках
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct ToolAssets {
    stone: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    clay: Handle<StandardMaterial>,
    brew: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    linen: Handle<StandardMaterial>,
    parchment: Handle<StandardMaterial>,
    ink: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    leather: Handle<StandardMaterial>,
    wax: Handle<StandardMaterial>,
    pebble: Handle<StandardMaterial>,
    cylinder: Handle<Mesh>,
    cube: Handle<Mesh>,
    capsule: Handle<Mesh>,
    bowl: Handle<Mesh>,
}

fn setup_tool_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut mat = |r: f32, g: f32, b: f32, rough: f32| materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: rough, ..default() });
    commands.insert_resource(ToolAssets {
        stone: mat(0.52, 0.5, 0.46, 0.9),
        wood: mat(0.42, 0.29, 0.17, 0.8),
        clay: mat(0.55, 0.33, 0.22, 0.85),
        brew: mat(0.28, 0.2, 0.08, 0.25),
        iron: mat(0.22, 0.22, 0.24, 0.45),
        linen: mat(0.86, 0.82, 0.72, 0.95),
        parchment: mat(0.85, 0.77, 0.58, 0.9),
        ink: mat(0.12, 0.09, 0.07, 0.9),
        skin: mat(0.72, 0.53, 0.42, 0.7),
        leather: mat(0.35, 0.22, 0.13, 0.8),
        wax: mat(0.2, 0.12, 0.05, 0.55),
        pebble: mat(0.84, 0.82, 0.76, 0.7),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0)),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        capsule: meshes.add(Capsule3d::new(0.5, 1.0)),
        bowl: meshes.add(Torus::new(0.34, 0.5)),
    });
}

/// Корень инструмента (дочерний к камере).
#[derive(Component)]
struct WorkTool(Drill);

/// Подвижная часть инструмента.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum ToolPart {
    /// Пестик, ложка, рука — ходит за мышью по кругу.
    Circler,
    /// Верхняя доска мехов / рукоять — вверх-вниз.
    Lever,
    /// Молот — замах по маятнику.
    Hammer,
    /// Указка над строкой.
    Pointer,
    /// Слово, которое читаете.
    Word,
    /// Натянутая полоса льна.
    Strip,
    /// Рука ученика (повторяет движения).
    Hand,
    /// Стиль: кончик ходит по воску за мышью.
    Stylus,
    /// Камешек на счётной доске: столбец и номер.
    Pebble(u8, u8),
}

fn spawn_tool(
    mut commands: Commands,
    work: Res<MyWork>,
    assets: Option<Res<ToolAssets>>,
    camera: Query<Entity, With<PlayerCamera>>,
    waiting: Query<Entity, (With<crate::client::WaitingCamera>, Without<PlayerCamera>)>,
    tools: Query<(Entity, &WorkTool)>,
) {
    let want = work.meter.as_ref().map(|m| m.drill);
    for (e, t) in &tools {
        if Some(t.0) != want {
            commands.entity(e).despawn();
        }
    }
    let Some(drill) = want else { return };
    if tools.iter().any(|(_, t)| t.0 == drill) {
        return;
    }
    // Снимки с камеры ожидания (`MRPG_CAM`) — инструмент у неё.
    let cam = camera.single().or_else(|_| waiting.single());
    let (Some(a), Ok(cam)) = (assets, cam) else { return };
    let part = |mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform| (Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t);
    let root = commands
        .spawn((WorkTool(drill), Transform::from_xyz(0.0, -0.24, -0.5), Visibility::default()))
        .with_children(|r| match drill {
            Drill::Grind => {
                // Ступка и пестик.
                r.spawn(part(&a.cylinder, &a.stone, Transform::from_xyz(0.0, -0.04, 0.0).with_scale(Vec3::new(0.2, 0.08, 0.2))));
                r.spawn(part(&a.bowl, &a.stone, Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.2, 0.25, 0.2))));
                r.spawn((ToolPart::Circler, part(&a.capsule, &a.wood, Transform::from_scale(Vec3::new(0.035, 0.11, 0.035)))));
            }
            Drill::Stir => {
                // Горшок с варевом и деревянная ложка.
                r.spawn(part(&a.cylinder, &a.clay, Transform::from_xyz(0.0, -0.06, -0.05).with_scale(Vec3::new(0.3, 0.12, 0.3))));
                r.spawn(part(&a.bowl, &a.clay, Transform::from_xyz(0.0, 0.0, -0.05).with_scale(Vec3::new(0.3, 0.3, 0.3))));
                r.spawn(part(&a.cylinder, &a.brew, Transform::from_xyz(0.0, -0.005, -0.05).with_scale(Vec3::new(0.27, 0.004, 0.27))));
                r.spawn((ToolPart::Circler, part(&a.cylinder, &a.wood, Transform::from_scale(Vec3::new(0.018, 0.3, 0.018)))));
            }
            Drill::Pump => {
                // Мехи: нижняя доска, кожа, верхняя доска на шарнире.
                r.spawn(part(&a.cube, &a.wood, Transform::from_xyz(0.0, -0.06, 0.0).with_scale(Vec3::new(0.2, 0.015, 0.34))));
                r.spawn(part(&a.cube, &a.leather, Transform::from_xyz(0.0, -0.03, 0.02).with_scale(Vec3::new(0.18, 0.05, 0.28))));
                r.spawn(part(&a.cylinder, &a.iron, Transform::from_xyz(0.0, -0.05, -0.2).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.03, 0.12, 0.03))));
                r.spawn((ToolPart::Lever, Transform::from_xyz(0.0, -0.045, -0.15), Visibility::default())).with_children(|l| {
                    l.spawn(part(&a.cube, &a.wood, Transform::from_xyz(0.0, 0.0, 0.2).with_scale(Vec3::new(0.2, 0.015, 0.36))));
                    l.spawn(part(&a.cylinder, &a.wood, Transform::from_xyz(0.0, 0.0, 0.42).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.03, 0.12, 0.03))));
                });
            }
            Drill::Rhythm => {
                // Молот (или нож, которым обрывают листья) — замах по маятнику.
                r.spawn((ToolPart::Hammer, Transform::from_xyz(0.12, -0.02, 0.05), Visibility::default())).with_children(|hm| {
                    hm.spawn(part(&a.cylinder, &a.wood, Transform::from_xyz(0.0, 0.0, -0.13).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.025, 0.28, 0.025))));
                    hm.spawn(part(&a.cube, &a.iron, Transform::from_xyz(0.0, 0.0, -0.28).with_scale(Vec3::new(0.05, 0.11, 0.05))));
                });
                r.spawn(part(&a.cube, &a.iron, Transform::from_xyz(-0.02, -0.16, -0.25).with_scale(Vec3::new(0.3, 0.08, 0.12))));
            }
            Drill::Track => {
                // Развёрнутый свиток, строки и указка.
                r.spawn(part(&a.cube, &a.parchment, Transform::from_xyz(0.0, 0.05, -0.05).with_rotation(Quat::from_rotation_x(0.9)).with_scale(Vec3::new(0.5, 0.004, 0.3))));
                for i in 0..5 {
                    let y = 0.11 - i as f32 * 0.03;
                    r.spawn(part(&a.cube, &a.ink, Transform::from_xyz(0.0, y, -0.05 - (0.11 - y) * 0.8 + 0.005).with_rotation(Quat::from_rotation_x(0.9)).with_scale(Vec3::new(0.42, 0.005, 0.006))));
                }
                r.spawn((ToolPart::Word, part(&a.cube, &a.ink, Transform::from_scale(Vec3::new(0.05, 0.006, 0.014)))));
                r.spawn((ToolPart::Pointer, part(&a.cylinder, &a.wood, Transform::from_scale(Vec3::new(0.008, 0.14, 0.008)))));
            }
            Drill::Tension => {
                // Полоса льна в двух руках.
                r.spawn((ToolPart::Strip, part(&a.cube, &a.linen, Transform::from_scale(Vec3::new(0.3, 0.006, 0.06)))));
                r.spawn((ToolPart::Hand, part(&a.cube, &a.skin, Transform::from_xyz(-0.2, 0.0, 0.0).with_scale(Vec3::new(0.07, 0.04, 0.09)))));
                r.spawn((ToolPart::Hand, part(&a.cube, &a.skin, Transform::from_xyz(0.2, 0.0, 0.0).with_scale(Vec3::new(0.07, 0.04, 0.09)))));
            }
            Drill::Mimic => {
                r.spawn((ToolPart::Hand, part(&a.cube, &a.skin, Transform::from_scale(Vec3::new(0.08, 0.05, 0.11)))));
            }
            Drill::Trace | Drill::Scribe => {
                // Восковая табличка в рамке на коленях и стиль (железный, с лопаточкой на другом конце).
                let tilt = Quat::from_rotation_x(0.75);
                r.spawn(part(&a.cube, &a.wood, Transform::from_xyz(0.0, -0.02, -0.06).with_rotation(tilt).with_scale(Vec3::new(0.34, 0.012, 0.2))));
                r.spawn(part(&a.cube, &a.wax, Transform::from_xyz(0.0, -0.013, -0.054).with_rotation(tilt).with_scale(Vec3::new(0.3, 0.006, 0.165))));
                r.spawn((ToolPart::Stylus, Transform::default(), Visibility::default())).with_children(|s| {
                    s.spawn(part(&a.cylinder, &a.iron, Transform::from_xyz(0.0, 0.065, 0.0).with_scale(Vec3::new(0.006, 0.13, 0.006))));
                    s.spawn(part(&a.cube, &a.iron, Transform::from_xyz(0.0, 0.135, 0.0).with_scale(Vec3::new(0.016, 0.012, 0.003))));
                });
            }
            Drill::Count => {
                // Счётная доска и горсть камешков.
                r.spawn(part(&a.cube, &a.wood, Transform::from_xyz(0.0, -0.05, -0.06).with_rotation(Quat::from_rotation_x(0.6)).with_scale(Vec3::new(0.4, 0.015, 0.22))));
                for col in 0..3u8 {
                    for i in 0..9u8 {
                        r.spawn((ToolPart::Pebble(col, i), part(&a.capsule, &a.pebble, Transform::from_scale(Vec3::splat(0.012)))));
                    }
                }
            }
        })
        .id();
    commands.entity(cam).add_child(root);
}

fn animate_tool(work: Res<MyWork>, mut parts: Query<(&ToolPart, &mut Transform)>, mut materials: ResMut<Assets<StandardMaterial>>, assets: Option<Res<ToolAssets>>) {
    let Some(m) = work.meter.as_ref() else { return };
    let mut strip_seen = false;
    let mut hand_i = 0;
    for (part, mut tf) in &mut parts {
        match part {
            ToolPart::Circler => {
                let r = if m.drill == Drill::Grind { 0.05 } else { 0.09 };
                let off = Vec3::new(m.pos.x * r, 0.0, -m.pos.y * r);
                let (tip_y, len) = if m.drill == Drill::Grind { (0.0, 0.11) } else { (-0.02, 0.15) };
                let z0 = if m.drill == Drill::Stir { -0.05 } else { 0.0 };
                // Нижний конец ходит по кругу, верхний — в руке ближе к камере.
                let tip = Vec3::new(off.x, tip_y, z0 + off.z);
                let top = Vec3::new(off.x * 0.4 + 0.03, tip_y + len * 1.6, z0 + 0.07 + off.z * 0.4);
                let dir = (top - tip).normalize_or(Vec3::Y);
                tf.translation = (tip + top) * 0.5;
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
            }
            ToolPart::Lever => {
                // Верх доски: от закрытых мехов до раскрытых.
                tf.rotation = Quat::from_rotation_x(0.05 + 0.3 * (m.pos.y * 0.5 + 0.5));
            }
            ToolPart::Hammer => {
                // Маятник: на краях — замах, в середине — удар по наковальне.
                let lift = 1.0 - m.swing().abs();
                tf.rotation = Quat::from_rotation_x(1.1 - 1.4 * lift);
            }
            ToolPart::Pointer => {
                let x = (m.cursor - 0.5) * 0.42;
                tf.translation = Vec3::new(x, 0.13, 0.02);
                tf.rotation = Quat::from_rotation_x(-0.5);
            }
            ToolPart::Word => {
                let line = (m.line % 5) as f32;
                let y = 0.11 - line * 0.03;
                tf.translation = Vec3::new((m.target - 0.5) * 0.42, y + 0.002, -0.05 - (0.11 - y) * 0.8 + 0.008);
                tf.rotation = Quat::from_rotation_x(0.9);
            }
            ToolPart::Stylus => {
                // Кончик — на воске там, куда ведёт мышь; рука сзади-справа.
                let tilt = Quat::from_rotation_x(0.75);
                let local = Vec3::new((m.nib.x - 0.5) * 0.22, 0.004, -(m.nib.y - 0.5) * 0.14);
                let tip = Vec3::new(0.0, -0.013, -0.054) + tilt * local;
                tf.translation = tip;
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, Vec3::new(0.35, 0.8, 0.5).normalize());
            }
            ToolPart::Pebble(col, i) => {
                let n = m.pebbles.get(*col as usize).copied().unwrap_or(0);
                let shown = *i < n;
                let tilt = Quat::from_rotation_x(0.6);
                let (row, k) = (*i / 5, *i % 5);
                let local = Vec3::new((*col as f32 - 1.0) * 0.12 + (k as f32 - 2.0) * 0.018, 0.012, 0.07 - row as f32 * 0.025);
                tf.translation = Vec3::new(0.0, -0.05, -0.06) + tilt * local;
                tf.scale = if shown { Vec3::new(0.012, 0.006, 0.012) } else { Vec3::ZERO };
            }
            ToolPart::Strip => {
                strip_seen = true;
                tf.scale.x = 0.26 + 0.14 * m.tension.min(1.0);
                tf.scale.y = 0.006 * (1.2 - 0.5 * m.tension.min(1.0));
            }
            ToolPart::Hand => {
                if m.drill == Drill::Mimic {
                    let d = match (m.shown(), m.seq.get(m.seq_i.saturating_sub(1))) {
                        (Some(i), _) => m.seq.get(i).copied(),
                        (None, d) if !m.showing && m.seq_i > 0 => d.copied(),
                        _ => None,
                    };
                    let v = d.map(Dir::vec).unwrap_or(Vec2::ZERO) * 0.08;
                    tf.translation = tf.translation.lerp(Vec3::new(v.x, v.y, 0.0), 0.2);
                } else {
                    let side = if hand_i == 0 { -1.0 } else { 1.0 };
                    hand_i += 1;
                    tf.translation.x = side * (0.14 + 0.07 * m.tension.min(1.0));
                }
            }
        }
    }
    // Перетянутый лён краснеет.
    if strip_seen && let Some(a) = assets && let Some(mut mat) = materials.get_mut(&a.linen) {
        let hot = ((m.tension - 0.75) / 0.25).clamp(0.0, 1.0);
        mat.base_color = Color::srgb(0.86, 0.82 - 0.4 * hot, 0.72 - 0.45 * hot);
    }
}
