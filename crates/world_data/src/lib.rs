//! Shared protocol-neutral domain data.

mod combat;
mod composition;
mod health_geometry;
mod identity;
mod input;
mod life;
mod map;
mod mass;
mod movement;
mod respawn;
mod status;

pub use combat::*;
pub use composition::*;
pub use health_geometry::*;
pub use identity::*;
pub use input::*;
pub use life::*;
pub use map::*;
pub use mass::*;
pub use movement::*;
pub use respawn::*;
pub use status::*;
