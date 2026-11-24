//! Кузнечное и литейное дело Херсона около 510 года. Чистая модель без графики и сети:
//! её шагают и сервер (честный расчёт), и клиент (плавный жар между снимками).
//!
//! Всё, как у настоящего кузнеца, только время сжато:
//! - **Металл.** Железо приходит крицей из сыродутного горна (руда с Керченского полуострова
//!   и древесный уголь), полосами от купцов или ломом. Сталь — то же железо с углеродом:
//!   кусками из удачной крицы или после цементации. Расплавить железо горн не может —
//!   его только куют. Льют бронзу (медь с оловом, около 9:1), серебро и свинец.
//! - **Жар.** Термометров нет: кузнец смотрит на цвет каления. Модель хранит градусы,
//!   а игроку показывает только цвет. Знание приёмов подписывает цвета («ковочный жар»).
//! - **Ковка.** Металл остывает на наковальне за десяток секунд. Бить можно от вишнёвого
//!   до жёлтого; по остывшему — трещины, перегретый в горне горит. Тяжёлый молот тянет,
//!   лёгкий выглаживает забоины. Пакет из нескольких полос сначала сваривают на белом жару.
//! - **Закалка и отпуск.** Только сталь твердеет: с вишнёвого жара в воду (твёрже, но может
//!   треснуть), рассол или масло (мягче). Потом отпуск по цветам побежалости.
//! - **Отделка.** Точило снимает окалину и забоины, даёт кромку; рукоять и навершие —
//!   из ясеня и бронзы.
//!
//! Результат — [`Make`]: качество, изъяны, закалка, развесовка и прочность вещи.

use crate::items::{Flaw, ItemBaseId, Make, Quality, Rng, base_durability};
use crate::lore::Technique;
use serde::{Deserialize, Serialize};

/// Комнатная температура (°C).
pub const AMBIENT: f32 = 20.0;
/// Секунд горения одной меры угля в горне.
pub const FUEL_PER_CHARCOAL: f32 = 70.0;
/// Сколько угля помещается в горн (секунд).
pub const FUEL_MAX: f32 = 280.0;
/// Самый короткий промежуток между ударами (с): лёгким и тяжёлым молотом.
pub const STRIKE_GAP: [f32; 2] = [0.42, 0.75];
/// Период качания «маятника» удара на экране (с): лёгкий и тяжёлый молот.
pub const SWING_PERIOD: [f32; 2] = [1.0, 1.45];

pub const CHARCOAL: &str = "Древесный уголь";
pub const IRON: &str = "Железная полоса";
pub const STEEL: &str = "Стальная полоса";
pub const SCRAP: &str = "Железный лом";
pub const BLOOM: &str = "Крица";
pub const ORE: &str = "Железная руда";
pub const ASH: &str = "Ясеневая заготовка";
pub const HIDE: &str = "Сырая шкура";
pub const POMMEL: &str = "Бронзовое навершие";
pub const MOULD: &str = "Каменная литейная форма";
pub const CRUCIBLE: &str = "Тигель";
pub const WAX: &str = "Воск";
pub const CLAY: &str = "Гончарная глина";
pub const POT: &str = "Глиняный горшок";
pub const CAST_IRON: &str = "Чугунная корка";

// ---------------------------------------------------------------------------
// Цвет каления
// ---------------------------------------------------------------------------

/// Как светится железо в полутёмной кузнице (sRGB) и как это называют кузнецы.
pub fn heat_color(t: f32) -> ([f32; 3], &'static str) {
    const STOPS: [(f32, [f32; 3], &str); 10] = [
        (20.0, [0.18, 0.17, 0.17], "холодное"),
        (480.0, [0.2, 0.15, 0.14], "не светится"),
        (600.0, [0.38, 0.06, 0.03], "тёмно-красный"),
        (720.0, [0.62, 0.08, 0.03], "вишнёвый"),
        (830.0, [0.85, 0.16, 0.04], "ярко-вишнёвый"),
        (950.0, [1.0, 0.33, 0.07], "оранжево-красный"),
        (1050.0, [1.0, 0.55, 0.12], "оранжевый"),
        (1150.0, [1.0, 0.8, 0.3], "жёлтый"),
        (1260.0, [1.0, 0.95, 0.68], "светло-жёлтый"),
        (1340.0, [1.0, 1.0, 0.94], "белый"),
    ];
    let mut name = STOPS[0].2;
    for w in STOPS.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t >= a.0 {
            name = a.2;
        }
        if t >= a.0 && t < b.0 {
            let k = (t - a.0) / (b.0 - a.0);
            let c = [a.1[0] + (b.1[0] - a.1[0]) * k, a.1[1] + (b.1[1] - a.1[1]) * k, a.1[2] + (b.1[2] - a.1[2]) * k];
            return (c, name);
        }
    }
    if t >= 1380.0 { (STOPS[9].1, "искрит — горит!") } else { (STOPS[9].1, STOPS[9].2) }
}

/// Цвета побежалости на зачищенной стали при отпуске (°C → цвет, название).
pub fn temper_color(t: f32) -> ([f32; 3], &'static str) {
    match t {
        t if t < 200.0 => ([0.78, 0.78, 0.76], "светлая сталь"),
        t if t < 235.0 => ([0.88, 0.82, 0.55], "бледно-соломенный"),
        t if t < 255.0 => ([0.85, 0.7, 0.35], "соломенный"),
        t if t < 268.0 => ([0.7, 0.48, 0.25], "бронзовый"),
        t if t < 282.0 => ([0.55, 0.25, 0.4], "пурпурный"),
        t if t < 295.0 => ([0.4, 0.2, 0.55], "фиолетовый"),
        t if t < 315.0 => ([0.2, 0.25, 0.62], "синий"),
        t if t < 345.0 => ([0.45, 0.6, 0.78], "светло-синий"),
        _ => ([0.55, 0.56, 0.58], "серый — перегрет"),
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Насколько металл поддаётся молоту при этой температуре (0..1).
pub fn plasticity(t: f32) -> f32 {
    smooth(620.0, 1150.0, t)
}

// ---------------------------------------------------------------------------
// Что можно сделать
// ---------------------------------------------------------------------------

/// Что кует кузнец.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pattern {
    /// Проковка крицы: выбить шлак на белом жару, получить полосы.
    Bloom,
    /// Сварить лом в полосу.
    ScrapWeld,
    Knife,
    Spatha,
    CavalrySpatha,
    Axe,
    Spear,
    Mace,
    Arrowheads,
    Spangenhelm,
    RidgeHelmet,
    Mail,
    Lamellar,
    Vambraces,
    Greaves,
    OvalShield,
    RoundShield,
    /// Вид из [`crate::arsenal::ARMS`] (мечи, ножи, топоры, булавы, копья, наконечники).
    Arms(u16),
}

/// Чем вещь будет после ковки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    /// Заготовка: полосы.
    Stock,
    Weapon,
    Armour,
}

pub struct PatternInfo {
    /// Готовое изделие (имя предмета).
    pub product: &'static str,
    pub count: u16,
    /// Сколько полос (или лома, или одна крица) идёт в работу.
    pub metal: u8,
    /// Что ещё нужно при сборке (рукоять, ремни).
    pub parts: &'static [(&'static str, u16)],
    /// Участки поковки слева направо (для экрана).
    pub segments: &'static [&'static str],
    /// Нужна ли кузнечная сварка до формовки (пакет, крица, лом).
    pub weld: bool,
    /// Калится ли (клинки, наконечники, топоры).
    pub harden: bool,
    /// Холодная работа (заклёпывание колец кольчуги).
    pub cold: bool,
    /// Без этого знания не понять, как это вообще делается.
    pub needs: Option<Technique>,
    /// Насколько массивна поковка: медленнее греется и остывает.
    pub mass: f32,
    /// Лучший отпуск (°C): кромка ножа твёрже, клинок меча упруже.
    pub temper_best: f32,
    pub kind: PieceKind,
}

