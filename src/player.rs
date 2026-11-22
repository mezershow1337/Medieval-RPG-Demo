//! Управление своим персонажем: ходьба, бег, прыжок, приседание, камера от первого лица.

use crate::collision::{Body, LevelColliders};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

pub const WALK_SPEED: f32 = 4.0;
pub const SPRINT_SPEED: f32 = 6.5;
pub const CROUCH_SPEED: f32 = 2.0;
pub const JUMP_SPEED: f32 = 5.2;
pub const GRAVITY: f32 = 18.0;
const GROUND_ACCEL: f32 = 40.0;
const AIR_ACCEL: f32 = 6.0;
const MOUSE_SENSITIVITY: f32 = 0.0022;
const EYE_STANDING: f32 = 1.65;
const EYE_CROUCHING: f32 = 1.05;

/// Свой персонаж. Transform этой сущности — точка у ног, поворот только по горизонтали.
#[derive(Component)]
pub struct LocalPlayer {
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: Vec3,
    pub grounded: bool,
    pub crouching: bool,
    /// Бежит (тратит выносливость).
    pub sprinting: bool,
    /// Замедление во время ударов и оглушения (0 — не может двигаться).
    pub speed_mult: f32,
}

impl Default for LocalPlayer {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.0, velocity: Vec3::ZERO, grounded: false, crouching: false, sprinting: false, speed_mult: 1.0 }
    }
}

/// Камера игрока (дочерняя сущность персонажа).
#[derive(Component)]
pub struct PlayerCamera;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (grab_cursor, (mouse_look, movement, update_camera).chain()));
    }
}

pub fn spawn_local_player(commands: &mut Commands, pos: Vec3, assets: &crate::client::CharacterAssets) {
    commands
        .spawn((
            LocalPlayer::default(),
            crate::fight::LocalFighter::new(crate::npc::fighter_for(crate::protocol::ActorKind::Player)),
            Transform::from_translation(pos),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((
                PlayerCamera,
                Camera3d::default(),
                Projection::Perspective(PerspectiveProjection { fov: 80f32.to_radians(), ..default() }),
                Transform::from_xyz(0.0, EYE_STANDING, 0.0),
                DistanceFog {
                    color: Color::srgb(0.55, 0.5, 0.48),
                    directional_light_color: Color::srgba(1.0, 0.8, 0.55, 0.4),
                    directional_light_exponent: 20.0,
                    falloff: FogFalloff::from_visibility(160.0),
                },
            ))
            .with_children(|cam| crate::fight::spawn_view_sword(cam, assets));
        });
}

fn cursor_locked(cursor: &CursorOptions) -> bool {
    cursor.grab_mode != CursorGrabMode::None
}

