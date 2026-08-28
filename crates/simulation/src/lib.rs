//! Input-, transport-, and presentation-independent game simulation.

use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use game01_configs::DesignConfig;
use game01_world_data::{
    AttackIntent, BodyFacing, GazeDirection, GazeIntent, HammerAttackPhase, HammerAttackState,
    MovementDirection, MovementIntent, Position, SelectedCharacter,
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

    fn displacement_scaled(self, intent: MovementIntent, multiplier: f32) -> Vec2 {
        self.displacement(intent) * multiplier
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct HammerAttackRules {
    maximum_charge_ticks: u32,
    swing_ticks: u32,
    recovery_ticks: u32,
    charging_movement_multiplier: f32,
}

impl HammerAttackRules {
    pub fn from_design(config: &DesignConfig) -> Result<Self, HammerAttackConfigError> {
        if !config.hammer_attack.is_valid() {
            return Err(HammerAttackConfigError);
        }
        let ticks_per_second = config.simulation.ticks_per_second;
        if ticks_per_second == 0 {
            return Err(HammerAttackConfigError);
        }

        let ticks = |seconds: f32| (seconds * ticks_per_second as f32).round().max(1.0) as u32;
        Ok(Self {
            maximum_charge_ticks: ticks(config.hammer_attack.maximum_charge_seconds),
            swing_ticks: ticks(config.hammer_attack.swing_seconds),
            recovery_ticks: ticks(config.hammer_attack.recovery_seconds),
            charging_movement_multiplier: config.hammer_attack.charging_movement_multiplier,
        })
    }

    pub fn maximum_charge_ticks(self) -> u32 {
        self.maximum_charge_ticks
    }

    pub fn swing_ticks(self) -> u32 {
        self.swing_ticks
    }

    pub fn recovery_ticks(self) -> u32 {
        self.recovery_ticks
    }

    pub fn charge_ratio(self, charge_ticks: u32) -> f32 {
        charge_ticks.min(self.maximum_charge_ticks) as f32 / self.maximum_charge_ticks as f32
    }

    fn movement_multiplier(self, attack: Option<&HammerAttackState>) -> f32 {
        match attack.map(|state| state.phase) {
            Some(HammerAttackPhase::Charging) => self.charging_movement_multiplier,
            _ => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HammerAttackConfigError;

impl fmt::Display for HammerAttackConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Hammer attack timings and movement multiplier must be valid")
    }
}

impl Error for HammerAttackConfigError {}

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
    attack_rules: Res<HammerAttackRules>,
    mut players: Query<(&MovementIntent, Option<&HammerAttackState>, &mut Position)>,
) {
    for (intent, attack, mut position) in &mut players {
        let displacement =
            step.displacement_scaled(*intent, attack_rules.movement_multiplier(attack));
        *position = Position::new(position.x + displacement.x, position.y + displacement.y);
    }
}

pub fn advance_hammer_attacks(
    rules: Res<HammerAttackRules>,
    mut players: Query<(
        &SelectedCharacter,
        &AttackIntent,
        &GazeDirection,
        &mut HammerAttackState,
    )>,
) {
    for (character, attack, gaze, mut state) in &mut players {
        if character.0.0 != "hammerer" {
            *state = HammerAttackState::IDLE;
            continue;
        }

        let gaze = valid_direction(*gaze);
        match state.phase {
            HammerAttackPhase::Idle => {
                if attack.pressed {
                    state.phase = HammerAttackPhase::Charging;
                    state.direction = gaze.unwrap_or(GazeDirection::ZERO);
                    state.phase_ticks = 0;
                    state.charge_ticks = 0;
                }
            }
            HammerAttackPhase::Charging => {
                if let Some(gaze) = gaze {
                    state.direction = gaze;
                }
                if attack.pressed {
                    state.phase_ticks = state
                        .phase_ticks
                        .saturating_add(1)
                        .min(rules.maximum_charge_ticks);
                    state.charge_ticks = state
                        .charge_ticks
                        .saturating_add(1)
                        .min(rules.maximum_charge_ticks);
                } else if valid_direction(state.direction).is_some() {
                    state.phase = HammerAttackPhase::Swing;
                    state.phase_ticks = 0;
                } else {
                    *state = HammerAttackState::IDLE;
                }
            }
            HammerAttackPhase::Swing => {
                let next_tick = state.phase_ticks.saturating_add(1);
                if next_tick >= rules.swing_ticks {
                    state.phase = HammerAttackPhase::Recovery;
                    state.phase_ticks = 0;
                } else {
                    state.phase_ticks = next_tick;
                }
            }
            HammerAttackPhase::Recovery => {
                let next_tick = state.phase_ticks.saturating_add(1);
                if next_tick >= rules.recovery_ticks {
                    *state = HammerAttackState::IDLE;
                } else {
                    state.phase_ticks = next_tick;
                }
            }
        }
    }
}

pub fn update_character_orientation(
    attack_rules: Res<HammerAttackRules>,
    mut players: Query<(
        &MovementIntent,
        Option<&HammerAttackState>,
        &mut MovementDirection,
        &mut BodyFacing,
    )>,
) {
    for (movement, attack, mut movement_direction, mut facing) in &mut players {
        let direction = normalized_intent(*movement) * attack_rules.movement_multiplier(attack);
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

pub fn update_gaze_direction(mut players: Query<(&GazeIntent, &mut GazeDirection)>) {
    for (gaze, mut gaze_direction) in &mut players {
        let gaze = Vec2::new(gaze.x, gaze.y);
        if gaze.is_finite() && gaze != Vec2::ZERO {
            let gaze = gaze.normalize();
            *gaze_direction = GazeDirection::new(gaze.x, gaze.y);
        }
    }
}

fn valid_direction(direction: GazeDirection) -> Option<GazeDirection> {
    let direction = Vec2::new(direction.x, direction.y);
    (direction.is_finite() && direction != Vec2::ZERO).then(|| {
        let direction = direction.normalize();
        GazeDirection::new(direction.x, direction.y)
    })
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
    use bevy::prelude::{App, IntoScheduleConfigs, Update};
    use game01_configs::{
        EyesConfig, HammerAttackConfig, MovementConfig, NetworkConfig, RoomConfig,
        SimulationConfig, load_embedded,
    };
    use game01_world_data::CharacterId;

    const EPSILON: f32 = 0.000_01;

    fn movement_step() -> MovementStep {
        let config = load_embedded().expect("embedded design configuration parses");
        MovementStep::from_design(&config).expect("embedded movement configuration is valid")
    }

    fn attack_rules() -> HammerAttackRules {
        let config = load_embedded().expect("embedded design configuration parses");
        HammerAttackRules::from_design(&config)
            .expect("embedded Hammer attack configuration is valid")
    }

    #[test]
    fn cardinal_movement_uses_configured_speed_and_tick_rate() {
        let displacement = movement_step().displacement(MovementIntent::new(1.0, 0.0));

        assert!((displacement.x - 0.8 / 60.0).abs() < EPSILON);
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
    fn sixty_ticks_cover_eight_tenths_of_a_meter() {
        let step = movement_step();
        let mut position = Vec2::ZERO;

        for _ in 0..60 {
            position += step.displacement(MovementIntent::new(0.0, 1.0));
        }

        assert!((position.y - 0.8).abs() < EPSILON);
    }

    #[test]
    fn movement_system_updates_authoritative_position() {
        let mut app = App::new();
        app.insert_resource(movement_step())
            .insert_resource(attack_rules())
            .add_systems(Update, move_players);
        let player = app
            .world_mut()
            .spawn((MovementIntent::new(-1.0, 0.0), Position::ZERO))
            .id();

        app.update();

        let position = app
            .world()
            .get::<Position>(player)
            .expect("spawned test player has a Position");
        assert!((position.x + 0.8 / 60.0).abs() < EPSILON);
        assert_eq!(position.y, 0.0);
    }

    #[test]
    fn orientation_follows_horizontal_movement_and_retains_last_gaze() {
        let mut app = App::new();
        app.insert_resource(attack_rules()).add_systems(
            Update,
            (update_gaze_direction, update_character_orientation).chain(),
        );
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(1.0, 0.0),
                GazeIntent::new(-1.0, 1.0),
                MovementDirection::ZERO,
                BodyFacing::Authored,
                GazeDirection::ZERO,
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<BodyFacing>(player),
            Some(&BodyFacing::Right)
        );
        assert_eq!(
            app.world().get::<MovementDirection>(player),
            Some(&MovementDirection::new(1.0, 0.0))
        );
        let diagonal = 1.0 / 2.0_f32.sqrt();
        assert_eq!(
            app.world().get::<GazeDirection>(player),
            Some(&GazeDirection::new(-diagonal, diagonal))
        );

        *app.world_mut()
            .get_mut::<MovementIntent>(player)
            .expect("player retains movement intent") = MovementIntent::new(0.0, 1.0);
        *app.world_mut()
            .get_mut::<GazeIntent>(player)
            .expect("player retains gaze intent") = GazeIntent::ZERO;
        app.update();

        assert_eq!(
            app.world().get::<BodyFacing>(player),
            Some(&BodyFacing::Right)
        );
        assert_eq!(
            app.world().get::<MovementDirection>(player),
            Some(&MovementDirection::new(0.0, 1.0))
        );
        assert_eq!(
            app.world().get::<GazeDirection>(player),
            Some(&GazeDirection::new(-diagonal, diagonal))
        );
    }

    #[test]
    fn invalid_orientation_input_does_not_replace_valid_state() {
        let mut app = App::new();
        app.insert_resource(attack_rules()).add_systems(
            Update,
            (update_gaze_direction, update_character_orientation).chain(),
        );
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(f32::NAN, 0.0),
                GazeIntent::new(f32::INFINITY, 0.0),
                MovementDirection::new(1.0, 0.0),
                BodyFacing::Left,
                GazeDirection::new(0.0, -1.0),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<BodyFacing>(player),
            Some(&BodyFacing::Left)
        );
        assert_eq!(
            app.world().get::<MovementDirection>(player),
            Some(&MovementDirection::ZERO)
        );
        assert_eq!(
            app.world().get::<GazeDirection>(player),
            Some(&GazeDirection::new(0.0, -1.0))
        );
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
            hammer_attack: HammerAttackConfig {
                maximum_charge_seconds: 5.0,
                swing_seconds: 0.45,
                recovery_seconds: 0.30,
                charging_movement_multiplier: 0.0,
            },
            room: RoomConfig {
                width_tiles: 15,
                height_tiles: 9,
            },
            camera: game01_configs::CameraConfig {
                view_preset: 0,
                view_width_tiles: 22,
                view_height_tiles: 20,
            },
            eyes: EyesConfig {
                pupil_area_ratio: 0.26,
                hammerer_collision_radius_ratio: 0.35,
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
            hammer_attack: HammerAttackConfig {
                maximum_charge_seconds: 5.0,
                swing_seconds: 0.45,
                recovery_seconds: 0.30,
                charging_movement_multiplier: 0.0,
            },
            room: RoomConfig {
                width_tiles: 15,
                height_tiles: 9,
            },
            camera: game01_configs::CameraConfig {
                view_preset: 0,
                view_width_tiles: 22,
                view_height_tiles: 20,
            },
            eyes: EyesConfig {
                pupil_area_ratio: 0.26,
                hammerer_collision_radius_ratio: 0.35,
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

    #[test]
    fn charging_caps_at_five_seconds_and_release_freezes_direction() {
        let rules = attack_rules();
        assert_eq!(rules.maximum_charge_ticks(), 300);
        assert_eq!(rules.swing_ticks(), 27);
        assert_eq!(rules.recovery_ticks(), 18);

        let mut app = App::new();
        app.insert_resource(rules).add_systems(
            Update,
            (update_gaze_direction, advance_hammer_attacks).chain(),
        );
        let player = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".to_owned())),
                AttackIntent::PRESSED,
                GazeIntent::new(1.0, 0.0),
                GazeDirection::new(1.0, 0.0),
                HammerAttackState::IDLE,
            ))
            .id();

        app.update();
        *app.world_mut()
            .get_mut::<GazeIntent>(player)
            .expect("Hammerer has gaze input") = GazeIntent::new(0.0, 1.0);
        for _ in 0..400 {
            app.update();
        }
        let charging = *app
            .world()
            .get::<HammerAttackState>(player)
            .expect("Hammerer has attack state");
        assert_eq!(charging.phase, HammerAttackPhase::Charging);
        assert_eq!(charging.charge_ticks, 300);
        assert_eq!(charging.direction, GazeDirection::new(0.0, 1.0));

        *app.world_mut()
            .get_mut::<AttackIntent>(player)
            .expect("Hammerer has attack input") = AttackIntent::RELEASED;
        *app.world_mut()
            .get_mut::<GazeIntent>(player)
            .expect("Hammerer has gaze input") = GazeIntent::new(-1.0, 0.0);
        app.update();
        *app.world_mut()
            .get_mut::<GazeIntent>(player)
            .expect("Hammerer has gaze input") = GazeIntent::new(0.0, -1.0);
        app.update();

        let swinging = app
            .world()
            .get::<HammerAttackState>(player)
            .expect("Hammerer has attack state");
        assert_eq!(swinging.phase, HammerAttackPhase::Swing);
        assert_eq!(swinging.direction, GazeDirection::new(-1.0, 0.0));
        assert_eq!(rules.charge_ratio(swinging.charge_ticks), 1.0);
    }

    #[test]
    fn charging_lock_blocks_authoritative_movement_until_release() {
        let mut app = App::new();
        app.insert_resource(movement_step())
            .insert_resource(attack_rules())
            .add_systems(Update, move_players);
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(1.0, 0.0),
                HammerAttackState {
                    phase: HammerAttackPhase::Charging,
                    ..HammerAttackState::IDLE
                },
                Position::ZERO,
            ))
            .id();

        app.update();
        assert_eq!(app.world().get::<Position>(player), Some(&Position::ZERO));

        app.world_mut()
            .get_mut::<HammerAttackState>(player)
            .expect("Hammerer has attack state")
            .phase = HammerAttackPhase::Swing;
        app.update();
        let position = app
            .world()
            .get::<Position>(player)
            .expect("Hammerer has Position");
        assert!((position.x - 0.8 / 60.0).abs() < EPSILON);
    }
}
