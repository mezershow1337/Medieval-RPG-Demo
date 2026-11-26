//! Журнал странника: «с чего начать» и как развивать каждый навык.
//!
//! Только направление, а не ответы: кто учит, где, от чего растёт навык, какие книги
//! бывают. Рецептов, доз и «куда прикладывать» здесь нет — это игрок узнаёт сам.
//! Учителей и книги журнал берёт из данных игры (лавки в `trade.rs`, книги в `lore.rs`),
//! поэтому новые мастера появятся в нём сами.

use crate::lore::{Book, Skill, Teacher};
use crate::skill_tree::{self, Perk};

/// Глава журнала.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chapter {
    Start,
    Controls,
    Skill(Skill),
    Law,
    Wounds,
}

impl Chapter {
    pub fn all() -> Vec<Chapter> {
        let mut v = vec![Chapter::Start, Chapter::Controls];
        v.extend(Skill::ALL.map(Chapter::Skill));
        v.extend([Chapter::Law, Chapter::Wounds]);
        v
    }

    pub fn title(self) -> String {
        match self {
            Chapter::Start => "С чего начать".into(),
            Chapter::Controls => "Управление".into(),
            Chapter::Skill(s) => s.name().into(),
            Chapter::Law => "Закон и стража".into(),
            Chapter::Wounds => "Раны и лечение".into(),
        }
    }

    /// Абзацы страницы. Строка, начинающаяся с «# », — подзаголовок.
    pub fn page(self) -> Vec<String> {
        match self {
            Chapter::Start => start_page(),
            Chapter::Controls => controls_page(),
            Chapter::Skill(s) => skill_page(s),
            Chapter::Law => law_page(),
            Chapter::Wounds => wounds_page(),
        }
    }
}

