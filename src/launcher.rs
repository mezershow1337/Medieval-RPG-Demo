//! Текстовое меню для готовой сборки (MedievalRPG.exe, запущенный двойным щелчком):
//! «Создать игру» или «Подключиться к другу» — без командной строки и без cargo.

use crate::protocol::DEFAULT_PORT;
use std::io::{BufRead, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;

/// Что выбрал игрок в меню.
pub enum Choice {
    Host,
    Join(SocketAddr),
}

/// Открыть ли меню: exe запущен без параметров и не через `cargo run`
/// (cargo выставляет запущенной программе переменную CARGO) и есть консоль для ввода.
pub fn wanted() -> bool {
    use std::io::IsTerminal;
    std::env::var_os("CARGO").is_none() && std::env::var_os("MRPG_NO_MENU").is_none() && std::io::stdin().is_terminal()
}

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
fn clean(s: &str) -> String {
    let s = s.trim().trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>').trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    s.trim_end_matches('/').to_string()
}

/// Файл с последним адресом — рядом с exe, чтобы в следующий раз просто нажать Enter.
fn last_address_file() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("last_address.txt"))
}

fn read_line() -> String {
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
        // Ввод закрыт — дальше спрашивать бессмысленно.
        std::process::exit(0);
    }
    line.trim().to_string()
}

fn prompt(text: &str) -> String {
    print!("{text}");
    let _ = std::io::stdout().flush();
    read_line()
}

/// Подождать Enter перед выходом, чтобы окно консоли не закрылось раньше, чем человек прочтёт текст.
pub fn pause() {
    print!("\nНажмите Enter, чтобы закрыть окно…");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
}

/// При падении игры окно консоли не исчезает сразу: видно, что случилось.
pub fn keep_console_on_panic() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        if std::thread::current().name() == Some("main") {
            eprintln!("\nИгра остановилась из-за ошибки (текст выше).");
            pause();
        }
    }));
}

pub fn run_menu() -> Choice {
    println!("==============================================");
    println!("   Medieval RPG — Херсонес, 510 год");
    println!("==============================================");
    println!();
    println!("  1 — Создать игру (ты хост, друзья подключаются к тебе)");
    println!("  2 — Подключиться к другу");
    println!();
    loop {
        match prompt("Выбери 1 или 2 и нажми Enter: ").as_str() {
            "1" => {
                println!();
                println!("Игра создаётся на UDP-порту {DEFAULT_PORT}.");
                println!("Друзьям дай адрес своего туннеля playit.gg (вида xxx.at.ply.gg:12345)");
                println!("или свой IP, если порт {DEFAULT_PORT}/UDP открыт на роутере.");
                println!("Не закрывай это окно, пока играешь.");
                println!();
                return Choice::Host;
            }
            "2" => return Choice::Join(ask_address()),
            _ => println!("Нужно ввести цифру 1 или 2."),
        }
    }
}

fn ask_address() -> SocketAddr {
    let file = last_address_file();
    let last = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    println!();
    println!("Адрес друга, например  abc.at.ply.gg:12345  или  192.168.1.5");
    loop {
        let hint = last.as_ref().map(|l| format!(" [Enter — {l}]")).unwrap_or_default();
        let mut input = prompt(&format!("Адрес{hint}: "));
        if input.is_empty() {
            match &last {
                Some(l) => input = l.clone(),
                None => continue,
            }
        }
        println!("Ищу {}…", clean(&input));
        match resolve(&input) {
            Ok(addr) => {
                if let Some(f) = &file {
                    let _ = std::fs::write(f, clean(&input));
                }
                println!("Подключаюсь к {addr}. Окно игры сейчас откроется.");
                println!("Не закрывай это окно, пока играешь.");
                println!();
                return addr;
            }
            Err(e) => println!("Не получилось: {e}. Проверь адрес и попробуй ещё раз."),
        }
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
