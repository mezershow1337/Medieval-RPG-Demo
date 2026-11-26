//! Плавание: на поверхности и под водой, запас дыхания, вид из-под воды.
//!
//! Клиент сам решает, плывёт ли его персонаж (ноги глубже ~1,2 м там, где можно утонуть),
//! и двигает его по-плавательному; сервер получает обычные положения (новых сообщений нет)
//! и по тем же правилам считает дыхание: кто слишком долго под водой, тот захлёбывается.
//! Другие игроки видят пловца по его положению: тело ложится в воду и гребёт (`pose_swimmers`).

use crate::collision::{Body, LevelColliders};
use crate::level::WATER_Y;
use crate::player::{LocalPlayer, PlayerCamera};
use bevy::prelude::*;

/// Свой персонаж плывёт (компонент на сущности [`LocalPlayer`]).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Swimming {
    /// Голова под водой (ныряет).
    pub underwater: bool,
}

/// Запас дыхания, с.
pub const BREATH_MAX: f32 = 40.0;
/// Сколько здоровья в секунду отнимает удушье, когда воздух кончился.
pub const DROWN_DPS: f32 = 9.0;
/// Глубже этого (от уровня моря до дна) уже не стоят, а плывут.
const SWIM_DEPTH: f32 = 1.25;
/// Ноги ниже поверхности на столько — начинаем плыть, выше — встаём на дно.
const ENTER: f32 = 1.2;
const LEAVE: f32 = 1.0;
/// На плаву ноги ниже поверхности на столько (глаза чуть над водой).
const FLOAT: f32 = 1.47;
const SWIM_SPEED: f32 = 2.3;
const SWIM_SPRINT: f32 = 3.6;
const DIVE_SPEED: f32 = 2.1;

/// Голова под водой: одно правило для клиента (полоска дыхания) и сервера (удушье).
/// По среднему уровню моря, без волн: сервер волн не считает.
pub fn head_underwater(feet: Vec3, eye_height: f32) -> bool {
    crate::water::is_water(feet.x, feet.z) && feet.y + eye_height < WATER_Y - 0.08
}

/// Запас дыхания за кадр: под водой тратится, на воздухе восстанавливается за ~6 с.
pub fn breathe(left: f32, under: bool, dt: f32) -> f32 {
    if under { (left - dt).max(0.0) } else { (left + dt * BREATH_MAX / 6.0).min(BREATH_MAX) }
}

/// Можно ли здесь плыть: море и под ногами глубже `SWIM_DEPTH`.
pub fn deep_water(x: f32, z: f32) -> bool {
    crate::water::is_water(x, z) && WATER_Y - crate::level::ground_y(x, z) > SWIM_DEPTH
}

pub struct SwimPlugin;

impl Plugin for SwimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Breath>()
            .add_systems(Startup, spawn_breath_bar.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (swim_state.before(crate::player::PlayerMoveSet), (track_breath, show_breath_bar).chain().after(crate::player::PlayerMoveSet)))
            .add_systems(Update, pose_swimmers.after(crate::rig::PoseSet).before(crate::rig::ApplyPoseSet))
            .add_systems(PostUpdate, underwater_view)
            .add_systems(Update, dev_swim_shot.run_if(crate::settings::env_flag("MRPG_SWIM_SHOT")))
            .add_systems(Update, dev_swim_body.in_set(crate::rig::PoseSet).run_if(crate::settings::env_flag("MRPG_SWIM_BODY")));
    }
}

/// Сервер: удушье под водой.
pub struct SwimServerPlugin;

impl Plugin for SwimServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServerBreath>().add_systems(Update, drown);
    }
}

// ---------------------------------------------------------------------------------------------
// Свой персонаж

/// Вход в воду и выход на берег.
fn swim_state(mut commands: Commands, time: Res<Time>, dev: Option<Res<crate::console::DevMove>>, player: Query<(Entity, &Transform, &LocalPlayer, Option<&Swimming>)>, cams: Query<&GlobalTransform, With<PlayerCamera>>) {
    let flying = dev.is_some_and(|d| d.cheats.fly);
    let t = time.elapsed_secs_wrapped();
    for (e, tf, _p, swim) in &player {
        let feet = tf.translation;
        let surface = crate::water::surface_y(feet.x, feet.z, t);
        let depth = surface - feet.y;
        let want = !flying && deep_water(feet.x, feet.z) && if swim.is_some() { depth > LEAVE } else { depth > ENTER };
        let eye = cams.iter().next().map_or(feet.y + 1.65, |c| c.translation().y);
        let underwater = eye < surface - 0.05;
        match (want, swim) {
            (true, Some(s)) if s.underwater != underwater => {
                commands.entity(e).insert(Swimming { underwater });
            }
            (true, None) => {
                commands.entity(e).insert(Swimming { underwater });
            }
            (false, Some(_)) => {
                commands.entity(e).remove::<Swimming>();
            }
            _ => {}
        }
    }
}

