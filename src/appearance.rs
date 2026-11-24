//! Облик человека: причёска, борода, цвет волос и кожи, телосложение и черты лица.
//!
//! Игрок выбирает облик в главном меню («Облик»), он хранится в settings.txt и едет на сервер
//! в профиле подключения (`estates::pack_profile`), а оттуда — всем в `Gear::look`. Жители, торговцы
//! и враги получают облик по своему номеру (`Look::random`): кто-то бородат, кто-то брит, у кого-то
//! усы; кожа, волосы и сложение — по народу (ромеи, готы, аланы, гунны, заморские купцы).
//!
//! Телосложение и черты лица — цели формы (морфы) в `assets/models/humans.glb`
//! (`tools/models/build_humans.py`, цели MakeHuman CC0): `thin`, `heavy`, `rugged`, `longnose`, `steppe`.

use crate::rig::{Beard, Hair};
use bevy::color::Color;
use serde::{Deserialize, Serialize};

/// Облик. `set == false` — не выбран (бот, старый клиент): берётся `Look::random` по номеру.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Look {
    pub set: bool,
    /// Номер в `HAIR_STYLES`.
    pub hair: u8,
    /// Номер в `BEARD_STYLES`.
    pub beard: u8,
    /// Номер в `HAIR_COLORS`.
    pub hair_color: u8,
    /// Номер в `SKIN_TONES`.
    pub skin: u8,
    /// Номер в `BUILDS`.
    pub build: u8,
    /// Номер в `FACES`.
    pub face: u8,
}

pub const HAIR_STYLES: [(Hair, &str); 5] = [
    (Hair::Short, "Короткая стрижка"),
    (Hair::Cropped, "Под машинку"),
    (Hair::Long, "Длинные волосы"),
    (Hair::Balding, "Лысеющий"),
    (Hair::Shaved, "Бритая голова"),
];

pub const BEARD_STYLES: [(Beard, &str); 5] = [
    (Beard::Short, "Короткая борода"),
    (Beard::Full, "Окладистая борода"),
    (Beard::Moustache, "Усы"),
    (Beard::Stubble, "Щетина"),
    (Beard::None, "Гладко выбрит"),
];

/// Цвет волос (оттенок текстуры со средней яркостью 1).
pub const HAIR_COLORS: [([f32; 3], &str); 9] = [
    ([0.065, 0.052, 0.045], "Чёрные"),
    ([0.13, 0.085, 0.055], "Тёмно-каштановые"),
    ([0.24, 0.155, 0.095], "Каштановые"),
    ([0.36, 0.2, 0.1], "Рыжевато-каштановые"),
    ([0.48, 0.36, 0.22], "Русые"),
    ([0.7, 0.56, 0.36], "Светлые"),
    ([0.52, 0.23, 0.1], "Рыжие"),
    ([0.42, 0.4, 0.38], "С проседью"),
    ([0.76, 0.74, 0.71], "Седые"),
];

/// Цвет кожи: множитель к текстуре (её основа ≈ srgb 0.80, 0.60, 0.48).
pub const SKIN_TONES: [([f32; 3], &str); 7] = [
    ([1.0, 1.0, 1.02], "Бледная"),
    ([0.97, 0.955, 0.96], "Светлая"),
    ([0.92, 0.88, 0.85], "Смуглая"),
    ([0.85, 0.79, 0.72], "Оливковая"),
    ([0.76, 0.68, 0.6], "Загорелая"),
    ([0.63, 0.55, 0.48], "Тёмная"),
    ([0.45, 0.38, 0.34], "Очень тёмная"),
];

/// Телосложение: (худоба, полнота/мощь) — веса целей формы `thin` и `heavy`.
pub const BUILDS: [((f32, f32), &str); 5] = [
    ((1.0, 0.0), "Худой"),
    ((0.5, 0.0), "Поджарый"),
    ((0.0, 0.0), "Обычный"),
    ((0.0, 0.5), "Крепкий"),
    ((0.0, 1.0), "Здоровяк"),
];

/// Черты лица: веса целей `rugged` (грубые/тонкие), `longnose` (длинный/короткий нос), `steppe`.
pub const FACES: [([f32; 3], &str); 6] = [
    ([0.0, 0.0, 0.0], "Обычное"),
    ([0.9, 0.0, 0.0], "Грубое, скуластое"),
    ([-0.7, 0.6, 0.0], "Тонкое, длинный нос"),
    ([0.3, 1.0, 0.0], "Горбоносое"),
    ([-0.6, -0.8, 0.0], "Мягкое, курносое"),
    ([0.3, -0.2, 1.0], "Степное"),
];

