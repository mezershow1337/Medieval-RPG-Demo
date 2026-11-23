//! Сервер: знания игроков, чтение книг, сбор трав и уроки у мастеров.
//!
//! # API для других частей игры (торговля, город)
//!
//! ```ignore
//! // Урок или смена работы у мастера: знание продвигается по программе мастера,
//! // игрок получает уведомление. Плату и зарплату ведёт вызывающий.
//! commands.trigger(Lesson { player, teacher: Teacher::Iatros, hours: 1.0, mode: LessonMode::Apprentice });
//! // Выдать знание сразу.
//! commands.trigger(GrantLore { player, lore: Lore::Read(Language::Greek) });
//! // Чему мастер научит следующим (None — учить нечему, денег не брать).
//! knowledge_world.next_lesson(player, Teacher::Scribe);
//! ```
//!
//! Уроки в лавках торговли приходят событием `trade_server::LessonRequested` и переводятся
//! в [`Lesson`]; если платному ученику учить нечего — `trade_server::RefundLesson`.

use crate::collision::LevelColliders;
use crate::economy::{NUMMI_PER_FOLLIS, format_money, give_money, take_money};
use crate::combat::Action;
use crate::inventory::{Inventory, Target};
use crate::items::{ItemBaseId, ItemUid, Quality, Rng, roll_item};
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::brewing::{self, BrewSpec, Vessel};
use crate::lore::*;
use crate::medicine::{self, Body, Treated};
use crate::medicine_server::Bodies;
use crate::lore_net::*;
use crate::protocol::{decode, encode};
use crate::server::GameWorld;
use bevy::prelude::*;
use crate::trade_server::{LessonRequested, RefundLesson};
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::HashMap;

/// Падение здоровья за кадр больше этого — по нам попали (кровотечение за кадр куда меньше).
pub const HIT_THRESHOLD: f32 = 0.5;
/// Секунд реального времени на час урока у мастера.
pub const LESSON_SECS: f32 = 12.0;
/// Сбор: секунд на траву и на корень (корни выкапывать дольше).
const GATHER_SECS: f32 = 2.5;
const DIG_SECS: f32 = 4.0;
/// Сколько раз можно собрать с одного места и за сколько секунд отрастает одна порция.
const PATCH_CHARGES: u8 = 3;
const REGROW_SECS: f32 = 90.0;
/// Сорванный одиночный кустик вырастает где-нибудь рядом через столько секунд.
const WILD_RESPAWN_SECS: (f32, f32) = (60.0, 150.0);
/// Сколько жаровен и столов может поставить один игрок.
const MAX_STATIONS: usize = 3;

pub struct KnowledgeServerPlugin;

impl Plugin for KnowledgeServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::medicine_server::MedicineServerPlugin)
            .insert_resource(KnowledgeWorld::default())
            .add_observer(on_connection)
            .add_observer(on_lesson)
            .add_observer(on_grant)
            .add_observer(on_trade_lesson)
            .add_systems(Startup, setup_world)
            .add_systems(Update, (debug_kit, receive_lore_messages, tick_activities, regrow_patches).chain());
    }
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

/// Час урока (или работы подмастерьем) у мастера.
#[derive(Event, Clone, Debug)]
pub struct Lesson {
    pub player: u64,
    pub teacher: Teacher,
    pub hours: f32,
    pub mode: LessonMode,
}

/// Выдать знание сразу (награда за поручение, подслушанный рецепт и т. п.).
#[derive(Event, Clone, Debug)]
pub struct GrantLore {
    pub player: u64,
    pub lore: Lore,
}

// ---------------------------------------------------------------------------
// Состояние
// ---------------------------------------------------------------------------

pub struct Patch {
    pub substance: Substance,
    pub pos: Vec3,
    pub charges: u8,
    /// Сколько можно собрать с полной заросли.
    pub max_charges: u8,
    /// Радиус заросли (у временных кустов — полметра).
    pub radius: f32,
    regrow: f32,
    draw: bool,
    /// Одиночный кустик в поле: центр и радиус «родной» заросли, около которой он вырастает.
    home: Option<(Vec3, f32)>,
    respawn: f32,
}

/// Поставленная на землю жаровня или стол.
pub struct Station {
    pub kind: StationKind,
    pub pos: Vec3,
    pub yaw: f32,
    pub owner: u64,
    /// Предмет, который вернётся в сумку.
    item: crate::items::Item,
}

pub struct Master {
    pub teacher: Teacher,
    pub pos: Vec3,
    pub yaw: f32,
    pub price: u32,
    pub wage: u32,
}

/// Чем занят игрок.
#[derive(Clone, Debug)]
enum Task {
    Read { uid: ItemUid, text: Book },
    Gather { patch: u32 },
    Lesson { master: u32, mode: LessonMode },
    Treat { uid: ItemUid },
    Brew { vessel: ItemUid, spec: BrewSpec },
}

struct Activity {
    task: Task,
    elapsed: f32,
    total: f32,
    start: Vec3,
    last_hp: f32,
}

#[derive(Resource)]
pub struct KnowledgeWorld {
    pub players: HashMap<u64, Knowledge>,
    pub patches: HashMap<u32, Patch>,
    pub masters: HashMap<u32, Master>,
    pub stations: HashMap<u32, Station>,
    next_station: u32,
    activities: HashMap<u64, Activity>,
    rng: Rng,
}

impl Default for KnowledgeWorld {
    fn default() -> Self {
        Self { players: HashMap::new(), patches: HashMap::new(), masters: HashMap::new(), stations: HashMap::new(), next_station: 1, activities: HashMap::new(), rng: Rng::from_time() }
    }
}

#[allow(dead_code)] // API для торговли и города
impl KnowledgeWorld {
    pub fn knows(&self, player: u64, lore: Lore) -> bool {
        self.players.get(&player).is_some_and(|k| k.knows(lore))
    }

    pub fn next_lesson(&self, player: u64, teacher: Teacher) -> Option<Lore> {
        self.players.get(&player).and_then(|k| k.next_lesson(teacher))
    }

    fn patch_views(&self) -> Vec<PatchView> {
        let mut v: Vec<PatchView> =
            self.patches.iter().map(|(id, p)| PatchView { id: *id, substance: p.substance, pos: p.pos.into(), charges: p.charges, draw: p.draw, radius: p.radius }).collect();
        v.sort_by_key(|p| p.id);
        v
    }

