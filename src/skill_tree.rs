//! Древо навыков и умений.
//!
//! Навык (0–100, [`Skill`]) растёт от дела: кто куёт — тот и кузнец, кто таскает мешки —
//! крепнет. Связанные навыки растут вместе ([`Skill::spillover`]): кузнец набирает силу,
//! боец — силу и выносливость, лекарь лучше знает травы.
//!
//! Умение ([`Perk`]) — ступень мастерства. Его не покупают очками: оно приходит само, когда
//! набраны навыки (часто — несколько разных) и выучены нужные знания ([`Lore`]). Так и связано
//! всё древо: «Хирургу» нужны и врачевание, и выученные приёмы Цельса, и умение «Травник-лекарь»,
//! которое само просит травничества.
//!
//! Всё здесь — чистые функции от [`Knowledge`]: сервер по ним меняет бой, торговлю и чтение,
//! клиент рисует древо. Хранить ничего не нужно.

use crate::lore::{Knowledge, Language, Lore, Skill, Substance, Technique};

// ---------------------------------------------------------------------------
// Навыки: что дают и как растут
// ---------------------------------------------------------------------------

/// Описание навыка для книги навыков и журнала.
pub struct SkillAbout {
    /// Что даёт навык.
    pub gives: &'static str,
    /// Отчего растёт (без рецептов и правильных ответов).
    pub grows: &'static str,
}

pub fn about(skill: Skill) -> SkillAbout {
    let a = |gives, grows| SkillAbout { gives, grows };
    match skill {
        Skill::Strength => a(
            "Больше ноши без одышки, тяжелее удар, больше запас сил.",
            "Тяжёлый труд: кузнечный молот и меха, лопата у корней, ноша на пределе. Немного — от боя и тугого лука.",
        ),
        Skill::Endurance => a(
            "Быстрее переводите дух и дольше держитесь в бою и в беге.",
            "Бег, долгий бой, работа без передышки, походы за стены за травами.",
        ),
        Skill::Arms => a(
            "Защита держит крепче, удары точнее и чуть быстрее.",
            "Настоящий бой: попадания по живому противнику, отражённые и принятые на щит удары.",
        ),
        Skill::Archery => a("Стрелы ложатся кучнее и бьют сильнее.", "Попадания из лука по живой цели."),
        Skill::Smithing => a(
            "Лучше изделия: ровнее ковка, вернее закалка, крепче сварка.",
            "Работа у горна. Подмастерьем у кузнеца рука набивается быстрее, чем по книгам.",
        ),
        Skill::Alchemy => a(
            "Точнее чувствуете жар, меньше бьётся стекло, чище выход.",
            "Перегонка, обжиг, плавка, краски — своими руками. Подмастерьем у стеклодува, гончара, красильщика.",
        ),
        Skill::Herbalism => a(
            "Больше и лучше сбор, крепче отвары и мази.",
            "Сбор трав, варка отваров и мазей, работа у ризотома и аптекаря.",
        ),
        Skill::Medicine => a(
            "Вернее лечение: меньше вреда, быстрее заживает, видно больше в ранах.",
            "Уход за ранеными и больными, смены в ксеноне при базилике, уроки у ятроса.",
        ),
        Skill::Letters => a(
            "Быстрее читаете, лучше схватываете уроки мастеров.",
            "Уроки грамоты, чтение книг от начала до конца.",
        ),
        Skill::Trade => a(
            "Дешевле покупаете и дороже продаёте.",
            "Сделки в лавках и мена за стенами. Чем больше оборот — тем больше опыта.",
        ),
        Skill::Eloquence => a(
            "Стража чаще верит вашим словам и отпускает без пени.",
            "Разговоры со стражей (удачные — особенно), торг в лавках, уроки риторики по книгам.",
        ),
    }
}

// ---------------------------------------------------------------------------
// Умения
// ---------------------------------------------------------------------------

/// Умение — ступень мастерства в древе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Perk {
    // Сила
    PortersBack,
    HeavyHand,
    Hammerman,
    // Выносливость
    SecondWind,
    ArmourHabit,
    ForcedMarch,
    // Ратное дело
    Drill,
    TurnTheBlade,
    Veteran,
    Bucellarius,
    // Стрельба
    SteadyAim,
    SteppeBow,
    Sagittarius,
    // Кузнечное дело
    ForgeEye,
    Temperer,
    Bladesmith,
    MasterSmith,
    // Алхимия
    Stoker,
    Zosimian,
    Colourman,
    FireMaster,
    // Травничество
    Rhizotomist,
    HerbKnower,
    Pharmacist,
    // Врачевание
    Dresser,
    HerbHealer,
    Surgeon,
    Archiatros,
    // Книжность
    Literate,
    Bookman,
    Rhetor,
    Calligrapher,
    // Торговля
    KnowsPrice,
    Haggler,
    Merchant,
    // Красноречие
    SweetWords,
    Advocate,
}

