//! Клиент: городская библиотека — страницы на пюпитрах, чтение рукописей и разговоры с людьми.
//!
//! - На раскрытые книги и свитки ложатся настоящие страницы (текстуры из
//!   `tools/textures/build_library_textures.py`), над входом — вывеска, в торце зала — мозаика.
//! - Подошёл к пюпитру и нажал E — окно чтения: заглавие, кто написал, как выглядит книга,
//!   греческое начало и перевод по страницам. Кто не знает греческой грамоты, видит только
//!   «закорючки» — но библиофилакс прочтёт вслух.
//! - E у служителя библиотеки — разговор (`library_talk.rs`): приветствие по времени суток и
//!   по тому, в который раз пришёл гость, темы на выбор, книги вслух, спор с ритором.
#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::level::library::{self, Staff};
use crate::library_talk::{self as talk, Ctx, Then, Topic};
use crate::library_texts::TEXTS;
use crate::lore::{Language, Lore};
use crate::player::{LocalPlayer, PlayerCamera};
use crate::ui_theme::{AppearChildren, Skin, ink, theme};
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use std::collections::HashMap;

pub struct LibraryClientPlugin;

impl Plugin for LibraryClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LibraryUi::default())
            .insert_resource(LecternAim::default())
            .add_systems(Startup, (spawn_pages, setup_library_ui.after(crate::inventory_ui::setup_fonts)))
            .add_systems(
                Update,
                (aim_lectern, library_input, library_clicks, rebuild_library, library_prompt, library_barks)
                    .chain()
                    .after(crate::life_client::LifeSystems)
                    .before(crate::doors::DoorSystems),
            )
            .add_systems(Update, shot_library.run_if(crate::settings::env_flag("MRPG_SHOT_UI")));
    }
}

// ---------------------------------------------------------------------------
// Состояние
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Closed,
    /// Читает рукопись `text`, страница `page`; `aloud` — кто читает вслух.
    Read { text: usize, page: usize, aloud: Option<Staff> },
    /// Разговор с жителем `resident` (служитель `staff`).
    Talk { staff: Staff, resident: usize, choice: Choice },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Topics,
    Books,
    Side,
}

/// Окно библиотеки и память разговоров (на этот запуск игры).
#[derive(Resource)]
pub struct LibraryUi {
    mode: Mode,
    /// Последняя реплика собеседника.
    line: String,
    /// Где стоял игрок, когда открыл (отошёл — окно закрывается).
    at: Option<Vec3>,
    /// Окно открыто в этом кадре (то же E не должно его закрыть).
    fresh: bool,
    rev: u64,
    built: u64,
    visits: HashMap<Staff, u32>,
    /// За кого гость в споре о Гомере (true — за Гомера).
    side: Option<bool>,
    /// Номер жителя → служитель (по раскладке уровня; считается при первом обращении).
    staff: Option<Vec<(usize, Staff)>>,
    /// Последняя «пара слов» при приближении: (кто, когда, текст).
    bark: Option<(Staff, f32, String)>,
}

impl Default for LibraryUi {
    fn default() -> Self {
        Self { mode: Mode::Closed, line: String::new(), at: None, fresh: false, rev: 1, built: 0, visits: HashMap::new(), side: None, staff: None, bark: None }
    }
}

impl LibraryUi {
    pub fn open(&self) -> bool {
        self.mode != Mode::Closed
    }

    fn staff_list(&mut self) -> &[(usize, Staff)] {
        self.staff.get_or_insert_with(|| library::staff_of(&crate::level::shared().residents))
    }

    /// Служитель библиотеки ли это (номер жителя).
    pub fn staff_of(&mut self, resident: usize) -> Option<Staff> {
        self.staff_list().iter().find(|(i, _)| *i == resident).map(|(_, s)| *s)
    }

    /// E у жителя: если это служитель библиотеки — открыть разговор и вернуть `true`.
    pub fn talk_to(&mut self, resident: usize, ctx: Ctx) -> bool {
        let Some(staff) = self.staff_of(resident) else { return false };
        let visits = self.visits.get(&staff).copied().unwrap_or(0);
        let ctx = Ctx { visits, side: self.side, ..ctx };
        self.line = talk::greeting(staff, &ctx);
        self.visits.insert(staff, visits + 1);
        self.mode = Mode::Talk { staff, resident, choice: Choice::Topics };
        self.at = None;
        self.fresh = true;
        self.rev += 1;
        true
    }

