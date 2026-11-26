//! «Ковка на удачу» — простое кузнечное дело без горы материалов.
//!
//! Заготовка уже раскалена (горн держит мастер), игрок только бьёт молотом в такт маятнику:
//! - каждый удар **поднимает цену** будущей вещи — от «ветхой» до «легендарной»;
//! - каждый удар **копит напряжение**: следующий может расколоть заготовку, и тогда
//!   не остаётся ничего. Риск виден на экране — решать игроку: снять вещь сейчас или
//!   рискнуть ещё одним ударом;
//! - три точных удара подряд дают **раж**: несколько ударов металл не треснет, а цена
//!   растёт быстрее. Промах в раже обрывает его;
//! - готовые вещи идут в **заказ мастера** (платит сверх цены) или на **клейма мастера** —
//!   постоянные приёмы, с которыми следующая заготовка выдержит больше.
//!
//! Материал — одна мера железа на мелкую вещь: полоса, крица, два куска лома или руды.
//! Чистая модель без графики и сети: её шагают сервер (честный расчёт) и клиент (показ).

use crate::items::{Quality, Rng};
use crate::smithing::{BLOOM, IRON, ORE, SCRAP, STEEL, Pattern, quality_of};
use serde::{Deserialize, Serialize};

/// Что можно сковать на удачу: короткий список ходовых вещей.
pub const QUICK_PATTERNS: [Pattern; 6] = [Pattern::Knife, Pattern::Arrowheads, Pattern::Spear, Pattern::Axe, Pattern::Mace, Pattern::Spatha];

/// Мер железа на вещь.
pub fn iron_needed(p: Pattern) -> u8 {
    match p {
        Pattern::Spatha | Pattern::CavalrySpatha => 3,
        Pattern::Axe | Pattern::Mace => 2,
        _ => 1,
    }
}

/// Чем можно заменить меру железа: (предмет, сколько штук на меру, начальная цена 0..1).
/// Порядок — в каком брать: сначала дешёвое.
pub const IRON_SOURCES: [(&str, u8, f32); 7] = [
    (ORE, 2, 0.10),
    (crate::mining::BOG_ORE, 2, 0.08),
    (crate::mining::ROASTED_ORE, 2, 0.12),
    (SCRAP, 2, 0.12),
    (BLOOM, 1, 0.16),
    (IRON, 1, 0.2),
    (STEEL, 1, 0.28),
];

/// Сколько мер железа можно собрать из того, что есть (имя → штук).
pub fn iron_units(have: impl Fn(&str) -> u32) -> u32 {
    IRON_SOURCES.iter().map(|(n, per, _)| have(n) / *per as u32).sum()
}

/// Какие стопки взять на `need` мер (сначала дешёвое): (имя, штук).
pub fn pick_iron(need: u8, have: impl Fn(&str) -> u32) -> Option<Vec<(&'static str, u32)>> {
    let mut left = need as u32;
    let mut out = Vec::new();
    for (name, per, _) in IRON_SOURCES {
        if left == 0 {
            break;
        }
        let units = (have(name) / per as u32).min(left);
        if units > 0 {
            out.push((name, units * per as u32));
            left -= units;
        }
    }
    (left == 0).then_some(out)
}

/// Начальная цена заготовки по взятому железу.
pub fn start_score(picked: &[(&str, u32)]) -> f32 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for (name, count) in picked {
        if let Some((_, per, s)) = IRON_SOURCES.iter().find(|(m, ..)| m == name) {
            let units = (*count / *per as u32) as f32;
            sum += s * units;
            n += units;
        }
    }
    if n > 0.0 { sum / n } else { 0.1 }
}

/// Клейма мастера: что игрок выменял на готовые вещи. Действуют на каждую заготовку.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub struct Marks {
    /// «Твёрдая рука»: напряжение копится на треть медленнее.
    pub steady: bool,
    /// «Долгий раж»: раж держится на два удара дольше.
    pub long_rage: bool,
    /// «Ровный такт»: раж приходит после двух точных ударов, а не трёх.
    pub quick_rage: bool,
    /// «Тяжёлая рука»: каждый удар добавляет цене на пятую часть больше.
    pub heavy: bool,
    /// Обереги наковальни: каждый один раз спасает заготовку от раскола.
    pub wards: u8,
}

