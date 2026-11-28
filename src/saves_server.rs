//! Сервер: помнит персонажей игроков между входами и пишет сохранения (см. `saves.rs`).
//!
//! * Вошёл игрок — по ключу профиля ищем его персонажа и возвращаем сумку, знания, репутацию,
//!   раны, клейма; место и здоровье берёт `server.rs` при появлении ([`CharacterStore::spawn_for`]).
//! * Пока игрок в игре, раз в полсекунды снимаем с него «слепок» (в памяти), раз в полминуты
//!   и при выходе игрока пишем всех на диск — файл персонажей текущей игры.
//! * «Сохранить игру» хозяина — мир, персонажи и описание в слот; друга — только его персонаж
//!   (у хоста, в файле текущей игры).

use crate::estates::unpack_profile;
use crate::factions::Reputation;
use crate::knowledge::KnowledgeWorld;
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::lore_net::{CH_LORE, LoreServerMessage};
use crate::medicine_server::Bodies;
use crate::protocol::{ActorKind, encode};
use crate::saves::*;
use crate::server::GameWorld;
use crate::smithing_server::SmithWorld;
use crate::trade_server::TradeWorld;
use crate::world_clock::WorldClock;
use bevy::prelude::*;
use bevy_renet::netcode::NetcodeServerTransport;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Как часто снимать слепки с игроков (с).
const SNAPSHOT_EVERY: f32 = 0.5;
/// Как часто писать персонажей на диск, пока кто-то в игре (с).
const WRITE_EVERY: f64 = 30.0;

/// Просьба сохранить: (кто, слот или только свой персонаж, секрет хозяина).
#[derive(Clone, Copy, Debug)]
pub struct SaveRequest {
    pub player: u64,
    pub slot: Option<u8>,
    pub token: u64,
}

#[derive(Resource, Default)]
pub struct SaveRequests(pub Vec<SaveRequest>);

#[derive(Resource)]
pub struct CharacterStore {
    file: CharactersFile,
    /// Игрок в игре -> ключ профиля.
    keys: HashMap<u64, u64>,
    /// Только что вошли: персонажа ещё не вернули.
    pending: Vec<u64>,
    /// Персонаж возвращён — с него можно снимать слепки.
    applied: HashSet<u64>,
    path: Option<PathBuf>,
    dirty: bool,
    last_write: f64,
    snap_timer: f32,
}

fn level_name() -> &'static str {
    match crate::level::active() {
        crate::level::LevelKind::Chersonesus => "Херсонес",
        crate::level::LevelKind::Castle => "Замок",
    }
}

impl CharacterStore {
    pub fn load() -> Self {
        let path = characters_path();
        let file = path.as_deref().and_then(CharactersFile::read).unwrap_or(CharactersFile { version: 1, ..Default::default() });
        if !file.characters.is_empty() {
            info!("Персонажи загружены: {}", file.characters.len());
        }
        Self { file, keys: HashMap::new(), pending: Vec::new(), applied: HashSet::new(), path, dirty: false, last_write: 0.0, snap_timer: 0.0 }
    }

    /// Где появиться вернувшемуся игроку: (ступни, поворот, здоровье). `None` — нового
    /// персонажа (или погибшего) ставим на случайное место.
    pub fn spawn_for(&self, key: u64) -> Option<(Vec3, f32, Option<f32>)> {
        let c = self.file.characters.get(&key)?;
        if !c.level.is_empty() && c.level != level_name() {
            return None;
        }
        let p = c.pos?;
        let pos = Vec3::from_array(p);
        pos.is_finite().then_some((pos, c.yaw, c.hp))
    }

    fn text(&self) -> String {
        serde_json::to_string(&self.file).unwrap_or_default()
    }

    fn write(&mut self, now: f64) {
        self.last_write = now;
        self.dirty = false;
        let Some(path) = self.path.clone() else { return };
        if let Err(e) = write_atomic(&path, &self.text()) {
            warn!("Не записать персонажей в {}: {e}", path.display());
        }
    }
}

pub struct SavesServerPlugin;

impl Plugin for SavesServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CharacterStore::load())
            .init_resource::<SaveRequests>()
            .add_observer(on_connection)
            .add_systems(Startup, restore_clock)
            .add_systems(Update, (restore_characters, snapshot_characters, handle_requests, write_characters).chain());
    }
}

/// Часы мира — как были при сохранении (`MRPG_HOUR` для снимков важнее).
fn restore_clock(store: Res<CharacterStore>, mut clock: ResMut<WorldClock>) {
    if std::env::var_os("MRPG_HOUR").is_none()
        && let Some(game) = store.file.clock.filter(|g| g.is_finite() && *g >= 0.0)
    {
        clock.game = game;
    }
}

