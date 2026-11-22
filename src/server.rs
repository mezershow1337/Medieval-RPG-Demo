//! Сервер без графики. Хранит всех бойцов (игроков и врагов), считает бой,
//! проверяет перемещения игроков и рассылает снимки мира.

use crate::collision::{Body, LevelColliders};
use crate::combat::{self, Fighter, HitOutcome, Pose, Swing, TickEvent};
use crate::level;
use crate::npc::{self, Brain, NpcSpawn, Rng, Seen};
use crate::protocol::*;
use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy_renet::netcode::{NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig};
use bevy_renet::{RenetServer, RenetServerEvent, RenetServerPlugin, renet::ServerEvent};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::time::{Duration, SystemTime};

/// Сколько метров по горизонтали игрок может пройти за секунду (с запасом на лаги).
const MAX_HORIZONTAL_SPEED: f32 = 9.0;
const MAX_VERTICAL_SPEED: f32 = 40.0;
const PLAYER_RESPAWN_SECS: f64 = 5.0;
const NPC_RESPAWN_SECS: f64 = 60.0;
/// Сколько секунд после появления враги не нападают на игрока.
const SPAWN_PROTECTION_SECS: f64 = 4.0;
const MAX_CORPSES: usize = 40;
const NPC_ID_BASE: u64 = 1 << 62;
const CORPSE_ID_BASE: u64 = 1 << 61;

pub struct Actor {
    pub kind: ActorKind,
    pub pose: Pose,
    pub fighter: Fighter,
    pub alive: bool,
    last_update: f64,
    respawn_at: Option<f64>,
    protected_until: f64,
    brain: Option<Brain>,
    spawn: Option<NpcSpawn>,
    /// Участок траектории удара, пройденный за текущий кадр: (удар, от, до).
    sweep: Option<(Swing, f32, f32)>,
}

/// Тело на земле. Предметы для обыска добавит система лута.
pub struct Corpse {
    pub id: u64,
    pub kind: ActorKind,
    pub pos: Vec3,
    pub yaw: f32,
}

#[derive(Resource)]
pub struct GameWorld {
    pub actors: HashMap<u64, Actor>,
    pub corpses: Vec<Corpse>,
    joined_count: usize,
    next_corpse_id: u64,
    npc_respawns: Vec<(f64, NpcSpawn)>,
    next_npc_id: u64,
    rng: Rng,
}

#[derive(Resource)]
struct SnapshotTimer {
    timer: Timer,
    tick: u32,
}

pub fn bind_socket(port: u16) -> std::io::Result<UdpSocket> {
    UdpSocket::bind(("0.0.0.0", port))
}

/// Запускает сервер в текущем потоке (блокирует до выхода).
pub fn run(socket: UdpSocket, with_logging: bool) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))));
    if with_logging {
        app.add_plugins(bevy::log::LogPlugin::default());
    }
    let addr = socket.local_addr().expect("адрес сокета");
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let config = ServerConfig {
        current_time: now,
        max_clients: MAX_PLAYERS,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![addr],
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = NetcodeServerTransport::new(config, socket).expect("не удалось запустить сервер");
    let level = level::level_colliders();
    let mut world = GameWorld {
        actors: HashMap::new(),
        corpses: Vec::new(),
        joined_count: 0,
        next_corpse_id: CORPSE_ID_BASE,
        npc_respawns: Vec::new(),
        next_npc_id: NPC_ID_BASE,
        rng: Rng::new(now.as_nanos() as u64),
    };
    for spawn in npc::npc_spawns() {
        spawn_npc(&mut world, &level, spawn, 0.0);
    }
    app.add_plugins((RenetServerPlugin, NetcodeServerPlugin))
        .insert_resource(RenetServer::new(connection_config()))
        .insert_resource(transport)
        .insert_resource(level)
        .insert_resource(world)
        .insert_resource(SnapshotTimer { timer: Timer::from_seconds((1.0 / SNAPSHOT_RATE_HZ) as f32, TimerMode::Repeating), tick: 0 })
        .add_observer(on_server_event)
        .add_systems(Update, (receive_client_messages, run_npcs, combat_tick, respawns, send_snapshots).chain());
    info!("Сервер слушает {addr}");
    app.run();
}

