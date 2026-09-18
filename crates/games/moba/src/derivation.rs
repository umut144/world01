//! How the MOBA derives its own world from the one the sandbox composed.
//!
//! This is the seam described by [`world01_simulation::WorldDerivation`]: the
//! host calls [`MobaWorldDerivation::derive`] to build the first world and
//! calls it again, unchanged, every time a Template or a switch recomposes
//! that world underneath the match. Deriving Totems is therefore a rule the
//! MOBA states once, not a step it runs at startup.

use std::error::Error;

use bevy::prelude::Resource;
use world01_design::DesignError;
use world01_simulation::WorldDerivation;
use world01_world_data::WorldMap;

use crate::{MobaMapOwnership, MobaTotemHealthDesign, TotemLayout};

/// Everything the MOBA's derivation reads besides the map.
///
/// Bundled into one Resource because the seam takes one: a MOBA that later
/// needs a third design file adds a field here, and no signature in the
/// sandbox changes.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MobaWorldSource {
    pub ownership: MobaMapOwnership,
    pub totem_health: MobaTotemHealthDesign,
}

impl MobaWorldSource {
    /// Both design files, refused loudly rather than defaulted.
    pub fn load_embedded() -> Result<Self, DesignError> {
        Ok(Self {
            ownership: MobaMapOwnership::load_embedded()?,
            totem_health: MobaTotemHealthDesign::load_embedded()?,
        })
    }
}

/// The MOBA as the sandbox's world rebuild sees it.
pub struct MobaWorldDerivation;

impl WorldDerivation for MobaWorldDerivation {
    const GAME_KEY: &'static str = "moba";

    type Source = MobaWorldSource;
    type Derived = TotemLayout;

    fn derive(map: &WorldMap, source: &MobaWorldSource) -> Result<TotemLayout, Box<dyn Error>> {
        TotemLayout::from_map(map, &source.ownership, &source.totem_health).map_err(Into::into)
    }
}
