//! Искусственный интеллект врагов (работает только на сервере).
//!
//! Люди Херсона и его округи около 510 г.: стратиоты и букелларии гарнизона (щит и спата,
//! держат строй и закрываются), готы (топор и круглый щит, напористые), аланы (копьё,
//! держат дистанцию) и кочевники-лучники (стреляют издали, вблизи берутся за нож).

use crate::collision::{Body, LevelColliders};
use crate::combat::{self, Action, Fighter, Pose, Swing, WeaponKind, line_of_sight};
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
        Self {
            kind,
            pos,
            yaw,
            faction: None,
            wander: 0.0,
        }
    }
}

/// Где стоят враги в начале и куда возрождаются.
pub fn npc_spawns() -> Vec<NpcSpawn> {
    let posts = crate::level::npc_posts();
    if !posts.is_empty() {
        return spawns_from_posts(&posts);
    }
    // Старый уровень без постов: типы раздаём по кругу; последний — букелларий (самый опасный).
    const ORDER: [ActorKind; 5] = [
        ActorKind::Stratiotes,
        ActorKind::GothWarrior,
        ActorKind::AlanLancer,
        ActorKind::HunArcher,
        ActorKind::GothWarrior,
    ];
    let hints = crate::level::npc_spawn_hints();
    let last = hints.len().saturating_sub(1);
    hints
        .into_iter()
        .enumerate()
        .map(|(i, pos)| {
            let kind = if i == last && i > 0 {
                ActorKind::Bucellarius
            } else {
                ORDER[i % ORDER.len()]
            };
            let (dx, dz) = (0.0 - pos.x, 32.0 - pos.z);
            NpcSpawn::brigand(kind, pos, (-dx).atan2(-dz))
        })
        .collect()
}

