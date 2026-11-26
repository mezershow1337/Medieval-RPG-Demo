//! Жилые усадьбы. В позднеантичном Херсоне квартал делился на две усадьбы; дом — комнаты
//! вокруг мощёного или земляного двора с цистерной для дождевой воды. Стены — бутовая кладка
//! на глине, углы и проёмы — тёсаный известняк; крыши черепичные, скаты — во двор (вода стекает
//! в цистерну). На главной улице первые этажи — лавки с широкими проёмами.

use super::estate;
use super::interior;
use super::kit::{self, Op, Rect, Rng, Side};
use super::*;
use crate::collision::Aabb;
use crate::level::DoorSpec;

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
    /// Отрезок u0..u1 (вдоль фасада) в мировых координатах по его оси.
    pub fn span_u(&self, u0: f32, u1: f32) -> (f32, f32) {
        let r = self.rect(u0, u1, 0.0, 0.0);
        if self.front.along_x() { (r.x0, r.x1) } else { (r.z0, r.z1) }
    }
    /// Отрезок v0..v1 (вглубь от фасада) в мировых координатах по его оси.
    pub fn span_v(&self, v0: f32, v1: f32) -> (f32, f32) {
        let r = self.rect(0.0, 0.0, v0, v1);
        if self.front.along_x() { (r.z0, r.z1) } else { (r.x0, r.x1) }
    }
}

pub fn map_op(o: Op, f: impl Fn(f32) -> f32) -> Op {
    match o {
        Op::Door { s, w, h } => Op::Door { s: f(s), w, h },
        Op::Window { s, w, sill, top } => Op::Window { s: f(s), w, sill, top },
        Op::Arch { s, w, spring } => Op::Arch { s: f(s), w, spring },
        Op::Upper { s, w, sill, top } => Op::Upper { s: f(s), w, sill, top },
    }
}

/// Участок квартала: прямоугольник, сторона фасада, зеркальность (с какой боковой улицы
/// боковое крыло) и своё зерно планировки.
#[derive(Clone, Copy, Debug)]
pub struct Lot {
    pub r: Rect,
    pub front: Side,
    pub mirror: bool,
    /// Половина квартала: 0 — к северу (фасад на N), 1 — к югу.
    pub half: usize,
    pub seed: u32,
    /// Узкий дом (квартал поделён на три участка): стена двора — общая с соседом, без ворот.
    pub narrow: bool,
    /// Насколько фасадное крыло выше соседнего (м): у соседних узких домов крыши — уступом.
    pub step: f32,
}

/// Зерно квартала (то же, что у `city_layout`).
pub fn block_seed(i: i32, j: i32) -> u32 {
    (i as u32 * 131 + (j + 4) as u32 * 7919) ^ 0x5eed
}

/// Как поделён квартал. Межа между усадьбами — не ровно посередине (участки покупали, делили
/// по наследству), а примерно каждый четвёртый квартал в одной половине — два узких дома
/// вместо одного. В кварталах с особыми лавками, пустующими домами и пепелищами — две равные
/// усадьбы (на них завязаны места и номера участков).
pub fn lots(i: i32, j: i32) -> Vec<Lot> {
    let r = block(i, j);
    let seed = block_seed(i, j);
    let special = [Side::N, Side::S].iter().any(|&s| special_shop(i, j, s) != Shop::None) || (0..2).any(|k| lot_mode(i, j, k, Shop::None) != LotMode::Lived || iron_yard_lot(i, j, k));
    let mut h = Rng::new(seed ^ 0x10ce);
    let shift = h.range(-4.5, 4.5);
    let split = h.chance(0.28);
    let split_half = h.int(2);
    let split_at = h.range(0.44, 0.56);
    let zm = if special { r.cz() } else { r.cz() + shift };
    let halves = [(Rect::new(r.x0, r.z0, r.x1, zm), Side::N), (Rect::new(r.x0, zm, r.x1, r.z1), Side::S)];
    let mut out = Vec::new();
    for (k, (lot, front)) in halves.into_iter().enumerate() {
        let lot_seed = seed.wrapping_add(k as u32 * 977);
        let mirror = Rng::new(lot_seed).chance(0.5);
        if !special && split && split_half == k {
            let xm = lot.x0 + lot.w() * split_at;
            // Боковые крылья — по внешним улицам, стены дворов сходятся на меже.
            let tall = h.int(2);
            out.push(Lot { r: Rect::new(lot.x0, lot.z0, xm, lot.z1), front, mirror: false, half: k, seed: lot_seed, narrow: true, step: if tall == 0 { 0.45 } else { 0.0 } });
            out.push(Lot { r: Rect::new(xm, lot.z0, lot.x1, lot.z1), front, mirror: true, half: k, seed: lot_seed ^ 0x2b1d, narrow: true, step: if tall == 1 { 0.45 } else { 0.0 } });
        } else {
            out.push(Lot { r: lot, front, mirror, half: k, seed: lot_seed, narrow: false, step: 0.0 });
        }
    }
    out
}

/// Северный участок квартала (6, 2) — угольный и железный двор напротив кузницы.
fn iron_yard_lot(i: i32, j: i32, half: usize) -> bool {
    (i, j, half) == (6, 2, 0)
}

/// Усадьбы квартала.
pub fn block_houses(b: &mut Builder, i: i32, j: i32, _seed: u32) {
    for lot in lots(i, j) {
        let k = lot.half;
        let mut rng = Rng::new(lot.seed);
        // Первое число зерна ушло на зеркальность (как в `lots`).
        rng.f();
        let front = lot.front;
        let main_street = (j == -1 && front == Side::S) || (j == 1 && front == Side::N);
        let shop = special_shop(i, j, front);
        let f = Frame { r: lot.r, front, mirror: lot.mirror };
        let shape = Shape { narrow: lot.narrow, step: lot.step };
        if iron_yard_lot(i, j, k) {
            super::crafts::iron_yard(b, f);
            continue;
        }
        match lot_mode(i, j, k, shop) {
            LotMode::Lived => {
                let (h, r0) = (b.houses.len() as u32, b.residents.len() as u32);
                b.house = Some(h);
                // Достаток: у главной улицы и с лавкой живут богаче.
                let rich = if main_street || shop != Shop::None { 0.45 } else { 0.15 };
                let mut wr = Rng::new(lot.seed ^ (0x0a11 + k as u32 * 31));
                b.wealth = if wr.chance(rich) { 2 } else if wr.chance(0.3) { 0 } else { 1 };
                let info = house(b, f, main_street || shop != Shop::None, shop, &mut rng, LotMode::Lived, shape);
                b.house = None;
                b.wealth = 1;
                let name = house_name(&f, shop, info.two_storey);
                let r1 = b.residents.len() as u32;
                b.houses.push(crate::level::HouseSpec { name, min: f.r.min3(-0.5), max: f.r.max3(info.top), residents: r0..r1 });
            }
            LotMode::Vacant => {
                let (d0, c0, bl0) = (b.doors.len(), b.containers.len(), b.blocks.len());
                let info = house(b, f, false, Shop::None, &mut rng, LotMode::Vacant, shape);
                let (d1, c1) = (b.doors.len(), b.containers.len());
                let name = estate_name(&f, info.two_storey);
                estate::register(b, name, crate::towns::TownId::Herson, f.r, info.top, info.entrance, d0..d1, c0..c1, bl0, info.rooms, info.two_storey, None);
            }
            LotMode::Site => site(b, f, &mut rng),
        }
    }
}

/// Узкий дом или обычный; уступ крыши.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shape {
    pub narrow: bool,
    pub step: f32,
}

/// Что стоит на участке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LotMode {
    /// Живут люди (обычная усадьба).
    Lived,
    /// Пустует: продаётся или сдаётся (выморочный дом, хозяева уехали).
    Vacant,
    /// Пепелище: его отстраивает город (стройплощадка).
    Site,
}

/// Какие участки пустуют и какие сгорели (одинаково на сервере и у клиентов).
pub fn lot_mode(i: i32, j: i32, k: usize, shop: Shop) -> LotMode {
    if shop != Shop::None {
        return LotMode::Lived;
    }
    match (i, j, k) {
        // Пепелища — после пожара в квартале у главной улицы и в портовой части.
        (14, 1, 1) | (10, -2, 0) | (6, 1, 1) | (15, -3, 1) => LotMode::Site,
        // Пустующие дома: у агоры, у терм, у западной церкви, в порту.
        (17, -3, 1) | (16, 2, 0) | (11, -1, 1) | (9, 1, 0) | (4, -2, 1) | (13, 3, 0) | (7, -1, 0) => LotMode::Vacant,
        _ => LotMode::Lived,
    }
}

