//! Lightyear-specific protocol and transport boundary.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use bevy::{
    log::{info, warn},
    prelude::*,
};
use lightyear::{
    connection::client::Disconnecting,
    netcode::Key,
    prelude::{client::*, server::*},
};
use lightyear::{prelude::server::ServerUdpIo, prelude::*};

pub const MAX_CLIENTS: usize = 5;
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);

const PROTOCOL_ID: u64 = 0x47_41_4d_45_30_31;

#[derive(Resource, Debug, Clone, Copy)]
struct ClientNetworkConfig {
    client_id: u64,
}

#[derive(Resource, Debug, Default)]
pub struct ConnectionRegistry {
    clients: HashMap<PeerId, Entity>,
}

impl ConnectionRegistry {
    pub fn len(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    pub fn entity(&self, peer: PeerId) -> Option<Entity> {
        self.clients.get(&peer).copied()
    }

    fn register(&mut self, peer: PeerId, entity: Entity) -> Admission {
        if self.clients.contains_key(&peer) {
            return Admission::Duplicate;
        }
        if self.clients.len() >= MAX_CLIENTS {
            return Admission::Full;
        }

        self.clients.insert(peer, entity);
        Admission::Accepted
    }

    fn unregister(&mut self, peer: PeerId, entity: Entity) {
        if self.clients.get(&peer) == Some(&entity) {
            self.clients.remove(&peer);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    Accepted,
    Duplicate,
    Full,
}

pub fn configure_server(app: &mut App, tick_duration: Duration) {
    app.add_plugins(ServerPlugins { tick_duration })
        .init_resource::<ConnectionRegistry>()
        .add_systems(Startup, start_server)
        .add_observer(track_connected_client)
        .add_observer(track_disconnected_client);
}

pub fn configure_client(app: &mut App, tick_duration: Duration, client_id: u64) {
    app.add_plugins(ClientPlugins { tick_duration })
        .insert_resource(ClientNetworkConfig { client_id })
        .add_systems(Startup, start_client)
        .add_observer(report_client_connected)
        .add_observer(report_client_disconnected);
}

fn start_server(mut commands: Commands) {
    let server = commands
        .spawn((
            NetcodeServer::new(server::NetcodeConfig {
                protocol_id: PROTOCOL_ID,
                private_key: Key::default(),
                ..default()
            }),
            LocalAddr(SERVER_ADDR),
            ServerUdpIo::default(),
        ))
        .id();

    commands.trigger(Start { entity: server });
    info!(%SERVER_ADDR, maximum_clients = MAX_CLIENTS, "local server starting");
}

fn start_client(mut commands: Commands, config: Res<ClientNetworkConfig>) -> Result {
    let authentication = Authentication::Manual {
        server_addr: SERVER_ADDR,
        client_id: config.client_id,
        private_key: Key::default(),
        protocol_id: PROTOCOL_ID,
    };
    let client = commands
        .spawn((
            Client::default(),
            LocalAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)),
            PeerAddr(SERVER_ADDR),
            Link::new(None),
            NetcodeClient::new(authentication, client::NetcodeConfig::default())?,
            UdpIo::default(),
        ))
        .id();

    commands.trigger(Connect { entity: client });
    info!(client_id = config.client_id, %SERVER_ADDR, "client connecting");
    Ok(())
}

fn track_connected_client(
    trigger: On<Add, Connected>,
    remotes: Query<&RemoteId, With<ClientOf>>,
    mut registry: ResMut<ConnectionRegistry>,
    mut commands: Commands,
) {
    let Ok(remote) = remotes.get(trigger.entity) else {
        return;
    };

    match registry.register(remote.0, trigger.entity) {
        Admission::Accepted => {
            info!(peer = ?remote.0, clients = registry.len(), "client connected");
        }
        Admission::Duplicate => {
            warn!(peer = ?remote.0, "rejecting duplicate client identity");
            commands.entity(trigger.entity).insert(Disconnecting);
        }
        Admission::Full => {
            warn!(peer = ?remote.0, maximum_clients = MAX_CLIENTS, "rejecting client because server is full");
            commands.entity(trigger.entity).insert(Disconnecting);
        }
    }
}

fn track_disconnected_client(
    trigger: On<Add, Disconnected>,
    remotes: Query<&RemoteId, With<ClientOf>>,
    mut registry: ResMut<ConnectionRegistry>,
) {
    let Ok(remote) = remotes.get(trigger.entity) else {
        return;
    };

    registry.unregister(remote.0, trigger.entity);
    info!(peer = ?remote.0, clients = registry.len(), "client disconnected");
}

fn report_client_connected(trigger: On<Add, Connected>, clients: Query<&LocalId, With<Client>>) {
    if let Ok(local_id) = clients.get(trigger.entity) {
        info!(client = ?local_id.0, "connected to local server");
    }
}

fn report_client_disconnected(
    trigger: On<Add, Disconnected>,
    clients: Query<&Disconnected, With<Client>>,
) {
    if let Ok(disconnected) = clients.get(trigger.entity) {
        info!(reason = ?disconnected.reason, "disconnected from local server");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_accepts_five_unique_clients_and_rejects_sixth() {
        let mut registry = ConnectionRegistry::default();

        for id in 1..=MAX_CLIENTS as u64 {
            assert_eq!(
                registry.register(PeerId::Netcode(id), Entity::PLACEHOLDER),
                Admission::Accepted
            );
        }

        assert_eq!(registry.len(), MAX_CLIENTS);
        assert_eq!(
            registry.register(PeerId::Netcode(6), Entity::PLACEHOLDER),
            Admission::Full
        );
    }

    #[test]
    fn registry_rejects_duplicate_identity() {
        let mut registry = ConnectionRegistry::default();
        let peer = PeerId::Netcode(1);

        assert_eq!(
            registry.register(peer, Entity::PLACEHOLDER),
            Admission::Accepted
        );
        assert_eq!(
            registry.register(peer, Entity::PLACEHOLDER),
            Admission::Duplicate
        );
    }

    #[test]
    fn disconnect_releases_capacity() {
        let mut registry = ConnectionRegistry::default();
        let peer = PeerId::Netcode(1);
        let entity = Entity::PLACEHOLDER;
        assert_eq!(registry.register(peer, entity), Admission::Accepted);

        registry.unregister(peer, entity);

        assert!(registry.is_empty());
        assert_eq!(registry.entity(peer), None);
    }
}
