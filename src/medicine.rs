//! Врачевание: раны, кровотечение, воспаление, жар, боль, яды и ожоги.
//!
//! Только данные и расчёты (без Bevy-систем), чтобы всё проверялось тестами.
//! Сервер ведёт [`Body`] каждого игрока (`medicine_server.rs`), клиент получает его копию
//! и сам решает, как назвать недуг: без знаний виден только признак («рана горячая и опухла»),
//! со знаниями — диагноз («воспаление раны»).
//!
//! Главный принцип: вещество действует одинаково, знает о нём игрок или нет. Знание даёт
//! умелое применение (приём, доза), а не волшебную силу. Квасцы останавливают кровь и у неуча.
//! Времена сжаты: то, что в жизни длится дни, здесь занимает минуты.

use crate::lore::{Knowledge, Lore, Skill, Substance, Technique};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Константы течения болезни (секунды игрового времени)
// ---------------------------------------------------------------------------

/// Как быстро стихает кровотечение из небольшой раны (кровь сворачивается).
const CLOT_SECS: f32 = 25.0;
/// Сколько рана «спокойна», прежде чем может начаться воспаление (в жизни сутки-двое).
pub const INCUBATION_SECS: f32 = 120.0;
/// Рост воспаления в секунду у грязной средней раны.
const INFLAME_RATE: f32 = 0.004;
/// Заживление: доля тяжести в секунду у чистой перевязанной раны.
const HEAL_RATE: f32 = 0.0022;
/// Полураспад действия антисептика (мирра, мёд, мазь) и жаропонижающего.
const ANTISEPTIC_HALF_LIFE: f32 = 300.0;
const ANTIPYRETIC_HALF_LIFE: f32 = 150.0;
const ANTIDOTE_HALF_LIFE: f32 = 120.0;

// ---------------------------------------------------------------------------
// Раны
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WoundKind {
    /// Рубленая или резаная (спата, сакс, топор).
    Cut,
    /// Колотая (копьё): глубокая, сильнее кровит и чаще гноится.
    Stab,
    /// Стрела. Наконечник может остаться в ране.
    Arrow,
    /// Ожог (огонь, кипящее масло, негашёная известь).
    Burn,
}

