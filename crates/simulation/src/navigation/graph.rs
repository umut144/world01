//! The derived ground navigation representation.
//!
//! A node is a place a grounded Actor can stand: a horizontal position, the
//! physical height it stands at, and the identity of the support whose height
//! that is. Terrain contributes one node per walking surface its column
//! resolves to - two where an authored Path has been driven through a hill,
//! one everywhere else - and each Route surface contributes one node per baked
//! centerline sample.
//!
//! Nodes deliberately carry no judgement about who may use them. Which
//! surfaces a Character may cross is its own traversal profile's answer, given
//! when a path is asked for, so one derived world serves every Character. The
//! one thing a node does decide is whether the world's own collision geometry
//! stands there, because that is the same for everybody.

use std::{error::Error, fmt};

use bevy::prelude::Vec2;
use world01_content::WorldCollisionGeometryCatalog;
use world01_world_data::{GroundSupport, Position, WorldMap};

use super::{CharacterTraversalProfile, TraversalSpeed};
use crate::spatial::broadphase::{Aabb, WorldColliderGrid};
use crate::spatial::overlap::{GeometryTransform, point_in_component};

/// The eight directions Terrain movement may take, as cell offsets.
const TERRAIN_DIRECTIONS: [(i64, i64); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
];

/// One surface name inside a graph.
///
/// A world repeats very few surface names across very many nodes, so a node
/// refers to one instead of carrying its own copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NavigationSurface(u16);

/// A place a grounded Actor can stand, and the support whose height it follows.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundNavigationNode {
    position: Position,
    elevation_meters: f32,
    surface: NavigationSurface,
    support: GroundSupport,
    blocked: bool,
}

impl GroundNavigationNode {
    pub const fn position(&self) -> Position {
        self.position
    }

    pub const fn elevation_meters(&self) -> f32 {
        self.elevation_meters
    }

    pub const fn surface(&self) -> NavigationSurface {
        self.surface
    }

    pub const fn support(&self) -> &GroundSupport {
        &self.support
    }

    /// Whether the world's own collision geometry stands at this node.
    ///
    /// The test is the node's point, not a Character's body, so a gap narrower
    /// than a Character still offers unblocked nodes. Fitting through is the
    /// mover's problem, not the topology's.
    pub const fn is_blocked(&self) -> bool {
        self.blocked
    }
}

/// The nodes a composed world offers a grounded Actor.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundNavigationGraph {
    surfaces: Vec<String>,
    nodes: Vec<GroundNavigationNode>,
    width_tiles: u32,
    height_tiles: u32,
    terrain_cell_meters: f32,
    /// The nodes each Terrain cell's column resolved to, by row-major cell.
    cell_nodes: Vec<TerrainCellNodes>,
    /// The cell each Terrain node stands in. Terrain nodes are the leading run
    /// of `nodes`, so an index beyond this is a Route node.
    node_cells: Vec<u32>,
}

/// Where one Terrain cell's nodes sit inside the node list.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct TerrainCellNodes {
    first: u32,
    count: u32,
}

/// One move a Character may make from a node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundNavigationStep {
    pub node: usize,
    pub distance_meters: f32,
    pub speed: TraversalSpeed,
}

