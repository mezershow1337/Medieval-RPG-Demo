//! Сервер кузниц: горны в поселениях, работа игроков у наковальни, износ и ремонт вещей.
//!
//! Модель — в `smithing.rs`, здесь — проверка, что у игрока есть металл и он стоит у горна,
//! шаг времени, уголь в горне, выдача готовых вещей, навык и знание «на опыте».

use crate::combat::Action;
use crate::inventory::{Inventory, Target};
use crate::items::{EquipSlot, Flaw, Item, ItemBaseId, ItemUid, Make, Quality, Rng, plain_item};
use crate::knowledge::KnowledgeWorld;
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::lore::{Knowledge, Lore, Skill, Technique};
use crate::lore_net::{CH_LORE, LoreServerMessage};
use crate::protocol::{decode, encode};
use crate::server::GameWorld;
use crate::smithing::*;
use crate::smithing_net::*;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::HashMap;

pub struct SmithingServerPlugin;

impl Plugin for SmithingServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SmithWorld::default())
            .add_observer(on_connection)
            .add_systems(Startup, setup_stations)
            .add_systems(Update, (smith_kit, receive_smith_messages, tick_works, apply_wear).chain());
    }
}

/// Горн в поселении.
pub struct Station {
    pub name: String,
    pub kind: StationKind,
    pub fire: Vec3,
    pub anvil: Vec3,
    pub trough: Vec3,
    pub grind: Vec3,
    /// Секунды горения угля.
    pub fuel: f32,
    pub blast: f32,
    sent: (f32, f32),
}

pub struct Work {
    pub station: u32,
    pub job: Job,
    pub bellows: bool,
    /// Что взято из сумки (вернуть часть, если бросить работу).
    taken: Vec<(String, u16)>,
    since_view: f32,
}

#[derive(Resource)]
pub struct SmithWorld {
    pub stations: HashMap<u32, Station>,
    pub works: HashMap<u64, Work>,
    rng: Rng,
    /// С последней рассылки раскалённых поковок (с); было ли что слать.
    pieces_t: f32,
    pieces_any: bool,
}

impl Default for SmithWorld {
    fn default() -> Self {
        Self { stations: HashMap::new(), works: HashMap::new(), rng: Rng::from_time(), pieces_t: 0.0, pieces_any: false }
    }
}

impl SmithWorld {
    fn views(&self) -> Vec<StationView> {
        let mut v: Vec<StationView> = self.stations.iter().map(|(id, s)| StationView { id: *id, name: s.name.clone(), kind: s.kind, fire: s.fire.into(), anvil: s.anvil.into(), trough: s.trough.into(), grind: s.grind.into() }).collect();
        v.sort_by_key(|s| s.id);
        v
    }
}

fn send(server: &mut RenetServer, player: u64, msg: &SmithServerMessage) {
    server.send_message(player, CH_SMITH, encode(msg));
}

/// Всем: искры, пар, звон. Клиент сам отсекает далёкие.
fn fx(server: &mut RenetServer, station: u32, player: u64, fx: Fx) {
    server.broadcast_message(CH_SMITH, encode(&SmithServerMessage::Fx { station, player, fx }));
}

fn piece_view(player: u64, w: &Work) -> Option<PieceView> {
    PieceView::of(player, w.station, &w.job)
}

fn notice(server: &mut RenetServer, player: u64, text: impl Into<String>) {
    send(server, player, &SmithServerMessage::Notice(text.into()));
}

fn sync_inventory(server: &mut RenetServer, loot: &LootWorld, player: u64) {
    if let Some(inv) = loot.inventories.get(&player) {
        server.send_message(player, CH_LOOT, encode(&LootServerMessage::Inventory(inv.clone())));
    }
}

fn sync_knowledge(server: &mut RenetServer, k: &Knowledge, player: u64) {
    server.send_message(player, CH_LORE, encode(&LoreServerMessage::Knowledge(k.clone())));
}

fn send_work(server: &mut RenetServer, world: &SmithWorld, player: u64) {
    let view = world.works.get(&player).map(|w| WorkView { station: w.station, job: w.job.clone(), bellows: w.bellows });
    send(server, player, &SmithServerMessage::Work(view));
}

// ---------------------------------------------------------------------------
// Горны
// ---------------------------------------------------------------------------

fn setup_stations(mut world: ResMut<SmithWorld>) {
    for (i, f) in crate::level::forges().iter().enumerate() {
        let fuel = if f.kind == StationKind::Forge { FUEL_MAX * 0.5 } else { 0.0 };
        world.stations.insert(i as u32 + 1, Station { name: f.name.to_string(), kind: f.kind, fire: f.fire, anvil: f.anvil, trough: f.trough, grind: f.grind, fuel, blast: 0.0, sent: (-1.0, -1.0) });
    }
}

fn on_connection(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, mut world: ResMut<SmithWorld>, mut loot: ResMut<LootWorld>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let views = world.views();
            send(&mut server, *client_id, &SmithServerMessage::Stations(views));
            for (id, st) in &world.stations {
                send(&mut server, *client_id, &SmithServerMessage::Fire { station: *id, fuel: st.fuel, blast: st.blast });
            }
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            // Незаконченная вещь при ремонте возвращается владельцу... если он ещё здесь.
            if let Some(w) = world.works.remove(client_id)
                && let Job::Repair { item, .. } = w.job
                && let Some(inv) = loot.inventories.get_mut(client_id)
            {
                let _ = inv.place(item, Target::Auto);
            }
        }
    }
}

fn near(feet: Vec3, station: &Station) -> bool {
    let d = station.anvil - feet;
    Vec2::new(d.x, d.z).length() <= SMITH_REACH && d.y.abs() < 2.5
}

// ---------------------------------------------------------------------------
// Сумка
// ---------------------------------------------------------------------------

fn count_named(inv: &Inventory, name: &str) -> u32 {
    inv.grid.items.iter().filter(|p| p.item.info().name == name).map(|p| p.item.count as u32).sum()
}

