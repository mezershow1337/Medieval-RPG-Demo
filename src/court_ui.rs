//! Клиент: разговор с комитом Херсона и его двором (окно в теме пергамента).
//!
//! По E у человека из двора открывается окно разговора: кто это и чем сейчас занят, что он
//! сказал, и вопросы, которые можно задать (цифрами 1–9 или мышью). Ответы меняются, если
//! спросить ещё раз; иные вопросы открываются, только когда узнал что-то от других (прошение
//! вдовы, письмо хартулария…), а у комита — только в час приёма, когда он на троне. «Что
//! скажешь о…» — что этот человек думает о других при дворе. Слишком долгие расспросы
//! утомляют: человек прощается сам.
//!
//! Что было сказано и что узнано — только у этого игрока на клиенте (как подсказки):
//! на сервер не уходит, протокол не меняется.
#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::court::{self, Courtier, Ctx, Flag};
use crate::daily_life::Act;
use crate::loot_client::LootState;
use crate::player::LocalPlayer;
use crate::ui_theme::{AppearChildren, Skin, ink, theme};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use std::collections::{HashMap, HashSet};

pub struct CourtUiPlugin;

impl Plugin for CourtUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CourtUi::default())
            .add_systems(Startup, setup_court.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (shot_court, court_input, court_keys, court_clicks, rebuild_court).chain())
            .add_systems(PostUpdate, keep_in_shots.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate).before(bevy::ui::UiSystems::Prepare));
    }
}

/// Что показано в окне.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Вопросы этому человеку.
    Topics,
    /// «Что скажешь о…» — список людей двора.
    About,
    /// Разговор окончен (простился или устал): только «Уйти».
    Ended,
}

/// Окно разговора с человеком двора и то, что игрок уже знает.
#[derive(Resource)]
pub struct CourtUi {
    who: Option<(Courtier, usize)>,
    /// Где стоял игрок, когда начал разговор (отошёл — окно закрывается).
    at: Option<Vec3>,
    mode: Mode,
    /// Последний вопрос игрока и ответ.
    asked_line: Option<String>,
    said: String,
    /// Вопросов за этот разговор.
    asked: u32,
    /// Разговоров с каждым (с первого — знакомство).
    met: HashMap<Courtier, u32>,
    /// Сколько раз задан вопрос (кому, номер вопроса).
    times: HashMap<(Courtier, usize), u32>,
    flags: HashSet<Flag>,
    /// Кнопки по порядку (для цифр 1–9).
    order: Vec<CourtButton>,
    rev: u64,
    built: u64,
}

impl Default for CourtUi {
    fn default() -> Self {
        CourtUi { who: None, at: None, mode: Mode::Topics, asked_line: None, said: String::new(), asked: 0, met: HashMap::new(), times: HashMap::new(), flags: HashSet::new(), order: Vec::new(), rev: 1, built: 0 }
    }
}

impl CourtUi {
    pub fn open(&self) -> bool {
        self.who.is_some()
    }

    /// Начать разговор по E: `x` без знакомства — его знает только окно.
    pub fn begin(&mut self, c: Courtier, i: usize, mut x: Ctx) {
        x.met = self.met(c);
        self.show(c, i, &x);
    }

    /// Начать разговор с человеком двора (`i` — номер жителя).
    fn show(&mut self, c: Courtier, i: usize, x: &Ctx) {
        self.who = Some((c, i));
        self.at = None;
        self.mode = Mode::Topics;
        self.asked_line = None;
        self.asked = 0;
        self.said = court::greeting(c, x).to_string();
        self.rev += 1;
    }

    fn close(&mut self) {
        if let Some((c, _)) = self.who.take() {
            *self.met.entry(c).or_default() += 1;
        }
        self.at = None;
        self.rev += 1;
    }

    fn met(&self, c: Courtier) -> u32 {
        self.met.get(&c).copied().unwrap_or(0)
    }

    /// Обстоятельства разговора сейчас.
    fn ctx(&self, c: Courtier, act: Act, clock: Option<&crate::world_clock::WorldClock>) -> Ctx {
        Ctx { hour: clock.map_or(10.0, |k| k.hour()), day: clock.map_or(0, |k| k.day()), met: self.met(c), act, rain: clock.is_some_and(|k| k.rain() > 0.2) }
    }

