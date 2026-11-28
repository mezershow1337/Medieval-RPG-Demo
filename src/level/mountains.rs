//! Горы к югу от Херсонеса — настоящие, по ним можно ходить.
//!
//! Раньше горы были только картинкой на небе (`mountain_map.rs`): панорама считала луч по
//! карте высот `mountain_height` в шейдере `sky_common.wesl`, а карта кончалась невидимой
//! стеной в 900 м от города. Теперь та же карта высот считается и здесь, точь-в-точь как в
//! шейдере (тот же шум, те же числа), и поднимает землю на юге: предгорья, которые видно из
//! города, — те самые, по которым идёшь. Панорама рисует горы от игрока и только дальше края
//! прорисовки (или края карты), так что склон перед тобой и гряда за ним — одна земля.
//!
//! Как в Крыму: за плато Гераклейского полуострова — лесистые предгорья с гривами и балками
//! (здесь — до 400–500 м над морем), дальше — Главная гряда; на неё уже не подняться: туда
//! карта не доходит, она остаётся на панораме.

use bevy::prelude::*;

/// Направление «в горы» (от моря вглубь суши). То же, что `MOUNT_L` в шейдере.
pub const MOUNT_L: Vec2 = Vec2::new(0.2873, 0.9578);
/// Высота главных вершин, м (`MOUNT_H`).
const MOUNT_H: f32 = 4600.0;
/// Предгорья начинаются в стольких метрах от города (по `MOUNT_L`) и набирают полную высоту к
/// `FOOT_U1` (`FOOT_U0`, `FOOT_U1` в шейдере).
pub const FOOT_U0: f32 = 1100.0;
pub const FOOT_U1: f32 = 2300.0;
/// Высота гребней предгорий, м (`FOOT_H`).
const FOOT_H: f32 = 600.0;
/// Плато, на котором стоят предгорья (`MOUNT_BASE`): средняя высота земли на юге карты.
pub const MOUNT_BASE: f32 = 43.0;

/// Где кончается земля, по которой можно ходить (невидимая стена), — на юге.
pub const WORLD_SOUTH: f32 = 2700.0;
/// Докуда на юг строится меш земли: за стеной склоны продолжаются ещё на триста метров —
/// с края карты не видно обрыва в небо, а дальше горы рисует панорама.
pub const MESH_SOUTH: f32 = 3000.0;

fn fract(x: f32) -> f32 {
    x - x.floor()
}

/// `hash21` из `sky_common.wesl`.
fn hash21(p: Vec2) -> f32 {
    let mut q = Vec2::new(fract(p.x * 123.34), fract(p.y * 456.21));
    let d = q.dot(q + Vec2::splat(45.32));
    q += Vec2::splat(d);
    fract(q.x * q.y)
}

/// `vnoise` из `sky_common.wesl`.
pub fn vnoise(p: Vec2) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec2::splat(3.0) - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + Vec2::new(1.0, 0.0));
    let c = hash21(i + Vec2::new(0.0, 1.0));
    let d = hash21(i + Vec2::new(1.0, 1.0));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    lerp(lerp(a, b, u.x), lerp(c, d, u.x), u.y)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `mountain_ridged`: гребни (1 − |2n − 1|)², слоями.
fn ridged(p0: Vec2, octs: i32) -> f32 {
    let (mut h, mut a, mut sum, mut p) = (0.0, 0.5, 0.0, p0);
    for i in 0..octs {
        let r = 1.0 - (2.0 * vnoise(p + Vec2::new(17.1, 9.3) * i as f32) - 1.0).abs();
        h += a * r * r;
        sum += a;
        // Каждый слой повёрнут, как в шейдере: гребни не ложатся по осям сетки шума.
        p = Vec2::new(0.8 * p.x - 0.6 * p.y, 0.6 * p.x + 0.8 * p.y) * 2.07 + Vec2::new(3.1, 1.7);
        a *= 0.5;
    }
    h / sum
}

/// «Огибающая» гор на расстоянии `u` от города по `MOUNT_L`: предгорья и главная гряда.
fn envelope(u: f32) -> f32 {
    MOUNT_H * smoothstep(3600.0, 8200.0, u) * (1.0 - 0.35 * smoothstep(15000.0, 30000.0, u)) + FOOT_H * smoothstep(FOOT_U0, FOOT_U1, u)
}

/// `mountain_height` из шейдера: высота гор над плато в точке `p` (X/Z), `octs` — слоёв шума.
pub fn mountain_height(p: Vec2, octs: i32) -> f32 {
    let u = p.dot(MOUNT_L);
    let env = envelope(u);
    if env <= 0.0 {
        return 0.0;
    }
    let q = p / 9000.0;
    let warp = (Vec2::new(vnoise(q + Vec2::new(3.3, 0.0)), vnoise(q + Vec2::new(0.0, 7.1))) - Vec2::splat(0.5)) * 0.875;
    let r = ridged(p / 3600.0 + warp, octs);
    env * (0.32 + 0.68 * r.powf(1.1))
}