/// Наибольшее число оберегов про запас.
pub const WARDS_MAX: u8 = 3;

/// Клеймо, которое можно выменять.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Steady,
    LongRage,
    QuickRage,
    Heavy,
    Ward,
}

impl Mark {
    pub const ALL: [Mark; 5] = [Mark::Steady, Mark::QuickRage, Mark::LongRage, Mark::Heavy, Mark::Ward];

    pub fn name(self) -> &'static str {
        match self {
            Mark::Steady => "Твёрдая рука",
            Mark::LongRage => "Долгий раж",
            Mark::QuickRage => "Ровный такт",
            Mark::Heavy => "Тяжёлая рука",
            Mark::Ward => "Оберег наковальни",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Mark::Steady => "риск трещины растёт на треть медленнее",
            Mark::LongRage => "раж длится на 2 удара дольше",
            Mark::QuickRage => "раж после 2 точных ударов вместо 3",
            Mark::Heavy => "каждый удар поднимает цену на 20% сильнее",
            Mark::Ward => "один раз спасает заготовку от раскола",
        }
    }

    /// Сколько готовых вещей и какого качества (не хуже) за клеймо.
    pub fn cost(self) -> (u8, Quality) {
        match self {
            Mark::Ward => (1, Quality::Poor),
            Mark::Steady | Mark::QuickRage => (2, Quality::Common),
            Mark::LongRage | Mark::Heavy => (2, Quality::Uncommon),
        }
    }

    /// Уже есть (обереги — когда запас полон).
    pub fn owned(self, m: &Marks) -> bool {
        match self {
            Mark::Steady => m.steady,
            Mark::LongRage => m.long_rage,
            Mark::QuickRage => m.quick_rage,
            Mark::Heavy => m.heavy,
            Mark::Ward => m.wards >= WARDS_MAX,
        }
    }

    pub fn grant(self, m: &mut Marks) {
        match self {
            Mark::Steady => m.steady = true,
            Mark::LongRage => m.long_rage = true,
            Mark::QuickRage => m.quick_rage = true,
            Mark::Heavy => m.heavy = true,
            Mark::Ward => m.wards = (m.wards + 1).min(WARDS_MAX),
        }
    }
}

/// Какая вещь годится в уплату клейма или в заказ: только то, что куют на удачу.
pub fn is_quick_product(name: &str) -> bool {
    QUICK_PATTERNS.iter().any(|p| p.info().product == name)
}

/// Заказ мастера: вещь не хуже такого качества — плата сверх цены.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Order {
    pub pattern: Pattern,
    pub min: Quality,
    /// Плата, в долях.
    pub reward: u32,
}

impl Order {
    /// Новый заказ: что-нибудь ходовое, чаще простое. `done` — сколько заказов уже сдано
    /// (чем больше, тем требовательнее мастер).
    pub fn roll(rng: &mut Rng, done: u32) -> Self {
        let pattern = QUICK_PATTERNS[rng.range_u32(0, QUICK_PATTERNS.len() as u32 - 1) as usize];
        let top = (1 + done.min(6) / 2).min(3) as usize;
        let min = Quality::ALL[rng.range_u32(1, top as u32 + 1).min(4) as usize];
        Self::new(pattern, min)
    }

    pub fn new(pattern: Pattern, min: Quality) -> Self {
        let info = pattern.info();
        let base = crate::items::ItemBaseId::find(info.product).map_or(100, crate::economy::base_price);
        let value = base as f32 * crate::economy::quality_price(min) * info.count.max(1) as f32;
        // Мастер платит на четверть больше цены и сверху — за спешку.
        let reward = crate::economy::round_price(value * 1.25 + 400.0);
        Self { pattern, min, reward }
    }
}

