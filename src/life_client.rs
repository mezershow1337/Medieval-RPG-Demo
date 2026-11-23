//! Клиент: живые жители. Ставит каждого туда, где он по распорядку (`daily_life.rs`), играет
//! позу занятия (кузнец бьёт молотом, хозяйка мешает в котле, рыбак держит удочку, священник
//! воздевает руки), даёт в руки инструмент, ночью — факел, укладывает спать в постель.
//! Соседи на агоре и в таверне беседуют между собой — реплики видно над головами вблизи.
//!
//! Производительность: положение берётся из плана за пару сравнений, позу считаем
//! каждый кадр только вблизи (дальше 22 м — через кадр, дальше 38 м — раз в 4 кадра), дальше
//! 60 м жителей не рисуем.

use crate::daily_life::{self, Act, Fright, NpcState};
use crate::level::Role;
use crate::residents::ResidentView;
use crate::rig::{PoseInput, RigPose};
use crate::world_clock::WorldClock;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub struct LifeClientPlugin;

impl Plugin for LifeClientPlugin {
    fn build(&self, app: &mut App) {
        daily_life::warm_up();
        app.init_resource::<Fright>()
            .init_resource::<LiveActs>()
            .add_message::<daily_life::Alarm>()
            .add_systems(Startup, (setup_props, setup_rain, setup_chatter.after(crate::inventory_ui::setup_fonts)))
            .add_systems(Update, rain)
            .add_systems(Update, (attach_gait, drive_residents, update_props, chatter, clock_hud).chain().in_set(LifeSystems).before(crate::doors::DoorSystems));
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifeSystems;

/// Чем сейчас занят каждый житель (для разговора на «E»).
#[derive(Resource, Default)]
pub struct LiveActs(pub Vec<Act>);

/// Сглаженный поворот и предметы в руках жителя.
#[derive(Component)]
struct Gait {
    yaw: f32,
    right: Entity,
    left: Entity,
    kinds: [Option<Prop>; 2],
    /// Сжатая поза ещё не посчитана (первый кадр).
    fresh: bool,
}

/// Пустышка в правой руке жителя; предмет — её дочерняя сущность (`ShownProp` — какой).
#[derive(Component)]
pub struct PropSlot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    Hammer,
    Tongs,
    Spoon,
    Broom,
    Rod,
    Spear,
    Hoe,
    Amphora,
    Jar,
    Basket,
    Cup,
    Torch,
    Book,
}

/// Что из предмета куда смотрит: точка хвата — начало координат, ось рукояти — +Y.
struct PropParts(Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)>);

#[derive(Resource)]
struct PropKit {
    parts: std::collections::HashMap<Prop, PropParts>,
}

