//! Общая логика ближнего боя (используют и сервер, и клиент).
//!
//! Устроено по мотивам Dark and Darker:
//! * Удар: замах -> удар -> восстановление. Урон наносится только там, где реально проходит
//!   клинок. Удары идут серией (справа, слева, сверху); пауза или отскок сбрасывают серию.
//! * Защита — это **стойка** (зажать ПКМ), а не короткое окно. В стойке оружие (или щит)
//!   становится «защитной зоной»: если вражеский клинок попадает в неё, удар отражён.
//!   Защита направленная: удар сбоку или со спины проходит в тело.
//! * Оружием — **парирование**: атакующий отбрасывается и теряет серию, защитник на мгновение
//!   «вздрагивает», и пока длится это вздрагивание, можно нанести быстрый **ответный удар**.
//! * Щитом — **блок**: урона нет, но защитника отбрасывает тем сильнее, чем больше
//!   «сила удара» оружия по сравнению с «устойчивостью» щита. Тяжёлый удар серию не прерывает.
//! * Клинок, задевший стену, отскакивает: короткое оглушение и сброс серии.
//! * Выносливость: удары, бег и принятые на защиту удары тратят силы. Когда силы кончаются
//!   в стойке, защиту пробивает.
//! * Оружие — эпохи Херсонеса 510 г.: спата, топор, сакс, копьё-контос и составной лук.

