//! Рыбацкая лачуга на берегу Песочной бухты, за западной стеной Херсона.
//!
//! Бутовая мазанка под камышом, перед ней — холщовый навес, под которым рыбак Евпл торгует
//! снастями и покупает улов (trade.rs, место `fish_hut`). От лачуги в бухту уходят мостки
//! на сваях: у берега мелко (хамса, барабулька), к концу мостков — глубоко (ёрш, луфарь,
//! калкан). Вокруг — вешала с сетями, рама с вялящейся рыбой, костёр с котлом ухи, лодки:
//! одна у мостков, другая вытащена на берег.

use super::*;

/// Лачуга (по наружным граням стен). Дверь — на запад, к воде.
pub const HUT: Rect = Rect::new(-339.0, 101.5, -331.0, 108.5);
/// Ось мостков по Z (напротив двери).
pub const JETTY_Z: f32 = 106.6;
/// Ширина мостков.
pub const JETTY_W: f32 = 2.4;
/// Где кончаются мостки (x).
pub const JETTY_END: f32 = -390.0;
/// Высота настила мостков.
pub const JETTY_DECK: f32 = -0.2;

/// Где мостки начинаются на берегу (x): первая точка от лачуги к воде, где земля ниже настила.
pub fn jetty_start() -> f32 {
    let mut x = HUT.x0 - 6.0;
    while x > JETTY_END + 10.0 && ground_y(x, JETTY_Z) > JETTY_DECK + 0.05 {
        x -= 0.5;
    }
    x + 1.0
}

/// Где стоит рыбак-торговец: под навесом у северного угла лачуги, на земле.
pub fn trader_point() -> Vec3 {
    g(HUT.x0 - 1.8, HUT.z0 + 1.7)
}

pub fn fishing_hut(b: &mut Builder) {
    let mut rng = Rng::new(4021);
    hut(b);
    jetty(b);
    // Лодка у мостков, вторая вытащена на берег к северу.
    skiff(b, Vec3::new(JETTY_END + 6.0, WATER_Y + 0.05, JETTY_Z + JETTY_W / 2.0 + 1.25), 0.06, 0.0);
    let beach = Vec2::new(-347.5, 97.0);
    let by = ground_y(beach.x, beach.y).max(WATER_Y + 0.25);
    skiff(b, Vec3::new(beach.x, by + 0.42, beach.y), -0.65, 0.12);
    oars_on_ground(b, g(-343.0, 98.8), Vec3::new(0.8, 0.0, -0.6).normalize());
    // Вешала с сетями к югу, рама с рыбой к северу.
    net_rack(b, g(-339.5, 113.0), g(-333.5, 113.0), &mut rng);
    net_rack(b, g(-346.0, 112.0), g(-342.5, 115.8), &mut rng);
    fish_frame(b, Vec2::new(-335.0, 98.6), &mut rng);
    // Костёр с котлом ухи и чурбаки вокруг.
    let fire = g(-343.6, 110.3);
    b.at(fire.x, fire.z);
    b.mesh(Mat::Rubble).lathe(fire - Vec3::Y * 0.05, &[(0.55, 0.0), (0.62, 0.14), (0.55, 0.24), (0.45, 0.22), (0.45, 0.08)], 10);
    b.mesh(Mat::Coals).disc(fire + Vec3::Y * 0.09, 0.45, 10, true);
    b.hearths.push(fire);
    super::interior::tripod_cauldron(b, fire);
    for (dx, dz) in [(1.2f32, 0.4f32), (-0.6, 1.2)] {
        let p = g(fire.x + dx, fire.z + dz);
        b.mesh(Mat::Wood).lathe(p - Vec3::Y * 0.05, &[(0.2, 0.0), (0.21, 0.42), (0.0, 0.43)], 9);
        kit::hidden(b, p - Vec3::new(0.2, 0.0, 0.2), p + Vec3::new(0.2, 0.42, 0.2));
    }
    // Улов в корзинах и амфоры у навеса.
    let t = trader_point();
    for (k, (dx, dz)) in [(0.5f32, 2.2f32), (-0.2, 2.9), (1.0, -1.2)].into_iter().enumerate() {
        let p = g(t.x + dx, t.z + dz);
        let fill = if k == 2 { Mat::Leather } else { Mat::Iron };
        super::interior::basket(b, p, 0.3, 0.42, fill, None);
    }
    kit::amphora(b, g(HUT.x0 - 0.5, HUT.z0 + 0.35), 0.9, 1);
    kit::amphora_lying(b, g(HUT.x0 - 0.6, HUT.z1 + 0.9), true, 0.85);
    // Якорь-камень с проушиной и бухта каната у начала мостков.
    let js = jetty_start();
    let a = g(js + 2.4, JETTY_Z - 2.2);
    b.mesh(Mat::Limestone).lathe(a - Vec3::Y * 0.05, &[(0.0, 0.0), (0.32, 0.02), (0.36, 0.2), (0.28, 0.38), (0.0, 0.4)], 9);
    b.mesh(Mat::Canvas).lathe(a + Vec3::new(0.9, 0.0, 0.3), &[(0.12, 0.0), (0.35, 0.0), (0.38, 0.06), (0.35, 0.12), (0.12, 0.12)], 14);
    kit::hidden(b, a - Vec3::new(0.4, 0.05, 0.4), a + Vec3::new(1.3, 0.4, 0.7));
    // Тропа от дороги у главных ворот вдоль стены к лачуге.
    path(b, &[Vec2::new(main_gate_x() - 3.0, 199.0), Vec2::new(-170.0, 188.0), Vec2::new(-226.0, 166.0), Vec2::new(-262.0, 140.0), Vec2::new(-296.0, 117.0), Vec2::new(-322.0, 108.0), Vec2::new(HUT.x1 + 0.5, HUT.cz())]);
    path(b, &[Vec2::new(HUT.x0 - 0.5, JETTY_Z), Vec2::new(js, JETTY_Z)]);
}

