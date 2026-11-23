//! Сервер торговли: лавки и их запасы, сделки и мена, заказы мастерам, уроки и смены,
//! кража с прилавка, штрафы и пожертвования, обмен между игроками, репутация.
//!
//! Сервер — единственный источник правды: клиент присылает намерение («беру это, отдаю то»),
//! сервер сам считает цены, проверяет деньги, место в сумке и расстояние и только потом
//! меняет инвентари (`LootWorld::inventories`).
//!
//! # API для других систем
//!
//! - `TradeWorld::change_rep(player, faction, delta)` — изменить репутацию (лечение стражника,
//!   выполненный заказ знати…). Клиенту уходит новое состояние на следующем кадре.
//! - Событие [`LessonRequested`] — игрок отучился у мастера или отработал смену. Тред знаний
//!   переводит его в `knowledge::Lesson`; если учить нечему — шлёт [`RefundLesson`], деньги вернутся.
//! - Событие [`CrimeCommitted`] — игрок попался (для стражи).

use crate::collision::LevelColliders;
use crate::economy::{format_money, give_money, purse, take_money};
use crate::factions::{Crime, Faction, Reputation, Standing};
use crate::inventory::{Inventory, Target};
use crate::items::*;
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::protocol::{decode, encode};
use crate::server::GameWorld;
use crate::trade::*;
use crate::trade_net::*;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::{HashMap, HashSet};

/// Как часто лавки пополняют товар и кассу (секунды).
const RESTOCK_SECS: f64 = 300.0;
/// Сколько секунд обиженный торговец не разговаривает с вором.
const GRUDGE_SECS: f64 = 600.0;
/// Сколько живёт приглашение к обмену.
const INVITE_SECS: f64 = 30.0;

#[path = "trade_shelves.rs"]
mod shelves;

pub struct TradeServerPlugin;

impl Plugin for TradeServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_connection)
            .add_observer(on_refund_lesson)
            .add_systems(Startup, setup_merchants)
            .add_systems(
                Update,
                (
                    receive_trade_messages,
                    tick_merchants,
                    tick_lessons,
                    tick_p2p,
                    close_far_windows,
                    send_standing,
                )
                    .chain(),
            )
            .add_systems(Update, shelves::tick_shelves.after(crate::props_server::tick_props).after(receive_trade_messages));
    }
}

// ---------------------------------------------------------------------------
// События
// ---------------------------------------------------------------------------

/// Игрок закончил урок или смену. Тред знаний решает, чему он научился.
#[derive(Event, Clone, Debug)]
#[allow(dead_code)] // поля читает тред знаний (knowledge.rs)
pub struct LessonRequested {
    pub player: u64,
    pub teacher: TeacherKind,
    pub hours: f32,
    pub mode: LessonMode,
    /// Сколько игрок заплатил (вернуть через [`RefundLesson`], если учить нечему).
    pub fee: u32,
}

/// Вернуть плату за урок (мастеру больше нечему учить).
#[derive(Event, Clone, Debug)]
pub struct RefundLesson {
    pub player: u64,
    pub fee: u32,
    pub reason: String,
}

/// Игрок попался на преступлении.
#[derive(Event, Clone, Debug)]
#[allow(dead_code)] // поля читает стража (будущее)
pub struct CrimeCommitted {
    pub player: u64,
    pub crime: Crime,
    pub victim: Faction,
    pub at: Vec3,
    pub fine: u32,
}

// ---------------------------------------------------------------------------
// Состояние
// ---------------------------------------------------------------------------

pub struct MerchantState {
    pub id: MerchantId,
    pub pos: Vec3,
    pub yaw: f32,
    pub shelf: Vec<Item>,
    pub cash: u32,
    pub attention: Attention,
    attention_until: f64,
    next_restock: f64,
    /// Кого торговец не обслуживает и до какого времени.
    grudges: HashMap<u64, f64>,
    /// Когда у него крали.
    thefts: Vec<f64>,
    /// Прилавок (в системе торговца): на нём лежит товар.
    pub stall: Vec<StallBox>,
    /// Торговец убит: лавка закрыта, пока дело не перейдёт к наследнику.
    pub dead: bool,
}

struct Order {
    id: u64,
    player: u64,
    merchant: MerchantId,
    base: ItemBaseId,
    count: u16,
    ready_at: f64,
    due: u32,
}

struct ActiveLesson {
    merchant: MerchantId,
    index: usize,
    /// Урок идёт, пока ученик работает руками (см. work.rs).
    effort: crate::work::Effort,
    fee: u32,
    send_in: f32,
}

/// Каким движением идёт урок или смена у мастера-торговца.
fn lesson_drill(l: &crate::trade::LessonDef) -> crate::work::Drill {
    match l.mode {
        LessonMode::Paid => crate::work::Drill::Mimic,
        LessonMode::Apprentice => crate::work::chore_drill(crate::knowledge::teacher_from_trade(l.teacher)),
    }
}

struct Stolen {
    from: MerchantId,
}

struct P2pSession {
    a: u64,
    b: u64,
    offer_a: (Vec<Line>, u32),
    offer_b: (Vec<Line>, u32),
    ok_a: bool,
    ok_b: bool,
}

impl P2pSession {
    fn partner(&self, p: u64) -> u64 {
        if p == self.a { self.b } else { self.a }
    }
    fn offer_mut(&mut self, p: u64) -> &mut (Vec<Line>, u32) {
        if p == self.a {
            &mut self.offer_a
        } else {
            &mut self.offer_b
        }
    }
}

#[derive(Resource)]
pub struct TradeWorld {
    pub merchants: Vec<MerchantState>,
    reps: HashMap<u64, Reputation>,
    /// Неуплаченные штрафы (нуммии).
    fines: HashMap<u64, u32>,
    open: HashMap<u64, MerchantId>,
    orders: Vec<Order>,
    next_order: u64,
    lessons: HashMap<u64, ActiveLesson>,
    stolen: HashMap<ItemUid, Stolen>,
    p2p: Vec<P2pSession>,
    /// Приглашения: кому -> (от кого, когда).
    invites: HashMap<u64, (u64, f64)>,
    /// Кому разослать репутацию.
    standing_dirty: HashSet<u64>,
    /// Умение торговаться (покупка, продажа) — множители цен от навыка «Торговля» (ведёт skills_server.rs).
    pub haggle: HashMap<u64, (f32, f32)>,
    /// Обороты сделок за кадр (игрок, нуммии) — из них растёт навык торговли.
    pub turnover: Vec<(u64, u32)>,
    rng: Rng,
}

impl TradeWorld {
    pub fn rep(&mut self, player: u64) -> &mut Reputation {
        self.reps.entry(player).or_default()
    }

    /// Множители цен от навыка торговли: (покупка, продажа).
    pub fn haggle(&self, player: u64) -> (f32, f32) {
        self.haggle.get(&player).copied().unwrap_or((1.0, 1.0))
    }

    pub fn standing(&self, player: u64, f: Faction) -> Standing {
        self.reps
            .get(&player)
            .map(|r| r.standing(f))
            .unwrap_or(Standing::of(f.initial_reputation()))
    }

    /// Изменить репутацию игрока у фракции (с отголосками у союзников и врагов).
    pub fn change_rep(&mut self, player: u64, f: Faction, delta: f32) {
        self.rep(player).change(f, delta);
        self.standing_dirty.insert(player);
    }

    /// Снять пеню (уплачена или отсижена).
    pub fn clear_fine(&mut self, player: u64) {
        self.fines.remove(&player);
        self.standing_dirty.insert(player);
    }

    /// Убийце лавочник (или его наследник) не продаст ничего ещё долго.
    pub fn grudge(&mut self, player: u64, id: MerchantId, until: f64) {
        self.merchant(id).grudges.insert(player, until);
    }

    pub fn fine(&self, player: u64) -> u32 {
        self.fines.get(&player).copied().unwrap_or(0)
    }

    pub fn add_fine(&mut self, player: u64, nummi: u32) {
        *self.fines.entry(player).or_default() += nummi;
        self.standing_dirty.insert(player);
    }

    fn merchant(&mut self, id: MerchantId) -> &mut MerchantState {
        &mut self.merchants[id.0 as usize]
    }

