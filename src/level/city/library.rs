//! Городская библиотека Херсона (βιβλιοθήκη) — квартал к западу от дидаскалиона, у главной улицы.
//!
//! Как устроена (по образцу позднеантичных библиотек и книгохранилищ при епископиях):
//! - с улицы — портик на мраморных колоннах, над дверью — доска с надписью;
//! - вестибюль, келья библиофилакса (хранителя книг) и переплётная;
//! - двор-перистиль с садиком и колодцем: в портиках читают при дневном свете;
//! - скрипторий в западном крыле: переписчики за пюпитрами у окон во двор;
//! - в восточном крыле — пергаментная: рамы с растянутыми кожами, кадки, скребки;
//! - читальный зал-базилика: неф на мраморных колоннах с окнами второго света,
//!   в боковых нефах — армарии (шкафы) с кодексами, у северной стены — соты со свитками,
//!   посередине — длинные столы и пюпитры с раскрытыми книгами, в торце — мозаика с надписью.
//!
//! Где лежат раскрытые книги ([`lecterns`]) и кто служит в библиотеке ([`staff_spots`]), знают и
//! уровень (ставит пюпитры и людей), и клиент (накладывает страницы, открывает чтение и разговор).

use super::kit::{self, Op, Rect, Rng, Side};
use super::public::{column, polycandelon};
use super::interior;
use super::*;
use crate::collision::Aabb;
use crate::level::Role;
use crate::level::geom::MeshBuilder;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Квартал библиотеки.
pub const BLOCK: (i32, i32) = (14, -1);

/// Наружные грани стен.
pub fn rect() -> Rect {
    block(BLOCK.0, BLOCK.1).inset(0.5)
}

/// Внутри здания (с портиком) — для распорядка жителей и школы.
pub fn inside(p: Vec3) -> bool {
    let r = rect();
    p.x > r.x0 && p.x < r.x1 && p.z > r.z0 && p.z < r.z1 + 0.5 && p.y < 12.0
}

/// План: все линии здания (мировые координаты).
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub outer: Rect,
    /// Линия фасада (наружная грань стены с дверью); перед ней — портик до `outer.z1`.
    pub facade_z: f32,
    /// Передний корпус (вестибюль и комнаты), внутренние грани.
    pub front: Rect,
    /// Двор (по граням стен крыльев).
    pub court: Rect,
    /// Скрипторий (западное крыло) и пергаментная (восточное).
    pub scriptorium: Rect,
    pub parchment: Rect,
    /// Читальный зал (внутренние грани) и линии колоннад нефа.
    pub hall: Rect,
    pub nave_x: (f32, f32),
    /// Келья библиофилакса, вестибюль, переплётная.
    pub office: Rect,
    pub vestibule: Rect,
    pub bindery: Rect,
}

const OT: f32 = 0.7;
const IT: f32 = 0.5;
const FRONT_H: f32 = 5.0;
const WING_H: f32 = 4.8;
const AISLE_H: f32 = 6.4;
const NAVE_H: f32 = 10.0;
const COL_H: f32 = 5.4;

pub fn plan() -> Plan {
    let o = rect();
    let facade_z = o.z1 - 2.8;
    let ix0 = o.x0 + OT;
    let ix1 = o.x1 - OT;
    let front = Rect::new(ix0, facade_z - 6.2, ix1, facade_z - OT);
    let hall_s = o.z0 + 22.4;
    let hall = Rect::new(ix0, o.z0 + OT, ix1, hall_s - OT);
    let nave_x = (o.cx() - 5.5, o.cx() + 5.5);
    let court = Rect::new(nave_x.0 + IT / 2.0, hall_s, nave_x.1 - IT / 2.0, front.z0 - IT);
    let scriptorium = Rect::new(ix0, court.z0, nave_x.0 - IT / 2.0, court.z1);
    let parchment = Rect::new(nave_x.1 + IT / 2.0, court.z0, ix1, court.z1);
    let office = Rect::new(ix0, front.z0, nave_x.0 - IT / 2.0, front.z1);
    let vestibule = Rect::new(nave_x.0 + IT / 2.0, front.z0, nave_x.1 - IT / 2.0, front.z1);
    let bindery = Rect::new(nave_x.1 + IT / 2.0, front.z0, ix1, front.z1);
    Plan { outer: o, facade_z, front, court, scriptorium, parchment, hall, nave_x, office, vestibule, bindery }
}

// ---------------------------------------------------------------------------
// Раскрытые книги и люди: общие для уровня и клиента
// ---------------------------------------------------------------------------

/// Раскрытая книга на пюпитре или на столе: середина разворота, куда смотрит читающий
/// (от книги к нему), наклон доски и номер рукописи (см. `library_texts`).
#[derive(Clone, Copy, Debug)]
pub struct Lectern {
    /// Середина разворота (верх страниц).
    pub at: Vec3,
    /// От книги к читателю (горизонтально).
    pub toward: Vec3,
    /// Наклон доски от горизонтали (рад).
    pub tilt: f32,
    /// Ширина разворота (м).
    pub width: f32,
    pub text: usize,
    /// Свиток, раскатанный на столе (а не кодекс).
    pub scroll: bool,
}

impl Lectern {
    /// Высота разворота (страницы — 2:3, разворот — 3:2, как клетка текстуры).
    pub fn page_height(&self) -> f32 {
        self.width / 1.5
    }

    /// Оси разворота: вдоль строк (слева направо для читающего), вверх по странице, нормаль.
    pub fn axes(&self) -> (Vec3, Vec3, Vec3) {
        let t = self.toward.normalize();
        // Правая рука читающего, который смотрит на книгу (против `toward`).
        let right = Vec3::Y.cross(t).normalize();
        // Страница поднята дальним от читателя краем.
        let up = (-t * self.tilt.cos() + Vec3::Y * self.tilt.sin()).normalize();
        (right, up, right.cross(up).normalize())
    }
}

/// Все раскрытые книги библиотеки (порядок постоянный: по нему клиент кладёт страницы).
pub fn lecterns() -> Vec<Lectern> {
    let p = plan();
    let h = p.hall;
    let cx = h.cx();
    let mut v = Vec::new();
    // Большой аналой посреди нефа — Четвероевангелие на пурпуре.
    v.push(Lectern { at: on_board(Vec3::new(cx, 1.32 - 0.04, h.z0 + 5.6), Vec3::Z, 0.55, 0.04), toward: Vec3::Z, tilt: 0.55, width: 0.62, text: 0, scroll: false });
    // Кодексы на столах нефа: по два на стол, читающие сидят лицом к оси нефа.
    let tables = table_rows(&p);
    let mut k = 1;
    for t in &tables {
        for (dz, side) in [(-0.9, -1.0), (0.9, 1.0)] {
            // Кодекс раскрыт прямо на столе, лицом к сидящему на скамье.
            let toward = Vec3::new(side, 0.0, 0.0);
            v.push(Lectern { at: Vec3::new(t.cx() + side * 0.2, 0.86 + 0.026, t.cz() + dz), toward, tilt: 0.0, width: 0.46, text: k, scroll: false });
            k += 1;
        }
    }
    // Свитки, раскатанные на столах в портиках двора.
    for (i, x) in [p.court.x0 + 0.9, p.court.x1 - 0.9].into_iter().enumerate() {
        let toward = if i == 0 { Vec3::X } else { Vec3::NEG_X };
        v.push(Lectern { at: Vec3::new(x, 0.765, p.court.cz() + 1.6), toward, tilt: 0.0, width: 0.7, text: k, scroll: true });
        k += 1;
    }
    // В скриптории — образец (то, с чего переписывают), на подставке у окна.
    let board = Vec3::new(p.scriptorium.x0 + 1.25 - 0.18, 1.05 - 0.04, p.scriptorium.z0 + 2.6);
    v.push(Lectern { at: on_board(board, Vec3::X, 0.6, 0.04), toward: Vec3::X, tilt: 0.6, width: 0.4, text: k, scroll: false });
    k += 1;
    // На столе библиофилакса — опись книг (пинакс).
    v.push(Lectern { at: Vec3::new(p.office.cx() + 0.2, 0.83, p.office.z0 + 1.35), toward: Vec3::Z, tilt: 0.0, width: 0.42, text: k, scroll: false });
    v
}

/// Точка над серединой наклонной доски: книга лежит на ней, а не тонет в дереве
/// (у граней уровня ещё и сдвиг по нормали — `graphics::depth_rank`).
fn on_board(board: Vec3, toward: Vec3, tilt: f32, lift: f32) -> Vec3 {
    let l = Lectern { at: board, toward, tilt, width: 0.0, text: 0, scroll: false };
    board + l.axes().2 * lift
}

/// Длинные столы нефа (по оси нефа, друг за другом).
fn table_rows(p: &Plan) -> Vec<Rect> {
    let h = p.hall;
    let cx = h.cx();
    (0..2).map(|i| {
        let z = h.z0 + 9.6 + i as f32 * 5.4;
        Rect::new(cx - 0.45, z - 1.7, cx + 0.45, z + 1.7)
    }).collect()
}

/// Кто служит в библиотеке.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Staff {
    /// Хранитель книг — диакон Евагрий.
    Librarian,
    /// Каллиграф-переписчик Феодор.
    Calligrapher,
    /// Мальчишка-пергаментщик Захарий.
    Parchmenter,
    /// Старый ритор Аммоний — читатель, ходит каждый день.
    Rhetor,
    /// Переплётчица Мария.
    Binder,
}

impl Staff {
    #[cfg(test)]
    pub const ALL: [Staff; 5] = [Staff::Librarian, Staff::Calligrapher, Staff::Parchmenter, Staff::Rhetor, Staff::Binder];

    pub fn role(self) -> Role {
        match self {
            Staff::Librarian | Staff::Rhetor => Role::Scholar,
            Staff::Calligrapher | Staff::Parchmenter => Role::Student,
            Staff::Binder => Role::Craftsman,
        }
    }
}