/// Условие умения.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Need {
    /// Наработанный навык (без прибавок от умений).
    Skill(Skill, f32),
    Lore(Lore),
    /// Любое из знаний.
    AnyLore(&'static [Lore]),
    Perk(Perk),
    /// Узнаёт в лицо столько диких трав.
    HerbsKnown(usize),
}

impl Need {
    pub fn met(&self, k: &Knowledge, earned: &[Perk]) -> bool {
        match *self {
            Need::Skill(s, lvl) => k.base_skill(s) >= lvl,
            Need::Lore(l) => k.knows(l),
            Need::AnyLore(list) => list.iter().any(|l| k.knows(*l)),
            Need::Perk(p) => earned.contains(&p),
            Need::HerbsKnown(n) => herbs_known(k) >= n,
        }
    }

    /// Подпись условия: «Сила 35», «Знание: приём: ковка», «Умение «Выучка»».
    pub fn label(&self) -> String {
        match *self {
            Need::Skill(s, lvl) => format!("{} {:.0}", s.name(), lvl),
            Need::Lore(l) => lore_label(l),
            Need::AnyLore(list) => {
                let names: Vec<String> = list.iter().map(|l| lore_label(*l)).collect();
                names.join(" или ")
            }
            Need::Perk(p) => format!("умение «{}»", p.info().name),
            Need::HerbsKnown(n) => format!("узнавать {n} диких трав"),
        }
    }
}

fn lore_label(l: Lore) -> String {
    match l {
        Lore::Read(lang) => format!("читать: {}", lang.name()),
        Lore::Technique(t) => format!("приём «{}»", t.name().to_lowercase()),
        other => other.name().to_lowercase(),
    }
}

/// Сколько диких трав персонаж узнаёт в лицо.
pub fn herbs_known(k: &Knowledge) -> usize {
    Substance::ALL.iter().filter(|s| s.is_wild() && k.knows(Lore::Identify(**s))).count()
}

pub struct PerkInfo {
    pub name: &'static str,
    /// Ветвь (столбец древа).
    pub skill: Skill,
    /// Ступень в ветви: 0 — первая.
    pub tier: u8,
    /// Что это за мастерство (исторически, без цифр).
    pub about: &'static str,
    /// Что даёт в игре.
    pub effect: &'static str,
    pub needs: &'static [Need],
}

use Need as N;
use Skill as S;

impl Perk {
    pub const ALL: [Perk; 37] = {
        use Perk::*;
        [
            PortersBack, HeavyHand, Hammerman, SecondWind, ArmourHabit, ForcedMarch, Drill, TurnTheBlade, Veteran, Bucellarius, SteadyAim, SteppeBow, Sagittarius, ForgeEye, Temperer,
            Bladesmith, MasterSmith, Stoker, Zosimian, Colourman, FireMaster, Rhizotomist, HerbKnower, Pharmacist, Dresser, HerbHealer, Surgeon, Archiatros, Literate, Bookman, Rhetor,
            KnowsPrice, Haggler, Merchant, SweetWords, Advocate, Calligrapher,
        ]
    };

