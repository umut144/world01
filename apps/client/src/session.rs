use std::time::Duration;

use bevy::prelude::*;
use game01_network::{NetworkSimulationProfile, configure_client};
use game01_world_data::CharacterId;

pub struct ClientSessionPlugin {
    pub client_id: u64,
    pub tick_duration: Duration,
    pub snapshot_interval: Duration,
    pub remote_interpolation_ratio: f32,
    pub network_simulation: NetworkSimulationProfile,
}

impl Plugin for ClientSessionPlugin {
    fn build(&self, app: &mut App) {
        configure_client(app, self.tick_duration, self.snapshot_interval);
        app.insert_resource(Time::<Fixed>::from_duration(self.tick_duration))
            .init_state::<ClientScreen>()
            .insert_resource(ClientSession {
                client_id: self.client_id,
                remote_interpolation_ratio: self.remote_interpolation_ratio,
                network_simulation: self.network_simulation,
                selected: None,
                joining: false,
            });
    }
}

#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub(crate) enum ClientScreen {
    #[default]
    CharacterSelection,
    InGame,
}

#[derive(Resource)]
pub(crate) struct ClientSession {
    pub client_id: u64,
    pub remote_interpolation_ratio: f32,
    pub network_simulation: NetworkSimulationProfile,
    pub selected: Option<CharacterId>,
    pub joining: bool,
}
