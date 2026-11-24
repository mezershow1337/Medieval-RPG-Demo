//! Клиент: окно варки. Открывается F или ПКМ по горшку, котелку или ступке в сумке.
//!
//! В окне видно, что из сумки можно положить в сосуд (и сколько это драхм), сколько налить
//! воды, какой огонь и сколько варить. Ниже — что выходит по весу (чтобы игрок, нашедший
//! рецепт у Галена, мог отмерить «один к четырём»), и подсказки из выученных приёмов.
//!
//! Драхмы видит только тот, у кого есть весы с гирями или кто выучил дозировку; остальные
//! сыплют на глаз — горстями и щепотями.
#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::brewing::{self, BrewSpec, DRACHMS_PER_KOTYLE, Heat, Vessel};
use crate::inventory_ui::UiFonts;
use crate::items::ItemUid;
use crate::loot_client::LootState;
use crate::lore_client::{MyKnowledge, send};
use crate::lore_net::LoreClientMessage;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy_renet::RenetClient;
use std::collections::BTreeMap;

pub struct BrewingClientPlugin;

impl Plugin for BrewingClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BrewState::default())
            .add_systems(Startup, setup_brew_panel.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (brew_buttons, rebuild_brew_panel).chain());
    }
}

const TIME_STEPS: [f32; 14] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 24.0, 48.0, 72.0, 96.0];

#[derive(Resource)]
pub struct BrewState {
    open: Option<(ItemUid, Vessel)>,
    counts: BTreeMap<String, u16>,
    water: u8,
    heat: Heat,
    hours: f32,
    bath: bool,
    rev: u64,
    built: Option<(u64, u64, u64)>,
}

impl Default for BrewState {
    fn default() -> Self {
        Self { open: None, counts: BTreeMap::new(), water: 2, heat: Heat::Low, hours: 1.0, bath: false, rev: 1, built: None }
    }
}

impl BrewState {
    /// Открыть окно для сосуда из сумки.
    pub fn open_vessel(&mut self, uid: ItemUid, vessel: Vessel) {
        if self.open.map(|o| o.0) != Some(uid) {
            self.counts.clear();
            self.bath = false;
            if !vessel.takes_water() {
                self.water = 0;
            }
            match vessel {
                Vessel::Mortar => {
                    self.heat = Heat::None;
                    self.hours = 0.25;
                }
                Vessel::Crucible => self.heat = Heat::High,
                Vessel::DryingTray => {
                    self.heat = Heat::None;
                    self.hours = 12.0;
                }
                v if v.still() => self.heat = Heat::Medium,
                _ => {}
            }
        }
        self.open = Some((uid, vessel));
        self.rev += 1;
    }
}

#[derive(Clone, PartialEq, Debug)]
enum BrewAction {
    Add(String),
    Sub(String),
    WaterUp,
    WaterDown,
    Heat(Heat),
    TimeUp,
    TimeDown,
    Bath,
    Start,
    Close,
}

#[derive(Component)]
struct BrewButton(BrewAction);

#[derive(Component)]
struct BrewPanel;

use crate::ui_theme::{Skin, ink, theme};

const TITLE: Color = ink::TITLE;
const TEXT: Color = ink::TEXT;
const DIM: Color = ink::DIM;

fn setup_brew_panel(mut commands: Commands) {
    commands.spawn((
        BrewPanel,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            left: px(12),
            top: px(60),
            width: px(440),
            max_height: percent(88),
            flex_direction: FlexDirection::Column,
            padding: Skin::Wood.padding(),
            row_gap: px(4),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition::default(),
        crate::ui_theme::ScrollWheel,
        theme().skin(Skin::Wood),
        GlobalZIndex(30),
        Interaction::default(),
        FocusPolicy::Block,
    ));
}

fn label(fonts: &UiFonts, s: impl Into<String>, size: f32, color: Color, bold: bool) -> impl Bundle {
    (
        Text::new(s),
        TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: bevy::text::FontSize::Px(size), ..default() },
        TextColor(color),
    )
}

