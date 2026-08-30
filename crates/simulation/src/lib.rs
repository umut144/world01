//! Input-, transport-, and presentation-independent game simulation.

pub mod aim;
pub mod combat;
pub mod life;
pub mod locomotion;
pub mod mass;
pub mod movement;
pub mod respawn;
mod schedule;

pub use aim::{WeaponAimConfigError, WeaponAimRules, update_gaze_direction, update_weapon_aim};
pub use combat::hammer::{
    HammerAttackConfigError, HammerAttackRules, HammerStrikeConfigError, HammerStrikeRules,
    advance_hammer_attacks, apply_hammer_strike_damage, constrain_embedded_hammer_reach,
};
pub use life::{CharacterLifeConfigError, CharacterLifeRules, update_character_life};
pub use locomotion::{LocomotionConfigError, LocomotionRules, update_locomotion};
pub use mass::{CharacterMassCatalog, MassModelError};
pub use movement::{MovementConfigError, MovementStep, move_players, update_character_orientation};
pub use respawn::{RespawnPlayer, choose_respawn_position};
pub use schedule::{SimulationSet, add_simulation_step};

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;
    use bevy::prelude::{App, IntoScheduleConfigs, Update, Vec2};
    use world01_configs::load_embedded;
    use world01_content::{CharacterMassGeometryCatalog, HammerCombatGeometry, RuntimeContent};
    use world01_world_data::{
        AttackIntent, BodyFacing, CharacterId, CharacterMass, GazeDirection, GazeIntent,
        HammerAttackPhase, HammerAttackState, MovementDirection, MovementIntent, Position,
        SelectedCharacter, WeaponAimState, WeaponTurnDirection,
    };

    use crate::combat::hammer::constrain_embedded_position;

    const EPSILON: f32 = 0.000_01;

    fn movement_step() -> MovementStep {
        let config = load_embedded().expect("embedded design configuration parses");
        MovementStep::from_design(&config).expect("embedded movement configuration is valid")
    }

    fn attack_rules() -> HammerAttackRules {
        let config = load_embedded().expect("embedded design configuration parses");
        let design = world01_design::load_embedded().expect("embedded game design parses");
        HammerAttackRules::from_design(config.simulation.ticks_per_second, &design.hammer)
            .expect("embedded Hammer attack configuration is valid")
    }

    fn weapon_aim_rules() -> WeaponAimRules {
        let config = load_embedded().expect("embedded design configuration parses");
        WeaponAimRules::from_design(&config).expect("embedded weapon aim configuration is valid")
    }

    fn hammer_geometry() -> HammerCombatGeometry {
        let content = RuntimeContent::load_embedded().expect("embedded runtime content is valid");
        let design = world01_design::load_embedded().expect("embedded game design parses");
        HammerCombatGeometry::from_content(&content, &design.hammer.attack_components)
            .expect("synced Hammer manifests define valid combat geometry")
    }

    fn mass_catalog() -> CharacterMassCatalog {
        let config = load_embedded().expect("embedded design configuration parses");
        let content = RuntimeContent::load_embedded().expect("embedded runtime content is valid");
        let design = world01_design::load_embedded().expect("embedded game design parses");
        let geometry = CharacterMassGeometryCatalog::from_content(&content, &design.mass)
            .expect("mass design covers embedded content");
        CharacterMassCatalog::from_geometry(&config, &geometry)
            .expect("embedded mass configuration is valid")
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
    fn cardinal_movement_uses_character_speed_and_tick_rate() {
        let displacement = movement_step().displacement(MovementIntent::new(1.0, 0.0), 0.6);

        assert!((displacement.x - 0.6 / 60.0).abs() < EPSILON);
        assert_eq!(displacement.y, 0.0);
    }

    #[test]
    fn diagonal_movement_is_normalized() {
        let step = movement_step();
        let cardinal_distance = step
            .displacement(MovementIntent::new(1.0, 0.0), 0.6)
            .length();
        let diagonal_distance = step
            .displacement(MovementIntent::new(1.0, 1.0), 0.6)
            .length();

        assert!((cardinal_distance - diagonal_distance).abs() < EPSILON);
    }

    #[test]
    fn intent_above_unit_length_is_clamped() {
        let step = movement_step();
        let unit_distance = step
            .displacement(MovementIntent::new(1.0, 0.0), 0.6)
            .length();
        let excessive_distance = step
            .displacement(MovementIntent::new(10.0, 0.0), 0.6)
            .length();

        assert!((unit_distance - excessive_distance).abs() < EPSILON);
    }

    #[test]
    fn invalid_intent_does_not_move() {
        let displacement = movement_step().displacement(MovementIntent::new(f32::NAN, 1.0), 0.6);

        assert_eq!(displacement, Vec2::ZERO);
    }

    #[test]
    fn sixty_ticks_cover_a_hammerers_normal_distance() {
        let step = movement_step();
        let mut position = Vec2::ZERO;

        for _ in 0..60 {
            position += step.displacement(MovementIntent::new(0.0, 1.0), 0.6);
        }

        assert!((position.y - 0.6).abs() < EPSILON);
    }

    #[test]
    fn movement_system_updates_authoritative_position() {
        let mut app = App::new();
        app.insert_resource(movement_step())
            .insert_resource(attack_rules())
            .insert_resource(hammer_geometry())
            .add_systems(Update, move_players);
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(-1.0, 0.0),
                CharacterMass::new(2.0, 0.0, 2.0, 0.6),
                Position::ZERO,
            ))
            .id();

        app.update();

        let position = app
            .world()
            .get::<Position>(player)
            .expect("spawned test player has a Position");
        assert!((position.x + 0.6 / 60.0).abs() < EPSILON);
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
    fn gaze_changes_immediately_and_retains_its_direction_after_release() {
        let mut app = App::new();
        app.add_systems(Update, update_gaze_direction);
        let player = app
            .world_mut()
            .spawn((GazeIntent::new(0.0, 1.0), GazeDirection::RIGHT))
            .id();

        app.update();
        assert_direction(*app.world().get::<GazeDirection>(player).unwrap(), Vec2::Y);

        *app.world_mut().get_mut::<GazeIntent>(player).unwrap() = GazeIntent::ZERO;
        app.update();
        assert_direction(*app.world().get::<GazeDirection>(player).unwrap(), Vec2::Y);
    }

    #[test]
    fn weapon_aim_moves_only_while_gaze_input_is_held() {
        let mut app = App::new();
        app.insert_resource(weapon_aim_rules())
            .add_systems(Update, (update_gaze_direction, update_weapon_aim).chain());
        let player = app
            .world_mut()
            .spawn((
                character("hammerer"),
                GazeIntent::new(0.0, 1.0),
                GazeDirection::RIGHT,
                WeaponAimState::RIGHT,
            ))
            .id();

        for _ in 0..30 {
            app.update();
        }
        assert_direction(*app.world().get::<GazeDirection>(player).unwrap(), Vec2::Y);
        let stopped = *app.world().get::<WeaponAimState>(player).unwrap();
        assert!((stopped.angle_radians - 30.0_f32.to_radians()).abs() < EPSILON);

        *app.world_mut().get_mut::<GazeIntent>(player).unwrap() = GazeIntent::ZERO;
        for _ in 0..60 {
            app.update();
        }
        assert_eq!(app.world().get::<WeaponAimState>(player), Some(&stopped));
        assert_direction(*app.world().get::<GazeDirection>(player).unwrap(), Vec2::Y);
    }

    #[test]
    fn opposite_target_continues_the_last_turn_direction() {
        let mut app = App::new();
        app.insert_resource(weapon_aim_rules())
            .add_systems(Update, update_weapon_aim);
        let clockwise = app
            .world_mut()
            .spawn((
                character("mage"),
                GazeIntent::new(-1.0, 0.0),
                GazeDirection::new(-1.0, 0.0),
                WeaponAimState::RIGHT,
            ))
            .id();
        let counterclockwise = app
            .world_mut()
            .spawn((
                character("rogue"),
                GazeIntent::new(-1.0, 0.0),
                GazeDirection::new(-1.0, 0.0),
                WeaponAimState::new(0.0, WeaponTurnDirection::CounterClockwise),
            ))
            .id();

        app.update();

        let clockwise = app.world().get::<WeaponAimState>(clockwise).unwrap();
        assert_eq!(
            clockwise.last_turn_direction,
            WeaponTurnDirection::Clockwise
        );
        assert!((clockwise.angle_radians - (2.0 * PI - PI / 180.0)).abs() < EPSILON);
        let counterclockwise = app.world().get::<WeaponAimState>(counterclockwise).unwrap();
        assert_eq!(
            counterclockwise.last_turn_direction,
            WeaponTurnDirection::CounterClockwise
        );
        assert!((counterclockwise.angle_radians - PI / 180.0).abs() < EPSILON);
    }

    #[test]
    fn character_override_changes_only_that_characters_weapon_aim_speed() {
        let mut config = load_embedded().expect("embedded design configuration parses");
        config
            .weapon_aim
            .character_degrees_per_second
            .insert("hammerer".to_owned(), 30.0);
        let rules = WeaponAimRules::from_design(&config).expect("weapon aim override is valid");

        assert!((rules.radians_per_tick(&character("wizard")) - PI / 180.0).abs() < EPSILON);
        assert!((rules.radians_per_tick(&character("hammerer")) - PI / 360.0).abs() < EPSILON);
    }

    #[test]
    fn target_within_one_degree_clamps_to_the_exact_angle() {
        let mut app = App::new();
        app.insert_resource(weapon_aim_rules())
            .add_systems(Update, update_weapon_aim);
        let player = app
            .world_mut()
            .spawn((
                character("wizard"),
                GazeIntent::new(0.0, 1.0),
                GazeDirection::new(0.0, 1.0),
                WeaponAimState::new(
                    PI / 2.0 - 0.5_f32.to_radians(),
                    WeaponTurnDirection::CounterClockwise,
                ),
            ))
            .id();

        app.update();

        let state = app.world().get::<WeaponAimState>(player).unwrap();
        assert_eq!(state.angle_radians, PI / 2.0);
        assert_direction(state.direction(), Vec2::Y);
    }

    #[test]
    fn all_eight_held_input_targets_settle_weapon_aim_exactly() {
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
            app.insert_resource(weapon_aim_rules())
                .add_systems(Update, update_weapon_aim);
            let player = app
                .world_mut()
                .spawn((
                    character("wizard"),
                    GazeIntent::new(target.x, target.y),
                    GazeDirection::new(target.x, target.y),
                    WeaponAimState::RIGHT,
                ))
                .id();

            for _ in 0..180 {
                app.update();
            }

            assert_direction(
                app.world()
                    .get::<WeaponAimState>(player)
                    .unwrap()
                    .direction(),
                target.normalize(),
            );
        }
    }

    #[test]
    fn invalid_design_values_are_rejected() {
        let mut zero_tick_rate = load_embedded().expect("embedded design configuration parses");
        zero_tick_rate.simulation.ticks_per_second = 0;

        assert_eq!(
            MovementStep::from_design(&zero_tick_rate),
            Err(MovementConfigError::ZeroTickRate)
        );
        assert_eq!(
            WeaponAimRules::from_design(&zero_tick_rate),
            Err(WeaponAimConfigError)
        );
    }

    #[test]
    fn embedded_mass_model_derives_confirmed_hammerer_and_rogue_speeds() {
        let catalog = mass_catalog();
        let hammerer = catalog
            .character(&CharacterId("hammerer".into()))
            .expect("Hammerer mass is derived");
        let rogue = catalog
            .character(&CharacterId("rogue".into()))
            .expect("Rogue mass is derived");

        assert!((hammerer.body - 2.191_913).abs() < EPSILON);
        assert!((hammerer.equipped_weapon - 4.643_827).abs() < EPSILON);
        assert!((hammerer.movement - hammerer.body).abs() < EPSILON);
        assert!((hammerer.normal_speed_meters_per_second - 0.6).abs() < EPSILON);
        assert!((rogue.normal_speed_meters_per_second - 1.230_552).abs() < EPSILON);
        assert!(
            rogue.normal_speed_meters_per_second > 2.0 * hammerer.normal_speed_meters_per_second
        );
    }

    #[test]
    fn charging_caps_at_five_seconds_and_release_freezes_direction() {
        let rules = attack_rules();
        assert_eq!(rules.maximum_charge_ticks(), 300);
        assert_eq!(rules.swing_ticks(), 69);
        assert_eq!(rules.embedded_ticks(), 120);
        assert_eq!(rules.recovery_ticks(), 60);

        let mut app = App::new();
        app.insert_resource(rules)
            .insert_resource(weapon_aim_rules())
            .insert_resource(hammer_geometry())
            .add_systems(
                Update,
                (
                    update_gaze_direction,
                    update_weapon_aim,
                    advance_hammer_attacks,
                )
                    .chain(),
            );
        let player = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".to_owned())),
                AttackIntent::PRESSED,
                GazeIntent::new(1.0, 0.0),
                GazeDirection::RIGHT,
                WeaponAimState::RIGHT,
                HammerAttackState::IDLE,
                Position::ZERO,
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
            .get::<WeaponAimState>(player)
            .expect("Hammerer has weapon aim state")
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
    }

    #[test]
    fn attack_uses_weapon_aim_while_gaze_remains_independent() {
        let mut app = App::new();
        app.insert_resource(attack_rules())
            .insert_resource(weapon_aim_rules())
            .insert_resource(hammer_geometry())
            .add_systems(
                Update,
                (
                    update_gaze_direction,
                    update_weapon_aim,
                    advance_hammer_attacks,
                )
                    .chain(),
            );
        let player = app
            .world_mut()
            .spawn((
                character("hammerer"),
                AttackIntent::RELEASED,
                GazeIntent::new(0.0, 1.0),
                GazeDirection::RIGHT,
                WeaponAimState::RIGHT,
                HammerAttackState::IDLE,
                Position::ZERO,
            ))
            .id();

        for _ in 0..29 {
            app.update();
        }
        *app.world_mut().get_mut::<AttackIntent>(player).unwrap() = AttackIntent::PRESSED;
        app.update();

        let gaze = *app.world().get::<GazeDirection>(player).unwrap();
        let weapon_aim = *app.world().get::<WeaponAimState>(player).unwrap();
        let attack = *app.world().get::<HammerAttackState>(player).unwrap();
        assert_direction(gaze, Vec2::Y);
        assert!((weapon_aim.angle_radians - 30.0_f32.to_radians()).abs() < EPSILON);
        assert_direction(attack.direction, Vec2::from_angle(30.0_f32.to_radians()));
        let visible_hammer_angle = (weapon_aim.angle_radians + PI).rem_euclid(2.0 * PI);
        assert!((visible_hammer_angle - 210.0_f32.to_radians()).abs() < EPSILON);

        *app.world_mut().get_mut::<GazeIntent>(player).unwrap() = GazeIntent::ZERO;
        *app.world_mut().get_mut::<AttackIntent>(player).unwrap() = AttackIntent::RELEASED;
        app.update();

        let swinging = app.world().get::<HammerAttackState>(player).unwrap();
        assert_eq!(swinging.phase, HammerAttackPhase::Swing);
        assert_direction(swinging.direction, Vec2::from_angle(30.0_f32.to_radians()));
        assert_direction(*app.world().get::<GazeDirection>(player).unwrap(), Vec2::Y);
    }

    #[test]
    fn charging_retains_normal_authoritative_movement() {
        let mut app = App::new();
        app.insert_resource(movement_step())
            .insert_resource(attack_rules())
            .insert_resource(hammer_geometry())
            .add_systems(Update, move_players);
        let player = app
            .world_mut()
            .spawn((
                MovementIntent::new(1.0, 0.0),
                CharacterMass::new(2.191_913, 4.643_827, 2.191_913, 0.6),
                HammerAttackState {
                    phase: HammerAttackPhase::Charging,
                    ..HammerAttackState::IDLE
                },
                Position::ZERO,
            ))
            .id();

        app.update();
        let charging_position = app
            .world()
            .get::<Position>(player)
            .expect("charging Hammerer retains Position");
        assert!((charging_position.x - 0.6 / 60.0).abs() < EPSILON);

        app.world_mut()
            .get_mut::<HammerAttackState>(player)
            .expect("Hammerer has attack state")
            .phase = HammerAttackPhase::Swing;
        app.update();
        let position = app
            .world()
            .get::<Position>(player)
            .expect("Hammerer has Position");
        assert!((position.x - 2.0 * 0.6 / 60.0).abs() < EPSILON);
    }

    #[test]
    fn release_time_selects_the_authored_attack_radius() {
        let rules = attack_rules();
        let geometry = hammer_geometry();

        for charge_ticks in [0, 60, 120, 300] {
            let mut app = App::new();
            app.insert_resource(rules)
                .insert_resource(geometry.clone())
                .add_systems(Update, advance_hammer_attacks);
            let player_position = Position::new(2.0, -3.0);
            let player = app
                .world_mut()
                .spawn((
                    character("hammerer"),
                    AttackIntent::RELEASED,
                    WeaponAimState::RIGHT,
                    player_position,
                    HammerAttackState {
                        phase: HammerAttackPhase::Swing,
                        direction: GazeDirection::RIGHT,
                        phase_ticks: rules.swing_ticks() - 1,
                        charge_ticks,
                        impact_point: Position::ZERO,
                    },
                ))
                .id();

            app.update();

            let state = *app.world().get::<HammerAttackState>(player).unwrap();
            let socket = Vec2::new(player_position.x, player_position.y) + geometry.socket_offset();
            let impact = Vec2::new(state.impact_point.x, state.impact_point.y);
            let expected_radius = geometry.attack_radius(rules.grip_progress(charge_ticks as f32));
            assert_eq!(state.phase, HammerAttackPhase::Embedded);
            assert!((impact.distance(socket) - expected_radius).abs() < EPSILON);
        }

        let primary = geometry.attack_radius(0.0);
        let halfway = geometry.attack_radius(0.5);
        let secondary = geometry.attack_radius(1.0);
        assert!((halfway - (primary + secondary) * 0.5).abs() < EPSILON);
        assert_eq!(rules.grip_progress(120.0), 1.0);
        assert_eq!(rules.grip_progress(300.0), 1.0);
    }

    #[test]
    fn identical_release_state_produces_the_same_headless_impact() {
        let rules = attack_rules();
        let geometry = hammer_geometry();
        let resolve_impact = || {
            let mut app = App::new();
            app.insert_resource(rules)
                .insert_resource(geometry.clone())
                .add_systems(Update, advance_hammer_attacks);
            let player = app
                .world_mut()
                .spawn((
                    character("hammerer"),
                    AttackIntent::RELEASED,
                    WeaponAimState::RIGHT,
                    Position::new(-2.5, 1.75),
                    HammerAttackState {
                        phase: HammerAttackPhase::Swing,
                        direction: GazeDirection::new(0.6, 0.8),
                        phase_ticks: rules.swing_ticks() - 1,
                        charge_ticks: 60,
                        impact_point: Position::ZERO,
                    },
                ))
                .id();

            app.update();
            *app.world()
                .get::<HammerAttackState>(player)
                .expect("Hammerer retains its authoritative attack state")
        };

        let first = resolve_impact();
        let second = resolve_impact();

        assert_eq!(first.phase, HammerAttackPhase::Embedded);
        assert_eq!(first, second);
    }

    #[test]
    fn embedded_and_recovery_phases_use_the_configured_durations() {
        let rules = attack_rules();
        let mut app = App::new();
        app.insert_resource(rules)
            .insert_resource(hammer_geometry())
            .add_systems(Update, advance_hammer_attacks);
        let player = app
            .world_mut()
            .spawn((
                character("hammerer"),
                AttackIntent::RELEASED,
                WeaponAimState::RIGHT,
                Position::ZERO,
                HammerAttackState {
                    phase: HammerAttackPhase::Embedded,
                    direction: GazeDirection::RIGHT,
                    phase_ticks: 0,
                    charge_ticks: 120,
                    impact_point: Position::new(1.0, 0.0),
                },
            ))
            .id();

        for _ in 0..rules.embedded_ticks() - 1 {
            app.update();
        }
        assert_eq!(
            app.world().get::<HammerAttackState>(player).unwrap().phase,
            HammerAttackPhase::Embedded
        );
        app.update();
        assert_eq!(
            app.world().get::<HammerAttackState>(player).unwrap().phase,
            HammerAttackPhase::Recovery
        );

        for _ in 0..rules.recovery_ticks() - 1 {
            app.update();
        }
        assert_eq!(
            app.world().get::<HammerAttackState>(player).unwrap().phase,
            HammerAttackPhase::Recovery
        );
        app.update();
        assert_eq!(
            *app.world().get::<HammerAttackState>(player).unwrap(),
            HammerAttackState::IDLE
        );
    }

    #[test]
    fn embedded_reach_projects_only_positions_outside_the_authored_limit() {
        let geometry = hammer_geometry();
        let planted_head = Vec2::new(4.0, -2.0);
        let socket_offset = geometry.socket_offset();
        let reach = geometry.maximum_reach();
        let player_for_socket = |socket: Vec2| socket - socket_offset;

        let inside = player_for_socket(planted_head + Vec2::X * reach * 0.5);
        assert_eq!(
            constrain_embedded_position(inside, planted_head, socket_offset, reach),
            inside
        );

        let outside = player_for_socket(planted_head + Vec2::new(3.0, 4.0) * reach);
        let constrained = constrain_embedded_position(outside, planted_head, socket_offset, reach);
        let constrained_socket = constrained + socket_offset;
        assert!((constrained_socket.distance(planted_head) - reach).abs() < EPSILON);
        assert_direction(
            GazeDirection::new(
                (constrained_socket - planted_head).normalize().x,
                (constrained_socket - planted_head).normalize().y,
            ),
            Vec2::new(3.0, 4.0).normalize(),
        );
    }
}
