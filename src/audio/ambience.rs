//! Окружение: петли (море, ветер, город, рынок, степь, ночь, комната, базилика), огонь
//! очагов и горнов рядом, редкие голоса округи (чайки, собаки, овцы, осёл, семантрон).
//! Громкость каждой петли плавно следует за тем, где стоит слушатель.

use super::surface::Surfaces;
use super::{Bank, Channel, Ears, Emitter, Sfx, SfxRng, spawn_loop};
use crate::level::places::{self, PlaceKind};
use bevy::prelude::*;
use std::collections::HashMap;

pub struct AmbiencePlugin;

impl Plugin for AmbiencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scene>().add_systems(Update, (sense_scene, layers, fires, critters).chain().before(super::AudioSet));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    Day,
    Evening,
    Night,
}

pub fn time_of_day() -> TimeOfDay {
    time_of_day_of(&crate::graphics::mood())
}

/// Время суток по нынешнему свету: высота самого солнца (`sun_sky`; главный свет `sun`
/// ночью — луна, она бывает высоко над горизонтом).
pub fn time_of_day_of(m: &crate::graphics::Mood) -> TimeOfDay {
    let y = m.sun_sky.normalize_or_zero().y;
    if y > 0.35 {
        TimeOfDay::Day
    } else if y > -0.05 {
        TimeOfDay::Evening
    } else {
        TimeOfDay::Night
    }
}

/// Что вокруг слушателя (0..1, сглажено). Читают и музыка, и петли.
#[derive(Resource, Default, Clone, Debug)]
pub struct Scene {
    pub sea: f32,
    pub indoor: f32,
    pub city: f32,
    pub market: f32,
    pub sacred: f32,
    pub height: f32,
    pub time: Option<TimeOfDay>,
    /// Ближайшее место, если мы в его пределах.
    pub place: Option<PlaceKind>,
    pub(super) next: f32,
    pub(super) target: [f32; 6],
}

fn sense_scene(time: Res<Time>, ears: Res<Ears>, surfaces: Res<Surfaces>, mood: Option<Res<crate::daylight::CurrentMood>>, mut scene: ResMut<Scene>) {
    if !ears.valid {
        return;
    }
    let now = time.elapsed_secs();
    if now >= scene.next {
        scene.next = now + 0.3;
        // Время суток — по часам мира (свет считает `daylight`), а не раз навсегда при входе:
        // раньше ночные сверчки и ночная музыка так и не включались, днём оставалось утро.
        scene.time = Some(mood.as_ref().map_or_else(time_of_day, |m| time_of_day_of(&m.0)));
        let p = ears.pos;
        let mut t = [0.0f32; 6];
        if let Some(map) = surfaces.get() {
            // Ближайшее море: по кругу вокруг слушателя.
            let mut sea_d = f32::MAX;
            'outer: for r in [6.0, 15.0, 30.0, 55.0, 90.0, 140.0] {
                for k in 0..16 {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    if map.is_sea(p.x + a.cos() * r, p.z + a.sin() * r) {
                        sea_d = r;
                        break 'outer;
                    }
                }
            }
            t[0] = if sea_d < f32::MAX { (1.0 - sea_d / 160.0).clamp(0.0, 1.0).powf(1.3) } else { 0.0 };
            t[1] = if map.roofed(p + Vec3::Y * 0.2) { 1.0 } else { 0.0 };
            let ground = map.ground_y(p.x, p.z).unwrap_or(p.y - 1.65);
            t[5] = ((p.y - 1.65 - ground - 4.0) / 8.0).clamp(0.0, 1.0);
        }
        t[2] = if crate::level::inside_city(p.x, p.z) { 1.0 } else { 0.0 };
        let mut place = None;
        let mut best = f32::MAX;
        for pl in places::places() {
            let d = pl.pos.distance(p);
            match pl.kind {
                PlaceKind::Agora | PlaceKind::PortMarket => t[3] = t[3].max((1.0 - (d - pl.radius).max(0.0) / 35.0).clamp(0.0, 1.0)),
                PlaceKind::Basilica | PlaceKind::Church | PlaceKind::Baptistery if d < pl.radius + 4.0 => t[4] = 1.0,
                _ => {}
            }
            if d < pl.radius && d < best {
                best = d;
                place = Some(pl.kind);
            }
        }
        scene.place = place;
        scene.target = t;
    }
    let k = 1.0 - (-time.delta_secs() * 1.2).exp();
    let t = scene.target;
    scene.sea += (t[0] - scene.sea) * k;
    scene.indoor += (t[1] - scene.indoor) * k * 2.0;
    scene.city += (t[2] - scene.city) * k;
    scene.market += (t[3] - scene.market) * k;
    // В храме «священность» только под крышей.
    scene.sacred += (t[4] * t[1] - scene.sacred) * k;
    scene.height += (t[5] - scene.height) * k;
}

