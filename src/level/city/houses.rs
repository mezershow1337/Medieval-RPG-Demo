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
        let shape = Shape { narrow: lot.narrow, step: lot.step, ..Default::default() };
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
                let name = house_name(f.r.center(0.0), shop != Shop::None, info.storeys);
                let r1 = b.residents.len() as u32;
                b.houses.push(crate::level::HouseSpec { name, min: f.r.min3(-0.5), max: f.r.max3(info.top), residents: r0..r1 });
            }
            LotMode::Vacant => {
                let (d0, c0, bl0) = (b.doors.len(), b.containers.len(), b.blocks.len());
                let info = house(b, f, false, Shop::None, &mut rng, LotMode::Vacant, shape);
                let (d1, c1) = (b.doors.len(), b.containers.len());
                let name = estate_name(f.r.center(0.0), info.storeys, false);
                estate::register(b, name, crate::towns::TownId::Herson, f.r, info.top, info.entrance, d0..d1, c0..c1, bl0, info.rooms, info.two_storey, None);
            }
            LotMode::Site => site(b, f, &mut rng),
        }
    }
}

/// Узкий дом или обычный; уступ крыши; что вокруг участка и сколько этажей.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub narrow: bool,
    pub step: f32,
    /// Улица вдоль бокового крыла (u = 0): в его наружной стене есть окна.
    pub open0: bool,
    /// Улица вдоль стены двора (u = w): в ней могут быть ворота.
    pub open1: bool,
    /// Улица позади участка: в задней стене двора — калитка.
    pub open_back: bool,
    /// Этажей у фасадного крыла (0 — как выпадет: один или два).
    pub storeys: u8,
    /// Боковое крыло — в два этажа (если фасадное не ниже).
    pub side2: bool,
    /// Насколько стены выше обычного (у соседей карнизы на разной высоте).
    pub lift: f32,
    /// Зерно мелочей фасада (0 — по месту участка на карте).
    pub seed: u32,
}

impl Default for Shape {
    fn default() -> Self {
        Shape { narrow: false, step: 0.0, open0: true, open1: true, open_back: false, storeys: 0, side2: false, lift: 0.0, seed: 0 }
    }
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

/// Ближнее именованное место внутри стен — как адрес дома.
fn near_place(c: Vec3) -> &'static str {
    crate::level::places::places()
        .iter()
        .filter(|p| crate::level::inside_city(p.pos.x, p.pos.z))
        .min_by(|a, b| a.pos.distance_squared(c).total_cmp(&b.pos.distance_squared(c)))
        .map(|p| p.name)
        .unwrap_or("Главная улица")
}

/// «Усадьба · Термы», «Двухэтажный дом в ряду · Агора».
fn estate_name(c: Vec3, storeys: u8, row: bool) -> String {
    let what = match (row, storeys) {
        (false, 3..) => "Трёхэтажная усадьба",
        (false, 2) => "Двухэтажная усадьба",
        (false, _) => "Усадьба",
        (true, 3..) => "Трёхэтажный дом в ряду",
        (true, 2) => "Двухэтажный дом в ряду",
        (true, _) => "Домик в ряду",
    };
    format!("{what} · {}", near_place(c))
}

