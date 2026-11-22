//! Общая логика ближнего боя (используют и сервер, и клиент).
//!
//! Удар состоит из фаз: замах -> удар -> восстановление. Урон наносится только в фазе удара,
//! и только если клинок реально проходит через тело цели (проверяем отрезок клинка против капсулы).
//!
//! Парирование: короткое окно после нажатия. Если в это окно в вас попадает парируемый удар
//! спереди, атакующий оглушён, а вы сразу можете ответить. Промах парированием оставляет
//! вас открытым на короткое время.

use crate::collision::LevelColliders;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WeaponKind {
    /// Одноручный меч игрока и мечника.
    ArmingSword,
    /// Двуручный меч тяжёлого рыцаря.
    Greatsword,
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponStats {
    pub windup: f32,
    pub strike: f32,
    pub recovery: f32,
    pub damage: f32,
    /// Длина клинка от рукояти до острия.
    pub reach: f32,
    /// Каждый N-й удар нельзя парировать (0 — все удары парируемые).
    pub unparryable_every: u8,
}

impl WeaponKind {
    pub fn stats(self) -> WeaponStats {
        match self {
            WeaponKind::ArmingSword => WeaponStats { windup: 0.32, strike: 0.18, recovery: 0.32, damage: 25.0, reach: 1.0, unparryable_every: 0 },
            WeaponKind::Greatsword => WeaponStats { windup: 0.75, strike: 0.28, recovery: 0.65, damage: 45.0, reach: 1.5, unparryable_every: 3 },
        }
    }
}

pub const PARRY_WINDOW: f32 = 0.25;
pub const PARRY_RECOVERY: f32 = 0.4;
pub const STAGGER_TIME: f32 = 1.0;
pub const FLINCH_TIME: f32 = 0.3;
pub const BLOCK_RECOIL_TIME: f32 = 0.45;
/// Насколько больше урона получает оглушённый (окно для ответного удара).
pub const STAGGER_DAMAGE_MULT: f32 = 1.5;
/// Максимальный угол (от направления взгляда), под которым работают парирование и щит.
pub const DEFENSE_ARC_COS: f32 = 0.35; // ~70 градусов

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Swing {
    /// Справа налево.
    #[default]
    RightToLeft,
    /// Слева направо.
    LeftToRight,
    /// Сверху вниз.
    Overhead,
}

impl Swing {
    pub fn next(self) -> Swing {
        match self {
            Swing::RightToLeft => Swing::LeftToRight,
            Swing::LeftToRight => Swing::Overhead,
            Swing::Overhead => Swing::RightToLeft,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum Action {
    #[default]
    Idle,
    Windup { t: f32, swing: Swing, unparryable: bool },
    Strike { t: f32, swing: Swing, unparryable: bool },
    Recovery { t: f32, swing: Swing },
    Parry { t: f32 },
    ParryRecovery { t: f32 },
    /// Держит щит (только у мечника-рыцаря).
    Block,
    /// Отдача после удара в щит.
    BlockRecoil { t: f32 },
    /// Оглушён после парирования противником.
    Stagger { t: f32 },
    /// Короткая заминка от полученного удара.
    Flinch { t: f32 },
    Dead,
}

/// Боевое состояние одного бойца.
#[derive(Clone, Debug)]
pub struct Fighter {
    pub weapon: WeaponKind,
    pub action: Action,
    pub next_swing: Swing,
    pub swing_counter: u8,
    pub hp: f32,
    pub max_hp: f32,
    /// Есть ли щит (может блокировать).
    pub has_shield: bool,
    /// Попадание запомнено, чтобы удар бил каждую цель один раз.
    pub hit_this_swing: Vec<u64>,
    /// Атака, нажатая во время восстановления, выполнится сразу после него.
    pub queued_attack: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TickEvent {
    StrikeStarted,
    StrikeEnded,
}

impl Fighter {
    pub fn new(weapon: WeaponKind, max_hp: f32, has_shield: bool) -> Self {
        Self {
            weapon,
            action: Action::Idle,
            next_swing: Swing::RightToLeft,
            swing_counter: 0,
            hp: max_hp,
            max_hp,
            has_shield,
            hit_this_swing: Vec::new(),
            queued_attack: false,
        }
    }

    pub fn is_dead(&self) -> bool {
        matches!(self.action, Action::Dead)
    }

    /// Может ли боец начать новое действие прямо сейчас.
    pub fn is_free(&self) -> bool {
        matches!(self.action, Action::Idle | Action::Block)
    }

    /// Множитель скорости ходьбы в текущем действии.
    pub fn move_speed_mult(&self) -> f32 {
        match self.action {
            Action::Idle => 1.0,
            Action::Windup { .. } | Action::Strike { .. } | Action::Recovery { .. } => 0.55,
            Action::Parry { .. } | Action::ParryRecovery { .. } => 0.6,
            Action::Block | Action::BlockRecoil { .. } => 0.5,
            Action::Stagger { .. } => 0.25,
            Action::Flinch { .. } => 0.5,
            Action::Dead => 0.0,
        }
    }

    pub fn try_attack(&mut self) -> bool {
        if matches!(self.action, Action::Recovery { .. }) {
            self.queued_attack = true;
            return false;
        }
        if !self.is_free() {
            return false;
        }
        self.start_attack();
        true
    }

    fn start_attack(&mut self) {
        let stats = self.weapon.stats();
        self.swing_counter = self.swing_counter.wrapping_add(1);
        let unparryable = stats.unparryable_every > 0 && self.swing_counter % stats.unparryable_every == 0;
        let swing = if unparryable { Swing::Overhead } else { self.next_swing };
        self.next_swing = swing.next();
        self.queued_attack = false;
        self.action = Action::Windup { t: 0.0, swing, unparryable };
    }

    pub fn try_parry(&mut self) -> bool {
        // Парировать можно из покоя, из блока и прервав собственный замах (финт).
        if self.is_free() || matches!(self.action, Action::Windup { .. }) {
            self.action = Action::Parry { t: 0.0 };
            self.queued_attack = false;
            return true;
        }
        false
    }

    pub fn set_block(&mut self, on: bool) {
        if !self.has_shield {
            return;
        }
        match (on, self.action) {
            (true, Action::Idle) => self.action = Action::Block,
            (false, Action::Block) => self.action = Action::Idle,
            _ => {}
        }
    }

    /// Продвигает таймеры. Возвращает событие смены фазы удара, если было.
    pub fn tick(&mut self, dt: f32) -> Option<TickEvent> {
        let stats = self.weapon.stats();
        let mut event = None;
        self.action = match self.action {
            Action::Windup { t, swing, unparryable } => {
                let t = t + dt;
                if t >= stats.windup {
                    self.hit_this_swing.clear();
                    event = Some(TickEvent::StrikeStarted);
                    Action::Strike { t: t - stats.windup, swing, unparryable }
                } else {
                    Action::Windup { t, swing, unparryable }
                }
            }
            Action::Strike { t, swing, unparryable } => {
                let t = t + dt;
                if t >= stats.strike {
                    event = Some(TickEvent::StrikeEnded);
                    Action::Recovery { t: 0.0, swing }
                } else {
                    Action::Strike { t, swing, unparryable }
                }
            }
            Action::Recovery { t, swing } => {
                let t = t + dt;
                if t >= stats.recovery {
                    Action::Idle
                } else {
                    Action::Recovery { t, swing }
                }
            }
            Action::Parry { t } => {
                let t = t + dt;
                if t >= PARRY_WINDOW { Action::ParryRecovery { t: 0.0 } } else { Action::Parry { t } }
            }
            Action::ParryRecovery { t } => timed(t + dt, PARRY_RECOVERY, |t| Action::ParryRecovery { t }),
            Action::BlockRecoil { t } => timed(t + dt, BLOCK_RECOIL_TIME, |t| Action::BlockRecoil { t }),
            Action::Stagger { t } => timed(t + dt, STAGGER_TIME, |t| Action::Stagger { t }),
            Action::Flinch { t } => timed(t + dt, FLINCH_TIME, |t| Action::Flinch { t }),
            a @ (Action::Idle | Action::Block | Action::Dead) => a,
        };
        if self.action == Action::Idle && self.queued_attack {
            self.start_attack();
        }
        event
    }

    /// Нормализованный прогресс фазы удара 0..1 (для анимации и трассировки клинка).
    pub fn strike_progress(&self) -> Option<(Swing, f32)> {
        let stats = self.weapon.stats();
        match self.action {
            Action::Strike { t, swing, .. } => Some((swing, (t / stats.strike).clamp(0.0, 1.0))),
            _ => None,
        }
    }
}

fn timed(t: f32, limit: f32, make: impl Fn(f32) -> Action) -> Action {
    if t >= limit { Action::Idle } else { make(t) }
}

/// Поза бойца в мире, нужная для расчёта клинка.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub crouching: bool,
}

impl Pose {
    pub fn eye_height(&self) -> f32 {
        if self.crouching { 1.05 } else { 1.65 }
    }
    pub fn shoulder(&self) -> Vec3 {
        self.feet + Vec3::Y * (self.eye_height() - 0.25)
    }
    pub fn forward_flat(&self) -> Vec3 {
        Quat::from_rotation_y(self.yaw) * Vec3::NEG_Z
    }
}

/// Направление клинка в локальных координатах бойца для данного удара и прогресса 0..1.
/// -Z — вперёд, +X — вправо, +Y — вверх.
pub fn blade_local_dir(swing: Swing, progress: f32) -> Vec3 {
    let p = smooth(progress);
    match swing {
        Swing::RightToLeft => {
            let a = (75.0 - 150.0 * p).to_radians();
            Vec3::new(a.sin(), -0.1, -a.cos()).normalize()
        }
        Swing::LeftToRight => {
            let a = (-75.0 + 150.0 * p).to_radians();
            Vec3::new(a.sin(), 0.05, -a.cos()).normalize()
        }
        Swing::Overhead => {
            let a = (80.0 - 130.0 * p).to_radians();
            Vec3::new(0.08, a.sin(), -a.cos()).normalize()
        }
    }
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Точка рукояти в «пространстве прицела» во время удара.
pub const STRIKE_HILT: Vec3 = Vec3::new(0.0, -0.1, -0.35);
const REST_HILT: Vec3 = Vec3::new(0.38, -0.32, -0.5);

fn rest_dir() -> Vec3 {
    Vec3::new(0.3, 0.7, -0.65).normalize()
}

fn mix(a: (Vec3, Vec3), b: (Vec3, Vec3), k: f32) -> (Vec3, Vec3) {
    let k = smooth(k.clamp(0.0, 1.0));
    (a.0.lerp(b.0, k), a.1.lerp(b.1, k).normalize_or(Vec3::Y))
}

/// Положение рукояти и направление клинка в «пространстве прицела» (поворот = yaw, pitch*0.6,
/// начало — плечо) для любого действия. Нужно для анимации; в фазе удара совпадает с тем,
/// по чему считается попадание.
pub fn weapon_pose(action: &Action, weapon: WeaponKind, time: f32) -> (Vec3, Vec3) {
    let stats = weapon.stats();
    let rest = (REST_HILT, rest_dir());
    let windup_pose = |swing: Swing| {
        let start = blade_local_dir(swing, 0.0);
        let hilt = match swing {
            Swing::RightToLeft => Vec3::new(0.4, 0.05, -0.15),
            Swing::LeftToRight => Vec3::new(-0.35, 0.0, -0.15),
            Swing::Overhead => Vec3::new(0.1, 0.25, -0.05),
        };
        (hilt, (start + Vec3::Y * 0.6 + Vec3::Z * 0.4).normalize())
    };
    match *action {
        Action::Windup { t, swing, .. } => mix(rest, windup_pose(swing), t / stats.windup),
        Action::Strike { t, swing, .. } => (STRIKE_HILT, blade_local_dir(swing, (t / stats.strike).clamp(0.0, 1.0))),
        Action::Recovery { t, swing } => mix((STRIKE_HILT, blade_local_dir(swing, 1.0)), rest, t / stats.recovery),
        Action::Parry { t } => {
            let parry = (Vec3::new(0.0, 0.0, -0.45), Vec3::new(-0.85, 0.5, -0.15).normalize());
            mix(rest, parry, t / 0.07)
        }
        Action::ParryRecovery { t } => mix((Vec3::new(0.0, 0.0, -0.45), Vec3::new(-0.85, 0.5, -0.15).normalize()), rest, t / PARRY_RECOVERY),
        Action::Stagger { t } | Action::BlockRecoil { t } => {
            let wobble = (time * 30.0).sin() * 0.08 * (1.0 - t).max(0.0);
            (Vec3::new(0.4, 0.15 + wobble, -0.05), Vec3::new(0.6, 0.8, 0.35 + wobble).normalize())
        }
        Action::Flinch { t } => mix((REST_HILT + Vec3::new(0.05, -0.1, 0.1), rest_dir()), rest, t / FLINCH_TIME),
        Action::Block => (Vec3::new(0.35, -0.35, -0.2), Vec3::new(0.3, 0.6, -0.7).normalize()),
        Action::Idle | Action::Dead => rest,
    }
}

/// Отрезок клинка в мире: (рукоять, острие).
pub fn blade_segment(pose: &Pose, weapon: WeaponKind, swing: Swing, progress: f32) -> (Vec3, Vec3) {
    let stats = weapon.stats();
    // Наклон по вертикали частично следует за взглядом, чтобы можно было бить вниз/вверх.
    let rot = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_x(pose.pitch * 0.6);
    let dir = rot * blade_local_dir(swing, progress);
    let hilt = pose.shoulder() + rot * STRIKE_HILT;
    let tip = hilt + dir * stats.reach;
    (hilt, tip)
}

/// Капсула тела цели: вертикальный отрезок и радиус.
pub fn body_capsule(pose: &Pose) -> (Vec3, Vec3, f32) {
    let top = if pose.crouching { 1.05 } else { 1.6 };
    (pose.feet + Vec3::Y * 0.3, pose.feet + Vec3::Y * top, 0.38)
}

/// Квадрат минимального расстояния между отрезками [p1,q1] и [p2,q2].
pub fn segment_distance_sq(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.length_squared();
    let e = d2.length_squared();
    let f = d2.dot(r);
    let (s, t);
    if a <= 1e-8 && e <= 1e-8 {
        return r.length_squared();
    }
    if a <= 1e-8 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 1e-8 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s0 = if denom != 0.0 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    let c1 = p1 + d1 * s;
    let c2 = p2 + d2 * t;
    (c1 - c2).length_squared()
}

/// Проверяет, задевает ли клинок тело. Возвращает точку попадания.
pub fn blade_hits_body(hilt: Vec3, tip: Vec3, target: &Pose) -> Option<Vec3> {
    let (a, b, r) = body_capsule(target);
    // Проверяем несколько точек вдоль клинка, чтобы найти точку контакта.
    if segment_distance_sq(hilt, tip, a, b) > r * r {
        return None;
    }
    let mut best = tip;
    let mut best_d = f32::MAX;
    for i in 0..=8 {
        let p = hilt.lerp(tip, i as f32 / 8.0);
        let d = segment_distance_sq(p, p, a, b);
        if d < best_d {
            best_d = d;
            best = p;
        }
    }
    Some(best)
}

/// Есть ли прямая видимость (без стен) между точками.
pub fn line_of_sight(level: &LevelColliders, from: Vec3, to: Vec3) -> bool {
    let d = to - from;
    let len = d.length();
    let steps = (len / 0.15).ceil().max(1.0) as usize;
    for i in 1..steps {
        let p = from + d * (i as f32 / steps as f32);
        if level.0.iter().any(|c| p.cmpgt(c.min).all() && p.cmplt(c.max).all()) {
            return false;
        }
    }
    true
}

/// Защищающийся смотрит в сторону атакующего?
pub fn facing(defender: &Pose, attacker_pos: Vec3) -> bool {
    let to = attacker_pos - defender.feet;
    let to = Vec3::new(to.x, 0.0, to.z).normalize_or_zero();
    defender.forward_flat().dot(to) >= DEFENSE_ARC_COS
}

/// Чем закончилось попадание клинка в цель.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum HitOutcome {
    Damage { amount: f32, killed: bool },
    Parried,
    Blocked,
}

/// Применяет попадание атакующего по защищающемуся. Меняет состояния обоих.
pub fn resolve_hit(attacker: &mut Fighter, attacker_pose: &Pose, defender: &mut Fighter, defender_pose: &Pose, damage_mult: f32) -> HitOutcome {
    let unparryable = matches!(attacker.action, Action::Strike { unparryable: true, .. });
    let front = facing(defender_pose, attacker_pose.feet);
    match defender.action {
        Action::Parry { .. } if front && !unparryable => {
            attacker.action = Action::Stagger { t: 0.0 };
            attacker.queued_attack = false;
            defender.action = Action::Idle; // сразу можно ответить
            return HitOutcome::Parried;
        }
        Action::Block if front && !unparryable => {
            attacker.action = Action::BlockRecoil { t: 0.0 };
            attacker.queued_attack = false;
            return HitOutcome::Blocked;
        }
        _ => {}
    }
    let mut amount = attacker.weapon.stats().damage * damage_mult;
    if matches!(defender.action, Action::Stagger { .. }) {
        amount *= STAGGER_DAMAGE_MULT;
    }
    defender.hp = (defender.hp - amount).max(0.0);
    let killed = defender.hp <= 0.0;
    if killed {
        defender.action = Action::Dead;
    } else if matches!(defender.action, Action::Windup { .. } | Action::Idle | Action::Block | Action::ParryRecovery { .. } | Action::Recovery { .. }) {
        // Попадание сбивает замах (кто ударил первым — тот и прав).
        defender.action = Action::Flinch { t: 0.0 };
        defender.queued_attack = false;
    }
    HitOutcome::Damage { amount, killed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(x: f32, z: f32, yaw: f32) -> Pose {
        Pose { feet: Vec3::new(x, 0.0, z), yaw, pitch: 0.0, crouching: false }
    }

    /// Прогоняет удар атакующего и возвращает первое попадание.
    fn swing_until_hit(att: &mut Fighter, att_pose: &Pose, def: &mut Fighter, def_pose: &Pose) -> Option<HitOutcome> {
        att.try_attack();
        for _ in 0..200 {
            let before = att.strike_progress();
            att.tick(1.0 / 60.0);
            def.tick(1.0 / 60.0);
            if let (Some((swing, p)), _) | (None, Some((swing, p))) = (att.strike_progress(), before) {
                let (h, t) = blade_segment(att_pose, att.weapon, swing, p);
                if blade_hits_body(h, t, def_pose).is_some() {
                    return Some(resolve_hit(att, att_pose, def, def_pose, 1.0));
                }
            }
        }
        None
    }

    #[test]
    fn sword_hits_target_in_front() {
        let mut a = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        let mut d = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        // Атакующий смотрит на -Z, цель в 1.2 м перед ним и смотрит на него (+Z => yaw = PI).
        let out = swing_until_hit(&mut a, &pose(0.0, 0.0, 0.0), &mut d, &pose(0.0, -1.2, std::f32::consts::PI));
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
        assert!(d.hp < 100.0);
    }

    #[test]
    fn sword_misses_target_out_of_reach() {
        let mut a = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        let mut d = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        let out = swing_until_hit(&mut a, &pose(0.0, 0.0, 0.0), &mut d, &pose(0.0, -3.5, std::f32::consts::PI));
        assert!(out.is_none());
    }

    #[test]
    fn parry_staggers_attacker() {
        let mut a = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        let mut d = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        a.try_attack();
        // Защитник жмёт парирование незадолго до удара.
        let mut parried = None;
        for i in 0..120 {
            if i == 14 {
                d.try_parry();
            }
            a.tick(1.0 / 60.0);
            d.tick(1.0 / 60.0);
            if let Some((swing, p)) = a.strike_progress() {
                let (h, t) = blade_segment(&pose(0.0, 0.0, 0.0), a.weapon, swing, p);
                if blade_hits_body(h, t, &pose(0.0, -1.2, std::f32::consts::PI)).is_some() {
                    parried = Some(resolve_hit(&mut a, &pose(0.0, 0.0, 0.0), &mut d, &pose(0.0, -1.2, std::f32::consts::PI), 1.0));
                    break;
                }
            }
        }
        assert_eq!(parried, Some(HitOutcome::Parried));
        assert!(matches!(a.action, Action::Stagger { .. }));
        assert_eq!(d.hp, 100.0);
    }

    #[test]
    fn parry_from_behind_fails() {
        let mut a = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        let mut d = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        a.action = Action::Strike { t: 0.05, swing: Swing::RightToLeft, unparryable: false };
        d.action = Action::Parry { t: 0.05 };
        // Защитник смотрит в ту же сторону, что и атакующий (спиной к нему).
        let out = resolve_hit(&mut a, &pose(0.0, 0.0, 0.0), &mut d, &pose(0.0, -1.2, 0.0), 1.0);
        assert!(matches!(out, HitOutcome::Damage { .. }));
    }

    #[test]
    fn unparryable_ignores_parry() {
        let mut a = Fighter::new(WeaponKind::Greatsword, 100.0, false);
        let mut d = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        a.action = Action::Strike { t: 0.05, swing: Swing::Overhead, unparryable: true };
        d.action = Action::Parry { t: 0.05 };
        let out = resolve_hit(&mut a, &pose(0.0, 0.0, 0.0), &mut d, &pose(0.0, -1.2, std::f32::consts::PI), 1.0);
        assert!(matches!(out, HitOutcome::Damage { .. }));
    }

    #[test]
    fn greatsword_every_third_swing_unparryable() {
        let mut f = Fighter::new(WeaponKind::Greatsword, 100.0, false);
        let mut flags = Vec::new();
        for _ in 0..6 {
            f.action = Action::Idle;
            f.try_attack();
            if let Action::Windup { unparryable, .. } = f.action {
                flags.push(unparryable);
            }
        }
        assert_eq!(flags, vec![false, false, true, false, false, true]);
    }

    #[test]
    fn whiffed_parry_leaves_window() {
        let mut d = Fighter::new(WeaponKind::ArmingSword, 100.0, false);
        d.try_parry();
        for _ in 0..20 {
            d.tick(1.0 / 60.0);
        }
        assert!(matches!(d.action, Action::ParryRecovery { .. }));
        assert!(!d.try_attack());
    }
}
