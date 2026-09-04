//! Deterministic world navigation and the traversal rules it consumes.

mod ground;
mod traversal;

pub use ground::{apply_grounded_route_speed, constrain_grounded_movement};
pub use traversal::{
    CharacterTraversalProfile, TraversalCatalog, TraversalCatalogError, TraversalSpeed,
};
