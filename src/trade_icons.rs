//! Иконки монет и товаров торговли (часть модуля `icons`, рисуется тем же холстом).

use super::{
    CLOTH, CLOTH_DARK, Canvas, GLASS, GOLD, GOLD_DARK, LEATHER, LEATHER_DARK, RED, Rgba, WHITE, rgb,
};

const BRONZE: Rgba = rgb(150, 98, 52);
const BRONZE_LIGHT: Rgba = rgb(196, 136, 78);
const BRONZE_DARK: Rgba = rgb(98, 62, 32);
const CLAY: Rgba = rgb(176, 104, 62);
const CLAY_LIGHT: Rgba = rgb(206, 138, 92);
const CLAY_DARK: Rgba = rgb(122, 68, 38);
const WINE: Rgba = rgb(110, 24, 36);
const OIL: Rgba = rgb(170, 160, 60);
const FISH: Rgba = rgb(150, 162, 170);
const FISH_DARK: Rgba = rgb(88, 98, 110);
const WAX: Rgba = rgb(226, 186, 82);
const WAX_DARK: Rgba = rgb(176, 136, 46);
const HONEY: Rgba = rgb(214, 140, 30);
const WOOL: Rgba = rgb(222, 212, 188);
const WOOL_DARK: Rgba = rgb(180, 166, 136);
const FELT: Rgba = rgb(150, 138, 116);
const FELT_DARK: Rgba = rgb(108, 98, 80);
const LINEN: Rgba = rgb(226, 220, 200);
const WOOLEN: Rgba = rgb(140, 64, 44);
const WOOLEN_DARK: Rgba = rgb(98, 42, 30);
const GREEN_GLASS: Rgba = [0.55, 0.78, 0.62, 0.75];

/// Монета: кружок с ободком и знаком номинала.
fn coin(c: &mut Canvas, x: f32, y: f32, r: f32, face: Rgba, edge: Rgba, light: Rgba) {
    c.circle(x, y, r, edge);
    c.circle(x, y - r * 0.08, r * 0.82, face);
    c.ring(x, y - r * 0.08, r * 0.55, r * 0.66, edge);
    c.circle(x - r * 0.3, y - r * 0.38, r * 0.14, light);
}

/// Буква «M» фоллиса.
fn letter_m(c: &mut Canvas, x: f32, y: f32, s: f32, col: Rgba) {
    let w = s * 0.12;
    c.line(x - s * 0.4, y + s * 0.4, x - s * 0.4, y - s * 0.4, w, col);
    c.line(x - s * 0.4, y - s * 0.4, x, y + s * 0.1, w, col);
    c.line(x, y + s * 0.1, x + s * 0.4, y - s * 0.4, w, col);
    c.line(x + s * 0.4, y - s * 0.4, x + s * 0.4, y + s * 0.4, w, col);
}

/// Амфора: узкое горло, две ручки, яйцевидное тело и острый носик (тип LRA).
fn amphora(c: &mut Canvas, dipinto: Rgba) {
    c.poly(
        &[
            (0.75, 0.45),
            (1.25, 0.45),
            (1.6, 0.95),
            (1.72, 1.6),
            (1.55, 2.25),
            (1.12, 2.65),
            (1.0, 2.9),
            (0.88, 2.65),
            (0.45, 2.25),
            (0.28, 1.6),
            (0.4, 0.95),
        ],
        CLAY,
    );
    c.poly(
        &[
            (0.75, 0.45),
            (1.0, 0.45),
            (0.85, 1.0),
            (0.55, 1.6),
            (0.5, 2.2),
            (0.45, 2.25),
            (0.28, 1.6),
            (0.4, 0.95),
        ],
        CLAY_LIGHT,
    );
    // Горло и венчик.
    c.rect(0.82, 0.18, 1.18, 0.5, CLAY);
    c.rect(0.76, 0.12, 1.24, 0.24, CLAY_DARK);
    // Ручки.
    c.line(0.82, 0.3, 0.46, 0.55, 0.09, CLAY_DARK);
    c.line(0.46, 0.55, 0.48, 0.95, 0.09, CLAY_DARK);
    c.line(1.18, 0.3, 1.54, 0.55, 0.09, CLAY_DARK);
    c.line(1.54, 0.55, 1.52, 0.95, 0.09, CLAY_DARK);
    // Желобки на теле (как у позднеримских амфор).
    for y in [1.2f32, 1.4, 1.6, 1.8, 2.0] {
        c.line(0.38, y, 1.62, y, 0.02, CLAY_DARK);
    }
    // Надпись краской (дипинто) — что внутри.
    c.line(0.75, 1.05, 1.25, 1.15, 0.07, dipinto);
    c.line(0.8, 1.25, 1.15, 1.3, 0.05, dipinto);
}