/// Движение вплавь (зовёт `player::movement` вместо ходьбы). Вперёд — куда смотришь: взгляд
/// вниз уводит под воду, вверх — к поверхности; Пробел — всплыть, Ctrl/C — нырнуть.
/// На поверхности держимся на плаву и качаемся на волнах.
#[allow(clippy::too_many_arguments)]
pub fn swim_move(p: &mut LocalPlayer, tf: &mut Transform, level: &LevelColliders, fighter: Option<&mut crate::fight::LocalFighter>, dt: f32, t: f32, pressed: impl Fn(KeyCode) -> bool) {
    let feet = tf.translation;
    let surface = crate::water::surface_y(feet.x, feet.z, t);
    let float_y = surface - FLOAT;
    let at_surface = feet.y > float_y - 0.25;
    let look = Quat::from_rotation_y(p.yaw) * Quat::from_rotation_x(p.pitch);
    let flat = Quat::from_rotation_y(p.yaw);
    let fwd_in = (pressed(KeyCode::KeyW) as i32 - pressed(KeyCode::KeyS) as i32) as f32;
    let side_in = (pressed(KeyCode::KeyD) as i32 - pressed(KeyCode::KeyA) as i32) as f32;
    // На поверхности смотрим чуть вниз — плывём ровно; ныряем, только глядя заметно вниз.
    let dive_look = p.pitch < -0.45 || !at_surface;
    let fwd = if dive_look { look * Vec3::NEG_Z } else { flat * Vec3::NEG_Z };
    let mut wish = fwd * fwd_in + flat * Vec3::X * side_in;
    let up = pressed(KeyCode::Space);
    let down = pressed(KeyCode::ControlLeft) || pressed(KeyCode::KeyC);
    if up {
        wish.y += 1.0;
    }
    if down {
        wish.y -= 1.0;
    }
    let wish = wish.normalize_or_zero();
    let has_stamina = fighter.as_ref().is_none_or(|f| f.fighter.can_sprint() || (p.sprinting && f.fighter.stamina > 0.0));
    let sprint = pressed(KeyCode::ShiftLeft) && fwd_in > 0.0 && has_stamina;
    p.sprinting = sprint;
    let gear = fighter.as_ref().map_or(1.0, |f| f.move_mult);
    if let Some(f) = fighter {
        f.fighter.sprinting = sprint;
    }
    let speed = if sprint { SWIM_SPRINT } else if at_surface { SWIM_SPEED } else { DIVE_SPEED };
    let mut target = wish * speed * p.speed_mult.max(0.3) * gear.max(0.6);
    // Плавучесть: без гребков вверх-вниз тело само всплывает к поверхности (медленно с глубины).
    if !down && wish.y.abs() < 0.3 && !(dive_look && fwd_in > 0.0) {
        let rise = ((float_y - feet.y) * 1.8).clamp(-1.2, 0.9);
        target.y = if at_surface { rise } else { target.y + 0.35 };
    }
    // Над поверхностью не выпрыгиваем: выше плавучей высоты — только вниз.
    if feet.y > float_y + 0.05 {
        target.y = target.y.min((float_y - feet.y) * 3.0);
    }
    // Вода вязкая: скорость догоняет желаемую плавно.
    let k = 1.0 - (-2.8 * dt).exp();
    p.velocity += (target - p.velocity) * k;
    p.grounded = false;
    p.crouching = false;
    let mut pos = feet;
    let r = level.move_body(&Body::CROUCHING, &mut pos, p.velocity * dt, false);
    if r.blocked_x {
        p.velocity.x = 0.0;
    }
    if r.blocked_z {
        p.velocity.z = 0.0;
    }
    if r.grounded && p.velocity.y < 0.0 {
        p.velocity.y = 0.0;
    }
    // У отвесной набережной: Пробел у стенки — выбраться наверх, если там есть где встать.
    if up && (r.blocked_x || r.blocked_z) && at_surface {
        let ahead = pos + flat * Vec3::NEG_Z * 0.6;
        for step in [0.6f32, 1.0, 1.4, 1.8] {
            let top = Vec3::new(ahead.x, surface + step, ahead.z);
            if level.body_is_free(&Body::STANDING, top) && !level.body_is_free(&Body::STANDING, top - Vec3::Y * 0.3) {
                pos = top;
                p.velocity = Vec3::ZERO;
                break;
            }
        }
    }
    tf.translation = pos;
}

// ---------------------------------------------------------------------------------------------
// Дыхание

#[derive(Resource)]
pub struct Breath {
    /// Сколько секунд ещё можно не дышать.
    pub left: f32,
}

impl Default for Breath {
    fn default() -> Self {
        Self { left: BREATH_MAX }
    }
}

