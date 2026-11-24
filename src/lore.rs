//! Знания персонажа: травы и вещества, приёмы работы, грамота и книги.
//!
//! Чистые данные без графики и сети — ими пользуются и сервер (кто что знает,
//! чему учит мастер), и клиент (как называть предмет в сумке).
//!
//! Устройство:
//! - [`Lore`] — единица знания: «читать по-гречески», «узнавать зверобой»,
//!   «свойства мирры», «приём: отвар».
//! - [`Knowledge`] — что знает один персонаж, плюс навыки (практика) и ход
//!   чтения книг и уроков.
//! - [`Substance`] — травы и вещества с описанием по Диоскориду; дикие травы
//!   неизвестны, пока их не научили узнавать.
//! - [`Book`] — книги и свитки: язык, время чтения, чему учат.
//! - [`Teacher`] — мастера и их программы (по порядку, с пререквизитами).

use crate::items::{Item, ItemBaseId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// Навыки и языки
// ---------------------------------------------------------------------------

/// Практический навык (0–100): растёт только от дела (и немного — у мастера).
/// Порядок вариантов — индекс в [`Knowledge::skills`]: новые добавлять только в конец.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Skill {
    Medicine,
    Herbalism,
    Alchemy,
    /// Кузнечное и литейное дело.
    Smithing,
    /// Сила: тяжёлый труд, ноша, удар.
    Strength,
    /// Выносливость: бег, долгий бой, работа без отдыха.
    Endurance,
    /// Ратное дело: ближний бой клинком, топором, копьём, щитом.
    Arms,
    /// Стрельба из лука.
    Archery,
    /// Торговля: торг, мена, знание цен.
    Trade,
    /// Книжность: чтение, письмо, учёба.
    Letters,
    /// Красноречие: уговорить стражника, сторговаться, заболтать.
    Eloquence,
}

/// Сколько всего навыков (длина [`Knowledge::skills`]).
pub const SKILL_COUNT: usize = 11;

impl Skill {
    /// Порядок для окон: тело, бой, ремёсла, знахарство, город.
    pub const ALL: [Skill; SKILL_COUNT] = [
        Skill::Strength,
        Skill::Endurance,
        Skill::Arms,
        Skill::Archery,
        Skill::Smithing,
        Skill::Alchemy,
        Skill::Herbalism,
        Skill::Medicine,
        Skill::Letters,
        Skill::Trade,
        Skill::Eloquence,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Skill::Medicine => "Врачевание",
            Skill::Herbalism => "Травничество",
            Skill::Alchemy => "Алхимия",
            Skill::Smithing => "Кузнечное дело",
            Skill::Strength => "Сила",
            Skill::Endurance => "Выносливость",
            Skill::Arms => "Ратное дело",
            Skill::Archery => "Стрельба из лука",
            Skill::Trade => "Торговля",
            Skill::Letters => "Книжность",
            Skill::Eloquence => "Красноречие",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Какие навыки понемногу растут вместе с этим (кузнец крепнет, лучник набирает силу рук).
    pub fn spillover(self) -> &'static [(Skill, f32)] {
        match self {
            Skill::Smithing => &[(Skill::Strength, 0.4), (Skill::Endurance, 0.15)],
            Skill::Arms => &[(Skill::Strength, 0.25), (Skill::Endurance, 0.3)],
            Skill::Archery => &[(Skill::Strength, 0.2)],
            Skill::Herbalism => &[(Skill::Endurance, 0.1)],
            Skill::Medicine => &[(Skill::Herbalism, 0.1)],
            Skill::Trade => &[(Skill::Letters, 0.05), (Skill::Eloquence, 0.15)],
            Skill::Eloquence => &[(Skill::Trade, 0.05)],
            _ => &[],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    Greek,
    Latin,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::Greek => "греческий",
            Language::Latin => "латынь",
        }
    }
}

// ---------------------------------------------------------------------------
// Вещества
// ---------------------------------------------------------------------------

/// Трава, смола, минерал или другое сырьё.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Substance {
    // Местные травы (дикие: без знания их не узнать).
    Sage,
    Thyme,
    StJohnsWort,
    Henbane,
    Juniper,
    Wormwood,
    Yarrow,
    Plantain,
    RoseHip,
    Mallow,
    Poppy,
    Mint,
    Madder,
    Woad,
    OakGall,
    Comfrey,
    Elecampane,
    // Степь.
    Licorice,
    Rhubarb,
    MuttonFat,
    // Привозное.
    Myrrh,
    Frankincense,
    Sulfur,
    Alum,
    Cinnabar,
    Opium,
    Pepper,
    Saffron,
    Cinnamon,
    Mastic,
    Naphtha,
    Natron,
    GumArabic,
    // Городские материалы.
    PineResin,
    Quicklime,
    CopperVitriol,
    Lead,
    CopperScrap,
    WoodAsh,
    Wine,
    OliveOil,
}

/// Описание вещества.
pub struct SubstanceInfo {
    /// Точное имя предмета в `ITEM_BASES`.
    pub item: &'static str,
    /// Латинский род: id точки сбора в `level::herb_patches()` (только у трав).
    #[allow(dead_code)] // понадобится, когда город отдаст свои точки сбора
    pub latin: Option<&'static str>,
    /// Название у Диоскорида / в греческих текстах.
    pub greek: &'static str,
    /// Дикое растение: без знания «узнавать» в сумке оно безымянное.
    pub wild: bool,
    /// Как называется, пока не опознано.
    pub unknown: &'static str,
    /// Что видит человек, который не знает растения.
    pub look: &'static str,
    /// Свойства (видны, когда изучены).
    pub uses: &'static str,
    /// Вкус и запах (для опыта на себе).
    #[allow(dead_code)] // понадобится опыту на себе (этап медицины)
    pub taste: &'static str,
    /// Ядовито в большой дозе.
    #[allow(dead_code)]
    pub toxic: bool,
}

macro_rules! sub {
    ($item:expr, $latin:expr, $greek:expr, $wild:expr, $unknown:expr, $look:expr, $uses:expr, $taste:expr, $toxic:expr) => {
        SubstanceInfo { item: $item, latin: $latin, greek: $greek, wild: $wild, unknown: $unknown, look: $look, uses: $uses, taste: $taste, toxic: $toxic }
    };
}

impl Substance {
    pub const ALL: [Substance; 41] = {
        use Substance::*;
        [
            Sage, Thyme, StJohnsWort, Henbane, Juniper, Wormwood, Yarrow, Plantain, RoseHip, Mallow, Poppy, Mint, Madder, Woad, OakGall, Comfrey, Elecampane, Licorice,
            Rhubarb, MuttonFat, Myrrh, Frankincense, Sulfur, Alum, Cinnabar, Opium, Pepper, Saffron, Cinnamon, Mastic, Naphtha, Natron, GumArabic, PineResin, Quicklime,
            CopperVitriol, Lead, CopperScrap, WoodAsh, Wine, OliveOil,
        ]
    };

