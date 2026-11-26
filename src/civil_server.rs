//! Сервер: мирные люди и закон.
//!
//! Жители и торговцы смертны. Нападение, убийство или кража в городе — преступление, но лишь
//! если кто-то видел. Каждое замеченное преступление — «происшествие» со свидетелями:
//!
//! * Свидетель кричит («Держи вора!», «Убивают!»). У вора есть [`WITNESS_SECS`] секунд, пока
//!   свидетель не донёс: время идёт, когда вы не рядом с ним. Убрали последнего свидетеля —
//!   вы чисты (но сюда идёт стража: уходите с места преступления).
//! * На крик бежит стража: все стражники поблизости, а если рядом никого — только один,
//!   ближайший. Прибежавший и увидевший вас — тоже свидетель.
//! * Ночью темно: без огня вора видно лишь вблизи, люди спят и стражи вдвое меньше. Но шум
//!   будит спящих, а крепкие мужчины выбегают кто с топором, кто с рогатиной.
//!
//! Донесённое преступление — пеня (общая с кражами в лавках, см. `TradeWorld::fine`) и удар по
//! репутации. Разыскиваемого стражник останавливает: уплатить на месте, сдаться (карцер
//! претории), уговорить отпустить (красноречие) или сопротивляться — тогда гарнизон берётся
//! за оружие.

use crate::civilians::*;
use crate::collision::LevelColliders;
use crate::combat::{Pose, WeaponKind};
use crate::daily_life::Act;
use crate::factions::Faction;
use crate::level::{Resident, Role};
use crate::npc::{NpcSpawn, Rng};
use crate::protocol::*;
use crate::server::{CivCorpse, GameWorld};
use crate::trade_server::TradeWorld;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::{HashMap, HashSet};

/// Мирные люди в списках целей (стрелы, «уже задет этим ударом») получают id с этим смещением.
pub const CIV_ID_BASE: u64 = 1 << 60;
/// Номера горожан, схватившихся за оружие: смещение + (поколение << 16) + номер жителя.
/// Клиент по номеру узнаёт жителя и одевает бойца как его.
pub const MILITIA_ID_BASE: u64 = (1 << 62) + (1 << 48);
/// Через сколько в дом погибшего въезжает другой человек.
const RESIDENT_RETURN_SECS: f64 = 20.0 * 60.0;
/// Через сколько лавку убитого открывает наследник.
const HEIR_SECS: f64 = 15.0 * 60.0;
/// Как далеко мирные видят днём (ночью без огня — [`SIGHT_DARK`]).
const SIGHT_DAY: f32 = 22.0;
const GUARD_SIGHT_DAY: f32 = 26.0;
const SIGHT_DARK: f32 = 6.5;
/// Ближе этого вора замечают даже спиной (шорох, тень).
const SENSE_CLOSE: f32 = 2.2;
/// Рядом со свидетелем (держите его, дерётесь): он не может бежать к страже.
const ENGAGED: f32 = 4.0;
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
/// Как далеко стража слышит крик (днём город шумит, ночью тихо).
const HEAR_DAY: f32 = 45.0;
const HEAR_NIGHT: f32 = 65.0;
/// Сколько стражников бегут на один крик.
const MAX_RESPONDERS: usize = 4;
/// Сколько стражник ищет на месте происшествия, прежде чем вернуться на пост.
const SEARCH_SECS: f64 = 75.0;
/// Застали у свежего тела или взломанного сундука — подозреваемый.
const SCENE_SUSPECT: f32 = 12.0;
/// Сколько горожан может выбежать с оружием против одного игрока.
const MAX_MILITIA: usize = 6;
/// Горожанин остывает, если давно не видит врага.
const MILITIA_CALM_SECS: f64 = 20.0;

pub struct CivilServerPlugin;

impl Plugin for CivilServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_connect).add_observer(on_merchant_caught).add_observer(on_container_taken).add_observer(on_owned_item_taken).add_systems(Update, civil_tick);
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
    /// Чем занят сейчас (по распорядку дня; ставит `daily_life`).
    pub act: Act,
    /// Разбужен шумом до этого времени.
    pub awake_until: f64,
    /// Схватился за оружие: номер бойца, которым он сейчас стал.
    pub rallied: Option<u64>,
}

impl Person {
    pub fn alive(&self) -> bool {
        self.dead_until.is_none()
    }

    fn vulnerable(&self) -> bool {
        self.alive() && self.rallied.is_none() && self.role.is_none_or(vulnerable)
    }

    pub fn pose(&self) -> Pose {
        Pose { feet: self.pos, yaw: self.yaw, pitch: 0.0, crouching: false }
    }

    fn eye(&self) -> Vec3 {
        // Чуть выше своей коробки-тела, чтобы она не заслоняла взгляд.
        self.pos + Vec3::Y * 1.85
    }

    /// Спит (или ушёл в дом) и не разбужен.
    fn asleep(&self, now: f64) -> bool {
        matches!(self.act, Act::Sleep | Act::Away | Act::Lie) && self.awake_until < now
    }

