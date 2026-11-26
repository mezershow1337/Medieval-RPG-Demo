//! Установка и обновление игры.
//!
//! Архив сборки не качается целиком каждый раз: лаунчер читает оглавление zip прямо с
//! GitHub (HTTP Range), сверяет размер и CRC32 каждого файла с тем, что уже лежит в папке,
//! и докачивает только изменившиеся файлы. Если сервер не умеет Range — качает архив целиком.
//! Новые файлы сперва распаковываются во временную папку и проверяются по CRC, и только
//! потом встают на место, поэтому оборванная загрузка не портит установленную игру.
//! settings.txt и crash.txt в архиве нет — их лаунчер не трогает никогда.

use std::cell::Cell;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::github::{Asset, LAUNCHER_ASSET, Release};

pub const GAME_EXE: &str = "MedievalRPG.exe";
const STATE_DIR: &str = ".launcher";
const CHUNK: u64 = 8 << 20;
/// Соседние нужные файлы, между которыми меньше этого, качаются одним запросом.
const MERGE_GAP: u64 = 1 << 20;

/// Счётчики для полосы загрузки.
#[derive(Default)]
pub struct Bytes {
    pub done: AtomicU64,
    pub total: AtomicU64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Comparing,
    Downloading { files: usize },
    Installing,
}

pub struct Ctx<'a> {
    pub agent: &'a ureq::Agent,
    pub dir: &'a Path,
    pub bytes: &'a Arc<Bytes>,
    pub step: &'a dyn Fn(Step),
}

pub fn installed_version(dir: &Path) -> Option<String> {
    let v = fs::read_to_string(dir.join(STATE_DIR).join("version.txt")).ok()?;
    let v = v.trim().to_string();
    (!v.is_empty()).then_some(v)
}

pub fn game_installed(dir: &Path) -> bool {
    dir.join(GAME_EXE).is_file()
}

/// Удаляет хвосты прошлых обновлений (заменённые exe, которые были заняты).
pub fn cleanup(dir: &Path) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "old") {
                let _ = fs::remove_file(&p);
            }
        }
    }
    let _ = fs::remove_dir_all(dir.join(STATE_DIR).join("staging"));
    let _ = fs::remove_file(dir.join(STATE_DIR).join("download.zip"));
}

/// Ставит или обновляет игру до `release`. Возвращает, сколько файлов заменено.
pub fn install(ctx: &Ctx, release: &Release, asset: &Asset) -> Result<usize, String> {
    fs::create_dir_all(ctx.dir.join(STATE_DIR)).map_err(|e| format!("не создать папку: {e}"))?;
    ctx.bytes.done.store(0, Ordering::Relaxed);
    ctx.bytes.total.store(0, Ordering::Relaxed);
    let n = match HttpRange::open(ctx.agent, &asset.browser_download_url, asset.size, ctx.bytes.clone()) {
        Ok(reader) => {
            let hint = reader.hint.clone();
            hint.set(u64::MAX); // оглавление — крупными кусками
            install_from(ctx, reader, Some(hint))?
        }
        Err(RangeError::Unsupported) => {
            let path = ctx.dir.join(STATE_DIR).join("download.zip");
            (ctx.step)(Step::Downloading { files: 0 });
            download_to(ctx.agent, &asset.browser_download_url, asset.size, &path, ctx.bytes)?;
            let file = fs::File::open(&path).map_err(|e| e.to_string())?;
            let n = install_from(ctx, io::BufReader::new(file), None)?;
            let _ = fs::remove_file(&path);
            n
        }
        Err(RangeError::Other(e)) => return Err(e),
    };
    fs::write(ctx.dir.join(STATE_DIR).join("version.txt"), &release.tag_name)
        .map_err(|e| format!("не записать версию: {e}"))?;
    Ok(n)
}

struct Entry {
    name: String,
    rel: String,
    start: u64,
    end: u64,
}

