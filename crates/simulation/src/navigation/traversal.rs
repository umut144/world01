use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
use world01_design::TraversalDesign;
use world01_world_data::CharacterId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TraversalSpeed {
    Normal,
    Reduced(f32),
}

impl TraversalSpeed {
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::Normal => 1.0,
            Self::Reduced(multiplier) => multiplier,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharacterTraversalProfile {
    surfaces: HashSet<String>,
    max_step_height_meters: f32,
    max_wade_depth_meters: f32,
    normal_speed_max_abs_grade_percent: u32,
    passable_max_abs_grade_percent: u32,
    reduced_speed_multiplier: f32,
}

impl CharacterTraversalProfile {
    pub fn permits_surface(&self, surface: &str) -> bool {
        self.surfaces.contains(surface)
    }

    pub fn permits_step(&self, first_elevation: f32, second_elevation: f32) -> bool {
        first_elevation.is_finite()
            && second_elevation.is_finite()
            && (first_elevation - second_elevation).abs() <= self.max_step_height_meters
    }

    /// Whether a Character may stand on ground with this much water over it.
    ///
    /// The world says how deep the water is; how deep it may be is the
    /// Character's own business, exactly as with the height of a step.
    pub fn permits_wade(&self, water_depth_meters: f32) -> bool {
        water_depth_meters.is_finite() && water_depth_meters <= self.max_wade_depth_meters
    }

    /// Wading is slower than walking, so that crossing shallow water is a
    /// decision and not a shortcut. It borrows the multiplier a steep Path
    /// already uses; if the two ever want different numbers, this is where
    /// they part.
    pub fn speed_through_water(&self, water_depth_meters: f32) -> TraversalSpeed {
        if water_depth_meters > 0.0 {
            TraversalSpeed::Reduced(self.reduced_speed_multiplier)
        } else {
            TraversalSpeed::Normal
        }
    }

    pub fn speed_for_grade(&self, grade_percent: i32) -> Option<TraversalSpeed> {
        let magnitude = grade_percent.unsigned_abs();
        if magnitude <= self.normal_speed_max_abs_grade_percent {
            Some(TraversalSpeed::Normal)
        } else if magnitude <= self.passable_max_abs_grade_percent {
            Some(TraversalSpeed::Reduced(self.reduced_speed_multiplier))
        } else {
            None
        }
    }
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TraversalCatalog {
    profiles: HashMap<CharacterId, CharacterTraversalProfile>,
}

impl TraversalCatalog {
    pub fn from_design(design: &TraversalDesign) -> Result<Self, TraversalCatalogError> {
        let mut profiles = HashMap::with_capacity(design.characters.len());
        for authored in &design.characters {
            let character = CharacterId::new(&authored.asset_key).ok_or_else(|| {
                TraversalCatalogError("traversal Character ID must not be empty".into())
            })?;
            let profile = CharacterTraversalProfile {
                surfaces: authored.surfaces.iter().cloned().collect(),
                max_step_height_meters: authored.max_step_height_meters,
                max_wade_depth_meters: authored.max_wade_depth_meters,
                normal_speed_max_abs_grade_percent: authored.normal_speed_max_abs_grade_percent,
                passable_max_abs_grade_percent: authored.passable_max_abs_grade_percent,
                reduced_speed_multiplier: authored.reduced_speed_multiplier,
            };
            if profiles.insert(character, profile).is_some() {
                return Err(TraversalCatalogError(format!(
                    "duplicate traversal Character '{}'",
                    authored.asset_key
                )));
            }
        }
        Ok(Self { profiles })
    }

    pub fn character(&self, character: &CharacterId) -> Option<&CharacterTraversalProfile> {
        self.profiles.get(character)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraversalCatalogError(String);

impl fmt::Display for TraversalCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for TraversalCatalogError {}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_design::load_embedded;

    fn catalog() -> TraversalCatalog {
        let design = load_embedded().expect("embedded design loads");
        TraversalCatalog::from_design(&design.traversal)
            .expect("embedded traversal profiles are valid")
    }

    #[test]
    fn exact_authored_grade_boundaries_are_stable() {
        let catalog = catalog();
        let profile = catalog
            .character(&CharacterId("hammerer".into()))
            .expect("Hammerer has traversal rules");

        assert_eq!(profile.speed_for_grade(0), Some(TraversalSpeed::Normal));
        assert_eq!(profile.speed_for_grade(25), Some(TraversalSpeed::Normal));
        assert_eq!(profile.speed_for_grade(-25), Some(TraversalSpeed::Normal));
        assert_eq!(
            profile.speed_for_grade(50),
            Some(TraversalSpeed::Reduced(0.5))
        );
        assert_eq!(
            profile.speed_for_grade(-50),
            Some(TraversalSpeed::Reduced(0.5))
        );
        assert_eq!(profile.speed_for_grade(51), None);
    }

    #[test]
    fn wading_stops_at_the_authored_depth_and_costs_speed_up_to_it() {
        let catalog = catalog();
        let profile = catalog
            .character(&CharacterId("hammerer".into()))
            .expect("Hammerer has traversal rules");

        assert!(profile.permits_wade(0.0));
        assert!(profile.permits_wade(0.4));
        assert!(!profile.permits_wade(0.400_1));
        assert!(!profile.permits_wade(f32::NAN));

        assert_eq!(profile.speed_through_water(0.0), TraversalSpeed::Normal);
        assert_eq!(
            profile.speed_through_water(0.1),
            TraversalSpeed::Reduced(0.5),
            "water at all is slower than none, so crossing it is a decision"
        );
        assert_eq!(
            profile.speed_through_water(0.4),
            TraversalSpeed::Reduced(0.5)
        );
    }

    #[test]
    fn every_authored_character_wades_the_same_depth_for_now() {
        let design = load_embedded().expect("embedded design loads");

        assert!(
            design
                .traversal
                .characters
                .iter()
                .all(|character| character.max_wade_depth_meters == 0.4),
            "one depth for every Character until a Character is meant to differ"
        );
    }

    #[test]
    fn steps_are_symmetric_and_include_the_half_meter_boundary() {
        let catalog = catalog();
        let profile = catalog
            .character(&CharacterId("mage".into()))
            .expect("Mage has traversal rules");

        assert!(profile.permits_step(1.0, 1.5));
        assert!(profile.permits_step(1.5, 1.0));
        assert!(!profile.permits_step(1.0, 1.500_1));
    }
}