    /// Задать вопрос `k` из списка этого человека.
    fn ask(&mut self, c: Courtier, k: usize, x: &Ctx) {
        let Some(topic) = court::topics(c).get(k) else { return };
        let n = self.times.entry((c, k)).or_default();
        let say = if topic.say.is_empty() { court::news(c, x.day + *n) } else { court::answer(topic, *n) };
        *n += 1;
        if let Some(f) = topic.learn {
            self.flags.insert(f);
        }
        self.asked_line = Some(topic.ask.to_string());
        self.said = say.to_string();
        self.after_question(c);
    }

    /// Спросить, что он думает о `about`.
    fn ask_about(&mut self, c: Courtier, about: Courtier) {
        self.asked_line = Some(format!("Что скажешь о {}?", about_name(about)));
        self.said = court::opinion(c, about).unwrap_or("Мне нечего о нём сказать.").to_string();
        self.mode = Mode::Topics;
        self.after_question(c);
    }

    fn after_question(&mut self, c: Courtier) {
        self.asked += 1;
        if self.asked >= court::patience(c) {
            self.said = format!("{}\n\n{}", self.said, court::tired(c));
            self.mode = Mode::Ended;
        }
        self.rev += 1;
    }

    fn farewell(&mut self, c: Courtier, x: &Ctx) {
        self.asked_line = Some("Прощай.".to_string());
        self.said = court::farewell(c, x).to_string();
        self.mode = Mode::Ended;
        self.rev += 1;
    }
}

/// «О ком» — в предложном падеже.
fn about_name(c: Courtier) -> &'static str {
    match c {
        Courtier::Comes => "комите",
        Courtier::Wife => "госпоже Евфимии",
        Courtier::Son => "Феодоре, сыне комита",
        Courtier::Chartularius => "хартуларии Стефане",
        Courtier::Sacellarius => "сакелларии Никифоре",
        Courtier::Tribune => "трибуне Фравитте",
        Courtier::Presbyter => "отце Евсевии",
        Courtier::Praepositus => "препозите Мине",
        Courtier::Cook => "кухарке Агафье",
        Courtier::Guard(_) => "страже у ворот",
        Courtier::Protevon => "протевоне Аристоне",
        Courtier::Petitioner(_) => "просителях",
    }
}

#[derive(Component)]
struct CourtRoot;

#[derive(Component)]
struct CourtBody;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum CourtButton {
    Topic(usize),
    AboutMenu,
    About(Courtier),
    Back,
    Farewell,
    Leave,
}

fn setup_court(mut commands: Commands) {
    let theme = theme();
    commands
        .spawn((
            CourtRoot,
            crate::civil_client::ShotKeep,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                // Справа внизу: собеседник остаётся виден слева.
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::FlexEnd,
                padding: UiRect { right: px(28), bottom: px(28), ..default() },
                ..default()
            },
            FocusPolicy::Pass,
            GlobalZIndex(23),
        ))
        .with_children(|root| {
            root.spawn((
                CourtBody,
                Node {
                    width: px(640),
                    max_width: percent(94),
                    max_height: percent(78),
                    flex_direction: FlexDirection::Column,
                    padding: Skin::Parchment.padding(),
                    row_gap: px(5),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
                crate::ui_theme::ScrollWheel,
                theme.skin(Skin::Parchment),
                Interaction::default(),
                FocusPolicy::Block,
            ));
        });
}

#[allow(clippy::too_many_arguments)]
fn court_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<CourtUi>,
    mut loot: ResMut<LootState>,
    acts: Res<crate::life_client::LiveActs>,
    me: Query<&Transform, With<LocalPlayer>>,
    mut root: Single<(Entity, &mut Node), With<CourtRoot>>,
    mut commands: Commands,
) {
    let pos = me.iter().next().map(|t| t.translation);
    if let Some((_, i)) = ui.who {
        let walked = match (ui.at, pos) {
            (Some(a), Some(p)) => a.distance(p) > 3.0,
            _ => false,
        };
        // Ушёл спать или испугался драки — разговор окончен.
        let gone = matches!(acts.0.get(i), Some(Act::Sleep | Act::Away | Act::Cower));
        if walked || gone || keys.just_pressed(KeyCode::Escape) {
            ui.close();
        } else if ui.at.is_none() {
            ui.at = pos;
        }
    }
    let open = ui.open();
    if open && loot.ui_open {
        loot.ui_open = false;
        loot.rev += 1;
    }
    let (entity, node) = &mut *root;
    let want = if open { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
        if open {
            commands.entity(*entity).insert(AppearChildren);
        }
    }
}

