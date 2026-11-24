//! Греческие буквы для школы: как их выводят стилем по воску (черты по порядку), как они
//! называются, школьные слова для письма и греческие цифры для счёта на абаке.
//!
//! Буквы — унциальные (библейское маюскульное письмо VI века): эпсилон и сигма лунные (Є, Ϲ),
//! омега — «ω» в рост строки. Чистые данные: их читают и миниигры (work.rs), и рисование
//! таблички (work_client.rs).

use bevy::math::Vec2;
use std::f32::consts::{PI, TAU};

/// Черта — ломаная в клетке буквы: x вправо, y вверх, всё в 0..1.
pub type Stroke = Vec<Vec2>;

/// Буква: имя по-русски, строчное начертание (для подсказок) и черты.
pub struct Glyph {
    pub name: &'static str,
    /// Строчная буква для подписи (шрифт игры её знает; для архаичных — пусто).
    pub lower: &'static str,
    pub strokes: Vec<Stroke>,
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn line(pts: &[(f32, f32)]) -> Stroke {
    pts.iter().map(|&(x, y)| v(x, y)).collect()
}

/// Дуга эллипса от `a0` до `a1` (радианы, против часовой; a1 < a0 — по часовой).
fn arc(c: (f32, f32), r: (f32, f32), a0: f32, a1: f32) -> Stroke {
    let n = (((a1 - a0).abs() / TAU) * 28.0).ceil().max(4.0) as usize;
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            v(c.0 + r.0 * a.cos(), c.1 + r.1 * a.sin())
        })
        .collect()
}

fn deg(d: f32) -> f32 {
    d * PI / 180.0
}

/// Сколько букв в азбуке (24) — дальше идут знаки только для цифр.
pub const ALPHABET_LEN: usize = 24;
/// Номера знаков-цифр: стигма (6), коппа (90), сампи (900).
pub const STIGMA: usize = 24;
pub const KOPPA: usize = 25;
pub const SAMPI: usize = 26;

/// Все знаки: 24 буквы по порядку и три знака-цифры.
pub fn glyphs() -> &'static [Glyph] {
    static G: std::sync::OnceLock<Vec<Glyph>> = std::sync::OnceLock::new();
    G.get_or_init(build)
}

pub fn glyph(i: usize) -> &'static Glyph {
    &glyphs()[i % glyphs().len()]
}

