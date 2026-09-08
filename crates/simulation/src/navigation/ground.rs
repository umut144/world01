use bevy::{
    log::info,
    prelude::{ParamSet, Query, Res},
};
use world01_content::CharacterHurtGeometryCatalog;
use world01_world_data::{
    ActorId, Ankh, AnkhLayout, BodyFacing, CharacterLifeState, MovementMedium, MovementVelocity,
    Position, SelectedCharacter, WorldMap, WorldPosition,
};

use crate::respawn::{RespawnActor, choose_respawn_position};
use crate::{CharacterLifeRules, MovementStep};

use super::TraversalCatalog;
use super::support::{
    CurrentSupport, SampleSupport, resolve_current_support, resolve_target,
    resolve_terrain_position, sample_is_usable,
};

/// Recovers Actors whose support disappeared or became unusable after a world
/// change. This is the server-owned safety rule until airborne falling exists.
pub(crate) fn recover_invalid_ground_support(
    rules: Option<Res<CharacterLifeRules>>,
    map: Option<Res<WorldMap>>,
    traversal: Option<Res<TraversalCatalog>>,
    ankhs: Option<Res<AnkhLayout>>,
    hurt_geometry: Option<Res<CharacterHurtGeometryCatalog>>,
    mut actors: ParamSet<(
        Query<(
            &ActorId,
            &SelectedCharacter,
            &WorldPosition,
            &BodyFacing,
            &CharacterLifeState,
        )>,
        Query<(
            &ActorId,
            &SelectedCharacter,
            &mut WorldPosition,
            &mut MovementMedium,
            &BodyFacing,
            Option<&mut MovementVelocity>,
        )>,
    )>,
) {
    let (Some(rules), Some(map), Some(traversal), Some(ankhs), Some(hurt_geometry)) =
        (rules, map, traversal, ankhs, hurt_geometry)
    else {
        return;
    };
    let snapshots = actors
        .p0()
        .iter()
        .map(
            |(actor_id, character, position, facing, life)| RespawnActor {
                actor_id: actor_id.0,
                character: Some(character.0.clone()),
                position: Some(*position),
                facing: Some(*facing),
                life: *life,
            },
        )
        .collect::<Vec<_>>();
    let ankhs = ankhs
        .positions
        .iter()
        .copied()
        .enumerate()
        .map(|(index, position)| (Ankh::new(index as u32), position))
        .collect::<Vec<_>>();

    for (actor_id, character, mut position, mut medium, facing, velocity) in &mut actors.p1() {
        let Some(profile) = traversal.character(&character.0) else {
            continue;
        };
        let recovery_reason = match &*medium {
            MovementMedium::Grounded(support) => {
                match resolve_current_support(&map, profile, support, *position) {
                    CurrentSupport::Resolved(sample) if sample_is_usable(profile, sample) => {
                        position.elevation_meters = sample.elevation_meters;
                        if sample.support != SampleSupport::from(support) {
                            *medium = MovementMedium::Grounded(sample.support.to_owned());
                        }
                        None
                    }
                    CurrentSupport::Resolved(_)
                    | CurrentSupport::MissingIdentity
                    | CurrentSupport::Unsupported => Some("invalid ground support"),
                }
            }
            MovementMedium::Airborne => Some("airborne movement is not implemented"),
            MovementMedium::Flying => None,
        };
        let Some(recovery_reason) = recovery_reason else {
            continue;
        };

        let Some(candidate) = choose_respawn_position(
            actor_id.0,
            0,
            *position,
            Some(&character.0),
            Some(*facing),
            rules.ankh_respawn_radius_meters(),
            &ankhs,
            &snapshots,
            Some(&hurt_geometry),
            |candidate| {
                resolve_terrain_position(&map, &traversal, &character.0, candidate).is_some()
            },
        ) else {
            if let Some(mut velocity) = velocity {
                *velocity = MovementVelocity::ZERO;
            }
            continue;
        };
        let Some(resolved) = resolve_terrain_position(&map, &traversal, &character.0, candidate)
        else {
            continue;
        };

        *position = resolved;
        *medium = MovementMedium::GROUNDED_TERRAIN;
        if let Some(mut velocity) = velocity {
            *velocity = MovementVelocity::ZERO;
        }
        info!(
            target: "game_console",
            "Actor {} safely relocated to an Ankh position ({:.2}, {:.2}, {:.2} m): {}",
            actor_id.0,
            resolved.x,
            resolved.y,
            resolved.elevation_meters,
            recovery_reason,
        );
    }
}

