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
use game01_world_data::{
    CharacterKind, MovementIntent, PlayerId, PlayerOwner, Position, SelectedCharacter,
};
use lightyear::interpolation::timeline::InterpolationConfig;
use lightyear::prediction::correction::PreviousVisual;
use lightyear::{connection::client::Disconnecting, netcode::Key};
use lightyear::{prelude::server::ServerUdpIo, prelude::*};
use lightyear::{
    prelude::{
        Controlled, ControlledBy, Lifetime,
        input::{
            client::InputSystems as ClientInputSystems,
            native::{
                ActionState, InputMarker, InputPlugin as NativeInputPlugin, NativeStateSequence,
            },
            server::{InputValidationAppExt, authorize_controlled_targets},
        },
    },
    prelude::{client::*, server::*},
};
use serde::{Deserialize, Serialize};

pub const MAX_CLIENTS: usize = 5;
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);
pub const NETWORK_SIMULATION_ENV: &str = "GAME01_NETWORK_SIMULATION";
const MAX_REMOTE_EXTRAPOLATION_INTERVALS: f32 = 2.0;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NetworkSimulationProfile {
    #[default]
    Off,
    LatencyJitter,
    Average,
}

impl NetworkSimulationProfile {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "off" => Some(Self::Off),
            "latency-jitter" => Some(Self::LatencyJitter),
            "average" => Some(Self::Average),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::LatencyJitter => "latency-jitter",
            Self::Average => "average",
        }
    }

    fn receive_config(self) -> Option<LinkConditionerConfig> {
        let end_to_end = match self {
            Self::Off => return None,
            Self::LatencyJitter => LinkConditionerConfig::new(
                Duration::from_millis(100),
                Duration::from_millis(20),
                0.0,
            ),
            Self::Average => LinkConditionerConfig::new(
                Duration::from_millis(100),
                Duration::from_millis(20),
                0.02,
            ),
        };
        Some(end_to_end.half())
    }

    fn receive_conditioner(self) -> Option<RecvLinkConditioner> {
        self.receive_config().map(RecvLinkConditioner::new)
    }
}

