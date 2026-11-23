//! Работа руками: вместо полоски, которая сама ползёт, — движения игрока.
//!
//! Каждое долгое занятие (чтение, сбор трав, лечение, варка, урок, смена у мастера) идёт
//! только пока игрок работает: растирает пестиком, мешает, качает, бьёт в такт, ведёт
//! строку, держит натяжение, повторяет за мастером. Как сделано — так и выйдет.
//!
//! Клиент считает каждое движение ([`Meter`]) и шлёт серверу «мазок» с качеством 0..1;
//! сервер ([`Effort`]) не верит частым мазкам и сам решает, сколько дела сделано.
//! Сам модуль чистый: без Bevy-систем и сети, его покрывают тесты.

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

/// Каким движением делается работа.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Drill {
    /// Растирать пестиком в ступке: ровные круги мышью, не частить.
    Grind,
    /// Мешать в горшке: круги мышью, медленнее; быстро — расплещете.
    Stir,
    /// Качать мехи, копать, таскать, стирать: мышь вверх-вниз на полный ход, в такт.
    Pump,
    /// Бить, рубить, щипать в такт: ЛКМ, когда маятник посередине.
    Rhythm,
    /// Читать: вести указку (мышь влево-вправо) за словом, которое читаете.
    Track,
    /// Тянуть, бинтовать, вынимать: держать ЛКМ, чтобы натяжение было в нужной зоне.
    Tension,
    /// Урок: мастер показывает движения — повторите их мышью.
    Mimic,
}

impl Drill {
    #[allow(dead_code)] // в тестах
    pub const ALL: [Drill; 7] = [Drill::Grind, Drill::Stir, Drill::Pump, Drill::Rhythm, Drill::Track, Drill::Tension, Drill::Mimic];

    /// Сколько секунд в среднем между мазками у умелого работника.
    pub fn period(self) -> f32 {
        match self {
            Drill::Grind => 0.9,
            Drill::Stir => 1.6,
            Drill::Pump => 1.1,
            Drill::Rhythm => 0.6,
            Drill::Track | Drill::Tension => WINDOW,
            Drill::Mimic => 3.2,
        }
    }

    /// Подсказка, как работать.
    pub fn hint(self) -> &'static str {
        match self {
            Drill::Grind => "Водите мышью по кругу — ровно и не частя",
            Drill::Stir => "Мешайте мышью по кругу — медленно, не расплескайте",
            Drill::Pump => "Мышь вверх-вниз на полный ход, в такт",
            Drill::Rhythm => "ЛКМ, когда отметка посередине",
            Drill::Track => "Ведите указку мышью за словом",
            Drill::Tension => "Держите ЛКМ — натяжение в светлой зоне, не порвите",
            Drill::Mimic => "Смотрите, что показывает мастер, и повторите мышью",
        }
    }
}

/// Работа идёт порциями: окно непрерывных занятий (чтение, натяжение) — полсекунды.
pub const WINDOW: f32 = 0.5;
/// Мазок хуже этого не засчитывается в дело (но портит среднее качество).
pub const MISS: f32 = 0.15;

// ---------------------------------------------------------------------------
// Сервер: сколько дела сделано
// ---------------------------------------------------------------------------

/// Сделанная работа на сервере.
#[derive(Clone, Debug, PartialEq)]
pub struct Effort {
    pub drill: Drill,
    /// Сколько «единиц дела» нужно.
    pub need: f32,
    pub done: f32,
    q_sum: f32,
    n: u32,
    /// Секунд с последнего принятого мазка.
    since: f32,
}

impl Effort {
    /// `secs` — сколько длилось бы занятие у умелого работника.
    pub fn new(drill: Drill, secs: f32) -> Self {
        Effort { drill, need: (secs / drill.period() * 0.8).max(2.0), done: 0.0, q_sum: 0.0, n: 0, since: f32::MAX / 4.0 }
    }

    pub fn tick(&mut self, dt: f32) {
        self.since += dt;
    }

    /// Мазок с качеством `q`. false — слишком часто (подделка или дребезг), не принят.
    pub fn stroke(&mut self, q: f32) -> bool {
        if self.since < self.drill.period() * 0.45 || !q.is_finite() {
            return false;
        }
        let q = q.clamp(0.0, 1.0);
        self.since = 0.0;
        self.n += 1;
        self.q_sum += q;
        self.done += credit(q);
        true
    }