/// Где стоят (сидят) служащие и куда смотрят.
pub fn staff_spots() -> Vec<(Staff, Vec3, Vec3)> {
    let p = plan();
    let h = p.hall;
    let s = p.scriptorium;
    let pa = p.parchment;
    let rows = table_rows(&p);
    vec![
        // Библиофилакс — у своего стола при входе в зал, лицом к входящим.
        (Staff::Librarian, Vec3::new(h.cx() + 2.6, 0.0, h.z1 - 2.2), Vec3::new(h.cx(), 0.0, h.z1)),
        (Staff::Calligrapher, Vec3::new(s.x0 + 1.9, 0.0, s.z0 + 5.6), Vec3::new(s.x0 + 1.0, 0.0, s.z0 + 5.6)),
        (Staff::Parchmenter, Vec3::new(pa.cx(), 0.0, pa.z0 + 4.2), Vec3::new(pa.cx() + 1.0, 0.0, pa.z0 + 4.2)),
        (Staff::Rhetor, Vec3::new(rows[1].x1 + 0.75, 0.0, rows[1].cz() + 0.9), Vec3::new(rows[1].cx(), 0.0, rows[1].cz() + 0.9)),
        (Staff::Binder, Vec3::new(p.bindery.cx() - 0.4, 0.0, p.bindery.z0 + 1.6), Vec3::new(p.bindery.cx() - 0.4, 0.0, p.bindery.z0)),
    ]
}

/// Номер жителя каждого служащего в раскладке уровня (по месту, где он поставлен).
pub fn staff_of(residents: &[crate::level::Resident]) -> Vec<(usize, Staff)> {
    staff_spots()
        .into_iter()
        .filter_map(|(s, at, _)| residents.iter().position(|r| r.pos.distance(at) < 0.05).map(|i| (i, s)))
        .collect()
}

// ---------------------------------------------------------------------------
// Здание
// ---------------------------------------------------------------------------

pub(super) fn library(b: &mut Builder) {
    let p = plan();
    let o = p.outer;
    let mut rng = Rng::new(0xB1B1);
    b.at(o.cx(), o.cz());
    shell(b, &p);
    portico(b, &p);
    b.at(o.cx(), p.hall.cz());
    reading_hall(b, &p, &mut rng);
    b.at(o.cx(), p.court.cz());
    court(b, &p, &mut rng);
    scriptorium(b, &p, &mut rng);
    parchment_room(b, &p, &mut rng);
    front_rooms(b, &p, &mut rng);
    for l in lecterns() {
        open_book(b, &l);
    }
    for (s, at, look) in staff_spots() {
        let d = look - at;
        b.resident(at, (-d.x).atan2(-d.z), s.role());
    }
}

/// Стены, крыши, полы, потолки.
fn shell(b: &mut Builder, p: &Plan) {
    let o = p.outer;
    let h = p.hall;
    let (nx0, nx1) = p.nave_x;
    let hall_s = h.z1 + OT; // наружная грань южной стены зала (во двор)
    let wall = Mat::OpusMixtum;
    let cx = o.cx();

    // ---- передний корпус ----
    let fz = p.facade_z;
    let front_out = Rect::new(o.x0, p.front.z0 - IT, o.x1, fz);
    kit::wall_x(b, o.x0, o.x1, fz - OT / 2.0, OT, FRONT_H, &[
        Op::Door { s: cx, w: 2.4, h: 3.4 },
        Op::Window { s: (o.x0 + nx0) / 2.0, w: 1.0, sill: 1.6, top: 2.9 },
        Op::Window { s: (nx1 + o.x1) / 2.0, w: 1.0, sill: 1.6, top: 2.9 },
    ], wall);
    // Боковые стены переднего корпуса и крыльев — одной линией до зала.
    kit::wall_z(b, hall_s, fz - OT, o.x0 + OT / 2.0, OT, FRONT_H, &[
        Op::Window { s: p.scriptorium.z0 + 2.6, w: 0.9, sill: 1.7, top: 2.9 },
        Op::Window { s: p.scriptorium.z1 - 2.6, w: 0.9, sill: 1.7, top: 2.9 },
    ], wall);
    kit::wall_z(b, hall_s, fz - OT, o.x1 - OT / 2.0, OT, FRONT_H, &[
        Op::Window { s: p.parchment.z0 + 2.6, w: 0.9, sill: 1.7, top: 2.9 },
    ], wall);
    // Стена переднего корпуса во двор и перегородки.
    kit::wall_x(b, o.x0 + OT, o.x1 - OT, p.front.z0 - IT / 2.0, IT, FRONT_H, &[
        Op::Arch { s: cx, w: 2.4, spring: 2.4 },
        Op::Door { s: (p.office.x0 + p.office.x1) / 2.0, w: 1.2, h: 2.4 },
        Op::Door { s: (p.bindery.x0 + p.bindery.x1) / 2.0, w: 1.2, h: 2.4 },
    ], Mat::Plaster);
    for x in [nx0, nx1] {
        kit::wall_z(b, p.front.z0, p.front.z1, x, IT, FRONT_H, &[Op::Door { s: p.front.cz() + 1.2, w: 1.1, h: 2.4 }], Mat::Plaster);
    }
    kit::quoins(b, o.x0 + OT / 2.0, fz - OT / 2.0, FRONT_H, OT);
    kit::quoins(b, o.x1 - OT / 2.0, fz - OT / 2.0, FRONT_H, OT);
    kit::gable_roof(b, front_out, FRONT_H, 1.7, true, 0.45, wall);
    // Потолок вестибюля — дощатый на балках.
    ceiling(b, p.front, FRONT_H - 0.3, true);

    // ---- крылья (скрипторий и пергаментная) ----
    for (r, side, x) in [(p.scriptorium, Side::W, nx0), (p.parchment, Side::E, nx1)] {
        let ops = [
            Op::Door { s: r.cz() + 3.0, w: 1.3, h: 2.6 },
            Op::Window { s: r.cz() - 2.8, w: 1.3, sill: 1.0, top: 2.6 },
            Op::Window { s: r.cz() + 0.2, w: 1.3, sill: 1.0, top: 2.6 },
            Op::Window { s: r.z0 + 1.6, w: 1.0, sill: 1.0, top: 2.6 },
        ];
        kit::wall_z(b, r.z0, r.z1, x, IT, WING_H, &ops, Mat::Plaster);
        let roof = if side == Side::W { Rect::new(o.x0, r.z0, x + IT / 2.0, r.z1) } else { Rect::new(x - IT / 2.0, r.z0, o.x1, r.z1) };
        kit::lean_roof(b, roof, side, WING_H + 1.3, WING_H + 0.05, 0.35);
        kit::lean_fill(b, roof, side, WING_H + 1.3, WING_H + 0.05, OT, WING_H, &[], wall);
        ceiling(b, r, WING_H - 0.2, false);
    }

    // ---- читальный зал ----
    // Южная стена (во двор): дверь в неф и по двери в боковые нефы, высокое окно над входом.
    let sz = h.z1 + OT / 2.0;
    kit::wall(b, kit::fx(), o.x0, o.x1, sz - OT / 2.0, sz + OT / 2.0, 0.0, AISLE_H, &[
        Op::Door { s: cx, w: 2.2, h: 3.4 },
        Op::Window { s: (o.x0 + nx0) / 2.0, w: 1.1, sill: 3.4, top: 5.2 },
        Op::Window { s: (nx1 + o.x1) / 2.0, w: 1.1, sill: 3.4, top: 5.2 },
    ], wall, Mat::Limestone);
    kit::wall(b, kit::fx(), nx0 - 0.35, nx1 + 0.35, sz - OT / 2.0, sz + OT / 2.0, AISLE_H, NAVE_H, &[Op::Window { s: cx, w: 1.8, sill: 0.6, top: 2.8 }], wall, Mat::Limestone);
    // Северная стена: глухая, с нишей для мозаики в нефе.
    let nz = h.z0 - OT / 2.0;
    kit::wall(b, kit::fx(), o.x0, o.x1, nz - OT / 2.0, nz + OT / 2.0, 0.0, AISLE_H, &[], wall, Mat::Limestone);
    kit::wall(b, kit::fx(), nx0 - 0.35, nx1 + 0.35, nz - OT / 2.0, nz + OT / 2.0, AISLE_H, NAVE_H, &[Op::Window { s: cx, w: 1.4, sill: 1.0, top: 2.9 }], wall, Mat::Limestone);
    // Наружные стены боковых нефов: окна выше шкафов.
    for x in [o.x0 + OT / 2.0, o.x1 - OT / 2.0] {
        let wins: Vec<Op> = (0..4).map(|k| Op::Window { s: h.z0 + 3.2 + k as f32 * 5.0, w: 1.0, sill: 3.5, top: 5.4 }).collect();
        kit::wall(b, kit::fz(), h.z0 - OT, h.z1 + OT, x - OT / 2.0, x + OT / 2.0, 0.0, AISLE_H, &wins, wall, Mat::Limestone);
    }
    for (x, z) in [(o.x0, o.z0), (o.x1, o.z0)] {
        let sx = if x > cx { -1.0 } else { 1.0 };
        kit::quoins(b, x + sx * OT / 2.0, z + OT / 2.0, AISLE_H, OT);
    }
    // Колоннады нефа и стены второго света над ними.
    let ncol = 5;
    let step = (h.d() - 2.0) / ncol as f32;
    for x in [nx0, nx1] {
        let mut arches = Vec::new();
        let mut wins = Vec::new();
        for k in 0..ncol {
            let z = h.z0 + 1.0 + step * (k as f32 + 0.5);
            arches.push(Op::Arch { s: z, w: step - 0.75, spring: 0.0 });
            wins.push(Op::Window { s: z, w: 1.1, sill: 0.6, top: 2.6 });
        }
        kit::wall(b, kit::fz(), h.z0, h.z1, x - 0.35, x + 0.35, COL_H, COL_H + 2.2, &arches, wall, Mat::Marble);
        kit::wall(b, kit::fz(), h.z0, h.z1, x - 0.35, x + 0.35, COL_H + 2.2, NAVE_H, &wins, wall, Mat::Limestone);
        for k in 0..=ncol {
            let z = h.z0 + 1.0 + step * k as f32;
            column(b, Vec3::new(x, 0.0, z), COL_H, 0.26, Mat::Marble);
        }
    }
    // Крыши: неф — двускатная (конёк вдоль зала), боковые нефы — односкатные.
    kit::gable_roof(b, Rect::new(nx0 - 0.35, o.z0, nx1 + 0.35, hall_s), NAVE_H, 2.4, false, 0.5, wall);
    kit::lean_roof(b, Rect::new(o.x0, o.z0, nx0 - 0.35, hall_s), Side::E, COL_H + 2.0, AISLE_H, 0.5);
    kit::lean_roof(b, Rect::new(nx1 + 0.35, o.z0, o.x1, hall_s), Side::W, COL_H + 2.0, AISLE_H, 0.5);
    // Кессонный потолок нефа и дощатые — боковых.
    let m = b.mesh(Mat::Wood);
    m.cuboid(Vec3::new(nx0 + 0.35, NAVE_H - 0.12, h.z0), Vec3::new(nx1 - 0.35, NAVE_H, h.z1));
    let beams = 10;
    for k in 0..=beams {
        let z = h.z0 + h.d() * k as f32 / beams as f32;
        m.cuboid(Vec3::new(nx0 + 0.35, NAVE_H - 0.42, z - 0.13), Vec3::new(nx1 - 0.35, NAVE_H - 0.12, z + 0.13));
    }
    for x in [nx0 + 2.0, (nx0 + nx1) / 2.0, nx1 - 2.0] {
        m.cuboid(Vec3::new(x - 0.1, NAVE_H - 0.34, h.z0), Vec3::new(x + 0.1, NAVE_H - 0.12, h.z1));
    }
    // Расписной фриз под потолком нефа: красная полоса с золотой каймой.
    for x in [nx0 + 0.35, nx1 - 0.35] {
        let d = if x < cx { 0.03 } else { -0.03 };
        b.decor([x.min(x + d), NAVE_H - 0.95, h.z0], [x.max(x + d), NAVE_H - 0.6, h.z1], Mat::Ochre);
        b.decor([x.min(x + d * 1.3), NAVE_H - 0.62, h.z0], [x.max(x + d * 1.3), NAVE_H - 0.55, h.z1], Mat::Gold);
    }
    for (x0, x1) in [(h.x0, nx0 - 0.35), (nx1 + 0.35, h.x1)] {
        ceiling(b, Rect::new(x0, h.z0, x1, h.z1), AISLE_H - 0.15, false);
    }
    // Полы: мозаика в нефе, плиты — в боковых нефах и в комнатах.
    b.decor([nx0 + 0.35, 0.0, h.z0], [nx1 - 0.35, 0.03, h.z1], Mat::Mosaic);
    b.decor([h.x0, 0.0, h.z0], [nx0 + 0.35, 0.025, h.z1], Mat::Floor);
    b.decor([nx1 - 0.35, 0.0, h.z0], [h.x1, 0.025, h.z1], Mat::Floor);
    for r in [p.scriptorium, p.parchment, p.office, p.vestibule, p.bindery] {
        b.decor([r.x0, 0.0, r.z0], [r.x1, 0.025, r.z1], Mat::Floor);
    }
    // Цоколь зала: красная цемянка, полоса росписи.
    for (lo, hi) in [([h.x0, h.z0], [h.x0 + 0.03, h.z1]), ([h.x1 - 0.03, h.z0], [h.x1, h.z1])] {
        b.decor([lo[0], 0.05, lo[1]], [hi[0], 0.6, hi[1]], Mat::Signinum);
    }
    // Внутри стены оштукатурены: известковая штукатурка поверх кладки,
    // охристые пояса над цоколем и под окнами. Проёмы остаются открытыми.
    let aisle_wins: Vec<(f32, f32, f32, f32)> = (0..4).map(|k| {
        let z = h.z0 + 3.2 + k as f32 * 5.0;
        (z - 0.5, z + 0.5, 3.5, 5.4)
    }).collect();
    for (x, inx) in [(h.x0, 1.0f32), (h.x1, -1.0)] {
        plaster(b, Face::X(x, inx), h.z0, h.z1, 0.6, AISLE_H - 0.15, &aisle_wins, Mat::Plaster);
        plaster(b, Face::X(x, inx * 1.6), h.z0, h.z1, 0.6, 0.72, &[], Mat::Ochre);
        plaster(b, Face::X(x, inx * 1.6), h.z0, h.z1, 3.28, 3.4, &[], Mat::Ochre);
    }
    // Северная стена: штукатурка от пола до кровли бокового нефа, в нефе — до потолка.
    let nwin = [(cx - 0.7, cx + 0.7, AISLE_H + 1.0, AISLE_H + 2.9)];
    plaster(b, Face::Z(h.z0, 1.0), h.x0, h.x1, 0.0, AISLE_H - 0.15, &[], Mat::Plaster);
    plaster(b, Face::Z(h.z0, 1.0), nx0 + 0.35, nx1 - 0.35, AISLE_H - 0.15, NAVE_H - 0.95, &nwin, Mat::Plaster);
    // Южная стена изнутри: двери и окна.
    let swin = [
        (cx - 1.1, cx + 1.1, 0.0, 3.4),
        ((h.x0 + nx0) / 2.0 - 0.55, (h.x0 + nx0) / 2.0 + 0.55, 3.4, 5.2),
        ((nx1 + h.x1) / 2.0 - 0.55, (nx1 + h.x1) / 2.0 + 0.55, 3.4, 5.2),
    ];
    plaster(b, Face::Z(h.z1, -1.0), h.x0, h.x1, 0.0, AISLE_H - 0.15, &swin, Mat::Plaster);
    plaster(b, Face::Z(h.z1, -1.0), nx0 + 0.35, nx1 - 0.35, AISLE_H - 0.15, NAVE_H - 0.95, &[(cx - 0.9, cx + 0.9, AISLE_H + 0.6, AISLE_H + 2.8)], Mat::Plaster);
    // Стены второго света над колоннадами — с обеих сторон, между окнами.
    let ncol = 5;
    let step = (h.d() - 2.0) / ncol as f32;
    let clere: Vec<(f32, f32, f32, f32)> = (0..ncol).map(|k| {
        let z = h.z0 + 1.0 + step * (k as f32 + 0.5);
        (z - 0.55, z + 0.55, COL_H + 2.8, COL_H + 4.8)
    }).collect();
    for x in [nx0, nx1] {
        let toward_nave = if x < cx { 1.0 } else { -1.0 };
        plaster(b, Face::X(x + toward_nave * 0.35, toward_nave), h.z0, h.z1, COL_H + 2.2, NAVE_H - 0.95, &clere, Mat::Plaster);
    }
}

