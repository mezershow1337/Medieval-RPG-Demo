//! Клиент горного дела: что видно у рудника и в стане углежогов.
//!
//! - Места добычи (руда, болото, делянка, глина, песок, камень): навести взгляд и [E].
//! - Угольные кучи и обжиговые кострища: поленья, земля, отдушины, дым по стадии (белый —
//!   выходит вода, жёлтый — обугливание, голубой — готово, пламя — прогар), отсвет, пламя.
//!   Клавиши [E] / [R] / [Z] — по подсказке; что значит дым, подсказка говорит только знающему.
//! - Рудокопы, углежоги, дровосек и копатели болотной руды живут по часам мира: днём бьют
//!   забой, крутят ворот, носят корзины, ворошат обжиг, колют дрова; в полдень едят у огня,
//!   ночью спят в землянке (а углежоги по очереди сторожат кучу — она горит и ночью).

use crate::graphics::{Billboard, FlameMaterial, SurfaceMaterial, surface_material};
use crate::level::Mat;
use crate::inventory_ui::UiFonts;
use crate::level::{MeshBuilder, Role, WorkSpots, mine_sites, mine_work_spots, pile_spots};
use crate::loot_client::LootState;
use crate::lore::{Lore, Technique};
use crate::lore_client::{LoreView, MyKnowledge};
use crate::lore_net::LoreClientMessage;
use crate::mining::*;
use crate::mining_net::*;
use crate::player::PlayerCamera;
use crate::protocol::decode;
use crate::rig::{NeedsRig, PoseInput, RigPose};
use crate::smoke::{SmokeMaterial, SmokeUniform, column_mesh};
use crate::world_clock::WorldClock;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy_renet::{RenetClient, client_connected};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

pub struct MiningClientPlugin;

impl Plugin for MiningClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MineView::default())
            .add_systems(Startup, (setup_mine_assets, setup_mine_ui.after(crate::inventory_ui::setup_fonts)))
            .add_systems(Startup, (spawn_piles, spawn_workers).after(setup_mine_assets))
            .add_systems(Update, (receive_mine, mine_input).chain().run_if(client_connected))
            .add_systems(Update, (aim_mine, update_mine_prompt, update_piles, drive_workers, update_worker_props).chain());
    }
}

// ---------------------------------------------------------------------------
// Что известно о кучах и местах
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MineTarget {
    Site(u32),
    Pile(u32),
}

#[derive(Resource, Default)]
pub struct MineView {
    /// Сколько осталось в местах добычи.
    pub sites: HashMap<u32, u8>,
    pub piles: HashMap<u32, PileView>,
    pub target: Option<MineTarget>,
}

fn receive_mine(mut client: ResMut<RenetClient>, mut view: ResMut<MineView>) {
    while let Some(bytes) = client.receive_message(CH_MINE) {
        let Some(msg) = decode::<MineServerMessage>(&bytes) else { continue };
        match msg {
            MineServerMessage::Sites(v) => view.sites = v.into_iter().collect(),
            MineServerMessage::SiteCharges { id, charges } => {
                view.sites.insert(id, charges);
            }
            MineServerMessage::Piles(v) => view.piles = v.into_iter().map(|p| (p.id, p)).collect(),
            MineServerMessage::Pile(p) => {
                view.piles.insert(p.id, p);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Наведение, клавиши, подсказка
// ---------------------------------------------------------------------------

fn aim_mine(mut view: ResMut<MineView>, loot: Res<LootState>, lore: Res<LoreView>, camera: Query<&GlobalTransform, With<PlayerCamera>>) {
    let target = (|| {
        if loot.ui_open || loot.target.is_some() || lore.target.is_some() {
            return None;
        }
        let cam = camera.single().ok()?;
        let (origin, dir) = (cam.translation(), cam.forward().as_vec3());
        let mut best: Option<(f32, MineTarget)> = None;
        let mut zone: Option<(f32, MineTarget)> = None;
        let test = |center: Vec3, radius: f32, reach: f32, t: MineTarget, best: &mut Option<(f32, MineTarget)>| {
            let to = center - origin;
            let along = to.dot(dir);
            if !(0.0..=reach).contains(&along) {
                return;
            }
            let miss = (to - dir * along).length();
            if miss <= radius && best.is_none_or(|(s, _)| along + miss < s) {
                *best = Some((along + miss, t));
            }
        };
        for p in view.piles.values() {
            test(Vec3::from(p.pos) + Vec3::Y * 0.6, 1.5, MINE_REACH + 2.0, MineTarget::Pile(p.id), &mut best);
        }
        for s in mine_sites() {
            test(s.pos + Vec3::Y * 0.7, s.radius.min(1.3), MINE_REACH + 1.5, MineTarget::Site(s.id), &mut best);
            // Широкое место (болото, песок, делянка): работать, стоя в нём.
            let d = s.pos - origin;
            let flat = Vec2::new(d.x, d.z).length();
            if s.radius > 1.5 && flat <= s.radius + 0.4 && d.y.abs() < 3.0 && zone.is_none_or(|(f, _)| flat < f) {
                zone = Some((flat, MineTarget::Site(s.id)));
            }
        }
        best.or(zone).map(|(_, t)| t)
    })();
    if view.target != target {
        view.target = target;
    }
}

/// Что можно сделать у кучи: клавиша, подпись, действие.
pub fn pile_options(v: &PileView) -> Vec<(KeyCode, String, MineAct)> {
    let p = &v.pile;
    let id = v.id;
    let mut o: Vec<(KeyCode, String, MineAct)> = Vec::new();
    let (lo, hi) = p.wood_limits();
    match (p.kind, p.stage) {
        (PileKind::Clamp, Stage::Stack) => {
            o.push((KeyCode::KeyE, format!("Сложить поленья ({} из {hi})", p.wood), MineAct::StackWood { pile: id }));
            if p.wood >= lo {
                o.push((KeyCode::KeyR, "Укрыть листвой, дёрном и землёй (заступ)".into(), MineAct::Cover { pile: id }));
                o.push((KeyCode::KeyZ, "Поджечь".into(), MineAct::Light { pile: id }));
            }
        }
        (PileKind::Clamp, Stage::Cover) => {
            o.push((KeyCode::KeyE, format!("Укрывать дальше (укрыто {:.0}%)", p.cover * 100.0), MineAct::Cover { pile: id }));
            o.push((KeyCode::KeyZ, "Поджечь".into(), MineAct::Light { pile: id }));
        }
        (PileKind::Clamp, Stage::Ready) => o.push((KeyCode::KeyE, "Поджечь через кол".into(), MineAct::Light { pile: id })),
        (PileKind::Clamp, Stage::Burning) if p.out => {
            o.push((KeyCode::KeyE, "Разобрать".into(), MineAct::Rake { pile: id }));
            o.push((KeyCode::KeyZ, "Поджечь снова".into(), MineAct::Light { pile: id }));
        }
        (PileKind::Clamp, Stage::Burning) => {
            if p.cracks > 0.05 {
                o.push((KeyCode::KeyE, "Засыпать прогар землёй".into(), MineAct::Patch { pile: id }));
            } else {
                o.push((KeyCode::KeyE, "Заглушить кучу (закрыть всё, засыпать)".into(), MineAct::Smother { pile: id }));
            }
            o.push((KeyCode::KeyR, format!("Открыть отдушину ({} из {VENTS})", p.vents), MineAct::Vent { pile: id, open: true }));
            o.push((KeyCode::KeyZ, "Прикрыть отдушину".into(), MineAct::Vent { pile: id, open: false }));
        }
        (PileKind::Roast, Stage::Stack) => {
            o.push((KeyCode::KeyE, format!("Положить дрова ({} из {hi})", p.wood), MineAct::StackWood { pile: id }));
            o.push((KeyCode::KeyR, format!("Насыпать руду ({} из {ROAST_ORE})", p.ore + p.bog), MineAct::LoadOre { pile: id }));
            if p.wood >= lo && p.ore + p.bog > 0 {
                o.push((KeyCode::KeyZ, "Поджечь".into(), MineAct::Light { pile: id }));
            }
        }
        (PileKind::Roast, Stage::Burning) if p.out => o.push((KeyCode::KeyE, "Выгрести руду".into(), MineAct::Rake { pile: id })),
        (PileKind::Roast, Stage::Burning) => {
            o.push((KeyCode::KeyE, "Ворошить руду".into(), MineAct::Stir { pile: id }));
            o.push((KeyCode::KeyR, "Подбросить полено".into(), MineAct::Feed { pile: id }));
        }
        (_, Stage::Cooling) => {}
        (PileKind::Clamp, Stage::Done) => o.push((KeyCode::KeyE, "Разобрать кучу, выбрать уголь".into(), MineAct::Rake { pile: id })),
        (PileKind::Roast, Stage::Done) => o.push((KeyCode::KeyE, "Выгрести руду из золы".into(), MineAct::Rake { pile: id })),
        (PileKind::Roast, _) => {}
    }
    o
}

fn key_label(k: KeyCode) -> &'static str {
    match k {
        KeyCode::KeyE => "E",
        KeyCode::KeyR => "R",
        KeyCode::KeyZ => "Z",
        _ => "?",
    }
}

fn mine_input(keys: Res<ButtonInput<KeyCode>>, mut client: ResMut<RenetClient>, view: Res<MineView>, loot: Res<LootState>) {
    if loot.ui_open {
        return;
    }
    let send = |client: &mut RenetClient, act: MineAct| crate::lore_client::send(client, &LoreClientMessage::Mine(act));
    match view.target {
        Some(MineTarget::Site(site)) if keys.just_pressed(KeyCode::KeyE) => send(&mut client, MineAct::Dig { site }),
        Some(MineTarget::Pile(id)) => {
            let Some(v) = view.piles.get(&id) else { return };
            for (k, _, act) in pile_options(v) {
                if keys.just_pressed(k) {
                    send(&mut client, act);
                    break;
                }
            }
        }
        _ => {}
    }
}

#[derive(Component)]
struct MinePrompt;

fn setup_mine_ui(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), top: percent(56), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            crate::ui_theme::theme().plaque(p).with_children(|p| {
                p.spawn((
                    MinePrompt,
                    Text::new(""),
                    TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(19.0), ..default() },
                    TextColor(Color::WHITE),
                    TextLayout::justify(Justify::Center),
                    TextShadow::default(),
                    crate::ui_theme::Plaque,
                ));
            });
        });
}