impl GroundNavigationGraph {
    /// Derives the nodes of one composed world.
    ///
    /// Terrain heights come from resolving the cell's column at the node's own
    /// position, so an excavated place offers the floor of the excavation and
    /// the ground still standing above it as two separate places to be. Route
    /// heights come from the same projection onto the baked centerline that
    /// carries a walking Actor, rather than from the sample's stored height, so
    /// a node cannot describe a height the Route would not actually give.
    ///
    /// The grid only narrows which colliders each node is tested against; the
    /// shared narrow phase still decides.
    pub fn from_world(
        map: &WorldMap,
        collision: &WorldCollisionGeometryCatalog,
        grid: &WorldColliderGrid,
    ) -> Result<Self, GroundNavigationError> {
        let mut surfaces = Vec::new();
        let mut nodes = Vec::with_capacity(map.terrain_cells().len());
        let mut candidates = Vec::new();
        let cell_count = (map.width_tiles() as usize).saturating_mul(map.height_tiles() as usize);
        let mut cell_nodes = vec![TerrainCellNodes::default(); cell_count];
        let mut node_cells = Vec::with_capacity(map.terrain_cells().len());

        let mut column = Vec::new();
        for cell in map.terrain_cells() {
            let surface = intern(&mut surfaces, &cell.surface)?;
            map.terrain_walking_surfaces(cell.center, &mut column);
            if column.is_empty() {
                return Err(GroundNavigationError(format!(
                    "Terrain cell ({}, {}) resolves to no walking surface",
                    cell.x, cell.y
                )));
            }
            // World collision is planar, so it says the same thing about every
            // height this column offers.
            let blocked = stands_in_world_collision(cell.center, collision, grid, &mut candidates);
            let first = u32::try_from(nodes.len()).map_err(|_| {
                GroundNavigationError("a world cannot hold more nodes than a `u32` counts".into())
            })?;
            let Some(index) = cell_index(map.width_tiles(), map.height_tiles(), cell.x, cell.y)
            else {
                return Err(GroundNavigationError(format!(
                    "Terrain cell ({}, {}) lies outside its own map",
                    cell.x, cell.y
                )));
            };
            let count = u32::try_from(column.len()).unwrap_or(u32::MAX);
            cell_nodes[index] = TerrainCellNodes { first, count };
            for elevation_meters in column.iter().copied() {
                node_cells.push(u32::try_from(index).unwrap_or(u32::MAX));
                nodes.push(GroundNavigationNode {
                    position: cell.center,
                    elevation_meters,
                    surface,
                    support: GroundSupport::Terrain,
                    blocked,
                });
            }
        }

        for route in map.route_surfaces() {
            let surface = intern(&mut surfaces, &route.surface)?;
            for (index, sample) in route.centerline_samples.iter().enumerate() {
                let Some(resolved) = route.sample_at(sample.position) else {
                    return Err(GroundNavigationError(format!(
                        "Route '{}' does not support its own centerline sample {index}",
                        route.route_surface_id
                    )));
                };
                nodes.push(GroundNavigationNode {
                    position: sample.position,
                    elevation_meters: resolved.elevation_meters,
                    surface,
                    support: GroundSupport::RouteSurface {
                        route_surface_id: route.route_surface_id.clone(),
                    },
                    blocked: stands_in_world_collision(
                        sample.position,
                        collision,
                        grid,
                        &mut candidates,
                    ),
                });
            }
        }

        Ok(Self {
            surfaces,
            nodes,
            width_tiles: map.width_tiles(),
            height_tiles: map.height_tiles(),
            terrain_cell_meters: map.terrain_cell_meters(),
            cell_nodes,
            node_cells,
        })
    }

    /// Fills `steps` with the places a Character with this profile may reach
    /// from one node in a single move, and clears whatever was there before.
    ///
    /// Terrain moves in eight directions. A diagonal needs both of the
    /// orthogonal cells it passes between to offer a step of their own, so a
    /// Character cannot slip through the corner between two obstacles. Which
    /// heights connect is the profile's own step rule, so the two nodes of an
    /// excavated column join floor to floor and ground to ground rather than
    /// across.
    pub fn steps_from(
        &self,
        node: usize,
        profile: &CharacterTraversalProfile,
        steps: &mut Vec<GroundNavigationStep>,
    ) {
        steps.clear();
        let Some(from) = self.nodes.get(node) else {
            return;
        };
        if !self.is_usable(from, profile) {
            return;
        }
        let Some(cell) = self.node_cells.get(node).copied() else {
            return;
        };
        let Some((x, y)) = self.cell_position(cell) else {
            return;
        };

        let diagonal = self.terrain_cell_meters * std::f32::consts::SQRT_2;
        for (offset_x, offset_y) in TERRAIN_DIRECTIONS {
            if offset_x != 0
                && offset_y != 0
                && !(self.offers_a_step(from, profile, x, y, offset_x, 0)
                    && self.offers_a_step(from, profile, x, y, 0, offset_y))
            {
                continue;
            }
            let distance_meters = if offset_x != 0 && offset_y != 0 {
                diagonal
            } else {
                self.terrain_cell_meters
            };
            for reachable in self.reachable_in(from, profile, x, y, offset_x, offset_y) {
                steps.push(GroundNavigationStep {
                    node: reachable,
                    distance_meters,
                    speed: TraversalSpeed::Normal,
                });
            }
        }
    }

    fn is_usable(&self, node: &GroundNavigationNode, profile: &CharacterTraversalProfile) -> bool {
        !node.blocked
            && self
                .surface_name(node.surface())
                .is_some_and(|surface| profile.permits_surface(surface))
    }