/// «Усадьба у терм», «Двухэтажный дом у агоры».
fn estate_name(f: &Frame, two_storey: bool) -> String {
    let c = f.r.center(0.0);
    let near = crate::level::places::places()
        .iter()
        .filter(|p| crate::level::inside_city(p.pos.x, p.pos.z))
        .min_by(|a, b| a.pos.distance_squared(c).total_cmp(&b.pos.distance_squared(c)))
        .map(|p| p.name)
        .unwrap_or("Главная улица");
    let what = if two_storey { "Двухэтажная усадьба" } else { "Усадьба" };
    // Ближнее место — как адрес: «Усадьба · Западная церковь».
    format!("{what} · {near}")
}

/// «Дом · Термы», «Лавка · Агора» — как адрес жилого дома.
fn house_name(f: &Frame, shop: Shop, two_storey: bool) -> String {
    let c = f.r.center(0.0);
    let near = crate::level::places::places()
        .iter()
        .filter(|p| crate::level::inside_city(p.pos.x, p.pos.z))
        .min_by(|a, b| a.pos.distance_squared(c).total_cmp(&b.pos.distance_squared(c)))
        .map(|p| p.name)
        .unwrap_or("Главная улица");
    let what = match (shop, two_storey) {
        (Shop::None, true) => "Двухэтажный дом",
        (Shop::None, false) => "Дом",
        _ => "Дом с лавкой",
    };
    format!("{what} · {near}")
}

/// Сгоревшая усадьба: двор с цистерной есть всегда, а дом — по стадиям стройки.
fn site(b: &mut Builder, f: Frame, rng: &mut Rng) {
    let s = b.sites;
    b.sites += 1;
    let mut t = Builder::new();
    t.chunk = b.chunk;
    let near = f.r.inset(-3.0);
    t.keep_clear = b.keep_clear.iter().filter(|p| p.x > near.x0 && p.x < near.x1 && p.z > near.z0 && p.z < near.z1).copied().collect();
    let info = house(&mut t, f, false, Shop::None, rng, LotMode::Site, Shape::default());
    // Двор (цистерна, пифосы, деревья) пережил пожар — он в общей геометрии.
    courtyard(b, &f, info.court.0, info.court.1, info.back_wing, rng);
    // Двери и сундуки готового дома — в общих списках (номера постоянные), пока дом не достроен,
    // сервер держит двери открытыми, а клиент не рисует створки.
    let (d0, c0) = (b.doors.len(), b.containers.len());
    b.doors.extend(t.doors.iter().copied());
    b.containers.extend(t.containers.iter().cloned());
    let (d1, c1) = (b.doors.len(), b.containers.len());
    let front_out = f.front_side().out();
    estate::site_stages(b, s, &t, f.r, front_out, f.front.along_x(), info.top - 1.0, rng);
    // Обстановку готового дома собираем по его же стенам и дверям.
    let bl0 = b.blocks.len();
    b.blocks.extend(t.blocks.iter().copied());
    let name = estate_name(&f, info.two_storey);
    let mut chars = name.chars();
    let name = format!("Новая {}{}", chars.next().map(|c| c.to_lowercase().to_string()).unwrap_or_default(), chars.as_str());
    estate::register(b, name, crate::towns::TownId::Herson, f.r, info.top, info.entrance, d0..d1, c0..c1, bl0, info.rooms, info.two_storey, Some(s));
    // Блоки дома живут только в группах стадий.
    b.blocks.truncate(bl0);
}

/// Что известно о построенном доме.
pub struct HouseInfo {
    pub rooms: Vec<estate::RoomPlan>,
    pub entrance: Vec3,
    pub two_storey: bool,
    /// Высота конька (для границ усадьбы).
    pub top: f32,
    pub court: ((f32, f32), (f32, f32)),
    pub back_wing: bool,
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
    // Кварталы с особыми лавками делятся ровно на две усадьбы (см. `lots`).
    let lot = lots(i, j).into_iter().find(|l| l.front == front).expect("участок лавки");
    let f = Frame { r: lot.r, front, mirror: lot.mirror };
    let (u0, u1) = front_rooms(&f)[SHOP_ROOM];
    f.p((u0 + u1) / 2.0, 0.0, FRONT_D * 0.55)
}

/// Прилавок особой лавки: середина стола и куда он обращён (к двери на улицу). Стол стоит
/// в глубине комнаты, чтобы у входа оставалось место покупателю.
pub fn shop_counter(i: i32, j: i32, front: Side) -> (Vec3, Vec3) {
    let p = shop_point(i, j, front);
    let facing = if front == Side::N { Vec3::NEG_Z } else { Vec3::Z };
    // `shop_point` — на 0.55 глубины корпуса (3.1 м от улицы): стол на 0.3 м ближе к двери,
    // продавец — за ним, у задней стены лавки; у входа остаётся 1.8 м.
    (p + facing * 0.3, facing)
}

const SHOP_ROOM: usize = 1;

/// Комнаты фасадного крыла (u0, u1 между перегородками).
fn front_rooms(f: &Frame) -> Vec<(f32, f32)> {
    let w = f.w();
    // Узкий дом — две комнаты по фасаду.
    let n = if w > 16.0 { 4 } else { 2 };
    let inner0 = OUTER_T;
    let inner1 = w - OUTER_T;
    let step = (inner1 - inner0) / n as f32;
    (0..n).map(|k| (inner0 + step * k as f32 + if k > 0 { INNER_T / 2.0 } else { 0.0 }, inner0 + step * (k + 1) as f32 - if k + 1 < n { INNER_T / 2.0 } else { 0.0 })).collect()
}