/// Подсказка у места добычи.
pub fn site_line(kind: SiteKind, charges: u8, k: &crate::lore::Knowledge) -> String {
    let info = kind.info();
    let name = if kind.ore() && !k.knows(Lore::Technique(Technique::Prospecting)) { info.plain } else { info.name };
    if charges == 0 {
        return match kind {
            SiteKind::Shaft => format!("{name}\nБадьи пока не готовы — в шурфе рубят"),
            _ => format!("{name}\nВсё выбрано, нужно подождать"),
        };
    }
    let what = match kind {
        SiteKind::Shaft => "Крутить ворот (поденщина)".to_string(),
        SiteKind::Adit if !k.knows(Lore::Technique(Technique::Mining)) => "Бить забой (без науки десятник не пустит)".to_string(),
        _ => info.verb.to_string(),
    };
    let tool = info.tool.map(|t| format!(" — нужен {}", t.name())).unwrap_or_default();
    format!("{name}\n[E] {what}{tool}")
}

/// Подсказка у кучи: что видно (дым, огонь) и что можно сделать.
pub fn pile_line(v: &PileView, k: &crate::lore::Knowledge) -> String {
    let p = &v.pile;
    let coal = k.knows(Lore::Technique(Technique::Charcoal));
    let roast = k.knows(Lore::Technique(Technique::Roasting));
    let title = match (p.kind, p.stage) {
        (PileKind::Clamp, Stage::Stack) if p.wood == 0 => "Угольная площадка",
        (PileKind::Clamp, Stage::Stack) => "Угольная куча (кладут поленья)",
        (PileKind::Clamp, Stage::Cover) => "Угольная куча (укрывают)",
        (PileKind::Clamp, Stage::Ready) => "Угольная куча — укрыта, готова к поджогу",
        (PileKind::Clamp, Stage::Burning) => "Угольная куча — тлеет",
        (PileKind::Clamp, Stage::Cooling) => "Угольная куча — заглушена, остывает",
        (PileKind::Clamp, Stage::Done) => "Угольная куча — остыла",
        (PileKind::Roast, Stage::Burning) => "Обжиговое кострище — горит",
        (PileKind::Roast, Stage::Cooling) => "Обжиговое кострище — руда остывает",
        (PileKind::Roast, Stage::Done) => "Обжиговое кострище — остыло",
        (PileKind::Roast, _) => "Обжиговое кострище",
    };
    let mut out = vec![title.to_string()];
    let reading = match p.kind {
        PileKind::Clamp => clamp_reading(p, coal),
        PileKind::Roast => roast_reading(p, roast),
    };
    if !reading.is_empty() {
        out.push(reading);
    }
    if p.kind == PileKind::Clamp && coal && p.stage == Stage::Stack && p.wood > 0 && p.wood < CLAMP_WOOD.1 / 2 {
        out.push("Мала куча: чем больше, тем ровнее тлеет".into());
    }
    if p.kind == PileKind::Clamp && coal && matches!(p.stage, Stage::Stack | Stage::Cover) && p.wood >= CLAMP_WOOD.0 {
        out.push("Неукрытая куча сгорит костром — сперва укройте".into());
    }
    for (key, label, _) in pile_options(v) {
        out.push(format!("[{}] {label}", key_label(key)));
    }
    out.join("\n")
}

fn update_mine_prompt(view: Res<MineView>, knowledge: Res<MyKnowledge>, mut text: Single<&mut Text, With<MinePrompt>>) {
    if !view.is_changed() && !knowledge.is_changed() {
        return;
    }
    let line = match view.target {
        Some(MineTarget::Site(id)) => mine_sites().iter().find(|s| s.id == id).map(|s| {
            let charges = view.sites.get(&id).copied().unwrap_or(s.kind.info().charges);
            site_line(s.kind, charges, &knowledge.0)
        }),
        Some(MineTarget::Pile(id)) => view.piles.get(&id).map(|v| pile_line(v, &knowledge.0)),
        None => None,
    }
    .unwrap_or_default();
    if text.0 != line {
        text.0 = line;
    }
}

// ---------------------------------------------------------------------------
// Кучи: модели
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct MineAssets {
    log: Handle<Mesh>,
    ball: Handle<Mesh>,
    /// Купол кучи (бугристый), навал кусков, кольца поленьев угольной кучи.
    dome: Handle<Mesh>,
    heap: Handle<Mesh>,
    rings: Vec<Handle<Mesh>>,
    flame_mesh: Handle<Mesh>,
    wood: Handle<SurfaceMaterial>,
    charred: Handle<SurfaceMaterial>,
    earth: Handle<SurfaceMaterial>,
    sod: Handle<SurfaceMaterial>,
    coal: Handle<SurfaceMaterial>,
    ore: Handle<SurfaceMaterial>,
    roasted: Handle<SurfaceMaterial>,
    soot: Handle<SurfaceMaterial>,
    ember: Handle<StandardMaterial>,
    flame: Handle<FlameMaterial>,
    /// Предметы в руках работников.
    tools: HashMap<Tool2, Vec<(Handle<Mesh>, Handle<SurfaceMaterial>, Transform)>>,
}

/// Что держит работник.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Tool2 {
    Pick,
    Spade,
    Axe,
    Pole,
    Basket,
    Log,
    Cup,
}

