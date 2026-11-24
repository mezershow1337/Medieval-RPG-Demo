//! Часы мира: время суток и погода, общие для сервера и всех игроков.
//!
//! Сервер ведёт часы и раз в несколько секунд рассылает их; клиент идёт сам и подтягивается
//! к серверу. Время в игре идёт в [`TIME_SCALE`] раз быстрее настоящего (сутки — 72 минуты,
//! как в больших ролевых играх). По часам живут жители (`daily_life.rs`): спят, работают,
//! ходят на рынок, в церковь и в таверну. Погода тоже выводится из часов (одинаково у всех),
//! поэтому её не нужно передавать по сети.
//!
//! Начальный час можно задать переменной окружения `MRPG_HOUR` (например, `MRPG_HOUR=21.5`).

use bevy::prelude::*;

/// Во сколько раз игровое время быстрее настоящего.
pub const TIME_SCALE: f64 = 20.0;
/// Секунд в сутках.
pub const DAY: f64 = 86_400.0;
/// Как часто сервер рассылает часы (настоящие секунды).
const SYNC_EVERY: f32 = 5.0;

/// Игровое время: секунды от полуночи первого дня.
#[derive(Resource, Clone, Copy, Debug)]
pub struct WorldClock {
    pub game: f64,
    /// Клиент: получил ли часы от сервера.
    pub synced: bool,
}

impl Default for WorldClock {
    fn default() -> Self {
        // С `MRPG_HOUR` часы идут и без сервера (снимки города с камеры ожидания).
        Self { game: start_hour() as f64 * 3600.0, synced: std::env::var("MRPG_HOUR").is_ok() }
    }
}

fn start_hour() -> f32 {
    // Больше 24 — следующие сутки (`MRPG_HOUR=40` — 16:00 второго дня).
    std::env::var("MRPG_HOUR").ok().and_then(|h| h.parse::<f32>().ok()).map_or(7.5, |h| h.max(0.0))
}

impl WorldClock {
    pub fn day(&self) -> u32 {
        (self.game / DAY).floor().max(0.0) as u32
    }

    /// Секунды от полуночи текущих суток.
    pub fn of_day(&self) -> f64 {
        self.game.rem_euclid(DAY)
    }

    pub fn hour(&self) -> f32 {
        (self.of_day() / 3600.0) as f32
    }

    /// «ЧЧ:ММ».
    pub fn hhmm(&self) -> String {
        let m = (self.of_day() / 60.0) as u32;
        format!("{:02}:{:02}", m / 60, m % 60)
    }

    pub fn night(&self) -> bool {
        is_night(self.hour())
    }

    pub fn rain(&self) -> f32 {
        rain_at(self.game)
    }

    pub fn weather(&self) -> Weather {
        weather_at(self.game)
    }

    /// Темнота 0..1 (см. [`darkness`]).
    pub fn darkness(&self) -> f32 {
        darkness(self.game)
    }
}

/// Тёмное время: от заката до рассвета (летом в Херсонесе — около 21 и 5 часов).
pub fn is_night(hour: f32) -> bool {
    !(5.0..21.0).contains(&hour)
}

/// Как называли час сами херсониты (церковные часы и время дня).
pub fn part_of_day(hour: f32) -> &'static str {
    match hour {
        h if h < 4.5 => "глубокая ночь",
        h if h < 6.5 => "рассвет, утреня",
        h if h < 9.0 => "утро",
        h if h < 12.0 => "день, третий час",
        h if h < 14.0 => "полдень, шестой час",
        h if h < 17.0 => "после полудня, девятый час",
        h if h < 19.0 => "вечер, вечерня",
        h if h < 21.0 => "сумерки",
        _ => "ночь",
    }
}

/// Простой детерминированный хэш (одинаковый у сервера и клиентов).
pub fn hash(a: u64, b: u64) -> u64 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_add(0x632B_E59B_D9B4_E019).rotate_left(23);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 32)
}