    pub fn info(self) -> SubstanceInfo {
        use Substance::*;
        const PLANT: &str = "Неизвестная трава";
        const ROOT: &str = "Неизвестный корень";
        match self {
            Sage => sub!("Шалфей", Some("salvia"), "elelisphakon", true, PLANT,
                "Серо-зелёные бархатистые листья, резкий камфорный запах. Растёт на каменистых склонах.",
                "Вяжущее. Отваром промывают раны и полощут горло: не даёт ране воспалиться и снимает воспаление.",
                "Горьковато-пряный, вяжет во рту.", false),
            Thyme => sub!("Чабрец", Some("thymus"), "thymos", true, PLANT,
                "Стелющийся кустик с мелкими лиловыми цветками, сильный пряный запах.",
                "Согревает и сушит. Тёплый отвар понемногу укрепляет больного и слабо помогает при воспалении.",
                "Пряный, жгучий, приятный.", false),
            StJohnsWort => sub!("Зверобой", Some("hypericum"), "hyperikon", true, PLANT,
                "Жёлтые цветы, листья на просвет в мелких светлых точках, растёртый цветок красит пальцы красным.",
                "Масло из цветов на солнце — лучшее средство от ран и ожогов, основа ранозаживляющей мази.",
                "Горьковатый, смолистый.", false),
            Henbane => sub!("Белена", Some("hyoscyamus"), "hyoskyamos", true, PLANT,
                "Липкие волосистые листья с дурным запахом, грязно-жёлтые цветы в фиолетовых жилках. Растёт у жилья и на пустырях.",
                "Сильно унимает боль и наводит сон. Опасна: лишняя доза вызывает бред, жар и может убить.",
                "Неприятный, дурманящий: голова тяжелеет, во рту сохнет.", true),
            Juniper => sub!("Ягоды можжевельника", Some("juniperus"), "arkeuthos", true, "Неизвестные ягоды",
                "Иссиня-чёрные ягоды с сизым налётом на колючем кустарнике, смолистый запах.",
                "Согревает, гонит мочу. Входит в териак; дымом окуривают жилища больных.",
                "Смолистый, сладковато-горький.", false),
            Wormwood => sub!("Полынь", Some("artemisia"), "apsinthion", true, PLANT,
                "Серебристые рассечённые листья, очень сильный горький запах.",
                "Горечь очищает желудок: ослабляет лёгкое отравление, гонит глистов.",
                "Невыносимо горький.", false),
            Yarrow => sub!("Тысячелистник", Some("achillea"), "achilleios", true, PLANT,
                "Мелкие перистые листья, белые цветки собраны плоским щитком.",
                "Лучше всех трав останавливает кровь. По преданию, им Ахилл лечил Телефа.",
                "Горьковатый, терпкий.", false),
            Plantain => sub!("Подорожник", Some("plantago"), "arnoglosson", true, PLANT,
                "Широкие листья с продольными жилками, растёт у дорог.",
                "Размятый лист под повязкой останавливает кровь и чистит рану.",
                "Травянистый, чуть вяжет.", false),
            RoseHip => sub!("Шиповник", Some("rosa"), "kynosbatos", true, "Неизвестные плоды",
                "Колючий кустарник, продолговатые красные плоды.",
                "Вяжущий и укрепляющий отвар; понемногу восстанавливает силы.",
                "Кисло-сладкий, вяжет.", false),
            Mallow => sub!("Просвирник", Some("malva"), "malache", true, PLANT,
                "Округлые лопастные листья, розовые цветки, плоды как маленькие калачики.",
                "Мягчит: припарка снимает боль и отёк.",
                "Пресный, слизистый.", false),
            Poppy => sub!("Маковые коробочки", Some("papaver"), "mekon", true, "Неизвестные коробочки",
                "Сухие округлые коробочки с «короной» сверху, внутри гремят семена.",
                "Отвар коробочек унимает боль и наводит сон. Перебор опасен.",
                "Горький, клонит в сон.", true),
            Mint => sub!("Мята", Some("mentha"), "hedyosmos", true, PLANT,
                "Четырёхгранный стебель, супротивные листья, холодящий запах. Растёт у ручьёв.",
                "Унимает тошноту, улучшает вкус горьких отваров.",
                "Холодящий, свежий.", false),
            Madder => sub!("Корень марены", Some("rubia"), "erythrodanon", true, ROOT,
                "Цепкие стебли с мутовками жёстких листьев, корень на срезе красный.",
                "Красильный корень: с квасцами даёт стойкую красную краску.",
                "Горьковато-сладкий, красит слюну.", false),
            Woad => sub!("Вайда", Some("isatis"), "isatis", true, PLANT,
                "Сизые продолговатые листья, метёлки мелких жёлтых цветков.",
                "Синяя краска (листья бродят в щелочном чане). По Диоскориду листья заживляют раны.",
                "Горький, капустный.", false),
            OakGall => sub!("Дубовые галлы", Some("quercus"), "kekis", true, "Странные наросты",
                "Твёрдые круглые шарики на дубовых ветках, лёгкие, с дырочкой.",
                "Сильно вяжут. С купоросом и камедью дают чёрные чернила.",
                "Очень терпкий, сводит рот.", false),
            Comfrey => sub!("Корень окопника", Some("symphytum"), "symphyton", true, ROOT,
                "Корень чёрный снаружи, белый и слизистый внутри; растёт в сырых местах у ручьёв.",
                "«Сращивает»: слизистый корень склеивает края глубоких ран.",
                "Пресный, слизистый.", false),
            Elecampane => sub!("Корень девясила", Some("inula"), "helenion", true, ROOT,
                "Крупные шершавые листья, жёлтые корзинки, душистый толстый корень.",
                "Грудное средство, в отварах и оксимеле.",
                "Пряно-горький, душистый.", false),
            Licorice => sub!("Солодковый корень", None, "glykyrrhiza", false, ROOT,
                "Жёлтый волокнистый корень.",
                "«Скифский корень» Феофраста: утоляет жажду, смягчает грудь, сглаживает горечь лекарств.",
                "Очень сладкий.", false),
            Rhubarb => sub!("Ревень понтийский", None, "rha", false, ROOT,
                "Тёмный корень, жёлтый на изломе.",
                "Rha с берегов Ра (Волги), по Диоскориду: очищает желудок, слабит, вяжет.",
                "Горький, вяжущий.", false),
            MuttonFat => sub!("Бараний жир", None, "stear", false, "Жир",
                "Белое твёрдое сало.",
                "Основа мазей и мыла.", "Жирный.", false),
            Myrrh => sub!("Мирра", None, "smyrna", false, "Смола",
                "Красновато-бурые комочки ароматной смолы.",
                "Сушит и чистит раны, не даёт гнить. Входит в мази и териак.",
                "Горький, ароматный.", false),
            Frankincense => sub!("Ладан", None, "libanos", false, "Смола",
                "Желтоватые прозрачные слёзы смолы.",
                "Порошок на рану останавливает кровь и заживляет.",
                "Смолистый, горьковатый.", false),
            Sulfur => sub!("Сера", None, "theion apyron", false, "Жёлтый камень",
                "Жёлтые хрупкие куски, горят синим удушливым пламенем.",
                "Лечит кожу, в окуриваниях. В алхимии — серная вода; в зажигательных смесях.",
                "Не для еды.", false),
            Alum => sub!("Квасцы", None, "stypteria", false, "Белый камень",
                "Белые кристаллы с вяжущим вкусом.",
                "Протрава красильщиков; вяжет и останавливает кровь.",
                "Сильно вяжущий, кислый.", false),
            Cinnabar => sub!("Киноварь", None, "kinnabari", false, "Красный камень",
                "Тяжёлый ярко-красный минерал.",
                "Дорогая красная краска. При возгонке даёт ртуть; пары ядовиты.",
                "Не для еды.", true),
            Opium => sub!("Опий", None, "opion", false, "Тёмный сок",
                "Бурые комки высохшего млечного сока мака.",
                "Сильнейшее средство от боли и бессонницы; основа териака. Чуть больше — и сон не кончится.",
                "Очень горький, дурманит.", true),
            Pepper => sub!("Перец", None, "peperi", false, "Пряность",
                "Чёрные морщинистые горошины из Индии.",
                "Согревает, входит в териак.", "Жгучий.", false),
            Saffron => sub!("Шафран", None, "krokos", false, "Пряность",
                "Тёмно-оранжевые нити рылец.",
                "Дорогая пряность и краска, входит в териак и глазные мази.", "Горьковато-медовый.", false),
            Cinnamon => sub!("Корица", None, "kinnamomon", false, "Пряность",
                "Свёрнутые трубочками куски душистой коры.",
                "Согревает, входит в териак.", "Сладко-жгучий.", false),
            Mastic => sub!("Хиосская мастика", None, "mastiche", false, "Смола",
                "Прозрачные бледные слёзы смолы с Хиоса.",
                "Укрепляет дёсны и желудок, связывает мази.", "Смолистый, свежий.", false),
            Naphtha => sub!("Нефть", None, "naphtha", false, "Чёрное масло",
                "Густое тёмное масло с резким запахом, с Боспора и Тамани.",
                "Горит так, что вода не тушит. Основа зажигательных смесей.", "Не для еды.", false),
            Natron => sub!("Натр", None, "nitron", false, "Белая соль",
                "Сероватая щелочная соль из Египта.",
                "Моет и чистит, нужен стеклоделам и красильщикам.", "Мыльный, солёный.", false),
            GumArabic => sub!("Камедь", None, "kommi", false, "Смола",
                "Прозрачные желтоватые комочки, растворяются в воде.",
                "Связывает чернила и краски, смягчает горло.", "Пресный, клейкий.", false),
            PineResin => sub!("Сосновая смола", None, "pitys rhetine", false, "Смола",
                "Липкая пахучая смола крымской сосны.",
                "Горит; перегонкой даёт смоляное масло. Входит в пластыри.", "Смолистый.", false),
            Quicklime => sub!("Негашёная известь", None, "titanos", false, "Белый порошок",
                "Белые куски обожжённого известняка, с водой шипят и греются.",
                "Едкая; с серой и водой даёт серную воду. Нужна строителям.", "Жжёт.", false),
            CopperVitriol => sub!("Медный купорос", None, "chalkanthon", false, "Синий камень",
                "Синие прозрачные кристаллы.",
                "С галлами даёт чернила; прижигает дикое мясо в ранах.", "Металлический, рвотный.", true),
            Lead => sub!("Свинец", None, "molybdos", false, "Металл",
                "Мягкий тяжёлый серый металл.",
                "Над уксусом даёт свинцовые белила.", "Не для еды.", true),
            CopperScrap => sub!("Медный лом", None, "chalkos", false, "Металл",
                "Обрезки и старые медные вещи.",
                "Над уксусом даёт ярь-медянку.", "Не для еды.", false),
            WoodAsh => sub!("Древесная зола", None, "tephra", false, "Зола",
                "Серая зола из очага.",
                "Залитая водой даёт щёлок для мыла и красок.", "Горький, щелочной.", false),
            Wine => sub!("Кувшин вина", None, "oinos", false, "Вино",
                "Кувшин простого вина.",
                "Промывают раны (Гиппократ). Скисая, даёт уксус.", "Вино.", false),
            OliveOil => sub!("Кувшин оливкового масла", None, "elaion", false, "Масло",
                "Кувшин оливкового масла.",
                "Основа масляных настоев, мазей и кератов.", "Масло.", false),
        }
    }

