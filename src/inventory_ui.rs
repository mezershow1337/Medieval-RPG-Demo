//! Окно инвентаря: сетка сумки, слоты экипировки, окно обыска тела, подсказки.
//!
//! Управление:
//! - Tab или I — открыть/закрыть инвентарь; Esc — закрыть.
//! - ЛКМ по предмету — взять; ЛКМ по клетке или слоту — положить; ЛКМ мимо окон — выбросить.
//! - ПКМ или Shift+ЛКМ — быстро переложить (из тела в сумку, из сумки в тело или надеть/снять).
//! - Q над предметом — выбросить на землю.

#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще для сетки.

use crate::inventory::{Inventory, Loc, Target};
use crate::items::{EquipSlot, Item, ItemUid, Quality, Stat};
use crate::loot_client::{LootState, send};
use crate::loot_net::{LootClientMessage, Owner};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;

/// Размер клетки сетки в пикселях.
const CELL: f32 = 44.0;
const SLOT: f32 = 76.0;
const SMALL_SLOT: f32 = 50.0;

const PANEL_BG: Color = Color::srgba(0.11, 0.09, 0.07, 0.96);
const PANEL_BORDER: Color = Color::srgb(0.45, 0.36, 0.22);
const CELL_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
const CELL_BORDER: Color = Color::srgba(0.4, 0.34, 0.25, 0.35);
const TITLE: Color = Color::srgb(0.93, 0.83, 0.6);
const DIM: Color = Color::srgb(0.6, 0.56, 0.5);

/// Шрифты с кириллицей (Lora, лицензия OFL — assets/fonts/Lora-OFL.txt).
#[derive(Resource, Clone)]
pub struct UiFonts {
    pub regular: Handle<Font>,
    pub bold: Handle<Font>,
}

pub fn setup_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    // Встраиваем в exe, чтобы игра не зависела от папки запуска.
    let regular = fonts.add(Font::from_bytes(include_bytes!("../assets/fonts/Lora-Regular.ttf").to_vec()));
    let bold = fonts.add(Font::from_bytes(include_bytes!("../assets/fonts/Lora-Bold.ttf").to_vec()));
    commands.insert_resource(UiFonts { regular, bold });
}

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Held::default())
            .insert_resource(Built::default())
            .insert_resource(TooltipFor::default())
            .add_systems(Startup, (setup_fonts, crate::icons::setup_icons, setup_tooltip).chain())
            .add_systems(Update, (toggle_inventory, rebuild_ui, handle_input, highlight_cells, update_tooltip, move_ghost).chain())
            .add_systems(PostUpdate, free_cursor_while_open);
    }
}

// ---------------------------------------------------------------------------
// Состояние окна
// ---------------------------------------------------------------------------

/// Предмет «в руке» (взяли мышью, ещё не положили).
#[derive(Clone)]
struct HeldItem {
    owner: Owner,
    item: Item,
    /// За какую клетку предмета взяли.
    grab: (u8, u8),
}

#[derive(Resource, Default)]
struct Held(Option<HeldItem>);

/// Что сейчас нарисовано (чтобы перестраивать окно только при изменениях).
#[derive(Resource, Default, PartialEq, Clone)]
struct Built {
    rev: u64,
    open: bool,
    held: Option<ItemUid>,
}

#[derive(Resource, Default)]
struct TooltipFor(Option<(Owner, ItemUid)>);

#[derive(Component)]
struct InventoryRoot;

#[derive(Component)]
struct Tooltip;

#[derive(Component)]
struct HeldGhost;

/// Во что попадает мышь.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Hit {
    Cell { owner: Owner, x: u8, y: u8 },
    Equip { owner: Owner, slot: EquipSlot },
    Item { owner: Owner, uid: ItemUid },
    TakeAll,
    Panel,
    Outside,
}

fn text(fonts: &UiFonts, s: impl Into<String>, size: f32, color: Color, bold: bool) -> impl Bundle {
    (
        Text::new(s),
        TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: bevy::text::FontSize::Px(size), ..default() },
        TextColor(color),
    )
}

fn quality_color(q: Quality) -> Color {
    let [r, g, b] = q.color();
    Color::srgb(r, g, b)
}

fn item_bg(q: Quality) -> Color {
    quality_color(q).mix(&Color::srgb(0.06, 0.05, 0.04), 0.82)
}

