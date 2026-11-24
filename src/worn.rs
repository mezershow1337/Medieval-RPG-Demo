//! Вещи на персонаже, которые не одежда: оружие в руке своей моделью (у каждого из 80 видов
//! свой вид — та же 3D-модель, что лежит на земле и рисуется на иконке), убранное оружие
//! на поясе или за спиной, колчан, нательный крест и перстень.
//!
//! Модели предметов (`icons::ItemModels`) лежат «на земле»: длинная ось вдоль −Z (острие к −Z,
//! навершие/торец к +Z), толщина вверх по +Y, низ на y = 0, центр над началом координат.
//! Здесь они переводятся в систему оружия рига (клинок/древко по +Y, кулак в `weapon_grips`).

use crate::combat::WeaponKind;
use crate::gear_look::Gear;
use crate::icons::ItemModels;
use crate::items::ItemBaseId;
use crate::rig::{Bone, Rig};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use std::f32::consts::{FRAC_PI_2, PI};

/// Есть ли у предмета своя 3D-модель (иначе в руке рисуется общая модель вида оружия).
pub fn has_model(base: ItemBaseId) -> bool {
    crate::icons::own_art(base).is_some()
}

/// Где на модели предмета рука: расстояние от торца (навершия, конца древка) к острию.
fn grip_from_butt(kind: WeaponKind, len: f32) -> f32 {
    match kind {
        WeaponKind::Spatha => 0.075,
        WeaponKind::Seax => 0.06,
        WeaponKind::Axe => 0.13,
        WeaponKind::Spear => (len * 0.3).min(0.55),
        WeaponKind::Bow | WeaponKind::Fists => len * 0.5,
    }
}

/// Поставить модель предмета так, чтобы её точка на оси (`along` м от торца, `None` — середина)
/// оказалась в `at`, а острие смотрело по `q * +Y`.
fn place(base: ItemBaseId, along: Option<f32>, at: Vec3, q: Quat) -> Transform {
    let [_, hy, hz] = crate::icons::model_half_extents(base);
    let p = Vec3::new(0.0, hy, along.map_or(0.0, |d| hz - d));
    // −Z модели (острие) -> +Y оружия, +Y модели (толщина) -> +Z.
    let rot = q * Quat::from_rotation_x(FRAC_PI_2);
    Transform { translation: at - rot * p, rotation: rot, scale: Vec3::ONE }
}

/// Модель предмета в руке, в системе оружия рига.
pub fn in_hand(base: ItemBaseId, kind: WeaponKind) -> Transform {
    let len = crate::icons::model_half_extents(base)[2] * 2.0;
    place(base, Some(grip_from_butt(kind, len)), crate::client::weapon_grips(kind).0, Quat::IDENTITY)
}

/// Показать оружие под сущностью `e`: своя модель предмета, если есть, иначе общая по виду.
pub fn set_weapon_model(commands: &mut Commands, e: Entity, kind: WeaponKind, base: Option<ItemBaseId>, finish: u8, models: Option<&ItemModels>) {
    commands.entity(e).despawn_related::<Children>();
    match (base.filter(|b| has_model(*b) && kind != WeaponKind::Fists), models) {
        (Some(b), Some(models)) => {
            commands.entity(e).with_children(|w| {
                let mut m = w.spawn((WorldAssetRoot(models.get(b)), in_hand(b, kind)));
                if finish < 160 {
                    m.insert(crate::rig::LookTint(finish));
                }
            });
        }
        _ => {
            commands.entity(e).insert(crate::rig::NeedsWeapon(kind));
        }
    }
}

/// Что носится на теле помимо одежды.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    pub sheathed: Option<u16>,
    pub quiver: Option<u16>,
    pub neck: Option<u16>,
    pub ring: Option<u16>,
}

impl Extras {
    pub fn of(gear: &Gear) -> Self {
        Extras { sheathed: gear.main_hand.filter(|_| gear.holstered), quiver: gear.quiver, neck: gear.neck, ring: gear.ring }
    }
}

/// Вешается на сущность со скелетом (`NeedsRig` / `Rig`): что на нём должно висеть.
#[derive(Component, Default)]
pub struct Worn {
    pub want: Extras,
    built: Option<Extras>,
    spawned: Vec<Entity>,
}

impl Worn {
    pub fn new(want: Extras) -> Self {
        Self { want, built: None, spawned: Vec::new() }
    }
    /// Скелет пересобран (его дети удалены вместе с вещами) — повесить заново.
    pub fn reset(&mut self) {
        self.built = None;
        self.spawned.clear();
    }
}

