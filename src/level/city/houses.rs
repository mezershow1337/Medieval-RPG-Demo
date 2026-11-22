//! Жилые усадьбы. В позднеантичном Херсоне квартал делился на две усадьбы; дом — комнаты
//! вокруг мощёного или земляного двора с цистерной для дождевой воды. Стены — бутовая кладка
//! на глине, углы и проёмы — тёсаный известняк; крыши черепичные, скаты — во двор (вода стекает
//! в цистерну). На главной улице первые этажи — лавки с широкими проёмами.

use super::kit::{self, Op, Rect, Rng, Side};
use super::*;
use crate::collision::Aabb;

pub const H1: f32 = 3.4;
const OUTER_T: f32 = 0.6;
const INNER_T: f32 = 0.45;
const FRONT_D: f32 = 5.6;
const SIDE_D: f32 = 5.2;

/// Особая лавка на месте одной из комнат по главной/торговой улице.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shop {
    None,
    Apothecary,
    Bookseller,
    Tavern,
}

/// Местная система координат усадьбы: u — вдоль улицы фасада, v — вглубь от фасада.
#[derive(Clone, Copy)]
pub struct Frame {
    pub r: Rect,
    pub front: Side,
    pub mirror: bool,
}

impl Frame {
    pub fn w(&self) -> f32 {
        self.r.w()
    }
    pub fn d(&self) -> f32 {
        self.r.d()
    }
    pub fn x(&self, u: f32) -> f32 {
        if self.mirror { self.r.x1 - u } else { self.r.x0 + u }
    }
    pub fn z(&self, v: f32) -> f32 {
        if self.front == Side::N { self.r.z0 + v } else { self.r.z1 - v }
    }
    pub fn p(&self, u: f32, y: f32, v: f32) -> Vec3 {
        Vec3::new(self.x(u), y, self.z(v))
    }
    pub fn rect(&self, u0: f32, u1: f32, v0: f32, v1: f32) -> Rect {
        let (xa, xb) = (self.x(u0), self.x(u1));
        let (za, zb) = (self.z(v0), self.z(v1));
        Rect::new(xa.min(xb), za.min(zb), xa.max(xb), za.max(zb))
    }
    /// Стена вдоль u на глубине v.
    pub fn wall_u(&self, b: &mut Builder, v: f32, u0: f32, u1: f32, t: f32, h: f32, ops: &[Op], mat: Mat) {
        let (xa, xb) = (self.x(u0), self.x(u1));
        let ops: Vec<Op> = ops.iter().map(|o| map_op(*o, |u| self.x(u))).collect();
        kit::wall_x(b, xa.min(xb), xa.max(xb), self.z(v), t, h, &ops, mat);
    }
    /// Стена вдоль v на позиции u.
    pub fn wall_v(&self, b: &mut Builder, u: f32, v0: f32, v1: f32, t: f32, h: f32, ops: &[Op], mat: Mat) {
        let (za, zb) = (self.z(v0), self.z(v1));
        let ops: Vec<Op> = ops.iter().map(|o| map_op(*o, |v| self.z(v))).collect();
        kit::wall_z(b, za.min(zb), za.max(zb), self.x(u), t, h, &ops, mat);
    }
    /// Сторона мира, куда смотрит фасад (u-стена при v = 0).
    pub fn front_side(&self) -> Side {
        self.front
    }
    /// Сторона мира боковой улицы при u = 0.
    pub fn side0(&self) -> Side {
        if self.mirror { Side::E } else { Side::W }
    }
}

pub fn map_op(o: Op, f: impl Fn(f32) -> f32) -> Op {
    match o {
        Op::Door { s, w, h } => Op::Door { s: f(s), w, h },
        Op::Window { s, w, sill, top } => Op::Window { s: f(s), w, sill, top },
        Op::Arch { s, w, spring } => Op::Arch { s: f(s), w, spring },
    }
}

