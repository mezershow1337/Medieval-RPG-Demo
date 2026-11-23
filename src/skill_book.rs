//! Книга навыков (K) и журнал (F1): одно окно на пергаменте с тремя вкладками.
//!
//! * «Навыки» — древо: десять ветвей, в каждой ступени умений. Наведите на умение — видно,
//!   чего не хватает и с какими ветвями оно связано (связанные столбцы и умения подсвечиваются);
//!   щелчок закрепляет описание.
//! * «Знания» — что персонаж знает: грамота, травы, приёмы, книги.
//! * «Журнал» — с чего начать и как развивать каждый навык (без готовых ответов).

#![allow(deprecated)] // Interaction в Bevy 0.20 помечен устаревшим, но работает

use crate::guide::Chapter;
use crate::hints::Hints;
use crate::loot_client::LootState;
use crate::lore::{Knowledge, Skill};
use crate::lore_client::MyKnowledge;
use crate::skill_tree::{self, Need, Perk};
use crate::ui_theme::{AppearChildren, ScrollWheel, Skin, UiTheme, ink};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

pub struct SkillBookPlugin;

impl Plugin for SkillBookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BookUi>()
            .add_systems(Startup, setup_book.after(crate::inventory_ui::setup_fonts))
            .add_systems(
                Update,
                (
                    book_input,
                    book_clicks,
                    book_hover,
                    rebuild_book,
                    rebuild_detail,
                    highlight_links,
                )
                    .chain(),
            );
        if std::env::var("MRPG_SHOT").is_ok() {
            app.add_systems(Update, dev_open_book);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    Skills,
    Knowledge,
    Journal,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Skills => "Навыки",
            Tab::Knowledge => "Знания",
            Tab::Journal => "Журнал",
        }
    }
}

/// На что смотрит игрок в древе.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Perk(Perk),
    Skill(Skill),
}

#[derive(Resource)]
pub struct BookUi {
    pub open: bool,
    pub tab: Tab,
    chapter: Chapter,
    hovered: Option<Focus>,
    pinned: Option<Focus>,
    rev: u64,
    built: u64,
    detail_built: Option<(Option<Focus>, u64)>,
}

impl Default for BookUi {
    fn default() -> Self {
        Self {
            open: false,
            tab: Tab::Skills,
            chapter: Chapter::Start,
            hovered: None,
            pinned: None,
            rev: 1,
            built: 0,
            detail_built: None,
        }
    }
}

impl BookUi {
    fn focus(&self) -> Option<Focus> {
        self.hovered.or(self.pinned)
    }

    pub fn show(&mut self, tab: Tab) {
        self.open = true;
        self.tab = tab;
        self.rev += 1;
    }

    pub fn show_chapter(&mut self, chapter: Chapter) {
        self.chapter = chapter;
        self.show(Tab::Journal);
    }
}

#[derive(Component)]
struct BookRoot;
#[derive(Component)]
struct BookBody;
#[derive(Component)]
struct BookTitle;
#[derive(Component)]
struct TabButton(Tab);
#[derive(Component)]
struct ChapterButton(Chapter);
#[derive(Component)]
struct HintsToggle;
#[derive(Component)]
struct CloseButton;
#[derive(Component)]
struct PerkNode(Perk);
#[derive(Component)]
struct SkillHeader(Skill);
#[derive(Component)]
struct DetailBox;
/// Обычный цвет рамки (подсветка связей его временно меняет).
#[derive(Component, Clone, Copy)]
struct BaseBorder(Color);

const RUBRIC_FILL: Color = Color::srgba(0.62, 0.15, 0.09, 0.14);
const LINK: Color = Color::srgb(0.78, 0.52, 0.12);
const OK_INK: Color = Color::srgb(0.16, 0.38, 0.14);