impl Pattern {
    pub const ALL: [Pattern; 17] = {
        use Pattern::*;
        [Bloom, ScrapWeld, Knife, Spatha, CavalrySpatha, Axe, Spear, Mace, Arrowheads, Spangenhelm, RidgeHelmet, Mail, Lamellar, Vambraces, Greaves, OvalShield, RoundShield]
    };

    /// Всё, что можно сковать: заготовки, старые образцы, доспехи и весь арсенал.
    pub fn all() -> Vec<Pattern> {
        let mut v = Self::ALL.to_vec();
        v.extend((0..crate::arsenal::ARMS.len() as u16).map(Pattern::Arms));
        v
    }

    /// Семейство для списка в кузне.
    pub fn fam(self) -> Option<crate::arsenal::Fam> {
        use crate::arsenal::Fam;
        match self {
            Pattern::Arms(i) => crate::arsenal::ARMS.get(i as usize).map(|a| a.fam),
            Pattern::Knife => Some(Fam::Seax),
            Pattern::Spatha | Pattern::CavalrySpatha => Some(Fam::Sword),
            Pattern::Axe => Some(Fam::Axe),
            Pattern::Mace => Some(Fam::Mace),
            Pattern::Spear => Some(Fam::Spear),
            Pattern::Arrowheads => Some(Fam::Head),
            _ => None,
        }
    }

    pub fn info(self) -> PatternInfo {
        use Pattern::*;
        if let Arms(i) = self {
            let a = &crate::arsenal::ARMS[(i as usize).min(crate::arsenal::ARMS.len() - 1)];
            let head = a.fam == crate::arsenal::Fam::Head;
            return PatternInfo {
                product: a.name,
                count: if head { 12 } else { 1 },
                metal: a.metal,
                parts: a.parts,
                segments: a.fam.segments(),
                weld: a.weld,
                harden: a.temper > 0.0,
                cold: false,
                needs: a.needs,
                mass: a.mass,
                temper_best: a.temper,
                kind: PieceKind::Weapon,
            };
        }
        use Technique as T;
        let p = |product, count, metal, parts, segments, weld, harden, needs, mass, temper_best, kind| PatternInfo {
            product,
            count,
            metal,
            parts,
            segments,
            weld,
            harden,
            cold: false,
            needs,
            mass,
            temper_best,
            kind,
        };
        const BLADE6: &[&str] = &["хвостовик", "пята", "дол", "дол", "сильная часть", "острие"];
        match self {
            Bloom => p(IRON, 2, 1, &[], &["край", "середина", "край"], true, false, None, 2.4, 0.0, PieceKind::Stock),
            ScrapWeld => p(IRON, 1, 3, &[], &["пакет", "полоса"], true, false, None, 1.6, 0.0, PieceKind::Stock),
            Knife => p("Кинжал", 1, 1, &[(ASH, 1)], &["хвостовик", "клинок", "острие"], false, true, None, 0.7, 240.0, PieceKind::Weapon),
            Spatha => p("Спата", 1, 3, &[(ASH, 1)], BLADE6, true, true, None, 1.2, 290.0, PieceKind::Weapon),
            CavalrySpatha => p("Кавалерийская спата", 1, 4, &[(ASH, 1)], &["хвостовик", "пята", "дол", "дол", "дол", "сильная часть", "острие"], true, true, None, 1.35, 295.0, PieceKind::Weapon),
            Axe => p("Боевой топор", 1, 2, &[(ASH, 1)], &["обух", "проух", "щёки", "лезвие"], true, true, None, 1.5, 255.0, PieceKind::Weapon),
            Spear => p("Пехотное копьё", 1, 1, &[(ASH, 1)], &["втулка", "шейка", "перо", "острие"], false, true, None, 0.9, 270.0, PieceKind::Weapon),
            Mace => p("Булава", 1, 2, &[(ASH, 1)], &["втулка", "навершие", "рёбра"], true, false, None, 1.7, 0.0, PieceKind::Weapon),
            Arrowheads => p("Наконечники стрел", 12, 1, &[], &["черешки", "перья", "острия"], false, true, None, 0.5, 260.0, PieceKind::Weapon),
            Spangenhelm => p("Спангенхельм", 1, 3, &[(HIDE, 1)], &["сегмент", "сегмент", "сегмент", "сегмент", "обруч"], false, false, Some(T::Armoury), 1.3, 0.0, PieceKind::Armour),
            RidgeHelmet => p("Гребенчатый шлем", 1, 2, &[(HIDE, 1)], &["левая половина", "гребень", "правая половина"], false, false, Some(T::Armoury), 1.2, 0.0, PieceKind::Armour),
            Mail => PatternInfo {
                cold: true,
                ..p("Кольчуга", 1, 5, &[], &["ворот", "грудь", "спина", "бок", "бок", "рукав", "рукав", "подол"], false, false, Some(T::MailMaking), 0.6, 0.0, PieceKind::Armour)
            },
            Lamellar => p("Ламеллярный панцирь", 1, 5, &[(HIDE, 2)], &["ряд I", "ряд II", "ряд III", "ряд IV", "ряд V", "наплечья"], false, false, Some(T::Armoury), 1.0, 0.0, PieceKind::Armour),
            Vambraces => p("Ламеллярные наручи", 1, 1, &[(HIDE, 1)], &["левый", "правый"], false, false, Some(T::Armoury), 0.8, 0.0, PieceKind::Armour),
            Greaves => p("Железные поножи", 1, 2, &[(HIDE, 1)], &["левая", "правая", "наколенники"], false, false, Some(T::Armoury), 1.1, 0.0, PieceKind::Armour),
            OvalShield => p("Овальный щит", 1, 1, &[(ASH, 2), (HIDE, 1)], &["умбон", "рукоять", "оковка"], false, false, None, 1.0, 0.0, PieceKind::Armour),
            RoundShield => p("Круглый щит", 1, 1, &[(ASH, 1), (HIDE, 1)], &["умбон", "рукоять"], false, false, None, 1.0, 0.0, PieceKind::Armour),
            Arms(_) => unreachable!("разобрано выше"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pattern::Bloom => "Проковать крицу",
            Pattern::ScrapWeld => "Сварить лом в полосу",
            p => p.info().product,
        }
    }
}

// ---------------------------------------------------------------------------
// Ковка
// ---------------------------------------------------------------------------

/// Где сейчас поковка.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    /// В горне, в углях.
    Fire,
    /// На наковальне.
    Anvil,
    /// На краю горна, без дутья: отпуск.
    Temper,
    /// Остывает, зарытая в золу: отжиг.
    Ashes,
    /// На точиле.
    Grindstone,
}

/// Чем калить.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quench {
    Water,
    Brine,
    Oil,
}

impl Quench {
    pub const ALL: [Quench; 3] = [Quench::Water, Quench::Brine, Quench::Oil];
    pub fn name(self) -> &'static str {
        match self {
            Quench::Water => "в воду",
            Quench::Brine => "в рассол",
            Quench::Oil => "в масло",
        }
    }
    /// Резкость охлаждения.
    fn severity(self) -> f32 {
        match self {
            Quench::Water => 1.0,
            Quench::Brine => 1.15,
            Quench::Oil => 0.62,
        }
    }
}

/// Поковка на наковальне (одна у игрока).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Forging {
    pub pattern: Pattern,
    /// °C.
    pub temp: f32,
    pub spot: Spot,
    /// Сколько вытянут каждый участок: 1 — как надо.
    pub shape: Vec<f32>,
    /// Сварка пакета или проковка крицы: 1 — слои схватились.
    pub weld: f32,
    /// Чистота и углерод исходного металла.
    pub purity: f32,
    pub carbon: f32,
    /// Накопленные беды 0..1+.
    pub cracks: f32,
    pub cold_shut: f32,
    pub burn: f32,
    pub scale: f32,
    pub dents: f32,
    pub stress: f32,
    /// После закалки: твёрдость 0..1, «хрупкость» (снимается отпуском).
    pub hardness: f32,
    pub brittle: f32,
    pub quenched: bool,
    pub warp: f32,
    /// Наибольший жар отпуска (°C, 0 — не отпускали).
    pub temper: f32,
    /// Точило: 1 — чисто.
    pub ground: f32,
    pub strikes: u32,
    /// Время с последнего удара (с) — сервер не даёт бить чаще молота.
    pub since_strike: f32,
    /// Сколько заготовку «ведёт» от ударов по одной грани (знак — в какую сторону).
    /// Кузнец переворачивает её клещами, иначе полосу выгибает.
    #[serde(default)]
    pub lean: f32,
    /// Какой гранью вверх лежит заготовка.
    #[serde(default)]
    pub flipped: bool,
}

