//! Главное меню, подключение, настройки и пауза — прямо в окне игры (без консоли).
//!
//! За меню виден Херсонес на закате: камера ожидания медленно облетает город.
//! Оформление — порфир, золото и пергамент, как в византийских рукописях VI века.

#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::client::NetState;
use crate::inventory_ui::UiFonts;
use crate::ui_theme::{Appear, Skin, ink, theme};
use crate::protocol::{DEFAULT_PORT, PROTOCOL_ID, connection_config};
use crate::settings::{DRAW_DISTANCES, Preset, RENDER_SCALES, Settings};
use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, FontSize, TextCursorStyle};
use bevy::ui_widgets::TextInput;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;
use bevy_renet::netcode::{ClientAuthentication, ConnectToken, NETCODE_KEY_BYTES, NetcodeClientTransport};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// С чего начинать: меню или сразу хост / подключение (из командной строки).
#[derive(Resource, Clone, Copy, Debug)]
pub enum Start {
    Menu,
    Host(u16),
    Join(SocketAddr),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// Меню закрыто — идёт игра.
    None,
    Main,
    Join,
    Connecting,
    Settings,
    Pause,
    /// Сервер пропал посреди игры.
    Lost,
}

#[derive(Resource)]
pub struct MenuState {
    pub screen: Screen,
    /// Куда вернуться из настроек.
    back: Screen,
    /// Игра началась (сервер прислал приветствие).
    pub in_game: bool,
    /// Этот компьютер — хост (сервер запущен в этом процессе).
    pub hosting: bool,
    /// Текст ошибки на экране подключения.
    error: Option<String>,
    /// Когда начали подключаться (для тайм-аута).
    connect_started: f32,
    /// Адрес ищется в фоне (DNS), чтобы окно не замирало.
    resolving: Option<Arc<Mutex<Option<Result<SocketAddr, String>>>>>,
    /// Строка адреса, которую сейчас ищем (запомнится при успехе).
    pending_address: String,
}

impl MenuState {
    pub fn is_open(&self) -> bool {
        self.screen != Screen::None
    }
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MenuState {
            screen: Screen::Main,
            back: Screen::Main,
            in_game: false,
            hosting: false,
            error: None,
            connect_started: 0.0,
            resolving: None,
            pending_address: String::new(),
        })
        .add_systems(Startup, begin.after(crate::inventory_ui::setup_fonts))
        .add_systems(PreUpdate, escape_key.after(bevy::input::InputSystems))
        .add_systems(Update, (sync_name, handle_buttons, submit_on_enter, poll_resolve, watch_connection, rebuild, orbit_camera, hud_visibility, cursor_in_menu).chain());
    }
}

// ---------------------------------------------------------------------------
// Подключение
// ---------------------------------------------------------------------------

fn begin(mut commands: Commands, start: Res<Start>, mut menu: ResMut<MenuState>, mut net: ResMut<NetState>, time: Res<Time>, settings: Res<Settings>) {
    let name = settings.nickname.clone();
    match *start {
        // MRPG_MENU=settings|join — открыть экран сразу (для снимков меню в облаке).
        Start::Menu => {
            menu.screen = match std::env::var("MRPG_MENU").as_deref() {
                Ok("settings") => Screen::Settings,
                Ok("join") => Screen::Join,
                _ => Screen::Main,
            }
        }
        Start::Host(port) => host(&mut commands, &mut menu, &mut net, &time, port, &name),
        Start::Join(addr) => {
            menu.screen = Screen::Join;
            connect(&mut commands, &mut menu, &mut net, &time, addr, &name);
        }
    }
}

/// Запустить сервер в этом процессе и подключиться к нему.
fn host(commands: &mut Commands, menu: &mut MenuState, net: &mut NetState, time: &Time, port: u16, name: &str) {
    if !menu.hosting {
        match crate::server::bind_socket(port) {
            Ok(socket) => {
                // Секрет хозяина: консоль разработчика даёт читы только этому окну (см. console_net.rs).
                crate::console_net::make_host_token();
                std::thread::Builder::new()
                    .name("server".into())
                    .stack_size(16 * 1024 * 1024)
                    .spawn(move || crate::server::run(socket, false))
                    .expect("не удалось запустить сервер");
                menu.hosting = true;
            }
            Err(e) => {
                menu.screen = Screen::Main;
                menu.error = Some(format!("Порт {port} занят ({e}). Возможно, игра или сервер уже запущены."));
                return;
            }
        }
    }
    menu.back = Screen::Main;
    connect(commands, menu, net, time, SocketAddr::from(([127, 0, 0, 1], port)), name);
}