/// Забрать `n` штук по имени (только из сетки, не надетое). Возвращает взятые (для качества).
fn take_named(inv: &mut Inventory, name: &str, mut n: u32) -> Vec<Item> {
    let stacks: Vec<ItemUid> = inv.grid.items.iter().filter(|p| p.item.info().name == name).map(|p| p.item.uid).collect();
    let mut out = Vec::new();
    for uid in stacks {
        if n == 0 {
            break;
        }
        let Some((mut item, loc)) = inv.take(uid) else { continue };
        let used = (item.count as u32).min(n);
        n -= used;
        let mut part = item.clone();
        part.count = used as u16;
        out.push(part);
        item.count -= used as u16;
        if item.count > 0 {
            let target = match loc {
                crate::inventory::Loc::Grid { x, y } => Target::Grid { x, y },
                crate::inventory::Loc::Equip(s) => Target::Equip(s),
            };
            if let Err(rest) = inv.place(item, target) {
                let _ = inv.place(rest, Target::Auto);
            }
        }
    }
    out
}

/// Положить в сумку; что не влезло — на землю.
fn give(commands: &mut Commands, inv: &mut Inventory, item: Item, feet: Vec3) {
    if let Err(rest) = inv.place(item, Target::Auto) {
        commands.trigger(crate::loot_server::SpawnGroundItem { pos: feet + Vec3::Y * 0.9, vel: Vec3::new(0.0, 1.0, 0.0), item: rest });
    }
}

fn give_named(commands: &mut Commands, loot: &mut LootWorld, player: u64, name: &str, quality: Quality, count: u16, feet: Vec3) {
    let Some(base) = ItemBaseId::find(name) else { return };
    let mut left = count;
    while left > 0 {
        let item = plain_item(base, quality, left, &mut loot.uids);
        left -= item.count;
        if let Some(inv) = loot.inventories.get_mut(&player) {
            give(commands, inv, item, feet);
        }
    }
}

// ---------------------------------------------------------------------------
// Начало работы
// ---------------------------------------------------------------------------

