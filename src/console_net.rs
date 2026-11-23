//! Консоль разработчика: сообщения между клиентом и сервером (канал 11).
//!
//! Читы (полёт, бессмертие, телепорт, выдача предметов) разрешает только сервер. Хозяин игры
//! (тот, кто нажал «Создать игру», или одиночная игра) доказывает, что он хозяин, секретом
//! [`host_token`]: сервер и его окно живут в одном процессе, а подключившиеся друзья этого
//! секрета не знают. Остальным читы можно открыть командой `sv_cheats 1` хозяина или
//! переменной окружения `MEDIEVAL_CHEATS=1` у выделенного сервера.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub const CH_CONSOLE: u8 = 11;

/// Секрет хозяина этого процесса (0 — процесс не хостит игру).
static HOST_TOKEN: OnceLock<u64> = OnceLock::new();

/// Запомнить, что этот процесс — хост (вызывать до запуска сервера и окна).
pub fn make_host_token() {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let addr = &nanos as *const u64 as u64;
    // Перемешиваем, чтобы секрет нельзя было угадать по времени запуска.
    let mut x = nanos ^ addr.rotate_left(29) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    let _ = HOST_TOKEN.set(x | 1);
}

pub fn host_token() -> u64 {
    HOST_TOKEN.get().copied().unwrap_or(0)
}

/// Читы одного игрока.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Cheats {
    /// Полёт (сквозь стены, если `noclip`).
    pub fly: bool,
    pub noclip: bool,
    /// Не получает урона, не устаёт.
    pub god: bool,
    /// Враги не замечают.
    pub notarget: bool,
    /// Множитель скорости ходьбы и полёта.
    pub speed: f32,
}

impl Default for Cheats {
    fn default() -> Self {
        Self { fly: false, noclip: false, god: false, notarget: false, speed: 1.0 }
    }
}

impl Cheats {
    /// Сервер не проверяет скорость и стены.
    pub fn free_move(&self) -> bool {
        self.fly || self.noclip || self.speed > 1.01
    }
}

/// Клиент -> сервер.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ConsoleClientMessage {
    /// Строка команды; `token` — секрет хозяина (см. модуль), у гостей 0.
    Command { line: String, token: u64 },
}

/// Сервер -> клиент.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ConsoleServerMessage {
    /// Строка ответа в консоль.
    Reply { text: String, error: bool },
    /// Новые читы этого игрока.
    Cheats(Cheats),
    /// Повернуть взгляд (после `tp x y z yaw pitch`).
    Look { yaw: f32, pitch: f32 },
}

/// Разбить строку на слова; «кавычки» и "кавычки" склеивают слова с пробелами.
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' | '«' | '»' => {
                if quoted && !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// «вкл/выкл»: явный аргумент или переключение.
pub fn parse_toggle(arg: Option<&str>, current: bool) -> Option<bool> {
    match arg.map(|s| s.to_lowercase()) {
        None => Some(!current),
        Some(s) => match s.as_str() {
            "1" | "on" | "true" | "вкл" | "да" => Some(true),
            "0" | "off" | "false" | "выкл" | "нет" => Some(false),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_names() {
        assert_eq!(split_args("give «Целебное зелье» 3"), vec!["give", "Целебное зелье", "3"]);
        assert_eq!(split_args("  tp  1 2   3 "), vec!["tp", "1", "2", "3"]);
        assert_eq!(split_args("give \"a b\""), vec!["give", "a b"]);
    }

    #[test]
    fn toggles() {
        assert_eq!(parse_toggle(None, true), Some(false));
        assert_eq!(parse_toggle(Some("вкл"), false), Some(true));
        assert_eq!(parse_toggle(Some("0"), true), Some(false));
        assert_eq!(parse_toggle(Some("абв"), true), None);
    }

    #[test]
    fn speed_needs_free_move() {
        assert!(!Cheats::default().free_move());
        assert!(Cheats { speed: 3.0, ..default() }.free_move());
        assert!(Cheats { fly: true, ..default() }.free_move());
    }

    fn default() -> Cheats {
        Cheats::default()
    }
}