    fn station_views(&self) -> Vec<StationView> {
        let mut v: Vec<StationView> = self.stations.iter().map(|(id, s)| StationView { id: *id, kind: s.kind, pos: s.pos.into(), yaw: s.yaw }).collect();
        v.sort_by_key(|s| s.id);
        v
    }

    /// Ближайшая жаровня или стол в пределах `reach` от ног.
    fn station_near(&self, feet: Vec3, reach: f32) -> Option<(u32, &Station)> {
        self.stations
            .iter()
            .map(|(id, s)| (Vec2::new(s.pos.x - feet.x, s.pos.z - feet.z).length(), *id, s))
            .filter(|(d, _, s)| *d <= reach && (s.pos.y - feet.y).abs() < 2.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id, s)| (id, s))
    }

    /// Есть ли рядом алхимический стол (точные весы, подставка, ровный жар).
    fn bench_near(&self, feet: Vec3) -> bool {
        self.stations.values().any(|s| s.kind == StationKind::Table && Vec2::new(s.pos.x - feet.x, s.pos.z - feet.z).length() <= HEARTH_REACH && (s.pos.y - feet.y).abs() < 2.0)
    }

    fn master_views(&self) -> Vec<MasterView> {
        let mut v: Vec<MasterView> = self
            .masters
            .iter()
            .map(|(id, m)| MasterView { id: *id, teacher: m.teacher, pos: m.pos.into(), yaw: m.yaw, price: m.price, wage: m.wage })
            .collect();
        v.sort_by_key(|m| m.id);
        v
    }
}

fn send(server: &mut RenetServer, player: u64, msg: &LoreServerMessage) {
    server.send_message(player, CH_LORE, encode(msg));
}

fn notice(server: &mut RenetServer, player: u64, text: impl Into<String>) {
    send(server, player, &LoreServerMessage::Notice(text.into()));
}

fn sync_knowledge(server: &mut RenetServer, world: &KnowledgeWorld, player: u64) {
    if let Some(k) = world.players.get(&player) {
        send(server, player, &LoreServerMessage::Knowledge(k.clone()));
    }
}

fn sync_inventory(server: &mut RenetServer, loot: &LootWorld, player: u64) {
    if let Some(inv) = loot.inventories.get(&player) {
        server.send_message(player, CH_LOOT, encode(&LootServerMessage::Inventory(inv.clone())));
    }
}

/// Дать предметы в сумку; что не влезло — вернуть.
fn give(inv: &mut Inventory, item: crate::items::Item) -> Option<crate::items::Item> {
    match inv.place(item, Target::Auto) {
        Ok(_) => None,
        Err(rest) => Some(rest),
    }
}

// ---------------------------------------------------------------------------
// Мир: временные места сбора и мастера
// ---------------------------------------------------------------------------

/// Заросли и мастера: в Херсонесе — по местам города, на других уровнях — у места появления.
fn setup_world(mut world: ResMut<KnowledgeWorld>, level: Res<LevelColliders>) {
    let origin = crate::level::spawn_point(0);
    let city = crate::level::herb_patches();
    for (i, h) in city.iter().enumerate() {
        let Some(s) = Substance::from_latin(h.herb) else { continue };
        let max = (PATCH_CHARGES as f32 + h.radius * 0.8).round().clamp(3.0, 12.0) as u8;
        world.patches.insert(i as u32 + 1, Patch { substance: s, pos: h.pos, charges: max, max_charges: max, radius: h.radius, regrow: 0.0, draw: false, home: None, respawn: 0.0 });
    }
    // За стенами вокруг каждой заросли растут одиночные кустики: сорвёшь — вырастет другой
    // где-нибудь поблизости. Так травы попадаются по всей округе, а не только в зарослях.
    let mut rng = Rng::from_time();
    let mut id = WILD_PATCH_BASE;
    for h in city {
        let Some(s) = Substance::from_latin(h.herb) else { continue };
        if crate::level::inside_city(h.pos.x, h.pos.z) {
            continue;
        }
        let n = if h.radius >= 7.0 { 3 } else { 2 };
        for _ in 0..n {
            let Some(pos) = wild_spot(&mut rng, &level, h.pos, h.radius) else { continue };
            let respawn = rng.range_f32(WILD_RESPAWN_SECS.0, WILD_RESPAWN_SECS.1);
            world.patches.insert(id, Patch { substance: s, pos, charges: 1, max_charges: 1, radius: 0.35, regrow: 0.0, draw: true, home: Some((h.pos, h.radius)), respawn });
            id += 1;
        }
    }
    if city.is_empty() {
        place_demo_patches(&mut world, &level, origin);
    }
    // Врача, ризотома, аптекаря и прочих ведёт торговля (лавки в trade.rs). Здесь — только те,
    // кого там нет: церковный писец (грамота) и деревенская знахарка. Цены в нуммиях.
    let masters = [(Teacher::Scribe, 2 * NUMMI_PER_FOLLIS, 10, "church_west"), (Teacher::Wisewoman, NUMMI_PER_FOLLIS, 0, "goth_village")];
    for (i, (teacher, price, wage, place)) in masters.into_iter().enumerate() {
        let pos = crate::level::place_pos(place).unwrap_or(origin + Vec3::new(-2.0 + i as f32 * 4.0, 0.0, 5.0));
        let pos = crate::npc::find_free_spot(&level, pos);
        world.masters.insert(i as u32 + 1, Master { teacher, pos, yaw: 0.0, price, wage });
    }
}

/// Случайное место для кустика около заросли: за стенами, на суше, где можно стоять.
fn wild_spot(rng: &mut Rng, level: &LevelColliders, center: Vec3, radius: f32) -> Option<Vec3> {
    for _ in 0..16 {
        let a = rng.f32() * std::f32::consts::TAU;
        let d = rng.range_f32(radius * 0.9, radius * 2.5 + 14.0);
        let (x, z) = (center.x + a.cos() * d, center.z + a.sin() * d);
        if crate::level::inside_city(x, z) {
            continue;
        }
        let y = crate::level::ground_y(x, z);
        if y < crate::level::WATER_Y + 0.4 || (y - center.y).abs() > 12.0 {
            continue;
        }
        let p = crate::npc::find_free_spot(level, Vec3::new(x, y, z));
        if Vec2::new(p.x - x, p.z - z).length() > 0.5 || p.y - y > 0.7 {
            continue; // в стене, в доме или на скале
        }
        return Some(Vec3::new(x, y, z));
    }
    None
}

