use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
use serde::Deserialize;

use crate::{Position, WorldPosition};

const FORMAT: &str = "scene_maker_scene_export";
const FORMAT_VERSION: u32 = 22;
const SCENE_SCHEMA: &str = "srt.scene_maker_scene";
const SCENE_VERSION: u32 = 17;
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
    terrain_cell_indices: Vec<Option<usize>>,
    props: Vec<MapProp>,
    route_surfaces: Vec<MapRouteSurface>,
    route_surface_cuts: Vec<MapRouteSurfaceCut>,
    bridges: Vec<MapBridge>,
    bridge_decks: Vec<MapRouteSurface>,
    water_bodies: Vec<MapWaterBody>,
    switches: Vec<MapSwitch>,
    /// Where each switch stands now, which starts at what the map declares.
    ///
    /// This is runtime state living on the composed world rather than beside
    /// it, because the column has to answer with it and every place that asks
    /// the column would otherwise have to carry it along.
    switch_positions: HashMap<String, bool>,
    /// The bodies that exist at those positions, resolved through the feeding
    /// chain.
    present_bodies: HashSet<String>,
    terrain_cuts: HashMap<(u32, u32), Vec<TerrainCut>>,
    water_columns: HashMap<(u32, u32), WaterColumn>,
    water_cell_meters: f32,
    template_anchors: Vec<MapTemplateAnchor>,
}

/// One height range the authored world takes out of a Terrain column.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TerrainCut {
    floor_meters: f32,
    cut_top_meters: f32,
}

/// The water standing in one cell, and the channel it was given.
///
/// More than one body may lie over the same cell - every junction is such a
/// place, and a river may cross another entirely - so a cell holds all of them
/// rather than whichever was read last. Fills that meet are one body of water
/// and merge; fills with air between them stay apart, which is what an aqueduct
/// over a river is.
///
/// The cuts are kept beside the fills instead of inside them, because the two
/// do not have to line up: in `stack01` the deep river's cut reaches through
/// the height where the upper fill stands, and that upper fill survives it. A
/// cut takes Terrain away and never water.
#[derive(Debug, Clone, PartialEq, Default)]
struct WaterColumn {
    fills: Vec<WaterFill>,
    cuts: Vec<TerrainCut>,
}

/// One span of standing water, from the bed it lies on to its surface.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WaterFill {
    bed_meters: f32,
    surface_meters: f32,
}

impl WaterColumn {
    /// How deep the water over a surface at this height stands, and zero where
    /// no fill reaches it.
    ///
    /// Only the span a fill actually occupies carries water. Ground below a
    /// bed is dry, not drowned: the export says where the water is, and a hole
    /// somebody dug under a river is not something the map claims to fill.
    fn depth_over(&self, elevation_meters: f32) -> f32 {
        self.fills
            .iter()
            .find(|fill| {
                fill.bed_meters <= elevation_meters && elevation_meters < fill.surface_meters
            })
            .map(|fill| fill.surface_meters - elevation_meters)
            .unwrap_or_default()
    }
}

impl WorldMap {
    /// Loads the embedded Instance named `scene_id`.
    ///
    /// Every synchronized SceneMaker export is embedded, including Templates,
    /// but only an explicitly requested Instance may become a `WorldMap`. A
    /// scene ID identifies a Scene across the whole authored world: SceneMaker
    /// authors one flat set of Scenes, and which Realm a Scene is played in is
    /// a decision this simulation makes, not something the export carries.
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