/// Две усадьбы квартала.
pub fn block_houses(b: &mut Builder, i: i32, j: i32, seed: u32) {
    let r = block(i, j);
    let zm = r.cz();
    let lots = [(Rect::new(r.x0, r.z0, r.x1, zm), Side::N), (Rect::new(r.x0, zm, r.x1, r.z1), Side::S)];
    for (k, (lot, front)) in lots.into_iter().enumerate() {
        let mut rng = Rng::new(seed.wrapping_add(k as u32 * 977));
        let main_street = (j == -1 && front == Side::S) || (j == 1 && front == Side::N);
        let shop = special_shop(i, j, front);
        let f = Frame { r: lot, front, mirror: rng.chance(0.5) };
        house(b, f, main_street || shop != Shop::None, shop, &mut rng);
    }
}

/// Особые лавки у агоры и у главных ворот.
pub fn special_shop(i: i32, j: i32, front: Side) -> Shop {
    match (i, j, front) {
        // Северная сторона агоры: лавки смотрят на площадь (через продольную улицу).
        (16, -2, Side::S) => Shop::Bookseller,
        (17, -2, Side::S) => Shop::Apothecary,
        (7, 1, Side::N) => Shop::Tavern,
        (17, 1, Side::N) => Shop::Apothecary,
        _ => Shop::None,
    }
}

/// Центр комнаты особой лавки (для именованных точек): лавка — вторая комната фасада.
pub fn shop_point(i: i32, j: i32, front: Side) -> Vec3 {
    let r = block(i, j);
    let zm = r.cz();
    let lot = if front == Side::N { Rect::new(r.x0, r.z0, r.x1, zm) } else { Rect::new(r.x0, zm, r.x1, r.z1) };
    let seed = (i as u32 * 131 + (j + 4) as u32 * 7919) ^ 0x5eed;
    let k = if front == Side::N { 0 } else { 1 };
    let mut rng = Rng::new(seed.wrapping_add(k * 977));
    let f = Frame { r: lot, front, mirror: rng.chance(0.5) };
    let (u0, u1) = front_rooms(&f)[SHOP_ROOM];
    f.p((u0 + u1) / 2.0, 0.0, FRONT_D * 0.55)
}

const SHOP_ROOM: usize = 1;

/// Комнаты фасадного крыла (u0, u1 между перегородками).
fn front_rooms(f: &Frame) -> Vec<(f32, f32)> {
    let w = f.w();
    let n = 4;
    let inner0 = OUTER_T;
    let inner1 = w - OUTER_T;
    let step = (inner1 - inner0) / n as f32;
    (0..n).map(|k| (inner0 + step * k as f32 + if k > 0 { INNER_T / 2.0 } else { 0.0 }, inner0 + step * (k + 1) as f32 - if k + 1 < n { INNER_T / 2.0 } else { 0.0 })).collect()
}

