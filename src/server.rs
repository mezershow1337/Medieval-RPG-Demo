//! Сервер без графики. Хранит всех бойцов (игроков и врагов), считает бой,
//! проверяет перемещения игроков и рассылает снимки мира.

use crate::collision::{Body, LevelColliders};
use crate::combat::{self, Arrow, ArrowStep, Contact, Fighter, HitOutcome, Pose, Swing, TickEvent};
use crate::items::{EquipSlot, ItemShape, Stat};
use crate::loot_server::LootWorld;
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
    pub(crate) last_update: f64,
    respawn_at: Option<f64>,
    protected_until: f64,
    pub brain: Option<Brain>,
    spawn: Option<NpcSpawn>,
    /// Участок траектории удара, пройденный за текущий кадр: (удар, от, до).
    sweep: Option<(Swing, f32, f32)>,
    /// Множитель скорости ходьбы от экипировки (для своего клиента).
    move_mult: f32,
    /// Вид оружия в руке для снимков (`Item::look` × 255).
    finish: u8,
    /// В этом кадре выпущена стрела.
    shoot: bool,
    /// Как раны, боль и яды меняют бойца (ведёт medicine_server.rs).
    pub body_mods: crate::medicine::BodyMods,
    /// Читы из консоли разработчика.
    pub cheats: crate::console_net::Cheats,
    /// Игрок убрал оружие (хотбар, слот «Руки»): дерётся кулаками.
    holstered: bool,
    /// Что на нём видно (для снимков, чтобы другие видели доспех).
    gear: crate::gear_look::Gear,
}

impl Actor {
    /// Вызванный консолью враг: после смерти не возрождается.
    pub fn forget_spawn(&mut self) {
        self.spawn = None;
    }
}

/// Тело на земле. Предметы для обыска добавит система лута.
pub struct Corpse {
    pub id: u64,
    /// Кто погиб (id игрока или врага) и кто убил — нужны системе лута.
    pub actor: u64,
    pub killer: Option<u64>,
    pub kind: ActorKind,
    pub pos: Vec3,
    pub yaw: f32,
    /// Во что был одет (тело лежит в своих доспехах).
    pub gear: crate::gear_look::Gear,
    /// Погиб мирный человек (житель или торговец), а не боец.
    pub civ: Option<CivCorpse>,
}

/// Что лежит при мирном человеке.
#[derive(Clone, Debug)]
pub struct CivCorpse {
    pub civ: crate::civilians::Civ,
    pub name: String,
    pub table: crate::items::LootTableId,
    /// Деньги в кошеле, нуммии.
    pub coins: u32,
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
    arrows: Vec<Arrow>,
    next_arrow_id: u64,
    /// Попадания по игрокам за кадр: из них medicine_server.rs делает раны.
    pub hits_taken: Vec<crate::medicine::HitTaken>,
    /// Износ вещей игроков от ударов (применяет smithing_server.rs).
    pub gear_wear: Vec<(u64, crate::smithing_server::Wear)>,
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
    let layout = level::layout();
    let civil = crate::civil_server::CivilWorld::new(&layout.residents);
    let level = level::colliders(layout);
    let mut world = GameWorld {
        actors: HashMap::new(),
        corpses: Vec::new(),
        joined_count: 0,
        next_corpse_id: CORPSE_ID_BASE,
        npc_respawns: Vec::new(),
        next_npc_id: NPC_ID_BASE,
        rng: Rng::new(now.as_nanos() as u64),
        arrows: Vec::new(),
        next_arrow_id: 1,
        hits_taken: Vec::new(),
        gear_wear: Vec::new(),
    };
    for spawn in npc::npc_spawns() {
        spawn_npc(&mut world, &level, spawn, 0.0);
    }
    app.add_plugins((RenetServerPlugin, NetcodeServerPlugin))
        .insert_resource(RenetServer::new(connection_config()))
        .insert_resource(transport)
        .insert_resource(level)
        .insert_resource(world)
        .insert_resource(civil)
        .add_plugins(crate::civil_server::CivilServerPlugin)
        .insert_resource(SnapshotTimer { timer: Timer::from_seconds((1.0 / SNAPSHOT_RATE_HZ) as f32, TimerMode::Repeating), tick: 0 })
        .add_observer(on_server_event)
        .add_plugins(crate::loot_server::LootServerPlugin)
        .add_plugins(crate::doors::DoorServerPlugin)
        .add_plugins(crate::map::MapServerPlugin)
        .add_plugins(crate::trade_server::TradeServerPlugin)
        .add_plugins(crate::knowledge::KnowledgeServerPlugin)
        .add_plugins(crate::smithing_server::SmithingServerPlugin)
        .add_plugins(crate::console_server::ConsoleServerPlugin)
        .add_systems(Update, (receive_client_messages, run_npcs, combat_tick, respawns, send_snapshots).chain());
    info!("Сервер слушает {addr}");
    app.run();
}