    pub fn info(self) -> PerkInfo {
        use Perk::*;
        let p = |name, skill, tier, about, effect, needs| PerkInfo { name, skill, tier, about, effect, needs };
        match self {
            PortersBack => p(
                "Спина грузчика",
                S::Strength,
                0,
                "Мешки с зерном в порту, бочки солёной рыбы, корзины угля — спина привыкла.",
                "Ноша: +8 кг без одышки.",
                &[N::Skill(S::Strength, 15.0)],
            ),
            HeavyHand => p(
                "Тяжёлая рука",
                S::Strength,
                1,
                "Сильный и обученный бьёт так, что щит гудит.",
                "Урон в ближнем бою +6%.",
                &[N::Skill(S::Strength, 35.0), N::Skill(S::Arms, 15.0), N::Perk(PortersBack)],
            ),
            Hammerman => p(
                "Молотобоец",
                S::Strength,
                2,
                "Подручный у горна весь день бьёт кувалдой по слову мастера.",
                "Кузнечное дело +5 в работе, запас сил +10.",
                &[N::Skill(S::Strength, 50.0), N::Skill(S::Smithing, 25.0), N::Perk(PortersBack)],
            ),
            SecondWind => p(
                "Второе дыхание",
                S::Endurance,
                0,
                "Сбил дыхание — и снова ровно дышишь.",
                "Выносливость восстанавливается на 12% быстрее.",
                &[N::Skill(S::Endurance, 15.0)],
            ),
            ArmourHabit => p(
                "Привычка к доспеху",
                S::Endurance,
                1,
                "Кольчуга и шлем больше не давят: тело привыкло к железу.",
                "Отдых +8%, шаг +3%.",
                &[N::Skill(S::Endurance, 35.0), N::Skill(S::Strength, 20.0), N::Perk(SecondWind)],
            ),
            ForcedMarch => p(
                "Марш-бросок",
                S::Endurance,
                2,
                "Пеший стратиот проходит за день до двухсот стадиев и ещё стоит в строю.",
                "Запас выносливости +15.",
                &[N::Skill(S::Endurance, 55.0), N::Perk(ArmourHabit)],
            ),
            Drill => p(
                "Строевая выучка",
                S::Arms,
                0,
                "Стойка, щит, шаг — как на плацу у кампидуктора.",
                "Защита держит крепче (+6% устойчивости).",
                &[N::Skill(S::Arms, 15.0)],
            ),
            TurnTheBlade => p(
                "Отвести клинок",
                S::Arms,
                1,
                "Не ловить удар, а уводить его — и сразу бить в ответ.",
                "Окно ответного удара после парирования длиннее.",
                &[N::Skill(S::Arms, 35.0), N::Skill(S::Endurance, 20.0), N::Perk(Drill)],
            ),
            Veteran => p(
                "Ветеран",
                S::Arms,
                2,
                "Пережил не одну стычку с гуннами за стенами.",
                "Урон +6%, удары на 4% быстрее.",
                &[N::Skill(S::Arms, 60.0), N::Skill(S::Strength, 30.0), N::Skill(S::Endurance, 30.0), N::Perk(TurnTheBlade)],
            ),
            Bucellarius => p(
                "Букелларий",
                S::Arms,
                3,
                "Личный дружинник полководца: и копьём, и мечом, и луком.",
                "Урон +5%, защита +5%.",
                &[N::Skill(S::Arms, 80.0), N::Skill(S::Archery, 30.0), N::Perk(Veteran)],
            ),
            SteadyAim => p(
                "Верный глаз",
                S::Archery,
                0,
                "Рука не дрожит, выдох — и выстрел.",
                "Разброс стрел −35%.",
                &[N::Skill(S::Archery, 15.0)],
            ),
            SteppeBow => p(
                "Тугой степной лук",
                S::Archery,
                1,
                "Сложный лук из рога и жил натягивают по-гуннски, к уху.",
                "Урон стрел +8%.",
                &[N::Skill(S::Archery, 35.0), N::Skill(S::Strength, 25.0), N::Perk(SteadyAim)],
            ),
            Sagittarius => p(
                "Сагиттарий",
                S::Archery,
                2,
                "Стрелок римского войска: бьёт в щель между щитами.",
                "Разброс ещё −30%, урон +5%.",
                &[N::Skill(S::Archery, 60.0), N::Skill(S::Endurance, 25.0), N::Perk(SteppeBow)],
            ),
            ForgeEye => p(
                "Глаз на цвет каления",
                S::Smithing,
                0,
                "Вишнёвый, алый, соломенный — по цвету видно, когда бить.",
                "Кузнечное дело +4 в работе.",
                &[N::Skill(S::Smithing, 15.0), N::Lore(Lore::Technique(Technique::Forging))],
            ),
            Temperer => p(
                "Закальщик",
                S::Smithing,
                1,
                "Знает, в воду или в масло и когда вынуть.",
                "Кузнечное дело +5 в работе.",
                &[N::Skill(S::Smithing, 35.0), N::Lore(Lore::Technique(Technique::Quenching)), N::Perk(ForgeEye)],
            ),
            Bladesmith => p(
                "Клинковый мастер",
                S::Smithing,
                2,
                "Сварить пакет, наварить сталь на лезвие — для этого нужны и сила, и глаз.",
                "Кузнечное дело +6 в работе.",
                &[N::Skill(S::Smithing, 60.0), N::Skill(S::Strength, 35.0), N::Lore(Lore::Technique(Technique::ForgeWelding)), N::Perk(Temperer)],
            ),
            MasterSmith => p(
                "Мастер фабрики",
                S::Smithing,
                3,
                "Такого берут в государственную оружейную — фабрику.",
                "Кузнечное дело +6, алхимия +3 в работе.",
                &[
                    N::Skill(S::Smithing, 80.0),
                    N::AnyLore(&[Lore::Technique(Technique::Armoury), Lore::Technique(Technique::MailMaking), Lore::Technique(Technique::LostWax)]),
                    N::Perk(Bladesmith),
                ],
            ),
            Stoker => p(
                "Печник",
                S::Alchemy,
                0,
                "Ровный жар — половина дела у любого сосуда.",
                "Алхимия +4 в работе.",
                &[N::Skill(S::Alchemy, 15.0), N::AnyLore(&[Lore::Technique(Technique::Fermentation), Lore::Technique(Technique::Distillation), Lore::Technique(Technique::Pottery), Lore::Technique(Technique::Glassblowing)])],
            ),
            Zosimian => p(
                "Ученик Зосима",
                S::Alchemy,
                1,
                "Амбик, водяная баня Марии, керотакис — по книгам александрийцев.",
                "Алхимия +5 в работе.",
                &[N::Skill(S::Alchemy, 35.0), N::Lore(Lore::Technique(Technique::Distillation)), N::Lore(Lore::Read(Language::Greek)), N::Perk(Stoker)],
            ),
            Colourman => p(
                "Красильщик",
                S::Alchemy,
                2,
                "Квасцы, марена, вайда: краски держатся, а травы известны.",
                "Алхимия +4, травничество +3 в работе.",
                &[N::Skill(S::Alchemy, 45.0), N::Skill(S::Herbalism, 25.0), N::Lore(Lore::Technique(Technique::Mordanting)), N::Perk(Stoker)],
            ),
            FireMaster => p(
                "Мастер огня",
                S::Alchemy,
                3,
                "Смола, сера и нефть для обороны стен — как у Энея Тактика.",
                "Алхимия +6 в работе.",
                &[N::Skill(S::Alchemy, 65.0), N::Skill(S::Smithing, 20.0), N::Lore(Lore::Technique(Technique::Incendiary)), N::Perk(Zosimian)],
            ),
            Rhizotomist => p(
                "Корнерез",
                S::Herbalism,
                0,
                "Ризотом знает, когда и как брать траву, не губя корня.",
                "Травничество +4 в работе.",
                &[N::Skill(S::Herbalism, 15.0), N::Lore(Lore::Technique(Technique::Gathering))],
            ),
            HerbKnower => p(
                "Знаток трав",
                S::Herbalism,
                1,
                "Узнаёт в степи десяток трав с первого взгляда.",
                "Травничество +5 в работе.",
                &[N::Skill(S::Herbalism, 30.0), N::HerbsKnown(8), N::Perk(Rhizotomist)],
            ),
            Pharmacist => p(
                "Мирепс",
                S::Herbalism,
                2,
                "Аптекарь: отвары и мази по мере и весу.",
                "Травничество +4, врачевание +3 в работе.",
                &[N::Skill(S::Herbalism, 50.0), N::Skill(S::Medicine, 15.0), N::Lore(Lore::Technique(Technique::Decoction)), N::Lore(Lore::Technique(Technique::Salve)), N::Perk(HerbKnower)],
            ),
            Dresser => p(
                "Перевязчик",
                S::Medicine,
                0,
                "Промыть, перевязать, не занести грязи.",
                "Врачевание +4 в работе.",
                &[N::Skill(S::Medicine, 15.0), N::Lore(Lore::Technique(Technique::WoundWashing))],
            ),
            HerbHealer => p(
                "Травник-лекарь",
                S::Medicine,
                1,
                "Лечит не только руками, но и травой.",
                "Врачевание +5 в работе.",
                &[N::Skill(S::Medicine, 30.0), N::Skill(S::Herbalism, 25.0), N::Perk(Dresser), N::Perk(Rhizotomist)],
            ),
            Surgeon => p(
                "Хирург",
                S::Medicine,
                2,
                "Вынуть наконечник и вправить кость — по Цельсу и Гиппократу.",
                "Врачевание +6 в работе.",
                &[N::Skill(S::Medicine, 50.0), N::Lore(Lore::Technique(Technique::ArrowExtraction)), N::Lore(Lore::Technique(Technique::Bonesetting)), N::Perk(HerbHealer)],
            ),
            Archiatros => p(
                "Архиатр",
                S::Medicine,
                3,
                "Городской врач: его зовут к епископу и к комиту.",
                "Врачевание +6 в работе.",
                &[N::Skill(S::Medicine, 75.0), N::Skill(S::Letters, 40.0), N::Lore(Lore::Technique(Technique::Theriac)), N::Perk(Surgeon)],
            ),
            Literate => p(
                "Грамотей",
                S::Letters,
                0,
                "Читает по-гречески, хоть и по слогам.",
                "Чтение +15% быстрее.",
                &[N::Lore(Lore::Read(Language::Greek))],
            ),
            Bookman => p(
                "Книжник",
                S::Letters,
                1,
                "Прочёл не одну книгу и знает, где что искать.",
                "Чтение +20%, уроки мастеров +10% быстрее.",
                &[N::Skill(S::Letters, 30.0), N::Perk(Literate)],
            ),
            Rhetor => p(
                "Ритор",
                S::Letters,
                2,
                "Знает и латынь: читает Цельса и Плиния, пишет прошения комиту.",
                "Чтение +15%, уроки +10%, торг +2%.",
                &[N::Skill(S::Letters, 55.0), N::Lore(Lore::Read(Language::Latin)), N::Perk(Bookman)],
            ),
            Calligrapher => p(
                "Каллиграф",
                S::Letters,
                3,
                "Твёрдая рука книгописца: ровные строки унциала, ни одной пропущенной строки.",
                "Переписка +30% быстрее, чтение +10%.",
                &[N::Skill(S::Letters, 65.0), N::Lore(Lore::Write(Language::Greek)), N::Lore(Lore::Grammar), N::Perk(Rhetor)],
            ),
            KnowsPrice => p(
                "Знает цену",
                S::Trade,
                0,
                "Знает, почём рыба в порту и почём она у готов.",
                "Покупка −3%, продажа +3%.",
                &[N::Skill(S::Trade, 15.0)],
            ),
            Haggler => p("Торгаш", S::Trade, 1, "Торгуется до последнего медяка.", "Покупка −3%, продажа +3%.", &[N::Skill(S::Trade, 35.0), N::Perk(KnowsPrice)]),
            Merchant => p(
                "Купец",
                S::Trade,
                2,
                "Ведёт счета на табличках и знает, куда что везти.",
                "Покупка −4%, продажа +4%.",
                &[N::Skill(S::Trade, 60.0), N::Skill(S::Letters, 25.0), N::Lore(Lore::Reckon), N::Perk(Haggler)],
            ),
            SweetWords => p(
                "Ласковое слово",
                S::Eloquence,
                0,
                "Вовремя улыбнуться, назвать стражника по имени, вспомнить общего знакомого.",
                "Уговорить стражу легче (+8%).",
                &[N::Skill(S::Eloquence, 15.0)],
            ),
            Advocate => p(
                "Ходатай",
                S::Eloquence,
                1,
                "Знает, на какой закон сослаться и чьё имя назвать в претории.",
                "Уговорить стражу ещё легче (+10%), торг +2%.",
                &[N::Skill(S::Eloquence, 40.0), N::Skill(S::Letters, 20.0), N::Perk(SweetWords)],
            ),
        }
    }