fn cloth_bolt(c: &mut Canvas, body: Rgba, dark: Rgba) {
    c.poly(&[(0.25, 0.55), (1.4, 0.38), (1.5, 1.42), (0.45, 1.7)], body);
    c.circle(1.42, 0.9, 0.5, body);
    c.ring(1.42, 0.9, 0.2, 0.5, dark);
    c.ring(1.42, 0.9, 0.34, 0.38, dark);
    c.circle(1.42, 0.9, 0.12, dark);
    c.poly(&[(0.25, 0.55), (0.45, 1.7), (0.3, 1.85), (0.1, 0.7)], dark);
    for k in [0.2f32, 0.45, 0.7] {
        c.line(0.35, 0.6 + k, 1.35, 0.45 + k, 0.03, dark);
    }
}

fn hide(c: &mut Canvas, body: Rgba, dark: Rgba) {
    c.poly(
        &[
            (0.5, 0.2),
            (0.75, 0.4),
            (1.25, 0.4),
            (1.5, 0.2),
            (1.62, 0.55),
            (1.5, 0.9),
            (1.75, 1.3),
            (1.55, 1.85),
            (1.15, 1.7),
            (0.85, 1.7),
            (0.45, 1.85),
            (0.25, 1.3),
            (0.5, 0.9),
            (0.38, 0.55),
        ],
        body,
    );
    c.poly(
        &[
            (0.75, 0.55),
            (1.25, 0.55),
            (1.35, 1.3),
            (1.0, 1.5),
            (0.65, 1.3),
        ],
        dark,
    );
}