fn short(s: Skill) -> &'static str {
    match s {
        Skill::Strength => "Сила",
        Skill::Endurance => "Вынос.",
        Skill::Arms => "Ратное",
        Skill::Archery => "Лук",
        Skill::Smithing => "Кузня",
        Skill::Alchemy => "Алхимия",
        Skill::Herbalism => "Травы",
        Skill::Medicine => "Врач.",
        Skill::Letters => "Книжн.",
        Skill::Trade => "Торг",
    }
}

fn setup_book(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            BookRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            theme.backdrop(),
            GlobalZIndex(22),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: percent(96),
                    height: percent(94),
                    flex_direction: FlexDirection::Column,
                    padding: Skin::Parchment.padding(),
                    row_gap: px(4),
                    ..default()
                },
                theme.skin(Skin::Parchment),
            ))
            .with_children(|panel| {
                // Шапка: вкладки слева, заголовок посередине, «закрыть» справа.
                panel
                    .spawn(Node {
                        width: percent(100),
                        align_items: AlignItems::Center,
                        column_gap: px(8),
                        ..default()
                    })
                    .with_children(|head| {
                        for tab in [Tab::Skills, Tab::Knowledge, Tab::Journal] {
                            theme.toggle_button(
                                head,
                                tab.label(),
                                TabButton(tab),
                                tab == Tab::Skills,
                            );
                        }
                        head.spawn(Node {
                            flex_grow: 1.0,
                            justify_content: JustifyContent::Center,
                            ..default()
                        })
                        .with_children(|t| {
                            t.spawn((
                                BookTitle,
                                theme.title_colored("Книга навыков", 30.0, ink::RUBRIC),
                            ));
                        });
                        head.spawn(theme.text("K — книга · F1 — журнал · Esc", 13.0, ink::INK_DIM));
                        theme.small_button(head, "Закрыть", CloseButton, true);
                    });
                theme.divider(panel);
                panel.spawn((
                    BookBody,
                    Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        min_height: px(0),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                ));
            });
        });
}

