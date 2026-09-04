use std::{error::Error, fmt};

use bevy::{ecs::schedule::ScheduleLabel, log::error, prelude::*};
use world01_content::{RegionGeometryError, RuntimeContent, WorldCollisionGeometryCatalog};
use world01_world_data::{
    AnkhLayout, PlacementRanks, WorldComposition, WorldMap, WorldOccupancyRequest,
    WorldTemplateCatalog,
};

use crate::{SimulationSet, WorldColliderGrid};

/// The atomic world-resource transition before collision reads the new world.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorldRuntimeSet {
    Rebuild,
}

/// Tracks which composition generation the derived runtime resources represent.
///
/// The generation comparison makes repeated fixed ticks and rollback replays
/// harmless. A rejected generation is remembered so an invalid authority
/// snapshot is not rebuilt and logged every tick.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldRuntimeState {
    applied_generation: Option<u64>,
    rejected_generation: Option<u64>,
}

impl WorldRuntimeState {
    pub const fn applied_generation(self) -> Option<u64> {
        self.applied_generation
    }
}

/// Installs the shared server/client world-resource transaction.
pub fn add_world_runtime_rebuild(app: &mut App, schedule: impl ScheduleLabel + Clone) {
    app.init_resource::<WorldOccupancyRequest>()
        .init_resource::<WorldRuntimeState>()
        .add_systems(
            schedule,
            rebuild_world_runtime
                .in_set(WorldRuntimeSet::Rebuild)
                .before(SimulationSet::Collision),
        );
}

#[derive(Debug)]
enum WorldRuntimeBuildError {
    Composition(String),
    Collision(RegionGeometryError),
    MissingAnkh,
}

impl fmt::Display for WorldRuntimeBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Composition(message) => formatter.write_str(message),
            Self::Collision(error) => error.fmt(formatter),
            Self::MissingAnkh => {
                formatter.write_str("composed world requires at least one Ankh placement")
            }
        }
    }
}

impl Error for WorldRuntimeBuildError {}

struct DerivedWorldResources {
    map: WorldMap,
    collision: WorldCollisionGeometryCatalog,
    grid: WorldColliderGrid,
    ankhs: AnkhLayout,
}

