//! План жилой застройки Херсона.
//!
//! Античная сетка к VI веку уже не та, что при закладке города: главная улица, улицы от ворот
//! и улицы вдоль общественных зданий остались прямыми, а внутри старых кварталов усадьбы
//! делили между наследниками, продавали по частям, застраивали дворы и пробивали переулки.
//! Поэтому жилая ткань тут строится так:
//!
//! 1. **Зоны** — куски сетки, свободные от общественных построек (соседние кварталы сливаются,
//!    продольная улица между ними пропадает).
//! 2. Зона режется переулками разной ширины на **инсулы** неодинаковой величины: переулки
//!    упираются друг в друга (Т-образные перекрёстки), а не идут насквозь через весь город.
//! 3. Инсула делится на **участки**: два ряда спина к спине, по торцам — участки фасадом на
//!    поперечный переулок, так что дома смотрят на все четыре стороны. Участки разной ширины:
//!    усадьба с двором, рядом — три узких дома в ряд, пустырь с огородом, площадь с колодцем.
//!
//! Богатые дома (в два-три этажа, с лавками) — у главной улицы и агоры, бедные — у стен.

use super::houses::LotMode;
use super::kit::{Rect, Rng, Side};
use super::*;
use std::sync::OnceLock;

/// Стороны по кругу: каждая следующая — на четверть оборота по часовой (если смотреть сверху, север — −Z).
pub const SIDES: [Side; 4] = [Side::N, Side::E, Side::S, Side::W];

pub fn side_index(s: Side) -> usize {
    match s {
        Side::N => 0,
        Side::E => 1,
        Side::S => 2,
        Side::W => 3,
    }
}

/// Сторона, в которую смотрит `s` после поворота на `turn` четвертей (см. `level::quarter`).
pub fn rotated(s: Side, turn: u8) -> Side {
    SIDES[(side_index(s) + turn as usize) % 4]
}

pub fn opposite(s: Side) -> Side {
    rotated(s, 2)
}

/// Что стоит на участке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LotKind {
    /// Усадьба с двором: крылья вокруг двора с цистерной.
    Estate,
    /// Дом в ряду: узкий корпус в один-три этажа, за ним дворик.
    Row,
    /// Огород или сад за оградой.
    Garden,
    /// Площадь с колодцем.
    Square,
}

/// Участок.
#[derive(Clone, Copy, Debug)]
pub struct PLot {
    pub r: Rect,
    /// Куда смотрит фасад.
    pub face: Side,
    pub kind: LotKind,
    pub seed: u32,
    /// Улица у стороны участка (по порядку [`SIDES`]).
    pub open: [bool; 4],
    /// Насколько место богатое: 0 — у стены, 1 — у агоры на главной улице.
    pub rich: f32,
    pub mode: LotMode,
    /// Первый этаж — лавки.
    pub shops: bool,
    /// Этажей у фасадного корпуса.
    pub storeys: u8,
    /// Боковое крыло усадьбы — тоже в два этажа.
    pub side2: bool,
    /// Насколько стены выше обычного: у соседей карнизы на разной высоте.
    pub lift: f32,
    /// Достаток хозяев (0..2).
    pub wealth: u8,
}

pub struct Plan {
    pub zones: Vec<Rect>,
    pub insulae: Vec<Rect>,
    pub lots: Vec<PLot>,
}

/// Кварталы, которые строятся по-старому (две усадьбы на квартал): на их лавки и дворы
/// завязаны именованные места — книжник и аптека у агоры, корчма, аптека у порта, железный двор.
pub fn classic(i: i32, j: i32) -> bool {
    matches!((i, j), (16, -2) | (17, -2) | (7, 1) | (17, 1) | (6, 2))
}

/// Улицы от ворот — к западу от квартала с таким номером: они идут через весь город.
fn artery(i: i32) -> bool {
    i == 6 || i == 16
}

const WEST: f32 = -290.0;
const EAST: f32 = 267.0;
const NORTH: f32 = -154.0;
const SOUTH: f32 = 156.0;

