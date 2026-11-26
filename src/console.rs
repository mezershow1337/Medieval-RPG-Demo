//! Консоль разработчика (клавиша ~ / Ё): полёт, телепорт, выдача предметов, снимки багов.
//!
//! Команды, которые меняют мир (читы), выполняет сервер — см. `console_server.rs`. Здесь окно,
//! ввод, история и команды, которые касаются только своего компьютера: координаты, снимки,
//! счётчик кадров, каркас.

use crate::console_net::*;
use crate::player::LocalPlayer;
use crate::protocol::{decode, encode};
use bevy::clipboard::Clipboard;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;
use std::collections::VecDeque;
use std::path::PathBuf;

const MAX_LINES: usize = 300;
const VISIBLE_LINES: usize = 16;
const COLOR_TEXT: Color = Color::srgb(0.88, 0.85, 0.78);
const COLOR_ECHO: Color = Color::srgb(0.95, 0.78, 0.42);
const COLOR_ERROR: Color = Color::srgb(1.0, 0.45, 0.38);
const COLOR_SERVER: Color = Color::srgb(0.68, 0.86, 0.66);

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WireframePlugin::default())
            .insert_resource(Console::default())
            .insert_resource(DevMove::default())
            .insert_resource(DevView::default())
            .add_systems(PreUpdate, console_input.after(bevy::input::InputSystems))
            .add_systems(Update, (spawn_console_ui, autoexec, execute_commands, receive_replies, take_shots, redraw_console, update_stats).chain());
    }
}

/// Читы движения своего персонажа (их подтверждает сервер).
#[derive(Resource, Default)]
pub struct DevMove {
    pub cheats: Cheats,
}

/// Что показывать поверх игры.
#[derive(Resource, Default)]
struct DevView {
    stats: bool,
    /// Снимок экрана: путь, сколько кадров ещё ждать (пока спрячется консоль), запись для bugs.txt.
    shot: Option<(PathBuf, u8, Option<String>)>,
}

#[derive(Resource)]
pub struct Console {
    pub open: bool,
    input: String,
    lines: VecDeque<(String, Color)>,
    history: Vec<String>,
    /// Позиция в истории при листании стрелками (None — новая строка).
    hist_pos: Option<usize>,
    /// Сколько строк прокручено вверх.
    scroll: usize,
    /// Мышь была захвачена до открытия — вернуть при закрытии.
    was_locked: bool,
    ctrl: bool,
    pub(crate) queue: Vec<String>,
    dirty: bool,
}

impl Default for Console {
    fn default() -> Self {
        let mut c = Self {
            open: false,
            input: String::new(),
            lines: VecDeque::new(),
            history: Vec::new(),
            hist_pos: None,
            scroll: 0,
            was_locked: false,
            ctrl: false,
            queue: Vec::new(),
            dirty: true,
        };
        c.print("Консоль разработчика. help — список команд, Tab — дополнить, ↑↓ — история, Esc или ~ — закрыть.", COLOR_SERVER);
        c
    }
}

impl Console {
    /// Выполнить команду, как если бы её набрали (для обходов со снимками).
    pub fn push(&mut self, line: impl Into<String>) {
        self.queue.push(line.into());
    }

    fn print(&mut self, text: impl AsRef<str>, color: Color) {
        for l in text.as_ref().lines() {
            self.lines.push_back((l.to_string(), color));
        }
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
        self.scroll = 0;
        self.dirty = true;
    }
    fn ok(&mut self, text: impl AsRef<str>) {
        self.print(text, COLOR_TEXT);
    }
    fn err(&mut self, text: impl AsRef<str>) {
        self.print(text, COLOR_ERROR);
    }
}

