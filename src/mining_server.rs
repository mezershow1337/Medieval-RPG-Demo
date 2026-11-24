//! Сервер горного дела и углежжения: места добычи (сколько осталось, как прибывает),
//! угольные кучи и обжиговые кострища (честная симуляция `mining.rs`, идёт сама, даже
//! когда рядом никого нет), работа игроков.
//!
//! Работа руками идёт через занятия знаний (`knowledge.rs`, `Task::Mine`): там же мазки,
//! прерывание и полоска. Здесь — что можно начать, чем кончилось и мгновенные действия
//! (поджечь, отдушина, подбросить дров).

use crate::economy::give_money;
use crate::inventory::{Inventory, Target};
use crate::items::{ItemBaseId, ItemUid, Quality, Rng, UidGen, roll_item};
use crate::level::{mine_sites, pile_spots};
use crate::lore::{Knowledge, Lore, Skill, Technique};
use crate::mining::*;
use crate::mining_net::*;
use crate::protocol::encode;
use crate::work::Drill;
use bevy::prelude::*;
use bevy_renet::{RenetServer, RenetServerEvent, renet::ServerEvent};
use std::collections::HashMap;

pub struct MiningServerPlugin;

impl Plugin for MiningServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MineWorld::new()).add_observer(on_connection).add_systems(Update, tick_mines);
    }
}

/// Сколько можно поленьев положить за один заход.
const STACK_BATCH: u32 = 10;
/// Чужую готовую кучу можно разобрать, если хозяин не пришёл за ней столько секунд.
const OWNER_GRACE: f32 = 180.0;
/// Кого оповещать о прогарах и прочем (м).
const NEAR: f32 = 30.0;

pub struct SiteState {
    pub kind: SiteKind,
    pub pos: Vec3,
    pub radius: f32,
    pub charges: u8,
    regrow: f32,
}

pub struct PileState {
    pub pos: Vec3,
    pub pile: Pile,
    pub owner: Option<u64>,
    /// Секунд с тех пор, как куча готова к разборке.
    done_for: f32,
    dirty: bool,
}

#[derive(Resource)]
pub struct MineWorld {
    pub sites: HashMap<u32, SiteState>,
    pub piles: HashMap<u32, PileState>,
    rng: Rng,
    send_in: f32,
    /// Для снимков: кучи стоят в своих стадиях (не горят дальше).
    frozen: bool,
}

impl MineWorld {
    pub fn new() -> Self {
        let sites = mine_sites().iter().map(|s| (s.id, SiteState { kind: s.kind, pos: s.pos, radius: s.radius, charges: s.kind.info().charges, regrow: 0.0 })).collect();
        let mut piles: HashMap<u32, PileState> = pile_spots().iter().map(|p| (p.id, PileState { pos: p.pos, pile: Pile::new(p.kind), owner: None, done_for: 0.0, dirty: false })).collect();
        // Одна угольная куча стана уже горит — её ведут углежоги (и её видно издалека).
        if let Some(id) = pile_spots().iter().find(|p| p.kind == PileKind::Clamp).map(|p| p.id)
            && let Some(p) = piles.get_mut(&id)
        {
            p.pile.add_wood(30, 0.9);
            p.pile.add_cover(1.0);
            let _ = p.pile.light();
            p.pile.moisture = 0.0;
            p.pile.charred = 0.45;
            p.pile.heat = 0.85;
            p.pile.set_vents(2);
        }
        // Для снимков: `MRPG_MINE_DEMO=1` — кучи во всех стадиях сразу (и замирают в них).
        let frozen = std::env::var("MRPG_MINE_DEMO").is_ok();
        if frozen {
            demo_piles(&mut piles);
        }
        MineWorld { sites, piles, rng: Rng::from_time(), send_in: 0.0, frozen }
    }

    fn view(&self, id: u32) -> Option<PileView> {
        view_of(&self.piles, id)
    }

    fn views(&self) -> Vec<PileView> {
        let mut ids: Vec<u32> = self.piles.keys().copied().collect();
        ids.sort();
        ids.into_iter().filter_map(|id| self.view(id)).collect()
    }

