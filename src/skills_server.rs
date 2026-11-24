//! Сервер: навыки растут от дела, умения приходят сами, поправки идут в бой и торговлю.
//!
//! Другие части сервера кладут опыт в [`PracticeQueue`] (бой — попадания и отражённые удары,
//! торговля — сделки). Здесь же:
//! * выносливость растёт от потраченных сил (бег, удары, защита);
//! * сила — от ходьбы с тяжёлой ношей;
//! * каждые полсекунды пересчитываются [`Mods`] игроков (их читает бой через `Actor::skill_mods`
//!   и торговля через [`SkillMods`]);
//! * новое умение — уведомление игроку.

use crate::combat::HitOutcome;
use crate::knowledge::{KnowledgeWorld, notice, sync_knowledge};
use crate::loot_server::LootWorld;
use crate::lore::{SKILL_COUNT, Skill};
use crate::protocol::ActorKind;
use crate::server::GameWorld;
use crate::skill_tree::{self, Mods, Perk};
use bevy::prelude::*;
use bevy_renet::RenetServer;
use std::collections::{HashMap, HashSet};

pub struct SkillsServerPlugin;

impl Plugin for SkillsServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PracticeQueue>()
            .init_resource::<SkillMods>()
            .init_resource::<SkillTrack>()
            .add_systems(
                Update,
                (debug_skills, body_practice, apply_practice, refresh_mods).chain(),
            );
    }
}

/// Опыт от дела: (игрок, навык, сколько).
#[derive(Resource, Default)]
pub struct PracticeQueue(pub Vec<(u64, Skill, f32)>);

impl PracticeQueue {
    pub fn add(&mut self, player: u64, skill: Skill, amount: f32) {
        if amount > 0.0 {
            self.0.push((player, skill, amount));
        }
    }

    /// Учится только тот, кто бьётся с настоящим противником (бойцом из мира, не с тенью).
    fn fair_target(world: &GameWorld, target: u64) -> bool {
        world.actors.contains_key(&target)
    }

    /// Удар в ближнем бою дошёл до цели: учится нападавший или защищавшийся.
    pub fn melee(&mut self, world: &GameWorld, attacker: u64, target: u64, outcome: HitOutcome) {
        let is_player = |id: u64| {
            world
                .actors
                .get(&id)
                .is_some_and(|a| a.kind == ActorKind::Player)
        };
        if !Self::fair_target(world, target) {
            return;
        }
        match outcome {
            HitOutcome::Damage { .. } if is_player(attacker) => {
                self.add(attacker, Skill::Arms, skill_tree::PRACTICE_HIT)
            }
            HitOutcome::Parried | HitOutcome::Blocked { .. } if is_player(target) => {
                self.add(target, Skill::Arms, skill_tree::PRACTICE_DEFEND)
            }
            _ => {}
        }
    }

    /// Стрела попала.
    pub fn arrow(&mut self, world: &GameWorld, owner: u64, target: u64, outcome: HitOutcome) {
        let shooter = world
            .actors
            .get(&owner)
            .is_some_and(|a| a.kind == ActorKind::Player);
        if shooter
            && owner != target
            && matches!(outcome, HitOutcome::Damage { .. })
            && Self::fair_target(world, target)
        {
            self.add(owner, Skill::Archery, skill_tree::PRACTICE_ARROW);
        }
    }
}

/// Поправки от навыков по игрокам (для торговли; бой берёт их из `Actor::skill_mods`).
#[derive(Resource, Default)]
pub struct SkillMods(pub HashMap<u64, Mods>);

impl SkillMods {
    pub fn get(&self, player: u64) -> Mods {
        self.0.get(&player).copied().unwrap_or_default()
    }
}

/// Что помним об игроках между кадрами.
#[derive(Resource, Default)]
struct SkillTrack {
    stamina: HashMap<u64, f32>,
    feet: HashMap<u64, Vec3>,
    /// Накопленный мелкий опыт (чтобы не дёргать знания каждый кадр).
    pending: HashMap<(u64, Skill), f32>,
    perks: HashMap<u64, Vec<Perk>>,
    /// Целые уровни навыков, которые видел клиент.
    shown: HashMap<u64, [i32; SKILL_COUNT]>,
    dirty: HashSet<u64>,
    timer: f32,
}

/// Выносливость от потраченных сил, сила от тяжёлой ноши.
fn body_practice(
    time: Res<Time>,
    world: Res<GameWorld>,
    loot: Option<Res<LootWorld>>,
    mods: Res<SkillMods>,
    mut track: ResMut<SkillTrack>,
    mut queue: ResMut<PracticeQueue>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let track = &mut *track;
    for (id, a) in &world.actors {
        if a.kind != ActorKind::Player {
            continue;
        }
        if !a.alive {
            track.stamina.remove(id);
            track.feet.remove(id);
            continue;
        }
        let st = a.fighter.stamina;
        if let Some(prev) = track.stamina.insert(*id, st) {
            let spent = (prev - st).max(0.0);
            // Скачок больше 40 — это смена экипировки или воскрешение, не труд.
            if spent > 0.0 && spent < 40.0 {
                *track.pending.entry((*id, Skill::Endurance)).or_default() +=
                    spent / 100.0 * skill_tree::PRACTICE_STAMINA_PER_100;
            }
        }
        let feet = a.pose.feet;
        let moved = track
            .feet
            .insert(*id, feet)
            .map_or(0.0, |p| Vec2::new(feet.x - p.x, feet.z - p.z).length());
        if moved / dt > 0.8
            && moved < 3.0
            && let Some(inv) = loot.as_ref().and_then(|l| l.inventories.get(id))
        {
            let strength = inv.equipped_stats().get(crate::items::Stat::Strength);
            let cap = crate::weight::carry_capacity(strength) + mods.get(*id).carry;
            let load = crate::weight::inventory_weight(inv);
            if load > cap * 0.7 {
                *track.pending.entry((*id, Skill::Strength)).or_default() +=
                    skill_tree::PRACTICE_LOAD_PER_SEC * dt * (load / cap).min(1.5);
            }
        }
    }
    // Мелочь копится и уходит в знания порциями.
    let ready: Vec<(u64, Skill)> = track
        .pending
        .iter()
        .filter(|(_, v)| **v >= 0.05)
        .map(|(k, _)| *k)
        .collect();
    for key in ready {
        if let Some(v) = track.pending.remove(&key) {
            queue.add(key.0, key.1, v);
        }
    }
}

