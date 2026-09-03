use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
use serde::Deserialize;

use crate::Position;

const FORMAT: &str = "scene_maker_scene_export";
const FORMAT_VERSION: u32 = 9;
const SCENE_SCHEMA: &str = "srt.scene_maker_scene";
const SCENE_VERSION: u32 = 10;
const WORKSPACE_KEY: &str = "world01";
const COORDINATE_SPACE: &str = "scene_local_bottom_left_y_up";

include!(concat!(env!("OUT_DIR"), "/embedded_world_exports.rs"));

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct WorldMap {
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    terrain_cells: Vec<MapTerrainCell>,
    props: Vec<MapProp>,
    template_anchors: Vec<MapTemplateAnchor>,
}

impl WorldMap {
    /// Loads the embedded Instance named `scene_id`.
    ///
    /// Every synchronized SceneMaker export is embedded, including Templates,
    /// but only an explicitly requested Instance may become a `WorldMap`.
    pub fn load_embedded(scene_id: &str) -> Result<Self, WorldMapError> {
        Self::from_source(embedded_instance_source(scene_id)?, scene_id)
    }

    pub fn from_source(source: &str, scene_id: &str) -> Result<Self, WorldMapError> {
        let export: ExportDocument = serde_json::from_str(source)
            .map_err(|error| WorldMapError::new(format!("cannot parse SceneMaker map: {error}")))?;
        validate_header(&export, scene_id)?;

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
        if !export.grid.water_cell_meters.is_finite() || export.grid.water_cell_meters <= 0.0 {
            return Err(WorldMapError::new(
                "water_cell_meters must be finite and greater than zero",
            ));
        }

        let surfaces = export
            .asset_profiles
            .into_iter()
            .map(|profile| (profile.asset_key, profile.surface))
            .collect::<HashMap<_, _>>();
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
            let Some(surface) = require_profile(&surfaces, &cell.asset_key)?.clone() else {
                return Err(WorldMapError::new(format!(
                    "terrain asset '{}' has no exported surface",
                    cell.asset_key
                )));
            };
            if !cell.elevation_meters.is_finite() {
                return Err(WorldMapError::new(format!(
                    "terrain cell ({}, {}) has a non-finite elevation",
                    cell.x, cell.y
                )));
            }
            terrain_cells.push(MapTerrainCell {
                x: cell.x,
                y: cell.y,
                asset_key: cell.asset_key,
                surface,
                elevation_meters: cell.elevation_meters,
                center: Position::new(
                    (cell.x as f32 + 0.5) * terrain_cell_meters - half_width,
                    (cell.y as f32 + 0.5) * terrain_cell_meters - half_height,
                ),
            });
        }

        let mut instance_ids = HashSet::new();
        let props = convert_props(
            export.scene.props,
            &surfaces,
            &mut instance_ids,
            authoring_pixels_per_meter,
            half_width,
            half_height,
        )?;
        let template_anchors = export
            .scene
            .template_anchors
            .into_iter()
            .map(|anchor| MapTemplateAnchor {
                anchor_id: anchor.anchor_id,
                group_number: anchor.group_number,
                position: Position::new(
                    anchor.position_authoring_px.x as f32 / authoring_pixels_per_meter - half_width,
                    anchor.position_authoring_px.y as f32 / authoring_pixels_per_meter
                        - half_height,
                ),
            })
            .collect();