impl WoundKind {
    pub fn name(self) -> &'static str {
        match self {
            WoundKind::Cut => "Рубленая рана",
            WoundKind::Stab => "Колотая рана",
            WoundKind::Arrow => "Рана от стрелы",
            WoundKind::Burn => "Ожог",
        }
    }

    fn bleed_factor(self) -> f32 {
        match self {
            WoundKind::Cut => 1.0,
            WoundKind::Stab => 1.35,
            WoundKind::Arrow => 0.9,
            WoundKind::Burn => 0.0,
        }
    }

    fn infection_factor(self) -> f32 {
        match self {
            WoundKind::Cut => 1.0,
            WoundKind::Stab => 1.5,
            WoundKind::Arrow => 1.3,
            WoundKind::Burn => 1.4,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Wound {
    pub id: u32,
    pub kind: WoundKind,
    pub head: bool,
    /// Глубина 0..1. Уменьшается при заживлении.
    pub severity: f32,
    /// Глубина при получении (кровотечение слабеет вместе с заживлением).
    pub initial: f32,
    /// Кровотечение сразу после удара, здоровья в секунду.
    pub bleed0: f32,
    /// Сколько секунд прошло с удара (или с извлечения наконечника).
    pub age: f32,
    /// Перевязана.
    pub dressed: bool,
    /// Множитель кровотечения от кровоостанавливающих (1 — ничего не приложено).
    pub styptic: f32,
    /// Грязь в ране 0..1: промывание вином или уксусом её убирает.
    pub dirt: f32,
    /// Противогнилостное (мирра, ладан, мёд, мазь) 0..1, со временем выветривается.
    pub antiseptic: f32,
    /// Наконечник остался внутри: рана не заживает и гноится.
    pub arrowhead: bool,
    /// Воспаление 0..1. 1 — антонов огонь.
    pub inflammation: f32,
    /// Множитель заживления (окопник, мазь).
    pub healing: f32,
    /// Ожог смягчён маслом или мальвой 0..1.
    pub soothed: f32,
}

impl Wound {
    /// Текущее кровотечение, здоровья в секунду.
    pub fn bleeding(&self) -> f32 {
        if self.kind == WoundKind::Burn {
            return 0.0;
        }
        // Мелкие раны затягиваются сами, глубокие — нет (порванная жила).
        let floor = ((self.initial - 0.3) / 0.7).clamp(0.0, 1.0) * 0.55;
        let natural = (-self.age / CLOT_SECS).exp().max(floor);
        let dressing = if self.dressed { 0.35 } else { 1.0 };
        let shrink = (self.severity / self.initial.max(0.01)).clamp(0.0, 1.0);
        self.bleed0 * natural * dressing * self.styptic * shrink
    }

    pub fn pain(&self) -> f32 {
        let base = match self.kind {
            WoundKind::Burn => self.severity * 0.8 * (1.0 - self.soothed * 0.6),
            _ => self.severity * 0.45,
        };
        base + self.inflammation * 0.35 + if self.arrowhead { 0.3 } else { 0.0 }
    }

    /// Нужна ли ещё помощь (для выбора, что лечить).
    fn urgency(&self) -> f32 {
        self.bleeding() * 10.0 + self.inflammation * 2.0 + if self.arrowhead { 3.0 } else { 0.0 } + self.severity * (1.0 - self.antiseptic) * (if self.dressed { 0.2 } else { 1.0 })
    }

    fn tick(&mut self, dt: f32) {
        self.age += dt;
        self.antiseptic *= half_life(dt, ANTISEPTIC_HALF_LIFE);
        // Воспаление: после «спокойного» срока, если в ране грязь или железо.
        if self.age > INCUBATION_SECS {
            let dirt = if self.arrowhead { self.dirt.max(0.8) } else { self.dirt };
            let small = self.initial < 0.25 && dirt < 0.5 && !self.arrowhead;
            let mut grow = if small { 0.0 } else { INFLAME_RATE * dirt * self.kind.infection_factor() * (0.5 + self.severity) };
            if self.arrowhead {
                grow *= 2.2;
            }
            grow *= 1.0 - self.antiseptic * 0.9;
            let fall = 0.003 * self.antiseptic + if dirt < 0.3 { 0.0008 } else { 0.0 };
            self.inflammation = (self.inflammation + (grow - fall) * dt).clamp(0.0, 1.0);
        }
        // Заживление: когда кровь остановилась, наконечника нет и рана не горит.
        if self.bleeding() < 0.05 && !self.arrowhead && self.inflammation < 0.4 {
            let rate = HEAL_RATE * if self.dressed { 1.5 } else { 1.0 } * self.healing * (1.0 - self.inflammation);
            self.severity = (self.severity - rate * dt).max(0.0);
        }
    }
}

fn half_life(dt: f32, half: f32) -> f32 {
    (-(dt / half) * std::f32::consts::LN_2).exp()
}

/// Попадание по игроку (заполняет server.rs, раны делает medicine_server.rs).
#[derive(Debug, Clone, Copy)]
pub struct HitTaken {
    pub target: u64,
    pub weapon: crate::combat::WeaponKind,
    pub amount: f32,
    pub head: bool,
}

impl HitTaken {
    pub fn wound_kind(&self) -> WoundKind {
        use crate::combat::WeaponKind as W;
        match self.weapon {
            W::Spatha | W::Axe | W::Seax => WoundKind::Cut,
            W::Spear => WoundKind::Stab,
            W::Bow => WoundKind::Arrow,
        }
    }
}

// ---------------------------------------------------------------------------
// Яды
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Toxin {
    /// Опий и маковый сок: снимает боль, клонит в сон, в избытке останавливает дыхание.
    Opium,
    /// Белена: немного снимает боль, но сушит рот, гонит сердце и мутит рассудок.
    Henbane,
    /// Ртуть и её пары: металлический вкус, дрожь. Выводится очень долго.
    Mercury,
    /// Медная зелень (ярь-медянка, купорос): рвота, колики.
    Copper,
    /// Свинец (белила, сурик): колики, слабость, выводится долго.
    Lead,
    /// Едкое (щёлок, серная вода, известь): жжёт рот и горло.
    Caustic,
    /// Нефть: тошнота, кашель.
    Naphtha,
}

impl Toxin {
    /// Какое вещество отравляет (для диагноза по знанию свойств).
    pub fn substance(self) -> Substance {
        match self {
            Toxin::Opium => Substance::Opium,
            Toxin::Henbane => Substance::Henbane,
            Toxin::Mercury => Substance::Cinnabar,
            Toxin::Copper => Substance::CopperVitriol,
            Toxin::Lead => Substance::Lead,
            Toxin::Caustic => Substance::Quicklime,
            Toxin::Naphtha => Substance::Naphtha,
        }
    }

    /// (до какой дозы вреда нет, вред в секунду на дозу сверх неё, полувыведение в секундах).
    fn params(self) -> (f32, f32, f32) {
        match self {
            Toxin::Opium => (1.2, 0.6, 90.0),
            Toxin::Henbane => (0.6, 0.8, 120.0),
            Toxin::Mercury => (0.2, 0.3, 900.0),
            Toxin::Copper => (0.3, 0.5, 200.0),
            Toxin::Lead => (0.3, 0.25, 1200.0),
            Toxin::Caustic => (0.0, 1.5, 30.0),
            Toxin::Naphtha => (0.3, 0.4, 120.0),
        }
    }

    pub fn symptom(self) -> &'static str {
        match self {
            Toxin::Opium => "Клонит в сон, зрачки сузились",
            Toxin::Henbane => "Сухо во рту, сердце колотится, всё плывёт",
            Toxin::Mercury => "Металлический вкус, дрожат руки",
            Toxin::Copper => "Тошнит, рвёт зеленью",
            Toxin::Lead => "Колики в животе, слабость",
            Toxin::Caustic => "Жжёт рот и горло",
            Toxin::Naphtha => "Тошнота и кашель",
        }
    }

    pub fn diagnosis(self) -> &'static str {
        match self {
            Toxin::Opium => "Опийное опьянение",
            Toxin::Henbane => "Отравление беленой",
            Toxin::Mercury => "Отравление ртутью",
            Toxin::Copper => "Отравление медью",
            Toxin::Lead => "Отравление свинцом",
            Toxin::Caustic => "Ожог едким",
            Toxin::Naphtha => "Отравление нефтью",
        }
    }
}

// ---------------------------------------------------------------------------
// Тело
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Body {
    pub wounds: Vec<Wound>,
    /// Яды в крови, в дозах.
    pub toxins: BTreeMap<Toxin, f32>,
    /// Жаропонижающее (оксимель, шалфей) 0..1.
    pub antipyretic: f32,
    /// Действует противоядие (териак) 0..1.
    pub antidote: f32,
    next_id: u32,
}

/// Как недуги меняют бойца (множители к тому, что дала экипировка).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyMods {
    pub action_speed: f32,
    pub stamina_regen: f32,
    pub max_stamina: f32,
    pub move_mult: f32,
}

