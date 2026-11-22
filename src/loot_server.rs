//! Сервер: тела, предметы на земле и инвентари игроков. Сервер — единственный
//! источник правды: клиент только просит («переложи», «подбери»), сервер проверяет
//! расстояние и место в сумке, а потом рассылает новое состояние.
//!
//! # API для боевой системы
//!
//! Когда кто-то умирает, вызовите на сервере:
//!
//! ```ignore
//! commands.trigger(CharacterDied {
//!     pos, yaw,
//!     name: "Рыцарь".into(),
//!     loot: DeathLoot::Table { table: LootTableId::Knight, luck: 0.0 },
//! });
//! // для игрока: loot: DeathLoot::Player(client_id) — всё его снаряжение переходит в тело.
//! ```
//!
//! Выбросить предмет на землю: `commands.trigger(SpawnGroundItem { pos, vel, item })`.
//! Характеристики надетого на игроке (урон оружия, броня, окно парирования):
//! `Res<LootWorld>::equipped_stats(client_id)`.

use crate::collision::LevelColliders;
use crate::inventory::{CORPSE_GRID, Inventory, MoveError, PLAYER_GRID, Target, transfer};
use crate::items::*;
use crate::loot_net::*;
use crate::protocol::{decode, encode};
use crate::protocol::ActorKind;
use crate::server::GameWorld;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::{HashMap, HashSet};

/// Сколько тел храним одновременно (старые исчезают).
const MAX_CORPSES: usize = 64;

pub struct LootServerPlugin;

impl Plugin for LootServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LootWorld::default())
            .add_plugins(crate::consumables::ConsumablesPlugin)
            .add_observer(on_connection)
            .add_observer(on_character_died)
            .add_observer(on_spawn_ground_item)
            .add_systems(Startup, spawn_demo_loot)
            .add_systems(Update, (sync_combat_corpses, receive_loot_messages, close_far_containers).chain());
    }
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

/// Кто-то умер: на его месте остаётся тело, которое можно обыскать.
#[derive(Event, Clone, Debug)]
pub struct CharacterDied {
    /// Точка у ног.
    pub pos: Vec3,
    pub yaw: f32,
    /// Подпись при обыске: «Рыцарь», «Тело игрока».
    pub name: String,
    pub loot: DeathLoot,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // Player и Items понадобятся боевой системе
pub enum DeathLoot {
    /// Бросить таблицу добычи (для врагов). `luck` — удача убийцы.
    Table { table: LootTableId, luck: f32 },
    /// Умер игрок: всё его снаряжение переходит в тело, сам он остаётся ни с чем.
    Player(u64),
    /// Готовый список вещей.
    Items(Vec<Item>),
}

/// Положить предмет на землю (с полётом из `pos` со скоростью `vel`).
#[derive(Event, Clone, Debug)]
pub struct SpawnGroundItem {
    pub pos: Vec3,
    pub vel: Vec3,
    pub item: Item,
}

// ---------------------------------------------------------------------------
// Состояние
// ---------------------------------------------------------------------------

pub struct Corpse {
    pub pos: Vec3,
    pub yaw: f32,
    pub name: String,
    pub inv: Inventory,
    created: u64,
    /// Модель рисуем мы (а не боевая система).
    own_model: bool,
}

pub struct GroundItem {
    pub pos: Vec3,
    pub start: Vec3,
    pub vel: Vec3,
    pub item: Item,
}

#[derive(Resource)]
pub struct LootWorld {
    next_id: LootId,
    pub corpses: HashMap<LootId, Corpse>,
    pub ground: HashMap<LootId, GroundItem>,
    pub inventories: HashMap<u64, Inventory>,
    /// Какое тело открыто у игрока.
    open: HashMap<u64, LootId>,
    /// Тела боевой системы, для которых добыча уже создана.
    linked: HashSet<u64>,
    pub rng: Rng,
    pub uids: UidGen,
}

impl Default for LootWorld {
    fn default() -> Self {
        Self {
            // Свои номера — в отдельном диапазоне, чтобы не совпасть с телами боевой системы.
            next_id: 1 << 40,
            linked: HashSet::new(),
            corpses: HashMap::new(),
            ground: HashMap::new(),
            inventories: HashMap::new(),
            open: HashMap::new(),
            rng: Rng::from_time(),
            uids: UidGen::default(),
        }
    }
}

impl LootWorld {
    fn new_id(&mut self) -> LootId {
        self.next_id += 1;
        self.next_id
    }

    /// Характеристики надетых вещей игрока.
    #[allow(dead_code)] // для боевой системы
    pub fn equipped_stats(&self, player: u64) -> StatTotals {
        self.inventories.get(&player).map(Inventory::equipped_stats).unwrap_or_default()
    }