/// «Дом · Термы», «Дом с лавкой · Агора» — как адрес жилого дома.
fn house_name(c: Vec3, shop: bool, storeys: u8) -> String {
    let what = match (shop, storeys) {
        (true, _) => "Дом с лавкой",
        (false, 3..) => "Трёхэтажный дом",
        (false, 2) => "Двухэтажный дом",
        (false, _) => "Дом",
    };
    format!("{what} · {}", near_place(c))
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
    courtyard(b, &f, info.court.0, info.court.1, info.back_wing.then_some((SIDE_D + f.w()) / 2.0), None, rng);
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
    let name = estate_name(f.r.center(0.0), info.storeys, false);
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
    pub storeys: u8,
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
    let look_seed = if shape.seed != 0 { shape.seed } else { (f.r.x0 * 13.0 + f.r.z0 * 71.0) as i32 as u32 };
    let mat = wall_mat(rng.chance(0.35), look_seed);
    // Узкий дом в паре — одноэтажный и без кладовых сзади: лестнице и заднему крылу во дворе тесно.
    let want_two = match shape.storeys {
        0 => rng.chance(0.45) && !shape.narrow,
        n => n >= 2,
    };
    let mut has_back_wing = rng.chance(0.35) && !shape.narrow && d >= 16.5;
    // Лестница на второй этаж идёт со двора: девять ступеней и площадка перед ними.
    let stair_fits = |back: bool| d - if back { 4.4 } else { 0.0 } - FRONT_D - 0.3 >= 6.7;
    if want_two && !stair_fits(has_back_wing) {
        has_back_wing = false;
    }
    let two_storey = want_two && stair_fits(has_back_wing);
    let three = two_storey && shape.storeys >= 3 && w >= 15.0;
    let side2 = two_storey && shape.side2;
    let storeys: u8 = if three { 3 } else if two_storey { 2 } else { 1 };
    // Одноэтажные дома строили в разное время и разные артели: стены разной высоты,
    // поэтому крыши квартала не идут одной линией (своё зерно — планировка не сдвигается).
    let place_seed = if shape.seed != 0 { shape.seed } else { (f.r.x0 * 13.0 + f.r.z0 * 71.0) as i32 as u32 };
    let mut hr = Rng::new(place_seed ^ 0x4e1f);
    let h_front = if three {
        H1 * 3.0 - 0.3 + shape.lift
    } else if two_storey {
        H1 * 2.0 - 0.2 + shape.lift
    } else if shape.narrow {
        // У соседних узких домов крыши — уступом (одна выше на `step`), иначе черепица
        // двух крыш легла бы в одну плоскость внахлёст.
        H1 + shape.step
    } else {
        H1 + [0.0, 0.0, 0.25, 0.5][hr.int(4)] + shape.lift * 0.5
    };
    // Боковое крыло в два этажа ниже фасадного: два объёма, а не один.
    let side_h = if side2 { H1 * 2.0 - 0.55 + shape.lift * 0.5 } else { H1 };
    let back_d = if has_back_wing { 4.4 } else { 0.0 };
    // Дверь из горницы в верхнюю комнату бокового крыла.
    let side_door_u = OUTER_T + (SIDE_D - INNER_T - OUTER_T) / 2.0;
    // Марш на третий этаж — в горнице вдоль внутренней стены, у дальнего от бокового крыла торца.
    let iv_face = FRONT_D - INNER_T;
    let (fl_v0, fl_v1) = (iv_face - 1.05, iv_face);
    let fl_us = w - OUTER_T - 1.0;
    let fl_ut = fl_us - FLIGHT_RUN;
    let rooms = front_rooms(&f);
    let vestibule = if shops { 0 } else { (1 + rng.int(2)).min(rooms.len() - 1) };
    // Что где: комнаты обставляются, когда все стены (и двери в них) уже стоят.
    let mut plan: Vec<Rect> = Vec::new();
    let mut shop_rooms: Vec<(f32, f32, Shop)> = Vec::new();
    // Лестница на второй этаж — со двора, у перегородки между средними комнатами.
    // (в узкой усадьбе — подальше от дверей бокового крыла).
    let stair_u = if rooms.len() > 2 { (rooms[1].1 + rooms[2].0) / 2.0 } else { ((rooms[0].1 + rooms[1].0) / 2.0).max(SIDE_D + 1.75).min(w - OUTER_T - 0.8) };

    // Балкон на брусьях над улицей — у горницы, не над входом (своё зерно: планировка не сдвигается).
    let balcony_room = if two_storey && hr.chance(0.4) {
        let k = hr.int(rooms.len());
        (k != vestibule && rooms[k].1 - rooms[k].0 >= 3.0).then_some(k)
    } else {
        None
    };
    let balcony_u = balcony_room.map(|k| (rooms[k].0 + rooms[k].1) / 2.0);

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
        for level in 1..storeys {
            let y = H1 * level as f32;
            if level == 1 && balcony_room == Some(k) {
                front_ops.push(Op::Upper { s: uc, w: 0.9, sill: y + 0.12, top: y + 2.25 });
                continue;
            }
            front_ops.push(Op::Window { s: uc, w: 0.8, sill: y + 1.0, top: y + 2.0 });
            windows.push((uc, 0.8, y + 1.0, y + 2.0));
        }
    }
    // Двери фасада (у лавок — двустворчатые ставни): их номера нужны лавкам.
    let front_doors = b.doors.len();
    f.wall_u(b, OUTER_T / 2.0, 0.0, w, OUTER_T, h_front, &front_ops, mat);
    let iv = FRONT_D - INNER_T / 2.0;
    let mut inner_wall_ops = inner_ops.clone();
    if two_storey {
        for (k, &(u0, u1)) in rooms.iter().enumerate() {
            // Над двухэтажным боковым крылом окна нет: там дверь в его верхнюю комнату.
            if !(side2 && k == 0) {
                inner_wall_ops.push(Op::Window { s: (u0 + u1) / 2.0, w: 0.9, sill: H1 + 0.9, top: H1 + 2.0 });
            }
            if three {
                inner_wall_ops.push(Op::Window { s: (u0 + u1) / 2.0, w: 0.9, sill: H1 * 2.0 + 0.9, top: H1 * 2.0 + 2.0 });
            }
        }
        inner_wall_ops.push(Op::Upper { s: stair_u, w: 1.0, sill: H1 + 0.12, top: H1 + 2.3 });
        if side2 {
            inner_wall_ops.push(Op::Upper { s: side_door_u, w: 1.0, sill: H1 + 0.12, top: H1 + 2.3 });
        }
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
    if three {
        // Перекрытие третьего этажа с проёмом над маршем; марш — деревянный, вдоль внутренней стены.
        let y = H1 * 2.0;
        for r in [f.rect(OUTER_T, w - OUTER_T, OUTER_T, fl_v0), f.rect(OUTER_T, fl_ut, fl_v0, fl_v1), f.rect(fl_us - 1.0, w - OUTER_T, fl_v0, fl_v1)] {
            b.solid([r.x0, y, r.z0], [r.x1, y + 0.12, r.z1], Mat::Wood);
        }
        let foot = f.p(fl_us, H1 + 0.12, (fl_v0 + fl_v1) / 2.0);
        let dir = (f.p(fl_us - 1.0, 0.0, 0.0) - f.p(fl_us, 0.0, 0.0)).normalize();
        flight(b, foot, dir, 1.0, H1);
        // Перила вдоль проёма наверху.
        let rail = f.rect(fl_ut, fl_us - 1.0, fl_v0 - 0.06, fl_v0);
        b.solid([rail.x0, y + 0.12, rail.z0], [rail.x1, y + 1.0, rail.z1], Mat::Wood);
        let m = b.mesh(Mat::Wood);
        let n = (w / 1.1) as usize;
        for k in 0..n {
            let u = 0.6 + (w - 1.2) * (k as f32 + 0.5) / n as f32;
            m.beam(f.p(u, y - 0.1, OUTER_T), f.p(u, y - 0.1, fl_v0), 0.08, 0.09, Vec3::Y);
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
    let mut frng = Rng::new(if shape.seed != 0 { shape.seed.wrapping_mul(131) } else { (f.r.x0 * 7.0 + f.r.z0 * 131.0) as i32 as u32 } ^ 0x5eed);
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
    if let Some(u) = balcony_u {
        balcony(b, &f, u, H1 + 0.12, &mut frng);
    }
    let door = (!shops).then_some(((rooms[vestibule].0 + rooms[vestibule].1) / 2.0, 1.5, 2.6));
    facade_details(b, &f, &windows, rooms[vestibule], shops, storeys, &mut frng, Finish { ops: &front_ops, mat, eave: h_front + 0.95, door });
    // Дымник на скате: черепичный колпак над очагом.
    if frng.chance(0.6) {
        let k = frng.int(rooms.len());
        let (u, v) = ((rooms[k].0 + rooms[k].1) / 2.0 + frng.range(-0.6, 0.6), FRONT_D * frng.range(0.4, 0.62));
        smoke_hood(b, &f, u, v, h_front + 1.1 - 1.05 * v / FRONT_D, mat);
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
        // Окна наружу — только на улицу: в стену соседа окон не рубят.
        if shape.open0 {
            side_outer_ops.push(Op::Window { s: vc, w: 0.6, sill: 2.0, top: 2.6 });
        }
        if side2 {
            side_inner_ops.push(Op::Window { s: vc, w: 0.8, sill: H1 + 0.95, top: H1 + 1.9 });
            if shape.open0 {
                side_outer_ops.push(Op::Window { s: vc, w: 0.7, sill: H1 + 1.0, top: H1 + 1.9 });
            }
        }
        if k > 0 {
            f.wall_u(b, v0, OUTER_T, SIDE_D - INNER_T, INNER_T, H1, &[], mat);
        }
        plan.push(f.rect(OUTER_T, SIDE_D - INNER_T, v0 + INNER_T / 2.0, v1 - INNER_T / 2.0));
    }
    // Наружная стена по боковой улице — и вдоль кладовых, до задней стены.
    f.wall_v(b, OUTER_T / 2.0, side_v0, side_v1, OUTER_T, side_h, &side_outer_ops, mat);
    if has_back_wing {
        f.wall_v(b, OUTER_T / 2.0, side_v1, d - 0.3, OUTER_T, H1, &[], mat);
    }
    f.wall_v(b, SIDE_D - INNER_T / 2.0, side_v0, side_v1, INNER_T, side_h, &side_inner_ops, mat);
    if has_back_wing {
        // Торец бокового крыла со стороны кладовых.
        f.wall_u(b, side_v1, OUTER_T, SIDE_D, OUTER_T, side_h, &[], mat);
    }
    let side_wing = f.rect(0.0, SIDE_D, side_v0, side_v1);
    // Дальняя грань комнат бокового крыла.
    let side_far = if has_back_wing { side_v1 - OUTER_T / 2.0 } else { d - 0.3 };
    let side_loft = f.rect(OUTER_T, SIDE_D - INNER_T, FRONT_D, side_far);
    if side2 {
        b.solid([side_loft.x0, H1, side_loft.z0], [side_loft.x1, H1 + 0.12, side_loft.z1], Mat::Wood);
        let m = b.mesh(Mat::Wood);
        let n = (side_len / 1.1) as usize;
        for k in 0..n {
            let v = side_v0 + side_len * (k as f32 + 0.5) / n as f32;
            m.beam(f.p(OUTER_T, H1 - 0.1, v), f.p(SIDE_D - INNER_T, H1 - 0.1, v), 0.08, 0.09, Vec3::Y);
        }
    }
    kit::lean_roof(b, side_wing, f.side0(), side_h + 1.0, side_h + 0.05, 0.7);
    // Наружная стена и торцы — до черепицы: ближний торец стоит на внутренней стене фасадного
    // крыла, дальний — на задней стене (наша половина) или на торце со стороны кладовых.
    let (n0, n1) = f.span_v(FRONT_D - INNER_T, FRONT_D);
    let (e0, e1) = if has_back_wing { f.span_v(side_v1 - OUTER_T / 2.0, side_v1 + OUTER_T / 2.0) } else { f.span_v(d - 0.3, d) };
    kit::lean_fill(b, side_wing, f.side0(), side_h + 1.0, side_h + 0.05, OUTER_T, side_h, &[(n0, n1, h_front), (e0, e1, side_h)], mat);

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
    // Если позади улица, а кладовых нет — в ней калитка во двор.
    let back_ops = if shape.open_back && !has_back_wing && w - SIDE_D > 4.0 { vec![Op::Door { s: (SIDE_D + w) / 2.0, w: 1.3, h: 2.2 }] } else { vec![] };
    if side2 {
        f.wall_u(b, d - 0.15, 0.0, SIDE_D, 0.3, side_h, &[], mat);
        f.wall_u(b, d - 0.15, SIDE_D, w, 0.3, H1, &back_ops, mat);
    } else {
        f.wall_u(b, d - 0.15, 0.0, w, 0.3, H1, &back_ops, mat);
    }
    // Стена двора вдоль второй боковой улицы, с воротами.
    let gate_v = (FRONT_D + d - back_d) / 2.0;
    let gate = rng.chance(0.6) && !shape.narrow && shape.open1;
    let ops = if gate { vec![Op::Door { s: gate_v, w: 2.2, h: 2.6 }] } else { vec![] };
    let yard_wall_h = 3.0;
    if shape.narrow {
        // Межа с соседним узким домом: у каждого — половина общей стены, гребень кладёт западный.
        f.wall_v(b, w - OUTER_T / 4.0, FRONT_D, d - back_d, OUTER_T / 2.0, yard_wall_h, &[], mat);
        if !f.mirror {
            let top = f.rect(w - OUTER_T / 2.0, w + OUTER_T / 2.0, FRONT_D, d - back_d);
            b.roof(Mat::Tile, |m| m.gable_roof_slopes(Vec2::new(top.x0 - 0.08, top.z0), Vec2::new(top.x1 + 0.08, top.z1), yard_wall_h, yard_wall_h + 0.22, !f.front.along_x()));
        }
    } else {
        f.wall_v(b, w - OUTER_T / 2.0, FRONT_D, d - back_d, OUTER_T, yard_wall_h, &ops, mat);
        if has_back_wing {
            f.wall_v(b, w - OUTER_T / 2.0, d - back_d, d, OUTER_T, H1, &[], mat);
        }
        // Черепичный «гребень» по верху стены двора.
        let top = f.rect(w - OUTER_T, w, FRONT_D, d - back_d);
        b.roof(Mat::Tile, |m| m.gable_roof_slopes(Vec2::new(top.x0 - 0.08, top.z0), Vec2::new(top.x1 + 0.08, top.z1), yard_wall_h, yard_wall_h + 0.22, !f.front.along_x()));
    }

    // ---- Двор ----
    let court = ((SIDE_D, w - OUTER_T), (FRONT_D, d - back_d - 0.3));
    if mode != LotMode::Site {
        let back_door = (has_back_wing || !back_ops.is_empty()).then_some((SIDE_D + w) / 2.0);
        courtyard(b, &f, court.0, court.1, back_door, gate.then_some(gate_v), rng);
    }
    let top = h_front + 1.4;
    // Комната третьего этажа (без полосы марша) и что в горнице нельзя заставлять: марш и подход к нему.
    let third = f.rect(OUTER_T, w - OUTER_T, OUTER_T, fl_v0 - 0.1);
    let mut loft_clear: Vec<Vec3> = Vec::new();
    if let Some(u) = balcony_u {
        loft_clear.push(f.p(u, 0.0, OUTER_T + 0.6));
    }
    if three {
        let mut u = fl_ut - 0.4;
        while u < w - OUTER_T {
            loft_clear.push(f.p(u, 0.0, fl_v0 + 0.3));
            u += 0.9;
        }
    }
    if !lived {
        // Пустой дом: голые стены, сундук в первой комнате и ларь в кладовой — их отопрёт хозяин.
        let mut rooms: Vec<estate::RoomPlan> = plan.iter().map(|r| estate::RoomPlan { r: *r, floor: 0.0, kinds: estate::GROUND_KINDS.to_vec() }).collect();
        if has_back_wing {
            let v0 = d - back_d;
            rooms.push(estate::RoomPlan { r: f.rect(SIDE_D, w - OUTER_T, v0 + INNER_T, d - 0.15), floor: 0.0, kinds: vec![crate::level::FurnishKind::Storeroom, crate::level::FurnishKind::Kitchen, crate::level::FurnishKind::Workshop] });
        }
        if two_storey {
            // С маршем на третий этаж горница для обстановки короче: полоса у марша свободна.
            let r = if three { f.rect(OUTER_T, w - OUTER_T, OUTER_T, fl_v0 - 0.1) } else { f.rect(OUTER_T, w - OUTER_T, OUTER_T, iv - INNER_T / 2.0) };
            rooms.push(estate::RoomPlan { r, floor: H1 + 0.12, kinds: estate::UPPER_KINDS.to_vec() });
        }
        if side2 {
            rooms.push(estate::RoomPlan { r: side_loft, floor: H1 + 0.12, kinds: estate::UPPER_KINDS.to_vec() });
        }
        if three {
            rooms.push(estate::RoomPlan { r: third, floor: H1 * 2.0 + 0.12, kinds: estate::UPPER_KINDS.to_vec() });
        }
        for (n, name) in [(0usize, "Сундук"), (1, "Ларь")] {
            let Some(room) = rooms.get(n) else { continue };
            let r = room.r.inset(0.15);
            // В углу комнаты.
            let along_x = r.w() > r.d();
            let chest = if along_x { Rect::new(r.x1 - 1.1, r.z0, r.x1 - 0.1, r.z0 + 0.6) } else { Rect::new(r.x0, r.z1 - 1.1, r.x0 + 0.6, r.z1 - 0.1) };
            interior::chest(b, chest, name);
        }
        return HouseInfo { rooms, entrance, two_storey, storeys, top, court, back_wing: has_back_wing };
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
        shop_fittings(b, &f, u0, u1, kind, front_doors, FRONT_D - INNER_T / 2.0, rng);
    }
    if two_storey {
        // Горница: пол на высоте второго этажа, дверь — со стороны лестницы.
        let loft = f.rect(OUTER_T, w - OUTER_T, OUTER_T, iv - INNER_T / 2.0);
        let lift = Vec3::Y * (H1 + 0.12);
        let upper_doors = |b: &Builder, at: Vec3| -> Vec<DoorSpec> { b.doors.iter().filter(|d| (d.center.y - lift.y).abs() < 0.2 && d.center.distance(at) < 0.5).map(|d| DoorSpec { center: d.center - lift, ..*d }).collect() };
        let mut doors = upper_doors(b, f.p(stair_u, lift.y, iv));
        if let Some(u) = balcony_u {
            doors.extend(upper_doors(b, f.p(u, lift.y, OUTER_T / 2.0)));
        }
        if side2 {
            doors.extend(upper_doors(b, f.p(side_door_u, lift.y, iv)));
        }
        let seed = rng.int(1 << 20) as u32;
        let chunk = b.chunk;
        let clear = loft_clear.clone();
        b.placed_in(lift, chunk, move |t| {
            t.doors = doors;
            t.keep_clear = clear;
            let mut rng = Rng::new(seed);
            let mut people = interior::Household::full();
            interior::furnish(t, loft, interior::Kind::Loft, &mut people, &mut rng);
            t.doors.clear();
        });
        if side2 {
            // Верхняя комната бокового крыла — спальня; дверь — из горницы.
            let doors = upper_doors(b, f.p(side_door_u, lift.y, iv));
            let seed = rng.int(1 << 20) as u32;
            b.placed_in(lift, chunk, move |t| {
                t.doors = doors;
                let mut rng = Rng::new(seed);
                interior::furnish(t, side_loft, interior::Kind::Bedroom, &mut interior::Household::full(), &mut rng);
                t.doors.clear();
            });
        }
        if three {
            // Третий этаж: ещё спальня, кладовая или ткацкая; сход с марша свободен.
            let seed = rng.int(1 << 20) as u32;
            let kind = [interior::Kind::Loft, interior::Kind::Storeroom, interior::Kind::Weaving, interior::Kind::Bedroom][rng.int(4)];
            let clear = vec![f.p(fl_ut - 0.6, 0.0, fl_v0 - 0.7), f.p(fl_ut + 0.6, 0.0, fl_v0 - 0.7)];
            b.placed_in(Vec3::Y * (H1 * 2.0 + 0.12), chunk, move |t| {
                t.keep_clear = clear;
                let mut rng = Rng::new(seed);
                interior::furnish(t, third, kind, &mut interior::Household::full(), &mut rng);
            });
        }
    }
    let yard = f.rect(SIDE_D, w - OUTER_T, FRONT_D, d - back_d - 0.3);
    let _ = stair_rect;
    interior::yard_extras(b, yard, stair_rect, &mut people, rng);
    HouseInfo { rooms: Vec::new(), entrance, two_storey, storeys, top, court, back_wing: has_back_wing }
}

/// Число ступеней и длина деревянного марша между этажами.
const FLIGHT_STEPS: usize = 9;
const FLIGHT_RUN: f32 = FLIGHT_STEPS as f32 * 0.5;

/// Деревянный марш: ступени-плахи на двух косоурах, от `foot` в сторону `dir` (вдоль X или Z)
/// на высоту `rise`. Ступени тонкие: под верхней половиной марша можно пройти.
fn flight(b: &mut Builder, foot: Vec3, dir: Vec3, width: f32, rise: f32) {
    let across = Vec3::new(dir.z, 0.0, -dir.x);
    let side = across.abs() * (width / 2.0);
    let step = rise / FLIGHT_STEPS as f32;
    for k in 0..FLIGHT_STEPS {
        let top = foot.y + step * (k + 1) as f32;
        let (a, c) = (foot + dir * (0.5 * k as f32), foot + dir * (0.5 * (k + 1) as f32));
        let (min, max) = (a.min(c) - side, a.max(c) + side);
        b.solid([min.x, top - 0.09, min.z], [max.x, top, max.z], Mat::Wood);
    }
    let lift = Vec3::Y * (step * 0.5 - 0.2);
    let end = foot + dir * FLIGHT_RUN + Vec3::Y * rise;
    for s in [-1.0f32, 1.0] {
        let o = across * (s * (width / 2.0 - 0.04)) + lift;
        b.mesh(Mat::Wood).beam(foot + o, end + o, 0.035, 0.1, Vec3::Y);
    }
}

/// Глубина корпуса дома в ряду (в два-три этажа: внутри — сени с маршем).
const ROW_D: f32 = 7.8;
/// Ширина сеней с маршем и толщина их перегородки.
const HALL_W: f32 = 1.2;
const HALL_T: f32 = 0.3;

/// Дом в ряду: узкий корпус во всю ширину участка, за ним дворик за стеной. Внизу — жилая
/// комната с очагом или лавка, сбоку — сени с деревянным маршем наверх; наверху — горница,
/// выше — спальня или кладовая. Одноэтажный — просто комната и дворик (беднота у стен).
fn townhouse(b: &mut Builder, f: Frame, shop: bool, rng: &mut Rng, mode: LotMode, shape: Shape) -> HouseInfo {
    let lived = mode == LotMode::Lived;
    let (w, d) = (f.w(), f.d());
    let storeys = if d < ROW_D + 2.4 || w < 7.4 { 1 } else { shape.storeys.clamp(1, 3) as usize };
    let bd = if storeys == 1 { (d - 2.6).clamp(4.6, 5.6) } else { ROW_D };
    let mat = wall_mat(rng.chance(0.5), shape.seed);
    let h = storeys as f32 * H1 - if storeys > 1 { 0.25 } else { 0.0 } + shape.lift * if storeys > 1 { 1.0 } else { 0.6 };
    let hall = storeys > 1;
    let back = bd - OUTER_T;
    let (ru0, ru1) = (OUTER_T, if hall { w - OUTER_T - HALL_W - HALL_T } else { w - OUTER_T });
    let (hu0, hu1) = (w - OUTER_T - HALL_W, w - OUTER_T);
    let (rc, hc) = ((ru0 + ru1) / 2.0, (hu0 + hu1) / 2.0);
    // Марш: подход — у фасадной стены, наверху — площадка у задней.
    let v_s = OUTER_T + 1.2;
    let v_t = v_s + FLIGHT_RUN;
    let mut frng = Rng::new(shape.seed.wrapping_mul(131) ^ 0x70f1);

    let mut windows: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut front_ops = Vec::new();
    let door_u = if shop { rc } else { ru0 + 1.25 };
    if shop {
        front_ops.push(Op::Door { s: rc, w: 2.8, h: 2.7 });
        if hall {
            front_ops.push(Op::Door { s: hc, w: 1.0, h: 2.3 });
        }
    } else {
        front_ops.push(Op::Door { s: door_u, w: 1.3, h: 2.4 });
        if ru1 - ru0 > 4.4 {
            let u = ru1 - 1.3;
            front_ops.push(Op::Window { s: u, w: 0.7, sill: 1.7, top: 2.5 });
            windows.push((u, 0.7, 1.7, 2.5));
        }
    }
    // Балкон над улицей — у горницы, со стороны комнаты (у другого торца — марш).
    let balcony_u = (storeys > 1 && w >= 8.0 && frng.chance(0.3)).then_some(w * 0.27);
    for level in 1..storeys {
        let y = H1 * level as f32;
        for u in [w * 0.27, w * 0.73] {
            if level == 1 && balcony_u == Some(u) {
                front_ops.push(Op::Upper { s: u, w: 0.9, sill: y + 0.12, top: y + 2.25 });
                continue;
            }
            front_ops.push(Op::Window { s: u, w: 0.8, sill: y + 1.0, top: y + 2.0 });
            windows.push((u, 0.8, y + 1.0, y + 2.0));
        }
    }
    let front_doors = b.doors.len();
    f.wall_u(b, OUTER_T / 2.0, 0.0, w, OUTER_T, h, &front_ops, mat);
    // Задняя стена: дверь во двор (из лавки её нет: там полки с запасом) и окна наверху.
    let mut back_ops = Vec::new();
    if !shop {
        back_ops.push(Op::Door { s: rc, w: 1.1, h: 2.2 });
    }
    for level in 1..storeys {
        let y = H1 * level as f32;
        back_ops.push(Op::Window { s: rc, w: 0.9, sill: y + 0.95, top: y + 1.95 });
    }
    f.wall_u(b, bd - OUTER_T / 2.0, 0.0, w, OUTER_T, h, &back_ops, mat);
    // Торцы: окна — только на улицу и только со стороны комнаты (у другого торца — марш).
    let mut end_ops = Vec::new();
    if shape.open0 {
        end_ops.push(Op::Window { s: bd * 0.5, w: 0.6, sill: 1.9, top: 2.5 });
        for level in 1..storeys {
            let y = H1 * level as f32;
            end_ops.push(Op::Window { s: bd * 0.5, w: 0.8, sill: y + 1.0, top: y + 2.0 });
        }
    }
    f.wall_v(b, OUTER_T / 2.0, OUTER_T, back, OUTER_T, h, &end_ops, mat);
    f.wall_v(b, w - OUTER_T / 2.0, OUTER_T, back, OUTER_T, h, &[], mat);
    if hall {
        // Перегородка сеней; из жилой комнаты в сени — дверь у подхода к маршу (в лавке сени свои, с улицы).
        let ops = if shop { vec![] } else { vec![Op::Door { s: OUTER_T + 0.65, w: 1.0, h: 2.2 }] };
        f.wall_v(b, hu0 - HALL_T / 2.0, OUTER_T, back, HALL_T, H1, &ops, mat);
        for level in 1..storeys {
            let y = H1 * level as f32;
            // Перекрытие с проёмом над маршем.
            for r in [f.rect(OUTER_T, hu0, OUTER_T, back), f.rect(hu0, hu1, OUTER_T, v_s + 1.0), f.rect(hu0, hu1, v_t, back)] {
                b.solid([r.x0, y, r.z0], [r.x1, y + 0.12, r.z1], Mat::Wood);
            }
            let foot_y = if level == 1 { 0.0 } else { H1 * (level - 1) as f32 + 0.12 };
            let dir = (f.p(hc, 0.0, v_s + 1.0) - f.p(hc, 0.0, v_s)).normalize();
            flight(b, f.p(hc, foot_y, v_s), dir, HALL_W - 0.1, y + 0.12 - foot_y);
            // Перила вдоль проёма.
            let rail = f.rect(hu0 - 0.06, hu0, v_s + 1.0, v_t);
            b.solid([rail.x0, y + 0.12, rail.z0], [rail.x1, y + 1.0, rail.z1], Mat::Wood);
            let m = b.mesh(Mat::Wood);
            let n = ((hu0 - OUTER_T) / 1.1) as usize;
            for k in 0..n {
                let u = OUTER_T + (hu0 - OUTER_T) * (k as f32 + 0.5) / n as f32;
                m.beam(f.p(u, y - 0.1, OUTER_T), f.p(u, y - 0.1, back), 0.07, 0.09, Vec3::Y);
            }
        }
    }
    // Крыша: двускатная (конёк вдоль улицы) или односкатная во двор.
    let wing = f.rect(0.0, w, 0.0, bd);
    let gable = rng.chance(0.55);
    if gable {
        kit::gable_roof(b, wing, h, rng.range(1.2, 1.7), f.front.along_x(), 0.3, mat);
    } else {
        kit::lean_roof(b, wing, f.front_side(), h + 1.1, h + 0.05, 0.5);
        let ends = [f.span_u(0.0, OUTER_T), f.span_u(w - OUTER_T, w)].map(|(a, c)| (a, c, h));
        kit::lean_fill(b, wing, f.front_side(), h + 1.1, h + 0.05, OUTER_T, h, &ends, mat);
    }
    for (u, v) in [(0.0, 0.0), (w, 0.0)] {
        let p = f.p(u, 0.0, v);
        let sx = if p.x > f.r.cx() { -1.0 } else { 1.0 };
        let sz = if p.z > f.r.cz() { -1.0 } else { 1.0 };
        kit::quoins(b, p.x + sx * OUTER_T / 2.0, p.z + sz * OUTER_T / 2.0, h, OUTER_T);
    }
    if shop {
        let cloth = [Mat::Canvas, Mat::Canvas, Mat::Rug, Mat::Felt, Mat::Cloth][frng.int(5)];
        kit::awning_of(b, f.p(ru0 + 0.2, 0.0, -0.02), f.p(ru1 - 0.2, 0.0, -0.02), f.front_side().out(), 1.6, 2.9, cloth);
        shop_front(b, &f, ru0, ru1, &mut frng);
    }
    if let Some(u) = balcony_u {
        balcony(b, &f, u, H1 + 0.12, &mut frng);
    }
    let door = (!shop).then_some((door_u, 1.3, 2.4));
    facade_details(b, &f, &windows, (door_u - 0.6, door_u + 0.6), shop, storeys as u8, &mut frng, Finish { ops: &front_ops, mat, eave: if gable { h - 0.1 } else { h + 0.95 }, door });
    if !gable && frng.chance(0.5) {
        let (u, v) = (rc + frng.range(-0.5, 0.5), bd * frng.range(0.4, 0.6));
        smoke_hood(b, &f, u, v, h + 1.1 - 1.05 * v / bd, mat);
    }

    // ---- Дворик ----
    let yard_h = 2.3;
    let side_gate = |open: bool, frng: &mut Rng| if open && d - bd > 3.4 && frng.chance(0.5) { vec![Op::Door { s: (bd + d) / 2.0, w: 1.2, h: 2.1 }] } else { vec![] };
    let g0 = side_gate(shape.open0, &mut frng);
    // Двор побольше — с цистерной и хозяйством, тесный — только с мелочами у стен.
    let roomy = d - bd - 0.3 >= 6.5 && g0.is_empty();
    f.wall_v(b, 0.15, bd, d, 0.3, yard_h, &g0, mat);
    f.wall_v(b, w - 0.15, bd, d, 0.3, yard_h, &[], mat);
    let gate_u = ((SIDE_D + w) / 2.0).min(w - 1.2);
    let yard_back = if shape.open_back { vec![Op::Door { s: gate_u, w: 1.3, h: 2.1 }] } else { vec![] };
    f.wall_u(b, d - 0.15, 0.0, w, 0.3, yard_h, &yard_back, mat);
    let court = ((0.3, w - 0.3), (bd, d - 0.3));
    if mode != LotMode::Site && roomy {
        courtyard(b, &f, court.0, court.1, shape.open_back.then_some(gate_u), None, rng);
    }

    let top = h + if gable { 1.8 } else { 1.4 };
    let entrance = f.p(door_u, 0.0, -1.2);
    let room = f.rect(ru0, ru1, OUTER_T, back);
    let upper = f.rect(OUTER_T, hu0 - 0.08, OUTER_T, back);
    // Наверху не заставлять: сход с марша, подход к следующему и проход между ними.
    let mut upper_clear = vec![f.p(hu0 - 0.6, 0.0, OUTER_T + 0.7), f.p(hu0 - 0.6, 0.0, v_s + 0.6), f.p(hu0 - 0.6, 0.0, (v_t + back) / 2.0)];
    let two_storey = storeys > 1;
    if let Some(u) = balcony_u {
        upper_clear.push(f.p(u, 0.0, OUTER_T + 0.6));
    }
    if !lived {
        let mut rooms = vec![estate::RoomPlan { r: room, floor: 0.0, kinds: estate::GROUND_KINDS.to_vec() }];
        for level in 1..storeys {
            rooms.push(estate::RoomPlan { r: upper, floor: H1 * level as f32 + 0.12, kinds: estate::UPPER_KINDS.to_vec() });
        }
        if two_storey {
            b.keep_clear.extend(upper_clear.iter().copied());
        }
        let r = room.inset(0.15);
        let along_x = r.w() > r.d();
        let chest = if along_x { Rect::new(r.x1 - 1.1, r.z0, r.x1 - 0.1, r.z0 + 0.6) } else { Rect::new(r.x0, r.z1 - 1.1, r.x0 + 0.6, r.z1 - 0.1) };
        interior::chest(b, chest, "Сундук");
        return HouseInfo { rooms, entrance, two_storey, storeys: storeys as u8, top, court, back_wing: false };
    }

    // ---- Обстановка ----
    // В домах в ряд живут теснее и беднее: детей во дворике нет, а в части домов днём никого
    // (хозяева на работе в порту и мастерских) — город и так многолюден.
    let mut people = interior::Household { child: true, ..interior::Household::new() };
    if frng.chance(0.4) {
        people = interior::Household::full();
    }
    if shop {
        shop_fittings(b, &f, ru0, ru1, Shop::None, front_doors, back, rng);
        people = interior::Household::full();
    } else {
        let kind = if rng.chance(0.5) { interior::Kind::Living } else { interior::Kind::Kitchen };
        interior::furnish(b, room, kind, &mut people, rng);
        if storeys == 1 {
            // Одна комната на всё: тут же и спят (ложе встаёт туда, где осталось место).
            let seed = frng.int(1 << 20) as u32;
            interior::furnish(b, room, interior::Kind::Loft, &mut interior::Household::full(), &mut Rng::new(seed));
        }
    }
    let chunk = b.chunk;
    for level in 1..storeys {
        let kind = match (level, shop) {
            (1, true) => interior::Kind::Living,
            (1, false) => interior::Kind::Loft,
            _ => [interior::Kind::Bedroom, interior::Kind::Storeroom, interior::Kind::Weaving][rng.int(3)],
        };
        let seed = rng.int(1 << 20) as u32;
        let clear = upper_clear.clone();
        b.placed_in(Vec3::Y * (H1 * level as f32 + 0.12), chunk, move |t| {
            t.keep_clear = clear;
            let mut rng = Rng::new(seed);
            interior::furnish(t, upper, kind, &mut interior::Household::full(), &mut rng);
        });
    }
    if !shop || shape.open_back {
        interior::yard_extras(b, f.rect(0.3, w - 0.3, bd, d - 0.3), None, &mut people, rng);
    }
    HouseInfo { rooms: Vec::new(), entrance, two_storey, storeys: storeys as u8, top, court, back_wing: false }
}

/// Какая доля мелких вещей остаётся в усадьбе и в доме в ряд (см. [`plan_houses`]).
const ESTATE_ITEMS_KEEP: f32 = 0.68;
const ROW_ITEMS_KEEP: f32 = 0.85;

/// Жилая застройка по плану города (см. [`super::plan`]).
pub fn plan_houses(b: &mut Builder) {
    use super::plan::{LotKind, opposite, rotated, side_index};
    for lot in &super::plan::plan().lots {
        b.at(lot.r.cx(), lot.r.cz());
        match lot.kind {
            LotKind::Garden => {
                garden(b, lot.r, lot.seed);
                continue;
            }
            LotKind::Square => {
                square(b, lot.r, lot.seed);
                continue;
            }
            LotKind::Estate | LotKind::Row => {}
        }
        let row = lot.kind == LotKind::Row;
        let mut rng = Rng::new(lot.seed);
        let mirror = rng.chance(0.5);
        // Фасадом на север или юг дом строится на месте; на восток и запад — в своих осях
        // (фасад на север) и поворачивается.
        let (turn, f, at) = match lot.face {
            Side::N | Side::S => (0u8, Frame { r: lot.r, front: lot.face, mirror }, Vec3::ZERO),
            Side::E | Side::W => {
                let (hw, hd) = (lot.r.d() / 2.0, lot.r.w() / 2.0);
                (if lot.face == Side::E { 1 } else { 3 }, Frame { r: Rect::new(-hw, -hd, hw, hd), front: Side::N, mirror }, lot.r.center(0.0))
            }
        };
        // Улицы вокруг участка — в осях дома.
        let open = |local: Side| lot.open[side_index(rotated(local, turn))];
        let back_side = opposite(f.front);
        let shape = Shape { narrow: false, step: 0.0, open0: open(f.side0()), open1: open(opposite(f.side0())), open_back: open(back_side), storeys: lot.storeys, side2: lot.side2, lift: lot.lift, seed: lot.seed | 1 };
        let centre = lot.r.center(0.0);
        let shops = lot.shops;
        let build = |t: &mut Builder, rng: &mut Rng, mode: LotMode| if row { townhouse(t, f, shops, rng, mode, shape) } else { house(t, f, shops, Shop::None, rng, mode, shape) };
        match lot.mode {
            LotMode::Lived => {
                let (h, r0) = (b.houses.len() as u32, b.residents.len() as u32);
                b.house = Some(h);
                b.wealth = lot.wealth;
                let items0 = b.house_items.len();
                let mut info = None;
                if turn == 0 {
                    info = Some(build(b, &mut rng, LotMode::Lived));
                } else {
                    b.turned(at, turn, |t| info = Some(build(t, &mut rng, LotMode::Lived)));
                }
                b.house = None;
                b.wealth = 1;
                let info = info.expect("дом построен");
                // Домов стало вдвое больше: мелочей в каждом — поменьше, чтобы серверу хватало сил
                // (вещь — отдельное тело с физикой). Что убрать, решает зерно участка.
                let keep = if row { ROW_ITEMS_KEEP } else { ESTATE_ITEMS_KEEP };
                let mut k = 0usize;
                let mut thin = Rng::new(lot.seed ^ 0x17e5);
                b.house_items.retain(|_| {
                    k += 1;
                    k <= items0 || thin.chance(keep)
                });
                let name = house_name(centre, shops, info.storeys);
                let r1 = b.residents.len() as u32;
                b.houses.push(crate::level::HouseSpec { name, min: lot.r.min3(-0.5), max: lot.r.max3(info.top), residents: r0..r1 });
            }
            LotMode::Vacant => {
                let register = |t: &mut Builder, rng: &mut Rng| {
                    let (d0, c0, bl0) = (t.doors.len(), t.containers.len(), t.blocks.len());
                    let info = build(t, rng, LotMode::Vacant);
                    let (d1, c1) = (t.doors.len(), t.containers.len());
                    let name = estate_name(centre, info.storeys, row);
                    estate::register(t, name, crate::towns::TownId::Herson, f.r, info.top, info.entrance, d0..d1, c0..c1, bl0, info.rooms, info.two_storey, None);
                };
                if turn == 0 {
                    register(b, &mut rng);
                } else {
                    b.turned(at, turn, |t| register(t, &mut rng));
                }
            }
            LotMode::Site => site(b, f, &mut rng),
        }
    }
}

/// Площадь в жилых кварталах: утоптанная земля, колодец-цистерна под навесом, дерево,
/// каменные скамьи, поилка. Середина свободна — тут сходятся переулки.
fn square(b: &mut Builder, r: Rect, seed: u32) {
    let mut rng = Rng::new(seed ^ 0x5a0a);
    if r.w() < 7.0 || r.d() < 7.0 {
        return;
    }
    let free = |b: &Builder, p: Vec3| b.keep_clear.iter().all(|q| q.xz().distance(p.xz()) > 3.0);
    // Колодец — в стороне от середины.
    let c = r.center(0.0);
    let well = c + Vec3::new(rng.range(-0.25, 0.25) * r.w(), 0.0, rng.range(-0.25, 0.25) * r.d());
    if free(b, well) {
        kit::cistern_mouth(b, well);
        // Навес на четырёх столбах.
        let m = b.mesh(Mat::Wood);
        for (dx, dz) in [(-1.1, -1.1), (1.1, -1.1), (-1.1, 1.1), (1.1, 1.1)] {
            let q = well + Vec3::new(dx, 0.0, dz);
            m.beam(q, q + Vec3::Y * 2.5, 0.07, 0.07, Vec3::X);
        }
        kit::hidden(b, well + Vec3::new(-1.17, 0.0, -1.17), well + Vec3::new(-1.03, 2.5, -1.03));
        kit::hidden(b, well + Vec3::new(1.03, 0.0, -1.17), well + Vec3::new(1.17, 2.5, -1.03));
        kit::hidden(b, well + Vec3::new(-1.17, 0.0, 1.03), well + Vec3::new(-1.03, 2.5, 1.17));
        kit::hidden(b, well + Vec3::new(1.03, 0.0, 1.03), well + Vec3::new(1.17, 2.5, 1.17));
        kit::hip_roof(b, Rect::new(well.x - 1.5, well.z - 1.5, well.x + 1.5, well.z + 1.5), 2.5, 0.8);
    }
    // Дерево в углу, скамьи по краям.
    let corner = Vec3::new(if rng.chance(0.5) { r.x0 + 2.6 } else { r.x1 - 2.6 }, 0.0, if rng.chance(0.5) { r.z0 + 2.6 } else { r.z1 - 2.6 });
    if free(b, corner) && corner.distance(well) > 4.0 {
        if rng.chance(0.5) {
            kit::oak(b, corner, rng.range(6.0, 8.0), &mut rng);
        } else {
            kit::fig_tree(b, corner, rng.range(4.5, 5.5), &mut rng);
        }
    }
    for k in 0..3 {
        let along_x = k % 2 == 0;
        let t = rng.range(0.25, 0.75);
        let bench = if along_x {
            let x = r.x0 + r.w() * t;
            let z = if k == 0 { r.z0 + 0.5 } else { r.z1 - 0.95 };
            Rect::new(x - 0.8, z, x + 0.8, z + 0.45)
        } else {
            let z = r.z0 + r.d() * t;
            Rect::new(r.x0 + 0.5, z - 0.8, r.x0 + 0.95, z + 0.8)
        };
        let p = bench.center(0.0);
        if free(b, p) && p.distance(well) > 2.6 && p.distance(corner) > 1.6 {
            b.solid([bench.x0, 0.0, bench.z0], [bench.x1, 0.42, bench.z1], Mat::Limestone);
        }
    }
}

/// Мелочи фасада, без которых стена выглядит макетом: распахнутые ставни, подоконные плиты,
/// карниз между этажами, у входа — каменная скамья и пара амфор.
fn facade_details(b: &mut Builder, f: &Frame, windows: &[(f32, f32, f32, f32)], (vu0, vu1): (f32, f32), shops: bool, storeys: u8, rng: &mut Rng, fin: Finish) {
    let dec = |b: &mut Builder, r: Rect, y0: f32, y1: f32, mat: Mat| b.decor([r.x0, y0, r.z0], [r.x1, y1, r.z1], mat);
    finish(b, f, windows, &fin, storeys, rng);
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
    for level in 1..storeys {
        let y = H1 * level as f32;
        dec(b, f.rect(0.05, f.w() - 0.05, -0.07, 0.02), y - 0.06, y + 0.1, Mat::Limestone);
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

/// Отделка стен дома: бут на глине, старая штукатурка или побелка — белая, с жёлтой охрой,
/// с толчёной керамикой. `plaster` — бросок самого дома; ещё часть домов белит своё зерно.
fn wall_mat(plaster: bool, seed: u32) -> Mat {
    let mut r = Rng::new(seed ^ 0x9a17);
    if !(plaster || r.chance(0.3)) {
        return Mat::Rubble;
    }
    match r.int(10) {
        0..=3 => Mat::Limewash,
        4..=6 => Mat::Plaster,
        7 | 8 => Mat::PlasterOchre,
        _ => Mat::PlasterRose,
    }
}

/// Что нужно знать отделке фасада о стене.
struct Finish<'a> {
    /// Проёмы фасадной стены (в осях дома).
    ops: &'a [Op],
    mat: Mat,
    /// Низ кровли над фасадом.
    eave: f32,
    /// Жилой вход: (u, ширина, высота).
    door: Option<(f32, f32, f32)>,
}

/// Отделка фасада: расписная панель по низу оштукатуренной стены, карниз из плинфы под кровлей,
/// козырёк или плита с крестом над входом, горшки с зеленью и бельё на жердях у верхних окон.
fn finish(b: &mut Builder, f: &Frame, windows: &[(f32, f32, f32, f32)], fin: &Finish, storeys: u8, rng: &mut Rng) {
    let dec = |b: &mut Builder, r: Rect, y0: f32, y1: f32, mat: Mat| b.decor([r.x0, y0, r.z0], [r.x1, y1, r.z1], mat);
    let w = f.w();
    let out = f.p(0.0, 0.0, -1.0) - f.p(0.0, 0.0, 0.0);
    let along = f.p(1.0, 0.0, 0.0) - f.p(0.0, 0.0, 0.0);
    // Панель: красная охра по штукатурке от цоколя до пояса, с тёмной полосой поверху.
    if fin.mat.plastered() && rng.chance(0.6) {
        let mut gaps: Vec<(f32, f32)> = fin.ops.iter().filter_map(|o| match *o {
            Op::Door { s, w, .. } | Op::Arch { s, w, .. } => Some((s - w / 2.0 - 0.18, s + w / 2.0 + 0.18)),
            _ => None,
        }).collect();
        gaps.sort_by(|a, c| a.0.total_cmp(&c.0));
        gaps.push((w - 0.32, w));
        let mut u = 0.32;
        for (g0, g1) in gaps {
            if g0 - u > 0.3 {
                dec(b, f.rect(u, g0, -0.012, 0.0), 0.42, 1.05, Mat::Ochre);
                dec(b, f.rect(u, g0, -0.016, 0.0), 1.05, 1.11, Mat::Soot);
            }
            u = u.max(g1);
        }
    }
    // Карниз: два ряда плинфы, верхний — с выносом, под самой кровлей.
    if storeys >= 2 || rng.chance(0.45) {
        dec(b, f.rect(0.02, w - 0.02, -0.05, 0.0), fin.eave - 0.3, fin.eave - 0.18, Mat::Brick);
        dec(b, f.rect(0.0, w, -0.11, 0.0), fin.eave - 0.18, fin.eave - 0.07, Mat::Brick);
        // Зубцы: плинфа через одну выдвинута.
        let n = (w / 0.3) as usize;
        for k in 0..n {
            let u = (k as f32 + 0.5) * w / n as f32;
            if k % 2 == 0 {
                dec(b, f.rect(u - 0.07, u + 0.07, -0.1, -0.05), fin.eave - 0.3, fin.eave - 0.18, Mat::Brick);
            }
        }
    }
    // Вход.
    if let Some((u, dw, dh)) = fin.door {
        match rng.int(10) {
            0..=3 => porch(b, f, u, dw, dh + 0.3),
            4..=6 => {
                // Плита над притолокой с резным крестом, подкрашенным охрой.
                dec(b, f.rect(u - dw / 2.0 - 0.3, u + dw / 2.0 + 0.3, -0.06, 0.0), dh + 0.22, dh + 0.62, Mat::Limestone);
                dec(b, f.rect(u - 0.025, u + 0.025, -0.075, -0.06), dh + 0.29, dh + 0.55, Mat::Ochre);
                dec(b, f.rect(u - 0.09, u + 0.09, -0.075, -0.06), dh + 0.43, dh + 0.48, Mat::Ochre);
            }
            _ => {}
        }
        // Порог-ступень из плиты.
        dec(b, f.rect(u - dw / 2.0 - 0.2, u + dw / 2.0 + 0.2, -0.38, 0.0), 0.0, 0.07, Mat::Limestone);
    }
    // Верхние окна: горшок с зеленью на подоконнике или бельё на жерди.
    for &(u, ww, sill, top) in windows {
        if sill < H1 {
            continue;
        }
        match rng.int(9) {
            0 | 1 => {
                let p = f.p(u + rng.range(-0.15, 0.15) * ww, sill, 0.1);
                b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.06, 0.0), (0.085, 0.11), (0.095, 0.13), (0.07, 0.13), (0.06, 0.1), (0.0, 0.1)], 9);
                kit::herb(b, [Mat::HerbGreen, Mat::HerbGreen, Mat::HerbSilver][rng.int(3)], p + Vec3::Y * 0.1, rng.range(0.9, 1.3), rng.range(0.0, 6.0));
            }
            2 => {
                // Жердь из окна, на ней сохнет бельё: рубахи, полотно, покрывало.
                let a = f.p(u, top - 0.25, 0.25);
                let c = f.p(u, top - 0.2, -1.5);
                b.mesh(Mat::Wood).beam(a, c, 0.018, 0.018, Vec3::Y);
                let mut v = 0.25;
                while v < 1.35 {
                    let len = rng.range(0.28, 0.45);
                    let drop = rng.range(0.45, 0.85);
                    let cloth = [Mat::Canvas, Mat::Canvas, Mat::Canvas, Mat::Canvas, Mat::Felt, Mat::Cloth][rng.int(6)];
                    let top0 = f.p(u, top - 0.21, -v);
                    let top1 = f.p(u, top - 0.21, -v - len);
                    let sag = along * rng.range(-0.012, 0.012);
                    let q = [top0, top1, top1 - Vec3::Y * drop + sag, top0 - Vec3::Y * (drop * rng.range(0.85, 1.0)) + sag];
                    let m = b.mesh(cloth);
                    m.quad(q, along);
                    m.quad(q, -along);
                    v += len + 0.08;
                }
            }
            _ => {}
        }
    }
    let _ = out;
}