fn owner_inv(state: &LootState, owner: Owner) -> Option<&Inventory> {
    match owner {
        Owner::Me => state.inventory.as_ref(),
        Owner::Container(id) => state.container.as_ref().filter(|c| c.id == id).map(|c| &c.inv),
    }
}

// ---------------------------------------------------------------------------
// Открытие и закрытие
// ---------------------------------------------------------------------------

fn toggle_inventory(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<LootState>,
    mut held: ResMut<Held>,
    mut client: Option<ResMut<RenetClient>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if state.inventory.is_none() {
        return;
    }
    let toggle = keys.just_pressed(KeyCode::Tab) || keys.just_pressed(KeyCode::KeyI);
    let close_with_e = state.ui_open && state.container.is_some() && keys.just_pressed(KeyCode::KeyE);
    let escape = state.ui_open && keys.just_pressed(KeyCode::Escape);
    if !(toggle || close_with_e || escape) {
        return;
    }
    if state.ui_open {
        state.ui_open = false;
        held.0 = None;
        if state.container.take().is_some()
            && let Some(c) = client.as_mut()
        {
            send(c, &LootClientMessage::Close);
        }
        if !escape {
            // Возвращаемся в игру: снова захватываем мышь.
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    } else {
        state.ui_open = true;
    }
    state.rev += 1;
}

/// Пока окно открыто, мышь свободна (персонаж не крутит головой и не ходит).
fn free_cursor_while_open(state: Res<LootState>, mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    if state.ui_open && (cursor.grab_mode != CursorGrabMode::None || !cursor.visible) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

// ---------------------------------------------------------------------------
// Построение окна
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn rebuild_ui(
    mut commands: Commands,
    state: Res<LootState>,
    held: Res<Held>,
    fonts: Res<UiFonts>,
    icons: Res<crate::icons::ItemIcons>,
    mut built: ResMut<Built>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut tooltip_for: ResMut<TooltipFor>,
) {
    let want = Built { rev: state.rev, open: state.ui_open, held: held.0.as_ref().map(|h| h.item.uid) };
    if *built == want && (roots.is_empty() != state.ui_open) {
        return;
    }
    *built = want;
    for e in &roots {
        commands.entity(e).despawn();
    }
    tooltip_for.0 = None;
    let Some(inv) = state.inventory.as_ref().filter(|_| state.ui_open) else { return };
    let held_uid = held.0.as_ref().map(|h| h.item.uid);
    let res = UiRes { fonts: fonts.clone(), icons: icons.0.clone() };

    commands
        .spawn((
            InventoryRoot,
            Hit::Outside,
            Interaction::default(),
            FocusPolicy::Pass,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(14),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            if let Some(c) = &state.container {
                panel(root).with_children(|p| {
                    p.spawn(text(&fonts, c.title.clone(), 22.0, TITLE, true));
                    p.spawn(text(&fonts, "Снаряжение", 14.0, DIM, false));
                    p.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), max_width: px(c.inv.grid.w as f32 * CELL), ..default() })
                        .with_children(|row| {
                            for slot in EquipSlot::ALL {
                                equip_slot(row, &res, Owner::Container(c.id), slot, c.inv.equipped(slot), SMALL_SLOT, held_uid);
                            }
                        });
                    p.spawn(text(&fonts, "Сумка", 14.0, DIM, false));
                    grid(p, &res, Owner::Container(c.id), &c.inv, held_uid);
                    if !c.inv.is_empty() {
                        p.spawn((
                            Hit::TakeAll,
                            Interaction::default(),
                            FocusPolicy::Pass,
                            Node { padding: UiRect::axes(px(14), px(6)), border: UiRect::all(px(1)), align_self: AlignSelf::FlexStart, ..default() },
                            BorderColor::all(PANEL_BORDER),
                            BackgroundColor(Color::srgba(0.3, 0.22, 0.12, 0.9)),
                        ))
                        .with_children(|b| {
                            b.spawn(text(&fonts, "Забрать всё", 16.0, TITLE, true));
                        });
                    } else {
                        p.spawn(text(&fonts, "Ничего не осталось", 15.0, DIM, false));
                    }
                });
            }

            // Экипировка и характеристики.
            panel(root).with_children(|p| {
                p.spawn(text(&fonts, "Экипировка", 22.0, TITLE, true));
                p.spawn(Node {
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::px(3, SLOT),
                    column_gap: px(6),
                    row_gap: px(6),
                    ..default()
                })
                .with_children(|g| {
                    use EquipSlot::*;
                    for slot in [Necklace, Head, Ring, MainHand, Chest, OffHand, Hands, Legs, Feet] {
                        equip_slot(g, &res, Owner::Me, slot, inv.equipped(slot), SLOT, held_uid);
                    }
                });
                p.spawn(text(&fonts, "Характеристики", 16.0, DIM, false));
                let stats = inv.equipped_stats();
                let order = [
                    Stat::WeaponDamage,
                    Stat::BonusDamage,
                    Stat::Armor,
                    Stat::MaxHealth,
                    Stat::Strength,
                    Stat::Agility,
                    Stat::MoveSpeed,
                    Stat::ActionSpeed,
                    Stat::ParryWindow,
                    Stat::BlockStability,
                    Stat::HeadshotReduction,
                    Stat::Luck,
                ];
                let lines: Vec<String> = order
                    .iter()
                    .filter(|s| stats.get(**s) != 0.0 || matches!(s, Stat::WeaponDamage | Stat::Armor))
                    .map(|s| format!("{}: {}", s.name(), s.format(stats.get(*s))))
                    .collect();
                p.spawn(text(&fonts, lines.join("\n"), 15.0, Color::srgb(0.85, 0.82, 0.76), false));
            });

            // Сумка.
            panel(root).with_children(|p| {
                p.spawn(text(&fonts, "Сумка", 22.0, TITLE, true));
                grid(p, &res, Owner::Me, inv, held_uid);
                let purse = crate::economy::purse(inv);
                let value: u32 = inv.all_items().filter(|i| crate::economy::coin_value(i.base).is_none()).map(Item::value).sum();
                p.spawn(text(
                    &fonts,
                    format!("Кошель: {}\nВещи примерно на {}", crate::economy::format_money(purse), crate::economy::format_money(value)),
                    15.0,
                    Color::srgb(0.9, 0.75, 0.35),
                    false,
                ));
                p.spawn(text(
                    &fonts,
                    "ЛКМ — взять/положить, мимо окна — выбросить\nПКМ или Shift+ЛКМ — быстро переложить или надеть\nF — использовать: лечить, читать, варить в горшке; Q — выбросить, Tab — закрыть\nK — окно знаний",
                    13.0,
                    DIM,
                    false,
                ));
            });

            // Предмет в руке, следует за мышью.
            if let Some(h) = &held.0 {
                let (w, hgt) = h.item.size();
                root.spawn((
                    HeldGhost,
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(w as f32 * CELL - 2.0),
                        height: px(hgt as f32 * CELL - 2.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                    GlobalZIndex(50),
                ))
                .with_children(|g| item_body(g, &res, &h.item, 0.9));
            }
        });
}