    /// Крепкий мужчина, который схватит что под рукой и выбежит на шум.
    fn fighter(&self) -> bool {
        matches!(self.role, Some(Role::Householder | Role::Craftsman | Role::Farmer | Role::Sailor | Role::Servant | Role::Shopkeeper | Role::Soldier | Role::Officer | Role::Bather))
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

/// Кража: что, где и у кого (ставит лавка, сундук, хозяйская утварь).
#[derive(Clone, Debug)]
pub struct Theft {
    pub player: u64,
    /// Цена украденного, нуммии.
    pub value: u32,
    pub at: Vec3,
    pub what: String,
    /// Чьё (для репутации): фракция хозяина.
    pub faction: Faction,
    /// Поймал за руку этот человек (торговец у своего прилавка).
    pub caught_by: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Witness {
    Civ(usize),
    Actor(u64),
}

/// Свидетель происшествия и сколько секунд он уже свободен (не рядом с вором).
struct Seen {
    who: Witness,
    free: f32,
}

/// Замеченное преступление, о котором ещё не донесли.
struct Incident {
    player: u64,
    crime: Crime,
    faction: Faction,
    victim: Option<Victim>,
    scene: Vec3,
    witnesses: Vec<Seen>,
}

/// Шум: крик свидетеля, драка, вопль раненого.
#[derive(Clone, Copy, Debug)]
struct Noise {
    at: Vec3,
    radius: f32,
    /// Кричат «на помощь!» — бежит стража, выбегают с оружием.
    shout: bool,
    /// Кто виноват.
    player: u64,
    violent: bool,
}

/// Стражник, бегущий на крик или ищущий на месте происшествия.
struct Responder {
    scene: Vec3,
    /// Его пост: куда вернуться.
    post: (Vec3, f32, f32),
    until: f64,
    /// Ищут виноватого в этом (после того как свидетелей не стало).
    suspect: Option<(u64, Crime, Faction)>,
}

/// Горожанин с оружием.
struct Militia {
    person: usize,
    player: u64,
    /// Когда последний раз видел врага.
    seen_enemy: f64,
    /// Идёт домой.
    going_home: Option<f64>,
}

/// Ночная смена: часть стражи уходит спать в казарму.
#[derive(Default)]
struct Shifts {
    /// Ушли в казарму: стражник идёт туда (номер -> пост).
    leaving: HashMap<u64, (NpcSpawn, f64)>,
    /// Спят в казарме до утра.
    asleep: Vec<NpcSpawn>,
    /// Посты, где ночью стоит лишь половина: номера бойцов, которые уходят.
    plan: Option<HashSet<u64>>,
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
    /// Кражи за кадр (лавки, сундуки, хозяйская утварь).
    pub thefts: Vec<Theft>,
    /// Ответы игроков стражникам.
    pub answers: Vec<(u64, LawAnswer)>,
    incidents: Vec<Incident>,
    noises: Vec<Noise>,
    arrests: HashMap<u64, Arrest>,
    /// Пытался уговорить стражника в этом разговоре (вторая попытка не даётся).
    talked: HashSet<u64>,
    /// Кто сидит в карцере и до какого времени.
    jailed: HashMap<u64, f64>,
    /// (игрок, жертва) -> когда было последнее нападение.
    recent: HashMap<(u64, u64), f64>,
    /// За что разыскивается (для разговора со стражей).
    crimes: HashMap<u64, Vec<String>>,
    /// Оказали сопротивление: гарнизон дерётся, пока пеня не уплачена.
    law_provoked: HashSet<u64>,
    responders: HashMap<u64, Responder>,
    militia: HashMap<u64, Militia>,
    militia_gen: u64,
    shifts: Shifts,
    /// Где светло: факелы и лампы (точка, радиус).
    lights: Vec<(Vec3, f32)>,
    /// Что показано игроку про свидетелей (число, секунды).
    shown: HashMap<u64, (u8, i32)>,
    jail: Option<Vec3>,
    barracks: Option<Vec3>,
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
                act: Act::Idle,
                awake_until: 0.0,
                rallied: None,
            })
            .collect();
        Self {
            people,
            merchants_added: false,
            queue: Vec::new(),
            thefts: Vec::new(),
            answers: Vec::new(),
            incidents: Vec::new(),
            noises: Vec::new(),
            arrests: HashMap::new(),
            talked: HashSet::new(),
            jailed: HashMap::new(),
            recent: HashMap::new(),
            crimes: HashMap::new(),
            law_provoked: HashSet::new(),
            responders: HashMap::new(),
            militia: HashMap::new(),
            militia_gen: 0,
            shifts: Shifts::default(),
            lights: Vec::new(),
            shown: HashMap::new(),
            jail: None,
            barracks: None,
            rng: Rng::new(0x5EED_C1F1),
            alarms: Vec::new(),
        }
    }