/// Сколько секунд тишины от другой стороны терпим, прежде чем считать связь потерянной.
const CONNECTION_TIMEOUT_SECS: i32 = 60;
/// Срок жизни билета: с запасом, чтобы часы хоста и друга могли расходиться до часа.
const CONNECT_TOKEN_EXPIRE_SECS: u64 = 3600;

/// Профиль с диска (ключ хозяина) с ником из меню.
fn profile_with_name(name: &str) -> crate::estates::Profile {
    let mut p = crate::estates::local_profile();
    let nick = crate::protocol::clean_name(name);
    if !nick.is_empty() {
        p.name = nick;
    }
    p
}

fn connect(commands: &mut Commands, menu: &mut MenuState, net: &mut NetState, time: &Time, addr: SocketAddr, name: &str) {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let client_id = now.as_nanos() as u64;
    let transport = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string()).and_then(|socket| {
        // Билет подключения собираем сами, а не через ClientAuthentication::Unsecure: там таймаут
        // жёстко 15 с, и если игра у друга замирает дольше (первая загрузка уровня, компиляция
        // шейдеров, перетаскивание окна), сервер выкидывал его. Сервер работает в режиме
        // Unsecure — его ключ из нулей, тот же, что здесь. Ник едет в user_data.
        let token = ConnectToken::generate(
            now,
            PROTOCOL_ID,
            CONNECT_TOKEN_EXPIRE_SECS,
            client_id,
            CONNECTION_TIMEOUT_SECS,
            vec![addr],
            // Постоянный профиль (ключ дома и ник): по ключу сервер узнаёт хозяина усадьбы.
            Some(&crate::estates::pack_profile(&profile_with_name(name))),
            &[0; NETCODE_KEY_BYTES],
        )
        .map_err(|e| e.to_string())?;
        NetcodeClientTransport::new(now, ClientAuthentication::Secure { connect_token: token }, socket).map_err(|e| e.to_string())
    });
    match transport {
        Ok(t) => {
            commands.insert_resource(RenetClient::new(connection_config()));
            commands.insert_resource(t);
            net.server_addr = Some(addr);
            net.error = None;
            menu.error = None;
            menu.screen = Screen::Connecting;
            menu.connect_started = time.elapsed_secs();
        }
        Err(e) => menu.error = Some(format!("Не удалось открыть соединение: {e}")),
    }
}

/// Бросить неудачное подключение: в следующий раз начнём с чистого клиента.
fn reset_connection(commands: &mut Commands) {
    commands.remove_resource::<NetcodeClientTransport>();
    commands.insert_resource(RenetClient::new(connection_config()));
}

fn start_resolve(menu: &mut MenuState, address: String) {
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let text = address.clone();
    std::thread::spawn(move || {
        let r = crate::launcher::resolve(&text);
        if let Ok(mut s) = out.lock() {
            *s = Some(r);
        }
    });
    menu.resolving = Some(slot);
    menu.pending_address = crate::launcher::clean(&address);
    menu.error = None;
}

fn poll_resolve(mut commands: Commands, mut menu: ResMut<MenuState>, mut net: ResMut<NetState>, mut settings: ResMut<Settings>, time: Res<Time>) {
    let Some(slot) = menu.resolving.clone() else { return };
    let Some(result) = slot.lock().ok().and_then(|mut s| s.take()) else { return };
    menu.resolving = None;
    match result {
        Ok(addr) => {
            if settings.last_address != menu.pending_address {
                settings.last_address = menu.pending_address.clone();
                settings.save();
            }
            menu.back = Screen::Join;
            let name = settings.nickname.clone();
            connect(&mut commands, &mut menu, &mut net, &time, addr, &name);
        }
        Err(e) => menu.error = Some(format!("Не получилось: {e}. Проверьте адрес.")),
    }
}

