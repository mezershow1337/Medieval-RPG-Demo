//! Мини-карта в правом верхнем углу: кусок карты писца вокруг игрока, север вверху.
//!
//! На ней в реальном времени: вы (стрелка в центре — куда смотрите), товарищи (стрелки своих
//! красок; ушедший за край прижимается к рамке), метки товарищей, а точками — люди города:
//! жители (по распорядку дня, даже за стенами домов), торговцы и вооружённые люди.
//!
//! Подложка — тот же лист, что на M (`map_render.rs`): новых текстур мини-карта не заводит.
//! N — показать/спрятать, +/− — масштаб (запоминаются в settings.txt).

use crate::client::{NetState, RemoteActor};
use crate::map::{MapSheet, MapView, MarkerArt, MarkerKey, Party, gather, text};
use crate::player::LocalPlayer;
use crate::protocol::ActorKind;
use crate::settings::{MINIMAP_RADII, Settings};
use crate::ui_theme::{Skin, UiTheme, ink};
use bevy::prelude::*;
use std::collections::HashMap;

/// Сторона окошка карты на экране (логические пиксели).
const SIDE: f32 = 232.0;
const INK: Color = ink::INK;
const FRAME_DARK: Color = Color::srgb(0.16, 0.10, 0.05);
const PARCHMENT_BG: Color = Color::srgb(0.80, 0.71, 0.53);
/// Краски точек: жители — умбра, торговцы — шафран, люди с оружием — киноварь.
const FOLK: Color = Color::srgb(0.30, 0.20, 0.12);
const MERCHANT: Color = Color::srgb(0.88, 0.64, 0.14);
const ARMED: Color = Color::srgb(0.74, 0.13, 0.08);
const RIM_LIGHT: Color = Color::srgb(0.97, 0.92, 0.78);
const RIM_DARK: Color = Color::srgba(0.13, 0.08, 0.04, 0.9);

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MiniPool::default())
            .add_systems(Startup, setup_minimap.after(crate::map::setup_marker_art).after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (minimap_input, update_minimap).chain());
    }
}

#[derive(Component)]
struct MiniRoot;
#[derive(Component)]
struct MiniViewport;
/// Подложка — лист карты писца.
#[derive(Component)]
struct MiniSheet;
#[derive(Component)]
struct MiniPlace;
#[derive(Component)]
struct MiniKeys;
#[derive(Component)]
struct MiniScaleBar;
#[derive(Component)]
struct MiniScaleText;
#[derive(Component)]
struct MiniLoading;
/// Точка человека города.
#[derive(Component)]
struct MiniDot;

/// Узлы значков, чтобы не пересоздавать их каждый кадр.
#[derive(Resource, Default)]
struct MiniPool {
    dots: Vec<Entity>,
    mates: HashMap<MarkerKey, Entity>,
}

