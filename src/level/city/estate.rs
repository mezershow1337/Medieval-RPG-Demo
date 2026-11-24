//! Усадьбы на продажу и стройплощадки.
//!
//! * Пустующая усадьба строится как обычная, но без жильцов и обстановки: голые стены, двор
//!   с цистерной и пара сундуков. Для каждой комнаты заранее собирается обстановка всех видов
//!   ([`FurnishKind`]) — отдельными переключаемыми группами; купивший дом заказывает мастерам,
//!   и группа включается (и в коллизиях, и на экране).
//! * Стройплощадка — участок сгоревшей усадьбы. Готовый дом собирается целиком, а из него
//!   нарезаются стадии: пепелище (обломки стен разной высоты, обгорелые балки), фундамент
//!   (кладка по колено, штабеля камня и леса, яма для извести), стены в рост человека с лесами,
//!   стены под крышу со стропилами и настилом без черепицы, готовый дом. Стадии переключает
//!   симуляция города (`towns.rs`): курия нанимает каменщиков, когда городу тесно.

use super::interior::{self, Household};
use super::kit::{Rect, Rng};
use super::*;
use crate::collision::Aabb;
use crate::level::{
    Block, Chunk, DoorSpec, EstateRoom, EstateSpec, FurnishKind, Lamp, SwitchGroup, SwitchKey,
};
use crate::towns::{SITE_DONE, TownId};

/// Комната для обстановки: прямоугольник по внутренним граням стен, высота пола, что можно поставить.
pub struct RoomPlan {
    pub r: Rect,
    pub floor: f32,
    pub kinds: Vec<FurnishKind>,
}

pub const GROUND_KINDS: [FurnishKind; 6] = [
    FurnishKind::Living,
    FurnishKind::Bedroom,
    FurnishKind::Kitchen,
    FurnishKind::Storeroom,
    FurnishKind::Weaving,
    FurnishKind::Workshop,
];
pub const UPPER_KINDS: [FurnishKind; 3] = [
    FurnishKind::Loft,
    FurnishKind::Storeroom,
    FurnishKind::Weaving,
];

/// Записать усадьбу, построенную в `b` начиная с блоков `blocks0` (двери `doors`, сундуки `containers`).
pub fn register(
    b: &mut Builder,
    name: String,
    town: TownId,
    area: Rect,
    top: f32,
    entrance: Vec3,
    doors: std::ops::Range<usize>,
    containers: std::ops::Range<usize>,
    blocks0: usize,
    rooms: Vec<RoomPlan>,
    two_storey: bool,
    site: Option<u16>,
) {
    let estate = b.estates.len() as u16;
    let house_blocks: Vec<Block> = b.blocks[blocks0..].to_vec();
    let house_doors: Vec<DoorSpec> = b.doors[doors.clone()].to_vec();
    register_with(b, estate, &house_blocks, &house_doors, &rooms);
    b.estates.push(EstateSpec {
        name,
        town,
        min: Vec3::new(area.x0, 0.0, area.z0),
        max: Vec3::new(area.x1, top, area.z1),
        entrance,
        doors: doors.map(|d| d as u32).collect(),
        containers: containers.map(|c| c as u32).collect(),
        rooms: rooms
            .iter()
            .map(|r| EstateRoom {
                min: r.r.min3(r.floor),
                max: r.r.max3(r.floor + 3.0),
                kinds: r.kinds.clone(),
            })
            .collect(),
        two_storey,
        site,
    });
}

/// Группы обстановки всех комнат усадьбы.
fn register_with(
    b: &mut Builder,
    estate: u16,
    blocks: &[Block],
    doors: &[DoorSpec],
    rooms: &[RoomPlan],
) {
    for (ri, room) in rooms.iter().enumerate() {
        for (ki, &kind) in room.kinds.iter().enumerate() {
            let seed = (estate as u32) * 7919 + ri as u32 * 131 + ki as u32 * 17 + 5;
            let g = furnish_group(b.chunk, blocks, doors, &b.keep_clear, room, kind, seed);
            b.switches.push(SwitchGroup {
                key: SwitchKey::Furnish {
                    estate,
                    room: ri as u8,
                    kind,
                },
                ..g
            });
        }
    }
}

