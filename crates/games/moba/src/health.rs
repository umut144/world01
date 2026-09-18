//! A Bevy resource around the MOBA's embedded Totem-health design data.
//!
//! [`crate::design`] stays free of Bevy so the parsing can be read and tested
//! as plain data - the same reason [`super::MobaMapOwnership`] wraps its
//! design type instead of deriving `Resource` on it directly.

use crate::design::MobaTotemDesign;
use bevy::prelude::Resource;
use world01_design::DesignError;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MobaTotemHealthDesign(MobaTotemDesign);

impl MobaTotemHealthDesign {
    pub fn load_embedded() -> Result<Self, DesignError> {
        Ok(Self(MobaTotemDesign::load_embedded()?))
    }

    /// Builds a design from Totem-health JSON that is not the embedded one,
    /// for a test to check `TotemLayout::from_map` against health data
    /// `load_embedded` never carries.
    #[cfg(test)]
    pub(crate) fn from_sources(source: &str) -> Result<Self, DesignError> {
        Ok(Self(MobaTotemDesign::parse(source)?))
    }

    /// A Totem kind's maximum health, or `None` when the design says nothing
    /// about the Asset key.
    pub fn max_hp(&self, asset_key: &str) -> Option<f32> {
        self.0.max_hp(asset_key)
    }
}