    pub fn item(self) -> Option<ItemBaseId> {
        ItemBaseId::find(self.info().item)
    }

    /// Вещество предмета; сушёное и толчёное — то же растение или минерал.
    pub fn from_item(base: ItemBaseId) -> Option<Substance> {
        let name = crate::brewing::canonical(base.base().name).0;
        Substance::ALL.into_iter().find(|s| s.info().item == name)
    }

    #[allow(dead_code)] // для level::herb_patches(), когда город будет в main
    pub fn from_latin(latin: &str) -> Option<Substance> {
        Substance::ALL.into_iter().find(|s| s.info().latin == Some(latin))
    }

    pub fn is_wild(self) -> bool {
        self.info().wild
    }
}

// ---------------------------------------------------------------------------
// Приёмы
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Technique {
    /// Ризотомия: когда и как собирать, не губя корни.
    Gathering,
    Poultice,
    WoundWashing,
    Decoction,
    Dosage,
    Maceration,
    Salve,
    Oxymel,
    Theriac,
    Fermentation,
    Distillation,
    Sublimation,
    Calcination,
    Mordanting,
    VatDyeing,
    Ink,
    Soap,
    Incendiary,
    Glassblowing,
    Pottery,
    /// Цельс, книга VII: вырезать или вытолкнуть наконечник, не порвав жил.
    ArrowExtraction,
    // --- Кузнечное дело (smithing.rs) ---
    /// Ковка: по цвету каления знать, когда бить, и как вытягивать металл.
    Forging,
    /// Кузнечная сварка на белом жару (пакет, наварка стальных лезвий, проковка крицы).
    ForgeWelding,
    /// Закалка: с вишнёвого жара в воду, рассол или масло.
    Quenching,
    /// Отпуск по цветам побежалости.
    Tempering,
    /// Сыродутная плавка руды в горне.
    Smelting,
    /// Цементация: железо в угле в запечатанном горшке становится сталью.
    Carburizing,
    /// Литьё бронзы, серебра и свинца в каменные формы.
    Casting,
    /// Литьё по восковой модели в глиняную форму.
    LostWax,
    /// Кольчужное плетение: проволока, кольца, заклёпки.
    MailMaking,
    /// Доспешное дело: пластины, ламеллы, шлемы.
    Armoury,
    /// Гиппократ «О переломах», Цельс, книга VIII: вытянуть, вправить, наложить лубки и бинты.
    Bonesetting,
    // --- Горное дело и углежжение (mining.rs) ---
    /// Рудознатство: отличать руду от пустой породы, болотную руду — от рыжей глины.
    Prospecting,
    /// Горное дело: вести штольню, ставить крепь.
    Mining,
    /// Обжиг руды на кострище перед плавкой.
    Roasting,
    /// Углежжение: сложить, укрыть и вести угольную кучу.
    Charcoal,
}

impl Technique {
    pub const ALL: [Technique; 36] = {
        use Technique::*;
        [
            Gathering, Poultice, WoundWashing, Decoction, Dosage, Maceration, Salve, Oxymel, Theriac, Fermentation, Distillation, Sublimation, Calcination, Mordanting,
            VatDyeing, Ink, Soap, Incendiary, Glassblowing, Pottery, ArrowExtraction, Forging, ForgeWelding, Quenching, Tempering, Smelting, Carburizing, Casting, LostWax,
            MailMaking, Armoury, Bonesetting, Prospecting, Mining, Roasting, Charcoal,
        ]
    };

    pub fn name(self) -> &'static str {
        use Technique::*;
        match self {
            Gathering => "Сбор трав",
            Poultice => "Припарка под повязку",
            WoundWashing => "Промывание ран вином",
            Decoction => "Отвар",
            Dosage => "Дозировка",
            Maceration => "Масляный настой",
            Salve => "Мази и кераты",
            Oxymel => "Оксимель",
            Theriac => "Териак",
            Fermentation => "Брожение (уксус)",
            Distillation => "Перегонка в алембике",
            Sublimation => "Возгонка",
            Calcination => "Обжиг в тигле",
            Mordanting => "Протрава квасцами",
            VatDyeing => "Крашение в чане",
            Ink => "Чернила",
            Soap => "Щёлок и мыло",
            Incendiary => "Зажигательные смеси",
            Glassblowing => "Выдувание стекла",
            Pottery => "Гончарное дело",
            ArrowExtraction => "Извлечение стрел",
            Forging => "Ковка",
            ForgeWelding => "Кузнечная сварка",
            Quenching => "Закалка",
            Tempering => "Отпуск",
            Smelting => "Сыродутная плавка",
            Carburizing => "Цементация",
            Casting => "Литьё в формы",
            LostWax => "Литьё по воску",
            MailMaking => "Кольчужное плетение",
            Armoury => "Доспешное дело",
            Bonesetting => "Вправление переломов",
            Prospecting => "Рудознатство",
            Mining => "Горное дело (крепь)",
            Roasting => "Обжиг руды",
            Charcoal => "Углежжение",
        }
    }

    pub fn description(self) -> &'static str {
        use Technique::*;
        match self {
            Gathering => "Рвать в сухую погоду, корни выкапывать, не губя куста. Больше сбор, растения быстрее отрастают.",
            Poultice => "Размять свежие листья, приложить к ране и прибинтовать.",
            WoundWashing => "Промыть рану вином перед перевязкой, чтобы не воспалилась.",
            Decoction => "Траву в горшок с водой, варить на слабом огне, уварить до трети и процедить.",
            Dosage => "Мерить драхмами и оболами: сколько можно дать, чтобы помочь, а не убить.",
            Maceration => "Цветы в оливковом масле стоят на солнце, пока масло не окрасится.",
            Salve => "Воск с маслом (около одной части к четырём) на водяной бане, с травами или смолами.",
            Oxymel => "Мёд с уксусом и водой уварить до сиропа.",
            Theriac => "Сложнейшее противоядие Галена: опий, мирра, пряности, можжевельник, мёд и вино, долгая выдержка.",
            Fermentation => "Вино в открытом горшке в тепле скисает в уксус; дольше — крепче.",
            Distillation => "Кукурбита с веществом, сверху амбик, пары стекают в приёмник. Огонь ровный, стекло боится рывков жара.",
            Sublimation => "Вещество в чаше под амбиком на сильном огне: пары оседают на холодной крышке. Работать на ветру.",
            Calcination => "Обжиг в тигле на сильном огне: белила становятся суриком.",
            Mordanting => "Ткань и краску варят с квасцами, чтобы цвет держался.",
            VatDyeing => "Листья вайды бродят в щелочном чане несколько дней.",
            Ink => "Толчёные галлы настоять, добавить купорос и камедь.",
            Soap => "Золу залить водой — выйдет щёлок; щёлок сварить с жиром.",
            Incendiary => "Смола, сера, ладан и пакля в глиняном горшке; с нефтью горит сильнее.",
            Glassblowing => "Выдувать флаконы, реторты и амбики из стекла.",
            Pottery => "Лепить и обжигать горшки и тигли.",
            ArrowExtraction => "Расширить рану, вынуть наконечник по ходу древка или вытолкнуть насквозь, не порвав жил; потом промыть и перевязать.",
            Forging => "Бить, пока металл светится от вишнёвого до жёлтого. Тяжёлым молотом тянуть, лёгким выглаживать. Остывшее не бить: пойдёт трещинами.",
            ForgeWelding => "Пакет греть до белого, почти искрящего жара и сразу бить тяжело и часто, пока слои не схватятся. Перегреешь — железо горит.",
            Quenching => "Сталь греть до вишнёвого (не выше) и разом окунуть. Вода — твёрже, но может треснуть; масло мягче. Плиний: мелкое железо калят в масле.",
            Tempering => "Закалённое зачистить до блеска и греть с краю углей. Цвета: соломенный — для ножа, пурпурный и синий — для меча, серый — перегрел.",
            Smelting => "Руду и уголь слоями, примерно поровну по весу, и непрерывно дуть мехами. Жёлто-белый жар держать часами: холоднее — сырая крица, жарче — чугун.",
            Carburizing => "Железную полосу обложить углём в глиняном горшке, замазать и долго держать в красном жару — поверхность наберёт углерода.",
            Casting => "Бронза — девять частей меди на одну олова. Расплавить в тигле до жёлтого, снять шлак, прогреть форму и лить ровно, не прерываясь.",
            LostWax => "Вылепить вещь из воска, облепить глиной, выжечь воск — и лить в пустоту. Форма разбивается, зато вещь тонкая.",
            MailMaking => "Тянуть проволоку, навить кольца, каждое кольцо продеть в четыре соседних и заклепать холодным, лёгкими точными ударами.",
            Armoury => "Пластины выковать, пробить отверстия, связать ремнями; шлем собрать из сегментов на каркасе и склепать.",
            Bonesetting => "Сломанную кость вытянуть за концы, пока обломки не встанут на место, обернуть льном и уложить в лубки. Неподвижно сорок дней — иначе срастётся криво.",
            Prospecting => "Руда тяжелее камня. Бурый железняк — охристый, на изломе ржавый, черта по камню бурая. Болотную руду ищут у ржавых ключей, где на воде радужная плёнка: под дёрном ноздреватые бурые комья.",
            Mining => "Штольню ведут в склон с уклоном к выходу, чтобы стекала вода. Каждые полтора локтя — крепь: две стойки и верхняк, между ними затяжка из жердей. Плиний: галереи держат брёвнами, иначе кровля садится.",
            Roasting => "Руду кладут на костёр из дров слоями и ворошат. Уходит вода и сера, руда буреет и трескается — легче дробить, лучше плавится. Перегреть — спечётся в шлаковую корку.",
            Charcoal => "Феофраст: плотные поленья дуба ставят вокруг кола, кроют листвой, дёрном и землёй, поджигают сверху. Отдушинами правят воздух: дым белый — сохнет, жёлтый — обугливается, голубой и прозрачный — готово, глушить землёй. Прогары засыпать сразу.",
        }
    }

    pub fn skill(self) -> Skill {
        use Technique::*;
        match self {
            Gathering | Maceration => Skill::Herbalism,
            Poultice | WoundWashing | Dosage | Theriac | ArrowExtraction | Bonesetting => Skill::Medicine,
            Decoction | Salve | Oxymel => Skill::Herbalism,
            Forging | ForgeWelding | Quenching | Tempering | Smelting | Carburizing | Casting | LostWax | MailMaking | Armoury => Skill::Smithing,
            Prospecting | Mining | Roasting | Charcoal => Skill::Smithing,
            _ => Skill::Alchemy,
        }
    }
}

