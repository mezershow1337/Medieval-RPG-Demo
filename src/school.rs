//! Школа и учителя грамоты.
//!
//! Где учат читать, писать и считать в Херсоне 510 года:
//! - **дидаскалион** — городская школа: грамматист с утра до полудня учит в классе азбуке,
//!   чтению, счёту и письму (дёшево, но медленно: два десятка мальчишек), а грамматик в
//!   книгохранилище — письму, грамматике и латыни (дорого);
//! - **церковный писец** в западной церкви — азбука, чтение и письмо по Псалтири;
//! - **нотарий** в претории — письмо и счёт (договоры, подати);
//! - **сочинитель** на агоре — пишет письма и прошения неграмотным, учит слогу и грамматике;
//! - книгописец Иоанн и философ Олимпий учат через свои лавки (trade.rs).
//!
//! Школьные учителя — настоящие жители города из раскладки уровня (`Role::Scholar` в
//! дидаскалионе): они живут по распорядку (`daily_life.rs`), и урок возможен, только когда
//! учитель на месте и учит. Остальные стоят на своих местах в часы работы.
//!
//! Модуль общий для сервера и клиента: только данные и чистые функции.

use crate::level::Role;
use crate::lore::Teacher;
use bevy::math::Vec3;

/// Где и когда учит учитель грамоты.
#[derive(Clone, Debug, PartialEq)]
pub struct Tutor {
    pub teacher: Teacher,
    /// Ключ места (`level::places()`).
    pub place: &'static str,
    /// Житель, который учит (номер в `Layout::residents`); None — стоит на месте сам.
    pub resident: Option<usize>,
    pub pos: Vec3,
    pub yaw: f32,
    /// Плата за урок (в долях) и плата помощнику за смену.
    pub price: u32,
    pub wage: u32,
    /// Часы, когда учит (игровые, 0..24).
    pub hours: (f32, f32),
}

/// Плата за урок: в школе — медная монета с ученика (как у Диоклетиана: грамматисту платили
/// гроши), у частных наставников — несколько медных.
pub const SCHOOL_FEE: u32 = 10;

/// Житель для поиска учителей: роль, дом (точка на уровне) и взгляд.
#[derive(Clone, Copy, Debug)]
pub struct Townsman {
    pub role: Option<Role>,
    pub pos: Vec3,
    pub yaw: f32,
}

/// Учителя грамоты на уровне. `people` — жители по номерам (как `Layout::residents` или
/// `CivilWorld::people`). На уровне без города (замок) — пусто.
pub fn tutors(people: &[Townsman]) -> Vec<Tutor> {
    let mut out = Vec::new();
    if let Some(school) = crate::level::place("didaskaleion") {
        // Учителя дидаскалиона: тот, вокруг кого сидят ученики, — грамматист (класс),
        // остальные — грамматики в книгохранилище.
        let near = |p: Vec3, r: f32| (p - school.pos).length() < r;
        let scholars: Vec<usize> = people.iter().enumerate().filter(|(_, r)| r.role == Some(Role::Scholar) && near(r.pos, 30.0) && !crate::level::library::inside(r.pos)).map(|(i, _)| i).collect();
        let pupils = |i: usize| people.iter().filter(|r| r.role == Some(Role::Student) && (r.pos - people[i].pos).length() < 9.0).count();
        let class = scholars.iter().copied().max_by_key(|i| (pupils(*i), usize::MAX - *i));
        if let Some(c) = class {
            let r = &people[c];
            out.push(Tutor { teacher: Teacher::Grammatist, place: "didaskaleion", resident: Some(c), pos: r.pos, yaw: r.yaw, price: SCHOOL_FEE, wage: 10, hours: (6.5, 12.5) });
        }
        // Грамматик — учёный, что сидит у книгохранилища (ближайший к нему из прочих).
        let library = crate::level::place_pos("library").unwrap_or(school.pos);
        let by_library = |i: &usize| ((people[*i].pos - library).length() * 100.0) as u32;
        if let Some(g) = scholars.iter().copied().filter(|i| Some(*i) != class).min_by_key(by_library) {
            let r = &people[g];
            out.push(Tutor { teacher: Teacher::Grammarian, place: "library", resident: Some(g), pos: r.pos, yaw: r.yaw, price: 40 * 6, wage: 20, hours: (6.5, 17.0) });
        }
    }
    let post = |teacher, place: &'static str, offset: Vec3, price, wage, hours| {
        crate::level::place_pos(place).map(|p| Tutor { teacher, place, resident: None, pos: p + offset, yaw: 0.0, price, wage, hours })
    };
    out.extend(post(Teacher::Notary, "praetorium", Vec3::new(-2.5, 0.0, 1.5), 40 * 4, 40, (8.0, 16.0)));
    out.extend(post(Teacher::Logograph, "agora", Vec3::new(4.0, 0.0, -7.0), 40 * 3, 30, (8.0, 18.0)));
    out
}

