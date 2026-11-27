//! Двор комита Херсона: правитель, его семья и приближённые — кто они, где живут, чем заняты
//! в течение дня и что говорят.
//!
//! Около 510 г. Херсоном от имени императора Анастасия правит **комит** (κόμης) — начальник
//! гарнизона, таможни и суда по делам империи; город же живёт по своему обычаю: курия с
//! протевонами, «отец города», епископ. При комите — канцелярия (хартуларий), казна
//! (сакелларий), трибун гарнизона, домовый пресвитер, препозит (дворецкий), палатная стража.
//! Все лица вымышлены, обстановка — по источникам: при Зеноне (488 г.) стены чинили на
//! таможенные сборы, Анастасий в 498 г. отменил хрисаргир и ввёл крупный медный фоллис
//! с буквой «М» (40 нуммиев), с персами в 506 г. заключён мир, в степи кочуют утигуры.
//!
//! Здесь только данные и чистые функции (их проверяют тесты): постройку ведёт
//! `level/city/palace.rs`, распорядок — `daily_life.rs`, окно разговора — `court_ui.rs`.

use crate::daily_life::{Act, Dest};
use crate::level::palace::{self, Seat};
use crate::level::Role;
use bevy::prelude::*;
use std::collections::HashMap;

/// Кто из двора.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Courtier {
    /// Комит Флавий Евагрий — правитель Херсона.
    Comes,
    /// Госпожа Евфимия, супруга комита.
    Wife,
    /// Феодор, сын комита (лет девяти).
    Son,
    /// Хартуларий Стефан — секретарь, ведёт переписку и прошения.
    Chartularius,
    /// Сакелларий Никифор — казначей.
    Sacellarius,
    /// Трибун Фравитта — начальник гарнизона и палатной стражи.
    Tribune,
    /// Пресвитер Евсевий — священник домовой церкви.
    Presbyter,
    /// Препозит Мина — дворецкий, распоряжается домом и пускает к комиту.
    Praepositus,
    /// Кухарка Агафья.
    Cook,
    /// Палатные стражи у пропилея (0, 1 — днём, 2, 3 — ночью).
    Guard(u8),
    /// Протевон курии Аристон — первый из городских советников, приходит с делами города.
    Protevon,
    /// Просители в дни приёма: вдова рыбака и винодел с клера.
    Petitioner(u8),
}

impl Courtier {
    pub const ALL: [Courtier; 16] = [
        Courtier::Comes,
        Courtier::Wife,
        Courtier::Son,
        Courtier::Chartularius,
        Courtier::Sacellarius,
        Courtier::Tribune,
        Courtier::Presbyter,
        Courtier::Praepositus,
        Courtier::Cook,
        Courtier::Guard(0),
        Courtier::Guard(1),
        Courtier::Guard(2),
        Courtier::Guard(3),
        Courtier::Protevon,
        Courtier::Petitioner(0),
        Courtier::Petitioner(1),
    ];

    /// Имя.
    pub fn name(self) -> &'static str {
        match self {
            Courtier::Comes => "Флавий Евагрий",
            Courtier::Wife => "Евфимия",
            Courtier::Son => "Феодор",
            Courtier::Chartularius => "Стефан",
            Courtier::Sacellarius => "Никифор",
            Courtier::Tribune => "Фравитта",
            Courtier::Presbyter => "Евсевий",
            Courtier::Praepositus => "Мина",
            Courtier::Cook => "Агафья",
            Courtier::Guard(k) => ["Зинон", "Маврикий", "Гайна", "Савва"][k as usize % 4],
            Courtier::Protevon => "Аристон",
            Courtier::Petitioner(k) => ["Анна", "Ксенофонт"][k as usize % 2],
        }
    }

    /// Должность (как к нему обращаются).
    pub fn office(self) -> &'static str {
        match self {
            Courtier::Comes => "Комит Херсона",
            Courtier::Wife => "Супруга комита",
            Courtier::Son => "Сын комита",
            Courtier::Chartularius => "Хартуларий",
            Courtier::Sacellarius => "Сакелларий",
            Courtier::Tribune => "Трибун гарнизона",
            Courtier::Presbyter => "Пресвитер претории",
            Courtier::Praepositus => "Препозит претории",
            Courtier::Cook => "Кухарка претории",
            Courtier::Guard(_) => "Палатный страж",
            Courtier::Protevon => "Протевон курии",
            Courtier::Petitioner(0) => "Вдова рыбака",
            Courtier::Petitioner(_) => "Винодел с клера",
        }
    }

    /// Подпись над человеком: «Комит Флавий Евагрий», «Хартуларий Стефан».
    pub fn label(self) -> String {
        match self {
            Courtier::Comes => "Комит Флавий Евагрий".into(),
            Courtier::Wife => "Госпожа Евфимия".into(),
            Courtier::Son => "Феодор, сын комита".into(),
            Courtier::Cook => "Кухарка Агафья".into(),
            Courtier::Guard(_) => format!("Палатин {}", self.name()),
            Courtier::Petitioner(_) => format!("{}, {}", self.name(), self.office().to_lowercase()),
            _ => format!("{} {}", self.office().split(' ').next().unwrap_or(""), self.name()),
        }
    }

    /// Роль жителя (фракция, добыча, поведение в драке).
    pub fn role(self) -> Role {
        palace::role_of(self)
    }
}

// ---------------------------------------------------------------------------------------------
// Кто из жителей уровня — из двора
// ---------------------------------------------------------------------------------------------

/// Из двора ли житель, стоящий дома в точке `home` (по раскладке претории).
pub fn of_home(home: Vec3) -> Option<Courtier> {
    palace::court_posts().into_iter().find(|(_, p, _)| p.distance(home) < 0.05).map(|(c, _, _)| c)
}

fn table() -> &'static (HashMap<usize, Courtier>, HashMap<Courtier, usize>) {
    static T: std::sync::OnceLock<(HashMap<usize, Courtier>, HashMap<Courtier, usize>)> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut by_i = HashMap::new();
        let mut by_c = HashMap::new();
        if crate::level::active() == crate::level::LevelKind::Chersonesus {
            for (i, r) in crate::level::layout().residents.iter().enumerate() {
                if let Some(c) = of_home(r.pos) {
                    by_i.insert(i, c);
                    by_c.insert(c, i);
                }
            }
        }
        (by_i, by_c)
    })
}

/// Кто из двора этот житель (номер в списке уровня).
pub fn of_resident(i: usize) -> Option<Courtier> {
    table().0.get(&i).copied()
}

/// Номер жителя — этого человека двора.
pub fn resident_of(c: Courtier) -> Option<usize> {
    table().1.get(&c).copied()
}

// ---------------------------------------------------------------------------------------------
// Распорядок
// ---------------------------------------------------------------------------------------------

/// Куда идти по распорядку.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Where {
    /// Своё место (покой, келья).
    Home,
    /// Постель (если её нет — уходит в дом, не виден).
    Bed,
    /// Место во дворце.
    Seat(Seat),
    /// Место во дворце, лицом к собеседнику из двора.
    Talk(Seat, Courtier),
    /// Место в городе (базилика, агора, плац…).
    Dest(Dest),
}

/// Шаг распорядка: с часа `at` быть там-то и делать то-то.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub at: f32,
    pub place: Where,
    pub act: Act,
}

fn s(at: f32, place: Where, act: Act) -> Step {
    Step { at, place, act }
}

/// День приёма у комита: всякий, кроме воскресенья (каждый седьмой день — литургия в базилике).
pub fn audience_day(day: u32) -> bool {
    day % 7 != 6
}

/// Часы приёма в зале.
pub const AUDIENCE: (f32, f32) = (9.0, 12.0);