impl Default for BodyMods {
    fn default() -> Self {
        Self { action_speed: 1.0, stamina_regen: 1.0, max_stamina: 1.0, move_mult: 1.0 }
    }
}

impl Body {
    pub fn is_healthy(&self) -> bool {
        self.wounds.is_empty() && self.toxins.is_empty()
    }

    /// Попадание оружием. `damage` — потерянное здоровье, `max_hp` — для оценки тяжести.
    /// `roll` 0..1 — случай (наконечник застрял, если меньше 0,7).
    pub fn hit(&mut self, kind: WoundKind, damage: f32, max_hp: f32, head: bool, roll: f32) -> Option<u32> {
        let severity = (damage / (max_hp * 0.3)).clamp(0.0, 1.0);
        if severity < 0.04 {
            return None;
        }
        let arrowhead = kind == WoundKind::Arrow && roll < 0.7;
        Some(self.add(Wound {
            id: 0,
            kind,
            head,
            severity,
            initial: severity,
            bleed0: 1.1 * severity * kind.bleed_factor() * if head { 1.2 } else { 1.0 },
            age: 0.0,
            dressed: false,
            styptic: 1.0,
            dirt: if kind == WoundKind::Burn { 0.5 } else { 1.0 },
            antiseptic: 0.0,
            arrowhead,
            inflammation: 0.0,
            healing: 1.0,
            soothed: 0.0,
        }))
    }

    /// Ожог (огонь, кипяток, известь). `severity` 0..1.
    pub fn burn(&mut self, severity: f32) -> u32 {
        let severity = severity.clamp(0.05, 1.0);
        self.add(Wound {
            id: 0,
            kind: WoundKind::Burn,
            head: false,
            severity,
            initial: severity,
            bleed0: 0.0,
            age: 0.0,
            dressed: false,
            styptic: 1.0,
            dirt: 0.5,
            antiseptic: 0.0,
            arrowhead: false,
            inflammation: 0.0,
            healing: 1.0,
            soothed: 0.0,
        })
    }

    fn add(&mut self, mut w: Wound) -> u32 {
        self.next_id += 1;
        w.id = self.next_id;
        self.wounds.push(w);
        self.next_id
    }

    pub fn poison(&mut self, toxin: Toxin, doses: f32) {
        if doses > 0.0 {
            *self.toxins.entry(toxin).or_insert(0.0) += doses;
        }
    }

    pub fn toxin(&self, t: Toxin) -> f32 {
        self.toxins.get(&t).copied().unwrap_or(0.0)
    }

    pub fn bleeding(&self) -> f32 {
        self.wounds.iter().map(Wound::bleeding).sum()
    }

    /// Жар 0..1: от воспалённых ран.
    pub fn fever(&self) -> f32 {
        let raw = self.wounds.iter().map(|w| ((w.inflammation - 0.45) / 0.55).clamp(0.0, 1.0)).fold(0.0, f32::max);
        (raw * (1.0 - self.antipyretic * 0.7)).clamp(0.0, 1.0)
    }

    /// Обезболивание 0..1 от опия и белены.
    pub fn analgesia(&self) -> f32 {
        (self.toxin(Toxin::Opium).min(1.2) * 0.6 + self.toxin(Toxin::Henbane).min(1.0) * 0.25).min(0.85)
    }

    /// Боль 0..1 с учётом обезболивания.
    pub fn pain(&self) -> f32 {
        let raw: f32 = self.wounds.iter().map(Wound::pain).sum::<f32>() + self.toxin(Toxin::Caustic).min(1.0) * 0.5;
        (raw.min(1.0) * (1.0 - self.analgesia())).clamp(0.0, 1.0)
    }

    /// Самая нуждающаяся в помощи рана.
    pub fn worst_wound(&self) -> Option<&Wound> {
        self.wounds.iter().max_by(|a, b| a.urgency().total_cmp(&b.urgency()))
    }

    pub fn wound_mut(&mut self, id: u32) -> Option<&mut Wound> {
        self.wounds.iter_mut().find(|w| w.id == id)
    }

    /// Течение болезни за `dt`. Возвращает потерю здоровья.
    pub fn tick(&mut self, dt: f32) -> f32 {
        let mut loss = self.bleeding() * dt;
        for w in &mut self.wounds {
            w.tick(dt);
            if w.inflammation >= 0.999 {
                // Антонов огонь: заражение крови.
                loss += 0.25 * dt;
            }
        }
        self.wounds.retain(|w| w.severity > 0.02 || w.arrowhead || w.inflammation > 0.05);
        loss += self.fever() * 0.08 * dt;

        let antidote = self.antidote;
        for (t, amount) in self.toxins.iter_mut() {
            let (safe, harm, half) = t.params();
            let harm = harm * if antidote > 0.05 { 0.6 } else { 1.0 };
            loss += (*amount - safe).max(0.0) * harm * dt;
            let half = if antidote > 0.05 { half / 2.0 } else { half };
            *amount *= half_life(dt, half);
        }
        self.toxins.retain(|_, a| *a > 0.02);
        self.antipyretic *= half_life(dt, ANTIPYRETIC_HALF_LIFE);
        self.antidote *= half_life(dt, ANTIDOTE_HALF_LIFE);
        if self.antidote < 0.02 {
            self.antidote = 0.0;
        }
        loss
    }