/// Временные кусты полукругом у места появления (уровень без своих зарослей).
fn place_demo_patches(world: &mut KnowledgeWorld, level: &LevelColliders, origin: Vec3) {
    let herbs = [
        Substance::Plantain,
        Substance::Yarrow,
        Substance::Sage,
        Substance::Thyme,
        Substance::StJohnsWort,
        Substance::Mallow,
        Substance::Henbane,
        Substance::Poppy,
        Substance::Wormwood,
        Substance::Mint,
        Substance::Comfrey,
        Substance::Juniper,
    ];
    for (i, s) in herbs.into_iter().enumerate() {
        // Полукруг перед воротами, в паре метров друг от друга.
        let a = -1.2 + i as f32 * (2.4 / (herbs.len() - 1) as f32);
        let pos = origin + Vec3::new(a.sin() * 9.0, 0.0, -a.cos() * 9.0 + 2.0);
        let pos = crate::npc::find_free_spot(level, pos);
        world.patches.insert(i as u32 + 1, Patch { substance: s, pos: Vec3::new(pos.x, pos.y.max(0.0), pos.z), charges: PATCH_CHARGES, max_charges: PATCH_CHARGES, radius: 0.5, regrow: 0.0, draw: true, home: None, respawn: 0.0 });
    }
}

fn on_connection(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, mut world: ResMut<KnowledgeWorld>) {
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let id = *client_id;
            world.players.insert(id, Knowledge::default());
            sync_knowledge(&mut server, &world, id);
            send(&mut server, id, &LoreServerMessage::Patches(world.patch_views()));
            send(&mut server, id, &LoreServerMessage::Masters(world.master_views()));
            send(&mut server, id, &LoreServerMessage::Stations(world.station_views()));
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            world.players.remove(client_id);
            world.activities.remove(client_id);
        }
    }
}

fn learned_message(learned: &[Lore]) -> Option<String> {
    if learned.is_empty() {
        return None;
    }
    let names: Vec<String> = learned.iter().map(|l| l.name()).collect();
    Some(format!("Вы узнали: {}", names.join("; ")))
}

fn on_lesson(event: On<Lesson>, mut server: ResMut<RenetServer>, mut world: ResMut<KnowledgeWorld>) {
    let ev = event.event().clone();
    let Some(k) = world.players.get_mut(&ev.player) else { return };
    let before = k.next_lesson(ev.teacher);
    let learned = k.study(ev.teacher, ev.mode, ev.hours);
    match (learned_message(&learned), before) {
        (Some(msg), _) => notice(&mut server, ev.player, msg),
        (None, Some(lore)) => {
            let p = k.lessons.get(&lore).copied().unwrap_or(0.0);
            notice(&mut server, ev.player, format!("{}: выучено {:.0}%", lore.name(), p * 100.0));
        }
        (None, None) => notice(&mut server, ev.player, format!("{} больше ничему не может вас научить", ev.teacher.name())),
    }
    sync_knowledge(&mut server, &world, ev.player);
}

fn on_grant(event: On<GrantLore>, mut server: ResMut<RenetServer>, mut world: ResMut<KnowledgeWorld>) {
    let ev = event.event().clone();
    let Some(k) = world.players.get_mut(&ev.player) else { return };
    if k.learn(ev.lore) {
        notice(&mut server, ev.player, format!("Вы узнали: {}", ev.lore.name()));
        sync_knowledge(&mut server, &world, ev.player);
    }
}

/// Урок или смена у мастера из торговли (trade_server). Если учить нечему — деньги назад.
fn on_trade_lesson(event: On<LessonRequested>, mut commands: Commands, world: Res<KnowledgeWorld>) {
    let ev = event.event();
    let teacher = teacher_from_trade(ev.teacher);
    let mode = match ev.mode {
        crate::trade::LessonMode::Paid => LessonMode::Paid,
        crate::trade::LessonMode::Apprentice => LessonMode::Apprentice,
    };
    if mode == LessonMode::Paid && world.next_lesson(ev.player, teacher).is_none() {
        if ev.fee > 0 {
            commands.trigger(RefundLesson {
                player: ev.player,
                fee: ev.fee,
                reason: format!("{}: «Мне больше нечему тебя учить»", teacher.name()),
            });
        }
        return;
    }
    // Подмастерью смена засчитывается всегда: даже когда новых знаний нет, растёт навык.
    commands.trigger(Lesson { player: ev.player, teacher, hours: ev.hours, mode });
}

/// Мастера торговли называются так же, как наши.
pub fn teacher_from_trade(t: crate::trade::TeacherKind) -> Teacher {
    use crate::trade::TeacherKind as K;
    match t {
        K::Iatros => Teacher::Iatros,
        K::Rhizotomos => Teacher::Rhizotomos,
        K::Apothecary => Teacher::Apothecary,
        K::Glassblower => Teacher::Glassblower,
        K::Potter => Teacher::Potter,
        K::Dyer => Teacher::Dyer,
        K::Alchemist => Teacher::Alchemist,
        K::Grammarian => Teacher::Grammarian,
        K::SteppeHealer => Teacher::SteppeHealer,
    }
}

// ---------------------------------------------------------------------------
// Занятия
// ---------------------------------------------------------------------------

fn in_reach(feet: Vec3, target: Vec3) -> bool {
    let d = target - feet;
    Vec2::new(d.x, d.z).length() <= LORE_REACH + 0.4 && d.y.abs() < 2.0
}

fn start(server: &mut RenetServer, world: &mut KnowledgeWorld, player: u64, task: Task, verb: String, done: f32, total: f32, feet: Vec3, hp: f32) {
    world.activities.insert(player, Activity { task, elapsed: done, total, start: feet, last_hp: hp });
    send(server, player, &LoreServerMessage::Activity { verb, done, total });
}

