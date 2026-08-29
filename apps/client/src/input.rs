use bevy::prelude::*;
use game01_network::ClientPlayerInput;
use game01_world_data::{AttackIntent, GazeIntent, MovementIntent};

use crate::controller::ControllerInput;

const CONTROLLER_STICK_DEADZONE: f32 = 0.15;

pub fn collect_movement_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut controller_input: NonSendMut<ControllerInput>,
    mut input: ResMut<ClientPlayerInput>,
) {
    let keyboard_direction = Vec2::new(
        axis(&keyboard, KeyCode::KeyD, KeyCode::KeyA),
        axis(&keyboard, KeyCode::KeyW, KeyCode::KeyS),
    );
    let direction = movement_direction(keyboard_direction, controller_input.left_stick());
    input.0.movement = MovementIntent::new(direction.x, direction.y);
}

pub fn collect_gaze_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    local_players: Query<(), With<MovementIntent>>,
    mut controller_input: NonSendMut<ControllerInput>,
    mut input: ResMut<ClientPlayerInput>,
) {
    if local_players.is_empty() {
        return;
    }
    let keyboard_direction = Vec2::new(
        axis(&keyboard, KeyCode::KeyL, KeyCode::KeyJ),
        axis(&keyboard, KeyCode::KeyI, KeyCode::KeyK),
    );
    let direction = controller_input
        .right_stick()
        .and_then(controller_stick_direction)
        .unwrap_or_else(|| keyboard_direction.normalize_or_zero());
    input.0.gaze = gaze_intent(direction);
}

pub fn collect_attack_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut controller_input: NonSendMut<ControllerInput>,
    mut input: ResMut<ClientPlayerInput>,
) {
    let pressed = attack_pressed(
        keyboard.pressed(KeyCode::Space),
        controller_input.right_trigger_pressed(),
    );
    input.0.attack = AttackIntent::new(pressed);
}

fn attack_pressed(space: bool, right_trigger: bool) -> bool {
    space || right_trigger
}

fn gaze_intent(direction: Vec2) -> GazeIntent {
    let direction = direction.normalize_or_zero();
    GazeIntent::new(direction.x, direction.y)
}

fn axis(keyboard: &ButtonInput<KeyCode>, positive: KeyCode, negative: KeyCode) -> f32 {
    f32::from(keyboard.pressed(positive)) - f32::from(keyboard.pressed(negative))
}

pub(crate) fn movement_direction(
    keyboard_direction: Vec2,
    controller_direction: Option<Vec2>,
) -> Vec2 {
    controller_direction
        .and_then(controller_stick_direction)
        .unwrap_or_else(|| keyboard_direction.normalize_or_zero())
}

pub(crate) fn controller_stick_direction(stick: Vec2) -> Option<Vec2> {
    if !stick.is_finite() {
        return None;
    }

    let magnitude = stick.length();
    if magnitude <= CONTROLLER_STICK_DEADZONE {
        return None;
    }

    let scaled_magnitude =
        ((magnitude - CONTROLLER_STICK_DEADZONE) / (1.0 - CONTROLLER_STICK_DEADZONE)).min(1.0);
    Some(stick.normalize_or_zero() * scaled_magnitude)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_direction_overrides_keyboard_direction() {
        assert_eq!(movement_direction(Vec2::X, Some(Vec2::Y)), Vec2::Y);
    }

    #[test]
    fn neutral_controller_direction_uses_keyboard_fallback() {
        assert_eq!(
            movement_direction(Vec2::new(1.0, 1.0), Some(Vec2::ZERO)),
            Vec2::new(1.0, 1.0).normalize(),
        );
    }

    #[test]
    fn controller_deadzone_blocks_small_stick_drift() {
        assert_eq!(controller_stick_direction(Vec2::new(0.15, 0.0)), None);
    }

    #[test]
    fn controller_stick_preserves_partial_movement_strength() {
        let direction = controller_stick_direction(Vec2::new(0.575, 0.0))
            .expect("stick outside the deadzone produces movement");
        assert!((direction.x - 0.5).abs() < f32::EPSILON);
        assert_eq!(direction.y, 0.0);
    }

    #[test]
    fn gaze_input_is_active_only_while_a_direction_is_held() {
        let left = gaze_intent(-Vec2::X);

        assert_eq!(left, GazeIntent::new(-1.0, 0.0));
        assert_eq!(gaze_intent(Vec2::ZERO), GazeIntent::ZERO);
    }

    #[test]
    fn gaze_input_normalizes_diagonals() {
        let diagonal = gaze_intent(Vec2::new(1.0, 1.0));
        let expected = 1.0 / 2.0_f32.sqrt();

        assert_eq!(diagonal, GazeIntent::new(expected, expected));
    }

    #[test]
    fn attack_accepts_space_and_right_trigger_equally() {
        assert!(attack_pressed(true, false));
        assert!(attack_pressed(false, true));
        assert!(!attack_pressed(false, false));
    }
}
