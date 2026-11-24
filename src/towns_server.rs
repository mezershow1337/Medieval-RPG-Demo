//! Сервер городов и домов: каждую игровую неделю живут поселения (`towns.rs`), курия продаёт
//! и сдаёт усадьбы, мастера обставляют комнаты, стройки растут на глазах, по степи приходят
//! налётчики. Состояние мира и хозяева домов сохраняются на диск хоста (JSON в папке данных игры).
//!
//! # Связи с другими системами
//!
//! - Торговцы: каждую неделю `MerchantState::market/supply` получают цены рынка своего поселения.
//! - Двери (`doors.rs`) и сундуки (`loot_server.rs`) спрашивают [`Locks`]: чужой дом заперт.
//! - Набег: налётчики ставятся через `GameWorld::spawn_raider` у селения и уходят через три минуты.

use crate::collision::LevelColliders;
use crate::economy::{format_money, give_money, take_money};
use crate::estates::*;
use crate::inventory::Placed;
use crate::items::ItemBaseId;
use crate::level::{FurnishKind, SwitchKey, estates};
use crate::loot_net::{CH_LOOT, CONTAINER_ID_BASE, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::protocol::{ActorKind, decode, encode};
use crate::server::GameWorld;
use crate::towns::*;
use crate::trade::Category;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Сколько секунд налётчики хозяйничают в селении, прежде чем уйти в степь с добычей.
const RAID_SECS: f64 = 180.0;
/// С какого расстояния можно договориться о доме (от входа или изнутри).
const DEAL_RANGE: f32 = 9.0;
/// Как часто сохранять мир (если что-то поменялось), секунды.
const SAVE_EVERY: f64 = 30.0;

// ---------------------------------------------------------------------------
// Состояние
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct EstateState {
    /// Постоянный ключ хозяина (или съёмщика).
    pub owner: Option<u64>,
    pub owner_name: String,
    /// Снят до этой недели.
    pub rented_until: Option<u32>,
    /// Кого хозяин пускает: (ключ, имя).
    pub guests: Vec<(u64, String)>,
    pub furnish: Vec<Option<FurnishKind>>,
    /// Заказы мастерам: (комната, что, к какой неделе).
    pub orders: Vec<(u8, FurnishKind, u32)>,
    #[serde(default)]
    warned: bool,
}

/// Кто куда может войти: спрашивают двери и сундуки.
#[derive(Resource, Default)]
pub struct Locks {
    door_estate: HashMap<u32, u16>,
    container_estate: HashMap<u32, u16>,
    /// У дома есть хозяин: пускать только этих (номера подключений).
    allowed: HashMap<u16, HashSet<u64>>,
    /// Двери недостроенных домов: открыты и не закрываются.
    frozen: HashSet<u32>,
    /// Кому отказали за кадр (сообщит `towns_server`).
    pub denied: Vec<(u64, String)>,
}

impl Locks {
    /// Можно ли игроку открыть или закрыть дверь `door`.
    pub fn door(&mut self, door: u32, player: u64) -> bool {
        if self.frozen.contains(&door) {
            self.denied
                .push((player, "Дом ещё строится: двери не навешены".into()));
            return false;
        }
        let Some(e) = self.door_estate.get(&door) else {
            return true;
        };
        match self.allowed.get(e) {
            Some(who) if !who.contains(&player) => {
                self.denied.push((player, "Заперто: это чужой дом".into()));
                false
            }
            _ => true,
        }
    }

    /// Сундук стоит в доме этого игрока (купленном, снятом или где он гость).
    pub fn own_container(&self, container: u32, player: u64) -> bool {
        self.container_estate
            .get(&container)
            .and_then(|e| self.allowed.get(e))
            .is_some_and(|who| who.contains(&player))
    }

    /// Можно ли игроку открыть сундук уровня номер `container`.
    pub fn container(&mut self, container: u32, player: u64) -> bool {
        let Some(e) = self.container_estate.get(&container) else {
            return true;
        };
        match self.allowed.get(e) {
            Some(who) if !who.contains(&player) => {
                self.denied
                    .push((player, "Сундук заперт на замок: это чужой дом".into()));
                false
            }
            _ => true,
        }
    }
}

/// Сохранённая вещь: по названию, чтобы пережить перестановки в таблице предметов.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SavedPlaced {
    name: String,
    placed: Placed,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SaveFile {
    version: u32,
    towns: Towns,
    estates: Vec<EstateState>,
    /// Содержимое сундуков в усадьбах: (номер сундука уровня, вещи).
    storage: Vec<(u32, Vec<SavedPlaced>)>,
}

#[derive(Resource)]
pub struct TownWorld {
    pub towns: Towns,
    pub estates: Vec<EstateState>,
    players: HashMap<u64, Profile>,
    next_week_at: f64,
    week_secs: f64,
    dirty: bool,
    last_save: f64,
    raids: Vec<(f64, Vec<u64>, TownId)>,
    /// Загруженное с диска, ещё не разложенное по сундукам.
    pending_storage: Vec<(u32, Vec<SavedPlaced>)>,
    save_path: Option<std::path::PathBuf>,
    /// Что разослать: изменившиеся усадьбы.
    changed: HashSet<u16>,
}

fn save_path() -> Option<std::path::PathBuf> {
    match std::env::var("MRPG_SAVE").ok().as_deref() {
        Some("off") | Some("0") => None,
        Some(p) if !p.is_empty() => Some(p.into()),
        _ => Some(data_dir().join("herson-world.json")),
    }
}

impl TownWorld {
    pub fn new_or_load() -> Self {
        let cat = estates();
        let path = save_path();
        let mut loaded: Option<SaveFile> = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok());
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(9);
        let mut towns = loaded
            .as_ref()
            .map(|s| s.towns.clone())
            .unwrap_or_else(|| Towns::new(cat.sites, seed));
        towns.fit_sites(cat.sites);
        towns.reseed(seed);
        let mut list: Vec<EstateState> = loaded
            .as_mut()
            .map(|s| std::mem::take(&mut s.estates))
            .unwrap_or_default();
        list.resize(cat.estates.len(), EstateState::default());
        for (e, spec) in list.iter_mut().zip(&cat.estates) {
            e.furnish.resize(spec.rooms.len(), None);
            e.orders.retain(|(r, k, _)| {
                spec.rooms
                    .get(*r as usize)
                    .is_some_and(|room| room.kinds.contains(k))
            });
            for (r, f) in e.furnish.iter_mut().enumerate() {
                if f.is_some_and(|k| !spec.rooms[r].kinds.contains(&k)) {
                    *f = None;
                }
            }
        }
        if loaded.is_some() {
            info!("Мир Херсона загружен: {}", date_text(towns.week));
        }
        TownWorld {
            towns,
            estates: list,
            players: HashMap::new(),
            next_week_at: 0.0,
            week_secs: week_secs(),
            dirty: false,
            last_save: 0.0,
            raids: Vec::new(),
            pending_storage: loaded.map(|s| s.storage).unwrap_or_default(),
            save_path: path,
            changed: HashSet::new(),
        }
    }

    fn status(&self, id: u16) -> Status {
        let spec = &estates().estates[id as usize];
        if let Some(s) = spec.site
            && self.towns.sites.get(s as usize).copied().unwrap_or(0) < SITE_DONE
        {
            return Status::Building;
        }
        let e = &self.estates[id as usize];
        match (e.owner, e.rented_until) {
            (Some(_), None) => Status::Owned,
            (Some(_), Some(until)) => Status::Rented { until },
            _ => Status::ForSale,
        }
    }

    fn price(&self, id: u16) -> u32 {
        let spec = &estates().estates[id as usize];
        estate_price(spec, self.towns.town(spec.town))
    }

    /// Ключ игрока в сети.
    fn key(&self, player: u64) -> Option<u64> {
        self.players.get(&player).map(|p| p.key)
    }

    fn is_owner(&self, id: u16, player: u64) -> bool {
        let e = &self.estates[id as usize];
        e.owner.is_some() && e.owner == self.key(player)
    }

    /// Кто из игроков в сети может входить.
    fn access(&self, id: u16) -> Vec<u64> {
        let e = &self.estates[id as usize];
        let Some(owner) = e.owner else {
            return Vec::new();
        };
        let mut v: Vec<u64> = self
            .players
            .iter()
            .filter(|(_, p)| p.key == owner || e.guests.iter().any(|(k, _)| *k == p.key))
            .map(|(c, _)| *c)
            .collect();
        v.sort();
        v
    }

    fn brief(&self, id: u16) -> EstateBrief {
        let e = &self.estates[id as usize];
        let price = self.price(id);
        EstateBrief {
            id,
            status: self.status(id),
            owner: e.owner_name.clone(),
            access: self.access(id),
            price,
            rent: rent_price(price),
            furnish: e.furnish.clone(),
        }
    }

    fn detail(&self, id: u16, player: u64) -> EstateDetail {
        let spec = &estates().estates[id as usize];
        let e = &self.estates[id as usize];
        let brief = self.brief(id);
        let mine = self.is_owner(id, player);
        let week = self.towns.week;
        let rooms = spec
            .rooms
            .iter()
            .enumerate()
            .map(|(r, room)| RoomView {
                now: e.furnish[r],
                kinds: room.kinds.clone(),
                order: e
                    .orders
                    .iter()
                    .find(|o| o.0 as usize == r)
                    .map(|o| (o.1, o.2.saturating_sub(week).max(1))),
                upper: room.min.y > spec.min.y + 1.5,
            })
            .collect();
        let players = if mine {
            let mut v: Vec<(u64, String, bool)> = self
                .players
                .iter()
                .filter(|(c, p)| **c != player && Some(p.key) != e.owner)
                .map(|(c, p)| {
                    (
                        *c,
                        p.name.clone(),
                        e.guests.iter().any(|(k, _)| *k == p.key),
                    )
                })
                .collect();
            v.sort_by(|a, b| a.1.cmp(&b.1));
            v
        } else {
            Vec::new()
        };
        EstateDetail {
            buyback: buyback_price(brief.price),
            weeks_left: e.rented_until.map(|u| u.saturating_sub(week)),
            brief,
            name: spec.name.clone(),
            town: spec.town,
            rooms,
            chests: spec.containers.len() as u32,
            mine,
            guests: e.guests.iter().map(|(_, n)| n.clone()).collect(),
            players,
            date: date_text(week),
        }
    }

    fn news_view(&self) -> NewsView {
        let week = self.towns.week;
        let herson = self.towns.town(TownId::Herson);
        let market = [
            (Category::Food, "Хлеб, рыба, соль"),
            (Category::WineOil, "Вино и масло"),
            (Category::Textile, "Ткани"),
            (Category::Hide, "Шкуры"),
            (Category::Weapon, "Оружие"),
            (Category::Material, "Лес, железо, известь"),
            (Category::Import, "Заморские пряности и смолы"),
            (Category::Remedy, "Снадобья"),
            (Category::Herb, "Травы"),
        ]
        .iter()
        .map(|(c, n)| (n.to_string(), market_mult(herson, *c)))
        .collect();
        let cat = estates();
        let est = (0..cat.estates.len() as u16)
            .map(|i| {
                let price = self.price(i);
                (
                    i,
                    cat.estates[i as usize].name.clone(),
                    price,
                    rent_price(price),
                    self.status(i),
                    self.estates[i as usize].owner_name.clone(),
                )
            })
            .collect();
        NewsView {
            date: date_text(week),
            month: month_name(week).to_string(),
            sea_open: sea_open(week),
            towns: self.towns.towns.iter().map(town_view).collect(),
            news: self
                .towns
                .latest(30)
                .into_iter()
                .map(|n| (date_text(n.week), n.town, n.text.clone()))
                .collect(),
            market,
            estates: est,
            sites: self.towns.sites.clone(),
        }
    }

    /// Освободить дом (продан курии, кончилась аренда).
    fn release(&mut self, id: u16) {
        let e = &mut self.estates[id as usize];
        e.owner = None;
        e.owner_name.clear();
        e.rented_until = None;
        e.guests.clear();
        e.orders.clear();
        e.warned = false;
        self.changed.insert(id);
        self.dirty = true;
    }
}