    fn offers_a_step(
        &self,
        from: &GroundNavigationNode,
        profile: &CharacterTraversalProfile,
        x: u32,
        y: u32,
        offset_x: i64,
        offset_y: i64,
    ) -> bool {
        self.reachable_in(from, profile, x, y, offset_x, offset_y)
            .next()
            .is_some()
    }

    fn reachable_in<'a>(
        &'a self,
        from: &'a GroundNavigationNode,
        profile: &'a CharacterTraversalProfile,
        x: u32,
        y: u32,
        offset_x: i64,
        offset_y: i64,
    ) -> impl Iterator<Item = usize> + 'a {
        let neighbour = offset_cell(
            self.width_tiles,
            self.height_tiles,
            x,
            y,
            offset_x,
            offset_y,
        )
        .and_then(|index| self.cell_nodes.get(index).copied())
        .unwrap_or_default();
        let first = neighbour.first as usize;
        (first..first + neighbour.count as usize).filter(move |index| {
            self.nodes.get(*index).is_some_and(|node| {
                self.is_usable(node, profile)
                    && profile.permits_step(from.elevation_meters, node.elevation_meters)
            })
        })
    }

    fn cell_position(&self, index: u32) -> Option<(u32, u32)> {
        let index = index as usize;
        if index >= self.cell_nodes.len() || self.width_tiles == 0 {
            return None;
        }
        let width = self.width_tiles as usize;
        Some(((index % width) as u32, (index / width) as u32))
    }

    pub fn nodes(&self) -> &[GroundNavigationNode] {
        &self.nodes
    }

    pub fn surface_name(&self, surface: NavigationSurface) -> Option<&str> {
        self.surfaces
            .get(usize::from(surface.0))
            .map(String::as_str)
    }
}

fn cell_index(width_tiles: u32, height_tiles: u32, x: u32, y: u32) -> Option<usize> {
    (x < width_tiles && y < height_tiles).then(|| y as usize * width_tiles as usize + x as usize)
}

fn offset_cell(
    width_tiles: u32,
    height_tiles: u32,
    x: u32,
    y: u32,
    offset_x: i64,
    offset_y: i64,
) -> Option<usize> {
    let x = u32::try_from(i64::from(x) + offset_x).ok()?;
    let y = u32::try_from(i64::from(y) + offset_y).ok()?;
    cell_index(width_tiles, height_tiles, x, y)
}

fn stands_in_world_collision(
    position: Position,
    collision: &WorldCollisionGeometryCatalog,
    grid: &WorldColliderGrid,
    candidates: &mut Vec<u32>,
) -> bool {
    let point = Vec2::new(position.x, position.y);
    grid.candidates(
        Aabb {
            minimum: point,
            maximum: point,
        },
        candidates,
    );
    candidates.iter().any(|index| {
        collision
            .regions
            .get(*index as usize)
            .is_some_and(|region| {
                point_in_component(
                    region.component.geometry(),
                    GeometryTransform::translated(region.position),
                    point,
                )
            })
    })
}

fn intern(
    surfaces: &mut Vec<String>,
    name: &str,
) -> Result<NavigationSurface, GroundNavigationError> {
    if let Some(known) = surfaces.iter().position(|surface| surface.as_str() == name) {
        return index_of(known);
    }
    let added = index_of(surfaces.len())?;
    surfaces.push(name.to_owned());
    Ok(added)
}

