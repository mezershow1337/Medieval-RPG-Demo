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
use crate::items::{ItemUid, Quality, Rng, roll_item};
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::lore::*;
use crate::lore_net::*;
use crate::protocol::{decode, encode};
use crate::server::GameWorld;
use bevy::prelude::*;
use crate::trade_server::{LessonRequested, RefundLesson};
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::HashMap;

/// Секунд реального времени на час урока у мастера.
pub const LESSON_SECS: f32 = 12.0;
/// Сбор: секунд на траву и на корень (корни выкапывать дольше).
const GATHER_SECS: f32 = 2.5;
const DIG_SECS: f32 = 4.0;
/// Сколько раз можно собрать с одного места и за сколько секунд отрастает одна порция.
const PATCH_CHARGES: u8 = 3;
const REGROW_SECS: f32 = 90.0;

pub struct KnowledgeServerPlugin;

impl Plugin for KnowledgeServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(KnowledgeWorld::default())
            .add_observer(on_connection)
            .add_observer(on_lesson)
            .add_observer(on_grant)
            .add_observer(on_trade_lesson)
            .add_systems(Startup, setup_world)
            .add_systems(Update, (receive_lore_messages, tick_activities, regrow_patches).chain());
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
    regrow: f32,
    draw: bool,
}

pub struct Master {
    pub teacher: Teacher,
    pub pos: Vec3,
    pub yaw: f32,
    pub price: u32,
    pub wage: u32,
}

/// Чем занят игрок.
#[derive(Clone, Copy, Debug)]
enum Task {
    Read { uid: ItemUid, text: Book },
    Gather { patch: u32 },
    Lesson { master: u32, mode: LessonMode },
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
    activities: HashMap<u64, Activity>,
    rng: Rng,
}

impl Default for KnowledgeWorld {
    fn default() -> Self {
        Self { players: HashMap::new(), patches: HashMap::new(), masters: HashMap::new(), activities: HashMap::new(), rng: Rng::from_time() }
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
            self.patches.iter().map(|(id, p)| PatchView { id: *id, substance: p.substance, pos: p.pos.into(), charges: p.charges, draw: p.draw }).collect();
        v.sort_by_key(|p| p.id);
        v
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

/// Пока в уровне нет своих точек сбора и мастеров, ставим их у места появления.
fn setup_world(mut world: ResMut<KnowledgeWorld>, level: Res<LevelColliders>) {
    let origin = crate::level::spawn_point(0);
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
        let pos = crate::npc::find_free_spot(&level, pos);
        world.patches.insert(i as u32 + 1, Patch { substance: s, pos: Vec3::new(pos.x, pos.y.max(0.0), pos.z), charges: PATCH_CHARGES, regrow: 0.0, draw: true });
    }
    // Врача, ризотома, аптекаря и прочих ведёт торговля (лавки в trade.rs). Здесь — только те,
    // кого там нет: церковный писец (грамота) и деревенская знахарка. Цены в нуммиях.
    let masters = [(Teacher::Scribe, 2 * NUMMI_PER_FOLLIS, 10), (Teacher::Wisewoman, NUMMI_PER_FOLLIS, 0)];
    for (i, (teacher, price, wage)) in masters.into_iter().enumerate() {
        let pos = origin + Vec3::new(-2.0 + i as f32 * 4.0, 0.0, 5.0);
        let pos = crate::npc::find_free_spot(&level, pos);
        world.masters.insert(i as u32 + 1, Master { teacher, pos, yaw: 0.0, price, wage });
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

fn receive_lore_messages(mut server: ResMut<RenetServer>, game: Res<GameWorld>, mut world: ResMut<KnowledgeWorld>, mut loot: ResMut<LootWorld>) {
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
                    if !in_reach(feet, p.pos) {
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
                LoreClientMessage::Stop => {}
            }
        }
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
        Teacher::Scribe | Teacher::Grammarian => "Растираете чернила и чините перья",
    }
}

fn tick_activities(time: Res<Time>, mut server: ResMut<RenetServer>, game: Res<GameWorld>, mut world: ResMut<KnowledgeWorld>, mut loot: ResMut<LootWorld>) {
    let dt = time.delta_secs();
    let mut ended: Vec<(u64, bool)> = Vec::new();
    for (pid, a) in world.activities.iter_mut() {
        let Some(actor) = game.actors.get(pid) else {
            ended.push((*pid, false));
            continue;
        };
        let f = &actor.fighter;
        let moved = Vec2::new(actor.pose.feet.x - a.start.x, actor.pose.feet.z - a.start.z).length() > 0.6;
        if !actor.alive || f.is_dead() || f.hp < a.last_hp - 0.01 || !matches!(f.action, Action::Idle) || moved {
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

fn regrow_patches(time: Res<Time>, mut server: ResMut<RenetServer>, mut world: ResMut<KnowledgeWorld>) {
    let dt = time.delta_secs();
    for (id, p) in world.patches.iter_mut() {
        if p.charges >= PATCH_CHARGES {
            p.regrow = 0.0;
            continue;
        }
        p.regrow += dt;
        if p.regrow >= REGROW_SECS {
            p.regrow = 0.0;
            p.charges += 1;
            server.broadcast_message(CH_LORE, encode(&LoreServerMessage::PatchCharges { id: *id, charges: p.charges }));
        }
    }
}