impl DerivedWorldResources {
    fn build(content: &RuntimeContent, map: &WorldMap) -> Result<Self, WorldRuntimeBuildError> {
        let collision = WorldCollisionGeometryCatalog::from_content_and_map(content, map)
            .map_err(WorldRuntimeBuildError::Collision)?;
        let grid = WorldColliderGrid::from_catalog(&collision);
        let ankhs = AnkhLayout::from_map(map);
        if ankhs.positions.is_empty() {
            return Err(WorldRuntimeBuildError::MissingAnkh);
        }
        Ok(Self {
            map: map.clone(),
            collision,
            grid,
            ankhs,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_world_runtime(
    request: Res<WorldOccupancyRequest>,
    content: Res<RuntimeContent>,
    templates: Res<WorldTemplateCatalog>,
    ranks: Res<PlacementRanks>,
    mut composition: ResMut<WorldComposition>,
    mut map: ResMut<WorldMap>,
    mut collision: ResMut<WorldCollisionGeometryCatalog>,
    mut grid: ResMut<WorldColliderGrid>,
    mut ankhs: ResMut<AnkhLayout>,
    mut state: ResMut<WorldRuntimeState>,
) {
    let requested = request.latest().filter(|occupancy| {
        occupancy.generation() > composition.occupancy().generation()
            && state.rejected_generation != Some(occupancy.generation())
    });

    let mut candidate = None;
    let target_generation = if let Some(occupancy) = requested {
        let mut next = composition.clone();
        if let Err(error) = next.apply_newer_occupancy(occupancy.clone(), &templates, &ranks) {
            reject_generation(
                occupancy.generation(),
                WorldRuntimeBuildError::Composition(error.to_string()),
                &mut state,
            );
            return;
        }
        let generation = next.occupancy().generation();
        candidate = Some(next);
        generation
    } else {
        composition.occupancy().generation()
    };

    if state.applied_generation == Some(target_generation)
        || state.rejected_generation == Some(target_generation)
    {
        return;
    }

    let source = candidate.as_ref().unwrap_or(&composition);
    let derived = match DerivedWorldResources::build(&content, source.current_map()) {
        Ok(derived) => derived,
        Err(error) => {
            reject_generation(target_generation, error, &mut state);
            return;
        }
    };

    if let Some(candidate) = candidate {
        *composition = candidate;
    }
    *map = derived.map;
    *collision = derived.collision;
    *grid = derived.grid;
    *ankhs = derived.ankhs;
    state.applied_generation = Some(target_generation);
    state.rejected_generation = None;
}

fn reject_generation(
    generation: u64,
    build_error: WorldRuntimeBuildError,
    state: &mut WorldRuntimeState,
) {
    error!(%build_error, generation, "cannot apply composed world runtime");
    state.rejected_generation = Some(generation);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorldSeparationStep, separate_characters_from_world};
    use world01_configs::load_embedded;
    use world01_content::CharacterCollisionGeometryCatalog;
    use world01_design::load_world01_embedded;
    use world01_world_data::{BodyFacing, CharacterId, SelectedCharacter, WorldPosition};

    #[derive(Resource, Debug, Default)]
    struct MapChangeCount(u32);

    #[derive(Resource, Debug, Default)]
    struct RuntimeStateChangeCount(u32);

    fn count_map_changes(map: Res<WorldMap>, mut count: ResMut<MapChangeCount>) {
        if map.is_changed() {
            count.0 += 1;
        }
    }

    fn count_runtime_state_changes(
        state: Res<WorldRuntimeState>,
        mut count: ResMut<RuntimeStateChangeCount>,
    ) {
        if state.is_changed() {
            count.0 += 1;
        }
    }

    struct EmbeddedWorld {
        content: RuntimeContent,
        templates: WorldTemplateCatalog,
        ranks: PlacementRanks,
        composition: WorldComposition,
        map: WorldMap,
        collision: WorldCollisionGeometryCatalog,
        grid: WorldColliderGrid,
        ankhs: AnkhLayout,
    }

    fn embedded_world() -> EmbeddedWorld {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("embedded Templates are valid");
        let ranks = load_world01_embedded()
            .expect("embedded world design parses")
            .placement_ranks()
            .expect("embedded Placement Ranks are valid");
        let map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        let composition = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("startup composition is valid");
        let collision = WorldCollisionGeometryCatalog::from_content_and_map(&content, &map)
            .expect("startup collision geometry is valid");
        let grid = WorldColliderGrid::from_catalog(&collision);
        let ankhs = AnkhLayout::from_map(&map);
        EmbeddedWorld {
            content,
            templates,
            ranks,
            composition,
            map,
            collision,
            grid,
            ankhs,
        }
    }

    fn app_with_world(world: EmbeddedWorld) -> App {
        let mut app = App::new();
        app.insert_resource(world.content)
            .insert_resource(world.templates)
            .insert_resource(world.ranks)
            .insert_resource(world.composition)
            .insert_resource(world.map)
            .insert_resource(world.collision)
            .insert_resource(world.grid)
            .insert_resource(world.ankhs)
            .init_resource::<WorldOccupancyRequest>()
            .init_resource::<WorldRuntimeState>();
        app
    }

    #[test]
    fn rebuild_is_atomic_generation_bound_and_restores_the_base_world() {
        let world = embedded_world();
        let initial_map = world.map.clone();
        let initial_collision = world.collision.clone();
        let initial_grid = world.grid.clone();
        let initial_ankhs = world.ankhs.clone();
        let mut authority = world.composition.clone();
        let templates = world.templates.clone();
        let ranks = world.ranks.clone();
        let content = world.content.clone();
        let mut app = app_with_world(world);
        app.init_resource::<MapChangeCount>().add_systems(
            Update,
            (
                rebuild_world_runtime,
                count_map_changes.after(rebuild_world_runtime),
            ),
        );

        app.update();
        assert_eq!(app.world().resource::<MapChangeCount>().0, 1);

        authority
            .set_occupant("template_anchor_002", "test_template02", &templates, &ranks)
            .expect("the authority assignment is valid");
        app.world_mut()
            .resource_mut::<WorldOccupancyRequest>()
            .submit(authority.occupancy().clone());
        app.update();

        let expected_map = authority.current_map();
        let expected_collision =
            WorldCollisionGeometryCatalog::from_content_and_map(&content, expected_map)
                .expect("composed collision geometry is valid");
        let expected_grid = WorldColliderGrid::from_catalog(&expected_collision);
        assert_eq!(app.world().resource::<WorldMap>(), expected_map);
        assert_eq!(
            app.world().resource::<WorldCollisionGeometryCatalog>(),
            &expected_collision
        );
        assert_eq!(app.world().resource::<WorldColliderGrid>(), &expected_grid);
        assert_eq!(
            app.world().resource::<AnkhLayout>(),
            &AnkhLayout::from_map(expected_map)
        );
        assert_eq!(
            app.world()
                .resource::<WorldRuntimeState>()
                .applied_generation(),
            Some(1)
        );
        assert_eq!(app.world().resource::<MapChangeCount>().0, 2);

        app.update();
        assert_eq!(app.world().resource::<MapChangeCount>().0, 2);

        authority
            .clear_occupant("template_anchor_002", &templates, &ranks)
            .expect("clearing the assignment is valid");
        app.world_mut()
            .resource_mut::<WorldOccupancyRequest>()
            .submit(authority.occupancy().clone());
        app.update();

        assert_eq!(app.world().resource::<WorldMap>(), &initial_map);
        assert_eq!(
            app.world().resource::<WorldCollisionGeometryCatalog>(),
            &initial_collision
        );
        assert_eq!(app.world().resource::<WorldColliderGrid>(), &initial_grid);
        assert_eq!(app.world().resource::<AnkhLayout>(), &initial_ankhs);
        assert_eq!(app.world().resource::<MapChangeCount>().0, 3);
        assert_eq!(
            app.world()
                .resource::<WorldRuntimeState>()
                .applied_generation(),
            Some(2)
        );
    }

    #[test]
    fn a_generation_without_an_ankh_is_rejected_without_partial_changes() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("embedded Templates are valid");
        let ranks = PlacementRanks::from_entries([("grass", 200), ("tree", 20), ("ankh", 100)])
            .expect("test Placement Ranks are valid");
        let map = WorldMap::from_source(&single_ankh_instance(), "runtime_test")
            .expect("the synthetic Instance is valid");
        let mut authority = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the initial composition is valid");
        authority
            .set_occupant("template_anchor_001", "test_template", &templates, &ranks)
            .expect("the high-rank Template assignment composes");
        assert!(
            AnkhLayout::from_map(authority.current_map())
                .positions
                .is_empty(),
            "remaining props: {:?}",
            authority.current_map().props()
        );

        let collision = WorldCollisionGeometryCatalog::from_content_and_map(&content, &map)
            .expect("initial collision geometry is valid");
        let grid = WorldColliderGrid::from_catalog(&collision);
        let ankhs = AnkhLayout::from_map(&map);
        let initial_composition = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the replica composition is valid");
        let mut app = App::new();
        app.insert_resource(content)
            .insert_resource(templates)
            .insert_resource(ranks)
            .insert_resource(initial_composition.clone())
            .insert_resource(map.clone())
            .insert_resource(collision.clone())
            .insert_resource(grid.clone())
            .insert_resource(ankhs.clone())
            .init_resource::<WorldOccupancyRequest>()
            .init_resource::<WorldRuntimeState>()
            .init_resource::<RuntimeStateChangeCount>()
            .add_systems(
                Update,
                (
                    rebuild_world_runtime,
                    count_runtime_state_changes.after(rebuild_world_runtime),
                ),
            );

        app.update();
        assert_eq!(app.world().resource::<RuntimeStateChangeCount>().0, 1);
        app.world_mut()
            .resource_mut::<WorldOccupancyRequest>()
            .submit(authority.occupancy().clone());
        app.update();

        assert_eq!(
            app.world().resource::<WorldComposition>(),
            &initial_composition
        );
        assert_eq!(app.world().resource::<WorldMap>(), &map);
        assert_eq!(
            app.world().resource::<WorldCollisionGeometryCatalog>(),
            &collision
        );
        assert_eq!(app.world().resource::<WorldColliderGrid>(), &grid);
        assert_eq!(app.world().resource::<AnkhLayout>(), &ankhs);
        assert_eq!(
            app.world()
                .resource::<WorldRuntimeState>()
                .applied_generation(),
            Some(0)
        );
        assert_eq!(
            app.world()
                .resource::<WorldRuntimeState>()
                .rejected_generation,
            Some(1)
        );
        assert_eq!(app.world().resource::<RuntimeStateChangeCount>().0, 2);

        app.update();
        assert_eq!(
            app.world().resource::<WorldComposition>(),
            &initial_composition
        );
        assert_eq!(app.world().resource::<WorldMap>(), &map);
        assert_eq!(app.world().resource::<RuntimeStateChangeCount>().0, 2);
    }

    #[test]
    fn newly_composed_collision_separates_an_overlapping_character() {
        let world = embedded_world();
        let templates = world.templates.clone();
        let ranks = world.ranks.clone();
        let initial_regions = world
            .collision
            .regions
            .iter()
            .map(|region| (region.instance_id.clone(), region.position))
            .collect::<Vec<_>>();
        let mut authority = world.composition.clone();
        authority
            .set_occupant("template_anchor_002", "test_template02", &templates, &ranks)
            .expect("the authority assignment is valid");
        let composed_collision = WorldCollisionGeometryCatalog::from_content_and_map(
            &world.content,
            authority.current_map(),
        )
        .expect("composed collision geometry is valid");
        let new_region = composed_collision
            .regions
            .iter()
            .find(|region| {
                !initial_regions.iter().any(|(instance_id, position)| {
                    instance_id == &region.instance_id && position == &region.position
                })
            })
            .expect("the Template adds collision geometry");
        let start = WorldPosition::new(new_region.position.x, new_region.position.y + 0.5, 1.0);

        let collision_geometry = CharacterCollisionGeometryCatalog::from_content(&world.content)
            .expect("embedded Character collision geometry is valid");
        let separation_step = WorldSeparationStep::from_runtime(
            &load_embedded().expect("embedded runtime configuration parses"),
        )
        .expect("embedded world separation configuration is valid");
        let mut app = app_with_world(world);
        app.insert_resource(collision_geometry)
            .insert_resource(separation_step)
            .add_systems(
                Update,
                separate_characters_from_world.in_set(SimulationSet::Collision),
            );
        add_world_runtime_rebuild(&mut app, Update);
        let actor = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                BodyFacing::Authored,
                start,
            ))
            .id();
        app.world_mut()
            .resource_mut::<WorldOccupancyRequest>()
            .submit(authority.occupancy().clone());

        for _ in 0..80 {
            app.update();
        }
        let separated = *app
            .world()
            .get::<WorldPosition>(actor)
            .expect("the Character keeps a position");
        assert_ne!(separated, start);
        app.update();
        assert_eq!(app.world().get::<WorldPosition>(actor), Some(&separated));
    }

