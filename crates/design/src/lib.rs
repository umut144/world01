use serde::Deserialize;
use std::{collections::HashSet, error::Error, fmt};
use world01_world_data::MassModelDefinition;

const HAMMER_DESIGN: &str = include_str!("../weapons/hammer.json");
const HAMMER_STRIKE_DESIGN: &str = include_str!("../abilities/hammer_strike.json");
const HAMMERER_DESIGN: &str = include_str!("../characters/hammerer.json");
const MAGE_DESIGN: &str = include_str!("../characters/mage.json");
const MAGE_EYE_BEAMS_DESIGN: &str = include_str!("../abilities/mage_eye_beams.json");
const MASS_DESIGN: &str = include_str!("../mass.json");

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GameDesign {
    pub hammer: HammerDesign,
    pub hammer_strike: HammerStrikeDesign,
    pub hammerer: CharacterDesign,
    pub mage: CharacterDesign,
    pub mage_eye_beams: MageEyeBeamsDesign,
    pub mass: MassModelDefinition,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MageEyeBeamsDesign {
    pub schema_version: u32,
    pub name_key: String,
    pub minimum_charge_seconds: f32,
    pub maximum_charge_seconds: f32,
    pub forced_release_seconds: f32,
    pub cooldown_seconds: f32,
    pub projectile_speed_meters_per_second: f32,
    pub gaze_lock_seconds_per_charge_second: f32,
    pub range_meters_per_charge_second: f32,
    pub damage_per_charge_second: f32,
    pub eye_width_ratio: f32,
}

impl MageEyeBeamsDesign {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && self.name_key == "MageEyeBeams"
            && self.minimum_charge_seconds.is_finite()
            && self.minimum_charge_seconds > 0.0
            && self.maximum_charge_seconds.is_finite()
            && self.maximum_charge_seconds >= self.minimum_charge_seconds
            && self.forced_release_seconds.is_finite()
            && self.forced_release_seconds > self.maximum_charge_seconds
            && self.cooldown_seconds.is_finite()
            && self.cooldown_seconds > 0.0
            && self.projectile_speed_meters_per_second.is_finite()
            && self.projectile_speed_meters_per_second > 0.0
            && self.gaze_lock_seconds_per_charge_second.is_finite()
            && self.gaze_lock_seconds_per_charge_second > 0.0
            && self.range_meters_per_charge_second.is_finite()
            && self.range_meters_per_charge_second > 0.0
            && self.damage_per_charge_second.is_finite()
            && self.damage_per_charge_second > 0.0
            && self.eye_width_ratio.is_finite()
            && self.eye_width_ratio > 0.0
            && self.eye_width_ratio < 1.0
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HammerDesign {
    pub schema_version: u32,
    pub asset_key: String,
    pub ability_name_keys: Vec<String>,
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
            && !self.ability_name_keys.is_empty()
            && self
                .ability_name_keys
                .iter()
                .all(|name_key| !name_key.is_empty())
            && unique_names(&self.ability_name_keys)
            && !self.attack_components.is_empty()
            && self.attack_components.iter().all(|name| !name.is_empty())
            && unique_names(&self.attack_components)
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HammerStrikeDesign {
    pub schema_version: u32,
    pub name_key: String,
    pub base_damage: f32,
    pub charge_step_seconds: f32,
    pub charge_damage_percent_per_step: f32,
    pub component_effects: Vec<ComponentEffectsDesign>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComponentEffectsDesign {
    pub component_name: String,
    pub effects: Vec<EffectDesign>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EffectDesign {
    pub name_key: String,
    pub duration_seconds: f32,
}

impl HammerStrikeDesign {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && self.name_key == "HammerStrike"
            && self.base_damage.is_finite()
            && self.base_damage > 0.0
            && self.charge_step_seconds.is_finite()
            && self.charge_step_seconds > 0.0
            && self.charge_damage_percent_per_step.is_finite()
            && self.charge_damage_percent_per_step >= 0.0
            && !self.component_effects.is_empty()
            && self
                .component_effects
                .iter()
                .all(ComponentEffectsDesign::is_valid)
            && unique_names(
                &self
                    .component_effects
                    .iter()
                    .map(|component| component.component_name.clone())
                    .collect::<Vec<_>>(),
            )
    }
}

impl ComponentEffectsDesign {
    fn is_valid(&self) -> bool {
        !self.component_name.is_empty()
            && !self.effects.iter().any(|effect| {
                effect.name_key.is_empty()
                    || !effect.duration_seconds.is_finite()
                    || effect.duration_seconds <= 0.0
            })
            && unique_names(
                &self
                    .effects
                    .iter()
                    .map(|effect| effect.name_key.clone())
                    .collect::<Vec<_>>(),
            )
    }
}

fn unique_names(names: &[String]) -> bool {
    names.iter().collect::<HashSet<_>>().len() == names.len()
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
    let hammer_strike: HammerStrikeDesign = serde_json::from_str(HAMMER_STRIKE_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse HammerStrike design: {error}")))?;
    let hammerer: CharacterDesign = serde_json::from_str(HAMMERER_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse hammerer design: {error}")))?;
    let mage: CharacterDesign = serde_json::from_str(MAGE_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse Mage design: {error}")))?;
    let mage_eye_beams: MageEyeBeamsDesign = serde_json::from_str(MAGE_EYE_BEAMS_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse MageEyeBeams design: {error}")))?;
    let mass: MassModelDefinition = serde_json::from_str(MASS_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse mass design: {error}")))?;
    if !hammer.is_valid() {
        return Err(DesignError("Hammer design is invalid".into()));
    }
    if !hammer_strike.is_valid() {
        return Err(DesignError("HammerStrike design is invalid".into()));
    }
    if hammer.ability_name_keys != [hammer_strike.name_key.clone()] {
        return Err(DesignError(
            "Hammer abilities must reference HammerStrike exactly once".into(),
        ));
    }
    let attack_components = hammer.attack_components.iter().collect::<HashSet<_>>();
    let effect_components = hammer_strike
        .component_effects
        .iter()
        .map(|component| &component.component_name)
        .collect::<HashSet<_>>();
    if attack_components != effect_components {
        return Err(DesignError(
            "HammerStrike component effects must match Hammer attack components".into(),
        ));
    }
    if !hammerer.is_valid("hammerer") {
        return Err(DesignError("Hammerer design is invalid".into()));
    }
    if !mage.is_valid("mage") || !mage_eye_beams.is_valid() {
        return Err(DesignError("Mage eye-beam design is invalid".into()));
    }
    if !mass.is_valid() {
        return Err(DesignError("mass design is invalid".into()));
    }
    Ok(GameDesign {
        hammer,
        hammer_strike,
        hammerer,
        mage,
        mage_eye_beams,
        mass,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_design_contains_the_component_based_hammer_attack() {
        let design = load_embedded().expect("embedded game design parses");
        assert_eq!(design.hammer.asset_key, "hammer");
        assert_eq!(design.hammer.ability_name_keys, ["HammerStrike"]);
        assert_eq!(
            design.hammer.attack_components,
            ["head_mid", "head_left", "head_right"]
        );
        assert_eq!(design.hammer_strike.name_key, "HammerStrike");
        assert_eq!(design.hammer_strike.base_damage, 20.0);
        assert_eq!(design.hammer_strike.charge_step_seconds, 0.5);
        assert_eq!(design.hammer_strike.charge_damage_percent_per_step, 10.0);
        assert_eq!(
            design
                .hammer_strike
                .component_effects
                .iter()
                .find(|component| component.component_name == "head_left")
                .expect("left HammerStrike component is configured")
                .effects[0]
                .name_key,
            "STUNNED"
        );
        assert_eq!(design.hammerer.asset_key, "hammerer");
        assert_eq!(design.mage.asset_key, "mage");
        assert_eq!(design.mage_eye_beams.minimum_charge_seconds, 1.0);
        assert_eq!(design.mage_eye_beams.maximum_charge_seconds, 2.0);
        assert_eq!(design.mage_eye_beams.forced_release_seconds, 4.0);
        assert_eq!(design.mage_eye_beams.cooldown_seconds, 2.0);
        assert_eq!(
            design.mage_eye_beams.projectile_speed_meters_per_second,
            10.0
        );
        assert_eq!(design.mass.characters.len(), 11);
        assert_eq!(design.mass.weapons.len(), 1);
        assert!(
            design
                .mass
                .characters
                .iter()
                .any(|assignment| assignment.asset_key == "hammerer")
        );
    }
}
