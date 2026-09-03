//! Where an actor is allowed to end this tick.
//!
//! Collision runs between deciding a velocity and applying it: it may refuse a
//! step, never move an actor. That keeps `integrate_movement` the only writer
//! of `Position` and keeps the refusal itself readable as a zeroed velocity.
//!
//! What blocks is authored, not derived. An actor whose character has no
//! `collision` Region occupies no space: it passes through everything and stops
//! nobody. That is a content state the design allows, not a missing feature.

use bevy::prelude::{Entity, Query, Res, Vec2};
use world01_content::{
    CharacterCollisionGeometry, CharacterCollisionGeometryCatalog, WorldCollisionGeometryCatalog,
};
use world01_world_data::{BodyFacing, MovementVelocity, Position, SelectedCharacter};

use crate::movement::MovementStep;
use crate::spatial::broadphase::{Aabb, WorldColliderGrid};
use crate::spatial::overlap::{
    ComponentSeparation, GeometryTransform, component_separation, components_overlap,
    facing_transform, transformed_points,
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

/// Shortens movement that would end inside world geometry or another actor.
///
/// A blocked step is retried once along the surface it hit: the component of
/// the step that points into the surface is removed and the rest is kept, so an
/// actor walking at a tree slides past it instead of stopping dead. If that
/// shortened step is blocked too - a corner, or a second collider the slide
/// runs into - the actor does not move. One retry rather than several, because
/// each further one can pick a different surface and walk the actor around a
/// shape it never touched.
///
/// A step straight into a surface has no part along it and is simply stopped.
/// The normal comes from authored Component boundary edges, so interior edges
/// introduced only by triangulation cannot deflect a head-on meeting.
///
/// An actor is tested against both where the others stand and where their own
/// steps would take them. Measuring only against where they stand is
/// order-independent, which is why it was chosen, but it lets two actors
/// approaching each other each take a step that is legal on its own while the
/// pair of them ends up overlapping. Counting the other's step as well is
/// equally order-independent and does not have that hole.
///
/// An actor that already overlaps something may leave but may not go deeper.
/// Being able to leave is what keeps a character spawned inside a prop, or put
/// there by a correction from the server, from being stuck forever; not being
/// able to go deeper is what stops it from continuing straight out the far
/// side.
pub fn block_colliding_movement(
    step: Res<MovementStep>,
    catalog: Res<CharacterCollisionGeometryCatalog>,
    world: Res<WorldCollisionGeometryCatalog>,
    grid: Res<WorldColliderGrid>,
    mut actors: Query<(
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
    let mut blockers = Vec::new();
    for (entity, character, position, facing, velocity) in actors.iter() {
        let Some(geometry) = catalog.character(&character.0) else {
            continue;
        };
        let Some(standing) = posed(entity, geometry, *position, *facing) else {
            continue;
        };
        let displacement = step.step(*velocity);
        if displacement != Vec2::ZERO && displacement.is_finite() {
            blockers.push(standing.translated(displacement));
        }
        blockers.push(standing);
    }

    let mut candidates = Vec::new();
    for (entity, character, position, facing, mut velocity) in &mut actors {
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
        if !blocked(proposed, &blockers, &world, &grid, &mut candidates) {
            continue;
        }
        if let Some(separation) =
            deepest_separation(current, &blockers, &world, &grid, &mut candidates)
        {
            if displacement.dot(separation.normal) < 0.0 {
                *velocity = MovementVelocity::ZERO;
            }
            continue;
        }
        let slid = deepest_separation(proposed, &blockers, &world, &grid, &mut candidates)
            .and_then(|separation| slide_along(displacement, separation))
            .filter(|slid| {
                !blocked(
                    current.translated(*slid),
                    &blockers,
                    &world,
                    &grid,
                    &mut candidates,
                )
            });
        *velocity = slid.map_or(MovementVelocity::ZERO, |slid| step.velocity_of(slid));
    }
}

/// The part of `displacement` that runs along the surface rather than into it.
///
/// `None` when the step does not point into the surface at all, which means the
/// contact is not what stopped this step and sliding along it would invent
/// movement, and when nothing is left to slide with.
fn slide_along(displacement: Vec2, separation: ComponentSeparation) -> Option<Vec2> {
    let into_surface = displacement.dot(separation.normal);
    if into_surface >= 0.0 {
        return None;
    }
    let slid = displacement - separation.normal * into_surface;
    (slid.length_squared() > 0.0).then_some(slid)
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
        bounds: Aabb::around(
            geometry
                .components
                .iter()
                .flat_map(|component| transformed_points(component.geometry(), transform)),
        )?,
    })
}