fn book_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<BookUi>,
    mut loot: ResMut<LootState>,
    trade: Option<Res<crate::trade_client::TradeState>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut root: Single<(Entity, &mut Node), With<BookRoot>>,
    mut commands: Commands,
) {
    let busy = trade.as_ref().is_some_and(|t| t.window_open());
    let was = ui.open;
    if keys.just_pressed(KeyCode::KeyK) && !busy {
        if ui.open && ui.tab != Tab::Journal {
            ui.open = false;
        } else {
            let tab = if ui.tab == Tab::Journal {
                Tab::Skills
            } else {
                ui.tab
            };
            ui.show(tab);
        }
    }
    if keys.just_pressed(KeyCode::F1) && !busy {
        if ui.open && ui.tab == Tab::Journal {
            ui.open = false;
        } else {
            ui.show(Tab::Journal);
        }
    }
    if ui.open && keys.just_pressed(KeyCode::Escape) {
        ui.open = false;
    }
    if ui.open && !was && loot.ui_open {
        // Книга поверх сумки: сумку закрываем, чтобы клики не путались.
        loot.ui_open = false;
        loot.rev += 1;
    }
    let (entity, node) = &mut *root;
    let want = if ui.open {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != want {
        node.display = want;
        if ui.open {
            commands.entity(*entity).insert(AppearChildren);
        }
    }
    if ui.open && (cursor.grab_mode != CursorGrabMode::None || !cursor.visible) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    if was && !ui.open && !loot.ui_open && !busy && !keys.just_pressed(KeyCode::Escape) {
        // Обратно в игру.
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

#[allow(clippy::type_complexity)]
fn book_clicks(
    mut ui: ResMut<BookUi>,
    mut hints: ResMut<Hints>,
    tabs: Query<(&Interaction, &TabButton), Changed<Interaction>>,
    chapters: Query<(&Interaction, &ChapterButton), Changed<Interaction>>,
    toggles: Query<&Interaction, (Changed<Interaction>, With<HintsToggle>)>,
    close: Query<&Interaction, (Changed<Interaction>, With<CloseButton>)>,
    perks: Query<(&Interaction, &PerkNode), Changed<Interaction>>,
    headers: Query<(&Interaction, &SkillHeader), Changed<Interaction>>,
    mut tab_buttons: Query<(&TabButton, &mut crate::ui_theme::ThemeButton)>,
) {
    if !ui.open {
        return;
    }
    for (i, t) in &tabs {
        if *i == Interaction::Pressed && ui.tab != t.0 {
            ui.tab = t.0;
            ui.rev += 1;
        }
    }
    for (b, mut tb) in &mut tab_buttons {
        let on = b.0 == ui.tab;
        if tb.on != on {
            tb.on = on;
        }
    }
    for (i, c) in &chapters {
        if *i == Interaction::Pressed && ui.chapter != c.0 {
            ui.chapter = c.0;
            ui.rev += 1;
        }
    }
    for i in &toggles {
        if *i == Interaction::Pressed {
            let on = !hints.enabled();
            hints.set_enabled(on);
            ui.rev += 1;
        }
    }
    for i in &close {
        if *i == Interaction::Pressed {
            ui.open = false;
        }
    }
    for (i, p) in &perks {
        if *i == Interaction::Pressed {
            let f = Some(Focus::Perk(p.0));
            ui.pinned = if ui.pinned == f { None } else { f };
        }
    }
    for (i, h) in &headers {
        if *i == Interaction::Pressed {
            let f = Some(Focus::Skill(h.0));
            ui.pinned = if ui.pinned == f { None } else { f };
        }
    }
}

fn book_hover(
    mut ui: ResMut<BookUi>,
    perks: Query<(&Interaction, &PerkNode)>,
    headers: Query<(&Interaction, &SkillHeader)>,
) {
    if !ui.open || ui.tab != Tab::Skills {
        return;
    }
    let hovered = perks
        .iter()
        .find(|(i, _)| **i != Interaction::None)
        .map(|(_, p)| Focus::Perk(p.0))
        .or_else(|| {
            headers
                .iter()
                .find(|(i, _)| **i != Interaction::None)
                .map(|(_, h)| Focus::Skill(h.0))
        });
    if ui.hovered != hovered {
        ui.hovered = hovered;
    }
}

// ---------------------------------------------------------------------------
// Построение вкладок
// ---------------------------------------------------------------------------

fn rebuild_book(
    mut commands: Commands,
    mut ui: ResMut<BookUi>,
    knowledge: Res<MyKnowledge>,
    hints: Res<Hints>,
    theme: Res<UiTheme>,
    body: Single<Entity, With<BookBody>>,
    mut title: Single<&mut Text, With<BookTitle>>,
) {
    if !ui.open {
        return;
    }
    if knowledge.is_changed() && ui.tab != Tab::Journal {
        ui.rev += 1;
    }
    if ui.built == ui.rev {
        return;
    }
    ui.built = ui.rev;
    ui.detail_built = None;
    title.0 = match ui.tab {
        Tab::Skills => "Книга навыков",
        Tab::Knowledge => "Что вы знаете",
        Tab::Journal => "Журнал странника",
    }
    .into();
    let k = &knowledge.0;
    let tab = ui.tab;
    let chapter = ui.chapter;
    commands
        .entity(*body)
        .despawn_children()
        .with_children(|p| match tab {
            Tab::Skills => build_tree(p, &theme, k),
            Tab::Knowledge => build_knowledge(p, &theme, k),
            Tab::Journal => build_journal(p, &theme, chapter, hints.enabled()),
        });
}

/// Состояние умения для рамки и цвета.
#[derive(Clone, Copy, PartialEq)]
enum State {
    Earned,
    /// Знаний и умений хватает, осталось набить навык.
    Near,
    Far,
}

fn perk_state(p: Perk, k: &Knowledge, earned: &[Perk]) -> State {
    if earned.contains(&p) {
        return State::Earned;
    }
    if p.missing(k, earned)
        .iter()
        .all(|n| matches!(n, Need::Skill(..)))
    {
        State::Near
    } else {
        State::Far
    }
}

/// Коротко об условиях: «от 35 · + Сила 20 · знание».
fn short_needs(p: Perk) -> String {
    let own = p.info().skill;
    let mut parts = Vec::new();
    for n in p.info().needs {
        match *n {
            Need::Skill(s, lvl) if s == own => parts.insert(0, format!("от {lvl:.0}")),
            Need::Skill(s, lvl) => parts.push(format!("{} {lvl:.0}", short(s))),
            Need::Lore(_) | Need::AnyLore(_) => {
                if !parts.iter().any(|s: &String| s == "знание") {
                    parts.push("знание".into());
                }
            }
            Need::Perk(q) if q.info().skill != own => {
                // Ветвь уже названа навыком — не повторяем.
                let tag = short(q.info().skill);
                if !p
                    .info()
                    .needs
                    .iter()
                    .any(|m| matches!(m, Need::Skill(s, _) if *s == q.info().skill))
                {
                    parts.push(format!("+{tag}"));
                }
            }
            Need::Perk(_) => {}
            Need::HerbsKnown(n) => parts.push(format!("{n} трав")),
        }
    }
    parts.join(" · ")
}

const NODE_H: f32 = 78.0;
const LINK_H: f32 = 12.0;

fn build_tree(p: &mut ChildSpawnerCommands, theme: &UiTheme, k: &Knowledge) {
    let earned = skill_tree::earned(k);
    p.spawn(Node {
        width: percent(100),
        flex_grow: 1.0,
        min_height: px(0),
        column_gap: px(6),
        ..default()
    })
    .with_children(|row| {
        for skill in Skill::ALL {
            row.spawn(Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|col| {
                // Шапка ветви: имя, уровень, полоса.
                let base = k.base_skill(skill);
                let bonus = k.skill(skill) - base;
                let level = if bonus > 0.05 {
                    format!("{base:.0} (+{bonus:.0})")
                } else {
                    format!("{base:.0}")
                };
                col.spawn((
                    SkillHeader(skill),
                    Interaction::default(),
                    BaseBorder(Color::NONE),
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(px(4), px(4)),
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(5)),
                        row_gap: px(2),
                        ..default()
                    },
                    BorderColor::all(Color::NONE),
                ))
                .with_children(|h| {
                    h.spawn(Node {
                        height: px(22),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|t| {
                        t.spawn((
                            theme.bold(skill.name(), 14.0, ink::INK),
                            TextLayout::justify(Justify::Center),
                        ));
                    });
                    h.spawn(theme.bold(level, 17.0, ink::RUBRIC));
                    theme.bar(
                        h,
                        Node {
                            width: percent(92),
                            height: px(13),
                            ..default()
                        },
                        base / 100.0,
                        Color::srgb(0.58, 0.16, 0.08),
                        (),
                    );
                });
                // Ступени.
                let perks: Vec<Perk> = Perk::ALL
                    .into_iter()
                    .filter(|q| q.info().skill == skill)
                    .collect();
                let mut prev: Option<State> = None;
                // Ступени делят высоту поровну: древо влезает и в 720 строк, и в 1440.
                col.spawn(Node {
                    width: percent(100),
                    flex_grow: 1.0,
                    min_height: px(0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|col| {
                    for tier in 0..4u8 {
                        let Some(perk) = perks.iter().copied().find(|q| q.info().tier == tier)
                        else {
                            col.spawn(Node {
                                flex_grow: 1.0,
                                flex_basis: px(0),
                                ..default()
                            });
                            prev = None;
                            continue;
                        };
                        let state = perk_state(perk, k, &earned);
                        // Соединитель от предыдущей ступени.
                        let line = match prev {
                            Some(State::Earned) if state == State::Earned => ink::RUBRIC,
                            Some(_) => ink::INK_DIM,
                            None => Color::NONE,
                        };
                        col.spawn(Node {
                            flex_grow: 1.0,
                            flex_basis: px(0),
                            min_height: px(0),
                            max_height: px(NODE_H + LINK_H),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|col| {
                            col.spawn((
                                Node {
                                    width: px(3),
                                    height: px(LINK_H),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                BackgroundColor(line),
                            ));
                            let (border, fill, name_color) = match state {
                                State::Earned => (ink::RUBRIC, RUBRIC_FILL, ink::RUBRIC),
                                State::Near => (ink::INK, Color::NONE, ink::INK),
                                State::Far => {
                                    (ink::INK_DIM.with_alpha(0.45), Color::NONE, ink::INK_DIM)
                                }
                            };
                            col.spawn((
                                PerkNode(perk),
                                Interaction::default(),
                                BaseBorder(border),
                                Node {
                                    width: percent(96),
                                    flex_grow: 1.0,
                                    min_height: px(0),
                                    overflow: Overflow::clip(),
                                    flex_direction: FlexDirection::Column,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::axes(px(4), px(3)),
                                    border: UiRect::all(px(if state == State::Earned {
                                        2
                                    } else {
                                        1
                                    })),
                                    border_radius: BorderRadius::all(px(6)),
                                    row_gap: px(3),
                                    ..default()
                                },
                                BorderColor::all(border),
                                BackgroundColor(fill),
                            ))
                            .with_children(|n| {
                                n.spawn((
                                    theme.bold(perk.info().name, 13.0, name_color),
                                    TextLayout::justify(Justify::Center),
                                ));
                                n.spawn((
                                    theme.text(short_needs(perk), 11.0, ink::INK_DIM),
                                    TextLayout::justify(Justify::Center),
                                ));
                            });
                        });
                        prev = Some(state);
                    }
                });
            });
        }
    });
    theme.divider(p);
    p.spawn((
        DetailBox,
        Node {
            width: percent(100),
            height: px(128),
            flex_shrink: 0.0,
            column_gap: px(28),
            ..default()
        },
    ));
}

fn rebuild_detail(
    mut commands: Commands,
    mut ui: ResMut<BookUi>,
    knowledge: Res<MyKnowledge>,
    theme: Res<UiTheme>,
    boxes: Query<Entity, With<DetailBox>>,
) {
    if !ui.open || ui.tab != Tab::Skills {
        return;
    }
    let Ok(entity) = boxes.single() else { return };
    let want = (ui.focus(), ui.rev);
    if ui.detail_built == Some(want) {
        return;
    }
    ui.detail_built = Some(want);
    let k = &knowledge.0;
    commands.entity(entity).despawn_children().with_children(|p| match want.0 {
        None => {
            p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), flex_grow: 1.0, ..default() }).with_children(|c| {
                c.spawn(theme.title_colored("Как растут навыки", 22.0, ink::RUBRIC));
                c.spawn(theme.text(
                    "Навык растёт от дела: кто куёт — становится кузнецом, кто носит тяжести — крепнет. У мастера за плату учишься быстрее всего, подмастерьем — медленнее, но рука набивается. Книги дают знание, а не руку.",
                    15.0,
                    ink::INK,
                ));
                c.spawn(theme.text(
                    "Умения не покупают: они приходят сами, когда набран навык и выучено нужное. Лучшие требуют сразу нескольких ветвей — лекарю нужны травы, кузнецу сила, купцу грамота.",
                    15.0,
                    ink::INK,
                ));
                c.spawn(theme.text(
                    "Киноварью — освоено, чернилами — осталось набить навык, бледно — нужны ещё знания или другие умения. Наведите на умение или навык — подсветятся связи; щелчок закрепляет описание.",
                    13.0,
                    ink::INK_DIM,
                ));
            });
        }
        Some(Focus::Perk(perk)) => perk_detail(p, &theme, k, perk),
        Some(Focus::Skill(skill)) => skill_detail(p, &theme, k, skill),
    });
}

fn perk_detail(p: &mut ChildSpawnerCommands, theme: &UiTheme, k: &Knowledge, perk: Perk) {
    let info = perk.info();
    let earned = skill_tree::earned(k);
    let state = perk_state(perk, k, &earned);
    p.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(5),
        flex_basis: percent(55),
        flex_grow: 1.0,
        ..default()
    })
    .with_children(|c| {
        c.spawn(theme.title_colored(info.name, 24.0, ink::RUBRIC));
        c.spawn(theme.text(
            format!(
                "Ветвь: {} · ступень {}",
                info.skill.name().to_lowercase(),
                info.tier + 1
            ),
            13.0,
            ink::INK_DIM,
        ));
        c.spawn(theme.text(info.about, 15.0, ink::INK));
        c.spawn(theme.bold(format!("Даёт: {}", info.effect), 15.0, ink::INK));
        let status = match state {
            State::Earned => "Освоено.".to_string(),
            State::Near => "Знаний хватает — осталось набить руку.".to_string(),
            State::Far => "Пока далеко.".to_string(),
        };
        c.spawn(theme.bold(
            status,
            14.0,
            if state == State::Earned {
                ink::RUBRIC
            } else {
                ink::INK_DIM
            },
        ));
    });
    p.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(4),
        flex_basis: percent(45),
        flex_grow: 1.0,
        ..default()
    })
    .with_children(|c| {
        c.spawn(theme.bold("Что нужно", 16.0, ink::RUBRIC));
        for n in info.needs {
            let met = n.met(k, &earned);
            let have = match *n {
                Need::Skill(s, _) => format!(" (у вас {:.0})", k.base_skill(s)),
                Need::HerbsKnown(_) => format!(" (узнаёте {})", skill_tree::herbs_known(k)),
                _ => String::new(),
            };
            let line = format!(
                "{} {}{}",
                if met {
                    "есть —"
                } else {
                    "нужно —"
                },
                n.label(),
                have
            );
            c.spawn(theme.text(line, 14.0, if met { OK_INK } else { ink::RUBRIC }));
        }
        let links = perk.links();
        if !links.is_empty() {
            let names: Vec<&str> = links.iter().map(|s| s.name()).collect();
            c.spawn(theme.text(
                format!("Связано с: {}", names.join(", ").to_lowercase()),
                13.0,
                ink::INK_DIM,
            ));
        }
    });
}