fn build() -> Vec<Glyph> {
    let g = |name, lower, strokes: Vec<Stroke>| Glyph {
        name,
        lower,
        strokes,
    };
    // Чаша Β, Ρ: от стойки вправо и обратно к стойке.
    let bowl = |x0: f32, y0: f32, y1: f32, w: f32| -> Stroke {
        let cy = (y0 + y1) / 2.0;
        let r = (y1 - y0) / 2.0;
        let mut s = vec![v(x0, y1)];
        s.extend(
            arc((x0 + w - r, cy), (r, r), deg(90.0), deg(-90.0))
                .into_iter()
                .map(|p| v(p.x.max(x0), p.y)),
        );
        s.push(v(x0, y0));
        s
    };
    vec![
        g(
            "альфа",
            "α",
            vec![
                line(&[(0.08, 0.0), (0.5, 1.0), (0.92, 0.0)]),
                line(&[(0.27, 0.42), (0.73, 0.42)]),
            ],
        ),
        g(
            "бета",
            "β",
            vec![
                line(&[(0.2, 0.0), (0.2, 1.0)]),
                bowl(0.2, 0.52, 1.0, 0.52),
                bowl(0.2, 0.0, 0.52, 0.62),
            ],
        ),
        g(
            "гамма",
            "γ",
            vec![line(&[(0.22, 0.0), (0.22, 1.0), (0.85, 1.0)])],
        ),
        g(
            "дельта",
            "δ",
            vec![line(&[(0.5, 1.0), (0.06, 0.0), (0.94, 0.0), (0.5, 1.0)])],
        ),
        g(
            "эпсилон",
            "ε",
            vec![
                arc((0.58, 0.5), (0.42, 0.5), deg(55.0), deg(305.0)),
                line(&[(0.16, 0.5), (0.66, 0.5)]),
            ],
        ),
        g(
            "дзета",
            "ζ",
            vec![line(&[(0.14, 1.0), (0.86, 1.0), (0.14, 0.0), (0.86, 0.0)])],
        ),
        g(
            "эта",
            "η",
            vec![
                line(&[(0.16, 1.0), (0.16, 0.0)]),
                line(&[(0.84, 1.0), (0.84, 0.0)]),
                line(&[(0.16, 0.5), (0.84, 0.5)]),
            ],
        ),
        g(
            "тета",
            "θ",
            vec![
                arc((0.5, 0.5), (0.4, 0.5), deg(90.0), deg(450.0)),
                line(&[(0.14, 0.5), (0.86, 0.5)]),
            ],
        ),
        g("йота", "ι", vec![line(&[(0.5, 1.0), (0.5, 0.0)])]),
        g(
            "каппа",
            "κ",
            vec![
                line(&[(0.2, 1.0), (0.2, 0.0)]),
                line(&[(0.84, 1.0), (0.22, 0.46), (0.86, 0.0)]),
            ],
        ),
        g(
            "лямбда",
            "λ",
            vec![line(&[(0.08, 0.0), (0.5, 1.0), (0.92, 0.0)])],
        ),
        g(
            "мю",
            "μ",
            vec![line(&[
                (0.06, 0.0),
                (0.16, 1.0),
                (0.5, 0.28),
                (0.84, 1.0),
                (0.94, 0.0),
            ])],
        ),
        g(
            "ню",
            "ν",
            vec![line(&[(0.16, 0.0), (0.16, 1.0), (0.84, 0.0), (0.84, 1.0)])],
        ),
        g(
            "кси",
            "ξ",
            vec![
                line(&[(0.14, 1.0), (0.86, 1.0)]),
                line(&[(0.26, 0.5), (0.74, 0.5)]),
                line(&[(0.14, 0.0), (0.86, 0.0)]),
            ],
        ),
        g(
            "омикрон",
            "ο",
            vec![arc((0.5, 0.5), (0.42, 0.5), deg(90.0), deg(450.0))],
        ),
        g(
            "пи",
            "π",
            vec![line(&[(0.16, 0.0), (0.16, 1.0), (0.84, 1.0), (0.84, 0.0)])],
        ),
        g(
            "ро",
            "ρ",
            vec![
                line(&[(0.22, 0.0), (0.22, 1.0)]),
                bowl(0.22, 0.44, 1.0, 0.6),
            ],
        ),
        g(
            "сигма",
            "σ",
            vec![arc((0.58, 0.5), (0.44, 0.5), deg(50.0), deg(310.0))],
        ),
        g(
            "тау",
            "τ",
            vec![
                line(&[(0.08, 1.0), (0.92, 1.0)]),
                line(&[(0.5, 1.0), (0.5, 0.0)]),
            ],
        ),
        g(
            "ипсилон",
            "υ",
            vec![
                line(&[(0.08, 1.0), (0.5, 0.48), (0.92, 1.0)]),
                line(&[(0.5, 0.48), (0.5, 0.0)]),
            ],
        ),
        g(
            "фи",
            "φ",
            vec![
                line(&[(0.5, 1.0), (0.5, 0.0)]),
                arc((0.5, 0.55), (0.42, 0.28), deg(90.0), deg(450.0)),
            ],
        ),
        g(
            "хи",
            "χ",
            vec![
                line(&[(0.1, 1.0), (0.9, 0.0)]),
                line(&[(0.9, 1.0), (0.1, 0.0)]),
            ],
        ),
        g(
            "пси",
            "ψ",
            vec![
                line(&[(0.5, 1.0), (0.5, 0.0)]),
                line(&[
                    (0.08, 0.95),
                    (0.14, 0.58),
                    (0.5, 0.36),
                    (0.86, 0.58),
                    (0.92, 0.95),
                ]),
            ],
        ),
        g(
            "омега",
            "ω",
            vec![line(&[
                (0.1, 0.92),
                (0.03, 0.45),
                (0.14, 0.06),
                (0.36, 0.04),
                (0.5, 0.42),
                (0.64, 0.04),
                (0.86, 0.06),
                (0.97, 0.45),
                (0.9, 0.92),
            ])],
        ),
        // Цифры, которых уже нет в азбуке.
        g(
            "стигма",
            "",
            vec![{
                let mut s = arc((0.55, 0.62), (0.38, 0.36), deg(40.0), deg(230.0));
                s.extend([v(0.5, 0.3), v(0.72, 0.18), v(0.6, -0.05)]);
                s
            }],
        ),
        g(
            "коппа",
            "",
            vec![line(&[(0.18, 1.0), (0.62, 0.62), (0.3, 0.4), (0.82, 0.0)])],
        ),
        g(
            "сампи",
            "",
            vec![
                line(&[(0.18, 0.0), (0.62, 1.0)]),
                line(&[(0.4, 0.5), (0.9, 0.12)]),
                line(&[(0.5, 0.74), (0.92, 0.42)]),
            ],
        ),
    ]
}

