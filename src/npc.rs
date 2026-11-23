//! Искусственный интеллект врагов (работает только на сервере).
//!
//! Люди Херсона и его округи около 510 г.: стратиоты и букелларии гарнизона (щит и спата,
//! держат строй и закрываются), готы (топор и круглый щит, напористые), аланы (копьё,
//! держат дистанцию) и кочевники-лучники (стреляют издали, вблизи берутся за нож).

use crate::collision::{Body, LevelColliders};
use crate::combat::{self, Action, Fighter, Pose, WeaponKind, line_of_sight};
use crate::factions::Faction;
use crate::level::NpcGroup;
use crate::protocol::ActorKind;
use bevy::prelude::*;

/// Простой генератор случайных чисел (xorshift), чтобы не тянуть отдельную библиотеку.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next_f32()
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NpcSpawn {
    pub kind: ActorKind,
    pub pos: Vec3,
    pub yaw: f32,
    /// Чей это человек. `None` — разбойник, нападает на всех.
    pub faction: Option<Faction>,
    /// Насколько далеко бродит вокруг поста.
    pub wander: f32,
}

impl NpcSpawn {
    /// Разбойник без фракции, стоит на месте.
    pub fn brigand(kind: ActorKind, pos: Vec3, yaw: f32) -> Self {
        Self { kind, pos, yaw, faction: None, wander: 0.0 }
    }
}

/// Где стоят враги в начале и куда возрождаются.
pub fn npc_spawns() -> Vec<NpcSpawn> {
    let posts = crate::level::npc_posts();
    if !posts.is_empty() {
        return spawns_from_posts(&posts);
    }
    // Старый уровень без постов: типы раздаём по кругу; последний — букелларий (самый опасный).
    const ORDER: [ActorKind; 5] = [ActorKind::Stratiotes, ActorKind::GothWarrior, ActorKind::AlanLancer, ActorKind::HunArcher, ActorKind::GothWarrior];
    let hints = crate::level::npc_spawn_hints();
    let last = hints.len().saturating_sub(1);
    hints
        .into_iter()
        .enumerate()
        .map(|(i, pos)| {
            let kind = if i == last && i > 0 { ActorKind::Bucellarius } else { ORDER[i % ORDER.len()] };
            let (dx, dz) = (0.0 - pos.x, 32.0 - pos.z);
            NpcSpawn::brigand(kind, pos, (-dx).atan2(-dz))
        })
        .collect()
}

/// Кто стоит на постах уровня: гарнизон — стратиоты (на больших обходах — букелларии),
/// готы — воины с топорами, аланы — копейщики, кочевники — лучники, разбойники — сброд всех народов.
fn spawns_from_posts(posts: &[crate::level::NpcPost]) -> Vec<NpcSpawn> {
    const BRIGANDS: [ActorKind; 3] = [ActorKind::GothWarrior, ActorKind::HunArcher, ActorKind::AlanLancer];
    let (mut garrison, mut brigands) = (0usize, 0usize);
    posts
        .iter()
        .map(|p| {
            let kind = match p.group {
                NpcGroup::Garrison => {
                    garrison += 1;
                    if p.wander >= 10.0 && garrison % 2 == 1 { ActorKind::Bucellarius } else { ActorKind::Stratiotes }
                }
                NpcGroup::Goths => ActorKind::GothWarrior,
                NpcGroup::Alans => ActorKind::AlanLancer,
                NpcGroup::Nomads => ActorKind::HunArcher,
                NpcGroup::Brigands => {
                    brigands += 1;
                    BRIGANDS[(brigands - 1) % BRIGANDS.len()]
                }
            };
            NpcSpawn { kind, pos: p.pos, yaw: p.yaw, faction: p.group.faction(), wander: p.wander }
        })
        .collect()
}

/// Боевые свойства и повадки типа бойца.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub hp: f32,
    pub weapon: WeaponKind,
    pub shield: bool,
    /// Броня (см. `combat::armor_reduction`).
    pub armor: f32,
    pub damage_mult: f32,
    /// Скорость ходьбы, м/с (игрок ходит 4 м/с).
    pub speed: f32,
    /// Вероятность поднять защиту на чужой замах.
    pub defend_chance: f32,
    /// Пауза между атаками, с.
    pub cooldown: (f32, f32),
    /// Сколько ждать, пока цель стоит в стойке, прежде чем бить в лоб.
    pub patience: f32,
    /// С какого расстояния замечает игрока.
    pub aggro: f32,
}

