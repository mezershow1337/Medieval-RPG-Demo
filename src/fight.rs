//! Бой на стороне клиента: ввод (ЛКМ — удар в направлении движения мыши, СКМ — укол,
//! Q — финт, ПКМ — парирование), предсказание своего состояния, меч в руках от первого лица,
//! движение камеры вслед за ударом и эффекты попаданий.

use crate::client::{CharacterAssets, NetState, RemoteActor, RemoteIndex};
use crate::rig::Blade;
use crate::combat::{self, Action, Fighter, HitOutcome, Swing};
use bevy::input::mouse::AccumulatedMouseMotion;
use crate::player::{LocalPlayer, PlayerCamera};
use crate::protocol::*;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;

/// Набор систем боя; сеть обрабатывается до него.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FightSet;

/// Боевое состояние своего персонажа (предсказываем локально, сервер поправляет).
#[derive(Component)]
pub struct LocalFighter {
    pub fighter: Fighter,
    /// Множитель скорости ходьбы от экипировки (присылает сервер).
    pub move_mult: f32,
    /// Вид своего клинка (присылает сервер).
    pub finish: u8,
    /// Что надето (присылает сервер): рука от первого лица в перчатке, наруче, рукаве доспеха.
    pub gear: crate::gear_look::Gear,
}

impl LocalFighter {
    pub fn new(fighter: Fighter) -> Self {
        Self { fighter, move_mult: 1.0, finish: 200, gear: Default::default() }
    }
}

/// Меч в руках (вид от первого лица).
#[derive(Component)]
struct ViewSword;

/// События боя для эффектов и интерфейса.
#[derive(Message, Clone, Copy, Debug)]
pub enum CombatFeedback {
    Hit { attacker: u64, target: u64, outcome: HitOutcome, at: Vec3 },
    Died { id: u64, killer: Option<u64> },
}

/// Последнее заметное событие для надписи в центре экрана.
#[derive(Resource, Default)]
pub struct Banner {
    pub text: String,
    pub color: Color,
    pub time_left: f32,
}

/// Красная вспышка при получении урона (0..1).
#[derive(Resource, Default)]
pub struct DamageFlash(pub f32);

/// Тряска камеры (0..1).
#[derive(Resource, Default)]
struct Shake(f32);

/// Куда пойдёт следующий удар: движение мыши за последние доли секунды (x вправо, y вверх).
/// По нему же рисуется метка направления у прицела.
#[derive(Resource, Default)]
pub struct AttackAim {
    pub motion: Vec2,
    pub pending: Option<Swing>,
}

/// Сколько помнится движение мыши для выбора направления, с.
const AIM_MEMORY: f32 = 0.12;
/// Сколько надо сдвинуть мышь, чтобы выбрать направление (иначе удар по серии).
const AIM_THRESHOLD: f32 = 5.0;

#[derive(Component)]
struct Spark {
    velocity: Vec3,
    life: f32,
}

#[derive(Resource)]
struct SparkAssets {
    mesh: Handle<Mesh>,
    gold: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    blood: Handle<StandardMaterial>,
}

pub struct FightPlugin;

impl Plugin for FightPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CombatFeedback>()
            .init_resource::<Banner>()
            .init_resource::<DamageFlash>()
            .init_resource::<Shake>()
            .init_resource::<AttackAim>()
            .insert_resource(CombatHints::from_env())
            .add_systems(Update, toggle_combat_hints)
            .add_systems(Startup, setup_sparks)
            .add_systems(
                Update,
                (
                    combat_input.before(crate::player::grab_cursor),
                    tick_local,
                    fp_pose_for_shots.run_if(|| std::env::var("MRPG_FP_POSE").is_ok()),
                    animate_view_sword,
                    feedback_effects,
                    update_sparks,
                    decay_effects,
                )
                    .chain()
                    .in_set(FightSet),
            )
            .add_systems(Update, (refresh_view_weapon, refresh_view_arm));
    }
}

