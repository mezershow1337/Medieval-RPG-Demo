//! Мини-карта в левом верхнем углу: круглый кусок карты писца вокруг игрока в бронзовом ободе,
//! север вверху.
//!
//! На ней в реальном времени: вы (стрелка в центре — куда смотрите), товарищи (стрелки своих
//! красок; ушедший за край прижимается к ободу), метки товарищей, а точками — люди города:
//! жители (по распорядку дня, даже за стенами домов), торговцы и вооружённые люди.
//!
//! Подложка — тот же лист, что на M (`map_render.rs`): новых текстур мини-карта не заводит.
//! Круг и обод рисует шейдер `shaders/minimap.wesl`.
//! N — показать/спрятать, +/− — масштаб (запоминаются в settings.txt).

use crate::client::{NetState, RemoteActor};
use crate::map::{MapSheet, MapView, MarkerArt, MarkerKey, Party, gather};
use crate::map_render::MapBounds;
use crate::player::LocalPlayer;
use crate::protocol::ActorKind;
use crate::settings::{MINIMAP_RADII, Settings};
use crate::ui_theme::{UiTheme, ink};
use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use std::collections::HashMap;

/// Диаметр круга вместе с ободом (логические пиксели).
const DIAM: f32 = 184.0;
/// Где кончается карта и начинается обод (доля радиуса) — как `RIM` в шейдере.
const RIM: f32 = 0.88;
/// Радиус видимой карты на экране.
const MAP_R: f32 = DIAM / 2.0 * RIM;
/// Краски точек: жители — умбра, торговцы — шафран, люди с оружием — киноварь.
const FOLK: Color = Color::srgb(0.30, 0.20, 0.12);
const MERCHANT: Color = Color::srgb(0.88, 0.64, 0.14);
const ARMED: Color = Color::srgb(0.74, 0.13, 0.08);
const RIM_LIGHT: Color = Color::srgba(0.97, 0.92, 0.78, 0.6);
const RIM_DARK: Color = Color::srgba(0.13, 0.08, 0.04, 0.9);

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/minimap.wesl");
        app.add_plugins(UiMaterialPlugin::<MiniMapMaterial>::default())
            .insert_resource(MiniPool::default())
            .add_systems(Startup, setup_minimap.after(crate::map::setup_marker_art).after(crate::inventory_ui::setup_fonts).after(crate::hud::setup_hud))
            .add_systems(Update, (minimap_input, update_minimap).chain());
    }
}

/// Круг карты: какой кусок листа показать (см. шейдер).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct MiniMapMaterial {
    #[uniform(0)]
    window: Vec4,
    #[texture(1)]
    #[sampler(2)]
    sheet: Option<Handle<Image>>,
}

impl UiMaterial for MiniMapMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/minimap.wesl".into()
    }
}

/// Доля листа в левом верхнем углу круга и сколько листа на его ширину.
fn sheet_window(center: Vec2, radius: f32, b: MapBounds) -> Vec4 {
    let half = radius / RIM;
    let corner = (center - Vec2::splat(half) - b.min) / b.size();
    Vec4::new(corner.x, corner.y, 2.0 * half / b.size(), 1.0)
}

#[derive(Component)]
struct MiniRoot;
#[derive(Component)]
struct MiniPlace;
/// Точка человека города.
#[derive(Component)]
struct MiniDot;

/// Узлы значков, чтобы не пересоздавать их каждый кадр.
#[derive(Resource, Default)]
struct MiniPool {
    dots: Vec<Entity>,
    mates: HashMap<MarkerKey, Entity>,
}

#[derive(Resource)]
struct MiniUi {
    disc: Entity,
    root: Entity,
    material: Handle<MiniMapMaterial>,
}