// ---------------------------------------------------------------------------
// Плагин
// ---------------------------------------------------------------------------

pub struct TownServerPlugin;

impl Plugin for TownServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TownWorld::new_or_load())
            .insert_resource(Locks::default())
            .add_observer(on_connection)
            .add_systems(PostStartup, setup_world)
            .add_systems(
                Update,
                (
                    receive_town_messages,
                    tick_towns,
                    end_raids,
                    send_changes,
                    tell_denied,
                    save_world,
                )
                    .chain(),
            );
    }
}

fn send(server: &mut RenetServer, player: u64, msg: &TownServerMessage) {
    server.send_message(player, CH_TOWN, encode(msg));
}

fn notice(server: &mut RenetServer, player: u64, text: impl Into<String>) {
    send(server, player, &TownServerMessage::Notice(text.into()));
}

/// Включить нужные группы, заморозить двери строек, сундуки усадеб сделать личными, разложить сохранённые вещи.
/// Для снимков: `MRPG_SITE_STAGES=0,1,2,3` — стройки стоят на заданных стадиях и не двигаются.
fn pinned_sites() -> Option<Vec<u8>> {
    let v = std::env::var("MRPG_SITE_STAGES").ok()?;
    Some(
        v.split(',')
            .filter_map(|s| s.trim().parse::<u8>().ok())
            .map(|s| s.min(crate::towns::SITE_DONE))
            .collect(),
    )
}

