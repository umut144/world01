use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub enum CharacterLifeState {
    #[default]
    Alive,
    Dead,
    DeathConfirming,
    Reviving,
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

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct RevivalState {
    pub reviver_actor_id: Option<u64>,
    pub held_ticks: u32,
}

impl RevivalState {
    pub const IDLE: Self = Self {
        reviver_actor_id: None,
        held_ticks: 0,
    };
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct RespawnState {
    pub count: u32,
}