    fn read(&mut self, text: usize, aloud: Option<Staff>) {
        self.mode = Mode::Read { text, page: 0, aloud };
        self.fresh = true;
        self.rev += 1;
    }

    fn close(&mut self) {
        self.mode = Mode::Closed;
        self.at = None;
        self.rev += 1;
    }

    /// Подсказка над служителем вместо «[E] Поговорить: Грамматик».
    pub fn prompt_for(&mut self, resident: usize) -> Option<String> {
        let s = self.staff_of(resident)?;
        Some(format!("[E] Поговорить: {} — {}", talk::name(s), talk::title(s)))
    }
}

/// На какую раскрытую книгу смотрит игрок (номер в `library::lecterns()`).
#[derive(Resource, Default)]
struct LecternAim(Option<usize>);

// ---------------------------------------------------------------------------
// Страницы на пюпитрах, вывеска, мозаика
// ---------------------------------------------------------------------------

/// Клетка атласа разворотов: (атлас, uv0, uv1).
fn spread_uv(text: usize) -> (usize, Vec2, Vec2) {
    let k = text % 6;
    let (col, row) = ((k % 2) as f32, (k / 2) as f32);
    let ch = 682.0 / 2048.0;
    (text / 6, Vec2::new(col * 0.5, row * ch), Vec2::new(col * 0.5 + 0.5, row * ch + ch))
}

/// Плоский прямоугольник: середина, полуоси вдоль u и v, uv углов.
fn quad_mesh(c: Vec3, ax_u: Vec3, ax_v: Vec3, uv0: Vec2, uv1: Vec2) -> Mesh {
    let n = ax_u.cross(ax_v).normalize();
    let pos = vec![(c - ax_u - ax_v).to_array(), (c + ax_u - ax_v).to_array(), (c + ax_u + ax_v).to_array(), (c - ax_u + ax_v).to_array()];
    let uv = vec![[uv0.x, uv1.y], [uv1.x, uv1.y], [uv1.x, uv0.y], [uv0.x, uv0.y]];
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![n.to_array(); 4]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    m
}

fn spawn_pages(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, server: Res<AssetServer>) {
    if crate::level::active() != crate::level::LevelKind::Chersonesus {
        return;
    }
    let mat = |materials: &mut Assets<StandardMaterial>, path: &'static str, rough: f32| {
        materials.add(StandardMaterial { base_color_texture: Some(server.load(path)), perceptual_roughness: rough, reflectance: 0.25, ..default() })
    };
    let atlases = [mat(&mut materials, "textures/library/codices_a.jpg", 0.85), mat(&mut materials, "textures/library/codices_b.jpg", 0.85)];
    for l in library::lecterns() {
        let (right, up, n) = l.axes();
        let (a, uv0, uv1) = spread_uv(l.text);
        // Над листами: геометрия уровня сдвинута по нормали на depth_rank (пергамент — 12 мм).
        let c = l.at + n * 0.016;
        let mesh = meshes.add(quad_mesh(c, right * (l.width / 2.0), up * (l.page_height() / 2.0), uv0, uv1));
        // Листы крошечные (меньше порога «мелочи» в settings) — но это два треугольника, а
        // дальность видимости им ни к чему: без текста книга выглядит пустой.
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(atlases[a].clone()), Transform::default(), NotShadowCaster, crate::settings::NoDetailRange));
    }
    let ins = mat(&mut materials, "textures/library/inscriptions.jpg", 0.55);
    let (mc, mw, mh) = library::mosaic_panel();
    let m = meshes.add(quad_mesh(mc + Vec3::Z * 0.01, Vec3::X * (mw / 2.0), Vec3::Y * (mh / 2.0), Vec2::new(0.0, 0.0), Vec2::new(1.0, 576.0 / 1024.0)));
    commands.spawn((Mesh3d(m), MeshMaterial3d(ins.clone()), Transform::default(), NotShadowCaster));
    let (sc, sw, sh) = library::sign_board();
    let s = meshes.add(quad_mesh(sc + Vec3::Z * 0.012, Vec3::X * (sw / 2.0), Vec3::Y * (sh / 2.0), Vec2::new(0.0, 576.0 / 1024.0), Vec2::new(1.0, 1.0)));
    commands.spawn((Mesh3d(s), MeshMaterial3d(ins), Transform::default(), NotShadowCaster));
}