/// Сейчас учит? Житель-учитель учит, пока занят своим делом на месте (не спит, не ест, не ушёл).
pub fn on_duty(hours: (f32, f32), hour: f32, act: Option<crate::daily_life::Act>) -> bool {
    use crate::daily_life::Act;
    let in_hours = hour >= hours.0 && hour < hours.1;
    match act {
        None => in_hours,
        Some(a) => in_hours && matches!(a, Act::Teach | Act::Write | Act::Work | Act::Idle | Act::Talk),
    }
}

/// «с 6:30 до 12:30».
pub fn hours_text(hours: (f32, f32)) -> String {
    let f = |h: f32| {
        let m = (h * 60.0).round() as u32;
        format!("{}:{:02}", m / 60, m % 60)
    };
    format!("с {} до {}", f(hours.0), f(hours.1))
}

/// Что говорят ученику, когда учителя нет на месте.
pub fn away_line(teacher: Teacher, hour: f32, hours: (f32, f32)) -> String {
    match teacher {
        Teacher::Grammatist if hour >= hours.1 || hour < 5.0 => "Школа кончилась. Приходи завтра с рассветом, к первому уроку.".into(),
        Teacher::Grammatist => "Учитель ещё не пришёл. Уроки — с рассвета до полудня.".into(),
        _ if hour < hours.0 => format!("Ещё рано. {} учит {}.", teacher.name(), hours_text(hours)),
        _ => format!("{} закончил на сегодня. Приходи {}.", teacher.name(), hours_text(hours)),
    }
}

/// Кто какой урок даёт: подпись для окна школы.
pub fn tutor_title(t: Teacher) -> &'static str {
    match t {
        Teacher::Grammatist => "Школа грамматиста",
        Teacher::Grammarian => "Грамматик",
        Teacher::Scribe => "Церковный писец",
        Teacher::Notary => "Нотарий претории",
        Teacher::Copyist => "Книгописец",
        Teacher::Philosopher => "Философ",
        Teacher::Logograph => "Сочинитель прошений",
        other => other.name(),
    }
}

/// Пара слов от учителя в начале урока.
pub fn tutor_greeting(t: Teacher) -> &'static str {
    match t {
        Teacher::Grammatist => "Садись на скамью, бери табличку. Сначала буквы, потом слоги, потом Псалтирь. Кто ленится — того розгой.",
        Teacher::Grammarian => "Буквы ты знаешь? Тогда начнём с Дионисия, а там и до Гомера с Вергилием дойдём.",
        Teacher::Scribe => "Псалтирь — лучшая азбука. Буква за буквой, псалом за псалмом.",
        Teacher::Notary => "Договор, завещание, расписка в податях — всё на письме и всё со счётом. Научу, если платишь.",
        Teacher::Logograph => "Половина города не умеет написать и пары слов, а прошение комиту надо подать. Учись — без хлеба не останешься.",
        Teacher::Copyist => "Рука должна быть твёрдой, а глаз — верным: одна пропущенная строка губит книгу.",
        Teacher::Philosopher => "Никомах говорит: число — начало всего. Но сначала научись складывать камешки.",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn school_has_its_teachers() {
        if crate::level::active() != crate::level::LevelKind::Chersonesus {
            return;
        }
        let residents = crate::level::layout().residents;
        let people: Vec<Townsman> = residents.iter().map(|r| Townsman { role: Some(r.role), pos: r.pos, yaw: r.yaw }).collect();
        let t = tutors(&people);
        let gram = t.iter().find(|t| t.teacher == Teacher::Grammatist).expect("в дидаскалионе есть грамматист");
        let i = gram.resident.expect("грамматист — житель");
        assert_eq!(residents[i].role, Role::Scholar);
        assert!(t.iter().any(|t| t.teacher == Teacher::Grammarian && t.resident.is_some()));
        assert!(t.iter().any(|t| t.teacher == Teacher::Notary));
        assert!(t.iter().any(|t| t.teacher == Teacher::Logograph));
        for tutor in &t {
            assert!(tutor.teacher.teaches_letters());
            assert!(tutor.hours.0 < tutor.hours.1);
        }
        // Школа дешевле частных уроков.
        assert!(t.iter().all(|x| x.price >= gram.price));
    }

    #[test]
    fn teachers_keep_hours() {
        use crate::daily_life::Act;
        let h = (6.5, 12.5);
        assert!(on_duty(h, 9.0, Some(Act::Teach)));
        assert!(!on_duty(h, 9.0, Some(Act::Sleep)));
        assert!(!on_duty(h, 13.0, Some(Act::Teach)));
        assert!(on_duty(h, 9.0, None));
        assert!(!on_duty(h, 3.0, None));
        assert_eq!(hours_text(h), "с 6:30 до 12:30");
    }
}