use crate::collision::LevelColliders;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WeaponKind {
    /// Спата: прямой длинный меч поздней Римской империи (клинок ~80 см). Оружие гарнизона и игрока.
    Spatha,
    /// Бородовидный топор: короткий, сильный удар, слабо парирует. Оружие готов.
    Axe,
    /// Сакс: длинный боевой нож. Быстрый, но короткий и слабый.
    Seax,
    /// Контос: тяжёлое копьё двумя руками (~2,6 м), только колющие удары. Оружие аланов.
    Spear,
    /// Составной лук степняков: удар — это натяжение и выстрел.
    Bow,
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponStats {
    pub windup: f32,
    pub strike: f32,
    pub recovery: f32,
    /// Базовый урон (у игрока заменяется уроном надетого оружия).
    pub damage: f32,
    /// Длина клинка от рукояти до острия.
    pub reach: f32,
    /// Сила удара: насколько сильно отбрасывает того, кто принял удар на щит.
    pub impact_power: i32,
    /// Устойчивость оружия в стойке парирования.
    pub guard_resist: i32,
    /// Сколько выносливости стоит удар.
    pub stamina_cost: f32,
}

impl WeaponKind {
    pub fn stats(self) -> WeaponStats {
        match self {
            WeaponKind::Spatha => WeaponStats { windup: 0.45, strike: 0.22, recovery: 0.5, damage: 22.0, reach: 0.95, impact_power: 3, guard_resist: 3, stamina_cost: 12.0 },
            WeaponKind::Axe => WeaponStats { windup: 0.5, strike: 0.2, recovery: 0.55, damage: 26.0, reach: 0.75, impact_power: 5, guard_resist: 2, stamina_cost: 15.0 },
            WeaponKind::Seax => WeaponStats { windup: 0.3, strike: 0.15, recovery: 0.35, damage: 13.0, reach: 0.5, impact_power: 1, guard_resist: 1, stamina_cost: 7.0 },
            WeaponKind::Spear => WeaponStats { windup: 0.55, strike: 0.2, recovery: 0.6, damage: 24.0, reach: 2.1, impact_power: 4, guard_resist: 3, stamina_cost: 14.0 },
            // У лука «замах» — натяжение тетивы, «удар» — выстрел.
            WeaponKind::Bow => WeaponStats { windup: 1.1, strike: 0.05, recovery: 0.7, damage: 30.0, reach: 0.0, impact_power: 2, guard_resist: 0, stamina_cost: 10.0 },
        }
    }
    /// Бьёт только уколом (копьё).
    pub fn thrust_only(self) -> bool {
        matches!(self, WeaponKind::Spear)
    }
    /// Стреляет, а не рубит.
    pub fn ranged(self) -> bool {
        matches!(self, WeaponKind::Bow)
    }
    /// Можно ли этим оружием держать защиту.
    pub fn can_guard(self) -> bool {
        !self.ranged()
    }

    /// Какое оружие соответствует предмету (по названию предмета).
    pub fn from_item_name(name: &str) -> WeaponKind {
        let n = name.to_lowercase();
        let has = |w: &[&str]| w.iter().any(|k| n.contains(k));
        if has(&["топор", "секира"]) {
            WeaponKind::Axe
        } else if has(&["кинжал", "нож", "сакс"]) {
            WeaponKind::Seax
        } else if has(&["копь", "контос", "алебард", "пика", "дротик"]) {
            WeaponKind::Spear
        } else if has(&["лук"]) {
            WeaponKind::Bow
        } else {
            WeaponKind::Spatha
        }
    }
}

/// Полёт стрелы: скорость и гравитация.
pub const ARROW_SPEED: f32 = 42.0;
pub const ARROW_GRAVITY: f32 = 9.8;

/// Выносливость.
pub const STAMINA_MAX: f32 = 100.0;
pub const STAMINA_REGEN: f32 = 22.0;
/// Пауза после траты, прежде чем силы начнут восстанавливаться.
pub const STAMINA_REGEN_DELAY: f32 = 0.9;
pub const SPRINT_DRAIN: f32 = 12.0;
/// Оглушение, когда силы кончились в стойке и защиту пробило.
pub const GUARD_BREAK_STAGGER: f32 = 1.3;

/// Устойчивость щита.
pub const SHIELD_RESIST: i32 = 4;
/// Сколько поднимается стойка (до этого защиты ещё нет).
pub const GUARD_RAISE_WEAPON: f32 = 0.15;
pub const GUARD_RAISE_SHIELD: f32 = 0.2;
/// Замах ответного удара после парирования.
pub const RIPOSTE_WINDUP: f32 = 0.12;
pub const RIPOSTE_DAMAGE_MULT: f32 = 1.3;
/// Сколько после «вздрагивания» ещё можно начать ответный удар.
pub const RIPOSTE_GRACE: f32 = 0.25;
/// Атакующий, которого спарировали, оглушён дольше, чем вздрагивает защитник.
pub const PARRIED_EXTRA_STAGGER: f32 = 0.55;
/// Отскок от щита (если удар не тяжёлый) и от стены.
pub const BLOCK_BOUNCE: f32 = 0.45;
pub const WALL_BOUNCE: f32 = 0.5;
pub const FLINCH_TIME: f32 = 0.3;
/// Пауза после удара, после которой серия начинается заново.
pub const COMBO_RESET: f32 = 0.7;
pub const HEAD_MULT: f32 = 1.5;
/// Защита работает в секторе примерно ±75 градусов от взгляда.
pub const DEFENSE_ARC_COS: f32 = 0.26;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImpactTier {
    Weak,
    Medium,
    Heavy,
}

impl ImpactTier {
    pub fn from_diff(power: i32, resist: i32) -> Self {
        match power - resist {
            d if d > 0 => ImpactTier::Heavy,
            0 => ImpactTier::Medium,
            _ => ImpactTier::Weak,
        }
    }
    /// Сколько вздрагивает защитник, принявший удар на щит.
    pub fn shield_stagger(self) -> f32 {
        match self {
            ImpactTier::Weak => 0.5,
            ImpactTier::Medium => 0.9,
            ImpactTier::Heavy => 1.5,
        }
    }
    /// Сколько выносливости теряет защитник, принявший удар.
    pub fn guard_stamina(self, shield: bool) -> f32 {
        let base = match self {
            ImpactTier::Weak => 6.0,
            ImpactTier::Medium => 11.0,
            ImpactTier::Heavy => 18.0,
        };
        // Оружием держать удар тяжелее, чем щитом.
        if shield { base } else { base * 1.2 }
    }
    /// Сколько вздрагивает защитник после парирования оружием (это и есть окно для ответного удара).
    pub fn parry_stagger(self) -> f32 {
        match self {
            ImpactTier::Weak => 0.3,
            ImpactTier::Medium => 0.45,
            ImpactTier::Heavy => 0.65,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Swing {
    /// Справа налево.
    #[default]
    RightToLeft,
    /// Слева направо.
    LeftToRight,
    /// Сверху вниз.
    Overhead,
    /// Укол вперёд (копьё).
    Thrust,
}

impl Swing {
    pub fn next(self) -> Swing {
        match self {
            Swing::RightToLeft => Swing::LeftToRight,
            Swing::LeftToRight => Swing::Overhead,
            Swing::Overhead => Swing::RightToLeft,
            Swing::Thrust => Swing::Thrust,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum Action {
    #[default]
    Idle,
    Windup { t: f32, swing: Swing, riposte: bool },
    Strike { t: f32, swing: Swing, riposte: bool },
    Recovery { t: f32, swing: Swing },
    /// Стойка защиты (ПКМ зажата). `t` — сколько уже держим.
    Guard { t: f32 },
    /// Защитник принял удар и вздрагивает (защита при этом остаётся).
    GuardStagger { t: f32, dur: f32 },
    /// Оглушение атакующего: спарировали, отскок от щита или стены.
    Stagger { t: f32, dur: f32 },
    /// Короткая заминка от полученного удара.
    Flinch { t: f32 },
    Dead,
}

impl Action {
    /// Время внутри текущего действия.
    pub fn time(&self) -> f32 {
        match *self {
            Action::Windup { t, .. }
            | Action::Strike { t, .. }
            | Action::Recovery { t, .. }
            | Action::Guard { t }
            | Action::GuardStagger { t, .. }
            | Action::Stagger { t, .. }
            | Action::Flinch { t } => t,
            Action::Idle | Action::Dead => 0.0,
        }
    }
}

/// Боевое состояние одного бойца.
#[derive(Clone, Debug)]
pub struct Fighter {
    pub weapon: WeaponKind,
    pub action: Action,
    pub next_swing: Swing,
    pub hp: f32,
    pub max_hp: f32,
    /// Есть ли щит (стойка = блок щитом, без ответного удара).
    pub has_shield: bool,
    /// Попадание запомнено, чтобы удар бил каждую цель один раз.
    pub hit_this_swing: Vec<u64>,
    /// Атака, нажатая во время восстановления, выполнится сразу после него.
    pub queued_attack: bool,
    /// Зажата ли кнопка защиты.
    pub guard_held: bool,
    /// Пока > 0, следующая атака — быстрый ответный удар.
    pub riposte_timer: f32,
    /// Добавка к окну ответного удара (из экипировки), секунды.
    pub riposte_bonus: f32,
    /// Сколько прошло с конца последнего удара (для сброса серии).
    idle_time: f32,
    pub stamina: f32,
    pub max_stamina: f32,
    /// Пока > 0, выносливость не восстанавливается.
    pub stamina_delay: f32,
    /// Бежит (тратит выносливость).
    pub sprinting: bool,
    /// Множитель скорости ударов (экипировка «Скорость действий»).
    pub action_speed: f32,
    /// Доля снижения потерь выносливости при блоке (экипировка «Устойчивость блока»), 0..0.8.
    pub block_stability: f32,
    /// Множитель восстановления выносливости (тяжёлая броня замедляет).
    pub stamina_regen_mult: f32,
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
            hp: max_hp,
            max_hp,
            has_shield,
            hit_this_swing: Vec::new(),
            queued_attack: false,
            guard_held: false,
            riposte_timer: 0.0,
            riposte_bonus: 0.0,
            idle_time: 0.0,
            stamina: STAMINA_MAX,
            max_stamina: STAMINA_MAX,
            stamina_delay: 0.0,
            sprinting: false,
            action_speed: 1.0,
            block_stability: 0.0,
            stamina_regen_mult: 1.0,
        }
    }

    pub fn is_dead(&self) -> bool {
        matches!(self.action, Action::Dead)
    }

    /// Защита сейчас действует (стойка поднята или защитник вздрагивает после удара).
    pub fn guard_active(&self) -> bool {
        let raise = if self.has_shield { GUARD_RAISE_SHIELD } else { GUARD_RAISE_WEAPON };
        match self.action {
            Action::Guard { t } => t >= raise,
            Action::GuardStagger { .. } => true,
            _ => false,
        }
    }

    pub fn can_riposte(&self) -> bool {
        !self.has_shield && self.riposte_timer > 0.0
    }

    /// Множитель скорости ходьбы в текущем действии.
    pub fn move_speed_mult(&self) -> f32 {
        match self.action {
            Action::Idle => 1.0,
            Action::Windup { .. } | Action::Strike { .. } | Action::Recovery { .. } => 0.6,
            Action::Guard { .. } => 0.55,
            Action::GuardStagger { .. } => 0.3,
            Action::Stagger { .. } => 0.3,
            Action::Flinch { .. } => 0.6,
            Action::Dead => 0.0,
        }
    }

    /// Потратить выносливость.
    pub fn spend_stamina(&mut self, amount: f32) {
        self.stamina = (self.stamina - amount).max(0.0);
        self.stamina_delay = STAMINA_REGEN_DELAY;
    }

    /// Хватает ли сил на бег.
    pub fn can_sprint(&self) -> bool {
        self.stamina > 1.0
    }

    pub fn try_attack(&mut self) -> bool {
        // Без сил не замахнуться.
        if self.stamina < 1.0 {
            return false;
        }
        // Ответный удар после парирования: прямо из вздрагивания.
        if self.can_riposte() && matches!(self.action, Action::Idle | Action::Guard { .. } | Action::GuardStagger { .. }) {
            self.riposte_timer = 0.0;
            self.queued_attack = false;
            self.start_attack(true);
            return true;
        }
        if matches!(self.action, Action::Recovery { .. }) {
            self.queued_attack = true;
            return false;
        }
        if matches!(self.action, Action::Idle | Action::Guard { .. }) {
            self.start_attack(false);
            return true;
        }
        false
    }

    fn start_attack(&mut self, riposte: bool) {
        if self.idle_time > COMBO_RESET {
            self.next_swing = Swing::RightToLeft;
        }
        let swing = if self.weapon.thrust_only() {
            Swing::Thrust
        } else if riposte {
            Swing::LeftToRight
        } else {
            self.next_swing
        };
        self.next_swing = if self.weapon.thrust_only() { Swing::RightToLeft } else { swing.next() };
        let cost = self.weapon.stats().stamina_cost * if riposte { 0.5 } else { 1.0 };
        self.spend_stamina(cost);
        self.queued_attack = false;
        self.action = Action::Windup { t: 0.0, swing, riposte };
    }

    /// Сбросить серию ударов (после отскока).
    pub fn reset_combo(&mut self) {
        self.next_swing = Swing::RightToLeft;
        self.queued_attack = false;
    }

    /// Зажать или отпустить защиту.
    pub fn set_guard(&mut self, on: bool) {
        let on = on && self.weapon.can_guard();
        self.guard_held = on;
        match (on, self.action) {
            (true, Action::Idle) => self.action = Action::Guard { t: 0.0 },
            // Защиту можно поднять, отменив свой замах (финт).
            (true, Action::Windup { riposte: false, .. }) => {
                self.queued_attack = false;
                self.action = Action::Guard { t: 0.0 };
            }
            (false, Action::Guard { .. }) => self.action = Action::Idle,
            _ => {}
        }
    }

    fn after_busy(&self) -> Action {
        if self.guard_held { Action::Guard { t: 0.0 } } else { Action::Idle }
    }

    /// Продвигает таймеры. Возвращает событие смены фазы удара, если было.
    pub fn tick(&mut self, dt: f32) -> Option<TickEvent> {
        let stats = self.weapon.stats();
        let mut event = None;
        self.riposte_timer = (self.riposte_timer - dt).max(0.0);
        self.tick_stamina(dt);
        // Скорость ударов меняется экипировкой.
        let real_dt = dt;
        let dt_attack = dt * self.action_speed.clamp(0.5, 1.6);
        self.action = match self.action {
            Action::Windup { t, swing, riposte } => {
                let t = t + dt_attack;
                let windup = if riposte { RIPOSTE_WINDUP } else { stats.windup };
                if t >= windup {
                    self.hit_this_swing.clear();
                    event = Some(TickEvent::StrikeStarted);
                    Action::Strike { t: t - windup, swing, riposte }
                } else {
                    Action::Windup { t, swing, riposte }
                }
            }
            Action::Strike { t, swing, riposte } => {
                let t = t + dt_attack;
                if t >= stats.strike {
                    event = Some(TickEvent::StrikeEnded);
                    Action::Recovery { t: 0.0, swing }
                } else {
                    Action::Strike { t, swing, riposte }
                }
            }
            Action::Recovery { t, swing } => {
                let t = t + dt_attack;
                if t >= stats.recovery {
                    self.idle_time = 0.0;
                    self.after_busy()
                } else {
                    Action::Recovery { t, swing }
                }
            }
            Action::Guard { t } => Action::Guard { t: t + dt },
            Action::GuardStagger { t, dur } => {
                let t = t + dt;
                if t >= dur {
                    // Стойка уже поднята — сразу действует.
                    if self.guard_held { Action::Guard { t: 1.0 } } else { Action::Idle }
                } else {
                    Action::GuardStagger { t, dur }
                }
            }
            Action::Stagger { t, dur } => {
                let t = t + dt;
                if t >= dur { self.after_busy() } else { Action::Stagger { t, dur } }
            }
            Action::Flinch { t } => {
                let t = t + dt;
                if t >= FLINCH_TIME { self.after_busy() } else { Action::Flinch { t } }
            }
            a @ (Action::Idle | Action::Dead) => a,
        };
        if matches!(self.action, Action::Idle | Action::Guard { .. }) {
            self.idle_time += real_dt;
        }
        if self.queued_attack && matches!(self.action, Action::Idle | Action::Guard { .. }) {
            self.start_attack(false);
        }
        event
    }

    fn tick_stamina(&mut self, dt: f32) {
        if matches!(self.action, Action::Dead) {
            return;
        }
        if self.sprinting {
            self.stamina = (self.stamina - SPRINT_DRAIN * dt).max(0.0);
            self.stamina_delay = self.stamina_delay.max(0.3);
            if self.stamina <= 0.0 {
                self.sprinting = false;
            }
            return;
        }
        if self.stamina_delay > 0.0 {
            self.stamina_delay -= dt;
            return;
        }
        let k = match self.action {
            Action::Guard { .. } | Action::GuardStagger { .. } => 0.4,
            Action::Windup { .. } | Action::Strike { .. } | Action::Recovery { .. } => 0.0,
            _ => 1.0,
        };
        self.stamina = (self.stamina + STAMINA_REGEN * self.stamina_regen_mult * k * dt).min(self.max_stamina);
    }

    /// Нормализованный прогресс фазы удара 0..1 (для анимации и трассировки клинка).
    pub fn strike_progress(&self) -> Option<(Swing, f32)> {
        let stats = self.weapon.stats();
        if self.weapon.ranged() {
            return None;
        }
        match self.action {
            Action::Strike { t, swing, .. } => Some((swing, (t / stats.strike).clamp(0.0, 1.0))),
            _ => None,
        }
    }

    pub fn is_riposte_strike(&self) -> bool {
        matches!(self.action, Action::Strike { riposte: true, .. })
    }
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
        Swing::Thrust => Vec3::new(0.0, -0.06, -1.0).normalize(),
    }
}

/// Рукоять (передняя рука) во время удара: при уколе руки выбрасывают копьё вперёд.
pub fn strike_hilt(swing: Swing, progress: f32) -> Vec3 {
    match swing {
        Swing::Thrust => Vec3::new(0.12, -0.12, 0.25 - 0.75 * smooth(progress.clamp(0.0, 1.0))),
        _ => STRIKE_HILT,
    }
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Точка рукояти в «пространстве прицела» во время удара.
pub const STRIKE_HILT: Vec3 = Vec3::new(0.0, -0.1, -0.35);
const REST_HILT: Vec3 = Vec3::new(0.38, -0.32, -0.5);
/// Стойка парирования оружием: клинок поперёк перед собой, острием влево-вверх.
pub const GUARD_HILT: Vec3 = Vec3::new(0.2, 0.0, -0.45);
pub fn guard_dir() -> Vec3 {
    Vec3::new(-0.85, 0.5, -0.15).normalize()
}
/// Щит в стойке: центр относительно ног (без наклона взгляда), половина высоты и радиус.
pub fn shield_guard_center(pose: &Pose) -> Vec3 {
    pose.feet + Vec3::Y * (pose.eye_height() - 0.4) + Quat::from_rotation_y(pose.yaw) * Vec3::new(-0.05, 0.0, -0.5)
}
const SHIELD_HALF_HEIGHT: f32 = 0.3;
const SHIELD_RADIUS: f32 = 0.32;
const GUARD_BLADE_RADIUS: f32 = 0.08;

fn rest_dir() -> Vec3 {
    Vec3::new(0.3, 0.7, -0.65).normalize()
}

fn mix(a: (Vec3, Vec3), b: (Vec3, Vec3), k: f32) -> (Vec3, Vec3) {
    let k = smooth(k.clamp(0.0, 1.0));
    (a.0.lerp(b.0, k), a.1.lerp(b.1, k).normalize_or(Vec3::Y))
}

/// Положение рукояти и направление клинка в «пространстве прицела» (поворот = yaw, pitch*0.6,
/// начало — плечо) для любого действия. В фазе удара и в стойке совпадает с тем,
/// по чему считается бой.
pub fn weapon_pose(action: &Action, weapon: WeaponKind, time: f32) -> (Vec3, Vec3) {
    let stats = weapon.stats();
    let rest = (REST_HILT, rest_dir());
    let guard = (GUARD_HILT, guard_dir());
    let windup_pose = |swing: Swing| {
        let start = blade_local_dir(swing, 0.0);
        let hilt = match swing {
            Swing::RightToLeft => Vec3::new(0.4, 0.05, -0.15),
            Swing::LeftToRight => Vec3::new(-0.35, 0.0, -0.15),
            Swing::Overhead => Vec3::new(0.1, 0.25, -0.05),
            Swing::Thrust => return (strike_hilt(Swing::Thrust, 0.0) + Vec3::new(0.0, 0.0, 0.1), Vec3::new(0.0, 0.0, -1.0)),
        };
        (hilt, (start + Vec3::Y * 0.6 + Vec3::Z * 0.4).normalize())
    };
    if weapon.ranged() {
        // Лук: держим перед собой вертикально, пока натягиваем и стреляем.
        let aim = (Vec3::new(-0.05, 0.0, -0.55), Vec3::Y);
        let low = (Vec3::new(0.3, -0.35, -0.3), Vec3::new(0.2, 1.0, -0.4).normalize());
        return match *action {
            Action::Windup { t, .. } => mix(low, aim, t / (stats.windup * 0.4)),
            Action::Strike { .. } => aim,
            Action::Recovery { t, .. } => mix(aim, low, t / stats.recovery),
            _ => low,
        };
    }
    match *action {
        Action::Windup { t, swing, riposte } => {
            let dur = if riposte { RIPOSTE_WINDUP } else { stats.windup };
            let from = if riposte { guard } else { rest };
            mix(from, windup_pose(swing), t / dur)
        }
        Action::Strike { t, swing, .. } => {
            let p = (t / stats.strike).clamp(0.0, 1.0);
            (strike_hilt(swing, p), blade_local_dir(swing, p))
        }
        Action::Recovery { t, swing } => mix((strike_hilt(swing, 1.0), blade_local_dir(swing, 1.0)), rest, t / stats.recovery),
        Action::Guard { t } => mix(rest, guard, t / GUARD_RAISE_WEAPON),
        Action::GuardStagger { t, dur } => {
            // Клинок отбрасывает назад и он возвращается в стойку.
            let k = (1.0 - t / dur).max(0.0);
            let wobble = (time * 30.0).sin() * 0.05 * k;
            (GUARD_HILT + Vec3::new(0.1, 0.08, 0.15) * k, (guard_dir() + Vec3::new(0.2, 0.2 + wobble, 0.4) * k).normalize())
        }
        Action::Stagger { t, dur } => {
            let k = (1.0 - t / dur).max(0.0);
            let wobble = (time * 30.0).sin() * 0.08 * k;
            (Vec3::new(0.4, 0.15 + wobble, -0.05), Vec3::new(0.6, 0.8, 0.35 + wobble).normalize())
        }
        Action::Flinch { t } => mix((REST_HILT + Vec3::new(0.05, -0.1, 0.1), rest_dir()), rest, t / FLINCH_TIME),
        Action::Idle | Action::Dead => rest,
    }
}

/// Отрезок клинка в мире: (рукоять, острие).
pub fn blade_segment(pose: &Pose, weapon: WeaponKind, swing: Swing, progress: f32) -> (Vec3, Vec3) {
    let stats = weapon.stats();
    // Наклон по вертикали частично следует за взглядом, чтобы можно было бить вниз/вверх.
    let rot = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_x(pose.pitch * 0.6);
    let dir = rot * blade_local_dir(swing, progress);
    let hilt = pose.shoulder() + rot * strike_hilt(swing, progress);
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
        if level.point_solid(p) {
            return false;
        }
    }
    true
}

/// Отрезок клинка в стойке парирования (мир).
pub fn guard_blade_segment(pose: &Pose, weapon: WeaponKind) -> (Vec3, Vec3) {
    let rot = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_x(pose.pitch * 0.6);
    let hilt = pose.shoulder() + rot * GUARD_HILT;
    // Длинное древко держат поперёк, защищает только его середина.
    (hilt, hilt + rot * guard_dir() * weapon.stats().reach.min(1.1))
}

/// Защищающийся смотрит в сторону атакующего?
pub fn facing(defender: &Pose, attacker_pos: Vec3) -> bool {
    let to = attacker_pos - defender.feet;
    let to = Vec3::new(to.x, 0.0, to.z).normalize_or_zero();
    defender.forward_flat().dot(to) >= DEFENSE_ARC_COS
}

/// Куда попал клинок.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Contact {
    /// В оружие или щит защитника.
    Guard(Vec3),
    Body { point: Vec3, head: bool },
}

/// Проверяет одно положение клинка против защитника: сначала защиту, потом тело.
pub fn trace_blade(hilt: Vec3, tip: Vec3, attacker_pose: &Pose, defender: &Fighter, defender_pose: &Pose) -> Option<Contact> {
    let body = blade_hits_body(hilt, tip, defender_pose);
    // Как в Dark and Darker: поднятая защита отражает любой удар спереди (в секторе обзора),
    // даже если клинок шёл в корпус, а не точно в оружие или щит.
    if defender.guard_active() && facing(defender_pose, attacker_pose.feet) {
        let (a, b, r) = if defender.has_shield {
            let c = shield_guard_center(defender_pose);
            (c - Vec3::Y * SHIELD_HALF_HEIGHT, c + Vec3::Y * SHIELD_HALF_HEIGHT, SHIELD_RADIUS)
        } else {
            let (h, t) = guard_blade_segment(defender_pose, defender.weapon);
            (h, t, GUARD_BLADE_RADIUS)
        };
        if segment_distance_sq(hilt, tip, a, b) <= r * r || body.is_some() {
            return Some(Contact::Guard((a + b) / 2.0));
        }
    }
    body.map(|point| {
        let head_line = defender_pose.feet.y + if defender_pose.crouching { 0.9 } else { 1.42 };
        Contact::Body { point, head: point.y > head_line }
    })
}

/// Клинок задевает стену? Не считаем пол, на котором стоит атакующий.
pub fn blade_hits_wall(level: &LevelColliders, hilt: Vec3, tip: Vec3, feet: Vec3) -> Option<Vec3> {
    for i in 3..=8 {
        let p = hilt.lerp(tip, i as f32 / 8.0);
        let inside = level.boxes_near(p).any(|c| c.max.y > feet.y + 0.3);
        if inside {
            return Some(p);
        }
    }
    None
}

/// Чем закончилось попадание клинка.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum HitOutcome {
    Damage { amount: f32, killed: bool, head: bool },
    /// Отражено оружием: атакующий оглушён, у защитника есть ответный удар.
    Parried,
    /// Принято на щит.
    Blocked { tier: ImpactTier },
    /// Клинок отскочил от стены.
    Bounced,
    /// У защитника кончились силы: защиту пробило, он оглушён.
    GuardBroken,
}

/// Применяет контакт клинка. `damage` — урон с учётом оружия и брони (до множителей головы и ответного удара).
pub fn resolve_contact(attacker: &mut Fighter, defender: &mut Fighter, contact: Contact, damage: f32) -> HitOutcome {
    let stats = attacker.weapon.stats();
    match contact {
        Contact::Guard(_) if guard_breaks(defender, stats.impact_power) => {
            defender.action = Action::Stagger { t: 0.0, dur: GUARD_BREAK_STAGGER };
            defender.riposte_timer = 0.0;
            HitOutcome::GuardBroken
        }
        Contact::Guard(_) if defender.has_shield => {
            let tier = ImpactTier::from_diff(stats.impact_power, SHIELD_RESIST);
            defender.action = Action::GuardStagger { t: 0.0, dur: tier.shield_stagger() };
            if tier != ImpactTier::Heavy {
                attacker.action = Action::Stagger { t: 0.0, dur: BLOCK_BOUNCE };
                attacker.reset_combo();
            }
            HitOutcome::Blocked { tier }
        }
        Contact::Guard(_) => {
            let defender_resist = defender.weapon.stats().guard_resist;
            let dur = ImpactTier::from_diff(stats.impact_power, defender_resist).parry_stagger();
            defender.action = Action::GuardStagger { t: 0.0, dur };
            defender.riposte_timer = dur + RIPOSTE_GRACE + defender.riposte_bonus;
            attacker.action = Action::Stagger { t: 0.0, dur: dur + PARRIED_EXTRA_STAGGER };
            attacker.reset_combo();
            HitOutcome::Parried
        }
        Contact::Body { head, .. } => {
            let mut amount = damage;
            if head {
                amount *= HEAD_MULT;
            }
            if attacker.is_riposte_strike() {
                amount *= RIPOSTE_DAMAGE_MULT;
            }
            defender.hp = (defender.hp - amount).max(0.0);
            let killed = defender.hp <= 0.0;
            if killed {
                defender.action = Action::Dead;
            } else if matches!(defender.action, Action::Windup { .. } | Action::Idle | Action::Recovery { .. }) {
                // Попадание сбивает замах (кто ударил первым — тот и прав). Стойку не сбивает.
                defender.action = Action::Flinch { t: 0.0 };
                defender.queued_attack = false;
            }
            HitOutcome::Damage { amount, killed, head }
        }
    }
}

/// Тратит выносливость защитника на принятый удар. true — сил не хватило, защиту пробило.
fn guard_breaks(defender: &mut Fighter, impact_power: i32) -> bool {
    let resist = if defender.has_shield { SHIELD_RESIST } else { defender.weapon.stats().guard_resist };
    let cost = ImpactTier::from_diff(impact_power, resist).guard_stamina(defender.has_shield) * (1.0 - defender.block_stability.clamp(0.0, 0.8));
    let broken = defender.stamina < cost;
    defender.spend_stamina(cost);
    broken
}

/// Стрела попала в бойца. Щит её останавливает, оружием стрелу не отбить.
pub fn resolve_arrow(defender: &mut Fighter, shield_facing: bool, head: bool, damage: f32) -> HitOutcome {
    if shield_facing && defender.has_shield && defender.guard_active() {
        defender.spend_stamina(ImpactTier::Weak.guard_stamina(true) * (1.0 - defender.block_stability.clamp(0.0, 0.8)));
        return HitOutcome::Blocked { tier: ImpactTier::Weak };
    }
    let amount = if head { damage * HEAD_MULT } else { damage };
    defender.hp = (defender.hp - amount).max(0.0);
    let killed = defender.hp <= 0.0;
    if killed {
        defender.action = Action::Dead;
    } else if matches!(defender.action, Action::Windup { .. } | Action::Idle | Action::Recovery { .. }) {
        defender.action = Action::Flinch { t: 0.0 };
        defender.queued_attack = false;
    }
    HitOutcome::Damage { amount, killed, head }
}

/// Начальная точка и скорость стрелы при выстреле из позы.
pub fn arrow_launch(pose: &Pose) -> (Vec3, Vec3) {
    let dir = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_x(pose.pitch) * Vec3::NEG_Z;
    (pose.feet + Vec3::Y * (pose.eye_height() - 0.1) + dir * 0.6, dir * ARROW_SPEED)
}

/// Наклон, под которым надо стрелять, чтобы попасть из `from` в `to` (с учётом падения стрелы).
pub fn ballistic_pitch(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    let flat = Vec2::new(d.x, d.z).length().max(0.1);
    let t = flat / ARROW_SPEED;
    let drop = 0.5 * ARROW_GRAVITY * t * t;
    (d.y + drop).atan2(flat)
}

/// Летящая стрела (считается на сервере).
#[derive(Clone, Copy, Debug)]
pub struct Arrow {
    pub id: u64,
    pub owner: u64,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Урон до брони цели.
    pub damage: f32,
    pub life: f32,
}

pub enum ArrowStep {
    Flying,
    /// Воткнулась в стену или землю.
    Stuck,
    /// Попала в бойца `targets[index]`. `shield_facing` — цель стоит к стреле лицом.
    Hit { index: usize, point: Vec3, head: bool, shield_facing: bool },
    Expired,
}

/// Продвигает стрелу на `dt`. `targets` — (id, поза) живых бойцов.
pub fn step_arrow(arrow: &mut Arrow, dt: f32, level: &LevelColliders, targets: &[(u64, Pose)]) -> ArrowStep {
    arrow.life -= dt;
    if arrow.life <= 0.0 {
        return ArrowStep::Expired;
    }
    let from = arrow.pos;
    arrow.vel.y -= ARROW_GRAVITY * dt;
    let to = from + arrow.vel * dt;
    arrow.pos = to;
    // Ближайшее попадание в тело вдоль отрезка полёта.
    let mut best: Option<(f32, usize, Vec3)> = None;
    for (i, (id, pose)) in targets.iter().enumerate() {
        if *id == arrow.owner {
            continue;
        }
        let (a, b, _) = body_capsule(pose);
        const R: f32 = 0.3;
        if segment_distance_sq(from, to, a, b) > R * R {
            continue;
        }
        // Точка входа: шагаем по отрезку до первого касания.
        for k in 0..=10 {
            let p = from.lerp(to, k as f32 / 10.0);
            if segment_distance_sq(p, p, a, b) <= R * R {
                let d = from.distance(p);
                if best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, i, p));
                }
                break;
            }
        }
    }
    let wall = !line_of_sight(level, from, to) || to.y < -10.0;
    if let Some((d, index, point)) = best {
        // Стена ближе бойца — стрела в стене.
        if !wall || line_of_sight(level, from, point) {
            let pose = &targets[index].1;
            let head_line = pose.feet.y + if pose.crouching { 0.9 } else { 1.42 };
            let came_from = from - arrow.vel.normalize_or_zero() * 5.0;
            let _ = d;
            return ArrowStep::Hit { index, point, head: point.y > head_line, shield_facing: facing(pose, came_from) };
        }
    }
    if wall { ArrowStep::Stuck } else { ArrowStep::Flying }
}