/// Грань стены, на которую кладётся штукатурка: плоскость и куда смотрит (в комнату).
#[derive(Clone, Copy)]
enum Face {
    /// x = const, нормаль ±X (знак — направление и множитель толщины).
    X(f32, f32),
    /// z = const, нормаль ±Z.
    Z(f32, f32),
}

/// Тонкий слой отделки на грани стены от `s0` до `s1` (вдоль стены) и от `y0` до `y1`,
/// с вырезами под проёмы `holes` (s0, s1, y0, y1).
fn plaster(b: &mut Builder, face: Face, s0: f32, s1: f32, y0: f32, y1: f32, holes: &[(f32, f32, f32, f32)], mat: Mat) {
    let mut ys = vec![y0, y1];
    for &(_, _, hy0, hy1) in holes {
        for y in [hy0, hy1] {
            if y > y0 && y < y1 {
                ys.push(y);
            }
        }
    }
    ys.sort_by(|a, b| a.total_cmp(b));
    ys.dedup();
    for w in ys.windows(2) {
        let (ya, yb) = (w[0], w[1]);
        if yb - ya < 0.01 {
            continue;
        }
        let mid = (ya + yb) / 2.0;
        let mut cuts: Vec<(f32, f32)> = holes.iter().filter(|h| h.2 <= mid && h.3 >= mid).map(|h| (h.0, h.1)).collect();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut s = s0;
        for (c0, c1) in cuts.into_iter().chain(std::iter::once((s1, s1))) {
            let e = c0.min(s1);
            if e - s > 0.01 {
                piece(b, face, s, e, ya, yb, mat);
            }
            s = s.max(c1);
        }
    }
}

fn piece(b: &mut Builder, face: Face, s0: f32, s1: f32, y0: f32, y1: f32, mat: Mat) {
    match face {
        Face::X(x, n) => {
            let t = 0.02 * n;
            b.decor([x.min(x + t), y0, s0], [x.max(x + t), y1, s1], mat);
        }
        Face::Z(z, n) => {
            let t = 0.02 * n;
            b.decor([s0, y0, z.min(z + t)], [s1, y1, z.max(z + t)], mat);
        }
    }
}

/// Дощатый потолок на балках (балки поперёк длинной стороны, если `across`).
fn ceiling(b: &mut Builder, r: Rect, y: f32, across_x: bool) {
    b.decor([r.x0, y, r.z0], [r.x1, y + 0.06, r.z1], Mat::Wood);
    let along_x = if across_x { false } else { r.w() < r.d() };
    let len = if along_x { r.d() } else { r.w() };
    let n = (len / 1.2).floor().max(1.0) as usize;
    for k in 0..=n {
        let s = k as f32 / n as f32;
        if along_x {
            let z = r.z0 + 0.1 + (r.d() - 0.2) * s;
            b.decor([r.x0, y - 0.18, z - 0.08], [r.x1, y, z + 0.08], Mat::Wood);
        } else {
            let x = r.x0 + 0.1 + (r.w() - 0.2) * s;
            b.decor([x - 0.08, y - 0.18, r.z0], [x + 0.08, y, r.z1], Mat::Wood);
        }
    }
}

/// Портик на улицу: мраморные колонны, архитрав, скат; над дверью — доска с надписью.
fn portico(b: &mut Builder, p: &Plan) {
    let o = p.outer;
    let fz = p.facade_z;
    let cz = o.z1 - 0.45;
    // Стилобат — две ступени.
    b.decor([o.x0 + 1.0, 0.0, fz], [o.x1 - 1.0, 0.16, o.z1], Mat::Limestone);
    b.decor([o.x0 + 1.4, 0.16, fz], [o.x1 - 1.4, 0.3, o.z1 - 0.35], Mat::Marble);
    let n = 6;
    let (x0, x1) = (o.x0 + 1.9, o.x1 - 1.9);
    for k in 0..n {
        let x = x0 + (x1 - x0) * k as f32 / (n - 1) as f32;
        column(b, Vec3::new(x, 0.3, cz), 4.3, 0.22, Mat::Marble);
    }
    let top = 4.6;
    b.mesh(Mat::Marble).cuboid(Vec3::new(o.x0 + 1.5, top, cz - 0.3), Vec3::new(o.x1 - 1.5, top + 0.45, cz + 0.3));
    b.mesh(Mat::Limestone).cuboid(Vec3::new(o.x0 + 1.4, top + 0.45, cz - 0.38), Vec3::new(o.x1 - 1.4, top + 0.6, cz + 0.38));
    kit::lean_roof(b, Rect::new(o.x0 + 1.4, fz, o.x1 - 1.4, cz + 0.3), Side::N, FRONT_H + 0.25, top + 0.6, 0.35);
    // Доска над дверью (надпись кладёт клиент поверх).
    let dz = fz + 0.03;
    b.mesh(Mat::Marble).cuboid(Vec3::new(o.cx() - 1.7, 3.62, fz), Vec3::new(o.cx() + 1.7, 4.42, dz + 0.06));
    // Скамьи под портиком.
    for x in [o.x0 + 2.6, o.x1 - 4.6] {
        b.solid([x, 0.3, fz + 0.15], [x + 2.0, 0.72, fz + 0.6], Mat::Marble);
    }
}