const PROTOCOL_ID: u64 = 0x47_41_4d_45_30_31;
pub struct JoinChannel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRequest {
    pub character: CharacterKind,
}

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ClientMovementInput(pub MovementIntent);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ClientPositionCorrection {
    pub offset: Vec2,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct RemotePositionExtrapolation {
    pub offset: Vec2,
}

#[derive(Resource, Debug, Clone, Copy)]
struct RemoteExtrapolationConfig {
    tick_duration: Duration,
    maximum_duration: Duration,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerNetworkSet {
    PrepareSimulation,
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

pub fn configure_server(
    app: &mut App,
    tick_duration: Duration,
    snapshot_interval: Duration,
    network_simulation: NetworkSimulationProfile,
) {
    app.add_plugins(ServerPlugins { tick_duration })
        .add_plugins(NativeInputPlugin::<MovementIntent>::default())
        .insert_resource(ReplicationMetadata::new(snapshot_interval))
        .insert_resource(network_simulation)
        .init_resource::<ConnectionRegistry>()
        .init_resource::<NextPlayerId>()
        .register_game_protocol()
        .add_systems(Startup, start_server)
        .add_systems(Update, handle_join_requests)
        .add_systems(
            FixedUpdate,
            apply_tick_movement_intents.in_set(ServerNetworkSet::PrepareSimulation),
        )
        .add_observer(prepare_server_client)
        .add_observer(track_connected_client)
        .add_observer(track_disconnected_client)
        .add_input_validator(authorize_controlled_targets::<NativeStateSequence<MovementIntent>>);
}

pub fn configure_client(app: &mut App, tick_duration: Duration, snapshot_interval: Duration) {
    app.add_plugins(ClientPlugins { tick_duration })
        .add_plugins(NativeInputPlugin::<MovementIntent>::default())
        .init_resource::<ClientMovementInput>()
        .insert_resource(RemoteExtrapolationConfig {
            tick_duration,
            maximum_duration: snapshot_interval.mul_f32(MAX_REMOTE_EXTRAPOLATION_INTERVALS),
        })
        .register_game_protocol()
        .add_systems(
            FixedPreUpdate,
            write_client_movement_input.in_set(ClientInputSystems::WriteClientInputs),
        )
        .add_systems(
            PreUpdate,
            expose_position_corrections.in_set(RollbackSystems::EndRollback),
        )
        .add_systems(
            Update,
            expose_remote_position_extrapolation.after(InterpolationSystems::Interpolate),
        )
        .add_observer(enable_controlled_input)
        .add_observer(enable_remote_position_extrapolation)
        .add_observer(send_join_when_connected)
        .add_observer(report_client_connected)
        .add_observer(report_client_disconnected);
}

fn enable_remote_position_extrapolation(trigger: On<Add, Interpolated>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(RemotePositionExtrapolation::default());
}

fn expose_remote_position_extrapolation(
    timelines: Query<&InterpolationTimeline>,
    config: Res<RemoteExtrapolationConfig>,
    mut players: Query<
        (
            &ConfirmedHistory<Position>,
            &mut RemotePositionExtrapolation,
        ),
        With<Interpolated>,
    >,
) {
    let Ok(timeline) = timelines.single() else {
        return;
    };
    let current_tick = timeline.tick().0;
    let overstep = timeline.overstep().to_f32();

    for (history, mut extrapolation) in &mut players {
        let mut previous = None;
        let mut latest = None;
        for sample in history {
            previous = latest;
            latest = Some(sample);
        }

        extrapolation.offset = match (previous, latest) {
            (Some((previous_tick, previous)), Some((latest_tick, latest))) => {
                bounded_position_extrapolation(
                    *previous,
                    previous_tick.0,
                    *latest,
                    latest_tick.0,
                    current_tick,
                    overstep,
                    config.tick_duration,
                    config.maximum_duration,
                )
            }
            _ => Vec2::ZERO,
        };
    }
}

fn bounded_position_extrapolation(
    previous: Position,
    previous_tick: u32,
    latest: Position,
    latest_tick: u32,
    current_tick: u32,
    overstep: f32,
    tick_duration: Duration,
    maximum_duration: Duration,
) -> Vec2 {
    let sample_ticks = latest_tick.saturating_sub(previous_tick);
    if sample_ticks == 0 || current_tick < latest_tick {
        return Vec2::ZERO;
    }

    let sample_seconds = tick_duration.as_secs_f32() * sample_ticks as f32;
    if sample_seconds <= 0.0 {
        return Vec2::ZERO;
    }

    let velocity = Vec2::new(latest.x - previous.x, latest.y - previous.y) / sample_seconds;
    let missing_ticks = current_tick.saturating_sub(latest_tick) as f32 + overstep;
    let extrapolation_seconds =
        (missing_ticks * tick_duration.as_secs_f32()).min(maximum_duration.as_secs_f32());
    velocity * extrapolation_seconds
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
        self.component::<Position>()
            .replicate()
            .predict()
            .enable_correction()
            .into_component_registration()
            .add_interpolation_with(interpolate_position);
        self
    }
}

fn interpolate_position(start: Position, end: Position, t: f32) -> Position {
    Position::new(
        start.x + (end.x - start.x) * t,
        start.y + (end.y - start.y) * t,
    )
}

fn expose_position_corrections(
    corrected: Query<(Entity, &Position, &PreviousVisual<Position>)>,
    mut commands: Commands,
) {
    for (entity, position, previous_visual) in &corrected {
        commands
            .entity(entity)
            .insert(ClientPositionCorrection {
                offset: Vec2::new(
                    previous_visual.0.x - position.x,
                    previous_visual.0.y - position.y,
                ),
            })
            .remove::<PreviousVisual<Position>>();
    }
}

fn start_server(network_simulation: Res<NetworkSimulationProfile>, mut commands: Commands) {
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
    info!(
        %SERVER_ADDR,
        maximum_clients = MAX_CLIENTS,
        network_simulation = network_simulation.name(),
        "local server starting"
    );
}

pub fn connect_client(
    commands: &mut Commands,
    client_id: u64,
    character: CharacterKind,
    remote_interpolation_ratio: f32,
    network_simulation: NetworkSimulationProfile,
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
            Link::new(network_simulation.receive_conditioner()),
            ReplicationReceiver,
            PredictionManager::default(),
            InterpolationConfig::default().with_send_interval_ratio(remote_interpolation_ratio),
            PendingJoin(character),
            NetcodeClient::new(authentication, client::NetcodeConfig::default())?,
            UdpIo::default(),
        ))
        .id();

    commands.trigger(Connect { entity: client });
    info!(client_id, %SERVER_ADDR, network_simulation = network_simulation.name(), "client connecting");
    Ok(client)
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

fn enable_controlled_input(trigger: On<Add, Controlled>, mut commands: Commands) {
    commands.entity(trigger.entity).insert((
        InputMarker::<MovementIntent>::default(),
        MovementIntent::ZERO,
    ));
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
            MovementIntent::ZERO,
            Position::new(spawn.x, spawn.y),
            ControlledBy {
                owner: connection,
                lifetime: Lifetime::SessionBased,
            },
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::Single(remote.0)),
            InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(remote.0)),
        ));
        info!(?connection, player_id, owner, character = ?request.character, "authoritative player spawned");
    }
}

fn write_client_movement_input(
    input: Res<ClientMovementInput>,
    mut players: Query<
        &mut ActionState<MovementIntent>,
        (With<Controlled>, With<InputMarker<MovementIntent>>),
    >,
) {
    for mut action_state in &mut players {
        action_state.0 = input.0;
    }
}

pub fn apply_tick_movement_intents(
    mut players: Query<(&ActionState<MovementIntent>, &mut MovementIntent)>,
) {
    for (action_state, mut intent) in &mut players {
        *intent = action_state.0;
    }
}