/// Куда и как вешается вещь: (кость, положение предмета в системе кости).
pub fn mount(slot: &str, base: ItemBaseId) -> (Bone, Transform) {
    match slot {
        "sheath" => {
            let kind = WeaponKind::from_item_name(base.base().name);
            if matches!(kind, WeaponKind::Spear | WeaponKind::Bow) {
                // Копьё и лук — за спиной наискось (от правого бедра к левому плечу).
                (Bone::Torso, place(base, None, Vec3::new(0.0, 0.12, 0.17), Quat::from_rotation_z(0.55)))
            } else {
                // Меч, сакс, топор, булава — у левого бедра, рукоять вперёд-вверх, остриё вниз-назад.
                let len = crate::icons::model_half_extents(base)[2] * 2.0;
                let q = Quat::from_rotation_z(-0.12) * Quat::from_rotation_x(PI - 0.45);
                (Bone::Pelvis, place(base, Some(grip_from_butt(kind, len)), Vec3::new(-0.2, 0.04, -0.08), q))
            }
        }
        // Колчан со стрелами у правого бедра сзади, оперением вверх.
        "quiver" => (Bone::Pelvis, place(base, None, Vec3::new(0.19, -0.02, 0.1), Quat::from_rotation_z(0.15) * Quat::from_rotation_x(PI + 0.2))),
        // Нательный крест на груди: лицом вперёд, верхом вверх.
        "neck" => (Bone::Torso, Transform::from_translation(Vec3::new(0.0, 0.15, -0.168)).with_rotation(Quat::from_rotation_z(PI) * Quat::from_rotation_x(-FRAC_PI_2))),
        // Перстень на левой руке.
        _ => (Bone::HandL, Transform::from_translation(Vec3::new(0.01, -0.075, -0.02)).with_rotation(Quat::from_rotation_x(FRAC_PI_2))),
    }
}

/// Повесить вещи на суставы готовых скелетов.
pub fn attach_worn(mut commands: Commands, models: Option<Res<ItemModels>>, mut rigs: Query<(&Rig, &mut Worn, Option<&crate::rig::RigBulk>)>) {
    let Some(models) = models else { return };
    for (rig, mut worn, bulk) in &mut rigs {
        let bulk = bulk.map_or(0.0, |b| b.0);
        if worn.built == Some(worn.want) {
            continue;
        }
        for e in worn.spawned.drain(..) {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.despawn();
            }
        }
        let w = worn.want;
        for (slot, id) in [("sheath", w.sheathed), ("quiver", w.quiver), ("neck", w.neck), ("ring", w.ring)] {
            let Some(base) = id.map(ItemBaseId).filter(|b| (b.0 as usize) < crate::items::ITEM_BASES.len() && has_model(*b)) else { continue };
            let (bone, mut tf) = mount(slot, base);
            // У плотного телосложения грудь, бока и спина дальше от костей.
            tf.translation += bulk
                * match (slot, bone) {
                    ("neck", _) => Vec3::new(0.0, 0.0, -0.045),
                    ("quiver", _) => Vec3::new(0.035, 0.0, 0.02),
                    (_, Bone::Torso) => Vec3::new(0.0, 0.0, 0.05),
                    ("sheath", _) => Vec3::new(-0.035, 0.0, -0.015),
                    _ => Vec3::ZERO,
                };
            let e = commands.spawn((WorldAssetRoot(models.get(base)), tf, ChildOf(rig.joint(bone)))).id();
            worn.spawned.push(e);
        }
        worn.built = Some(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sword_in_hand_points_up_from_the_fist() {
        let base = ItemBaseId::by_name("Спата");
        let [_, hy, hz] = crate::icons::model_half_extents(base);
        let tf = in_hand(base, WeaponKind::Spatha);
        // Остриё (−Z модели) выше кулака, навершие — чуть ниже.
        let tip = tf.transform_point(Vec3::new(0.0, hy, -hz));
        let butt = tf.transform_point(Vec3::new(0.0, hy, hz));
        assert!(tip.y > 0.6, "{tip}");
        assert!(butt.y < -0.06 && butt.y > -0.2, "{butt}");
        assert!(tip.x.abs() < 1e-4 && tip.z.abs() < 1e-4);
    }

    #[test]
    fn every_weapon_can_be_carried_and_every_jewel_worn() {
        for (i, b) in crate::items::ITEM_BASES.iter().enumerate() {
            let id = ItemBaseId(i as u16);
            let slot = match b.slot {
                Some(crate::items::EquipSlot::MainHand) => "sheath",
                Some(crate::items::EquipSlot::Necklace) => "neck",
                Some(crate::items::EquipSlot::Ring) => "ring",
                _ if crate::archery::arrow_type(b.name).is_some() => "quiver",
                _ => continue,
            };
            assert!(has_model(id), "нет модели у «{}»", b.name);
            let (_, tf) = mount(slot, id);
            assert!(tf.translation.is_finite() && tf.rotation.is_normalized(), "{}", b.name);
        }
    }

    #[test]
    fn sword_hangs_on_the_left_hip_and_spear_on_the_back() {
        let (bone, tf) = mount("sheath", ItemBaseId::by_name("Спата"));
        assert_eq!(bone, Bone::Pelvis);
        let [_, hy, hz] = crate::icons::model_half_extents(ItemBaseId::by_name("Спата"));
        let tip = tf.transform_point(Vec3::new(0.0, hy, -hz));
        let butt = tf.transform_point(Vec3::new(0.0, hy, hz));
        // Слева (−X у скелета), остриё ниже рукояти.
        assert!(tf.translation.x < -0.1 && tip.y < butt.y, "{tip} {butt}");
        let (bone, tf) = mount("sheath", ItemBaseId::by_name("Контос"));
        assert_eq!(bone, Bone::Torso);
        // За спиной (+Z — спина).
        assert!(tf.translation.z > 0.1);
    }
}
