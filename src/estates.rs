//! Свои дома: покупка, аренда, замки, сундуки и обстановка (общее для сервера и клиента).
//!
//! # Как это было около 510 года
//!
//! Дом в провинциальном городе продавали по записи у табулария (городского нотария) в присутствии
//! свидетелей; выморочное имущество (без наследников) отходило городу или церкви и продавалось
//! с торгов. Цены по папирусам VI века: половина небольшого дома — 3–5 солидов, целый дом
//! в городе — от 8 до 30 и больше; комнату или дом снимали помесячно. Ключи и замки — бронзовые
//! и железные (римские замки с бородчатым ключом).
//!
//! # Как это в игре
//!
//! * Пустующие усадьбы и новые дома со строек продаёт курия Херсона: «U» у входа — окно усадьбы.
//! * Купить — навсегда; снять — на месяцы вперёд. Кончилась плата — дом снова свободен, а что
//!   осталось в сундуках, достанется следующему хозяину; поэтому о конце аренды предупреждают заранее.
//! * Двери и сундуки чужого дома не открываются. Хозяин может пустить друзей.
//! * Комнаты обставляют мастера: плотник, гончар, ткач — за деньги и через пару недель.
//! * Всё это хранится на диске хоста и переживает перезапуск сервера. Игрока узнают по
//!   постоянному ключу из файла профиля (`profile.txt` в папке данных игры).

use crate::level::{EstateSpec, FurnishKind};
use crate::towns::{TownId, TownState, TownView};
use serde::{Deserialize, Serialize};

/// Канал renet для городов и домов (надёжный, по порядку). Настроен в `protocol::connection_config`.
pub const CH_TOWN: u8 = 13;

/// Сколько недель в месяце аренды.
pub const RENT_WEEKS: u32 = 4;

// ---------------------------------------------------------------------------
// Профиль игрока: постоянный ключ и имя
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub key: u64,
    pub name: String,
    /// Облик персонажа (выбран в главном меню, хранится в settings.txt).
    pub look: crate::appearance::Look,
    /// Стартовый набор (оружие и доспех на выбор), выдаётся при входе и после смерти.
    pub kit: crate::start_kit::StartKit,
}

/// Папка данных игры: `%APPDATA%\MedievalRPG` в Windows, `~/.local/share/medieval_rpg` в Linux.
pub fn data_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("MRPG_DATA_DIR") {
        return dir.into();
    }
    if let Some(app) = std::env::var_os("APPDATA") {
        return std::path::Path::new(&app).join("MedievalRPG");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return std::path::Path::new(&home).join(".local/share/medieval_rpg");
    }
    std::path::PathBuf::from("saves")
}

/// Профиль этого компьютера (создаётся при первом запуске). `MRPG_PROFILE=имя` — второй профиль
/// на том же компьютере (например, чтобы зайти вторым игроком для проверки).
/// `MRPG_START_KIT=4,2,0,0,0,0` — стартовый набор без меню (для снимков: 4 — пехотное копьё).
pub fn local_profile() -> Profile {
    let mut p = stored_profile();
    if let Some(kit) = std::env::var("MRPG_START_KIT").ok().and_then(|s| crate::start_kit::StartKit::from_text(&s)) {
        p.kit = kit;
    }
    p
}

fn stored_profile() -> Profile {
    let suffix = std::env::var("MRPG_PROFILE").ok().filter(|s| !s.is_empty());
    let file = data_dir().join(match &suffix {
        Some(s) => format!("profile-{s}.txt"),
        None => "profile.txt".into(),
    });
    if let Ok(text) = std::fs::read_to_string(&file)
        && let Some(p) = parse_profile(&text)
    {
        return p;
    }
    let key = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) ^ (std::process::id() as u64) << 32;
    let base = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "Путник".into());
    let name = match &suffix {
        Some(s) => format!("{base} ({s})"),
        None => base,
    };
    let p = Profile { key: key.max(1), name: clean_name(&name), look: Default::default(), kit: Default::default() };
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(&file, format!("key={:016x}\nname={}\n", p.key, p.name));
    p
}

fn parse_profile(text: &str) -> Option<Profile> {
    let mut key = None;
    let mut name = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("key=") {
            key = u64::from_str_radix(v.trim(), 16).ok();
        } else if let Some(v) = line.strip_prefix("name=") {
            name = Some(clean_name(v));
        }
    }
    Some(Profile { key: key.filter(|k| *k != 0)?, name: name.unwrap_or_else(|| "Путник".into()), look: Default::default(), kit: Default::default() })
}