    /// Прибавка к навыку в работе (ремёсла и знахарство).
    fn skill_bonus(self, skill: Skill) -> f32 {
        use Perk::*;
        match (self, skill) {
            (Hammerman, S::Smithing) => 5.0,
            (ForgeEye, S::Smithing) => 4.0,
            (Temperer, S::Smithing) => 5.0,
            (Bladesmith, S::Smithing) => 6.0,
            (MasterSmith, S::Smithing) => 6.0,
            (MasterSmith, S::Alchemy) => 3.0,
            (Stoker, S::Alchemy) => 4.0,
            (Zosimian, S::Alchemy) => 5.0,
            (Colourman, S::Alchemy) => 4.0,
            (Colourman, S::Herbalism) => 3.0,
            (FireMaster, S::Alchemy) => 6.0,
            (Rhizotomist, S::Herbalism) => 4.0,
            (HerbKnower, S::Herbalism) => 5.0,
            (Pharmacist, S::Herbalism) => 4.0,
            (Pharmacist, S::Medicine) => 3.0,
            (Dresser, S::Medicine) => 4.0,
            (HerbHealer, S::Medicine) => 5.0,
            (Surgeon, S::Medicine) => 6.0,
            (Archiatros, S::Medicine) => 6.0,
            _ => 0.0,
        }
    }