fn house(b: &mut Builder, f: Frame, shops: bool, special: Shop, rng: &mut Rng) {
    let (w, d) = (f.w(), f.d());
    let mat = if rng.chance(0.35) { Mat::Plaster } else { Mat::Rubble };
    let two_storey = rng.chance(0.45);
    let h_front = if two_storey { H1 * 2.0 - 0.2 } else { H1 };
    let has_back_wing = rng.chance(0.35);
    let back_d = if has_back_wing { 4.4 } else { 0.0 };
    let rooms = front_rooms(&f);
    let vestibule = if shops { 0 } else { 1 + rng.int(2) };

    // ---- Фасадное крыло ----
    let mut front_ops = Vec::new();
    let mut inner_ops = Vec::new();
    for (k, &(u0, u1)) in rooms.iter().enumerate() {
        let uc = (u0 + u1) / 2.0;
        let is_shop = shops && k != vestibule && (special == Shop::None || k == SHOP_ROOM || rng.chance(0.6));
        if k == vestibule {
            front_ops.push(Op::Door { s: uc, w: 1.5, h: 2.6 });
            inner_ops.push(Op::Door { s: uc, w: 1.3, h: 2.4 });
            // Мощёный проход.
            let r = f.rect(u0, u1, 0.0, FRONT_D);
            b.decor([r.x0, 0.0, r.z0], [r.x1, 0.03, r.z1], Mat::Paving);
        } else if is_shop {
            front_ops.push(Op::Door { s: uc, w: 2.8, h: 2.7 });
            shop_fittings(b, &f, u0, u1, if k == SHOP_ROOM { special } else { Shop::None }, rng);
        } else {
            front_ops.push(Op::Window { s: uc, w: 0.7, sill: 1.9, top: 2.5 });
            inner_ops.push(Op::Door { s: uc, w: 1.1, h: 2.2 });
            room_contents(b, &f, f.rect(u0, u1, OUTER_T, FRONT_D - INNER_T / 2.0), rng);
        }
        if two_storey {
            front_ops.push(Op::Window { s: uc, w: 0.8, sill: H1 + 1.0, top: H1 + 2.0 });
        }
    }
    f.wall_u(b, OUTER_T / 2.0, 0.0, w, OUTER_T, h_front, &front_ops, mat);
    let iv = FRONT_D - INNER_T / 2.0;
    let mut inner_wall_ops = inner_ops.clone();
    if two_storey {
        for &(u0, u1) in &rooms {
            inner_wall_ops.push(Op::Window { s: (u0 + u1) / 2.0, w: 0.9, sill: H1 + 0.9, top: H1 + 2.0 });
        }
    }
    f.wall_u(b, iv, OUTER_T, w - OUTER_T, INNER_T, h_front, &inner_wall_ops, mat);
    for k in 1..rooms.len() {
        let u = (rooms[k - 1].1 + rooms[k].0) / 2.0;
        f.wall_v(b, u, OUTER_T, iv - INNER_T / 2.0, INNER_T, h_front, &[], mat);
    }
    let wing = f.rect(0.0, w, 0.0, FRONT_D);
    if two_storey {
        // Перекрытие между этажами (снизу видны балки).
        let m = b.mesh(Mat::Wood);
        m.cuboid(wing.inset(0.3).min3(H1), wing.inset(0.3).max3(H1 + 0.12));
        let n = (w / 1.1) as usize;
        for k in 0..n {
            let u = 0.6 + (w - 1.2) * (k as f32 + 0.5) / n as f32;
            let (a, c) = (f.p(u, H1 - 0.1, OUTER_T), f.p(u, H1 - 0.1, iv));
            m.beam(a, c, 0.08, 0.09, Vec3::Y);
        }
    }
    kit::lean_roof(b, wing, f.front_side(), h_front + 1.1, h_front + 0.05, 0.7);
    for u in [0.0, w] {
        let a = f.p(u, h_front, 0.0);
        let c = f.p(u, h_front, FRONT_D);
        kit::lean_gable(b, mat, c, a, 1.1, if f.front.along_x() { Vec3::X } else { Vec3::Z }, OUTER_T);
    }
    // Угловые камни.
    for (u, v) in [(0.0, 0.0), (w, 0.0)] {
        let p = f.p(u, 0.0, v);
        let sx = if p.x > f.r.cx() { -1.0 } else { 1.0 };
        let sz = if p.z > f.r.cz() { -1.0 } else { 1.0 };
        kit::quoins(b, p.x + sx * OUTER_T / 2.0, p.z + sz * OUTER_T / 2.0, h_front, OUTER_T);
    }
    // Навесы над лавками.
    if shops {
        for (k, &(u0, u1)) in rooms.iter().enumerate() {
            if k != vestibule && rng.chance(0.7) {
                let out = f.front_side().out();
                kit::awning(b, f.p(u0 + 0.2, 0.0, -0.02), f.p(u1 - 0.2, 0.0, -0.02), out, 1.6, 2.9);
            }
        }
    }

    // ---- Боковое крыло вдоль боковой улицы (u = 0) ----
    let side_v0 = FRONT_D;
    let side_v1 = d - back_d;
    let side_len = side_v1 - side_v0;
    let n_side = if side_len > 11.0 { 3 } else { 2 };
    let step = (side_len - OUTER_T / 2.0) / n_side as f32;
    let mut side_inner_ops = Vec::new();
    let mut side_outer_ops = Vec::new();
    for k in 0..n_side {
        let v0 = side_v0 + step * k as f32;
        let v1 = v0 + step;
        let vc = (v0 + v1) / 2.0;
        side_inner_ops.push(Op::Door { s: vc, w: 1.1, h: 2.2 });
        side_outer_ops.push(Op::Window { s: vc, w: 0.6, sill: 2.0, top: 2.6 });
        if k > 0 {
            f.wall_u(b, v0, OUTER_T, SIDE_D - INNER_T, INNER_T, H1, &[], mat);
        }
        room_contents(b, &f, f.rect(OUTER_T, SIDE_D - INNER_T, v0 + 0.25, v1 - 0.25), rng);
    }
    f.wall_v(b, OUTER_T / 2.0, side_v0, side_v1, OUTER_T, H1, &side_outer_ops, mat);
    f.wall_v(b, SIDE_D - INNER_T / 2.0, side_v0, side_v1, INNER_T, H1, &side_inner_ops, mat);
    let side_wing = f.rect(0.0, SIDE_D, side_v0, side_v1);
    kit::lean_roof(b, side_wing, f.side0(), H1 + 1.0, H1 + 0.05, 0.7);
    let end_dir = if f.front.along_x() { Vec3::Z } else { Vec3::X };
    let a = f.p(0.0, H1, side_v1);
    let c = f.p(SIDE_D, H1, side_v1);
    kit::lean_gable(b, mat, c, a, 1.0, end_dir, OUTER_T);

    // ---- Заднее крыло (кладовые) или глухая стена соседа ----
    if has_back_wing {
        let v0 = d - back_d;
        f.wall_u(b, v0 + INNER_T / 2.0, SIDE_D, w - OUTER_T, INNER_T, H1, &[Op::Door { s: (SIDE_D + w) / 2.0, w: 1.2, h: 2.2 }], mat);
        let r = f.rect(SIDE_D, w - OUTER_T, v0 + INNER_T, d - 0.3);
        for k in 0..3 {
            let t = (k as f32 + 0.5) / 3.0;
            let p = Vec3::new(r.x0 + r.w() * t, 0.0, r.cz());
            kit::pithos(b, p, 1.3, 0.25);
        }
        let back_wing = f.rect(0.0, w, v0, d);
        let back_side = if f.front == Side::N { Side::S } else { Side::N };
        kit::lean_roof(b, back_wing, back_side, H1 + 0.9, H1 + 0.05, 0.6);
    }
    // Задняя (общая с соседней усадьбой) стена — половина толщины на нашей стороне.
    f.wall_u(b, d - 0.15, 0.0, w, 0.3, H1, &[], mat);
    // Стена двора вдоль второй боковой улицы, с воротами.
    let gate_v = (FRONT_D + d - back_d) / 2.0;
    let gate = rng.chance(0.6);
    let ops = if gate { vec![Op::Door { s: gate_v, w: 2.2, h: 2.6 }] } else { vec![] };
    let yard_wall_h = 3.0;
    f.wall_v(b, w - OUTER_T / 2.0, FRONT_D, d - back_d, OUTER_T, yard_wall_h, &ops, mat);
    if has_back_wing {
        f.wall_v(b, w - OUTER_T / 2.0, d - back_d, d, OUTER_T, H1, &[], mat);
    }
    // Черепичный «гребень» по верху стены двора.
    let top = f.rect(w - OUTER_T, w, FRONT_D, d - back_d);
    b.mesh(Mat::Tile).gable_roof_slopes(Vec2::new(top.x0 - 0.08, top.z0), Vec2::new(top.x1 + 0.08, top.z1), yard_wall_h, yard_wall_h + 0.22, !f.front.along_x());

    // ---- Двор ----
    courtyard(b, &f, (SIDE_D, w - OUTER_T), (FRONT_D, d - back_d - 0.3), has_back_wing, rng);
}