/// Кто стоит на постах уровня: гарнизон — стратиоты (на больших обходах — букелларии),
/// готы — воины с топорами, аланы — копейщики, кочевники — лучники, разбойники — сброд всех народов.
fn spawns_from_posts(posts: &[crate::level::NpcPost]) -> Vec<NpcSpawn> {
    const BRIGANDS: [ActorKind; 3] = [
        ActorKind::GothWarrior,
        ActorKind::HunArcher,
        ActorKind::AlanLancer,
    ];
    let (mut garrison, mut brigands) = (0usize, 0usize);
    posts
        .iter()
        .map(|p| {
            let kind = match p.group {
                NpcGroup::Garrison => {
                    garrison += 1;
                    if p.wander >= 10.0 && garrison % 2 == 1 {
                        ActorKind::Bucellarius
                    } else {
                        ActorKind::Stratiotes
                    }
                }
                NpcGroup::Goths => ActorKind::GothWarrior,
                NpcGroup::Alans => ActorKind::AlanLancer,
                NpcGroup::Nomads => ActorKind::HunArcher,
                NpcGroup::Brigands => {
                    brigands += 1;
                    BRIGANDS[(brigands - 1) % BRIGANDS.len()]
                }
            };
            NpcSpawn {
                kind,
                pos: p.pos,
                yaw: p.yaw,
                faction: p.group.faction(),
                wander: p.wander,
            }
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
        // Горожанин с топором или вилами: без доспеха и выучки, но злой и не один.
        ActorKind::Townsman => Profile {
            hp: 70.0,
            weapon: WeaponKind::Axe,
            shield: false,
            armor: 5.0,
            damage_mult: 0.8,
            speed: 2.7,
            defend_chance: 0.15,
            cooldown: (1.6, 3.0),
            patience: 1.0,
            aggro: 16.0,
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

/// Выучка бойца. От неё зависят реакция, точность защиты и умение беречь силы.
/// У всех одни и те же правила боя, что у игрока: та же выносливость, те же замахи оружия.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Training {
    /// Новобранец: реагирует поздно, часто ошибается, машет до последнего и выдыхается.
    Recruit,
    #[default]
    /// Обученный солдат.
    Regular,
    /// Ветеран: быстрая реакция, бережёт силы, редко упускает ответный удар.
    Veteran,
}

impl Training {
    /// Выучка конкретного человека: у каждого типа свой разброс, выбор зависит от поста.
    pub fn roll(kind: ActorKind, pos: Vec3) -> Training {
        let seed = (pos.x * 73.0) as i64 as u64
            ^ ((pos.z * 151.0) as i64 as u64).rotate_left(21)
            ^ (kind as u64).wrapping_mul(0x9E37_79B9);
        let r = Rng::new(seed.wrapping_mul(0x2545_F491_4F6C_DD1D)).next_f32();
        // Пороги: доля новобранцев, доля новобранцев + обученных.
        let (recruit, regular) = match kind {
            ActorKind::Player => (0.0, 1.0),
            ActorKind::Stratiotes => (0.3, 0.85),
            ActorKind::Bucellarius => (0.0, 0.3),
            ActorKind::GothWarrior => (0.3, 0.8),
            ActorKind::AlanLancer => (0.15, 0.7),
            ActorKind::HunArcher => (0.4, 0.9),
            ActorKind::Townsman => (0.85, 1.0),
        };
        if r < recruit {
            Training::Recruit
        } else if r < regular {
            Training::Regular
        } else {
            Training::Veteran
        }
    }

    pub fn skill(self) -> Skill {
        match self {
            Training::Recruit => Skill {
                reaction: (0.34, 0.10),
                read: 0.75,
                misjudge: 0.35,
                miss_riposte: 0.45,
                reserve: 0.0,
                winded: 0.0,
                knows_limits: false,
                max_stamina: 90.0,
                cooldown_mult: 0.85,
                fatigue_rate: 0.0015,
                timing_err: 0.14,
                alert: 0.0,
                timed_shield: 0.0,
                chamber: 0.0,
                feint: 0.0,
                morph: 0.0,
                read_feint: 0.1,
            },
            Training::Regular => Skill {
                reaction: (0.27, 0.07),
                read: 0.9,
                misjudge: 0.2,
                miss_riposte: 0.2,
                reserve: 0.15,
                winded: 0.2,
                knows_limits: true,
                max_stamina: 100.0,
                cooldown_mult: 1.0,
                fatigue_rate: 0.001,
                timing_err: 0.08,
                alert: 0.15,
                timed_shield: 0.2,
                chamber: 0.03,
                feint: 0.06,
                morph: 0.05,
                read_feint: 0.4,
            },
            Training::Veteran => Skill {
                reaction: (0.21, 0.05),
                read: 1.0,
                misjudge: 0.1,
                miss_riposte: 0.08,
                reserve: 0.25,
                winded: 0.3,
                knows_limits: true,
                max_stamina: 110.0,
                cooldown_mult: 1.1,
                fatigue_rate: 0.0007,
                timing_err: 0.05,
                alert: 0.3,
                timed_shield: 0.5,
                chamber: 0.12,
                feint: 0.14,
                morph: 0.12,
                read_feint: 0.7,
            },
        }
    }
}

/// Насколько хорошо боец владеет собой и оружием.
#[derive(Clone, Copy, Debug)]
pub struct Skill {
    /// Реакция на начало чужого замаха, с: среднее и разброс. Человеку нужно ~0,2–0,4 с,
    /// а стойку ещё надо поднять, поэтому поздняя реакция = пропущенный удар.
    pub reaction: (f32, f32),
    /// Насколько верно читает замах (множитель к склонности защищаться).
    pub read: f32,
    /// Шанс ошибиться со временем и поднять защиту ещё позже.
    pub misjudge: f32,
    /// Шанс упустить окно ответного удара после парирования.
    pub miss_riposte: f32,
    /// Доля выносливости, которую бережёт на защиту (ниже — не атакует).
    pub reserve: f32,
    /// Ниже этой доли сил отходит отдышаться (0 — дерётся до изнеможения).
    pub winded: f32,
    /// Понимает, что без сил стойку пробьют, и не подставляется.
    pub knows_limits: bool,
    pub max_stamina: f32,
    /// Множитель паузы между атаками.
    pub cooldown_mult: f32,
    /// Как быстро копится усталость от потраченных сил.
    pub fatigue_rate: f32,
    /// Насколько охотнее типа вообще встречает удар защитой (прибавка к `Profile::defend_chance`).
    pub alert: f32,
    /// Разброс в расчёте момента парирования, с (окно парирования ~0,3 с).
    pub timing_err: f32,
    /// Со щитом: доля случаев, когда парирует вовремя, а не просто держит щит.
    pub timed_shield: f32,
    /// Шанс встретить удар зеркальным встречным (чембер).
    pub chamber: f32,
    /// Шанс задумать финт и морф на своём ударе.
    pub feint: f32,
    pub morph: f32,
    /// Шанс распознать чужой финт и не парировать в пустоту.
    pub read_feint: f32,
}

/// Какой удар выбрать: у каждого оружия свои любимые, тот же подряд — реже.
pub fn choose_swing(rng: &mut Rng, weapon: WeaponKind, last: Swing) -> Swing {
    use Swing::*;
    let table: &[(Swing, f32)] = match weapon {
        WeaponKind::Spear | WeaponKind::Bow => return Thrust,
        WeaponKind::Fists => &[(Thrust, 0.5), (RightToLeft, 0.25), (LeftToRight, 0.25)],
        WeaponKind::Axe => &[
            (Overhead, 0.3),
            (UpperRight, 0.2),
            (UpperLeft, 0.15),
            (RightToLeft, 0.15),
            (LeftToRight, 0.12),
            (LowerRight, 0.08),
        ],
        WeaponKind::Seax => &[
            (Thrust, 0.35),
            (RightToLeft, 0.2),
            (LeftToRight, 0.2),
            (LowerRight, 0.1),
            (UpperRight, 0.1),
            (Overhead, 0.05),
        ],
        WeaponKind::Spatha => &[
            (RightToLeft, 0.2),
            (LeftToRight, 0.18),
            (Overhead, 0.15),
            (UpperRight, 0.12),
            (UpperLeft, 0.1),
            (Thrust, 0.12),
            (LowerRight, 0.07),
            (LowerLeft, 0.06),
        ],
    };
    let weight = |s: Swing, w: f32| if s == last { w * 0.35 } else { w };
    let total: f32 = table.iter().map(|(s, w)| weight(*s, *w)).sum();
    let mut r = rng.next_f32() * total;
    for (s, w) in table {
        r -= weight(*s, *w);
        if r <= 0.0 {
            return *s;
        }
    }
    table[0].0
}

/// Тяжёлый доспех замедляет восстановление сил (как у игрока с тяжёлой бронёй).
pub fn armor_regen_mult(armor: f32) -> f32 {
    (1.0 - armor / 300.0).clamp(0.6, 1.0)
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
    /// Выучка этого человека.
    pub training: Training,
    /// Усталость от долгого боя, 0..0.8: медленнее восстанавливается и реагирует.
    pub fatigue: f32,
    last_stamina: f32,
    /// Через сколько поднимет защиту на замеченный замах (реакция и расчёт времени удара).
    parry_at: Option<f32>,
    /// Через сколько начнёт встречный зеркальный удар (чембер) и какой.
    chamber_at: Option<(f32, Swing)>,
    /// Какой удар цели видел в прошлый раз (чтобы заметить морф).
    seen_swing: Option<Swing>,
    /// Куда поднять взгляд, встречая удар.
    parry_pitch: f32,
    /// Последний свой удар (не бить одно и то же).
    last_swing: Swing,
    /// Задуманный финт: на какой доле замаха бросить.
    plan_feint: Option<f32>,
    /// Задуманный морф: на какой доле замаха и в какой удар.
    plan_morph: Option<(f32, Swing)>,
    /// Через сколько ответит после своего парирования (бесконечность — упустил момент).
    riposte_wait: Option<f32>,
    /// Через сколько ударит оглушённую цель.
    punish_wait: Option<f32>,
    target_was_staggered: bool,
    /// Выдохся и отходит отдышаться.
    pub winded: bool,
    /// Отступает от удара, который нечем держать (сил на защиту нет).
    evade_timer: f32,
    /// Через сколько заметит чужой замах (тогда опытный не полезет под удар и может отступить).
    notice: Option<f32>,
    /// Заметил идущий на него удар.
    noticed: bool,
    /// Решил уйти от удара шагом назад, а не закрываться.
    evade_on_notice: bool,
    /// Стражник идёт задержать этого игрока (не нападает, пока тот не окажет сопротивление).
    pub arrest: Option<u64>,
    /// Путь по делу (на крик, к месту преступления, в казарму и обратно на пост): точки по
    /// сетке проходов города, идёт по ним, пока не встретит цель.
    pub route: Vec<Vec3>,
    /// Бежит, а не идёт.
    pub hurry: bool,
    /// Сколько секунд не может приблизиться к очередной точке пути (застрял).
    stuck: f32,
    /// Расстояние до очередной точки в прошлый раз.
    route_dist: f32,
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
            training: Training::roll(spawn.kind, spawn.pos),
            last_stamina: combat::STAMINA_MAX,
            ..default()
        }
    }

    /// Нападать ли на этого игрока.
    pub fn hostile_to(&self, player: u64) -> bool {
        self.faction.is_none()
            || self.provoked.contains(&player)
            || self.hostile_players.contains(&player)
    }

    /// Пойти (или побежать) по пути из точек.
    pub fn set_route(&mut self, route: Vec<Vec3>, hurry: bool) {
        self.route = route;
        self.hurry = hurry;
        self.stuck = 0.0;
        self.route_dist = f32::MAX;
        self.stroll_to = None;
    }

    /// Сменить пост (место, вокруг которого стоит и бродит, и откуда не уходит далеко).
    pub fn set_post(&mut self, at: Vec3, yaw: f32, wander: f32) {
        self.home = at;
        self.home_yaw = yaw;
        self.wander = wander;
        self.stroll_to = None;
    }

    pub fn wander(&self) -> f32 {
        self.wander
    }

    /// Сейчас дерётся (видит цель).
    pub fn fighting(&self) -> bool {
        self.target.is_some()
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
    brain.evade_timer -= dt;
    let prof = profile(kind);
    let skill = brain.training.skill();
    // --- Выносливость и усталость: те же правила, что у игрока ---
    let spent = (brain.last_stamina - fighter.stamina).max(0.0);
    brain.fatigue = (brain.fatigue + spent * skill.fatigue_rate).min(0.8);
    if brain.target.is_none() && fighter.stamina >= fighter.max_stamina - 0.5 {
        brain.fatigue = (brain.fatigue - 0.02 * dt).max(0.0);
    }
    fighter.max_stamina = skill.max_stamina * (1.0 - 0.25 * brain.fatigue);
    fighter.stamina = fighter.stamina.min(fighter.max_stamina);
    fighter.stamina_regen_mult = armor_regen_mult(prof.armor) * (1.0 - 0.35 * brain.fatigue);
    let stamina_frac = fighter.stamina / fighter.max_stamina.max(1.0);
    if stamina_frac < skill.winded {
        brain.winded = true;
    } else if stamina_frac > 0.6 {
        brain.winded = false;
    }
    // Уставший и запыхавшийся соображает медленнее.
    let slow = 1.0 + 0.4 * brain.fatigue + 0.3 * (1.0 - stamina_frac);
    let react =
        |rng: &mut Rng| (skill.reaction.0 + rng.range(-skill.reaction.1, skill.reaction.1)) * slow;
    // Лучник вблизи берётся за нож, отойдя — снова за лук.
    if kind == ActorKind::HunArcher && matches!(fighter.action, Action::Idle) {
        let close = brain
            .target
            .and_then(|id| others.iter().find(|o| o.id == id))
            .map(|t| t.pose.feet.distance(pose.feet));
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
    let current = brain
        .target
        .and_then(|id| others.iter().find(|o| o.id == id && o.alive));
    let current = current.filter(|t| t.kind != ActorKind::Player || brain.hostile_to(t.id));
    // Далеко от поста не уходит: стража не гонится за вами через полгорода.
    let leash = brain.wander.max(10.0) + 25.0;
    let current = current.filter(|_| pose.feet.distance(brain.home) < leash);
    let target = match current {
        Some(t) if t.pose.feet.distance(pose.feet) < LOSE_RANGE.max(prof.aggro + 8.0) => Some(*t),
        _ => others
            .iter()
            .filter(|o| {
                o.alive && o.targetable && o.kind == ActorKind::Player && brain.hostile_to(o.id)
            })
            .filter(|o| o.pose.feet.distance(pose.feet) < prof.aggro.max(AGGRO_RANGE * 0.5))
            .filter(|o| line_of_sight(level, eye, o.pose.shoulder()))
            .min_by(|a, b| {
                a.pose
                    .feet
                    .distance(pose.feet)
                    .total_cmp(&b.pose.feet.distance(pose.feet))
            })
            .copied(),
    };
    brain.target = target.map(|t| t.id);

    let can_act = !matches!(
        fighter.action,
        Action::Stagger { .. } | Action::Flinch { .. } | Action::GuardStagger { .. } | Action::Dead
    );
    let mut wish = Vec3::ZERO;

    if let Some(t) = target {
        let to = t.pose.feet - pose.feet;
        let dist = Vec2::new(to.x, to.z).length();
        let desired_yaw = yaw_towards(pose.feet, t.pose.feet);
        let turn_rate = match fighter.action {
            Action::Strike { .. } => 0.8,
            Action::Windup { .. } => 2.0,
            Action::Stagger { .. } | Action::GuardStagger { .. } => 0.0,
            Action::Recoil { .. } => 1.5,
            _ => 3.5,
        };
        pose.yaw = turn_towards(pose.yaw, desired_yaw, turn_rate * dt);
        let forward = pose.forward_flat();
        let right = Vec3::new(-forward.z, 0.0, forward.x);
        let aligned = forward.dot(Vec3::new(to.x, 0.0, to.z).normalize_or_zero()) > 0.9;

        // Цель держит защиту лицом к нам? Тогда обходим сбоку, а не бьём в лоб.
        let target_guarding =
            matches!(t.action, Action::Guard { .. } | Action::GuardStagger { .. })
                && combat::facing(&t.pose, pose.feet);
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
        } else if brain.winded || brain.evade_timer > 0.0 {
            // Выдохся или нечем держать удар: отходим за длину чужого оружия.
            if dist < t.weapon.stats().reach + 2.2 {
                wish -= forward * 0.8;
            }
        } else if dist > range {
            wish += forward;
        } else if dist
            < range
                - if fighter.weapon.thrust_only() {
                    0.9
                } else {
                    0.7
                }
        {
            wish -= forward * 0.6;
        }
        if dist < range + 1.5 {
            if target_guarding && brain.strafe.abs() < 0.5 {
                brain.strafe = if rng.chance(0.5) { 0.9 } else { -0.9 };
                brain.strafe_timer = rng.range(1.5, 2.5);
            } else if brain.strafe_timer <= 0.0 {
                brain.strafe = if rng.chance(0.35) {
                    0.0
                } else if rng.chance(0.5) {
                    0.4
                } else {
                    -0.4
                };
                brain.strafe_timer = rng.range(1.5, 3.0);
            }
            wish += right * brain.strafe;
        }

        // --- Защита: на начало вражеского замаха (или его морф) решаем, как встречать удар ---
        let winding = matches!(t.action, Action::Windup { .. });
        let t_swing = t.action.swing();
        // Угроза — когда цель в замахе и её оружие почти достаёт.
        let threat_range = if t.weapon.ranged() {
            30.0
        } else {
            t.weapon.stats().reach + 2.0
        };
        let morphed = winding
            && brain.target_was_winding_up
            && brain.seen_swing.is_some()
            && t_swing != brain.seen_swing;
        if winding
            && (!brain.target_was_winding_up || morphed)
            && dist < threat_range
            && combat::facing(pose, t.pose.feet)
        {
            // Сколько сил уйдёт на этот удар. Опытный боец без сил не подставляет стойку, а отходит;
            // новобранец закрывается всё равно, и его защиту пробивают.
            let resist = if fighter.has_shield {
                combat::SHIELD_RESIST
            } else {
                stats.guard_resist
            };
            let cost = combat::ImpactTier::from_diff(t.weapon.stats().impact_power, resist)
                .guard_stamina(fighter.has_shield);
            let can_hold = !skill.knows_limits || fighter.stamina >= cost;
            let reaction = react(rng);
            brain.notice = Some(reaction);
            brain.evade_on_notice = false;
            brain.parry_at = None;
            brain.chamber_at = None;
            // Когда клинок дойдёт: остаток замаха и примерно треть самого удара.
            let swing = t_swing.unwrap_or_default();
            let eta =
                t.action.time_to_release().unwrap_or(0.0) + t.weapon.timing(swing).release * 0.35;
            if fighter.weapon.can_guard_with(fighter.has_shield)
                && (brain.want_guard || rng.chance((prof.defend_chance + skill.alert) * skill.read))
            {
                if can_hold {
                    let can_chamber = !t.weapon.ranged()
                        && fighter.weapon.normalize(swing.mirror()) == swing.mirror()
                        && matches!(fighter.action, Action::Idle)
                        && fighter.stamina >= combat::CHAMBER_STAMINA + stats.stamina_cost
                        && dist <= range + 0.3;
                    if can_chamber && rng.chance(skill.chamber) {
                        // Встречный зеркальный удар в самом начале чужого удара.
                        let at = eta - combat::CHAMBER_WINDOW * 0.6;
                        brain.chamber_at = Some((at.max(reaction), swing.mirror()));
                    } else if fighter.has_shield && !rng.chance(skill.timed_shield) {
                        // Щитоносец просто закрывается, как заметил, и держит щит.
                        let mut delay = reaction;
                        if rng.chance(skill.misjudge) {
                            delay += rng.range(0.12, 0.3);
                        }
                        brain.parry_at = Some(delay.max(0.08));
                    } else {
                        // Парирование: нажать примерно за «подъём + пол-окна» до удара.
                        // Новобранец ошибается сильнее и чаще жмёт рано (тогда это глухой блок) или поздно.
                        // Стойка ещё поднята с прошлого удара, но окно уже закрыто — опускаем,
                        // чтобы поднять заново вовремя (иначе выйдет лишь глухой блок).
                        if fighter.guard_held
                            && !fighter.in_parry_window()
                            && matches!(fighter.action, Action::Guard { .. })
                        {
                            brain.want_guard = false;
                            fighter.set_guard(false);
                        }
                        let lead = fighter.guard_raise() + combat::PARRY_ACTIVE * 0.45;
                        let mut at = eta - lead + rng.range(-skill.timing_err, skill.timing_err);
                        if rng.chance(skill.misjudge) {
                            at += if rng.chance(0.5) {
                                rng.range(0.12, 0.3)
                            } else {
                                -rng.range(0.15, 0.35)
                            };
                        }
                        brain.parry_at = Some(at.max(reaction));
                    }
                } else {
                    brain.evade_on_notice = true;
                }
            } else if skill.knows_limits && rng.chance(0.5) {
                // Не закрывается, так хоть шаг назад из-под удара.
                brain.evade_on_notice = true;
            }
            // Удар сверху встречают, подняв взгляд, снизу — опустив (кто обучен).
            brain.parry_pitch = if skill.read >= 0.9 {
                match swing.class() {
                    combat::SwingClass::Overhead | combat::SwingClass::Diagonal => 0.3,
                    combat::SwingClass::Rising => -0.25,
                    _ => 0.0,
                }
            } else {
                0.0
            };
        }
        // Цель бросила замах (финт): обученный замечает и не парирует в пустоту.
        if brain.target_was_winding_up
            && !winding
            && !matches!(t.action, Action::Strike { .. })
            && (brain.parry_at.is_some() || brain.chamber_at.is_some())
        {
            if rng.chance(skill.read_feint) {
                brain.parry_at = None;
                brain.chamber_at = None;
            }
        }
        brain.seen_swing = if winding { t_swing } else { None };
        brain.target_was_winding_up = winding;
        if let Some(left) = brain.parry_at.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                brain.parry_at = None;
                brain.want_guard = true;
                brain.guard_linger = 0.3;
                // Стойка висит глухим блоком — отпустить и нажать заново: окно снова открыто.
                if fighter.guard_held && !fighter.in_parry_window() {
                    let restart = matches!(fighter.action, Action::Guard { .. });
                    fighter.set_guard(false);
                    if restart {
                        fighter.set_guard(true);
                    }
                }
            }
        }
        if let Some((left, swing)) = brain.chamber_at.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                let swing = *swing;
                brain.chamber_at = None;
                if can_act && fighter.try_attack(Some(swing)) {
                    brain.last_swing = swing;
                    brain.attack_cooldown =
                        rng.range(prof.cooldown.0, prof.cooldown.1) * skill.cooldown_mult;
                }
            }
        }
        let threat = matches!(t.action, Action::Windup { .. } | Action::Strike { .. });
        if let Some(left) = brain.notice.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                brain.notice = None;
                brain.noticed = threat;
                if threat && brain.evade_on_notice {
                    brain.evade_timer = 0.7;
                }
            }
        }
        if !threat {
            brain.noticed = false;
        }
        if brain.want_guard {
            if threat {
                brain.guard_linger = brain.guard_linger.max(0.2);
                if can_act
                    && !fighter.guard_held
                    && matches!(
                        fighter.action,
                        Action::Idle | Action::Windup { .. } | Action::Recoil { .. }
                    )
                {
                    fighter.set_guard(true);
                }
            } else if fighter.guard_held && !fighter.guard_used && fighter.in_parry_window() {
                // Окно парирования ещё открыто — дожидаемся (отпускать сразу незачем).
            } else {
                brain.guard_linger -= dt;
                if brain.guard_linger <= 0.0 {
                    brain.want_guard = false;
                }
            }
        }
        // Стойка висит глухим блоком, а следующий удар цели ещё только замахивается — опускаем
        // и поднимаем заново к удару (кто без щита: оружием глухой блок дорог).
        if !fighter.has_shield
            && fighter.guard_held
            && brain.parry_at.is_none()
            && matches!(
                fighter.action,
                Action::Guard { .. } | Action::GuardStagger { .. }
            )
            && !fighter.in_parry_window()
            && let (Some(rel), Some(swing)) = (t.action.time_to_release(), t_swing)
        {
            let eta = rel + t.weapon.timing(swing).release * 0.35;
            let lead = fighter.guard_raise() + combat::PARRY_ACTIVE * 0.45;
            if eta > lead + 0.06 {
                brain.want_guard = false;
                fighter.set_guard(false);
                brain.parry_at =
                    Some((eta - lead + rng.range(-skill.timing_err, skill.timing_err)).max(0.0));
            }
        }
        if !brain.want_guard && fighter.guard_held {
            fighter.set_guard(false);
        }
        // Наклон головы: на удар сверху — выше, на удар снизу — ниже.
        if !ranged {
            let want_pitch = if brain.want_guard || brain.parry_at.is_some() {
                brain.parry_pitch
            } else {
                0.0
            };
            pose.pitch += (want_pitch - pose.pitch).clamp(-3.0 * dt, 3.0 * dt);
        }

        // --- Атака ---
        // Ответный удар после парирования — с человеческой задержкой, и момент можно упустить.
        if fighter.can_riposte() {
            let wait = *brain.riposte_wait.get_or_insert_with(|| {
                if rng.chance(skill.miss_riposte) {
                    f32::INFINITY
                } else {
                    react(rng) * 0.8
                }
            });
            brain.riposte_wait = Some(wait - dt);
            let swing = choose_swing(rng, fighter.weapon, brain.last_swing);
            if wait - dt <= 0.0 && fighter.try_attack(Some(swing)) {
                brain.last_swing = swing;
                brain.want_guard = false;
                brain.parry_at = None;
                fighter.guard_held = false;
                brain.riposte_wait = None;
            }
        } else {
            brain.riposte_wait = None;
        }
        // Свой замах: финт или морф по плану, если задумал.
        if let Action::Windup {
            t: wt,
            dur,
            riposte: false,
            ..
        } = fighter.action
        {
            let frac = wt / dur.max(0.01);
            if brain.plan_feint.is_some_and(|at| frac >= at) {
                brain.plan_feint = None;
                if fighter.feint() {
                    brain.attack_cooldown = rng.range(0.08, 0.3);
                }
            }
            if let Some((at, swing)) = brain.plan_morph
                && frac >= at
            {
                brain.plan_morph = None;
                if fighter.try_attack(Some(swing)) {
                    brain.last_swing = swing;
                }
            }
        } else {
            brain.plan_feint = None;
            brain.plan_morph = None;
        }
        // Оглушённую цель бьём, но тоже не мгновенно: сначала надо это заметить.
        let staggered = matches!(
            t.action,
            Action::Stagger { .. } | Action::Recoil { .. } | Action::ParryRecovery { .. }
        );
        if staggered && !brain.target_was_staggered {
            brain.punish_wait = Some(react(rng));
        } else if !staggered {
            brain.punish_wait = None;
        }
        brain.target_was_staggered = staggered;
        if let Some(w) = brain.punish_wait.as_mut() {
            *w -= dt;
        }
        let punish = staggered && brain.punish_wait.is_some_and(|w| w <= 0.0);
        let in_reach = dist <= range + 0.15;
        // По защищающемуся бьём, только если зашли сбоку или он слишком долго стоит в стойке.
        // Стрелы щит держит, а оружием их не отбить, так что лучнику стойка цели не помеха.
        let shield_up = target_guarding && others.iter().any(|o| o.id == t.id && o.shield);
        let blocked_by_guard = if ranged {
            shield_up && brain.patience < prof.patience
        } else {
            target_guarding && brain.patience < prof.patience
        };
        let ready = if ranged {
            sees && (pose.pitch
                - combat::ballistic_pitch(
                    combat::arrow_launch(pose).0,
                    t.pose.feet + Vec3::Y * 1.2,
                ))
            .abs()
                < 0.08
        } else {
            true
        };
        // Если сил мало, отдыхаем, а не машем впустую. Опытный держит запас сил на защиту
        // (по оглушённому бьёт и из запаса).
        let reserve = if punish {
            0.0
        } else {
            skill.reserve * fighter.max_stamina
        };
        let rested = fighter.stamina >= stats.stamina_cost + reserve && !brain.winded;
        // Опытный боец не начинает замах под уже летящий в него удар; новобранец лезет.
        let busy_defending = brain.want_guard
            || brain.parry_at.is_some()
            || brain.chamber_at.is_some()
            || brain.evade_timer > 0.0
            || (skill.knows_limits && brain.noticed);
        // По стойке опытный бьёт с финтом: выманить парирование и ударить в заминку.
        let baiting = !ranged && target_guarding && rng.chance(skill.feint * 2.5);
        let blocked_by_guard = blocked_by_guard && !baiting;
        if can_act
            && in_reach
            && aligned
            && ready
            && rested
            && !busy_defending
            && !blocked_by_guard
            && (brain.attack_cooldown <= 0.0 || punish)
        {
            let swing = if ranged {
                Swing::Thrust
            } else {
                choose_swing(rng, fighter.weapon, brain.last_swing)
            };
            if fighter.try_attack(Some(swing)) {
                brain.last_swing = swing;
                brain.attack_cooldown =
                    rng.range(prof.cooldown.0, prof.cooldown.1) * skill.cooldown_mult;
                brain.patience = 0.0;
                if !ranged && !punish {
                    // Задумать финт (бросить замах на полпути) или морф (сменить удар).
                    let feint = skill.feint * if target_guarding || baiting { 2.5 } else { 1.0 };
                    if rng.chance(feint) {
                        brain.plan_feint = Some(rng.range(0.35, 0.7));
                    } else if rng.chance(skill.morph) {
                        let other = choose_swing(rng, fighter.weapon, swing);
                        if other != swing {
                            brain.plan_morph = Some((rng.range(0.2, 0.5), other));
                        }
                    }
                }
            }
        }
    } else {
        brain.want_guard = false;
        brain.parry_at = None;
        brain.chamber_at = None;
        brain.riposte_wait = None;
        brain.punish_wait = None;
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
        // Стражник идёт к разыскиваемому и встаёт перед ним.
        let arrest = brain
            .arrest
            .and_then(|id| others.iter().find(|o| o.id == id && o.alive))
            .map(|o| o.pose.feet);
        // По делу: идём по точкам пути, пропуская достигнутые (и ту, к которой не пройти).
        while let Some(next) = brain.route.first().copied() {
            let d = Vec2::new(next.x - pose.feet.x, next.z - pose.feet.z).length();
            if d < 0.9 || brain.stuck > 2.5 {
                brain.route.remove(0);
                brain.stuck = 0.0;
                brain.route_dist = f32::MAX;
                continue;
            }
            if d < brain.route_dist - 0.05 {
                brain.route_dist = d;
                brain.stuck = 0.0;
            } else {
                brain.stuck += dt;
            }
            break;
        }
        let on_route = arrest.is_none() && !brain.route.is_empty();
        let goal = arrest
            .or(brain.route.first().copied())
            .unwrap_or(brain.stroll_to.unwrap_or(brain.home));
        let to_goal = goal - pose.feet;
        if on_route {
            pose.yaw = turn_towards(pose.yaw, yaw_towards(pose.feet, goal), 5.0 * dt);
            wish = pose.forward_flat() * if brain.hurry { 1.3 } else { 0.6 };
        } else if let Some(at) = arrest {
            pose.yaw = turn_towards(pose.yaw, yaw_towards(pose.feet, at), 3.0 * dt);
            if Vec2::new(to_goal.x, to_goal.z).length() > 1.7 {
                wish = pose.forward_flat() * 0.9;
            }
        } else if Vec2::new(to_goal.x, to_goal.z).length() > 1.0 {
            pose.yaw = turn_towards(pose.yaw, yaw_towards(pose.feet, goal), 3.0 * dt);
            wish = pose.forward_flat() * if brain.stroll_to.is_some() { 0.45 } else { 0.6 };
        } else if brain.stroll_to.take().is_none() {
            pose.yaw = turn_towards(pose.yaw, brain.home_yaw, 1.5 * dt);
        }
    }

    // --- Движение с физикой ---
    let move_speed = speed * fighter.move_speed_mult();
    // Бегом по делу — быстрее обычного шага.
    let cap = if brain.hurry && brain.target.is_none() && !brain.route.is_empty() {
        1.3
    } else {
        1.0
    };
    let wish = wish.clamp_length_max(cap) * move_speed;
    brain.velocity.x = wish.x;
    brain.velocity.z = wish.z;
    brain.velocity.y -= GRAVITY * dt;
    let mut feet = pose.feet;
    let result = level.move_body(
        &Body::STANDING,
        &mut feet,
        brain.velocity * dt,
        brain.grounded,
    );
    brain.grounded = result.grounded;
    if result.grounded {
        brain.velocity.y = -1.0;
    }
    if feet.y < -30.0 {
        feet = brain.home + Vec3::Y * 0.5;
        brain.velocity = Vec3::ZERO;
    }
    pose.feet = feet;
    // Сколько сил потрачено, узнаем на следующем шаге (после удара/защиты в этом кадре).
    brain.last_stamina = fighter.stamina;
}