fn setup_mine_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut surfaces: ResMut<Assets<SurfaceMaterial>>,
    mut flames: ResMut<Assets<FlameMaterial>>,
) {
    // Всё — на фотоматериалах уровня (трипланарно, с рельефом); цвет — перекраска фото.
    let mut surf = |m: Mat, tint: Option<(Color, f32)>| {
        let mut s = surface_material(m);
        if let Some((c, k)) = tint {
            s.base.base_color = c;
            s.extension.photo_mean.w = k;
        }
        surfaces.add(s)
    };
    let wood = surf(Mat::Wood, Some((Color::srgb(0.46, 0.34, 0.22), 0.7)));
    let bark = surf(Mat::Wood, Some((Color::srgb(0.27, 0.21, 0.15), 0.9)));
    let charred = surf(Mat::Wood, Some((Color::srgb(0.05, 0.045, 0.04), 1.0)));
    let earth = surf(Mat::Earth, Some((Color::srgb(0.3, 0.24, 0.17), 0.85)));
    let sod = surf(Mat::Grass, None);
    let coal = surf(Mat::Bedrock, Some((Color::srgb(0.03, 0.03, 0.035), 1.0)));
    let ore = surf(Mat::Limonite, None);
    let roasted = surf(Mat::Limonite, Some((Color::srgb(0.16, 0.1, 0.08), 0.95)));
    let soot = surf(Mat::Soot, None);
    let iron = surf(Mat::Iron, None);
    let wicker = surf(Mat::Wood, Some((Color::srgb(0.62, 0.5, 0.3), 0.85)));
    let clay = surf(Mat::Plaster, Some((Color::srgb(0.6, 0.36, 0.22), 1.0)));
    let ember = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.35, 0.08), emissive: LinearRgba::rgb(9.0, 2.2, 0.3), ..default() });
    let flame = flames.add(FlameMaterial::default());
    let mut m = |mesh: Mesh| meshes.add(mesh);
    let at = |y: f32| Transform::from_xyz(0.0, y, 0.0);
    let mut tools = HashMap::new();
    // Ось рукояти — +Y, хват — начало координат; рабочий конец ниже хвата (как у мотыги).
    tools.insert(
        Tool2::Pick,
        vec![
            (m(Cylinder::new(0.018, 0.85).into()), wood.clone(), at(-0.28)),
            (m(Cuboid::new(0.04, 0.045, 0.52).into()), iron.clone(), Transform::from_xyz(0.0, -0.68, 0.0)),
        ],
    );
    tools.insert(
        Tool2::Spade,
        vec![
            (m(Cylinder::new(0.018, 1.0).into()), wood.clone(), at(-0.3)),
            (m(Cuboid::new(0.2, 0.3, 0.025).into()), wood.clone(), at(-0.92)),
            (m(Cuboid::new(0.21, 0.06, 0.03).into()), iron.clone(), at(-1.08)),
        ],
    );
    tools.insert(
        Tool2::Axe,
        vec![
            (m(Cylinder::new(0.017, 0.75).into()), wood.clone(), at(-0.22)),
            (m(Cuboid::new(0.03, 0.13, 0.17).into()), iron.clone(), Transform::from_xyz(0.0, -0.55, -0.07)),
        ],
    );
    tools.insert(Tool2::Pole, vec![(m(Cylinder::new(0.02, 2.4).into()), bark.clone(), at(-0.9))]);
    tools.insert(
        Tool2::Basket,
        vec![
            (m(Cylinder::new(0.2, 0.26).into()), wicker.clone(), at(-0.2)),
            (m(Sphere::new(0.16).into()), ore.clone(), Transform::from_xyz(0.0, -0.1, 0.0).with_scale(Vec3::new(1.0, 0.45, 1.0))),
        ],
    );
    tools.insert(Tool2::Log, vec![(m(Cylinder::new(0.08, 0.9).into()), bark.clone(), Transform::from_rotation(Quat::from_rotation_z(PI / 2.0)))]);
    tools.insert(Tool2::Cup, vec![(m(Cylinder::new(0.045, 0.08).into()), clay.clone(), at(0.03))]);
    commands.insert_resource(MineAssets {
        log: m(Cylinder::new(0.09, 1.0).into()),
        ball: m(Sphere::new(1.0).mesh().uv(24, 14)),
        dome: m(dome_mesh(11)),
        heap: m(heap_mesh(5)),
        rings: (0..CLAMP_RINGS).map(|k| m(clamp_ring(k))).collect(),
        flame_mesh: m(Rectangle::new(0.42, 0.75).into()),
        wood,
        charred,
        earth,
        sod,
        coal,
        ore,
        roasted,
        soot,
        ember,
        flame,
        tools,
    });
}

/// Колец поленьев в угольной куче (от кола наружу).
const CLAMP_RINGS: usize = 5;

/// Полено: восьмигранный цилиндр от `a` до `b` с торцом сверху.
fn add_log(mb: &mut MeshBuilder, a: Vec3, b: Vec3, r: f32) {
    let d = (b - a).normalize();
    let u = d.any_orthonormal_vector();
    let v = d.cross(u);
    let n = 8;
    let base = (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32 * TAU;
            let o = u * t.cos() + v * t.sin();
            let len = (b - a).length();
            (mb.vert(a + o * r, o, Vec2::new(k as f32 / n as f32, 0.0)), mb.vert(b + o * r, o, Vec2::new(k as f32 / n as f32, len)))
        })
        .collect::<Vec<_>>();
    for k in 0..n {
        let (a0, b0) = base[k];
        let (a1, b1) = base[k + 1];
        mb.index_tri(a0, a1, b1);
        mb.index_tri(a0, b1, b0);
    }
    let c = mb.vert(b, d, Vec2::splat(0.5));
    let cap: Vec<u32> = (0..n).map(|k| {
        let t = k as f32 / n as f32 * TAU;
        mb.vert(b + (u * t.cos() + v * t.sin()) * r, d, Vec2::new(0.5 + 0.5 * t.cos(), 0.5 + 0.5 * t.sin()))
    }).collect();
    for k in 0..n {
        mb.index_tri(c, cap[k], cap[(k + 1) % n]);
    }
}

/// Кольцо поленьев угольной кучи: стоймя, плотно, с наклоном к колу; середина выше.
fn clamp_ring(k: usize) -> Mesh {
    let mut mb = MeshBuilder::default();
    let r = 0.22 + k as f32 * 0.33;
    let n = ((TAU * r) / 0.2).ceil().max(5.0) as usize;
    for i in 0..n {
        let a = (i as f32 + 0.5 * (k % 2) as f32) / n as f32 * TAU;
        let h = 1.85 - 0.22 * k as f32 - 0.05 * ((i * 7 + k * 3) % 4) as f32;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        let foot = dir * r;
        let top = dir * (r - 0.1 - 0.05 * k as f32) + Vec3::Y * h;
        add_log(&mut mb, foot, top, 0.085 + 0.012 * ((i + k) % 3) as f32);
    }
    // Сверху на внутренних кольцах — уложенные поперёк короткие поленья (кровля кучи).
    if k == 0 {
        for i in 0..6 {
            let a = i as f32 / 6.0 * PI;
            let d = Vec3::new(a.cos(), 0.0, a.sin()) * 0.55;
            add_log(&mut mb, -d + Vec3::Y * 1.9, d + Vec3::Y * 1.86, 0.08);
        }
    }
    mb.build()
}

/// Плавный шум для бугров (детерминированный).
fn bump(p: Vec3) -> f32 {
    ((p.x * 3.1 + p.z * 1.7).sin() * (p.z * 2.3 - p.y * 1.3).cos() + (p.x * 7.3 - p.z * 5.9 + p.y * 4.1).sin() * 0.45) * 0.5
}