fn interior_kind(kind: FurnishKind, rng: &mut Rng) -> interior::Kind {
    match kind {
        FurnishKind::Living => interior::Kind::Living,
        FurnishKind::Bedroom => interior::Kind::Bedroom,
        FurnishKind::Kitchen => interior::Kind::Kitchen,
        FurnishKind::Storeroom => interior::Kind::Storeroom,
        FurnishKind::Weaving => interior::Kind::Weaving,
        FurnishKind::Workshop => interior::Kind::Workshop(interior::Trade::pick(rng)),
        FurnishKind::Loft => interior::Kind::Loft,
    }
}

/// Обстановка одной комнаты в отдельном сборщике (видит стены и двери дома, чтобы не загородить проход).
fn furnish_group(
    chunk: Chunk,
    blocks: &[Block],
    doors: &[DoorSpec],
    keep_clear: &[Vec3],
    room: &RoomPlan,
    kind: FurnishKind,
    seed: u32,
) -> SwitchGroup {
    let lift = Vec3::Y * room.floor;
    let mut t = Builder::new();
    t.chunk = chunk;
    let near = room.r.inset(-2.0);
    let inside = |p: Vec3| p.x > near.x0 && p.x < near.x1 && p.z > near.z0 && p.z < near.z1;
    t.doors = doors
        .iter()
        .filter(|d| (d.center.y - room.floor).abs() < 0.3 && inside(d.center))
        .map(|d| DoorSpec {
            center: d.center - lift,
            ..*d
        })
        .collect();
    t.blocks = blocks
        .iter()
        .filter(|k| {
            k.solid
                && k.aabb.max.y > room.floor + 0.3
                && k.aabb.min.y < room.floor + 2.0
                && k.aabb.min.x < near.x1
                && k.aabb.max.x > near.x0
                && k.aabb.min.z < near.z1
                && k.aabb.max.z > near.z0
        })
        .map(|k| Block {
            aabb: Aabb::new(k.aabb.min - lift, k.aabb.max - lift),
            ..*k
        })
        .collect();
    t.keep_clear = keep_clear.iter().filter(|p| inside(**p)).copied().collect();
    let n0 = t.blocks.len();
    let mut rng = Rng::new(seed);
    let k = interior_kind(kind, &mut rng);
    interior::furnish(&mut t, room.r, k, &mut Household::full(), &mut rng);
    let blocks = t.blocks[n0..]
        .iter()
        .map(|k| Block {
            aabb: Aabb::new(k.aabb.min + lift, k.aabb.max + lift),
            ..*k
        })
        .collect();
    let decor = t
        .decor
        .into_iter()
        .filter(|(_, mb)| !mb.is_empty())
        .map(|((mat, _), mut mb)| {
            mb.translate(lift);
            (mat, mb)
        })
        .collect();
    let lamps = t
        .lamps
        .iter()
        .map(|l| Lamp {
            pos: l.pos + lift,
            ..*l
        })
        .collect();
    SwitchGroup {
        key: SwitchKey::Furnish {
            estate: 0,
            room: 0,
            kind,
        },
        blocks,
        invisible: Vec::new(),
        decor,
        lamps,
    }
}

// ---------------------------------------------------------------------------
// Стройплощадка
// ---------------------------------------------------------------------------

/// Высота уцелевшей кладки на пепелище в точке (обломки разной высоты).
fn ruin_height(p: Vec3, seed: f32) -> f32 {
    let n = (p.x * 0.83 + seed).sin() * (p.z * 0.71 - seed * 0.5).cos() * 0.55
        + (p.x * 2.1 + p.z * 1.7 + seed * 3.0).sin() * 0.22
        + (p.z * 0.37 + p.x * 0.11 + seed).sin() * 0.35;
    (1.05 + n).clamp(0.25, 2.3)
}