// ---------------------------------------------------------------------------
// Знание
// ---------------------------------------------------------------------------

/// Единица знания.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lore {
    /// Грамота.
    Read(Language),
    /// Узнавать растение (имя в сумке).
    Identify(Substance),
    /// Знать свойства вещества.
    Properties(Substance),
    Technique(Technique),
    /// Азбука: буквы (они же цифры) — первая ступень грамоты.
    Alphabet,
    /// Письмо: выводить буквы и слова стилем по воску и каламом по пергаменту.
    Write(Language),
    /// Счёт: греческие цифры, счёт на камешках (абаке), доли и проценты.
    Reckon,
    /// Грамматика: части речи, склонения, чтение Гомера с толкованием (Дионисий Фракиец).
    Grammar,
}

impl Lore {
    pub fn name(self) -> String {
        match self {
            Lore::Read(l) => format!("Грамота: {}", l.name()),
            Lore::Identify(s) => format!("Узнавать: {}", s.info().item.to_lowercase()),
            Lore::Properties(s) => format!("Свойства: {}", s.info().item.to_lowercase()),
            Lore::Technique(t) => format!("Приём: {}", t.name().to_lowercase()),
            Lore::Alphabet => "Грамота: азбука".into(),
            Lore::Write(l) => format!("Письмо: {}", l.name()),
            Lore::Reckon => "Счёт и цифры".into(),
            Lore::Grammar => "Грамматика".into(),
        }
    }

    /// Предмет школьной науки (грамота, письмо, счёт, грамматика).
    pub fn is_letters(self) -> bool {
        matches!(self, Lore::Read(_) | Lore::Alphabet | Lore::Write(_) | Lore::Reckon | Lore::Grammar)
    }

    /// Короткое название предмета для школьного окна.
    pub fn subject(self) -> String {
        match self {
            Lore::Alphabet => "Азбука".into(),
            Lore::Read(Language::Greek) => "Чтение".into(),
            Lore::Read(Language::Latin) => "Латынь: чтение".into(),
            Lore::Write(Language::Greek) => "Письмо".into(),
            Lore::Write(Language::Latin) => "Латынь: письмо".into(),
            Lore::Reckon => "Счёт".into(),
            Lore::Grammar => "Грамматика".into(),
            other => other.name(),
        }
    }

    /// Чем занимаются на уроке (для окна школы).
    pub fn subject_about(self) -> &'static str {
        match self {
            Lore::Alphabet => "Двадцать четыре буквы: учитель выводит образец на воске, вы повторяете стилем. Буквы — они же и цифры.",
            Lore::Read(Language::Greek) => "Слоги, потом слова, потом Псалтирь: вести указку за строкой и читать вслух.",
            Lore::Read(Language::Latin) => "Латинские буквы и слова по Вергилию и законам. Язык претории и старых книг.",
            Lore::Write(Language::Greek) => "Слова и строки: стилем по воску, потом каламом и чернилами. Без письма не переписать книгу.",
            Lore::Write(Language::Latin) => "Латинское письмо: прошения и грамоты на языке закона.",
            Lore::Reckon => "Греческие цифры и счёт камешками на абаке: сотни, десятки, единицы.",
            Lore::Grammar => "Части речи, склонения, разбор Гомера. Без грамматики латыни не выучить.",
            _ => "",
        }
    }

    /// Сколько часов уроков у мастера нужно, чтобы выучить.
    pub fn lesson_hours(self) -> f32 {
        use Technique::*;
        match self {
            Lore::Alphabet => 2.0,
            Lore::Read(Language::Greek) => 6.0,
            Lore::Read(Language::Latin) => 8.0,
            Lore::Write(Language::Greek) => 4.0,
            Lore::Write(Language::Latin) => 3.0,
            Lore::Reckon => 3.0,
            Lore::Grammar => 6.0,
            Lore::Identify(_) => 0.5,
            Lore::Properties(_) => 1.0,
            Lore::Technique(t) => match t {
                Poultice | WoundWashing => 0.5,
                Gathering | Decoction | Maceration | Oxymel | Fermentation | Ink => 1.0,
                Salve | Calcination | Mordanting | Soap => 2.0,
                Dosage | Incendiary | VatDyeing | ArrowExtraction | Bonesetting => 3.0,
                Distillation | Sublimation => 4.0,
                Theriac | Pottery => 6.0,
                Glassblowing => 10.0,
                Forging => 2.0,
                ForgeWelding | Quenching | Casting => 3.0,
                Tempering | Carburizing => 2.0,
                Smelting | LostWax => 4.0,
                Armoury => 6.0,
                MailMaking => 8.0,
                Prospecting | Roasting => 2.0,
                Mining | Charcoal => 3.0,
            },
        }
    }

    /// Что нужно знать прежде (мастер не учит через голову).
    pub fn requires(self) -> Vec<Lore> {
        use Technique::*;
        match self {
            Lore::Read(Language::Greek) => vec![Lore::Alphabet],
            Lore::Write(Language::Greek) => vec![Lore::Read(Language::Greek)],
            Lore::Reckon => vec![Lore::Alphabet],
            Lore::Grammar => vec![Lore::Write(Language::Greek)],
            Lore::Read(Language::Latin) => vec![Lore::Grammar],
            Lore::Write(Language::Latin) => vec![Lore::Read(Language::Latin)],
            Lore::Technique(Theriac) => vec![Lore::Technique(Dosage), Lore::Technique(Decoction), Lore::Properties(Substance::Opium)],
            Lore::Technique(Sublimation) | Lore::Technique(Calcination) => vec![Lore::Technique(Distillation)],
            Lore::Technique(VatDyeing) => vec![Lore::Technique(Mordanting)],
            Lore::Technique(Oxymel) => vec![Lore::Technique(Decoction)],
            Lore::Technique(Dosage) => vec![Lore::Technique(Decoction)],
            Lore::Technique(ForgeWelding) | Lore::Technique(Quenching) | Lore::Technique(Smelting) | Lore::Technique(MailMaking) | Lore::Technique(Armoury) => vec![Lore::Technique(Forging)],
            Lore::Technique(Tempering) => vec![Lore::Technique(Quenching)],
            Lore::Technique(Carburizing) => vec![Lore::Technique(ForgeWelding)],
            Lore::Technique(LostWax) => vec![Lore::Technique(Casting)],
            Lore::Technique(Mining) => vec![Lore::Technique(Prospecting)],
            _ => vec![],
        }
    }

    /// Каким навыком подкрепляется практика этого знания.
    pub fn skill(self) -> Option<Skill> {
        match self {
            Lore::Read(_) | Lore::Alphabet | Lore::Write(_) | Lore::Reckon | Lore::Grammar => Some(Skill::Letters),
            Lore::Identify(_) | Lore::Properties(_) => Some(Skill::Herbalism),
            Lore::Technique(t) => Some(t.skill()),
        }
    }
}

// ---------------------------------------------------------------------------
// Книги и свитки
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Book {
    HerbalFragment,
    Dioscorides,
    IllustratedHerbal,
    HippocratesUlcers,
    GalenSimples,
    GalenAntidotes,
    Oribasius,
    Zosimos,
    DyerBook,
    Aeneas,
    Theophrastus,
    Celsus,
    Pliny,
    /// Псалтирь: по ней учатся читать. Знающий азбуку осилит её сам — по слогам, долго.
    Psalter,
    /// Дионисий Фракиец, «Искусство грамматики» — школьный учебник со II в. до н. э.
    DionysiusThrax,
    /// Никомах Герасский, «Введение в арифметику».
    Nicomachus,
    /// Феофраст, «Исследование о растениях»: книга V об угле и дереве.
    TheophrastusPlants,
}