fn begin(world: &mut SmithWorld, loot: &mut LootWorld, k: &Knowledge, pid: u64, station: u32, plan: Plan) -> Result<String, String> {
    let st = world.stations.get(&station).ok_or("Нет такого горна")?;
    let kind = st.kind;
    let inv = loot.inventories.get_mut(&pid).ok_or("Нет сумки")?;
    let mut taken: Vec<(String, u16)> = Vec::new();
    let job = match plan {
        Plan::Forge { pattern, metal } => {
            if kind != StationKind::Forge {
                return Err("Ковать можно только у горна с наковальней".into());
            }
            let info = pattern.info();
            if let Some(t) = info.needs
                && !k.knows(Lore::Technique(t))
            {
                return Err(format!("Вы не знаете, как это делается. Нужно знать: {}", t.name().to_lowercase()));
            }
            let allowed: &[&str] = match pattern {
                Pattern::Bloom => &[BLOOM],
                Pattern::ScrapWeld => &[SCRAP],
                _ => &[IRON, STEEL],
            };
            // Берём указанные полосы; лом — стопкой.
            let mut bars: Vec<Item> = Vec::new();
            let mut need = info.metal as u32;
            if pattern == Pattern::ScrapWeld {
                if count_named(inv, SCRAP) < need {
                    return Err(format!("Нужно {need} шт. железного лома"));
                }
                bars = take_named(inv, SCRAP, need);
                let _ = need;
            } else {
                let mut picked: Vec<ItemUid> = metal.into_iter().collect();
                picked.dedup();
                // Сначала проверка, потом изъятие.
                let mut have = 0u32;
                for uid in &picked {
                    let Some((_, it)) = inv.find(*uid) else { continue };
                    if allowed.contains(&it.info().name) {
                        have += it.count as u32;
                    }
                }
                if have < need {
                    let what = allowed.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>().join(" или ");
                    return Err(format!("Нужно {need} шт.: {what}"));
                }
                for uid in picked {
                    if need == 0 {
                        break;
                    }
                    let Some((it, _)) = inv.find(uid).map(|(l, i)| (i.clone(), l)) else { continue };
                    if !allowed.contains(&it.info().name) {
                        continue;
                    }
                    let n = need.min(it.count as u32);
                    let Some((mut whole, loc)) = inv.take(uid) else { continue };
                    let mut part = whole.clone();
                    part.count = n as u16;
                    whole.count -= n as u16;
                    if whole.count > 0 {
                        let target = match loc {
                            crate::inventory::Loc::Grid { x, y } => Target::Grid { x, y },
                            crate::inventory::Loc::Equip(s) => Target::Equip(s),
                        };
                        let _ = inv.place(whole, target);
                    }
                    need -= n;
                    bars.push(part);
                }
            }
            let total: f32 = bars.iter().map(|b| b.count as f32).sum::<f32>().max(1.0);
            let purity = bars.iter().map(|b| purity_of(b.quality) * b.count as f32).sum::<f32>() / total;
            let carbon_of = |b: &Item| b.make.as_ref().map(|m| m.carbon).filter(|c| *c > 0.0).unwrap_or_else(|| bar_carbon(b.info().name));
            // Сталь наваривают на лезвие: кромка берёт углерод стальной полосы.
            let max_c = bars.iter().map(carbon_of).fold(0.0f32, f32::max);
            let avg_c = bars.iter().map(|b| carbon_of(b) * b.count as f32).sum::<f32>() / total;
            let carbon = if info.harden { max_c.max(avg_c) } else { avg_c };
            for b in &bars {
                taken.push((b.info().name.to_string(), b.count));
            }
            Job::Forge(Forging::new(pattern, purity, carbon))
        }
        Plan::Cast { piece, metals } => {
            if kind != StationKind::Forge {
                return Err("Тигель ставят в горн кузницы".into());
            }
            let info = piece.info();
            if info.lost_wax && !k.knows(Lore::Technique(Technique::LostWax)) {
                return Err("Как лить по воску, вы не знаете: этому учат литейщики".into());
            }
            if count_named(inv, CRUCIBLE) == 0 {
                return Err("Нужен тигель".into());
            }
            if info.lost_wax {
                if count_named(inv, WAX) == 0 || count_named(inv, CLAY) == 0 {
                    return Err("Для литья по воску нужны воск и гончарная глина".into());
                }
            } else if count_named(inv, MOULD) == 0 {
                return Err("Нужна каменная литейная форма".into());
            }
            for (m, n) in &metals {
                if *n > 0 && count_named(inv, m.item()) < *n as u32 {
                    return Err(format!("Не хватает: {}", m.item().to_lowercase()));
                }
            }
            if metals.iter().all(|(_, n)| *n == 0) {
                return Err("Тигель пуст: положите металл".into());
            }
            let (mut cu, mut sn, mut ag, mut pb) = (0.0, 0.0, 0.0, 0.0);
            for (m, n) in metals {
                if n == 0 {
                    continue;
                }
                take_named(inv, m.item(), n as u32);
                taken.push((m.item().to_string(), n as u16));
                let d = m.drachms() * n as f32;
                match m {
                    CastMetal::Copper => cu += d,
                    CastMetal::Tin => sn += d,
                    // Старая бронза — около десятой части олова.
                    CastMetal::Bronze => {
                        cu += d * 0.9;
                        sn += d * 0.1;
                    }
                    CastMetal::Silver => ag += d,
                    CastMetal::Lead => pb += d,
                }
            }
            if info.lost_wax {
                take_named(inv, WAX, 1);
                take_named(inv, CLAY, 1);
            }
            Job::Cast(Casting::new(piece, cu, sn, ag, pb))
        }
        Plan::Smelt { ore, charcoal } => {
            if kind != StationKind::Bloomery {
                return Err("Руду плавят в сыродутном горне, а не в кузнечном".into());
            }
            if ore < 3 {
                return Err("Мало руды: заложите хотя бы три меры".into());
            }
            if count_named(inv, ORE) < ore as u32 || count_named(inv, CHARCOAL) < charcoal as u32 {
                return Err("Не хватает руды или угля".into());
            }
            take_named(inv, ORE, ore as u32);
            take_named(inv, CHARCOAL, charcoal as u32);
            Job::Soak(Soak::new(SoakKind::Bloomery { ore, charcoal }))
        }
        Plan::Carburize { bar } => {
            if kind != StationKind::Forge {
                return Err("Цементацию ведут в кузнечном горне".into());
            }
            let Some((_, it)) = inv.find(bar) else { return Err("Нет полосы".into()) };
            if it.info().name != IRON {
                return Err("Цементируют железную полосу".into());
            }
            if count_named(inv, CHARCOAL) < 2 || count_named(inv, POT) < 1 {
                return Err("Нужны две меры угля и глиняный горшок".into());
            }
            let q = it.quality;
            let Some((mut whole, _)) = inv.take(bar) else { return Err("Нет полосы".into()) };
            whole.count -= 1;
            if whole.count > 0 {
                let _ = inv.place(whole, Target::Auto);
            }
            take_named(inv, CHARCOAL, 2);
            take_named(inv, POT, 1);
            taken.push((IRON.into(), 1));
            taken.push((format!("q{}", q.index()), 0));
            Job::Soak(Soak::new(SoakKind::Carburize))
        }
        Plan::Repair { uid } => {
            if kind != StationKind::Forge {
                return Err("Чинят у кузнечного горна".into());
            }
            let Some((_, it)) = inv.find(uid) else { return Err("Нет вещи".into()) };
            let Some(m) = it.make.as_ref().filter(|m| m.dur_max > 0.0) else { return Err("Это не нужно чинить".into()) };
            if m.condition() > 0.97 {
                return Err("Вещь цела".into());
            }
            if m.flaws.contains(&Flaw::Crack) && m.broken() {
                return Err("Треснувшую и сломанную вещь не спасти — только на лом".into());
            }
            let Some((item, _)) = inv.take(uid) else { return Err("Нет вещи".into()) };
            Job::Repair { item, progress: 0.0 }
        }
        Plan::Fletch(what) => {
            use crate::archery::{GLUE, SINEW};
            if let crate::archery::Fletch::Arrows(i) = what
                && i as usize >= crate::archery::ARROWS.len()
            {
                return Err("Нет такой стрелы".into());
            }
            for (name, n) in what.inputs() {
                if count_named(inv, name) < n as u32 {
                    return Err(format!("Не хватает: {} (нужно {n})", name.to_lowercase()));
                }
            }
            let glue = count_named(inv, GLUE) > 0;
            if what.needs_glue() && !glue && count_named(inv, SINEW) == 0 {
                return Err("Нужен рыбий клей или сухожилия — приклеить перья и примотать наконечник".into());
            }
            let mut head = Quality::Common;
            for (name, n) in what.inputs() {
                let got = take_named(inv, name, n as u32);
                if crate::archery::ARROWS.iter().any(|a| a.head == name) {
                    // Худший наконечник и решает: в десятке стрел ровня по слабой.
                    head = got.iter().map(|i| i.quality).min_by_key(|q| q.index()).unwrap_or(Quality::Common);
                }
                taken.push((name.to_string(), n));
            }
            let sinew = what.needs_glue() && !glue;
            if what.needs_glue() {
                take_named(inv, if glue { GLUE } else { SINEW }, 1);
            }
            Job::Fletch { what, progress: 0.0, acc: 0.0, strokes: 0, head, sinew }
        }
    };
    let verb = match &job {
        Job::Forge(f) => format!("Работа: {}", f.pattern.name().to_lowercase()),
        Job::Cast(c) => format!("Литьё: {}", c.piece.info().product.to_lowercase()),
        Job::Soak(s) => match s.kind {
            SoakKind::Bloomery { .. } => "Горн заложен: дуйте мехами, держите жёлто-белый жар".into(),
            SoakKind::Carburize => "Горшок с полосой в горне: держите красный жар".into(),
        },
        Job::Repair { item, .. } => format!("Ремонт: {}", item.info().name.to_lowercase()),
        Job::Fletch { what, .. } => format!("Верстак: {}", what.verb().to_lowercase()),
    };
    world.works.insert(pid, Work { station, job, bellows: false, taken, since_view: 0.0 });
    Ok(verb)
}

// ---------------------------------------------------------------------------
// Приём сообщений
// ---------------------------------------------------------------------------

/// Кто сделал вещь (подпись в подсказке).
fn smith_name(pid: u64) -> String {
    format!("игрок {}", pid % 1000)
}