/// Число 0..1 из хэша.
pub fn unit(a: u64, b: u64) -> f32 {
    (hash(a, b) >> 40) as f32 / (1u64 << 24) as f32
}

/// Дождь в этот день: (начало, конец) в часах, если будет. Херсонес — сухое побережье:
/// дождливых дней около четверти, дождь идёт несколько часов.
pub fn rain_window(day: u32) -> Option<(f32, f32)> {
    if day == 0 || unit(day as u64, 0xA1) > 0.27 {
        return None;
    }
    let start = 6.0 + 13.0 * unit(day as u64, 0xA2);
    let len = 1.5 + 3.5 * unit(day as u64, 0xA3);
    Some((start, (start + len).min(23.5)))
}

/// Сила дождя 0..1 в момент `game` (с плавным началом и концом).
pub fn rain_at(game: f64) -> f32 {
    let day = (game / DAY).floor().max(0.0) as u32;
    let h = (game.rem_euclid(DAY) / 3600.0) as f32;
    let Some((a, b)) = rain_window(day) else { return 0.0 };
    if h < a || h > b {
        return 0.0;
    }
    let edge = ((h - a).min(b - h) / 0.4).clamp(0.0, 1.0);
    edge * (0.6 + 0.4 * unit(day as u64, 0xA4))
}

/// Идёт ли дождь в этот день в промежутке часов [from, to).
pub fn rains_during(day: u32, from: f32, to: f32) -> bool {
    rain_window(day).is_some_and(|(a, b)| a < to && b > from)
}

// ---------- Погода ----------

/// Погода в один момент: одинакова у сервера и всех игроков (выводится из часов).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weather {
    /// Сила дождя 0..1.
    pub rain: f32,
    /// Облачность 0..1 (1 — сплошная пелена туч).
    pub clouds: f32,
    /// Туман 0..1 (утренняя морская мгла, сырость после дождя).
    pub fog: f32,
    /// Ветер: куда дует, м/с (в плоскости X–Z).
    pub wind: Vec2,
}

impl Weather {
    pub fn wind_speed(&self) -> f32 {
        self.wind.length()
    }
}

/// Шаг ключевых точек облачности и ветра (6 часов): между ними — плавный переход.
const WEATHER_STEP: f64 = 6.0 * 3600.0;

fn keyed(game: f64, f: impl Fn(u64) -> f32) -> f32 {
    let k = (game / WEATHER_STEP).floor();
    let t = ((game / WEATHER_STEP) - k) as f32;
    let s = t * t * (3.0 - 2.0 * t);
    let a = f(k.max(0.0) as u64);
    let b = f(k.max(0.0) as u64 + 1);
    a + (b - a) * s
}

/// Насколько небо затянуто перед дождём, во время и после него (0..1).
fn rain_overcast(game: f64) -> f32 {
    let day = (game / DAY).floor().max(0.0) as i64;
    let mut best = 0.0f32;
    for d in (day - 1)..=(day + 1) {
        if d < 0 {
            continue;
        }
        let Some((a, b)) = rain_window(d as u32) else { continue };
        let (a, b) = (d as f64 * DAY + a as f64 * 3600.0, d as f64 * DAY + b as f64 * 3600.0);
        let v = if game < a {
            1.0 - ((a - game) / (2.0 * 3600.0)) as f32
        } else if game > b {
            1.0 - ((game - b) / (1.5 * 3600.0)) as f32
        } else {
            1.0
        };
        best = best.max(v.clamp(0.0, 1.0));
    }
    best
}

