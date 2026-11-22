//! Сервер: применение расходников (бинты, зелья). Лечение меняет здоровье бойца
//! из `GameWorld`, клиент видит его в обычном снимке мира.

use crate::combat::Action;
use crate::items::{ItemBaseId, ItemUid};
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::protocol::encode;
use crate::server::GameWorld;
use bevy::prelude::*;
use bevy_renet::RenetServer;
use std::collections::HashMap;

/// Идёт применение предмета.
struct Using {
    uid: ItemUid,
    base: ItemBaseId,
    elapsed: f32,
    /// Здоровье после прошлого кадра: если стало меньше — по нам попали.
    last_hp: f32,
}

/// Лечение после применения (зелье).
struct HealOverTime {
    player: u64,
    per_sec: f32,
    left: f32,
}

#[derive(Resource, Default)]
pub struct Healing {
    using: HashMap<u64, Using>,
    hots: Vec<HealOverTime>,
    /// Просьбы клиентов: (игрок, предмет или None — прервать).
    pub requests: Vec<(u64, Option<ItemUid>)>,
}

pub struct ConsumablesPlugin;

impl Plugin for ConsumablesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Healing::default()).add_systems(Update, tick_consumables);
    }
}

fn send(server: &mut RenetServer, player: u64, msg: &LootServerMessage) {
    server.send_message(player, CH_LOOT, encode(msg));
}

fn tick_consumables(
    time: Res<Time>,
    mut server: ResMut<RenetServer>,
    mut game: ResMut<GameWorld>,
    mut world: ResMut<LootWorld>,
    mut healing: ResMut<Healing>,
) {
    let dt = time.delta_secs();
    let healing = &mut *healing;

    for (pid, request) in std::mem::take(&mut healing.requests) {
        let Some(uid) = request else {
            if healing.using.remove(&pid).is_some() {
                send(&mut server, pid, &LootServerMessage::UseFinished { ok: false });
            }
            continue;
        };
        let notice = |server: &mut RenetServer, text: &str| send(server, pid, &LootServerMessage::Notice(text.into()));
        let Some(actor) = game.actors.get(&pid).filter(|a| a.alive && !a.fighter.is_dead()) else { continue };
        let Some(base) = world.inventories.get(&pid).and_then(|inv| inv.find(uid)).map(|(_, item)| item.base) else { continue };
        let Some(c) = base.consumable() else {
            notice(&mut server, "Это нельзя использовать");
            continue;
        };
        if healing.using.contains_key(&pid) {
            continue;
        }
        if actor.fighter.hp >= actor.fighter.max_hp {
            notice(&mut server, "Здоровье и так полное");
            continue;
        }
        healing.using.insert(pid, Using { uid, base, elapsed: 0.0, last_hp: actor.fighter.hp });
        send(&mut server, pid, &LootServerMessage::UseStarted { verb: c.verb.into(), duration: c.use_time });
    }

    let mut finished = Vec::new();
    for (pid, u) in healing.using.iter_mut() {
        let c = u.base.consumable().expect("проверено при старте");
        let Some(actor) = game.actors.get_mut(pid) else {
            finished.push((*pid, false));
            continue;
        };
        let f = &mut actor.fighter;
        // Удар по нам, своя атака или парирование прерывают применение.
        let interrupted = !actor.alive || f.is_dead() || f.hp < u.last_hp - crate::knowledge::HIT_THRESHOLD || !matches!(f.action, Action::Idle);
        if interrupted {
            finished.push((*pid, false));
            continue;
        }
        u.elapsed += dt;
        if c.heal_over == 0.0 {
            f.hp = (f.hp + c.heal * dt / c.use_time).min(f.max_hp);
        }
        u.last_hp = f.hp;
        if u.elapsed >= c.use_time {
            finished.push((*pid, true));
        }
    }
    for (pid, ok) in finished {
        let Some(u) = healing.using.remove(&pid) else { continue };
        if ok {
            // Списываем одну штуку из стопки.
            let consumed = world.inventories.get_mut(&pid).is_some_and(|inv| {
                let Some((mut item, loc)) = inv.take(u.uid) else { return false };
                if item.count > 1 {
                    item.count -= 1;
                    let _ = inv.place(item, loc.into());
                }
                true
            });
            if !consumed {
                send(&mut server, pid, &LootServerMessage::UseFinished { ok: false });
                continue;
            }
            let c = u.base.consumable().unwrap();
            if c.heal_over > 0.0 {
                healing.hots.push(HealOverTime { player: pid, per_sec: c.heal / c.heal_over, left: c.heal_over });
            }
            if let Some(inv) = world.inventories.get(&pid) {
                send(&mut server, pid, &LootServerMessage::Inventory(inv.clone()));
            }
        }
        send(&mut server, pid, &LootServerMessage::UseFinished { ok });
    }

    healing.hots.retain_mut(|h| {
        let Some(actor) = game.actors.get_mut(&h.player).filter(|a| a.alive && !a.fighter.is_dead()) else { return false };
        let step = dt.min(h.left);
        actor.fighter.hp = (actor.fighter.hp + h.per_sec * step).min(actor.fighter.max_hp);
        h.left -= step;
        h.left > 0.0
    });
}