fn zone_rects() -> Vec<Rect> {
    let free = |i: i32, j: i32| (0..NCOL).contains(&i) && matches!(block_use(i, j), Use::Houses | Use::Garden) && !classic(i, j);
    let span = |i0: i32, i1: i32, j0: i32, j1: i32| {
        let mut r = block(i0, j0).union(&block(i1, j1));
        if i0 == 0 {
            r.x0 = WEST;
        }
        if i1 == NCOL - 1 {
            r.x1 = EAST;
        }
        if j0 == -3 {
            r.z0 = NORTH;
        }
        if j1 == 3 {
            r.z1 = SOUTH;
        }
        r
    };
    let mut used = std::collections::HashSet::new();
    let mut out = Vec::new();
    // Два ряда кварталов сливаются, где оба свободны.
    for (ja, jb) in [(-3, -2), (2, 3)] {
        let mut i = 0;
        while i < NCOL {
            if free(i, ja) && free(i, jb) {
                let i0 = i;
                while free(i + 1, ja) && free(i + 1, jb) && !artery(i + 1) {
                    i += 1;
                }
                for k in i0..=i {
                    used.insert((k, ja));
                    used.insert((k, jb));
                }
                out.push(span(i0, i, ja, jb));
            }
            i += 1;
        }
    }
    for j in ROWS {
        let mut i = 0;
        while i < NCOL {
            if free(i, j) && !used.contains(&(i, j)) {
                let i0 = i;
                while free(i + 1, j) && !used.contains(&(i + 1, j)) && !artery(i + 1) {
                    i += 1;
                }
                out.push(span(i0, i, j, j));
            }
            i += 1;
        }
    }
    out
}

/// Режет зону переулками на инсулы.
fn carve(r: Rect, rng: &mut Rng, out: &mut Vec<Rect>) {
    let along_x = r.w() >= r.d();
    let (l, s) = if along_x { (r.w(), r.d()) } else { (r.d(), r.w()) };
    let street = [2.6, 3.0, 3.0, 3.4, 4.0][rng.int(5)];
    let cut = |r: Rect, across_x: bool, t: f32| {
        if across_x {
            let x = r.x0 + r.w() * t;
            (Rect::new(r.x0, r.z0, x - street / 2.0, r.z1), Rect::new(x + street / 2.0, r.z0, r.x1, r.z1))
        } else {
            let z = r.z0 + r.d() * t;
            (Rect::new(r.x0, r.z0, r.x1, z - street / 2.0), Rect::new(r.x0, z + street / 2.0, r.x1, r.z1))
        }
    };
    let (a, c) = if l > 72.0 || (l > 56.0 && s <= 50.0 && rng.chance(0.45)) {
        cut(r, along_x, rng.range(0.36, 0.64))
    } else if s > 50.0 {
        cut(r, !along_x, rng.range(0.4, 0.6))
    } else {
        out.push(r);
        return;
    };
    carve(a, rng, out);
    carve(c, rng, out);
}

/// Главная улица, продольные улицы у общественных зданий и улицы от ворот: тут лавки.
fn principal(r: &Rect, side: Side) -> u8 {
    let near = |a: f32, b: f32| (a - b).abs() < 0.05;
    match side {
        Side::N if near(r.z0, MAIN_HALF) => 2,
        Side::S if near(r.z1, -MAIN_HALF) => 2,
        Side::W if artery_x(r.x0) => 1,
        Side::E if artery_x(r.x1) => 1,
        _ => 0,
    }
}

fn artery_x(x: f32) -> bool {
    [6, 16].iter().any(|&i| (x - cross_x(i)).abs() < CROSS_W / 2.0 + 0.1)
}

/// Насколько место богатое.
fn rich_at(x: f32, z: f32) -> f32 {
    let agora = Vec2::new(170.0, -26.0);
    let a = 1.0 - terrain::smoothstep(30.0, 300.0, Vec2::new(x, z).distance(agora));
    let m = 1.0 - terrain::smoothstep(10.0, 130.0, z.abs());
    let e = terrain::smoothstep(8.0, 40.0, edge_distance(x, z));
    ((0.5 * a + 0.5 * m) * (0.45 + 0.55 * e)).clamp(0.0, 1.0)
}

/// Делит отрезок улицы на фасады: глубокие ряды — усадьбы вперемежку с группами узких домов,
/// мелкие — только узкие дома.
fn fronts(len: f32, deep: bool, rng: &mut Rng) -> Vec<(f32, f32)> {
    let mut ws: Vec<f32> = Vec::new();
    if !deep || len < 12.5 {
        let n = (len / rng.range(8.6, 10.4)).round().max(1.0) as usize;
        for _ in 0..n {
            ws.push(rng.range(0.88, 1.12));
        }
    } else {
        let mut sum = 0.0;
        while sum < len - 6.0 {
            let pick = rng.int(10);
            let group: Vec<f32> = match pick {
                0..=1 => (0..2 + rng.int(2)).map(|_| rng.range(8.0, 10.2)).collect(),
                2..=6 => vec![rng.range(15.0, 19.5)],
                _ => vec![rng.range(19.5, 25.0)],
            };
            for w in group {
                if sum < len - 6.0 {
                    ws.push(w);
                    sum += w;
                }
            }
        }
    }
    let total: f32 = ws.iter().sum();
    let mut out = Vec::new();
    let mut at = 0.0;
    for (k, w) in ws.iter().enumerate() {
        let next = if k + 1 == ws.len() { len } else { at + w / total * len };
        out.push((at, next));
        at = next;
    }
    out
}

