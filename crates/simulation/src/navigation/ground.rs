use bevy::{
    log::info,
    prelude::{ParamSet, Query, Res},
};
use world01_content::CharacterHurtGeometryCatalog;
use world01_world_data::{
    ActorId, Ankh, AnkhLayout, BodyFacing, CharacterLifeState, GroundSupport, MapRouteSurface,
    MovementMedium, MovementVelocity, Position, SelectedCharacter, WorldMap, WorldPosition,
};

use crate::respawn::{RespawnActor, choose_respawn_position};
use crate::{CharacterLifeRules, MovementStep};

use super::{CharacterTraversalProfile, TraversalCatalog};

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
        let target_grade = resolve_target(&map, profile, current, proposed)
            .and_then(|target| target.grade_percent);
        let grade = target_grade.or(current.grade_percent);
        if let Some(grade) = grade {
            let Some(speed) = profile.speed_for_grade(grade) else {
                *velocity = MovementVelocity::ZERO;
                continue;
            };
            *velocity = velocity.scaled(speed.multiplier());
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

pub(crate) fn resolve_terrain_position(
    map: &WorldMap,
    traversal: &TraversalCatalog,
    character: &world01_world_data::CharacterId,
    candidate: WorldPosition,
) -> Option<WorldPosition> {
    let profile = traversal.character(character)?;
    let cell = map.terrain_cell_at(candidate.horizontal())?;
    profile
        .permits_surface(&cell.surface)
        .then(|| WorldPosition::new(candidate.x, candidate.y, cell.elevation_meters))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleSupport<'a> {
    Terrain,
    RouteSurface(&'a str),
}

impl SampleSupport<'_> {
    fn to_owned(self) -> GroundSupport {
        match self {
            Self::Terrain => GroundSupport::Terrain,
            Self::RouteSurface(route_surface_id) => GroundSupport::RouteSurface {
                route_surface_id: route_surface_id.to_owned(),
            },
        }
    }
}

impl<'a> From<&'a GroundSupport> for SampleSupport<'a> {
    fn from(support: &'a GroundSupport) -> Self {
        match support {
            GroundSupport::Terrain => Self::Terrain,
            GroundSupport::RouteSurface { route_surface_id } => {
                Self::RouteSurface(route_surface_id)
            }
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

enum CurrentSupport<'a> {
    Resolved(GroundSample<'a>),
    MissingIdentity,
    Unsupported,
}

fn resolve_current_support<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    support: &GroundSupport,
    position: WorldPosition,
) -> CurrentSupport<'a> {
    let horizontal = position.horizontal();
    let (sample, excluded_route) = match support {
        GroundSupport::Terrain => (sample_terrain(map, horizontal), None),
        GroundSupport::RouteSurface { route_surface_id } => {
            let Some(route) = map.route_surface(route_surface_id) else {
                return CurrentSupport::MissingIdentity;
            };
            (
                sample_route(route, horizontal),
                Some(route_surface_id.as_str()),
            )
        }
    };
    if let Some(sample) = sample {
        return CurrentSupport::Resolved(sample);
    }
    match resolve_detached_support(
        map,
        profile,
        position.elevation_meters,
        horizontal,
        excluded_route,
    ) {
        Some(sample) => CurrentSupport::Resolved(sample),
        None => CurrentSupport::Unsupported,
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
    match unique_reachable_route(
        map,
        profile,
        current.elevation_meters,
        position,
        excluded_route,
    ) {
        ReachableRoute::One(route) => return Some(route),
        ReachableRoute::Ambiguous => return None,
        ReachableRoute::None => {}
    }

    let terrain = sample_terrain(map, position)?;
    sample_is_reachable(profile, current.elevation_meters, terrain).then_some(terrain)
}

fn resolve_detached_support<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    position: Position,
    excluded_route: Option<&str>,
) -> Option<GroundSample<'a>> {
    match unique_reachable_route(
        map,
        profile,
        current_elevation_meters,
        position,
        excluded_route,
    ) {
        ReachableRoute::One(route) => return Some(route),
        ReachableRoute::Ambiguous => return None,
        ReachableRoute::None => {}
    }
    let terrain = sample_terrain(map, position)?;
    sample_is_reachable(profile, current_elevation_meters, terrain).then_some(terrain)
}

#[derive(Debug, Clone, Copy)]
enum ReachableRoute<'a> {
    None,
    One(GroundSample<'a>),
    Ambiguous,
}

fn unique_reachable_route<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    position: Position,
    excluded_route: Option<&str>,
) -> ReachableRoute<'a> {
    select_unique_route(map.route_surfaces().iter().filter_map(|route| {
        if excluded_route == Some(route.route_surface_id.as_str()) {
            return None;
        }
        let sample = sample_route(route, position)?;
        if !sample_is_reachable(profile, current_elevation_meters, sample) {
            return None;
        }
        Some(sample)
    }))
}

fn select_unique_route<'a>(
    candidates: impl IntoIterator<Item = GroundSample<'a>>,
) -> ReachableRoute<'a> {
    let mut candidates = candidates.into_iter();
    let Some(first) = candidates.next() else {
        return ReachableRoute::None;
    };
    if candidates.next().is_some() {
        ReachableRoute::Ambiguous
    } else {
        ReachableRoute::One(first)
    }
}

fn sample_is_reachable(
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    sample: GroundSample<'_>,
) -> bool {
    sample_is_usable(profile, sample)
        && profile.permits_step(current_elevation_meters, sample.elevation_meters)
}

fn sample_is_usable(profile: &CharacterTraversalProfile, sample: GroundSample<'_>) -> bool {
    profile.permits_surface(sample.surface)
        && sample
            .grade_percent
            .is_none_or(|grade| profile.speed_for_grade(grade).is_some())
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{App, IntoScheduleConfigs, Update, Vec2};
    use world01_configs::load_embedded as load_runtime;
    use world01_content::{
        CharacterCollisionGeometryCatalog, CharacterHurtGeometryCatalog, RuntimeContent,
    };
    use world01_design::{load_embedded as load_game_design, load_world01_embedded};
    use world01_world_data::{
        ActorId, AnkhLayout, BodyFacing, CharacterHealth, CharacterId, CharacterLifeState,
        GroundSupport, MovementMedium, RespawnState,
    };

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

    fn recovery_app() -> App {
        let runtime = load_runtime().expect("embedded runtime configuration is valid");
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let geometry = CharacterCollisionGeometryCatalog::from_content(&content)
            .expect("embedded collision geometry is valid");
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
            TraversalCatalog::from_design_and_geometry(&game_design.traversal, &geometry)
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

    #[test]
    fn multiple_reachable_routes_are_explicitly_ambiguous() {
        let samples = [
            GroundSample {
                support: SampleSupport::RouteSurface("first"),
                surface: "land",
                elevation_meters: 1.0,
                grade_percent: Some(0),
            },
            GroundSample {
                support: SampleSupport::RouteSurface("second"),
                surface: "land",
                elevation_meters: 1.0,
                grade_percent: Some(0),
            },
        ];

        assert!(matches!(
            select_unique_route(samples),
            ReachableRoute::Ambiguous
        ));
    }
}
