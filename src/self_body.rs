//! Своё тело от первого лица. Камера сидит в голове, поэтому тело собрано дважды:
//!
//! * «тень» — всё тело целиком, с оружием в руках и щитом, в отдельном слое `SHADOW_LAYER`:
//!   его видит только солнце (тень на земле и стенах), камера его не рисует;
//! * «видимое» — только пояс и ноги (всё выше схлопнуто в точку): глянешь вниз — видны
//!   ноги и сапоги; теней оно не отбрасывает, чтобы не было двойной.
//!
//! Руки с оружием рисует `fight.rs` (рука от первого лица): они кадрированы под камеру.
//! Поза обоих тел считается так же, как у других игроков (`client::solve_fighter_body`).

use crate::client::{BodyMotion, solve_fighter_body};
use crate::combat::{self, WeaponKind};
use crate::fight::LocalFighter;
use crate::gear_look::Gear;
use crate::player::LocalPlayer;
use crate::protocol::ActorKind;
use crate::rig::{self, Bone, NeedsRig, Outfit, Rig, RigPose};
use crate::worn;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

/// Слой «только для теней»: его рисует солнце в карту теней, но не камера игрока.
pub const SHADOW_LAYER: usize = 1;

/// Слои солнца: обычный мир и тени своего тела.
pub fn sun_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, SHADOW_LAYER])
}

/// Как рисовать меши под этой сущностью (ставится на корень, на меши раскладывает
/// `propagate_shade`: модели оружия и одежды досоздаются позже).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shade {
    /// Видно камере, тени нет (руки и оружие от первого лица, видимое тело).
    NoShadow,
    /// Только тень.
    ShadowOnly,
}

/// Меш уже разложен по `Shade`.
#[derive(Component)]
struct Shaded;

/// Своё тело: корень (дочерний к `LocalPlayer`), хранит движение для позы.
#[derive(Component, Default)]
pub struct SelfBody {
    walk_phase: f32,
    walk: f32,
    speed: f32,
    crouch: f32,
    weapon_smooth: Option<(Vec3, Vec3)>,
    shown_gear: Option<Gear>,
}

/// Скелет своего тела: `visible` — видимое камере (только пояс и ноги).
#[derive(Component)]
struct SelfRig {
    visible: bool,
}

/// Оружие в руке тела-тени и что в нём показано.
#[derive(Component)]
struct SelfWeapon(WeaponKind, Option<u16>);

/// Щит на руке тела-тени.
#[derive(Component)]
struct SelfShield(rig::ShieldKind);

pub struct SelfBodyPlugin;

impl Plugin for SelfBodyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                spawn_self_body,
                refresh_self_gear,
                refresh_self_weapon,
                collapse_view_body,
            ),
        )
        .add_systems(
            Update,
            animate_self_body
                .in_set(rig::PoseSet)
                .after(crate::player::PlayerMoveSet),
        )
        .add_systems(PostUpdate, propagate_shade);
    }
}

fn outfit(gear: &Gear, my_id: u64) -> Outfit {
    crate::client::outfit_of(ActorKind::Player, my_id, gear)
}

fn my_id(net: Option<&crate::client::NetState>) -> u64 {
    net.and_then(|n| n.my_id).unwrap_or(0)
}

