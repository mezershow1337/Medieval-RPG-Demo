//! Карта Херсона и окрестностей на клавишу M, компас вверху экрана и имена товарищей над головой.
//!
//! Карту рисует «писец» в отдельном потоке по геометрии уровня (см. `map_render.rs`): лист
//! пергамента с морем, стенами, кровлями и дорогами. Поверх — подписи мест (из `level::places`),
//! стрелки всех игроков (куда смотрят) с именами и метки: ПКМ по карте ставит свою метку,
//! её видят все товарищи — на карте, на компасе и в мире.
//!
//! Сеть — свой канал [`CH_MAP`]: клиент называет себя (`Hello`) и ставит/снимает метку,
//! сервер рассылает список имён и меток. Позиции игроков уже приходят в снимках мира.

use crate::client::{NetState, RemoteActor};
use crate::map_render::{self, MapBounds, MapRaster};
use crate::player::{LocalPlayer, PlayerCamera};
use crate::protocol::{ActorKind, decode, encode};
use crate::ui_theme::{AppearChildren, Skin, UiTheme, ink};
use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::FontSize;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::{RenetClient, RenetServer, RenetServerEvent, renet::ServerEvent};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

/// Канал renet для карты (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_MAP: u8 = 12;

/// Сторона растра карты в пикселях.
const MAP_PX: usize = 2048;
/// Насколько можно приблизить карту (1 — весь лист в окне).
const MAX_ZOOM: f32 = 7.0;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MapClientMessage {
    /// Как меня зовут (один раз после подключения).
    Hello { name: String },
    /// Поставить метку (мировые X, Z) или снять свою.
    SetMark(Option<[f32; 2]>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MapServerMessage {
    /// Все подключённые игроки и их имена.
    Roster(Vec<(u64, String)>),
    /// Все метки на карте: чья и где.
    Marks(Vec<(u64, [f32; 2])>),
}

// ---------------------------------------------------------------------------
// Сервер
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct MapWorld {
    names: BTreeMap<u64, String>,
    marks: BTreeMap<u64, [f32; 2]>,
}

impl MapWorld {
    fn roster(&self) -> MapServerMessage {
        MapServerMessage::Roster(self.names.iter().map(|(id, n)| (*id, n.clone())).collect())
    }
    fn marks(&self) -> MapServerMessage {
        MapServerMessage::Marks(self.marks.iter().map(|(id, p)| (*id, *p)).collect())
    }
    /// Имя без повторов: второй «Иван» станет «Иван 2».
    fn unique_name(&self, id: u64, wanted: &str) -> String {
        let base = match crate::launcher::clean_name(wanted) {
            s if s.is_empty() => "Странник".to_string(),
            s => s,
        };
        let taken = |n: &str| self.names.iter().any(|(other, name)| *other != id && name == n);
        if !taken(&base) {
            return base;
        }
        (2..).map(|k| format!("{base} {k}")).find(|n| !taken(n)).unwrap()
    }
}

pub struct MapServerPlugin;

impl Plugin for MapServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MapWorld::default()).add_observer(on_connection).add_systems(Update, receive_map_messages);
    }
}

fn on_connection(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, mut world: ResMut<MapWorld>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            server.send_message(*client_id, CH_MAP, encode(&world.roster()));
            server.send_message(*client_id, CH_MAP, encode(&world.marks()));
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            world.names.remove(client_id);
            let had_mark = world.marks.remove(client_id).is_some();
            server.broadcast_message(CH_MAP, encode(&world.roster()));
            if had_mark {
                server.broadcast_message(CH_MAP, encode(&world.marks()));
            }
        }
    }
}

