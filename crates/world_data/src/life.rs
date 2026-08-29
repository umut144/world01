use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub enum CharacterLifeState {
    #[default]
    Alive,
    Downed,
    Dead,
}

impl CharacterLifeState {
    pub const fn is_alive(self) -> bool {
        matches!(self, Self::Alive)
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct DeathConfirmationState {
    pub held_ticks: u32,
}