// ---------------------------------------------------------------------------
// Прицел и ввод
// ---------------------------------------------------------------------------

fn aim_lectern(mut aim: ResMut<LecternAim>, ui: Res<LibraryUi>, camera: Query<&GlobalTransform, With<PlayerCamera>>, residents: Res<crate::residents::ResidentAim>) {
    aim.0 = None;
    if ui.open() || residents.0.is_some() {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let (o, dir) = (cam.translation(), cam.forward().as_vec3());
    if !library::inside(o) {
        return;
    }
    let mut best: Option<(f32, usize)> = None;
    for (i, l) in library::lecterns().iter().enumerate() {
        let to = l.at - o;
        let along = to.dot(dir);
        if !(0.2..=2.4).contains(&along) || (to - dir * along).length() > l.width * 0.55 + 0.1 {
            continue;
        }
        if best.is_none_or(|(d, _)| along < d) {
            best = Some((along, i));
        }
    }
    aim.0 = best.map(|(_, i)| i);
}

#[allow(clippy::too_many_arguments)]
fn library_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<LibraryUi>,
    aim: Res<LecternAim>,
    loot: Res<crate::loot_client::LootState>,
    me: Query<&Transform, With<LocalPlayer>>,
    mut root: Single<(Entity, &mut Node), With<LibraryRoot>>,
    mut commands: Commands,
) {
    let pos = me.iter().next().map(|t| t.translation);
    let fresh = std::mem::take(&mut ui.fresh);
    match ui.mode {
        Mode::Closed => {
            if keys.just_pressed(KeyCode::KeyE)
                && !loot.ui_open
                && let Some(i) = aim.0
            {
                let text = library::lecterns()[i].text;
                ui.read(text, None);
                ui.fresh = false;
            }
        }
        mode => {
            let walked = matches!((ui.at, pos), (Some(a), Some(p)) if a.distance(p) > 3.0);
            let close = keys.just_pressed(KeyCode::Escape) || (!fresh && keys.just_pressed(KeyCode::KeyE)) || walked;
            if close {
                ui.close();
            } else {
                if ui.at.is_none() {
                    ui.at = pos;
                }
                if let Mode::Read { text, page, aloud } = mode {
                    let n = TEXTS[text].pages.len();
                    let next = keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD);
                    let prev = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA);
                    let p = if next { (page + 1).min(n - 1) } else if prev { page.saturating_sub(1) } else { page };
                    if p != page {
                        ui.mode = Mode::Read { text, page: p, aloud };
                        ui.rev += 1;
                    }
                }
            }
        }
    }
    let open = ui.open();
    let (entity, node) = &mut *root;
    let want = if open { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
        if open {
            commands.entity(*entity).insert(AppearChildren);
        }
    }
}

/// Пара слов служителя, когда гость подходит близко (раз в полминуты, не чаще).
fn library_barks(
    time: Res<Time>,
    clock: Res<crate::world_clock::WorldClock>,
    mut ui: ResMut<LibraryUi>,
    me: Query<&Transform, With<LocalPlayer>>,
    people: Query<(&crate::residents::ResidentView, &Transform)>,
    mut next: Local<f32>,
    mut k: Local<u32>,
) {
    let now = time.elapsed_secs();
    if ui.open() || now < *next {
        return;
    }
    let Some(p) = me.iter().next().map(|t| t.translation) else { return };
    if !library::inside(p) {
        return;
    }
    *next = now + 1.0;
    for (v, t) in &people {
        if t.translation.distance(p) > 3.2 {
            continue;
        }
        let Some(s) = ui.staff_of(v.index) else { continue };
        if ui.bark.as_ref().is_some_and(|(b, at, _)| *b == s && now - at < 40.0) {
            continue;
        }
        *k += 1;
        if let Some(line) = talk::bark(s, clock.hour(), *k) {
            ui.bark = Some((s, now, format!("{}: «{line}»", talk::name(s))));
            *next = now + 25.0;
        }
        return;
    }
}

