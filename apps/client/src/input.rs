use bevy::prelude::*;
use game01_network::ClientMovementInput;
use game01_world_data::MovementIntent;

use crate::controller::ControllerInput;

const CONTROLLER_STICK_DEADZONE: f32 = 0.15;

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct LocalGaze(pub Vec2);

pub fn collect_movement_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut controller_input: NonSendMut<ControllerInput>,
    mut input: ResMut<ClientMovementInput>,
) {
    let keyboard_direction = Vec2::new(
        axis(&keyboard, KeyCode::KeyD, KeyCode::KeyA),
        axis(&keyboard, KeyCode::KeyW, KeyCode::KeyS),
    );
    let direction = movement_direction(keyboard_direction, controller_input.left_stick());
    input.0 = MovementIntent::new(direction.x, direction.y);
}

pub fn collect_gaze_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut local_players: Query<&mut LocalGaze, With<MovementIntent>>,
) {
    let direction = Vec2::new(
        axis(&keyboard, KeyCode::KeyL, KeyCode::KeyJ),
        axis(&keyboard, KeyCode::KeyI, KeyCode::KeyK),
    )
    .normalize_or_zero();

    for mut gaze in &mut local_players {
        gaze.0 = direction;
    }
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
