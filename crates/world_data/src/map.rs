use std::{collections::HashSet, error::Error, fmt};

use bevy::prelude::Resource;
use serde::Deserialize;

use crate::Position;

const FORMAT: &str = "scene_maker_scene_export";
const FORMAT_VERSION: u32 = 1;
const SCENE_SCHEMA: &str = "srt.scene_maker_scene";
const SCENE_VERSION: u32 = 5;
const WORKSPACE_KEY: &str = "world01";
const SCENE_ID: &str = "world01";
const COORDINATE_SPACE: &str = "scene_local_bottom_left_y_up";

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct WorldMap {
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    terrain_cells: Vec<MapTerrainCell>,
    placements: Vec<MapPlacement>,
    transitions: Vec<MapPlacement>,
}

impl WorldMap {
    pub fn load_embedded() -> Result<Self, WorldMapError> {
        Self::from_source(include_str!(
            "../../../assets/maps/world01.scene_export.json"
        ))
    }

    pub fn from_source(source: &str) -> Result<Self, WorldMapError> {
        let export: ExportDocument = serde_json::from_str(source)
            .map_err(|error| WorldMapError::new(format!("cannot parse SceneMaker map: {error}")))?;
        validate_header(&export)?;

        let width_tiles = export.scene.size_cells.width;
        let height_tiles = export.scene.size_cells.height;
        if width_tiles == 0 || height_tiles == 0 {
            return Err(WorldMapError::new(
                "map dimensions must be greater than zero",
            ));
        }
        let terrain_cell_meters = export.grid.terrain_cell_meters;
        let authoring_pixels_per_meter = export.grid.authoring_pixels_per_meter;
        if !terrain_cell_meters.is_finite() || terrain_cell_meters <= 0.0 {
            return Err(WorldMapError::new(
                "terrain_cell_meters must be finite and greater than zero",
            ));
        }
        if !authoring_pixels_per_meter.is_finite() || authoring_pixels_per_meter <= 0.0 {
            return Err(WorldMapError::new(
                "authoring_pixels_per_meter must be finite and greater than zero",
            ));
        }
        if !export.grid.game_pixels_per_meter.is_finite()
            || export.grid.game_pixels_per_meter <= 0.0
        {
            return Err(WorldMapError::new(
                "game_pixels_per_meter must be finite and greater than zero",
            ));
        }

        let profile_keys = export
            .asset_profiles
            .into_iter()
            .map(|profile| profile.asset_key)
            .collect::<HashSet<_>>();
        let half_width = width_tiles as f32 * terrain_cell_meters * 0.5;
        let half_height = height_tiles as f32 * terrain_cell_meters * 0.5;
        let mut occupied_cells = HashSet::new();
        let mut terrain_cells = Vec::with_capacity(export.scene.terrain_cells.len());
        for cell in export.scene.terrain_cells {
            if cell.x >= width_tiles || cell.y >= height_tiles {
                return Err(WorldMapError::new(format!(
                    "terrain cell ({}, {}) lies outside the map",
                    cell.x, cell.y
                )));
            }
            if !occupied_cells.insert((cell.x, cell.y)) {
                return Err(WorldMapError::new(format!(
                    "terrain cell ({}, {}) is duplicated",
                    cell.x, cell.y
                )));
            }
            require_profile(&profile_keys, &cell.asset_key)?;
            terrain_cells.push(MapTerrainCell {
                x: cell.x,
                y: cell.y,
                asset_key: cell.asset_key,
                center: Position::new(
                    (cell.x as f32 + 0.5) * terrain_cell_meters - half_width,
                    (cell.y as f32 + 0.5) * terrain_cell_meters - half_height,
                ),
            });
        }

        let mut instance_ids = HashSet::new();
        let placements = convert_placements(
            export.scene.placements,
            &profile_keys,
            &mut instance_ids,
            authoring_pixels_per_meter,
            half_width,
            half_height,
        )?;
        let transitions = convert_placements(
            export.scene.transitions,
            &profile_keys,
            &mut instance_ids,
            authoring_pixels_per_meter,
            half_width,
            half_height,
        )?;

        Ok(Self {
            width_tiles,
            height_tiles,
            terrain_cell_meters,
            terrain_cells,
            placements,
            transitions,
        })
    }