/// Броня снижает урон: 50 брони ≈ −25%, 100 ≈ −40%.
pub fn armor_reduction(armor: f32) -> f32 {
    let a = armor.max(0.0);
    a / (a + 150.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn pose(x: f32, z: f32, yaw: f32) -> Pose {
        Pose { feet: Vec3::new(x, 0.0, z), yaw, pitch: 0.0, crouching: false }
    }

    /// Атакующий в (0,0) смотрит на -Z; защитник в 1.2 м перед ним.
    fn duel(att: &mut Fighter, def: &mut Fighter, def_yaw: f32, guard_from_frame: Option<usize>) -> Option<HitOutcome> {
        let ap = pose(0.0, 0.0, 0.0);
        let dp = pose(0.0, -1.2, def_yaw);
        att.try_attack();
        for i in 0..240 {
            if Some(i) == guard_from_frame {
                def.set_guard(true);
            }
            att.tick(1.0 / 60.0);
            def.tick(1.0 / 60.0);
            if let Some((swing, p)) = att.strike_progress() {
                let (h, t) = blade_segment(&ap, att.weapon, swing, p);
                if let Some(c) = trace_blade(h, t, &ap, def, &dp) {
                    return Some(resolve_contact(att, def, c, att.weapon.stats().damage));
                }
            }
        }
        None
    }

    fn sword() -> Fighter {
        Fighter::new(WeaponKind::Spatha, 150.0, false)
    }

    #[test]
    fn sword_hits_target_in_front() {
        let (mut a, mut d) = (sword(), sword());
        let out = duel(&mut a, &mut d, PI, None);
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
        assert!(d.hp < 150.0);
    }

    #[test]
    fn sword_misses_target_out_of_reach() {
        let mut a = sword();
        let d = sword();
        a.try_attack();
        let ap = pose(0.0, 0.0, 0.0);
        let dp = pose(0.0, -3.5, PI);
        for _ in 0..120 {
            a.tick(1.0 / 60.0);
            if let Some((s, p)) = a.strike_progress() {
                let (h, t) = blade_segment(&ap, a.weapon, s, p);
                assert!(trace_blade(h, t, &ap, &d, &dp).is_none());
            }
        }
    }

    #[test]
    fn weapon_guard_parries_and_gives_riposte() {
        let (mut a, mut d) = (sword(), sword());
        let out = duel(&mut a, &mut d, PI, Some(0));
        assert_eq!(out, Some(HitOutcome::Parried));
        assert!(matches!(a.action, Action::Stagger { .. }));
        assert!(d.can_riposte());
        // Ответный удар успевает до конца оглушения атакующего.
        assert!(d.try_attack());
        let ap = pose(0.0, -1.2, PI);
        let tp = pose(0.0, 0.0, 0.0);
        let mut landed = false;
        for _ in 0..60 {
            a.tick(1.0 / 60.0);
            d.tick(1.0 / 60.0);
            if let Some((s, p)) = d.strike_progress() {
                let (h, t) = blade_segment(&ap, d.weapon, s, p);
                if let Some(c) = trace_blade(h, t, &ap, &a, &tp) {
                    assert!(matches!(a.action, Action::Stagger { .. }), "атакующий уже очнулся: {:?}", a.action);
                    let out = resolve_contact(&mut d, &mut a, c, 22.0);
                    assert!(matches!(out, HitOutcome::Damage { amount, .. } if amount >= 22.0 * RIPOSTE_DAMAGE_MULT - 0.01));
                    landed = true;
                    break;
                }
            }
        }
        assert!(landed);
    }

    #[test]
    fn guard_from_behind_fails() {
        let (mut a, mut d) = (sword(), sword());
        // Защитник стоит спиной.
        let out = duel(&mut a, &mut d, 0.0, Some(0));
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
    }

    #[test]
    fn guard_too_late_takes_damage() {
        let (mut a, mut d) = (sword(), sword());
        // Стойка поднята за пару кадров до удара — не успевает подняться.
        let frames_to_strike = (WeaponKind::Spatha.stats().windup * 60.0) as usize;
        let out = duel(&mut a, &mut d, PI, Some(frames_to_strike - 2));
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
    }

    #[test]
    fn shield_blocks_by_impact() {
        // Меч против щита: слабый удар, атакующий отскакивает.
        let mut a = sword();
        let mut d = Fighter::new(WeaponKind::Spatha, 150.0, true);
        let out = duel(&mut a, &mut d, PI, Some(0));
        assert_eq!(out, Some(HitOutcome::Blocked { tier: ImpactTier::Weak }));
        assert!(matches!(a.action, Action::Stagger { .. }));
        assert!(!d.can_riposte());
        // Топор против щита: тяжёлый удар, атакующий продолжает серию.
        let mut a = Fighter::new(WeaponKind::Axe, 200.0, false);
        let mut d = Fighter::new(WeaponKind::Spatha, 150.0, true);
        let out = duel(&mut a, &mut d, PI, Some(0));
        assert_eq!(out, Some(HitOutcome::Blocked { tier: ImpactTier::Heavy }));
        assert!(matches!(a.action, Action::Strike { .. }));
        assert!(matches!(d.action, Action::GuardStagger { dur, .. } if dur == ImpactTier::Heavy.shield_stagger()));
    }

    #[test]
    fn combo_resets_after_pause() {
        let mut f = sword();
        let mut swings = Vec::new();
        // Третий удар после паузы начинает серию заново.
        for pause in [0.0, 0.0, 1.0] {
            for _ in 0..(pause * 60.0) as usize {
                f.tick(1.0 / 60.0);
            }
            f.try_attack();
            if let Action::Windup { swing, .. } = f.action {
                swings.push(swing);
            }
            for _ in 0..(1.2 * 60.0) as usize {
                f.tick(1.0 / 60.0);
            }
        }
        assert_eq!(swings, vec![Swing::RightToLeft, Swing::LeftToRight, Swing::RightToLeft]);
    }

    #[test]
    fn feint_into_guard() {
        let mut f = sword();
        f.try_attack();
        f.tick(0.1);
        f.set_guard(true);
        assert!(matches!(f.action, Action::Guard { .. }));
    }

    #[test]
    fn spear_thrust_reaches_far() {
        let mut a = Fighter::new(WeaponKind::Spear, 150.0, false);
        let mut d = sword();
        // Копейщик достаёт цель в 2,3 м, меч — нет.
        let ap = pose(0.0, 0.0, 0.0);
        let dp = pose(0.0, -2.3, PI);
        a.try_attack();
        let mut hit = false;
        for _ in 0..120 {
            a.tick(1.0 / 60.0);
            if let Some((s, p)) = a.strike_progress() {
                assert_eq!(s, Swing::Thrust);
                let (h, t) = blade_segment(&ap, a.weapon, s, p);
                hit |= trace_blade(h, t, &ap, &d, &dp).is_some();
            }
        }
        assert!(hit, "копьё не достало");
        d.tick(0.0);
    }

    #[test]
    fn stamina_runs_out_and_guard_breaks() {
        let mut a = Fighter::new(WeaponKind::Axe, 150.0, false);
        let mut d = Fighter::new(WeaponKind::Spatha, 150.0, true);
        let mut broken = false;
        for _ in 0..20 {
            a.stamina = STAMINA_MAX;
            a.action = Action::Idle;
            d.guard_held = false;
            d.action = Action::Idle;
            if let Some(out) = duel(&mut a, &mut d, PI, Some(0)) {
                if out == HitOutcome::GuardBroken {
                    broken = true;
                    assert!(matches!(d.action, Action::Stagger { .. }));
                    break;
                }
            }
        }
        assert!(broken, "защиту так и не пробило, выносливость {}", d.stamina);
        // Без сил не замахнуться.
        let mut f = sword();
        f.stamina = 0.0;
        assert!(!f.try_attack());
    }

    #[test]
    fn arrow_hits_and_shield_blocks() {
        let level = LevelColliders::new(Vec::new(), None);
        let shooter = pose(0.0, 0.0, 0.0);
        let target = pose(0.0, -15.0, PI);
        let (pos, _) = arrow_launch(&shooter);
        let mut sp = shooter;
        sp.pitch = ballistic_pitch(pos, target.feet + Vec3::Y * 1.2);
        let (pos, vel) = arrow_launch(&sp);
        let mut arrow = Arrow { id: 1, owner: 1, pos, vel, damage: 30.0, life: 5.0 };
        let targets = [(1, sp), (2, target)];
        let mut hit = None;
        for _ in 0..300 {
            match step_arrow(&mut arrow, 1.0 / 60.0, &level, &targets) {
                ArrowStep::Flying => {}
                ArrowStep::Hit { index, shield_facing, head, .. } => {
                    hit = Some((index, shield_facing, head));
                    break;
                }
                _ => break,
            }
        }
        let (index, shield_facing, head) = hit.expect("стрела не попала");
        assert_eq!(index, 1);
        let mut d = Fighter::new(WeaponKind::Spatha, 150.0, true);
        d.set_guard(true);
        d.tick(0.5);
        assert_eq!(resolve_arrow(&mut d, shield_facing, head, 30.0), HitOutcome::Blocked { tier: ImpactTier::Weak });
        // Оружием стрелу не отбить.
        let mut d = sword();
        d.set_guard(true);
        d.tick(0.5);
        assert!(matches!(resolve_arrow(&mut d, shield_facing, head, 30.0), HitOutcome::Damage { .. }));
    }

    #[test]
    fn weapon_from_item_name() {
        assert_eq!(WeaponKind::from_item_name("Боевой топор"), WeaponKind::Axe);
        assert_eq!(WeaponKind::from_item_name("Кинжал"), WeaponKind::Seax);
        assert_eq!(WeaponKind::from_item_name("Пехотное копьё"), WeaponKind::Spear);
        assert_eq!(WeaponKind::from_item_name("Кавалерийская спата"), WeaponKind::Spatha);
    }

    #[test]
    fn armor_reduces_damage() {
        assert!((armor_reduction(150.0) - 0.5).abs() < 1e-6);
        assert_eq!(armor_reduction(0.0), 0.0);
    }
}