/// Имя без управляющих символов, не длиннее 32 символов.
pub fn clean_name(s: &str) -> String {
    let n: String = s.trim().chars().filter(|c| !c.is_control()).take(32).collect();
    if n.is_empty() { "Путник".into() } else { n }
}

/// Упаковать профиль в «пользовательские данные» билета подключения netcode (256 байт).
pub fn pack_profile(p: &Profile) -> [u8; 256] {
    let mut out = [0u8; 256];
    out[..4].copy_from_slice(b"MRPG");
    out[4..12].copy_from_slice(&p.key.to_le_bytes());
    let name = p.name.as_bytes();
    let n = name.len().min(200);
    // Не режем букву пополам.
    let n = (0..=n).rev().find(|&k| p.name.is_char_boundary(k)).unwrap_or(0);
    out[12] = n as u8;
    out[13..13 + n].copy_from_slice(&name[..n]);
    // Облик — в хвосте блока (имя занимает не дальше 213-го байта).
    out[LOOK_AT..LOOK_AT + 7].copy_from_slice(&p.look.pack());
    out[KIT_AT..KIT_AT + 6].copy_from_slice(&p.kit.pack());
    out
}

/// Где в профиле подключения лежит облик.
const LOOK_AT: usize = 240;
/// И стартовый набор.
const KIT_AT: usize = 248;

pub fn unpack_profile(data: &[u8; 256]) -> Option<Profile> {
    if &data[..4] != b"MRPG" {
        return None;
    }
    let key = u64::from_le_bytes(data[4..12].try_into().ok()?);
    let n = (data[12] as usize).min(200);
    let name = String::from_utf8_lossy(&data[13..13 + n]).to_string();
    let look = crate::appearance::Look::unpack(&data[LOOK_AT..LOOK_AT + 7]);
    let kit = crate::start_kit::StartKit::unpack(&data[KIT_AT..KIT_AT + 6]);
    (key != 0).then(|| Profile { key, name: clean_name(&name), look, kit })
}

// ---------------------------------------------------------------------------
// Цены
// ---------------------------------------------------------------------------

/// Цена усадьбы (нуммии): по величине дома и по тому, как теснится и богатеет город.
pub fn estate_price(spec: &EstateSpec, town: &TownState) -> u32 {
    let rooms = spec.rooms.len() as f32;
    let base_solidi = match spec.town {
        TownId::Herson => 4.0 + 1.7 * rooms + if spec.two_storey { 5.0 } else { 0.0 } + if spec.site.is_some() { 2.0 } else { 0.0 },
        _ => 1.5 + 0.6 * rooms,
    };
    let demand = (0.75 + 0.6 * (town.crowding() - 0.85)).clamp(0.7, 1.5);
    let mood = 0.85 + 0.3 * town.mood;
    let plague = if town.plague() { 0.8 } else { 1.0 };
    let nummi = base_solidi * crate::economy::NUMMI_PER_SOLIDUS as f32 * demand * mood * plague;
    // Круглые суммы: до тремисса (треть солида).
    ((nummi / 2400.0).round().max(1.0) * 2400.0) as u32
}

/// Плата за месяц (4 недели) аренды: около 1/90 цены, до фоллиса.
pub fn rent_price(sale: u32) -> u32 {
    let n = sale as f32 / 90.0;
    ((n / crate::economy::NUMMI_PER_FOLLIS as f32).round().max(1.0) * crate::economy::NUMMI_PER_FOLLIS as f32) as u32
}

/// Сколько курия даёт, если продать дом обратно.
pub fn buyback_price(sale: u32) -> u32 {
    crate::economy::round_price(sale as f32 * 0.7)
}

// ---------------------------------------------------------------------------
// Состояние усадьбы
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// На месте дома ещё стройка.
    Building,
    ForSale,
    Owned,
    /// Снят до недели `until`.
    Rented { until: u32 },
}

/// Коротко об усадьбе — рассылается всем (для подсказок у входа и замков).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EstateBrief {
    pub id: u16,
    pub status: Status,
    pub owner: String,
    /// Кто из игроков в сети может входить (номера подключений).
    pub access: Vec<u64>,
    /// Цена покупки и месяца аренды (нуммии).
    pub price: u32,
    pub rent: u32,
    /// Обстановка по комнатам.
    pub furnish: Vec<Option<FurnishKind>>,
}

/// Комната в окне усадьбы.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RoomView {
    pub now: Option<FurnishKind>,
    pub kinds: Vec<FurnishKind>,
    /// Заказано у мастеров: что и через сколько недель будет готово.
    pub order: Option<(FurnishKind, u32)>,
    /// Наверху.
    pub upper: bool,
}