/// Мазанка: бутовые стены, камышовая кровля, внутри — очаг, лежанка, ларь, сети на колышках.
fn hut(b: &mut Builder) {
    let r = HUT;
    let y = terrace(b, r.inset(-0.8), Mat::Rubble);
    let (w, d) = (r.w(), r.d());
    let door_z = JETTY_Z - r.cz();
    b.at(r.cx(), r.cz());
    b.placed(Vec3::new(r.cx(), y, r.cz()), move |t| {
        let mut rng = Rng::new(77);
        let l = Rect::new(-w / 2.0, -d / 2.0, w / 2.0, d / 2.0);
        let (h, wt) = (2.35, 0.6);
        let door = [Op::Door { s: door_z, w: 1.15, h: 1.95 }];
        let win_n = [Op::Window { s: 1.2, w: 0.6, sill: 1.1, top: 1.6 }];
        let win_e = [Op::Window { s: -0.8, w: 0.55, sill: 1.15, top: 1.6 }];
        kit::wall_x(t, l.x0, l.x1, l.z0 + wt / 2.0, wt, h, &win_n, Mat::Rubble);
        kit::wall_x(t, l.x0, l.x1, l.z1 - wt / 2.0, wt, h, &[], Mat::Rubble);
        kit::wall_z(t, l.z0 + wt, l.z1 - wt, l.x0 + wt / 2.0, wt, h, &door, Mat::Rubble);
        kit::wall_z(t, l.z0 + wt, l.z1 - wt, l.x1 - wt / 2.0, wt, h, &win_e, Mat::Rubble);
        thatch_roof(t, l, h, 2.0, true, 0.7, Mat::Wood);
        t.decor([l.x0 + wt, 0.0, l.z0 + wt], [l.x1 - wt, 0.03, l.z1 - wt], Mat::Earth);
        // Сети и вёсла на колышках у стены, связка сушёной рыбы под стрехой.
        super::interior::pegs(t, Vec3::new(l.x1 - wt - 0.02, 1.75, 0.6), Vec3::NEG_X, Mat::Canvas);
        kit::hearth(t, Vec3::new(0.6, 0.0, -0.2));
        kit::bed(t, Rect::new(l.x1 - wt - 2.0, l.z1 - wt - 0.95, l.x1 - wt - 0.05, l.z1 - wt - 0.05));
        super::interior::chest(t, Rect::new(l.x1 - wt - 1.1, l.z0 + wt + 0.05, l.x1 - wt - 0.05, l.z0 + wt + 0.65), "Рыбацкий ларь");
        kit::pithos(t, Vec3::new(l.x0 + wt + 0.55, 0.0, l.z0 + wt + 0.55), 1.0, 0.15);
        super::interior::basket(t, Vec3::new(l.x0 + wt + 0.5, 0.0, l.z0 + wt + 1.5), 0.26, 0.4, Mat::Leather, Some("Корзина с уловом"));
        let mut fish_line = |a: Vec3, c: Vec3, n: usize| {
            t.mesh(Mat::Wood).beam(a, c, 0.02, 0.02, Vec3::Y);
            for k in 0..n {
                let q = a + (c - a) * ((k as f32 + 0.5) / n as f32);
                hanging_fish(t, q, rng.range(0.22, 0.32), rng.f());
            }
        };
        fish_line(Vec3::new(l.x0 - 0.4, h - 0.15, l.z0 + 0.4), Vec3::new(l.x0 - 0.4, h - 0.15, door_z - 0.9), 5);
        // Оконце и дверь — светлые, ночью у двери горит факел.
        t.torch(l.x0 - 0.15, 2.0, door_z + 0.95, Vec3::NEG_X);
    });
    // Навес перед лачугой: под ним торгует рыбак.
    let a = Vec3::new(r.x0, y, r.z0 + 0.1);
    let c = Vec3::new(r.x0, y, JETTY_Z - 0.9);
    kit::awning(b, a, c, Vec3::NEG_X, 3.4, 2.3);
}