fn receive_lore_messages(
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    mut world: ResMut<KnowledgeWorld>,
    mut loot: ResMut<LootWorld>,
    bodies: Res<Bodies>,
    level: Res<LevelColliders>,
) {
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_LORE) {
            let Some(msg) = decode::<LoreClientMessage>(&bytes) else { continue };
            let Some(actor) = game.actors.get(&pid).filter(|a| a.alive && !a.fighter.is_dead()) else { continue };
            let (feet, hp) = (actor.pose.feet, actor.fighter.hp);
            if let LoreClientMessage::Stop = msg {
                if world.activities.remove(&pid).is_some() {
                    send(&mut server, pid, &LoreServerMessage::ActivityEnded);
                }
                continue;
            }
            if world.activities.contains_key(&pid) {
                notice(&mut server, pid, "Вы уже чем-то заняты");
                continue;
            }
            if !matches!(actor.fighter.action, Action::Idle) {
                continue;
            }
            if let LoreClientMessage::PlaceStation { .. } | LoreClientMessage::TakeStation { .. } | LoreClientMessage::UpgradeStation { .. } = msg {
                station_message(&mut server, &mut world, &mut loot, &level, pid, &actor.pose, msg);
                continue;
            }
            let Some(k) = world.players.get(&pid) else { continue };
            match msg {
                LoreClientMessage::Read { uid } => {
                    let Some(base) = loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).map(|(_, i)| i.base) else { continue };
                    let Some(text) = Book::from_item(base) else {
                        notice(&mut server, pid, "Это нельзя читать");
                        continue;
                    };
                    if let Err(e) = k.can_read(text) {
                        notice(&mut server, pid, e.message());
                        continue;
                    }
                    let done = k.read.get(&text).copied().unwrap_or(0.0);
                    if done >= 1.0 && text != Book::HerbalFragment {
                        notice(&mut server, pid, "Вы это уже прочли");
                        continue;
                    }
                    let total = text.info().read_secs;
                    start(&mut server, &mut world, pid, Task::Read { uid, text }, "Чтение".into(), done * total, total, feet, hp);
                }
                LoreClientMessage::Gather { patch } => {
                    let Some(p) = world.patches.get(&patch) else { continue };
                    let d = p.pos - feet;
                    if Vec2::new(d.x, d.z).length() > p.radius + LORE_REACH * 0.5 || d.y.abs() > 3.0 {
                        notice(&mut server, pid, "Слишком далеко");
                        continue;
                    }
                    if p.charges == 0 {
                        notice(&mut server, pid, "Здесь уже всё собрано, нужно подождать");
                        continue;
                    }
                    let root = p.substance.info().unknown.contains("корень");
                    let (verb, total) = if root { ("Выкапываете корень", DIG_SECS) } else { ("Собираете", GATHER_SECS) };
                    start(&mut server, &mut world, pid, Task::Gather { patch }, verb.into(), 0.0, total, feet, hp);
                }
                LoreClientMessage::PaidLesson { master } | LoreClientMessage::Apprentice { master } => {
                    let paid = matches!(msg, LoreClientMessage::PaidLesson { .. });
                    let Some(m) = world.masters.get(&master) else { continue };
                    if !in_reach(feet, m.pos) {
                        notice(&mut server, pid, "Подойдите ближе");
                        continue;
                    }
                    let Some(next) = k.next_lesson(m.teacher) else {
                        notice(&mut server, pid, format!("{}: «Мне больше нечему тебя учить»", m.teacher.name()));
                        continue;
                    };
                    if paid {
                        let loot = &mut *loot;
                        let Some(inv) = loot.inventories.get_mut(&pid) else { continue };
                        if let Err(e) = take_money(inv, m.price, &mut loot.uids) {
                            notice(&mut server, pid, format!("Урок стоит {}. {}", format_money(m.price), e.message()));
                            continue;
                        }
                        sync_inventory(&mut server, loot, pid);
                    }
                    let (mode, verb) = if paid {
                        (LessonMode::Paid, format!("Урок: {}", next.name().to_lowercase()))
                    } else {
                        (LessonMode::Apprentice, apprentice_chore(m.teacher).to_string())
                    };
                    start(&mut server, &mut world, pid, Task::Lesson { master, mode }, verb, 0.0, LESSON_SECS, feet, hp);
                }
                LoreClientMessage::Treat { uid } => {
                    let Some(base) = loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).map(|(_, i)| i.base) else { continue };
                    let healthy = Body::default();
                    let body = bodies.bodies.get(&pid).unwrap_or(&healthy);
                    match medicine::plan(body, base.base().name) {
                        Some((verb, secs)) => start(&mut server, &mut world, pid, Task::Treat { uid }, verb.into(), 0.0, secs, feet, hp),
                        None if medicine::is_remedy(base.base().name) => notice(&mut server, pid, "Нечего лечить"),
                        None => notice(&mut server, pid, "Это не лекарство"),
                    }
                }
                LoreClientMessage::Brew { vessel, spec } => {
                    let Some(inv) = loot.inventories.get(&pid) else { continue };
                    match check_brew(inv, vessel, &spec).and_then(|v| need_fire(&spec, feet, &world, inv).map(|_| v)) {
                        Ok(v) => {
                            let what = match v {
                                Vessel::Mortar => "растираете",
                                Vessel::Crucible => "обжигаете",
                                v if v.still() => "перегоняете",
                                _ if spec.heat == brewing::Heat::None => "настаиваете",
                                _ => "варите",
                            };
                            let verb = format!("{}: {what}", v.name());
                            let secs = spec.real_secs();
                            start(&mut server, &mut world, pid, Task::Brew { vessel, spec }, verb, 0.0, secs, feet, hp);
                        }
                        Err(e) => notice(&mut server, pid, e),
                    }
                }
                LoreClientMessage::Stop | LoreClientMessage::PlaceStation { .. } | LoreClientMessage::TakeStation { .. } | LoreClientMessage::UpgradeStation { .. } => {}
            }
        }
    }
}