fn skill_detail(p: &mut ChildSpawnerCommands, theme: &UiTheme, k: &Knowledge, skill: Skill) {
    let about = skill_tree::about(skill);
    let base = k.base_skill(skill);
    let bonus = k.skill(skill) - base;
    p.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(5),
        flex_basis: percent(55),
        flex_grow: 1.0,
        ..default()
    })
    .with_children(|c| {
        c.spawn(theme.title_colored(skill.name(), 24.0, ink::RUBRIC));
        let lvl = if bonus > 0.05 {
            format!("Навык {base:.1} из 100, в работе +{bonus:.0} от умений")
        } else {
            format!("Навык {base:.1} из 100")
        };
        c.spawn(theme.text(lvl, 13.0, ink::INK_DIM));
        c.spawn(theme.text(format!("Даёт: {}", about.gives), 15.0, ink::INK));
        c.spawn(theme.text(format!("Растёт: {}", about.grows), 15.0, ink::INK));
    });
    p.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(4),
        flex_basis: percent(45),
        flex_grow: 1.0,
        ..default()
    })
    .with_children(|c| {
        let spill = skill.spillover();
        if !spill.is_empty() {
            let names: Vec<&str> = spill.iter().map(|(s, _)| s.name()).collect();
            c.spawn(theme.text(
                format!("Заодно растёт: {}", names.join(", ").to_lowercase()),
                14.0,
                ink::INK,
            ));
        }
        let teachers = crate::guide::teachers(skill);
        c.spawn(theme.bold("Кто учит", 16.0, ink::RUBRIC));
        if teachers.is_empty() {
            c.spawn(theme.text("Учителя нет — учит само дело.", 14.0, ink::INK));
        }
        for t in teachers.iter().take(3) {
            c.spawn(theme.text(t.clone(), 13.0, ink::INK));
        }
        c.spawn(theme.text("Подробнее — в журнале (F1).", 13.0, ink::INK_DIM));
    });
}

