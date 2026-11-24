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
use crate::work::{Drill, Effort};
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
        app.add_plugins((crate::medicine_server::MedicineServerPlugin, crate::skills_server::SkillsServerPlugin))
            .insert_resource(KnowledgeWorld::default())
            .add_observer(on_connection)
            .add_observer(on_lesson)
            .add_observer(on_grant)
            .add_observer(on_trade_lesson)
            .add_systems(Startup, setup_world)
            .add_systems(Update, (debug_kit, update_masters, receive_lore_messages, tick_fires, tick_activities, regrow_patches).chain());
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
    /// Житель города, который учит (номер в `CivilWorld::people`): ходит по распорядку.
    pub resident: Option<usize>,
    /// Часы работы (игровые).
    pub hours: (f32, f32),
    /// Сейчас на месте и учит.
    pub present: bool,
    pub place: &'static str,
}

/// Чем занят игрок.
#[derive(Clone, Debug)]
enum Task {
    Read { uid: ItemUid, text: Book },
    Gather { patch: u32 },
    Lesson { master: u32, mode: LessonMode, subject: Option<Lore> },
    /// Переписка книги: калам, чернила, пергамент.
    Copy { uid: ItemUid, text: Book },
    /// Упражнения на своей восковой табличке.
    Practice,
    Treat { uid: ItemUid, target: Option<medicine::CareTarget> },
    Brew { vessel: ItemUid, spec: BrewSpec },
}

struct Activity {
    task: Task,
    elapsed: f32,
    total: f32,
    start: Vec3,
    last_hp: f32,
    /// Сосуд на огне — его видят все (пар, дым, капли).
    work: Option<WorkView>,
    /// Огонь под сосудом, за которым надо следить.
    tend: Option<Tend>,
    /// Работа руками (см. work.rs): без неё занятие не движется.
    effort: Option<Effort>,
    /// Когда снова сказать игроку, сколько сделано.
    send_in: f32,
}

/// Сколько секунд можно не мешать горшок на огне, прежде чем дело встанет.
const STIR_GRACE: f32 = 2.6;
/// Брошенный без помешивания горшок на среднем огне и выше начинает пригорать.
const SCORCH_AFTER: f32 = 5.0;
/// Насколько один качок мехов (качество 1) раздувает угли под перегонным прибором.
const PUMP_FIRE: f32 = 0.16;

/// Огонь во время варки: угли прогорают, порыв ветра раздувает; купол перегонного
/// прибора греется, пока его не остудят мокрой тряпицей.
struct Tend {
    fire: f32,
    target: f32,
    dome: Option<f32>,
    bath: bool,
    time: f32,
    over: f32,
    under: f32,
    dome_peak: f32,
    send_in: f32,
    cool_ready: f32,
    band: i32,
}

/// Жар по шкале 0..3.
fn heat_level(h: brewing::Heat) -> f32 {
    match h {
        brewing::Heat::None => 0.0,
        brewing::Heat::Low => 1.0,
        brewing::Heat::Medium => 2.0,
        brewing::Heat::High => 3.0,
    }
}

fn heat_of_level(l: i32) -> brewing::Heat {
    match l {
        i32::MIN..=0 => brewing::Heat::None,
        1 => brewing::Heat::Low,
        2 => brewing::Heat::Medium,
        _ => brewing::Heat::High,
    }
}

/// Угли прогорают, жар падает.
const FIRE_DECAY: f32 = 0.035;
/// Насколько мехи раздувают и заслонка сбивает жар.
const FIRE_STEP: f32 = 0.45;
const FIRE_MAX: f32 = 3.6;

impl Tend {
    fn new(spec: &BrewSpec, still: bool) -> Self {
        let target = heat_level(spec.heat);
        Tend {
            fire: target,
            target,
            dome: still.then_some(0.0),
            bath: spec.bath,
            time: 0.0,
            over: 0.0,
            under: 0.0,
            dome_peak: 0.0,
            send_in: 0.0,
            cool_ready: 0.0,
            band: target as i32,
        }
    }

    fn tick(&mut self, dt: f32, rng: &mut Rng) {
        self.fire -= FIRE_DECAY * dt;
        // Порыв ветра или вспыхнувший уголь.
        if rng.f32() < 0.05 * dt {
            self.fire += 0.55;
        }
        self.fire = self.fire.clamp(0.0, FIRE_MAX);
        self.time += dt;
        if !self.bath && self.fire > self.target + 0.75 {
            self.over += dt;
        }
        if self.fire < self.target - 0.75 {
            self.under += dt;
        }
        if let Some(d) = self.dome.as_mut() {
            *d = (*d + ((self.fire - 1.4).max(0.0) * 0.07 - 0.015) * dt).max(0.0);
            self.dome_peak = self.dome_peak.max(*d);
        }
        self.cool_ready -= dt;
        self.send_in -= dt;
    }

    fn view(&self) -> TendView {
        TendView { fire: self.fire, target: self.target, dome: self.dome, bath: self.bath }
    }

    fn log(&self) -> brewing::FireLog {
        let t = self.time.max(0.01);
        brewing::FireLog { over: self.over / t, under: self.under / t, dome_peak: self.dome_peak }
    }
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
            .map(|(id, m)| MasterView {
                id: *id,
                teacher: m.teacher,
                pos: m.pos.into(),
                yaw: m.yaw,
                price: m.price,
                wage: m.wage,
                resident: m.resident.map(|i| i as u32),
                hours: m.hours,
                present: m.present,
                place: m.place.to_string(),
            })
            .collect();
        v.sort_by_key(|m| m.id);
        v
    }
}