/// Поставить, забрать или достроить жаровню / стол.
fn station_message(server: &mut RenetServer, world: &mut KnowledgeWorld, loot: &mut LootWorld, level: &LevelColliders, pid: u64, pose: &crate::combat::Pose, msg: LoreClientMessage) {
    let feet = pose.feet;
    let Some(inv) = loot.inventories.get_mut(&pid) else { return };
    match msg {
        LoreClientMessage::PlaceStation { uid } => {
            let Some(kind) = inv.find(uid).and_then(|(_, i)| StationKind::from_item(i.info().name)) else { return };
            if world.stations.values().filter(|s| s.owner == pid).count() >= MAX_STATIONS {
                notice(server, pid, format!("Больше {MAX_STATIONS} жаровен и столов не уследить: заберите какую-нибудь"));
                return;
            }
            let want = feet + pose.forward_flat() * 1.3;
            let p = crate::npc::find_free_spot(level, want);
            if Vec2::new(p.x - want.x, p.z - want.z).length() > 0.6 || p.y - want.y > 0.7 {
                notice(server, pid, "Здесь не поставить: мешает стена или уступ");
                return;
            }
            if world.station_near(want, 1.0).is_some() {
                notice(server, pid, "Здесь уже что-то стоит");
                return;
            }
            let Some((item, _)) = inv.take(uid) else { return };
            let id = world.next_station;
            world.next_station += 1;
            world.stations.insert(id, Station { kind, pos: Vec3::new(want.x, feet.y, want.z), yaw: pose.yaw, owner: pid, item });
            let text = match kind {
                StationKind::Brazier => "Жаровня поставлена. Варить на ней можно где угодно, но нужен древесный уголь",
                StationKind::Table => "Стол поставлен: жаровня, весы и подставка под рукой",
            };
            notice(server, pid, text);
        }
        LoreClientMessage::TakeStation { id } => {
            let Some(s) = world.stations.get(&id) else { return };
            if !in_reach(feet, s.pos) {
                notice(server, pid, "Подойдите ближе");
                return;
            }
            let s = world.stations.remove(&id).expect("есть");
            if let Some(rest) = give(inv, s.item) {
                notice(server, pid, "Нет места в сумке");
                world.stations.insert(id, Station { item: rest, ..s });
                return;
            }
            notice(server, pid, format!("Вы забрали: {}", s.kind.item().to_lowercase()));
        }
        LoreClientMessage::UpgradeStation { id } => {
            let Some(s) = world.stations.get(&id) else { return };
            if !in_reach(feet, s.pos) {
                notice(server, pid, "Подойдите ближе");
                return;
            }
            if s.kind != StationKind::Brazier {
                return;
            }
            let missing: Vec<&str> = TABLE_PARTS.iter().copied().filter(|n| count_named(inv, n) == 0).collect();
            if !missing.is_empty() {
                notice(server, pid, format!("Для стола не хватает: {}", missing.join(", ").to_lowercase()));
                return;
            }
            for n in TABLE_PARTS {
                take_named(inv, n, 1);
            }
            let Some(base) = ItemBaseId::find(StationKind::Table.item()) else { return };
            let item = roll_item(base, Quality::Common, 1, &mut world.rng, &mut loot.uids);
            let s = world.stations.get_mut(&id).expect("есть");
            s.kind = StationKind::Table;
            s.item = item;
            notice(server, pid, "Вы собрали алхимический стол: весы точнее, сосуды не пляшут на огне");
        }
        _ => return,
    }
    sync_inventory(server, loot, pid);
    server.broadcast_message(CH_LORE, encode(&LoreServerMessage::Stations(world.station_views())));
}

/// Что нужно, кроме жаровни, чтобы собрать стол.
pub const TABLE_PARTS: [&str; 2] = ["Весы с гирями", "Ступка с пестиком"];

/// Грязная работа подмастерья.
fn apprentice_chore(teacher: Teacher) -> &'static str {
    match teacher {
        Teacher::Iatros => "Носите воду, стираете повязки, держите больных",
        Teacher::Rhizotomos | Teacher::Wisewoman | Teacher::SteppeHealer => "Перебираете и сушите травы",
        Teacher::Apothecary => "Толчёте корни в ступке",
        Teacher::Alchemist => "Топите печь и моете сосуды",
        Teacher::Dyer => "Мешаете чан и полощете ткань",
        Teacher::Glassblower | Teacher::Potter => "Носите дрова к печи",
        Teacher::Scribe | Teacher::Grammarian => "Растираете чернила и чините перья",
    }
}

#[allow(clippy::too_many_arguments)]
fn tick_activities(
    mut commands: Commands,
    time: Res<Time>,
    mut server: ResMut<RenetServer>,
    mut game: ResMut<GameWorld>,
    mut world: ResMut<KnowledgeWorld>,
    mut loot: ResMut<LootWorld>,
    mut bodies: ResMut<Bodies>,
) {
    let dt = time.delta_secs();
    let mut ended: Vec<(u64, bool)> = Vec::new();
    for (pid, a) in world.activities.iter_mut() {
        let Some(actor) = game.actors.get(pid) else {
            ended.push((*pid, false));
            continue;
        };
        let f = &actor.fighter;
        let moved = Vec2::new(actor.pose.feet.x - a.start.x, actor.pose.feet.z - a.start.z).length() > 0.6;
        if !actor.alive || f.is_dead() || f.hp < a.last_hp - HIT_THRESHOLD || !matches!(f.action, Action::Idle) || moved {
            ended.push((*pid, false));
            continue;
        }
        a.last_hp = f.hp;
        a.elapsed += dt;
        if a.elapsed >= a.total {
            ended.push((*pid, true));
        }
    }
    // Чтение сохраняет прогресс даже при прерывании.
    let reads: Vec<(u64, Book, f32)> =
        world.activities.iter().filter_map(|(pid, a)| if let Task::Read { text, .. } = a.task { Some((*pid, text, (a.elapsed / a.total).min(1.0))) } else { None }).collect();
    for (pid, text, frac) in reads {
        if let Some(k) = world.players.get_mut(&pid) {
            let r = k.read.entry(text).or_insert(0.0);
            *r = r.max(frac);
        }
    }

    for (pid, ok) in ended {
        let Some(a) = world.activities.remove(&pid) else { continue };
        send(&mut server, pid, &LoreServerMessage::ActivityEnded);
        if !ok {
            if let Task::Read { .. } = a.task {
                notice(&mut server, pid, "Чтение прервано, место запомнили");
                sync_knowledge(&mut server, &world, pid);
            }
            continue;
        }
        match a.task {
            Task::Read { uid, text } => finish_reading(&mut server, &mut world, &mut loot, pid, uid, text),
            Task::Gather { patch } => finish_gathering(&mut server, &mut world, &mut loot, pid, patch),
            Task::Treat { uid } => finish_treatment(&mut server, &mut world, &mut loot, &mut game, &mut bodies, pid, uid),
            Task::Brew { vessel, spec } => {
                let feet = game.actors.get(&pid).map(|a| a.pose.feet).unwrap_or_default();
                finish_brewing(&mut server, &mut commands, &mut world, &mut loot, &mut bodies, pid, vessel, &spec, feet)
            }
            Task::Lesson { master, mode } => {
                let Some(m) = world.masters.get(&master) else { continue };
                let (teacher, wage) = (m.teacher, m.wage);
                // Подмастерью платят гроши медью.
                if mode == LessonMode::Apprentice && wage > 0 {
                    let loot = &mut *loot;
                    if let Some(inv) = loot.inventories.get_mut(&pid)
                        && give_money(inv, wage, &mut loot.uids).is_ok()
                    {
                        notice(&mut server, pid, format!("За работу вам дали {}", format_money(wage)));
                    }
                    sync_inventory(&mut server, loot, pid);
                }
                // Сам урок: то же, что событие Lesson.
                let Some(k) = world.players.get_mut(&pid) else { continue };
                let before = k.next_lesson(teacher);
                let learned = k.study(teacher, mode, 1.0);
                let msg = match (learned_message(&learned), before) {
                    (Some(m), _) => m,
                    (None, Some(lore)) => format!("{}: выучено {:.0}%", lore.name(), k.lessons.get(&lore).copied().unwrap_or(0.0) * 100.0),
                    (None, None) => "Вы хорошо поработали".into(),
                };
                notice(&mut server, pid, msg);
                sync_knowledge(&mut server, &world, pid);
            }
        }
    }
}

