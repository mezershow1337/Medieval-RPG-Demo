//! Музыка: выбирается по месту, времени суток и бою, сменяется плавным перекрёстным
//! затуханием. Спокойная музыка звучит волнами — пьеса, затем тишина, как в жизни.

use super::ambience::{Scene, TimeOfDay};
use super::combat_sfx::Fighters;
use super::{Bank, Channel, Ears, Emitter, SfxRng, spawn_loop};
use bevy::prelude::*;

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicDirector>()
            .add_systems(Update, (choose, crossfade).chain().before(super::AudioSet));
    }
}

/// Сколько длится пьеса (с) — из `bank_gen::MUSIC`.
fn track_len(name: &str) -> f32 {
    super::bank_gen::MUSIC
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .unwrap_or(120.0)
}

/// Что играет и что должно играть.
#[derive(Resource, Default)]
pub struct MusicDirector {
    /// Желаемая пьеса (`None` — тишина).
    pub want: Option<&'static str>,
    playing: Vec<Track>,
    /// Пауза между волнами спокойной музыки — до этого времени молчим.
    rest_until: f32,
    /// Когда закончился бой (музыка боя держится ещё немного).
    calm_since: f32,
    in_combat: bool,
}

struct Track {
    name: &'static str,
    entity: Entity,
    started: f32,
    target: f32,
}

pub fn calm_track(scene: &Scene) -> &'static str {
    if scene.sacred > 0.5 {
        return "sacred";
    }
    match scene.time.unwrap_or(TimeOfDay::Day) {
        TimeOfDay::Night => "night",
        TimeOfDay::Evening => "evening",
        TimeOfDay::Day if scene.city > 0.5 && scene.indoor < 0.5 => "city",
        TimeOfDay::Day => "evening",
    }
}

fn choose(
    time: Res<Time>,
    scene: Res<Scene>,
    fighters: Res<Fighters>,
    ears: Res<Ears>,
    mut dir: ResMut<MusicDirector>,
    mut rng: ResMut<SfxRng>,
) {
    if !ears.valid {
        return;
    }
    let now = time.elapsed_secs();
    let fighting = now - fighters.last_fight < 8.0 || now - fighters.hostile_near < 5.0;
    if fighting {
        dir.in_combat = true;
        dir.calm_since = now;
    } else if dir.in_combat && now - dir.calm_since > 10.0 {
        dir.in_combat = false;
        // После боя — немного тишины.
        dir.rest_until = now + rng.range(8.0, 20.0);
    }
    if dir.in_combat {
        dir.want = Some("combat");
        return;
    }
    if now < dir.rest_until {
        dir.want = None;
        return;
    }
    let calm = calm_track(&scene);
    // Пьеса доиграла — тишина на минуту-другую (в храме пение почти без перерыва).
    if let Some(t) = dir
        .playing
        .iter()
        .find(|t| t.target > 0.0 && t.name == calm)
        && now - t.started > track_len(calm) - 5.0
    {
        dir.rest_until = now
            + if calm == "sacred" {
                rng.range(6.0, 15.0)
            } else {
                rng.range(45.0, 110.0)
            };
        dir.want = None;
        return;
    }
    dir.want = Some(calm);
}

fn crossfade(
    mut commands: Commands,
    time: Res<Time>,
    bank: Option<Res<Bank>>,
    ears: Res<Ears>,
    mut dir: ResMut<MusicDirector>,
    mut q: Query<&mut Emitter>,
) {
    let Some(bank) = bank else { return };
    let now = time.elapsed_secs();
    let want = dir.want;
    if let Some(name) = want
        && !dir.playing.iter().any(|t| t.name == name && t.target > 0.0)
        && let Some(h) = bank.music.get(name)
    {
        let gain = match name {
            "combat" => 0.8,
            "sacred" => 0.75,
            _ => 0.65,
        };
        let em = Emitter {
            at: None,
            follow: None,
            gain,
            near: 1.0,
            range: 1.0,
            channel: Channel::Music,
            fade: 0.0,
        };
        let entity = spawn_loop(&mut commands, h.clone(), em, &ears);
        dir.playing.push(Track {
            name,
            entity,
            started: now,
            target: 1.0,
        });
    }
    let into_combat = want == Some("combat");
    let dt = time.delta_secs();
    for t in &mut dir.playing {
        if Some(t.name) != want {
            t.target = 0.0;
        }
        // В бой — быстро, из боя и между пьесами — медленно.
        let secs = if t.target > 0.0 {
            if t.name == "combat" { 1.5 } else { 6.0 }
        } else if into_combat {
            1.5
        } else {
            5.0
        };
        if let Ok(mut em) = q.get_mut(t.entity) {
            let step = dt / secs;
            em.fade = if em.fade < t.target {
                (em.fade + step).min(t.target)
            } else {
                (em.fade - step).max(t.target)
            };
        }
    }
    dir.playing.retain(|t| {
        let silent = q
            .get(t.entity)
            .map(|em| em.fade <= 0.0 && t.target == 0.0)
            .unwrap_or(true);
        if silent {
            commands.entity(t.entity).despawn();
        }
        !silent
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calm_music_by_place() {
        let temple = Scene {
            sacred: 1.0,
            ..default()
        };
        assert_eq!(calm_track(&temple), "sacred");
        let mut street = Scene {
            city: 1.0,
            ..default()
        };
        street.time = Some(TimeOfDay::Day);
        assert_eq!(calm_track(&street), "city");
        street.time = Some(TimeOfDay::Night);
        assert_eq!(calm_track(&street), "night");
        for (n, len) in super::super::bank_gen::MUSIC {
            assert!(*len > 30.0, "{n} слишком короткая");
        }
    }
}