    pub const fn width_tiles(&self) -> u32 {
        self.width_tiles
    }

    pub const fn height_tiles(&self) -> u32 {
        self.height_tiles
    }

    pub const fn terrain_cell_meters(&self) -> f32 {
        self.terrain_cell_meters
    }

    pub fn width_meters(&self) -> f32 {
        self.width_tiles as f32 * self.terrain_cell_meters
    }

    pub fn height_meters(&self) -> f32 {
        self.height_tiles as f32 * self.terrain_cell_meters
    }

    pub fn terrain_cells(&self) -> &[MapTerrainCell] {
        &self.terrain_cells
    }

    pub fn placements(&self) -> &[MapPlacement] {
        &self.placements
    }

    pub fn transitions(&self) -> &[MapPlacement] {
        &self.transitions
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapTerrainCell {
    pub x: u32,
    pub y: u32,
    pub asset_key: String,
    pub center: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapPlacement {
    pub instance_id: String,
    pub asset_key: String,
    pub position: Position,
}

fn validate_header(export: &ExportDocument) -> Result<(), WorldMapError> {
    let scene = &export.scene;
    if export.format != FORMAT
        || export.version != FORMAT_VERSION
        || export.workspace_key != WORKSPACE_KEY
        || scene.schema != SCENE_SCHEMA
        || scene.version != SCENE_VERSION
        || scene.scene_id != SCENE_ID
        || scene.scene_kind != "instance"
        || scene.coordinate_space != COORDINATE_SPACE
    {
        return Err(WorldMapError::new(
            "SceneMaker map header does not match the world01 import contract",
        ));
    }
    Ok(())
}

fn require_profile(profile_keys: &HashSet<String>, asset_key: &str) -> Result<(), WorldMapError> {
    if asset_key.is_empty() || !profile_keys.contains(asset_key) {
        return Err(WorldMapError::new(format!(
            "map asset '{asset_key}' has no exported asset profile"
        )));
    }
    Ok(())
}

fn convert_placements(
    source: Vec<PlacementDocument>,
    profile_keys: &HashSet<String>,
    instance_ids: &mut HashSet<String>,
    authoring_pixels_per_meter: f32,
    half_width: f32,
    half_height: f32,
) -> Result<Vec<MapPlacement>, WorldMapError> {
    source
        .into_iter()
        .map(|placement| {
            require_profile(profile_keys, &placement.asset_key)?;
            if placement.instance_id.is_empty()
                || !instance_ids.insert(placement.instance_id.clone())
            {
                return Err(WorldMapError::new(format!(
                    "map instance ID '{}' is empty or duplicated",
                    placement.instance_id
                )));
            }
            let position = Position::new(
                placement.position_authoring_px.x as f32 / authoring_pixels_per_meter - half_width,
                placement.position_authoring_px.y as f32 / authoring_pixels_per_meter - half_height,
            );
            if !position.x.is_finite() || !position.y.is_finite() {
                return Err(WorldMapError::new(format!(
                    "map instance '{}' has a non-finite position",
                    placement.instance_id
                )));
            }
            Ok(MapPlacement {
                instance_id: placement.instance_id,
                asset_key: placement.asset_key,
                position,
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct WorldMapError(String);

impl WorldMapError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for WorldMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for WorldMapError {}

#[derive(Deserialize)]
struct ExportDocument {
    format: String,
    version: u32,
    workspace_key: String,
    grid: GridDocument,
    asset_profiles: Vec<AssetProfileDocument>,
    scene: SceneDocument,
}

#[derive(Deserialize)]
struct GridDocument {
    terrain_cell_meters: f32,
    authoring_pixels_per_meter: f32,
    game_pixels_per_meter: f32,
}

#[derive(Deserialize)]
struct AssetProfileDocument {
    asset_key: String,
}

#[derive(Deserialize)]
struct SceneDocument {
    schema: String,
    version: u32,
    scene_id: String,
    scene_kind: String,
    size_cells: SizeDocument,
    coordinate_space: String,
    terrain_cells: Vec<TerrainCellDocument>,
    placements: Vec<PlacementDocument>,
    transitions: Vec<PlacementDocument>,
}

#[derive(Deserialize)]
struct SizeDocument {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct TerrainCellDocument {
    x: u32,
    y: u32,
    asset_key: String,
}

#[derive(Deserialize)]
struct PlacementDocument {
    instance_id: String,
    asset_key: String,
    position_authoring_px: PointDocument,
}

#[derive(Deserialize)]
struct PointDocument {
    x: i32,
    y: i32,
}

#[cfg(test)]
pub(crate) const TEST_GRASS_CELL: &str = r#"{ "x": 0, "y": 0, "asset_key": "grass" }"#;

/// Builds a four-by-four export that satisfies the import contract.
///
/// Tests pin conversion and validation against this instead of the authored
/// scene, which is edited by hand and must stay free to change.
#[cfg(test)]
pub(crate) fn test_export(terrain_cells: &str, placements: &str) -> String {
    format!(
        r#"{{
            "format": "{FORMAT}",
            "version": {FORMAT_VERSION},
            "workspace_key": "{WORKSPACE_KEY}",
            "grid": {{
                "terrain_cell_meters": 1.0,
                "authoring_pixels_per_meter": 32.0,
                "game_pixels_per_meter": 192.0
            }},
            "asset_profiles": [
                {{ "asset_key": "grass" }},
                {{ "asset_key": "ankh" }},
                {{ "asset_key": "tree" }}
            ],
            "scene": {{
                "schema": "{SCENE_SCHEMA}",
                "version": {SCENE_VERSION},
                "scene_id": "{SCENE_ID}",
                "scene_kind": "instance",
                "size_cells": {{ "width": 4, "height": 4 }},
                "coordinate_space": "{COORDINATE_SPACE}",
                "terrain_cells": [{terrain_cells}],
                "placements": [{placements}],
                "transitions": []
            }}
        }}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANKH: &str = r#"{
        "instance_id": "ankh_0001",
        "asset_key": "ankh",
        "position_authoring_px": { "x": 64, "y": 96 }
    }"#;

    /// The server refuses to start without an Ankh, so that much must hold for
    /// whatever scene is currently authored.
    #[test]
    fn the_embedded_scene_still_imports() {
        let map = WorldMap::load_embedded().expect("embedded SceneMaker map is valid");

        assert_eq!((map.width_tiles(), map.height_tiles()), (100, 100));
        assert!(!map.terrain_cells().is_empty());
        assert!(map.terrain_cells().len() <= 100 * 100);
        assert!(
            map.placements()
                .iter()
                .any(|placement| placement.asset_key == "ankh")
        );
    }

    #[test]
    fn authoring_pixels_become_positions_around_the_map_centre() {
        let source = test_export(TEST_GRASS_CELL, ANKH);
        let map = WorldMap::from_source(&source).expect("the synthetic export is valid");

        assert_eq!(map.terrain_cells()[0].center, Position::new(-1.5, -1.5));
        assert_eq!(map.placements()[0].position, Position::new(0.0, 1.0));
    }

    #[test]
    fn cells_outside_the_map_or_without_a_profile_are_rejected() {
        let outside = test_export(r#"{ "x": 9, "y": 0, "asset_key": "grass" }"#, "");
        let unprofiled = test_export(r#"{ "x": 0, "y": 0, "asset_key": "lava" }"#, "");

        assert!(WorldMap::from_source(&outside).is_err());
        assert!(WorldMap::from_source(&unprofiled).is_err());
    }

    #[test]
    fn duplicated_cells_and_instance_ids_are_rejected() {
        let twice = format!("{TEST_GRASS_CELL}, {TEST_GRASS_CELL}");
        let duplicated_cell = test_export(&twice, "");
        let duplicated_id = test_export(TEST_GRASS_CELL, &format!("{ANKH}, {ANKH}"));

        assert!(WorldMap::from_source(&duplicated_cell).is_err());
        assert!(WorldMap::from_source(&duplicated_id).is_err());
    }
}
