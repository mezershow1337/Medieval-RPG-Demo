//! Рыбалка у Херсона: снасти, рыба Чёрного моря, где и на что она клюёт, и сама борьба
//! с рыбой на леске (вываживание). Чистые данные и расчёты без графики и сети: ими пользуются
//! и сервер (`fishing_server.rs`: кто клюнул, честно ли вытащили), и клиент
//! (`fishing_client.rs`: заброс, поплавок, полоска натяжения).
//!
//! Как ловили около 510 г.: удилище из тростника или гибкой можжевеловой жерди, леса из
//! конского волоса, бронзовый крючок, свинцовое грузило и поплавок из пробки или гусиного пера
//! (Элиан и Оппиан описывают ровно такую снасть). У берега Песочной бухты берёт мелочь —
//! хамса, ставрида, барабулька, кефаль; с мостков, где глубже, — морской ёрш, луфарь,
//! а то и калкан. Осётр заходит в бухту редко.

use crate::items::Rng;

/// Удочки: название, дальность заброса (м), прочность лесы (во сколько раз выдерживает рывок).
pub const RODS: &[Rod] = &[
    Rod { name: "Тростниковая удочка", cast: 16.0, strength: 1.0, reel: 1.0 },
    Rod { name: "Можжевеловая удочка", cast: 24.0, strength: 1.35, reel: 1.2 },
];

#[derive(Debug, Clone, Copy)]
pub struct Rod {
    pub name: &'static str,
    /// Дальше этого (м) не забросить.
    pub cast: f32,
    /// Множитель прочности лесы: крепче — дольше терпит натяжение.
    pub strength: f32,
    /// Множитель скорости подмотки.
    pub reel: f32,
}

pub fn rod(name: &str) -> Option<&'static Rod> {
    RODS.iter().find(|r| r.name == name)
}

/// Крючки: без крючка не забросить; когда леса рвётся, крючок уходит с рыбой.
pub const HOOKS: &str = "Бронзовые крючки";

/// Наживка: червь — на мелочь у берега, креветка — на хищника и крупную рыбу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Bait {
    Worms,
    Shrimp,
}

impl Bait {
    pub const ALL: [Bait; 2] = [Bait::Worms, Bait::Shrimp];

    pub fn item(self) -> &'static str {
        match self {
            Bait::Worms => "Морские черви",
            Bait::Shrimp => "Креветки",
        }
    }

    pub fn of_item(name: &str) -> Option<Bait> {
        Bait::ALL.into_iter().find(|b| b.item() == name)
    }

    /// Насколько быстрее клюёт (множитель к ожиданию).
    fn wait_mult(self) -> f32 {
        match self {
            Bait::Worms => 0.85,
            Bait::Shrimp => 1.1,
        }
    }
}

/// Где заброшен поплавок.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Water {
    /// У самого берега, по колено-по пояс (до 1,5 м).
    Shallow,
    /// Под мостками и подальше от берега (1,5–4 м).
    Middle,
    /// Глубоко: с конца мостков, с причала, в бухте (больше 4 м).
    Deep,
}

impl Water {
    pub fn of_depth(depth: f32) -> Water {
        if depth < 1.5 {
            Water::Shallow
        } else if depth < 4.0 {
            Water::Middle
        } else {
            Water::Deep
        }
    }
}

/// Вид рыбы.
#[derive(Debug, Clone, Copy)]
pub struct Fish {
    /// Название предмета (items.rs).
    pub name: &'static str,
    /// Вес пойманной рыбы, кг (от и до).
    pub kg: (f32, f32),
    /// Справочная цена штуки среднего веса, нуммии.
    pub price: u32,
    /// Сила рывков 0..1 (мелочь — 0,1, осётр — 1).
    pub strength: f32,
    /// Сколько секунд в среднем сопротивляется, пока не устанет.
    pub stamina: f32,
    /// Как часто клюёт: на мелководье, на средней глубине, на глубине.
    pub water: [f32; 3],
    /// Множитель к частоте на червя и на креветку.
    pub bait: [f32; 2],
    /// Клюёт на зорьке (утро и вечер) охотнее во столько раз.
    pub dawn: f32,
    pub about: &'static str,
}

