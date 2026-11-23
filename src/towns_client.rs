//! Клиент городов и домов: подсказка у входа в усадьбу («U»), окно усадьбы (купить, снять,
//! обставить, пустить друзей), окно «Вести» («M»: поселения, цены, стройки, дома на продажу)
//! и глашатай с важными вестями. Стройки и обстановка включаются по сообщениям сервера.

#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::economy::{format_money, purse};
use crate::estates::*;
use crate::inventory_ui::UiFonts;
use crate::level::{FurnishKind, SwitchKey, estates};
use crate::loot_client::LootState;
use crate::protocol::{decode, encode};
use crate::towns::{GOODS, SITE_DONE, TownId, mood_word, price_word};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;

const PANEL_BG: Color = Color::srgba(0.11, 0.09, 0.07, 0.97);
const PANEL_BORDER: Color = Color::srgb(0.45, 0.36, 0.22);
const TITLE: Color = Color::srgb(0.93, 0.83, 0.6);
const TEXT: Color = Color::srgb(0.86, 0.82, 0.75);
const DIM: Color = Color::srgb(0.6, 0.56, 0.5);
const GOOD: Color = Color::srgb(0.5, 0.85, 0.45);
const BAD: Color = Color::srgb(0.95, 0.4, 0.32);
const GOLD: Color = Color::srgb(0.92, 0.76, 0.36);
const BTN: Color = Color::srgba(0.3, 0.22, 0.12, 0.95);
const BTN_HOVER: Color = Color::srgba(0.45, 0.33, 0.17, 1.0);
const BTN_OFF: Color = Color::srgba(0.18, 0.16, 0.14, 0.9);
const BTN_ON: Color = Color::srgba(0.25, 0.36, 0.18, 0.95);

/// С какого расстояния у входа видна подсказка «U».
const PROMPT_RANGE: f32 = 3.5;

pub struct TownClientPlugin;

impl Plugin for TownClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TownUi::default())
            .add_systems(Startup, setup_hud.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (receive_town, aim_estate, input).chain().run_if(bevy_renet::client_connected))
            .add_systems(Update, (crate::level::sync_switch_visuals, hide_unbuilt_doors, rebuild, click, hover, update_hud).chain());
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Window {
    #[default]
    None,
    Estate,
    News,
}

#[derive(Resource, Default)]
pub struct TownUi {
    pub you: u64,
    pub week: u32,
    pub date: String,
    pub sites: Vec<u8>,
    pub estates: Vec<Option<EstateBrief>>,
    pub detail: Option<EstateDetail>,
    pub news: Option<NewsView>,
    pub window: Window,
    /// У какой усадьбы стоит игрок.
    pub near: Option<u16>,
    /// Глашатай: (текст, когда).
    pub crier: Vec<(String, f32)>,
    rev: u32,
    built: u32,
    /// Вкладка окна «Вести»: 0 — поселения, 1 — вести, 2 — дома.
    tab: u8,
}

impl TownUi {
    pub fn open(&self) -> bool {
        self.window != Window::None
    }
}

fn send(client: &mut RenetClient, msg: &TownClientMessage) {
    client.send_message(CH_TOWN, encode(msg));
}

fn receive_town(mut client: ResMut<RenetClient>, mut ui: ResMut<TownUi>, level: Res<crate::collision::LevelColliders>, mut loot: ResMut<LootState>, time: Res<Time>) {
    let now = time.elapsed_secs();
    let mut switches = false;
    while let Some(bytes) = client.receive_message(CH_TOWN) {
        let Some(msg) = decode::<TownServerMessage>(&bytes) else { continue };
        match msg {
            TownServerMessage::Init { week, date, sites, estates: list, you } => {
                ui.you = you;
                ui.week = week;
                ui.date = date;
                ui.sites = sites;
                let n = crate::level::estates().estates.len();
                ui.estates = vec![None; n];
                for b in list {
                    let i = b.id as usize;
                    if i < n {
                        ui.estates[i] = Some(b);
                    }
                }
                switches = true;
            }
            TownServerMessage::Week { week, date } => {
                ui.week = week;
                ui.date = date;
                if ui.window == Window::News {
                    send(&mut client, &TownClientMessage::News);
                }
            }
            TownServerMessage::Site { site, stage } => {
                if let Some(s) = ui.sites.get_mut(site as usize) {
                    *s = stage;
                }
                switches = true;
            }
            TownServerMessage::Estate(b) => {
                let i = b.id as usize;
                if i < ui.estates.len() {
                    ui.estates[i] = Some(b);
                    switches = true;
                }
            }
            TownServerMessage::Detail(d) => {
                ui.detail = Some(d);
                ui.window = Window::Estate;
                ui.rev += 1;
            }
            TownServerMessage::News(v) => {
                ui.news = Some(v);
                if ui.window != Window::Estate {
                    ui.window = Window::News;
                }
                ui.rev += 1;
            }
            TownServerMessage::Crier(text) => {
                ui.crier.push((text, now));
                if ui.crier.len() > 3 {
                    ui.crier.remove(0);
                }
            }
            TownServerMessage::Notice(text) => loot.notify(text, now),
        }
    }
    if switches {
        apply_switches(&ui, &level);
    }
}