        let terrain_cell_indices = build_terrain_cell_indices(
            scene.width_tiles,
            scene.height_tiles,
            &scene.terrain_cells,
        )?;
        let switch_positions: HashMap<String, bool> = scene
            .switches
            .iter()
            .map(|switch| (switch.name.clone(), switch.initially_on))
            .collect();
        let present_bodies = resolve_present_bodies(&scene.water_bodies, &switch_positions);
        let water_columns = index_water_columns(&scene.water_bodies, &present_bodies);
        Ok(Self {
            scene_id: scene_id.to_owned(),
            width_tiles: scene.width_tiles,
            height_tiles: scene.height_tiles,
            terrain_cell_meters: scene.terrain_cell_meters,
            terrain_cells: scene.terrain_cells,
            terrain_cell_indices,
            props: scene.props,
            route_surfaces: scene.route_surfaces,
            bridges: scene.bridges,
            bridge_decks: scene.bridge_decks,
            switch_positions,
            present_bodies,
            water_columns,
            water_bodies: scene.water_bodies,
            switches: scene.switches,
            terrain_cuts: index_terrain_cuts(&scene.route_surface_cuts),
            route_surface_cuts: scene.route_surface_cuts,
            water_cell_meters: scene.water_cell_meters,
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

    /// Returns the authored Terrain cell containing a world-space point.
    ///
    /// Missing cells remain missing rather than inheriting a default surface
    /// or elevation. Points on the map's maximum boundary are out of bounds.
    pub fn terrain_cell_at(&self, position: Position) -> Option<&MapTerrainCell> {
        if !position.x.is_finite() || !position.y.is_finite() {
            return None;
        }
        let minimum_x = -self.width_meters() / 2.0;
        let minimum_y = -self.height_meters() / 2.0;
        let x = ((position.x - minimum_x) / self.terrain_cell_meters).floor();
        let y = ((position.y - minimum_y) / self.terrain_cell_meters).floor();
        if x < 0.0 || y < 0.0 || x >= self.width_tiles as f32 || y >= self.height_tiles as f32 {
            return None;
        }
        self.terrain_cell(x as u32, y as u32)
    }

    /// Returns one authored Terrain cell by its stable row-major coordinates.
    pub fn terrain_cell(&self, x: u32, y: u32) -> Option<&MapTerrainCell> {
        terrain_cell_index(self.width_tiles, self.height_tiles, x, y)
            .and_then(|index| self.terrain_cell_indices.get(index).copied().flatten())
            .and_then(|index| self.terrain_cells.get(index))
    }

    /// Fills `surfaces` with the Terrain walking surfaces at a point, lowest
    /// first, and clears whatever was there before.
    ///
    /// A cell's `elevation_meters` is the top of its solid column rather than
    /// necessarily a walking surface: an authored Path may take a range out of
    /// that column, and every maximal run of solid that survives presents a
    /// surface at its top. A point under an excavated hill therefore reports
    /// two, the floor of the excavation and the ground still standing above it.
    ///
    /// The column is resolved at the point asked for, never voted on across a
    /// cell, because the authored excavation is finer than one Terrain cell.
    ///
    /// Standing water adds no surface of its own. It is reported as a depth
    /// over the ground it covers, and how much of it a Character may walk
    /// through is that Character's own rule, the same way the height of a step
    /// is.
    pub fn terrain_walking_surfaces<'a>(
        &'a self,
        position: Position,
        surfaces: &mut Vec<MapColumnSurface<'a>>,
    ) {
        surfaces.clear();
        let Some(cell) = self.terrain_cell_at(position) else {
            return;
        };
        let water = self.water_column_at(position);
        let mut cuts = self
            .terrain_cuts_at(position)
            .map(<[TerrainCut]>::to_vec)
            .unwrap_or_default();
        // Standing water takes the same kind of range out of the column an
        // excavating Path does: down to the bed it was cut to, up to the height
        // that keeps the channel open. Every body over this cell takes its own,
        // and the merge below unites them.
        if let Some(water) = water {
            cuts.extend(water.cuts.iter().copied());
            cuts.sort_by(|first, second| {
                first
                    .floor_meters
                    .total_cmp(&second.floor_meters)
                    .then_with(|| first.cut_top_meters.total_cmp(&second.cut_top_meters))
            });
        }

        // How deep the standing water lies over a surface at this height, and
        // zero where no fill over this cell reaches it.
        let depth_over =
            |elevation_meters: f32| water.map_or(0.0, |water| water.depth_over(elevation_meters));

        let mut ceiling = cell.elevation_meters;
        let mut merged: Vec<TerrainCut> = Vec::new();
        for cut in cuts.iter().filter(|cut| cut.floor_meters < ceiling) {
            match merged.last_mut() {
                Some(last) if cut.floor_meters <= last.cut_top_meters => {
                    last.cut_top_meters = last.cut_top_meters.max(cut.cut_top_meters);
                }
                _ => merged.push(*cut),
            }
        }
        for cut in merged.iter().rev() {
            if cut.cut_top_meters < ceiling {
                surfaces.push(MapColumnSurface {
                    elevation_meters: ceiling,
                    surface: &cell.surface,
                    water_depth_meters: depth_over(ceiling),
                });
            }
            ceiling = cut.floor_meters;
        }
        surfaces.push(MapColumnSurface {
            elevation_meters: ceiling,
            surface: &cell.surface,
            water_depth_meters: depth_over(ceiling),
        });
        surfaces.reverse();
    }

    fn water_column_at(&self, position: Position) -> Option<&WaterColumn> {
        if self.water_columns.is_empty() {
            return None;
        }
        let minimum_x = -self.width_meters() / 2.0;
        let minimum_y = -self.height_meters() / 2.0;
        let x = ((position.x - minimum_x) / self.water_cell_meters).floor();
        let y = ((position.y - minimum_y) / self.water_cell_meters).floor();
        if x < 0.0 || y < 0.0 {
            return None;
        }
        self.water_columns.get(&(x as u32, y as u32))
    }

    fn terrain_cuts_at(&self, position: Position) -> Option<&[TerrainCut]> {
        if self.terrain_cuts.is_empty() {
            return None;
        }
        let minimum_x = -self.width_meters() / 2.0;
        let minimum_y = -self.height_meters() / 2.0;
        let x = ((position.x - minimum_x) / self.water_cell_meters).floor();
        let y = ((position.y - minimum_y) / self.water_cell_meters).floor();
        if x < 0.0 || y < 0.0 {
            return None;
        }
        self.terrain_cuts
            .get(&(x as u32, y as u32))
            .map(Vec::as_slice)
    }

    /// Places a point at the authored elevation of its containing Terrain cell.
    pub fn terrain_world_position_at(&self, position: Position) -> Option<WorldPosition> {
        self.terrain_cell_at(position)
            .map(|cell| WorldPosition::new(position.x, position.y, cell.elevation_meters))
    }

    pub fn props(&self) -> &[MapProp] {
        &self.props
    }

    /// Independently elevated Path surfaces, already tessellated by SceneMaker.
    pub fn route_surfaces(&self) -> &[MapRouteSurface] {
        &self.route_surfaces
    }

    /// The finer grid the authored volumetric rasters use, in meters.
    pub const fn water_cell_meters(&self) -> f32 {
        self.water_cell_meters
    }

    /// The Terrain each excavating Path removes, one entry per such Path.
    ///
    /// A purely additive Path is absent rather than present and empty, and an
    /// excavation that falls outside the Scene leaves its Path with no cells.
    pub fn route_surface_cuts(&self) -> &[MapRouteSurfaceCut] {
        &self.route_surface_cuts
    }

    /// The authored bridges, each already laid out by SceneMaker.
    pub fn bridges(&self) -> &[MapBridge] {
        &self.bridges
    }

    /// The decks of those bridges, as the Paths they are baked from.
    pub fn bridge_decks(&self) -> &[MapRouteSurface] {
        &self.bridge_decks
    }

    /// The authored water of this map, each body with the cells it occupies.
    pub fn water_bodies(&self) -> &[MapWaterBody] {
        &self.water_bodies
    }

    /// The switches this map declares, and the position each opens in.
    ///
    /// Where a switch stands later is runtime state. The map only says which
    /// switches exist, so that nothing can be switched that was never authored.
    pub fn switches(&self) -> &[MapSwitch] {
        &self.switches
    }

    /// Where a switch stands now, or `None` for one this map never declared.
    pub fn switch_is_on(&self, name: &str) -> Option<bool> {
        self.switch_positions.get(name).copied()
    }

    /// Moves a switch on this map, and answers whether anything changed.
    ///
    /// Everything that hangs on the switch follows at once: a body whose switch
    /// went off carries neither its fill nor its cut from here on, and neither
    /// does anything fed by it. The column therefore has to be re-indexed, which
    /// is why this is the only way to move one.
    ///
    /// It moves the switch on *this* map and does nothing else, which is not
    /// enough to flip one while a world runs. Three things stay with the
    /// caller, and none of them is visible from here:
    ///
    /// - **The server owns the flip.** Server and client each load their own
    ///   copy of a map, so moving a switch on one side alone leaves water on
    ///   one machine and whole ground on the other, without anything reporting
    ///   a disagreement.
    /// - **The positions have to reach the clients**, or their water and the
    ///   server's stop being the same world.
    /// - **The derived world has to be rebuilt.** Collision and the navigation
    ///   graph come from the column, and the column just changed; until
    ///   `rebuild_world_runtime` runs again, whatever walks does so on the nodes
    ///   of the world as it was.
    ///
    /// So this is not the door a trigger uses. It is how a composition applies
    /// the positions it holds after rebuilding this map from the authored base;
    /// `WorldComposition::set_switch` is what moves one, and it is the only
    /// thing outside this crate that can.
    pub(crate) fn set_switch(&mut self, name: &str, on: bool) -> Result<bool, WorldMapError> {
        let Some(position) = self.switch_positions.get_mut(name) else {
            return Err(WorldMapError::new(format!(
                "this map declares no switch '{name}'"
            )));
        };
        if *position == on {
            return Ok(false);
        }
        *position = on;
        self.present_bodies = resolve_present_bodies(&self.water_bodies, &self.switch_positions);
        self.water_columns = index_water_columns(&self.water_bodies, &self.present_bodies);
        Ok(true)
    }

    /// Whether a body of water is there at the switch positions of the moment.
    ///
    /// A body that is not there carries nothing at all: no water to be in, and
    /// no channel either, so the Terrain stands as though it had never been
    /// authored.
    pub fn water_body_is_present(&self, water_body_id: &str) -> bool {
        self.present_bodies.contains(water_body_id)
    }

    /// The bodies of water that are there, in authored order.
    pub fn present_water_bodies(&self) -> impl Iterator<Item = &MapWaterBody> {
        self.water_bodies
            .iter()
            .filter(|body| self.present_bodies.contains(&body.water_body_id))
    }

    /// Every authored surface a Character walks along rather than over: the
    /// Paths and the bridge decks, which are the same thing to whoever walks.
    pub fn walked_surfaces(&self) -> impl Iterator<Item = &MapRouteSurface> {
        self.route_surfaces.iter().chain(self.bridge_decks.iter())
    }

    /// An authored Path or a bridge deck, by the ID its nodes carry.
    pub fn route_surface(&self, route_surface_id: &str) -> Option<&MapRouteSurface> {
        self.bridge_decks
            .binary_search_by_key(&route_surface_id, |deck| deck.route_surface_id.as_str())
            .ok()
            .and_then(|index| self.bridge_decks.get(index))
            .or_else(|| self.authored_route_surface(route_surface_id))
    }

    fn authored_route_surface(&self, route_surface_id: &str) -> Option<&MapRouteSurface> {
        self.route_surfaces
            .binary_search_by_key(&route_surface_id, |surface| {
                surface.route_surface_id.as_str()
            })
            .ok()
            .and_then(|index| self.route_surfaces.get(index))
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

        let terrain_cell_indices =
            build_terrain_cell_indices(self.width_tiles, self.height_tiles, &terrain_cells)?;
        Ok(Self {
            scene_id: self.scene_id.clone(),
            width_tiles: self.width_tiles,
            height_tiles: self.height_tiles,
            terrain_cell_meters: self.terrain_cell_meters,
            terrain_cells,
            terrain_cell_indices,
            props,
            route_surfaces: self.route_surfaces.clone(),
            route_surface_cuts: self.route_surface_cuts.clone(),
            // A Template carries no bridges, so composition keeps the ones the
            // Instance was authored with.
            bridges: self.bridges.clone(),
            bridge_decks: self.bridge_decks.clone(),
            // A Template carries no water yet, so composition keeps the water
            // the Instance was authored with.
            water_bodies: self.water_bodies.clone(),
            switches: self.switches.clone(),
            switch_positions: self.switch_positions.clone(),
            present_bodies: self.present_bodies.clone(),
            terrain_cuts: self.terrain_cuts.clone(),
            water_columns: self.water_columns.clone(),
            water_cell_meters: self.water_cell_meters,
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

    /// Adds more entries to this table, refusing to touch any Asset key it
    /// already ranks.
    ///
    /// An overlay may only *add* Placement Ranks, never override a scalar
    /// value the base table already claims - the base table is what an
    /// overlay's author cannot see all of, so silently replacing one of its
    /// entries would be a decision the overlay never actually made.
    pub fn extended_with<I, S>(mut self, entries: I) -> Result<Self, WorldMapError>
    where
        I: IntoIterator<Item = (S, u32)>,
        S: Into<String>,
    {
        for (asset_key, rank) in entries {
            let asset_key = asset_key.into();
            if asset_key.is_empty() || self.ranks.insert(asset_key.clone(), rank).is_some() {
                return Err(WorldMapError::new(format!(
                    "Placement Rank Asset key '{asset_key}' is empty or duplicated"
                )));
            }
        }
        Ok(self)
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
        if !export.route_surface_cut_raster.is_empty() {
            return Err(WorldMapError::new(
                "a SceneMaker Template must not carry route surface cuts",
            ));
        }
        if !export.route_surface_bakes.is_empty() || !export.scene.route_surfaces.is_empty() {
            return Err(WorldMapError::new(
                "a SceneMaker Template must not carry route surfaces until composition defines them",
            ));
        }
        if !export.bridge_bakes.is_empty() || !export.scene.bridges.is_empty() {
            return Err(WorldMapError::new(
                "a SceneMaker Template must not carry bridges until composition defines them",
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

fn terrain_cell_index(width_tiles: u32, height_tiles: u32, x: u32, y: u32) -> Option<usize> {
    if x >= width_tiles || y >= height_tiles {
        return None;
    }
    let width = usize::try_from(width_tiles).ok()?;
    usize::try_from(y)
        .ok()?
        .checked_mul(width)?
        .checked_add(usize::try_from(x).ok()?)
}

fn build_terrain_cell_indices(
    width_tiles: u32,
    height_tiles: u32,
    terrain_cells: &[MapTerrainCell],
) -> Result<Vec<Option<usize>>, WorldMapError> {
    let cell_count = usize::try_from(width_tiles)
        .ok()
        .and_then(|width| {
            usize::try_from(height_tiles)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| WorldMapError::new("Terrain dimensions exceed addressable memory"))?;
    let mut indices = vec![None; cell_count];
    for (cell_index, cell) in terrain_cells.iter().enumerate() {
        let index =
            terrain_cell_index(width_tiles, height_tiles, cell.x, cell.y).ok_or_else(|| {
                WorldMapError::new(format!(
                    "Terrain cell ({}, {}) lies outside the scene",
                    cell.x, cell.y
                ))
            })?;
        if indices[index].replace(cell_index).is_some() {
            return Err(WorldMapError::new(format!(
                "Terrain cell ({}, {}) is duplicated",
                cell.x, cell.y
            )));
        }
    }
    Ok(indices)
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
    bounds: MapRouteBounds,
}

impl MapRouteSurface {
    /// Samples the baked walking height and authored grade at a point inside
    /// this Path's horizontal footprint.
    ///
    /// Triangles define only footprint containment because SceneMaker's curved
    /// join patches may overlap. Height and grade both follow the nearest
    /// baked centerline interval, with authored order breaking exact ties.
    pub fn sample_at(&self, position: Position) -> Option<MapRouteSurfaceSample> {
        if !position.x.is_finite()
            || !position.y.is_finite()
            || !self.bounds.contains(position)
            || !self.triangle_indices.chunks_exact(3).any(|triangle| {
                let Some(first) = self.vertices.get(triangle[0] as usize) else {
                    return false;
                };
                let Some(second) = self.vertices.get(triangle[1] as usize) else {
                    return false;
                };
                let Some(third) = self.vertices.get(triangle[2] as usize) else {
                    return false;
                };
                triangle_contains(position, first.position, second.position, third.position)
            })
        {
            return None;
        }
        let (_, _, _, factor, first, second, grade_percent) = self
            .segments
            .iter()
            .enumerate()
            .flat_map(|(segment_index, segment)| {
                let start = segment.start_sample_index as usize;
                let end = segment.end_sample_index as usize;
                self.centerline_samples
                    .get(start..=end)
                    .into_iter()
                    .flat_map(move |samples| {
                        samples
                            .windows(2)
                            .enumerate()
                            .map(move |(interval_index, samples)| {
                                let (distance_squared, factor) = point_segment_projection(
                                    position,
                                    samples[0].position,
                                    samples[1].position,
                                );
                                (
                                    distance_squared,
                                    segment_index,
                                    interval_index,
                                    factor,
                                    samples[0],
                                    samples[1],
                                    segment.grade_percent,
                                )
                            })
                    })
            })
            .min_by(|first, second| {
                first
                    .0
                    .total_cmp(&second.0)
                    .then_with(|| first.1.cmp(&second.1))
                    .then_with(|| first.2.cmp(&second.2))
            })?;
        let elevation_meters =
            first.elevation_meters + factor * (second.elevation_meters - first.elevation_meters);
        Some(MapRouteSurfaceSample {
            elevation_meters,
            grade_percent,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MapRouteBounds {
    minimum: Position,
    maximum: Position,
}

impl MapRouteBounds {
    fn from_vertices(vertices: &[MapRouteVertex]) -> Option<Self> {
        let first = vertices.first()?.position;
        let mut minimum = first;
        let mut maximum = first;
        for vertex in &vertices[1..] {
            minimum.x = minimum.x.min(vertex.position.x);
            minimum.y = minimum.y.min(vertex.position.y);
            maximum.x = maximum.x.max(vertex.position.x);
            maximum.y = maximum.y.max(vertex.position.y);
        }
        Some(Self { minimum, maximum })
    }

    fn contains(self, position: Position) -> bool {
        const BOUNDS_EPSILON_METERS: f32 = 1.0e-5;

        position.x >= self.minimum.x - BOUNDS_EPSILON_METERS
            && position.y >= self.minimum.y - BOUNDS_EPSILON_METERS
            && position.x <= self.maximum.x + BOUNDS_EPSILON_METERS
            && position.y <= self.maximum.y + BOUNDS_EPSILON_METERS
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapRouteSurfaceSample {
    pub elevation_meters: f32,
    pub grade_percent: i32,
}

fn triangle_contains(point: Position, first: Position, second: Position, third: Position) -> bool {
    // Dimensionless barycentric tolerance: it closes round-off cracks between
    // adjacent bake triangles, and is not gameplay collision clearance.
    const EDGE_WEIGHT_EPSILON: f32 = 1.0e-5;

    let denominator = cross(
        second.x - first.x,
        second.y - first.y,
        third.x - first.x,
        third.y - first.y,
    );
    if !denominator.is_finite() || denominator == 0.0 {
        return false;
    }
    let second_weight = cross(
        point.x - first.x,
        point.y - first.y,
        third.x - first.x,
        third.y - first.y,
    ) / denominator;
    let third_weight = cross(
        second.x - first.x,
        second.y - first.y,
        point.x - first.x,
        point.y - first.y,
    ) / denominator;
    let first_weight = 1.0 - second_weight - third_weight;
    first_weight >= -EDGE_WEIGHT_EPSILON
        && second_weight >= -EDGE_WEIGHT_EPSILON
        && third_weight >= -EDGE_WEIGHT_EPSILON
}

const fn cross(first_x: f32, first_y: f32, second_x: f32, second_y: f32) -> f32 {
    first_x * second_y - first_y * second_x
}

fn point_segment_projection(point: Position, start: Position, end: Position) -> (f32, f32) {
    let segment_x = end.x - start.x;
    let segment_y = end.y - start.y;
    let length_squared = segment_x * segment_x + segment_y * segment_y;
    let factor = if length_squared > 0.0 {
        (((point.x - start.x) * segment_x + (point.y - start.y) * segment_y) / length_squared)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let difference_x = point.x - (start.x + factor * segment_x);
    let difference_y = point.y - (start.y + factor * segment_y);
    (
        difference_x * difference_x + difference_y * difference_y,
        factor,
    )
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

/// What an authored Path segment does to the Terrain it crosses.
///
/// This is the authored meaning. It is never recovered from whether Terrain
/// happens to overlap the Path, and a subtractive segment is never treated as
/// an additive one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteSegmentOperation {
    /// Materializes its surface and removes nothing.
    Additive,
    /// Removes `[floor, floor + clearance]` from the Terrain solid at every
    /// station, while its own surface survives that cut.
    Subtractive { clearance_above_meters: f32 },
}

/// The Terrain one excavating Path removes.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRouteSurfaceCut {
    pub route_surface_id: String,
    pub cells: Vec<MapRouteCutCell>,
}

/// One excavated cell, on the finer grid the authored volumetric rasters use.
///
/// SceneMaker derives these cells rather than leaving them to a consumer: at an
/// authored operation transition a bisecting plane decides which neighbour owns
/// which side, and reconstructing that from baked triangles would disagree with
/// what the author inspected.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRouteCutCell {
    pub x: u32,
    pub y: u32,
    /// The segment that asked for this cell.
    pub segment_id: String,
    /// The height that survives the cut, and the floor an Actor walks on.
    pub floor_meters: f32,
    /// The height up to which Terrain is gone.
    pub cut_top_meters: f32,
}

/// A SceneMaker-authored body of water, with the ground it has taken.
///
/// The authored spine is what the author drew; these cells are what SceneMaker
/// resolved from it on the finer water grid, and they are what the simulation
/// reads. The spine itself belongs to the drawing and arrives with the bake
/// that draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct MapWaterBody {
    pub water_body_id: String,
    /// What kind of water this is, as the author classified it.
    pub water_kind: String,
    pub asset_key: String,
    /// The named switch this body hangs on, and `None` for one that is always
    /// there. Whether it is on is runtime state; the map only says which.
    pub switch: Option<String>,
    /// The bodies this one's source sits on, from the authored `end` of each
    /// junction. More than one means water as soon as any of them runs.
    pub feeders: Vec<String>,
    /// Every junction this body makes, with the station on each side.
    pub junctions: Vec<MapWaterJunction>,
    /// What this water names itself, from its Asset's profile - which is how a
    /// Terrain cell gets its surface too. Water is not something to walk on:
    /// a column reports it as a depth over the ground it covers, and this token
    /// is what that water is called.
    pub surface: String,
    pub cells: Vec<MapWaterCell>,
    /// The band this body is drawn as, baked by SceneMaker from the same
    /// flattener a Path uses. It carries no authored interval, because water
    /// has no grade to walk and takes nothing out of the Terrain that the
    /// cells above have not already taken.
    pub vertices: Vec<MapRouteVertex>,
    pub triangle_indices: Vec<u32>,
    /// The line the author drew, which is where anything flowing in this water
    /// is placed: station along the course, width across it.
    pub centerline_samples: Vec<MapRouteCenterlineSample>,
}

impl MapWaterBody {
    /// How far the water runs, from the end the author drew first.
    pub fn length_meters(&self) -> f32 {
        self.centerline_samples
            .last()
            .map(|sample| sample.station_meters)
            .unwrap_or_default()
    }

    /// Where the water is at one station of its course, which way it runs
    /// there, and how wide it is.
    ///
    /// This is the frame anything carried by the water is placed in: a station
    /// down the course and an offset across it.
    pub fn flow_at(&self, station_meters: f32) -> Option<MapWaterFlow> {
        if !station_meters.is_finite() || station_meters < 0.0 {
            return None;
        }
        let interval = self
            .centerline_samples
            .windows(2)
            .find(|pair| station_meters <= pair[1].station_meters)?;
        let (from, to) = (interval[0], interval[1]);
        let span = to.station_meters - from.station_meters;
        let factor = if span > 0.0 {
            ((station_meters - from.station_meters) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (x, y) = (
            to.position.x - from.position.x,
            to.position.y - from.position.y,
        );
        let length = x.hypot(y);
        if !(length > 0.0) {
            return None;
        }
        Some(MapWaterFlow {
            position: Position::new(from.position.x + factor * x, from.position.y + factor * y),
            elevation_meters: from.elevation_meters
                + factor * (to.elevation_meters - from.elevation_meters),
            direction: [x / length, y / length],
            width_meters: from.width_meters + factor * (to.width_meters - from.width_meters),
        })
    }
}

/// One place in a body of water, as anything carried by it needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapWaterFlow {
    pub position: Position,
    pub elevation_meters: f32,
    /// The unit direction the water runs in at this station.
    pub direction: [f32; 2],
    pub width_meters: f32,
}

/// One cell of water, on the same finer grid the Path cuts use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapWaterCell {
    pub x: u32,
    pub y: u32,
    /// The bed the channel was cut down to.
    pub bed_meters: f32,
    /// The height the water stands at.
    pub surface_meters: f32,
    /// The height up to which Terrain is gone, so that the channel keeps the
    /// air the author gave it.
    pub cut_top_meters: f32,
    /// How far along its own course this cell sits.
    pub station_meters: f32,
}

/// Where two bodies of water meet, from the side of the body that carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct MapWaterJunction {
    pub water_body_id: String,
    /// The station on this body's own course.
    pub own_station_meters: f32,
    /// The station the other body is at.
    pub other_station_meters: f32,
}

/// One named switch of a map, and the position the map opens in.
///
/// The map says which switches exist and where they start. Where they stand
/// later is runtime state and belongs to the simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct MapSwitch {
    pub name: String,
    pub initially_on: bool,
}

/// A SceneMaker-authored bridge: a deck laid across a gap, the planks that
/// show it, and the posts standing at its corners.
///
/// SceneMaker bakes the layout, so `plank_count` and the authored gap travel
/// only as a record of what was ordered. Laying the row out a second time here
/// is the one way this world and the authored one could drift apart.
#[derive(Debug, Clone, PartialEq)]
pub struct MapBridge {
    pub bridge_id: String,
    /// The Asset each plank shows, and the Asset standing at each corner.
    pub plank_asset_key: String,
    pub anchor_asset_key: String,
    /// What the deck offers to walk on, taken from the plank Asset's profile.
    pub surface: String,
    pub elevation_meters: f32,
    /// The direction the deck spans, counter-clockwise from +X.
    pub heading_radians: f32,
    /// The deck itself: the walking surface, as a closed authored polygon.
    pub vertices: Vec<MapRouteVertex>,
    pub triangle_indices: Vec<u32>,
    pub boundary_edges: Vec<MapRouteBoundaryEdge>,
    /// The line the author drew, in the same shape a Path publishes, because
    /// SceneMaker bakes a deck as a Path. A straight span is already straight,
    /// so this is its two ends.
    pub centerline_samples: Vec<MapRouteCenterlineSample>,
    pub planks: Vec<MapBridgePlank>,
    pub posts: Vec<MapBridgePost>,
    /// What lies at each end of the deck when the deck itself is taken away.
    /// `None` where nothing lies there at all, which is a bridge into nothing.
    pub ground_at_start: Option<MapBridgeGround>,
    pub ground_at_end: Option<MapBridgeGround>,
}

/// The ground a bridge end rests over, as SceneMaker's own column rule finds
/// it. Whether it can be walked on is read from the Asset, the way it is
/// everywhere else, and whether it is within a step is an Actor's question.
#[derive(Debug, Clone, PartialEq)]
pub struct MapBridgeGround {
    pub elevation_meters: f32,
    pub asset_key: String,
    /// The Path or Bridge this ground belongs to, or `None` for Terrain.
    pub source_id: Option<String>,
}

/// One plank of a deck. `depth_meters` runs along the span and
/// `width_meters` across it, which is the full deck width.
#[derive(Debug, Clone, PartialEq)]
pub struct MapBridgePlank {
    pub plank_id: String,
    pub position: Position,
    pub elevation_meters: f32,
    pub depth_meters: f32,
    pub width_meters: f32,
}

/// One post at a corner of a deck.
#[derive(Debug, Clone, PartialEq)]
pub struct MapBridgePost {
    pub post_id: String,
    pub corner: String,
    pub position: Position,
    pub elevation_meters: f32,
}

/// Runtime meaning retained from one authored Path interval.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRouteSegment {
    pub segment_id: String,
    pub grade_percent: i32,
    pub operation: RouteSegmentOperation,
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
    route_surface_cuts: Vec<MapRouteSurfaceCut>,
    bridges: Vec<MapBridge>,
    bridge_decks: Vec<MapRouteSurface>,
    water_bodies: Vec<MapWaterBody>,
    switches: Vec<MapSwitch>,
    water_cell_meters: f32,
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
        route_surface_cut_raster,
        bridge_bakes,
        water_bakes,
        water_raster,
        scene,
        ..
    } = export;
    let SceneDocument {
        size_cells,
        terrain_cells: source_terrain_cells,
        props: source_props,
        route_surfaces: source_route_surfaces,
        bridges: source_bridges,
        switches: source_switches,
        water_bodies: source_water_bodies,
        template_anchors: source_template_anchors,
        ..
    } = scene;

    let mut switches = Vec::with_capacity(source_switches.len());
    let mut switch_names = HashSet::with_capacity(source_switches.len());
    for declared in source_switches {
        if declared.switch.is_empty() || !switch_names.insert(declared.switch.clone()) {
            return Err(WorldMapError::new(format!(
                "switch '{}' is empty or declared twice",
                declared.switch
            )));
        }
        switches.push(MapSwitch {
            name: declared.switch,
            initially_on: declared.initially_on,
        });
    }
    let declared_switches: HashSet<&str> = switches
        .iter()
        .map(|declared| declared.name.as_str())
        .collect();

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
    if !grid.water_cell_meters.is_finite() || grid.water_cell_meters <= 0.0 {
        return Err(WorldMapError::new(
            "water_cell_meters must be finite and greater than zero",
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
    let bridges = convert_bridges(source_bridges, bridge_bakes, &profiles, offset_x, offset_y)?;
    let bridge_decks = bridge_decks(&bridges, &route_surfaces)?;
    let water_bodies = convert_water_bodies(
        source_water_bodies,
        water_raster,
        water_bakes,
        offset_x,
        offset_y,
        &profiles,
        width_tiles,
        height_tiles,
        grid.terrain_cell_meters,
        grid.water_cell_meters,
        &declared_switches,
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

    let route_surface_cuts = convert_route_surface_cuts(
        route_surface_cut_raster,
        &route_surfaces,
        width_tiles,
        height_tiles,
        grid.terrain_cell_meters,
        grid.water_cell_meters,
    )?;

    Ok(ConvertedSceneBody {
        width_tiles,
        height_tiles,
        terrain_cell_meters: grid.terrain_cell_meters,
        authoring_pixels_per_meter: grid.authoring_pixels_per_meter,
        terrain_cells,
        props,
        route_surfaces,
        route_surface_cuts,
        bridges,
        bridge_decks,
        water_bodies,
        switches,
        water_cell_meters: grid.water_cell_meters,
        template_anchors,
    })
}

/// Joins each authored body of water with the cells SceneMaker resolved for it.
///
/// A water cell says three heights: the bed the channel was cut to, the height
/// the water stands at, and the height up to which Terrain is gone. They are
/// read here and checked against each other; what they mean for walking is the
/// column rule's business.
fn convert_water_bodies(
    sources: Vec<WaterBodyDocument>,
    raster: Vec<WaterRasterDocument>,
    bakes: Vec<WaterBakeDocument>,
    offset_x: f32,
    offset_y: f32,
    profiles: &HashMap<String, AssetProfileDocument>,
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    water_cell_meters: f32,
    declared_switches: &HashSet<&str>,
) -> Result<Vec<MapWaterBody>, WorldMapError> {
    if sources.len() != raster.len() || sources.len() != bakes.len() {
        return Err(WorldMapError::new(
            "authored water bodies, water_raster and water_bakes must have the same length",
        ));
    }
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let ratio = terrain_cell_meters / water_cell_meters;
    if !ratio.is_finite() || ratio < 1.0 || ratio.fract() != 0.0 {
        return Err(WorldMapError::new(
            "a Terrain cell must cover a whole number of water cells",
        ));
    }
    let per_terrain_cell = ratio as u32;
    let mut body_ids = HashSet::with_capacity(sources.len());
    let mut converted = Vec::with_capacity(sources.len());
    for ((source, entry), bake) in sources.into_iter().zip(raster).zip(bakes) {
        if source.water_body_id.is_empty() || !body_ids.insert(source.water_body_id.clone()) {
            return Err(WorldMapError::new(format!(
                "water body ID '{}' is empty or duplicated",
                source.water_body_id
            )));
        }
        if entry.water_body_id != source.water_body_id || bake.water_body_id != source.water_body_id
        {
            return Err(WorldMapError::new(format!(
                "water body '{}' is rastered or baked under another ID",
                source.water_body_id
            )));
        }
        if source.water_kind.is_empty() || source.asset_key.is_empty() {
            return Err(WorldMapError::new(format!(
                "water body '{}' does not name its kind and Asset",
                source.water_body_id
            )));
        }
        // The switch is carried in both halves of the export so that the two
        // can disagree loudly rather than quietly.
        if entry.switch != source.switch {
            return Err(WorldMapError::new(format!(
                "water body '{}' hangs on a different switch in the scene than in the raster",
                source.water_body_id
            )));
        }
        if let Some(name) = &source.switch
            && !declared_switches.contains(name.as_str())
        {
            return Err(WorldMapError::new(format!(
                "water body '{}' hangs on switch '{name}', which the scene does not declare",
                source.water_body_id
            )));
        }
        let mut feeders = Vec::new();
        for junction in &source.junctions {
            if junction.end.is_empty() || junction.water_body_id.is_empty() {
                return Err(WorldMapError::new(format!(
                    "water body '{}' has a junction that names no end or no other body",
                    source.water_body_id
                )));
            }
            // Which body feeds which is authored here and inferred nowhere: a
            // station of zero says where a junction sits, never which way the
            // water runs through it.
            if junction.end == "source" {
                feeders.push(junction.water_body_id.clone());
            }
        }
        let mut junctions = Vec::with_capacity(entry.junctions.len());
        for junction in &entry.junctions {
            if !junction.own_station_meters.is_finite()
                || !junction.station_meters.is_finite()
                || junction.own_station_meters < 0.0
                || junction.station_meters < 0.0
            {
                return Err(WorldMapError::new(format!(
                    "water body '{}' has a junction at no station",
                    source.water_body_id
                )));
            }
            junctions.push(MapWaterJunction {
                water_body_id: junction.water_body_id.clone(),
                own_station_meters: junction.own_station_meters,
                other_station_meters: junction.station_meters,
            });
        }
        let Some(surface) = require_profile(profiles, &source.asset_key)?
            .surface
            .clone()
        else {
            return Err(WorldMapError::new(format!(
                "water asset '{}' has no exported surface",
                source.asset_key
            )));
        };
        let mut occupied = HashSet::with_capacity(entry.cells.len());
        let mut cells = Vec::with_capacity(entry.cells.len());
        for cell in entry.cells {
            if cell.x >= width_tiles.saturating_mul(per_terrain_cell)
                || cell.y >= height_tiles.saturating_mul(per_terrain_cell)
                || !occupied.insert((cell.x, cell.y))
            {
                return Err(WorldMapError::new(format!(
                    "water body '{}' has a cell outside the world or twice over",
                    source.water_body_id
                )));
            }
            if !cell.bed_meters.is_finite()
                || !cell.surface_meters.is_finite()
                || !cell.cut_top_meters.is_finite()
                || cell.surface_meters <= cell.bed_meters
                || cell.cut_top_meters < cell.surface_meters
            {
                return Err(WorldMapError::new(format!(
                    "water body '{}' has a cell whose bed, surface and cut do not stack up",
                    source.water_body_id
                )));
            }
            if !cell.station_meters.is_finite() || cell.station_meters < 0.0 {
                return Err(WorldMapError::new(format!(
                    "water body '{}' has a cell at no station of its own course",
                    source.water_body_id
                )));
            }
            cells.push(MapWaterCell {
                x: cell.x,
                y: cell.y,
                bed_meters: cell.bed_meters,
                surface_meters: cell.surface_meters,
                cut_top_meters: cell.cut_top_meters,
                station_meters: cell.station_meters,
            });
        }
        if cells.is_empty() {
            return Err(WorldMapError::new(format!(
                "water body '{}' occupies no cell at all",
                source.water_body_id
            )));
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
                        "water body '{}' has a non-finite corner",
                        source.water_body_id
                    )));
                }
                Ok(MapRouteVertex {
                    position: Position::new(vertex.x_meters + offset_x, vertex.y_meters + offset_y),
                    elevation_meters: vertex.elevation_meters,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if vertices.len() < 3
            || bake.triangle_indices.is_empty()
            || bake.triangle_indices.len() % 3 != 0
            || bake
                .triangle_indices
                .iter()
                .any(|index| *index as usize >= vertices.len())
        {
            return Err(WorldMapError::new(format!(
                "water body '{}' has no usable band",
                source.water_body_id
            )));
        }
        let mut centerline_samples = Vec::with_capacity(bake.centerline_samples.len());
        let mut previous_station: Option<f32> = None;
        for sample in bake.centerline_samples {
            if !sample.x_meters.is_finite()
                || !sample.y_meters.is_finite()
                || !sample.elevation_meters.is_finite()
                || !sample.width_meters.is_finite()
                || sample.width_meters <= 0.0
                || !sample.station_meters.is_finite()
                || previous_station.is_some_and(|previous| sample.station_meters <= previous)
            {
                return Err(WorldMapError::new(format!(
                    "water body '{}' has an invalid centerline sample",
                    source.water_body_id
                )));
            }
            previous_station = Some(sample.station_meters);
            centerline_samples.push(MapRouteCenterlineSample {
                position: Position::new(sample.x_meters + offset_x, sample.y_meters + offset_y),
                elevation_meters: sample.elevation_meters,
                width_meters: sample.width_meters,
                station_meters: sample.station_meters,
                authored_point_index: sample.authored_point_index,
            });
        }
        if centerline_samples.len() < 2 {
            return Err(WorldMapError::new(format!(
                "water body '{}' has no centerline to flow along",
                source.water_body_id
            )));
        }
        converted.push(MapWaterBody {
            water_body_id: source.water_body_id,
            water_kind: source.water_kind,
            asset_key: source.asset_key,
            switch: source.switch,
            feeders,
            junctions,
            surface,
            cells,
            vertices,
            triangle_indices: bake.triangle_indices,
            centerline_samples,
        });
    }
    validate_water_topology(&converted)?;
    Ok(converted)
}

/// Checks what the two halves of the export say about how bodies of water meet.
///
/// The scene names a junction only on the side where a body has an end of its
/// own, and the raster carries both sides with the stations swapped. So the
/// invariant is not "one entry each way" but this:
///
/// - every junction the raster names is mirrored by the body at the other side,
///   with the two stations exchanged;
/// - exactly one of the two sides declares the junction as its `source`, which
///   is what makes the direction of the water unambiguous without reading it
///   out of a station;
/// - and a body's own scene junctions are all present in its raster.
///
/// Feeding must also reach a beginning: a ring of bodies feeding each other is
/// refused rather than walked, because a reader that never terminates fails
/// worse than one that says no.
fn validate_water_topology(bodies: &[MapWaterBody]) -> Result<(), WorldMapError> {
    let known: HashMap<&str, &MapWaterBody> = bodies
        .iter()
        .map(|body| (body.water_body_id.as_str(), body))
        .collect();
    for body in bodies {
        for junction in &body.junctions {
            let Some(other) = known.get(junction.water_body_id.as_str()) else {
                return Err(WorldMapError::new(format!(
                    "water body '{}' meets '{}', which this map does not carry",
                    body.water_body_id, junction.water_body_id
                )));
            };
            let mirrored = other.junctions.iter().any(|mirror| {
                mirror.water_body_id == body.water_body_id
                    && (mirror.own_station_meters - junction.other_station_meters).abs() <= 1.0e-3
                    && (mirror.other_station_meters - junction.own_station_meters).abs() <= 1.0e-3
            });
            if !mirrored {
                return Err(WorldMapError::new(format!(
                    "the junction between '{}' and '{}' is not carried from both sides",
                    body.water_body_id, junction.water_body_id
                )));
            }
            let declares = body
                .feeders
                .iter()
                .any(|fed| fed == &junction.water_body_id);
            let other_declares = other.feeders.iter().any(|fed| fed == &body.water_body_id);
            if declares == other_declares {
                return Err(WorldMapError::new(format!(
                    "the junction between '{}' and '{}' is claimed as a source by {}",
                    body.water_body_id,
                    junction.water_body_id,
                    if declares {
                        "both sides"
                    } else {
                        "neither side"
                    }
                )));
            }
        }
        for feeder in &body.feeders {
            if !body
                .junctions
                .iter()
                .any(|junction| &junction.water_body_id == feeder)
            {
                return Err(WorldMapError::new(format!(
                    "water body '{}' is fed by '{feeder}', which its raster does not meet",
                    body.water_body_id
                )));
            }
        }
    }
    for body in bodies {
        let mut walked = HashSet::new();
        let mut pending = vec![body.water_body_id.as_str()];
        while let Some(current) = pending.pop() {
            if !walked.insert(current) {
                continue;
            }
            let Some(current) = known.get(current) else {
                continue;
            };
            for feeder in &current.feeders {
                if feeder == &body.water_body_id {
                    return Err(WorldMapError::new(format!(
                        "water body '{}' is fed, around a ring, by itself",
                        body.water_body_id
                    )));
                }
                pending.push(feeder.as_str());
            }
        }
    }
    Ok(())
}

/// A deck is a Path to whoever walks on it.
///
/// SceneMaker bakes a bridge as a Path bake and publishes the same centerline,
/// so world01 carries it as one and every rule that already knows how to stand
/// on a Path, step onto it and leave it applies unchanged. The single thing not
/// delivered is the authored interval, because a bridge has exactly one and the
/// contract says what it is: level, and taking nothing out of the Terrain.
fn bridge_decks(
    bridges: &[MapBridge],
    route_surfaces: &[MapRouteSurface],
) -> Result<Vec<MapRouteSurface>, WorldMapError> {
    let mut decks = Vec::with_capacity(bridges.len());
    for bridge in bridges {
        if route_surfaces
            .iter()
            .any(|route| route.route_surface_id == bridge.bridge_id)
        {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' carries the ID of an authored Path",
                bridge.bridge_id
            )));
        }
        let Some(bounds) = MapRouteBounds::from_vertices(&bridge.vertices) else {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' deck has no corners to bound",
                bridge.bridge_id
            )));
        };
        let last_sample = u32::try_from(bridge.centerline_samples.len() - 1).map_err(|_| {
            WorldMapError::new(format!(
                "Bridge '{}' has more centerline samples than a `u32` counts",
                bridge.bridge_id
            ))
        })?;
        decks.push(MapRouteSurface {
            route_surface_id: bridge.bridge_id.clone(),
            asset_key: bridge.plank_asset_key.clone(),
            surface: bridge.surface.clone(),
            vertices: bridge.vertices.clone(),
            triangle_indices: bridge.triangle_indices.clone(),
            boundary_edges: bridge.boundary_edges.clone(),
            centerline_samples: bridge.centerline_samples.clone(),
            segments: vec![MapRouteSegment {
                segment_id: format!("{}.deck", bridge.bridge_id),
                grade_percent: 0,
                operation: RouteSegmentOperation::Additive,
                start_point_index: 0,
                end_point_index: last_sample,
                start_sample_index: 0,
                end_sample_index: last_sample,
            }],
            bounds,
        });
    }
    decks.sort_by(|first, second| first.route_surface_id.cmp(&second.route_surface_id));
    if decks
        .windows(2)
        .any(|pair| pair[0].route_surface_id == pair[1].route_surface_id)
    {
        return Err(WorldMapError::new("two bridges share one ID"));
    }
    Ok(decks)
}

/// Joins each authored bridge with the layout SceneMaker baked for it.
///
/// Nothing here is re-derived from the authored line: the deck, the planks and
/// the posts are taken as delivered, and the authored counts are used only to
/// check that what arrived is what was ordered.
fn convert_bridges(
    sources: Vec<BridgeDocument>,
    bakes: Vec<BridgeBakeDocument>,
    profiles: &HashMap<String, AssetProfileDocument>,
    offset_x: f32,
    offset_y: f32,
) -> Result<Vec<MapBridge>, WorldMapError> {
    if sources.len() != bakes.len() {
        return Err(WorldMapError::new(
            "authored bridges and bridge_bakes must have the same length",
        ));
    }
    let mut bridge_ids = HashSet::with_capacity(sources.len());
    let mut converted = Vec::with_capacity(sources.len());
    for (source, bake) in sources.into_iter().zip(bakes) {
        if source.bridge_id.is_empty() || !bridge_ids.insert(source.bridge_id.clone()) {
            return Err(WorldMapError::new(format!(
                "Bridge ID '{}' is empty or duplicated",
                source.bridge_id
            )));
        }
        if bake.bridge_id != source.bridge_id {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' is baked under another ID",
                source.bridge_id
            )));
        }
        if source.plank_asset_key.is_empty() || source.anchor_asset_key.is_empty() {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' does not name both of its Assets",
                source.bridge_id
            )));
        }
        if bake.plank_asset_key != source.plank_asset_key || bake.plank_count != source.plank_count
        {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' is baked from another plank Asset or count",
                source.bridge_id
            )));
        }
        let Some(surface) = require_profile(profiles, &source.plank_asset_key)?
            .surface
            .clone()
        else {
            return Err(WorldMapError::new(format!(
                "bridge asset '{}' has no exported surface",
                source.plank_asset_key
            )));
        };
        if !source.elevation_meters.is_finite()
            || !bake.heading_degrees.is_finite()
            || !source.width_meters.is_finite()
            || source.width_meters <= 0.0
        {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' has a non-finite elevation, heading or width",
                source.bridge_id
            )));
        }
        if bake.vertices.len() != 4 {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' deck is not a quad",
                source.bridge_id
            )));
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
                        "Bridge '{}' deck has a non-finite corner",
                        source.bridge_id
                    )));
                }
                Ok(MapRouteVertex {
                    position: Position::new(vertex.x_meters + offset_x, vertex.y_meters + offset_y),
                    elevation_meters: vertex.elevation_meters,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if bake.triangle_indices.is_empty()
            || bake.triangle_indices.len() % 3 != 0
            || bake
                .triangle_indices
                .iter()
                .any(|index| *index as usize >= vertices.len())
        {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' deck has invalid triangles",
                source.bridge_id
            )));
        }
        // SceneMaker's contract says a bridge is straight and level, so a
        // single elevation carries it. That is checked here rather than
        // trusted: a deck that ever tilts must arrive as a new export version.
        if vertices
            .iter()
            .any(|vertex| !level_with(vertex.elevation_meters, source.elevation_meters))
        {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' has a corner off its own elevation",
                source.bridge_id
            )));
        }
        let boundary_edges = bake
            .boundary_edges
            .iter()
            .map(|edge| MapRouteBoundaryEdge {
                start_vertex_index: edge.start_vertex_index,
                end_vertex_index: edge.end_vertex_index,
            })
            .collect::<Vec<_>>();
        if boundary_edges.is_empty()
            || boundary_edges.iter().any(|edge| {
                edge.start_vertex_index as usize >= vertices.len()
                    || edge.end_vertex_index as usize >= vertices.len()
            })
        {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' deck has no usable outline",
                source.bridge_id
            )));
        }
        if bake.centerline_samples.len() < 2 || !bake.length_meters.is_finite() {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' has no centerline to walk along",
                source.bridge_id
            )));
        }
        let mut centerline_samples = Vec::with_capacity(bake.centerline_samples.len());
        let mut previous_station: Option<f32> = None;
        for sample in bake.centerline_samples {
            if !sample.x_meters.is_finite()
                || !sample.y_meters.is_finite()
                || !level_with(sample.elevation_meters, source.elevation_meters)
                || !level_with(sample.width_meters, source.width_meters)
                || !sample.station_meters.is_finite()
                || previous_station.is_some_and(|previous| sample.station_meters <= previous)
            {
                return Err(WorldMapError::new(format!(
                    "Bridge '{}' has an invalid centerline sample",
                    source.bridge_id
                )));
            }
            previous_station = Some(sample.station_meters);
            centerline_samples.push(MapRouteCenterlineSample {
                position: Position::new(sample.x_meters + offset_x, sample.y_meters + offset_y),
                elevation_meters: sample.elevation_meters,
                width_meters: sample.width_meters,
                station_meters: sample.station_meters,
                authored_point_index: sample.authored_point_index,
            });
        }
        let first_station = centerline_samples
            .first()
            .map(|sample| sample.station_meters)
            .unwrap_or_default();
        let last_station = previous_station.unwrap_or_default();
        if !level_with(first_station, 0.0) || !level_with(last_station, bake.length_meters) {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' does not run from one end of its own span to the other",
                source.bridge_id
            )));
        }
        let ground_at_start = convert_bridge_ground(bake.ground_at_start, &source.bridge_id)?;
        let ground_at_end = convert_bridge_ground(bake.ground_at_end, &source.bridge_id)?;
        if bake.planks.len() as u32 != source.plank_count {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' was ordered with {} planks and baked with {}",
                source.bridge_id,
                source.plank_count,
                bake.planks.len()
            )));
        }
        let mut plank_ids = HashSet::with_capacity(bake.planks.len());
        let mut planks = Vec::with_capacity(bake.planks.len());
        for plank in bake.planks {
            if plank.plank_id.is_empty() || !plank_ids.insert(plank.plank_id.clone()) {
                return Err(WorldMapError::new(format!(
                    "plank ID '{}' is empty or duplicated",
                    plank.plank_id
                )));
            }
            if plank.asset_key != source.plank_asset_key {
                return Err(WorldMapError::new(format!(
                    "plank '{}' shows another Asset than its Bridge",
                    plank.plank_id
                )));
            }
            if !plank.x_meters.is_finite()
                || !plank.y_meters.is_finite()
                || !plank.elevation_meters.is_finite()
                || !plank.depth_meters.is_finite()
                || plank.depth_meters <= 0.0
                || !plank.width_meters.is_finite()
                || plank.width_meters <= 0.0
            {
                return Err(WorldMapError::new(format!(
                    "plank '{}' has invalid placement or size",
                    plank.plank_id
                )));
            }
            planks.push(MapBridgePlank {
                plank_id: plank.plank_id,
                position: Position::new(plank.x_meters + offset_x, plank.y_meters + offset_y),
                elevation_meters: plank.elevation_meters,
                depth_meters: plank.depth_meters,
                width_meters: plank.width_meters,
            });
        }
        let mut post_ids = HashSet::with_capacity(bake.posts.len());
        let mut corners = HashSet::with_capacity(bake.posts.len());
        let mut posts = Vec::with_capacity(bake.posts.len());
        for post in bake.posts {
            if post.post_id.is_empty() || !post_ids.insert(post.post_id.clone()) {
                return Err(WorldMapError::new(format!(
                    "post ID '{}' is empty or duplicated",
                    post.post_id
                )));
            }
            if post.corner.is_empty() || !corners.insert(post.corner.clone()) {
                return Err(WorldMapError::new(format!(
                    "post '{}' repeats a corner of its Bridge",
                    post.post_id
                )));
            }
            if post.asset_key != source.anchor_asset_key {
                return Err(WorldMapError::new(format!(
                    "post '{}' shows another Asset than its Bridge",
                    post.post_id
                )));
            }
            if !post.x_meters.is_finite()
                || !post.y_meters.is_finite()
                || !post.elevation_meters.is_finite()
            {
                return Err(WorldMapError::new(format!(
                    "post '{}' has a non-finite placement",
                    post.post_id
                )));
            }
            posts.push(MapBridgePost {
                post_id: post.post_id,
                corner: post.corner,
                position: Position::new(post.x_meters + offset_x, post.y_meters + offset_y),
                elevation_meters: post.elevation_meters,
            });
        }
        if posts.len() != vertices.len() {
            return Err(WorldMapError::new(format!(
                "Bridge '{}' does not carry a post at every deck corner",
                source.bridge_id
            )));
        }
        converted.push(MapBridge {
            bridge_id: source.bridge_id,
            plank_asset_key: source.plank_asset_key,
            anchor_asset_key: source.anchor_asset_key,
            surface,
            elevation_meters: source.elevation_meters,
            heading_radians: bake.heading_degrees.to_radians(),
            vertices,
            triangle_indices: bake.triangle_indices,
            boundary_edges,
            centerline_samples,
            planks,
            posts,
            ground_at_start,
            ground_at_end,
        });
    }
    Ok(converted)
}