/// Мостки на сваях: дощатый настил, сваи до дна, тумбы для швартовки, факел на конце.
fn jetty(b: &mut Builder) {
    let x0 = JETTY_END;
    let x1 = jetty_start();
    let (z0, z1) = (JETTY_Z - JETTY_W / 2.0, JETTY_Z + JETTY_W / 2.0);
    let y = JETTY_DECK;
    // Коллизия настила — одной коробкой; видимые доски — поперёк, с зазорами.
    b.at((x0 + x1) / 2.0, JETTY_Z);
    b.invisible.push(Aabb::new(Vec3::new(x0, y - 0.25, z0), Vec3::new(x1, y, z1)));
    let mut x = x0;
    let mut k = 0;
    while x < x1 {
        let xx = (x + 0.28).min(x1);
        b.at(x, JETTY_Z);
        let sag = 0.012 * ((k * 7) % 5) as f32;
        let jut = 0.04 * ((k * 3) % 4) as f32;
        b.mesh(Mat::Wood).cuboid(Vec3::new(x + 0.015, y - 0.06 - sag, z0 - jut), Vec3::new(xx - 0.015, y - sag, z1 + jut * 0.5));
        x = xx;
        k += 1;
    }
    // Продольные балки под досками и сваи.
    let m = b.mesh(Mat::Wood);
    for z in [z0 + 0.2, JETTY_Z, z1 - 0.2] {
        m.beam(Vec3::new(x0, y - 0.17, z), Vec3::new(x1, y - 0.17, z), 0.08, 0.1, Vec3::Y);
    }
    let mut x = x0 + 0.3;
    while x < x1 - 0.5 {
        let bottom = ground_y(x, JETTY_Z) - 0.6;
        for zz in [z0 + 0.15, z1 - 0.15] {
            b.at(x, zz);
            let lean = ((x * 1.7).sin() * 0.03, (x * 2.3).cos() * 0.03);
            b.mesh(Mat::Wood).beam(Vec3::new(x, bottom, zz), Vec3::new(x + lean.0, y + 0.02, zz + lean.1), 0.12, 0.12, Vec3::X);
        }
        x += 2.6;
    }
    // Тумбы на конце и посередине, ограждение вдоль краёв невидимое (чтобы не падали сослепу).
    for px in [x0 + 0.35, (x0 + x1) / 2.0] {
        for pz in [z0 + 0.25, z1 - 0.25] {
            let p = Vec3::new(px, y, pz);
            b.mesh(Mat::Wood).lathe(p, &[(0.14, 0.0), (0.14, 0.55), (0.1, 0.6), (0.0, 0.62)], 8);
            kit::hidden(b, p - Vec3::new(0.15, 0.0, 0.15), p + Vec3::new(0.15, 0.62, 0.15));
        }
    }
    b.invisible.push(Aabb::new(Vec3::new(x0, y, z0 - 0.3), Vec3::new(x1 - 3.0, y + 1.0, z0)));
    b.invisible.push(Aabb::new(Vec3::new(x0, y, z1), Vec3::new(x1 - 3.0, y + 1.0, z1 + 0.3)));
    b.invisible.push(Aabb::new(Vec3::new(x0 - 0.3, y, z0), Vec3::new(x0, y + 1.0, z1)));
    // Лесенка к лодке.
    let lx = x0 + 6.0;
    let m = b.mesh(Mat::Wood);
    for dx in [-0.3f32, 0.3] {
        m.beam(Vec3::new(lx + dx, y, z1 + 0.05), Vec3::new(lx + dx, WATER_Y - 0.4, z1 + 0.25), 0.03, 0.03, Vec3::X);
    }
    for k in 0..3 {
        let t = (k as f32 + 0.5) / 3.0;
        let p = Vec3::new(lx, y + (WATER_Y - 0.4 - y) * t, z1 + 0.05 + 0.2 * t);
        m.beam(p - Vec3::X * 0.3, p + Vec3::X * 0.3, 0.025, 0.025, Vec3::Y);
    }
    b.torch(x0 + 0.35, y + 1.55, z0 + 0.25, Vec3::NEG_X);
    // Старик-рыбак удит с конца мостков (житель; его удочка — просто жердь с лесой).
    let p = Vec3::new(x0 + 2.4, y, z0 + 0.55);
    b.resident(p, std::f32::consts::PI, Role::Sailor);
    let tip = p + Vec3::new(-3.1, 1.9, -0.4);
    b.mesh(Mat::Wood).beam(p + Vec3::new(-0.3, 0.9, -0.1), tip, 0.018, 0.018, Vec3::Y);
    b.mesh(Mat::Canvas).beam(tip, Vec3::new(tip.x - 0.6, WATER_Y + 0.05, tip.z - 0.1), 0.003, 0.003, Vec3::X);
    b.mesh(Mat::PetalRed).lathe(Vec3::new(tip.x - 0.6, WATER_Y - 0.02, tip.z - 0.1), &[(0.0, 0.0), (0.03, 0.04), (0.02, 0.09), (0.0, 0.1)], 6);
}