    /// Доля сделанного 0..1.
    pub fn frac(&self) -> f32 {
        (self.done / self.need).clamp(0.0, 1.0)
    }

    pub fn finished(&self) -> bool {
        self.done >= self.need
    }

    /// Среднее качество работы 0..1 (без единого мазка — посредственно).
    pub fn quality(&self) -> f32 {
        if self.n == 0 { 0.5 } else { self.q_sum / self.n as f32 }
    }

    /// Сколько секунд игрок ничего не делал.
    pub fn idle(&self) -> f32 {
        self.since
    }
}

/// Сколько дела даёт мазок качества `q`.
pub fn credit(q: f32) -> f32 {
    if q < MISS { 0.0 } else { 0.3 + 0.7 * q }
}

/// Слово о качестве работы (для сообщений).
pub fn quality_word(q: f32) -> &'static str {
    match q {
        q if q >= 0.85 => "чисто, как у мастера",
        q if q >= 0.65 => "добротно",
        q if q >= 0.45 => "сносно",
        q if q >= 0.25 => "неряшливо",
        _ => "из рук вон плохо",
    }
}

/// Множитель часов урока от прилежания: 0.5..1.3.
pub fn lesson_mult(q: f32) -> f32 {
    0.5 + 0.8 * q.clamp(0.0, 1.0)
}

/// Множитель платы подмастерью: 0.5..1.2 — мастер видит, как работали.
pub fn wage_mult(q: f32) -> f32 {
    0.5 + 0.7 * q.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Клиент: разбор движений
// ---------------------------------------------------------------------------

/// Ввод за кадр.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub dt: f32,
    /// Сдвиг мыши в пикселях (y вниз, как в Bevy).
    pub delta: Vec2,
    pub lmb_pressed: bool,
    pub lmb_held: bool,
}

/// Что случилось за кадр.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub q: f32,
    /// Короткое слово для игрока («ровно», «частите», «порвали»).
    pub note: &'static str,
}

/// Чувствительность: пикселей мыши на единицу хода.
const SENS: f32 = 1.0 / 160.0;

/// Насколько `v` близко к `ideal` в разах: 1 — точно, 0 — в `ratio` раз и дальше.
fn closeness_ratio(v: f32, ideal: f32, ratio: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    (1.0 - (v / ideal).ln().abs() / ratio.ln()).clamp(0.0, 1.0)
}

/// Умелые руки сглаживают огрехи: при навыке 1 плохой мазок выходит заметно лучше.
fn with_skill(q: f32, skill: f32) -> f32 {
    q.clamp(0.0, 1.0).powf(1.3 - 0.6 * skill.clamp(0.0, 1.0))
}

/// Направление для повторения за мастером.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub fn arrow(self) -> &'static str {
        match self {
            Dir::Up => "↑",
            Dir::Down => "↓",
            Dir::Left => "←",
            Dir::Right => "→",
        }
    }

    /// Единичный вектор (y вверх).
    pub fn vec(self) -> Vec2 {
        match self {
            Dir::Up => Vec2::Y,
            Dir::Down => Vec2::NEG_Y,
            Dir::Left => Vec2::NEG_X,
            Dir::Right => Vec2::X,
        }
    }

    fn from_index(i: u32) -> Dir {
        [Dir::Up, Dir::Down, Dir::Left, Dir::Right][(i % 4) as usize]
    }
}

/// Состояние движения на клиенте.
#[derive(Clone, Debug)]
pub struct Meter {
    pub drill: Drill,
    pub skill: f32,
    /// Время с начала.
    pub t: f32,
    /// Круги: точка руки в круге радиуса ~1 (y вверх). Качание: `pos.y` −1..1.
    pub pos: Vec2,
    angle: Option<f32>,
    turned: f32,
    rev_t: f32,
    rad_sum: f32,
    rad_n: u32,
    /// Качание: дошли до низа и ждём верха.
    pump_low: bool,
    /// Ритм: фаза маятника (с); чтение: где слово и указка; натяжение: сила.
    pub phase: f32,
    pub target: f32,
    pub cursor: f32,
    pub tension: f32,
    /// Чтение: какая строка по счёту и сколько указка стоит без движения.
    pub line: u32,
    still_t: f32,
    window_t: f32,
    on_t: f32,
    snapped: bool,
    /// Урок: что показал мастер, сколько уже повторено, верно ли.
    pub seq: Vec<Dir>,
    pub seq_i: usize,
    pub seq_ok: Vec<bool>,
    pub showing: bool,
    show_t: f32,
    flick: Vec2,
    flick_cool: f32,
    rounds: u32,
    rng: u32,
    /// Последний мазок — для подсказки на экране.
    pub last: Option<Stroke>,
    pub last_t: f32,
}