/// Что случилось после удара.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    /// Удар лёг: цена выросла на столько (0..1).
    Good(f32),
    /// Вскользь: цены почти не прибавилось, напряжение выросло.
    Glancing,
    /// Пошёл раж.
    Rage,
    /// Промах оборвал раж.
    RageLost,
    /// Оберег спас заготовку.
    Saved,
    /// Заготовка раскололась.
    Shattered,
}

/// Заготовка на наковальне.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct QuickForge {
    pub pattern: Pattern,
    /// Цена будущей вещи 0..1 ([`quality_of`] даёт качество).
    pub score: f32,
    pub start: f32,
    /// Вероятность, что следующий удар (вне ража) расколет заготовку.
    pub risk: f32,
    /// Точных ударов подряд.
    pub streak: u8,
    /// Сколько ударов ещё длится раж.
    pub rage: u8,
    pub strikes: u16,
    pub shattered: bool,
    /// Время с последнего удара (с).
    pub since: f32,
    /// Клейма игрока (обереги тратятся прямо отсюда — сервер переносит остаток к игроку).
    pub marks: Marks,
    pub last: Option<Hit>,
    /// Лучшая цена за все удары (для итоговой строки).
    pub best: f32,
}

/// Удар с такой точностью и выше — точный.
pub const GOOD_HIT: f32 = 0.7;
/// Удар хуже этого — промах.
pub const MISS: f32 = 0.25;
/// Самый частый удар (с).
pub const QUICK_GAP: f32 = 0.42;
/// Выше этого риск не поднимается.
pub const RISK_MAX: f32 = 0.85;

impl QuickForge {
    pub fn new(pattern: Pattern, start: f32, marks: Marks) -> Self {
        Self { pattern, score: start, start, risk: 0.0, streak: 0, rage: 0, strikes: 0, shattered: false, since: 10.0, marks, last: None, best: start }
    }

    pub fn quality(&self) -> Quality {
        quality_of(self.score)
    }

    /// Сколько осталось до следующего качества: (следующее, доля пути 0..1).
    pub fn next_quality(&self) -> Option<(Quality, f32)> {
        const STEPS: [f32; 6] = [0.0, 0.36, 0.56, 0.7, 0.82, 0.92];
        let i = self.quality().index();
        let next = *STEPS.get(i + 1)?;
        let from = STEPS[i].max(self.start.min(STEPS[i + 1]));
        Some((Quality::ALL[i + 1], ((self.score - from) / (next - from)).clamp(0.0, 1.0)))
    }

    pub fn raging(&self) -> bool {
        self.rage > 0
    }

    /// Сколько точных ударов подряд нужно для ража.
    pub fn rage_need(&self) -> u8 {
        if self.marks.quick_rage { 2 } else { 3 }
    }

    fn rage_len(&self) -> u8 {
        if self.marks.long_rage { 5 } else { 3 }
    }

    /// Насколько вытянута заготовка (для показа у горна) 0..1.
    pub fn drawn(&self) -> f32 {
        ((self.score - self.start) / (0.92 - self.start).max(0.1)).clamp(0.0, 1.0) * 0.8 + 0.2
    }

    /// Жар для показа: остывает от ударов, в раже металл светится ярче.
    pub fn temp(&self) -> f32 {
        if self.shattered {
            return 600.0;
        }
        let t = 1120.0 - self.strikes as f32 * 22.0;
        if self.raging() { 1190.0 } else { t.max(780.0) }
    }

    /// Можно ли бить сейчас (молот не успевает чаще).
    pub fn ready(&self) -> bool {
        !self.shattered && self.since >= QUICK_GAP * 0.85
    }

    pub fn step(&mut self, dt: f32) {
        self.since += dt;
    }