    /// Бросить таблицу добычи.
    pub fn roll(&mut self, table: LootTableId, luck: f32) -> Vec<Item> {
        roll_loot(table, luck, &mut self.rng, &mut self.uids)
    }

    fn make(&mut self, name: &str, quality: Quality, count: u16) -> Item {
        roll_item(ItemBaseId::by_name(name), quality, count, &mut self.rng, &mut self.uids)
    }

    fn starter_inventory(&mut self) -> Inventory {
        let mut inv = Inventory::new(PLAYER_GRID);
        let kit = vec![
            self.make("Спата", Quality::Common, 1),
            self.make("Стёганая туника", Quality::Common, 1),
            self.make("Кожаные сапоги", Quality::Poor, 1),
            self.make("Бинт", Quality::Common, 3),
            self.make("Золотые монеты", Quality::Common, 10),
        ];
        inv.fill(kit, true);
        inv
    }

    /// Тело со своей моделью (демо, отладка, `CharacterDied`).
    fn spawn_corpse(&mut self, server: &mut RenetServer, level: &LevelColliders, pos: Vec3, yaw: f32, name: String, inv: Inventory) -> LootId {
        let own: Vec<(LootId, u64)> = self.corpses.iter().filter(|(_, c)| c.own_model).map(|(id, c)| (*id, c.created)).collect();
        if own.len() >= MAX_CORPSES
            && let Some((oldest, _)) = own.iter().min_by_key(|(_, created)| *created)
        {
            self.corpses.remove(oldest);
            server.broadcast_message(CH_LOOT, encode(&LootServerMessage::Removed { id: *oldest }));
        }
        // Тело ложится на пол, даже если человек умер в прыжке.
        let pos = simulate_drop(level, pos + Vec3::Y * 0.2, Vec3::ZERO);
        let id = self.new_id();
        self.insert_corpse(server, id, pos, yaw, name, inv, true);
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_corpse(&mut self, server: &mut RenetServer, id: LootId, pos: Vec3, yaw: f32, name: String, inv: Inventory, own_model: bool) {
        let msg = LootServerMessage::CorpseSpawned { id, pos: pos.into(), yaw, name: name.clone(), empty: inv.is_empty(), own_model };
        server.broadcast_message(CH_LOOT, encode(&msg));
        self.corpses.insert(id, Corpse { pos, yaw, name, inv, created: self.next_id, own_model });
    }

    fn spawn_ground(&mut self, server: &mut RenetServer, level: &LevelColliders, start: Vec3, vel: Vec3, item: Item) -> LootId {
        let rest = simulate_drop(level, start, vel);
        let id = self.new_id();
        let ground = GroundItem { pos: rest, start, vel, item };
        server.broadcast_message(CH_LOOT, encode(&ground_message(id, &ground)));
        self.ground.insert(id, ground);
        id
    }

    /// Вынимает инвентарь на время операции (вернуть через `put_inv`).
    fn take_inv(&mut self, player: u64, owner: Owner) -> Option<Inventory> {
        match owner {
            Owner::Me => self.inventories.remove(&player),
            Owner::Container(id) => self.corpses.get_mut(&id).map(|c| std::mem::replace(&mut c.inv, Inventory::new((0, 0)))),
        }
    }

    fn put_inv(&mut self, player: u64, owner: Owner, inv: Inventory) {
        match owner {
            Owner::Me => {
                self.inventories.insert(player, inv);
            }
            Owner::Container(id) => {
                if let Some(c) = self.corpses.get_mut(&id) {
                    c.inv = inv;
                }
            }
        }
    }

    /// Разослать изменения: свой инвентарь и тело всем, у кого оно открыто.
    fn sync(&self, server: &mut RenetServer, player: u64, owners: &[Owner], was_empty: &[(LootId, bool)]) {
        for owner in owners {
            match owner {
                Owner::Me => {
                    if let Some(inv) = self.inventories.get(&player) {
                        server.send_message(player, CH_LOOT, encode(&LootServerMessage::Inventory(inv.clone())));
                    }
                }
                Owner::Container(id) => {
                    let Some(c) = self.corpses.get(id) else { continue };
                    let msg = encode(&LootServerMessage::ContainerUpdated { id: *id, inv: c.inv.clone() });
                    for (viewer, open) in &self.open {
                        if open == id {
                            server.send_message(*viewer, CH_LOOT, msg.clone());
                        }
                    }
                    if let Some((_, before)) = was_empty.iter().find(|(i, _)| i == id)
                        && *before != c.inv.is_empty()
                    {
                        server.broadcast_message(CH_LOOT, encode(&LootServerMessage::CorpseEmpty { id: *id, empty: c.inv.is_empty() }));
                    }
                }
            }
        }
    }
}

fn ground_message(id: LootId, g: &GroundItem) -> LootServerMessage {
    LootServerMessage::GroundSpawned {
        id,
        start: g.start.into(),
        vel: g.vel.into(),
        rest: g.pos.into(),
        base: g.item.base,
        quality: g.item.quality,
        count: g.item.count,
    }
}

fn notice(server: &mut RenetServer, player: u64, text: &str) {
    server.send_message(player, CH_LOOT, encode(&LootServerMessage::Notice(text.to_string())));
}

/// Можно ли дотянуться от ног игрока до точки.
fn in_reach(player_feet: Vec3, target: Vec3) -> bool {
    let d = target - player_feet;
    Vec2::new(d.x, d.z).length() <= INTERACT_RANGE + 0.5 && d.y.abs() < 2.5
}

fn forward(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Vec3::NEG_Z
}

// ---------------------------------------------------------------------------
// Системы
// ---------------------------------------------------------------------------

fn on_connection(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, mut world: ResMut<LootWorld>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let id = *client_id;
            let inv = world.starter_inventory();
            server.send_message(id, CH_LOOT, encode(&LootServerMessage::Inventory(inv.clone())));
            world.inventories.insert(id, inv);
            for (cid, c) in &world.corpses {
                let msg = LootServerMessage::CorpseSpawned { id: *cid, pos: c.pos.into(), yaw: c.yaw, name: c.name.clone(), empty: c.inv.is_empty(), own_model: c.own_model };
                server.send_message(id, CH_LOOT, encode(&msg));
            }
            for (gid, g) in &world.ground {
                server.send_message(id, CH_LOOT, encode(&ground_message(*gid, g)));
            }
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            world.inventories.remove(client_id);
            world.open.remove(client_id);
        }
    }
}