/// Стадии строек и обстановка — в коллизиях клиента (меши видны по ним же).
fn apply_switches(ui: &TownUi, level: &crate::collision::LevelColliders) {
    let cat = estates();
    for (g, key) in cat.keys.iter().enumerate() {
        let on = match *key {
            SwitchKey::Site { site, stage } => ui.sites.get(site as usize).copied().unwrap_or(0) == stage,
            SwitchKey::Furnish { estate, room, kind } => ui.estates.get(estate as usize).and_then(|b| b.as_ref()).and_then(|b| b.furnish.get(room as usize)).copied().flatten() == Some(kind),
        };
        level.set_switch(g, on);
    }
}

/// Створки дверей недостроенных домов не рисуем.
fn hide_unbuilt_doors(ui: Res<TownUi>, mut leaves: Query<(&crate::doors::DoorLeaf, &mut Visibility)>, mut done: Local<Vec<u8>>) {
    if *done == ui.sites {
        return;
    }
    *done = ui.sites.clone();
    let cat = estates();
    for (leaf, mut vis) in &mut leaves {
        for (s, doors) in cat.site_doors.iter().enumerate() {
            if doors.contains(&leaf.id) {
                let built = ui.sites.get(s).copied().unwrap_or(0) >= SITE_DONE;
                *vis = if built { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

fn aim_estate(mut ui: ResMut<TownUi>, player: Query<&Transform, With<crate::player::LocalPlayer>>) {
    let Ok(t) = player.single() else { return };
    let p = t.translation;
    let cat = estates();
    let near = cat
        .estates
        .iter()
        .enumerate()
        .filter(|(_, e)| p.distance(e.entrance) < PROMPT_RANGE || (e.contains(p) && p.distance(e.entrance) < 14.0))
        .min_by(|a, b| a.1.entrance.distance_squared(p).total_cmp(&b.1.entrance.distance_squared(p)))
        .map(|(i, _)| i as u16);
    if ui.near != near {
        ui.near = near;
    }
}

#[allow(clippy::too_many_arguments)]
fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<TownUi>,
    mut client: ResMut<RenetClient>,
    loot: Res<LootState>,
    trade: Res<crate::trade_client::TradeState>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if ui.open() {
        let close = keys.just_pressed(KeyCode::Escape) || (ui.window == Window::Estate && keys.just_pressed(KeyCode::KeyU)) || (ui.window == Window::News && keys.just_pressed(KeyCode::KeyM));
        if close || loot.ui_open || trade.window_open() {
            ui.window = Window::None;
            ui.rev += 1;
            if !keys.just_pressed(KeyCode::Escape) && !loot.ui_open {
                cursor.grab_mode = CursorGrabMode::Locked;
                cursor.visible = false;
            }
        }
        return;
    }
    if loot.ui_open || trade.window_open() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyU)
        && let Some(id) = ui.near
    {
        send(&mut client, &TownClientMessage::Open { estate: id });
        free_cursor(&mut cursor);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        send(&mut client, &TownClientMessage::News);
        ui.window = Window::News;
        ui.rev += 1;
        free_cursor(&mut cursor);
    }
}

fn free_cursor(cursor: &mut CursorOptions) {
    cursor.grab_mode = CursorGrabMode::None;
    cursor.visible = true;
}

// ---------------------------------------------------------------------------
// Подсказка и глашатай
// ---------------------------------------------------------------------------

#[derive(Component)]
struct EstatePrompt;

#[derive(Component)]
struct CrierText;

fn setup_hud(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn((Node { position_type: PositionType::Absolute, width: percent(100), top: percent(61), justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE))
        .with_children(|p| {
            p.spawn((EstatePrompt, Text::new(""), TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(18.0), ..default() }, TextColor(GOLD), TextShadow::default(), TextLayout::justify(Justify::Center)));
        });
    commands
        .spawn((Node { position_type: PositionType::Absolute, width: percent(100), top: px(54), justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE))
        .with_children(|p| {
            p.spawn((
                CrierText,
                Text::new(""),
                TextFont { font: fonts.regular.clone().into(), font_size: bevy::text::FontSize::Px(17.0), ..default() },
                TextColor(Color::srgb(0.98, 0.92, 0.78)),
                TextShadow::default(),
                TextLayout::justify(Justify::Center),
                Node { max_width: px(900), ..default() },
            ));
        });
}

fn status_line(b: &EstateBrief, you: u64, week: u32) -> String {
    let mine = b.access.contains(&you);
    match b.status {
        Status::Building => "строится".into(),
        Status::ForSale => format!("продаётся за {} · сдаётся за {} в месяц", format_money(b.price), format_money(b.rent)),
        Status::Owned if mine => "ваш дом".into(),
        Status::Owned => format!("хозяин — {}", b.owner),
        Status::Rented { until } if mine => format!("вы снимаете ещё {} нед.", until.saturating_sub(week)),
        Status::Rented { .. } => format!("снимает {}", b.owner),
    }
}

fn update_hud(ui: Res<TownUi>, time: Res<Time>, mut prompt: Single<&mut Text, (With<EstatePrompt>, Without<CrierText>)>, mut crier: Single<&mut Text, (With<CrierText>, Without<EstatePrompt>)>) {
    let line = match (ui.near, ui.open()) {
        (Some(id), false) => {
            let spec = &estates().estates[id as usize];
            match ui.estates.get(id as usize).and_then(|b| b.as_ref()) {
                Some(b) => format!("[U] {} — {}", spec.name, status_line(b, ui.you, ui.week)),
                None => format!("[U] {}", spec.name),
            }
        }
        _ => String::new(),
    };
    if prompt.0 != line {
        prompt.0 = line;
    }
    let now = time.elapsed_secs();
    let lines: Vec<&str> = ui.crier.iter().filter(|(_, t)| now - *t < 12.0).map(|(s, _)| s.as_str()).collect();
    let text = if lines.is_empty() { String::new() } else { format!("Глашатай на агоре: «{}»", lines.join("» · «")) };
    if crier.0 != text {
        crier.0 = text;
    }
}

// ---------------------------------------------------------------------------
// Окна
// ---------------------------------------------------------------------------

#[derive(Component)]
struct TownRoot;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Act {
    Close,
    Buy(u16),
    Rent(u16, u8),
    Sell(u16),
    Furnish(u16, u8, Option<FurnishKind>),
    Guest(u16, u64, bool),
    Tab(u8),
    /// Открыть окно дома из списка (только посмотреть).
    Show(u16),
}

#[derive(Component)]
struct Enabled(bool);

fn txt(p: &mut ChildSpawnerCommands, fonts: &UiFonts, s: impl Into<String>, size: f32, color: Color, bold: bool) {
    p.spawn((Text::new(s), TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: bevy::text::FontSize::Px(size), ..default() }, TextColor(color)));
}

fn button(p: &mut ChildSpawnerCommands, fonts: &UiFonts, label: impl Into<String>, act: Act, enabled: bool, on: bool) {
    let bg = if on {
        BTN_ON
    } else if enabled {
        BTN
    } else {
        BTN_OFF
    };
    p.spawn((
        Interaction::default(),
        act,
        Enabled(enabled),
        Node { padding: UiRect::axes(px(10), px(4)), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BorderColor::all(PANEL_BORDER),
        BackgroundColor(bg),
    ))
    .with_children(|b| txt(b, fonts, label, 14.0, if enabled { TEXT } else { DIM }, false));
}

fn panel(width: f32) -> impl Bundle {
    (
        Node { width: px(width), max_height: percent(88), flex_direction: FlexDirection::Column, padding: UiRect::all(px(16)), row_gap: px(6), border: UiRect::all(px(2)), overflow: Overflow::scroll_y(), ..default() },
        BorderColor::all(PANEL_BORDER),
        BackgroundColor(PANEL_BG),
        FocusPolicy::Block,
    )
}

fn row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(4), align_items: AlignItems::Center, ..default() }
}

fn rebuild(mut commands: Commands, mut ui: ResMut<TownUi>, fonts: Res<UiFonts>, loot: Res<LootState>, roots: Query<Entity, With<TownRoot>>) {
    if ui.built == ui.rev {
        return;
    }
    ui.built = ui.rev;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if !ui.open() {
        return;
    }
    let money = loot.inventory.as_ref().map(purse).unwrap_or(0);
    commands
        .spawn((
            TownRoot,
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            GlobalZIndex(13),
        ))
        .with_children(|root| match ui.window {
            Window::Estate => {
                if let Some(d) = &ui.detail {
                    estate_window(root, &fonts, d, money, ui.week);
                }
            }
            Window::News => news_window(root, &fonts, ui.news.as_ref(), ui.tab, ui.you),
            Window::None => {}
        });
}

fn estate_window(root: &mut ChildSpawnerCommands, fonts: &UiFonts, d: &EstateDetail, money: u32, week: u32) {
    let id = d.brief.id;
    root.spawn(panel(640.0)).with_children(|p| {
        txt(p, fonts, &d.name, 22.0, TITLE, true);
        let place = match d.town {
            TownId::Herson => "Херсон",
            _ => d.town.def().name,
        };
        let rooms = d.rooms.len();
        txt(p, fonts, format!("{place} · {rooms} {} · {} {} · {}", crate::economy::plural(rooms as u32, "комната", "комнаты", "комнат"), d.chests, crate::economy::plural(d.chests, "сундук", "сундука", "сундуков"), d.date), 14.0, DIM, false);
        let status = match d.brief.status {
            Status::Building => ("Дом ещё строится: курия продаст его, когда положат черепицу.".to_string(), DIM),
            Status::ForSale => (format!("Продаётся. Цена — {}; снять — {} в месяц.", format_money(d.brief.price), format_money(d.brief.rent)), GOLD),
            Status::Owned if d.mine => ("Это ваш дом: двери и сундуки открываются только вам и тем, кого вы пустите.".into(), GOOD),
            Status::Owned => (format!("Хозяин — {}.", d.brief.owner), TEXT),
            Status::Rented { .. } if d.mine => (format!("Вы снимаете этот дом: осталось {} нед. (до {}).", d.weeks_left.unwrap_or(0), crate::towns::date_text(week + d.weeks_left.unwrap_or(0))), GOOD),
            Status::Rented { .. } => (format!("Дом снимает {}.", d.brief.owner), TEXT),
        };
        txt(p, fonts, status.0, 16.0, status.1, false);
        txt(p, fonts, format!("В кошельке: {}", format_money(money)), 14.0, DIM, false);
        p.spawn(row()).with_children(|r| match d.brief.status {
            Status::ForSale => {
                button(r, fonts, format!("Купить — {}", format_money(d.brief.price)), Act::Buy(id), money >= d.brief.price, false);
                for m in [1u8, 3, 6] {
                    let cost = d.brief.rent * m as u32;
                    button(r, fonts, format!("Снять на {m} мес. — {}", format_money(cost)), Act::Rent(id, m), money >= cost, false);
                }
            }
            Status::Rented { .. } if d.mine => {
                for m in [1u8, 3] {
                    let cost = d.brief.rent * m as u32;
                    button(r, fonts, format!("Продлить на {m} мес. — {}", format_money(cost)), Act::Rent(id, m), money >= cost, false);
                }
                button(r, fonts, "Съехать", Act::Sell(id), true, false);
            }
            Status::Owned if d.mine => {
                button(r, fonts, format!("Продать курии за {}", format_money(d.buyback)), Act::Sell(id), true, false);
            }
            _ => {}
        });
        // Комнаты.
        p.spawn(Node { height: px(6), ..default() });
        txt(p, fonts, "Комнаты", 18.0, TITLE, true);
        if !d.mine {
            txt(p, fonts, "Обставить дом можно, когда он ваш: мастера сделают на заказ за неделю-две.", 13.0, DIM, false);
        }
        for (ri, room) in d.rooms.iter().enumerate() {
            p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::all(px(6)), border: UiRect::all(px(1)), ..default() }).insert(BorderColor::all(Color::srgba(0.45, 0.36, 0.22, 0.4))).with_children(|c| {
                let what = match (room.now, room.order) {
                    (_, Some((k, weeks))) => (format!("заказано: {} — будет через {weeks} нед.", k.name().to_lowercase()), GOLD),
                    (Some(k), None) => (k.name().to_string(), GOOD),
                    (None, None) => ("пустая".to_string(), DIM),
                };
                let floor = if room.upper { " (наверху)" } else { "" };
                c.spawn(row()).with_children(|r| {
                    txt(r, fonts, format!("{}{}:", ri + 1, floor), 15.0, TEXT, true);
                    txt(r, fonts, what.0, 15.0, what.1, false);
                });
                if d.mine {
                    c.spawn(row()).with_children(|r| {
                        for &k in &room.kinds {
                            let on = room.now == Some(k);
                            let label = format!("{} · {}", k.name(), crate::economy::format_money_short(k.price()));
                            button(r, fonts, label, Act::Furnish(id, ri as u8, Some(k)), !on && money >= k.price(), on);
                        }
                        if room.now.is_some() {
                            button(r, fonts, "Вынести всё", Act::Furnish(id, ri as u8, None), true, false);
                        }
                    });
                }
            });
        }
        // Гости.
        if d.mine {
            p.spawn(Node { height: px(6), ..default() });
            txt(p, fonts, "Ключи", 18.0, TITLE, true);
            if d.players.is_empty() && d.guests.is_empty() {
                txt(p, fonts, "Друзей в игре нет. Когда зайдут — здесь можно дать им ключ.", 13.0, DIM, false);
            }
            p.spawn(row()).with_children(|r| {
                for (pid, name, has) in &d.players {
                    button(r, fonts, if *has { format!("{name}: забрать ключ") } else { format!("{name}: дать ключ") }, Act::Guest(id, *pid, !has), true, *has);
                }
            });
            if !d.guests.is_empty() {
                txt(p, fonts, format!("Ключи есть у: {}", d.guests.join(", ")), 13.0, DIM, false);
            }
        }
        p.spawn(Node { height: px(4), ..default() });
        p.spawn(row()).with_children(|r| {
            button(r, fonts, "Закрыть (Esc)", Act::Close, true, false);
            txt(r, fonts, "Вести и дома на продажу — клавиша M", 13.0, DIM, false);
        });
    });
}