/// До какого перекоса бить по одной грани безопасно.
pub const LEAN_SAFE: f32 = 0.18;

/// Что случилось после действия (для подсказки игроку).
#[derive(Debug, Clone, PartialEq)]
pub enum SmithEvent {
    Crack,
    Sparks,
    Welded,
    ColdShut,
    Missed,
    Quenched { hard: bool, cracked: bool },
    Annealed,
    TooCold,
    Note(String),
}

impl SmithEvent {
    pub fn text(&self) -> String {
        match self {
            SmithEvent::Crack => "Металл треснул: били по остывшему".into(),
            SmithEvent::Sparks => "Летят искры — железо горит в горне!".into(),
            SmithEvent::Welded => "Слои схватились: сварка взялась".into(),
            SmithEvent::ColdShut => "Сварка не берётся: жара мало".into(),
            SmithEvent::Missed => "Удар вскользь: забоина".into(),
            SmithEvent::Quenched { hard: true, cracked: false } => "Шипит пар: закалилось".into(),
            SmithEvent::Quenched { hard: true, cracked: true } => "Закалилось, но пошла трещина".into(),
            SmithEvent::Quenched { hard: false, cracked: false } => "Не закалилось: металл был холоден или это мягкое железо".into(),
            SmithEvent::Quenched { hard: false, cracked: true } => "Треснуло и не закалилось".into(),
            SmithEvent::Annealed => "Остыло в золе: напряжение ушло, металл мягкий".into(),
            SmithEvent::TooCold => "Холодное: не поддаётся".into(),
            SmithEvent::Note(s) => s.clone(),
        }
    }
}

impl Forging {
    pub fn new(pattern: Pattern, purity: f32, carbon: f32) -> Self {
        let info = pattern.info();
        Self {
            pattern,
            temp: AMBIENT,
            spot: Spot::Anvil,
            shape: vec![0.0; info.segments.len()],
            weld: if info.weld { 0.0 } else { 1.0 },
            purity,
            carbon,
            cracks: 0.0,
            cold_shut: 0.0,
            burn: 0.0,
            scale: 0.0,
            dents: 0.0,
            stress: 0.0,
            hardness: 0.0,
            brittle: 0.0,
            quenched: false,
            warp: 0.0,
            temper: 0.0,
            ground: 0.0,
            strikes: 0,
            since_strike: 10.0,
            lean: 0.0,
            flipped: false,
        }
    }

    /// Перевернуть заготовку клещами на другую грань.
    pub fn flip(&mut self) {
        self.flipped = !self.flipped;
    }

    /// Шаг времени: жар горна (°C, 0 — горн погас), куда идёт тепло. Возвращает события.
    pub fn step(&mut self, dt: f32, fire_temp: f32) -> Vec<SmithEvent> {
        let mut ev = Vec::new();
        self.since_strike += dt;
        let mass = self.pattern.info().mass;
        let before = self.temp;
        match self.spot {
            Spot::Fire => {
                self.temp += (fire_temp.max(AMBIENT) - self.temp) * (1.0 - (-0.26 * dt / mass).exp());
            }
            Spot::Temper => {
                // Край углей без дутья: медленно и не выше ~420 °C.
                let edge = (fire_temp * 0.36).clamp(AMBIENT, 430.0);
                self.temp += (edge - self.temp) * (1.0 - (-0.07 * dt / mass).exp());
            }
            Spot::Ashes => {
                self.temp -= (self.temp - AMBIENT) * (1.0 - (-0.03 * dt / mass).exp());
                if before > 300.0 && self.temp <= 300.0 {
                    self.stress = 0.0;
                    self.hardness = 0.0;
                    self.brittle = 0.0;
                    self.quenched = false;
                    ev.push(SmithEvent::Annealed);
                }
            }
            Spot::Anvil | Spot::Grindstone => self.temp = cool(self.temp, dt, mass),
        }
        // Выше красного жара сталь снова мягкая: прежняя закалка пропадает.
        if self.temp > 730.0 && self.quenched {
            self.quenched = false;
            self.hardness = 0.0;
            self.brittle = 0.0;
            self.temper = 0.0;
        }
        // Отпуск закалённой стали.
        if self.quenched && self.temp > self.temper && self.temp > 150.0 {
            self.temper = self.temp;
            let soft = smooth(180.0, 330.0, self.temper);
            self.brittle = (self.hardness * (1.0 - soft)).max(0.0);
        }
        // Окалина, пережог и выгорание углерода.
        if self.temp > 850.0 {
            self.scale += dt * (self.temp - 850.0) / 450.0 * 0.0045;
        }
        if self.temp > 1000.0 && self.spot == Spot::Fire {
            self.carbon = (self.carbon - dt * 0.0006 * (self.temp - 1000.0) / 200.0).max(0.02);
        }
        if self.temp > 1340.0 {
            let was = self.burn;
            self.burn += dt * (self.temp - 1340.0) / 100.0 * 0.25;
            if was < 0.05 && self.burn >= 0.05 {
                ev.push(SmithEvent::Sparks);
            }
        }
        ev
    }

    /// Удар молотом по участку `zone`. `acc` — точность 0..1 (с экрана), `skill` — навык 0..100.
    pub fn strike(&mut self, zone: usize, heavy: bool, acc: f32, skill: f32, rng: &mut Rng) -> Vec<SmithEvent> {
        let mut ev = Vec::new();
        let info = self.pattern.info();
        let n = self.shape.len();
        if n == 0 || self.spot != Spot::Anvil {
            return ev;
        }
        self.strikes += 1;
        self.since_strike = 0.0;
        let s = (skill / 100.0).clamp(0.0, 1.0);
        let acc = acc.clamp(0.0, 1.0);
        // Неумелая рука промахивается по соседнему участку.
        let mut zone = zone.min(n - 1);
        if rng.f32() < 0.22 * (1.0 - s) * (1.0 - acc * 0.5) {
            zone = if rng.f32() < 0.5 { zone.saturating_sub(1) } else { (zone + 1).min(n - 1) };
        }
        let force = if heavy { 2.2 } else { 1.0 };
        let plast = if info.cold { 1.0 } else { plasticity(self.temp) };
        let cold_limit = if self.carbon > 0.3 { 780.0 } else { 700.0 };
        if !info.cold && self.temp < cold_limit {
            // Остывший металл не тянется, а копит напряжение и трескается.
            self.stress += 0.03 * force;
            let risk = (cold_limit - self.temp) / 320.0 * force * (0.35 + self.carbon) * (1.0 - 0.5 * s);
            if rng.f32() < risk * 0.35 {
                self.cracks += 0.12 * force;
                ev.push(SmithEvent::Crack);
            } else if self.temp < 500.0 {
                ev.push(SmithEvent::TooCold);
            }
        }
        if info.cold && heavy {
            // По кольчужным кольцам кувалдой не бьют: кольца рвутся.
            self.dents += 0.05;
            ev.push(SmithEvent::Note("Тяжёлым молотом кольца рвутся".into()));
        }
        // Сварка пакета: только на белом жару, сильными ударами.
        if self.weld < 1.0 {
            if self.temp >= 1230.0 {
                let gain = 0.09 * force * (0.4 + 0.6 * acc) * smooth(1230.0, 1320.0, self.temp).max(0.35);
                self.weld = (self.weld + gain).min(1.0);
                if self.weld >= 1.0 {
                    ev.push(SmithEvent::Welded);
                }
                return ev;
            } else if self.temp > 650.0 {
                // Тянуть несваренный пакет: слои расходятся.
                self.cold_shut += 0.035 * force;
                if !ev.contains(&SmithEvent::ColdShut) {
                    ev.push(SmithEvent::ColdShut);
                }
            }
        }
        let noise = 1.0 + (rng.f32() - 0.5) * 0.6 * (1.0 - s);
        let quality = acc.powf(1.3);
        let base = if info.cold { 0.06 } else { 0.065 };
        let gain = base * force * plast * (0.25 + 0.75 * quality) * noise;
        self.shape[zone] += gain;
        // Удары по одной грани тянут её сильнее другой: полосу выгибает, пока не перевернёшь.
        if !info.cold {
            self.lean += gain * 0.5 * if self.flipped { -1.0 } else { 1.0 };
            let over = self.lean.abs() - LEAN_SAFE;
            if over > 0.0 {
                self.warp += over * gain * 3.0 * (1.0 - 0.4 * s);
                if over > 0.06 {
                    ev.push(SmithEvent::Note("Заготовку повело на одну сторону — переверните (R)".into()));
                }
            }
        }
        for nb in [zone.wrapping_sub(1), zone + 1] {
            if nb < n {
                self.shape[nb] += gain * 0.25;
            }
        }
        if acc < 0.35 {
            self.dents += 0.05 * (1.0 - acc) * force;
            ev.push(SmithEvent::Missed);
        } else if heavy {
            self.dents += 0.006;
        } else if (600.0..=980.0).contains(&self.temp) || info.cold {
            // Лёгкие точные удары на неярком жару выглаживают поверхность.
            self.dents = (self.dents - 0.025 * acc).max(0.0);
        }
        ev
    }

