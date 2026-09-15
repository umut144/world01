use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterId(pub String);

impl CharacterId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()).then_some(Self(value))
    }

    pub fn label(&self) -> String {
        self.0
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Identifies a simulated character, whoever or whatever drives it.
///
/// Every entity the simulation steps carries one, so simulation code never has
/// to ask whether a human is behind it. The id is assigned by the server and
/// replicated, which makes it stable across the client's prediction as well.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActorId(pub u64);

/// The connected peer that owns an actor.
///
/// Present only while a human client controls the actor; a server-driven actor
/// has none. No simulation system reads it - ownership is a transport concern.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerOwner(pub u64);

#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedCharacter(pub CharacterId);

/// The side an Actor fights for.
///
/// Identity and nothing else. A team decides what a game wants it to decide -
/// which objectives are yours, where you return, what a bot reads as an enemy -
/// and no rule in this crate or in the simulation reads it. In particular it
/// grants no damage immunity: World 01 has no friendly-fire category, so a team
/// is never a reason an otherwise valid collision does not land.
///
/// An Actor without one belongs to no side. That is a legal state, not a
/// missing value, because a game without sides is the ordinary case.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamId(pub u8);

/// Marks an entity as a placed, damageable Prop rather than a Character.
///
/// Identity and nothing else, the same as [`TeamId`]: it groups a
/// [`crate::WorldPosition`] and a [`crate::CharacterHealth`] as belonging to
/// one placed, destructible thing, so client code can find "the destructible
/// Props" without also matching every player Character, which carries both of
/// those components too. No rule in this crate or in the simulation reads it,
/// and it is never present alongside [`SelectedCharacter`] - a Prop is not a
/// Character and plays through neither its abilities nor its life state.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DestructibleProp;