/// Погода в момент `game`. Херсонес — сухое солнечное побережье: чаще ясно или лёгкие
/// кучевые облака; изредка утренняя морская мгла; дождь (см. [`rain_window`]) приходит с
/// тучами и ветром с моря.
pub fn weather_at(game: f64) -> Weather {
    let rain = rain_at(game);
    let over = rain_overcast(game);
    let base = keyed(game, |k| 0.08 + 0.55 * unit(k, 0xB1).powf(1.4));
    let clouds = base.max(0.95 * over * over.sqrt());
    // Морская мгла: в иные дни с ночи до позднего утра (пик к рассвету).
    let day = (game / DAY).floor().max(0.0) as u32;
    let h = (game.rem_euclid(DAY) / 3600.0) as f32;
    let mist_day = unit(day as u64, 0xC1) < 0.22;
    let mist = if mist_day {
        let strength = 0.55 + 0.45 * unit(day as u64, 0xC2);
        let rise = ((h - 1.5) / 3.0).clamp(0.0, 1.0);
        let fall = ((10.5 - h) / 3.5).clamp(0.0, 1.0);
        strength * rise.min(fall)
    } else {
        0.0
    };
    let fog = mist.max(0.35 * over * rain.max(0.4 * over));
    // Ветер: направление и сила плавно меняются; в непогоду крепчает.
    let ang = keyed(game, |k| unit(k, 0xD1) * std::f32::consts::TAU * 3.0);
    let speed = keyed(game, |k| 1.0 + 6.0 * unit(k, 0xD2).powi(2)) + 5.0 * over * (0.5 + rain);
    // В мгле ветра почти нет.
    let speed = speed * (1.0 - 0.7 * mist);
    Weather { rain, clouds: clouds.clamp(0.0, 1.0), fog: fog.clamp(0.0, 1.0), wind: Vec2::new(ang.cos(), ang.sin()) * speed }
}

// ---------- Солнце и луна ----------

/// Широта Херсонеса.
const LATITUDE: f32 = 44.6;
/// Склонение солнца в начале осени (календарь мира начинается с 1 сентября).
const SUN_DECLINATION: f32 = 12.0;
/// Местный солнечный полдень, часы.
const SOLAR_NOON: f32 = 12.8;
/// Лунный месяц, сутки.
const LUNAR_MONTH: f64 = 29.53;
/// Возраст луны в первый день (растущая, почти полная — первые ночи светлые).
const MOON_AGE0: f64 = 10.5;

/// Направление на светило по часовому углу (часы от его кульминации) и склонению.
/// Мир: +X — восток, −Z — север (море), +Y — вверх.
fn body_dir(hours_from_noon: f32, declination: f32) -> Vec3 {
    let h = (hours_from_noon * 15.0).to_radians();
    let (phi, d) = (LATITUDE.to_radians(), declination.to_radians());
    let up = phi.sin() * d.sin() + phi.cos() * d.cos() * h.cos();
    let east = -d.cos() * h.sin();
    let north = phi.cos() * d.sin() - phi.sin() * d.cos() * h.cos();
    Vec3::new(east, up, -north).normalize()
}

/// Направление на солнце в момент `game`.
pub fn sun_dir(game: f64) -> Vec3 {
    let h = (game.rem_euclid(DAY) / 3600.0) as f32;
    body_dir(h - SOLAR_NOON, SUN_DECLINATION)
}

/// Возраст луны (сутки от новолуния).
pub fn moon_age(game: f64) -> f64 {
    (MOON_AGE0 + game / DAY).rem_euclid(LUNAR_MONTH)
}

/// Освещённая часть диска луны 0..1.
pub fn moon_light(game: f64) -> f32 {
    let a = moon_age(game) / LUNAR_MONTH;
    (0.5 * (1.0 - (a * std::f64::consts::TAU).cos())) as f32
}

/// Направление на луну: каждый день она восходит позже (в полнолуние — напротив солнца).
pub fn moon_dir(game: f64) -> Vec3 {
    let h = (game.rem_euclid(DAY) / 3600.0) as f32;
    let lag = (moon_age(game) / LUNAR_MONTH * 24.0) as f32;
    body_dir(h - SOLAR_NOON - lag, -SUN_DECLINATION * 0.6)
}

/// Насколько светло от солнца 0..1 (0 — ночь, 1 — день; сумерки — плавно).
pub fn daylight(game: f64) -> f32 {
    let e = sun_dir(game).y;
    ((e + 0.1) / 0.3).clamp(0.0, 1.0)
}

