//! Input-, transport-, and presentation-independent game simulation.

use std::{collections::HashMap, error::Error, f32::consts::PI, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use game01_configs::DesignConfig;
use game01_world_data::{
    AttackIntent, BodyFacing, GazeDirection, GazeIntent, GazeState, GazeTurnDirection,
    HammerAttackPhase, HammerAttackState, MovementDirection, MovementIntent, Position,
    SelectedCharacter,
};

const OPPOSITE_ANGLE_EPSILON: f32 = 0.000_01;
const TARGET_CLAMP_EPSILON: f32 = 0.000_1;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct GazeRules {
    default_radians_per_tick: f32,
    character_radians_per_tick: HashMap<String, f32>,
}

impl GazeRules {
    pub fn from_design(config: &DesignConfig) -> Result<Self, GazeConfigError> {
        if config.simulation.ticks_per_second == 0 || !config.gaze.is_valid() {
            return Err(GazeConfigError);
        }

        let ticks_per_second = config.simulation.ticks_per_second as f32;
        let radians_per_tick =
            |degrees_per_second: f32| degrees_per_second.to_radians() / ticks_per_second;
        Ok(Self {
            default_radians_per_tick: radians_per_tick(config.gaze.default_degrees_per_second),
            character_radians_per_tick: config
                .gaze
                .character_degrees_per_second
                .iter()
                .map(|(character, speed)| (character.clone(), radians_per_tick(*speed)))
                .collect(),
        })
    }

    fn radians_per_tick(&self, character: &SelectedCharacter) -> f32 {
        self.character_radians_per_tick
            .get(&character.0.0)
            .copied()
            .unwrap_or(self.default_radians_per_tick)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GazeConfigError;

impl fmt::Display for GazeConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .write_str("gaze speeds must be finite and positive and tick rate must be non-zero")
    }
}

impl Error for GazeConfigError {}

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
        &GazeState,
        &mut HammerAttackState,
    )>,
) {
    for (character, attack, gaze, mut state) in &mut players {
        if character.0.0 != "hammerer" {
            *state = HammerAttackState::IDLE;
            continue;
        }

        let gaze = valid_direction(gaze.direction());
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

pub fn update_gaze_state(
    rules: Res<GazeRules>,
    mut players: Query<(&SelectedCharacter, &GazeIntent, &mut GazeState)>,
) {
    for (character, gaze, mut state) in &mut players {
        let target = Vec2::new(gaze.x, gaze.y);
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
            GazeTurnDirection::Clockwise
        } else {
            GazeTurnDirection::CounterClockwise
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
        EyesConfig, GazeConfig, HammerAttackConfig, MovementConfig, NetworkConfig, RoomConfig,
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

    fn gaze_rules() -> GazeRules {
        let config = load_embedded().expect("embedded design configuration parses");
        GazeRules::from_design(&config).expect("embedded gaze configuration is valid")
    }

    fn character(name: &str) -> SelectedCharacter {
        SelectedCharacter(CharacterId(name.to_owned()))
    }

    fn assert_direction(actual: GazeDirection, expected: Vec2) {
        let actual = Vec2::new(actual.x, actual.y);
        assert!(
            actual.distance(expected) < EPSILON,
            "{actual:?} != {expected:?}"
        );
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
    fn orientation_follows_horizontal_movement() {
        let mut app = App::new();
        app.insert_resource(attack_rules())
            .add_systems(Update, update_character_orientation);
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(1.0, 0.0),
                MovementDirection::ZERO,
                BodyFacing::Authored,
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
        *app.world_mut()
            .get_mut::<MovementIntent>(player)
            .expect("player retains movement intent") = MovementIntent::new(0.0, 1.0);
        app.update();

        assert_eq!(
            app.world().get::<BodyFacing>(player),
            Some(&BodyFacing::Right)
        );
        assert_eq!(
            app.world().get::<MovementDirection>(player),
            Some(&MovementDirection::new(0.0, 1.0))
        );
    }

    #[test]
    fn invalid_orientation_input_does_not_replace_valid_state() {
        let mut app = App::new();
        app.insert_resource(attack_rules())
            .add_systems(Update, update_character_orientation);
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(f32::NAN, 0.0),
                MovementDirection::new(1.0, 0.0),
                BodyFacing::Left,
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
    }

    #[test]
    fn gaze_starts_right_and_approaches_target_at_60_degrees_per_second() {
        let mut app = App::new();
        app.insert_resource(gaze_rules())
            .add_systems(Update, update_gaze_state);
        let player = app
            .world_mut()
            .spawn((character("wizard"), GazeIntent::ZERO, GazeState::RIGHT))
            .id();

        app.update();
        assert_direction(
            app.world().get::<GazeState>(player).unwrap().direction(),
            Vec2::X,
        );

        *app.world_mut().get_mut::<GazeIntent>(player).unwrap() = GazeIntent::new(0.0, 1.0);
        for _ in 0..45 {
            app.update();
        }
        assert_direction(
            app.world().get::<GazeState>(player).unwrap().direction(),
            Vec2::from_angle(PI / 4.0),
        );
        for _ in 0..45 {
            app.update();
        }
        assert_direction(
            app.world().get::<GazeState>(player).unwrap().direction(),
            Vec2::Y,
        );
    }

    #[test]
    fn zero_or_invalid_gaze_target_retains_the_current_angle() {
        let mut app = App::new();
        app.insert_resource(gaze_rules())
            .add_systems(Update, update_gaze_state);
        let initial = GazeState::new(PI / 3.0, GazeTurnDirection::CounterClockwise);
        let player = app
            .world_mut()
            .spawn((character("mage"), GazeIntent::ZERO, initial))
            .id();

        app.update();
        assert_eq!(app.world().get::<GazeState>(player), Some(&initial));
        *app.world_mut().get_mut::<GazeIntent>(player).unwrap() =
            GazeIntent::new(f32::INFINITY, 0.0);
        app.update();
        assert_eq!(app.world().get::<GazeState>(player), Some(&initial));
    }

    #[test]
    fn opposite_target_continues_the_last_turn_direction() {
        let mut app = App::new();
        app.insert_resource(gaze_rules())
            .add_systems(Update, update_gaze_state);
        let clockwise = app
            .world_mut()
            .spawn((
                character("mage"),
                GazeIntent::new(-1.0, 0.0),
                GazeState::RIGHT,
            ))
            .id();
        let counterclockwise = app
            .world_mut()
            .spawn((
                character("rogue"),
                GazeIntent::new(-1.0, 0.0),
                GazeState::new(0.0, GazeTurnDirection::CounterClockwise),
            ))
            .id();

        app.update();

        let clockwise = app.world().get::<GazeState>(clockwise).unwrap();
        assert_eq!(clockwise.last_turn_direction, GazeTurnDirection::Clockwise);
        assert!((clockwise.angle_radians - (2.0 * PI - PI / 180.0)).abs() < EPSILON);
        let counterclockwise = app.world().get::<GazeState>(counterclockwise).unwrap();
        assert_eq!(
            counterclockwise.last_turn_direction,
            GazeTurnDirection::CounterClockwise
        );
        assert!((counterclockwise.angle_radians - PI / 180.0).abs() < EPSILON);
    }

    #[test]
    fn character_override_changes_only_that_characters_linear_speed() {
        let mut config = load_embedded().expect("embedded design configuration parses");
        config
            .gaze
            .character_degrees_per_second
            .insert("hammerer".to_owned(), 30.0);
        let rules = GazeRules::from_design(&config).expect("gaze override is valid");

        assert!((rules.radians_per_tick(&character("wizard")) - PI / 180.0).abs() < EPSILON);
        assert!((rules.radians_per_tick(&character("hammerer")) - PI / 360.0).abs() < EPSILON);
    }

    #[test]
    fn target_within_one_degree_clamps_to_the_exact_angle() {
        let mut app = App::new();
        app.insert_resource(gaze_rules())
            .add_systems(Update, update_gaze_state);
        let player = app
            .world_mut()
            .spawn((
                character("wizard"),
                GazeIntent::new(0.0, 1.0),
                GazeState::new(
                    PI / 2.0 - 0.5_f32.to_radians(),
                    GazeTurnDirection::CounterClockwise,
                ),
            ))
            .id();

        app.update();

        let state = app.world().get::<GazeState>(player).unwrap();
        assert_eq!(state.angle_radians, PI / 2.0);
        assert_direction(state.direction(), Vec2::Y);
    }

    #[test]
    fn all_eight_absolute_input_targets_settle_exactly() {
        let targets = [
            Vec2::X,
            Vec2::new(1.0, 1.0),
            Vec2::Y,
            Vec2::new(-1.0, 1.0),
            Vec2::NEG_X,
            Vec2::new(-1.0, -1.0),
            Vec2::NEG_Y,
            Vec2::new(1.0, -1.0),
        ];

        for target in targets {
            let mut app = App::new();
            app.insert_resource(gaze_rules())
                .add_systems(Update, update_gaze_state);
            let player = app
                .world_mut()
                .spawn((
                    character("wizard"),
                    GazeIntent::new(target.x, target.y),
                    GazeState::RIGHT,
                ))
                .id();

            for _ in 0..180 {
                app.update();
            }

            assert_direction(
                app.world().get::<GazeState>(player).unwrap().direction(),
                target.normalize(),
            );
        }
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
            gaze: GazeConfig {
                default_degrees_per_second: 60.0,
                character_degrees_per_second: HashMap::new(),
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
            gaze: GazeConfig {
                default_degrees_per_second: 60.0,
                character_degrees_per_second: HashMap::new(),
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
        assert_eq!(
            GazeRules::from_design(&zero_tick_rate),
            Err(GazeConfigError)
        );
    }

    #[test]
    fn charging_caps_at_five_seconds_and_release_freezes_direction() {
        let rules = attack_rules();
        assert_eq!(rules.maximum_charge_ticks(), 300);
        assert_eq!(rules.swing_ticks(), 27);
        assert_eq!(rules.recovery_ticks(), 18);

        let mut app = App::new();
        app.insert_resource(rules)
            .insert_resource(gaze_rules())
            .add_systems(Update, (update_gaze_state, advance_hammer_attacks).chain());
        let player = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".to_owned())),
                AttackIntent::PRESSED,
                GazeIntent::new(1.0, 0.0),
                GazeState::RIGHT,
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
        assert_direction(charging.direction, Vec2::Y);

        *app.world_mut()
            .get_mut::<AttackIntent>(player)
            .expect("Hammerer has attack input") = AttackIntent::RELEASED;
        *app.world_mut()
            .get_mut::<GazeIntent>(player)
            .expect("Hammerer has gaze input") = GazeIntent::new(-1.0, 0.0);
        app.update();
        let released_direction = app
            .world()
            .get::<GazeState>(player)
            .expect("Hammerer has gaze state")
            .direction();
        *app.world_mut()
            .get_mut::<GazeIntent>(player)
            .expect("Hammerer has gaze input") = GazeIntent::new(0.0, -1.0);
        app.update();

        let swinging = app
            .world()
            .get::<HammerAttackState>(player)
            .expect("Hammerer has attack state");
        assert_eq!(swinging.phase, HammerAttackPhase::Swing);
        assert_direction(
            swinging.direction,
            Vec2::new(released_direction.x, released_direction.y),
        );
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