impl Meter {
    pub fn new(drill: Drill, skill: f32, seed: u32) -> Self {
        let mut m = Meter {
            drill,
            skill: skill.clamp(0.0, 1.0),
            t: 0.0,
            pos: Vec2::ZERO,
            angle: None,
            turned: 0.0,
            rev_t: 0.0,
            rad_sum: 0.0,
            rad_n: 0,
            pump_low: false,
            phase: 0.0,
            target: 0.05,
            cursor: 0.05,
            tension: 0.0,
            line: 0,
            still_t: 1.0,
            window_t: 0.0,
            on_t: 0.0,
            snapped: false,
            seq: Vec::new(),
            seq_i: 0,
            seq_ok: Vec::new(),
            showing: false,
            show_t: 0.0,
            flick: Vec2::ZERO,
            flick_cool: 0.0,
            rounds: 0,
            rng: seed | 1,
            last: None,
            last_t: -10.0,
        };
        if drill == Drill::Mimic {
            m.new_sequence();
        }
        m
    }

    fn rand(&mut self) -> u32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// Ширина хорошей зоны (ритм, натяжение) — растёт с навыком.
    pub fn zone(&self) -> f32 {
        match self.drill {
            Drill::Rhythm => 0.28 + 0.3 * self.skill,
            Drill::Tension => 0.09 + 0.06 * self.skill,
            Drill::Track => 0.06 + 0.04 * self.skill,
            _ => 0.0,
        }
    }

    /// Центр зоны натяжения: плывёт, как тянется ткань или поддаётся корень.
    pub fn tension_center(&self) -> f32 {
        0.55 + 0.18 * (self.t * 0.9).sin() + 0.07 * (self.t * 2.3 + 1.0).sin()
    }

    /// Положение маятника −1..1 (0 — середина).
    pub fn swing(&self) -> f32 {
        (self.phase / (2.0 * self.drill.period()) * PI).cos()
    }

    /// Скорость слова при чтении (строк в секунду).
    fn read_speed(&self) -> f32 {
        0.11 + 0.04 * self.skill
    }

    fn new_sequence(&mut self) {
        let len = (3 + self.rounds / 2).min(5) as usize;
        let mut seq: Vec<Dir> = Vec::with_capacity(len);
        while seq.len() < len {
            let d = Dir::from_index(self.rand() >> 7);
            // Не повторять одно и то же подряд — иначе непонятно, сколько раз махнуть.
            if seq.last() != Some(&d) {
                seq.push(d);
            }
        }
        self.seq = seq;
        self.seq_i = 0;
        self.seq_ok.clear();
        self.showing = true;
        self.show_t = 0.0;
        self.flick = Vec2::ZERO;
    }

    /// Сколько показывать одно движение мастера.
    pub const SHOW_STEP: f32 = 0.6;

    /// Какое движение мастер показывает сейчас (во время показа).
    pub fn shown(&self) -> Option<usize> {
        if !self.showing {
            return None;
        }
        let i = (self.show_t / Self::SHOW_STEP) as usize;
        (i < self.seq.len()).then_some(i)
    }

    /// Прошёл кадр. Возвращает мазок, если движение завершено.
    pub fn update(&mut self, inp: Input) -> Option<Stroke> {
        let dt = inp.dt.clamp(0.0, 0.1);
        self.t += dt;
        let s = match self.drill {
            Drill::Grind | Drill::Stir => self.circle(inp, dt),
            Drill::Pump => self.pump(inp, dt),
            Drill::Rhythm => self.rhythm(inp, dt),
            Drill::Track => self.track(inp, dt),
            Drill::Tension => self.pull(inp, dt),
            Drill::Mimic => self.mimic(inp, dt),
        };
        if let Some(s) = &s {
            self.last = Some(s.clone());
            self.last_t = self.t;
        }
        s
    }