/// Applies the grade multiplier of the Path supporting an Actor before planar
/// collision evaluates this tick's movement endpoint.
pub fn apply_grounded_route_speed(
    step: Res<MovementStep>,
    map: Option<Res<WorldMap>>,
    traversal: Option<Res<TraversalCatalog>>,
    mut actors: Query<(
        &SelectedCharacter,
        &mut WorldPosition,
        &mut MovementMedium,
        &mut MovementVelocity,
    )>,
) {
    let (Some(map), Some(traversal)) = (map, traversal) else {
        for (_, _, _, mut velocity) in &mut actors {
            *velocity = MovementVelocity::ZERO;
        }
        return;
    };

    for (character, mut position, mut medium, mut velocity) in &mut actors {
        if *velocity == MovementVelocity::ZERO {
            continue;
        }
        let MovementMedium::Grounded(support) = &*medium else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        let Some(profile) = traversal.character(&character.0) else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        let current = match resolve_current_support(&map, profile, support, *position) {
            CurrentSupport::Resolved(sample) => sample,
            CurrentSupport::MissingIdentity | CurrentSupport::Unsupported => {
                *velocity = MovementVelocity::ZERO;
                *medium = MovementMedium::Airborne;
                continue;
            }
        };
        position.elevation_meters = current.elevation_meters;
        if current.support != SampleSupport::from(support) {
            *medium = MovementMedium::Grounded(current.support.to_owned());
        }
        if !profile.permits_surface(current.surface) {
            *velocity = MovementVelocity::ZERO;
            continue;
        }

        let displacement = step.step(*velocity);
        let proposed = Position::new(position.x + displacement.x, position.y + displacement.y);
        let target = resolve_target(&map, profile, current, proposed);
        let grade = target
            .and_then(|target| target.grade_percent)
            .or(current.grade_percent);
        if let Some(grade) = grade {
            let Some(speed) = profile.speed_for_grade(grade) else {
                *velocity = MovementVelocity::ZERO;
                continue;
            };
            *velocity = velocity.scaled(speed.multiplier());
        }
        // Wading is slower than walking. A step that begins or ends in water is
        // priced by the deeper of its two ends, so leaving the water costs what
        // entering it costs.
        let water_depth_meters = current.water_depth_meters.max(
            target
                .map(|target| target.water_depth_meters)
                .unwrap_or_default(),
        );
        *velocity = velocity.scaled(profile.speed_through_water(water_depth_meters).multiplier());
    }
}