#[allow(clippy::too_many_arguments)]
fn receive_smith_messages(mut commands: Commands, mut server: ResMut<RenetServer>, game: Res<GameWorld>, mut world: ResMut<SmithWorld>, mut loot: ResMut<LootWorld>, mut know: ResMut<KnowledgeWorld>) {
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_SMITH) {
            let Some(msg) = decode::<SmithClientMessage>(&bytes) else { continue };
            let Some(actor) = game.actors.get(&pid).filter(|a| a.alive && !a.fighter.is_dead()) else { continue };
            let feet = actor.pose.feet;
            let busy = !matches!(actor.fighter.action, Action::Idle);
            let skill = know.players.get(&pid).map_or(0.0, |k| k.skill(Skill::Smithing));
            let world = &mut *world;
            let rng = &mut world.rng;
            // Сообщения о своей работе: стоять надо у своего горна.
            let at_station = |w: &Work, st: &HashMap<u32, Station>| st.get(&w.station).is_some_and(|s| near(feet, s));
            match msg {
                SmithClientMessage::Begin { station, plan } => {
                    if world.works.contains_key(&pid) {
                        notice(&mut server, pid, "Сначала закончите или бросьте прежнюю работу");
                        continue;
                    }
                    if busy {
                        continue;
                    }
                    let Some(st) = world.stations.get(&station) else { continue };
                    if !near(feet, st) {
                        notice(&mut server, pid, "Подойдите к горну");
                        continue;
                    }
                    let empty = Knowledge::default();
                    let k = know.players.get(&pid).unwrap_or(&empty).clone();
                    match begin(world, &mut loot, &k, pid, station, plan) {
                        Ok(verb) => notice(&mut server, pid, verb),
                        Err(e) => notice(&mut server, pid, e),
                    }
                    sync_inventory(&mut server, &loot, pid);
                    send_work(&mut server, world, pid);
                }
                SmithClientMessage::AddFuel { station } => {
                    let Some(st) = world.stations.get_mut(&station) else { continue };
                    if !near(feet, st) {
                        continue;
                    }
                    if st.fuel > FUEL_MAX - FUEL_PER_CHARCOAL * 0.5 {
                        notice(&mut server, pid, "Горн полон угля");
                        continue;
                    }
                    let Some(inv) = loot.inventories.get_mut(&pid) else { continue };
                    if count_named(inv, CHARCOAL) == 0 {
                        notice(&mut server, pid, "Нет древесного угля");
                        continue;
                    }
                    take_named(inv, CHARCOAL, 1);
                    st.fuel = (st.fuel + FUEL_PER_CHARCOAL).min(FUEL_MAX);
                    sync_inventory(&mut server, &loot, pid);
                }
                SmithClientMessage::Bellows(on) => {
                    if let Some(w) = world.works.get_mut(&pid) {
                        w.bellows = on && world.stations.get(&w.station).is_some_and(|s| near(feet, s));
                    }
                }
                SmithClientMessage::Abandon => {
                    let Some(w) = world.works.remove(&pid) else { continue };
                    abandon(&mut commands, &mut loot, pid, w, feet);
                    notice(&mut server, pid, "Работа брошена");
                    sync_inventory(&mut server, &loot, pid);
                    send_work(&mut server, world, pid);
                }
                SmithClientMessage::Finish { fitting } => {
                    let Some(w) = world.works.get(&pid) else { continue };
                    if !at_station(w, &world.stations) {
                        notice(&mut server, pid, "Подойдите к горну");
                        continue;
                    }
                    let sid = w.station;
                    let k = know.players.entry(pid).or_default();
                    match finish(&mut commands, world, &mut loot, k, pid, fitting, feet) {
                        Ok(text) => {
                            fx(&mut server, sid, pid, Fx::Done);
                            notice(&mut server, pid, text);
                            sync_knowledge(&mut server, k, pid);
                        }
                        Err(e) => notice(&mut server, pid, e),
                    }
                    sync_inventory(&mut server, &loot, pid);
                    send_work(&mut server, world, pid);
                }
                other => {
                    let stations = &world.stations;
                    let Some(w) = world.works.get_mut(&pid) else { continue };
                    if !at_station(w, stations) {
                        notice(&mut server, pid, "Вы отошли от горна");
                        continue;
                    }
                    let mut events: Vec<SmithEvent> = Vec::new();
                    let mut fxs: Vec<Fx> = Vec::new();
                    match (&mut w.job, other) {
                        (Job::Forge(f), SmithClientMessage::Move(spot)) => {
                            if spot == Spot::Ashes && f.temp < 650.0 {
                                events.push(SmithEvent::Note("Отжигают с красного жара: сначала нагрейте".into()));
                            } else {
                                f.spot = spot;
                            }
                        }
                        (Job::Forge(f), SmithClientMessage::Strike { zone, heavy, acc }) => {
                            if f.since_strike >= STRIKE_GAP[heavy as usize] * 0.85 && f.spot == Spot::Anvil {
                                fxs.push(Fx::Strike { heavy, temp: f.temp });
                                events.extend(f.strike(zone as usize, heavy, acc, skill, rng));
                            }
                        }
                        (Job::Forge(f), SmithClientMessage::Flip) => {
                            if f.spot == Spot::Anvil {
                                f.flip();
                            }
                        }
                        (Job::Forge(f), SmithClientMessage::Quench(q)) => {
                            if f.spot == Spot::Fire {
                                f.spot = Spot::Anvil;
                            }
                            if q == Quench::Oil && count_named(loot.inventories.get(&pid).unwrap_or(&Inventory::new((1, 1))), "Кувшин оливкового масла") == 0 {
                                events.push(SmithEvent::Note("Для закалки в масле нужен кувшин оливкового масла".into()));
                            } else if q == Quench::Brine && count_named(loot.inventories.get(&pid).unwrap_or(&Inventory::new((1, 1))), "Мешок соли") == 0 {
                                events.push(SmithEvent::Note("Для рассола нужен мешок соли".into()));
                            } else {
                                let temp = f.temp;
                                let e = f.quench(q, skill, rng);
                                let cracked = matches!(e, SmithEvent::Quenched { cracked: true, .. });
                                fxs.push(Fx::Quench { medium: q, temp, cracked });
                                events.push(e);
                            }
                        }
                        (Job::Forge(f), SmithClientMessage::Grind { acc }) => {
                            if f.spot == Spot::Grindstone {
                                if f.temp > 250.0 {
                                    events.push(SmithEvent::Note("Горячее на точило не кладут".into()));
                                } else {
                                    f.grind(acc);
                                    fxs.push(Fx::Grind);
                                }
                            }
                        }
                        (Job::Cast(c), SmithClientMessage::Move(spot)) => {
                            if c.pouring {
                                continue;
                            }
                            c.spot = if spot == Spot::Fire { Spot::Fire } else { Spot::Anvil };
                        }
                        (Job::Cast(c), SmithClientMessage::Skim) => {
                            c.skim();
                            fxs.push(Fx::Skim);
                        }
                        (Job::Cast(c), SmithClientMessage::WarmMould(on)) => c.mould_warm = on,
                        (Job::Cast(c), SmithClientMessage::Pour(on)) => {
                            if on {
                                if c.spot == Spot::Fire {
                                    c.spot = Spot::Anvil;
                                }
                                match c.start_pour() {
                                    Err(e) => events.push(SmithEvent::Note(e)),
                                    Ok(()) => fxs.push(Fx::Pour(true)),
                                }
                            } else if c.pouring {
                                c.pouring = false;
                                fxs.push(Fx::Pour(false));
                            }
                        }
                        (Job::Cast(c), SmithClientMessage::OpenMould) => {
                            if c.poured <= 0.0 {
                                events.push(SmithEvent::Note("Сначала залейте форму".into()));
                            } else if c.temp > c.liquidus() - 150.0 && c.temp > 200.0 {
                                events.push(SmithEvent::Note("Металл ещё жидкий: подождите, пока застынет".into()));
                            } else {
                                c.opened = true;
                                c.pouring = false;
                                fxs.push(Fx::OpenMould);
                            }
                        }
                        (Job::Cast(c), SmithClientMessage::Strike { acc, heavy, .. }) => {
                            if c.opened {
                                c.chase(acc, heavy);
                                fxs.push(Fx::Strike { heavy, temp: c.temp });
                            }
                        }
                        (Job::Cast(c), SmithClientMessage::Grind { acc }) => {
                            if c.opened {
                                c.chase(acc * 0.9, false);
                                fxs.push(Fx::Grind);
                            }
                        }
                        (Job::Repair { progress, .. }, SmithClientMessage::Strike { acc, heavy, .. }) => {
                            fxs.push(Fx::Strike { heavy, temp: 20.0 });
                            *progress += 0.1 * (0.3 + 0.7 * acc.clamp(0.0, 1.0)) * if heavy { 0.6 } else { 1.0 };
                        }
                        (Job::Repair { progress, .. }, SmithClientMessage::Grind { acc }) => {
                            fxs.push(Fx::Grind);
                            *progress += 0.1 * (0.3 + 0.7 * acc.clamp(0.0, 1.0));
                        }
                        (Job::Fletch { what, progress, acc: avg, strokes, .. }, SmithClientMessage::Grind { acc }) => {
                            let acc = acc.clamp(0.0, 1.0);
                            *avg = (*avg * *strokes as f32 + acc) / (*strokes as f32 + 1.0);
                            *strokes += 1;
                            *progress += 0.1 * (0.4 + 0.6 * acc) / what.work();
                        }
                        (_, _) => {}
                    }
                    let sid = w.station;
                    for e in events {
                        if e == SmithEvent::Crack {
                            fxs.push(Fx::Crack);
                        }
                        notice(&mut server, pid, e.text());
                    }
                    for f in fxs {
                        fx(&mut server, sid, pid, f);
                    }
                    // Ремонт закончен сам собой.
                    if let Job::Repair { progress, .. } = &w.job
                        && *progress >= 1.0
                        && let Some(Work { job: Job::Repair { mut item, .. }, .. }) = world.works.remove(&pid)
                    {
                        if let Some(m) = item.make.as_mut() {
                            repair(m, skill);
                        }
                        let name = item.info().name;
                        if let Some(inv) = loot.inventories.get_mut(&pid) {
                            give(&mut commands, inv, item, feet);
                        }
                        if let Some(k) = know.players.get_mut(&pid) {
                            k.practice(Skill::Smithing, 0.6);
                            sync_knowledge(&mut server, k, pid);
                        }
                        notice(&mut server, pid, format!("Выправлено и наточено: {}", name.to_lowercase()));
                        sync_inventory(&mut server, &loot, pid);
                    }
                    // Верстак: работа готова.
                    let fletched = matches!(world.works.get(&pid).map(|w| &w.job), Some(Job::Fletch { progress, .. }) if *progress >= 1.0);
                    if fletched
                        && let Some(Work { job: Job::Fletch { what, acc, head, sinew, .. }, .. }) = world.works.remove(&pid)
                    {
                        let (name, n) = what.product();
                        let q = match what {
                            crate::archery::Fletch::Shafts => crate::smithing::quality_of(0.35 + acc * 0.4 + skill / 100.0 * 0.25),
                            crate::archery::Fletch::Arrows(_) => crate::archery::assembled_quality(head, skill, acc, sinew),
                        };
                        let quality = if crate::items::ItemBaseId::find(name).is_some_and(|b| b.base().has_quality) { q } else { Quality::Common };
                        give_named(&mut commands, &mut loot, pid, name, quality, n, feet);
                        if let Some(k) = know.players.get_mut(&pid) {
                            k.practice(Skill::Smithing, 0.3);
                            sync_knowledge(&mut server, k, pid);
                        }
                        notice(&mut server, pid, format!("Готово: {} ×{n}", name.to_lowercase()));
                        sync_inventory(&mut server, &loot, pid);
                    }
                    send_work(&mut server, world, pid);
                }
            }
        }
    }
}