/// Рыба, которую можно поймать. Порядок не важен: в сеть уходит название предмета.
pub const FISH: &[Fish] = &[
    Fish {
        name: "Хамса",
        kg: (0.01, 0.03),
        price: 4,
        strength: 0.08,
        stamina: 2.0,
        water: [1.0, 0.7, 0.2],
        bait: [1.0, 0.2],
        dawn: 1.2,
        about: "Черноморский анчоус: ходит стаями у самого берега. Мелочь, но её много — её солят в цистернах.",
    },
    Fish {
        name: "Ставрида",
        kg: (0.08, 0.35),
        price: 14,
        strength: 0.25,
        stamina: 4.0,
        water: [0.8, 1.0, 0.6],
        bait: [1.0, 0.7],
        dawn: 1.3,
        about: "Быстрая стайная рыба с колючим боковым швом. Хороша жареной.",
    },
    Fish {
        name: "Барабулька",
        kg: (0.05, 0.2),
        price: 36,
        strength: 0.2,
        stamina: 3.5,
        water: [0.7, 0.8, 0.3],
        bait: [1.0, 0.6],
        dawn: 1.0,
        about: "Красная султанка с усами. Римляне платили за неё серебром — и в Херсоне её берут охотно.",
    },
    Fish {
        name: "Кефаль",
        kg: (0.4, 2.2),
        price: 70,
        strength: 0.45,
        stamina: 7.0,
        water: [0.5, 0.9, 0.5],
        bait: [1.0, 0.4],
        dawn: 1.6,
        about: "Серебристая, осторожная: клюёт тихо, а на крючке ходит сильно. Лучше всего — на зорьке.",
    },
    Fish {
        name: "Морской ёрш",
        kg: (0.2, 0.9),
        price: 30,
        strength: 0.35,
        stamina: 4.0,
        water: [0.1, 0.6, 1.0],
        bait: [0.4, 1.0],
        dawn: 0.8,
        about: "Скорпена: колючая, с ядовитыми шипами. Держится у камней и свай.",
    },
    Fish {
        name: "Луфарь",
        kg: (0.8, 3.5),
        price: 130,
        strength: 0.75,
        stamina: 9.0,
        water: [0.05, 0.4, 1.0],
        bait: [0.2, 1.0],
        dawn: 1.4,
        about: "Зубастый хищник: бьёт наживку с ходу и рвётся до последнего. Держите натяжение.",
    },
    Fish {
        name: "Калкан",
        kg: (2.0, 7.0),
        price: 320,
        strength: 0.65,
        stamina: 11.0,
        water: [0.0, 0.15, 0.6],
        bait: [0.1, 1.0],
        dawn: 1.0,
        about: "Черноморская камбала в костяных бугорках. Лежит на дне на глубине — тяжёл, тянет вниз.",
    },
    Fish {
        name: "Осётр",
        kg: (6.0, 22.0),
        price: 1500,
        strength: 1.0,
        stamina: 16.0,
        water: [0.0, 0.03, 0.12],
        bait: [0.05, 1.0],
        dawn: 1.0,
        about: "Царская рыба: заходит из Меотиды редко. Икра и балык — на стол самого дуки.",
    },
];

pub fn fish(name: &str) -> Option<&'static Fish> {
    FISH.iter().find(|f| f.name == name)
}

/// Утро (5–8 ч) и вечер (18–21 ч): рыба берёт охотнее.
pub fn dawn_or_dusk(hour: f32) -> bool {
    (5.0..8.0).contains(&hour) || (18.0..21.0).contains(&hour)
}

/// Кто клюнет: вид рыбы и её вес (кг).
pub fn pick_fish(rng: &mut Rng, water: Water, bait: Bait, hour: f32) -> (&'static Fish, f32) {
    let w = water as usize;
    let b = bait as usize;
    let dawn = dawn_or_dusk(hour);
    let weights: Vec<(usize, f32)> = FISH.iter().enumerate().map(|(i, f)| (i, f.water[w] * f.bait[b] * if dawn { f.dawn } else { 1.0 })).collect();
    let i = rng.weighted(&weights).unwrap_or(0);
    let f = &FISH[i];
    // Вес: чаще средний, изредка — крупный (квадрат случайного числа тянет к лёгким).
    let t = rng.f32();
    let t = t * t * 0.7 + rng.f32() * 0.3;
    let kg = f.kg.0 + (f.kg.1 - f.kg.0) * t;
    (f, (kg * 1000.0).round() / 1000.0)
}