/// Вяленая рыба, подвешенная за хвост (`q` — точка подвеса).
fn hanging_fish(b: &mut Builder, q: Vec3, len: f32, turn: f32) {
    b.mesh(Mat::Canvas).beam(q, q - Vec3::Y * 0.08, 0.004, 0.004, Vec3::X);
    let top = q - Vec3::Y * 0.08;
    let p = |r: f32, h: f32| (r * len, h * len);
    // Профиль снизу вверх: голова, тело, хвостовой стебель, хвост.
    let prof = [p(0.0, 0.0), p(0.07, 0.04), p(0.12, 0.2), p(0.11, 0.5), p(0.05, 0.78), p(0.025, 0.86), p(0.09, 0.97), p(0.0, 1.0)];
    // Сечение — эллипс: рыба плоская с боков; `turn` — куда повёрнута боком.
    let a = turn * std::f32::consts::TAU;
    let (wx, wz) = (Vec3::new(a.cos(), 0.0, a.sin()), Vec3::new(-a.sin(), 0.0, a.cos()));
    let n = 7;
    let ring = |r: f32, h: f32, k: usize| {
        let t = k as f32 / n as f32 * std::f32::consts::TAU;
        top - Vec3::Y * (len - h) + wx * (t.cos() * r) + wz * (t.sin() * r * 0.42)
    };
    let m = b.mesh(Mat::Leather);
    for w in prof.windows(2) {
        let ((r0, h0), (r1, h1)) = (w[0], w[1]);
        for k in 0..n {
            let q = [ring(r0, h0, k), ring(r0, h0, k + 1), ring(r1, h1, k + 1), ring(r1, h1, k)];
            let mid = (q[0] + q[2]) / 2.0;
            let axis = Vec3::new(top.x, mid.y, top.z);
            m.quad(q, (mid - axis).normalize_or_zero());
        }
    }
}