fn on_character_died(event: On<CharacterDied>, mut server: ResMut<RenetServer>, level: Res<LevelColliders>, mut world: ResMut<LootWorld>) {
    let ev = event.event().clone();
    let inv = match ev.loot {
        DeathLoot::Table { table, luck } => {
            let items = world.roll(table, luck);
            let mut inv = Inventory::new(CORPSE_GRID);
            // Враг «носит» то, что можно надеть, — как в настоящем бою.
            inv.fill(items, true);
            inv
        }
        DeathLoot::Items(items) => {
            let mut inv = Inventory::new(CORPSE_GRID);
            inv.fill(items, false);
            inv
        }
        DeathLoot::Player(player) => {
            let inv = world.inventories.insert(player, Inventory::new(PLAYER_GRID)).unwrap_or_else(|| Inventory::new(PLAYER_GRID));
            if let Some(fresh) = world.inventories.get(&player) {
                server.send_message(player, CH_LOOT, encode(&LootServerMessage::Inventory(fresh.clone())));
            }
            if world.open.remove(&player).is_some() {
                server.send_message(player, CH_LOOT, encode(&LootServerMessage::ContainerClosed));
            }
            inv
        }
    };
    world.spawn_corpse(&mut server, &level, ev.pos, ev.yaw, ev.name, inv);
}

fn on_spawn_ground_item(event: On<SpawnGroundItem>, mut server: ResMut<RenetServer>, level: Res<LevelColliders>, mut world: ResMut<LootWorld>) {
    let ev = event.event().clone();
    world.spawn_ground(&mut server, &level, ev.pos, ev.vel, ev.item);
}

/// Одно тело и немного вещей во дворе у точки появления, чтобы сразу было что обыскать.
fn spawn_demo_loot(mut commands: Commands) {
    commands.trigger(CharacterDied {
        pos: crate::level::spawn_point(0) + Vec3::new(5.5, 0.4, -5.5),
        yaw: 0.6,
        name: "Павший рыцарь".into(),
        loot: DeathLoot::Table { table: LootTableId::Knight, luck: 0.0 },
    });
    let mut rng = Rng::from_time();
    let mut uids = UidGen(1_000_000_000);
    for (i, item) in roll_loot(LootTableId::Chest, 30.0, &mut rng, &mut uids).into_iter().enumerate() {
        let a = i as f32 * 1.3;
        commands.trigger(SpawnGroundItem { pos: crate::level::spawn_point(0) + Vec3::new(8.0, 1.1, -3.0), vel: Vec3::new(a.cos() * 2.0, 2.0, a.sin() * 2.0), item });
    }
}

