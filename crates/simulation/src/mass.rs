use std::{collections::HashMap, error::Error, fmt};

use bevy::prelude::Resource;
use world01_configs::DesignConfig;
use world01_content::{CharacterMassGeometryCatalog, DensityAreas};
use world01_world_data::{CharacterId, CharacterMass, DensityClass};

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CharacterMassCatalog {
    profiles: HashMap<CharacterId, CharacterMass>,
}

impl CharacterMassCatalog {
    pub fn from_geometry(
        config: &DesignConfig,
        geometry: &CharacterMassGeometryCatalog,
    ) -> Result<Self, MassModelError> {
        if !config.mass.is_valid() {
            return Err(MassModelError("mass configuration is invalid".into()));
        }
        let mut values = HashMap::new();
        for character in geometry.character_ids() {
            let body_areas = geometry.body(character).ok_or_else(|| {
                MassModelError(format!("{} is missing body mass geometry", character.0))
            })?;
            let weapon_areas = geometry.equipped_weapon(character).ok_or_else(|| {
                MassModelError(format!(
                    "{} is missing equipped weapon mass geometry",
                    character.0
                ))
            })?;
            let body = weighted_mass(body_areas, config);
            let equipped_weapon = weighted_mass(weapon_areas, config);
            let movement = if config.mass.include_equipped_weapon_mass {
                body + equipped_weapon
            } else {
                body
            };
            if !body.is_finite() || body <= 0.0 || !movement.is_finite() || movement <= 0.0 {
                return Err(MassModelError(format!(
                    "{} does not derive a positive finite movement mass",
                    character.0
                )));
            }
            values.insert(character.clone(), (body, equipped_weapon, movement));
        }
        let hammerer_movement = values
            .get(&CharacterId("hammerer".into()))
            .map(|(_, _, movement)| *movement)
            .ok_or_else(|| MassModelError("missing Hammerer mass geometry".into()))?;
        let mut profiles = HashMap::new();
        for (character, (body, equipped_weapon, movement)) in values {
            let normal_speed_meters_per_second = config.mass.hammerer_speed_meters_per_second
                * (hammerer_movement / movement).powf(config.mass.speed_mass_exponent);
            if !normal_speed_meters_per_second.is_finite() || normal_speed_meters_per_second <= 0.0
            {
                return Err(MassModelError(format!(
                    "{} does not derive a positive finite normal speed",
                    character.0
                )));
            }
            profiles.insert(
                character,
                CharacterMass::new(
                    body,
                    equipped_weapon,
                    movement,
                    normal_speed_meters_per_second,
                ),
            );
        }
        Ok(Self { profiles })
    }

    pub fn character(&self, character: &CharacterId) -> Option<CharacterMass> {
        self.profiles.get(character).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MassModelError(String);

impl fmt::Display for MassModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for MassModelError {}

fn weighted_mass(areas: DensityAreas, config: &DesignConfig) -> f32 {
    DensityClass::ALL
        .into_iter()
        .map(|class| areas.area(class) * config.mass.density_factor(class))
        .sum()
}