fn install_from<R: Read + Seek>(ctx: &Ctx, mut reader: R, hint: Option<Rc<Cell<u64>>>) -> Result<usize, String> {
    (ctx.step)(Step::Comparing);
    // Оглавление читаем сами: zip-библиотека на каждый файл лезла бы в его локальный заголовок,
    // а это лишний запрос к серверу на каждый из сотен файлов.
    let (cd, cd_start) = read_central_dir(&mut reader).map_err(|e| format!("архив сборки повреждён: {e}"))?;
    let names: Vec<String> = cd.iter().map(|e| e.name.replace('\\', "/")).collect();
    let prefix = common_root(&names);
    let own_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));

    let mut wanted = std::collections::HashSet::new();
    let mut changed = Vec::new();
    for (e, name) in cd.iter().zip(&names) {
        if name.ends_with('/') {
            continue;
        }
        let Some(rel) = safe_rel(name.strip_prefix(&prefix).unwrap_or(name)) else {
            continue;
        };
        let lower = rel.to_lowercase();
        if lower == LAUNCHER_ASSET.to_lowercase() || Some(&lower) == own_exe.as_ref() {
            continue; // лаунчер обновляет себя сам
        }
        wanted.insert(lower);
        if same_file(&ctx.dir.join(&rel), e.size, e.crc) {
            continue;
        }
        changed.push(Entry { name: e.name.clone(), rel, start: e.offset, end: e.offset });
    }
    // Конец записи — начало следующей по порядку в архиве.
    let mut starts: Vec<u64> = cd.iter().map(|e| e.offset).collect();
    starts.sort_unstable();
    for e in &mut changed {
        let next = starts.partition_point(|&s| s <= e.start);
        e.end = starts.get(next).copied().unwrap_or(cd_start);
    }

    changed.sort_by_key(|e| e.start);
    // Конец «пачки» соседних файлов, чтобы качать их одним запросом.
    let mut run_end = vec![0u64; changed.len()];
    let mut i = changed.len();
    let mut cur = 0;
    while i > 0 {
        i -= 1;
        if i + 1 == changed.len() || changed[i + 1].start > changed[i].end + MERGE_GAP {
            cur = changed[i].end;
        }
        run_end[i] = cur.max(changed[i].end);
    }
    if hint.is_some() {
        let total: u64 = changed.iter().map(|e| e.end - e.start).sum();
        ctx.bytes.done.store(0, Ordering::Relaxed);
        ctx.bytes.total.store(total, Ordering::Relaxed);
    }
    (ctx.step)(Step::Downloading { files: changed.len() });

    let staging = ctx.dir.join(STATE_DIR).join("staging");
    let _ = fs::remove_dir_all(&staging);
    let mut zip = if changed.is_empty() {
        None
    } else {
        if let Some(h) = &hint {
            h.set(u64::MAX);
        }
        Some(zip::ZipArchive::new(reader).map_err(|e| format!("архив сборки повреждён: {e}"))?)
    };
    for (k, e) in changed.iter().enumerate() {
        let zip = zip.as_mut().unwrap();
        let index = zip.index_for_name(&e.name).ok_or_else(|| format!("{}: нет в архиве", e.rel))?;
        if let Some(h) = &hint {
            h.set(run_end[k]);
        }
        let mut f = zip.by_index(index).map_err(|err| format!("{}: {err}", e.rel))?;
        let dst = staging.join(&e.rel);
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p).map_err(|err| err.to_string())?;
        }
        let mut out = io::BufWriter::new(fs::File::create(&dst).map_err(|err| format!("{}: {err}", e.rel))?);
        // Чтение до конца проверяет CRC32 — битый файл сюда не пройдёт.
        io::copy(&mut f, &mut out).map_err(|err| format!("{}: {err}", e.rel))?;
        out.flush().map_err(|err| err.to_string())?;
    }
    if let Some(h) = &hint {
        let t = ctx.bytes.total.load(Ordering::Relaxed);
        ctx.bytes.done.store(t, Ordering::Relaxed);
        h.set(0);
    }

    (ctx.step)(Step::Installing);
    for e in &changed {
        replace(&staging.join(&e.rel), &ctx.dir.join(&e.rel))
            .map_err(|err| format!("не заменить {} ({err}). Закрой игру и попробуй снова", e.rel))?;
    }
    let _ = fs::remove_dir_all(&staging);
    remove_stale(&ctx.dir.join("assets"), ctx.dir, &wanted);
    Ok(changed.len())
}

struct CdEntry {
    name: String,
    crc: u32,
    size: u64,
    offset: u64,
}

