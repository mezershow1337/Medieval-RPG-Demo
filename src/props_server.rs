//! Сервер: физика всех предметов в мире (выброшенных, упавших, товаров на прилавках)
//! и предметы в руках игроков.
//!
//! Каждый кадр (60 Гц): руки тянут свои предметы к точке перед глазами, люди расталкивают
//! лежащее, `props::step_bodies` двигает и сталкивает тела. Движущиеся тела рассылаются
//! ~30 раз в секунду по ненадёжному каналу [`CH_PROPS`] — каждому игроку только те, что рядом с ним;
//! улёгшиеся — один раз надёжно и всем (`PropRest`), так что лежащие предметы сеть не нагружают.

use crate::collision::Body;
use crate::items::Stat;
use crate::loot_net::*;
use crate::loot_server::LootWorld;
use crate::props::*;
use crate::protocol::encode;
use crate::server::GameWorld;
use bevy::prelude::*;
use bevy_renet::RenetServer;
use std::collections::HashMap;

/// Прилавки и столы торговцев для физики (заполняет торговля).
#[derive(Resource, Default, Clone)]
pub struct PropStatics(pub Vec<Obb>);

const SNAPSHOT_SECS: f32 = 1.0 / 30.0;
/// Сколько тел в одном пакете снимка.
const SNAPSHOT_CHUNK: usize = 40;
/// Движение предмета шлём игрокам не дальше этого: мелочь дальше не рисуется даже на самой
/// большой дальности деталей (`Settings::detail_distance`). Где он улёгся, узнают все (`PropRest`).
const PROP_RADIUS: f32 = 160.0;

#[derive(Default)]
pub struct PropClock {
    acc: f32,
    snap: f32,
    last_feet: HashMap<u64, Vec3>,
    /// О ком с последнего «улёгся» уже слали движение: им `PropRest` нужен, даже если вещь вернулась на место.
    sent: std::collections::HashSet<LootId>,
}

#[allow(clippy::too_many_arguments)]
pub fn tick_props(
    time: Res<Time>,
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    level: Res<crate::collision::LevelColliders>,
    statics: Option<Res<PropStatics>>,
    mut world: ResMut<LootWorld>,
    mut clock: Local<PropClock>,
) {
    let dt = time.delta_secs().min(0.1);
    let empty = PropStatics::default();
    let statics = statics.as_deref().unwrap_or(&empty);

    // Люди: где стоят и как быстро идут.
    let mut movers = Vec::new();
    for (id, a) in &game.actors {
        if !a.alive {
            continue;
        }
        let body = if a.pose.crouching { Body::CROUCHING } else { Body::STANDING };
        let last = clock.last_feet.insert(*id, a.pose.feet).unwrap_or(a.pose.feet);
        let vel = if dt > 0.0 { ((a.pose.feet - last) / dt).clamp_length_max(8.0) } else { Vec3::ZERO };
        movers.push(Mover { aabb: body.aabb_at(a.pose.feet), vel });
    }
    clock.last_feet.retain(|id, _| game.actors.contains_key(id));

    clock.acc += dt;
    let mut steps = 0;
    while clock.acc >= TICK && steps < 3 {
        clock.acc -= TICK;
        steps += 1;
        step_world(&mut server, &game, &level, &statics.0, &movers, &mut world, &mut clock.sent);
    }
    if steps == 3 {
        clock.acc = 0.0;
    }

    clock.snap += dt;
    if clock.snap >= SNAPSHOT_SECS {
        clock.snap = 0.0;
        // Разбуженное прохожим, но не сдвинутое (так почти всегда с вещами в домах) — не шлём.
        let moving: Vec<PropState> = world
            .ground
            .iter()
            .filter(|(_, g)| g.holder.is_some() || (!g.body.asleep && g.body.moved_since_rest()))
            .map(|(id, g)| PropState::new(*id, g.body.pos, g.body.rot, g.body.vel))
            .collect();
        clock.sent.extend(moving.iter().map(|p| p.id));
        if !moving.is_empty() {
            let everyone = crate::server::snapshot_everyone();
            for client in server.clients_id() {
                let center = game.actors.get(&client).map(|a| a.pose.feet).filter(|_| !everyone);
                let near: Vec<PropState> = moving.iter().filter(|p| center.is_none_or(|c| c.distance_squared(Vec3::from_array(p.pos)) < PROP_RADIUS * PROP_RADIUS)).copied().collect();
                for chunk in near.chunks(SNAPSHOT_CHUNK) {
                    server.send_message(client, CH_PROPS, encode(&PropsSnapshot(chunk.to_vec())));
                }
            }
        }
    }
}

/// Клетка (2 м) для поиска прохожих рядом с вещью.
fn wake_cell(p: Vec3) -> (i32, i32) {
    ((p.x / 2.0).floor() as i32, (p.z / 2.0).floor() as i32)
}