pub fn profile(kind: ActorKind) -> Profile {
    match kind {
        ActorKind::Player => Profile {
            hp: crate::server::PLAYER_BASE_HP,
            weapon: WeaponKind::Spatha,
            shield: false,
            armor: 0.0,
            damage_mult: 1.0,
            speed: crate::player::WALK_SPEED,
            defend_chance: 0.0,
            cooldown: (0.0, 0.0),
            patience: 0.0,
            aggro: 0.0,
        },
        // Обученный солдат: дисциплинированно закрывается щитом, бьёт выверенно.
        ActorKind::Stratiotes => Profile {
            hp: 130.0,
            weapon: WeaponKind::Spatha,
            shield: true,
            armor: 35.0,
            damage_mult: 0.85,
            speed: 2.3,
            defend_chance: 0.6,
            cooldown: (1.8, 3.2),
            patience: 2.5,
            aggro: 13.0,
        },
        // Ветеран в ламеллярном доспехе: медленный, но крепкий и почти всегда закрывается.
        ActorKind::Bucellarius => Profile {
            hp: 190.0,
            weapon: WeaponKind::Spatha,
            shield: true,
            armor: 75.0,
            damage_mult: 1.1,
            speed: 2.0,
            defend_chance: 0.75,
            cooldown: (1.6, 2.8),
            patience: 2.0,
            aggro: 13.0,
        },
        // Гот почти без доспеха: напирает, часто рубит, щит поднимает редко.
        ActorKind::GothWarrior => Profile {
            hp: 125.0,
            weapon: WeaponKind::Axe,
            shield: true,
            armor: 15.0,
            damage_mult: 1.0,
            speed: 2.6,
            defend_chance: 0.3,
            cooldown: (1.2, 2.4),
            patience: 1.5,
            aggro: 15.0,
        },
        // Алан держит врага на длине копья.
        ActorKind::AlanLancer => Profile {
            hp: 150.0,
            weapon: WeaponKind::Spear,
            shield: false,
            armor: 50.0,
            damage_mult: 1.0,
            speed: 2.2,
            defend_chance: 0.3,
            cooldown: (2.0, 3.4),
            patience: 2.5,
            aggro: 15.0,
        },
        // Кочевник стреляет издали и отходит, вблизи отбивается ножом.
        ActorKind::HunArcher => Profile {
            hp: 100.0,
            weapon: WeaponKind::Bow,
            shield: false,
            armor: 10.0,
            damage_mult: 1.0,
            speed: 2.6,
            defend_chance: 0.15,
            cooldown: (2.2, 3.5),
            patience: 1.5,
            aggro: 26.0,
        },
    }
}

pub fn fighter_for(kind: ActorKind) -> Fighter {
    let p = profile(kind);
    Fighter::new(p.weapon, p.hp, p.shield)
}

/// Находит свободное место рядом с точкой (на случай, если уровень поменяли).
pub fn find_free_spot(level: &LevelColliders, pos: Vec3) -> Vec3 {
    for radius in [0.0f32, 1.0, 2.0, 3.0, 4.0, 6.0] {
        for i in 0..12 {
            let a = i as f32 / 12.0 * std::f32::consts::TAU;
            for dy in [0.1f32, 0.6, 1.2, 2.5] {
                let p = pos + Vec3::new(a.cos() * radius, dy, a.sin() * radius);
                if level.body_is_free(&Body::STANDING, p) {
                    return p;
                }
            }
            if radius == 0.0 {
                break;
            }
        }
    }
    pos + Vec3::Y * 3.0
}

/// Краткая сводка о другом бойце, которую видит ИИ.
#[derive(Clone, Copy)]
pub struct Seen {
    pub id: u64,
    pub kind: ActorKind,
    pub pose: Pose,
    pub action: Action,
    pub weapon: WeaponKind,
    /// Защищается щитом.
    pub shield: bool,
    pub alive: bool,
    /// Можно ли нападать (после возрождения игрок пару секунд под защитой).
    pub targetable: bool,
}


