//! The derived ground navigation representation.
//!
//! A node is a place a grounded Actor can stand: a horizontal position, the
//! physical height it stands at, and the identity of the support whose height
//! that is. Terrain contributes one node per authored cell, and each Route
//! surface contributes one node per baked centerline sample.
//!
//! Nodes deliberately carry no judgement about who may use them. Which
//! surfaces a Character may cross is its own traversal profile's answer, given
//! when a path is asked for, so one derived world serves every Character.

use std::{error::Error, fmt};

use world01_world_data::{GroundSupport, Position, WorldMap};

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
    /// Terrain heights come from the authored cell. Route heights come from the
    /// same projection onto the baked centerline that carries a walking Actor,
    /// rather than from the sample's stored height, so a node cannot describe a
    /// height the Route would not actually give.
    pub fn from_map(map: &WorldMap) -> Result<Self, GroundNavigationError> {
        let mut surfaces = Vec::new();
        let mut nodes = Vec::with_capacity(map.terrain_cells().len());

        for cell in map.terrain_cells() {
            let surface = intern(&mut surfaces, &cell.surface)?;
            nodes.push(GroundNavigationNode {
                position: cell.center,
                elevation_meters: cell.elevation_meters,
                surface,
                support: GroundSupport::Terrain,
            });
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

    use super::*;

    fn overworld() -> WorldMap {
        WorldMap::load_embedded("overworld01").expect("embedded Instance is valid")
    }

    #[test]
    fn every_terrain_cell_and_centerline_sample_becomes_a_node() {
        let map = overworld();
        let graph = GroundNavigationGraph::from_map(&map).expect("embedded world derives nodes");

        let samples = map
            .route_surfaces()
            .iter()
            .map(|route| route.centerline_samples.len())
            .sum::<usize>();

        assert_eq!(graph.nodes().len(), map.terrain_cells().len() + samples);
        assert_eq!(
            graph
                .nodes()
                .iter()
                .filter(|node| matches!(node.support(), GroundSupport::Terrain))
                .count(),
            map.terrain_cells().len()
        );
    }

    #[test]
    fn a_terrain_node_stands_at_its_authored_cell_height() {
        let map = overworld();
        let graph = GroundNavigationGraph::from_map(&map).expect("embedded world derives nodes");

        for cell in [
            map.terrain_cells().first().expect("the world has Terrain"),
            map.terrain_cells()
                .iter()
                .find(|cell| cell.elevation_meters > 1.0)
                .expect("the world has raised Terrain"),
        ] {
            let node = graph
                .nodes()
                .iter()
                .find(|node| node.position() == cell.center)
                .expect("every authored cell has a node");

            assert_eq!(node.elevation_meters(), cell.elevation_meters);
            assert_eq!(
                graph.surface_name(node.surface()),
                Some(cell.surface.as_str())
            );
            assert_eq!(node.support(), &GroundSupport::Terrain);
        }
    }

    #[test]
    fn route_nodes_carry_their_authored_support_identity() {
        let map = overworld();
        let graph = GroundNavigationGraph::from_map(&map).expect("embedded world derives nodes");

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
        let graph = GroundNavigationGraph::from_map(&map).expect("embedded world derives nodes");

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
}