/// Доска-вывеска над входом: середина лицевой грани и ширина (для надписи на клиенте).
pub fn sign_board() -> (Vec3, f32, f32) {
    let p = plan();
    (Vec3::new(p.outer.cx(), 4.02, p.facade_z + 0.095), 3.3, 0.72)
}

/// Мозаичная надпись в торце нефа: середина, ширина, высота (лицом на юг, +Z).
pub fn mosaic_panel() -> (Vec3, f32, f32) {
    let p = plan();
    (Vec3::new(p.hall.cx(), 6.7, p.hall.z0 + 0.06), 6.4, 1.8)
}

// ---------------------------------------------------------------------------
// Читальный зал
// ---------------------------------------------------------------------------

fn reading_hall(b: &mut Builder, p: &Plan, rng: &mut Rng) {
    let h = p.hall;
    let (nx0, nx1) = p.nave_x;
    let cx = h.cx();
    // Армарии вдоль наружных стен боковых нефов (по 5 с каждой стороны), между окнами.
    for (x, side) in [(h.x0, Side::W), (h.x1, Side::E)] {
        for k in 0..5 {
            let z = h.z0 + 1.3 + k as f32 * 4.1;
            let f = if side == Side::W { Rect::new(x, z, x + 0.6, z + 1.7) } else { Rect::new(x - 0.6, z, x, z + 1.7) };
            armarium(b, f, side, rng);
            // Между шкафами — капсы со свитками.
            if k < 4 {
                let c = Vec3::new(if side == Side::W { x + 0.45 } else { x - 0.45 }, 0.0, z + 2.65);
                capsa(b, c, rng);
                if k % 2 == 1 {
                    capsa(b, c + Vec3::Z * 0.75, rng);
                }
            }
        }
        // Лестница-стремянка к верхним полкам.
        let lx = if side == Side::W { x + 0.75 } else { x - 0.75 };
        ladder(b, Vec3::new(lx, 0.0, h.z0 + 9.95), if side == Side::W { Vec3::NEG_X } else { Vec3::X });
    }
    // Соты со свитками у северной стены боковых нефов.
    scroll_wall(b, Rect::new(h.x0 + 0.7, h.z0, nx0 - 0.6, h.z0 + 0.5), rng);
    scroll_wall(b, Rect::new(nx1 + 0.6, h.z0, h.x1 - 0.7, h.z0 + 0.5), rng);
    // Ниша под мозаикой: мраморная рама.
    let (mc, mw, mh) = mosaic_panel();
    let m = b.mesh(Mat::Marble);
    m.cuboid(Vec3::new(mc.x - mw / 2.0 - 0.18, mc.y - mh / 2.0 - 0.18, h.z0), Vec3::new(mc.x + mw / 2.0 + 0.18, mc.y - mh / 2.0, h.z0 + 0.12));
    m.cuboid(Vec3::new(mc.x - mw / 2.0 - 0.18, mc.y + mh / 2.0, h.z0), Vec3::new(mc.x + mw / 2.0 + 0.18, mc.y + mh / 2.0 + 0.18, h.z0 + 0.12));
    for s in [-1.0, 1.0] {
        let x = mc.x + s * (mw / 2.0 + 0.09);
        m.cuboid(Vec3::new(x - 0.09, mc.y - mh / 2.0, h.z0), Vec3::new(x + 0.09, mc.y + mh / 2.0, h.z0 + 0.12));
    }
    b.decor([mc.x - mw / 2.0, mc.y - mh / 2.0, h.z0], [mc.x + mw / 2.0, mc.y + mh / 2.0, h.z0 + 0.04], Mat::Mosaic);
    // Под мозаикой — помост с креслом чтеца и большой аналой.
    b.solid([cx - 2.4, 0.0, h.z0], [cx + 2.4, 0.32, h.z0 + 2.6], Mat::Marble);
    interior::chair(b, Vec3::new(cx, 0.32, h.z0 + 1.2), Vec3::NEG_Z);
    lectern_stand(b, Vec3::new(cx, 0.0, h.z0 + 5.6), Vec3::Z, 1.32, 0.55, 0.62, true);
    // Столы со скамьями, на них раскрытые кодексы (из [`lecterns`]).
    for t in table_rows(p) {
        kit::table(b, t, 0.86);
        for s in [-1.0f32, 1.0] {
            let bx = t.cx() + s * 0.85;
            b.solid([bx - 0.2, 0.0, t.z0 + 0.2], [bx + 0.2, 0.45, t.z1 - 0.2], Mat::Wood);
        }
        // Чернильница и лампа посередине стола, стопка кодексов на краю.
        let c = t.center(0.86);
        inkwell(b, c + Vec3::new(0.0, 0.0, 0.05));
        interior::oil_lamp(b, c + Vec3::new(0.0, 0.0, -0.25));
        codex_stack(b, Vec3::new(c.x, 0.86, t.z1 - 0.3), 3, rng);
    }
    // Стол библиофилакса у входа: книга выдачи, свитки ждут возвращения на место.
    let d = Rect::new(cx + 2.0, h.z1 - 3.4, cx + 3.2, h.z1 - 2.7);
    kit::table(b, d, 0.82);
    codex_flat(b, Vec3::new(d.cx() - 0.2, 0.82, d.cz()), Vec2::new(0.32, 0.24), 0.07, Mat::Leather, Vec3::NEG_Z);
    for k in 0..3 {
        scroll(b, Vec3::new(d.cx() + 0.35, 0.86 + k as f32 * 0.05, d.z0 + 0.15 + k as f32 * 0.12), Vec3::Z, 0.28, 0.035, rng);
    }
    interior::stool(b, Vec3::new(cx + 2.6, 0.0, h.z1 - 2.25));
    // Свет: три поликандила над нефом, светильники на подставках у колонн.
    for k in 0..3 {
        polycandelon(b, Vec3::new(cx, 4.2, h.z0 + 4.8 + k as f32 * 5.6), NAVE_H - 0.4);
    }
    for z in [h.z0 + 7.5, h.z0 + 15.0] {
        interior::lampstand(b, Vec3::new(nx0 + 0.9, 0.0, z), true);
        interior::lampstand(b, Vec3::new(nx1 - 0.9, 0.0, z), true);
    }
    b.lamp(Vec3::new(h.x0 + 1.8, 3.2, h.cz()), Color::srgb(1.0, 0.8, 0.6), 9_000.0, 9.0);
    b.lamp(Vec3::new(h.x1 - 1.8, 3.2, h.cz()), Color::srgb(1.0, 0.8, 0.6), 9_000.0, 9.0);
    // Ковёр на помосте.
    interior::rug(b, Rect::new(cx - 1.6, h.z0 + 2.7, cx + 1.6, h.z0 + 4.6), Mat::Rug);
    let _ = (PI, TAU, FRAC_PI_2);
}

