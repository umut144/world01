//! A Bevy resource around the MOBA's embedded map-ownership data.
//!
//! `world01_design::moba` stays free of Bevy so the sandbox can depend on
//! `world01-design` without depending on the ECS at all - the same reason no
//! other design type in that crate is a `Resource` either. This wrapper is
//! where the data becomes something a system can read.

use bevy::prelude::Resource;
use world01_design::DesignError;
use world01_design::moba::{MobaMapCatalog, MobaMapDesign};

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
