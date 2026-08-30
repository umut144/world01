//! Shared protocol-neutral domain data.

mod combat;
mod identity;
mod input;
mod life;
mod mass;
mod movement;
mod respawn;
mod status;

pub use combat::*;
pub use identity::*;
pub use input::*;
pub use life::*;
pub use mass::*;
pub use movement::*;
pub use respawn::*;
pub use status::*;