/// Все команды с короткой подсказкой (для help и Tab).
const COMMANDS: &[(&str, &str)] = &[
    ("help", "этот список"),
    ("fly", "[вкл|выкл] полёт: WASD по взгляду, Пробел вверх, Ctrl вниз, Shift быстрее"),
    ("noclip", "[вкл|выкл] полёт сквозь стены"),
    ("speed", "<множитель> скорость ходьбы и полёта (1 — обычная)"),
    ("god", "[вкл|выкл] бессмертие и бесконечные силы"),
    ("notarget", "[вкл|выкл] враги вас не замечают"),
    ("tp", "x y z [yaw pitch] | <место> | <id игрока> — телепорт"),
    ("places", "[часть] список мест для tp"),
    ("pos", "координаты; копирует в буфер готовую команду tp"),
    ("bug", "[описание] снимок экрана + координаты в screenshots/bugs.txt (то же — F12)"),
    ("screenshot", "снимок экрана с координатами в имени файла"),
    ("give", "<название> [кол-во] [качество] выдать предмет"),
    ("items", "<часть названия> найти предметы"),
    ("heal", "вылечить раны, восстановить здоровье"),
    ("kill", "умереть (проверить смерть и возрождение)"),
    ("killall", "[радиус] убить всех людей рядом (по умолчанию 30 м)"),
    ("spawn", "<stratiot|bucellarius|goth|alan|hun> [сколько] враг перед вами"),
    ("stats", "FPS, координаты, читы в углу экрана (fps — то же)"),
    ("wireframe", "каркас всех моделей"),
    ("time", "[ЧЧ[:ММ]] который час; с числом — перевести часы мира (ночь, рассвет…)"),
    ("weather", "[дождь|туман|ясно|ветер] погода сейчас; со словом — промотать до неё"),
    ("who", "игроки на сервере"),
    ("sv_cheats", "0|1 (только хозяин) разрешить читы всем игрокам"),
    ("clear", "очистить консоль"),
    ("quit", "выйти из игры"),
    ("autoexec.cfg", "файл рядом с игрой: команды по строке, выполняются при входе"),
];

#[derive(Component)]
struct ConsoleRoot;
#[derive(Component)]
struct ConsoleLog;
#[derive(Component)]
struct ConsoleInputText;
#[derive(Component)]
struct StatsText;

fn spawn_console_ui(mut commands: Commands, fonts: Option<Res<crate::inventory_ui::UiFonts>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some(fonts) = fonts else { return };
    *done = true;
    let font = |size: f32| TextFont { font: fonts.regular.clone().into(), font_size: FontSize::Px(size), ..default() };
    commands
        .spawn((
            ConsoleRoot,
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                left: px(0),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(14), px(8)),
                border: UiRect::bottom(px(2)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.94)),
            BorderColor::all(Color::srgb(0.55, 0.42, 0.22)),
            GlobalZIndex(100),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((ConsoleLog, Node { flex_direction: FlexDirection::Column, min_height: px(VISIBLE_LINES as f32 * 19.0), justify_content: JustifyContent::FlexEnd, ..default() }));
            p.spawn((ConsoleInputText, Text::new("> "), font(17.0), TextColor(COLOR_ECHO), Node { margin: UiRect::top(px(6)), ..default() }));
        });
    commands.spawn((
        StatsText,
        Text::new(""),
        font(15.0),
        TextColor(Color::srgb(0.9, 0.95, 0.8)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, right: px(12), top: px(60), ..default() },
        GlobalZIndex(99),
        Visibility::Hidden,
    ));
}