fn news_window(root: &mut ChildSpawnerCommands, fonts: &UiFonts, v: Option<&NewsView>, tab: u8, you: u64) {
    let _ = you;
    root.spawn(panel(820.0)).with_children(|p| {
        let Some(v) = v else {
            txt(p, fonts, "Вести собираются…", 18.0, DIM, false);
            return;
        };
        txt(p, fonts, "Вести Таврики", 24.0, TITLE, true);
        let sea = if v.sea_open { "море открыто — корабли ходят" } else { "море закрыто до весны — кораблей нет" };
        txt(p, fonts, format!("{} · {sea}", v.date), 14.0, DIM, false);
        p.spawn(row()).with_children(|r| {
            button(r, fonts, "Поселения", Act::Tab(0), true, tab == 0);
            button(r, fonts, "Что слышно", Act::Tab(1), true, tab == 1);
            button(r, fonts, "Дома и стройки", Act::Tab(2), true, tab == 2);
            button(r, fonts, "Закрыть (Esc)", Act::Close, true, false);
        });
        match tab {
            0 => {
                for t in &v.towns {
                    let d = t.id.def();
                    p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), padding: UiRect::all(px(6)), border: UiRect::all(px(1)), ..default() }).insert(BorderColor::all(Color::srgba(0.45, 0.36, 0.22, 0.4))).with_children(|c| {
                        c.spawn(row()).with_children(|r| {
                            txt(r, fonts, d.name, 17.0, TITLE, true);
                            txt(r, fonts, format!("· {} жителей, {} дворов · народ {}", t.pop, t.homes, mood_word(t.mood)), 14.0, TEXT, false);
                            if !t.troubles.is_empty() {
                                txt(r, fonts, format!("· {}", t.troubles.join(", ")), 14.0, BAD, false);
                            }
                        });
                        txt(c, fonts, d.about, 13.0, DIM, false);
                        let dear: Vec<String> = GOODS.iter().filter(|g| (t.prices[**g as usize] - 1.0).abs() > 0.12).map(|g| format!("{} {}", g.name().to_lowercase(), price_word(t.prices[*g as usize]))).collect();
                        if !dear.is_empty() {
                            txt(c, fonts, dear.join("; "), 13.0, DIM, false);
                        }
                        if !t.projects.is_empty() {
                            let pr: Vec<String> = t.projects.iter().map(|(k, f)| format!("{} — {}%", k.name(), (f * 100.0) as u32)).collect();
                            txt(c, fonts, format!("Строят: {}", pr.join(", ")), 13.0, GOLD, false);
                        }
                    });
                }
                txt(p, fonts, "Цены в лавках Херсона", 17.0, TITLE, true);
                p.spawn(row()).with_children(|r| {
                    for (name, m) in &v.market {
                        let color = if *m > 1.1 { BAD } else if *m < 0.92 { GOOD } else { TEXT };
                        txt(r, fonts, format!("{name}: {};", price_word(*m)), 13.0, color, false);
                    }
                });
            }
            1 => {
                if v.news.is_empty() {
                    txt(p, fonts, "Пока тихо.", 15.0, DIM, false);
                }
                for (date, town, text) in &v.news {
                    p.spawn(row()).with_children(|r| {
                        txt(r, fonts, format!("{} · {}:", date, town.def().name), 13.0, DIM, false);
                        txt(r, fonts, text, 15.0, TEXT, false);
                    });
                }
            }
            _ => {
                let stages = ["пепелище", "фундамент", "стены", "кровля", "готово"];
                let s: Vec<String> = v.sites.iter().enumerate().map(|(i, st)| format!("площадка {}: {}", i + 1, stages[(*st as usize).min(4)])).collect();
                txt(p, fonts, format!("Стройки Херсона: {}", s.join("; ")), 14.0, GOLD, false);
                for (id, name, price, rent, status, owner) in &v.estates {
                    p.spawn(row()).with_children(|r| {
                        let st = match status {
                            Status::Building => "строится".to_string(),
                            Status::ForSale => format!("продаётся за {} · аренда {}/мес.", format_money(*price), format_money(*rent)),
                            Status::Owned => format!("хозяин — {owner}"),
                            Status::Rented { .. } => format!("снимает {owner}"),
                        };
                        txt(r, fonts, name, 15.0, TEXT, true);
                        txt(r, fonts, st, 14.0, if *status == Status::ForSale { GOLD } else { DIM }, false);
                        button(r, fonts, "Подробнее", Act::Show(*id), true, false);
                    });
                }
                txt(p, fonts, "Купить или снять — у дверей дома (клавиша U).", 13.0, DIM, false);
            }
        }
    });
}