    pub fn mods(&self) -> BodyMods {
        let pain = self.pain();
        let fever = self.fever();
        let sleepy = (self.toxin(Toxin::Opium) / 2.5).min(1.0);
        let mad = (self.toxin(Toxin::Henbane) / 1.5).min(1.0);
        BodyMods {
            action_speed: ((1.0 - 0.22 * pain) * (1.0 - 0.3 * sleepy) * (1.0 - 0.25 * mad)).max(0.45),
            stamina_regen: ((1.0 - 0.3 * pain) * (1.0 - 0.5 * fever)).max(0.3),
            max_stamina: (1.0 - 0.3 * fever).max(0.5),
            move_mult: ((1.0 - 0.25 * sleepy) * (1.0 - 0.15 * mad) * if pain > 0.8 { 0.88 } else { 1.0 }).max(0.5),
        }
    }
}

// ---------------------------------------------------------------------------
// Лечебные средства
// ---------------------------------------------------------------------------

/// Что вещество делает с раной.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WoundCare {
    pub verb: &'static str,
    pub secs: f32,
    /// Это повязка.
    pub dress: bool,
    /// Припарка из свежих листьев: держится только под повязкой.
    pub poultice: bool,
    /// Множитель кровотечения (1 — не влияет).
    pub styptic: f32,
    /// Сколько грязи остаётся после промывания (1 — не промывает).
    pub wash: f32,
    /// Противогнилостное.
    pub antiseptic: f32,
    /// Множитель заживления.
    pub heal: f32,
    /// Смягчает ожог.
    pub soothe: f32,
    /// Каким приёмом это делают умело.
    pub technique: Technique,
}

const CARE: WoundCare = WoundCare {
    verb: "Перевязываете",
    secs: 4.0,
    dress: false,
    poultice: false,
    styptic: 1.0,
    wash: 1.0,
    antiseptic: 0.0,
    heal: 1.0,
    soothe: 0.0,
    technique: Technique::Poultice,
};

/// Что вещество делает, если его выпить или съесть.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Intake {
    pub verb: &'static str,
    pub secs: f32,
    /// Яды (и лекарства, которые в избытке — яд) в дозах.
    pub toxins: &'static [(Toxin, f32)],
    pub antipyretic: f32,
    pub antidote: f32,
    /// Без знания дозы человек берёт на глаз: от ½ до почти двойной.
    pub needs_dosage: bool,
}

const INTAKE: Intake = Intake { verb: "Пьёте", secs: 2.0, toxins: &[], antipyretic: 0.0, antidote: 0.0, needs_dosage: false };