fn panel<'a>(root: &'a mut ChildSpawnerCommands) -> EntityCommands<'a> {
    root.spawn((
        Hit::Panel,
        Interaction::default(),
        FocusPolicy::Pass,
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(12)),
            row_gap: px(8),
            border: UiRect::all(px(2)),
            align_self: AlignSelf::Center,
            ..default()
        },
        BorderColor::all(PANEL_BORDER),
        BackgroundColor(PANEL_BG),
    ))
}

/// Шрифты и иконки для построения окна.
struct UiRes {
    fonts: UiFonts,
    icons: Vec<Handle<Image>>,
}

/// Внешний вид предмета (фон и рамка по качеству, иконка, количество).
fn item_body(p: &mut ChildSpawnerCommands, fonts: &UiRes, item: &Item, alpha: f32) {
    let q = item.quality;
    p.spawn((
        Node {
            width: percent(100),
            height: percent(100),
            border: UiRect::all(px(2)),
            padding: UiRect::all(px(3)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
            ..default()
        },
        BorderColor::all(quality_color(q).with_alpha(alpha)),
        BackgroundColor(item_bg(q).with_alpha(0.95 * alpha)),
        FocusPolicy::Pass,
    ))
    .with_children(|b| {
        // Иконка вписывается в рамку с сохранением пропорций.
        let (w, h) = item.size();
        b.spawn((
            ImageNode::new(fonts.icons[item.base.0 as usize].clone()).with_color(Color::WHITE.with_alpha(alpha)),
            Node { height: percent(100), aspect_ratio: Some(w as f32 / h as f32), max_width: percent(100), ..default() },
            FocusPolicy::Pass,
        ));
        if item.count > 1 {
            b.spawn((
                text(&fonts.fonts, item.count.to_string(), 13.0, Color::WHITE.with_alpha(alpha), true),
                TextShadow::default(),
                Node { position_type: PositionType::Absolute, right: px(3), bottom: px(0), ..default() },
            ));
        }
    });
}

fn grid(p: &mut ChildSpawnerCommands, fonts: &UiRes, owner: Owner, inv: &Inventory, held: Option<ItemUid>) {
    let g = &inv.grid;
    p.spawn(Node { width: px(g.w as f32 * CELL), height: px(g.h as f32 * CELL), ..default() }).with_children(|n| {
        for y in 0..g.h {
            for x in 0..g.w {
                n.spawn((
                    Hit::Cell { owner, x, y },
                    Interaction::default(),
                    FocusPolicy::Pass,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(x as f32 * CELL),
                        top: px(y as f32 * CELL),
                        width: px(CELL),
                        height: px(CELL),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BorderColor::all(CELL_BORDER),
                    BackgroundColor(CELL_BG),
                ));
            }
        }
        for placed in &g.items {
            let (w, h) = placed.item.size();
            let alpha = if held == Some(placed.item.uid) { 0.3 } else { 1.0 };
            n.spawn((
                Hit::Item { owner, uid: placed.item.uid },
                Interaction::default(),
                FocusPolicy::Pass,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(placed.x as f32 * CELL + 1.0),
                    top: px(placed.y as f32 * CELL + 1.0),
                    width: px(w as f32 * CELL - 2.0),
                    height: px(h as f32 * CELL - 2.0),
                    ..default()
                },
            ))
            .with_children(|b| item_body(b, fonts, &placed.item, alpha));
        }
    });
}