/// Распорядок на сутки. `rain(a, b)` — идёт ли дождь между часами a и b.
pub fn day_plan(c: Courtier, day: u32, rain: impl Fn(f32, f32) -> bool) -> Vec<Step> {
    use Act::*;
    use Where::{Bed, Dest as D, Home, Seat as At, Talk as With};
    let audience = audience_day(day);
    let odd = day % 2 == 1;
    let wet_evening = rain(16.5, 18.0);
    let mut v = vec![s(0.0, Bed, Sleep)];
    match c {
        Courtier::Comes => {
            v.push(s(5.8, Home, Idle));
            v.push(s(6.2, At(Seat::ChapelPray(0)), Pray));
            v.push(s(7.0, With(Seat::PorticoNorthA, Courtier::Chartularius), Talk));
            v.push(s(8.0, With(Seat::TreasuryTalkA, Courtier::Sacellarius), Talk));
            if audience {
                v.push(s(AUDIENCE.0, At(Seat::Throne), Sit));
            } else {
                // Воскресенье: литургия у епископа.
                v.push(s(8.7, D(Dest::Basilica), Pray));
                v.push(s(11.0, With(Seat::GardenA, Courtier::Wife), Talk));
            }
            v.push(s(12.2, At(Seat::Triclinium(0)), Dine));
            v.push(s(13.4, Home, Idle));
            if !rain(15.0, 16.5) {
                v.push(s(15.0, D(Dest::Garrison), Guard));
            }
            if !wet_evening {
                v.push(s(16.6, With(Seat::GardenA, Courtier::Wife), Talk));
            }
            if odd && !rain(18.0, 19.3) {
                v.push(s(18.0, D(Dest::Basilica), Pray));
            } else {
                v.push(s(18.0, Home, Idle));
            }
            v.push(s(19.5, At(Seat::Triclinium(0)), Dine));
            v.push(s(21.4, Bed, Sleep));
        }
        Courtier::Wife => {
            v.push(s(6.0, Home, Idle));
            v.push(s(6.2, At(Seat::ChapelPray(1)), Pray));
            v.push(s(7.4, At(Seat::Loom), Weave));
            if !rain(10.0, 11.5) {
                v.push(s(10.0, At(Seat::GardenB), Idle));
            }
            v.push(s(11.5, At(Seat::Loom), Weave));
            if !audience {
                v.retain(|x| x.at < 8.6);
                v.push(s(8.7, D(Dest::Basilica), Pray));
                v.push(s(11.0, With(Seat::GardenB, Courtier::Comes), Talk));
            }
            v.push(s(12.2, At(Seat::Triclinium(1)), Dine));
            v.push(s(13.4, At(Seat::Loom), Weave));
            if !wet_evening {
                v.push(s(16.6, With(Seat::GardenB, Courtier::Comes), Talk));
            }
            v.push(s(18.0, if odd && !rain(18.0, 19.3) { D(Dest::Basilica) } else { Home }, if odd { Pray } else { Idle }));
            v.push(s(19.5, At(Seat::Triclinium(1)), Dine));
            v.push(s(21.2, Bed, Sleep));
        }
        Courtier::Son => {
            v.push(s(6.8, Home, Idle));
            v.push(s(7.0, At(Seat::ChapelPray(3)), Pray));
            v.push(s(7.6, Home, Idle));
            if !rain(8.5, 12.0) {
                v.push(s(8.5, At(Seat::GardenPlay), Idle));
            }
            v.push(s(12.2, At(Seat::Triclinium(3)), Dine));
            v.push(s(13.2, Home, Idle));
            if !rain(14.0, 17.5) {
                v.push(s(14.0, At(Seat::GardenPlay), Idle));
            }
            v.push(s(17.5, Home, Idle));
            v.push(s(19.5, At(Seat::Triclinium(4)), Dine));
            v.push(s(20.4, Bed, Sleep));
        }
        Courtier::Chartularius => {
            v.push(s(5.9, Home, Idle));
            v.push(s(6.2, At(Seat::ChapelPray(2)), Pray));
            v.push(s(7.0, With(Seat::PorticoNorthB, Courtier::Comes), Talk));
            v.push(s(8.0, At(Seat::ScriptoriumDesk), Write));
            if audience {
                v.push(s(AUDIENCE.0 - 0.1, At(Seat::NotaryDesk), Write));
            }
            v.push(s(12.2, Home, Eat));
            v.push(s(13.0, At(Seat::ScriptoriumDesk), Write));
            v.push(s(16.2, At(Seat::ArchiveShelf), Fetch));
            v.push(s(17.0, At(Seat::ScriptoriumDesk), Write));
            v.push(s(18.2, With(Seat::PorticoSouthA, Courtier::Sacellarius), Talk));
            v.push(s(19.5, At(Seat::Triclinium(2)), Dine));
            v.push(s(21.0, At(Seat::ScriptoriumDesk), Write));
            v.push(s(22.3, Bed, Sleep));
        }
        Courtier::Sacellarius => {
            v.push(s(6.3, Home, Idle));
            v.push(s(6.8, At(Seat::TreasuryDesk), Write));
            v.push(s(8.0, With(Seat::TreasuryTalkB, Courtier::Comes), Talk));
            if audience {
                v.push(s(AUDIENCE.0, At(Seat::HallTreasurer), Idle));
            }
            // Сверить коммеркий на таможне (через день).
            if odd && !rain(10.5, 12.0) {
                v.push(s(10.5, D(Dest::Quay), Browse));
            } else {
                v.push(s(10.5, At(Seat::TreasuryDesk), Write));
            }
            v.push(s(12.4, Home, Eat));
            v.push(s(13.3, At(Seat::TreasuryDesk), Write));
            v.push(s(18.2, With(Seat::PorticoSouthB, Courtier::Chartularius), Talk));
            v.push(s(19.4, Home, Eat));
            v.push(s(21.5, Bed, Sleep));
        }
        Courtier::Tribune => {
            v.push(s(5.3, Home, Idle));
            v.push(s(5.7, D(Dest::Garrison), Guard));
            if audience {
                v.push(s(AUDIENCE.0 - 0.15, At(Seat::HallTribune), Guard));
            }
            v.push(s(12.0, Home, Eat));
            v.push(s(13.0, D(Dest::MainGate), Guard));
            v.push(s(14.8, D(Dest::Garrison), Guard));
            v.push(s(17.0, At(Seat::TribuneTable), Write));
            if !rain(18.3, 19.6) && odd {
                v.push(s(18.3, D(Dest::Tavern), Drink));
            }
            v.push(s(19.7, Home, Eat));
            v.push(s(21.8, Bed, Sleep));
        }
        Courtier::Presbyter => {
            v.push(s(4.6, At(Seat::ChapelAltar), Preach)); // утреня
            v.push(s(5.8, At(Seat::ChapelAltar), Preach)); // молитва с домом комита
            v.push(s(7.4, At(Seat::Lectern), Write));
            if !rain(9.0, 11.0) {
                v.push(s(9.0, D(Dest::Basilica), Pray));
            }
            v.push(s(11.3, At(Seat::ChapelAltar), Pray));
            v.push(s(12.5, Home, Eat));
            v.push(s(13.5, At(Seat::Lectern), Write));
            v.push(s(17.0, At(Seat::ChapelAltar), Preach)); // вечерня
            v.push(s(19.5, At(Seat::Triclinium(2)), Dine));
            if odd {
                // Через день хартуларий ужинает у себя — место пресвитеру.
            }
            v.push(s(21.0, Bed, Sleep));
        }
        Courtier::Praepositus => {
            v.push(s(5.0, Home, Idle));
            v.push(s(5.3, With(Seat::KitchenTalkA, Courtier::Cook), Talk));
            v.push(s(6.3, At(Seat::PorticoEast), Idle));
            if !rain(7.3, 8.8) {
                v.push(s(7.3, D(Dest::Agora), Browse));
            }
            if audience {
                v.push(s(AUDIENCE.0 - 0.2, At(Seat::HallUsher), Idle));
            }
            v.push(s(12.1, At(Seat::TricliniumServe), Arrange));
            v.push(s(13.3, Home, Eat));
            v.push(s(14.2, Home, Idle));
            v.push(s(16.0, With(Seat::KitchenTalkA, Courtier::Cook), Talk));
            v.push(s(17.0, At(Seat::PorticoEast), Idle));
            v.push(s(19.4, At(Seat::TricliniumServe), Arrange));
            v.push(s(21.2, Home, Idle));
            v.push(s(22.0, Bed, Sleep));
        }
        Courtier::Cook => {
            v.push(s(4.8, At(Seat::KitchenHearth), Stir));
            v.push(s(5.3, With(Seat::KitchenTalkB, Courtier::Praepositus), Talk));
            v.push(s(6.3, At(Seat::KitchenHearth), Stir));
            v.push(s(12.1, Home, Work));
            v.push(s(13.5, Home, Eat));
            v.push(s(14.2, At(Seat::KitchenHearth), Stir));
            v.push(s(16.0, With(Seat::KitchenTalkB, Courtier::Praepositus), Talk));
            v.push(s(17.0, At(Seat::KitchenHearth), Stir));
            v.push(s(20.3, Home, Work));
            v.push(s(21.0, Bed, Sleep));
        }
        Courtier::Guard(k) => {
            // Палатины несут службу сменами: двое — с рассвета до вечера, двое — ночью.
            let post = At(Seat::Propylon(k % 2));
            // Сменившись, уходят в караульню при пропилее (их не видно до следующей смены).
            let off = Home;
            if k < 2 {
                v[0] = s(0.0, off, Away);
                v.push(s(6.0, post, Guard));
                v.push(s(20.0, off, Away));
            } else {
                v[0] = s(0.0, post, Guard);
                v.push(s(6.0, off, Away));
                v.push(s(20.0, post, Guard));
            }
        }
        Courtier::Protevon => {
            // Гости дворца: приходят к приёму, в остальное время их здесь нет.
            v[0] = s(0.0, Home, Away);
            if audience {
                v.push(s(8.4, At(Seat::PorticoEast), Idle));
                v.push(s(AUDIENCE.0 + 0.3, At(Seat::HallPetition), Talk));
                if !rain(11.0, 13.0) {
                    v.push(s(11.0, D(Dest::Agora), Talk));
                }
                v.push(s(13.0, Home, Away));
            }
        }
        Courtier::Petitioner(k) => {
            v[0] = s(0.0, Home, Away);
            if audience && (day + k as u32) % 3 != 0 {
                v.push(s(AUDIENCE.0 + 0.1 + k as f32 * 0.3, At(Seat::HallPetitioner(k)), Idle));
                v.push(s(AUDIENCE.1 - 0.2, Home, Away));
            }
        }
    }
    v.sort_by(|a, b| a.at.total_cmp(&b.at));
    v.dedup_by(|b, a| (b.at - a.at).abs() < 0.05 && {
        *a = *b;
        true
    });
    v
}

