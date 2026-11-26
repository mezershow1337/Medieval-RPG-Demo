//! Откуда кузнецу руда и уголь: горное дело и углежжение в округе Херсона около 510 года.
//! Чистая модель без Bevy-систем и сети — её шагают сервер (честный расчёт) и тесты.
//!
//! Как это было на самом деле (и как сделано в игре):
//! - **Болотная руда.** В балке ниже родника вода сочится сквозь железистые глины: на ней
//!   радужная плёнка, дно рыжее, под дёрном — бурые ноздреватые комья (лимонит). Её копают
//!   заступом с поверхности; железа в ней меньше, чем в горной руде, и она «хладноломкая»
//!   (фосфор), зато доступна каждому.
//! - **Бурый железняк.** На склоне в предгорьях выходит жила охристого камня. Её бьют кайлом
//!   с поверхности, а глубже ведут короткую штольню: каждые полтора локтя — крепь из двух
//!   стоек и верхняка (Плиний, XXXIII, 70: галереи держат «арками и брёвнами»). Без знания
//!   крепи десятник в забой не пустит. Шурф с воротом: руду поднимают бадьёй, крутят ворот
//!   нанятые работники — за поденную плату.
//! - **Рудознатство.** Новичок не отличит руду от пустой породы: накопает рыжей глины и
//!   камня. Знающий берёт тяжёлые куски, у которых черта по камню бурая.
//! - **Обжиг руды.** Перед плавкой руду обжигают на кострище: уходит вода и сера, руда
//!   буреет, трескается, лучше дробится и плавится. Перегреть — спечётся в шлаковую корку.
//! - **Углежжение** (Феофраст, «Исследование о растениях», V, 9). Поленья дуба ставят плотно
//!   вокруг кола, кучу кроют листвой, дёрном и землёй, поджигают сверху и правят воздухом
//!   через отдушины. Дым сначала белый (выходит вода), потом жёлтый и густой, а когда
//!   становится голубым и прозрачным — уголь готов: отдушины закрыть, кучу заглушить землёй,
//!   дать остыть и разобрать. Много воздуха — уголь сгорит в золу; мало — угаснет, и выйдут
//!   головни-недожоги. Куча «прогорает»: в кровле открываются дыры, их засыпают землёй.
//!
//! Время сжато, как везде в игре: куча горит минуты, а не дни.

use crate::lore::Technique;
use crate::work::Drill;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Предметы
// ---------------------------------------------------------------------------

pub const ORE: &str = crate::smithing::ORE;
pub const BOG_ORE: &str = "Болотная руда";
pub const ROASTED_ORE: &str = "Обожжённая руда";
pub const WASTE: &str = "Пустая порода";
pub const WOOD: &str = "Дрова";
pub const CHARCOAL: &str = crate::smithing::CHARCOAL;
pub const CLAY: &str = crate::smithing::CLAY;
pub const SAND: &str = "Морской песок";
pub const LIMESTONE: &str = "Известняковый блок";
pub const PICK: &str = "Кайло рудокопа";
pub const SPADE: &str = "Заступ";
#[cfg_attr(not(test), allow(dead_code))] // название для проверок; в игре топор узнаётся по `Tool::Axe`
pub const WOOD_AXE: &str = "Топор дровосека";
/// Книга об углежжении (Феофраст, V, 9).
#[cfg_attr(not(test), allow(dead_code))] // название для проверок
pub const THEOPHRASTUS_PLANTS: &str = "Феофраст. Исследование о растениях";

/// Какой руду кладут в сыродутный горн. Чем богаче, тем больше крица (множитель)
/// и чище железо (прибавка к чистоте).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OreGrade {
    Bog,
    Raw,
    Roasted,
}

impl OreGrade {
    pub const ALL: [OreGrade; 3] = [OreGrade::Roasted, OreGrade::Raw, OreGrade::Bog];

    pub fn item(self) -> &'static str {
        match self {
            OreGrade::Bog => BOG_ORE,
            OreGrade::Raw => ORE,
            OreGrade::Roasted => ROASTED_ORE,
        }
    }

    /// Сколько железа выходит с куска (сырая горная руда — 1).
    pub fn richness(self) -> f32 {
        match self {
            OreGrade::Bog => 0.8,
            OreGrade::Raw => 1.0,
            OreGrade::Roasted => 1.25,
        }
    }

    /// Прибавка к чистоте крицы.
    pub fn purity(self) -> f32 {
        match self {
            OreGrade::Bog => -0.05,
            OreGrade::Raw => 0.0,
            OreGrade::Roasted => 0.06,
        }
    }
}