fn pin_sites(towns: &mut Towns) -> bool {
    let Some(pin) = pinned_sites() else {
        return false;
    };
    for (site, st) in towns.sites.iter_mut().zip(pin) {
        *site = st;
    }
    true
}

fn setup_world(
    mut tw: ResMut<TownWorld>,
    mut locks: ResMut<Locks>,
    level: Res<LevelColliders>,
    mut loot: ResMut<LootWorld>,
) {
    let cat = estates();
    for (i, spec) in cat.estates.iter().enumerate() {
        for d in &spec.doors {
            locks.door_estate.insert(*d, i as u16);
        }
        for c in &spec.containers {
            locks.container_estate.insert(*c, i as u16);
            let id = CONTAINER_ID_BASE + *c as u64;
            loot.make_private(id);
        }
    }
    let pending = std::mem::take(&mut tw.pending_storage);
    let known: HashSet<u32> = locks.container_estate.keys().copied().collect();
    for (c, items) in pending {
        if !known.contains(&c) {
            continue;
        }
        let id = CONTAINER_ID_BASE + c as u64;
        let mut placed = Vec::new();
        for s in items {
            let Some(base) = ItemBaseId::find(&s.name) else {
                continue;
            };
            let mut p = s.placed;
            p.item.base = base;
            p.item.uid = loot.uids.next();
            placed.push(p);
        }
        if let Some(inv) = loot.container_mut(id) {
            inv.grid.items = placed;
        }
    }
    pin_sites(&mut tw.towns);
    apply_switches(&tw, &level, &mut locks);
    for i in 0..cat.estates.len() {
        tw.changed.insert(i as u16);
    }
    refresh_access(&tw, &mut locks);
}