/// Плечо руки от первого лица в пространстве камеры (за краем экрана справа внизу).
const VIEW_SHOULDER: Vec3 = Vec3::new(0.24, -0.3, 0.12);
/// Плечо левой руки: ниже и дальше в сторону, чтобы рука, тянущаяся к древку через грудь,
/// не закрывала полэкрана.
const VIEW_SHOULDER_L: Vec3 = Vec3::new(-0.3, -0.46, 0.06);
/// Вокруг чего опускаем оружие с руками, когда смотрят под ноги (у груди).
const VIEW_LIFT_PIVOT: Vec3 = Vec3::new(0.0, -0.3, 0.1);
/// Насколько меч от первого лица выше, чем в мире (только внешний вид).
const VIEW_LIFT: f32 = 0.36;

/// Подсказки боя: метки направления и кольцо парирования у прицела, свечение рипоста.
/// Игра реалистичная, поэтому подсказки сдержанные и выключаются F7
/// (`MRPG_COMBAT_HINTS=0` — выключены с самого начала).
#[derive(Resource, Clone, Copy)]
pub struct CombatHints(pub bool);

impl CombatHints {
    fn from_env() -> Self {
        CombatHints(std::env::var("MRPG_COMBAT_HINTS").map(|v| v != "0").unwrap_or(true))
    }
}

fn toggle_combat_hints(keys: Res<ButtonInput<KeyCode>>, mut hints: ResMut<CombatHints>, mut banner: ResMut<Banner>) {
    if keys.just_pressed(KeyCode::F7) {
        hints.0 = !hints.0;
        banner.text = if hints.0 { "Подсказки боя включены (F7)" } else { "Подсказки боя выключены (F7)" }.into();
        banner.color = Color::srgb(0.85, 0.85, 0.85);
        banner.time_left = 1.5;
    }
}

/// Свечение рипоста у меча от первого лица.
#[derive(Component)]
struct ViewGlow;

pub fn spawn_view_sword(parent: &mut ChildSpawnerCommands, assets: &CharacterAssets) {
    parent.spawn((ViewGlow, crate::client::riposte_glow_bundle(assets)));
    parent.spawn((
        ViewSword,
        ShownViewWeapon(combat::WeaponKind::Spatha, None),
        crate::rig::NeedsWeapon(combat::WeaponKind::Spatha),
        Transform::default(),
        Visibility::default(),
        crate::self_body::Shade::NoShadow,
    ));
    // Правая рука держит оружие всегда, левая — второй хват (копьё) или кулак в стойке.
    for left in [false, true] {
        parent.spawn((ViewArmRoot, crate::rig::NeedsViewArm(None, left), Transform::default(), Visibility::default(), crate::self_body::Shade::NoShadow));
    }
}

/// Какое оружие показано от первого лица.
#[derive(Component)]
struct ShownViewWeapon(combat::WeaponKind, Option<u16>);

/// Сменили оружие в инвентаре — меняем модель в руке (модель именно этого предмета).
fn refresh_view_weapon(
    mut commands: Commands,
    local: Query<&LocalFighter>,
    mut shown: Query<(Entity, &mut ShownViewWeapon)>,
    models: Option<Res<crate::icons::ItemModels>>,
) {
    let Ok(f) = local.single() else { return };
    let kind = f.fighter.weapon;
    let held = f.gear.main_hand.filter(|_| !f.gear.holstered && kind != combat::WeaponKind::Fists);
    for (e, mut w) in &mut shown {
        if (w.0, w.1) != (kind, held) {
            (w.0, w.1) = (kind, held);
            crate::worn::set_weapon_model(&mut commands, e, kind, held.map(crate::items::ItemBaseId), f.finish, models.as_deref());
        }
    }
}

/// Рука, держащая меч (вид от первого лица).
#[derive(Component)]
struct ViewArmRoot;

/// Надел перчатки, наручи или доспех — переодеваем руку от первого лица.
fn refresh_view_arm(
    mut commands: Commands,
    local: Query<&LocalFighter>,
    arms: Query<(Entity, &crate::rig::ViewArm), With<ViewArmRoot>>,
    net: Option<Res<NetState>>,
    mut shown: Local<crate::gear_look::Gear>,
) {
    let Ok(f) = local.single() else { return };
    if f.gear == *shown {
        return;
    }
    // Ждём, пока обе руки соберутся (у них уже есть суставы), иначе переодевать нечего.
    if arms.iter().len() < 2 {
        return;
    }
    *shown = f.gear;
    let id = net.as_ref().and_then(|n| n.my_id).unwrap_or(0);
    let outfit = crate::rig::Outfit::for_actor(ActorKind::Player, id).with_gear(&f.gear);
    for (e, arm) in &arms {
        commands.entity(e).despawn_related::<Children>().remove::<crate::rig::ViewArm>().insert(crate::rig::NeedsViewArm(Some(outfit), arm.left));
    }
}

