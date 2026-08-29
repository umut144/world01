//! Shared protocol-neutral domain data.

mod combat;
mod identity;
mod input;
mod movement;
mod status;

pub use combat::*;
pub use identity::*;
pub use input::*;
pub use movement::*;
pub use status::*;
