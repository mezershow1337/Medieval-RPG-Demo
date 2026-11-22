// Создано tools/models/build_humans.py — не править руками.
// Размеры скелета под тело из assets/models/humans.glb.
use bevy::math::Vec3;

/// Смещения суставов от родителя в покое (конечности висят прямо вниз).
pub const REST: [Vec3; 15] = [
    Vec3::new(0.0, 0.9529, -0.005662), // pelvis
    Vec3::new(0.0, 0.145, 0.02281), // torso
    Vec3::new(0.0, 0.4066, -0.03119), // head
    Vec3::new(0.2107, 0.2866, -0.03848), // upper_arm_r
    Vec3::new(0.0, -0.2453, 0.0), // forearm_r
    Vec3::new(0.0, -0.264, 0.0), // hand_r
    Vec3::new(-0.2107, 0.2866, -0.03848), // upper_arm_l
    Vec3::new(0.0, -0.2453, 0.0), // forearm_l
    Vec3::new(0.0, -0.264, 0.0), // hand_l
    Vec3::new(0.1098, -0.005876, 0.01065), // thigh_r
    Vec3::new(0.0, -0.4328, 0.0), // shin_r
    Vec3::new(0.0, -0.448, 0.0), // foot_r
    Vec3::new(-0.1098, -0.005876, 0.01065), // thigh_l
    Vec3::new(0.0, -0.4328, 0.0), // shin_l
    Vec3::new(0.0, -0.448, 0.0), // foot_l
];
pub const UPPER_ARM: f32 = 0.2453;
pub const FOREARM: f32 = 0.264;
pub const THIGH: f32 = 0.4328;
pub const SHIN: f32 = 0.448;
/// Высота голеностопа над землёй, когда стопа стоит ровно.
pub const ANKLE_HEIGHT: f32 = 0.07226;
/// Центр кулака (где рукоять) и ось отверстия кулака в системе кисти: правая и левая.
pub const GRIP_R: Vec3 = Vec3::new(-0.01949, -0.09466, 0.00541);
pub const GRIP_AXIS_R: Vec3 = Vec3::new(0.007856, 0.1682, 0.9857);
pub const GRIP_L: Vec3 = Vec3::new(0.02018, -0.09479, 0.005017);
pub const GRIP_AXIS_L: Vec3 = Vec3::new(-0.007856, 0.1682, 0.9857);