/// Армарий: шкаф с распахнутыми створками, на полках — кодексы плашмя и стоймя.
/// `f` — место у стены `side`.
fn armarium(b: &mut Builder, f: Rect, side: Side, rng: &mut Rng) {
    let hgt = 2.75;
    let inward = -side.out();
    let along_x = side.along_x();
    let (len, s0) = if along_x { (f.w(), f.x0) } else { (f.d(), f.z0) };
    let back = match side {
        Side::W => f.x0,
        Side::E => f.x1,
        Side::N => f.z0,
        Side::S => f.z1,
    };
    let depth = 0.55;
    // Точка на «плане шкафа»: s — вдоль стены, d — от стены внутрь.
    let pt = |s: f32, y: f32, d: f32| -> Vec3 {
        if along_x { Vec3::new(s0 + s, y, back + inward.z * d) } else { Vec3::new(back + inward.x * d, y, s0 + s) }
    };
    let bx = |b: &mut Builder, mat: Mat, s: (f32, f32), y: (f32, f32), d: (f32, f32)| {
        let a = pt(s.0, y.0, d.0);
        let c = pt(s.1, y.1, d.1);
        b.mesh(mat).cuboid(a.min(c), a.max(c));
    };
    b.invisible.push({
        let a = pt(0.0, 0.0, 0.0);
        let c = pt(len, hgt, depth);
        Aabb::new(a.min(c), a.max(c))
    });
    // Короб: задник, бока, цоколь, карниз с фронтончиком.
    bx(b, Mat::Wood, (0.0, len), (0.0, hgt), (0.0, 0.04));
    bx(b, Mat::Wood, (0.0, 0.05), (0.0, hgt), (0.0, depth));
    bx(b, Mat::Wood, (len - 0.05, len), (0.0, hgt), (0.0, depth));
    bx(b, Mat::Wood, (-0.03, len + 0.03), (0.0, 0.18), (0.0, depth + 0.03));
    bx(b, Mat::Wood, (-0.06, len + 0.06), (hgt, hgt + 0.1), (0.0, depth + 0.06));
    // Фронтон: треугольник из двух наклонных брусков и тимпан.
    {
        let a = pt(-0.04, hgt + 0.1, depth + 0.05);
        let c = pt(len + 0.04, hgt + 0.1, depth + 0.05);
        let top = pt(len / 2.0, hgt + 0.42, depth + 0.05);
        let m = b.mesh(Mat::Wood);
        m.beam(a, top, 0.035, 0.03, inward);
        m.beam(top, c, 0.035, 0.03, inward);
        let mut tri = MeshBuilder::default();
        tri.tri(a + Vec3::Y * 0.02 - inward * 0.02, c + Vec3::Y * 0.02 - inward * 0.02, top - inward * 0.02 - Vec3::Y * 0.03, inward);
        b.mesh(Mat::Ochre).append(tri);
        b.mesh(Mat::Gold).lathe(top - inward * 0.0 + Vec3::Y * 0.02, &[(0.0, 0.0), (0.04, 0.03), (0.0, 0.1)], 6);
    }
    // Полки и книги.
    let shelves = [0.18, 0.72, 1.24, 1.76, 2.28];
    for (k, &y) in shelves.iter().enumerate() {
        bx(b, Mat::Wood, (0.05, len - 0.05), (y, y + 0.035), (0.04, depth - 0.02));
        if k == shelves.len() - 1 {
            break;
        }
        let y = y + 0.035;
        let room = shelves[k + 1] - y - 0.04;
        let mut s = 0.1;
        while s < len - 0.2 {
            let kind = rng.f();
            if kind < 0.45 {
                // Стопка кодексов плашмя, корешками наружу.
                let w = rng.range(0.22, 0.34);
                if s + w > len - 0.08 {
                    break;
                }
                let n = 1 + rng.int(3);
                let mut yy = y;
                for _ in 0..n {
                    let th = rng.range(0.05, 0.1);
                    if yy + th > y + room {
                        break;
                    }
                    let c = pt(s + w / 2.0, yy, depth * 0.45);
                    let size = if along_x { Vec2::new(w, rng.range(0.24, 0.34)) } else { Vec2::new(rng.range(0.24, 0.34), w) };
                    codex_flat(b, c, size, th, cover(rng), inward);
                    yy += th;
                }
                s += w + 0.04;
            } else if kind < 0.8 {
                // Кодексы стоймя.
                let n = 2 + rng.int(4);
                for _ in 0..n {
                    let th = rng.range(0.05, 0.09);
                    let hh = rng.range(0.26, room.min(0.42));
                    if s + th > len - 0.08 {
                        break;
                    }
                    let c = pt(s + th / 2.0, y, depth * 0.45);
                    codex_upright(b, c, th, hh, rng.range(0.22, 0.3), cover(rng), inward, along_x);
                    s += th + 0.005;
                }
                s += 0.05;
            } else {
                // Свитки, сложенные поленницей (торцами наружу).
                let w = 0.3;
                if s + w > len - 0.08 {
                    break;
                }
                for row in 0..2 {
                    for i in 0..(3 - row) {
                        let r = 0.04;
                        let c = pt(s + 0.06 + i as f32 * 0.09 + row as f32 * 0.045, y + r + row as f32 * 0.075, depth * 0.5);
                        scroll(b, c, inward, depth * 0.8, r, rng);
                    }
                }
                s += w + 0.05;
            }
        }
    }
    // Распахнутые створки (на петлях у боков, развёрнуты на ~135°).
    let door_w = len / 2.0 - 0.02;
    for (edge, sgn) in [(0.0, -1.0f32), (len, 1.0)] {
        let hinge = pt(edge, 0.2, depth + 0.01);
        let along = if along_x { Vec3::X } else { Vec3::Z } * sgn;
        let dir = (along * 0.72 + inward * 0.69).normalize();
        let c = hinge + dir * (door_w / 2.0);
        let yaw = (-dir.z).atan2(dir.x);
        let half = Vec3::new(door_w / 2.0, (hgt - 0.25) / 2.0, 0.018);
        b.mesh(Mat::Wood).obox(c + Vec3::Y * half.y, half, yaw);
        // Филёнки и бронзовое кольцо.
        b.mesh(Mat::Ochre).obox(c + Vec3::Y * (half.y + 0.55) + dir.cross(Vec3::Y) * 0.0, Vec3::new(door_w * 0.36, 0.38, 0.022), yaw);
        b.mesh(Mat::Ochre).obox(c + Vec3::Y * (half.y - 0.45), Vec3::new(door_w * 0.36, 0.38, 0.022), yaw);
        b.mesh(Mat::Bronze).obox(hinge + dir * (door_w - 0.06) + Vec3::Y * 1.25, Vec3::new(0.02, 0.04, 0.03), yaw);
    }
}

/// Случайный цвет переплёта: кожа, крашенная мареной или вайдой, иногда в окладе.
fn cover(rng: &mut Rng) -> Mat {
    match rng.int(10) {
        0..=3 => Mat::Leather,
        4..=6 => Mat::Rug,
        7 | 8 => Mat::Cloth,
        _ => Mat::Felt,
    }
}

/// Кодекс плашмя: `c` — низ середины, `size` — по X и Z, `th` — толщина, `face` — куда смотрит обрез.
fn codex_flat(b: &mut Builder, c: Vec3, size: Vec2, th: f32, mat: Mat, face: Vec3) {
    let h = Vec3::new(size.x / 2.0, 0.0, size.y / 2.0);
    let cov = 0.012;
    b.mesh(mat).cuboid(c - h, c + h + Vec3::Y * cov);
    b.mesh(mat).cuboid(c - h + Vec3::Y * (th - cov), c + h + Vec3::Y * th);
    // Блок страниц (чуть утоплен с трёх сторон, корешок — со стороны, противоположной `face`).
    let spine = -face;
    let inset = Vec3::new(0.01, 0.0, 0.01);
    let mut lo = c - h + inset + Vec3::Y * cov;
    let mut hi = c + h - inset + Vec3::Y * (th - cov);
    if spine.x < -0.5 { lo.x -= 0.0 } else if spine.x > 0.5 { hi.x += 0.0 }
    // Корешок — кожаная полоса.
    let sp = c + spine * (if spine.x.abs() > 0.5 { size.x } else { size.y } / 2.0);
    let across = Vec3::new(spine.z.abs(), 0.0, spine.x.abs());
    let half_len = if spine.x.abs() > 0.5 { size.y } else { size.x } / 2.0;
    b.mesh(mat).cuboid((sp - across * half_len - spine.abs() * 0.012).with_y(c.y), (sp + across * half_len + spine.abs() * 0.0).with_y(c.y + th));
    b.mesh(Mat::Parchment).cuboid(lo, hi);
    // Иногда — застёжки.
    if (c.x * 13.0 + c.z * 7.0).sin() > 0.3 {
        let fp = c + face * ((if face.x.abs() > 0.5 { size.x } else { size.y }) / 2.0) + Vec3::Y * th * 0.5;
        for t in [-0.3f32, 0.3] {
            let q = fp + across * half_len * t * 2.0 * 0.6;
            b.mesh(Mat::Bronze).cuboid(q - Vec3::splat(0.012) - Vec3::Y * th * 0.45, q + Vec3::splat(0.012) + face.abs() * 0.004 + Vec3::Y * th * 0.45);
        }
    }
}

/// Кодекс стоймя на полке: `c` — низ середины; толщина вдоль полки, высота, глубина.
fn codex_upright(b: &mut Builder, c: Vec3, th: f32, h: f32, depth: f32, mat: Mat, face: Vec3, along_x: bool) {
    let half = if along_x { Vec3::new(th / 2.0, 0.0, depth / 2.0) } else { Vec3::new(depth / 2.0, 0.0, th / 2.0) };
    let lo = c - half;
    let hi = c + half + Vec3::Y * h;
    b.mesh(mat).cuboid(lo, hi);
    // Корешок с бинтами (поперечные валики) — к проходу.
    let spine_d = (if along_x { depth } else { depth }) / 2.0;
    let sc = c + face * spine_d;
    for k in 1..4 {
        let y = c.y + h * k as f32 / 4.0;
        let w = if along_x { Vec3::new(th / 2.0 + 0.003, 0.008, 0.006) } else { Vec3::new(0.006, 0.008, th / 2.0 + 0.003) };
        b.mesh(Mat::Leather).cuboid(sc.with_y(y) - w, sc.with_y(y) + w + face.abs() * 0.004);
    }
    // Верхний обрез страниц.
    let inset = if along_x { Vec3::new(0.008, 0.0, 0.015) } else { Vec3::new(0.015, 0.0, 0.008) };
    b.mesh(Mat::Parchment).cuboid(lo + inset + Vec3::Y * 0.01, hi - inset + Vec3::Y * 0.002);
}

/// Свиток: цилиндр вдоль `axis` (середина `c`), длина `len`, радиус `r`; на конце — палочка (омфалос)
/// и ярлычок-силлиб (иногда).
fn scroll(b: &mut Builder, c: Vec3, axis: Vec3, len: f32, r: f32, rng: &mut Rng) {
    let a = axis.normalize();
    let p0 = c - a * len / 2.0;
    let p1 = c + a * len / 2.0;
    let mat = if rng.chance(0.4) { Mat::Canvas } else { Mat::Parchment };
    cylinder(b.mesh(mat), p0, p1, r, 10);
    // Торцы — спираль (тёмный круг поменьше) и палочка.
    cylinder(b.mesh(Mat::Wood), p1, p1 + a * 0.025, r * 0.28, 6);
    cylinder(b.mesh(Mat::Wood), p0 - a * 0.025, p0, r * 0.28, 6);
    if rng.chance(0.35) {
        let tag = p1 + Vec3::Y * -r - a * 0.01;
        let side = a.cross(Vec3::Y).normalize_or_zero();
        b.mesh(Mat::Banner).cuboid(tag - side * 0.025 - Vec3::Y * 0.06 + a * 0.002, tag + side * 0.025 + a * 0.006);
    }
}

/// Цилиндр между точками (с донцами).
fn cylinder(m: &mut MeshBuilder, a: Vec3, c: Vec3, r: f32, sides: usize) {
    let ax = (c - a).normalize_or_zero();
    let u = ax.any_orthonormal_vector();
    let v = ax.cross(u);
    let ring = |k: usize| {
        let t = k as f32 / sides as f32 * TAU;
        u * t.cos() * r + v * t.sin() * r
    };
    for k in 0..sides {
        let (o0, o1) = (ring(k), ring(k + 1));
        let mid = (o0 + o1).normalize_or_zero();
        m.quad([a + o0, a + o1, c + o1, c + o0], mid);
        m.tri(a, a + o0, a + o1, -ax);
        m.tri(c, c + o0, c + o1, ax);
    }
}

/// Капса — кожаное ведёрко со свитками стоймя, крышка прислонена рядом.
fn capsa(b: &mut Builder, c: Vec3, rng: &mut Rng) {
    let r = 0.2;
    let h = 0.5;
    b.mesh(Mat::Leather).lathe(c, &[(0.0, 0.0), (r, 0.0), (r * 1.05, h), (r * 0.95, h)], 12);
    b.mesh(Mat::Bronze).lathe(c + Vec3::Y * (h - 0.06), &[(r * 1.06, 0.0), (r * 1.07, 0.04)], 12);
    b.mesh(Mat::Bronze).lathe(c + Vec3::Y * 0.05, &[(r * 1.01, 0.0), (r * 1.02, 0.04)], 12);
    let n = 5 + rng.int(3);
    for k in 0..n {
        let a = k as f32 / n as f32 * TAU + rng.range(-0.2, 0.2);
        let rr = if k == 0 { 0.0 } else { r * 0.58 };
        let q = c + Vec3::new(a.cos() * rr, 0.0, a.sin() * rr);
        let top = h + rng.range(0.05, 0.2);
        cylinder(b.mesh(if rng.chance(0.5) { Mat::Parchment } else { Mat::Canvas }), q, q + Vec3::Y * top, 0.035, 8);
        cylinder(b.mesh(Mat::Wood), q + Vec3::Y * top, q + Vec3::Y * (top + 0.02), 0.012, 5);
        if rng.chance(0.5) {
            b.mesh(Mat::Banner).cuboid(q + Vec3::new(-0.02, top - 0.08, 0.034), q + Vec3::new(0.02, top - 0.01, 0.038));
        }
    }
    b.invisible.push(Aabb::new(c - Vec3::new(r, 0.0, r), c + Vec3::new(r, h, r)));
}