/// Бугристый купол радиуса 1, высотой 1 (земляной колпак угольной кучи).
fn dome_mesh(lat: usize) -> Mesh {
    let mut mb = MeshBuilder::default();
    let lon = lat * 3;
    let mut grid = vec![vec![0u32; lon + 1]; lat + 1];
    for i in 0..=lat {
        let phi = i as f32 / lat as f32 * PI / 2.0;
        for j in 0..=lon {
            let th = j as f32 / lon as f32 * TAU;
            let n = Vec3::new(phi.cos() * th.cos(), phi.sin(), phi.cos() * th.sin());
            // Комья дёрна и земли; к подошве — шире, растекается.
            let k = 1.0 + 0.07 * bump(n * 2.0) * (1.0 - (i as f32 / lat as f32).powi(3)) + if i == 0 { 0.08 } else { 0.0 };
            grid[i][j] = mb.vert(n * k, n, Vec2::new(j as f32 / lon as f32 * 6.0, i as f32 / lat as f32 * 2.0));
        }
    }
    for i in 0..lat {
        for j in 0..lon {
            let (a, b, c, d) = (grid[i][j], grid[i][j + 1], grid[i + 1][j + 1], grid[i + 1][j]);
            mb.index_tri(a, c, b);
            mb.index_tri(a, d, c);
        }
    }
    mb.build()
}

/// Навал угловатых кусков (руда, уголь) в радиусе 1, высотой ~0.7.
fn heap_mesh(seed: u32) -> Mesh {
    let mut mb = MeshBuilder::default();
    let mut rng = crate::items::Rng::new(seed as u64);
    for k in 0..70 {
        let a = rng.f32() * TAU;
        let d = rng.f32().sqrt();
        let top = 0.7 * (1.0 - d.powf(1.5)).max(0.0);
        let s = (0.07 + rng.f32() * 0.09) * if k % 7 == 0 { 1.5 } else { 1.0 };
        let c = Vec3::new(a.cos() * d, top * (0.5 + 0.45 * rng.f32()), a.sin() * d);
        mb.obox(c, Vec3::new(s, s * (0.55 + 0.3 * rng.f32()), s * (0.7 + 0.3 * rng.f32())), rng.f32() * TAU);
    }
    // Ложе под кусками.
    mb.lathe(Vec3::ZERO, &[(0.0, 0.0), (0.95, 0.0), (0.7, 0.25), (0.3, 0.45), (0.0, 0.5)], 10);
    mb.build()
}

/// Модель кучи и её части (чтобы менять, не пересоздавая).
#[derive(Component)]
struct PileModel {
    id: u32,
    kind: PileKind,
    logs: Vec<Entity>,
    mound: Entity,
    coal: Entity,
    ore: Entity,
    vents: Vec<Entity>,
    flames: Vec<Entity>,
    smoke: Entity,
    smoke_mat: Handle<SmokeMaterial>,
    light: Entity,
    /// Что показано сейчас (чтобы не трогать каждый кадр).
    shown: Option<(Stage, u16, u16, u8, i32, Smoke, bool)>,
}

/// Сколько поленьев рисовать в клетке кострища.
const ROAST_LOGS: usize = 10;

fn spawn_piles(mut commands: Commands, assets: Res<MineAssets>, mut meshes: ResMut<Assets<Mesh>>, mut smokes: ResMut<Assets<SmokeMaterial>>) {
    let sky = crate::graphics::mood().sky_uniform();
    for spot in pile_spots() {
        let root = commands.spawn((Transform::from_translation(spot.pos), Visibility::default())).id();
        let mut kids = Vec::new();
        let mut logs = Vec::new();
        match spot.kind {
            PileKind::Clamp => {
                // Поленья стоймя вокруг кола, плотными кольцами (одно кольцо — одна сущность).
                for ring in &assets.rings {
                    logs.push(commands.spawn((Mesh3d(ring.clone()), MeshMaterial3d(assets.wood.clone()), Transform::default(), Visibility::Hidden)).id());
                }
            }
            PileKind::Roast => {
                // Клетка из поленьев: слои накрест.
                for layer in 0..5 {
                    for side in [-1.0f32, 1.0] {
                        let y = 0.09 + layer as f32 * 0.16;
                        let rot = if layer % 2 == 0 { Quat::from_rotation_z(PI / 2.0) } else { Quat::from_rotation_x(PI / 2.0) };
                        let off = if layer % 2 == 0 { Vec3::new(0.0, y, side * 0.45) } else { Vec3::new(side * 0.45, y, 0.0) };
                        logs.push(commands.spawn((Mesh3d(assets.log.clone()), MeshMaterial3d(assets.wood.clone()), Transform::from_translation(off).with_rotation(rot).with_scale(Vec3::new(1.0, 1.3, 1.0)), Visibility::Hidden)).id());
                    }
                }
            }
        }
        kids.extend(logs.iter().copied());
        let mound = commands.spawn((Mesh3d(assets.dome.clone()), MeshMaterial3d(assets.earth.clone()), Transform::from_scale(Vec3::new(1.9, 1.2, 1.9)), Visibility::Hidden)).id();
        // Разгребённый уголь по краю остывшей кучи.
        let coal = commands.spawn((Mesh3d(assets.heap.clone()), MeshMaterial3d(assets.coal.clone()), Transform::from_xyz(1.9, 0.0, 0.4).with_scale(Vec3::new(0.9, 0.6, 0.7)), Visibility::Hidden)).id();
        let ore = commands.spawn((Mesh3d(assets.heap.clone()), MeshMaterial3d(assets.ore.clone()), Transform::from_xyz(0.0, 0.9, 0.0).with_scale(Vec3::new(0.75, 0.5, 0.75)), Visibility::Hidden)).id();
        let mut vents = Vec::new();
        for k in 0..VENTS {
            let a = k as f32 / VENTS as f32 * TAU + 0.3;
            let p = Vec3::new(a.cos() * 1.72, 0.32, a.sin() * 1.72);
            vents.push(commands.spawn((Mesh3d(assets.ball.clone()), MeshMaterial3d(assets.ember.clone()), Transform::from_translation(p).with_scale(Vec3::splat(0.11)), Visibility::Hidden, NotShadowCaster)).id());
        }
        let mut flames = Vec::new();
        for k in 0..3 {
            let a = k as f32 * 2.1;
            let p = match spot.kind {
                PileKind::Clamp => Vec3::new(a.cos() * 0.6, 1.25, a.sin() * 0.6),
                PileKind::Roast => Vec3::new(a.cos() * 0.35, 0.55, a.sin() * 0.35),
            };
            flames.push(commands.spawn((Mesh3d(assets.flame_mesh.clone()), MeshMaterial3d(assets.flame.clone()), Transform::from_translation(p).with_scale(Vec3::splat(1.8)), Billboard, NotShadowCaster, NotShadowReceiver, Visibility::Hidden)).id());
        }
        let smoke_mat = smokes.add(SmokeMaterial { smoke: SmokeUniform { sky, tint: Vec4::new(0.9, 0.9, 0.9, 0.5) } });
        let smoke = commands
            .spawn((Mesh3d(meshes.add(column_mesh(9.0, 16.0, 700 + spot.id))), MeshMaterial3d(smoke_mat.clone()), Transform::from_xyz(0.0, 0.8, 0.0), Billboard, NotShadowCaster, NotShadowReceiver, Visibility::Hidden))
            .id();
        let light = commands.spawn((PointLight { color: Color::srgb(1.0, 0.5, 0.2), intensity: 0.0, range: 11.0, radius: 0.4, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 1.0, 0.0), Visibility::Hidden)).id();
        kids.extend([mound, coal, ore, smoke, light]);
        kids.extend(vents.iter().copied());
        kids.extend(flames.iter().copied());
        commands.entity(root).add_children(&kids);
        commands.entity(root).insert(PileModel { id: spot.id, kind: spot.kind, logs, mound, coal, ore, vents, flames, smoke, smoke_mat, light, shown: None });
    }
}