/// Средство по имени предмета: (на рану, внутрь). Значения взяты из Диоскорида, Цельса и Галена
/// и сжаты под игру; там, где древние ошибались (териак от всех ядов), действие скромнее.
pub fn remedy(item: &str) -> (Option<WoundCare>, Option<Intake>) {
    use Technique::*;
    let w = |c: WoundCare| Some(c);
    let i = |c: Intake| Some(c);
    match item {
        "Бинт" => (w(WoundCare { verb: "Перевязываете рану", dress: true, technique: Poultice, ..CARE }), None),
        // Свежие листья под повязку.
        "Подорожник" => (w(WoundCare { verb: "Прикладываете размятый подорожник", poultice: true, styptic: 0.7, antiseptic: 0.15, heal: 1.2, ..CARE }), None),
        "Тысячелистник" => (w(WoundCare { verb: "Прикладываете тысячелистник", poultice: true, styptic: 0.5, antiseptic: 0.1, ..CARE }), None),
        "Корень окопника" => (w(WoundCare { verb: "Прикладываете тёртый окопник", poultice: true, heal: 1.6, ..CARE }), None),
        "Просвирник" => (w(WoundCare { verb: "Прикладываете распаренную мальву", poultice: true, soothe: 0.5, antiseptic: 0.1, ..CARE }), None),
        "Зверобой" => (w(WoundCare { verb: "Прикладываете зверобой", poultice: true, antiseptic: 0.2, soothe: 0.2, ..CARE }), None),
        // Порошки и смолы прямо в рану.
        "Квасцы" => (w(WoundCare { verb: "Присыпаете рану квасцами", secs: 3.0, styptic: 0.3, antiseptic: 0.2, technique: Salve, ..CARE }), None),
        "Ладан" => (w(WoundCare { verb: "Присыпаете рану толчёным ладаном", secs: 3.0, styptic: 0.65, antiseptic: 0.35, technique: Salve, ..CARE }), None),
        "Мирра" => (w(WoundCare { verb: "Кладёте в рану мирру", secs: 3.0, antiseptic: 0.5, heal: 1.2, technique: Salve, ..CARE }), None),
        "Горшок мёда" => (w(WoundCare { verb: "Смазываете рану мёдом", secs: 3.0, antiseptic: 0.55, heal: 1.2, soothe: 0.3, technique: WoundWashing, ..CARE }), None),
        "Кувшин оливкового масла" => (w(WoundCare { verb: "Смазываете ожог маслом", secs: 3.0, soothe: 0.4, technique: Salve, ..CARE }), None),
        // Промывание.
        "Кувшин вина" | "Амфора вина" => (w(WoundCare { verb: "Промываете рану вином", secs: 3.0, wash: 0.25, antiseptic: 0.15, technique: WoundWashing, ..CARE }), None),
        "Винный уксус" => (w(WoundCare { verb: "Промываете рану уксусом", secs: 3.0, wash: 0.3, antiseptic: 0.2, styptic: 0.85, technique: WoundWashing, ..CARE }), None),
        // Готовые снадобья.
        "Кровоостанавливающая припарка" => (w(WoundCare { verb: "Прикладываете припарку", poultice: true, styptic: 0.2, antiseptic: 0.2, ..CARE }), None),
        "Ранозаживляющая мазь" => (w(WoundCare { verb: "Накладываете мазь", antiseptic: 0.7, heal: 1.8, soothe: 0.4, technique: Salve, ..CARE }), None),
        "Зверобойное масло" => (w(WoundCare { verb: "Смазываете зверобойным маслом", secs: 3.0, antiseptic: 0.3, heal: 1.3, soothe: 0.8, technique: Maceration, ..CARE }), None),
        // Внутрь.
        "Отвар шалфея" => (None, i(Intake { verb: "Пьёте отвар шалфея", antipyretic: 0.3, ..INTAKE })),
        "Отвар чабреца" => (None, i(Intake { verb: "Пьёте отвар чабреца", antipyretic: 0.25, ..INTAKE })),
        "Оксимель" => (None, i(Intake { verb: "Пьёте оксимель", antipyretic: 0.55, ..INTAKE })),
        "Маковый отвар" => (None, i(Intake { verb: "Пьёте маковый отвар", toxins: &[(Toxin::Opium, 0.9)], needs_dosage: true, ..INTAKE })),
        "Маковые коробочки" => (None, i(Intake { verb: "Жуёте маковые коробочки", secs: 3.0, toxins: &[(Toxin::Opium, 0.35)], ..INTAKE })),
        "Опий" => (None, i(Intake { verb: "Глотаете кусочек опия", secs: 1.5, toxins: &[(Toxin::Opium, 1.4)], needs_dosage: true, ..INTAKE })),
        "Белена" => (None, i(Intake { verb: "Жуёте белену", secs: 3.0, toxins: &[(Toxin::Henbane, 1.2)], ..INTAKE })),
        "Териак" => (None, i(Intake { verb: "Принимаете териак", secs: 3.0, toxins: &[(Toxin::Opium, 0.3)], antidote: 1.0, needs_dosage: true, ..INTAKE })),
        // То, что пить нельзя, но игрок может попробовать.
        // Жидкая ртуть в желудке почти не всасывается; опасны пары (возгонка).
        "Ртуть" => (None, i(Intake { verb: "Глотаете ртуть", secs: 1.5, toxins: &[(Toxin::Mercury, 0.35)], ..INTAKE })),
        "Ярь-медянка" | "Медный купорос" => (None, i(Intake { verb: "Глотаете зелёный порошок", secs: 1.5, toxins: &[(Toxin::Copper, 1.2)], ..INTAKE })),
        "Свинцовые белила" | "Сурик" => (None, i(Intake { verb: "Глотаете порошок", secs: 1.5, toxins: &[(Toxin::Lead, 1.0)], ..INTAKE })),
        "Щёлок" | "Серная вода" => (None, i(Intake { verb: "Отпиваете", secs: 1.5, toxins: &[(Toxin::Caustic, 1.0)], ..INTAKE })),
        "Нефть" => (None, i(Intake { verb: "Отпиваете нефть", secs: 1.5, toxins: &[(Toxin::Naphtha, 1.2)], ..INTAKE })),
        // Неудачная варка: в лучшем случае ничего, в худшем — тошнит.
        "Испорченное варево" => (None, i(Intake { verb: "Пьёте мутное варево", toxins: &[(Toxin::Copper, 0.2)], ..INTAKE })),
        _ => (None, None),
    }
}

/// Лечебное ли (или опасное для питья) это вещество.
pub fn is_remedy(item: &str) -> bool {
    let (a, b) = remedy(item);
    a.is_some() || b.is_some()
}

// ---------------------------------------------------------------------------
// Применение
// ---------------------------------------------------------------------------

/// Чем закончилось применение средства.
#[derive(Debug, Clone, PartialEq)]
pub enum Treated {
    /// Рана обработана. `bandage_used` — к припарке ушёл бинт.
    Wound { text: String, bandage_used: bool },
    Drunk { text: String },
    /// Вынут наконечник (или не вышло: тогда `ok` false и рана стала хуже).
    Extraction { ok: bool, damage: f32, text: String },
    /// Ничего не сделано, предмет не тратится.
    Nothing(String),
}

impl Treated {
    pub fn consumes_item(&self) -> bool {
        matches!(self, Treated::Wound { .. } | Treated::Drunk { .. })
    }
}

/// Умелость применения 0.6..1.3: знание приёма и навык врачевания.
pub fn care_skill(k: &Knowledge, technique: Technique) -> f32 {
    let known = if k.knows(Lore::Technique(technique)) { 1.0 } else { 0.6 };
    known + k.skill(Skill::Medicine) / 100.0 * 0.3
}

/// Сила готового снадобья по качеству (его сделал мастер с таким-то навыком).
pub fn potency(quality_index: usize) -> f32 {
    [0.6, 0.85, 1.0, 1.15, 1.3, 1.45][quality_index.min(5)]
}

/// Что сделает применение (для выбора длительности и подписи до начала).
pub fn plan(body: &Body, item: &str) -> Option<(&'static str, f32)> {
    let (care, intake) = remedy(item);
    if let Some(c) = care
        && let Some(w) = body.worst_wound()
    {
        if w.arrowhead && w.kind == WoundKind::Arrow {
            return Some(("Вынимаете наконечник", 5.0));
        }
        if !(c.dress && w.dressed && w.bleeding() < 0.05) {
            return Some((c.verb, c.secs));
        }
    }
    intake.map(|i| (i.verb, i.secs))
}