    fn single_ankh_instance() -> String {
        r#"{
            "format": "scene_maker_scene_export",
            "version": 10,
            "workspace_key": "world01",
            "grid": {
                "terrain_cell_meters": 1.0,
                "authoring_pixels_per_meter": 32.0,
                "game_pixels_per_meter": 192.0,
                "water_cell_meters": 0.5
            },
            "asset_profiles": [
                { "asset_key": "grass", "surface": "land", "footprint_meters": null, "anchor_meters": null },
                { "asset_key": "ankh", "surface": null, "footprint_meters": { "width": 1.0625, "height": 1.71875 }, "anchor_meters": { "x": 0.53125, "y": 0.3125 } },
                { "asset_key": "tree", "surface": null, "footprint_meters": { "width": 6.4375, "height": 8.0625 }, "anchor_meters": { "x": 3.21875, "y": 0.03125 } }
            ],
            "water_raster": [],
            "route_surface_bakes": [],
            "scene": {
                "schema": "srt.scene_maker_scene",
                "version": 11,
                "scene_id": "runtime_test",
                "scene_kind": "instance",
                "size_cells": { "width": 16, "height": 16 },
                "coordinate_space": "scene_local_bottom_left_y_up",
                "terrain_cells": [
                    { "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }
                ],
                "props": [{
                    "instance_id": "ankh_only",
                    "asset_key": "ankh",
                    "position_authoring_px": { "x": 288, "y": 256 },
                    "elevation_meters": 1.0
                }],
                "water_bodies": [],
                "route_surfaces": [],
                "template_definition": null,
                "template_anchors": [{
                    "anchor_id": "template_anchor_001",
                    "group_number": 1,
                    "position_authoring_px": { "x": 256, "y": 256 }
                }],
                "default_elevation_meters": 1.0
            }
        }"#
        .to_owned()
    }
}