fn setup_props(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mut mat = |c: Color, rough: f32, metal: f32| mats.add(StandardMaterial { base_color: c, perceptual_roughness: rough, metallic: metal, ..default() });
    let wood = mat(Color::srgb(0.42, 0.3, 0.18), 0.85, 0.0);
    let dark_wood = mat(Color::srgb(0.28, 0.2, 0.12), 0.8, 0.0);
    let iron = mat(Color::srgb(0.32, 0.32, 0.34), 0.45, 0.9);
    let clay = mat(Color::srgb(0.66, 0.38, 0.24), 0.9, 0.0);
    let wicker = mat(Color::srgb(0.7, 0.58, 0.36), 0.95, 0.0);
    let straw = mat(Color::srgb(0.78, 0.68, 0.4), 0.95, 0.0);
    let leather = mat(Color::srgb(0.35, 0.18, 0.1), 0.7, 0.0);
    let flame = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.6, 0.2), emissive: LinearRgba::rgb(14.0, 6.0, 1.5), unlit: true, ..default() });
    let mut m = |mesh: Mesh| meshes.add(mesh);
    let at = |y: f32| Transform::from_xyz(0.0, y, 0.0);
    use std::collections::HashMap;
    let mut parts: HashMap<Prop, PropParts> = HashMap::new();
    parts.insert(
        Prop::Hammer,
        PropParts(vec![
            (m(Cylinder::new(0.016, 0.42).into()), wood.clone(), at(0.12)),
            (m(Cuboid::new(0.05, 0.05, 0.15).into()), iron.clone(), at(0.31)),
        ]),
    );
    parts.insert(Prop::Tongs, PropParts(vec![(m(Cuboid::new(0.02, 0.5, 0.03).into()), iron.clone(), at(0.15))]));
    parts.insert(Prop::Spoon, PropParts(vec![(m(Cylinder::new(0.012, 0.6).into()), wood.clone(), at(-0.15))]));
    parts.insert(
        Prop::Broom,
        PropParts(vec![
            (m(Cylinder::new(0.016, 1.25).into()), wood.clone(), at(-0.35)),
            (m(Cone { radius: 0.11, height: 0.35 }.into()), straw.clone(), Transform::from_xyz(0.0, -1.05, 0.0)),
        ]),
    );
    parts.insert(Prop::Rod, PropParts(vec![(m(Cone { radius: 0.02, height: 3.2 }.into()), dark_wood.clone(), at(1.35))]));
    parts.insert(
        Prop::Spear,
        PropParts(vec![
            (m(Cylinder::new(0.017, 2.2).into()), wood.clone(), at(0.55)),
            (m(Cone { radius: 0.03, height: 0.26 }.into()), iron.clone(), at(1.78)),
        ]),
    );
    parts.insert(
        Prop::Hoe,
        PropParts(vec![
            (m(Cylinder::new(0.017, 1.2).into()), wood.clone(), at(-0.3)),
            (m(Cuboid::new(0.16, 0.03, 0.2).into()), iron.clone(), Transform::from_xyz(0.0, -0.9, -0.08)),
        ]),
    );
    // Амфора на плече: корпус, горло, ножка.
    parts.insert(
        Prop::Amphora,
        PropParts(vec![
            (m(Capsule3d::new(0.15, 0.36).into()), clay.clone(), Transform::IDENTITY),
            (m(Cylinder::new(0.045, 0.22).into()), clay.clone(), at(0.36)),
            (m(Cone { radius: 0.05, height: 0.14 }.into()), clay.clone(), Transform::from_xyz(0.0, -0.36, 0.0).with_rotation(Quat::from_rotation_x(PI))),
        ]),
    );
    parts.insert(
        Prop::Jar,
        PropParts(vec![(m(Sphere::new(0.15).into()), clay.clone(), at(0.13)), (m(Cylinder::new(0.05, 0.12).into()), clay.clone(), at(0.3))]),
    );
    parts.insert(Prop::Basket, PropParts(vec![(m(Cylinder::new(0.16, 0.16).into()), wicker.clone(), at(-0.16)), (m(Torus::new(0.006, 0.012).into()), wicker.clone(), at(0.0))]));
    parts.insert(Prop::Cup, PropParts(vec![(m(Cylinder::new(0.04, 0.08).into()), clay.clone(), at(0.03))]));
    parts.insert(
        Prop::Torch,
        PropParts(vec![(m(Cylinder::new(0.02, 0.55).into()), dark_wood.clone(), at(0.05)), (m(Sphere::new(0.07).into()), flame.clone(), at(0.36))]),
    );
    parts.insert(Prop::Book, PropParts(vec![(m(Cuboid::new(0.2, 0.05, 0.26).into()), leather.clone(), Transform::IDENTITY)]));
    commands.insert_resource(PropKit { parts });
}

fn attach_gait(mut commands: Commands, added: Query<(Entity, &Transform), Added<ResidentView>>) {
    for (e, t) in &added {
        let right = commands.spawn((PropSlot, Transform::default(), Visibility::Hidden)).id();
        let left = commands.spawn((PropSlot, Transform::default(), Visibility::Hidden)).id();
        commands.entity(e).add_children(&[right, left]);
        let (yaw, _, _) = t.rotation.to_euler(EulerRot::YXZ);
        commands.entity(e).insert(Gait { yaw, right, left, kinds: [None, None], fresh: true });
    }
}

/// Поза и предметы для занятия.
struct Look {
    input: PoseInput,
    props: [Option<Prop>; 2],
    lying: bool,
}

fn grip(x: f32, y: f32, z: f32) -> Option<Vec3> {
    Some(Vec3::new(x, y, z))
}

/// Плавный «удар»: 0 — замах, 1 — удар (быстро вниз, медленно вверх).
fn strike(t: f32, period: f32) -> f32 {
    let p = (t / period).fract();
    if p < 0.65 { 1.0 - (p / 0.65) } else if p < 0.78 { (p - 0.65) / 0.13 } else { 1.0 }
}