/// Средняя богатость закладки (×100), если кладут `counts` кусков каждого сорта.
pub fn charge_richness(counts: &[(OreGrade, u32)]) -> u8 {
    let n: u32 = counts.iter().map(|(_, c)| c).sum();
    if n == 0 {
        return 100;
    }
    let r: f32 = counts.iter().map(|(g, c)| g.richness() * *c as f32).sum::<f32>() / n as f32;
    (r * 100.0).round().clamp(50.0, 150.0) as u8
}

/// Прибавка к чистоте по средней богатости (×100).
pub fn richness_purity(rich: u8) -> f32 {
    let r = rich as f32 / 100.0;
    if r >= 1.0 { (r - 1.0) / 0.25 * OreGrade::Roasted.purity() } else { (1.0 - r) / 0.2 * OreGrade::Bog.purity() }
}

// ---------------------------------------------------------------------------
// Инструменты
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// Кайло: бить руду и камень.
    Pick,
    /// Заступ: деревянная лопата с железной оковкой.
    Spade,
    /// Любой топор.
    Axe,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pick => "кайло",
            Tool::Spade => "заступ",
            Tool::Axe => "топор",
        }
    }

    /// Годится ли вещь с таким именем.
    pub fn fits(self, item: &str) -> bool {
        let low = item.to_lowercase();
        match self {
            Tool::Pick => item == PICK || low.contains("кирк"),
            Tool::Spade => item == SPADE || low.contains("лопат"),
            Tool::Axe => low.contains("топор") || low.contains("секира"),
        }
    }
}

// ---------------------------------------------------------------------------
// Места добычи
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SiteKind {
    /// Рыжая мочажина в балке: болотная руда под дёрном.
    BogOre,
    /// Выход бурого железняка на склоне.
    Outcrop,
    /// Забой в штольне (с крепью).
    Adit,
    /// Шурф с воротом: крутить ворот за поденную плату.
    Shaft,
    /// Делянка углежогов: колоть и пилить дрова.
    Woodlot,
    /// Глиняный карьер у гончаров.
    ClayPit,
    /// Песок на берегу Песочной бухты.
    Sand,
    /// Каменоломня: известняк для стройки.
    Quarry,
}

pub struct SiteInfo {
    /// Название для знающего.
    pub name: &'static str,
    /// Как видит место новичок (без рудознатства).
    pub plain: &'static str,
    pub verb: &'static str,
    pub tool: Option<Tool>,
    pub drill: Drill,
    /// Секунд работы умелого.
    pub secs: f32,
    /// Сколько раз можно взять до истощения и за сколько секунд прибывает одна порция.
    pub charges: u8,
    pub regrow: f32,
    /// Без этого знания работать не дадут.
    pub needs: Option<Technique>,
}

impl SiteKind {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [SiteKind; 8] = [SiteKind::BogOre, SiteKind::Outcrop, SiteKind::Adit, SiteKind::Shaft, SiteKind::Woodlot, SiteKind::ClayPit, SiteKind::Sand, SiteKind::Quarry];

    pub fn info(self) -> SiteInfo {
        use SiteKind::*;
        let i = |name, plain, verb, tool, drill, secs, charges, regrow, needs| SiteInfo { name, plain, verb, tool, drill, secs, charges, regrow, needs };
        match self {
            BogOre => i("Болотная руда", "Рыжая мочажина", "Режете дёрн, выбираете бурые комья", Some(Tool::Spade), Drill::Pump, 4.5, 4, 110.0, None),
            Outcrop => i("Выход бурого железняка", "Бурые камни на склоне", "Бьёте кайлом по жиле", Some(Tool::Pick), Drill::Rhythm, 4.5, 3, 140.0, None),
            Adit => i("Забой штольни", "Забой штольни", "Отбиваете руду в забое", Some(Tool::Pick), Drill::Rhythm, 5.0, 6, 80.0, Some(Technique::Mining)),
            Shaft => i("Ворот над шурфом", "Ворот над шурфом", "Крутите ворот, поднимаете бадью", None, Drill::Grind, 6.0, 8, 45.0, None),
            Woodlot => i("Делянка", "Делянка", "Колете поленья", Some(Tool::Axe), Drill::Rhythm, 4.0, 5, 100.0, None),
            ClayPit => i("Глиняный карьер", "Глиняный карьер", "Копаете глину", Some(Tool::Spade), Drill::Pump, 4.0, 6, 70.0, None),
            Sand => i("Песчаный берег", "Песчаный берег", "Насыпаете песок в мешок", None, Drill::Pump, 3.0, 8, 30.0, None),
            Quarry => i("Каменоломня", "Каменоломня", "Откалываете блок по клиньям", Some(Tool::Pick), Drill::Rhythm, 6.0, 4, 120.0, None),
        }
    }