pub fn client_input_timeline_synced(
    clients: Query<(), (With<Client>, With<IsSynced<InputTimeline>>)>,
) -> bool {
    !clients.is_empty()
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
    use bevy::ecs::world::CommandQueue;

    #[test]
    fn connected_client_initializes_prediction_context() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        configure_client(
            &mut app,
            Duration::from_secs_f64(1.0 / 60.0),
            Duration::from_secs_f64(1.0 / 30.0),
        );

        let mut queue = CommandQueue::default();
        let client = {
            let mut commands = Commands::new(&mut queue, app.world());
            connect_client(
                &mut commands,
                1,
                CharacterKind::Wizard,
                1.0,
                NetworkSimulationProfile::Off,
            )
            .expect("client configuration should be valid")
        };
        queue.apply(app.world_mut());

        assert!(app.world().entity(client).contains::<PredictionManager>());
        assert!(app.world().entity(client).contains::<InputTimelineConfig>());
        assert_eq!(
            app.world()
                .entity(client)
                .get::<InterpolationConfig>()
                .map(|config| config.send_interval_ratio),
            Some(1.0)
        );
    }

    #[test]
    fn average_network_simulation_is_opt_in_and_conditions_receive_links() {
        assert!(
            NetworkSimulationProfile::Off
                .receive_conditioner()
                .is_none()
        );
        assert!(
            NetworkSimulationProfile::LatencyJitter
                .receive_conditioner()
                .is_some()
        );
        assert!(
            NetworkSimulationProfile::Average
                .receive_conditioner()
                .is_some()
        );

        let link = Link::new(NetworkSimulationProfile::Average.receive_conditioner());
        assert!(link.recv.conditioner.is_some());

        let config = NetworkSimulationProfile::Average
            .receive_config()
            .expect("average profile should have receive-side settings");
        assert_eq!(config.incoming_latency, Duration::from_millis(50));
        assert_eq!(config.incoming_jitter, Duration::from_millis(10));
        assert_eq!(config.incoming_loss, 0.01);
    }

    #[test]
    fn position_interpolation_is_linear() {
        let start = Position::new(-2.0, 4.0);
        let end = Position::new(6.0, -4.0);

        assert_eq!(interpolate_position(start, end, 0.0), start);
        assert_eq!(
            interpolate_position(start, end, 0.25),
            Position::new(0.0, 2.0)
        );
        assert_eq!(interpolate_position(start, end, 1.0), end);
    }

    #[test]
    fn remote_extrapolation_is_bounded_to_two_snapshot_intervals() {
        let offset = bounded_position_extrapolation(
            Position::new(0.0, 0.0),
            10,
            Position::new(4.0 / 30.0, 0.0),
            12,
            30,
            0.0,
            Duration::from_secs_f64(1.0 / 60.0),
            Duration::from_secs_f64(2.0 / 30.0),
        );

        assert!((offset.x - 4.0 * 2.0 / 30.0).abs() < 0.000_001);
        assert_eq!(offset.y, 0.0);
    }

    #[test]
    fn remote_extrapolation_does_not_run_before_latest_sample() {
        let offset = bounded_position_extrapolation(
            Position::new(0.0, 0.0),
            10,
            Position::new(1.0, 0.0),
            12,
            11,
            0.5,
            Duration::from_secs_f64(1.0 / 60.0),
            Duration::from_secs_f64(2.0 / 30.0),
        );

        assert_eq!(offset, Vec2::ZERO);
    }

    #[test]
    fn reconciliation_exposes_only_a_client_visual_offset() {
        let mut app = App::new();
        app.add_systems(Update, expose_position_corrections);
        let entity = app
            .world_mut()
            .spawn((
                Position::new(2.0, 3.0),
                PreviousVisual(Position::new(5.0, 1.0)),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<ClientPositionCorrection>(entity),
            Some(&ClientPositionCorrection {
                offset: Vec2::new(3.0, -2.0),
            })
        );
        assert!(
            app.world()
                .get::<PreviousVisual<Position>>(entity)
                .is_none()
        );
        assert_eq!(
            app.world().get::<Position>(entity),
            Some(&Position::new(2.0, 3.0))
        );
    }

    #[test]
    fn dedicated_server_protocol_updates_without_client_timeline() {
        let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
        let snapshot_interval = Duration::from_secs_f64(1.0 / 30.0);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin));
        app.add_plugins(ServerPlugins { tick_duration })
            .add_plugins(NativeInputPlugin::<MovementIntent>::default())
            .insert_resource(ReplicationMetadata::new(snapshot_interval))
            .register_game_protocol();

        app.finish();
        app.cleanup();
        app.update();
    }

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

    #[test]
    fn tick_input_applies_direction_and_stop_without_timeout() {
        let mut app = App::new();
        app.add_systems(FixedUpdate, apply_tick_movement_intents);
        let player = app
            .world_mut()
            .spawn((
                ActionState(MovementIntent::new(1.0, 0.0)),
                MovementIntent::ZERO,
            ))
            .id();

        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world().get::<MovementIntent>(player),
            Some(&MovementIntent::new(1.0, 0.0))
        );

        app.world_mut()
            .get_mut::<ActionState<MovementIntent>>(player)
            .expect("test player has native action state")
            .0 = MovementIntent::ZERO;
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world().get::<MovementIntent>(player),
            Some(&MovementIntent::ZERO)
        );
    }
}