pub(crate) fn spawn_npc(world: &mut GameWorld, level: &LevelColliders, spawn: NpcSpawn, now: f64) -> u64 {
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
            move_mult: 1.0,
            finish: 200,
            shoot: false,
            body_mods: Default::default(),
            cheats: Default::default(),
            holstered: false,
            gear: Default::default(),
        },
    );
    id
}

fn on_server_event(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, mut world: ResMut<GameWorld>, time: Res<Time>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let id = *client_id;
            let spawn = level::spawn_point(world.joined_count);
            world.joined_count += 1;
            info!("Игрок {id} подключился");
            server.send_message(id, CH_RELIABLE, encode(&ServerMessage::Welcome { your_id: id, spawn: spawn.into() }));
            for c in world.corpses.iter().filter(|c| c.civ.is_none()) {
                let msg = ServerMessage::CorpseSpawned { corpse_id: c.id, kind: c.kind, pos: c.pos.into(), yaw: c.yaw, gear: c.gear };
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
                    move_mult: 1.0,
                    finish: 200,
                    shoot: false,
                    body_mods: Default::default(),
                    cheats: Default::default(),
                    holstered: false,
                    gear: Default::default(),
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

fn receive_client_messages(mut server: ResMut<RenetServer>, mut world: ResMut<GameWorld>, mut civil: ResMut<crate::civil_server::CivilWorld>, level: Res<LevelColliders>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    for id in server.clients_id() {
        // Боевые команды.
        while let Some(bytes) = server.receive_message(id, CH_RELIABLE) {
            let Some(msg) = decode::<ClientMessage>(&bytes) else { continue };
            if let ClientMessage::Law(answer) = msg {
                civil.answers.push((id, answer));
                continue;
            }
            let Some(actor) = world.actors.get_mut(&id) else { continue };
            if !actor.alive {
                continue;
            }
            // В карцере не дерутся.
            if civil.jailed(id) && matches!(msg, ClientMessage::Attack { .. }) {
                continue;
            }
            match msg {
                ClientMessage::Attack { yaw, pitch, swing } => {
                    actor.pose.yaw = yaw;
                    actor.pose.pitch = pitch.clamp(-1.6, 1.6);
                    actor.fighter.try_attack(swing);
                }
                ClientMessage::Feint => {
                    actor.fighter.feint();
                }
                ClientMessage::Guard { on, yaw, pitch } => {
                    actor.pose.yaw = yaw;
                    actor.pose.pitch = pitch.clamp(-1.6, 1.6);
                    actor.fighter.set_guard(on);
                }
                ClientMessage::Holster { on } => actor.holstered = on,
                ClientMessage::Law(_) | ClientMessage::PlayerState { .. } => {}
            }
        }
        // Положение.
        while let Some(bytes) = server.receive_message(id, CH_UNRELIABLE) {
            let Some(ClientMessage::PlayerState { pos, yaw, pitch, crouching, sprinting }) = decode::<ClientMessage>(&bytes) else {
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
            // Полёт и ускорение из консоли разработчика: не проверяем скорость и стены.
            let free = player.cheats.free_move();
            if !new_pos.is_finite() || (!free && (too_fast || in_wall)) {
                server.send_message(id, CH_RELIABLE, encode(&ServerMessage::Correction { pos: player.pose.feet.into() }));
                player.last_update = now;
                continue;
            }
            player.pose = Pose { feet: new_pos, yaw, pitch: pitch.clamp(-1.6, 1.6), crouching };
            player.fighter.sprinting = sprinting && player.fighter.can_sprint();
            player.last_update = now;
        }
    }
}

fn run_npcs(mut world: ResMut<GameWorld>, level: Res<LevelColliders>, time: Res<Time>, trade: Option<Res<crate::trade_server::TradeWorld>>) {
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs_f64();
    // Кого из игроков фракции считают врагами (по репутации).
    let players: Vec<u64> = world.actors.iter().filter(|(_, a)| a.kind == ActorKind::Player).map(|(id, _)| *id).collect();
    for actor in world.actors.values_mut() {
        let Some(brain) = actor.brain.as_mut() else { continue };
        brain.hostile_players.clear();
        if let (Some(f), Some(trade)) = (brain.faction, trade.as_deref()) {
            brain.hostile_players.extend(players.iter().copied().filter(|p| trade.standing(*p, f) == crate::factions::Standing::Hostile));
        }
        brain.provoked.retain(|p| players.contains(p));
    }
    let seen: Vec<Seen> = world
        .actors
        .iter()
        .map(|(id, a)| Seen {
            id: *id,
            kind: a.kind,
            pose: a.pose,
            action: a.fighter.action,
            weapon: a.fighter.weapon,
            shield: a.fighter.has_shield,
            alive: a.alive,
            targetable: now >= a.protected_until && !a.cheats.notarget,
        })
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

/// Базовое здоровье игрока (плюс «Здоровье» с вещей).
pub const PLAYER_BASE_HP: f32 = 150.0;
/// Урон оружия игрока = «Урон оружия» предмета × этот множитель + «Доп. урон».
const PLAYER_WEAPON_DAMAGE_SCALE: f32 = 0.75;

/// Экипировка игроков влияет на бой: здоровье, оружие, щит, скорость ударов, выносливость.
fn sync_equipment(world: &mut GameWorld, loot: Option<&LootWorld>) {
    let Some(loot) = loot else { return };
    for (id, actor) in world.actors.iter_mut() {
        if actor.kind != ActorKind::Player || !actor.alive {
            continue;
        }
        let stats = loot.equipped_stats(*id);
        let max_hp = PLAYER_BASE_HP + stats.get(Stat::MaxHealth);
        if (max_hp - actor.fighter.max_hp).abs() > 0.01 {
            let k = actor.fighter.hp / actor.fighter.max_hp.max(1.0);
            actor.fighter.max_hp = max_hp;
            actor.fighter.hp = (max_hp * k).min(max_hp);
        }
        let inv = loot.inventories.get(id);
        actor.fighter.riposte_bonus = stats.get(Stat::ParryWindow) / 1000.0;
        actor.fighter.has_shield = inv.and_then(|inv| inv.equipped(EquipSlot::OffHand)).is_some_and(|item| item.info().shape == ItemShape::Shield);
        // Оружие в руке определяет темп, длину и силу ударов. Менять можно только вне удара.
        actor.finish = inv.and_then(|inv| inv.equipped(EquipSlot::MainHand)).map_or(200, |item| (item.look() * 255.0) as u8);
        // Пустая правая рука или оружие убрано — кулаки.
        let weapon = inv
            .and_then(|inv| inv.equipped(EquipSlot::MainHand))
            .filter(|_| !actor.holstered)
            .map(|item| combat::WeaponKind::from_item_name(item.info().name))
            .unwrap_or(combat::WeaponKind::Fists);
        if weapon != actor.fighter.weapon && matches!(actor.fighter.action, combat::Action::Idle | combat::Action::Guard { .. }) {
            actor.fighter.weapon = weapon;
            if !weapon.can_guard_with(actor.fighter.has_shield) {
                actor.fighter.set_guard(false);
            }
        }
        actor.gear = inv.map(crate::gear_look::Gear::from_inventory).unwrap_or_default();
        actor.gear.holstered = actor.holstered;
        let e = gear_effects(&stats);
        let m = actor.body_mods;
        actor.fighter.action_speed = e.action_speed * m.action_speed;
        actor.fighter.block_stability = e.block_stability;
        actor.fighter.stamina_regen_mult = e.stamina_regen * m.stamina_regen;
        actor.fighter.max_stamina = e.max_stamina * m.max_stamina;
        actor.fighter.stamina = actor.fighter.stamina.min(actor.fighter.max_stamina);
        // Ноша: сверх грузоподъёмности идёшь медленнее (вес — `weight.rs`).
        let load = inv.map(crate::weight::inventory_weight).unwrap_or(0.0);
        let burden = crate::weight::encumbrance(load, crate::weight::carry_capacity(stats.get(Stat::Strength)));
        actor.move_mult = e.move_mult * m.move_mult * burden;
    }
}

/// Как вещи меняют бой и выносливость.
pub struct GearEffects {
    pub action_speed: f32,
    pub block_stability: f32,
    pub stamina_regen: f32,
    pub max_stamina: f32,
    pub move_mult: f32,
}

pub fn gear_effects(stats: &crate::items::StatTotals) -> GearEffects {
    let armor = stats.get(Stat::Armor).max(0.0);
    // Тяжёлый доспех: медленнее ходишь и дольше переводишь дух (100 брони ≈ −8% скорости, −25% отдыха).
    let weight = armor / (armor + 300.0);
    GearEffects {
        action_speed: 1.0 + stats.get(Stat::ActionSpeed) / 100.0,
        block_stability: (stats.get(Stat::BlockStability) / 100.0).clamp(0.0, 0.8),
        stamina_regen: ((1.0 - weight) * (1.0 + stats.get(Stat::Agility) * 0.01)).max(0.3),
        max_stamina: combat::STAMINA_MAX + stats.get(Stat::Strength) * 2.0,
        move_mult: ((1.0 + stats.get(Stat::MoveSpeed) / 100.0) * (1.0 - weight * 0.32)).clamp(0.6, 1.3),
    }
}

/// Урон оружия атакующего до брони цели.
fn base_damage(world: &GameWorld, loot: Option<&LootWorld>, attacker: u64) -> f32 {
    let Some(a) = world.actors.get(&attacker) else { return 0.0 };
    if a.kind == ActorKind::Player {
        let stats = loot.map(|l| l.equipped_stats(attacker)).unwrap_or_default();
        if a.fighter.weapon == combat::WeaponKind::Fists {
            // Кулак: урон от силы, а не от оружия в сумке.
            return combat::FIST_DAMAGE + stats.get(Stat::Strength).max(0.0) * 0.4 + stats.get(Stat::BonusDamage) * 0.5;
        }
        let weapon = stats.get(Stat::WeaponDamage);
        let weapon = if weapon > 0.0 { weapon * PLAYER_WEAPON_DAMAGE_SCALE } else { a.fighter.weapon.stats().damage };
        weapon + stats.get(Stat::BonusDamage)
    } else {
        a.fighter.weapon.stats().damage * npc::damage_mult(a.kind)
    }
}

/// Урон после брони цели.
fn mitigate(world: &GameWorld, loot: Option<&LootWorld>, target: u64, base: f32, head: bool) -> f32 {
    mitigate_pierced(world, loot, target, base, head, 0.0)
}

/// Урон после брони; `pierce` ослабляет доспех (бронебойный наконечник) или усиливает (срезень).
fn mitigate_pierced(world: &GameWorld, loot: Option<&LootWorld>, target: u64, base: f32, head: bool, pierce: f32) -> f32 {
    let Some(t) = world.actors.get(&target) else { return 0.0 };
    let (armor, head_reduction) = if t.kind == ActorKind::Player {
        let stats = loot.map(|l| l.equipped_stats(target)).unwrap_or_default();
        (stats.get(Stat::Armor), stats.get(Stat::HeadshotReduction))
    } else {
        (npc::armor(t.kind), 0.0)
    };
    let armor = armor * (1.0 - pierce.clamp(-1.0, 0.9));
    let mut dmg = base * (1.0 - combat::armor_reduction(armor));
    if head {
        dmg *= 1.0 - (head_reduction / 100.0).clamp(0.0, 0.8);
    }
    dmg
}

/// Урон удара с учётом оружия атакующего и брони цели.
/// Бессмертный из консоли разработчика.
fn is_god(world: &GameWorld, id: u64) -> bool {
    world.actors.get(&id).is_some_and(|a| a.cheats.god)
}

fn hit_damage(world: &GameWorld, loot: Option<&LootWorld>, attacker: u64, target: u64, head: bool) -> f32 {
    if is_god(world, target) {
        return 0.0;
    }
    mitigate(world, loot, target, base_damage(world, loot, attacker), head)
}

enum Strike {
    Contact { attacker: u64, target: u64, contact: Contact, point: Vec3 },
    Wall { attacker: u64, point: Vec3 },
    /// Клинок задел мирного человека (номер в `CivilWorld::people`).
    Civil { attacker: u64, person: usize, point: Vec3, head: bool },
}

/// Игрок напал на человека фракции: тот и его товарищи рядом дают отпор, фракция злится.
/// На разбойников можно нападать без последствий.
fn on_player_attacks(world: &mut GameWorld, trade: Option<&mut crate::trade_server::TradeWorld>, civil: &mut crate::civil_server::CivilWorld, attacker: u64, target: u64, killed: bool) {
    let (Some(a), Some(t)) = (world.actors.get(&attacker), world.actors.get(&target)) else { return };
    if a.kind != ActorKind::Player {
        return;
    }
    let Some(faction) = t.brain.as_ref().and_then(|b| b.faction) else { return };
    let at = t.pose.feet;
    // В городе это преступление: если кто-то видел, донесут страже.
    if faction.city_law() {
        civil.queue.push(crate::civil_server::Offense { player: attacker, victim: crate::civil_server::Victim::Actor(target), faction, killed });
    }
    for other in world.actors.values_mut() {
        if let Some(b) = other.brain.as_mut()
            && b.faction == Some(faction)
            && other.pose.feet.distance(at) < 30.0
        {
            b.provoke(attacker);
        }
    }
    if let Some(trade) = trade {
        trade.change_rep(attacker, faction, if killed { -20.0 } else { -4.0 });
    }
}

#[allow(clippy::too_many_arguments)]
fn combat_tick(
    mut server: ResMut<RenetServer>,
    mut world: ResMut<GameWorld>,
    level: Res<LevelColliders>,
    loot: Option<Res<LootWorld>>,
    mut trade: Option<ResMut<crate::trade_server::TradeWorld>>,
    mut civil: ResMut<crate::civil_server::CivilWorld>,
    time: Res<Time>,
) {
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs_f64();
    let loot = loot.as_deref();
    sync_equipment(&mut world, loot);
    // Бессмертие из консоли: всегда полное здоровье и силы.
    for a in world.actors.values_mut().filter(|a| a.alive && a.cheats.god) {
        a.fighter.hp = a.fighter.max_hp;
        a.fighter.stamina = a.fighter.max_stamina;
    }
    // Умершие не от удара (кровопотеря, яд, антонов огонь).
    let dying: Vec<u64> = world.actors.iter().filter(|(_, a)| a.alive && a.fighter.hp <= 0.0).map(|(id, _)| *id).collect();
    for id in dying {
        kill(&mut server, &mut world, id, None, now);
    }

    // 1. Таймеры и участок траектории удара за этот кадр.
    for actor in world.actors.values_mut() {
        if !actor.alive {
            actor.sweep = None;
            continue;
        }
        let before = actor.fighter.strike_progress();
        let event = actor.fighter.tick(dt);
        let after = actor.fighter.strike_progress();
        actor.shoot = actor.fighter.weapon.ranged() && event == Some(TickEvent::StrikeStarted);
        actor.sweep = match (before, after, event) {
            (_, Some((swing, p)), Some(TickEvent::StrikeStarted)) => Some((swing, 0.0, p)),
            (Some((swing, a)), Some((_, b)), _) => Some((swing, a, b)),
            (Some((swing, a)), None, Some(TickEvent::StrikeEnded)) => Some((swing, a, 1.0)),
            _ => None,
        };
    }

    // 2. Ведём клинок по траектории: защита и тела целей, потом стены.
    let mut strikes: Vec<Strike> = Vec::new();
    for (&aid, a) in &world.actors {
        let Some((swing, from, to)) = a.sweep else { continue };
        let mut done: Vec<u64> = a.fighter.hit_this_swing.clone();
        let mut done_civ: Vec<usize> = a.fighter.hit_this_swing.iter().filter_map(|id| id.checked_sub(crate::civil_server::CIV_ID_BASE).map(|i| i as usize)).collect();
        'samples: for k in 0..=4 {
            let p = from + (to - from) * k as f32 / 4.0;
            let (hilt, tip) = combat::blade_segment(&a.pose, a.fighter.weapon, swing, p);
            for (&tid, t) in &world.actors {
                if tid == aid || !t.alive || done.contains(&tid) {
                    continue;
                }
                // Свои своих не бьют.
                if !a.kind.hostile(t.kind) {
                    continue;
                }
                if a.pose.feet.distance(t.pose.feet) > 4.0 {
                    continue;
                }
                if let Some(contact) = combat::trace_blade(hilt, tip, &a.pose, &t.fighter, &t.pose) {
                    let point = match contact {
                        Contact::Guard(p) => p,
                        Contact::Body { point, .. } => point,
                    };
                    if combat::line_of_sight(&level, a.pose.shoulder(), point) {
                        done.push(tid);
                        strikes.push(Strike::Contact { attacker: aid, target: tid, contact, point });
                        // Отражённый удар дальше не идёт.
                        if matches!(contact, Contact::Guard(_)) {
                            break 'samples;
                        }
                    }
                }
            }
            // Мирные люди рядом (бьёт их только игрок).
            if a.kind == ActorKind::Player {
                for (person, pose) in civil.targets_near(a.pose.feet, 3.5) {
                    if done_civ.contains(&person) {
                        continue;
                    }
                    if let Some(point) = combat::blade_hits_body(hilt, tip, &pose)
                        && crate::civil_server::walls_clear(&level, a.pose.shoulder(), point)
                    {
                        done_civ.push(person);
                        strikes.push(Strike::Civil { attacker: aid, person, point, head: point.y > pose.feet.y + 1.42 });
                    }
                }
            }
            if let Some(point) = combat::blade_hits_wall(&level, hilt, tip, a.pose.feet) {
                strikes.push(Strike::Wall { attacker: aid, point });
                break;
            }
        }
    }

    // 3. Стрелы: новые и летящие.
    let shooters: Vec<u64> = world.actors.iter().filter(|(_, a)| a.shoot && a.alive).map(|(id, _)| *id).collect();
    for id in shooters {
        let mut damage = base_damage(&world, loot, id);
        let mut pierce = 0.0;
        // Игрок стреляет стрелами из сумки: первой стопкой, что лежит в ней.
        if world.actors[&id].kind == ActorKind::Player {
            let quiver = loot.and_then(|l| l.inventories.get(&id)).and_then(|inv| inv.grid.items.iter().find(|p| crate::archery::is_arrow(p.item.info().name)));
            let Some(p) = quiver else {
                world.gear_wear.push((id, crate::smithing_server::Wear::NoAmmo));
                continue;
            };
            let shot = crate::archery::Shot::of(&p.item);
            damage *= shot.damage_mult;
            pierce = shot.pierce;
            world.gear_wear.push((id, crate::smithing_server::Wear::Ammo(p.item.uid)));
        }
        let a = &world.actors[&id];
        let mut pose = a.pose;
        // Небольшой разброс: стрелы не летят в одну точку.
        pose.yaw += (world.rng.next_f32() - 0.5) * 0.04;
        pose.pitch += (world.rng.next_f32() - 0.5) * 0.03;
        let (pos, vel) = combat::arrow_launch(&pose);
        let arrow_id = world.next_arrow_id;
        world.next_arrow_id += 1;
        world.arrows.push(Arrow { id: arrow_id, owner: id, pos, vel, damage, life: 4.0, pierce });
    }
    let mut targets: Vec<(u64, Pose)> = world.actors.iter().filter(|(_, a)| a.alive).map(|(id, a)| (*id, a.pose)).collect();
    // Мирные люди рядом со стрелами игроков тоже могут попасть под выстрел.
    for arrow in world.arrows.iter().filter(|ar| world.actors.get(&ar.owner).is_some_and(|a| a.kind == ActorKind::Player)) {
        for (person, pose) in civil.targets_near(arrow.pos, 60.0) {
            let id = crate::civil_server::CIV_ID_BASE + person as u64;
            if !targets.iter().any(|(t, _)| *t == id) {
                targets.push((id, pose));
            }
        }
    }
    let mut arrows = std::mem::take(&mut world.arrows);
    let mut arrow_hits = Vec::new();
    arrows.retain_mut(|arrow| {
        // Свои по своим не стреляют: стрела пролетает мимо союзников.
        let owner_kind = world.actors.get(&arrow.owner).map(|a| a.kind);
        let valid: Vec<(u64, Pose)> = targets
            .iter()
            .filter(|(id, _)| {
                *id == arrow.owner
                    || (*id >= crate::civil_server::CIV_ID_BASE && owner_kind == Some(ActorKind::Player))
                    || owner_kind.is_none_or(|k| world.actors.get(id).is_some_and(|t| k.hostile(t.kind)))
            })
            .copied()
            .collect();
        match combat::step_arrow(arrow, dt, &level, &valid) {
            ArrowStep::Flying => true,
            ArrowStep::Hit { index, point, head, shield_facing } => {
                arrow_hits.push((*arrow, valid[index].0, point, head, shield_facing));
                false
            }
            ArrowStep::Stuck | ArrowStep::Expired => false,
        }
    });
    world.arrows = arrows;
    for (arrow, tid, point, head, shield_facing) in arrow_hits {
        if let Some(person) = tid.checked_sub(crate::civil_server::CIV_ID_BASE) {
            let damage = arrow.damage * if head { combat::HEAD_MULT } else { 1.0 };
            crate::civil_server::hurt(&mut server, &mut world, &mut civil, &level, trade.as_deref_mut(), person as usize, arrow.owner, damage, point, head, now);
            continue;
        }
        let damage = if is_god(&world, tid) { 0.0 } else { mitigate_pierced(&world, loot, tid, arrow.damage, head, arrow.pierce) };
        let Some(t) = world.actors.get_mut(&tid) else { continue };
        if !t.alive {
            continue;
        }
        let outcome = combat::resolve_arrow(&mut t.fighter, shield_facing, head, damage);
        let player = t.kind == ActorKind::Player;
        if player {
            use crate::smithing_server::Wear;
            match outcome {
                HitOutcome::Damage { .. } => world.gear_wear.push((tid, Wear::Armour { head, amount: 1.0 })),
                HitOutcome::Blocked { .. } => world.gear_wear.push((tid, Wear::Shield(0.7))),
                _ => {}
            }
        }
        if let HitOutcome::Damage { amount, head, .. } = outcome
            && player
            && !is_god(&world, tid)
        {
            let part = world.actors.get(&tid).map_or(crate::medicine::BodyPart::Chest, |t| crate::medicine::BodyPart::from_hit(&t.pose, point, head));
            world.hits_taken.push(crate::medicine::HitTaken { target: tid, weapon: combat::WeaponKind::Bow, amount, head, part });
        }
        server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::Hit { attacker: arrow.owner, target: tid, outcome, at: point.into() }));
        let killed = matches!(outcome, HitOutcome::Damage { killed: true, .. });
        on_player_attacks(&mut world, trade.as_deref_mut(), &mut civil, arrow.owner, tid, killed);
        if killed {
            kill(&mut server, &mut world, tid, Some(arrow.owner), now);
        }
    }

    // 4. Применяем удары.
    for strike in strikes {
        match strike {
            Strike::Wall { attacker, point } => {
                let Some(a) = world.actors.get_mut(&attacker) else { continue };
                if !matches!(a.fighter.action, combat::Action::Strike { .. }) {
                    continue;
                }
                a.fighter.action = combat::Action::Stagger { t: 0.0, dur: combat::WALL_BOUNCE };
                a.fighter.reset_combo();
                server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::Hit { attacker, target: attacker, outcome: HitOutcome::Bounced, at: point.into() }));
            }
            Strike::Civil { attacker, person, point, head } => {
                let Some(a) = world.actors.get_mut(&attacker) else { continue };
                if !matches!(a.fighter.action, combat::Action::Strike { .. }) {
                    continue;
                }
                a.fighter.hit_this_swing.push(crate::civil_server::CIV_ID_BASE + person as u64);
                let mut damage = base_damage(&world, loot, attacker);
                if head {
                    damage *= combat::HEAD_MULT;
                }
                if let Some(a) = world.actors.get(&attacker) {
                    damage *= a.fighter.strike_damage_mult();
                }
                world.gear_wear.push((attacker, crate::smithing_server::Wear::Weapon(1.0)));
                crate::civil_server::hurt(&mut server, &mut world, &mut civil, &level, trade.as_deref_mut(), person, attacker, damage, point, head, now);
            }
            Strike::Contact { attacker: aid, target: tid, contact, point } => {
                let head = matches!(contact, Contact::Body { head: true, .. });
                let damage = hit_damage(&world, loot, aid, tid, head);
                let Some(mut attacker) = world.actors.remove(&aid) else { continue };
                let still_striking = matches!(attacker.fighter.action, combat::Action::Strike { .. });
                let outcome = match world.actors.get_mut(&tid) {
                    Some(target) if still_striking && target.alive => {
                        attacker.fighter.hit_this_swing.push(tid);
                        Some(combat::resolve_contact(&mut attacker.fighter, &mut target.fighter, contact, damage))
                    }
                    _ => None,
                };
                world.actors.insert(aid, attacker);
                let Some(outcome) = outcome else { continue };
                debug!("Попадание {aid} -> {tid}: {outcome:?}");
                record_wear(&mut world, aid, tid, outcome);
                if let HitOutcome::Damage { amount, head, .. } = outcome
                    && world.actors.get(&tid).is_some_and(|t| t.kind == ActorKind::Player && !t.cheats.god)
                {
                    let weapon = world.actors.get(&aid).map_or(combat::WeaponKind::Spatha, |a| a.fighter.weapon);
                    let part = world.actors.get(&tid).map_or(crate::medicine::BodyPart::Chest, |t| crate::medicine::BodyPart::from_hit(&t.pose, point, head));
                    world.hits_taken.push(crate::medicine::HitTaken { target: tid, weapon, amount, head, part });
                }
                server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::Hit { attacker: aid, target: tid, outcome, at: point.into() }));
                let killed = matches!(outcome, HitOutcome::Damage { killed: true, .. });
                on_player_attacks(&mut world, trade.as_deref_mut(), &mut civil, aid, tid, killed);
                if killed {
                    kill(&mut server, &mut world, tid, Some(aid), now);
                }
            }
        }
    }
}