fn finish_reading(server: &mut RenetServer, world: &mut KnowledgeWorld, loot: &mut LootWorld, pid: u64, uid: ItemUid, text: Book) {
    // Книгу могли выбросить во время чтения.
    if loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).is_none() {
        return;
    }
    let mut learned = Vec::new();
    if text == Book::HerbalFragment {
        // Одна случайная незнакомая трава; ветхий папирус рассыпается.
        let k = world.players.get(&pid).cloned().unwrap_or_default();
        let unknown: Vec<Substance> = Substance::ALL.into_iter().filter(|s| s.is_wild() && !(k.knows(Lore::Identify(*s)) && k.knows(Lore::Properties(*s)))).collect();
        if let Some(i) = (!unknown.is_empty()).then(|| (world.rng.next_u64() % unknown.len() as u64) as usize) {
            let s = unknown[i];
            for lore in [Lore::Identify(s), Lore::Properties(s)] {
                if let Some(k) = world.players.get_mut(&pid)
                    && k.learn(lore)
                {
                    learned.push(lore);
                }
            }
        }
        if let Some(inv) = loot.inventories.get_mut(&pid) {
            inv.take(uid);
        }
        sync_inventory(server, loot, pid);
        if let Some(k) = world.players.get_mut(&pid) {
            k.read.remove(&text);
        }
        if learned.is_empty() {
            notice(server, pid, "Ничего нового. Ветхий папирус рассыпался.");
        } else {
            notice(server, pid, format!("{} Ветхий папирус рассыпался.", learned_message(&learned).unwrap()));
        }
    } else if let Some(k) = world.players.get_mut(&pid) {
        k.read.insert(text, 1.0);
        for lore in text.teaches() {
            if k.learn(lore) {
                learned.push(lore);
            }
        }
        // Книга даёт теорию, а не руку: навык чуть-чуть.
        if let Some(skill) = text.teaches().iter().find_map(|l| l.skill()) {
            k.practice(skill, 1.0);
        }
        let msg = match learned.len() {
            0 => format!("Вы дочитали «{}», но нового не узнали.", text.info().item),
            n if n > 3 => format!("Вы дочитали «{}» и узнали {} новых вещей. Смотрите в окне знаний (K).", text.info().item, n),
            _ => learned_message(&learned).unwrap(),
        };
        notice(server, pid, msg);
    }
    sync_knowledge(server, world, pid);
}

fn finish_gathering(server: &mut RenetServer, world: &mut KnowledgeWorld, loot: &mut LootWorld, pid: u64, patch: u32) {
    let Some(p) = world.patches.get(&patch) else { return };
    if p.charges == 0 {
        return;
    }
    let substance = p.substance;
    let k = world.players.get(&pid).cloned().unwrap_or_default();
    let skilled = k.knows(Lore::Technique(Technique::Gathering));
    let skill = k.skill(Skill::Herbalism);
    // Знающий собирает больше и не губит корни; неумеха может повредить куст.
    let mut amount = 1u16;
    if skilled {
        amount += 1;
    }
    if world.rng.f32() < skill / 100.0 {
        amount += 1;
    }
    let damage = if !skilled && world.rng.f32() < 0.35 { 2 } else { 1 };
    let Some(base) = substance.item() else { return };
    let Some(inv) = loot.inventories.get_mut(&pid) else { return };
    let item = roll_item(base, Quality::Common, amount, &mut world.rng, &mut loot.uids);
    if let Some(rest) = give(inv, item) {
        if rest.count == amount {
            notice(server, pid, "Нет места в сумке");
            return;
        }
    }
    let p = world.patches.get_mut(&patch).unwrap();
    p.charges = p.charges.saturating_sub(damage);
    let charges = p.charges;
    server.broadcast_message(CH_LORE, encode(&LoreServerMessage::PatchCharges { id: patch, charges }));
    if let Some(k) = world.players.get_mut(&pid) {
        k.practice(Skill::Herbalism, 0.8);
    }
    let name = k.item_name(base);
    let mut msg = format!("Собрано: {name} ×{amount}");
    if damage > 1 {
        msg.push_str(". Вы неумело вырвали куст с корнем");
    }
    notice(server, pid, msg);
    sync_inventory(server, loot, pid);
    sync_knowledge(server, world, pid);
}

fn regrow_patches(time: Res<Time>, mut server: ResMut<RenetServer>, mut world: ResMut<KnowledgeWorld>, level: Res<LevelColliders>) {
    let dt = time.delta_secs();
    let KnowledgeWorld { patches, rng, .. } = &mut *world;
    for (id, p) in patches.iter_mut() {
        if p.charges >= p.max_charges {
            p.regrow = 0.0;
            continue;
        }
        p.regrow += dt;
        // Сорванный кустик в поле вырастает на новом месте около своей заросли.
        if let Some((center, radius)) = p.home {
            if p.regrow >= p.respawn {
                p.regrow = 0.0;
                p.respawn = rng.range_f32(WILD_RESPAWN_SECS.0, WILD_RESPAWN_SECS.1);
                if let Some(pos) = wild_spot(rng, &level, center, radius) {
                    p.pos = pos;
                }
                p.charges = 1;
                server.broadcast_message(CH_LORE, encode(&LoreServerMessage::PatchMoved { id: *id, pos: p.pos.into(), charges: 1 }));
            }
            continue;
        }
        if p.regrow >= REGROW_SECS {
            p.regrow = 0.0;
            p.charges += 1;
            server.broadcast_message(CH_LORE, encode(&LoreServerMessage::PatchCharges { id: *id, charges: p.charges }));
        }
    }
}