fn look_for(me: usize, role: Role, s: &NpcState, t: f32, phase: f32, night: bool, rain: f32) -> Look {
    let mut i = PoseInput { time: t + phase, ..default() };
    let mut props = [None, None];
    let tt = t + phase;
    let sway = (tt * 0.35).sin();
    i.twist = sway * 0.1;
    if s.walking {
        i.walk = 1.0;
        i.walk_phase = s.walked / 1.5 * TAU + phase;
        // В дождь идут, сгорбившись.
        i.lean = -0.12 * rain;
        i.twist = 0.0;
        if s.carry {
            let (prop, r, l) = match role {
                Role::Sailor | Role::Servant | Role::Householder => (Prop::Amphora, grip(0.2, 1.62, 0.02), grip(0.12, 1.42, -0.18)),
                Role::Housewife | Role::Cook => (Prop::Jar, grip(0.22, 1.55, -0.02), grip(0.1, 1.45, -0.12)),
                _ => (Prop::Basket, grip(0.24, 0.82, -0.08), None),
            };
            i.right_grip = r;
            i.left_grip = l;
            if prop == Prop::Amphora {
                i.right_axis = Some(Vec3::new(-0.4, 0.3, 0.9).normalize());
            }
            props[0] = Some(prop);
        } else if s.patrol && role == Role::Soldier {
            i.right_grip = grip(0.26, 1.0, -0.12);
            i.right_axis = Some(Vec3::Y);
            props[0] = Some(Prop::Spear);
        }
        if night && !s.carry {
            i.left_grip = grip(-0.22, 1.28, -0.3);
            i.left_axis = Some(Vec3::new(0.0, 1.0, -0.2).normalize());
            props[1] = Some(Prop::Torch);
        }
        return Look { input: i, props, lying: false };
    }
    match s.act {
        Act::Sleep | Act::Lie => {
            i.twist = 0.0;
            i.time = tt * 0.6;
            return Look { input: i, props, lying: true };
        }
        Act::Idle | Act::Bathe | Act::Away => {
            i.pitch = 0.15 * (tt * 0.13).sin();
        }
        Act::Work => {
            i.lean = -0.32 + 0.05 * (tt * 0.9).sin();
            i.pitch = -0.5;
            let k = (tt * 2.1).sin();
            i.right_grip = grip(0.14, 0.88 + 0.06 * k, -0.46);
            i.left_grip = grip(-0.14, 0.88 - 0.06 * k, -0.46);
        }
        Act::Hammer => {
            let h = strike(tt, 1.25);
            i.lean = -0.12 - 0.08 * (1.0 - h);
            i.pitch = -0.55;
            i.twist = -0.12 * h;
            i.right_grip = Some(Vec3::new(0.16, 0.98, -0.58).lerp(Vec3::new(0.28, 1.58, -0.22), h));
            i.right_axis = Some(Vec3::new(0.0, 0.3, -1.0).lerp(Vec3::new(0.0, 1.0, 0.4), h).normalize());
            i.left_grip = grip(-0.12, 0.98, -0.52);
            i.left_axis = Some(Vec3::new(0.3, 0.0, -1.0).normalize());
            props = [Some(Prop::Hammer), Some(Prop::Tongs)];
        }
        Act::Stir => {
            let a = tt * 1.6;
            i.lean = -0.22;
            i.pitch = -0.55;
            i.right_grip = grip(0.08 + 0.09 * a.cos(), 1.02, -0.46 + 0.07 * a.sin());
            i.right_axis = Some(Vec3::new(0.0, 1.0, 0.25).normalize());
            props[0] = Some(Prop::Spoon);
        }
        Act::Weave => {
            let k = (tt * 0.8).sin();
            i.lean = -0.08;
            i.pitch = -0.2;
            i.right_grip = grip(0.05 + 0.3 * k, 1.2, -0.42);
            i.left_grip = grip(-0.05 + 0.3 * k, 1.25, -0.42);
        }
        Act::Write => {
            i.lean = -0.3;
            i.pitch = -0.6;
            i.right_grip = grip(0.1 + 0.03 * (tt * 3.0).sin(), 0.86, -0.42 + 0.02 * (tt * 2.3).cos());
            i.left_grip = grip(-0.16, 0.84, -0.42);
        }
        Act::Teach => {
            let g = ((tt * 0.45).sin() * 1.5).clamp(0.0, 1.0);
            i.twist = 0.25 * (tt * 0.2).sin();
            i.right_grip = Some(Vec3::new(0.25, 0.95, -0.2).lerp(Vec3::new(0.32, 1.42, -0.42), g));
        }
        Act::Treat => {
            i.crouch = 0.85;
            i.lean = -0.25;
            i.pitch = -0.7;
            let k = (tt * 1.3).sin();
            i.right_grip = grip(0.12, 0.55 + 0.05 * k, -0.48);
            i.left_grip = grip(-0.12, 0.55 - 0.04 * k, -0.48);
        }
        Act::Pray => {
            // Оранта: руки воздеты по сторонам, взгляд вверх.
            let b = 0.03 * (tt * 0.5).sin();
            i.pitch = 0.25;
            i.right_grip = grip(0.42, 1.52 + b, -0.14);
            i.left_grip = grip(-0.42, 1.52 + b, -0.14);
        }
        Act::Preach => {
            let cycle = (tt / 9.0).fract();
            if cycle < 0.5 {
                i.pitch = 0.2;
                i.right_grip = grip(0.42, 1.55, -0.15);
                i.left_grip = grip(-0.42, 1.55, -0.15);
            } else {
                // Благословляет правой, в левой — Евангелие.
                i.right_grip = grip(0.18, 1.55, -0.34);
                i.left_grip = grip(-0.12, 1.12, -0.32);
                props[1] = Some(Prop::Book);
            }
        }
        Act::Talk => {
            let speaking = speaking(me, s.partner, t);
            let g = if speaking { ((tt * 1.3).sin() * 1.2).clamp(0.0, 1.0) } else { 0.0 };
            i.twist = 0.12 * (tt * 0.6).sin();
            i.pitch = 0.05;
            i.right_grip = if g > 0.05 { Some(Vec3::new(0.24, 1.0, -0.22).lerp(Vec3::new(0.3, 1.3, -0.42), g)) } else { None };
        }
        Act::Browse => {
            i.lean = -0.12;
            i.pitch = -0.4;
            let reach = ((tt * 0.3).sin() * 2.0).clamp(0.0, 1.0);
            i.right_grip = if reach > 0.1 { Some(Vec3::new(0.22, 0.95, -0.25).lerp(Vec3::new(0.15, 1.0, -0.6), reach)) } else { None };
            i.left_grip = grip(-0.26, 0.82, -0.08);
            props[1] = Some(Prop::Basket);
        }
        Act::Arrange => {
            let k = (tt * 0.7).sin();
            i.lean = -0.1;
            i.pitch = -0.3;
            i.twist = 0.3 * k;
            i.right_grip = grip(0.2 + 0.1 * k, 1.05 + 0.1 * (tt * 1.4).sin(), -0.5);
        }
        Act::Eat | Act::Drink => {
            let period = if s.act == Act::Drink { 7.0 } else { 4.0 };
            let up = ((tt / period).fract() * TAU).sin().max(0.0).powf(0.6);
            i.pitch = 0.25 * up - 0.2;
            i.right_grip = Some(Vec3::new(0.14, 1.02, -0.34).lerp(Vec3::new(0.05, 1.56, -0.13), up));
            i.right_axis = Some(Vec3::Y.lerp(Vec3::new(0.0, 0.3, 1.0), up).normalize());
            props[0] = Some(Prop::Cup);
        }
        Act::Carry => {
            i.lean = -0.3;
            i.pitch = -0.5;
            i.right_grip = grip(0.15, 0.85, -0.45);
            i.left_grip = grip(-0.15, 0.85, -0.45);
        }
        Act::Fish => {
            let bob = 0.02 * (tt * 0.9).sin();
            i.pitch = -0.25;
            let r = Vec3::new(0.12, 1.05 + bob, -0.26);
            let l = Vec3::new(-0.02, 1.22 + bob, -0.44);
            i.right_grip = Some(r);
            i.left_grip = Some(l);
            let axis = (l - r).normalize();
            i.right_axis = Some(axis);
            i.left_axis = Some(axis);
            props[0] = Some(Prop::Rod);
        }
        Act::Sweep => {
            let k = (tt * 1.4).sin();
            i.lean = -0.18;
            i.pitch = -0.5;
            i.twist = 0.2 * k;
            let r = Vec3::new(0.12 + 0.1 * k, 0.95, -0.18);
            let l = Vec3::new(0.04 + 0.08 * k, 1.28, -0.12);
            i.right_grip = Some(r);
            i.left_grip = Some(l);
            let axis = (l - r).normalize();
            i.right_axis = Some(axis);
            i.left_axis = Some(axis);
            props[0] = Some(Prop::Broom);
        }
        Act::Guard => {
            i.pitch = 0.1 * (tt * 0.11).sin();
            if role == Role::Soldier {
                i.right_grip = grip(0.26, 1.02, -0.1);
                i.right_axis = Some(Vec3::Y);
                props[0] = Some(Prop::Spear);
            }
        }
        Act::Hoe => {
            let h = strike(tt, 1.6);
            i.lean = -0.25 - 0.1 * (1.0 - h);
            i.pitch = -0.6;
            let r = Vec3::new(0.1, 0.9, -0.42).lerp(Vec3::new(0.12, 1.4, -0.2), h);
            let l = r + Vec3::new(-0.12, 0.28, 0.12).lerp(Vec3::new(-0.1, 0.25, -0.05), h);
            i.right_grip = Some(r);
            i.left_grip = Some(l);
            let axis = (l - r).normalize();
            i.right_axis = Some(axis);
            i.left_axis = Some(axis);
            props[0] = Some(Prop::Hoe);
        }
        Act::Cower => {
            i.crouch = 1.0;
            i.lean = -0.35;
            i.pitch = -0.6;
            let shake = 0.015 * (tt * 17.0).sin();
            i.right_grip = grip(0.12 + shake, 1.12, -0.18);
            i.left_grip = grip(-0.12 - shake, 1.12, -0.18);
        }
    }
    if night && !matches!(s.act, Act::Sleep | Act::Lie | Act::Away) && props[1].is_none() && s.patrol {
        props[1] = Some(Prop::Torch);
    }
    Look { input: i, props, lying: false }
}