/// Удар изнашивает оружие нападавшего и доспех, щит или клинок защитника (только у игроков).
fn record_wear(world: &mut GameWorld, attacker: u64, target: u64, outcome: HitOutcome) {
    use crate::smithing_server::Wear;
    let is_player = |id: u64| world.actors.get(&id).is_some_and(|a| a.kind == ActorKind::Player);
    let (a, t) = (is_player(attacker), is_player(target));
    let mut push = |id: u64, w: Wear| world.gear_wear.push((id, w));
    match outcome {
        HitOutcome::Damage { head, .. } => {
            if a {
                push(attacker, Wear::Weapon(1.0));
            }
            if t {
                push(target, Wear::Armour { head, amount: 1.0 });
            }
        }
        HitOutcome::Parried => {
            if a {
                push(attacker, Wear::Weapon(1.2));
            }
            if t {
                push(target, Wear::Weapon(1.5));
            }
        }
        HitOutcome::Blocked { .. } | HitOutcome::GuardBroken => {
            if a {
                push(attacker, Wear::Weapon(0.8));
            }
            if t {
                push(target, Wear::Shield(1.2));
            }
        }
        HitOutcome::Chambered => {
            if a {
                push(attacker, Wear::Weapon(1.2));
            }
            if t {
                push(target, Wear::Weapon(1.2));
            }
        }
        HitOutcome::Bounced => {}
    }
}

