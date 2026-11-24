//! Интерфейс: прицел с меткой направления удара и окна парирования, здоровье, надписи боя,
//! статус подключения, подсказка по управлению. Надписи боя и подсказка — по-русски (шрифт Lora).

use crate::client::{NetState, RemoteActor};
use crate::fight::{Banner, DamageFlash, LocalFighter};
use crate::player::{LocalPlayer, PlayerCamera};
use crate::protocol::ActorKind;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::text::FontSize;

pub struct HudPlugin;

/// Корни интерфейса боя (прячутся, пока открыто главное меню).
#[derive(Component)]
pub struct HudRoot;

#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct HealthFill;
#[derive(Component)]
struct HealthText;
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct FlashOverlay;
#[derive(Component)]
struct TargetText;
#[derive(Component)]
struct StaminaFill;
#[derive(Component)]
struct HelpText;
/// Точка у прицела: направление удара `Swing::ALL[i]` (через каждые 45°, от «справа» против часовой).
#[derive(Component)]
struct AimDot(usize);
/// Кольцо у прицела: окно парирования, глухой блок, заминка, рипост.
#[derive(Component)]
struct ParryRing;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, setup_hud.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (update_status, update_health, update_banner, update_target, update_aim, toggle_help));
    }
}

fn text(s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

const AIM_BOX: f32 = 76.0;
const AIM_RADIUS: f32 = 26.0;

pub fn setup_hud(mut commands: Commands, theme: Res<crate::ui_theme::UiTheme>) {
    // Красная вспышка при уроне (на весь экран).
    commands.spawn((HudRoot, FlashOverlay, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::NONE)));
    // Прицел.
    commands
        .spawn((HudRoot, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
        .with_children(|p| {
            // Прицел, вокруг — восемь точек направлений удара и кольцо парирования.
            p.spawn(Node { width: px(AIM_BOX), height: px(AIM_BOX), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|b| {
                b.spawn((Node { width: px(4), height: px(4), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8))));
                let ring = 34.0;
                b.spawn((
                    ParryRing,
                    Node {
                        width: px(ring),
                        height: px(ring),
                        position_type: PositionType::Absolute,
                        left: px((AIM_BOX - ring) / 2.0),
                        top: px((AIM_BOX - ring) / 2.0),
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(ring / 2.0)),
                        ..default()
                    },
                    BorderColor::all(Color::NONE),
                ));
                for i in 0..8 {
                    let a = (i as f32 * 45.0).to_radians();
                    let d = 6.0;
                    b.spawn((
                        AimDot(i),
                        Node {
                            width: px(d),
                            height: px(d),
                            position_type: PositionType::Absolute,
                            left: px(AIM_BOX / 2.0 + a.cos() * AIM_RADIUS - d / 2.0),
                            top: px(AIM_BOX / 2.0 - a.sin() * AIM_RADIUS - d / 2.0),
                            border_radius: BorderRadius::all(px(d / 2.0)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ));
                }
            });
        });
    // Надпись по центру (парирование, смерть).
    commands
        .spawn((HudRoot, Node { width: percent(100), position_type: PositionType::Absolute, top: percent(30), justify_content: JustifyContent::Center, ..default() }))
        .with_children(|p| {
            p.spawn((BannerText, theme.title_colored("", 40.0, Color::WHITE)));
        });
    // Цель (враг перед вами) — вверху по центру.
    commands
        .spawn((HudRoot, Node { width: percent(100), position_type: PositionType::Absolute, top: px(14), justify_content: JustifyContent::Center, ..default() }))
        .with_children(|p| {
            p.spawn((TargetText, text("", 18.0, Color::srgb(1.0, 0.9, 0.8))));
        });
    // Здоровье внизу по центру.
    commands
        .spawn((HudRoot, Node {
            width: percent(100),
            position_type: PositionType::Absolute,
            bottom: px(96),
            justify_content: JustifyContent::Center,
            ..default()
        }))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: px(320),
                    height: px(24),
                    padding: UiRect::axes(px(5), px(4)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                theme.bar_frame(),
            ))
            .with_children(|bar| {
                bar.spawn((
                    HealthFill,
                    Node {
                        width: percent(100),
                        height: percent(100),
                        border_radius: BorderRadius::all(px(2)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.62, 0.07, 0.06)),
                ));
                bar.spawn((
                    HealthText,
                    theme.bold("100", 13.0, crate::ui_theme::ink::TEXT),
                    TextShadow {
                        offset: Vec2::new(1.0, 1.0),
                        color: Color::BLACK,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(10),
                        ..default()
                    },
                ));
            });
        });
    // Выносливость — тонкая полоска под здоровьем.
    commands
        .spawn((HudRoot, Node { width: percent(100), position_type: PositionType::Absolute, bottom: px(80), justify_content: JustifyContent::Center, ..default() }))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: px(320),
                    height: px(14),
                    padding: UiRect::axes(px(5), px(4)),
                    ..default()
                },
                theme.bar_frame(),
            ))
            .with_children(|bar| {
                bar.spawn((
                    StaminaFill,
                    Node {
                        width: percent(100),
                        height: percent(100),
                        border_radius: BorderRadius::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.85, 0.75, 0.25)),
                ));
            });
        });
    // Левая колонка сверху: мини-карта (её вставляет `minimap.rs` первой), строка связи, управление.
    let column = commands
        .spawn((
            HudRoot,
            Node { position_type: PositionType::Absolute, left: px(12), top: px(10), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, row_gap: px(6), ..default() },
            // Под окнами (сумка, торговля), если они дотянутся до угла.
            GlobalZIndex(-1),
            Pickable::IGNORE,
        ))
        .id();
    commands.insert_resource(HudLeftColumn(column));
    commands.spawn((StatusText, text("Connecting...", 14.0, Color::WHITE), TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.8) }, Node { display: Display::None, ..default() }, ChildOf(column)));
    commands.spawn((HelpHint, theme.text("F2 — управление", 12.0, Color::srgba(1.0, 1.0, 1.0, 0.7)), TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.8) }, ChildOf(column)));
    commands.spawn((
        HelpText,
        TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.8) },
        theme.text(

            "Esc — меню (пауза, настройки) | F9 — эффекты | F11 — полный экран | F2 — скрыть\n\
             WASD — ходьба | Shift — бег | Пробел — прыжок | Ctrl/C — присесть\n\
             ЛКМ — удар оттуда, куда ведёте мышь (вправо, влево, вверх, наискось; вниз — укол), СКМ — укол\n\
             ПКМ вовремя — парирование и рипост (ЛКМ); держать — глухой блок (дорого, без рипоста)\n\
             Q или ПКМ на замахе — финт | другой удар на замахе — морф | встречный зеркальный удар — чембер\n\
             1 — кулаки | 2–9 / колесо — хотбар | F7 — подсказки боя у прицела\n\
             M — карта | N — мини-карта | +/− — её масштаб",
            12.0,
            Color::srgba(1.0, 1.0, 1.0, 0.8),
        ),
        Node { display: Display::None, ..default() },
        ChildOf(column),
    ));
}

