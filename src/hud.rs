//! Интерфейс: прицел, здоровье, надписи боя, статус подключения, подсказка по управлению.
//! Пока на английском: встроенный шрифт Bevy не содержит кириллицы
//! (русский шрифт добавляется в треде про инвентарь).

use crate::client::{NetState, RemoteActor};
use crate::fight::{Banner, DamageFlash, LocalFighter};
use crate::player::{LocalPlayer, PlayerCamera};
use crate::protocol::ActorKind;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::text::FontSize;

pub struct HudPlugin;

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

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, setup_hud)
            .add_systems(Update, (update_status, update_health, update_banner, update_target));
    }
}

fn text(s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn setup_hud(mut commands: Commands) {
    // Красная вспышка при уроне (на весь экран).
    commands.spawn((FlashOverlay, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::NONE)));
    // Прицел.
    commands
        .spawn(Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })
        .with_children(|p| {
            p.spawn((Node { width: px(4), height: px(4), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8))));
        });
    // Надпись по центру (парирование, смерть).
    commands
        .spawn(Node { width: percent(100), position_type: PositionType::Absolute, top: percent(30), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((BannerText, text("", 34.0, Color::WHITE)));
        });
    // Цель (враг перед вами).
    commands
        .spawn(Node { width: percent(100), position_type: PositionType::Absolute, top: px(14), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((TargetText, text("", 18.0, Color::srgb(1.0, 0.9, 0.8))));
        });
    // Здоровье внизу по центру.
    commands
        .spawn(Node {
            width: percent(100),
            position_type: PositionType::Absolute,
            bottom: px(96),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((Node { width: px(300), height: px(18), border: UiRect::all(px(2)), ..default() }, BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.8)), BackgroundColor(Color::srgba(0.1, 0.0, 0.0, 0.6))))
                .with_children(|bar| {
                    bar.spawn((HealthFill, Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(Color::srgb(0.7, 0.08, 0.08))));
                    bar.spawn((
                        HealthText,
                        text("100", 13.0, Color::WHITE),
                        Node { position_type: PositionType::Absolute, left: px(6), ..default() },
                    ));
                });
        });
    commands.spawn((
        StatusText,
        text("Connecting...", 16.0, Color::WHITE),
        Node { position_type: PositionType::Absolute, left: px(12), top: px(10), ..default() },
    ));
    commands.spawn((
        text(
            "Click - capture mouse | Esc - release\nWASD - move | Shift - sprint | Space - jump | Ctrl/C - crouch\nLMB - attack | RMB (hold) - guard: parry with weapon / block with shield\nAfter a successful parry press LMB for a fast riposte",
            14.0,
            Color::srgba(1.0, 1.0, 1.0, 0.75),
        ),
        Node { position_type: PositionType::Absolute, left: px(12), bottom: px(10), ..default() },
    ));
}

fn update_status(
    state: Res<NetState>,
    client: Option<Res<bevy_renet::RenetClient>>,
    diagnostics: Res<DiagnosticsStore>,
    remotes: Query<&RemoteActor>,
    player: Query<&Transform, With<LocalPlayer>>,
    mut text: Single<&mut Text, With<StatusText>>,
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
    text.0 = format!("{status}\nFPS {fps:.0}{pos}");
}

fn update_health(
    local: Query<&LocalFighter>,
    flash: Res<DamageFlash>,
    mut fill: Single<&mut Node, With<HealthFill>>,
    mut label: Single<&mut Text, With<HealthText>>,
    mut overlay: Single<&mut BackgroundColor, With<FlashOverlay>>,
) {
    if let Ok(f) = local.single() {
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
    mut label: Single<&mut Text, With<TargetText>>,
) {
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
            let name = match r.kind {
                ActorKind::Player => "Player",
                ActorKind::Swordsman => "Knight (sword & shield)",
                ActorKind::HeavyKnight => "Heavy Knight",
            };
            format!("{name}  {:.0} / {:.0}", r.fighter.hp, r.fighter.max_hp)
        }
        None => String::new(),
    };
}
