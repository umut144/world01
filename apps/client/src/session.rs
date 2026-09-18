use std::time::Duration;

use bevy::prelude::*;
use world01_network::{NetworkSimulationProfile, configure_client, configure_client_world_state};
use world01_world_data::{CharacterId, TeamId};

pub struct ClientSessionPlugin {
    pub client_id: u64,
    pub team: Option<TeamId>,
    pub tick_duration: Duration,
    pub snapshot_interval: Duration,
    pub remote_interpolation_ratio: f32,
    pub network_simulation: NetworkSimulationProfile,
}

impl Plugin for ClientSessionPlugin {
    fn build(&self, app: &mut App) {
        configure_client(app, self.tick_duration, self.snapshot_interval);
        configure_client_world_state(app);
        app.insert_resource(Time::<Fixed>::from_duration(self.tick_duration))
            .init_state::<ClientScreen>()
            .insert_resource(ClientSession {
                client_id: self.client_id,
                team: self.team,
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
    /// The side this client was started with, passed on unchanged at join.
    pub team: Option<TeamId>,
    pub remote_interpolation_ratio: f32,
    pub network_simulation: NetworkSimulationProfile,
    pub selected: Option<CharacterId>,
    pub joining: bool,
}