    /// Чего ещё не хватает (пусто — умение есть или вот-вот будет).
    pub fn missing(self, k: &Knowledge, earned: &[Perk]) -> Vec<Need> {
        self.info().needs.iter().filter(|n| !n.met(k, earned)).copied().collect()
    }

    /// Другие ветви, от которых зависит умение (для подсветки связей).
    pub fn links(self) -> Vec<Skill> {
        let own = self.info().skill;
        let mut v = Vec::new();
        for n in self.info().needs {
            let s = match *n {
                Need::Skill(s, _) => Some(s),
                Need::Perk(p) => Some(p.info().skill),
                Need::Lore(l) => l.skill(),
                Need::AnyLore(list) => list.first().and_then(|l| l.skill()),
                Need::HerbsKnown(_) => Some(S::Herbalism),
            };
            if let Some(s) = s
                && s != own
                && !v.contains(&s)
            {
                v.push(s);
            }
        }
        v
    }
}

/// Какие умения есть у персонажа (по порядку [`Perk::ALL`]).
pub fn earned(k: &Knowledge) -> Vec<Perk> {
    let mut out = Vec::new();
    // Умения ссылаются только на более ранние, так что хватает одного прохода.
    for p in Perk::ALL {
        if p.info().needs.iter().all(|n| n.met(k, &out)) {
            out.push(p);
        }
    }
    out
}

