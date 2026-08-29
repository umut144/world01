use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use crate::{
    advance_hammer_attacks, constrain_embedded_hammer_reach, update_character_orientation,
    update_gaze_direction, update_locomotion, update_weapon_aim,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimulationSet {
    GameplayStep,
}

/// Adds the canonical deterministic gameplay step to the schedule selected by the app.
pub fn add_simulation_step(app: &mut App, schedule: impl ScheduleLabel) {
    app.add_systems(
        schedule,
        (
            update_gaze_direction,
            update_weapon_aim,
            advance_hammer_attacks,
            update_locomotion,
            constrain_embedded_hammer_reach,
            update_character_orientation,
        )
            .chain()
            .in_set(SimulationSet::GameplayStep),
    );
}