struct Raw {
    r: Rect,
    face: Side,
    /// Номер в ряду (для уступов крыш).
    k: usize,
}

/// Участки инсулы.
fn lots_of(r: Rect, rng: &mut Rng, out: &mut Vec<Raw>) {
    let along_x = r.w() >= r.d();
    let (l, s) = if along_x { (r.w(), r.d()) } else { (r.d(), r.w()) };
    // (вдоль длинной стороны, поперёк) → участок.
    let rect = |a0: f32, a1: f32, c0: f32, c1: f32| if along_x { Rect::new(r.x0 + a0, r.z0 + c0, r.x0 + a1, r.z0 + c1) } else { Rect::new(r.x0 + c0, r.z0 + a0, r.x0 + c1, r.z0 + a1) };
    let (side_c0, side_c1, side_a0, side_a1) = if along_x { (Side::N, Side::S, Side::W, Side::E) } else { (Side::W, Side::E, Side::N, Side::S) };
    let (mut a0, mut a1) = (0.0, l);
    // Неглубокая инсула — один ряд усадеб во всю глубину (иногда — два ряда узких домов).
    let two_rows = s >= 25.0 || (s >= 21.0 && rng.chance(0.3));
    // Торцевые участки — фасадом на поперечную улицу (всегда, если там улица с лавками).
    for (end, side) in [(0, side_a0), (1, side_a1)] {
        let must = principal(&r, side) > 0;
        if (a1 - a0) >= 36.0 && s >= 15.0 && (must || rng.chance(0.6)) {
            let e = if must { rng.range(12.6, 15.0) } else { rng.range(10.6, 14.5) };
            let (e0, e1) = if end == 0 { (a0, a0 + e) } else { (a1 - e, a1) };
            for (k, (c0, c1)) in fronts(s, e >= 12.5, rng).into_iter().enumerate() {
                out.push(Raw { r: rect(e0, e1, c0, c1), face: side, k });
            }
            if end == 0 {
                a0 += e;
            } else {
                a1 -= e;
            }
        }
    }
    if two_rows {
        let split = s * rng.range(0.44, 0.56);
        for (c0, c1, face) in [(0.0, split, side_c0), (split, s, side_c1)] {
            for (k, (f0, f1)) in fronts(a1 - a0, c1 - c0 >= 12.5, rng).into_iter().enumerate() {
                out.push(Raw { r: rect(a0 + f0, a0 + f1, c0, c1), face, k });
            }
        }
    } else {
        // Один ряд: фасадом на улицу получше, сзади — глухая стена двора.
        let better = |side: Side| {
            let c = match side {
                Side::N => Vec2::new(r.cx(), r.z0),
                Side::S => Vec2::new(r.cx(), r.z1),
                Side::W => Vec2::new(r.x0, r.cz()),
                Side::E => Vec2::new(r.x1, r.cz()),
            };
            principal(&r, side) as f32 * 2.0 + rich_at(c.x, c.y)
        };
        let face = if better(side_c0) >= better(side_c1) { side_c0 } else { side_c1 };
        for (k, (f0, f1)) in fronts(a1 - a0, s >= 12.5, rng).into_iter().enumerate() {
            out.push(Raw { r: rect(a0 + f0, a0 + f1, 0.0, s), face, k });
        }
    }
}

