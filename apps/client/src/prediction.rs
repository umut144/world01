use std::marker::PhantomData;

use bevy::prelude::*;
use world01_network::{apply_tick_player_input, client_input_timeline_synced};
use world01_simulation::{
    SimulationAuthority, SimulationSet, WorldDerivation, WorldNavigation, add_simulation_step,
    add_world_runtime_rebuild,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PredictionSet {
    PrepareInput,
}

/// The predicting half of a game, carried as a type rather than a value.
///
/// A predicting client rebuilds the world exactly as the server does, so it
/// has to derive the same game state from it - the type parameter is what
/// makes the two impossible to configure apart.
pub struct ClientPredictionPlugin<D: WorldDerivation>(PhantomData<fn() -> D>);

impl<D: WorldDerivation> Default for ClientPredictionPlugin<D> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<D: WorldDerivation> Plugin for ClientPredictionPlugin<D> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            apply_tick_player_input.in_set(PredictionSet::PrepareInput),
        );
        add_simulation_step(app, FixedUpdate, SimulationAuthority::Predicted);
        add_world_runtime_rebuild::<D>(app, FixedUpdate, WorldNavigation::Absent);
        app.configure_sets(
            FixedUpdate,
            (PredictionSet::PrepareInput, SimulationSet::GameplayStep)
                .chain()
                .run_if(client_input_timeline_synced),
        );
    }
}