/// Длина ломаной.
pub fn stroke_len(s: &Stroke) -> f32 {
    s.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Точка ломаной на расстоянии `d` от начала (по дуге).
pub fn point_at(s: &Stroke, d: f32) -> Vec2 {
    let mut left = d.max(0.0);
    for w in s.windows(2) {
        let l = w[0].distance(w[1]);
        if left <= l {
            return w[0].lerp(w[1], if l > 0.0 { left / l } else { 0.0 });
        }
        left -= l;
    }
    *s.last().unwrap_or(&Vec2::ZERO)
}

/// Ближайшая к `p` точка ломаной на участке дуги [d0, d1]: (длина дуги до неё, расстояние).
pub fn nearest_in(s: &Stroke, p: Vec2, d0: f32, d1: f32) -> (f32, f32) {
    let mut best = (d0, p.distance(point_at(s, d0)));
    let mut at = 0.0;
    for w in s.windows(2) {
        let l = w[0].distance(w[1]);
        let (a, b) = (at, at + l);
        at = b;
        if b < d0 || a > d1 || l <= 0.0 {
            continue;
        }
        let t = ((p - w[0]).dot(w[1] - w[0]) / (l * l)).clamp(0.0, 1.0);
        let d = (a + t * l).clamp(d0, d1);
        let q = point_at(s, d);
        let dist = p.distance(q);
        if dist < best.1 {
            best = (d, dist);
        }
    }
    best
}

/// Школьные слова для письма: (греческие буквы по номерам, как пишется строчными, что значит).
pub const WORDS: &[(&[usize], &str, &str)] = &[
    (&[2, 16, 0, 11, 11, 0, 18, 0], "γράμματα", "буквы, грамота"),
    (&[21, 4, 16, 17, 23, 12], "Χερσών", "Херсон"),
    (&[7, 4, 14, 17], "Θεός", "Бог"),
    (&[21, 0, 8, 16, 4], "χαῖρε", "радуйся (привет)"),
    (&[0, 16, 18, 14, 17], "ἄρτος", "хлеб"),
    (&[14, 8, 12, 14, 17], "οἶνος", "вино"),
    (&[20, 23, 17], "φῶς", "свет"),
    (&[10, 14, 2, 14, 17], "λόγος", "слово"),
    (&[8, 21, 7, 19, 17], "ἰχθύς", "рыба"),
    (&[3, 8, 9, 0, 8, 14, 17], "δίκαιος", "праведный"),
    (&[7, 0, 10, 0, 17, 17, 0], "θάλασσα", "море"),
    (&[15, 14, 10, 8, 17], "πόλις", "город"),
    (&[1, 8, 1, 10, 14, 17], "βίβλος", "книга"),
    (&[9, 0, 10, 0, 11, 14, 17], "κάλαμος", "калам, перо"),
    (&[11, 0, 7, 6, 18, 6, 17], "μαθητής", "ученик"),
];

/// Буквы-цифры: единицы, десятки, сотни (номера знаков; None — ноль, разряд пуст).
pub fn numeral_digit(place: usize, d: u8) -> Option<usize> {
    const UNITS: [usize; 9] = [0, 1, 2, 3, 4, STIGMA, 5, 6, 7];
    const TENS: [usize; 9] = [8, 9, 10, 11, 12, 13, 14, 15, KOPPA];
    const HUNDREDS: [usize; 9] = [16, 17, 18, 19, 20, 21, 22, 23, SAMPI];
    if d == 0 || d > 9 {
        return None;
    }
    let i = d as usize - 1;
    Some(match place {
        0 => UNITS[i],
        1 => TENS[i],
        _ => HUNDREDS[i],
    })
}

/// Число 1..999 греческими цифрами: знаки от старшего разряда к младшему.
pub fn numeral(n: u32) -> Vec<usize> {
    let n = n.min(999);
    let digits = [(n / 100) as u8, ((n / 10) % 10) as u8, (n % 10) as u8];
    digits
        .iter()
        .enumerate()
        .filter_map(|(k, d)| numeral_digit(2 - k, *d))
        .collect()
}

/// Подпись числа для подсказки: «ρμεʹ» (архаичные знаки — по имени).
pub fn numeral_text(n: u32) -> String {
    let mut s: String = numeral(n)
        .into_iter()
        .map(|g| {
            if glyph(g).lower.is_empty() {
                format!("[{}]", glyph(g).name)
            } else {
                glyph(g).lower.to_string()
            }
        })
        .collect();
    s.push('ʹ');
    s
}

/// Подсказка новичку: какой знак сколько значит («ρ = 100, μ = 40, ε = 5»).
pub fn numeral_legend(n: u32) -> String {
    let n = n.min(999);
    let digits = [(n / 100) as u8, ((n / 10) % 10) as u8, (n % 10) as u8];
    let mut parts = Vec::new();
    for (k, d) in digits.iter().enumerate() {
        let place = 2 - k;
        if let Some(g) = numeral_digit(place, *d) {
            let gl = glyph(g);
            let mark = if gl.lower.is_empty() {
                gl.name.to_string()
            } else {
                format!("{} ({})", gl.lower, gl.name)
            };
            parts.push(format!("{mark} = {}", *d as u32 * 10u32.pow(place as u32)));
        }
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_fits_its_box() {
        assert_eq!(glyphs().len(), 27);
        for g in glyphs() {
            assert!(!g.strokes.is_empty(), "{}", g.name);
            for s in &g.strokes {
                assert!(s.len() >= 2 && stroke_len(s) > 0.2, "{}", g.name);
                for p in s {
                    assert!(
                        p.x >= -0.06 && p.x <= 1.06 && p.y >= -0.06 && p.y <= 1.06,
                        "{} {p:?}",
                        g.name
                    );
                }
            }
        }
    }

    #[test]
    fn words_spell_with_known_letters() {
        // Без придыханий и ударений: ά -> α и т. п.
        let plain = |c: char| match c {
            'ά' | 'ἄ' => 'α',
            'έ' => 'ε',
            'ί' | 'ἰ' | 'ῖ' | 'ἶ' => 'ι',
            'ό' => 'ο',
            'ύ' => 'υ',
            'ή' => 'η',
            'ώ' | 'ῶ' => 'ω',
            'ς' => 'σ',
            c => c.to_lowercase().next().unwrap(),
        };
        for (letters, word, _) in WORDS {
            assert!(letters.iter().all(|i| *i < ALPHABET_LEN), "{word}");
            let spelled: String = letters.iter().map(|i| glyph(*i).lower).collect();
            let plain_word: String = word.chars().map(plain).collect();
            assert_eq!(spelled, plain_word, "{word}");
        }
    }

    #[test]
    fn greek_numerals() {
        // 145 = ρμε, 6 = стигма, 90 = коппа, 900 = сампи, 10 = ι.
        assert_eq!(numeral(145), vec![16, 11, 4]);
        assert_eq!(numeral(6), vec![STIGMA]);
        assert_eq!(numeral(90), vec![KOPPA]);
        assert_eq!(numeral(900), vec![SAMPI]);
        assert_eq!(numeral(10), vec![8]);
        assert_eq!(numeral_text(145), "ρμεʹ");
        assert_eq!(numeral_legend(205), "σ (сигма) = 200, ε (эпсилон) = 5");
    }

    #[test]
    fn nearest_point_on_stroke() {
        let s = line(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        assert!((stroke_len(&s) - 2.0).abs() < 1e-5);
        let (d, dist) = nearest_in(&s, v(0.5, 0.1), 0.0, 2.0);
        assert!((d - 0.5).abs() < 1e-4 && (dist - 0.1).abs() < 1e-4);
        // Окно не пускает дальше: от точки у второй черты, но с окном до 0.6 — конец окна.
        let (d, _) = nearest_in(&s, v(1.0, 0.9), 0.0, 0.6);
        assert!((d - 0.6).abs() < 1e-4);
        assert_eq!(point_at(&s, 1.5), v(1.0, 0.5));
    }
}
