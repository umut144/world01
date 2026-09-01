use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use crate::{
    advance_dash, advance_hammer_attacks, advance_mage_attacks, apply_damage,
    apply_hammer_strike_damage, apply_mage_beam_damage, constrain_embedded_hammer_reach,
    damage::DamageDealt, expire_mage_beams, finish_mage_cooldowns, integrate_movement,
    tick_status_effects, update_character_life, update_character_orientation, update_exertion,
    update_gaze_direction, update_weapon_aim,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimulationSet {
    /// Advances intent-driven character state for the current tick.
    GameplayStep,
    /// Sits between deciding a velocity and applying it to a position.
    ///
    /// Reserved for world and actor collision; no system runs here yet.
    Collision,
    /// Resolves the consequences of the gameplay step: damage, expiry, and life state.
    Resolution,
}

/// Selects which parts of the simulation step an app is allowed to run.
///
/// Both authorities run the identical gameplay step. Only the server resolves
/// damage, so a predicting client cannot invent hits that the server never saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationAuthority {
    /// The authoritative simulation: resolves damage as well.
    Server,
    /// A predicting client: replays the gameplay step without resolving damage.
    Predicted,
}

impl SimulationAuthority {
    const fn resolves_damage(self) -> bool {
        matches!(self, Self::Server)
    }
}

/// Adds the canonical deterministic simulation step to the schedule selected by the app.
///
/// This is the single definition of tick order for both the server and the
/// predicting client; neither app may register gameplay systems of its own.
pub fn add_simulation_step(
    app: &mut App,
    schedule: impl ScheduleLabel + Clone,
    authority: SimulationAuthority,
) {
    app.add_message::<DamageDealt>();
    app.add_systems(
        schedule.clone(),
        (
            update_gaze_direction,
            update_weapon_aim,
            advance_hammer_attacks,
            advance_mage_attacks,
            tick_status_effects,
            update_exertion,
            integrate_movement,
            advance_dash,
            constrain_embedded_hammer_reach,
            update_character_orientation,
        )
            .chain()
            .in_set(SimulationSet::GameplayStep),
    );
    if authority.resolves_damage() {
        app.add_systems(
            schedule.clone(),
            (apply_hammer_strike_damage, apply_mage_beam_damage)
                .chain()
                .before(apply_damage)
                .in_set(SimulationSet::Resolution),
        );
    }
    app.add_systems(
        schedule.clone(),
        (
            apply_damage,
            expire_mage_beams,
            finish_mage_cooldowns,
            update_character_life,
        )
            .chain()
            .in_set(SimulationSet::Resolution),
    );
    app.configure_sets(
        schedule.clone(),
        SimulationSet::Collision
            .after(update_exertion)
            .before(integrate_movement),
    );
    app.configure_sets(
        schedule,
        SimulationSet::Resolution.after(SimulationSet::GameplayStep),
    );
}