/// Прибавка к навыку от умений.
pub fn skill_bonus(k: &Knowledge, skill: Skill) -> f32 {
    // Быстрая проверка: у новичка умений нет.
    if k.skills.iter().all(|s| *s < 15.0) && !k.knows(Lore::Read(Language::Greek)) {
        return 0.0;
    }
    earned(k).into_iter().map(|p| p.skill_bonus(skill)).sum()
}

// ---------------------------------------------------------------------------
// Что навыки и умения меняют в игре
// ---------------------------------------------------------------------------

/// Поправки для боя, торговли и учёбы. Всё — множители (1 — без изменений) или прибавки.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mods {
    /// Ноша без одышки, кг (прибавка).
    pub carry: f32,
    /// Запас выносливости (прибавка).
    pub max_stamina: f32,
    pub stamina_regen: f32,
    pub melee_damage: f32,
    pub action_speed: f32,
    /// Устойчивость защиты (прибавка, 0.1 = 10%).
    pub block_stability: f32,
    /// Окно ответного удара (прибавка, секунды).
    pub riposte: f32,
    pub move_speed: f32,
    pub bow_spread: f32,
    pub bow_damage: f32,
    /// Цена покупки (меньше — выгоднее).
    pub buy: f32,
    /// Цена продажи (больше — выгоднее).
    pub sell: f32,
    /// Скорость чтения.
    pub reading: f32,
    /// Скорость учения у мастера.
    pub lessons: f32,
    /// Скорость письма (переписка книг).
    pub writing: f32,
    /// Прибавка к удаче уговорить стражу (0.1 = +10%).
    pub persuade: f32,
}

impl Default for Mods {
    fn default() -> Self {
        Self {
            carry: 0.0,
            max_stamina: 0.0,
            stamina_regen: 1.0,
            melee_damage: 1.0,
            action_speed: 1.0,
            block_stability: 0.0,
            riposte: 0.0,
            move_speed: 1.0,
            bow_spread: 1.0,
            bow_damage: 1.0,
            buy: 1.0,
            sell: 1.0,
            reading: 1.0,
            lessons: 1.0,
            writing: 1.0,
            persuade: 0.0,
        }
    }
}

