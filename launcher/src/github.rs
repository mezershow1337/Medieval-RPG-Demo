//! Последняя сборка в GitHub Releases.

use serde::Deserialize;

pub const REPO: &str = "mezershow1337/Medieval-RPG-Demo";
/// Архив с игрой в каждом релизе.
pub const GAME_ASSET: &str = "MedievalRPG-windows.zip";
/// Сам лаунчер в каждом релизе (для самообновления).
pub const LAUNCHER_ASSET: &str = "MedievalRPGLauncher.exe";

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    /// «sha256:…» — GitHub считает сам.
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.eq_ignore_ascii_case(name))
    }
}

pub fn build_number(tag: &str) -> Option<u32> {
    tag.trim().strip_prefix("build-")?.parse().ok()
}

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(60)))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .user_agent("MedievalRPG-Launcher")
        .http_status_as_error(false)
        .build()
        .into()
}

pub fn latest(agent: &ureq::Agent) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("нет связи с GitHub ({e})"))?;
    let status = resp.status().as_u16();
    if status == 403 || status == 429 {
        return Err("GitHub временно ограничил запросы, попробуй через несколько минут".into());
    }
    if status != 200 {
        return Err(format!("GitHub ответил {status}"));
    }
    resp.body_mut()
        .read_json::<Release>()
        .map_err(|e| format!("не понял ответ GitHub ({e})"))
}

/// Описание релиза без разметки Markdown — для окна «Что нового».
pub fn plain_notes(body: &str) -> Vec<(u8, String)> {
    // (уровень, строка): 0 — обычный текст, 1 — пункт списка, 2 — заголовок
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("<!--") || t == "---" {
            continue;
        }
        let (kind, rest) = if let Some(h) = t.strip_prefix('#') {
            (2, h.trim_start_matches('#').trim())
        } else if let Some(li) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            (1, li.trim())
        } else {
            (0, t)
        };
        let clean = strip_inline(rest);
        if !clean.is_empty() {
            out.push((kind, clean));
        }
    }
    out
}

/// Убирает **жирный**, `код` и [ссылки](url) → «ссылки».
fn strip_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '`' => {}
            '[' => {
                let mut text = String::new();
                for c2 in chars.by_ref() {
                    if c2 == ']' {
                        break;
                    }
                    text.push(c2);
                }
                if chars.peek() == Some(&'(') {
                    for c2 in chars.by_ref() {
                        if c2 == ')' {
                            break;
                        }
                    }
                }
                out.push_str(&text);
            }
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_numbers() {
        assert_eq!(build_number("build-147"), Some(147));
        assert_eq!(build_number("v1.0"), None);
    }

    #[test]
    fn notes_without_markdown() {
        let n = plain_notes("## Для игрока\n- **Новое:** [кузня](https://x/y) `стала` лучше\n\n<!-- x -->\nТекст");
        assert_eq!(n[0], (2, "Для игрока".into()));
        assert_eq!(n[1], (1, "Новое: кузня стала лучше".into()));
        assert_eq!(n[2], (0, "Текст".into()));
    }

    #[test]
    fn parse_release_json() {
        let r: Release = serde_json::from_str(
            r#"{"tag_name":"build-5","assets":[{"name":"MedievalRPG-windows.zip","size":10,"browser_download_url":"u","digest":"sha256:ab"}]}"#,
        )
        .unwrap();
        assert_eq!(build_number(&r.tag_name), Some(5));
        assert_eq!(r.asset(GAME_ASSET).unwrap().digest.as_deref(), Some("sha256:ab"));
    }
}