fn house(b: &mut Builder, f: Frame, shops: bool, special: Shop, rng: &mut Rng, mode: LotMode, shape: Shape) -> HouseInfo {
    let lived = mode == LotMode::Lived;
    let mut entrance = f.p(f.w() / 2.0, 0.0, -1.2);
    let (w, d) = (f.w(), f.d());
    let mat = if rng.chance(0.35) { Mat::Plaster } else { Mat::Rubble };
    // Узкий дом — одноэтажный и без кладовых сзади: лестнице и заднему крылу во дворе тесно.
    let two_storey = rng.chance(0.45) && !shape.narrow;
    // Одноэтажные дома строили в разное время и разные артели: стены разной высоты,
    // поэтому крыши квартала не идут одной линией (своё зерно — планировка не сдвигается).
    let mut hr = Rng::new((f.r.x0 * 13.0 + f.r.z0 * 71.0) as i32 as u32 ^ 0x4e1f);
    let h_front = if two_storey {
        H1 * 2.0 - 0.2
    } else if shape.narrow {
        // У соседних узких домов крыши — уступом (одна выше на `step`), иначе черепица
        // двух крыш легла бы в одну плоскость внахлёст.
        H1 + shape.step
    } else {
        H1 + [0.0, 0.0, 0.25, 0.5][hr.int(4)]
    };
    let has_back_wing = rng.chance(0.35) && !shape.narrow;
    let back_d = if has_back_wing { 4.4 } else { 0.0 };
    let rooms = front_rooms(&f);
    let vestibule = if shops { 0 } else { (1 + rng.int(2)).min(rooms.len() - 1) };
    // Что где: комнаты обставляются, когда все стены (и двери в них) уже стоят.
    let mut plan: Vec<Rect> = Vec::new();
    let mut shop_rooms: Vec<(f32, f32, Shop)> = Vec::new();
    // Лестница на второй этаж — со двора, у перегородки между средними комнатами.
    let stair_u = if rooms.len() > 2 { (rooms[1].1 + rooms[2].0) / 2.0 } else { (rooms[0].1 + rooms[1].0) / 2.0 };

    // ---- Фасадное крыло ----
    let mut front_ops = Vec::new();
    let mut inner_ops = Vec::new();
    // Окна фасада (u, ширина, подоконник, верх) — для ставней и подоконных плит.
    let mut windows: Vec<(f32, f32, f32, f32)> = Vec::new();
    for (k, &(u0, u1)) in rooms.iter().enumerate() {
        let uc = (u0 + u1) / 2.0;
        let is_shop = shops && k != vestibule && (special == Shop::None || k == SHOP_ROOM || rng.chance(0.6));
        if k == vestibule {
            entrance = f.p(uc, 0.0, -1.2);
            front_ops.push(Op::Door { s: uc, w: 1.5, h: 2.6 });
            inner_ops.push(Op::Door { s: uc, w: 1.3, h: 2.4 });
            // Мощёный проход.
            let r = f.rect(u0, u1, 0.0, FRONT_D);
            b.decor([r.x0, 0.0, r.z0], [r.x1, 0.03, r.z1], Mat::Paving);
        } else if is_shop {
            front_ops.push(Op::Door { s: uc, w: 2.8, h: 2.7 });
            shop_rooms.push((u0, u1, if k == SHOP_ROOM { special } else { Shop::None }));
        } else {
            front_ops.push(Op::Window { s: uc, w: 0.7, sill: 1.9, top: 2.5 });
            windows.push((uc, 0.7, 1.9, 2.5));
            inner_ops.push(Op::Door { s: uc, w: 1.1, h: 2.2 });
            plan.push(f.rect(u0, u1, OUTER_T, FRONT_D - INNER_T / 2.0));
        }
        if two_storey {
            front_ops.push(Op::Window { s: uc, w: 0.8, sill: H1 + 1.0, top: H1 + 2.0 });
            windows.push((uc, 0.8, H1 + 1.0, H1 + 2.0));
        }
    }
    f.wall_u(b, OUTER_T / 2.0, 0.0, w, OUTER_T, h_front, &front_ops, mat);
    let iv = FRONT_D - INNER_T / 2.0;
    let mut inner_wall_ops = inner_ops.clone();
    if two_storey {
        for &(u0, u1) in &rooms {
            inner_wall_ops.push(Op::Window { s: (u0 + u1) / 2.0, w: 0.9, sill: H1 + 0.9, top: H1 + 2.0 });
        }
        inner_wall_ops.push(Op::Upper { s: stair_u, w: 1.0, sill: H1 + 0.12, top: H1 + 2.3 });
    }
    f.wall_u(b, iv, OUTER_T, w - OUTER_T, INNER_T, h_front, &inner_wall_ops, mat);
    // Перегородки — на первом этаже; наверху одна большая горница.
    for k in 1..rooms.len() {
        let u = (rooms[k - 1].1 + rooms[k].0) / 2.0;
        f.wall_v(b, u, OUTER_T, iv - INNER_T / 2.0, INNER_T, if two_storey { H1 } else { h_front }, &[], mat);
    }
    let wing = f.rect(0.0, w, 0.0, FRONT_D);
    let mut stair_rect = None;
    if two_storey {
        // Перекрытие между этажами (снизу видны балки) — по нему ходят.
        let floor = f.rect(OUTER_T, w - OUTER_T, OUTER_T, iv - INNER_T / 2.0);
        b.solid([floor.x0, H1, floor.z0], [floor.x1, H1 + 0.12, floor.z1], Mat::Wood);
        // Каменная лестница со двора к двери горницы.
        let top = H1 + 0.12;
        let n = (top / 0.4).ceil();
        let foot = f.p(stair_u, 0.0, FRONT_D + n * 0.6);
        let dir = (f.p(stair_u, 0.0, FRONT_D) - foot).normalize();
        kit::stairs(b, foot, dir, 1.1, top, mat);
        stair_rect = Some(f.rect(stair_u - 0.75, stair_u + 0.75, FRONT_D, FRONT_D + n * 0.6 + 1.2));
        // Перильца из жердей вдоль открытой стороны.
        let side = Vec3::new(dir.z, 0.0, -dir.x) * 0.55;
        b.mesh(Mat::Wood).beam(foot + side + Vec3::Y * 0.9, foot + side + dir * (n * 0.6) + Vec3::Y * (top + 0.9), 0.03, 0.03, Vec3::Y);
        for k in [0.0, 0.5, 1.0] {
            let q = foot + side + dir * (n * 0.6 * k);
            b.mesh(Mat::Wood).beam(q + Vec3::Y * (top * k).max(0.0), q + Vec3::Y * (top * k + 0.9), 0.025, 0.025, Vec3::X);
        }
        let m = b.mesh(Mat::Wood);
        let n = (w / 1.1) as usize;
        for k in 0..n {
            let u = 0.6 + (w - 1.2) * (k as f32 + 0.5) / n as f32;
            let (a, c) = (f.p(u, H1 - 0.1, OUTER_T), f.p(u, H1 - 0.1, iv));
            m.beam(a, c, 0.08, 0.09, Vec3::Y);
        }
    }
    // Торцевые стены фасадного крыла (дальше по боковым улицам идут стена бокового крыла и стена двора).
    for u in [OUTER_T / 2.0, w - OUTER_T / 2.0] {
        f.wall_v(b, u, OUTER_T, FRONT_D, OUTER_T, h_front, &[], mat);
    }
    kit::lean_roof(b, wing, f.front_side(), h_front + 1.1, h_front + 0.05, 0.7);
    // Скат — во двор: фасад и торцы доведены кладкой до черепицы.
    let ends = [f.span_u(0.0, OUTER_T), f.span_u(w - OUTER_T, w)].map(|(a, c)| (a, c, h_front));
    kit::lean_fill(b, wing, f.front_side(), h_front + 1.1, h_front + 0.05, OUTER_T, h_front, &ends, mat);
    // Угловые камни.
    for (u, v) in [(0.0, 0.0), (w, 0.0)] {
        let p = f.p(u, 0.0, v);
        let sx = if p.x > f.r.cx() { -1.0 } else { 1.0 };
        let sz = if p.z > f.r.cz() { -1.0 } else { 1.0 };
        kit::quoins(b, p.x + sx * OUTER_T / 2.0, p.z + sz * OUTER_T / 2.0, h_front, OUTER_T);
    }
    // Навесы над лавками.
    let mut frng = Rng::new((f.r.x0 * 7.0 + f.r.z0 * 131.0) as i32 as u32 ^ 0x5eed);
    if shops {
        for (k, &(u0, u1)) in rooms.iter().enumerate() {
            if k != vestibule && rng.chance(0.7) {
                let out = f.front_side().out();
                let cloth = [Mat::Canvas, Mat::Canvas, Mat::Canvas, Mat::Rug, Mat::Felt, Mat::Cloth][frng.int(6)];
                kit::awning_of(b, f.p(u0 + 0.2, 0.0, -0.02), f.p(u1 - 0.2, 0.0, -0.02), out, 1.6, 2.9, cloth);
                shop_front(b, &f, u0, u1, &mut frng);
            }
        }
    }

    // Своё зерно (`frng`), чтобы мелочи фасада не сдвигали остальную планировку усадьбы.
    facade_details(b, &f, &windows, rooms[vestibule], shops, two_storey, &mut frng);

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
        plan.push(f.rect(OUTER_T, SIDE_D - INNER_T, v0 + INNER_T / 2.0, v1 - INNER_T / 2.0));
    }
    // Наружная стена по боковой улице — и вдоль кладовых, до задней стены.
    f.wall_v(b, OUTER_T / 2.0, side_v0, if has_back_wing { d - 0.3 } else { side_v1 }, OUTER_T, H1, &side_outer_ops, mat);
    f.wall_v(b, SIDE_D - INNER_T / 2.0, side_v0, side_v1, INNER_T, H1, &side_inner_ops, mat);
    if has_back_wing {
        // Торец бокового крыла со стороны кладовых.
        f.wall_u(b, side_v1, OUTER_T, SIDE_D, OUTER_T, H1, &[], mat);
    }
    let side_wing = f.rect(0.0, SIDE_D, side_v0, side_v1);
    kit::lean_roof(b, side_wing, f.side0(), H1 + 1.0, H1 + 0.05, 0.7);
    // Наружная стена и торцы — до черепицы: ближний торец стоит на внутренней стене фасадного
    // крыла, дальний — на задней стене (наша половина) или на торце со стороны кладовых.
    let (n0, n1) = f.span_v(FRONT_D - INNER_T, FRONT_D);
    let (e0, e1) = if has_back_wing { f.span_v(side_v1 - OUTER_T / 2.0, side_v1 + OUTER_T / 2.0) } else { f.span_v(d - 0.3, d) };
    kit::lean_fill(b, side_wing, f.side0(), H1 + 1.0, H1 + 0.05, OUTER_T, H1, &[(n0, n1, h_front), (e0, e1, H1)], mat);

    // ---- Заднее крыло (кладовые) или глухая стена соседа ----
    if has_back_wing {
        let v0 = d - back_d;
        f.wall_u(b, v0 + INNER_T / 2.0, SIDE_D, w - OUTER_T, INNER_T, H1, &[Op::Door { s: (SIDE_D + w) / 2.0, w: 1.2, h: 2.2 }], mat);
        let back_wing = f.rect(0.0, w, v0, d);
        let back_side = if f.front == Side::N { Side::S } else { Side::N };
        kit::lean_roof(b, back_wing, back_side, H1 + 0.9, H1 + 0.05, 0.6);
        // Задняя стена (наша половина) и торцы на боковых стенах — до черепицы.
        let ends = [f.span_u(0.0, OUTER_T), f.span_u(w - OUTER_T, w)].map(|(a, c)| (a, c, H1));
        kit::lean_fill(b, back_wing, back_side, H1 + 0.9, H1 + 0.05, 0.3, H1, &ends, mat);
    }
    // Задняя (общая с соседней усадьбой) стена — половина толщины на нашей стороне.
    f.wall_u(b, d - 0.15, 0.0, w, 0.3, H1, &[], mat);
    // Стена двора вдоль второй боковой улицы, с воротами.
    let gate_v = (FRONT_D + d - back_d) / 2.0;
    let gate = rng.chance(0.6) && !shape.narrow;
    let ops = if gate { vec![Op::Door { s: gate_v, w: 2.2, h: 2.6 }] } else { vec![] };
    let yard_wall_h = 3.0;
    if shape.narrow {
        // Межа с соседним узким домом: у каждого — половина общей стены, гребень кладёт западный.
        f.wall_v(b, w - OUTER_T / 4.0, FRONT_D, d - back_d, OUTER_T / 2.0, yard_wall_h, &[], mat);
        if !f.mirror {
            let top = f.rect(w - OUTER_T / 2.0, w + OUTER_T / 2.0, FRONT_D, d - back_d);
            b.mesh(Mat::Tile).gable_roof_slopes(Vec2::new(top.x0 - 0.08, top.z0), Vec2::new(top.x1 + 0.08, top.z1), yard_wall_h, yard_wall_h + 0.22, !f.front.along_x());
        }
    } else {
        f.wall_v(b, w - OUTER_T / 2.0, FRONT_D, d - back_d, OUTER_T, yard_wall_h, &ops, mat);
        if has_back_wing {
            f.wall_v(b, w - OUTER_T / 2.0, d - back_d, d, OUTER_T, H1, &[], mat);
        }
        // Черепичный «гребень» по верху стены двора.
        let top = f.rect(w - OUTER_T, w, FRONT_D, d - back_d);
        b.mesh(Mat::Tile).gable_roof_slopes(Vec2::new(top.x0 - 0.08, top.z0), Vec2::new(top.x1 + 0.08, top.z1), yard_wall_h, yard_wall_h + 0.22, !f.front.along_x());
    }

    // ---- Двор ----
    let court = ((SIDE_D, w - OUTER_T), (FRONT_D, d - back_d - 0.3));
    if mode != LotMode::Site {
        courtyard(b, &f, court.0, court.1, has_back_wing, rng);
    }
    let top = h_front + 1.4;
    if !lived {
        // Пустой дом: голые стены, сундук в первой комнате и ларь в кладовой — их отопрёт хозяин.
        let mut rooms: Vec<estate::RoomPlan> = plan.iter().map(|r| estate::RoomPlan { r: *r, floor: 0.0, kinds: estate::GROUND_KINDS.to_vec() }).collect();
        if has_back_wing {
            let v0 = d - back_d;
            rooms.push(estate::RoomPlan { r: f.rect(SIDE_D, w - OUTER_T, v0 + INNER_T, d - 0.15), floor: 0.0, kinds: vec![crate::level::FurnishKind::Storeroom, crate::level::FurnishKind::Kitchen, crate::level::FurnishKind::Workshop] });
        }
        if two_storey {
            rooms.push(estate::RoomPlan { r: f.rect(OUTER_T, w - OUTER_T, OUTER_T, iv - INNER_T / 2.0), floor: H1 + 0.12, kinds: estate::UPPER_KINDS.to_vec() });
        }
        for (n, name) in [(0usize, "Сундук"), (1, "Ларь")] {
            let Some(room) = rooms.get(n) else { continue };
            let r = room.r.inset(0.15);
            // В углу комнаты.
            let along_x = r.w() > r.d();
            let chest = if along_x { Rect::new(r.x1 - 1.1, r.z0, r.x1 - 0.1, r.z0 + 0.6) } else { Rect::new(r.x0, r.z1 - 1.1, r.x0 + 0.6, r.z1 - 0.1) };
            interior::chest(b, chest, name);
        }
        return HouseInfo { rooms, entrance, two_storey, top, court, back_wing: has_back_wing };
    }

    // ---- Обстановка ----
    let mut people = interior::Household::new();
    // Главная комната, кухня, спальня обязательны; остальное — по хозяйству.
    let mut kinds = vec![interior::Kind::Living, interior::Kind::Kitchen, interior::Kind::Bedroom];
    let extra = [interior::Kind::Bedroom, interior::Kind::Storeroom, interior::Kind::Weaving, interior::Kind::Workshop(interior::Trade::pick(rng))];
    while kinds.len() < plan.len() {
        kinds.push(extra[rng.int(extra.len())]);
    }
    // Мастерская — лучше в комнате с окном на улицу (первые в списке — фасадные).
    if shops && rng.chance(0.5) {
        kinds.swap(0, 2);
    }
    for (r, kind) in plan.iter().zip(kinds) {
        interior::furnish(b, *r, kind, &mut people, rng);
    }
    if has_back_wing {
        let v0 = d - back_d;
        interior::furnish(b, f.rect(SIDE_D, w - OUTER_T, v0 + INNER_T, d - 0.15), interior::Kind::Storeroom, &mut people, rng);
    }
    for (u0, u1, kind) in shop_rooms {
        shop_fittings(b, &f, u0, u1, kind, rng);
    }
    if two_storey {
        // Горница: пол на высоте второго этажа, дверь — со стороны лестницы.
        let loft = f.rect(OUTER_T, w - OUTER_T, OUTER_T, iv - INNER_T / 2.0);
        let lift = Vec3::Y * (H1 + 0.12);
        let doors: Vec<DoorSpec> = b.doors.iter().filter(|d| (d.center.y - lift.y).abs() < 0.2 && d.center.distance(f.p(stair_u, lift.y, iv)) < 0.5).map(|d| DoorSpec { center: d.center - lift, ..*d }).collect();
        let seed = rng.int(1 << 20) as u32;
        let chunk = b.chunk;
        b.placed_in(lift, chunk, move |t| {
            t.doors = doors;
            let mut rng = Rng::new(seed);
            let mut people = interior::Household::full();
            interior::furnish(t, loft, interior::Kind::Loft, &mut people, &mut rng);
            t.doors.clear();
        });
    }
    let yard = f.rect(SIDE_D, w - OUTER_T, FRONT_D, d - back_d - 0.3);
    let _ = stair_rect;
    interior::yard_extras(b, yard, stair_rect, &mut people, rng);
    HouseInfo { rooms: Vec::new(), entrance, two_storey, top, court, back_wing: has_back_wing }
}

