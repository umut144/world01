//! Shared protocol-neutral domain data.

mod combat;
mod identity;
mod input;
mod life;
mod movement;
mod status;

pub use combat::*;
pub use identity::*;
pub use input::*;
pub use life::*;
pub use movement::*;
pub use status::*;
