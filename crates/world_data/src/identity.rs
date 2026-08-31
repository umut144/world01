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