/// Стена-соты для свитков (лаковые ниши, «nidi»): решётка ячеек, в каждой — несколько свитков.
fn scroll_wall(b: &mut Builder, f: Rect, rng: &mut Rng) {
    let (x0, x1) = (f.x0, f.x1);
    let z_back = f.z0;
    let depth = 0.48;
    let cell = 0.3;
    let y0 = 0.25;
    let rows = 9;
    let cols = ((x1 - x0) / cell).floor() as usize;
    let w = cols as f32 * cell;
    let xs = (x0 + x1) / 2.0 - w / 2.0;
    let top = y0 + rows as f32 * cell;
    b.invisible.push(Aabb::new(Vec3::new(xs, 0.0, z_back), Vec3::new(xs + w, top, z_back + depth)));
    let m = b.mesh(Mat::Wood);
    m.cuboid(Vec3::new(xs - 0.05, 0.0, z_back), Vec3::new(xs + w + 0.05, y0, z_back + depth + 0.03));
    m.cuboid(Vec3::new(xs - 0.07, top, z_back), Vec3::new(xs + w + 0.07, top + 0.12, z_back + depth + 0.06));
    m.cuboid(Vec3::new(xs, y0, z_back), Vec3::new(xs + w, top, z_back + 0.03));
    for i in 0..=rows {
        let y = y0 + i as f32 * cell;
        m.cuboid(Vec3::new(xs, y - 0.015, z_back), Vec3::new(xs + w, y + 0.015, z_back + depth));
    }
    for j in 0..=cols {
        let x = xs + j as f32 * cell;
        m.cuboid(Vec3::new(x - 0.015, y0, z_back), Vec3::new(x + 0.015, top, z_back + depth));
    }
    for i in 0..rows {
        for j in 0..cols {
            if rng.chance(0.08) {
                continue;
            }
            let cx = xs + (j as f32 + 0.5) * cell;
            let cy = y0 + i as f32 * cell + 0.015;
            let n = 2 + rng.int(4);
            for k in 0..n {
                let r = rng.range(0.035, 0.05);
                let (dx, dy) = match k {
                    0 => (-0.06, 0.0),
                    1 => (0.05, 0.0),
                    2 => (0.0, 0.08),
                    3 => (-0.08, 0.085),
                    _ => (0.08, 0.09),
                };
                let c = Vec3::new(cx + dx, cy + r + dy, z_back + 0.05 + depth / 2.0 + rng.range(-0.02, 0.03));
                scroll(b, c, Vec3::Z, depth - rng.range(0.02, 0.1), r, rng);
            }
        }
    }
    // Лесенка у сот.
    ladder(b, Vec3::new(xs + w - 0.2, 0.0, z_back + depth + 0.25), Vec3::NEG_Z);
}

/// Приставная лестница; `to_wall` — куда она наклонена.
fn ladder(b: &mut Builder, foot: Vec3, to_wall: Vec3) {
    let h = 2.6;
    let lean = to_wall * 0.55;
    let side = Vec3::new(to_wall.z, 0.0, to_wall.x).abs() * 0.22;
    let m = b.mesh(Mat::Wood);
    for s in [-1.0, 1.0] {
        m.beam(foot + side * s, foot + side * s + lean + Vec3::Y * h, 0.025, 0.025, to_wall);
    }
    for k in 1..8 {
        let t = k as f32 / 8.0;
        let c = foot + lean * t + Vec3::Y * h * t;
        m.beam(c - side, c + side, 0.015, 0.015, Vec3::Y);
    }
}

/// Наклонная доска пюпитра на столе (`p` — середина на столешнице, `toward` — к читателю).
fn desk_slope(b: &mut Builder, p: Vec3, toward: Vec3, w: f32, d: f32) {
    let t = toward.normalize();
    let right = Vec3::Y.cross(t).normalize();
    let tilt: f32 = 0.42;
    let lo = p + t * (d / 2.0 * tilt.cos()) + Vec3::Y * 0.03;
    let hi = p - t * (d / 2.0 * tilt.cos()) + Vec3::Y * (0.03 + d * tilt.sin());
    let m = b.mesh(Mat::Wood);
    m.beam(lo, hi, w / 2.0, 0.012, right.cross(hi - lo).normalize());
    // Подставки и бортик, на который опирается книга.
    m.beam(p - t * (d / 2.0 * tilt.cos()) - right * (w / 2.0 - 0.03), hi - right * (w / 2.0 - 0.03), 0.015, 0.015, t);
    m.beam(p - t * (d / 2.0 * tilt.cos()) + right * (w / 2.0 - 0.03), hi + right * (w / 2.0 - 0.03), 0.015, 0.015, t);
    m.beam(lo - right * (w / 2.0) + Vec3::Y * 0.02, lo + right * (w / 2.0) + Vec3::Y * 0.02, 0.012, 0.02, Vec3::Y);
}

/// Отдельно стоящий аналой: точёная стойка на трёх ножках, наклонная доска, сверху — книга.
fn lectern_stand(b: &mut Builder, foot: Vec3, toward: Vec3, top: f32, tilt: f32, w: f32, grand: bool) {
    let t = toward.normalize();
    let right = Vec3::Y.cross(t).normalize();
    let mat = if grand { Mat::Marble } else { Mat::Wood };
    let m = b.mesh(mat);
    if grand {
        m.lathe(foot, &[(0.32, 0.0), (0.32, 0.12), (0.22, 0.2), (0.11, 0.3), (0.09, top - 0.35), (0.14, top - 0.25), (0.18, top - 0.18)], 12);
    } else {
        m.lathe(foot, &[(0.2, 0.0), (0.2, 0.05), (0.05, 0.12), (0.04, top - 0.25), (0.08, top - 0.18)], 8);
    }
    let d = 0.5;
    let c = foot + Vec3::Y * (top - 0.04);
    let lo = c + t * (d / 2.0 * tilt.cos()) - Vec3::Y * (d / 2.0 * tilt.sin());
    let hi = c - t * (d / 2.0 * tilt.cos()) + Vec3::Y * (d / 2.0 * tilt.sin());
    let mm = b.mesh(Mat::Wood);
    mm.beam(lo, hi, w / 2.0 + 0.04, 0.02, right.cross(hi - lo).normalize());
    mm.beam(lo - right * (w / 2.0 + 0.04) + Vec3::Y * 0.025, lo + right * (w / 2.0 + 0.04) + Vec3::Y * 0.025, 0.015, 0.025, Vec3::Y);
    if grand {
        // Покров из пурпурной ткани с золотой каймой под книгой.
        b.mesh(Mat::Banner).beam(lo + t * 0.08 - Vec3::Y * 0.12, hi - t * 0.0 + Vec3::Y * 0.005, w / 2.0 + 0.1, 0.006, right.cross(hi - lo).normalize());
        b.invisible.push(Aabb::new(foot - Vec3::new(0.35, 0.0, 0.35), foot + Vec3::new(0.35, top, 0.35)));
        // Свечи по сторонам.
        for s in [-1.0, 1.0] {
            let q = foot + right * s * 0.75;
            interior::lampstand(b, q, s > 0.0);
        }
    }
}

/// Раскрытая книга (или свиток) по описанию [`Lectern`]: два листа, обрез, переплёт.
fn open_book(b: &mut Builder, l: &Lectern) {
    let (right, up, n) = l.axes();
    let c = l.at;
    let hw = l.width / 2.0;
    if l.scroll {
        // Свиток раскатан: середина листа и два свёрнутых конца.
        let hh = l.page_height() / 2.0;
        let m = b.mesh(Mat::Parchment);
        m.cuboid(c - right * hw - up * hh - Vec3::Y * 0.004, c + right * hw + up * hh);
        for s in [-1.0, 1.0] {
            let e = c + right * s * (hw + 0.035) + Vec3::Y * 0.03;
            cylinder(b.mesh(Mat::Parchment), e - up * (hh + 0.01), e + up * (hh + 0.01), 0.035, 10);
            cylinder(b.mesh(Mat::Wood), e + up * (hh + 0.01), e + up * (hh + 0.04), 0.012, 6);
            cylinder(b.mesh(Mat::Wood), e - up * (hh + 0.04), e - up * (hh + 0.01), 0.012, 6);
        }
        // Грузики, чтобы свиток не скатывался.
        for s in [-0.6f32, 0.6] {
            b.mesh(Mat::Bronze).lathe(c + right * hw * s + up * (hh + 0.06), &[(0.0, 0.0), (0.025, 0.0), (0.02, 0.03), (0.0, 0.035)], 8);
        }
        return;
    }
    let hh = l.page_height() / 2.0;
    // Переплёт под страницами и блок листов (каждая половина чуть приподнята к краю).
    let cov = if l.text == 0 { Mat::Gold } else { Mat::Leather };
    let m = b.mesh(cov);
    m.beam(c - right * (hw + 0.012) - n * 0.02, c + right * (hw + 0.012) - n * 0.02, hh + 0.012, 0.005, n);
    for s in [-1.0f32, 1.0] {
        let edge = c + right * s * hw * 0.5 - n * 0.008;
        b.mesh(Mat::Parchment).beam(edge - right * hw * 0.5, edge + right * hw * 0.5, hh, 0.007, n);
    }
    // Ляссе — красная закладка, свисает с нижнего края.
    let tail = c - up * hh + right * 0.03;
    b.mesh(Mat::Banner).beam(tail, tail - up * 0.08 - n * 0.06, 0.006, 0.001, n);
}

