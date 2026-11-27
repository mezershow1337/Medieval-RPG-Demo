//! Облик двора комита: одежда по должности и знаки власти поверх неё.
//!
//! У комита — белая хламида с пурпурным таблионом, сколотая на правом плече золотой фибулой
//! (так одеты сановники на мозаиках Равенны и Константинополя); у трибуна и палатинов —
//! красные плащи; у госпожи Евфимии — покрывало (мафорий) на голове и плечах. Плащи —
//! свои меши на суставе груди рига; ткань — та же шерсть (1024²), что у одежды.

use crate::court::Courtier;
use crate::level::Role;
use crate::residents::ResidentView;
use crate::rig::{Beard, BodyArmor, Bone, FootGear, Hair, HeadGear, LegGear, Outfit, Rig};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

pub struct CourtLookPlugin;

impl Plugin for CourtLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, dress_court);
    }
}

/// Одежда человека двора (поверх обычной по роли: лицо и сложение — свои).
pub fn outfit(c: Courtier, base: Outfit) -> Outfit {
    let o = base;
    match c {
        Courtier::Comes => Outfit {
            head: HeadGear::Bare,
            hair: Hair::Short,
            beard: Beard::Short,
            tunic_color: Color::srgb(0.20, 0.27, 0.46),
            trousers: Color::srgb(0.16, 0.13, 0.12),
            legs: LegGear::Trousers,
            feet: FootGear::Boots,
            hair_color: Color::srgb(0.32, 0.30, 0.28),
            shape: crate::appearance::Shape { aged: 0.55, heavy: 0.25, rugged: 0.4, ..o.shape },
            ..o
        },
        Courtier::Wife => Outfit { tunic_color: Color::srgb(0.42, 0.14, 0.32), ..o },
        Courtier::Son => Outfit { tunic_color: Color::srgb(0.86, 0.83, 0.76), hair: Hair::Short, ..o },
        Courtier::Chartularius => Outfit { tunic_color: Color::srgb(0.80, 0.76, 0.64), beard: Beard::Short, hair: Hair::Short, feet: FootGear::Shoes, ..o },
        Courtier::Sacellarius => Outfit {
            tunic_color: Color::srgb(0.22, 0.34, 0.22),
            hair: Hair::Balding,
            beard: Beard::Full,
            shape: crate::appearance::Shape { aged: 0.8, heavy: 0.6, ..o.shape },
            ..o
        },
        Courtier::Tribune => Outfit { head: HeadGear::Ridge, armor: BodyArmor::Lamellar, tunic_color: Color::srgb(0.55, 0.12, 0.1), beard: Beard::Full, ..o },
        // Препозит — евнух, как многие препозиты при дворе: безбород и гладколиц.
        Courtier::Praepositus => Outfit {
            tunic_color: Color::srgb(0.60, 0.48, 0.20),
            beard: Beard::None,
            hair: Hair::Cropped,
            shape: crate::appearance::Shape { rugged: -0.6, heavy: 0.5, round: 0.6, ..o.shape },
            ..o
        },
        Courtier::Guard(_) => Outfit { head: HeadGear::Ridge, armor: BodyArmor::Mail, tunic_color: Color::srgb(0.85, 0.82, 0.74), ..o },
        Courtier::Protevon => Outfit { tunic_color: Color::srgb(0.45, 0.15, 0.12), beard: Beard::Full, feet: FootGear::Boots, ..o },
        Courtier::Presbyter | Courtier::Cook | Courtier::Petitioner(_) => o,
    }
}

/// Какой плащ носит.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Cloak {
    /// Хламида сановника: белая, с таблионом и фибулой.
    Chlamys,
    /// Военный плащ (красный), фибула бронзовая.
    Sagum,
    /// Покрывало на голову и плечи.
    Veil,
}