/// Где рубить кладку на стадии: высота над полом (или по пепелищу).
#[derive(Clone, Copy)]
enum Cut {
    Ruin(f32),
    Flat(f32),
    Full,
}

impl Cut {
    fn at(self, p: Vec3) -> f32 {
        match self {
            Cut::Ruin(seed) => ruin_height(p, seed),
            Cut::Flat(h) => h,
            Cut::Full => f32::INFINITY,
        }
    }
}

/// Нарезать готовый дом `house` (собран отдельно, пол на y = 0) по стадиям и записать группы стройплощадки `site`.
/// `outline` — участок под домом, `wing` — фасадное крыло, `side` — сторона фасада наружу, `h` — высота стен.
pub fn site_stages(
    b: &mut Builder,
    site: u16,
    house: &Builder,
    outline: Rect,
    front_out: Vec3,
    along_x: bool,
    h: f32,
    rng: &mut Rng,
) {
    let seed = site as f32 * 1.37 + 0.4;
    for stage in 0..=SITE_DONE {
        let cut = match stage {
            0 => Cut::Ruin(seed),
            1 => Cut::Flat(0.7),
            2 => Cut::Flat(2.1),
            _ => Cut::Full,
        };
        let mut g = SwitchGroup {
            key: SwitchKey::Site { site, stage },
            blocks: Vec::new(),
            invisible: Vec::new(),
            decor: Vec::new(),
            lamps: Vec::new(),
        };
        // Кладка (коробки).
        for k in &house.blocks {
            match cut {
                Cut::Full => g.blocks.push(Block {
                    mat: bare(k.mat, stage),
                    ..*k
                }),
                Cut::Flat(top) => {
                    if k.aabb.min.y < top - 0.02 {
                        g.blocks.push(Block {
                            aabb: Aabb::new(
                                k.aabb.min,
                                Vec3::new(k.aabb.max.x, k.aabb.max.y.min(top), k.aabb.max.z),
                            ),
                            mat: bare(k.mat, stage),
                            ..*k
                        });
                    }
                }
                Cut::Ruin(_) => {
                    if !k.solid {
                        if k.aabb.max.y < 0.3 {
                            g.blocks.push(*k);
                        }
                        continue;
                    }
                    // Длинную стену рубим кусками разной высоты.
                    let size = k.aabb.max - k.aabb.min;
                    let along = if size.x > size.z { Vec3::X } else { Vec3::Z };
                    let len = size.dot(along);
                    let n = (len / 0.7).ceil().max(1.0) as usize;
                    for i in 0..n {
                        let a = k.aabb.min + along * (len * i as f32 / n as f32);
                        let c = k.aabb.min + along * (len * (i + 1) as f32 / n as f32);
                        let mid = (a + c) / 2.0 + (Vec3::ONE - along) * size / 2.0;
                        let top = cut.at(mid);
                        if k.aabb.min.y >= top {
                            continue;
                        }
                        let min = Vec3::new(
                            if along == Vec3::X { a.x } else { k.aabb.min.x },
                            k.aabb.min.y,
                            if along == Vec3::Z { a.z } else { k.aabb.min.z },
                        );
                        let max = Vec3::new(
                            if along == Vec3::X { c.x } else { k.aabb.max.x },
                            k.aabb.max.y.min(top),
                            if along == Vec3::Z { c.z } else { k.aabb.max.z },
                        );
                        g.blocks.push(Block {
                            aabb: Aabb::new(min, max),
                            mat: bare(k.mat, stage),
                            ..*k
                        });
                    }
                }
            }
        }
        if matches!(cut, Cut::Full) {
            g.invisible.extend(house.invisible.iter().copied());
        }
        // Меши: черепица — только на готовом доме; остальное — по высоте стадии.
        for ((mat, _), mb) in &house.decor {
            if mb.is_empty() {
                continue;
            }
            if *mat == Mat::Tile && stage < SITE_DONE {
                // Под кровлю: стропила и обрешётка по скатам будущей крыши.
                if stage == SITE_DONE - 1 {
                    g.decor.push((Mat::Wood, roof_frame(mb)));
                }
                continue;
            }
            let part = match cut {
                Cut::Full => mb.clone(),
                Cut::Flat(top) => mb.clipped(|p| top - p.y),
                Cut::Ruin(_) => {
                    // На пепелище дерево сгорело: от него — только то, что у самой земли.
                    if *mat == Mat::Wood {
                        mb.clipped(|p| 0.25 - p.y)
                    } else {
                        mb.clipped(|p| cut.at(p) - p.y)
                    }
                }
            };
            if !part.is_empty() {
                g.decor.push((bare(*mat, stage), part));
            }
        }
        if stage == SITE_DONE {
            g.lamps.extend(house.lamps.iter().copied());
        }
        let mut extra = Builder::new();
        extra.chunk = b.chunk;
        match stage {
            0 => rubble(&mut extra, outline, rng),
            1 => supplies(&mut extra, outline, front_out, along_x, false, rng),
            2 => {
                supplies(&mut extra, outline, front_out, along_x, false, rng);
                scaffold(&mut extra, outline, front_out, along_x, 2.1 + 1.2);
            }
            3 => {
                supplies(&mut extra, outline, front_out, along_x, true, rng);
                scaffold(&mut extra, outline, front_out, along_x, h + 0.6);
            }
            _ => {}
        }
        g.blocks.extend(extra.blocks);
        g.invisible.extend(extra.invisible);
        for ((mat, _), mb) in extra.decor {
            if !mb.is_empty() {
                g.decor.push((mat, mb));
            }
        }
        b.switches.push(g);
    }
}