    /// Факелы и лампы уровня: возле них светло и ночью.
    pub fn set_lights(&mut self, layout: &crate::level::Layout) {
        self.lights = layout.torches.iter().map(|t| (t.pos, 7.0)).chain(layout.lamps.iter().map(|l| (l.pos, (l.range * 0.6).clamp(3.0, 9.0)))).collect();
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
                act: Act::Idle,
                awake_until: 0.0,
                rallied: None,
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

    /// Номер человека (в `people`) по торговцу.
    fn merchant_person(&self, id: u16) -> Option<usize> {
        self.people.iter().position(|p| p.civ == Civ::Merchant(id))
    }

    /// Кража: проверят, видел ли кто (см. `civil_tick`).
    pub fn report_theft(&mut self, theft: Theft) {
        self.thefts.push(theft);
    }

    /// Насколько светло у точки от факелов и ламп (0..1).
    fn lit_at(&self, p: Vec3) -> f32 {
        let mut best = 0.0f32;
        for (at, r) in &self.lights {
            let d2 = at.distance_squared(p);
            if d2 < r * r {
                best = best.max(1.0 - d2 / (r * r));
            }
        }
        // Ночной дозор ходит с факелами.
        for c in &self.people {
            if c.alive() && c.rallied.is_none() && matches!(c.role, Some(Role::Soldier | Role::Officer)) && !c.asleep(f64::MAX) {
                let d2 = c.pos.distance_squared(p);
                if d2 < 36.0 {
                    best = best.max(1.0 - d2 / 36.0);
                }
            }
        }
        best
    }

    /// Пеня за ещё не донесённые происшествия.
    fn pending_fine(&self, player: u64) -> u32 {
        self.incidents.iter().filter(|i| i.player == player).map(|i| i.crime.fine()).sum()
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

/// Условия видимости в этот момент.
#[derive(Clone, Copy)]
struct Sight {
    /// Темнота 0..1 (ночь без луны — 1).
    dark: f32,
    fog: f32,
    rain: f32,
    now: f64,
}

impl Sight {
    fn of(clock: &crate::world_clock::WorldClock, now: f64) -> Self {
        let w = clock.weather();
        Sight { dark: clock.darkness(), fog: w.fog, rain: w.rain, now }
    }

    /// Насколько далеко видно человека в точке со светом `lit` (0..1).
    fn range(&self, day: f32, lit: f32, crouching: bool) -> f32 {
        let dark = self.dark * (1.0 - lit);
        let r = day + (SIGHT_DARK - day) * dark;
        r * (1.0 - 0.6 * self.fog) * (1.0 - 0.15 * self.rain) * if crouching { 0.65 } else { 1.0 }
    }
}

/// Что именно видели: кражу замечают лишь смотрящие в вашу сторону, драку слышат все вокруг.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Theft,
    Violence,
}

/// Видит ли смотрящий из `eye` (взгляд `yaw`) человека в `target`.
fn notices(level: &LevelColliders, eye: Vec3, yaw: f32, target: Vec3, range: f32, look: Look) -> bool {
    let d = target - eye;
    let flat = Vec3::new(d.x, 0.0, d.z);
    let dist = flat.length();
    if dist > range {
        return false;
    }
    if look == Look::Theft && dist > SENSE_CLOSE {
        // Поле зрения ~140°.
        let fwd = crate::daily_life::forward(yaw);
        if fwd.dot(flat / dist.max(1e-3)) < 0.34 {
            return false;
        }
    }
    walls_clear(level, eye, target)
}

/// Кто видел игрока (кроме убитой жертвы).
fn witnesses(civil: &CivilWorld, world: &GameWorld, level: &LevelColliders, sight: Sight, player: u64, victim: Option<Victim>, killed: bool, look: Look) -> Vec<Witness> {
    let Some(me) = world.actors.get(&player) else { return Vec::new() };
    let target = me.pose.shoulder();
    let lit = civil.lit_at(me.pose.feet);
    let crouch = me.pose.crouching && look == Look::Theft;
    let mut out = Vec::new();
    let r = sight.range(SIGHT_DAY, lit, crouch);
    for (i, p) in civil.people.iter().enumerate() {
        if !p.alive() || p.rallied.is_some() || p.pos.distance_squared(me.pose.feet) > (r + 1.0) * (r + 1.0) {
            continue;
        }
        if killed && victim == Some(Victim::Civ(i)) {
            continue;
        }
        // Сама жертва видела, кто её бил.
        if victim == Some(Victim::Civ(i)) {
            out.push(Witness::Civ(i));
            continue;
        }
        if p.asleep(sight.now) {
            continue;
        }
        if notices(level, p.eye(), p.yaw, target, r, look) {
            out.push(Witness::Civ(i));
        }
    }
    let rg = sight.range(GUARD_SIGHT_DAY, lit, crouch);
    for (&id, a) in &world.actors {
        let Some(faction) = a.brain.as_ref().and_then(|b| b.faction) else { continue };
        if !a.alive || faction.outside_walls() || a.pose.feet.distance(me.pose.feet) > rg + 1.0 {
            continue;
        }
        if killed && victim == Some(Victim::Actor(id)) {
            continue;
        }
        if victim == Some(Victim::Actor(id)) || notices(level, a.pose.shoulder(), a.pose.yaw, target, rg, look) {
            out.push(Witness::Actor(id));
        }
    }
    out
}

fn witness_alive(civil: &CivilWorld, world: &GameWorld, w: Witness) -> bool {
    match w {
        Witness::Civ(i) => civil.people.get(i).is_some_and(|p| p.alive() && p.rallied.is_none()),
        Witness::Actor(id) => world.actors.get(&id).is_some_and(|a| a.alive),
    }
}

fn witness_pos(civil: &CivilWorld, world: &GameWorld, w: Witness) -> Option<Vec3> {
    match w {
        Witness::Civ(i) => civil.people.get(i).map(|p| p.pos),
        Witness::Actor(id) => world.actors.get(&id).map(|a| a.pose.feet),
    }
}

fn victim_key(v: Victim) -> u64 {
    match v {
        Victim::Civ(i) => CIV_ID_BASE + i as u64,
        Victim::Actor(id) => id,
    }
}

/// Номер жителя по номеру горожанина-бойца.
pub fn militia_resident(id: u64) -> Option<usize> {
    id.checked_sub(MILITIA_ID_BASE).map(|k| (k & 0xFFFF) as usize)
}

fn on_connect(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, civil: Res<CivilWorld>, world: Res<GameWorld>) {
    let ServerEvent::ClientConnected { client_id } = &event.0 else { return };
    for p in civil.people.iter().filter(|p| !p.alive() || p.rallied.is_some()) {
        let corpse = world.corpses.iter().find(|c| c.civ.as_ref().is_some_and(|cc| cc.civ == p.civ)).map(|c| (c.id, c.pos.into(), c.yaw));
        server.send_message(*client_id, CH_RELIABLE, encode(&ServerMessage::CivilianDead { civ: p.civ, corpse: corpse.filter(|_| !p.alive()) }));
    }
}

/// Вынули вещь из сундука в чужом доме, церкви, казарме или на складе — кража, если видели.
fn on_container_taken(event: On<crate::loot_server::ContainerTaken>, mut civil: ResMut<CivilWorld>, locks: Option<Res<crate::towns_server::Locks>>) {
    use crate::items::LootTableId as T;
    let c = event.event();
    if crate::level::active() != crate::level::LevelKind::Chersonesus || locks.is_some_and(|l| l.own_container(c.container, c.player)) {
        return;
    }
    let faction = match c.table {
        T::Church => Faction::Church,
        T::Barracks => Faction::Garrison,
        T::Storehouse => Faction::Merchants,
        _ => Faction::Citizens,
    };
    civil.report_theft(Theft { player: c.player, value: c.value.max(1), at: c.at, what: c.what.clone(), faction, caught_by: None });
}

/// Взял хозяйскую вещь в жилом доме (`home_items`): в сумку с места или вынес из дома.
fn on_owned_item_taken(event: On<crate::home_items::OwnedItemTaken>, mut civil: ResMut<CivilWorld>) {
    let t = event.event();
    if crate::level::active() != crate::level::LevelKind::Chersonesus {
        return;
    }
    let what = t.item.info().name.to_string();
    civil.report_theft(Theft { player: t.player, value: t.item.value().max(1), at: t.at, what, faction: Faction::Citizens, caught_by: None });
}

/// Торговец поймал вора за руку (см. `trade_server::caught_stealing`).
fn on_merchant_caught(event: On<crate::trade_server::CrimeCommitted>, mut civil: ResMut<CivilWorld>) {
    let c = event.event();
    if c.crime != crate::factions::Crime::TheftCaught || c.victim.outside_walls() {
        return;
    }
    let caught_by = c.merchant.and_then(|m| civil.merchant_person(m.0));
    civil.thefts.push(Theft { player: c.player, value: c.value, at: c.at, what: c.what.clone(), faction: c.victim, caught_by });
}

#[allow(clippy::too_many_arguments)]
pub fn civil_tick(
    mut server: ResMut<RenetServer>,
    mut world: ResMut<GameWorld>,
    mut civil: ResMut<CivilWorld>,
    level: Res<LevelColliders>,
    mut trade: Option<ResMut<TradeWorld>>,
    mut loot: Option<ResMut<crate::loot_server::LootWorld>>,
    knowledge: Option<Res<crate::knowledge::KnowledgeWorld>>,
    mut practice: Option<ResMut<crate::skills_server::PracticeQueue>>,
    clock: Res<crate::world_clock::WorldClock>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs();
    let civil = &mut *civil;
    let world = &mut *world;
    if let Some(t) = trade.as_deref() {
        civil.add_merchants(t);
    }
    if civil.jail.is_none() {
        let spot = crate::level::places::place("praetorium").map(|p| p.pos).unwrap_or_else(|| crate::level::spawn_point(0));
        civil.jail = Some(crate::npc::find_free_spot(&level, spot));
        let barracks = crate::level::places::place("garrison").map(|p| p.pos).unwrap_or(spot);
        civil.barracks = Some(crate::npc::find_free_spot(&level, barracks));
    }
    let sight = Sight::of(&clock, now);

    // 1. Нападения: кто видел? Драка шумит в любом случае.
    for off in std::mem::take(&mut civil.queue) {
        if off.faction.outside_walls() {
            continue;
        }
        let at = world.actors.get(&off.player).map_or(Vec3::ZERO, |a| a.pose.feet);
        civil.noises.push(Noise { at, radius: if off.killed { 26.0 } else { 18.0 }, shout: false, player: off.player, violent: true });
        let key = (off.player, victim_key(off.victim));
        if !off.killed && civil.recent.get(&key).is_some_and(|t| now - t < SAME_CRIME_SECS) {
            continue;
        }
        civil.recent.insert(key, now);
        let guard_victim = off.faction == Faction::Garrison;
        let crime = Crime::Violence(Violence::of(guard_victim, off.killed));
        let seen = witnesses(civil, world, &level, sight, off.player, Some(off.victim), off.killed, Look::Violence);
        if seen.is_empty() {
            if off.killed {
                send_law(&mut server, off.player, LawMessage::Notice("Кажется, никто не видел.".into()));
            }
            continue;
        }
        if off.killed {
            send_law(&mut server, off.player, LawMessage::Notice("«Убивают! Стража!» — вас видели.".into()));
        }
        open_incident(civil, world, off.player, crime, off.faction, Some(off.victim), at, seen);
    }

    // 2. Кражи: кто смотрел в вашу сторону?
    for th in std::mem::take(&mut civil.thefts) {
        let mut seen = witnesses(civil, world, &level, sight, th.player, None, false, Look::Theft);
        if let Some(i) = th.caught_by
            && !seen.contains(&Witness::Civ(i))
            && civil.people.get(i).is_some_and(Person::alive)
        {
            seen.push(Witness::Civ(i));
        }
        if seen.is_empty() {
            continue;
        }
        // Мелкие кражи подряд на одном месте — одно происшествие.
        if let Some(inc) = civil.incidents.iter_mut().find(|i| i.player == th.player && matches!(i.crime, Crime::Theft { .. }) && i.scene.distance(th.at) < 20.0) {
            if let Crime::Theft { value } = &mut inc.crime {
                *value += th.value;
            }
            for w in seen {
                if !inc.witnesses.iter().any(|s| s.who == w) {
                    inc.witnesses.push(Seen { who: w, free: 0.0 });
                }
            }
            continue;
        }
        let who = if th.caught_by.is_some() { "торговец схватил вас за руку" } else { "вас заметили" };
        send_law(&mut server, th.player, LawMessage::Notice(format!("«Держи вора!» — {who}: «{}».", th.what)));
        open_incident(civil, world, th.player, Crime::Theft { value: th.value }, th.faction, None, th.at, seen);
    }

    // 3. Шум: будит спящих, зовёт стражу, поднимает горожан.
    process_noises(civil, world, &level, &mut server, sight);

    // 4. Свидетели: бегут к страже. Убрали всех — чисто; кто-то успел — донёс.
    tick_incidents(civil, world, &level, &mut server, trade.as_deref_mut(), sight, dt);

    // 5. Стража на крике и на месте происшествия.
    tick_responders(civil, world, &level, &mut server, sight);

    // 6. Горожане с оружием.
    tick_militia(civil, world, &level, &mut server, now);

    // 7. Ночная смена стражи.
    tick_shifts(civil, world, &level, &mut server, &clock, now);

    // 8. Ответы стражнику.
    for (player, answer) in std::mem::take(&mut civil.answers) {
        let Some(arrest) = civil.arrests.get(&player) else { continue };
        if !arrest.halted {
            continue;
        }
        let guard = arrest.guard;
        let (Some(t), Some(l)) = (trade.as_deref_mut(), loot.as_deref_mut()) else { continue };
        let fine = t.fine(player) + civil.pending_fine(player);
        match answer {
            LawAnswer::Pay => {
                let mut uids = l.uids.clone();
                let Some(inv) = l.inventories.get_mut(&player) else { continue };
                match crate::economy::take_money(inv, fine, &mut uids) {
                    Ok(()) => {
                        l.uids = uids;
                        server.send_message(player, crate::loot_net::CH_LOOT, encode(&crate::loot_net::LootServerMessage::Inventory(inv.clone())));
                        settle(civil, t, player);
                        let forgiven = t.fine_paid(player);
                        end_arrest(civil, world, &mut server, player);
                        send_law(&mut server, player, LawMessage::Notice(format!("Пеня {} уплачена. Вы больше не в розыске.{forgiven}", crate::economy::format_money(fine))));
                    }
                    Err(e) => send_law(&mut server, player, LawMessage::Notice(format!("{}: пеня {}.", e.message(), crate::economy::format_money(fine)))),
                }
            }
            LawAnswer::Jail => {
                let secs = jail_secs(fine);
                settle(civil, t, player);
                end_arrest(civil, world, &mut server, player);
                civil.jailed.insert(player, now + secs as f64);
                if let (Some(a), Some(jail)) = (world.actors.get_mut(&player), civil.jail) {
                    a.pose.feet = jail;
                    server.send_message(player, CH_RELIABLE, encode(&ServerMessage::Correction { pos: jail.into() }));
                }
                send_law(&mut server, player, LawMessage::Jailed { secs });
            }
            LawAnswer::Persuade => {
                if !civil.talked.insert(player) {
                    continue;
                }
                let eloquence = knowledge.as_deref().and_then(|k| k.players.get(&player)).map_or(0.0, |k| k.skill(crate::lore::Skill::Eloquence));
                let bonus = world.actors.get(&player).map_or(0.0, |a| a.skill_mods.persuade);
                let rep = t.rep(player).get(Faction::Garrison);
                let chance = (persuade_chance(eloquence, fine, rep) + bonus).min(0.95);
                let ok = civil.rng.chance(chance);
                if let Some(q) = practice.as_deref_mut() {
                    q.add(player, crate::lore::Skill::Eloquence, if ok { 2.5 } else { 1.0 });
                }
                if ok {
                    settle(civil, t, player);
                    end_arrest(civil, world, &mut server, player);
                    send_law(&mut server, player, LawMessage::Notice("«Ладно, ступай. Но чтобы я тебя больше здесь не видел». Стражник отпустил вас.".into()));
                } else {
                    let purse = loot.as_deref().and_then(|l| l.inventories.get(&player)).map_or(0, crate::economy::purse);
                    let crimes = civil.crimes.get(&player).cloned().unwrap_or_default();
                    let crimes = with_pending(civil, player, crimes);
                    send_law(&mut server, player, LawMessage::Halt { fine, crimes, purse, persuade: None });
                    send_law(&mut server, player, LawMessage::Notice("«Не заговаривай мне зубы!» Стражник не поверил.".into()));
                }
            }
            LawAnswer::Resist => resist(civil, world, &level, &mut server, player, guard, sight),
        }
    }

    // 9. Стража ищет и останавливает разыскиваемых.
    if let Some(t) = trade.as_deref_mut() {
        let players: Vec<(u64, Vec3)> = world.actors.iter().filter(|(_, a)| a.kind == ActorKind::Player && a.alive).map(|(id, a)| (*id, a.pose.feet)).collect();
        for &(p, feet) in &players {
            let fine = t.fine(p);
            let pending = civil.pending_fine(p);
            // Пеня уплачена (в том числе казначею) и свидетелей нет: гарнизон отстаёт.
            if fine == 0 && pending == 0 && civil.law_provoked.remove(&p) {
                for a in world.actors.values_mut() {
                    if let Some(b) = a.brain.as_mut().filter(|b| b.faction == Some(Faction::Garrison)) {
                        b.provoked.retain(|x| *x != p);
                    }
                }
            }
            if let Some(arrest) = civil.arrests.get_mut(&p) {
                let guard = world.actors.get(&arrest.guard).filter(|g| g.alive).map(|g| g.pose.feet);
                let Some(gpos) = guard.filter(|_| fine + pending > 0) else {
                    end_arrest(civil, world, &mut server, p);
                    continue;
                };
                let dist = gpos.distance(feet);
                if !arrest.halted && dist < HALT_DIST {
                    arrest.halted = true;
                    arrest.since = now;
                    civil.talked.remove(&p);
                    let purse = loot.as_deref().and_then(|l| l.inventories.get(&p)).map_or(0, crate::economy::purse);
                    let crimes = civil.crimes.get(&p).cloned().unwrap_or_default();
                    let crimes = with_pending(civil, p, crimes);
                    let eloquence = knowledge.as_deref().and_then(|k| k.players.get(&p)).map_or(0.0, |k| k.skill(crate::lore::Skill::Eloquence));
                    send_law(&mut server, p, LawMessage::Halt { fine: fine + pending, crimes, purse, persuade: Some(eloquence.round() as u8) });
                } else if arrest.halted && (dist > WALK_AWAY || now - arrest.since > HALT_TIMEOUT) {
                    let guard = arrest.guard;
                    resist(civil, world, &level, &mut server, p, guard, sight);
                } else if !arrest.halted && now - arrest.since > 45.0 {
                    end_arrest(civil, world, &mut server, p);
                }
                continue;
            }
            if civil.jailed(p) || civil.law_provoked.contains(&p) {
                continue;
            }
            // Останавливает тот, кто знает: за вами пеня — любой стражник; ещё не донесли —
            // лишь тот, кто сам всё видел.
            let knows = |id: u64| fine > 0 || civil.incidents.iter().any(|i| i.player == p && i.witnesses.iter().any(|w| w.who == Witness::Actor(id)));
            let guard = world
                .actors
                .iter()
                .filter(|(_, a)| a.alive && a.pose.feet.distance(feet) < ARREST_SPOT)
                .filter(|(_, a)| a.brain.as_ref().is_some_and(|b| b.faction == Some(Faction::Garrison) && b.arrest.is_none() && !b.hostile_to(p)))
                .filter(|(id, _)| knows(**id))
                .filter(|(_, a)| walls_clear(&level, a.pose.shoulder(), feet + Vec3::Y * 1.4))
                .min_by(|a, b| a.1.pose.feet.distance(feet).total_cmp(&b.1.pose.feet.distance(feet)))
                .map(|(id, _)| *id);
            if let Some(g) = guard {
                if let Some(b) = world.actors.get_mut(&g).and_then(|a| a.brain.as_mut()) {
                    b.arrest = Some(p);
                    b.route.clear();
                }
                civil.arrests.insert(p, Arrest { guard: g, since: now, halted: false });
                send_law(&mut server, p, LawMessage::Notice("«Стой! Именем закона!» — к вам идёт стражник.".into()));
            }
        }
    }

    // 10. Карцер.
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

    // 11. На место погибших приходят другие.
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

/// Список вин для разговора со стражей: донесённые и только что замеченные.
fn with_pending(civil: &CivilWorld, player: u64, mut crimes: Vec<String>) -> Vec<String> {
    for i in civil.incidents.iter().filter(|i| i.player == player) {
        crimes.push(i.crime.name().to_string());
    }
    crimes
}

/// Пеня уплачена, отсижена или прощена: всё забыто, включая не донесённое.
fn settle(civil: &mut CivilWorld, trade: &mut TradeWorld, player: u64) {
    trade.clear_fine(player);
    civil.crimes.remove(&player);
    civil.incidents.retain(|i| i.player != player);
}

#[allow(clippy::too_many_arguments)]
fn open_incident(civil: &mut CivilWorld, world: &GameWorld, player: u64, crime: Crime, faction: Faction, victim: Option<Victim>, scene: Vec3, seen: Vec<Witness>) {
    // Свидетели кричат: на крик бегут стража и соседи.
    for w in &seen {
        if let Some(at) = witness_pos(civil, world, *w) {
            civil.noises.push(Noise { at, radius: 30.0, shout: true, player, violent: crime.violent() });
        }
    }
    // Новое преступление против той же жертвы заменяет старое (ранил — потом убил).
    if let Some(v) = victim
        && let Some(old) = civil.incidents.iter_mut().find(|i| i.player == player && i.victim == Some(v))
    {
        if crime.weight() >= old.crime.weight() {
            old.crime = crime;
        }
        for w in seen {
            if !old.witnesses.iter().any(|s| s.who == w) {
                old.witnesses.push(Seen { who: w, free: 0.0 });
            }
        }
        return;
    }
    civil.incidents.push(Incident { player, crime, faction, victim, scene, witnesses: seen.into_iter().map(|w| Seen { who: w, free: 0.0 }).collect() });
}

/// Свидетели бегут к страже: кто свободен [`WITNESS_SECS`] секунд — донёс.
fn tick_incidents(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, server: &mut RenetServer, mut trade: Option<&mut TradeWorld>, sight: Sight, dt: f32) {
    let mut sealed = Vec::new();
    let mut cleared = Vec::new();
    let mut incidents = std::mem::take(&mut civil.incidents);
    for (k, inc) in incidents.iter_mut().enumerate() {
        let me = world.actors.get(&inc.player).filter(|a| a.alive).map(|a| a.pose.feet);
        inc.witnesses.retain(|w| witness_alive(civil, world, w.who));
        if inc.witnesses.is_empty() {
            cleared.push(k);
            continue;
        }
        for w in inc.witnesses.iter_mut() {
            let near = match (me, witness_pos(civil, world, w.who)) {
                (Some(me), Some(at)) => me.distance(at) < ENGAGED,
                _ => false,
            };
            if !near {
                w.free += dt;
            }
        }
        if inc.witnesses.iter().any(|w| w.free >= WITNESS_SECS) || me.is_none() {
            sealed.push(k);
        }
    }
    // Донесли.
    for &k in &sealed {
        let inc = &incidents[k];
        let Some(t) = trade.as_deref_mut() else { continue };
        t.add_fine(inc.player, inc.crime.fine());
        t.change_rep(inc.player, inc.faction, inc.crime.rep());
        if inc.faction != Faction::Garrison {
            t.change_rep(inc.player, Faction::Garrison, -2.0);
        }
        if let Some(Victim::Civ(i)) = inc.victim
            && let Civ::Merchant(id) = civil.people[i].civ
            && matches!(inc.crime, Crime::Violence(Violence::Murder))
        {
            // Родня убитого торговца с убийцей дел иметь не будет.
            t.grudge(inc.player, crate::trade::MerchantId(id), sight.now + 3.0 * 3600.0);
        }
        civil.crimes.entry(inc.player).or_default().push(inc.crime.name().to_string());
        let fine = t.fine(inc.player);
        send_law(
            server,
            inc.player,
            LawMessage::Notice(format!("Свидетель добежал до стражи: {}. За вами пеня {} — вы в розыске.", inc.crime.name(), crate::economy::format_money(fine))),
        );
    }
    // Свидетелей не стало.
    for &k in &cleared {
        let inc = &incidents[k];
        send_law(server, inc.player, LawMessage::Notice("Последний свидетель замолчал: стража ничего не узнает. Но уходите — сюда идут.".into()));
        // На шум всё равно придут: двое ближайших стражников осмотрят место.
        summon_guards(civil, world, level, inc.scene, 2, Some((inc.player, inc.crime, inc.faction)), sight);
    }
    let gone: HashSet<usize> = sealed.into_iter().chain(cleared).collect();
    civil.incidents = incidents.into_iter().enumerate().filter(|(k, _)| !gone.contains(k)).map(|(_, i)| i).collect();

    // Подсказка вору: сколько свидетелей и сколько у него секунд.
    let mut state: HashMap<u64, (u8, i32)> = HashMap::new();
    for inc in &civil.incidents {
        let e = state.entry(inc.player).or_insert((0, i32::MAX));
        e.0 = e.0.saturating_add(inc.witnesses.len() as u8);
        let left = inc.witnesses.iter().map(|w| WITNESS_SECS - w.free).fold(f32::MAX, f32::min);
        e.1 = e.1.min(left.ceil().max(0.0) as i32);
    }
    let players: Vec<u64> = civil.shown.keys().copied().chain(state.keys().copied()).collect::<HashSet<_>>().into_iter().collect();
    for p in players {
        let want = state.get(&p).copied().unwrap_or((0, 0));
        if civil.shown.get(&p) != Some(&want) {
            send_law(server, p, LawMessage::Witnessed { count: want.0, secs: want.1 as f32 });
            if want.0 == 0 {
                civil.shown.remove(&p);
            } else {
                civil.shown.insert(p, want);
            }
        }
    }
}

/// Шум будит спящих, зовёт стражу и поднимает горожан.
fn process_noises(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, server: &mut RenetServer, sight: Sight) {
    let noises = std::mem::take(&mut civil.noises);
    if noises.is_empty() {
        return;
    }
    let night = sight.dark > 0.5;
    let mut summoned: Vec<Vec3> = Vec::new();
    for n in noises {
        // Ночью тихо — слышно дальше.
        let r = n.radius * if night { 1.35 } else { 1.0 };
        for p in civil.people.iter_mut() {
            if p.alive() && p.pos.distance(n.at) < r && matches!(p.act, Act::Sleep | Act::Away | Act::Lie) {
                p.awake_until = sight.now + 150.0;
            }
        }
        if !n.shout {
            continue;
        }
        // На один и тот же крик стражу зовём один раз.
        if !summoned.iter().any(|s| s.distance(n.at) < 15.0) {
            summoned.push(n.at);
            summon_guards(civil, world, level, n.at, MAX_RESPONDERS, None, sight);
        }
        // Горожане: ночью на шум выбегают все, кто проснулся; днём — лишь при драке и те, кто рядом.
        if night || n.violent {
            let reach = if night { r } else { 14.0 };
            let active = civil.militia.values().filter(|m| m.player == n.player).count();
            let mut cands: Vec<(f32, usize)> = civil
                .people
                .iter()
                .enumerate()
                .filter(|(_, p)| p.alive() && p.rallied.is_none() && p.fighter() && !p.asleep(sight.now))
                .map(|(i, p)| (p.pos.distance(n.at), i))
                .filter(|(d, _)| *d < reach)
                .collect();
            cands.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, i) in cands.into_iter().take(MAX_MILITIA.saturating_sub(active)) {
                // Днём хватаются за оружие не все: кто-то прячется.
                if !night && crate::world_clock::unit(i as u64, (sight.now / 60.0) as u64) > 0.45 {
                    continue;
                }
                rally(civil, world, level, server, i, n.player, n.at, sight.now);
            }
        }
    }
}

/// Стража бежит к месту: все поблизости (не больше `max`), а если рядом никого — один ближайший.
fn summon_guards(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, at: Vec3, max: usize, suspect: Option<(u64, Crime, Faction)>, sight: Sight) {
    let hear = if sight.dark > 0.5 { HEAR_NIGHT } else { HEAR_DAY };
    let mut free: Vec<(f32, u64)> = world
        .actors
        .iter()
        .filter(|(id, a)| a.alive && !civil.responders.contains_key(id) && !civil.shifts.leaving.contains_key(id))
        .filter(|(_, a)| a.brain.as_ref().is_some_and(|b| b.faction == Some(Faction::Garrison) && b.arrest.is_none() && !b.fighting()))
        .map(|(id, a)| (a.pose.feet.distance(at), *id))
        .collect();
    free.sort_by(|a, b| a.0.total_cmp(&b.0));
    let near: Vec<u64> = free.iter().filter(|(d, _)| *d < hear).take(max).map(|(_, id)| *id).collect();
    let chosen = if near.is_empty() || suspect.is_some() { free.iter().take(if suspect.is_some() { max } else { 1 }).filter(|(d, _)| *d < 400.0).map(|(_, id)| *id).collect() } else { near };
    for id in chosen {
        let Some(a) = world.actors.get_mut(&id) else { continue };
        let feet = a.pose.feet;
        let Some(b) = a.brain.as_mut() else { continue };
        let route = crate::daily_life::town().and_then(|t| t.nav.route(feet, at)).unwrap_or_else(|| vec![at]);
        let post = (b.home, b.home_yaw, b.wander());
        b.set_route(route, true);
        b.set_post(at, b.home_yaw, 7.0);
        civil.responders.insert(id, Responder { scene: at, post, until: sight.now + SEARCH_SECS + feet.distance(at) as f64 / 3.0, suspect });
    }
    let _ = level;
}

/// Стражники на месте: кто увидел виноватого — свидетель; время вышло — назад на пост.
fn tick_responders(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, server: &mut RenetServer, sight: Sight) {
    let ids: Vec<u64> = civil.responders.keys().copied().collect();
    for id in ids {
        let Some(a) = world.actors.get(&id).filter(|a| a.alive) else {
            civil.responders.remove(&id);
            continue;
        };
        let (feet, eye, yaw) = (a.pose.feet, a.pose.shoulder(), a.pose.yaw);
        let r = civil.responders.get(&id).unwrap();
        let (scene, until, suspect) = (r.scene, r.until, r.suspect);
        let arrived = feet.distance(scene) < 10.0;
        // Прибежал и видит виноватого (или подозреваемого у места).
        for inc in civil.incidents.iter_mut().filter(|i| i.scene.distance(scene) < 40.0) {
            if inc.witnesses.iter().any(|w| w.who == Witness::Actor(id)) {
                continue;
            }
            let Some(p) = world.actors.get(&inc.player).filter(|p| p.alive) else { continue };
            let lit = civil.lights.iter().any(|(at, rr)| at.distance(p.pose.feet) < *rr);
            let range = sight.range(GUARD_SIGHT_DAY, if lit { 0.8 } else { 0.0 }, false);
            if notices(level, eye, yaw, p.pose.shoulder(), range, Look::Violence) {
                inc.witnesses.push(Seen { who: Witness::Actor(id), free: 0.0 });
            }
        }
        if arrived && let Some((player, crime, faction)) = suspect {
            let caught = world.actors.get(&player).filter(|p| p.alive).is_some_and(|p| {
                p.pose.feet.distance(scene) < SCENE_SUSPECT && notices(level, eye, yaw, p.pose.shoulder(), sight.range(GUARD_SIGHT_DAY, 0.3, false), Look::Violence)
            });
            if caught {
                if let Some(r) = civil.responders.get_mut(&id) {
                    r.suspect = None;
                }
                send_law(server, player, LawMessage::Notice("«А ты что тут делаешь?!» Стражник застал вас на месте преступления.".into()));
                open_incident(civil, world, player, crime, faction, None, scene, vec![Witness::Actor(id)]);
            }
        }
        let busy = world.actors.get(&id).and_then(|a| a.brain.as_ref()).is_some_and(|b| b.arrest.is_some() || b.fighting());
        if sight.now > until && !busy {
            let r = civil.responders.remove(&id).unwrap();
            if let Some(b) = world.actors.get_mut(&id).and_then(|a| a.brain.as_mut()) {
                b.set_post(r.post.0, r.post.1, r.post.2);
                let route = crate::daily_life::town().and_then(|t| t.nav.route(feet, r.post.0)).unwrap_or_default();
                b.set_route(route, false);
            }
        }
    }
}

/// Чем вооружится горожанин этой роли: оружие (для боя) и предмет (для модели в руке).
fn militia_arms(role: Role, i: usize) -> (WeaponKind, &'static str) {
    let pick = crate::world_clock::unit(i as u64, 0x3A11) < 0.5;
    match role {
        Role::Soldier | Role::Officer => (WeaponKind::Spatha, "Спата"),
        // Деревянные вилы в 510 г. в бой не годятся — хватают трезубую острогу или рогатину.
        Role::Farmer if pick => (WeaponKind::Spear, "Рыбацкая острога"),
        Role::Farmer => (WeaponKind::Spear, "Рогатина"),
        Role::Craftsman => (WeaponKind::Axe, "Плотничий топор"),
        Role::Sailor => (WeaponKind::Seax, "Тесак"),
        Role::Shopkeeper | Role::Bather => (WeaponKind::Seax, "Тесак"),
        _ if pick => (WeaponKind::Spear, "Рыбацкая острога"),
        _ => (WeaponKind::Axe, "Плотничий топор"),
    }
}

/// Житель хватает что под рукой и выбегает на шум.
#[allow(clippy::too_many_arguments)]
fn rally(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, server: &mut RenetServer, i: usize, player: u64, at: Vec3, now: f64) {
    let Some(p) = civil.people.get(i) else { return };
    let (Some(role), Civ::Resident(ri)) = (p.role, p.civ) else { return };
    if world.actors.get(&player).is_none_or(|a| !a.alive) {
        return;
    }
    civil.militia_gen = (civil.militia_gen + 1) & 0xFFFF_FFFF;
    let id = MILITIA_ID_BASE + (civil.militia_gen << 16) + ri as u64;
    let spawn = NpcSpawn { kind: ActorKind::Townsman, pos: p.pos, yaw: p.yaw, faction: Some(Faction::Citizens), wander: 5.0 };
    let (weapon, item) = militia_arms(role, i);
    let hp = p.hp;
    world.spawn_militia(level, id, spawn, weapon, crate::items::ItemBaseId::find(item).map(|b| b.0), hp, now);
    if let Some(b) = world.actors.get_mut(&id).and_then(|a| a.brain.as_mut()) {
        b.provoke(player);
        let route = crate::daily_life::town().and_then(|t| t.nav.route(p.pos, at)).unwrap_or_else(|| vec![at]);
        b.set_route(route, true);
        b.set_post(at, 0.0, 6.0);
    }
    let p = &mut civil.people[i];
    p.rallied = Some(id);
    level.set_body_gone(i, true);
    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CivilianDead { civ: p.civ, corpse: None }));
    civil.militia.insert(id, Militia { person: i, player, seen_enemy: now, going_home: None });
    info!("Житель {i} ({role:?}) схватился за оружие против игрока {player}");
}

