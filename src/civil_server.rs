//! Сервер: мирные люди и закон.
//!
//! Жители и торговцы смертны. Нападение или убийство в городе — преступление, но лишь если
//! кто-то видел: свидетель бежит к страже (несколько секунд, за которые его ещё можно
//! заставить замолчать), а солдат гарнизона, видевший всё сам, знает сразу. Донесённое
//! преступление — пеня (общая с кражами, см. `TradeWorld::fine`) и удар по репутации.
//! Разыскиваемого стражник останавливает: уплатить на месте, сдаться (карцер претории)
//! или сопротивляться — тогда гарнизон берётся за оружие.

use crate::civilians::*;
use crate::collision::LevelColliders;
use crate::combat::Pose;
use crate::factions::Faction;
use crate::level::{Resident, Role};
use crate::npc::Rng;
use crate::protocol::*;
use crate::server::{CivCorpse, GameWorld};
use crate::trade_server::TradeWorld;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::{HashMap, HashSet};

/// Мирные люди в списках целей (стрелы, «уже задет этим ударом») получают id с этим смещением.
pub const CIV_ID_BASE: u64 = 1 << 60;
/// Через сколько в дом погибшего въезжает другой человек.
const RESIDENT_RETURN_SECS: f64 = 20.0 * 60.0;
/// Через сколько лавку убитого открывает наследник.
const HEIR_SECS: f64 = 15.0 * 60.0;
/// Сколько свидетель бежит до стражи.
const REPORT_DELAY: f64 = 6.0;
const WITNESS_RANGE: f32 = 20.0;
const GUARD_SIGHT: f32 = 25.0;
/// С какого расстояния стражник замечает разыскиваемого и идёт к нему.
const ARREST_SPOT: f32 = 14.0;
/// Стражник подошёл и заговорил.
const HALT_DIST: f32 = 2.8;
/// Столько ждёт ответа.
const HALT_TIMEOUT: f64 = 30.0;
/// Ушёл от стражника во время разговора — сопротивление.
const WALK_AWAY: f32 = 9.0;
/// Повторное нападение на того же человека в это время — то же преступление.
const SAME_CRIME_SECS: f64 = 20.0;

pub struct CivilServerPlugin;

impl Plugin for CivilServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_connect).add_systems(Update, civil_tick);
    }
}

/// Мирный человек.
pub struct Person {
    pub civ: Civ,
    pub pos: Vec3,
    pub yaw: f32,
    pub faction: Faction,
    pub name: String,
    pub max_hp: f32,
    pub hp: f32,
    /// Житель (у торговца — нет).
    pub role: Option<Role>,
    /// Погиб; на его место придут в это время.
    pub dead_until: Option<f64>,
}

impl Person {
    pub fn alive(&self) -> bool {
        self.dead_until.is_none()
    }

    fn vulnerable(&self) -> bool {
        self.alive() && self.role.is_none_or(vulnerable)
    }

    pub fn pose(&self) -> Pose {
        Pose { feet: self.pos, yaw: self.yaw, pitch: 0.0, crouching: false }
    }

    fn eye(&self) -> Vec3 {
        // Чуть выше своей коробки-тела, чтобы она не заслоняла взгляд.
        self.pos + Vec3::Y * 1.85
    }
}