/// Спит ли во дворце (стража спит в казарме, просители и гости — у себя в городе).
pub fn sleeps_here(c: Courtier) -> bool {
    !matches!(c, Courtier::Guard(_) | Courtier::Protevon | Courtier::Petitioner(_))
}

/// Место (и куда смотреть) для шага распорядка во дворце.
pub fn seat_of(w: Where) -> Option<(Vec3, f32)> {
    match w {
        Where::Seat(s) | Where::Talk(s, _) => Some(palace::seat(s)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Разговоры
// ---------------------------------------------------------------------------------------------

/// Что игрок узнал в разговорах (открывает новые вопросы).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flag {
    /// Подал прошение комиту.
    Petition,
    /// Хартуларий проболтался о письме из столицы.
    Letter,
    /// Спросил трибуна о службе.
    Service,
    /// Узнал от пресвитера о святом Клименте.
    Clement,
    /// Госпожа просила отнести хлеб в ксенон.
    Alms,
    /// Протевон пожаловался на водопровод.
    Aqueduct,
}

/// Когда вопрос можно задать.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    /// Узнав это.
    Knows(Flag),
    /// Пока не узнал.
    Unknown(Flag),
    /// Только на приёме в зале (комит на троне).
    Audience,
    /// Не на приёме.
    Private,
    /// После стольких разговоров.
    Familiar(u32),
}

/// Вопрос и ответы на него (по очереди: спросишь ещё раз — скажет иначе).
pub struct Topic {
    pub ask: &'static str,
    pub say: &'static [&'static str],
    pub when: When,
    pub learn: Option<Flag>,
}

const fn t(ask: &'static str, say: &'static [&'static str]) -> Topic {
    Topic { ask, say, when: When::Always, learn: None }
}

const fn tw(ask: &'static str, when: When, learn: Option<Flag>, say: &'static [&'static str]) -> Topic {
    Topic { ask, say, when, learn }
}

/// Обстоятельства разговора.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub hour: f32,
    pub day: u32,
    /// Сколько раз уже говорили (до этого).
    pub met: u32,
    pub act: Act,
    pub rain: bool,
}

impl Ctx {
    /// Комит сейчас на троне — идёт приём.
    pub fn audience(&self) -> bool {
        self.act == Act::Sit && audience_day(self.day) && (AUDIENCE.0..AUDIENCE.1).contains(&self.hour)
    }
}

/// Приветствие: с первой встречи, утром, на приёме, вечером, в дождь, когда зачастил.
pub fn greeting(c: Courtier, x: &Ctx) -> &'static str {
    let pick = |l: &'static [&'static str]| l[(x.day as usize + x.met as usize) % l.len()];
    let late = x.hour >= 19.5 || x.hour < 5.0;
    let morning = (5.0..9.0).contains(&x.hour);
    let evening = (17.0..19.5).contains(&x.hour);
    match c {
        Courtier::Comes => {
            if x.met == 0 {
                return "Ты стоишь перед комитом Херсона, Флавием Евагрием, слугой благочестивейшего августа Анастасия. Назови себя и скажи, с чем пришёл.";
            }
            if x.audience() {
                return pick(&["Приём открыт. Говори, проситель, хартуларий запишет каждое слово.", "Подойди. Здесь говорят коротко и по делу — за тобой ещё люди.", "Слушаю. Если о пошлине — сначала к сакелларию, он скажет, что я откажу."]);
            }
            if x.met > 6 {
                return "Ты зачастил в преторию. Либо ты очень предан империи, либо тебе что-то нужно. Скорее второе.";
            }
            if x.act == Act::Dine {
                return "Мы за столом. Садись с краю, раз пришёл, — Агафья сегодня не пожалела перца.";
            }
            if x.act == Act::Guard {
                return "Не до бесед — смотрю, как трибун гоняет новобранцев. Говори, пока они бегут.";
            }
            if x.rain {
                return "Дождь в Таврике — к хорошему урожаю и к плохим дорогам. Говори, раз промок не зря.";
            }
            if morning {
                return pick(&["Рано же ты. Я ещё не выслушал донесений с ворот — говори коротко.", "Утро — для дел империи. Если твоё дело к ним относится — слушаю."]);
            }
            if evening || late {
                return pick(&["Вечер — время для вина и честных разговоров. Если не с жалобой — говори.", "День кончен, печати убраны. Что тебе, странник?"]);
            }
            pick(&["А, снова ты. Ромеи после полудня отдыхают, комиту отдых не положен.", "Говори. Только не о гуннах — о гуннах я сегодня уже слышал трижды."])
        }
        Courtier::Wife => {
            if x.met == 0 {
                return "Новое лицо в претории! Здравствуй. Я Евфимия, жена комита. Ты из города или издалека? Здесь так редко бывают люди, с которыми можно поговорить не о податях.";
            }
            if x.act == Act::Weave {
                return pick(&["Не обращай внимания на станок — руки заняты, а уши свободны.", "Это шерсть, крашенная в Тире. Муж ворчит, что она дороже его коня. Садись, рассказывай."]);
            }
            if x.act == Act::Dine {
                return "Садись к столу, не стой у двери. Мина, принеси гостю чашу!";
            }
            if late {
                return "Поздно уже. Говори тише — Феодор только уснул.";
            }
            pick(&["Ты снова здесь — я рада. Что нового в городе?", "А, это ты! Расскажи что-нибудь, пока комит занят своими донесениями.", "Мир тебе. Сад сегодня пахнет розами — я велела полить их морской водой… шучу, конечно."])
        }
        Courtier::Son => {
            if x.met == 0 {
                return "Ты кто? Воин? У тебя есть меч? Я Феодор, мой отец — комит, а я буду кентархом!";
            }
            pick(&["Опять ты! Сыграешь со мной в осаду? Ты будешь гуннами.", "Тсс! Я прячусь от Мины, он хочет вести меня к пресвитеру на урок.", "Смотри, какой у меня меч! Его сделали в оружейной, по-настоящему почти."])
        }
        Courtier::Chartularius => {
            if x.met == 0 {
                return "Стефан, хартуларий комита, — к услугам. Письма, прошения, грамоты, копии с печатью. Ты по делу или просто поглазеть на преторию?";
            }
            if x.act == Act::Write {
                return pick(&["Минуту… «…и да сохранит Господь благочестивое царство ваше». Так. Слушаю.", "Говори, только не толкай стол — третья копия, и опять кляксу посадишь не ты, а я."]);
            }
            if x.met > 4 {
                return "Ты приходишь чаще, чем почта из Константинополя. Это комплимент — почта приходит раз в месяц.";
            }
            pick(&["А, знакомое лицо. Что-нибудь записать? Или узнать? Узнать обычно дороже.", "Мир тебе. Перо очинено, чернила свежие, память — лучше любого архива."])
        }
        Courtier::Sacellarius => {
            if x.met == 0 {
                return "Никифор, сакелларий. Казна комита в моих руках, и руки эти не дрожат. Если пришёл за деньгами — уходи. Если принёс — садись.";
            }
            if x.act == Act::Write {
                return pick(&["…сорок нуммиев, сорок один… Ну вот, сбился. Что тебе?", "Не загораживай свет. Медь пересчитывают при свете, золото — при свидетелях."]);
            }
            pick(&["Опять ты. Казна от этого не прибавляется.", "Говори быстро: время сакеллария стоит дороже твоего.", "Мир тебе. Надеюсь, ты пришёл не просить."])
        }
        Courtier::Tribune => {
            if x.met == 0 {
                return "Фравитта, трибун. Гарнизон Херсона и стража претории — мои. Ты не похож на лазутчика, но и на просителя не похож. Кто ты?";
            }
            if x.act == Act::Guard {
                return pick(&["Говори на ходу. Я смотрю за постами.", "Стой рядом и не мешай. Видишь того, с кривым щитом? Сейчас получит."]);
            }
            if x.act == Act::Drink {
                return "О, садись! Вино здесь разбавляют так, что и гот не напьётся. Но я стараюсь.";
            }
            pick(&["Опять ты. Рука ещё держит меч?", "Мир тебе, странник. Говори прямо — я гот, хитрых речей не понимаю. Притворяюсь, конечно."])
        }
        Courtier::Presbyter => {
            if x.met == 0 {
                return "Мир тебе, чадо. Я Евсевий, служу в домовой церкви комита. Если ищешь утешения — ты пришёл туда, куда нужно. Если ищешь комита — тоже, но ему утешение нужнее.";
            }
            if matches!(x.act, Act::Pray | Act::Preach) {
                return "Подожди… «…и всех и за вся». Аминь. Теперь говори, чадо.";
            }
            pick(&["Мир тебе. Бог не любит торопливых, но любит пришедших.", "Снова здравствуй, чадо. Как душа?", "Садись. Я как раз читал Иоанна Златоуста — он говорит, что беседа с ближним тоже молитва."])
        }
        Courtier::Praepositus => {
            if x.met == 0 {
                return "Я Мина, препозит этого дома. Всякий, кто хочет к комиту, идёт через меня. Всякий, кто хочет к казне, идёт через стражу. Ты какой?";
            }
            if x.act == Act::Arrange {
                return "Не сейчас, не сейчас — господам подаю. Скажи коротко.";
            }
            pick(&["Снова ты. Обувь вытер? Мрамор здесь проконнесский, не херсонский булыжник.", "Чего тебе? Говори, пока слуги не разбежались.", "Ты бываешь здесь чаще иного куриала. Подозрительно."])
        }
        Courtier::Cook => {
            if x.met == 0 {
                return "Ой, кто это на моей кухне? Агафья я, кухарка. Ничего не трогай, а если голодный — так и скажи.";
            }
            pick(&["Опять пришёл на запах? Садись, только под руку не лезь.", "Котёл кипит, рыба ждёт, а ты болтать. Ну, говори.", "Голодный? По глазам вижу — голодный."])
        }
        Courtier::Guard(_) => {
            if late {
                return pick(&["Стой. Ночью в преторию не ходят. С чем пришёл?", "Кто идёт? Подойди к огню, чтобы я видел лицо."]);
            }
            if x.met == 0 {
                return "Стой! Здесь претория комита. Оружие держи в ножнах и говори, к кому.";
            }
            pick(&["Опять ты. Проходи, если по делу.", "Тихо сегодня. Что тебе?", "Видел тебя. Иди, но под ноги смотри — пол натёрт."])
        }
        Courtier::Protevon => {
            if x.met == 0 {
                return "Аристон, протевон курии Херсона. Мой род в этом городе дольше, чем стены Зенона. А ты, вижу, не здешний. По какому делу в претории?";
            }
            if x.act == Act::Talk && x.audience() {
                return "Не перебивай, я говорю с комитом о делах города!";
            }
            pick(&["Снова ты. Ждёшь приёма? Становись за мной.", "Мир тебе. Город не ждёт, а комит не торопится.", "Что скажешь, странник? Только не о погоде — о погоде у нас спорят в курии."])
        }
        Courtier::Petitioner(0) => {
            if x.met == 0 {
                return "Ох… ты тоже к комиту? Я Анна, вдова Феофила-рыбака. Третий день прихожу.";
            }
            "Всё стою, всё жду. Может, сегодня примет…"
        }
        Courtier::Petitioner(_) => {
            if x.met == 0 {
                return "Ксенофонт, с клера за южными воротами. Виноград растим, вино давим. А ты чей будешь?";
            }
            "Опять очередь. Протевон говорит долго — у них в курии все говорят долго."
        }
    }
}

