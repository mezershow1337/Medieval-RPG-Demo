//! Искусственный интеллект рыцарей-врагов (работает только на сервере).

use crate::collision::{Body, LevelColliders};
use crate::combat::{Action, Fighter, Pose, WeaponKind, line_of_sight};
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
    use std::f32::consts::PI;
    vec![
        // Тренировочная площадка во дворе.
        NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(-15.0, 0.0, 0.0), yaw: -PI / 2.0 },
        NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(-12.0, 0.0, 5.0), yaw: -PI / 2.0 },
        // Двор у конюшни.
        NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(14.0, 0.0, -3.0), yaw: PI / 2.0 },
        // Тронный зал: два стражника и тяжёлый рыцарь у трона.
        NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(-3.0, 0.0, -14.0), yaw: PI },
        NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(3.0, 0.0, -14.0), yaw: PI },
        NpcSpawn { kind: ActorKind::HeavyKnight, pos: Vec3::new(0.0, 0.0, -21.0), yaw: PI },
    ]
}

pub fn fighter_for(kind: ActorKind) -> Fighter {
    match kind {
        ActorKind::Player => Fighter::new(WeaponKind::ArmingSword, 100.0, false),
        ActorKind::Swordsman => Fighter::new(WeaponKind::ArmingSword, 90.0, true),
        ActorKind::HeavyKnight => Fighter::new(WeaponKind::Greatsword, 160.0, false),
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
    pub weapon: WeaponKind,
    pub alive: bool,
    /// Можно ли нападать (после возрождения игрок пару секунд под защитой).
    pub targetable: bool,
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
    pending_parry: bool,
    pending_block: bool,
    holding_block: bool,
}

impl Brain {
    pub fn new(spawn: &NpcSpawn) -> Self {
        Self { home: spawn.pos, home_yaw: spawn.yaw, attack_cooldown: 1.0, ..default() }
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
    let speed = match kind {
        ActorKind::HeavyKnight => 2.7,
        _ => 3.3,
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

    let can_act = !matches!(fighter.action, Action::Stagger { .. } | Action::Flinch { .. } | Action::BlockRecoil { .. } | Action::Dead);
    let mut wish = Vec3::ZERO;

    if let Some(t) = target {
        let to = t.pose.feet - pose.feet;
        let dist = Vec2::new(to.x, to.z).length();
        let desired_yaw = yaw_towards(pose.feet, t.pose.feet);
        let turn_rate = match fighter.action {
            Action::Strike { .. } => 1.2,
            Action::Windup { .. } => 4.0,
            Action::Stagger { .. } => 0.0,
            _ => 7.0,
        };
        pose.yaw = turn_towards(pose.yaw, desired_yaw, turn_rate * dt);
        let forward = pose.forward_flat();
        let right = Vec3::new(-forward.z, 0.0, forward.x);
        let aligned = forward.dot(Vec3::new(to.x, 0.0, to.z).normalize_or_zero()) > 0.9;

        if dist > range {
            wish += forward;
        } else if dist < range - 0.7 {
            wish -= forward * 0.6;
        }
        if dist < range + 1.5 {
            if brain.strafe_timer <= 0.0 {
                brain.strafe = if rng.chance(0.3) { 0.0 } else if rng.chance(0.5) { 0.45 } else { -0.45 };
                brain.strafe_timer = rng.range(1.2, 2.8);
            }
            wish += right * brain.strafe;
        }

        // --- Защита: реагируем на начало вражеского замаха ---
        let winding = matches!(t.action, Action::Windup { .. });
        let unparryable = matches!(t.action, Action::Windup { unparryable: true, .. } | Action::Strike { unparryable: true, .. });
        if winding && !brain.target_was_winding_up && dist < range + 1.6 && can_act && !unparryable {
            let (block_p, parry_p) = match kind {
                ActorKind::Swordsman => (0.5, 0.18),
                ActorKind::HeavyKnight => (0.0, 0.25),
                ActorKind::Player => (0.0, 0.0),
            };
            let roll = rng.next_f32();
            if roll < block_p && fighter.has_shield {
                brain.pending_block = true;
            } else if roll < block_p + parry_p {
                brain.pending_parry = true;
            }
        }
        brain.target_was_winding_up = winding;

        if brain.pending_parry {
            if let Action::Windup { t: wt, .. } = t.action {
                if wt >= t.weapon.stats().windup - 0.12 {
                    fighter.try_parry();
                    brain.pending_parry = false;
                }
            } else if !matches!(t.action, Action::Strike { .. }) {
                brain.pending_parry = false;
            }
        }

        // Щит поднимаем, как только освободимся (если ещё успеваем).
        if brain.pending_block {
            if !matches!(t.action, Action::Windup { .. } | Action::Strike { .. }) {
                brain.pending_block = false;
            } else if matches!(fighter.action, Action::Idle) {
                fighter.set_block(true);
                brain.holding_block = true;
                brain.pending_block = false;
            }
        }

        if brain.holding_block && !matches!(t.action, Action::Windup { .. } | Action::Strike { .. }) {
            fighter.set_block(false);
            brain.holding_block = false;
        }

        // --- Атака ---
        let punish = matches!(t.action, Action::Stagger { .. } | Action::ParryRecovery { .. });
        let in_reach = dist <= range + 0.15;
        let defending = brain.pending_parry || brain.pending_block || (brain.holding_block && !punish);
        if can_act && in_reach && aligned && !defending && (brain.attack_cooldown <= 0.0 || punish) {
            if brain.holding_block {
                fighter.set_block(false);
                brain.holding_block = false;
            }
            if fighter.try_attack() {
                brain.attack_cooldown = match kind {
                    ActorKind::HeavyKnight => rng.range(1.4, 2.6),
                    _ => rng.range(0.9, 1.9),
                };
            }
        }
    } else {
        brain.pending_parry = false;
        brain.pending_block = false;
        if brain.holding_block {
            fighter.set_block(false);
            brain.holding_block = false;
        }
        let to_home = brain.home - pose.feet;
        if Vec2::new(to_home.x, to_home.z).length() > 1.0 {
            pose.yaw = turn_towards(pose.yaw, yaw_towards(pose.feet, brain.home), 5.0 * dt);
            wish = pose.forward_flat() * 0.6;
        } else {
            pose.yaw = turn_towards(pose.yaw, brain.home_yaw, 2.0 * dt);
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
    use crate::combat::{HitOutcome, blade_hits_body, blade_segment, resolve_hit};

    /// Рыцарь подходит к стоящему игроку и попадает по нему.
    #[test]
    fn knight_approaches_and_hits_player() {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(0.0, 0.0, 10.0), yaw: 0.0 };
        let mut brain = Brain::new(&spawn);
        let mut pose = Pose { feet: Vec3::new(0.0, 0.0, 10.0), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut knight = fighter_for(ActorKind::Swordsman);
        let player_pose = Pose { feet: Vec3::new(0.0, 0.0, 16.0), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut player = fighter_for(ActorKind::Player);
        let mut rng = Rng::new(42);
        let dt = 1.0 / 60.0;
        for _ in 0..(60 * 15) {
            let seen = [Seen { id: 1, kind: ActorKind::Player, pose: player_pose, action: player.action, weapon: player.weapon, alive: true, targetable: true }];
            think(&mut brain, ActorKind::Swordsman, &mut pose, &mut knight, &seen, &level, &mut rng, dt);
            knight.tick(dt);
            player.tick(dt);
            if let Some((swing, p)) = knight.strike_progress() {
                let (h, t) = blade_segment(&pose, knight.weapon, swing, p);
                if !knight.hit_this_swing.contains(&1) && blade_hits_body(h, t, &player_pose).is_some() {
                    knight.hit_this_swing.push(1);
                    let out = resolve_hit(&mut knight, &pose, &mut player, &player_pose, 1.0);
                    assert!(matches!(out, HitOutcome::Damage { .. }));
                }
            }
        }
        assert!(player.hp < 100.0, "рыцарь так и не попал; позиция {:?}", pose.feet);
    }

    /// Игрок раз за разом бьёт мечника: тот иногда блокирует или парирует, но в итоге погибает.
    #[test]
    fn player_fights_swordsman() {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn { kind: ActorKind::Swordsman, pos: Vec3::new(0.0, 0.0, 10.0), yaw: 0.0 };
        let mut brain = Brain::new(&spawn);
        let mut npc_pose = Pose { feet: Vec3::new(0.0, 0.0, 10.0), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut knight = fighter_for(ActorKind::Swordsman);
        let mut player = fighter_for(ActorKind::Player);
        player.hp = 100_000.0; // игрок бессмертен в этом тесте
        player.max_hp = 100_000.0;
        let mut player_pose = Pose { feet: Vec3::new(0.0, 0.0, 11.5), yaw: 0.0, pitch: 0.0, crouching: false };
        let mut rng = Rng::new(7);
        let dt = 1.0 / 60.0;
        let (mut damage, mut blocked, mut parried) = (0, 0, 0);
        for _ in 0..(60 * 120) {
            // Игрок всегда смотрит на рыцаря и жмёт атаку.
            player_pose.yaw = yaw_towards(player_pose.feet, npc_pose.feet);
            player.try_attack();
            let seen = [Seen { id: 1, kind: ActorKind::Player, pose: player_pose, action: player.action, weapon: player.weapon, alive: true, targetable: true }];
            think(&mut brain, ActorKind::Swordsman, &mut npc_pose, &mut knight, &seen, &level, &mut rng, dt);
            knight.tick(dt);
            player.tick(dt);
            // Удар игрока по рыцарю.
            if let Some((swing, p)) = player.strike_progress() {
                let (h, t) = blade_segment(&player_pose, player.weapon, swing, p);
                if !player.hit_this_swing.contains(&2) && blade_hits_body(h, t, &npc_pose).is_some() {
                    player.hit_this_swing.push(2);
                    match resolve_hit(&mut player, &player_pose, &mut knight, &npc_pose, 1.0) {
                        HitOutcome::Damage { .. } => damage += 1,
                        HitOutcome::Blocked => blocked += 1,
                        HitOutcome::Parried => parried += 1,
                    }
                }
            }
            // Удар рыцаря по игроку.
            if let Some((swing, p)) = knight.strike_progress() {
                let (h, t) = blade_segment(&npc_pose, knight.weapon, swing, p);
                if !knight.hit_this_swing.contains(&1) && blade_hits_body(h, t, &player_pose).is_some() {
                    knight.hit_this_swing.push(1);
                    resolve_hit(&mut knight, &npc_pose, &mut player, &player_pose, 1.0);
                }
            }
            if knight.is_dead() {
                break;
            }
        }
        eprintln!("урон {damage}, блоков {blocked}, парирований {parried}, hp игрока потеряно {}", 100_000.0 - player.hp);
        assert!(knight.is_dead(), "рыцарь выжил: урон {damage}, блоков {blocked}, парирований {parried}");
        assert!(blocked + parried > 0, "рыцарь ни разу не защитился: урон {damage}");
    }
}

/// Множитель урона для разных бойцов (баланс).
pub fn damage_mult(kind: ActorKind) -> f32 {
    match kind {
        ActorKind::Player => 1.0,
        ActorKind::Swordsman => 0.8,
        ActorKind::HeavyKnight => 1.0,
    }
}