/// Петли окружения: создаются один раз, громкость меняется плавно.
#[derive(Default)]
struct Layers(HashMap<&'static str, Entity>);

fn layer_targets(s: &Scene) -> [(&'static str, f32); 9] {
    let out = 1.0 - 0.8 * s.indoor.clamp(0.0, 1.0);
    let tod = s.time.unwrap_or(TimeOfDay::Day);
    let (day, evening, night) = match tod {
        TimeOfDay::Day => (1.0, 0.0, 0.0),
        TimeOfDay::Evening => (0.75, 1.0, 0.0),
        TimeOfDay::Night => (0.0, 0.0, 1.0),
    };
    let outside = 1.0 - s.city;
    [
        ("sea", 0.9 * s.sea * out),
        ("wind", (0.25 + 0.35 * outside + 0.2 * s.height) * out),
        ("wind_strong", 0.6 * s.height * out),
        ("city", 0.55 * s.city * (1.0 - s.market) * (day + 0.2 * night) * out),
        ("market", 0.75 * s.market * day * out),
        ("night", (night * 0.8 + evening * (0.25 + 0.25 * outside)) * out),
        ("steppe", 0.55 * outside * day * out),
        ("room", 0.35 * s.indoor * (1.0 - s.sacred)),
        ("basilica", 0.7 * s.sacred),
    ]
}

fn layers(mut commands: Commands, bank: Option<Res<Bank>>, ears: Res<Ears>, scene: Res<Scene>, time: Res<Time>, mut live: Local<Layers>, mut q: Query<&mut Emitter>) {
    let Some(bank) = bank else { return };
    if !ears.valid {
        return;
    }
    let k = 1.0 - (-time.delta_secs() * 0.8).exp();
    for (name, target) in layer_targets(&scene) {
        let e = match live.0.get(name) {
            Some(e) => *e,
            None => {
                let Some(h) = bank.ambient.get(name) else { continue };
                let em = Emitter { at: None, follow: None, gain: 1.0, near: 1.0, range: 1.0, channel: Channel::Ambient, fade: 0.0 };
                let e = spawn_loop(&mut commands, h.clone(), em, &ears);
                live.0.insert(name, e);
                continue;
            }
        };
        if let Ok(mut em) = q.get_mut(e) {
            em.fade += (target - em.fade) * k;
        }
    }
}

/// Треск огня в ближайших очагах и горнах.
fn fires(mut commands: Commands, bank: Option<Res<Bank>>, ears: Res<Ears>, surfaces: Res<Surfaces>, mut live: Local<HashMap<usize, Entity>>, mut next: Local<f32>, time: Res<Time>) {
    let (Some(bank), Some(map)) = (bank, surfaces.get()) else { return };
    if !ears.valid || time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 0.5;
    let Some(h) = bank.ambient.get("fire") else { return };
    for (i, f) in map.fires.iter().enumerate() {
        let d = f.distance(ears.pos);
        match live.get(&i) {
            None if d < 16.0 => {
                let em = Emitter { at: Some(*f + Vec3::Y * 0.3), follow: None, gain: 0.55, near: 1.2, range: 16.0, channel: Channel::Ambient, fade: 1.0 };
                live.insert(i, spawn_loop(&mut commands, h.clone(), em, &ears));
            }
            Some(e) if d > 19.0 => {
                commands.entity(*e).despawn();
                live.remove(&i);
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct CritterClock {
    next: HashMap<&'static str, f32>,
}

/// Редкие голоса округи: чайки над морем, собаки и стук ремёсел в городе, скот у ферм,
/// семантрон у храма.
#[allow(clippy::too_many_arguments)]
fn critters(time: Res<Time>, ears: Res<Ears>, scene: Res<Scene>, surfaces: Res<Surfaces>, mut rng: ResMut<SfxRng>, mut clock: Local<CritterClock>, mut out: MessageWriter<Sfx>) {
    if !ears.valid {
        return;
    }
    let now = time.elapsed_secs();
    let p = ears.pos;
    let out_k = 1.0 - 0.8 * scene.indoor;
    let tod = scene.time.unwrap_or(TimeOfDay::Day);
    let awake = tod != TimeOfDay::Night;
    let mut due = |name: &'static str, lo: f32, hi: f32, rng: &mut SfxRng| -> bool {
        let t = clock.next.entry(name).or_insert(now + rng.range(lo * 0.3, hi * 0.6));
        if now >= *t {
            *t = now + rng.range(lo, hi);
            true
        } else {
            false
        }
    };
    let around = |rng: &mut SfxRng, r0: f32, r1: f32| {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let r = rng.range(r0, r1);
        p + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
    };
    // Чайки: над морем, в стороне берега.
    if awake && scene.sea > 0.12 && due("gull", 5.0, 16.0, &mut rng) {
        let mut at = around(&mut rng, 20.0, 60.0);
        if let Some(map) = surfaces.get() {
            for _ in 0..6 {
                let c = around(&mut rng, 25.0, 80.0);
                if map.is_sea(c.x, c.z) {
                    at = c;
                    break;
                }
            }
        }
        at.y = p.y + rng.range(10.0, 25.0);
        out.write(Sfx::at("gull", at).gain(0.55 * out_k * scene.sea.sqrt()).range(10.0, 140.0).spread(0.08).channel(Channel::Ambient));
    }
    if scene.city > 0.5 {
        if due("dog_bark", 18.0, 45.0, &mut rng) {
            let at = around(&mut rng, 25.0, 70.0);
            out.write(Sfx::at("dog_bark", at).gain(0.4 * out_k).range(8.0, 110.0).spread(0.1).channel(Channel::Ambient));
        }
        if awake && due("far_hammer", 20.0, 50.0, &mut rng) {
            let at = around(&mut rng, 30.0, 80.0);
            out.write(Sfx::at("far_hammer", at).gain(0.35 * out_k).range(8.0, 110.0).channel(Channel::Ambient));
        }
        if awake && due("cart", 40.0, 90.0, &mut rng) {
            let at = around(&mut rng, 20.0, 50.0);
            out.write(Sfx::at("cart", at).gain(0.4 * out_k).range(6.0, 70.0).channel(Channel::Ambient));
        }
        if tod == TimeOfDay::Day && due("rooster", 60.0, 150.0, &mut rng) {
            let at = around(&mut rng, 40.0, 90.0);
            out.write(Sfx::at("rooster", at).gain(0.35 * out_k).range(10.0, 130.0).channel(Channel::Ambient));
        }
    }
    // Скот у ферм, сёл и кочевых станов.
    for (name, spot) in [("sheep", places::KLEROS_FARM), ("sheep", places::GOTH_VILLAGE), ("sheep", places::STEPPE_CAMP), ("donkey", places::KLEROS_FARM), ("donkey", places::ALAN_CAMP)] {
        let flat = Vec2::new(spot.x - p.x, spot.z - p.z).length();
        if flat < 140.0 && awake && due(if name == "sheep" { "sheep" } else { "donkey" }, 10.0, 28.0, &mut rng) {
            let at = Vec3::new(spot.x + rng.range(-20.0, 20.0), p.y, spot.z + rng.range(-20.0, 20.0));
            out.write(Sfx::at(name, at).gain(0.45 * out_k).range(8.0, 150.0).spread(0.1).channel(Channel::Ambient));
        }
    }
    // Семантрон: монах сзывает на службу ударами по доске.
    if let Some(b) = places::places().iter().find(|pl| pl.kind == PlaceKind::Basilica)
        && b.pos.distance(p) < 160.0
        && due("semantron", 140.0, 280.0, &mut rng)
    {
        out.write(Sfx::at("semantron", b.pos + Vec3::Y * 4.0).gain(0.8).range(15.0, 200.0).spread(0.02).channel(Channel::Ambient));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_follow_scene() {
        let sea = Scene { sea: 1.0, ..default() };
        let t = layer_targets(&sea);
        assert!(t.iter().find(|(n, _)| *n == "sea").unwrap().1 > 0.8);
        let inside = Scene { sea: 1.0, indoor: 1.0, ..default() };
        let t2 = layer_targets(&inside);
        assert!(t2.iter().find(|(n, _)| *n == "sea").unwrap().1 < 0.3);
        assert!(t2.iter().find(|(n, _)| *n == "room").unwrap().1 > 0.2);
        for (name, _) in t {
            assert!(super::super::bank_gen::AMBIENT.contains(&name), "нет петли {name}");
        }
    }

    #[test]
    fn time_of_day_follows_the_clock() {
        use crate::graphics::mood_at;
        use crate::world_clock::DAY;
        let at = |h: f64| time_of_day_of(&mood_at(DAY * 2.0 + h * 3600.0));
        assert_eq!(at(13.0), TimeOfDay::Day);
        // Ночью главный свет — луна, но ночь остаётся ночью.
        assert_eq!(at(0.5), TimeOfDay::Night);
        assert_eq!(at(3.0), TimeOfDay::Night);
        // За сутки — и вечер (сумерки).
        let hours: Vec<TimeOfDay> = (0..96).map(|q| at(q as f64 / 4.0)).collect();
        assert!(hours.contains(&TimeOfDay::Evening));
    }
}
