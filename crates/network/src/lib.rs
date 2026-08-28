//! Lightyear-specific protocol and transport boundary.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

#[cfg(feature = "server")]
use std::collections::HashMap;

#[cfg(feature = "server")]
use bevy::log::warn;
use bevy::{log::info, prelude::*};
use game01_world_data::{
    AttackIntent, BodyFacing, CharacterHealth, CharacterId, GazeDirection, GazeIntent,
    HammerAttackState, MovementDirection, MovementIntent, PlayerId, PlayerInput, PlayerOwner,
    Position, RoomId, SelectedCharacter, WeaponAimState,
};
#[cfg(feature = "server")]
use game01_world_data::{CharacterCatalog, CharacterHealthCatalog, StartingRoomGrid};
#[cfg(feature = "server")]
use lightyear::connection::client::Disconnecting;
#[cfg(feature = "client")]
use lightyear::interpolation::timeline::InterpolationConfig;
#[cfg(feature = "client")]
use lightyear::prediction::correction::PreviousVisual;
#[cfg(feature = "client")]
use lightyear::prelude::input::native::InputMarker;
use lightyear::prelude::input::native::{ActionState, InputPlugin as NativeInputPlugin};
#[cfg(feature = "server")]
use lightyear::prelude::server::ServerUdpIo;
#[cfg(feature = "client")]
use lightyear::prelude::{
    Controlled, client::*, input::client::InputSystems as ClientInputSystems,
};
#[cfg(feature = "server")]
use lightyear::prelude::{
    ControlledBy, Lifetime,
    input::{
        native::NativeStateSequence,
        server::{InputValidationAppExt, authorize_controlled_targets},
    },
    server::*,
};
use lightyear::{netcode::Key, prelude::*};
use serde::{Deserialize, Serialize};

pub const MAX_CLIENTS: usize = 5;
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);
pub const NETWORK_SIMULATION_ENV: &str = "GAME01_NETWORK_SIMULATION";
#[cfg(feature = "client")]
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

// Character identifiers are catalog strings rather than the former fixed enum.
const PROTOCOL_ID: u64 = 0x47_41_4d_45_30_32;
pub struct JoinChannel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRequest {
    pub character: CharacterId,
}

#[cfg(feature = "client")]
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ClientPlayerInput(pub PlayerInput);

#[cfg(feature = "client")]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ClientPositionCorrection {
    pub offset: Vec2,
}

#[cfg(feature = "client")]
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct RemotePositionExtrapolation {
    pub offset: Vec2,
}

#[cfg(feature = "client")]
#[derive(Resource, Debug, Clone, Copy)]
struct RemoteExtrapolationConfig {
    tick_duration: Duration,
    maximum_duration: Duration,
}

#[cfg(feature = "server")]
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerNetworkSet {
    PrepareSimulation,
}

#[cfg(feature = "client")]
#[derive(Component, Debug, Clone)]
struct PendingJoin(CharacterId);

#[cfg(feature = "server")]
#[derive(Resource, Debug)]
struct NextPlayerId(u64);

#[cfg(feature = "server")]
impl Default for NextPlayerId {
    fn default() -> Self {
        Self(1)
    }
}

#[cfg(feature = "server")]
#[derive(Resource, Debug, Default)]
pub struct ConnectionRegistry {
    clients: HashMap<PeerId, Entity>,
}

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    Accepted,
    Duplicate,
    Full,
}

#[cfg(feature = "server")]
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
        .init_resource::<ConnectionRegistry>()
        .init_resource::<NextPlayerId>()
        .register_game_protocol()
        .add_systems(Startup, start_server)
        .add_systems(Update, handle_join_requests)
        .add_systems(
            FixedUpdate,
            apply_tick_player_input.in_set(ServerNetworkSet::PrepareSimulation),
        )
        .add_observer(prepare_server_client)
        .add_observer(track_connected_client)
        .add_observer(track_disconnected_client)
        .add_input_validator(authorize_controlled_targets::<NativeStateSequence<PlayerInput>>);
}