fn setup_minimap(mut commands: Commands, theme: Res<UiTheme>, column: Res<crate::hud::HudLeftColumn>, mut materials: ResMut<Assets<MiniMapMaterial>>) {
    // Блок мини-карты — первым в левой колонке HUD (её прячет главное меню).
    let root = commands
        .spawn((MiniRoot, Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, width: px(DIAM), row_gap: px(2), display: Display::None, ..default() }))
        .id();
    commands.entity(column.0).insert_children(0, &[root]);
    let material = materials.add(MiniMapMaterial { window: Vec4::ZERO, sheet: None });
    let disc = commands
        .spawn((
            MaterialNode(material.clone()),
            Node { width: px(DIAM), height: px(DIAM), ..default() },
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), px(0), px(3), px(0), px(10)),
            ChildOf(root),
        ))
        .id();
    // Север — буква на ободе вверху.
    commands.spawn((
        theme.bold("С", 13.0, Color::srgb(0.98, 0.93, 0.80)),
        TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.1, 0.05, 0.0, 0.9) },
        TextLayout::justify(Justify::Center),
        Node { position_type: PositionType::Absolute, left: px(DIAM / 2.0 - 10.0), width: px(20), top: px(-2), ..default() },
        GlobalZIndex(3),
        ChildOf(disc),
    ));
    commands.spawn((
        MiniPlace,
        theme.bold("", 14.0, ink::TITLE),
        TextShadow { offset: Vec2::splat(1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.85) },
        TextLayout::justify(Justify::Center),
        Node { max_width: px(DIAM + 40.0), ..default() },
        ChildOf(root),
    ));
    commands.insert_resource(MiniUi { disc, root, material });
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
    mut materials: ResMut<Assets<MiniMapMaterial>>,
    (net, party, art, level): (Res<NetState>, Res<Party>, Res<MarkerArt>, Res<crate::collision::LevelColliders>),
    (residents, mut pool): (Option<Res<crate::life_client::ResidentSpots>>, ResMut<MiniPool>),
    (local, remotes, merchants): (Query<(&LocalPlayer, &Transform)>, Query<(&RemoteActor, &Transform)>, Query<&Transform, With<crate::trade_client::MerchantNpc>>),
    mut vis: Query<&mut Visibility>,
    mut nodes: Query<&mut Node>,
    mut colors: Query<(&mut BackgroundColor, &mut BorderColor)>,
    mut icons: Query<(&mut ImageNode, &mut UiTransform)>,
    mut place: Query<&mut Text, With<MiniPlace>>,
    children: Query<&Children>,
) {
    let me = local.single().ok();
    let shown = settings.as_ref().is_none_or(|s| s.minimap) && !view.open && me.is_some();
    if let Ok(mut n) = nodes.get_mut(ui.root) {
        let want = if shown { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
    let Some((_, me_t)) = me else { return };
    if !shown {
        return;
    }
    let zoom = settings.as_ref().map_or(1, |s| s.minimap_zoom as usize).min(MINIMAP_RADII.len() - 1);
    let radius = MINIMAP_RADII[zoom];
    let s = MAP_R / radius;
    let c = me_t.translation.xz();
    let to_screen = |p: Vec2| (p - c) * s + Vec2::splat(DIAM / 2.0);

    // Подложка: кусок листа вокруг игрока (шейдер обрезает кругом).
    if let Some(mut m) = materials.get_mut(&ui.material) {
        let window = sheet.bounds.map_or(Vec4::ZERO, |b| sheet_window(c, radius, b));
        let window = if sheet.image.is_some() { window } else { Vec4::ZERO };
        if m.window != window || m.sheet != sheet.image {
            m.window = window;
            m.sheet = sheet.image.clone();
        }
    }

    // Люди города: точки внутри круга.
    let mut people: Vec<(Vec2, Color, f32)> = Vec::new();
    let inside = |p: Vec2| p.distance(c) < radius - 4.0 / s;
    if let Some(r) = &residents {
        people.extend(r.0.iter().flatten().filter(|p| inside(**p)).map(|p| (*p, FOLK, 6.0)));
    }
    people.extend(merchants.iter().map(|t| t.translation.xz()).filter(|p| inside(*p)).map(|p| (p, MERCHANT, 7.0)));
    people.extend(
        remotes
            .iter()
            .filter(|(r, _)| r.kind != ActorKind::Player && r.fighter.hp > 0.0)
            .map(|(_, t)| t.translation.xz())
            .filter(|p| inside(*p))
            .map(|p| (p, ARMED, 7.0)),
    );
    while pool.dots.len() < people.len() {
        let e = commands
            .spawn((
                MiniDot,
                Node { position_type: PositionType::Absolute, width: px(6), height: px(6), border: UiRect::all(px(1)), border_radius: BorderRadius::MAX, ..default() },
                BorderColor::all(RIM_DARK),
                BackgroundColor(FOLK),
                Visibility::Hidden,
                ChildOf(ui.disc),
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

    // Игроки и метки (поверх точек). Кто за краем — прижат к ободу.
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
            (true, true) => 18.0,
            (_, true) => 15.0,
            _ => 13.0,
        };
        let entity = *pool.mates.entry(m.key).or_insert_with(|| {
            let img = if m.yaw.is_some() { art.arrow.clone() } else { art.pin.clone() };
            let e = commands.spawn((Node { position_type: PositionType::Absolute, width: px(size), height: px(size), ..default() }, GlobalZIndex(2), ChildOf(ui.disc))).id();
            commands.spawn((ImageNode::new(img), Node { width: percent(100), height: percent(100), ..default() }, UiTransform::default(), ChildOf(e)));
            e
        });
        let edge = MAP_R - size / 2.0 - 1.0;
        let q = to_screen(m.pos.xz()) - Vec2::splat(DIAM / 2.0);
        let q = q.clamp_length_max(edge) + Vec2::splat(DIAM / 2.0);
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

    let caption = place_caption(me_t.translation);
    if let Ok(mut t) = place.single_mut()
        && t.0 != caption
    {
        t.0 = caption;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_is_in_the_middle_of_the_disc() {
        let b = MapBounds { min: Vec2::new(-800.0, -700.0), max: Vec2::new(800.0, 900.0) };
        for (c, r) in [(Vec2::ZERO, 50.0), (Vec2::new(182.0, -15.0), 28.0), (Vec2::new(-200.0, 140.0), 210.0)] {
            let w = sheet_window(c, r, b);
            // Середина узла (uv 0.5) — точка игрока на листе.
            let mid = w.xy() + Vec2::splat(0.5) * w.z;
            assert!((mid - b.to_uv(c.x, c.y)).length() < 1e-4);
            // Край карты (RIM) — ровно `r` метров от игрока.
            let edge = w.xy() + Vec2::new(0.5 + RIM / 2.0, 0.5) * w.z;
            assert!((b.from_uv(edge).x - c.x - r).abs() < 0.01);
        }
    }
}
