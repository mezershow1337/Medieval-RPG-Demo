//! Клиент городов и домов: подсказка у входа в усадьбу («U»), окно усадьбы (купить, снять,
//! обставить, пустить друзей), окно «Вести» («O»: поселения, цены, стройки, дома на продажу)
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

use crate::ui_theme::{AppearChildren, ScrollWheel, Skin, ink, theme};

const TITLE: Color = ink::TITLE;
const TEXT: Color = ink::TEXT;
const DIM: Color = ink::DIM;
const GOOD: Color = ink::GOOD;
const BAD: Color = ink::BAD;
const GOLD: Color = ink::GOLD;

/// С какого расстояния у входа видна подсказка «U».
const PROMPT_RANGE: f32 = 3.5;

pub struct TownClientPlugin;

impl Plugin for TownClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TownUi::default())
            .add_systems(Startup, setup_hud.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (receive_town, aim_estate, input, town_tour).chain().run_if(bevy_renet::client_connected))
            .add_systems(Update, (crate::level::sync_switch_visuals, hide_unbuilt_doors, rebuild, click, update_hud).chain());
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
        let close = keys.just_pressed(KeyCode::Escape) || (ui.window == Window::Estate && keys.just_pressed(KeyCode::KeyU)) || (ui.window == Window::News && keys.just_pressed(KeyCode::KeyO));
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
    if keys.just_pressed(KeyCode::KeyO) {
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
        .spawn((Node { position_type: PositionType::Absolute, width: percent(100), top: percent(67), justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE))
        .with_children(|p| {
            theme().plaque(p).with_children(|p| {
                p.spawn((
                    EstatePrompt,
                    Text::new(""),
                    TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(18.0), ..default() },
                    TextColor(Color::srgb(1.0, 0.8, 0.5)),
                    TextShadow::default(),
                    TextLayout::justify(Justify::Center),
                    crate::ui_theme::Plaque,
                ));
            });
        });
    commands
        .spawn((Node { position_type: PositionType::Absolute, width: percent(100), top: px(54), justify_content: JustifyContent::Center, ..default() }, Pickable::IGNORE))
        .with_children(|p| {
            theme().plaque(p).with_children(|p| {
                p.spawn((
                    CrierText,
                    Text::new(""),
                    TextFont { font: fonts.regular.clone().into(), font_size: bevy::text::FontSize::Px(17.0), ..default() },
                    TextColor(Color::srgb(0.98, 0.92, 0.78)),
                    TextShadow::default(),
                    TextLayout::justify(Justify::Center),
                    crate::ui_theme::Plaque,
                ));
            });
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
    let text = if lines.is_empty() || ui.open() { String::new() } else { format!("Глашатай на агоре: «{}»", lines.join("» · «")) };
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

/// Заголовок раздела: унциал бронзой.
fn heading(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32) {
    p.spawn(theme().title(s, size));
}

/// Кнопка темы; `on` — выбранная вкладка или то, что уже стоит в комнате.
fn button(p: &mut ChildSpawnerCommands, label: impl Into<String>, act: Act, enabled: bool, on: bool) {
    let label = label.into();
    if on {
        theme().toggle_button(p, &label, (act, Enabled(enabled)), true);
    } else {
        theme().small_button(p, &label, (act, Enabled(enabled)), enabled);
    }
}

/// Окно на тёмной доске в бронзовой раме, с прокруткой колесом.
fn panel(width: f32) -> impl Bundle {
    (
        Node { width: px(width), max_height: percent(100), flex_direction: FlexDirection::Column, padding: Skin::Wood.padding(), row_gap: px(6), overflow: Overflow::scroll_y(), overflow_clip_margin: OverflowClipMargin::content_box(), ..default() },
        ScrollPosition::default(),
        ScrollWheel,
        theme().skin(Skin::Wood),
        Interaction::default(),
        FocusPolicy::Block,
    )
}

/// Карточка внутри окна (комната, поселение): углубление с тонким ободком.
fn card() -> impl Bundle {
    (Node { flex_direction: FlexDirection::Column, row_gap: px(3), padding: UiRect::axes(px(10), px(7)), ..default() }, theme().slot_image(false), FocusPolicy::Pass)
}

fn row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(4), align_items: AlignItems::Center, ..default() }
}

fn rebuild(mut commands: Commands, mut ui: ResMut<TownUi>, fonts: Res<UiFonts>, loot: Res<LootState>, roots: Query<Entity, With<TownRoot>>, mut shown: Local<Window>) {
    if ui.built == ui.rev {
        return;
    }
    ui.built = ui.rev;
    let opening = *shown != ui.window && ui.window != Window::None;
    *shown = ui.window;
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
            // Снизу — место под хотбар и строку уведомлений.
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, padding: UiRect { top: px(16), bottom: px(150), ..default() }, ..default() },
            theme().backdrop(),
            GlobalZIndex(13),
        ))
        .insert_if(AppearChildren, || opening)
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
        heading(p, &d.name, 26.0);
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
                button(r, format!("Купить — {}", format_money(d.brief.price)), Act::Buy(id), money >= d.brief.price, false);
                for m in [1u8, 3, 6] {
                    let cost = d.brief.rent * m as u32;
                    button(r, format!("Снять на {m} мес. — {}", format_money(cost)), Act::Rent(id, m), money >= cost, false);
                }
            }
            Status::Rented { .. } if d.mine => {
                for m in [1u8, 3] {
                    let cost = d.brief.rent * m as u32;
                    button(r, format!("Продлить на {m} мес. — {}", format_money(cost)), Act::Rent(id, m), money >= cost, false);
                }
                button(r, "Съехать", Act::Sell(id), true, false);
            }
            Status::Owned if d.mine => {
                button(r, format!("Продать курии за {}", format_money(d.buyback)), Act::Sell(id), true, false);
            }
            _ => {}
        });
        // Комнаты.
        p.spawn(Node { height: px(6), ..default() });
        theme().divider(p);
        heading(p, "Комнаты", 20.0);
        if !d.mine {
            txt(p, fonts, "Обставить дом можно, когда он ваш: мастера сделают на заказ за неделю-две.", 13.0, DIM, false);
        }
        for (ri, room) in d.rooms.iter().enumerate() {
            p.spawn(card()).with_children(|c| {
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
                            button(r, label, Act::Furnish(id, ri as u8, Some(k)), !on && money >= k.price(), on);
                        }
                        if room.now.is_some() {
                            button(r, "Вынести всё", Act::Furnish(id, ri as u8, None), true, false);
                        }
                    });
                }
            });
        }
        // Гости.
        if d.mine {
            p.spawn(Node { height: px(6), ..default() });
            theme().divider(p);
            heading(p, "Ключи", 20.0);
            if d.players.is_empty() && d.guests.is_empty() {
                txt(p, fonts, "Друзей в игре нет. Когда зайдут — здесь можно дать им ключ.", 13.0, DIM, false);
            }
            p.spawn(row()).with_children(|r| {
                for (pid, name, has) in &d.players {
                    button(r, if *has { format!("{name}: забрать ключ") } else { format!("{name}: дать ключ") }, Act::Guest(id, *pid, !has), true, *has);
                }
            });
            if !d.guests.is_empty() {
                txt(p, fonts, format!("Ключи есть у: {}", d.guests.join(", ")), 13.0, DIM, false);
            }
        }
        p.spawn(Node { height: px(4), ..default() });
        p.spawn(row()).with_children(|r| {
            button(r, "Закрыть (Esc)", Act::Close, true, false);
            txt(r, fonts, "Вести и дома на продажу — клавиша O", 13.0, DIM, false);
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
        heading(p, "Вести Таврики", 28.0);
        let sea = if v.sea_open { "море открыто — корабли ходят" } else { "море закрыто до весны — кораблей нет" };
        txt(p, fonts, format!("{} · {sea}", v.date), 14.0, DIM, false);
        p.spawn(row()).with_children(|r| {
            button(r, "Поселения", Act::Tab(0), true, tab == 0);
            button(r, "Что слышно", Act::Tab(1), true, tab == 1);
            button(r, "Дома и стройки", Act::Tab(2), true, tab == 2);
            button(r, "Закрыть (Esc)", Act::Close, true, false);
        });
        theme().divider(p);
        match tab {
            0 => {
                for t in &v.towns {
                    let d = t.id.def();
                    p.spawn(card()).with_children(|c| {
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
                theme().divider(p);
                heading(p, "Цены в лавках Херсона", 20.0);
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
                        button(r, "Подробнее", Act::Show(*id), true, false);
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

/// Обход со снимками для проверки городов: `MRPG_TOWN_TOUR="имя#кадры|команда;команда @ …"`.
/// Команды — как в консоли (`fly`, `tp x y z yaw pitch`, `give Солид 30`), а ещё:
/// `!open` — окно усадьбы у входа, `!buy` — купить её, `!furnish` — заказать обстановку
/// первых комнат, `!news0..2` — окно «Вести» на вкладке, `!close` — закрыть окна.
/// После команд шага ждём кадры (по умолчанию 24) и снимаем в `MRPG_TOUR_DIR/имя.png`
/// (без имени — без снимка). После последнего шага игра закрывается.
#[allow(clippy::too_many_arguments)]
fn town_tour(
    mut commands: Commands,
    mut ui: ResMut<TownUi>,
    mut client: ResMut<RenetClient>,
    mut console: ResMut<crate::console::Console>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut state: Local<(usize, u32, bool)>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(tour) = std::env::var("MRPG_TOWN_TOUR") else { return };
    if ui.date.is_empty() {
        return;
    }
    let steps: Vec<&str> = tour.split('@').map(str::trim).collect();
    let (i, frames, started) = &mut *state;
    let Some(step) = steps.get(*i) else {
        *frames += 1;
        if *frames > 20 {
            exit.write(AppExit::Success);
        }
        return;
    };
    let (head, cmds) = step.split_once('|').unwrap_or((step, ""));
    let (name, wait) = head.split_once('#').map(|(n, w)| (n.trim(), w.trim().parse().unwrap_or(24))).unwrap_or((head.trim(), 24));
    if !*started {
        *started = true;
        *frames = 0;
        for c in cmds.split(';').map(str::trim).filter(|c| !c.is_empty()) {
            let estate = ui.near.unwrap_or(0);
            match c {
                "!open" => {
                    send(&mut client, &TownClientMessage::Open { estate });
                    free_cursor(&mut cursor);
                }
                "!buy" => send(&mut client, &TownClientMessage::Buy { estate }),
                "!furnish" => {
                    let kinds = [FurnishKind::Living, FurnishKind::Bedroom, FurnishKind::Kitchen, FurnishKind::Storeroom];
                    for (room, kind) in kinds.into_iter().enumerate() {
                        send(&mut client, &TownClientMessage::Furnish { estate, room: room as u8, kind: Some(kind) });
                    }
                }
                "!news0" | "!news1" | "!news2" => {
                    send(&mut client, &TownClientMessage::News);
                    ui.window = Window::News;
                    ui.tab = c.as_bytes()[5] - b'0';
                    ui.rev += 1;
                    free_cursor(&mut cursor);
                }
                "!close" => {
                    ui.window = Window::None;
                    ui.rev += 1;
                }
                _ => console.push(c),
            }
        }
        return;
    }
    *frames += 1;
    if *frames >= wait {
        if !name.is_empty() {
            let dir = std::env::var("MRPG_TOUR_DIR").unwrap_or_else(|_| ".".into());
            info!("обход городов: снимок {name}");
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(format!("{dir}/{name}.png")));
        }
        *i += 1;
        *started = false;
        *frames = 0;
    }
}

