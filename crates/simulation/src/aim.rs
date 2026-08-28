use std::{collections::HashMap, error::Error, f32::consts::PI, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use game01_configs::DesignConfig;
use game01_world_data::{
    GazeDirection, GazeIntent, SelectedCharacter, WeaponAimState, WeaponTurnDirection,
};

const OPPOSITE_ANGLE_EPSILON: f32 = 0.000_01;
const TARGET_CLAMP_EPSILON: f32 = 0.000_1;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct WeaponAimRules {
    default_radians_per_tick: f32,
    character_radians_per_tick: HashMap<String, f32>,
}

impl WeaponAimRules {
    pub fn from_design(config: &DesignConfig) -> Result<Self, WeaponAimConfigError> {
        if config.simulation.ticks_per_second == 0 || !config.weapon_aim.is_valid() {
            return Err(WeaponAimConfigError);
        }
        let ticks_per_second = config.simulation.ticks_per_second as f32;
        let radians_per_tick =
            |degrees_per_second: f32| degrees_per_second.to_radians() / ticks_per_second;
        Ok(Self {
            default_radians_per_tick: radians_per_tick(
                config.weapon_aim.default_degrees_per_second,
            ),
            character_radians_per_tick: config
                .weapon_aim
                .character_degrees_per_second
                .iter()
                .map(|(character, speed)| (character.clone(), radians_per_tick(*speed)))
                .collect(),
        })
    }

    pub(crate) fn radians_per_tick(&self, character: &SelectedCharacter) -> f32 {
        self.character_radians_per_tick
            .get(&character.0.0)
            .copied()
            .unwrap_or(self.default_radians_per_tick)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponAimConfigError;

impl fmt::Display for WeaponAimConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "weapon aim speeds must be finite and positive and tick rate must be non-zero",
        )
    }
}

impl Error for WeaponAimConfigError {}

pub fn update_gaze_direction(mut players: Query<(&GazeIntent, &mut GazeDirection)>) {
    for (intent, mut gaze) in &mut players {
        let direction = Vec2::new(intent.x, intent.y);
        if direction.is_finite() && direction != Vec2::ZERO {
            let direction = direction.normalize();
            *gaze = GazeDirection::new(direction.x, direction.y);
        }
    }
}

pub fn update_weapon_aim(
    rules: Res<WeaponAimRules>,
    mut players: Query<(
        &SelectedCharacter,
        &GazeIntent,
        &GazeDirection,
        &mut WeaponAimState,
    )>,
) {
    for (character, gaze_intent, gaze, mut state) in &mut players {
        let active_target = Vec2::new(gaze_intent.x, gaze_intent.y);
        let target = Vec2::new(gaze.x, gaze.y);
        if !active_target.is_finite() || active_target == Vec2::ZERO {
            continue;
        }
        if !target.is_finite() || target == Vec2::ZERO || !state.angle_radians.is_finite() {
            continue;
        }
        let target_angle = target.to_angle();
        let delta = shortest_angle_delta(state.angle_radians, target_angle);
        if delta.abs() <= f32::EPSILON {
            state.angle_radians = target_angle.rem_euclid(2.0 * PI);
            continue;
        }
        let turn_direction = if (delta.abs() - PI).abs() <= OPPOSITE_ANGLE_EPSILON {
            state.last_turn_direction
        } else if delta < 0.0 {
            WeaponTurnDirection::Clockwise
        } else {
            WeaponTurnDirection::CounterClockwise
        };
        let maximum_step = rules.radians_per_tick(character);
        state.last_turn_direction = turn_direction;
        if delta.abs() <= maximum_step + TARGET_CLAMP_EPSILON {
            state.angle_radians = target_angle.rem_euclid(2.0 * PI);
        } else {
            let signed_step = turn_direction.angle_sign() * maximum_step;
            state.angle_radians = (state.angle_radians + signed_step).rem_euclid(2.0 * PI);
        }
    }
}

fn shortest_angle_delta(current: f32, target: f32) -> f32 {
    (target - current + PI).rem_euclid(2.0 * PI) - PI
}