/// Черепичный козырёк над входом на двух подкосах.
fn porch(b: &mut Builder, f: &Frame, u: f32, dw: f32, y: f32) {
    let (u0, u1) = (u - dw / 2.0 - 0.4, u + dw / 2.0 + 0.4);
    let depth = 0.95;
    let hi = [f.p(u0, y + 0.5, 0.0), f.p(u1, y + 0.5, 0.0)];
    let lo = [f.p(u0, y + 0.12, -depth), f.p(u1, y + 0.12, -depth)];
    b.roof(Mat::Tile, |m| m.lean_roof(hi, lo));
    let down = Vec3::Y * -0.07;
    let mut under = crate::level::geom::MeshBuilder::default();
    under.lean_roof([hi[0] + down, hi[1] + down], [lo[0] + down, lo[1] + down]);
    b.mesh(Mat::Wood).append(under.flipped());
    let t = b.mesh(Mat::Tile);
    let front = lo[0] - hi[0];
    let outn = Vec3::new(front.x, 0.0, front.z).normalize_or_zero();
    t.quad([lo[0], lo[1], lo[1] + down, lo[0] + down], outn);
    let ax = (hi[1] - hi[0]).normalize_or_zero();
    t.quad([hi[0], lo[0], lo[0] + down, hi[0] + down], -ax);
    t.quad([hi[1], lo[1], lo[1] + down, hi[1] + down], ax);
    // Подкосы от стены к переднему брусу и сам брус.
    let m = b.mesh(Mat::Wood);
    for uu in [u0 + 0.12, u1 - 0.12] {
        m.beam(f.p(uu, y - 0.55, -0.02), f.p(uu, y + 0.1, -depth + 0.12), 0.035, 0.045, Vec3::Y);
        m.beam(f.p(uu, y + 0.42, 0.0), f.p(uu, y + 0.07, -depth + 0.04), 0.035, 0.04, Vec3::Y);
    }
    m.beam(f.p(u0, y + 0.06, -depth + 0.1), f.p(u1, y + 0.06, -depth + 0.1), 0.04, 0.04, Vec3::Y);
}