#[allow(clippy::too_many_arguments)]
fn watch_connection(
    mut commands: Commands,
    mut menu: ResMut<MenuState>,
    mut net: ResMut<NetState>,
    client: Option<Res<RenetClient>>,
    transport: Option<Res<NetcodeClientTransport>>,
    time: Res<Time>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if menu.screen == Screen::Connecting {
        if net.my_id.is_some() {
            menu.screen = if std::env::var("MRPG_MENU").as_deref() == Ok("pause") { Screen::Pause } else { Screen::None };
            menu.in_game = true;
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
            return;
        }
        let failed = transport.as_ref().and_then(|t| t.disconnect_reason()).map(|r| format!("{r:?}"));
        let failed = failed.or_else(|| net.error.clone()).or_else(|| client.as_ref().filter(|c| c.is_disconnected()).map(|_| "соединение закрыто".to_string()));
        let timeout = time.elapsed_secs() - menu.connect_started > 45.0;
        if failed.is_some() || timeout {
            let why = failed.unwrap_or_else(|| "сервер не отвечает 45 секунд".into());
            let addr = net.server_addr.map(|a| a.to_string()).unwrap_or_default();
            menu.error = Some(format!(
                "Не удалось подключиться к {addr}: {why}.\nПроверьте адрес, что хост запустил игру и туннель playit.gg, и что версии игры одинаковые."
            ));
            net.error = None;
            reset_connection(&mut commands);
            menu.screen = if menu.back == Screen::Join { Screen::Join } else { Screen::Main };
        }
    } else if menu.in_game && menu.screen != Screen::Lost && client.as_ref().is_some_and(|c| c.is_disconnected()) {
        menu.screen = Screen::Lost;
    }
}

// ---------------------------------------------------------------------------
// Клавиши
// ---------------------------------------------------------------------------

fn escape_key(keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<MenuState>, cursor: Single<&CursorOptions, With<PrimaryWindow>>) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    menu.screen = match menu.screen {
        // Esc в игре открывает паузу, только если мышь захвачена (не открыт инвентарь, торговля…).
        Screen::None if menu.in_game && cursor.grab_mode != CursorGrabMode::None => Screen::Pause,
        Screen::Pause => Screen::None,
        Screen::Settings => menu.back,
        Screen::Join => Screen::Main,
        other => other,
    };
}

/// Enter в поле адреса — подключиться.
fn submit_on_enter(keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<MenuState>, field: Query<&EditableText, With<AddressField>>) {
    if menu.screen != Screen::Join || menu.resolving.is_some() || !(keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)) {
        return;
    }
    if let Ok(text) = field.single() {
        let address = text.value().to_string();
        if address.trim().is_empty() {
            menu.error = Some("Введите адрес хоста.".into());
        } else {
            start_resolve(&mut menu, address);
        }
    }
}