#[allow(clippy::too_many_arguments)]
fn step_world(server: &mut RenetServer, game: &GameWorld, level: &crate::collision::LevelColliders, statics: &[Obb], movers: &[Mover], world: &mut LootWorld, sent: &mut std::collections::HashSet<LootId>) {
    // Руки: куда тянуть каждый предмет.
    let mut holds: HashMap<LootId, Hold> = HashMap::new();
    let mut drop: Vec<(u64, &'static str)> = Vec::new();
    let players: Vec<u64> = world.holds.keys().copied().collect();
    for pid in players {
        let h = world.holds[&pid].clone();
        let actor = game.actors.get(&pid).filter(|a| a.alive);
        let (Some(actor), Some(g)) = (actor, world.ground.get(&h.id)) else {
            drop.push((pid, ""));
            continue;
        };
        let eye = actor.pose.feet + Vec3::Y * actor.pose.eye_height();
        let target = hold_point(eye, actor.pose.yaw, actor.pose.pitch, h.dist);
        if g.body.pos.distance(eye) > GRAB_RANGE + 2.5 {
            drop.push((pid, "Выскользнуло из рук"));
            continue;
        }
        let strength = world.equipped_stats(pid).get(Stat::Strength);
        let target_rot = Quat::from_rotation_y(actor.pose.yaw - h.yaw0) * h.rot0;
        holds.insert(h.id, Hold { target, target_rot, max_force: hand_force(strength) });
        // Застрял (зажат между ящиками, слишком тяжёл): через секунду руки разжимаются.
        let far = g.body.pos.distance(target) > 1.3;
        let hs = world.holds.get_mut(&pid).unwrap();
        hs.stuck = if far { hs.stuck + TICK } else { 0.0 };
        if hs.stuck > 1.2 {
            drop.push((pid, "Не удержать"));
        }
    }
    for (pid, why) in drop {
        world.release(server, pid, None);
        if !why.is_empty() {
            server.send_message(pid, CH_LOOT, encode(&LootServerMessage::Notice(why.into())));
        }
    }

    // Люди будят лежащее, на что наступили. Идущих раскладываем по клеткам: спящих вещей
    // в домах тысячи, а сверять каждую с каждым прохожим дорого.
    let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (k, m) in movers.iter().enumerate() {
        if m.vel.length() <= 0.3 {
            continue;
        }
        let (x0, z0) = wake_cell(m.aabb.min - Vec3::splat(0.6));
        let (x1, z1) = wake_cell(m.aabb.max + Vec3::splat(0.6));
        for x in x0..=x1 {
            for z in z0..=z1 {
                cells.entry((x, z)).or_default().push(k);
            }
        }
    }
    if !cells.is_empty() {
        for g in world.ground.values_mut() {
            if !g.body.asleep {
                continue;
            }
            let Some(near) = cells.get(&wake_cell(g.body.pos)) else { continue };
            let a = g.body.obb().aabb();
            if near.iter().any(|k| movers[*k].aabb.overlaps(&crate::collision::Aabb::new(a.min - Vec3::splat(0.03), a.max + Vec3::splat(0.03)))) {
                g.body.wake();
            }
        }
    }

    // Всё спит и никто ничего не держит (обычное дело): шагать нечего. Иначе на каждом шаге
    // собирали бы списки из тысяч спящих вещей домов.
    if holds.is_empty() && world.ground.values().all(|g| g.body.asleep) {
        return;
    }
    let env = Env { level, statics, movers };
    let mut ids = Vec::with_capacity(world.ground.len());
    let mut list = Vec::with_capacity(world.ground.len());
    for (id, g) in world.ground.iter_mut() {
        ids.push(*id);
        let hold = holds.get(id).copied();
        list.push((&mut g.body, hold));
    }
    let slept = step_bodies(&mut list, &env, TICK);
    for i in slept {
        let id = ids[i];
        let Some(g) = world.ground.get_mut(&id) else { continue };
        // Не сдвинулось и движения никому не слали — клиенты и так видят его на месте. Место покоя
        // при этом прежнее (о нём клиенты и знают): мелкое ёрзанье не копится незаметно для них.
        if sent.remove(&id) || g.body.moved_since_rest() {
            server.broadcast_message(CH_LOOT, encode(&LootServerMessage::PropRest { id, pos: g.body.pos.into(), rot: g.body.rot.to_array() }));
            g.body.rest = Some((g.body.pos, g.body.rot));
        }
    }
    sent.retain(|id| world.ground.contains_key(id));

    // Упавшее сквозь мир (не должно быть, но мало ли): кладём обратно на землю. Спящее не двигается.
    for (id, g) in world.ground.iter_mut() {
        if !g.body.asleep && (g.body.pos.y < -40.0 || !g.body.pos.is_finite()) {
            let p = if g.body.pos.is_finite() { g.body.pos } else { crate::level::spawn_point(0) };
            let ground = level.ground_height(p.x, p.z);
            let y = if ground.is_finite() { ground } else { crate::level::spawn_point(0).y };
            g.body.pos = Vec3::new(p.x, y + 1.0, p.z);
            g.body.vel = Vec3::ZERO;
            g.body.wake();
            let _ = id;
        }
    }
}
