//! Whether an actor is in a condition to act at all this tick.

use world01_world_data::{CharacterLifeState, StatusEffectState};

/// The two things every gameplay system asks before letting an actor act:
/// which status effects hold it, and whether it is alive.
///
/// Both are optional components, and every caller used to pair them by hand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActorCondition {
    status: StatusEffectState,
    life: CharacterLifeState,
}

impl ActorCondition {
    /// A missing component means the actor is unaffected: no status effects,
    /// and alive.
    pub fn new(status: Option<&StatusEffectState>, life: Option<&CharacterLifeState>) -> Self {
        Self {
            status: status.copied().unwrap_or_default(),
            life: life.copied().unwrap_or_default(),
        }
    }

    pub fn is_alive(self) -> bool {
        self.life.is_alive()
    }

    /// Knocked down, stunned or dead: no input reaches the actor.
    pub fn blocks_all_input(self) -> bool {
        !self.is_alive() || self.status.blocks_all_input()
    }

    /// Additionally disarmed: attacks and abilities are unavailable.
    pub fn blocks_action_buttons(self) -> bool {
        !self.is_alive() || self.status.blocks_action_buttons()
    }

    /// Additionally rooted: the actor cannot move under its own power.
    pub fn blocks_movement(self) -> bool {
        !self.is_alive() || self.status.blocks_movement()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_component_leaves_the_actor_unaffected() {
        let condition = ActorCondition::new(None, None);

        assert!(condition.is_alive());
        assert!(!condition.blocks_all_input());
        assert!(!condition.blocks_action_buttons());
        assert!(!condition.blocks_movement());
    }

    #[test]
    fn death_blocks_everything_a_status_effect_could_block() {
        let condition = ActorCondition::new(None, Some(&CharacterLifeState::Dead));

        assert!(!condition.is_alive());
        assert!(condition.blocks_all_input());
        assert!(condition.blocks_action_buttons());
        assert!(condition.blocks_movement());
    }

    #[test]
    fn a_root_stops_movement_without_stopping_actions() {
        let rooted = StatusEffectState {
            rooted_ticks: 3,
            ..StatusEffectState::default()
        };
        let condition = ActorCondition::new(Some(&rooted), None);

        assert!(condition.blocks_movement());
        assert!(!condition.blocks_action_buttons());
        assert!(!condition.blocks_all_input());
    }
}