/// Бросить работу: металл не пропадает весь — он идёт в лом.
fn abandon(commands: &mut Commands, loot: &mut LootWorld, pid: u64, w: Work, feet: Vec3) {
    match w.job {
        Job::Forge(f) => {
            let n = f.pattern.info().metal.max(1) as u16;
            give_named(commands, loot, pid, SCRAP, Quality::Common, n, feet);
        }
        Job::Cast(c) => {
            // Застывший металл тигля — бронзовый, серебряный или свинцовый лом.
            let total = c.total();
            let (name, unit) = if c.lead > total * 0.5 {
                ("Свинец", CastMetal::Lead.drachms())
            } else if c.silver > total * 0.5 {
                ("Серебряный лом", CastMetal::Silver.drachms())
            } else if c.tin <= 0.0 {
                ("Медный лом", CastMetal::Copper.drachms())
            } else {
                ("Бронзовый лом", CastMetal::Bronze.drachms())
            };
            let n = (total / unit).floor() as u16;
            if n > 0 {
                give_named(commands, loot, pid, name, Quality::Common, n, feet);
            }
        }
        Job::Soak(_) => {}
        // Со верстака всё возвращается, кроме клея.
        Job::Fletch { what, head, .. } => {
            for (name, n) in what.inputs() {
                let q = if crate::archery::ARROWS.iter().any(|a| a.head == name) { head } else { Quality::Common };
                give_named(commands, loot, pid, name, q, n, feet);
            }
        }
        Job::Repair { item, .. } => {
            if let Some(inv) = loot.inventories.get_mut(&pid) {
                give(commands, inv, item, feet);
            }
        }
    }
    let _ = w.taken;
}