fn tick_militia(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, server: &mut RenetServer, now: f64) {
    let ids: Vec<u64> = civil.militia.keys().copied().collect();
    for id in ids {
        let m = civil.militia.get(&id).unwrap();
        let (i, player) = (m.person, m.player);
        match world.actors.get(&id) {
            // Погиб: тело — как у мирного жителя (в своей одежде, с кошелём), дом пустеет.
            None | Some(crate::server::Actor { alive: false, .. }) => {
                civil.militia.remove(&id);
                let corpse = world.corpses.iter().position(|c| c.actor == id);
                let (pos, yaw, killer) = match corpse {
                    Some(k) => {
                        let c = world.corpses.remove(k);
                        server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CorpseRemoved { corpse_id: c.id }));
                        (c.pos, c.yaw, c.killer)
                    }
                    None => (civil.people[i].pos, civil.people[i].yaw, Some(player)),
                };
                let p = &mut civil.people[i];
                p.rallied = None;
                p.hp = 0.0;
                p.dead_until = Some(now + RESIDENT_RETURN_SECS);
                let role = p.role.unwrap_or(Role::Householder);
                let name = format!("Тело: {}", p.name);
                let civ = p.civ;
                let purse = role_purse(role);
                let coins = civil.rng.range(purse.0 as f32, purse.1.max(purse.0 + 1) as f32) as u32;
                crate::server::add_civ_corpse(server, world, killer, pos, yaw, CivCorpse { civ, name, table: role_loot(role), coins });
            }
            Some(a) => {
                let fighting = a.brain.as_ref().is_some_and(|b| b.fighting());
                let feet = a.pose.feet;
                let enemy_here = world.actors.get(&player).is_some_and(|p| p.alive && p.pose.feet.distance(feet) < 30.0);
                let m = civil.militia.get_mut(&id).unwrap();
                if fighting {
                    m.seen_enemy = now;
                }
                if m.going_home.is_none() && (now - m.seen_enemy > MILITIA_CALM_SECS || (!enemy_here && now - m.seen_enemy > 6.0)) {
                    // Остыл: идёт домой.
                    m.going_home = Some(now);
                    let home = civil.people[i].pos;
                    if let Some(b) = world.actors.get_mut(&id).and_then(|a| a.brain.as_mut()) {
                        b.provoked.clear();
                        let route = crate::daily_life::town().and_then(|t| t.nav.route(feet, home)).unwrap_or_else(|| vec![home]);
                        b.set_route(route, false);
                        b.set_post(home, 0.0, 0.0);
                    }
                    continue;
                }
                let home = civil.people[i].pos;
                if let Some(since) = m.going_home
                    && (feet.distance(home) < 2.0 || now - since > 45.0)
                {
                    // Дома: снова мирный житель.
                    let hp = a.fighter.hp;
                    civil.militia.remove(&id);
                    world.remove_npc(server, id);
                    let p = &mut civil.people[i];
                    p.rallied = None;
                    p.hp = hp.min(p.max_hp).max(1.0);
                    level.set_body_gone(i, false);
                    server.broadcast_message(CH_RELIABLE, encode(&ServerMessage::CivilianBack { civ: p.civ }));
                }
            }
        }
    }
}