fn u16le(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn u32le(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn u64le(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}

/// Центральный каталог zip (с поддержкой zip64): имя, CRC32, размер и начало каждой записи.
fn read_central_dir<R: Read + Seek>(r: &mut R) -> io::Result<(Vec<CdEntry>, u64)> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
    let len = r.seek(SeekFrom::End(0))?;
    let tail_len = len.min(65_557);
    r.seek(SeekFrom::Start(len - tail_len))?;
    let mut tail = vec![0u8; tail_len as usize];
    r.read_exact(&mut tail)?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| u32le(&tail, i) == 0x0605_4b50)
        .ok_or_else(|| bad("нет конца каталога"))?;
    let mut count = u16le(&tail, eocd + 10) as u64;
    let mut cd_size = u32le(&tail, eocd + 12) as u64;
    let mut cd_off = u32le(&tail, eocd + 16) as u64;
    if (cd_off == 0xFFFF_FFFF || count == 0xFFFF) && eocd >= 20 && u32le(&tail, eocd - 20) == 0x0706_4b50 {
        let z64 = u64le(&tail, eocd - 20 + 8);
        r.seek(SeekFrom::Start(z64))?;
        let mut h = [0u8; 56];
        r.read_exact(&mut h)?;
        if u32le(&h, 0) != 0x0606_4b50 {
            return Err(bad("битый zip64"));
        }
        count = u64le(&h, 32);
        cd_size = u64le(&h, 40);
        cd_off = u64le(&h, 48);
    }
    if cd_off + cd_size > len {
        return Err(bad("каталог за концом файла"));
    }
    r.seek(SeekFrom::Start(cd_off))?;
    let mut cd = vec![0u8; cd_size as usize];
    r.read_exact(&mut cd)?;
    let mut out = Vec::with_capacity(count as usize);
    let mut i = 0;
    while i + 46 <= cd.len() && u32le(&cd, i) == 0x0201_4b50 {
        let crc = u32le(&cd, i + 16);
        let mut size = u32le(&cd, i + 24) as u64;
        let mut offset = u32le(&cd, i + 42) as u64;
        let comp32 = u32le(&cd, i + 20);
        let (n, x, c) = (u16le(&cd, i + 28) as usize, u16le(&cd, i + 30) as usize, u16le(&cd, i + 32) as usize);
        let end = i + 46 + n + x + c;
        if end > cd.len() {
            return Err(bad("обрезанный каталог"));
        }
        let name = String::from_utf8_lossy(&cd[i + 46..i + 46 + n]).into_owned();
        // zip64: в доп. поле 0x0001 по порядку идут только те значения, что в основном равны 0xFFFFFFFF.
        let mut j = i + 46 + n;
        while j + 4 <= i + 46 + n + x {
            let (id, sz) = (u16le(&cd, j), u16le(&cd, j + 2) as usize);
            if id == 1 {
                let mut k = j + 4;
                if size == 0xFFFF_FFFF && k + 8 <= j + 4 + sz {
                    size = u64le(&cd, k);
                    k += 8;
                }
                if comp32 == 0xFFFF_FFFF && k + 8 <= j + 4 + sz {
                    k += 8;
                }
                if offset == 0xFFFF_FFFF && k + 8 <= j + 4 + sz {
                    offset = u64le(&cd, k);
                }
            }
            j += 4 + sz;
        }
        out.push(CdEntry { name, crc, size, offset });
        i = end;
    }
    Ok((out, cd_off))
}

/// Общая первая папка всех файлов архива («MedievalRPG/»), если она есть.
fn common_root(names: &[String]) -> String {
    let first = match names.first().and_then(|n| n.split_once('/')) {
        Some((root, _)) => format!("{root}/"),
        None => return String::new(),
    };
    if names.iter().all(|n| n.starts_with(&first)) { first } else { String::new() }
}

/// Путь внутри папки игры без «..», дисков и абсолютных путей.
fn safe_rel(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if parts.is_empty() || parts.iter().any(|p| *p == ".." || p.contains(':')) || name.starts_with('/') {
        return None;
    }
    Some(parts.join("/"))
}

fn same_file(path: &Path, size: u64, crc: u32) -> bool {
    let Ok(meta) = fs::metadata(path) else { return false };
    if !meta.is_file() || meta.len() != size {
        return false;
    }
    let Ok(mut f) = fs::File::open(path) else { return false };
    let mut h = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 256 << 10];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(_) => return false,
        }
    }
    h.finalize() == crc
}

/// Ставит файл на место. Занятый файл (запущенный exe) в Windows нельзя удалить, но можно
/// переименовать — он уходит в *.old и удаляется при следующем запуске лаунчера.
fn replace(src: &Path, dst: &Path) -> io::Result<()> {
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p)?;
    }
    if dst.exists() && fs::remove_file(dst).is_err() {
        let old = old_name(dst);
        let _ = fs::remove_file(&old);
        fs::rename(dst, &old)?;
    }
    fs::rename(src, dst)
}

