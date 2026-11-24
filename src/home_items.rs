//! Вещи хозяев в жилых домах Херсонеса: светильники, миски, хлеб, гребни, монеты на
//! столах, полках и лавках (раскладку задаёт уровень — `Layout::house_items`).
//!
//! Это обычные предметы на земле (`GroundItem`) со своей физикой, но с хозяином
//! (`GroundItem::owner`). Взять их — кража:
//!
//! * сунуть в сумку («E» / «G») или вынуть из хозяйского сундука — сразу;
//! * взять в руки (ЛКМ) и переставить можно, но вынести из дома (или унести дальше
//!   [`CARRY_STEAL`] от места за стены дома) — тоже кража.
//!
//! О каждой краже сервер сообщает событием [`OwnedItemTaken`] (`commands.trigger`):
//! кто, из какого дома, что и где. Его разбирает система свидетелей и стражи.
//!
//! Хозяева прибирают сдвинутое через [`TIDY_AFTER`] секунд, если рядом нет игроков, а
//! пропавшее со временем покупают заново ([`RESTOCK_AFTER`]).

use crate::items::{ItemBaseId, Quality, roll_item};
use crate::level::{HouseSpec, HouseItemSpec, Layout};
use crate::loot_net::{CH_LOOT, LootId, LootServerMessage};
use crate::loot_server::{GroundItem, LootWorld, ground_message};
use crate::props::PropBody;
use crate::protocol::{ActorKind, encode};
use crate::server::GameWorld;
use bevy::prelude::*;
use bevy_renet::RenetServer;

pub use crate::items::Item;

/// Унёс дальше этого от места (м) и за стены дома — кража.
pub const CARRY_STEAL: f32 = 1.6;
/// Через сколько секунд хозяева ставят сдвинутое на место.
pub const TIDY_AFTER: f32 = 90.0;
/// Через сколько секунд хозяева заводят взамен пропавшего новое.
pub const RESTOCK_AFTER: f32 = 20.0 * 60.0;
/// Не прибирать и не докупать, пока игрок ближе этого (м): на глазах вещи не появляются.
const NOBODY_NEAR: f32 = 8.0;
const TICK: f32 = 0.25;

/// Чья вещь и где её место.
#[derive(Clone, Debug)]
#[allow(dead_code)] // поля читает система свидетелей и стражи
pub struct HomeTag {
    /// Номер дома (`Layout::houses`).
    pub house: u32,
    /// Номер места в раскладке дома (`HomeItems::slots`).
    pub slot: u32,
    /// Где стоит: центр тела и поворот.
    pub home: (Vec3, Quat),
    /// Сколько секунд лежит не на месте.
    pub away: f32,
}

/// Как взяли.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeHow {
    /// Сунул в сумку прямо с места.
    Pocketed,
    /// Вынес в руках из дома.
    CarriedOut,
    /// Вынул из хозяйского сундука или шкафа.
    FromContainer,
}

/// Игрок взял чужую вещь в жилом доме (сервер, `commands.trigger`).
#[derive(Event, Clone, Debug)]
#[allow(dead_code)] // разбирает система свидетелей и стражи
pub struct OwnedItemTaken {
    pub player: u64,
    /// Номер дома (`Layout::houses`, `level::places::house_at`).
    pub house: u32,
    /// Что взял (со счётом штук).
    pub item: Item,
    /// Где это было.
    pub at: Vec3,
    pub how: TakeHow,
}

struct Slot {
    spec: HouseItemSpec,
    /// Предмет в мире; `None` — пропал (унесли).
    id: Option<LootId>,
    /// Сколько секунд места нет.
    gone: f32,
}

/// Раскладка вещей по домам и что из неё сейчас в мире.
#[derive(Resource, Default)]
pub struct HomeItems {
    slots: Vec<Slot>,
    houses: Vec<(Vec3, Vec3)>,
    clock: f32,
}

impl HomeItems {
    /// Разложить вещи по всем домам (сервер, при запуске).
    pub fn setup(&mut self, server: &mut RenetServer, world: &mut LootWorld, layout: &Layout) {
        self.houses = layout.houses.iter().map(|h: &HouseSpec| (h.min, h.max)).collect();
        self.slots = layout.house_items.iter().map(|s| Slot { spec: s.clone(), id: None, gone: 0.0 }).collect();
        for i in 0..self.slots.len() {
            self.spawn(server, world, i);
        }
        info!("Вещи хозяев: {} в {} домах", self.slots.len(), self.houses.len());
    }

    /// Сколько мест в раскладке.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    fn spawn(&mut self, server: &mut RenetServer, world: &mut LootWorld, i: usize) {
        let spec = &self.slots[i].spec;
        let Some(base) = ItemBaseId::find(spec.item) else { return };
        let mut uids = world.uids.clone();
        let item = roll_item(base, Quality::Common, spec.count.max(1), &mut world.rng, &mut uids);
        world.uids = uids;
        let rot = Quat::from_rotation_y(spec.yaw);
        let half = crate::icons::model_half_extents(base);
        let pos = spec.pos + Vec3::Y * (half[1] + 0.002);
        let mut body = PropBody::for_item(item.base, item.count, pos, rot);
        body.place(pos, rot);
        let owner = Some(HomeTag { house: spec.house, slot: i as u32, home: (pos, rot), away: 0.0 });
        let id = world.insert_ground(server, GroundItem { body, item, shelf: None, holder: None, owner });
        self.slots[i].id = Some(id);
        self.slots[i].gone = 0.0;
    }

    fn inside(&self, house: u32, p: Vec3) -> bool {
        self.houses.get(house as usize).is_some_and(|(min, max)| {
            p.x >= min.x - 0.3 && p.x <= max.x + 0.3 && p.z >= min.z - 0.3 && p.z <= max.z + 0.3 && p.y >= min.y - 1.0 && p.y <= max.y + 1.0
        })
    }
}