fn receive_map_messages(mut server: ResMut<RenetServer>, mut world: ResMut<MapWorld>) {
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_MAP) {
            match decode::<MapClientMessage>(&bytes) {
                Some(MapClientMessage::Hello { name }) => {
                    let name = world.unique_name(pid, &name);
                    world.names.insert(pid, name);
                    server.broadcast_message(CH_MAP, encode(&world.roster()));
                }
                Some(MapClientMessage::SetMark(mark)) => {
                    match mark.filter(|p| p.iter().all(|v| v.is_finite() && v.abs() < 5000.0)) {
                        Some(p) => world.marks.insert(pid, p),
                        None => world.marks.remove(&pid),
                    };
                    server.broadcast_message(CH_MAP, encode(&world.marks()));
                }
                None => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Клиент: товарищи и их метки
// ---------------------------------------------------------------------------

/// Краски для игроков — пигменты, что были у переписчиков VI века.
const PIGMENTS: [(f32, f32, f32); 6] = [
    (0.78, 0.16, 0.10), // киноварь
    (0.16, 0.32, 0.72), // ляпис-лазурь
    (0.10, 0.52, 0.36), // ярь-медянка
    (0.86, 0.60, 0.08), // шафран
    (0.52, 0.13, 0.44), // пурпур
    (0.35, 0.22, 0.10), // умбра
];

/// Кто в игре и где их метки (по данным сервера).
#[derive(Resource, Default)]
pub struct Party {
    names: BTreeMap<u64, String>,
    marks: BTreeMap<u64, Vec2>,
}

impl Party {
    pub fn name(&self, id: u64) -> String {
        self.names.get(&id).cloned().unwrap_or_else(|| "Странник".to_string())
    }
    /// Цвет игрока одинаков у всех: по порядку в списке сервера.
    pub fn color(&self, id: u64) -> Color {
        let k = self.names.keys().position(|k| *k == id).unwrap_or((id % PIGMENTS.len() as u64) as usize);
        let (r, g, b) = PIGMENTS[k % PIGMENTS.len()];
        Color::srgb(r, g, b)
    }
}

/// Готовая карта (или ещё рисуется).
#[derive(Resource)]
pub struct MapSheet {
    pending: Arc<Mutex<Option<MapRaster>>>,
    pub image: Option<Handle<Image>>,
    pub bounds: Option<MapBounds>,
}

/// Окно карты: открыто ли, масштаб, куда смотрим.
#[derive(Resource)]
pub struct MapView {
    pub open: bool,
    zoom: f32,
    /// Центр окна — доля листа (0..1).
    center: Vec2,
    drag: Option<(Vec2, Vec2)>,
    /// Сторона окна карты в логических пикселях (считается от размера окна игры).
    side: f32,
    /// Где на карте стоят значки игроков и меток с подписями: подписи мест их не перекрывают.
    occupied: Vec<Rect>,
    /// Центр листа на экране (логические пиксели) — по прошлому кадру раскладки.
    screen_center: Vec2,
}

impl Default for MapView {
    fn default() -> Self {
        Self { open: false, zoom: 2.4, center: Vec2::splat(0.5), drag: None, side: 600.0, occupied: Vec::new(), screen_center: Vec2::ZERO }
    }
}

#[derive(Resource)]
pub(crate) struct MarkerArt {
    pub arrow: Handle<Image>,
    pub pin: Handle<Image>,
}

pub struct MapClientPlugin;

impl Plugin for MapClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Party::default())
            .insert_resource(MapView::default())
            .insert_resource(MarkerPool::default())
            .add_systems(Startup, (start_drawing_map, (setup_marker_art, setup_map_ui, setup_compass).chain().after(crate::inventory_ui::setup_fonts)))
            .add_systems(Update, (say_hello, receive_map).run_if(bevy_renet::client_connected))
            .add_systems(Update, finish_map_sheet)
            .add_systems(Update, (toggle_map, free_cursor_while_open, map_input, update_map_markers, layout_map).chain())
            .add_systems(Update, (update_compass, update_name_tags));
    }
}

fn start_drawing_map(mut commands: Commands) {
    let pending: Arc<Mutex<Option<MapRaster>>> = Arc::default();
    let out = pending.clone();
    // Растр рисуется ~секунду-две: не задерживаем запуск игры.
    std::thread::Builder::new()
        .name("map".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let layout = crate::level::layout();
            let bounds = map_render::bounds_for(&layout, &map_points());
            let raster = map_render::render(&layout, bounds, MAP_PX);
            *out.lock().unwrap() = Some(raster);
        })
        .expect("поток карты");
    commands.insert_resource(MapSheet { pending, image: None, bounds: None });
}

/// Места, которые обязательно должны попасть на лист.
fn map_points() -> Vec<Vec3> {
    crate::level::places().iter().map(|p| p.pos).collect()
}

fn finish_map_sheet(mut sheet: ResMut<MapSheet>, mut images: ResMut<Assets<Image>>, mut view: ResMut<MapView>) {
    if sheet.image.is_some() {
        return;
    }
    let Some(raster) = sheet.pending.lock().unwrap().take() else { return };
    let n = raster.size as u32;
    let mut img = Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, raster.rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    crate::textures::generate_mips(&mut img, false);
    img.sampler = crate::textures::quality_sampler(false);
    sheet.image = Some(images.add(img));
    sheet.bounds = Some(raster.bounds);
    view.center = raster.bounds.to_uv(crate::level::spawn_point(0).x, crate::level::spawn_point(0).z);
}

fn say_hello(mut client: ResMut<RenetClient>, mut sent: Local<bool>) {
    if !*sent {
        *sent = true;
        let name = crate::launcher::player_name();
        client.send_message(CH_MAP, encode(&MapClientMessage::Hello { name }));
        // Для снимков: `MRPG_MARK="x,z"` сразу ставит метку.
        if let Some(v) = std::env::var("MRPG_MARK").ok().map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>())
            && v.len() == 2
        {
            client.send_message(CH_MAP, encode(&MapClientMessage::SetMark(Some([v[0], v[1]]))));
        }
    }
}

