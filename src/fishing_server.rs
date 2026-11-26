//! Сервер рыбалки: принимает заброс, решает, кто клюнет (вид, вес, когда), и выдаёт рыбу,
//! только если игрок подсёк вовремя и вываживал не быстрее, чем это вообще возможно.
//! Наживка тратится при подсечке и при пустой поклёвке, крючок — когда рвётся леса.

use crate::fishing::{self, Bait, FightSpec, Fish, HOOK_WINDOW, HOOKS, Water};
use crate::fishing_net::*;
use crate::inventory::{Inventory, Target};
use crate::items::{EquipSlot, ItemBaseId, Quality, Rng, plain_item};
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::protocol::{decode, encode};
use crate::server::GameWorld;
use bevy::prelude::*;
use bevy_renet::RenetServer;
use std::collections::HashMap;

pub struct FishingServerPlugin;

impl Plugin for FishingServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FishWorld { sessions: HashMap::new(), rng: Rng::from_time() }).add_systems(Update, receive_fishing);
    }
}

/// Запас по времени на задержку сети (с).
const LAG: f32 = 0.45;
/// Дальше этого от места заброса уйти нельзя — леса оборвётся (м).
const WANDER: f32 = 4.0;

struct Session {
    at: Vec3,
    from: Vec3,
    bait: Bait,
    fish: &'static Fish,
    kg: f32,
    spec: FightSpec,
    cast_t: f64,
    bite_in: f32,
    hooked_at: Option<f64>,
}

#[derive(Resource)]
pub struct FishWorld {
    sessions: HashMap<u64, Session>,
    rng: Rng,
}

fn send(server: &mut RenetServer, player: u64, msg: &FishServerMessage) {
    server.send_message(player, CH_FISH, encode(msg));
}

fn sync_inventory(server: &mut RenetServer, inv: &Inventory, player: u64) {
    server.send_message(player, CH_LOOT, encode(&LootServerMessage::Inventory(inv.clone())));
}

fn count(inv: &Inventory, name: &str) -> u32 {
    inv.all_items().filter(|i| i.info().name == name).map(|i| i.count as u32).sum()
}

/// Забрать одну штуку из сумки.
fn take_one(inv: &mut Inventory, name: &str) -> bool {
    let Some(uid) = inv.grid.items.iter().find(|p| p.item.info().name == name).map(|p| p.item.uid) else { return false };
    if let Some(p) = inv.grid.items.iter_mut().find(|p| p.item.uid == uid) {
        p.item.count -= 1;
    }
    inv.grid.items.retain(|p| p.item.count > 0);
    true
}

/// Удочка в правой руке (если это удочка).
pub fn rod_in(inv: &Inventory) -> Option<&'static fishing::Rod> {
    inv.equipped(EquipSlot::MainHand).and_then(|i| fishing::rod(i.info().name))
}

/// Глубина воды в точке (м); меньше нуля — суша.
pub fn water_depth(x: f32, z: f32) -> f32 {
    crate::level::WATER_Y - crate::level::height_at(x, z)
}

/// Можно ли забросить из `feet` в `at` этой удочкой: текст отказа или глубина.
pub fn check_cast(feet: Vec3, at: Vec3, rod: &fishing::Rod) -> Result<f32, &'static str> {
    let d = Vec2::new(at.x - feet.x, at.z - feet.z).length();
    if d > rod.cast + 2.0 {
        return Err("Так далеко не забросить");
    }
    let depth = water_depth(at.x, at.z);
    if depth < 0.25 {
        return Err("Поплавок упал не в воду");
    }
    Ok(depth)
}

