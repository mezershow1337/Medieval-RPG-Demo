//! Ставни лавок: днём распахнуты, к вечеру лавочник их запирает, утром открывает.
//!
//! Ставни — обычные двустворчатые двери проёма лавки (`doors.rs`): их коллизия, створки
//! и рассылка состояния уже есть. Здесь сервер по часам мира (`world_clock.rs`) закрывает
//! их на ночь и запирает снаружи: изнутри (если кто-то задержался в лавке) открыть можно.
//! Пока в лавке или в проёме кто-то есть, ставни не захлопываются — лавочник ждёт.

use crate::collision::{Body, LevelColliders};
use crate::doors::{CH_DOOR, DoorServerMessage};
use crate::level::ShopSpec;
use crate::protocol::encode;
use bevy::prelude::*;
use bevy_renet::RenetServer;
use std::collections::HashMap;

/// Как часто сверять ставни с часами (настоящие секунды).
const CHECK_SECS: f32 = 1.0;

pub struct ShopHoursServerPlugin;

impl Plugin for ShopHoursServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ShopShutters::from_level(crate::level::shops()))
            .add_systems(Update, tick_shutters);
    }
}

/// Ставни одной лавки.
#[derive(Clone, Copy, Debug)]
pub struct Shutter {
    pub door: u32,
    pub shop: ShopSpec,
    pub hours: [f32; 2],
}

/// Ставни всех лавок и какие из них заперты на ночь.
#[derive(Resource, Default)]
pub struct ShopShutters {
    pub list: Vec<Shutter>,
    /// Дверь -> номер в `list`.
    by_door: HashMap<u32, usize>,
    /// Заперта ли лавка (ночь): по номерам в `list`. `None` — ещё не сверялись.
    locked: Vec<Option<bool>>,
}

impl ShopShutters {
    pub fn from_level(shops: &[ShopSpec]) -> Self {
        let list: Vec<Shutter> = shops
            .iter()
            .enumerate()
            .filter_map(|(k, s)| Some(Shutter { door: s.door?, shop: *s, hours: crate::shops::hours_of(k, s.kind) }))
            .collect();
        let by_door = list.iter().enumerate().map(|(i, s)| (s.door, i)).collect();
        let locked = vec![None; list.len()];
        Self { list, by_door, locked }
    }

    /// Можно ли тронуть ставни двери `door`, стоя в `feet`: ночью — только изнутри лавки.
    /// `Err` — почему нельзя.
    pub fn may_toggle(&self, door: u32, feet: Vec3) -> Result<(), String> {
        let Some(&i) = self.by_door.get(&door) else { return Ok(()) };
        let s = &self.list[i];
        if self.locked[i] == Some(true) && !s.shop.contains(feet, 0.25) {
            return Err(format!("Ставни лавки заперты на ночь. Лавка открыта с {}.", crate::shops::hours_text(s.hours)));
        }
        Ok(())
    }
}

/// Ставни лавки по номеру двери (для подсказки у клиента: та же раскладка, что у сервера).
pub fn shutter(door: u32) -> Option<Shutter> {
    static ALL: std::sync::OnceLock<ShopShutters> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| ShopShutters::from_level(crate::level::shops()));
    all.by_door.get(&door).map(|&i| all.list[i])
}

/// Сверяет ставни с часами: утром открыть, вечером (когда в лавке пусто) закрыть.
fn tick_shutters(
    time: Res<Time>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
    mut shutters: ResMut<ShopShutters>,
    level: Res<LevelColliders>,
    game: Res<crate::server::GameWorld>,
    mut server: ResMut<RenetServer>,
    mut next: Local<f32>,
) {
    let Some(clock) = clock else { return };
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + CHECK_SECS;
    let hour = clock.hour();
    let shutters = &mut *shutters;
    for (i, s) in shutters.list.iter().enumerate() {
        let id = s.door as usize;
        if id >= level.door_count() {
            continue;
        }
        let night = !crate::shops::is_open(s.hours, hour);
        let was = shutters.locked[i];
        shutters.locked[i] = Some(night);
        let open = level.door_open(id);
        if !night {
            // Утро: лавочник распахивает ставни (днём их не трогает — игрок мог прикрыть).
            if was != Some(false) && !open {
                set_door(&level, &mut server, s.door, true);
            }
            continue;
        }
        if !open {
            continue;
        }
        // Вечер: запирает, когда в лавке и в проёме никого (тех, кто внутри, не запирает).
        let doorway = level.door_box(id).unwrap();
        let busy = game.actors.values().any(|a| a.alive && (s.shop.contains(a.pose.feet, 0.3) || Body::STANDING.aabb_at(a.pose.feet).overlaps(&doorway)));
        if !busy {
            set_door(&level, &mut server, s.door, false);
        }
    }
}

fn set_door(level: &LevelColliders, server: &mut RenetServer, door: u32, open: bool) {
    level.set_door_open(door as usize, open);
    server.broadcast_message(CH_DOOR, encode(&DoorServerMessage::Set { id: door, open }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_only_from_outside_at_night() {
        let shops = crate::level::shops();
        let mut sh = ShopShutters::from_level(shops);
        assert_eq!(sh.list.len(), shops.len(), "у каждой лавки свои ставни");
        let s = sh.list[0];
        let inside = (s.shop.room_min + s.shop.room_max).with_y(s.shop.room_min.y * 2.0) / 2.0;
        let street = s.shop.seller - s.shop.facing * 4.5;
        assert!(sh.may_toggle(s.door, street).is_ok(), "днём ставни открывают все");
        sh.locked[0] = Some(true);
        assert!(sh.may_toggle(s.door, street).is_err(), "ночью снаружи не открыть");
        assert!(sh.may_toggle(s.door, inside).is_ok(), "изнутри выйти можно");
        // Двери у лавок разные.
        let doors: std::collections::HashSet<u32> = sh.list.iter().map(|s| s.door).collect();
        assert_eq!(doors.len(), sh.list.len());
    }
}