/// Ввод: открывает консоль и, пока она открыта, забирает клавиатуру и мышь у игры.
#[allow(clippy::too_many_arguments)]
fn console_input(
    mut events: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut console: ResMut<Console>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut clipboard: Option<ResMut<Clipboard>>,
) {
    let was_open = console.open;
    for ev in events.read() {
        if matches!(ev.key_code, KeyCode::ControlLeft | KeyCode::ControlRight) {
            console.ctrl = ev.state == ButtonState::Pressed;
        }
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if ev.key_code == KeyCode::Backquote && !ev.repeat {
            let open = !console.open;
            set_open(&mut console, &mut cursor, open);
            continue;
        }
        if !console.open {
            if ev.key_code == KeyCode::F12 && !ev.repeat {
                console.queue.push("bug".into());
            }
            continue;
        }
        match ev.key_code {
            KeyCode::Escape => set_open(&mut console, &mut cursor, false),
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let line = std::mem::take(&mut console.input);
                let line = line.trim().to_string();
                if !line.is_empty() {
                    if console.history.last() != Some(&line) {
                        console.history.push(line.clone());
                    }
                    console.queue.push(line);
                }
                console.hist_pos = None;
            }
            KeyCode::Backspace => {
                if console.ctrl {
                    let trimmed = console.input.trim_end().len();
                    let cut = console.input[..trimmed].rfind(' ').map_or(0, |i| i + 1);
                    console.input.truncate(cut);
                } else {
                    console.input.pop();
                }
            }
            KeyCode::ArrowUp => {
                if console.history.is_empty() {
                    continue;
                }
                let i = console.hist_pos.map_or(console.history.len() - 1, |i| i.saturating_sub(1));
                console.hist_pos = Some(i);
                console.input = console.history[i].clone();
            }
            KeyCode::ArrowDown => match console.hist_pos {
                Some(i) if i + 1 < console.history.len() => {
                    console.hist_pos = Some(i + 1);
                    console.input = console.history[i + 1].clone();
                }
                _ => {
                    console.hist_pos = None;
                    console.input.clear();
                }
            },
            KeyCode::PageUp => {
                console.scroll = (console.scroll + VISIBLE_LINES / 2).min(console.lines.len().saturating_sub(VISIBLE_LINES));
            }
            KeyCode::PageDown => console.scroll = console.scroll.saturating_sub(VISIBLE_LINES / 2),
            KeyCode::Tab => complete(&mut console),
            KeyCode::KeyV if console.ctrl => {
                if let Some(cb) = clipboard.as_deref_mut()
                    && let Some(Ok(text)) = cb.fetch_text().poll_result()
                {
                    let text: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
                    console.input.push_str(text.trim());
                }
            }
            KeyCode::KeyL if console.ctrl => {
                console.lines.clear();
            }
            _ if console.ctrl => {}
            _ => {
                if let Some(text) = &ev.text {
                    console.input.extend(text.chars().filter(|c| !c.is_control()));
                } else if let Key::Character(s) = &ev.logical_key {
                    console.input.push_str(s);
                } else if ev.logical_key == Key::Space {
                    console.input.push(' ');
                }
            }
        }
        console.dirty = true;
    }
    if console.open || was_open {
        // Игра не видит нажатий, пока печатаем (иначе E, Tab, 4… сработали бы в мире).
        keys.reset_all();
        mouse.reset_all();
        console.ctrl = console.ctrl && console.open;
    }
}