    /// Нужно ли рудознатство, чтобы взять руду, а не пустую породу.
    pub fn ore(self) -> bool {
        matches!(self, SiteKind::BogOre | SiteKind::Outcrop | SiteKind::Adit)
    }
}

/// Что вышло из одной ходки.
#[derive(Debug, Clone, PartialEq)]
pub struct Dug {
    pub items: Vec<(&'static str, u16)>,
    /// Руда получше (богатый кусок жилы).
    pub rich: bool,
    pub note: Option<&'static str>,
}

/// Сколько и чего накопали. `prospect` — знает ли рудознатство, `skill` 0..100 (кузнечное),
/// `q` 0..1 — как работали руками, `roll` — случайные числа 0..1.
pub fn dig_yield(kind: SiteKind, prospect: bool, skill: f32, q: f32, roll: [f32; 3]) -> Dug {
    let s = (skill / 100.0).clamp(0.0, 1.0);
    let good = q >= 0.75;
    let bad = q < 0.35;
    let mut items = Vec::new();
    let mut note = None;
    let mut rich = false;
    let add = |items: &mut Vec<(&'static str, u16)>, name: &'static str, n: i32| {
        if n > 0 {
            items.push((name, n as u16));
        }
    };
    match kind {
        SiteKind::BogOre | SiteKind::Outcrop | SiteKind::Adit => {
            let ore = match kind {
                SiteKind::BogOre => BOG_ORE,
                _ => ORE,
            };
            if prospect {
                let mut n = match kind {
                    SiteKind::Adit => 2,
                    _ => 1,
                } + good as i32
                    + (roll[0] < s * 0.8) as i32
                    - bad as i32;
                n = n.max(1);
                add(&mut items, ore, n);
                // Знающий откидывает пустое в отвал, в сумку берёт только руду.
                rich = kind == SiteKind::Adit && roll[1] < 0.25 + 0.35 * s;
            } else {
                // Новичок не отличит: берёт всё подряд, руды — случайно.
                let n = (roll[0] < 0.35 + 0.2 * q) as i32;
                add(&mut items, ore, n);
                add(&mut items, WASTE, 2 - n + (roll[1] < 0.5) as i32);
                note = Some(match kind {
                    SiteKind::BogOre => "Рыжая глина, камни, какие-то комья — где тут руда, не разобрать",
                    _ => "Набили камня, а какой из них руда — не понять",
                });
            }
        }
        SiteKind::Shaft => {
            // Поденщина: плату даёт десятник (сервер), изредка — кусок руды.
            if roll[2] < 0.25 + 0.3 * q {
                add(&mut items, ORE, 1);
            }
        }
        SiteKind::Woodlot => add(&mut items, WOOD, 2 + good as i32 + (roll[0] < s * 0.5) as i32 - bad as i32),
        SiteKind::ClayPit => add(&mut items, CLAY, 2 + good as i32 - bad as i32),
        SiteKind::Sand => add(&mut items, SAND, 2 + good as i32),
        SiteKind::Quarry => {
            if bad && roll[0] < 0.6 {
                note = Some("Блок раскололся не по клиньям — только щебень");
                add(&mut items, WASTE, 2);
            } else {
                add(&mut items, LIMESTONE, 1);
            }
        }
    }
    Dug { items, rich, note }
}

/// Поденная плата за смену у ворота (в долях) при качестве работы `q`.
pub fn shaft_wage(q: f32) -> u32 {
    (12.0 * crate::work::wage_mult(q)).round() as u32
}

// ---------------------------------------------------------------------------
// Куча: угольная («кабан») и обжиговое кострище
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PileKind {
    /// Угольная куча: поленья под землёй, тлеют без пламени.
    Clamp,
    /// Обжиговое кострище: руда на дровах, открытый огонь.
    Roast,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Пустая площадка: складывать поленья (и руду).
    Stack,
    /// Угольная куча сложена: укрыть листвой и землёй.
    Cover,
    /// Готово к поджогу.
    Ready,
    Burning,
    /// Заглушена (или прогорело кострище): остывает.
    Cooling,
    /// Остыло: разбирать.
    Done,
}

/// Какой дым над кучей (его видят все и по нему судят, как идёт).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Smoke {
    None,
    /// Густой белый: выходит вода.
    White,
    /// Жёлто-серый, густой, едкий: идёт обугливание.
    Yellow,
    /// Голубой, прозрачный: уголь готов.
    Blue,
    /// Пламя прорвалось: горит, а не тлеет.
    Flame,
    /// Обжиговый костёр.
    Fire,
}

/// Сколько поленьев принимает угольная куча и кострище, сколько руды на кострище.
pub const CLAMP_WOOD: (u16, u16) = (6, 40);
pub const ROAST_WOOD: (u16, u16) = (2, 10);
pub const ROAST_ORE: u16 = 16;
/// Отдушин в угольной куче.
pub const VENTS: u8 = 6;
/// Сколько угля даёт полено при идеальном пережоге.
pub const COAL_PER_WOOD: f32 = 3.0;

/// Состояние кучи. Все доли — 0..1.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Pile {
    pub kind: PileKind,
    pub stage: Stage,
    pub wood: u16,
    /// Руда на кострище по сортам (сырая горная, болотная).
    pub ore: u16,
    pub bog: u16,
    /// Как плотно сложено (среднее качество укладки).
    pub stack: f32,
    /// Насколько укрыто землёй (угольная куча).
    pub cover: f32,
    pub vents: u8,
    /// Жар внутри: 0 — холодно, ~0.6–1.1 — тлеет как надо, выше — горит.
    pub heat: f32,
    /// Сколько воды осталось в дровах.
    pub moisture: f32,
    /// Доля дров, ставших углём (у кострища — сколько дров прогорело).
    pub charred: f32,
    /// Доля, сгоревшая в золу.
    pub ash: f32,
    /// Прогары в кровле (действуют как лишние отдушины).
    pub cracks: f32,
    /// Кострище: обожжено, спеклось.
    pub roast: f32,
    pub sinter: f32,
    /// Секунд с последнего ворошения (кострище).
    pub since_stir: f32,
    pub elapsed: f32,
    pub cool: f32,
    /// Огонь погас сам (мало воздуха / кончились дрова).
    pub out: bool,
}

