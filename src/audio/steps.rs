//! Шаги (свои и чужие — по пройденному пути), приземление после прыжка, тяжёлое дыхание.

use super::surface::{Surface, Surfaces};
use super::{AudioSet, Sfx, SfxRng};
use crate::client::RemoteActor;
use crate::fight::LocalFighter;
use crate::player::LocalPlayer;
use crate::protocol::ActorKind;
use bevy::prelude::*;
use std::collections::HashMap;

pub struct StepsPlugin;

impl Plugin for StepsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (local_steps, remote_steps, tired_breath).before(AudioSet));
    }
}

/// Длина шага (м) при скорости `speed` (м/с).
pub fn stride(speed: f32) -> f32 {
    (0.55 + 0.085 * speed).clamp(0.55, 1.15)
}

fn surface(surfaces: &Surfaces, feet: Vec3) -> Surface {
    surfaces.get().map(|m| m.surface_at(feet)).unwrap_or(Surface::Dirt)
}

/// Громкость шага от поверхности: по земле и траве тише, по доскам громче.
fn surface_gain(s: Surface) -> f32 {
    match s {
        Surface::Stone => 1.0,
        Surface::Wood => 1.1,
        Surface::Dirt => 0.9,
        Surface::Grass => 0.75,
        Surface::Cloth => 0.6,
        Surface::Tile => 0.9,
        Surface::Water => 1.0,
    }
}

#[derive(Default)]
struct LocalStepState {
    dist: f32,
    last: Option<Vec3>,
    was_grounded: bool,
    fall_speed: f32,
}

fn local_steps(surfaces: Res<Surfaces>, mut out: MessageWriter<Sfx>, q: Query<(&LocalPlayer, &Transform)>, mut st: Local<LocalStepState>) {
    let Ok((p, t)) = q.single() else { return };
    let feet = t.translation;
    let last = st.last.replace(feet).unwrap_or(feet);
    let moved = Vec2::new(feet.x - last.x, feet.z - last.z).length();
    // Телепорт (возрождение, поправка сервера) — не шаги.
    if moved > 3.0 {
        st.dist = 0.0;
        return;
    }
    if p.grounded && !st.was_grounded && st.fall_speed > 4.5 {
        let s = surface(&surfaces, feet);
        let g = ((st.fall_speed - 4.0) / 6.0).clamp(0.3, 1.0);
        out.write(Sfx::at("land", feet).gain(0.55 * g).range(2.0, 30.0));
        out.write(Sfx::at(s.step_sound(), feet).gain(0.6 * surface_gain(s)).range(2.0, 30.0));
        st.dist = 0.0;
    }
    st.was_grounded = p.grounded;
    st.fall_speed = if p.grounded { 0.0 } else { -p.velocity.y };
    if !p.grounded {
        return;
    }
    st.dist += moved;
    let speed = Vec2::new(p.velocity.x, p.velocity.z).length();
    if speed < 0.4 {
        // Остановились: следующий шаг — сразу, как двинемся.
        st.dist = st.dist.min(stride(speed) * 0.6);
        return;
    }
    if st.dist >= stride(speed) {
        st.dist = 0.0;
        let s = surface(&surfaces, feet);
        let gain = if p.crouching {
            0.12
        } else if p.sprinting {
            0.5
        } else {
            0.3
        };
        out.write(Sfx::at(s.step_sound(), feet).gain(gain * surface_gain(s)).range(2.0, 30.0).spread(0.08));
    }
}

#[derive(Default)]
struct RemoteStep {
    last: Vec3,
    dist: f32,
    count: u32,
}

fn remote_steps(
    surfaces: Res<Surfaces>,
    ears: Res<super::Ears>,
    time: Res<Time>,
    mut out: MessageWriter<Sfx>,
    q: Query<(Entity, &RemoteActor, &Transform)>,
    mut state: Local<HashMap<Entity, RemoteStep>>,
) {
    let dt = time.delta_secs().max(1e-4);
    let mut alive = Vec::new();
    for (e, r, t) in &q {
        alive.push(e);
        let feet = t.translation;
        let s = state.entry(e).or_insert(RemoteStep { last: feet, dist: 0.0, count: 0 });
        let moved = Vec2::new(feet.x - s.last.x, feet.z - s.last.z).length();
        s.last = feet;
        if moved > 3.0 || r.fighter.is_dead() {
            s.dist = 0.0;
            continue;
        }
        s.dist += moved;
        let speed = moved / dt;
        if speed < 0.3 {
            s.dist = s.dist.min(0.3);
            continue;
        }
        if s.dist < stride(speed) {
            continue;
        }
        s.dist = 0.0;
        s.count += 1;
        if ears.valid && feet.distance(ears.pos) > 32.0 {
            continue;
        }
        let surf = surface(&surfaces, feet);
        let gain = (0.22 + 0.06 * speed).min(0.55) * surface_gain(surf);
        out.write(Sfx::at(surf.step_sound(), feet).gain(gain).range(2.0, 32.0).spread(0.08));
        // Воины в доспехах звякают кольчугой и ремнями.
        let armored = !matches!(r.kind, ActorKind::Player | ActorKind::HunArcher | ActorKind::GothWarrior);
        if armored && s.count % 2 == 0 {
            out.write(Sfx::at("gear", feet + Vec3::Y).gain(0.18 + 0.04 * speed).range(1.5, 20.0));
        }
    }
    state.retain(|e, _| alive.contains(e));
}

/// Выдохся — слышно своё тяжёлое дыхание.
fn tired_breath(time: Res<Time>, mut out: MessageWriter<Sfx>, q: Query<&LocalFighter>, mut next: Local<f32>, mut rng: ResMut<SfxRng>) {
    let Ok(f) = q.single() else { return };
    if f.fighter.is_dead() || f.fighter.max_stamina <= 0.0 {
        return;
    }
    let frac = f.fighter.stamina / f.fighter.max_stamina;
    let now = time.elapsed_secs();
    if frac > 0.3 {
        *next = now.max(*next);
        return;
    }
    if now < *next {
        return;
    }
    let tired = 1.0 - frac / 0.3;
    out.write(Sfx::ui("breath_tired").gain(0.25 + 0.35 * tired).spread(0.04));
    *next = now + rng.range(1.05, 1.3) - 0.25 * tired;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stride_grows_with_speed() {
        assert!(stride(crate::player::SPRINT_SPEED) > stride(crate::player::WALK_SPEED));
        assert!(stride(0.0) >= 0.55);
    }
}
