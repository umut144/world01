use bevy::prelude::Vec2;
use gilrs::{Axis, Gilrs};

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
}