/// Колонка HUD в левом верхнем углу (мини-карта, связь, управление).
#[derive(Resource)]
pub struct HudLeftColumn(pub Entity);

/// «F2 — управление», пока список клавиш спрятан.
#[derive(Component)]
struct HelpHint;

/// F2 — показать или спрятать список управления (запоминается в settings.txt).
fn toggle_help(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: Option<ResMut<crate::settings::Settings>>,
    console: Option<Res<crate::console::Console>>,
    mut help: Single<&mut Node, (With<HelpText>, Without<HelpHint>)>,
    mut hint: Single<&mut Node, (With<HelpHint>, Without<HelpText>)>,
) {
    let Some(s) = settings.as_mut() else { return };
    if keys.just_pressed(KeyCode::F2) && !console.is_some_and(|c| c.open) {
        s.show_controls = !s.show_controls;
        s.save();
    }
    let (shown, hidden) = (Display::Flex, Display::None);
    let want = if s.show_controls { (shown, hidden) } else { (hidden, shown) };
    if help.display != want.0 {
        help.display = want.0;
    }
    if hint.display != want.1 {
        hint.display = want.1;
    }
}

/// Метка у прицела: куда пойдёт удар (по движению мыши), какой удар идёт сейчас,
/// и кольцо — открыто ли окно парирования (золотое), глухой блок (серое), заминка (красное).
fn update_aim(
    time: Res<Time>,
    aim: Res<crate::fight::AttackAim>,
    local: Query<&LocalFighter>,
    mut dots: Query<(&AimDot, &mut BackgroundColor)>,
    mut ring: Single<&mut BorderColor, With<ParryRing>>,
    hints: Res<crate::fight::CombatHints>,
) {
    use crate::combat::{Action, Swing};
    let Ok(f) = local.single() else { return };
    let melee = !f.fighter.weapon.ranged() && !f.fighter.is_dead() && hints.0;
    let active = f.fighter.action.swing().filter(|_| f.fighter.action.is_attacking());
    let riposte = matches!(f.fighter.action, Action::Windup { riposte: true, .. } | Action::Strike { riposte: true, .. });
    let pulse = 0.6 + 0.4 * (time.elapsed_secs() * 14.0).sin();
    for (dot, mut bg) in &mut dots {
        let swing = Swing::ALL[dot.0];
        let allowed = f.fighter.weapon.normalize(swing) == swing;
        bg.0 = if !melee || !allowed {
            Color::NONE
        } else if active == Some(swing) {
            if riposte { Color::srgb(1.0, 0.25, 0.15) } else { Color::srgb(1.0, 0.65, 0.2) }
        } else if active.is_none() && aim.pending == Some(swing) {
            Color::srgba(1.0, 1.0, 1.0, 0.95)
        } else if active.is_none() && f.fighter.can_riposte() {
            Color::srgba(1.0, 0.8, 0.3, 0.5 * pulse)
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.16)
        };
    }
    let c = if !melee {
        Color::NONE
    } else if f.fighter.can_riposte() {
        Color::srgba(1.0, 0.8, 0.25, pulse)
    } else if f.fighter.in_parry_window() {
        Color::srgba(1.0, 0.85, 0.3, 0.95)
    } else if f.fighter.guard_active() {
        Color::srgba(0.75, 0.75, 0.8, 0.55)
    } else if matches!(f.fighter.action, Action::ParryRecovery { .. } | Action::Recoil { .. }) {
        Color::srgba(1.0, 0.3, 0.2, 0.6)
    } else {
        Color::NONE
    };
    **ring = BorderColor::all(c);
}