fn receive_loot_messages(
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    level: Res<LevelColliders>,
    mut world: ResMut<LootWorld>,
    mut healing: ResMut<crate::consumables::Healing>,
) {
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_LOOT) {
            let Some(msg) = decode::<LootClientMessage>(&bytes) else { continue };
            // Мёртвые не обыскивают.
            let Some(actor) = game.actors.get(&pid).filter(|a| a.alive) else { continue };
            let (feet, yaw, pitch) = (actor.pose.feet, actor.pose.yaw, actor.pose.pitch);
            if !world.inventories.contains_key(&pid) {
                continue;
            }
            match msg {
                LootClientMessage::Interact { id } => {
                    if let Some(c) = world.corpses.get(&id) {
                        if !in_reach(feet, c.pos) {
                            notice(&mut server, pid, "Слишком далеко");
                            continue;
                        }
                        let msg = LootServerMessage::ContainerOpened { id, title: c.name.clone(), inv: c.inv.clone() };
                        server.send_message(pid, CH_LOOT, encode(&msg));
                        world.open.insert(pid, id);
                    } else if let Some(g) = world.ground.get(&id) {
                        if !in_reach(feet, g.pos) {
                            notice(&mut server, pid, "Слишком далеко");
                            continue;
                        }
                        let item = g.item.clone();
                        let before = item.count;
                        let inv = world.inventories.get_mut(&pid).unwrap();
                        match inv.place(item, Target::Auto) {
                            Ok(_) => {
                                world.ground.remove(&id);
                                server.broadcast_message(CH_LOOT, encode(&LootServerMessage::Removed { id }));
                            }
                            Err(rest) => {
                                if rest.count != before {
                                    // Часть стопки влезла — на земле остаётся меньше.
                                    let g = world.ground.get_mut(&id).unwrap();
                                    g.item.count = rest.count;
                                    g.start = g.pos;
                                    g.vel = Vec3::ZERO;
                                    let msg = ground_message(id, g);
                                    server.broadcast_message(CH_LOOT, encode(&LootServerMessage::Removed { id }));
                                    server.broadcast_message(CH_LOOT, encode(&msg));
                                }
                                notice(&mut server, pid, MoveError::NoSpace.message());
                            }
                        }
                        world.sync(&mut server, pid, &[Owner::Me], &[]);
                    }
                }
                LootClientMessage::Move { uid, from, to, target } => {
                    if !owners_allowed(&world, pid, feet, &[from, to]) {
                        continue;
                    }
                    let was_empty = empty_flags(&world, &[from, to]);
                    let src_inv = world.take_inv(pid, from);
                    let Some(src_inv) = src_inv else { continue };
                    let result = if from == to {
                        let mut invs = [src_inv];
                        let r = transfer(&mut invs, 0, 0, uid, target);
                        let [a] = invs;
                        world.put_inv(pid, from, a);
                        r
                    } else {
                        let Some(dst_inv) = world.take_inv(pid, to) else {
                            world.put_inv(pid, from, src_inv);
                            continue;
                        };
                        let mut invs = [src_inv, dst_inv];
                        let r = transfer(&mut invs, 0, 1, uid, target);
                        let [a, b] = invs;
                        world.put_inv(pid, from, a);
                        world.put_inv(pid, to, b);
                        r
                    };
                    if let Err(e) = result {
                        notice(&mut server, pid, e.message());
                    }
                    // Даже при ошибке шлём состояние: клиент вернёт предмет на место.
                    let owners: Vec<Owner> = if from == to { vec![from] } else { vec![from, to] };
                    world.sync(&mut server, pid, &owners, &was_empty);
                }
                LootClientMessage::Drop { uid, from } => {
                    if !owners_allowed(&world, pid, feet, &[from]) {
                        continue;
                    }
                    let was_empty = empty_flags(&world, &[from]);
                    let Some(mut inv) = world.take_inv(pid, from) else { continue };
                    let taken = inv.take(uid);
                    world.put_inv(pid, from, inv);
                    let Some((item, _)) = taken else { continue };
                    let dir = forward(yaw, pitch);
                    let mut start = feet + Vec3::Y * 1.3 + Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero() * 0.35;
                    if !level.body_is_free(&DROP_BODY, start) {
                        start = feet + Vec3::Y * 1.0;
                    }
                    let vel = dir * 3.5 + Vec3::Y * 1.5;
                    world.spawn_ground(&mut server, &level, start, vel, item);
                    world.sync(&mut server, pid, &[from], &was_empty);
                }
                LootClientMessage::Close => {
                    world.open.remove(&pid);
                }
                LootClientMessage::Use { uid } => healing.requests.push((pid, Some(uid))),
                LootClientMessage::CancelUse => healing.requests.push((pid, None)),
                LootClientMessage::DebugSpawnCorpse => {
                    let dir = forward(yaw, 0.0);
                    let table = [LootTableId::Footman, LootTableId::Knight, LootTableId::KnightCaptain][world.rng.range_u32(0, 2) as usize];
                    let name = match table {
                        LootTableId::Footman => "Пехотинец",
                        LootTableId::KnightCaptain => "Капитан стражи",
                        _ => "Рыцарь",
                    };
                    let items = world.roll(table, 0.0);
                    let mut inv = Inventory::new(CORPSE_GRID);
                    inv.fill(items, true);
                    let yaw = world.rng.range_f32(-3.1, 3.1);
                    world.spawn_corpse(&mut server, &level, feet + dir * 1.8 + Vec3::Y * 0.5, yaw, name.into(), inv);
                }
            }
        }
    }
}