/// Вешала для сетей: два столба с перекладиной; через неё перекинута дублёная (бурая) сеть,
/// свисает на обе стороны складками, низ с грузилами не достаёт до земли.
fn net_rack(b: &mut Builder, a: Vec3, c: Vec3, rng: &mut Rng) {
    let h = 1.95;
    b.at((a.x + c.x) / 2.0, (a.z + c.z) / 2.0);
    let m = b.mesh(Mat::Wood);
    for p in [a, c] {
        m.beam(p - Vec3::Y * 0.3, p + Vec3::Y * (h + 0.12), 0.06, 0.06, Vec3::X);
    }
    let (ta, tc) = (a + Vec3::Y * h, c + Vec3::Y * h);
    m.beam(ta - (tc - ta) * 0.04, tc + (tc - ta) * 0.04, 0.04, 0.04, Vec3::Y);
    for p in [a, c] {
        kit::hidden(b, p - Vec3::new(0.08, 0.3, 0.08), p + Vec3::new(0.08, h, 0.08));
    }
    let along = tc - ta;
    let side = Vec3::new(-along.z, 0.0, along.x).normalize_or_zero();
    let cell = 0.09;
    let nu = (along.length() * 0.9 / cell).round().max(4.0) as usize;
    for sd in [-1.0f32, 1.0] {
        let (ph, ph2) = (rng.range(0.0, 6.0), rng.range(0.0, 6.0));
        let (d0, f0) = (rng.range(1.05, 1.35), rng.range(5.0, 8.0));
        // Подол неровный, сеть ложится складками (по u — вдоль жерди, по v — вниз).
        let point = |u: f32, v: f32| {
            let drop = d0 * (0.85 + 0.15 * (u * 9.0 + ph).sin() + 0.08 * (u * 23.0 + ph2).sin());
            let fold = 0.07 * (u * along.length() * f0 + ph).sin() * v.sqrt();
            ta + along * (0.05 + 0.9 * u) + side * sd * (0.045 + v * 0.1 + fold) - Vec3::Y * (v * drop)
        };
        let nv = (d0 / cell).round().max(4.0) as usize;
        let normal = side * sd;
        let mut lines: Vec<Vec<Vec3>> = Vec::new();
        for i in 0..=nu {
            let u = i as f32 / nu as f32;
            lines.push((0..=nv).map(|j| point(u, j as f32 / nv as f32)).collect());
        }
        for j in 1..=nv {
            let v = j as f32 / nv as f32;
            lines.push((0..=nu).map(|i| point(i as f32 / nu as f32, v)).collect());
        }
        let m = b.mesh(Mat::Leather);
        for (k, pts) in lines.iter().enumerate() {
            let w = if k == lines.len() - 1 { 0.012 } else { 0.0045 };
            net_strip(m, pts, w, normal);
        }
        // Грузила по нижней подборе.
        for i in (0..=nu).step_by(4) {
            let q = point(i as f32 / nu as f32, 1.0);
            b.mesh(Mat::Iron).lathe(q - Vec3::Y * 0.03, &[(0.0, 0.0), (0.02, 0.005), (0.02, 0.04), (0.0, 0.045)], 5);
        }
    }
}

/// Бечева сети — плоская лента в обе стороны (дешевле бруска: сетей много).
fn net_strip(m: &mut crate::level::geom::MeshBuilder, pts: &[Vec3], w: f32, normal: Vec3) {
    for seg in pts.windows(2) {
        let dir = (seg[1] - seg[0]).normalize_or_zero();
        let t = dir.cross(normal).normalize_or_zero() * w;
        let q = [seg[0] - t, seg[1] - t, seg[1] + t, seg[0] + t];
        m.quad(q, normal);
        m.quad([q[1], q[0], q[3], q[2]], -normal);
    }
}

