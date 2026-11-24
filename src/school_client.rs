//! Клиент: окно учителя грамоты (школа грамматиста, грамматик, писец, нотарий, сочинитель,
//! книгописец, философ). Открывается по E у учителя.
//!
//! В окне — кто учит и когда, что он даёт (азбука, чтение, письмо, счёт, грамматика, латынь),
//! что уже выучено и что сначала надо пройти. «Урок» платит учителю и начинает занятие с
//! руками (обводить буквы на воске, писать пером, считать камешками — см. work.rs);
//! «Помогать» — работа за плату (растирать чернила, подметать, носить книги).
#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::economy::format_money;
use crate::loot_client::LootState;
use crate::lore::{Knowledge, Lore, Teacher};
use crate::lore_client::{LoreView, MyKnowledge, send};
use crate::lore_net::{LoreClientMessage, MasterView};
use crate::player::LocalPlayer;
use crate::ui_theme::{AppearChildren, Skin, ink, theme};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;

pub struct SchoolClientPlugin;

impl Plugin for SchoolClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SchoolUi::default())
            .add_systems(Startup, setup_school.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (shot_school, school_input, school_clicks, rebuild_school).chain());
    }
}

/// Открытое окно учителя.
#[derive(Resource, Default)]
pub struct SchoolUi {
    master: Option<u32>,
    /// Где стоял игрок, когда открыл (отошёл — окно закрывается).
    at: Option<Vec3>,
    built: Option<(u32, u64)>,
    rev: u64,
}

impl SchoolUi {
    pub fn open(&self) -> bool {
        self.master.is_some()
    }

    /// Открыть окно учителя (номер мастера).
    pub fn show(&mut self, master: u32) {
        self.master = Some(master);
        self.at = None;
        self.rev += 1;
    }

    fn close(&mut self) {
        self.master = None;
        self.at = None;
        self.rev += 1;
    }
}

#[derive(Component)]
struct SchoolRoot;

#[derive(Component)]
struct SchoolBody;

#[derive(Component, Clone, Copy)]
enum SchoolButton {
    Lesson(Lore),
    Help,
    Close,
}