/// Мелочи фасада, без которых стена выглядит макетом: распахнутые ставни, подоконные плиты,
/// карниз между этажами, у входа — каменная скамья и пара амфор.
fn facade_details(b: &mut Builder, f: &Frame, windows: &[(f32, f32, f32, f32)], (vu0, vu1): (f32, f32), shops: bool, two_storey: bool, rng: &mut Rng) {
    let dec = |b: &mut Builder, r: Rect, y0: f32, y1: f32, mat: Mat| b.decor([r.x0, y0, r.z0], [r.x1, y1, r.z1], mat);
    for &(u, w, sill, top) in windows {
        // Подоконная плита из известняка чуть выступает из стены.
        dec(b, f.rect(u - w / 2.0 - 0.1, u + w / 2.0 + 0.1, -0.09, 0.02), sill - 0.09, sill, Mat::Limestone);
        // Перемычка над окном — деревянный брус.
        dec(b, f.rect(u - w / 2.0 - 0.18, u + w / 2.0 + 0.18, -0.05, 0.02), top, top + 0.16, Mat::Wood);
        // Ставни: чаще распахнуты к стене, иногда одной створкой.
        if rng.chance(0.8) {
            let leaf = w / 2.0 + 0.04;
            for side in [-1.0f32, 1.0] {
                if side > 0.0 && rng.chance(0.15) {
                    continue;
                }
                let inner = u + side * (w / 2.0 + 0.05);
                let outer = inner + side * leaf;
                dec(b, f.rect(inner.min(outer), inner.max(outer), -0.07, -0.02), sill + 0.02, top - 0.02, Mat::Wood);
                // Две поперечные планки.
                for y in [sill + 0.12, top - 0.14] {
                    dec(b, f.rect(inner.min(outer) + 0.03, inner.max(outer) - 0.03, -0.095, -0.07), y, y + 0.07, Mat::Wood);
                }
            }
        }
    }
    // Тяга между этажами.
    if two_storey {
        dec(b, f.rect(0.05, f.w() - 0.05, -0.07, 0.02), H1 - 0.06, H1 + 0.1, Mat::Limestone);
    }
    // Скамья у входа и амфоры (у лавок — свои прилавки).
    if !shops {
        let door = (vu0 + vu1) / 2.0;
        if rng.chance(0.55) && door + 2.3 < f.w() - 0.3 {
            let r = f.rect(door + 1.0, door + 2.3, -0.42, -0.02);
            b.solid([r.x0, 0.0, r.z0], [r.x1, 0.42, r.z1], Mat::Limestone);
        }
        if rng.chance(0.45) && door - 1.6 > 0.4 {
            for k in 0..(1 + rng.int(2)) {
                kit::amphora(b, f.p(door - 1.1 - k as f32 * 0.42, 0.0, -0.28), 1.0, rng.int(3) as u32);
            }
        }
    }
}