/// Игрок может трогать только свой инвентарь и тело, которое открыл (и до которого дотягивается).
fn owners_allowed(world: &LootWorld, pid: u64, feet: Vec3, owners: &[Owner]) -> bool {
    owners.iter().all(|o| match o {
        Owner::Me => true,
        Owner::Container(id) => world.open.get(&pid) == Some(id) && world.corpses.get(id).is_some_and(|c| in_reach(feet, c.pos)),
    })
}

fn empty_flags(world: &LootWorld, owners: &[Owner]) -> Vec<(LootId, bool)> {
    owners
        .iter()
        .filter_map(|o| match o {
            Owner::Container(id) => world.corpses.get(id).map(|c| (*id, c.inv.is_empty())),
            Owner::Me => None,
        })
        .collect()
}

/// Закрывает окно обыска, если игрок отошёл или тело исчезло.
fn close_far_containers(mut server: ResMut<RenetServer>, game: Res<GameWorld>, mut world: ResMut<LootWorld>) {
    let far: Vec<u64> = world
        .open
        .iter()
        .filter(|(pid, id)| {
            let corpse = world.corpses.get(id);
            let player = game.actors.get(pid).filter(|a| a.alive);
            match (corpse, player) {
                (Some(c), Some(p)) => !in_reach(p.pose.feet, c.pos),
                _ => true,
            }
        })
        .map(|(pid, _)| *pid)
        .collect();
    for pid in far {
        world.open.remove(&pid);
        server.send_message(pid, CH_LOOT, encode(&LootServerMessage::ContainerClosed));
    }
}

/// Добыча для тел, которые создаёт боевая система (`GameWorld.corpses`):
/// враг получает вещи по своей таблице, погибший игрок оставляет в теле всё снаряжение.
fn sync_combat_corpses(mut server: ResMut<RenetServer>, game: Res<GameWorld>, mut world: ResMut<LootWorld>) {
    for c in &game.corpses {
        if world.linked.contains(&c.id) {
            continue;
        }
        world.linked.insert(c.id);
        let (name, inv) = match c.kind {
            ActorKind::Player => {
                let fresh = world.starter_inventory();
                let old = world.inventories.insert(c.actor, fresh.clone()).unwrap_or_else(|| Inventory::new(PLAYER_GRID));
                server.send_message(c.actor, CH_LOOT, encode(&LootServerMessage::Inventory(fresh)));
                if world.open.remove(&c.actor).is_some() {
                    server.send_message(c.actor, CH_LOOT, encode(&LootServerMessage::ContainerClosed));
                }
                ("Тело игрока", old)
            }
            kind => {
                let (name, table) = match kind {
                    ActorKind::HeavyKnight => ("Тяжёлый рыцарь", LootTableId::KnightCaptain),
                    _ => ("Рыцарь", LootTableId::Knight),
                };
                let luck = c.killer.map(|k| world.equipped_stats(k).get(Stat::Luck)).unwrap_or(0.0);
                let items = world.roll(table, luck);
                let mut inv = Inventory::new(CORPSE_GRID);
                inv.fill(items, true);
                (name, inv)
            }
        };
        world.insert_corpse(&mut server, c.id, c.pos, c.yaw, name.into(), inv, false);
    }
    // Боевая система убрала старое тело — убираем и добычу.
    let alive: HashSet<u64> = game.corpses.iter().map(|c| c.id).collect();
    let gone: Vec<u64> = world.linked.iter().copied().filter(|id| !alive.contains(id)).collect();
    for id in gone {
        world.linked.remove(&id);
        if world.corpses.remove(&id).is_some() {
            server.broadcast_message(CH_LOOT, encode(&LootServerMessage::Removed { id }));
        }
    }
}