/// Темно ли так, что вора без огня видно лишь вблизи (0 — светло, 1 — глубокая ночь;
/// луна и тучи учтены).
pub fn darkness(game: f64) -> f32 {
    let w = weather_at(game);
    let moon = if moon_dir(game).y > 0.05 { moon_light(game) * (1.0 - 0.85 * w.clouds) } else { 0.0 };
    let night = 1.0 - daylight(game);
    (night * (1.0 - 0.35 * moon)).clamp(0.0, 1.0)
}

// ---------- Сервер ----------

pub struct ClockServerPlugin;

impl Plugin for ClockServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldClock>().add_observer(clock_on_connect).add_systems(PreUpdate, tick_server_clock);
    }
}

fn tick_server_clock(time: Res<Time>, mut clock: ResMut<WorldClock>, mut server: Option<ResMut<bevy_renet::RenetServer>>, mut next: Local<f32>) {
    clock.game += time.delta_secs_f64() * TIME_SCALE;
    clock.synced = true;
    let now = time.elapsed_secs();
    if now >= *next {
        *next = now + SYNC_EVERY;
        if let Some(s) = server.as_deref_mut() {
            s.broadcast_message(crate::protocol::CH_RELIABLE, crate::protocol::encode(&crate::protocol::ServerMessage::Clock { game: clock.game }));
        }
    }
}

fn clock_on_connect(event: On<bevy_renet::RenetServerEvent>, mut server: ResMut<bevy_renet::RenetServer>, clock: Res<WorldClock>) {
    let bevy_renet::renet::ServerEvent::ClientConnected { client_id } = &event.0 else { return };
    server.send_message(*client_id, crate::protocol::CH_RELIABLE, crate::protocol::encode(&crate::protocol::ServerMessage::Clock { game: clock.game }));
}

// ---------- Клиент ----------

pub struct ClockClientPlugin;

impl Plugin for ClockClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldClock>().add_message::<ClockSync>().add_systems(PreUpdate, tick_client_clock);
    }
}

/// Часы сервера (пересылает `client::receive_reliable`).
#[derive(Message, Clone, Copy, Debug)]
pub struct ClockSync(pub f64);

