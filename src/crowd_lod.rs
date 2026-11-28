//! Люди вдали проще (ЛОД толпы): на агоре, в порту и на главной улице в кадре бывает по
//! сотне жителей, а каждый — до 16 отдельных деталей (голова, кисти, глаза, брови, пояс,
//! туника, штаны, обувь, по три слоя волос и бороды) и до 70 тыс. треугольников. Всё это
//! рисовалось и в кадр, и в предпроход, и в каждый каскад теней — отсюда просадки FPS.
//!
//! Что меняется с расстоянием от камеры (с запасом, чтобы детали не мигали на границе):
//! - ближе [`SHELL_FAR`] — человек целиком;
//! - дальше — без верхних слоёв волос и бороды (`.l1`, `.l2`): с такого расстояния
//!   объём причёски не разглядеть, остаётся нижний слой того же цвета;
//! - дальше [`FINE_FAR`] — без глаз, бровей, пояса и щетины (это пара пикселей);
//! - дальше [`SHADOW_FAR`] — тело не отбрасывает тень (её там почти не видно, а каскад
//!   теней рисует каждого заново).
//!
//! Верхние слои волос, глаза, брови, пояс и щетина теней не дают никогда: они лежат на
//! теле или внутри головы, и их тень ничего не добавляет.

use bevy::camera::visibility::VisibilitySystems;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

/// Ближе этого — все слои волос и бороды.
pub const SHELL_FAR: f32 = 14.0;
/// Дальше этого — без глаз, бровей, пояса и щетины.
pub const FINE_FAR: f32 = 30.0;
/// Дальше этого тело не отбрасывает тень.
pub const SHADOW_FAR: f32 = 45.0;
/// Запас на границах: деталь пропадает на `X + HYST`, а возвращается на `X − HYST`.
const HYST: f32 = 1.5;

/// Что за деталь тела (ставится при создании детали в `rig`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartLod {
    /// Тело, одежда, доспех, шлем, нижний слой волос и бороды.
    Core,
    /// Верхние слои волос и бороды.
    Shell,
    /// Глаза, брови, пояс, щетина.
    Fine,
}

impl PartLod {
    /// Вид детали по имени детали в glb и имени материала примитива.
    pub fn of(part: &str, material: &str) -> Self {
        if material.ends_with(".l1") || material.ends_with(".l2") {
            PartLod::Shell
        } else if matches!(part, "eyes" | "brows" | "belt" | "beard.stubble") {
            PartLod::Fine
        } else {
            PartLod::Core
        }
    }

    /// Даёт ли деталь тень хоть когда-нибудь.
    pub fn casts_shadow(self) -> bool {
        self == PartLod::Core
    }

    /// Видна ли деталь на ступени `tier`.
    fn shown(self, tier: u8) -> bool {
        match self {
            PartLod::Core => true,
            PartLod::Shell => tier < 1,
            PartLod::Fine => tier < 2,
        }
    }
}

/// Тень детали снята ЛОДом (а не кем-то ещё — например, своим телом от первого лица):
/// возвращаем только такие.
#[derive(Component)]
struct LodShadowOff;

/// Текущая ступень детализации человека (на корне скелета).
#[derive(Component, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct CrowdTier(pub u8);

/// Ступень по расстоянию с учётом прежней (запас против мигания).
pub fn tier_for(dist: f32, prev: u8) -> u8 {
    const EDGES: [f32; 3] = [SHELL_FAR, FINE_FAR, SHADOW_FAR];
    let mut tier = prev.min(3);
    // Дальше: переходим, только отойдя за край с запасом.
    while tier < 3 && dist > EDGES[tier as usize] + HYST {
        tier += 1;
    }
    // Ближе: возвращаемся, только подойдя к краю с запасом.
    while tier > 0 && dist < EDGES[tier as usize - 1] - HYST {
        tier -= 1;
    }
    tier
}

/// ЛОД толпы включён (по умолчанию да). `MRPG_CROWD_LOD=0` — как было раньше: все детали
/// всегда, тени от всех, рамки по суставам каждый кадр (для сравнения FPS).
pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("MRPG_CROWD_LOD").map_or(true, |v| v != "0"))
}

pub struct CrowdLodPlugin;

impl Plugin for CrowdLodPlugin {
    fn build(&self, app: &mut App) {
        if enabled() {
            app.add_systems(PostUpdate, crowd_lod.before(VisibilitySystems::VisibilityPropagate));
        }
    }
}