pub fn grab_cursor(
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    loot: Option<Res<crate::loot_client::LootState>>,
) {
    // Пока открыт инвентарь, клики принадлежат окну, а не бою.
    if loot.is_some_and(|l| l.ui_open) {
        return;
    }
    if mouse.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn mouse_look(
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut player: Query<(&mut LocalPlayer, &mut Transform)>,
) {
    if !cursor_locked(&cursor) {
        return;
    }
    for (mut p, mut transform) in &mut player {
        p.yaw -= motion.delta.x * MOUSE_SENSITIVITY;
        p.pitch = (p.pitch - motion.delta.y * MOUSE_SENSITIVITY).clamp(-1.5, 1.5);
        transform.rotation = Quat::from_rotation_y(p.yaw);
    }
}

fn movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    level: Res<LevelColliders>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut player: Query<(&mut LocalPlayer, &mut Transform, Option<&mut crate::fight::LocalFighter>)>,
    others: Query<&Transform, (With<crate::client::RemoteActor>, Without<LocalPlayer>)>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    let controls = cursor_locked(&cursor);
    for (mut p, mut transform, mut fighter) in &mut player {
        let pressed = |k| controls && keys.pressed(k);
        let mut wish = Vec3::ZERO;
        if pressed(KeyCode::KeyW) {
            wish.z -= 1.0;
        }
        if pressed(KeyCode::KeyS) {
            wish.z += 1.0;
        }
        if pressed(KeyCode::KeyA) {
            wish.x -= 1.0;
        }
        if pressed(KeyCode::KeyD) {
            wish.x += 1.0;
        }
        let wish = Quat::from_rotation_y(p.yaw) * wish.normalize_or_zero();

        // Приседание: встать можно, только если над головой свободно.
        let want_crouch = pressed(KeyCode::ControlLeft) || pressed(KeyCode::KeyC);
        if want_crouch {
            p.crouching = true;
        } else if p.crouching && level.body_is_free(&Body::STANDING, transform.translation) {
            p.crouching = false;
        }
        let body = if p.crouching { Body::CROUCHING } else { Body::STANDING };

        // Бег тратит выносливость; без сил не побежишь.
        let has_stamina = fighter.as_ref().is_none_or(|f| f.fighter.can_sprint() || (p.sprinting && f.fighter.stamina > 0.0));
        let sprinting = pressed(KeyCode::ShiftLeft) && !p.crouching && has_stamina && wish.dot(Quat::from_rotation_y(p.yaw) * Vec3::NEG_Z) > 0.5;
        p.sprinting = sprinting;
        let gear_mult = fighter.as_ref().map_or(1.0, |f| f.move_mult);
        if let Some(f) = fighter.as_mut() {
            f.fighter.sprinting = sprinting;
        }
        let speed = if p.crouching {
            CROUCH_SPEED
        } else if sprinting {
            SPRINT_SPEED
        } else {
            WALK_SPEED
        };

        let target = wish * speed * p.speed_mult * gear_mult;
        let accel = if p.grounded { GROUND_ACCEL } else { AIR_ACCEL };
        let horizontal = Vec3::new(p.velocity.x, 0.0, p.velocity.z);
        let diff = target - horizontal;
        let change = diff.clamp_length_max(accel * dt);
        p.velocity.x += change.x;
        p.velocity.z += change.z;

        if p.grounded && pressed(KeyCode::Space) && !p.crouching && p.speed_mult > 0.5 {
            p.velocity.y = JUMP_SPEED;
            p.grounded = false;
        }
        p.velocity.y -= GRAVITY * dt;

        let mut feet = transform.translation;
        let delta = p.velocity * dt;
        let result = level.move_body(&body, &mut feet, delta, p.grounded);
        p.grounded = result.grounded;
        if result.grounded && p.velocity.y < 0.0 {
            p.velocity.y = -1.0; // лёгкий прижим к земле, чтобы спускаться по ступенькам
        }
        if result.hit_ceiling && p.velocity.y > 0.0 {
            p.velocity.y = 0.0;
        }
        if result.blocked_x {
            p.velocity.x = 0.0;
        }
        if result.blocked_z {
            p.velocity.z = 0.0;
        }

        // Не проходим сквозь других бойцов.
        for other in &others {
            let d = feet - other.translation;
            let flat = Vec2::new(d.x, d.z);
            let dist = flat.length();
            const MIN_DIST: f32 = 0.7;
            if dist < MIN_DIST && d.y.abs() < 1.5 {
                let push = if dist > 1e-3 { flat / dist } else { Vec2::X } * (MIN_DIST - dist);
                let candidate = feet + Vec3::new(push.x, 0.0, push.y);
                if level.body_is_free(&body, candidate) {
                    feet = candidate;
                }
            }
        }

        // Упали за пределы мира — возвращаемся во двор.
        if feet.y < -30.0 {
            feet = crate::level::spawn_point(0);
            p.velocity = Vec3::ZERO;
        }
        transform.translation = feet;
    }
}

fn update_camera(time: Res<Time>, player: Query<&LocalPlayer>, mut camera: Query<&mut Transform, With<PlayerCamera>>) {
    let Ok(p) = player.single() else { return };
    let Ok(mut cam) = camera.single_mut() else { return };
    let eye = if p.crouching { EYE_CROUCHING } else { EYE_STANDING };
    // Плавно меняем высоту глаз (приседание, ступеньки).
    let t = 1.0 - (-14.0 * time.delta_secs()).exp();
    cam.translation.y += (eye - cam.translation.y) * t;
    // Поворот камеры по вертикали (с тряской) задаёт fight.rs.
}