/// Вопросы, которые можно задать этому человеку (без «Прощай» и «Что скажешь о…»).
pub fn topics(c: Courtier) -> &'static [Topic] {
    match c {
        Courtier::Comes => COMES,
        Courtier::Wife => WIFE,
        Courtier::Son => SON,
        Courtier::Chartularius => CHARTULARIUS,
        Courtier::Sacellarius => SACELLARIUS,
        Courtier::Tribune => TRIBUNE,
        Courtier::Presbyter => PRESBYTER,
        Courtier::Praepositus => PRAEPOSITUS,
        Courtier::Cook => COOK,
        Courtier::Guard(_) => GUARD,
        Courtier::Protevon => PROTEVON,
        Courtier::Petitioner(0) => WIDOW,
        Courtier::Petitioner(_) => VINTNER,
    }
}

/// Можно ли сейчас задать вопрос.
pub fn open(topic: &Topic, x: &Ctx, known: &impl Fn(Flag) -> bool) -> bool {
    match topic.when {
        When::Always => true,
        When::Knows(f) => known(f),
        When::Unknown(f) => !known(f),
        When::Audience => x.audience(),
        When::Private => !x.audience(),
        When::Familiar(n) => x.met >= n,
    }
}

/// Ответ на вопрос (n — который раз спрашивают).
pub fn answer(topic: &Topic, n: u32) -> &'static str {
    topic.say[n as usize % topic.say.len()]
}

/// Что слышно (новости и слухи меняются день ото дня).
pub fn news(c: Courtier, day: u32) -> &'static str {
    let l: &[&str] = match c {
        Courtier::Comes => &[
            "Из столицы пишут: август отменил хрисаргир, налог на ремесло и торг. Лавочники до сих пор ставят свечи за его здравие — и правильно делают.",
            "Новая медь, крупный фоллис с буквой «М», дошла и до нас: сорок нуммиев, не обманешься. Сакелларий впервые за годы улыбнулся.",
            "Персы держат мир, заключённый пять лет назад. Пока Кавад смотрит на восток, мы можем смотреть на север.",
            "Готы с гор Дори прислали людей: хотят соли и вина, предлагают мёд, воск и копья. Я дал им соль. Копья у них и так хороши.",
            "В Боспоре опять не могут решить, кто ими правит — император или гуннский вождь. Я бы подсказал, да меня не спрашивают.",
        ],
        Courtier::Wife => &[
            "Говорят, в столице на ипподроме опять дрались «зелёные» с «синими». Я скучаю по ипподрому… не по дракам, по колесницам.",
            "Купцы из Синопы привезли шёлк — настоящий, из Серики, через Персию. Никифор сказал, что казна не выдержит. Я сказала, что выдержит.",
            "В ксеноне у базилики опять не хватает хлеба для больных. Я велела Агафье печь лишнюю корзину каждое утро.",
            "Жена протевона Аристона снова ждёт ребёнка — уже седьмого! Курия растит себе смену.",
        ],
        Courtier::Chartularius => &[
            "Вчера пришла почта с дромона: магистр оффиций требует отчёт о стенах. Опять. Мы ведь их не двигали с прошлого отчёта.",
            "Епископ прислал жалобу на таможню: с церковного вина взяли коммеркий. Комит велел вернуть. Сакелларий велел подождать. Я пишу обоим «немедленно».",
            "Говорят, в Константинополе новый препозит священной спальни. Значит, все наши прошения опять лягут под сукно — новые люди читают с начала.",
            "Нотарий из курии продаёт копии старых договоров с Боспором. Я купил одну — там ошибка в имени царя. Теперь у меня есть, о чём рассказать за ужином.",
        ],
        Courtier::Sacellarius => &[
            "Ветер с севера держит суда в Синопе. Нет судов — нет пошлины, нет пошлины — нет жалованья. Простая арифметика.",
            "Солид стоит сто восемьдесят фоллисов, а на рынке пытаются взять двести. Я бы этих менял — в карцер, но комит говорит «рынок».",
            "Таможня за прошлый месяц собрала на восьмую часть больше. Не радуйся за меня — значит, в следующий раз столица попросит больше.",
        ],
        Courtier::Tribune => &[
            "Разъезд видел дым у Балаклавы. Это не гунны — пастухи жгут сухую траву. Но коня я всё равно оседлал.",
            "Трое новобранцев из готов сбежали домой на сбор винограда. Двое вернулись с вином. Третьего прощу, если вернётся с двумя кувшинами.",
            "Утигуры дерутся с кутригурами за пастбища у Меотиды. Пусть дерутся подольше — нам спокойнее.",
        ],
        Courtier::Presbyter => &[
            "Епископ собирает синод: снова спорят об Энотиконе Зенона. Кто-то говорит, что в Христе одна природа, кто-то — что две. А больные в ксеноне спрашивают только, будет ли хлеб.",
            "Из монастыря на мысе пришли иноки — просят масла для лампад. Комит дал. Он скуп на слова, но не на масло.",
            "В годовщину святого Климента море, говорят, отступает от его гробницы. Я не видел, но верю тем, кто видел.",
        ],
        Courtier::Praepositus => &[
            "Слуга Петр опять разбил кувшин. Третий за месяц. Я вычитаю из жалования, а он говорит, что кувшины сами прыгают.",
            "Госпожа велела выбелить сад к Пасхе. Сад! Белить! Хорошо хоть розы не красить.",
            "К комиту ездит один гот, будто бы торговец мёдом. Мёда я от него не видел ни разу.",
        ],
        Courtier::Protevon => &[
            "Водопровод опять течёт у резервуара. Курия чинит на свои — а таможня, между прочим, собирает с нашего же порта.",
            "Отец города хочет новую цистерну у западной церкви. Комит говорит — сначала стены. У нас всегда сначала стены.",
            "Рыбаки ругаются: готы ставят сети у самых наших берегов. Курия написала комиту, комит написал готам. Готы, по-моему, не читают.",
        ],
        _ => &["Ничего особенного. Тихо. Слава Богу."],
    };
    l[(day as usize) % l.len()]
}