    /// Пока идёт ворошение — куча знает, что руду ворошат.
    pub fn touch(&mut self, task: &MineTask) {
        if let MineTask::Stir { pile } = task
            && let Some(p) = self.piles.get_mut(pile)
        {
            p.pile.stirred();
        }
    }
}

/// Кучи в разных стадиях (только для снимков).
fn demo_piles(piles: &mut HashMap<u32, PileState>) {
    let mut ids: Vec<(u32, PileKind)> = piles.iter().map(|(id, p)| (*id, p.pile.kind)).collect();
    ids.sort_by_key(|(id, _)| *id);
    let clamps: Vec<u32> = ids.iter().filter(|(_, k)| *k == PileKind::Clamp).map(|(id, _)| *id).collect();
    let roasts: Vec<u32> = ids.iter().filter(|(_, k)| *k == PileKind::Roast).map(|(id, _)| *id).collect();
    // Вторая куча: наполовину укрыта; третья — кладут поленья.
    if let Some(p) = clamps.get(1).and_then(|id| piles.get_mut(id)) {
        p.pile.add_wood(34, 0.9);
        p.pile.add_cover(0.1);
        p.pile.stage = Stage::Cover;
    }
    if let Some(p) = clamps.get(2).and_then(|id| piles.get_mut(id)) {
        p.pile.add_wood(18, 0.9);
    }
    // Первое кострище рудника горит, второе — сложено с рудой.
    if let Some(p) = roasts.first().and_then(|id| piles.get_mut(id)) {
        p.pile.add_wood(8, 0.9);
        p.pile.add_ore(12, 0);
        let _ = p.pile.light();
        p.pile.heat = 0.95;
        p.pile.roast = 0.35;
    }
    if let Some(p) = roasts.get(1).and_then(|id| piles.get_mut(id)) {
        p.pile.add_wood(6, 0.9);
        p.pile.add_ore(10, 0);
    }
}

impl Default for MineWorld {
    fn default() -> Self {
        Self::new()
    }
}

/// Занятие руками у места добычи или у кучи.
#[derive(Clone, Debug, PartialEq)]
pub enum MineTask {
    Dig { site: u32 },
    StackWood { pile: u32, n: u32 },
    Cover { pile: u32 },
    Patch { pile: u32 },
    Stir { pile: u32 },
    Rake { pile: u32 },
}

/// Что вышло из просьбы игрока.
pub enum Outcome {
    /// Начать работу руками: занятие, подпись, секунды умелого, движение, навык 0..100.
    Start(MineTask, String, f32, Drill, f32),
    /// Сделано сразу (сказать игроку).
    Done(String),
}

fn count(inv: &Inventory, name: &str) -> u32 {
    inv.all_items().filter(|i| i.info().name == name).map(|i| i.count as u32).sum()
}

fn take(inv: &mut Inventory, name: &str, mut n: u32) -> u32 {
    let mut taken = 0;
    let stacks: Vec<ItemUid> = inv.all_items().filter(|i| i.info().name == name).map(|i| i.uid).collect();
    for uid in stacks {
        if n == 0 {
            break;
        }
        let Some((mut item, loc)) = inv.take(uid) else { continue };
        let used = (item.count as u32).min(n);
        n -= used;
        taken += used;
        item.count -= used as u16;
        if item.count > 0 {
            let _ = inv.place(item, loc.into());
        }
    }
    taken
}

fn has_tool(inv: &Inventory, tool: Tool) -> bool {
    inv.all_items().any(|i| tool.fits(i.info().name))
}

/// Положить в сумку; сколько не влезло.
fn give(inv: &mut Inventory, name: &str, n: u16, quality: Quality, rng: &mut Rng, uids: &mut UidGen) -> u16 {
    if n == 0 {
        return 0;
    }
    let Some(base) = ItemBaseId::find(name) else { return n };
    let item = roll_item(base, quality, n, rng, uids);
    match inv.place(item, Target::Auto) {
        Ok(_) => 0,
        Err(rest) => rest.count,
    }
}