fn build() -> Plan {
    let zones = zone_rects();
    let mut insulae = Vec::new();
    for (n, z) in zones.iter().enumerate() {
        let mut rng = Rng::new(0x51a7 ^ (n as u32 * 7919 + 13));
        carve(*z, &mut rng, &mut insulae);
    }
    let keep: Vec<Vec3> = super::super::places::places().iter().map(|p| p.pos).chain(super::super::places::npc_posts().iter().map(|p| p.pos)).collect();
    let inside = |x: f32, z: f32, m: f32| terrain::inside_city(x, z) && edge_distance(x, z) > m;
    let mut lots: Vec<PLot> = Vec::new();
    for (n, ins) in insulae.iter().enumerate() {
        let mut rng = Rng::new(0x1075 ^ (n as u32 * 104729 + 71));
        // Маленькая инсула иногда — площадь с колодцем.
        let area = ins.w() * ins.d();
        let whole = area < 520.0 || (area < 900.0 && rng.chance(0.14));
        let mut raw = Vec::new();
        if whole || ins.w().min(ins.d()) < 9.0 {
            raw.push(Raw { r: *ins, face: Side::S, k: 0 });
        } else {
            lots_of(*ins, &mut rng, &mut raw);
        }
        for q in raw {
            let r = q.r;
            let (cx, cz) = (r.cx(), r.cz());
            let corners = [(r.x0, r.z0), (r.x1, r.z0), (r.x0, r.z1), (r.x1, r.z1)];
            let fits = corners.iter().all(|&(x, z)| inside(x, z, 6.0));
            if !fits && !inside(cx, cz, 9.0) {
                continue;
            }
            let (w, d) = if q.face.along_x() { (r.w(), r.d()) } else { (r.d(), r.w()) };
            let rich = rich_at(cx, cz);
            let seed = 0x9e37 ^ ((lots.len() as u32 + 1) * 2654435 + n as u32 * 97);
            let mut lr = Rng::new(seed ^ 0xa5a5);
            let busy = keep.iter().any(|p| p.x > r.x0 - 1.5 && p.x < r.x1 + 1.5 && p.z > r.z0 - 1.5 && p.z < r.z1 + 1.5);
            let mut kind = if whole || busy {
                LotKind::Square
            } else if !fits {
                LotKind::Garden
            } else if w >= 12.5 && d >= 12.5 {
                LotKind::Estate
            } else if w >= 7.4 && d >= 7.0 {
                LotKind::Row
            } else {
                LotKind::Garden
            };
            // У стен часть участков — огороды и сады.
            if matches!(kind, LotKind::Estate | LotKind::Row) && lr.chance((0.4 - rich).max(0.0) * 0.8) {
                kind = LotKind::Garden;
            }
            let open = SIDES.map(|side| match side {
                Side::N => (r.z0 - ins.z0).abs() < 0.01,
                Side::S => (r.z1 - ins.z1).abs() < 0.01,
                Side::W => (r.x0 - ins.x0).abs() < 0.01,
                Side::E => (r.x1 - ins.x1).abs() < 0.01,
            });
            let street = principal(ins, q.face) * open[side_index(q.face)] as u8;
            let x = lr.f();
            let storeys = match kind {
                LotKind::Estate => {
                    if x < 0.05 + 0.22 * rich {
                        3
                    } else if x < 0.42 + 0.33 * rich {
                        2
                    } else {
                        1
                    }
                }
                _ => {
                    if x < 0.1 + 0.35 * rich {
                        3
                    } else if x < 0.72 + 0.25 * rich {
                        2
                    } else {
                        1
                    }
                }
            };
            // Лавки — на главных улицах через дом, на прочих редки: у каждой свой продавец.
            let shops = match street {
                2 => lr.chance(0.55),
                1 => lr.chance(0.25),
                _ => kind == LotKind::Row && lr.chance(0.05),
            };
            let wealth = if lr.chance(0.08 + 0.5 * rich + if street == 2 { 0.2 } else { 0.0 }) {
                2
            } else if lr.chance(0.4 * (1.0 - rich)) {
                0
            } else {
                1
            };
            let side2 = storeys >= 2 && lr.chance(0.4);
            let lift = [0.0, 0.32, 0.56, 0.16, 0.44][(q.k + n) % 5];
            lots.push(PLot { r, face: q.face, kind, seed, open, rich, mode: LotMode::Lived, shops, storeys, side2, lift, wealth });
        }
    }
    // Пепелища (их отстраивает город) и пустующие дома на продажу — ближайшие к заданным местам.
    fn pick(lots: &mut [PLot], at: (f32, f32), ok: &dyn Fn(&PLot) -> bool, mode: LotMode) {
        let p = Vec2::new(at.0, at.1);
        let best = lots
            .iter()
            .enumerate()
            .filter(|(_, l)| l.mode == LotMode::Lived && !l.shops && ok(l))
            .min_by(|a, b| p.distance_squared(Vec2::new(a.1.r.cx(), a.1.r.cz())).total_cmp(&p.distance_squared(Vec2::new(b.1.r.cx(), b.1.r.cz()))))
            .map(|(i, _)| i);
        if let Some(i) = best {
            lots[i].mode = mode;
        }
    }
    // Площади с колодцем — на углах, где сходятся переулки.
    let corner_ok = |l: &PLot| l.kind == LotKind::Estate && l.open.iter().filter(|o| **o).count() >= 2 && l.r.w().max(l.r.d()) < 21.0;
    for at in [(-160.0, -110.0), (-180.0, 30.0), (-60.0, -78.0), (20.0, 110.0), (60.0, -110.0), (110.0, 30.0), (-20.0, -28.0), (220.0, 128.0), (-230.0, -70.0), (130.0, -110.0)] {
        pick(&mut lots, at, &corner_ok, LotMode::Site);
    }
    for l in lots.iter_mut().filter(|l| l.mode == LotMode::Site) {
        l.mode = LotMode::Lived;
        l.kind = LotKind::Square;
    }
    let site_ok = |l: &PLot| l.kind == LotKind::Estate && l.face.along_x() && l.r.w() >= 16.5 && l.r.d() >= 17.0;
    for at in [(118.0, 30.0), (0.0, -78.0), (-95.0, 28.0), (130.0, -128.0)] {
        pick(&mut lots, at, &site_ok, LotMode::Site);
    }
    let estate_ok = |l: &PLot| l.kind == LotKind::Estate;
    let row_ok = |l: &PLot| l.kind == LotKind::Row && l.storeys >= 2 && l.r.w().min(l.r.d()) >= 7.5 && l.r.w().max(l.r.d()) >= 10.4;
    for at in [(196.0, -128.0), (170.0, 78.0), (30.0, -28.0), (-28.0, 26.0), (-160.0, -78.0), (78.0, 128.0), (-80.0, -26.0)] {
        pick(&mut lots, at, &estate_ok, LotMode::Vacant);
    }
    for at in [(120.0, -80.0), (-40.0, 80.0), (-200.0, 30.0), (60.0, -30.0), (210.0, 130.0)] {
        pick(&mut lots, at, &row_ok, LotMode::Vacant);
    }
    Plan { zones, insulae, lots }
}