/// Подсветка связей: наведённое умение, то, что ему нужно, и ветви, на которые оно опирается.
fn highlight_links(
    ui: Res<BookUi>,
    mut last: Local<Option<(Option<Focus>, u64)>>,
    mut perks: Query<(&PerkNode, &BaseBorder, &mut BorderColor), Without<SkillHeader>>,
    mut headers: Query<(&SkillHeader, &BaseBorder, &mut BorderColor), Without<PerkNode>>,
) {
    if !ui.open || ui.tab != Tab::Skills {
        return;
    }
    let focus = ui.focus();
    // После перестройки древа рамки новые — подсветить заново.
    if *last == Some((focus, ui.built)) {
        return;
    }
    *last = Some((focus, ui.built));
    let (lit_perks, lit_skills): (Vec<Perk>, Vec<Skill>) = match focus {
        Some(Focus::Perk(p)) => {
            let mut ps: Vec<Perk> = p
                .info()
                .needs
                .iter()
                .filter_map(|n| {
                    if let Need::Perk(q) = n {
                        Some(*q)
                    } else {
                        None
                    }
                })
                .collect();
            // И то, что откроется следом.
            ps.extend(
                Perk::ALL
                    .iter()
                    .filter(|q| q.info().needs.contains(&Need::Perk(p))),
            );
            ps.push(p);
            (ps, p.links())
        }
        Some(Focus::Skill(s)) => {
            let ps: Vec<Perk> = Perk::ALL
                .into_iter()
                .filter(|q| q.info().skill != s && q.links().contains(&s))
                .collect();
            let mut ss: Vec<Skill> = s.spillover().iter().map(|(t, _)| *t).collect();
            ss.push(s);
            (ps, ss)
        }
        None => (Vec::new(), Vec::new()),
    };
    for (n, base, mut color) in &mut perks {
        let want = if lit_perks.contains(&n.0) {
            LINK
        } else {
            base.0
        };
        *color = BorderColor::all(want);
    }
    for (h, base, mut color) in &mut headers {
        let want = if lit_skills.contains(&h.0) {
            LINK
        } else {
            base.0
        };
        *color = BorderColor::all(want);
    }
}