/// Что этот человек думает о другом из двора (для «Что скажешь о…»).
pub fn opinion(c: Courtier, about: Courtier) -> Option<&'static str> {
    use Courtier::*;
    Some(match (c, about) {
        (Comes, Wife) => "Моя супруга — из рода, что при Льве сидел в сенате. Поехала со мной на край ойкумены и ни разу не попрекнула. Разве что за шёлк.",
        (Comes, Son) => "Феодор бегает по саду с деревянным мечом и считает себя кентархом. Да хранит его Бог от настоящей войны.",
        (Comes, Chartularius) => "Стефан пишет быстрее, чем я думаю, и болтает быстрее, чем пишет. Первое ценю, за второе однажды сошлю его на остров.",
        (Comes, Sacellarius) => "Никифор скуп, как монах на исповеди. Поэтому у Херсона ещё есть стены и нет долгов.",
        (Comes, Tribune) => "Фравитта был со мной под Амидой. Гот, а вернее иного ромея. Если он скажет, что ты годен, — значит, годен.",
        (Comes, Presbyter) => "Отец Евсевий молится за меня чаще, чем я сам. Я ему за это должен больше, чем казне.",
        (Comes, Praepositus) => "Мина служил ещё моему отцу. Он знает в этом доме всё, кроме того, когда пора замолчать.",
        (Comes, Protevon) => "Аристон любит Херсон и не любит меня. Это честно: я тоже люблю Херсон больше, чем Аристона.",
        (Wife, Comes) => "Он суров только с виду. Ночью, когда думает, что все спят, ходит к образу и молится за стражу на стенах.",
        (Wife, Son) => "Феодор — моё солнце и моё наказание. Вчера он «взял в плен» Агафьину курицу.",
        (Wife, Chartularius) => "Стефан влюблён в вдову с портовой улицы и думает, что никто не знает. Знают все, даже куры.",
        (Wife, Sacellarius) => "Никифор считает, что женщине нужны только веретено и молитвослов. А сам каждый вечер просит у меня совета о ценах на шерсть.",
        (Wife, Tribune) => "Фравитта рычит на солдат, а Феодору вырезал из дерева коня. Хороший человек, только пьёт за двоих готов.",
        (Wife, Presbyter) => "Отец Евсевий — единственный здесь, кто читал Платона. И единственный, кто в этом не признаётся.",
        (Wife, Praepositus) => "Мина бранится на всех и любит всех. Без него этот дом развалился бы за неделю.",
        (Chartularius, Comes) => "Комит — человек слова. Буквально: пока я не запишу, он не считает, что сказал. Это удобно — для меня.",
        (Chartularius, Sacellarius) => "Никифор однажды выдал мне жалованье и плакал. Я не преувеличиваю — плакал.",
        (Chartularius, Tribune) => "Трибун считает, что перо — оружие трусов. А сам просит меня писать письма его матери в Дори. Каждый месяц.",
        (Chartularius, Wife) => "Госпожа — самый умный человек в этом доме. Я записываю её советы комиту под его именем. Не говори никому.",
        (Chartularius, Praepositus) => "Мина знает, кто с кем, когда и почём. Я знаю, кто что пишет. Вместе мы знаем всё, поэтому друг друга не любим.",
        (Sacellarius, Comes) => "Комит щедр. Слишком щедр. Каждая его щедрость — моя бессонница.",
        (Sacellarius, Chartularius) => "Стефан тратит чернила, как будто они растут на деревьях. Это не так. Я проверял.",
        (Sacellarius, Wife) => "Госпожа понимает в шерсти больше, чем все купцы Синопы. Не говори ей, что я так сказал.",
        (Sacellarius, Tribune) => "Трибун просит жалованье солдатам. Каждый месяц. Будто солдаты каждый месяц едят.",
        (Tribune, Comes) => "Он был под Амидой. Я тоже. Когда персы вошли в город, он вывел нас через пролом. Этого хватает, чтобы я пошёл за ним в огонь.",
        (Tribune, Chartularius) => "Писарь. Языком машет, как мечом не умеет. Но письма матери пишет хорошо — она плачет.",
        (Tribune, Sacellarius) => "Никифор каждую медь рассматривает, будто она отравлена. Солдат он рассматривает так же.",
        (Tribune, Son) => "Мальчишка держит меч правильно. Я учил. Ещё год — и дам ему настоящий, только тупой.",
        (Presbyter, Comes) => "У комита тяжёлая совесть — он видел, как умирает город. Такие люди или ожесточаются, или становятся милосердны. Он пока выбирает.",
        (Presbyter, Wife) => "Госпожа Евфимия раздаёт хлеб больным, а говорит, что это остатки с кухни. Господь видит, какие это «остатки».",
        (Presbyter, Tribune) => "Фравитта крестился уже взрослым, отрёкся от ереси Ария. Верит так, как воюет: прямо и без хитростей.",
        (Praepositus, Comes) => "Я служил его отцу. Отец был мягче, сын — справедливее. Не знаю, что лучше для слуг.",
        (Praepositus, Chartularius) => "Хартуларий таскает из кухни мёд, думая, что я не вижу. Я вижу всё. Я молчу. Пока.",
        (Praepositus, Sacellarius) => "Никифор и я спорим о каждой свече. Тридцать лет спорим. Свечей от этого больше не стало.",
        (Praepositus, Cook) => "Агафья лучше всех в Херсоне варит рыбу и хуже всех в Херсоне молчит.",
        (Cook, Comes) => "Комит ест, как солдат: что дали, то и съел, лишь бы горячее. Госпожа — та разборчива.",
        (Cook, Praepositus) => "Мина вечно лезет в котлы. Ворчит, пробует, ворчит. Ему просто нравится моя рыба.",
        (Protevon, Comes) => "Он умный человек и честный чиновник. Это самое опасное сочетание — его не подкупишь и не переспоришь.",
        (Protevon, Sacellarius) => "Никифор собирает с нашего порта коммеркий и называет это «заботой императора». Забота у императора дорогая.",
        (Guard(_), Tribune) => "Трибун строг. Проспишь смену — неделю на стене стоять. Но и сам ночью посты обходит, не ленится.",
        (Guard(_), Comes) => "Комит здоровается с каждым часовым. По имени. Ни один из прежних так не делал.",
        (Son, Comes) => "Отец самый сильный. Он под Амидой был! Это такой город, его персы взяли, а отец не сдался.",
        (Son, Tribune) => "Дядя Фравитта мне коня вырезал! И учит меня на мечах. Только маме не говори.",
        (Son, Presbyter) => "Отец Евсевий хороший, но заставляет учить псалмы. Я уже знаю два. Почти.",
        _ => return None,
    })
}

/// О ком можно спросить этого человека.
pub fn gossip_about(c: Courtier) -> Vec<Courtier> {
    Courtier::ALL.iter().copied().filter(|a| *a != c && !matches!(a, Courtier::Guard(1..) | Courtier::Petitioner(_)) && opinion(c, *a).is_some()).collect()
}

/// Прощание.
pub fn farewell(c: Courtier, x: &Ctx) -> &'static str {
    let k = (x.day + x.met) as usize;
    let l: &[&str] = match c {
        Courtier::Comes => &["Ступай с Богом.", "Иди. И не задерживайся у казны — стража у Никифора неразговорчивая.", "Будь здоров. Империя помнит своих друзей. Иногда."],
        Courtier::Wife => &["Приходи ещё, слышишь? Здесь так тихо без новостей.", "Да хранит тебя Богородица.", "Иди с миром. И загляни в ксенон, если будешь у базилики."],
        Courtier::Son => &["Пока! Приходи сражаться!", "Только Мине не говори, где я прячусь!"],
        Courtier::Chartularius => &["Заходи. Чернила у меня всегда есть, а новости — почти всегда.", "Прощай. Если что — я ничего не говорил."],
        Courtier::Sacellarius => &["Иди. И дверь закрой — сквозняк монеты сдувает.", "Прощай. Ничего не трогай по дороге."],
        Courtier::Tribune => &["Держи меч наточенным.", "Иди. Если услышишь трубу — беги к стенам, не к тавернам."],
        Courtier::Presbyter => &["Господь с тобою, чадо.", "Иди с миром и молись за этот дом."],
        Courtier::Praepositus => &["Иди, иди. Да ковры не топчи.", "Прощай. Я тебя запомнил."],
        Courtier::Cook => &["Иди, а то котёл убежит!", "Ступай. Будешь голодный — знаешь, где кухня."],
        Courtier::Guard(_) => &["Проходи.", "Ступай. Без оружия наголо."],
        Courtier::Protevon => &["Прощай. И помни: Херсон — это курия, а не претория.", "Будь здоров."],
        Courtier::Petitioner(_) => &["Помолись за меня, чтоб принял.", "Иди с Богом."],
    };
    l[k % l.len()]
}

/// Устал от расспросов: после стольких вопросов за один разговор уходит.
pub fn patience(c: Courtier) -> u32 {
    match c {
        Courtier::Comes => 6,
        Courtier::Sacellarius | Courtier::Guard(_) => 4,
        Courtier::Son => 5,
        Courtier::Wife | Courtier::Chartularius => 10,
        _ => 7,
    }
}

/// Что говорит, когда устал.
pub fn tired(c: Courtier) -> &'static str {
    match c {
        Courtier::Comes => "Довольно. У комита есть и другие дела, кроме твоих вопросов. Приходи в час приёма.",
        Courtier::Wife => "Ох, ты меня совсем заговорил. Приходи завтра — я отдохну и расскажу ещё.",
        Courtier::Son => "Скучно! Я пошёл воевать с кипарисами.",
        Courtier::Chartularius => "Всё, всё, перо остывает. Остальное — за отдельную плату… шучу. Или нет.",
        Courtier::Sacellarius => "Время — тоже деньги. Твоё время кончилось.",
        Courtier::Tribune => "Хватит болтать. Мне на стены.",
        Courtier::Presbyter => "Устал я, чадо. Иди, помолись и сам подумай над тем, что спрашивал.",
        Courtier::Praepositus => "Всё. Работа ждёт. Иди.",
        Courtier::Cook => "Иди-иди, у меня рыба горит!",
        Courtier::Guard(_) => "Проходи, не стой на посту.",
        Courtier::Protevon => "Простите, меня ждут в курии.",
        Courtier::Petitioner(_) => "Ох, мне бы с мыслями собраться перед комитом…",
    }
}

