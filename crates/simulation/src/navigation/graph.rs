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

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
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
    water_depth_meters: f32,
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

    /// How deep the standing water over this node is, and zero where there is
    /// none. Whether that is too deep is the Character's own rule.
    pub const fn water_depth_meters(&self) -> f32 {
        self.water_depth_meters
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
#[derive(Resource, Debug, Clone, PartialEq)]
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
    /// For each Route node, the authored grade of the interval to the next
    /// sample of its own Path, and `None` where its Path ends.
    route_intervals: Vec<Option<i32>>,
    /// The Route nodes standing in each Terrain cell.
    cell_route_nodes: HashMap<u32, Vec<usize>>,
    /// The Terrain cell each Route node stands in, where it stands in one.
    route_cells: Vec<Option<u32>>,
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

        let mut route_intervals = Vec::new();
        let mut route_cells = Vec::new();
        let mut cell_route_nodes: HashMap<u32, Vec<usize>> = HashMap::new();

        let mut column = Vec::new();
        for cell in map.terrain_cells() {
            map.terrain_walking_surfaces(cell.center, &mut column);
            // Ground under standing water is still ground, and it becomes a
            // node carrying the depth over it. Whether that depth is walkable
            // is the profile's rule, asked when a Character is at hand.
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
            for standing_on in column.iter() {
                let surface = intern(&mut surfaces, standing_on.surface)?;
                node_cells.push(u32::try_from(index).unwrap_or(u32::MAX));
                nodes.push(GroundNavigationNode {
                    position: cell.center,
                    elevation_meters: standing_on.elevation_meters,
                    surface,
                    water_depth_meters: standing_on.water_depth_meters,
                    support: GroundSupport::Terrain,
                    blocked,
                });
            }
        }

        for route in map.walked_surfaces() {
            let surface = intern(&mut surfaces, &route.surface)?;
            for (index, sample) in route.centerline_samples.iter().enumerate() {
                let ends_here = index + 1 == route.centerline_samples.len();
                let grade = route
                    .segments
                    .iter()
                    .find(|segment| {
                        segment.start_sample_index as usize <= index
                            && index + 1 <= segment.end_sample_index as usize
                    })
                    .map(|segment| segment.grade_percent);
                if !ends_here && grade.is_none() {
                    return Err(GroundNavigationError(format!(
                        "Route '{}' has an interval no authored segment covers",
                        route.route_surface_id
                    )));
                }
                route_intervals.push((!ends_here).then_some(grade).flatten());
                let standing_in = map
                    .terrain_cell_at(sample.position)
                    .and_then(|cell| {
                        cell_index(map.width_tiles(), map.height_tiles(), cell.x, cell.y)
                    })
                    .and_then(|cell| u32::try_from(cell).ok());
                route_cells.push(standing_in);
                if let Some(cell) = standing_in {
                    cell_route_nodes.entry(cell).or_default().push(nodes.len());
                }
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
                    // A Route carries its own surface over whatever is below
                    // it, so nothing stands in water on a deck.
                    water_depth_meters: 0.0,
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
            route_intervals,
            cell_route_nodes,
            route_cells,
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
        match from.support() {
            GroundSupport::Terrain => self.terrain_steps(node, from, profile, steps),
            GroundSupport::RouteSurface { .. } => self.route_steps(node, from, profile, steps),
        }
    }

    fn terrain_steps(
        &self,
        node: usize,
        from: &GroundNavigationNode,
        profile: &CharacterTraversalProfile,
        steps: &mut Vec<GroundNavigationStep>,
    ) {
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
                    speed: self.speed_between(from, &self.nodes[reachable], profile),
                });
            }
        }
        self.enter_a_path(from, profile, x, y, steps);
    }

    /// Adds the Path a Character may step onto from Terrain.
    ///
    /// Exactly one Path may be within reach. Two overlapping ones leave the
    /// Character on Terrain rather than being resolved by nearest height or
    /// first ID, which is the rule movement already follows.
    fn enter_a_path(
        &self,
        from: &GroundNavigationNode,
        profile: &CharacterTraversalProfile,
        x: u32,
        y: u32,
        steps: &mut Vec<GroundNavigationStep>,
    ) {
        let mut reachable = Vec::new();
        let mut entered: Option<&str> = None;
        for (offset_x, offset_y) in around() {
            let Some(cell) = offset_cell(
                self.width_tiles,
                self.height_tiles,
                x,
                y,
                offset_x,
                offset_y,
            ) else {
                continue;
            };
            let Some(candidates) = u32::try_from(cell)
                .ok()
                .and_then(|cell| self.cell_route_nodes.get(&cell))
            else {
                continue;
            };
            for index in candidates.iter().copied() {
                let Some(node) = self.nodes.get(index) else {
                    continue;
                };
                let GroundSupport::RouteSurface { route_surface_id } = node.support() else {
                    continue;
                };
                if !self.is_usable(node, profile)
                    || !profile.permits_step(from.elevation_meters, node.elevation_meters)
                {
                    continue;
                }
                match entered {
                    Some(known) if known != route_surface_id => return,
                    _ => entered = Some(route_surface_id),
                }
                reachable.push(index);
            }
        }
        for index in reachable {
            steps.push(GroundNavigationStep {
                node: index,
                distance_meters: distance_between(from, &self.nodes[index]),
                speed: self.speed_between(from, &self.nodes[index], profile),
            });
        }
    }

    /// A Path carries a Character along its own authored intervals, and lets it
    /// leave for the Terrain around it.
    fn route_steps(
        &self,
        node: usize,
        from: &GroundNavigationNode,
        profile: &CharacterTraversalProfile,
        steps: &mut Vec<GroundNavigationStep>,
    ) {
        let Some(interval) = node.checked_sub(self.node_cells.len()) else {
            return;
        };
        let along = [
            (interval.checked_sub(1), node.checked_sub(1)),
            (Some(interval), Some(node + 1)),
        ];
        for (interval, neighbour) in along {
            let Some(grade) = interval.and_then(|interval| self.route_intervals.get(interval))
            else {
                continue;
            };
            let (Some(grade), Some(neighbour)) = (*grade, neighbour) else {
                continue;
            };
            let Some(next) = self.nodes.get(neighbour) else {
                continue;
            };
            let Some(speed) = profile.speed_for_grade(grade) else {
                continue;
            };
            if self.is_usable(next, profile) {
                steps.push(GroundNavigationStep {
                    node: neighbour,
                    distance_meters: distance_between(from, next),
                    speed,
                });
            }
        }

        let Some(cell) = self.route_cells.get(interval).copied().flatten() else {
            return;
        };
        let Some((x, y)) = self.cell_position(cell) else {
            return;
        };
        for (offset_x, offset_y) in around() {
            for index in self.reachable_in(from, profile, x, y, offset_x, offset_y) {
                steps.push(GroundNavigationStep {
                    node: index,
                    distance_meters: distance_between(from, &self.nodes[index]),
                    speed: self.speed_between(from, &self.nodes[index], profile),
                });
            }
        }
    }

    /// The cheapest way from one node to another for a Character, or `None`
    /// when its own rules leave no way at all.
    ///
    /// Cost is time rather than distance: a stretch a Character crosses at half
    /// speed costs twice its length, so a longer level way can beat a shorter
    /// steep one. Equal costs resolve by node order, so the same question asked
    /// twice answers the same way.
    pub fn path(
        &self,
        from: usize,
        to: usize,
        profile: &CharacterTraversalProfile,
    ) -> Option<Vec<usize>> {
        let start = self.nodes.get(from)?;
        let goal = self.nodes.get(to)?;
        if !self.is_usable(start, profile) || !self.is_usable(goal, profile) {
            return None;
        }
        if from == to {
            return Some(vec![from]);
        }

        let mut costs = vec![f32::INFINITY; self.nodes.len()];
        let mut came_from: Vec<Option<usize>> = vec![None; self.nodes.len()];
        let mut frontier = BinaryHeap::new();
        let mut steps = Vec::new();

        costs[from] = 0.0;
        frontier.push(Frontier {
            estimate: distance_between(start, goal),
            cost: 0.0,
            node: from,
        });

        while let Some(current) = frontier.pop() {
            if current.node == to {
                return retrace(&came_from, from, to);
            }
            if current.cost > costs[current.node] {
                continue;
            }
            self.steps_from(current.node, profile, &mut steps);
            for step in &steps {
                let cost = current.cost + step.distance_meters / step.speed.multiplier();
                if cost >= costs[step.node] {
                    continue;
                }
                costs[step.node] = cost;
                came_from[step.node] = Some(current.node);
                frontier.push(Frontier {
                    estimate: cost + distance_between(&self.nodes[step.node], goal),
                    cost,
                    node: step.node,
                });
            }
        }
        None
    }

    fn is_usable(&self, node: &GroundNavigationNode, profile: &CharacterTraversalProfile) -> bool {
        !node.blocked
            && profile.permits_wade(node.water_depth_meters)
            && self
                .surface_name(node.surface())
                .is_some_and(|surface| profile.permits_surface(surface))
    }

    /// What a move between two nodes costs a Character in speed.
    ///
    /// A move that begins or ends in water is a wading move, priced by the
    /// deeper of its two ends: leaving the water costs what entering it costs,
    /// so there is no free step back onto the bank.
    fn speed_between(
        &self,
        from: &GroundNavigationNode,
        to: &GroundNavigationNode,
        profile: &CharacterTraversalProfile,
    ) -> TraversalSpeed {
        profile.speed_through_water(from.water_depth_meters.max(to.water_depth_meters))
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

/// One node waiting to be expanded, ordered so the cheapest estimate leaves the
/// heap first and an equal estimate resolves by node order.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frontier {
    estimate: f32,
    cost: f32,
    node: usize,
}

