//! Искусственный интеллект рыцарей-врагов (работает только на сервере).

use crate::collision::{Body, LevelColliders};
use crate::combat::{self, Action, Fighter, Pose, WeaponKind, line_of_sight};
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
}

/// Где стоят враги в начале и куда возрождаются.
pub fn npc_spawns() -> Vec<NpcSpawn> {
    // Места берём из уровня: пять мечников (двор, конюшня, неф собора) и тяжёлый рыцарь перед хорами.
    // Все смотрят в сторону точки появления игроков.
    let hints = crate::level::npc_spawn_hints();
    let last = hints.len().saturating_sub(1);
    hints
        .into_iter()
        .enumerate()
        .map(|(i, pos)| {
            let kind = if i == last { ActorKind::HeavyKnight } else { ActorKind::Swordsman };
            let (dx, dz) = (0.0 - pos.x, 32.0 - pos.z);
            NpcSpawn { kind, pos, yaw: (-dx).atan2(-dz) }
        })
        .collect()
}

pub fn fighter_for(kind: ActorKind) -> Fighter {
    match kind {
        ActorKind::Player => Fighter::new(WeaponKind::ArmingSword, crate::server::PLAYER_BASE_HP, false),
        ActorKind::Swordsman => Fighter::new(WeaponKind::ArmingSword, 140.0, true),
        ActorKind::HeavyKnight => Fighter::new(WeaponKind::Greatsword, 220.0, false),
    }
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
    #[allow(dead_code)] // пригодится для выбора тактики против разного оружия
    pub weapon: WeaponKind,
    pub alive: bool,
    /// Можно ли нападать (после возрождения игрок пару секунд под защитой).
    pub targetable: bool,
}


/// Множитель урона для разных бойцов (баланс).
pub fn damage_mult(kind: ActorKind) -> f32 {
    match kind {
        ActorKind::Player => 1.0,
        ActorKind::Swordsman => 0.8,
        ActorKind::HeavyKnight => 1.0,
    }
}

/// Броня врагов (снижает получаемый урон, см. `combat::armor_reduction`).
pub fn armor(kind: ActorKind) -> f32 {
    match kind {
        ActorKind::Player => 0.0,
        ActorKind::Swordsman => 40.0,
        ActorKind::HeavyKnight => 80.0,
    }
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
}

impl Brain {
    pub fn new(spawn: &NpcSpawn) -> Self {
        Self { home: spawn.pos, home_yaw: spawn.yaw, attack_cooldown: 1.5, ..default() }
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
    let stats = fighter.weapon.stats();
    let range = stats.reach + 0.45;
    // Враги заметно медленнее игрока (4 м/с), от них можно отойти.
    let speed = match kind {
        ActorKind::HeavyKnight => 1.9,
        _ => 2.3,
    };

    // --- Выбор цели ---
    let eye = pose.shoulder();
    let current = brain.target.and_then(|id| others.iter().find(|o| o.id == id && o.alive));
    let target = match current {
        Some(t) if t.pose.feet.distance(pose.feet) < LOSE_RANGE => Some(*t),
        _ => others
            .iter()
            .filter(|o| o.alive && o.targetable && o.kind == ActorKind::Player)
            .filter(|o| o.pose.feet.distance(pose.feet) < AGGRO_RANGE)
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

        if dist > range {
            wish += forward;
        } else if dist < range - 0.7 {
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
        if winding && !brain.target_was_winding_up && dist < range + 2.0 {
            let p = match kind {
                ActorKind::Swordsman => 0.6,  // щит
                ActorKind::HeavyKnight => 0.3, // парирование двуручником
                ActorKind::Player => 0.0,
            };
            brain.want_guard = rng.chance(p);
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
        let blocked_by_guard = target_guarding && brain.patience < 2.5;
        if can_act && in_reach && aligned && !brain.want_guard && !blocked_by_guard && (brain.attack_cooldown <= 0.0 || punish) {
            if fighter.try_attack() {
                brain.attack_cooldown = match kind {
                    ActorKind::HeavyKnight => rng.range(2.4, 4.0),
                    _ => rng.range(1.8, 3.2),
                };
                brain.patience = 0.0;
            }
        }
    } else {
        brain.want_guard = false;
        if fighter.guard_held {
            fighter.set_guard(false);
        }
        let to_home = brain.home - pose.feet;
        if Vec2::new(to_home.x, to_home.z).length() > 1.0 {
            pose.yaw = turn_towards(pose.yaw, yaw_towards(pose.feet, brain.home), 3.0 * dt);
            wish = pose.forward_flat() * 0.6;
        } else {
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
    use crate::combat::{Contact, HitOutcome, blade_segment, resolve_contact, trace_blade};

    /// Прогоняет поединок игрока со стратегией `player_brain` против врага. Возвращает статистику.
    fn fight(kind: ActorKind, player_guard: bool, seconds: f32) -> (Fighter, Fighter, [u32; 3]) {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn { kind, pos: Vec3::new(10.0, 0.0, 0.0), yaw: 0.0 };
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
            let seen = [Seen { id: 1, kind: ActorKind::Player, pose: player_pose, action: player.action, weapon: player.weapon, alive: true, targetable: true }];
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
        let spawn = NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(10.0, 0.0, 0.0), yaw: 0.0 };
        let mut brain = Brain::new(&spawn);
        let mut pose = Pose { feet: find_free_spot(&level, spawn.pos), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut knight = fighter_for(ActorKind::Swordsman);
        let player_pose = Pose { feet: find_free_spot(&level, Vec3::new(10.0, 0.0, 6.0)), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut player = fighter_for(ActorKind::Player);
        let mut rng = Rng::new(42);
        let dt = 1.0 / 60.0;
        for _ in 0..(60 * 20) {
            let seen = [Seen { id: 1, kind: ActorKind::Player, pose: player_pose, action: player.action, weapon: player.weapon, alive: true, targetable: true }];
            think(&mut brain, ActorKind::Swordsman, &mut pose, &mut knight, &seen, &level, &mut rng, dt);
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
    fn swordsman_defends_with_shield() {
        let (npc, player, stats) = fight(ActorKind::Swordsman, false, 120.0);
        eprintln!("урон по врагу {}, отражено {}, урон по игроку {}, hp игрока {}", stats[0], stats[1], stats[2], player.hp);
        assert!(stats[1] > 0, "мечник ни разу не закрылся щитом");
        assert!(npc.is_dead() || player.is_dead());
        assert!(stats[0] + stats[1] >= 6, "бой слишком короткий: {stats:?}");
    }

    /// Игрок держит стойку и отвечает после парирования: тяжёлый рыцарь должен проиграть.
    #[test]
    fn parry_and_riposte_beats_heavy_knight() {
        let (npc, player, stats) = fight(ActorKind::HeavyKnight, true, 180.0);
        eprintln!("урон по врагу {}, отражено {}, урон по игроку {}, hp игрока {}", stats[0], stats[1], stats[2], player.hp);
        assert!(stats[0] > 0, "ни одного ответного удара");
        assert!(!player.is_dead(), "игрок в стойке проиграл: {stats:?}");
        let _ = npc;
    }
}
