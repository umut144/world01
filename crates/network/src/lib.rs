//! Lightyear-specific protocol and transport boundary.

mod protocol;

#[cfg(feature = "client")]
mod client_transport;
#[cfg(feature = "server")]
mod server_transport;

#[cfg(feature = "client")]
pub use client_transport::{
    Client, ClientPlayerInput, ClientPositionCorrection, ClientWorldTemplateDebugRequest,
    RemotePositionExtrapolation, client_input_timeline_synced, configure_client,
    configure_client_world_state, connect_client,
};
pub use protocol::{
    MAX_CLIENTS, NETWORK_SIMULATION_ENV, NetworkSimulationProfile, WorldTemplateDebugPreset,
    apply_tick_player_input,
};
#[cfg(feature = "server")]
pub use server_transport::{
    ServerJoinRequest, ServerNetworkSet, ServerWorldTemplateDebugRequest,
    configure_replicated_destructible_prop, configure_replicated_player,
    configure_replicated_world_state, configure_server,
};
