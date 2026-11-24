//! Звуки боя: свист клинка, звон парирования, удары в щит и доспех, вскрики, падение тел,
//! тетива и стрелы. Всё выводится из того, что клиент и так знает о бойцах (снимки сервера
//! и события попаданий), поэтому удары других игроков слышны так же, как свои.

use super::{AudioSet, Sfx, SfxRng};
use crate::client::{ArrowVisual, NetState, RemoteActor};
use crate::combat::{Action, HitOutcome, ImpactTier, WeaponKind};
use crate::fight::{CombatFeedback, LocalFighter};
use crate::player::LocalPlayer;
use crate::protocol::ActorKind;
use bevy::prelude::*;
use std::collections::HashMap;

pub struct CombatSfxPlugin;

impl Plugin for CombatSfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fighters>().add_systems(Update, (track_fighters, actions, hits, arrows).chain().before(AudioSet));
    }
}

/// Кто где стоит и чем вооружён (по id бойца) — для звуков событий, у которых есть только id.
#[derive(Resource)]
pub struct Fighters {
    pub by_id: HashMap<u64, FighterInfo>,
    /// Время последнего боевого события рядом (для музыки).
    pub last_fight: f32,
    /// Последний раз, когда враг атаковал или был ранен поблизости.
    pub hostile_near: f32,
}

impl Default for Fighters {
    fn default() -> Self {
        Self { by_id: HashMap::new(), last_fight: -1000.0, hostile_near: -1000.0 }
    }
}

#[derive(Clone, Copy)]
pub struct FighterInfo {
    pub pos: Vec3,
    pub kind: ActorKind,
    pub local: bool,
}

/// Голос у каждого свой: высота по id.
pub fn voice_pitch(id: u64) -> f32 {
    let h = id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 54;
    0.86 + (h as f32 / 1024.0) * 0.26
}

fn armored(kind: ActorKind) -> bool {
    matches!(kind, ActorKind::Stratiotes | ActorKind::Bucellarius | ActorKind::AlanLancer)
}

