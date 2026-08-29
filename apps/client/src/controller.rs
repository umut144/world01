use bevy::prelude::Vec2;
use gilrs::{Axis, Button, Gilrs};

pub struct ControllerInput {
    gilrs: Gilrs,
}

impl ControllerInput {
    pub fn new() -> Result<Self, gilrs::Error> {
        Ok(Self {
            gilrs: Gilrs::new()?,
        })
    }

    pub fn left_stick(&mut self) -> Option<Vec2> {
        while self.gilrs.next_event().is_some() {}

        self.gilrs
            .gamepads()
            .map(|(_, gamepad)| {
                Vec2::new(
                    gamepad.value(Axis::LeftStickX),
                    gamepad.value(Axis::LeftStickY),
                )
            })
            .max_by(|left, right| left.length_squared().total_cmp(&right.length_squared()))
    }

    pub fn right_stick(&mut self) -> Option<Vec2> {
        while self.gilrs.next_event().is_some() {}

        self.gilrs
            .gamepads()
            .map(|(_, gamepad)| {
                Vec2::new(
                    gamepad.value(Axis::RightStickX),
                    gamepad.value(Axis::RightStickY),
                )
            })
            .max_by(|left, right| left.length_squared().total_cmp(&right.length_squared()))
    }

    pub fn right_trigger_pressed(&mut self) -> bool {
        while self.gilrs.next_event().is_some() {}

        self.gilrs
            .gamepads()
            .any(|(_, gamepad)| gamepad.is_pressed(Button::RightTrigger2))
    }

    pub fn run_pressed(&mut self) -> bool {
        self.button_pressed(Button::West)
    }

    pub fn dash_pressed(&mut self) -> bool {
        self.button_pressed(Button::East)
    }

    fn button_pressed(&mut self, button: Button) -> bool {
        while self.gilrs.next_event().is_some() {}

        self.gilrs
            .gamepads()
            .any(|(_, gamepad)| gamepad.is_pressed(button))
    }
}
