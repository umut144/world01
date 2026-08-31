//! Resolves a character's design profile into the components its abilities need.

use std::{collections::HashMap, error::Error, fmt};

use bevy::prelude::*;
use world01_design::GameDesign;
use world01_world_data::{CharacterId, HammerAttackState, MageAttackState, WeaponAimState};

/// A gameplay ability the simulation implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ability {
    HammerStrike,
    MageEyeBeams,
}

impl Ability {
    /// The single place where a design name key becomes executable behaviour.
    ///
    /// Adding an ability means adding an arm here and the systems behind it,
    /// not another character-name comparison somewhere in the simulation.
    fn from_name_key(name_key: &str) -> Option<Self> {
        match name_key {
            "HammerStrike" => Some(Self::HammerStrike),
            "MageEyeBeams" => Some(Self::MageEyeBeams),
            _ => None,
        }
    }
}

/// Which abilities each character has, derived once from the game design.
///
/// Every spawner - the join handler today, bot spawners later - asks this
/// catalog what to attach, so players and bots cannot drift apart.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct CharacterArchetypeCatalog {
    by_character: HashMap<CharacterId, Vec<Ability>>,
}

impl CharacterArchetypeCatalog {
    pub fn from_design(design: &GameDesign) -> Result<Self, ArchetypeError> {
        let mut by_character = HashMap::with_capacity(design.characters.len());
        for (character, profile) in &design.characters {
            let mut abilities = Vec::with_capacity(profile.ability_name_keys.len());
            for name_key in &profile.ability_name_keys {
                let Some(ability) = Ability::from_name_key(name_key) else {
                    return Err(ArchetypeError(format!(
                        "{} needs ability '{name_key}', which is not implemented",
                        character.0
                    )));
                };
                abilities.push(ability);
            }
            by_character.insert(character.clone(), abilities);
        }
        Ok(Self { by_character })
    }

    pub fn abilities(&self, character: &CharacterId) -> &[Ability] {
        self.by_character.get(character).map_or(&[], Vec::as_slice)
    }

    /// Attaches the authoritative state every ability of `character` needs.
    pub fn insert_ability_state(&self, character: &CharacterId, entity: &mut EntityCommands) {
        for ability in self.abilities(character) {
            match ability {
                Ability::HammerStrike => {
                    entity.insert((WeaponAimState::RIGHT, HammerAttackState::IDLE));
                }
                Ability::MageEyeBeams => {
                    entity.insert(MageAttackState::IDLE);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchetypeError(String);

impl fmt::Display for ArchetypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ArchetypeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_design::{CharacterProfile, load_embedded};

    #[test]
    fn embedded_design_resolves_the_implemented_abilities() {
        let design = load_embedded().expect("embedded game design parses");
        let catalog =
            CharacterArchetypeCatalog::from_design(&design).expect("every ability is implemented");

        assert_eq!(
            catalog.abilities(&CharacterId("hammerer".into())),
            [Ability::HammerStrike].as_slice()
        );
        assert_eq!(
            catalog.abilities(&CharacterId("mage".into())),
            [Ability::MageEyeBeams].as_slice()
        );
        assert!(catalog.abilities(&CharacterId("rogue".into())).is_empty());
    }

    #[test]
    fn an_ability_without_an_implementation_is_rejected() {
        let mut design = load_embedded().expect("embedded game design parses");
        design.characters.insert(
            CharacterId("rogue".into()),
            CharacterProfile {
                ability_name_keys: vec!["Backstab".to_owned()],
                equipped_weapon_asset_key: None,
            },
        );

        assert!(CharacterArchetypeCatalog::from_design(&design).is_err());
    }

    fn spawn_with_abilities(
        world: &mut World,
        catalog: &CharacterArchetypeCatalog,
        character: &str,
    ) -> Entity {
        let entity = world.spawn_empty().id();
        {
            let mut commands = world.commands();
            let mut entity_commands = commands.entity(entity);
            catalog.insert_ability_state(&CharacterId(character.to_owned()), &mut entity_commands);
        }
        world.flush();
        entity
    }

    #[test]
    fn spawning_attaches_only_the_state_a_character_uses() {
        let design = load_embedded().expect("embedded game design parses");
        let catalog =
            CharacterArchetypeCatalog::from_design(&design).expect("every ability is implemented");
        let mut world = World::new();

        let hammerer = spawn_with_abilities(&mut world, &catalog, "hammerer");
        let mage = spawn_with_abilities(&mut world, &catalog, "mage");
        let rogue = spawn_with_abilities(&mut world, &catalog, "rogue");

        assert_eq!(
            world.get::<HammerAttackState>(hammerer),
            Some(&HammerAttackState::IDLE)
        );
        assert_eq!(
            world.get::<WeaponAimState>(hammerer),
            Some(&WeaponAimState::RIGHT)
        );
        assert!(world.get::<MageAttackState>(hammerer).is_none());

        assert_eq!(
            world.get::<MageAttackState>(mage),
            Some(&MageAttackState::IDLE)
        );
        assert!(world.get::<HammerAttackState>(mage).is_none());

        assert!(world.get::<HammerAttackState>(rogue).is_none());
        assert!(world.get::<MageAttackState>(rogue).is_none());
    }
}