/// Утварь комнаты: кухня, спальня, кладовая, мастерская, трапезная.
fn room_contents(b: &mut Builder, _f: &Frame, r: Rect, rng: &mut Rng) {
    let r = r.inset(0.15);
    if r.w() < 1.6 || r.d() < 1.6 {
        return;
    }
    match rng.int(6) {
        0 => {
            // Кухня: очаг, полка с горшками, амфоры у стены.
            kit::hearth(b, Vec3::new(r.cx(), 0.0, r.cz()));
            let sh = if r.w() > r.d() { Rect::new(r.x0 + 0.3, r.z0, r.x1 - 0.3, r.z0 + 0.35) } else { Rect::new(r.x0, r.z0 + 0.3, r.x0 + 0.35, r.z1 - 0.3) };
            kit::shelf(b, sh, 2, rng, Mat::Ceramic);
            kit::amphora(b, Vec3::new(r.x1 - 0.3, 0.0, r.z1 - 0.3), 1.0, rng.int(3) as u32);
        }
        1 => {
            // Спальня: ложе и сундук.
            let bed = if r.w() > r.d() { Rect::new(r.x0 + 0.1, r.z0 + 0.1, r.x0 + 2.0, r.z0 + 1.0) } else { Rect::new(r.x0 + 0.1, r.z0 + 0.1, r.x0 + 1.0, r.z0 + 2.0) };
            kit::bed(b, bed);
            kit::chest(b, Vec3::new(r.x1 - 0.45, 0.0, r.z1 - 0.4), r.w() > r.d());
        }
        2 => {
            // Кладовая: пифосы и амфоры.
            kit::pithos(b, Vec3::new(r.x0 + 0.6, 0.0, r.z0 + 0.6), 1.2, 0.2);
            kit::pithos(b, Vec3::new(r.x1 - 0.6, 0.0, r.z1 - 0.6), 1.1, 0.2);
            for k in 0..3 {
                kit::amphora(b, Vec3::new(r.x0 + 0.4 + k as f32 * 0.45, 0.0, r.z1 - 0.35), 1.0, k as u32);
            }
        }
        3 => {
            // Ткацкая: станок и корзины.
            kit::loom(b, Vec3::new(r.cx(), 0.0, r.cz()), r.w() > r.d());
        }
        4 => {
            // Трапезная: стол и скамьи.
            let c = r.center(0.0);
            let (hw, hd) = if r.w() > r.d() { (0.9, 0.4) } else { (0.4, 0.9) };
            kit::table(b, Rect::new(c.x - hw, c.z - hd, c.x + hw, c.z + hd), 0.75);
            for k in 0..3 {
                kit::jug(b, Vec3::new(c.x - hw * 0.5 + hw * 0.5 * k as f32, 0.75, c.z), 0.2, Mat::Ceramic);
            }
        }
        _ => {}
    }
}