/// Применить средство. `has_bandage` — есть ли в сумке бинт, чтобы прибинтовать припарку.
/// `roll` 0..1 — случай (неудача при извлечении, доза на глаз).
pub fn apply(body: &mut Body, k: &Knowledge, item: &str, quality_index: usize, has_bandage: bool, roll: f32) -> Treated {
    let (care, intake) = remedy(item);
    let wound = body.worst_wound().map(|w| w.id);
    if let (Some(c), Some(id)) = (care, wound) {
        let w = body.wound_mut(id).expect("рана есть");
        if w.arrowhead && w.kind == WoundKind::Arrow {
            return extract(w, k, roll);
        }
        if !(c.dress && w.dressed && w.bleeding() < 0.05) {
            return care_for(w, k, c, quality_index, has_bandage);
        }
    }
    if let Some(i) = intake {
        return drink(body, k, i, quality_index, roll);
    }
    if care.is_some() {
        return Treated::Nothing("Нечего лечить".into());
    }
    Treated::Nothing("Это не лекарство".into())
}

fn extract(w: &mut Wound, k: &Knowledge, roll: f32) -> Treated {
    let knows = k.knows(Lore::Technique(Technique::ArrowExtraction));
    let chance = (0.3 + if knows { 0.45 } else { 0.0 } + k.skill(Skill::Medicine) / 100.0 * 0.4).min(0.95);
    if roll < chance {
        w.arrowhead = false;
        // Рана снова открывается и кровит, но теперь может зажить.
        w.age = 0.0;
        w.bleed0 *= 1.15;
        w.dirt = w.dirt.max(0.8);
        Treated::Extraction { ok: true, damage: 2.0, text: "Наконечник вынут. Рана снова кровит: промойте и перевяжите".into() }
    } else {
        w.severity = (w.severity + 0.08).min(1.0);
        w.initial = w.initial.max(w.severity);
        w.bleed0 += 0.25;
        w.age = 0.0;
        let text = if knows { "Наконечник засел глубоко и не идёт" } else { "Вы рвёте древко, но наконечник остаётся в ране" };
        Treated::Extraction { ok: false, damage: 4.0 + w.severity * 6.0, text: text.into() }
    }
}

fn care_for(w: &mut Wound, k: &Knowledge, c: WoundCare, quality_index: usize, has_bandage: bool) -> Treated {
    let mut q = care_skill(k, c.technique) * potency(quality_index);
    let mut notes = Vec::new();
    let mut bandage_used = false;
    if c.poultice && !w.dressed {
        if has_bandage {
            bandage_used = true;
            w.dressed = true;
            notes.push("прибинтовали");
        } else {
            q *= 0.5;
            notes.push("без повязки припарка держится плохо");
        }
    }
    if c.dress {
        w.dressed = true;
    }
    // Каждое средство приближает кровотечение к своему пределу, а не множится бесконечно.
    if c.styptic < 1.0 {
        let target = 1.0 - (1.0 - c.styptic) * q.min(1.2);
        w.styptic = w.styptic.min(target.clamp(0.05, 1.0));
    }
    if c.wash < 1.0 {
        let left = 1.0 - (1.0 - c.wash) * q.min(1.0);
        // Промывать надо свежую рану: когда уже гноится, вино помогает мало.
        w.dirt = (w.dirt * left).max(0.05);
        if w.inflammation > 0.3 {
            w.inflammation = (w.inflammation - 0.05 * q).max(0.0);
        }
    }
    if c.antiseptic > 0.0 {
        w.antiseptic = w.antiseptic.max((c.antiseptic * q).min(1.0));
    }
    if c.heal > 1.0 {
        w.healing = w.healing.max(1.0 + (c.heal - 1.0) * q);
    }
    if c.soothe > 0.0 && w.kind == WoundKind::Burn {
        w.soothed = w.soothed.max((c.soothe * q).min(1.0));
    }
    let mut text = format!("{}: {}", w.kind.name(), c.verb.to_lowercase());
    if !notes.is_empty() {
        text.push_str(&format!(" ({})", notes.join(", ")));
    }
    Treated::Wound { text, bandage_used }
}

fn drink(body: &mut Body, k: &Knowledge, i: Intake, quality_index: usize, roll: f32) -> Treated {
    let p = potency(quality_index);
    // Доза на глаз, если не учили мерить драхмами.
    let dose = if i.needs_dosage && !k.knows(Lore::Technique(Technique::Dosage)) { 0.5 + roll * 1.4 } else { 1.0 };
    for (t, d) in i.toxins {
        body.poison(*t, d * dose * if *t == Toxin::Opium { p } else { 1.0 });
    }
    if i.antipyretic > 0.0 {
        body.antipyretic = body.antipyretic.max((i.antipyretic * p).min(1.0));
    }
    if i.antidote > 0.0 {
        body.antidote = body.antidote.max((i.antidote * p).min(1.0));
    }
    let text = if dose > 1.35 { format!("{} — кажется, слишком много", i.verb) } else { i.verb.to_string() };
    Treated::Drunk { text }
}

// ---------------------------------------------------------------------------
// Как это видит больной
// ---------------------------------------------------------------------------

/// Насколько тревожна строка (для цвета).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Alarm {
    Mild,
    Serious,
    Deadly,
}

/// Разбирается ли больной в ранах: учился у врача или читал Гиппократа/Цельса.
fn knows_wounds(k: &Knowledge) -> bool {
    k.knows(Lore::Technique(Technique::WoundWashing)) || k.skill(Skill::Medicine) >= 15.0
}