fn receive_fishing(
    time: Res<Time>,
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    mut loot: ResMut<LootWorld>,
    mut world: ResMut<FishWorld>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
) {
    let now = time.elapsed_secs_f64();
    let hour = clock.map(|c| c.hour()).unwrap_or(12.0);
    let world = &mut *world;
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_FISH) {
            let Some(msg) = decode::<FishClientMessage>(&bytes) else { continue };
            let Some(actor) = game.actors.get(&pid).filter(|a| a.alive && !a.fighter.is_dead()) else {
                world.sessions.remove(&pid);
                continue;
            };
            let feet = actor.pose.feet;
            let loot = &mut *loot;
            let Some(inv) = loot.inventories.get_mut(&pid) else { continue };
            match msg {
                FishClientMessage::Cast { at, bait } => {
                    let at = Vec3::from_array(at);
                    let Some(rod) = rod_in(inv) else {
                        send(&mut server, pid, &FishServerMessage::Refused("Возьмите удочку в руку".into()));
                        continue;
                    };
                    if count(inv, HOOKS) == 0 {
                        send(&mut server, pid, &FishServerMessage::Refused("Нет крючка: купите бронзовые крючки у рыбака".into()));
                        continue;
                    }
                    if count(inv, bait.item()) == 0 {
                        send(&mut server, pid, &FishServerMessage::Refused(format!("Нет наживки: {}", bait.item().to_lowercase())));
                        continue;
                    }
                    let depth = match check_cast(feet, at, rod) {
                        Ok(d) => d,
                        Err(e) => {
                            send(&mut server, pid, &FishServerMessage::Refused(e.into()));
                            continue;
                        }
                    };
                    let rng = &mut world.rng;
                    let (fish, kg) = fishing::pick_fish(rng, Water::of_depth(depth), bait, hour);
                    let bite_in = fishing::bite_wait(rng, bait, hour);
                    let mut nibbles = Vec::new();
                    let mut t = rng.range_f32(1.5, 3.0);
                    while t < bite_in - 1.2 {
                        if rng.f32() < 0.6 {
                            nibbles.push(t);
                        }
                        t += rng.range_f32(1.2, 3.0);
                    }
                    let spec = FightSpec::of(fish, kg, rng.next_u64() as u32);
                    world.sessions.insert(pid, Session { at, from: feet, bait, fish, kg, spec, cast_t: now, bite_in, hooked_at: None });
                    send(&mut server, pid, &FishServerMessage::Cast { bite_in, nibbles, spec, line: rod.strength, reel: rod.reel });
                }
                FishClientMessage::Hook => {
                    let Some(s) = world.sessions.get_mut(&pid) else { continue };
                    if s.hooked_at.is_some() {
                        continue;
                    }
                    let t = (now - s.cast_t) as f32;
                    if t < s.bite_in - LAG {
                        world.sessions.remove(&pid);
                        send(&mut server, pid, &FishServerMessage::Missed("Рано подсекли — рыба испугалась".into()));
                    } else if t > s.bite_in + HOOK_WINDOW + LAG {
                        let bait = s.bait;
                        world.sessions.remove(&pid);
                        take_one(inv, bait.item());
                        sync_inventory(&mut server, inv, pid);
                        send(&mut server, pid, &FishServerMessage::Missed("Поздно: рыба объела наживку".into()));
                    } else {
                        s.hooked_at = Some(now);
                        take_one(inv, s.bait.item());
                        sync_inventory(&mut server, inv, pid);
                        send(&mut server, pid, &FishServerMessage::Hooked);
                    }
                }
                FishClientMessage::Landed => {
                    let Some(s) = world.sessions.remove(&pid) else { continue };
                    let Some(hooked) = s.hooked_at else { continue };
                    let dist = Vec2::new(s.at.x - s.from.x, s.at.z - s.from.z).length();
                    let fought = (now - hooked) as f32;
                    if fought + LAG < s.spec.min_secs(dist) * 0.8 || feet.distance(s.from) > WANDER + 2.0 {
                        send(&mut server, pid, &FishServerMessage::Notice("Рыба сорвалась у самого берега".into()));
                        continue;
                    }
                    let Some(base) = ItemBaseId::find(s.fish.name) else { continue };
                    let item = plain_item(base, Quality::Common, 1, &mut loot.uids);
                    let kg = s.kg;
                    let inv = loot.inventories.get_mut(&pid).unwrap();
                    match inv.place(item, Target::Auto) {
                        Ok(_) => {
                            sync_inventory(&mut server, inv, pid);
                            send(&mut server, pid, &FishServerMessage::Caught { name: s.fish.name.into(), kg });
                        }
                        Err(_) => send(&mut server, pid, &FishServerMessage::Notice(format!("{} не помещается в сумку — пришлось отпустить", s.fish.name))),
                    }
                }
                FishClientMessage::Lost { snapped } => {
                    let Some(s) = world.sessions.remove(&pid) else { continue };
                    if snapped && s.hooked_at.is_some() {
                        take_one(inv, HOOKS);
                        sync_inventory(&mut server, inv, pid);
                    }
                }
                FishClientMessage::Reel => {
                    world.sessions.remove(&pid);
                }
            }
        }
    }
    // Ушёл с места или убрал удочку — леса брошена.
    world.sessions.retain(|pid, s| {
        game.actors.get(pid).is_some_and(|a| a.alive && a.pose.feet.distance(s.from) < WANDER + 6.0) && loot.inventories.get(pid).is_some_and(|i| rod_in(i).is_some())
    });
}