/// Балкон над улицей: настил на выпущенных из стены брусьях с подкосами, перила из жердей.
/// По настилу ходят (дверь — из горницы), перила держат.
fn balcony(b: &mut Builder, f: &Frame, u: f32, y: f32, rng: &mut Rng) {
    let (hw, depth) = (1.25, 1.05);
    let fl = f.rect(u - hw, u + hw, -depth, 0.0);
    b.solid([fl.x0, y - 0.1, fl.z0], [fl.x1, y, fl.z1], Mat::Wood);
    let m = b.mesh(Mat::Wood);
    // Брусья под настилом и подкосы.
    for du in [-hw + 0.15, 0.0, hw - 0.15] {
        m.beam(f.p(u + du, y - 0.17, 0.05), f.p(u + du, y - 0.17, -depth - 0.08), 0.06, 0.07, Vec3::Y);
        if du != 0.0 {
            m.beam(f.p(u + du, y - 1.0, -0.02), f.p(u + du, y - 0.24, -depth + 0.15), 0.045, 0.05, Vec3::Y);
        }
    }
    // Перила: стойки, поручень, средняя жердь и частые балясины.
    let rail = y + 0.95;
    let corners = [(u - hw + 0.05, -0.08), (u - hw + 0.05, -depth + 0.05), (u + hw - 0.05, -depth + 0.05), (u + hw - 0.05, -0.08)];
    for &(pu, pv) in &corners[1..3] {
        m.beam(f.p(pu, y, pv), f.p(pu, rail + 0.06, pv), 0.04, 0.04, Vec3::X);
    }
    for k in 0..3 {
        let (a, c) = (corners[k], corners[k + 1]);
        for (yy, t) in [(rail, 0.035), (y + 0.5, 0.02)] {
            m.beam(f.p(a.0, yy, a.1), f.p(c.0, yy, c.1), t, t, Vec3::Y);
        }
        let len = ((a.0 - c.0).abs() + (a.1 - c.1).abs()).max(0.1);
        let n = (len / 0.22) as usize;
        for j in 1..n {
            let t = j as f32 / n as f32;
            let (pu, pv) = (a.0 + (c.0 - a.0) * t, a.1 + (c.1 - a.1) * t);
            m.beam(f.p(pu, y, pv), f.p(pu, rail, pv), 0.014, 0.014, Vec3::X);
        }
    }
    // Невидимые стенки перил.
    for r in [f.rect(u - hw, u - hw + 0.1, -depth, 0.0), f.rect(u + hw - 0.1, u + hw, -depth, 0.0), f.rect(u - hw, u + hw, -depth, -depth + 0.1)] {
        kit::hidden(b, Vec3::new(r.x0, y, r.z0), Vec3::new(r.x1, rail + 0.1, r.z1));
    }
    // На балконе: горшки с зеленью, иногда покрывало на перилах.
    for side in [-1.0f32, 1.0] {
        if rng.chance(0.6) {
            let p = f.p(u + side * (hw - 0.3), y, -depth + 0.3);
            b.mesh(Mat::Ceramic).lathe(p, &[(0.0, 0.0), (0.1, 0.0), (0.14, 0.2), (0.155, 0.23), (0.12, 0.23), (0.1, 0.18), (0.0, 0.18)], 10);
            kit::herb(b, Mat::HerbGreen, p + Vec3::Y * 0.18, rng.range(1.4, 2.0), rng.range(0.0, 6.0));
        }
    }
    if rng.chance(0.4) {
        let cu = u + rng.range(-0.5, 0.5);
        let cloth = [Mat::Rug, Mat::Cloth, Mat::Canvas][rng.int(3)];
        let (a, c) = (f.p(cu - 0.35, rail + 0.04, -depth + 0.05), f.p(cu + 0.35, rail + 0.04, -depth + 0.05));
        let outv = f.p(0.0, 0.0, -1.0) - f.p(0.0, 0.0, 0.0);
        let m = b.mesh(cloth);
        for (o, dy) in [(outv * 0.05, 0.6), (-outv * 0.05, 0.35)] {
            let q = [a + o, c + o, c + o - Vec3::Y * dy, a + o - Vec3::Y * dy];
            m.quad(q, o);
            m.quad(q, -o);
        }
        m.quad([a + outv * 0.05, c + outv * 0.05, c - outv * 0.05, a - outv * 0.05], Vec3::Y);
    }
}