fn update_status(
    state: Res<NetState>,
    client: Option<Res<bevy_renet::RenetClient>>,
    diagnostics: Res<DiagnosticsStore>,
    remotes: Query<&RemoteActor>,
    player: Query<&Transform, With<LocalPlayer>>,
    mut text: Single<(&mut Text, &mut Node), With<StatusText>>,
    settings: Option<Res<crate::settings::Settings>>,
) {
    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let addr = state.server_addr.map(|a| a.to_string()).unwrap_or_default();
    let status = match (&client, state.my_id, &state.error) {
        (_, _, Some(err)) => format!("Network error: {err}"),
        (Some(c), Some(_), _) if c.is_connected() => {
            let ping = c.rtt() * 1000.0;
            let players = remotes.iter().filter(|r| r.kind == ActorKind::Player).count() + 1;
            let enemies = remotes.iter().filter(|r| r.kind.is_npc()).count();
            format!("Connected to {addr} | players: {players} | enemies: {enemies} | ping {ping:.0} ms")
        }
        (Some(c), _, _) if c.is_disconnected() => format!("Disconnected from {addr}"),
        _ => format!("Connecting to {addr}..."),
    };
    let pos = player.single().map(|t| format!(" | pos {:.1} {:.1} {:.1}", t.translation.x, t.translation.y, t.translation.z)).unwrap_or_default();
    // Без «Показывать FPS» строка видна, только пока что-то не так со связью.
    let show = settings.is_none_or(|s| s.show_fps);
    let connected = client.as_ref().is_some_and(|c| c.is_connected()) && state.error.is_none();
    let (text, node) = &mut *text;
    text.0 = if show { format!("{status}\nFPS {fps:.0}{pos}") } else if connected { String::new() } else { status };
    // Пустая строка не занимает места в колонке.
    let want = if text.0.is_empty() { Display::None } else { Display::Flex };
    if node.display != want {
        node.display = want;
    }
}