fn setup_minimap(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>, theme: Res<UiTheme>) {
    // Внешний узел — часть HUD (его прячет главное меню), внутренний — по настройке и карте M.
    let hud = commands
        .spawn((
            crate::hud::HudRoot,
            Node { position_type: PositionType::Absolute, right: px(10), top: px(46), ..default() },
            GlobalZIndex(-1),
            Pickable::IGNORE,
        ))
        .id();
    let root = commands
        .spawn((
            MiniRoot,
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::all(px(15)), row_gap: px(5), ..default() },
            theme.skin(Skin::Leather),
            Visibility::Hidden,
            ChildOf(hud),
        ))
        .id();
    let viewport = commands
        .spawn((
            MiniViewport,
            Node { width: px(SIDE + 4.0), height: px(SIDE + 4.0), overflow: Overflow::clip(), border: UiRect::all(px(2)), ..default() },
            BorderColor::all(FRAME_DARK),
            BackgroundColor(PARCHMENT_BG),
            ChildOf(root),
        ))
        .id();
    commands.spawn((MiniSheet, ImageNode::default(), Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, ChildOf(viewport)));
    commands.spawn((
        MiniLoading,
        text(&fonts, "Писец рисует…", 15.0, INK, false),
        Node { position_type: PositionType::Absolute, left: px(SIDE / 2.0 - 52.0), top: px(SIDE / 2.0 - 10.0), ..default() },
        ChildOf(viewport),
    ));
    // Роза ветров в углу: север вверху.
    commands.spawn((
        text(&fonts, "С", 17.0, ink::RUBRIC, true),
        TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.96, 0.9, 0.76, 0.9) },
        Node { position_type: PositionType::Absolute, left: px(6), top: px(2), ..default() },
        GlobalZIndex(1),
        ChildOf(viewport),
    ));
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: px(8), top: px(22), width: px(2), height: px(9), ..default() },
        BackgroundColor(ink::RUBRIC),
        ChildOf(viewport),
    ));
    // Линейка масштаба внизу слева.
    commands.spawn((
        MiniScaleBar,
        Node { position_type: PositionType::Absolute, left: px(8), bottom: px(8), height: px(4), width: px(50), border: UiRect::axes(px(1), px(1)), ..default() },
        BorderColor::all(INK),
        BackgroundColor(Color::srgba(0.95, 0.89, 0.74, 0.9)),
        ChildOf(viewport),
    ));
    commands.spawn((
        MiniScaleText,
        text(&fonts, "", 12.0, INK, true),
        TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.96, 0.9, 0.76, 0.9) },
        Node { position_type: PositionType::Absolute, left: px(8), bottom: px(13), ..default() },
        ChildOf(viewport),
    ));
    commands.spawn((
        MiniPlace,
        theme.bold("", 15.0, ink::TITLE),
        TextLayout::justify(Justify::Center),
        Node { max_width: px(SIDE), ..default() },
        ChildOf(root),
    ));
    commands.spawn((MiniKeys, theme.text("N — спрятать  ·  +/− — масштаб", 11.0, ink::DIM), ChildOf(root)));
    commands.insert_resource(MiniUi { viewport, root });
}

#[derive(Resource)]
struct MiniUi {
    viewport: Entity,
    root: Entity,
}

fn minimap_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: Option<ResMut<Settings>>,
    view: Res<MapView>,
    trade: Option<Res<crate::trade_client::TradeState>>,
    console: Option<Res<crate::console::Console>>,
    mut env_applied: Local<bool>,
) {
    let Some(s) = settings.as_mut() else { return };
    // Снимки: `MRPG_MINIMAP_ZOOM=0..4` — масштаб без нажатий.
    if !*env_applied {
        *env_applied = true;
        if let Some(z) = std::env::var("MRPG_MINIMAP_ZOOM").ok().and_then(|v| v.parse::<u8>().ok()) {
            s.minimap_zoom = z.min(MINIMAP_RADII.len() as u8 - 1);
        }
    }
    if view.open || console.is_some_and(|c| c.open) || trade.is_some_and(|t| t.window_open()) {
        return;
    }
    let mut changed = false;
    if keys.just_pressed(KeyCode::KeyN) {
        s.minimap = !s.minimap;
        changed = true;
    }
    if s.minimap {
        let last = MINIMAP_RADII.len() as u8 - 1;
        if keys.any_just_pressed([KeyCode::Equal, KeyCode::NumpadAdd]) && s.minimap_zoom > 0 {
            s.minimap_zoom -= 1;
            changed = true;
        }
        if keys.any_just_pressed([KeyCode::Minus, KeyCode::NumpadSubtract]) && s.minimap_zoom < last {
            s.minimap_zoom += 1;
            changed = true;
        }
    }
    if changed {
        s.save();
    }
}

/// Красивая длина линейки масштаба (метры), чтобы на экране было 40–80 px.
fn scale_bar_meters(px_per_m: f32) -> f32 {
    [5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0].into_iter().find(|m| m * px_per_m >= 40.0).unwrap_or(200.0)
}

