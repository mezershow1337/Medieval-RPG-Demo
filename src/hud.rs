//! Простой интерфейс: прицел, статус подключения и подсказка по управлению.
//! Пока на английском: встроенный шрифт Bevy не содержит кириллицы.
//! Позже добавим свой шрифт с русскими буквами.

use crate::client::NetState;
use crate::player::LocalPlayer;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

pub struct HudPlugin;

#[derive(Component)]
struct StatusText;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, setup_hud)
            .add_systems(Update, update_status);
    }
}

fn setup_hud(mut commands: Commands) {
    // Прицел.
    commands
        .spawn(Node { width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })
        .with_children(|p| {
            p.spawn((Node { width: px(4), height: px(4), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8))));
        });
    commands.spawn((
        StatusText,
        Text::new("Connecting..."),
        TextFont { font_size: bevy::text::FontSize::Px(16.0), ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, left: px(12), top: px(10), ..default() },
    ));
    commands.spawn((
        Text::new("Click - capture mouse | Esc - release\nWASD - move | Shift - sprint | Space - jump | Ctrl/C - crouch"),
        TextFont { font_size: bevy::text::FontSize::Px(14.0), ..default() },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.75)),
        Node { position_type: PositionType::Absolute, left: px(12), bottom: px(10), ..default() },
    ));
}

fn update_status(
    state: Res<NetState>,
    client: Option<Res<bevy_renet::RenetClient>>,
    diagnostics: Res<DiagnosticsStore>,
    remotes: Query<(), With<crate::client::RemotePlayer>>,
    player: Query<&Transform, With<LocalPlayer>>,
    mut text: Single<&mut Text, With<StatusText>>,
) {
    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let addr = state.server_addr.map(|a| a.to_string()).unwrap_or_default();
    let status = match (&client, state.my_id, &state.error) {
        (_, _, Some(err)) => format!("Network error: {err}"),
        (Some(c), Some(_), _) if c.is_connected() => {
            let ping = c.rtt() * 1000.0;
            format!("Connected to {addr} | players: {} | ping {ping:.0} ms", remotes.iter().count() + 1)
        }
        (Some(c), _, _) if c.is_disconnected() => format!("Disconnected from {addr}"),
        _ => format!("Connecting to {addr}..."),
    };
    let pos = player.single().map(|t| format!(" | pos {:.1} {:.1} {:.1}", t.translation.x, t.translation.y, t.translation.z)).unwrap_or_default();
    text.0 = format!("{status}\nFPS {fps:.0}{pos}");
}