fn track_fighters(
    net: Option<Res<NetState>>,
    mut fighters: ResMut<Fighters>,
    remotes: Query<(&RemoteActor, &Transform)>,
    local: Query<&Transform, (With<LocalFighter>, With<LocalPlayer>)>,
) {
    for (r, t) in &remotes {
        fighters.by_id.insert(r.id, FighterInfo { pos: t.translation, kind: r.kind, local: false });
    }
    if let (Some(id), Ok(t)) = (net.and_then(|n| n.my_id), local.single()) {
        fighters.by_id.insert(id, FighterInfo { pos: t.translation, kind: ActorKind::Player, local: true });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Windup,
    Strike,
    Other,
}

fn phase(a: &Action) -> Phase {
    match a {
        Action::Idle | Action::Guard { .. } => Phase::Idle,
        Action::Windup { .. } => Phase::Windup,
        Action::Strike { .. } => Phase::Strike,
        _ => Phase::Other,
    }
}

/// Замах и удар: свист оружия, выдох с усилием, натяжение и спуск тетивы.
#[allow(clippy::too_many_arguments)]
fn actions(
    time: Res<Time>,
    mut out: MessageWriter<Sfx>,
    mut rng: ResMut<SfxRng>,
    mut fighters: ResMut<Fighters>,
    ears: Res<super::Ears>,
    remotes: Query<(Entity, &RemoteActor, &Transform)>,
    local: Query<(Entity, &LocalFighter, &Transform), With<LocalPlayer>>,
    mut last: Local<HashMap<Entity, Phase>>,
) {
    let now = time.elapsed_secs();
    let mut seen = Vec::new();
    let mut each = |e: Entity, id: u64, kind: ActorKind, action: &Action, weapon: WeaponKind, t: &Transform, is_local: bool| {
        seen.push(e);
        let ph = phase(action);
        let prev = last.insert(e, ph).unwrap_or(ph);
        if prev == ph {
            return;
        }
        let hand = t.translation + Vec3::Y * 1.35 + t.rotation * Vec3::new(0.25, 0.0, -0.5);
        // Враг рядом замахивается в нашу сторону — для музыки это уже бой.
        if kind.is_npc() && ears.valid && matches!(ph, Phase::Windup | Phase::Strike) {
            let to_us = ears.pos - t.translation;
            let facing = (t.rotation * Vec3::NEG_Z).dot(to_us.normalize_or_zero());
            if to_us.length() < 20.0 && facing > 0.5 {
                fighters.hostile_near = now;
            }
        }
        match (weapon, ph) {
            (WeaponKind::Bow, Phase::Windup) => {
                out.write(Sfx::at("bow_draw", hand).gain(0.45).range(1.5, 25.0));
            }
            (WeaponKind::Bow, Phase::Strike) => {
                out.write(Sfx::at("bow_release", hand).gain(0.8).range(2.0, 45.0));
            }
            (_, Phase::Strike) => {
                let heavy = matches!(weapon, WeaponKind::Axe | WeaponKind::Spear);
                let name = if heavy { "swing_heavy" } else { "swing" };
                let gain = if is_local { 0.55 } else { 0.7 };
                out.write(Sfx::at(name, hand).gain(gain).range(1.5, 22.0).spread(0.1));
                if rng.chance(if kind.is_npc() { 0.45 } else { 0.3 }) {
                    out.write(Sfx::at("grunt", t.translation + Vec3::Y * 1.6).gain(0.5).pitch(voice_pitch(id)).range(1.5, 25.0));
                }
            }
            _ => {}
        }
    };
    for (e, r, t) in &remotes {
        each(e, r.id, r.kind, &r.fighter.action, r.fighter.weapon, t, false);
    }
    if let Ok((e, f, t)) = local.single() {
        each(e, 0, ActorKind::Player, &f.fighter.action, f.fighter.weapon, t, true);
    }
    last.retain(|e, _| seen.contains(e));
}

/// Попадания, парирования, блоки, смерти.
fn hits(time: Res<Time>, mut events: MessageReader<CombatFeedback>, mut out: MessageWriter<Sfx>, mut fighters: ResMut<Fighters>, mut rng: ResMut<SfxRng>) {
    let now = time.elapsed_secs();
    for ev in events.read() {
        match *ev {
            CombatFeedback::Hit { attacker, target, outcome, at } => {
                let tgt = fighters.by_id.get(&target).copied();
                let att = fighters.by_id.get(&attacker).copied();
                if tgt.is_some_and(|t| t.local) || att.is_some_and(|a| a.local) {
                    fighters.last_fight = now;
                }
                // Схватка в двух шагах тоже тревожит.
                if fighters.by_id.values().any(|f| f.local && f.pos.distance(at) < 12.0) {
                    fighters.hostile_near = now;
                }
                match outcome {
                    HitOutcome::Damage { killed, head, .. } => {
                        let name = if tgt.is_some_and(|t| armored(t.kind)) || head { "hit_armor" } else { "hit_flesh" };
                        out.write(Sfx::at(name, at).gain(0.95).range(2.0, 35.0));
                        if !killed && let Some(t) = tgt {
                            if rng.chance(0.75) {
                                out.write(Sfx::at("pain", t.pos + Vec3::Y * 1.6).gain(0.6).pitch(voice_pitch(target)).range(2.0, 35.0));
                            }
                        }
                    }
                    HitOutcome::Parried => {
                        out.write(Sfx::at("parry", at).gain(1.0).range(3.0, 60.0).spread(0.08));
                    }
                    // Встречный удар: клинки сшиблись в полёте — звон выше и резче.
                    HitOutcome::Chambered => {
                        out.write(Sfx::at("parry", at).gain(1.0).pitch(1.12).range(3.0, 60.0).spread(0.08));
                    }
                    HitOutcome::Blocked { tier } => {
                        let g = match tier {
                            ImpactTier::Weak => 0.6,
                            ImpactTier::Medium => 0.8,
                            ImpactTier::Heavy => 1.0,
                        };
                        out.write(Sfx::at("block_shield", at).gain(g).range(2.5, 45.0));
                    }
                    HitOutcome::Bounced => {
                        out.write(Sfx::at("parry", at).gain(0.45).pitch(0.8).range(2.0, 30.0));
                        out.write(Sfx::at("drop_metal", at).gain(0.3).range(2.0, 25.0));
                    }
                    HitOutcome::GuardBroken => {
                        out.write(Sfx::at("guard_break", at).gain(1.0).range(3.0, 50.0));
                        if let Some(t) = tgt {
                            out.write(Sfx::at("grunt", t.pos + Vec3::Y * 1.6).gain(0.55).pitch(voice_pitch(target) * 0.95).range(2.0, 30.0));
                        }
                    }
                }
            }
            CombatFeedback::Died { id, .. } => {
                if let Some(t) = fighters.by_id.get(&id).copied() {
                    out.write(Sfx::at("death", t.pos + Vec3::Y * 1.5).gain(0.7).pitch(voice_pitch(id)).range(2.0, 40.0));
                    out.write(Sfx::at("body_fall", t.pos + Vec3::Y * 0.3).gain(0.85).range(2.0, 30.0));
                    if t.local {
                        fighters.last_fight = now;
                    }
                }
                fighters.by_id.remove(&id);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ArrowTrack {
    pos: Vec3,
    best: f32,
    whizzed: bool,
}

/// Стрелы: свист у самого уха и стук, когда стрела воткнулась.
fn arrows(ears: Res<super::Ears>, mut out: MessageWriter<Sfx>, q: Query<(Entity, &Transform), With<ArrowVisual>>, mut gone: RemovedComponents<ArrowVisual>, mut tracks: Local<HashMap<Entity, ArrowTrack>>) {
    for (e, t) in &q {
        let p = t.translation;
        let d = if ears.valid { p.distance(ears.pos) } else { f32::MAX };
        let tr = tracks.entry(e).or_insert(ArrowTrack { pos: p, best: d, whizzed: false });
        tr.pos = p;
        if d < tr.best {
            tr.best = d;
        } else if !tr.whizzed && tr.best < 4.0 && d > tr.best + 0.05 {
            // Ближайшая точка пройдена — стрела просвистела мимо.
            tr.whizzed = true;
            out.write(Sfx::at("arrow_whiz", p).gain(0.8).range(1.0, 8.0));
        }
    }
    for e in gone.read() {
        if let Some(tr) = tracks.remove(&e) {
            out.write(Sfx::at("arrow_hit", tr.pos).gain(0.6).range(1.5, 30.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voices_differ_but_stay_human() {
        let a = voice_pitch(1);
        let b = voice_pitch(2);
        assert!(a != b);
        for id in 0..500 {
            let p = voice_pitch(id);
            assert!((0.85..=1.13).contains(&p), "{p}");
        }
    }
}
