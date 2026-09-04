use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
use serde::Deserialize;

use crate::Position;

const FORMAT: &str = "scene_maker_scene_export";
const FORMAT_VERSION: u32 = 10;
const SCENE_SCHEMA: &str = "srt.scene_maker_scene";
const SCENE_VERSION: u32 = 11;
const WORKSPACE_KEY: &str = "world01";
const COORDINATE_SPACE: &str = "scene_local_bottom_left_y_up";

include!(concat!(env!("OUT_DIR"), "/embedded_world_exports.rs"));

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct WorldMap {
    scene_id: String,
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    terrain_cells: Vec<MapTerrainCell>,
    props: Vec<MapProp>,
    route_surfaces: Vec<MapRouteSurface>,
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
        validate_header(&export, scene_id, "instance")?;
        if export.scene.template_definition.is_some() {
            return Err(WorldMapError::new(
                "a SceneMaker Instance must not carry a Template definition",
            ));
        }
        let scene = convert_scene_body(export, SceneCoordinateFrame::Centered)?;
        validate_prop_origins(
            &scene.props,
            scene.width_tiles,
            scene.height_tiles,
            scene.terrain_cell_meters,
            "Instance",
        )?;

        Ok(Self {
            scene_id: scene_id.to_owned(),
            width_tiles: scene.width_tiles,
            height_tiles: scene.height_tiles,
            terrain_cell_meters: scene.terrain_cell_meters,
            terrain_cells: scene.terrain_cells,
            props: scene.props,
            route_surfaces: scene.route_surfaces,
            template_anchors: scene.template_anchors,
        })
    }

    pub fn scene_id(&self) -> &str {
        &self.scene_id
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

    /// Independently elevated Path surfaces, already tessellated by SceneMaker.
    pub fn route_surfaces(&self) -> &[MapRouteSurface] {
        &self.route_surfaces
    }

    /// Every place a Template may be put, with the group it asks for.
    ///
    /// The base map loads with its Anchors empty. [`Self::project_template`]
    /// can translate one choice into map coordinates but does not merge it.
    pub fn template_anchors(&self) -> &[MapTemplateAnchor] {
        &self.template_anchors
    }

    /// Projects one explicitly chosen Template into this Instance's coordinate
    /// frame without mutating or merging either input.
    ///
    /// The returned Terrain is the Template mask. [`Self::merged_with`]
    /// resolves it against existing Terrain and Props.
    pub fn project_template(
        &self,
        anchor_id: &str,
        template: &WorldTemplate,
    ) -> Result<WorldTemplatePlacement, WorldMapError> {
        let anchor = self
            .template_anchors
            .iter()
            .find(|anchor| anchor.anchor_id == anchor_id)
            .ok_or_else(|| {
                WorldMapError::new(format!("Template Anchor '{anchor_id}' does not exist"))
            })?;
        if anchor.group_number != template.group_number {
            return Err(WorldMapError::new(format!(
                "Template '{}' belongs to group {}, but Anchor '{anchor_id}' requires group {}",
                template.scene_id, template.group_number, anchor.group_number
            )));
        }
        if self.terrain_cell_meters != template.terrain_cell_meters {
            return Err(WorldMapError::new(format!(
                "Template '{}' uses terrain cells of {} meters, but this Instance uses {} meters",
                template.scene_id, template.terrain_cell_meters, self.terrain_cell_meters
            )));
        }
        // Authoring pixel densities may differ: both anchors have already been
        // normalized to meters and the shared Terrain grid.

        let grid_offset = SceneGridOffset {
            x: i64::from(anchor.grid_position.x) - i64::from(template.insertion_anchor_grid.x),
            y: i64::from(anchor.grid_position.y) - i64::from(template.insertion_anchor_grid.y),
        };
        let position_offset = Position::new(
            anchor.position.x - template.insertion_anchor.x,
            anchor.position.y - template.insertion_anchor.y,
        );
        let half_width = self.width_meters() * 0.5;
        let half_height = self.height_meters() * 0.5;
        let terrain_cells = template
            .terrain_cells
            .iter()
            .map(|cell| {
                let target_x = i64::from(cell.x) + grid_offset.x;
                let target_y = i64::from(cell.y) + grid_offset.y;
                if target_x < 0
                    || target_y < 0
                    || target_x >= i64::from(self.width_tiles)
                    || target_y >= i64::from(self.height_tiles)
                {
                    return Err(WorldMapError::new(format!(
                        "Template '{}' cell ({}, {}) targets Instance cell ({target_x}, {target_y}) outside Anchor '{anchor_id}'",
                        template.scene_id, cell.x, cell.y
                    )));
                }
                let target_x = target_x as u32;
                let target_y = target_y as u32;
                Ok(MapTerrainCell {
                    x: target_x,
                    y: target_y,
                    asset_key: cell.asset_key.clone(),
                    surface: cell.surface.clone(),
                    elevation_meters: cell.elevation_meters,
                    center: Position::new(
                        (target_x as f32 + 0.5) * self.terrain_cell_meters - half_width,
                        (target_y as f32 + 0.5) * self.terrain_cell_meters - half_height,
                    ),
                })
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;
        let props = template
            .props
            .iter()
            .map(|prop| {
                let position = Position::new(
                    prop.position.x + position_offset.x,
                    prop.position.y + position_offset.y,
                );
                if position.x < -half_width
                    || position.x > half_width
                    || position.y < -half_height
                    || position.y > half_height
                {
                    return Err(WorldMapError::new(format!(
                        "Template '{}' Prop '{}' lies outside the Instance at Anchor '{anchor_id}'",
                        template.scene_id, prop.instance_id
                    )));
                }
                Ok(MapProp {
                    instance_id: prop.instance_id.clone(),
                    asset_key: prop.asset_key.clone(),
                    position,
                    elevation_meters: prop.elevation_meters,
                    footprint: prop.footprint,
                })
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;

        Ok(WorldTemplatePlacement {
            instance_scene_id: self.scene_id.clone(),
            anchor_id: anchor.anchor_id.clone(),
            template_scene_id: template.scene_id.clone(),
            grid_offset,
            position_offset,
            terrain_cells,
            props,
        })
    }

    /// Returns a new map with one projected Template resolved against this
    /// Instance. Neither input is changed.
    ///
    /// Template Terrain and Props win equal ranks. Existing Props compete by
    /// their SceneMaker placement footprints: Terrain mask cells and incoming
    /// Props remove overlapping lower- or equal-ranked Props, while any
    /// overlapping higher-ranked Prop blocks an incoming Prop completely.
    /// Touching footprint edges do not overlap, matching SceneMaker placement.
    /// Callers must rebuild collision, navigation, and other state derived from
    /// the returned world.
    pub fn merged_with(
        &self,
        placement: &WorldTemplatePlacement,
        ranks: &PlacementRanks,
    ) -> Result<Self, WorldMapError> {
        if placement.instance_scene_id != self.scene_id {
            return Err(WorldMapError::new(format!(
                "Template placement belongs to Instance '{}', not '{}'",
                placement.instance_scene_id, self.scene_id
            )));
        }
        if !self
            .template_anchors
            .iter()
            .any(|anchor| anchor.anchor_id == placement.anchor_id)
        {
            return Err(WorldMapError::new(format!(
                "Template Anchor '{}' does not exist in Instance '{}'",
                placement.anchor_id, self.scene_id
            )));
        }
        ranks.validate_map_and_placement(self, placement)?;

        let mut placement_by_cell = HashMap::with_capacity(placement.terrain_cells.len());
        for cell in &placement.terrain_cells {
            if cell.x >= self.width_tiles || cell.y >= self.height_tiles {
                return Err(WorldMapError::new(format!(
                    "Template placement cell ({}, {}) lies outside the Instance",
                    cell.x, cell.y
                )));
            }
            if placement_by_cell.insert((cell.x, cell.y), cell).is_some() {
                return Err(WorldMapError::new(format!(
                    "Template placement cell ({}, {}) is duplicated",
                    cell.x, cell.y
                )));
            }
        }

        let mut occupied_cells =
            HashSet::with_capacity(self.terrain_cells.len() + placement.terrain_cells.len());
        let mut terrain_cells =
            Vec::with_capacity(self.terrain_cells.len() + placement.terrain_cells.len());
        for existing in &self.terrain_cells {
            occupied_cells.insert((existing.x, existing.y));
            let Some(incoming) = placement_by_cell.get(&(existing.x, existing.y)) else {
                terrain_cells.push(existing.clone());
                continue;
            };
            if ranks.required(&incoming.asset_key)? >= ranks.required(&existing.asset_key)? {
                terrain_cells.push((*incoming).clone());
            } else {
                terrain_cells.push(existing.clone());
            }
        }
        for incoming in &placement.terrain_cells {
            if occupied_cells.insert((incoming.x, incoming.y)) {
                terrain_cells.push(incoming.clone());
            }
        }

        let original_prop_ids = self
            .props
            .iter()
            .map(|prop| prop.instance_id.as_str())
            .collect::<HashSet<_>>();
        let placement_cell_bounds = placement
            .terrain_cells
            .iter()
            .map(|cell| {
                Ok((
                    PropBounds::around_cell(cell.center, self.terrain_cell_meters),
                    ranks.required(&cell.asset_key)?,
                ))
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;
        let mut props = Vec::with_capacity(self.props.len() + placement.props.len());
        for existing in &self.props {
            let existing_rank = ranks.required(&existing.asset_key)?;
            let existing_bounds = existing.footprint.bounds_at(existing.position);
            let keep = placement_cell_bounds
                .iter()
                .all(|(cell_bounds, incoming_rank)| {
                    !existing_bounds.overlaps(*cell_bounds) || existing_rank > *incoming_rank
                });
            if keep {
                props.push(existing.clone());
            }
        }

        let mut generated_prop_ids = HashSet::with_capacity(placement.props.len());
        let incoming_props = placement
            .props
            .iter()
            .map(|incoming| {
                let instance_id = format!(
                    "template.{}.{}.{}",
                    placement.anchor_id, placement.template_scene_id, incoming.instance_id
                );
                if original_prop_ids.contains(instance_id.as_str())
                    || !generated_prop_ids.insert(instance_id.clone())
                {
                    return Err(WorldMapError::new(format!(
                        "merged Template Prop ID '{instance_id}' is duplicated"
                    )));
                }
                Ok((instance_id, incoming))
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;
        for (instance_id, incoming) in incoming_props {
            let incoming_rank = ranks.required(&incoming.asset_key)?;
            let incoming_bounds = incoming.footprint.bounds_at(incoming.position);
            let existing_relations = props
                .iter()
                .map(|existing| {
                    Ok((
                        existing
                            .footprint
                            .bounds_at(existing.position)
                            .overlaps(incoming_bounds),
                        ranks.required(&existing.asset_key)?,
                    ))
                })
                .collect::<Result<Vec<_>, WorldMapError>>()?;
            if existing_relations
                .iter()
                .any(|(overlaps, existing_rank)| *overlaps && *existing_rank > incoming_rank)
            {
                continue;
            }
            let mut relation_index = 0;
            props.retain(|_| {
                let (overlaps, existing_rank) = existing_relations[relation_index];
                relation_index += 1;
                !overlaps || existing_rank > incoming_rank
            });
            props.push(MapProp {
                instance_id,
                asset_key: incoming.asset_key.clone(),
                position: incoming.position,
                elevation_meters: incoming.elevation_meters,
                footprint: incoming.footprint,
            });
        }

        Ok(Self {
            scene_id: self.scene_id.clone(),
            width_tiles: self.width_tiles,
            height_tiles: self.height_tiles,
            terrain_cell_meters: self.terrain_cell_meters,
            terrain_cells,
            props,
            route_surfaces: self.route_surfaces.clone(),
            template_anchors: self.template_anchors.clone(),
        })
    }
}

/// One shared precedence scale for authored Terrain and Props.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct PlacementRanks {
    ranks: HashMap<String, u32>,
}

impl PlacementRanks {
    pub fn from_entries<I, S>(entries: I) -> Result<Self, WorldMapError>
    where
        I: IntoIterator<Item = (S, u32)>,
        S: Into<String>,
    {
        let mut ranks = HashMap::new();
        for (asset_key, rank) in entries {
            let asset_key = asset_key.into();
            if asset_key.is_empty() || ranks.insert(asset_key.clone(), rank).is_some() {
                return Err(WorldMapError::new(format!(
                    "Placement Rank Asset key '{asset_key}' is empty or duplicated"
                )));
            }
        }
        if ranks.is_empty() {
            return Err(WorldMapError::new(
                "Placement Ranks require at least one Asset",
            ));
        }
        Ok(Self { ranks })
    }

    pub fn rank(&self, asset_key: &str) -> Option<u32> {
        self.ranks.get(asset_key).copied()
    }

    /// Ensures design ranks cover one Instance and every Template currently
    /// available to it. Extra future-facing rank entries remain valid.
    pub fn validate_for(
        &self,
        map: &WorldMap,
        templates: &WorldTemplateCatalog,
    ) -> Result<(), WorldMapError> {
        for asset_key in map
            .terrain_cells
            .iter()
            .map(|cell| cell.asset_key.as_str())
            .chain(map.props.iter().map(|prop| prop.asset_key.as_str()))
            .chain(templates.groups.values().flatten().flat_map(|template| {
                template
                    .terrain_cells
                    .iter()
                    .map(|cell| cell.asset_key.as_str())
                    .chain(template.props.iter().map(|prop| prop.asset_key.as_str()))
            }))
        {
            self.required(asset_key)?;
        }
        Ok(())
    }

    fn validate_map_and_placement(
        &self,
        map: &WorldMap,
        placement: &WorldTemplatePlacement,
    ) -> Result<(), WorldMapError> {
        for asset_key in map
            .terrain_cells
            .iter()
            .map(|cell| cell.asset_key.as_str())
            .chain(map.props.iter().map(|prop| prop.asset_key.as_str()))
            .chain(
                placement
                    .terrain_cells
                    .iter()
                    .map(|cell| cell.asset_key.as_str()),
            )
            .chain(placement.props.iter().map(|prop| prop.asset_key.as_str()))
        {
            self.required(asset_key)?;
        }
        Ok(())
    }

    fn required(&self, asset_key: &str) -> Result<u32, WorldMapError> {
        self.rank(asset_key).ok_or_else(|| {
            WorldMapError::new(format!("Placement Rank is missing for Asset '{asset_key}'"))
        })
    }
}

/// SceneMaker Templates grouped by the number requested by Instance Anchors.
///
/// This catalog only imports authored data. It does not choose, place, or
/// compose a Template into a [`WorldMap`].
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct WorldTemplateCatalog {
    groups: BTreeMap<u32, Vec<WorldTemplate>>,
}

impl WorldTemplateCatalog {
    pub fn load_embedded() -> Result<Self, WorldMapError> {
        let mut groups = BTreeMap::<u32, Vec<WorldTemplate>>::new();
        for (_, scene_id, scene_kind, source) in EMBEDDED_WORLD_EXPORTS {
            if *scene_kind != "template" {
                continue;
            }
            let template = import_catalog_template(scene_id, source)?;
            groups
                .entry(template.group_number())
                .or_default()
                .push(template);
        }
        Ok(Self { groups })
    }

    /// Templates in deterministic export-file order. An absent group is a
    /// normal empty result because an Anchor may have no available occupant.
    pub fn templates_for_group(&self, group_number: u32) -> &[WorldTemplate] {
        self.groups
            .get(&group_number)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn template(&self, scene_id: &str) -> Option<&WorldTemplate> {
        self.groups
            .values()
            .flatten()
            .find(|template| template.scene_id() == scene_id)
    }

    pub fn groups(&self) -> impl ExactSizeIterator<Item = (u32, &[WorldTemplate])> {
        self.groups
            .iter()
            .map(|(group_number, templates)| (*group_number, templates.as_slice()))
    }

    #[cfg(test)]
    pub(crate) fn from_templates(templates: impl IntoIterator<Item = WorldTemplate>) -> Self {
        let mut groups = BTreeMap::<u32, Vec<WorldTemplate>>::new();
        for template in templates {
            groups
                .entry(template.group_number)
                .or_default()
                .push(template);
        }
        Self { groups }
    }
}

fn import_catalog_template(scene_id: &str, source: &str) -> Result<WorldTemplate, WorldMapError> {
    WorldTemplate::from_source(source, scene_id).map_err(|error| {
        WorldMapError::new(format!(
            "cannot import embedded Template '{scene_id}': {error}"
        ))
    })
}

/// A Template in its own bottom-left-local coordinate frame.
///
/// Positions are deliberately not centered like a [`WorldMap`]. Projection
/// translates them by an Instance Anchor minus `insertion_anchor`.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldTemplate {
    scene_id: String,
    group_number: u32,
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    insertion_anchor: Position,
    insertion_anchor_grid: SceneGridPosition,
    terrain_cells: Vec<MapTerrainCell>,
    props: Vec<MapProp>,
}

impl WorldTemplate {
    pub fn load_embedded(scene_id: &str) -> Result<Self, WorldMapError> {
        Self::from_source(
            embedded_scene_source(scene_id, "template", "Template")?,
            scene_id,
        )
    }

    pub fn from_source(source: &str, scene_id: &str) -> Result<Self, WorldMapError> {
        let mut export: ExportDocument = serde_json::from_str(source).map_err(|error| {
            WorldMapError::new(format!("cannot parse SceneMaker Template: {error}"))
        })?;
        validate_header(&export, scene_id, "template")?;
        if !export.scene.template_anchors.is_empty() {
            return Err(WorldMapError::new(
                "a SceneMaker Template must not carry Template Anchors",
            ));
        }
        if !export.water_raster.is_empty() || !export.scene.water_bodies.is_empty() {
            return Err(WorldMapError::new(
                "a SceneMaker Template must not carry water",
            ));
        }
        if !export.route_surface_bakes.is_empty() || !export.scene.route_surfaces.is_empty() {
            return Err(WorldMapError::new(
                "a SceneMaker Template must not carry route surfaces until composition defines them",
            ));
        }
        let Some(definition) = export.scene.template_definition.take() else {
            return Err(WorldMapError::new(
                "a SceneMaker Template requires a Template definition",
            ));
        };
        if definition.group_number == 0 {
            return Err(WorldMapError::new(
                "a SceneMaker Template group number must be positive",
            ));
        }
        let scene = convert_scene_body(export, SceneCoordinateFrame::TemplateLocal)?;
        let insertion_anchor = validate_grid_anchor(
            "Template insertion anchor",
            &definition.insertion_anchor_authoring_px,
            scene.authoring_pixels_per_meter,
            scene.terrain_cell_meters,
            scene.width_tiles,
            scene.height_tiles,
        )?;

        Ok(Self {
            scene_id: scene_id.to_owned(),
            group_number: definition.group_number,
            width_tiles: scene.width_tiles,
            height_tiles: scene.height_tiles,
            terrain_cell_meters: scene.terrain_cell_meters,
            insertion_anchor: insertion_anchor.position,
            insertion_anchor_grid: insertion_anchor.grid_position,
            terrain_cells: scene.terrain_cells,
            props: scene.props,
        })
    }

    pub fn scene_id(&self) -> &str {
        &self.scene_id
    }

    pub const fn group_number(&self) -> u32 {
        self.group_number
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

    pub const fn insertion_anchor(&self) -> Position {
        self.insertion_anchor
    }

    pub const fn insertion_anchor_grid(&self) -> SceneGridPosition {
        self.insertion_anchor_grid
    }

    pub fn terrain_cells(&self) -> &[MapTerrainCell] {
        &self.terrain_cells
    }

    pub fn props(&self) -> &[MapProp] {
        &self.props
    }
}

fn embedded_instance_source(scene_id: &str) -> Result<&'static str, WorldMapError> {
    embedded_scene_source(scene_id, "instance", "Instance")
}

fn embedded_scene_source(
    scene_id: &str,
    expected_kind: &str,
    expected_label: &str,
) -> Result<&'static str, WorldMapError> {
    let Some((_, _, scene_kind, source)) = EMBEDDED_WORLD_EXPORTS
        .iter()
        .find(|entry| entry.1 == scene_id)
        .copied()
    else {
        return Err(WorldMapError::new(format!(
            "SceneMaker {expected_label} '{scene_id}' is not embedded"
        )));
    };
    if scene_kind != expected_kind {
        let article = if expected_kind == "instance" {
            "an"
        } else {
            "a"
        };
        return Err(WorldMapError::new(format!(
            "SceneMaker scene '{scene_id}' is a '{scene_kind}', not {article} {expected_label}"
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

/// A SceneMaker-authored Path kept separate from the Terrain height field.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRouteSurface {
    pub route_surface_id: String,
    pub asset_key: String,
    pub surface: String,
    pub vertices: Vec<MapRouteVertex>,
    pub triangle_indices: Vec<u32>,
    pub boundary_edges: Vec<MapRouteBoundaryEdge>,
    pub centerline_samples: Vec<MapRouteCenterlineSample>,
    pub segments: Vec<MapRouteSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapRouteVertex {
    pub position: Position,
    pub elevation_meters: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapRouteBoundaryEdge {
    pub start_vertex_index: u32,
    pub end_vertex_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapRouteCenterlineSample {
    pub position: Position,
    pub elevation_meters: f32,
    pub width_meters: f32,
    pub station_meters: f32,
    pub authored_point_index: Option<u32>,
}

/// Runtime meaning retained from one authored Path interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRouteSegment {
    pub segment_id: String,
    pub grade_percent: i32,
    pub start_point_index: u32,
    pub end_point_index: u32,
    pub start_sample_index: u32,
    pub end_sample_index: u32,
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
    /// SceneMaker's visible placement bounds, used only for authoring-style
    /// replacement overlap. Gameplay collision remains separate geometry.
    pub footprint: MapPropFootprint,
}

/// One Asset's axis-aligned SceneMaker placement footprint in meters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapPropFootprint {
    width_meters: f32,
    height_meters: f32,
    anchor: Position,
}

impl MapPropFootprint {
    pub const fn width_meters(self) -> f32 {
        self.width_meters
    }

    pub const fn height_meters(self) -> f32 {
        self.height_meters
    }

    /// Offset from the footprint's lower-left corner to the Prop pivot.
    pub const fn anchor(self) -> Position {
        self.anchor
    }

    fn bounds_at(self, position: Position) -> PropBounds {
        let left = position.x - self.anchor.x;
        let bottom = position.y - self.anchor.y;
        PropBounds {
            left,
            right: left + self.width_meters,
            bottom,
            top: bottom + self.height_meters,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PropBounds {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
}

impl PropBounds {
    fn around_cell(center: Position, cell_size: f32) -> Self {
        let half = cell_size * 0.5;
        Self {
            left: center.x - half,
            right: center.x + half,
            bottom: center.y - half,
            top: center.y + half,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.bottom < other.top
            && self.top > other.bottom
    }
}

/// A place where a Template of `group_number` belongs.
#[derive(Debug, Clone, PartialEq)]
pub struct MapTemplateAnchor {
    pub anchor_id: String,
    pub group_number: u32,
    pub position: Position,
    pub grid_position: SceneGridPosition,
}

/// A checked intersection on a Scene's Terrain-cell grid.
///
/// Each axis ranges from zero through the corresponding Scene dimension,
/// inclusive. It is not necessarily the index of a Terrain cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneGridPosition {
    pub x: u32,
    pub y: u32,
}

/// A signed translation between two [`SceneGridPosition`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneGridOffset {
    pub x: i64,
    pub y: i64,
}

/// Template geometry translated into, and identity-bound to, one Instance.
/// It has not yet been merged into that Instance.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldTemplatePlacement {
    instance_scene_id: String,
    anchor_id: String,
    template_scene_id: String,
    grid_offset: SceneGridOffset,
    position_offset: Position,
    terrain_cells: Vec<MapTerrainCell>,
    props: Vec<MapProp>,
}

impl WorldTemplatePlacement {
    pub fn instance_scene_id(&self) -> &str {
        &self.instance_scene_id
    }

    pub fn anchor_id(&self) -> &str {
        &self.anchor_id
    }

    pub fn template_scene_id(&self) -> &str {
        &self.template_scene_id
    }

    pub const fn grid_offset(&self) -> SceneGridOffset {
        self.grid_offset
    }

    pub const fn position_offset(&self) -> Position {
        self.position_offset
    }

    /// The translated Template Terrain cells, which form its placement mask.
    pub fn terrain_cells(&self) -> &[MapTerrainCell] {
        &self.terrain_cells
    }

    /// Translated Template Props. Their IDs remain Template-local until
    /// [`WorldMap::merged_with`] applies the documented namespace.
    pub fn props(&self) -> &[MapProp] {
        &self.props
    }
}

#[derive(Clone, Copy)]
enum SceneCoordinateFrame {
    Centered,
    TemplateLocal,
}

struct ConvertedSceneBody {
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    authoring_pixels_per_meter: f32,
    terrain_cells: Vec<MapTerrainCell>,
    props: Vec<MapProp>,
    route_surfaces: Vec<MapRouteSurface>,
    template_anchors: Vec<MapTemplateAnchor>,
}

fn convert_scene_body(
    export: ExportDocument,
    coordinate_frame: SceneCoordinateFrame,
) -> Result<ConvertedSceneBody, WorldMapError> {
    let ExportDocument {
        grid,
        asset_profiles,
        route_surface_bakes,
        scene,
        ..
    } = export;
    let SceneDocument {
        size_cells,
        terrain_cells: source_terrain_cells,
        props: source_props,
        route_surfaces: source_route_surfaces,
        template_anchors: source_template_anchors,
        ..
    } = scene;

    let width_tiles = size_cells.width;
    let height_tiles = size_cells.height;
    if width_tiles == 0 || height_tiles == 0 {
        return Err(WorldMapError::new(
            "scene dimensions must be greater than zero",
        ));
    }
    if !grid.terrain_cell_meters.is_finite() || grid.terrain_cell_meters <= 0.0 {
        return Err(WorldMapError::new(
            "terrain_cell_meters must be finite and greater than zero",
        ));
    }
    if !grid.authoring_pixels_per_meter.is_finite() || grid.authoring_pixels_per_meter <= 0.0 {
        return Err(WorldMapError::new(
            "authoring_pixels_per_meter must be finite and greater than zero",
        ));
    }
    if !grid.game_pixels_per_meter.is_finite() || grid.game_pixels_per_meter <= 0.0 {
        return Err(WorldMapError::new(
            "game_pixels_per_meter must be finite and greater than zero",
        ));
    }
    if !grid.water_cell_meters.is_finite() || grid.water_cell_meters <= 0.0 {
        return Err(WorldMapError::new(
            "water_cell_meters must be finite and greater than zero",
        ));
    }

    let profiles = asset_profiles
        .into_iter()
        .map(|profile| (profile.asset_key.clone(), profile))
        .collect::<HashMap<_, _>>();
    let (offset_x, offset_y) = match coordinate_frame {
        SceneCoordinateFrame::Centered => (
            -(width_tiles as f32 * grid.terrain_cell_meters * 0.5),
            -(height_tiles as f32 * grid.terrain_cell_meters * 0.5),
        ),
        SceneCoordinateFrame::TemplateLocal => (0.0, 0.0),
    };

    let mut occupied_cells = HashSet::new();
    let mut terrain_cells = Vec::with_capacity(source_terrain_cells.len());
    for cell in source_terrain_cells {
        if cell.x >= width_tiles || cell.y >= height_tiles {
            return Err(WorldMapError::new(format!(
                "terrain cell ({}, {}) lies outside the scene",
                cell.x, cell.y
            )));
        }
        if !occupied_cells.insert((cell.x, cell.y)) {
            return Err(WorldMapError::new(format!(
                "terrain cell ({}, {}) is duplicated",
                cell.x, cell.y
            )));
        }
        let Some(surface) = require_profile(&profiles, &cell.asset_key)?.surface.clone() else {
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
                (cell.x as f32 + 0.5) * grid.terrain_cell_meters + offset_x,
                (cell.y as f32 + 0.5) * grid.terrain_cell_meters + offset_y,
            ),
        });
    }

    let mut instance_ids = HashSet::new();
    let props = convert_props(
        source_props,
        &profiles,
        &mut instance_ids,
        grid.authoring_pixels_per_meter,
        offset_x,
        offset_y,
    )?;
    validate_authored_prop_footprints(&props)?;
    let route_surfaces = convert_route_surfaces(
        source_route_surfaces,
        route_surface_bakes,
        &profiles,
        offset_x,
        offset_y,
    )?;
    let mut anchor_ids = HashSet::new();
    let template_anchors = source_template_anchors
        .into_iter()
        .map(|anchor| {
            if anchor.anchor_id.is_empty() || !anchor_ids.insert(anchor.anchor_id.clone()) {
                return Err(WorldMapError::new(format!(
                    "Template Anchor ID '{}' is empty or duplicated",
                    anchor.anchor_id
                )));
            }
            if anchor.group_number == 0 {
                return Err(WorldMapError::new(format!(
                    "Template Anchor '{}' group number must be positive",
                    anchor.anchor_id
                )));
            }
            let checked = validate_grid_anchor(
                &format!("Template Anchor '{}'", anchor.anchor_id),
                &anchor.position_authoring_px,
                grid.authoring_pixels_per_meter,
                grid.terrain_cell_meters,
                width_tiles,
                height_tiles,
            )?;
            Ok(MapTemplateAnchor {
                anchor_id: anchor.anchor_id,
                group_number: anchor.group_number,
                position: Position::new(
                    checked.position.x + offset_x,
                    checked.position.y + offset_y,
                ),
                grid_position: checked.grid_position,
            })
        })
        .collect::<Result<Vec<_>, WorldMapError>>()?;

    Ok(ConvertedSceneBody {
        width_tiles,
        height_tiles,
        terrain_cell_meters: grid.terrain_cell_meters,
        authoring_pixels_per_meter: grid.authoring_pixels_per_meter,
        terrain_cells,
        props,
        route_surfaces,
        template_anchors,
    })
}

fn convert_route_surfaces(
    sources: Vec<RouteSurfaceDocument>,
    bakes: Vec<RouteSurfaceBakeDocument>,
    profiles: &HashMap<String, AssetProfileDocument>,
    offset_x: f32,
    offset_y: f32,
) -> Result<Vec<MapRouteSurface>, WorldMapError> {
    if sources.len() != bakes.len() {
        return Err(WorldMapError::new(
            "authored route surfaces and route_surface_bakes must have the same length",
        ));
    }
    let mut route_ids = HashSet::with_capacity(sources.len());
    let mut segment_ids = HashSet::new();
    let mut previous_route_id: Option<String> = None;
    let mut converted = Vec::with_capacity(sources.len());
    for (source, bake) in sources.into_iter().zip(bakes) {
        if source.route_surface_id.is_empty()
            || !route_ids.insert(source.route_surface_id.clone())
            || previous_route_id
                .as_ref()
                .is_some_and(|previous| previous >= &source.route_surface_id)
        {
            return Err(WorldMapError::new(format!(
                "route surface ID '{}' is empty, duplicated, or out of order",
                source.route_surface_id
            )));
        }
        previous_route_id = Some(source.route_surface_id.clone());
        if source.route_surface_id != bake.route_surface_id || source.asset_key != bake.asset_key {
            return Err(WorldMapError::new(format!(
                "route surface '{}' does not match its runtime bake",
                source.route_surface_id
            )));
        }
        let Some(surface) = require_profile(profiles, &source.asset_key)?
            .surface
            .clone()
        else {
            return Err(WorldMapError::new(format!(
                "route surface '{}' Asset '{}' has no exported surface",
                source.route_surface_id, source.asset_key
            )));
        };
        validate_route_source(&source)?;
        for segment in &source.segments {
            if !segment_ids.insert(segment.segment_id.clone()) {
                return Err(WorldMapError::new(format!(
                    "route segment ID '{}' is duplicated across Paths",
                    segment.segment_id
                )));
            }
        }
        let vertices = bake
            .vertices
            .into_iter()
            .map(|vertex| {
                if !vertex.x_meters.is_finite()
                    || !vertex.y_meters.is_finite()
                    || !vertex.elevation_meters.is_finite()
                {
                    return Err(WorldMapError::new(format!(
                        "route surface '{}' has a non-finite baked vertex",
                        source.route_surface_id
                    )));
                }
                Ok(MapRouteVertex {
                    position: Position::new(vertex.x_meters + offset_x, vertex.y_meters + offset_y),
                    elevation_meters: vertex.elevation_meters,
                })
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;
        if bake.triangle_indices.is_empty() || bake.triangle_indices.len() % 3 != 0 {
            return Err(WorldMapError::new(format!(
                "route surface '{}' needs complete baked triangles",
                source.route_surface_id
            )));
        }
        for triangle in bake.triangle_indices.chunks_exact(3) {
            if triangle
                .iter()
                .any(|index| *index as usize >= vertices.len())
            {
                return Err(WorldMapError::new(format!(
                    "route surface '{}' has an invalid triangle index",
                    source.route_surface_id
                )));
            }
            let a = vertices[triangle[0] as usize].position;
            let b = vertices[triangle[1] as usize].position;
            let c = vertices[triangle[2] as usize].position;
            let twice_area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if !twice_area.is_finite() || twice_area == 0.0 {
                return Err(WorldMapError::new(format!(
                    "route surface '{}' has a degenerate baked triangle",
                    source.route_surface_id
                )));
            }
        }
        let boundary_edges = bake
            .boundary_edges
            .into_iter()
            .map(|edge| {
                if edge.start_vertex_index as usize >= vertices.len()
                    || edge.end_vertex_index as usize >= vertices.len()
                    || edge.start_vertex_index == edge.end_vertex_index
                {
                    return Err(WorldMapError::new(format!(
                        "route surface '{}' has an invalid boundary edge",
                        source.route_surface_id
                    )));
                }
                Ok(MapRouteBoundaryEdge {
                    start_vertex_index: edge.start_vertex_index,
                    end_vertex_index: edge.end_vertex_index,
                })
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;
        if boundary_edges.is_empty() {
            return Err(WorldMapError::new(format!(
                "route surface '{}' needs baked boundary edges",
                source.route_surface_id
            )));
        }
        let mut previous_station = None;
        let centerline_samples = bake
            .centerline_samples
            .into_iter()
            .map(|sample| {
                if !sample.x_meters.is_finite()
                    || !sample.y_meters.is_finite()
                    || !sample.elevation_meters.is_finite()
                    || !sample.width_meters.is_finite()
                    || sample.width_meters <= 0.0
                    || !sample.station_meters.is_finite()
                    || sample.station_meters < 0.0
                    || previous_station.is_some_and(|previous| previous >= sample.station_meters)
                    || sample
                        .authored_point_index
                        .is_some_and(|index| index as usize >= source.points.len())
                {
                    return Err(WorldMapError::new(format!(
                        "route surface '{}' has an invalid centerline sample",
                        source.route_surface_id
                    )));
                }
                previous_station = Some(sample.station_meters);
                Ok(MapRouteCenterlineSample {
                    position: Position::new(sample.x_meters + offset_x, sample.y_meters + offset_y),
                    elevation_meters: sample.elevation_meters,
                    width_meters: sample.width_meters,
                    station_meters: sample.station_meters,
                    authored_point_index: sample.authored_point_index,
                })
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;
        if centerline_samples.len() < 2 {
            return Err(WorldMapError::new(format!(
                "route surface '{}' needs at least two centerline samples",
                source.route_surface_id
            )));
        }
        let segments =
            validate_route_bake_segments(&source, bake.segments, centerline_samples.len())?;
        converted.push(MapRouteSurface {
            route_surface_id: source.route_surface_id,
            asset_key: source.asset_key,
            surface,
            vertices,
            triangle_indices: bake.triangle_indices,
            boundary_edges,
            centerline_samples,
            segments,
        });
    }
    Ok(converted)
}

fn validate_route_source(source: &RouteSurfaceDocument) -> Result<(), WorldMapError> {
    if source.points.len() < 2 || source.segments.len() != source.points.len() - 1 {
        return Err(WorldMapError::new(format!(
            "route surface '{}' needs one segment per point pair",
            source.route_surface_id
        )));
    }
    for point in &source.points {
        if !point.elevation_meters.is_finite()
            || !point.width_meters.is_finite()
            || point.width_meters <= 0.0
            || point.position_authoring_px.x < 0
            || point.position_authoring_px.y < 0
            || !matches!(point.mode.as_str(), "linear" | "aligned")
            || (point.mode == "linear"
                && (point.handle_in_authoring_px.x != 0
                    || point.handle_in_authoring_px.y != 0
                    || point.handle_out_authoring_px.x != 0
                    || point.handle_out_authoring_px.y != 0))
        {
            return Err(WorldMapError::new(format!(
                "route surface '{}' has an invalid authored point",
                source.route_surface_id
            )));
        }
    }
    let mut segment_ids = HashSet::with_capacity(source.segments.len());
    for segment in &source.segments {
        if segment.segment_id.is_empty()
            || !segment_ids.insert(segment.segment_id.clone())
            || !matches!(segment.grade_percent, -50 | -25 | 0 | 25 | 50)
        {
            return Err(WorldMapError::new(format!(
                "route surface '{}' has an invalid authored segment",
                source.route_surface_id
            )));
        }
    }
    Ok(())
}

fn validate_route_bake_segments(
    source: &RouteSurfaceDocument,
    baked: Vec<RouteSurfaceBakeSegmentDocument>,
    sample_count: usize,
) -> Result<Vec<MapRouteSegment>, WorldMapError> {
    if baked.len() != source.segments.len() {
        return Err(WorldMapError::new(format!(
            "route surface '{}' bake has the wrong segment count",
            source.route_surface_id
        )));
    }
    baked
        .into_iter()
        .enumerate()
        .map(|(index, segment)| {
            let authored = &source.segments[index];
            if segment.segment_id != authored.segment_id
                || segment.grade_percent != authored.grade_percent
                || segment.start_point_index as usize != index
                || segment.end_point_index as usize != index + 1
                || segment.start_sample_index as usize >= sample_count
                || segment.end_sample_index as usize >= sample_count
                || segment.start_sample_index >= segment.end_sample_index
            {
                return Err(WorldMapError::new(format!(
                    "route surface '{}' has an invalid baked segment mapping",
                    source.route_surface_id
                )));
            }
            Ok(MapRouteSegment {
                segment_id: segment.segment_id,
                grade_percent: segment.grade_percent,
                start_point_index: segment.start_point_index,
                end_point_index: segment.end_point_index,
                start_sample_index: segment.start_sample_index,
                end_sample_index: segment.end_sample_index,
            })
        })
        .collect()
}

struct ValidatedGridAnchor {
    position: Position,
    grid_position: SceneGridPosition,
}

fn validate_grid_anchor(
    label: &str,
    anchor: &PointDocument,
    authoring_pixels_per_meter: f32,
    terrain_cell_meters: f32,
    width_tiles: u32,
    height_tiles: u32,
) -> Result<ValidatedGridAnchor, WorldMapError> {
    let pixels_per_cell = f64::from(authoring_pixels_per_meter) * f64::from(terrain_cell_meters);
    let x = f64::from(anchor.x);
    let y = f64::from(anchor.y);
    let width_pixels = f64::from(width_tiles) * pixels_per_cell;
    let height_pixels = f64::from(height_tiles) * pixels_per_cell;
    if x < 0.0 || y < 0.0 || x > width_pixels || y > height_pixels {
        return Err(WorldMapError::new(format!(
            "{label} must lie inside its scene"
        )));
    }
    let cell_x = x / pixels_per_cell;
    let cell_y = y / pixels_per_cell;
    if (cell_x - cell_x.round()).abs() > 1.0e-6 || (cell_y - cell_y.round()).abs() > 1.0e-6 {
        return Err(WorldMapError::new(format!(
            "{label} must lie on the Terrain cell grid"
        )));
    }

    Ok(ValidatedGridAnchor {
        position: Position::new(
            anchor.x as f32 / authoring_pixels_per_meter,
            anchor.y as f32 / authoring_pixels_per_meter,
        ),
        grid_position: SceneGridPosition {
            x: cell_x.round() as u32,
            y: cell_y.round() as u32,
        },
    })
}

fn validate_header(
    export: &ExportDocument,
    scene_id: &str,
    expected_scene_kind: &str,
) -> Result<(), WorldMapError> {
    let scene = &export.scene;
    if scene.scene_id != scene_id {
        return Err(WorldMapError::new(format!(
            "requested scene '{scene_id}', but this document is '{}'",
            scene.scene_id
        )));
    }
    if export.format != FORMAT
        || export.version != FORMAT_VERSION
        || export.workspace_key != WORKSPACE_KEY
        || scene.schema != SCENE_SCHEMA
        || scene.version != SCENE_VERSION
        || scene.coordinate_space != COORDINATE_SPACE
    {
        return Err(WorldMapError::new(
            "SceneMaker map header does not match the world01 import contract",
        ));
    }
    if scene.scene_kind != expected_scene_kind {
        return Err(WorldMapError::new(format!(
            "SceneMaker scene '{scene_id}' has kind '{}', expected '{expected_scene_kind}'",
            scene.scene_kind
        )));
    }
    Ok(())
}

fn require_profile<'a>(
    profiles: &'a HashMap<String, AssetProfileDocument>,
    asset_key: &str,
) -> Result<&'a AssetProfileDocument, WorldMapError> {
    if asset_key.is_empty() {
        return Err(WorldMapError::new("map asset key must not be empty"));
    }
    profiles.get(asset_key).ok_or_else(|| {
        WorldMapError::new(format!(
            "map asset '{asset_key}' has no exported asset profile"
        ))
    })
}

fn convert_props(
    source: Vec<PropDocument>,
    profiles: &HashMap<String, AssetProfileDocument>,
    instance_ids: &mut HashSet<String>,
    authoring_pixels_per_meter: f32,
    offset_x: f32,
    offset_y: f32,
) -> Result<Vec<MapProp>, WorldMapError> {
    source
        .into_iter()
        .map(|placement| {
            let profile = require_profile(profiles, &placement.asset_key)?;
            let footprint = convert_prop_footprint(profile, &placement.asset_key)?;
            if placement.instance_id.is_empty()
                || !instance_ids.insert(placement.instance_id.clone())
            {
                return Err(WorldMapError::new(format!(
                    "map instance ID '{}' is empty or duplicated",
                    placement.instance_id
                )));
            }
            let position = Position::new(
                placement.position_authoring_px.x as f32 / authoring_pixels_per_meter + offset_x,
                placement.position_authoring_px.y as f32 / authoring_pixels_per_meter + offset_y,
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
                footprint,
            })
        })
        .collect()
}

fn convert_prop_footprint(
    profile: &AssetProfileDocument,
    asset_key: &str,
) -> Result<MapPropFootprint, WorldMapError> {
    let (Some(size), Some(anchor)) = (&profile.footprint_meters, &profile.anchor_meters) else {
        return Err(WorldMapError::new(format!(
            "Prop asset '{asset_key}' has no exported placement footprint"
        )));
    };
    if !size.width.is_finite()
        || size.width <= 0.0
        || !size.height.is_finite()
        || size.height <= 0.0
        || !anchor.x.is_finite()
        || !anchor.y.is_finite()
        || anchor.x < 0.0
        || anchor.x > size.width
        || anchor.y < 0.0
        || anchor.y > size.height
    {
        return Err(WorldMapError::new(format!(
            "Prop asset '{asset_key}' has an invalid placement footprint"
        )));
    }
    Ok(MapPropFootprint {
        width_meters: size.width,
        height_meters: size.height,
        anchor: Position::new(anchor.x, anchor.y),
    })
}

fn validate_authored_prop_footprints(props: &[MapProp]) -> Result<(), WorldMapError> {
    for (index, prop) in props.iter().enumerate() {
        let bounds = prop.footprint.bounds_at(prop.position);
        for other in &props[index + 1..] {
            if bounds.overlaps(other.footprint.bounds_at(other.position)) {
                return Err(WorldMapError::new(format!(
                    "map instances '{}' and '{}' have overlapping placement footprints",
                    prop.instance_id, other.instance_id
                )));
            }
        }
    }
    Ok(())
}

fn validate_prop_origins(
    props: &[MapProp],
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    scene_label: &str,
) -> Result<(), WorldMapError> {
    let half_width = width_tiles as f32 * terrain_cell_meters * 0.5;
    let half_height = height_tiles as f32 * terrain_cell_meters * 0.5;
    for prop in props {
        if prop.position.x < -half_width
            || prop.position.x > half_width
            || prop.position.y < -half_height
            || prop.position.y > half_height
        {
            return Err(WorldMapError::new(format!(
                "{scene_label} Prop '{}' lies outside the scene",
                prop.instance_id
            )));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct WorldMapError(String);

impl WorldMapError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
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
    water_raster: Vec<serde::de::IgnoredAny>,
    route_surface_bakes: Vec<RouteSurfaceBakeDocument>,
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
    footprint_meters: Option<SizeMetersDocument>,
    anchor_meters: Option<PointMetersDocument>,
}

#[derive(Deserialize)]
struct SizeMetersDocument {
    width: f32,
    height: f32,
}

#[derive(Deserialize)]
struct PointMetersDocument {
    x: f32,
    y: f32,
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
    water_bodies: Vec<serde::de::IgnoredAny>,
    route_surfaces: Vec<RouteSurfaceDocument>,
    template_definition: Option<TemplateDefinitionDocument>,
    template_anchors: Vec<TemplateAnchorDocument>,
}

#[derive(Deserialize)]
struct RouteSurfaceDocument {
    route_surface_id: String,
    asset_key: String,
    points: Vec<RouteSurfacePointDocument>,
    segments: Vec<RouteSurfaceSourceSegmentDocument>,
}

#[derive(Deserialize)]
struct RouteSurfacePointDocument {
    position_authoring_px: PointDocument,
    mode: String,
    handle_in_authoring_px: PointDocument,
    handle_out_authoring_px: PointDocument,
    elevation_meters: f32,
    width_meters: f32,
}

#[derive(Deserialize)]
struct RouteSurfaceSourceSegmentDocument {
    segment_id: String,
    grade_percent: i32,
}

#[derive(Deserialize)]
struct RouteSurfaceBakeDocument {
    route_surface_id: String,
    asset_key: String,
    vertices: Vec<RouteSurfaceVertexDocument>,
    triangle_indices: Vec<u32>,
    boundary_edges: Vec<RouteSurfaceBoundaryEdgeDocument>,
    centerline_samples: Vec<RouteSurfaceCenterlineSampleDocument>,
    segments: Vec<RouteSurfaceBakeSegmentDocument>,
}

#[derive(Deserialize)]
struct RouteSurfaceVertexDocument {
    x_meters: f32,
    y_meters: f32,
    elevation_meters: f32,
}

#[derive(Deserialize)]
struct RouteSurfaceBoundaryEdgeDocument {
    start_vertex_index: u32,
    end_vertex_index: u32,
}

#[derive(Deserialize)]
struct RouteSurfaceCenterlineSampleDocument {
    x_meters: f32,
    y_meters: f32,
    elevation_meters: f32,
    width_meters: f32,
    station_meters: f32,
    authored_point_index: Option<u32>,
}

#[derive(Deserialize)]
struct RouteSurfaceBakeSegmentDocument {
    segment_id: String,
    grade_percent: i32,
    start_point_index: u32,
    end_point_index: u32,
    start_sample_index: u32,
    end_sample_index: u32,
}

#[derive(Deserialize)]
struct TemplateDefinitionDocument {
    group_number: u32,
    insertion_anchor_authoring_px: PointDocument,
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
pub(crate) fn test_export_with_anchors(terrain_cells: &str, props: &str, anchors: &str) -> String {
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
                {{ "asset_key": "grass", "surface": "land", "footprint_meters": null, "anchor_meters": null }},
                {{ "asset_key": "ankh", "surface": null, "footprint_meters": {{ "width": 1.0, "height": 1.0 }}, "anchor_meters": {{ "x": 0.5, "y": 0.5 }} }},
                {{ "asset_key": "tree", "surface": null, "footprint_meters": {{ "width": 1.0, "height": 1.0 }}, "anchor_meters": {{ "x": 0.5, "y": 0.5 }} }}
            ],
            "water_raster": [],
            "route_surface_bakes": [],
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
                "route_surfaces": [],
                "template_definition": null,
                "template_anchors": [{anchors}],
                "default_elevation_meters": 1.0
            }}
        }}"#
    )
}

#[cfg(test)]
pub(crate) fn template_export(
    template_definition: &str,
    terrain_cells: &str,
    props: &str,
) -> String {
    template_export_with_anchors(template_definition, terrain_cells, props, "")
}

#[cfg(test)]
fn template_export_with_anchors(
    template_definition: &str,
    terrain_cells: &str,
    props: &str,
    anchors: &str,
) -> String {
    export_document(FORMAT_VERSION, SCENE_VERSION, terrain_cells, props, anchors)
        .replacen(
            &format!(r#""scene_id": "{TEST_SCENE_ID}""#),
            r#""scene_id": "test_template_unit""#,
            1,
        )
        .replacen(
            r#""scene_kind": "instance""#,
            r#""scene_kind": "template""#,
            1,
        )
        .replacen(
            r#""template_definition": null"#,
            &format!(r#""template_definition": {template_definition}"#),
            1,
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

    fn with_merge_profiles(source: String) -> String {
        source.replacen(
            r#"{ "asset_key": "grass", "surface": "land", "footprint_meters": null, "anchor_meters": null }"#,
            r#"{ "asset_key": "grass", "surface": "land", "footprint_meters": null, "anchor_meters": null },
                { "asset_key": "stone", "surface": "stone", "footprint_meters": null, "anchor_meters": null }"#,
            1,
        )
    }

    fn merge_ranks() -> PlacementRanks {
        PlacementRanks::from_entries([("grass", 10), ("tree", 20), ("stone", 30), ("ankh", 100)])
            .expect("the synthetic Placement Ranks are valid")
    }

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
        assert_eq!(map.route_surfaces().len(), 3);
        assert_eq!(
            map.route_surfaces()
                .iter()
                .map(|route| route.segments[0].grade_percent)
                .collect::<Vec<_>>(),
            vec![25, 50, -50]
        );
        assert!(!map.route_surfaces()[0].vertices.is_empty());
        assert!(!map.route_surfaces()[0].boundary_edges.is_empty());
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
    fn route_grades_and_baked_indices_are_validated_at_import() {
        let source =
            embedded_instance_source(TEST_SCENE_ID).expect("the embedded overworld source exists");
        let unsupported_grade = source.replace(r#""grade_percent": 25"#, r#""grade_percent": 49"#);
        let grade_error = WorldMap::from_source(&unsupported_grade, TEST_SCENE_ID)
            .expect_err("an unsupported authored grade must be rejected");
        assert!(grade_error.to_string().contains("authored segment"));

        let mut invalid_index: serde_json::Value =
            serde_json::from_str(source).expect("the embedded export is JSON");
        invalid_index["route_surface_bakes"][0]["triangle_indices"][0] =
            serde_json::Value::from(u32::MAX);
        let invalid_index =
            serde_json::to_string(&invalid_index).expect("the mutated export remains JSON");
        let index_error = WorldMap::from_source(&invalid_index, TEST_SCENE_ID)
            .expect_err("an out-of-range baked index must be rejected");
        assert!(index_error.to_string().contains("triangle index"));
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
    fn embedded_export_metadata_is_valid_and_unique() {
        let mut scene_ids = HashSet::new();
        for (_, scene_id, scene_kind, _) in EMBEDDED_WORLD_EXPORTS {
            assert!(!scene_id.is_empty());
            assert!(matches!(*scene_kind, "instance" | "template"));
            assert!(
                scene_ids.insert(*scene_id),
                "duplicate scene ID: {scene_id}"
            );
        }
    }

    #[test]
    fn embedded_templates_are_grouped_in_stable_order() {
        let catalog = WorldTemplateCatalog::load_embedded()
            .expect("the embedded SceneMaker Templates are valid");
        let group_one = catalog.templates_for_group(1);

        assert_eq!(group_one.len(), 2);
        assert_eq!(group_one[0].scene_id(), "test_template");
        assert_eq!(group_one[1].scene_id(), "test_template02");
        assert_eq!(
            catalog
                .template("test_template02")
                .map(WorldTemplate::scene_id),
            Some("test_template02")
        );
        assert!(catalog.template("not_a_template").is_none());
        assert!(catalog.templates_for_group(99).is_empty());
        assert_eq!(catalog.groups().count(), 1);
    }

    #[test]
    fn template_geometry_keeps_its_bottom_left_local_coordinates_and_gaps() {
        let source = template_export(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            ANKH,
        );
        let template = WorldTemplate::from_source(&source, "test_template_unit")
            .expect("the synthetic Template is valid");

        assert_eq!(template.width_tiles(), 4);
        assert_eq!(template.height_tiles(), 4);
        assert_eq!(template.terrain_cells().len(), 1);
        assert_eq!(template.terrain_cells()[0].center, Position::new(0.5, 0.5));
        assert_eq!(template.props()[0].position, Position::new(2.0, 3.0));
        assert_eq!(template.insertion_anchor(), Position::new(1.0, 1.0));
        assert_eq!(
            template.insertion_anchor_grid(),
            SceneGridPosition { x: 1, y: 1 }
        );
    }

    #[test]
    fn template_projection_translates_geometry_without_merging_it() {
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, ANKH, ANCHOR),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let template_prop = ANKH.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 64, "y": 32 }"#,
        );
        let template = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
                }"#,
                r#"{ "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": 2.5 }"#,
                &template_prop,
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");

        let placement = map
            .project_template("template_anchor_001", &template)
            .expect("the Template fits at the matching Anchor");

        assert_eq!(placement.anchor_id(), "template_anchor_001");
        assert_eq!(placement.template_scene_id(), "test_template_unit");
        assert_eq!(placement.grid_offset(), SceneGridOffset { x: 1, y: 2 });
        assert_eq!(placement.position_offset(), Position::new(-1.0, 0.0));
        assert_eq!(
            (
                placement.terrain_cells()[0].x,
                placement.terrain_cells()[0].y
            ),
            (1, 2)
        );
        assert_eq!(
            placement.terrain_cells()[0].center,
            Position::new(-0.5, 0.5)
        );
        assert_eq!(placement.terrain_cells()[0].elevation_meters, 2.5);
        assert_eq!(placement.props()[0].position, Position::new(1.0, 1.0));
        assert_eq!(placement.props()[0].elevation_meters, 1.0);
        assert_eq!(map.terrain_cells().len(), 1);
        assert_eq!(map.props().len(), 1);
    }

    #[test]
    fn projected_cell_centers_come_from_the_instance_grid() {
        let anchor = ANCHOR.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 0, "y": 0 }"#,
        );
        let map_source = test_export_with_anchors(TEST_GRASS_CELL, "", &anchor)
            .replace(
                r#""terrain_cell_meters": 1.0"#,
                r#""terrain_cell_meters": 0.1"#,
            )
            .replace(
                r#""authoring_pixels_per_meter": 32.0"#,
                r#""authoring_pixels_per_meter": 10.0"#,
            )
            .replace(
                r#""size_cells": { "width": 4, "height": 4 }"#,
                r#""size_cells": { "width": 3, "height": 3 }"#,
            );
        let map = WorldMap::from_source(&map_source, TEST_SCENE_ID)
            .expect("the synthetic Instance is valid");
        let template_source = template_export(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 2, "y": 0 }
            }"#,
            r#"{ "x": 1, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }"#,
            "",
        )
        .replace(
            r#""terrain_cell_meters": 1.0"#,
            r#""terrain_cell_meters": 0.1"#,
        )
        .replace(
            r#""authoring_pixels_per_meter": 32.0"#,
            r#""authoring_pixels_per_meter": 20.0"#,
        )
        .replace(
            r#""size_cells": { "width": 4, "height": 4 }"#,
            r#""size_cells": { "width": 3, "height": 3 }"#,
        );
        let template = WorldTemplate::from_source(&template_source, "test_template_unit")
            .expect("the synthetic Template is valid");

        let placement = map
            .project_template("template_anchor_001", &template)
            .expect("different authoring pixel densities do not affect the shared grid");
        let expected_center = 0.5_f32 * 0.1_f32 - 3.0_f32 * 0.1_f32 * 0.5_f32;

        assert_eq!(placement.terrain_cells()[0].x, 0);
        assert_eq!(placement.terrain_cells()[0].center.x, expected_center);
    }

    #[test]
    fn negative_grid_offsets_are_safe_when_the_mask_still_fits() {
        let anchor = ANCHOR.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 0, "y": 0 }"#,
        );
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, "", &anchor),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let template = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 32, "y": 0 }
                }"#,
                r#"{ "x": 1, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }"#,
                "",
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");

        let placement = map
            .project_template("template_anchor_001", &template)
            .expect("the translated mask remains inside the Instance");

        assert_eq!(placement.grid_offset(), SceneGridOffset { x: -1, y: 0 });
        assert_eq!(
            (
                placement.terrain_cells()[0].x,
                placement.terrain_cells()[0].y
            ),
            (0, 0)
        );
    }

    #[test]
    fn template_projection_rejects_a_mask_outside_the_instance() {
        let anchor = ANCHOR.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 0, "y": 0 }"#,
        );
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, "", &anchor),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let template = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 32, "y": 0 }
                }"#,
                TEST_GRASS_CELL,
                "",
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");

        let error = map
            .project_template("template_anchor_001", &template)
            .expect_err("the translated mask leaves the Instance");

        assert!(error.to_string().contains("cell (0, 0)"));
        assert!(error.to_string().contains("cell (-1, 0)"));
    }

    #[test]
    fn projected_prop_origins_must_stay_within_the_instance_boundary() {
        let anchor = ANCHOR.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 0, "y": 0 }"#,
        );
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, "", &anchor),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let definition = r#"{
            "group_number": 1,
            "insertion_anchor_authoring_px": { "x": 0, "y": 0 }
        }"#;
        let edge_prop = r#"{
            "instance_id": "tree_edge",
            "asset_key": "tree",
            "position_authoring_px": { "x": 0, "y": 0 },
            "elevation_meters": 1.0
        }"#;
        let outside_prop = r#"{
            "instance_id": "tree_outside",
            "asset_key": "tree",
            "position_authoring_px": { "x": 160, "y": 0 },
            "elevation_meters": 1.0
        }"#;
        let edge_template = WorldTemplate::from_source(
            &template_export(definition, TEST_GRASS_CELL, edge_prop),
            "test_template_unit",
        )
        .expect("the synthetic edge Template is valid");
        let outside_template = WorldTemplate::from_source(
            &template_export(definition, TEST_GRASS_CELL, outside_prop),
            "test_template_unit",
        )
        .expect("the synthetic outside Template is valid");

        let edge = map
            .project_template("template_anchor_001", &edge_template)
            .expect("a Prop origin may lie on the Instance boundary");
        let outside = map
            .project_template("template_anchor_001", &outside_template)
            .expect_err("a Prop origin may not leave the Instance");

        assert_eq!(edge.props()[0].position, Position::new(-2.0, -2.0));
        assert!(outside.to_string().contains("tree_outside"));
    }

    #[test]
    fn template_projection_rejects_unknown_or_incompatible_choices() {
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, "", ANCHOR),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let wrong_group = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 2,
                    "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
                }"#,
                TEST_GRASS_CELL,
                "",
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");
        let wrong_cell_size = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
                }"#,
                TEST_GRASS_CELL,
                "",
            )
            .replacen(
                r#""terrain_cell_meters": 1.0"#,
                r#""terrain_cell_meters": 0.5"#,
                1,
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is internally valid");

        assert!(
            map.project_template("missing_anchor", &wrong_group)
                .is_err()
        );
        assert!(
            map.project_template("template_anchor_001", &wrong_group)
                .is_err()
        );
        assert!(
            map.project_template("template_anchor_001", &wrong_cell_size)
                .is_err()
        );
    }

    #[test]
    fn template_merge_resolves_ranks_and_preserves_stable_order() {
        let map_cells = r#"
            { "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 },
            { "x": 1, "y": 0, "asset_key": "stone", "elevation_meters": 4.0 },
            { "x": 2, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }
        "#;
        let map_props = r#"
            {
                "instance_id": "tree_removed",
                "asset_key": "tree",
                "position_authoring_px": { "x": 16, "y": 16 },
                "elevation_meters": 1.0
            },
            {
                "instance_id": "ankh_kept",
                "asset_key": "ankh",
                "position_authoring_px": { "x": 48, "y": 16 },
                "elevation_meters": 1.0
            },
            {
                "instance_id": "tree_kept",
                "asset_key": "tree",
                "position_authoring_px": { "x": 80, "y": 16 },
                "elevation_meters": 1.0
            },
            {
                "instance_id": "outside_mask",
                "asset_key": "tree",
                "position_authoring_px": { "x": 16, "y": 48 },
                "elevation_meters": 1.0
            }
        "#;
        let map = WorldMap::from_source(
            &with_merge_profiles(test_export_with_anchors(map_cells, map_props, ANCHOR)),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let template_cells = r#"
            { "x": 0, "y": 0, "asset_key": "stone", "elevation_meters": 2.0 },
            { "x": 1, "y": 0, "asset_key": "grass", "elevation_meters": 2.0 },
            { "x": 2, "y": 0, "asset_key": "grass", "elevation_meters": 3.0 },
            { "x": 3, "y": 0, "asset_key": "grass", "elevation_meters": 4.0 }
        "#;
        let template_props = r#"{
            "instance_id": "tree_new",
            "asset_key": "tree",
            "position_authoring_px": { "x": 112, "y": 16 },
            "elevation_meters": 3.0
        }"#;
        let template = WorldTemplate::from_source(
            &with_merge_profiles(template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 64, "y": 96 }
                }"#,
                template_cells,
                template_props,
            )),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");
        let placement = map
            .project_template("template_anchor_001", &template)
            .expect("the synthetic Template fits");
        let original_map = map.clone();
        let original_placement = placement.clone();

        let merged = map
            .merged_with(&placement, &merge_ranks())
            .expect("the ranks resolve the synthetic placement");
        let merged_again = map
            .merged_with(&placement, &merge_ranks())
            .expect("the same inputs resolve deterministically");

        assert_eq!(merged, merged_again);
        assert_eq!(map, original_map);
        assert_eq!(placement, original_placement);
        assert_eq!(
            merged
                .terrain_cells()
                .iter()
                .map(|cell| {
                    (
                        cell.x,
                        cell.asset_key.as_str(),
                        cell.surface.as_str(),
                        cell.elevation_meters,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                (0, "stone", "stone", 2.0),
                (1, "stone", "stone", 4.0),
                (2, "grass", "land", 3.0),
                (3, "grass", "land", 4.0),
            ]
        );
        assert_eq!(
            merged
                .props()
                .iter()
                .map(|prop| prop.instance_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "ankh_kept",
                "tree_kept",
                "outside_mask",
                "template.template_anchor_001.test_template_unit.tree_new",
            ]
        );
        assert_eq!(merged.props()[3].position, Position::new(1.5, -1.5));
        assert_eq!(merged.props()[3].elevation_meters, 3.0);
    }

    #[test]
    fn terrain_mask_uses_full_prop_footprints_but_allows_touching_edges() {
        let map_props = r#"
            {
                "instance_id": "crosses_into_mask",
                "asset_key": "tree",
                "position_authoring_px": { "x": 24, "y": 16 },
                "elevation_meters": 1.0
            },
            {
                "instance_id": "touches_mask_edge",
                "asset_key": "tree",
                "position_authoring_px": { "x": 80, "y": 16 },
                "elevation_meters": 1.0
            }
        "#;
        let map = WorldMap::from_source(
            &with_merge_profiles(test_export_with_anchors(TEST_GRASS_CELL, map_props, ANCHOR)),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let template = WorldTemplate::from_source(
            &with_merge_profiles(template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 64, "y": 96 }
                }"#,
                r#"{ "x": 1, "y": 0, "asset_key": "stone", "elevation_meters": 2.0 }"#,
                "",
            )),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");
        let placement = map
            .project_template("template_anchor_001", &template)
            .expect("the synthetic Template fits");

        let merged = map
            .merged_with(&placement, &merge_ranks())
            .expect("footprints resolve against the Terrain mask");

        assert_eq!(
            merged
                .props()
                .iter()
                .map(|prop| prop.instance_id.as_str())
                .collect::<Vec<_>>(),
            vec!["touches_mask_edge"]
        );
    }

    #[test]
    fn template_props_replace_overlaps_by_rank_and_a_higher_blocker_wins_whole() {
        let map_props = r#"
            {
                "instance_id": "tree_existing",
                "asset_key": "tree",
                "position_authoring_px": { "x": 64, "y": 64 },
                "elevation_meters": 1.0
            },
            {
                "instance_id": "ankh_existing",
                "asset_key": "ankh",
                "position_authoring_px": { "x": 96, "y": 64 },
                "elevation_meters": 1.0
            }
        "#;
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, map_props, ANCHOR),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let blocked_template = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 64, "y": 96 }
                }"#,
                "",
                r#"{
                    "instance_id": "tree_blocked",
                    "asset_key": "tree",
                    "position_authoring_px": { "x": 80, "y": 64 },
                    "elevation_meters": 2.0
                }"#,
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");
        let blocked_placement = map
            .project_template("template_anchor_001", &blocked_template)
            .expect("the synthetic Template fits");
        let blocked = map
            .merged_with(&blocked_placement, &merge_ranks())
            .expect("the higher-ranked Ankh blocks the incoming Tree");

        assert_eq!(blocked.props(), map.props());

        let winning_template = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 64, "y": 96 }
                }"#,
                "",
                r#"{
                    "instance_id": "ankh_wins",
                    "asset_key": "ankh",
                    "position_authoring_px": { "x": 64, "y": 64 },
                    "elevation_meters": 2.0
                }"#,
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");
        let winning_placement = map
            .project_template("template_anchor_001", &winning_template)
            .expect("the synthetic Template fits");
        let winning = map
            .merged_with(&winning_placement, &merge_ranks())
            .expect("the incoming Ankh replaces the lower-ranked Tree");

        assert_eq!(
            winning
                .props()
                .iter()
                .map(|prop| prop.instance_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "ankh_existing",
                "template.template_anchor_001.test_template_unit.ankh_wins",
            ]
        );
    }

    #[test]
    fn template_merge_rejects_missing_ranks_and_namespaced_id_collisions() {
        let colliding_id = "template.template_anchor_001.test_template_unit.tree_new";
        let map_prop = format!(
            r#"{{
                "instance_id": "{colliding_id}",
                "asset_key": "ankh",
                "position_authoring_px": {{ "x": 16, "y": 48 }},
                "elevation_meters": 1.0
            }}"#
        );
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, &map_prop, ANCHOR),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let template = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 64, "y": 96 }
                }"#,
                TEST_GRASS_CELL,
                r#"{
                    "instance_id": "tree_new",
                    "asset_key": "tree",
                    "position_authoring_px": { "x": 16, "y": 16 },
                    "elevation_meters": 1.0
                }"#,
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");
        let placement = map
            .project_template("template_anchor_001", &template)
            .expect("the synthetic Template fits");
        let incomplete = PlacementRanks::from_entries([("grass", 10), ("tree", 20)])
            .expect("the partial catalog itself is well formed");

        let missing = map
            .merged_with(&placement, &incomplete)
            .expect_err("the Instance Ankh has no rank");
        let collision = map
            .merged_with(&placement, &merge_ranks())
            .expect_err("the generated ID collides with an Instance Prop");

        assert!(missing.to_string().contains("ankh"));
        assert!(collision.to_string().contains(colliding_id));
    }

    #[test]
    fn template_merge_rejects_a_placement_from_another_instance() {
        let map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, "", ANCHOR),
            TEST_SCENE_ID,
        )
        .expect("the synthetic Instance is valid");
        let other_source = test_export_with_anchors(TEST_GRASS_CELL, "", ANCHOR)
            .replace(r#""scene_id": "overworld01""#, r#""scene_id": "cave01""#);
        let other = WorldMap::from_source(&other_source, "cave01")
            .expect("the other synthetic Instance is valid");
        let template = WorldTemplate::from_source(
            &template_export(
                r#"{
                    "group_number": 1,
                    "insertion_anchor_authoring_px": { "x": 64, "y": 96 }
                }"#,
                TEST_GRASS_CELL,
                "",
            ),
            "test_template_unit",
        )
        .expect("the synthetic Template is valid");
        let foreign_placement = other
            .project_template("template_anchor_001", &template)
            .expect("the Template fits the other Instance");

        let error = map
            .merged_with(&foreign_placement, &merge_ranks())
            .expect_err("a placement is bound to its source Instance");

        assert_eq!(map.scene_id(), "overworld01");
        assert_eq!(foreign_placement.instance_scene_id(), "cave01");
        assert!(error.to_string().contains("cave01"));
        assert!(error.to_string().contains("overworld01"));
    }

    #[test]
    fn placement_rank_entries_must_have_unique_nonempty_asset_keys() {
        assert!(PlacementRanks::from_entries(std::iter::empty::<(&str, u32)>()).is_err());
        assert!(PlacementRanks::from_entries([("", 1)]).is_err());
        assert!(PlacementRanks::from_entries([("grass", 1), ("grass", 2)]).is_err());
        assert_eq!(
            PlacementRanks::from_entries([("grass", 10)])
                .expect("the catalog is valid")
                .rank("grass"),
            Some(10)
        );
    }

    #[test]
    fn an_instance_or_a_template_without_definition_is_rejected_as_a_template() {
        assert!(
            WorldTemplate::from_source(&test_export(TEST_GRASS_CELL, ""), TEST_SCENE_ID).is_err()
        );

        let missing_definition = template_export("null", TEST_GRASS_CELL, "");
        assert!(WorldTemplate::from_source(&missing_definition, "test_template_unit").is_err());
    }

    #[test]
    fn instances_with_definitions_and_templates_with_anchors_are_rejected() {
        let instance_with_definition = test_export(TEST_GRASS_CELL, "").replacen(
            r#""template_definition": null"#,
            r#""template_definition": {
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
            }"#,
            1,
        );
        let template_with_anchor = template_export_with_anchors(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            "",
            ANCHOR,
        );

        let instance_error = WorldMap::from_source(&instance_with_definition, TEST_SCENE_ID)
            .expect_err("an Instance cannot carry a Template definition");
        let template_error =
            WorldTemplate::from_source(&template_with_anchor, "test_template_unit")
                .expect_err("a Template cannot carry Template Anchors");

        assert!(instance_error.to_string().contains("must not carry"));
        assert!(template_error.to_string().contains("must not carry"));
    }

    #[test]
    fn template_insertion_anchor_must_be_inside_and_on_the_cell_grid() {
        let outside = template_export(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 160, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            "",
        );
        let off_grid = template_export(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 31, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            "",
        );

        assert!(WorldTemplate::from_source(&outside, "test_template_unit").is_err());
        assert!(WorldTemplate::from_source(&off_grid, "test_template_unit").is_err());

        let on_edge = template_export(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 128, "y": 128 }
            }"#,
            TEST_GRASS_CELL,
            "",
        );
        assert!(WorldTemplate::from_source(&on_edge, "test_template_unit").is_ok());
    }

    #[test]
    fn template_group_number_must_be_positive() {
        let source = template_export(
            r#"{
                "group_number": 0,
                "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            "",
        );

        assert!(WorldTemplate::from_source(&source, "test_template_unit").is_err());
    }

    #[test]
    fn catalog_import_errors_name_the_template() {
        let source = template_export(
            r#"{
                "group_number": 0,
                "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            "",
        );

        let error = import_catalog_template("test_template_unit", &source)
            .expect_err("the invalid Template must be rejected");
        assert!(error.to_string().contains("test_template_unit"));
    }

    #[test]
    fn templates_with_water_are_rejected() {
        let source = template_export(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            "",
        );
        let raster = source.replacen(r#""water_raster": []"#, r#""water_raster": [{}]"#, 1);
        let body = source.replacen(r#""water_bodies": []"#, r#""water_bodies": [{}]"#, 1);

        assert!(WorldTemplate::from_source(&raster, "test_template_unit").is_err());
        assert!(WorldTemplate::from_source(&body, "test_template_unit").is_err());
    }

    #[test]
    fn templates_with_route_surfaces_are_explicitly_rejected() {
        let source = template_export(
            r#"{
                "group_number": 1,
                "insertion_anchor_authoring_px": { "x": 32, "y": 32 }
            }"#,
            TEST_GRASS_CELL,
            "",
        );
        let authored = source.replacen(
            r#""route_surfaces": []"#,
            r#""route_surfaces": [{
                "route_surface_id": "route_0001",
                "asset_key": "grass",
                "points": [],
                "segments": []
            }]"#,
            1,
        );
        let baked = source.replacen(
            r#""route_surface_bakes": []"#,
            r#""route_surface_bakes": [{
                "route_surface_id": "route_0001",
                "asset_key": "grass",
                "vertices": [],
                "triangle_indices": [],
                "boundary_edges": [],
                "centerline_samples": [],
                "segments": []
            }]"#,
            1,
        );

        let authored_error = WorldTemplate::from_source(&authored, "test_template_unit")
            .expect_err("a Template Path is not composed yet");
        assert!(authored_error.to_string().contains("route surfaces"));
        let baked_error = WorldTemplate::from_source(&baked, "test_template_unit")
            .expect_err("a Template Path bake is not composed yet");
        assert!(baked_error.to_string().contains("route surfaces"));
    }

    #[test]
    fn obsolete_export_and_scene_versions_are_rejected() {
        assert!(
            WorldMap::from_source(
                &export_document(9, 11, TEST_GRASS_CELL, "", ""),
                TEST_SCENE_ID
            )
            .is_err()
        );
        assert!(
            WorldMap::from_source(
                &export_document(10, 10, TEST_GRASS_CELL, "", ""),
                TEST_SCENE_ID
            )
            .is_err()
        );
    }

    #[test]
    fn authoring_pixels_become_positions_around_the_map_centre() {
        let source = test_export(TEST_GRASS_CELL, ANKH)
            .replacen(
                r#""width": 1.0, "height": 1.0"#,
                r#""width": 2.0, "height": 1.0"#,
                1,
            )
            .replacen(r#""x": 0.5, "y": 0.5"#, r#""x": 0.25, "y": 0.75"#, 1);
        let map =
            WorldMap::from_source(&source, TEST_SCENE_ID).expect("the synthetic export is valid");

        assert_eq!(map.terrain_cells()[0].center, Position::new(-1.5, -1.5));
        assert_eq!(map.props()[0].position, Position::new(0.0, 1.0));
        assert_eq!(map.props()[0].footprint.width_meters(), 2.0);
        assert_eq!(map.props()[0].footprint.height_meters(), 1.0);
        assert_eq!(map.props()[0].footprint.anchor(), Position::new(0.25, 0.75));
        let bounds = map.props()[0].footprint.bounds_at(map.props()[0].position);
        assert_eq!(bounds.left, -0.25);
        assert_eq!(bounds.right, 1.75);
        assert_eq!(bounds.bottom, 0.25);
        assert_eq!(bounds.top, 1.25);
    }

    #[test]
    fn props_require_a_finite_positive_footprint_and_an_anchor_inside_it() {
        let missing = test_export(TEST_GRASS_CELL, ANKH).replacen(
            r#""footprint_meters": { "width": 1.0, "height": 1.0 }"#,
            r#""footprint_meters": null"#,
            1,
        );
        let zero_width = test_export(TEST_GRASS_CELL, ANKH).replacen(
            r#""width": 1.0, "height": 1.0"#,
            r#""width": 0.0, "height": 1.0"#,
            1,
        );
        let outside_anchor = test_export(TEST_GRASS_CELL, ANKH).replacen(
            r#""x": 0.5, "y": 0.5"#,
            r#""x": 1.5, "y": 0.5"#,
            1,
        );

        assert!(WorldMap::from_source(&missing, TEST_SCENE_ID).is_err());
        assert!(WorldMap::from_source(&zero_width, TEST_SCENE_ID).is_err());
        assert!(WorldMap::from_source(&outside_anchor, TEST_SCENE_ID).is_err());
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
        assert_eq!(
            map.template_anchors()[0].grid_position,
            SceneGridPosition { x: 2, y: 3 }
        );
    }

    #[test]
    fn template_anchors_require_a_positive_group_and_a_valid_grid_position() {
        let zero_group = ANCHOR.replace(r#""group_number": 1"#, r#""group_number": 0"#);
        let off_grid = ANCHOR.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 63, "y": 96 }"#,
        );
        let outside = ANCHOR.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 160, "y": 96 }"#,
        );
        let on_edge = ANCHOR.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 128, "y": 128 }"#,
        );

        assert!(
            WorldMap::from_source(
                &test_export_with_anchors(TEST_GRASS_CELL, "", &zero_group),
                TEST_SCENE_ID,
            )
            .is_err()
        );
        assert!(
            WorldMap::from_source(
                &test_export_with_anchors(TEST_GRASS_CELL, "", &off_grid),
                TEST_SCENE_ID,
            )
            .is_err()
        );
        assert!(
            WorldMap::from_source(
                &test_export_with_anchors(TEST_GRASS_CELL, "", &outside),
                TEST_SCENE_ID,
            )
            .is_err()
        );
        let edge_map = WorldMap::from_source(
            &test_export_with_anchors(TEST_GRASS_CELL, "", &on_edge),
            TEST_SCENE_ID,
        )
        .expect("an Anchor may lie on the Scene edge");
        assert_eq!(
            edge_map.template_anchors()[0].grid_position,
            SceneGridPosition { x: 4, y: 4 }
        );
    }

    #[test]
    fn template_anchor_ids_must_be_unique() {
        let duplicated = format!("{ANCHOR}, {ANCHOR}");
        let empty = ANCHOR.replace("template_anchor_001", "");

        assert!(
            WorldMap::from_source(
                &test_export_with_anchors(TEST_GRASS_CELL, "", &duplicated),
                TEST_SCENE_ID,
            )
            .is_err()
        );
        assert!(
            WorldMap::from_source(
                &test_export_with_anchors(TEST_GRASS_CELL, "", &empty),
                TEST_SCENE_ID,
            )
            .is_err()
        );
    }

    #[test]
    fn instance_prop_origins_must_stay_within_the_scene_boundary() {
        let edge = ANKH.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 128, "y": 128 }"#,
        );
        let outside = ANKH.replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 160, "y": 128 }"#,
        );

        assert!(WorldMap::from_source(&test_export(TEST_GRASS_CELL, &edge), TEST_SCENE_ID).is_ok());
        assert!(
            WorldMap::from_source(&test_export(TEST_GRASS_CELL, &outside), TEST_SCENE_ID).is_err()
        );
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

    #[test]
    fn authored_props_may_touch_but_must_not_overlap() {
        let first = ANKH.replace("ankh_0001", "ankh_first");
        let touching = ANKH.replace("ankh_0001", "ankh_touching").replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 96, "y": 96 }"#,
        );
        let overlapping = ANKH.replace("ankh_0001", "ankh_overlapping").replace(
            r#""position_authoring_px": { "x": 64, "y": 96 }"#,
            r#""position_authoring_px": { "x": 80, "y": 96 }"#,
        );

        assert!(
            WorldMap::from_source(
                &test_export(TEST_GRASS_CELL, &format!("{first}, {touching}")),
                TEST_SCENE_ID,
            )
            .is_ok()
        );
        let error = WorldMap::from_source(
            &test_export(TEST_GRASS_CELL, &format!("{first}, {overlapping}")),
            TEST_SCENE_ID,
        )
        .expect_err("SceneMaker-authored Prop footprints must not overlap");
        assert!(error.to_string().contains("ankh_first"));
        assert!(error.to_string().contains("ankh_overlapping"));
    }
}