/// Стадии строек и обстановка — в коллизии; двери недостроенных домов — настежь.
fn apply_switches(tw: &TownWorld, level: &LevelColliders, locks: &mut Locks) {
    let cat = estates();
    for (g, key) in cat.keys.iter().enumerate() {
        let on = match *key {
            SwitchKey::Site { site, stage } => {
                tw.towns.sites.get(site as usize).copied().unwrap_or(0) == stage
            }
            SwitchKey::Furnish { estate, room, kind } => {
                tw.estates
                    .get(estate as usize)
                    .and_then(|e| e.furnish.get(room as usize))
                    .copied()
                    .flatten()
                    == Some(kind)
            }
        };
        level.set_switch(g, on);
    }
    locks.frozen.clear();
    for (s, doors) in cat.site_doors.iter().enumerate() {
        let built = tw.towns.sites.get(s).copied().unwrap_or(0) >= SITE_DONE;
        for d in doors {
            if !built {
                locks.frozen.insert(*d);
                level.set_door_open(*d as usize, true);
            }
        }
    }
}

fn refresh_access(tw: &TownWorld, locks: &mut Locks) {
    locks.allowed.clear();
    for i in 0..tw.estates.len() {
        if tw.estates[i].owner.is_some() {
            locks
                .allowed
                .insert(i as u16, tw.access(i as u16).into_iter().collect());
        }
    }
}

fn on_connection(
    event: On<RenetServerEvent>,
    mut server: ResMut<RenetServer>,
    transport: Option<Res<bevy_renet::netcode::NetcodeServerTransport>>,
    mut tw: ResMut<TownWorld>,
    mut locks: ResMut<Locks>,
) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let id = *client_id;
            let profile = transport
                .as_ref()
                .and_then(|t| t.0.user_data(id))
                .and_then(|d| unpack_profile(&d))
                .unwrap_or_else(|| Profile {
                    key: id ^ 0x5EED_0000_0000_0000,
                    name: "Путник".into(),
                    look: Default::default(),
                    kit: Default::default(),
                });
            info!("Игрок {id}: профиль «{}»", profile.name);
            tw.players.insert(id, profile);
            let cat = estates();
            let briefs = (0..cat.estates.len() as u16).map(|i| tw.brief(i)).collect();
            let week = tw.towns.week;
            send(
                &mut server,
                id,
                &TownServerMessage::Init {
                    week,
                    date: date_text(week),
                    sites: tw.towns.sites.clone(),
                    estates: briefs,
                    you: id,
                },
            );
            // Свои дома: двери и сундуки теперь открываются.
            for i in 0..tw.estates.len() as u16 {
                if tw.estates[i as usize].owner.is_some() {
                    tw.changed.insert(i);
                }
            }
            refresh_access(&tw, &mut locks);
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            tw.players.remove(client_id);
            for i in 0..tw.estates.len() as u16 {
                if tw.estates[i as usize].owner.is_some() {
                    tw.changed.insert(i);
                }
            }
            refresh_access(&tw, &mut locks);
        }
    }
}

/// Стоит ли игрок у дома (у входа или внутри).
fn near_estate(game: &GameWorld, player: u64, id: u16) -> bool {
    let spec = &estates().estates[id as usize];
    let Some(a) = game.actors.get(&player).filter(|a| a.alive) else {
        return false;
    };
    let p = a.pose.feet;
    spec.contains(p) || p.distance(spec.entrance) < DEAL_RANGE
}

fn sync_inventory(server: &mut RenetServer, loot: &LootWorld, player: u64) {
    if let Some(inv) = loot.inventories.get(&player) {
        server.send_message(
            player,
            CH_LOOT,
            encode(&LootServerMessage::Inventory(inv.clone())),
        );
    }
}

/// Заплатить из сумки. Ошибка — текст для игрока.
fn pay(loot: &mut LootWorld, player: u64, nummi: u32) -> Result<(), String> {
    let mut uids = loot.uids.clone();
    let Some(inv) = loot.inventories.get_mut(&player) else {
        return Err("Нет сумки".into());
    };
    let r = take_money(inv, nummi, &mut uids)
        .map_err(|e| format!("{}: нужно {}", e.message(), format_money(nummi)));
    loot.uids = uids;
    r
}

