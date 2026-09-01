//! Where an actor is allowed to end this tick.
//!
//! Collision runs between deciding a velocity and applying it: it may refuse a
//! step, never move an actor. That keeps `integrate_movement` the only writer
//! of `Position` and keeps the refusal itself readable as a zeroed velocity.
//!
//! What blocks is authored, not derived. An actor whose character has no
//! `collision` Region is a Ghost - it passes through everything and stops
//! nobody - which is a content decision rather than a missing feature.

use bevy::prelude::{Entity, Query, Res, Vec2};
use world01_content::{
    CharacterCollisionGeometry, CharacterCollisionGeometryCatalog, WorldCollisionGeometryCatalog,
};
use world01_world_data::{BodyFacing, MovementVelocity, Position, SelectedCharacter};

use crate::movement::MovementStep;
use crate::spatial::broadphase::{Aabb, WorldColliderGrid};
use crate::spatial::overlap::{
    GeometryTransform, components_overlap, facing_transform, transformed_points,
};

/// An actor's collision geometry, placed where it stands.
#[derive(Debug, Clone, Copy)]
struct PosedCollider<'a> {
    entity: Entity,
    geometry: &'a CharacterCollisionGeometry,
    transform: GeometryTransform,
    bounds: Aabb,
}

impl PosedCollider<'_> {
    fn translated(self, offset: Vec2) -> Self {
        Self {
            transform: GeometryTransform {
                origin: self.transform.origin + offset,
                ..self.transform
            },
            bounds: Aabb {
                minimum: self.bounds.minimum + offset,
                maximum: self.bounds.maximum + offset,
            },
            ..self
        }
    }
}

/// Refuses movement that would end inside world geometry or another actor.
///
/// Blocking is all or nothing for now: the actor keeps its position instead of
/// sliding along the surface it hit. Sliding is a separate decision and belongs
/// in a later change, because it needs a contact normal this phase does not
/// compute yet.
///
/// Every test is against the other actors' *current* positions, so two actors
/// walking into each other are judged identically no matter which one the query
/// visits first.
///
/// An actor that already overlaps something may still move. Without that escape
/// valve a character spawned inside a prop, or pushed there by a correction from
/// the server, would never get out again.
pub fn block_colliding_movement(
    step: Res<MovementStep>,
    catalog: Res<CharacterCollisionGeometryCatalog>,
    world: Res<WorldCollisionGeometryCatalog>,
    grid: Res<WorldColliderGrid>,
    standing: Query<(Entity, &SelectedCharacter, &Position, &BodyFacing)>,
    mut movers: Query<(
        Entity,
        &SelectedCharacter,
        &Position,
        &BodyFacing,
        &mut MovementVelocity,
    )>,
) {
    if catalog.is_empty() {
        return;
    }
    let blockers = standing
        .iter()
        .filter_map(|(entity, character, position, facing)| {
            let geometry = catalog.character(&character.0)?;
            posed(entity, geometry, *position, *facing)
        })
        .collect::<Vec<_>>();

    let mut candidates = Vec::new();
    for (entity, character, position, facing, mut velocity) in &mut movers {
        let Some(geometry) = catalog.character(&character.0) else {
            continue;
        };
        let displacement = step.step(*velocity);
        if displacement == Vec2::ZERO || !displacement.is_finite() {
            continue;
        }
        let Some(current) = posed(entity, geometry, *position, *facing) else {
            continue;
        };
        let proposed = current.translated(displacement);
        if blocked(proposed, &blockers, &world, &grid, &mut candidates)
            && !blocked(current, &blockers, &world, &grid, &mut candidates)
        {
            *velocity = MovementVelocity::ZERO;
        }
    }
}

/// Returns `None` for geometry the broad phase cannot bound, which drops the
/// actor out of collision entirely rather than letting a non-finite vertex
/// decide who may move.
fn posed<'a>(
    entity: Entity,
    geometry: &'a CharacterCollisionGeometry,
    position: Position,
    facing: BodyFacing,
) -> Option<PosedCollider<'a>> {
    let transform = facing_transform(geometry.authored_facing, position, facing);
    Some(PosedCollider {
        entity,
        geometry,
        transform,
        bounds: Aabb::around(transformed_points(&geometry.components, transform))?,
    })
}