/// Цвет и плотность дыма.
fn smoke_tint(s: Smoke) -> Vec4 {
    match s {
        Smoke::None => Vec4::ZERO,
        Smoke::White => Vec4::new(1.0, 1.0, 1.0, 0.75),
        Smoke::Yellow => Vec4::new(0.95, 0.85, 0.55, 0.7),
        Smoke::Blue => Vec4::new(0.62, 0.72, 0.95, 0.28),
        Smoke::Flame => Vec4::new(0.35, 0.33, 0.32, 0.8),
        Smoke::Fire => Vec4::new(0.55, 0.52, 0.5, 0.55),
    }
}

#[allow(clippy::too_many_arguments)]
fn update_piles(
    time: Res<Time>,
    view: Res<MineView>,
    assets: Res<MineAssets>,
    mut piles: Query<&mut PileModel>,
    mut vis: Query<&mut Visibility>,
    mut tfs: Query<&mut Transform>,
    mut mats: Query<&mut MeshMaterial3d<SurfaceMaterial>>,
    mut lights: Query<&mut PointLight>,
    mut smokes: ResMut<Assets<SmokeMaterial>>,
) {
    let t = time.elapsed_secs();
    let show = |e: Entity, on: bool, vis: &mut Query<&mut Visibility>| {
        if let Ok(mut v) = vis.get_mut(e) {
            let want = if on { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    };
    for mut m in &mut piles {
        let Some(v) = view.piles.get(&m.id) else { continue };
        let p = &v.pile;
        let lit = p.stage == Stage::Burning && !p.out;
        // Отсвет мерцает каждый кадр.
        if let Ok(mut l) = lights.get_mut(m.light) {
            let flick = 0.85 + 0.15 * (t * 7.3 + m.id as f32).sin() * (t * 3.1).cos();
            let glow = match m.kind {
                PileKind::Roast if lit => 60_000.0 * p.heat.min(1.6),
                PileKind::Clamp if lit => 1_500.0 * p.vents as f32 + 40_000.0 * p.cracks.min(1.0) + if p.smoke() == Smoke::Flame { 50_000.0 } else { 0.0 },
                _ => 0.0,
            };
            l.intensity = glow * flick;
        }
        show(m.light, lit, &mut vis);
        let key = (p.stage, p.wood, p.ore + p.bog, p.vents, (p.cover * 10.0) as i32 + (p.cracks * 100.0) as i32 * 100 + (p.roast * 4.0) as i32 * 100_000, p.smoke(), p.out);
        if m.shown == Some(key) {
            continue;
        }
        m.shown = Some(key);
        let burnt = matches!(p.stage, Stage::Cooling | Stage::Done) || (p.stage == Stage::Burning && p.charred > 0.3);
        match m.kind {
            PileKind::Clamp => {
                let n_logs = ((p.wood as f32 / CLAMP_WOOD.1 as f32) * CLAMP_RINGS as f32).ceil() as usize;
                let logs_seen = matches!(p.stage, Stage::Stack | Stage::Cover) || (p.stage == Stage::Burning && p.cover < 0.5);
                for (k, e) in m.logs.clone().into_iter().enumerate() {
                    show(e, logs_seen && k < n_logs, &mut vis);
                    if let Ok(mut mm) = mats.get_mut(e) {
                        let want = if burnt { assets.charred.clone() } else { assets.wood.clone() };
                        if mm.0 != want {
                            mm.0 = want;
                        }
                    }
                }
                // Земляной колпак растёт по мере укрытия; размер — от числа поленьев.
                let size = 0.55 + 0.45 * (p.wood as f32 / CLAMP_WOOD.1 as f32).sqrt();
                let covered = p.cover > 0.05 && matches!(p.stage, Stage::Cover | Stage::Ready | Stage::Burning | Stage::Cooling);
                show(m.mound, covered, &mut vis);
                if let Ok(mut tf) = tfs.get_mut(m.mound) {
                    let h = 1.35 * size * if p.stage == Stage::Burning { 1.0 - 0.35 * p.charred } else { 1.0 };
                    *tf = Transform::from_xyz(0.0, -0.05, 0.0).with_scale(Vec3::new(1.95 * size, h * 1.15 * p.cover.max(0.35), 1.95 * size));
                }
                show(m.coal, p.stage == Stage::Done && p.wood > 0, &mut vis);
                if let Ok(mut mm) = mats.get_mut(m.mound) {
                    // Тлеет — земля чернеет от сажи.
                    let want = if matches!(p.stage, Stage::Cooling | Stage::Done) || (p.stage == Stage::Burning && p.charred > 0.5) { assets.soot.clone() } else if p.cover < 0.6 { assets.sod.clone() } else { assets.earth.clone() };
                    if mm.0 != want {
                        mm.0 = want;
                    }
                }
                for (k, e) in m.vents.clone().into_iter().enumerate() {
                    show(e, lit && (k as u8) < p.vents, &mut vis);
                }
                let fire = lit && (p.cracks > 0.05 || p.smoke() == Smoke::Flame);
                for (k, e) in m.flames.clone().into_iter().enumerate() {
                    show(e, fire && (k == 0 || p.cracks > 0.4 || p.smoke() == Smoke::Flame), &mut vis);
                }
                show(m.ore, false, &mut vis);
            }
            PileKind::Roast => {
                let n_logs = ((p.wood as f32 / ROAST_WOOD.1 as f32) * ROAST_LOGS as f32).ceil() as usize;
                let burning_down = p.stage == Stage::Burning && p.charred > 0.6;
                for (k, e) in m.logs.clone().into_iter().enumerate() {
                    show(e, p.stage != Stage::Done && k < n_logs && !burning_down, &mut vis);
                    if let Ok(mut mm) = mats.get_mut(e) {
                        let want = if p.stage == Stage::Burning { assets.charred.clone() } else { assets.wood.clone() };
                        if mm.0 != want {
                            mm.0 = want;
                        }
                    }
                }
                let ore = p.ore + p.bog;
                show(m.ore, ore > 0, &mut vis);
                if let Ok(mut tf) = tfs.get_mut(m.ore) {
                    let s = 0.35 + 0.5 * (ore as f32 / ROAST_ORE as f32).sqrt();
                    let y = if p.stage == Stage::Done || burning_down { 0.02 } else { 0.1 + 0.16 * (n_logs as f32 / 2.0).ceil().min(5.0) };
                    *tf = Transform::from_xyz(0.0, y, 0.0).with_scale(Vec3::new(s, 0.85 * s, s));
                }
                if let Ok(mut mm) = mats.get_mut(m.ore) {
                    let want = if p.roast > 0.5 { assets.roasted.clone() } else { assets.ore.clone() };
                    if mm.0 != want {
                        mm.0 = want;
                    }
                }
                for e in m.flames.clone() {
                    show(e, lit && p.heat > 0.3, &mut vis);
                }
                for (k, e) in m.vents.clone().into_iter().enumerate() {
                    // Угли по краю — пока горит и пока остывает.
                    show(e, (lit || p.stage == Stage::Cooling) && k < 3, &mut vis);
                }
                show(m.mound, false, &mut vis);
                show(m.coal, false, &mut vis);
            }
        }
        let smoke = p.smoke();
        show(m.smoke, smoke != Smoke::None, &mut vis);
        // Материал дыма меняем, только когда сменился цвет: иначе он считался бы изменённым каждый кадр.
        let tint = smoke_tint(smoke);
        if smokes.get(&m.smoke_mat).is_some_and(|mat| mat.smoke.tint != tint)
            && let Some(mut mat) = smokes.get_mut(&m.smoke_mat)
        {
            mat.smoke.tint = tint;
        }
        if let Ok(mut tf) = tfs.get_mut(m.smoke) {
            let y = if m.kind == PileKind::Clamp { 0.9 + 0.6 * p.cover } else { 0.6 };
            if tf.translation.y != y {
                tf.translation.y = y;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Работники рудника и стана
// ---------------------------------------------------------------------------

/// Чем занят работник.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    /// Бьёт кайлом (забой, выход жилы).
    Hew,
    /// Копает заступом (болото, земля на кучу).
    Dig,
    /// Колет дрова.
    Chop,
    /// Крутит ворот.
    Crank,
    /// Носит корзину / полено между двумя точками.
    Haul,
    /// Перебирает руду, присев.
    Sort,
    /// Шурует шестом (кострище, отдушины кучи).
    Poke,
    Eat,
    Idle,
    /// Спит в землянке или шалаше — не виден.
    Away,
}

#[derive(Clone, Copy, Debug)]
struct Shift {
    /// С какого часа.
    from: f32,
    job: Job,
    at: Vec3,
    /// Куда смотрит; для переноски — вторая точка.
    to: Vec3,
}

struct WorkerDef {
    shifts: Vec<Shift>,
    role: Role,
    seed: u64,
}

/// Распорядок всех работников (одинаков у всех клиентов: зависит только от часа мира).
fn worker_defs(w: &WorkSpots) -> Vec<WorkerDef> {
    let s = |from: f32, job: Job, at: Vec3, to: Vec3| Shift { from, job, at, to };
    let ahead = |p: Vec3, dx: f32, dz: f32| p + Vec3::new(dx, 0.0, dz);
    let fire = |p: Vec3, k: f32| p + Vec3::new((k * 2.1).cos() * 1.6, 0.0, (k * 2.1).sin() * 1.6);
    let day = |job: Job, at: Vec3, to: Vec3, fire_at: Vec3, k: f32, hut: Vec3| {
        vec![
            s(0.0, Job::Away, hut, hut),
            s(5.5, Job::Idle, fire(fire_at, k), fire_at),
            s(6.0, job, at, to),
            s(12.0, Job::Eat, fire(fire_at, k), fire_at),
            s(13.0, job, at, to),
            s(19.0, Job::Eat, fire(fire_at, k), fire_at),
            s(20.5, Job::Away, hut, hut),
        ]
    };
    let mut v = vec![
        // Забойщики: в штольне и на выходе жилы.
        WorkerDef { shifts: day(Job::Hew, w.face, ahead(w.face, 0.0, 2.0), w.mine_fire, 0.0, w.mine_hut), role: Role::Craftsman, seed: 501 },
        WorkerDef { shifts: day(Job::Hew, w.outcrops[0], ahead(w.outcrops[0], 0.0, 2.0), w.mine_fire, 1.0, w.mine_hut), role: Role::Farmer, seed: 502 },
        // Воротовый над шурфом.
        WorkerDef { shifts: day(Job::Crank, w.windlass, ahead(w.windlass, 1.0, 0.0), w.mine_fire, 2.0, w.mine_hut), role: Role::Servant, seed: 503 },
        // Носильщик: из штольни в отвал и на разборку.
        WorkerDef { shifts: day(Job::Haul, ahead(w.face, 0.0, -1.0), w.sorting, w.mine_fire, 3.0, w.mine_hut), role: Role::Servant, seed: 504 },
        // Разборщик руды.
        WorkerDef { shifts: day(Job::Sort, w.sorting, ahead(w.sorting, 0.0, 1.5), w.mine_fire, 4.0, w.mine_hut), role: Role::Farmer, seed: 505 },
        // Обжигальщик у кострища.
        WorkerDef { shifts: day(Job::Poke, w.roast, ahead(w.roast, -1.7, 0.0), w.mine_fire, 5.0, w.mine_hut), role: Role::Craftsman, seed: 506 },
        // Дровосек стана: колет, носит к штабелю.
        WorkerDef {
            shifts: vec![
                s(0.0, Job::Away, w.camp_hut, w.camp_hut),
                s(6.0, Job::Chop, w.woodlot[0], ahead(w.woodlot[0], 0.0, -1.0)),
                s(9.0, Job::Haul, w.woodlot[0], w.stack),
                s(10.0, Job::Chop, w.woodlot[1], ahead(w.woodlot[1], 0.0, -1.0)),
                s(12.0, Job::Eat, fire(w.camp_fire, 0.0), w.camp_fire),
                s(13.0, Job::Chop, w.woodlot[1], ahead(w.woodlot[1], 0.0, -1.0)),
                s(16.0, Job::Haul, w.woodlot[1], w.stack),
                s(18.5, Job::Eat, fire(w.camp_fire, 0.0), w.camp_fire),
                s(20.0, Job::Away, w.camp_hut, w.camp_hut),
            ],
            role: Role::Farmer,
            seed: 507,
        },
        // Углежоги: двое, по очереди всю ночь при куче (горит и ночью).
        WorkerDef {
            shifts: vec![
                s(0.0, Job::Poke, w.clamps[0], ahead(w.clamps[0], -3.6, 0.0)),
                s(4.0, Job::Idle, fire(w.camp_fire, 1.5), w.camp_fire),
                s(6.0, Job::Away, w.camp_hut, w.camp_hut),
                s(13.0, Job::Eat, fire(w.camp_fire, 1.5), w.camp_fire),
                s(14.0, Job::Dig, w.clamps[1], ahead(w.clamps[1], 3.6, 0.0)),
                s(18.0, Job::Poke, w.clamps[0], ahead(w.clamps[0], -3.6, 0.0)),
            ],
            role: Role::Farmer,
            seed: 508,
        },
        WorkerDef {
            shifts: vec![
                s(0.0, Job::Away, w.camp_hut, w.camp_hut),
                s(5.0, Job::Poke, w.clamps[0], ahead(w.clamps[0], -3.6, 0.0)),
                s(12.0, Job::Eat, fire(w.camp_fire, 3.0), w.camp_fire),
                s(13.0, Job::Haul, w.stack, ahead(w.clamps[1], 2.0, 0.0)),
                s(16.0, Job::Poke, w.clamps[0], ahead(w.clamps[0], -3.6, 0.0)),
                s(19.0, Job::Eat, fire(w.camp_fire, 3.0), w.camp_fire),
                s(21.0, Job::Idle, fire(w.camp_fire, 3.0), w.camp_fire),
                s(23.0, Job::Away, w.camp_hut, w.camp_hut),
            ],
            role: Role::Farmer,
            seed: 509,
        },
    ];
    // Копатели болотной руды (готы из селения): только днём.
    for (k, p) in w.bog.iter().enumerate() {
        let home = *p + Vec3::new(0.0, 0.0, -60.0);
        v.push(WorkerDef {
            shifts: vec![
                s(0.0, Job::Away, home, home),
                s(7.0 + k as f32 * 0.5, Job::Dig, *p, *p + Vec3::new(if k == 0 { -1.0 } else { 1.0 }, 0.0, 0.3)),
                s(12.0, Job::Idle, *p + Vec3::new(1.2, 0.0, 1.2), *p),
                s(13.0, Job::Dig, *p, *p + Vec3::new(if k == 0 { -1.0 } else { 1.0 }, 0.0, 0.3)),
                s(17.5, Job::Away, home, home),
            ],
            role: Role::Farmer,
            seed: 520 + k as u64,
        });
    }
    v
}

/// Где работник и чем занят в час `h` (реальное время `t` — для переноски туда-обратно).
fn worker_state(def: &WorkerDef, h: f32, t: f32) -> (Vec3, f32, Job, bool, f32) {
    let sh = def.shifts.iter().rev().find(|s| s.from <= h).copied().unwrap_or(def.shifts[def.shifts.len() - 1]);
    let yaw_to = crate::daily_life::yaw_to;
    if sh.job == Job::Haul {
        // Туда — пауза — обратно — пауза.
        let len = sh.at.distance(sh.to).max(0.5);
        let walk = len / 1.1;
        let cycle = walk * 2.0 + 8.0;
        let phase = (t + def.seed as f32 * 3.7) % cycle;
        let (pos, yaw, walking, carry) = if phase < walk {
            let k = phase / walk;
            (sh.at.lerp(sh.to, k), yaw_to(sh.at, sh.to), true, true)
        } else if phase < walk + 4.0 {
            (sh.to, yaw_to(sh.at, sh.to), false, true)
        } else if phase < walk * 2.0 + 4.0 {
            let k = (phase - walk - 4.0) / walk;
            (sh.to.lerp(sh.at, k), yaw_to(sh.to, sh.at), true, false)
        } else {
            (sh.at, yaw_to(sh.to, sh.at), false, false)
        };
        let walked = if walking { (phase % walk) * 1.1 } else { 0.0 };
        return (pos, yaw, if carry { Job::Haul } else { Job::Idle }, walking, walked);
    }
    (sh.at, yaw_to(sh.at, sh.to), sh.job, false, 0.0)
}

#[derive(Component)]
struct Worker {
    idx: usize,
    right: Entity,
    left: Entity,
    props: [Option<Tool2>; 2],
    shown: [Option<Tool2>; 2],
}

#[derive(Resource)]
struct Workers(Vec<WorkerDef>);

fn spawn_workers(mut commands: Commands) {
    let Some(w) = mine_work_spots() else { return };
    let defs = worker_defs(&w);
    for (i, d) in defs.iter().enumerate() {
        let mut outfit = crate::residents::outfit(d.role, d.seed);
        // Рабочая одежда: некрашеная, в пыли и саже.
        outfit.tunic_color = Color::srgb(0.36 + 0.05 * (i % 3) as f32, 0.31, 0.25);
        outfit.legs = crate::rig::LegGear::Trousers;
        outfit.trousers = Color::srgb(0.24, 0.21, 0.18);
        let right = commands.spawn((Transform::default(), Visibility::Hidden)).id();
        let left = commands.spawn((Transform::default(), Visibility::Hidden)).id();
        let body = commands.spawn((NeedsRig(outfit), Transform::default(), Visibility::default())).id();
        commands
            .spawn((Worker { idx: i, right, left, props: [None, None], shown: [None, None] }, Transform::from_translation(d.shifts[0].at), Visibility::Hidden))
            .add_children(&[body, right, left]);
    }
    commands.insert_resource(Workers(defs));
}

/// Поза для дела. Хваты — в пространстве персонажа (вперёд — −Z).
fn worker_pose(job: Job, tt: f32, walking: bool, walked: f32) -> (PoseInput, [Option<Tool2>; 2]) {
    let mut i = PoseInput { time: tt, ..default() };
    let mut props = [None, None];
    let strike = |period: f32| {
        let p = (tt / period).fract();
        if p < 0.65 { 1.0 - p / 0.65 } else if p < 0.78 { (p - 0.65) / 0.13 } else { 1.0 }
    };
    let two_hands = |i: &mut PoseInput, r: Vec3, l: Vec3| {
        i.right_grip = Some(r);
        i.left_grip = Some(l);
        let axis = (l - r).normalize();
        i.right_axis = Some(axis);
        i.left_axis = Some(axis);
    };
    if walking {
        i.walk = 1.0;
        i.walk_phase = walked / crate::rig::step_cycle(1.1) * TAU;
        if job == Job::Haul {
            // Корзина на плече.
            i.right_grip = Some(Vec3::new(0.22, 1.5, 0.02));
            i.left_grip = Some(Vec3::new(0.1, 1.38, -0.16));
            i.lean = -0.08;
            props[0] = Some(Tool2::Basket);
        }
        return (i, props);
    }
    match job {
        Job::Hew => {
            // Кайло: замах из-за плеча, удар вниз в забой.
            let h = strike(1.5);
            i.lean = -0.2 - 0.1 * (1.0 - h);
            i.pitch = -0.35;
            i.twist = -0.1 * h;
            let r = Vec3::new(0.1, 1.0, -0.48).lerp(Vec3::new(0.18, 1.55, -0.05), h);
            let l = r + Vec3::new(-0.1, -0.05, 0.22).lerp(Vec3::new(-0.08, -0.25, 0.05), h);
            two_hands(&mut i, r, l);
            // Рабочий конец — по оси от левой руки к правой и дальше.
            let axis = (r - l).normalize();
            i.right_axis = Some(axis);
            i.left_axis = Some(axis);
            props[0] = Some(Tool2::Pick);
        }
        Job::Chop => {
            let h = strike(1.3);
            i.lean = -0.15 - 0.12 * (1.0 - h);
            i.pitch = -0.5;
            let r = Vec3::new(0.08, 0.85, -0.5).lerp(Vec3::new(0.15, 1.6, -0.08), h);
            let l = r + Vec3::new(-0.06, -0.12, 0.12);
            two_hands(&mut i, r, l);
            let axis = (r - l).normalize();
            i.right_axis = Some(axis);
            i.left_axis = Some(axis);
            props[0] = Some(Tool2::Axe);
        }
        Job::Dig => {
            // Заступом: воткнуть, нажать, поднять и бросить в сторону.
            let p = (tt / 2.4).fract();
            let (lift, side) = if p < 0.45 { (0.0, 0.0) } else if p < 0.75 { ((p - 0.45) / 0.3, 0.0) } else { (1.0 - (p - 0.75) / 0.25, ((p - 0.75) / 0.25 * PI).sin()) };
            i.lean = -0.35 + 0.15 * lift;
            i.pitch = -0.6;
            i.twist = 0.35 * side;
            let r = Vec3::new(0.12, 0.72 + 0.3 * lift, -0.38);
            let l = r + Vec3::new(-0.08, 0.38, 0.1);
            two_hands(&mut i, r, l);
            props[0] = Some(Tool2::Spade);
        }
        Job::Crank => {
            // Две рукояти ворота, по кругу в плоскости y-z.
            let a = tt * 2.2;
            i.lean = -0.12 + 0.06 * a.sin();
            i.pitch = -0.3;
            i.right_grip = Some(Vec3::new(0.26, 1.0 + 0.2 * a.sin(), -0.5 + 0.2 * a.cos()));
            i.left_grip = Some(Vec3::new(-0.26, 1.0 - 0.2 * a.sin(), -0.5 - 0.2 * a.cos()));
        }
        Job::Haul => {
            // Стоит с корзиной: ссыпает.
            i.lean = -0.25;
            i.pitch = -0.4;
            i.right_grip = Some(Vec3::new(0.18, 1.0, -0.42));
            i.left_grip = Some(Vec3::new(-0.12, 0.92, -0.42));
            props[0] = Some(Tool2::Basket);
        }
        Job::Sort => {
            i.crouch = 0.85;
            i.lean = -0.3;
            i.pitch = -0.75;
            let k = (tt * 1.1).sin();
            i.right_grip = Some(Vec3::new(0.15 + 0.08 * k, 0.48, -0.5));
            i.left_grip = Some(Vec3::new(-0.14, 0.5 + 0.04 * k, -0.48));
        }
        Job::Poke => {
            // Шест вперёд-вниз: ворошит руду или проверяет отдушины.
            let k = ((tt * 0.9).sin() * 0.5 + 0.5).powf(1.5);
            i.lean = -0.2 - 0.1 * k;
            i.pitch = -0.45;
            let r = Vec3::new(0.16, 1.0, -0.05) + Vec3::new(0.0, 0.0, -0.25) * k;
            let l = Vec3::new(-0.02, 1.08, -0.42) + Vec3::new(0.0, -0.05, -0.25) * k;
            i.right_grip = Some(r);
            i.left_grip = Some(l);
            let axis = (r - l).normalize();
            i.right_axis = Some(axis);
            i.left_axis = Some(axis);
            props[0] = Some(Tool2::Pole);
        }
        Job::Eat => {
            let up = ((tt / 5.0).fract() * TAU).sin().max(0.0).powf(0.6);
            i.crouch = 0.9;
            i.pitch = 0.2 * up - 0.25;
            i.right_grip = Some(Vec3::new(0.14, 0.75, -0.34).lerp(Vec3::new(0.05, 1.12, -0.13), up));
            i.right_axis = Some(Vec3::Y.lerp(Vec3::new(0.0, 0.3, 1.0), up).normalize());
            props[0] = Some(Tool2::Cup);
        }
        Job::Idle | Job::Away => {
            i.pitch = 0.12 * (tt * 0.13).sin();
            i.twist = 0.1 * (tt * 0.3).sin();
        }
    }
    (i, props)
}

/// Дальше этого работников не рисуем.
const WORKER_VIEW: f32 = 70.0;

#[allow(clippy::too_many_arguments)]
fn drive_workers(
    time: Res<Time>,
    clock: Res<WorldClock>,
    defs: Option<Res<Workers>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut workers: Query<(&mut Worker, &mut Transform, &mut Visibility, &Children)>,
    mut poses: Query<&mut RigPose>,
    mut slots: Query<(&mut Transform, &mut Visibility), Without<Worker>>,
    mut frame: Local<u32>,
) {
    let Some(defs) = defs else { return };
    *frame = frame.wrapping_add(1);
    let t = time.elapsed_secs();
    let h = clock.hour();
    let cam = camera.iter().find(|(c, _)| c.is_active).map(|(_, t)| t.translation());
    for (mut w, mut tf, mut vis, children) in &mut workers {
        let Some(def) = defs.0.get(w.idx) else { continue };
        let (pos, yaw, job, walking, walked) = worker_state(def, h, t);
        let dist = cam.map_or(0.0, |c| c.distance(pos));
        let on = job != Job::Away && dist < WORKER_VIEW;
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if !on {
            continue;
        }
        tf.translation = pos;
        tf.rotation = Quat::from_rotation_y(yaw);
        let every = if dist < 25.0 { 1 } else { 3 };
        if !(*frame + w.idx as u32).is_multiple_of(every) {
            continue;
        }
        let tt = t + w.idx as f32 * 1.37;
        let (input, props) = worker_pose(job, tt, walking, walked);
        let solved = crate::rig::solve_pose(&input);
        for c in children.iter() {
            if let Ok(mut p) = poses.get_mut(c) {
                *p = solved.pose.clone();
            }
        }
        w.props = props;
        for (k, (slot, at, axis)) in [(w.right, solved.right_grip, input.right_axis), (w.left, solved.left_grip, input.left_axis)].into_iter().enumerate() {
            let Ok((mut st, mut sv)) = slots.get_mut(slot) else { continue };
            let v = if props[k].is_some() { Visibility::Inherited } else { Visibility::Hidden };
            if *sv != v {
                *sv = v;
            }
            if props[k].is_some() {
                st.translation = at;
                st.rotation = Quat::from_rotation_arc(Vec3::Y, axis.unwrap_or(Vec3::Y).normalize_or(Vec3::Y));
            }
        }
    }
}

fn update_worker_props(mut commands: Commands, assets: Res<MineAssets>, mut workers: Query<&mut Worker>) {
    for mut w in &mut workers {
        for k in 0..2 {
            if w.shown[k] == w.props[k] {
                continue;
            }
            let slot = if k == 0 { w.right } else { w.left };
            let want = w.props[k];
            w.shown[k] = want;
            let mut e = commands.entity(slot);
            e.despawn_related::<Children>();
            if let Some(parts) = want.and_then(|p| assets.tools.get(&p)) {
                e.with_children(|c| {
                    for (mesh, mat, tf) in parts {
                        c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), *tf, NotShadowCaster));
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore::Knowledge;

    fn view(kind: PileKind, f: impl FnOnce(&mut Pile)) -> PileView {
        let mut p = Pile::new(kind);
        f(&mut p);
        PileView { id: 1, kind, pos: [0.0; 3], stage: p.stage, wood: p.wood, ore: p.ore + p.bog, cover: p.cover, vents: p.vents, smoke: p.smoke(), heat: p.heat, cracks: p.cracks, pile: p }
    }

    #[test]
    fn pile_keys_follow_the_stage() {
        let empty = view(PileKind::Clamp, |_| {});
        let o = pile_options(&empty);
        assert_eq!(o.len(), 1);
        assert!(matches!(o[0].2, MineAct::StackWood { .. }));
        let burning = view(PileKind::Clamp, |p| {
            p.add_wood(20, 0.9);
            p.add_cover(1.0);
            p.light().unwrap();
        });
        let o = pile_options(&burning);
        assert!(o.iter().any(|(k, _, a)| *k == KeyCode::KeyR && matches!(a, MineAct::Vent { open: true, .. })));
        assert!(o.iter().any(|(_, _, a)| matches!(a, MineAct::Smother { .. })));
        // Клавиши не повторяются.
        let mut keys: Vec<_> = o.iter().map(|(k, _, _)| *k).collect();
        keys.dedup();
        assert_eq!(keys.len(), o.len());
        let roast = view(PileKind::Roast, |p| {
            p.add_wood(4, 0.9);
            p.add_ore(6, 0);
        });
        assert!(pile_options(&roast).iter().any(|(k, _, a)| *k == KeyCode::KeyZ && matches!(a, MineAct::Light { .. })));
    }

    #[test]
    fn smoke_is_read_only_by_the_knowing() {
        let mut v = view(PileKind::Clamp, |p| {
            p.add_wood(20, 0.9);
            p.add_cover(1.0);
            p.light().unwrap();
        });
        v.pile.moisture = 0.0;
        v.pile.charred = 0.9;
        v.pile.heat = 0.8;
        let novice = pile_line(&v, &Knowledge::default());
        let mut k = Knowledge::default();
        k.learn(Lore::Technique(Technique::Charcoal));
        let master = pile_line(&v, &k);
        assert!(master.contains("Глушите"), "{master}");
        assert!(!novice.contains("Глушите"), "{novice}");
    }

    #[test]
    fn ore_outcrops_look_plain_to_the_unlearned() {
        let k = Knowledge::default();
        let s = site_line(SiteKind::Outcrop, 3, &k);
        assert!(s.starts_with("Бурые камни"), "{s}");
        assert!(s.contains("[E]"));
        assert!(site_line(SiteKind::Outcrop, 0, &k).contains("выбрано"));
    }

    #[test]
    fn workers_keep_hours() {
        let spots = mine_work_spots().expect("рудник есть в Херсоне");
        let defs = worker_defs(&spots);
        assert!(defs.len() >= 9);
        // Ночью рудник спит, а у угольной кучи кто-то сторожит.
        let night: Vec<Job> = defs.iter().map(|d| worker_state(d, 2.0, 0.0).2).collect();
        assert_eq!(night[0], Job::Away);
        assert!(night.contains(&Job::Poke));
        let noon: Vec<Job> = defs.iter().map(|d| worker_state(d, 12.5, 0.0).2).collect();
        assert!(noon.iter().filter(|j| **j == Job::Eat).count() >= 6);
        let work: Vec<Job> = defs.iter().map(|d| worker_state(d, 9.5, 0.0).2).collect();
        assert!(work.contains(&Job::Hew) && work.contains(&Job::Crank));
    }

    /// Где что (для снимков): `cargo test print_mine_spots -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_mine_spots() {
        for s in mine_sites() {
            println!("site {:2} {:?} {:?}", s.id, s.kind, s.pos);
        }
        for p in pile_spots() {
            println!("pile {:2} {:?} {:?}", p.id, p.kind, p.pos);
        }
        let w = mine_work_spots().unwrap();
        println!("face {:?} windlass {:?} sorting {:?} roast {:?} fire {:?} hut {:?}", w.face, w.windlass, w.sorting, w.roast, w.mine_fire, w.mine_hut);
        println!("camp fire {:?} hut {:?} stack {:?} wood {:?} bog {:?}", w.camp_fire, w.camp_hut, w.stack, w.woodlot, w.bog);
        for (x, z) in [(-239.0, 680.0), (-262.0, 676.0), (-545.0, 700.0), (-600.0, 450.0), (440.0, 285.0), (101.0, 238.0), (-410.0, 195.0), (-230.0, 715.0)] {
            println!("ground {x} {z} = {}", crate::level::ground_y(x, z));
        }
    }
}
