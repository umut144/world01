use bevy::prelude::*;
use world01_network::{apply_tick_player_input, client_input_timeline_synced};
use world01_simulation::{
    SimulationAuthority, SimulationSet, WorldNavigation, add_simulation_step,
    add_world_runtime_rebuild,
};

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
        add_simulation_step(app, FixedUpdate, SimulationAuthority::Predicted);
        add_world_runtime_rebuild(app, FixedUpdate, WorldNavigation::Absent);
        app.configure_sets(
            FixedUpdate,
            (PredictionSet::PrepareInput, SimulationSet::GameplayStep)
                .chain()
                .run_if(client_input_timeline_synced),
        );
    }
}