pub(crate) fn kill(server: &mut RenetServer, world: &mut GameWorld, id: u64, killer: Option<u64>, now: f64) {
    let Some(actor) = world.actors.get_mut(&id) else { return };
    actor.alive = false;
    actor.fighter.action = combat::Action::Dead;
    let (kind, pos, yaw, mut gear) = (actor.kind, actor.pose.feet, actor.pose.yaw, actor.gear);
    // Оружие убитого остаётся при теле (в ножнах), пока его не заберут при обыске.
    gear.holstered = true;
    if kind == ActorKind::Player {
        actor.respawn_at = Some(now + PLAYER_RESPAWN_SECS);
    } else if let Some(spawn) = actor.spawn {
        world.actors.remove(&id);
        world.npc_respawns.push((now + NPC_RESPAWN_SECS, spawn));
    } else {
        // Вызванный консолью враг: тело остаётся, сам он не возвращается.
        world.actors.remove(&id);
    }
    info!("{kind:?} {id} погиб");
    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::Died { id, killer }));

    let corpse_id = world.next_corpse_id;
    world.next_corpse_id += 1;
    world.corpses.push(Corpse { id: corpse_id, actor: id, killer, kind, pos, yaw, gear, civ: None });
    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CorpseSpawned { corpse_id, kind, pos: pos.into(), yaw, gear }));
    if world.corpses.len() > MAX_CORPSES {
        let old = world.corpses.remove(0);
        server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CorpseRemoved { corpse_id: old.id }));
    }
}

