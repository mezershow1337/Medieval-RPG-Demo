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
}
#[cfg(test)]
mod print_rain {
    #[test]
    fn print_rain_days() {
        for d in 0..12 {
            eprintln!("day {d}: {:?}", super::rain_window(d));
        }
    }
}
