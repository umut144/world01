use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
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

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct MovementDirection {
    pub x: f32,
    pub y: f32,
}

impl MovementDirection {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub enum BodyFacing {
    #[default]
    Authored,
    Left,
    Right,
}
