use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use world01_configs::RuntimeConfig;
use world01_world_data::{
    BodyFacing, MovementDirection, MovementIntent, MovementVelocity, Position,
};

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct MovementStep {
    seconds_per_tick: f32,
}

impl MovementStep {
    pub fn from_runtime(config: &RuntimeConfig) -> Result<Self, MovementConfigError> {
        let ticks_per_second = config.simulation.ticks_per_second;
        if ticks_per_second == 0 {
            return Err(MovementConfigError::ZeroTickRate);
        }
        Ok(Self {
            seconds_per_tick: 1.0 / ticks_per_second as f32,
        })
    }

    pub fn displacement(self, intent: MovementIntent, normal_speed: f32) -> Vec2 {
        self.step(self.velocity(intent, normal_speed, 1.0))
    }

    /// The offset one tick of `velocity` produces.
    ///
    /// The collision phase proposes a position with this before
    /// [`integrate_movement`] applies it, so both agree by construction on what
    /// a tick of movement means.
    pub fn step(self, velocity: MovementVelocity) -> Vec2 {
        Vec2::new(velocity.x, velocity.y) * self.seconds_per_tick
    }

    /// The inverse of [`Self::step`]: the velocity that produces `displacement`
    /// in one tick, so a collision that shortens a step says so as a velocity
    /// rather than by writing a position.
    pub fn velocity_of(self, displacement: Vec2) -> MovementVelocity {
        let velocity = displacement / self.seconds_per_tick;
        MovementVelocity::new(velocity.x, velocity.y)
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

/// Applies the velocity decided this tick to the actor's position.
///
/// The only writer of `Position` during the gameplay step, which is what makes
/// a collision phase possible between deciding a velocity and applying it.
pub fn integrate_movement(
    step: Res<MovementStep>,
    mut actors: Query<(&MovementVelocity, &mut Position)>,
) {
    for (velocity, mut position) in &mut actors {
        if *velocity == MovementVelocity::ZERO {
            continue;
        }
        let current = Vec2::new(position.x, position.y);
        let proposed = current + step.step(*velocity);
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