/// Стропила и обрешётка: полосы по скатам крыши (решёткой по обеим осям), чуть ниже черепицы.
fn roof_frame(tiles: &crate::level::geom::MeshBuilder) -> crate::level::geom::MeshBuilder {
    let mut out = crate::level::geom::MeshBuilder::default();
    let Some((lo, hi)) = tiles.bounds() else {
        return out;
    };
    for (axis, step, half) in [(Vec3::X, 0.62, 0.06), (Vec3::Z, 0.62, 0.06)] {
        let (a, b) = (lo.dot(axis), hi.dot(axis));
        let mut v = a + 0.15;
        while v < b {
            let strip = tiles
                .clipped(|p| p.dot(axis) - (v - half))
                .clipped(|p| (v + half) - p.dot(axis));
            out.append(strip);
            v += step;
        }
    }
    out.shifted(Vec3::Y * -0.05)
}

/// Пепелище: кучи обломков и золы в комнатах, обгорелые балки, сорная трава.
/// Штукатурка — последнее дело: на пепелище она обгорела и осыпалась, на стройке её ещё нет,
/// видна бутовая кладка.
fn bare(mat: Mat, stage: u8) -> Mat {
    if stage < SITE_DONE && mat == Mat::Plaster {
        Mat::Rubble
    } else {
        mat
    }
}

