use bevy::prelude::*;
use game01_network::{apply_tick_player_input, client_input_timeline_synced};
use game01_simulation::{move_players, update_character_orientation};

pub struct ClientPredictionPlugin;

impl Plugin for ClientPredictionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                apply_tick_player_input,
                (move_players, update_character_orientation),
            )
                .chain()
                .run_if(client_input_timeline_synced),
        );
    }
}
