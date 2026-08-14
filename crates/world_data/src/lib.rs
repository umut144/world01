//! Shared protocol-neutral domain data.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CharacterKind {
    Wizard,
    Mage,
    Sorcerer,
    Rogue,
    Glavier,
}

impl CharacterKind {
    pub const ALL: [Self; 5] = [
        Self::Wizard,
        Self::Mage,
        Self::Sorcerer,
        Self::Rogue,
        Self::Glavier,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Wizard => "Wizard",
            Self::Mage => "Mage",
            Self::Sorcerer => "Sorcerer",
            Self::Rogue => "Rogue",
            Self::Glavier => "Glavier",
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u64);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerOwner(pub u64);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedCharacter(pub CharacterKind);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Player;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(pub u32);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StandardRoom;

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub room: RoomId,
    pub x_meters: f32,
    pub y_meters: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MovementIntent {
    pub x: f32,
    pub y: f32,
}

impl MovementIntent {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}