fn near(feet: Vec3, pos: Vec3, radius: f32) -> bool {
    let d = pos - feet;
    Vec2::new(d.x, d.z).length() <= radius + MINE_REACH && d.y.abs() < 3.0
}

fn stack_verb(kind: PileKind) -> &'static str {
    match kind {
        PileKind::Clamp => "Ставите поленья стоймя вокруг кола, плотно",
        PileKind::Roast => "Кладёте дрова клеткой под руду",
    }
}

/// Игрок просит что-то сделать. `busy` — уже занят работой руками (тогда можно только мгновенное).
pub fn act(world: &mut MineWorld, k: &Knowledge, inv: &mut Inventory, feet: Vec3, player: u64, busy: bool, act: MineAct) -> Result<Outcome, String> {
    let smith = k.skill(Skill::Smithing);
    let start = |task: MineTask, verb: &str, secs: f32, drill: Drill| -> Result<Outcome, String> {
        if busy {
            return Err("Вы уже чем-то заняты".into());
        }
        Ok(Outcome::Start(task, verb.to_string(), secs, drill, smith))
    };
    if let MineAct::Dig { site } = act {
        let s = world.sites.get(&site).ok_or("Нет такого места")?;
        if !near(feet, s.pos, s.radius) {
            return Err("Подойдите ближе".into());
        }
        let info = s.kind.info();
        if let Some(t) = info.needs
            && !k.knows(Lore::Technique(t))
        {
            return Err(match s.kind {
                SiteKind::Adit => "Десятник не пускает в забой: «Крепь не знаешь — завалит. Сначала поучись»".into(),
                _ => format!("Сначала нужно знать: {}", Lore::Technique(t).name().to_lowercase()),
            });
        }
        if let Some(tool) = info.tool
            && !has_tool(inv, tool)
        {
            return Err(format!("Нужен {}", tool.name()));
        }
        if s.charges == 0 {
            return Err(match s.kind {
                SiteKind::Shaft => "Бадьи пока не готовы: в шурфе ещё не нарубили".into(),
                SiteKind::Woodlot => "Здесь всё порублено — подождите, нарубят ещё".into(),
                _ => "Здесь пока всё выбрано, нужно подождать".into(),
            });
        }
        return start(MineTask::Dig { site }, info.verb, info.secs, info.drill);
    }
    let id = match act {
        MineAct::StackWood { pile }
        | MineAct::LoadOre { pile }
        | MineAct::Cover { pile }
        | MineAct::Light { pile }
        | MineAct::Vent { pile, .. }
        | MineAct::Patch { pile }
        | MineAct::Smother { pile }
        | MineAct::Stir { pile }
        | MineAct::Feed { pile }
        | MineAct::Rake { pile } => pile,
        MineAct::Dig { .. } => unreachable!(),
    };
    let ps = world.piles.get_mut(&id).ok_or("Нет такой кучи")?;
    if !near(feet, ps.pos, 2.5) {
        return Err("Подойдите ближе".into());
    }
    let p = &mut ps.pile;
    let kind = p.kind;
    let knows_coal = k.knows(Lore::Technique(Technique::Charcoal));
    match act {
        MineAct::StackWood { .. } => {
            if p.stage != Stage::Stack {
                return Err(if kind == PileKind::Clamp { "Куча уже укрыта — поленьев не добавить".into() } else { "Кострище уже горит — подбрасывайте дров".into() });
            }
            let room = (p.wood_limits().1 - p.wood) as u32;
            if room == 0 {
                return Err("Больше не поместится".into());
            }
            let have = count(inv, WOOD);
            if have == 0 {
                return Err("Нет дров. Наколите на делянке (нужен топор) или купите у углежога".into());
            }
            let n = have.min(room).min(STACK_BATCH);
            start(MineTask::StackWood { pile: id, n }, stack_verb(kind), 0.6 * n as f32 + 1.5, Drill::Tension)
        }
        MineAct::LoadOre { .. } => {
            if kind != PileKind::Roast || p.stage != Stage::Stack {
                return Err("Сюда руду не насыпать".into());
            }
            let room = ROAST_ORE.saturating_sub(p.ore + p.bog) as u32;
            let raw = count(inv, ORE).min(room);
            let bog = count(inv, BOG_ORE).min(room - raw);
            if raw + bog == 0 {
                return Err(if room == 0 { "Кострище полно".into() } else { "Нет сырой руды (горной или болотной)".into() });
            }
            take(inv, ORE, raw);
            take(inv, BOG_ORE, bog);
            p.add_ore(raw as u16, bog as u16);
            ps.owner.get_or_insert(player);
            ps.dirty = true;
            Ok(Outcome::Done(format!("Насыпали руды: {} (на кострище {} из {ROAST_ORE})", raw + bog, ps.pile.ore + ps.pile.bog)))
        }
        MineAct::Cover { .. } => {
            if kind != PileKind::Clamp || !matches!(p.stage, Stage::Stack | Stage::Cover) {
                return Err("Укрывать нечего".into());
            }
            if p.wood < CLAMP_WOOD.0 {
                return Err(format!("Сначала сложите кучу: нужно хотя бы {} поленьев", CLAMP_WOOD.0));
            }
            if !has_tool(inv, Tool::Spade) {
                return Err("Нужен заступ — кидать землю".into());
            }
            start(MineTask::Cover { pile: id }, "Кроете кучу листвой и дёрном, засыпаете землёй", 5.0, Drill::Pump)
        }
        MineAct::Light { .. } => {
            p.light().map_err(String::from)?;
            ps.owner.get_or_insert(player);
            ps.dirty = true;
            Ok(Outcome::Done(match kind {
                PileKind::Clamp if knows_coal => "Подожгли сверху через кол. Следите за дымом и отдушинами".into(),
                PileKind::Clamp => "Подожгли кучу".into(),
                PileKind::Roast => "Кострище занялось. Ворошите руду, чтобы не спеклась".into(),
            }))
        }
        MineAct::Vent { open, .. } => {
            if kind != PileKind::Clamp || p.stage != Stage::Burning {
                return Err("Куча не горит".into());
            }
            let v = if open { (p.vents + 1).min(VENTS) } else { p.vents.saturating_sub(1) };
            p.set_vents(v);
            ps.dirty = true;
            Ok(Outcome::Done(format!("Отдушин открыто: {v} из {VENTS}")))
        }
        MineAct::Patch { .. } => {
            if kind != PileKind::Clamp || p.stage != Stage::Burning {
                return Err("Куча не горит".into());
            }
            if p.cracks <= 0.0 {
                return Err("Прогаров нет".into());
            }
            start(MineTask::Patch { pile: id }, "Засыпаете прогар землёй", 2.5, Drill::Pump)
        }
        MineAct::Smother { .. } => {
            if kind != PileKind::Clamp || p.stage != Stage::Burning {
                return Err("Куча не горит".into());
            }
            p.smother();
            ps.dirty = true;
            Ok(Outcome::Done("Закрыли отдушины, засыпали кучу землёй — пусть остынет".into()))
        }
        MineAct::Stir { .. } => {
            if kind != PileKind::Roast || p.stage != Stage::Burning {
                return Err("Кострище не горит".into());
            }
            start(MineTask::Stir { pile: id }, "Ворошите руду в огне длинной кочергой", 10.0, Drill::Stir)
        }
        MineAct::Feed { .. } => {
            if kind != PileKind::Roast || p.stage != Stage::Burning {
                return Err("Кострище не горит".into());
            }
            if p.wood >= ROAST_WOOD.1 + 4 {
                return Err("Дров и так много".into());
            }
            if take(inv, WOOD, 1) == 0 {
                return Err("Нет дров".into());
            }
            // Свежее полено: прогоревшая доля уменьшается.
            let w = p.wood as f32;
            p.charred = p.charred * w / (w + 1.0);
            p.wood += 1;
            ps.dirty = true;
            Ok(Outcome::Done("Подбросили полено".into()))
        }
        MineAct::Rake { .. } => {
            let ok = match p.stage {
                Stage::Done | Stage::Stack | Stage::Cover | Stage::Ready => true,
                Stage::Burning => p.out,
                Stage::Cooling => false,
            };
            if !ok {
                return Err(if p.stage == Stage::Cooling { "Ещё горячо — пусть остынет".into() } else { "Горит — сейчас не разобрать".into() });
            }
            if p.wood == 0 && p.ore + p.bog == 0 {
                return Err("Разбирать нечего".into());
            }
            if let Some(o) = ps.owner
                && o != player
                && ps.done_for < OWNER_GRACE
            {
                return Err("Это чужая куча — хозяин ещё придёт за ней".into());
            }
            start(MineTask::Rake { pile: id }, if kind == PileKind::Clamp { "Разгребаете кучу, выбираете уголь" } else { "Выгребаете руду из золы" }, 5.0, Drill::Pump)
        }
        MineAct::Dig { .. } => unreachable!(),
    }
}