pub struct BookInfo {
    pub item: &'static str,
    pub language: Language,
    /// «свиток», «кодекс», «обрывок папируса».
    pub form: &'static str,
    /// Секунд чтения от начала до конца (прогресс сохраняется).
    pub read_secs: f32,
    pub about: &'static str,
}

impl Book {
    pub const ALL: [Book; 17] = {
        use Book::*;
        [HerbalFragment, Dioscorides, IllustratedHerbal, HippocratesUlcers, GalenSimples, GalenAntidotes, Oribasius, Zosimos, DyerBook, Aeneas, Theophrastus, Celsus, Pliny, Psalter, DionysiusThrax, Nicomachus, TheophrastusPlants]
    };

    pub fn info(self) -> BookInfo {
        use Book::*;
        let t = |item, language, form, read_secs, about| BookInfo { item, language, form, read_secs, about };
        match self {
            HerbalFragment => t("Обрывок травника", Language::Greek, "обрывок папируса", 15.0, "Клочок чьего-то травника: одно растение, как узнать и чем полезно."),
            Dioscorides => t(
                "Диоскорид. О лекарственных веществах",
                Language::Greek,
                "свиток",
                180.0,
                "Педаний Диоскорид, I век. Свойства трав, смол и минералов. Без рисунков: в поле по нему траву не узнаешь.",
            ),
            IllustratedHerbal => t(
                "Иллюстрированный травник",
                Language::Greek,
                "пергаментный кодекс",
                150.0,
                "Роскошный кодекс с рисунками растений, как тот, что в Константинополе делают для Аникии Юлианы. По рисункам узнаёшь травы.",
            ),
            HippocratesUlcers => t("Гиппократ. О язвах", Language::Greek, "свиток", 120.0, "Как лечить раны: промывать вином, прикладывать травы, перевязывать."),
            GalenSimples => t("Гален. О простых лекарствах", Language::Greek, "кодекс", 240.0, "Гален Пергамский о силе простых веществ: смолы, соли, пряности, мази."),
            GalenAntidotes => t("Гален. О противоядиях", Language::Greek, "кодекс", 300.0, "Териак и другие противоядия. Без основ врачевания непонятен."),
            Oribasius => t("Орибасий. Синопсис", Language::Greek, "кодекс", 200.0, "Краткий свод врача Юлиана: отвары, дозы, оксимель."),
            Zosimos => t("Зосим. О приборах и печах", Language::Greek, "свиток", 240.0, "Зосим Панополитанский: амбик, водяная баня Марии, керотакис, серная вода."),
            DyerBook => t("Книга красильщика", Language::Greek, "рабочие записи на папирусе", 150.0, "Рецепты египетских красильщиков: квасцы, марена, вайда, чернила, щёлок."),
            Aeneas => t("Эней Тактик. Об обороне городов", Language::Greek, "свиток", 150.0, "Как оборонять осаждённый город, в том числе огнём: смола, сера, пакля, ладан."),
            Theophrastus => t("Феофраст. О камнях", Language::Greek, "свиток", 150.0, "О минералах и красках: киноварь, свинцовые белила, ярь-медянка."),
            Celsus => t("Цельс. О медицине", Language::Latin, "кодекс", 240.0, "Авл Корнелий Цельс по-латыни: раны, кровотечения, мази, дозы, переломы."),
            Pliny => t(
                "Плиний. Естественная история",
                Language::Latin,
                "кодекс (книги XXXIII–XXXIV)",
                240.0,
                "Плиний Старший о металлах: где и как ищут руду, как ведут и крепят штольни, как плавят руду, как калят железо в воде и масле, сколько олова класть в бронзу.",
            ),
            TheophrastusPlants => t(
                "Феофраст. Исследование о растениях",
                Language::Greek,
                "свиток (книга V)",
                150.0,
                "Ученик Аристотеля о деревьях: какое дерево на что годится и как углежоги в горах жгут уголь в кучах под землёй.",
            ),
            Psalter => t(
                "Псалтирь",
                Language::Greek,
                "пергаментный кодекс",
                420.0,
                "Сто пятьдесят псалмов. По ней в школе учат читать: знающий одни буквы разберёт её сам, по слогам — и выучится читать.",
            ),
            DionysiusThrax => t(
                "Дионисий Фракиец. Искусство грамматики",
                Language::Greek,
                "свиток",
                200.0,
                "Тонкий учебник, по которому грамматики учат уже семь веков: буквы, слоги, восемь частей речи, склонения. Нужно уметь писать.",
            ),
            Nicomachus => t(
                "Никомах. Введение в арифметику",
                Language::Greek,
                "свиток",
                180.0,
                "Никомах Герасский о числах: чётные и нечётные, доли, отношения. С ним и цифры на абаке пойдут.",
            ),
        }
    }

    /// Сколько тетрадей пергамента уходит на список (переписку).
    pub fn quires(self) -> u16 {
        ((self.info().read_secs / 60.0).ceil() as u16).max(1)
    }

    pub fn from_item(base: ItemBaseId) -> Option<Book> {
        let name = base.base().name;
        Book::ALL.into_iter().find(|t| t.info().item == name)
    }

    /// Что нужно знать, чтобы понять текст (кроме грамоты).
    pub fn requires(self) -> Vec<Lore> {
        match self {
            Book::GalenAntidotes => vec![Lore::Technique(Technique::Decoction)],
            Book::Zosimos => vec![Lore::Technique(Technique::Decoction)],
            Book::DionysiusThrax => vec![Lore::Write(Language::Greek)],
            _ => vec![],
        }
    }

    /// Чему учит (обрывок травника — одной случайной траве, это решает сервер).
    pub fn teaches(self) -> Vec<Lore> {
        use Substance::*;
        use Technique::*;
        let props = |list: &[Substance]| list.iter().map(|s| Lore::Properties(*s)).collect::<Vec<_>>();
        match self {
            Book::HerbalFragment => vec![],
            Book::Dioscorides => {
                let mut v = props(&Substance::ALL.into_iter().filter(|s| !matches!(s, Wine | OliveOil | WoodAsh | CopperScrap)).collect::<Vec<_>>());
                v.push(Lore::Technique(Decoction));
                v.push(Lore::Technique(Gathering));
                v
            }
            Book::IllustratedHerbal => Substance::ALL.into_iter().filter(|s| s.is_wild()).map(Lore::Identify).collect(),
            Book::HippocratesUlcers => {
                let mut v = vec![Lore::Technique(WoundWashing), Lore::Technique(Poultice)];
                v.extend(props(&[Plantain, Mallow, Wine]));
                v
            }
            Book::GalenSimples => {
                let mut v = vec![Lore::Technique(Salve)];
                v.extend(props(&[Myrrh, Frankincense, Alum, Mastic, Pepper, Saffron, Cinnamon, OliveOil, MuttonFat, StJohnsWort]));
                v
            }
            Book::GalenAntidotes => {
                let mut v = vec![Lore::Technique(Dosage), Lore::Technique(Theriac)];
                v.extend(props(&[Opium, Juniper, Cinnamon, Saffron, Myrrh]));
                v
            }
            Book::Oribasius => {
                let mut v = vec![Lore::Technique(Decoction), Lore::Technique(Dosage), Lore::Technique(Oxymel), Lore::Technique(Maceration)];
                v.extend(props(&[Sage, Thyme, Poppy, Henbane, RoseHip, Elecampane, Wormwood]));
                v
            }
            Book::Zosimos => {
                let mut v = vec![Lore::Technique(Distillation), Lore::Technique(Sublimation)];
                v.extend(props(&[Sulfur, Quicklime, Cinnabar]));
                v
            }
            Book::DyerBook => {
                let mut v = vec![Lore::Technique(Mordanting), Lore::Technique(VatDyeing), Lore::Technique(Ink), Lore::Technique(Soap)];
                v.extend(props(&[Alum, Madder, Woad, OakGall, CopperVitriol, GumArabic, WoodAsh, Natron]));
                v
            }
            Book::Aeneas => {
                let mut v = vec![Lore::Technique(Incendiary)];
                v.extend(props(&[PineResin, Sulfur, Naphtha, Frankincense]));
                v
            }
            Book::Theophrastus => {
                let mut v = vec![Lore::Technique(Calcination), Lore::Technique(Fermentation)];
                v.extend(props(&[Cinnabar, Lead, CopperScrap, CopperVitriol]));
                v
            }
            Book::Celsus => {
                let mut v = vec![Lore::Technique(WoundWashing), Lore::Technique(Poultice), Lore::Technique(Salve), Lore::Technique(Dosage), Lore::Technique(ArrowExtraction), Lore::Technique(Bonesetting)];
                v.extend(props(&[Yarrow, Plantain, Frankincense, Alum, Myrrh, Comfrey]));
                v
            }
            // Только теория: руками это всё равно придётся делать самому.
            Book::Pliny => vec![Lore::Technique(Prospecting), Lore::Technique(Mining), Lore::Technique(Smelting), Lore::Technique(Quenching), Lore::Technique(Casting)],
            Book::Psalter => vec![Lore::Read(Language::Greek)],
            Book::DionysiusThrax => vec![Lore::Grammar],
            Book::Nicomachus => vec![Lore::Reckon],
            Book::TheophrastusPlants => vec![Lore::Technique(Charcoal)],
        }
    }
}