fn spawn_npc(world: &mut GameWorld, level: &LevelColliders, spawn: NpcSpawn, now: f64) {
    let id = world.next_npc_id;
    world.next_npc_id += 1;
    let mut spawn = spawn;
    spawn.pos = npc::find_free_spot(level, spawn.pos);
    world.actors.insert(
        id,
        Actor {
            kind: spawn.kind,
            pose: Pose { feet: spawn.pos, yaw: spawn.yaw, pitch: 0.0, crouching: false },
            fighter: npc::fighter_for(spawn.kind),
            alive: true,
            last_update: now,
            respawn_at: None,
            protected_until: 0.0,
            brain: Some(Brain::new(&spawn)),
            spawn: Some(spawn),
            sweep: None,
        },
    );
}

fn on_server_event(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, mut world: ResMut<GameWorld>, time: Res<Time>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let id = *client_id;
            let spawn = level::spawn_point(world.joined_count);
            world.joined_count += 1;
            info!("Игрок {id} подключился");
            server.send_message(id, CH_RELIABLE, encode(&ServerMessage::Welcome { your_id: id, spawn: spawn.into() }));
            for c in &world.corpses {
                let msg = ServerMessage::CorpseSpawned { corpse_id: c.id, kind: c.kind, pos: c.pos.into(), yaw: c.yaw };
                server.send_message(id, CH_RELIABLE, encode(&msg));
            }
            server.broadcast_message_except(id, CH_RELIABLE, encode(&ServerMessage::PlayerJoined { id }));
            world.actors.insert(
                id,
                Actor {
                    kind: ActorKind::Player,
                    pose: Pose { feet: spawn, yaw: 0.0, pitch: 0.0, crouching: false },
                    fighter: npc::fighter_for(ActorKind::Player),
                    alive: true,
                    last_update: time.elapsed_secs_f64(),
                    respawn_at: None,
                    protected_until: time.elapsed_secs_f64() + SPAWN_PROTECTION_SECS,
                    brain: None,
                    spawn: None,
                    sweep: None,
                },
            );
        }
        ServerEvent::ClientDisconnected { client_id, reason } => {
            info!("Игрок {client_id} отключился: {reason}");
            world.actors.remove(client_id);
            server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::PlayerLeft { id: *client_id }));
        }
    }
}

fn receive_client_messages(mut server: ResMut<RenetServer>, mut world: ResMut<GameWorld>, level: Res<LevelColliders>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    for id in server.clients_id() {
        // Боевые команды.
        while let Some(bytes) = server.receive_message(id, CH_RELIABLE) {
            let Some(msg) = decode::<ClientMessage>(&bytes) else { continue };
            let Some(actor) = world.actors.get_mut(&id) else { continue };
            if !actor.alive {
                continue;
            }
            match msg {
                ClientMessage::Attack { yaw, pitch } => {
                    actor.pose.yaw = yaw;
                    actor.pose.pitch = pitch.clamp(-1.6, 1.6);
                    actor.fighter.try_attack();
                }
                ClientMessage::Parry { yaw, pitch } => {
                    actor.pose.yaw = yaw;
                    actor.pose.pitch = pitch.clamp(-1.6, 1.6);
                    actor.fighter.try_parry();
                }
                ClientMessage::PlayerState { .. } => {}
            }
        }
        // Положение.
        while let Some(bytes) = server.receive_message(id, CH_UNRELIABLE) {
            let Some(ClientMessage::PlayerState { pos, yaw, pitch, crouching }) = decode::<ClientMessage>(&bytes) else {
                continue;
            };
            let Some(player) = world.actors.get_mut(&id) else { continue };
            if !player.alive {
                continue;
            }
            let new_pos: Vec3 = pos.into();
            let dt = (now - player.last_update).max(1.0 / 60.0) as f32;
            let d = new_pos - player.pose.feet;
            let horizontal = Vec2::new(d.x, d.z).length();
            let body = if crouching { Body::CROUCHING } else { Body::STANDING };
            let too_fast = horizontal > MAX_HORIZONTAL_SPEED * dt + 0.5 || d.y.abs() > MAX_VERTICAL_SPEED * dt + 0.6;
            let in_wall = !level.body_is_free(&body, new_pos);
            if !new_pos.is_finite() || too_fast || in_wall {
                server.send_message(id, CH_RELIABLE, encode(&ServerMessage::Correction { pos: player.pose.feet.into() }));
                player.last_update = now;
                continue;
            }
            player.pose = Pose { feet: new_pos, yaw, pitch: pitch.clamp(-1.6, 1.6), crouching };
            player.last_update = now;
        }
    }
}