/// Дымник: кладка в полкирпича над скатом и черепичный колпак на два ската.
fn smoke_hood(b: &mut Builder, f: &Frame, u: f32, v: f32, roof_y: f32, mat: Mat) {
    let r = f.rect(u - 0.28, u + 0.28, v - 0.28, v + 0.28);
    b.decor([r.x0, roof_y - 0.25, r.z0], [r.x1, roof_y + 0.42, r.z1], if mat.plastered() { mat } else { Mat::Brick });
    kit::gable_roof(b, Rect::new(r.x0 - 0.04, r.z0 - 0.04, r.x1 + 0.04, r.z1 + 0.04), roof_y + 0.42, 0.2, !f.front.along_x(), 0.1, Mat::Brick);
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
    b.mesh(Mat::Wicker).lathe(p, &[(0.0, 0.0), (r * 0.8, 0.0), (r, h * 0.85), (r * 1.04, h), (r * 0.94, h), (r * 0.86, h * 0.3), (0.0, h * 0.3)], 12);
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

/// Каменный прилавок лавки (вдоль фасада): от `u0 + COUNTER_IN` до `u1 - COUNTER_PASS`
/// (сбоку — проход за прилавок), по глубине — `COUNTER_V0..COUNTER_V1`, высота — `COUNTER_TOP`.
const COUNTER_IN: f32 = 0.2;
const COUNTER_PASS: f32 = 1.2;
const COUNTER_V0: f32 = OUTER_T + 0.9;
const COUNTER_V1: f32 = OUTER_T + 1.4;
const COUNTER_TOP: f32 = 1.0;
/// Безмен лежит у дальнего от прохода края прилавка: под товар — остальное.
const SCALE_ROOM: f32 = 0.6;
/// Продавец стоит за прилавком на таком расстоянии от его задней грани.
const SELLER_GAP: f32 = 0.65;

/// Лавку — в список лавок уровня: прилавок, место продавца, свободная под товар столешница
/// и ставни (двустворчатая дверь проёма на улицу).
fn register_shop(b: &mut Builder, f: &Frame, u0: f32, u1: f32, kind: Shop, front_doors: usize, back_v: f32) {
    let room = f.rect(u0, u1, OUTER_T, back_v);
    let counter = f.rect(u0 + COUNTER_IN, u1 - COUNTER_PASS, COUNTER_V0, COUNTER_V1);
    let (a, c) = (u0 + COUNTER_IN, u1 - COUNTER_PASS);
    let uc = (a + c) / 2.0;
    let facing = if f.front == Side::N { Vec3::NEG_Z } else { Vec3::Z };
    let door_at = f.p((u0 + u1) / 2.0, 0.0, OUTER_T / 2.0);
    let door = b.doors[front_doors..]
        .iter()
        .position(|d| d.double && d.width > 2.5 && (d.center - door_at).xz().length() < 0.2)
        .map(|k| (front_doors + k) as u32);
    let span = |u_from: f32, u_to: f32| -> Option<(Vec3, Vec3)> {
        if u_to - u_from < 0.45 {
            return None;
        }
        let r = f.rect(u_from, u_to, COUNTER_V0 + 0.03, COUNTER_V1 - 0.03);
        Some((r.min3(COUNTER_TOP), r.max3(COUNTER_TOP + 0.4)))
    };
    let free0 = a + SCALE_ROOM;
    let free1 = c - 0.05;
    let goods = if kind == Shop::Tavern {
        // Посередине стойки вмазаны долии: товар — по обе стороны.
        [span(free0, uc - 0.65), span(uc + 0.65, free1)]
    } else {
        [span(free0, free1), None]
    };
    let kind = match kind {
        Shop::None => crate::level::ShopKind::Common,
        Shop::Apothecary => crate::level::ShopKind::Apothecary,
        Shop::Bookseller => crate::level::ShopKind::Bookseller,
        Shop::Tavern => crate::level::ShopKind::Tavern,
    };
    b.shop(crate::level::ShopSpec {
        room_min: room.min3(0.0),
        room_max: room.max3(H1),
        counter_min: counter.min3(0.0),
        counter_max: counter.max3(COUNTER_TOP),
        facing,
        seller: f.p(uc, 0.0, COUNTER_V1 + SELLER_GAP),
        goods,
        door,
        kind,
    });
}

/// `back_v` — грань задней стены лавки (у неё полки с запасом).
fn shop_fittings(b: &mut Builder, f: &Frame, u0: f32, u1: f32, kind: Shop, front_doors: usize, back_v: f32, rng: &mut Rng) {
    let r = f.rect(u0, u1, OUTER_T, back_v).inset(0.1);
    // Прилавок у входа (каменный, с деревянной доской), проход сбоку.
    let counter = f.rect(u0 + COUNTER_IN, u1 - COUNTER_PASS, COUNTER_V0, COUNTER_V1);
    b.solid([counter.x0, 0.0, counter.z0], [counter.x1, COUNTER_TOP - 0.05, counter.z1], Mat::Rubble);
    b.mesh(Mat::Wood).cuboid(counter.min3(COUNTER_TOP - 0.05) - Vec3::new(0.05, 0.0, 0.05), counter.max3(COUNTER_TOP) + Vec3::new(0.05, 0.0, 0.05));
    register_shop(b, f, u0, u1, kind, front_doors, back_v);
    // В обычной лавке, книжной и корчме за прилавком стоит торговец (`shops.rs`): его место
    // обстановка не занимает. В аптеках — свои торговцы у своих столов (`trade.rs`).
    let seller = f.p((u0 + COUNTER_IN + u1 - COUNTER_PASS) / 2.0, 0.0, COUNTER_V1 + SELLER_GAP);
    if kind != Shop::Apothecary {
        b.keep_clear.push(seller);
    }
    // Полки у задней стены.
    let back = f.rect(u0 + 0.1, u1 - 0.1, back_v - 0.45, back_v - 0.1);
    let p = counter.center(1.0);
    // Мелочи, которые делают лавку обжитой: полка на боковой стене и связки под потолком.
    // Своё зерно — чтобы не сдвинуть выбор товара и обстановку.
    {
        let mut drng = Rng::new((r.x0 * 3.0 + r.z0 * 17.0) as i32 as u32 ^ 0xdec0);
        let n = (f.p(u0 + 1.0, 0.0, 1.0) - f.p(u0, 0.0, 1.0)).normalize();
        let (v0, v1) = (OUTER_T + 1.8, back_v - 0.6);
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
            // Штабель амфор в углу (кружки и кувшины на стойке — товар корчмаря).
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
            // Книжная лавка: полки со свитками, футляры-капсы на полу (кодексы, чернила
            // и пергамент на прилавке — товар книжника).
            kit::shelf(b, back, 3, rng, Mat::Canvas);
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
            // Безмен — у дальнего от прохода края прилавка, остальная столешница — под товар.
            let scale = f.p(u0 + COUNTER_IN + 0.3, COUNTER_TOP, (COUNTER_V0 + COUNTER_V1) / 2.0);
            interior::shop_extras(b, r, &[counter, back], scale, rng);
            // Обычная лавка: запас у задней стены (товар продавца — на прилавке).
            match rng.int(3) {
                0 => {
                    for k in 0..4 {
                        kit::amphora(b, back.min3(0.0) + Vec3::new(0.3 + k as f32 * 0.45, 0.0, 0.15), 1.0, rng.int(3) as u32);
                    }
                }
                1 => kit::shelf(b, back, 2, rng, Mat::Ceramic),
                _ => {
                    // Большие кувшины с запасом на полу у задней стены.
                    let c = back.center(0.0);
                    let along = if back.w() > back.d() { Vec3::X } else { Vec3::Z };
                    for k in 0..3 {
                        kit::jug(b, c + along * (-0.45 + k as f32 * 0.45), 0.42, Mat::Ceramic);
                    }
                }
            }
        }
    }
}