#[allow(clippy::type_complexity)]
fn crowd_lod(
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut roots: Query<(Entity, &GlobalTransform, &Children, Option<&mut CrowdTier>), With<crate::rig::Rig>>,
    fresh: Query<&ChildOf, Added<PartLod>>,
    mut parts: Query<(&PartLod, &mut Visibility, Has<NotShadowCaster>, Has<LodShadowOff>)>,
) {
    let Some(eye) = camera.iter().find(|(c, _)| c.is_active).map(|(_, t)| t.translation()) else {
        return;
    };
    // Новые детали (скелет только собран или переодет) получают ступень своего человека.
    let mut dirty: Vec<Entity> = fresh.iter().map(|c| c.parent()).collect();
    dirty.sort_unstable();
    dirty.dedup();
    for (root, at, children, tier) in &mut roots {
        let prev = tier.as_deref().copied();
        let want = tier_for(at.translation().distance(eye), prev.map_or(0, |t| t.0));
        let changed = prev != Some(CrowdTier(want));
        if !changed && dirty.binary_search(&root).is_err() {
            continue;
        }
        match tier {
            Some(mut t) => t.0 = want,
            None => {
                commands.entity(root).insert(CrowdTier(want));
            }
        }
        for child in children.iter() {
            let Ok((lod, mut vis, no_shadow, lod_off)) = parts.get_mut(child) else { continue };
            vis.set_if_neq(if lod.shown(want) { Visibility::Inherited } else { Visibility::Hidden });
            if !lod.casts_shadow() {
                continue;
            }
            let shadow = want < 3;
            if !shadow && !no_shadow {
                commands.entity(child).insert((NotShadowCaster, LodShadowOff));
            } else if shadow && lod_off {
                commands.entity(child).remove::<(NotShadowCaster, LodShadowOff)>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_follow_distance_with_hysteresis() {
        assert_eq!(tier_for(5.0, 0), 0);
        assert_eq!(tier_for(20.0, 0), 1);
        assert_eq!(tier_for(35.0, 0), 2);
        assert_eq!(tier_for(80.0, 0), 3);
        assert_eq!(tier_for(3.0, 3), 0);
        // У самой границы ступень не скачет туда-сюда.
        assert_eq!(tier_for(SHELL_FAR + 1.0, 0), 0);
        assert_eq!(tier_for(SHELL_FAR - 1.0, 1), 1);
        assert_eq!(tier_for(SHADOW_FAR + 1.0, 2), 2);
        assert_eq!(tier_for(SHADOW_FAR - 1.0, 3), 3);
    }

    #[test]
    fn parts_are_sorted() {
        assert_eq!(PartLod::of("hair.long", "hair_long"), PartLod::Core);
        assert_eq!(PartLod::of("hair.long", "hair_long.l1"), PartLod::Shell);
        assert_eq!(PartLod::of("beard.full", "beard_full.l2"), PartLod::Shell);
        assert_eq!(PartLod::of("eyes", "eyes"), PartLod::Fine);
        assert_eq!(PartLod::of("beard.stubble", "beard_stubble"), PartLod::Fine);
        assert_eq!(PartLod::of("tunic.byz", "tunic_byz.skirt"), PartLod::Core);
        assert!(!PartLod::Shell.shown(1) && PartLod::Shell.shown(0));
        assert!(PartLod::Fine.shown(1) && !PartLod::Fine.shown(2));
        assert!(PartLod::Core.shown(3));
    }
}

#[cfg(test)]
mod crowd_spots {
    /// Где днём толпа: `cargo test --release crowd_spots -- --ignored --nocapture`
    /// (места для замера FPS в толпе).
    #[test]
    #[ignore]
    fn print_crowd_spots() {
        use bevy::math::Vec3Swizzles;
        let town = crate::daily_life::test_town();
        let plans = town.plan_day(3);
        for hour in [8.0, 10.0, 12.0, 15.0, 18.0, 20.0] {
            let pts: Vec<_> = plans.people.iter().filter_map(|p| p.state(hour * 3600.0)).filter(|s| s.act != crate::daily_life::Act::Away).map(|s| s.pos).collect();
            let mut best: Vec<(usize, bevy::math::Vec3)> = pts.iter().map(|&c| (pts.iter().filter(|p| p.xz().distance(c.xz()) < 25.0).count(), c)).collect();
            best.sort_by(|a, b| b.0.cmp(&a.0));
            let mut shown: Vec<bevy::math::Vec3> = Vec::new();
            for (n, c) in best {
                if shown.iter().any(|s| s.xz().distance(c.xz()) < 40.0) {
                    continue;
                }
                shown.push(c);
                eprintln!("{hour:>4} ч: {n} человек в 25 м от {c:.0}");
                if shown.len() == 4 {
                    break;
                }
            }
        }
    }
}