/// A bridge end either rests over something or over nothing at all.
fn convert_bridge_ground(
    ground: Option<BridgeGroundDocument>,
    bridge_id: &str,
) -> Result<Option<MapBridgeGround>, WorldMapError> {
    let Some(ground) = ground else {
        return Ok(None);
    };
    if !ground.elevation_meters.is_finite() || ground.asset_key.is_empty() {
        return Err(WorldMapError::new(format!(
            "Bridge '{bridge_id}' names invalid ground at one of its ends"
        )));
    }
    Ok(Some(MapBridgeGround {
        elevation_meters: ground.elevation_meters,
        asset_key: ground.asset_key,
        source_id: ground.source_id,
    }))
}

/// Two authored measures agree when they differ by less than the export writes.
fn level_with(measured: f32, authored: f32) -> bool {
    const AUTHORED_TOLERANCE_METERS: f32 = 1.0e-4;

    (measured - authored).abs() <= AUTHORED_TOLERANCE_METERS
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
        let bounds = MapRouteBounds::from_vertices(&vertices).ok_or_else(|| {
            WorldMapError::new(format!(
                "route surface '{}' needs baked vertices",
                source.route_surface_id
            ))
        })?;
        converted.push(MapRouteSurface {
            route_surface_id: source.route_surface_id,
            asset_key: source.asset_key,
            surface,
            vertices,
            triangle_indices: bake.triangle_indices,
            boundary_edges,
            centerline_samples,
            segments,
            bounds,
        });
    }
    Ok(converted)
}