/// Рама для вяления рыбы: козлы с жердями, на жердях — ряды рыб.
fn fish_frame(b: &mut Builder, c: Vec2, rng: &mut Rng) {
    let y = ground_y(c.x, c.y);
    let base = Vec3::new(c.x, y, c.y);
    b.at(c.x, c.y);
    let len = 3.2;
    let m = b.mesh(Mat::Wood);
    for sx in [-len / 2.0, len / 2.0] {
        for sz in [-0.55f32, 0.55] {
            m.beam(base + Vec3::new(sx, -0.2, sz), base + Vec3::new(sx, 1.85, 0.0), 0.04, 0.04, Vec3::X);
        }
    }
    for (k, hz) in [(0, -0.28f32), (1, 0.28)] {
        let hy = 1.55 - k as f32 * 0.02;
        let a = base + Vec3::new(-len / 2.0 - 0.2, hy, hz);
        let cc = base + Vec3::new(len / 2.0 + 0.2, hy, hz);
        b.mesh(Mat::Wood).beam(a, cc, 0.025, 0.025, Vec3::Y);
        let n = 9;
        for i in 0..n {
            let q = a + (cc - a) * ((i as f32 + 0.6) / (n as f32 + 0.2));
            hanging_fish(b, q, rng.range(0.3, 0.42), rng.f());
        }
    }
    b.mesh(Mat::Wood).beam(base + Vec3::new(-len / 2.0 - 0.2, 1.85, 0.0), base + Vec3::new(len / 2.0 + 0.2, 1.85, 0.0), 0.035, 0.035, Vec3::Y);
    kit::hidden(b, base + Vec3::new(-len / 2.0 - 0.1, 0.0, -0.6), base + Vec3::new(len / 2.0 + 0.1, 1.9, 0.6));
}

/// Рыбацкая лодка-долблёнка с набоем: `keel` — середина днища снаружи, `yaw` — нос по +X,
/// повёрнутый на угол, `roll` — крен (у вытащенной на берег).
fn skiff(b: &mut Builder, keel: Vec3, yaw: f32, roll: f32) {
    let (len, beam, depth) = (5.6, 1.55, 0.62);
    let (s, c) = yaw.sin_cos();
    let fwd = Vec3::new(c, 0.0, s);
    let side0 = Vec3::new(-s, 0.0, c);
    let up0 = Vec3::Y;
    let side = side0 * roll.cos() + up0 * roll.sin();
    let up = up0 * roll.cos() - side0 * roll.sin();
    b.at(keel.x, keel.z);
    // Обшивка: сечение — полукруг, к оконечностям сходится и поднимается (седловатость).
    let pt = |t: f32, a: f32| -> Vec3 {
        let x = (t - 0.5) * len;
        let e = (2.0 * t - 1.0).abs();
        let full = (1.0 - e.powf(2.2)).max(0.0).sqrt();
        let sheer = 0.18 * e * e;
        let half = beam / 2.0 * full;
        let y = depth * (1.0 - a.cos()) * (0.35 + 0.65 * full) + sheer * (1.0 - a.cos()) - (1.0 - full) * 0.0;
        keel + fwd * x + side * (half * a.sin()) + up * (y * (1.0 - 0.25 * (1.0 - full)) - 0.02 * (1.0 - full))
    };
    let (nt, na) = (16, 8);
    let m = b.mesh(Mat::Wood);
    for i in 0..nt {
        for j in 0..na {
            let (t0, t1) = (i as f32 / nt as f32, (i + 1) as f32 / nt as f32);
            let (a0, a1) = (-FRAC_PI_2 + PI * j as f32 / na as f32, -FRAC_PI_2 + PI * (j + 1) as f32 / na as f32);
            let q = [pt(t0, a0), pt(t1, a0), pt(t1, a1), pt(t0, a1)];
            let mid = (q[0] + q[2]) / 2.0;
            let axis = keel + fwd * fwd.dot(mid - keel) + up * depth;
            let n = (mid - axis).normalize_or_zero();
            m.quad(q, n);
            // Изнутри — та же обшивка, чуть внутрь.
            let qi = q.map(|p| p + (axis - p).normalize_or_zero() * 0.035);
            m.quad(qi, -n);
        }
    }
    // Планширь, банки (сиденья), киль-брус, форштевень.
    for sd in [-1.0f32, 1.0] {
        let a = if sd < 0.0 { -FRAC_PI_2 } else { FRAC_PI_2 };
        let pts: Vec<Vec3> = (0..=nt).map(|i| pt(i as f32 / nt as f32, a) + up * 0.02).collect();
        m.beam_path(&pts, 0.035, 0.03, up);
    }
    for tt in [0.33f32, 0.62] {
        let l = pt(tt, -FRAC_PI_2) - up * 0.12;
        let r = pt(tt, FRAC_PI_2) - up * 0.12;
        m.beam(l, r, 0.12, 0.025, fwd);
    }
    let kp: Vec<Vec3> = (0..=nt).map(|i| pt(i as f32 / nt as f32, 0.0) - up * 0.03).collect();
    m.beam_path(&kp, 0.05, 0.04, side);
    m.beam(pt(1.0, 0.0), pt(1.0, 0.0) + up * (depth + 0.35) + fwd * 0.15, 0.05, 0.05, side);
    // Свёрнутая сеть на корме.
    b.mesh(Mat::Canvas).lathe(keel - fwd * (len * 0.3) + up * 0.12, &[(0.0, 0.0), (0.3, 0.02), (0.32, 0.12), (0.2, 0.2), (0.0, 0.22)], 9);
    let c0 = keel - fwd * (len / 2.0) - side0 * (beam / 2.0);
    let c1 = keel + fwd * (len / 2.0) + side0 * (beam / 2.0);
    b.invisible.push(Aabb::new(Vec3::new(c0.x.min(c1.x), keel.y - 0.1, c0.z.min(c1.z)), Vec3::new(c0.x.max(c1.x), keel.y + depth + 0.1, c0.z.max(c1.z))));
}