/// Сколько секунд ждать поклёвки.
pub fn bite_wait(rng: &mut Rng, bait: Bait, hour: f32) -> f32 {
    let base = rng.range_f32(5.0, 16.0) * bait.wait_mult();
    if dawn_or_dusk(hour) { base * 0.7 } else { base }
}

/// Насколько крупная эта рыба своего вида (0 — самая мелкая, 1 — самая крупная).
pub fn size_t(f: &Fish, kg: f32) -> f32 {
    ((kg - f.kg.0) / (f.kg.1 - f.kg.0).max(1e-6)).clamp(0.0, 1.0)
}

/// Как пойманная рыба тянет: исходные данные для вываживания (их получает клиент).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FightSpec {
    /// Сила рывков 0..1.
    pub strength: f32,
    /// Запас сил рыбы, с.
    pub stamina: f32,
    /// Зерно для рывков (одинаковые рывки на клиенте и при проверке на сервере).
    pub seed: u32,
}

impl FightSpec {
    pub fn of(f: &Fish, kg: f32, seed: u32) -> FightSpec {
        let s = size_t(f, kg);
        FightSpec { strength: (f.strength * (0.8 + 0.4 * s)).min(1.0), stamina: f.stamina * (0.8 + 0.5 * s), seed }
    }

    /// Быстрее этого рыбу не вытащить (с): сервер не верит пойманному раньше.
    pub fn min_secs(&self, distance: f32) -> f32 {
        (distance / (REEL_SPEED * 1.6)).max(0.4) + self.stamina * 0.25
    }
}

/// Подмотка без сопротивления, м/с.
pub const REEL_SPEED: f32 = 2.2;
/// Поплавок ближе этого — рыба у ног, вытащили.
pub const LANDED_AT: f32 = 1.6;
/// Сколько секунд держать поплавок под водой, чтобы успеть подсечь.
pub const HOOK_WINDOW: f32 = 1.1;
/// Сколько натяжения сверх предела терпит леса, с.
pub const SNAP_SECS: f32 = 0.7;
/// Сколько можно держать лесу совсем без натяжения, пока рыба не сойдёт, с.
pub const SLACK_SECS: f32 = 3.5;
/// Длина лесы на катушке... то есть на руке: дальше рыба уйти не может.
pub const LINE_MAX: f32 = 34.0;

/// Чем кончилась борьба.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FightEnd {
    Landed,
    /// Леса лопнула (слишком сильное натяжение).
    Snapped,
    /// Рыба сошла с крючка (леса провисла) или ушла на всю длину лесы.
    Escaped,
}

/// Борьба с рыбой на леске. Каждую долю секунды рыба тянет (изредка — рывком в сторону),
/// игрок подматывает (ЛКМ) или отпускает. Натяжение растёт, когда тянут оба; если
/// держать слишком долго — леса лопнет. Вести удочку в сторону, противоположную рыбе,
/// — натяжение меньше и подмотка быстрее (как в RDR2).
#[derive(Debug, Clone)]
pub struct Fight {
    pub spec: FightSpec,
    /// Расстояние до рыбы, м.
    pub distance: f32,
    /// Натяжение 0..~1.3 (больше 1 — леса на пределе).
    pub tension: f32,
    /// Сколько сил осталось у рыбы, 0..1.
    pub energy: f32,
    /// Куда рыба уходит: -1 влево, 1 вправо, 0 — прямо от берега.
    pub side: f32,
    /// Сейчас рывок (секунд осталось).
    pub burst: f32,
    next_burst: f32,
    over: f32,
    slack: f32,
    pub elapsed: f32,
    rng: Rng,
    line: f32,
}

impl Fight {
    pub fn new(spec: FightSpec, distance: f32, line_strength: f32) -> Fight {
        let mut rng = Rng::new(spec.seed as u64 + 1);
        let next_burst = rng.range_f32(0.8, 2.0);
        Fight { spec, distance, tension: 0.3, energy: 1.0, side: 0.0, burst: 0.0, next_burst, over: 0.0, slack: 0.0, elapsed: 0.0, rng, line: line_strength }
    }

    /// Сила, с которой рыба тянет сейчас (0..1).
    pub fn pull(&self) -> f32 {
        let tired = 0.25 + 0.75 * self.energy;
        let burst = if self.burst > 0.0 { 1.0 } else { 0.45 };
        (self.spec.strength * tired * burst).clamp(0.0, 1.0)
    }