#[allow(clippy::too_many_arguments)]
fn receive_town_messages(
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    level: Res<LevelColliders>,
    mut tw: ResMut<TownWorld>,
    mut locks: ResMut<Locks>,
    mut loot: ResMut<LootWorld>,
) {
    let cat = estates();
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_TOWN) {
            let Some(msg) = decode::<TownClientMessage>(&bytes) else {
                continue;
            };
            let estate = match &msg {
                TownClientMessage::Open { estate }
                | TownClientMessage::Buy { estate }
                | TownClientMessage::Rent { estate, .. }
                | TownClientMessage::Sell { estate }
                | TownClientMessage::Furnish { estate, .. }
                | TownClientMessage::Guest { estate, .. } => Some(*estate),
                TownClientMessage::News => None,
            };
            if estate.is_some_and(|e| e as usize >= cat.estates.len()) {
                continue;
            }
            let Some(key) = tw.key(pid) else { continue };
            let name = tw
                .players
                .get(&pid)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            match msg {
                TownClientMessage::News => {
                    let v = tw.news_view();
                    send(&mut server, pid, &TownServerMessage::News(v));
                    continue;
                }
                TownClientMessage::Open { estate } => {
                    let d = tw.detail(estate, pid);
                    send(&mut server, pid, &TownServerMessage::Detail(d));
                    continue;
                }
                _ => {}
            }
            let id = estate.unwrap();
            if !near_estate(&game, pid, id) {
                notice(
                    &mut server,
                    pid,
                    "Сделки по дому — у его дверей: подойдите к входу",
                );
                continue;
            }
            let spec = &cat.estates[id as usize];
            let week = tw.towns.week;
            match msg {
                TownClientMessage::Buy { estate: _ }
                | TownClientMessage::Rent { estate: _, .. } => {
                    let months = if let TownClientMessage::Rent { months, .. } = msg {
                        Some(months.clamp(1, 12) as u32)
                    } else {
                        None
                    };
                    let status = tw.status(id);
                    let extend = matches!(status, Status::Rented { .. })
                        && tw.is_owner(id, pid)
                        && months.is_some();
                    if status != Status::ForSale && !extend {
                        let why = match status {
                            Status::Building => "Дом ещё строится".to_string(),
                            _ => format!("Дом уже занят: {}", tw.estates[id as usize].owner_name),
                        };
                        notice(&mut server, pid, why);
                        continue;
                    }
                    let price = tw.price(id);
                    let cost = match months {
                        Some(m) => rent_price(price) * m,
                        None => price,
                    };
                    if let Err(e) = pay(&mut loot, pid, cost) {
                        notice(&mut server, pid, e);
                        continue;
                    }
                    sync_inventory(&mut server, &loot, pid);
                    let e = &mut tw.estates[id as usize];
                    e.owner = Some(key);
                    e.owner_name = name.clone();
                    e.warned = false;
                    match months {
                        Some(m) => {
                            let from = if extend {
                                e.rented_until.unwrap_or(week)
                            } else {
                                week
                            };
                            e.rented_until = Some(from + m * RENT_WEEKS);
                        }
                        None => e.rented_until = None,
                    }
                    // Новый хозяин — двери закрыть (ключ у него).
                    for d in &spec.doors {
                        if level.door_open(*d as usize) && !locks.frozen.contains(d) {
                            level.set_door_open(*d as usize, false);
                            server.broadcast_message(
                                crate::doors::CH_DOOR,
                                encode(&crate::doors::DoorServerMessage::Set {
                                    id: *d,
                                    open: false,
                                }),
                            );
                        }
                    }
                    let text = match months {
                        Some(m) if extend => format!(
                            "Аренда продлена на {m} мес. Заплачено {}",
                            format_money(cost)
                        ),
                        Some(m) => format!(
                            "Табуларий записал договор: «{}» ваша на {m} мес. Заплачено {}. Ключ у вас.",
                            spec.name,
                            format_money(cost)
                        ),
                        None => format!(
                            "Купчая записана у табулария: «{}» теперь ваша. Заплачено {}. Ключ у вас.",
                            spec.name,
                            format_money(cost)
                        ),
                    };
                    notice(&mut server, pid, text);
                    if !extend {
                        let what = if months.is_some() {
                            "снял"
                        } else {
                            "купил"
                        };
                        tw.towns.news.push(News {
                            week,
                            town: spec.town,
                            kind: NewsKind::People,
                            text: format!("{name} {what} дом: {}.", spec.name.to_lowercase()),
                            loud: false,
                        });
                    }
                    tw.changed.insert(id);
                    tw.dirty = true;
                }
                TownClientMessage::Sell { .. } => {
                    if !tw.is_owner(id, pid) {
                        notice(&mut server, pid, "Это не ваш дом");
                        continue;
                    }
                    let owned = tw.estates[id as usize].rented_until.is_none();
                    if owned {
                        let pay_back = buyback_price(tw.price(id));
                        let mut uids = loot.uids.clone();
                        let ok = loot
                            .inventories
                            .get_mut(&pid)
                            .map(|inv| give_money(inv, pay_back, &mut uids));
                        loot.uids = uids;
                        if !matches!(ok, Some(Ok(()))) {
                            notice(
                                &mut server,
                                pid,
                                "Деньги некуда положить: освободите место в сумке",
                            );
                            continue;
                        }
                        sync_inventory(&mut server, &loot, pid);
                        notice(
                            &mut server,
                            pid,
                            format!(
                                "Курия выкупила дом за {}. Что осталось в сундуках — уже не ваше.",
                                format_money(pay_back)
                            ),
                        );
                    } else {
                        notice(
                            &mut server,
                            pid,
                            "Вы съехали. Что осталось в сундуках — уже не ваше.",
                        );
                    }
                    tw.release(id);
                }
                TownClientMessage::Furnish { room, kind, .. } => {
                    if !tw.is_owner(id, pid) {
                        notice(&mut server, pid, "Обставлять может только хозяин");
                        continue;
                    }
                    let Some(r) = spec.rooms.get(room as usize) else {
                        continue;
                    };
                    match kind {
                        None => {
                            let e = &mut tw.estates[id as usize];
                            e.furnish[room as usize] = None;
                            e.orders.retain(|o| o.0 != room);
                            notice(
                                &mut server,
                                pid,
                                "Комнату освободили: вещи раздали бедным при ксеноне",
                            );
                        }
                        Some(k) => {
                            if !r.kinds.contains(&k) {
                                continue;
                            }
                            if tw.estates[id as usize].furnish[room as usize] == Some(k) {
                                notice(&mut server, pid, "Комната уже так обставлена");
                                continue;
                            }
                            if let Err(e) = pay(&mut loot, pid, k.price()) {
                                notice(&mut server, pid, e);
                                continue;
                            }
                            sync_inventory(&mut server, &loot, pid);
                            let e = &mut tw.estates[id as usize];
                            e.orders.retain(|o| o.0 != room);
                            e.orders.push((room, k, week + k.weeks()));
                            notice(
                                &mut server,
                                pid,
                                format!(
                                    "Мастера взялись: «{}» ({}) — через {} нед. Заплачено {}",
                                    k.name(),
                                    k.what(),
                                    k.weeks(),
                                    format_money(k.price())
                                ),
                            );
                        }
                    }
                    tw.changed.insert(id);
                    tw.dirty = true;
                }
                TownClientMessage::Guest { player, allow, .. } => {
                    if !tw.is_owner(id, pid) {
                        continue;
                    }
                    let Some(guest) = tw.players.get(&player).cloned() else {
                        continue;
                    };
                    let e = &mut tw.estates[id as usize];
                    e.guests.retain(|(k, _)| *k != guest.key);
                    if allow {
                        e.guests.push((guest.key, guest.name.clone()));
                        notice(
                            &mut server,
                            player,
                            format!("{name} дал вам ключ от дома: {}", spec.name.to_lowercase()),
                        );
                    }
                    tw.changed.insert(id);
                    tw.dirty = true;
                }
                _ => {}
            }
            refresh_access(&tw, &mut locks);
            apply_switches(&tw, &level, &mut locks);
            let d = tw.detail(id, pid);
            send(&mut server, pid, &TownServerMessage::Detail(d));
        }
    }
}

