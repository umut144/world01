use serde::Deserialize;
use std::{error::Error, fmt};

const HAMMER_DESIGN: &str = include_str!("../weapons/hammer.json");
const HAMMERER_DESIGN: &str = include_str!("../characters/hammerer.json");

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GameDesign {
    pub hammer: HammerDesign,
    pub hammerer: CharacterDesign,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HammerDesign {
    pub schema_version: u32,
    pub asset_key: String,
    pub attack_components: Vec<String>,
    pub maximum_charge_seconds: f32,
    pub grip_reach_seconds: f32,
    pub swing_seconds: f32,
    pub embedded_seconds: f32,
    pub recovery_seconds: f32,
    pub scale_at_full_reach: f32,
    pub scale_at_full_charge: f32,
    pub maximum_inward_pull_ratio: f32,
}

impl HammerDesign {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && self.asset_key == "hammer"
            && !self.attack_components.is_empty()
            && self.attack_components.iter().all(|name| !name.is_empty())
            && self.maximum_charge_seconds.is_finite()
            && self.maximum_charge_seconds > 0.0
            && self.grip_reach_seconds.is_finite()
            && self.grip_reach_seconds > 0.0
            && self.grip_reach_seconds < self.maximum_charge_seconds
            && self.swing_seconds.is_finite()
            && self.swing_seconds > 0.0
            && self.embedded_seconds.is_finite()
            && self.embedded_seconds > 0.0
            && self.recovery_seconds.is_finite()
            && self.recovery_seconds > 0.0
            && self.scale_at_full_reach.is_finite()
            && self.scale_at_full_reach > 0.0
            && self.scale_at_full_reach <= 1.0
            && self.scale_at_full_charge.is_finite()
            && self.scale_at_full_charge > 0.0
            && self.scale_at_full_charge <= self.scale_at_full_reach
            && self.maximum_inward_pull_ratio.is_finite()
            && (0.0..=1.0).contains(&self.maximum_inward_pull_ratio)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CharacterDesign {
    pub schema_version: u32,
    pub asset_key: String,
}

impl CharacterDesign {
    pub fn is_valid(&self, expected_asset_key: &str) -> bool {
        self.schema_version == 1 && self.asset_key == expected_asset_key
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignError(String);

impl fmt::Display for DesignError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for DesignError {}

pub fn load_embedded() -> Result<GameDesign, DesignError> {
    let hammer: HammerDesign = serde_json::from_str(HAMMER_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse hammer design: {error}")))?;
    let hammerer: CharacterDesign = serde_json::from_str(HAMMERER_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse hammerer design: {error}")))?;
    if !hammer.is_valid() {
        return Err(DesignError("Hammer design is invalid".into()));
    }
    if !hammerer.is_valid("hammerer") {
        return Err(DesignError("Hammerer design is invalid".into()));
    }
    Ok(GameDesign { hammer, hammerer })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_design_contains_the_component_based_hammer_attack() {
        let design = load_embedded().expect("embedded game design parses");
        assert_eq!(design.hammer.asset_key, "hammer");
        assert_eq!(
            design.hammer.attack_components,
            ["head_mid", "head_left", "head_right"]
        );
        assert_eq!(design.hammerer.asset_key, "hammerer");
    }
}