/// Что сказать и что ещё сделать после работы руками.
pub struct Finished {
    pub notice: String,
    /// Поденная плата (нуммии), уже положена в сумку.
    pub wage: u32,
}

/// Работа руками кончилась с качеством `q`.
pub fn finish(world: &mut MineWorld, k: &mut Knowledge, inv: &mut Inventory, uids: &mut UidGen, player: u64, task: &MineTask, q: f32) -> Finished {
    let mut wage = 0;
    let MineWorld { sites, piles, rng, .. } = world;
    let notice = match *task {
        MineTask::Dig { site } => {
            let Some(s) = sites.get_mut(&site) else { return Finished { notice: String::new(), wage } };
            if s.charges == 0 {
                return Finished { notice: "Здесь уже всё выбрано".into(), wage };
            }
            let prospect = k.knows(Lore::Technique(Technique::Prospecting));
            let dug = dig_yield(s.kind, prospect, k.skill(Skill::Smithing), q, [rng.f32(), rng.f32(), rng.f32()]);
            let quality = if dug.rich { Quality::Uncommon } else { Quality::Common };
            let mut got = Vec::new();
            let mut lost = false;
            for (name, n) in &dug.items {
                let rest = give(inv, name, *n, quality, rng, uids);
                lost |= rest > 0;
                if rest < *n {
                    got.push(format!("{} ×{}", k.item_name(ItemBaseId::by_name(name)), n - rest));
                }
            }
            if got.is_empty() && !dug.items.is_empty() {
                return Finished { notice: "Нет места в сумке".into(), wage };
            }
            s.charges -= 1;
            k.practice(Skill::Smithing, 0.3);
            let mut msg = match s.kind {
                SiteKind::Shaft => {
                    wage = shaft_wage(q);
                    if wage > 0 && give_money(inv, wage, uids).is_err() {
                        wage = 0;
                    }
                    let mut m = format!("Подняли бадью. Десятник заплатил {}", crate::economy::format_money(wage));
                    if !got.is_empty() {
                        m.push_str(&format!(" и дал кусок руды: {}", got.join(", ")));
                    }
                    m
                }
                _ => format!("Добыто: {}", got.join(", ")),
            };
            if let Some(n) = dug.note {
                msg.push_str(&format!(". {n}"));
            }
            if dug.rich {
                msg.push_str(". Богатый кусок жилы!");
            }
            if lost {
                msg.push_str(". Остальное не влезло в сумку");
            }
            // Рудознатство приходит и с опытом: кто долго копает, начинает отличать руду.
            if s.kind.ore() && !prospect && rng.f32() < 0.04 + k.skill(Skill::Smithing) / 1500.0 && k.learn(Lore::Technique(Technique::Prospecting)) {
                msg.push_str(". Вы начали отличать руду от пустой породы: она тяжелее и черта у неё бурая (рудознатство)");
            }
            msg
        }
        MineTask::StackWood { pile, n } => {
            let Some(ps) = piles.get_mut(&pile) else { return Finished { notice: String::new(), wage } };
            let taken = take(inv, WOOD, n);
            // Знающий ставит плотно, без щелей: качество укладки выше.
            let knows = k.knows(Lore::Technique(Technique::Charcoal));
            let q = if knows { 0.35 + 0.65 * q } else { 0.85 * q };
            let added = ps.pile.add_wood(taken as u16, q);
            if added < taken as u16 {
                give(inv, WOOD, taken as u16 - added, Quality::Common, rng, uids);
            }
            ps.owner.get_or_insert(player);
            ps.dirty = true;
            k.practice(Skill::Smithing, 0.2);
            let (lo, hi) = ps.pile.wood_limits();
            let mut m = format!("Сложено поленьев: {} из {hi}", ps.pile.wood);
            if ps.pile.wood < lo {
                m.push_str(&format!(" (нужно хотя бы {lo})"));
            }
            if ps.pile.kind == PileKind::Roast && ps.pile.ore + ps.pile.bog == 0 {
                m.push_str(". Теперь насыпьте руду");
            }
            m
        }
        MineTask::Cover { pile } => {
            let Some(ps) = piles.get_mut(&pile) else { return Finished { notice: String::new(), wage } };
            ps.pile.add_cover(q);
            if ps.pile.stage == Stage::Stack {
                ps.pile.stage = Stage::Cover;
            }
            if ps.pile.cover >= 1.0 {
                ps.pile.stage = Stage::Ready;
            }
            ps.dirty = true;
            k.practice(Skill::Smithing, 0.2);
            if ps.pile.cover >= 1.0 { "Куча укрыта целиком — можно поджигать".into() } else { format!("Укрыто примерно на {:.0}%", ps.pile.cover * 100.0) }
        }
        MineTask::Patch { pile } => {
            let Some(ps) = piles.get_mut(&pile) else { return Finished { notice: String::new(), wage } };
            ps.pile.patch(q);
            ps.dirty = true;
            if ps.pile.cracks > 0.0 { "Прогар засыпан не до конца".into() } else { "Прогар засыпан".into() }
        }
        MineTask::Stir { pile } => {
            if let Some(ps) = piles.get_mut(&pile) {
                ps.pile.stirred();
            }
            k.practice(Skill::Smithing, 0.15);
            "Поворошили руду".into()
        }
        MineTask::Rake { pile } => {
            let Some(ps) = piles.get_mut(&pile) else { return Finished { notice: String::new(), wage } };
            let kind = ps.pile.kind;
            // Не дожжённое (разобрали раньше) — дрова и руда возвращаются как есть.
            let h = if matches!(ps.pile.stage, Stage::Stack | Stage::Cover | Stage::Ready) {
                Harvest { wood: ps.pile.wood, ore: ps.pile.ore, bog: ps.pile.bog, ..default() }
            } else {
                ps.pile.harvest(q)
            };
            let was_done = ps.pile.stage == Stage::Done;
            let good = match kind {
                PileKind::Clamp => h.charcoal as f32 >= ps.pile.wood as f32 * COAL_PER_WOOD * 0.6,
                PileKind::Roast => h.roasted as f32 >= (ps.pile.ore + ps.pile.bog) as f32 * 0.7,
            };
            let mut lost = 0;
            let mut parts = Vec::new();
            for (name, n) in [(CHARCOAL, h.charcoal), (ROASTED_ORE, h.roasted), (ORE, h.ore), (BOG_ORE, h.bog), (WOOD, h.wood), (WASTE, h.waste)] {
                if n == 0 {
                    continue;
                }
                let rest = give(inv, name, n, Quality::Common, rng, uids);
                lost += rest;
                if rest < n {
                    let label = if name == WOOD && was_done { "Головни (недожог)".to_string() } else { name.to_string() };
                    parts.push(format!("{label} ×{}", n - rest));
                }
            }
            ps.pile = Pile::new(kind);
            ps.owner = None;
            ps.done_for = 0.0;
            ps.dirty = true;
            k.practice(Skill::Smithing, if was_done { 1.0 } else { 0.2 });
            let mut m = if parts.is_empty() { "Ничего путного не вышло: одна зола".to_string() } else { format!("Достали: {}", parts.join(", ")) };
            if lost > 0 {
                m.push_str(&format!(". В сумку не влезло: {lost} шт. — осталось в золе"));
            }
            // Удачная куча учит сама: понял, по какому дыму глушить.
            let tech = if kind == PileKind::Clamp { Technique::Charcoal } else { Technique::Roasting };
            if was_done && good && !k.knows(Lore::Technique(tech)) && rng.f32() < 0.3 && k.learn(Lore::Technique(tech)) {
                m.push_str(&format!(". Вы поняли, как это делается: {}", Lore::Technique(tech).name().to_lowercase()));
            }
            m
        }
    };
    Finished { notice, wage }
}