/// Игровая неделя: города живут, мастера сдают работу, кончается аренда.
#[allow(clippy::too_many_arguments)]
fn tick_towns(
    mut server: ResMut<RenetServer>,
    time: Res<Time>,
    level: Res<LevelColliders>,
    mut tw: ResMut<TownWorld>,
    mut locks: ResMut<Locks>,
    mut game: ResMut<GameWorld>,
    trade: Option<ResMut<crate::trade_server::TradeWorld>>,
    mut started: Local<bool>,
) {
    let now = time.elapsed_secs_f64();
    if !*started {
        *started = true;
        tw.next_week_at = now + tw.week_secs;
        if let Some(mut trade) = trade {
            apply_market(&tw, &mut trade);
        }
        return;
    }
    if now < tw.next_week_at {
        return;
    }
    tw.next_week_at = now + tw.week_secs;
    let news_before = tw.towns.news.len();
    let last_news_week = tw.towns.news.last().map(|n| n.week);
    let mut cues = tw.towns.tick_week();
    if pin_sites(&mut tw.towns) {
        cues.retain(|c| !matches!(c, WorldCue::SiteStage { .. } | WorldCue::HouseBuilt { .. }));
    }
    let week = tw.towns.week;
    let cat = estates();
    for cue in cues {
        match cue {
            WorldCue::SiteStage { site, stage } => {
                server.broadcast_message(CH_TOWN, encode(&TownServerMessage::Site { site, stage }));
                if stage == SITE_DONE {
                    // Двери навешены: закрыть.
                    if let Some(doors) = cat.site_doors.get(site as usize) {
                        for d in doors {
                            level.set_door_open(*d as usize, false);
                            server.broadcast_message(
                                crate::doors::CH_DOOR,
                                encode(&crate::doors::DoorServerMessage::Set {
                                    id: *d,
                                    open: false,
                                }),
                            );
                        }
                    }
                }
            }
            WorldCue::HouseBuilt { site } => {
                if let Some(i) = cat.estates.iter().position(|e| e.site == Some(site)) {
                    tw.changed.insert(i as u16);
                }
            }
            WorldCue::Raid { town, raiders } => {
                let ids = spawn_raid(&mut game, &level, town, raiders, now);
                if !ids.is_empty() {
                    tw.raids.push((now + RAID_SECS, ids, town));
                }
            }
            WorldCue::Plague { .. } => {}
        }
    }
    // Мастера сдают работу; аренда кончается.
    let mut ended = Vec::new();
    for i in 0..tw.estates.len() {
        let e = &mut tw.estates[i];
        let ready: Vec<(u8, FurnishKind)> = e
            .orders
            .iter()
            .filter(|o| o.2 <= week)
            .map(|o| (o.0, o.1))
            .collect();
        if !ready.is_empty() {
            e.orders.retain(|o| o.2 > week);
            for (r, k) in &ready {
                if let Some(f) = e.furnish.get_mut(*r as usize) {
                    *f = Some(*k);
                }
            }
            let owner = e.owner;
            let names: Vec<&str> = ready.iter().map(|(_, k)| k.name()).collect();
            let text = format!(
                "Мастера закончили работу в вашем доме: {}",
                names.join(", ").to_lowercase()
            );
            for (c, p) in tw.players.clone() {
                if Some(p.key) == owner {
                    notice(&mut server, c, text.clone());
                }
            }
            tw.changed.insert(i as u16);
        }
        let e = &mut tw.estates[i];
        if let Some(until) = e.rented_until {
            if until <= week {
                ended.push(i as u16);
            } else if until == week + 1 && !e.warned {
                e.warned = true;
                let owner = e.owner;
                for (c, p) in tw.players.clone() {
                    if Some(p.key) == owner {
                        notice(
                            &mut server,
                            c,
                            format!(
                                "Через неделю кончается аренда: {}. Продлите у дверей дома (U), иначе вещи из сундуков достанутся новому жильцу.",
                                cat.estates[i].name.to_lowercase()
                            ),
                        );
                    }
                }
            }
        }
    }
    for i in ended {
        let owner = tw.estates[i as usize].owner;
        for (c, p) in tw.players.clone() {
            if Some(p.key) == owner {
                notice(
                    &mut server,
                    c,
                    format!(
                        "Аренда кончилась: {}. Дом снова сдаётся.",
                        cat.estates[i as usize].name.to_lowercase()
                    ),
                );
            }
        }
        tw.release(i);
    }
    // Цены у торговцев.
    if let Some(mut trade) = trade {
        apply_market(&tw, &mut trade);
    }
    // Цены на дома меняются каждую неделю.
    for i in 0..cat.estates.len() as u16 {
        tw.changed.insert(i);
    }
    refresh_access(&tw, &mut locks);
    apply_switches(&tw, &level, &mut locks);
    let date = date_text(week);
    server.broadcast_message(CH_TOWN, encode(&TownServerMessage::Week { week, date }));
    // Глашатай: громкие вести этой недели.
    let fresh: Vec<String> = tw.towns.news[news_before.min(tw.towns.news.len())..]
        .iter()
        .filter(|n| n.loud)
        .map(|n| n.text.clone())
        .collect();
    let _ = last_news_week;
    for text in fresh {
        server.broadcast_message(CH_TOWN, encode(&TownServerMessage::Crier(text)));
    }
    tw.dirty = true;
}