/// Стопка кодексов на столе.
fn codex_stack(b: &mut Builder, p: Vec3, n: usize, rng: &mut Rng) {
    let mut y = p.y;
    for k in 0..n {
        let th = rng.range(0.04, 0.08);
        let off = Vec3::new(rng.range(-0.03, 0.03), 0.0, rng.range(-0.03, 0.03));
        codex_flat(b, Vec3::new(p.x, y, p.z) + off, Vec2::new(rng.range(0.22, 0.3), rng.range(0.28, 0.34)), th, cover(rng), if k % 2 == 0 { Vec3::X } else { Vec3::NEG_X });
        y += th;
    }
}

/// Бронзовая чернильница с двумя тростниковыми каламами.
fn inkwell(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Bronze).lathe(p, &[(0.0, 0.0), (0.04, 0.0), (0.045, 0.04), (0.03, 0.055), (0.015, 0.06), (0.0, 0.058)], 10);
    b.mesh(Mat::Coals).disc(p + Vec3::Y * 0.059, 0.014, 6, true);
    let m = b.mesh(Mat::Thatch);
    m.beam(p + Vec3::new(0.005, 0.05, 0.0), p + Vec3::new(0.09, 0.2, 0.05), 0.004, 0.004, Vec3::Y);
    m.beam(p + Vec3::new(-0.005, 0.05, 0.003), p + Vec3::new(-0.06, 0.21, -0.07), 0.004, 0.004, Vec3::Y);
}

// ---------------------------------------------------------------------------
// Двор
// ---------------------------------------------------------------------------

fn court(b: &mut Builder, p: &Plan, rng: &mut Rng) {
    let c = p.court;
    // Портики вдоль крыльев: колонны в 2 м от стены, скат к стене крыла.
    let inset = 2.0;
    let ch = 3.3;
    for (x, side) in [(c.x0 + inset, Side::W), (c.x1 - inset, Side::E)] {
        let n = 4;
        for k in 0..=n {
            let z = c.z0 + 1.0 + (c.d() - 2.0) * k as f32 / n as f32;
            column(b, Vec3::new(x, 0.0, z), ch, 0.18, Mat::Limestone);
        }
        b.mesh(Mat::Wood).beam(Vec3::new(x, ch + 0.13, c.z0 + 0.6), Vec3::new(x, ch + 0.13, c.z1 - 0.6), 0.16, 0.13, Vec3::Y);
        let r = if side == Side::W { Rect::new(c.x0, c.z0 + 0.6, x + 0.2, c.z1 - 0.6) } else { Rect::new(x - 0.2, c.z0 + 0.6, c.x1, c.z1 - 0.6) };
        kit::lean_roof(b, r, side, WING_H - 0.1, ch + 0.28, 0.35);
        // Каменные скамьи и столы для чтения у стены крыла.
        let bx = if side == Side::W { c.x0 + 0.15 } else { c.x1 - 0.55 };
        b.solid([bx, 0.0, c.z0 + 3.4], [bx + 0.4, 0.45, c.z0 + 5.6], Mat::Limestone);
        let t = Rect::new(if side == Side::W { c.x0 + 0.6 } else { c.x1 - 1.2 }, c.cz() + 1.1, if side == Side::W { c.x0 + 1.2 } else { c.x1 - 0.6 }, c.cz() + 2.1);
        kit::table(b, t, 0.75);
        interior::stool(b, Vec3::new(if side == Side::W { c.x0 + 0.35 } else { c.x1 - 0.35 }, 0.0, t.cz()));
    }
    // Плиты под портиками и дорожки, посередине — садик.
    b.decor([c.x0, 0.0, c.z0], [c.x1, 0.03, c.z1], Mat::Paving);
    let g = Rect::new(c.x0 + inset + 0.6, c.z0 + 1.6, c.x1 - inset - 0.6, c.z1 - 1.6);
    for (x0, x1) in [(g.x0, c.cx() - 0.7), (c.cx() + 0.7, g.x1)] {
        for (z0, z1) in [(g.z0, c.cz() - 0.7), (c.cz() + 0.7, g.z1)] {
            b.decor([x0, 0.0, z0], [x1, 0.045, z1], Mat::Earth);
            b.mesh(Mat::Limestone).cuboid(Vec3::new(x0 - 0.08, 0.0, z0 - 0.08), Vec3::new(x1 + 0.08, 0.12, z0));
            b.mesh(Mat::Limestone).cuboid(Vec3::new(x0 - 0.08, 0.0, z1), Vec3::new(x1 + 0.08, 0.12, z1 + 0.08));
            // Травы и розы на клумбах.
            let cc = Vec3::new((x0 + x1) / 2.0, 0.0, (z0 + z1) / 2.0);
            kit::shrub(b, cc + Vec3::new(-0.5, 0.0, 0.4), 0.7);
            kit::shrub(b, cc + Vec3::new(0.6, 0.0, -0.5), 0.6);
        }
    }
    // Кипарисы по углам сада, колодец посередине.
    for (x, z) in [(g.x0 + 0.5, g.z0 + 0.5), (g.x1 - 0.5, g.z0 + 0.5), (g.x0 + 0.5, g.z1 - 0.5), (g.x1 - 0.5, g.z1 - 0.5)] {
        kit::cypress(b, Vec3::new(x, 0.0, z), rng.range(5.5, 7.0));
    }
    kit::cistern_mouth(b, c.center(0.0));
}

// ---------------------------------------------------------------------------
// Скрипторий, пергаментная, передние комнаты
// ---------------------------------------------------------------------------

fn scriptorium(b: &mut Builder, p: &Plan, rng: &mut Rng) {
    let s = p.scriptorium;
    // Места переписчиков у окон во двор (окна на восточной стене): табурет, пюпитр-подставка.
    let wx = s.x1;
    for (k, z) in [s.z0 + 2.6, s.z0 + 5.6, s.z0 + 8.6].into_iter().enumerate() {
        if k == 0 {
            // Подставка с образцом (раскрытая книга — из [`lecterns`]).
            lectern_stand(b, Vec3::new(s.x0 + 1.25 - 0.18, 0.0, z), Vec3::X, 1.05, 0.6, 0.4, false);
        }
        scribe_place(b, Vec3::new(wx - 1.2, 0.0, z), Vec3::NEG_X, rng);
    }
    // Шкаф с чистыми тетрадями и перьями, полка с образцами.
    armarium(b, Rect::new(s.x0, s.z1 - 2.4, s.x0 + 0.6, s.z1 - 0.6), Side::W, rng);
    let t = Rect::new(s.x0 + 0.4, s.z0 + 9.8, s.x0 + 1.4, s.z0 + 11.4);
    kit::table(b, t, 0.8);
    for k in 0..4 {
        quire(b, Vec3::new(t.cx() + (k % 2) as f32 * 0.25 - 0.12, 0.8 + (k / 2) as f32 * 0.02, t.z0 + 0.35 + (k / 2) as f32 * 0.5));
    }
    inkwell(b, Vec3::new(t.cx(), 0.8, t.z1 - 0.25));
    // Свет: светильники у мест переписчиков (по вечерам).
    b.lamp(Vec3::new(s.cx(), 2.6, s.cz()), Color::srgb(1.0, 0.72, 0.45), 4_000.0, 7.0);
    interior::lampstand(b, Vec3::new(s.x0 + 0.5, 0.0, s.z0 + 7.1), true);
}

/// Место переписчика: табурет, низкий столик с наклонной доской, чернильница, листы.
/// `p` — середина, `face` — куда смотрит пишущий.
fn scribe_place(b: &mut Builder, p: Vec3, face: Vec3, rng: &mut Rng) {
    let seat = p - face * 0.45;
    interior::stool(b, seat);
    let desk = p + face * 0.15;
    let r = Rect::new(desk.x - 0.25, desk.z - 0.35, desk.x + 0.25, desk.z + 0.35);
    kit::table(b, r, 0.72);
    desk_slope(b, Vec3::new(desk.x, 0.72, desk.z), -face, 0.4, 0.42);
    // Лист на доске, чернильница, нож, линейка.
    let c = Vec3::new(desk.x, 0.72, desk.z);
    let up = Vec3::Y * 0.03 + face * 0.0;
    b.mesh(Mat::Parchment).beam(c + face * 0.12 + Vec3::Y * 0.14 + up, c - face * 0.14 + Vec3::Y * 0.05 + up, 0.15, 0.003, Vec3::Y);
    inkwell(b, c + Vec3::new(0.0, 0.0, 0.0) + face.cross(Vec3::Y) * 0.2 - face * 0.1);
    b.mesh(Mat::Iron).beam(c - face.cross(Vec3::Y) * 0.2 - face * 0.15 + Vec3::Y * 0.01, c - face.cross(Vec3::Y) * 0.12 - face * 0.2 + Vec3::Y * 0.01, 0.006, 0.003, Vec3::Y);
    // Стопка готовых листов на полу рядом (тетради-кватернионы).
    if rng.chance(0.7) {
        quire(b, seat + face.cross(Vec3::Y) * 0.45);
    }
}

/// Тетрадь (кватернион) — сложенные листы, перевязанные бечёвкой.
fn quire(b: &mut Builder, p: Vec3) {
    b.mesh(Mat::Parchment).cuboid(p + Vec3::new(-0.13, 0.0, -0.17), p + Vec3::new(0.13, 0.025, 0.17));
    b.mesh(Mat::Thatch).cuboid(p + Vec3::new(-0.132, 0.0, -0.01), p + Vec3::new(0.132, 0.028, 0.01));
}