/// Лечение закончено: применить средство, списать предмет (и бинт к припарке).
fn finish_treatment(server: &mut RenetServer, world: &mut KnowledgeWorld, loot: &mut LootWorld, game: &mut GameWorld, bodies: &mut Bodies, pid: u64, uid: ItemUid) {
    let Some(item) = loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).map(|(_, i)| i.clone()) else { return };
    let Some(k) = world.players.get_mut(&pid) else { return };
    let name = item.info().name;
    let bandage = ItemBaseId::find("Бинт");
    let has_bandage = name != "Бинт" && loot.inventories.get(&pid).is_some_and(|inv| inv.all_items().any(|i| Some(i.base) == bandage));
    let roll = bodies.rng.f32();
    let result = medicine::apply(bodies.body(pid), k, name, item.quality.index(), has_bandage, roll);
    bodies.touch(pid);
    let take_one = |loot: &mut LootWorld, uid: ItemUid| {
        if let Some(inv) = loot.inventories.get_mut(&pid)
            && let Some((mut it, loc)) = inv.take(uid)
            && it.count > 1
        {
            it.count -= 1;
            let _ = inv.place(it, loc.into());
        }
    };
    let text = match &result {
        Treated::Wound { text, bandage_used } => {
            take_one(loot, uid);
            if *bandage_used && let Some(b) = loot.inventories.get(&pid).and_then(|inv| inv.all_items().find(|i| Some(i.base) == bandage).map(|i| i.uid)) {
                take_one(loot, b);
            }
            // Перевязка даёт передышку и немного сил, как и раньше.
            if name == "Бинт"
                && let (Some(c), Some(a)) = (item.base.consumable(), game.actors.get_mut(&pid))
            {
                a.fighter.hp = (a.fighter.hp + c.heal).min(a.fighter.max_hp);
            }
            k.practice(Skill::Medicine, 0.6);
            text.clone()
        }
        Treated::Drunk { text } => {
            take_one(loot, uid);
            text.clone()
        }
        Treated::Extraction { ok, damage, text } => {
            if let Some(a) = game.actors.get_mut(&pid) {
                a.fighter.hp = (a.fighter.hp - damage).max(1.0);
            }
            k.practice(Skill::Medicine, if *ok { 1.5 } else { 0.5 });
            text.clone()
        }
        Treated::Nothing(text) => text.clone(),
    };
    notice(server, pid, text);
    sync_knowledge(server, world, pid);
    if result.consumes_item() {
        sync_inventory(server, loot, pid);
    }
}

/// Сколько штук предмета с таким именем в сумке.
fn count_named(inv: &Inventory, name: &str) -> u32 {
    inv.all_items().filter(|i| i.info().name == name).map(|i| i.count as u32).sum()
}

/// Забрать столько-то штук по имени (из разных стопок).
fn take_named(inv: &mut Inventory, name: &str, mut n: u32) {
    let stacks: Vec<ItemUid> = inv.all_items().filter(|i| i.info().name == name).map(|i| i.uid).collect();
    for uid in stacks {
        if n == 0 {
            break;
        }
        let Some((mut item, loc)) = inv.take(uid) else { continue };
        let used = (item.count as u32).min(n);
        n -= used;
        item.count -= used as u16;
        if item.count > 0 {
            let _ = inv.place(item, loc.into());
        }
    }
}

/// На огне варят у очага, костра, жаровни или стола (если на уровне есть очаги).
pub const HEARTH_REACH: f32 = 3.0;
pub const COAL: &str = "Древесный уголь";

/// Откуда огонь.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Fire {
    /// Огонь не нужен или есть очаг (дрова хозяйские).
    Free,
    /// Своя жаровня: жжёт уголь.
    Brazier { coal: u32 },
}

/// Сколько кусков угля сгорит: час слабого жара — четверть куска, сильного — больше половины.
pub fn coal_needed(spec: &BrewSpec) -> u32 {
    let per_hour = match spec.heat {
        brewing::Heat::None => return 0,
        brewing::Heat::Low => 0.25,
        brewing::Heat::Medium => 0.4,
        brewing::Heat::High => 0.65,
    };
    ((spec.hours * per_hour).ceil() as u32).clamp(1, 30)
}

fn need_fire(spec: &BrewSpec, feet: Vec3, world: &KnowledgeWorld, inv: &Inventory) -> Result<Fire, String> {
    let hearths = crate::level::hearths();
    if spec.heat == brewing::Heat::None || (hearths.is_empty() && world.stations.is_empty()) {
        return Ok(Fire::Free);
    }
    if hearths.iter().any(|h| Vec2::new(h.x - feet.x, h.z - feet.z).length() <= HEARTH_REACH && (h.y - feet.y).abs() < 2.5) {
        return Ok(Fire::Free);
    }
    if world.station_near(feet, HEARTH_REACH).is_some() {
        let coal = coal_needed(spec);
        let have = count_named(inv, COAL);
        if have < coal {
            return Err(format!("Жаровне нужен древесный уголь: {coal} шт., у вас {have}"));
        }
        return Ok(Fire::Brazier { coal });
    }
    if hearths.is_empty() {
        return Ok(Fire::Free);
    }
    Err("Для огня нужен очаг, костёр или жаровня рядом".into())
}

/// Можно ли начать варку: сосуд и всё положенное есть в сумке.
fn check_brew(inv: &Inventory, vessel: ItemUid, spec: &BrewSpec) -> Result<Vessel, String> {
    let v = inv.find(vessel).and_then(|(_, i)| Vessel::from_item(i.info().name)).ok_or("Нет сосуда")?;
    if spec.items.is_empty() {
        return Err("Сосуд пуст".into());
    }
    if spec.water > brewing::MAX_WATER || !(0.1..=96.0).contains(&spec.hours) {
        return Err("Так не сварить".into());
    }
    for (name, n) in &spec.items {
        if brewing::mass(name).is_none() {
            return Err(format!("«{name}» в сосуд не кладут"));
        }
        if *n == 0 || count_named(inv, name) < *n as u32 {
            return Err(format!("Не хватает: {name}"));
        }
    }
    Ok(v)
}