fn setup_sparks(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let glow = |materials: &mut Assets<StandardMaterial>, c: Color, e: LinearRgba| materials.add(StandardMaterial { base_color: c, emissive: e, unlit: true, ..default() });
    commands.insert_resource(SparkAssets {
        mesh: meshes.add(Cuboid::new(0.03, 0.03, 0.03)),
        gold: glow(&mut materials, Color::srgb(1.0, 0.8, 0.3), LinearRgba::rgb(20.0, 12.0, 3.0)),
        white: glow(&mut materials, Color::WHITE, LinearRgba::rgb(8.0, 8.0, 8.0)),
        blood: glow(&mut materials, Color::srgb(0.6, 0.02, 0.02), LinearRgba::rgb(0.6, 0.0, 0.0)),
    });
}

fn cursor_locked(cursor: &CursorOptions) -> bool {
    cursor.grab_mode != CursorGrabMode::None
}

#[allow(clippy::too_many_arguments)]
fn combat_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    client: Option<ResMut<RenetClient>>,
    mut local: Query<(&LocalPlayer, &mut LocalFighter)>,
    grab: Option<Res<crate::props_client::PropGrab>>,
    mut aim: ResMut<AttackAim>,
    work: Option<Res<crate::work_client::MyWork>>,
) {
    // Во время работы руками ЛКМ — удар молотом по заготовке, а не мечом.
    if !cursor_locked(&cursor) || crate::work_client::hands_busy(work.as_deref()) {
        aim.motion = Vec2::ZERO;
        aim.pending = None;
        return;
    }
    // Направление удара: куда вели мышь последние доли секунды.
    let decay = (-time.delta_secs() / AIM_MEMORY).exp();
    aim.motion = aim.motion * decay + Vec2::new(motion.delta.x, -motion.delta.y);
    // ЛКМ по предмету — захват (props_client), ПКМ с предметом в руках — бросок.
    let (no_attack, no_guard) = grab.map(|g| (g.blocks_attack(), g.blocks_guard())).unwrap_or((false, false));
    let Some(mut client) = client else { return };
    let Ok((p, mut f)) = local.single_mut() else { return };
    aim.pending = Swing::from_mouse(aim.motion, AIM_THRESHOLD).map(|s| f.fighter.weapon.normalize(s));
    if f.fighter.is_dead() {
        return;
    }
    let mut attack = |f: &mut LocalFighter, swing: Option<Swing>| {
        f.fighter.try_attack(swing);
        client.send_message(CH_RELIABLE, encode(&ClientMessage::Attack { yaw: p.yaw, pitch: p.pitch, swing }));
    };
    if !no_attack {
        if mouse.just_pressed(MouseButton::Left) {
            let swing = aim.pending;
            attack(&mut f, swing);
            aim.motion = Vec2::ZERO;
        }
        // Средняя кнопка — укол (кулаком — прямой).
        if mouse.just_pressed(MouseButton::Middle) {
            attack(&mut f, Some(Swing::Thrust));
        }
    }
    // Финт: бросить замах.
    if keys.just_pressed(KeyCode::KeyQ) && f.fighter.feint() {
        client.send_message(CH_RELIABLE, encode(&ClientMessage::Feint));
    }
    // Защита: ПКМ. Вовремя — парирование с рипостом, дольше — глухой блок.
    if mouse.just_pressed(MouseButton::Right) && !no_guard {
        f.fighter.set_guard(true);
        client.send_message(CH_RELIABLE, encode(&ClientMessage::Guard { on: true, yaw: p.yaw, pitch: p.pitch }));
    }
    if mouse.just_released(MouseButton::Right) {
        f.fighter.set_guard(false);
        client.send_message(CH_RELIABLE, encode(&ClientMessage::Guard { on: false, yaw: p.yaw, pitch: p.pitch }));
    }
}