/// Понял на опыте: удачная работа иногда сама учит приёму.
fn learn_by_doing(k: &mut Knowledge, t: Technique, chance: f32, rng: &mut Rng, msgs: &mut Vec<String>) {
    let lore = Lore::Technique(t);
    if !k.knows(lore) && k.prerequisites_met(lore) && rng.f32() < chance {
        k.learn(lore);
        msgs.push(format!("На опыте вы поняли приём: {}", t.name().to_lowercase()));
    }
}

fn finish(commands: &mut Commands, world: &mut SmithWorld, loot: &mut LootWorld, k: &mut Knowledge, pid: u64, fitting: bool, feet: Vec3) -> Result<String, String> {
    let w = world.works.get(&pid).ok_or("Нет работы")?;
    let rng = &mut world.rng;
    let mut msgs = Vec::new();
    match &w.job {
        Job::Forge(f) => {
            let info = f.pattern.info();
            if f.weld < 1.0 {
                return Err("Пакет не сварен: так он развалится".into());
            }
            if !f.shaped() {
                return Err("Поковка ещё не готова: доковать участки".into());
            }
            if f.temp > 200.0 {
                return Err("Горячее: дайте остыть".into());
            }
            let inv = loot.inventories.get_mut(&pid).ok_or("Нет сумки")?;
            for (part, n) in info.parts {
                if count_named(inv, part) < *n as u32 {
                    return Err(format!("Для сборки нужно: {} ×{n}", part.to_lowercase()));
                }
            }
            let fitting = fitting && info.kind == PieceKind::Weapon && count_named(inv, POMMEL) > 0;
            for (part, n) in info.parts {
                take_named(inv, part, *n as u32);
            }
            if fitting {
                take_named(inv, POMMEL, 1);
            }
            let outcome = f.outcome(&smith_name(pid), fitting);
            let f = f.clone();
            world.works.remove(&pid);
            // Навык: чем сложнее и лучше вещь, тем больше опыта; неудача тоже учит.
            k.practice(Skill::Smithing, 1.0 + info.metal as f32 * 0.6 + outcome.score * 2.5);
            if outcome.score > 0.45 {
                learn_by_doing(k, Technique::Forging, 0.35, rng, &mut msgs);
            }
            if info.weld && f.cold_shut < 0.1 {
                learn_by_doing(k, Technique::ForgeWelding, 0.25, rng, &mut msgs);
            }
            if info.harden && f.quenched && f.hardness > 0.5 && !f.make_cracked() {
                learn_by_doing(k, Technique::Quenching, 0.3, rng, &mut msgs);
            }
            if info.harden && f.temper > 220.0 && f.brittle < 0.3 {
                learn_by_doing(k, Technique::Tempering, 0.3, rng, &mut msgs);
            }
            if info.kind == PieceKind::Stock {
                // Крица и лом дают полосы; сталь — если в крице много углерода.
                let q = outcome.quality.min(Quality::Epic);
                let steel = f.pattern == Pattern::Bloom && f.carbon > 0.45;
                if steel {
                    give_named(commands, loot, pid, STEEL, q, 1, feet);
                    give_named(commands, loot, pid, IRON, q, info.count.saturating_sub(1).max(1), feet);
                } else {
                    give_named(commands, loot, pid, IRON, q, info.count, feet);
                }
                msgs.insert(0, format!("Готово: {} ({}){}", if steel { "стальная и железная полосы" } else { "железные полосы" }, outcome.quality.name().to_lowercase(), if steel { " — в крице была сталь" } else { "" }));
            } else {
                let base = ItemBaseId::find(info.product).ok_or("Нет такого предмета")?;
                let affixes = crafted_affixes(info.kind, &outcome.make, outcome.quality);
                let make = if crate::items::base_durability(base.base()) > 0.0 || info.kind == PieceKind::Weapon { Some(Box::new(outcome.make.clone())) } else { None };
                let mut item = plain_item(base, outcome.quality, info.count, &mut loot.uids);
                item.affixes = affixes;
                item.make = make;
                if let Some(inv) = loot.inventories.get_mut(&pid) {
                    give(commands, inv, item, feet);
                }
                let flaws: Vec<&str> = outcome.make.flaws.iter().map(|f| f.name()).collect();
                let tail = if flaws.is_empty() { String::new() } else { format!(". Изъяны: {}", flaws.join(", ")) };
                msgs.insert(0, format!("Готово: {} — {}{tail}", info.product.to_lowercase(), outcome.quality.name().to_lowercase()));
            }
        }
        Job::Cast(c) => {
            if !c.opened {
                return Err("Сначала раскройте форму".into());
            }
            let info = c.piece.info();
            let outcome = c.outcome(&smith_name(pid));
            let left = (c.total() * c.melt - info.drachms).max(0.0);
            let unit = match info.alloy {
                Alloy::Bronze => ("Бронзовый лом", CastMetal::Bronze.drachms()),
                Alloy::Silver => ("Серебряный лом", CastMetal::Silver.drachms()),
                Alloy::Lead => ("Свинец", CastMetal::Lead.drachms()),
            };
            let score = outcome.score;
            world.works.remove(&pid);
            let base = ItemBaseId::find(info.product).ok_or("Нет такого предмета")?;
            let mut item = plain_item(base, outcome.quality, info.count, &mut loot.uids);
            if base.base().slot.is_some() {
                // Крест и перстень: свойства — от красоты работы.
                let q = outcome.quality.index() as f32;
                if q >= 2.0 {
                    item.affixes.push((crate::items::Stat::Luck, (q * 3.0).round()));
                }
                item.make = Some(Box::new(outcome.make));
            }
            if let Some(inv) = loot.inventories.get_mut(&pid) {
                give(commands, inv, item, feet);
            }
            let rest = (left / unit.1).floor() as u16;
            if rest > 0 {
                give_named(commands, loot, pid, unit.0, Quality::Common, rest, feet);
            }
            k.practice(Skill::Smithing, 1.0 + score * 2.0);
            if score > 0.5 {
                learn_by_doing(k, Technique::Casting, 0.3, rng, &mut msgs);
            }
            msgs.insert(0, format!("Отливка готова: {} — {}", info.product.to_lowercase(), outcome.quality.name().to_lowercase()));
        }
        Job::Soak(s) => {
            if !s.done() {
                return Err(format!("Ещё не готово: осталось {}", crate::lore::fmt_secs(s.total - s.elapsed)));
            }
            let s = s.clone();
            let taken = w.taken.clone();
            world.works.remove(&pid);
            match s.kind {
                SoakKind::Bloomery { .. } => {
                    let (blooms, q, carbon, cast) = bloomery_yield(&s);
                    for _ in 0..blooms {
                        let base = ItemBaseId::by_name(BLOOM);
                        let mut item = plain_item(base, q, 1, &mut loot.uids);
                        item.make = Some(Box::new(Make { smith: smith_name(pid), finish: 0.0, edge: 0.0, balance: 0.0, flaws: Vec::new(), dur: 0.0, dur_max: 0.0, carbon: (carbon * (0.8 + rng.f32() * 0.4)).clamp(0.03, 1.6) }));
                        if let Some(inv) = loot.inventories.get_mut(&pid) {
                            give(commands, inv, item, feet);
                        }
                    }
                    if cast > 0 {
                        give_named(commands, loot, pid, CAST_IRON, Quality::Common, cast, feet);
                    }
                    k.practice(Skill::Smithing, 1.5 + blooms as f32);
                    if blooms > 0 {
                        learn_by_doing(k, Technique::Smelting, 0.3, rng, &mut msgs);
                    }
                    msgs.insert(
                        0,
                        match (blooms, cast) {
                            (0, 0) => "Горн разломали: внутри только шлак и недовосстановленная руда".into(),
                            (0, _) => "Вместо крицы — хрупкий чугун: его не выковать".into(),
                            (n, 0) => format!("Из горна вынули крицу: {n} шт. ({})", q.name().to_lowercase()),
                            (n, c) => format!("Крица: {n} шт., и ещё {c} шт. чугуна от перегрева"),
                        },
                    );
                }
                SoakKind::Carburize => {
                    let bar_q = taken.iter().find_map(|(n, _)| n.strip_prefix('q').and_then(|d| d.parse::<usize>().ok())).map(|i| Quality::ALL[i.min(5)]).unwrap_or(Quality::Common);
                    match carburize_yield(&s, bar_q) {
                        Some(q) => {
                            give_named(commands, loot, pid, STEEL, q, 1, feet);
                            k.practice(Skill::Smithing, 2.0);
                            learn_by_doing(k, Technique::Carburizing, 0.35, rng, &mut msgs);
                            msgs.insert(0, format!("Полоса набрала углерода: стальная полоса ({})", q.name().to_lowercase()));
                        }
                        None if s.over >= 0.3 => {
                            give_named(commands, loot, pid, SCRAP, Quality::Common, 1, feet);
                            msgs.insert(0, "Горшок лопнул, полоса обгорела — остался лом".into());
                        }
                        None => {
                            give_named(commands, loot, pid, IRON, bar_q, 1, feet);
                            msgs.insert(0, "Жара было мало: полоса осталась мягким железом".into());
                        }
                    }
                }
            }
        }
        Job::Repair { .. } => return Err("Доправьте вещь на наковальне или точиле".into()),
        Job::Fletch { .. } => return Err("Доделайте работу на верстаке".into()),
    }
    Ok(msgs.join(". "))
}