/// Цифры 1–9 — кнопки по порядку.
fn court_keys(keys: Res<ButtonInput<KeyCode>>, mut ui: ResMut<CourtUi>, acts: Res<crate::life_client::LiveActs>, clock: Option<Res<crate::world_clock::WorldClock>>) {
    if !ui.open() {
        return;
    }
    const DIGITS: [KeyCode; 9] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];
    let Some(k) = DIGITS.iter().position(|d| keys.just_pressed(*d)) else { return };
    let Some(b) = ui.order.get(k).copied() else { return };
    press(&mut ui, b, &acts, clock.as_deref());
}

fn court_clicks(
    buttons: Query<(&Interaction, &CourtButton, &crate::ui_theme::ThemeButton), Changed<Interaction>>,
    mut ui: ResMut<CourtUi>,
    acts: Res<crate::life_client::LiveActs>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
) {
    for (i, b, t) in &buttons {
        if *i == Interaction::Pressed && t.enabled {
            press(&mut ui, *b, &acts, clock.as_deref());
            return;
        }
    }
}

fn press(ui: &mut CourtUi, b: CourtButton, acts: &crate::life_client::LiveActs, clock: Option<&crate::world_clock::WorldClock>) {
    let Some((c, i)) = ui.who else { return };
    let act = acts.0.get(i).copied().unwrap_or(Act::Idle);
    let x = ui.ctx(c, act, clock);
    match b {
        CourtButton::Topic(k) => ui.ask(c, k, &x),
        CourtButton::AboutMenu => {
            ui.mode = Mode::About;
            ui.rev += 1;
        }
        CourtButton::About(a) => ui.ask_about(c, a),
        CourtButton::Back => {
            ui.mode = Mode::Topics;
            ui.rev += 1;
        }
        CourtButton::Farewell => ui.farewell(c, &x),
        CourtButton::Leave => ui.close(),
    }
}

/// Что сейчас делает (для шапки окна).
fn doing(c: Courtier, act: Act, audience: bool) -> Option<String> {
    Some(match (c, act) {
        (Courtier::Comes, Act::Sit) if audience => "принимает просителей на троне".to_string(),
        (_, Act::Dine) => "за трапезой в триклинии".to_string(),
        (_, Act::Idle) => return None,
        (_, a) => a.name().to_string(),
    })
}

fn rebuild_court(
    mut commands: Commands,
    mut ui: ResMut<CourtUi>,
    acts: Res<crate::life_client::LiveActs>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
    body: Single<Entity, With<CourtBody>>,
) {
    let Some((c, i)) = ui.who else { return };
    let act = acts.0.get(i).copied().unwrap_or(Act::Idle);
    let x = ui.ctx(c, act, clock.as_deref());
    // Перестраиваем при каждом вопросе и когда меняется занятие (комит сел на трон).
    let key = ui.rev ^ (act as u64) << 48 ^ (x.audience() as u64) << 56;
    if ui.built == key {
        return;
    }
    ui.built = key;
    let theme = theme();
    let mut order = Vec::new();
    commands.entity(*body).despawn_children();
    let ui_ref = &*ui;
    commands.entity(*body).with_children(|p| {
        // Шапка: имя, должность, чем занят.
        p.spawn(Node { width: percent(100), align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|head| {
            head.spawn(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                t.spawn(theme.title_colored(c.name(), 28.0, ink::RUBRIC));
                let now = doing(c, act, x.audience()).map(|d| format!(" · сейчас {d}")).unwrap_or_default();
                t.spawn(theme.text(format!("{}{now}", c.office()), 13.0, ink::INK_DIM));
            });
            theme.small_button(head, "Уйти", CourtButton::Leave, true);
        });
        theme.divider(p);
        if let Some(q) = &ui_ref.asked_line {
            p.spawn(theme.text(format!("— {q}"), 14.0, ink::INK_DIM));
        }
        for (k, para) in ui_ref.said.split("\n\n").enumerate() {
            let color = if k == 0 { ink::INK } else { ink::RUBRIC };
            p.spawn((theme.text(format!("«{para}»"), 16.0, color), Node { margin: UiRect::vertical(px(3)), ..default() }));
        }
        theme.divider(p);
        let mut row = |p: &mut ChildSpawnerCommands, label: &str, b: CourtButton| {
            order.push(b);
            let n = order.len();
            let label = if n <= 9 { format!("{n}. {label}") } else { label.to_string() };
            p.spawn(Node { width: percent(100), ..default() }).with_children(|r| {
                theme.small_button(r, &label, b, true).insert(Node {
                    padding: UiRect::axes(px(12), px(4)),
                    min_height: px(26),
                    justify_content: JustifyContent::FlexStart,
                    align_items: AlignItems::Center,
                    ..default()
                });
            });
        };
        match ui_ref.mode {
            Mode::Topics => {
                let known = |f: Flag| ui_ref.flags.contains(&f);
                for (k, topic) in court::topics(c).iter().enumerate() {
                    if court::open(topic, &x, &known) {
                        let again = ui_ref.times.get(&(c, k)).copied().unwrap_or(0) > 0 && !topic.say.is_empty() && topic.say.len() > 1;
                        let label = if again { format!("{} (ещё раз)", topic.ask) } else { topic.ask.to_string() };
                        row(p, &label, CourtButton::Topic(k));
                    }
                }
                if !court::gossip_about(c).is_empty() {
                    row(p, "Что скажешь о других при дворе?", CourtButton::AboutMenu);
                }
                row(p, "Прощай.", CourtButton::Farewell);
            }
            Mode::About => {
                for a in court::gossip_about(c) {
                    row(p, &format!("О {}", about_name(a)), CourtButton::About(a));
                }
                row(p, "Спросить о другом.", CourtButton::Back);
            }
            Mode::Ended => row(p, "Уйти.", CourtButton::Leave),
        }
        p.spawn(theme.text("1–9 — ответить · Esc — уйти. Отойдёте — разговор кончится.", 12.0, ink::INK_DIM));
    });
    ui.order = order;
}