/// Тело мирного человека (жителя или торговца). Возвращает id тела для обыска.
pub fn add_civ_corpse(server: &mut RenetServer, world: &mut GameWorld, killer: Option<u64>, pos: Vec3, yaw: f32, civ: CivCorpse) -> u64 {
    let corpse_id = world.next_corpse_id;
    world.next_corpse_id += 1;
    let who = civ.civ;
    world.corpses.push(Corpse { id: corpse_id, actor: 0, killer, kind: ActorKind::Player, pos, yaw, gear: Default::default(), civ: Some(civ) });
    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CivilianDead { civ: who, corpse: Some((corpse_id, pos.into(), yaw)) }));
    if world.corpses.len() > MAX_CORPSES {
        let old = world.corpses.remove(0);
        server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CorpseRemoved { corpse_id: old.id }));
    }
    corpse_id
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
                shield: a.fighter.has_shield,
                stamina: a.fighter.stamina,
                max_stamina: a.fighter.max_stamina,
                action_speed: a.fighter.action_speed,
                move_mult: a.move_mult,
                finish: a.finish,
                gear: a.gear,
            })
            .collect(),
        arrows: world.arrows.iter().map(|a| ArrowSnapshot { id: a.id, pos: a.pos.into(), vel: a.vel.into() }).collect(),
    };
    let bytes = encode(&snapshot);
    server.broadcast_message(CH_UNRELIABLE, bytes);
}