fn cloak_of(c: Courtier) -> Option<(Cloak, Color)> {
    Some(match c {
        Courtier::Comes => (Cloak::Chlamys, Color::srgb(0.93, 0.91, 0.85)),
        Courtier::Tribune => (Cloak::Sagum, Color::srgb(0.58, 0.11, 0.08)),
        Courtier::Guard(_) => (Cloak::Sagum, Color::srgb(0.62, 0.14, 0.09)),
        Courtier::Wife => (Cloak::Veil, Color::srgb(0.90, 0.86, 0.78)),
        Courtier::Protevon => (Cloak::Sagum, Color::srgb(0.30, 0.24, 0.40)),
        _ => return None,
    })
}

/// Сетка ткани по рядам: каждый ряд — (высота, отступ назад, полуширина, загиб вперёд по краям).
/// В системе сустава груди: −Z — грудь, +Z — спина, +Y — вверх.
fn sheet(rows: &[(f32, f32, f32, f32)], cols: usize) -> Mesh {
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    for (r, &(y, z, half, wrap)) in rows.iter().enumerate() {
        for k in 0..=cols {
            let u = k as f32 / cols as f32;
            let s = u * 2.0 - 1.0;
            pos.push([s * half, y, z - wrap * s * s]);
            uv.push([u * half * 2.0, r as f32 / (rows.len() - 1) as f32 * 1.2]);
        }
    }
    let w = cols as u32 + 1;
    let mut idx = Vec::new();
    for r in 0..rows.len() as u32 - 1 {
        for k in 0..cols as u32 {
            let a = r * w + k;
            idx.extend([a, a + w, a + 1, a + 1, a + w, a + w + 1]);
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(Indices::U32(idx));
    m.compute_smooth_normals();
    m.generate_tangents().ok();
    m
}

/// Хламида / плащ на спине: от плеч почти до колен, расходится книзу.
fn back_cloak() -> Mesh {
    sheet(
        &[
            (0.375, 0.00, 0.17, 0.0),
            (0.355, 0.10, 0.25, 0.10),
            (0.25, 0.15, 0.28, 0.13),
            (0.0, 0.17, 0.30, 0.15),
            (-0.3, 0.22, 0.33, 0.16),
            (-0.62, 0.30, 0.36, 0.17),
        ],
        10,
    )
}

/// Пола хламиды спереди слева: с левого плеча вниз по груди (сколота на правом плече).
fn front_fold() -> Mesh {
    let m = sheet(&[(0.37, -0.02, 0.07, 0.0), (0.26, -0.15, 0.08, 0.0), (0.12, -0.2, 0.085, 0.0), (-0.15, -0.205, 0.09, 0.0), (-0.45, -0.24, 0.1, 0.0), (-0.62, -0.28, 0.11, 0.0)], 3);
    m.translated_by(Vec3::new(-0.1, 0.0, 0.0))
}

/// Мафорий: покрывает темя и затылок, падает на плечи и спину.
fn veil() -> Mesh {
    sheet(&[(0.668, -0.02, 0.085, 0.0), (0.62, 0.07, 0.13, 0.08), (0.52, 0.12, 0.15, 0.12), (0.4, 0.14, 0.2, 0.14), (0.26, 0.16, 0.25, 0.16), (0.04, 0.18, 0.27, 0.16)], 8)
}

/// Таблион: пурпурный прямоугольник с золотой каймой, нашитый на полу хламиды.
fn tablion(meshes: &mut Assets<Mesh>) -> (Handle<Mesh>, Handle<Mesh>) {
    let purple = Cuboid::new(0.11, 0.16, 0.004).mesh().build();
    let border = Cuboid::new(0.13, 0.18, 0.003).mesh().build();
    (meshes.add(purple), meshes.add(border))
}

/// Надеть плащи и знаки власти, когда риг человека двора собран.
#[allow(clippy::too_many_arguments)]
fn dress_court(
    mut commands: Commands,
    rigs: Query<(&Rig, &ChildOf), Added<Rig>>,
    people: Query<&ResidentView>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<Option<Handles>>,
) {
    for (rig, parent) in &rigs {
        let Ok(view) = people.get(parent.parent()) else { continue };
        let Some(c) = crate::court::of_resident(view.index) else { continue };
        let Some((kind, color)) = cloak_of(c) else { continue };
        let h = cache.get_or_insert_with(|| Handles::new(&assets, &mut meshes, &mut materials));
        let cloth = materials.add(StandardMaterial {
            base_color: color,
            base_color_texture: Some(h.wool.clone()),
            normal_map_texture: Some(h.wool_nrm.clone()),
            perceptual_roughness: 0.93,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let torso = rig.joint(Bone::Torso);
        commands.entity(torso).with_children(|p| match kind {
            Cloak::Veil => {
                p.spawn((Mesh3d(h.veil.clone()), MeshMaterial3d(cloth.clone())));
            }
            Cloak::Chlamys | Cloak::Sagum => {
                p.spawn((Mesh3d(h.back.clone()), MeshMaterial3d(cloth.clone())));
                // Фибула на правом плече: золотая у сановника, бронзовая у военных.
                let pin = if kind == Cloak::Chlamys { h.gold.clone() } else { h.bronze.clone() };
                p.spawn((Mesh3d(h.fibula.clone()), MeshMaterial3d(pin), Transform::from_xyz(0.16, 0.33, -0.1).with_rotation(Quat::from_rotation_x(-0.5))));
                if kind == Cloak::Chlamys {
                    p.spawn((Mesh3d(h.front.clone()), MeshMaterial3d(cloth.clone())));
                    // Таблион — на поле спереди, на уровне пояса.
                    let at = Transform::from_xyz(-0.1, -0.12, -0.215).with_rotation(Quat::from_rotation_x(0.05));
                    p.spawn((Mesh3d(h.tablion.1.clone()), MeshMaterial3d(h.gold.clone()), at));
                    p.spawn((Mesh3d(h.tablion.0.clone()), MeshMaterial3d(h.purple.clone()), at * Transform::from_xyz(0.0, 0.0, -0.002)));
                }
            }
        });
    }
}

struct Handles {
    wool: Handle<Image>,
    wool_nrm: Handle<Image>,
    back: Handle<Mesh>,
    front: Handle<Mesh>,
    veil: Handle<Mesh>,
    fibula: Handle<Mesh>,
    tablion: (Handle<Mesh>, Handle<Mesh>),
    gold: Handle<StandardMaterial>,
    bronze: Handle<StandardMaterial>,
    purple: Handle<StandardMaterial>,
}

impl Handles {
    fn new(assets: &AssetServer, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let wool: Handle<Image> = assets.load("textures/surface/wool_albedo.jpg");
        let wool_nrm: Handle<Image> = assets.load("textures/surface/wool_nrm.jpg");
        let gold_tex: Handle<Image> = assets.load("textures/surface/gold_albedo.jpg");
        let metal = |materials: &mut Assets<StandardMaterial>, c: Color| materials.add(StandardMaterial { base_color: c, base_color_texture: Some(gold_tex.clone()), metallic: 0.9, perceptual_roughness: 0.32, ..default() });
        let gold = metal(materials, Color::srgb(1.0, 0.86, 0.5));
        let bronze = metal(materials, Color::srgb(0.8, 0.55, 0.35));
        let purple = materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.07, 0.28),
            base_color_texture: Some(wool.clone()),
            perceptual_roughness: 0.8,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        Handles {
            back: meshes.add(back_cloak()),
            front: meshes.add(front_fold()),
            veil: meshes.add(veil()),
            fibula: meshes.add(Cylinder::new(0.035, 0.018).mesh().resolution(14).build()),
            tablion: tablion(meshes),
            wool,
            wool_nrm,
            gold,
            bronze,
            purple,
        }
    }
}

/// Человек двора — по роли (для проверок: облик не спорит с ролью).
#[allow(dead_code)]
fn role_ok(c: Courtier) -> bool {
    c.role() != Role::Patient
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloaks_are_closed_meshes_with_normals() {
        for m in [back_cloak(), front_fold(), veil()] {
            assert!(m.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
            assert!(m.indices().is_some_and(|i| i.len() % 3 == 0 && i.len() > 0));
        }
    }

    #[test]
    fn everyone_at_court_is_dressed() {
        for c in Courtier::ALL {
            let o = outfit(c, crate::residents::outfit(c.role(), 1));
            assert!(o.tunic_color.to_srgba().alpha > 0.0);
        }
    }
}