fn send(server: &mut RenetServer, player: u64, msg: &LoreServerMessage) {
    server.send_message(player, CH_LORE, encode(msg));
}

pub(crate) fn notice(server: &mut RenetServer, player: u64, text: impl Into<String>) {
    send(server, player, &LoreServerMessage::Notice(text.into()));
}

pub(crate) fn sync_knowledge(server: &mut RenetServer, world: &KnowledgeWorld, player: u64) {
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
fn setup_world(mut world: ResMut<KnowledgeWorld>, level: Res<LevelColliders>, civil: Option<Res<crate::civil_server::CivilWorld>>) {
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
    // Учителя грамоты (school.rs): школа в дидаскалионе — жители города, нотарий и сочинитель — на местах.
    let masters = [(Teacher::Scribe, 2 * NUMMI_PER_FOLLIS, 10, "church_west", (6.0, 18.5)), (Teacher::Wisewoman, NUMMI_PER_FOLLIS, 0, "goth_village", (0.0, 24.0))];
    let mut id = 1;
    for (i, (teacher, price, wage, place, hours)) in masters.into_iter().enumerate() {
        let pos = crate::level::place_pos(place).unwrap_or(origin + Vec3::new(-2.0 + i as f32 * 4.0, 0.0, 5.0));
        let pos = crate::npc::find_free_spot(&level, pos);
        world.masters.insert(id, Master { teacher, pos, yaw: 0.0, price, wage, resident: None, hours, present: true, place });
        id += 1;
    }
    let people: Vec<crate::school::Townsman> =
        civil.as_ref().map(|c| c.people.iter().map(|p| crate::school::Townsman { role: p.role, pos: p.pos, yaw: p.yaw }).collect()).unwrap_or_default();
    for t in crate::school::tutors(&people) {
        let pos = if t.resident.is_some() { t.pos } else { crate::npc::find_free_spot(&level, t.pos) };
        // Стоит лицом к середине места (к агоре, ко входу в преторию).
        let yaw = match (t.resident, crate::level::place_pos(t.place)) {
            (None, Some(c)) if (c - pos).length() > 0.5 => crate::daily_life::yaw_to(pos, c),
            _ => t.yaw,
        };
        world.masters.insert(id, Master { teacher: t.teacher, pos, yaw, price: t.price, wage: t.wage, resident: t.resident, hours: t.hours, present: false, place: t.place });
        id += 1;
    }
}

/// Учителя приходят и уходят: житель-учитель ходит по распорядку, остальные — по часам работы.
fn update_masters(
    mut server: ResMut<RenetServer>,
    mut world: ResMut<KnowledgeWorld>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
    civil: Option<Res<crate::civil_server::CivilWorld>>,
) {
    let hour = clock.map_or(10.0, |c| c.hour());
    let mut changed = false;
    for m in world.masters.values_mut() {
        let present = match (m.resident, civil.as_ref()) {
            (Some(i), Some(c)) => match c.people.get(i) {
                Some(p) => {
                    m.pos = p.pos;
                    m.yaw = p.yaw;
                    p.dead_until.is_none() && p.rallied.is_none() && crate::school::on_duty(m.hours, hour, Some(p.act))
                }
                None => false,
            },
            (Some(_), None) => false,
            (None, _) => crate::school::on_duty(m.hours, hour, None),
        };
        if present != m.present {
            m.present = present;
            changed = true;
        }
    }
    if changed {
        let views = world.master_views();
        server.broadcast_message(CH_LORE, encode(&LoreServerMessage::Masters(views)));
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
            for (player, a) in &world.activities {
                if let Some(work) = a.work {
                    send(&mut server, id, &LoreServerMessage::Work { player: *player, work: Some(work) });
                }
            }
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            world.players.remove(client_id);
            if let Some(a) = world.activities.remove(client_id) {
                end_work(&mut server, *client_id, &a);
            }
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
        K::Smith => Teacher::Smith,
        K::Armourer => Teacher::Armourer,
        K::GothSmith => Teacher::GothSmith,
        K::SteppeSmith => Teacher::SteppeSmith,
        K::Copyist => Teacher::Copyist,
        K::Philosopher => Teacher::Philosopher,
    }
}

// ---------------------------------------------------------------------------
// Занятия
// ---------------------------------------------------------------------------

fn in_reach(feet: Vec3, target: Vec3) -> bool {
    let d = target - feet;
    Vec2::new(d.x, d.z).length() <= LORE_REACH + 0.4 && d.y.abs() < 2.0
}

/// Мехи, заслонка, мокрая тряпица.
/// Игрок закончил движение руками.
fn stroke(server: &mut RenetServer, world: &mut KnowledgeWorld, pid: u64, q: f32) {
    let Some(a) = world.activities.get_mut(&pid) else { return };
    let at: Vec3 = a.work.map_or(a.start, |w| w.pos.into());
    let Some(e) = a.effort.as_mut() else { return };
    if !e.stroke(q) {
        return;
    }
    // Мехи под перегонным прибором или тиглем: каждый качок раздувает угли.
    if e.drill == Drill::Pump
        && let Some(t) = a.tend.as_mut()
    {
        t.fire = (t.fire + PUMP_FIRE * q.clamp(0.0, 1.0)).min(FIRE_MAX);
        if q >= crate::work::MISS {
            burst(server, at, BurstFx::Bellows);
        }
        let view = t.view();
        send(server, pid, &LoreServerMessage::Tending(view));
    }
}

fn tend_action(server: &mut RenetServer, world: &mut KnowledgeWorld, loot: &LootWorld, pid: u64, action: TendAction) {
    let has_cloth = loot.inventories.get(&pid).is_some_and(|inv| count_named(inv, brewing::CLOTH) > 0);
    let Some(a) = world.activities.get_mut(&pid) else { return };
    let at: Vec3 = a.work.map_or(a.start, |w| w.pos.into());
    let Some(t) = a.tend.as_mut() else { return };
    match action {
        TendAction::Bellows if a.effort.as_ref().is_some_and(|e| e.drill == Drill::Pump) => {
            notice(server, pid, "Мехи качают руками: мышь вверх-вниз, в такт");
            return;
        }
        TendAction::Bellows => {
            t.fire = (t.fire + FIRE_STEP).min(FIRE_MAX);
            burst(server, at, BurstFx::Bellows);
        }
        TendAction::Damper => t.fire = (t.fire - FIRE_STEP).max(0.0),
        TendAction::CoolDome => {
            let Some(d) = t.dome.as_mut() else { return };
            if !has_cloth {
                notice(server, pid, "Нечем остудить купол: нужна мокрая тряпица (отрез льна)");
                return;
            }
            if t.cool_ready > 0.0 {
                return;
            }
            *d = (*d - 0.45).max(0.0);
            t.cool_ready = 0.6;
            burst(server, at + Vec3::Y * 0.25, BurstFx::Hiss);
        }
    }
    let view = t.view();
    send(server, pid, &LoreServerMessage::Tending(view));
}

/// Огонь под сосудами: прогорает, гуляет; игроку — как он идёт, всем — сила пламени.
fn tick_fires(time: Res<Time>, mut server: ResMut<RenetServer>, mut world: ResMut<KnowledgeWorld>) {
    let dt = time.delta_secs();
    let KnowledgeWorld { activities, rng, .. } = &mut *world;
    for (pid, a) in activities.iter_mut() {
        let Some(t) = a.tend.as_mut() else { continue };
        t.tick(dt, rng);
        if t.send_in <= 0.0 {
            t.send_in = 0.25;
            send(&mut server, *pid, &LoreServerMessage::Tending(t.view()));
        }
        // Пламя под сосудом видно всем: сменилась сила — обновить.
        let band = t.fire.round() as i32;
        if band != t.band {
            t.band = band;
            if let Some(w) = a.work.as_mut() {
                w.heat = if t.bath { brewing::Heat::Low } else { heat_of_level(band) };
                let work = *w;
                server.broadcast_message(CH_LORE, encode(&LoreServerMessage::Work { player: *pid, work: Some(work) }));
            }
        }
    }
}

/// Работа у сосуда кончилась — убрать сосуд и пар у всех.
fn end_work(server: &mut RenetServer, pid: u64, a: &Activity) {
    if a.work.is_some() {
        server.broadcast_message(CH_LORE, encode(&LoreServerMessage::Work { player: pid, work: None }));
    }
}

fn burst(server: &mut RenetServer, pos: Vec3, fx: BurstFx) {
    server.broadcast_message(CH_LORE, encode(&LoreServerMessage::Burst { pos: pos.into(), fx }));
}

/// Где стоит сосуд: на очаге, на жаровне, на столе или на земле перед игроком.
fn work_view(world: &KnowledgeWorld, spec: &BrewSpec, vessel: Vessel, feet: Vec3, yaw: f32) -> WorkView {
    let fire = spec.heat != brewing::Heat::None;
    let forward = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    let view = |pos: Vec3, yaw: f32, own_fire: bool| WorkView { vessel, heat: spec.heat, pos: pos.into(), yaw, own_fire };
    if fire
        && let Some(h) = crate::level::hearths().iter().filter(|h| Vec2::new(h.x - feet.x, h.z - feet.z).length() <= HEARTH_REACH && (h.y - feet.y).abs() < 2.5).min_by(|a, b| a.distance(feet).total_cmp(&b.distance(feet)))
    {
        return view(*h + Vec3::Y * 0.3, yaw, false);
    }
    if let Some((_, s)) = world.station_near(feet, HEARTH_REACH) {
        let rot = Quat::from_rotation_y(s.yaw);
        match (s.kind, fire) {
            (StationKind::Brazier, true) => return view(s.pos + Vec3::Y * 0.5, s.yaw, false),
            (StationKind::Table, true) => return view(s.pos + rot * Vec3::new(0.45, 1.12, 0.0), s.yaw, false),
            (StationKind::Table, false) => return view(s.pos + rot * Vec3::new(-0.02, 0.85, 0.14), s.yaw, false),
            (StationKind::Brazier, false) => {}
        }
    }
    view(feet + forward * 0.65, yaw, fire)
}

#[allow(clippy::too_many_arguments)]
fn start(server: &mut RenetServer, world: &mut KnowledgeWorld, player: u64, task: Task, verb: String, done: f32, total: f32, feet: Vec3, hp: f32, drill: Option<Drill>, skill: f32) {
    let effort = drill.map(|d| {
        let mut e = Effort::new(d, total);
        // Дочитывают с того места, где остановились.
        e.done = e.need * (done / total.max(0.01)).clamp(0.0, 1.0);
        e
    });
    world.activities.insert(player, Activity { task, elapsed: done, total, start: feet, last_hp: hp, work: None, tend: None, effort, send_in: 0.0 });
    let skill = (skill / 100.0).clamp(0.0, 1.0);
    send(server, player, &LoreServerMessage::Activity { verb, done, total, drill, skill });
}

fn receive_lore_messages(
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    mut world: ResMut<KnowledgeWorld>,
    mut loot: ResMut<LootWorld>,
    bodies: Res<Bodies>,
    level: Res<LevelColliders>,
    clock: Option<Res<crate::world_clock::WorldClock>>,
) {
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_LORE) {
            let Some(msg) = decode::<LoreClientMessage>(&bytes) else { continue };
            let Some(actor) = game.actors.get(&pid).filter(|a| a.alive && !a.fighter.is_dead()) else { continue };
            let (feet, hp) = (actor.pose.feet, actor.fighter.hp);
            if let LoreClientMessage::Stop = msg {
                if let Some(a) = world.activities.remove(&pid) {
                    end_work(&mut server, pid, &a);
                    send(&mut server, pid, &LoreServerMessage::ActivityEnded);
                }
                continue;
            }
            if let LoreClientMessage::Tend(action) = msg {
                tend_action(&mut server, &mut world, &loot, pid, action);
                continue;
            }
            if let LoreClientMessage::Stroke { q } = msg {
                stroke(&mut server, &mut world, pid, q);
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
                    // Книжный человек читает быстрее.
                    let total = text.info().read_secs / crate::skill_tree::Mods::of(k).reading;
                    start(&mut server, &mut world, pid, Task::Read { uid, text }, "Чтение".into(), done * total, total, feet, hp, Some(Drill::Track), 30.0);
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
                    let (verb, total, drill) = if root { ("Выкапываете корень", DIG_SECS, Drill::Pump) } else { ("Срываете осторожно, не губя куст", GATHER_SECS, Drill::Tension) };
                    let skill = k.skill(Skill::Herbalism);
                    start(&mut server, &mut world, pid, Task::Gather { patch }, verb.into(), 0.0, total, feet, hp, Some(drill), skill);
                }
                LoreClientMessage::PaidLesson { master } | LoreClientMessage::Apprentice { master } | LoreClientMessage::Study { master, .. } => {
                    let paid = !matches!(msg, LoreClientMessage::Apprentice { .. });
                    let subject = if let LoreClientMessage::Study { lore, .. } = msg { Some(lore) } else { None };
                    let Some(m) = world.masters.get(&master) else { continue };
                    if !in_reach(feet, m.pos) {
                        notice(&mut server, pid, "Подойдите ближе");
                        continue;
                    }
                    if !m.present {
                        let hour = clock.as_ref().map_or(10.0, |c| c.hour());
                        notice(&mut server, pid, crate::school::away_line(m.teacher, hour, m.hours));
                        continue;
                    }
                    if let Some(lore) = subject
                        && !k.can_study(m.teacher, lore)
                    {
                        let why = if k.knows(lore) {
                            "Это вы уже знаете".to_string()
                        } else if let Some(r) = lore.requires().into_iter().find(|r| !k.knows(*r)) {
                            format!("{}: «Сначала выучи: {}»", m.teacher.name(), r.subject().to_lowercase())
                        } else {
                            format!("{} этому не учит", m.teacher.name())
                        };
                        notice(&mut server, pid, why);
                        continue;
                    }
                    let Some(next) = subject.or_else(|| k.next_lesson(m.teacher)) else {
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
                    let can_write = k.knows(Lore::Write(Language::Greek));
                    let (mode, verb, drill) = if paid {
                        let drill = crate::work::lesson_drill(next);
                        let verb = match drill {
                            Drill::Mimic => format!("Урок: {} — повторяйте за мастером", next.name().to_lowercase()),
                            _ => format!("Урок: {} — {}", next.subject().to_lowercase(), lesson_verb(next)),
                        };
                        (LessonMode::Paid, verb, drill)
                    } else {
                        (LessonMode::Apprentice, apprentice_chore_for(m.teacher, can_write).to_string(), crate::work::chore_drill_for(m.teacher, can_write))
                    };
                    let skill = m.teacher.skill().map_or(0.0, |s| k.skill(s));
                    start(&mut server, &mut world, pid, Task::Lesson { master, mode, subject }, verb, 0.0, LESSON_SECS, feet, hp, Some(drill), skill);
                }
                LoreClientMessage::Copy { uid } => {
                    let Some(inv) = loot.inventories.get(&pid) else { continue };
                    let Some(text) = inv.find(uid).and_then(|(_, i)| Book::from_item(i.base)) else {
                        notice(&mut server, pid, "Переписывают книги и свитки");
                        continue;
                    };
                    match copy_check(k, inv, text) {
                        Err(why) => notice(&mut server, pid, why),
                        Ok(()) => {
                            let total = copy_secs(k, text);
                            let done = k.copied.get(&text).copied().unwrap_or(0.0).min(0.99);
                            let skill = k.skill(Skill::Letters);
                            start(&mut server, &mut world, pid, Task::Copy { uid, text }, format!("Переписываете «{}»", text.info().item), done * total, total, feet, hp, Some(Drill::Scribe), skill);
                        }
                    }
                }
                LoreClientMessage::Practice { uid } => {
                    let is_tablet = loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).is_some_and(|(_, i)| i.info().name == TABLET);
                    if !is_tablet {
                        continue;
                    }
                    if !k.knows(Lore::Alphabet) && !k.lessons.contains_key(&Lore::Alphabet) {
                        notice(&mut server, pid, "Вы не знаете ни одной буквы. Сначала — к учителю: в школу у дидаскалиона или к писцу.");
                        continue;
                    }
                    let (drill, verb) = if k.knows(Lore::Read(Language::Greek)) { (Drill::Scribe, "Пишете слова на табличке") } else { (Drill::Trace, "Выводите буквы на табличке") };
                    let skill = k.skill(Skill::Letters);
                    start(&mut server, &mut world, pid, Task::Practice, verb.into(), 0.0, 25.0, feet, hp, Some(drill), skill);
                }
                LoreClientMessage::Treat { uid } => {
                    let Some(base) = loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).map(|(_, i)| i.base) else { continue };
                    let healthy = Body::default();
                    let body = bodies.bodies.get(&pid).unwrap_or(&healthy);
                    match medicine::plan(body, base.base().name) {
                        Some((verb, secs)) => {
                            let drill = medicine::plan_drill(body, base.base().name, None);
                            let skill = k.skill(Skill::Medicine);
                            start(&mut server, &mut world, pid, Task::Treat { uid, target: None }, verb.into(), 0.0, secs, feet, hp, drill, skill)
                        }
                        None if medicine::is_remedy(base.base().name) => notice(&mut server, pid, "Нечего лечить"),
                        None => notice(&mut server, pid, "Это не лекарство"),
                    }
                }
                LoreClientMessage::TreatOn { uid, target } => {
                    let Some(base) = loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).map(|(_, i)| i.base) else { continue };
                    let healthy = Body::default();
                    let body = bodies.bodies.get(&pid).unwrap_or(&healthy);
                    match medicine::plan_to(body, base.base().name, target) {
                        Ok((verb, secs)) => {
                            let drill = medicine::plan_drill(body, base.base().name, Some(target));
                            let skill = k.skill(Skill::Medicine);
                            start(&mut server, &mut world, pid, Task::Treat { uid, target: Some(target) }, verb, 0.0, secs, feet, hp, drill, skill)
                        }
                        Err(why) => notice(&mut server, pid, why),
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
                            let work = work_view(&world, &spec, v, feet, actor.pose.yaw);
                            let (spec_heat, spec_copy) = (spec.heat, spec.clone());
                            let drill = crate::work::brew_drill(v, spec_heat);
                            let skill = k.skill(Skill::Alchemy).max(k.skill(Skill::Herbalism) * 0.5);
                            start(&mut server, &mut world, pid, Task::Brew { vessel, spec }, verb, 0.0, secs, feet, hp, Some(drill), skill);
                            let tend = (spec_heat != brewing::Heat::None).then(|| Tend::new(&spec_copy, v.still()));
                            if let Some(a) = world.activities.get_mut(&pid) {
                                a.work = Some(work);
                                a.tend = tend;
                            }
                            server.broadcast_message(CH_LORE, encode(&LoreServerMessage::Work { player: pid, work: Some(work) }));
                        }
                        Err(e) => notice(&mut server, pid, e),
                    }
                }
                LoreClientMessage::Stop | LoreClientMessage::Stroke { .. } | LoreClientMessage::Tend(_) | LoreClientMessage::PlaceStation { .. } | LoreClientMessage::TakeStation { .. } | LoreClientMessage::UpgradeStation { .. } => {}
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