    fn circle(&mut self, inp: Input, dt: f32) -> Option<Stroke> {
        let d = Vec2::new(inp.delta.x, -inp.delta.y) * SENS;
        self.pos = (self.pos + d).clamp_length_max(1.25);
        // Без движения рука сама сползает к середине.
        if d.length_squared() < 1e-8 {
            self.pos *= 1.0 - (dt * 1.5).min(1.0);
        }
        self.rev_t += dt;
        let r = self.pos.length();
        if r < 0.3 {
            self.angle = None;
            return None;
        }
        self.rad_sum += r;
        self.rad_n += 1;
        let a = self.pos.y.atan2(self.pos.x);
        if let Some(prev) = self.angle {
            let mut da = a - prev;
            if da > PI {
                da -= TAU;
            } else if da < -PI {
                da += TAU;
            }
            // Поворот в обратную сторону сбивает счёт оборота.
            if self.turned != 0.0 && da.signum() != self.turned.signum() && da.abs() > 0.05 {
                self.turned = 0.0;
                self.rev_t = 0.0;
                self.rad_sum = 0.0;
                self.rad_n = 0;
            }
            self.turned += da;
        }
        self.angle = Some(a);
        if self.turned.abs() < TAU {
            return None;
        }
        let ideal = self.drill.period();
        let speed = closeness_ratio(self.rev_t, ideal, 2.6);
        let radius = self.rad_sum / self.rad_n.max(1) as f32;
        let round = (1.0 - (radius - 0.9).abs() / 0.6).clamp(0.0, 1.0);
        let q = with_skill(0.65 * speed + 0.35 * round, self.skill);
        let note = if self.rev_t < ideal * 0.5 {
            if self.drill == Drill::Stir { "расплёскиваете — медленнее" } else { "частите — медленнее" }
        } else if self.rev_t > ideal * 2.0 {
            "слишком вяло"
        } else if round < 0.5 {
            "круг неровный — шире и ровнее"
        } else {
            "ровно"
        };
        self.turned -= TAU * self.turned.signum();
        self.rev_t = 0.0;
        self.rad_sum = 0.0;
        self.rad_n = 0;
        Some(Stroke { q, note })
    }

    fn pump(&mut self, inp: Input, _dt: f32) -> Option<Stroke> {
        self.pos.y = (self.pos.y - inp.delta.y * SENS * 1.4).clamp(-1.0, 1.0);
        self.rev_t += inp.dt;
        if self.pos.y < -0.85 {
            self.pump_low = true;
        }
        if !(self.pump_low && self.pos.y > 0.85) {
            return None;
        }
        self.pump_low = false;
        let ideal = self.drill.period();
        let q = with_skill(closeness_ratio(self.rev_t, ideal, 2.4), self.skill);
        let note = if self.rev_t < ideal * 0.55 {
            "дёргаете — ровнее"
        } else if self.rev_t > ideal * 1.9 {
            "слишком медленно"
        } else {
            "в такт"
        };
        self.rev_t = 0.0;
        Some(Stroke { q, note })
    }

    fn rhythm(&mut self, inp: Input, dt: f32) -> Option<Stroke> {
        self.phase += dt;
        if !inp.lmb_pressed {
            return None;
        }
        let pos = self.swing();
        let zone = self.zone();
        let raw = if pos.abs() <= zone { 1.0 - 0.35 * pos.abs() / zone } else { (0.65 * (1.0 - (pos.abs() - zone) / (1.0 - zone).max(0.05))).max(0.0) };
        // Маятник начинает новый замах с края.
        self.phase = 0.0;
        let q = with_skill(raw, self.skill);
        let note = if pos.abs() <= zone { "точно" } else if pos.abs() < 0.8 { "чуть мимо такта" } else { "мимо такта" };
        Some(Stroke { q, note })
    }