/// Разговор, который слышно, когда двое из двора беседуют рядом (строки по очереди).
pub fn overheard(a: Courtier, b: Courtier) -> Option<&'static [(Courtier, &'static str)]> {
    use Courtier::*;
    let pair = |x: Courtier, y: Courtier| (a == x && b == y) || (a == y && b == x);
    Some(if pair(Comes, Chartularius) {
        &[
            (Comes, "Что с ночными донесениями, Стефан?"),
            (Chartularius, "С портовых ворот — тихо. С главных — пьяный гот спорил со стражей о цене на вино."),
            (Comes, "Цену на вино пусть решает рынок, а не стража. Готу — ночь в карцере."),
            (Chartularius, "Записываю: «ночь в карцере и наставление»."),
            (Comes, "И напиши магистру: стены стоят, люди сыты, гунны далеко. Пока."),
            (Chartularius, "«Пока» тоже записать?"),
            (Comes, "Особенно «пока»."),
        ]
    } else if pair(Comes, Sacellarius) {
        &[
            (Sacellarius, "Коммеркий за неделю — триста солидов без малого. Если считать медь."),
            (Comes, "Сколько уйдёт на жалованье?"),
            (Sacellarius, "Всё. И ещё немного. Как обычно."),
            (Comes, "А на кладку у южной башни?"),
            (Sacellarius, "На кладку — молитвы отца Евсевия. Они бесплатные."),
            (Comes, "Найди, Никифор. Зенон чинил стены на таможенные деньги — и я починю."),
        ]
    } else if pair(Chartularius, Sacellarius) {
        &[
            (Chartularius, "Никифор, мне бы ещё пачку папируса."),
            (Sacellarius, "Пачку? Ты в прошлом месяце брал пачку."),
            (Chartularius, "А в этом месяце империя опять пишет нам письма."),
            (Sacellarius, "Пиши мельче."),
            (Chartularius, "Мельче — магистр не прочтёт. Прочтёт — не поверит."),
            (Sacellarius, "Ладно. Полпачки. И отчёт, на что ушла прежняя."),
        ]
    } else if pair(Comes, Wife) {
        &[
            (Wife, "Ты опять не спал. Я слышала, как ты ходил."),
            (Comes, "Утигуры у Меотиды. Думал, сколько у нас зерна в горреях."),
            (Wife, "И сколько?"),
            (Comes, "На зиму хватит. На осаду — нет."),
            (Wife, "Тогда пусть Бог хранит нас от осады, а ты — посмотри, как распустились розы."),
            (Comes, "…Красные. Как в саду у твоего отца."),
        ]
    } else if pair(Praepositus, Cook) {
        &[
            (Praepositus, "Агафья, что сегодня господам?"),
            (Cook, "Кефаль под гарумом, чечевица, хлеб, сыр, смоквы."),
            (Praepositus, "Опять кефаль?"),
            (Cook, "Хочешь — сам лови тюрбо. Порт в той стороне."),
            (Praepositus, "Гарум не жалей, комит любит острое."),
            (Cook, "Гарум мне Никифор выдаёт по ложке. Ты ему скажи."),
        ]
    } else if pair(Comes, Tribune) {
        &[
            (Comes, "Как новобранцы?"),
            (Tribune, "Бегают хорошо. Особенно от гуннов — я проверил, крикнул «гунны!»."),
            (Comes, "А стоять?"),
            (Tribune, "Стоять научу. Под Амидой и мы не сразу научились."),
        ]
    } else if pair(Wife, Son) {
        &[
            (Son, "Мама, я сегодня победил трёх персов!"),
            (Wife, "Это были кипарисы, Феодор."),
            (Son, "Персидские кипарисы!"),
            (Wife, "Иди умойся, воин. Скоро обед."),
        ]
    } else {
        return None;
    })
}