fn run_npcs(mut world: ResMut<GameWorld>, level: Res<LevelColliders>, time: Res<Time>) {
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs_f64();
    let seen: Vec<Seen> = world
        .actors
        .iter()
        .map(|(id, a)| Seen { id: *id, kind: a.kind, pose: a.pose, action: a.fighter.action, weapon: a.fighter.weapon, alive: a.alive, targetable: now >= a.protected_until })
        .collect();
    let GameWorld { actors, rng, .. } = &mut *world;
    for actor in actors.values_mut() {
        if !actor.alive {
            continue;
        }
        let Some(brain) = actor.brain.as_mut() else { continue };
        npc::think(brain, actor.kind, &mut actor.pose, &mut actor.fighter, &seen, &level, rng, dt);
    }
    // Не даём врагам влезать в других бойцов: отталкиваем их.
    for (id, actor) in actors.iter_mut() {
        if actor.brain.is_none() || !actor.alive {
            continue;
        }
        for other in seen.iter().filter(|o| o.id != *id && o.alive) {
            let d = actor.pose.feet - other.pose.feet;
            let flat = Vec2::new(d.x, d.z);
            let dist = flat.length();
            const MIN_DIST: f32 = 0.75;
            if dist < MIN_DIST && (d.y.abs() < 1.5) {
                let push = if dist > 1e-3 { flat / dist } else { Vec2::X } * (MIN_DIST - dist);
                let candidate = actor.pose.feet + Vec3::new(push.x, 0.0, push.y);
                if level.body_is_free(&Body::STANDING, candidate) {
                    actor.pose.feet = candidate;
                }
            }
        }
    }
}

fn combat_tick(mut server: ResMut<RenetServer>, mut world: ResMut<GameWorld>, level: Res<LevelColliders>, time: Res<Time>) {
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs_f64();

    // 1. Таймеры и участок траектории удара за этот кадр.
    for actor in world.actors.values_mut() {
        if !actor.alive {
            actor.sweep = None;
            continue;
        }
        let before = actor.fighter.strike_progress();
        let event = actor.fighter.tick(dt);
        let after = actor.fighter.strike_progress();
        actor.sweep = match (before, after, event) {
            (_, Some((swing, p)), Some(TickEvent::StrikeStarted)) => Some((swing, 0.0, p)),
            (Some((swing, a)), Some((_, b)), _) => Some((swing, a, b)),
            (Some((swing, a)), None, Some(TickEvent::StrikeEnded)) => Some((swing, a, 1.0)),
            _ => None,
        };
    }

    // 2. Поиск попаданий: клинок против тел.
    let mut hits: Vec<(u64, u64, Vec3)> = Vec::new();
    for (&aid, a) in &world.actors {
        let Some((swing, from, to)) = a.sweep else { continue };
        for (&tid, t) in &world.actors {
            if tid == aid || !t.alive || a.fighter.hit_this_swing.contains(&tid) {
                continue;
            }
            // Враги не бьют друг друга.
            if a.kind.is_npc() && t.kind.is_npc() {
                continue;
            }
            if a.pose.feet.distance(t.pose.feet) > 4.0 {
                continue;
            }
            for k in 0..=4 {
                let p = from + (to - from) * k as f32 / 4.0;
                let (hilt, tip) = combat::blade_segment(&a.pose, a.fighter.weapon, swing, p);
                if let Some(point) = combat::blade_hits_body(hilt, tip, &t.pose) {
                    if combat::line_of_sight(&level, a.pose.shoulder(), point) {
                        hits.push((aid, tid, point));
                    }
                    break;
                }
            }
        }
    }

    // 3. Применяем попадания.
    for (aid, tid, point) in hits {
        let Some(mut attacker) = world.actors.remove(&aid) else { continue };
        let still_striking = matches!(attacker.fighter.action, combat::Action::Strike { .. });
        let outcome = match world.actors.get_mut(&tid) {
            Some(target) if still_striking && target.alive => {
                attacker.fighter.hit_this_swing.push(tid);
                let outcome = combat::resolve_hit(&mut attacker.fighter, &attacker.pose, &mut target.fighter, &target.pose, npc::damage_mult(attacker.kind));
                Some(outcome)
            }
            _ => None,
        };
        world.actors.insert(aid, attacker);
        let Some(outcome) = outcome else { continue };
        debug!("Попадание {aid} -> {tid}: {outcome:?}");
        server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::Hit { attacker: aid, target: tid, outcome, at: point.into() }));
        if let HitOutcome::Damage { killed: true, .. } = outcome {
            kill(&mut server, &mut world, tid, Some(aid), now);
        }
    }
}