fn receive_map(mut client: ResMut<RenetClient>, mut party: ResMut<Party>) {
    while let Some(bytes) = client.receive_message(CH_MAP) {
        match decode::<MapServerMessage>(&bytes) {
            Some(MapServerMessage::Roster(list)) => party.names = list.into_iter().collect(),
            Some(MapServerMessage::Marks(list)) => party.marks = list.into_iter().map(|(id, p)| (id, Vec2::from_array(p))).collect(),
            None => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Картинки меток
// ---------------------------------------------------------------------------

/// Белая фигура с тёмным контуром (цвет игрока задаётся оттенком `ImageNode`).
fn stamp(n: u32, inside: impl Fn(f32, f32) -> f32) -> Image {
    let mut data = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            // Сглаживание: 4×4 подвыборки.
            let (mut fill, mut edge) = (0.0, 0.0);
            for sy in 0..4 {
                for sx in 0..4 {
                    let u = (x as f32 + (sx as f32 + 0.5) / 4.0) / n as f32 * 2.0 - 1.0;
                    let v = (y as f32 + (sy as f32 + 0.5) / 4.0) / n as f32 * 2.0 - 1.0;
                    let d = inside(u, v);
                    if d > 0.11 {
                        fill += 1.0 / 16.0;
                    } else if d > 0.0 {
                        edge += 1.0 / 16.0;
                    }
                }
            }
            let a = fill + edge;
            let c = if a > 0.0 { fill / a } else { 0.0 };
            let o = ((y * n + x) * 4) as usize;
            let ink = [0.13, 0.08, 0.04];
            for k in 0..3 {
                data[o + k] = ((ink[k] + (1.0 - ink[k]) * c) * 255.0) as u8;
            }
            data[o + 3] = (a * 255.0) as u8;
        }
    }
    let mut img = Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    crate::textures::generate_mips(&mut img, false);
    img.sampler = crate::textures::quality_sampler(false);
    img
}

/// «Расстояние внутрь» от края многоугольника (>0 внутри), в долях полуразмера.
fn polygon_depth(pts: &[(f32, f32)], u: f32, v: f32) -> f32 {
    let mut inside = false;
    let mut dmin = f32::MAX;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        if (a.1 > v) != (b.1 > v) && u < (b.0 - a.0) * (v - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let t = (((u - a.0) * ex + (v - a.1) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        let (dx, dy) = (u - a.0 - ex * t, v - a.1 - ey * t);
        dmin = dmin.min((dx * dx + dy * dy).sqrt());
    }
    if inside { dmin } else { -dmin }
}

pub(crate) fn setup_marker_art(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Наконечник стрелы остриём вверх, с вырезом сзади.
    let arrow = stamp(64, |u, v| polygon_depth(&[(0.0, -0.95), (0.72, 0.85), (0.0, 0.42), (-0.72, 0.85)], u, v));
    // Метка: равноконечный крест с расширенными концами (как на печатях и надгробиях того времени).
    let pin = stamp(64, |u, v| {
        let arm = |a: f32, b: f32| {
            // Плечо вдоль оси a, ширина растёт к концу.
            let w = 0.16 + 0.22 * a.abs();
            (w - b.abs()).min(0.92 - a.abs())
        };
        arm(u, v).max(arm(v, u)).max(0.3 - (u * u + v * v).sqrt())
    });
    commands.insert_resource(MarkerArt { arrow: images.add(arrow), pin: images.add(pin) });
}

// ---------------------------------------------------------------------------
// Окно карты
// ---------------------------------------------------------------------------

const INK: Color = ink::INK;
/// Тонкая тёмная кромка листа внутри рамы.
const WOOD_DARK: Color = Color::srgb(0.16, 0.10, 0.05);
const MAP_BORDER: f32 = 3.0;

#[derive(Component)]
struct MapRoot;
#[derive(Component)]
struct MapViewport;
#[derive(Component)]
struct MapImage;
#[derive(Component)]
struct MapLoading;
/// Подпись места на карте: где (доля листа) и с какого приближения видна.
#[derive(Component)]
struct MapLabel {
    uv: Vec2,
    min_zoom: f32,
    half_w: f32,
    /// Примерный размер надписи (для того, чтобы подписи не налезали друг на друга).
    est: Vec2,
    /// Порядок важности: важные места подписываются первыми.
    rank: u32,
}

fn label_tier(kind: crate::level::PlaceKind) -> f32 {
    use crate::level::PlaceKind::*;
    match kind {
        SteppeCamp | GothVillage | AlanCamp | Necropolis | Vineyard | Spring | Mine | CharcoalCamp | Quarry => 0.0,
        Agora | Basilica | Gate | PortMarket | Theatre | Garrison | Praetorium | Thermae | Quay | Cistern => 1.8,
        _ => 3.0,
    }
}

pub(crate) fn text(fonts: &crate::inventory_ui::UiFonts, s: &str, size: f32, color: Color, bold: bool) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn setup_map_ui(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>, theme: Res<UiTheme>) {
    let root = commands
        .spawn((
            MapRoot,
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            theme.backdrop(),
            GlobalZIndex(40),
            Visibility::Hidden,
        ))
        .id();
    // Рама — тёмная доска в бронзе (общая тема окон), внутри — сам лист карты.
    let frame = commands
        .spawn((
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: Skin::Wood.padding(), row_gap: px(8), ..default() },
            theme.skin(Skin::Wood),
            ChildOf(root),
        ))
        .id();
    commands.spawn((theme.title("Херсон и окрестности", 28.0), ChildOf(frame)));
    let viewport = commands
        .spawn((
            MapViewport,
            Node { width: px(600), height: px(600), overflow: Overflow::clip(), border: UiRect::all(px(MAP_BORDER)), ..default() },
            BorderColor::all(WOOD_DARK),
            BackgroundColor(Color::srgb(0.80, 0.71, 0.53)),
            ChildOf(frame),
        ))
        .id();
    commands.spawn((MapImage, ImageNode::default(), Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, ChildOf(viewport)));
    commands.spawn((
        MapLoading,
        text(&fonts, "Писец ещё дорисовывает карту…", 20.0, INK, false),
        Node { position_type: PositionType::Absolute, left: px(150), top: px(280), ..default() },
        ChildOf(viewport),
    ));
    // Подписи: места, город, море.
    let mut labels: Vec<(String, Vec3, f32, f32, bool)> = Vec::new();
    for p in crate::level::places() {
        if p.kind == crate::level::PlaceKind::Hearth {
            continue;
        }
        labels.push((p.name.to_string(), p.pos, label_tier(p.kind), 15.0, false));
    }
    if crate::level::active() == crate::level::LevelKind::Chersonesus {
        labels.push(("Х Е Р С О Н".into(), Vec3::new(0.0, 0.0, 30.0), 0.0, 26.0, true));
        labels.push(("П о н т   Э в к с и н с к и й".into(), Vec3::new(60.0, 0.0, -330.0), 0.0, 24.0, true));
        labels.push(("Гераклейский полуостров".into(), Vec3::new(-40.0, 0.0, 420.0), 0.0, 18.0, true));
    }
    // Крупные надписи — первыми, затем по важности места.
    labels.sort_by(|a, b| (!a.4).cmp(&!b.4).then(a.2.total_cmp(&b.2)));
    for (rank, (name, pos, min_zoom, size, big)) in labels.into_iter().enumerate() {
        let half_w = 160.0;
        let est = Vec2::new(name.chars().count() as f32 * size * 0.5, size * 1.2);
        commands.spawn((
            MapLabel { uv: Vec2::new(pos.x, pos.z), min_zoom, half_w, est, rank: rank as u32 },
            text(&fonts, &name, size, if big { Color::srgba(0.24, 0.15, 0.08, 0.75) } else { INK }, true),
            TextLayout::justify(Justify::Center),
            TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.95, 0.89, 0.74, 0.8) },
            Node { position_type: PositionType::Absolute, width: px(half_w * 2.0), ..default() },
            Visibility::Hidden,
            ChildOf(viewport),
        ));
    }
    commands.spawn((
        theme.text("Колесо — масштаб  ·  ЛКМ — двигать  ·  ПКМ — метка для товарищей  ·  Пробел — к себе  ·  M — закрыть", 15.0, ink::DIM),
        ChildOf(frame),
    ));
}