    /// Окунуть в закалочную ванну.
    pub fn quench(&mut self, medium: Quench, skill: f32, rng: &mut Rng) -> SmithEvent {
        let t = self.temp;
        let sev = medium.severity();
        let s = (skill / 100.0).clamp(0.0, 1.0);
        // Мартенсит выходит только с температуры выше ~727 °C; перегрев — крупное зерно.
        let transform = smooth(730.0, 790.0, t);
        let overheat = smooth(900.0, 1100.0, t);
        let carbon_k = (self.carbon / 0.6).clamp(0.0, 1.2);
        let hard = transform * carbon_k.min(1.0) * (0.5 + 0.5 * sev.min(1.1));
        let risk = sev * (0.08 + self.carbon * 0.45) * (0.25 + self.stress + overheat * 1.2) * (1.0 - 0.4 * s) * transform.max(0.2);
        let cracked = rng.f32() < risk;
        if cracked {
            self.cracks += 0.35;
        }
        self.warp += overheat * 0.3 * sev + rng.f32() * 0.08 * (1.0 - s);
        self.temp = 40.0;
        self.spot = Spot::Anvil;
        if hard > 0.15 {
            self.quenched = true;
            self.hardness = hard.min(1.0);
            self.brittle = self.hardness * (1.0 + overheat * 0.5);
            self.temper = 0.0;
        }
        SmithEvent::Quenched { hard: hard > 0.15, cracked }
    }

    /// Проход на точиле.
    pub fn grind(&mut self, acc: f32) {
        let acc = acc.clamp(0.0, 1.0);
        self.ground += 0.11 * (0.35 + 0.65 * acc);
        self.scale = (self.scale - 0.035 * acc).max(0.0);
        self.dents = (self.dents - 0.02 * acc).max(0.0);
    }

    /// Сколько недостаёт или лишнего по участкам (среднее |s−1|).
    pub fn shape_error(&self) -> f32 {
        if self.shape.is_empty() {
            return 0.0;
        }
        self.shape.iter().map(|s| if *s > 1.0 { (s - 1.0) * 1.6 } else { 1.0 - s }).sum::<f32>() / self.shape.len() as f32
    }

    /// Развесовка: перекованное острие легче, недокованное тяжелее.
    pub fn balance(&self) -> f32 {
        let n = self.shape.len();
        if n < 2 {
            return 0.0;
        }
        let mut b = 0.0;
        for (i, s) in self.shape.iter().enumerate() {
            let w = i as f32 / (n - 1) as f32 * 2.0 - 1.0;
            b += (1.0 - s) * w;
        }
        (b / n as f32 * 2.0 + self.warp * 0.5).clamp(-1.0, 1.0)
    }

    /// Готова ли форма настолько, чтобы собирать вещь.
    pub fn shaped(&self) -> bool {
        self.weld >= 1.0 && self.shape.iter().all(|s| *s >= 0.6)
    }
}

/// Остывание на воздухе: тонкий клинок с 1150 до 800 °C — около 10 секунд.
pub fn cool(t: f32, dt: f32, mass: f32) -> f32 {
    let k = 0.018 * (1.0 + (t / 1000.0).powi(2)) / mass;
    AMBIENT + (t - AMBIENT) * (-k * dt).exp()
}

/// Жар в горне: без дутья угли держат около 850 °C, с мехами — до 1400 °C.
pub fn fire_temp(fuel: f32, blast: f32) -> f32 {
    if fuel <= 0.0 { 200.0 } else { 850.0 + 560.0 * blast.clamp(0.0, 1.0) }
}

/// Как растёт и спадает дутьё (0..1) от мехов.
pub fn step_blast(blast: f32, bellows: bool, dt: f32) -> f32 {
    if bellows { (blast + dt * 0.45).min(1.0) } else { (blast - dt * 0.22).max(0.0) }
}

/// Итог ковки.
pub struct Outcome {
    pub quality: Quality,
    pub make: Make,
    /// 0..1 — общий балл.
    pub score: f32,
}

/// Балл чистоты металла по качеству полосы.
pub fn purity_of(q: Quality) -> f32 {
    [0.3, 0.55, 0.7, 0.85, 0.95, 1.0][q.index()]
}

/// Качество по общему баллу.
pub fn quality_of(score: f32) -> Quality {
    match score {
        s if s < 0.36 => Quality::Poor,
        s if s < 0.56 => Quality::Common,
        s if s < 0.7 => Quality::Uncommon,
        s if s < 0.82 => Quality::Rare,
        s if s < 0.92 => Quality::Epic,
        _ => Quality::Legendary,
    }
}

impl Forging {
    /// Насколько вытянута поковка: 0 — пруток, 1 — готовая форма.
    pub fn drawn(&self) -> f32 {
        if self.shape.is_empty() {
            return 0.0;
        }
        (self.shape.iter().map(|s| s.min(1.0)).sum::<f32>() / self.shape.len() as f32).clamp(0.0, 1.0)
    }