fn scroll_column(
    p: &mut ChildSpawnerCommands,
    flex_basis: Val,
    f: impl FnOnce(&mut ChildSpawnerCommands),
) {
    p.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            flex_basis,
            flex_grow: 1.0,
            min_width: px(0),
            min_height: px(0),
            row_gap: px(4),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition::default(),
        Interaction::default(),
        ScrollWheel,
    ))
    .with_children(f);
}

fn build_knowledge(p: &mut ChildSpawnerCommands, theme: &UiTheme, k: &Knowledge) {
    let [left, mid, right] = crate::lore_client::knowledge_lines(k);
    p.spawn(Node {
        width: percent(100),
        flex_grow: 1.0,
        min_height: px(0),
        column_gap: px(28),
        ..default()
    })
    .with_children(|row| {
        for (title, lines) in [
            ("Навыки и грамота", left),
            ("Травы и вещества", mid),
            ("Приёмы", right),
        ] {
            scroll_column(row, percent(33), |c| {
                c.spawn(theme.title_colored(title, 24.0, ink::RUBRIC));
                if lines.is_empty() {
                    c.spawn(theme.text(
                        "Пока ничего. Узнавайте у мастеров и из книг.",
                        15.0,
                        ink::INK_DIM,
                    ));
                }
                for line in lines {
                    if line.is_empty() {
                        c.spawn(Node {
                            height: px(6),
                            ..default()
                        });
                    } else if line.ends_with(':') {
                        c.spawn(theme.bold(line, 15.0, ink::RUBRIC));
                    } else {
                        c.spawn(theme.text(line, 15.0, ink::INK));
                    }
                }
            });
        }
    });
}