/// Тело появляется, когда появился свой персонаж.
fn spawn_self_body(
    mut commands: Commands,
    players: Query<(Entity, &LocalFighter), Added<LocalPlayer>>,
    net: Option<Res<crate::client::NetState>>,
) {
    for (e, f) in &players {
        let o = outfit(&f.gear, my_id(net.as_deref()));
        let extras = worn::Extras::of(&f.gear);
        commands.entity(e).with_children(|p| {
            p.spawn((
                SelfBody::default(),
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|b| {
                b.spawn((
                    SelfRig { visible: false },
                    Shade::ShadowOnly,
                    worn::Worn::new(extras),
                    NeedsRig(o),
                    Transform::default(),
                    Visibility::default(),
                ));
                b.spawn((
                    SelfRig { visible: true },
                    Shade::NoShadow,
                    worn::Worn::new(extras),
                    NeedsRig(o),
                    Transform::default(),
                    Visibility::default(),
                ));
                b.spawn((
                    SelfWeapon(WeaponKind::Fists, None),
                    Shade::ShadowOnly,
                    Transform::default(),
                    Visibility::default(),
                ));
            });
        });
    }
}

/// У видимого тела всё выше пояса (торс, голова, руки) схлопнуто в точку: камера внутри
/// головы, руки — свои, от первого лица, а грудь заслоняла бы ноги и лезла в камеру.
/// Масштаб суставов `apply_poses` не трогает, ставим один раз.
fn collapse_view_body(
    rigs: Query<(&Rig, &SelfRig), Added<Rig>>,
    mut joints: Query<&mut Transform>,
) {
    for (rig, s) in &rigs {
        if !s.visible {
            continue;
        }
        for bone in [Bone::Torso] {
            if let Ok(mut t) = joints.get_mut(rig.joint(bone)) {
                t.scale = Vec3::ZERO;
            }
        }
    }
}

/// Надел или снял доспех, щит, повесил меч на пояс — переодеваем оба тела.
fn refresh_self_gear(
    mut commands: Commands,
    local: Query<&LocalFighter>,
    mut bodies: Query<(Entity, &mut SelfBody, &Children)>,
    mut rigs: Query<&mut worn::Worn, With<SelfRig>>,
    shields: Query<(Entity, &SelfShield)>,
    models: Option<Res<rig::CharacterModels>>,
    net: Option<Res<crate::client::NetState>>,
) {
    if models.is_none() {
        return;
    }
    let Ok(f) = local.single() else { return };
    let id = my_id(net.as_deref());
    for (e, mut body, children) in &mut bodies {
        let old = body.shown_gear;
        if old == Some(f.gear) {
            continue;
        }
        body.shown_gear = Some(f.gear);
        let new_outfit = outfit(&f.gear, id);
        let rebuild = old.is_some_and(|g| outfit(&g, id) != new_outfit);
        for child in children.iter() {
            let Ok(mut w) = rigs.get_mut(child) else {
                continue;
            };
            w.want = worn::Extras::of(&f.gear);
            if rebuild {
                w.reset();
                commands
                    .entity(child)
                    .despawn_related::<Children>()
                    .remove::<Rig>()
                    .insert(NeedsRig(new_outfit));
            }
        }
        let want = crate::client::shield_of(ActorKind::Player, f.fighter.has_shield, &f.gear);
        let have = children.iter().find_map(|c| shields.get(c).ok());
        if have.map(|(_, s)| s.0) != want {
            if let Some((s, _)) = have {
                commands.entity(s).despawn();
            }
            if let Some(kind) = want {
                commands.entity(e).with_children(|p| {
                    p.spawn((
                        SelfShield(kind),
                        rig::NeedsShield(kind),
                        Shade::ShadowOnly,
                        Transform::from_xyz(-0.38, 1.0, -0.22),
                        Visibility::default(),
                    ));
                });
            }
        }
    }
}

/// Сменили оружие — меняем модель в руке тела-тени (своя модель предмета).
fn refresh_self_weapon(
    mut commands: Commands,
    local: Query<&LocalFighter>,
    mut shown: Query<(Entity, &mut SelfWeapon)>,
    models: Option<Res<crate::icons::ItemModels>>,
) {
    let Ok(f) = local.single() else { return };
    let kind = f.fighter.weapon;
    let held = f
        .gear
        .main_hand
        .filter(|_| !f.gear.holstered && kind != WeaponKind::Fists);
    for (e, mut w) in &mut shown {
        if (w.0, w.1) != (kind, held) {
            (w.0, w.1) = (kind, held);
            worn::set_weapon_model(
                &mut commands,
                e,
                kind,
                held.map(crate::items::ItemBaseId),
                f.finish,
                models.as_deref(),
            );
        }
    }
}

/// Видимое тело (пояс и ноги) чуть позади камеры: глянешь вниз — ступни под тобой
/// и чуть впереди при шаге.
fn view_body_offset(_pitch: f32) -> Vec3 {
    Vec3::new(0.0, 0.0, 0.03)
}

#[allow(clippy::type_complexity)]
fn animate_self_body(
    time: Res<Time>,
    local: Query<(&LocalPlayer, &LocalFighter)>,
    mut bodies: Query<(&mut SelfBody, &mut Visibility, &Children)>,
    mut rigs: Query<(&SelfRig, &mut RigPose, &mut Transform), Without<SelfShield>>,
    mut weapons: Query<&mut Transform, (With<SelfWeapon>, Without<SelfRig>, Without<SelfShield>)>,
    mut shields: Query<&mut Transform, (With<SelfShield>, Without<SelfRig>, Without<SelfWeapon>)>,
    work: Option<Res<crate::work_client::MyWork>>,
) {
    let Ok((p, f)) = local.single() else { return };
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (mut b, mut vis, children) in &mut bodies {
        // Мёртвый лежит телом на земле (его рисует сервер как труп).
        *vis = if f.fighter.is_dead() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        let flat = Vec3::new(p.velocity.x, 0.0, p.velocity.z);
        let speed = flat.length();
        // Назад — фаза в обратную сторону.
        let local_v = Quat::from_rotation_y(-p.yaw) * flat;
        let back = if local_v.z > 0.3 * speed { -1.0 } else { 1.0 };
        b.speed += (speed.min(9.0) - b.speed) * (1.0 - (-6.0 * dt).exp());
        b.walk_phase += back * speed * dt / rig::step_cycle(b.speed) * std::f32::consts::TAU;
        b.walk_phase = b.walk_phase.rem_euclid(std::f32::consts::TAU * 64.0);
        let walk_target = if p.grounded {
            (speed / 0.9).min(1.0)
        } else {
            b.walk * 0.9
        };
        b.walk += (walk_target - b.walk) * (1.0 - (-8.0 * dt).exp());
        let crouch_target = if p.crouching { 1.0 } else { 0.0 };
        b.crouch += (crouch_target - b.crouch) * (1.0 - (-10.0 * dt).exp());

        let (want_hilt, want_dir) = combat::weapon_pose(&f.fighter.action, f.fighter.weapon, now);
        let k = 1.0 - (-30.0 * dt).exp();
        let (hilt, dir) = match b.weapon_smooth {
            Some((h, d)) if h.distance(want_hilt) < 0.8 => (
                h.lerp(want_hilt, k),
                d.lerp(want_dir, k).normalize_or(want_dir),
            ),
            _ => (want_hilt, want_dir),
        };
        b.weapon_smooth = Some((hilt, dir));

        let mut shield_tf = None;
        for c in children.iter() {
            if let Ok(mut s) = shields.get_mut(c) {
                *s = crate::client::shield_pose(
                    &f.fighter.action,
                    b.crouch,
                    *s,
                    1.0 - (-22.0 * dt).exp(),
                );
                shield_tf = Some(*s);
            }
        }
        let motion = BodyMotion {
            crouch: b.crouch,
            walk_phase: b.walk_phase,
            walk: b.walk,
            speed: b.speed,
            pitch: p.pitch,
            time: now,
        };
        let (pose, sword_tf) = solve_fighter_body(&f.fighter, hilt, dir, &motion, shield_tf, now);
        // Видимое тело — без наклона головы: взгляд вниз не заваливает грудь под камеру.
        let upright = BodyMotion {
            pitch: 0.0,
            ..motion
        };
        let (view_pose, _) = solve_fighter_body(&f.fighter, hilt, dir, &upright, shield_tf, now);
        let busy = crate::work_client::hands_busy(work.as_deref());
        for c in children.iter() {
            if let Ok((s, mut rp, mut t)) = rigs.get_mut(c) {
                if s.visible {
                    *rp = view_pose;
                    t.translation = view_body_offset(p.pitch);
                } else {
                    *rp = pose;
                }
            } else if let Ok(mut w) = weapons.get_mut(c) {
                *w = sword_tf;
                // За работой руками оружие убрано (как и от первого лица).
                w.scale = if busy { Vec3::ZERO } else { Vec3::ONE };
            }
        }
    }
}

/// Разложить меши по слоям: тень — только солнцу, видимое — без тени.
fn propagate_shade(
    mut commands: Commands,
    roots: Query<(Entity, &Shade)>,
    children: Query<&Children>,
    meshes: Query<(), (With<Mesh3d>, Without<Shaded>)>,
) {
    for (root, shade) in &roots {
        for e in std::iter::once(root).chain(children.iter_descendants(root)) {
            if !meshes.contains(e) {
                continue;
            }
            let mut ec = commands.entity(e);
            match shade {
                Shade::NoShadow => ec.insert((NotShadowCaster, Shaded)),
                Shade::ShadowOnly => ec.insert((RenderLayers::layer(SHADOW_LAYER), Shaded)),
            };
        }
    }
}