/// Reads an authored segment operation, refusing every disagreement between it
/// and its clearance rather than repairing one from the other.
fn segment_operation(
    route_surface_id: &str,
    operation: &str,
    clearance_above_meters: Option<f32>,
) -> Result<RouteSegmentOperation, WorldMapError> {
    match (operation, clearance_above_meters) {
        ("additive", None) => Ok(RouteSegmentOperation::Additive),
        ("subtractive", Some(clearance)) if clearance.is_finite() && clearance > 0.0 => {
            Ok(RouteSegmentOperation::Subtractive {
                clearance_above_meters: clearance,
            })
        }
        _ => Err(WorldMapError::new(format!(
            "route surface '{route_surface_id}' has a segment whose operation and clearance disagree"
        ))),
    }
}

/// Converts the derived excavation cells of every Path that carries one.
///
/// The set of entries has to be exactly the Paths that excavate: a purely
/// additive Path is absent rather than present and empty, while an excavating
/// Path whose cells fall outside the Scene is present with none.
///
/// The finer grid has to nest inside the Terrain grid for a cell to be placed
/// at all, so a world without excavation is never asked to.
fn convert_route_surface_cuts(
    raster: Vec<RouteSurfaceCutRasterDocument>,
    routes: &[MapRouteSurface],
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    water_cell_meters: f32,
) -> Result<Vec<MapRouteSurfaceCut>, WorldMapError> {
    let expected = routes
        .iter()
        .filter(|route| {
            route.segments.iter().any(|segment| {
                matches!(segment.operation, RouteSegmentOperation::Subtractive { .. })
            })
        })
        .map(|route| route.route_surface_id.as_str())
        .collect::<HashSet<_>>();
    if expected.is_empty() && raster.is_empty() {
        return Ok(Vec::new());
    }
    let ratio = terrain_cell_meters / water_cell_meters;
    if !ratio.is_finite() || ratio < 1.0 || ratio.fract() != 0.0 {
        return Err(WorldMapError::new(
            "a Terrain cell must cover a whole number of water cells",
        ));
    }
    let per_terrain_cell = ratio as u32;
    let mut seen = HashSet::with_capacity(raster.len());
    let mut converted = Vec::with_capacity(raster.len());
    for entry in raster {
        if !expected.contains(entry.route_surface_id.as_str())
            || !seen.insert(entry.route_surface_id.clone())
        {
            return Err(WorldMapError::new(format!(
                "cut raster entry '{}' names no excavating Path, or names one twice",
                entry.route_surface_id
            )));
        }
        let Some(route) = routes
            .iter()
            .find(|route| route.route_surface_id == entry.route_surface_id)
        else {
            return Err(WorldMapError::new(format!(
                "cut raster entry '{}' has no route surface",
                entry.route_surface_id
            )));
        };
        let cells = entry
            .cells
            .into_iter()
            .map(|cell| {
                let excavates = route.segments.iter().any(|segment| {
                    segment.segment_id == cell.segment_id
                        && matches!(segment.operation, RouteSegmentOperation::Subtractive { .. })
                });
                if !excavates
                    || cell.x >= width_tiles.saturating_mul(per_terrain_cell)
                    || cell.y >= height_tiles.saturating_mul(per_terrain_cell)
                    || !cell.floor_meters.is_finite()
                    || !cell.cut_top_meters.is_finite()
                    || cell.cut_top_meters <= cell.floor_meters
                {
                    return Err(WorldMapError::new(format!(
                        "route surface '{}' has an invalid cut cell",
                        entry.route_surface_id
                    )));
                }
                Ok(MapRouteCutCell {
                    x: cell.x,
                    y: cell.y,
                    segment_id: cell.segment_id,
                    floor_meters: cell.floor_meters,
                    cut_top_meters: cell.cut_top_meters,
                })
            })
            .collect::<Result<Vec<_>, WorldMapError>>()?;
        converted.push(MapRouteSurfaceCut {
            route_surface_id: entry.route_surface_id,
            cells,
        });
    }
    if seen.len() != expected.len() {
        return Err(WorldMapError::new(
            "every excavating Path needs its derived cut cells",
        ));
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
        segment_operation(
            &source.route_surface_id,
            &segment.operation,
            segment.clearance_above_meters,
        )?;
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
            let operation = segment_operation(
                &source.route_surface_id,
                &segment.operation,
                segment.clearance_above_meters,
            )?;
            let authored_operation = segment_operation(
                &source.route_surface_id,
                &authored.operation,
                authored.clearance_above_meters,
            )?;
            if operation != authored_operation
                || segment.segment_id != authored.segment_id
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
                operation,
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

/// Groups the derived excavation cells by the cell they cut, so resolving a
/// column is a lookup instead of a scan across every Path.
///
/// Intervals are ordered by floor and then by top, because two Paths may cut
/// the same cell and a column must resolve the same way every time.
/// One walking surface a column offers.
///
/// The column says where the ground is and how deep the water over it stands.
/// It does not say who may go there: the water level is the authored one today
/// and a world with tides would move it, and the same ground would then be
/// walkable without the column having to be rebuilt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapColumnSurface<'a> {
    pub elevation_meters: f32,
    /// What this surface offers to walk on, which is the Terrain's own surface.
    pub surface: &'a str,
    /// How deep the standing water over this surface is, and zero where there
    /// is none.
    pub water_depth_meters: f32,
}

/// Which bodies of water exist at these switch positions.
///
/// A body is there when its own switch is on **and** a body feeding it is
/// there. A branch is fed by the river it leaves, so switching that river off
/// takes everything hanging under it, to any depth.
///
/// Resolved by starting with every body its own switch allows and dropping
/// those whose feeders all fell away, until nothing changes. A ring is already
/// refused when the map is read; settling to a fixed point rather than walking
/// the chain means that even a ring that slipped through would end here rather
/// than run forever.
fn resolve_present_bodies(
    bodies: &[MapWaterBody],
    positions: &HashMap<String, bool>,
) -> HashSet<String> {
    let mut present: HashSet<String> = bodies
        .iter()
        .filter(|body| {
            body.switch
                .as_ref()
                .is_none_or(|switch| positions.get(switch.as_str()).copied().unwrap_or_default())
        })
        .map(|body| body.water_body_id.clone())
        .collect();
    loop {
        let dried = bodies
            .iter()
            .filter(|body| present.contains(&body.water_body_id))
            .filter(|body| {
                !body.feeders.is_empty()
                    && !body.feeders.iter().any(|feeder| present.contains(feeder))
            })
            .map(|body| body.water_body_id.clone())
            .collect::<Vec<_>>();
        if dried.is_empty() {
            return present;
        }
        for body in dried {
            present.remove(&body);
        }
    }
}

fn index_water_columns(
    bodies: &[MapWaterBody],
    present: &HashSet<String>,
) -> HashMap<(u32, u32), WaterColumn> {
    let mut gathered: HashMap<(u32, u32), Vec<&MapWaterCell>> = HashMap::new();
    for body in bodies
        .iter()
        .filter(|body| present.contains(&body.water_body_id))
    {
        for cell in &body.cells {
            gathered.entry((cell.x, cell.y)).or_default().push(cell);
        }
    }
    gathered
        .into_iter()
        .map(|(at, cells)| {
            let mut fills: Vec<WaterFill> = cells
                .iter()
                .map(|cell| WaterFill {
                    bed_meters: cell.bed_meters,
                    surface_meters: cell.surface_meters,
                })
                .collect();
            fills.sort_by(|first, second| {
                first
                    .bed_meters
                    .total_cmp(&second.bed_meters)
                    .then_with(|| first.surface_meters.total_cmp(&second.surface_meters))
            });
            let mut merged: Vec<WaterFill> = Vec::with_capacity(fills.len());
            for fill in fills {
                match merged.last_mut() {
                    // Water that touches is one body of water, so a fill that
                    // begins where another ends joins it rather than stacking
                    // on it.
                    Some(last) if fill.bed_meters <= last.surface_meters => {
                        last.surface_meters = last.surface_meters.max(fill.surface_meters);
                    }
                    _ => merged.push(fill),
                }
            }
            let cuts = cells
                .iter()
                .map(|cell| TerrainCut {
                    floor_meters: cell.bed_meters,
                    cut_top_meters: cell.cut_top_meters,
                })
                .collect();
            (
                at,
                WaterColumn {
                    fills: merged,
                    cuts,
                },
            )
        })
        .collect()
}

fn index_terrain_cuts(cuts: &[MapRouteSurfaceCut]) -> HashMap<(u32, u32), Vec<TerrainCut>> {
    let mut indexed: HashMap<(u32, u32), Vec<TerrainCut>> = HashMap::new();
    for cut in cuts {
        for cell in &cut.cells {
            indexed
                .entry((cell.x, cell.y))
                .or_default()
                .push(TerrainCut {
                    floor_meters: cell.floor_meters,
                    cut_top_meters: cell.cut_top_meters,
                });
        }
    }
    for intervals in indexed.values_mut() {
        intervals.sort_by(|first, second| {
            first
                .floor_meters
                .total_cmp(&second.floor_meters)
                .then_with(|| first.cut_top_meters.total_cmp(&second.cut_top_meters))
        });
    }
    indexed
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
    water_raster: Vec<WaterRasterDocument>,
    route_surface_bakes: Vec<RouteSurfaceBakeDocument>,
    route_surface_cut_raster: Vec<RouteSurfaceCutRasterDocument>,
    bridge_bakes: Vec<BridgeBakeDocument>,
    water_bakes: Vec<WaterBakeDocument>,
    scene: SceneDocument,
}

#[derive(Deserialize)]
struct BridgeDocument {
    bridge_id: String,
    plank_asset_key: String,
    anchor_asset_key: String,
    width_meters: f32,
    elevation_meters: f32,
    plank_count: u32,
}

#[derive(Deserialize)]
struct BridgeBakeDocument {
    bridge_id: String,
    plank_asset_key: String,
    length_meters: f32,
    heading_degrees: f32,
    plank_count: u32,
    vertices: Vec<BridgeVertexDocument>,
    triangle_indices: Vec<u32>,
    boundary_edges: Vec<RouteSurfaceBoundaryEdgeDocument>,
    centerline_samples: Vec<RouteSurfaceCenterlineSampleDocument>,
    planks: Vec<BridgePlankDocument>,
    posts: Vec<BridgePostDocument>,
    ground_at_start: Option<BridgeGroundDocument>,
    ground_at_end: Option<BridgeGroundDocument>,
}

#[derive(Deserialize)]
struct BridgeVertexDocument {
    x_meters: f32,
    y_meters: f32,
    elevation_meters: f32,
}

#[derive(Deserialize)]
struct BridgeGroundDocument {
    elevation_meters: f32,
    asset_key: String,
    source_id: Option<String>,
}

#[derive(Deserialize)]
struct BridgePlankDocument {
    plank_id: String,
    asset_key: String,
    x_meters: f32,
    y_meters: f32,
    elevation_meters: f32,
    depth_meters: f32,
    width_meters: f32,
}

#[derive(Deserialize)]
struct BridgePostDocument {
    post_id: String,
    corner: String,
    asset_key: String,
    x_meters: f32,
    y_meters: f32,
    elevation_meters: f32,
}

#[derive(Deserialize)]
struct RouteSurfaceCutRasterDocument {
    route_surface_id: String,
    cells: Vec<RouteSurfaceCutCellDocument>,
}

#[derive(Deserialize)]
struct RouteSurfaceCutCellDocument {
    x: u32,
    y: u32,
    segment_id: String,
    floor_meters: f32,
    cut_top_meters: f32,
}

#[derive(Deserialize)]
struct WaterBodyDocument {
    water_body_id: String,
    water_kind: String,
    asset_key: String,
    /// The named switch this body hangs on, or `None` for a body that is
    /// always there.
    switch: Option<String>,
    junctions: Vec<BodyJunctionDocument>,
}

/// A junction as the author made it: which end of *this* body it is, and the
/// body at the other side.
///
/// `end == "source"` is the only thing that says which body feeds which, and it
/// is authored rather than inferred. The raster carries the same junctions with
/// their stations, and the two are checked against each other.
#[derive(Deserialize)]
struct BodyJunctionDocument {
    end: String,
    water_body_id: String,
}

#[derive(Deserialize)]
struct RasterJunctionDocument {
    water_body_id: String,
    own_station_meters: f32,
    station_meters: f32,
}

/// One named switch of a scene, and where the map opens.
#[derive(Deserialize)]
struct SwitchDocument {
    switch: String,
    initially_on: bool,
}

#[derive(Deserialize)]
struct WaterBakeDocument {
    water_body_id: String,
    vertices: Vec<BridgeVertexDocument>,
    triangle_indices: Vec<u32>,
    centerline_samples: Vec<RouteSurfaceCenterlineSampleDocument>,
}

#[derive(Deserialize)]
struct WaterRasterDocument {
    water_body_id: String,
    /// The same switch the scene names for this body. Carried twice so the two
    /// halves of the export can be checked against each other.
    switch: Option<String>,
    junctions: Vec<RasterJunctionDocument>,
    cells: Vec<WaterCellDocument>,
}

#[derive(Deserialize)]
struct WaterCellDocument {
    x: u32,
    y: u32,
    bed_meters: f32,
    surface_meters: f32,
    cut_top_meters: f32,
    /// How far along its own course this cell sits, which is what anything
    /// carried by the water is placed by.
    station_meters: f32,
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
    switches: Vec<SwitchDocument>,
    water_bodies: Vec<WaterBodyDocument>,
    route_surfaces: Vec<RouteSurfaceDocument>,
    bridges: Vec<BridgeDocument>,
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
    operation: String,
    clearance_above_meters: Option<f32>,
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
    operation: String,
    clearance_above_meters: Option<f32>,
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
            "route_surface_cut_raster": [],
            "bridge_bakes": [],
            "water_bakes": [],
            "scene": {{
                "schema": "{SCENE_SCHEMA}",
                "version": {scene_version},
                "scene_id": "{TEST_SCENE_ID}",
                "scene_kind": "instance",
                "size_cells": {{ "width": 4, "height": 4 }},
                "coordinate_space": "{COORDINATE_SPACE}",
                "terrain_cells": [{terrain_cells}],
                "props": [{props}],
                "switches": [],
                "water_bodies": [],
                "route_surfaces": [],
                "bridges": [],
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
        assert_eq!(map.route_surfaces().len(), 4);
        assert_eq!(
            map.route_surfaces()
                .iter()
                .map(|route| route.segments[0].grade_percent)
                .collect::<Vec<_>>(),
            vec![25, 50, -50, 0]
        );
        assert!(!map.route_surfaces()[0].vertices.is_empty());
        assert!(!map.route_surfaces()[0].boundary_edges.is_empty());
    }

    #[test]
    fn route_sampling_uses_baked_height_and_authored_grade() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");
        for route in map.route_surfaces() {
            let expected_grade = route.segments[0].grade_percent;
            for sample in &route.centerline_samples {
                let resolved = route
                    .sample_at(sample.position)
                    .expect("every baked centerline sample lies on its Path surface");
                assert!(
                    (resolved.elevation_meters - sample.elevation_meters).abs() < 0.000_1,
                    "{} sampled {} instead of {}",
                    route.route_surface_id,
                    resolved.elevation_meters,
                    sample.elevation_meters
                );
                assert_eq!(resolved.grade_percent, expected_grade);
            }
        }
    }

    #[test]
    fn route_sampling_rejects_points_outside_the_baked_surface() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");
        assert_eq!(
            map.route_surfaces()[0].sample_at(Position::new(10_000.0, 10_000.0)),
            None
        );
        assert_eq!(map.route_surface("missing"), None);
    }

    #[test]
    fn route_sampling_keeps_each_authored_segment_grade() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");
        let mut route = map.route_surfaces()[0].clone();
        let grades = [0, 25, 50, -25, -50];
        for (segment, grade) in route.segments.iter_mut().zip(grades) {
            segment.grade_percent = grade;
        }

        for segment in &route.segments {
            let first = route.centerline_samples[segment.start_sample_index as usize].position;
            let second = route.centerline_samples[segment.start_sample_index as usize + 1].position;
            let midpoint = Position::new((first.x + second.x) * 0.5, (first.y + second.y) * 0.5);
            assert_eq!(
                route
                    .sample_at(midpoint)
                    .expect("segment centerline lies on its Path")
                    .grade_percent,
                segment.grade_percent
            );
        }
    }

    #[test]
    fn route_sampling_is_independent_of_overlapping_triangle_order() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");
        for route in &map.route_surfaces()[..2] {
            let mut reordered = route.clone();
            reordered.triangle_indices = route
                .triangle_indices
                .chunks_exact(3)
                .rev()
                .flatten()
                .copied()
                .collect();
            for triangle in route.triangle_indices.chunks_exact(3) {
                let vertices = [
                    route.vertices[triangle[0] as usize].position,
                    route.vertices[triangle[1] as usize].position,
                    route.vertices[triangle[2] as usize].position,
                ];
                let center = Position::new(
                    (vertices[0].x + vertices[1].x + vertices[2].x) / 3.0,
                    (vertices[0].y + vertices[1].y + vertices[2].y) / 3.0,
                );
                assert_eq!(route.sample_at(center), reordered.sample_at(center));
            }
        }
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
    fn authored_segment_operations_reach_the_map() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");

        let excavating = map
            .route_surface("route_0004")
            .expect("the overworld carries an excavating Path");
        assert_eq!(
            excavating.segments[0].operation,
            RouteSegmentOperation::Subtractive {
                clearance_above_meters: 2.0
            }
        );
        for route in map.route_surfaces() {
            if route.route_surface_id == "route_0004" {
                continue;
            }
            for segment in &route.segments {
                assert_eq!(segment.operation, RouteSegmentOperation::Additive);
            }
        }
    }

    #[test]
    fn an_excavating_path_carries_its_derived_cut_cells() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");

        assert_eq!(map.water_cell_meters(), 0.5);
        assert_eq!(map.route_surface_cuts().len(), 1);
        let cut = &map.route_surface_cuts()[0];
        assert_eq!(cut.route_surface_id, "route_0004");
        assert_eq!(cut.cells.len(), 245);

        let route = map
            .route_surface(&cut.route_surface_id)
            .expect("a cut names its own Path");
        for cell in &cut.cells {
            assert!(
                route.segments.iter().any(|segment| {
                    segment.segment_id == cell.segment_id
                        && matches!(segment.operation, RouteSegmentOperation::Subtractive { .. })
                }),
                "every cell names a subtractive segment of its Path"
            );
            assert_eq!(cell.floor_meters, 1.0);
            assert_eq!(cell.cut_top_meters, 3.0);
        }
    }

    #[test]
    fn an_operation_that_disagrees_with_its_clearance_is_rejected() {
        let source =
            embedded_instance_source(TEST_SCENE_ID).expect("the embedded overworld source exists");
        let mutate = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut document: serde_json::Value =
                serde_json::from_str(source).expect("the embedded export is JSON");
            change(&mut document);
            let mutated =
                serde_json::to_string(&document).expect("the mutated export remains JSON");
            WorldMap::from_source(&mutated, TEST_SCENE_ID)
                .expect_err("a contradictory export must be rejected")
                .to_string()
        };

        let additive_with_clearance = mutate(&|document| {
            document["scene"]["route_surfaces"][0]["segments"][0]["clearance_above_meters"] =
                serde_json::Value::from(2.0);
        });
        assert!(additive_with_clearance.contains("operation and clearance disagree"));

        let subtractive_without_clearance = mutate(&|document| {
            document["scene"]["route_surfaces"][3]["segments"][0]["clearance_above_meters"] =
                serde_json::Value::Null;
        });
        assert!(subtractive_without_clearance.contains("operation and clearance disagree"));

        let disagreeing_bake = mutate(&|document| {
            document["route_surface_bakes"][3]["segments"][0]["operation"] =
                serde_json::Value::from("additive");
            document["route_surface_bakes"][3]["segments"][0]["clearance_above_meters"] =
                serde_json::Value::Null;
        });
        assert!(disagreeing_bake.contains("baked segment mapping"));

        let cut_without_excavation = mutate(&|document| {
            document["route_surface_cut_raster"][0]["route_surface_id"] =
                serde_json::Value::from("route_0001");
        });
        assert!(cut_without_excavation.contains("names no excavating Path"));

        let missing_cut = mutate(&|document| {
            document["route_surface_cut_raster"] = serde_json::Value::Array(Vec::new());
        });
        assert!(missing_cut.contains("needs its derived cut cells"));
    }

    fn elevations(surfaces: &[MapColumnSurface<'_>]) -> Vec<f32> {
        surfaces
            .iter()
            .map(|surface| surface.elevation_meters)
            .collect()
    }

    fn water_cell_center(map: &WorldMap, x: u32, y: u32) -> Position {
        Position::new(
            -map.width_meters() / 2.0 + (x as f32 + 0.5) * map.water_cell_meters(),
            -map.height_meters() / 2.0 + (y as f32 + 0.5) * map.water_cell_meters(),
        )
    }

    #[test]
    fn an_excavated_column_presents_the_floor_and_the_ground_still_above_it() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");
        let cut = &map.route_surface_cuts()[0];
        let mut surfaces = Vec::new();

        let under_the_hill = cut
            .cells
            .iter()
            .find(|cell| {
                map.terrain_cell_at(water_cell_center(&map, cell.x, cell.y))
                    .is_some_and(|terrain| terrain.elevation_meters > cell.cut_top_meters)
            })
            .expect("the excavation reaches ground that stands above it");
        let hill_top = map
            .terrain_cell_at(water_cell_center(&map, under_the_hill.x, under_the_hill.y))
            .expect("that cell has Terrain")
            .elevation_meters;
        map.terrain_walking_surfaces(
            water_cell_center(&map, under_the_hill.x, under_the_hill.y),
            &mut surfaces,
        );
        assert_eq!(
            elevations(&surfaces),
            vec![under_the_hill.floor_meters, hill_top]
        );

        let over_flat_ground = cut
            .cells
            .iter()
            .find(|cell| {
                map.terrain_cell_at(water_cell_center(&map, cell.x, cell.y))
                    .is_some_and(|terrain| terrain.elevation_meters <= cell.floor_meters)
            })
            .expect("the same excavation also crosses flat ground");
        map.terrain_walking_surfaces(
            water_cell_center(&map, over_flat_ground.x, over_flat_ground.y),
            &mut surfaces,
        );
        assert_eq!(
            elevations(&surfaces),
            vec![
                map.terrain_cell_at(water_cell_center(
                    &map,
                    over_flat_ground.x,
                    over_flat_ground.y
                ))
                .expect("that cell has Terrain")
                .elevation_meters
            ],
            "an excavation that starts at the surface removes nothing"
        );
    }

    #[test]
    fn a_column_under_water_offers_its_bed_and_no_surface_of_the_water_itself() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");
        let body = map
            .water_bodies()
            .first()
            .expect("the overworld carries water");
        let mut surfaces = Vec::new();
        let mut submerged = 0;

        for cell in &body.cells {
            // A cell two bodies both claim resolves to whichever the index kept,
            // so this asks only about the ones this body has to itself.
            if map.water_bodies().iter().any(|other| {
                other.water_body_id != body.water_body_id
                    && other
                        .cells
                        .iter()
                        .any(|shared| shared.x == cell.x && shared.y == cell.y)
            }) {
                continue;
            }
            let center = water_cell_center(&map, cell.x, cell.y);
            let Some(terrain) = map.terrain_cell_at(center) else {
                continue;
            };
            map.terrain_walking_surfaces(center, &mut surfaces);
            let Some(bed) = surfaces
                .iter()
                .find(|surface| surface.elevation_meters < cell.surface_meters)
            else {
                continue;
            };
            submerged += 1;
            assert_eq!(
                bed.water_depth_meters,
                cell.surface_meters - bed.elevation_meters,
                "a surface under water reports how deep the water over it stands"
            );
            assert_eq!(
                bed.surface, terrain.surface,
                "and it is still the Terrain's own surface"
            );
            assert!(
                surfaces
                    .iter()
                    .all(|surface| surface.surface != body.surface),
                "the water is no surface of its own to stand on"
            );
        }

        assert!(submerged > 0, "the water stands over ground somewhere");
    }

    #[test]
    fn ground_no_excavation_reaches_presents_one_surface() {
        let map = WorldMap::load_embedded(TEST_SCENE_ID)
            .expect("the embedded overworld Instance is valid");
        let cell = map.terrain_cell(0, 0).expect("the world has a first cell");
        let mut surfaces = Vec::new();

        map.terrain_walking_surfaces(cell.center, &mut surfaces);

        assert_eq!(elevations(&surfaces), vec![cell.elevation_meters]);
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
    fn extended_with_adds_new_entries_without_disturbing_the_base_table() {
        let base = PlacementRanks::from_entries([("grass", 10), ("tree", 20)])
            .expect("the base table is valid");

        let extended = base
            .extended_with([("shrine", 200)])
            .expect("adding a new Asset key is allowed");

        assert_eq!(extended.rank("grass"), Some(10));
        assert_eq!(extended.rank("tree"), Some(20));
        assert_eq!(extended.rank("shrine"), Some(200));
    }

    #[test]
    fn extended_with_refuses_to_override_a_base_entry() {
        let base = PlacementRanks::from_entries([("grass", 10)]).expect("the base table is valid");

        let error = base
            .extended_with([("grass", 999)])
            .expect_err("an overlay may add ranks but never override one");

        assert!(error.to_string().contains("grass"));
    }

    #[test]
    fn extended_with_refuses_an_empty_or_duplicated_key() {
        let base = PlacementRanks::from_entries([("grass", 10)]).expect("the base table is valid");

        assert!(
            base.clone()
                .extended_with(std::iter::empty::<(&str, u32)>())
                .is_ok(),
            "adding nothing at all is not an error"
        );
        assert!(base.clone().extended_with([("", 1)]).is_err());
        assert!(base.extended_with([("shrine", 1), ("shrine", 2)]).is_err());
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
                &export_document(10, 12, TEST_GRASS_CELL, "", ""),
                TEST_SCENE_ID
            )
            .is_err()
        );
        assert!(
            WorldMap::from_source(
                &export_document(11, 11, TEST_GRASS_CELL, "", ""),
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

    #[test]
    fn terrain_lookup_returns_the_authored_cell_at_a_world_position() {
        let source = test_export(
            r#"
                { "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": 2.5 },
                { "x": 1, "y": 0, "asset_key": "grass", "elevation_meters": 4.0 }
            "#,
            ANKH,
        );
        let map =
            WorldMap::from_source(&source, TEST_SCENE_ID).expect("the synthetic export is valid");

        assert_eq!(
            map.terrain_cell_at(Position::new(-1.75, -1.5))
                .map(|cell| cell.elevation_meters),
            Some(2.5)
        );
        assert_eq!(
            map.terrain_world_position_at(Position::new(-1.75, -1.5)),
            Some(WorldPosition::new(-1.75, -1.5, 2.5))
        );
        assert_eq!(
            map.terrain_cell_at(Position::new(-1.0, -1.5))
                .map(|cell| cell.elevation_meters),
            Some(4.0)
        );
        assert!(map.terrain_cell_at(Position::new(0.0, 0.0)).is_none());
        assert!(map.terrain_cell_at(Position::new(f32::NAN, 0.0)).is_none());
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

    fn present_bodies(map: &WorldMap) -> Vec<String> {
        map.present_water_bodies()
            .map(|body| body.water_body_id.clone())
            .collect()
    }

    /// Whether a body would have water, worked out the other way round from the
    /// map's own rule: by walking up the feeders looking for a way to a body
    /// that is fed by nothing, along which every switch is on.
    ///
    /// The map settles this by removing bodies until nothing changes. Asking it
    /// as a question about paths instead is what makes the answer worth
    /// comparing rather than a second copy of the same loop.
    fn reaches_running_water(
        map: &WorldMap,
        id: &str,
        off: &str,
        walked: &mut Vec<String>,
    ) -> bool {
        if walked.iter().any(|seen| seen == id) {
            return false;
        }
        let Some(body) = map
            .water_bodies()
            .iter()
            .find(|body| body.water_body_id == id)
        else {
            return false;
        };
        if body.switch.as_deref() == Some(off) {
            return false;
        }
        if body.feeders.is_empty() {
            return true;
        }
        walked.push(id.to_owned());
        let fed = body
            .feeders
            .iter()
            .any(|feeder| reaches_running_water(map, feeder, off, walked));
        walked.pop();
        fed
    }

    /// A switch with something hanging under it that carries no switch of its
    /// own - which is the only shape in which the cascade is worth anything.
    fn switch_with_descendants(map: &WorldMap) -> String {
        map.switches()
            .iter()
            .map(|switch| switch.name.as_str())
            .find(|switch| {
                map.water_bodies().iter().any(|body| {
                    body.switch.as_deref() != Some(switch)
                        && !reaches_running_water(map, &body.water_body_id, switch, &mut Vec::new())
                })
            })
            .map(str::to_owned)
            .expect("some switch has water hanging under it that carries no switch of its own")
    }

    #[test]
    fn every_body_hangs_on_a_declared_switch_and_is_fed_by_one_this_map_carries() {
        let map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        let declared = map
            .switches()
            .iter()
            .map(|switch| switch.name.as_str())
            .collect::<HashSet<_>>();

        assert!(!declared.is_empty(), "the overworld authors switches");
        assert!(map.switches().iter().all(|switch| !switch.name.is_empty()));
        assert!(
            map.water_bodies().iter().any(|body| body.switch.is_some()),
            "and at least one body hangs on one"
        );

        for body in map.water_bodies() {
            if let Some(switch) = &body.switch {
                assert!(
                    declared.contains(switch.as_str()),
                    "a body may only hang on a switch the map declares"
                );
            }
            for feeder in &body.feeders {
                assert!(
                    map.water_bodies()
                        .iter()
                        .any(|other| &other.water_body_id == feeder),
                    "a body is fed by one this map carries"
                );
            }
            assert!(
                reaches_running_water(&map, &body.water_body_id, "", &mut Vec::new()),
                "with every switch on, every body is fed from somewhere rather than a ring"
            );
        }
    }

    #[test]
    fn switching_a_river_off_takes_every_body_hanging_under_it() {
        let mut map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        for switch in map.switches().to_vec() {
            map.set_switch(&switch.name, true)
                .expect("the map declares its own switches");
        }
        let everything = present_bodies(&map);
        assert_eq!(
            everything.len(),
            map.water_bodies().len(),
            "with every switch on there is no water missing"
        );

        let off = switch_with_descendants(&map);
        let expected = map
            .water_bodies()
            .iter()
            .filter(|body| reaches_running_water(&map, &body.water_body_id, &off, &mut Vec::new()))
            .map(|body| body.water_body_id.clone())
            .collect::<Vec<_>>();
        let carries_it = map
            .water_bodies()
            .iter()
            .filter(|body| body.switch.as_deref() == Some(off.as_str()))
            .count();

        assert!(
            everything.len() - expected.len() > carries_it,
            "switching '{off}' off takes more bodies than the ones that name it"
        );

        assert!(
            map.set_switch(&off, false)
                .expect("the map declares that switch")
        );
        assert_eq!(present_bodies(&map), expected);

        assert!(
            !map.set_switch(&off, false)
                .expect("the map declares that switch"),
            "moving a switch to where it already stands changes nothing"
        );
        assert!(
            map.set_switch("no_such_switch", true).is_err(),
            "a switch the map never declared cannot be moved"
        );
        assert_eq!(map.switch_is_on("no_such_switch"), None);
    }

    #[test]
    fn a_cell_keeps_water_exactly_while_a_body_over_it_still_runs() {
        let mut map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        for switch in map.switches().to_vec() {
            map.set_switch(&switch.name, true)
                .expect("the map declares its own switches");
        }
        let off = switch_with_descendants(&map);
        let gone = map
            .water_bodies()
            .iter()
            .filter(|body| !reaches_running_water(&map, &body.water_body_id, &off, &mut Vec::new()))
            .flat_map(|body| body.cells.iter().map(|cell| (cell.x, cell.y)))
            .collect::<HashSet<_>>();
        let stays = map
            .water_bodies()
            .iter()
            .filter(|body| reaches_running_water(&map, &body.water_body_id, &off, &mut Vec::new()))
            .flat_map(|body| body.cells.iter().map(|cell| (cell.x, cell.y)))
            .collect::<HashSet<_>>();
        assert!(
            !gone.is_empty(),
            "switching '{off}' off empties some ground"
        );
        assert!(
            gone.intersection(&stays).next().is_some(),
            "and some of that ground is shared with water that stays, which is the case worth checking"
        );

        map.set_switch(&off, false)
            .expect("the map declares that switch");

        let mut surfaces = Vec::new();
        for at in &gone {
            map.terrain_walking_surfaces(water_cell_center(&map, at.0, at.1), &mut surfaces);
            let wet = surfaces
                .iter()
                .any(|surface| surface.water_depth_meters > 0.0);
            assert_eq!(
                wet,
                stays.contains(at),
                "cell {at:?} carries water only while a body that still runs covers it"
            );
        }
    }

    #[test]
    fn a_switched_off_body_leaves_the_ground_whole_and_a_running_one_a_bed_under_water() {
        let mut map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        let switchable = map
            .water_bodies()
            .iter()
            .find(|body| body.switch.is_some())
            .expect("the overworld carries a switchable body");
        let switch = switchable
            .switch
            .clone()
            .expect("that body names its switch");
        // Ground this body has to itself, so the reading is about it alone.
        let alone = switchable
            .cells
            .iter()
            .find(|cell| {
                map.water_bodies()
                    .iter()
                    .filter(|other| other.water_body_id != switchable.water_body_id)
                    .all(|other| {
                        !other
                            .cells
                            .iter()
                            .any(|shared| shared.x == cell.x && shared.y == cell.y)
                    })
            })
            .copied()
            .expect("it has ground of its own");
        let at = water_cell_center(&map, alone.x, alone.y);
        let ground = map
            .terrain_cell_at(at)
            .expect("that cell has Terrain")
            .elevation_meters;

        // Each reading is taken in its own scope: a resolved column borrows the
        // map it came from, and the switch between them needs it back.
        map.set_switch(&switch, false)
            .expect("the map declares that switch");
        {
            let mut surfaces = Vec::new();
            map.terrain_walking_surfaces(at, &mut surfaces);
            assert_eq!(
                elevations(&surfaces),
                vec![ground],
                "a body that is not there takes no channel out of the Terrain"
            );
            assert_eq!(surfaces[0].water_depth_meters, 0.0);
        }

        map.set_switch(&switch, true)
            .expect("the map declares that switch");
        {
            let mut surfaces = Vec::new();
            map.terrain_walking_surfaces(at, &mut surfaces);
            assert_eq!(
                elevations(&surfaces),
                vec![alone.bed_meters],
                "and running, the same place offers the bed the channel was cut to"
            );
            assert_eq!(
                surfaces[0].water_depth_meters,
                alone.surface_meters - alone.bed_meters
            );
            assert!(
                surfaces[0].water_depth_meters > 0.4,
                "deeper than any Character wades, so it is not crossed"
            );
        }
    }

    #[test]
    fn two_bodies_over_one_cell_stack_instead_of_the_last_one_winning() {
        let map = WorldMap::load_embedded("stack01").expect("embedded Instance is valid");
        let mut surfaces = Vec::new();

        // Where the two fills overlap - 0.0 to 1.0 under 0.5 to 1.5 - they are
        // one body of water from the lower bed to the higher surface, and the
        // two cuts unite to leave one floor at the bottom.
        let mut overlapping = 0;
        for x in 12..=19 {
            for y in 16..=23 {
                map.terrain_walking_surfaces(water_cell_center(&map, x, y), &mut surfaces);
                assert_eq!(
                    elevations(&surfaces),
                    vec![0.0],
                    "the united cut leaves one floor at ({x}, {y})"
                );
                assert_eq!(
                    surfaces[0].water_depth_meters, 1.5,
                    "the water at ({x}, {y}) stands from the lower bed to the higher surface"
                );
                overlapping += 1;
            }
        }
        assert_eq!(overlapping, 64);

        // Where they are disjoint the upper fill is an aqueduct: it survives
        // the cut that took the Terrain under it away, and it stands over no
        // ground at all.
        let mut disjoint = 0;
        for x in 34..=45 {
            for y in 16..=23 {
                map.terrain_walking_surfaces(water_cell_center(&map, x, y), &mut surfaces);
                assert_eq!(
                    elevations(&surfaces),
                    vec![0.0],
                    "the deep river's cut reaches through the upper fill at ({x}, {y})"
                );
                assert_eq!(
                    surfaces[0].water_depth_meters, 1.0,
                    "only the fill the ground lies in is standing on it"
                );
                disjoint += 1;
            }
        }
        assert_eq!(disjoint, 96);
    }

    #[test]
    fn the_last_body_read_would_answer_differently_where_they_stack() {
        // Guards the test above against passing for the wrong reason: if a cell
        // kept one body instead of all of them, these are the answers it would
        // give, and none of them is what the assertions there expect.
        let map = WorldMap::load_embedded("stack01").expect("embedded Instance is valid");
        let bodies = map.water_bodies();
        let cell_of = |id: &str, x: u32, y: u32| {
            bodies
                .iter()
                .find(|body| body.water_body_id == id)
                .and_then(|body| {
                    body.cells
                        .iter()
                        .find(|cell| cell.x == x && cell.y == y)
                        .copied()
                })
        };

        let deep = cell_of("river_0001", 15, 20).expect("the deep river crosses there");
        let crossing = cell_of("river_0002", 15, 20).expect("and so does the one over it");
        assert_ne!(
            (deep.bed_meters, deep.surface_meters),
            (crossing.bed_meters, crossing.surface_meters),
            "the two bodies say different things about the same cell"
        );
        assert_ne!(deep.surface_meters - deep.bed_meters, 1.5);
        assert_ne!(crossing.surface_meters - crossing.bed_meters, 1.5);

        let aqueduct = cell_of("river_0003", 40, 20).expect("the aqueduct crosses there");
        assert_eq!(
            aqueduct.bed_meters, 2.0,
            "keeping only the aqueduct would put the ground two metres up"
        );
    }

    #[test]
    fn the_embedded_world_carries_its_authored_water() {
        let map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        let bodies = map.water_bodies();

        assert!(
            bodies.len() > 1,
            "the overworld carries a river and the branches that leave it"
        );
        for body in bodies {
            assert_eq!(body.water_kind, "river");
            assert_eq!(body.surface, "water");
            assert!(!body.cells.is_empty());
            assert!(
                body.cells.iter().all(|cell| {
                    cell.bed_meters < cell.surface_meters
                        && cell.surface_meters <= cell.cut_top_meters
                }),
                "a water cell stacks bed, surface and cut in that order"
            );
            assert!(body.vertices.len() >= 3, "a body arrives with a band");
            assert!(
                body.triangle_indices.len() % 3 == 0
                    && body
                        .triangle_indices
                        .iter()
                        .all(|index| (*index as usize) < body.vertices.len())
            );
            assert!(body.centerline_samples.len() >= 2);
            let length = body.length_meters();
            assert!(length > 0.0);
            assert!(
                body.cells
                    .iter()
                    .all(|cell| cell.station_meters >= 0.0 && cell.station_meters <= length),
                "every cell sits somewhere on its own course"
            );
            let start = body.flow_at(0.0).expect("a course starts somewhere");
            let end = body.flow_at(length).expect("and ends somewhere");
            assert_eq!(
                start.position, body.centerline_samples[0].position,
                "station zero is the end the author drew first"
            );
            assert_eq!(
                end.position,
                body.centerline_samples[body.centerline_samples.len() - 1].position
            );
            assert!(body.flow_at(length + 1.0).is_none(), "a course is finite");
            for station in [0.0, length * 0.25, length * 0.5, length] {
                let flow = body.flow_at(station).expect("a station inside the course");
                assert!(
                    (flow.direction[0].hypot(flow.direction[1]) - 1.0).abs() < 1.0e-4,
                    "the direction of the water is a unit vector"
                );
                assert!(flow.width_meters > 0.0);
            }
            assert_eq!(
                body.centerline_samples
                    .first()
                    .map(|sample| sample.station_meters),
                Some(0.0),
                "the course is measured from the end the author drew first"
            );
        }
    }

    #[test]
    fn the_embedded_world_carries_its_authored_bridges() {
        let map = WorldMap::load_embedded("overworld01").expect("embedded Instance is valid");
        let half_width = map.width_meters() * 0.5;
        let half_height = map.height_meters() * 0.5;

        assert_eq!(map.bridges().len(), 2);
        for bridge in map.bridges() {
            assert_eq!(bridge.plank_asset_key, "plank");
            assert_eq!(bridge.anchor_asset_key, "post");
            assert_eq!(bridge.surface, "wood");
            assert_eq!(bridge.vertices.len(), 4);
            assert_eq!(bridge.posts.len(), 4);
            assert!(bridge.centerline_samples.len() >= 2);
            assert_eq!(
                bridge
                    .centerline_samples
                    .first()
                    .map(|sample| sample.station_meters),
                Some(0.0)
            );
            assert!(
                bridge
                    .centerline_samples
                    .iter()
                    .all(|sample| sample.elevation_meters == bridge.elevation_meters),
                "a deck is level, so its centerline sits at its own elevation"
            );
            for ground in [&bridge.ground_at_start, &bridge.ground_at_end] {
                let ground = ground
                    .as_ref()
                    .expect("both ends of an authored bridge rest over something");
                assert!(!ground.asset_key.is_empty());
                assert!(ground.elevation_meters.is_finite());
            }
            assert!(!bridge.planks.is_empty());
            assert!(bridge.heading_radians.is_finite());
            assert!(
                bridge
                    .triangle_indices
                    .iter()
                    .all(|index| (*index as usize) < bridge.vertices.len())
            );
            assert!(
                bridge
                    .planks
                    .iter()
                    .all(|plank| plank.depth_meters > 0.0 && plank.width_meters > 0.0),
                "every plank arrives with the two measures it is drawn from"
            );
            for position in bridge
                .vertices
                .iter()
                .map(|vertex| vertex.position)
                .chain(bridge.planks.iter().map(|plank| plank.position))
                .chain(bridge.posts.iter().map(|post| post.position))
            {
                assert!(
                    position.x.abs() <= half_width && position.y.abs() <= half_height,
                    "a bridge arrives in the same centred frame as the rest of the world"
                );
            }
        }
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