impl Eq for Frontier {}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn retrace(came_from: &[Option<usize>], from: usize, to: usize) -> Option<Vec<usize>> {
    let mut path = vec![to];
    let mut node = to;
    while node != from {
        node = (*came_from.get(node)?)?;
        path.push(node);
    }
    path.reverse();
    Some(path)
}

/// A cell and the eight around it.
fn around() -> impl Iterator<Item = (i64, i64)> {
    std::iter::once((0, 0)).chain(TERRAIN_DIRECTIONS)
}

fn distance_between(from: &GroundNavigationNode, to: &GroundNavigationNode) -> f32 {
    let x = to.position.x - from.position.x;
    let y = to.position.y - from.position.y;
    x.hypot(y)
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
    use world01_design::{
        CharacterTraversalDesign, TraversalDesign, load_embedded as load_game_design,
    };
    use world01_world_data::CharacterId;

    use crate::TraversalCatalog;

    use super::*;
    use world01_world_data::{MapColumnSurface, RouteSegmentOperation};

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

    /// The surfaces a column offers, which is what the graph turns into nodes.
    /// Ground under water is one of them; whether a Character may be there is
    /// asked of the profile, not of the column.
    fn standing_places(column: &[MapColumnSurface<'_>]) -> usize {
        column.len()
    }

    fn node_at(graph: &GroundNavigationGraph, position: Position) -> &GroundNavigationNode {
        graph
            .nodes()
            .iter()
            .find(|node| node.position() == position)
            .expect("every authored cell has a node")
    }

    /// A standing silhouette: as wide as it is tall, and entirely on one side
    /// of the position it is placed at.
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

    /// A silhouette drawn around the position it is placed at, the way an Asset
    /// centred on its own origin exports.
    fn centred_silhouette() -> CollisionComponentGeometry {
        CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: "boulder".into(),
            name: "boulder".into(),
            vertices: vec![
                Vec2::new(-0.9, -1.1),
                Vec2::new(0.9, -1.1),
                Vec2::new(0.9, 1.1),
                Vec2::new(-0.9, 1.1),
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
            .walked_surfaces()
            .map(|route| route.centerline_samples.len())
            .sum::<usize>();
        let mut column = Vec::new();
        let walking_surfaces = map
            .terrain_cells()
            .iter()
            .map(|cell| {
                map.terrain_walking_surfaces(cell.center, &mut column);
                standing_places(&column)
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
                standing_places(&column) > 1
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
                column
                    .iter()
                    .map(|surface| surface.elevation_meters)
                    .collect::<Vec<_>>(),
                "a cell offers exactly the places its column resolves to"
            );
            for (node, standing_on) in standing.iter().zip(column.iter()) {
                assert_eq!(
                    graph.surface_name(node.surface()),
                    Some(standing_on.surface)
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

        // Dry, because a river bed also stands below the ground around it and
        // is not what this test is about.
        let deepest = graph
            .nodes()
            .iter()
            .filter(|node| matches!(node.support(), GroundSupport::Terrain))
            .filter(|node| node.water_depth_meters() == 0.0)
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

        assert_eq!(counted.len(), map.walked_surfaces().count());
        for route in map.walked_surfaces() {
            assert_eq!(
                counted.get(route.route_surface_id.as_str()).copied(),
                Some(route.centerline_samples.len()),
                "Route '{}' contributes one node per centerline sample",
                route.route_surface_id
            );
        }
    }

    #[test]
    fn a_character_steps_from_the_ground_onto_an_authored_deck_and_back() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let mut steps = Vec::new();

        let bridge = map
            .bridges()
            .first()
            .expect("the overworld authors a bridge");
        let end = bridge
            .centerline_samples
            .first()
            .expect("a deck runs from one end to the other");
        let deck = index_at(&graph, end.position, end.elevation_meters);
        let ground = map
            .terrain_cell_at(end.position)
            .expect("a bridge end lies over authored Terrain");
        let standing = index_at(&graph, ground.center, ground.elevation_meters);

        graph.steps_from(standing, &profile, &mut steps);
        assert!(
            steps.iter().any(|step| step.node == deck),
            "a Character standing at the end of a bridge may step onto its deck"
        );

        graph.steps_from(deck, &profile, &mut steps);
        assert!(
            steps.iter().any(|step| step.node == standing),
            "and may step back off it"
        );
    }

    #[test]
    fn a_deck_is_carried_as_the_path_it_was_baked_from() {
        let map = overworld();

        assert_eq!(map.bridge_decks().len(), map.bridges().len());
        assert_eq!(
            map.walked_surfaces().count(),
            map.route_surfaces().len() + map.bridges().len()
        );
        for (bridge, deck) in map.bridges().iter().zip(map.bridge_decks()) {
            assert_eq!(deck.route_surface_id, bridge.bridge_id);
            assert_eq!(deck.surface, bridge.surface);
            assert_eq!(deck.centerline_samples, bridge.centerline_samples);
            assert_eq!(deck.segments.len(), 1, "a bridge has exactly one interval");
            assert_eq!(deck.segments[0].grade_percent, 0);
            assert_eq!(
                deck.segments[0].operation,
                RouteSegmentOperation::Additive,
                "a bridge takes nothing out of the Terrain it spans"
            );
            assert!(
                map.route_surface(&bridge.bridge_id).is_some(),
                "a deck answers to its own ID like any other walked surface"
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

    /// A Character built for one question, so a rule can be tested apart from
    /// whatever the authored Characters happen to allow.
    fn profile_of(
        surfaces: &[&str],
        max_step_height_meters: f32,
        max_wade_depth_meters: f32,
    ) -> CharacterTraversalProfile {
        TraversalCatalog::from_design(&TraversalDesign {
            schema_version: 1,
            characters: vec![CharacterTraversalDesign {
                asset_key: "test_character".into(),
                surfaces: surfaces.iter().map(|surface| (*surface).into()).collect(),
                max_step_height_meters,
                max_wade_depth_meters,
                normal_speed_max_abs_grade_percent: 25,
                passable_max_abs_grade_percent: 50,
                reduced_speed_multiplier: 0.5,
            }],
        })
        .expect("a single explicit profile is valid")
        .character(&CharacterId("test_character".into()))
        .expect("that profile was just defined")
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
                (standing_places(&column) == 1
                    && neighbour.elevation_meters - cell.elevation_meters > 0.5)
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
                standing_places(&column) > 1
            })
            .expect("the overworld is excavated somewhere");
        map.terrain_walking_surfaces(excavated.center, &mut column);
        let floor = column[0].elevation_meters;
        let ground = column[1].elevation_meters;

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
    fn a_path_carries_its_authored_grade_into_the_step() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let mut steps = Vec::new();

        let route = map
            .walked_surfaces()
            .find(|route| {
                route
                    .segments
                    .iter()
                    .any(|segment| segment.grade_percent.abs() == 50)
            })
            .expect("the overworld authors a half-speed Path");
        let segment = route
            .segments
            .iter()
            .find(|segment| segment.grade_percent.abs() == 50)
            .expect("that Path has the steep segment");
        let sample = &route.centerline_samples[segment.start_sample_index as usize];
        let elevation = route
            .sample_at(sample.position)
            .expect("a Path supports its own centerline")
            .elevation_meters;

        graph.steps_from(
            index_at(&graph, sample.position, elevation),
            &profile,
            &mut steps,
        );

        assert!(
            steps
                .iter()
                .any(|step| step.speed == TraversalSpeed::Reduced(0.5)),
            "a steep authored interval costs the Character its speed"
        );
    }

    #[test]
    fn terrain_and_a_path_reach_each_other() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let mut steps = Vec::new();

        let (terrain, route) = graph
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, node)| matches!(node.support(), GroundSupport::RouteSurface { .. }))
            .find_map(|(route_index, route_node)| {
                let cell = map.terrain_cell_at(route_node.position())?;
                let terrain_index = graph.nodes().iter().position(|node| {
                    node.position() == cell.center
                        && matches!(node.support(), GroundSupport::Terrain)
                        && (node.elevation_meters() - route_node.elevation_meters()).abs() <= 0.5
                })?;
                Some((terrain_index, route_index))
            })
            .expect("a Path starts within a step of the Terrain under it");

        graph.steps_from(terrain, &profile, &mut steps);
        assert!(
            steps.iter().any(|step| step.node == route),
            "Terrain enters the Path it is standing under"
        );

        graph.steps_from(route, &profile, &mut steps);
        assert!(
            steps.iter().any(|step| step.node == terrain),
            "and the Path lets the Character leave for that Terrain again"
        );
    }

    #[test]
    fn a_path_does_not_continue_past_its_own_end() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let mut steps = Vec::new();

        for route in map.walked_surfaces() {
            let last = route.centerline_samples.last().expect("a Path has samples");
            let elevation = route
                .sample_at(last.position)
                .expect("a Path supports its own centerline")
                .elevation_meters;
            graph.steps_from(
                index_at(&graph, last.position, elevation),
                &profile,
                &mut steps,
            );

            for step in &steps {
                if let GroundSupport::RouteSurface { route_surface_id } =
                    graph.nodes()[step.node].support()
                {
                    assert_eq!(
                        route_surface_id, &route.route_surface_id,
                        "the end of a Path never continues into another one"
                    );
                }
            }
        }
    }

    /// The nodes standing below ground that reaches above them: the excavation.
    /// The places an excavation left below the ground still standing above
    /// them - dry ones, because a river bed lies below its banks too and is not
    /// a tunnel.
    fn tunnel_nodes(map: &WorldMap, graph: &GroundNavigationGraph) -> Vec<usize> {
        graph
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, node)| matches!(node.support(), GroundSupport::Terrain))
            .filter(|(_, node)| node.water_depth_meters() == 0.0)
            .filter(|(_, node)| {
                map.terrain_cell_at(node.position())
                    .is_some_and(|cell| cell.elevation_meters > node.elevation_meters())
            })
            .map(|(index, _)| index)
            .collect()
    }

    #[test]
    fn a_path_leads_through_the_hill_rather_than_over_it() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();

        let tunnel = tunnel_nodes(&map, &graph);
        let west = *tunnel
            .iter()
            .min_by(|first, second| {
                graph.nodes()[**first]
                    .position()
                    .x
                    .total_cmp(&graph.nodes()[**second].position().x)
            })
            .expect("the excavation has a western end");
        let east = *tunnel
            .iter()
            .max_by(|first, second| {
                graph.nodes()[**first]
                    .position()
                    .x
                    .total_cmp(&graph.nodes()[**second].position().x)
            })
            .expect("the excavation has an eastern end");

        let path = graph
            .path(west, east, &profile)
            .expect("the excavation connects its own ends");

        assert_eq!(path.first().copied(), Some(west));
        assert_eq!(path.last().copied(), Some(east));

        let ceiling = map
            .route_surface_cuts()
            .first()
            .and_then(|cut| cut.cells.first())
            .expect("the excavation has cells")
            .cut_top_meters;
        for node in &path {
            assert!(
                graph.nodes()[*node].elevation_meters() <= ceiling,
                "the way through never climbs the ground standing above it"
            );
        }

        let mut steps = Vec::new();
        for pair in path.windows(2) {
            graph.steps_from(pair[0], &profile, &mut steps);
            assert!(
                steps.iter().any(|step| step.node == pair[1]),
                "every move on the way is one the Character may actually make"
            );
        }
    }

    #[test]
    fn a_character_that_crosses_no_surface_of_this_world_finds_no_way() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let stranger = profile_of(&["water"], 0.5, 0.4);

        let tunnel = tunnel_nodes(&map, &graph);
        let (west, east) = (tunnel[0], tunnel[tunnel.len() - 1]);

        assert!(graph.path(west, east, &profile).is_some());
        assert!(
            graph.path(west, east, &stranger).is_none(),
            "a Character that crosses none of this world's surfaces goes nowhere"
        );
    }

    #[test]
    fn ground_under_water_is_a_node_carrying_the_depth_over_it() {
        let map = overworld();
        let graph = without_collision(&map);

        let wet = graph
            .nodes()
            .iter()
            .filter(|node| matches!(node.support(), GroundSupport::Terrain))
            .filter(|node| node.water_depth_meters() > 0.0)
            .collect::<Vec<_>>();

        assert!(!wet.is_empty(), "the overworld carries a river");
        for node in &wet {
            let mut column = Vec::new();
            map.terrain_walking_surfaces(node.position(), &mut column);
            let bed = column
                .iter()
                .find(|surface| surface.elevation_meters == node.elevation_meters())
                .expect("the node stands where its own column resolves");
            assert_eq!(node.water_depth_meters(), bed.water_depth_meters);
            assert_ne!(
                graph.surface_name(node.surface()),
                Some("water"),
                "a node under water stands on the bed, not on the water"
            );
        }
    }

    #[test]
    fn water_is_crossed_by_whoever_may_wade_that_deep_and_costs_them_speed() {
        let map = overworld();
        let graph = without_collision(&map);
        let mut steps = Vec::new();

        let (standing_in_water, depth) = graph
            .nodes()
            .iter()
            .enumerate()
            .find_map(|(index, node)| {
                (matches!(node.support(), GroundSupport::Terrain)
                    && node.water_depth_meters() > 0.0)
                    .then(|| (index, node.water_depth_meters()))
            })
            .expect("the overworld has a river to stand in");
        let bed = graph
            .surface_name(graph.nodes()[standing_in_water].surface())
            .expect("that node names its surface")
            .to_owned();
        // A step height well over the bank, so only the depth decides.
        let step_height = depth * 2.0;

        graph.steps_from(
            standing_in_water,
            &profile_of(&[&bed], step_height, depth / 2.0),
            &mut steps,
        );
        assert!(
            steps.is_empty(),
            "a Character that may not wade this deep has no move to make in the water"
        );

        graph.steps_from(
            standing_in_water,
            &profile_of(&[&bed], step_height, depth),
            &mut steps,
        );
        assert!(
            !steps.is_empty(),
            "one whose own rule reaches the bed does have moves to make"
        );
        assert!(
            steps
                .iter()
                .all(|step| step.speed == TraversalSpeed::Reduced(0.5)),
            "and every one of them is a wading move, priced below walking"
        );
    }

    #[test]
    fn a_way_to_where_one_already_stands_is_that_place() {
        let map = overworld();
        let graph = without_collision(&map);
        let profile = hammerer();
        let cell = map.terrain_cell(5, 5).expect("the world has that cell");
        let node = index_at(&graph, cell.center, cell.elevation_meters);

        assert_eq!(graph.path(node, node, &profile), Some(vec![node]));
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
            "this collider lies beyond its placement, so the ground behind it stays open"
        );
        assert!(
            !node_at(&graph, placement).is_blocked(),
            "this collider leaves the ground at its own placement open"
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
    fn a_world_collider_may_be_drawn_around_its_own_placement() {
        let map = overworld();
        let placement = cell_center(&map, 50, 50);
        let collision = WorldCollisionGeometryCatalog {
            regions: vec![PlacedCollisionGeometry {
                instance_id: "boulder".into(),
                position: placement,
                component: centred_silhouette(),
            }],
        };
        let graph = derive(&map, &collision);

        assert!(
            node_at(&graph, placement).is_blocked(),
            "a Region that covers its own placement blocks that ground too"
        );
        for neighbour in [49, 51] {
            assert!(
                node_at(&graph, cell_center(&map, 50, neighbour)).is_blocked(),
                "the Region covers the ground on both sides of its placement"
            );
        }
        assert!(
            !node_at(&graph, cell_center(&map, 50, 52)).is_blocked(),
            "ground the Region does not cover stays open"
        );
    }

    #[test]
    fn the_embedded_world_blocks_ground_where_it_places_colliding_props() {
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
    }
}