/// Множитель урона для разных бойцов (баланс).
pub fn damage_mult(kind: ActorKind) -> f32 {
    profile(kind).damage_mult
}

/// Броня врагов (снижает получаемый урон, см. `combat::armor_reduction`).
pub fn armor(kind: ActorKind) -> f32 {
    profile(kind).armor
}

#[derive(Default)]
pub struct Brain {
    pub home: Vec3,
    pub home_yaw: f32,
    target: Option<u64>,
    velocity: Vec3,
    grounded: bool,
    attack_cooldown: f32,
    strafe: f32,
    strafe_timer: f32,
    target_was_winding_up: bool,
    /// Решили защищаться от текущего замаха.
    want_guard: bool,
    /// Сколько ещё держать стойку после того, как угроза прошла.
    guard_linger: f32,
    /// Сколько уже ждём, пока цель держит защиту.
    patience: f32,
    /// Чей человек (None — разбойник, враг всем игрокам).
    pub faction: Option<Faction>,
    /// Игроки, с которыми у фракции вражда (обновляет сервер по репутации).
    pub hostile_players: Vec<u64>,
    /// Игроки, напавшие на этого человека или его товарищей.
    pub provoked: Vec<u64>,
    wander: f32,
    stroll_to: Option<Vec3>,
    stroll_timer: f32,
}

impl Brain {
    pub fn new(spawn: &NpcSpawn) -> Self {
        Self {
            home: spawn.pos,
            home_yaw: spawn.yaw,
            attack_cooldown: 1.5,
            faction: spawn.faction,
            wander: spawn.wander,
            stroll_timer: 3.0,
            ..default()
        }
    }

    /// Нападать ли на этого игрока.
    pub fn hostile_to(&self, player: u64) -> bool {
        self.faction.is_none() || self.provoked.contains(&player) || self.hostile_players.contains(&player)
    }

    /// На него или его товарищей напали.
    pub fn provoke(&mut self, player: u64) {
        if !self.provoked.contains(&player) {
            self.provoked.push(player);
        }
    }
}

const AGGRO_RANGE: f32 = 13.0;
const LOSE_RANGE: f32 = 24.0;
const GRAVITY: f32 = 18.0;

fn yaw_towards(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.z)
}

fn turn_towards(current: f32, target: f32, max_step: f32) -> f32 {
    let mut diff = (target - current) % std::f32::consts::TAU;
    if diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    } else if diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    current + diff.clamp(-max_step, max_step)
}

/// Один шаг мышления и движения врага.