/// Пока открыто меню — мышь свободна; когда закрыли паузу — снова захвачена.
fn cursor_in_menu(menu: Res<MenuState>, mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>, mut was_open: Local<bool>) {
    let open = menu.is_open();
    if open && (cursor.grab_mode != CursorGrabMode::None || !cursor.visible) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    if !open && *was_open && menu.in_game {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    *was_open = open;
}

// ---------------------------------------------------------------------------
// Кнопки
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Field {
    DrawDistance,
    Shadows,
    Water,
    Effects,
    Textures,
    RenderScale,
    Vsync,
    WindowMode,
    Fov,
    Sensitivity,
    Volume,
    Music,
    Sfx,
    Ambient,
    ShowFps,
}

#[derive(Component, Clone, Copy, PartialEq, Debug)]
enum Act {
    Host,
    OpenJoin,
    Connect,
    OpenSettings,
    Back,
    Resume,
    MainMenu,
    Quit,
    CancelConnect,
    Preset(Preset),
    Adjust(Field, i32),
}

#[derive(Component)]
struct AddressField;

/// Поле ника в главном меню.
#[derive(Component)]
struct NameField;

/// Ник из поля — сразу в настройки (сохраняются при «Создать» / «Подключиться»).
fn sync_name(field: Query<&EditableText, (With<NameField>, Changed<EditableText>)>, mut settings: ResMut<Settings>) {
    for text in &field {
        let name = crate::protocol::clean_name(&text.value().to_string());
        if settings.nickname != name {
            settings.nickname = name;
        }
    }
}

#[derive(Component)]
struct MenuRoot;

#[allow(clippy::too_many_arguments)]
fn handle_buttons(
    mut commands: Commands,
    buttons: Query<(&Interaction, &Act), Changed<Interaction>>,
    mut menu: ResMut<MenuState>,
    mut net: ResMut<NetState>,
    mut settings: ResMut<Settings>,
    field: Query<&EditableText, With<AddressField>>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    for (i, act) in &buttons {
        if *i != Interaction::Pressed {
            continue;
        }
        match *act {
            Act::Host => {
                settings.save();
                let name = settings.nickname.clone();
                host(&mut commands, &mut menu, &mut net, &time, DEFAULT_PORT, &name);
            }
            Act::OpenJoin => {
                settings.save();
                menu.error = None;
                menu.screen = Screen::Join;
            }
            Act::Connect => {
                if menu.resolving.is_some() {
                    continue;
                }
                let address = field.single().map(|t| t.value().to_string()).unwrap_or_default();
                if address.trim().is_empty() {
                    menu.error = Some("Введите адрес хоста.".into());
                } else {
                    start_resolve(&mut menu, address);
                }
            }
            Act::OpenSettings => {
                menu.back = menu.screen;
                menu.screen = Screen::Settings;
            }
            Act::Back => {
                menu.screen = match menu.screen {
                    Screen::Settings => menu.back,
                    _ => Screen::Main,
                };
            }
            Act::Resume => menu.screen = Screen::None,
            Act::CancelConnect => {
                reset_connection(&mut commands);
                net.server_addr = None;
                menu.screen = if menu.back == Screen::Join { Screen::Join } else { Screen::Main };
            }
            Act::MainMenu => {
                // Самый надёжный способ всё сбросить — перезапустить игру с главным меню.
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).spawn();
                }
                exit.write(AppExit::Success);
            }
            Act::Quit => {
                exit.write(AppExit::Success);
            }
            Act::Preset(p) => {
                settings.apply_preset(p);
                settings.save();
            }
            Act::Adjust(f, d) => {
                adjust(&mut settings, f, d);
                settings.save();
            }
        }
    }
}

fn step_in<T: Copy + PartialOrd>(list: &[T], cur: T, d: i32) -> T {
    // Ближайший пункт списка к текущему значению, затем шаг.
    let i = list.iter().position(|v| *v >= cur).unwrap_or(list.len() - 1) as i32;
    let i = if d > 0 && list[i as usize] > cur { i - 1 } else { i };
    list[(i + d).clamp(0, list.len() as i32 - 1) as usize]
}

fn adjust(s: &mut Settings, f: Field, d: i32) {
    let bump = |v: u8, max: u8| (v as i32 + d).clamp(0, max as i32) as u8;
    match f {
        Field::DrawDistance => s.draw_distance = step_in(&DRAW_DISTANCES, s.draw_distance, d),
        Field::Shadows => s.shadows = bump(s.shadows, 3),
        Field::Water => s.water = bump(s.water, 1),
        Field::Effects => s.effects = bump(s.effects, 3),
        Field::Textures => s.textures = bump(s.textures, 2),
        Field::RenderScale => s.render_scale = step_in(&RENDER_SCALES, s.render_scale, d),
        Field::Vsync => s.vsync = !s.vsync,
        Field::WindowMode => s.window_mode = (s.window_mode as i32 + d).rem_euclid(3) as u8,
        Field::Fov => s.fov = (s.fov + 5.0 * d as f32).clamp(60.0, 110.0),
        Field::Sensitivity => s.sensitivity = ((s.sensitivity + 0.1 * d as f32) * 10.0).round().clamp(1.0, 50.0) / 10.0,
        Field::Volume => s.volume = ((s.volume + 0.1 * d as f32) * 10.0).round().clamp(0.0, 10.0) / 10.0,
        Field::Music => s.music = ((s.music + 0.1 * d as f32) * 10.0).round().clamp(0.0, 10.0) / 10.0,
        Field::Sfx => s.sfx = ((s.sfx + 0.1 * d as f32) * 10.0).round().clamp(0.0, 10.0) / 10.0,
        Field::Ambient => s.ambient = ((s.ambient + 0.1 * d as f32) * 10.0).round().clamp(0.0, 10.0) / 10.0,
        Field::ShowFps => s.show_fps = !s.show_fps,
    }
}

