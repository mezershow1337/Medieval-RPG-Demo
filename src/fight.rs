//! Бой на стороне клиента: ввод (ЛКМ — удар, ПКМ — парирование), предсказание своего
//! состояния, меч в руках от первого лица и эффекты попаданий.

use crate::client::{CharacterAssets, NetState, RemoteActor, RemoteIndex};
use crate::rig::Blade;
use crate::combat::{self, Action, Fighter, HitOutcome};
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
            .add_systems(Startup, setup_sparks)
            .add_systems(
                Update,
                (
                    combat_input.before(crate::player::grab_cursor),
                    tick_local,
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
/// Насколько меч от первого лица выше, чем в мире (только внешний вид).
const VIEW_LIFT: f32 = 0.36;

pub fn spawn_view_sword(parent: &mut ChildSpawnerCommands, _assets: &CharacterAssets) {
    parent.spawn((ViewSword, ShownViewWeapon(combat::WeaponKind::Spatha, None), crate::rig::NeedsWeapon(combat::WeaponKind::Spatha), Transform::default(), Visibility::default()));
    parent.spawn((ViewArmRoot, crate::rig::NeedsViewArm(None), Transform::default(), Visibility::default()));
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
    arms: Query<Entity, (With<ViewArmRoot>, With<crate::rig::ViewArm>)>,
    net: Option<Res<NetState>>,
    mut shown: Local<crate::gear_look::Gear>,
) {
    let Ok(f) = local.single() else { return };
    if f.gear == *shown {
        return;
    }
    // Ждём, пока рука соберётся (у неё уже есть суставы), иначе переодевать нечего.
    let Ok(e) = arms.single() else { return };
    *shown = f.gear;
    let id = net.as_ref().and_then(|n| n.my_id).unwrap_or(0);
    let outfit = crate::rig::Outfit::for_actor(ActorKind::Player, id).with_gear(&f.gear);
    commands.entity(e).despawn_related::<Children>().remove::<crate::rig::ViewArm>().insert(crate::rig::NeedsViewArm(Some(outfit)));
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

fn combat_input(
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    client: Option<ResMut<RenetClient>>,
    mut local: Query<(&LocalPlayer, &mut LocalFighter)>,
    grab: Option<Res<crate::props_client::PropGrab>>,
    work: Option<Res<crate::work_client::MyWork>>,
) {
    // Во время работы руками ЛКМ — удар молотом по заготовке, а не мечом.
    if !cursor_locked(&cursor) || crate::work_client::hands_busy(work.as_deref()) {
        return;
    }
    // ЛКМ по предмету — захват (props_client), ПКМ с предметом в руках — бросок.
    let (no_attack, no_guard) = grab.map(|g| (g.blocks_attack(), g.blocks_guard())).unwrap_or((false, false));
    let Some(mut client) = client else { return };
    let Ok((p, mut f)) = local.single_mut() else { return };
    if f.fighter.is_dead() {
        return;
    }
    if mouse.just_pressed(MouseButton::Left) && !no_attack {
        f.fighter.try_attack();
        client.send_message(CH_RELIABLE, encode(&ClientMessage::Attack { yaw: p.yaw, pitch: p.pitch }));
    }
    // Защита: держим ПКМ. Оружием — парирование, щитом — блок.
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
    let forced = matches!(snap.action, Action::Stagger { .. } | Action::Flinch { .. } | Action::GuardStagger { .. } | Action::Dead);
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
    models: Option<Res<crate::rig::CharacterModels>>,
    shake: Res<Shake>,
    local: Query<(&LocalPlayer, &LocalFighter)>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<ViewSword>)>,
    mut sword: Query<(&mut Transform, &mut Visibility, &Children), (With<ViewSword>, Without<PlayerCamera>)>,
    mut blades: Query<(&Blade, &mut MeshMaterial3d<StandardMaterial>)>,
    mut tints: ResMut<crate::rig::BladeTints>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut arm: Query<(&crate::rig::ViewArm, &mut Visibility), (With<ViewArmRoot>, Without<ViewSword>)>,
    mut joints: Query<&mut Transform, (Without<ViewSword>, Without<PlayerCamera>, Without<ViewArmRoot>)>,
    work: Option<Res<crate::work_client::MyWork>>,
) {
    let Ok((p, f)) = local.single() else { return };
    // Поворот камеры по вертикали и тряска.
    if let Ok(mut cam) = camera.single_mut() {
        let s = shake.0 * 0.04;
        let t = time.elapsed_secs() * 60.0;
        cam.rotation = Quat::from_rotation_x(p.pitch + (t.sin() * s)) * Quat::from_rotation_z((t * 1.3).cos() * s);
    }
    let Ok((mut st, mut vis, children)) = sword.single_mut() else { return };
    *vis = if f.fighter.is_dead() || crate::work_client::hands_busy(work.as_deref()) { Visibility::Hidden } else { Visibility::Inherited };
    let (hilt, dir) = combat::weapon_pose(&f.fighter.action, f.fighter.weapon, time.elapsed_secs());
    // Удар в мире наклонён на 0.6 от взгляда, а камера — на весь взгляд; компенсируем разницу.
    let fix = Quat::from_rotation_x(-p.pitch * 0.4);
    // Меч от первого лица чуть приподнят, чтобы в кадре была рука с рукоятью.
    st.translation = Vec3::new(-0.06, -0.25 + VIEW_LIFT, -0.04) + fix * hilt;
    st.rotation = Quat::from_rotation_arc(Vec3::Y, (fix * dir).normalize());
    let _ = (&assets, &models);
    for c in children.iter() {
        if let Ok((b, mut m)) = blades.get_mut(c) {
            m.0 = tints.get(&mut materials, &b.0, f.finish);
        }
    }
    // Рука держит рукоять.
    if let Ok((arm, mut arm_vis)) = arm.single_mut() {
        *arm_vis = *vis;
        let grip = st.transform_point(crate::client::weapon_grips(f.fighter.weapon).0);
        let axis = st.rotation * Vec3::Y;
        for (joint, (pos, rot)) in arm.joints().into_iter().zip(crate::rig::view_arm_pose(VIEW_SHOULDER, grip, axis)) {
            if let Ok(mut t) = joints.get_mut(joint) {
                t.translation = pos;
                t.rotation = rot;
            }
        }
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
                    HitOutcome::Parried => (18, sparks.gold.clone(), 4.0),
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
                match outcome {
                    HitOutcome::Parried if Some(target) == me => say("PARRY! Riposte now (LMB)", Color::srgb(1.0, 0.85, 0.3)),
                    HitOutcome::Parried if Some(attacker) == me => {
                        say("PARRIED - you are staggered", Color::srgb(1.0, 0.4, 0.3));
                        shake.0 = 0.8;
                        if let Ok(mut f) = local.single_mut() {
                            f.fighter.action = Action::Stagger { t: 0.0, dur: 1.0 };
                            f.fighter.reset_combo();
                        }
                    }
                    HitOutcome::Blocked { tier } if Some(attacker) == me => {
                        say("Blocked by shield", Color::srgb(0.8, 0.8, 0.85));
                        if tier != combat::ImpactTier::Heavy
                            && let Ok(mut f) = local.single_mut()
                        {
                            f.fighter.action = Action::Stagger { t: 0.0, dur: combat::BLOCK_BOUNCE };
                            f.fighter.reset_combo();
                        }
                    }
                    HitOutcome::Blocked { .. } if Some(target) == me => shake.0 = shake.0.max(0.25),
                    HitOutcome::GuardBroken if Some(target) == me => {
                        say("Guard broken - out of stamina", Color::srgb(1.0, 0.45, 0.3));
                        shake.0 = 0.8;
                        if let Ok(mut f) = local.single_mut() {
                            f.fighter.action = Action::Stagger { t: 0.0, dur: combat::GUARD_BREAK_STAGGER };
                        }
                    }
                    HitOutcome::GuardBroken if Some(attacker) == me => say("Guard broken!", Color::srgb(1.0, 0.85, 0.3)),
                    HitOutcome::Bounced if Some(target) == me => {
                        say("Your blade hit the wall", Color::srgb(0.8, 0.8, 0.85));
                        shake.0 = shake.0.max(0.4);
                        if let Ok(mut f) = local.single_mut() {
                            f.fighter.action = Action::Stagger { t: 0.0, dur: combat::WALL_BOUNCE };
                            f.fighter.reset_combo();
                        }
                    }
                    HitOutcome::Damage { amount, .. } if Some(target) == me => {
                        flash.0 = (0.3 + amount / 150.0).min(0.8);
                        shake.0 = 0.6;
                    }
                    HitOutcome::Damage { head: true, .. } if Some(attacker) == me => {
                        say("Headshot!", Color::srgb(1.0, 0.6, 0.3));
                        shake.0 = shake.0.max(0.25);
                    }
                    HitOutcome::Damage { .. } if Some(attacker) == me && is_npc(target) => shake.0 = shake.0.max(0.2),
                    _ => {}
                }
            }
            CombatFeedback::Died { id, killer } => {
                if Some(id) == me {
                    banner.text = "YOU DIED".into();
                    banner.color = Color::srgb(0.8, 0.1, 0.1);
                    banner.time_left = 5.0;
                } else if killer == me && me.is_some() {
                    banner.text = "Enemy slain".into();
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
