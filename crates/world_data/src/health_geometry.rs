//! Which authored parts of a Character its maximum health is derived from.
//!
//! Declared per Character and deliberately not defaulted to a set of Component
//! names. A Character the declaration does not name derives no area at all and
//! falls back to a single hit point, so a newly authored Character loads and
//! plays without a design edit, and gains real health the moment someone
//! decides which of its parts count.

use serde::Deserialize;

use crate::mass::{all_unique_nonempty, unique_nonempty};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HealthGeometryDefinition {
    pub schema_version: u32,
    pub characters: Vec<CharacterHealthAssignment>,
}

/// The Components whose Fill Mesh area forms one Character's health.
///
/// Only closed shapes carry an area: a Component named here that draws an open
/// Contour is a design mistake rather than a zero, because the intent was to
/// count something.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CharacterHealthAssignment {
    pub asset_key: String,
    #[serde(default)]
    pub components: Vec<String>,
}

impl CharacterHealthAssignment {
    pub fn is_valid(&self) -> bool {
        all_unique_nonempty(self.components.iter())
    }
}

impl HealthGeometryDefinition {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && unique_nonempty(self.characters.iter().map(|entry| &entry.asset_key))
            && self
                .characters
                .iter()
                .all(CharacterHealthAssignment::is_valid)
    }

    pub fn character(&self, asset_key: &str) -> Option<&CharacterHealthAssignment> {
        self.characters
            .iter()
            .find(|entry| entry.asset_key == asset_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(asset_key: &str, components: &[&str]) -> CharacterHealthAssignment {
        CharacterHealthAssignment {
            asset_key: asset_key.into(),
            components: components.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn an_assignment_without_components_is_valid_and_means_no_area() {
        assert!(assignment("the_mirror", &[]).is_valid());
        assert!(assignment("hammerer", &["body", "feet"]).is_valid());
        assert!(!assignment("hammerer", &["body", "body"]).is_valid());
        assert!(!assignment("hammerer", &["body", ""]).is_valid());
    }

    #[test]
    fn a_definition_names_every_character_at_most_once() {
        let definition = |characters: Vec<CharacterHealthAssignment>| HealthGeometryDefinition {
            schema_version: 1,
            characters,
        };
        assert!(definition(vec![assignment("hammerer", &["body"])]).is_valid());
        assert!(
            !definition(vec![
                assignment("hammerer", &["body"]),
                assignment("hammerer", &["feet"]),
            ])
            .is_valid()
        );
        assert!(
            !HealthGeometryDefinition {
                schema_version: 2,
                characters: vec![],
            }
            .is_valid()
        );
    }
}