fn rubble(b: &mut Builder, r: Rect, rng: &mut Rng) {
    let inner = r.inset(1.2);
    for _ in 0..7 {
        let p = Vec3::new(
            rng.range(inner.x0, inner.x1),
            0.0,
            rng.range(inner.z0, inner.z1),
        );
        let rad = rng.range(0.8, 1.7);
        let hgt = rng.range(0.35, 0.8);
        b.mesh(Mat::Rubble).lathe(
            p,
            &[
                (rad, 0.0),
                (rad * 0.75, hgt * 0.5),
                (rad * 0.3, hgt),
                (0.0, hgt * 1.05),
            ],
            9,
        );
        b.invisible.push(Aabb::new(
            Vec3::new(p.x - rad * 0.5, 0.0, p.z - rad * 0.5),
            Vec3::new(p.x + rad * 0.5, hgt * 0.6, p.z + rad * 0.5),
        ));
        // Зола и черепки вокруг.
        b.mesh(Mat::DarkStone)
            .disc(p + Vec3::Y * 0.02, rad * 1.25, 10, true);
    }
    // Обгорелые балки кровли — одним концом на обломке стены.
    for _ in 0..6 {
        let a = Vec3::new(
            rng.range(inner.x0, inner.x1),
            rng.range(0.6, 1.3),
            rng.range(inner.z0, inner.z1),
        );
        let dir = Vec3::new(rng.range(-1.0, 1.0), 0.0, rng.range(-1.0, 1.0)).normalize_or(Vec3::X);
        let c = a + dir * rng.range(2.0, 3.6) - Vec3::Y * a.y + Vec3::Y * 0.1;
        b.mesh(Mat::DarkStone).beam(a, c, 0.09, 0.11, Vec3::Y);
    }
    // Сорняки и битая черепица.
    for _ in 0..14 {
        let p = Vec3::new(
            rng.range(r.x0 + 0.5, r.x1 - 0.5),
            0.0,
            rng.range(r.z0 + 0.5, r.z1 - 0.5),
        );
        b.mesh(Mat::HerbGreen)
            .lathe(p, &[(0.0, 0.0), (0.28, 0.12), (0.18, 0.35), (0.0, 0.42)], 6);
    }
    for _ in 0..10 {
        let p = Vec3::new(
            rng.range(inner.x0, inner.x1),
            0.02,
            rng.range(inner.z0, inner.z1),
        );
        let s = rng.range(0.15, 0.3);
        b.mesh(Mat::Tile)
            .obox(p, Vec3::new(s, 0.02, s * 0.7), rng.range(0.0, 3.0));
    }
}

/// Припасы стройки у фасада: штабель тёсаного камня, бревна и доски, яма с гашёной известью,
/// корзины; на стадии кровли — штабеля черепицы.
fn supplies(b: &mut Builder, r: Rect, out: Vec3, along_x: bool, tiles: bool, rng: &mut Rng) {
    let axis = if along_x { Vec3::X } else { Vec3::Z };
    // Во дворе, ближе к задней стене (на улице не загораживаем проход).
    let back = -out;
    let c = r.center(0.0) + back * (if along_x { r.d() } else { r.w() } * 0.22);
    // Камень.
    let stones = c - axis * 2.6;
    for layer in 0..3 {
        for k in 0..3 {
            let p = stones + axis * (k as f32 * 0.62 - 0.62) + Vec3::Y * (layer as f32 * 0.32);
            if layer == 2 && k == 2 {
                continue;
            }
            let half = Vec3::new(0.29, 0.15, 0.29);
            b.mesh(Mat::Limestone)
                .cuboid(p - half + Vec3::Y * 0.15, p + half + Vec3::Y * 0.15);
        }
    }
    let half = axis * 0.95 + (Vec3::ONE - axis - Vec3::Y) * 0.32;
    b.invisible
        .push(Aabb::new(stones - half, stones + half + Vec3::Y * 0.95));
    // Бревна и доски.
    let wood = c + axis * 1.4;
    let across = Vec3::new(axis.z, 0.0, axis.x);
    for k in 0..5 {
        let y = 0.12 + (k / 3) as f32 * 0.22;
        let off = across * ((k % 3) as f32 * 0.24 - 0.24) + Vec3::Y * y;
        b.mesh(Mat::Wood).beam(
            wood + off - axis * 1.6,
            wood + off + axis * 1.6,
            0.11,
            0.11,
            Vec3::Y,
        );
    }
    let half = axis * 1.6 + across * 0.4;
    b.invisible
        .push(Aabb::new(wood - half, wood + half + Vec3::Y * 0.5));
    // Известь в яме, рядом корзины.
    let lime = c + across * 2.2;
    b.mesh(Mat::Plaster)
        .disc(lime + Vec3::Y * 0.04, 0.9, 14, true);
    b.mesh(Mat::Wood).lathe(
        lime,
        &[(0.95, 0.0), (0.95, 0.16), (0.9, 0.16), (0.9, 0.02)],
        14,
    );
    for k in 0..2 {
        let p = lime + axis * (1.4 + k as f32 * 0.6) + Vec3::Y * 0.0;
        b.mesh(Mat::Thatch).lathe(
            p,
            &[
                (0.0, 0.0),
                (0.2, 0.0),
                (0.26, 0.4),
                (0.22, 0.42),
                (0.0, 0.42),
            ],
            8,
        );
    }
    if tiles {
        let t0 = c - across * 2.2;
        for k in 0..3 {
            let p = t0 + axis * (k as f32 * 0.7 - 0.7);
            b.mesh(Mat::Tile).cuboid(
                p - Vec3::new(0.28, 0.0, 0.28),
                p + Vec3::new(0.28, 0.55 + rng.range(0.0, 0.2), 0.28),
            );
        }
        b.invisible.push(Aabb::new(
            t0 - axis * 1.1 - across * 0.3,
            t0 + axis * 1.1 + across * 0.3 + Vec3::Y * 0.7,
        ));
    }
}