/// Игрок напал на человека.
pub struct Offense {
    pub player: u64,
    pub victim: Victim,
    pub faction: Faction,
    pub killed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Victim {
    /// Мирный (номер в `CivilWorld::people`).
    Civ(usize),
    /// Боец со своим ИИ (стражник, солдат).
    Actor(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Witness {
    Civ(usize),
    Actor(u64),
}

struct Report {
    player: u64,
    crime: Violence,
    faction: Faction,
    victim: Victim,
    witnesses: Vec<Witness>,
    due: f64,
}

struct Arrest {
    guard: u64,
    since: f64,
    halted: bool,
}

#[derive(Resource)]
pub struct CivilWorld {
    pub people: Vec<Person>,
    merchants_added: bool,
    /// Нападения за кадр (кладёт бой, разбирает `civil_tick`).
    pub queue: Vec<Offense>,
    /// Ответы игроков стражникам.
    pub answers: Vec<(u64, LawAnswer)>,
    reports: Vec<Report>,
    arrests: HashMap<u64, Arrest>,
    /// Кто сидит в карцере и до какого времени.
    jailed: HashMap<u64, f64>,
    /// (игрок, жертва) -> когда было последнее нападение.
    recent: HashMap<(u64, u64), f64>,
    /// За что разыскивается (для разговора со стражей).
    crimes: HashMap<u64, Vec<String>>,
    /// Оказали сопротивление: гарнизон дерётся, пока пеня не уплачена.
    law_provoked: HashSet<u64>,
    jail: Option<Vec3>,
    rng: Rng,
    /// Где били людей за кадр: жители вокруг пугаются (см. `daily_life::Fright`).
    pub alarms: Vec<Vec3>,
}

impl CivilWorld {
    pub fn new(residents: &[Resident]) -> Self {
        let people = residents
            .iter()
            .enumerate()
            .map(|(i, r)| Person {
                civ: Civ::Resident(i as u16),
                pos: r.pos,
                yaw: r.yaw,
                faction: role_faction(r.role),
                name: crate::residents::title(r.role).to_string(),
                max_hp: resident_hp(r.role),
                hp: resident_hp(r.role),
                role: Some(r.role),
                dead_until: None,
            })
            .collect();
        Self {
            people,
            merchants_added: false,
            queue: Vec::new(),
            answers: Vec::new(),
            reports: Vec::new(),
            arrests: HashMap::new(),
            jailed: HashMap::new(),
            recent: HashMap::new(),
            crimes: HashMap::new(),
            law_provoked: HashSet::new(),
            jail: None,
            rng: Rng::new(0x5EED_C1F1),
            alarms: Vec::new(),
        }
    }

    /// Торговцы появляются у сервера торговли при старте; добавляем их, как только он готов.
    pub fn add_merchants(&mut self, trade: &TradeWorld) {
        if self.merchants_added {
            return;
        }
        self.merchants_added = true;
        for m in &trade.merchants {
            let def = m.id.def();
            self.people.push(Person {
                civ: Civ::Merchant(m.id.0),
                pos: m.pos,
                yaw: m.yaw,
                faction: def.faction,
                name: format!("{}, {}", def.name, def.title),
                max_hp: MERCHANT_HP,
                hp: MERCHANT_HP,
                role: None,
                dead_until: None,
            });
        }
    }

    /// Живые мирные (кроме детей) в радиусе `r`: номер и поза.
    pub fn targets_near(&self, p: Vec3, r: f32) -> Vec<(usize, Pose)> {
        let r2 = r * r;
        self.people.iter().enumerate().filter(|(_, c)| c.vulnerable() && c.pos.distance_squared(p) < r2).map(|(i, c)| (i, c.pose())).collect()
    }

    pub fn jailed(&self, player: u64) -> bool {
        self.jailed.contains_key(&player)
    }
}

/// Прямая видимость с учётом стен, но не людей (люди не загораживают вид друг на друга).
pub fn walls_clear(level: &LevelColliders, from: Vec3, to: Vec3) -> bool {
    let d = to - from;
    let steps = (d.length() / 0.2).ceil().max(1.0) as usize;
    (1..steps).all(|i| {
        let p = from + d * (i as f32 / steps as f32);
        p.y >= level.ground_height(p.x, p.z) && level.point_in_wall(p).is_none()
    })
}

fn send_law(server: &mut RenetServer, player: u64, msg: LawMessage) {
    server.send_message(player, CH_RELIABLE, encode(&ServerMessage::Law(msg)));
}

/// Удар по мирному человеку (клинком или стрелой).
#[allow(clippy::too_many_arguments)]
pub fn hurt(
    server: &mut RenetServer,
    world: &mut GameWorld,
    civil: &mut CivilWorld,
    level: &LevelColliders,
    trade: Option<&mut TradeWorld>,
    person: usize,
    attacker: u64,
    damage: f32,
    point: Vec3,
    head: bool,
    now: f64,
) {
    let Some(p) = civil.people.get_mut(person) else { return };
    if !p.vulnerable() {
        return;
    }
    p.hp = (p.hp - damage).max(0.0);
    let killed = p.hp <= 0.0;
    let (civ, pos, yaw, faction) = (p.civ, p.pos, p.yaw, p.faction);
    civil.alarms.push(point);
    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CivilianHit { civ, at: point.into(), amount: damage, head }));
    civil.queue.push(Offense { player: attacker, victim: Victim::Civ(person), faction, killed });
    let mut trade = trade;
    // За стенами суда нет: свои запомнят и отомстят.
    if faction.outside_walls()
        && let Some(t) = trade.as_deref_mut()
    {
        t.change_rep(attacker, faction, if killed { -25.0 } else { -5.0 });
    }
    if !killed {
        return;
    }
    let p = &mut civil.people[person];
    let (table, purse, back) = match (p.role, civ) {
        (Some(role), _) => (role_loot(role), role_purse(role), RESIDENT_RETURN_SECS),
        (None, Civ::Merchant(id)) => {
            // В кошеле торговца — часть кассы.
            let cash = trade.as_deref_mut().map_or(0, |t| {
                let m = &mut t.merchants[id as usize];
                m.dead = true;
                let take = m.cash / 4;
                m.cash -= take;
                take
            });
            (crate::items::LootTableId::Wealthy, (cash, cash), HEIR_SECS)
        }
        _ => (crate::items::LootTableId::Household, (0, 0), RESIDENT_RETURN_SECS),
    };
    p.dead_until = Some(now + back);
    let name = format!("Тело: {}", p.name);
    if let Civ::Resident(i) = civ {
        level.set_body_gone(i as usize, true);
    }
    let coins = if purse.1 > purse.0 { civil.rng.range(purse.0 as f32, purse.1 as f32) as u32 } else { purse.0 };
    info!("Мирный {civ:?} убит игроком {attacker}");
    crate::server::add_civ_corpse(server, world, Some(attacker), pos, yaw, CivCorpse { civ, name, table, coins });
}

/// Кто видел игрока в точке `at` (кроме убитой жертвы).
fn witnesses(civil: &CivilWorld, world: &GameWorld, level: &LevelColliders, player: u64, victim: Victim, killed: bool) -> Vec<Witness> {
    let Some(me) = world.actors.get(&player) else { return Vec::new() };
    let target = me.pose.shoulder();
    let mut out = Vec::new();
    let r2 = WITNESS_RANGE * WITNESS_RANGE;
    for (i, p) in civil.people.iter().enumerate() {
        if !p.alive() || p.pos.distance_squared(me.pose.feet) > r2 {
            continue;
        }
        if killed && victim == Victim::Civ(i) {
            continue;
        }
        // Сама жертва видела, кто её бил.
        if victim == Victim::Civ(i) || walls_clear(level, p.eye(), target) {
            out.push(Witness::Civ(i));
        }
    }
    for (&id, a) in &world.actors {
        let Some(faction) = a.brain.as_ref().and_then(|b| b.faction) else { continue };
        if !a.alive || faction.outside_walls() || a.pose.feet.distance(me.pose.feet) > GUARD_SIGHT {
            continue;
        }
        if killed && victim == Victim::Actor(id) {
            continue;
        }
        if victim == Victim::Actor(id) || walls_clear(level, a.pose.shoulder(), target) {
            out.push(Witness::Actor(id));
        }
    }
    out
}

fn witness_alive(civil: &CivilWorld, world: &GameWorld, w: Witness) -> bool {
    match w {
        Witness::Civ(i) => civil.people.get(i).is_some_and(Person::alive),
        Witness::Actor(id) => world.actors.get(&id).is_some_and(|a| a.alive),
    }
}

fn is_guard(world: &GameWorld, w: Witness) -> bool {
    matches!(w, Witness::Actor(id) if world.actors.get(&id).and_then(|a| a.brain.as_ref()).and_then(|b| b.faction) == Some(Faction::Garrison))
}

fn victim_key(v: Victim) -> u64 {
    match v {
        Victim::Civ(i) => CIV_ID_BASE + i as u64,
        Victim::Actor(id) => id,
    }
}

fn on_connect(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, civil: Res<CivilWorld>, world: Res<GameWorld>) {
    let ServerEvent::ClientConnected { client_id } = &event.0 else { return };
    for p in civil.people.iter().filter(|p| !p.alive()) {
        let corpse = world.corpses.iter().find(|c| c.civ.as_ref().is_some_and(|cc| cc.civ == p.civ)).map(|c| (c.id, c.pos.into(), c.yaw));
        server.send_message(*client_id, CH_RELIABLE, encode(&ServerMessage::CivilianDead { civ: p.civ, corpse }));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn civil_tick(
    mut server: ResMut<RenetServer>,
    mut world: ResMut<GameWorld>,
    mut civil: ResMut<CivilWorld>,
    level: Res<LevelColliders>,
    mut trade: Option<ResMut<TradeWorld>>,
    mut loot: Option<ResMut<crate::loot_server::LootWorld>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    let civil = &mut *civil;
    let world = &mut *world;
    if let Some(t) = trade.as_deref() {
        civil.add_merchants(t);
    }
    if civil.jail.is_none() {
        let spot = crate::level::places::place("praetorium").map(|p| p.pos).unwrap_or_else(|| crate::level::spawn_point(0));
        civil.jail = Some(crate::npc::find_free_spot(&level, spot));
    }

    // 1. Нападения: кто видел?
    for off in std::mem::take(&mut civil.queue) {
        if off.faction.outside_walls() {
            continue;
        }
        let key = (off.player, victim_key(off.victim));
        if !off.killed && civil.recent.get(&key).is_some_and(|t| now - t < SAME_CRIME_SECS) {
            continue;
        }
        civil.recent.insert(key, now);
        let guard_victim = off.faction == Faction::Garrison;
        let crime = Violence::of(guard_victim, off.killed);
        let seen = witnesses(civil, world, &level, off.player, off.victim, off.killed);
        if seen.is_empty() {
            if off.killed {
                send_law(&mut server, off.player, LawMessage::Notice("Кажется, никто не видел.".into()));
            }
            continue;
        }
        // Солдат, видевший всё сам, знает сразу; горожанину надо добежать до стражи.
        let instant = seen.iter().any(|w| is_guard(world, *w));
        if !instant && off.killed {
            send_law(&mut server, off.player, LawMessage::Notice("«Убивают! Стража!» — свидетель бежит за стражей.".into()));
        }
        // Новое преступление того же рода против той же жертвы заменяет старое (ранил — потом убил).
        civil.reports.retain(|r| !(r.player == off.player && r.victim == off.victim));
        civil.reports.push(Report { player: off.player, crime, faction: off.faction, victim: off.victim, witnesses: seen, due: now + if instant { 0.0 } else { REPORT_DELAY } });
    }

    // 2. Свидетели добежали до стражи.
    let (due, wait): (Vec<Report>, Vec<Report>) = std::mem::take(&mut civil.reports).into_iter().partition(|r| now >= r.due);
    civil.reports = wait;
    for r in due {
        if !r.witnesses.iter().any(|w| witness_alive(civil, world, *w)) {
            send_law(&mut server, r.player, LawMessage::Notice("Свидетелей не осталось: стража ничего не узнала.".into()));
            continue;
        }
        let Some(t) = trade.as_deref_mut() else { continue };
        t.add_fine(r.player, r.crime.fine());
        t.change_rep(r.player, r.faction, r.crime.rep());
        if r.faction != Faction::Garrison {
            t.change_rep(r.player, Faction::Garrison, -2.0);
        }
        if let Victim::Civ(i) = r.victim
            && let Civ::Merchant(id) = civil.people[i].civ
        {
            // Родня убитого торговца с убийцей дел иметь не будет.
            t.grudge(r.player, crate::trade::MerchantId(id), now + 3.0 * 3600.0);
        }
        civil.crimes.entry(r.player).or_default().push(r.crime.name().to_string());
        let fine = t.fine(r.player);
        send_law(
            &mut server,
            r.player,
            LawMessage::Notice(format!("Страже донесли: {}. За вами пеня {} — вы в розыске.", r.crime.name(), crate::economy::format_money(fine))),
        );
    }

    // 3. Ответы стражнику.
    for (player, answer) in std::mem::take(&mut civil.answers) {
        let Some(arrest) = civil.arrests.get(&player) else { continue };
        if !arrest.halted {
            continue;
        }
        let guard = arrest.guard;
        let (Some(t), Some(l)) = (trade.as_deref_mut(), loot.as_deref_mut()) else { continue };
        let fine = t.fine(player);
        match answer {
            LawAnswer::Pay => {
                let mut uids = l.uids.clone();
                let Some(inv) = l.inventories.get_mut(&player) else { continue };
                match crate::economy::take_money(inv, fine, &mut uids) {
                    Ok(()) => {
                        l.uids = uids;
                        server.send_message(player, crate::loot_net::CH_LOOT, encode(&crate::loot_net::LootServerMessage::Inventory(inv.clone())));
                        t.clear_fine(player);
                        t.change_rep(player, Faction::Garrison, 1.0);
                        civil.crimes.remove(&player);
                        end_arrest(civil, world, &mut server, player);
                        send_law(&mut server, player, LawMessage::Notice(format!("Пеня {} уплачена. Вы больше не в розыске.", crate::economy::format_money(fine))));
                    }
                    Err(e) => send_law(&mut server, player, LawMessage::Notice(format!("{}: пеня {}.", e.message(), crate::economy::format_money(fine)))),
                }
            }
            LawAnswer::Jail => {
                let secs = jail_secs(fine);
                t.clear_fine(player);
                civil.crimes.remove(&player);
                end_arrest(civil, world, &mut server, player);
                civil.jailed.insert(player, now + secs as f64);
                if let (Some(a), Some(jail)) = (world.actors.get_mut(&player), civil.jail) {
                    a.pose.feet = jail;
                    server.send_message(player, CH_RELIABLE, encode(&ServerMessage::Correction { pos: jail.into() }));
                }
                send_law(&mut server, player, LawMessage::Jailed { secs });
            }
            LawAnswer::Resist => resist(civil, world, &mut server, t, player, guard),
        }
    }

    // 4. Стража ищет и останавливает разыскиваемых.
    if let Some(t) = trade.as_deref_mut() {
        let players: Vec<(u64, Vec3)> = world.actors.iter().filter(|(_, a)| a.kind == ActorKind::Player && a.alive).map(|(id, a)| (*id, a.pose.feet)).collect();
        for &(p, feet) in &players {
            let fine = t.fine(p);
            // Пеня уплачена (в том числе казначею): гарнизон отстаёт.
            if fine == 0 && civil.law_provoked.remove(&p) {
                for a in world.actors.values_mut() {
                    if let Some(b) = a.brain.as_mut().filter(|b| b.faction == Some(Faction::Garrison)) {
                        b.provoked.retain(|x| *x != p);
                    }
                }
            }
            if let Some(arrest) = civil.arrests.get_mut(&p) {
                let guard = world.actors.get(&arrest.guard).filter(|g| g.alive).map(|g| g.pose.feet);
                let Some(gpos) = guard.filter(|_| fine > 0) else {
                    end_arrest(civil, world, &mut server, p);
                    continue;
                };
                let dist = gpos.distance(feet);
                if !arrest.halted && dist < HALT_DIST {
                    arrest.halted = true;
                    arrest.since = now;
                    let purse = loot.as_deref().and_then(|l| l.inventories.get(&p)).map_or(0, crate::economy::purse);
                    let crimes = civil.crimes.get(&p).cloned().unwrap_or_default();
                    send_law(&mut server, p, LawMessage::Halt { fine, crimes, purse });
                } else if arrest.halted && (dist > WALK_AWAY || now - arrest.since > HALT_TIMEOUT) {
                    let guard = arrest.guard;
                    resist(civil, world, &mut server, t, p, guard);
                } else if !arrest.halted && now - arrest.since > 45.0 {
                    end_arrest(civil, world, &mut server, p);
                }
                continue;
            }
            if fine == 0 || civil.jailed(p) || civil.law_provoked.contains(&p) {
                continue;
            }
            let guard = world
                .actors
                .iter()
                .filter(|(_, a)| a.alive && a.pose.feet.distance(feet) < ARREST_SPOT)
                .filter(|(_, a)| a.brain.as_ref().is_some_and(|b| b.faction == Some(Faction::Garrison) && b.arrest.is_none() && !b.hostile_to(p)))
                .filter(|(_, a)| walls_clear(&level, a.pose.shoulder(), feet + Vec3::Y * 1.4))
                .min_by(|a, b| a.1.pose.feet.distance(feet).total_cmp(&b.1.pose.feet.distance(feet)))
                .map(|(id, _)| *id);
            if let Some(g) = guard {
                if let Some(b) = world.actors.get_mut(&g).and_then(|a| a.brain.as_mut()) {
                    b.arrest = Some(p);
                }
                civil.arrests.insert(p, Arrest { guard: g, since: now, halted: false });
                send_law(&mut server, p, LawMessage::Notice("«Стой! Именем закона!» — к вам идёт стражник.".into()));
            }
        }
    }

    // 5. Карцер.
    let jail = civil.jail;
    let mut released = Vec::new();
    for (&p, &until) in &civil.jailed {
        if now >= until || !world.actors.contains_key(&p) {
            released.push(p);
            continue;
        }
        if let (Some(a), Some(jail)) = (world.actors.get_mut(&p), jail)
            && a.pose.feet.distance(jail) > 2.5
        {
            a.pose.feet = jail;
            server.send_message(p, CH_RELIABLE, encode(&ServerMessage::Correction { pos: jail.into() }));
        }
    }
    for p in released {
        civil.jailed.remove(&p);
        send_law(&mut server, p, LawMessage::Released);
    }

    // 6. На место погибших приходят другие.
    for p in civil.people.iter_mut() {
        if p.dead_until.is_some_and(|t| now >= t) {
            p.dead_until = None;
            p.hp = p.max_hp;
            match p.civ {
                Civ::Resident(i) => level.set_body_gone(i as usize, false),
                Civ::Merchant(id) => {
                    if let Some(t) = trade.as_deref_mut() {
                        t.merchants[id as usize].dead = false;
                    }
                }
            }
            server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CivilianBack { civ: p.civ }));
        }
    }
    civil.recent.retain(|_, t| now - *t < 60.0);
}

fn end_arrest(civil: &mut CivilWorld, world: &mut GameWorld, server: &mut RenetServer, player: u64) {
    if let Some(a) = civil.arrests.remove(&player) {
        if let Some(b) = world.actors.get_mut(&a.guard).and_then(|g| g.brain.as_mut()) {
            b.arrest = None;
        }
        if a.halted {
            send_law(server, player, LawMessage::HaltEnd);
        }
    }
}

/// Разыскиваемый не подчинился: пеня растёт, гарнизон рядом берётся за оружие.
fn resist(civil: &mut CivilWorld, world: &mut GameWorld, server: &mut RenetServer, trade: &mut TradeWorld, player: u64, guard: u64) {
    end_arrest(civil, world, server, player);
    trade.add_fine(player, Violence::Resisting.fine());
    trade.change_rep(player, Faction::Garrison, Violence::Resisting.rep());
    civil.crimes.entry(player).or_default().push(Violence::Resisting.name().to_string());
    civil.law_provoked.insert(player);
    let at = world.actors.get(&guard).map_or(Vec3::ZERO, |g| g.pose.feet);
    for a in world.actors.values_mut() {
        if a.pose.feet.distance(at) < 40.0
            && let Some(b) = a.brain.as_mut().filter(|b| b.faction == Some(Faction::Garrison))
        {
            b.provoke(player);
        }
    }
    send_law(server, player, LawMessage::Notice("«Не хочешь по-хорошему — будет по-плохому!» Стража атакует.".into()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with(residents: &[Resident]) -> CivilWorld {
        CivilWorld::new(residents)
    }

    #[test]
    fn children_are_not_targets() {
        let r = [Resident { pos: Vec3::ZERO, yaw: 0.0, role: Role::Child }, Resident { pos: Vec3::X, yaw: 0.0, role: Role::Shopkeeper }];
        let w = world_with(&r);
        let t: Vec<usize> = w.targets_near(Vec3::ZERO, 5.0).into_iter().map(|(i, _)| i).collect();
        assert_eq!(t, vec![1]);
    }

    #[test]
    fn dead_bodies_stop_blocking() {
        let layout = crate::level::layout();
        let Some((i, r)) = layout.residents.iter().enumerate().find(|(_, r)| r.role != Role::Child) else { return };
        let level = crate::level::colliders(crate::level::layout());
        let inside = r.pos + Vec3::Y * 1.0;
        assert!(level.point_solid(inside), "живой житель должен загораживать проход");
        // Клинок по живому человеку не отскакивает, как от стены.
        assert!(level.point_in_wall(inside).is_none_or(|b| b.max.y - b.min.y > 2.0));
        level.set_body_gone(i, true);
        assert!(!level.point_solid(inside) || level.point_in_wall(inside).is_some(), "тело погибшего всё ещё мешает");
    }

    /// Удар спатой по жителю попадает в тело, а не отскакивает от него, как от стены.
    #[test]
    fn sword_hits_resident_instead_of_bouncing() {
        use crate::combat::{Swing, WeaponKind, blade_hits_body, blade_hits_wall, blade_segment};
        let layout = crate::level::layout();
        let residents = layout.residents.clone();
        let level = crate::level::colliders(layout);
        let w = CivilWorld::new(&residents);
        let mut tried = 0;
        let mut hit = 0;
        for (i, r) in residents.iter().enumerate().filter(|(_, r)| r.role != Role::Child).take(40) {
            // Встаём в метре перед жителем лицом к нему.
            let fwd = Vec3::new(-r.yaw.sin(), 0.0, -r.yaw.cos());
            let feet = r.pos + fwd * 1.0;
            if !level.body_is_free(&crate::collision::Body::STANDING, feet + Vec3::Y * 0.05) {
                continue;
            }
            tried += 1;
            let d = r.pos - feet;
            let pose = Pose { feet, yaw: (-d.x).atan2(-d.z), pitch: 0.0, crouching: false };
            let target = w.people[i].pose();
            for k in 0..=10 {
                let (h, t) = blade_segment(&pose, WeaponKind::Spatha, Swing::RightToLeft, k as f32 / 10.0);
                if blade_hits_wall(&level, h, t, feet).is_some() {
                    break;
                }
                if blade_hits_body(h, t, &target).is_some() {
                    hit += 1;
                    break;
                }
            }
        }
        assert!(tried >= 5, "мало жителей на открытом месте: {tried}");
        assert!(hit * 10 >= tried * 7, "клинок попал по жителю {hit} раз из {tried}");
    }
}