// ---------------------------------------------------------------------------
// Окно
// ---------------------------------------------------------------------------

#[derive(Component)]
struct LibraryRoot;

#[derive(Component)]
struct LibraryBody;

#[derive(Component)]
struct LibraryPrompt;

#[derive(Component, Clone, Copy)]
enum LibButton {
    Prev,
    Next,
    Close,
    Topic(Topic),
    Book(usize),
    Side(bool),
    Back,
}

fn setup_library_ui(mut commands: Commands) {
    let theme = theme();
    commands
        .spawn((
            LibraryRoot,
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
                LibraryBody,
                Node {
                    width: px(760),
                    max_width: percent(94),
                    max_height: percent(92),
                    flex_direction: FlexDirection::Column,
                    padding: Skin::Parchment.padding(),
                    row_gap: px(6),
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
    // Подсказка «[E] Читать» и пара слов служителей — кожаная плашка пониже середины экрана.
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), top: percent(66), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            theme.plaque(p).with_children(|p| {
                p.spawn((LibraryPrompt, theme.text("", 18.0, ink::TEXT), TextLayout::justify(Justify::Center), TextShadow::default(), crate::ui_theme::Plaque));
            });
        });
}

fn library_prompt(time: Res<Time>, aim: Res<LecternAim>, ui: Res<LibraryUi>, know: Option<Res<crate::lore_client::MyKnowledge>>, mut text: Single<&mut Text, With<LibraryPrompt>>) {
    let line = if ui.open() {
        String::new()
    } else if let Some(i) = aim.0 {
        let l = &library::lecterns()[i];
        let t = &TEXTS[l.text];
        let can = know.as_ref().is_none_or(|k| can_read(&k.0, l.text));
        let what = if l.scroll { "свиток" } else { "книга" };
        if can { format!("[E] Читать: {} ({what})", t.title) } else { format!("[E] Посмотреть: {} ({what})", t.title) }
    } else if let Some((_, at, b)) = &ui.bark {
        if time.elapsed_secs() - at < 4.5 { b.clone() } else { String::new() }
    } else {
        String::new()
    };
    if text.0 != line {
        text.0 = line;
    }
}

/// Может ли гость прочесть эту рукопись сам.
fn can_read(k: &crate::lore::Knowledge, text: usize) -> bool {
    let lang = TEXTS[text].language;
    // Псалтирь — первая книга, по ней читают по слогам, зная одни буквы.
    k.knows(Lore::Read(lang)) || (text == 7 && lang == Language::Greek && k.knows(Lore::Alphabet))
}

