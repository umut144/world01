use bevy::prelude::{Query, Res};
use world01_world_data::{
    GroundSupport, MapRouteSurface, MovementMedium, MovementVelocity, Position, SelectedCharacter,
    WorldMap, WorldPosition,
};

use crate::MovementStep;

use super::{CharacterTraversalProfile, TraversalCatalog};

/// Applies the grade multiplier of the Path supporting an Actor before planar
/// collision evaluates this tick's movement endpoint.
pub fn apply_grounded_route_speed(
    map: Option<Res<WorldMap>>,
    traversal: Option<Res<TraversalCatalog>>,
    mut actors: Query<(
        &SelectedCharacter,
        &WorldPosition,
        &MovementMedium,
        &mut MovementVelocity,
    )>,
) {
    let (Some(map), Some(traversal)) = (map, traversal) else {
        for (_, _, _, mut velocity) in &mut actors {
            *velocity = MovementVelocity::ZERO;
        }
        return;
    };

    for (character, position, medium, mut velocity) in &mut actors {
        if *velocity == MovementVelocity::ZERO {
            continue;
        }
        let MovementMedium::Grounded(support) = medium else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        let Some(profile) = traversal.character(&character.0) else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        match support {
            GroundSupport::Terrain => {}
            GroundSupport::RouteSurface { route_surface_id } => {
                let Some(route) = map.route_surface(route_surface_id) else {
                    *velocity = MovementVelocity::ZERO;
                    continue;
                };
                let Some(sample) = route.sample_at(position.horizontal()) else {
                    *velocity = MovementVelocity::ZERO;
                    continue;
                };
                if !profile.permits_surface(&route.surface) {
                    *velocity = MovementVelocity::ZERO;
                    continue;
                }
                let Some(speed) = profile.speed_for_grade(sample.grade_percent) else {
                    *velocity = MovementVelocity::ZERO;
                    continue;
                };
                *velocity = velocity.scaled(speed.multiplier());
            }
        }
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
        let current = sample_support(&map, support, position.horizontal()).or_else(|| {
            resolve_detached_support(
                &map,
                profile,
                position.elevation_meters,
                position.horizontal(),
            )
        });
        let Some(current) = current else {
            *velocity = MovementVelocity::ZERO;
            *medium = MovementMedium::Airborne;
            continue;
        };
        if !profile.permits_surface(current.surface) {
            *velocity = MovementVelocity::ZERO;
            *medium = MovementMedium::Airborne;
            continue;
        }

        // A stationary Actor follows an edited support as well. Template
        // changes therefore cannot leave Grounded state at a stale elevation.
        position.elevation_meters = current.elevation_meters;
        if !current.support.matches(support) {
            *medium = MovementMedium::Grounded(current.support.to_owned());
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleSupport<'a> {
    Terrain,
    RouteSurface(&'a str),
}

impl SampleSupport<'_> {
    fn matches(self, support: &GroundSupport) -> bool {
        match (self, support) {
            (Self::Terrain, GroundSupport::Terrain) => true,
            (
                Self::RouteSurface(first),
                GroundSupport::RouteSurface {
                    route_surface_id: second,
                },
            ) => first == second,
            _ => false,
        }
    }

    fn to_owned(self) -> GroundSupport {
        match self {
            Self::Terrain => GroundSupport::Terrain,
            Self::RouteSurface(route_surface_id) => GroundSupport::RouteSurface {
                route_surface_id: route_surface_id.to_owned(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct GroundSample<'a> {
    support: SampleSupport<'a>,
    surface: &'a str,
    elevation_meters: f32,
    grade_percent: Option<i32>,
}

fn sample_support<'a>(
    map: &'a WorldMap,
    support: &GroundSupport,
    position: Position,
) -> Option<GroundSample<'a>> {
    match support {
        GroundSupport::Terrain => sample_terrain(map, position),
        GroundSupport::RouteSurface { route_surface_id } => map
            .route_surface(route_surface_id)
            .and_then(|route| sample_route(route, position)),
    }
}

fn sample_terrain(map: &WorldMap, position: Position) -> Option<GroundSample<'_>> {
    let cell = map.terrain_cell_at(position)?;
    Some(GroundSample {
        support: SampleSupport::Terrain,
        surface: &cell.surface,
        elevation_meters: cell.elevation_meters,
        grade_percent: None,
    })
}

fn sample_route(route: &MapRouteSurface, position: Position) -> Option<GroundSample<'_>> {
    let sample = route.sample_at(position)?;
    Some(GroundSample {
        support: SampleSupport::RouteSurface(&route.route_surface_id),
        surface: &route.surface,
        elevation_meters: sample.elevation_meters,
        grade_percent: Some(sample.grade_percent),
    })
}

fn resolve_target<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current: GroundSample<'a>,
    position: Position,
) -> Option<GroundSample<'a>> {
    if let SampleSupport::RouteSurface(route_surface_id) = current.support {
        let route = map.route_surface(route_surface_id)?;
        if let Some(target) = sample_route(route, position) {
            return sample_is_reachable(profile, current.elevation_meters, target)
                .then_some(target);
        }
    }

    let excluded_route = match current.support {
        SampleSupport::Terrain => None,
        SampleSupport::RouteSurface(route_surface_id) => Some(route_surface_id),
    };
    if let Ok(Some(route)) = unique_reachable_route(
        map,
        profile,
        current.elevation_meters,
        position,
        excluded_route,
    ) {
        return Some(route);
    }

    let terrain = sample_terrain(map, position)?;
    sample_is_reachable(profile, current.elevation_meters, terrain).then_some(terrain)
}

fn resolve_detached_support<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    position: Position,
) -> Option<GroundSample<'a>> {
    if let Ok(Some(route)) =
        unique_reachable_route(map, profile, current_elevation_meters, position, None)
    {
        return Some(route);
    }
    let terrain = sample_terrain(map, position)?;
    sample_is_reachable(profile, current_elevation_meters, terrain).then_some(terrain)
}

fn unique_reachable_route<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    position: Position,
    excluded_route: Option<&str>,
) -> Result<Option<GroundSample<'a>>, ()> {
    let mut candidate = None;
    for route in map.route_surfaces() {
        if excluded_route == Some(route.route_surface_id.as_str()) {
            continue;
        }
        let Some(sample) = sample_route(route, position) else {
            continue;
        };
        if !sample_is_reachable(profile, current_elevation_meters, sample) {
            continue;
        }
        if candidate.is_some() {
            return Err(());
        }
        candidate = Some(sample);
    }
    Ok(candidate)
}

fn sample_is_reachable(
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    sample: GroundSample<'_>,
) -> bool {
    profile.permits_surface(sample.surface)
        && sample
            .grade_percent
            .is_none_or(|grade| profile.speed_for_grade(grade).is_some())
        && profile.permits_step(current_elevation_meters, sample.elevation_meters)
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{App, IntoScheduleConfigs, Update, Vec2};
    use world01_configs::load_embedded as load_runtime;
    use world01_content::{CharacterCollisionGeometryCatalog, RuntimeContent};
    use world01_design::load_embedded as load_game_design;
    use world01_world_data::{CharacterId, GroundSupport, MovementMedium};

    use super::*;

    fn app() -> App {
        let runtime = load_runtime().expect("embedded runtime configuration is valid");
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let geometry = CharacterCollisionGeometryCatalog::from_content(&content)
            .expect("embedded collision geometry is valid");
        let design = load_game_design().expect("embedded game design is valid");
        let traversal = TraversalCatalog::from_design_and_geometry(&design.traversal, &geometry)
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