pub fn plan() -> &'static Plan {
    static PLAN: OnceLock<Plan> = OnceLock::new();
    PLAN.get_or_init(build)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Участки не налезают друг на друга и на общественные кварталы, а домов хватает.
    #[test]
    fn lots_do_not_overlap_and_fill_the_city() {
        let p = plan();
        let over = |a: &Rect, b: &Rect| a.x0 < b.x1 - 0.01 && a.x1 > b.x0 + 0.01 && a.z0 < b.z1 - 0.01 && a.z1 > b.z0 + 0.01;
        for (i, a) in p.lots.iter().enumerate() {
            for b in &p.lots[i + 1..] {
                assert!(!over(&a.r, &b.r), "участки пересекаются: {:?} и {:?}", a.r, b.r);
            }
            for bi in 0..NCOL {
                for bj in ROWS {
                    if block_use(bi, bj) != Use::Houses && block_use(bi, bj) != Use::Garden || classic(bi, bj) {
                        assert!(!over(&a.r, &block(bi, bj)), "участок {:?} на квартале ({bi}, {bj})", a.r);
                    }
                }
            }
            assert!(!over(&a.r, &CITADEL.inset(-WALL_T)), "участок в цитадели: {:?}", a.r);
        }
        let count = |k: LotKind| p.lots.iter().filter(|l| l.kind == k).count();
        let (estates, rows) = (count(LotKind::Estate), count(LotKind::Row));
        eprintln!("зон {}, инсул {}, усадеб {estates}, домов в ряд {rows}, огородов {}, площадей {}", p.zones.len(), p.insulae.len(), count(LotKind::Garden), count(LotKind::Square));
        assert!(estates > 120 && rows > 80 && estates + rows < 400, "домов: усадеб {estates}, в ряд {rows}");
        let tall = p.lots.iter().filter(|l| l.kind != LotKind::Garden && l.storeys == 3).count();
        assert!(tall > 30, "мало трёхэтажных: {tall}");
        // Фасады смотрят на все четыре стороны.
        for s in SIDES {
            assert!(p.lots.iter().filter(|l| l.face == s && l.kind == LotKind::Estate).count() > 12, "мало усадеб фасадом на {s:?}");
        }
        assert_eq!(p.lots.iter().filter(|l| l.mode == LotMode::Site).count(), 4);
        assert!(p.lots.iter().filter(|l| l.mode == LotMode::Vacant).count() >= 10);
    }
}
