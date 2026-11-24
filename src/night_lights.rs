//! Живой свет от факелов в руках жителей (ночные стражники и патрули).
//! Свет — дочерняя сущность предмета в руке: когда житель убирает факел, он исчезает вместе с ним.
//! Далёкие огни гасит общий `graphics::cull_far_lights` (по компоненту `Flicker`).

use crate::life_client::{LifeSystems, Prop, PropSlot, ShownProp};
use bevy::prelude::*;

pub struct NightLightsPlugin;

impl Plugin for NightLightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, torch_lights.after(LifeSystems));
    }
}

/// Сила света факела (люмены) и дальность, м.
const TORCH_LM: f32 = 30_000.0;
const TORCH_RANGE: f32 = 10.0;

fn torch_lights(mut commands: Commands, slots: Query<(Entity, &ShownProp), (With<PropSlot>, Changed<ShownProp>)>) {
    for (e, shown) in &slots {
        if shown.0 != Some(Prop::Torch) {
            continue;
        }
        // Пламя — на конце древка (ось рукояти +Y, шар огня на 0.36 м).
        commands.entity(e).with_child((
            PointLight { color: Color::srgb(1.0, 0.55, 0.25), intensity: TORCH_LM, range: TORCH_RANGE, radius: 0.07, shadow_maps_enabled: false, ..default() },
            Transform::from_xyz(0.0, 0.42, 0.0),
            crate::graphics::Flicker { base: TORCH_LM, phase: (e.to_bits() % 997) as f32 * 0.37 },
        ));
    }
}