/// Строительные леса вдоль фасада: стойки, ригели, настил из досок и приставная лестница.
fn scaffold(b: &mut Builder, r: Rect, out: Vec3, along_x: bool, top: f32) {
    let axis = if along_x { Vec3::X } else { Vec3::Z };
    let (len, depth) = if along_x {
        (r.w(), r.d())
    } else {
        (r.d(), r.w())
    };
    // Фасадная линия.
    let face = r.center(0.0) + out * (depth / 2.0);
    let start = face - axis * (len / 2.0 - 0.4);
    let n = (len / 2.3).floor() as usize;
    let m = b.mesh(Mat::Wood);
    let levels: Vec<f32> = [1.6, 3.1, 4.6, 6.1]
        .into_iter()
        .filter(|y| *y < top)
        .collect();
    for k in 0..=n {
        let p = start + axis * (k as f32 * (len - 0.8) / n as f32);
        for d in [0.45, 1.45] {
            let foot = p + out * d;
            m.beam(foot, foot + Vec3::Y * top, 0.05, 0.05, Vec3::X);
        }
        for &y in &levels {
            m.beam(
                p + out * 0.45 + Vec3::Y * y,
                p + out * 1.45 + Vec3::Y * y,
                0.04,
                0.04,
                Vec3::Y,
            );
        }
    }
    for &y in &levels {
        for d in [0.45, 1.45] {
            m.beam(
                start + out * d + Vec3::Y * y,
                start + axis * (len - 0.8) + out * d + Vec3::Y * y,
                0.04,
                0.04,
                Vec3::Y,
            );
        }
        // Настил.
        let a = start + out * 0.5 + Vec3::Y * (y + 0.04);
        let c = start + axis * (len - 0.8) + out * 1.4 + Vec3::Y * (y + 0.08);
        m.cuboid(a.min(c), a.max(c));
    }
    // Лестница с торца.
    let foot = start + out * 2.2 - axis * 0.3;
    let topp = start + out * 1.5 + Vec3::Y * levels.last().copied().unwrap_or(1.6) - axis * 0.3;
    for s in [-0.22, 0.22] {
        m.beam(
            foot + axis * s,
            topp + axis * s + Vec3::Y * 0.8,
            0.03,
            0.03,
            axis,
        );
    }
    let rungs = 8;
    for k in 1..rungs {
        let t = k as f32 / rungs as f32;
        let p = foot.lerp(topp, t);
        m.beam(p - axis * 0.22, p + axis * 0.22, 0.02, 0.02, Vec3::Y);
    }
}