fn value_text(s: &Settings, f: Field) -> String {
    let q4 = |v: u8| ["выкл.", "низкие", "средние", "высокие"][v.min(3) as usize].to_string();
    let yes = |b: bool| if b { "вкл." } else { "выкл." }.to_string();
    match f {
        Field::DrawDistance => format!("{:.0} м", s.draw_distance),
        Field::Shadows => q4(s.shadows),
        Field::Water => if s.water == 0 { "простая" } else { "подробная" }.into(),
        Field::Effects => crate::settings::effects_name(s.effects).into(),
        Field::Textures => ["низкие", "средние", "высокие"][s.textures.min(2) as usize].into(),
        Field::RenderScale => format!("{} %", s.render_scale),
        Field::Vsync => yes(s.vsync),
        Field::WindowMode => crate::settings::window_mode_name(s.window_mode).into(),
        Field::Fov => format!("{:.0}°", s.fov),
        Field::Sensitivity => format!("{:.1}", s.sensitivity),
        Field::Volume => format!("{:.0} %", s.volume * 100.0),
        Field::Music => format!("{:.0} %", s.music * 100.0),
        Field::Sfx => format!("{:.0} %", s.sfx * 100.0),
        Field::Ambient => format!("{:.0} %", s.ambient * 100.0),
        Field::ShowFps => yes(s.show_fps),
    }
}

// ---------------------------------------------------------------------------
// Оформление
// ---------------------------------------------------------------------------

const GOLD: Color = ink::GOLD;
const GOLD_DIM: Color = Color::srgba(0.72, 0.56, 0.30, 0.55);
const PARCHMENT: Color = ink::TEXT;
const INK_SOFT: Color = ink::DIM;
const ERROR: Color = ink::BAD;

fn font(fonts: &UiFonts, bold: bool, size: f32) -> TextFont {
    TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: FontSize::Px(size), ..default() }
}

fn label(p: &mut ChildSpawnerCommands, fonts: &UiFonts, s: &str, size: f32, color: Color) {
    p.spawn((Text::new(s), font(fonts, false, size), TextColor(color)));
}

/// Разделитель с крестом (общая тема интерфейса).
fn ornament(p: &mut ChildSpawnerCommands) {
    theme().divider(p);
}

fn button(p: &mut ChildSpawnerCommands, _fonts: &UiFonts, text: &str, act: Act) {
    theme().button_sized(p, text, 20.0, act, true).insert(Node {
        width: percent(100),
        min_height: px(48),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    });
}

fn small_button(p: &mut ChildSpawnerCommands, _fonts: &UiFonts, text: &str, act: Act, width: f32) {
    theme().button_sized(p, text, 16.0, act, true).insert(Node {
        width: px(width),
        min_height: px(30),
        flex_shrink: 0.0,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    });
}

/// Кнопка набора графики: выбранный набор «вдавлен».
fn preset_button(p: &mut ChildSpawnerCommands, preset: Preset, on: bool) {
    theme().toggle_button(p, preset.name(), Act::Preset(preset), on).insert(Node {
        width: px(118),
        min_height: px(30),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    });
}