fn old_name(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".old");
    PathBuf::from(s)
}

/// Файлы в assets, которых больше нет в сборке.
fn remove_stale(dir: &Path, root: &Path, wanted: &std::collections::HashSet<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            remove_stale(&p, root, wanted);
            let _ = fs::remove_dir(&p); // только если пустая
        } else if let Ok(rel) = p.strip_prefix(root) {
            let rel = rel.to_string_lossy().replace('\\', "/").to_lowercase();
            if !wanted.contains(&rel) {
                let _ = fs::remove_file(&p);
            }
        }
    }
}

fn download_to(agent: &ureq::Agent, url: &str, size: u64, path: &Path, bytes: &Bytes) -> Result<(), String> {
    bytes.total.store(size, Ordering::Relaxed);
    bytes.done.store(0, Ordering::Relaxed);
    let mut resp = agent.get(url).call().map_err(|e| format!("загрузка прервалась: {e}"))?;
    if resp.status().as_u16() != 200 {
        return Err(format!("сервер ответил {}", resp.status()));
    }
    let mut r = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut out = io::BufWriter::new(fs::File::create(path).map_err(|e| e.to_string())?);
    let mut buf = vec![0u8; 256 << 10];
    loop {
        let n = r.read(&mut buf).map_err(|e| format!("загрузка прервалась: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        bytes.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    out.flush().map_err(|e| e.to_string())
}

enum RangeError {
    Unsupported,
    Other(String),
}

/// Файл на сервере как Read + Seek: читает кусками через HTTP Range.
struct HttpRange {
    agent: ureq::Agent,
    url: String,
    len: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_start: u64,
    /// До какого байта сейчас стоит качать одним запросом (0 — обычный кусок).
    hint: Rc<Cell<u64>>,
    bytes: Arc<Bytes>,
}

impl HttpRange {
    fn open(agent: &ureq::Agent, url: &str, len: u64, bytes: Arc<Bytes>) -> Result<Self, RangeError> {
        let resp = agent
            .get(url)
            .header("Range", "bytes=0-0")
            .call()
            .map_err(|e| RangeError::Other(format!("нет связи с сервером загрузки ({e})")))?;
        match resp.status().as_u16() {
            206 => {}
            200 => return Err(RangeError::Unsupported),
            s => return Err(RangeError::Other(format!("сервер загрузки ответил {s}"))),
        }
        Ok(Self {
            agent: agent.clone(),
            url: url.to_string(),
            len,
            pos: 0,
            buf: Vec::new(),
            buf_start: 0,
            hint: Rc::new(Cell::new(0)),
            bytes,
        })
    }

    fn fetch(&mut self) -> io::Result<()> {
        let hint = self.hint.get();
        let end = if hint > self.pos { hint.min(self.pos + CHUNK) } else { self.pos + (64 << 10) }.min(self.len);
        let mut last = None;
        for attempt in 0..4 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(700 * attempt));
            }
            match self.try_fetch(self.pos, end) {
                Ok(data) => {
                    self.buf = data;
                    self.buf_start = self.pos;
                    return Ok(());
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap())
    }

    fn try_fetch(&self, from: u64, to: u64) -> io::Result<Vec<u8>> {
        if std::env::var_os("MRPG_LAUNCHER_LOG").is_some() {
            eprintln!("range {from}..{to} ({} КБ)", (to - from) >> 10);
        }
        let mut resp = self
            .agent
            .get(&self.url)
            .header("Range", format!("bytes={from}-{}", to - 1))
            .call()
            .map_err(io::Error::other)?;
        if resp.status().as_u16() != 206 {
            return Err(io::Error::other(format!("сервер ответил {}", resp.status())));
        }
        let want = (to - from) as usize;
        let mut data = Vec::with_capacity(want);
        let mut r = resp.body_mut().with_config().limit(u64::MAX).reader();
        let mut tmp = vec![0u8; 128 << 10];
        let counted = self.hint.get() > 0;
        while data.len() < want {
            let n = r.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&tmp[..n]);
            if counted {
                self.bytes.done.fetch_add(n as u64, Ordering::Relaxed);
            }
        }
        if data.len() != want {
            if counted {
                self.bytes.done.fetch_sub(data.len() as u64, Ordering::Relaxed);
            }
            return Err(io::Error::other("загрузка оборвалась"));
        }
        Ok(data)
    }
}

impl Read for HttpRange {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || out.is_empty() {
            return Ok(0);
        }
        let in_buf = self.pos >= self.buf_start && self.pos < self.buf_start + self.buf.len() as u64;
        if !in_buf {
            self.fetch()?;
        }
        let off = (self.pos - self.buf_start) as usize;
        let n = out.len().min(self.buf.len() - off);
        out[..n].copy_from_slice(&self.buf[off..off + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpRange {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let p = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if p < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 << 10];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Если в релизе лаунчер новее — скачивает его, встаёт на место старого и перезапускается.
/// Возвращает true, если новый лаунчер уже запущен и этот должен закрыться.
pub fn self_update(agent: &ureq::Agent, release: &Release) -> Result<bool, String> {
    if !cfg!(windows) || std::env::var_os("MRPG_LAUNCHER_NO_SELF_UPDATE").is_some() {
        return Ok(false);
    }
    let Some(asset) = release.asset(LAUNCHER_ASSET) else { return Ok(false) };
    let Some(want) = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) else {
        return Ok(false);
    };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if sha256_file(&exe).map_err(|e| e.to_string())?.eq_ignore_ascii_case(want) {
        return Ok(false);
    }
    let new = exe.with_extension("new");
    let bytes = Bytes::default();
    download_to(agent, &asset.browser_download_url, asset.size, &new, &bytes)?;
    if !sha256_file(&new).map_err(|e| e.to_string())?.eq_ignore_ascii_case(want) {
        let _ = fs::remove_file(&new);
        return Err("новый лаунчер скачался с ошибкой".into());
    }
    replace(&new, &exe).map_err(|e| format!("не заменить лаунчер: {e}"))?;
    std::process::Command::new(&exe)
        .arg("--updated")
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map_err(|e| format!("не запустить новый лаунчер: {e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_paths_are_safe() {
        assert_eq!(safe_rel("assets/ui/a.png").as_deref(), Some("assets/ui/a.png"));
        assert_eq!(safe_rel("../evil.exe"), None);
        assert_eq!(safe_rel("C:/Windows/x"), None);
        assert_eq!(safe_rel("/etc/x"), None);
    }

    #[test]
    fn root_folder_is_stripped() {
        let n = vec!["MedievalRPG/MedievalRPG.exe".to_string(), "MedievalRPG/assets/a".to_string()];
        assert_eq!(common_root(&n), "MedievalRPG/");
        let m = vec!["a.exe".to_string(), "assets/b".to_string()];
        assert_eq!(common_root(&m), "");
    }

    /// Полный цикл на локальном архиве: установка, повтор без изменений, точечное обновление,
    /// удаление устаревших assets, сохранность settings.txt.
    #[test]
    fn install_and_update_from_zip() {
        let root = std::env::temp_dir().join(format!("mrpg-launcher-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("settings.txt"), "name=Ахи").unwrap();
        let make_zip = |files: &[(&str, &str)]| {
            let mut cur = io::Cursor::new(Vec::new());
            let mut w = zip::ZipWriter::new(&mut cur);
            for (n, d) in files {
                w.start_file(format!("MedievalRPG/{n}"), zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(d.as_bytes()).unwrap();
            }
            w.finish().unwrap();
            cur.into_inner()
        };
        let agent = crate::github::agent();
        let bytes = Arc::new(Bytes::default());
        let step = |_s: Step| {};
        let ctx = Ctx { agent: &agent, dir: &game, bytes: &bytes, step: &step };

        let v1 = make_zip(&[("MedievalRPG.exe", "exe1"), ("assets/a.txt", "A"), ("assets/old.txt", "O")]);
        assert_eq!(install_from(&ctx, io::Cursor::new(v1.clone()), None).unwrap(), 3);
        assert_eq!(install_from(&ctx, io::Cursor::new(v1), None).unwrap(), 0);

        let v2 = make_zip(&[("MedievalRPG.exe", "exe2"), ("assets/a.txt", "A"), ("MedievalRPGLauncher.exe", "L")]);
        assert_eq!(install_from(&ctx, io::Cursor::new(v2), None).unwrap(), 1);
        assert_eq!(fs::read_to_string(game.join(GAME_EXE)).unwrap(), "exe2");
        assert!(!game.join("assets/old.txt").exists());
        assert!(!game.join("MedievalRPGLauncher.exe").exists());
        assert_eq!(fs::read_to_string(game.join("settings.txt")).unwrap(), "name=Ахи");
        let _ = fs::remove_dir_all(&root);
    }
}
