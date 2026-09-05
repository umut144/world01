//! Deterministic world navigation and the traversal rules it consumes.

mod graph;
mod ground;
mod support;
mod traversal;

pub use graph::{
    GroundNavigationError, GroundNavigationGraph, GroundNavigationNode, NavigationSurface,
};
pub(crate) use ground::recover_invalid_ground_support;
pub use ground::{apply_grounded_route_speed, constrain_grounded_movement};
pub(crate) use support::resolve_terrain_position;
pub use traversal::{
    CharacterTraversalProfile, TraversalCatalog, TraversalCatalogError, TraversalSpeed,
};
