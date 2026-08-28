//! Lightyear-specific protocol and transport boundary.

mod protocol;

#[cfg(feature = "client")]
mod client_transport;
#[cfg(feature = "server")]
mod server_transport;

#[cfg(feature = "client")]
pub use client_transport::{
    Client, ClientPlayerInput, ClientPositionCorrection, RemotePositionExtrapolation,
    client_input_timeline_synced, configure_client, connect_client,
};
pub use protocol::{
    MAX_CLIENTS, NETWORK_SIMULATION_ENV, NetworkSimulationProfile, apply_tick_player_input,
};
#[cfg(feature = "server")]
pub use server_transport::{
    ServerJoinRequest, ServerNetworkSet, configure_replicated_player, configure_server,
};