    /// Оценить работу. `fitting` — бронзовое навершие (лучше развесовка, красивее).
    pub fn outcome(&self, smith: &str, fitting: bool) -> Outcome {
        let info = self.pattern.info();
        let err = self.shape_error();
        let shape = (1.0 - err * 2.2).clamp(0.0, 1.0);
        let integrity = (1.0 - self.cracks * 1.3 - self.cold_shut * 1.4 - self.burn * 1.5 - (1.0 - self.purity) * 0.45).clamp(0.0, 1.0);
        let pits = (self.scale * 2.0).clamp(0.0, 1.0);
        let surface = (1.0 - self.dents * 2.0 - pits * 0.7 + self.ground.min(1.0) * 0.35).clamp(0.0, 1.0);
        let over_ground = (self.ground - 1.35).max(0.0);
        // Закалка: хорошая твёрдость при малой хрупкости и правильном отпуске.
        let (edge, temper_score) = if info.harden {
            let soft = smooth(250.0, 470.0, self.temper);
            let edge = (self.hardness * (1.0 - soft * 0.75)).clamp(0.0, 1.0);
            let near = if self.temper > 0.0 { 1.0 - ((self.temper - info.temper_best).abs() / 70.0).min(1.0) } else { 0.0 };
            let score = (edge * 0.55 + near * 0.3 + (1.0 - self.brittle).max(0.0) * 0.15).clamp(0.0, 1.0);
            (edge, score)
        } else {
            (0.0, 1.0)
        };
        let score = if info.kind == PieceKind::Stock {
            shape * 0.25 + integrity * 0.5 + self.purity * 0.25
        } else if info.harden {
            shape * 0.3 + integrity * 0.25 + temper_score * 0.2 + surface * 0.1 + self.purity * 0.15
        } else {
            shape * 0.38 + integrity * 0.3 + surface * 0.15 + self.purity * 0.17
        };
        let mut score = (score - over_ground * 0.3 + if fitting { 0.03 } else { 0.0 }).clamp(0.0, 1.0);
        // Недокованная вещь — ещё не вещь: выше «плохой» не бывает.
        if self.shape.iter().any(|s| *s < 0.8) {
            score = score.min(0.3);
        }
        let mut flaws = Vec::new();
        let mut add = |cond: bool, f: Flaw| {
            if cond {
                flaws.push(f);
            }
        };
        add(self.cracks > 0.1, Flaw::Crack);
        add(self.cold_shut > 0.08, Flaw::ColdShut);
        add(self.burn > 0.08, Flaw::Burnt);
        add(self.purity < 0.45, Flaw::Slag);
        add(pits > 0.35 && self.ground < 0.6, Flaw::Pitted);
        add(self.warp > 0.25, Flaw::Warped);
        add(self.shape.iter().any(|s| *s > 1.25), Flaw::ThinSpot);
        add(self.shape.iter().any(|s| *s < 0.8), Flaw::Unfinished);
        add(info.harden && self.quenched && self.brittle > 0.5, Flaw::Brittle);
        add(info.harden && (!self.quenched || edge < 0.25) && info.kind == PieceKind::Weapon, Flaw::Soft);
        add(over_ground > 0.1, Flaw::Overground);
        add(self.dents > 0.25, Flaw::Dented);
        let base = ItemBaseId::find(info.product).map(|b| b.base());
        let dur_base = base.map(base_durability).unwrap_or(0.0);
        let tough = if info.harden { 1.0 - self.brittle.clamp(0.0, 1.0) * 0.6 } else { 1.0 };
        let dur_max = (dur_base * (0.35 + 0.8 * integrity) * (0.6 + 0.4 * tough) * (0.85 + 0.3 * shape)).round();
        let balance = if fitting { self.balance() * 0.6 } else { self.balance() };
        let finish = (surface * 0.8 + self.ground.min(1.0) * 0.2 - pits * 0.3).clamp(0.0, 1.0);
        let make = Make { smith: smith.to_string(), finish, edge, balance, flaws, dur: dur_max, dur_max, carbon: self.carbon };
        Outcome { quality: quality_of(score), make, score }
    }
}

/// Свойства, которые даёт работа мастера (в дополнение к основной характеристике).
pub fn crafted_affixes(kind: PieceKind, make: &Make, quality: Quality) -> Vec<(crate::items::Stat, f32)> {
    use crate::items::Stat;
    let mut v = Vec::new();
    let r1 = |x: f32| (x * 10.0).round() / 10.0;
    match kind {
        PieceKind::Weapon => {
            // Твёрдая кромка режет глубже; мягкая тупится.
            v.push((Stat::BonusDamage, r1((make.edge - 0.45) * 8.0)));
            // Хорошо уравновешенный клинок быстрее в руке.
            v.push((Stat::ActionSpeed, r1((0.22 - make.balance.abs()) * 14.0)));
            if quality >= Quality::Rare && !make.flaws.contains(&Flaw::Brittle) {
                v.push((Stat::ParryWindow, ((quality.index() as f32 - 2.0) * 12.0).round()));
            }
        }
        PieceKind::Armour => {
            if make.flaws.contains(&Flaw::Unfinished) {
                v.push((Stat::MoveSpeed, -2.5));
            }
            if quality >= Quality::Uncommon {
                v.push((Stat::MaxHealth, (quality.index() as f32 * 3.0).round()));
            }
        }
        PieceKind::Stock => {}
    }
    v.retain(|(_, x)| x.abs() >= 0.05);
    v
}

// ---------------------------------------------------------------------------
// Литьё
// ---------------------------------------------------------------------------

/// Что отливают.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastPiece {
    Buckle,
    Fibula,
    Pommel,
    Cross,
    Ring,
    Sinkers,
}

/// Металл в тигле.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastMetal {
    Copper,
    Tin,
    Bronze,
    Silver,
    Lead,
}

impl CastMetal {
    pub const ALL: [CastMetal; 5] = [CastMetal::Copper, CastMetal::Tin, CastMetal::Bronze, CastMetal::Silver, CastMetal::Lead];
    pub fn item(self) -> &'static str {
        match self {
            CastMetal::Copper => "Медный лом",
            CastMetal::Tin => "Олово",
            CastMetal::Bronze => "Бронзовый лом",
            CastMetal::Silver => "Серебряный лом",
            CastMetal::Lead => "Свинец",
        }
    }
    /// Вес одной штуки в драхмах (около 3,4 г).
    pub fn drachms(self) -> f32 {
        match self {
            CastMetal::Copper => 24.0,
            CastMetal::Tin => 4.0,
            CastMetal::Bronze => 24.0,
            CastMetal::Silver => 2.0,
            CastMetal::Lead => 32.0,
        }
    }
}

pub struct CastInfo {
    pub product: &'static str,
    pub count: u16,
    /// Сколько металла уходит на отливку с литником (драхмы).
    pub drachms: f32,
    /// Литьё по воску (форма из глины, одноразовая) или в каменную форму.
    pub lost_wax: bool,
    /// Какой сплав нужен.
    pub alloy: Alloy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alloy {
    Bronze,
    Silver,
    Lead,
}

impl CastPiece {
    pub const ALL: [CastPiece; 6] = [CastPiece::Buckle, CastPiece::Fibula, CastPiece::Pommel, CastPiece::Cross, CastPiece::Ring, CastPiece::Sinkers];
    pub fn info(self) -> CastInfo {
        let c = |product, count, drachms, lost_wax, alloy| CastInfo { product, count, drachms, lost_wax, alloy };
        match self {
            CastPiece::Buckle => c("Бронзовая пряжка", 1, 14.0, false, Alloy::Bronze),
            CastPiece::Fibula => c("Бронзовая фибула", 1, 10.0, false, Alloy::Bronze),
            CastPiece::Pommel => c("Бронзовое навершие", 1, 22.0, false, Alloy::Bronze),
            CastPiece::Cross => c("Нательный крест", 1, 12.0, true, Alloy::Bronze),
            CastPiece::Ring => c("Серебряный перстень", 1, 5.0, true, Alloy::Silver),
            CastPiece::Sinkers => c("Свинцовые грузила", 6, 60.0, false, Alloy::Lead),
        }
    }
}

/// Тигель с металлом и форма.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Casting {
    pub piece: CastPiece,
    /// Драхмы каждого металла в тигле.
    pub copper: f32,
    pub tin: f32,
    pub silver: f32,
    pub lead: f32,
    pub temp: f32,
    pub spot: Spot,
    /// Доля расплавленного 0..1.
    pub melt: f32,
    pub dross: f32,
    /// Прогрев формы 0..1 и стоит ли она у горна.
    pub mould_heat: f32,
    pub mould_warm: bool,
    /// Сколько формы залито 0..1 и сколько раз лили с перерывом.
    pub poured: f32,
    pub pouring: bool,
    pub pauses: u8,
    pub porosity: f32,
    pub misrun: f32,
    /// Форма раскрыта, отливку чеканят и шлифуют.
    pub opened: bool,
    pub chased: f32,
    pub dents: f32,
}

impl Casting {
    pub fn new(piece: CastPiece, copper: f32, tin: f32, silver: f32, lead: f32) -> Self {
        Self { piece, copper, tin, silver, lead, temp: AMBIENT, spot: Spot::Anvil, melt: 0.0, dross: 0.0, mould_heat: 0.0, mould_warm: false, poured: 0.0, pouring: false, pauses: 0, porosity: 0.0, misrun: 0.0, opened: false, chased: 0.0, dents: 0.0 }
    }