fn on_connection(event: On<RenetServerEvent>, mut store: ResMut<CharacterStore>, transport: Option<Res<NetcodeServerTransport>>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            // Без профиля (бот, старый клиент) персонажа не помним.
            let Some(profile) = transport.as_ref().and_then(|t| t.user_data(*client_id)).and_then(|d| unpack_profile(&d)) else { return };
            // Тот же профиль вошёл второй раз (второе окно) — старый вход больше не пишем.
            let dup: Vec<u64> = store.keys.iter().filter(|(_, k)| **k == profile.key).map(|(c, _)| *c).collect();
            for c in dup {
                store.keys.remove(&c);
                store.applied.remove(&c);
            }
            store.keys.insert(*client_id, profile.key);
            store.pending.push(*client_id);
            store.file.characters.entry(profile.key).or_default().name = profile.name;
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            store.pending.retain(|c| c != client_id);
            if store.applied.remove(client_id) {
                store.dirty = true;
            }
            store.keys.remove(client_id);
        }
    }
}

/// Вернуть вошедшим их персонажей (в Update: к этому времени остальные системы уже завели им
/// стартовую сумку и пустые знания — заменяем своими).
#[allow(clippy::too_many_arguments)]
fn restore_characters(
    mut store: ResMut<CharacterStore>,
    mut server: ResMut<RenetServer>,
    mut loot: ResMut<LootWorld>,
    mut knowledge: Option<ResMut<KnowledgeWorld>>,
    mut trade: Option<ResMut<TradeWorld>>,
    mut smith: Option<ResMut<SmithWorld>>,
    mut bodies: Option<ResMut<Bodies>>,
) {
    if store.pending.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut store.pending);
    for id in pending {
        let Some(key) = store.keys.get(&id).copied() else { continue };
        store.applied.insert(id);
        let Some(c) = store.file.characters.get(&key).cloned() else { continue };
        let mut restored = Vec::new();
        if let Some(saved) = &c.inventory {
            let inv = restore_inventory(saved, &mut loot.uids);
            server.send_message(id, CH_LOOT, encode(&LootServerMessage::Inventory(inv.clone())));
            loot.inventories.insert(id, inv);
            restored.push("вещи");
        }
        if let (Some(v), Some(kw)) = (&c.knowledge, knowledge.as_mut()) {
            let k = restore_knowledge(v);
            server.send_message(id, CH_LORE, encode(&LoreServerMessage::Knowledge(k.clone())));
            kw.players.insert(id, k);
            restored.push("знания");
        }
        if let (Some(r), Some(tw)) = (&c.reputation, trade.as_mut()) {
            let mut rep = Reputation::default();
            for (slot, v) in rep.0.iter_mut().zip(r) {
                *slot = v.clamp(-100.0, 100.0);
            }
            tw.restore_standing(id, rep, c.fine);
        }
        if let (Some(v), Some(sw)) = (&c.guild, smith.as_mut())
            && let Ok(g) = serde_json::from_value(v.clone())
        {
            sw.guild.insert(id, g);
            crate::smithing_server::send_guild(&mut server, sw, id);
        }
        if let (Some(v), Some(b)) = (&c.body, bodies.as_mut())
            && let Some(body) = restore_body(v)
        {
            b.bodies.insert(id, body);
        }
        info!("Персонаж «{}» вернулся ({})", c.name, if restored.is_empty() { "новый".to_string() } else { restored.join(", ") });
    }
}