/// Строки состояния для экрана. Без знаний — признаки, со знаниями — диагноз и совет.
pub fn symptoms(body: &Body, k: &Knowledge) -> Vec<(String, Alarm)> {
    let mut out = Vec::new();
    let expert = knows_wounds(k);
    for w in &body.wounds {
        let place = if w.head { " на голове" } else { "" };
        let b = w.bleeding();
        let mut parts: Vec<String> = Vec::new();
        let alarm;
        if w.kind == WoundKind::Burn {
            parts.push(if w.severity > 0.5 { "пузыри, кожа слезает".into() } else { "кожа красная, жжёт".into() });
        }
        if b > 0.6 {
            parts.push("кровь хлещет".into());
        } else if b > 0.2 {
            parts.push("сильно кровит".into());
        } else if b > 0.05 {
            parts.push("сочится кровь".into());
        }
        if w.arrowhead {
            parts.push(if expert { "наконечник в ране — вынуть".into() } else { "внутри что-то застряло".into() });
        }
        if w.inflammation >= 0.95 {
            parts.push(if expert { "антонов огонь".into() } else { "рана почернела и смердит".into() });
        } else if w.inflammation > 0.55 {
            parts.push(if expert { "гноится".into() } else { "течёт гной".into() });
        } else if w.inflammation > 0.25 {
            parts.push(if expert { "воспаляется".into() } else { "горячая, опухла, дёргает".into() });
        }
        if w.dressed {
            parts.push("перевязана".into());
        }
        if expert && w.dirt > 0.6 && w.inflammation < 0.25 && w.age < INCUBATION_SECS && !w.arrowhead && w.kind != WoundKind::Burn {
            parts.push("не промыта".into());
        }
        alarm = if b > 0.6 || w.inflammation >= 0.95 {
            Alarm::Deadly
        } else if b > 0.2 || w.inflammation > 0.55 || w.arrowhead {
            Alarm::Serious
        } else {
            Alarm::Mild
        };
        let head = format!("{}{}", w.kind.name(), place);
        let line = if parts.is_empty() { format!("{head}: затягивается") } else { format!("{head}: {}", parts.join(", ")) };
        out.push((line, alarm));
    }
    let fever = body.fever();
    if fever > 0.05 {
        let a = if fever > 0.6 { Alarm::Serious } else { Alarm::Mild };
        out.push((if expert { format!("Лихорадка{}", if fever > 0.6 { ", сильная" } else { "" }) } else { "Жар и озноб".into() }, a));
    }
    for (t, amount) in &body.toxins {
        let (safe, _, _) = t.params();
        if *t == Toxin::Opium && *amount < 0.6 {
            continue;
        }
        let a = if *amount > safe * 1.8 + 0.4 { Alarm::Deadly } else if *amount > safe { Alarm::Serious } else { Alarm::Mild };
        let named = k.knows(Lore::Properties(t.substance())) || (expert && k.knows(Lore::Technique(Technique::Dosage)));
        out.push((if named { t.diagnosis().to_string() } else { t.symptom().to_string() }, a));
    }
    if body.antidote > 0.05 {
        out.push(("Действует противоядие".into(), Alarm::Mild));
    }
    let pain = body.pain();
    if pain > 0.15 {
        let word = if pain > 0.7 { "Сильная боль" } else if pain > 0.35 { "Боль" } else { "Ноет" };
        out.push((word.into(), Alarm::Mild));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemBaseId;

    fn run(body: &mut Body, secs: f32) -> f32 {
        let mut loss = 0.0;
        let steps = (secs / 0.1) as usize;
        for _ in 0..steps {
            loss += body.tick(0.1);
        }
        loss
    }

    #[test]
    fn remedy_items_exist() {
        let names = [
            "Бинт",
            "Подорожник",
            "Тысячелистник",
            "Корень окопника",
            "Просвирник",
            "Зверобой",
            "Квасцы",
            "Ладан",
            "Мирра",
            "Горшок мёда",
            "Кувшин оливкового масла",
            "Кувшин вина",
            "Амфора вина",
            "Винный уксус",
            "Кровоостанавливающая припарка",
            "Ранозаживляющая мазь",
            "Зверобойное масло",
            "Отвар шалфея",
            "Отвар чабреца",
            "Оксимель",
            "Маковый отвар",
            "Маковые коробочки",
            "Опий",
            "Белена",
            "Териак",
            "Ртуть",
            "Ярь-медянка",
            "Медный купорос",
            "Свинцовые белила",
            "Сурик",
            "Щёлок",
            "Серная вода",
            "Нефть",
        ];
        for n in names {
            assert!(ItemBaseId::find(n).is_some(), "нет предмета {n}");
            assert!(is_remedy(n), "{n} не лекарство");
        }
    }

    #[test]
    fn small_cut_stops_bleeding_by_itself() {
        let mut b = Body::default();
        b.hit(WoundKind::Cut, 8.0, 150.0, false, 0.9);
        assert!(b.bleeding() > 0.1);
        run(&mut b, 90.0);
        assert!(b.bleeding() < 0.02, "кровь {:.3}", b.bleeding());
    }

    #[test]
    fn deep_wound_keeps_bleeding_until_treated() {
        let mut b = Body::default();
        b.hit(WoundKind::Stab, 40.0, 150.0, false, 0.9);
        run(&mut b, 90.0);
        let before = b.bleeding();
        assert!(before > 0.3, "кровь {before:.3}");
        let k = Knowledge::default();
        // Квасцы работают и у неуча: вещество действует само.
        assert!(matches!(apply(&mut b, &k, "Квасцы", 1, false, 0.5), Treated::Wound { .. }));
        assert!(apply(&mut b, &k, "Бинт", 1, false, 0.5).consumes_item());
        assert!(b.bleeding() < before * 0.3, "{:.3} -> {:.3}", before, b.bleeding());
    }

    #[test]
    fn dirty_wound_inflames_and_washing_prevents_it() {
        let mut dirty = Body::default();
        dirty.hit(WoundKind::Cut, 30.0, 150.0, false, 0.9);
        let mut clean = dirty.clone();
        let mut k = Knowledge::default();
        k.learn(Lore::Technique(Technique::WoundWashing));
        apply(&mut clean, &k, "Кувшин вина", 1, false, 0.5);
        apply(&mut clean, &k, "Бинт", 1, false, 0.5);
        apply(&mut dirty, &k, "Бинт", 1, false, 0.5);
        run(&mut dirty, 300.0);
        run(&mut clean, 300.0);
        let (d, c) = (dirty.wounds[0].inflammation, clean.wounds.first().map_or(0.0, |w| w.inflammation));
        assert!(d > 0.3, "грязная {d:.2}");
        assert!(c < d * 0.5, "чистая {c:.2} грязная {d:.2}");
        assert!(dirty.fever() >= 0.0);
    }

    #[test]
    fn arrowhead_must_come_out_before_dressing() {
        let mut b = Body::default();
        b.hit(WoundKind::Arrow, 30.0, 150.0, false, 0.1);
        assert!(b.wounds[0].arrowhead);
        let mut k = Knowledge::default();
        k.learn(Lore::Technique(Technique::ArrowExtraction));
        let r = apply(&mut b, &k, "Бинт", 1, false, 0.2);
        assert!(matches!(r, Treated::Extraction { ok: true, .. }), "{r:?}");
        assert!(!r.consumes_item());
        assert!(!b.wounds[0].arrowhead);
        // Неуч часто только рвёт рану.
        let mut b2 = Body::default();
        b2.hit(WoundKind::Arrow, 30.0, 150.0, false, 0.1);
        let r = apply(&mut b2, &Knowledge::default(), "Бинт", 1, false, 0.5);
        assert!(matches!(r, Treated::Extraction { ok: false, .. }), "{r:?}");
        assert!(b2.wounds[0].arrowhead);
    }

    #[test]
    fn poultice_needs_bandage() {
        let mut a = Body::default();
        a.hit(WoundKind::Cut, 40.0, 150.0, false, 0.9);
        let mut b = a.clone();
        let k = Knowledge::default();
        let ra = apply(&mut a, &k, "Тысячелистник", 1, true, 0.5);
        let rb = apply(&mut b, &k, "Тысячелистник", 1, false, 0.5);
        assert!(matches!(ra, Treated::Wound { bandage_used: true, .. }));
        assert!(matches!(rb, Treated::Wound { bandage_used: false, .. }));
        assert!(a.bleeding() < b.bleeding());
    }

    #[test]
    fn opium_kills_pain_but_overdose_hurts() {
        let mut b = Body::default();
        b.hit(WoundKind::Stab, 45.0, 150.0, false, 0.9);
        let pain = b.pain();
        let mut k = Knowledge::default();
        k.learn(Lore::Technique(Technique::Dosage));
        apply(&mut b, &k, "Маковый отвар", 1, false, 0.5);
        assert!(b.pain() < pain * 0.6, "{pain:.2} -> {:.2}", b.pain());
        // Правильная доза почти не вредит, большой кусок опия на глаз — опасен.
        let mut safe = Body::default();
        apply(&mut safe, &k, "Маковый отвар", 1, false, 0.5);
        let mut greedy = Body::default();
        apply(&mut greedy, &Knowledge::default(), "Опий", 1, false, 0.99);
        apply(&mut greedy, &Knowledge::default(), "Опий", 1, false, 0.99);
        let (ls, lg) = (run(&mut safe, 60.0), run(&mut greedy, 60.0));
        assert!(ls < 1.0, "{ls}");
        assert!(lg > 20.0, "{lg}");
        assert!(greedy.mods().action_speed < 0.8);
    }

    #[test]
    fn theriac_speeds_up_clearing_poison() {
        let mut a = Body::default();
        a.poison(Toxin::Henbane, 2.0);
        let mut b = a.clone();
        apply(&mut b, &Knowledge::default(), "Териак", 2, false, 0.4);
        let (la, lb) = (run(&mut a, 120.0), run(&mut b, 120.0));
        assert!(lb < la, "{lb} < {la}");
    }

    #[test]
    fn symptoms_need_knowledge_for_names() {
        let mut b = Body::default();
        b.poison(Toxin::Henbane, 1.0);
        let k = Knowledge::default();
        let lines = symptoms(&b, &k);
        assert!(lines.iter().any(|(s, _)| s.contains("Сухо во рту")));
        let mut k2 = Knowledge::default();
        k2.learn(Lore::Properties(Substance::Henbane));
        assert!(symptoms(&b, &k2).iter().any(|(s, _)| s.contains("беленой")));
    }

    #[test]
    fn wounds_heal_and_disappear() {
        let mut b = Body::default();
        b.hit(WoundKind::Cut, 20.0, 150.0, false, 0.9);
        let mut k = Knowledge::default();
        k.learn(Lore::Technique(Technique::WoundWashing));
        apply(&mut b, &k, "Кувшин вина", 1, false, 0.5);
        apply(&mut b, &k, "Бинт", 1, false, 0.5);
        run(&mut b, 600.0);
        assert!(b.wounds.is_empty(), "{:?}", b.wounds);
    }

    #[test]
    fn untreated_sepsis_kills() {
        let mut b = Body::default();
        b.hit(WoundKind::Arrow, 35.0, 150.0, false, 0.1);
        let loss = run(&mut b, 900.0);
        assert!(loss > 150.0, "{loss}");
    }
}