fn tick_local(time: Res<Time>, mut local: Query<(&mut LocalPlayer, &mut LocalFighter)>) {
    for (mut p, mut f) in &mut local {
        f.fighter.tick(time.delta_secs());
        p.speed_mult = f.fighter.move_speed_mult();
    }
}

/// Для снимков: `MRPG_FP_POSE=фаза:удар:доля` (см. `combat::showcase_action`) застывает своё
/// действие от первого лица; `MRPG_FP_AIM=ur` — метка направления у прицела.
fn fp_pose_for_shots(mut local: Query<&mut LocalFighter>, mut aim: ResMut<AttackAim>) {
    let Ok(mut f) = local.single_mut() else { return };
    let spec = std::env::var("MRPG_FP_POSE").unwrap_or_default();
    if let Some(a) = combat::showcase_action(&spec, f.fighter.weapon) {
        f.fighter.action = a;
        if spec.starts_with("guard") {
            f.fighter.guard_held = true;
        }
        if spec.starts_with("riposte") {
            f.fighter.riposte_timer = 0.0;
        }
    }
    if let Some(s) = std::env::var("MRPG_FP_AIM").ok().and_then(|s| Swing::from_code(&s)) {
        aim.pending = Some(s);
    }
}

/// Сверяем своё предсказанное состояние с сервером.
pub fn reconcile(local: &mut LocalFighter, snap: &ActorSnapshot) {
    local.fighter.hp = snap.hp;
    local.fighter.max_hp = snap.max_hp;
    local.fighter.has_shield = snap.shield;
    local.fighter.stamina = snap.stamina;
    local.fighter.max_stamina = snap.max_stamina;
    local.fighter.action_speed = snap.action_speed;
    local.move_mult = snap.move_mult;
    local.finish = snap.finish;
    local.gear = snap.gear;
    // Оружие меняем только вне удара, чтобы не сбить анимацию.
    if snap.weapon != local.fighter.weapon && matches!(local.fighter.action, Action::Idle | Action::Guard { .. }) {
        local.fighter.weapon = snap.weapon;
    }
    let forced = matches!(snap.action, Action::Stagger { .. } | Action::Recoil { .. } | Action::Flinch { .. } | Action::GuardStagger { .. } | Action::Dead);
    let same = std::mem::discriminant(&snap.action) == std::mem::discriminant(&local.fighter.action);
    if forced && !same {
        local.fighter.action = snap.action;
        local.fighter.queued_attack = false;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn animate_view_sword(
    time: Res<Time>,
    assets: Res<CharacterAssets>,
    shake: Res<Shake>,
    local: Query<(&LocalPlayer, &LocalFighter)>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<ViewSword>)>,
    mut sword: Query<(&mut Transform, &mut Visibility, &Children), (With<ViewSword>, Without<PlayerCamera>)>,
    mut blades: Query<(&Blade, &mut MeshMaterial3d<StandardMaterial>)>,
    mut tints: ResMut<crate::rig::BladeTints>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut arm: Query<(&crate::rig::ViewArm, &mut Visibility), (With<ViewArmRoot>, Without<ViewSword>)>,
    mut joints: Query<&mut Transform, (Without<ViewSword>, Without<PlayerCamera>, Without<ViewArmRoot>, Without<ViewGlow>)>,
    mut glow: Query<(&mut Transform, &mut Visibility), (With<ViewGlow>, Without<ViewSword>, Without<PlayerCamera>, Without<ViewArmRoot>)>,
    hints: Res<CombatHints>,
    mut cam_motion: Local<CamMotion>,
    mut smoothed: Local<Option<(Vec3, Quat)>>,
    work: Option<Res<crate::work_client::MyWork>>,
) {
    let Ok((p, f)) = local.single() else { return };
    let dt = time.delta_secs();
    // Камера идёт за ударом (плечи и голова двигаются вместе с клинком), плюс тряска.
    let target = swing_camera(&f.fighter.action, f.fighter.weapon);
    cam_motion.0 = cam_motion.0.lerp(target, 1.0 - (-18.0 * dt).exp());
    if let Ok(mut cam) = camera.single_mut() {
        let s = shake.0 * 0.04;
        let t = time.elapsed_secs() * 60.0;
        let m = cam_motion.0;
        cam.rotation = Quat::from_rotation_y(m.y) * Quat::from_rotation_x(p.pitch + m.x + (t.sin() * s)) * Quat::from_rotation_z(m.z + (t * 1.3).cos() * s);
    }
    let Ok((mut st, mut vis, children)) = sword.single_mut() else { return };
    *vis = if f.fighter.is_dead() || crate::work_client::hands_busy(work.as_deref()) { Visibility::Hidden } else { Visibility::Inherited };
    let (hilt, dir) = combat::view_weapon_pose(&f.fighter.action, f.fighter.weapon, time.elapsed_secs());
    // Удар в мире наклонён на 0.6 от взгляда, а камера — на весь взгляд; компенсируем разницу.
    let fix = Quat::from_rotation_x(-p.pitch * 0.4);
    // Меч от первого лица чуть приподнят, чтобы в кадре была рука с рукоятью.
    let (hilt, dir) = view_framing(fix * hilt, fix * dir);
    // Глядя под ноги, оружие с руками опускаем к низу кадра (держишь его у бока), иначе
    // рука с древком закрывает свои ноги.
    let look_down = ((-p.pitch - 0.6) / 0.7).clamp(0.0, 1.0);
    let lift = Quat::from_rotation_x(-0.6 * look_down);
    let lifted = |v: Vec3| VIEW_LIFT_PIVOT + lift * (v - VIEW_LIFT_PIVOT);
    let want_pos = lifted(Vec3::new(-0.06, -0.25 + VIEW_LIFT, -0.04) + hilt);
    let want_rot = lift * Quat::from_rotation_arc(Vec3::Y, dir);
    // Сглаживаем: смена фаз (комбо, отбитый удар, поправка сервера) без рывков.
    let k = 1.0 - (-35.0 * dt).exp();
    match *smoothed {
        Some((pos, rot)) if pos.distance(want_pos) < 0.8 => {
            st.translation = pos.lerp(want_pos, k);
            st.rotation = rot.slerp(want_rot, k);
        }
        _ => {
            st.translation = want_pos;
            st.rotation = want_rot;
        }
    }
    *smoothed = Some((st.translation, st.rotation));
    let riposte = matches!(f.fighter.action, Action::Windup { riposte: true, .. } | Action::Strike { riposte: true, .. });
    let _ = &assets;
    if let Ok((mut gt, mut gv)) = glow.single_mut() {
        let g = (riposte && hints.0).then(|| crate::client::riposte_glow_transform(&st, f.fighter.weapon)).flatten().filter(|_| *vis != Visibility::Hidden);
        *gv = if g.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(g) = g {
            *gt = g;
        }
    }
    for c in children.iter() {
        if let Ok((b, mut m)) = blades.get_mut(c) {
            m.0 = if riposte && hints.0 { assets.blade_red.clone() } else { tints.get(&mut materials, &b.0, f.finish) };
        }
    }
    // Правая рука держит рукоять; левая — древко копья спереди или поднята кулаком
    // в кулачной стойке; со щитом или одноручным оружием её в кадре нет.
    let (right_grip, left_grip) = crate::client::weapon_grips(f.fighter.weapon);
    let axis = st.rotation * Vec3::Y;
    let fists = f.fighter.weapon == combat::WeaponKind::Fists && !f.fighter.has_shield;
    for (arm, mut arm_vis) in &mut arm {
        let target = if !arm.left {
            Some((st.transform_point(right_grip), axis))
        } else if let Some(l) = left_grip {
            Some((st.transform_point(l), axis))
        } else if fists {
            let guard = matches!(f.fighter.action, Action::Windup { .. } | Action::Strike { .. } | Action::Recovery { .. });
            let fist = if guard { Vec3::new(-0.16, -0.08, -0.28) } else { Vec3::new(-0.2, -0.18, -0.3) };
            let (pos, _) = view_framing(fix * fist, Vec3::NEG_Z);
            Some((lifted(Vec3::new(-0.06, -0.25 + VIEW_LIFT, -0.04) + pos), lift * (fix * Vec3::new(0.15, 0.25, -1.0)).normalize()))
        } else {
            None
        };
        let Some((grip, axis)) = target else {
            *arm_vis = Visibility::Hidden;
            continue;
        };
        *arm_vis = *vis;
        let shoulder = if arm.left { VIEW_SHOULDER_L } else { VIEW_SHOULDER };
        for (joint, (pos, rot)) in arm.joints().into_iter().zip(crate::rig::view_arm_pose(shoulder, grip, axis, arm.left)) {
            if let Ok(mut t) = joints.get_mut(joint) {
                t.translation = pos;
                t.rotation = rot;
            }
        }
    }
}

/// Кадрирование меча от первого лица: замах в мире уходит за плечо и над головой — за край
/// экрана. Для глаза рукоять держим перед камерой (не ближе `VIEW_MIN_DEPTH`), высоту над
/// плечом сжимаем, а клинок не даём завалить назад — видно, откуда пойдёт удар.
/// Бой по-прежнему считается по настоящей позе.
fn view_framing(hilt: Vec3, dir: Vec3) -> (Vec3, Vec3) {
    const VIEW_MIN_DEPTH: f32 = 0.42;
    let mut y = if hilt.y > 0.0 { hilt.y * 0.6 } else { hilt.y };
    let mut z = hilt.z.min(-VIEW_MIN_DEPTH + 0.04);
    // Замах через грудь (рукоять левее центра): рука идёт ниже и дальше, не закрывая полэкрана.
    let cross = (-hilt.x / 0.3).clamp(0.0, 1.0);
    y = y.min(0.2 - 0.32 * cross);
    z = z.min(-0.38 - 0.16 * cross);
    // Клинок «раскрываем» поперёк взгляда: острие, смотрящее прямо от глаза, превращается
    // в точку — так дуга удара видна.
    let d = Vec3::new(dir.x, dir.y, (dir.z * 0.6).min(-0.08)).normalize_or(dir);
    (Vec3::new(hilt.x, y, z), d)
}

/// Сглаженный сдвиг камеры от удара: (наклон, поворот, крен).
#[derive(Default)]
struct CamMotion(Vec3);

/// Камера вслед за ударом: на замахе плечи уводят взгляд в сторону замаха, удар проносит его
/// через центр в другую сторону, укол — кивок вперёд, отбитый удар — рывок вверх.
/// Возвращает (наклон, поворот, крен) в радианах.
fn swing_camera(action: &Action, weapon: combat::WeaponKind) -> Vec3 {
    let at = |swing: Swing, m: f32, push: f32| {
        let s = swing.screen_dir();
        if swing == Swing::Thrust {
            return Vec3::new(-0.035 * push, 0.0, 0.0);
        }
        Vec3::new(0.025 * s.y * m - 0.015 * push, -0.03 * s.x * m, -0.05 * s.x * m)
    };
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    match *action {
        _ if weapon.ranged() => Vec3::ZERO,
        Action::Windup { t, dur, swing, .. } => {
            let k = 1.0 - (1.0 - (t.max(0.0) / dur.max(0.01)).clamp(0.0, 1.0)).powi(2);
            if swing == Swing::Thrust { Vec3::new(0.02 * k, 0.0, 0.0) } else { at(swing, k, 0.0) }
        }
        Action::Strike { t, swing, .. } => {
            let p = (t / weapon.timing(swing).release).clamp(0.0, 1.0);
            let push = (p * std::f32::consts::PI).sin();
            if swing == Swing::Thrust { at(swing, 0.0, push) } else { at(swing, 1.0 - 2.0 * smooth(p), push) }
        }
        Action::Recovery { t, swing } => {
            let k = 1.0 - smooth(t / weapon.timing(swing).recovery);
            if swing == Swing::Thrust { Vec3::ZERO } else { at(swing, -k, 0.0) }
        }
        Action::Recoil { t, dur } => {
            let k = (1.0 - t / dur).max(0.0).powi(2);
            Vec3::new(0.08 * k, -0.02 * k, 0.04 * k)
        }
        Action::GuardStagger { t, dur } => Vec3::new(0.035 * (1.0 - t / dur).max(0.0), 0.0, 0.0),
        Action::Flinch { t } => {
            let k = (1.0 - t / combat::FLINCH_TIME).max(0.0);
            Vec3::new(-0.04 * k, 0.0, 0.03 * k)
        }
        Action::Stagger { t, dur } => {
            let k = (1.0 - t / dur).max(0.0);
            Vec3::new(0.06 * k, 0.0, -0.05 * k)
        }
        _ => Vec3::ZERO,
    }
}

#[allow(clippy::too_many_arguments)]
fn feedback_effects(
    mut commands: Commands,
    mut events: MessageReader<CombatFeedback>,
    state: Res<NetState>,
    sparks: Res<SparkAssets>,
    index: Res<RemoteIndex>,
    remotes: Query<&RemoteActor>,
    mut banner: ResMut<Banner>,
    mut flash: ResMut<DamageFlash>,
    mut shake: ResMut<Shake>,
    mut local: Query<&mut LocalFighter>,
) {
    let me = state.my_id;
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5
    };
    let is_npc = |id: u64| index.0.get(&id).and_then(|e| remotes.get(*e).ok()).is_some_and(|r| r.kind.is_npc());
    for ev in events.read() {
        match *ev {
            CombatFeedback::Hit { attacker, target, outcome, at } => {
                let (count, mat, speed) = match outcome {
                    HitOutcome::Parried | HitOutcome::Chambered => (18, sparks.gold.clone(), 4.0),
                    HitOutcome::Blocked { .. } => (10, sparks.white.clone(), 3.0),
                    HitOutcome::Bounced => (8, sparks.white.clone(), 2.5),
                    HitOutcome::GuardBroken => (14, sparks.gold.clone(), 3.0),
                    HitOutcome::Damage { .. } => (12, sparks.blood.clone(), 2.0),
                };
                // Свою кровь не рисуем — вместо неё красная вспышка экрана.
                let count = if Some(target) == me && matches!(outcome, HitOutcome::Damage { .. }) { 0 } else { count };
                for _ in 0..count {
                    let v = Vec3::new(rnd(), rnd() + 0.6, rnd()).normalize() * speed * (0.5 + rnd() + 0.5);
                    commands.spawn((Spark { velocity: v, life: 0.45 }, Mesh3d(sparks.mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at)));
                }
                let mut say = |text: &str, color: Color| {
                    banner.text = text.to_string();
                    banner.color = color;
                    banner.time_left = 1.2;
                };
                let gold = Color::srgb(1.0, 0.85, 0.3);
                let red = Color::srgb(1.0, 0.4, 0.3);
                let grey = Color::srgb(0.8, 0.8, 0.85);
                // Своё состояние подгоняем под исход сразу (сервер подтвердит снимком).
                let mut me_f = local.single_mut().ok();
                match outcome {
                    HitOutcome::Parried if Some(target) == me => {
                        say("Отбил! Рипост — ЛКМ", gold);
                        shake.0 = shake.0.max(0.2);
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.action = Action::GuardStagger { t: 0.0, dur: combat::PARRY_SHOCK };
                            f.fighter.riposte_timer = combat::PARRY_SHOCK + combat::RIPOSTE_WINDOW;
                            f.fighter.last_parry_timed = true;
                            f.fighter.guard_used = true;
                        }
                    }
                    HitOutcome::Parried if Some(attacker) == me => {
                        say("Удар отбит — защищайся (ПКМ)!", red);
                        shake.0 = 0.7;
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.action = Action::Recoil { t: 0.0, dur: combat::PARRIED_RECOIL };
                            f.fighter.reset_combo();
                        }
                    }
                    HitOutcome::Chambered if Some(target) == me => {
                        say("Встречный удар!", gold);
                        if let Some(f) = me_f.as_mut()
                            && let Action::Windup { t, dur, swing, .. } = f.fighter.action
                        {
                            let left = (dur - t).min(combat::RIPOSTE_WINDUP).max(0.05);
                            f.fighter.action = Action::Windup { t: 0.0, dur: left, swing, riposte: false, morphed: true };
                        }
                    }
                    HitOutcome::Chambered if Some(attacker) == me => {
                        say("Ваш удар встречен", red);
                        shake.0 = 0.6;
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.action = Action::Recoil { t: 0.0, dur: combat::CHAMBERED_RECOIL };
                            f.fighter.reset_combo();
                        }
                    }
                    HitOutcome::Blocked { tier } if Some(attacker) == me => {
                        let shield = index.0.get(&target).and_then(|e| remotes.get(*e).ok()).is_some_and(|r| r.fighter.has_shield);
                        if shield && tier != combat::ImpactTier::Heavy {
                            say("Принято на щит", grey);
                            if let Some(f) = me_f.as_mut() {
                                f.fighter.action = Action::Recoil { t: 0.0, dur: combat::BLOCK_BOUNCE };
                                f.fighter.reset_combo();
                            }
                        } else {
                            // Глухой блок: инициатива у нас — можно комбо.
                            say("Блок — дави!", grey);
                            if let Some(f) = me_f.as_mut() {
                                f.fighter.swing_landed = true;
                            }
                        }
                    }
                    HitOutcome::Blocked { .. } if Some(target) == me => {
                        shake.0 = shake.0.max(0.25);
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.guard_used = true;
                            if !f.fighter.has_shield {
                                say("Глухой блок: поздно для рипоста", grey);
                            }
                        }
                    }
                    HitOutcome::GuardBroken if Some(target) == me => {
                        say("Защиту пробили — нет сил", red);
                        shake.0 = 0.8;
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.action = Action::Stagger { t: 0.0, dur: combat::GUARD_BREAK_STAGGER };
                        }
                    }
                    HitOutcome::GuardBroken if Some(attacker) == me => {
                        say("Защита пробита!", gold);
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.swing_landed = true;
                        }
                    }
                    HitOutcome::Bounced if Some(target) == me => {
                        say("Клинок ударился о стену", grey);
                        shake.0 = shake.0.max(0.4);
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.action = Action::Stagger { t: 0.0, dur: combat::WALL_BOUNCE };
                            f.fighter.reset_combo();
                        }
                    }
                    HitOutcome::Damage { amount, .. } if Some(target) == me => {
                        flash.0 = (0.3 + amount / 150.0).min(0.8);
                        shake.0 = 0.6;
                    }
                    HitOutcome::Damage { head, .. } if Some(attacker) == me => {
                        if head {
                            say("В голову!", Color::srgb(1.0, 0.6, 0.3));
                        }
                        if let Some(f) = me_f.as_mut() {
                            f.fighter.swing_landed = true;
                        }
                        shake.0 = shake.0.max(if head { 0.3 } else if is_npc(target) { 0.2 } else { 0.0 });
                    }
                    _ => {}
                }
            }
            CombatFeedback::Died { id, killer } => {
                if Some(id) == me {
                    banner.text = "ВЫ ПОГИБЛИ".into();
                    banner.color = Color::srgb(0.8, 0.1, 0.1);
                    banner.time_left = 5.0;
                } else if killer == me && me.is_some() {
                    banner.text = "Враг повержен".into();
                    banner.color = Color::srgb(0.9, 0.9, 0.9);
                    banner.time_left = 1.5;
                }
            }
        }
    }
}

fn update_sparks(mut commands: Commands, time: Res<Time>, mut sparks: Query<(Entity, &mut Spark, &mut Transform)>) {
    let dt = time.delta_secs();
    for (e, mut s, mut t) in &mut sparks {
        s.life -= dt;
        if s.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        s.velocity.y -= 9.8 * dt;
        t.translation += s.velocity * dt;
        t.scale = Vec3::splat((s.life / 0.45).max(0.05));
    }
}

fn decay_effects(time: Res<Time>, mut banner: ResMut<Banner>, mut flash: ResMut<DamageFlash>, mut shake: ResMut<Shake>) {
    let dt = time.delta_secs();
    banner.time_left = (banner.time_left - dt).max(0.0);
    flash.0 = (flash.0 - dt * 1.5).max(0.0);
    shake.0 = (shake.0 - dt * 3.0).max(0.0);
}
