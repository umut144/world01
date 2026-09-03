//! Server-authoritative correction of Characters that already occupy the same
//! space.
//!
//! Movement blocking prevents new overlap. This system handles overlap that
//! already exists because of spawning or an authoritative correction. It reads
//! every pair from one snapshot, accumulates the pair corrections in stable
//! `ActorId` order, and applies them simultaneously. A cluster may therefore
//! take more than one tick to settle, without letting pair iteration order give
//! one Character priority.

use bevy::prelude::{Entity, Query, Res, Vec2};
use world01_content::{CharacterCollisionGeometry, CharacterCollisionGeometryCatalog};
use world01_world_data::{ActorId, BodyFacing, CharacterMass, Position, SelectedCharacter};

use crate::spatial::broadphase::Aabb;
use crate::spatial::overlap::{
    GeometryTransform, character_separation, facing_transform, transformed_points,
};

/// Keeps the two rounded position writes safely on the non-penetrating side of
/// exact contact. At World 01's scale this is 0.192 px. The overlap predicate
/// may still report contact inside its established epsilon band, but its exact
/// separation distance is then zero and produces no further correction.
const SEPARATION_CLEARANCE_METERS: f32 = 0.001;

#[derive(Debug, Clone, Copy)]
struct ResolvedActor<'a> {
    entity: Entity,
    actor_id: u64,
    geometry: &'a CharacterCollisionGeometry,
    transform: GeometryTransform,
    bounds: Aabb,
    movement_mass: f32,
}