// ---------------------------------------------------------------------------
// Построение экранов
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    menu: Res<MenuState>,
    settings: Res<Settings>,
    net: Res<NetState>,
    fonts: Option<Res<UiFonts>>,
    roots: Query<Entity, With<MenuRoot>>,
    mut built: Local<Option<(Screen, Option<String>, bool)>>,
) {
    let Some(fonts) = fonts else { return };
    let key = (menu.screen, menu.error.clone(), menu.resolving.is_some());
    let settings_changed = menu.screen == Screen::Settings && settings.is_changed();
    if built.as_ref() == Some(&key) && !settings_changed {
        return;
    }
    *built = Some(key);
    for e in &roots {
        commands.entity(e).despawn();
    }
    match menu.screen {
        Screen::None => {}
        Screen::Main => side_panel(&mut commands, &fonts, |p| {
            p.spawn(Node { width: percent(100), align_items: AlignItems::Center, column_gap: px(12), margin: UiRect::bottom(px(4)), ..default() }).with_children(|r| {
                r.spawn((Text::new("Имя"), font(&fonts, false, 18.0), TextColor(INK_SOFT)));
                r.spawn((
                    NameField,
                    TextInput,
                    EditableText { max_characters: Some(24), ..EditableText::new(settings.nickname.as_str()) },
                    TextLayout::no_wrap(),
                    font(&fonts, false, 20.0),
                    TextColor(PARCHMENT),
                    TextCursorStyle { color: GOLD, ..default() },
                    Node { flex_grow: 1.0, height: px(40), padding: UiRect::axes(px(10), px(7)), border: UiRect::all(px(1)), ..default() },
                    BorderColor::all(GOLD_DIM),
                    BackgroundColor(Color::srgba(0.06, 0.03, 0.04, 0.95)),
                ));
            });
            button(p, &fonts, "Создать игру", Act::Host);
            button(p, &fonts, "Подключиться", Act::OpenJoin);
            button(p, &fonts, "Настройки", Act::OpenSettings);
            button(p, &fonts, "Выход", Act::Quit);
            if let Some(err) = &menu.error {
                label(p, &fonts, err, 16.0, ERROR);
            }
            p.spawn(Node { flex_grow: 1.0, ..default() });
            label(
                p,
                &fonts,
                &format!("«Создать игру» — вы хост: игра слушает UDP-порт {DEFAULT_PORT}. Друзьям дайте адрес своего туннеля playit.gg (вида xxx.at.ply.gg:12345) или свой IP, если порт открыт на роутере."),
                14.0,
                INK_SOFT,
            );
        }),
        Screen::Join => side_panel(&mut commands, &fonts, |p| {
            label(p, &fonts, "Адрес хоста", 20.0, PARCHMENT);
            p.spawn((
                AddressField,
                TextInput,
                EditableText::new(settings.last_address.as_str()),
                TextLayout::no_wrap(),
                font(&fonts, false, 22.0),
                TextColor(PARCHMENT),
                TextCursorStyle { color: GOLD, ..default() },
                AutoFocus,
                Node { width: percent(100), height: px(46), padding: UiRect::axes(px(12), px(9)), border: UiRect::all(px(1)), ..default() },
                BorderColor::all(GOLD),
                BackgroundColor(Color::srgba(0.06, 0.03, 0.04, 0.95)),
            ));
            label(p, &fonts, "Например  abc.at.ply.gg:12345  или  192.168.1.5  (порт по умолчанию 5000). Ctrl+V — вставить.", 14.0, INK_SOFT);
            if menu.resolving.is_some() {
                label(p, &fonts, "Ищу адрес…", 16.0, PARCHMENT);
            }
            if let Some(err) = &menu.error {
                label(p, &fonts, err, 16.0, ERROR);
            }
            button(p, &fonts, "Подключиться", Act::Connect);
            button(p, &fonts, "Назад", Act::Back);
        }),
        Screen::Connecting => side_panel(&mut commands, &fonts, |p| {
            let addr = net.server_addr.map(|a| a.to_string()).unwrap_or_default();
            let what = if menu.hosting && menu.back == Screen::Main { "Создаём игру…".to_string() } else { format!("Подключение к {addr}…") };
            label(p, &fonts, &what, 22.0, PARCHMENT);
            label(p, &fonts, "Город строится, корабли входят в гавань.", 15.0, INK_SOFT);
            button(p, &fonts, "Отмена", Act::CancelConnect);
        }),
        Screen::Pause => center_panel(&mut commands, &fonts, "Пауза", 440.0, |p| {
            button(p, &fonts, "Продолжить", Act::Resume);
            button(p, &fonts, "Настройки", Act::OpenSettings);
            button(p, &fonts, "В главное меню", Act::MainMenu);
            button(p, &fonts, "Выйти из игры", Act::Quit);
            if menu.hosting {
                label(p, &fonts, &format!("Вы хост (UDP-порт {DEFAULT_PORT}). Если выйти, игра закончится и для друзей."), 14.0, INK_SOFT);
            }
        }),
        Screen::Lost => center_panel(&mut commands, &fonts, "Связь потеряна", 480.0, |p| {
            label(p, &fonts, "Сервер не отвечает: хост вышел из игры или пропал интернет.", 16.0, PARCHMENT);
            button(p, &fonts, "В главное меню", Act::MainMenu);
            button(p, &fonts, "Выйти из игры", Act::Quit);
        }),
        Screen::Settings => center_panel(&mut commands, &fonts, "Настройки", 1000.0, |p| settings_body(p, &fonts, &settings)),
    }
}

