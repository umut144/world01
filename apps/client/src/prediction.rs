use bevy::prelude::*;
use game01_network::{apply_tick_player_input, client_input_timeline_synced};
use game01_simulation::{SimulationSet, add_simulation_step, update_character_life};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PredictionSet {
    PrepareInput,
}

pub struct ClientPredictionPlugin;

impl Plugin for ClientPredictionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            apply_tick_player_input.in_set(PredictionSet::PrepareInput),
        );
        add_simulation_step(app, FixedUpdate);
        app.configure_sets(
            FixedUpdate,
            (PredictionSet::PrepareInput, SimulationSet::GameplayStep)
                .chain()
                .run_if(client_input_timeline_synced),
        );
        app.add_systems(
            FixedUpdate,
            update_character_life.after(SimulationSet::GameplayStep),
        );
    }
}