    /// Шаг борьбы. `reeling` — игрок подматывает; `lead` — куда игрок ведёт удочку
    /// (-1 влево, 1 вправо, 0 — прямо). `reel_mult` — от удочки.
    pub fn step(&mut self, dt: f32, reeling: bool, lead: f32, reel_mult: f32) -> Option<FightEnd> {
        self.elapsed += dt;
        // Рывки: рыба то идёт ровно, то бросается в сторону.
        self.next_burst -= dt;
        if self.burst > 0.0 {
            self.burst -= dt;
        } else if self.next_burst <= 0.0 && self.energy > 0.05 {
            self.burst = self.rng.range_f32(0.6, 1.3) * (0.6 + 0.4 * self.energy);
            self.next_burst = self.burst + self.rng.range_f32(1.4, 3.6);
            let r = self.rng.f32();
            self.side = if r < 0.4 { -1.0 } else if r < 0.8 { 1.0 } else { 0.0 };
        }
        if self.burst <= 0.0 {
            self.side *= (1.0 - dt * 0.8).max(0.0);
        }
        let pull = self.pull();
        // Ведёшь удочку против рыбы — она тянет слабее.
        let against = (-lead * self.side).clamp(0.0, 1.0);
        let with = (lead * self.side).clamp(0.0, 1.0);
        let eff = pull * (1.0 - 0.4 * against + 0.25 * with);
        let target = if reeling { 0.25 + eff * 1.05 } else { eff * 0.55 };
        let k = if target > self.tension { 2.6 } else { 1.8 };
        self.tension += (target - self.tension) * (1.0 - (-k * dt).exp());
        // Рыба устаёт, пока леса натянута.
        self.energy = (self.energy - dt * (0.3 + self.tension) / self.spec.stamina.max(0.5)).max(0.0);
        // Расстояние: подмотка против тяги.
        let reel = if reeling { REEL_SPEED * reel_mult * (1.0 - eff * 0.85 + against * 0.35) } else { 0.0 };
        let run = eff * if reeling { 0.4 } else { 1.6 };
        self.distance = (self.distance - (reel - run) * dt).max(0.0);
        // Предел лесы.
        let limit = self.line;
        if self.tension > limit {
            self.over += dt;
        } else {
            self.over = (self.over - dt * 0.5).max(0.0);
        }
        if self.over > SNAP_SECS {
            return Some(FightEnd::Snapped);
        }
        if self.tension < 0.06 {
            self.slack += dt;
        } else {
            self.slack = 0.0;
        }
        if self.slack > SLACK_SECS || self.distance > LINE_MAX {
            return Some(FightEnd::Escaped);
        }
        if self.distance <= LANDED_AT {
            return Some(FightEnd::Landed);
        }
        None
    }

    /// Насколько близко к обрыву (0..1): для цвета полоски.
    pub fn danger(&self) -> f32 {
        (self.over / SNAP_SECS).clamp(0.0, 1.0).max(((self.tension - self.line * 0.8) / (self.line * 0.2)).clamp(0.0, 1.0) * 0.6)
    }

    /// Натяжение относительно предела этой лесы (1 — на пределе).
    pub fn tension_frac(&self) -> f32 {
        self.tension / self.line
    }
}

// ---------------- справочные данные для торговли и веса ----------------

/// Цены снастей и наживки (нуммии).
const TACKLE_PRICES: &[(&str, u32)] = &[("Тростниковая удочка", 160), ("Можжевеловая удочка", 640), (HOOKS, 10), ("Морские черви", 2), ("Креветки", 5)];
/// Вес снастей и наживки (кг за штуку).
const TACKLE_WEIGHTS: &[(&str, f32)] = &[("Тростниковая удочка", 0.6), ("Можжевеловая удочка", 0.9), (HOOKS, 0.004), ("Морские черви", 0.01), ("Креветки", 0.015)];

/// Справочная цена рыбы или снасти (economy::base_price).
pub fn price(name: &str) -> Option<u32> {
    if let Some(f) = fish(name) {
        return Some(f.price);
    }
    TACKLE_PRICES.iter().find(|(n, _)| *n == name).map(|(_, p)| *p)
}

/// Вес штуки (weight::unit_weight): у рыбы — средний для вида.
pub fn weight(name: &str) -> Option<f32> {
    if let Some(f) = fish(name) {
        return Some((f.kg.0 + f.kg.1) * 0.4);
    }
    TACKLE_WEIGHTS.iter().find(|(n, _)| *n == name).map(|(_, w)| *w)
}