    pub fn total(&self) -> f32 {
        self.copper + self.tin + self.silver + self.lead
    }

    /// Доля олова в бронзе.
    pub fn tin_frac(&self) -> f32 {
        let cu = self.copper + self.tin;
        if cu <= 0.0 { 0.0 } else { self.tin / cu }
    }

    /// Температура полного расплавления (°C).
    pub fn liquidus(&self) -> f32 {
        let total = self.total().max(1e-3);
        if self.lead / total > 0.6 {
            return 327.0;
        }
        if self.silver / total > 0.6 {
            return 962.0;
        }
        (1085.0 - 950.0 * self.tin_frac().min(0.25)).max(830.0)
    }

    pub fn step(&mut self, dt: f32, fire_temp: f32) -> Vec<SmithEvent> {
        let mut ev = Vec::new();
        let liq = self.liquidus();
        match self.spot {
            Spot::Fire => self.temp += (fire_temp.max(AMBIENT) - self.temp) * (1.0 - (-0.2 * dt).exp()),
            _ => self.temp = cool(self.temp, dt, 0.8),
        }
        if self.temp > liq {
            self.melt = (self.melt + dt * 0.12 * (1.0 + (self.temp - liq) / 120.0)).min(1.0);
            // Открытый расплав под дутьём окисляется.
            if self.spot == Spot::Fire {
                self.dross = (self.dross + dt * 0.012).min(1.0);
            }
        } else if self.temp < liq - 40.0 && self.melt > 0.0 && !self.opened {
            self.melt = (self.melt - dt * 0.2).max(0.0);
        }
        if self.mould_warm && !self.opened && self.poured <= 0.0 {
            self.mould_heat = (self.mould_heat + dt * 0.07).min(1.0);
        } else {
            self.mould_heat = (self.mould_heat - dt * 0.02).max(0.0);
        }
        if self.pouring {
            let info = self.piece.info();
            let rate = 0.28;
            let need = info.drachms;
            let have = self.total() * self.melt;
            let cap = (have / need).min(1.0);
            self.poured = (self.poured + dt * rate).min(cap.max(self.poured));
            // Металл стынет в форме, если он едва жидкий, а форма холодная.
            let margin = self.temp - liq;
            if margin < 60.0 {
                self.misrun += dt * 0.6 * (1.0 - self.mould_heat * 0.7) * (1.0 - margin.max(0.0) / 60.0);
            }
            if margin > 260.0 {
                self.porosity += dt * 0.15;
            }
            if self.melt < 0.98 {
                self.misrun += dt * 0.4;
            }
            if cap < 1.0 && self.poured >= cap {
                self.pouring = false;
                ev.push(SmithEvent::Note("Металла не хватило: форма залита не вся".into()));
                self.misrun += 0.5;
            } else if self.poured >= 1.0 {
                self.pouring = false;
                ev.push(SmithEvent::Note("Форма залита".into()));
            }
        }
        ev
    }

    pub fn start_pour(&mut self) -> Result<(), String> {
        if self.opened || self.poured >= 1.0 {
            return Err("Уже отлито".into());
        }
        if self.melt < 0.3 {
            return Err("Металл ещё не расплавился".into());
        }
        if self.poured > 0.0 {
            // Струю прервали — на стыке будет спай.
            self.pauses += 1;
        }
        self.porosity += self.dross * 0.6;
        self.dross *= 0.5;
        self.pouring = true;
        Ok(())
    }

    pub fn skim(&mut self) {
        self.dross = 0.0;
        let k = 0.96;
        self.copper *= k;
        self.tin *= k;
        self.silver *= k;
        self.lead *= k;
    }

    /// Чеканка и опиловка отливки.
    pub fn chase(&mut self, acc: f32, heavy: bool) {
        let acc = acc.clamp(0.0, 1.0);
        if heavy {
            self.dents += 0.08;
        }
        self.chased = (self.chased + 0.12 * (0.3 + 0.7 * acc)).min(1.4);
        if acc < 0.35 {
            self.dents += 0.04;
        }
    }

    pub fn outcome(&self, smith: &str) -> Outcome {
        let info = self.piece.info();
        let alloy = match info.alloy {
            Alloy::Bronze => {
                // 8–12 % олова — лучшая бронза: меньше — мягкая и густая, больше — хрупкая.
                let t = self.tin_frac();
                1.0 - ((t - 0.1).abs() / 0.08).min(1.0) * 0.8 - (self.silver + self.lead) / self.total().max(1.0)
            }
            Alloy::Silver => self.silver / self.total().max(1.0),
            Alloy::Lead => self.lead / self.total().max(1.0),
        }
        .clamp(0.0, 1.0);
        let fill = (1.0 - self.misrun.min(1.0) - self.pauses as f32 * 0.25).clamp(0.0, 1.0);
        let sound = (1.0 - self.porosity.min(1.0)).clamp(0.0, 1.0);
        let finish = (self.chased.min(1.0) - self.dents * 1.5 - (self.chased - 1.25).max(0.0)).clamp(0.0, 1.0);
        let score = (alloy * 0.3 + fill * 0.3 + sound * 0.2 + finish * 0.2).clamp(0.0, 1.0);
        let mut flaws = Vec::new();
        if self.misrun > 0.25 || self.pauses > 0 {
            flaws.push(Flaw::Misrun);
        }
        if self.porosity > 0.25 {
            flaws.push(Flaw::Porous);
        }
        if self.dents > 0.2 {
            flaws.push(Flaw::Dented);
        }
        let make = Make { smith: smith.into(), finish, edge: 0.0, balance: 0.0, flaws, dur: 0.0, dur_max: 0.0, carbon: 0.0 };
        Outcome { quality: quality_of(score), make, score }
    }
}

