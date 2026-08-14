use bevy::prelude::*;
use game01_network::{apply_tick_movement_intents, client_input_timeline_synced};
use game01_simulation::move_players;

pub struct ClientPredictionPlugin;

impl Plugin for ClientPredictionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (apply_tick_movement_intents, move_players)
                .chain()
                .run_if(client_input_timeline_synced),
        );
    }
}