fn index_of(index: usize) -> Result<NavigationSurface, GroundNavigationError> {
    u16::try_from(index).map(NavigationSurface).map_err(|_| {
        GroundNavigationError("a world cannot name more surfaces than a `u16` counts".into())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundNavigationError(String);

impl fmt::Display for GroundNavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for GroundNavigationError {}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use world01_content::{
        CollisionComponentGeometry, PlacedCollisionGeometry, RuntimeComponentGeometry,
        RuntimeContent,
    };
    use world01_design::load_embedded as load_game_design;
    use world01_world_data::CharacterId;

    use crate::TraversalCatalog;

    use super::*;

    fn overworld() -> WorldMap {
        WorldMap::load_embedded("overworld01").expect("embedded Instance is valid")
    }

    fn derive(map: &WorldMap, collision: &WorldCollisionGeometryCatalog) -> GroundNavigationGraph {
        let grid = WorldColliderGrid::from_catalog(collision);
        GroundNavigationGraph::from_world(map, collision, &grid)
            .expect("embedded world derives nodes")
    }

    fn without_collision(map: &WorldMap) -> GroundNavigationGraph {
        derive(map, &WorldCollisionGeometryCatalog::default())
    }

    fn cell_center(map: &WorldMap, x: u32, y: u32) -> Position {
        map.terrain_cells()
            .iter()
            .find(|cell| cell.x == x && cell.y == y)
            .expect("the world has that Terrain cell")
            .center
    }

    fn node_at(graph: &GroundNavigationGraph, position: Position) -> &GroundNavigationNode {
        graph
            .nodes()
            .iter()
            .find(|node| node.position() == position)
            .expect("every authored cell has a node")
    }

    /// A standing silhouette: as wide as it is tall, and entirely on one side
    /// of the position it is placed at, the way an authored Prop Region is.
    fn standing_silhouette() -> CollisionComponentGeometry {
        CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: "trunk".into(),
            name: "trunk".into(),
            vertices: vec![
                Vec2::new(-0.9, 0.1),
                Vec2::new(0.9, 0.1),
                Vec2::new(0.9, 3.1),
                Vec2::new(-0.9, 3.1),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        })
        .expect("the test silhouette has valid collision topology")
    }

    #[test]
    fn every_walking_surface_and_centerline_sample_becomes_a_node() {
        let map = overworld();
        let graph = without_collision(&map);

        let samples = map
            .route_surfaces()
            .iter()
            .map(|route| route.centerline_samples.len())
            .sum::<usize>();
        let mut column = Vec::new();
        let walking_surfaces = map
            .terrain_cells()
            .iter()
            .map(|cell| {
                map.terrain_walking_surfaces(cell.center, &mut column);
                column.len()
            })
            .sum::<usize>();

        assert!(
            walking_surfaces > map.terrain_cells().len(),
            "the overworld is excavated somewhere"
        );
        assert_eq!(graph.nodes().len(), walking_surfaces + samples);
        assert_eq!(
            graph
                .nodes()
                .iter()
                .filter(|node| matches!(node.support(), GroundSupport::Terrain))
                .count(),
            walking_surfaces
        );
    }

    #[test]
    fn a_terrain_node_stands_where_its_column_resolves() {
        let map = overworld();
        let graph = without_collision(&map);
        let mut column = Vec::new();

        let excavated = map
            .terrain_cells()
            .iter()
            .find(|cell| {
                map.terrain_walking_surfaces(cell.center, &mut column);
                column.len() > 1
            })
            .expect("the overworld is excavated somewhere");

        for cell in [
            map.terrain_cells().first().expect("the world has Terrain"),
            excavated,
        ] {
            map.terrain_walking_surfaces(cell.center, &mut column);
            let standing = graph
                .nodes()
                .iter()
                .filter(|node| {
                    node.position() == cell.center
                        && matches!(node.support(), GroundSupport::Terrain)
                })
                .collect::<Vec<_>>();

            assert_eq!(
                standing
                    .iter()
                    .map(|node| node.elevation_meters())
                    .collect::<Vec<_>>(),
                column,
                "a cell offers exactly the places its column resolves to"
            );
            for node in standing {
                assert_eq!(
                    graph.surface_name(node.surface()),
                    Some(cell.surface.as_str())
                );
            }
        }
    }

    #[test]
    fn an_excavated_place_offers_the_tunnel_and_the_ground_above_it() {
        let map = overworld();
        let graph = without_collision(&map);
        let cut = map
            .route_surface_cuts()
            .first()
            .expect("the overworld carries an excavating Path");

        let deepest = graph
            .nodes()
            .iter()
            .filter(|node| matches!(node.support(), GroundSupport::Terrain))
            .filter(|node| {
                map.terrain_cell_at(node.position())
                    .is_some_and(|cell| cell.elevation_meters > node.elevation_meters())
            })
            .collect::<Vec<_>>();

        assert!(
            !deepest.is_empty(),
            "an excavation leaves places to stand below the ground above them"
        );
        for node in &deepest {
            let above = graph
                .nodes()
                .iter()
                .find(|other| {
                    other.position() == node.position()
                        && matches!(other.support(), GroundSupport::Terrain)
                        && other.elevation_meters() > node.elevation_meters()
                })
                .expect("the ground above an excavation is a place of its own");
            assert_eq!(
                above.elevation_meters(),
                map.terrain_cell_at(node.position())
                    .expect("that cell has Terrain")
                    .elevation_meters
            );
            assert!(
                cut.cells
                    .iter()
                    .any(|cell| cell.floor_meters == node.elevation_meters()),
                "the lower place is the floor the authored excavation left"
            );
        }
    }

    #[test]
    fn route_nodes_carry_their_authored_support_identity() {
        let map = overworld();
        let graph = without_collision(&map);

        let mut counted = HashMap::new();
        for node in graph.nodes() {
            if let GroundSupport::RouteSurface { route_surface_id } = node.support() {
                *counted.entry(route_surface_id.as_str()).or_insert(0usize) += 1;
            }
        }

        assert_eq!(counted.len(), map.route_surfaces().len());
        for route in map.route_surfaces() {
            assert_eq!(
                counted.get(route.route_surface_id.as_str()).copied(),
                Some(route.centerline_samples.len()),
                "Route '{}' contributes one node per centerline sample",
                route.route_surface_id
            );
        }
    }

    #[test]
    fn a_repeated_surface_is_named_once() {
        let map = overworld();
        let graph = without_collision(&map);

        let terrain = graph
            .nodes()
            .iter()
            .find(|node| matches!(node.support(), GroundSupport::Terrain))
            .expect("the world has Terrain");
        let route = graph
            .nodes()
            .iter()
            .find(|node| matches!(node.support(), GroundSupport::RouteSurface { .. }))
            .expect("the world has a Route");

        assert_eq!(terrain.surface(), route.surface());
        assert_eq!(graph.surface_name(terrain.surface()), Some("land"));
    }

    fn hammerer() -> CharacterTraversalProfile {
        let design = load_game_design().expect("embedded design loads");
        TraversalCatalog::from_design(&design.traversal)
            .expect("embedded traversal profiles are valid")
            .character(&CharacterId("hammerer".into()))
            .expect("the Hammerer has traversal rules")
            .clone()
    }

    fn index_at(graph: &GroundNavigationGraph, position: Position, elevation: f32) -> usize {
        graph
            .nodes()
            .iter()
            .position(|node| node.position() == position && node.elevation_meters() == elevation)
            .expect("that place has a node")
    }

    #[test]
    fn flat_ground_connects_in_eight_directions() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let mut steps = Vec::new();

        let cell = map.terrain_cell(5, 5).expect("the world has that cell");
        graph.steps_from(
            index_at(&graph, cell.center, cell.elevation_meters),
            &profile,
            &mut steps,
        );

        assert_eq!(steps.len(), 8);
        let diagonal = map.terrain_cell_meters() * std::f32::consts::SQRT_2;
        assert_eq!(
            steps
                .iter()
                .filter(|step| step.distance_meters == diagonal)
                .count(),
            4,
            "four of the eight are diagonals and cost more"
        );
    }

    #[test]
    fn a_diagonal_needs_both_cells_it_passes_between() {
        let map = overworld();
        let profile = hammerer();
        let placement = cell_center(&map, 6, 5);
        let collision = WorldCollisionGeometryCatalog {
            regions: vec![PlacedCollisionGeometry {
                instance_id: "silhouette".into(),
                position: Position::new(placement.x, placement.y - 1.0),
                component: standing_silhouette(),
            }],
        };
        let graph = derive(&map, &collision);
        let mut steps = Vec::new();

        let blocked = index_at(
            &graph,
            placement,
            map.terrain_cell(6, 5)
                .expect("the world has that cell")
                .elevation_meters,
        );
        assert!(
            graph.nodes()[blocked].is_blocked(),
            "the collider covers the cell east of the one we step from"
        );

        let cell = map.terrain_cell(5, 5).expect("the world has that cell");
        graph.steps_from(
            index_at(&graph, cell.center, cell.elevation_meters),
            &profile,
            &mut steps,
        );

        let reached = steps
            .iter()
            .map(|step| graph.nodes()[step.node].position())
            .collect::<Vec<_>>();
        assert!(
            !reached.contains(&placement),
            "a blocked cell is never a step of its own"
        );
        for corner in [cell_center(&map, 6, 4), cell_center(&map, 6, 6)] {
            assert!(
                !reached.contains(&corner),
                "a diagonal past the blocked cell is refused rather than cutting the corner"
            );
        }
        assert_eq!(reached.len(), 5, "the other five directions remain");
    }

    #[test]
    fn a_step_too_high_is_no_connection() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let mut steps = Vec::new();

        let mut column = Vec::new();
        let (low, high) = map
            .terrain_cells()
            .iter()
            .find_map(|cell| {
                let neighbour = map.terrain_cell(cell.x + 1, cell.y)?;
                map.terrain_walking_surfaces(cell.center, &mut column);
                (column.len() == 1 && neighbour.elevation_meters - cell.elevation_meters > 0.5)
                    .then_some((cell, neighbour))
            })
            .expect("the overworld steps up somewhere too steeply to walk");

        graph.steps_from(
            index_at(&graph, low.center, low.elevation_meters),
            &profile,
            &mut steps,
        );

        assert!(
            !steps
                .iter()
                .any(|step| graph.nodes()[step.node].position() == high.center
                    && graph.nodes()[step.node].elevation_meters() == high.elevation_meters),
            "a rise of more than the Character's step height is not a connection"
        );
    }

    #[test]
    fn an_excavated_column_connects_floor_to_floor_and_ground_to_ground() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let mut steps = Vec::new();
        let mut column = Vec::new();

        let excavated = map
            .terrain_cells()
            .iter()
            .find(|cell| {
                map.terrain_walking_surfaces(cell.center, &mut column);
                column.len() > 1
            })
            .expect("the overworld is excavated somewhere");
        map.terrain_walking_surfaces(excavated.center, &mut column);
        let floor = column[0];
        let ground = column[1];

        graph.steps_from(
            index_at(&graph, excavated.center, floor),
            &profile,
            &mut steps,
        );
        for step in &steps {
            let reached = graph.nodes()[step.node].elevation_meters();
            assert!(
                (reached - floor).abs() <= 0.5,
                "the tunnel connects along its own floor, never up to the ground above"
            );
        }

        graph.steps_from(
            index_at(&graph, excavated.center, ground),
            &profile,
            &mut steps,
        );
        for step in &steps {
            let reached = graph.nodes()[step.node].elevation_meters();
            assert!(
                (reached - ground).abs() <= 0.5,
                "and the ground above connects along itself"
            );
        }
    }

    #[test]
    fn a_world_collider_blocks_the_ground_it_actually_covers() {
        let map = overworld();
        let placement = cell_center(&map, 50, 50);
        let collision = WorldCollisionGeometryCatalog {
            regions: vec![PlacedCollisionGeometry {
                instance_id: "silhouette".into(),
                position: placement,
                component: standing_silhouette(),
            }],
        };
        let graph = derive(&map, &collision);

        for offset in 1..=3 {
            let covered = cell_center(&map, 50, 50 + offset);
            assert!(
                node_at(&graph, covered).is_blocked(),
                "the collider covers the cell {offset} m away from its placement"
            );
        }

        assert!(
            !node_at(&graph, cell_center(&map, 50, 49)).is_blocked(),
            "the collider reaches away from its placement, never behind it"
        );
        assert!(
            !node_at(&graph, placement).is_blocked(),
            "the ground the Prop itself stands on is outside its authored Region"
        );

        for node in graph.nodes().iter().filter(|node| node.is_blocked()) {
            let local = node.position();
            assert!(
                (local.x - placement.x).abs() <= 0.9
                    && (0.1..=3.1).contains(&(local.y - placement.y)),
                "only ground inside the authored Region is blocked"
            );
        }
    }

    #[test]
    fn authored_world_colliders_reach_away_from_the_prop_position() {
        let map = overworld();
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let collision = WorldCollisionGeometryCatalog::from_content_and_map(&content, &map)
            .expect("embedded world collision is valid");
        let graph = derive(&map, &collision);

        assert!(
            !collision.regions.is_empty(),
            "the embedded world places colliding Props"
        );
        assert!(
            graph.nodes().iter().any(GroundNavigationNode::is_blocked),
            "those Props block ground"
        );

        for region in &collision.regions {
            let lowest = region
                .component
                .geometry()
                .vertices
                .iter()
                .map(|vertex| vertex.y)
                .fold(f32::INFINITY, f32::min);
            assert!(
                lowest >= 0.0,
                "'{}' is authored as a standing silhouette starting at its own \
                 position, so the ground it blocks lies beyond it rather than \
                 around it",
                region.instance_id
            );
        }
    }
}