fn any_other_window_open(loot: &Option<Res<crate::loot_client::LootState>>, trade: &Option<Res<crate::trade_client::TradeState>>) -> bool {
    loot.as_ref().is_some_and(|l| l.ui_open) || trade.as_ref().is_some_and(|t| t.window_open())
}

fn toggle_map(
    mut commands: Commands,
    root: Single<Entity, With<MapRoot>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<MapView>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    loot: Option<Res<crate::loot_client::LootState>>,
    trade: Option<Res<crate::trade_client::TradeState>>,
    sheet: Res<MapSheet>,
    player: Query<&Transform, With<LocalPlayer>>,
    mut shot_opened: Local<bool>,
) {
    let mut m = keys.just_pressed(KeyCode::KeyM);
    // Снимки для проверки: `MRPG_SHOT_UI=map` открывает карту, как только она нарисована и игрок появился
    // (`MRPG_MAP_ZOOM` — приближение).
    if !*shot_opened && sheet.image.is_some() && player.single().is_ok() && std::env::var("MRPG_SHOT_UI").as_deref() == Ok("map") {
        *shot_opened = true;
        m = !view.open;
        if let Some(z) = std::env::var("MRPG_MAP_ZOOM").ok().and_then(|s| s.parse::<f32>().ok()) {
            view.zoom = z.clamp(1.0, MAX_ZOOM);
        }
    }
    if view.open {
        if m || keys.just_pressed(KeyCode::Escape) || any_other_window_open(&loot, &trade) {
            view.open = false;
            view.drag = None;
            if m {
                cursor.grab_mode = CursorGrabMode::Locked;
                cursor.visible = false;
            }
        }
    } else if m && !any_other_window_open(&loot, &trade) {
        view.open = true;
        commands.entity(*root).insert(AppearChildren);
        // Открываем на себе.
        if let (Some(b), Ok(t)) = (sheet.bounds, player.single()) {
            view.center = b.to_uv(t.translation.x, t.translation.z);
        }
    }
}

fn free_cursor_while_open(view: Res<MapView>, mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    if view.open && (cursor.grab_mode != CursorGrabMode::None || !cursor.visible) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Курсор в окне карты: доля листа под ним (или `None`, если мимо карты).
fn cursor_uv(window: &Window, view: &MapView) -> Option<Vec2> {
    let c = window.cursor_position()?;
    let local = c - view.screen_center;
    if local.x.abs() > view.side / 2.0 || local.y.abs() > view.side / 2.0 {
        return None;
    }
    Some(view.center + local / (view.side * view.zoom))
}


#[allow(clippy::too_many_arguments)]
fn map_input(
    mut view: ResMut<MapView>,
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    sheet: Res<MapSheet>,
    party: Res<Party>,
    net: Res<NetState>,
    mut client: Option<ResMut<RenetClient>>,
    player: Query<&Transform, With<LocalPlayer>>,
) {
    if !view.open {
        return;
    }
    let Some(bounds) = sheet.bounds else { return };
    let cur = cursor_uv(&window, &view);
    // Масштаб колесом — вокруг точки под курсором.
    let steps = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 60.0,
    };
    if steps != 0.0 {
        let old = view.zoom;
        view.zoom = (view.zoom * 1.18f32.powf(steps)).clamp(1.0, MAX_ZOOM);
        if let Some(p) = cur {
            view.center = p - (p - view.center) * (old / view.zoom);
        }
    }
    // Перетаскивание.
    if mouse.just_pressed(MouseButton::Left) && cur.is_some() {
        view.drag = window.cursor_position().map(|c| (c, view.center));
    }
    if !mouse.pressed(MouseButton::Left) {
        view.drag = None;
    }
    if let (Some((start, c0)), Some(now)) = (view.drag, window.cursor_position()) {
        view.center = c0 - (now - start) / (view.side * view.zoom);
    }
    if keys.just_pressed(KeyCode::Space)
        && let Ok(t) = player.single()
    {
        view.center = bounds.to_uv(t.translation.x, t.translation.z);
    }
    // Метка: ПКМ по карте; ПКМ по своей метке — снять.
    if mouse.just_pressed(MouseButton::Right)
        && let (Some(p), Some(c)) = (cur, client.as_mut())
    {
        let world = bounds.from_uv(p);
        let mine = net.my_id.and_then(|id| party.marks.get(&id).copied());
        let near_mine = mine.is_some_and(|m| (bounds.to_uv(m.x, m.y) - p).length() * view.side * view.zoom < 14.0);
        let msg = if near_mine { MapClientMessage::SetMark(None) } else { MapClientMessage::SetMark(Some(world.to_array())) };
        c.send_message(CH_MAP, encode(&msg));
    }
    // Не даём уехать за край листа.
    let half = 0.5 / view.zoom;
    view.center = view.center.clamp(Vec2::splat(half), Vec2::splat(1.0 - half));
}