/// Насколько горы поднимают землю в точке (м), с рельефом, которого на панораме нет:
/// гривки, промоины и куэсты — длинные известняковые уступы Внутренней гряды (пологий склон
/// с севера, обрыв к югу, местами разорванный — там проходы). Ноль у города и на плато.
pub fn lift(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let u = p.dot(MOUNT_L);
    if u < FOOT_U0 {
        return 0.0;
    }
    let base = mountain_height(p, 5);
    // Мелкий рельеф растёт вместе с горами (у их подножия — почти ничего).
    let k = smoothstep(FOOT_U0, FOOT_U0 + 500.0, u);
    let fine = (super::terrain::fbm(x * 0.011, z * 0.011, 301) - 0.47) * 18.0 + (super::terrain::fbm(x * 0.035, z * 0.035, 307) - 0.47) * 5.0;
    // Гривы и лощины в полкилометра. Шум повёрнут и изогнут: иначе гребни ложатся по осям
    // его сетки — прямыми линиями с севера на юг и с запада на восток.
    let rot = Vec2::new(0.8776, 0.4794);
    let q = Vec2::new(p.x * rot.x - p.y * rot.y, p.x * rot.y + p.y * rot.x) / 700.0;
    let warp = Vec2::new(vnoise(q * 0.45 + Vec2::new(1.7, 8.1)), vnoise(q * 0.45 + Vec2::new(6.3, 2.9))) - Vec2::splat(0.5);
    let rib = ridged(q + warp * 1.4 + Vec2::new(5.1, 2.3), 3);
    // Куэсты вырастают вместе с горами (у подножия их нет — иначе ступенька на границе).
    let cu = smoothstep(FOOT_U0, FOOT_U0 + 300.0, u) * (cuesta(p, u, 1850.0, 75.0, 1.0) + cuesta(p, u, 2450.0, 60.0, 2.0));
    (base + k * (fine + (rib - 0.45) * 70.0) + cu).max(0.0)
}

/// Куэста с бровкой на расстоянии `at` (по `MOUNT_L`) и высотой обрыва `high`: земля
/// поднимается к бровке почти километр и обрывается за ней. Линия бровки извилистая, обрыв
/// местами сходит на нет.
fn cuesta(p: Vec2, u: f32, at: f32, high: f32, seed: f32) -> f32 {
    let v = p.dot(Vec2::new(-MOUNT_L.y, MOUNT_L.x));
    let up = u + (vnoise(Vec2::new(v / 600.0 + seed, seed * 3.1)) - 0.5) * 520.0 + (vnoise(Vec2::new(v / 170.0 + seed * 2.0, seed * 5.1)) - 0.5) * 90.0;
    let gap = smoothstep(0.22, 0.62, vnoise(Vec2::new(v / 520.0 + seed * 7.7, seed * 1.3)));
    let rise = ((up - (at - 900.0)) / 900.0).clamp(0.0, 1.0).powf(1.6);
    // Обрыв почти отвесный: известняковая стена в два-три десятка метров и выше.
    let drop = 1.0 - smoothstep(at - 3.0, at + 12.0, up);
    high * gap * rise * drop
}

/// Высота земли предгорий, как её видит панорама (`mountain_ground` в шейдере).
pub fn panorama_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    mountain_height(p, 5) + MOUNT_BASE * smoothstep(FOOT_U0 - 300.0, FOOT_U0 + 300.0, p.dot(MOUNT_L))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Числа шума те же, что в шейдере (проверено по нескольким значениям, посчитанным вручную
    /// по формуле `hash21`), и горы растут вглубь суши.
    #[test]
    fn mountains_rise_inland() {
        assert_eq!(mountain_height(Vec2::new(0.0, 0.0), 5), 0.0);
        assert_eq!(lift(0.0, 900.0), 0.0);
        let mut sum = [0.0f32; 3];
        for k in 0..200 {
            let x = -900.0 + k as f32 * 9.0;
            for (i, z) in [1300.0f32, 1900.0, 2600.0].into_iter().enumerate() {
                sum[i] += mountain_height(Vec2::new(x, z), 5) / 200.0;
            }
        }
        assert!(sum[0] < sum[1] && sum[1] < sum[2], "горы не поднимаются к югу: {sum:?}");
        assert!(sum[2] > 120.0 && sum[2] < 600.0, "предгорья у края карты {:.0} м", sum[2]);
        // Земля у южного края карты в среднем там же, где её продолжает панорама.
        let mut diff = 0.0;
        for k in 0..100 {
            let x = -800.0 + k as f32 * 16.0;
            diff += (super::super::terrain::height_at(x, MESH_SOUTH - 10.0) - panorama_ground(x, MESH_SOUTH - 10.0)) / 100.0;
        }
        assert!(diff.abs() < 40.0, "земля и панорама расходятся на {diff:.0} м");
        let h = hash21(Vec2::new(3.0, 7.0));
        assert!((0.0..1.0).contains(&h));
    }
}