// ---------------------------------------------------------------------------
// Мастера
// ---------------------------------------------------------------------------

/// Кто может учить. Плату, место и NPC даёт другая часть игры; здесь — программа.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Teacher {
    /// Городской врач (архиатр) и ксенон при базилике.
    Iatros,
    /// Ризотом, «корнерез»: травник-сборщик.
    Rhizotomos,
    /// Знахарка в готской деревне. Учит без книг.
    Wisewoman,
    /// Аптекарь (мирепс).
    Apothecary,
    /// Ссыльный философ-алхимик из Александрии.
    Alchemist,
    Dyer,
    Glassblower,
    Potter,
    /// Церковный писец: греческая грамота.
    Scribe,
    /// Учитель-грамматик: греческий и латынь.
    Grammarian,
    /// Лекарь у кочевников за стенами.
    SteppeHealer,
    /// Городской кузнец (χαλκεύς): ковка, сварка, закалка, литьё.
    Smith,
    /// Мастер-оружейник гарнизонной мастерской: клинки, кольчуги, доспехи.
    Armourer,
    /// Готский кузнец: сам плавит руду, варит пакетные клинки.
    GothSmith,
    /// Кочевой кузнец степняков: наконечники стрел, литьё бронзовых бляшек.
    SteppeSmith,
    /// Грамматист — школьный учитель в дидаскалионе: азбука, чтение, счёт, письмо. Дёшево, но в классе.
    Grammatist,
    /// Нотарий городской канцелярии: пишет договоры и завещания, считает подати.
    Notary,
    /// Книгописец (каллиграф): переписывает книги — лучше всех учит письму.
    Copyist,
    /// Философ: грамматика и арифметика по Никомаху, недёшево и не всякому.
    Philosopher,
    /// Сочинитель (логограф) на агоре: пишет письма и прошения неграмотным, учит слогу.
    Logograph,
    /// Старший рудокоп (десятник) на руднике: руда, штольня, обжиг.
    Miner,
    /// Готский углежог в горном лесу.
    CharcoalBurner,
}

/// Как учится игрок.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LessonMode {
    /// Платный урок: мастер сразу объясняет — быстрее всего.
    Paid,
    /// Подмастерье: грязная работа рядом с мастером. Знание приходит медленно
    /// (наблюдение), зато навык растёт быстрее всего.
    Apprentice,
}

impl LessonMode {
    /// Доля часа урока, которая идёт в знание.
    pub fn knowledge_rate(self) -> f32 {
        match self {
            LessonMode::Paid => 1.0,
            LessonMode::Apprentice => 0.35,
        }
    }

    /// Навык за час.
    pub fn skill_per_hour(self) -> f32 {
        match self {
            LessonMode::Paid => 0.6,
            LessonMode::Apprentice => 2.0,
        }
    }
}

impl Teacher {
    pub const ALL: [Teacher; 22] = [
        Teacher::Iatros,
        Teacher::Rhizotomos,
        Teacher::Wisewoman,
        Teacher::Apothecary,
        Teacher::Alchemist,
        Teacher::Dyer,
        Teacher::Glassblower,
        Teacher::Potter,
        Teacher::Scribe,
        Teacher::Grammarian,
        Teacher::SteppeHealer,
        Teacher::Smith,
        Teacher::Armourer,
        Teacher::GothSmith,
        Teacher::SteppeSmith,
        Teacher::Grammatist,
        Teacher::Notary,
        Teacher::Copyist,
        Teacher::Philosopher,
        Teacher::Logograph,
        Teacher::Miner,
        Teacher::CharcoalBurner,
    ];

    /// Наставник грамоты (окно школы, а не просто урок).
    pub fn teaches_letters(self) -> bool {
        self.curriculum().iter().any(|l| l.is_letters())
    }

    /// Как быстро идёт урок этого предмета у этого учителя (1 — как у частного наставника).
    /// В классе с двумя десятками мальчишек выходит медленнее, книгописец лучше всех ставит руку.
    pub fn pace(self, lore: Lore) -> f32 {
        match (self, lore) {
            (Teacher::Grammatist, _) => 0.75,
            (Teacher::Scribe, _) => 0.9,
            (Teacher::Copyist, Lore::Write(_)) => 1.4,
            (Teacher::Notary, Lore::Reckon | Lore::Write(_)) => 1.2,
            (Teacher::Philosopher, Lore::Grammar | Lore::Reckon) => 1.35,
            (Teacher::Logograph, Lore::Grammar) => 1.15,
            _ => 1.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Teacher::Iatros => "Ятр",
            Teacher::Rhizotomos => "Ризотом",
            Teacher::Wisewoman => "Знахарка",
            Teacher::Apothecary => "Аптекарь",
            Teacher::Alchemist => "Философ-алхимик",
            Teacher::Dyer => "Красильщик",
            Teacher::Glassblower => "Стеклодув",
            Teacher::Potter => "Гончар",
            Teacher::Scribe => "Церковный писец",
            Teacher::Grammarian => "Грамматик",
            Teacher::SteppeHealer => "Степной лекарь",
            Teacher::Smith => "Кузнец",
            Teacher::Armourer => "Оружейник",
            Teacher::GothSmith => "Готский кузнец",
            Teacher::SteppeSmith => "Кочевой кузнец",
            Teacher::Grammatist => "Грамматист",
            Teacher::Notary => "Нотарий",
            Teacher::Copyist => "Книгописец",
            Teacher::Philosopher => "Философ",
            Teacher::Logograph => "Сочинитель",
            Teacher::Miner => "Десятник рудокопов",
            Teacher::CharcoalBurner => "Углежог",
        }
    }

    /// Какой навык растёт у подмастерья.
    pub fn skill(self) -> Option<Skill> {
        match self {
            Teacher::Iatros => Some(Skill::Medicine),
            Teacher::Rhizotomos | Teacher::Wisewoman | Teacher::Apothecary | Teacher::SteppeHealer => Some(Skill::Herbalism),
            Teacher::Alchemist | Teacher::Dyer | Teacher::Glassblower | Teacher::Potter => Some(Skill::Alchemy),
            Teacher::Smith | Teacher::Armourer | Teacher::GothSmith | Teacher::SteppeSmith | Teacher::Miner | Teacher::CharcoalBurner => Some(Skill::Smithing),
            Teacher::Scribe | Teacher::Grammarian | Teacher::Grammatist | Teacher::Notary | Teacher::Copyist | Teacher::Philosopher | Teacher::Logograph => Some(Skill::Letters),
        }
    }

