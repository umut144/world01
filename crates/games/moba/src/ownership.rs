//! A Bevy resource around the MOBA's embedded map-ownership data.
//!
//! [`crate::design`] stays free of Bevy so the parsing can be read and tested
//! as plain data - the same reason no other design type there is a `Resource`
//! either. This wrapper is where the data becomes something a system reads.

use crate::design::{MobaMapCatalog, MobaMapDesign};
use bevy::prelude::Resource;
use world01_design::DesignError;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MobaMapOwnership(MobaMapCatalog);

impl MobaMapOwnership {
    pub fn load_embedded() -> Result<Self, DesignError> {
        Ok(Self(MobaMapCatalog::load_embedded()?))
    }

    /// Builds a catalog from map-ownership sources that are not the embedded
    /// one, for a test to check `TotemLayout::from_map` against a scene
    /// `load_embedded` never carries.
    #[cfg(test)]
    pub(crate) fn from_sources<'a>(
        sources: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, DesignError> {
        Ok(Self(MobaMapCatalog::parse(sources)?))
    }

    /// Who owns what in one authored map, or `None` when the MOBA has no
    /// ownership file for that scene at all.
    pub fn map(&self, scene_id: &str) -> Option<&MobaMapDesign> {
        self.0.map(scene_id)
    }
}