fn apply_practice(
    mut queue: ResMut<PracticeQueue>,
    mut knowledge: ResMut<KnowledgeWorld>,
    mut track: ResMut<SkillTrack>,
    trade: Option<ResMut<crate::trade_server::TradeWorld>>,
) {
    // Сделки в лавках: опыт от оборота.
    if let Some(mut tw) = trade {
        for (player, turnover) in std::mem::take(&mut tw.turnover) {
            queue.add(player, Skill::Trade, skill_tree::deal_practice(turnover));
        }
    }
    for (player, skill, amount) in queue.0.drain(..) {
        if let Some(k) = knowledge.players.get_mut(&player) {
            k.practice(skill, amount);
            track.dirty.insert(player);
        }
    }
}

/// Раз в полсекунды: поправки в бой, новые умения, знания клиенту (когда сменился целый уровень).
fn refresh_mods(
    time: Res<Time>,
    mut server: ResMut<RenetServer>,
    knowledge: Res<KnowledgeWorld>,
    mut world: ResMut<GameWorld>,
    mut mods: ResMut<SkillMods>,
    mut track: ResMut<SkillTrack>,
    mut trade: Option<ResMut<crate::trade_server::TradeWorld>>,
) {
    track.timer -= time.delta_secs();
    if track.timer > 0.0 {
        return;
    }
    track.timer = 0.5;
    let track = &mut *track;
    mods.0.retain(|id, _| knowledge.players.contains_key(id));
    track
        .perks
        .retain(|id, _| knowledge.players.contains_key(id));
    track
        .shown
        .retain(|id, _| knowledge.players.contains_key(id));
    for (id, k) in &knowledge.players {
        let m = Mods::of(k);
        mods.0.insert(*id, m);
        if let Some(a) = world.actors.get_mut(id) {
            a.skill_mods = m;
        }
        if let Some(tw) = trade.as_mut()
            && tw.haggle(*id) != (m.buy, m.sell)
        {
            tw.haggle.insert(*id, (m.buy, m.sell));
        }
        let now = skill_tree::earned(k);
        let before = track.perks.insert(*id, now.clone());
        if let Some(before) = before.as_ref() {
            for p in now.iter().filter(|p| !before.contains(p)) {
                let info = p.info();
                notice(
                    &mut server,
                    *id,
                    format!(
                        "Новое умение: «{}» ({}). {} Книга навыков — K",
                        info.name,
                        info.skill.name().to_lowercase(),
                        info.effect
                    ),
                );
                track.dirty.insert(*id);
            }
        }
        let levels: [i32; SKILL_COUNT] = std::array::from_fn(|i| k.skills[i].floor() as i32);
        let old = track.shown.insert(*id, levels);
        if track.dirty.contains(id) {
            if let Some(old) = old {
                for s in Skill::ALL {
                    let i = s.index();
                    if levels[i] > old[i] && levels[i] % 5 == 0 {
                        notice(
                            &mut server,
                            *id,
                            format!("{} растёт: {}", s.name(), levels[i]),
                        );
                    }
                }
            }
            // Шлём, только если на экране что-то поменяется: целый уровень или умение.
            if old != Some(levels) || before.as_ref() != Some(&now) {
                sync_knowledge(&mut server, &knowledge, *id);
            }
            track.dirty.remove(id);
        }
    }
}

/// `MRPG_DEBUG_SKILLS=1` — новый игрок сразу с навыками и знаниями (для снимков древа).
fn debug_skills(
    mut knowledge: ResMut<KnowledgeWorld>,
    mut track: ResMut<SkillTrack>,
    mut done: Local<HashSet<u64>>,
) {
    if std::env::var("MRPG_DEBUG_SKILLS").is_err() {
        return;
    }
    use crate::lore::{Language, Lore, Technique};
    for (id, k) in knowledge.players.iter_mut() {
        if !done.insert(*id) {
            continue;
        }
        track.dirty.insert(*id);
        for (s, v) in [
            (Skill::Strength, 41.0),
            (Skill::Endurance, 37.0),
            (Skill::Arms, 26.0),
            (Skill::Archery, 12.0),
            (Skill::Smithing, 38.0),
            (Skill::Alchemy, 8.0),
            (Skill::Herbalism, 22.0),
            (Skill::Medicine, 17.0),
            (Skill::Letters, 19.0),
            (Skill::Trade, 16.0),
        ] {
            k.skills[s.index()] = v;
        }
        for l in [
            Lore::Read(Language::Greek),
            Lore::Technique(Technique::Forging),
            Lore::Technique(Technique::Quenching),
            Lore::Technique(Technique::Gathering),
            Lore::Technique(Technique::WoundWashing),
            Lore::Technique(Technique::Poultice),
        ] {
            k.learn(l);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_ignores_nothing_amounts() {
        let mut q = PracticeQueue::default();
        q.add(1, Skill::Arms, 0.0);
        q.add(1, Skill::Arms, 0.5);
        assert_eq!(q.0.len(), 1);
    }
}
