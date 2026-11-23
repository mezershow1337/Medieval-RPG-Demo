//! Запуск без консоли: разбор адреса друга и отчёт о падении.
//! Само меню «Создать игру / Подключиться» — в окне игры (`menu.rs`).

use crate::protocol::DEFAULT_PORT;
use std::net::{SocketAddr, ToSocketAddrs};

/// Разобрать адрес «хост[:порт]»: порт по умолчанию 5000, имена вроде abc.at.ply.gg тоже годятся.
pub fn resolve(s: &str) -> Result<SocketAddr, String> {
    let s = clean(s);
    if s.is_empty() {
        return Err("адрес пустой".into());
    }
    let with_port = if s.contains(':') { s.clone() } else { format!("{s}:{DEFAULT_PORT}") };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("не удалось найти «{s}»: {e}"))?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("у «{s}» нет IPv4-адреса"))
}

/// Убрать то, что люди часто копируют вместе с адресом: пробелы, кавычки, «udp://», слэш в конце.
pub fn clean(s: &str) -> String {
    let s = s.trim().trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>').trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    s.trim_end_matches('/').to_string()
}

/// Готовая сборка запускается без консоли, поэтому при падении пишем отчёт в
/// `crash.txt` рядом с exe и (в Windows) показываем окно с текстом ошибки.
pub fn install_crash_report() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        let text = format!("Игра остановилась из-за ошибки (поток «{thread}»):\n\n{info}\n\nОтправьте этот текст разработчику.");
        if let Some(path) = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("crash.txt"))) {
            let _ = std::fs::write(&path, &text);
        }
        #[cfg(windows)]
        windows::message_box("Medieval RPG — ошибка", &text);
    }));
}

/// В Windows готовый exe — «оконное» приложение без консоли. Если его запустили из
/// терминала с параметрами (`server`, `join …`), подключаемся к консоли родителя, чтобы был виден журнал.
pub fn attach_parent_console() {
    #[cfg(windows)]
    windows::attach_parent_console();
}

#[cfg(windows)]
mod windows {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut core::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    pub fn attach_parent_console() {
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        // SAFETY: простой вызов WinAPI без указателей.
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }

    pub fn message_box(caption: &str, text: &str) {
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (t, c) = (wide(text), wide(caption));
        const MB_ICONERROR: u32 = 0x10;
        // SAFETY: строки оканчиваются нулём и живут до конца вызова.
        unsafe { MessageBoxW(core::ptr::null_mut(), t.as_ptr(), c.as_ptr(), MB_ICONERROR) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_pasted_addresses() {
        assert_eq!(clean("  \"abc.at.ply.gg:12345\" "), "abc.at.ply.gg:12345");
        assert_eq!(clean("udp://1.2.3.4:5000/"), "1.2.3.4:5000");
    }

    #[test]
    fn default_port_is_added() {
        assert_eq!(resolve("127.0.0.1").unwrap(), SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)));
        assert_eq!(resolve("127.0.0.1:7777").unwrap().port(), 7777);
        assert!(resolve("   ").is_err());
    }
}