#[allow(clippy::too_many_arguments)]
fn library_clicks(
    buttons: Query<(&Interaction, &LibButton), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<LibraryUi>,
    clock: Res<crate::world_clock::WorldClock>,
    know: Option<Res<crate::lore_client::MyKnowledge>>,
) {
    let mut pressed: Option<LibButton> = buttons.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| *b);
    // Цифры — темы разговора по порядку.
    if pressed.is_none()
        && let Mode::Talk { staff, choice: Choice::Topics, .. } = ui.mode
    {
        let ctx = Ctx { hour: clock.hour(), literate: know.as_ref().is_some_and(|k| k.0.knows(Lore::Read(Language::Greek))), side: ui.side, ..Default::default() };
        let digits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];
        for (k, (topic, _)) in talk::topics(staff, &ctx).into_iter().enumerate().take(9) {
            if keys.just_pressed(digits[k]) {
                pressed = Some(LibButton::Topic(topic));
            }
        }
    }
    let Some(b) = pressed else { return };
    match (b, ui.mode) {
        (LibButton::Close, _) => ui.close(),
        (LibButton::Prev | LibButton::Next, Mode::Read { text, page, aloud }) => {
            let n = TEXTS[text].pages.len();
            let p = if matches!(b, LibButton::Next) { (page + 1).min(n - 1) } else { page.saturating_sub(1) };
            ui.mode = Mode::Read { text, page: p, aloud };
            ui.rev += 1;
        }
        (LibButton::Back, Mode::Read { aloud: Some(staff), .. }) => {
            // Назад к разговору с тем, кто читал.
            let resident = ui.staff_list().iter().find(|(_, s)| *s == staff).map(|(i, _)| *i).unwrap_or(0);
            ui.mode = Mode::Talk { staff, resident, choice: Choice::Books };
            ui.line = "Что ещё прочесть?".into();
            ui.rev += 1;
        }
        (LibButton::Back, Mode::Talk { staff, resident, .. }) => {
            ui.mode = Mode::Talk { staff, resident, choice: Choice::Topics };
            ui.rev += 1;
        }
        (LibButton::Topic(t), Mode::Talk { staff, resident, .. }) => {
            let visits = ui.visits.get(&staff).copied().unwrap_or(1).saturating_sub(1);
            let ctx = Ctx { hour: clock.hour(), literate: know.as_ref().is_some_and(|k| k.0.knows(Lore::Read(Language::Greek))), visits, side: ui.side, ..Default::default() };
            let (line, then) = talk::answer(staff, t, &ctx);
            ui.line = line;
            ui.rev += 1;
            match then {
                Then::Close => ui.close(),
                Then::ChooseBook => ui.mode = Mode::Talk { staff, resident, choice: Choice::Books },
                Then::ChooseSide => ui.mode = Mode::Talk { staff, resident, choice: Choice::Side },
                Then::OpenBook(i) => ui.read(i, Some(staff)),
                Then::Nothing => ui.mode = Mode::Talk { staff, resident, choice: Choice::Topics },
            }
        }
        (LibButton::Book(i), Mode::Talk { staff, .. }) => ui.read(i, Some(staff)),
        (LibButton::Side(homer), Mode::Talk { staff, resident, .. }) => {
            ui.side = Some(homer);
            let (line, _) = talk::answer(staff, if homer { Topic::SideHomer } else { Topic::SidePsalms }, &Ctx::default());
            ui.line = line;
            ui.mode = Mode::Talk { staff, resident, choice: Choice::Topics };
            ui.rev += 1;
        }
        _ => {}
    }
    ui.fresh = false;
}