trait Cracked {
    fn make_cracked(&self) -> bool;
}

impl Cracked for Forging {
    fn make_cracked(&self) -> bool {
        self.cracks > 0.1
    }
}

// ---------------------------------------------------------------------------
// Время
// ---------------------------------------------------------------------------

fn tick_works(time: Res<Time>, mut server: ResMut<RenetServer>, game: Res<GameWorld>, mut world: ResMut<SmithWorld>) {
    let dt = time.delta_secs().min(0.1);
    let world = &mut *world;
    // Мехи у горна: дует любой, кто качает.
    let mut blowing: HashMap<u32, bool> = HashMap::new();
    let mut burning: HashMap<u32, bool> = HashMap::new();
    for (pid, w) in world.works.iter_mut() {
        let here = game.actors.get(pid).is_some_and(|a| a.alive && world.stations.get(&w.station).is_some_and(|s| near(a.pose.feet, s)));
        if !here {
            w.bellows = false;
        }
        if w.bellows {
            blowing.insert(w.station, true);
        }
        let hot = match &w.job {
            Job::Forge(f) => matches!(f.spot, Spot::Fire | Spot::Temper),
            Job::Cast(c) => c.spot == Spot::Fire,
            Job::Soak(s) => matches!(s.kind, SoakKind::Carburize) && !s.done(),
            Job::Repair { .. } | Job::Fletch { .. } => false,
        };
        if hot {
            burning.insert(w.station, true);
        }
    }
    for (id, st) in world.stations.iter_mut() {
        let blow = blowing.contains_key(id) && st.fuel > 0.0;
        st.blast = step_blast(st.blast, blow, dt);
        if burning.contains_key(id) || st.blast > 0.05 {
            st.fuel = (st.fuel - dt * (0.6 + st.blast * 1.4)).max(0.0);
        }
    }
    let mut notes: Vec<(u64, String)> = Vec::new();
    let mut burns: Vec<(u32, u64)> = Vec::new();
    for (pid, w) in world.works.iter_mut() {
        let Some(st) = world.stations.get(&w.station) else { continue };
        let fire = fire_temp(st.fuel, st.blast);
        let events = match &mut w.job {
            Job::Forge(f) => f.step(dt, fire),
            Job::Cast(c) => c.step(dt, fire),
            Job::Soak(s) => {
                s.bellows = w.bellows;
                s.step(dt, st.fuel > 0.0)
            }
            Job::Repair { .. } | Job::Fletch { .. } => Vec::new(),
        };
        for e in events {
            if e == SmithEvent::Sparks {
                burns.push((w.station, *pid));
            }
            notes.push((*pid, e.text()));
        }
        // Пока что-то греется или остывает — снимки чаще.
        w.since_view += dt;
        let lively = match &w.job {
            Job::Forge(f) => f.temp > 60.0 || f.spot == Spot::Fire,
            Job::Cast(c) => c.temp > 60.0 || c.spot == Spot::Fire || c.pouring,
            Job::Soak(s) => !s.done() || s.temp > 60.0,
            Job::Repair { .. } | Job::Fletch { .. } => false,
        };
        if lively && w.since_view > 0.12 {
            w.since_view = 0.0;
            let view = WorkView { station: w.station, job: w.job.clone(), bellows: w.bellows };
            server.send_message(*pid, CH_SMITH, encode(&SmithServerMessage::Work(Some(view))));
        }
    }
    for (pid, text) in notes {
        notice(&mut server, pid, text);
    }
    for (sid, pid) in burns {
        fx(&mut server, sid, pid, Fx::Burn);
    }
    // Жар горна — всем (огонь и дым видно издали), при заметной перемене.
    for (id, st) in world.stations.iter_mut() {
        if (st.fuel - st.sent.0).abs() > 2.0 || (st.blast - st.sent.1).abs() > 0.03 || (st.fuel <= 0.0) != (st.sent.0 <= 0.0) {
            st.sent = (st.fuel, st.blast);
            server.broadcast_message(CH_SMITH, encode(&SmithServerMessage::Fire { station: *id, fuel: st.fuel, blast: st.blast }));
        }
    }
    // Раскалённое железо у всех горнов — 5 раз в секунду, пока оно есть.
    world.pieces_t += dt;
    if world.pieces_t >= 0.2 {
        world.pieces_t = 0.0;
        let v: Vec<PieceView> = world.works.iter().filter_map(|(p, w)| piece_view(*p, w)).collect();
        if !v.is_empty() || world.pieces_any {
            world.pieces_any = !v.is_empty();
            server.broadcast_message(CH_SMITH, encode(&SmithServerMessage::Pieces(v)));
        }
    }
}

