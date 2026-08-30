use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DensityClass {
    Weightless,
    VeryLight,
    Light,
    Medium,
    Heavy,
    VeryHeavy,
}

impl DensityClass {
    pub const ALL: [Self; 6] = [
        Self::Weightless,
        Self::VeryLight,
        Self::Light,
        Self::Medium,
        Self::Heavy,
        Self::VeryHeavy,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Weightless => 0,
            Self::VeryLight => 1,
            Self::Light => 2,
            Self::Medium => 3,
            Self::Heavy => 4,
            Self::VeryHeavy => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentMassClass {
    Excluded,
    Weightless,
    VeryLight,
    Light,
    Medium,
    Heavy,
    VeryHeavy,
}

impl ComponentMassClass {
    pub const fn density_class(self) -> Option<DensityClass> {
        match self {
            Self::Excluded => None,
            Self::Weightless => Some(DensityClass::Weightless),
            Self::VeryLight => Some(DensityClass::VeryLight),
            Self::Light => Some(DensityClass::Light),
            Self::Medium => Some(DensityClass::Medium),
            Self::Heavy => Some(DensityClass::Heavy),
            Self::VeryHeavy => Some(DensityClass::VeryHeavy),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentMassAssignment {
    pub component_name: String,
    pub classification: ComponentMassClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMassAssignment {
    pub asset_key: String,
    pub components: Vec<ComponentMassAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterMassAssignment {
    pub asset_key: String,
    pub components: Vec<ComponentMassAssignment>,
    pub equipped_weapon_asset_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MassModelDefinition {
    pub schema_version: u32,
    pub characters: Vec<CharacterMassAssignment>,
    pub weapons: Vec<AssetMassAssignment>,
}

impl MassModelDefinition {
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && unique_nonempty(
                self.characters
                    .iter()
                    .map(|assignment| &assignment.asset_key),
            )
            && unique_nonempty(self.weapons.iter().map(|assignment| &assignment.asset_key))
            && self.characters.iter().all(|assignment| {
                valid_component_assignments(&assignment.components)
                    && all_unique_nonempty(assignment.equipped_weapon_asset_keys.iter())
            })
            && self
                .weapons
                .iter()
                .all(|assignment| valid_component_assignments(&assignment.components))
    }
}

fn valid_component_assignments(assignments: &[ComponentMassAssignment]) -> bool {
    !assignments.is_empty()
        && unique_nonempty(
            assignments
                .iter()
                .map(|assignment| &assignment.component_name),
        )
}

fn unique_nonempty<'a>(values: impl Iterator<Item = &'a String>) -> bool {
    let mut values = values.peekable();
    if values.peek().is_none() {
        return false;
    }
    all_unique_nonempty(values)
}

fn all_unique_nonempty<'a>(mut values: impl Iterator<Item = &'a String>) -> bool {
    let mut unique = std::collections::HashSet::new();
    values.all(|value| !value.is_empty() && unique.insert(value))
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CharacterMass {
    pub body: f32,
    pub equipped_weapon: f32,
    pub movement: f32,
    pub normal_speed_meters_per_second: f32,
}

impl CharacterMass {
    pub const fn new(
        body: f32,
        equipped_weapon: f32,
        movement: f32,
        normal_speed_meters_per_second: f32,
    ) -> Self {
        Self {
            body,
            equipped_weapon,
            movement,
            normal_speed_meters_per_second,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_definition_rejects_empty_or_duplicate_assignments() {
        let valid_component = ComponentMassAssignment {
            component_name: "body".into(),
            classification: ComponentMassClass::Medium,
        };
        let mut definition = MassModelDefinition {
            schema_version: 1,
            characters: vec![CharacterMassAssignment {
                asset_key: "hammerer".into(),
                components: vec![valid_component.clone()],
                equipped_weapon_asset_keys: vec!["hammer".into()],
            }],
            weapons: vec![AssetMassAssignment {
                asset_key: "hammer".into(),
                components: vec![valid_component.clone()],
            }],
        };
        assert!(definition.is_valid());

        definition.characters[0].components.push(valid_component);
        assert!(!definition.is_valid());
    }
}