fn tick_client_clock(time: Res<Time>, mut clock: ResMut<WorldClock>, mut sync: MessageReader<ClockSync>) {
    clock.game += time.delta_secs_f64() * TIME_SCALE;
    for s in sync.read() {
        let diff = s.0 - clock.game;
        // Большое расхождение (вход в игру, пропуск времени) — сразу; малое — мягко, чтобы
        // идущие люди не дёргались.
        if !clock.synced || diff.abs() > 120.0 {
            clock.game = s.0;
        } else {
            clock.game += diff * 0.5;
        }
        clock.synced = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_and_days() {
        let c = WorldClock { game: DAY * 2.0 + 13.5 * 3600.0, synced: true };
        assert_eq!(c.day(), 2);
        assert!((c.hour() - 13.5).abs() < 1e-4);
        assert_eq!(c.hhmm(), "13:30");
        assert!(!c.night());
        assert!(WorldClock { game: 23.0 * 3600.0, synced: true }.night());
    }

    #[test]
    fn rain_is_rare_and_smooth() {
        let rainy = (0..400).filter(|d| rain_window(*d).is_some()).count();
        assert!((40..160).contains(&rainy), "дождливых дней {rainy} из 400");
        let (d, (a, b)) = (0..400).find_map(|d| rain_window(d).map(|w| (d, w))).unwrap();
        let mid = d as f64 * DAY + ((a + b) * 0.5) as f64 * 3600.0;
        assert!(rain_at(mid) > 0.3);
        assert_eq!(rain_at(d as f64 * DAY + (a as f64 - 0.5) * 3600.0), 0.0);
    }

    /// Подбор моментов для снимков: `cargo test weather_samples -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn weather_samples() {
        for d in 1..40u32 {
            for h in [6.5f32, 9.0, 13.0, 16.0, 22.0] {
                let g = d as f64 * DAY + h as f64 * 3600.0;
                let w = weather_at(g);
                if w.rain > 0.5 || w.fog > 0.5 || w.clouds > 0.8 {
                    println!("{:6.2} ч: дождь {:.2} тучи {:.2} туман {:.2} ветер {:.1}", g / 3600.0, w.rain, w.clouds, w.fog, w.wind_speed());
                }
            }
            let m = moon_light(d as f64 * DAY);
            println!("сутки {d}: луна {m:.2}, в полночь на высоте {:.2}", moon_dir(d as f64 * DAY + 0.5 * 3600.0).y);
        }
    }

    #[test]
    fn sun_rises_in_the_east_and_sets_in_the_west() {
        let at = |h: f32| sun_dir(DAY * 2.0 + h as f64 * 3600.0);
        assert!(at(13.0).y > 0.75, "полдень: {:?}", at(13.0));
        // В полдень солнце на юге (+Z).
        assert!(at(12.8).z > 0.4);
        assert!(at(7.0).x > 0.5 && at(7.0).y > 0.0, "утро на востоке: {:?}", at(7.0));
        assert!(at(19.0).x < -0.5 && at(19.0).y > 0.0, "вечер на западе: {:?}", at(19.0));
        assert!(at(23.5).y < -0.3 && at(2.0).y < -0.3);
        // Восход около шести, закат около восьми вечера.
        let rise = (40..90).map(|i| i as f32 * 0.1).find(|h| at(*h).y > 0.0).unwrap();
        let set = (170..220).map(|i| i as f32 * 0.1).find(|h| at(*h).y < 0.0).unwrap();
        assert!((5.5..6.8).contains(&rise) && (19.0..20.3).contains(&set), "восход {rise}, закат {set}");
        assert!(daylight(DAY + 13.0 * 3600.0) > 0.99 && daylight(DAY + 1.0 * 3600.0) < 0.01);
    }

    #[test]
    fn moon_waxes_and_wanes() {
        let full = (0..30).map(|d| moon_light(d as f64 * DAY)).fold(0.0f32, f32::max);
        let new = (0..30).map(|d| moon_light(d as f64 * DAY)).fold(1.0f32, f32::min);
        assert!(full > 0.95 && new < 0.05);
        // Первые ночи — светлые: луна высоко около полуночи.
        assert!(moon_dir(DAY * 1.0 + 0.5 * 3600.0).y > 0.2, "{:?}", moon_dir(DAY + 1800.0));
        assert!(darkness(DAY * 2.0 + 13.0 * 3600.0) < 0.01);
        assert!(darkness(DAY * 2.0 + 1.0 * 3600.0) > 0.5);
    }

    #[test]
    fn weather_is_smooth_and_follows_rain() {
        let mut prev = weather_at(0.0);
        for i in 1..(20 * 24 * 12) {
            let w = weather_at(i as f64 * 300.0);
            assert!((w.clouds - prev.clouds).abs() < 0.08, "облака скачут в {i}");
            assert!((w.wind - prev.wind).length() < 2.5, "ветер скачет в {i}");
            assert!((0.0..=1.0).contains(&w.fog) && (0.0..=1.0).contains(&w.clouds));
            prev = w;
        }
        let (d, (a, b)) = (1..400).find_map(|d| rain_window(d).map(|w| (d, w))).unwrap();
        let mid = weather_at(d as f64 * DAY + ((a + b) * 0.5) as f64 * 3600.0);
        assert!(mid.rain > 0.3 && mid.clouds > 0.85, "{mid:?}");
        let foggy = (0..60).filter(|d| weather_at(*d as f64 * DAY + 6.0 * 3600.0).fog > 0.4).count();
        assert!((3..30).contains(&foggy), "туманных утр {foggy} из 60");
    }
}