fn equip_slot(p: &mut ChildSpawnerCommands, fonts: &UiRes, owner: Owner, slot: EquipSlot, item: Option<&Item>, size: f32, held: Option<ItemUid>) {
    p.spawn((
        Hit::Equip { owner, slot },
        Interaction::default(),
        FocusPolicy::Pass,
        Node {
            width: px(size),
            height: px(size),
            border: UiRect::all(px(1)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(CELL_BORDER),
        BackgroundColor(CELL_BG),
    ))
    .with_children(|s| match item {
        Some(item) => {
            let alpha = if held == Some(item.uid) { 0.3 } else { 1.0 };
            s.spawn((Hit::Item { owner, uid: item.uid }, Interaction::default(), FocusPolicy::Pass, Node { width: percent(100), height: percent(100), ..default() }))
                .with_children(|b| item_body(b, fonts, item, alpha));
        }
        None => {
            let size = if size < SLOT { 10.0 } else { 12.0 };
            s.spawn((text(&fonts.fonts, slot.name(), size, Color::srgba(0.6, 0.56, 0.5, 0.6), false), TextLayout::justify(Justify::Center)));
        }
    });
}

// ---------------------------------------------------------------------------
// Мышь и клавиши
// ---------------------------------------------------------------------------

/// Что под мышью, с приоритетом: при переноске важнее клетки и слоты, иначе — предметы.
fn hovered(hits: &Query<(&Interaction, &Hit)>, holding: bool) -> Option<Hit> {
    let rank = |h: &Hit| match (h, holding) {
        (Hit::Equip { .. }, true) => 0,
        (Hit::Cell { .. }, true) => 1,
        (Hit::Item { .. }, false) => 0,
        (Hit::TakeAll, _) => 2,
        (Hit::Item { .. }, true) => 3,
        (Hit::Equip { .. } | Hit::Cell { .. }, false) => 4,
        (Hit::Panel, _) => 5,
        (Hit::Outside, _) => 6,
    };
    hits.iter().filter(|(i, _)| **i != Interaction::None).map(|(_, h)| *h).min_by_key(rank)
}

/// Куда быстро переложить предмет (ПКМ).
fn quick_move(state: &LootState, owner: Owner, uid: ItemUid) -> Option<(Owner, Target)> {
    let (loc, item) = owner_inv(state, owner)?.find(uid)?;
    Some(match (owner, loc) {
        (Owner::Container(_), _) => (Owner::Me, Target::EquipOrAuto),
        (Owner::Me, Loc::Equip(_)) => (Owner::Me, Target::Auto),
        (Owner::Me, Loc::Grid { .. }) => match &state.container {
            Some(c) => (Owner::Container(c.id), Target::Auto),
            None => (Owner::Me, Target::Equip(item.info().slot?)),
        },
    })
}

fn handle_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    hits: Query<(&Interaction, &Hit)>,
    mut state: ResMut<LootState>,
    mut held: ResMut<Held>,
    mut client: Option<ResMut<RenetClient>>,
    time: Res<Time>,
    body: Option<Res<crate::medicine_client::MyBody>>,
    mut brew: Option<ResMut<crate::brewing_client::BrewState>>,
) {
    if !state.ui_open {
        return;
    }
    let Some(client) = client.as_mut() else { return };
    let hit = hovered(&hits, held.0.is_some());
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let mv = |client: &mut RenetClient, uid, from, to, target| send(client, &LootClientMessage::Move { uid, from, to, target });

    // Отмена переноски.
    if held.0.is_some() && mouse.just_pressed(MouseButton::Right) {
        held.0 = None;
        return;
    }

    if let Some(h) = held.0.clone() {
        if !mouse.just_pressed(MouseButton::Left) {
            return;
        }
        let uid = h.item.uid;
        match hit {
            Some(Hit::Cell { owner, x, y }) => {
                let (w, hgt) = h.item.size();
                let grid = owner_inv(&state, owner).map(|i| (i.grid.w, i.grid.h)).unwrap_or((0, 0));
                // Верхний левый угол с учётом того, за какую клетку держим; не вылезаем за край.
                let tx = x.saturating_sub(h.grab.0).min(grid.0.saturating_sub(w));
                let ty = y.saturating_sub(h.grab.1).min(grid.1.saturating_sub(hgt));
                mv(client, uid, h.owner, owner, Target::Grid { x: tx, y: ty });
                held.0 = None;
            }
            Some(Hit::Equip { owner, slot }) => {
                if h.item.info().slot == Some(slot) {
                    mv(client, uid, h.owner, owner, Target::Equip(slot));
                    held.0 = None;
                } else {
                    state.notify("Сюда это не надеть", time.elapsed_secs());
                }
            }
            Some(Hit::Outside) => {
                send(client, &LootClientMessage::Drop { uid, from: h.owner });
                held.0 = None;
            }
            _ => {}
        }
        return;
    }

    let Some(Hit::Item { owner, uid }) = hit else {
        if mouse.just_pressed(MouseButton::Left)
            && hit == Some(Hit::TakeAll)
            && let Some(c) = &state.container
        {
            for item in c.inv.all_items() {
                mv(client, item.uid, Owner::Container(c.id), Owner::Me, Target::EquipOrAuto);
            }
        }
        return;
    };

    if keys.just_pressed(KeyCode::KeyQ) {
        send(client, &LootClientMessage::Drop { uid, from: owner });
        return;
    }
    // F — использовать (бинт, зелье). Также ПКМ по расходнику в своей сумке, если тело не открыто.
    let mine = if owner == Owner::Me { owner_inv(&state, owner).and_then(|i| i.find(uid)).map(|(_, it)| it.clone()) } else { None };
    let lore_usable = mine.as_ref().is_some_and(|it| crate::lore_client::usable(it.base));
    let consumable = mine.as_ref().is_some_and(|it| it.base.consumable().is_some());
    let use_pressed = keys.just_pressed(KeyCode::KeyF) || (state.container.is_none() && mouse.just_pressed(MouseButton::Right));
    // Горшок, котелок, ступка — окно варки.
    if lore_usable
        && use_pressed
        && let (Some(item), Some(brew)) = (mine.as_ref(), brew.as_mut())
        && let Some(vessel) = crate::brewing::Vessel::from_item(item.info().name)
    {
        brew.open_vessel(item.uid, vessel);
        return;
    }
    if lore_usable
        && use_pressed
        && crate::lore_client::try_use(client, mine.as_ref().unwrap(), body.as_deref().map(|b| &b.0))
    {
        // Книги — читать, лекарства — на рану или внутрь.
        return;
    }
    if keys.just_pressed(KeyCode::KeyF) || (consumable && state.container.is_none() && mouse.just_pressed(MouseButton::Right)) {
        if consumable {
            send(client, &LootClientMessage::Use { uid });
        } else {
            state.notify("Это нельзя использовать", time.elapsed_secs());
        }
        return;
    }
    let quick = mouse.just_pressed(MouseButton::Right) || (shift && mouse.just_pressed(MouseButton::Left));
    if quick {
        match quick_move(&state, owner, uid) {
            Some((to, target)) => mv(client, uid, owner, to, target),
            None => state.notify("Это нельзя надеть", time.elapsed_secs()),
        }
        return;
    }
    if mouse.just_pressed(MouseButton::Left) {
        let Some((loc, item)) = owner_inv(&state, owner).and_then(|i| i.find(uid)) else { return };
        let (w, h) = item.size();
        // За какую клетку взяли: ищем клетку под мышью в той же сетке.
        let grab = match loc {
            Loc::Grid { x, y } => hits
                .iter()
                .find_map(|(i, hit)| match hit {
                    Hit::Cell { owner: o, x: cx, y: cy } if *i != Interaction::None && *o == owner => Some((cx.saturating_sub(x).min(w - 1), cy.saturating_sub(y).min(h - 1))),
                    _ => None,
                })
                .unwrap_or((0, 0)),
            Loc::Equip(_) => ((w - 1) / 2, (h - 1) / 2),
        };
        held.0 = Some(HeldItem { owner, item: item.clone(), grab });
    }
}

/// Подсветка клеток, куда ляжет предмет: зелёная — влезает, красная — нет.
fn highlight_cells(
    state: Res<LootState>,
    held: Res<Held>,
    hits: Query<(&Interaction, &Hit)>,
    mut cells: Query<(&Hit, &mut BackgroundColor)>,
) {
    if !state.ui_open {
        return;
    }
    let target = held.0.as_ref().and_then(|h| match hovered(&hits, true) {
        Some(Hit::Cell { owner, x, y }) => {
            let inv = owner_inv(&state, owner)?;
            let (w, hgt) = h.item.size();
            let tx = x.saturating_sub(h.grab.0).min(inv.grid.w.saturating_sub(w));
            let ty = y.saturating_sub(h.grab.1).min(inv.grid.h.saturating_sub(hgt));
            // Проверяем без самого переносимого предмета и с учётом обмена/стопки.
            let mut copy = inv.clone();
            copy.take(h.item.uid);
            let ok = copy.place(h.item.clone(), Target::Grid { x: tx, y: ty }).is_ok();
            Some((owner, tx, ty, w, hgt, ok))
        }
        _ => None,
    });
    for (hit, mut bg) in &mut cells {
        let Hit::Cell { owner, x, y } = *hit else { continue };
        let color = match target {
            Some((o, tx, ty, w, h, ok)) if o == owner && x >= tx && x < tx + w && y >= ty && y < ty + h => {
                if ok {
                    Color::srgba(0.2, 0.6, 0.2, 0.45)
                } else {
                    Color::srgba(0.7, 0.15, 0.1, 0.45)
                }
            }
            _ => CELL_BG,
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

// ---------------------------------------------------------------------------
// Подсказка и предмет в руке
// ---------------------------------------------------------------------------

fn setup_tooltip(mut commands: Commands) {
    commands.spawn((
        Tooltip,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(10)),
            row_gap: px(3),
            border: UiRect::all(px(2)),
            width: px(300),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.97)),
        BorderColor::all(PANEL_BORDER),
        GlobalZIndex(100),
        FocusPolicy::Pass,
    ));
}

#[allow(clippy::too_many_arguments)]
fn update_tooltip(
    mut commands: Commands,
    state: Res<LootState>,
    held: Res<Held>,
    fonts: Res<UiFonts>,
    icons: Res<crate::icons::ItemIcons>,
    knowledge: Option<Res<crate::lore_client::MyKnowledge>>,
    hits: Query<(&Interaction, &Hit)>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut tooltip: Single<(Entity, &mut Node, &mut BorderColor), With<Tooltip>>,
    mut tooltip_for: ResMut<TooltipFor>,
) {
    let (entity, ref mut node, ref mut border) = *tooltip;
    let hovered_item = if state.ui_open && held.0.is_none() {
        match hovered(&hits, false) {
            Some(Hit::Item { owner, uid }) => owner_inv(&state, owner).and_then(|i| i.find(uid)).map(|(_, item)| (owner, item.clone())),
            _ => None,
        }
    } else {
        None
    };
    let Some((owner, item)) = hovered_item else {
        node.display = Display::None;
        tooltip_for.0 = None;
        return;
    };
    if let Some(pos) = window.cursor_position() {
        // Не даём подсказке уйти за правый и нижний край.
        let x = if pos.x + 340.0 > window.width() { pos.x - 330.0 } else { pos.x + 18.0 };
        let y = (pos.y + 18.0).min(window.height() - 260.0).max(0.0);
        node.left = px(x);
        node.top = px(y);
    }
    node.display = Display::Flex;
    // Знания меняют имя и описание (опознали траву, выучили грамоту).
    if tooltip_for.0 == Some((owner, item.uid)) && !knowledge.as_ref().is_some_and(|k| k.is_changed()) {
        return;
    }
    tooltip_for.0 = Some((owner, item.uid));
    border.set_all(quality_color(item.quality));
    let info = item.info();
    commands.entity(entity).despawn_children().with_children(|t| {
        let (w, h) = item.size();
        let scale = 40.0 / (w.max(h) as f32 / 2.0).max(1.0);
        t.spawn((ImageNode::new(icons.get(item.base)), Node { width: px(w as f32 * scale), height: px(h as f32 * scale), align_self: AlignSelf::Center, ..default() }));
        let name = knowledge.as_ref().map(|k| k.item_name(&item)).unwrap_or_else(|| item.display_name());
        t.spawn(text(&fonts, name, 18.0, quality_color(item.quality), true));
        if let Some(k) = knowledge.as_ref() {
            for line in k.0.describe(item.base) {
                t.spawn(text(&fonts, line, 13.0, Color::srgb(0.85, 0.8, 0.68), false));
            }
        }
        if crate::medicine::is_remedy(info.name) && item.base.consumable().is_none() {
            t.spawn(text(&fonts, "F или ПКМ — на рану или внутрь".to_string(), 13.0, Color::srgb(0.75, 0.85, 0.6), false));
        }
        if let Some(c) = item.base.consumable() {
            let how = if c.heal_over > 0.0 { format!("за {:.0} с после применения", c.heal_over) } else { "во время применения".to_string() };
            t.spawn(text(&fonts, format!("Лечит {:.0} здоровья {how}", c.heal), 14.0, Color::srgb(0.5, 0.9, 0.5), false));
            t.spawn(text(&fonts, format!("Применение {:.0} с · F или ПКМ", c.use_time), 13.0, Color::srgb(0.5, 0.9, 0.5), false));
        }
        let mut kind = String::new();
        if info.has_quality {
            kind.push_str(item.quality.name());
        }
        if let Some(slot) = info.slot {
            if !kind.is_empty() {
                kind.push_str(" · ");
            }
            kind.push_str(slot.name());
        }
        if !kind.is_empty() {
            t.spawn(text(&fonts, kind, 13.0, DIM, false));
        }
        if let Some((s, v)) = item.main_stat() {
            t.spawn(text(&fonts, format!("{}: {}", s.name(), s.format(v)), 15.0, Color::WHITE, true));
        }
        for (s, v) in &item.affixes {
            t.spawn(text(&fonts, format!("{} {}", s.format(*v), s.name()), 14.0, Color::srgb(0.55, 0.75, 1.0), false));
        }
        t.spawn(text(&fonts, format!("Размер {}×{}   Цена ≈ {}", info.size.0, info.size.1, crate::economy::format_money(item.value())), 12.0, DIM, false));
        // Сравнение с надетым.
        if let (Some(slot), Some(me)) = (info.slot, state.inventory.as_ref())
            && let Some(worn) = me.equipped(slot).filter(|w| w.uid != item.uid)
            && let (Some((s, a)), Some((_, b))) = (item.main_stat(), worn.main_stat())
        {
            let d = a - b;
            let color = if d >= 0.0 { Color::srgb(0.4, 0.85, 0.4) } else { Color::srgb(0.9, 0.4, 0.35) };
            t.spawn(text(&fonts, format!("{} {:+.0} к надетому", s.name(), d), 13.0, color, false));
        }
    });
}

fn move_ghost(held: Res<Held>, window: Single<&Window, With<PrimaryWindow>>, mut ghost: Query<&mut Node, With<HeldGhost>>) {
    let Some(h) = &held.0 else { return };
    let Some(pos) = window.cursor_position() else { return };
    for mut node in &mut ghost {
        node.left = px(pos.x - (h.grab.0 as f32 + 0.5) * CELL);
        node.top = px(pos.y - (h.grab.1 as f32 + 0.5) * CELL);
    }
}
