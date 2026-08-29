use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct StatusEffectState {
    pub knockdowned_ticks: u32,
    pub stunned_ticks: u32,
    pub silenced_ticks: u32,
    pub disarmed_ticks: u32,
    pub rooted_ticks: u32,
}

impl StatusEffectState {
    pub fn blocks_all_input(self) -> bool {
        self.knockdowned_ticks > 0 || self.stunned_ticks > 0
    }

    pub fn blocks_action_buttons(self) -> bool {
        self.blocks_all_input() || self.disarmed_ticks > 0
    }

    pub fn blocks_movement(self) -> bool {
        self.blocks_all_input() || self.rooted_ticks > 0
    }

    pub fn tick(&mut self) {
        self.knockdowned_ticks = self.knockdowned_ticks.saturating_sub(1);
        self.stunned_ticks = self.stunned_ticks.saturating_sub(1);
        self.silenced_ticks = self.silenced_ticks.saturating_sub(1);
        self.disarmed_ticks = self.disarmed_ticks.saturating_sub(1);
        self.rooted_ticks = self.rooted_ticks.saturating_sub(1);
    }
}