fn button(p: &mut ChildSpawnerCommands, fonts: &UiFonts, text: &str, action: BrewAction, on: bool) {
    let _ = fonts;
    theme().toggle_button(p, text, (BrewButton(action), FocusPolicy::Block), on);
}

fn row<'a>(p: &'a mut ChildSpawnerCommands<'_>) -> EntityCommands<'a> {
    p.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), ..default() })
}

fn brew_buttons(
    buttons: Query<(&Interaction, &BrewButton), Changed<Interaction>>,
    mut state: ResMut<BrewState>,
    loot: Res<LootState>,
    mut client: Option<ResMut<RenetClient>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if state.open.is_some() && (!loot.ui_open || keys.just_pressed(KeyCode::Escape)) {
        state.open = None;
        state.rev += 1;
        return;
    }
    let Some((vessel, _)) = state.open else { return };
    for (i, b) in &buttons {
        if *i != Interaction::Pressed {
            continue;
        }
        let have = |name: &str| loot.inventory.as_ref().map_or(0, |inv| inv.all_items().filter(|i| i.info().name == name).map(|i| i.count as u32).sum::<u32>());
        match &b.0 {
            BrewAction::Add(n) => {
                let c = state.counts.get(n).copied().unwrap_or(0);
                if (c as u32) < have(n) {
                    state.counts.insert(n.clone(), c + 1);
                }
            }
            BrewAction::Sub(n) => {
                let c = state.counts.get(n).copied().unwrap_or(0);
                if c <= 1 {
                    state.counts.remove(n);
                } else {
                    state.counts.insert(n.clone(), c - 1);
                }
            }
            BrewAction::WaterUp => state.water = (state.water + 1).min(brewing::MAX_WATER),
            BrewAction::WaterDown => state.water = state.water.saturating_sub(1),
            BrewAction::Heat(h) => state.heat = *h,
            BrewAction::Bath => state.bath = !state.bath,
            BrewAction::TimeUp => state.hours = TIME_STEPS.iter().copied().find(|t| *t > state.hours + 1e-3).unwrap_or(state.hours),
            BrewAction::TimeDown => state.hours = TIME_STEPS.iter().rev().copied().find(|t| *t < state.hours - 1e-3).unwrap_or(state.hours),
            BrewAction::Start => {
                let spec = BrewSpec { items: state.counts.iter().map(|(n, c)| (n.clone(), *c)).collect(), water: state.water, heat: state.heat, hours: state.hours, bath: state.bath && state.heat != Heat::None };
                if let Some(client) = client.as_mut() {
                    send(client, &LoreClientMessage::Brew { vessel, spec });
                }
                state.counts.clear();
            }
            BrewAction::Close => state.open = None,
        }
        state.rev += 1;
    }
}