fn update_health(
    local: Query<&LocalFighter>,
    flash: Res<DamageFlash>,
    mut fill: Single<&mut Node, (With<HealthFill>, Without<StaminaFill>)>,
    mut label: Single<&mut Text, With<HealthText>>,
    mut overlay: Single<&mut BackgroundColor, (With<FlashOverlay>, Without<StaminaFill>)>,
    mut stamina: Single<(&mut Node, &mut BackgroundColor), (With<StaminaFill>, Without<HealthFill>)>,
) {
    if let Ok(f) = local.single() {
        let s = (f.fighter.stamina / f.fighter.max_stamina.max(1.0)).clamp(0.0, 1.0);
        let (node, color) = &mut *stamina;
        node.width = percent(s * 100.0);
        // Почти без сил — полоска краснеет.
        color.0 = if s < 0.2 { Color::srgb(0.85, 0.3, 0.15) } else { Color::srgb(0.85, 0.75, 0.25) };
        let k = (f.fighter.hp / f.fighter.max_hp).clamp(0.0, 1.0);
        fill.width = percent(k * 100.0);
        label.0 = format!("{:.0} / {:.0}", f.fighter.hp, f.fighter.max_hp);
        let dead = f.fighter.is_dead();
        let a = if dead { 0.45 } else { flash.0 * 0.5 };
        overlay.0 = Color::srgba(0.6, 0.0, 0.0, a);
    }
}

fn update_banner(banner: Res<Banner>, mut q: Single<(&mut Text, &mut TextColor), With<BannerText>>) {
    let (text, color) = &mut *q;
    if banner.time_left > 0.0 {
        text.0 = banner.text.clone();
        color.0 = banner.color.with_alpha(banner.time_left.min(1.0));
    } else {
        text.0.clear();
    }
}

fn update_target(
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    remotes: Query<(&RemoteActor, &Transform)>,
    mut label: Single<(&mut Text, &mut TextFont), With<TargetText>>,
    fonts: Option<Res<crate::inventory_ui::UiFonts>>,
    names: Option<Res<crate::client::PlayerNames>>,
) {
    // Названия врагов по-русски — нужен шрифт с кириллицей.
    let (label, font) = &mut *label;
    if let Some(fonts) = fonts {
        font.font = fonts.regular.clone().into();
    }
    let Ok(cam) = camera.single() else { return };
    let eye = cam.translation();
    let fwd = cam.forward();
    let best = remotes
        .iter()
        .filter_map(|(r, t)| {
            let center = t.translation + Vec3::Y * 1.2;
            let d = center - eye;
            let dist = d.length();
            (dist < 10.0 && fwd.dot(d / dist) > 0.96).then_some((r, dist))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1));
    label.0 = match best {
        Some((r, _)) => {
            let name = if r.kind == ActorKind::Player { names.as_ref().map_or_else(|| r.kind.name_ru().to_string(), |n| n.of(r.id)) } else { r.kind.name_ru().to_string() };
            format!("{name}  {:.0} / {:.0}", r.fighter.hp, r.fighter.max_hp)
        }
        None => String::new(),
    };
}