pub struct HomeItemsPlugin;

impl Plugin for HomeItemsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HomeItems::default()).add_systems(Update, (tick_home_items, flush_taken).chain());
    }
}

/// Разослать кражи за кадр.
fn flush_taken(mut commands: Commands, mut world: ResMut<LootWorld>) {
    for ev in std::mem::take(&mut world.taken) {
        commands.trigger(ev);
    }
}

/// Вынесли из дома — кража; сдвинутое прибрать; пропавшее завести заново.
fn tick_home_items(time: Res<Time>, mut server: ResMut<RenetServer>, game: Res<GameWorld>, mut world: ResMut<LootWorld>, mut homes: ResMut<HomeItems>) {
    let dt = time.delta_secs();
    homes.clock += dt;
    if homes.clock < TICK {
        return;
    }
    let dt = std::mem::take(&mut homes.clock);
    let players: Vec<Vec3> = game.actors.values().filter(|a| a.kind == ActorKind::Player && a.alive).map(|a| a.pose.feet).collect();
    let nobody_near = |p: Vec3| players.iter().all(|q| q.distance_squared(p) > NOBODY_NEAR * NOBODY_NEAR);
    let world = &mut *world;
    let homes = &mut *homes;

    let mut carried = Vec::new();
    for i in 0..homes.slots.len() {
        let Some(id) = homes.slots[i].id else {
            let s = &mut homes.slots[i];
            s.gone += dt;
            if s.gone > RESTOCK_AFTER && nobody_near(s.spec.pos) {
                homes.spawn(&mut server, world, i);
            }
            continue;
        };
        let Some(g) = world.ground.get_mut(&id) else {
            // Подобрали в сумку (или вещь стала ничьей).
            homes.slots[i].id = None;
            continue;
        };
        let Some(tag) = g.owner.as_mut() else {
            homes.slots[i].id = None;
            continue;
        };
        let off = g.body.pos.distance(tag.home.0);
        if let Some(p) = g.holder {
            tag.away = 0.0;
            if off > CARRY_STEAL && !homes.inside(tag.house, g.body.pos) {
                carried.push((p, id, tag.house));
            }
        } else if off > 0.12 || g.body.rot.angle_between(tag.home.1) > 0.3 {
            tag.away += dt;
            if tag.away > TIDY_AFTER && g.body.asleep && nobody_near(tag.home.0) && nobody_near(g.body.pos) {
                tag.away = 0.0;
                let home = tag.home;
                g.body.place(home.0, home.1);
                server.broadcast_message(CH_LOOT, encode(&LootServerMessage::PropRest { id, pos: home.0.into(), rot: home.1.to_array() }));
            }
        } else {
            tag.away = 0.0;
        }
    }
    for (player, id, house) in carried {
        let Some(g) = world.ground.get_mut(&id) else { continue };
        g.owner = None;
        let item = g.item.clone();
        let at = g.body.pos;
        let msg = ground_message(id, g);
        server.broadcast_message(CH_LOOT, encode(&msg));
        server.broadcast_message(CH_LOOT, encode(&LootServerMessage::PropHeld { id, holder: Some(player) }));
        world.taken.push(OwnedItemTaken { player, house, item, at, how: TakeHow::CarriedOut });
        if let Some(s) = homes.slots.iter_mut().find(|s| s.id == Some(id)) {
            s.id = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn houses_are_full_of_things_to_take() {
        let layout = crate::level::layout();
        assert!(layout.houses.len() > 40, "жилых домов: {}", layout.houses.len());
        for s in &layout.house_items {
            assert!(ItemBaseId::find(s.item).is_some(), "нет предмета «{}»", s.item);
            assert!((s.house as usize) < layout.houses.len());
            assert!(layout.houses[s.house as usize].contains(s.pos), "«{}» вне дома {}", s.item, s.house);
        }
        let mut per_house = vec![0usize; layout.houses.len()];
        for s in &layout.house_items {
            per_house[s.house as usize] += 1;
        }
        per_house.sort();
        let median = per_house[per_house.len() / 2];
        eprintln!("домов {}, вещей {}, в доме от {} до {}, обычно {median}", per_house.len(), layout.house_items.len(), per_house[0], per_house[per_house.len() - 1]);
        assert!(median >= 12, "в доме обычно {median} вещей");
        assert!(layout.house_items.len() < 6000, "вещей {} — тяжело для сервера", layout.house_items.len());
        // В сундуках жилых домов — тоже хозяйское.
        assert!(layout.containers.iter().filter(|c| c.house.is_some()).count() > 40);
    }
}