fn click(buttons: Query<(&Interaction, &Act, &Enabled), Changed<Interaction>>, mut ui: ResMut<TownUi>, mut client: Option<ResMut<RenetClient>>, mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    let Some(client) = client.as_mut() else { return };
    for (interaction, act, enabled) in &buttons {
        if *interaction != Interaction::Pressed || !enabled.0 {
            continue;
        }
        match *act {
            Act::Close => {
                ui.window = Window::None;
                ui.rev += 1;
                cursor.grab_mode = CursorGrabMode::Locked;
                cursor.visible = false;
            }
            Act::Buy(estate) => send(client, &TownClientMessage::Buy { estate }),
            Act::Rent(estate, months) => send(client, &TownClientMessage::Rent { estate, months }),
            Act::Sell(estate) => send(client, &TownClientMessage::Sell { estate }),
            Act::Furnish(estate, room, kind) => send(client, &TownClientMessage::Furnish { estate, room, kind }),
            Act::Guest(estate, player, allow) => send(client, &TownClientMessage::Guest { estate, player, allow }),
            Act::Tab(t) => {
                ui.tab = t;
                ui.rev += 1;
            }
            Act::Show(estate) => send(client, &TownClientMessage::Open { estate }),
        }
    }
}

fn hover(mut buttons: Query<(&Interaction, &Enabled, &mut BackgroundColor), (Changed<Interaction>, With<Act>)>) {
    for (interaction, enabled, mut bg) in &mut buttons {
        if !enabled.0 || bg.0 == BTN_ON {
            continue;
        }
        bg.0 = match interaction {
            Interaction::Hovered | Interaction::Pressed => BTN_HOVER,
            Interaction::None => BTN,
        };
    }
}
