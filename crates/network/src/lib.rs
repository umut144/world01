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
use game01_world_data::{CharacterKind, PlayerId, PlayerOwner, SelectedCharacter};
use lightyear::{
    connection::client::Disconnecting,
    netcode::Key,
    prelude::{client::*, server::*},
};
use lightyear::{prelude::server::ServerUdpIo, prelude::*};
use serde::{Deserialize, Serialize};

pub const MAX_CLIENTS: usize = 5;
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);

const PROTOCOL_ID: u64 = 0x47_41_4d_45_30_31;

pub struct JoinChannel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRequest {
    pub character: CharacterKind,
}

#[derive(Component, Debug, Clone, Copy)]
struct PendingJoin(CharacterKind);

#[derive(Resource, Debug)]
struct NextPlayerId(u64);

impl Default for NextPlayerId {
    fn default() -> Self {
        Self(1)
    }
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

    fn unregister(&mut self, peer: PeerId, entity: Entity) -> bool {
        if self.clients.get(&peer) == Some(&entity) {
            self.clients.remove(&peer);
            return true;
        }
        false
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
        .insert_resource(ReplicationMetadata::new(tick_duration))
        .init_resource::<ConnectionRegistry>()
        .init_resource::<NextPlayerId>()
        .register_game_protocol()
        .add_systems(Startup, start_server)
        .add_systems(Update, handle_join_requests)
        .add_observer(prepare_server_client)
        .add_observer(track_connected_client)
        .add_observer(track_disconnected_client);
}

pub fn configure_client(app: &mut App, tick_duration: Duration) {
    app.add_plugins(ClientPlugins { tick_duration })
        .register_game_protocol()
        .add_observer(send_join_when_connected)
        .add_observer(report_client_connected)
        .add_observer(report_client_disconnected);
}

trait GameProtocolAppExt {
    fn register_game_protocol(&mut self) -> &mut Self;
}

impl GameProtocolAppExt for App {
    fn register_game_protocol(&mut self) -> &mut Self {
        self.register_message::<JoinRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        self.add_channel::<JoinChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        self.component::<PlayerId>().replicate_once();
        self.component::<PlayerOwner>().replicate_once();
        self.component::<SelectedCharacter>().replicate_once();
        self.component::<Transform>().replicate();
        self
    }
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

pub fn connect_client(
    commands: &mut Commands,
    client_id: u64,
    character: CharacterKind,
) -> Result<Entity> {
    let authentication = Authentication::Manual {
        server_addr: SERVER_ADDR,
        client_id,
        private_key: Key::default(),
        protocol_id: PROTOCOL_ID,
    };
    let client = commands
        .spawn((
            Client::default(),
            LocalAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)),
            PeerAddr(SERVER_ADDR),
            Link::new(None),
            ReplicationReceiver,
            PendingJoin(character),
            NetcodeClient::new(authentication, client::NetcodeConfig::default())?,
            UdpIo::default(),
        ))
        .id();

    commands.trigger(Connect { entity: client });
    info!(client_id, %SERVER_ADDR, "client connecting");
    Ok(client)
}

fn prepare_server_client(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(ReplicationSender);
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
    players: Query<(Entity, &PlayerOwner)>,
    mut commands: Commands,
) {
    let Ok(remote) = remotes.get(trigger.entity) else {
        return;
    };

    let was_registered = registry.unregister(remote.0, trigger.entity);
    if was_registered && let PeerId::Netcode(owner) = remote.0 {
        for (player, player_owner) in &players {
            if player_owner.0 == owner {
                commands.entity(player).despawn();
            }
        }
    }
    info!(peer = ?remote.0, clients = registry.len(), "client disconnected");
}

fn send_join_when_connected(
    trigger: On<Add, Connected>,
    mut clients: Query<(&PendingJoin, &mut MessageSender<JoinRequest>), With<Client>>,
    mut commands: Commands,
) {
    let Ok((selection, mut sender)) = clients.get_mut(trigger.entity) else {
        return;
    };

    sender.send::<JoinChannel>(JoinRequest {
        character: selection.0,
    });
    commands.entity(trigger.entity).remove::<PendingJoin>();
}

fn handle_join_requests(
    mut clients: Query<
        (Entity, &RemoteId, &mut MessageReceiver<JoinRequest>),
        (With<ClientOf>, With<Connected>),
    >,
    players: Query<&PlayerOwner>,
    mut next_player_id: ResMut<NextPlayerId>,
    mut commands: Commands,
) {
    for (connection, remote, mut receiver) in &mut clients {
        let Some(request) = receiver.receive().next() else {
            continue;
        };
        let PeerId::Netcode(owner) = remote.0 else {
            warn!(peer = ?remote.0, "ignoring join from unsupported peer identity");
            continue;
        };
        if players.iter().any(|player_owner| player_owner.0 == owner) {
            warn!(peer = ?remote.0, "ignoring repeated join request");
            continue;
        }

        let player_id = next_player_id.0;
        let Some(following_id) = player_id.checked_add(1) else {
            warn!("player id space exhausted; ignoring join request");
            continue;
        };
        next_player_id.0 = following_id;
        let spawn = spawn_position(player_id);
        commands.spawn((
            PlayerId(player_id),
            PlayerOwner(owner),
            SelectedCharacter(request.character),
            Transform::from_xyz(spawn.x, spawn.y, 0.0),
            Replicate::to_clients(NetworkTarget::All),
        ));
        info!(?connection, player_id, owner, character = ?request.character, "authoritative player spawned");
    }
}

fn spawn_position(player_id: u64) -> Vec2 {
    const POSITIONS: [Vec2; MAX_CLIENTS] = [
        Vec2::new(-4.0, 0.0),
        Vec2::new(-2.0, 0.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(2.0, 0.0),
        Vec2::new(4.0, 0.0),
    ];
    let index = (player_id.saturating_sub(1) % POSITIONS.len() as u64) as usize;
    POSITIONS[index]
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

        assert!(registry.unregister(peer, entity));

        assert!(registry.is_empty());
        assert_eq!(registry.entity(peer), None);
    }

    #[test]
    fn rejected_duplicate_disconnect_keeps_original_registration() {
        let mut registry = ConnectionRegistry::default();
        let peer = PeerId::Netcode(1);
        let original = Entity::from_raw_u32(1).expect("test entity index is valid");
        let duplicate = Entity::from_raw_u32(2).expect("test entity index is valid");
        assert_eq!(registry.register(peer, original), Admission::Accepted);

        assert!(!registry.unregister(peer, duplicate));

        assert_eq!(registry.entity(peer), Some(original));
    }

    #[test]
    fn spawn_positions_are_separated_and_repeat_safely() {
        let first: Vec2 = spawn_position(1);
        let second = spawn_position(2);

        assert_ne!(first, second);
        assert_eq!(first, spawn_position(6));
    }
}