/// Товар, выставленный под навесом по бокам от входа: корзины, мешки, кувшины, амфоры.
/// Только декор у самой стены (проход в лавку и улица свободны), зерно — своё.
fn shop_front(b: &mut Builder, f: &Frame, u0: f32, u1: f32, rng: &mut Rng) {
    for (u, dir) in [(u0 + 0.45, 1.0f32), (u1 - 0.45, -1.0)] {
        if rng.chance(0.1) {
            continue;
        }
        let at = |du: f32, dv: f32| f.p(u + du * dir, 0.0, -0.35 - dv);
        match rng.int(4) {
            0 => {
                // Корзины с зерном, бобами, луком.
                for k in 0..2 {
                    let fill = [Mat::Thatch, Mat::Earth, Mat::HerbGreen][rng.int(3)];
                    basket(b, at(0.0, k as f32 * 0.55), rng.range(0.18, 0.24), fill);
                }
            }
            1 => {
                // Мешки, один на другом.
                sack(b, at(0.0, 0.0), 0.62);
                sack(b, at(0.05, 0.5), 0.55);
                if rng.chance(0.5) {
                    basket(b, at(0.0, 1.0), 0.2, Mat::Thatch);
                }
            }
            2 => {
                // Кувшины и миски на низкой подставке.
                let c = at(0.0, 0.35);
                let m = b.mesh(Mat::Wood);
                m.cuboid(c - Vec3::new(0.32, 0.0, 0.32), c + Vec3::new(0.32, 0.42, 0.32));
                for k in 0..3 {
                    let q = c + Vec3::new(-0.18 + k as f32 * 0.18, 0.42, if k == 1 { 0.1 } else { -0.08 });
                    kit::jug(b, q, rng.range(0.2, 0.32), Mat::Ceramic);
                }
            }
            _ => {
                // Амфоры, прислонённые к стене.
                for k in 0..2 {
                    kit::amphora(b, at(k as f32 * 0.4, 0.0), 0.95, rng.int(3) as u32);
                }
            }
        }
    }
}

/// Плетёная корзина с насыпанным товаром.
fn basket(b: &mut Builder, p: Vec3, r: f32, fill: Mat) {
    let h = r * 1.1;
    b.mesh(Mat::Thatch).lathe(p, &[(0.0, 0.0), (r * 0.8, 0.0), (r, h * 0.85), (r * 1.04, h), (r * 0.94, h), (r * 0.86, h * 0.3), (0.0, h * 0.3)], 12);
    b.mesh(fill).lathe(p + Vec3::Y * h * 0.82, &[(r * 0.95, 0.0), (r * 0.6, r * 0.22), (0.0, r * 0.3)], 12);
}

/// Мешок из грубого холста, завязанный у горла.
fn sack(b: &mut Builder, p: Vec3, h: f32) {
    let r = h * 0.36;
    b.mesh(Mat::Felt).lathe(p, &[(0.0, 0.0), (r * 0.9, 0.0), (r * 1.05, h * 0.3), (r, h * 0.62), (r * 0.55, h * 0.85), (r * 0.18, h * 0.9), (r * 0.3, h), (0.0, h)], 10);
}

/// Настенная полка на кронштейнах (без коллизии) с кувшинами, мисками и кружками.
fn wall_shelf(b: &mut Builder, a: Vec3, c: Vec3, n: Vec3, rng: &mut Rng) {
    let along = (c - a).normalize_or_zero();
    let len = (c - a).length();
    b.mesh(Mat::Wood).beam(a + n * 0.14, c + n * 0.14, 0.13, 0.02, Vec3::Y);
    for t in [0.12, 0.88] {
        let q = a + (c - a) * t;
        b.mesh(Mat::Wood).beam(q + n * 0.02 - Vec3::Y * 0.25, q + n * 0.24 - Vec3::Y * 0.02, 0.02, 0.02, along);
    }
    let k = (len / 0.2) as usize;
    for i in 0..k {
        if rng.chance(0.3) {
            continue;
        }
        let q = a + along * (0.1 + i as f32 * 0.2) + n * 0.14 + Vec3::Y * 0.02;
        if rng.chance(0.4) {
            // Миска.
            b.mesh(Mat::Ceramic).lathe(q, &[(0.0, 0.0), (0.04, 0.0), (0.08, 0.06), (0.07, 0.06), (0.0, 0.02)], 9);
        } else {
            kit::jug(b, q, rng.range(0.1, 0.24), Mat::Ceramic);
        }
    }
}