    /// Удар молотом. `acc` 0..1 — точность по маятнику, `skill` 0..100.
    pub fn strike(&mut self, acc: f32, skill: f32, rng: &mut Rng) -> Hit {
        let acc = acc.clamp(0.0, 1.0);
        let s = (skill / 100.0).clamp(0.0, 1.0);
        self.since = 0.0;
        self.strikes += 1;
        let hit = if self.raging() {
            if acc < MISS {
                // Промах в раже: раж обрывается, но металл ещё держит.
                self.rage = 0;
                self.streak = 0;
                self.risk = (self.risk + 0.06).min(RISK_MAX);
                Hit::RageLost
            } else {
                self.rage -= 1;
                Hit::Good(self.gain(acc, s) * 1.6)
            }
        } else {
            // Раскол проверяется до прибавки: риск на экране — честная вероятность этого удара.
            if rng.f32() < self.risk {
                if self.marks.wards > 0 {
                    self.marks.wards -= 1;
                    self.risk *= 0.5;
                    self.streak = 0;
                    self.last = Some(Hit::Saved);
                    return Hit::Saved;
                }
                self.shattered = true;
                self.last = Some(Hit::Shattered);
                return Hit::Shattered;
            }
            let steady = if self.marks.steady { 0.67 } else { 1.0 };
            let grow = (0.022 + 0.045 * (1.0 - acc)) * (1.0 - 0.35 * s) * steady;
            self.risk = (self.risk + grow + if acc < MISS { 0.05 } else { 0.0 }).min(RISK_MAX);
            if acc >= GOOD_HIT {
                self.streak += 1;
            } else {
                self.streak = 0;
            }
            if self.streak >= self.rage_need() {
                self.streak = 0;
                self.rage = self.rage_len();
                Hit::Rage
            } else if acc < MISS {
                Hit::Glancing
            } else {
                Hit::Good(self.gain(acc, s))
            }
        };
        let add = match hit {
            Hit::Good(g) => g,
            Hit::Rage => self.gain(acc, s),
            Hit::Glancing => self.gain(acc, s) * 0.3,
            _ => 0.0,
        };
        self.score = (self.score + add).min(1.0);
        self.best = self.best.max(self.score);
        self.last = Some(hit);
        hit
    }

    /// Прибавка цены от удара: точный удар — больше; ближе к вершине — труднее.
    fn gain(&self, acc: f32, s: f32) -> f32 {
        let heavy = if self.marks.heavy { 1.2 } else { 1.0 };
        (0.03 + 0.05 * acc) * (1.0 - self.score * 0.45) * (1.0 + 0.2 * s) * heavy
    }
}