fn build_journal(p: &mut ChildSpawnerCommands, theme: &UiTheme, chapter: Chapter, hints_on: bool) {
    p.spawn(Node {
        width: percent(100),
        flex_grow: 1.0,
        min_height: px(0),
        column_gap: px(26),
        ..default()
    })
    .with_children(|row| {
        row.spawn(Node {
            flex_direction: FlexDirection::Column,
            width: px(230),
            flex_shrink: 0.0,
            row_gap: px(3),
            ..default()
        })
        .with_children(|list| {
            for c in Chapter::all() {
                let mut b = theme.toggle_button(list, &c.title(), ChapterButton(c), c == chapter);
                b.entry::<Node>().and_modify(|mut n| n.width = percent(100));
            }
            list.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            let label = if hints_on {
                "Подсказки: включены"
            } else {
                "Подсказки: выключены"
            };
            let mut b = theme.toggle_button(list, label, HintsToggle, hints_on);
            b.entry::<Node>().and_modify(|mut n| n.width = percent(100));
        });
        scroll_column(row, px(0), |page| {
            page.spawn(theme.title_colored(chapter.title(), 30.0, ink::RUBRIC));
            for para in chapter.page() {
                if let Some(h) = para.strip_prefix("# ") {
                    page.spawn((
                        theme.bold(h, 18.0, ink::RUBRIC),
                        Node {
                            margin: UiRect::top(px(8)),
                            ..default()
                        },
                    ));
                } else {
                    page.spawn(theme.text(para, 16.0, ink::INK));
                }
            }
        });
    });
}

