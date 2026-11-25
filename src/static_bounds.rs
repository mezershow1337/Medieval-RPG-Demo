//! Рамки (Aabb) неподвижных мешей для отсечения по камере.
//!
//! Bevy пересчитывает рамку по всем вершинам меша, как только у сущности «изменился» `Mesh3d`.
//! А `Mesh3d` помечается изменённым и при любой правке её *материала*: листва (12 тыс. мешей,
//! 31 млн вершин) пересчитывала рамки каждый кадр, пока ветер правил её материалы, а весь
//! город — каждый раз, когда намокают или сохнут камни. Сами меши в игре не меняются.
//!
//! Поэтому, как только у меша есть рамка, отключаем Bevy-пересчёт (`NoAutoAabb`) и
//! запоминаем, для какого меша она посчитана. Пересчитываем сами — только если у сущности
//! правда другой меш или сам меш правили. Скиннованных людей не трогаем: их рамки по костям
//! ведёт Bevy.

use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::camera::visibility::{NoAutoAabb, NoFrustumCulling, VisibilitySystems};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

pub struct StaticBoundsPlugin;

impl Plugin for StaticBoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, (freeze_bounds, refresh_frozen_bounds).chain().after(VisibilitySystems::CalculateBounds).before(VisibilitySystems::CheckVisibility));
    }
}

/// Для какого меша посчитана рамка сущности.
#[derive(Component)]
struct BoundsFor(AssetId<Mesh>);

/// Рамка уже есть — дальше её ведём мы (Bevy больше не перебирает такие сущности).
#[allow(clippy::type_complexity)]
fn freeze_bounds(mut commands: Commands, fresh: Query<(Entity, &Mesh3d), (With<Aabb>, Without<NoAutoAabb>, Without<NoFrustumCulling>, Without<SkinnedMesh>)>) {
    for (e, mesh) in &fresh {
        commands.entity(e).try_insert((NoAutoAabb, BoundsFor(mesh.id())));
    }
}

/// «Изменился» `Mesh3d` — чаще всего это правка материала: тот же меш, рамка та же.
/// Пересчитываем, только если меш другой или его самого правили.
fn refresh_frozen_bounds(meshes: Res<Assets<Mesh>>, mut events: MessageReader<AssetEvent<Mesh>>, mut q: Query<(&Mesh3d, &mut Aabb, &mut BoundsFor), Changed<Mesh3d>>) {
    let edited: HashSet<AssetId<Mesh>> = events.read().filter_map(|e| if let AssetEvent::Modified { id } = e { Some(*id) } else { None }).collect();
    for (mesh, mut aabb, mut of) in &mut q {
        if of.0 == mesh.id() && !edited.contains(&of.0) {
            continue;
        }
        if let Some(b) = meshes.get(mesh).and_then(MeshAabb::get_aabb) {
            *aabb = b;
            of.0 = mesh.id();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    fn cube(size: f32) -> Mesh {
        Mesh::from(Cuboid::from_length(size))
    }

    #[test]
    fn material_edits_keep_the_box_and_real_mesh_changes_update_it() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Messages<AssetEvent<Mesh>>>();
        let (small, big) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (meshes.add(cube(1.0)), meshes.add(cube(4.0)))
        };
        let small_box = cube(1.0).get_aabb().unwrap();
        let e = world.spawn((Mesh3d(small.clone()), small_box)).id();
        world.run_system_once(freeze_bounds).unwrap();
        assert!(world.get::<NoAutoAabb>(e).is_some());
        world.run_system_once(refresh_frozen_bounds).unwrap();

        // «Изменился» тот же меш (так Bevy отмечает правку материала) — рамку не пересчитываем.
        world.get_mut::<Mesh3d>(e).unwrap().set_changed();
        *world.get_mut::<Aabb>(e).unwrap() = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE * 9.0);
        world.run_system_once(refresh_frozen_bounds).unwrap();
        assert_eq!(world.get::<Aabb>(e).unwrap().half_extents, Vec3A::splat(4.5));

        // Сущности дали другой меш — рамка по новому мешу.
        world.get_mut::<Mesh3d>(e).unwrap().0 = big;
        world.run_system_once(refresh_frozen_bounds).unwrap();
        assert_eq!(world.get::<Aabb>(e).unwrap().half_extents, Vec3A::splat(2.0));

        // Сам меш правили — тоже пересчёт.
        world.get_mut::<Mesh3d>(e).unwrap().0 = small.clone();
        world.run_system_once(refresh_frozen_bounds).unwrap();
        assert_eq!(world.get::<Aabb>(e).unwrap().half_extents, Vec3A::splat(0.5));
        *world.resource_mut::<Assets<Mesh>>().get_mut(&small).unwrap() = cube(2.0);
        world.resource_mut::<Messages<AssetEvent<Mesh>>>().write(AssetEvent::Modified { id: small.id() });
        world.get_mut::<Mesh3d>(e).unwrap().set_changed();
        world.run_system_once(refresh_frozen_bounds).unwrap();
        assert_eq!(world.get::<Aabb>(e).unwrap().half_extents, Vec3A::splat(1.0));
    }

    #[test]
    fn skinned_and_unculled_meshes_stay_with_bevy() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let h = world.resource_mut::<Assets<Mesh>>().add(cube(1.0));
        let b = cube(1.0).get_aabb().unwrap();
        let unculled = world.spawn((Mesh3d(h.clone()), b, NoFrustumCulling)).id();
        let skinned = world.spawn((Mesh3d(h), b, SkinnedMesh::default())).id();
        world.run_system_once(freeze_bounds).unwrap();
        assert!(world.get::<NoAutoAabb>(unculled).is_none());
        assert!(world.get::<NoAutoAabb>(skinned).is_none());
    }
}