/// Снять слепки со всех вернувшихся в игру персонажей.
#[allow(clippy::too_many_arguments)]
fn snapshot_all(
    store: &mut CharacterStore,
    world: &GameWorld,
    loot: &LootWorld,
    knowledge: Option<&KnowledgeWorld>,
    trade: Option<&TradeWorld>,
    smith: Option<&SmithWorld>,
    bodies: Option<&Bodies>,
    clock: &WorldClock,
) {
    store.file.clock = Some(clock.game);
    let ids: Vec<(u64, u64)> = store.keys.iter().filter(|(c, _)| store.applied.contains(c)).map(|(c, k)| (*c, *k)).collect();
    for (id, key) in ids {
        let c = store.file.characters.entry(key).or_default();
        if let Some(a) = world.actors.get(&id).filter(|a| a.kind == ActorKind::Player) {
            if a.alive && a.fighter.hp > 0.0 {
                c.pos = Some(a.pose.feet.to_array());
                c.yaw = a.pose.yaw;
                c.hp = Some(a.fighter.hp);
            } else {
                // Погиб: вернётся, как после смерти, на случайное место.
                c.pos = None;
                c.hp = None;
            }
            c.level = level_name().to_string();
        }
        if let Some(inv) = loot.inventories.get(&id) {
            c.inventory = Some(save_inventory(inv));
        }
        if let Some(k) = knowledge.and_then(|kw| kw.players.get(&id)) {
            c.knowledge = Some(save_knowledge(k));
        }
        if let Some((rep, fine)) = trade.and_then(|tw| tw.saved_standing(id)) {
            c.reputation = Some(rep.0.to_vec());
            c.fine = fine;
        }
        if let Some(g) = smith.and_then(|sw| sw.guild.get(&id)) {
            c.guild = serde_json::to_value(g).ok();
        }
        if let Some(b) = bodies {
            c.body = b.bodies.get(&id).and_then(save_body);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot_characters(
    time: Res<Time>,
    mut store: ResMut<CharacterStore>,
    world: Res<GameWorld>,
    loot: Res<LootWorld>,
    knowledge: Option<Res<KnowledgeWorld>>,
    trade: Option<Res<TradeWorld>>,
    smith: Option<Res<SmithWorld>>,
    bodies: Option<Res<Bodies>>,
    clock: Res<WorldClock>,
) {
    store.snap_timer -= time.delta_secs();
    if store.snap_timer > 0.0 || store.applied.is_empty() {
        return;
    }
    store.snap_timer = SNAPSHOT_EVERY;
    snapshot_all(&mut store, &world, &loot, knowledge.as_deref(), trade.as_deref(), smith.as_deref(), bodies.as_deref(), &clock);
}

fn write_characters(time: Res<Time>, mut store: ResMut<CharacterStore>) {
    let now = time.elapsed_secs_f64();
    let due = store.dirty || (!store.applied.is_empty() && now - store.last_write > WRITE_EVERY);
    if due {
        store.write(now);
    }
}

fn notice(server: &mut RenetServer, player: u64, text: impl Into<String>) {
    server.send_message(player, CH_LOOT, encode(&LootServerMessage::Notice(text.into())));
}

/// «Сохранить»: хозяину — слот (мир и все персонажи), другу — его персонаж.
#[allow(clippy::too_many_arguments)]
fn handle_requests(
    mut requests: ResMut<SaveRequests>,
    mut server: ResMut<RenetServer>,
    time: Res<Time>,
    mut store: ResMut<CharacterStore>,
    world: Res<GameWorld>,
    loot: Res<LootWorld>,
    knowledge: Option<Res<KnowledgeWorld>>,
    trade: Option<Res<TradeWorld>>,
    smith: Option<Res<SmithWorld>>,
    bodies: Option<Res<Bodies>>,
    clock: Res<WorldClock>,
    towns: Option<Res<crate::towns_server::TownWorld>>,
    names: Res<crate::server::PlayerNames>,
) {
    if requests.0.is_empty() {
        return;
    }
    let list = std::mem::take(&mut requests.0);
    // Свежий слепок со всех, кто в игре, — и в файл текущей игры.
    snapshot_all(&mut store, &world, &loot, knowledge.as_deref(), trade.as_deref(), smith.as_deref(), bodies.as_deref(), &clock);
    store.write(time.elapsed_secs_f64());
    let host = crate::console_net::host_token();
    for r in list {
        let is_host = host != 0 && r.token == host;
        let Some(slot) = r.slot.filter(|_| is_host) else {
            if store.applied.contains(&r.player) {
                notice(&mut server, r.player, if is_host { "Персонаж сохранён" } else { "Персонаж сохранён у хоста. Загружать игру может только хост." });
            } else {
                notice(&mut server, r.player, "Без профиля персонаж не сохраняется");
            }
            continue;
        };
        if slot > SLOTS {
            continue;
        }
        let world_text = towns.as_ref().filter(|_| crate::level::active() == crate::level::LevelKind::Chersonesus).and_then(|t| t.save_text(&loot));
        // Хозяин — первым в списке игроков.
        let mut players: Vec<String> = vec![names.0.get(&r.player).cloned().unwrap_or_default()];
        players.extend(names.0.iter().filter(|(id, _)| **id != r.player).map(|(_, n)| n.clone()));
        players.retain(|n| !n.is_empty());
        let purse = loot.inventories.get(&r.player).map(|inv| crate::economy::format_money(crate::economy::purse(inv))).unwrap_or_default();
        let meta = SlotMeta {
            version: 1,
            saved_at: now_unix(),
            date: towns.as_ref().map(|t| t.date()).unwrap_or_default(),
            time: clock.hhmm(),
            players,
            purse,
            level: level_name().to_string(),
        };
        let dir = slot_dir(slot);
        match write_slot(&dir, world_text.as_deref(), &store.text(), &meta) {
            Ok(()) => {
                info!("Игра сохранена: {} ({})", slot_name(slot), dir.display());
                notice(&mut server, r.player, format!("Игра сохранена: {}", slot_name(slot).to_lowercase()));
            }
            Err(e) => notice(&mut server, r.player, format!("Не удалось сохранить: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_only_on_same_level_and_alive() {
        unsafe { std::env::set_var("MRPG_SAVE", "off") };
        let mut store = CharacterStore::load();
        store.file.characters.insert(1, CharacterSave { pos: Some([1.0, 2.0, 3.0]), yaw: 0.5, hp: Some(70.0), level: level_name().into(), ..Default::default() });
        store.file.characters.insert(2, CharacterSave { pos: None, ..Default::default() });
        store.file.characters.insert(3, CharacterSave { pos: Some([1.0, 2.0, 3.0]), level: "Другой".into(), ..Default::default() });
        assert_eq!(store.spawn_for(1), Some((Vec3::new(1.0, 2.0, 3.0), 0.5, Some(70.0))));
        assert_eq!(store.spawn_for(2), None);
        assert_eq!(store.spawn_for(3), None);
        assert_eq!(store.spawn_for(9), None);
    }
}
