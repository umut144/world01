use bevy::prelude::*;
use game01_network::{apply_tick_player_input, client_input_timeline_synced};
use game01_simulation::{
    advance_hammer_attacks, move_players, update_character_orientation, update_gaze_direction,
    update_weapon_aim,
};

pub struct ClientPredictionPlugin;

impl Plugin for ClientPredictionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                apply_tick_player_input,
                (
                    update_gaze_direction,
                    update_weapon_aim,
                    advance_hammer_attacks,
                    move_players,
                    update_character_orientation,
                )
                    .chain(),
            )
                .chain()
                .run_if(client_input_timeline_synced),
        );
    }
}