/// Поза торговца за прилавком: то стоит, то поправляет товар, то зазывает (жест рукой).
pub fn merchant_pose(t: f32, phase: f32) -> PoseInput {
    let cycle = ((t + phase * 7.0) / 30.0).fract();
    let act = if cycle < 0.25 {
        Act::Arrange
    } else if cycle < 0.38 {
        Act::Teach
    } else {
        Act::Idle
    };
    let s = NpcState { pos: Vec3::ZERO, yaw: 0.0, act, walking: false, walked: 0.0, carry: false, patrol: false, partner: None, lie: None };
    look_for(0, Role::Shopkeeper, &s, t, phase, false, 0.0).input
}

/// Кто из пары сейчас говорит: собеседники по очереди, по 5 с.
fn speaking(me: usize, partner: Option<u16>, t: f32) -> bool {
    let Some(p) = partner else { return false };
    ((t / 5.0) as u32 + (me < p as usize) as u32).is_multiple_of(2)
}

/// Положение «лёжа на спине» на постели: голова — к изголовью.
fn lying_transform(s: &NpcState, scale: f32) -> Transform {
    // Голова лежит по направлению −(ось ложа) (подушка — у меньшего края).
    let head = -daily_life::forward(s.yaw);
    let yaw = head.x.atan2(head.z);
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(FRAC_PI_2);
    let pos = s.lie.unwrap_or(s.pos + Vec3::Y * 0.62) - head * 0.88 * scale + Vec3::Y * 0.13;
    Transform::from_translation(pos).with_rotation(rot).with_scale(Vec3::splat(scale))
}

