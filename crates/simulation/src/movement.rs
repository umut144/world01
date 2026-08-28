use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use game01_configs::DesignConfig;
use game01_world_data::{
    BodyFacing, MovementDirection, MovementIntent, MovementSpeedScale, Position,
};

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct MovementStep {
    speed_meters_per_second: f32,
    seconds_per_tick: f32,
}

impl MovementStep {
    pub fn from_design(config: &DesignConfig) -> Result<Self, MovementConfigError> {
        let speed = config.movement.speed_meters_per_second;
        if !speed.is_finite() || speed < 0.0 {
            return Err(MovementConfigError::InvalidSpeed(speed));
        }
        let ticks_per_second = config.simulation.ticks_per_second;
        if ticks_per_second == 0 {
            return Err(MovementConfigError::ZeroTickRate);
        }
        Ok(Self {
            speed_meters_per_second: speed,
            seconds_per_tick: 1.0 / ticks_per_second as f32,
        })
    }

    pub fn displacement(self, intent: MovementIntent) -> Vec2 {
        normalized_intent(intent) * self.speed_meters_per_second * self.seconds_per_tick
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MovementConfigError {
    InvalidSpeed(f32),
    ZeroTickRate,
}

impl fmt::Display for MovementConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSpeed(speed) => write!(
                formatter,
                "movement speed must be finite and non-negative, received {speed}"
            ),
            Self::ZeroTickRate => {
                formatter.write_str("simulation tick rate must be greater than zero")
            }
        }
    }
}

impl Error for MovementConfigError {}

pub fn move_players(
    step: Res<MovementStep>,
    mut players: Query<(&MovementIntent, Option<&MovementSpeedScale>, &mut Position)>,
) {
    for (intent, speed_scale, mut position) in &mut players {
        let multiplier = speed_scale.map_or(1.0, |scale| scale.0);
        let displacement = step.displacement(*intent) * multiplier;
        let current = Vec2::new(position.x, position.y);
        let proposed = current + displacement;
        *position = Position::new(proposed.x, proposed.y);
    }
}

pub fn update_character_orientation(
    mut players: Query<(
        &MovementIntent,
        Option<&MovementSpeedScale>,
        &mut MovementDirection,
        &mut BodyFacing,
    )>,
) {
    for (movement, speed_scale, mut movement_direction, mut facing) in &mut players {
        let direction = normalized_intent(*movement) * speed_scale.map_or(1.0, |scale| scale.0);
        *movement_direction = MovementDirection::new(direction.x, direction.y);
        if direction.x.is_finite() {
            if direction.x < 0.0 {
                *facing = BodyFacing::Left;
            } else if direction.x > 0.0 {
                *facing = BodyFacing::Right;
            }
        }
    }
}

fn normalized_intent(intent: MovementIntent) -> Vec2 {
    if !intent.x.is_finite() || !intent.y.is_finite() {
        return Vec2::ZERO;
    }
    Vec2::new(intent.x, intent.y).clamp_length_max(1.0)
}