    fn track(&mut self, inp: Input, dt: f32) -> Option<Stroke> {
        self.cursor = (self.cursor + inp.delta.x * SENS * 0.5).clamp(0.0, 1.0);
        // Слово идёт по строке, на длинных словах задерживается.
        let wobble = 0.6 + 0.4 * (self.t * 1.7).sin().abs();
        self.target += self.read_speed() * wobble * dt;
        if self.target > 0.97 {
            // Новая строка: глаз прыгает в начало.
            self.target = 0.03;
            self.line += 1;
        }
        if inp.delta.x.abs() > 0.0 {
            self.still_t = 0.0;
        } else {
            self.still_t += dt;
        }
        // Читает тот, кто ведёт указку: стоящая указка не читает, даже если слово под ней.
        if (self.cursor - self.target).abs() <= self.zone() && self.still_t < 0.4 {
            self.on_t += dt;
        }
        self.window(dt, "строка ушла — догоняйте", "читаете ровно")
    }

    fn pull(&mut self, inp: Input, dt: f32) -> Option<Stroke> {
        if inp.lmb_held {
            self.tension += 0.85 * dt;
        } else {
            self.tension -= 0.65 * dt;
        }
        self.tension = self.tension.max(0.0);
        if self.tension > 1.0 {
            self.snapped = true;
            self.tension = 0.25;
        }
        if (self.tension - self.tension_center()).abs() <= self.zone() {
            self.on_t += dt;
        }
        if self.snapped {
            self.window_t += dt;
            if self.window_t >= WINDOW {
                self.window_t = 0.0;
                self.on_t = 0.0;
                self.snapped = false;
                return Some(Stroke { q: 0.0, note: "перетянули!" });
            }
            return None;
        }
        self.window(dt, "то слабо, то туго", "ровное натяжение")
    }

    /// Непрерывные занятия: раз в полсекунды — доля времени «в зоне».
    fn window(&mut self, dt: f32, bad: &'static str, good: &'static str) -> Option<Stroke> {
        self.window_t += dt;
        if self.window_t < WINDOW {
            return None;
        }
        let frac = (self.on_t / self.window_t).clamp(0.0, 1.0);
        self.window_t = 0.0;
        self.on_t = 0.0;
        let q = with_skill(frac, self.skill);
        Some(Stroke { q, note: if frac < 0.5 { bad } else { good } })
    }

    fn mimic(&mut self, inp: Input, dt: f32) -> Option<Stroke> {
        if self.showing {
            self.show_t += dt;
            if self.show_t >= Self::SHOW_STEP * self.seq.len() as f32 + 0.3 {
                self.showing = false;
                self.show_t = 0.0;
                self.flick = Vec2::ZERO;
            }
            return None;
        }
        self.show_t += dt;
        self.flick_cool -= dt;
        if self.flick_cool > 0.0 {
            return None;
        }
        self.flick += Vec2::new(inp.delta.x, -inp.delta.y);
        // Медленный дрейф мыши не считается движением.
        self.flick *= 1.0 - (dt * 3.0).min(1.0);
        if self.flick.length() >= 70.0 {
            let d = if self.flick.x.abs() > self.flick.y.abs() {
                if self.flick.x > 0.0 { Dir::Right } else { Dir::Left }
            } else if self.flick.y > 0.0 {
                Dir::Up
            } else {
                Dir::Down
            };
            self.seq_ok.push(self.seq.get(self.seq_i) == Some(&d));
            self.seq_i += 1;
            self.flick = Vec2::ZERO;
            self.flick_cool = 0.22;
        }
        let len = self.seq.len();
        let timeout = self.show_t > 1.2 * len as f32 + 1.5;
        if self.seq_i < len && !timeout {
            return None;
        }
        let right = self.seq_ok.iter().filter(|ok| **ok).count() as f32;
        let mut q = right / len as f32;
        if timeout {
            q *= 0.5;
        }
        let q = with_skill(q, self.skill);
        let note = if timeout {
            "мастер не дождался"
        } else if right as usize == len {
            "мастер кивает"
        } else if right as usize * 2 >= len {
            "почти — мастер поправляет руку"
        } else {
            "не так — мастер хмурится"
        };
        self.rounds += 1;
        self.new_sequence();
        Some(Stroke { q, note })
    }
}

// ---------------------------------------------------------------------------
// Какое занятие каким движением
// ---------------------------------------------------------------------------