fn blocked(
    actor: PosedCollider<'_>,
    blockers: &[PosedCollider<'_>],
    world: &WorldCollisionGeometryCatalog,
    grid: &WorldColliderGrid,
    candidates: &mut Vec<u32>,
) -> bool {
    grid.candidates(actor.bounds, candidates);
    for index in candidates.iter() {
        let region = &world.regions[*index as usize];
        let placement = GeometryTransform::translated(region.position);
        for component in &actor.geometry.components {
            if components_overlap(component, actor.transform, &region.component, placement) {
                return true;
            }
        }
    }
    blockers
        .iter()
        .filter(|blocker| blocker.entity != actor.entity && blocker.bounds.overlaps(actor.bounds))
        .any(|blocker| overlaps(actor, *blocker))
}

fn overlaps(actor: PosedCollider<'_>, blocker: PosedCollider<'_>) -> bool {
    actor.geometry.components.iter().any(|component| {
        blocker.geometry.components.iter().any(|other| {
            components_overlap(component, actor.transform, other, blocker.transform)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, IntoScheduleConfigs, Update};
    use world01_configs::load_embedded;
    use world01_content::{AuthoredFacing, PlacedCollisionGeometry, RuntimeComponentGeometry};
    use world01_world_data::CharacterId;

    use crate::movement::integrate_movement;

    /// One tick at this velocity covers exactly one meter, so a test can name
    /// the position a step would reach instead of counting ticks.
    const ONE_METER_PER_TICK: f32 = 60.0;

    fn square(name: &str, corner: Vec2, size: f32) -> RuntimeComponentGeometry {
        RuntimeComponentGeometry {
            component_id: name.to_owned(),
            name: name.to_owned(),
            vertices: vec![
                corner,
                corner + Vec2::new(size, 0.0),
                corner + Vec2::splat(size),
                corner + Vec2::new(0.0, size),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    /// A character whose collider is a 0.4 m box centred on its position.
    fn walker() -> CharacterCollisionGeometry {
        CharacterCollisionGeometry {
            authored_facing: AuthoredFacing::Right,
            components: vec![square("body", Vec2::splat(-0.2), 0.4)],
        }
    }

    fn catalog() -> CharacterCollisionGeometryCatalog {
        CharacterCollisionGeometryCatalog::from_geometries([(
            CharacterId("walker".into()),
            walker(),
        )])
    }

    /// A 1 m block covering x in [1, 2] and y in [-0.5, 0.5].
    fn wall() -> WorldCollisionGeometryCatalog {
        WorldCollisionGeometryCatalog {
            regions: vec![PlacedCollisionGeometry {
                instance_id: "wall".to_owned(),
                position: Position::new(1.0, -0.5),
                component: square("block", Vec2::ZERO, 1.0),
            }],
        }
    }

    fn app(world: WorldCollisionGeometryCatalog) -> App {
        let config = load_embedded().expect("embedded runtime configuration parses");
        let mut app = App::new();
        app.insert_resource(MovementStep::from_runtime(&config).expect("runtime is valid"))
            .insert_resource(catalog())
            .insert_resource(WorldColliderGrid::from_catalog(&world))
            .insert_resource(world)
            .add_systems(Update, (block_colliding_movement, integrate_movement).chain());
        app
    }

    fn spawn(app: &mut App, character: &str, position: Vec2, velocity: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                SelectedCharacter(CharacterId(character.to_owned())),
                Position::new(position.x, position.y),
                BodyFacing::Right,
                MovementVelocity::new(velocity.x, velocity.y),
            ))
            .id()
    }

    fn position_of(app: &App, actor: Entity) -> Position {
        *app.world()
            .get::<Position>(actor)
            .expect("the actor keeps its position")
    }

    fn velocity_of(app: &App, actor: Entity) -> MovementVelocity {
        *app.world()
            .get::<MovementVelocity>(actor)
            .expect("the actor keeps its velocity")
    }

    #[test]
    fn a_step_into_world_geometry_is_refused_and_the_actor_stays_put() {
        let mut app = app(wall());
        let actor = spawn(
            &mut app,
            "walker",
            Vec2::ZERO,
            Vec2::new(ONE_METER_PER_TICK, 0.0),
        );

        app.update();

        assert_eq!(velocity_of(&app, actor), MovementVelocity::ZERO);
        assert_eq!(position_of(&app, actor), Position::ZERO);
    }

    #[test]
    fn a_step_that_clears_the_geometry_keeps_its_velocity() {
        let mut app = app(wall());
        let actor = spawn(
            &mut app,
            "walker",
            Vec2::ZERO,
            Vec2::new(0.0, ONE_METER_PER_TICK),
        );

        app.update();

        assert_eq!(
            velocity_of(&app, actor),
            MovementVelocity::new(0.0, ONE_METER_PER_TICK)
        );
        assert_eq!(position_of(&app, actor), Position::new(0.0, 1.0));
    }

    #[test]
    fn characters_block_each_other() {
        let mut app = app(WorldCollisionGeometryCatalog::default());
        let mover = spawn(
            &mut app,
            "walker",
            Vec2::ZERO,
            Vec2::new(ONE_METER_PER_TICK, 0.0),
        );
        spawn(&mut app, "walker", Vec2::new(1.0, 0.0), Vec2::ZERO);

        app.update();

        assert_eq!(velocity_of(&app, mover), MovementVelocity::ZERO);
        assert_eq!(position_of(&app, mover), Position::ZERO);
    }

    #[test]
    fn a_character_without_collision_geometry_walks_through_everything() {
        let mut app = app(wall());
        let ghost = spawn(
            &mut app,
            "ghost",
            Vec2::ZERO,
            Vec2::new(ONE_METER_PER_TICK, 0.0),
        );
        spawn(&mut app, "walker", Vec2::new(1.0, 0.0), Vec2::ZERO);

        app.update();

        assert_eq!(position_of(&app, ghost), Position::new(1.0, 0.0));
    }

    #[test]
    fn an_actor_that_already_overlaps_may_still_move() {
        let mut app = app(wall());
        let stuck = spawn(
            &mut app,
            "walker",
            Vec2::new(1.0, 0.0),
            Vec2::new(ONE_METER_PER_TICK, 0.0),
        );

        app.update();

        assert_eq!(position_of(&app, stuck), Position::new(2.0, 0.0));
    }
}
