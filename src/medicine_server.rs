//! Сервер: тела игроков. Попадания превращаются в раны, раны кровят и воспаляются,
//! яды действуют, и всё это меняет здоровье и бойца. Лечение предметами запускает knowledge.rs
//! (как и другие занятия), а сами расчёты — в medicine.rs.
//!
//! # API для алхимии и других частей
//!
//! ```ignore
//! commands.trigger(Burned { player, severity: 0.4 });           // огонь, кипяток, известь
//! commands.trigger(Poisoned { player, toxin: Toxin::Mercury, doses: 0.5 }); // пары ртути
//! ```

use crate::items::Rng;
use crate::lore_net::{CH_LORE, LoreServerMessage};
use crate::medicine::{Body, BodyMods, Toxin};
use crate::protocol::encode;
use crate::protocol::ActorKind;
use crate::server::GameWorld;
use bevy::prelude::*;
use bevy_renet::RenetServer;
use std::collections::HashMap;

/// Как часто слать состояние тела его хозяину.
const SEND_EVERY: f32 = 0.5;

pub struct MedicineServerPlugin;

impl Plugin for MedicineServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Bodies::default())
            .add_observer(on_burned)
            .add_observer(on_poisoned)
            .add_systems(Update, (take_hits, tick_bodies).chain());
    }
}

/// Ожог (алхимия, зажигательный горшок, известь).
#[derive(Event, Clone, Debug)]
pub struct Burned {
    pub player: u64,
    pub severity: f32,
}

/// Яд или пары (возгонка ртути, серный дым).
#[derive(Event, Clone, Debug)]
pub struct Poisoned {
    pub player: u64,
    pub toxin: Toxin,
    pub doses: f32,
}

#[derive(Resource)]
pub struct Bodies {
    pub bodies: HashMap<u64, Body>,
    /// Сколько секунд до следующей отправки.
    send_timer: HashMap<u64, f32>,
    pub rng: Rng,
}

impl Default for Bodies {
    fn default() -> Self {
        Self { bodies: HashMap::new(), send_timer: HashMap::new(), rng: Rng::from_time() }
    }
}

impl Bodies {
    /// Тело игрока (новое, если он был здоров).
    pub fn body(&mut self, player: u64) -> &mut Body {
        self.bodies.entry(player).or_default()
    }

    /// Отправить состояние в ближайшем кадре.
    pub fn touch(&mut self, player: u64) {
        self.send_timer.insert(player, 0.0);
    }
}

fn on_burned(event: On<Burned>, mut bodies: ResMut<Bodies>) {
    let ev = event.event();
    bodies.body(ev.player).burn(ev.severity);
    bodies.touch(ev.player);
}

fn on_poisoned(event: On<Poisoned>, mut bodies: ResMut<Bodies>) {
    let ev = event.event();
    bodies.body(ev.player).poison(ev.toxin, ev.doses);
    bodies.touch(ev.player);
}

/// Попадания этого кадра — в раны.
fn take_hits(mut game: ResMut<GameWorld>, mut bodies: ResMut<Bodies>, mut seeded: Local<std::collections::HashSet<u64>>) {
    // Для проверки картинкой: `MRPG_DEBUG_WOUNDS=1` — каждый вошедший сразу ранен и отравлен.
    if std::env::var("MRPG_DEBUG_WOUNDS").is_ok() {
        let players: Vec<(u64, f32)> = game.actors.iter().filter(|(id, a)| a.kind == ActorKind::Player && a.alive && !seeded.contains(*id)).map(|(id, a)| (*id, a.fighter.max_hp)).collect();
        for (id, max_hp) in players {
            seeded.insert(id);
            let b = bodies.body(id);
            use crate::medicine::{BodyPart, WoundKind};
            b.hit_on(WoundKind::Stab, 30.0, max_hp, BodyPart::Belly, 0.9, 0.0, 1.0);
            b.hit_on(WoundKind::Arrow, 20.0, max_hp, BodyPart::LeftArm, 0.1, 0.0, 1.0);
            b.hit_on(WoundKind::Cut, 15.0, max_hp, BodyPart::Head, 0.9, 0.0, 1.0);
            b.fracture(BodyPart::RightLeg, 0.7);
            b.burn(0.3);
            b.poison(Toxin::Henbane, 0.8);
            bodies.touch(id);
        }
    }
    for hit in std::mem::take(&mut game.hits_taken) {
        let Some(actor) = game.actors.get(&hit.target).filter(|a| a.alive) else { continue };
        let max_hp = actor.fighter.max_hp;
        let roll = bodies.rng.f32();
        let bone_roll = bodies.rng.f32();
        if bodies.body(hit.target).hit_on(hit.wound_kind(), hit.amount, max_hp, hit.part, roll, hit.bone_chance(), bone_roll).is_some() {
            bodies.touch(hit.target);
        } else if bodies.bodies.get(&hit.target).is_some_and(Body::is_healthy) {
            bodies.bodies.remove(&hit.target);
        }
    }
}

fn send_body(server: &mut RenetServer, player: u64, body: &Body) {
    server.send_message(player, CH_LORE, encode(&LoreServerMessage::Body(body.clone())));
}

fn tick_bodies(time: Res<Time>, mut server: ResMut<RenetServer>, mut game: ResMut<GameWorld>, mut bodies: ResMut<Bodies>) {
    let dt = time.delta_secs().min(0.1);
    let bodies = &mut *bodies;
    // Ушедшие и погибшие: тело заново здорово (после смерти игрок появляется целым).
    let gone: Vec<u64> = bodies.bodies.keys().copied().filter(|id| game.actors.get(id).is_none_or(|a| !a.alive)).collect();
    for id in gone {
        bodies.bodies.remove(&id);
        bodies.send_timer.remove(&id);
        if game.actors.contains_key(&id) {
            send_body(&mut server, id, &Body::default());
        }
    }
    for (id, actor) in game.actors.iter_mut() {
        if actor.kind != ActorKind::Player {
            continue;
        }
        let Some(body) = bodies.bodies.get_mut(id) else {
            actor.body_mods = BodyMods::default();
            continue;
        };
        let loss = body.tick(dt);
        if actor.alive {
            actor.fighter.hp = (actor.fighter.hp - loss).max(0.0);
        }
        actor.body_mods = body.mods();
        let timer = bodies.send_timer.entry(*id).or_insert(0.0);
        *timer -= dt;
        if *timer <= 0.0 {
            *timer = SEND_EVERY;
            send_body(&mut server, *id, body);
        }
    }
    // Выздоровевшие: последнее (пустое) состояние ушло, тело можно забыть.
    let healed: Vec<u64> = bodies.bodies.iter().filter(|(id, b)| b.is_healthy() && bodies.send_timer.get(*id).is_some_and(|t| *t >= SEND_EVERY - dt)).map(|(id, _)| *id).collect();
    for id in healed {
        bodies.bodies.remove(&id);
        bodies.send_timer.remove(&id);
    }
}