/// Рисует предмет торговли. false — не наш предмет.
pub fn draw(name: &str, c: &mut Canvas) -> bool {
    match name {
        "Тремисс" => {
            coin(c, 0.62, 0.66, 0.24, GOLD, GOLD_DARK, WHITE);
            coin(c, 0.4, 0.4, 0.24, GOLD, GOLD_DARK, WHITE);
        }
        "Фоллис" => {
            coin(c, 0.62, 0.7, 0.24, BRONZE, BRONZE_DARK, BRONZE_LIGHT);
            coin(c, 0.45, 0.45, 0.33, BRONZE, BRONZE_DARK, BRONZE_LIGHT);
            letter_m(c, 0.45, 0.42, 0.28, BRONZE_DARK);
        }
        "Декануммий" => {
            for (x, y) in [(0.3, 0.7), (0.7, 0.7), (0.5, 0.42)] {
                coin(c, x, y, 0.2, BRONZE, BRONZE_DARK, BRONZE_LIGHT);
                c.line(x, y - 0.12, x, y + 0.08, 0.05, BRONZE_DARK);
            }
        }
        "Амфора вина" => amphora(c, WINE),
        "Амфора оливкового масла" => amphora(c, OIL),
        "Стеклянная чаша" => {
            c.poly(
                &[(0.12, 0.35), (0.88, 0.35), (0.75, 0.68), (0.25, 0.68)],
                GREEN_GLASS,
            );
            c.rect(0.44, 0.68, 0.56, 0.82, GREEN_GLASS);
            c.rect(0.28, 0.82, 0.72, 0.9, GREEN_GLASS);
            c.line(0.2, 0.42, 0.35, 0.6, 0.04, GLASS);
            c.line(0.12, 0.35, 0.88, 0.35, 0.04, WHITE);
        }
        "Отрез льна" => cloth_bolt(c, LINEN, CLOTH_DARK),
        "Отрез шерсти" => cloth_bolt(c, WOOLEN, WOOLEN_DARK),
        "Солёная рыба" => {
            c.poly(
                &[
                    (0.5, 0.15),
                    (0.78, 0.55),
                    (0.8, 1.3),
                    (0.6, 1.62),
                    (0.82, 1.9),
                    (0.18, 1.9),
                    (0.4, 1.62),
                    (0.2, 1.3),
                    (0.22, 0.55),
                ],
                FISH,
            );
            c.poly(
                &[
                    (0.5, 0.15),
                    (0.62, 0.5),
                    (0.6, 1.4),
                    (0.5, 1.6),
                    (0.4, 1.4),
                    (0.38, 0.5),
                ],
                FISH_DARK,
            );
            c.circle(0.42, 0.42, 0.05, WHITE);
            // Бечёвка, на которой рыбу вялят.
            c.line(0.5, 0.0, 0.5, 0.25, 0.05, LEATHER_DARK);
        }
        "Мешок соли" => {
            c.poly(
                &[
                    (0.6, 0.45),
                    (1.4, 0.45),
                    (1.75, 1.2),
                    (1.65, 1.8),
                    (0.35, 1.8),
                    (0.25, 1.2),
                ],
                CLOTH,
            );
            c.poly(
                &[(0.75, 0.15), (1.25, 0.15), (1.4, 0.45), (0.6, 0.45)],
                CLOTH_DARK,
            );
            c.line(0.6, 0.45, 1.4, 0.45, 0.07, LEATHER_DARK);
            c.circle(1.0, 0.25, 0.14, WHITE);
            for (x, y) in [(0.55, 1.6), (1.4, 1.55), (1.0, 1.2)] {
                c.line(x - 0.12, y, x + 0.12, y, 0.03, CLOTH_DARK);
            }
        }
        "Сырая шкура" => hide(c, LEATHER, LEATHER_DARK),
        "Овчина" => {
            hide(c, WOOL, WOOL_DARK);
            for (x, y) in [
                (0.6, 0.7),
                (1.0, 0.9),
                (1.4, 0.7),
                (0.8, 1.3),
                (1.2, 1.3),
                (1.0, 0.55),
            ] {
                c.ring(x, y, 0.06, 0.1, WOOL_DARK);
            }
        }
        "Войлок" => {
            c.rect(0.2, 0.45, 1.4, 1.55, FELT);
            c.circle(1.42, 1.0, 0.55, FELT);
            c.ring(1.42, 1.0, 0.22, 0.55, FELT_DARK);
            c.ring(1.42, 1.0, 0.38, 0.42, FELT_DARK);
            // Узор, как на войлоке кочевников.
            c.line(0.3, 0.72, 1.0, 0.72, 0.06, RED);
            c.line(0.3, 1.28, 1.0, 1.28, 0.06, RED);
            c.poly(&[(0.65, 0.85), (0.8, 1.0), (0.65, 1.15), (0.5, 1.0)], RED);
        }
        "Воск" => {
            c.circle(0.5, 0.55, 0.38, WAX_DARK);
            c.circle(0.5, 0.5, 0.33, WAX);
            c.ring(0.5, 0.5, 0.16, 0.2, WAX_DARK);
        }
        "Горшок мёда" => {
            c.poly(
                &[
                    (0.25, 0.3),
                    (0.75, 0.3),
                    (0.88, 0.6),
                    (0.75, 0.92),
                    (0.25, 0.92),
                    (0.12, 0.6),
                ],
                CLAY,
            );
            c.rect(0.22, 0.2, 0.78, 0.32, CLAY_DARK);
            c.poly(
                &[
                    (0.3, 0.32),
                    (0.42, 0.32),
                    (0.4, 0.55),
                    (0.34, 0.6),
                    (0.3, 0.5),
                ],
                HONEY,
            );
            c.rect(0.2, 0.12, 0.8, 0.2, CLOTH);
        }
        _ => return false,
    }
    true
}