/// Снимок уровня (`MRPG_CAM`) без интерфейса: HUD (мини-карту, полоски здоровья) меню
/// показывает заново каждый кадр, поэтому прячем его здесь, после всех систем `Update`.
/// Окно разговора на снимке `MRPG_SHOT_UI=court…` оставляем.
#[allow(clippy::type_complexity)]
fn keep_in_shots(ui: Res<CourtUi>, mut roots: Query<(&mut Visibility, Has<CourtRoot>, Has<crate::civil_client::ShotKeep>), (With<Node>, Without<ChildOf>)>, mut on: Local<Option<(bool, bool)>>) {
    let (cam, court) = *on.get_or_insert_with(|| (std::env::var_os("MRPG_CAM").is_some(), std::env::var("MRPG_SHOT_UI").is_ok_and(|s| s.starts_with("court"))));
    if !cam {
        return;
    }
    let other_ui = std::env::var_os("MRPG_SHOT_UI").is_some() || std::env::var_os("MRPG_LAW_DEMO").is_some() || std::env::var_os("MRPG_WORK_DEMO").is_some();
    for (mut v, is_court, keep) in &mut roots {
        let want = if is_court && court && ui.open() {
            Visibility::Inherited
        } else if keep && other_ui && !court {
            continue;
        } else {
            Visibility::Hidden
        };
        v.set_if_neq(want);
    }
}

/// Снимок окна: `MRPG_SHOT_UI=court:comes` (или wife, son, chartularius, sacellarius, tribune,
/// presbyter, praepositus, cook, guard, protevon, widow, vintner), `court:comes:2` — сразу задать
/// третий вопрос. Без сервера, по раскладке уровня.
fn shot_court(mut ui: ResMut<CourtUi>, acts: Res<crate::life_client::LiveActs>, clock: Option<Res<crate::world_clock::WorldClock>>, mut done: Local<bool>, mut frames: Local<u32>) {
    if *done {
        return;
    }
    let Some(rest) = std::env::var("MRPG_SHOT_UI").ok().and_then(|s| s.strip_prefix("court").map(|r| r.trim_start_matches(':').to_string())) else {
        *done = true;
        return;
    };
    *frames += 1;
    if *frames < 20 {
        return;
    }
    let mut parts = rest.split(':');
    let c = match parts.next().unwrap_or("") {
        "wife" => Courtier::Wife,
        "son" => Courtier::Son,
        "chartularius" => Courtier::Chartularius,
        "sacellarius" => Courtier::Sacellarius,
        "tribune" => Courtier::Tribune,
        "presbyter" => Courtier::Presbyter,
        "praepositus" => Courtier::Praepositus,
        "cook" => Courtier::Cook,
        "guard" => Courtier::Guard(0),
        "protevon" => Courtier::Protevon,
        "widow" => Courtier::Petitioner(0),
        "vintner" => Courtier::Petitioner(1),
        _ => Courtier::Comes,
    };
    let Some(i) = court::resident_of(c) else {
        *done = true;
        return;
    };
    let act = acts.0.get(i).copied().unwrap_or(Act::Idle);
    let x = ui.ctx(c, act, clock.as_deref());
    ui.show(c, i, &x);
    if let Some(k) = parts.next().and_then(|k| k.parse::<usize>().ok()) {
        ui.met.insert(c, 1);
        ui.ask(c, k, &x);
    }
    *done = true;
}