fn parchment_room(b: &mut Builder, p: &Plan, rng: &mut Rng) {
    let s = p.parchment;
    // Рамы с растянутыми кожами (сохнут) у восточной стены.
    for (k, z) in [s.z0 + 1.6, s.z0 + 3.6, s.z0 + 5.6].into_iter().enumerate() {
        stretch_frame(b, Vec3::new(s.x1 - 0.45, 0.0, z), Vec3::NEG_X, k);
    }
    // Чаны с известью (кожи вымачивают перед выскабливанием).
    for z in [s.z0 + 8.4, s.z0 + 9.5] {
        let c = Vec3::new(s.x1 - 0.7, 0.0, z);
        b.mesh(Mat::Wood).lathe(c, &[(0.0, 0.0), (0.42, 0.0), (0.45, 0.62), (0.4, 0.62)], 12);
        b.mesh(Mat::PetalWhite).disc(c + Vec3::Y * 0.55, 0.4, 12, true);
        b.mesh(Mat::Iron).lathe(c + Vec3::Y * 0.45, &[(0.455, 0.0), (0.46, 0.05)], 12);
        b.invisible.push(Aabb::new(c - Vec3::new(0.45, 0.0, 0.45), c + Vec3::new(0.45, 0.62, 0.45)));
    }
    // Колода для скобления, скребок-лунарий, стопка готового пергамента.
    let t = Rect::new(s.cx() - 0.5, s.z0 + 6.6, s.cx() + 0.5, s.z0 + 8.0);
    kit::table(b, t, 0.85);
    let c = t.center(0.85);
    b.mesh(Mat::Parchment).cuboid(c + Vec3::new(-0.42, 0.0, -0.55), c + Vec3::new(0.38, 0.01, 0.5));
    b.mesh(Mat::Iron).lathe_arc(c + Vec3::new(0.1, 0.012, 0.2), &[(0.08, 0.0), (0.08, 0.006)], 0.0, PI, 8, false);
    b.mesh(Mat::Wood).beam(c + Vec3::new(0.1, 0.02, 0.2), c + Vec3::new(0.1, 0.02, 0.36), 0.012, 0.012, Vec3::Y);
    for k in 0..6 {
        b.mesh(Mat::Parchment).cuboid(Vec3::new(s.x0 + 0.3, 0.0 + k as f32 * 0.03, s.z1 - 1.6), Vec3::new(s.x0 + 1.0, 0.025 + k as f32 * 0.03, s.z1 - 0.6));
    }
    // Шкуры на шесте.
    let a = Vec3::new(s.x0 + 0.3, 2.3, s.z0 + 0.8);
    let c2 = Vec3::new(s.x0 + 0.3, 2.3, s.z0 + 4.2);
    b.mesh(Mat::Wood).beam(a, c2, 0.03, 0.03, Vec3::Y);
    for k in 0..3 {
        let z = s.z0 + 1.2 + k as f32 * 1.05;
        b.mesh(if k == 1 { Mat::Leather } else { Mat::Felt }).cuboid(Vec3::new(s.x0 + 0.27, 1.3 + rng.range(0.0, 0.2), z - 0.4), Vec3::new(s.x0 + 0.33, 2.3, z + 0.4));
    }
    b.lamp(Vec3::new(s.cx(), 2.6, s.cz()), Color::srgb(1.0, 0.72, 0.45), 2_500.0, 6.0);
}

/// Деревянная рама с кожей на шнурах; `out` — лицом в комнату.
fn stretch_frame(b: &mut Builder, foot: Vec3, out: Vec3, k: usize) {
    let side = Vec3::new(out.z, 0.0, out.x).abs();
    let w = 0.75;
    let h0 = 0.35;
    let h1 = 2.05;
    let lean = -out * 0.18;
    let m = b.mesh(Mat::Wood);
    for s in [-1.0, 1.0] {
        m.beam(foot + side * w * s, foot + side * w * s + lean + Vec3::Y * (h1 + 0.1), 0.035, 0.035, out);
    }
    for y in [h0, h1] {
        let t = y / (h1 + 0.1);
        m.beam(foot - side * w + lean * t + Vec3::Y * y, foot + side * w + lean * t + Vec3::Y * y, 0.03, 0.03, Vec3::Y);
    }
    // Кожа — чуть меньше рамы, шнуры от края к раме.
    let mid = |y: f32| foot + lean * (y / (h1 + 0.1)) + Vec3::Y * y;
    let sw = w - 0.17;
    let (y0, y1) = (h0 + 0.12, h1 - 0.12);
    let mut q = MeshBuilder::default();
    q.quad([mid(y0) - side * sw, mid(y0) + side * sw, mid(y1) + side * sw * 0.9, mid(y1) - side * sw * 0.9], out);
    q.quad([mid(y0) - side * sw, mid(y0) + side * sw, mid(y1) + side * sw * 0.9, mid(y1) - side * sw * 0.9], -out);
    b.mesh(if k == 1 { Mat::Felt } else { Mat::Parchment }).append(q);
    let cord = b.mesh(Mat::Thatch);
    for i in 0..6 {
        let t = (i as f32 + 0.5) / 6.0;
        let y = y0 + (y1 - y0) * t;
        for s in [-1.0, 1.0] {
            cord.beam(mid(y) + side * sw * s, mid(y) + side * w * s, 0.004, 0.004, Vec3::Y);
        }
        let x = -sw + 2.0 * sw * t;
        cord.beam(mid(y0) + side * x, mid(h0) + side * x, 0.004, 0.004, out);
        cord.beam(mid(y1) + side * x * 0.9, mid(h1) + side * x * 0.9, 0.004, 0.004, out);
    }
    b.invisible.push(Aabb::new(foot - side * w - Vec3::new(0.1, 0.0, 0.1) + lean.min(Vec3::ZERO), foot + side * w + Vec3::new(0.1, h1, 0.1) + lean.max(Vec3::ZERO)));
}

fn front_rooms(b: &mut Builder, p: &Plan, rng: &mut Rng) {
    // Вестибюль: скамьи, ниша-гардероб (колышки для плащей), табличка с правилами.
    let v = p.vestibule;
    kit::bench(b, Rect::new(v.x0 + 0.3, v.z0 + 1.8, v.x0 + 0.7, v.z1 - 1.2));
    kit::bench(b, Rect::new(v.x1 - 0.7, v.z0 + 1.8, v.x1 - 0.3, v.z1 - 1.2));
    interior::pegs(b, Vec3::new(v.x0, 1.7, v.cz()), Vec3::X, Mat::Cloth);
    interior::pegs(b, Vec3::new(v.x1, 1.7, v.cz() + 0.6), Vec3::NEG_X, Mat::Canvas);
    polycandelon(b, Vec3::new(v.cx(), 3.1, v.cz()), FRONT_H - 0.3);
    // Келья библиофилакса: стол с описью, ложе, сундук, икона.
    let o = p.office;
    let t = Rect::new(o.cx() - 0.5, o.z0 + 1.0, o.cx() + 0.9, o.z0 + 1.7);
    kit::table(b, t, 0.8);
    interior::chair(b, Vec3::new(o.cx() + 0.2, 0.0, o.z0 + 2.25), Vec3::Z);
    inkwell(b, Vec3::new(t.x1 - 0.2, 0.8, t.cz()));
    interior::oil_lamp(b, Vec3::new(t.x0 + 0.15, 0.8, t.cz()));
    interior::bed(b, Rect::new(o.x0 + 0.15, o.z1 - 2.1, o.x0 + 1.0, o.z1 - 0.15), rng);
    interior::chest(b, Rect::new(o.x1 - 1.1, o.z1 - 0.75, o.x1 - 0.1, o.z1 - 0.15), "Сундук библиофилакса");
    interior::wall_cross(b, Vec3::new(o.x0 + 0.01, 1.9, o.cz()), Vec3::X);
    b.lamp(Vec3::new(o.cx(), 2.4, o.cz()), Color::srgb(1.0, 0.7, 0.42), 1_800.0, 5.0);
    // Переплётная: верстак с прессом, доски для переплётов, кожи, нитки.
    let d = p.bindery;
    let w = Rect::new(d.cx() - 1.2, d.z0 + 0.2, d.cx() + 0.6, d.z0 + 0.9);
    kit::table(b, w, 0.85);
    book_press(b, Vec3::new(w.cx() - 0.4, 0.85, w.cz()));
    for k in 0..3 {
        codex_flat(b, Vec3::new(w.x1 - 0.3, 0.85 + k as f32 * 0.06, w.cz()), Vec2::new(0.26, 0.34), 0.06, if k == 0 { Mat::Wood } else { Mat::Leather }, Vec3::Z);
    }
    interior::stool(b, Vec3::new(d.cx() - 0.4, 0.0, d.z0 + 1.2));
    shelf_of_boards(b, Rect::new(d.x1 - 0.45, d.z0 + 0.4, d.x1 - 0.05, d.z1 - 0.6));
    b.lamp(Vec3::new(d.cx(), 2.4, d.cz()), Color::srgb(1.0, 0.7, 0.42), 1_800.0, 5.0);
}

/// Переплётный пресс: две толстые доски и два деревянных винта.
fn book_press(b: &mut Builder, p: Vec3) {
    let m = b.mesh(Mat::Wood);
    m.cuboid(p + Vec3::new(-0.3, 0.0, -0.2), p + Vec3::new(0.3, 0.08, 0.2));
    m.cuboid(p + Vec3::new(-0.3, 0.2, -0.2), p + Vec3::new(0.3, 0.28, 0.2));
    for s in [-0.24, 0.24] {
        m.lathe(p + Vec3::new(s, 0.0, 0.0), &[(0.025, 0.0), (0.025, 0.42), (0.05, 0.42), (0.05, 0.48), (0.0, 0.48)], 8);
        m.beam(p + Vec3::new(s - 0.09, 0.45, 0.0), p + Vec3::new(s + 0.09, 0.45, 0.0), 0.012, 0.012, Vec3::Y);
    }
    // Зажатые тетради.
    b.mesh(Mat::Parchment).cuboid(p + Vec3::new(-0.15, 0.08, -0.17), p + Vec3::new(0.15, 0.2, 0.17));
}

/// Стеллаж с переплётными досками и рулонами кожи.
fn shelf_of_boards(b: &mut Builder, r: Rect) {
    b.invisible.push(Aabb::new(r.min3(0.0), r.max3(1.9)));
    let m = b.mesh(Mat::Wood);
    for y in [0.3, 0.9, 1.5] {
        m.cuboid(r.min3(y - 0.03), r.max3(y));
    }
    for (x, z) in [(r.x0, r.z0), (r.x1, r.z0), (r.x0, r.z1), (r.x1, r.z1)] {
        m.cuboid(Vec3::new(x - 0.02, 0.0, z - 0.02), Vec3::new(x + 0.02, 1.9, z + 0.02));
    }
    let mut z = r.z0 + 0.1;
    while z < r.z1 - 0.3 {
        b.mesh(Mat::Wood).cuboid(Vec3::new(r.x0 + 0.05, 0.3, z), Vec3::new(r.x1 - 0.05, 0.62, z + 0.025));
        z += 0.06;
    }
    for k in 0..3 {
        let c = Vec3::new(r.cx(), 0.9 + 0.08, r.z0 + 0.4 + k as f32 * 0.4);
        cylinder(b.mesh(Mat::Leather), c - Vec3::X * 0.17, c + Vec3::X * 0.17, 0.08, 10);
    }
    for k in 0..4 {
        let c = Vec3::new(r.cx(), 1.5 + 0.06, r.z0 + 0.3 + k as f32 * 0.25);
        b.mesh(Mat::Canvas).lathe(c, &[(0.0, 0.0), (0.05, 0.0), (0.05, 0.1), (0.0, 0.1)], 8);
    }
}