impl Mods {
    pub fn of(k: &Knowledge) -> Mods {
        let s = |x: Skill| k.base_skill(x);
        let mut m = Mods::default();
        // Плавно от навыков: прибавки небольшие, бой решают руки игрока.
        m.carry += s(S::Strength) * 0.12;
        m.max_stamina += s(S::Strength) * 0.1 + s(S::Endurance) * 0.2;
        m.melee_damage *= 1.0 + s(S::Strength) * 0.0006;
        m.stamina_regen *= 1.0 + s(S::Endurance) * 0.002;
        m.block_stability += s(S::Arms) * 0.001;
        m.action_speed *= 1.0 + s(S::Arms) * 0.0004;
        m.bow_spread *= 1.0 - s(S::Archery) * 0.004;
        m.bow_damage *= 1.0 + s(S::Archery) * 0.001;
        m.buy *= 1.0 - s(S::Trade) * 0.0006;
        m.sell *= 1.0 + s(S::Trade) * 0.0006;
        m.reading *= 1.0 + s(S::Letters) * 0.005;
        m.lessons *= 1.0 + s(S::Letters) * 0.002;
        m.writing *= 1.0 + s(S::Letters) * 0.003;
        for p in earned(k) {
            use Perk::*;
            match p {
                PortersBack => m.carry += 8.0,
                HeavyHand => m.melee_damage *= 1.06,
                Hammerman => m.max_stamina += 10.0,
                SecondWind => m.stamina_regen *= 1.12,
                ArmourHabit => {
                    m.stamina_regen *= 1.08;
                    m.move_speed *= 1.03;
                }
                ForcedMarch => m.max_stamina += 15.0,
                Drill => m.block_stability += 0.06,
                TurnTheBlade => m.riposte += 0.08,
                Veteran => {
                    m.melee_damage *= 1.06;
                    m.action_speed *= 1.04;
                }
                Bucellarius => {
                    m.melee_damage *= 1.05;
                    m.block_stability += 0.05;
                }
                SteadyAim => m.bow_spread *= 0.65,
                SteppeBow => m.bow_damage *= 1.08,
                Sagittarius => {
                    m.bow_spread *= 0.7;
                    m.bow_damage *= 1.05;
                }
                KnowsPrice => {
                    m.buy *= 0.97;
                    m.sell *= 1.03;
                }
                Haggler => {
                    m.buy *= 0.97;
                    m.sell *= 1.03;
                }
                Merchant => {
                    m.buy *= 0.96;
                    m.sell *= 1.04;
                }
                Literate => m.reading *= 1.15,
                Bookman => {
                    m.reading *= 1.2;
                    m.lessons *= 1.1;
                }
                Rhetor => {
                    m.reading *= 1.15;
                    m.lessons *= 1.1;
                    m.buy *= 0.98;
                    m.sell *= 1.02;
                    m.persuade += 0.04;
                }
                Calligrapher => {
                    m.writing *= 1.3;
                    m.reading *= 1.1;
                }
                SweetWords => m.persuade += 0.08,
                Advocate => {
                    m.persuade += 0.1;
                    m.buy *= 0.98;
                    m.sell *= 1.02;
                }
                _ => {}
            }
        }
        m.bow_spread = m.bow_spread.max(0.15);
        m
    }
}

// ---------------------------------------------------------------------------
// Практика: сколько даёт дело
// ---------------------------------------------------------------------------

/// Опыт за удар, достигший живого противника.
pub const PRACTICE_HIT: f32 = 0.35;
/// За отражённый (парированный или принятый на щит) удар противника.
pub const PRACTICE_DEFEND: f32 = 0.45;
/// За попадание стрелой.
pub const PRACTICE_ARROW: f32 = 0.6;
/// За сделку: столько за каждые 100 долей оборота, но не больше [`PRACTICE_DEAL_MAX`].
pub const PRACTICE_DEAL_PER_100: f32 = 0.15;
pub const PRACTICE_DEAL_MAX: f32 = 2.5;
/// Выносливость за 100 потраченных единиц сил.
pub const PRACTICE_STAMINA_PER_100: f32 = 0.35;
/// Сила за секунду ходьбы с ношей больше 70% от предела.
pub const PRACTICE_LOAD_PER_SEC: f32 = 0.004;
/// Книжность за минуту чтения.
pub const PRACTICE_READ_PER_MIN: f32 = 0.6;