/// Веса целей формы модели (их имена — как в build_humans.py).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shape {
    pub thin: f32,
    pub heavy: f32,
    pub rugged: f32,
    pub longnose: f32,
    pub steppe: f32,
}

impl Shape {
    pub fn weight(&self, target: &str) -> f32 {
        match target {
            "thin" => self.thin,
            "heavy" => self.heavy,
            "rugged" => self.rugged,
            "longnose" => self.longnose,
            "steppe" => self.steppe,
            _ => 0.0,
        }
    }
}

/// Народ: от него зависят кожа, волосы и привычки в бороде.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folk {
    /// Ромеи Херсонеса: греки, смуглые и оливковые, тёмные волосы.
    Romaioi,
    /// Крымские готы: светлая кожа, русые и светлые волосы, длинные волосы, усы.
    Goths,
    /// Аланы: светлая и смуглая кожа, тёмные и русые волосы, бороды.
    Alans,
    /// Гунны и другие кочевники: степные черты, бритые головы, редкие бороды.
    Nomads,
    /// Заморские купцы и моряки (Сирия, Египет, Понт): от смуглых до очень тёмных.
    Overseas,
}

/// Детерминированная «случайность» по номеру и соли: 0..1.
fn rnd(seed: u64, salt: u64) -> f32 {
    let mut h = seed ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    h = (h ^ (h >> 32)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = (h ^ (h >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    ((h >> 40) % 10_000) as f32 / 10_000.0
}

/// Выбор по весам: `weights[i]` — относительная частота варианта i.
fn pick(r: f32, weights: &[f32]) -> u8 {
    let total: f32 = weights.iter().sum();
    let mut acc = 0.0;
    for (i, w) in weights.iter().enumerate() {
        acc += w / total;
        if r < acc {
            return i as u8;
        }
    }
    (weights.len() - 1) as u8
}

impl Look {
    /// Облик по умолчанию для нового игрока.
    pub const PLAYER_DEFAULT: Look = Look { set: true, hair: 0, beard: 0, hair_color: 2, skin: 2, build: 2, face: 0 };

    /// Облик человека по номеру и народу. Возраст (седина, лысина) тоже случайный.
    pub fn random(seed: u64, folk: Folk) -> Look {
        let r = |s| rnd(seed, s);
        // Порядок весов — как в HAIR_STYLES / BEARD_STYLES / HAIR_COLORS / SKIN_TONES / BUILDS / FACES.
        let (hair, beard, colors, skin, face): (&[f32], &[f32], &[f32], &[f32], &[f32]) = match folk {
            // Короткая, под машинку, длинные, лысеющий, бритый.
            // Короткая борода, окладистая, усы, щетина, выбрит.
            Folk::Romaioi => (&[5.0, 2.0, 0.6, 1.4, 0.4], &[3.0, 2.0, 0.8, 2.0, 2.2], &[3.0, 4.0, 3.0, 1.0, 0.6, 0.0, 0.2, 0.0, 0.0], &[0.2, 1.0, 3.0, 3.0, 2.0, 0.5, 0.0], &[3.0, 1.0, 2.0, 2.0, 1.0, 0.0]),
            Folk::Goths => (&[2.0, 0.5, 5.0, 0.8, 0.0], &[1.5, 2.0, 3.0, 1.0, 0.8], &[0.0, 0.5, 1.5, 2.0, 4.0, 3.0, 1.2, 0.0, 0.0], &[3.0, 3.0, 1.0, 0.2, 0.3, 0.0, 0.0], &[3.0, 2.0, 1.0, 0.5, 1.5, 0.0]),
            Folk::Alans => (&[3.0, 1.0, 2.0, 0.8, 0.5], &[2.0, 3.0, 2.0, 1.0, 0.5], &[1.5, 3.0, 2.5, 1.0, 1.5, 0.5, 0.3, 0.0, 0.0], &[0.8, 2.0, 2.5, 1.5, 1.5, 0.2, 0.0], &[2.0, 2.0, 1.0, 2.0, 0.5, 1.0]),
            Folk::Nomads => (&[0.5, 1.5, 1.0, 0.3, 4.0], &[1.0, 0.3, 3.0, 2.0, 2.0], &[5.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], &[0.0, 0.5, 2.0, 2.0, 3.0, 1.0, 0.0], &[0.3, 1.0, 0.0, 0.0, 0.3, 6.0]),
            Folk::Overseas => (&[4.0, 3.0, 1.0, 1.0, 1.0], &[2.5, 2.0, 1.5, 2.0, 1.5], &[5.0, 4.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], &[0.0, 0.0, 1.0, 2.0, 2.5, 2.5, 1.5], &[2.0, 1.5, 2.0, 2.5, 1.0, 0.0]),
        };
        let mut look = Look {
            set: true,
            hair: pick(r(1), hair),
            beard: pick(r(2), beard),
            hair_color: pick(r(3), colors),
            skin: pick(r(4), skin),
            build: pick(r(5), &[0.7, 2.0, 3.0, 2.0, 0.8]),
            face: pick(r(6), face),
        };
        // Каждый седьмой — в годах: седина, а у кого-то и лысина.
        if r(7) < 0.15 {
            look.hair_color = if r(8) < 0.6 { 7 } else { 8 };
            if look.hair == 0 && r(9) < 0.5 {
                look.hair = 3;
            }
        }
        look
    }

    pub fn hair_style(&self) -> Hair {
        HAIR_STYLES[self.hair as usize % HAIR_STYLES.len()].0
    }
    pub fn beard_style(&self) -> Beard {
        BEARD_STYLES[self.beard as usize % BEARD_STYLES.len()].0
    }
    pub fn hair_tint(&self) -> Color {
        let c = HAIR_COLORS[self.hair_color as usize % HAIR_COLORS.len()].0;
        Color::srgb(c[0], c[1], c[2])
    }
    pub fn skin_tint(&self) -> Color {
        let c = SKIN_TONES[self.skin as usize % SKIN_TONES.len()].0;
        Color::srgb(c[0], c[1], c[2])
    }
    pub fn shape(&self) -> Shape {
        let (thin, heavy) = BUILDS[self.build as usize % BUILDS.len()].0;
        let [rugged, longnose, steppe] = FACES[self.face as usize % FACES.len()].0;
        Shape { thin, heavy, rugged, longnose, steppe }
    }

    /// Число вариантов и подпись каждого поля (для окна облика).
    pub fn field(&self, f: LookField) -> (u8, usize, &'static str) {
        match f {
            LookField::Hair => (self.hair, HAIR_STYLES.len(), HAIR_STYLES[self.hair as usize % HAIR_STYLES.len()].1),
            LookField::Beard => (self.beard, BEARD_STYLES.len(), BEARD_STYLES[self.beard as usize % BEARD_STYLES.len()].1),
            LookField::HairColor => (self.hair_color, HAIR_COLORS.len(), HAIR_COLORS[self.hair_color as usize % HAIR_COLORS.len()].1),
            LookField::Skin => (self.skin, SKIN_TONES.len(), SKIN_TONES[self.skin as usize % SKIN_TONES.len()].1),
            LookField::Build => (self.build, BUILDS.len(), BUILDS[self.build as usize % BUILDS.len()].1),
            LookField::Face => (self.face, FACES.len(), FACES[self.face as usize % FACES.len()].1),
        }
    }

    /// Следующий/предыдущий вариант поля (по кругу).
    pub fn step(&mut self, f: LookField, d: i32) {
        let (v, n, _) = self.field(f);
        let v = (v as i32 + d).rem_euclid(n as i32) as u8;
        match f {
            LookField::Hair => self.hair = v,
            LookField::Beard => self.beard = v,
            LookField::HairColor => self.hair_color = v,
            LookField::Skin => self.skin = v,
            LookField::Build => self.build = v,
            LookField::Face => self.face = v,
        }
        self.set = true;
    }

    /// Строка для settings.txt: `причёска 0, борода 2, …` — порядок полей как в `LookField::ALL`.
    pub fn to_text(&self) -> String {
        LookField::ALL.iter().map(|f| format!("{} {}", f.key(), self.field(*f).0)).collect::<Vec<_>>().join(", ")
    }

    pub fn from_text(text: &str) -> Option<Look> {
        let mut look = Look::PLAYER_DEFAULT;
        let mut any = false;
        for part in text.split(',') {
            let mut it = part.split_whitespace();
            let (Some(k), Some(v)) = (it.next(), it.next()) else { continue };
            let Ok(v) = v.parse::<u8>() else { continue };
            let Some(f) = LookField::ALL.iter().find(|f| f.key() == k) else { continue };
            let n = look.field(*f).1 as u8;
            let v = v.min(n - 1);
            match f {
                LookField::Hair => look.hair = v,
                LookField::Beard => look.beard = v,
                LookField::HairColor => look.hair_color = v,
                LookField::Skin => look.skin = v,
                LookField::Build => look.build = v,
                LookField::Face => look.face = v,
            }
            any = true;
        }
        any.then_some(look)
    }

    /// Упаковка в 7 байт (профиль подключения).
    pub fn pack(&self) -> [u8; 7] {
        [self.set as u8, self.hair, self.beard, self.hair_color, self.skin, self.build, self.face]
    }

    pub fn unpack(b: &[u8]) -> Look {
        if b.len() < 7 || b[0] != 1 {
            return Look::default();
        }
        let clamp = |v: u8, n: usize| v.min(n as u8 - 1);
        Look {
            set: true,
            hair: clamp(b[1], HAIR_STYLES.len()),
            beard: clamp(b[2], BEARD_STYLES.len()),
            hair_color: clamp(b[3], HAIR_COLORS.len()),
            skin: clamp(b[4], SKIN_TONES.len()),
            build: clamp(b[5], BUILDS.len()),
            face: clamp(b[6], FACES.len()),
        }
    }
}

/// Поля окна облика.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookField {
    Hair,
    HairColor,
    Beard,
    Skin,
    Build,
    Face,
}

impl LookField {
    pub const ALL: [LookField; 6] = [LookField::Hair, LookField::HairColor, LookField::Beard, LookField::Skin, LookField::Build, LookField::Face];

    pub fn title(&self) -> &'static str {
        match self {
            LookField::Hair => "Причёска",
            LookField::HairColor => "Цвет волос",
            LookField::Beard => "Борода",
            LookField::Skin => "Кожа",
            LookField::Build => "Сложение",
            LookField::Face => "Лицо",
        }
    }

    fn key(&self) -> &'static str {
        match self {
            LookField::Hair => "причёска",
            LookField::HairColor => "волосы",
            LookField::Beard => "борода",
            LookField::Skin => "кожа",
            LookField::Build => "сложение",
            LookField::Face => "лицо",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_bytes_round_trip() {
        let mut l = Look::PLAYER_DEFAULT;
        l.step(LookField::Beard, 2);
        l.step(LookField::Skin, -1);
        l.step(LookField::Face, 5);
        assert_eq!(Look::from_text(&l.to_text()), Some(l));
        assert_eq!(Look::unpack(&l.pack()), l);
        assert_eq!(Look::unpack(&[0; 7]), Look::default());
        // Мусор и числа вне списка не ломают облик.
        let g = Look::from_text("причёска 99, кожа abc, ???").unwrap();
        assert_eq!(g.hair as usize, HAIR_STYLES.len() - 1);
        assert_eq!(g.skin, Look::PLAYER_DEFAULT.skin);
    }

    #[test]
    fn people_are_varied() {
        // Среди сотни горожан есть бородатые, бритые, усатые, разные причёски, кожа и сложение.
        use std::collections::HashSet;
        let looks: Vec<Look> = (0..100).map(|i| Look::random(i, Folk::Romaioi)).collect();
        let distinct = |f: fn(&Look) -> u8| looks.iter().map(f).collect::<HashSet<_>>().len();
        assert!(distinct(|l| l.beard) >= 5);
        assert!(distinct(|l| l.hair) >= 4);
        assert!(distinct(|l| l.skin) >= 4);
        assert!(distinct(|l| l.build) >= 4);
        assert!(distinct(|l| l.face) >= 4);
        assert!(looks.iter().collect::<HashSet<_>>().len() > 80, "почти все разные");
        // Гунны чаще бритоголовые и со степными чертами, готы светлее ромеев.
        let huns: Vec<Look> = (0..100).map(|i| Look::random(i, Folk::Nomads)).collect();
        assert!(huns.iter().filter(|l| l.face == 5).count() > 40);
        let goths: Vec<Look> = (0..100).map(|i| Look::random(i, Folk::Goths)).collect();
        let avg = |v: &[Look]| v.iter().map(|l| l.skin as f32).sum::<f32>() / v.len() as f32;
        assert!(avg(&goths) + 1.0 < avg(&looks));
    }
}