/// Левая панель главного меню: заголовок, орнамент, содержимое. Справа виден город.
fn side_panel(commands: &mut Commands, fonts: &UiFonts, body: impl FnOnce(&mut ChildSpawnerCommands)) {
    commands
        .spawn((MenuRoot, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() }, GlobalZIndex(100)))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(480),
                    height: percent(100),
                    padding: UiRect::axes(px(48), px(52)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    ..default()
                },
                theme().skin(Skin::Wood),
                Appear::default(),
            ))
            .with_children(|p| {
                title_block(p, fonts);
                body(p);
            });
        });
}

fn title_block(p: &mut ChildSpawnerCommands, fonts: &UiFonts) {
    p.spawn(theme().title("Херсонес", 56.0));
    p.spawn((Text::new("Лето 510 года. Царствование Анастасия"), font(fonts, false, 18.0), TextColor(PARCHMENT)));
    ornament(p);
}

/// Окно по центру поверх затемнённой игры (пауза, настройки).
fn center_panel(commands: &mut Commands, fonts: &UiFonts, title: &str, width: f32, body: impl FnOnce(&mut ChildSpawnerCommands)) {
    commands
        .spawn((
            MenuRoot,
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            theme().backdrop(),
            GlobalZIndex(100),
        ))
        .with_children(|root| {
            root.spawn((
                Node { width: px(width), max_width: percent(98), max_height: percent(96), padding: UiRect::axes(px(40), px(30)), flex_direction: FlexDirection::Column, row_gap: px(10), ..default() },
                theme().skin(Skin::Wood),
                Appear::default(),
            ))
            .with_children(|p| {
                p.spawn(theme().title(title, 34.0));
                let _ = fonts;
                ornament(p);
                body(p);
            });
        });
}

fn settings_body(p: &mut ChildSpawnerCommands, fonts: &UiFonts, s: &Settings) {
    // Наборы.
    p.spawn(Node { width: percent(100), column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
        r.spawn((Text::new("Набор графики"), font(fonts, false, 17.0), TextColor(INK_SOFT), Node { width: px(170), ..default() }));
        let current = s.preset();
        for preset in Preset::ALL {
            preset_button(r, preset, current == Some(preset));
        }
    });
    // Две колонки: графика слева, экран, управление и звук справа — чтобы всё влезало и в 720p.
    p.spawn(Node { width: percent(100), column_gap: px(36), ..default() }).with_children(|cols| {
        cols.spawn(Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|c| {
            section(c, fonts, "Графика");
            row(c, fonts, s, "Дальность прорисовки", Field::DrawDistance, None);
            row(c, fonts, s, "Тени", Field::Shadows, None);
            row(c, fonts, s, "Вода", Field::Water, None);
            row(c, fonts, s, "Эффекты", Field::Effects, Some("блум, SSAO, лучи, дым"));
            row(c, fonts, s, "Текстуры", Field::Textures, Some("после перезапуска"));
            row(c, fonts, s, "Масштаб рендера", Field::RenderScale, None);
        });
        cols.spawn(Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|c| {
            section(c, fonts, "Экран");
            row(c, fonts, s, "Режим", Field::WindowMode, Some("F11"));
            row(c, fonts, s, "Верт. синхронизация", Field::Vsync, None);
            row(c, fonts, s, "Показывать FPS", Field::ShowFps, None);
            section(c, fonts, "Управление и звук");
            row(c, fonts, s, "Угол обзора", Field::Fov, None);
            row(c, fonts, s, "Чувствит. мыши", Field::Sensitivity, None);
            row(c, fonts, s, "Громкость", Field::Volume, None);
            row(c, fonts, s, "Музыка", Field::Music, None);
            row(c, fonts, s, "Эффекты", Field::Sfx, None);
            row(c, fonts, s, "Окружение", Field::Ambient, None);
        });
    });
    p.spawn(Node { width: percent(100), justify_content: JustifyContent::Center, margin: UiRect::top(px(8)), ..default() }).with_children(|r| {
        small_button(r, fonts, "Готово", Act::Back, 220.0);
    });
}

