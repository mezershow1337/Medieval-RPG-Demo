//! Смена дня и ночи и погода на экране (клиент).
//!
//! Время суток и погоду дают часы мира (`world_clock.rs`) — одинаково у всех игроков.
//! Отсюда по ним ведутся: солнце и луна (направление, цвет, сила света), небо (купол,
//! отражения в море, дым), дымка и туман, цветокоррекция, рассеянный свет и карта
//! окружения (её пересчитываем в фоне, когда небо заметно изменилось).
//!
//! `MRPG_TIME=day|golden` замораживает время для снимков (см. `graphics::frozen_time`).

use crate::graphics::{HazeVolume, Mood, SkyDome, SkyMaterial, SunLight, mood_at};
use crate::sky::{SkyEnvironment, SkyUniform, WaterMaterial};
use crate::smoke::SmokeMaterial;
use bevy::light::{EnvironmentMapLight, FogVolume};
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};

pub struct DaylightPlugin;

impl Plugin for DaylightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnvBake>().add_systems(Update, (drive_daylight, rebake_environment).chain());
    }
}

/// Свет и небо сейчас (для других систем: дождь, факелы, свечение окон).
#[derive(Resource, Clone, Copy)]
pub struct CurrentMood(pub Mood);

/// Время, по которому рисуется небо (часы мира или застывшее время для снимков).
pub fn sky_time(clock: &crate::world_clock::WorldClock) -> f64 {
    crate::graphics::frozen_time().unwrap_or(clock.game)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_daylight(
    mut commands: Commands,
    time: Res<Time>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
    settings: Res<crate::settings::Settings>,
    mut next: Local<f32>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut haze: Query<&mut FogVolume, With<HazeVolume>>,
    dome: Query<&MeshMaterial3d<SkyMaterial>, With<SkyDome>>,
    mut skies: ResMut<Assets<SkyMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
    mut smokes: ResMut<Assets<SmokeMaterial>>,
    mut cams: Query<(Option<&mut DistanceFog>, Option<&mut ColorGrading>, Option<&mut EnvironmentMapLight>), With<Camera3d>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
) {
    if crate::level::active() != crate::level::LevelKind::Chersonesus {
        return;
    }
    let Some(clock) = clock else { return };
    let now = time.elapsed_secs();
    // Пять раз в секунду: при 20-кратном времени солнце сдвигается за это время на 4 игровые
    // секунды — глазу незаметно, а материалы не перезаливаются каждый кадр.
    if now < *next {
        return;
    }
    *next = now + 0.2;
    let m = mood_at(sky_time(&clock));
    commands.insert_resource(CurrentMood(m));
    for (mut light, mut tf) in &mut sun {
        light.illuminance = m.lux;
        light.color = m.sun_color;
        *tf = Transform::from_translation(m.sun * 100.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    for mut f in &mut haze {
        f.density_factor = m.haze;
        // Ночью дымка подсвечена лишь огнями — серо-синяя, днём — тёплая.
        f.fog_color = Color::srgb(0.95, 0.9, 0.88).mix(&Color::srgb(0.55, 0.6, 0.75), m.night);
    }
    let u = m.sky_uniform();
    for h in &dome {
        if let Some(mut mat) = skies.get_mut(&h.0) {
            mat.sky = u;
        }
    }
    let ids: Vec<_> = waters.ids().collect();
    for id in ids {
        if let Some(mut w) = waters.get_mut(id) {
            w.extension.water.sky = u;
            w.extension.water.params.x = 3.0 - m.dayness;
        }
    }
    let ids: Vec<_> = smokes.ids().collect();
    for id in ids {
        if let Some(mut s) = smokes.get_mut(id) {
            s.smoke.sky = u;
        }
    }
    let fog = crate::graphics::distance_fog(&m, settings.draw_distance);
    let grading = crate::graphics::color_grading(&m);
    for (f, g, env) in &mut cams {
        if let Some(mut f) = f {
            *f = fog.clone();
        }
        if let Some(mut g) = g {
            g.global = grading.global.clone();
        }
        if let Some(mut env) = env {
            env.intensity = m.sky_light;
        }
    }
    ambient.color = m.ambient.0;
    ambient.brightness = m.ambient.1;
    clear.0 = m.fog();
}

/// Пересчёт карты окружения (свет неба и отражения) в фоне.
#[derive(Resource, Default)]
struct EnvBake {
    /// По какому небу посчитана нынешняя карта.
    baked: Option<SkyUniform>,
    pending: Option<(SkyUniform, Mutex<Receiver<(Image, Image)>>)>,
    next_check: f32,
}

/// Насколько небо изменилось с прошлого пересчёта.
fn sky_change(a: &SkyUniform, b: &SkyUniform) -> f32 {
    let col = |x: Vec4, y: Vec4| (x.truncate() - y.truncate()).abs().max_element() / (x.truncate().max_element() + y.truncate().max_element() + 0.02);
    let sun = 1.0 - a.sun.truncate().normalize().dot(b.sun.truncate().normalize());
    sun * 40.0
        + (a.sun.w - b.sun.w).abs()
        + (a.extra.x - b.extra.x).abs()
        + (a.extra.y - b.extra.y).abs()
        + (a.horizon.w - b.horizon.w).abs() * 0.5
        + col(a.horizon, b.horizon)
        + col(a.zenith, b.zenith)
}

fn rebake_environment(
    time: Res<Time>,
    current: Option<Res<CurrentMood>>,
    mut bake: ResMut<EnvBake>,
    env: Option<ResMut<SkyEnvironment>>,
    mut images: ResMut<Assets<Image>>,
    mut cams: Query<&mut EnvironmentMapLight, With<Camera3d>>,
) {
    let (Some(current), Some(mut env)) = (current, env) else { return };
    // Готова новая карта — подменяем у всех камер, старую выбрасываем.
    if let Some((sky, rx)) = bake.pending.as_ref()
        && let Ok((diffuse, specular)) = rx.lock().map_err(|_| ()).and_then(|r| r.try_recv().map_err(|_| ()))
    {
        let sky = *sky;
        let (old_d, old_s) = (env.diffuse.clone(), env.specular.clone());
        env.diffuse = images.add(diffuse);
        env.specular = images.add(specular);
        for mut light in &mut cams {
            light.diffuse_map = env.diffuse.clone();
            light.specular_map = env.specular.clone();
        }
        images.remove(&old_d);
        images.remove(&old_s);
        bake.baked = Some(sky);
        bake.pending = None;
    }
    let now = time.elapsed_secs();
    if bake.pending.is_some() || now < bake.next_check {
        return;
    }
    bake.next_check = now + 1.5;
    let sky = current.0.sky_uniform();
    let baked = *bake.baked.get_or_insert_with(|| crate::graphics::mood().sky_uniform());
    if sky_change(&sky, &baked) < 0.08 {
        return;
    }
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::sky::sky_environment_sized(&sky, 64, 0.5));
    });
    bake.pending = Some((sky, Mutex::new(rx)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_clock::DAY;

    #[test]
    fn night_is_dark_and_day_is_bright() {
        let noon = mood_at(DAY * 2.0 + 13.0 * 3600.0);
        let midnight = mood_at(DAY * 2.0 + 0.5 * 3600.0);
        let dusk = mood_at(DAY * 2.0 + 20.0 * 3600.0);
        assert!(noon.lux > 5000.0 && noon.night < 0.01 && noon.dayness > 0.9, "{noon:?}");
        assert!(midnight.night > 0.99 && midnight.lux < 600.0, "{midnight:?}");
        assert!(midnight.horizon.length() < noon.horizon.length() * 0.1);
        // На закате небо у солнца тёплое.
        assert!(dusk.horizon_sun.0.x > dusk.horizon_sun.0.z);
        // Ночью светит луна (не снизу из-под земли).
        assert!(midnight.sun.y > 0.0);
    }

    #[test]
    fn sky_changes_smoothly() {
        let mut prev = mood_at(DAY * 3.0).sky_uniform();
        for i in 1..(24 * 60) {
            let u = mood_at(DAY * 3.0 + i as f64 * 60.0).sky_uniform();
            assert!(sky_change(&u, &prev) < 0.12, "скачок неба в {} мин: {}", i, sky_change(&u, &prev));
            prev = u;
        }
    }
}

#[cfg(test)]
mod shot_points {
    /// Точки для снимков: `cargo test print_places -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_places() {
        for p in crate::level::places() {
            println!("{} {:.1} {:.1} {:.1}", p.id, p.pos.x, p.pos.y, p.pos.z);
        }
    }
}

#[cfg(test)]
mod mood_dump {
    #[test]
    #[ignore]
    fn dump_moods() {
        for h in [61.0f64, 66.75, 68.3, 71.5, 81.0, 54.5] {
            let m = crate::graphics::mood_at(h * 3600.0);
            println!("{h}: lux {:.0} sun {:?} sky {:?} amb {:.0} clouds {:.2} oc {:.2} night {:.2} day {:.2} haze {:.4} vis {:.0}", m.lux, m.sun, m.sun_sky, m.ambient.1, m.clouds, m.overcast, m.night, m.dayness, m.haze, m.visibility);
        }
    }
}