/// Что произошло за шаг (для уведомлений рядом стоящим).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PileEvent {
    /// Прогар: в кровле открылась дыра, оттуда пламя.
    Crack,
    WentOut,
    /// Дым поголубел.
    Blue,
    /// Кострище прогорело.
    BurntDown,
    Cooled,
    /// Спекается в корку.
    Sintering,
}

/// Время на остывание (с).
pub const COOL_SECS: f32 = 35.0;

impl Pile {
    pub fn new(kind: PileKind) -> Self {
        Pile {
            kind,
            stage: Stage::Stack,
            wood: 0,
            ore: 0,
            bog: 0,
            stack: 0.0,
            cover: 0.0,
            vents: 0,
            heat: 0.0,
            moisture: 1.0,
            charred: 0.0,
            ash: 0.0,
            cracks: 0.0,
            roast: 0.0,
            sinter: 0.0,
            since_stir: 99.0,
            elapsed: 0.0,
            cool: 0.0,
            out: false,
        }
    }

    pub fn wood_limits(&self) -> (u16, u16) {
        match self.kind {
            PileKind::Clamp => CLAMP_WOOD,
            PileKind::Roast => ROAST_WOOD,
        }
    }

    /// Положили поленья, уложенные с качеством `q`.
    pub fn add_wood(&mut self, n: u16, q: f32) -> u16 {
        let room = self.wood_limits().1.saturating_sub(self.wood);
        let n = n.min(room);
        if n == 0 {
            return 0;
        }
        self.stack = (self.stack * self.wood as f32 + q.clamp(0.0, 1.0) * n as f32) / (self.wood + n) as f32;
        self.wood += n;
        n
    }

    /// Насыпали руду на кострище.
    pub fn add_ore(&mut self, raw: u16, bog: u16) -> (u16, u16) {
        let room = ROAST_ORE.saturating_sub(self.ore + self.bog);
        let raw = raw.min(room);
        let bog = bog.min(room - raw);
        self.ore += raw;
        self.bog += bog;
        (raw, bog)
    }

    /// Укрыли землёй (качество работы `q`).
    pub fn add_cover(&mut self, q: f32) {
        self.cover = (self.cover + 0.45 + 0.55 * q.clamp(0.0, 1.0)).min(1.0);
        if self.cover >= 0.99 {
            self.cover = 1.0;
        }
    }

    pub fn lit(&self) -> bool {
        self.stage == Stage::Burning
    }