fn set_open(console: &mut Console, cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>, open: bool) {
    if console.open == open {
        return;
    }
    console.open = open;
    console.dirty = true;
    let Ok(mut cursor) = cursor.single_mut() else { return };
    if open {
        console.was_locked = cursor.grab_mode != CursorGrabMode::None;
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if console.was_locked {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

/// Дополнение по Tab: команды, места для tp, предметы для give.
fn complete(console: &mut Console) {
    let input = console.input.clone();
    let (head, last) = match input.rfind(' ') {
        Some(i) => (&input[..=i], &input[i + 1..]),
        None => ("", input.as_str()),
    };
    let first = input.split_whitespace().next().unwrap_or("").to_lowercase();
    let low = last.to_lowercase();
    let options: Vec<String> = if head.is_empty() {
        COMMANDS.iter().map(|(c, _)| c.to_string()).filter(|c| c.starts_with(&low) && !c.contains('.')).collect()
    } else if first == "tp" && head.split_whitespace().count() == 1 {
        crate::level::places().iter().map(|p| p.id.to_string()).filter(|id| id.starts_with(&low)).collect()
    } else if first == "spawn" && head.split_whitespace().count() == 1 {
        ["stratiot", "bucellarius", "goth", "alan", "hun"].iter().map(|s| s.to_string()).filter(|s| s.starts_with(&low)).collect()
    } else if first == "give" {
        let q = input[4..].trim_start().to_lowercase();
        let found: Vec<String> = crate::items::ITEM_BASES.iter().map(|b| b.name.to_string()).filter(|n| n.to_lowercase().starts_with(&q)).collect();
        if found.len() == 1 {
            console.input = format!("give «{}» ", found[0]);
            return;
        }
        found
    } else {
        Vec::new()
    };
    match options.len() {
        0 => {}
        1 => console.input = format!("{head}{} ", options[0]),
        _ => {
            // Общее начало всех вариантов.
            let mut common: String = options[0].clone();
            for o in &options[1..] {
                let n = common.chars().zip(o.chars()).take_while(|(a, b)| a == b).count();
                common = common.chars().take(n).collect();
            }
            if common.chars().count() > last.chars().count() && first != "give" {
                console.input = format!("{head}{common}");
            }
            let shown: Vec<_> = options.iter().take(40).cloned().collect();
            let more = if options.len() > 40 { format!(" … ещё {}", options.len() - 40) } else { String::new() };
            console.ok(format!("  {}{more}", shown.join("  ")));
        }
    }
}

/// Выполнить команды: свои — здесь, остальные отправить серверу.
#[allow(clippy::too_many_arguments)]
fn execute_commands(
    mut console: ResMut<Console>,
    mut view: ResMut<DevView>,
    mut client: Option<ResMut<RenetClient>>,
    player: Query<(&LocalPlayer, &Transform)>,
    mut clipboard: Option<ResMut<Clipboard>>,
    mut wireframe: Option<ResMut<WireframeConfig>>,
    mut exit: MessageWriter<AppExit>,
) {
    let queue = std::mem::take(&mut console.queue);
    for line in queue {
        let args = split_args(&line);
        let Some(cmd) = args.first().map(|s| s.to_lowercase()) else { continue };
        if cmd != "bug" || console.open {
            console.print(format!("> {line}"), COLOR_ECHO);
        }
        let me = player.single().ok().map(|(p, t)| (t.translation, p.yaw, p.pitch));
        match cmd.as_str() {
            "help" | "?" | "помощь" => {
                for (c, h) in COMMANDS {
                    console.ok(format!("  {c} — {h}"));
                }
            }
            "clear" | "cls" => {
                console.lines.clear();
                console.dirty = true;
            }
            "quit" | "exit" => {
                exit.write(AppExit::Success);
            }
            "pos" | "getpos" => {
                let Some((pos, yaw, pitch)) = me else {
                    console.err("Персонажа ещё нет.");
                    continue;
                };
                let tp = tp_line(pos, yaw, pitch);
                let place = nearest_place(pos);
                console.ok(format!("{tp}   ({place})"));
                if copy(clipboard.as_deref_mut(), &tp) {
                    console.ok("Скопировано в буфер обмена — можно вставить в сообщение.");
                }
            }
            "places" => {
                let filter = args.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
                let list: Vec<String> = crate::level::places()
                    .iter()
                    .filter(|p| p.id.contains(&filter) || p.name.to_lowercase().contains(&filter))
                    .map(|p| format!("  {} — {}", p.id, p.name))
                    .collect();
                if list.is_empty() {
                    console.err("Мест не найдено.");
                }
                for l in list {
                    console.ok(l);
                }
            }
            "items" => {
                let q = args[1..].join(" ").to_lowercase();
                let found: Vec<&str> = crate::items::ITEM_BASES.iter().map(|b| b.name).filter(|n| n.to_lowercase().contains(&q)).collect();
                if found.is_empty() {
                    console.err("Ничего не нашлось.");
                } else {
                    let shown = found.iter().take(60).copied().collect::<Vec<_>>().join(", ");
                    let more = if found.len() > 60 { format!(" … ещё {}", found.len() - 60) } else { String::new() };
                    console.ok(format!("Предметов: {}. {shown}{more}", found.len()));
                }
            }
            "stats" | "fps" | "net_graph" => {
                view.stats = parse_toggle(args.get(1).map(String::as_str), view.stats).unwrap_or(!view.stats);
                console.ok(format!("Статистика: {}", if view.stats { "вкл" } else { "выкл" }));
            }
            "wireframe" | "wire" => match wireframe.as_deref_mut() {
                Some(w) => {
                    w.global = parse_toggle(args.get(1).map(String::as_str), w.global).unwrap_or(!w.global);
                    console.ok(format!("Каркас: {}", if w.global { "вкл" } else { "выкл" }));
                }
                None => console.err("Видеокарта не умеет рисовать каркас."),
            },
            "screenshot" | "shot" | "bug" => {
                let Some((pos, yaw, pitch)) = me else {
                    console.err("Персонажа ещё нет.");
                    continue;
                };
                let note = args[1..].join(" ");
                let stamp = timestamp();
                let prefix = if cmd == "bug" { "bug" } else { "shot" };
                let name = format!("{prefix}_{stamp}_x{:.0}_y{:.0}_z{:.0}.png", pos.x, pos.y, pos.z);
                let path = PathBuf::from("screenshots").join(&name);
                if let Err(e) = std::fs::create_dir_all("screenshots") {
                    console.err(format!("Не создать папку screenshots: {e}"));
                    continue;
                }
                let tp = tp_line(pos, yaw, pitch);
                let record = (cmd == "bug").then(|| format!("{stamp}\t{name}\t{tp}\t{}\t{note}", nearest_place(pos)));
                if cmd == "bug" {
                    copy(clipboard.as_deref_mut(), &tp);
                }
                let dir = std::env::current_dir().map(|d| d.join("screenshots").display().to_string()).unwrap_or_else(|_| "screenshots".into());
                console.ok(format!("Снимок: {dir}{}{name}", std::path::MAIN_SEPARATOR));
                if cmd == "bug" {
                    console.ok(format!("Записано в bugs.txt, в буфере: {tp}"));
                }
                // Прячем консоль на пару кадров, чтобы её не было на снимке.
                view.shot = Some((path, 3, record));
            }
            _ => {
                let Some(client) = client.as_deref_mut().filter(|c| c.is_connected()) else {
                    console.err("Нет связи с сервером.");
                    continue;
                };
                let msg = ConsoleClientMessage::Command { line: line.clone(), token: host_token() };
                client.send_message(CH_CONSOLE, encode(&msg));
            }
        }
    }
}

/// Команды при входе в игру: файл `autoexec.cfg` рядом с игрой (по строке на команду, `#` —
/// комментарий) и переменная `MRPG_CONSOLE="fly; tp agora"`. `MRPG_CONSOLE_OPEN=1` — открыть консоль.
fn autoexec(
    mut console: ResMut<Console>,
    state: Res<crate::client::NetState>,
    player: Query<(), With<LocalPlayer>>,
    time: Res<Time>,
    mut since: Local<Option<f32>>,
    mut done: Local<bool>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if *done || state.my_id.is_none() || player.is_empty() {
        return;
    }
    // Даём серверу секунду принять игрока.
    let start = *since.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - start < 1.0 {
        return;
    }
    *done = true;
    let mut lines: Vec<String> = Vec::new();
    if let Ok(text) = std::fs::read_to_string("autoexec.cfg") {
        lines.extend(text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("//")).map(String::from));
    }
    if let Ok(env) = std::env::var("MRPG_CONSOLE") {
        lines.extend(env.split(';').map(str::trim).filter(|l| !l.is_empty()).map(String::from));
    }
    console.queue.extend(lines);
    if std::env::var("MRPG_CONSOLE_OPEN").is_ok() {
        set_open(&mut console, &mut cursor, true);
    }
}

fn copy(clipboard: Option<&mut Clipboard>, text: &str) -> bool {
    clipboard.is_some_and(|c| c.set_text(text.to_string()).is_ok())
}

fn tp_line(pos: Vec3, yaw: f32, pitch: f32) -> String {
    format!("tp {:.2} {:.2} {:.2} {:.2} {:.2}", pos.x, pos.y, pos.z, yaw, pitch)
}

/// Ближайшее место уровня (для записи о баге).
fn nearest_place(pos: Vec3) -> String {
    crate::level::places()
        .iter()
        .map(|p| (p, p.pos.distance(pos)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, d)| format!("{} — {} м", p.name, d.round()))
        .unwrap_or_else(|| "вне мест".into())
}

/// Дата и время (UTC) для имени файла: 2026-11-23_18-40-05.
fn timestamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Алгоритм Хиннанта: дни от 1970-01-01 -> гражданская дата.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

fn receive_replies(
    mut client: Option<ResMut<RenetClient>>,
    mut console: ResMut<Console>,
    mut dev: ResMut<DevMove>,
    mut player: Query<(&mut LocalPlayer, &mut Transform)>,
) {
    let Some(client) = client.as_deref_mut() else { return };
    while let Some(bytes) = client.receive_message(CH_CONSOLE) {
        let Some(msg) = decode::<ConsoleServerMessage>(&bytes) else { continue };
        match msg {
            ConsoleServerMessage::Reply { text, error } => console.print(text, if error { COLOR_ERROR } else { COLOR_SERVER }),
            ConsoleServerMessage::Cheats(c) => dev.cheats = c,
            ConsoleServerMessage::Look { yaw, pitch } => {
                for (mut p, mut t) in &mut player {
                    p.yaw = yaw;
                    p.pitch = pitch;
                    t.rotation = Quat::from_rotation_y(yaw);
                }
            }
        }
    }
}

fn take_shots(mut commands: Commands, mut view: ResMut<DevView>, mut console: ResMut<Console>) {
    let Some((path, frames, record)) = view.shot.as_mut() else { return };
    if *frames > 0 {
        *frames -= 1;
        if *frames == 0 {
            use bevy::render::view::screenshot::{Screenshot, save_to_disk};
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
            if let Some(record) = record.take() {
                use std::io::Write;
                let file = std::fs::OpenOptions::new().create(true).append(true).open("screenshots/bugs.txt");
                if let Err(e) = file.and_then(|mut f| writeln!(f, "{record}")) {
                    console.err(format!("Не записать bugs.txt: {e}"));
                }
            }
            view.shot = None;
        }
    }
    console.dirty = true;
}

fn redraw_console(
    mut commands: Commands,
    mut console: ResMut<Console>,
    view: Res<DevView>,
    fonts: Option<Res<crate::inventory_ui::UiFonts>>,
    time: Res<Time>,
    mut root: Query<&mut Visibility, With<ConsoleRoot>>,
    log: Query<Entity, With<ConsoleLog>>,
    mut input: Query<&mut Text, With<ConsoleInputText>>,
) {
    let Ok(mut vis) = root.single_mut() else { return };
    // Мигающий курсор.
    let caret = if console.open && (time.elapsed_secs() * 2.0) as u32 % 2 == 0 { "_" } else { " " };
    if let Ok(mut t) = input.single_mut() {
        let s = format!("> {}{caret}", console.input);
        if t.0 != s {
            t.0 = s;
        }
    }
    let shown = console.open && view.shot.is_none();
    vis.set_if_neq(if shown { Visibility::Visible } else { Visibility::Hidden });
    if !console.dirty {
        return;
    }
    console.dirty = false;
    let (Ok(log), Some(fonts)) = (log.single(), fonts) else { return };
    let end = console.lines.len().saturating_sub(console.scroll);
    let start = end.saturating_sub(VISIBLE_LINES);
    commands.entity(log).despawn_children();
    let lines: Vec<(String, Color)> = console.lines.range(start..end).cloned().collect();
    commands.entity(log).with_children(|p| {
        for (text, color) in lines {
            p.spawn((Text::new(text), TextFont { font: fonts.regular.clone().into(), font_size: FontSize::Px(15.0), ..default() }, TextColor(color)));
        }
    });
}

fn update_stats(
    view: Res<DevView>,
    dev: Res<DevMove>,
    diagnostics: Res<DiagnosticsStore>,
    player: Query<(&LocalPlayer, &Transform)>,
    entities: Query<Entity>,
    mut text: Query<(&mut Text, &mut Visibility), With<StatsText>>,
) {
    let Ok((mut text, mut vis)) = text.single_mut() else { return };
    vis.set_if_neq(if view.stats { Visibility::Visible } else { Visibility::Hidden });
    if !view.stats {
        return;
    }
    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let ms = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let mut s = format!("FPS {fps:.0}  ({ms:.1} мс)\nОбъектов: {}", entities.iter().count());
    if let Ok((p, t)) = player.single() {
        let v = t.translation;
        s += &format!("\n{:.1} {:.1} {:.1}  взгляд {:.2} {:.2}\n{}", v.x, v.y, v.z, p.yaw, p.pitch, nearest_place(v));
    }
    let c = dev.cheats;
    let mut on = Vec::new();
    if c.noclip {
        on.push("noclip".to_string());
    } else if c.fly {
        on.push("fly".into());
    }
    if c.god {
        on.push("god".into());
    }
    if c.notarget {
        on.push("notarget".into());
    }
    if (c.speed - 1.0).abs() > 0.01 {
        on.push(format!("speed ×{}", c.speed));
    }
    if !on.is_empty() {
        s += &format!("\nЧиты: {}", on.join(", "));
    }
    text.0 = s;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_looks_like_a_date() {
        let s = timestamp();
        assert_eq!(s.len(), 19, "{s}");
        assert!(s.starts_with("20"), "{s}");
    }

    #[test]
    fn tab_completes_commands_and_places() {
        let mut c = Console::default();
        c.input = "nocl".into();
        complete(&mut c);
        assert_eq!(c.input, "noclip ");
        c.input = "give фолл".into();
        complete(&mut c);
        assert_eq!(c.input, "give «Фоллис» ");
    }
}