/// Separates overlapping Character pairs once per authoritative tick.
///
/// Each pair shares its complete correction inversely to movement mass. Speed,
/// velocity, RUN, DASH, and life state do not participate. Missing collision
/// geometry means the Character occupies no space. Missing or invalid mass
/// excludes the whole Character rather than applying a one-sided fallback.
pub fn separate_overlapping_characters(
    catalog: Res<CharacterCollisionGeometryCatalog>,
    mut actors: Query<(
        Entity,
        &ActorId,
        &SelectedCharacter,
        &BodyFacing,
        &CharacterMass,
        &mut Position,
    )>,
) {
    if catalog.is_empty() {
        return;
    }

    let mut resolved = Vec::new();
    for (entity, actor_id, character, facing, mass, position) in &mut actors {
        if !mass.movement.is_finite() || mass.movement <= 0.0 {
            continue;
        }
        let Some(geometry) = catalog.character(&character.0) else {
            continue;
        };
        let transform = facing_transform(geometry.authored_facing, *position, *facing);
        let Some(bounds) = Aabb::around(
            geometry
                .components
                .iter()
                .flat_map(|component| transformed_points(component.geometry(), transform)),
        ) else {
            continue;
        };
        resolved.push(ResolvedActor {
            entity,
            actor_id: actor_id.0,
            geometry,
            transform,
            bounds,
            movement_mass: mass.movement,
        });
    }

    // ActorId is assigned uniquely by the server. Sorting before enumerating
    // pairs also fixes the order of floating-point accumulation.
    resolved.sort_unstable_by_key(|actor| actor.actor_id);
    let mut offsets = vec![Vec2::ZERO; resolved.len()];
    for first_index in 0..resolved.len() {
        for second_index in (first_index + 1)..resolved.len() {
            let first = resolved[first_index];
            let second = resolved[second_index];
            if !first.bounds.overlaps(second.bounds) {
                continue;
            }
            let Some(separation) = character_separation(
                first.geometry,
                first.transform,
                second.geometry,
                second.transform,
            ) else {
                continue;
            };
            if separation.separation_distance <= 0.0 {
                continue;
            }
            let combined_mass = first.movement_mass + second.movement_mass;
            let total_distance = separation.separation_distance + SEPARATION_CLEARANCE_METERS;
            if !combined_mass.is_finite() || combined_mass <= 0.0 || !total_distance.is_finite() {
                continue;
            }

            let first_distance = total_distance * (second.movement_mass / combined_mass);
            let second_distance = total_distance - first_distance;
            offsets[first_index] += separation.normal * first_distance;
            offsets[second_index] -= separation.normal * second_distance;
        }
    }

    for (actor, offset) in resolved.into_iter().zip(offsets) {
        if offset == Vec2::ZERO || !offset.is_finite() {
            continue;
        }
        let Ok((_, _, _, _, _, mut position)) = actors.get_mut(actor.entity) else {
            continue;
        };
        let corrected = Vec2::new(position.x, position.y) + offset;
        if corrected.is_finite() {
            *position = Position::new(corrected.x, corrected.y);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use bevy::prelude::{App, Update};
    use world01_content::{
        AuthoredFacing, CollisionComponentGeometry, RuntimeComponentGeometry, RuntimeContent,
    };
    use world01_world_data::{CharacterLifeState, DashState, RunState};

    use crate::spatial::overlap::component_separation;

    const EPSILON: f32 = 0.000_01;

    fn rectangle_at(name: &str, offset: Vec2, size: Vec2) -> CollisionComponentGeometry {
        let half_size = size / 2.0;
        CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: name.to_owned(),
            name: name.to_owned(),
            vertices: vec![
                offset - half_size,
                offset + Vec2::new(half_size.x, -half_size.y),
                offset + half_size,
                offset + Vec2::new(-half_size.x, half_size.y),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        })
        .expect("the test rectangle has valid collision topology")
    }

    fn square_at(name: &str, offset: Vec2) -> CollisionComponentGeometry {
        rectangle_at(name, offset, Vec2::splat(0.4))
    }

    fn geometry(component_count: usize) -> CharacterCollisionGeometry {
        CharacterCollisionGeometry {
            authored_facing: AuthoredFacing::Right,
            components: (0..component_count)
                .map(|index| square_at(&format!("body_{index}"), Vec2::ZERO))
                .collect(),
        }
    }

    fn app_with_geometry(geometry: CharacterCollisionGeometry) -> App {
        app_with_catalog(CharacterCollisionGeometryCatalog::from_geometries([(
            world01_world_data::CharacterId("walker".into()),
            geometry,
        )]))
    }

    fn app_with_catalog(catalog: CharacterCollisionGeometryCatalog) -> App {
        let mut app = App::new();
        app.insert_resource(catalog)
            .add_systems(Update, separate_overlapping_characters);
        app
    }

    fn app(component_count: usize) -> App {
        app_with_geometry(geometry(component_count))
    }

    fn mass(movement: f32, normal_speed: f32) -> CharacterMass {
        CharacterMass::new(movement, 0.0, movement, normal_speed)
    }

    fn spawn(app: &mut App, actor_id: u64, position: Vec2, mass: CharacterMass) -> Entity {
        spawn_as(app, actor_id, "walker", position, mass)
    }

    fn spawn_as(
        app: &mut App,
        actor_id: u64,
        character: &str,
        position: Vec2,
        mass: CharacterMass,
    ) -> Entity {
        app.world_mut()
            .spawn((
                ActorId(actor_id),
                SelectedCharacter(world01_world_data::CharacterId(character.into())),
                BodyFacing::Right,
                Position::new(position.x, position.y),
                mass,
            ))
            .id()
    }

    fn position(app: &App, entity: Entity) -> Vec2 {
        let position = app
            .world()
            .get::<Position>(entity)
            .expect("the actor keeps its Position");
        Vec2::new(position.x, position.y)
    }

    fn assert_vec2(actual: Vec2, expected: Vec2) {
        assert!(
            actual.distance(expected) <= EPSILON,
            "{actual:?} != {expected:?}"
        );
    }

    fn remaining_component_penetration(
        app: &App,
        character: &str,
        first: Entity,
        second: Entity,
    ) -> f32 {
        let geometry = app
            .world()
            .resource::<CharacterCollisionGeometryCatalog>()
            .character(&world01_world_data::CharacterId(character.into()))
            .expect("the measured Character has collision geometry");
        let first_transform = facing_transform(
            geometry.authored_facing,
            *app.world()
                .get::<Position>(first)
                .expect("the first actor keeps its Position"),
            BodyFacing::Right,
        );
        let second_transform = facing_transform(
            geometry.authored_facing,
            *app.world()
                .get::<Position>(second)
                .expect("the second actor keeps its Position"),
            BodyFacing::Right,
        );

        geometry
            .components
            .iter()
            .flat_map(|component| {
                geometry.components.iter().filter_map(move |other| {
                    component_separation(component, first_transform, other, second_transform)
                        .map(|separation| separation.separation_distance)
                })
            })
            .sum()
    }

    fn assert_converges_within(
        app: &mut App,
        character: &str,
        first: Entity,
        second: Entity,
        tick_budget: usize,
    ) -> usize {
        let mut remaining = remaining_component_penetration(app, character, first, second);
        assert!(remaining > 0.0, "the regression starts overlapped");

        for tick in 0..tick_budget {
            app.update();
            remaining = remaining_component_penetration(app, character, first, second);
            assert!(remaining.is_finite(), "remaining penetration is finite");
            if remaining <= EPSILON {
                return tick + 1;
            }
        }

        panic!("the pair still has {remaining} m of summed penetration after {tick_budget} ticks");
    }

    #[test]
    fn equal_masses_share_one_complete_pair_correction() {
        let mut app = app(1);
        let first = spawn(&mut app, 1, Vec2::ZERO, mass(1.0, 0.6));
        let second = spawn(&mut app, 2, Vec2::new(0.3, 0.0), mass(1.0, 0.6));

        app.update();

        let half = (0.1 + SEPARATION_CLEARANCE_METERS) / 2.0;
        assert_vec2(position(&app, first), Vec2::new(-half, 0.0));
        assert_vec2(position(&app, second), Vec2::new(0.3 + half, 0.0));
        let geometry = app
            .world()
            .resource::<CharacterCollisionGeometryCatalog>()
            .character(&world01_world_data::CharacterId("walker".into()))
            .expect("the test Character has geometry");
        let remaining = character_separation(
            geometry,
            translated(position(&app, first)),
            geometry,
            translated(position(&app, second)),
        );
        assert!(remaining.is_none_or(|separation| separation.separation_distance == 0.0));
    }

    #[test]
    fn inverse_mass_moves_the_heavier_character_less() {
        let mut app = app(1);
        let heavy = spawn(&mut app, 1, Vec2::ZERO, mass(3.0, 0.6));
        let light = spawn(&mut app, 2, Vec2::new(0.3, 0.0), mass(1.0, 0.6));

        app.update();

        let total = 0.1 + SEPARATION_CLEARANCE_METERS;
        let heavy_distance = position(&app, heavy).length();
        let light_distance = (position(&app, light) - Vec2::new(0.3, 0.0)).length();
        assert!((heavy_distance + light_distance - total).abs() <= EPSILON);
        assert!((light_distance / heavy_distance - 3.0).abs() <= EPSILON);
    }

    #[test]
    fn exact_contact_does_not_move_either_character() {
        let mut app = app(1);
        let first = spawn(&mut app, 1, Vec2::ZERO, mass(1.0, 0.6));
        let second = spawn(&mut app, 2, Vec2::new(0.4, 0.0), mass(1.0, 0.6));

        app.update();

        assert_eq!(position(&app, first), Vec2::ZERO);
        assert_eq!(position(&app, second), Vec2::new(0.4, 0.0));
    }

    #[test]
    fn clearance_prevents_a_second_rounding_correction_at_world_scale() {
        let mut app = app(1);
        let first = spawn(&mut app, 1, Vec2::new(100.0, 100.0), mass(1.0, 0.6));
        let second = spawn(&mut app, 2, Vec2::new(100.3, 100.0), mass(1.0, 0.6));

        app.update();
        let corrected = (position(&app, first), position(&app, second));
        app.update();

        assert_eq!((position(&app, first), position(&app, second)), corrected);
    }

    #[test]
    fn speed_run_and_dash_do_not_change_separation() {
        fn corrected_positions(first_speed: f32, second_speed: f32) -> (Vec2, Vec2) {
            let mut app = app(1);
            let first = spawn(&mut app, 1, Vec2::ZERO, mass(1.0, first_speed));
            let second = spawn(&mut app, 2, Vec2::new(0.3, 0.0), mass(1.0, second_speed));
            app.world_mut().entity_mut(first).insert((
                RunState {
                    toggled: true,
                    active: true,
                    input_pressed: true,
                },
                DashState {
                    active: true,
                    ..DashState::default()
                },
            ));
            app.update();
            (position(&app, first), position(&app, second))
        }

        assert_eq!(
            corrected_positions(0.1, 100.0),
            corrected_positions(50.0, 0.2)
        );
    }

    #[test]
    fn dead_characters_still_occupy_space() {
        let mut app = app(1);
        let dead = spawn(&mut app, 1, Vec2::ZERO, mass(1.0, 0.6));
        let living = spawn(&mut app, 2, Vec2::new(0.3, 0.0), mass(1.0, 0.6));
        app.world_mut()
            .entity_mut(dead)
            .insert(CharacterLifeState::Dead);

        app.update();

        assert!(position(&app, dead).x < 0.0);
        assert!(position(&app, living).x > 0.3);
    }

    #[test]
    fn coincident_characters_use_actor_id_for_one_opposed_pair() {
        let mut app = app(1);
        let higher = spawn(&mut app, 20, Vec2::ZERO, mass(1.0, 0.6));
        let lower = spawn(&mut app, 10, Vec2::ZERO, mass(1.0, 0.6));

        app.update();

        let half = (0.4 + SEPARATION_CLEARANCE_METERS) / 2.0;
        assert_vec2(position(&app, lower), Vec2::new(0.0, half));
        assert_vec2(position(&app, higher), Vec2::new(0.0, -half));
    }

    fn three_actor_positions(spawn_order: [u64; 3], ticks: usize) -> BTreeMap<u64, Vec2> {
        let mut app = app(1);
        let starts = [(1, 0.0), (2, 0.2), (3, 0.4)];
        let mut entities = BTreeMap::new();
        for actor_id in spawn_order {
            let x = starts
                .iter()
                .find_map(|(id, x)| (*id == actor_id).then_some(*x))
                .expect("the spawn order names every actor");
            entities.insert(
                actor_id,
                spawn(&mut app, actor_id, Vec2::new(x, 0.0), mass(1.0, 0.6)),
            );
        }
        for _ in 0..ticks {
            app.update();
        }
        entities
            .into_iter()
            .map(|(actor_id, entity)| (actor_id, position(&app, entity)))
            .collect()
    }

    #[test]
    fn three_actor_result_is_independent_from_spawn_order() {
        assert_eq!(
            three_actor_positions([1, 2, 3], 1),
            three_actor_positions([3, 2, 1], 1)
        );
    }

    #[test]
    fn a_three_actor_cluster_converges_across_ticks() {
        let after_one = three_actor_positions([1, 2, 3], 1);
        let after_two = three_actor_positions([1, 2, 3], 2);
        let first_gap = after_one[&2].x - after_one[&1].x;
        let second_gap = after_two[&2].x - after_two[&1].x;

        assert!(second_gap > first_gap);
        assert!(second_gap < 0.4 + SEPARATION_CLEARANCE_METERS);
        assert_vec2(after_one[&2], after_two[&2]);
    }

    #[test]
    fn multiple_distinct_components_still_apply_only_the_deepest_pair_correction() {
        let geometry = CharacterCollisionGeometry {
            authored_facing: AuthoredFacing::Right,
            components: vec![
                square_at("body_left", Vec2::new(-0.1, 0.0)),
                square_at("body_right", Vec2::new(0.1, 0.0)),
            ],
        };
        let mut app = app_with_geometry(geometry);
        let first = spawn(&mut app, 1, Vec2::ZERO, mass(1.0, 0.6));
        let second = spawn(&mut app, 2, Vec2::new(0.3, 0.0), mass(1.0, 0.6));

        app.update();

        let half = (0.3 + SEPARATION_CLEARANCE_METERS) / 2.0;
        assert_vec2(position(&app, first), Vec2::new(-half, 0.0));
        assert_vec2(position(&app, second), Vec2::new(0.3 + half, 0.0));
    }

    fn stacked_character_positions(spawn_order: [u64; 2]) -> BTreeMap<u64, Vec2> {
        let geometry = CharacterCollisionGeometry {
            authored_facing: AuthoredFacing::Right,
            components: vec![
                rectangle_at("feet", Vec2::new(0.0, 0.1), Vec2::new(1.0, 0.2)),
                rectangle_at("body", Vec2::new(0.0, 0.7), Vec2::new(1.0, 1.0)),
            ],
        };
        let mut app = app_with_geometry(geometry);
        let starts = [(1, Vec2::ZERO), (2, Vec2::ZERO)];
        let mut entities = BTreeMap::new();
        for actor_id in spawn_order {
            let start = starts
                .iter()
                .find_map(|(id, position)| (*id == actor_id).then_some(*position))
                .expect("the spawn order names both actors");
            entities.insert(actor_id, spawn(&mut app, actor_id, start, mass(1.0, 0.6)));
        }

        let ticks = assert_converges_within(&mut app, "walker", entities[&1], entities[&2], 16);
        assert!(
            ticks > 1,
            "the stacked Regions exercise multi-tick settling"
        );
        entities
            .into_iter()
            .map(|(actor_id, entity)| (actor_id, position(&app, entity)))
            .collect()
    }

    #[test]
    fn stacked_character_regions_converge_independently_from_spawn_order() {
        assert_eq!(
            stacked_character_positions([1, 2]),
            stacked_character_positions([2, 1])
        );
    }

    #[test]
    fn embedded_hammerers_converge_with_real_collision_geometry() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let catalog = CharacterCollisionGeometryCatalog::from_content(&content)
            .expect("embedded Character collision geometry is valid");
        let mut app = app_with_catalog(catalog);
        let first = spawn_as(&mut app, 1, "hammerer", Vec2::ZERO, mass(1.0, 0.6));
        let second = spawn_as(&mut app, 2, "hammerer", Vec2::new(0.0, 0.5), mass(1.0, 0.6));

        // Resolving the deepest body pair first temporarily deepens another
        // Component pairing, so the contract is bounded eventual clearance,
        // not a monotonic decrease after every individual tick.
        let ticks = assert_converges_within(&mut app, "hammerer", first, second, 16);
        assert!(ticks > 1, "the real Regions exercise multi-tick settling");
    }

    #[test]
    fn missing_mass_excludes_the_whole_pair() {
        let mut app = app(1);
        let missing = app
            .world_mut()
            .spawn((
                ActorId(1),
                SelectedCharacter(world01_world_data::CharacterId("walker".into())),
                BodyFacing::Right,
                Position::ZERO,
            ))
            .id();
        let valid = spawn(&mut app, 2, Vec2::new(0.3, 0.0), mass(1.0, 0.6));

        app.update();

        assert_eq!(position(&app, missing), Vec2::ZERO);
        assert_eq!(position(&app, valid), Vec2::new(0.3, 0.0));
    }

    #[test]
    fn invalid_mass_excludes_the_whole_pair_without_corrupting_positions() {
        for invalid_mass in [0.0, -1.0, f32::NAN] {
            let mut app = app(1);
            let invalid = spawn(&mut app, 1, Vec2::ZERO, mass(invalid_mass, 0.6));
            let valid = spawn(&mut app, 2, Vec2::new(0.3, 0.0), mass(1.0, 0.6));

            app.update();

            assert_eq!(position(&app, invalid), Vec2::ZERO);
            assert_eq!(position(&app, valid), Vec2::new(0.3, 0.0));
        }
    }

    fn translated(position: Vec2) -> GeometryTransform {
        GeometryTransform {
            origin: position,
            ..GeometryTransform::IDENTITY
        }
    }
}
