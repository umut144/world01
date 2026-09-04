use bevy::prelude::{Query, Res};
use world01_world_data::{
    GroundSupport, MovementMedium, MovementVelocity, Position, SelectedCharacter, WorldMap,
    WorldPosition,
};

use crate::MovementStep;

use super::TraversalCatalog;

/// Constrains the already collision-shortened horizontal step to the Actor's
/// authored ground surface before movement integration applies it.
///
/// Terrain is the first supported ground type. Route surfaces are explicit in
/// the protocol now, but remain stationary until their height/topology resolver
/// is introduced by the next SBX-36 slice.
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
        let GroundSupport::Terrain = support else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        let Some(profile) = traversal.character(&character.0) else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        let Some(current_cell) = map.terrain_cell_at(position.horizontal()) else {
            *velocity = MovementVelocity::ZERO;
            *medium = MovementMedium::Airborne;
            continue;
        };
        if !profile.permits_surface(&current_cell.surface) {
            *velocity = MovementVelocity::ZERO;
            *medium = MovementMedium::Airborne;
            continue;
        }

        // A stationary Actor follows an edited support as well. Template
        // changes therefore cannot leave Grounded state at a stale elevation.
        position.elevation_meters = current_cell.elevation_meters;
        if *velocity == MovementVelocity::ZERO {
            continue;
        }

        let displacement = step.step(*velocity);
        let proposed = Position::new(position.x + displacement.x, position.y + displacement.y);
        let Some(target_cell) = map.terrain_cell_at(proposed) else {
            *velocity = MovementVelocity::ZERO;
            continue;
        };
        if !profile.permits_surface(&target_cell.surface)
            || !profile.permits_step(current_cell.elevation_meters, target_cell.elevation_meters)
        {
            *velocity = MovementVelocity::ZERO;
            continue;
        }
        position.elevation_meters = target_cell.elevation_meters;
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{App, Update, Vec2};
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
        .add_systems(Update, constrain_grounded_movement);
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
}