fn track_breath(time: Res<Time>, mut breath: ResMut<Breath>, player: Query<(&Transform, &LocalPlayer)>) {
    let Ok((tf, p)) = player.single() else { return };
    let eye = if p.crouching { 1.05 } else { 1.65 };
    let under = head_underwater(tf.translation, eye);
    let next = breathe(breath.left, under, time.delta_secs().min(0.1));
    if next != breath.left {
        breath.left = next;
    }
}

#[derive(Component)]
struct BreathBar;

#[derive(Component)]
struct BreathFill;

/// Полоска дыхания над здоровьем (только когда воздух тратится).
fn spawn_breath_bar(mut commands: Commands) {
    let theme = crate::ui_theme::theme();
    commands
        .spawn((
            BreathBar,
            Node { width: percent(100), position_type: PositionType::Absolute, bottom: px(132), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_direction: FlexDirection::Column, row_gap: px(2), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn((theme.text("Дыхание", 13.0, crate::ui_theme::ink::TEXT), TextShadow { offset: Vec2::splat(1.0), color: Color::BLACK }));
            theme.bar(p, Node { width: px(220), height: px(14), ..default() }, 1.0, Color::srgb(0.35, 0.65, 0.85), BreathFill);
        });
}

fn show_breath_bar(breath: Res<Breath>, mut bar: Query<&mut Visibility, With<BreathBar>>, mut fill: Query<(&mut Node, &mut BackgroundColor), With<BreathFill>>) {
    if !breath.is_changed() {
        return;
    }
    let k = breath.left / BREATH_MAX;
    for mut v in &mut bar {
        v.set_if_neq(if k < 0.999 { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (mut node, mut color) in &mut fill {
        node.width = percent((k * 100.0).clamp(0.0, 100.0));
        // Последние секунды — тревожно-красная.
        color.set_if_neq(BackgroundColor(if k < 0.25 { Color::srgb(0.85, 0.3, 0.2) } else { Color::srgb(0.35, 0.65, 0.85) }));
    }
}

/// Сервер: дыхание каждого игрока по его положению; без воздуха теряет здоровье.
#[derive(Resource, Default)]
struct ServerBreath(std::collections::HashMap<u64, f32>);

fn drown(time: Res<Time>, mut world: ResMut<crate::server::GameWorld>, mut breath: ResMut<ServerBreath>) {
    let dt = time.delta_secs().min(0.1);
    let world = &mut *world;
    breath.0.retain(|id, _| world.actors.contains_key(id));
    for (id, a) in world.actors.iter_mut() {
        if a.kind != crate::protocol::ActorKind::Player {
            continue;
        }
        let left = breath.0.entry(*id).or_insert(BREATH_MAX);
        if !a.alive || a.cheats.free_move() {
            *left = BREATH_MAX;
            continue;
        }
        // Сервер чуть добрее клиента (на 2 с): из-за задержки сети полоска уже пуста, когда
        // сервер начинает отнимать здоровье, а не наоборот.
        *left = breathe(*left, head_underwater(a.pose.feet, a.pose.eye_height()), dt);
        if *left <= 0.0 {
            a.fighter.hp = (a.fighter.hp - DROWN_DPS * dt).max(0.0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Вид из-под воды

/// Камера под водой: густая зелёная дымка вместо воздушной (дальше 20–30 м не видно), а над
/// головой — изнанка волн. Воздушную дымку возвращаем, как только голова над водой.
fn underwater_view(time: Res<Time>, mood: Option<Res<crate::daylight::CurrentMood>>, mut cams: Query<(Entity, &GlobalTransform, &mut DistanceFog), With<Camera3d>>, mut saved: Local<std::collections::HashMap<Entity, DistanceFog>>) {
    let t = time.elapsed_secs_wrapped();
    let (light, night) = mood.as_ref().map_or((1.0, 0.0), |m| (m.0.lux / 100_000.0, m.0.night));
    let light = light.clamp(0.02, 1.0).sqrt() * (1.0 - 0.6 * night);
    for (e, gt, mut fog) in &mut cams {
        let p = gt.translation();
        let under = crate::water::is_water(p.x, p.z) && p.y < crate::water::surface_y(p.x, p.z, t) - 0.03;
        if !under {
            if let Some(f) = saved.remove(&e) {
                *fog = f;
            }
            continue;
        }
        saved.entry(e).or_insert_with(|| fog.clone());
        // Чем глубже, тем темнее и синее.
        let depth = (WATER_Y - p.y).max(0.0);
        let dim = (-depth * 0.09).exp();
        let c = Vec3::new(0.05, 0.22, 0.24) * light * dim;
        *fog = DistanceFog {
            color: Color::linear_rgb(c.x, c.y, c.z),
            directional_light_color: Color::linear_rgba(0.2 * light, 0.35 * light, 0.3 * light, 0.4),
            directional_light_exponent: 8.0,
            falloff: FogFalloff::from_visibility(26.0),
        };
    }
}

// ---------------------------------------------------------------------------------------------
// Другие игроки вплавь

/// Тело игрока, который плывёт: лежит в воде и гребёт брассом. Узнаём по положению: ноги
/// глубоко там, где плавают.
fn pose_swimmers(time: Res<Time>, actors: Query<(&Transform, &Children), (With<crate::client::RemoteActor>, Without<crate::client::BodyRig>)>, mut rigs: Query<(&mut crate::rig::RigPose, &mut Transform), With<crate::client::BodyRig>>) {
    let t = time.elapsed_secs();
    let tw = time.elapsed_secs_wrapped();
    for (tf, children) in &actors {
        let feet = tf.translation;
        let swimming = deep_water(feet.x, feet.z) && crate::water::surface_y(feet.x, feet.z, tw) - feet.y > LEAVE;
        if !swimming {
            // Вышел на берег — тело снова стоит (поворот корня у живых бойцов есть только вплавь).
            for child in children.iter() {
                if let Ok((_, mut body)) = rigs.get_mut(child)
                    && body.rotation != Quat::IDENTITY
                {
                    *body = Transform::default();
                }
            }
            continue;
        }
        for child in children.iter() {
            let Ok((mut pose, mut body)) = rigs.get_mut(child) else { continue };
            *pose = crate::rig::swim_pose(t);
            // Тело ложится вперёд, голова у поверхности.
            *body = swim_tilt();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Снимки для проверки

/// `MRPG_SWIM_SHOT="x y z yaw pitch"` — хозяин игры сам переносится в эту точку (команда `tp`
/// консоли), чтобы снять вид пловца.
fn dev_swim_shot(mut client: Option<ResMut<bevy_renet::RenetClient>>, player: Query<(), With<LocalPlayer>>, mut frames: Local<u32>) {
    let Some(client) = client.as_mut() else { return };
    if player.is_empty() || !client.is_connected() {
        return;
    }
    *frames += 1;
    if *frames == 5 {
        let line = format!("tp {}", std::env::var("MRPG_SWIM_SHOT").unwrap_or_default());
        let msg = crate::console_net::ConsoleClientMessage::Command { line, token: crate::console_net::host_token() };
        client.send_message(crate::console_net::CH_CONSOLE, crate::protocol::encode(&msg));
    }
}

#[derive(Component)]
struct SwimShowcase;

/// `MRPG_SWIM_BODY="x,z,yaw"` — человек плывёт брассом в этой точке (как другой игрок вплавь).
fn dev_swim_body(mut commands: Commands, time: Res<Time>, mut done: Local<bool>, mut rigs: Query<(&mut crate::rig::RigPose, &mut Transform), With<SwimShowcase>>) {
    if !*done {
        *done = true;
        let v: Vec<f32> = std::env::var("MRPG_SWIM_BODY").unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if v.len() == 3 {
            let feet = Vec3::new(v[0], WATER_Y - FLOAT, v[1]);
            commands
                .spawn((Transform::from_translation(feet).with_rotation(Quat::from_rotation_y(v[2])), Visibility::default()))
                .with_children(|c| {
                    c.spawn((SwimShowcase, crate::rig::NeedsRig(crate::rig::Outfit::for_actor(crate::protocol::ActorKind::Player, 3)), Transform::default(), Visibility::default()));
                });
        }
    }
    for (mut pose, mut body) in &mut rigs {
        *pose = crate::rig::swim_pose(time.elapsed_secs());
        *body = swim_tilt();
    }
}

/// Тело вплавь: лежит вперёд, голова у поверхности.
fn swim_tilt() -> Transform {
    Transform::from_xyz(0.0, 1.2, 0.0).with_rotation(Quat::from_rotation_x(-1.25)) * Transform::from_xyz(0.0, -1.2, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breath_runs_out_and_returns() {
        let mut b = BREATH_MAX;
        for _ in 0..(BREATH_MAX as usize * 10 + 5) {
            b = breathe(b, true, 0.1);
        }
        assert_eq!(b, 0.0);
        for _ in 0..61 {
            b = breathe(b, false, 0.1);
        }
        assert_eq!(b, BREATH_MAX);
    }

    #[test]
    fn swim_only_in_deep_sea() {
        // Открытое море к северу от города — плывём; на агоре и на мелководье у пляжа — нет.
        assert!(deep_water(0.0, -260.0));
        assert!(!deep_water(0.0, 0.0));
        let feet = Vec3::new(0.0, WATER_Y - 3.0, -260.0);
        assert!(head_underwater(feet, 1.65));
        assert!(!head_underwater(Vec3::new(0.0, WATER_Y - FLOAT, -260.0), 1.65));
    }
}