/// Мимолётная фраза, когда подходишь (без разговора).
pub fn bark(c: Courtier, x: &Ctx) -> Option<&'static str> {
    let k = (x.day as usize + x.met as usize) % 3;
    Some(match (c, x.act) {
        (_, Act::Sleep | Act::Away | Act::Cower) => return None,
        (Courtier::Guard(_), _) => ["Оружие — в ножнах.", "Претория комита. Проходи тихо.", "Стой… а, это ты."][k],
        (Courtier::Comes, Act::Sit) => ["Следующий!", "Хартуларий, запиши.", "Говорите по очереди."][k],
        (Courtier::Praepositus, _) if x.hour > 8.8 && x.hour < 12.0 => ["Просители — налево, к стене. Не толпитесь!", "Тише, комит слушает.", "Ждать здесь."][k],
        (Courtier::Son, _) => ["Ура! В атаку!", "Не поймаешь!", "Я — кентарх!"][k],
        (Courtier::Cook, _) => ["Ох, жарко…", "Где же соль?", "Петр! Дров!"][k],
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Вопросы по лицам
// ---------------------------------------------------------------------------------------------

static COMES: &[Topic] = &[
    t("Кто ты, господин?", &[
        "Я из Константинополя, родом фракиец. Служил под Амидой, когда персы Кавада взяли её после осады. Я видел, как город умирает от голода и измены, — и поклялся, что мой так не падёт. Август прислал меня сюда три года назад: держать стены, собирать коммеркий, мирить готов с херсонитами и не пускать гуннов за Боспор.",
        "Я уже говорил: комит, фракиец, ветеран персидской войны. Остальное — дело империи, а не твоё.",
    ]),
    t("Как устроена власть в Херсоне?", &[
        "Я — комит: в моих руках гарнизон, таможня и суд по делам империи. Но город живёт по своему обычаю: курия выбирает протевонов, отец города ведает водопроводом, хлебом и стенами. Епископ печётся о душах и о бедных. Я не мешаю им, пока они не мешают императору.",
        "Курия решает, где класть мостовую. Я решаю, кто будет по ней маршировать. Так и живём.",
    ]),
    t("Что тебя тревожит?", &[
        "Утигуры кочуют у Меотиды. Пока они грызутся с кутригурами — нам спокойно. Помирятся — придут сюда, и я хочу, чтобы к тому дню у нас были целы стены и полны горреи.",
        "Деньги. Жалованье стратиотам из столицы приходит, когда сакелларий перестаёт молиться о нём. Зенон починил половину стен на таможенные сборы — я хочу починить вторую половину тем же способом.",
        "Вера. Август склоняется к тем, кто говорит об одной природе Христа; наш епископ держится Халкидона. Мне приходится быть глухим на оба уха — и молиться, чтобы спор не дошёл до мечей.",
    ]),
    t("Что слышно в мире?", &[]),
    tw("Подать прошение", When::Audience, Some(Flag::Petition), &[
        "Говори… Так. Хартуларий запишет твоё прошение. Если оно о земле — суд будет после Ид. Если о пошлине — сакелларий объяснит, почему я откажу. Если о несправедливости — я разберу сам. Ступай и жди ответа.",
        "Твоё прошение уже у хартулария. Дважды одно и то же я не читаю — второе ляжет под первое.",
    ]),
    tw("Можно ли служить тебе?", When::Private, None, &[
        "Империи нужны люди, которые умеют держать меч и язык за зубами. Поговори с трибуном Фравиттой — если он скажет, что ты годен, я найду тебе дело. И научись грамоте: неграмотных я посылаю только на стены.",
        "Я сказал: сначала трибун. Мне не нужны люди, которых я не видел в деле.",
    ]),
    tw("О твоей семье", When::Familiar(2), None, &[
        "Моя супруга Евфимия — из старого сенаторского рода. Сын Феодор считает себя кентархом. Я хочу, чтобы он вырос там, где ромеев не убивают за стенами собственного города. Поэтому стены — первое.",
    ]),
    tw("Стефан говорил о письме из столицы…", When::Knows(Flag::Letter), None, &[
        "Значит, Стефан проболтался… Да, пришло письмо от магистра оффиций: через Херсон в столицу поедет посольство утигуров. Мне велено принять их с почётом и показать им наши стены — пусть видят, что зубы у империи ещё есть. Об этом — никому. Стефану передай, что язык у писца должен быть короче пера.",
        "Я уже сказал больше, чем следовало. Забудь.",
    ]),
    tw("Я хочу служить, трибун меня видел", When::Knows(Flag::Service), None, &[
        "Фравитта сказал, что ты не трус и не дурак. Для начала хватит. Служи городу: следи, чтобы на рынке не обвешивали, чтобы у ворот не пили, чтобы в порту не воровали. Хорошие дела дойдут до меня — Херсон маленький.",
    ]),
    tw("О святом Клименте", When::Knows(Flag::Clement), None, &[
        "Отец Евсевий рассказал? Я не знаю, правда ли море отступает от гробницы. Но я знаю, что папа Климент был сослан сюда и не сломался. В этом городе ссыльные часто сильнее тех, кто их сослал.",
    ]),
];

static WIFE: &[Topic] = &[
    t("Кто ты, госпожа?", &[
        "Я Евфимия, дочь сенатора Лонгина. Росла в Константинополе, у Влахерн, а теперь вот — на краю ойкумены. Не смотри с жалостью: здесь море, сад и муж, которого я выбрала сама. Ну, почти сама.",
        "Я тебе уже рассказывала — дочь сенатора, жена комита, мать воина. Последнее — по мнению Феодора.",
    ]),
    t("Скучаешь по столице?", &[
        "По Святой Софии — да. По ипподрому — немного. По сплетням при дворе — ни капли. Здесь сплетни честнее: если жена протевона кого-то ругает, то в лицо.",
        "Иногда снится Месса — главная улица, колоннады, лавки с шёлком. Просыпаюсь — а за окном кипарис и чайки. Тоже неплохо.",
    ]),
    t("Что ты ткёшь?", &[
        "Хламиду мужу, к празднику. Шерсть крашена тирским пурпуром — чуть-чуть, по краю, большего комиту не положено: пурпур весь — августу. А таблион вышью золотом сама. Мои руки ещё помнят, чему учили во дворце.",
        "Всё ту же хламиду. Шерсть длинная, вечера короткие.",
    ]),
    tw("Чем помочь тебе?", When::Unknown(Flag::Alms), Some(Flag::Alms), &[
        "Помочь? Как мило… Тогда вот что: в ксеноне у базилики больные лежат без хлеба. Если будешь там — отнеси им хоть что-нибудь, лепёшку, мёда, бинтов. Скажешь, что от Евфимии, — врач поймёт.",
    ]),
    tw("Я был в ксеноне", When::Knows(Flag::Alms), None, &[
        "Правда? Спасибо тебе. Господь видит такие дела, даже если их не видит никто. А я — я запомню.",
        "Ты уже говорил. И я уже запомнила.",
    ]),
    t("Что слышно в доме и в городе?", &[]),
];

static SON: &[Topic] = &[
    t("Ты правда будешь кентархом?", &[
        "Буду! Нет, сначала декархом. Дядя Фравитта говорит, что сначала надо научиться чистить коня. Фу.",
        "А потом стратигом! И пойду на персов. Только мама не пустит.",
    ]),
    t("Во что ты играешь?", &[
        "В осаду Амиды! Сад — это город, кипарисы — персы, а я — отец, и я вывожу всех через пролом!",
        "Сегодня в гуннов. Курица — гуннский вождь. Только она убежала.",
    ]),
    t("Ты учишься грамоте?", &[
        "Учусь… Отец Евсевий заставляет писать на воске «альфа, бета, гамма». А я хочу рисовать коней. Я на табличке нарисовал коня — он сказал, что это верблюд.",
    ]),
];

static CHARTULARIUS: &[Topic] = &[
    t("Кто ты?", &[
        "Стефан, сын Иоанна-нотария, хартуларий комита. Учился в Константинополе у грамматика Прокла — не того, философа, а другого, который бил линейкой. Теперь пишу письма магистру оффиций и отвечаю за всё, что комит сказал, — и за то, чего не говорил, тоже.",
    ]),
    t("Чем занят хартуларий?", &[
        "Хартии — грамоты, письма, прошения. Комит говорит — я пишу. Комит подписывает — я запечатываю. Курия приносит жалобу — я её читаю первым и решаю, в каком настроении её прочтёт комит. Это и есть власть, если подумать.",
        "Переписываю, сверяю, храню. В архиве за стеной лежат грамоты ещё времён Зенона. Пахнут мышами и историей.",
    ]),
    tw("Что пишут из столицы?", When::Unknown(Flag::Letter), Some(Flag::Letter), &[
        "Ну… вообще-то это тайна. Но раз ты спросил так вежливо… Пришло письмо от самого магистра оффиций, с пурпурной печатью! Что-то про посольство с севера. Больше — ни слова. Спроси комита, если не боишься.",
    ]),
    tw("Что пишут из столицы?", When::Knows(Flag::Letter), None, &[
        "Я и так сказал лишнее. Комит, наверное, уже знает, что ты знаешь, — у него на это чутьё.",
    ]),
    t("Научишь грамоте?", &[
        "Я? Нет, у меня рука для письма, а не для розги. Иди к нотарию в портике — он учит писать и считать за плату. Или в дидаскалион у агоры, там грамматист учит мальчишек азбуке. А книги — в книгохранилище, если пустят.",
    ]),
    tw("Как моё прошение?", When::Knows(Flag::Petition), None, &[
        "Твоё прошение? Так… вот оно, третье снизу. Комит прочтёт его после Ид. Если хочешь раньше — ну, ты понимаешь: перо тупится, чернила дорожают…",
        "Всё ещё третье снизу. Нет, второе — одного просителя вчера похоронили.",
    ]),
    t("Что нового?", &[]),
];

static SACELLARIUS: &[Topic] = &[
    t("Кто ты?", &[
        "Никифор, сакелларий — хранитель мешка. Сорок лет при казне: начинал при Льве, считал при Зеноне, пересчитываю при Анастасии. Императоры меняются, медь остаётся.",
    ]),
    t("Откуда деньги у Херсона?", &[
        "Коммеркий — пошлина с каждого судна: восьмая часть товара, что входит и выходит. Подати с клеров. Аренда лавок у порта. Из столицы — жалованье гарнизону, когда вспомнят. А тратим на стены, на солдат, на хлеб в неурожай — и на шёлк госпожи, да простит меня Бог.",
        "Я сказал тебе — восьмая часть с купцов. Повтори, а то и с тебя возьму восьмую часть за урок.",
    ]),
    t("Расскажи о монетах", &[
        "Смотри: золотой солид — семьдесят два из фунта, вес не подделать, на нём август в доспехе. Серебро — милиарисий, редкость. А медь — вот: новый фоллис, крупный, с буквой «М». «М» — это сорок нуммиев. Анастасий велел чеканить такие, чтобы менялы не обманывали народ. Половина — «К», двадцать. Четверть — «Л»… нет, «I», десять. Запомнил? Проверю.",
        "Солид, фоллис, нуммий. «М» — сорок. Если меняла даёт тебе за солид меньше ста восьмидесяти фоллисов — он вор, а если больше — фальшивомонетчик.",
    ]),
    t("Можно взять в долг?", &[
        "В долг? У казны императора? Ха. Казна даёт в долг только императору — и то под залог провинций.",
        "Нет.",
        "Ты опять? Нет — по-гречески «οὐ», по-латыни «non», по-готски не знаю, но тоже нет.",
    ]),
    t("Что слышно?", &[]),
];

static TRIBUNE: &[Topic] = &[
    t("Кто ты?", &[
        "Фравитта, сын Сунильды, из готов Дори. Отец пас овец, а я пошёл в федераты. Воевал с персами под Амидой — там и встретил комита. Теперь я трибун: гарнизон Херсона, ворота, стены, палатная стража. Крещён в православии — не смотри так, не все готы ариане.",
    ]),
    t("Сколько воинов в гарнизоне?", &[
        "Сколько надо — столько и есть. Не твоё дело, если ты не лазутчик. А если лазутчик — тем более.",
        "Ладно. Нумер — несколько сотен копий, лучники на башнях, конный разъезд по Гераклейскому полуострову. Мало для войны, много для набега.",
    ]),
    t("Опасны ли гунны?", &[
        "Утигуры — да. Конные лучники, бьют с седла на полном скаку. Мы против них — стены и камнемёты. В поле я бы с ними не встречался без союзных готов.",
        "Гунн боится трёх вещей: стены, голода и другого гунна. Стену мы держим, голод — не наш, а другого гунна мы им всегда находим.",
    ]),
    tw("Возьмёшь меня на службу?", When::Unknown(Flag::Service), Some(Flag::Service), &[
        "Хм. Покажи руки… Мозоли есть, держать меч умеешь. Иди на плац, поработай с чучелом, а я посмотрю. Потом скажи комиту, что Фравитта тебя видел. Остальное — его дело.",
    ]),
    tw("Возьмёшь меня на службу?", When::Knows(Flag::Service), None, &[
        "Я своё сказал. Иди к комиту. И не забывай упражняться — меч ржавеет быстрее, чем совесть.",
    ]),
    t("Что слышно на стенах?", &[]),
];

static PRESBYTER: &[Topic] = &[
    t("Благослови, отче", &[
        "Господь да благословит тебя и сохранит тебя. Да осияет тебя лицом Своим и помилует тебя. Иди, чадо, и не греши… по крайней мере, до вечера.",
        "Благословляю. Бог видит, что ты пришёл не только за этим.",
    ]),
    tw("Расскажи о святых Херсона", When::Unknown(Flag::Clement), Some(Flag::Clement), &[
        "Слушай. Сюда, в каменоломни Таврики, при императоре Траяне сослали Климента, епископа Рима. Он и здесь проповедовал, и крестил, и нашёл источник для узников. За это его с якорем на шее бросили в море. А море, говорят, каждый год в день его кончины отступает и открывает гробницу. Ещё в нашем городе пострадали святые епископы-мученики — Василей, Ефрем, Капитон и другие. Херсон стоит на их крови.",
    ]),
    tw("Расскажи о святых Херсона", When::Knows(Flag::Clement), None, &[
        "Я уже рассказал о Клименте. Пойди к морю в его день — может, увидишь сам. А нет — молись, этого достаточно.",
    ]),
    t("О спорах о вере", &[
        "Ты про Халкидон и Энотикон? Тише, чадо. В этом доме об этом говорят шёпотом, а в столице — с камнями в руках. Я скажу так: Христос один, и людей Он любит больше, чем наши споры о Нём.",
    ]),
    t("Исповедуй меня, отче", &[
        "Говори… Нет, не мне на ухо, а Богу, я только свидетель… Так. Тяжело, но не безнадёжно. Прочти «Отче наш» трижды и подай нищему у базилики. И не кради больше — даже у сакеллария. Особенно у сакеллария.",
        "Опять? Значит, грех вернулся. Возвращайся и ты — каяться, а не грешить.",
    ]),
    t("Что слышно?", &[]),
];

static PRAEPOSITUS: &[Topic] = &[
    t("Кто ты?", &[
        "Мина, вольноотпущенник дома Евагриев. Был рабом отца комита, получил свободу и остался служить. Я препозит: слуги, кухня, двери, ключи, гости. Если что-то в этом доме стоит, горит или пахнет — это моё.",
    ]),
    t("Как попасть к комиту?", &[
        "Комит принимает в зале с девятого часа утра до полудня — всякий день, кроме воскресенья. Вход с плаца, через пропилей. Стань в очередь, не перебивай протевона, говори коротко. Вне приёма — только если трибун или я впустим. Я не впущу.",
        "С девяти до полудня. В зале. Я же сказал. Ты что, в первый раз в претории?",
    ]),
    t("Как живёт дом комита?", &[
        "Вставать — до рассвета: в шестом часу комит молится с отцом Евсевием в оратории. Потом донесения в портике, казна, с девяти — приём. В полдень — обед в триклинии, после — отдых. Под вечер комит смотрит гарнизон на плацу, потом гуляет с госпожой в саду, иногда идёт к вечерне в базилику. Ужин — когда стемнеет. Спать — когда я задую лампы.",
    ]),
    t("Какие слухи ходят?", &[]),
];

static COOK: &[Topic] = &[
    t("Чем кормишь комита?", &[
        "Комит ест, как солдат: хлеб, сыр, рыбу, чечевицу. Но госпожа — та из столицы, ей подавай кефаль под гарумом, перепелов, смоквы в мёду. Вот и варю на два вкуса.",
        "Сегодня — уха из барабульки, лепёшки и оливки. Завтра — опять уха. Море рядом, а Никифор денег на мясо не даёт.",
    ]),
    t("Угостишь?", &[
        "Ну на, держи лепёшку с оливковым маслом. Только Мине не говори — он считает каждую.",
        "Опять? Ох, да ты как Феодор. Ладно, кусок сыра. И всё!",
        "Больше не дам — самой не хватит. Приходи после ужина, может, останется.",
    ]),
    t("Что такое гарум?", &[
        "Рыбный соус! Рыбу с солью кладут в чаны — вон, у базилики цистерны, — и месяц она там томится на солнце. Потом сливают сок. Пахнет — ох, но в еде — чистое золото.",
    ]),
];

static GUARD: &[Topic] = &[
    t("Можно пройти?", &[
        "Днём — в зал, если на приём. В покои — нельзя. В казну — тем более нельзя. Понял?",
        "Я сказал: в зал — да, дальше — нет.",
    ]),
    t("Как служба?", &[
        "Стоим. Днём — жарко, ночью — холодно, жалованье — когда сакелларий вспомнит.",
        "Хорошая служба. Кормят, крыша над головой, и никаких гуннов. Пока.",
    ]),
];

static PROTEVON: &[Topic] = &[
    t("Кто ты?", &[
        "Аристон, сын Аполлония. Протевон — первый среди куриалов Херсона. Мой прадед подписывал договор с Боспором, мой дед строил цистерны у западной церкви. Я — спорю с комитом. Каждый в своё время служит городу как может.",
    ]),
    t("Что такое курия?", &[
        "Городской совет. Мы, куриалы, отвечаем за подати, водопровод, хлеб, бани, мостовые — своим имуществом отвечаем! Если город не соберёт податей — платим мы. Поэтому мы любим наш город. И не любим, когда комит берёт порт себе.",
    ]),
    tw("С чем ты к комиту?", When::Unknown(Flag::Aqueduct), Some(Flag::Aqueduct), &[
        "Водопровод течёт у резервуара — уже месяц. Курия чинит, но камень дорог, а таможенные деньги идут в казну комита. Я прошу часть коммеркия на воду. Если откажет — пойду к епископу. Если и епископ откажет — напишу в столицу. Посмотрим, кто кого.",
    ]),
    tw("Как дела с водопроводом?", When::Knows(Flag::Aqueduct), None, &[
        "Комит обещал подумать. «Подумать» у него значит «после стен». Но я терпелив — я куриал, мы рождаемся терпеливыми, а умираем — в долгах.",
    ]),
    t("Что нового в городе?", &[]),
];

static WIDOW: &[Topic] = &[
    t("О чём ты просишь?", &[
        "Муж мой, Феофил, утонул весной у мыса. Лодка его осталась — моя, значит, и детям кормиться. А мытарь взял лодку за недоимку. Говорит, муж не заплатил коммеркий с последнего улова. А какой улов, если он не вернулся? Я прошу комита вернуть лодку.",
    ]),
    t("Думаешь, комит поможет?", &[
        "Говорят, он справедливый. Говорят, он под Амидой сам голодал. Значит, должен понимать, каково это. Я молюсь.",
    ]),
];

static VINTNER: &[Topic] = &[
    t("О чём ты просишь?", &[
        "Межа с соседом! Клер мой — от отца, от деда, ещё при Феодосии нарезан. А сосед, Агафон, говорит, что три ряда лоз — его. Курия судила — не рассудила. Теперь к комиту.",
    ]),
    t("Хорошее ли вино на клерах?", &[
        "Хорошее! Белое, крепкое, с полынью — степняки за него коней отдают. Только разбавлять надо, а то голова наутро как у гунна в седле.",
    ]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(hour: f32, met: u32, act: Act) -> Ctx {
        Ctx { hour, day: 3, met, act, rain: false }
    }

    #[test]
    fn everyone_has_a_post_and_a_plan() {
        let posts = palace::court_posts();
        for c in Courtier::ALL {
            assert!(posts.iter().any(|(p, _, _)| *p == c), "нет места у {c:?}");
            for day in 0..14 {
                let plan = day_plan(c, day, |_, _| false);
                assert!(!plan.is_empty());
                assert!(plan.windows(2).all(|w| w[0].at < w[1].at), "{c:?}: шаги не по порядку");
                let rainy = day_plan(c, day, |_, _| true);
                assert!(!rainy.is_empty());
            }
        }
    }

    #[test]
    fn comes_holds_audience_on_the_throne() {
        let plan = day_plan(Courtier::Comes, 3, |_, _| false);
        let at10 = plan.iter().rev().find(|s| s.at <= 10.0).unwrap();
        assert_eq!(at10.place, Where::Seat(Seat::Throne));
        assert_eq!(at10.act, Act::Sit);
        // В воскресенье приёма нет.
        let sunday = day_plan(Courtier::Comes, 6, |_, _| false);
        assert!(!sunday.iter().any(|s| s.place == Where::Seat(Seat::Throne)));
    }

    #[test]
    fn everyone_speaks() {
        for c in Courtier::ALL {
            for (h, met, act) in [(7.0, 0, Act::Idle), (10.0, 1, Act::Sit), (14.0, 3, Act::Write), (18.0, 9, Act::Idle), (22.0, 2, Act::Idle)] {
                assert!(!greeting(c, &ctx(h, met, act)).is_empty());
                assert!(!farewell(c, &ctx(h, met, act)).is_empty());
            }
            assert!(!topics(c).is_empty(), "{c:?} без вопросов");
            for tp in topics(c) {
                assert!(!tp.ask.is_empty());
            }
            assert!(!news(c, 5).is_empty());
            assert!(!tired(c).is_empty());
        }
    }

    #[test]
    fn audience_topics_only_on_the_throne() {
        let petition = COMES.iter().find(|t| t.learn == Some(Flag::Petition)).unwrap();
        let none = |_: Flag| false;
        assert!(open(petition, &ctx(10.0, 1, Act::Sit), &none));
        assert!(!open(petition, &ctx(15.0, 1, Act::Idle), &none));
        assert!(!open(petition, &Ctx { day: 6, ..ctx(10.0, 1, Act::Sit) }, &none));
    }

    #[test]
    fn secrets_unlock_follow_ups() {
        let letter = COMES.iter().find(|t| t.when == When::Knows(Flag::Letter)).unwrap();
        assert!(!open(letter, &ctx(14.0, 2, Act::Idle), &|_| false));
        assert!(open(letter, &ctx(14.0, 2, Act::Idle), &|f| f == Flag::Letter));
        assert!(CHARTULARIUS.iter().any(|t| t.learn == Some(Flag::Letter)));
    }

    #[test]
    fn overheard_lines_belong_to_the_pair() {
        for a in Courtier::ALL {
            for b in Courtier::ALL {
                if let Some(lines) = overheard(a, b) {
                    assert!(lines.len() >= 4);
                    assert!(lines.iter().all(|(who, l)| (*who == a || *who == b) && !l.is_empty()));
                }
            }
        }
    }

    #[test]
    fn talk_partners_meet_at_paired_seats() {
        // Кто с кем говорит — тот в это время говорит с ним же.
        for c in Courtier::ALL {
            for day in 0..7 {
                for st in day_plan(c, day, |_, _| false) {
                    if let Where::Talk(_, other) = st.place {
                        let theirs = day_plan(other, day, |_, _| false);
                        let at = theirs.iter().rev().find(|s| s.at <= st.at + 0.5).unwrap();
                        assert!(matches!(at.place, Where::Talk(_, w) if w == c), "{c:?} ждёт {other:?} в {} ч, день {day}", st.at);
                    }
                }
            }
        }
    }
}