#[allow(clippy::type_complexity)]
fn layout_map(
    mut view: ResMut<MapView>,
    window: Single<&Window, With<PrimaryWindow>>,
    sheet: Res<MapSheet>,
    mut root: Single<&mut Visibility, (With<MapRoot>, Without<MapImage>, Without<MapLabel>, Without<MapLoading>)>,
    mut viewport: Single<(&mut Node, &ComputedNode, &UiGlobalTransform), (With<MapViewport>, Without<MapImage>, Without<MapLabel>, Without<MapLoading>)>,
    mut image: Single<(&mut Node, &mut ImageNode, &mut Visibility), (With<MapImage>, Without<MapViewport>, Without<MapLabel>, Without<MapLoading>)>,
    mut loading: Single<&mut Visibility, (With<MapLoading>, Without<MapRoot>, Without<MapImage>, Without<MapLabel>)>,
    mut labels: Query<(&MapLabel, &mut Node, &mut Visibility), (Without<MapViewport>, Without<MapImage>, Without<MapRoot>, Without<MapLoading>)>,
) {
    **root = if view.open { Visibility::Inherited } else { Visibility::Hidden };
    if !view.open {
        return;
    }
    // Окно карты — квадрат, сколько позволяет экран (место под заголовок и подсказку).
    let side = (window.width() * 0.9 - 70.0).min(window.height() - 190.0).max(240.0).floor();
    view.side = side;
    let (vp_node, vp_computed, vp_tf) = &mut *viewport;
    vp_node.width = px(side + 2.0 * MAP_BORDER);
    vp_node.height = px(side + 2.0 * MAP_BORDER);
    view.screen_center = vp_tf.translation * vp_computed.inverse_scale_factor();
    let (Some(handle), Some(bounds)) = (&sheet.image, sheet.bounds) else {
        **loading = Visibility::Inherited;
        return;
    };
    **loading = Visibility::Hidden;
    let (node, img, vis) = &mut *image;
    if img.image != *handle {
        img.image = handle.clone();
    }
    **vis = Visibility::Inherited;
    let full = side * view.zoom;
    node.width = px(full);
    node.height = px(full);
    node.left = px(side / 2.0 - view.center.x * full);
    node.top = px(side / 2.0 - view.center.y * full);
    // Подписи по важности; та, что налезла бы на уже поставленную, прячется (до приближения).
    let mut placed: Vec<Rect> = view.occupied.clone();
    let mut sorted: Vec<_> = labels.iter_mut().collect();
    sorted.sort_by_key(|(l, ..)| l.rank);
    for (label, mut node, mut vis) in sorted {
        let uv = bounds.to_uv(label.uv.x, label.uv.y);
        let p = (uv - view.center) * full + side / 2.0;
        let rect = Rect::from_center_size(p + Vec2::new(0.0, 6.0 + label.est.y / 2.0), label.est + Vec2::new(8.0, 0.0));
        let free = !placed.iter().any(|r| !r.intersect(rect).is_empty());
        // Обрезанных краем листа подписей не показываем.
        let inside = rect.min.x > 2.0 && rect.min.y > 2.0 && rect.max.x < side - 2.0 && rect.max.y < side - 2.0;
        let show = free && inside && view.zoom >= label.min_zoom;
        if show {
            placed.push(rect);
        }
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
        node.left = px(p.x - label.half_w);
        node.top = px(p.y + 6.0);
    }
}

/// Что рисуем поверх карты, на компасе и над головами — по ключу.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum MarkerKey {
    Player(u64),
    Mark(u64),
}

#[derive(Resource, Default)]
struct MarkerPool {
    map: HashMap<MarkerKey, Entity>,
    compass: HashMap<MarkerKey, Entity>,
    tags: HashMap<MarkerKey, Entity>,
}

/// Сводка о товарище: где он, куда смотрит, как зовут, цвет.
pub(crate) struct Mate {
    pub key: MarkerKey,
    pub pos: Vec3,
    /// Поворот вокруг Y (как `LocalPlayer::yaw`); у меток — нет.
    pub yaw: Option<f32>,
    pub label: String,
    pub color: Color,
    pub me: bool,
}

pub(crate) fn yaw_of(t: &Transform) -> f32 {
    let f = t.rotation * Vec3::NEG_Z;
    (-f.x).atan2(-f.z)
}