/// Чем занят подмастерье у мастера (по ремеслу) и каким движением.
pub fn chore_drill(teacher: crate::lore::Teacher) -> Drill {
    use crate::lore::Teacher as T;
    match teacher {
        // Носить воду, стирать повязки — качать вверх-вниз.
        T::Iatros => Drill::Pump,
        // Обрывать листья, вязать пучки — щипок в такт.
        T::Rhizotomos | T::Wisewoman | T::SteppeHealer => Drill::Rhythm,
        // Толочь корни, растирать чернила.
        T::Apothecary | T::Scribe | T::Grammarian => Drill::Grind,
        // Топить печь, качать мехи, носить дрова.
        T::Alchemist | T::Glassblower | T::Potter => Drill::Pump,
        // Мешать чан.
        T::Dyer => Drill::Stir,
        // Бить кувалдой по указке мастера.
        T::Smith | T::Armourer | T::GothSmith | T::SteppeSmith => Drill::Rhythm,
    }
}

/// Движение для варки в сосуде.
pub fn brew_drill(vessel: crate::brewing::Vessel, heat: crate::brewing::Heat) -> Drill {
    use crate::brewing::{Heat, Vessel};
    match vessel {
        Vessel::Mortar => Drill::Grind,
        // Перегонку и обжиг ведут огнём: мехами.
        v if v.still() => Drill::Pump,
        Vessel::Crucible => Drill::Pump,
        // Решётку ворошат, горшки мешают (и на огне, и при настаивании — встряхивают).
        _ if heat == Heat::None => Drill::Stir,
        _ => Drill::Stir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(m: &mut Meter, frames: usize, dt: f32, mut f: impl FnMut(f32) -> Input) -> Vec<Stroke> {
        let mut out = Vec::new();
        for i in 0..frames {
            let mut inp = f(i as f32 * dt);
            inp.dt = dt;
            if let Some(s) = m.update(inp) {
                out.push(s);
            }
        }
        out
    }

    /// Мышь водит по кругу радиуса ~0.9 с оборотом за `rev` секунд.
    fn circling(rev: f32) -> impl FnMut(f32) -> Input {
        let mut prev = Vec2::new(0.9, 0.0);
        let mut first = true;
        move |t| {
            let a = t / rev * TAU;
            let p = Vec2::new(a.cos(), a.sin()) * 0.9;
            let d = if first {
                first = false;
                p
            } else {
                p - prev
            };
            prev = p;
            Input { delta: Vec2::new(d.x, -d.y) / SENS, ..default_input() }
        }
    }

    fn default_input() -> Input {
        Input::default()
    }

    #[test]
    fn steady_grinding_is_good_and_frantic_is_bad() {
        let mut m = Meter::new(Drill::Grind, 0.0, 7);
        let good = feed(&mut m, 600, 1.0 / 60.0, circling(0.9));
        assert!(good.len() >= 9, "оборотов: {}", good.len());
        let avg: f32 = good.iter().skip(1).map(|s| s.q).sum::<f32>() / (good.len() - 1) as f32;
        assert!(avg > 0.8, "ровное растирание {avg}");

        let mut m = Meter::new(Drill::Grind, 0.0, 7);
        let fast = feed(&mut m, 600, 1.0 / 60.0, circling(0.2));
        let avg_fast: f32 = fast.iter().skip(1).map(|s| s.q).sum::<f32>() / (fast.len() - 1) as f32;
        assert!(avg_fast < 0.45, "частят {avg_fast}");
    }

    #[test]
    fn no_motion_no_strokes() {
        for d in Drill::ALL {
            let mut m = Meter::new(d, 0.5, 3);
            let s = feed(&mut m, 600, 1.0 / 60.0, |_| Input::default());
            let useful = s.iter().filter(|s| s.q >= MISS).count();
            assert_eq!(useful, 0, "{d:?} без движения даёт дело");
        }
    }

    #[test]
    fn pumping_in_time() {
        let mut m = Meter::new(Drill::Pump, 0.0, 1);
        // Полный ход (2 единицы вниз и вверх) за 1.1 с.
        let s = feed(&mut m, 660, 1.0 / 60.0, |t| {
            let v = (t / 1.1 * TAU).cos() * 2.4 / 1.1 * TAU / 60.0;
            Input { delta: Vec2::new(0.0, v / (SENS * 1.4)), ..Input::default() }
        });
        assert!(s.len() >= 8, "качков {}", s.len());
        assert!(s.iter().skip(1).all(|s| s.q > 0.75), "{s:?}");
    }

    #[test]
    fn rhythm_rewards_the_middle() {
        let mut m = Meter::new(Drill::Rhythm, 0.0, 1);
        let p = Drill::Rhythm.period();
        // Нажимаем ровно в середине каждый раз.
        let mut since = 0.0;
        let s = feed(&mut m, 300, 1.0 / 100.0, |_| {
            since += 0.01;
            let press = since >= p - 1e-4;
            if press {
                since = 0.0;
            }
            Input { lmb_pressed: press, ..Input::default() }
        });
        assert!(!s.is_empty());
        assert!(s.iter().all(|s| s.q > 0.9), "{s:?}");
        // Нажатие на краю — плохо.
        let mut m = Meter::new(Drill::Rhythm, 0.0, 1);
        let s = m.update(Input { dt: 0.01, lmb_pressed: true, ..Input::default() }).unwrap();
        assert!(s.q < 0.2, "{s:?}");
    }

    #[test]
    fn reading_follows_the_word() {
        let mut m = Meter::new(Drill::Track, 0.0, 1);
        let mut out = Vec::new();
        for _ in 0..600 {
            let want = m.target - m.cursor;
            let inp = Input { dt: 1.0 / 60.0, delta: Vec2::new(want / (SENS * 0.5), 0.0), ..Input::default() };
            if let Some(s) = m.update(inp) {
                out.push(s.q);
            }
        }
        let avg = out.iter().sum::<f32>() / out.len() as f32;
        assert!(avg > 0.8, "{avg}");
    }

    #[test]
    fn tension_held_too_long_snaps() {
        let mut m = Meter::new(Drill::Tension, 0.0, 1);
        let s = feed(&mut m, 300, 1.0 / 60.0, |_| Input { lmb_held: true, ..Input::default() });
        assert!(s.iter().any(|s| s.note == "перетянули!"));
        // Держать по зоне — хорошо.
        let mut m = Meter::new(Drill::Tension, 0.0, 1);
        let mut out = Vec::new();
        for _ in 0..600 {
            let hold = m.tension < m.tension_center();
            if let Some(s) = m.update(Input { dt: 1.0 / 60.0, lmb_held: hold, ..Input::default() }) {
                out.push(s.q);
            }
        }
        let avg = out.iter().skip(2).sum::<f32>() / (out.len() - 2) as f32;
        assert!(avg > 0.75, "{avg}");
    }

    #[test]
    fn mimic_repeat_master() {
        let mut m = Meter::new(Drill::Mimic, 0.0, 99);
        // Ждём конца показа.
        while m.showing {
            m.update(Input { dt: 0.05, ..Input::default() });
        }
        let seq = m.seq.clone();
        let mut got = None;
        for d in seq {
            // Пауза между движениями, затем рывок.
            for _ in 0..15 {
                m.update(Input { dt: 1.0 / 60.0, ..Input::default() });
            }
            let v = d.vec() * 100.0;
            got = m.update(Input { dt: 1.0 / 60.0, delta: Vec2::new(v.x, -v.y), ..Input::default() }).or(got);
        }
        let s = got.expect("урок засчитан");
        assert!(s.q > 0.95, "{s:?}");
        assert!(m.showing, "мастер показывает новое");
    }

    #[test]
    fn effort_rejects_spam_and_counts_quality() {
        let mut e = Effort::new(Drill::Rhythm, 6.0);
        assert!(e.stroke(1.0));
        assert!(!e.stroke(1.0), "слишком часто");
        let mut t = 0.0;
        while !e.finished() {
            e.tick(0.6);
            t += 0.6;
            e.stroke(1.0);
            assert!(t < 60.0);
        }
        // Умелый справляется быстрее заявленного.
        assert!(t <= 6.0, "{t}");
        assert!(e.quality() > 0.95);
        // Неумеха с мазками ниже порога не продвигается вовсе.
        let mut e = Effort::new(Drill::Pump, 5.0);
        for _ in 0..50 {
            e.tick(2.0);
            e.stroke(0.1);
        }
        assert_eq!(e.frac(), 0.0);
        assert!(e.quality() < 0.2);
    }
}