fn kill(server: &mut RenetServer, world: &mut GameWorld, id: u64, killer: Option<u64>, now: f64) {
    let Some(actor) = world.actors.get_mut(&id) else { return };
    actor.alive = false;
    actor.fighter.action = combat::Action::Dead;
    let (kind, pos, yaw) = (actor.kind, actor.pose.feet, actor.pose.yaw);
    if kind == ActorKind::Player {
        actor.respawn_at = Some(now + PLAYER_RESPAWN_SECS);
    } else if let Some(spawn) = actor.spawn {
        world.actors.remove(&id);
        world.npc_respawns.push((now + NPC_RESPAWN_SECS, spawn));
    }
    info!("{kind:?} {id} погиб");
    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::Died { id, killer }));

    let corpse_id = world.next_corpse_id;
    world.next_corpse_id += 1;
    world.corpses.push(Corpse { id: corpse_id, kind, pos, yaw });
    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CorpseSpawned { corpse_id, kind, pos: pos.into(), yaw }));
    if world.corpses.len() > MAX_CORPSES {
        let old = world.corpses.remove(0);
        server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CorpseRemoved { corpse_id: old.id }));
    }
}

fn respawns(mut server: ResMut<RenetServer>, mut world: ResMut<GameWorld>, level: Res<LevelColliders>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    let world = &mut *world;
    for (&id, actor) in world.actors.iter_mut() {
        if actor.respawn_at.is_some_and(|t| now >= t) {
            let spawn = level::spawn_point((world.rng.next_f32() * 8.0) as usize);
            actor.respawn_at = None;
            actor.alive = true;
            actor.fighter = npc::fighter_for(actor.kind);
            actor.pose.feet = spawn;
            actor.last_update = now;
            actor.protected_until = now + SPAWN_PROTECTION_SECS;
            server.send_message(id, CH_RELIABLE, encode(&ServerMessage::Respawned { pos: spawn.into() }));
        }
    }
    let (due, waiting): (Vec<_>, Vec<_>) = world.npc_respawns.drain(..).partition(|(t, _)| now >= *t);
    world.npc_respawns = waiting;
    for (_, spawn) in due {
        spawn_npc(world, &level, spawn, now);
    }
}

fn send_snapshots(mut server: ResMut<RenetServer>, world: Res<GameWorld>, mut timer: ResMut<SnapshotTimer>, time: Res<Time>) {
    if !timer.timer.tick(time.delta()).just_finished() {
        return;
    }
    timer.tick = timer.tick.wrapping_add(1);
    let snapshot = Snapshot {
        tick: timer.tick,
        actors: world
            .actors
            .iter()
            .filter(|(_, a)| a.alive)
            .map(|(id, a)| ActorSnapshot {
                id: *id,
                kind: a.kind,
                pos: a.pose.feet.into(),
                yaw: a.pose.yaw,
                pitch: a.pose.pitch,
                crouching: a.pose.crouching,
                weapon: a.fighter.weapon,
                action: a.fighter.action,
                hp: a.fighter.hp,
                max_hp: a.fighter.max_hp,
            })
            .collect(),
    };
    let bytes = encode(&snapshot);
    server.broadcast_message(CH_UNRELIABLE, bytes);
}