/// Один шаг мышления и движения врага.
#[allow(clippy::too_many_arguments)]
pub fn think(
    brain: &mut Brain,
    kind: ActorKind,
    pose: &mut Pose,
    fighter: &mut Fighter,
    others: &[Seen],
    level: &LevelColliders,
    rng: &mut Rng,
    dt: f32,
) {
    brain.attack_cooldown -= dt;
    brain.strafe_timer -= dt;
    let prof = profile(kind);
    // Лучник вблизи берётся за нож, отойдя — снова за лук.
    if kind == ActorKind::HunArcher && matches!(fighter.action, Action::Idle) {
        let close = brain.target.and_then(|id| others.iter().find(|o| o.id == id)).map(|t| t.pose.feet.distance(pose.feet));
        match (fighter.weapon, close) {
            (WeaponKind::Bow, Some(d)) if d < 3.2 => fighter.weapon = WeaponKind::Seax,
            (WeaponKind::Seax, d) if d.is_none_or(|d| d > 6.0) => fighter.weapon = WeaponKind::Bow,
            _ => {}
        }
    }
    let stats = fighter.weapon.stats();
    let ranged = fighter.weapon.ranged();
    let range = if ranged { 22.0 } else { stats.reach + 0.45 };
    // Враги заметно медленнее игрока (4 м/с), от них можно отойти.
    let speed = prof.speed;

    // --- Выбор цели ---
    let eye = pose.shoulder();
    let current = brain.target.and_then(|id| others.iter().find(|o| o.id == id && o.alive));
    let current = current.filter(|t| t.kind != ActorKind::Player || brain.hostile_to(t.id));
    // Далеко от поста не уходит: стража не гонится за вами через полгорода.
    let leash = brain.wander.max(10.0) + 25.0;
    let current = current.filter(|_| pose.feet.distance(brain.home) < leash);
    let target = match current {
        Some(t) if t.pose.feet.distance(pose.feet) < LOSE_RANGE.max(prof.aggro + 8.0) => Some(*t),
        _ => others
            .iter()
            .filter(|o| o.alive && o.targetable && o.kind == ActorKind::Player && brain.hostile_to(o.id))
            .filter(|o| o.pose.feet.distance(pose.feet) < prof.aggro.max(AGGRO_RANGE * 0.5))
            .filter(|o| line_of_sight(level, eye, o.pose.shoulder()))
            .min_by(|a, b| a.pose.feet.distance(pose.feet).total_cmp(&b.pose.feet.distance(pose.feet)))
            .copied(),
    };
    brain.target = target.map(|t| t.id);

    let can_act = !matches!(fighter.action, Action::Stagger { .. } | Action::Flinch { .. } | Action::GuardStagger { .. } | Action::Dead);
    let mut wish = Vec3::ZERO;

    if let Some(t) = target {
        let to = t.pose.feet - pose.feet;
        let dist = Vec2::new(to.x, to.z).length();
        let desired_yaw = yaw_towards(pose.feet, t.pose.feet);
        let turn_rate = match fighter.action {
            Action::Strike { .. } => 0.8,
            Action::Windup { .. } => 2.0,
            Action::Stagger { .. } | Action::GuardStagger { .. } => 0.0,
            _ => 3.5,
        };
        pose.yaw = turn_towards(pose.yaw, desired_yaw, turn_rate * dt);
        let forward = pose.forward_flat();
        let right = Vec3::new(-forward.z, 0.0, forward.x);
        let aligned = forward.dot(Vec3::new(to.x, 0.0, to.z).normalize_or_zero()) > 0.9;

        // Цель держит защиту лицом к нам? Тогда обходим сбоку, а не бьём в лоб.
        let target_guarding = matches!(t.action, Action::Guard { .. } | Action::GuardStagger { .. }) && combat::facing(&t.pose, pose.feet);
        if target_guarding {
            brain.patience += dt;
        } else {
            brain.patience = 0.0;
        }

        let sees = line_of_sight(level, eye, t.pose.shoulder());
        if ranged {
            // Стрелок держит дистанцию 9–18 м и подходит, только если не видит цель.
            if !sees || dist > 18.0 {
                wish += forward;
            } else if dist < 9.0 {
                wish -= forward * 0.8;
            }
            // Целимся с упреждением на падение стрелы.
            let aim_to = t.pose.feet + Vec3::Y * 1.2;
            let want_pitch = combat::ballistic_pitch(combat::arrow_launch(pose).0, aim_to);
            pose.pitch += (want_pitch - pose.pitch).clamp(-2.0 * dt, 2.0 * dt);
        } else if dist > range {
            wish += forward;
        } else if dist < range - if fighter.weapon.thrust_only() { 0.9 } else { 0.7 } {
            wish -= forward * 0.6;
        }
        if dist < range + 1.5 {
            if target_guarding && brain.strafe.abs() < 0.5 {
                brain.strafe = if rng.chance(0.5) { 0.9 } else { -0.9 };
                brain.strafe_timer = rng.range(1.5, 2.5);
            } else if brain.strafe_timer <= 0.0 {
                brain.strafe = if rng.chance(0.35) { 0.0 } else if rng.chance(0.5) { 0.4 } else { -0.4 };
                brain.strafe_timer = rng.range(1.5, 3.0);
            }
            wish += right * brain.strafe;
        }

        // --- Защита: на начало вражеского замаха решаем, поднимать ли стойку ---
        let winding = matches!(t.action, Action::Windup { .. });
        // Угроза — когда цель в замахе и её оружие почти достаёт.
        let threat_range = if t.weapon.ranged() { 30.0 } else { t.weapon.stats().reach + 2.0 };
        if winding && !brain.target_was_winding_up && dist < threat_range {
            // Без сил стойку не держат.
            brain.want_guard = fighter.weapon.can_guard() && fighter.stamina > 15.0 && rng.chance(prof.defend_chance);
        }
        brain.target_was_winding_up = winding;
        let threat = matches!(t.action, Action::Windup { .. } | Action::Strike { .. });
        if brain.want_guard {
            if threat {
                brain.guard_linger = 0.35;
                if can_act && !fighter.guard_held && matches!(fighter.action, Action::Idle | Action::Windup { .. }) {
                    fighter.set_guard(true);
                }
            } else {
                brain.guard_linger -= dt;
                if brain.guard_linger <= 0.0 {
                    brain.want_guard = false;
                }
            }
        }
        if !brain.want_guard && fighter.guard_held {
            fighter.set_guard(false);
        }

        // --- Атака ---
        // Ответный удар после парирования — сразу.
        if fighter.can_riposte() && fighter.try_attack() {
            brain.want_guard = false;
            fighter.guard_held = false;
        }
        let punish = matches!(t.action, Action::Stagger { .. });
        let in_reach = dist <= range + 0.15;
        // По защищающемуся бьём, только если зашли сбоку или он слишком долго стоит в стойке.
        // Стрелы щит держит, а оружием их не отбить, так что лучнику стойка цели не помеха.
        let shield_up = target_guarding && others.iter().any(|o| o.id == t.id && o.shield);
        let blocked_by_guard = if ranged { shield_up && brain.patience < prof.patience } else { target_guarding && brain.patience < prof.patience };
        let ready = if ranged { sees && (pose.pitch - combat::ballistic_pitch(combat::arrow_launch(pose).0, t.pose.feet + Vec3::Y * 1.2)).abs() < 0.08 } else { true };
        // Если сил мало, отдыхаем, а не машем впустую.
        let rested = fighter.stamina >= stats.stamina_cost;
        if can_act && in_reach && aligned && ready && rested && !brain.want_guard && !blocked_by_guard && (brain.attack_cooldown <= 0.0 || punish) {
            if fighter.try_attack() {
                brain.attack_cooldown = rng.range(prof.cooldown.0, prof.cooldown.1);
                brain.patience = 0.0;
            }
        }
    } else {
        brain.want_guard = false;
        pose.pitch *= 1.0 - (3.0 * dt).min(1.0);
        if fighter.guard_held {
            fighter.set_guard(false);
        }
        // Без цели: обход вокруг поста (если пост позволяет) или стоим на месте.
        brain.stroll_timer -= dt;
        if brain.wander > 2.0 && brain.stroll_timer <= 0.0 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let r = rng.range(0.0, brain.wander);
            brain.stroll_to = Some(brain.home + Vec3::new(a.cos() * r, 0.0, a.sin() * r));
            brain.stroll_timer = rng.range(8.0, 16.0);
        }
        let goal = brain.stroll_to.unwrap_or(brain.home);
        let to_goal = goal - pose.feet;
        if Vec2::new(to_goal.x, to_goal.z).length() > 1.0 {
            pose.yaw = turn_towards(pose.yaw, yaw_towards(pose.feet, goal), 3.0 * dt);
            wish = pose.forward_flat() * if brain.stroll_to.is_some() { 0.45 } else { 0.6 };
        } else if brain.stroll_to.take().is_none() {
            pose.yaw = turn_towards(pose.yaw, brain.home_yaw, 1.5 * dt);
        }
    }

    // --- Движение с физикой ---
    let move_speed = speed * fighter.move_speed_mult();
    let wish = wish.clamp_length_max(1.0) * move_speed;
    brain.velocity.x = wish.x;
    brain.velocity.z = wish.z;
    brain.velocity.y -= GRAVITY * dt;
    let mut feet = pose.feet;
    let result = level.move_body(&Body::STANDING, &mut feet, brain.velocity * dt, brain.grounded);
    brain.grounded = result.grounded;
    if result.grounded {
        brain.velocity.y = -1.0;
    }
    if feet.y < -30.0 {
        feet = brain.home + Vec3::Y * 0.5;
        brain.velocity = Vec3::ZERO;
    }
    pose.feet = feet;
}