        Ok(Self {
            width_tiles,
            height_tiles,
            terrain_cell_meters,
            terrain_cells,
            props,
            template_anchors,
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

    pub fn props(&self) -> &[MapProp] {
        &self.props
    }

    /// Every place a Template may be put, with the group it asks for.
    ///
    /// Placement itself is not implemented: the map loads with its Anchors
    /// empty, so it is missing whatever would stand there.
    pub fn template_anchors(&self) -> &[MapTemplateAnchor] {
        &self.template_anchors
    }
}

fn embedded_instance_source(scene_id: &str) -> Result<&'static str, WorldMapError> {
    let mut matched = None;
    for (file_name, source) in EMBEDDED_WORLD_EXPORTS {
        let export: ExportDocument = serde_json::from_str(source).map_err(|error| {
            WorldMapError::new(format!(
                "cannot parse embedded SceneMaker export '{file_name}': {error}"
            ))
        })?;
        if export.scene.scene_id != scene_id {
            continue;
        }
        if matched.is_some() {
            return Err(WorldMapError::new(format!(
                "embedded SceneMaker scene ID '{scene_id}' is duplicated"
            )));
        }
        matched = Some((export.scene.scene_kind, *source));
    }

    let Some((scene_kind, source)) = matched else {
        return Err(WorldMapError::new(format!(
            "SceneMaker Instance '{scene_id}' is not embedded"
        )));
    };
    if scene_kind != "instance" {
        return Err(WorldMapError::new(format!(
            "SceneMaker scene '{scene_id}' is a '{scene_kind}', not an Instance"
        )));
    }
    Ok(source)
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapTerrainCell {
    pub x: u32,
    pub y: u32,
    pub asset_key: String,
    /// The domain this cell presents, joined from the cell's Asset profile.
    ///
    /// An open token - `land`, `water`, whatever is authored next - because
    /// SceneMaker checks its shape and never its meaning. What may cross it is
    /// decided here, by intersecting it with the Actor's own domains.
    pub surface: String,
    /// The height of the walking surface, in meters.
    pub elevation_meters: f32,
    pub center: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapProp {
    pub instance_id: String,
    pub asset_key: String,
    pub position: Position,
    /// The height this Prop stands at, usually the Terrain under it.
    ///
    /// Separate from the Terrain's own height because a bridge deck sits above
    /// the water it crosses: over a river there are two surfaces, and which one
    /// an Actor uses is its domain's decision, not the map's.
    pub elevation_meters: f32,
}

/// A place where a Template of `group_number` belongs.
#[derive(Debug, Clone, PartialEq)]
pub struct MapTemplateAnchor {
    pub anchor_id: String,
    pub group_number: u32,
    pub position: Position,
}

fn validate_header(export: &ExportDocument, scene_id: &str) -> Result<(), WorldMapError> {
    let scene = &export.scene;
    if scene.scene_id != scene_id {
        return Err(WorldMapError::new(format!(
            "requested map '{scene_id}', but this document is '{}'",
            scene.scene_id
        )));
    }
    if export.format != FORMAT
        || export.version != FORMAT_VERSION
        || export.workspace_key != WORKSPACE_KEY
        || scene.schema != SCENE_SCHEMA
        || scene.version != SCENE_VERSION
        || scene.scene_kind != "instance"
        || scene.coordinate_space != COORDINATE_SPACE
    {
        return Err(WorldMapError::new(
            "SceneMaker map header does not match the world01 import contract",
        ));
    }
    Ok(())
}

fn require_profile<'a>(
    surfaces: &'a HashMap<String, Option<String>>,
    asset_key: &str,
) -> Result<&'a Option<String>, WorldMapError> {
    if asset_key.is_empty() {
        return Err(WorldMapError::new("map asset key must not be empty"));
    }
    surfaces.get(asset_key).ok_or_else(|| {
        WorldMapError::new(format!(
            "map asset '{asset_key}' has no exported asset profile"
        ))
    })
}

fn convert_props(
    source: Vec<PropDocument>,
    surfaces: &HashMap<String, Option<String>>,
    instance_ids: &mut HashSet<String>,
    authoring_pixels_per_meter: f32,
    half_width: f32,
    half_height: f32,
) -> Result<Vec<MapProp>, WorldMapError> {
    source
        .into_iter()
        .map(|placement| {
            require_profile(surfaces, &placement.asset_key)?;
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
            if !position.x.is_finite()
                || !position.y.is_finite()
                || !placement.elevation_meters.is_finite()
            {
                return Err(WorldMapError::new(format!(
                    "map instance '{}' has a non-finite position or elevation",
                    placement.instance_id
                )));
            }
            Ok(MapProp {
                instance_id: placement.instance_id,
                asset_key: placement.asset_key,
                position,
                elevation_meters: placement.elevation_meters,
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
    #[serde(rename = "water_raster")]
    _water_raster: Vec<serde_json::Value>,
    scene: SceneDocument,
}

#[derive(Deserialize)]
struct GridDocument {
    terrain_cell_meters: f32,
    authoring_pixels_per_meter: f32,
    game_pixels_per_meter: f32,
    water_cell_meters: f32,
}

#[derive(Deserialize)]
struct AssetProfileDocument {
    asset_key: String,
    /// Set for Terrain Assets and null for every other kind, which is why a
    /// cell painted with an Asset that has none is a corrupt export.
    surface: Option<String>,
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
    props: Vec<PropDocument>,
    #[serde(rename = "water_bodies")]
    _water_bodies: Vec<serde_json::Value>,
    template_anchors: Vec<TemplateAnchorDocument>,
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
    elevation_meters: f32,
}

#[derive(Deserialize)]
struct PropDocument {
    instance_id: String,
    asset_key: String,
    position_authoring_px: PointDocument,
    elevation_meters: f32,
}

#[derive(Deserialize)]
struct TemplateAnchorDocument {
    anchor_id: String,
    group_number: u32,
    position_authoring_px: PointDocument,
}

#[derive(Deserialize)]
struct PointDocument {
    x: i32,
    y: i32,
}

#[cfg(test)]
pub(crate) const TEST_SCENE_ID: &str = "overworld01";

#[cfg(test)]
pub(crate) const TEST_GRASS_CELL: &str =
    r#"{ "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }"#;

/// Builds a four-by-four export that satisfies the current import contract.
///
/// Tests pin conversion and validation against this instead of the authored
/// scene, which is edited by hand and must stay free to change.
#[cfg(test)]
pub(crate) fn test_export(terrain_cells: &str, props: &str) -> String {
    test_export_with_anchors(terrain_cells, props, "")
}

#[cfg(test)]
fn test_export_with_anchors(terrain_cells: &str, props: &str, anchors: &str) -> String {
    export_document(FORMAT_VERSION, SCENE_VERSION, terrain_cells, props, anchors)
}

#[cfg(test)]
fn export_document(
    version: u32,
    scene_version: u32,
    terrain_cells: &str,
    props: &str,
    anchors: &str,
) -> String {
    format!(
        r#"{{
            "format": "{FORMAT}",
            "version": {version},
            "workspace_key": "{WORKSPACE_KEY}",
            "grid": {{
                "terrain_cell_meters": 1.0,
                "authoring_pixels_per_meter": 32.0,
                "game_pixels_per_meter": 192.0,
                "water_cell_meters": 0.5
            }},
            "asset_profiles": [
                {{ "asset_key": "grass", "surface": "land" }},
                {{ "asset_key": "ankh", "surface": null }},
                {{ "asset_key": "tree", "surface": null }}
            ],
            "water_raster": [],
            "scene": {{
                "schema": "{SCENE_SCHEMA}",
                "version": {scene_version},
                "scene_id": "{TEST_SCENE_ID}",
                "scene_kind": "instance",
                "size_cells": {{ "width": 4, "height": 4 }},
                "coordinate_space": "{COORDINATE_SPACE}",
                "terrain_cells": [{terrain_cells}],
                "props": [{props}],
                "water_bodies": [],
                "template_anchors": [{anchors}],
                "default_elevation_meters": 1.0
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
        "position_authoring_px": { "x": 64, "y": 96 },
        "elevation_meters": 1.0
    }"#;

    const ANCHOR: &str = r#"{
        "anchor_id": "template_anchor_001",
        "group_number": 1,
        "position_authoring_px": { "x": 64, "y": 96 }
    }"#;

    /// The server refuses to start without an Ankh, so that much must hold for
    /// whatever scene is currently authored.
    #[test]
    fn the_embedded_scene_still_imports() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID).expect("embedded SceneMaker map is valid");

        let cells = (map.width_tiles() as usize) * (map.height_tiles() as usize);
        assert!(map.width_tiles() > 0 && map.height_tiles() > 0);
        assert!(!map.terrain_cells().is_empty());
        assert!(map.terrain_cells().len() <= cells);
        assert!(
            map.props()
                .iter()
                .any(|placement| placement.asset_key == "ankh")
        );
    }

    /// The embedded directory is a catalog rather than one hard-coded file.
    #[test]
    fn embedded_instances_are_selected_by_scene_id() {
        let overworld = WorldMap::load_embedded("overworld01")
            .expect("the embedded overworld Instance is valid");
        let cave = WorldMap::load_embedded("cave01").expect("the embedded cave Instance is valid");

        assert!(overworld.width_tiles() > cave.width_tiles());
        assert!(overworld.height_tiles() > cave.height_tiles());
    }

    #[test]
    fn templates_and_unknown_names_are_not_substituted_for_instances() {
        let template = WorldMap::load_embedded("test_template")
            .expect_err("an embedded Template is not a WorldMap");
        assert!(template.to_string().contains("not an Instance"));
        assert!(WorldMap::load_embedded("not_a_scene").is_err());
        assert!(WorldMap::from_source(&test_export(TEST_GRASS_CELL, ""), "elsewhere").is_err());
    }

    #[test]
    fn embedded_export_files_are_catalogued_in_stable_order() {
        assert!(
            EMBEDDED_WORLD_EXPORTS
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
        );
    }

    #[test]
    fn obsolete_export_and_scene_versions_are_rejected() {
        assert!(
            WorldMap::from_source(
                &export_document(5, 10, TEST_GRASS_CELL, "", ""),
                TEST_SCENE_ID
            )
            .is_err()
        );
        assert!(
            WorldMap::from_source(
                &export_document(9, 7, TEST_GRASS_CELL, "", ""),
                TEST_SCENE_ID
            )
            .is_err()
        );
    }

    #[test]
    fn authoring_pixels_become_positions_around_the_map_centre() {
        let source = test_export(TEST_GRASS_CELL, ANKH);
        let map =
            WorldMap::from_source(&source, TEST_SCENE_ID).expect("the synthetic export is valid");

        assert_eq!(map.terrain_cells()[0].center, Position::new(-1.5, -1.5));
        assert_eq!(map.props()[0].position, Position::new(0.0, 1.0));
    }

    /// Surface sits on the Asset and height sits on the cell, so the importer
    /// resolves the join once and every consumer reads a whole cell.
    #[test]
    fn a_cell_carries_the_surface_of_its_asset_and_its_own_height() {
        let source = test_export(
            r#"{ "x": 1, "y": 0, "asset_key": "grass", "elevation_meters": 2.5 }"#,
            ANKH,
        );
        let map =
            WorldMap::from_source(&source, TEST_SCENE_ID).expect("the synthetic export is valid");

        assert_eq!(map.terrain_cells()[0].surface, "land");
        assert_eq!(map.terrain_cells()[0].elevation_meters, 2.5);
        assert_eq!(map.props()[0].elevation_meters, 1.0);
    }

    /// Only Terrain Assets may be painted as Terrain, so a cell whose Asset has
    /// no surface is a corrupt export rather than a case to work around.
    #[test]
    fn a_cell_painted_with_an_asset_that_has_no_surface_is_rejected() {
        let source = test_export(
            r#"{ "x": 0, "y": 0, "asset_key": "tree", "elevation_meters": 1.0 }"#,
            "",
        );

        assert!(WorldMap::from_source(&source, TEST_SCENE_ID).is_err());
    }

    /// Anchors survive the import even though nothing composes them yet, so the
    /// gap stays visible instead of the map quietly losing what belongs there.
    #[test]
    fn template_anchors_are_kept_for_a_composition_step_that_does_not_exist_yet() {
        let source = test_export_with_anchors(TEST_GRASS_CELL, ANKH, ANCHOR);
        let map =
            WorldMap::from_source(&source, TEST_SCENE_ID).expect("the synthetic export is valid");

        assert_eq!(map.template_anchors().len(), 1);
        assert_eq!(map.template_anchors()[0].group_number, 1);
        assert_eq!(map.template_anchors()[0].position, Position::new(0.0, 1.0));
    }

    #[test]
    fn cells_outside_the_map_or_without_a_profile_are_rejected() {
        let outside = test_export(
            r#"{ "x": 9, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }"#,
            "",
        );
        let unprofiled = test_export(
            r#"{ "x": 0, "y": 0, "asset_key": "lava", "elevation_meters": 1.0 }"#,
            "",
        );

        assert!(WorldMap::from_source(&outside, TEST_SCENE_ID).is_err());
        assert!(WorldMap::from_source(&unprofiled, TEST_SCENE_ID).is_err());
    }

    #[test]
    fn duplicated_cells_and_instance_ids_are_rejected() {
        let twice = format!("{TEST_GRASS_CELL}, {TEST_GRASS_CELL}");
        let duplicated_cell = test_export(&twice, "");
        let duplicated_id = test_export(TEST_GRASS_CELL, &format!("{ANKH}, {ANKH}"));

        assert!(WorldMap::from_source(&duplicated_cell, TEST_SCENE_ID).is_err());
        assert!(WorldMap::from_source(&duplicated_id, TEST_SCENE_ID).is_err());
    }
}