fn section(p: &mut ChildSpawnerCommands, fonts: &UiFonts, title: &str) {
    p.spawn((Text::new(title), font(fonts, true, 19.0), TextColor(GOLD), Node { margin: UiRect::top(px(8)), ..default() }));
}

fn row(p: &mut ChildSpawnerCommands, fonts: &UiFonts, s: &Settings, title: &str, f: Field, hint: Option<&str>) {
    p.spawn(Node { width: percent(100), height: px(32), align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|r| {
        r.spawn(Node { flex_grow: 1.0, align_items: AlignItems::Baseline, column_gap: px(8), ..default() }).with_children(|l| {
            l.spawn((Text::new(title), font(fonts, false, 17.0), TextColor(PARCHMENT)));
            if let Some(h) = hint {
                l.spawn((Text::new(format!("({h})")), font(fonts, false, 12.0), TextColor(INK_SOFT)));
            }
        });
        small_button(r, fonts, "‹", Act::Adjust(f, -1), 34.0);
        r.spawn(Node { width: px(128), justify_content: JustifyContent::Center, ..default() }).with_children(|v| {
            v.spawn((Text::new(value_text(s, f)), font(fonts, true, 17.0), TextColor(GOLD)));
        });
        small_button(r, fonts, "›", Act::Adjust(f, 1), 34.0);
    });
}

// ---------------------------------------------------------------------------
// Фон меню и интерфейс игры
// ---------------------------------------------------------------------------

/// Пока открыто главное меню, камера медленно облетает город.
fn orbit_camera(menu: Res<MenuState>, time: Res<Time>, mut cams: Query<&mut Transform, With<crate::client::WaitingCamera>>) {
    if menu.in_game || std::env::var_os("MRPG_CAM").is_some() {
        return;
    }
    let center = crate::level::spawn_point(0) + Vec3::Y * 4.0;
    let a = time.elapsed_secs() * 0.025 + 2.2;
    for mut t in &mut cams {
        let eye = center + Vec3::new(a.cos() * 75.0, 34.0, a.sin() * 75.0);
        *t = Transform::from_translation(eye).looking_at(center, Vec3::Y);
    }
}

/// Прицел, полоски здоровья и подсказки не нужны, пока игра не началась.
fn hud_visibility(menu: Res<MenuState>, mut hud: Query<&mut Visibility, With<crate::hud::HudRoot>>) {
    let want = if menu.in_game { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut hud {
        v.set_if_neq(want);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_through_lists() {
        assert_eq!(step_in(&DRAW_DISTANCES, 350.0, 1), 450.0);
        assert_eq!(step_in(&DRAW_DISTANCES, 350.0, -1), 275.0);
        assert_eq!(step_in(&DRAW_DISTANCES, 1000.0, 1), 1000.0);
        assert_eq!(step_in(&DRAW_DISTANCES, 150.0, -1), 150.0);
        // Значение не из списка (поправлено руками) — к ближайшему соседу.
        assert_eq!(step_in(&DRAW_DISTANCES, 400.0, 1), 450.0);
        assert_eq!(step_in(&DRAW_DISTANCES, 400.0, -1), 350.0);
        assert_eq!(step_in(&RENDER_SCALES, 100, -1), 85);
    }

    #[test]
    fn adjusting_settings_stays_in_range() {
        let mut s = Settings::default();
        for _ in 0..10 {
            adjust(&mut s, Field::Fov, 1);
            adjust(&mut s, Field::Volume, 1);
            adjust(&mut s, Field::Shadows, 1);
        }
        assert_eq!(s.fov, 110.0);
        assert_eq!(s.volume, 1.0);
        assert_eq!(s.shadows, 3);
        adjust(&mut s, Field::WindowMode, -1);
        assert_eq!(s.window_mode, 2);
    }
}

