//! Сетевые сообщения добычи и инвентаря. Идут по своему каналу [`CH_LOOT`],
//! чтобы не мешать основным сообщениям из `protocol.rs`.

use crate::collision::{Body, LevelColliders};
use crate::inventory::{Inventory, Target};
use crate::items::{ItemBaseId, ItemUid, Quality};
use crate::protocol::NetPos;
use bevy::math::Vec3;
use serde::{Deserialize, Serialize};

/// Номер канала renet для добычи (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_LOOT: u8 = 3;

/// Дальше этого расстояния (метры) нельзя обыскивать и подбирать.
pub const INTERACT_RANGE: f32 = 3.0;

/// Номер тела или предмета на земле.
pub type LootId = u64;

/// Сундуки, шкафы и корзины уровня: номер = база + индекс в `level::Layout::containers`.
pub const CONTAINER_ID_BASE: LootId = 1 << 50;

/// Чей инвентарь.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owner {
    Me,
    /// Открытое тело (или другой контейнер).
    Container(LootId),
}

/// Клиент -> сервер.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LootClientMessage {
    /// «E» на теле — обыскать, на предмете — подобрать.
    Interact { id: LootId },
    /// Переложить предмет.
    Move { uid: ItemUid, from: Owner, to: Owner, target: Target },
    /// Выбросить предмет на землю перед собой.
    Drop { uid: ItemUid, from: Owner },
    /// Закрыть окно обыска.
    Close,
    /// Применить расходник (бинт, зелье) из своего инвентаря.
    Use { uid: ItemUid },
    /// Прервать применение.
    CancelUse,
    /// Отладка, пока нет врагов: положить перед игроком тело рыцаря с добычей (F8).
    DebugSpawnCorpse,
}

/// Сервер -> клиент.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LootServerMessage {
    /// Полное состояние своего инвентаря.
    Inventory(Inventory),
    /// Открылось тело: что в нём.
    ContainerOpened { id: LootId, title: String, inv: Inventory },
    ContainerUpdated { id: LootId, inv: Inventory },
    ContainerClosed,
    /// Тело с добычей. `own_model == false` — модель тела уже рисует боевая система
    /// (`CorpseMarker` с тем же id), нужно только привязать к ней обыск.
    CorpseSpawned { id: LootId, pos: NetPos, yaw: f32, name: String, empty: bool, own_model: bool },
    CorpseEmpty { id: LootId, empty: bool },
    /// Предмет падает на землю: из точки `start` со скоростью `vel`, остановится в `rest`.
    GroundSpawned { id: LootId, start: NetPos, vel: NetPos, rest: NetPos, base: ItemBaseId, quality: Quality, count: u16 },
    Removed { id: LootId },
    /// Началось применение расходника: подпись и длительность полоски.
    UseStarted { verb: String, duration: f32 },
    /// Применение закончилось (`ok == false` — прервано).
    UseFinished { ok: bool },
    /// Короткое сообщение игроку («Нет места»).
    Notice(String),
}

/// Тело падающего предмета для столкновений.
pub const DROP_BODY: Body = Body { half_width: 0.12, height: 0.12 };
pub const DROP_GRAVITY: f32 = 18.0;

/// Один шаг полёта брошенного предмета. Возвращает true, когда он лёг.
pub fn step_drop(level: &LevelColliders, pos: &mut Vec3, vel: &mut Vec3, dt: f32) -> bool {
    vel.y -= DROP_GRAVITY * dt;
    let r = level.move_body(&DROP_BODY, pos, *vel * dt, false);
    if r.blocked_x {
        vel.x *= -0.3;
    }
    if r.blocked_z {
        vel.z *= -0.3;
    }
    if r.hit_ceiling {
        vel.y = 0.0;
    }
    if r.grounded {
        // Трение о землю.
        vel.x *= 0.5;
        vel.z *= 0.5;
        vel.y = 0.0;
        return Vec3::new(vel.x, 0.0, vel.z).length() < 0.3;
    }
    false
}

/// Где остановится предмет, брошенный из `start` со скоростью `vel`.
pub fn simulate_drop(level: &LevelColliders, start: Vec3, vel: Vec3) -> Vec3 {
    let (mut pos, mut vel) = (start, vel);
    for _ in 0..(60 * 6) {
        if step_drop(level, &mut pos, &mut vel, 1.0 / 60.0) {
            break;
        }
        if pos.y < -30.0 {
            return start;
        }
    }
    pos
}
