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

use crate::spatial::broadphase::{Aabb, WorldColliderGrid};
use crate::spatial::overlap::{GeometryTransform, point_in_component};

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
            for elevation_meters in column.iter().copied() {
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

        Ok(Self { surfaces, nodes })
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