fn setup_school(mut commands: Commands) {
    let theme = theme();
    commands
        .spawn((
            SchoolRoot,
            crate::civil_client::ShotKeep,
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
            GlobalZIndex(23),
        ))
        .with_children(|root| {
            root.spawn((
                SchoolBody,
                Node {
                    width: px(660),
                    max_width: percent(94),
                    max_height: percent(92),
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
fn school_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<SchoolUi>,
    mut loot: ResMut<LootState>,
    view: Res<LoreView>,
    work: Option<Res<crate::work_client::MyWork>>,
    me: Query<&Transform, With<LocalPlayer>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut root: Single<(Entity, &mut Node), With<SchoolRoot>>,
    mut commands: Commands,
    mut was_open: Local<bool>,
) {
    let pos = me.iter().next().map(|t| t.translation);
    if let Some(id) = ui.master {
        let gone = view.master(id).is_none();
        let walked = match (ui.at, pos) {
            (Some(a), Some(p)) => a.distance(p) > 3.0,
            _ => false,
        };
        // Урок начался — руки заняты, окно больше не нужно.
        let busy = crate::work_client::hands_busy(work.as_deref());
        if gone || walked || busy || keys.just_pressed(KeyCode::Escape) {
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
    if open && (cursor.grab_mode != CursorGrabMode::None || !cursor.visible) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    if *was_open && !open && !loot.ui_open && !keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    *was_open = open;
}

fn school_clicks(buttons: Query<(&Interaction, &SchoolButton, &crate::ui_theme::ThemeButton), Changed<Interaction>>, mut ui: ResMut<SchoolUi>, mut client: Option<ResMut<RenetClient>>) {
    let Some(master) = ui.master else { return };
    for (i, b, t) in &buttons {
        if *i != Interaction::Pressed || !t.enabled {
            continue;
        }
        let msg = match *b {
            SchoolButton::Lesson(lore) => Some(LoreClientMessage::Study { master, lore }),
            SchoolButton::Help => Some(LoreClientMessage::Apprentice { master }),
            SchoolButton::Close => None,
        };
        if let (Some(msg), Some(client)) = (msg, client.as_mut()) {
            send(client, &msg);
        }
        ui.close();
        return;
    }
}

/// Снимок окна (`MRPG_SHOT_UI=school` или `school:notary|scribe|grammarian|logograph|copyist|philosopher`):
/// открыть окно учителя, как только сервер пришлёт мастеров.
fn shot_school(mut ui: ResMut<SchoolUi>, mut view: ResMut<LoreView>, mut knowledge: ResMut<MyKnowledge>, mut done: Local<bool>, mut frames: Local<u32>) {
    if *done {
        return;
    }
    let Ok(want) = std::env::var("MRPG_SHOT_UI") else {
        *done = true;
        return;
    };
    let Some(rest) = want.strip_prefix("school") else {
        *done = true;
        return;
    };
    let teacher = match rest.trim_start_matches(':') {
        "notary" => Teacher::Notary,
        "scribe" => Teacher::Scribe,
        "grammarian" => Teacher::Grammarian,
        "logograph" => Teacher::Logograph,
        "copyist" => Teacher::Copyist,
        "philosopher" => Teacher::Philosopher,
        _ => Teacher::Grammatist,
    };
    if let Some(id) = view.masters_iter().find(|m| m.teacher == teacher).map(|m| m.id) {
        ui.show(id);
        *done = true;
        return;
    }
    // Без сервера (облачные снимки идут по кадру в секунды, и связь рвётся) — учитель из
    // раскладки уровня, а ученик уже знает азбуку и начал чтение.
    *frames += 1;
    if *frames < 20 {
        return;
    }
    let people: Vec<crate::school::Townsman> = crate::level::layout().residents.iter().map(|r| crate::school::Townsman { role: Some(r.role), pos: r.pos, yaw: r.yaw }).collect();
    if let Some(t) = crate::school::tutors(&people).into_iter().find(|t| t.teacher == teacher) {
        let id = 9_000;
        view.insert_demo_master(MasterView { id, teacher, pos: t.pos.into(), yaw: t.yaw, price: t.price, wage: t.wage, resident: t.resident.map(|r| r as u32), hours: t.hours, present: true, place: t.place.to_string() });
        knowledge.0.learn(Lore::Alphabet);
        knowledge.0.lessons.insert(Lore::Read(crate::lore::Language::Greek), 0.35);
        ui.show(id);
    }
    *done = true;
}

/// Сколько примерно уроков осталось до знания (урок засчитывается за 0.5–1.3 часа).
fn lessons_left(k: &Knowledge, teacher: Teacher, lore: Lore) -> u32 {
    let need = lore.lesson_hours() / teacher.pace(lore) / crate::skill_tree::Mods::of(k).lessons;
    let done = k.lessons.get(&lore).copied().unwrap_or(0.0);
    ((1.0 - done) * need).ceil().max(1.0) as u32
}

fn plural_lessons(n: u32) -> &'static str {
    match (n % 10, n % 100) {
        (1, x) if x != 11 => "урок",
        (2..=4, x) if !(12..=14).contains(&x) => "урока",
        _ => "уроков",
    }
}

fn rebuild_school(
    mut commands: Commands,
    mut ui: ResMut<SchoolUi>,
    view: Res<LoreView>,
    knowledge: Res<MyKnowledge>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
    body: Single<Entity, With<SchoolBody>>,
) {
    let Some(id) = ui.master else {
        ui.built = None;
        return;
    };
    let Some(m) = view.master(id).cloned() else { return };
    let hour = clock.as_ref().map_or(12.0, |c| c.hour());
    // Перестраиваем при смене учителя, знаний, присутствия или раз в игровой час (надпись «учит до…»).
    let key = (id, ui.rev ^ (m.present as u64) << 40 ^ (hour as u64) << 48);
    if ui.built == Some(key) && !knowledge.is_changed() {
        return;
    }
    ui.built = Some(key);
    let theme = theme();
    let k = &knowledge.0;
    commands.entity(*body).despawn_children();
    commands.entity(*body).with_children(|p| {
        build(p, theme, &m, k, hour);
    });
}

fn build(p: &mut ChildSpawnerCommands, theme: &crate::ui_theme::UiTheme, m: &MasterView, k: &Knowledge, hour: f32) {
    let teacher = m.teacher;
    // Шапка: кто и где.
    p.spawn(Node { width: percent(100), align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|head| {
        head.spawn(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
            t.spawn(theme.title_colored(crate::school::tutor_title(teacher), 28.0, ink::RUBRIC));
            let place = crate::level::place(&m.place).map_or(String::new(), |pl| format!("{} · ", pl.name));
            t.spawn(theme.text(format!("{place}учит {}", crate::school::hours_text(m.hours)), 13.0, ink::INK_DIM));
        });
        theme.small_button(head, "Закрыть", SchoolButton::Close, true);
    });
    let (state, color) = if m.present {
        ("Сейчас на месте и учит.".to_string(), ink::INK)
    } else {
        (crate::school::away_line(teacher, hour, m.hours), ink::RUBRIC)
    };
    p.spawn(theme.bold(state, 14.0, color));
    let greet = crate::school::tutor_greeting(teacher);
    if !greet.is_empty() {
        p.spawn((theme.text(format!("«{greet}»"), 14.0, ink::INK_DIM), Node { margin: UiRect::vertical(px(2)), ..default() }));
    }
    theme.divider(p);

    let letters = k.skill(crate::lore::Skill::Letters);
    p.spawn(theme.text(format!("Книжное дело: {:.0} · урок — {}", letters, format_money(m.price)), 13.0, ink::INK_DIM));
    for lore in teacher.curriculum() {
        let known = k.knows(lore);
        let missing: Vec<Lore> = lore.requires().into_iter().filter(|r| !k.knows(*r)).collect();
        let started = k.lessons.get(&lore).copied().unwrap_or(0.0);
        p.spawn(Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(10),
            padding: UiRect::vertical(px(4)),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node { flex_grow: 1.0, flex_shrink: 1.0, flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|c| {
                c.spawn(theme.bold(lore.subject(), 17.0, if known { ink::INK_DIM } else { ink::INK }));
                c.spawn(theme.text(lore.subject_about(), 13.0, ink::INK_DIM));
                let (note, col) = if known {
                    ("Выучено.".to_string(), Color::srgb(0.20, 0.45, 0.15))
                } else if !missing.is_empty() {
                    let names: Vec<String> = missing.iter().map(|l| l.subject().to_lowercase()).collect();
                    (format!("Сначала: {}.", names.join(", ")), ink::RUBRIC)
                } else {
                    let n = lessons_left(k, teacher, lore);
                    let begun = if started > 0.0 { format!("пройдено {:.0}%, ", started * 100.0) } else { String::new() };
                    (format!("{begun}ещё около {n} {}.", plural_lessons(n)), ink::INK)
                };
                c.spawn(theme.text(note, 13.0, col));
            });
            let can = k.can_study(teacher, lore) && m.present;
            let label = if known { "Выучено".to_string() } else { format!("Урок · {}", crate::economy::format_money_short(m.price)) };
            theme.small_button(row, &label, SchoolButton::Lesson(lore), can);
        });
    }
    theme.divider(p);
    if m.wage > 0 {
        p.spawn(Node { width: percent(100), align_items: AlignItems::Center, column_gap: px(10), ..default() }).with_children(|row| {
            row.spawn((
                theme.text("Нет денег на уроки — помогайте учителю: растирать чернила, подметать, носить книги. Платят мало, но кое-чему научитесь.", 13.0, ink::INK_DIM),
                Node { flex_grow: 1.0, flex_shrink: 1.0, ..default() },
            ));
            theme.small_button(row, &format!("Помогать · {}", crate::economy::format_money_short(m.wage)), SchoolButton::Help, m.present);
        });
    }
    p.spawn(theme.text("Esc — закрыть. Отойдёте от учителя — окно закроется.", 12.0, ink::INK_DIM));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_plurals() {
        assert_eq!(plural_lessons(1), "урок");
        assert_eq!(plural_lessons(3), "урока");
        assert_eq!(plural_lessons(5), "уроков");
        assert_eq!(plural_lessons(11), "уроков");
        assert_eq!(plural_lessons(22), "урока");
    }

    #[test]
    fn lessons_left_counts_down() {
        let mut k = Knowledge::default();
        let n0 = lessons_left(&k, Teacher::Grammatist, Lore::Alphabet);
        k.lessons.insert(Lore::Alphabet, 0.5);
        let n1 = lessons_left(&k, Teacher::Grammatist, Lore::Alphabet);
        assert!(n1 < n0 && n1 >= 1);
    }
}
