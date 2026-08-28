//! Lightyear-specific protocol and transport boundary.

pub mod protocol;

#[cfg(feature = "client")]
pub mod client_transport;
#[cfg(feature = "server")]
pub mod server_transport;

#[cfg(feature = "client")]
pub use client_transport::{
    Client, ClientPlayerInput, ClientPositionCorrection, RemotePositionExtrapolation,
    client_input_timeline_synced, configure_client, connect_client,
};
pub use protocol::{
    JoinChannel, JoinRequest, MAX_CLIENTS, NETWORK_SIMULATION_ENV, NetworkSimulationProfile,
    SERVER_ADDR, apply_tick_player_input,
};
#[cfg(feature = "server")]
pub use server_transport::{
    ConnectionRegistry, ServerJoinRequest, ServerNetworkSet, configure_replicated_player,
    configure_server,
};
