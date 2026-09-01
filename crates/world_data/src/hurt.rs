//! Which authored parts of a Character can be hit.
//!
//! Declared per Character and never defaulted. A Character the declaration
//! forgets is a load error, not an actor that silently cannot be hit, which is
//! the only way an art change that renames a Component gets noticed.

use serde::Deserialize;

use crate::mass::{all_unique_nonempty, unique_nonempty};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HurtGeometryDefinition {
    pub schema_version: u32,
    pub characters: Vec<CharacterHurtAssignment>,
}

/// What forms one Character's hurt geometry.
///
/// Exactly one of `components` and `regions` is populated. Components are the
/// provisional choice for the current roster; a Character that should be hit at
/// authored `hurt` Regions instead - the design's Ghost hit only at its eyes -
/// says so here rather than relying on a rule inside the importer.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CharacterHurtAssignment {
    pub asset_key: String,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub regions: Vec<String>,
}

impl CharacterHurtAssignment {
    pub fn is_valid(&self) -> bool {
        if self.components.is_empty() == self.regions.is_empty() {
            return false;
        }
        all_unique_nonempty(self.components.iter()) && all_unique_nonempty(self.regions.iter())
    }
}

impl HurtGeometryDefinition {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && unique_nonempty(self.characters.iter().map(|entry| &entry.asset_key))
            && self
                .characters
                .iter()
                .all(CharacterHurtAssignment::is_valid)
    }

    pub fn character(&self, asset_key: &str) -> Option<&CharacterHurtAssignment> {
        self.characters
            .iter()
            .find(|entry| entry.asset_key == asset_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(components: &[&str], regions: &[&str]) -> CharacterHurtAssignment {
        CharacterHurtAssignment {
            asset_key: "wraith".to_owned(),
            components: components.iter().map(|name| (*name).to_owned()).collect(),
            regions: regions.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn a_character_names_components_or_regions_but_never_both_and_never_neither() {
        assert!(assignment(&["body", "head"], &[]).is_valid());
        assert!(assignment(&[], &["eye_left", "eye_right"]).is_valid());
        assert!(!assignment(&[], &[]).is_valid());
        assert!(!assignment(&["body"], &["eye_left"]).is_valid());
    }

    #[test]
    fn repeated_or_empty_names_are_rejected() {
        assert!(!assignment(&["body", "body"], &[]).is_valid());
        assert!(!assignment(&["body", ""], &[]).is_valid());
    }

    #[test]
    fn two_assignments_for_one_character_are_rejected() {
        let definition = HurtGeometryDefinition {
            schema_version: 1,
            characters: vec![assignment(&["body"], &[]), assignment(&["head"], &[])],
        };

        assert!(!definition.is_valid());
    }
}