/// Налётчики у селения: со стороны степи, по двое-трое.
fn spawn_raid(
    game: &mut GameWorld,
    level: &LevelColliders,
    town: TownId,
    n: u32,
    now: f64,
) -> Vec<u64> {
    let Some(place) = town.def().place.and_then(crate::level::place_pos) else {
        return Vec::new();
    };
    // Херсон: налёт на клеры (усадьба с виноградниками за стенами).
    let center = if town == TownId::Herson {
        crate::level::place_pos("kleros").unwrap_or(place)
    } else {
        place
    };
    // Степь — на востоке и юге карты.
    let from = (crate::level::place_pos("steppe_camp")
        .unwrap_or(center + Vec3::new(200.0, 0.0, 200.0))
        - center)
        .normalize_or(Vec3::Z);
    let mut ids = Vec::new();
    for k in 0..n.min(8) {
        let a = k as f32 * 1.3;
        let off = from * 26.0 + Vec3::new(a.cos(), 0.0, a.sin()) * 5.0;
        let mut pos = center + off;
        pos.y = crate::level::ground_y(pos.x, pos.z);
        let kind = if k % 3 == 2 {
            ActorKind::GothWarrior
        } else {
            ActorKind::HunArcher
        };
        let to = center - pos;
        let mut spawn = crate::npc::NpcSpawn::brigand(kind, pos, (-to.x).atan2(-to.z));
        spawn.wander = 14.0;
        ids.push(game.spawn_raider(level, spawn, now));
    }
    ids
}

fn end_raids(
    mut server: ResMut<RenetServer>,
    time: Res<Time>,
    mut tw: ResMut<TownWorld>,
    mut game: ResMut<GameWorld>,
) {
    let now = time.elapsed_secs_f64();
    let (done, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut tw.raids)
        .into_iter()
        .partition(|r| r.0 <= now);
    tw.raids = rest;
    for (_, ids, town) in done {
        let mut left = 0;
        for id in ids {
            if game.actors.get(&id).is_some_and(|a| a.alive) {
                left += 1;
            }
            game.remove_npc(&mut server, id);
        }
        if left > 0 {
            server.broadcast_message(
                CH_TOWN,
                encode(&TownServerMessage::Crier(format!(
                    "Налётчики ушли в степь с добычей ({}).",
                    town.def().name
                ))),
            );
        } else {
            server.broadcast_message(
                CH_TOWN,
                encode(&TownServerMessage::Crier(format!(
                    "{}: набег отбит, налётчики перебиты.",
                    town.def().name
                ))),
            );
        }
    }
}