#[cfg(feature = "client")]
pub fn configure_client(app: &mut App, tick_duration: Duration, snapshot_interval: Duration) {
    app.add_plugins(ClientPlugins { tick_duration })
        .add_plugins(NativeInputPlugin::<PlayerInput>::default())
        .init_resource::<ClientPlayerInput>()
        .insert_resource(RemoteExtrapolationConfig {
            tick_duration,
            maximum_duration: snapshot_interval.mul_f32(MAX_REMOTE_EXTRAPOLATION_INTERVALS),
        })
        .register_game_protocol()
        .add_systems(
            FixedPreUpdate,
            write_client_player_input.in_set(ClientInputSystems::WriteClientInputs),
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

#[cfg(feature = "client")]
fn enable_remote_position_extrapolation(trigger: On<Add, Interpolated>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(RemotePositionExtrapolation::default());
}

#[cfg(feature = "client")]
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

#[cfg(feature = "client")]
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
        self.component::<RoomId>().replicate().predict();
        self.component::<MovementDirection>().replicate().predict();
        self.component::<BodyFacing>().replicate().predict();
        self.component::<GazeDirection>().replicate().predict();
        self.component::<WeaponAimState>().replicate().predict();
        self.component::<HammerAttackState>().replicate().predict();
        self.component::<CharacterHealth>().replicate();
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

#[cfg(feature = "client")]
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

#[cfg(feature = "server")]
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

#[cfg(feature = "client")]
pub fn connect_client(
    commands: &mut Commands,
    client_id: u64,
    character: CharacterId,
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

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
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

#[cfg(feature = "client")]
fn enable_controlled_input(trigger: On<Add, Controlled>, mut commands: Commands) {
    commands.entity(trigger.entity).insert((
        InputMarker::<PlayerInput>::default(),
        MovementIntent::ZERO,
        GazeIntent::ZERO,
        AttackIntent::RELEASED,
    ));
}

#[cfg(feature = "client")]
fn send_join_when_connected(
    trigger: On<Add, Connected>,
    mut clients: Query<(&PendingJoin, &mut MessageSender<JoinRequest>), With<Client>>,
    mut commands: Commands,
) {
    let Ok((selection, mut sender)) = clients.get_mut(trigger.entity) else {
        return;
    };

    sender.send::<JoinChannel>(JoinRequest {
        character: selection.0.clone(),
    });
    commands.entity(trigger.entity).remove::<PendingJoin>();
}

#[cfg(feature = "server")]
fn handle_join_requests(
    mut clients: Query<
        (Entity, &RemoteId, &mut MessageReceiver<JoinRequest>),
        (With<ClientOf>, With<Connected>),
    >,
    players: Query<&PlayerOwner>,
    mut next_player_id: ResMut<NextPlayerId>,
    room_grid: Res<StartingRoomGrid>,
    catalog: Option<Res<CharacterCatalog>>,
    health_catalog: Option<Res<CharacterHealthCatalog>>,
    mut commands: Commands,
) {
    for (connection, remote, mut receiver) in &mut clients {
        let Some(request) = receiver.receive().next() else {
            continue;
        };
        if let Some(catalog) = catalog.as_ref()
            && !catalog.contains(&request.character)
        {
            warn!(peer = ?remote.0, character = ?request.character, "ignoring join for unknown character");
            continue;
        }
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
        let selected_character = request.character.clone();
        let maximum_health = health_catalog
            .as_ref()
            .and_then(|catalog| catalog.max_hp(&selected_character))
            .unwrap_or(140.0);
        let mut player = commands.spawn((
            PlayerId(player_id),
            PlayerOwner(owner),
            SelectedCharacter(selected_character.clone()),
            MovementIntent::ZERO,
            GazeIntent::ZERO,
            AttackIntent::RELEASED,
            MovementDirection::ZERO,
            BodyFacing::Authored,
            GazeDirection::RIGHT,
            Position::new(spawn.x, spawn.y),
            room_grid.starting_room(),
            ControlledBy {
                owner: connection,
                lifetime: Lifetime::SessionBased,
            },
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::Single(remote.0)),
            InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(remote.0)),
        ));
        player.insert(CharacterHealth::full(maximum_health));
        if selected_character.0 == "hammerer" {
            player.insert((WeaponAimState::RIGHT, HammerAttackState::IDLE));
        }
        info!(?connection, player_id, owner, character = ?selected_character, "authoritative player spawned");
    }
}

#[cfg(feature = "client")]
fn write_client_player_input(
    input: Res<ClientPlayerInput>,
    mut players: Query<
        &mut ActionState<PlayerInput>,
        (With<Controlled>, With<InputMarker<PlayerInput>>),
    >,
) {
    for mut action_state in &mut players {
        action_state.0 = input.0;
    }
}

#[cfg(any(feature = "client", feature = "server"))]
pub fn apply_tick_player_input(
    mut players: Query<(
        &ActionState<PlayerInput>,
        &mut MovementIntent,
        &mut GazeIntent,
        &mut AttackIntent,
    )>,
) {
    for (action_state, mut movement, mut gaze, mut attack) in &mut players {
        *movement = action_state.0.movement;
        *gaze = action_state.0.gaze;
        *attack = action_state.0.attack;
    }
}

#[cfg(feature = "client")]
pub fn client_input_timeline_synced(
    clients: Query<(), (With<Client>, With<IsSynced<InputTimeline>>)>,
) -> bool {
    !clients.is_empty()
}

#[cfg(feature = "server")]
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

#[cfg(feature = "client")]
fn report_client_connected(trigger: On<Add, Connected>, clients: Query<&LocalId, With<Client>>) {
    if let Ok(local_id) = clients.get(trigger.entity) {
        info!(client = ?local_id.0, "connected to local server");
    }
}

#[cfg(feature = "client")]
fn report_client_disconnected(
    trigger: On<Add, Disconnected>,
    clients: Query<&Disconnected, With<Client>>,
) {
    if let Ok(disconnected) = clients.get(trigger.entity) {
        info!(reason = ?disconnected.reason, "disconnected from local server");
    }
}

#[cfg(all(test, feature = "client", feature = "server"))]
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
                CharacterId::new("wizard").expect("static character id"),
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
            .add_plugins(NativeInputPlugin::<PlayerInput>::default())
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
    fn tick_input_applies_movement_gaze_and_attack_without_timeout() {
        let mut app = App::new();
        app.add_systems(FixedUpdate, apply_tick_player_input);
        let player = app
            .world_mut()
            .spawn((
                ActionState(PlayerInput {
                    movement: MovementIntent::new(1.0, 0.0),
                    gaze: GazeIntent::new(-1.0, 0.0),
                    attack: AttackIntent::PRESSED,
                }),
                MovementIntent::ZERO,
                GazeIntent::ZERO,
                AttackIntent::RELEASED,
            ))
            .id();

        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world().get::<MovementIntent>(player),
            Some(&MovementIntent::new(1.0, 0.0))
        );
        assert_eq!(
            app.world().get::<GazeIntent>(player),
            Some(&GazeIntent::new(-1.0, 0.0))
        );
        assert_eq!(
            app.world().get::<AttackIntent>(player),
            Some(&AttackIntent::PRESSED)
        );

        app.world_mut()
            .get_mut::<ActionState<PlayerInput>>(player)
            .expect("test player has native action state")
            .0 = PlayerInput::ZERO;
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world().get::<MovementIntent>(player),
            Some(&MovementIntent::ZERO)
        );
        assert_eq!(
            app.world().get::<GazeIntent>(player),
            Some(&GazeIntent::ZERO)
        );
        assert_eq!(
            app.world().get::<AttackIntent>(player),
            Some(&AttackIntent::RELEASED)
        );
    }
}