    /// Сколько воздуха проходит в кучу.
    pub fn air(&self) -> f32 {
        match self.kind {
            PileKind::Clamp => self.vents as f32 * 0.11 + self.cracks * 0.9 + (1.0 - self.cover) * 1.2,
            PileKind::Roast => 1.0,
        }
    }

    /// Каким бы стал жар при этом воздухе.
    fn target_heat(&self) -> f32 {
        let fuel = (1.0 - self.charred - self.ash).max(0.0);
        match self.kind {
            // Мало воздуха — угасает; ровно — тлеет; много — горит пламенем.
            PileKind::Clamp => {
                if fuel <= 0.01 {
                    0.0
                } else {
                    (0.15 + self.air() * 1.55).min(1.8) * (0.4 + 0.6 * fuel.min(1.0).sqrt())
                }
            }
            // Чем больше дров на руду, тем жарче костёр.
            PileKind::Roast => {
                if fuel <= 0.01 {
                    0.0
                } else {
                    let load = (self.ore + self.bog).max(1) as f32;
                    (0.45 + 0.42 * (self.wood as f32 * 2.0 / load).min(2.2)) * (0.5 + 0.5 * fuel.sqrt())
                }
            }
        }
    }

    /// Дым, который видно со стороны.
    pub fn smoke(&self) -> Smoke {
        if self.stage != Stage::Burning || self.out {
            return Smoke::None;
        }
        match self.kind {
            PileKind::Roast => Smoke::Fire,
            PileKind::Clamp => {
                if self.heat > 1.25 || self.cover < 0.5 {
                    Smoke::Flame
                } else if self.moisture > 0.12 {
                    Smoke::White
                } else if self.charred < 0.82 {
                    Smoke::Yellow
                } else {
                    Smoke::Blue
                }
            }
        }
    }