/// Двор в координатах усадьбы: u — от бокового крыла до стены двора, v — от фасадного крыла назад.
/// `back_door` — где в задней стене двора дверь (кладовая или калитка), `side_gate` — на какой
/// глубине ворота в стене двора (u = u1): перед ними ничего не ставится.
fn courtyard(b: &mut Builder, f: &Frame, (u0, u1): (f32, f32), (v0, v1): (f32, f32), back_door: Option<f32>, side_gate: Option<f32>, rng: &mut Rng) {
    if u1 - u0 < 4.0 || v1 - v0 < 4.0 {
        return;
    }
    // Цистерна под дождевую воду — посередине.
    kit::cistern_mouth(b, f.p((u0 + u1) / 2.0 + rng.range(-1.0, 1.0), 0.0, (v0 + v1) / 2.0 + rng.range(-1.0, 1.0)));
    // Пифосы, врытые у задней стены (не перед дверью кладовой).
    let n = 1 + rng.int(3);
    let mut u = u0 + 0.9;
    let mut placed = 0;
    while placed < n && u < u1 - 3.5 {
        if back_door.is_none_or(|door_u| (u - door_u).abs() > 1.6) {
            kit::pithos(b, f.p(u, 0.0, v1 - 0.8), 1.3, 0.6);
            placed += 1;
        }
        u += 1.2;
    }
    // Дальний задний угол: печь, давильня, мельница или дрова.
    let corner = f.p(u1 - 1.4, 0.0, v1 - 1.4);
    let corner_free = back_door.is_none_or(|door_u| door_u < u1 - 3.6) && side_gate.is_none_or(|gv| gv < v1 - 4.4);
    match if corner_free { rng.int(5) } else { rng.int(5) + 5 } {
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
        pergola(b, f, u0 + 1.2, (u0 + 7.0).min(u1 - 1.0), v.min(v1 - 2.9), rng);
    }
    if rng.chance(0.5) && side_gate.is_none_or(|gv| gv > v0 + 3.2) {
        for k in 0..3 {
            let p = f.p(u1 - 0.6, 0.17, v0 + 1.0 + k as f32 * 0.38);
            kit::amphora_lying(b, p, f.front.along_x(), 1.0);
        }
    }
    // Бельевая верёвка вдоль задней стены (своё зерно: остальное во дворе не сдвигается).
    let mut xr = Rng::new(((f.r.x0 * 31.0 + f.r.z0 * 17.0) as i32 as u32) ^ 0xc107);
    let mid = (u0 + u1) / 2.0;
    if xr.chance(0.5) && u1 - u0 > 8.0 && back_door.is_none() {
        clothesline(b, f.p(mid - 1.6, 0.0, v1 - 1.9), f.p(mid + 1.6, 0.0, v1 - 1.9), &mut xr);
    }
}