/// Подпись к последнему удару.
pub fn hit_text(h: Hit) -> &'static str {
    match h {
        Hit::Good(_) => "Удар лёг ровно — цена растёт",
        Hit::Glancing => "Вскользь: металл почти не поддался, напряжение растёт",
        Hit::Rage => "Раж! Несколько ударов металл не треснет",
        Hit::RageLost => "Промах — раж ушёл",
        Hit::Saved => "Пошла трещина, но оберег наковальни спас заготовку",
        Hit::Shattered => "Заготовка раскололась — ничего не осталось",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn have_of<'a>(list: &'a [(&'static str, u32)]) -> impl Fn(&str) -> u32 + 'a {
        move |n| list.iter().find(|(m, _)| *m == n).map_or(0, |(_, c)| *c)
    }

    #[test]
    fn iron_from_ore_and_bars() {
        let inv = [(ORE, 5), (IRON, 1)];
        assert_eq!(iron_units(have_of(&inv)), 3);
        // Сначала дешёвое: руда по две штуки на меру, потом полоса.
        assert_eq!(pick_iron(3, have_of(&inv)), Some(vec![(ORE, 4), (IRON, 1)]));
        assert_eq!(pick_iron(4, have_of(&inv)), None);
        let s = start_score(&[(ORE, 4), (IRON, 1)]);
        assert!((s - (0.1 * 2.0 + 0.2) / 3.0).abs() < 1e-4);
    }

    #[test]
    fn first_strike_never_shatters() {
        for seed in 1..200 {
            let mut rng = Rng::new(seed);
            let mut q = QuickForge::new(Pattern::Knife, 0.2, Marks::default());
            assert_ne!(q.strike(0.0, 0.0, &mut rng), Hit::Shattered);
        }
    }

    #[test]
    fn rage_protects_and_speeds_up() {
        let mut rng = Rng::new(7);
        let mut q = QuickForge::new(Pattern::Spatha, 0.2, Marks::default());
        q.strike(1.0, 0.0, &mut rng);
        q.strike(1.0, 0.0, &mut rng);
        // Третий точный удар (если не раскололось) даёт раж.
        if !q.shattered {
            assert_eq!(q.strike(1.0, 0.0, &mut rng), Hit::Rage);
            q.risk = 1.0;
            let before = q.score;
            for _ in 0..3 {
                assert!(matches!(q.strike(1.0, 0.0, &mut rng), Hit::Good(_)));
            }
            assert!(!q.shattered && q.score > before);
            assert!(!q.raging());
        }
    }

    #[test]
    fn miss_breaks_rage() {
        let mut rng = Rng::new(1);
        let mut q = QuickForge::new(Pattern::Axe, 0.2, Marks { quick_rage: true, ..Default::default() });
        q.rage = 3;
        assert_eq!(q.strike(0.0, 0.0, &mut rng), Hit::RageLost);
        assert!(!q.raging());
    }

    #[test]
    fn ward_saves_once() {
        let mut rng = Rng::new(3);
        let mut q = QuickForge::new(Pattern::Knife, 0.2, Marks { wards: 1, ..Default::default() });
        q.risk = 1.0;
        assert_eq!(q.strike(0.5, 0.0, &mut rng), Hit::Saved);
        q.risk = 1.0;
        assert_eq!(q.strike(0.5, 0.0, &mut rng), Hit::Shattered);
        assert!(q.shattered);
    }

    /// Баланс: осторожный кузнец (снимает при риске выше 10%) почти всегда уносит вещь,
    /// жадный (бьёт до «эпической») часто теряет заготовку, но иногда выигрывает много.
    #[test]
    fn risk_and_reward_balance() {
        let play = |stop_risk: f32, target: f32, acc: f32| {
            let (mut kept, mut lost, mut sum) = (0, 0, 0.0);
            for seed in 1..2001u64 {
                let mut rng = Rng::new(seed);
                let mut q = QuickForge::new(Pattern::Spatha, 0.2, Marks::default());
                loop {
                    if q.score >= target || (!q.raging() && q.risk > stop_risk) {
                        break;
                    }
                    // Рука не идеальна: точность гуляет.
                    let a = (acc + (rng.f32() - 0.5) * 0.5).clamp(0.0, 1.0);
                    q.strike(a, 20.0, &mut rng);
                    if q.shattered {
                        break;
                    }
                }
                if q.shattered {
                    lost += 1;
                } else {
                    kept += 1;
                    sum += q.score;
                }
            }
            (kept, lost, sum / kept.max(1) as f32)
        };
        let (kept, _, avg) = play(0.1, 1.0, 0.75);
        assert!(kept > 1500, "осторожный теряет слишком часто: {kept}");
        assert!(quality_of(avg) >= Quality::Common, "осторожный выходит слишком плохо: {avg}");
        let (kept, lost, avg) = play(1.0, 0.82, 0.75);
        assert!(lost > 300, "жадность почти не наказана: {lost}");
        assert!(kept > 300, "до редкой почти не дойти: {kept}");
        assert!(avg >= 0.82);
    }

    #[test]
    fn orders_pay_above_value() {
        let o = Order::new(Pattern::Knife, Quality::Uncommon);
        let base = crate::economy::base_price(crate::items::ItemBaseId::find("Кинжал").unwrap());
        assert!(o.reward as f32 > base as f32 * crate::economy::quality_price(Quality::Uncommon));
        let mut rng = Rng::new(5);
        for done in 0..20 {
            let o = Order::roll(&mut rng, done);
            assert!(QUICK_PATTERNS.contains(&o.pattern));
            assert!(o.min >= Quality::Common && o.min <= Quality::Epic);
        }
    }

    #[test]
    fn products_exist() {
        for p in QUICK_PATTERNS {
            assert!(crate::items::ItemBaseId::find(p.info().product).is_some(), "{:?}", p);
        }
        for (n, ..) in IRON_SOURCES {
            assert!(crate::items::ItemBaseId::find(n).is_some(), "{n}");
        }
    }
}
