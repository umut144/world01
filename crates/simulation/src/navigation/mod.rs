//! Deterministic world navigation and the traversal rules it consumes.

mod ground;
mod traversal;

pub use ground::{apply_grounded_route_speed, constrain_grounded_movement};
pub(crate) use ground::{recover_invalid_ground_support, resolve_terrain_position};
pub use traversal::{
    CharacterTraversalProfile, TraversalCatalog, TraversalCatalogError, TraversalSpeed,
};