/// Виноградная беседка: два ряда столбов, жерди поверху, лоза по столбам и шапкой над головой.
fn pergola(b: &mut Builder, f: &Frame, u0: f32, u1: f32, v: f32, rng: &mut Rng) {
    if u1 - u0 < 2.0 {
        return;
    }
    let h = 2.35;
    let deep = 1.8;
    let n = ((u1 - u0) / 2.2).ceil().max(1.0) as usize;
    let m = b.mesh(Mat::Wood);
    for k in 0..=n {
        let u = u0 + (u1 - u0) * k as f32 / n as f32;
        for dv in [0.0, deep] {
            m.beam(f.p(u, 0.0, v + dv), f.p(u, h, v + dv), 0.045, 0.045, Vec3::X);
        }
        m.beam(f.p(u, h, v - 0.25), f.p(u, h, v + deep + 0.25), 0.035, 0.045, Vec3::Y);
    }
    for j in 0..5 {
        let dv = deep * j as f32 / 4.0;
        m.beam(f.p(u0 - 0.3, h + 0.07, v + dv), f.p(u1 + 0.3, h + 0.07, v + dv), 0.022, 0.022, Vec3::Y);
    }
    // Лоза: узловатый ствол вьётся по столбу.
    let mut seed = Rng::new(((u0 * 53.0 + v * 29.0 + f.r.x0 * 7.0) as i32 as u32) ^ 0x71e5);
    for k in 0..=n {
        if seed.chance(0.25) {
            continue;
        }
        let u = u0 + (u1 - u0) * k as f32 / n as f32;
        let dv = if seed.chance(0.5) { 0.0 } else { deep };
        let pts: Vec<Vec3> = (0..=6).map(|i| {
            let t = i as f32 / 6.0;
            let a = t * 5.0 + k as f32;
            f.p(u + a.cos() * 0.07, t * (h + 0.05), v + dv + a.sin() * 0.07)
        }).collect();
        b.mesh(Mat::Timber).beam_path(&pts, 0.025, 0.025, Vec3::X);
    }
    // Шапка листвы: низкие неровные подушки внахлёст, местами свисают плети.
    let leaves = b.mesh(Mat::Leaves);
    let count = ((u1 - u0) * 1.6) as usize + 2;
    for _ in 0..count {
        let (u, dv) = (seed.range(u0 - 0.1, u1 + 0.1), seed.range(-0.1, deep + 0.1));
        let r = seed.range(0.45, 0.8);
        let top = seed.range(0.16, 0.3);
        let p = f.p(u, h + 0.06, v + dv);
        leaves.lathe(p, &[(0.0, -0.1), (r * 0.7, -0.08), (r, 0.02), (r * 0.8, top * 0.7), (r * 0.4, top), (0.0, top * 1.05)], 7);
        if seed.chance(0.3) {
            let q = f.p(u + seed.range(-0.3, 0.3), h - 0.35, v + dv + seed.range(-0.3, 0.3));
            leaves.lathe(q, &[(0.0, -0.25), (0.12, -0.1), (0.16, 0.2), (0.0, 0.4)], 5);
        }
    }
    let _ = rng;
}