    /// Программа по порядку.
    pub fn curriculum(self) -> Vec<Lore> {
        use Substance::*;
        use Technique::*;
        let idp = |list: &[Substance]| list.iter().flat_map(|s| [Lore::Identify(*s), Lore::Properties(*s)]).collect::<Vec<_>>();
        let props = |list: &[Substance]| list.iter().map(|s| Lore::Properties(*s)).collect::<Vec<_>>();
        let t = Lore::Technique;
        let mut v = Vec::new();
        match self {
            // Грамота у всех начинается с азбуки: учитель не учит через голову.
            Teacher::Scribe => v.extend([Lore::Alphabet, Lore::Read(Language::Greek), Lore::Write(Language::Greek)]),
            Teacher::Grammatist => v.extend([Lore::Alphabet, Lore::Read(Language::Greek), Lore::Reckon, Lore::Write(Language::Greek)]),
            Teacher::Grammarian => v.extend([
                Lore::Alphabet,
                Lore::Read(Language::Greek),
                Lore::Write(Language::Greek),
                Lore::Grammar,
                Lore::Read(Language::Latin),
                Lore::Write(Language::Latin),
            ]),
            Teacher::Notary => v.extend([Lore::Alphabet, Lore::Read(Language::Greek), Lore::Write(Language::Greek), Lore::Reckon]),
            Teacher::Copyist => v.extend([Lore::Alphabet, Lore::Read(Language::Greek), Lore::Write(Language::Greek)]),
            Teacher::Philosopher => v.extend([Lore::Alphabet, Lore::Reckon, Lore::Read(Language::Greek), Lore::Write(Language::Greek), Lore::Grammar]),
            Teacher::Logograph => v.extend([Lore::Alphabet, Lore::Read(Language::Greek), Lore::Write(Language::Greek), Lore::Grammar]),
            Teacher::Rhizotomos => {
                v.push(t(Gathering));
                v.extend(
                    [Plantain, Yarrow, Sage, Thyme, StJohnsWort, Mallow, Mint, Wormwood, RoseHip, Juniper, Comfrey, Elecampane, Henbane, Poppy, Madder, Woad, OakGall]
                        .map(Lore::Identify),
                );
                v.push(t(Maceration));
            }
            Teacher::Wisewoman => {
                v.extend(idp(&[Plantain]));
                v.push(t(Poultice));
                v.extend(idp(&[Yarrow, StJohnsWort, Mallow, Comfrey, Wormwood, Henbane]));
                v.push(t(Maceration));
                v.push(t(Decoction));
            }
            Teacher::Iatros => {
                v.extend([t(WoundWashing), t(Poultice)]);
                v.extend(props(&[Plantain, Yarrow, Sage, Wine]));
                v.push(t(Decoction));
                v.push(t(Dosage));
                v.extend(props(&[Poppy, Henbane]));
                v.push(t(ArrowExtraction));
                v.push(t(Bonesetting));
                v.push(t(Salve));
                v.extend(props(&[Myrrh, Frankincense, Alum]));
                v.push(t(Oxymel));
                v.extend(props(&[Opium, Juniper, Cinnamon, Saffron, Pepper]));
                v.push(t(Theriac));
            }
            Teacher::Apothecary => {
                v.push(t(Decoction));
                v.extend(props(&[Sage, Thyme, RoseHip, Mint, Elecampane]));
                v.extend([t(Maceration), t(Salve)]);
                v.extend(props(&[OliveOil, MuttonFat, Myrrh, Frankincense, Mastic, Licorice, Rhubarb, Pepper, Saffron, Cinnamon]));
                v.extend([t(Oxymel), t(Dosage)]);
            }
            Teacher::Alchemist => {
                v.push(t(Fermentation));
                v.push(t(Distillation));
                v.extend(props(&[PineResin, Sulfur, Quicklime]));
                v.push(t(Sublimation));
                v.extend(props(&[Cinnabar, Lead, CopperScrap]));
                v.push(t(Calcination));
                v.extend(props(&[Naphtha]));
                v.push(t(Incendiary));
            }
            Teacher::Dyer => {
                v.push(t(Mordanting));
                v.extend(props(&[Alum]));
                v.extend(idp(&[Madder, Woad]));
                v.extend(props(&[WoodAsh, Natron]));
                v.extend([t(Soap), t(VatDyeing)]);
                v.extend(idp(&[OakGall]));
                v.extend(props(&[CopperVitriol, GumArabic]));
                v.push(t(Ink));
            }
            Teacher::Glassblower => v.extend(props(&[Natron]).into_iter().chain([t(Glassblowing)])),
            Teacher::Potter => v.push(t(Pottery)),
            Teacher::SteppeHealer => {
                v.extend(props(&[Licorice, Rhubarb, MuttonFat]));
                v.extend(idp(&[Wormwood, Thyme]));
                v.push(t(Salve));
                // Степняки живут луком: стрелу из раны вынимают лучше городских врачей.
                v.push(t(ArrowExtraction));
            }
            Teacher::Smith => v.extend([t(Forging), t(ForgeWelding), t(Quenching), t(Tempering), t(Casting), t(Carburizing)]),
            Teacher::Armourer => v.extend([t(Forging), t(Quenching), t(Tempering), t(Armoury), t(MailMaking)]),
            Teacher::GothSmith => v.extend([t(Forging), t(Smelting), t(ForgeWelding), t(Quenching)]),
            Teacher::SteppeSmith => v.extend([t(Forging), t(Casting), t(LostWax), t(Quenching)]),
            Teacher::Miner => v.extend([t(Prospecting), t(Roasting), t(Mining)]),
            Teacher::CharcoalBurner => v.extend([t(Charcoal), t(Roasting)]),
        }
        v
    }
}

// ---------------------------------------------------------------------------
// Знания персонажа
// ---------------------------------------------------------------------------

/// Чем закончилось чтение или урок.
#[derive(Debug, Clone, PartialEq)]
pub enum CannotRead {
    Illiterate(Language),
    NeedsLore(Lore),
}

impl CannotRead {
    pub fn message(&self) -> String {
        match self {
            CannotRead::Illiterate(Language::Greek) => "Для вас это непонятные закорючки. Сначала нужно выучить грамоту: в школе-дидаскалионе, у писца или книгописца.".into(),
            CannotRead::Illiterate(Language::Latin) => "Это латынь. Ей учит только грамматик.".into(),
            CannotRead::NeedsLore(l) => format!("Ничего не понятно. Сначала нужно знать: {}", l.name().to_lowercase()),
        }
    }
}

/// Всё, что знает и умеет один персонаж.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Knowledge {
    pub known: BTreeSet<Lore>,
    /// Навыки 0–100 по [`Skill::index`] (без прибавок от умений).
    pub skills: [f32; SKILL_COUNT],
    /// Незаконченные уроки: доля 0..1.
    pub lessons: BTreeMap<Lore, f32>,
    /// Сколько прочитано: доля 0..1.
    pub read: BTreeMap<Book, f32>,
    /// Сколько переписано (начатые списки книг): доля 0..1.
    #[serde(default)]
    pub copied: BTreeMap<Book, f32>,
}

impl Knowledge {
    pub fn knows(&self, lore: Lore) -> bool {
        self.known.contains(&lore)
    }

    /// Выучить. true — знание новое.
    pub fn learn(&mut self, lore: Lore) -> bool {
        self.lessons.remove(&lore);
        self.known.insert(lore)
    }

    /// Навык в деле: наработанное плюс прибавка от умений ([`crate::skill_tree`]), не больше 100.
    pub fn skill(&self, skill: Skill) -> f32 {
        (self.base_skill(skill) + crate::skill_tree::skill_bonus(self, skill)).min(100.0)
    }

    /// Наработанный навык без умений.
    pub fn base_skill(&self, skill: Skill) -> f32 {
        self.skills[skill.index()]
    }

    /// Навык растёт всё медленнее к сотне; заодно понемногу растут связанные навыки.
    pub fn practice(&mut self, skill: Skill, amount: f32) {
        self.practice_only(skill, amount);
        for (other, k) in skill.spillover() {
            self.practice_only(*other, amount * k);
        }
    }

    /// Только этот навык, без связанных.
    pub fn practice_only(&mut self, skill: Skill, amount: f32) {
        let s = &mut self.skills[skill.index()];
        *s = (*s + amount * (1.0 - *s / 110.0)).clamp(0.0, 100.0);
    }

    pub fn prerequisites_met(&self, lore: Lore) -> bool {
        lore.requires().into_iter().all(|r| self.knows(r))
    }

    /// Можно ли читать текст.
    pub fn can_read(&self, text: Book) -> Result<(), CannotRead> {
        let lang = text.info().language;
        // Псалтирь осилит и тот, кто знает одни буквы: по ней и учатся читать.
        if text == Book::Psalter && self.knows(Lore::Alphabet) {
            return Ok(());
        }
        if !self.knows(Lore::Read(lang)) {
            return Err(CannotRead::Illiterate(lang));
        }
        if let Some(missing) = text.requires().into_iter().find(|l| !self.knows(*l)) {
            return Err(CannotRead::NeedsLore(missing));
        }
        Ok(())
    }

    /// Следующее, чему может научить мастер (None — учить нечему).
    pub fn next_lesson(&self, teacher: Teacher) -> Option<Lore> {
        teacher.curriculum().into_iter().find(|l| !self.knows(*l) && self.prerequisites_met(*l))
    }

    /// Час урока. Возвращает выученное за этот час (обычно пусто или одно знание).
    pub fn study(&mut self, teacher: Teacher, mode: LessonMode, hours: f32) -> Vec<Lore> {
        self.study_subject(teacher, None, mode, hours)
    }

    /// Можно ли сейчас учить этот предмет у этого учителя.
    pub fn can_study(&self, teacher: Teacher, lore: Lore) -> bool {
        !self.knows(lore) && self.prerequisites_met(lore) && teacher.curriculum().contains(&lore)
    }

    /// Урок выбранного предмета (`subject`; None — что учитель даст по порядку). Остаток часа —
    /// дальше по программе.
    pub fn study_subject(&mut self, teacher: Teacher, subject: Option<Lore>, mode: LessonMode, hours: f32) -> Vec<Lore> {
        if let Some(skill) = teacher.skill() {
            self.practice(skill, mode.skill_per_hour() * hours);
        }
        // Книжный человек схватывает объяснения быстрее.
        let mut budget = hours * mode.knowledge_rate() * crate::skill_tree::Mods::of(self).lessons;
        let mut learned = Vec::new();
        while budget > 1e-4 {
            let chosen = subject.filter(|l| self.can_study(teacher, *l));
            let Some(lore) = chosen.or_else(|| self.next_lesson(teacher)) else { break };
            let need = lore.lesson_hours() / teacher.pace(lore);
            let done = self.lessons.get(&lore).copied().unwrap_or(0.0);
            let left = (1.0 - done) * need;
            if budget >= left {
                budget -= left;
                self.learn(lore);
                learned.push(lore);
            } else {
                self.lessons.insert(lore, done + budget / need);
                budget = 0.0;
            }
        }
        learned
    }

    /// Опознано ли вещество (у покупных и нелесных имя известно всегда).
    pub fn identifies(&self, s: Substance) -> bool {
        !s.is_wild() || self.knows(Lore::Identify(s))
    }

    /// Как предмет называется для этого персонажа.
    pub fn item_name(&self, base: ItemBaseId) -> String {
        match Substance::from_item(base) {
            Some(s) if !self.identifies(s) => match crate::brewing::canonical(base.base().name).1 {
                crate::brewing::Form::Dried => format!("{} (сушёное)", s.info().unknown),
                crate::brewing::Form::Ground => format!("{} (толчёное)", s.info().unknown),
                crate::brewing::Form::Raw => s.info().unknown.to_string(),
            },
            _ => base.base().name.to_string(),
        }
    }