fn shop_fittings(b: &mut Builder, f: &Frame, u0: f32, u1: f32, kind: Shop, rng: &mut Rng) {
    let r = f.rect(u0, u1, OUTER_T, FRONT_D - INNER_T / 2.0).inset(0.1);
    // Прилавок у входа (каменный, с деревянной доской), проход сбоку.
    let counter = f.rect(u0 + 0.2, u1 - 1.2, OUTER_T + 0.9, OUTER_T + 1.4);
    b.solid([counter.x0, 0.0, counter.z0], [counter.x1, 0.95, counter.z1], Mat::Rubble);
    b.mesh(Mat::Wood).cuboid(counter.min3(0.95) - Vec3::new(0.05, 0.0, 0.05), counter.max3(1.0) + Vec3::new(0.05, 0.0, 0.05));
    // Полки у задней стены.
    let back = f.rect(u0 + 0.1, u1 - 0.1, FRONT_D - INNER_T / 2.0 - 0.45, FRONT_D - INNER_T / 2.0 - 0.1);
    let p = counter.center(1.0);
    match kind {
        Shop::Apothecary => {
            // Аптекарь: склянки, ступка, весы, пучки сушёных трав под потолком.
            kit::shelf(b, back, 3, rng, Mat::Ceramic);
            kit::jug(b, p + Vec3::new(-0.3, 0.0, 0.0), 0.16, Mat::Bronze);
            b.mesh(Mat::Limestone).lathe(p + Vec3::new(0.3, 0.0, 0.0), &[(0.0, 0.0), (0.16, 0.0), (0.18, 0.15), (0.12, 0.15), (0.08, 0.06), (0.0, 0.06)], 10);
            for k in 0..6 {
                let t = (k as f32 + 0.5) / 6.0;
                let q = Vec3::new(r.x0 + r.w() * t, H1 - 0.6, r.cz());
                b.mesh(Mat::Wood).beam(q + Vec3::Y * 0.55, q + Vec3::Y * 0.2, 0.01, 0.01, Vec3::X);
                b.mesh(if k % 2 == 0 { Mat::HerbSilver } else { Mat::HerbGreen }).lathe(q, &[(0.0, -0.1), (0.1, 0.0), (0.06, 0.2), (0.0, 0.25)], 6);
            }
            b.lamp(r.center(2.6), Color::srgb(1.0, 0.75, 0.45), 6_000.0, 6.0);
        }
        Shop::Bookseller => {
            // Книжная лавка: футляры-капсы со свитками, кодексы на столе.
            kit::shelf(b, back, 3, rng, Mat::Canvas);
            let m = b.mesh(Mat::Wood);
            for k in 0..3 {
                let q = p + Vec3::new(-0.5 + k as f32 * 0.5, 0.0, 0.0);
                m.cuboid(q - Vec3::new(0.14, 0.0, 0.1), q + Vec3::new(0.14, 0.06, 0.1));
            }
            for k in 0..3 {
                let q = r.min3(0.0) + Vec3::new(0.4 + k as f32 * 0.5, 0.0, 0.4);
                b.mesh(Mat::Wood).lathe(q, &[(0.18, 0.0), (0.18, 0.5), (0.0, 0.5)], 8);
            }
            b.lamp(r.center(2.6), Color::srgb(1.0, 0.75, 0.45), 6_000.0, 6.0);
        }
        Shop::Tavern => {
            // Корчма: вмазанные в прилавок долии, амфоры с вином, столы.
            for k in 0..2 {
                b.mesh(Mat::Coals).disc(p + Vec3::new(-0.4 + k as f32 * 0.8, 0.02, 0.0), 0.2, 10, true);
            }
            for k in 0..4 {
                kit::amphora(b, back.min3(0.0) + Vec3::new(0.3 + k as f32 * 0.45, 0.0, 0.15), 1.0, k as u32);
            }
            b.lamp(r.center(2.6), Color::srgb(1.0, 0.7, 0.4), 8_000.0, 7.0);
        }
        Shop::None => {
            // Обычная лавка: товар на прилавке и у стены.
            match rng.int(3) {
                0 => {
                    for k in 0..4 {
                        kit::amphora(b, back.min3(0.0) + Vec3::new(0.3 + k as f32 * 0.45, 0.0, 0.15), 1.0, rng.int(3) as u32);
                    }
                }
                1 => kit::shelf(b, back, 2, rng, Mat::Ceramic),
                _ => {
                    for k in 0..3 {
                        kit::jug(b, p + Vec3::new(-0.4 + k as f32 * 0.4, 0.0, 0.0), 0.25, Mat::Ceramic);
                    }
                }
            }
        }
    }
}