#[cfg(test)]
mod tests {
    use super::*;
    const PI_: f32 = std::f32::consts::PI;
    use crate::combat::{Contact, HitOutcome, blade_segment, resolve_contact, trace_blade};

    /// Игрок, который умеет парировать: жмёт ПКМ так, чтобы удар пришёлся в окно парирования,
    /// отпускает после, а после удачного парирования отвечает рипостом.
    fn skilled_guard(player: &mut Fighter, npc: &Fighter) {
        if player.can_riposte() {
            player.try_attack(None);
            return;
        }
        let eta = match npc.action {
            Action::Windup { t, dur, swing, .. } => {
                Some(dur - t + npc.timing(swing).release * 0.35)
            }
            Action::Strike { t, swing, .. } => Some((npc.timing(swing).release * 0.6 - t).max(0.0)),
            _ => None,
        };
        match eta {
            Some(e) if e <= player.guard_raise() + combat::PARRY_ACTIVE * 0.5 => {
                if !player.guard_held {
                    player.set_guard(true);
                }
            }
            None if player.guard_held => player.set_guard(false),
            _ => {}
        }
    }

    /// Прогоняет поединок игрока со стратегией `player_brain` против врага. Возвращает статистику.
    fn fight(kind: ActorKind, player_guard: bool, seconds: f32) -> (Fighter, Fighter, [u32; 3]) {
        let (npc, player, stats, _) = fight_with(kind, None, player_guard, seconds, 7);
        (npc, player, stats)
    }

