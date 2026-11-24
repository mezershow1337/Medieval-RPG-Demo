//! Общая логика ближнего боя (используют и сервер, и клиент).
//!
//! Устроено по образцу Chivalry 2 и Mordhau (только механики):
//! * **Направленные удары.** Удар идёт с той стороны, куда игрок ведёт мышь в момент нажатия:
//!   справа, слева, сверху, сверху наискось, снизу наискось; мышь вниз (или средняя кнопка) — укол.
//!   Удар: замах -> удар (клинок идёт по дуге, урон только там, где реально проходит) -> восстановление.
//! * **Финт** (Q или ПКМ во время замаха): отменить замах, это стоит сил.
//!   **Морф**: во время замаха нажать другой удар — замах перестраивается на него (один раз за удар).
//! * **Парирование по таймингу и взгляду.** ПКМ поднимает оружие; первые доли секунды — активное
//!   парирование: удар, попавший в это окно, отбит, атакующий отброшен, у защитника — быстрый
//!   **рипост**. Если держать дольше — глухой блок: урона нет, но силы уходят быстро, рипоста нет,
//!   а инициатива у атакующего. Отбить можно только удар, который идёт туда, куда вы смотрите
//!   (удар сверху — смотреть выше, со спины и с фланга не отбить). Пустое парирование
//!   (никто не ударил) — короткая заминка, так что спамить ПКМ нельзя.
//! * Отброшенный парированием может успеть поднять защиту и отбить рипост — так рождаются
//!   обмены парированиями, как в Chivalry.
//! * **Чембер**: ответить на удар тем же зеркальным ударом в самом начале своего замаха —
//!   ваш удар отбивает чужой и продолжается.
//! * **Комбо**: удар, нажатый во время удара, который попал, начинается сразу и быстрее.
//!   Промах стоит сил и полного восстановления.
//! * Щит держит удары сколько угодно (по «силе удара» оружия против устойчивости щита),
//!   а вовремя поднятый — тоже парирует.
//! * Выносливость: удары, финты, морфы, промахи, блоки и бег тратят силы; попадание немного
//!   возвращает. Когда силы кончаются в стойке, защиту пробивает.
//! * Оружие — эпохи Херсонеса 510 г.: спата, топор, сакс, копьё-контос, составной лук, кулаки.

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
    /// Пустые руки: прямые и боковые удары кулаком. Слабо, медленно для боя с оружием,
    /// парировать клинок голой рукой нельзя (защищает только щит, если он в левой руке).
    Fists,
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponStats {
    /// Замах, удар и восстановление базового (рубящего) удара, с.
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

/// Темп и сила одного удара из набора приёмов оружия.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackTiming {
    pub windup: f32,
    pub release: f32,
    pub recovery: f32,
    /// Множитель урона.
    pub damage: f32,
    /// Множитель цены в выносливости.
    pub stamina: f32,
}

impl WeaponKind {
    pub fn stats(self) -> WeaponStats {
        match self {
            WeaponKind::Spatha => WeaponStats {
                windup: 0.42,
                strike: 0.3,
                recovery: 0.45,
                damage: 22.0,
                reach: 0.95,
                impact_power: 3,
                guard_resist: 3,
                stamina_cost: 12.0,
            },
            WeaponKind::Axe => WeaponStats {
                windup: 0.48,
                strike: 0.3,
                recovery: 0.5,
                damage: 26.0,
                reach: 0.75,
                impact_power: 5,
                guard_resist: 2,
                stamina_cost: 15.0,
            },
            WeaponKind::Seax => WeaponStats {
                windup: 0.3,
                strike: 0.2,
                recovery: 0.32,
                damage: 13.0,
                reach: 0.5,
                impact_power: 1,
                guard_resist: 1,
                stamina_cost: 7.0,
            },
            WeaponKind::Spear => WeaponStats {
                windup: 0.5,
                strike: 0.26,
                recovery: 0.55,
                damage: 24.0,
                reach: 2.1,
                impact_power: 4,
                guard_resist: 3,
                stamina_cost: 14.0,
            },
            // У лука «замах» — натяжение тетивы, «удар» — выстрел.
            WeaponKind::Bow => WeaponStats {
                windup: 1.1,
                strike: 0.05,
                recovery: 0.7,
                damage: 30.0,
                reach: 0.0,
                impact_power: 2,
                guard_resist: 0,
                stamina_cost: 10.0,
            },
            // Кулак: «клинок» — от костяшек вперёд на длину выпада.
            WeaponKind::Fists => WeaponStats {
                windup: 0.34,
                strike: 0.18,
                recovery: 0.4,
                damage: FIST_DAMAGE,
                reach: 0.4,
                impact_power: 0,
                guard_resist: 0,
                stamina_cost: 8.0,
            },
        }
    }

    /// Набор приёмов: темп и сила каждого удара этим оружием.
    pub fn timing(self, swing: Swing) -> AttackTiming {
        let s = self.stats();
        let swing = self.normalize(swing);
        let base = AttackTiming {
            windup: s.windup,
            release: s.strike,
            recovery: s.recovery,
            damage: 1.0,
            stamina: 1.0,
        };
        let (w, r, rec, dmg, st) = match (self, swing.class()) {
            (_, SwingClass::Horizontal) => (1.0, 1.0, 1.0, 1.0, 1.0),
            (_, SwingClass::Rising) => (1.0, 1.0, 1.05, 0.92, 1.0),
            // Топором и булавой сверху — самый тяжёлый удар.
            (WeaponKind::Axe, SwingClass::Overhead) => (1.12, 1.0, 1.12, 1.25, 1.15),
            (WeaponKind::Axe, SwingClass::Diagonal) => (1.06, 1.0, 1.05, 1.12, 1.05),
            (_, SwingClass::Overhead) => (1.1, 1.0, 1.1, 1.15, 1.1),
            (_, SwingClass::Diagonal) => (1.05, 1.0, 1.04, 1.07, 1.05),
            // Укол: спата им колет хуже, чем рубит; сакс — его главный удар; копьё только колет.
            (WeaponKind::Spatha, SwingClass::Thrust) => (0.95, 0.85, 1.0, 0.9, 0.9),
            (WeaponKind::Seax, SwingClass::Thrust) => (0.95, 0.9, 1.0, 1.3, 0.9),
            (_, SwingClass::Thrust) => (1.0, 1.0, 1.0, 1.0, 1.0),
        };
        AttackTiming {
            windup: base.windup * w,
            release: base.release * r,
            recovery: base.recovery * rec,
            damage: dmg,
            stamina: st,
        }
    }

    /// Какой удар на самом деле выйдет этим оружием, если нажать `swing`.
    /// Копьё только колет; топором не уколоть (выйдет удар сверху); кулаком — прямой и боковые.
    pub fn normalize(self, swing: Swing) -> Swing {
        match self {
            WeaponKind::Spear | WeaponKind::Bow => Swing::Thrust,
            WeaponKind::Axe if swing == Swing::Thrust => Swing::Overhead,
            WeaponKind::Fists => match swing {
                Swing::Thrust | Swing::Overhead => Swing::Thrust,
                s if s.screen_dir().x > 0.0 => Swing::RightToLeft,
                _ => Swing::LeftToRight,
            },
            _ => swing,
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
        !self.ranged() && self != WeaponKind::Fists
    }
    /// Можно ли держать защиту с этим оружием и (возможно) щитом в левой руке.
    pub fn can_guard_with(self, shield: bool) -> bool {
        self.can_guard() || (shield && self == WeaponKind::Fists)
    }

    /// Какое оружие соответствует предмету (по названию предмета).
    pub fn from_item_name(name: &str) -> WeaponKind {
        if let Some(a) = crate::arsenal::find(name) {
            return a.fam.kind();
        }
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

/// Урон удара кулаком без доспеха (у игрока прибавляется сила, см. server.rs).
pub const FIST_DAMAGE: f32 = 7.0;

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
/// Цена финта, морфа, чембера и промаха; сколько сил возвращает попадание.
pub const FEINT_STAMINA: f32 = 8.0;
pub const MORPH_STAMINA: f32 = 5.0;
pub const CHAMBER_STAMINA: f32 = 10.0;
pub const WHIFF_STAMINA: f32 = 6.0;
pub const HIT_STAMINA_REFUND: f32 = 6.0;

/// Устойчивость щита.
pub const SHIELD_RESIST: i32 = 4;
/// Сколько поднимается стойка (до этого защиты ещё нет).
pub const GUARD_RAISE_WEAPON: f32 = 0.08;
pub const GUARD_RAISE_SHIELD: f32 = 0.14;
/// Окно активного парирования после подъёма стойки. Дальше — глухой блок.
pub const PARRY_ACTIVE: f32 = 0.32;
/// Заминка после пустого парирования (никто не ударил, ПКМ отпущена).
pub const PARRY_WHIFF_RECOVERY: f32 = 0.35;
/// Сколько вздрагивает защитник после удачного парирования.
pub const PARRY_SHOCK: f32 = 0.12;
/// Сколько после этого ещё можно начать рипост.
pub const RIPOSTE_WINDOW: f32 = 0.45;
/// Замах рипоста.
pub const RIPOSTE_WINDUP: f32 = 0.26;
pub const RIPOSTE_DAMAGE_MULT: f32 = 1.2;
/// Отдача у того, чей удар отбили: сначала нельзя ничего, после `RECOIL_GUARD` можно поднять
/// защиту (и успеть отбить рипост), атаковать — только после конца отдачи.
pub const PARRIED_RECOIL: f32 = 0.55;
pub const CHAMBERED_RECOIL: f32 = 0.5;
pub const RECOIL_GUARD: f32 = 0.25;
/// Чембер: свой зеркальный замах должен быть не старше этого, когда приходит чужой удар.
pub const CHAMBER_WINDOW: f32 = 0.3;
/// Морф возможен, пока замах не дошёл до этой доли.
pub const MORPH_LATEST: f32 = 0.8;
/// Комбо: после попадания следующий удар начинается через столько после конца удара
/// и замахивается быстрее.
pub const COMBO_MIN_RECOVERY: f32 = 0.12;
pub const COMBO_WINDUP_MULT: f32 = 0.85;
/// Атака прямо из окна парирования (без удара по вам) замахивается дольше на столько.
pub const PARRY_CANCEL_PENALTY: f32 = 0.1;
/// Отскок от щита (если удар не тяжёлый) и от стены.
pub const BLOCK_BOUNCE: f32 = 0.4;
pub const WALL_BOUNCE: f32 = 0.5;
pub const FLINCH_TIME: f32 = 0.3;
/// Пауза после удара, после которой серия начинается заново.
pub const COMBO_RESET: f32 = 0.9;
pub const HEAD_MULT: f32 = 1.5;
/// Защита в принципе работает только спереди (примерно ±75 градусов от взгляда).
pub const DEFENSE_ARC_COS: f32 = 0.26;
/// Парировать можно удар, идущий в пределах этого конуса от взгляда (оружие ~70°, щит ~77°).
pub const PARRY_CONE_COS: f32 = 0.34;
pub const SHIELD_CONE_COS: f32 = 0.22;

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
            ImpactTier::Weak => 0.35,
            ImpactTier::Medium => 0.6,
            ImpactTier::Heavy => 1.0,
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
    /// Сколько вздрагивает защитник, принявший удар в глухой блок оружием.
    pub fn held_stagger(self) -> f32 {
        match self {
            ImpactTier::Weak => 0.25,
            ImpactTier::Medium => 0.35,
            ImpactTier::Heavy => 0.5,
        }
    }
}

/// Удар: откуда идёт клинок. Направление выбирает мышь.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum Swing {
    /// Справа налево.
    #[default]
    RightToLeft,
    /// Слева направо.
    LeftToRight,
    /// Сверху вниз.
    Overhead,
    /// Сверху справа наискось.
    UpperRight,
    /// Сверху слева наискось.
    UpperLeft,
    /// Снизу справа вверх.
    LowerRight,
    /// Снизу слева вверх.
    LowerLeft,
    /// Укол вперёд.
    Thrust,
}

/// Род удара (для темпа и урона).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwingClass {
    Horizontal,
    Diagonal,
    Overhead,
    Rising,
    Thrust,
}