/// Ночью половина постов пустеет: стража уходит спать в казарму и возвращается к утру.
fn tick_shifts(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, server: &mut RenetServer, clock: &crate::world_clock::WorldClock, now: f64) {
    let h = clock.hour();
    let night = !(5.0..22.0).contains(&h);
    let Some(barracks) = civil.barracks else { return };
    let near_player = |world: &GameWorld, p: Vec3| world.actors.values().any(|a| a.kind == ActorKind::Player && a.pose.feet.distance(p) < 30.0);
    if night {
        // Кто уходит: каждый второй стражник гарнизона (по номеру), кроме букеллариев на обходах.
        if civil.shifts.plan.is_none() {
            let mut guards: Vec<u64> = world
                .actors
                .iter()
                .filter(|(_, a)| a.alive && a.kind == ActorKind::Stratiotes && a.brain.as_ref().is_some_and(|b| b.faction == Some(Faction::Garrison)))
                .map(|(id, _)| *id)
                .collect();
            guards.sort();
            civil.shifts.plan = Some(guards.into_iter().skip(1).step_by(2).collect());
        }
        let plan = civil.shifts.plan.clone().unwrap_or_default();
        for id in plan {
            if civil.shifts.leaving.contains_key(&id) || civil.responders.contains_key(&id) {
                continue;
            }
            let Some(a) = world.actors.get_mut(&id).filter(|a| a.alive) else { continue };
            let Some(spawn) = a.spawn_point() else { continue };
            let feet = a.pose.feet;
            let Some(b) = a.brain.as_mut().filter(|b| b.arrest.is_none() && !b.fighting()) else { continue };
            let route = crate::daily_life::town().and_then(|t| t.nav.route(feet, barracks)).unwrap_or_else(|| vec![barracks]);
            b.set_route(route, false);
            b.set_post(barracks, 0.0, 0.0);
            civil.shifts.leaving.insert(id, (spawn, now));
        }
        // Дошли до казармы (или долго идут, а рядом никого) — легли спать.
        let leaving: Vec<(u64, NpcSpawn, f64)> = civil.shifts.leaving.iter().map(|(id, (s, t))| (*id, *s, *t)).collect();
        for (id, spawn, since) in leaving {
            let Some(a) = world.actors.get(&id) else {
                civil.shifts.leaving.remove(&id);
                continue;
            };
            let feet = a.pose.feet;
            if (feet.distance(barracks) < 4.0 || now - since > 150.0) && !near_player(world, feet) {
                civil.shifts.leaving.remove(&id);
                world.remove_npc(server, id);
                civil.shifts.asleep.push(spawn);
            }
        }
    } else if !civil.shifts.asleep.is_empty() || !civil.shifts.leaving.is_empty() {
        // Утро: смена встаёт и идёт по постам.
        civil.shifts.plan = None;
        for (id, (spawn, _)) in std::mem::take(&mut civil.shifts.leaving) {
            if let Some(b) = world.actors.get_mut(&id).and_then(|a| a.brain.as_mut()) {
                b.set_post(spawn.pos, spawn.yaw, spawn.wander);
                b.route.clear();
            }
        }
        if near_player(world, barracks) {
            return;
        }
        for spawn in std::mem::take(&mut civil.shifts.asleep) {
            let id = crate::server::spawn_npc(world, level, spawn, now);
            if let Some(a) = world.actors.get_mut(&id) {
                a.pose.feet = crate::npc::find_free_spot(level, barracks);
                if let Some(b) = a.brain.as_mut() {
                    let route = crate::daily_life::town().and_then(|t| t.nav.route(barracks, spawn.pos)).unwrap_or_else(|| vec![spawn.pos]);
                    b.set_route(route, false);
                }
            }
        }
    }
}