/// Constrains the already collision-shortened horizontal step to the Actor's
/// authored ground surface before movement integration applies it.
///
/// Height comes from the current support's baked geometry. A transition uses
/// the Character's surface and step rules, and never guesses between multiple
/// overlapping Route surfaces.
pub fn constrain_grounded_movement(
    step: Res<MovementStep>,
    map: Option<Res<WorldMap>>,
    traversal: Option<Res<TraversalCatalog>>,
    mut actors: Query<(
        &SelectedCharacter,
        &mut MovementVelocity,
        &mut WorldPosition,
        &mut MovementMedium,
    )>,
) {
    let (Some(map), Some(traversal)) = (map, traversal) else {
        for (_, mut velocity, _, _) in &mut actors {
            *velocity = MovementVelocity::ZERO;
        }
        return;
    };

    for (character, mut velocity, mut position, mut medium) in &mut actors {
        let MovementMedium::Grounded(support) = &*medium else {
            // No airborne or flying movement rule exists yet. Keeping these
            // Actors stationary avoids accidentally treating either medium as
            // unconstrained planar movement.
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        let Some(profile) = traversal.character(&character.0) else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        let current = match resolve_current_support(&map, profile, support, *position) {
            CurrentSupport::Resolved(sample) => sample,
            CurrentSupport::MissingIdentity | CurrentSupport::Unsupported => {
                *velocity = MovementVelocity::ZERO;
                *medium = MovementMedium::Airborne;
                continue;
            }
        };

        // A stationary Actor follows an edited support as well. Template
        // changes therefore cannot leave Grounded state at a stale elevation.
        position.elevation_meters = current.elevation_meters;
        if current.support != SampleSupport::from(support) {
            *medium = MovementMedium::Grounded(current.support.to_owned());
        }
        if !profile.permits_surface(current.surface) {
            *velocity = MovementVelocity::ZERO;
            continue;
        }
        if *velocity == MovementVelocity::ZERO {
            continue;
        }

        let displacement = step.step(*velocity);
        let proposed = Position::new(position.x + displacement.x, position.y + displacement.y);
        let Some(target) = resolve_target(&map, profile, current, proposed) else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        position.elevation_meters = target.elevation_meters;
        if target.support != current.support {
            *medium = MovementMedium::Grounded(target.support.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{App, IntoScheduleConfigs, Update, Vec2};
    use world01_configs::load_embedded as load_runtime;
    use world01_content::{CharacterHurtGeometryCatalog, RuntimeContent};
    use world01_design::{load_embedded as load_game_design, load_world01_embedded};
    use world01_world_data::{
        ActorId, AnkhLayout, BodyFacing, CharacterHealth, CharacterId, CharacterLifeState,
        GroundSupport, MovementMedium, RespawnState,
    };

    use super::*;

    fn app() -> App {
        let runtime = load_runtime().expect("embedded runtime configuration is valid");
        let design = load_game_design().expect("embedded game design is valid");
        let traversal = TraversalCatalog::from_design(&design.traversal)
            .expect("embedded traversal profiles are valid");
        let mut app = App::new();
        app.insert_resource(
            MovementStep::from_runtime(&runtime).expect("runtime movement step is valid"),
        )
        .insert_resource(
            WorldMap::load_embedded("overworld01").expect("embedded Instance is valid"),
        )
        .insert_resource(traversal)
        .add_systems(
            Update,
            (
                apply_grounded_route_speed,
                constrain_grounded_movement,
                crate::integrate_movement,
            )
                .chain(),
        );
        app
    }

    fn recovery_app() -> App {
        let runtime = load_runtime().expect("embedded runtime configuration is valid");
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let game_design = load_game_design().expect("embedded game design is valid");
        let world_design = load_world01_embedded().expect("embedded World 01 design is valid");
        let map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        let ankhs = AnkhLayout::from_map(&map);
        let mut app = App::new();
        app.insert_resource(
            CharacterLifeRules::from_design(
                runtime.simulation.ticks_per_second,
                &world_design.health,
            )
            .expect("embedded life rules are valid"),
        )
        .insert_resource(map)
        .insert_resource(ankhs)
        .insert_resource(
            TraversalCatalog::from_design(&game_design.traversal)
                .expect("embedded traversal profiles are valid"),
        )
        .insert_resource(
            CharacterHurtGeometryCatalog::from_content(&content, &game_design.hurt)
                .expect("embedded hurt geometry is valid"),
        )
        .add_systems(Update, recover_invalid_ground_support);
        app
    }

    #[test]
    fn invalid_ground_support_is_safely_relocated_without_life_side_effects() {
        let mut app = recovery_app();
        let actor = app
            .world_mut()
            .spawn((
                ActorId(7),
                SelectedCharacter(CharacterId("hammerer".into())),
                WorldPosition::new(10_000.0, 10_000.0, -100.0),
                MovementMedium::GROUNDED_TERRAIN,
                MovementVelocity::new(1.0, 0.0),
                BodyFacing::Authored,
                CharacterLifeState::Alive,
                CharacterHealth {
                    current: 73.0,
                    maximum: 100.0,
                },
                RespawnState { count: 9 },
            ))
            .id();

        app.update();

        let position = *app
            .world()
            .get::<WorldPosition>(actor)
            .expect("actor keeps a world position");
        let terrain = app
            .world()
            .resource::<WorldMap>()
            .terrain_cell_at(position.horizontal())
            .expect("safety destination has valid Terrain");
        assert_eq!(position.elevation_meters, terrain.elevation_meters);
        assert_eq!(
            app.world().get::<MovementMedium>(actor),
            Some(&MovementMedium::GROUNDED_TERRAIN)
        );
        assert_eq!(
            app.world().get::<MovementVelocity>(actor),
            Some(&MovementVelocity::ZERO)
        );
        assert_eq!(
            app.world().get::<CharacterHealth>(actor),
            Some(&CharacterHealth {
                current: 73.0,
                maximum: 100.0,
            })
        );
        assert_eq!(
            app.world().get::<CharacterLifeState>(actor),
            Some(&CharacterLifeState::Alive)
        );
        assert_eq!(
            app.world().get::<RespawnState>(actor),
            Some(&RespawnState { count: 9 })
        );
    }

    #[test]
    fn airborne_actor_uses_the_same_safety_recovery() {
        let mut app = recovery_app();
        let actor = app
            .world_mut()
            .spawn((
                ActorId(8),
                SelectedCharacter(CharacterId("hammerer".into())),
                WorldPosition::new(10_000.0, 10_000.0, -100.0),
                MovementMedium::Airborne,
                BodyFacing::Authored,
                CharacterLifeState::Alive,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<MovementMedium>(actor),
            Some(&MovementMedium::GROUNDED_TERRAIN)
        );
        let position = *app
            .world()
            .get::<WorldPosition>(actor)
            .expect("actor keeps a world position");
        assert!(
            app.world()
                .resource::<WorldMap>()
                .terrain_cell_at(position.horizontal())
                .is_some()
        );
    }

    #[test]
    fn terrain_support_sets_the_authored_elevation_before_movement() {
        let mut app = app();
        let start = Position::new(0.0, 0.0);
        let expected_elevation = app
            .world()
            .resource::<WorldMap>()
            .terrain_cell_at(start)
            .expect("test position has Terrain")
            .elevation_meters;
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                MovementVelocity::new(1.0, 0.0),
                WorldPosition::new(start.x, start.y, -100.0),
                MovementMedium::GROUNDED_TERRAIN,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<WorldPosition>(actor)
                .expect("actor keeps its position")
                .elevation_meters,
            expected_elevation
        );
        assert_ne!(
            app.world()
                .get::<MovementVelocity>(actor)
                .expect("actor keeps its velocity"),
            &MovementVelocity::ZERO
        );
    }

    #[test]
    fn leaving_authored_terrain_blocks_the_horizontal_step() {
        let mut app = app();
        let x = app.world().resource::<WorldMap>().width_meters() / 2.0 - 0.01;
        let start = Position::new(x, 0.0);
        let elevation = app
            .world()
            .resource::<WorldMap>()
            .terrain_cell_at(start)
            .expect("test position has Terrain")
            .elevation_meters;
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                MovementVelocity::new(100.0, 0.0),
                WorldPosition::new(start.x, start.y, elevation),
                MovementMedium::GROUNDED_TERRAIN,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<MovementVelocity>(actor)
                .expect("actor keeps its velocity"),
            &MovementVelocity::ZERO
        );
    }

    #[test]
    fn authored_height_discontinuity_above_half_a_meter_blocks_the_step() {
        let mut app = app();
        let (start, target, start_elevation) = {
            let map = app.world().resource::<WorldMap>();
            map.terrain_cells()
                .iter()
                .find_map(|cell| {
                    [
                        (cell.x.saturating_add(1), cell.y),
                        (cell.x, cell.y.saturating_add(1)),
                    ]
                    .into_iter()
                    .filter_map(|(x, y)| map.terrain_cell(x, y))
                    .find(|neighbor| {
                        (cell.elevation_meters - neighbor.elevation_meters).abs() > 0.5
                    })
                    .map(|neighbor| (cell.center, neighbor.center, cell.elevation_meters))
                })
                .expect("embedded Instance has a height discontinuity")
        };
        let velocity = app
            .world()
            .resource::<MovementStep>()
            .velocity_of(Vec2::new(target.x - start.x, target.y - start.y));
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                velocity,
                WorldPosition::new(start.x, start.y, start_elevation),
                MovementMedium::GROUNDED_TERRAIN,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<MovementVelocity>(actor)
                .expect("actor keeps its velocity"),
            &MovementVelocity::ZERO
        );
        assert_eq!(
            app.world()
                .get::<WorldPosition>(actor)
                .expect("actor keeps its position")
                .elevation_meters,
            start_elevation
        );
    }

    #[test]
    fn unsupported_ground_becomes_explicitly_airborne() {
        let mut app = app();
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                MovementVelocity::new(1.0, 0.0),
                WorldPosition::new(10_000.0, 10_000.0, 1.0),
                MovementMedium::GROUNDED_TERRAIN,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<MovementMedium>(actor)
                .expect("actor keeps its movement medium"),
            &MovementMedium::Airborne
        );
        assert_eq!(
            app.world()
                .get::<MovementVelocity>(actor)
                .expect("actor keeps its velocity"),
            &MovementVelocity::ZERO
        );
    }

    #[test]
    fn route_support_does_not_fall_back_to_terrain() {
        let mut app = app();
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                MovementVelocity::new(1.0, 0.0),
                WorldPosition::new(0.0, 0.0, 1.0),
                MovementMedium::Grounded(GroundSupport::RouteSurface {
                    route_surface_id: "not-yet-resolved".into(),
                }),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<MovementVelocity>(actor)
                .expect("actor keeps its velocity"),
            &MovementVelocity::ZERO
        );
        assert_eq!(
            app.world().get::<MovementMedium>(actor),
            Some(&MovementMedium::Airborne)
        );
    }

    #[test]
    fn one_place_under_an_excavated_hill_carries_two_actors_at_their_own_heights() {
        let mut app = app();
        let (position, floor, hill_top) = {
            let map = app.world().resource::<WorldMap>();
            let cut = map
                .route_surface_cuts()
                .first()
                .expect("the overworld carries an excavating Path");
            let water_cell = |x: u32, y: u32| {
                Position::new(
                    -map.width_meters() / 2.0 + (x as f32 + 0.5) * map.water_cell_meters(),
                    -map.height_meters() / 2.0 + (y as f32 + 0.5) * map.water_cell_meters(),
                )
            };
            let cell = cut
                .cells
                .iter()
                .find(|cell| {
                    map.terrain_cell_at(water_cell(cell.x, cell.y))
                        .is_some_and(|terrain| terrain.elevation_meters > cell.cut_top_meters)
                })
                .expect("the excavation reaches ground that stands above it");
            let position = water_cell(cell.x, cell.y);
            let hill_top = map
                .terrain_cell_at(position)
                .expect("that cell has Terrain")
                .elevation_meters;
            (position, cell.floor_meters, hill_top)
        };

        let spawn = |app: &mut App, elevation| {
            app.world_mut()
                .spawn((
                    SelectedCharacter(CharacterId("hammerer".into())),
                    MovementVelocity::ZERO,
                    WorldPosition::new(position.x, position.y, elevation),
                    MovementMedium::GROUNDED_TERRAIN,
                ))
                .id()
        };
        let through = spawn(&mut app, floor);
        let over = spawn(&mut app, hill_top);

        app.update();

        assert_eq!(
            app.world()
                .get::<WorldPosition>(through)
                .expect("the Actor in the excavation keeps its position")
                .elevation_meters,
            floor,
            "the excavation leaves a floor to walk on"
        );
        assert_eq!(
            app.world()
                .get::<WorldPosition>(over)
                .expect("the Actor above keeps its position")
                .elevation_meters,
            hill_top,
            "the ground above the excavation still stands"
        );
        for actor in [through, over] {
            assert!(
                matches!(
                    app.world().get::<MovementMedium>(actor),
                    Some(MovementMedium::Grounded(_))
                ),
                "neither Actor loses its support"
            );
        }
    }

    #[test]
    fn terrain_enters_the_reachable_start_of_one_route() {
        let mut app = app();
        let (route_id, start, direction, terrain_elevation) = {
            let map = app.world().resource::<WorldMap>();
            let route = &map.route_surfaces()[0];
            let start = route.centerline_samples[0].position;
            let next = route.centerline_samples[1].position;
            let direction = Vec2::new(next.x - start.x, next.y - start.y).normalize() * 0.05;
            let terrain_elevation = map
                .terrain_cell_at(start)
                .expect("the authored Path start meets Terrain")
                .elevation_meters;
            (
                route.route_surface_id.clone(),
                start,
                direction,
                terrain_elevation,
            )
        };
        let velocity = app
            .world()
            .resource::<MovementStep>()
            .velocity_of(direction);
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                velocity,
                WorldPosition::new(start.x, start.y, terrain_elevation),
                MovementMedium::GROUNDED_TERRAIN,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<MovementMedium>(actor),
            Some(&MovementMedium::Grounded(GroundSupport::RouteSurface {
                route_surface_id: route_id,
            }))
        );
        let position = *app
            .world()
            .get::<WorldPosition>(actor)
            .expect("actor keeps its position");
        let route = app
            .world()
            .resource::<WorldMap>()
            .route_surfaces()
            .first()
            .expect("test route exists");
        let sampled = route
            .sample_at(position.horizontal())
            .expect("actor ends on the Path");
        assert!((position.elevation_meters - sampled.elevation_meters).abs() < 0.000_1);
    }

    #[test]
    fn route_grade_controls_actual_horizontal_speed_in_both_directions() {
        for (route_index, expected_multiplier) in [(0, 1.0), (1, 0.5), (2, 0.5)] {
            let mut app = app();
            let (route_id, start, requested) = {
                let map = app.world().resource::<WorldMap>();
                let route = &map.route_surfaces()[route_index];
                let start = route.centerline_samples[0].position;
                let next = route.centerline_samples[1].position;
                (
                    route.route_surface_id.clone(),
                    start,
                    Vec2::new(next.x - start.x, next.y - start.y).normalize() * 0.05,
                )
            };
            let velocity = app
                .world()
                .resource::<MovementStep>()
                .velocity_of(requested);
            let actor = app
                .world_mut()
                .spawn((
                    SelectedCharacter(CharacterId("hammerer".into())),
                    velocity,
                    WorldPosition::new(start.x, start.y, 0.0),
                    MovementMedium::Grounded(GroundSupport::RouteSurface {
                        route_surface_id: route_id,
                    }),
                ))
                .id();

            app.update();

            let position = *app
                .world()
                .get::<WorldPosition>(actor)
                .expect("actor keeps its position");
            let actual = Vec2::new(position.x - start.x, position.y - start.y).length();
            assert!(
                (actual - requested.length() * expected_multiplier).abs() < 0.000_1,
                "route {route_index} moved {actual}"
            );
            let route = &app.world().resource::<WorldMap>().route_surfaces()[route_index];
            let sampled = route
                .sample_at(position.horizontal())
                .expect("actor remains on its Path");
            assert!((position.elevation_meters - sampled.elevation_meters).abs() < 0.000_1);
        }
    }

    #[test]
    fn first_step_onto_a_half_speed_route_is_already_reduced() {
        let mut app = app();
        let (start, requested, terrain_elevation) = {
            let map = app.world().resource::<WorldMap>();
            let route = &map.route_surfaces()[1];
            let start = route.centerline_samples[0].position;
            let next = route.centerline_samples[1].position;
            (
                start,
                Vec2::new(next.x - start.x, next.y - start.y).normalize() * 0.05,
                map.terrain_cell_at(start)
                    .expect("the authored Path start meets Terrain")
                    .elevation_meters,
            )
        };
        let velocity = app
            .world()
            .resource::<MovementStep>()
            .velocity_of(requested);
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                velocity,
                WorldPosition::new(start.x, start.y, terrain_elevation),
                MovementMedium::GROUNDED_TERRAIN,
            ))
            .id();

        app.update();

        let position = *app
            .world()
            .get::<WorldPosition>(actor)
            .expect("actor keeps its position");
        assert!(
            (Vec2::new(position.x - start.x, position.y - start.y).length()
                - requested.length() * 0.5)
                .abs()
                < 0.000_1
        );
        assert!(matches!(
            app.world().get::<MovementMedium>(actor),
            Some(MovementMedium::Grounded(GroundSupport::RouteSurface { .. }))
        ));
    }

    #[test]
    fn elevated_route_does_not_pull_a_terrain_actor_up_from_below() {
        let mut app = app();
        let (start, requested, terrain_elevation) = {
            let map = app.world().resource::<WorldMap>();
            let route = &map.route_surfaces()[1];
            let sample_index = route.centerline_samples.len() / 2;
            let start = route.centerline_samples[sample_index].position;
            let next = route.centerline_samples[sample_index + 1].position;
            let terrain_elevation = map
                .terrain_cell_at(start)
                .expect("Terrain exists below the elevated Path")
                .elevation_meters;
            assert!(
                (route.centerline_samples[sample_index].elevation_meters - terrain_elevation).abs()
                    > 0.5
            );
            (
                start,
                Vec2::new(next.x - start.x, next.y - start.y).normalize() * 0.05,
                terrain_elevation,
            )
        };
        let velocity = app
            .world()
            .resource::<MovementStep>()
            .velocity_of(requested);
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                velocity,
                WorldPosition::new(start.x, start.y, terrain_elevation),
                MovementMedium::GROUNDED_TERRAIN,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<MovementMedium>(actor),
            Some(&MovementMedium::GROUNDED_TERRAIN)
        );
        let position = *app
            .world()
            .get::<WorldPosition>(actor)
            .expect("actor keeps its position");
        assert_eq!(position.elevation_meters, terrain_elevation);
    }
}