/// Цены и подвоз у торговцев — по рынку их поселения.
fn apply_market(tw: &TownWorld, trade: &mut crate::trade_server::TradeWorld) {
    for m in trade.merchants.iter_mut() {
        let town = tw.towns.town(TownId::of_place(m.id.def().place));
        for (c, slot) in m.market.iter_mut().enumerate() {
            *slot = market_mult(town, category_at(c));
        }
        for (c, slot) in m.supply.iter_mut().enumerate() {
            *slot = supply_mult(town, category_at(c));
        }
    }
}

fn category_at(i: usize) -> Category {
    use Category::*;
    [
        Weapon, Armor, Jewelry, Coin, Remedy, Herb, SteppeGood, Import, Material, Vessel, Book,
        WineOil, Textile, Hide, Food, Gem, Dye,
    ][i]
}

fn send_changes(mut server: ResMut<RenetServer>, mut tw: ResMut<TownWorld>) {
    if tw.changed.is_empty() {
        return;
    }
    let mut ids: Vec<u16> = tw.changed.drain().collect();
    ids.sort();
    for id in ids {
        let b = tw.brief(id);
        server.broadcast_message(CH_TOWN, encode(&TownServerMessage::Estate(b)));
    }
}

fn tell_denied(mut server: ResMut<RenetServer>, mut locks: ResMut<Locks>) {
    for (pid, text) in std::mem::take(&mut locks.denied) {
        server.send_message(pid, CH_LOOT, encode(&LootServerMessage::Notice(text)));
    }
}

fn save_world(time: Res<Time>, mut tw: ResMut<TownWorld>, loot: Res<LootWorld>) {
    let now = time.elapsed_secs_f64();
    // Сделки — через пару секунд, сундуки (их меняют без нас) — раз в полминуты.
    if !((tw.dirty && now - tw.last_save > 2.0) || now - tw.last_save > SAVE_EVERY) {
        return;
    }
    tw.last_save = now;
    tw.dirty = false;
    let Some(path) = tw.save_path.clone() else {
        return;
    };
    let cat = estates();
    let mut storage = Vec::new();
    for spec in &cat.estates {
        for c in &spec.containers {
            let Some(inv) = loot
                .corpses
                .get(&(CONTAINER_ID_BASE + *c as u64))
                .map(|c| &c.inv)
            else {
                continue;
            };
            if inv.grid.items.is_empty() {
                continue;
            }
            storage.push((
                *c,
                inv.grid
                    .items
                    .iter()
                    .map(|p| SavedPlaced {
                        name: p.item.info().name.to_string(),
                        placed: p.clone(),
                    })
                    .collect(),
            ));
        }
    }
    let file = SaveFile {
        version: 1,
        towns: tw.towns.clone(),
        estates: tw.estates.clone(),
        storage,
    };
    let Ok(json) = serde_json::to_string(&file) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_keep_strangers_out() {
        let mut locks = Locks::default();
        locks.door_estate.insert(5, 0);
        locks.container_estate.insert(9, 0);
        // Ничей дом — открыт всем.
        assert!(locks.door(5, 1));
        locks.allowed.insert(0, [1u64].into_iter().collect());
        assert!(locks.door(5, 1));
        assert!(!locks.door(5, 2));
        assert!(!locks.container(9, 2));
        assert!(locks.container(9, 1));
        assert!(locks.door(77, 2), "чужие двери не трогаем");
        locks.frozen.insert(5);
        assert!(!locks.door(5, 1));
        assert_eq!(locks.denied.len(), 3);
    }

    #[test]
    fn world_save_round_trip() {
        unsafe { std::env::set_var("MRPG_SAVE", "off") };
        let mut tw = TownWorld::new_or_load();
        assert_eq!(tw.estates.len(), estates().estates.len());
        tw.estates[0].owner = Some(42);
        tw.estates[0].owner_name = "Феодор".into();
        tw.estates[0].furnish[0] = Some(FurnishKind::Living);
        let file = SaveFile {
            version: 1,
            towns: tw.towns.clone(),
            estates: tw.estates.clone(),
            storage: vec![],
        };
        let json = serde_json::to_string(&file).unwrap();
        let back: SaveFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.estates[0], tw.estates[0]);
        tw.players.insert(
            7,
            Profile {
                key: 42,
                name: "Феодор".into(),
                look: Default::default(),
                kit: Default::default(),
            },
        );
        tw.players.insert(
            8,
            Profile {
                key: 43,
                name: "Гость".into(),
                look: Default::default(),
                kit: Default::default(),
            },
        );
        assert!(tw.is_owner(0, 7));
        assert!(!tw.is_owner(0, 8));
        assert_eq!(tw.access(0), vec![7]);
        tw.estates[0].guests.push((43, "Гость".into()));
        assert_eq!(tw.access(0), vec![7, 8]);
        assert_eq!(
            tw.status(0),
            if estates().estates[0].site.is_some() {
                Status::Building
            } else {
                Status::Owned
            }
        );
    }
}