/// Что делают на уроке грамоты (подпись к работе руками).
fn lesson_verb(lore: Lore) -> &'static str {
    match lore {
        Lore::Alphabet => "выводите буквы стилем по воску",
        Lore::Write(_) => "пишите слова под диктовку",
        Lore::Read(_) => "читайте вслух, ведите указку",
        Lore::Reckon => "раскладывайте камешки на абаке",
        _ => "повторяйте за учителем",
    }
}

/// Работа помощника у пишущего мастера: умеющему писать дают переписывать.
fn apprentice_chore_for(teacher: Teacher, can_write: bool) -> &'static str {
    match teacher {
        Teacher::Scribe | Teacher::Notary | Teacher::Copyist | Teacher::Logograph if can_write => "Переписываете грамоты набело",
        Teacher::Grammatist | Teacher::Grammarian if can_write => "Показываете малышам буквы на табличке",
        Teacher::Grammatist => "Разглаживаете воск на табличках, точите стили",
        t => apprentice_chore(t),
    }
}

/// Грязная работа подмастерья.
fn apprentice_chore(teacher: Teacher) -> &'static str {
    match teacher {
        Teacher::Iatros => "Носите воду, стираете повязки, держите больных",
        Teacher::Rhizotomos | Teacher::Wisewoman | Teacher::SteppeHealer => "Перебираете и сушите травы",
        Teacher::Apothecary => "Толчёте корни в ступке",
        Teacher::Alchemist => "Топите печь и моете сосуды",
        Teacher::Dyer => "Мешаете чан и полощете ткань",
        Teacher::Glassblower | Teacher::Potter => "Носите дрова к печи",
        Teacher::Scribe | Teacher::Grammarian | Teacher::Grammatist | Teacher::Notary | Teacher::Copyist | Teacher::Philosopher | Teacher::Logograph => "Растираете чернила и чините перья",
        Teacher::Smith | Teacher::Armourer | Teacher::GothSmith | Teacher::SteppeSmith => "Качаете меха, носите уголь, бьёте молотом-кувалдой",
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
    // Учитель ушёл (кончились уроки, пошёл обедать) — урок кончается.
    let masters: HashMap<u32, (bool, Vec3)> = world.masters.iter().map(|(id, m)| (*id, (m.present, m.pos))).collect();
    let mut left: Vec<u64> = Vec::new();
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
        if let Task::Lesson { master, .. } = a.task
            && masters.get(&master).is_none_or(|(here, at)| !here || Vec2::new(at.x - actor.pose.feet.x, at.z - actor.pose.feet.z).length() > LORE_REACH + 2.5)
        {
            ended.push((*pid, false));
            left.push(*pid);
            continue;
        }
        a.last_hp = f.hp;
        if let Some(e) = a.effort.as_mut() {
            e.tick(dt);
        }
        a.elapsed = match (&a.effort, a.tend.as_mut()) {
            // На огне время варки идёт, пока горит как надо и (в горшке) пока мешают.
            (Some(e), Some(t)) => {
                let lit = t.fire >= t.target - 0.75;
                let stirred = e.drill != Drill::Stir || e.idle() < STIR_GRACE;
                if e.drill == Drill::Stir && e.idle() > SCORCH_AFTER && t.fire > 1.5 && !t.bath {
                    // Брошенное варево пристаёт ко дну.
                    t.over += dt;
                }
                a.elapsed + dt * if stirred { 1.0 } else { 0.0 } * if lit { 1.0 } else { 0.3 }
            }
            // Без огня дело идёт только руками.
            (Some(e), None) => e.frac() * a.total,
            (None, _) => a.elapsed + dt,
        };
        if a.effort.is_some() {
            a.send_in -= dt;
            if a.send_in <= 0.0 {
                a.send_in = 0.2;
                send(&mut server, *pid, &LoreServerMessage::Progress((a.elapsed / a.total.max(0.01)).min(1.0)));
            }
        }
        if a.elapsed >= a.total {
            ended.push((*pid, true));
        }
    }
    // Чтение и переписка сохраняют прогресс даже при прерывании.
    let reads: Vec<(u64, Book, f32, bool)> = world
        .activities
        .iter()
        .filter_map(|(pid, a)| match a.task {
            Task::Read { text, .. } => Some((*pid, text, (a.elapsed / a.total).min(1.0), false)),
            Task::Copy { text, .. } => Some((*pid, text, (a.elapsed / a.total).min(1.0), true)),
            _ => None,
        })
        .collect();
    for (pid, text, frac, copy) in reads {
        if let Some(k) = world.players.get_mut(&pid) {
            let r = if copy { k.copied.entry(text).or_insert(0.0) } else { k.read.entry(text).or_insert(0.0) };
            *r = r.max(frac);
            // Чтение само по себе учит читать, а письмо — ещё лучше.
            let per_min = crate::skill_tree::PRACTICE_READ_PER_MIN * if copy { 1.5 } else { 1.0 };
            k.practice(Skill::Letters, dt / 60.0 * per_min);
        }
    }

    for (pid, ok) in ended {
        let Some(a) = world.activities.remove(&pid) else { continue };
        end_work(&mut server, pid, &a);
        send(&mut server, pid, &LoreServerMessage::ActivityEnded);
        if !ok {
            match a.task {
                Task::Read { .. } => {
                    notice(&mut server, pid, "Чтение прервано, место запомнили");
                    sync_knowledge(&mut server, &world, pid);
                }
                Task::Copy { .. } => {
                    notice(&mut server, pid, "Переписка прервана — место отмечено, допишете потом");
                    sync_knowledge(&mut server, &world, pid);
                }
                // Прерванный урок: что успели — то и выучили.
                Task::Lesson { master, mode, subject } => {
                    let frac = a.effort.as_ref().map_or(0.0, |e| e.frac());
                    let Some(teacher) = world.masters.get(&master).map(|m| m.teacher) else { continue };
                    let why = if left.contains(&pid) { format!("{} ушёл — урок окончен", teacher.name()) } else { "Урок прерван".to_string() };
                    if frac < 0.15 {
                        notice(&mut server, pid, why);
                        continue;
                    }
                    let q = a.effort.as_ref().map_or(0.5, |e| e.quality());
                    let Some(k) = world.players.get_mut(&pid) else { continue };
                    let learned = k.study_subject(teacher, subject, mode, frac * crate::work::lesson_mult(q));
                    let msg = learned_message(&learned).unwrap_or_else(|| "засчитана часть урока".into());
                    notice(&mut server, pid, format!("{why}: {msg}"));
                    sync_knowledge(&mut server, &world, pid);
                }
                _ => {}
            }
            continue;
        }
        let q = a.effort.as_ref().map(|e| e.quality());
        match a.task {
            Task::Read { uid, text } => finish_reading(&mut server, &mut world, &mut loot, pid, uid, text),
            Task::Gather { patch } => finish_gathering(&mut server, &mut world, &mut loot, pid, patch, a.start, q.unwrap_or(0.5)),
            Task::Treat { uid, target } => finish_treatment(&mut server, &mut world, &mut loot, &mut game, &mut bodies, pid, uid, target, q),
            Task::Brew { vessel, spec } => {
                let feet = game.actors.get(&pid).map(|a| a.pose.feet).unwrap_or_default();
                let at = a.work.map_or(feet, |w| w.pos.into());
                let fire = a.tend.as_ref().map(|t| t.log());
                finish_brewing(&mut server, &mut commands, &mut world, &mut loot, &mut bodies, pid, vessel, &spec, feet, at, fire, q)
            }
            Task::Copy { uid, text } => {
                let feet = game.actors.get(&pid).map(|a| a.pose.feet).unwrap_or_default();
                finish_copy(&mut server, &mut commands, &mut world, &mut loot, pid, uid, text, feet, q.unwrap_or(0.5))
            }
            Task::Practice => {
                let q = q.unwrap_or(0.5);
                let Some(k) = world.players.get_mut(&pid) else { continue };
                // Сам с собой: рука набивается, а знание — понемногу, если начато у учителя.
                k.practice_only(Skill::Letters, 0.8 * q);
                let subject = [Lore::Alphabet, Lore::Write(Language::Greek)].into_iter().find(|l| !k.knows(*l) && k.prerequisites_met(*l) && (k.lessons.contains_key(l) || *l == Lore::Write(Language::Greek)));
                let mut msg = format!("Исписали табличку {} и разгладили воск", crate::work::quality_word(q));
                if let Some(l) = subject {
                    let need = l.lesson_hours();
                    let p = k.lessons.get(&l).copied().unwrap_or(0.0) + 0.25 * q / need;
                    if p >= 1.0 {
                        k.learn(l);
                        msg = format!("{msg}. Вы узнали: {}", l.name());
                    } else {
                        k.lessons.insert(l, p);
                        msg = format!("{msg}. {}: {:.0}%", l.subject(), p * 100.0);
                    }
                }
                notice(&mut server, pid, msg);
                sync_knowledge(&mut server, &world, pid);
            }
            Task::Lesson { master, mode, subject } => {
                let Some(m) = world.masters.get(&master) else { continue };
                let (teacher, wage) = (m.teacher, m.wage);
                let q = q.unwrap_or(0.5);
                let wage = (wage as f32 * crate::work::wage_mult(q)).round() as u32;
                // Подмастерью платят гроши медью — сколько заработал.
                if mode == LessonMode::Apprentice && wage > 0 {
                    let loot = &mut *loot;
                    if let Some(inv) = loot.inventories.get_mut(&pid)
                        && give_money(inv, wage, &mut loot.uids).is_ok()
                    {
                        notice(&mut server, pid, format!("Работали {}. Вам дали {}", crate::work::quality_word(q), format_money(wage)));
                    }
                    sync_inventory(&mut server, loot, pid);
                }
                // Сам урок: то же, что событие Lesson.
                let Some(k) = world.players.get_mut(&pid) else { continue };
                let before = k.next_lesson(teacher);
                // Прилежный ученик за тот же час берёт больше.
                let learned = k.study_subject(teacher, subject, mode, crate::work::lesson_mult(q));
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

/// Письменный прибор для переписки.
pub const PEN: &str = "Тростниковое перо";
pub const INK: &str = "Чернила";
pub const QUIRE: &str = "Тетрадь пергамента";
pub const TABLET: &str = "Восковая табличка";

/// Можно ли переписать книгу: знать письмо её языка, понимать текст, иметь калам, чернила и пергамент.
fn copy_check(k: &Knowledge, inv: &Inventory, text: Book) -> Result<(), String> {
    if text == Book::HerbalFragment {
        return Err("С ветхого обрывка список не снять: половина букв осыпалась".into());
    }
    let lang = text.info().language;
    if !k.knows(Lore::Write(lang)) {
        return Err(match lang {
            Language::Greek => "Вы не умеете писать. Письму учат в школе, у писца, нотария и книгописца.".into(),
            Language::Latin => "Латинскому письму учит только грамматик.".into(),
        });
    }
    k.can_read(text).map_err(|e| e.message())?;
    let need = text.quires() as u32;
    let mut lack = Vec::new();
    if count_named(inv, PEN) == 0 {
        lack.push("тростниковое перо (калам)".to_string());
    }
    if count_named(inv, INK) == 0 {
        lack.push("чернила".to_string());
    }
    let have = count_named(inv, QUIRE);
    if have < need {
        lack.push(format!("пергамент: нужно тетрадей {need}, есть {have}"));
    }
    if !lack.is_empty() {
        return Err(format!("Для списка не хватает: {}", lack.join(", ")));
    }
    Ok(())
}

/// Сколько секунд переписывать книгу: втрое дольше чтения у новичка, быстрее — у писца.
fn copy_secs(k: &Knowledge, text: Book) -> f32 {
    let letters = k.skill(Skill::Letters);
    text.info().read_secs * (2.2 - letters / 100.0) / crate::skill_tree::Mods::of(k).writing
}

/// Список готов: расходуется пергамент, чернила и калам; небрежный список никто не купит.
#[allow(clippy::too_many_arguments)]
fn finish_copy(server: &mut RenetServer, commands: &mut Commands, world: &mut KnowledgeWorld, loot: &mut LootWorld, pid: u64, uid: ItemUid, text: Book, feet: Vec3, q: f32) {
    let Some(k) = world.players.get(&pid).cloned() else { return };
    let LootWorld { inventories, uids, .. } = loot;
    let Some(inv) = inventories.get_mut(&pid) else { return };
    if inv.find(uid).is_none() {
        notice(server, pid, "Книги, с которой переписывали, больше нет в сумке");
        return;
    }
    if let Err(why) = copy_check(&k, inv, text) {
        notice(server, pid, why);
        return;
    }
    take_named(inv, QUIRE, text.quires() as u32);
    take_named(inv, INK, 1);
    take_named(inv, PEN, 1);
    if let Some(k) = world.players.get_mut(&pid) {
        k.copied.remove(&text);
        k.practice(Skill::Letters, 2.0 * q);
    }
    if q < 0.3 {
        notice(server, pid, "Список вышел с кляксами и пропущенными строками — такой никто не купит. Пергамент пропал.");
    } else {
        let Some(base) = ItemBaseId::find(text.info().item) else { return };
        let item = roll_item(base, Quality::Common, 1, &mut world.rng, uids);
        let msg = format!("Вы переписали «{}» ({}). Книжник купит список.", text.info().item, crate::work::quality_word(q));
        if let Some(rest) = give(inv, item) {
            commands.trigger(crate::loot_server::SpawnGroundItem { pos: feet + Vec3::Y * 0.8, vel: Vec3::new(0.0, 1.0, 0.0), item: rest });
            notice(server, pid, format!("{msg} Не влез в сумку — положили на землю."));
        } else {
            notice(server, pid, msg);
        }
    }
    sync_inventory(server, loot, pid);
    sync_knowledge(server, world, pid);
}

#[allow(clippy::too_many_arguments)]
fn finish_gathering(server: &mut RenetServer, world: &mut KnowledgeWorld, loot: &mut LootWorld, pid: u64, patch: u32, feet: Vec3, q: f32) {
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
    // Бережно и ровно — ещё пучок; рвали рывками — меньше и куст губите.
    if q >= 0.8 {
        amount += 1;
    } else if q < 0.35 && amount > 1 {
        amount -= 1;
    }
    let damage = if q < 0.3 || (!skilled && world.rng.f32() < 0.35 * (1.3 - q)) { 2 } else { 1 };
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
    let at = if p.radius > 1.0 { feet } else { p.pos };
    burst(server, at, BurstFx::Gathered);
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
#[allow(clippy::too_many_arguments)]
fn finish_treatment(server: &mut RenetServer, world: &mut KnowledgeWorld, loot: &mut LootWorld, game: &mut GameWorld, bodies: &mut Bodies, pid: u64, uid: ItemUid, target: Option<medicine::CareTarget>, q: Option<f32>) {
    let Some(item) = loot.inventories.get(&pid).and_then(|inv| inv.find(uid)).map(|(_, i)| i.clone()) else { return };
    let Some(k) = world.players.get_mut(&pid) else { return };
    let name = item.info().name;
    let bandage = ItemBaseId::find("Бинт");
    let has_bandage = name != "Бинт" && loot.inventories.get(&pid).is_some_and(|inv| inv.all_items().any(|i| Some(i.base) == bandage));
    // Твёрдая и ровная рука (наконечник тянут без рывков) повышает удачу.
    let roll = bodies.rng.f32() * q.map_or(1.0, |q| 1.25 - 0.5 * q.clamp(0.0, 1.0));
    let result = match target {
        Some(t) => medicine::apply_to(bodies.body(pid), k, name, item.quality.index(), has_bandage, roll, t),
        None => medicine::apply(bodies.body(pid), k, name, item.quality.index(), has_bandage, roll),
    };
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
    at: Vec3,
    fire_log: Option<brewing::FireLog>,
    work: Option<f32>,
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
    let pots = count_named(inv, "Глиняный горшок") + count_named(inv, "Медный котелок");
    let wp = brewing::Workplace { table: bench, cloth: count_named(inv, brewing::CLOTH) > 0, outer_pot: pots >= 2, clay: count_named(inv, "Гончарная глина") > 0, fire: fire_log, work };
    let result = brewing::brew(spec, v, k, flasks, roll, &wp);
    // Ничего не вышло (например, некуда разлить) — всё остаётся в сумке.
    if result.product.is_none() && !result.vessel_broken {
        notice(server, pid, result.text);
        return;
    }
    for fx in brew_bursts(&result, v) {
        burst(server, at, fx);
    }
    for (name, n) in &spec.items {
        take_named(inv, name, *n as u32);
    }
    take_named(inv, brewing::FLASK, result.flasks_used as u32);
    if result.luted {
        take_named(inv, "Гончарная глина", 1);
    }
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
    if result.cut > 0.0 {
        // Порезы осколками: как удар ножом в треть здоровья при тяжести 1.
        bodies.body(pid).hit(medicine::WoundKind::Cut, result.cut * 30.0, 100.0, false, 1.0);
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

/// Что увидят все вокруг, когда варка кончилась.
fn brew_bursts(r: &brewing::BrewResult, v: Vessel) -> Vec<BurstFx> {
    let mut out = Vec::new();
    if r.ignited {
        out.push(BurstFx::Ignition);
    }
    if r.vessel_broken {
        out.push(BurstFx::Shatter { glass: v.glass() });
    }
    if r.burn > 0.0 && !r.ignited && !r.vessel_broken {
        out.push(BurstFx::BoilOver);
    }
    if r.poison.is_some() {
        out.push(BurstFx::Fumes);
    }
    match r.product {
        Some((name, ..)) if name == brewing::SPOILED => {
            if !r.ignited {
                out.push(BurstFx::Spoiled);
            }
        }
        Some(_) => out.push(BurstFx::Done),
        None => {}
    }
    out
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
            ("Отрез льна", 1),
            ("Сушильная решётка", 1),
            ("Лубок", 1),
            ("Отвар шалфея", 1),
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
        BrewSpec { items: vec![("Шалфей".into(), 1)], water: 2, heat, hours, bath: false }
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