fn angle_diff(a: f32, b: f32) -> f32 {
    (b - a + PI).rem_euclid(TAU) - PI
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_residents(
    time: Res<Time>,
    clock: Res<WorldClock>,
    mut fright: ResMut<Fright>,
    mut alarms: MessageReader<daily_life::Alarm>,
    mut acts: ResMut<LiveActs>,
    level: Res<crate::collision::LevelColliders>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut people: Query<(&ResidentView, &mut Transform, &mut Visibility, &Children, &mut Gait, Has<crate::civil_client::Fallen>)>,
    mut poses: Query<&mut RigPose>,
    mut slots: Query<(&mut Transform, &mut Visibility), (With<PropSlot>, Without<ResidentView>)>,
    mut frame: Local<u32>,
) {
    *frame = frame.wrapping_add(1);
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    let day = clock.day();
    let now = clock.of_day();
    let night = clock.night();
    let rain = clock.rain();
    if clock.hour() > 18.0 {
        daily_life::prefetch(day + 1);
    }
    let plans = if clock.synced { daily_life::plans(day) } else { None };
    if let Some(p) = &plans {
        for a in alarms.read() {
            fright.alarm(p, a.0, now);
        }
        fright.settle(day, now);
    } else {
        alarms.clear();
    }
    let cam = camera.iter().find(|(c, _)| c.is_active).map(|(_, t)| t.translation());
    let n = people.iter().len();
    if acts.0.len() != n {
        acts.0 = vec![Act::Idle; n];
    }
    for (r, mut tf, mut vis, children, mut gait, fallen) in &mut people {
        let i = r.index;
        let scale = tf.scale.y;
        let anchor = daily_life::town().map(|t| t.residents.get(i).map(|x| (x.pos, x.yaw))).flatten();
        let s = plans
            .as_ref()
            .and_then(|p| p.people.get(i))
            .and_then(|plan| fright.state(plan, i, now))
            .unwrap_or_else(|| {
                // План ещё не готов: стоит дома за своим делом, как раньше.
                let (pos, yaw) = anchor.unwrap_or((tf.translation, gait.yaw));
                NpcState { pos, yaw, act: fallback_act(r.role), walking: false, walked: 0.0, carry: false, patrol: false, partner: None, lie: None }
            });
        if let Some(a) = acts.0.get_mut(i) {
            *a = s.act;
        }
        // Тело-коробка в доме мешает, только пока житель там (и жив).
        if let Some((home, _)) = anchor {
            level.set_body_gone(i, fallen || s.pos.distance(home) > 0.4 || matches!(s.act, Act::Sleep | Act::Away));
        }
        if fallen {
            continue;
        }
        let dist = cam.map_or(0.0, |c| c.distance(s.pos));
        let want = if dist > VIEW_DIST || s.act == Act::Away { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
        if want == Visibility::Hidden {
            gait.fresh = true;
            continue;
        }
        // Поворот: плавно, кроме первого кадра и скачков.
        let jump = tf.translation.distance(s.pos) > 3.0;
        if gait.fresh || jump {
            gait.yaw = s.yaw;
        } else {
            let d = angle_diff(gait.yaw, s.yaw);
            gait.yaw += d.clamp(-5.0 * dt, 5.0 * dt);
        }
        // Дальних обновляем реже (поза и предметы), но место — каждый кадр.
        let every = if dist < 22.0 {
            1
        } else if dist < 38.0 {
            2
        } else {
            4
        };
        let pose_now = gait.fresh || (*frame + i as u32).is_multiple_of(every);
        let look = look_for(i, r.role, &s, t, r_phase(i), night, rain);
        if look.lying {
            *tf = lying_transform(&s, scale);
        } else {
            tf.translation = s.pos;
            tf.rotation = Quat::from_rotation_y(gait.yaw);
        }
        gait.fresh = false;
        if !pose_now {
            continue;
        }
        let solved = crate::rig::solve_pose(&look.input);
        for c in children.iter() {
            if let Ok(mut pose) = poses.get_mut(c) {
                *pose = solved.pose.clone();
            }
        }
        // Предметы в руках: точка хвата — кулак, ось — заданная или вдоль предплечья.
        let grips = [(gait.right, solved.right_grip, look.input.right_axis), (gait.left, solved.left_grip, look.input.left_axis)];
        for (k, (slot, at, axis)) in grips.into_iter().enumerate() {
            let Ok((mut st, mut sv)) = slots.get_mut(slot) else { continue };
            let kind = look.props[k];
            gait.kinds[k] = kind;
            let v = if kind.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            if *sv != v {
                *sv = v;
            }
            if kind.is_some() {
                let axis = axis.unwrap_or(Vec3::Y);
                st.translation = at;
                st.rotation = Quat::from_rotation_arc(Vec3::Y, axis.normalize_or(Vec3::Y));
            }
        }
    }
}

/// Дальше этого жителей не рисуем (их сотни, а видно — десятки).
const VIEW_DIST: f32 = 60.0;

fn r_phase(i: usize) -> f32 {
    i as f32 * 2.31
}

fn fallback_act(role: Role) -> Act {
    match role {
        Role::Craftsman | Role::Weaver | Role::Cook | Role::Scholar | Role::Student => Act::Work,
        _ => Act::Idle,
    }
}

/// Какой предмет сейчас в руке (на сущности с `PropSlot`); меняется вместе с занятием.
/// Тред графики вешает свет на `Prop::Torch`.
#[derive(Component)]
pub struct ShownProp(pub Option<Prop>);

fn update_props(mut commands: Commands, kit: Option<Res<PropKit>>, gaits: Query<&Gait>, shown: Query<Option<&ShownProp>, With<PropSlot>>) {
    let Some(kit) = kit else { return };
    for g in &gaits {
        for (k, slot) in [g.right, g.left].into_iter().enumerate() {
            let Ok(cur) = shown.get(slot) else { continue };
            let want = g.kinds[k];
            if cur.map(|c| c.0) == Some(want) {
                continue;
            }
            let mut e = commands.entity(slot);
            e.despawn_related::<Children>().insert(ShownProp(want));
            if let Some(parts) = want.and_then(|p| kit.parts.get(&p)) {
                e.with_children(|c| {
                    for (mesh, mat, tf) in &parts.0 {
                        c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), *tf, bevy::light::NotShadowCaster));
                    }
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Разговоры соседей и часы
// ---------------------------------------------------------------------------------------------

/// Беседы: по очереди реплики собеседников. Темы — заботы Херсона начала VI века.
const TALKS: &[&[&str]] = &[
    &["Слыхал? Пшеницу с Боспора опять задержали.", "Значит, хлеб подорожает. Как всегда к зиме.", "Хорошо хоть рыба своя.", "Рыба-то своя, а соль привозная…"],
    &["Говорят, гунны кочуют у самого Боспора.", "Стены у нас крепкие, Зенон не зря чинил.", "Стены крепкие, а виноградники за стенами.", "Вот за них и страшно."],
    &["В базилике новый диакон, молодой совсем.", "Голос у него хороший, на утрене слышал.", "Лишь бы не из тех, что с монофиситами спорят.", "Император сам им благоволит, тише ты."],
    &["Корабль из Синопы пришёл, вино и масло.", "А пошлину портовые уже насчитали?", "Десятую часть, как положено.", "Грабёж, а не пошлина."],
    &["Готы с гор привезли мёд и шкуры.", "Меняют на что?", "На соль и на ткани. Наши ткани им по нраву.", "Пусть меняют, лишь бы мирно."],
    &["Сын у меня в ученье у кузнеца.", "Дело доброе. Кузнец без хлеба не останется.", "Только руки все в ожогах.", "Зато ремеслу научится."],
    &["Видел, сколько солёной рыбы грузят в порту?", "Тарихос в столице ценят, вот и грузят.", "А нам самим что останется?", "Хвосты да головы, как обычно."],
    &["Врач из ксенона, говорят, учился в Александрии.", "Потому и берёт дорого.", "Зато руку моему брату вправил.", "Ну, тогда не жалко."],
    &["Аланы на торг пригнали коней.", "Хорошие кони, низкие, выносливые.", "Комит, говорят, купил двух.", "Комиту можно, у него жалованье."],
    &["Опять жалованье гарнизону задерживают.", "Из Константинополя золото везти — не близко.", "Солдат голодный — плохой солдат.", "Лишь бы не разбежались, как в тот раз."],
];

const GREETINGS: &[&str] = &["Мир тебе.", "Доброго дня.", "Здравствуй, путник.", "Посторонись-ка.", "Бог в помощь."];
const NIGHT_GREETINGS: &[&str] = &["Поздно бродишь, путник.", "Ночью по городу ходить — к беде.", "Иди с Богом, да смотри под ноги."];
const RAIN_GREETINGS: &[&str] = &["Ну и льёт…", "Промок до нитки.", "Дождь, а работы никто не отменял."];

#[derive(Component)]
struct Bubble(usize);
#[derive(Component)]
struct BubbleText(usize);
#[derive(Component)]
struct ClockText;
#[derive(Component)]
struct ClockRoot;

const BUBBLES: usize = 5;

fn setup_chatter(mut commands: Commands, theme: Res<crate::ui_theme::UiTheme>) {
    use crate::ui_theme::Skin;
    let font = |size: f32| TextFont { font: theme.fonts.regular.clone().into(), font_size: bevy::text::FontSize::Px(size), ..default() };
    for k in 0..BUBBLES {
        commands
            .spawn((
                Bubble(k),
                Node { position_type: PositionType::Absolute, max_width: Val::Px(340.0), padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)), ..default() },
                theme.skin(Skin::Leather),
                Visibility::Hidden,
            ))
            .with_children(|p| {
                p.spawn((BubbleText(k), Text::new(""), font(15.0), TextColor(Skin::Leather.text()), TextLayout::justify(Justify::Center)));
            });
    }
    commands
        .spawn((Node { position_type: PositionType::Absolute, right: Val::Px(12.0), top: Val::Px(8.0), padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)), ..default() }, theme.skin(Skin::Leather), ClockRoot, Visibility::Hidden))
            .with_children(|p| {
                p.spawn((ClockText, Text::new(""), font(16.0), TextColor(Skin::Leather.title())));
            });
}

/// Реплики над головами: беседующие соседи и прохожие, которые здороваются с игроком.
#[allow(clippy::type_complexity)]
fn chatter(
    time: Res<Time>,
    clock: Res<WorldClock>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    people: Query<(&ResidentView, &Transform, &Visibility), Without<crate::civil_client::Fallen>>,
    acts: Res<LiveActs>,
    mut bubbles: Query<(&Bubble, &mut Node, &ComputedNode, &mut Visibility), Without<ResidentView>>,
    mut texts: Query<(&BubbleText, &mut Text)>,
    mut greeted: Local<std::collections::HashMap<usize, f32>>,
) {
    let t = time.elapsed_secs();
    let Some((cam, cam_tf)) = camera.iter().find(|(c, _)| c.is_active) else { return };
    let eye = cam_tf.translation();
    let plans = daily_life::plans(clock.day());
    // (расстояние, точка над головой, текст)
    let mut lines: Vec<(f32, Vec3, String)> = Vec::new();
    for (r, tf, vis) in &people {
        if *vis == Visibility::Hidden {
            continue;
        }
        let d = tf.translation.distance(eye);
        if d > 13.0 {
            continue;
        }
        let act = acts.0.get(r.index).copied().unwrap_or(Act::Idle);
        let head = tf.translation + Vec3::Y * 2.05 * tf.scale.y;
        if act == Act::Talk {
            let partner = plans.as_ref().and_then(|p| p.people.get(r.index)).and_then(|p| p.state(clock.of_day())).and_then(|s| s.partner);
            let Some(p) = partner else { continue };
            if !speaking(r.index, Some(p), t) {
                continue;
            }
            let pair = (r.index.min(p as usize) as u64) << 16 | r.index.max(p as usize) as u64;
            let topic = TALKS[(crate::world_clock::hash(pair, clock.day() as u64) % TALKS.len() as u64) as usize];
            let turn = (t / 5.0) as usize;
            lines.push((d, head, format!("{}: «{}»", crate::residents::title(r.role), topic[turn % topic.len()])));
        } else if d < 3.2 && !matches!(act, Act::Sleep | Act::Lie | Act::Cower | Act::Away) {
            // Прохожий здоровается раз в минуту.
            let last = greeted.get(&r.index).copied().unwrap_or(-100.0);
            if t - last > 60.0 {
                greeted.insert(r.index, t);
            }
            if t - greeted[&r.index] < 3.0 {
                let pool = if clock.rain() > 0.2 {
                    RAIN_GREETINGS
                } else if clock.night() {
                    NIGHT_GREETINGS
                } else {
                    GREETINGS
                };
                lines.push((d, head, pool[(r.index + clock.day() as usize) % pool.len()].to_string()));
            }
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Реплики, что на экране налезли бы на ближнюю, не показываем.
    let mut placed: Vec<(Vec2, String)> = Vec::new();
    for (_, at, line) in lines {
        let Ok(px) = cam.world_to_viewport(cam_tf, at) else { continue };
        if placed.iter().any(|(p, _)| (p.y - px.y).abs() < 34.0 && (p.x - px.x).abs() < 260.0) {
            continue;
        }
        placed.push((px, line));
        if placed.len() >= BUBBLES {
            break;
        }
    }
    let mut shown = 0;
    for (b, mut node, computed, mut vis) in &mut bubbles {
        let Some((px, line)) = placed.get(b.0) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        let px = *px;
        if let Some((_, mut text)) = texts.iter_mut().find(|(t, _)| t.0 == b.0)
            && text.0 != *line
        {
            text.0 = line.clone();
        }
        let size = computed.size() * computed.inverse_scale_factor();
        node.left = Val::Px(px.x - size.x * 0.5);
        node.top = Val::Px(px.y - size.y);
        *vis = Visibility::Inherited;
        shown += 1;
    }
    let _ = shown;
}

fn clock_hud(clock: Res<WorldClock>, mut text: Single<&mut Text, With<ClockText>>, mut root: Single<&mut Visibility, With<ClockRoot>>) {
    if !clock.synced || crate::level::active() != crate::level::LevelKind::Chersonesus {
        return;
    }
    if **root == Visibility::Hidden && std::env::var("MRPG_CAM").is_err() {
        **root = Visibility::Inherited;
    }
    let rain = if clock.rain() > 0.1 { " · дождь" } else { "" };
    let line = format!("{} · {}{}", clock.hhmm(), crate::world_clock::part_of_day(clock.hour()), rain);
    if text.0 != line {
        text.0 = line;
    }
}

// ---------------------------------------------------------------------------------------------
// Дождь
// ---------------------------------------------------------------------------------------------

/// Коробка струй дождя вокруг камеры (две друг над другом, едут вниз и меняются местами).
#[derive(Component)]
struct RainBox(f32);

#[derive(Resource)]
struct RainLook(Handle<StandardMaterial>);

const RAIN_H: f32 = 22.0;
const RAIN_R: f32 = 24.0;

fn setup_rain(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    // Струи: косые крестики из двух тонких полосок.
    for k in 0..7000u64 {
        let x = (crate::world_clock::unit(k, 1) * 2.0 - 1.0) * RAIN_R;
        let z = (crate::world_clock::unit(k, 2) * 2.0 - 1.0) * RAIN_R;
        let y = crate::world_clock::unit(k, 3) * RAIN_H;
        let len = 0.5 + 0.4 * crate::world_clock::unit(k, 4);
        let top = Vec3::new(x, y, z);
        let bot = top + Vec3::new(0.06, -len, 0.03);
        for side in [Vec3::new(0.006, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.006)] {
            let b = pos.len() as u32;
            for p in [top - side, top + side, bot + side, bot - side] {
                pos.push(p.to_array());
                nrm.push([0.0, 1.0, 0.0]);
            }
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_indices(Indices::U32(idx));
    let mesh = meshes.add(mesh);
    let mat = mats.add(StandardMaterial { base_color: Color::srgba(0.78, 0.82, 0.88, 0.0), unlit: true, alpha_mode: AlphaMode::Blend, double_sided: true, cull_mode: None, ..default() });
    for k in 0..2 {
        commands.spawn((RainBox(k as f32), Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster));
    }
    commands.insert_resource(RainLook(mat));
}

fn rain(
    time: Res<Time>,
    clock: Res<WorldClock>,
    look: Option<Res<RainLook>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut boxes: Query<(&RainBox, &mut Transform, &mut Visibility)>,
) {
    let r = if crate::level::active() == crate::level::LevelKind::Chersonesus { clock.rain() } else { 0.0 };
    let Some(cam) = camera.iter().find(|(c, _)| c.is_active).map(|(_, t)| t.translation()) else { return };
    let want = if r > 0.02 { Visibility::Inherited } else { Visibility::Hidden };
    if let Some(look) = look
        && let Some(mut m) = mats.get_mut(&look.0)
    {
        let a = 0.45 * r;
        if (m.base_color.alpha() - a).abs() > 0.01 {
            m.base_color.set_alpha(a);
        }
    }
    // Падают 9 м/с; коробки сдвигаются по кругу, рисунок повторяется без скачков.
    let fall = (time.elapsed_secs() * 9.0).rem_euclid(RAIN_H);
    for (b, mut tf, mut vis) in &mut boxes {
        if *vis != want {
            *vis = want;
        }
        tf.translation = Vec3::new(cam.x, cam.y - 4.0 - fall + b.0 * RAIN_H, cam.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_act_has_a_pose() {
        let acts = [
            Act::Sleep,
            Act::Away,
            Act::Idle,
            Act::Work,
            Act::Hammer,
            Act::Stir,
            Act::Weave,
            Act::Write,
            Act::Teach,
            Act::Treat,
            Act::Lie,
            Act::Pray,
            Act::Preach,
            Act::Talk,
            Act::Browse,
            Act::Arrange,
            Act::Eat,
            Act::Drink,
            Act::Carry,
            Act::Fish,
            Act::Sweep,
            Act::Guard,
            Act::Hoe,
            Act::Bathe,
            Act::Cower,
        ];
        for act in acts {
            for t in [0.0, 0.7, 3.1] {
                let s = NpcState { pos: Vec3::ZERO, yaw: 0.0, act, walking: false, walked: 0.0, carry: false, patrol: false, partner: Some(1), lie: None };
                let look = look_for(0, Role::Soldier, &s, t, 0.3, true, 0.0);
                let solved = crate::rig::solve_pose(&look.input);
                assert!(solved.right_grip.is_finite() && solved.left_grip.is_finite(), "{act:?}");
            }
        }
    }

    #[test]
    fn hammer_reaches_anvil_height() {
        let s = NpcState { pos: Vec3::ZERO, yaw: 0.0, act: Act::Hammer, walking: false, walked: 0.0, carry: false, patrol: false, partner: None, lie: None };
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for k in 0..50 {
            let look = look_for(0, Role::Craftsman, &s, k as f32 * 0.05, 0.0, false, 0.0);
            let g = crate::rig::solve_pose(&look.input).right_grip;
            low = low.min(g.y);
            high = high.max(g.y);
        }
        assert!(low < 1.1 && high > 1.4, "молот ходит от {low} до {high}");
    }
}
