//! MOBA-owned world derivations.
//!
//! Nothing in this module is part of the World-01 baseline: every type here
//! names a MOBA concept - a Totem, a side - so no commit that touches it
//! returns to `main`, per
//! `docs/games/moba/GAME_MOBA_DESIGN.md#what-returns-to-main`. It sits beside
//! the sandbox-generic modules in this crate rather than in a crate of its
//! own because the MOBA has none yet; the boundary this module keeps is
//! about commits, not about files.

mod health;
mod ownership;
mod totem;

pub use health::MobaTotemHealthDesign;
pub use ownership::MobaMapOwnership;
pub use totem::{PlacedTotem, Totem, TotemKind, TotemLayout, TotemLayoutError};