/// Шест под потолком с подвешенными связками (лук, чеснок, травы, колбасы).
fn hanging_wares(b: &mut Builder, a: Vec3, c: Vec3, mats: &[Mat], rng: &mut Rng) {
    b.mesh(Mat::Wood).beam(a, c, 0.025, 0.025, Vec3::Y);
    let len = (c - a).length();
    let n = (len / 0.35) as usize;
    for i in 0..n {
        let q = a + (c - a) * ((i as f32 + 0.5) / n as f32);
        let drop = rng.range(0.18, 0.34);
        b.mesh(Mat::Wood).beam(q, q - Vec3::Y * 0.12, 0.004, 0.004, Vec3::X);
        let m = mats[rng.int(mats.len())];
        b.mesh(m).lathe(q - Vec3::Y * (0.12 + drop), &[(0.0, 0.0), (0.035, 0.04), (0.045, drop * 0.5), (0.03, drop), (0.0, drop)], 7);
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
    // Мелочи, которые делают лавку обжитой: полка на боковой стене и связки под потолком.
    // Своё зерно — чтобы не сдвинуть выбор товара и обстановку.
    {
        let mut drng = Rng::new((r.x0 * 3.0 + r.z0 * 17.0) as i32 as u32 ^ 0xdec0);
        let n = (f.p(u0 + 1.0, 0.0, 1.0) - f.p(u0, 0.0, 1.0)).normalize();
        let (v0, v1) = (OUTER_T + 1.8, FRONT_D - INNER_T / 2.0 - 0.6);
        if v1 - v0 > 0.6 {
            wall_shelf(b, f.p(u0 + 0.02, 1.55, v0), f.p(u0 + 0.02, 1.55, v1), n, &mut drng);
        }
        let mats: &[Mat] = match kind {
            Shop::Apothecary => &[Mat::HerbGreen, Mat::HerbSilver],
            Shop::Tavern => &[Mat::Thatch, Mat::Earth, Mat::Wood],
            Shop::Bookseller => &[],
            Shop::None => &[Mat::Thatch, Mat::HerbGreen, Mat::Earth],
        };
        if !mats.is_empty() && kind != Shop::Apothecary {
            let v = (OUTER_T + FRONT_D) / 2.0 + 0.3;
            hanging_wares(b, f.p(u0 + 0.4, H1 - 0.35, v), f.p(u1 - 0.4, H1 - 0.35, v), mats, &mut drng);
        }
        if kind == Shop::Tavern {
            // Кружки и кувшинчики на прилавке, штабель амфор в углу.
            for k in 0..4 {
                kit::jug(b, f.p(u0 + 0.5 + k as f32 * 0.35, 1.0, OUTER_T + 1.15), drng.range(0.1, 0.16), Mat::Ceramic);
            }
            for k in 0..3 {
                kit::amphora(b, f.p(u0 + 0.35 + k as f32 * 0.4, 0.0, FRONT_D - 1.3), 1.0, drng.int(3) as u32);
            }
        }
    }
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
            interior::shop_extras(b, r, &[counter, back], true, rng);
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

/// Пустырь у стены: огороды, сады и виноградники горожан. Квартал поделён межами на участки
/// разной величины — у каждого хозяина своё: грядки, смоковницы, лоза, шалаш со сторожем.
/// Ограды из бута с проходами, кипарисы вдоль межи; деревья и грядки — не по линейке.
pub fn garden(b: &mut Builder, r: Rect, seed: u32) {
    let mut rng = Rng::new(seed);
    // Только там, где квартал в городе (у стены — с запасом на улицу вдоль неё).
    let inside = |x: f32, z: f32| terrain::inside_city(x, z) && edge_distance(x, z) > 5.0;
    let inside_r = |q: Rect, m: f32| [(q.x0 + m, q.z0 + m), (q.x1 - m, q.z0 + m), (q.x0 + m, q.z1 - m), (q.x1 - m, q.z1 - m), (q.cx(), q.cz())].iter().filter(|&&(x, z)| inside(x, z)).count();
    // Межи поперёк квартала — на разной глубине; иногда участок ещё поделён вдоль.
    let mut cuts = vec![r.z0];
    let mut z = r.z0;
    loop {
        z += rng.range(10.0, 17.0);
        if r.z1 - z < 8.0 {
            break;
        }
        cuts.push(z);
    }
    cuts.push(r.z1);
    let mut plots: Vec<Rect> = Vec::new();
    for w in cuts.windows(2) {
        let (z0, z1) = (w[0], w[1]);
        if rng.chance(0.4) {
            let xm = r.x0 + r.w() * rng.range(0.38, 0.62);
            plots.push(Rect::new(r.x0, z0, xm, z1));
            plots.push(Rect::new(xm, z0, r.x1, z1));
        } else {
            plots.push(Rect::new(r.x0, z0, r.x1, z1));
        }
    }
    // Ограда квартала по улицам: низкая бутовая стенка, у каждого участка — свой проход.
    let wall_h = 0.85;
    let mut gaps_x: Vec<(f32, f32, f32)> = Vec::new(); // (z стены, x0, x1) проходов
    let mut gaps_z: Vec<(f32, f32, f32)> = Vec::new(); // (x стены, z0, z1)
    for p in &plots {
        let gw = 1.6;
        if (p.x0 - r.x0).abs() < 0.01 && rng.chance(0.6) {
            let zc = rng.range(p.z0 + 2.0, p.z1 - 2.0);
            gaps_z.push((r.x0, zc - gw / 2.0, zc + gw / 2.0));
        }
        if (p.x1 - r.x1).abs() < 0.01 && rng.chance(0.6) {
            let zc = rng.range(p.z0 + 2.0, p.z1 - 2.0);
            gaps_z.push((r.x1, zc - gw / 2.0, zc + gw / 2.0));
        }
        if (p.z0 - r.z0).abs() < 0.01 {
            let xc = rng.range(p.x0 + 2.0, p.x1 - 2.0);
            gaps_x.push((r.z0, xc - gw / 2.0, xc + gw / 2.0));
        }
        if (p.z1 - r.z1).abs() < 0.01 {
            let xc = rng.range(p.x0 + 2.0, p.x1 - 2.0);
            gaps_x.push((r.z1, xc - gw / 2.0, xc + gw / 2.0));
        }
    }
    // Стенка кусками по 2–3 м: где кусок выходит за город (к стене), его нет; иногда кладка
    // осыпалась и в стенке пролом.
    let low_wall_x = |b: &mut Builder, z: f32, x0: f32, x1: f32, gaps: &[(f32, f32)], rng: &mut Rng| {
        let mut x = x0;
        while x < x1 - 0.3 {
            let mut x_end = (x + rng.range(2.0, 3.2)).min(x1);
            if let Some(&(g0, _)) = gaps.iter().find(|&&(g0, g1)| g0 >= x - 0.01 && g0 < x_end || (x >= g0 && x < g1)) {
                if x >= g0 {
                    x = gaps.iter().find(|&&(a, c)| x >= a && x < c).map(|&(_, c)| c).unwrap_or(x_end);
                    continue;
                }
                x_end = g0;
            }
            if inside(x, z) && inside(x_end, z) && x_end - x > 0.3 {
                let h = wall_h * rng.range(0.85, 1.1);
                b.solid([x, 0.0, z - 0.25], [x_end, h, z + 0.25], Mat::Rubble);
                b.mesh(Mat::Limestone).cuboid(Vec3::new(x - 0.02, h, z - 0.28), Vec3::new(x_end + 0.02, h + 0.07, z + 0.28));
            }
            x = x_end;
        }
    };
    for &zw in &[r.z0, r.z1] {
        let gaps: Vec<(f32, f32)> = gaps_x.iter().filter(|g| (g.0 - zw).abs() < 0.01).map(|g| (g.1, g.2)).collect();
        let zz = if zw == r.z0 { zw + 0.3 } else { zw - 0.3 };
        low_wall_x(b, zz, r.x0, r.x1, &gaps, &mut rng);
    }
    let low_wall_z = |b: &mut Builder, x: f32, z0: f32, z1: f32, gaps: &[(f32, f32)], rng: &mut Rng| {
        let mut z = z0;
        while z < z1 - 0.3 {
            let mut z_end = (z + rng.range(2.0, 3.2)).min(z1);
            if let Some(&(g0, _)) = gaps.iter().find(|&&(g0, g1)| g0 >= z - 0.01 && g0 < z_end || (z >= g0 && z < g1)) {
                if z >= g0 {
                    z = gaps.iter().find(|&&(a, c)| z >= a && z < c).map(|&(_, c)| c).unwrap_or(z_end);
                    continue;
                }
                z_end = g0;
            }
            if inside(x, z) && inside(x, z_end) && z_end - z > 0.3 {
                let h = wall_h * rng.range(0.85, 1.1);
                b.solid([x - 0.25, 0.0, z], [x + 0.25, h, z_end], Mat::Rubble);
                b.mesh(Mat::Limestone).cuboid(Vec3::new(x - 0.28, h, z - 0.02), Vec3::new(x + 0.28, h + 0.07, z_end + 0.02));
            }
            z = z_end;
        }
    };
    for &xw in &[r.x0, r.x1] {
        let gaps: Vec<(f32, f32)> = gaps_z.iter().filter(|g| (g.0 - xw).abs() < 0.01).map(|g| (g.1, g.2)).collect();
        let xx = if xw == r.x0 { xw + 0.3 } else { xw - 0.3 };
        low_wall_z(b, xx, r.z0 + 0.55, r.z1 - 0.55, &gaps, &mut rng);
    }
    // Межи внутри квартала: плетень из прутьев на кольях (дешевле кладки), с перелазом.
    for p in &plots {
        if p.z1 < r.z1 - 0.01 {
            let gap = rng.range(p.x0 + 1.5, p.x1 - 3.0);
            for (a, c) in [(p.x0 + 0.6, gap), (gap + 1.4, p.x1 - 0.6)] {
                wattle(b, Vec3::new(a, 0.0, p.z1), Vec3::new(c, 0.0, p.z1), &inside, &mut rng);
            }
        }
        if p.x1 < r.x1 - 0.01 {
            let gap = rng.range(p.z0 + 1.5, p.z1 - 3.0);
            for (a, c) in [(p.z0 + 0.6, gap), (gap + 1.4, p.z1 - 0.6)] {
                wattle(b, Vec3::new(p.x1, 0.0, a), Vec3::new(p.x1, 0.0, c), &inside, &mut rng);
            }
        }
    }
    // Что на участке: у кого огород, у кого сад или лоза; шалаш — не больше одного на квартал.
    let mut hut = false;
    let mut last = 9;
    for p in &plots {
        let inner = p.inset(1.0);
        if inner.w() < 4.0 || inner.d() < 4.0 || inside_r(*p, 1.5) < 3 {
            // Клочок у стены: пара деревьев и куст, без грядок.
            for _ in 0..2 {
                let q = Vec3::new(rng.range(inner.x0, inner.x1), 0.0, rng.range(inner.z0, inner.z1));
                if inside(q.x, q.z) {
                    if rng.chance(0.6) {
                        kit::fig_tree(b, q, rng.range(2.8, 4.6), &mut rng);
                    } else {
                        kit::shrub(b, q, rng.range(0.8, 1.4));
                    }
                }
            }
            continue;
        }
        b.at(p.cx(), p.cz());
        let mut kind = rng.int(9);
        // Соседние участки разные.
        if kind / 3 == last / 3 {
            kind = (kind + 3) % 9;
        }
        last = kind;
        match kind {
            0..=2 => kitchen_garden(b, inner, &inside, &mut rng),
            3..=5 => orchard(b, inner, &inside, &mut rng),
            _ => {
                if !hut && inner.w() > 6.0 && inner.d() > 7.0 {
                    hut = true;
                    vine_yard(b, inner, &inside, true, &mut rng);
                } else {
                    vine_yard(b, inner, &inside, false, &mut rng);
                }
            }
        }
        // Кипарисы по меже — по одному-два, не строем.
        if rng.chance(0.5) {
            let n = 1 + rng.int(2);
            for _ in 0..n {
                let q = Vec3::new(rng.range(p.x0 + 0.8, p.x1 - 0.8), 0.0, if rng.chance(0.5) { p.z0 + 0.9 } else { p.z1 - 0.9 });
                if inside(q.x, q.z) {
                    kit::cypress(b, q, rng.range(5.5, 8.5));
                }
            }
        }
    }
}

/// Плетень: колья через 0,5 м и переплетённые прутья, местами подгнил и покосился.
fn wattle(b: &mut Builder, a: Vec3, c: Vec3, inside: &impl Fn(f32, f32) -> bool, rng: &mut Rng) {
    let len = a.distance(c);
    if len < 0.6 {
        return;
    }
    let dir = (c - a) / len;
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    let n = (len / 0.5).ceil() as usize;
    let h = rng.range(0.9, 1.15);
    let m = b.mesh(Mat::Wood);
    let mut seg_start: Option<Vec3> = None;
    for k in 0..=n {
        let p = a + dir * (len * k as f32 / n as f32);
        if !inside(p.x, p.z) {
            seg_start = None;
            continue;
        }
        let lean = side * rng.range(-0.04, 0.04);
        m.beam(p, p + Vec3::Y * (h + rng.range(-0.05, 0.08)) + lean, 0.025, 0.025, dir);
        if let Some(s) = seg_start {
            for y in [0.25f32, 0.5, 0.75] {
                if y < h - 0.1 {
                    let wob = if k % 2 == 0 { 0.02 } else { -0.02 };
                    m.beam(s + Vec3::Y * y + side * wob, p + Vec3::Y * y - side * wob, 0.018, 0.03, Vec3::Y);
                }
            }
        }
        seg_start = Some(p);
    }
    let (lo, hi) = (a.min(c) - Vec3::new(0.06, 0.0, 0.06), a.max(c) + Vec3::new(0.06, h, 0.06));
    if inside(a.x, a.z) && inside(c.x, c.z) {
        kit::hidden(b, lo, hi);
    }
}

/// Огород: грядки разной длины вдоль участка с тропкой, разные овощи; что-то уже убрано.
fn kitchen_garden(b: &mut Builder, r: Rect, inside: &impl Fn(f32, f32) -> bool, rng: &mut Rng) {
    let along_x = if rng.chance(0.5) { r.w() > r.d() } else { r.w() <= r.d() } || r.d() < 5.0;
    let (len_total, across) = if along_x { (r.w(), r.d()) } else { (r.d(), r.w()) };
    let path_at = across * rng.range(0.35, 0.65);
    let pitch = rng.range(1.25, 1.55);
    let mut t = 0.4;
    while t < across - 0.6 {
        if (t - path_at).abs() < 0.7 {
            t += 1.3;
            continue;
        }
        // Грядка: длина — не до самой ограды и у всех разная.
        let s0 = rng.range(0.2, 1.4);
        let s1 = len_total - rng.range(0.2, 1.8);
        let wdt = rng.range(0.75, 0.95);
        let bed = if along_x { Rect::new(r.x0 + s0, r.z0 + t, r.x0 + s1, r.z0 + t + wdt) } else { Rect::new(r.x0 + t, r.z0 + s0, r.x0 + t + wdt, r.z0 + s1) };
        if !(inside(bed.x0, bed.z0) && inside(bed.x1, bed.z1)) {
            t += pitch;
            continue;
        }
        let hgt = rng.range(0.1, 0.17);
        b.mesh(Mat::Earth).cuboid(bed.min3(0.0), bed.max3(hgt));
        let crop = rng.int(6);
        let n = ((s1 - s0) / 0.5) as usize;
        let at = |k: usize, rng: &mut Rng| {
            let u = s0 + 0.3 + (s1 - s0 - 0.6) * (k as f32 + rng.range(0.3, 0.7)) / n.max(1) as f32;
            let v = t + wdt * rng.range(0.3, 0.7);
            if along_x { Vec3::new(r.x0 + u, hgt, r.z0 + v) } else { Vec3::new(r.x0 + v, hgt, r.z0 + u) }
        };
        match crop {
            // Капуста и свёкла: кочаны разной величины, кое-где выдернуты.
            0 | 1 => {
                for k in 0..n {
                    if rng.chance(0.15) {
                        continue;
                    }
                    let p = at(k, rng);
                    let s = rng.range(0.7, 1.25);
                    b.mesh(Mat::HerbGreen).lathe(p, &[(0.0, 0.0), (0.2 * s, 0.05 * s), (0.17 * s, 0.2 * s), (0.0, 0.26 * s)], 7);
                }
            }
            // Лук и чеснок: пучки тонких перьев.
            2 => {
                for k in 0..n * 2 {
                    let p = at(k / 2, rng) + Vec3::new(rng.range(-0.15, 0.15), 0.0, rng.range(-0.15, 0.15));
                    let m = b.mesh(Mat::HerbGreen);
                    for q in 0..3 {
                        let a = q as f32 * 2.1 + rng.range(0.0, 1.0);
                        m.beam(p, p + Vec3::new(a.cos() * 0.06, rng.range(0.25, 0.4), a.sin() * 0.06), 0.008, 0.008, Vec3::X);
                    }
                }
            }
            // Бобы на тычинах: колья шалашиком, обвитые зеленью.
            3 => {
                let m = (n / 2).max(2);
                for k in 0..m {
                    let p = at(k * 2, rng);
                    let top = p + Vec3::new(rng.range(-0.08, 0.08), rng.range(1.2, 1.5), rng.range(-0.08, 0.08));
                    for d in [-0.25f32, 0.25] {
                        let foot = p + if along_x { Vec3::new(0.0, 0.0, d) } else { Vec3::new(d, 0.0, 0.0) };
                        b.mesh(Mat::Wood).beam(foot, top, 0.015, 0.015, Vec3::Y);
                    }
                    b.mesh(Mat::Leaves).lathe(p + Vec3::Y * 0.3, &[(0.0, 0.0), (0.22, 0.2), (0.2, 0.7), (0.08, 1.05), (0.0, 1.1)], 6);
                }
            }
            // Зелень и травы: низкие кустики разного цвета.
            4 => {
                for k in 0..n {
                    let p = at(k, rng);
                    let mat = [Mat::HerbGreen, Mat::HerbSilver, Mat::HerbGreen][rng.int(3)];
                    b.mesh(mat).lathe(p, &[(0.0, 0.0), (0.16, 0.03), (0.14, 0.14), (0.0, 0.2)], 6);
                }
            }
            // Вскопано под посев: бороздки.
            _ => {
                let m = b.mesh(Mat::Earth);
                let k_n = 3;
                for k in 0..k_n {
                    let v = t + wdt * (k as f32 + 0.5) / k_n as f32;
                    let (a, c) = if along_x { (Vec3::new(r.x0 + s0 + 0.1, hgt, r.z0 + v), Vec3::new(r.x0 + s1 - 0.1, hgt, r.z0 + v)) } else { (Vec3::new(r.x0 + v, hgt, r.z0 + s0 + 0.1), Vec3::new(r.x0 + v, hgt, r.z0 + s1 - 0.1)) };
                    m.beam(a, c, 0.06, 0.035, Vec3::Y);
                }
            }
        }
        t += pitch;
    }
    // Мелочи хозяина: кувшин для полива, корзина, мотыга, воткнутая в землю.
    let p = Vec3::new(r.x0 + rng.range(0.4, 1.0), 0.0, r.z0 + rng.range(0.4, 1.0));
    if inside(p.x, p.z) {
        kit::jug(b, p, 0.45, Mat::Ceramic);
        let q = p + Vec3::new(0.6, 0.0, 0.2);
        b.mesh(Mat::Wood).beam(q, q + Vec3::new(0.15, 1.2, 0.3), 0.02, 0.02, Vec3::X);
        b.mesh(Mat::Iron).beam(q + Vec3::new(-0.1, 0.02, 0.0), q + Vec3::new(0.1, 0.02, 0.0), 0.012, 0.08, Vec3::Z);
    }
}

/// Сад: смоковницы, гранаты и миндаль врассыпную — где посадили, где выросло само;
/// у старых деревьев — подпорки, под деревом — лестница для сбора и корзины.
fn orchard(b: &mut Builder, r: Rect, inside: &impl Fn(f32, f32) -> bool, rng: &mut Rng) {
    let mut trees: Vec<Vec3> = Vec::new();
    let area = r.w() * r.d();
    let want = ((area / 26.0) as usize).clamp(2, 14);
    let mut tries = 0;
    while trees.len() < want && tries < want * 12 {
        tries += 1;
        let p = Vec3::new(rng.range(r.x0 + 1.0, r.x1 - 1.0), 0.0, rng.range(r.z0 + 1.0, r.z1 - 1.0));
        // Разные расстояния между деревьями: где-то кроны сомкнулись, где-то прогалина.
        let min_d = rng.range(3.2, 5.0);
        if !inside(p.x, p.z) || trees.iter().any(|t| t.distance(p) < min_d) {
            continue;
        }
        trees.push(p);
    }
    for (k, p) in trees.iter().enumerate() {
        // Старые деревья выше, молодые саженцы — по пояс.
        let h = if k % 5 == 4 { rng.range(1.6, 2.4) } else { rng.range(3.2, 5.6) };
        kit::fig_tree(b, *p, h, rng);
    }
    if let Some(&p) = trees.first() {
        // Лестница-стремянка прислонена к стволу (верх — в кроне).
        let dir = Vec3::new(rng.range(-1.0, 1.0), 0.0, rng.range(-1.0, 1.0)).normalize_or(Vec3::X);
        let foot = p + dir * 1.2;
        let top = p + dir * 0.25 + Vec3::Y * 2.4;
        let side = Vec3::new(-dir.z, 0.0, dir.x) * 0.22;
        let m = b.mesh(Mat::Wood);
        for s in [-1.0f32, 1.0] {
            m.beam(foot + side * s, top + side * s, 0.025, 0.025, dir);
        }
        for k in 1..7 {
            let t = k as f32 / 7.0;
            let q = foot.lerp(top, t);
            m.beam(q - side, q + side, 0.015, 0.015, Vec3::Y);
        }
        for _ in 0..2 {
            let q = p + Vec3::new(rng.range(-1.6, 1.6), 0.0, rng.range(-1.6, 1.6));
            if q.distance(p) > 0.8 && inside(q.x, q.z) {
                b.mesh(Mat::Thatch).lathe(q, &[(0.18, 0.0), (0.24, 0.3), (0.26, 0.36), (0.0, 0.36)], 9);
            }
        }
    }
    // Подлесок у ограды.
    for _ in 0..rng.int(3) {
        let q = Vec3::new(rng.range(r.x0, r.x1), 0.0, if rng.chance(0.5) { r.z0 + 0.3 } else { r.z1 - 0.3 });
        if inside(q.x, q.z) {
            kit::shrub(b, q, rng.range(0.7, 1.3));
        }
    }
}

/// Виноградник: ряды лозы на шпалерах (ряды неровные, где-то лоза выпала); с шалашом
/// сторожа, давильней и пифосом у входа, если `hut`.
fn vine_yard(b: &mut Builder, r: Rect, inside: &impl Fn(f32, f32) -> bool, hut: bool, rng: &mut Rng) {
    let along_x = r.w() > r.d();
    let (len, across) = if along_x { (r.w(), r.d()) } else { (r.d(), r.w()) };
    let hut_r = hut.then(|| {
        let s = 3.2;
        if along_x { Rect::new(r.x1 - s - 0.2, r.z0 + 0.2, r.x1 - 0.2, r.z0 + s + 0.2) } else { Rect::new(r.x0 + 0.2, r.z1 - s - 0.2, r.x0 + s + 0.2, r.z1 - 0.2) }
    });
    let pitch = rng.range(1.9, 2.3);
    let mut t = rng.range(0.4, 0.9);
    while t < across - 0.3 {
        let s0 = rng.range(0.3, 1.0);
        let s1 = len - rng.range(0.3, 1.0);
        let (a, c) = if along_x { (Vec3::new(r.x0 + s0, 0.0, r.z0 + t), Vec3::new(r.x0 + s1, 0.0, r.z0 + t)) } else { (Vec3::new(r.x0 + t, 0.0, r.z0 + s0), Vec3::new(r.x0 + t, 0.0, r.z0 + s1)) };
        // Ряд обрывается у шалаша и у границы города; иногда в ряду прогал.
        let mut pieces = vec![(a, c)];
        if let Some(h) = hut_r {
            pieces = pieces
                .into_iter()
                .flat_map(|(a, c)| {
                    let hit = |p: Vec3| p.x > h.x0 - 0.8 && p.x < h.x1 + 0.8 && p.z > h.z0 - 0.8 && p.z < h.z1 + 0.8;
                    if hit(a) || hit(c) || hit((a + c) / 2.0) {
                        if along_x { vec![(a, Vec3::new(h.x0 - 0.9, 0.0, a.z))] } else { vec![(Vec3::new(a.x, 0.0, h.z1 + 0.9), c)] }
                    } else {
                        vec![(a, c)]
                    }
                })
                .collect();
        }
        if rng.chance(0.35) {
            let (a, c) = pieces[0];
            let m = rng.range(0.3, 0.7);
            let g0 = a.lerp(c, m - 0.08);
            let g1 = a.lerp(c, m + 0.08);
            pieces = vec![(a, g0), (g1, c)];
        }
        for (a, c) in pieces {
            if a.distance(c) > 1.6 && inside(a.x, a.z) && inside(c.x, c.z) {
                kit::vine_row(b, a, c, rng.range(1.35, 1.7));
            }
        }
        t += pitch;
    }
    let Some(h) = hut_r else { return };
    if !(inside(h.x0, h.z0) && inside(h.x1, h.z1)) {
        return;
    }
    // Шалаш сторожа: столбы, плетёные стенки с трёх сторон, соломенная крыша.
    let posts = [(h.x0, h.z0), (h.x1, h.z0), (h.x0, h.z1), (h.x1, h.z1)];
    for (x, z) in posts {
        b.solid([x - 0.08, 0.0, z - 0.08], [x + 0.08, 2.1, z + 0.08], Mat::Wood);
    }
    let open_side = if along_x { Side::S } else { Side::E };
    let sides = [
        (Side::N, Vec3::new(h.x0, 0.0, h.z0), Vec3::new(h.x1, 0.0, h.z0)),
        (Side::S, Vec3::new(h.x0, 0.0, h.z1), Vec3::new(h.x1, 0.0, h.z1)),
        (Side::W, Vec3::new(h.x0, 0.0, h.z0), Vec3::new(h.x0, 0.0, h.z1)),
        (Side::E, Vec3::new(h.x1, 0.0, h.z0), Vec3::new(h.x1, 0.0, h.z1)),
    ];
    for (s, a, c) in sides {
        if s == open_side {
            continue;
        }
        let m = b.mesh(Mat::Thatch);
        let dir = (c - a).normalize();
        let n = Vec3::new(-dir.z, 0.0, dir.x);
        m.obox((a + c) / 2.0 + Vec3::Y * 0.95, Vec3::new(a.distance(c) / 2.0 * dir.x.abs() + 0.04 * n.x.abs(), 0.95, a.distance(c) / 2.0 * dir.z.abs() + 0.04 * n.z.abs()), 0.0);
        let lo = a.min(c) - Vec3::new(0.06, 0.0, 0.06);
        let hi = a.max(c) + Vec3::new(0.06, 1.9, 0.06);
        b.invisible.push(Aabb::new(lo, hi));
    }
    super::outside::thatch_roof(b, h.inset(-0.35), 2.1, 1.0, along_x, 0.0, Mat::Thatch);
    // У входа: пифос, корзины для гроздей, скамья.
    let front = match open_side {
        Side::S => Vec3::new(h.cx(), 0.0, h.z1 + 0.9),
        _ => Vec3::new(h.x1 + 0.9, 0.0, h.cz()),
    };
    kit::pithos(b, front + Vec3::new(0.9, 0.0, 0.4), 1.2, 0.5);
    b.mesh(Mat::Thatch).lathe(front + Vec3::new(-0.7, 0.0, 0.2), &[(0.2, 0.0), (0.26, 0.32), (0.28, 0.38), (0.0, 0.38)], 9);
    let bench = if open_side == Side::S { Rect::new(h.x0 + 0.3, h.z1 - 0.7, h.x0 + 1.7, h.z1 - 0.35) } else { Rect::new(h.x1 - 0.7, h.z0 + 0.3, h.x1 - 0.35, h.z0 + 1.7) };
    kit::bench(b, bench);
}