    /// Почему торговец не хочет иметь дела с игроком.
    fn refusal(&self, player: u64, id: MerchantId, now: f64) -> Option<String> {
        let def = id.def();
        let m = &self.merchants[id.0 as usize];
        if m.dead {
            return Some("Лавка закрыта: хозяина убили. Говорят, дело перейдёт к родне.".into());
        }
        if m.grudges.get(&player).is_some_and(|t| *t > now) {
            return Some("«Я тебя запомнил, вор. Проваливай, пока стражу не кликнул».".into());
        }
        if self.standing(player, def.faction) == Standing::Hostile {
            return Some(format!(
                "«Нам с тобой говорить не о чем». ({}: вражда)",
                def.faction.name()
            ));
        }
        let fine = self.fine(player);
        if fine > 0 && def.faction.city_law() && !def.has(Service::Fines) {
            return Some(format!(
                "«Сперва уладь дело с гарнизоном». За вами штраф {}: уплатите казначею гарнизона.",
                format_money(fine)
            ));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Расстановка
// ---------------------------------------------------------------------------

/// Точка места по ключу (см. `level::places`); на уровне-замке мест нет — тогда запасная расстановка.
fn place_pos(key: &str) -> Option<Vec3> {
    crate::level::place(key).map(|p| p.pos)
}

/// Пока мест города нет: ставим лавки полукругом у точки появления, каждое место — своя «лавка».
fn fallback_pos(place_index: usize, places: usize) -> Vec3 {
    let center = crate::level::spawn_point(0);
    let a = std::f32::consts::PI * (0.15 + 0.7 * place_index as f32 / places.max(1) as f32);
    let r = 11.0 + (place_index % 2) as f32 * 3.5;
    center + Vec3::new(a.cos() * r, 0.0, -a.sin() * r)
}

/// Куда можно повернуть торговца: четыре стороны света (прилавок встаёт ровно, вдоль стен
/// и улиц), от самой просторной к самой тесной; второе число — сколько метров свободно впереди.
/// При равенстве — к точке появления.
fn open_yaws(level: &LevelColliders, pos: Vec3) -> Vec<(f32, f32)> {
    let to_spawn = (crate::level::spawn_point(0) - pos).with_y(0.0).normalize_or_zero();
    let mut v: Vec<(f32, f32, f32)> = [Vec3::NEG_Z, Vec3::X, Vec3::Z, Vec3::NEG_X]
        .into_iter()
        .map(|dir| {
            let mut free = 0.0;
            let mut d = 0.5;
            while d <= 6.0 && level.body_is_free(&crate::collision::Body::STANDING, pos + dir * d + Vec3::Y * 0.1) {
                free = d;
                d += 0.5;
            }
            (free + dir.dot(to_spawn) * 0.25, yaw_towards(dir), free)
        })
        .collect();
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    v.into_iter().map(|(_, yaw, free)| (yaw, free)).collect()
}

/// Поворот, при котором торговец смотрит вдоль `dir` (он смотрит в свою −Z).
fn yaw_towards(dir: Vec3) -> f32 {
    f32::atan2(-dir.x, -dir.z)
}

/// Свободны ли столешницы прилавка, поставленного у торговца в `pos` с поворотом `yaw`.
fn stall_fits(level: &LevelColliders, pos: Vec3, yaw: f32, boxes: &[StallBox]) -> bool {
    let probe = crate::collision::Body { half_width: 0.12, height: 0.8 };
    stall_obbs(pos, yaw, boxes).iter().all(|t| {
        [-1.0f32, 0.0, 1.0].iter().flat_map(|&u| [-1.0f32, 1.0].map(move |v| (u, v))).all(|(u, v)| {
            let p = t.center + t.rot * Vec3::new(t.half.x * u * 0.9, 0.0, t.half.z * v * 0.8);
            level.body_is_free(&probe, p.with_y(pos.y + 0.1))
        })
    })
}

/// Место на полу рядом с `anchor`, где торговец и его прилавок помещаются: человек не залезает
/// на столы и крыши (в отличие от `find_free_spot`) и не встаёт над ямой или цистерной.
fn ground_spot(level: &LevelColliders, anchor: Vec3, boxes: &[StallBox]) -> Option<(Vec3, f32)> {
    for radius in [0.0f32, 0.75, 1.5, 2.25, 3.0, 4.0, 5.0] {
        let n = if radius == 0.0 { 1 } else { 16 };
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let p = anchor + Vec3::new(a.cos() * radius, 0.1, a.sin() * radius);
            let ground = level.ground_height(p.x, p.z);
            // Пол города — коллайдеры на y≈0, за стенами — рельеф; над дырой рельефа не стоим.
            if ground.is_finite() && (ground - anchor.y).abs() > 0.6 {
                continue;
            }
            if ground == f32::NEG_INFINITY && level.ground_height(anchor.x, anchor.z).is_finite() {
                continue;
            }
            if !level.body_is_free(&crate::collision::Body::STANDING, p) {
                continue;
            }
            // Перед прилавком должно оставаться место покупателю.
            for (yaw, free) in open_yaws(level, p) {
                if free >= 2.5 && stall_fits(level, p, yaw, boxes) {
                    return Some((p, yaw));
                }
            }
        }
    }
    None
}

/// Где стоят торговцы: точка, поворот (лицом к прилавку) и прилавок.
///
/// У мест с готовыми лотками (`stall_slots`: агора, портовый торг, лавки) торговец встаёт за
/// своим лотком; остальные — на полу возле точки места, лицом туда, где просторнее.
pub fn merchant_spots(level: &LevelColliders) -> Vec<(Vec3, f32, Vec<StallBox>)> {
    let mut places: Vec<&str> = Vec::new();
    for def in MERCHANTS {
        if !places.contains(&def.place) {
            places.push(def.place);
        }
    }
    let mut used: HashMap<&str, usize> = HashMap::new();
    let mut spots = MERCHANTS
        .iter()
        .map(|def| {
            let slot = used.entry(def.place).or_default();
            let k = *slot;
            *slot += 1;
            let outside = def.faction.outside_walls();
            if let Some(s) = crate::level::stall_slots(def.place).get(k) {
                return (s.pos + Vec3::Y * 0.1, yaw_towards(s.facing), stall_layout(outside, false));
            }
            let place_index = places.iter().position(|p| *p == def.place).unwrap();
            let anchor =
                place_pos(def.place).unwrap_or_else(|| fallback_pos(place_index, places.len()));
            // Несколько торговцев на одном месте — рядом друг с другом.
            let side = Vec3::new((k as f32 * 2.8) * if k % 2 == 0 { 1.0 } else { -1.0 }, 0.0, 0.0);
            let one = stall_layout(outside, false);
            let (pos, yaw) = ground_spot(level, anchor + side, &one)
                .or_else(|| ground_spot(level, anchor, &one))
                .unwrap_or_else(|| {
                    let p = crate::npc::find_free_spot(level, anchor + side);
                    (p, open_yaws(level, p)[0].0)
                });
            // Боковой столик — только если справа от торговца свободно.
            let with_side = stall_layout(outside, true);
            let side_ok = !outside && stall_fits(level, pos, yaw, &with_side);
            (pos, yaw, stall_layout(outside, side_ok))
        })
        .collect::<Vec<_>>();
    // Торговцы на одном месте стоят тесно: боковой столик не должен залезать на соседний прилавок.
    for i in 0..spots.len() {
        if spots[i].2.len() < 2 {
            continue;
        }
        let side = stall_obbs(spots[i].0, spots[i].1, &spots[i].2[1..])[0].aabb();
        let crowded = spots.iter().enumerate().any(|(j, o)| {
            j != i
                && (side.overlaps(&crate::collision::Body::STANDING.aabb_at(o.0))
                    || stall_obbs(o.0, o.1, &o.2).iter().any(|t| t.aabb().overlaps(&side)))
        });
        if crowded {
            spots[i].2.truncate(1);
        }
    }
    spots
}

fn setup_merchants(mut commands: Commands, level: Res<LevelColliders>) {
    let mut rng = Rng::from_time();
    let mut uids = UidGen(4_000_000_000);
    let merchants = merchant_spots(&level)
        .into_iter()
        .enumerate()
        .map(|(i, (pos, yaw, stall))| {
            let def = &MERCHANTS[i];
            let mut m = MerchantState {
                id: MerchantId(i as u16),
                pos,
                yaw,
                shelf: Vec::new(),
                cash: def.cash,
                attention: Attention::Watching,
                attention_until: 0.0,
                next_restock: RESTOCK_SECS,
                grudges: HashMap::new(),
                thefts: Vec::new(),
                stall,
                dead: false,
            };
            restock(&mut m, &mut rng, &mut uids, true);
            m
        })
        .collect::<Vec<MerchantState>>();
    let statics = merchants.iter().flat_map(|m| stall_obbs(m.pos, m.yaw, &m.stall)).collect();
    commands.insert_resource(crate::props_server::PropStatics(statics));
    commands.insert_resource(TradeWorld {
        merchants,
        reps: HashMap::new(),
        fines: HashMap::new(),
        open: HashMap::new(),
        orders: Vec::new(),
        next_order: 1,
        lessons: HashMap::new(),
        stolen: HashMap::new(),
        p2p: Vec::new(),
        invites: HashMap::new(),
        standing_dirty: HashSet::new(),
        haggle: HashMap::new(),
        turnover: Vec::new(),
        rng,
    });
}

/// Пополнить полки до обычного запаса; лишнее, купленное у игроков, понемногу уходит покупателям.
fn restock(m: &mut MerchantState, rng: &mut Rng, uids: &mut UidGen, first: bool) {
    let def = m.id.def();
    for st in def.stock {
        let Some(base) = ItemBaseId::find(st.item) else {
            continue;
        };
        let have: u16 = m
            .shelf
            .iter()
            .filter(|i| i.base == base)
            .map(|i| i.count)
            .sum();
        if !first && have >= st.min.max(1) {
            continue;
        }
        let want = rng.range_u32(st.min as u32, st.max as u32) as u16;
        let mut add = want.saturating_sub(have);
        while add > 0 {
            let stack = add.min(base.base().max_stack.max(1));
            // Мастер делает ровно, но бывает и удача: добротные вещи — редкость.
            let q = roll_quality([6.0, 74.0, 15.0, 5.0, 0.0, 0.0], 0.0, rng);
            let item = roll_item(base, q, stack, rng, uids);
            add -= item.count;
            put_on_shelf(&mut m.shelf, item);
        }
    }
    if !first {
        // Скупленное у игроков (чего нет в обычном товаре) распродаётся.
        m.shelf
            .retain(|i| def.stock.iter().any(|st| st.item == i.info().name) || rng.f32() > 0.35);
        m.cash = m.cash + (def.cash.saturating_sub(m.cash)) / 3;
    }
}

fn put_on_shelf(shelf: &mut Vec<Item>, mut item: Item) {
    for s in shelf.iter_mut() {
        if s.can_stack_with(&item) && item.affixes.is_empty() && s.affixes.is_empty() {
            let room = s.info().max_stack.saturating_sub(s.count);
            let moved = room.min(item.count);
            s.count += moved;
            item.count -= moved;
            if item.count == 0 {
                return;
            }
        }
    }
    shelf.push(item);
}

/// Отделить `count` штук от стопки в списке. Возвращает отделённое (с новым номером, если стопка осталась).
fn split_from(list: &mut Vec<Item>, uid: ItemUid, count: u16, uids: &mut UidGen) -> Option<Item> {
    let i = list.iter().position(|it| it.uid == uid)?;
    let count = count.clamp(1, list[i].count);
    if count == list[i].count {
        return Some(list.remove(i));
    }
    list[i].count -= count;
    let mut part = list[i].clone();
    part.uid = uids.next();
    part.count = count;
    Some(part)
}

/// То же для сумки (только сетка: надетое не продаём, сначала снимите).
fn split_from_inv(
    inv: &mut Inventory,
    uid: ItemUid,
    count: u16,
    uids: &mut UidGen,
) -> Option<Item> {
    let i = inv.grid.items.iter().position(|p| p.item.uid == uid)?;
    let p = &mut inv.grid.items[i];
    let count = count.clamp(1, p.item.count);
    if count == p.item.count {
        return Some(inv.grid.items.swap_remove(i).item);
    }
    p.item.count -= count;
    let mut part = p.item.clone();
    part.uid = uids.next();
    part.count = count;
    Some(part)
}

fn notice(server: &mut RenetServer, player: u64, text: impl Into<String>) {
    server.send_message(
        player,
        CH_LOOT,
        encode(&LootServerMessage::Notice(text.into())),
    );
}

fn send(server: &mut RenetServer, player: u64, msg: &TradeServerMessage) {
    server.send_message(player, CH_TRADE, encode(msg));
}

fn send_inventory(server: &mut RenetServer, loot: &LootWorld, player: u64) {
    if let Some(inv) = loot.inventories.get(&player) {
        server.send_message(
            player,
            CH_LOOT,
            encode(&LootServerMessage::Inventory(inv.clone())),
        );
    }
}

fn near(a: Vec3, b: Vec3, range: f32) -> bool {
    let d = a - b;
    Vec2::new(d.x, d.z).length() <= range && d.y.abs() < 3.0
}

// ---------------------------------------------------------------------------
// Окно торговца
// ---------------------------------------------------------------------------

/// Цена с поправкой на умение торговаться (не меньше нуммия, если была).
fn haggled(price: u32, k: f32) -> u32 {
    if price == 0 {
        return 0;
    }
    ((price as f32 * k).round() as u32).max(1)
}

fn build_view(
    tw: &TradeWorld,
    loot: &LootWorld,
    player: u64,
    id: MerchantId,
    now: f64,
) -> MerchantView {
    let def = id.def();
    let m = &tw.merchants[id.0 as usize];
    let standing = tw.standing(player, def.faction);
    let (buy_k, sell_k) = tw.haggle(player);
    let rep = tw
        .reps
        .get(&player)
        .map(|r| r.get(def.faction))
        .unwrap_or(def.faction.initial_reputation());
    let normal = |base: ItemBaseId| {
        def.stock
            .iter()
            .find(|st| st.item == base.base().name)
            .map(|st| st.max)
            .unwrap_or(1)
    };
    let count_of = |base: ItemBaseId| {
        m.shelf
            .iter()
            .filter(|i| i.base == base)
            .map(|i| i.count)
            .sum::<u16>()
    };
    let shelf = m
        .shelf
        .iter()
        .map(|item| {
            let (w, h) = item.size();
            ShelfItem {
                item: item.clone(),
                price: haggled(
                    ask_price(
                        def,
                        item.base,
                        item.quality,
                        standing,
                        count_of(item.base),
                        normal(item.base),
                    ),
                    buy_k,
                ),
                pocketable: w * h <= MAX_STEAL_CELLS,
            }
        })
        .collect();
    let bids = loot
        .inventories
        .get(&player)
        .map(|inv| {
            inv.grid
                .items
                .iter()
                .map(|p| {
                    let stolen = tw.stolen.contains_key(&p.item.uid);
                    Bid {
                        uid: p.item.uid,
                        price: bid_price(
                            def,
                            p.item.base,
                            p.item.quality,
                            standing,
                            count_of(p.item.base),
                            stolen,
                        )
                        .map(|p| haggled(p, sell_k)),
                        contraband: is_contraband_for(def, p.item.base),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let lock =
        |min: Standing| (standing < min).then(|| format!("нужно отношение «{}»", min.name()));
    let orders = def
        .orders
        .iter()
        .enumerate()
        .map(|(i, o)| OrderOffer {
            index: i as u8,
            item: o.item.to_string(),
            count: o.count,
            price: o.price,
            secs: o.secs,
            locked: if ItemBaseId::find(o.item).is_none() {
                Some("мастер ещё не берётся".into())
            } else {
                lock(o.min)
            },
        })
        .collect();
    let my_orders = tw
        .orders
        .iter()
        .filter(|o| o.player == player && o.merchant == id)
        .map(|o| OrderStatus {
            id: o.id,
            item: o.base.base().name.to_string(),
            count: o.count,
            remaining: (o.ready_at - now).max(0.0) as f32,
            due: o.due,
        })
        .collect();
    let lessons = def
        .lessons
        .iter()
        .enumerate()
        .map(|(i, l)| LessonOffer {
            index: i as u8,
            title: l.title.to_string(),
            paid: l.mode == LessonMode::Paid,
            money: l.money,
            secs: l.secs,
            locked: lock(l.min),
        })
        .collect();
    MerchantView {
        id,
        name: def.name.to_string(),
        title: def.title.to_string(),
        greeting: def.greeting.to_string(),
        faction: def.faction,
        rep,
        money: def.money,
        attention: m.attention,
        cash: m.cash,
        shelf,
        bids,
        orders,
        my_orders,
        lessons,
        fines: def.has(Service::Fines),
        donations: def.has(Service::Donations),
        repairs: repair_offers(def, loot, player),
        refuse: tw.refusal(player, id, now),
    }
}

/// Сбитые вещи игрока, за которые возьмётся кузнец.
fn repair_offers(def: &MerchantDef, loot: &LootWorld, player: u64) -> Vec<RepairOffer> {
    if !def.has(Service::Repair) {
        return Vec::new();
    }
    let Some(inv) = loot.inventories.get(&player) else { return Vec::new() };
    inv.all_items()
        .filter_map(|i| {
            let m = i.make.as_ref()?;
            let c = m.condition();
            (m.dur_max > 0.0 && c < 0.98).then(|| RepairOffer { uid: i.uid, name: i.info().name.to_string(), condition: c, price: crate::smithing::repair_price(m, i.value()) })
        })
        .collect()
}

/// Мастер у своего горна правит лучше, чем самоучка.
const MASTER_REPAIR_SKILL: f32 = 85.0;

fn refresh_view(
    server: &mut RenetServer,
    tw: &TradeWorld,
    loot: &LootWorld,
    player: u64,
    now: f64,
) {
    if let Some(id) = tw.open.get(&player) {
        send(
            server,
            player,
            &TradeServerMessage::View(build_view(tw, loot, player, *id, now)),
        );
    }
}

/// Обновить окно всем, у кого открыт этот торговец (полка общая).
fn refresh_merchant(
    server: &mut RenetServer,
    tw: &TradeWorld,
    loot: &LootWorld,
    id: MerchantId,
    now: f64,
) {
    let viewers: Vec<u64> = tw
        .open
        .iter()
        .filter(|(_, m)| **m == id)
        .map(|(p, _)| *p)
        .collect();
    for p in viewers {
        refresh_view(server, tw, loot, p, now);
    }
}

// ---------------------------------------------------------------------------
// Сделка
// ---------------------------------------------------------------------------

enum DealOutcome {
    Done {
        said: String,
    },
    /// Узнали краденое: вещь отобрали, штраф.
    Caught {
        said: String,
    },
    Failed(String),
}

#[allow(clippy::too_many_arguments)]
fn deal(
    tw: &mut TradeWorld,
    loot: &mut LootWorld,
    player: u64,
    id: MerchantId,
    buy: &[Line],
    sell: &[Line],
    at: Vec3,
    now: f64,
    crimes: &mut Vec<CrimeCommitted>,
) -> DealOutcome {
    let def = id.def();
    if let Some(r) = tw.refusal(player, id, now) {
        return DealOutcome::Failed(r);
    }
    if buy.is_empty() && sell.is_empty() {
        return DealOutcome::Failed("Ничего не выбрано".into());
    }
    let standing = tw.standing(player, def.faction);
    let (buy_k, sell_k) = tw.haggle(player);
    let Some(mut inv) = loot.inventories.get(&player).cloned() else {
        return DealOutcome::Failed("Нет сумки".into());
    };
    let mut uids = loot.uids.clone();
    let mut shelf = tw.merchants[id.0 as usize].shelf.clone();
    let mut cash = tw.merchants[id.0 as usize].cash;
    let normal = |base: ItemBaseId| {
        def.stock
            .iter()
            .find(|st| st.item == base.base().name)
            .map(|st| st.max)
            .unwrap_or(1)
    };
    let count_of = |shelf: &Vec<Item>, base: ItemBaseId| {
        shelf
            .iter()
            .filter(|i| i.base == base)
            .map(|i| i.count)
            .sum::<u16>()
    };

    // Краденое, которое узнали: сделка срывается, вещь отбирают.
    for (uid, _) in sell {
        let Some(st) = tw.stolen.get(uid) else {
            continue;
        };
        if def.has(Service::Fence) {
            continue;
        }
        let chance = if st.from == id {
            0.75
        } else if st.from.def().faction == def.faction
            || (def.faction.city_law() && st.from.def().faction.city_law())
        {
            0.2
        } else {
            0.0
        };
        let roll = tw.rng.f32();
        if roll < chance {
            let victim = st.from;
            let Some(item) = split_from_inv(&mut inv, *uid, u16::MAX, &mut uids) else {
                continue;
            };
            let fine = item.value() * Crime::StolenGoodsRecognized.fine_multiplier();
            let name = item.info().name;
            tw.stolen.remove(uid);
            put_on_shelf(&mut tw.merchant(victim).shelf, item);
            loot.inventories.insert(player, inv);
            loot.uids = uids;
            tw.change_rep(player, def.faction, -10.0);
            if def.faction.city_law() {
                tw.add_fine(player, fine);
            }
            crimes.push(CrimeCommitted {
                player,
                crime: Crime::StolenGoodsRecognized,
                victim: victim.def().faction,
                at,
                fine,
            });
            return DealOutcome::Caught {
                said: format!(
                    "«Это же {} — вещь {}!» Краденое отобрали{}",
                    name,
                    victim.def().name,
                    if def.faction.city_law() {
                        format!(", штраф {}", format_money(fine))
                    } else {
                        String::new()
                    }
                ),
            };
        }
    }

    // Берём с полки.
    let mut bought = Vec::new();
    let mut buy_total = 0u32;
    for (uid, count) in buy {
        let Some(item) = shelf.iter().find(|i| i.uid == *uid) else {
            return DealOutcome::Failed("Этого товара уже нет".into());
        };
        let price = haggled(
            ask_price(
                def,
                item.base,
                item.quality,
                standing,
                count_of(&shelf, item.base),
                normal(item.base),
            ),
            buy_k,
        );
        let part = split_from(&mut shelf, *uid, *count, &mut uids).unwrap();
        buy_total += price * part.count as u32;
        bought.push(part);
    }
    // Отдаём из сумки.
    let mut sold = Vec::new();
    let mut sell_total = 0u32;
    let mut contraband = Vec::new();
    for (uid, count) in sell {
        let Some(p) = inv.grid.items.iter().find(|p| p.item.uid == *uid) else {
            return DealOutcome::Failed("Этой вещи у вас нет (надетое сначала снимите)".into());
        };
        let stolen = tw.stolen.contains_key(uid);
        let Some(price) = bid_price(
            def,
            p.item.base,
            p.item.quality,
            standing,
            count_of(&shelf, p.item.base),
            stolen,
        )
        .map(|p| haggled(p, sell_k)) else {
            return DealOutcome::Failed(format!("«{}» ему не нужно", p.item.info().name));
        };
        if crate::economy::coin_value(p.item.base).is_none() && is_contraband_for(def, p.item.base)
        {
            contraband.push(p.item.clone());
        }
        let part = split_from_inv(&mut inv, *uid, *count, &mut uids).unwrap();
        sell_total += price * part.count as u32;
        sold.push(part);
    }

    let said;
    match def.money {
        MoneyPolicy::Barter { .. } => {
            if sell_total < buy_total {
                return DealOutcome::Failed(format!(
                    "Мало. За это он хочет ещё товара на {}",
                    format_money(buy_total - sell_total)
                ));
            }
            said = if sell_total > buy_total + buy_total / 5 + 200 {
                "Он довольно кивает: вы явно переплатили."
            } else {
                "Ударили по рукам."
            }
            .to_string();
        }
        MoneyPolicy::Coins => {
            if sell_total > buy_total {
                let owe = sell_total - buy_total;
                if owe > cash {
                    return DealOutcome::Failed(format!(
                        "У него в кассе только {}",
                        format_money(cash)
                    ));
                }
                if let Err(e) = give_money(&mut inv, owe, &mut uids) {
                    return DealOutcome::Failed(e.message().into());
                }
                cash -= owe;
                said = format!("Вы получили {}", format_money(owe));
            } else {
                let pay = buy_total - sell_total;
                if let Err(e) = take_money(&mut inv, pay, &mut uids) {
                    return DealOutcome::Failed(format!(
                        "{} (нужно {}, в кошеле {})",
                        e.message(),
                        format_money(pay),
                        format_money(purse(&inv))
                    ));
                }
                cash += pay;
                said = if pay > 0 {
                    format!("Вы заплатили {}", format_money(pay))
                } else {
                    "Обменялись без доплаты".into()
                };
            }
        }
    }
    for item in bought {
        // Купленное кладём так, чтобы не смешать стопки с краденым — по месту, без обмена.
        if inv.place(item, Target::Auto).is_err() {
            return DealOutcome::Failed("Не помещается в сумку".into());
        }
    }
    // Всё сошлось: применяем.
    for item in sold {
        tw.stolen.remove(&item.uid);
        // Монеты за стенами торговец «съедает» (нанизывает на ожерелье), на полку не кладёт.
        if crate::economy::coin_value(item.base).is_none() {
            put_on_shelf(&mut shelf, item);
        }
    }
    let ms = tw.merchant(id);
    ms.shelf = shelf;
    ms.cash = cash;
    loot.inventories.insert(player, inv);
    loot.uids = uids;
    // Оборот учит торговать.
    tw.turnover.push((player, buy_total + sell_total));
    // Оборот улучшает отношения: +1 за каждые полсолида, не больше +3 за сделку.
    let turnover = (buy_total + sell_total) as f32;
    tw.change_rep(player, def.faction, (turnover / 3600.0).min(3.0));
    // Оружие варварам: по закону нельзя. Кто-нибудь да донесёт.
    if !contraband.is_empty() {
        tw.change_rep(player, def.faction, 1.5 * contraband.len() as f32);
        if tw.rng.f32() < 0.35 {
            let value: u32 = contraband.iter().map(Item::value).sum();
            let fine = value * Crime::ArmsToBarbarians.fine_multiplier();
            tw.add_fine(player, fine);
            let garrison_loss = if def.faction == Faction::Goths {
                3.0
            } else {
                8.0
            };
            tw.change_rep(player, Faction::Garrison, -garrison_loss);
            crimes.push(CrimeCommitted {
                player,
                crime: Crime::ArmsToBarbarians,
                victim: Faction::Garrison,
                at,
                fine,
            });
            return DealOutcome::Done {
                said: format!(
                    "{said}. Но кто-то донёс в гарнизон о продаже оружия варварам: штраф {}",
                    format_money(fine)
                ),
            };
        }
    }
    DealOutcome::Done { said }
}

// ---------------------------------------------------------------------------
// Кража
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn steal(
    tw: &mut TradeWorld,
    loot: &mut LootWorld,
    game: &GameWorld,
    player: u64,
    id: MerchantId,
    uid: ItemUid,
    now: f64,
    crimes: &mut Vec<CrimeCommitted>,
) -> Result<String, String> {
    let def = id.def();
    if tw.merchants[id.0 as usize]
        .grudges
        .get(&player)
        .is_some_and(|t| *t > now)
    {
        return Err("Он с вас глаз не сводит".into());
    }
    let Some(item) = tw.merchants[id.0 as usize]
        .shelf
        .iter()
        .find(|i| i.uid == uid)
        .cloned()
    else {
        return Err("Этого уже нет на прилавке".into());
    };
    let (w, h) = item.size();
    if w * h > MAX_STEAL_CELLS {
        return Err("Такое под одеждой не спрячешь".into());
    }
    let Some(actor) = game.actors.get(&player) else {
        return Err(String::new());
    };
    let Some(inv) = loot.inventories.get(&player) else {
        return Err(String::new());
    };
    let Some((fx, fy)) = inv.grid.find_free((w, h)) else {
        return Err("Некуда спрятать: в сумке нет места".into());
    };
    let agility = inv.equipped_stats().get(Stat::Agility);
    let m = &mut tw.merchants[id.0 as usize];
    m.thefts.retain(|t| now - t < 900.0);
    let attempt = TheftAttempt {
        vigilance: def.vigilance,
        attention: m.attention,
        cells: w * h,
        crouching: actor.pose.crouching,
        agility,
        recent_thefts: m.thefts.len() as u32,
        guards_near: def.faction == Faction::Garrison,
    };
    let chance = theft_detection(&attempt);
    m.thefts.push(now);
    if tw.rng.f32() < chance {
        return Ok(caught_stealing(tw, player, id, &item, actor.pose.feet, now, crimes));
    }
    let mut uids = loot.uids.clone();
    let part = split_from(&mut tw.merchant(id).shelf, uid, 1, &mut uids).unwrap();
    let stolen_uid = part.uid;
    let name = part.info().name;
    let inv = loot.inventories.get_mut(&player).unwrap();
    // В отдельную клетку, чтобы краденое не смешалось со своими стопками.
    let _ = inv.place(part, Target::Grid { x: fx, y: fy });
    loot.uids = uids;
    tw.stolen.insert(stolen_uid, Stolen { from: id });
    // Портовое дно уважает ловкие руки.
    tw.change_rep(player, Faction::Underworld, 1.0);
    Ok(format!(
        "Вы незаметно спрятали «{name}». Не продавайте это {} — узнает.",
        def.name
    ))
}

/// Вора поймали за руку: торговец запомнит, штраф (в городе) или вражда (за стенами).
#[allow(clippy::too_many_arguments)]
fn caught_stealing(tw: &mut TradeWorld, player: u64, id: MerchantId, item: &Item, at: Vec3, now: f64, crimes: &mut Vec<CrimeCommitted>) -> String {
    let def = id.def();
    // Попался с поличным.
    let m = tw.merchant(id);
    m.grudges.insert(player, now + GRUDGE_SECS);
    m.attention = Attention::Watching;
    m.attention_until = now + 30.0;
    let name = item.info().name;
    if def.faction.outside_walls() {
        tw.change_rep(player, def.faction, -25.0);
        return format!(
            "{} схватил вас за руку! За стенами суда нет: он запомнит, и его люди тоже.",
            def.name
        );
    }
    let fine = item.value() * Crime::TheftCaught.fine_multiplier();
    tw.add_fine(player, fine);
    tw.change_rep(player, def.faction, -12.0);
    if def.faction != Faction::Garrison {
        tw.change_rep(player, Faction::Garrison, -4.0);
    }
    crimes.push(CrimeCommitted {
        player,
        crime: Crime::TheftCaught,
        victim: def.faction,
        at,
        fine,
    });
    format!(
        "«Держи вора!» {} поймал вас с «{name}». По закону — штраф в четверную цену: {}",
        def.name,
        format_money(fine)
    )
}

// ---------------------------------------------------------------------------
// Обмен между игроками
// ---------------------------------------------------------------------------

fn p2p_state(s: &P2pSession, viewer: u64, loot: &LootWorld) -> P2pState {
    let items = |owner: u64, lines: &[Line]| -> Vec<Item> {
        let Some(inv) = loot.inventories.get(&owner) else {
            return Vec::new();
        };
        lines
            .iter()
            .filter_map(|(uid, count)| {
                inv.grid.items.iter().find(|p| p.item.uid == *uid).map(|p| {
                    let mut it = p.item.clone();
                    it.count = (*count).clamp(1, it.count);
                    it
                })
            })
            .collect()
    };
    let partner = s.partner(viewer);
    let (mine, theirs) = if viewer == s.a {
        (&s.offer_a, &s.offer_b)
    } else {
        (&s.offer_b, &s.offer_a)
    };
    let (ok_me, ok_them) = if viewer == s.a {
        (s.ok_a, s.ok_b)
    } else {
        (s.ok_b, s.ok_a)
    };
    P2pState {
        partner,
        mine: items(viewer, &mine.0),
        my_nummi: mine.1,
        theirs: items(partner, &theirs.0),
        their_nummi: theirs.1,
        i_confirmed: ok_me,
        they_confirmed: ok_them,
    }
}

fn send_p2p(server: &mut RenetServer, s: &P2pSession, loot: &LootWorld) {
    for p in [s.a, s.b] {
        send(server, p, &TradeServerMessage::P2p(p2p_state(s, p, loot)));
    }
}

/// Выполнить обмен: всё или ничего.
fn execute_p2p(s: &P2pSession, loot: &mut LootWorld) -> Result<(), String> {
    let (Some(mut a), Some(mut b)) = (
        loot.inventories.get(&s.a).cloned(),
        loot.inventories.get(&s.b).cloned(),
    ) else {
        return Err("Обмен сорвался".into());
    };
    let mut uids = loot.uids.clone();
    let mut from_a = Vec::new();
    for (uid, count) in &s.offer_a.0 {
        from_a.push(
            split_from_inv(&mut a, *uid, *count, &mut uids).ok_or("Предложенной вещи уже нет")?,
        );
    }
    let mut from_b = Vec::new();
    for (uid, count) in &s.offer_b.0 {
        from_b.push(
            split_from_inv(&mut b, *uid, *count, &mut uids).ok_or("Предложенной вещи уже нет")?,
        );
    }
    take_money(&mut a, s.offer_a.1, &mut uids).map_err(|e| e.message().to_string())?;
    take_money(&mut b, s.offer_b.1, &mut uids).map_err(|e| e.message().to_string())?;
    give_money(&mut b, s.offer_a.1, &mut uids)
        .map_err(|_| "У второго игрока нет места для монет".to_string())?;
    give_money(&mut a, s.offer_b.1, &mut uids)
        .map_err(|_| "У вас нет места для монет".to_string())?;
    for item in from_a {
        b.place(item, Target::Auto)
            .map_err(|_| "Не помещается в сумку партнёра".to_string())?;
    }
    for item in from_b {
        a.place(item, Target::Auto)
            .map_err(|_| "Не помещается в вашу сумку".to_string())?;
    }
    loot.inventories.insert(s.a, a);
    loot.inventories.insert(s.b, b);
    loot.uids = uids;
    Ok(())
}

// ---------------------------------------------------------------------------
// Системы
// ---------------------------------------------------------------------------

fn merchant_list(tw: &TradeWorld) -> Vec<MerchantInfo> {
    tw.merchants
        .iter()
        .map(|m| {
            let def = m.id.def();
            MerchantInfo {
                id: m.id,
                name: def.name.into(),
                title: def.title.into(),
                faction: def.faction,
                pos: m.pos.into(),
                yaw: m.yaw,
                look: def.look,
                stall: m.stall.clone(),
            }
        })
        .collect()
}

fn on_connection(
    event: On<RenetServerEvent>,
    mut server: ResMut<RenetServer>,
    tw: Option<ResMut<TradeWorld>>,
) {
    let Some(mut tw) = tw else { return };
    match &event.0 {
        ServerEvent::ClientConnected { client_id } => {
            let id = *client_id;
            send(
                &mut server,
                id,
                &TradeServerMessage::Merchants(merchant_list(&tw)),
            );
            tw.rep(id);
            tw.standing_dirty.insert(id);
        }
        ServerEvent::ClientDisconnected { client_id, .. } => {
            let id = *client_id;
            tw.open.remove(&id);
            tw.lessons.remove(&id);
            tw.invites.retain(|to, (from, _)| *to != id && *from != id);
            let partners: Vec<u64> = tw
                .p2p
                .iter()
                .filter(|s| s.a == id || s.b == id)
                .map(|s| s.partner(id))
                .collect();
            tw.p2p.retain(|s| s.a != id && s.b != id);
            for p in partners {
                send(
                    &mut server,
                    p,
                    &TradeServerMessage::P2pClosed {
                        reason: "Партнёр ушёл".into(),
                    },
                );
            }
            tw.orders.retain(|o| o.player != id);
            tw.reps.remove(&id);
            tw.fines.remove(&id);
        }
    }
}

fn on_refund_lesson(
    event: On<RefundLesson>,
    mut server: ResMut<RenetServer>,
    mut loot: ResMut<LootWorld>,
) {
    let ev = event.event().clone();
    if ev.fee == 0 {
        notice(&mut server, ev.player, ev.reason);
        return;
    }
    let mut uids = loot.uids.clone();
    if let Some(inv) = loot.inventories.get_mut(&ev.player)
        && give_money(inv, ev.fee, &mut uids).is_ok()
    {
        loot.uids = uids;
        notice(
            &mut server,
            ev.player,
            format!("{} Плату вернули: {}", ev.reason, format_money(ev.fee)),
        );
        send_inventory(&mut server, &loot, ev.player);
    }
}

#[allow(clippy::too_many_arguments)]
fn receive_trade_messages(
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    time: Res<Time>,
    mut loot: ResMut<LootWorld>,
    tw: Option<ResMut<TradeWorld>>,
    knowledge: Option<Res<crate::knowledge::KnowledgeWorld>>,
) {
    let Some(mut tw) = tw else { return };
    let tw = &mut *tw;
    let now = time.elapsed_secs_f64();
    let mut crimes = Vec::new();
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_TRADE) {
            let Some(msg) = decode::<TradeClientMessage>(&bytes) else {
                continue;
            };
            let Some(actor) = game.actors.get(&pid).filter(|a| a.alive) else {
                continue;
            };
            let feet = actor.pose.feet;
            if !loot.inventories.contains_key(&pid) {
                continue;
            }
            // Окно, открытое у этого игрока, и дотягивается ли он до торговца.
            let open = tw
                .open
                .get(&pid)
                .copied()
                .filter(|id| near(feet, tw.merchants[id.0 as usize].pos, TALK_RANGE + 0.7));
            match msg {
                TradeClientMessage::Open { merchant } => {
                    if merchant.0 as usize >= tw.merchants.len() {
                        continue;
                    }
                    if !near(
                        feet,
                        tw.merchants[merchant.0 as usize].pos,
                        TALK_RANGE + 0.5,
                    ) {
                        notice(&mut server, pid, "Слишком далеко");
                        continue;
                    }
                    tw.open.insert(pid, merchant);
                    refresh_view(&mut server, tw, &loot, pid, now);
                }
                TradeClientMessage::Close => {
                    tw.open.remove(&pid);
                }
                TradeClientMessage::Deal { buy, sell } => {
                    let Some(id) = open else { continue };
                    let reply =
                        match deal(tw, &mut loot, pid, id, &buy, &sell, feet, now, &mut crimes) {
                            DealOutcome::Done { said } => said,
                            DealOutcome::Caught { said } => said,
                            DealOutcome::Failed(why) => why,
                        };
                    send(&mut server, pid, &TradeServerMessage::Say(reply));
                    send_inventory(&mut server, &loot, pid);
                    refresh_merchant(&mut server, tw, &loot, id, now);
                }
                TradeClientMessage::Steal { uid } => {
                    let Some(id) = open else { continue };
                    let reply = match steal(tw, &mut loot, &game, pid, id, uid, now, &mut crimes) {
                        Ok(s) | Err(s) => s,
                    };
                    if !reply.is_empty() {
                        send(&mut server, pid, &TradeServerMessage::Say(reply));
                    }
                    send_inventory(&mut server, &loot, pid);
                    refresh_merchant(&mut server, tw, &loot, id, now);
                }
                TradeClientMessage::PlaceOrder { index } => {
                    let Some(id) = open else { continue };
                    let def = id.def();
                    let Some(o) = def.orders.get(index as usize) else {
                        continue;
                    };
                    let reply = (|| {
                        if let Some(r) = tw.refusal(pid, id, now) {
                            return r;
                        }
                        if tw.standing(pid, def.faction) < o.min {
                            return format!(
                                "«Тебе я такое не доверю». Нужно отношение «{}»",
                                o.min.name()
                            );
                        }
                        let Some(base) = ItemBaseId::find(o.item) else {
                            return "Мастер пока за такое не берётся".into();
                        };
                        if tw.orders.iter().filter(|x| x.player == pid).count() >= 6 {
                            return "У вас и так много заказов".into();
                        }
                        let deposit = crate::economy::round_price(o.price as f32 / 2.0);
                        let mut uids = loot.uids.clone();
                        let inv = loot.inventories.get_mut(&pid).unwrap();
                        if let Err(e) = take_money(inv, deposit, &mut uids) {
                            return format!("{}: задаток {}", e.message(), format_money(deposit));
                        }
                        loot.uids = uids;
                        let order_id = tw.next_order;
                        tw.next_order += 1;
                        tw.orders.push(Order {
                            id: order_id,
                            player: pid,
                            merchant: id,
                            base,
                            count: o.count,
                            ready_at: now + o.secs as f64,
                            due: o.price - deposit,
                        });
                        tw.change_rep(pid, def.faction, 0.5);
                        format!(
                            "Задаток {} принят. «Приходи через {} мин.»",
                            format_money(deposit),
                            (o.secs / 60.0).ceil()
                        )
                    })();
                    send(&mut server, pid, &TradeServerMessage::Say(reply));
                    send_inventory(&mut server, &loot, pid);
                    refresh_view(&mut server, tw, &loot, pid, now);
                }
                TradeClientMessage::CollectOrder { id: order_id } => {
                    let Some(id) = open else { continue };
                    let Some(i) = tw
                        .orders
                        .iter()
                        .position(|o| o.id == order_id && o.player == pid && o.merchant == id)
                    else {
                        continue;
                    };
                    let reply = (|| {
                        let o = &tw.orders[i];
                        if o.ready_at > now {
                            return format!("«Ещё не готово, жди {} с»", (o.ready_at - now).ceil());
                        }
                        let (base, count, due) = (o.base, o.count, o.due);
                        let Some(mut inv) = loot.inventories.get(&pid).cloned() else {
                            return String::new();
                        };
                        let mut uids = loot.uids.clone();
                        if let Err(e) = take_money(&mut inv, due, &mut uids) {
                            return format!("{}: доплатить {}", e.message(), format_money(due));
                        }
                        // Мастер старается: заказная вещь чаще выходит добротной.
                        let q = roll_quality([0.0, 60.0, 30.0, 10.0, 0.0, 0.0], 0.0, &mut tw.rng);
                        let item = roll_item(base, q, count, &mut tw.rng, &mut uids);
                        if inv.place(item, Target::Auto).is_err() {
                            return "Не помещается в сумку: освободите место".into();
                        }
                        loot.inventories.insert(pid, inv);
                        loot.uids = uids;
                        tw.orders.remove(i);
                        tw.change_rep(pid, id.def().faction, 2.0);
                        format!("«Принимай работу». Вы доплатили {}", format_money(due))
                    })();
                    send(&mut server, pid, &TradeServerMessage::Say(reply));
                    send_inventory(&mut server, &loot, pid);
                    refresh_view(&mut server, tw, &loot, pid, now);
                }
                TradeClientMessage::StartLesson { index } => {
                    let Some(id) = open else { continue };
                    let def = id.def();
                    let Some(l) = def.lessons.get(index as usize) else {
                        continue;
                    };
                    let reply = (|| {
                        if let Some(r) = tw.refusal(pid, id, now) {
                            return Err(r);
                        }
                        if tw.lessons.contains_key(&pid) {
                            return Err("Вы уже заняты уроком".into());
                        }
                        if tw.standing(pid, def.faction) < l.min {
                            return Err(format!(
                                "«Чужого не учу». Нужно отношение «{}»",
                                l.min.name()
                            ));
                        }
                        let mut fee = 0;
                        if l.mode == LessonMode::Paid {
                            let mut uids = loot.uids.clone();
                            let inv = loot.inventories.get_mut(&pid).unwrap();
                            take_money(inv, l.money, &mut uids).map_err(|e| {
                                format!("{}: урок стоит {}", e.message(), format_money(l.money))
                            })?;
                            loot.uids = uids;
                            fee = l.money;
                        }
                        tw.lessons.insert(
                            pid,
                            ActiveLesson {
                                merchant: id,
                                index: index as usize,
                                effort: crate::work::Effort::new(lesson_drill(l), l.secs),
                                fee,
                                send_in: 0.0,
                            },
                        );
                        Ok(())
                    })();
                    match reply {
                        Ok(()) => send(
                            &mut server,
                            pid,
                            &TradeServerMessage::LessonStarted {
                                title: l.title.to_string(),
                                secs: l.secs,
                                drill: Some(lesson_drill(l)),
                                skill: crate::knowledge::teacher_from_trade(l.teacher)
                                    .skill()
                                    .and_then(|sk| {
                                        knowledge
                                            .as_ref()
                                            .and_then(|k| k.players.get(&pid))
                                            .map(|k| k.skill(sk) / 100.0)
                                    })
                                    .unwrap_or(0.0),
                            },
                        ),
                        Err(r) => send(&mut server, pid, &TradeServerMessage::Say(r)),
                    }
                    send_inventory(&mut server, &loot, pid);
                }
                TradeClientMessage::Stroke { q } => {
                    if let Some(l) = tw.lessons.get_mut(&pid) {
                        l.effort.stroke(q);
                    }
                }
                TradeClientMessage::StopLesson => {
                    if tw.lessons.remove(&pid).is_some() {
                        send(
                            &mut server,
                            pid,
                            &TradeServerMessage::LessonFinished { ok: false },
                        );
                    }
                }
                TradeClientMessage::PayFine => {
                    let Some(id) = open else { continue };
                    if !id.def().has(Service::Fines) {
                        continue;
                    }
                    let fine = tw.fine(pid);
                    let reply = if fine == 0 {
                        "«За тобой ничего нет. Пока».".to_string()
                    } else {
                        let mut uids = loot.uids.clone();
                        let inv = loot.inventories.get_mut(&pid).unwrap();
                        match take_money(inv, fine, &mut uids) {
                            Ok(()) => {
                                loot.uids = uids;
                                tw.fines.remove(&pid);
                                tw.standing_dirty.insert(pid);
                                tw.change_rep(pid, Faction::Garrison, 2.0);
                                format!(
                                    "Штраф {} уплачен. Вы больше не в розыске.",
                                    format_money(fine)
                                )
                            }
                            Err(e) => format!("{}: штраф {}", e.message(), format_money(fine)),
                        }
                    };
                    send(&mut server, pid, &TradeServerMessage::Say(reply));
                    send_inventory(&mut server, &loot, pid);
                    refresh_view(&mut server, tw, &loot, pid, now);
                }
                TradeClientMessage::Repair { uid } => {
                    let Some(id) = open else { continue };
                    if !id.def().has(Service::Repair) {
                        continue;
                    }
                    let Some(offer) = repair_offers(id.def(), &loot, pid).into_iter().find(|o| o.uid == uid) else { continue };
                    let mut uids = loot.uids.clone();
                    let inv = loot.inventories.get_mut(&pid).unwrap();
                    let reply = match take_money(inv, offer.price, &mut uids) {
                        Ok(()) => {
                            // Вещь могла уйти в обмен за деньги (сдача) — ищем заново.
                            if let Some(m) = inv.find_mut(uid).and_then(|i| i.make.as_mut()) {
                                crate::smithing::repair(m, MASTER_REPAIR_SKILL);
                            }
                            loot.uids = uids;
                            format!("«Готово: выправил и навёл кромку». {} — {}", offer.name, format_money(offer.price))
                        }
                        Err(e) => format!("{}: правка стоит {}", e.message(), format_money(offer.price)),
                    };
                    send(&mut server, pid, &TradeServerMessage::Say(reply));
                    send_inventory(&mut server, &loot, pid);
                    refresh_view(&mut server, tw, &loot, pid, now);
                }
                TradeClientMessage::Donate { nummi } => {
                    let Some(id) = open else { continue };
                    if !id.def().has(Service::Donations) || nummi == 0 {
                        continue;
                    }
                    let mut uids = loot.uids.clone();
                    let inv = loot.inventories.get_mut(&pid).unwrap();
                    let reply = match take_money(inv, nummi, &mut uids) {
                        Ok(()) => {
                            loot.uids = uids;
                            // Солид милостыни — +10 у церкви (и немного у горожан).
                            tw.change_rep(pid, Faction::Church, (nummi as f32 / 720.0).min(25.0));
                            format!(
                                "«Да воздаст тебе Господь». Вы пожертвовали {}",
                                format_money(nummi)
                            )
                        }
                        Err(e) => e.message().to_string(),
                    };
                    send(&mut server, pid, &TradeServerMessage::Say(reply));
                    send_inventory(&mut server, &loot, pid);
                    refresh_view(&mut server, tw, &loot, pid, now);
                }
                TradeClientMessage::P2pRequest { target } => {
                    let Some(other) = game
                        .actors
                        .get(&target)
                        .filter(|a| a.alive && a.kind == crate::protocol::ActorKind::Player)
                    else {
                        continue;
                    };
                    if target == pid || !near(feet, other.pose.feet, P2P_RANGE) {
                        notice(&mut server, pid, "Подойдите ближе");
                        continue;
                    }
                    if tw
                        .p2p
                        .iter()
                        .any(|s| [s.a, s.b].contains(&pid) || [s.a, s.b].contains(&target))
                    {
                        notice(&mut server, pid, "Кто-то из вас уже меняется");
                        continue;
                    }
                    tw.invites.insert(target, (pid, now));
                    send(
                        &mut server,
                        target,
                        &TradeServerMessage::P2pInvite { from: pid },
                    );
                    notice(&mut server, pid, "Вы предложили обмен. Ждём ответа…");
                }
                TradeClientMessage::P2pRespond { from, accept } => {
                    let Some((inviter, at)) = tw.invites.remove(&pid) else {
                        continue;
                    };
                    if inviter != from || now - at > INVITE_SECS {
                        continue;
                    }
                    if !accept {
                        notice(&mut server, from, "Вам отказали в обмене");
                        continue;
                    }
                    if tw
                        .p2p
                        .iter()
                        .any(|s| [s.a, s.b].contains(&pid) || [s.a, s.b].contains(&from))
                    {
                        continue;
                    }
                    let s = P2pSession {
                        a: from,
                        b: pid,
                        offer_a: (vec![], 0),
                        offer_b: (vec![], 0),
                        ok_a: false,
                        ok_b: false,
                    };
                    send_p2p(&mut server, &s, &loot);
                    tw.p2p.push(s);
                }
                TradeClientMessage::P2pOffer { items, nummi } => {
                    let Some(i) = tw.p2p.iter().position(|s| s.a == pid || s.b == pid) else {
                        continue;
                    };
                    let Some(inv) = loot.inventories.get(&pid) else {
                        continue;
                    };
                    // Только то, что есть в сумке (не надетое), и не больше денег, чем в кошеле.
                    let mut lines: Vec<Line> = Vec::new();
                    for (uid, count) in items.into_iter().take(24) {
                        if let Some(p) = inv.grid.items.iter().find(|p| p.item.uid == uid)
                            && crate::economy::coin_value(p.item.base).is_none()
                            && !lines.iter().any(|(u, _)| *u == uid)
                        {
                            lines.push((uid, count.clamp(1, p.item.count)));
                        }
                    }
                    let nummi = nummi.min(purse(inv));
                    let s = &mut tw.p2p[i];
                    *s.offer_mut(pid) = (lines, nummi);
                    s.ok_a = false;
                    s.ok_b = false;
                    send_p2p(&mut server, &tw.p2p[i], &loot);
                }
                TradeClientMessage::P2pConfirm(ok) => {
                    let Some(i) = tw.p2p.iter().position(|s| s.a == pid || s.b == pid) else {
                        continue;
                    };
                    {
                        let s = &mut tw.p2p[i];
                        if s.a == pid {
                            s.ok_a = ok;
                        } else {
                            s.ok_b = ok;
                        }
                    }
                    let s = &tw.p2p[i];
                    if s.ok_a && s.ok_b {
                        let (a, b) = (s.a, s.b);
                        match execute_p2p(s, &mut loot) {
                            Ok(()) => {
                                tw.p2p.remove(i);
                                for p in [a, b] {
                                    send(
                                        &mut server,
                                        p,
                                        &TradeServerMessage::P2pClosed {
                                            reason: "Обмен состоялся".into(),
                                        },
                                    );
                                    send_inventory(&mut server, &loot, p);
                                }
                            }
                            Err(why) => {
                                let s = &mut tw.p2p[i];
                                s.ok_a = false;
                                s.ok_b = false;
                                for p in [a, b] {
                                    notice(&mut server, p, why.clone());
                                }
                                send_p2p(&mut server, &tw.p2p[i], &loot);
                            }
                        }
                    } else {
                        send_p2p(&mut server, s, &loot);
                    }
                }
                TradeClientMessage::P2pCancel => {
                    let Some(i) = tw.p2p.iter().position(|s| s.a == pid || s.b == pid) else {
                        continue;
                    };
                    let s = tw.p2p.remove(i);
                    for p in [s.a, s.b] {
                        send(
                            &mut server,
                            p,
                            &TradeServerMessage::P2pClosed {
                                reason: "Обмен отменён".into(),
                            },
                        );
                    }
                }
            }
        }
    }
    for c in crimes {
        commands.trigger(c);
    }
}

/// Внимание торговцев, пополнение полок, готовность заказов.
fn tick_merchants(
    mut server: ResMut<RenetServer>,
    time: Res<Time>,
    mut loot: ResMut<LootWorld>,
    tw: Option<ResMut<TradeWorld>>,
) {
    let Some(mut tw) = tw else { return };
    let now = time.elapsed_secs_f64();
    let tw = &mut *tw;
    let mut changed = Vec::new();
    let mut uids = loot.uids.clone();
    for m in tw.merchants.iter_mut() {
        let def = m.id.def();
        if now >= m.attention_until {
            let rng = &mut tw.rng;
            // Зоркий торговец отвлекается реже и ненадолго.
            m.attention = match m.attention {
                Attention::Watching => {
                    Attention::Distracted(rng.range_u32(0, DISTRACTIONS.len() as u32 - 1) as u8)
                }
                Attention::Distracted(_) => Attention::Watching,
            };
            let secs = match m.attention {
                Attention::Watching => rng.range_f32(6.0, 14.0) * (0.6 + def.vigilance),
                Attention::Distracted(_) => rng.range_f32(3.0, 7.0) * (1.3 - def.vigilance),
            };
            m.attention_until = now + secs as f64;
            changed.push(m.id);
        }
        if now >= m.next_restock {
            m.next_restock = now + RESTOCK_SECS;
            restock(m, &mut tw.rng, &mut uids, false);
            m.grudges.retain(|_, t| *t > now);
            changed.push(m.id);
        }
    }
    loot.uids = uids;
    // Готовые заказы: сказать игроку один раз.
    for o in tw.orders.iter_mut() {
        if o.ready_at <= now && o.ready_at > now - time.delta_secs_f64() - 1e-9 {
            notice(
                &mut server,
                o.player,
                format!(
                    "{}: заказ готов — «{}»",
                    o.merchant.def().name,
                    o.base.base().name
                ),
            );
        }
    }
    for id in changed {
        refresh_merchant(&mut server, tw, &loot, id, now);
    }
}

/// Уроки и смены: игрок должен быть рядом с мастером всё время.
fn tick_lessons(
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    time: Res<Time>,
    game: Res<GameWorld>,
    mut loot: ResMut<LootWorld>,
    tw: Option<ResMut<TradeWorld>>,
) {
    let Some(mut tw) = tw else { return };
    let players: Vec<u64> = tw.lessons.keys().copied().collect();
    for pid in players {
        let l = &tw.lessons[&pid];
        let mpos = tw.merchants[l.merchant.0 as usize].pos;
        let here = game
            .actors
            .get(&pid)
            .is_some_and(|a| a.alive && near(a.pose.feet, mpos, LESSON_RANGE));
        if !here {
            tw.lessons.remove(&pid);
            send(
                &mut server,
                pid,
                &TradeServerMessage::LessonFinished { ok: false },
            );
            notice(&mut server, pid, "Урок прерван: вы ушли от мастера");
            continue;
        }
        let dt = time.delta_secs();
        let l = tw.lessons.get_mut(&pid).unwrap();
        l.effort.tick(dt);
        l.send_in -= dt;
        if l.send_in <= 0.0 {
            l.send_in = 0.2;
            let p = l.effort.frac();
            send(&mut server, pid, &TradeServerMessage::LessonProgress(p));
        }
        if !l.effort.finished() {
            continue;
        }
        let l = tw.lessons.remove(&pid).unwrap();
        let q = l.effort.quality();
        let def = l.merchant.def();
        let lesson = &def.lessons[l.index];
        if lesson.mode == LessonMode::Apprentice {
            let mut uids = loot.uids.clone();
            // Платят по работе: кто работал рывками, получает меньше.
            let pay = ((lesson.money as f32 * crate::work::wage_mult(q)).round() as u32).max(1);
            if let Some(inv) = loot.inventories.get_mut(&pid)
                && give_money(inv, pay, &mut uids).is_ok()
            {
                loot.uids = uids;
                notice(
                    &mut server,
                    pid,
                    format!(
                        "Смена окончена: работали {}. {} платит вам {}",
                        crate::work::quality_word(q),
                        def.name,
                        format_money(pay)
                    ),
                );
            }
            send_inventory(&mut server, &loot, pid);
        } else {
            notice(
                &mut server,
                pid,
                format!("Урок у {} окончен: повторяли {}", def.name, crate::work::quality_word(q)),
            );
        }
        tw.change_rep(pid, def.faction, 1.0);
        send(
            &mut server,
            pid,
            &TradeServerMessage::LessonFinished { ok: true },
        );
        commands.trigger(LessonRequested {
            player: pid,
            teacher: lesson.teacher,
            // Прилежный ученик берёт с урока больше.
            hours: lesson.hours * crate::work::lesson_mult(q),
            mode: lesson.mode,
            fee: l.fee,
        });
    }
}

/// Обмен между игроками закрывается, если они разошлись или кто-то погиб.
fn tick_p2p(
    mut server: ResMut<RenetServer>,
    time: Res<Time>,
    game: Res<GameWorld>,
    tw: Option<ResMut<TradeWorld>>,
) {
    let Some(mut tw) = tw else { return };
    let now = time.elapsed_secs_f64();
    tw.invites.retain(|_, (_, at)| now - *at <= INVITE_SECS);
    let mut closed = Vec::new();
    tw.p2p.retain(|s| {
        let pos = |p: u64| game.actors.get(&p).filter(|a| a.alive).map(|a| a.pose.feet);
        let ok = match (pos(s.a), pos(s.b)) {
            (Some(a), Some(b)) => near(a, b, P2P_RANGE + 1.5),
            _ => false,
        };
        if !ok {
            closed.push((s.a, s.b));
        }
        ok
    });
    for (a, b) in closed {
        for p in [a, b] {
            send(
                &mut server,
                p,
                &TradeServerMessage::P2pClosed {
                    reason: "Вы разошлись — обмен отменён".into(),
                },
            );
        }
    }
}

/// Окно торговца закрывается, если игрок отошёл или погиб.
fn close_far_windows(
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    tw: Option<ResMut<TradeWorld>>,
) {
    let Some(mut tw) = tw else { return };
    let far: Vec<u64> = tw
        .open
        .iter()
        .filter(|(pid, id)| {
            let mpos = tw.merchants[id.0 as usize].pos;
            !game
                .actors
                .get(pid)
                .is_some_and(|a| a.alive && near(a.pose.feet, mpos, TALK_RANGE + 1.0))
        })
        .map(|(p, _)| *p)
        .collect();
    for p in far {
        tw.open.remove(&p);
        send(&mut server, p, &TradeServerMessage::Closed);
    }
}

fn send_standing(mut server: ResMut<RenetServer>, tw: Option<ResMut<TradeWorld>>) {
    let Some(mut tw) = tw else { return };
    if tw.standing_dirty.is_empty() {
        return;
    }
    let dirty: Vec<u64> = tw.standing_dirty.drain().collect();
    for p in dirty {
        let rep = tw.rep(p).clone();
        let fine = tw.fine(p);
        send(&mut server, p, &TradeServerMessage::Standing { rep, fine });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn dump_merchant_spots() {
        let level = crate::level::city_colliders();
        for (i, (p, yaw, st)) in merchant_spots(&level).iter().enumerate() {
            let d = &MERCHANTS[i];
            let a = crate::level::place(d.place).map(|x| x.pos).unwrap_or(Vec3::ZERO);
            println!("{:12} {:16} anchor ({:7.1},{:5.1},{:7.1}) pos ({:7.1},{:5.2},{:7.1}) moved {:4.1} yaw {:5.2} tables {} ground {:.2}", d.key, d.place, a.x, a.y, a.z, p.x, p.y, p.z, p.xz().distance(a.xz()), yaw, st.len(), level.ground_height(p.x, p.z));
        }
    }

    /// Торговцы стоят на полу у своих мест, за свободными прилавками, не друг на друге.
    #[test]
    fn merchants_stand_at_their_places() {
        let level = crate::level::city_colliders();
        let spots = merchant_spots(&level);
        let mut slot_used: HashMap<&str, usize> = HashMap::new();
        for (i, (p, yaw, st)) in spots.iter().enumerate() {
            let d = &MERCHANTS[i];
            let place = crate::level::place(d.place).unwrap_or_else(|| panic!("{}: нет места {}", d.key, d.place));
            let k = slot_used.entry(d.place).or_default();
            if let Some(s) = crate::level::stall_slots(d.place).get(*k) {
                assert!(p.xz().distance(s.pos.xz()) < 0.01, "{} не за своим лотком", d.key);
            } else {
                assert!(p.xz().distance(place.pos.xz()) < 6.0, "{} ушёл от {} на {:.1} м", d.key, d.place, p.xz().distance(place.pos.xz()));
            }
            *k += 1;
            assert!((p.y - place.pos.y).abs() < 0.3, "{} стоит не на полу: y {:.2} при полу {:.2}", d.key, p.y, place.pos.y);
            assert!(level.body_is_free(&crate::collision::Body::STANDING, *p), "{} в стене или предмете", d.key);
            assert!(level.ground_height(p.x, p.z) > f32::NEG_INFINITY || level.ground_height(place.pos.x, place.pos.z) == f32::NEG_INFINITY, "{} над ямой", d.key);
            assert!(stall_fits(&level, *p, *yaw, st), "прилавок {} упирается в стену", d.key);
            for (j, (q, qyaw, qst)) in spots.iter().enumerate() {
                if i == j {
                    continue;
                }
                let me = stall_obbs(*p, *yaw, st);
                let other = stall_obbs(*q, *qyaw, qst);
                let body = crate::collision::Body::STANDING.aabb_at(*q);
                assert!(
                    me.iter().all(|t| !t.aabb().overlaps(&body) && other.iter().all(|o| !o.aabb().overlaps(&t.aabb()))),
                    "прилавок {} налезает на {}",
                    d.key,
                    MERCHANTS[j].key
                );
            }
        }
    }

    fn world() -> (TradeWorld, LootWorld) {
        let mut rng = Rng::new(5);
        let mut uids = UidGen(4_000_000_000);
        let merchants = MERCHANTS
            .iter()
            .enumerate()
            .map(|(i, def)| {
                let mut m = MerchantState {
                    id: MerchantId(i as u16),
                    pos: Vec3::ZERO,
                    yaw: 0.0,
                    shelf: vec![],
                    cash: def.cash,
                    attention: Attention::Watching,
                    attention_until: 0.0,
                    next_restock: 0.0,
                    grudges: HashMap::new(),
                    thefts: vec![],
                    stall: stall_layout(false, true),
                    dead: false,
                };
                restock(&mut m, &mut rng, &mut uids, true);
                m
            })
            .collect();
        let tw = TradeWorld {
            merchants,
            reps: HashMap::new(),
            fines: HashMap::new(),
            open: HashMap::new(),
            orders: vec![],
            next_order: 1,
            lessons: HashMap::new(),
            stolen: HashMap::new(),
            p2p: vec![],
            invites: HashMap::new(),
            standing_dirty: HashSet::new(),
            haggle: HashMap::new(),
            turnover: Vec::new(),
            rng: Rng::new(9),
        };
        let mut loot = LootWorld::default();
        let mut inv = Inventory::new(crate::inventory::PLAYER_GRID);
        give_money(&mut inv, 7200 * 3, &mut loot.uids).unwrap();
        loot.inventories.insert(1, inv);
        (tw, loot)
    }

    fn shelf_uid(tw: &TradeWorld, id: MerchantId, name: &str) -> ItemUid {
        tw.merchants[id.0 as usize]
            .shelf
            .iter()
            .find(|i| i.info().name == name)
            .unwrap()
            .uid
    }

    #[test]
    fn buy_and_sell_with_coins() {
        let (mut tw, mut loot) = world();
        let id = merchant_by_key("andronikos").unwrap();
        let uid = shelf_uid(&tw, id, "Бинт");
        let before = purse(&loot.inventories[&1]);
        let out = deal(
            &mut tw,
            &mut loot,
            1,
            id,
            &[(uid, 2)],
            &[],
            Vec3::ZERO,
            0.0,
            &mut vec![],
        );
        assert!(matches!(out, DealOutcome::Done { .. }));
        let inv = &loot.inventories[&1];
        assert!(purse(inv) < before);
        assert_eq!(
            inv.all_items()
                .filter(|i| i.info().name == "Бинт")
                .map(|i| i.count)
                .sum::<u16>(),
            2
        );
        // Продаём один бинт обратно — дешевле, чем купили.
        let bandage = inv
            .grid
            .items
            .iter()
            .find(|p| p.item.info().name == "Бинт")
            .unwrap()
            .item
            .uid;
        let mid = purse(inv);
        let out = deal(
            &mut tw,
            &mut loot,
            1,
            id,
            &[],
            &[(bandage, 1)],
            Vec3::ZERO,
            0.0,
            &mut vec![],
        );
        assert!(matches!(out, DealOutcome::Done { .. }));
        let gained = purse(&loot.inventories[&1]) - mid;
        assert!(gained > 0 && gained < (before - mid) / 2);
    }

    #[test]
    fn barter_needs_goods_not_bronze() {
        let (mut tw, mut loot) = world();
        let id = merchant_by_key("eskam").unwrap();
        let fat = shelf_uid(&tw, id, "Овчина");
        // Только медью — нельзя (медь кочевнику не нужна, а солиды в кошеле разменяны не будут).
        let inv = loot.inventories.get_mut(&1).unwrap();
        let folles = Item {
            uid: 77,
            base: ItemBaseId::by_name("Фоллис"),
            quality: Quality::Common,
            affixes: vec![],
            count: 50,
            make: None,
        };
        inv.place(folles, Target::Auto).unwrap();
        let out = deal(
            &mut tw,
            &mut loot,
            1,
            id,
            &[(fat, 1)],
            &[(77, 50)],
            Vec3::ZERO,
            0.0,
            &mut vec![],
        );
        assert!(matches!(out, DealOutcome::Failed(_)));
        // Амфора вина — пожалуйста.
        let wine = Item {
            uid: 78,
            base: ItemBaseId::by_name("Амфора вина"),
            quality: Quality::Common,
            affixes: vec![],
            count: 1,
            make: None,
        };
        loot.inventories
            .get_mut(&1)
            .unwrap()
            .place(wine, Target::Auto)
            .unwrap();
        let out = deal(
            &mut tw,
            &mut loot,
            1,
            id,
            &[(fat, 1)],
            &[(78, 1)],
            Vec3::ZERO,
            0.0,
            &mut vec![],
        );
        assert!(
            matches!(out, DealOutcome::Done { .. }),
            "{:?}",
            match out {
                DealOutcome::Failed(s) => s,
                _ => String::new(),
            }
        );
        assert!(
            loot.inventories[&1]
                .all_items()
                .any(|i| i.info().name == "Овчина")
        );
    }

    #[test]
    fn fine_blocks_city_trade_until_paid() {
        let (mut tw, mut loot) = world();
        tw.add_fine(1, 400);
        let shop = merchant_by_key("andronikos").unwrap();
        assert!(tw.refusal(1, shop, 0.0).is_some());
        // Таверна и гарнизон всё равно говорят.
        assert!(
            tw.refusal(1, merchant_by_key("mardonios").unwrap(), 0.0)
                .is_none()
        );
        assert!(
            tw.refusal(1, merchant_by_key("markianos").unwrap(), 0.0)
                .is_none()
        );
        let inv = loot.inventories.get_mut(&1).unwrap();
        take_money(inv, 400, &mut loot.uids).unwrap();
        tw.fines.remove(&1);
        assert!(tw.refusal(1, shop, 0.0).is_none());
    }

    #[test]
    fn stolen_goods_are_recognized_by_victim() {
        let (mut tw, mut loot) = world();
        let id = merchant_by_key("andronikos").unwrap();
        let wax = Item {
            uid: 99,
            base: ItemBaseId::by_name("Воск"),
            quality: Quality::Common,
            affixes: vec![],
            count: 1,
            make: None,
        };
        loot.inventories
            .get_mut(&1)
            .unwrap()
            .place(wax, Target::Auto)
            .unwrap();
        tw.stolen.insert(99, Stolen { from: id });
        let mut caught = 0;
        for _ in 0..20 {
            let snapshot = loot.inventories[&1].clone();
            if let DealOutcome::Caught { .. } = deal(
                &mut tw,
                &mut loot,
                1,
                id,
                &[],
                &[(99, 1)],
                Vec3::ZERO,
                0.0,
                &mut vec![],
            ) {
                caught += 1;
                assert!(tw.fine(1) > 0);
                break;
            }
            // Не узнал — вернём как было и попробуем ещё.
            loot.inventories.insert(1, snapshot);
            tw.stolen.insert(99, Stolen { from: id });
        }
        assert!(caught > 0);
        // Скупщик краденого берёт без вопросов.
        let fence = merchant_by_key("mardonios").unwrap().def();
        assert!(
            bid_price(
                fence,
                ItemBaseId::by_name("Воск"),
                Quality::Common,
                Standing::Neutral,
                0,
                true
            )
            .is_some()
        );
    }

    #[test]
    fn p2p_swap_is_atomic() {
        let (_tw, mut loot) = world();
        let mut inv2 = Inventory::new((1, 2));
        let sword = Item {
            uid: 500,
            base: ItemBaseId::by_name("Кинжал"),
            quality: Quality::Common,
            affixes: vec![],
            count: 1,
            make: None,
        };
        inv2.place(sword, Target::Auto).unwrap();
        loot.inventories.insert(2, inv2);
        // Игрок 1 платит 2 фоллиса за кинжал; у игрока 2 сумка 1×2 занята кинжалом.
        let s = P2pSession {
            a: 1,
            b: 2,
            offer_a: (vec![], 80),
            offer_b: (vec![(500, 1)], 0),
            ok_a: true,
            ok_b: true,
        };
        let before = (loot.inventories[&1].clone(), loot.inventories[&2].clone());
        // Кинжал уходит — место освобождается, так что сделка проходит.
        execute_p2p(&s, &mut loot).unwrap();
        assert!(loot.inventories[&1].all_items().any(|i| i.uid == 500));
        assert_eq!(purse(&loot.inventories[&2]), 80);
        assert!(purse(&loot.inventories[&1]) < purse(&before.0));
    }
}
