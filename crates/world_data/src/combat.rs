use std::f32::consts::TAU;

use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};

use crate::Position;

#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
pub struct CharacterHealth {
    pub current: f32,
    pub maximum: f32,
}

impl CharacterHealth {
    pub fn full(maximum: f32) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub enum HammerAttackPhase {
    #[default]
    Idle,
    Charging,
    Swing,
    Embedded,
    Recovery,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct HammerAttackState {
    pub phase: HammerAttackPhase,
    pub direction: GazeDirection,
    pub phase_ticks: u32,
    pub charge_ticks: u32,
    pub impact_point: Position,
}

impl HammerAttackState {
    pub const IDLE: Self = Self {
        phase: HammerAttackPhase::Idle,
        direction: GazeDirection::ZERO,
        phase_ticks: 0,
        charge_ticks: 0,
        impact_point: Position::ZERO,
    };
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct GazeDirection {
    pub x: f32,
    pub y: f32,
}

impl GazeDirection {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const RIGHT: Self = Self { x: 1.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub enum WeaponTurnDirection {
    #[default]
    Clockwise,
    CounterClockwise,
}

impl WeaponTurnDirection {
    pub const fn angle_sign(self) -> f32 {
        match self {
            Self::Clockwise => -1.0,
            Self::CounterClockwise => 1.0,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
pub struct WeaponAimState {
    pub angle_radians: f32,
    pub last_turn_direction: WeaponTurnDirection,
}

impl WeaponAimState {
    pub const RIGHT: Self = Self {
        angle_radians: 0.0,
        last_turn_direction: WeaponTurnDirection::Clockwise,
    };

    pub fn new(angle_radians: f32, last_turn_direction: WeaponTurnDirection) -> Self {
        Self {
            angle_radians: angle_radians.rem_euclid(TAU),
            last_turn_direction,
        }
    }

    pub fn direction(self) -> GazeDirection {
        GazeDirection::new(self.angle_radians.cos(), self.angle_radians.sin())
    }
}

impl Default for WeaponAimState {
    fn default() -> Self {
        Self::RIGHT
    }
}