fn lines(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn start_page() -> Vec<String> {
    lines(&[
        "Херсон, 510 год. Вы приезжий: ни грамоты, ни ремесла, ни знакомых. Всё, что умеет персонаж, он узнаёт здесь — у мастеров, из книг и своими руками. Игра не подскажет верный рецепт или лечение: узнавайте, пробуйте, спрашивайте.",
        "# Первые шаги",
        "1. Осмотритесь на агоре. Там торговцы, Иоанн-книжник и казначей. Подойдите к торговцу и нажмите E.",
        "2. Заработайте. Подмастерьем у мастера (R) платят гроши, зато рука учится. Можно собирать за стенами и продавать, можно брать добычу с разбойников — но это опасно.",
        "3. Выучите грамоту. Почти всё знание — в книгах, а без грамоты книга — закорючки. Грамоте учат церковный писец в западной церкви и Иоанн-книжник на агоре.",
        "4. Выберите дело. Навык растёт от своего дела, умения приходят сами, когда набрано нужное. В книге навыков (K) у каждого умения видно, чего не хватает.",
        "5. Берегите себя. Кровотечение, переломы и яд сами не проходят. Сумка — Tab, рядом с ней окно «Тело».",
        "# Куда податься",
        "Лекарь: грамота, потом смены в ксеноне при базилике у ятроса Агапия, потом книги Гиппократа и Цельса.",
        "Травник: Харитон-ризотом у главных ворот учит узнавать травы; собирайте за стенами, варите у аптекаря Стефана на агоре.",
        "Кузнец: подмастерьем у кузнеца Филиппа — качать меха и бить кувалдой. Сила придёт вместе с ремеслом.",
        "Воин: разбойники и чужие отряды за стенами. Держите защиту и следите за выносливостью.",
        "Торговец: дешевле там, где товар свой. Рыба и вино — в порту, меха и ревень — в степи.",
        "Навыки связаны: кузнец крепнет, боец набирает силу и выносливость, лекарь лучше знает травы. Лучшие умения требуют сразу нескольких навыков.",
    ])
}

fn controls_page() -> Vec<String> {
    lines(&[
        "# Движение",
        "W A S D — идти, Shift — бежать, пробел — прыжок, C или Ctrl — пригнуться.",
        "# Бой",
        "Левая кнопка мыши — удар, правая (держать) — защита. Бег, удары и защита тратят выносливость: без неё защита ломается.",
        "# Мир",
        "E — говорить, взять, собрать, урок у мастера. R — работать подмастерьем. G — стащить с прилавка (если не заметят).",
        "T — предложить обмен игроку. J — отношения фракций и штрафы. B — окно кузни у горна.",
        "# Окна",
        "Tab или I — сумка и тело. K — книга навыков и знаний. F1 — этот журнал. M — карта. Esc — закрыть окно.",
        "В сумке: F или правая кнопка — использовать, Q — выбросить, Shift+клик — переложить.",
    ])
}

/// Где стоит место (название для журнала).
fn place_name(id: &str) -> String {
    crate::level::place(id).map(|p| p.name.to_string()).unwrap_or_else(|| match id {
        "church_west" => "Западная церковь".into(),
        "goth_village" => "Готское селение".into(),
        _ => id.replace('_', " "),
    })
}

/// Кто учит навыку: (мастер, где, чему: «урок» / «подмастерье»).
pub fn teachers(skill: Skill) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in crate::trade::MERCHANTS {
        let mut modes: Vec<&str> = Vec::new();
        for l in m.lessons {
            let t = crate::knowledge::teacher_from_trade(l.teacher);
            // Общественные работы (hours = 0) ничему не учат.
            if t.skill() != Some(skill) || l.hours <= 0.0 {
                continue;
            }
            let mode = match l.mode {
                crate::trade::LessonMode::Paid => "уроки",
                crate::trade::LessonMode::Apprentice => "работа подмастерьем",
            };
            if !modes.contains(&mode) {
                modes.push(mode);
            }
        }
        if !modes.is_empty() {
            out.push(format!("{} ({}, {}) — {}", m.name, m.title, place_name(m.place), modes.join(" и ")));
        }
    }
    // Мастера вне лавок (стоят сами по себе или живут в городе, см. knowledge.rs и school.rs).
    let schooling = [
        (Teacher::Grammatist, "didaskaleion", "школа с рассвета до полудня: азбука, чтение, счёт, письмо — дёшево"),
        (Teacher::Grammarian, "library", "письмо, грамматика и латынь — дорого"),
        (Teacher::Scribe, "church_west", "азбука, чтение и письмо по Псалтири"),
        (Teacher::Notary, "praetorium", "письмо и счёт"),
        (Teacher::Logograph, "agora", "письмо и грамматика"),
        (Teacher::Wisewoman, "goth_village", "уроки и работа подмастерьем"),
    ];
    for (teacher, place, what) in schooling {
        if teacher.skill() == Some(skill) {
            out.push(format!("{} ({}) — {}", teacher.name(), place_name(place), what));
        }
    }
    out
}

/// Какие книги касаются навыка (только названия и язык).
pub fn books(skill: Skill) -> Vec<String> {
    Book::ALL
        .into_iter()
        .filter(|b| match skill {
            // Для книжности годится любая книга.
            Skill::Letters => !matches!(b, Book::HerbalFragment),
            _ => b.teaches().iter().any(|l| l.skill() == Some(skill)) || (skill == Skill::Herbalism && *b == Book::HerbalFragment),
        })
        .map(|b| {
            let i = b.info();
            format!("{} ({}, {})", i.item, i.form, i.language.name())
        })
        .collect()
}

fn skill_page(skill: Skill) -> Vec<String> {
    let about = skill_tree::about(skill);
    let mut v = vec![format!("Даёт: {}", about.gives), format!("Растёт: {}", about.grows)];
    let spill = skill.spillover();
    if !spill.is_empty() {
        let names: Vec<&str> = spill.iter().map(|(s, _)| s.name()).collect();
        v.push(format!("Вместе с ним понемногу растёт: {}.", names.join(", ").to_lowercase()));
    }
    let from: Vec<&str> = Skill::ALL.iter().filter(|s| s.spillover().iter().any(|(t, _)| *t == skill)).map(|s| s.name()).collect();
    if !from.is_empty() {
        v.push(format!("Сам растёт и от дел: {}.", from.join(", ").to_lowercase()));
    }
    v.push("# Кто учит".into());
    let t = teachers(skill);
    if t.is_empty() {
        v.push(match skill {
            Skill::Strength | Skill::Endurance => "Учителя нет — учит работа и дорога.".into(),
            Skill::Arms | Skill::Archery => "Учителя нет — учит только настоящий бой. Начинайте с разбойников поодиночке, не с отряда.".into(),
            Skill::Trade => "Учителя нет — учат сделки. Сравнивайте цены в разных лавках и за стенами.".into(),
            _ => "Учителя пока не нашлось.".into(),
        });
    } else {
        v.extend(t);
        v.push("Урок за плату — быстрее всего, но дорого. Подмастерьем знание приходит медленнее, зато платят и рука набивается быстрее.".into());
    }
    let b = books(skill);
    if !b.is_empty() {
        v.push("# Книги".into());
        v.extend(b);
        v.push("Книги дают теорию, а не руку: прочитанное надо ещё сделать самому.".into());
    }
    v.push("# Умения".into());
    for p in Perk::ALL.iter().filter(|p| p.info().skill == skill) {
        let i = p.info();
        let needs: Vec<String> = i.needs.iter().map(|n| n.label()).collect();
        v.push(format!("{} — {} Нужно: {}.", i.name, i.effect, needs.join(", ")));
    }
    v
}