/// Двор в координатах усадьбы: u — от бокового крыла до стены двора, v — от фасадного крыла назад.
fn courtyard(b: &mut Builder, f: &Frame, (u0, u1): (f32, f32), (v0, v1): (f32, f32), back_door: bool, rng: &mut Rng) {
    if u1 - u0 < 4.0 || v1 - v0 < 4.0 {
        return;
    }
    // Цистерна под дождевую воду — посередине.
    kit::cistern_mouth(b, f.p((u0 + u1) / 2.0 + rng.range(-1.0, 1.0), 0.0, (v0 + v1) / 2.0 + rng.range(-1.0, 1.0)));
    // Пифосы, врытые у задней стены (не перед дверью кладовой).
    let n = 1 + rng.int(3);
    let door_u = (SIDE_D + f.w()) / 2.0;
    let mut u = u0 + 0.9;
    let mut placed = 0;
    while placed < n && u < u1 - 3.5 {
        if !back_door || (u - door_u).abs() > 1.6 {
            kit::pithos(b, f.p(u, 0.0, v1 - 0.8), 1.3, 0.6);
            placed += 1;
        }
        u += 1.2;
    }
    // Дальний задний угол: печь, давильня, мельница или дрова.
    let corner = f.p(u1 - 1.4, 0.0, v1 - 1.4);
    match rng.int(5) {
        0 => kit::bread_oven(b, corner, 0.8),
        1 => kit::wine_press(b, f.rect(u1 - 2.4, u1 - 0.6, v1 - 2.2, v1 - 0.4).inset(0.0)),
        2 => kit::hand_mill(b, corner),
        3 => kit::woodpile(b, f.rect(u1 - 0.9, u1 - 0.2, v1 - 3.2, v1 - 0.8), 0.8),
        _ => {}
    }
    if rng.chance(0.45) {
        kit::fig_tree(b, f.p(u0 + 1.8, 0.0, (v0 + v1) / 2.0 + 1.0), rng.range(3.5, 5.0), rng);
    } else if rng.chance(0.5) {
        // Виноградная беседка поперёк двора.
        let v = (v0 + v1) / 2.0 + 1.6;
        kit::vine_row(b, f.p(u0 + 1.2, 0.0, v), f.p((u0 + 7.0).min(u1 - 1.0), 0.0, v), 2.3);
    }
    if rng.chance(0.5) {
        for k in 0..3 {
            let p = f.p(u1 - 0.6, 0.17, v0 + 1.0 + k as f32 * 0.38);
            kit::amphora_lying(b, p, f.front.along_x(), 1.0);
        }
    }
}

