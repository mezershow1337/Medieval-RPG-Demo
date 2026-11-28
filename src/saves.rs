//! Сохранение и загрузка игры: слоты на диске хоста и запись персонажей (общее для сервера и окна).
//!
//! # Что хранится
//!
//! * **Мир** — то же, что и раньше писалось само в `herson-world.json`: торговые города, стройки,
//!   усадьбы игроков и вещи в их сундуках ([`crate::towns_server`]).
//! * **Персонажи** — `herson-characters.json`: у каждого игрока (по постоянному ключу профиля, см.
//!   [`crate::estates::Profile`]) сумка и надетое, деньги, навыки, грамота и прочие знания,
//!   прочитанные книги, репутация у фракций и долг-штраф, раны и яды, клейма кузнеца,
//!   место, где стоял, и здоровье. Плюс время суток мира.
//!
//! Оба файла — «текущая игра»: сервер обновляет их сам (персонажей — раз в полминуты и когда
//! игрок выходит), поэтому «Создать игру» продолжает с того же места.
//!
//! **Слот** — папка `saves/slot-N` с копией обоих файлов и описанием (`meta.json`). «Сохранить»
//! записывает в слот свежий снимок, «Загрузить» кладёт файлы слота на место текущей игры и
//! запускает хост заново (у друзей связь обрывается — им нужно подключиться ещё раз).
//!
//! Вещи пишутся по названию, а не по номеру в таблице предметов: так сохранение переживает
//! новые предметы в обновлениях. Знания и раны разбираются по одной записи — неизвестная
//! новой версии игры запись просто пропускается, а не ломает весь файл.