fn law_page() -> Vec<String> {
    lines(&[
        "В городе действует закон. Кража, драка и убийство — преступления, если кто-нибудь видел. Свидетель донесёт страже, и вас станут искать.",
        "Стражник остановит и предложит заплатить штраф, пойти в тюрьму или драться. Штрафы принимает казначей гарнизона — актуарий Маркиан в цитадели.",
        "Как вернуть доброе имя: уплаченный штраф прощает половину вражды с городом (−100 станет −50). Смена у мастера в городе даёт +10–15 к репутации, общественные работы у Маркиана — +10–20 у гарнизона.",
        "За стенами свои порядки: у готов, аланов и степняков — своя честь и своя месть. Отношения с ними видно по J.",
        "Краденое могут узнать при продаже. Скупщики берут его без вопросов, но за бесценок.",
        "Убитые торговцы и жители не возвращаются сразу: лавка закрыта, а наследник помнит обиду.",
    ])
}

fn wounds_page() -> Vec<String> {
    lines(&[
        "Окно «Тело» — в сумке (Tab). Там видно, где рана, что кровит и что сломано.",
        "Чем лечить и куда прикладывать — игра не подскажет. Это узнают у лекарей, из книг и на опыте. Неверное средство может навредить.",
        "Если не знаете сами — купите готовое у аптекаря или заплатите ятросу. Знахарка в готском селении учит без книг.",
        "Перелом сам не срастётся как надо: криво сросшаяся кость остаётся с вами надолго.",
        "Раненый быстрее устаёт и хуже держит удар. Не лезьте в новый бой, пока не перевязались.",
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chapter_has_text() {
        for c in Chapter::all() {
            assert!(!c.page().is_empty(), "{}", c.title());
        }
    }

    #[test]
    fn crafts_have_teachers_and_books() {
        for s in [Skill::Medicine, Skill::Herbalism, Skill::Alchemy, Skill::Smithing, Skill::Letters] {
            assert!(!teachers(s).is_empty(), "некому учить: {}", s.name());
            assert!(!books(s).is_empty(), "нет книг: {}", s.name());
        }
    }

    #[test]
    fn named_people_in_start_page_exist() {
        let text = start_page().join(" ") + &law_page().join(" ");
        for name in ["Иоанн-книжник", "Харитон-ризотом", "Стефан-аптекарь", "Филипп", "Ятрос Агапий", "Актуарий Маркиан"] {
            assert!(crate::trade::MERCHANTS.iter().any(|m| m.name == name), "нет такого торговца: {name}");
            let word = name.split(['-', ' ']).nth(if name.starts_with("Ятрос") || name.starts_with("Актуарий") { 1 } else { 0 }).unwrap();
            // Имя склоняется: сверяем основу.
            let stem: String = word.chars().take(4).collect();
            assert!(text.contains(&stem), "{stem}");
        }
    }

    #[test]
    fn guide_gives_no_recipes() {
        // Журнал не выдаёт доз и пропорций.
        for c in Chapter::all() {
            for line in c.page() {
                for bad in ["драхм", "котил", "1:4", "пропорц"] {
                    assert!(!line.contains(bad), "{}: {line}", c.title());
                }
            }
        }
    }
}