/// Опыт торговли за сделку на `turnover` долей.
pub fn deal_practice(turnover: u32) -> f32 {
    (turnover as f32 / 100.0 * PRACTICE_DEAL_PER_100).min(PRACTICE_DEAL_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perks_only_need_earlier_perks() {
        for (i, p) in Perk::ALL.iter().enumerate() {
            for n in p.info().needs {
                if let Need::Perk(q) = n {
                    let j = Perk::ALL.iter().position(|x| x == q).unwrap();
                    assert!(j < i, "{:?} требует более позднее {:?}", p, q);
                }
            }
        }
    }

    #[test]
    fn every_branch_has_a_first_step_and_tiers_are_sorted() {
        for s in Skill::ALL {
            let tiers: Vec<u8> = Perk::ALL.iter().filter(|p| p.info().skill == s).map(|p| p.info().tier).collect();
            assert!(tiers.contains(&0), "у ветви {} нет первой ступени", s.name());
            assert!(tiers.windows(2).all(|w| w[0] <= w[1]));
        }
    }

    #[test]
    fn tree_is_connected_across_branches() {
        // Почти каждая ветвь опирается на другую — древо связано.
        let linked: std::collections::BTreeSet<Skill> = Perk::ALL.iter().flat_map(|p| p.links()).collect();
        assert!(linked.len() >= 7, "связей мало: {linked:?}");
        for s in Skill::ALL {
            let out = Perk::ALL.iter().filter(|p| p.info().skill == s).any(|p| !p.links().is_empty());
            let into = Perk::ALL.iter().any(|p| p.links().contains(&s));
            assert!(out || into, "ветвь {} ни с чем не связана", s.name());
        }
    }

    #[test]
    fn novice_has_nothing_and_practice_unlocks_perks() {
        let mut k = Knowledge::default();
        assert!(earned(&k).is_empty());
        assert_eq!(Mods::of(&k), Mods::default());
        // Без знания приёма кузнечный навык не даёт «Глаза на цвет каления».
        k.skills[Skill::Smithing.index()] = 20.0;
        assert!(!earned(&k).contains(&Perk::ForgeEye));
        k.learn(Lore::Technique(Technique::Forging));
        assert!(earned(&k).contains(&Perk::ForgeEye));
        assert!((k.skill(Skill::Smithing) - 24.0).abs() < 1e-3);
        assert!((k.base_skill(Skill::Smithing) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn cross_branch_perk_needs_both_skills() {
        let mut k = Knowledge::default();
        k.skills[Skill::Medicine.index()] = 40.0;
        k.learn(Lore::Technique(Technique::WoundWashing));
        assert!(earned(&k).contains(&Perk::Dresser));
        assert!(!earned(&k).contains(&Perk::HerbHealer));
        k.skills[Skill::Herbalism.index()] = 30.0;
        k.learn(Lore::Technique(Technique::Gathering));
        let e = earned(&k);
        assert!(e.contains(&Perk::Rhizotomist) && e.contains(&Perk::HerbHealer));
        let missing = Perk::Surgeon.missing(&k, &e);
        assert!(missing.iter().any(|n| matches!(n, Need::Lore(_))));
    }

    #[test]
    fn smithing_makes_you_stronger() {
        let mut k = Knowledge::default();
        for _ in 0..50 {
            k.practice(Skill::Smithing, 1.0);
        }
        assert!(k.base_skill(Skill::Strength) > 10.0);
        assert!(k.base_skill(Skill::Strength) < k.base_skill(Skill::Smithing));
        assert!(Mods::of(&k).carry > 0.0);
    }

    #[test]
    fn mods_stay_modest_at_mastery() {
        let mut k = Knowledge::default();
        k.skills = [100.0; crate::lore::SKILL_COUNT];
        for t in Technique::ALL {
            k.learn(Lore::Technique(t));
        }
        for l in [Lore::Alphabet, Lore::Read(Language::Greek), Lore::Write(Language::Greek), Lore::Reckon, Lore::Grammar, Lore::Read(Language::Latin), Lore::Write(Language::Latin)] {
            k.learn(l);
        }
        for s in Substance::ALL {
            k.learn(Lore::Identify(s));
        }
        assert_eq!(earned(&k).len(), Perk::ALL.len());
        let m = Mods::of(&k);
        assert!(m.melee_damage < 1.3 && m.action_speed < 1.15);
        assert!(m.buy > 0.75 && m.sell < 1.3);
        assert!(m.bow_spread >= 0.15);
        assert!(k.skill(Skill::Medicine) <= 100.0);
    }

    #[test]
    fn deal_practice_is_capped() {
        assert!(deal_practice(50) < deal_practice(500));
        assert_eq!(deal_practice(1_000_000), PRACTICE_DEAL_MAX);
    }
}