#[cfg(test)]
mod tests {
    use super::*;
    const PI_: f32 = std::f32::consts::PI;
    use crate::combat::{Contact, HitOutcome, blade_segment, resolve_contact, trace_blade};

    /// Прогоняет поединок игрока со стратегией `player_brain` против врага. Возвращает статистику.
    fn fight(kind: ActorKind, player_guard: bool, seconds: f32) -> (Fighter, Fighter, [u32; 3]) {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn::brigand(kind, Vec3::new(10.0, 0.0, 0.0), 0.0);
        let mut brain = Brain::new(&spawn);
        let mut npc_pose = Pose { feet: find_free_spot(&level, spawn.pos), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut npc = fighter_for(kind);
        let mut player = fighter_for(ActorKind::Player);
        let mut player_pose = Pose { feet: find_free_spot(&level, Vec3::new(10.0, 0.0, 2.0)), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut rng = Rng::new(7);
        let dt = 1.0 / 60.0;
        let mut stats = [0u32; 3]; // урон по врагу, отражено врагом, урон по игроку
        for _ in 0..(seconds / dt) as usize {
            player_pose.yaw = yaw_towards(player_pose.feet, npc_pose.feet);
            // Игрок сближается до длины меча (против копья иначе не достать).
            let to = npc_pose.feet - player_pose.feet;
            if Vec2::new(to.x, to.z).length() > 1.2 {
                let step = Vec3::new(to.x, 0.0, to.z).normalize_or_zero() * 4.0 * player.move_speed_mult() * dt;
                let mut feet = player_pose.feet;
                level.move_body(&Body::STANDING, &mut feet, step, true);
                player_pose.feet = feet;
            }
            if player_guard {
                // Держим защиту, а после парирования бьём в ответ.
                if player.can_riposte() {
                    player.try_attack();
                } else {
                    player.set_guard(true);
                }
            } else {
                player.try_attack();
            }
            let seen = [Seen { id: 1, kind: ActorKind::Player, pose: player_pose, action: player.action, weapon: player.weapon, shield: false, alive: true, targetable: true }];
            think(&mut brain, kind, &mut npc_pose, &mut npc, &seen, &level, &mut rng, dt);
            npc.tick(dt);
            player.tick(dt);
            if let Some((swing, p)) = player.strike_progress() {
                let (h, t) = blade_segment(&player_pose, player.weapon, swing, p);
                if !player.hit_this_swing.contains(&2) {
                    if let Some(c) = trace_blade(h, t, &player_pose, &npc, &npc_pose) {
                        player.hit_this_swing.push(2);
                        match resolve_contact(&mut player, &mut npc, c, 22.0) {
                            HitOutcome::Damage { .. } => stats[0] += 1,
                            _ => stats[1] += 1,
                        }
                    }
                }
            }
            if let Some((swing, p)) = npc.strike_progress() {
                let (h, t) = blade_segment(&npc_pose, npc.weapon, swing, p);
                if !npc.hit_this_swing.contains(&1) {
                    if let Some(c) = trace_blade(h, t, &npc_pose, &player, &player_pose) {
                        npc.hit_this_swing.push(1);
                        if matches!(c, Contact::Body { .. }) {
                            stats[2] += 1;
                        }
                        resolve_contact(&mut npc, &mut player, c, 15.0);
                    }
                }
            }
            if npc.is_dead() || player.is_dead() {
                break;
            }
        }
        (npc, player, stats)
    }

    /// Рыцарь подходит к игроку, который только стоит в стойке… и обходит её.
    #[test]
    fn knight_hits_idle_player() {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn::brigand(ActorKind::Stratiotes, Vec3::new(10.0, 0.0, 0.0), 0.0);
        let mut brain = Brain::new(&spawn);
        let mut pose = Pose { feet: find_free_spot(&level, spawn.pos), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut knight = fighter_for(ActorKind::Stratiotes);
        let player_pose = Pose { feet: find_free_spot(&level, Vec3::new(16.0, 0.0, 0.0)), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut player = fighter_for(ActorKind::Player);
        let mut rng = Rng::new(42);
        let dt = 1.0 / 60.0;
        for _ in 0..(60 * 20) {
            let seen = [Seen { id: 1, kind: ActorKind::Player, pose: player_pose, action: player.action, weapon: player.weapon, shield: false, alive: true, targetable: true }];
            think(&mut brain, ActorKind::Stratiotes, &mut pose, &mut knight, &seen, &level, &mut rng, dt);
            knight.tick(dt);
            player.tick(dt);
            if let Some((swing, p)) = knight.strike_progress() {
                let (h, t) = blade_segment(&pose, knight.weapon, swing, p);
                if !knight.hit_this_swing.contains(&1) {
                    if let Some(c) = trace_blade(h, t, &pose, &player, &player_pose) {
                        knight.hit_this_swing.push(1);
                        resolve_contact(&mut knight, &mut player, c, 15.0);
                    }
                }
            }
        }
        assert!(player.hp < player.max_hp, "рыцарь так и не попал; позиция {:?}", pose.feet);
    }

    /// Игрок просто машет мечом: мечник часто закрывается щитом, но в итоге погибает,
    /// а бой длится заметно дольше пары секунд.
    #[test]
    fn stratiotes_defends_with_shield() {
        let (npc, player, stats) = fight(ActorKind::Stratiotes, false, 120.0);
        eprintln!("урон по врагу {}, отражено {}, урон по игроку {}, hp игрока {}", stats[0], stats[1], stats[2], player.hp);
        assert!(stats[1] > 0, "мечник ни разу не закрылся щитом");
        assert!(npc.is_dead() || player.is_dead());
        assert!(stats[0] + stats[1] >= 6, "бой слишком короткий: {stats:?}");
    }

    /// Кочевник издали стреляет в стоящего игрока и попадает; щит в стойке стрелы держит.
    #[test]
    fn archer_shoots_from_distance() {
        use crate::combat::{Arrow, ArrowStep, arrow_launch, resolve_arrow, step_arrow};
        let level = crate::level::level_colliders();
        for shield in [false, true] {
            // Главная улица Херсонеса: открытая прямая вдоль X.
            let spawn = NpcSpawn::brigand(ActorKind::HunArcher, Vec3::new(4.0, 0.0, 0.0), -PI_ / 2.0);
            let mut brain = Brain::new(&spawn);
            let mut pose = Pose { feet: find_free_spot(&level, spawn.pos), yaw: 0.0, pitch: 0.0, crouching: false };
            let mut archer = fighter_for(ActorKind::HunArcher);
            let player_pose = Pose { feet: find_free_spot(&level, Vec3::new(16.0, 0.0, 0.0)), yaw: 0.0, pitch: 0.0, crouching: false };
            let mut player = Fighter::new(WeaponKind::Spatha, 150.0, shield);
            let mut rng = Rng::new(3);
            let dt = 1.0 / 60.0;
            let (mut arrows, mut shots, mut hits, mut blocked) = (Vec::<Arrow>::new(), 0, 0, 0);
            for _ in 0..(60 * 30) {
                // Игрок смотрит на лучника; со щитом держит стойку.
                let mut pp = player_pose;
                pp.yaw = yaw_towards(pp.feet, pose.feet);
                if shield {
                    player.set_guard(true);
                }
                let seen = [Seen { id: 1, kind: ActorKind::Player, pose: pp, action: player.action, weapon: player.weapon, shield, alive: true, targetable: true }];
                think(&mut brain, ActorKind::HunArcher, &mut pose, &mut archer, &seen, &level, &mut rng, dt);
                if archer.tick(dt) == Some(crate::combat::TickEvent::StrikeStarted) && archer.weapon.ranged() {
                    let (p, v) = arrow_launch(&pose);
                    arrows.push(Arrow { id: shots, owner: 2, pos: p, vel: v, damage: 30.0, life: 4.0, pierce: 0.0 });
                    shots += 1;
                }
                player.tick(dt);
                let targets = [(1, pp), (2, pose)];
                arrows.retain_mut(|a| match step_arrow(a, dt, &level, &targets) {
                    ArrowStep::Flying => true,
                    ArrowStep::Hit { head, shield_facing, .. } => {
                        match resolve_arrow(&mut player, shield_facing, head, 30.0) {
                            HitOutcome::Damage { .. } => hits += 1,
                            _ => blocked += 1,
                        }
                        false
                    }
                    _ => false,
                });
                if player.is_dead() {
                    break;
                }
            }
            eprintln!("щит {shield}: выстрелов {shots}, попаданий {hits}, в щит {blocked}");
            assert!(shots >= 3, "лучник почти не стреляет");
            if shield {
                assert!(blocked > 0 && hits == 0, "щит не держит стрелы");
            } else {
                assert!(hits > 0, "ни одна стрела не попала");
            }
        }
    }

    #[test]
    fn posts_get_era_people() {
        use crate::level::{NpcGroup, NpcPost};
        let post = |group, wander| NpcPost { pos: Vec3::ZERO, group, yaw: 0.0, wander };
        let posts = [post(NpcGroup::Garrison, 2.0), post(NpcGroup::Garrison, 15.0), post(NpcGroup::Goths, 12.0), post(NpcGroup::Alans, 12.0), post(NpcGroup::Nomads, 12.0), post(NpcGroup::Brigands, 20.0), post(NpcGroup::Brigands, 20.0)];
        let kinds: Vec<_> = spawns_from_posts(&posts).iter().map(|s| (s.kind, s.faction.is_some())).collect();
        assert_eq!(
            kinds,
            vec![
                (ActorKind::Stratiotes, true),
                (ActorKind::Stratiotes, true),
                (ActorKind::GothWarrior, true),
                (ActorKind::AlanLancer, true),
                (ActorKind::HunArcher, true),
                (ActorKind::GothWarrior, false),
                (ActorKind::HunArcher, false),
            ]
        );
        // Каждый тип есть на уровне Херсонеса.
        let all: Vec<_> = npc_spawns().iter().map(|s| s.kind).collect();
        for k in ActorKind::ENEMIES {
            assert!(all.contains(&k) || crate::level::npc_posts().is_empty(), "нет {k:?} на уровне");
        }
    }

    /// Стражник не трогает игрока, пока тот не нападёт или не станет врагом фракции.
    #[test]
    fn garrison_is_peaceful_until_provoked() {
        let level = crate::level::level_colliders();
        let mut spawn = NpcSpawn::brigand(ActorKind::Stratiotes, Vec3::new(10.0, 0.0, 0.0), 0.0);
        spawn.faction = Some(Faction::Garrison);
        let mut brain = Brain::new(&spawn);
        let mut pose = Pose { feet: find_free_spot(&level, spawn.pos), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut guard = fighter_for(ActorKind::Stratiotes);
        let player_pose = Pose { feet: find_free_spot(&level, Vec3::new(12.0, 0.0, 0.0)), yaw: 0.0, pitch: 0.0, crouching: false };
        let player = fighter_for(ActorKind::Player);
        let mut rng = Rng::new(5);
        let seen = [Seen { id: 1, kind: ActorKind::Player, pose: player_pose, action: player.action, weapon: player.weapon, shield: false, alive: true, targetable: true }];
        let mut attacked = |brain: &mut Brain, guard: &mut Fighter, pose: &mut Pose| {
            for _ in 0..(60 * 10) {
                think(brain, ActorKind::Stratiotes, pose, guard, &seen, &level, &mut rng, 1.0 / 60.0);
                guard.tick(1.0 / 60.0);
                if matches!(guard.action, Action::Windup { .. }) {
                    return true;
                }
            }
            false
        };
        assert!(!attacked(&mut brain, &mut guard, &mut pose), "стражник напал без причины");
        brain.provoke(1);
        assert!(attacked(&mut brain, &mut guard, &mut pose), "стражник не ответил на нападение");
        // Вражда по репутации тоже повод.
        let mut brain = Brain::new(&spawn);
        brain.hostile_players.push(1);
        let mut guard = fighter_for(ActorKind::Stratiotes);
        let mut pose = Pose { feet: find_free_spot(&level, spawn.pos), yaw: 0.0, pitch: 0.0, crouching: false };
        assert!(attacked(&mut brain, &mut guard, &mut pose));
    }

    /// Игрок держит стойку и отвечает после парирования: копейщик должен проиграть.
    #[test]
    fn parry_and_riposte_beats_lancer() {
        let (npc, player, stats) = fight(ActorKind::AlanLancer, true, 180.0);
        eprintln!("урон по врагу {}, отражено {}, урон по игроку {}, hp игрока {}", stats[0], stats[1], stats[2], player.hp);
        assert!(stats[0] > 0, "ни одного ответного удара");
        assert!(!player.is_dead(), "игрок в стойке проиграл: {stats:?}");
        let _ = npc;
    }
}