/// Бельевая верёвка на двух жердях: рубахи, полотна, покрывало.
fn clothesline(b: &mut Builder, a: Vec3, c: Vec3, rng: &mut Rng) {
    let h = 1.9;
    let m = b.mesh(Mat::Wood);
    for p in [a, c] {
        m.beam(p, p + Vec3::Y * (h + 0.1), 0.03, 0.03, Vec3::X);
    }
    let dir = (c - a).normalize_or_zero();
    let side = Vec3::new(dir.z, 0.0, -dir.x);
    let len = a.distance(c);
    // Верёвка провисает к середине.
    let rope = |t: f32| a + dir * (len * t) + Vec3::Y * (h - 0.12 * (1.0 - (2.0 * t - 1.0).powi(2)));
    let pts: Vec<Vec3> = (0..=8).map(|i| rope(i as f32 / 8.0)).collect();
    b.mesh(Mat::Canvas).beam_path(&pts, 0.008, 0.008, Vec3::Y);
    let mut t = 0.08;
    while t < 0.86 {
        let w = rng.range(0.1, 0.17);
        let drop = rng.range(0.5, 1.05);
        let cloth = [Mat::Canvas, Mat::Canvas, Mat::Canvas, Mat::Felt, Mat::Cloth, Mat::Rug][rng.int(6)];
        let (p0, p1) = (rope(t), rope(t + w));
        let q = [p0, p1, p1 - Vec3::Y * drop, p0 - Vec3::Y * (drop * rng.range(0.9, 1.0))];
        let m = b.mesh(cloth);
        m.quad(q, side);
        m.quad(q, -side);
        t += w + rng.range(0.02, 0.06);
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
        // Огород чаще всего, сад реже, виноградник — у немногих.
        let pick = |k: usize| match k {
            0..=5 => 0,
            6..=9 => 1,
            _ => 2,
        };
        let mut kind = pick(rng.int(12));
        // Соседние участки разные.
        if kind == last {
            kind = (kind + 1 + rng.int(2)) % 3;
        }
        last = kind;
        match kind {
            0 => kitchen_garden(b, inner, &inside, &mut rng),
            1 => orchard(b, inner, &inside, &mut rng),
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
        // Шаг посадки ~0,55 м; меш скромный — огородов много, а кадр должен держать 60 FPS.
        let n = ((s1 - s0) / 0.55) as usize;
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
                    kit::cabbage(b, Mat::HerbGreen, p, s, 1.0, p.x * 2.3 + p.z);
                }
            }
            // Лук и чеснок: пучки тонких перьев.
            2 => {
                for k in 0..n {
                    let p = at(k, rng) + Vec3::new(rng.range(-0.15, 0.15), 0.0, rng.range(-0.15, 0.15));
                    // Те же четыре случайных числа, что и раньше (две «палочки»): остальной огород на месте.
                    let (mut turn, mut tall) = (0.0, 0.0);
                    for q in 0..2 {
                        turn += q as f32 * 3.1 + rng.range(0.0, 1.0);
                        tall += rng.range(0.25, 0.4);
                    }
                    kit::leek(b, Mat::HerbGreen, p, tall / 0.66, turn);
                }
            }
            // Бобы на тычинах: колья шалашиком, плети кое-где добрались до верха.
            3 => {
                let m = (n / 2).max(2);
                for k in 0..m {
                    let p = at(k * 2, rng);
                    let top = p + Vec3::new(rng.range(-0.08, 0.08), rng.range(1.1, 1.4), rng.range(-0.08, 0.08));
                    for d in [-0.25f32, 0.25] {
                        let foot = p + if along_x { Vec3::new(0.0, 0.0, d) } else { Vec3::new(d, 0.0, 0.0) };
                        b.mesh(Mat::Wood).beam(foot, top, 0.015, 0.015, Vec3::Y);
                    }
                    // Плети у тычин: куст пониже, кое-где побеги уже поднялись выше.
                    if rng.chance(0.65) {
                        kit::shrub(b, p, rng.range(0.6, 0.95));
                    }
                }
            }
            // Зелень и травы: низкие кустики разного цвета.
            4 => {
                for k in 0..n {
                    let p = at(k, rng);
                    let mat = [Mat::HerbGreen, Mat::HerbSilver, Mat::HerbGreen][rng.int(3)];
                    kit::herb(b, mat, p, 1.0, p.x * 1.9 - p.z * 2.7);
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

/// Ряд лозы на шпалере: кривые колья через 2–2,5 м, жердь поверху, лоза — сплошной
/// неровной зелёной лентой (где гуще, где проредили), снизу — узловатые штамбы и грозди.
fn vine_hedge(b: &mut Builder, a: Vec3, c: Vec3, h: f32, rng: &mut Rng) {
    let len = a.distance(c);
    let dir = (c - a) / len;
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    // Колья и жердь.
    let mut t = 0.0;
    let mut posts = Vec::new();
    while t <= len + 0.01 {
        posts.push(a + dir * t.min(len) + side * rng.range(-0.05, 0.05));
        t += rng.range(2.0, 2.6);
    }
    let m = b.mesh(Mat::Wood);
    for p in &posts {
        m.beam(*p, *p + Vec3::Y * (h + rng.range(-0.06, 0.08)) + side * rng.range(-0.05, 0.05), 0.03, 0.03, dir);
    }
    m.beam(a + Vec3::Y * h, c + Vec3::Y * h, 0.012, 0.012, Vec3::Y);
    // Кусты лозы вдоль шпалеры — разной силы, где-то лоза выпала.
    let mut t = rng.range(0.3, 0.8);
    while t < len - 0.2 {
        if !rng.chance(0.1) {
            let p = a + dir * t + side * rng.range(-0.1, 0.1);
            kit::shrub(b, p, h * rng.range(0.95, 1.15));
        }
        t += rng.range(2.0, 2.6);
    }
    // Грозди под листвой.
    let n = (len / 1.6) as usize;
    for _ in 0..n {
        if rng.chance(0.5) {
            let p = a + dir * rng.range(0.3, len - 0.3) + side * (if rng.chance(0.5) { 0.22 } else { -0.22 }) + Vec3::Y * (h * 0.55);
            b.mesh(Mat::PetalPurple).lathe(p - Vec3::Y * 0.16, &[(0.0, 0.0), (0.05, 0.05), (0.07, 0.13), (0.0, 0.17)], 6);
        }
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
                b.mesh(Mat::Wicker).lathe(q, &[(0.18, 0.0), (0.24, 0.3), (0.26, 0.36), (0.0, 0.36)], 9);
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
                vine_hedge(b, a, c, rng.range(1.3, 1.6), rng);
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
    b.mesh(Mat::Wicker).lathe(front + Vec3::new(-0.7, 0.0, 0.2), &[(0.2, 0.0), (0.26, 0.32), (0.28, 0.38), (0.0, 0.38)], 9);
    let bench = if open_side == Side::S { Rect::new(h.x0 + 0.3, h.z1 - 0.7, h.x0 + 1.7, h.z1 - 0.35) } else { Rect::new(h.x1 - 0.7, h.z0 + 0.3, h.x1 - 0.35, h.z0 + 1.7) };
    kit::bench(b, bench);
}
