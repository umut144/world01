use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use world01_configs::DesignConfig;
use world01_world_data::{
    BodyFacing, CharacterLifeState, CharacterMass, MovementDirection, MovementIntent,
    MovementVelocity, Position,
};

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct MovementStep {
    seconds_per_tick: f32,
}

impl MovementStep {
    pub fn from_design(config: &DesignConfig) -> Result<Self, MovementConfigError> {
        let ticks_per_second = config.simulation.ticks_per_second;
        if ticks_per_second == 0 {
            return Err(MovementConfigError::ZeroTickRate);
        }
        Ok(Self {
            seconds_per_tick: 1.0 / ticks_per_second as f32,
        })
    }

    pub fn displacement(self, intent: MovementIntent, normal_speed: f32) -> Vec2 {
        let velocity = self.velocity(intent, normal_speed, 1.0);
        Vec2::new(velocity.x, velocity.y) * self.seconds_per_tick
    }

    pub fn velocity(
        self,
        intent: MovementIntent,
        normal_speed: f32,
        speed_multiplier: f32,
    ) -> MovementVelocity {
        let velocity = normalized_intent(intent) * normal_speed * speed_multiplier;
        MovementVelocity::new(velocity.x, velocity.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MovementConfigError {
    ZeroTickRate,
}

impl fmt::Display for MovementConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTickRate => {
                formatter.write_str("simulation tick rate must be greater than zero")
            }
        }
    }
}

impl Error for MovementConfigError {}

pub fn move_players(
    step: Res<MovementStep>,
    mut players: Query<(
        &MovementIntent,
        Option<&MovementVelocity>,
        Option<&CharacterLifeState>,
        &CharacterMass,
        &mut Position,
    )>,
) {
    for (intent, velocity, life, mass, mut position) in &mut players {
        if life.is_some_and(|life| !life.is_alive()) {
            continue;
        }
        let displacement = velocity.map_or_else(
            || step.displacement(*intent, mass.normal_speed_meters_per_second),
            |velocity| Vec2::new(velocity.x, velocity.y) * step.seconds_per_tick,
        );
        let current = Vec2::new(position.x, position.y);
        let proposed = current + displacement;
        *position = Position::new(proposed.x, proposed.y);
    }
}

pub fn update_character_orientation(
    mut players: Query<(
        &MovementIntent,
        Option<&MovementVelocity>,
        &mut MovementDirection,
        &mut BodyFacing,
    )>,
) {
    for (movement, velocity, mut movement_direction, mut facing) in &mut players {
        let direction = velocity.map_or_else(
            || normalized_intent(*movement),
            |velocity| Vec2::new(velocity.x, velocity.y).normalize_or_zero(),
        );
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

pub(crate) fn normalized_intent(intent: MovementIntent) -> Vec2 {
    if !intent.x.is_finite() || !intent.y.is_finite() {
        return Vec2::ZERO;
    }
    Vec2::new(intent.x, intent.y).clamp_length_max(1.0)
}