/// Свежая рыба — съестной товар; крючки и наживка — простые материалы (удочка — «оружие»:
/// её держат в правой руке, как и всё, что держат в ней).
pub fn category(name: &str) -> Option<crate::trade::Category> {
    if fish(name).is_some() {
        return Some(crate::trade::Category::Food);
    }
    if name == HOOKS || Bait::of_item(name).is_some() {
        return Some(crate::trade::Category::Material);
    }
    None
}

/// Все предметы рыбалки (для тестов и консоли).
#[allow(dead_code)]
pub fn all_items() -> impl Iterator<Item = &'static str> {
    RODS.iter().map(|r| r.name).chain([HOOKS]).chain(Bait::ALL.iter().map(|b| b.item())).chain(FISH.iter().map(|f| f.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemBaseId;

    #[test]
    fn every_fishing_item_exists_and_has_price_and_weight() {
        for name in all_items() {
            let id = ItemBaseId::find(name).unwrap_or_else(|| panic!("нет предмета «{name}»"));
            assert!(crate::economy::base_price(id) > 0, "{name}");
            assert!(crate::weight::unit_weight(id) > 0.0, "{name}");
        }
        for r in RODS {
            assert_eq!(ItemBaseId::by_name(r.name).base().slot, Some(crate::items::EquipSlot::MainHand));
        }
    }

    #[test]
    fn deep_water_and_shrimp_bring_bigger_fish() {
        let mut rng = Rng::new(7);
        let avg = |rng: &mut Rng, w: Water, b: Bait| (0..4000).map(|_| pick_fish(rng, w, b, 12.0).0.price as f32).sum::<f32>() / 4000.0;
        let shore = avg(&mut rng, Water::Shallow, Bait::Worms);
        let deep = avg(&mut rng, Water::Deep, Bait::Shrimp);
        assert!(deep > shore * 3.0, "{shore} {deep}");
        // На мелководье осётр не берёт.
        assert!((0..4000).all(|_| pick_fish(&mut rng, Water::Shallow, Bait::Worms, 12.0).0.name != "Осётр"));
    }

    /// Простая стратегия: подматывать, пока натяжение не подошло к пределу, вести против рыбы.
    fn play(spec: FightSpec, line: f32, careful: bool) -> (FightEnd, f32) {
        let mut f = Fight::new(spec, 18.0, line);
        let mut reeling = true;
        for _ in 0..20000 {
            if careful {
                if f.tension_frac() > 0.85 {
                    reeling = false;
                } else if f.tension_frac() < 0.55 {
                    reeling = true;
                }
            }
            let lead = if careful { -f.side } else { 0.0 };
            if let Some(end) = f.step(1.0 / 60.0, reeling, lead, 1.0) {
                return (end, f.elapsed);
            }
        }
        panic!("борьба не кончилась");
    }

    #[test]
    fn small_fish_come_in_easily_big_fish_need_care() {
        let f = fish("Ставрида").unwrap();
        let (end, _) = play(FightSpec::of(f, 0.2, 1), 1.0, false);
        assert_eq!(end, FightEnd::Landed, "мелочь вытаскивается просто подмоткой");
        let s = fish("Осётр").unwrap();
        let (end, _) = play(FightSpec::of(s, 20.0, 3), 1.0, false);
        assert_eq!(end, FightEnd::Snapped, "осётр рвёт лесу, если тянуть без передышки");
        let mut landed = 0;
        for seed in 0..20 {
            let (end, secs) = play(FightSpec::of(s, 14.0, seed), 1.35, true);
            if end == FightEnd::Landed {
                landed += 1;
                assert!(secs >= FightSpec::of(s, 14.0, seed).min_secs(18.0), "{secs}");
            }
        }
        assert!(landed >= 14, "осторожно и с крепкой лесой осётра вытаскивают почти всегда: {landed}/20");
    }

    #[test]
    fn slack_line_loses_the_fish() {
        let f = fish("Кефаль").unwrap();
        let mut fight = Fight::new(FightSpec::of(f, 1.0, 5), 10.0, 1.0);
        let mut end = None;
        for _ in 0..6000 {
            end = fight.step(1.0 / 60.0, false, 0.0, 1.0);
            if end.is_some() {
                break;
            }
        }
        assert_eq!(end, Some(FightEnd::Escaped));
    }
}