    /// Поджечь. Ошибка — почему нельзя.
    pub fn light(&mut self) -> Result<(), &'static str> {
        match (self.kind, self.stage) {
            (_, Stage::Burning) if !self.out => Err("Уже горит"),
            (PileKind::Clamp, Stage::Stack | Stage::Cover) if self.wood < CLAMP_WOOD.0 => Err("Мало поленьев: куча не займётся"),
            (PileKind::Roast, Stage::Stack) if self.wood < ROAST_WOOD.0 => Err("Мало дров под рудой"),
            (PileKind::Roast, Stage::Stack) if self.ore + self.bog == 0 => Err("На кострище нет руды"),
            (_, Stage::Cooling | Stage::Done) => Err("Сначала разберите"),
            _ => {
                self.stage = Stage::Burning;
                self.out = false;
                self.heat = self.heat.max(0.35);
                if self.kind == PileKind::Clamp && self.vents == 0 {
                    self.vents = 3;
                }
                Ok(())
            }
        }
    }

    /// Заглушить угольную кучу: закрыть отдушины, засыпать прогары.
    pub fn smother(&mut self) {
        if self.stage == Stage::Burning {
            self.vents = 0;
            self.cracks = 0.0;
            self.cover = 1.0;
            self.stage = Stage::Cooling;
            self.cool = 0.0;
        }
    }

    pub fn set_vents(&mut self, v: u8) {
        self.vents = v.min(VENTS);
    }

    /// Засыпали прогар землёй.
    pub fn patch(&mut self, q: f32) {
        self.cracks = (self.cracks - 0.35 - 0.5 * q.clamp(0.0, 1.0)).max(0.0);
    }

    pub fn stirred(&mut self) {
        self.since_stir = 0.0;
    }

    /// Шаг на `dt` секунд. `rnd` — случайное 0..1 (прогары).
    pub fn step(&mut self, dt: f32, rnd: f32) -> Vec<PileEvent> {
        let mut ev = Vec::new();
        self.since_stir += dt;
        match self.stage {
            Stage::Burning => {}
            Stage::Cooling => {
                self.heat = (self.heat - dt * 0.03).max(0.0);
                self.cool += dt;
                if self.cool >= COOL_SECS {
                    self.stage = Stage::Done;
                    ev.push(PileEvent::Cooled);
                }
                return ev;
            }
            _ => return ev,
        }
        self.elapsed += dt;
        if self.out {
            self.heat = (self.heat - dt * 0.05).max(0.0);
            return ev;
        }
        let target = self.target_heat();
        self.heat += (target - self.heat) * (1.0 - (-0.12 * dt).exp());
        match self.kind {
            PileKind::Clamp => self.step_clamp(dt, rnd, &mut ev),
            PileKind::Roast => self.step_roast(dt, &mut ev),
        }
        ev
    }

    fn step_clamp(&mut self, dt: f32, rnd: f32, ev: &mut Vec<PileEvent>) {
        let h = self.heat;
        // Сначала сохнут дрова (белый дым), потом обугливаются от верха к низу.
        if self.moisture > 0.0 {
            self.moisture = (self.moisture - dt * 0.011 * (0.3 + h)).max(0.0);
        }
        let dry = 1.0 - self.moisture.min(1.0);
        // Плотная укладка тлеет ровнее: рыхлая — неровно, с недожогом.
        let even = 0.55 + 0.45 * self.stack;
        let window = (1.0 - ((h - 0.85) / 0.45).abs()).clamp(0.0, 1.0);
        let left = (1.0 - self.charred - self.ash).max(0.0);
        let was_blue = self.charred >= 0.82;
        self.charred += dt * 0.0085 * window * dry * even * left.min(1.0).sqrt() * 1.3;
        // Лишний воздух жжёт и дрова, и готовый уголь.
        let burn = (self.air() - 0.42).max(0.0) * 0.012 + (h - 1.2).max(0.0) * 0.02;
        let lost = (burn * dt).min(self.charred + left);
        // Сгорает сначала уже обугленное (у отдушин и в прогарах).
        let from_coal = lost.min(self.charred * 0.7);
        self.charred -= from_coal;
        self.ash += lost;
        let total = self.charred + self.ash;
        if total > 1.0 {
            self.charred -= total - 1.0;
        }
        self.charred = self.charred.max(0.0);
        if !was_blue && self.charred >= 0.82 && self.moisture <= 0.12 {
            ev.push(PileEvent::Blue);
        }
        // Прогары: жарко и рыхло — кровля оседает и прорывается.
        let p = dt * (0.004 + 0.02 * (h - 0.9).max(0.0)) * (1.3 - self.stack * 0.6) * (1.2 - self.cover * 0.4);
        if rnd < p {
            self.cracks = (self.cracks + 0.25).min(1.0);
            ev.push(PileEvent::Crack);
        }
        if self.heat < 0.18 && self.elapsed > 8.0 && left > 0.02 {
            self.out = true;
            ev.push(PileEvent::WentOut);
        }
        if left <= 0.01 {
            // Всё прогорело: тлеет зола.
            self.stage = Stage::Cooling;
            self.cool = 0.0;
        }
    }

    fn step_roast(&mut self, dt: f32, ev: &mut Vec<PileEvent>) {
        let h = self.heat;
        // Дрова прогорают за полторы-две минуты (чем жарче, тем быстрее).
        let wood_secs = 40.0 + self.wood as f32 * 9.0;
        self.charred = (self.charred + dt / wood_secs * (0.6 + 0.5 * h)).min(1.0);
        // Ворошат — руда прогревается ровно, без перегретых мест.
        let stirring = self.since_stir < 4.0;
        let window = (1.0 - ((h - 0.85) / 0.35).abs()).clamp(0.0, 1.0);
        let left = (1.0 - self.roast - self.sinter).max(0.0);
        self.roast += dt * 0.011 * window * if stirring { 1.35 } else { 0.9 } * left.sqrt();
        let hot = (h - 1.12).max(0.0) * if stirring { 0.25 } else { 1.0 };
        let was = self.sinter;
        self.sinter = (self.sinter + dt * 0.03 * hot * (1.0 - self.sinter)).min(1.0 - self.roast);
        if was < 0.08 && self.sinter >= 0.08 {
            ev.push(PileEvent::Sintering);
        }
        if self.charred >= 1.0 {
            self.stage = Stage::Cooling;
            self.cool = 0.0;
            ev.push(PileEvent::BurntDown);
        }
    }

    /// Что достанут при разборке.
    pub fn harvest(&self, q: f32) -> Harvest {
        let q = q.clamp(0.0, 1.0);
        match self.kind {
            PileKind::Clamp => {
                let w = self.wood as f32;
                // Неаккуратно разобрали — уголь крошится в мелочь.
                let care = 0.85 + 0.15 * q;
                let coal = (w * COAL_PER_WOOD * self.charred * care).floor() as u16;
                let raw = 1.0 - self.charred - self.ash;
                let brands = (w * raw * 0.85).floor() as u16;
                Harvest { charcoal: coal, wood: brands, roasted: 0, ore: 0, bog: 0, waste: 0 }
            }
            PileKind::Roast => {
                let n = (self.ore + self.bog) as f32;
                let roasted = (n * self.roast).round() as u16;
                let waste = ((n * self.sinter).round() as u16).min(self.ore + self.bog - roasted);
                let left = self.ore + self.bog - roasted - waste;
                // Недообожжённое возвращается как было: сначала болотная.
                let bog = left.min(self.bog);
                let ore = left - bog;
                Harvest { charcoal: 0, wood: 0, roasted, ore, bog, waste }
            }
        }
    }
}