/// Окно усадьбы.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EstateDetail {
    pub brief: EstateBrief,
    pub name: String,
    pub town: TownId,
    pub rooms: Vec<RoomView>,
    pub chests: u32,
    /// Вы хозяин (или съёмщик).
    pub mine: bool,
    /// Гости хозяина (имена).
    pub guests: Vec<String>,
    /// Кого из игроков в сети можно пустить: (номер подключения, имя, уже пущен).
    pub players: Vec<(u64, String, bool)>,
    pub buyback: u32,
    /// Сколько недель аренды осталось.
    pub weeks_left: Option<u32>,
    pub date: String,
}

// ---------------------------------------------------------------------------
// Сообщения
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TownClientMessage {
    /// Открыть окно усадьбы.
    Open { estate: u16 },
    Buy { estate: u16 },
    /// Снять на `months` месяцев (продлить, если уже снята вами).
    Rent { estate: u16, months: u8 },
    /// Продать курии (или отказаться от аренды).
    Sell { estate: u16 },
    /// Заказать обстановку комнаты (`None` — вынести всё и освободить комнату).
    Furnish { estate: u16, room: u8, kind: Option<FurnishKind> },
    /// Пустить или не пускать игрока (номер подключения).
    Guest { estate: u16, player: u64, allow: bool },
    /// Окно «Вести».
    News,
}

/// Вести для окна «Вести».
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NewsView {
    pub date: String,
    pub month: String,
    pub sea_open: bool,
    pub towns: Vec<TownView>,
    /// Новые сверху: (дата, поселение, текст).
    pub news: Vec<(String, TownId, String)>,
    /// Цены в лавках Херсона по видам товара (название, множитель).
    pub market: Vec<(String, f32)>,
    /// Дома на продажу и в аренду: (номер, название, цена, аренда, состояние).
    pub estates: Vec<(u16, String, u32, u32, Status, String)>,
    /// Стройки Херсона: стадия каждой площадки (0..=4).
    pub sites: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TownServerMessage {
    /// При подключении: неделя, стадии строек, все усадьбы.
    Init { week: u32, date: String, sites: Vec<u8>, estates: Vec<EstateBrief>, you: u64 },
    /// Началась новая неделя.
    Week { week: u32, date: String },
    Site { site: u16, stage: u8 },
    Estate(EstateBrief),
    Detail(EstateDetail),
    News(NewsView),
    /// Глашатай на агоре: важная весть всем.
    Crier(String),
    /// Ответ на действие (купил, не хватает денег…).
    Notice(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_round_trip() {
        let p = Profile { key: 0x1234_5678_9abc_def0, name: "Гагик из Херсона".into(), look: crate::appearance::Look::PLAYER_DEFAULT, kit: crate::start_kit::StartKit { weapon: 4, shield: 2, chest: 1, ..Default::default() } };
        assert_eq!(unpack_profile(&pack_profile(&p)), Some(p.clone()));
        // В profile.txt облика нет (он в settings.txt).
        assert_eq!(parse_profile(&format!("key={:016x}\nname={}\n", p.key, p.name)), Some(Profile { look: Default::default(), kit: Default::default(), ..p }));
        assert!(unpack_profile(&[0u8; 256]).is_none());
        // Очень длинное имя режется по границе буквы.
        let long = Profile { key: 5, name: "Ж".repeat(40), look: Default::default(), kit: Default::default() };
        let back = unpack_profile(&pack_profile(&long)).unwrap();
        assert!(back.name.chars().all(|c| c == 'Ж'));
    }

    #[test]
    fn houses_cost_like_in_papyri() {
        let cat = crate::level::estates();
        let town = TownState::new(TownId::Herson);
        for e in cat.estates.iter().filter(|e| e.town == TownId::Herson) {
            let p = estate_price(e, &town);
            let sol = p as f32 / crate::economy::NUMMI_PER_SOLIDUS as f32;
            assert!((5.0..=40.0).contains(&sol), "{}: {sol} солидов", e.name);
            let rent = rent_price(p);
            assert!(rent >= 400 && rent < p / 50, "аренда {rent} при цене {p}");
        }
        let hut = cat.estates.iter().find(|e| e.town == TownId::GothVillage).expect("мазанка у готов");
        let p = estate_price(hut, &TownState::new(TownId::GothVillage));
        assert!(p <= 5 * crate::economy::NUMMI_PER_SOLIDUS, "мазанка {p}");
    }
}
