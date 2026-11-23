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
    trade: Option<Res<crate::trade_client::TradeState>>,
    law: Option<Res<crate::civil_client::LawUi>>,
    book: Option<Res<crate::skill_book::BookUi>>,
    map: Option<Res<crate::map::MapView>>,
    towns: Option<Res<crate::towns_client::TownUi>>,
) {
    // Пока открыт инвентарь, окно торговли, книга навыков, разговор со стражей, карта или окно дома, клики принадлежат окну, а не бою.
    if loot.is_some_and(|l| l.ui_open) || trade.is_some_and(|t| t.window_open()) || law.is_some_and(|l| l.open()) || book.is_some_and(|b| b.open) || map.is_some_and(|m| m.open) || towns.is_some_and(|t| t.open()) {
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
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut player: Query<(&mut LocalPlayer, &mut Transform, Option<&crate::fight::LocalFighter>)>,
    work: Option<Res<crate::work_client::MyWork>>,
) {
    // Руки заняты работой: мышь водит пестик, а не голову.
    if !cursor_locked(&cursor) || crate::work_client::hands_busy(work.as_deref()) {
        return;
    }
    let dt = time.delta_secs().max(1.0 / 240.0);
    for (mut p, mut transform, fighter) in &mut player {
        let mut dyaw = -motion.delta.x * MOUSE_SENSITIVITY;
        let mut dpitch = -motion.delta.y * MOUSE_SENSITIVITY;
        // Ограничение поворота во время удара (как в Chivalry/Mordhau): разворотом можно
        // ускорить или придержать удар, но не крутануть клинок на полкруга мгновенно.
        if let Some(cap) = fighter.and_then(|f| crate::combat::turn_cap(&f.fighter.action)) {
            dyaw = dyaw.clamp(-cap * dt, cap * dt);
            dpitch = dpitch.clamp(-cap * dt, cap * dt);
        }
        p.yaw += dyaw;
        p.pitch = (p.pitch + dpitch).clamp(-1.5, 1.5);
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
    dev: Option<Res<crate::console::DevMove>>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    let controls = cursor_locked(&cursor);
    let cheats = dev.map(|d| d.cheats).unwrap_or_default();
    for (mut p, mut transform, mut fighter) in &mut player {
        let pressed = |k| controls && keys.pressed(k);
        if cheats.fly {
            fly(&mut p, &mut transform, &level, cheats, dt, pressed);
            continue;
        }
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

        let target = wish * speed * p.speed_mult * gear_mult * cheats.speed;
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

/// Полёт из консоли разработчика: WASD по направлению взгляда, Пробел вверх, Ctrl/C вниз,
/// Shift быстрее. С `noclip` — сквозь стены.
fn fly(p: &mut LocalPlayer, transform: &mut Transform, level: &LevelColliders, cheats: crate::console_net::Cheats, dt: f32, pressed: impl Fn(KeyCode) -> bool) {
    const FLY_SPEED: f32 = 9.0;
    let look = Quat::from_rotation_y(p.yaw) * Quat::from_rotation_x(p.pitch);
    let mut wish = Vec3::ZERO;
    if pressed(KeyCode::KeyW) {
        wish += look * Vec3::NEG_Z;
    }
    if pressed(KeyCode::KeyS) {
        wish += look * Vec3::Z;
    }
    if pressed(KeyCode::KeyA) {
        wish += look * Vec3::NEG_X;
    }
    if pressed(KeyCode::KeyD) {
        wish += look * Vec3::X;
    }
    if pressed(KeyCode::Space) {
        wish += Vec3::Y;
    }
    if pressed(KeyCode::ControlLeft) || pressed(KeyCode::KeyC) {
        wish -= Vec3::Y;
    }
    let boost = if pressed(KeyCode::ShiftLeft) { 4.0 } else { 1.0 };
    let target = wish.normalize_or_zero() * FLY_SPEED * boost * cheats.speed;
    // Плавный разгон и торможение.
    let t = 1.0 - (-10.0 * dt).exp();
    p.velocity += (target - p.velocity) * t;
    p.grounded = false;
    p.crouching = false;
    p.sprinting = false;
    let delta = p.velocity * dt;
    if cheats.noclip {
        transform.translation += delta;
    } else {
        let mut feet = transform.translation;
        level.move_body(&Body::STANDING, &mut feet, delta, false);
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