impl Swing {
    pub const ALL: [Swing; 8] = [
        Swing::RightToLeft,
        Swing::UpperRight,
        Swing::Overhead,
        Swing::UpperLeft,
        Swing::LeftToRight,
        Swing::LowerLeft,
        Swing::Thrust,
        Swing::LowerRight,
    ];

    /// Откуда идёт клинок на экране (x вправо, y вверх), для укола — ноль.
    pub fn screen_dir(self) -> Vec2 {
        const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
        match self {
            Swing::RightToLeft => Vec2::new(1.0, 0.0),
            Swing::LeftToRight => Vec2::new(-1.0, 0.0),
            Swing::Overhead => Vec2::new(0.0, 1.0),
            Swing::UpperRight => Vec2::new(D, D),
            Swing::UpperLeft => Vec2::new(-D, D),
            Swing::LowerRight => Vec2::new(D, -D),
            Swing::LowerLeft => Vec2::new(-D, -D),
            Swing::Thrust => Vec2::ZERO,
        }
    }

    pub fn class(self) -> SwingClass {
        match self {
            Swing::RightToLeft | Swing::LeftToRight => SwingClass::Horizontal,
            Swing::UpperRight | Swing::UpperLeft => SwingClass::Diagonal,
            Swing::Overhead => SwingClass::Overhead,
            Swing::LowerRight | Swing::LowerLeft => SwingClass::Rising,
            Swing::Thrust => SwingClass::Thrust,
        }
    }

    /// Удар, который встречает этот лоб в лоб (для чембера): у двух бойцов, стоящих лицом
    /// друг к другу, удар справа одного идёт навстречу удару слева другого; сверху — сверху.
    pub fn mirror(self) -> Swing {
        match self {
            Swing::RightToLeft => Swing::LeftToRight,
            Swing::LeftToRight => Swing::RightToLeft,
            Swing::UpperRight => Swing::UpperLeft,
            Swing::UpperLeft => Swing::UpperRight,
            Swing::LowerRight => Swing::LowerLeft,
            Swing::LowerLeft => Swing::LowerRight,
            s => s,
        }
    }

    /// Следующий удар серии, если направление не выбрано: справа, слева, сверху.
    pub fn next(self) -> Swing {
        match self {
            Swing::RightToLeft => Swing::LeftToRight,
            Swing::LeftToRight => Swing::Overhead,
            Swing::Thrust => Swing::Thrust,
            _ => Swing::RightToLeft,
        }
    }

    /// Удар по короткому имени (для снимков и отладки): r, l, o, ur, ul, lr, ll, t.
    pub fn from_code(code: &str) -> Option<Swing> {
        Some(match code.trim().to_lowercase().as_str() {
            "r" => Swing::RightToLeft,
            "l" => Swing::LeftToRight,
            "o" => Swing::Overhead,
            "ur" => Swing::UpperRight,
            "ul" => Swing::UpperLeft,
            "lr" => Swing::LowerRight,
            "ll" => Swing::LowerLeft,
            "t" => Swing::Thrust,
            _ => return None,
        })
    }