/// Пустырь у стены: огороды за низкой оградой, плодовые деревья.
pub fn garden(b: &mut Builder, r: Rect, seed: u32) {
    let mut rng = Rng::new(seed);
    // Ограда лишь там, где квартал целиком в городе; иначе — несколько деревьев.
    let inside = |x: f32, z: f32| terrain::inside_city(x, z) && edge_distance(x, z) > 5.0;
    let mut k = 0;
    let mut z = r.z0 + 3.0;
    while z < r.z1 - 2.0 {
        let mut x = r.x0 + 2.0;
        while x < r.x1 - 2.0 {
            if inside(x, z) {
                if k % 3 == 0 {
                    kit::fig_tree(b, Vec3::new(x + rng.range(-1.0, 1.0), 0.0, z), rng.range(3.0, 5.0), &mut rng);
                } else {
                    // Грядки.
                    let bed = Rect::new(x - 2.5, z - 0.5, x + 2.5, z + 0.5);
                    b.mesh(Mat::Earth).cuboid(bed.min3(0.0), bed.max3(0.18));
                    let n = 6;
                    for q in 0..n {
                        let p = Vec3::new(bed.x0 + 0.4 + q as f32 * 0.8, 0.18, bed.cz());
                        b.mesh(Mat::HerbGreen).lathe(p, &[(0.0, 0.0), (0.22, 0.08), (0.15, 0.25), (0.0, 0.3)], 6);
                    }
                }
            }
            k += 1;
            x += 7.0;
        }
        z += 6.0;
    }
    let _ = Aabb::new(Vec3::ZERO, Vec3::ZERO);
}