// ---------------------------------------------------------------------------
// Плавка руды и цементация: долго держать жар в нужных пределах
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoakKind {
    /// Сыродутный горн: руда + уголь → крица. `rich` — богатость закладки ×100
    /// (сырая горная руда 100, обожжённая богаче, болотная беднее; см. mining.rs).
    Bloomery { ore: u8, charcoal: u8, rich: u8 },
    /// Полоса в угле в запечатанном горшке → сталь.
    Carburize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Soak {
    pub kind: SoakKind,
    pub temp: f32,
    pub blast: f32,
    pub bellows: bool,
    pub elapsed: f32,
    /// Сколько всего секунд держать.
    pub total: f32,
    /// Секунды в нужном жару.
    pub good: f32,
    /// Перегрев (чугун или лопнувший горшок).
    pub over: f32,
    /// Углерод, набранный металлом (%).
    pub carbon: f32,
}

impl Soak {
    pub fn new(kind: SoakKind) -> Self {
        let total = match kind {
            SoakKind::Bloomery { .. } => 60.0,
            SoakKind::Carburize => 45.0,
        };
        Self { kind, temp: AMBIENT, blast: 0.0, bellows: false, elapsed: 0.0, total, good: 0.0, over: 0.0, carbon: 0.05 }
    }

    /// Нужный жар (°C): от и до.
    pub fn window(&self) -> (f32, f32) {
        match self.kind {
            SoakKind::Bloomery { .. } => (1150.0, 1300.0),
            SoakKind::Carburize => (900.0, 1080.0),
        }
    }

    /// Доля угля к руде по весу (руда и уголь — по штукам, штука угля легче).
    pub fn ratio(&self) -> f32 {
        match self.kind {
            SoakKind::Bloomery { ore, charcoal, .. } => charcoal as f32 * 0.6 / (ore.max(1) as f32),
            SoakKind::Carburize => 1.0,
        }
    }

    /// `fuel` — есть ли уголь в горне (для цементации; сыродутный горн заправлен заранее).
    pub fn step(&mut self, dt: f32, fuel: bool) -> Vec<SmithEvent> {
        let mut ev = Vec::new();
        if self.elapsed >= self.total {
            self.temp = cool(self.temp, dt, 3.0);
            return ev;
        }
        self.elapsed += dt;
        self.blast = step_blast(self.blast, self.bellows, dt);
        let (lo, hi) = self.window();
        let target = match self.kind {
            // В шахте горна жар зависит от дутья и сколько угля на руду.
            SoakKind::Bloomery { .. } => 820.0 + 620.0 * self.blast * (0.75 + 0.35 * self.ratio().min(1.5)),
            SoakKind::Carburize => fire_temp(if fuel { 1.0 } else { 0.0 }, self.blast),
        };
        self.temp += (target - self.temp) * (1.0 - (-0.22 * dt).exp());
        if self.temp >= lo && self.temp <= hi {
            self.good += dt;
        }
        match self.kind {
            SoakKind::Bloomery { .. } => {
                // Чем жарче и больше угля, тем больше углерода уходит в железо.
                if self.temp > 1150.0 {
                    self.carbon += dt * 0.006 * ((self.temp - 1150.0) / 100.0) * self.ratio().min(2.0);
                }
                if self.temp > 1350.0 {
                    let was = self.over;
                    self.over += dt * (self.temp - 1350.0) / 100.0 * 0.06;
                    if was < 0.1 && self.over >= 0.1 {
                        ev.push(SmithEvent::Note("Из горна течёт чугун: слишком жарко".into()));
                    }
                }
            }
            SoakKind::Carburize => {
                if self.temp > 880.0 {
                    self.carbon += dt * 0.0016 * ((self.temp - 880.0) / 120.0).min(1.6);
                }
                if self.temp > 1180.0 {
                    let was = self.over;
                    self.over += dt * 0.08;
                    if was < 0.3 && self.over >= 0.3 {
                        ev.push(SmithEvent::Note("Горшок лопнул от жара".into()));
                    }
                }
            }
        }
        ev
    }

    pub fn done(&self) -> bool {
        self.elapsed >= self.total
    }

    /// Доля времени в нужном жару.
    pub fn steadiness(&self) -> f32 {
        (self.good / self.total).clamp(0.0, 1.0)
    }
}

/// Что вышло из сыродутного горна: (сколько криц, их качество, углерод, сколько чугуна).
pub fn bloomery_yield(s: &Soak) -> (u16, Quality, f32, u16) {
    let SoakKind::Bloomery { ore, rich, .. } = s.kind else { return (0, Quality::Poor, 0.0, 0) };
    let ratio = s.ratio();
    // Мало угля — руда не восстановилась; много — зря сожгли, но железо чище от шлака.
    let reduce = (ratio / 0.9).min(1.0) * s.steadiness().powf(0.7);
    let iron = ore as f32 * 0.33 * (rich as f32 / 100.0) * reduce * (1.0 - s.over.min(1.0) * 0.7);
    let blooms = (iron / 1.0).floor() as u16;
    let cast = (ore as f32 * 0.25 * s.over.min(1.0)).round() as u16;
    let purity = (0.3 + 0.55 * s.steadiness() + 0.15 * (1.0 - ((ratio - 1.1).abs() / 0.8).min(1.0)) + crate::mining::richness_purity(rich)).clamp(0.0, 1.0);
    let q = match purity {
        p if p < 0.45 => Quality::Poor,
        p if p < 0.65 => Quality::Common,
        p if p < 0.8 => Quality::Uncommon,
        p if p < 0.92 => Quality::Rare,
        _ => Quality::Epic,
    };
    (blooms, q, s.carbon.clamp(0.03, 1.6), cast)
}

/// Что вышло после цементации: качество стальной полосы (None — испорчено).
pub fn carburize_yield(s: &Soak, bar_quality: Quality) -> Option<Quality> {
    if s.over >= 0.3 || s.carbon < 0.3 {
        return None;
    }
    let purity = purity_of(bar_quality) * (0.6 + 0.4 * s.steadiness());
    Some(match purity {
        p if p < 0.4 => Quality::Poor,
        p if p < 0.55 => Quality::Common,
        p if p < 0.7 => Quality::Uncommon,
        p if p < 0.85 => Quality::Rare,
        _ => Quality::Epic,
    })
}

/// Углерод полосы, если он не записан (%).
pub fn bar_carbon(name: &str) -> f32 {
    match name {
        STEEL => 0.7,
        IRON => 0.08,
        SCRAP => 0.12,
        _ => 0.05,
    }
}

// ---------------------------------------------------------------------------
// Точность удара по «маятнику»
// ---------------------------------------------------------------------------

/// Где маятник сейчас (−1..1). `t` — время с начала качания.
pub fn swing_pos(t: f32, heavy: bool) -> f32 {
    (t / SWING_PERIOD[heavy as usize] * std::f32::consts::TAU).sin()
}

/// Ширина «сладкой» середины маятника — растёт с навыком.
pub fn sweet_width(skill: f32) -> f32 {
    0.28 + 0.32 * (skill / 100.0).clamp(0.0, 1.0)
}

/// Точность удара по положению маятника.
pub fn accuracy(pos: f32, skill: f32) -> f32 {
    (1.0 - pos.abs() / sweet_width(skill)).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Ремонт
// ---------------------------------------------------------------------------

/// Сколько прочности восстанавливает правка и заточка у горна (доля от наибольшей).
/// Трещину не заварить: каждый ремонт чуть снижает наибольшую прочность.
pub fn repair(make: &mut Make, skill: f32) {
    let s = (skill / 100.0).clamp(0.0, 1.0);
    let lost = 0.04 * (1.0 - s * 0.7);
    make.dur_max = (make.dur_max * (1.0 - lost)).round().max(1.0);
    make.dur = make.dur_max * (0.75 + 0.25 * s);
    make.dur = make.dur.round();
}

/// Цена ремонта у кузнеца (нуммии): чем больше сбито, тем дороже.
pub fn repair_price(make: &Make, value: u32) -> u32 {
    if make.dur_max <= 0.0 {
        return 0;
    }
    let worn = 1.0 - make.condition();
    crate::economy::round_price((value as f32 * 0.25 * worn).max(if worn > 0.0 { 40.0 } else { 0.0 }))
}

#[cfg(test)]
mod lean_tests {
    use super::*;

    fn forge_blade(flip_every: Option<u32>) -> Forging {
        let mut f = Forging::new(Pattern::Spatha, 0.8, 0.5);
        f.weld = 1.0;
        f.temp = 1000.0;
        let mut rng = Rng::new(5);
        let n = f.shape.len();
        for i in 0..120u32 {
            if let Some(k) = flip_every
                && i % k == 0
            {
                f.flip();
            }
            f.temp = 1000.0;
            f.strike(i as usize % n, false, 1.0, 60.0, &mut rng);
        }
        f
    }

    #[test]
    fn turning_the_bar_keeps_it_straight() {
        let never = forge_blade(None);
        let turned = forge_blade(Some(5));
        assert!(never.warp > 0.25, "не переворачивали: {}", never.warp);
        assert!(turned.warp < 0.05, "переворачивали: {}", turned.warp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heat(f: &mut Forging, target: f32) {
        f.spot = Spot::Fire;
        for _ in 0..600 {
            f.step(0.05, fire_temp(100.0, 1.0));
            if f.temp >= target {
                break;
            }
        }
        f.spot = Spot::Anvil;
    }

    #[test]
    fn colors_go_from_red_to_white() {
        assert_eq!(heat_color(20.0).1, "холодное");
        assert_eq!(heat_color(760.0).1, "вишнёвый");
        assert_eq!(heat_color(1100.0).1, "оранжевый");
        assert_eq!(heat_color(1300.0).1, "светло-жёлтый");
        assert!(heat_color(1400.0).1.contains("горит"));
    }

    #[test]
    fn blade_cools_in_about_ten_seconds() {
        let mut t = 1150.0;
        let mut secs = 0.0;
        while t > 800.0 {
            t = cool(t, 0.1, 1.2);
            secs += 0.1;
        }
        assert!((7.0..16.0).contains(&secs), "{secs}");
    }

    #[test]
    fn forge_without_bellows_never_reaches_welding_heat() {
        let mut f = Forging::new(Pattern::Spatha, 0.7, 0.3);
        f.spot = Spot::Fire;
        for _ in 0..2000 {
            f.step(0.05, fire_temp(100.0, 0.0));
        }
        assert!(f.temp < 900.0);
        let mut f = Forging::new(Pattern::Spatha, 0.7, 0.3);
        heat(&mut f, 1260.0);
        assert!(f.temp >= 1260.0);
    }

    #[test]
    fn welding_needs_white_heat() {
        let mut rng = Rng::new(3);
        let mut f = Forging::new(Pattern::Spatha, 0.7, 0.3);
        heat(&mut f, 1000.0);
        f.strike(2, true, 1.0, 50.0, &mut rng);
        assert_eq!(f.weld, 0.0);
        assert!(f.cold_shut > 0.0);
        let mut f = Forging::new(Pattern::Spatha, 0.7, 0.3);
        for _ in 0..12 {
            heat(&mut f, 1290.0);
            f.strike(2, true, 1.0, 50.0, &mut rng);
        }
        assert!(f.weld >= 1.0);
    }

    #[test]
    fn cold_hammering_cracks_steel() {
        let mut rng = Rng::new(7);
        let mut f = Forging::new(Pattern::Knife, 0.7, 0.7);
        f.temp = 450.0;
        for _ in 0..40 {
            f.strike(1, true, 1.0, 0.0, &mut rng);
        }
        assert!(f.cracks > 0.3, "{}", f.cracks);
    }

    #[test]
    fn only_steel_from_red_heat_hardens() {
        let mut rng = Rng::new(1);
        let mut f = Forging::new(Pattern::Knife, 0.7, 0.7);
        f.temp = 800.0;
        assert!(matches!(f.quench(Quench::Oil, 50.0, &mut rng), SmithEvent::Quenched { hard: true, .. }));
        let mut f = Forging::new(Pattern::Knife, 0.7, 0.7);
        f.temp = 650.0;
        assert!(matches!(f.quench(Quench::Water, 50.0, &mut rng), SmithEvent::Quenched { hard: false, .. }));
        let mut f = Forging::new(Pattern::Knife, 0.7, 0.05);
        f.temp = 820.0;
        assert!(matches!(f.quench(Quench::Water, 50.0, &mut rng), SmithEvent::Quenched { hard: false, .. }));
    }

    #[test]
    fn tempering_removes_brittleness() {
        let mut rng = Rng::new(1);
        let mut f = Forging::new(Pattern::Spatha, 0.7, 0.7);
        f.temp = 790.0;
        f.quench(Quench::Oil, 80.0, &mut rng);
        assert!(f.brittle > 0.5);
        f.spot = Spot::Temper;
        while f.temp < 295.0 {
            f.step(0.1, fire_temp(100.0, 0.0));
        }
        assert!(f.brittle < 0.2, "{}", f.brittle);
        assert!(f.hardness > 0.5);
        assert_eq!(temper_color(f.temp).1, "синий");
    }

    /// Аккуратный кузнец по правилам делает хорошую спату, небрежный — плохую.
    #[test]
    fn careful_work_beats_sloppy_work() {
        let careful = {
            let mut rng = Rng::new(11);
            let mut f = Forging::new(Pattern::Spatha, 0.85, 0.65);
            while f.weld < 1.0 {
                heat(&mut f, 1290.0);
                for _ in 0..3 {
                    f.strike(2, true, 0.95, 60.0, &mut rng);
                }
            }
            for _ in 0..80 {
                let zone = (0..f.shape.len()).min_by(|a, b| f.shape[*a].total_cmp(&f.shape[*b])).unwrap();
                if f.shape[zone] > 0.97 {
                    break;
                }
                if f.temp < 900.0 {
                    heat(&mut f, 1150.0);
                }
                f.strike(zone, f.shape[zone] < 0.8, 0.95, 60.0, &mut rng);
                f.step(0.6, 0.0);
            }
            heat(&mut f, 790.0);
            f.quench(Quench::Oil, 60.0, &mut rng);
            f.spot = Spot::Temper;
            while f.temp < 288.0 {
                f.step(0.1, fire_temp(100.0, 0.0));
            }
            f.spot = Spot::Grindstone;
            for _ in 0..9 {
                f.grind(0.9);
            }
            f.outcome("тест", true)
        };
        let sloppy = {
            let mut rng = Rng::new(11);
            let mut f = Forging::new(Pattern::Spatha, 0.55, 0.3);
            heat(&mut f, 1000.0);
            for _ in 0..60 {
                f.strike(3, true, 0.3, 0.0, &mut rng);
                f.step(0.6, 0.0);
            }
            f.outcome("тест", false)
        };
        assert!(careful.quality >= Quality::Rare, "{:?} {}", careful.quality, careful.score);
        assert!(sloppy.quality <= Quality::Common, "{:?} {}", sloppy.quality, sloppy.score);
        assert!(careful.make.dur_max > sloppy.make.dur_max);
        assert!(sloppy.make.finish < careful.make.finish);
        assert!(!sloppy.make.flaws.is_empty());
    }

    #[test]
    fn bronze_nine_to_one_is_best() {
        // 3 меди (72 драхмы) и 2 олова (8) — 10 %.
        let good = Casting::new(CastPiece::Buckle, 72.0, 8.0, 0.0, 0.0);
        assert!((good.tin_frac() - 0.1).abs() < 0.01);
        assert!(good.liquidus() < 1000.0);
        let mut c = good.clone();
        c.melt = 1.0;
        c.opened = true;
        c.chased = 1.0;
        let bad = Casting { tin: 40.0, ..c.clone() };
        assert!(c.outcome("т").score > bad.outcome("т").score);
    }

    #[test]
    fn pouring_into_cold_mould_with_barely_molten_metal_misruns() {
        let mut c = Casting::new(CastPiece::Buckle, 72.0, 8.0, 0.0, 0.0);
        c.temp = c.liquidus() + 10.0;
        c.melt = 1.0;
        c.start_pour().unwrap();
        for _ in 0..40 {
            c.step(0.1, 0.0);
        }
        let mut hot = Casting::new(CastPiece::Buckle, 72.0, 8.0, 0.0, 0.0);
        hot.temp = hot.liquidus() + 130.0;
        hot.melt = 1.0;
        hot.mould_heat = 1.0;
        hot.start_pour().unwrap();
        for _ in 0..40 {
            hot.step(0.1, 0.0);
        }
        assert!(c.misrun > hot.misrun + 0.2, "{} {}", c.misrun, hot.misrun);
    }

    #[test]
    fn bloomery_needs_steady_heat_and_enough_charcoal() {
        let run = |ore, charcoal, blast_pattern: &dyn Fn(f32) -> bool| {
            let mut s = Soak::new(SoakKind::Bloomery { ore, charcoal, rich: 100 });
            while !s.done() {
                s.bellows = blast_pattern(s.temp);
                s.step(0.1, true);
            }
            bloomery_yield(&s)
        };
        let (good, q, _, cast) = run(6, 10, &|t| t < 1230.0);
        assert!(good >= 1, "{good}");
        assert!(q >= Quality::Common);
        assert_eq!(cast, 0);
        let (lazy, _, _, _) = run(6, 10, &|_| false);
        assert_eq!(lazy, 0);
        let (_, _, _, cast) = run(6, 14, &|_| true);
        assert!(cast > 0);
    }

    #[test]
    fn crafted_spatha_is_an_item_with_durability() {
        let f = Forging::new(Pattern::Spatha, 0.7, 0.5);
        let o = f.outcome("вы", false);
        assert!(o.make.dur_max > 0.0);
        assert!(o.make.flaws.contains(&Flaw::Unfinished));
        assert_eq!(o.quality, Quality::Poor);
    }
}