/// The exact-contact translation with the greatest distance across every
/// overlapping Component pair.
///
/// Component separation distances are measured in meters and are comparable,
/// unlike the old triangle-pair penetration heuristic. Equal distances choose
/// a normal by value so catalog and query iteration order cannot decide the
/// slide direction.
fn deepest_separation(
    actor: PosedCollider<'_>,
    blockers: &[PosedCollider<'_>],
    world: &WorldCollisionGeometryCatalog,
    grid: &WorldColliderGrid,
    candidates: &mut Vec<u32>,
) -> Option<ComponentSeparation> {
    let mut deepest: Option<ComponentSeparation> = None;
    let mut keep = |separation: Option<ComponentSeparation>| {
        if let Some(separation) = separation
            && deepest.is_none_or(|best| separation_is_deeper(separation, best))
        {
            deepest = Some(separation);
        }
    };
    grid.candidates(actor.bounds, candidates);
    for index in candidates.iter() {
        let region = &world.regions[*index as usize];
        let placement = GeometryTransform::translated(region.position);
        for component in &actor.geometry.components {
            keep(component_separation(
                component,
                actor.transform,
                &region.component,
                placement,
            ));
        }
    }
    for blocker in blockers {
        if blocker.entity == actor.entity || !blocker.bounds.overlaps(actor.bounds) {
            continue;
        }
        for component in &actor.geometry.components {
            for other in &blocker.geometry.components {
                keep(component_separation(
                    component,
                    actor.transform,
                    other,
                    blocker.transform,
                ));
            }
        }
    }
    deepest
}

fn separation_is_deeper(candidate: ComponentSeparation, current: ComponentSeparation) -> bool {
    candidate
        .separation_distance
        .total_cmp(&current.separation_distance)
        .then_with(|| current.normal.x.total_cmp(&candidate.normal.x))
        .then_with(|| current.normal.y.total_cmp(&candidate.normal.y))
        .is_gt()
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
            if components_overlap(
                component.geometry(),
                actor.transform,
                region.component.geometry(),
                placement,
            ) {
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
            components_overlap(
                component.geometry(),
                actor.transform,
                other.geometry(),
                blocker.transform,
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, IntoScheduleConfigs, Update};
    use world01_configs::load_embedded;
    use world01_content::{
        AuthoredFacing, CollisionComponentGeometry, PlacedCollisionGeometry,
        RuntimeComponentGeometry,
    };
    use world01_world_data::CharacterId;

    use crate::movement::integrate_movement;

    /// One tick at this velocity covers exactly one meter, so a test can name
    /// the position a step would reach instead of counting ticks.
    const ONE_METER_PER_TICK: f32 = 60.0;

    /// A shortened step is divided by the tick length and multiplied by it
    /// again, so it comes back a few bits short of where it started.
    const EPSILON: f32 = 0.000_1;

    fn square(name: &str, corner: Vec2, size: f32) -> CollisionComponentGeometry {
        CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: name.to_owned(),
            name: name.to_owned(),
            vertices: vec![
                corner,
                corner + Vec2::new(size, 0.0),
                corner + Vec2::splat(size),
                corner + Vec2::new(0.0, size),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        })
        .expect("the test square has valid collision topology")
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

    fn block(name: &str, corner: Vec2, width: f32, height: f32) -> CollisionComponentGeometry {
        CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: name.to_owned(),
            name: name.to_owned(),
            vertices: vec![
                corner,
                corner + Vec2::new(width, 0.0),
                corner + Vec2::new(width, height),
                corner + Vec2::new(0.0, height),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        })
        .expect("the test block has valid collision topology")
    }

    /// A wall covering x in [1, 2] and y in [-2, 2].
    fn wall() -> WorldCollisionGeometryCatalog {
        WorldCollisionGeometryCatalog {
            regions: vec![PlacedCollisionGeometry {
                instance_id: "wall".to_owned(),
                position: Position::new(1.0, -2.0),
                component: block("block", Vec2::ZERO, 1.0, 4.0),
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
            .add_systems(
                Update,
                (block_colliding_movement, integrate_movement).chain(),
            );
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

    fn assert_velocity(actual: MovementVelocity, expected: MovementVelocity) {
        assert!(
            (actual.x - expected.x).abs() < EPSILON && (actual.y - expected.y).abs() < EPSILON,
            "{actual:?} != {expected:?}"
        );
    }

    fn assert_position(actual: Position, expected: Position) {
        assert!(
            (actual.x - expected.x).abs() < EPSILON && (actual.y - expected.y).abs() < EPSILON,
            "{actual:?} != {expected:?}"
        );
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

    /// The reason this change exists: a diagonal step at a wall keeps the half
    /// of itself that runs along the wall instead of being thrown away.
    #[test]
    fn a_step_that_grazes_a_wall_keeps_the_part_that_runs_along_it() {
        let mut app = app(wall());
        let actor = spawn(
            &mut app,
            "walker",
            Vec2::ZERO,
            Vec2::splat(ONE_METER_PER_TICK),
        );

        app.update();

        assert_velocity(
            velocity_of(&app, actor),
            MovementVelocity::new(0.0, ONE_METER_PER_TICK),
        );
        assert_position(position_of(&app, actor), Position::new(0.0, 1.0));
    }

    /// A step straight into world geometry has no part that runs along it.
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

    /// Sliding may only shorten a step, never redirect it into something else.
    #[test]
    fn a_slide_that_would_end_inside_a_second_collider_stops_instead() {
        let mut app = app(WorldCollisionGeometryCatalog {
            regions: vec![
                PlacedCollisionGeometry {
                    instance_id: "wall".to_owned(),
                    position: Position::new(1.0, -2.0),
                    component: block("block", Vec2::ZERO, 1.0, 4.0),
                },
                // Clear of where the diagonal step would end, but across the
                // slide that step would turn into.
                PlacedCollisionGeometry {
                    instance_id: "ledge".to_owned(),
                    position: Position::new(-1.0, 0.5),
                    component: block("block", Vec2::ZERO, 1.5, 1.0),
                },
            ],
        });
        let actor = spawn(
            &mut app,
            "walker",
            Vec2::ZERO,
            Vec2::splat(ONE_METER_PER_TICK),
        );

        app.update();

        assert_eq!(velocity_of(&app, actor), MovementVelocity::ZERO);
        assert_eq!(position_of(&app, actor), Position::ZERO);
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
        let unblocked = spawn(
            &mut app,
            "no_collider",
            Vec2::ZERO,
            Vec2::new(ONE_METER_PER_TICK, 0.0),
        );
        spawn(&mut app, "walker", Vec2::new(1.0, 0.0), Vec2::ZERO);

        app.update();

        assert_eq!(position_of(&app, unblocked), Position::new(1.0, 0.0));
    }

    /// Leaving is what keeps an actor from being stuck forever. Going deeper is
    /// what used to let it continue straight out the far side.
    #[test]
    fn an_actor_that_already_overlaps_may_leave_but_not_go_deeper() {
        let mut app = app(wall());
        let deeper = spawn(
            &mut app,
            "walker",
            Vec2::new(1.0, 0.0),
            Vec2::new(ONE_METER_PER_TICK, 0.0),
        );
        let leaving = spawn(
            &mut app,
            "walker",
            Vec2::new(1.0, 8.0),
            Vec2::new(-ONE_METER_PER_TICK, 0.0),
        );

        app.update();

        assert_eq!(position_of(&app, deeper), Position::new(1.0, 0.0));
        assert_eq!(position_of(&app, leaving), Position::new(0.0, 8.0));
    }

    /// Both proposed poses are considered, and their Component boundaries say
    /// this is a head-on contact. The diagonal interior seam in each square's
    /// triangulation must not turn either step into a sideways slide.
    #[test]
    fn two_actors_walking_head_on_stop_without_diagonal_deflection() {
        let mut app = app(WorldCollisionGeometryCatalog::default());
        let left = spawn(
            &mut app,
            "walker",
            Vec2::ZERO,
            Vec2::new(ONE_METER_PER_TICK, 0.0),
        );
        let right = spawn(
            &mut app,
            "walker",
            Vec2::new(2.1, 0.0),
            Vec2::new(-ONE_METER_PER_TICK, 0.0),
        );

        app.update();

        assert_eq!(velocity_of(&app, left), MovementVelocity::ZERO);
        assert_eq!(velocity_of(&app, right), MovementVelocity::ZERO);
        assert_eq!(position_of(&app, left), Position::ZERO);
        assert_eq!(position_of(&app, right), Position::new(2.1, 0.0));
    }
}