fn rebuild_brew_panel(
    mut commands: Commands,
    mut state: ResMut<BrewState>,
    loot: Res<LootState>,
    knowledge: Res<MyKnowledge>,
    fonts: Res<UiFonts>,
    mut panel: Query<(Entity, &mut Node), With<BrewPanel>>,
    mut dock: ResMut<crate::ui_theme::SideDock>,
) {
    let Ok((entity, mut node)) = panel.single_mut() else { return };
    let visible = state.open.is_some() && loot.ui_open;
    let want = if visible { Display::Flex } else { Display::None };
    if node.display != want {
        if visible {
            commands
                .entity(entity)
                .insert(crate::ui_theme::Appear::default());
        }
        node.display = want;
    }
    let w = if visible { 440.0 } else { 0.0 };
    if dock.brew != w {
        dock.brew = w;
    }
    if !visible {
        return;
    }
    let key = (state.rev, loot.rev, knowledge.0.known.len() as u64);
    if state.built == Some(key) && !knowledge.is_changed() {
        return;
    }
    state.built = Some(key);
    let Some((_, vessel)) = state.open else { return };
    let Some(inv) = loot.inventory.as_ref() else { return };
    // Что в сумке можно положить в сосуд.
    let mut stock: BTreeMap<String, (u32, crate::items::Item)> = BTreeMap::new();
    for item in inv.all_items() {
        let n = item.info().name;
        if brewing::mass(n).is_some() {
            let e = stock.entry(n.to_string()).or_insert((0, item.clone()));
            e.0 += item.count as u32;
        }
    }
    // Положенного больше нет в сумке — убрать.
    let counts: Vec<(String, u16)> = state.counts.iter().map(|(n, c)| (n.clone(), *c)).collect();
    for (n, c) in counts {
        let have = stock.get(&n).map_or(0, |s| s.0);
        if have == 0 {
            state.counts.remove(&n);
        } else if c as u32 > have {
            state.counts.insert(n, have as u16);
        }
    }
    let flasks: u32 = inv.all_items().filter(|i| i.info().name == brewing::FLASK).map(|i| i.count as u32).sum();
    // Весы или выученная дозировка — точные драхмы; иначе — на глаз.
    let weighs = inv.all_items().any(|i| i.info().name == "Весы с гирями") || knowledge.0.knows(crate::lore::Lore::Technique(crate::lore::Technique::Dosage));
    let pots = inv.all_items().filter(|i| matches!(i.info().name, "Глиняный горшок" | "Медный котелок")).count();
    let dry: f32 = state.counts.iter().map(|(n, c)| brewing::mass(n).unwrap_or(0.0) * *c as f32).sum::<f32>() + 0.0;
    let water = state.water as f32 * DRACHMS_PER_KOTYLE;

    commands.entity(entity).despawn_children();
    let state = &*state;
    let fonts = &*fonts;
    commands.entity(entity).with_children(|p| {
        row(p).with_children(|r| {
            r.spawn((
                theme().title(vessel.name(), 26.0),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));
            button(r, fonts, "×", BrewAction::Close, false);
        });
        theme().divider(p);
        p.spawn(label(
            fonts,
            "Что положить (вес одной штуки):",
            13.0,
            DIM,
            false,
        ));
        if stock.is_empty() {
            p.spawn(label(fonts, "В сумке нет ничего, что кладут в сосуд", 14.0, DIM, false));
        }
        for (name, (have, item)) in &stock {
            let c = state.counts.get(name).copied().unwrap_or(0);
            let shown = knowledge.item_name(item);
            let mut shown = shown.split(" ×").next().unwrap_or(&shown).to_string();
            // Незнакомые травы различаются только видом.
            if let Some(sub) = crate::lore::Substance::from_item(item.base)
                && !knowledge.0.identifies(sub)
            {
                shown = format!("{shown} ({})", sub.info().look);
            }
            row(p).with_children(|r| {
                button(r, fonts, "−", BrewAction::Sub(name.clone()), false);
                r.spawn(label(fonts, format!("{c}/{have}"), 14.0, if c > 0 { TITLE } else { TEXT }, true));
                button(r, fonts, "+", BrewAction::Add(name.clone()), false);
                let m = brewing::mass(name).unwrap_or(0.0);
                let measure = if weighs { format!("{m:.0} др.") } else { by_eye(name, m).to_string() };
                r.spawn(label(fonts, format!("{shown} · {measure}"), 14.0, TEXT, false));
            });
        }
        if vessel.takes_water() {
            row(p).with_children(|r| {
                button(r, fonts, "−", BrewAction::WaterDown, false);
                let w = if weighs { format!("Вода: {} кот. ({:.0} др.)", state.water, water) } else { format!("Вода: {} кружк.", state.water) };
                r.spawn(label(fonts, w, 14.0, TEXT, true));
                button(r, fonts, "+", BrewAction::WaterUp, false);
            });
        }
        if vessel != Vessel::Mortar {
            row(p).with_children(|r| {
                r.spawn(label(fonts, "Огонь:", 14.0, TEXT, true));
                for h in Heat::ALL {
                    button(r, fonts, h.name(), BrewAction::Heat(h), state.heat == h);
                }
            });
        }
        // Водяная баня Марии: горшок в котле с водой.
        if vessel.takes_water() && state.heat != Heat::None {
            row(p).with_children(|r| {
                button(r, fonts, if state.bath { "Водяная баня: да" } else { "Водяная баня: нет" }, BrewAction::Bath, state.bath);
                let note = if pots >= 2 { "не пригорит, жар не выше кипения" } else { "нужен второй горшок или котелок" };
                r.spawn(label(fonts, note, 13.0, DIM, false));
            });
        }
        row(p).with_children(|r| {
            button(r, fonts, "−", BrewAction::TimeDown, false);
            let t = if state.hours >= 24.0 { format!("{:.0} сут.", state.hours / 24.0) } else if state.hours < 1.0 { format!("{:.0} мин", state.hours * 60.0) } else { format!("{} ч", state.hours) };
            r.spawn(label(fonts, format!("Время: {t}"), 14.0, TEXT, true));
            button(r, fonts, "+", BrewAction::TimeUp, false);
        });
        if weighs {
            let ratio = if dry > 0.0 && water > 0.0 { format!(" · воды к смеси {:.1} : 1", water / dry) } else { String::new() };
            p.spawn(label(fonts, format!("Смесь: {dry:.0} др.{ratio}"), 13.0, DIM, false));
        } else {
            p.spawn(label(fonts, "Сыплете на глаз: без весов драхм не узнать", 13.0, DIM, false));
        }
        // Доли по весу — чтобы отмерить по книге.
        if weighs && state.counts.len() > 1 {
            let parts: Vec<String> = state.counts.iter().map(|(n, c)| format!("{} {:.0}%", n.to_lowercase(), brewing::mass(n).unwrap_or(0.0) * *c as f32 / dry * 100.0)).collect();
            p.spawn(label(fonts, parts.join(", "), 13.0, DIM, false));
        }
        p.spawn(label(fonts, format!("Пустых флаконов: {flasks} (жидкое разливают во флаконы)"), 13.0, DIM, false));
        if vessel.glass() {
            p.spawn(label(fonts, "Стекло боится жара: остужайте купол мокрой тряпицей (3), иначе лопнет", 13.0, DIM, false));
        }
        if state.heat != Heat::None && vessel != Vessel::Mortar {
            p.spawn(label(fonts, "Огонь гуляет: 1 — раздуть мехами, 2 — прикрыть заслонку", 13.0, DIM, false));
        }
        if state.heat != Heat::None && vessel != Vessel::Mortar && !crate::level::hearths().is_empty() {
            p.spawn(label(fonts, "На огне варят у очага или костра (в домах, мастерских, у костров за стенами)", 13.0, DIM, false));
        }
        row(p).with_children(|r| {
            let verb = match vessel {
                Vessel::Mortar => "Растереть",
                Vessel::DryingTray => "Сушить",
                Vessel::Crucible => "Обжигать",
                v if v.still() => "Перегонять",
                _ if state.heat == Heat::None => "Начать",
                _ => "Варить",
            };
            theme().button(
                r,
                verb,
                (BrewButton(BrewAction::Start), FocusPolicy::Block),
                !state.counts.is_empty(),
            );
            r.spawn(label(
                fonts,
                format!(
                    "≈ {:.0} с",
                    BrewSpec {
                        items: vec![],
                        water: 0,
                        heat: state.heat,
                        hours: state.hours,
                        bath: false
                    }
                    .real_secs()
                ),
                13.0,
                DIM,
                false,
            ));
        });
        let hints = brewing::known_hints(&knowledge.0);
        if hints.is_empty() {
            p.spawn(label(fonts, "Рецептов вы не знаете: пробуйте сами или учитесь.", 13.0, DIM, false));
        } else {
            p.spawn(label(fonts, "Что вы знаете:", 14.0, TITLE, true));
            for (_, hint) in hints {
                p.spawn(label(fonts, format!("• {hint}"), 13.0, TEXT, false));
            }
        }
    });
}

/// Сколько это «на глаз».
fn by_eye(name: &str, m: f32) -> &'static str {
    if name.contains("Кувшин") || name.contains("Амфора") || name.contains("Горшок") {
        return "сосуд";
    }
    match brewing::canonical(name).1 {
        brewing::Form::Dried => return "пучок",
        brewing::Form::Ground => return "горстка порошка",
        brewing::Form::Raw => {}
    }
    match m {
        m if m <= 2.0 => "щепоть",
        m if m <= 6.0 => "кусочек",
        m if m <= 10.0 => "горсть",
        m if m <= 30.0 => "ком",
        _ => "много",
    }
}
