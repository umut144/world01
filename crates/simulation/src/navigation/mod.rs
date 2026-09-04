//! Deterministic world navigation and the traversal rules it consumes.

mod ground;
mod traversal;

pub(crate) use ground::resolve_terrain_position;
pub use ground::{
    apply_grounded_route_speed, constrain_grounded_movement, recover_invalid_ground_support,
};
pub use traversal::{
    CharacterTraversalProfile, TraversalCatalog, TraversalCatalogError, TraversalSpeed,
};
