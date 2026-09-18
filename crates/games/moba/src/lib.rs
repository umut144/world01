//! The MOBA: its design data, and the world derivations that data feeds.
//!
//! Nothing here is part of the World-01 baseline. Every type names a MOBA
//! concept - a Totem, a side - and the boundary that keeps it out of the
//! baseline is now the dependency graph rather than commit discipline: this
//! crate depends on `world_data` and `design`, and nothing shared depends on
//! it. A sandbox crate that reached for a Totem would have to name
//! `world01-moba` in its `Cargo.toml`, which Cargo refuses as a cycle. That
//! is the whole point of the crate existing: the rule is checked by the
//! compiler rather than remembered by whoever writes the next commit.
//!
//! **For whoever tunes the numbers**: the JSON under `data/` holds every
//! actual value - a Totem's MaxHP, which side owns which placed Prop. The
//! Rust here is schema. Changing a number, or adding another instance of a
//! shape the schema already knows, is a `data/*.json` edit and nothing in
//! `src/` needs to change.

mod derivation;
mod design;
mod health;
mod ownership;
mod totem;

pub use derivation::{MobaWorldDerivation, MobaWorldSource};
pub use design::{
    MobaMapCatalog, MobaMapDesign, MobaPlacementRank, MobaPlacementRanksDesign, MobaPropOwner,
    MobaTotemDesign, MobaTotemHealth,
};
pub use health::MobaTotemHealthDesign;
pub use ownership::MobaMapOwnership;
pub use totem::{PlacedTotem, Totem, TotemKind, TotemLayout, TotemLayoutError};