fn on_connection(event: On<RenetServerEvent>, mut server: ResMut<RenetServer>, world: Res<MineWorld>) {
    if let ServerEvent::ClientConnected { client_id } = &event.0 {
        let mut sites: Vec<(u32, u8)> = world.sites.iter().map(|(id, s)| (*id, s.charges)).collect();
        sites.sort();
        server.send_message(*client_id, CH_MINE, encode(&MineServerMessage::Sites(sites)));
        server.send_message(*client_id, CH_MINE, encode(&MineServerMessage::Piles(world.views())));
    }
}

fn tick_mines(time: Res<Time>, mut server: ResMut<RenetServer>, mut world: ResMut<MineWorld>, game: Res<crate::server::GameWorld>) {
    let dt = time.delta_secs();
    let MineWorld { sites, piles, rng, send_in, frozen } = &mut *world;
    let dt_piles = if *frozen { 0.0 } else { dt };
    for (id, s) in sites.iter_mut() {
        let info = s.kind.info();
        if s.charges >= info.charges {
            s.regrow = 0.0;
            continue;
        }
        s.regrow += dt;
        if s.regrow >= info.regrow {
            s.regrow = 0.0;
            s.charges += 1;
            server.broadcast_message(CH_MINE, encode(&MineServerMessage::SiteCharges { id: *id, charges: s.charges }));
        }
    }
    let mut notes: Vec<(Vec3, &'static str)> = Vec::new();
    for ps in piles.values_mut() {
        let before = (ps.pile.stage, ps.pile.smoke(), ps.pile.vents, (ps.pile.cracks * 4.0) as i32, ps.pile.out);
        for e in ps.pile.step(dt_piles, rng.f32()) {
            let line = match (ps.pile.kind, e) {
                (_, PileEvent::Crack) => "В кровле угольной кучи прогар — оттуда пламя!",
                (_, PileEvent::WentOut) => "Куча угасла",
                (_, PileEvent::Blue) => "Дым над угольной кучей поголубел",
                (_, PileEvent::BurntDown) => "Кострище прогорело, руда остывает в золе",
                (PileKind::Clamp, PileEvent::Cooled) => "Угольная куча остыла — можно разбирать",
                (PileKind::Roast, PileEvent::Cooled) => "Руда на кострище остыла — можно выгребать",
                (_, PileEvent::Sintering) => "Руда на кострище спекается в корку!",
            };
            notes.push((ps.pos, line));
        }
        if ps.pile.stage == Stage::Done {
            ps.done_for += dt;
        }
        let after = (ps.pile.stage, ps.pile.smoke(), ps.pile.vents, (ps.pile.cracks * 4.0) as i32, ps.pile.out);
        if before != after {
            ps.dirty = true;
        }
    }
    // Оповестить тех, кто рядом.
    for (pos, line) in notes {
        for (pid, a) in &game.actors {
            if a.pose.feet.distance(pos) < NEAR && server.is_connected(*pid) {
                crate::knowledge::notice(&mut server, *pid, line);
            }
        }
    }
    // Рядом стоящим — подробно и часто; всем — когда видно со стороны (стадия, дым).
    *send_in -= dt;
    let periodic = *send_in <= 0.0;
    if periodic {
        *send_in = 0.5;
    }
    let ids: Vec<u32> = piles.keys().copied().collect();
    for id in ids {
        let dirty = piles.get(&id).is_some_and(|p| p.dirty);
        let lit = piles.get(&id).is_some_and(|p| p.pile.lit() || p.pile.stage == Stage::Cooling);
        if !(dirty || periodic && lit) {
            continue;
        }
        if let Some(p) = piles.get_mut(&id) {
            p.dirty = false;
        }
        if let Some(view) = view_of(piles, id) {
            let bytes = encode(&MineServerMessage::Pile(view));
            if dirty {
                server.broadcast_message(CH_MINE, bytes);
            } else {
                let pos = piles[&id].pos;
                for (pid, a) in &game.actors {
                    if a.pose.feet.distance(pos) < 60.0 && server.is_connected(*pid) {
                        server.send_message(*pid, CH_MINE, bytes.clone());
                    }
                }
            }
        }
    }
}

fn view_of(piles: &HashMap<u32, PileState>, id: u32) -> Option<PileView> {
    let p = piles.get(&id)?;
    Some(PileView {
        id,
        kind: p.pile.kind,
        pos: p.pos.into(),
        stage: p.pile.stage,
        wood: p.pile.wood,
        ore: p.pile.ore + p.pile.bog,
        cover: p.pile.cover,
        vents: p.pile.vents,
        smoke: p.pile.smoke(),
        heat: p.pile.heat,
        cracks: p.pile.cracks,
        pile: p.pile.clone(),
    })
}

/// Изменились места добычи (после копки) — разослать.
pub fn broadcast_site(server: &mut RenetServer, world: &MineWorld, site: u32) {
    if let Some(s) = world.sites.get(&site) {
        server.broadcast_message(CH_MINE, encode(&MineServerMessage::SiteCharges { id: site, charges: s.charges }));
    }
}

/// Куча изменилась от действия игрока — разослать сразу.
pub fn broadcast_pile(server: &mut RenetServer, world: &mut MineWorld, pile: u32) {
    if let Some(p) = world.piles.get_mut(&pile) {
        p.dirty = false;
    }
    if let Some(v) = world.view(pile) {
        server.broadcast_message(CH_MINE, encode(&MineServerMessage::Pile(v)));
    }
}

/// К какой куче или месту относится действие (чтобы разослать после него).
pub fn target_of(act: &MineAct) -> (Option<u32>, Option<u32>) {
    match *act {
        MineAct::Dig { site } => (Some(site), None),
        MineAct::StackWood { pile }
        | MineAct::LoadOre { pile }
        | MineAct::Cover { pile }
        | MineAct::Light { pile }
        | MineAct::Vent { pile, .. }
        | MineAct::Patch { pile }
        | MineAct::Smother { pile }
        | MineAct::Stir { pile }
        | MineAct::Feed { pile }
        | MineAct::Rake { pile } => (None, Some(pile)),
    }
}

pub fn task_target(task: &MineTask) -> (Option<u32>, Option<u32>) {
    match *task {
        MineTask::Dig { site } => (Some(site), None),
        MineTask::StackWood { pile, .. } | MineTask::Cover { pile } | MineTask::Patch { pile } | MineTask::Stir { pile } | MineTask::Rake { pile } => (None, Some(pile)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mine_sites_and_piles_exist_in_the_world() {
        let w = MineWorld::new();
        for kind in SiteKind::ALL {
            assert!(w.sites.values().any(|s| s.kind == kind), "нет места {kind:?}");
        }
        assert!(w.piles.values().filter(|p| p.pile.kind == PileKind::Clamp).count() >= 2);
        assert!(w.piles.values().any(|p| p.pile.kind == PileKind::Roast));
        // Одна куча углежогов уже горит.
        assert!(w.piles.values().any(|p| p.pile.lit()));
    }

    #[test]
    fn adit_needs_mining_lore_and_a_pick() {
        let mut w = MineWorld::new();
        let (id, pos) = w.sites.iter().find(|(_, s)| s.kind == SiteKind::Adit).map(|(id, s)| (*id, s.pos)).unwrap();
        let mut inv = Inventory::new((6, 6));
        let k = Knowledge::default();
        assert!(act(&mut w, &k, &mut inv, pos, 1, false, MineAct::Dig { site: id }).is_err());
        let mut k = Knowledge::default();
        k.learn(Lore::Technique(Technique::Mining));
        let e = act(&mut w, &k, &mut inv, pos, 1, false, MineAct::Dig { site: id }).err().unwrap();
        assert!(e.contains("кайло"), "{e}");
    }
}