fn end_arrest(civil: &mut CivilWorld, world: &mut GameWorld, server: &mut RenetServer, player: u64) {
    civil.talked.remove(&player);
    if let Some(a) = civil.arrests.remove(&player) {
        if let Some(b) = world.actors.get_mut(&a.guard).and_then(|g| g.brain.as_mut()) {
            b.arrest = None;
        }
        if a.halted {
            send_law(server, player, LawMessage::HaltEnd);
        }
    }
}

/// Разыскиваемый не подчинился: гарнизон рядом берётся за оружие. Это новое преступление:
/// свидетели — стражники вокруг. Перебьёте всех и уйдёте — никто не донесёт.
fn resist(civil: &mut CivilWorld, world: &mut GameWorld, level: &LevelColliders, server: &mut RenetServer, player: u64, guard: u64, sight: Sight) {
    end_arrest(civil, world, server, player);
    civil.law_provoked.insert(player);
    let at = world.actors.get(&guard).map_or(Vec3::ZERO, |g| g.pose.feet);
    let mut seen = vec![Witness::Actor(guard)];
    for (id, a) in world.actors.iter_mut() {
        if a.pose.feet.distance(at) < 40.0
            && let Some(b) = a.brain.as_mut().filter(|b| b.faction == Some(Faction::Garrison))
        {
            b.provoke(player);
            if *id != guard && a.pose.feet.distance(at) < 25.0 {
                seen.push(Witness::Actor(*id));
            }
        }
    }
    open_incident(civil, world, player, Crime::Violence(Violence::Resisting), Faction::Garrison, None, at, seen);
    let _ = (level, sight);
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
    fn darkness_hides_thieves() {
        let day = Sight { dark: 0.0, fog: 0.0, rain: 0.0, now: 0.0 };
        let night = Sight { dark: 1.0, ..day };
        assert!(day.range(SIGHT_DAY, 0.0, false) > 20.0);
        assert!(night.range(SIGHT_DAY, 0.0, false) < 7.0);
        // У факела видно и ночью; пригнувшись — ближе.
        assert!(night.range(SIGHT_DAY, 1.0, false) > 20.0);
        assert!(night.range(SIGHT_DAY, 0.0, true) < night.range(SIGHT_DAY, 0.0, false));
        let foggy = Sight { fog: 1.0, ..day };
        assert!(foggy.range(SIGHT_DAY, 0.0, false) < 10.0);
    }

    #[test]
    fn thieves_are_seen_only_from_the_front() {
        let level = crate::level::level_colliders();
        let at = crate::level::spawn_point(0) + Vec3::Y * 1.4;
        let eye = at + Vec3::new(0.0, 0.4, -6.0);
        // Смотрит на +Z (к вору) — видит; отвернулся — нет; кулак в драке слышно и спиной.
        let toward = crate::daily_life::yaw_to(eye, at);
        if !walls_clear(&level, eye, at) {
            return;
        }
        assert!(notices(&level, eye, toward, at, 20.0, Look::Theft));
        assert!(!notices(&level, eye, toward + std::f32::consts::PI, at, 20.0, Look::Theft));
        assert!(notices(&level, eye, toward + std::f32::consts::PI, at, 20.0, Look::Violence));
        assert!(!notices(&level, eye, toward, at, 4.0, Look::Theft));
    }

    #[test]
    fn militia_ids_map_back_to_residents() {
        let id = MILITIA_ID_BASE + (7 << 16) + 123;
        assert_eq!(militia_resident(id), Some(123));
        assert_eq!(militia_resident(5), None);
        assert!(MILITIA_ID_BASE > (1 << 62) + 1_000_000, "не пересекается с обычными NPC");
        // Пахарь — с острогой или рогатиной, ремесленник — с топором; всё это есть в игре.
        assert_eq!(militia_arms(Role::Farmer, 1).0, WeaponKind::Spear);
        assert_eq!(militia_arms(Role::Craftsman, 1).0, WeaponKind::Axe);
        for role in [Role::Farmer, Role::Craftsman, Role::Sailor, Role::Householder, Role::Soldier] {
            for i in 0..8 {
                let item = militia_arms(role, i).1;
                assert!(crate::items::ItemBaseId::find(item).is_some(), "нет предмета «{item}»");
            }
        }
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
