//! Input-, transport-, and presentation-independent game simulation.

use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use game01_configs::DesignConfig;
use game01_world_data::{MovementIntent, Position, RoomId, StartingRoomGrid};

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
    room_grid: Res<StartingRoomGrid>,
    mut players: Query<(&MovementIntent, &mut Position, &mut RoomId)>,
) {
    for (intent, mut position, mut room) in &mut players {
        let displacement = step.displacement(*intent);
        *position = room_grid.constrain_position(Position::new(
            position.x + displacement.x,
            position.y + displacement.y,
        ));
        *room = room_grid.room_id_at(*position);
    }
}

fn normalized_intent(intent: MovementIntent) -> Vec2 {
    if !intent.x.is_finite() || !intent.y.is_finite() {
        return Vec2::ZERO;
    }

    Vec2::new(intent.x, intent.y).clamp_length_max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Update};
    use game01_configs::{MovementConfig, NetworkConfig, SimulationConfig, load_embedded};

    const EPSILON: f32 = 0.000_01;

    fn movement_step() -> MovementStep {
        let config = load_embedded().expect("embedded design configuration parses");
        MovementStep::from_design(&config).expect("embedded movement configuration is valid")
    }

    #[test]
    fn cardinal_movement_uses_configured_speed_and_tick_rate() {
        let displacement = movement_step().displacement(MovementIntent::new(1.0, 0.0));

        assert!((displacement.x - 4.0 / 60.0).abs() < EPSILON);
        assert_eq!(displacement.y, 0.0);
    }

    #[test]
    fn diagonal_movement_is_normalized() {
        let step = movement_step();
        let cardinal_distance = step.displacement(MovementIntent::new(1.0, 0.0)).length();
        let diagonal_distance = step.displacement(MovementIntent::new(1.0, 1.0)).length();

        assert!((cardinal_distance - diagonal_distance).abs() < EPSILON);
    }

    #[test]
    fn intent_above_unit_length_is_clamped() {
        let step = movement_step();
        let unit_distance = step.displacement(MovementIntent::new(1.0, 0.0)).length();
        let excessive_distance = step.displacement(MovementIntent::new(10.0, 0.0)).length();

        assert!((unit_distance - excessive_distance).abs() < EPSILON);
    }

    #[test]
    fn invalid_intent_does_not_move() {
        let displacement = movement_step().displacement(MovementIntent::new(f32::NAN, 1.0));

        assert_eq!(displacement, Vec2::ZERO);
    }

    #[test]
    fn sixty_ticks_cover_four_meters() {
        let step = movement_step();
        let mut position = Vec2::ZERO;

        for _ in 0..60 {
            position += step.displacement(MovementIntent::new(0.0, 1.0));
        }

        assert!((position.y - 4.0).abs() < EPSILON);
    }

    #[test]
    fn movement_system_updates_authoritative_position() {
        let mut app = App::new();
        app.insert_resource(movement_step())
            .init_resource::<StartingRoomGrid>()
            .add_systems(Update, move_players);
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(-1.0, 0.0),
                Position::ZERO,
                StartingRoomGrid.starting_room(),
            ))
            .id();

        app.update();

        let position = app
            .world()
            .get::<Position>(player)
            .expect("spawned test player has a Position");
        assert!((position.x + 4.0 / 60.0).abs() < EPSILON);
        assert_eq!(position.y, 0.0);
    }

    #[test]
    fn movement_assigns_the_cardinal_neighbor_room_after_crossing_its_boundary() {
        let mut app = App::new();
        app.insert_resource(movement_step())
            .init_resource::<StartingRoomGrid>()
            .add_systems(Update, move_players);
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(1.0, 0.0),
                Position::new(7.49, 0.0),
                StartingRoomGrid.starting_room(),
            ))
            .id();

        app.update();

        assert_eq!(app.world().get::<RoomId>(player), Some(&RoomId(5)));
    }

    #[test]
    fn invalid_design_values_are_rejected() {
        let zero_tick_rate = DesignConfig {
            simulation: SimulationConfig {
                ticks_per_second: 0,
            },
            network: NetworkConfig {
                snapshot_send_hz: 30,
                remote_interpolation_ratio: 1.0,
            },
            movement: MovementConfig {
                speed_meters_per_second: 4.0,
            },
        };
        let negative_speed = DesignConfig {
            simulation: SimulationConfig {
                ticks_per_second: 60,
            },
            network: NetworkConfig {
                snapshot_send_hz: 30,
                remote_interpolation_ratio: 1.0,
            },
            movement: MovementConfig {
                speed_meters_per_second: -1.0,
            },
        };

        assert_eq!(
            MovementStep::from_design(&zero_tick_rate),
            Err(MovementConfigError::ZeroTickRate)
        );
        assert_eq!(
            MovementStep::from_design(&negative_speed),
            Err(MovementConfigError::InvalidSpeed(-1.0))
        );
    }
}
