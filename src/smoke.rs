//! Дым над городом: из очагов во дворах и на кухнях (сквозь черепицу — у конька),
//! из кузнечных и сыродутных горнов. Не каждый очаг горит: к вечеру готовят ужин
//! примерно в каждом седьмом доме. Только клиент, без коллизий.

use crate::graphics::{Billboard, mood};
use crate::sky::SkyUniform;
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, PrimitiveTopology, ShaderType};
use bevy::shader::ShaderRef;

#[derive(ShaderType, Clone, Copy, Debug, Default)]
pub struct SmokeUniform {
    pub sky: SkyUniform,
    pub tint: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SmokeMaterial {
    #[uniform(0)]
    pub smoke: SmokeUniform,
}

impl Material for SmokeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://medieval_rpg/shaders/smoke.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

/// Откуда идёт дым.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeSource {
    /// Основание столба (над крышей, если очаг под ней).
    pub base: Vec3,
    /// Высота столба, м.
    pub height: f32,
    /// Горн — гуще и темнее.
    pub forge: bool,
}

/// Столб дыма: прямоугольник шириной `w`, высотой `h`, основание в начале координат.
/// В `uv.x` зашит номер столба (×4), чтобы у каждого был свой рисунок клубов.
pub fn column_mesh(w: f32, h: f32, seed: u32) -> Mesh {
    let s = (seed % 997) as f32 * 4.0;
    let pos = vec![[-w / 2.0, 0.0, 0.0], [w / 2.0, 0.0, 0.0], [w / 2.0, h, 0.0], [-w / 2.0, h, 0.0]];
    let nrm = vec![[0.0, 0.0, 1.0]; 4];
    let uv = vec![[s, 0.0], [s + 1.0, 0.0], [s + 1.0, 1.0], [s, 1.0]];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// Высота крыши над точкой: самая высокая черепица (треугольники `roofs`) над `p`.
pub fn roof_above(p: Vec3, roofs: &[[Vec3; 3]]) -> Option<f32> {
    let mut best: Option<f32> = None;
    for [a, b, c] in roofs {
        let (min_x, max_x) = (a.x.min(b.x).min(c.x), a.x.max(b.x).max(c.x));
        let (min_z, max_z) = (a.z.min(b.z).min(c.z), a.z.max(b.z).max(c.z));
        if p.x < min_x || p.x > max_x || p.z < min_z || p.z > max_z {
            continue;
        }
        // Барицентрические координаты в плоскости XZ.
        let v0 = Vec2::new(b.x - a.x, b.z - a.z);
        let v1 = Vec2::new(c.x - a.x, c.z - a.z);
        let v2 = Vec2::new(p.x - a.x, p.z - a.z);
        let den = v0.x * v1.y - v1.x * v0.y;
        if den.abs() < 1e-6 {
            continue;
        }
        let s = (v2.x * v1.y - v1.x * v2.y) / den;
        let t = (v0.x * v2.y - v2.x * v0.y) / den;
        if s < -1e-4 || t < -1e-4 || s + t > 1.0 + 1e-4 {
            continue;
        }
        let y = a.y + s * (b.y - a.y) + t * (c.y - a.y);
        if y > p.y + 1.0 && best.is_none_or(|bst| y > bst) {
            best = Some(y);
        }
    }
    best
}

/// Где дымит: часть очагов и все горны.
pub fn smoke_sources(hearths: &[Vec3], forges: &[Vec3], roofs: &[[Vec3; 3]]) -> Vec<SmokeSource> {
    let mut out = Vec::new();
    let place = |p: Vec3, forge: bool, out: &mut Vec<SmokeSource>| {
        let (base, height) = match roof_above(p, roofs) {
            // Под крышей дым выходит сквозь черепицу и продухи у конька.
            Some(y) => (Vec3::new(p.x, y - 0.2, p.z), if forge { 20.0 } else { 14.0 }),
            None => (p + Vec3::Y * 0.3, if forge { 18.0 } else { 12.0 }),
        };
        // Два очага в одном дворе дают один столб.
        if out.iter().any(|s: &SmokeSource| s.base.xz().distance(base.xz()) < 8.0) {
            return;
        }
        out.push(SmokeSource { base, height, forge });
    };
    for f in forges {
        place(*f, true, &mut out);
    }
    for (i, h) in hearths.iter().enumerate() {
        // Вечером топят не все: кухонные очаги, где готовят ужин (примерно каждый седьмой),
        // и костры в лагерях за стенами. Детерминированно, чтобы у всех игроков одинаково.
        let k = ((h.x * 12.9898 + h.z * 78.233).sin() * 43758.547).fract().abs();
        if k > 0.13 && i % 17 != 0 {
            continue;
        }
        place(*h, false, &mut out);
    }
    out
}

pub fn spawn_smoke(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<SmokeMaterial>, sources: &[SmokeSource]) {
    let sky = mood().sky_uniform();
    let wood = mats.add(SmokeMaterial { smoke: SmokeUniform { sky, tint: Vec4::new(0.78, 0.76, 0.76, 0.42) } });
    let forge = mats.add(SmokeMaterial { smoke: SmokeUniform { sky, tint: Vec4::new(0.42, 0.40, 0.40, 0.8) } });
    for (i, s) in sources.iter().enumerate() {
        let w = s.height * 0.8;
        commands.spawn((
            Mesh3d(meshes.add(column_mesh(w, s.height, i as u32 + 1))),
            MeshMaterial3d(if s.forge { forge.clone() } else { wood.clone() }),
            Transform::from_translation(s.base),
            Billboard,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    info!("Дым: {} столбов", sources.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_leaves_through_the_roof() {
        // Скат над очагом: от y=4 до y=6.
        let roof = [[Vec3::new(-5.0, 4.0, -5.0), Vec3::new(5.0, 4.0, -5.0), Vec3::new(0.0, 6.0, 5.0)]];
        let y = roof_above(Vec3::ZERO, &roof).unwrap();
        assert!((y - 5.0).abs() < 0.01, "{y}");
        assert!(roof_above(Vec3::new(20.0, 0.0, 0.0), &roof).is_none());
        let s = smoke_sources(&[], &[Vec3::ZERO], &roof);
        assert_eq!(s.len(), 1);
        assert!(s[0].base.y > 4.5 && s[0].forge);
    }

    #[test]
    fn neighbouring_hearths_share_a_column() {
        let s = smoke_sources(&[], &[Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0)], &[]);
        assert_eq!(s.len(), 2);
    }
}
