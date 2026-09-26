use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    path::Path,
};
use world01_world_data::{
    CharacterId, DensityClass, HealthGeometryDefinition, MassModelDefinition, PlacementRanks,
};

const HAMMER_DESIGN: &str = include_str!("../weapons/hammer.json");
const HAMMER_STRIKE_DESIGN: &str = include_str!("../abilities/hammer_strike.json");
const HAMMERER_DESIGN: &str = include_str!("../characters/hammerer.json");
const MAGE_DESIGN: &str = include_str!("../characters/mage.json");
const MAGE_EYE_BEAMS_DESIGN: &str = include_str!("../abilities/mage_eye_beams.json");
const MASS_DESIGN: &str = include_str!("../mass.json");
const HP_DESIGN: &str = include_str!("../hp.json");
const TRAVERSAL_DESIGN: &str = include_str!("../traversal.json");
const WORLD01_TOML: &str = include_str!("../world01.toml");

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct World01Design {
    pub mass: MassConfig,
    pub locomotion: LocomotionConfig,
    pub health: HealthConfig,
    pub weapon_aim: WeaponAimConfig,
    pub eyes: EyesConfig,
    pub placement_ranks: Vec<PlacementRankDesign>,
}

impl World01Design {
    pub fn placement_ranks(&self) -> Result<PlacementRanks, DesignError> {
        PlacementRanks::from_entries(
            self.placement_ranks
                .iter()
                .map(|entry| (entry.asset_key.as_str(), entry.rank)),
        )
        .map_err(|error| DesignError(format!("invalid placement ranks: {error}")))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PlacementRankDesign {
    pub asset_key: String,
    pub rank: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WeaponAimConfig {
    pub default_degrees_per_second: f32,
    #[serde(default)]
    pub character_degrees_per_second: HashMap<String, f32>,
}

impl WeaponAimConfig {
    pub fn is_valid(&self) -> bool {
        self.default_degrees_per_second.is_finite()
            && self.default_degrees_per_second > 0.0
            && self
                .character_degrees_per_second
                .iter()
                .all(|(character, speed)| {
                    !character.is_empty() && speed.is_finite() && *speed > 0.0
                })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct MassConfig {
    pub weightless_density_factor: f32,
    pub very_light_density_factor: f32,
    pub light_density_factor: f32,
    pub medium_density_factor: f32,
    pub heavy_density_factor: f32,
    pub very_heavy_density_factor: f32,
    pub hammerer_speed_meters_per_second: f32,
    pub speed_mass_exponent: f32,
    pub include_equipped_weapon_mass: bool,
}

impl MassConfig {
    pub fn density_factor(self, class: DensityClass) -> f32 {
        match class {
            DensityClass::Weightless => self.weightless_density_factor,
            DensityClass::VeryLight => self.very_light_density_factor,
            DensityClass::Light => self.light_density_factor,
            DensityClass::Medium => self.medium_density_factor,
            DensityClass::Heavy => self.heavy_density_factor,
            DensityClass::VeryHeavy => self.very_heavy_density_factor,
        }
    }

    pub fn is_valid(self) -> bool {
        [
            self.weightless_density_factor,
            self.very_light_density_factor,
            self.light_density_factor,
            self.medium_density_factor,
            self.heavy_density_factor,
            self.very_heavy_density_factor,
        ]
        .into_iter()
        .all(|factor| factor.is_finite() && factor >= 0.0)
            && self.medium_density_factor > 0.0
            && self.hammerer_speed_meters_per_second.is_finite()
            && self.hammerer_speed_meters_per_second > 0.0
            && self.speed_mass_exponent.is_finite()
            && self.speed_mass_exponent > 0.0
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct LocomotionConfig {
    pub default_max_stamina: f32,
    pub stamina_regeneration_percent_per_second: f32,
    pub run_speed_multiplier: f32,
    pub run_drain_per_second: f32,
    pub dash_cost_percent: f32,
    pub dash_speed_multiplier: f32,
    pub dash_duration_seconds: f32,
    pub dash_invulnerability_seconds: f32,
    pub knockdown_duration_seconds: f32,
    pub knockdown_damage_percent_max_hp: f32,
}

impl LocomotionConfig {
    pub fn is_valid(self) -> bool {
        self.default_max_stamina.is_finite()
            && self.default_max_stamina > 0.0
            && self.stamina_regeneration_percent_per_second.is_finite()
            && self.stamina_regeneration_percent_per_second >= 0.0
            && self.run_speed_multiplier.is_finite()
            && self.run_speed_multiplier >= 1.0
            && self.run_drain_per_second.is_finite()
            && self.run_drain_per_second >= 0.0
            && self.dash_cost_percent.is_finite()
            && (0.0..=100.0).contains(&self.dash_cost_percent)
            && self.dash_speed_multiplier.is_finite()
            && self.dash_speed_multiplier > 0.0
            && self.dash_duration_seconds.is_finite()
            && self.dash_duration_seconds > 0.0
            && self.dash_invulnerability_seconds.is_finite()
            && (0.0..=self.dash_duration_seconds).contains(&self.dash_invulnerability_seconds)
            && self.knockdown_duration_seconds.is_finite()
            && self.knockdown_duration_seconds > 0.0
            && self.knockdown_damage_percent_max_hp.is_finite()
            && (0.0..=100.0).contains(&self.knockdown_damage_percent_max_hp)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct HealthConfig {
    pub death_confirmation_seconds: f32,
    pub death_confirmation_initial_degrees_per_second: f32,
    pub death_confirmation_max_degrees_per_second: f32,
    pub revival_seconds: f32,
    pub revival_health_percent: f32,
    pub respawn_health_percent: f32,
    pub ankh_respawn_radius_meters: f32,
}

impl HealthConfig {
    pub fn is_valid(self) -> bool {
        self.death_confirmation_seconds.is_finite()
            && self.death_confirmation_seconds > 0.0
            && self
                .death_confirmation_initial_degrees_per_second
                .is_finite()
            && self.death_confirmation_initial_degrees_per_second >= 0.0
            && self.death_confirmation_max_degrees_per_second.is_finite()
            && self.death_confirmation_max_degrees_per_second
                >= self.death_confirmation_initial_degrees_per_second
            && self.revival_seconds.is_finite()
            && self.revival_seconds > 0.0
            && self.revival_health_percent.is_finite()
            && (0.0..=100.0).contains(&self.revival_health_percent)
            && self.respawn_health_percent.is_finite()
            && (0.0..=100.0).contains(&self.respawn_health_percent)
            && self.ankh_respawn_radius_meters.is_finite()
            && self.ankh_respawn_radius_meters >= 0.0
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct EyesConfig {
    pub pupil_area_ratio: f32,
    pub hammerer_collision_radius_ratio: f32,
}

impl EyesConfig {
    pub fn is_valid(self) -> bool {
        self.pupil_area_ratio.is_finite()
            && self.pupil_area_ratio > 0.0
            && self.pupil_area_ratio < 1.0
            && self.hammerer_collision_radius_ratio.is_finite()
            && self.hammerer_collision_radius_ratio > 0.0
            && self.hammerer_collision_radius_ratio <= 1.0
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GameDesign {
    pub hammer: HammerDesign,
    pub hammer_strike: HammerStrikeDesign,
    pub hammerer: CharacterDesign,
    /// Which authored parts of each Character can be hit.
    pub mage: MageDesign,
    pub mage_eye_beams: MageEyeBeamsDesign,
    pub mass: MassModelDefinition,
    /// Which authored parts of each Character its health is derived from.
    pub health_geometry: HealthGeometryDefinition,
    /// How every Character may traverse authored world surfaces.
    pub traversal: TraversalDesign,
    /// What each playable character brings into the world, keyed by character.
    ///
    /// Derived while loading so that callers look a character up instead of
    /// matching on its name.
    #[serde(skip)]
    pub characters: HashMap<CharacterId, CharacterProfile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TraversalDesign {
    pub schema_version: u32,
    pub characters: Vec<CharacterTraversalDesign>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CharacterTraversalDesign {
    pub asset_key: String,
    pub surfaces: Vec<String>,
    pub max_step_height_meters: f32,
    /// How deep the standing water may be that this Character still walks
    /// through. Deeper water is no place for it, the same way a higher step is
    /// no step.
    pub max_wade_depth_meters: f32,
    pub normal_speed_max_abs_grade_percent: u32,
    pub passable_max_abs_grade_percent: u32,
    pub reduced_speed_multiplier: f32,
}

impl TraversalDesign {
    fn is_valid_for(&self, mass: &MassModelDefinition) -> bool {
        if self.schema_version != 1 || self.characters.len() != mass.characters.len() {
            return false;
        }
        let expected = mass
            .characters
            .iter()
            .map(|assignment| assignment.asset_key.as_str())
            .collect::<HashSet<_>>();
        let actual = self
            .characters
            .iter()
            .map(|profile| profile.asset_key.as_str())
            .collect::<HashSet<_>>();
        expected == actual
            && actual.len() == self.characters.len()
            && self
                .characters
                .iter()
                .all(CharacterTraversalDesign::is_valid)
    }
}

impl CharacterTraversalDesign {
    fn is_valid(&self) -> bool {
        !self.asset_key.is_empty()
            && !self.surfaces.is_empty()
            && self.surfaces.iter().all(|surface| !surface.is_empty())
            && self.surfaces.iter().collect::<HashSet<_>>().len() == self.surfaces.len()
            && self.max_step_height_meters.is_finite()
            && self.max_step_height_meters >= 0.0
            && self.max_wade_depth_meters.is_finite()
            && self.max_wade_depth_meters >= 0.0
            && self.normal_speed_max_abs_grade_percent <= self.passable_max_abs_grade_percent
            && self.passable_max_abs_grade_percent <= 100
            && self.reduced_speed_multiplier.is_finite()
            && self.reduced_speed_multiplier > 0.0
            && self.reduced_speed_multiplier < 1.0
    }
}

/// The abilities and equipment a playable character brings into the world.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterProfile {
    pub ability_name_keys: Vec<String>,
    pub equipped_weapon_asset_key: Option<String>,
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
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MageDesign {
    pub schema_version: u32,
    pub asset_key: String,
    pub ability_name_keys: Vec<String>,
    pub pupil_size_ratio: f32,
    pub pupil_edge_clearance_ratio: f32,
    pub laser_width_to_eye_width_ratio: f32,
}

impl MageDesign {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 2
            && self.asset_key == "mage"
            && valid_ability_name_keys(&self.ability_name_keys)
            && self.pupil_size_ratio.is_finite()
            && self.pupil_size_ratio > 0.0
            && self.pupil_edge_clearance_ratio.is_finite()
            && (0.0..=1.0).contains(&self.pupil_edge_clearance_ratio)
            && self.laser_width_to_eye_width_ratio.is_finite()
            && self.laser_width_to_eye_width_ratio > 0.0
            && self.laser_width_to_eye_width_ratio <= 1.0
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HammerDesign {
    pub schema_version: u32,
    pub asset_key: String,
    pub ability_name_keys: Vec<String>,
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
    pub ability_name_keys: Vec<String>,
    #[serde(default)]
    pub equipped_weapon_asset_key: Option<String>,
}

impl CharacterDesign {
    pub fn is_valid(&self, expected_asset_key: &str) -> bool {
        self.schema_version == 2
            && self.asset_key == expected_asset_key
            && valid_ability_name_keys(&self.ability_name_keys)
            && !matches!(self.equipped_weapon_asset_key.as_deref(), Some(""))
    }
}

fn valid_ability_name_keys(name_keys: &[String]) -> bool {
    !name_keys.is_empty()
        && name_keys.iter().all(|name_key| !name_key.is_empty())
        && unique_names(name_keys)
}

/// Resolves each character's design into a profile, rejecting references to
/// abilities or weapons that no design defines.
fn character_profiles(
    designs: &[(&str, &[String], Option<&str>)],
    known_abilities: &HashSet<&str>,
    known_weapons: &HashSet<&str>,
) -> Result<HashMap<CharacterId, CharacterProfile>, DesignError> {
    let mut profiles = HashMap::new();
    for &(asset_key, ability_name_keys, equipped_weapon_asset_key) in designs {
        for name_key in ability_name_keys {
            if !known_abilities.contains(name_key.as_str()) {
                return Err(DesignError(format!(
                    "{asset_key} references unknown ability '{name_key}'"
                )));
            }
        }
        if let Some(weapon) = equipped_weapon_asset_key {
            if !known_weapons.contains(weapon) {
                return Err(DesignError(format!(
                    "{asset_key} equips unknown weapon '{weapon}'"
                )));
            }
        }
        let Some(character) = CharacterId::new(asset_key) else {
            return Err(DesignError("character asset key must not be empty".into()));
        };
        let profile = CharacterProfile {
            ability_name_keys: ability_name_keys.to_vec(),
            equipped_weapon_asset_key: equipped_weapon_asset_key.map(str::to_owned),
        };
        if profiles.insert(character, profile).is_some() {
            return Err(DesignError(format!(
                "duplicate character design '{asset_key}'"
            )));
        }
    }
    Ok(profiles)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignError(String);

impl DesignError {
    /// Refuse design data, from anywhere.
    ///
    /// A game crate parses its own design files and fails the same way the
    /// sandbox does, so the type is constructible from outside this crate
    /// while its message stays owned by it.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for DesignError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for DesignError {}

pub fn load_world01_embedded() -> Result<World01Design, DesignError> {
    parse_world01_design(WORLD01_TOML)
}

pub fn load_world01_file(path: &Path) -> Result<World01Design, Box<dyn Error + Send + Sync>> {
    let contents = std::fs::read_to_string(path)?;
    Ok(parse_world01_design(&contents)?)
}

fn parse_world01_design(contents: &str) -> Result<World01Design, DesignError> {
    let world01: World01Design = toml::from_str(contents)
        .map_err(|error| DesignError(format!("cannot parse World 01 design: {error}")))?;
    world01.placement_ranks()?;
    if !world01.mass.is_valid()
        || !world01.locomotion.is_valid()
        || !world01.health.is_valid()
        || !world01.weapon_aim.is_valid()
        || !world01.eyes.is_valid()
    {
        return Err(DesignError("World 01 design is invalid".into()));
    }
    Ok(world01)
}

pub fn load_embedded() -> Result<GameDesign, DesignError> {
    let hammer: HammerDesign = serde_json::from_str(HAMMER_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse hammer design: {error}")))?;
    let hammer_strike: HammerStrikeDesign = serde_json::from_str(HAMMER_STRIKE_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse HammerStrike design: {error}")))?;
    let hammerer: CharacterDesign = serde_json::from_str(HAMMERER_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse hammerer design: {error}")))?;
    let mage: MageDesign = serde_json::from_str(MAGE_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse Mage design: {error}")))?;
    let mage_eye_beams: MageEyeBeamsDesign = serde_json::from_str(MAGE_EYE_BEAMS_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse MageEyeBeams design: {error}")))?;
    let mass: MassModelDefinition = serde_json::from_str(MASS_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse mass design: {error}")))?;
    let health_geometry: HealthGeometryDefinition = serde_json::from_str(HP_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse health geometry design: {error}")))?;
    let traversal: TraversalDesign = serde_json::from_str(TRAVERSAL_DESIGN)
        .map_err(|error| DesignError(format!("cannot parse traversal design: {error}")))?;
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
    if !hammerer.is_valid("hammerer") {
        return Err(DesignError("Hammerer design is invalid".into()));
    }
    if !mage.is_valid() || !mage_eye_beams.is_valid() {
        return Err(DesignError("Mage eye-beam design is invalid".into()));
    }
    if !health_geometry.is_valid() {
        return Err(DesignError("health geometry design is invalid".into()));
    }
    if !mass.is_valid() {
        return Err(DesignError("mass design is invalid".into()));
    }
    if !traversal.is_valid_for(&mass) {
        return Err(DesignError("traversal design is invalid".into()));
    }
    let known_abilities = HashSet::from([
        hammer_strike.name_key.as_str(),
        mage_eye_beams.name_key.as_str(),
    ]);
    let known_weapons = HashSet::from([hammer.asset_key.as_str()]);
    let characters = character_profiles(
        &[
            (
                hammerer.asset_key.as_str(),
                hammerer.ability_name_keys.as_slice(),
                hammerer.equipped_weapon_asset_key.as_deref(),
            ),
            (
                mage.asset_key.as_str(),
                mage.ability_name_keys.as_slice(),
                None,
            ),
        ],
        &known_abilities,
        &known_weapons,
    )?;
    Ok(GameDesign {
        hammer,
        hammer_strike,
        hammerer,
        mage,
        mage_eye_beams,
        mass,
        health_geometry,
        traversal,
        characters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_world_data::{WorldMap, WorldTemplateCatalog};

    #[test]
    fn embedded_design_contains_the_component_based_hammer_attack() {
        let design = load_embedded().expect("embedded game design parses");
        assert_eq!(design.hammer.asset_key, "hammer");
        assert_eq!(design.hammer.ability_name_keys, ["HammerStrike"]);
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
        assert!(design.mage.is_valid());
        assert_eq!(design.mage_eye_beams.minimum_charge_seconds, 1.0);
        assert_eq!(design.mage_eye_beams.maximum_charge_seconds, 2.0);
        assert_eq!(design.mage_eye_beams.forced_release_seconds, 4.0);
        assert_eq!(design.mage_eye_beams.cooldown_seconds, 2.0);
        assert_eq!(
            design.mage_eye_beams.projectile_speed_meters_per_second,
            10.0
        );
        // Counted against each other rather than written down, because that is
        // the rule: a Character added in PolyTools needs both, and neither
        // number is interesting on its own.
        assert!(design.mass.characters.len() >= 11);
        assert_eq!(
            design.traversal.characters.len(),
            design.mass.characters.len()
        );
        assert_eq!(design.mass.weapons.len(), 1);
        assert!(
            design
                .mass
                .characters
                .iter()
                .any(|assignment| assignment.asset_key == "hammerer")
        );
    }

    #[test]
    fn embedded_traversal_covers_every_character_without_a_default() {
        let design = load_embedded().expect("embedded game design parses");
        let hammerer = design
            .traversal
            .characters
            .iter()
            .find(|profile| profile.asset_key == "hammerer")
            .expect("Hammerer has an explicit traversal profile");

        assert_eq!(hammerer.surfaces, ["land", "wood", "stone"]);
        assert_eq!(hammerer.max_wade_depth_meters, 0.4);
        assert_eq!(hammerer.max_step_height_meters, 0.5);
        assert_eq!(hammerer.normal_speed_max_abs_grade_percent, 25);
        assert_eq!(hammerer.passable_max_abs_grade_percent, 50);
        assert_eq!(hammerer.reduced_speed_multiplier, 0.5);
    }

    #[test]
    fn embedded_design_binds_each_character_to_its_abilities() {
        let design = load_embedded().expect("embedded game design parses");

        let hammerer = design
            .characters
            .get(&CharacterId("hammerer".into()))
            .expect("Hammerer has a character profile");
        assert_eq!(hammerer.ability_name_keys, ["HammerStrike"]);
        assert_eq!(
            hammerer.equipped_weapon_asset_key.as_deref(),
            Some("hammer")
        );

        let mage = design
            .characters
            .get(&CharacterId("mage".into()))
            .expect("Mage has a character profile");
        assert_eq!(mage.ability_name_keys, ["MageEyeBeams"]);
        assert_eq!(mage.equipped_weapon_asset_key, None);
    }

    #[test]
    fn character_profiles_reject_unknown_abilities_and_weapons() {
        let known_abilities = HashSet::from(["HammerStrike"]);
        let known_weapons = HashSet::from(["hammer"]);
        let abilities = ["HammerStrike".to_owned()];
        let unknown_ability = ["Fireball".to_owned()];

        assert!(
            character_profiles(
                &[("hammerer", &unknown_ability[..], Some("hammer"))],
                &known_abilities,
                &known_weapons,
            )
            .is_err()
        );
        assert!(
            character_profiles(
                &[("hammerer", &abilities[..], Some("greatsword"))],
                &known_abilities,
                &known_weapons,
            )
            .is_err()
        );
        assert!(
            character_profiles(
                &[
                    ("hammerer", &abilities[..], Some("hammer")),
                    ("hammerer", &abilities[..], None),
                ],
                &known_abilities,
                &known_weapons,
            )
            .is_err()
        );
        assert!(
            character_profiles(
                &[("hammerer", &abilities[..], Some("hammer"))],
                &known_abilities,
                &known_weapons,
            )
            .is_ok()
        );
    }

    #[test]
    fn embedded_world01_design_contains_shared_baselines() {
        let design = load_world01_embedded().expect("embedded World 01 design parses");
        assert_eq!(design.eyes.pupil_area_ratio, 0.26);
        assert_eq!(design.mass.hammerer_speed_meters_per_second, 0.6);
        assert_eq!(design.locomotion.default_max_stamina, 100.0);
        assert_eq!(design.health.revival_seconds, 8.0);
        assert_eq!(design.weapon_aim.default_degrees_per_second, 60.0);
        let ranks = design
            .placement_ranks()
            .expect("embedded Placement Ranks are valid");
        assert_eq!(ranks.rank("grass"), Some(10));
        assert_eq!(ranks.rank("tree"), Some(20));
        assert_eq!(ranks.rank("ankh"), Some(100));
    }

    #[test]
    fn invalid_placement_ranks_report_their_specific_cause() {
        let duplicated = WORLD01_TOML.replace("asset_key = \"tree\"", "asset_key = \"grass\"");

        let error = parse_world01_design(&duplicated)
            .expect_err("duplicate Placement Rank Asset keys are invalid");

        assert!(error.to_string().contains("empty or duplicated"));
        assert!(error.to_string().contains("grass"));
    }

    #[test]
    fn embedded_placement_ranks_cover_and_replace_real_anchor_overlaps() {
        let design = load_world01_embedded().expect("embedded World 01 design parses");
        let ranks = design
            .placement_ranks()
            .expect("embedded Placement Ranks are valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("embedded SceneMaker Templates are valid");
        let overworld =
            WorldMap::load_embedded("overworld01").expect("embedded overworld is valid");
        let cave = WorldMap::load_embedded("cave01").expect("embedded cave is valid");

        ranks
            .validate_for(&overworld, &templates)
            .expect("ranks cover the overworld and Template catalog");
        ranks
            .validate_for(&cave, &templates)
            .expect("ranks cover the cave and Template catalog");

        let template = templates
            .template("test_template02")
            .expect("the embedded integration Template exists");
        for (anchor_id, displaced_tree) in [
            ("template_anchor_001", "tree_0004"),
            ("template_anchor_002", "tree_0001"),
        ] {
            assert!(
                overworld
                    .props()
                    .iter()
                    .any(|prop| prop.instance_id == displaced_tree),
                "the authored overlap target must exist before placing at {anchor_id}"
            );
            let placement = overworld
                .project_template(anchor_id, template)
                .expect("the embedded Template fits the real Anchor");
            let merged = overworld
                .merged_with(&placement, &ranks)
                .expect("the embedded ranks resolve the real placement");

            assert!(
                !merged
                    .props()
                    .iter()
                    .any(|prop| prop.instance_id == displaced_tree),
                "the Template Ankh must replace {displaced_tree} at {anchor_id}"
            );
            assert!(merged.props().iter().any(|prop| {
                prop.instance_id == format!("template.{anchor_id}.test_template02.ankh_0001")
            }));
            assert!(merged.props().iter().any(|prop| {
                prop.instance_id == format!("template.{anchor_id}.test_template02.tree_0001")
            }));
        }
    }
}