use crate::estates::data_dir;
use crate::inventory::{Inventory, PLAYER_GRID, Placed};
use crate::items::{EquipSlot, Item, ItemBaseId, UidGen};
use crate::lore::{Book, Knowledge, Lore, SKILL_COUNT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Слот быстрого сохранения (F5).
pub const QUICK_SLOT: u8 = 0;
/// Обычные слоты: 1..=SLOTS.
pub const SLOTS: u8 = 6;

const META: &str = "meta.json";
const WORLD: &str = "world.json";
const CHARACTERS: &str = "characters.json";

// ---------------------------------------------------------------------------
// Пути
// ---------------------------------------------------------------------------

/// Файл мира текущей игры (`MRPG_SAVE=off` — не сохранять, `MRPG_SAVE=путь` — свой файл).
pub fn world_path() -> Option<PathBuf> {
    match std::env::var("MRPG_SAVE").ok().as_deref() {
        Some("off") | Some("0") => None,
        Some(p) if !p.is_empty() => Some(p.into()),
        _ => Some(data_dir().join("herson-world.json")),
    }
}

/// Файл персонажей текущей игры — рядом с файлом мира.
pub fn characters_path() -> Option<PathBuf> {
    let world = world_path()?;
    let stem = world.file_stem().and_then(|s| s.to_str()).unwrap_or("herson-world");
    let name = match stem.strip_suffix("-world") {
        Some(base) => format!("{base}-characters.json"),
        None => format!("{stem}-characters.json"),
    };
    Some(world.with_file_name(name))
}

pub fn saves_dir() -> PathBuf {
    data_dir().join("saves")
}

pub fn slot_dir(slot: u8) -> PathBuf {
    saves_dir().join(if slot == QUICK_SLOT { "quick".to_string() } else { format!("slot-{slot}") })
}

pub fn slot_name(slot: u8) -> String {
    if slot == QUICK_SLOT { "Быстрое сохранение".into() } else { format!("Слот {slot}") }
}

/// Записать файл целиком: сначала во временный, потом переименовать (не оставит половину файла,
/// если игру закроют посреди записи).
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

// ---------------------------------------------------------------------------
// Описание слота
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SlotMeta {
    pub version: u32,
    /// Когда сохранили (секунды UNIX).
    pub saved_at: u64,
    /// Дата в мире («вторая неделя октября 510 г.»).
    pub date: String,
    /// Время суток «14:30».
    #[serde(default)]
    pub time: String,
    /// Кто был в игре (первым — хозяин).
    #[serde(default)]
    pub players: Vec<String>,
    /// Сколько денег у хозяина, текстом.
    #[serde(default)]
    pub purse: String,
    /// Уровень (Херсонес или замок).
    #[serde(default)]
    pub level: String,
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// «только что», «5 мин назад», «3 ч назад», «2 дн. назад».
pub fn ago_text(saved_at: u64, now: u64) -> String {
    let s = now.saturating_sub(saved_at);
    match s {
        0..60 => "только что".into(),
        60..3600 => format!("{} мин назад", s / 60),
        3600..86_400 => format!("{} ч назад", s / 3600),
        _ => format!("{} дн. назад", s / 86_400),
    }
}

/// Описание слота с диска (`None` — слот пуст или испорчен).
pub fn read_meta(slot: u8) -> Option<SlotMeta> {
    read_meta_in(&slot_dir(slot))
}

fn read_meta_in(dir: &Path) -> Option<SlotMeta> {
    if !dir.join(WORLD).exists() && !dir.join(CHARACTERS).exists() {
        return None;
    }
    std::fs::read_to_string(dir.join(META)).ok().and_then(|s| serde_json::from_str(&s).ok())
}

/// Все слоты по порядку: быстрый, затем 1..=SLOTS.
pub fn all_slots() -> Vec<(u8, Option<SlotMeta>)> {
    std::iter::once(QUICK_SLOT).chain(1..=SLOTS).map(|s| (s, read_meta(s))).collect()
}

/// Записать слот: мир (уже готовый текст), персонажи и описание.
pub fn write_slot(dir: &Path, world: Option<&str>, characters: &str, meta: &SlotMeta) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    match world {
        Some(w) => write_atomic(&dir.join(WORLD), w)?,
        None => {
            let _ = std::fs::remove_file(dir.join(WORLD));
        }
    }
    write_atomic(&dir.join(CHARACTERS), characters)?;
    write_atomic(&dir.join(META), &serde_json::to_string_pretty(meta).unwrap_or_default())
}

/// Положить файлы слота на место текущей игры (вызывать до запуска сервера).
pub fn load_slot(slot: u8) -> Result<SlotMeta, String> {
    let (Some(world), Some(chars)) = (world_path(), characters_path()) else {
        return Err("сохранения выключены (MRPG_SAVE=off)".into());
    };
    load_slot_into(&slot_dir(slot), &world, &chars).map_err(|e| format!("{}: {e}", slot_name(slot)))
}

fn load_slot_into(dir: &Path, world: &Path, chars: &Path) -> Result<SlotMeta, String> {
    let meta = read_meta_in(dir).ok_or("слот пуст")?;
    for (from, to) in [(dir.join(WORLD), world), (dir.join(CHARACTERS), chars)] {
        match std::fs::read_to_string(&from) {
            Ok(text) => write_atomic(to, &text).map_err(|e| e.to_string())?,
            // В слоте нет мира (сохраняли на замке) — текущий мир не трогаем.
            Err(_) => {}
        }
    }
    Ok(meta)
}

/// Новый мир с нуля: текущие файлы игры уезжают в `saves/backup-<время>` (на всякий случай).
pub fn start_new_world() -> Result<(), String> {
    let (Some(world), Some(chars)) = (world_path(), characters_path()) else { return Ok(()) };
    let backup = saves_dir().join(format!("backup-{}", now_unix()));
    let mut moved = false;
    for (from, name) in [(world, WORLD), (chars, CHARACTERS)] {
        if from.exists() {
            std::fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
            std::fs::rename(&from, backup.join(name)).or_else(|_| std::fs::copy(&from, backup.join(name)).map(|_| ()).and_then(|_| std::fs::remove_file(&from))).map_err(|e| e.to_string())?;
            moved = true;
        }
    }
    if moved {
        let meta = SlotMeta { version: 1, saved_at: now_unix(), date: "копия перед новым миром".into(), ..Default::default() };
        let _ = write_atomic(&backup.join(META), &serde_json::to_string_pretty(&meta).unwrap_or_default());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Персонаж
// ---------------------------------------------------------------------------

/// Файл персонажей: по постоянному ключу профиля.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CharactersFile {
    pub version: u32,
    /// Часы мира (игровые секунды), чтобы после загрузки был тот же час.
    #[serde(default)]
    pub clock: Option<f64>,
    #[serde(default)]
    pub characters: BTreeMap<u64, CharacterSave>,
}

impl CharactersFile {
    pub fn read(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str(&text) {
            Ok(f) => Some(f),
            Err(e) => {
                bevy::log::warn!("Не прочитать персонажей из {}: {e}", path.display());
                None
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct CharacterSave {
    pub name: String,
    /// Где стоял (ступни) и куда смотрел. Нет — погиб или ещё не входил: место выберет сервер.
    #[serde(default)]
    pub pos: Option<[f32; 3]>,
    #[serde(default)]
    pub yaw: f32,
    /// На каком уровне стоял (место на другом уровне не годится).
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub hp: Option<f32>,
    #[serde(default)]
    pub inventory: Option<SavedInventory>,
    /// Знания и навыки (разбираются по записи, см. [`save_knowledge`]).
    #[serde(default)]
    pub knowledge: Option<Value>,
    /// Репутация по фракциям ([`crate::factions::Faction::index`]).
    #[serde(default)]
    pub reputation: Option<Vec<f32>>,
    /// Неуплаченный штраф, в долях.
    #[serde(default)]
    pub fine: u32,
    /// Клейма мастера и заказ у кузнечной гильдии.
    #[serde(default)]
    pub guild: Option<Value>,
    /// Раны, яды, кривые кости.
    #[serde(default)]
    pub body: Option<Value>,
}

/// Вещь по названию (номер вида в таблице меняется от версии к версии).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SavedItem {
    pub name: String,
    pub item: Item,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SavedInventory {
    pub grid: Vec<(u8, u8, SavedItem)>,
    pub equipment: Vec<(EquipSlot, SavedItem)>,
}

fn saved_item(item: &Item) -> SavedItem {
    SavedItem { name: item.info().name.to_string(), item: item.clone() }
}

/// Вещь из сохранения: вид — по названию, номер — новый. Неизвестная вещь пропадает.
fn restore_item(s: &SavedItem, uids: &mut UidGen) -> Option<Item> {
    let base = ItemBaseId::find(&s.name)?;
    let mut item = s.item.clone();
    item.base = base;
    item.uid = uids.next();
    item.count = item.count.max(1);
    Some(item)
}

pub fn save_inventory(inv: &Inventory) -> SavedInventory {
    SavedInventory {
        grid: inv.grid.items.iter().map(|p| (p.x, p.y, saved_item(&p.item))).collect(),
        equipment: EquipSlot::ALL.iter().filter_map(|s| inv.equipped(*s).map(|i| (*s, saved_item(i)))).collect(),
    }
}

pub fn restore_inventory(s: &SavedInventory, uids: &mut UidGen) -> Inventory {
    let mut inv = Inventory::new(PLAYER_GRID);
    let mut loose = Vec::new();
    for (slot, si) in &s.equipment {
        if let Some(item) = restore_item(si, uids) {
            inv.equipment[slot.index()] = Some(item);
        }
    }
    for (x, y, si) in &s.grid {
        let Some(item) = restore_item(si, uids) else { continue };
        let p = Placed { item, x: *x, y: *y };
        // Вещь по-прежнему влезает на своё место — кладём туда, иначе — куда найдётся.
        if inv.grid.fits(p.item.size(), p.x, p.y) {
            inv.grid.items.push(p);
        } else {
            loose.push(p.item);
        }
    }
    for item in loose {
        let _ = inv.place(item, crate::inventory::Target::Auto);
    }
    inv
}

/// Знания в виде, который понимает JSON (ключи словарей — строки, поэтому списки пар).
#[derive(Serialize)]
struct KnowledgeOut<'a> {
    known: Vec<&'a Lore>,
    skills: Vec<f32>,
    lessons: Vec<(&'a Lore, f32)>,
    read: Vec<(&'a Book, f32)>,
    copied: Vec<(&'a Book, f32)>,
}

pub fn save_knowledge(k: &Knowledge) -> Value {
    let out = KnowledgeOut {
        known: k.known.iter().collect(),
        skills: k.skills.to_vec(),
        lessons: k.lessons.iter().map(|(l, p)| (l, *p)).collect(),
        read: k.read.iter().map(|(b, p)| (b, *p)).collect(),
        copied: k.copied.iter().map(|(b, p)| (b, *p)).collect(),
    };
    serde_json::to_value(out).unwrap_or(Value::Null)
}

/// Разобрать знания по записи: незнакомое этой версии игры пропускаем.
pub fn restore_knowledge(v: &Value) -> Knowledge {
    let mut k = Knowledge::default();
    let list = |key: &str| v.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    for e in list("known") {
        if let Ok(l) = serde_json::from_value::<Lore>(e) {
            k.known.insert(l);
        }
    }
    for (i, e) in list("skills").iter().enumerate().take(SKILL_COUNT) {
        k.skills[i] = e.as_f64().unwrap_or(0.0).clamp(0.0, 100.0) as f32;
    }
    for e in list("lessons") {
        if let Ok((l, p)) = serde_json::from_value::<(Lore, f32)>(e) {
            k.lessons.insert(l, p);
        }
    }
    for e in list("read") {
        if let Ok((b, p)) = serde_json::from_value::<(Book, f32)>(e) {
            k.read.insert(b, p);
        }
    }
    for e in list("copied") {
        if let Ok((b, p)) = serde_json::from_value::<(Book, f32)>(e) {
            k.copied.insert(b, p);
        }
    }
    k
}

/// Тело: раны и кривые кости — по одной, остальное целиком.
pub fn save_body(b: &crate::medicine::Body) -> Option<Value> {
    if *b == crate::medicine::Body::default() {
        return None;
    }
    let mut v = serde_json::to_value(b).ok()?;
    // Яды — словарь с ключами-перечислениями; на всякий случай — списком пар.
    if let Some(obj) = v.as_object_mut() {
        obj.insert("toxins".into(), serde_json::to_value(b.toxins.iter().collect::<Vec<_>>()).unwrap_or(Value::Null));
    }
    Some(v)
}

pub fn restore_body(v: &Value) -> Option<crate::medicine::Body> {
    let mut v = v.clone();
    let obj = v.as_object_mut()?;
    let wounds: Vec<Value> = obj.get("wounds").and_then(Value::as_array).cloned().unwrap_or_default();
    let crooked: Vec<Value> = obj.get("crooked").and_then(Value::as_array).cloned().unwrap_or_default();
    let toxins: Vec<Value> = obj.get("toxins").and_then(Value::as_array).cloned().unwrap_or_default();
    obj.insert("wounds".into(), Value::Array(vec![]));
    obj.insert("crooked".into(), Value::Array(vec![]));
    obj.insert("toxins".into(), Value::Object(Default::default()));
    let mut body: crate::medicine::Body = serde_json::from_value(v).ok()?;
    body.wounds = wounds.into_iter().filter_map(|w| serde_json::from_value(w).ok()).collect();
    body.crooked = crooked.into_iter().filter_map(|c| serde_json::from_value(c).ok()).collect();
    body.toxins = toxins.into_iter().filter_map(|t| serde_json::from_value(t).ok()).collect();
    Some(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrpg-saves-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_inventory() -> (Inventory, UidGen) {
        let mut world = crate::loot_server::LootWorld::default();
        let mut kit = crate::start_kit::StartKit::default();
        kit.step(crate::start_kit::KitField::Weapon, 4);
        let inv = world.test_starter(kit);
        (inv, world.uids)
    }

    #[test]
    fn inventory_round_trip_keeps_items_and_places() {
        let (inv, mut uids) = sample_inventory();
        let saved = save_inventory(&inv);
        let json = serde_json::to_string(&saved).unwrap();
        let back: SavedInventory = serde_json::from_str(&json).unwrap();
        let restored = restore_inventory(&back, &mut uids);
        let names = |i: &Inventory| {
            let mut v: Vec<(String, u16)> = i.all_items().map(|it| (it.info().name.to_string(), it.count)).collect();
            v.sort();
            v
        };
        assert_eq!(names(&restored), names(&inv));
        assert_eq!(restored.equipped(EquipSlot::MainHand).map(|i| i.info().name), Some("Пехотное копьё"));
        assert_eq!(crate::economy::purse(&restored), crate::economy::purse(&inv));
        // Номера вещей новые (не совпадают со старыми).
        let old: Vec<u64> = inv.all_items().map(|i| i.uid).collect();
        assert!(restored.all_items().all(|i| !old.contains(&i.uid)));
    }

    #[test]
    fn unknown_items_are_dropped_not_fatal() {
        let (inv, mut uids) = sample_inventory();
        let mut saved = save_inventory(&inv);
        let n = saved.grid.len();
        saved.grid[0].2.name = "Вещь из будущей версии".into();
        let restored = restore_inventory(&saved, &mut uids);
        assert_eq!(restored.grid.items.len(), n - 1);
    }

    #[test]
    fn knowledge_round_trip_and_unknown_entries() {
        let mut k = Knowledge::default();
        k.learn(Lore::Alphabet);
        k.learn(Lore::Read(crate::lore::Language::Greek));
        k.skills[0] = 42.5;
        k.lessons.insert(Lore::Grammar, 0.4);
        k.read.insert(Book::Dioscorides, 0.25);
        let mut v = save_knowledge(&k);
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("Alphabet"));
        assert_eq!(restore_knowledge(&serde_json::from_str(&json).unwrap()), k);
        // Неведомое знание из другой версии пропускается, остальное цело.
        v["known"].as_array_mut().unwrap().push(Value::String("FutureLore".into()));
        v["skills"].as_array_mut().unwrap().push(Value::from(10.0));
        assert_eq!(restore_knowledge(&v), k);
    }

    #[test]
    fn body_round_trip() {
        let mut b = crate::medicine::Body::default();
        assert!(save_body(&b).is_none(), "здоровое тело не пишем");
        b.toxins.insert(crate::medicine::Toxin::Opium, 0.5);
        b.antidote = 0.3;
        let v = save_body(&b).unwrap();
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(restore_body(&serde_json::from_str(&json).unwrap()), Some(b));
    }

    #[test]
    fn characters_file_round_trip() {
        let (inv, _) = sample_inventory();
        let mut f = CharactersFile { version: 1, clock: Some(12.0 * 3600.0), characters: BTreeMap::new() };
        f.characters.insert(
            0xDEAD_BEEF_0000_0001,
            CharacterSave { name: "Феодор".into(), pos: Some([1.0, 2.0, 3.0]), yaw: 0.5, hp: Some(80.0), inventory: Some(save_inventory(&inv)), reputation: Some(vec![1.0; 9]), fine: 40, ..Default::default() },
        );
        let json = serde_json::to_string(&f).unwrap();
        let back: CharactersFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.clock, f.clock);
        assert_eq!(back.characters, f.characters);
    }

    #[test]
    fn slot_write_and_load() {
        let dir = temp_dir("slot");
        let slot = dir.join("slot-1");
        let meta = SlotMeta { version: 1, saved_at: 100, date: "первая неделя сентября 510 г.".into(), players: vec!["Феодор".into()], ..Default::default() };
        write_slot(&slot, Some("{\"w\":1}"), "{\"c\":2}", &meta).unwrap();
        assert_eq!(read_meta_in(&slot), Some(meta.clone()));
        let world = dir.join("live/herson-world.json");
        let chars = dir.join("live/herson-characters.json");
        assert_eq!(load_slot_into(&slot, &world, &chars), Ok(meta));
        assert_eq!(std::fs::read_to_string(&world).unwrap(), "{\"w\":1}");
        assert_eq!(std::fs::read_to_string(&chars).unwrap(), "{\"c\":2}");
        assert!(load_slot_into(&dir.join("slot-2"), &world, &chars).is_err(), "пустой слот не грузится");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ago_texts() {
        assert_eq!(ago_text(100, 130), "только что");
        assert_eq!(ago_text(0, 300), "5 мин назад");
        assert_eq!(ago_text(0, 7200), "2 ч назад");
        assert_eq!(ago_text(0, 3 * 86_400), "3 дн. назад");
    }
}