#[allow(clippy::too_many_arguments)]
fn finish_brewing(
    server: &mut RenetServer,
    commands: &mut Commands,
    world: &mut KnowledgeWorld,
    loot: &mut LootWorld,
    bodies: &mut Bodies,
    pid: u64,
    vessel: ItemUid,
    spec: &BrewSpec,
    feet: Vec3,
) {
    let Some(inv) = loot.inventories.get(&pid) else { return };
    let v = match check_brew(inv, vessel, spec) {
        Ok(v) => v,
        Err(e) => {
            notice(server, pid, e);
            return;
        }
    };
    let fire = need_fire(spec, feet, world, inv);
    let bench = world.bench_near(feet);
    let Some(k) = world.players.get_mut(&pid) else { return };
    let Some(inv) = loot.inventories.get_mut(&pid) else { return };
    let fire = match fire {
        Ok(f) => f,
        Err(e) => {
            notice(server, pid, e);
            return;
        }
    };
    let flasks = count_named(inv, brewing::FLASK).min(u16::MAX as u32) as u16;
    let roll = bodies.rng.f32();
    let result = brewing::brew(spec, v, k, flasks, roll, bench);
    // Ничего не вышло (например, некуда разлить) — всё остаётся в сумке.
    if result.product.is_none() && !result.vessel_broken {
        notice(server, pid, result.text);
        return;
    }
    for (name, n) in &spec.items {
        take_named(inv, name, *n as u32);
    }
    take_named(inv, brewing::FLASK, result.flasks_used as u32);
    if let Fire::Brazier { coal } = fire {
        take_named(inv, COAL, coal);
    }
    if result.vessel_broken || result.vessel_used {
        let _ = inv.take(vessel);
    }
    if let Some((name, count, q)) = result.product
        && let Some(base) = ItemBaseId::find(name)
    {
        let quality = if base.base().has_quality { Quality::ALL[q.min(5)] } else { Quality::Common };
        let item = roll_item(base, quality, count, &mut world.rng, &mut loot.uids);
        let inv = loot.inventories.get_mut(&pid).expect("есть");
        if let Some(rest) = give(inv, item) {
            commands.trigger(crate::loot_server::SpawnGroundItem { pos: feet + Vec3::Y * 0.8, vel: Vec3::new(0.0, 1.0, 0.0), item: rest });
            notice(server, pid, "Не влезло в сумку — поставили на землю");
        }
    }
    if result.burn > 0.0 {
        bodies.body(pid).burn(result.burn);
        bodies.touch(pid);
    }
    if let Some((toxin, doses)) = result.poison {
        bodies.body(pid).poison(toxin, doses);
        bodies.touch(pid);
    }
    if let Some(t) = result.technique {
        let q = result.product.map_or(0, |p| p.2) as f32;
        k.practice(t.skill(), 0.8 + q * 0.3);
    }
    notice(server, pid, result.text);
    sync_knowledge(server, world, pid);
    sync_inventory(server, loot, pid);
}

/// Для проверки: `MRPG_DEBUG_KIT=1` — каждый вошедший получает горшок, ступку, травы и флаконы.
fn debug_kit(mut server: ResMut<RenetServer>, mut loot: ResMut<LootWorld>, mut given: Local<std::collections::HashSet<u64>>, mut rng: Local<Option<Rng>>) {
    if std::env::var("MRPG_DEBUG_KIT").is_err() {
        return;
    }
    let rng = rng.get_or_insert_with(Rng::from_time);
    let loot = &mut *loot;
    let ids: Vec<u64> = loot.inventories.keys().copied().filter(|id| !given.contains(id)).collect();
    for id in ids {
        given.insert(id);
        let kit = [
            ("Глиняный горшок", 1),
            ("Ступка с пестиком", 1),
            ("Шалфей", 3),
            ("Тысячелистник", 2),
            ("Подорожник", 2),
            ("Пустой флакон", 4),
            ("Воск", 1),
            ("Кувшин оливкового масла", 1),
            ("Мирра", 1),
            ("Бинт", 2),
            ("Кувшин вина", 1),
            ("Жаровня", 1),
            ("Древесный уголь", 6),
            ("Весы с гирями", 1),
        ];
        for (name, n) in kit {
            let Some(base) = ItemBaseId::find(name) else { continue };
            let item = roll_item(base, Quality::Common, n, rng, &mut loot.uids);
            if let Some(inv) = loot.inventories.get_mut(&id) {
                let _ = give(inv, item);
            }
        }
        sync_inventory(&mut server, loot, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(heat: brewing::Heat, hours: f32) -> BrewSpec {
        BrewSpec { items: vec![("Шалфей".into(), 1)], water: 2, heat, hours }
    }

    #[test]
    fn brazier_burns_more_coal_on_strong_fire() {
        assert_eq!(coal_needed(&spec(brewing::Heat::None, 10.0)), 0);
        assert_eq!(coal_needed(&spec(brewing::Heat::Low, 1.0)), 1);
        assert!(coal_needed(&spec(brewing::Heat::High, 8.0)) > coal_needed(&spec(brewing::Heat::Low, 8.0)));
        assert!(coal_needed(&spec(brewing::Heat::High, 96.0)) <= 30);
    }

    /// Дикие кустики растут за стенами, на суше и там, где можно стоять; их много по округе.
    #[test]
    fn wild_herbs_grow_outside_the_walls() {
        if crate::level::herb_patches().is_empty() {
            return;
        }
        let level = crate::level::level_colliders();
        let mut world = KnowledgeWorld::default();
        // Тот же код, что и при запуске сервера.
        let mut app = App::new();
        app.insert_resource(level.clone()).insert_resource(KnowledgeWorld::default()).add_systems(Startup, setup_world);
        app.update();
        std::mem::swap(&mut world, &mut *app.world_mut().resource_mut::<KnowledgeWorld>());
        let wild: Vec<&Patch> = world.patches.iter().filter(|(id, _)| **id >= WILD_PATCH_BASE).map(|(_, p)| p).collect();
        assert!(wild.len() >= 40, "диких кустиков мало: {}", wild.len());
        let mut rng = Rng::new(7);
        for p in &wild {
            assert!(!crate::level::inside_city(p.pos.x, p.pos.z), "{:?}", p.pos);
            assert!(p.pos.y > crate::level::WATER_Y, "{:?}", p.pos);
            let (center, radius) = p.home.unwrap();
            // Переезд на новое место тоже остаётся около заросли.
            let moved = wild_spot(&mut rng, &level, center, radius).expect("место нашлось");
            assert!(Vec2::new(moved.x - center.x, moved.z - center.z).length() <= radius * 2.5 + 14.5);
        }
    }
}
