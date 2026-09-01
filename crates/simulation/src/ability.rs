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
pub struct CharacterAbilityCatalog {
    by_character: HashMap<CharacterId, Vec<Ability>>,
}

impl CharacterAbilityCatalog {
    pub fn from_design(design: &GameDesign) -> Result<Self, AbilityError> {
        let mut by_character = HashMap::with_capacity(design.characters.len());
        for (character, profile) in &design.characters {
            let mut abilities = Vec::with_capacity(profile.ability_name_keys.len());
            for name_key in &profile.ability_name_keys {
                let Some(ability) = Ability::from_name_key(name_key) else {
                    return Err(AbilityError(format!(
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
    pub fn insert_ability_state(&self, character: &CharacterId, entity: &mut EntityCommands<'_>) {
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
pub struct AbilityError(String);

impl fmt::Display for AbilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for AbilityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_design::{CharacterProfile, load_embedded};
    use world01_world_data::SelectedCharacter;

    #[test]
    fn embedded_design_resolves_the_implemented_abilities() {
        let design = load_embedded().expect("embedded game design parses");
        let catalog =
            CharacterAbilityCatalog::from_design(&design).expect("every ability is implemented");

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

        assert!(CharacterAbilityCatalog::from_design(&design).is_err());
    }

    /// Mirrors how the server attaches ability state: through `Commands`.
    fn attach_abilities(
        catalog: Res<CharacterAbilityCatalog>,
        characters: Query<(Entity, &SelectedCharacter)>,
        mut commands: Commands,
    ) {
        for (entity, character) in &characters {
            let mut entity = commands.entity(entity);
            catalog.insert_ability_state(&character.0, &mut entity);
        }
    }

    fn character(name: &str) -> SelectedCharacter {
        SelectedCharacter(CharacterId(name.to_owned()))
    }

    #[test]
    fn spawning_attaches_only_the_state_a_character_uses() {
        let design = load_embedded().expect("embedded game design parses");
        let catalog =
            CharacterAbilityCatalog::from_design(&design).expect("every ability is implemented");
        let mut app = App::new();
        app.insert_resource(catalog)
            .add_systems(Update, attach_abilities);
        let hammerer = app.world_mut().spawn(character("hammerer")).id();
        let mage = app.world_mut().spawn(character("mage")).id();
        let rogue = app.world_mut().spawn(character("rogue")).id();

        app.update();

        let world = app.world();
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