fn rebuild_library(
    mut ui: ResMut<LibraryUi>,
    body: Single<(Entity, Option<&Children>), With<LibraryBody>>,
    clock: Res<crate::world_clock::WorldClock>,
    know: Option<Res<crate::lore_client::MyKnowledge>>,
    acts: Res<crate::life_client::LiveActs>,
    mut commands: Commands,
) {
    if ui.built == ui.rev {
        return;
    }
    ui.built = ui.rev;
    let (body, children) = *body;
    if let Some(ch) = children {
        for c in ch.iter() {
            commands.entity(c).despawn();
        }
    }
    let th = theme();
    let literate = know.as_ref().is_some_and(|k| k.0.knows(Lore::Read(Language::Greek)));
    let mode = ui.mode;
    let line = ui.line.clone();
    let side = ui.side;
    commands.entity(body).with_children(|p| match mode {
        Mode::Closed => {}
        Mode::Read { text, page, aloud } => {
            let t = &TEXTS[text];
            let can = aloud.is_some() || know.as_ref().is_none_or(|k| can_read(&k.0, text));
            p.spawn((th.title_colored(t.title, 28.0, ink::RUBRIC), TextLayout::justify(Justify::Center), Node { align_self: AlignSelf::Center, ..default() }));
            p.spawn((th.text(t.author, 15.0, ink::INK_DIM), TextLayout::justify(Justify::Center), Node { align_self: AlignSelf::Center, ..default() }));
            p.spawn(th.text(t.form, 14.0, ink::INK_DIM));
            th.divider(p);
            p.spawn((th.bold(t.incipit, 18.0, ink::RUBRIC), Node { margin: UiRect::vertical(px(2)), ..default() }));
            if let Some(s) = aloud {
                p.spawn(th.text(format!("Читает вслух {}:", lower_first(talk::name(s))), 14.0, ink::INK_DIM));
            }
            if can {
                p.spawn((th.text(t.pages[page], 18.0, ink::INK), Node { margin: UiRect::vertical(px(4)), ..default() }));
                if aloud.is_some() && page + 1 == t.pages.len() {
                    p.spawn(th.text(format!("— {}", t.remark), 15.0, ink::INK_DIM));
                }
            } else {
                p.spawn(th.text(crate::lore::CannotRead::Illiterate(t.language).message(), 17.0, ink::INK));
                p.spawn(th.text("Хранитель книг, диакон Евагрий, читает вслух тем, кто не знает букв. Он обычно у своего стола при входе в читальный зал.", 15.0, ink::INK_DIM));
            }
            th.divider(p);
            p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(10), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() }).with_children(|r| {
                let n = t.pages.len();
                if can {
                    th.small_button(r, "‹ Назад", LibButton::Prev, page > 0);
                    r.spawn(th.text(format!("страница {} из {n}", page + 1), 15.0, ink::INK_DIM));
                    th.small_button(r, "Дальше ›", LibButton::Next, page + 1 < n);
                }
                if aloud.is_some() {
                    th.small_button(r, "Другую книгу", LibButton::Back, true);
                }
                th.small_button(r, "Закрыть", LibButton::Close, true);
            });
            p.spawn((th.text("← → листать · E или Esc — закрыть", 12.0, ink::INK_DIM), Node { align_self: AlignSelf::Center, ..default() }));
        }
        Mode::Talk { staff, resident, choice } => {
            p.spawn((th.title_colored(talk::name(staff), 26.0, ink::RUBRIC), Node { align_self: AlignSelf::Center, ..default() }));
            p.spawn((th.text(talk::title(staff), 14.0, ink::INK_DIM), Node { align_self: AlignSelf::Center, ..default() }));
            th.divider(p);
            p.spawn((th.text(line.clone(), 18.0, ink::INK), Node { margin: UiRect::vertical(px(6)), ..default() }));
            th.divider(p);
            match choice {
                Choice::Topics => {
                    let ctx = Ctx { hour: clock.hour(), literate, side, asleep: acts.0.get(resident) == Some(&crate::daily_life::Act::Sleep), ..Default::default() };
                    for (k, (topic, label)) in talk::topics(staff, &ctx).into_iter().enumerate() {
                        let mut b = th.small_button(p, &format!("{}. {label}", k + 1), LibButton::Topic(topic), true);
                        b.insert(Node { padding: UiRect::axes(px(12), px(5)), min_height: px(28), justify_content: JustifyContent::FlexStart, align_items: AlignItems::Center, ..default() });
                    }
                }
                Choice::Books => {
                    p.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), ..default() }).with_children(|r| {
                        for i in talk::aloud_books() {
                            th.small_button(r, TEXTS[i].title, LibButton::Book(i), true);
                        }
                    });
                    th.small_button(p, "Не сейчас", LibButton::Back, true);
                }
                Choice::Side => {
                    th.small_button(p, "1. Гомер: он учит красоте и мужеству", LibButton::Side(true), true);
                    th.small_button(p, "2. Псалмы: они учат правде, а не только красоте", LibButton::Side(false), true);
                }
            }
        }
    });
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Снимок окна для проверки (`MRPG_SHOT_UI=library:read:N`, `library:read:N:aloud`,
/// `library:talk:librarian|rhetor|scribe|boy|binder[:books]`).
fn shot_library(mut ui: ResMut<LibraryUi>, mut done: Local<bool>, clock: Res<crate::world_clock::WorldClock>) {
    if *done {
        return;
    }
    *done = true;
    let want = std::env::var("MRPG_SHOT_UI").unwrap_or_default();
    let Some(want) = want.strip_prefix("library:") else { return };
    let parts: Vec<&str> = want.split(':').collect();
    match parts.as_slice() {
        ["read", n, rest @ ..] => {
            let text = n.parse().unwrap_or(0).min(TEXTS.len() - 1);
            ui.read(text, if rest.first() == Some(&"aloud") { Some(Staff::Librarian) } else { None });
        }
        ["talk", who, rest @ ..] => {
            let staff = match *who {
                "rhetor" => Staff::Rhetor,
                "scribe" => Staff::Calligrapher,
                "boy" => Staff::Parchmenter,
                "binder" => Staff::Binder,
                _ => Staff::Librarian,
            };
            let resident = ui.staff_list().iter().find(|(_, s)| *s == staff).map(|(i, _)| *i).unwrap_or(0);
            let ctx = Ctx { hour: clock.hour(), literate: false, ..Default::default() };
            ui.talk_to(resident, ctx);
            if rest.first() == Some(&"books") {
                ui.mode = Mode::Talk { staff, resident, choice: Choice::Books };
            }
        }
        _ => {}
    }
    ui.fresh = true;
}