/// `MRPG_SHOT_UI=skills|knowledge|journal[:глава]` — открыть книгу перед снимком.
fn dev_open_book(
    time: Res<Time>,
    mut ui: ResMut<BookUi>,
    mut done: Local<bool>,
    mut frames: Local<u32>,
) {
    if *done {
        return;
    }
    let delay: f32 = std::env::var("MRPG_SHOT_DELAY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8.0);
    *frames += 1;
    let ready = match std::env::var("MRPG_SHOT_FRAMES")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        Some(n) => *frames >= n,
        None => time.elapsed_secs() > delay - 1.5,
    };
    if !ready {
        return;
    }
    let want = std::env::var("MRPG_SHOT_UI").unwrap_or_default();
    let (tab, arg) = want.split_once(':').unwrap_or((want.as_str(), ""));
    match tab {
        "skills" => ui.show(Tab::Skills),
        "knowledge" => ui.show(Tab::Knowledge),
        "journal" => {
            let chapter = Chapter::all()
                .into_iter()
                .find(|c| {
                    c.title().to_lowercase().starts_with(&arg.to_lowercase()) && !arg.is_empty()
                })
                .unwrap_or(Chapter::Start);
            ui.show_chapter(chapter);
        }
        _ => {}
    }
    if let Ok(focus) = std::env::var("MRPG_SHOT_FOCUS") {
        ui.pinned = Perk::ALL
            .into_iter()
            .find(|p| p.info().name == focus)
            .map(Focus::Perk);
    }
    *done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_needs_mention_other_branches() {
        let s = short_needs(Perk::HeavyHand);
        assert!(s.starts_with("от 35"), "{s}");
        assert!(s.contains("Ратное 15"), "{s}");
        assert!(short_needs(Perk::ForgeEye).contains("знание"));
    }

    #[test]
    fn perk_states() {
        let mut k = Knowledge::default();
        let e = skill_tree::earned(&k);
        // Нужен только навык — «почти».
        assert!(perk_state(Perk::PortersBack, &k, &e) == State::Near);
        // Нужен приём — «далеко».
        assert!(perk_state(Perk::ForgeEye, &k, &e) == State::Far);
        k.skills[Skill::Strength.index()] = 20.0;
        let e = skill_tree::earned(&k);
        assert!(perk_state(Perk::PortersBack, &k, &e) == State::Earned);
    }
}