    /// Удар по движению мыши (x вправо, y вверх): куда ведёте — оттуда и пойдёт клинок;
    /// вниз — укол. Слишком маленькое движение — `None` (удар по серии).
    pub fn from_mouse(delta: Vec2, threshold: f32) -> Option<Swing> {
        if delta.length() < threshold {
            return None;
        }
        let a = delta.y.atan2(delta.x).to_degrees();
        Some(match a {
            a if (-22.5..22.5).contains(&a) => Swing::RightToLeft,
            a if (22.5..67.5).contains(&a) => Swing::UpperRight,
            a if (67.5..112.5).contains(&a) => Swing::Overhead,
            a if (112.5..157.5).contains(&a) => Swing::UpperLeft,
            a if (-67.5..-22.5).contains(&a) => Swing::LowerRight,
            a if (-112.5..-67.5).contains(&a) => Swing::Thrust,
            a if (-157.5..-112.5).contains(&a) => Swing::LowerLeft,
            _ => Swing::LeftToRight,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum Action {
    #[default]
    Idle,
    /// Замах. `dur` — его полная длина (рипост, комбо и морф её меняют).
    Windup {
        t: f32,
        dur: f32,
        swing: Swing,
        riposte: bool,
        morphed: bool,
    },
    Strike {
        t: f32,
        swing: Swing,
        riposte: bool,
    },
    Recovery {
        t: f32,
        swing: Swing,
    },
    /// Стойка защиты (ПКМ). `t` — сколько уже держим: подъём, окно парирования, глухой блок.
    Guard {
        t: f32,
    },
    /// Защитник принял удар и вздрагивает (защита при этом остаётся).
    GuardStagger {
        t: f32,
        dur: f32,
    },
    /// Пустое парирование: заминка, ни ударить, ни снова парировать.
    ParryRecovery {
        t: f32,
    },
    /// Отдача: удар отбили (парирование, чембер, щит). После `RECOIL_GUARD` можно защищаться.
    Recoil {
        t: f32,
        dur: f32,
    },
    /// Оглушение: защиту пробили, клинок ударился в стену.
    Stagger {
        t: f32,
        dur: f32,
    },
    /// Короткая заминка от полученного удара.
    Flinch {
        t: f32,
    },
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
            | Action::ParryRecovery { t }
            | Action::Recoil { t, .. }
            | Action::Stagger { t, .. }
            | Action::Flinch { t } => t,
            Action::Idle | Action::Dead => 0.0,
        }
    }
    /// Удар, который сейчас готовится или идёт.
    pub fn swing(&self) -> Option<Swing> {
        match *self {
            Action::Windup { swing, .. }
            | Action::Strike { swing, .. }
            | Action::Recovery { swing, .. } => Some(swing),
            _ => None,
        }
    }
    pub fn is_attacking(&self) -> bool {
        matches!(self, Action::Windup { .. } | Action::Strike { .. })
    }
    /// Сколько осталось до начала удара (по замаху), `None` — не замахивается.
    pub fn time_to_release(&self) -> Option<f32> {
        match *self {
            Action::Windup { t, dur, .. } => Some((dur - t).max(0.0)),
            _ => None,
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
    /// Есть ли щит (стойка = блок щитом).
    pub has_shield: bool,
    /// Попадание запомнено, чтобы удар бил каждую цель один раз.
    pub hit_this_swing: Vec<u64>,
    /// Атака, нажатая во время удара или восстановления, выполнится сразу, как будет можно.
    pub queued_attack: bool,
    pub queued_swing: Option<Swing>,
    /// Зажата ли кнопка защиты.
    pub guard_held: bool,
    /// В этой стойке уже что-то приняли (тогда отпустить ПКМ можно без заминки).
    pub guard_used: bool,
    /// Последний принятый удар отбит вовремя (после вздрагивания окно парирования снова открыто).
    pub last_parry_timed: bool,
    /// Этот удар попал (в тело или в глухой блок) — можно комбо.
    pub swing_landed: bool,
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
            queued_swing: None,
            guard_held: false,
            guard_used: false,
            last_parry_timed: false,
            swing_landed: false,
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

    pub fn guard_raise(&self) -> f32 {
        if self.has_shield {
            GUARD_RAISE_SHIELD
        } else {
            GUARD_RAISE_WEAPON
        }
    }

    /// Защита сейчас действует (стойка поднята или защитник вздрагивает после удара).
    pub fn guard_active(&self) -> bool {
        match self.action {
            Action::Guard { t } => t >= self.guard_raise(),
            Action::GuardStagger { .. } => true,
            _ => false,
        }
    }

    /// Открыто окно активного парирования (удар сейчас будет отбит с рипостом).
    pub fn in_parry_window(&self) -> bool {
        let raise = self.guard_raise();
        match self.action {
            Action::Guard { t } => t >= raise && t < raise + PARRY_ACTIVE,
            Action::GuardStagger { .. } => self.last_parry_timed,
            _ => false,
        }
    }

    pub fn can_riposte(&self) -> bool {
        self.riposte_timer > 0.0
    }

    /// Темп удара этим оружием.
    pub fn timing(&self, swing: Swing) -> AttackTiming {
        self.weapon.timing(swing)
    }

    /// Множитель урона текущего удара (род удара и рипост).
    pub fn strike_damage_mult(&self) -> f32 {
        match self.action {
            Action::Strike { swing, riposte, .. } => {
                self.timing(swing).damage * if riposte { RIPOSTE_DAMAGE_MULT } else { 1.0 }
            }
            _ => 1.0,
        }
    }

    /// Множитель скорости ходьбы в текущем действии.
    pub fn move_speed_mult(&self) -> f32 {
        match self.action {
            Action::Idle => 1.0,
            Action::Windup { .. } | Action::Strike { .. } | Action::Recovery { .. } => 0.6,
            Action::Guard { .. } => 0.55,
            Action::GuardStagger { .. } => 0.3,
            Action::ParryRecovery { .. } => 0.7,
            Action::Recoil { .. } => 0.45,
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

    /// Нажата атака. `choice` — направление (`None` — по серии: справа, слева, сверху).
    /// Во время своего замаха другой удар — морф; во время удара или восстановления — в очередь
    /// (после попадания выйдет комбо); после парирования — рипост.
    pub fn try_attack(&mut self, choice: Option<Swing>) -> bool {
        // Без сил не замахнуться.
        if self.stamina < 1.0 {
            return false;
        }
        let choice = choice.map(|s| self.weapon.normalize(s));
        // Ответный удар после парирования: прямо из вздрагивания.
        if self.can_riposte()
            && matches!(
                self.action,
                Action::Idle | Action::Guard { .. } | Action::GuardStagger { .. }
            )
        {
            self.riposte_timer = 0.0;
            self.start_attack(choice, true, false);
            return true;
        }
        match self.action {
            Action::Windup {
                t,
                dur,
                swing,
                riposte: false,
                morphed: false,
            } => {
                // Морф: замах перестраивается на другой удар (дольше, чем просто продолжить).
                let Some(new) = choice else { return false };
                if new == swing || t >= dur * MORPH_LATEST {
                    return false;
                }
                let nd = self.timing(new).windup;
                self.spend_stamina(MORPH_STAMINA);
                self.action = Action::Windup {
                    t: t.min(nd * 0.4),
                    dur: nd,
                    swing: new,
                    riposte: false,
                    morphed: true,
                };
                true
            }
            Action::Strike { .. } | Action::Recovery { .. } => {
                self.queued_attack = true;
                self.queued_swing = choice;
                false
            }
            Action::Guard { t } => {
                // Атака из ещё не использованного окна парирования — с заминкой (иначе
                // «ПКМ-ЛКМ» было бы бесплатным парированием перед каждым ударом).
                let early = !self.guard_used && t < self.guard_raise() + PARRY_ACTIVE;
                self.start_attack(choice, false, false);
                if early && let Action::Windup { t, .. } = &mut self.action {
                    *t = -PARRY_CANCEL_PENALTY;
                }
                true
            }
            Action::Idle => {
                self.start_attack(choice, false, false);
                true
            }
            _ => false,
        }
    }

    fn start_attack(&mut self, choice: Option<Swing>, riposte: bool, combo: bool) {
        if self.idle_time > COMBO_RESET && !combo {
            self.next_swing = Swing::RightToLeft;
        }
        let swing = self.weapon.normalize(choice.unwrap_or(self.next_swing));
        self.next_swing = self.weapon.normalize(swing.next());
        if self.weapon.thrust_only() {
            self.next_swing = Swing::Thrust;
        }
        let timing = self.timing(swing);
        let cost =
            self.weapon.stats().stamina_cost * timing.stamina * if riposte { 0.5 } else { 1.0 };
        self.spend_stamina(cost);
        self.queued_attack = false;
        self.queued_swing = None;
        self.swing_landed = false;
        self.idle_time = 0.0;
        let dur = if riposte {
            RIPOSTE_WINDUP
        } else {
            timing.windup * if combo { COMBO_WINDUP_MULT } else { 1.0 }
        };
        self.action = Action::Windup {
            t: 0.0,
            dur,
            swing,
            riposte,
            morphed: false,
        };
    }

    /// Финт: бросить замах. Стоит сил.
    pub fn feint(&mut self) -> bool {
        if !matches!(self.action, Action::Windup { .. }) {
            return false;
        }
        self.spend_stamina(FEINT_STAMINA);
        self.queued_attack = false;
        self.idle_time = 0.0;
        self.action = if self.guard_held {
            self.fresh_guard()
        } else {
            Action::Idle
        };
        true
    }

    /// Сбросить серию ударов (после отскока).
    pub fn reset_combo(&mut self) {
        self.next_swing = if self.weapon.thrust_only() {
            Swing::Thrust
        } else {
            Swing::RightToLeft
        };
        self.queued_attack = false;
        self.queued_swing = None;
    }

    fn fresh_guard(&mut self) -> Action {
        self.guard_used = false;
        self.last_parry_timed = false;
        Action::Guard { t: 0.0 }
    }

    /// Зажать или отпустить защиту.
    pub fn set_guard(&mut self, on: bool) {
        let on = on && self.weapon.can_guard_with(self.has_shield);
        self.guard_held = on;
        match (on, self.action) {
            (true, Action::Idle) => self.action = self.fresh_guard(),
            // Защиту можно поднять, бросив свой замах (финт в парирование).
            (true, Action::Windup { .. }) => {
                self.spend_stamina(FEINT_STAMINA);
                self.queued_attack = false;
                self.action = self.fresh_guard();
            }
            (true, Action::Recoil { t, .. }) if t >= RECOIL_GUARD => {
                self.action = self.fresh_guard()
            }
            // Отпустили в окне парирования: окно доживает до конца (короткое нажатие = полное
            // парирование), а дальше — заминка, если ничего не приняли (см. `tick`).
            (false, Action::Guard { t }) => {
                if self.guard_used || t >= self.guard_raise() + PARRY_ACTIVE {
                    self.action = Action::Idle;
                }
            }
            _ => {}
        }
    }

    fn after_busy(&mut self) -> Action {
        if self.guard_held {
            self.fresh_guard()
        } else {
            Action::Idle
        }
    }

    /// Продвигает таймеры. Возвращает событие смены фазы удара, если было.
    pub fn tick(&mut self, dt: f32) -> Option<TickEvent> {
        let mut event = None;
        self.riposte_timer = (self.riposte_timer - dt).max(0.0);
        self.tick_stamina(dt);
        // Скорость ударов меняется экипировкой.
        let real_dt = dt;
        let dt_attack = dt * self.action_speed.clamp(0.5, 1.6);
        let raise = self.guard_raise();
        self.action = match self.action {
            Action::Windup {
                t,
                dur,
                swing,
                riposte,
                morphed,
            } => {
                let t = t + dt_attack;
                if t >= dur {
                    self.hit_this_swing.clear();
                    event = Some(TickEvent::StrikeStarted);
                    Action::Strike {
                        t: t - dur,
                        swing,
                        riposte,
                    }
                } else {
                    Action::Windup {
                        t,
                        dur,
                        swing,
                        riposte,
                        morphed,
                    }
                }
            }
            Action::Strike { t, swing, riposte } => {
                let t = t + dt_attack;
                if t >= self.timing(swing).release {
                    event = Some(TickEvent::StrikeEnded);
                    // Промах стоит сил (размахивать впустую утомительно).
                    if !self.swing_landed && !self.weapon.ranged() {
                        self.spend_stamina(WHIFF_STAMINA);
                    }
                    Action::Recovery { t: 0.0, swing }
                } else {
                    Action::Strike { t, swing, riposte }
                }
            }
            Action::Recovery { t, swing } => {
                let t = t + dt_attack;
                if self.queued_attack
                    && self.swing_landed
                    && t >= COMBO_MIN_RECOVERY
                    && self.stamina >= 1.0
                {
                    // Комбо: после попадания следующий удар идёт сразу.
                    let q = self.queued_swing;
                    self.start_attack(q, false, true);
                    self.action
                } else if t >= self.timing(swing).recovery {
                    self.idle_time = 0.0;
                    self.after_busy()
                } else {
                    Action::Recovery { t, swing }
                }
            }
            Action::Guard { t } => {
                let t = t + dt;
                if !self.guard_held && t >= raise + PARRY_ACTIVE {
                    if self.guard_used {
                        Action::Idle
                    } else {
                        Action::ParryRecovery { t: 0.0 }
                    }
                } else {
                    Action::Guard { t }
                }
            }
            Action::GuardStagger { t, dur } => {
                let t = t + dt;
                if t >= dur {
                    // Вовремя отбили — окно парирования снова открыто (против второго врага),
                    // иначе стойка уже глухая.
                    if !self.guard_held {
                        Action::Idle
                    } else if self.last_parry_timed {
                        Action::Guard { t: raise }
                    } else {
                        Action::Guard {
                            t: raise + PARRY_ACTIVE,
                        }
                    }
                } else {
                    Action::GuardStagger { t, dur }
                }
            }
            Action::ParryRecovery { t } => {
                let t = t + dt;
                if t >= PARRY_WHIFF_RECOVERY {
                    self.after_busy()
                } else {
                    Action::ParryRecovery { t }
                }
            }
            Action::Recoil { t, dur } => {
                let t = t + dt;
                if self.guard_held && t >= RECOIL_GUARD {
                    self.fresh_guard()
                } else if t >= dur {
                    self.after_busy()
                } else {
                    Action::Recoil { t, dur }
                }
            }
            Action::Stagger { t, dur } => {
                let t = t + dt;
                if t >= dur {
                    self.after_busy()
                } else {
                    Action::Stagger { t, dur }
                }
            }
            Action::Flinch { t } => {
                let t = t + dt;
                if t >= FLINCH_TIME {
                    self.after_busy()
                } else {
                    Action::Flinch { t }
                }
            }
            a @ (Action::Idle | Action::Dead) => a,
        };
        if matches!(self.action, Action::Idle | Action::Guard { .. }) {
            self.idle_time += real_dt;
        }
        if self.queued_attack && matches!(self.action, Action::Idle | Action::Guard { .. }) {
            let q = self.queued_swing;
            if self.stamina >= 1.0 {
                self.start_attack(q, false, false);
            } else {
                self.queued_attack = false;
            }
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
        self.stamina =
            (self.stamina + STAMINA_REGEN * self.stamina_regen_mult * k * dt).min(self.max_stamina);
    }

    /// Нормализованный прогресс фазы удара 0..1 (для анимации и трассировки клинка).
    pub fn strike_progress(&self) -> Option<(Swing, f32)> {
        if self.weapon.ranged() {
            return None;
        }
        match self.action {
            Action::Strike { t, swing, .. } => {
                Some((swing, (t / self.timing(swing).release).clamp(0.0, 1.0)))
            }
            _ => None,
        }
    }
}

/// Застывшее действие для снимков: `фаза:удар:доля`, фаза — windup, strike, recovery, guard
/// (доля < 0.5 — окно парирования, больше — глухой блок), riposte, recoil, whiff.
pub fn showcase_action(spec: &str, weapon: WeaponKind) -> Option<Action> {
    let mut it = spec.split(':');
    let phase = it.next()?.trim();
    let swing = weapon.normalize(it.next().and_then(Swing::from_code).unwrap_or_default());
    let k: f32 = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0.5);
    let t = weapon.timing(swing);
    Some(match phase {
        "windup" => Action::Windup {
            t: t.windup * k,
            dur: t.windup,
            swing,
            riposte: false,
            morphed: false,
        },
        "riposte" => Action::Windup {
            t: RIPOSTE_WINDUP * k,
            dur: RIPOSTE_WINDUP,
            swing,
            riposte: true,
            morphed: false,
        },
        "strike" => Action::Strike {
            t: t.release * k,
            swing,
            riposte: false,
        },
        "recovery" => Action::Recovery {
            t: t.recovery * k,
            swing,
        },
        "guard" => Action::Guard {
            t: if k < 0.5 {
                GUARD_RAISE_WEAPON + PARRY_ACTIVE * 0.5
            } else {
                1.0
            },
        },
        "recoil" => Action::Recoil {
            t: PARRIED_RECOIL * k,
            dur: PARRIED_RECOIL,
        },
        "whiff" => Action::ParryRecovery {
            t: PARRY_WHIFF_RECOVERY * k,
        },
        "idle" => Action::Idle,
        _ => return None,
    })
}

/// Наибольшая скорость поворота (рад/с) во время удара: в замахе свободнее, в самом ударе
/// туже. `None` — без ограничения.
pub fn turn_cap(action: &Action) -> Option<f32> {
    match action {
        Action::Windup { .. } => Some(7.0),
        Action::Strike { .. } => Some(3.6),
        _ => None,
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
    /// Куда смотрит (с наклоном головы).
    pub fn look_dir(&self) -> Vec3 {
        Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch) * Vec3::NEG_Z
    }
}

/// «Сторона» удара в локальных координатах бойца: откуда идёт клинок.
fn side3(swing: Swing) -> Vec3 {
    let s = swing.screen_dir();
    Vec3::new(s.x, s.y, 0.0)
}

/// Направление клинка в локальных координатах бойца для данного удара и прогресса 0..1.
/// -Z — вперёд, +X — вправо, +Y — вверх. Клинок идёт дугой с «той стороны» через «вперёд»
/// на противоположную.
pub fn blade_local_dir(swing: Swing, progress: f32) -> Vec3 {
    let p = smooth(progress.clamp(0.0, 1.0));
    if swing == Swing::Thrust {
        return Vec3::new(0.0, -0.06, -1.0).normalize();
    }
    let s = side3(swing);
    // Дуга: от `a0` градусов в сторону удара до `a1` в противоположную.
    // Сверху — заканчиваем клинком вперёд-вниз, снизу — начинаем ниже и уходим выше.
    let (a0, a1): (f32, f32) = match swing.class() {
        SwingClass::Overhead => (80.0, 50.0),
        SwingClass::Diagonal => (78.0, 65.0),
        SwingClass::Rising => (65.0, 75.0),
        _ => (75.0, 75.0),
    };
    let a = (a0 - (a0 + a1) * p).to_radians();
    // Горизонтальные удары чуть клонятся вниз, как у живого бойца.
    let bias = match swing {
        Swing::RightToLeft => Vec3::new(0.0, -0.1, 0.0),
        Swing::LeftToRight => Vec3::new(0.0, 0.05, 0.0),
        Swing::Overhead => Vec3::new(0.08, 0.0, 0.0),
        _ => Vec3::ZERO,
    };
    (s * a.sin() + Vec3::NEG_Z * a.cos() + bias).normalize()
}

/// Рукоять (передняя рука) во время удара: при уколе руки выбрасывают оружие вперёд,
/// при рубящем ударе кисти проходят поперёк тела вслед за клинком.
pub fn strike_hilt(swing: Swing, progress: f32) -> Vec3 {
    let p = smooth(progress.clamp(0.0, 1.0));
    match swing {
        Swing::Thrust => Vec3::new(0.12, -0.12, 0.25 - 0.75 * p),
        s => STRIKE_HILT + side3(s) * 0.12 * (1.0 - 2.0 * p),
    }
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Замедление к концу (замах «взводится»).
fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
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
    pose.feet
        + Vec3::Y * (pose.eye_height() - 0.4)
        + Quat::from_rotation_y(pose.yaw) * Vec3::new(-0.05, 0.0, -0.5)
}
const SHIELD_HALF_HEIGHT: f32 = 0.3;
const SHIELD_RADIUS: f32 = 0.32;
const GUARD_BLADE_RADIUS: f32 = 0.08;

fn rest_dir() -> Vec3 {
    Vec3::new(0.3, 0.7, -0.65).normalize()
}

/// Копьё от первого лица в покое: не торчит столбом у края экрана, а держится у пояса
/// острием вперёд и чуть вверх — левая рука впереди на древке, правая сзади у бедра.
const SPEAR_VIEW_REST_HILT: Vec3 = Vec3::new(0.18, -0.35, -0.46);
fn spear_view_rest_dir() -> Vec3 {
    Vec3::new(-0.045, 0.16, -0.986).normalize()
}

/// Положение оружия для вида от первого лица: как `weapon_pose`, но у копья стойка покоя
/// своя (см. `SPEAR_VIEW_REST_HILT`); замах и укол плавно выходят из неё и возвращаются.
pub fn view_weapon_pose(action: &Action, weapon: WeaponKind, time: f32) -> (Vec3, Vec3) {
    let (hilt, dir) = weapon_pose(action, weapon, time);
    if weapon != WeaponKind::Spear {
        return (hilt, dir);
    }
    // Насколько поза сейчас — стойка покоя (1 — в ней, 0 — далеко от неё).
    let near = (1.0 - hilt.distance(REST_HILT) / 0.35).clamp(0.0, 1.0);
    let along = ((dir.dot(rest_dir()) - 0.75) / 0.25).clamp(0.0, 1.0);
    let w = near * along;
    let mut h = hilt + (SPEAR_VIEW_REST_HILT - REST_HILT) * w;
    let mut d = dir.lerp(spear_view_rest_dir(), w).normalize_or(dir);
    // Древко поперёк (блок): копьё сдвигаем к острию, чтобы левая рука была слева, а правая
    // справа — иначе левая тянется через весь экран.
    h += d * 0.3 * d.x.abs();
    // Укол и замах: острие чуть вверх, руки ниже — тупой конец уходит под камеру, а не в лицо.
    let fwd = (-d.z).max(0.0) * (1.0 - w);
    d = (d + Vec3::Y * 0.25 * fwd).normalize_or(d);
    h += Vec3::new(0.05, -0.12, 0.0) * fwd;
    (h, d)
}

fn mix(a: (Vec3, Vec3), b: (Vec3, Vec3), k: f32) -> (Vec3, Vec3) {
    let k = smooth(k.clamp(0.0, 1.0));
    (a.0.lerp(b.0, k), a.1.lerp(b.1, k).normalize_or(Vec3::Y))
}

/// Взведённый замах: клинок отведён за плечо с той стороны, откуда пойдёт удар.
pub fn windup_pose(swing: Swing) -> (Vec3, Vec3) {
    if swing == Swing::Thrust {
        return (
            strike_hilt(Swing::Thrust, 0.0) + Vec3::new(0.02, 0.05, 0.12),
            Vec3::new(0.02, 0.04, -1.0).normalize(),
        );
    }
    let s = side3(swing);
    let start = blade_local_dir(swing, 0.0);
    let low = s.y < -0.3;
    let hilt = Vec3::new(0.02, if low { -0.12 } else { 0.06 }, -0.14)
        + Vec3::new(s.x * 0.36, s.y.max(0.0) * 0.24, 0.0);
    let up = if low { -0.15 } else { 0.4 };
    (
        hilt,
        (start * 0.4 + s * 0.5 + Vec3::Z * 0.4 + Vec3::Y * up).normalize(),
    )
}

/// Положение рукояти и направление клинка в «пространстве прицела» (поворот = yaw, pitch*0.6,
/// начало — плечо) для любого действия. В фазе удара и в стойке совпадает с тем,
/// по чему считается бой.
pub fn weapon_pose(action: &Action, weapon: WeaponKind, time: f32) -> (Vec3, Vec3) {
    let rest = (REST_HILT, rest_dir());
    let guard = (GUARD_HILT, guard_dir());
    if weapon == WeaponKind::Fists {
        return fist_pose(action, time);
    }
    if weapon.ranged() {
        let stats = weapon.stats();
        // Лук: держим перед собой вертикально, пока натягиваем и стреляем.
        let aim = (Vec3::new(-0.05, 0.0, -0.55), Vec3::Y);
        let low = (
            Vec3::new(0.3, -0.35, -0.3),
            Vec3::new(0.2, 1.0, -0.4).normalize(),
        );
        return match *action {
            Action::Windup { t, .. } => mix(low, aim, t / (stats.windup * 0.4)),
            Action::Strike { .. } => aim,
            Action::Recovery { t, .. } => mix(aim, low, t / stats.recovery),
            _ => low,
        };
    }
    match *action {
        Action::Windup {
            t,
            dur,
            swing,
            riposte,
            ..
        } => {
            let from = if riposte { guard } else { rest };
            let (h, d) = windup_pose(swing);
            let k = ease_out(t.max(0.0) / dur.max(0.01));
            // Небольшой «перевзвод» в конце замаха — клинок дрожит от напряжения.
            let tremble = if k > 0.85 {
                (time * 40.0).sin() * 0.006
            } else {
                0.0
            };
            (
                from.0.lerp(h, k) + Vec3::Y * tremble,
                from.1.lerp(d, k).normalize_or(Vec3::Y),
            )
        }
        Action::Strike { t, swing, .. } => {
            let p = (t / weapon.timing(swing).release).clamp(0.0, 1.0);
            (strike_hilt(swing, p), blade_local_dir(swing, p))
        }
        Action::Recovery { t, swing } => mix(
            (strike_hilt(swing, 1.0), blade_local_dir(swing, 1.0)),
            rest,
            t / weapon.timing(swing).recovery,
        ),
        Action::Guard { t } => {
            // Подъём, потом окно парирования (клинок выброшен навстречу удару),
            // потом глухой блок (клинок ближе к телу и ниже).
            let raise = GUARD_RAISE_WEAPON;
            let snap = (
                GUARD_HILT + Vec3::new(0.0, 0.05, -0.08),
                (guard_dir() + Vec3::new(0.0, 0.08, -0.1)).normalize(),
            );
            let held = (
                GUARD_HILT + Vec3::new(-0.02, -0.05, 0.06),
                (guard_dir() + Vec3::new(0.05, -0.08, 0.05)).normalize(),
            );
            if t < raise {
                mix(rest, snap, t / raise)
            } else if t < raise + PARRY_ACTIVE {
                snap
            } else {
                mix(snap, held, (t - raise - PARRY_ACTIVE) / 0.15)
            }
        }
        Action::GuardStagger { t, dur } => {
            // Клинок отбрасывает назад и он возвращается в стойку.
            let k = (1.0 - t / dur).max(0.0);
            let wobble = (time * 30.0).sin() * 0.05 * k;
            (
                GUARD_HILT + Vec3::new(0.1, 0.08, 0.15) * k,
                (guard_dir() + Vec3::new(0.2, 0.2 + wobble, 0.4) * k).normalize(),
            )
        }
        Action::ParryRecovery { t } => {
            // Пустое парирование: клинок опускается, рука «проваливается».
            let k = t / PARRY_WHIFF_RECOVERY;
            let dip = (
                GUARD_HILT + Vec3::new(0.08, -0.18, 0.05),
                (guard_dir() + Vec3::new(0.3, -0.4, 0.0)).normalize(),
            );
            if k < 0.4 {
                mix(guard, dip, k / 0.4)
            } else {
                mix(dip, rest, (k - 0.4) / 0.6)
            }
        }
        Action::Recoil { t, dur } => {
            // Удар отбит: клинок отлетает вверх-назад и возвращается.
            let k = (t / dur).clamp(0.0, 1.0);
            let wobble = (time * 32.0).sin() * 0.06 * (1.0 - k);
            let thrown = (
                Vec3::new(0.42, 0.2 + wobble, -0.02),
                Vec3::new(0.7, 0.75, 0.35 + wobble).normalize(),
            );
            if k < 0.25 {
                thrown
            } else {
                mix(thrown, rest, (k - 0.25) / 0.75)
            }
        }
        Action::Stagger { t, dur } => {
            let k = (1.0 - t / dur).max(0.0);
            let wobble = (time * 30.0).sin() * 0.08 * k;
            (
                Vec3::new(0.4, 0.15 + wobble, -0.05),
                Vec3::new(0.6, 0.8, 0.35 + wobble).normalize(),
            )
        }
        Action::Flinch { t } => mix(
            (REST_HILT + Vec3::new(0.05, -0.1, 0.1), rest_dir()),
            rest,
            t / FLINCH_TIME,
        ),
        Action::Idle | Action::Dead => rest,
    }
}

/// Кулак (пустые руки): «рукоять» — центр кулака, направление — от костяшек вперёд.
/// Прямой — выпад вперёд; боковой — кулак идёт дугой со стороны.
fn fist_pose(action: &Action, time: f32) -> (Vec3, Vec3) {
    let fwd = Vec3::new(0.0, -0.06, -1.0).normalize();
    let rest = (
        Vec3::new(0.2, -0.2, -0.3),
        Vec3::new(-0.15, 0.25, -1.0).normalize(),
    );
    let w = WeaponKind::Fists;
    let cocked = |swing: Swing| match swing {
        Swing::Thrust => (
            strike_hilt(Swing::Thrust, 0.0) + Vec3::new(0.0, 0.08, 0.0),
            fwd,
        ),
        s => (
            Vec3::new(0.32 * s.screen_dir().x.signum(), -0.1, -0.12),
            Vec3::new(-0.6 * s.screen_dir().x.signum(), 0.1, -0.8).normalize(),
        ),
    };
    let punch = |swing: Swing, p: f32| match swing {
        Swing::Thrust => (strike_hilt(Swing::Thrust, p), fwd),
        s => {
            // Боковой: кулак идёт дугой со стороны к центру и чуть дальше.
            let side = s.screen_dir().x.signum();
            let a = (70.0 - 110.0 * smooth(p)).to_radians();
            (
                Vec3::new(side * 0.42 * a.sin(), -0.12, -0.15 - 0.4 * a.cos()),
                Vec3::new(-side * a.cos() * 0.8, 0.05, -1.0).normalize(),
            )
        }
    };
    match *action {
        Action::Windup { t, dur, swing, .. } => {
            mix(rest, cocked(swing), t.max(0.0) / dur.max(0.01))
        }
        Action::Strike { t, swing, .. } => {
            punch(swing, (t / w.timing(swing).release).clamp(0.0, 1.0))
        }
        Action::Recovery { t, swing } => mix(punch(swing, 1.0), rest, t / w.timing(swing).recovery),
        Action::Stagger { t, dur }
        | Action::GuardStagger { t, dur }
        | Action::Recoil { t, dur } => {
            let k = (1.0 - t / dur).max(0.0);
            let wobble = (time * 30.0).sin() * 0.05 * k;
            (rest.0 + Vec3::new(0.1, 0.05 + wobble, 0.15) * k, rest.1)
        }
        Action::Flinch { t } => mix(
            (rest.0 + Vec3::new(0.05, -0.1, 0.1), rest.1),
            rest,
            t / FLINCH_TIME,
        ),
        // Со щитом в стойке правый кулак прижат к груди.
        Action::Guard { t } => mix(
            rest,
            (Vec3::new(0.15, -0.25, -0.2), rest.1),
            t / GUARD_RAISE_WEAPON,
        ),
        Action::ParryRecovery { .. } | Action::Idle | Action::Dead => rest,
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
            let mut s0 = if denom != 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
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
    (
        hilt,
        hilt + rot * guard_dir() * weapon.stats().reach.min(1.1),
    )
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
    /// В тело. `frontal` — защитник стоит к атакующему лицом (нужно для чембера).
    Body {
        point: Vec3,
        head: bool,
        frontal: bool,
    },
}

/// Удар идёт туда, куда смотрит защитник (в конус парирования)? Смотрим, где клинок
/// относительно груди защитника: удар сверху надо встречать взглядом выше, снизу — ниже,
/// а удар, обходящий сбоку, — повернувшись к нему.
pub fn guard_covers(defender_pose: &Pose, hilt: Vec3, tip: Vec3, shield: bool) -> bool {
    let chest = defender_pose.feet + Vec3::Y * (defender_pose.eye_height() - 0.45);
    let mid = hilt.lerp(tip, 0.6);
    let to = (mid - chest).normalize_or_zero();
    let cone = if shield {
        SHIELD_CONE_COS
    } else {
        PARRY_CONE_COS
    };
    defender_pose.look_dir().dot(to) >= cone
}

/// Проверяет одно положение клинка против защитника: сначала защиту, потом тело.
pub fn trace_blade(
    hilt: Vec3,
    tip: Vec3,
    attacker_pose: &Pose,
    defender: &Fighter,
    defender_pose: &Pose,
) -> Option<Contact> {
    let body = blade_hits_body(hilt, tip, defender_pose);
    let frontal = facing(defender_pose, attacker_pose.feet);
    // Поднятая защита отражает удар, если клинок попал в оружие или щит, или идёт в тело
    // с той стороны, куда защитник смотрит.
    if defender.guard_active() && frontal {
        let (a, b, r) = if defender.has_shield {
            let c = shield_guard_center(defender_pose);
            (
                c - Vec3::Y * SHIELD_HALF_HEIGHT,
                c + Vec3::Y * SHIELD_HALF_HEIGHT,
                SHIELD_RADIUS,
            )
        } else {
            let (h, t) = guard_blade_segment(defender_pose, defender.weapon);
            (h, t, GUARD_BLADE_RADIUS)
        };
        let touches_guard = segment_distance_sq(hilt, tip, a, b) <= r * r;
        if touches_guard
            || (body.is_some() && guard_covers(defender_pose, hilt, tip, defender.has_shield))
        {
            return Some(Contact::Guard((a + b) / 2.0));
        }
    }
    body.map(|point| {
        let head_line = defender_pose.feet.y + if defender_pose.crouching { 0.9 } else { 1.42 };
        Contact::Body {
            point,
            head: point.y > head_line,
            frontal,
        }
    })
}

/// Клинок задевает стену? Не считаем пол, на котором стоит атакующий.
pub fn blade_hits_wall(level: &LevelColliders, hilt: Vec3, tip: Vec3, feet: Vec3) -> Option<Vec3> {
    for i in 3..=8 {
        let p = hilt.lerp(tip, i as f32 / 8.0);
        // Живые люди — не стена: по ним клинок бьёт, а не отскакивает.
        let inside = level
            .point_in_wall(p)
            .is_some_and(|c| c.max.y > feet.y + 0.3);
        if inside {
            return Some(p);
        }
    }
    None
}

/// Чем закончилось попадание клинка.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum HitOutcome {
    Damage {
        amount: f32,
        killed: bool,
        head: bool,
    },
    /// Отбито вовремя (оружием или щитом): атакующего отбросило, у защитника — рипост.
    Parried,
    /// Принято в глухой блок (щитом или удержанным оружием): урона нет, но силы уходят.
    Blocked { tier: ImpactTier },
    /// Встречный зеркальный удар отбил этот, и защитник продолжает свой.
    Chambered,
    /// Клинок отскочил от стены.
    Bounced,
    /// У защитника кончились силы: защиту пробило, он оглушён.
    GuardBroken,
}

/// Может ли защитник прямо сейчас отбить этот удар встречным (чембер).
pub fn can_chamber(defender: &Fighter, attack: Swing) -> bool {
    match defender.action {
        Action::Windup {
            t,
            swing,
            riposte: false,
            ..
        } => {
            swing == attack.mirror()
                && (0.0..=CHAMBER_WINDOW).contains(&t)
                && defender.stamina >= CHAMBER_STAMINA * 0.5
        }
        _ => false,
    }
}

/// Применяет контакт клинка. `damage` — урон с учётом оружия и брони (до множителей рода удара,
/// головы и ответного удара).
pub fn resolve_contact(
    attacker: &mut Fighter,
    defender: &mut Fighter,
    contact: Contact,
    damage: f32,
) -> HitOutcome {
    let stats = attacker.weapon.stats();
    let attack_swing = attacker.action.swing().unwrap_or_default();
    match contact {
        Contact::Guard(_) => {
            let timed = defender.in_parry_window();
            let resist = if defender.has_shield {
                SHIELD_RESIST
            } else {
                defender.weapon.stats().guard_resist
            };
            let tier = ImpactTier::from_diff(stats.impact_power, resist);
            // Вовремя — дёшево; глухой блок оружием — дорого, щитом — как обычно.
            let mult = if timed {
                0.4
            } else if defender.has_shield {
                1.0
            } else {
                1.5
            };
            defender.guard_used = true;
            if guard_breaks(defender, tier.guard_stamina(defender.has_shield) * mult) {
                defender.action = Action::Stagger {
                    t: 0.0,
                    dur: GUARD_BREAK_STAGGER,
                };
                defender.riposte_timer = 0.0;
                attacker.swing_landed = true;
                return HitOutcome::GuardBroken;
            }
            if timed {
                defender.last_parry_timed = true;
                defender.action = Action::GuardStagger {
                    t: 0.0,
                    dur: PARRY_SHOCK,
                };
                defender.riposte_timer = PARRY_SHOCK + RIPOSTE_WINDOW + defender.riposte_bonus;
                // Тяжёлое оружие отбрасывает отбитого меньше, лёгкое против тяжёлого — больше.
                attacker.action = Action::Recoil {
                    t: 0.0,
                    dur: PARRIED_RECOIL,
                };
                attacker.reset_combo();
                return HitOutcome::Parried;
            }
            defender.last_parry_timed = false;
            if defender.has_shield {
                defender.action = Action::GuardStagger {
                    t: 0.0,
                    dur: tier.shield_stagger(),
                };
                if tier != ImpactTier::Heavy {
                    attacker.action = Action::Recoil {
                        t: 0.0,
                        dur: BLOCK_BOUNCE,
                    };
                    attacker.reset_combo();
                } else {
                    attacker.swing_landed = true;
                }
            } else {
                // Глухой блок оружием: держится, но инициатива у атакующего (его удар не
                // прерван, можно комбо), а защитник дольше приходит в себя.
                defender.action = Action::GuardStagger {
                    t: 0.0,
                    dur: tier.held_stagger(),
                };
                attacker.swing_landed = true;
            }
            HitOutcome::Blocked { tier }
        }
        Contact::Body { frontal: true, .. } if can_chamber(defender, attack_swing) => {
            // Встречный удар: свой замах отбивает чужой и сразу продолжается.
            defender.spend_stamina(CHAMBER_STAMINA);
            if let Action::Windup { t, dur, swing, .. } = defender.action {
                let left = (dur - t).min(RIPOSTE_WINDUP);
                defender.action = Action::Windup {
                    t: 0.0,
                    dur: left.max(0.05),
                    swing,
                    riposte: false,
                    morphed: true,
                };
            }
            attacker.action = Action::Recoil {
                t: 0.0,
                dur: CHAMBERED_RECOIL,
            };
            attacker.reset_combo();
            HitOutcome::Chambered
        }
        Contact::Body { head, .. } => {
            let mut amount = damage * attacker.strike_damage_mult();
            if head {
                amount *= HEAD_MULT;
            }
            attacker.swing_landed = true;
            attacker.stamina = (attacker.stamina + HIT_STAMINA_REFUND).min(attacker.max_stamina);
            defender.hp = (defender.hp - amount).max(0.0);
            let killed = defender.hp <= 0.0;
            if killed {
                defender.action = Action::Dead;
            } else if matches!(
                defender.action,
                Action::Windup { .. }
                    | Action::Idle
                    | Action::Recovery { .. }
                    | Action::ParryRecovery { .. }
            ) {
                // Попадание сбивает замах (кто ударил первым — тот и прав). Стойку не сбивает.
                defender.action = Action::Flinch { t: 0.0 };
                defender.queued_attack = false;
            }
            HitOutcome::Damage {
                amount,
                killed,
                head,
            }
        }
    }
}

/// Тратит выносливость защитника на принятый удар. true — сил не хватило, защиту пробило.
fn guard_breaks(defender: &mut Fighter, cost: f32) -> bool {
    let cost = cost * (1.0 - defender.block_stability.clamp(0.0, 0.8));
    let broken = defender.stamina < cost;
    defender.spend_stamina(cost);
    broken
}

/// Стрела попала в бойца. Щит её останавливает, оружием стрелу не отбить.
pub fn resolve_arrow(
    defender: &mut Fighter,
    shield_facing: bool,
    head: bool,
    damage: f32,
) -> HitOutcome {
    if shield_facing && defender.has_shield && defender.guard_active() {
        defender.spend_stamina(
            ImpactTier::Weak.guard_stamina(true) * (1.0 - defender.block_stability.clamp(0.0, 0.8)),
        );
        defender.guard_used = true;
        return HitOutcome::Blocked {
            tier: ImpactTier::Weak,
        };
    }
    let amount = if head { damage * HEAD_MULT } else { damage };
    defender.hp = (defender.hp - amount).max(0.0);
    let killed = defender.hp <= 0.0;
    if killed {
        defender.action = Action::Dead;
    } else if matches!(
        defender.action,
        Action::Windup { .. }
            | Action::Idle
            | Action::Recovery { .. }
            | Action::ParryRecovery { .. }
    ) {
        defender.action = Action::Flinch { t: 0.0 };
        defender.queued_attack = false;
    }
    HitOutcome::Damage {
        amount,
        killed,
        head,
    }
}

/// Начальная точка и скорость стрелы при выстреле из позы.
pub fn arrow_launch(pose: &Pose) -> (Vec3, Vec3) {
    let dir = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_x(pose.pitch) * Vec3::NEG_Z;
    (
        pose.feet + Vec3::Y * (pose.eye_height() - 0.1) + dir * 0.6,
        dir * ARROW_SPEED,
    )
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
    /// Пробивание доспеха наконечником (см. `archery::Shot`).
    pub pierce: f32,
}

pub enum ArrowStep {
    Flying,
    /// Воткнулась в стену или землю.
    Stuck,
    /// Попала в бойца `targets[index]`. `shield_facing` — цель стоит к стреле лицом.
    Hit {
        index: usize,
        point: Vec3,
        head: bool,
        shield_facing: bool,
    },
    Expired,
}

/// Продвигает стрелу на `dt`. `targets` — (id, поза) живых бойцов.
pub fn step_arrow(
    arrow: &mut Arrow,
    dt: f32,
    level: &LevelColliders,
    targets: &[(u64, Pose)],
) -> ArrowStep {
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
            return ArrowStep::Hit {
                index,
                point,
                head: point.y > head_line,
                shield_facing: facing(pose, came_from),
            };
        }
    }
    if wall {
        ArrowStep::Stuck
    } else {
        ArrowStep::Flying
    }
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

    const DT: f32 = 1.0 / 60.0;

    fn pose(x: f32, z: f32, yaw: f32) -> Pose {
        Pose {
            feet: Vec3::new(x, 0.0, z),
            yaw,
            pitch: 0.0,
            crouching: false,
        }
    }

    /// Атакующий в (0,0) смотрит на -Z; защитник в 1.2 м перед ним. Защитник нажимает ПКМ на
    /// кадре `guard_from_frame`. Возвращает исход и кадр первого контакта.
    fn duel_full(
        att: &mut Fighter,
        def: &mut Fighter,
        dp: Pose,
        swing: Option<Swing>,
        guard_from_frame: Option<usize>,
    ) -> Option<(HitOutcome, usize)> {
        let ap = pose(0.0, 0.0, 0.0);
        att.try_attack(swing);
        for i in 0..240 {
            if Some(i) == guard_from_frame {
                def.set_guard(true);
            }
            att.tick(DT);
            def.tick(DT);
            if let Some((swing, p)) = att.strike_progress() {
                let (h, t) = blade_segment(&ap, att.weapon, swing, p);
                if let Some(c) = trace_blade(h, t, &ap, def, &dp) {
                    return Some((resolve_contact(att, def, c, att.weapon.stats().damage), i));
                }
            }
        }
        None
    }

    fn duel(
        att: &mut Fighter,
        def: &mut Fighter,
        def_yaw: f32,
        guard_from_frame: Option<usize>,
    ) -> Option<HitOutcome> {
        duel_full(att, def, pose(0.0, -1.2, def_yaw), None, guard_from_frame).map(|r| r.0)
    }

    fn sword() -> Fighter {
        Fighter::new(WeaponKind::Spatha, 150.0, false)
    }

    /// Кадр, на котором удар `swing` достаёт неподвижную цель.
    fn contact_frame(weapon: WeaponKind, swing: Swing) -> usize {
        let mut a = Fighter::new(weapon, 150.0, false);
        let mut d = sword();
        duel_full(&mut a, &mut d, pose(0.0, -1.2, PI), Some(swing), None)
            .expect("удар не достал")
            .1
    }

    /// Кадр, когда нажать ПКМ, чтобы удар пришёлся на середину окна парирования.
    fn timed_guard_frame(weapon: WeaponKind, swing: Swing) -> usize {
        contact_frame(weapon, swing) - ((GUARD_RAISE_WEAPON + PARRY_ACTIVE * 0.5) / DT) as usize
    }

    #[test]
    fn every_direction_hits_target_in_front() {
        for swing in Swing::ALL {
            let (mut a, mut d) = (sword(), sword());
            let out = duel_full(&mut a, &mut d, pose(0.0, -1.2, PI), Some(swing), None);
            assert!(
                matches!(out, Some((HitOutcome::Damage { .. }, _))),
                "{swing:?}: {out:?}"
            );
        }
    }

    #[test]
    fn sword_misses_target_out_of_reach() {
        let mut a = sword();
        let d = sword();
        a.try_attack(None);
        let ap = pose(0.0, 0.0, 0.0);
        let dp = pose(0.0, -3.5, PI);
        for _ in 0..120 {
            a.tick(DT);
            if let Some((s, p)) = a.strike_progress() {
                let (h, t) = blade_segment(&ap, a.weapon, s, p);
                assert!(trace_blade(h, t, &ap, &d, &dp).is_none());
            }
        }
    }

    #[test]
    fn timed_parry_gives_riposte_that_lands() {
        let (mut a, mut d) = (sword(), sword());
        let frame = timed_guard_frame(WeaponKind::Spatha, Swing::RightToLeft);
        let out = duel_full(
            &mut a,
            &mut d,
            pose(0.0, -1.2, PI),
            Some(Swing::RightToLeft),
            Some(frame),
        )
        .map(|r| r.0);
        assert_eq!(out, Some(HitOutcome::Parried));
        assert!(matches!(a.action, Action::Recoil { .. }));
        assert!(d.can_riposte());
        // Отбитый не защищается — рипост попадает с бонусом.
        assert!(d.try_attack(None));
        let ap = pose(0.0, -1.2, PI);
        let tp = pose(0.0, 0.0, 0.0);
        let mut landed = false;
        for _ in 0..60 {
            a.tick(DT);
            d.tick(DT);
            if let Some((s, p)) = d.strike_progress() {
                let (h, t) = blade_segment(&ap, d.weapon, s, p);
                if let Some(c) = trace_blade(h, t, &ap, &a, &tp) {
                    let out = resolve_contact(&mut d, &mut a, c, 22.0);
                    assert!(
                        matches!(out, HitOutcome::Damage { amount, .. } if amount >= 22.0 * RIPOSTE_DAMAGE_MULT - 0.01),
                        "{out:?}"
                    );
                    landed = true;
                    break;
                }
            }
        }
        assert!(landed);
    }

    /// Отбитый успевает поднять защиту и отбить рипост — обмен парированиями.
    #[test]
    fn parried_attacker_can_parry_the_riposte() {
        let (mut a, mut d) = (sword(), sword());
        let frame = timed_guard_frame(WeaponKind::Spatha, Swing::RightToLeft);
        assert_eq!(
            duel_full(
                &mut a,
                &mut d,
                pose(0.0, -1.2, PI),
                Some(Swing::RightToLeft),
                Some(frame)
            )
            .map(|r| r.0),
            Some(HitOutcome::Parried)
        );
        // Защитник отвечает через 0,2 с (человеческая реакция), отбитый жмёт ПКМ после отдачи.
        let ap = pose(0.0, -1.2, PI);
        let tp = pose(0.0, 0.0, 0.0);
        let mut result = None;
        for i in 0..90 {
            if i == 12 {
                assert!(d.try_attack(Some(Swing::LeftToRight)));
            }
            if i == 22 {
                a.set_guard(true);
            }
            a.tick(DT);
            d.tick(DT);
            if let Some((s, p)) = d.strike_progress() {
                let (h, t) = blade_segment(&ap, d.weapon, s, p);
                if let Some(c) = trace_blade(h, t, &ap, &a, &tp) {
                    result = Some(resolve_contact(&mut d, &mut a, c, 22.0));
                    break;
                }
            }
        }
        assert_eq!(
            result,
            Some(HitOutcome::Parried),
            "рипост не отбит: {:?}",
            a.action
        );
    }

    #[test]
    fn held_guard_blocks_but_gives_no_riposte() {
        let (mut a, mut d) = (sword(), sword());
        let out = duel(&mut a, &mut d, PI, Some(0));
        assert!(matches!(out, Some(HitOutcome::Blocked { .. })), "{out:?}");
        assert!(!d.can_riposte());
        // Удар атакующего не прерван — инициатива у него.
        assert!(matches!(a.action, Action::Strike { .. }));
        assert!(a.swing_landed);
        // Глухой блок стоит заметно больше сил, чем парирование.
        assert!(d.stamina < STAMINA_MAX - 10.0, "{}", d.stamina);
    }

    #[test]
    fn whiffed_parry_has_recovery() {
        let mut f = sword();
        f.set_guard(true);
        f.tick(DT);
        f.set_guard(false);
        // Короткое нажатие — окно доживает до конца.
        assert!(matches!(f.action, Action::Guard { .. }));
        for _ in 0..30 {
            f.tick(DT);
        }
        assert!(
            matches!(f.action, Action::ParryRecovery { .. }),
            "{:?}",
            f.action
        );
        assert!(!f.try_attack(None));
        for _ in 0..30 {
            f.tick(DT);
        }
        assert!(f.try_attack(None));
    }

    #[test]
    fn guard_from_behind_fails() {
        let (mut a, mut d) = (sword(), sword());
        let out = duel(&mut a, &mut d, 0.0, Some(0));
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
    }

    #[test]
    fn guard_too_late_takes_damage() {
        let (mut a, mut d) = (sword(), sword());
        let frame = contact_frame(WeaponKind::Spatha, Swing::RightToLeft) - 2;
        let out = duel_full(
            &mut a,
            &mut d,
            pose(0.0, -1.2, PI),
            Some(Swing::RightToLeft),
            Some(frame),
        )
        .map(|r| r.0);
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
    }

    /// Удар сверху не отбить, глядя в землю; глядя вперёд или вверх — можно.
    #[test]
    fn parry_needs_looking_at_the_attack() {
        let frame = timed_guard_frame(WeaponKind::Spatha, Swing::Overhead);
        for (pitch, parried) in [(0.0, true), (0.3, true), (-1.2, false)] {
            let (mut a, mut d) = (sword(), sword());
            let mut dp = pose(0.0, -1.2, PI);
            dp.pitch = pitch;
            let out =
                duel_full(&mut a, &mut d, dp, Some(Swing::Overhead), Some(frame)).map(|r| r.0);
            assert_eq!(
                out == Some(HitOutcome::Parried),
                parried,
                "наклон {pitch}: {out:?}"
            );
        }
        // Боком к удару (отвернулся на 80°) тоже не отбить.
        let (mut a, mut d) = (sword(), sword());
        let frame = timed_guard_frame(WeaponKind::Spatha, Swing::LeftToRight);
        let out = duel_full(
            &mut a,
            &mut d,
            pose(0.0, -1.2, PI + 1.4),
            Some(Swing::LeftToRight),
            Some(frame),
        )
        .map(|r| r.0);
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
    }

    #[test]
    fn feint_cancels_windup_for_stamina() {
        let mut f = sword();
        f.try_attack(Some(Swing::Overhead));
        f.tick(0.1);
        let before = f.stamina;
        assert!(f.feint());
        assert!(matches!(f.action, Action::Idle));
        assert!(f.stamina <= before - FEINT_STAMINA + 0.01);
        // Финт в парирование: ПКМ во время замаха.
        f.try_attack(None);
        f.tick(0.1);
        f.set_guard(true);
        assert!(matches!(f.action, Action::Guard { .. }));
        assert!(!f.feint());
    }

    #[test]
    fn morph_changes_attack_once() {
        let mut f = sword();
        f.try_attack(Some(Swing::RightToLeft));
        f.tick(0.15);
        assert!(f.try_attack(Some(Swing::Thrust)));
        assert!(matches!(
            f.action,
            Action::Windup {
                swing: Swing::Thrust,
                morphed: true,
                ..
            }
        ));
        // Второй раз — нельзя.
        assert!(!f.try_attack(Some(Swing::Overhead)));
        // В конце замаха — поздно.
        let mut g = sword();
        g.try_attack(Some(Swing::RightToLeft));
        g.tick(WeaponKind::Spatha.timing(Swing::RightToLeft).windup * 0.9);
        assert!(!g.try_attack(Some(Swing::Overhead)));
    }

    /// Зеркальный встречный удар в начале замаха отбивает чужой.
    #[test]
    fn chamber_beats_incoming_attack() {
        let mut a = sword();
        let mut d = sword();
        let ap = pose(0.0, 0.0, 0.0);
        let dp = pose(0.0, -1.2, PI);
        a.try_attack(Some(Swing::RightToLeft));
        let hit = contact_frame(WeaponKind::Spatha, Swing::RightToLeft);
        let mut out = None;
        for i in 0..120 {
            if i + 8 == hit {
                assert!(d.try_attack(Some(Swing::LeftToRight)));
            }
            a.tick(DT);
            d.tick(DT);
            if let Some((s, p)) = a.strike_progress() {
                let (h, t) = blade_segment(&ap, a.weapon, s, p);
                if let Some(c) = trace_blade(h, t, &ap, &d, &dp) {
                    out = Some(resolve_contact(&mut a, &mut d, c, 22.0));
                    break;
                }
            }
        }
        assert_eq!(out, Some(HitOutcome::Chambered));
        assert!(matches!(a.action, Action::Recoil { .. }));
        assert!(matches!(d.action, Action::Windup { .. }));
        // Не зеркальный удар — не чембер, а размен (атакующий успел первым).
        let mut a = sword();
        let mut d = sword();
        a.try_attack(Some(Swing::RightToLeft));
        let mut out = None;
        for i in 0..120 {
            if i + 8 == hit {
                d.try_attack(Some(Swing::RightToLeft));
            }
            a.tick(DT);
            d.tick(DT);
            if let Some((s, p)) = a.strike_progress() {
                let (h, t) = blade_segment(&ap, a.weapon, s, p);
                if let Some(c) = trace_blade(h, t, &ap, &d, &dp) {
                    out = Some(resolve_contact(&mut a, &mut d, c, 22.0));
                    break;
                }
            }
        }
        assert!(matches!(out, Some(HitOutcome::Damage { .. })), "{out:?}");
    }

    /// После попадания следующий удар идёт быстрее, промах стоит сил и полного восстановления.
    #[test]
    fn combo_after_hit_is_faster_than_after_whiff() {
        let time_to_next_strike = |landed: bool| {
            let mut f = sword();
            f.try_attack(None);
            let mut t = 0.0;
            let mut strikes = 0;
            let mut stamina_at_end = 0.0;
            for _ in 0..300 {
                if matches!(f.action, Action::Strike { .. }) {
                    if landed {
                        f.swing_landed = true;
                    }
                    f.try_attack(None);
                }
                if f.tick(DT) == Some(TickEvent::StrikeStarted) {
                    strikes += 1;
                    if strikes == 2 {
                        return (t, stamina_at_end);
                    }
                }
                if matches!(f.action, Action::Recovery { t: 0.0, .. }) {
                    stamina_at_end = f.stamina;
                }
                t += DT;
            }
            (t, stamina_at_end)
        };
        let (hit, s_hit) = time_to_next_strike(true);
        let (miss, s_miss) = time_to_next_strike(false);
        assert!(hit + 0.25 < miss, "комбо {hit} против {miss}");
        assert!(s_miss < s_hit, "промах не стоит сил: {s_miss} / {s_hit}");
    }

    #[test]
    fn mouse_picks_direction() {
        assert_eq!(
            Swing::from_mouse(Vec2::new(20.0, 0.0), 5.0),
            Some(Swing::RightToLeft)
        );
        assert_eq!(
            Swing::from_mouse(Vec2::new(-20.0, 1.0), 5.0),
            Some(Swing::LeftToRight)
        );
        assert_eq!(
            Swing::from_mouse(Vec2::new(0.0, 20.0), 5.0),
            Some(Swing::Overhead)
        );
        assert_eq!(
            Swing::from_mouse(Vec2::new(10.0, 10.0), 5.0),
            Some(Swing::UpperRight)
        );
        assert_eq!(
            Swing::from_mouse(Vec2::new(-10.0, -10.0), 5.0),
            Some(Swing::LowerLeft)
        );
        assert_eq!(
            Swing::from_mouse(Vec2::new(0.0, -20.0), 5.0),
            Some(Swing::Thrust)
        );
        assert_eq!(Swing::from_mouse(Vec2::new(1.0, 1.0), 5.0), None);
        for s in Swing::ALL {
            assert_eq!(s.mirror().mirror(), s);
        }
    }

    #[test]
    fn movesets_by_weapon() {
        assert_eq!(WeaponKind::Spear.normalize(Swing::Overhead), Swing::Thrust);
        assert_eq!(WeaponKind::Axe.normalize(Swing::Thrust), Swing::Overhead);
        assert_eq!(
            WeaponKind::Fists.normalize(Swing::UpperLeft),
            Swing::LeftToRight
        );
        // Топор сверху бьёт сильнее, чем сбоку, и дольше замахивается.
        let (side, over) = (
            WeaponKind::Axe.timing(Swing::RightToLeft),
            WeaponKind::Axe.timing(Swing::Overhead),
        );
        assert!(over.damage > side.damage && over.windup > side.windup);
        // Сакс колет лучше, чем рубит, спата — наоборот.
        assert!(WeaponKind::Seax.timing(Swing::Thrust).damage > 1.0);
        assert!(WeaponKind::Spatha.timing(Swing::Thrust).damage < 1.0);
        let mut f = Fighter::new(WeaponKind::Spear, 150.0, false);
        f.try_attack(Some(Swing::LeftToRight));
        assert!(matches!(
            f.action,
            Action::Windup {
                swing: Swing::Thrust,
                ..
            }
        ));
    }

    #[test]
    fn shield_blocks_by_impact() {
        // Меч против щита: слабый удар, атакующий отскакивает.
        let mut a = sword();
        let mut d = Fighter::new(WeaponKind::Spatha, 150.0, true);
        let out = duel(&mut a, &mut d, PI, Some(0));
        assert_eq!(
            out,
            Some(HitOutcome::Blocked {
                tier: ImpactTier::Weak
            })
        );
        assert!(matches!(a.action, Action::Recoil { .. }));
        assert!(!d.can_riposte());
        // Топор против щита: тяжёлый удар, атакующий продолжает серию.
        let mut a = Fighter::new(WeaponKind::Axe, 200.0, false);
        let mut d = Fighter::new(WeaponKind::Spatha, 150.0, true);
        let out = duel(&mut a, &mut d, PI, Some(0));
        assert_eq!(
            out,
            Some(HitOutcome::Blocked {
                tier: ImpactTier::Heavy
            })
        );
        assert!(matches!(a.action, Action::Strike { .. }));
        assert!(
            matches!(d.action, Action::GuardStagger { dur, .. } if dur == ImpactTier::Heavy.shield_stagger())
        );
    }

    #[test]
    fn combo_resets_after_pause() {
        let mut f = sword();
        let mut swings = Vec::new();
        // Третий удар после паузы начинает серию заново.
        for pause in [0.0, 0.0, 1.2] {
            for _ in 0..(pause * 60.0) as usize {
                f.tick(DT);
            }
            f.try_attack(None);
            if let Action::Windup { swing, .. } = f.action {
                swings.push(swing);
            }
            for _ in 0..(1.3 * 60.0) as usize {
                f.tick(DT);
            }
        }
        assert_eq!(
            swings,
            vec![Swing::RightToLeft, Swing::LeftToRight, Swing::RightToLeft]
        );
    }

    #[test]
    fn spear_thrust_reaches_far() {
        let mut a = Fighter::new(WeaponKind::Spear, 150.0, false);
        let d = sword();
        // Копейщик достаёт цель в 2,3 м, меч — нет.
        let ap = pose(0.0, 0.0, 0.0);
        let dp = pose(0.0, -2.3, PI);
        a.try_attack(None);
        let mut hit = false;
        for _ in 0..120 {
            a.tick(DT);
            if let Some((s, p)) = a.strike_progress() {
                assert_eq!(s, Swing::Thrust);
                let (h, t) = blade_segment(&ap, a.weapon, s, p);
                hit |= trace_blade(h, t, &ap, &d, &dp).is_some();
            }
        }
        assert!(hit, "копьё не достало");
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
        assert!(
            broken,
            "защиту так и не пробило, выносливость {}",
            d.stamina
        );
        // Без сил не замахнуться.
        let mut f = sword();
        f.stamina = 0.0;
        assert!(!f.try_attack(None));
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
        let mut arrow = Arrow {
            id: 1,
            owner: 1,
            pos,
            vel,
            damage: 30.0,
            life: 5.0,
            pierce: 0.0,
        };
        let targets = [(1, sp), (2, target)];
        let mut hit = None;
        for _ in 0..300 {
            match step_arrow(&mut arrow, DT, &level, &targets) {
                ArrowStep::Flying => {}
                ArrowStep::Hit {
                    index,
                    shield_facing,
                    head,
                    ..
                } => {
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
        assert_eq!(
            resolve_arrow(&mut d, shield_facing, head, 30.0),
            HitOutcome::Blocked {
                tier: ImpactTier::Weak
            }
        );
        // Оружием стрелу не отбить.
        let mut d = sword();
        d.set_guard(true);
        d.tick(0.5);
        assert!(matches!(
            resolve_arrow(&mut d, shield_facing, head, 30.0),
            HitOutcome::Damage { .. }
        ));
    }

    #[test]
    fn weapon_from_item_name() {
        assert_eq!(WeaponKind::from_item_name("Боевой топор"), WeaponKind::Axe);
        assert_eq!(WeaponKind::from_item_name("Кинжал"), WeaponKind::Seax);
        assert_eq!(
            WeaponKind::from_item_name("Пехотное копьё"),
            WeaponKind::Spear
        );
        assert_eq!(
            WeaponKind::from_item_name("Кавалерийская спата"),
            WeaponKind::Spatha
        );
    }

    #[test]
    fn armor_reduces_damage() {
        assert!((armor_reduction(150.0) - 0.5).abs() < 1e-6);
        assert_eq!(armor_reduction(0.0), 0.0);
    }

    #[test]
    fn fists_punch_lands_but_cannot_parry() {
        for swing in [Swing::Thrust, Swing::RightToLeft, Swing::LeftToRight] {
            let mut a = Fighter::new(WeaponKind::Fists, 150.0, false);
            let mut d = Fighter::new(WeaponKind::Spatha, 150.0, false);
            let out = duel_full(&mut a, &mut d, pose(0.0, -0.9, PI), Some(swing), None);
            assert!(
                matches!(out, Some((HitOutcome::Damage { .. }, _))),
                "{swing:?}: {out:?}"
            );
        }
        // Голой рукой клинок не отбить, а со щитом стойка есть.
        let mut bare = Fighter::new(WeaponKind::Fists, 150.0, false);
        bare.set_guard(true);
        assert!(!bare.guard_held);
        let mut shielded = Fighter::new(WeaponKind::Fists, 150.0, true);
        shielded.set_guard(true);
        assert!(shielded.guard_held);
    }
}
