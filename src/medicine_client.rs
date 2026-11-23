//! Клиент: состояние тела на экране (раны, жар, яды, боль).
//! Без знаний видны признаки, со знаниями — диагнозы (см. `medicine::symptoms`).

use crate::inventory_ui::UiFonts;
use crate::lore_client::MyKnowledge;
use crate::medicine::{Alarm, Body, symptoms};
use bevy::prelude::*;

pub struct MedicineClientPlugin;

impl Plugin for MedicineClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MyBody::default())
            .add_systems(Startup, setup_body_panel.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, update_body_panel);
    }
}

/// Своё тело (присылает сервер).
#[derive(Resource, Default)]
pub struct MyBody(pub Body);

#[derive(Component)]
struct BodyPanel;

fn setup_body_panel(mut commands: Commands) {
    commands.spawn((
        BodyPanel,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(118),
            max_width: px(540),
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            padding: crate::ui_theme::Skin::Leather.padding(),
            display: Display::None,
            ..default()
        },
        crate::ui_theme::theme().skin(crate::ui_theme::Skin::Leather),
    ));
}

fn alarm_color(a: Alarm) -> Color {
    match a {
        Alarm::Mild => Color::srgb(0.92, 0.86, 0.7),
        Alarm::Serious => Color::srgb(1.0, 0.62, 0.3),
        Alarm::Deadly => Color::srgb(1.0, 0.3, 0.25),
    }
}

fn update_body_panel(
    mut commands: Commands,
    body: Res<MyBody>,
    knowledge: Res<MyKnowledge>,
    fonts: Res<UiFonts>,
    mut panel: Query<(Entity, &mut Node), With<BodyPanel>>,
    mut shown: Local<Vec<(String, Alarm)>>,
) {
    if !body.is_changed() && !knowledge.is_changed() {
        return;
    }
    let lines = symptoms(&body.0, &knowledge.0);
    if *shown == lines {
        return;
    }
    let Ok((panel, mut node)) = panel.single_mut() else {
        return;
    };
    // Пустая кожаная плашка не нужна: показываем, только когда есть что сказать.
    node.display = if lines.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    commands.entity(panel).despawn_children();
    commands.entity(panel).with_children(|p| {
        for (line, alarm) in &lines {
            p.spawn((
                Text::new(line.clone()),
                TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(16.0), ..default() },
                TextColor(alarm_color(*alarm)),
                TextShadow::default(),
            ));
        }
    });
    *shown = lines;
}
