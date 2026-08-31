//! Status effect timers, advanced once per tick for every actor that has them.

use bevy::prelude::Query;
use world01_world_data::StatusEffectState;

/// Counts every status effect down by one tick.
///
/// This used to be the first statement of the locomotion system, which tied a
/// stunned actor's recovery to it also having a full set of movement
/// components. Recovery is its own concern and belongs to every actor that can
/// be affected at all.
pub fn tick_status_effects(mut actors: Query<&mut StatusEffectState>) {
    for mut status in &mut actors {
        status.tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Update};

    #[test]
    fn every_timer_counts_down_and_stops_at_zero() {
        let mut app = App::new();
        app.add_systems(Update, tick_status_effects);
        let actor = app
            .world_mut()
            .spawn(StatusEffectState {
                knockdowned_ticks: 2,
                stunned_ticks: 1,
                silenced_ticks: 0,
                disarmed_ticks: 1,
                rooted_ticks: 0,
            })
            .id();

        app.update();

        let status = app.world().get::<StatusEffectState>(actor).unwrap();
        assert_eq!(status.knockdowned_ticks, 1);
        assert_eq!(status.stunned_ticks, 0);
        assert_eq!(status.silenced_ticks, 0);
        assert!(status.blocks_all_input());

        for _ in 0..8 {
            app.update();
        }

        let status = app.world().get::<StatusEffectState>(actor).unwrap();
        assert_eq!(*status, StatusEffectState::default());
        assert!(!status.blocks_all_input());
    }

    #[test]
    fn an_actor_recovers_without_any_movement_components() {
        let mut app = App::new();
        app.add_systems(Update, tick_status_effects);
        let actor = app
            .world_mut()
            .spawn(StatusEffectState {
                stunned_ticks: 1,
                ..StatusEffectState::default()
            })
            .id();

        app.update();

        assert!(
            !app.world()
                .get::<StatusEffectState>(actor)
                .unwrap()
                .blocks_all_input()
        );
    }
}