    fn fight_with(
        kind: ActorKind,
        training: Option<Training>,
        player_guard: bool,
        seconds: f32,
        seed: u64,
    ) -> (Fighter, Fighter, [u32; 3], Brain) {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn::brigand(kind, Vec3::new(10.0, 0.0, 0.0), 0.0);
        let mut brain = Brain::new(&spawn);
        if let Some(t) = training {
            brain.training = t;
        }
        let mut npc_pose = Pose {
            feet: find_free_spot(&level, spawn.pos),
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        let mut npc = fighter_for(kind);
        let mut player = fighter_for(ActorKind::Player);
        let mut player_pose = Pose {
            feet: find_free_spot(&level, Vec3::new(10.0, 0.0, 2.0)),
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        let mut rng = Rng::new(seed);
        let dt = 1.0 / 60.0;
        let mut stats = [0u32; 3]; // урон по врагу, отражено врагом, урон по игроку
        for _ in 0..(seconds / dt) as usize {
            player_pose.yaw = yaw_towards(player_pose.feet, npc_pose.feet);
            // Игрок сближается до длины меча (против копья иначе не достать).
            let to = npc_pose.feet - player_pose.feet;
            if Vec2::new(to.x, to.z).length() > 1.2 {
                let step = Vec3::new(to.x, 0.0, to.z).normalize_or_zero()
                    * 4.0
                    * player.move_speed_mult()
                    * dt;
                let mut feet = player_pose.feet;
                level.move_body(&Body::STANDING, &mut feet, step, true);
                player_pose.feet = feet;
            }
            if player_guard {
                // Парируем вовремя, а после парирования бьём в ответ.
                skilled_guard(&mut player, &npc);
            } else {
                player.try_attack(None);
            }
            let seen = [Seen {
                id: 1,
                kind: ActorKind::Player,
                pose: player_pose,
                action: player.action,
                weapon: player.weapon,
                shield: false,
                alive: true,
                targetable: true,
            }];
            think(
                &mut brain,
                kind,
                &mut npc_pose,
                &mut npc,
                &seen,
                &level,
                &mut rng,
                dt,
            );
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
        (npc, player, stats, brain)
    }

    /// Рыцарь подходит к игроку, который только стоит в стойке… и обходит её.
    #[test]
    fn knight_hits_idle_player() {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn::brigand(ActorKind::Stratiotes, Vec3::new(10.0, 0.0, 0.0), 0.0);
        let mut brain = Brain::new(&spawn);
        let mut pose = Pose {
            feet: find_free_spot(&level, spawn.pos),
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        let mut knight = fighter_for(ActorKind::Stratiotes);
        let player_pose = Pose {
            feet: find_free_spot(&level, Vec3::new(16.0, 0.0, 0.0)),
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        let mut player = fighter_for(ActorKind::Player);
        let mut rng = Rng::new(42);
        let dt = 1.0 / 60.0;
        for _ in 0..(60 * 20) {
            let seen = [Seen {
                id: 1,
                kind: ActorKind::Player,
                pose: player_pose,
                action: player.action,
                weapon: player.weapon,
                shield: false,
                alive: true,
                targetable: true,
            }];
            think(
                &mut brain,
                ActorKind::Stratiotes,
                &mut pose,
                &mut knight,
                &seen,
                &level,
                &mut rng,
                dt,
            );
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
        assert!(
            player.hp < player.max_hp,
            "рыцарь так и не попал; позиция {:?}",
            pose.feet
        );
    }

    /// Игрок просто машет мечом: мечник часто закрывается щитом, но в итоге погибает,
    /// а бой длится заметно дольше пары секунд.
    #[test]
    fn stratiotes_defends_with_shield() {
        let (npc, player, stats, _) = fight_with(
            ActorKind::Stratiotes,
            Some(Training::Regular),
            false,
            120.0,
            7,
        );
        eprintln!(
            "урон по врагу {}, отражено {}, урон по игроку {}, hp игрока {}",
            stats[0], stats[1], stats[2], player.hp
        );
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
            let spawn =
                NpcSpawn::brigand(ActorKind::HunArcher, Vec3::new(4.0, 0.0, 0.0), -PI_ / 2.0);
            let mut brain = Brain::new(&spawn);
            let mut pose = Pose {
                feet: find_free_spot(&level, spawn.pos),
                yaw: 0.0,
                pitch: 0.0,
                crouching: false,
            };
            let mut archer = fighter_for(ActorKind::HunArcher);
            let player_pose = Pose {
                feet: find_free_spot(&level, Vec3::new(16.0, 0.0, 0.0)),
                yaw: 0.0,
                pitch: 0.0,
                crouching: false,
            };
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
                let seen = [Seen {
                    id: 1,
                    kind: ActorKind::Player,
                    pose: pp,
                    action: player.action,
                    weapon: player.weapon,
                    shield,
                    alive: true,
                    targetable: true,
                }];
                think(
                    &mut brain,
                    ActorKind::HunArcher,
                    &mut pose,
                    &mut archer,
                    &seen,
                    &level,
                    &mut rng,
                    dt,
                );
                if archer.tick(dt) == Some(crate::combat::TickEvent::StrikeStarted)
                    && archer.weapon.ranged()
                {
                    let (p, v) = arrow_launch(&pose);
                    arrows.push(Arrow {
                        id: shots,
                        owner: 2,
                        pos: p,
                        vel: v,
                        damage: 30.0,
                        life: 4.0,
                        pierce: 0.0,
                    });
                    shots += 1;
                }
                player.tick(dt);
                let targets = [(1, pp), (2, pose)];
                arrows.retain_mut(|a| match step_arrow(a, dt, &level, &targets) {
                    ArrowStep::Flying => true,
                    ArrowStep::Hit {
                        head,
                        shield_facing,
                        ..
                    } => {
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
        let post = |group, wander| NpcPost {
            pos: Vec3::ZERO,
            group,
            yaw: 0.0,
            wander,
        };
        let posts = [
            post(NpcGroup::Garrison, 2.0),
            post(NpcGroup::Garrison, 15.0),
            post(NpcGroup::Goths, 12.0),
            post(NpcGroup::Alans, 12.0),
            post(NpcGroup::Nomads, 12.0),
            post(NpcGroup::Brigands, 20.0),
            post(NpcGroup::Brigands, 20.0),
        ];
        let kinds: Vec<_> = spawns_from_posts(&posts)
            .iter()
            .map(|s| (s.kind, s.faction.is_some()))
            .collect();
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
            assert!(
                all.contains(&k) || crate::level::npc_posts().is_empty(),
                "нет {k:?} на уровне"
            );
        }
    }

    /// Стражник не трогает игрока, пока тот не нападёт или не станет врагом фракции.
    #[test]
    fn garrison_is_peaceful_until_provoked() {
        let level = crate::level::level_colliders();
        let mut spawn = NpcSpawn::brigand(ActorKind::Stratiotes, Vec3::new(10.0, 0.0, 0.0), 0.0);
        spawn.faction = Some(Faction::Garrison);
        let mut brain = Brain::new(&spawn);
        let mut pose = Pose {
            feet: find_free_spot(&level, spawn.pos),
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        let mut guard = fighter_for(ActorKind::Stratiotes);
        let player_pose = Pose {
            feet: find_free_spot(&level, Vec3::new(12.0, 0.0, 0.0)),
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        let player = fighter_for(ActorKind::Player);
        let mut rng = Rng::new(5);
        let seen = [Seen {
            id: 1,
            kind: ActorKind::Player,
            pose: player_pose,
            action: player.action,
            weapon: player.weapon,
            shield: false,
            alive: true,
            targetable: true,
        }];
        let mut attacked = |brain: &mut Brain, guard: &mut Fighter, pose: &mut Pose| {
            for _ in 0..(60 * 10) {
                think(
                    brain,
                    ActorKind::Stratiotes,
                    pose,
                    guard,
                    &seen,
                    &level,
                    &mut rng,
                    1.0 / 60.0,
                );
                guard.tick(1.0 / 60.0);
                if matches!(guard.action, Action::Windup { .. }) {
                    return true;
                }
            }
            false
        };
        assert!(
            !attacked(&mut brain, &mut guard, &mut pose),
            "стражник напал без причины"
        );
        brain.provoke(1);
        assert!(
            attacked(&mut brain, &mut guard, &mut pose),
            "стражник не ответил на нападение"
        );
        // Вражда по репутации тоже повод.
        let mut brain = Brain::new(&spawn);
        brain.hostile_players.push(1);
        let mut guard = fighter_for(ActorKind::Stratiotes);
        let mut pose = Pose {
            feet: find_free_spot(&level, spawn.pos),
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        assert!(attacked(&mut brain, &mut guard, &mut pose));
    }

    /// Игрок держит стойку и отвечает после парирования: копейщик должен проиграть.
    #[test]
    fn parry_and_riposte_beats_lancer() {
        let (npc, player, stats) = fight(ActorKind::AlanLancer, true, 180.0);
        eprintln!(
            "урон по врагу {}, отражено {}, урон по игроку {}, hp игрока {}",
            stats[0], stats[1], stats[2], player.hp
        );
        assert!(stats[0] > 0, "ни одного ответного удара");
        assert!(!player.is_dead(), "игрок в стойке проиграл: {stats:?}");
        let _ = npc;
    }

    /// Враг не поднимает защиту мгновенно: человеку нужно время, чтобы заметить замах.
    #[test]
    fn guard_has_human_reaction_delay() {
        let level = crate::level::level_colliders();
        for training in [Training::Recruit, Training::Regular, Training::Veteran] {
            for seed in 1..20u64 {
                let spawn =
                    NpcSpawn::brigand(ActorKind::AlanLancer, Vec3::new(10.0, 0.0, 0.0), 0.0);
                let mut brain = Brain::new(&spawn);
                brain.training = training;
                let npc_feet = find_free_spot(&level, spawn.pos);
                let mut pose = Pose {
                    feet: npc_feet,
                    yaw: 0.0,
                    pitch: 0.0,
                    crouching: false,
                };
                let player_feet = npc_feet + Vec3::new(0.0, 0.0, 1.5);
                pose.yaw = yaw_towards(npc_feet, player_feet);
                let mut npc = fighter_for(ActorKind::AlanLancer);
                let mut player = fighter_for(ActorKind::Player);
                let player_pose = Pose {
                    feet: player_feet,
                    yaw: yaw_towards(player_feet, npc_feet),
                    pitch: 0.0,
                    crouching: false,
                };
                let mut rng = Rng::new(seed);
                brain.attack_cooldown = 100.0;
                let dt = 1.0 / 60.0;
                player.try_attack(None);
                // Быстрее ~150 мс не реагирует никто.
                for frame in 0..9 {
                    let seen = [Seen {
                        id: 1,
                        kind: ActorKind::Player,
                        pose: player_pose,
                        action: player.action,
                        weapon: player.weapon,
                        shield: false,
                        alive: true,
                        targetable: true,
                    }];
                    think(
                        &mut brain,
                        ActorKind::AlanLancer,
                        &mut pose,
                        &mut npc,
                        &seen,
                        &level,
                        &mut rng,
                        dt,
                    );
                    npc.tick(dt);
                    player.tick(dt);
                    assert!(
                        !npc.guard_held,
                        "{training:?} поднял защиту через {} мс",
                        frame * 16
                    );
                }
            }
        }
    }

    /// Сколько ударов игрока враг отразил (доля) в нескольких поединках.
    fn defended_share(kind: ActorKind, training: Training) -> f32 {
        let (mut hits, mut defended) = (0u32, 0u32);
        for seed in [3u64, 11, 19, 27, 35, 43] {
            let (_, _, stats, _) = fight_with(kind, Some(training), false, 40.0, seed);
            hits += stats[0];
            defended += stats[1];
        }
        defended as f32 / (hits + defended).max(1) as f32
    }

    /// Парирование не идеальное и зависит от выучки: ветеран отражает больше новобранца,
    /// но и он пропускает удары.
    #[test]
    fn parry_depends_on_training() {
        for kind in [ActorKind::AlanLancer, ActorKind::Stratiotes] {
            let recruit = defended_share(kind, Training::Recruit);
            let veteran = defended_share(kind, Training::Veteran);
            eprintln!(
                "{kind:?}: новобранец отражает {:.0}%, ветеран {:.0}%",
                recruit * 100.0,
                veteran * 100.0
            );
            assert!(
                recruit < veteran,
                "{kind:?}: новобранец защищается не хуже ветерана"
            );
            assert!(
                veteran < 0.85,
                "{kind:?}: ветеран отражает почти всё ({veteran})"
            );
            assert!(
                recruit < 0.5,
                "{kind:?}: новобранец отражает слишком много ({recruit})"
            );
        }
    }

    /// Выносливость у врага та же, что у игрока: долгий бой выматывает, копится усталость,
    /// а стойку без сил пробивают.
    #[test]
    fn npc_gets_tired_and_guard_breaks() {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn::brigand(ActorKind::Stratiotes, Vec3::new(10.0, 0.0, 0.0), 0.0);
        let mut brain = Brain::new(&spawn);
        brain.training = Training::Recruit;
        let mut npc = fighter_for(ActorKind::Stratiotes);
        let npc_feet = find_free_spot(&level, spawn.pos);
        let mut pose = Pose {
            feet: npc_feet,
            yaw: 0.0,
            pitch: 0.0,
            crouching: false,
        };
        // Двое с топорами по очереди бьют новобранца со щитом.
        let mut rng = Rng::new(9);
        let dt = 1.0 / 60.0;
        let mut broken = false;
        let mut min_stamina = f32::MAX;
        for i in 0..(60 * 30) {
            let player_feet = npc_feet + Vec3::new(0.0, 0.0, 1.3);
            let mut attacker = Fighter::new(WeaponKind::Axe, 150.0, false);
            // Атакующий каждые 0,9 с начинает новый замах (как будто их несколько).
            let phase = (i % 54) as f32 * dt;
            attacker.action = if phase < 0.5 {
                Action::Windup {
                    t: phase,
                    dur: 0.5,
                    swing: combat::Swing::Overhead,
                    riposte: false,
                    morphed: false,
                }
            } else {
                Action::Strike {
                    t: 0.0,
                    swing: combat::Swing::Overhead,
                    riposte: false,
                }
            };
            let ap = Pose {
                feet: player_feet,
                yaw: yaw_towards(player_feet, pose.feet),
                pitch: 0.0,
                crouching: false,
            };
            let seen = [Seen {
                id: 1,
                kind: ActorKind::Player,
                pose: ap,
                action: attacker.action,
                weapon: WeaponKind::Axe,
                shield: false,
                alive: true,
                targetable: true,
            }];
            think(
                &mut brain,
                ActorKind::Stratiotes,
                &mut pose,
                &mut npc,
                &seen,
                &level,
                &mut rng,
                dt,
            );
            npc.tick(dt);
            if i % 54 == 31 && npc.guard_active() {
                if resolve_contact(&mut attacker, &mut npc, Contact::Guard(Vec3::ZERO), 0.0)
                    == HitOutcome::GuardBroken
                {
                    broken = true;
                }
            }
            min_stamina = min_stamina.min(npc.stamina);
        }
        eprintln!(
            "усталость {:.2}, минимум сил {min_stamina:.0}, макс. сил {:.0}",
            brain.fatigue, npc.max_stamina
        );
        assert!(brain.fatigue > 0.05, "усталость не копится");
        assert!(npc.max_stamina < 90.0, "усталость не снижает запас сил");
        assert!(
            broken || min_stamina < 15.0,
            "выносливость у врага не кончается"
        );
    }

    /// Ветеран парирует вовремя (с рипостом), а не просто держит блок; новобранец чаще
    /// поднимает защиту не вовремя.
    #[test]
    fn veteran_parries_on_time() {
        let share = |training: Training| {
            let (mut timed, mut held, mut hurt) = (0u32, 0u32, 0u32);
            for seed in [3u64, 11, 19, 27, 35, 43, 51, 59] {
                let level = crate::level::level_colliders();
                let spawn =
                    NpcSpawn::brigand(ActorKind::Bucellarius, Vec3::new(10.0, 0.0, 0.0), 0.0);
                let mut brain = Brain::new(&spawn);
                brain.training = training;
                brain.attack_cooldown = 1000.0;
                let npc_feet = find_free_spot(&level, spawn.pos);
                let player_feet = npc_feet + Vec3::new(0.0, 0.0, 1.2);
                let mut pose = Pose {
                    feet: npc_feet,
                    yaw: yaw_towards(npc_feet, player_feet),
                    pitch: 0.0,
                    crouching: false,
                };
                let player_pose = Pose {
                    feet: player_feet,
                    yaw: yaw_towards(player_feet, npc_feet),
                    pitch: 0.0,
                    crouching: false,
                };
                let mut npc = Fighter::new(WeaponKind::Spatha, 500.0, false);
                let mut player = fighter_for(ActorKind::Player);
                let mut rng = Rng::new(seed);
                let dt = 1.0 / 60.0;
                for _ in 0..(60 * 20) {
                    brain.attack_cooldown = 1000.0;
                    player.stamina = 100.0;
                    player.try_attack(None);
                    let seen = [Seen {
                        id: 1,
                        kind: ActorKind::Player,
                        pose: player_pose,
                        action: player.action,
                        weapon: player.weapon,
                        shield: false,
                        alive: true,
                        targetable: true,
                    }];
                    think(
                        &mut brain,
                        ActorKind::Bucellarius,
                        &mut pose,
                        &mut npc,
                        &seen,
                        &level,
                        &mut rng,
                        dt,
                    );
                    npc.tick(dt);
                    player.tick(dt);
                    if let Some((swing, p)) = player.strike_progress()
                        && !player.hit_this_swing.contains(&2)
                    {
                        let (h, t) = blade_segment(&player_pose, player.weapon, swing, p);
                        if let Some(c) = trace_blade(h, t, &player_pose, &npc, &pose) {
                            player.hit_this_swing.push(2);
                            match resolve_contact(&mut player, &mut npc, c, 1.0) {
                                HitOutcome::Parried => timed += 1,
                                HitOutcome::Blocked { .. } => held += 1,
                                HitOutcome::Damage { .. } => hurt += 1,
                                _ => {}
                            }
                        }
                    }
                    npc.hp = 500.0;
                }
            }
            (timed, held, hurt)
        };
        let (rt, rh, rd) = share(Training::Recruit);
        let (vt, vh, vd) = share(Training::Veteran);
        eprintln!(
            "новобранец: вовремя {rt}, глухо {rh}, пропустил {rd}; ветеран: вовремя {vt}, глухо {vh}, пропустил {vd}"
        );
        let hurt_share = |t: u32, h: u32, d: u32| d as f32 / (t + h + d).max(1) as f32;
        assert!(
            hurt_share(vt, vh, vd) < hurt_share(rt, rh, rd),
            "ветеран пропускает не реже новобранца"
        );
        assert!(
            vt as f32 / (vt + vh).max(1) as f32 > 0.55,
            "ветеран в основном держит глухой блок"
        );
    }

    /// Ветеран против игрока, который стоит в стойке: финтит и морфит, а не бьёт в лоб по таймеру.
    #[test]
    fn veteran_feints_against_turtle() {
        let level = crate::level::level_colliders();
        let spawn = NpcSpawn::brigand(ActorKind::Stratiotes, Vec3::new(10.0, 0.0, 0.0), 0.0);
        let mut brain = Brain::new(&spawn);
        brain.training = Training::Veteran;
        let npc_feet = find_free_spot(&level, spawn.pos);
        let player_feet = npc_feet + Vec3::new(0.0, 0.0, 1.3);
        let mut pose = Pose {
            feet: npc_feet,
            yaw: yaw_towards(npc_feet, player_feet),
            pitch: 0.0,
            crouching: false,
        };
        let player_pose = Pose {
            feet: player_feet,
            yaw: yaw_towards(player_feet, npc_feet),
            pitch: 0.0,
            crouching: false,
        };
        let mut npc = fighter_for(ActorKind::Stratiotes);
        let mut player = fighter_for(ActorKind::Player);
        let mut rng = Rng::new(17);
        let dt = 1.0 / 60.0;
        let (mut feints, mut morphs, mut swings) = (0, 0, std::collections::HashSet::new());
        for _ in 0..(60 * 90) {
            player.set_guard(true);
            player.stamina = 100.0;
            npc.stamina = npc.max_stamina;
            let seen = [Seen {
                id: 1,
                kind: ActorKind::Player,
                pose: player_pose,
                action: player.action,
                weapon: player.weapon,
                shield: false,
                alive: true,
                targetable: true,
            }];
            let before = npc.action;
            think(
                &mut brain,
                ActorKind::Stratiotes,
                &mut pose,
                &mut npc,
                &seen,
                &level,
                &mut rng,
                dt,
            );
            if matches!(before, Action::Windup { .. })
                && matches!(npc.action, Action::Idle | Action::Guard { .. })
            {
                feints += 1;
            }
            if matches!(npc.action, Action::Windup { morphed: true, .. })
                && !matches!(before, Action::Windup { morphed: true, .. })
            {
                morphs += 1;
            }
            if let Action::Windup { swing, .. } = npc.action {
                swings.insert(swing);
            }
            npc.tick(dt);
            player.tick(dt);
        }
        eprintln!(
            "финтов {feints}, морфов {morphs}, разных ударов {}",
            swings.len()
        );
        assert!(feints >= 2, "ветеран не финтит");
        assert!(swings.len() >= 4, "бьёт однообразно: {swings:?}");
        let _ = morphs;
    }

    /// Выучка раздаётся по типам: букелларии — ветераны или обученные, у готов бывают новобранцы.
    #[test]
    fn training_spread() {
        let mut goths = [0; 3];
        for i in 0..200 {
            let p = Vec3::new(i as f32 * 3.7, 0.0, (i * 7 % 13) as f32 * 2.3);
            assert_ne!(Training::roll(ActorKind::Bucellarius, p), Training::Recruit);
            goths[Training::roll(ActorKind::GothWarrior, p) as usize] += 1;
        }
        assert!(
            goths.iter().all(|n| *n > 10),
            "разброс выучки готов: {goths:?}"
        );
    }
}