/// Что вышло из кучи.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Harvest {
    pub charcoal: u16,
    pub wood: u16,
    pub roasted: u16,
    pub ore: u16,
    pub bog: u16,
    pub waste: u16,
}

/// Как идёт угольная куча — словами. Знающий углежжение читает дым, новичок видит только цвет.
pub fn clamp_reading(p: &Pile, knows: bool) -> String {
    let cue = match p.smoke() {
        Smoke::None if p.out => "Куча не дымит — угасла",
        Smoke::None => "",
        Smoke::White => {
            if knows {
                "Дым белый, густой: выходит вода из дров. Рано"
            } else {
                "Дым белый, густой"
            }
        }
        Smoke::Yellow => {
            if knows {
                "Дым жёлтый, едкий: дрова обугливаются. Держите ровно"
            } else {
                "Дым жёлто-серый, едкий"
            }
        }
        Smoke::Blue => {
            if knows {
                "Дым голубой, прозрачный — уголь готов! Глушите"
            } else {
                "Дым поредел, синеватый"
            }
        }
        Smoke::Flame => {
            if knows {
                "Пламя прорвалось — уголь сгорает в золу! Закройте отдушины"
            } else {
                "Из кучи бьёт пламя!"
            }
        }
        Smoke::Fire => "",
    };
    let mut s = cue.to_string();
    if knows && p.lit() && !p.out {
        let tone = match p.heat {
            h if h < 0.45 => "Еле тлеет — дайте воздуха",
            h if h < 0.65 => "Тлеет слабо",
            h if h < 1.1 => "Тлеет ровно",
            _ => "Слишком жарко",
        };
        s.push_str(&format!("\n{tone}"));
    }
    if p.cracks > 0.05 && p.lit() {
        s.push_str("\nВ кровле прогар — засыпьте землёй");
    }
    s
}