pub(crate) fn gather(net: &NetState, party: &Party, local: &Query<(&LocalPlayer, &Transform)>, remotes: &Query<(&RemoteActor, &Transform)>, ground: &crate::collision::LevelColliders) -> Vec<Mate> {
    let mut out = Vec::new();
    let my_id = net.my_id.unwrap_or(0);
    if let Ok((p, t)) = local.single() {
        out.push(Mate { key: MarkerKey::Player(my_id), pos: t.translation, yaw: Some(p.yaw), label: "Вы".into(), color: party.color(my_id), me: true });
    }
    for (r, t) in remotes {
        if r.kind == ActorKind::Player {
            out.push(Mate { key: MarkerKey::Player(r.id), pos: t.translation, yaw: Some(yaw_of(t)), label: party.name(r.id), color: party.color(r.id), me: false });
        }
    }
    for (id, p) in &party.marks {
        let y = ground.ground_height(p.x, p.y);
        let y = if y.is_finite() { y } else { 0.0 };
        let label = if *id == my_id { "Ваша метка".to_string() } else { format!("Метка: {}", party.name(*id)) };
        out.push(Mate { key: MarkerKey::Mark(*id), pos: Vec3::new(p.x, y, p.y), yaw: None, label, color: party.color(*id), me: *id == my_id });
    }
    out
}

/// Значок с подписью: картинка в центре, текст справа.
fn spawn_marker(commands: &mut Commands, fonts: &crate::inventory_ui::UiFonts, parent: Entity, art: Handle<Image>, size: f32, font: f32) -> Entity {
    let e = commands
        .spawn((Node { position_type: PositionType::Absolute, width: px(size), height: px(size), ..default() }, ChildOf(parent)))
        .id();
    commands.spawn((ImageNode::new(art), Node { width: percent(100), height: percent(100), ..default() }, UiTransform::default(), ChildOf(e)));
    commands.spawn((
        text(fonts, "", font, INK, true),
        TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.96, 0.9, 0.76, 0.9) },
        Node { position_type: PositionType::Absolute, left: px(size + 2.0), top: px(size / 2.0 - font * 0.7), width: px(220), ..default() },
        ChildOf(e),
    ));
    e
}

#[allow(clippy::too_many_arguments)]
fn update_map_markers(
    mut commands: Commands,
    mut view: ResMut<MapView>,
    sheet: Res<MapSheet>,
    net: Res<NetState>,
    party: Res<Party>,
    art: Res<MarkerArt>,
    fonts: Res<crate::inventory_ui::UiFonts>,
    level: Res<crate::collision::LevelColliders>,
    mut pool: ResMut<MarkerPool>,
    viewport: Single<Entity, With<MapViewport>>,
    local: Query<(&LocalPlayer, &Transform)>,
    remotes: Query<(&RemoteActor, &Transform)>,
    children: Query<&Children>,
    mut nodes: Query<&mut Node>,
    mut icons: Query<(&mut ImageNode, &mut UiTransform)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (Some(bounds), true) = (sheet.bounds, view.open) else {
        for (_, e) in pool.map.drain() {
            commands.entity(e).despawn();
        }
        return;
    };
    let mates = gather(&net, &party, &local, &remotes, &level);
    let full = view.side * view.zoom;
    let mut occupied: Vec<Rect> = Vec::new();
    pool.map.retain(|k, e| {
        let keep = mates.iter().any(|m| m.key == *k);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    // Метки рисуем первыми (под стрелками игроков), себя — последним (поверх всех).
    let mut order: Vec<&Mate> = mates.iter().collect();
    order.sort_by_key(|m| (matches!(m.key, MarkerKey::Player(_)), m.me));
    for m in order {
        let size = if m.yaw.is_some() { 26.0 } else { 22.0 };
        let entity = *pool.map.entry(m.key).or_insert_with(|| {
            let img = if m.yaw.is_some() { art.arrow.clone() } else { art.pin.clone() };
            spawn_marker(&mut commands, &fonts, *viewport, img, size, 15.0)
        });
        // Только что созданный значок обновим в следующем кадре.
        let Ok(kids) = children.get(entity) else { continue };
        let uv = bounds.to_uv(m.pos.x, m.pos.z);
        let p = (uv - view.center) * full + view.side / 2.0;
        if let Ok(mut node) = nodes.get_mut(entity) {
            node.left = px(p.x - size / 2.0);
            node.top = px(p.y - size / 2.0);
        }
        if let Some(&icon) = kids.first()
            && let Ok((mut img, mut tf)) = icons.get_mut(icon)
        {
            img.color = m.color;
            // Стрелка смотрит туда же, куда игрок: вперёд — (−sin yaw, −cos yaw) по X/Z, север вверху.
            tf.rotation = Rot2::radians(-m.yaw.unwrap_or(0.0));
        }
        // Подпись справа от значка; если налезает на подпись соседа (стоят рядом) — ниже.
        let est = Vec2::new(m.label.chars().count() as f32 * 15.0 * 0.55, 18.0);
        let mut dy = 0.0;
        let label_rect = |dy: f32| Rect::from_corners(p + Vec2::new(size / 2.0 + 2.0, -10.0 + dy), p + Vec2::new(size / 2.0 + 2.0 + est.x, -10.0 + dy + est.y));
        while dy < 72.0 && occupied.iter().any(|r| !r.intersect(label_rect(dy)).is_empty()) {
            dy += 17.0;
        }
        occupied.push(Rect::from_center_size(p, Vec2::splat(size)));
        occupied.push(label_rect(dy));
        if let Some(&label) = kids.get(1) {
            if let Ok(mut n) = nodes.get_mut(label) {
                n.top = px(size / 2.0 - 10.0 + dy);
            }
            if let Ok((mut t, mut c)) = texts.get_mut(label) {
                if t.0 != m.label {
                    t.0 = m.label.clone();
                }
                c.0 = m.color.mix(&INK, 0.35);
            }
        }
    }
    view.occupied = occupied;
}

// ---------------------------------------------------------------------------
// Компас
// ---------------------------------------------------------------------------

#[derive(Component)]
struct CompassRoot;
#[derive(Component)]
struct CompassStrip;
/// Деление компаса: направление в градусах от севера по часовой стрелке.
#[derive(Component)]
struct CompassTick {
    deg: f32,
    half_w: f32,
}

const COMPASS_W: f32 = 440.0;
const COMPASS_H: f32 = 30.0;
/// Сколько градусов видно от центра до края.
const COMPASS_SPAN: f32 = 95.0;

fn setup_compass(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>, theme: Res<UiTheme>) {
    let root = commands
        .spawn((CompassRoot, Node { position_type: PositionType::Absolute, top: px(6), width: percent(100), justify_content: JustifyContent::Center, ..default() }, Visibility::Hidden))
        .id();
    // Кожаная плашка темы, внутри — полоса с делениями.
    let plaque = commands.spawn((Node { padding: Skin::Leather.padding(), ..default() }, theme.skin(Skin::Leather), ChildOf(root))).id();
    let strip = commands
        .spawn((CompassStrip, Node { width: px(COMPASS_W), height: px(COMPASS_H), overflow: Overflow::clip(), ..default() }, ChildOf(plaque)))
        .id();
    // Центральная риска (бронза).
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: px(COMPASS_W / 2.0 - 1.0), top: px(0), width: px(2), height: px(6), ..default() },
        BackgroundColor(ink::GOLD),
        ChildOf(strip),
    ));
    let names = [(0.0, "С"), (45.0, "СВ"), (90.0, "В"), (135.0, "ЮВ"), (180.0, "Ю"), (225.0, "ЮЗ"), (270.0, "З"), (315.0, "СЗ")];
    for (deg, name) in names {
        let main = (deg as i32) % 90 == 0;
        let half_w = 20.0;
        let color = if deg == 0.0 { ink::BAD } else if main { ink::TITLE } else { ink::DIM };
        commands.spawn((
            CompassTick { deg, half_w },
            text(&fonts, name, if main { 19.0 } else { 14.0 }, color, main),
            TextLayout::justify(Justify::Center),
            Node { position_type: PositionType::Absolute, width: px(half_w * 2.0), top: px(if main { 3.0 } else { 6.0 }), ..default() },
            ChildOf(strip),
        ));
    }
    for k in 0..24 {
        let deg = k as f32 * 15.0;
        if (deg as i32) % 45 == 0 {
            continue;
        }
        commands.spawn((
            CompassTick { deg, half_w: 0.5 },
            Node { position_type: PositionType::Absolute, width: px(1), height: px(8), top: px(COMPASS_H - 14.0), ..default() },
            BackgroundColor(ink::DIM.with_alpha(0.7)),
            ChildOf(strip),
        ));
    }
}

