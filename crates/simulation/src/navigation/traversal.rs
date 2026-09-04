use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
use world01_content::CharacterCollisionGeometryCatalog;
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
    normal_speed_max_abs_grade_percent: u32,
    passable_max_abs_grade_percent: u32,
    reduced_speed_multiplier: f32,
    clearance_meters: f32,
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

    pub const fn clearance_meters(&self) -> f32 {
        self.clearance_meters
    }
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TraversalCatalog {
    profiles: HashMap<CharacterId, CharacterTraversalProfile>,
}

impl TraversalCatalog {
    pub fn from_design_and_geometry(
        design: &TraversalDesign,
        geometry: &CharacterCollisionGeometryCatalog,
    ) -> Result<Self, TraversalCatalogError> {
        let mut profiles = HashMap::with_capacity(design.characters.len());
        for authored in &design.characters {
            let character = CharacterId::new(&authored.asset_key).ok_or_else(|| {
                TraversalCatalogError("traversal Character ID must not be empty".into())
            })?;
            let clearance_meters = geometry.character(&character).map_or(0.0, |geometry| {
                geometry
                    .components
                    .iter()
                    .map(|component| {
                        let mut minimum = bevy::prelude::Vec2::splat(f32::INFINITY);
                        let mut maximum = bevy::prelude::Vec2::splat(f32::NEG_INFINITY);
                        for point in &component.geometry().vertices {
                            minimum = minimum.min(*point);
                            maximum = maximum.max(*point);
                        }
                        let size = maximum - minimum;
                        size.x.max(size.y) * 0.5
                    })
                    .fold(0.0, f32::max)
            });
            if !clearance_meters.is_finite() {
                return Err(TraversalCatalogError(format!(
                    "Character '{}' has non-finite navigation clearance",
                    authored.asset_key
                )));
            }
            let profile = CharacterTraversalProfile {
                surfaces: authored.surfaces.iter().cloned().collect(),
                max_step_height_meters: authored.max_step_height_meters,
                normal_speed_max_abs_grade_percent: authored.normal_speed_max_abs_grade_percent,
                passable_max_abs_grade_percent: authored.passable_max_abs_grade_percent,
                reduced_speed_multiplier: authored.reduced_speed_multiplier,
                clearance_meters,
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
    use world01_content::{CharacterCollisionGeometryCatalog, RuntimeContent};
    use world01_design::load_embedded;

    fn catalog() -> TraversalCatalog {
        let content = RuntimeContent::load_embedded().expect("embedded content loads");
        let geometry = CharacterCollisionGeometryCatalog::from_content(&content)
            .expect("embedded collision geometry is valid");
        let design = load_embedded().expect("embedded design loads");
        TraversalCatalog::from_design_and_geometry(&design.traversal, &geometry)
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
    fn steps_are_symmetric_and_include_the_half_meter_boundary() {
        let catalog = catalog();
        let profile = catalog
            .character(&CharacterId("mage".into()))
            .expect("Mage has traversal rules");

        assert!(profile.permits_step(1.0, 1.5));
        assert!(profile.permits_step(1.5, 1.0));
        assert!(!profile.permits_step(1.0, 1.500_1));
    }

    #[test]
    fn clearance_comes_from_authored_character_collision() {
        let catalog = catalog();
        let hammerer = catalog
            .character(&CharacterId("hammerer".into()))
            .expect("Hammerer has traversal rules");
        let mage = catalog
            .character(&CharacterId("mage".into()))
            .expect("Mage has traversal rules");

        assert!(hammerer.clearance_meters() > mage.clearance_meters());
        assert!(
            (hammerer.clearance_meters() - 0.495).abs() < 0.001,
            "Hammerer clearance: {}",
            hammerer.clearance_meters()
        );
        assert!(
            (mage.clearance_meters() - 0.33).abs() < 0.001,
            "Mage clearance: {}",
            mage.clearance_meters()
        );
    }
}