/// Как идёт обжиг — словами.
pub fn roast_reading(p: &Pile, knows: bool) -> String {
    if p.stage != Stage::Burning {
        return String::new();
    }
    let mut v = Vec::new();
    if knows {
        v.push(match p.roast {
            r if r < 0.3 => "Руда ещё рыжая, парит",
            r if r < 0.7 => "Руда темнеет, потрескивает",
            _ => "Руда буро-чёрная, крошится — обожжена",
        });
        v.push(match p.heat {
            h if h < 0.55 => "Огонь слабый — подбросьте дров",
            h if h < 1.12 => "Жар ровный",
            _ => "Перегрев — руда спекается! Ворошите",
        });
    } else {
        v.push(if p.heat > 1.12 { "Костёр ревёт, камни раскалены докрасна" } else { "Костёр горит, камни дымят" });
    }
    if p.sinter > 0.08 {
        v.push("Местами руда сплавилась в корку");
    }
    v.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burn(p: &mut Pile, secs: f32, mut policy: impl FnMut(&mut Pile, f32)) {
        let mut t = 0.0;
        let mut seed = 7u32;
        while t < secs && p.stage == Stage::Burning {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let r = (seed >> 8) as f32 / 16777216.0;
            p.step(0.25, r);
            policy(p, t);
            t += 0.25;
        }
    }

    fn clamp(wood: u16) -> Pile {
        let mut p = Pile::new(PileKind::Clamp);
        p.add_wood(wood, 0.9);
        p.add_cover(1.0);
        p.light().unwrap();
        p
    }

    /// Хороший углежог: держит 2–3 отдушины, засыпает прогары, глушит на голубом дыме.
    fn careful(p: &mut Pile, _t: f32) {
        if p.cracks > 0.0 {
            p.patch(0.8);
        }
        let want = if p.heat < 0.7 { 4 } else if p.heat > 1.0 { 1 } else { 2 };
        p.set_vents(want);
        if p.smoke() == Smoke::Blue {
            p.smother();
        }
    }

    #[test]
    fn a_tended_clamp_turns_wood_into_charcoal() {
        let mut p = clamp(20);
        burn(&mut p, 900.0, careful);
        assert_eq!(p.stage, Stage::Cooling, "куча так и не дошла: {p:?}");
        assert!(p.elapsed > 120.0 && p.elapsed < 600.0, "горела {} с", p.elapsed);
        let h = p.harvest(0.9);
        assert!(h.charcoal >= 40, "угля мало: {h:?}, {p:?}");
    }

    #[test]
    fn open_vents_burn_charcoal_to_ash() {
        let mut good = clamp(20);
        burn(&mut good, 900.0, careful);
        let mut bad = clamp(20);
        burn(&mut bad, 900.0, |p, _| p.set_vents(VENTS));
        let (g, b) = (good.harvest(0.9), bad.harvest(0.9));
        assert!(b.charcoal * 2 < g.charcoal, "с открытыми отдушинами: {b:?}, бережно: {g:?}");
    }

    #[test]
    fn uncovered_pile_is_a_bonfire() {
        let mut p = Pile::new(PileKind::Clamp);
        p.add_wood(20, 0.9);
        p.light().unwrap();
        burn(&mut p, 900.0, |_, _| {});
        assert!(p.harvest(0.9).charcoal < 15, "{:?}", p.harvest(0.9));
    }

    #[test]
    fn smothering_early_leaves_brands() {
        let mut p = clamp(20);
        burn(&mut p, 900.0, |p, t| {
            careful(p, t);
            if t > 60.0 {
                p.smother();
            }
        });
        let h = p.harvest(0.9);
        assert!(h.wood >= 8 && h.charcoal < 30, "{h:?}");
    }

    #[test]
    fn closed_vents_let_the_fire_die() {
        let mut p = clamp(20);
        burn(&mut p, 300.0, |p, _| {
            p.set_vents(0);
            p.patch(1.0);
        });
        assert!(p.out, "{p:?}");
    }

    #[test]
    fn roasting_needs_steady_heat_and_stirring() {
        let mut stirred = Pile::new(PileKind::Roast);
        stirred.add_wood(6, 0.8);
        stirred.add_ore(10, 0);
        stirred.light().unwrap();
        burn(&mut stirred, 600.0, |p, _| p.stirred());
        let h = stirred.harvest(0.8);
        assert!(h.roasted >= 7, "{h:?} {stirred:?}");
        // Гора дров и никто не ворошит — спекается.
        let mut lazy = Pile::new(PileKind::Roast);
        lazy.add_wood(10, 0.8);
        lazy.add_ore(4, 0);
        lazy.light().unwrap();
        burn(&mut lazy, 600.0, |_, _| {});
        let l = lazy.harvest(0.8);
        assert!(l.waste >= 1, "{l:?} {lazy:?}");
    }

    #[test]
    fn novices_dig_mostly_waste() {
        let mut ore_k = 0;
        let mut ore_n = 0;
        for i in 0..200 {
            let r = [(i as f32 * 0.37).fract(), (i as f32 * 0.61).fract(), (i as f32 * 0.13).fract()];
            let count = |d: Dug| d.items.iter().filter(|(n, _)| *n == ORE).map(|(_, c)| *c as i32).sum::<i32>();
            ore_k += count(dig_yield(SiteKind::Outcrop, true, 20.0, 0.7, r));
            ore_n += count(dig_yield(SiteKind::Outcrop, false, 20.0, 0.7, r));
        }
        assert!(ore_k > ore_n * 2, "знающий {ore_k}, новичок {ore_n}");
    }

    #[test]
    fn richer_ore_gives_more_iron() {
        assert!(charge_richness(&[(OreGrade::Roasted, 6)]) > charge_richness(&[(OreGrade::Raw, 6)]));
        assert!(charge_richness(&[(OreGrade::Bog, 6)]) < 100);
        assert!(richness_purity(125) > 0.05 && richness_purity(80) < -0.04);
        assert_eq!(charge_richness(&[]), 100);
    }

    #[test]
    fn tools_are_recognised() {
        assert!(Tool::Axe.fits("Плотничий топор") && Tool::Axe.fits(WOOD_AXE) && Tool::Axe.fits("Бородовидная секира"));
        assert!(Tool::Pick.fits(PICK) && !Tool::Pick.fits(SPADE));
        assert!(Tool::Spade.fits(SPADE));
    }

    #[test]
    fn every_site_item_exists() {
        for name in [ORE, BOG_ORE, ROASTED_ORE, WASTE, WOOD, CHARCOAL, CLAY, SAND, LIMESTONE, PICK, SPADE, WOOD_AXE, THEOPHRASTUS_PLANTS] {
            assert!(crate::items::ItemBaseId::find(name).is_some(), "нет предмета {name}");
        }
    }
}