/// Пеленг точки: градусы от севера (−Z) по часовой стрелке (восток — +X).
fn bearing(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    d.x.atan2(-d.z).to_degrees()
}

fn wrap180(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

#[allow(clippy::too_many_arguments)]
fn update_compass(
    mut commands: Commands,
    (net, party): (Res<NetState>, Res<Party>),
    view: Res<MapView>,
    art: Res<MarkerArt>,
    fonts: Res<crate::inventory_ui::UiFonts>,
    level: Res<crate::collision::LevelColliders>,
    mut pool: ResMut<MarkerPool>,
    mut root: Single<&mut Visibility, (With<CompassRoot>, Without<CompassTick>)>,
    strip: Single<Entity, With<CompassStrip>>,
    mut ticks: Query<(&CompassTick, &mut Node, &mut Visibility), Without<CompassRoot>>,
    local: Query<(&LocalPlayer, &Transform)>,
    remotes: Query<(&RemoteActor, &Transform)>,
    children: Query<&Children>,
    mut nodes: Query<&mut Node, Without<CompassTick>>,
    mut icons: Query<&mut ImageNode>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let Ok((me, me_t)) = local.single() else {
        **root = Visibility::Hidden;
        return;
    };
    **root = if view.open { Visibility::Hidden } else { Visibility::Inherited };
    let heading = (-me.yaw).to_degrees();
    let scale = (COMPASS_W / 2.0) / COMPASS_SPAN;
    for (tick, mut node, mut vis) in &mut ticks {
        let rel = wrap180(tick.deg - heading);
        *vis = if rel.abs() < COMPASS_SPAN + 10.0 { Visibility::Inherited } else { Visibility::Hidden };
        node.left = px(COMPASS_W / 2.0 + rel * scale - tick.half_w);
    }
    // Товарищи и метки на полосе компаса, с расстоянием.
    let mates: Vec<Mate> = gather(&net, &party, &local, &remotes, &level).into_iter().filter(|m| !(m.me && m.yaw.is_some())).collect();
    pool.compass.retain(|k, e| {
        let keep = mates.iter().any(|m| m.key == *k);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for m in &mates {
        let size = 14.0;
        let entity = *pool.compass.entry(m.key).or_insert_with(|| {
            let img = if m.yaw.is_some() { art.arrow.clone() } else { art.pin.clone() };
            let e = spawn_marker(&mut commands, &fonts, *strip, img, size, 11.0);
            commands.entity(e).insert(Visibility::Hidden);
            e
        });
        let Ok(kids) = children.get(entity) else { continue };
        let rel = wrap180(bearing(me_t.translation, m.pos) - heading);
        let dist = me_t.translation.xz().distance(m.pos.xz());
        let visible = rel.abs() < COMPASS_SPAN;
        commands.entity(entity).insert(if visible { Visibility::Inherited } else { Visibility::Hidden });
        if let Ok(mut node) = nodes.get_mut(entity) {
            node.left = px(COMPASS_W / 2.0 + rel * scale - size / 2.0);
            node.top = px(COMPASS_H - size - 3.0);
        }
        if let Some(&icon) = kids.first()
            && let Ok(mut img) = icons.get_mut(icon)
        {
            img.color = m.color;
        }
        if let Some(&label) = kids.get(1) {
            if let Ok(mut n) = nodes.get_mut(label) {
                n.left = px(size + 1.0);
                n.top = px(-2.0);
            }
            if let Ok((mut t, mut c)) = texts.get_mut(label) {
                let s = format!("{:.0} м", dist);
                if t.0 != s {
                    t.0 = s;
                }
                c.0 = ink::TEXT;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Имена над головами и метки в мире
// ---------------------------------------------------------------------------

#[derive(Component)]
struct NameTag;

/// Дальше этого имена товарищей не показываем (метки видны всегда).
const TAG_RANGE: f32 = 120.0;

#[allow(clippy::too_many_arguments)]
fn update_name_tags(
    mut commands: Commands,
    (net, party): (Res<NetState>, Res<Party>),
    view: Res<MapView>,
    fonts: Res<crate::inventory_ui::UiFonts>,
    level: Res<crate::collision::LevelColliders>,
    mut pool: ResMut<MarkerPool>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    local: Query<(&LocalPlayer, &Transform)>,
    remotes: Query<(&RemoteActor, &Transform)>,
    mut tags: Query<(&mut Node, &mut Text, &mut TextColor, &mut Visibility), With<NameTag>>,
) {
    let Ok((cam, cam_t)) = camera.single() else { return };
    let eye = cam_t.translation();
    let mates: Vec<Mate> = gather(&net, &party, &local, &remotes, &level).into_iter().filter(|m| !(m.me && m.yaw.is_some())).collect();
    pool.tags.retain(|k, e| {
        let keep = mates.iter().any(|m| m.key == *k);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for m in &mates {
        let entity = *pool.tags.entry(m.key).or_insert_with(|| {
            commands
                .spawn((
                    NameTag,
                    text(&fonts, "", 16.0, Color::WHITE, true),
                    TextLayout::justify(Justify::Center),
                    TextShadow { offset: Vec2::splat(1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.85) },
                    Node { position_type: PositionType::Absolute, width: px(240), ..default() },
                    Visibility::Hidden,
                    GlobalZIndex(5),
                ))
                .id()
        });
        let Ok((mut node, mut t, mut c, mut vis)) = tags.get_mut(entity) else { continue };
        let is_mark = m.yaw.is_none();
        let head = m.pos + Vec3::Y * if is_mark { 2.5 } else { 2.05 };
        let dist = eye.distance(m.pos);
        let in_range = is_mark || dist < TAG_RANGE;
        match cam.world_to_viewport(cam_t, head) {
            Ok(p) if in_range && !view.open && (head - eye).dot(*cam_t.forward()) > 0.0 => {
                *vis = Visibility::Inherited;
                node.left = px(p.x - 120.0);
                node.top = px(p.y - 22.0);
                let s = if is_mark || dist > 25.0 { format!("{}  ·  {:.0} м", m.label, dist) } else { m.label.clone() };
                if t.0 != s {
                    t.0 = s;
                }
                // Цвет игрока, высветленный, чтобы читался на фоне неба и камня.
                c.0 = m.color.mix(&Color::WHITE, 0.45);
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_clean() {
        let mut w = MapWorld::default();
        let a = w.unique_name(1, "  Иван\n ");
        assert_eq!(a, "Иван");
        w.names.insert(1, a);
        assert_eq!(w.unique_name(2, "Иван"), "Иван 2");
        assert_eq!(w.unique_name(1, "Иван"), "Иван", "своё имя не считается занятым");
        assert_eq!(w.unique_name(3, "\u{7}"), "Странник");
        assert_eq!(w.unique_name(4, &"я".repeat(50)).chars().count(), 20);
    }

    #[test]
    fn bearings_follow_the_map() {
        let o = Vec3::ZERO;
        // Север — к морю (−Z), восток — к Карантинной бухте (+X).
        assert!(bearing(o, Vec3::new(0.0, 0.0, -10.0)).abs() < 1e-3);
        assert!((bearing(o, Vec3::new(10.0, 0.0, 0.0)) - 90.0).abs() < 1e-3);
        assert!((bearing(o, Vec3::new(-10.0, 0.0, 0.0)) + 90.0).abs() < 1e-3);
        // Курс игрока совпадает с пеленгом точки прямо перед ним.
        for yaw in [0.0f32, 0.7, -2.0, 3.0] {
            let fwd = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            assert!(wrap180(bearing(o, fwd) - (-yaw).to_degrees()).abs() < 1e-2, "yaw {yaw}");
            assert!((yaw_of(&Transform::from_rotation(Quat::from_rotation_y(yaw))) - yaw).abs() < 1e-4);
        }
    }

    #[test]
    fn messages_roundtrip() {
        let m = MapServerMessage::Marks(vec![(7, [1.5, -2.0])]);
        let back: MapServerMessage = decode(&encode(&m)).unwrap();
        assert!(matches!(back, MapServerMessage::Marks(v) if v == vec![(7, [1.5, -2.0])]));
    }
}