// ---------------------------------------------------------------------------
// Износ в бою
// ---------------------------------------------------------------------------

/// Что изнашивается от удара.
#[derive(Debug, Clone, Copy)]
pub enum Wear {
    /// Оружие в правой руке (рубил, парировал).
    Weapon(f32),
    /// Щит.
    Shield(f32),
    /// Доспех, по которому пришёлся удар.
    Armour { head: bool, amount: f32 },
    /// Выпущена стрела из этой стопки.
    Ammo(ItemUid),
    /// Тетива натянута, а стрел в сумке нет.
    NoAmmo,
}

fn wear_item(item: &mut Item, amount: f32) -> Option<bool> {
    let m = item.make.as_mut()?;
    if m.dur_max <= 0.0 || m.dur <= 0.0 {
        return None;
    }
    let mut k = 1.0;
    if m.flaws.contains(&Flaw::Brittle) {
        k *= 1.8;
    }
    if m.flaws.contains(&Flaw::Soft) {
        k *= 1.4;
    }
    if m.flaws.contains(&Flaw::Crack) {
        k *= 1.5;
    }
    let before = m.dur;
    m.dur = (m.dur - amount * k).max(0.0);
    // Сообщать клиенту не каждый удар: на каждой пятой единице и при поломке.
    let step = (before / 5.0).floor() != (m.dur / 5.0).floor();
    Some(step || m.dur <= 0.0)
}

fn apply_wear(mut server: ResMut<RenetServer>, mut game: ResMut<GameWorld>, mut loot: ResMut<LootWorld>) {
    let wear = std::mem::take(&mut game.gear_wear);
    let mut dirty: Vec<u64> = Vec::new();
    for (pid, w) in wear {
        let Some(inv) = loot.inventories.get_mut(&pid) else { continue };
        let slots: Vec<(EquipSlot, f32)> = match w {
            Wear::Ammo(uid) => {
                if let Some(stack) = inv.find_mut(uid) {
                    stack.count = stack.count.saturating_sub(1);
                    if stack.count == 0 {
                        inv.take(uid);
                        notice(&mut server, pid, "Это была последняя стрела этого вида");
                    }
                }
                if !dirty.contains(&pid) {
                    dirty.push(pid);
                }
                continue;
            }
            Wear::NoAmmo => {
                notice(&mut server, pid, "Нет стрел: соберите их у кузни или купите");
                continue;
            }
            Wear::Weapon(a) => vec![(EquipSlot::MainHand, a)],
            Wear::Shield(a) => vec![(EquipSlot::OffHand, a)],
            Wear::Armour { head: true, amount } => vec![(EquipSlot::Head, amount)],
            Wear::Armour { head: false, amount } => vec![(EquipSlot::Chest, amount), (EquipSlot::Hands, amount * 0.4), (EquipSlot::Legs, amount * 0.4)],
        };
        for (slot, amount) in slots {
            let Some(item) = inv.equipment[slot.index()].as_mut() else { continue };
            if let Some(changed) = wear_item(item, amount) {
                if item.make.as_ref().is_some_and(|m| m.broken()) {
                    notice(&mut server, pid, format!("Сломалось: {}. Почините у кузнеца", item.info().name.to_lowercase()));
                }
                if changed && !dirty.contains(&pid) {
                    dirty.push(pid);
                }
            }
        }
    }
    for pid in dirty {
        sync_inventory(&mut server, &loot, pid);
    }
}

// ---------------------------------------------------------------------------
// Отладка
// ---------------------------------------------------------------------------

/// `MRPG_SMITH_KIT=1` — каждый вошедший получает полосы, уголь, руду, металл для литья и навык.
fn smith_kit(mut server: ResMut<RenetServer>, mut loot: ResMut<LootWorld>, mut know: ResMut<KnowledgeWorld>, mut given: Local<std::collections::HashSet<u64>>) {
    if std::env::var("MRPG_SMITH_KIT").is_err() {
        return;
    }
    let ids: Vec<u64> = loot.inventories.keys().copied().filter(|id| !given.contains(id) && know.players.contains_key(id)).collect();
    for id in ids {
        given.insert(id);
        let kit: [(&str, Quality, u16); 12] = [
            (IRON, Quality::Uncommon, 3),
            (STEEL, Quality::Rare, 1),
            (CHARCOAL, Quality::Common, 8),
            (ORE, Quality::Common, 6),
            (ASH, Quality::Common, 2),
            (HIDE, Quality::Common, 1),
            (CRUCIBLE, Quality::Common, 1),
            (MOULD, Quality::Common, 1),
            ("Медный лом", Quality::Common, 3),
            ("Олово", Quality::Common, 2),
            ("Кувшин оливкового масла", Quality::Common, 1),
            (POMMEL, Quality::Uncommon, 1),
        ];
        for (name, q, n) in kit {
            let Some(base) = ItemBaseId::find(name) else { continue };
            let item = plain_item(base, q, n, &mut loot.uids);
            if let Some(inv) = loot.inventories.get_mut(&id) {
                let _ = inv.place(item, Target::Auto);
            }
        }
        if let Some(k) = know.players.get_mut(&id) {
            k.skills[Skill::Smithing.index()] = 30.0;
            k.learn(Lore::Technique(Technique::Forging));
            sync_knowledge(&mut server, k, id);
        }
        sync_inventory(&mut server, &loot, id);
    }
}