    pub fn display_name(&self, item: &Item) -> String {
        let name = self.item_name(item.base);
        if item.count > 1 { format!("{name} ×{}", item.count) } else { name }
    }

    /// Строки описания для подсказки.
    pub fn describe(&self, base: ItemBaseId) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(s) = Substance::from_item(base) {
            let info = s.info();
            if self.identifies(s) {
                if s.is_wild() {
                    lines.push(format!("У Диоскорида: {}", info.greek));
                }
            } else {
                lines.push(info.look.to_string());
                lines.push("Вы не знаете, что это за растение.".to_string());
            }
            if self.knows(Lore::Properties(s)) {
                lines.push(info.uses.to_string());
            } else if self.identifies(s) {
                lines.push("Свойства вам неизвестны.".to_string());
            }
        }
        if let Some(t) = Book::from_item(base) {
            let info = t.info();
            lines.push(format!("{}, язык: {}", capitalize(info.form), info.language.name()));
            match self.can_read(t) {
                Ok(()) => {
                    lines.push(info.about.to_string());
                    let done = self.read.get(&t).copied().unwrap_or(0.0);
                    if done >= 1.0 {
                        lines.push("Прочитано.".to_string());
                    } else {
                        if done > 0.0 {
                            lines.push(format!("Прочитано {:.0}%", done * 100.0));
                        }
                        lines.push(format!("Чтение около {} · F или ПКМ", fmt_secs(info.read_secs * (1.0 - done))));
                    }
                    if self.knows(Lore::Write(info.language)) && t != Book::HerbalFragment {
                        let copied = self.copied.get(&t).copied().unwrap_or(0.0);
                        let begun = if copied > 0.0 { format!(" (переписано {:.0}%)", copied * 100.0) } else { String::new() };
                        lines.push(format!("C — переписать{begun}: калам, чернила, тетрадей пергамента: {}", t.quires()));
                    }
                }
                Err(e) => lines.push(e.message()),
            }
        }
        lines
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn fmt_secs(secs: f32) -> String {
    let s = secs.round() as u32;
    if s >= 60 { format!("{} мин {} с", s / 60, s % 60) } else { format!("{s} с") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_substance_and_text_has_an_item() {
        for s in Substance::ALL {
            assert!(s.item().is_some(), "нет предмета для {:?}", s);
            assert_eq!(Substance::from_item(s.item().unwrap()), Some(s));
        }
        for t in Book::ALL {
            assert!(ItemBaseId::find(t.info().item).is_some(), "нет предмета для {:?}", t);
        }
    }

    #[test]
    fn substance_list_is_complete() {
        let set: BTreeSet<Substance> = Substance::ALL.into_iter().collect();
        assert_eq!(set.len(), Substance::ALL.len());
        let t: BTreeSet<Technique> = Technique::ALL.into_iter().collect();
        assert_eq!(t.len(), Technique::ALL.len());
    }

    #[test]
    fn herb_patch_ids_map_to_herbs() {
        for id in ["salvia", "thymus", "hypericum", "hyoscyamus", "juniperus", "artemisia", "achillea", "plantago", "rosa", "malva", "papaver", "mentha"] {
            assert!(Substance::from_latin(id).is_some(), "{id}");
        }
    }

    #[test]
    fn illiterate_cannot_read() {
        let mut k = Knowledge::default();
        assert_eq!(k.can_read(Book::Dioscorides), Err(CannotRead::Illiterate(Language::Greek)));
        k.learn(Lore::Read(Language::Greek));
        assert!(k.can_read(Book::Dioscorides).is_ok());
        assert_eq!(k.can_read(Book::Celsus), Err(CannotRead::Illiterate(Language::Latin)));
        assert!(matches!(k.can_read(Book::GalenAntidotes), Err(CannotRead::NeedsLore(_))));
    }

    #[test]
    fn unknown_herb_has_no_name() {
        let mut k = Knowledge::default();
        let sage = ItemBaseId::by_name("Шалфей");
        assert_eq!(k.item_name(sage), "Неизвестная трава");
        // Покупное называет продавец.
        assert_eq!(k.item_name(ItemBaseId::by_name("Мирра")), "Мирра");
        k.learn(Lore::Identify(Substance::Sage));
        assert_eq!(k.item_name(sage), "Шалфей");
    }

    #[test]
    fn paid_lessons_are_fastest_apprentice_trains_skill() {
        let mut paid = Knowledge::default();
        let mut apprentice = Knowledge::default();
        for _ in 0..4 {
            paid.study(Teacher::Iatros, LessonMode::Paid, 1.0);
            apprentice.study(Teacher::Iatros, LessonMode::Apprentice, 1.0);
        }
        assert!(paid.known.len() > apprentice.known.len());
        assert!(apprentice.skill(Skill::Medicine) > paid.skill(Skill::Medicine));
    }

    #[test]
    fn teachers_respect_prerequisites() {
        for teacher in Teacher::ALL {
            let mut k = Knowledge::default();
            // Хватит времени, чтобы пройти всю программу, если она проходима.
            for _ in 0..200 {
                k.study(teacher, LessonMode::Paid, 1.0);
            }
            for lore in teacher.curriculum() {
                assert!(k.knows(lore), "{:?} не научил {:?}", teacher, lore);
            }
            assert_eq!(k.next_lesson(teacher), None);
        }
        // Териаку не учат того, кто не варил отваров.
        let k = Knowledge::default();
        assert!(!k.prerequisites_met(Lore::Technique(Technique::Theriac)));
    }

    #[test]
    fn grammar_takes_several_lessons() {
        let mut k = Knowledge::default();
        let learned = k.study(Teacher::Scribe, LessonMode::Paid, 1.0);
        assert!(learned.is_empty());
        // Сначала азбука.
        assert!(k.lessons.get(&Lore::Alphabet).is_some_and(|p| *p > 0.0 && *p < 1.0));
        for _ in 0..9 {
            k.study(Teacher::Scribe, LessonMode::Paid, 1.0);
        }
        assert!(k.knows(Lore::Alphabet));
        assert!(k.knows(Lore::Read(Language::Greek)));
    }

    #[test]
    fn school_ladder_goes_step_by_step() {
        let k = Knowledge::default();
        // Не зная букв, ни читать, ни считать не начнёшь.
        assert!(!k.can_study(Teacher::Grammatist, Lore::Read(Language::Greek)));
        assert!(!k.can_study(Teacher::Grammatist, Lore::Reckon));
        assert!(k.can_study(Teacher::Grammatist, Lore::Alphabet));
        // Латыни учит только грамматик и только знающего грамматику.
        for t in Teacher::ALL {
            if t != Teacher::Grammarian {
                assert!(!t.curriculum().contains(&Lore::Read(Language::Latin)), "{t:?}");
            }
        }
        assert_eq!(Lore::Read(Language::Latin).requires(), vec![Lore::Grammar]);
        // Выбранный предмет идёт первым: счёт раньше чтения.
        let mut k = Knowledge::default();
        k.learn(Lore::Alphabet);
        k.study_subject(Teacher::Grammatist, Some(Lore::Reckon), LessonMode::Paid, 1.0);
        assert!(k.lessons.contains_key(&Lore::Reckon));
        assert!(!k.lessons.contains_key(&Lore::Read(Language::Greek)));
        // В классе медленнее, чем у книгописца с глазу на глаз.
        let mut school = Knowledge::default();
        let mut copyist = Knowledge::default();
        for k in [&mut school, &mut copyist] {
            k.learn(Lore::Alphabet);
            k.learn(Lore::Read(Language::Greek));
        }
        school.study_subject(Teacher::Grammatist, Some(Lore::Write(Language::Greek)), LessonMode::Paid, 1.0);
        copyist.study_subject(Teacher::Copyist, Some(Lore::Write(Language::Greek)), LessonMode::Paid, 1.0);
        let p = |k: &Knowledge| k.lessons.get(&Lore::Write(Language::Greek)).copied().unwrap_or(0.0);
        assert!(p(&copyist) > p(&school) * 1.5);
    }

    #[test]
    fn psalter_teaches_reading_to_who_knows_letters() {
        let mut k = Knowledge::default();
        assert!(k.can_read(Book::Psalter).is_err());
        k.learn(Lore::Alphabet);
        assert!(k.can_read(Book::Psalter).is_ok());
        assert!(k.can_read(Book::Dioscorides).is_err());
        assert_eq!(Book::Psalter.teaches(), vec![Lore::Read(Language::Greek)]);
        // Грамматику по Дионисию поймёт только пишущий.
        k.learn(Lore::Read(Language::Greek));
        assert!(matches!(k.can_read(Book::DionysiusThrax), Err(CannotRead::NeedsLore(Lore::Write(Language::Greek)))));
        assert!(Book::Dioscorides.quires() >= 2);
    }

    #[test]
    fn skill_is_capped() {
        let mut k = Knowledge::default();
        for _ in 0..10_000 {
            k.practice(Skill::Alchemy, 5.0);
        }
        assert!(k.skill(Skill::Alchemy) <= 100.0);
    }
}