/// Пара вёсел, брошенных на песок.
fn oars_on_ground(b: &mut Builder, p: Vec3, dir: Vec3) {
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    b.at(p.x, p.z);
    let m = b.mesh(Mat::Wood);
    for k in [-0.25f32, 0.3] {
        let a = p + side * k + Vec3::Y * 0.05;
        let c = a + dir * 2.6 + side * k * 0.2;
        m.beam(a, c, 0.025, 0.025, Vec3::Y);
        m.beam(c - dir * 0.55, c + dir * 0.1, 0.07, 0.012, Vec3::Y);
    }
}

/// Утоптанная тропа (без коллизии), лежит по рельефу; ýже дороги.
fn path(b: &mut Builder, pts: &[Vec2]) {
    let w = 1.6;
    let lift = |p: Vec2| Vec3::new(p.x, ground_y(p.x, p.y) + 0.08, p.y);
    for seg in pts.windows(2) {
        let (a, c) = (seg[0], seg[1]);
        let len = a.distance(c);
        let n = (len / 3.0).ceil().max(1.0) as usize;
        let dir = (c - a) / len;
        let side = Vec2::new(-dir.y, dir.x) * (w / 2.0);
        for k in 0..n {
            let p0 = a + (c - a) * (k as f32 / n as f32);
            let p1 = a + (c - a) * ((k + 1) as f32 / n as f32);
            // Края тропы гуляют, чтобы она не выглядела линейкой.
            let wob = |p: Vec2| 1.0 + 0.25 * ((p.x * 0.31).sin() + (p.y * 0.27).cos()) * 0.5;
            let (s0, s1) = (side * wob(p0), side * wob(p1));
            b.at(p0.x, p0.y);
            b.mesh(Mat::Earth).quad([lift(p0 - s0), lift(p1 - s1), lift(p1 + s1), lift(p0 + s0)], Vec3::Y);
        }
    }
    for p in &pts[1..pts.len() - 1] {
        b.at(p.x, p.y);
        b.mesh(Mat::Earth).disc(lift(*p) - Vec3::Y * 0.01, w / 2.0, 10, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fishing::{Water, rod};
    use crate::fishing_server::{check_cast, water_depth};

    #[test]
    fn hut_jetty_reaches_deep_water_and_shore_is_shallow() {
        let p = crate::level::place_pos("fish_hut").expect("нет места fish_hut");
        assert!(water_depth(p.x, p.z) < 0.0, "торговец стоит на суше");
        let r = rod("Можжевеловая удочка").unwrap();
        let end = Vec3::new(JETTY_END + 0.6, JETTY_DECK, JETTY_Z);
        let deep = check_cast(end, end - Vec3::X * 15.0, r).expect("с конца мостков — в воду");
        assert!(Water::of_depth(deep) == Water::Deep, "с конца мостков глубоко: {deep}");
        let start = Vec3::new(jetty_start() - 1.0, JETTY_DECK, JETTY_Z);
        let shallow = check_cast(start, start - Vec3::X * 4.0, r).expect("у берега — тоже вода");
        assert!(Water::of_depth(shallow) != Water::Deep, "у берега мелко: {shallow}");
        assert!(check_cast(p, p + Vec3::X * 10.0, r).is_err(), "в сторону суши — не вода");
        assert!(ground_y(HUT.cx(), HUT.cz()) > WATER_Y + 0.5, "лачуга на суше");
    }
}
