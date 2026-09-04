use std::{collections::HashMap, time::Duration};

use bevy::{
    log::{info, warn},
    prelude::*,
};
use lightyear::connection::client::Disconnecting;
use lightyear::prelude::server::ServerUdpIo;
use lightyear::prelude::{
    ControlledBy, Lifetime,
    input::{
        native::{InputPlugin as NativeInputPlugin, NativeStateSequence},
        server::{InputValidationAppExt, authorize_controlled_targets},
    },
    server::*,
};
use lightyear::{netcode::Key, prelude::*};
use world01_world_data::{AnchorOccupancy, CharacterId, PlayerInput};

use crate::protocol::{
    JoinRequest, MAX_CLIENTS, NetworkSimulationProfile, PROTOCOL_ID, ReplicatedWorldState,
    SERVER_ADDR, apply_tick_player_input, register_game_protocol,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerNetworkSet {
    ReceiveRequests,
    PrepareSimulation,
}

#[derive(Component, Debug, Clone)]
pub struct ServerJoinRequest {
    pub character: CharacterId,
    owner: u64,
    connection: Entity,
    peer: PeerId,
}

impl ServerJoinRequest {
    pub fn owner(&self) -> u64 {
        self.owner
    }
}

#[derive(Resource, Debug, Default)]
struct ConnectionRegistry {
    clients: HashMap<PeerId, Entity>,
}

impl ConnectionRegistry {
    #[cfg(test)]
    fn len(&self) -> usize {
        self.clients.len()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    #[cfg(test)]
    fn entity(&self, peer: PeerId) -> Option<Entity> {
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

pub fn configure_server(
    app: &mut App,
    tick_duration: Duration,
    snapshot_interval: Duration,
    network_simulation: NetworkSimulationProfile,
) {
    app.add_plugins(ServerPlugins { tick_duration })
        .add_plugins(NativeInputPlugin::<PlayerInput>::default())
        .insert_resource(ReplicationMetadata::new(snapshot_interval))
        .insert_resource(network_simulation)
        .init_resource::<ConnectionRegistry>();
    register_game_protocol(app);
    app.add_systems(Startup, start_server)
        .add_systems(
            Update,
            receive_join_requests.in_set(ServerNetworkSet::ReceiveRequests),
        )
        .add_systems(
            FixedUpdate,
            apply_tick_player_input.in_set(ServerNetworkSet::PrepareSimulation),
        )
        .add_observer(prepare_server_client)
        .add_observer(report_server_started)
        .add_observer(track_connected_client)
        .add_observer(track_disconnected_client)
        .add_input_validator(authorize_controlled_targets::<NativeStateSequence<PlayerInput>>);
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
}

fn report_server_started(trigger: On<Add, Started>, servers: Query<(), With<NetcodeServer>>) {
    if servers.contains(trigger.entity) {
        info!(target: "game_console", "Server erfolgreich gestartet");
    }
}

fn prepare_server_client(
    trigger: On<Add, LinkOf>,
    network_simulation: Res<NetworkSimulationProfile>,
    mut links: Query<&mut Link>,
    mut commands: Commands,
) {
    if let Ok(mut link) = links.get_mut(trigger.entity) {
        link.recv.conditioner = network_simulation.receive_conditioner();
    }
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
        Admission::Accepted => {}
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
}

fn receive_join_requests(
    mut clients: Query<
        (Entity, &RemoteId, &mut MessageReceiver<JoinRequest>),
        (With<ClientOf>, With<Connected>),
    >,
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
        commands.spawn(ServerJoinRequest {
            character: request.character,
            owner,
            connection,
            peer: remote.0,
        });
    }
}

pub fn configure_replicated_player(player: &mut EntityCommands<'_>, request: &ServerJoinRequest) {
    player.insert((
        ControlledBy {
            owner: request.connection,
            lifetime: Lifetime::SessionBased,
        },
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(request.peer)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(request.peer)),
    ));
}

/// Marks the persistent authoritative world-state singleton for all clients.
///
/// Unlike player state, occupancy has no owner, prediction target, or
/// interpolation target. Keeping the entity alive lets normal replication
/// deliver its current component value to late joiners.
pub fn configure_replicated_world_state(
    world_state: &mut EntityCommands<'_>,
    occupancy: AnchorOccupancy,
) {
    world_state.insert((occupancy, Replicate::to_clients(NetworkTarget::All)));
    world_state.insert(ReplicatedWorldState);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;

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
    fn world_occupancy_is_persistent_replicated_state_for_late_join() {
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let entity = {
            let mut commands = Commands::new(&mut queue, &world);
            let mut entity = commands.spawn_empty();
            let id = entity.id();
            configure_replicated_world_state(&mut entity, AnchorOccupancy::default());
            id
        };
        queue.apply(&mut world);

        assert!(world.entity(entity).contains::<AnchorOccupancy>());
        assert!(world.entity(entity).contains::<ReplicatedWorldState>());
        assert!(world.entity(entity).contains::<Replicate>());
        assert!(!world.entity(entity).contains::<PredictionTarget>());
        assert!(!world.entity(entity).contains::<InterpolationTarget>());
    }
}