/// Как назвать место, где стоит игрок.
fn place_caption(pos: Vec3) -> String {
    let near = crate::level::places()
        .iter()
        .filter(|p| p.kind != crate::level::PlaceKind::Hearth)
        .map(|p| (p, p.pos.xz().distance(pos.xz()) - p.radius))
        .filter(|(_, d)| *d < 6.0)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((p, _)) = near {
        return p.name.to_string();
    }
    // Перекрёстки и площади, где появляются игроки, тоже названы.
    if let Some(spot) = crate::level::spawn_spots().iter().find(|s| s.pos.xz().distance(pos.xz()) < 10.0) {
        return spot.name.to_string();
    }
    if crate::level::active() != crate::level::LevelKind::Chersonesus {
        return "Замок".into();
    }
    if crate::level::inside_city(pos.x, pos.z) {
        if pos.z.abs() < crate::level::MAIN_STREET_HALF + 1.0 { "Главная улица" } else { "Херсон" }.into()
    } else {
        "Окрестности Херсона".into()
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_minimap(
    mut commands: Commands,
    ui: Res<MiniUi>,
    settings: Option<Res<Settings>>,
    view: Res<MapView>,
    sheet: Res<MapSheet>,
    (net, party, art, level): (Res<NetState>, Res<Party>, Res<MarkerArt>, Res<crate::collision::LevelColliders>),
    (residents, mut pool): (Option<Res<crate::life_client::ResidentSpots>>, ResMut<MiniPool>),
    (local, remotes, merchants): (Query<(&LocalPlayer, &Transform)>, Query<(&RemoteActor, &Transform)>, Query<&Transform, With<crate::trade_client::MerchantNpc>>),
    mut vis: Query<&mut Visibility>,
    mut nodes: Query<&mut Node>,
    mut sheets: Query<(Entity, &mut ImageNode), With<MiniSheet>>,
    mut colors: Query<(&mut BackgroundColor, &mut BorderColor)>,
    mut icons: Query<(&mut ImageNode, &mut UiTransform), Without<MiniSheet>>,
    mut texts: ParamSet<(Query<&mut Text, With<MiniPlace>>, Query<&mut Text, With<MiniScaleText>>)>,
    (loading, scale_bar, children): (Query<Entity, With<MiniLoading>>, Query<Entity, With<MiniScaleBar>>, Query<&Children>),
) {
    let me = local.single().ok();
    let shown = settings.as_ref().is_none_or(|s| s.minimap) && !view.open && me.is_some();
    if let Ok(mut v) = vis.get_mut(ui.root) {
        v.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
    let Some((_, me_t)) = me else { return };
    if !shown {
        return;
    }
    let zoom = settings.as_ref().map_or(1, |s| s.minimap_zoom as usize).min(MINIMAP_RADII.len() - 1);
    let radius = MINIMAP_RADII[zoom];
    let s = SIDE / (2.0 * radius);
    let c = me_t.translation.xz();
    let to_screen = |p: Vec2| (p - c) * s + Vec2::splat(SIDE / 2.0);

    // Подложка: лист карты, сдвинутый так, чтобы игрок был в центре.
    let any_sheet = sheet.image.is_some() && sheet.bounds.is_some();
    if let (Some(handle), Some(b), Ok((e, mut img))) = (sheet.image.clone(), sheet.bounds, sheets.single_mut()) {
        if img.image != handle {
            img.image = handle;
        }
        let full = b.size() * s;
        let tl = to_screen(b.min);
        if let Ok(mut n) = nodes.get_mut(e) {
            n.width = px(full);
            n.height = px(full);
            n.left = px(tl.x);
            n.top = px(tl.y);
        }
        if let Ok(mut v) = vis.get_mut(e) {
            v.set_if_neq(Visibility::Inherited);
        }
    }
    for e in &loading {
        if let Ok(mut v) = vis.get_mut(e) {
            v.set_if_neq(if any_sheet { Visibility::Hidden } else { Visibility::Inherited });
        }
    }

    // Люди города: точки.
    let mut people: Vec<(Vec2, Color, f32)> = Vec::new();
    let inside = |p: Vec2| (p - c).abs().max_element() < radius - 2.0 / s;
    if let Some(r) = &residents {
        people.extend(r.0.iter().flatten().filter(|p| inside(**p)).map(|p| (*p, FOLK, 7.0)));
    }
    people.extend(merchants.iter().map(|t| t.translation.xz()).filter(|p| inside(*p)).map(|p| (p, MERCHANT, 8.0)));
    people.extend(
        remotes
            .iter()
            .filter(|(r, _)| r.kind != ActorKind::Player && r.fighter.hp > 0.0)
            .map(|(_, t)| t.translation.xz())
            .filter(|p| inside(*p))
            .map(|p| (p, ARMED, 8.0)),
    );
    while pool.dots.len() < people.len() {
        let e = commands
            .spawn((
                MiniDot,
                Node { position_type: PositionType::Absolute, width: px(6), height: px(6), border: UiRect::all(px(1)), border_radius: BorderRadius::MAX, ..default() },
                BorderColor::all(RIM_DARK),
                BackgroundColor(FOLK),
                Visibility::Hidden,
                ChildOf(ui.viewport),
            ))
            .id();
        pool.dots.push(e);
    }
    for (k, &e) in pool.dots.iter().enumerate() {
        let Some(&(p, color, size)) = people.get(k) else {
            if let Ok(mut v) = vis.get_mut(e) {
                v.set_if_neq(Visibility::Hidden);
            }
            continue;
        };
        let q = to_screen(p);
        if let Ok(mut n) = nodes.get_mut(e) {
            n.left = px((q.x - size / 2.0).round());
            n.top = px((q.y - size / 2.0).round());
            n.width = px(size);
            n.height = px(size);
        }
        if let Ok((mut bg, mut border)) = colors.get_mut(e) {
            bg.set_if_neq(BackgroundColor(color));
            // Жители — тёмные точки в светлом ободке, чтобы не спутать с рисунком кровель.
            border.set_if_neq(BorderColor::all(if color == FOLK { RIM_LIGHT } else { RIM_DARK }));
        }
        if let Ok(mut v) = vis.get_mut(e) {
            v.set_if_neq(Visibility::Inherited);
        }
    }

    // Игроки и метки (поверх точек). Кто за краем — прижат к рамке.
    let mates = gather(&net, &party, &local, &remotes, &level);
    pool.mates.retain(|k, e| {
        let keep = mates.iter().any(|m| m.key == *k);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    let mut order: Vec<_> = mates.iter().collect();
    order.sort_by_key(|m| (matches!(m.key, MarkerKey::Player(_)), m.me));
    for m in order {
        let size = match (m.me, m.yaw.is_some()) {
            (true, true) => 20.0,
            (_, true) => 16.0,
            _ => 14.0,
        };
        let entity = *pool.mates.entry(m.key).or_insert_with(|| {
            let img = if m.yaw.is_some() { art.arrow.clone() } else { art.pin.clone() };
            let e = commands.spawn((Node { position_type: PositionType::Absolute, width: px(size), height: px(size), ..default() }, GlobalZIndex(2), ChildOf(ui.viewport))).id();
            commands.spawn((ImageNode::new(img), Node { width: percent(100), height: percent(100), ..default() }, UiTransform::default(), ChildOf(e)));
            e
        });
        let half = SIDE / 2.0 - size / 2.0 - 1.0;
        let q = to_screen(m.pos.xz()) - Vec2::splat(SIDE / 2.0);
        let q = if q.abs().max_element() > half { q * (half / q.abs().max_element()) } else { q } + Vec2::splat(SIDE / 2.0);
        if let Ok(mut n) = nodes.get_mut(entity) {
            n.left = px(q.x - size / 2.0);
            n.top = px(q.y - size / 2.0);
        }
        if let Ok(kids) = children.get(entity)
            && let Some(&icon) = kids.first()
            && let Ok((mut img, mut tf)) = icons.get_mut(icon)
        {
            img.color = m.color;
            tf.rotation = Rot2::radians(-m.yaw.unwrap_or(0.0));
        }
    }

    // Подпись места и линейка.
    let caption = place_caption(me_t.translation);
    if let Ok(mut t) = texts.p0().single_mut()
        && t.0 != caption
    {
        t.0 = caption;
    }
    let meters = scale_bar_meters(s);
    for e in &scale_bar {
        if let Ok(mut n) = nodes.get_mut(e) {
            n.width = px(meters * s);
        }
    }
    let label = format!("{meters:.0} м");
    if let Ok(mut t) = texts.p1().single_mut()
        && t.0 != label
    {
        t.0 = label;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_bar_is_readable() {
        for r in MINIMAP_RADII {
            let s = SIDE / (2.0 * r);
            let m = scale_bar_meters(s);
            let w = m * s;
            assert!((40.0..=SIDE * 0.45).contains(&w), "радиус {r}: линейка {m} м = {w} px");
        }
    }
}
