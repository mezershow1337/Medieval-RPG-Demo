//! Клиент: живые жители. Ставит каждого туда, где он по распорядку (`daily_life.rs`), играет
//! позу занятия (кузнец бьёт молотом, хозяйка мешает в котле, рыбак держит удочку, священник
//! воздевает руки), даёт в руки инструмент, ночью — факел, укладывает спать в постель.
//! Соседи на агоре и в таверне беседуют между собой — реплики видно над головами вблизи.
//!
//! Производительность: положение берётся из плана за пару сравнений, позу считаем
//! каждый кадр только вблизи (дальше 22 м — через кадр, дальше 38 м — раз в 4 кадра), дальше
//! 60 м жителей не рисуем. Не рисуем и тех, кто вне кадра (за спиной, сбоку) вместе со своей
//! тенью, кроме ближних (12 м: их голос рядом); место жителя меняем, только когда он сдвинулся.

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
            .init_resource::<ResidentSpots>()
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

/// Где сейчас каждый житель на улице или в доме (для мини-карты); `None` — ушёл из города или погиб.
/// Считается для всех, не только для видимых вблизи.
#[derive(Resource, Default)]
pub struct ResidentSpots(pub Vec<Option<Vec2>>);

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
    /// Восковая табличка ученика и стиль.
    Tablet,
    Stylus,
    /// Клещи с раскалённой заготовкой.
    HotIron,
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
    let wax = mat(Color::srgb(0.2, 0.12, 0.05), 0.5, 0.0);
    let bronze = mat(Color::srgb(0.62, 0.45, 0.24), 0.4, 0.8);
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
    parts.insert(
        Prop::Tablet,
        PropParts(vec![(m(Cuboid::new(0.26, 0.018, 0.17).into()), wood.clone(), Transform::IDENTITY), (m(Cuboid::new(0.23, 0.006, 0.14).into()), wax, at(0.011))]),
    );
    parts.insert(Prop::Stylus, PropParts(vec![(m(Cylinder::new(0.004, 0.14).into()), bronze, at(0.04))]));
    // Клещи и в губках — светящаяся полоса железа.
    let hot = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.45, 0.12), emissive: LinearRgba::rgb(7.0, 1.8, 0.25), ..default() });
    parts.insert(
        Prop::HotIron,
        PropParts(vec![
            (m(Cuboid::new(0.02, 0.5, 0.03).into()), iron.clone(), at(0.15)),
            (m(Cuboid::new(0.028, 0.2, 0.028).into()), hot, at(0.47)),
        ]),
    );
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
        i.walk_phase = s.walked / crate::rig::step_cycle(1.3) * TAU + phase;
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
        } else if s.act == Act::Sweep {
            // Метёт на ходу: метла ходит из стороны в сторону перед ногами.
            let k = (tt * 1.4).sin();
            i.lean = -0.18;
            i.pitch = -0.45;
            i.twist = 0.15 * k;
            let r = Vec3::new(0.12 + 0.1 * k, 0.95, -0.2);
            let l = Vec3::new(0.04 + 0.08 * k, 1.28, -0.14);
            i.right_grip = Some(r);
            i.left_grip = Some(l);
            let axis = (l - r).normalize();
            i.right_axis = Some(axis);
            i.left_axis = Some(axis);
            props[0] = Some(Prop::Broom);
        } else if matches!(s.act, Act::Stoke | Act::Quench) {
            // Несёт клещами раскалённую заготовку от горна к наковальне или к корыту.
            i.left_grip = grip(-0.14, 1.0, -0.42);
            i.left_axis = Some(Vec3::new(0.15, 0.25, -1.0).normalize());
            props[1] = Some(Prop::HotIron);
        }
        if night && !s.carry && props[1].is_none() && s.act != Act::Sweep {
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
        Act::Write if role == Role::Student => {
            // Ученик сидит на скамье, табличка на коленях, стиль в правой руке.
            i.crouch = 0.82;
            i.lean = 0.12;
            i.pitch = -0.75;
            let k = (tt * 3.1).sin();
            i.left_grip = grip(-0.1, 0.6, -0.34);
            i.left_axis = Some(Vec3::new(1.0, 0.0, 0.0));
            i.right_grip = grip(0.06 + 0.03 * k, 0.66, -0.36 + 0.015 * (tt * 2.2).cos());
            i.right_axis = Some(Vec3::new(-0.2, -0.6, -0.5).normalize());
            props = [Some(Prop::Stylus), Some(Prop::Tablet)];
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
        Act::Fetch => {
            // Присел у сундука или пифоса: перебирает, достаёт, укладывает.
            let k = (tt * 0.9).sin();
            let reach = ((tt * 0.45).sin() * 1.4).clamp(0.0, 1.0);
            i.crouch = 0.7;
            i.lean = -0.3;
            i.pitch = -0.65;
            i.twist = 0.1 * k;
            i.right_grip = Some(Vec3::new(0.12, 0.62, -0.32).lerp(Vec3::new(0.08 + 0.05 * k, 0.42, -0.56), reach));
            i.left_grip = Some(Vec3::new(-0.12, 0.6, -0.36).lerp(Vec3::new(-0.08, 0.44, -0.54), reach * 0.7));
        }
        Act::Tend => {
            // У очага на корточках: подкладывает полено, ворошит угли.
            let p = (tt / 3.2).fract();
            let put = if p < 0.5 { p / 0.5 } else { 1.0 - (p - 0.5) / 0.5 };
            i.crouch = 0.9;
            i.lean = -0.18;
            i.pitch = -0.55;
            i.right_grip = Some(Vec3::new(0.16, 0.52, -0.3).lerp(Vec3::new(0.06, 0.36, -0.62), put));
            i.left_grip = grip(-0.2, 0.5, -0.2);
        }
        Act::Stoke => {
            // Левой держит клещами заготовку в углях, правой качает мехи.
            let pump = ((tt * 1.7).sin() * 0.5 + 0.5).powf(1.3);
            i.lean = -0.16;
            i.pitch = -0.5;
            i.twist = -0.08;
            i.left_grip = grip(-0.1, 0.98, -0.5);
            i.left_axis = Some(Vec3::new(0.1, -0.35, -1.0).normalize());
            i.right_grip = Some(Vec3::new(0.34, 1.12, -0.22).lerp(Vec3::new(0.34, 0.86, -0.26), pump));
            props[1] = Some(Prop::HotIron);
        }
        Act::Quench => {
            // Опускает поковку в корыто и водит в воде.
            let k = (tt * 2.4).sin();
            i.lean = -0.3;
            i.pitch = -0.7;
            i.left_grip = grip(-0.08 + 0.03 * k, 0.72, -0.5);
            i.left_axis = Some(Vec3::new(0.0, -1.0, -0.35).normalize());
            i.right_grip = grip(0.2, 0.9, -0.2);
            props[1] = Some(Prop::HotIron);
        }
        Act::Sit => {
            // Сидит прямо, руки на коленях; изредка говорит, поводя правой рукой.
            i.crouch = 0.82;
            i.lean = 0.08;
            i.pitch = 0.05 * (tt * 0.17).sin();
            i.twist = 0.1 * (tt * 0.11).sin();
            let g = (((tt / 11.0).fract() * TAU).sin() * 1.6 - 0.6).clamp(0.0, 1.0);
            i.right_grip = Some(Vec3::new(0.16, 0.62, -0.3).lerp(Vec3::new(0.3, 1.0, -0.42), g));
            i.left_grip = grip(-0.16, 0.62, -0.3);
        }
        Act::Dine => {
            // Сидит за трапезой: то тянется к блюду, то подносит чашу.
            i.crouch = 0.82;
            i.lean = -0.05;
            let up = ((tt / 6.0).fract() * TAU).sin().max(0.0).powf(0.6);
            let reach = ((tt / 6.0 + 0.5).fract() * TAU).sin().max(0.0);
            i.pitch = 0.2 * up - 0.25;
            i.right_grip = Some(Vec3::new(0.16, 0.7, -0.36).lerp(Vec3::new(0.05, 1.08, -0.13), up));
            i.right_axis = Some(Vec3::Y.lerp(Vec3::new(0.0, 0.3, 1.0), up).normalize());
            i.left_grip = Some(Vec3::new(-0.16, 0.64, -0.3).lerp(Vec3::new(-0.08, 0.66, -0.55), reach));
            props[0] = Some(Prop::Cup);
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

/// Учитель на своём месте (писец, нотарий, сочинитель): объясняет руками и ждёт учеников.
pub fn teacher_pose(t: f32, phase: f32) -> PoseInput {
    let cycle = ((t + phase * 7.0) / 24.0).fract();
    let act = if cycle < 0.45 { Act::Teach } else { Act::Idle };
    let s = NpcState { pos: Vec3::ZERO, yaw: 0.0, act, walking: false, walked: 0.0, carry: false, patrol: false, partner: None, lie: None };
    look_for(0, Role::Scholar, &s, t, phase, false, 0.0).input
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
    mut spots: ResMut<ResidentSpots>,
    level: Res<crate::collision::LevelColliders>,
    camera: Query<(&Camera, &GlobalTransform, &Projection), With<Camera3d>>,
    mut people: Query<(&ResidentView, &mut Transform, &mut Visibility, &Children, &mut Gait, Has<crate::civil_client::Fallen>)>,
    mut poses: Query<&mut RigPose>,
    mut slots: Query<(&mut Transform, &mut Visibility), (With<PropSlot>, Without<ResidentView>)>,
    mood: Option<Res<crate::daylight::CurrentMood>>,
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
    let view = camera.iter().find(|(c, _, _)| c.is_active).map(|(_, t, p)| ViewCone::new(t, p, shadow_light(mood.as_deref())));
    let town = daily_life::town();
    let n = people.iter().len();
    if acts.0.len() != n {
        acts.0 = vec![Act::Idle; n];
    }
    if spots.0.len() != n {
        spots.0 = vec![None; n];
    }
    for (r, mut tf, mut vis, children, mut gait, fallen) in &mut people {
        let i = r.index;
        let scale = tf.scale.y;
        let anchor = town.and_then(|t| t.residents.get(i)).map(|x| (x.pos, x.yaw));
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
        if let Some(spot) = spots.0.get_mut(i) {
            *spot = (!fallen && s.act != Act::Away).then(|| s.pos.xz());
        }
        if fallen {
            continue;
        }
        let dist = view.map_or(0.0, |v| v.eye.distance(s.pos));
        // Вне кадра (за спиной, сбоку) не рисуем и не считаем позу, если и тень его не в кадре;
        // факел виден по свету дальше — что у жителя в руках, смотрим, только если он рядом с кадром.
        let in_view = |reach: f32| view.is_none_or(|v| dist < ALWAYS_NEAR || v.sees_person(s.pos, dist, reach));
        let look = (dist <= VIEW_DIST && s.act != Act::Away && in_view(TORCH_REACH)).then(|| look_for(i, r.role, &s, t, r_phase(i), night, rain));
        let seen = look.as_ref().is_some_and(|l| l.props.contains(&Some(Prop::Torch)) || in_view(BODY_REACH));
        let want = if seen { Visibility::Inherited } else { Visibility::Hidden };
        vis.set_if_neq(want);
        let Some(look) = look.filter(|_| seen) else {
            gait.fresh = true;
            continue;
        };
        // Поворот: плавно, кроме первого кадра и скачков.
        let jump = tf.translation.distance(s.pos) > 3.0;
        if gait.fresh || jump {
            gait.yaw = s.yaw;
        } else {
            let d = angle_diff(gait.yaw, s.yaw);
            gait.yaw += d.clamp(-5.0 * dt, 5.0 * dt);
        }
        // Дальних обновляем реже (поза и предметы), но место — каждый кадр. Спящие дышат
        // медленно — им хватает и реже.
        let every = if dist < 22.0 {
            1
        } else if dist < 38.0 {
            2
        } else {
            4
        };
        let every = if look.lying && dist > 6.0 { every.max(4) } else { every };
        let pose_now = gait.fresh || (*frame + i as u32).is_multiple_of(every);
        // Пишем, только если сдвинулся: иначе Bevy каждый кадр пересчитывал бы все кости
        // и одежду под ним, даже в кадры, когда поза не менялась.
        let placed = if look.lying { lying_transform(&s, scale) } else { Transform { translation: s.pos, rotation: Quat::from_rotation_y(gait.yaw), scale: tf.scale } };
        tf.set_if_neq(placed);
        gait.fresh = false;
        if !pose_now {
            continue;
        }
        let solved = crate::rig::solve_pose(&look.input);
        for c in children.iter() {
            if let Ok(mut pose) = poses.get_mut(c) {
                *pose = solved.pose;
            }
        }
        // Предметы в руках: точка хвата — кулак, ось — заданная или вдоль предплечья.
        let grips = [(gait.right, solved.right_grip, look.input.right_axis), (gait.left, solved.left_grip, look.input.left_axis)];
        for (k, (slot, at, axis)) in grips.into_iter().enumerate() {
            let Ok((mut st, mut sv)) = slots.get_mut(slot) else { continue };
            let kind = look.props[k];
            if gait.kinds[k] != kind {
                gait.kinds[k] = kind;
            }
            sv.set_if_neq(if kind.is_some() { Visibility::Inherited } else { Visibility::Hidden });
            if kind.is_some() {
                let axis = axis.unwrap_or(Vec3::Y);
                st.translation = at;
                st.rotation = Quat::from_rotation_arc(Vec3::Y, axis.normalize_or(Vec3::Y));
            }
        }
    }
}

/// Ближе этого житель виден всегда, куда бы ни смотрела камера: его голос рядом.
pub(crate) const ALWAYS_NEAR: f32 = 12.0;
/// Радиус «шара» тела жителя для проверки кадра.
pub(crate) const BODY_REACH: f32 = 2.5;
/// С факелом — до края его света (`night_lights::TORCH_RANGE`).
const TORCH_REACH: f32 = 10.0;
/// Запас угла кадра: камера успевает повернуться за кадр, а поза считается до поворота.
const CULL_MARGIN: f32 = 0.2;
/// Дальше тень человека в расчёт не берём: её дальний конец при низком солнце бледен.
const SHADOW_MAX: f32 = 12.0;

/// Откуда падает свет, дающий тени (солнце или луна), если тени сейчас есть.
pub(crate) fn shadow_light(mood: Option<&crate::daylight::CurrentMood>) -> Option<Vec3> {
    // Под сплошными тучами солнце теней не даёт (`daylight::drive_daylight`).
    mood.map(|m| m.0).filter(|m| m.overcast < 0.6).map(|m| m.sun)
}

/// Что видит камера (конус вокруг взгляда). Жителю вне конуса не считаем позу, и он не
/// рисуется: иначе каждый из сотен жителей в 60 м каждый кадр получал бы новую позу, кости
/// и кожу, даже за спиной камеры.
#[derive(Clone, Copy)]
pub(crate) struct ViewCone {
    pub eye: Vec3,
    fwd: Vec3,
    /// Косинус и синус половины угла конуса (по диагонали кадра, с запасом).
    cos: f32,
    sin: f32,
    /// Куда и как далеко от ног ложится тень человека.
    shadow: Vec3,
}

impl ViewCone {
    pub fn new(tf: &GlobalTransform, proj: &Projection, light: Option<Vec3>) -> Self {
        let half = match proj {
            Projection::Perspective(p) => ((p.fov * 0.5).tan() * (1.0 + p.aspect_ratio * p.aspect_ratio).sqrt()).atan(),
            _ => PI,
        };
        let a = (half + CULL_MARGIN).min(PI);
        // Тень ложится от ног прочь от света: чем ниже солнце, тем она длиннее.
        let shadow = light.map_or(Vec3::ZERO, |l| {
            let flat = Vec3::new(l.x, 0.0, l.z);
            let len = (1.8 * flat.length() / l.y.max(0.05)).min(SHADOW_MAX);
            -flat.normalize_or_zero() * len
        });
        ViewCone { eye: tf.translation(), fwd: tf.forward().as_vec3(), cos: a.cos(), sin: a.sin(), shadow }
    }

    /// Виден ли человек, стоящий в `feet` (`dist` — до него от камеры), или его тень.
    pub fn sees_person(&self, feet: Vec3, dist: f32, reach: f32) -> bool {
        if self.sees(feet + Vec3::Y, dist, reach) {
            return true;
        }
        let half = self.shadow * 0.5;
        half != Vec3::ZERO && self.sees(feet + half, self.eye.distance(feet + half), reach + half.length())
    }

    /// Задевает ли конус шар радиуса `r` вокруг `p` (`dist` — от камеры до `p`, примерно).
    pub fn sees(&self, p: Vec3, dist: f32, r: f32) -> bool {
        if self.cos <= -0.999 || dist <= r {
            return true;
        }
        let d = p - self.eye;
        let along = d.dot(self.fwd);
        let side = (d.length_squared() - along * along).max(0.0).sqrt();
        // Расстояние от центра шара до боковой поверхности конуса (со знаком: внутри — меньше нуля).
        side * self.cos - along * self.sin < r
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
            // Двор комита говорит о своём: свои разговоры, реплики по очереди.
            if let (Some(a), Some(b)) = (crate::court::of_resident(r.index), crate::court::of_resident(p as usize))
                && let Some(talk) = crate::court::overheard(a, b)
            {
                let (who, line) = talk[(t / 5.0) as usize % talk.len()];
                if who == a {
                    lines.push((d, head, format!("{}: «{line}»", a.name())));
                }
                continue;
            }
            if !speaking(r.index, Some(p), t) {
                continue;
            }
            let pair = (r.index.min(p as usize) as u64) << 16 | r.index.max(p as usize) as u64;
            let topic = TALKS[(crate::world_clock::hash(pair, clock.day() as u64) % TALKS.len() as u64) as usize];
            let turn = (t / 5.0) as usize;
            lines.push((d, head, format!("{}: «{}»", crate::residents::name_of(r.index, r.role), topic[turn % topic.len()])));
        } else if d < 4.5
            && let Some(c) = crate::court::of_resident(r.index)
            && let Some(line) = crate::court::bark(c, &crate::court::Ctx { hour: clock.hour(), day: clock.day(), met: (t / 45.0) as u32, act, rain: clock.rain() > 0.2 })
        {
            // Двор комита: свои присказки (стража у ворот, препозит в час приёма, кухарка).
            let last = greeted.get(&r.index).copied().unwrap_or(-100.0);
            if t - last > 45.0 {
                greeted.insert(r.index, t);
            }
            if t - greeted[&r.index] < 3.5 {
                lines.push((d, head, format!("{}: «{line}»", c.name())));
            }
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
        // Реплика того, кто у края кадра, не обрезается краем экрана.
        let screen = cam.logical_viewport_size().unwrap_or(Vec2::splat(f32::MAX));
        // Меняем узел, только если реплика сдвинулась: любое изменение `Node` — пересчёт
        // раскладки всего интерфейса в этом кадре.
        let left = Val::Px((px.x - size.x * 0.5).clamp(8.0, (screen.x - size.x - 8.0).max(8.0)).round());
        let top = Val::Px((px.y - size.y).clamp(8.0, (screen.y - size.y - 8.0).max(8.0)).round());
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
        vis.set_if_neq(Visibility::Inherited);
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
    let w = clock.weather();
    let rain = if w.rain > 0.1 {
        " · дождь"
    } else if w.fog > 0.35 {
        " · туман"
    } else if w.wind_speed() > 9.0 {
        " · ветрено"
    } else {
        ""
    };
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

const RAIN_H: f32 = 14.0;
const RAIN_R: f32 = 16.0;

fn setup_rain(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    // Капли: короткие тонкие чёрточки (крестик из двух полосок), густо; у самой камеры — пусто,
    // чтобы капля не перечёркивала весь кадр.
    for k in 0..36000u64 {
        let x = (crate::world_clock::unit(k, 1) * 2.0 - 1.0) * RAIN_R;
        let z = (crate::world_clock::unit(k, 2) * 2.0 - 1.0) * RAIN_R;
        if x * x + z * z < 2.2 * 2.2 {
            continue;
        }
        let y = crate::world_clock::unit(k, 3) * RAIN_H;
        let len = 0.16 + 0.2 * crate::world_clock::unit(k, 4);
        let top = Vec3::new(x, y, z);
        let bot = top + Vec3::new(0.01, -len, 0.005);
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
    mood: Option<Res<crate::daylight::CurrentMood>>,
    look: Option<Res<RainLook>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut boxes: Query<(&RainBox, &mut Transform, &mut Visibility)>,
) {
    let m = mood.as_ref().map(|m| m.0);
    let r = m.filter(|_| crate::level::active() == crate::level::LevelKind::Chersonesus).map_or(0.0, |m| m.rain);
    let Some(cam) = camera.iter().find(|(c, _)| c.is_active).map(|(_, t)| t.translation()) else { return };
    let want = if r > 0.02 { Visibility::Inherited } else { Visibility::Hidden };
    if let Some(look) = look {
        // Струи светятся сами (unlit): ночью они лишь чуть светлее тьмы.
        let day = m.map_or(1.0, |md| md.dayness.max(1.0 - md.night));
        let lit = 0.16 + 0.66 * day;
        let c = Color::srgba(0.95 * lit, lit, (1.07 * lit).min(1.0), 0.75 * r);
        // `get_mut` сам по себе помечает материал изменённым — и он заново уходит в видеокарту
        // каждый кадр, даже в ясную погоду. Поэтому сначала смотрим, а правим только при смене.
        let stale = mats.get(&look.0).is_some_and(|mat| {
            let old = mat.base_color.to_srgba();
            (old.red - c.to_srgba().red).abs() + (old.alpha - c.alpha()).abs() > 0.01
        });
        if stale && let Some(mut mat) = mats.get_mut(&look.0) {
            mat.base_color = c;
        }
    }
    // Падают 9 м/с и сносятся ветром; коробки сдвигаются по кругу, рисунок повторяется без скачков.
    let wind = m.map_or(Vec2::ZERO, |m| m.wind) * 0.45;
    let dir = Vec3::new(wind.x, -9.0, wind.y).normalize();
    let tilt = Quat::from_rotation_arc(Vec3::NEG_Y, dir);
    let fall = (time.elapsed_secs() * 9.0).rem_euclid(RAIN_H);
    for (b, mut tf, mut vis) in &mut boxes {
        if *vis != want {
            *vis = want;
        }
        // Без дождя коробки спрятаны: двигать их за камерой незачем.
        if want == Visibility::Hidden {
            continue;
        }
        tf.rotation = tilt;
        tf.translation = cam + tilt * Vec3::new(0.0, -4.0 - fall + b.0 * RAIN_H, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Камера в начале координат смотрит вдоль −Z (обычный кадр 16:9).
    fn cone(light: Option<Vec3>) -> ViewCone {
        let tf = GlobalTransform::from(Transform::from_xyz(0.0, 1.7, 0.0).looking_to(Vec3::NEG_Z, Vec3::Y));
        let proj = Projection::Perspective(PerspectiveProjection { aspect_ratio: 16.0 / 9.0, ..default() });
        ViewCone::new(&tf, &proj, light)
    }

    fn sees(v: &ViewCone, feet: Vec3) -> bool {
        v.sees_person(feet, v.eye.distance(feet), BODY_REACH)
    }

    #[test]
    fn view_cone_keeps_frame_and_drops_back() {
        let v = cone(None);
        assert!(sees(&v, Vec3::new(0.0, 0.0, -30.0)), "прямо впереди");
        assert!(sees(&v, Vec3::new(-25.0, 0.0, -30.0)), "у края кадра");
        assert!(!sees(&v, Vec3::new(0.0, 0.0, 30.0)), "за спиной");
        assert!(!sees(&v, Vec3::new(40.0, 0.0, 5.0)), "сбоку и чуть позади");
    }

    #[test]
    fn view_cone_keeps_those_whose_shadow_is_in_frame() {
        // Низкое солнце светит из-за спины камеры справа: тень человека сбоку ложится в кадр.
        let feet = Vec3::new(22.0, 0.0, -10.0);
        assert!(!sees(&cone(None), feet));
        assert!(sees(&cone(Some(Vec3::new(1.0, 0.15, 0.4).normalize())), feet), "тень тянется в кадр");
        // Высокое солнце: тень короткая, в кадр не попадает.
        assert!(!sees(&cone(Some(Vec3::new(0.2, 1.0, 0.1).normalize())), feet));
    }

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
            Act::Sit,
            Act::Dine,
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
