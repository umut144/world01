use std::{net::Ipv4Addr, time::Duration};

use bevy::{
    log::{error, info},
    prelude::*,
};
use lightyear::interpolation::timeline::InterpolationConfig;
use lightyear::prediction::correction::PreviousVisual;
pub use lightyear::prelude::Client;
use lightyear::prelude::{
    Controlled, ReplicationSystems,
    client::*,
    input::client::InputSystems as ClientInputSystems,
    input::native::{ActionState, InputMarker, InputPlugin as NativeInputPlugin},
};
use lightyear::{netcode::Key, prelude::*};
use world01_world_data::{
    AnchorOccupancy, AttackIntent, CharacterId, DashIntent, DeathConfirmIntent, GazeIntent,
    MovementIntent, PlacementRanks, PlayerInput, Position, RunIntent, WorldComposition,
    WorldTemplateCatalog,
};

use crate::protocol::{
    JoinChannel, JoinRequest, NetworkSimulationProfile, PROTOCOL_ID, SERVER_ADDR,
    register_game_protocol,
};

const MAX_REMOTE_EXTRAPOLATION_INTERVALS: f32 = 2.0;

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ClientPlayerInput(pub PlayerInput);

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

#[derive(Component, Debug, Clone)]
struct PendingJoin(CharacterId);

pub fn configure_client(app: &mut App, tick_duration: Duration, snapshot_interval: Duration) {
    app.add_plugins(ClientPlugins { tick_duration })
        .add_plugins(NativeInputPlugin::<PlayerInput>::default())
        .init_resource::<ClientPlayerInput>()
        .insert_resource(RemoteExtrapolationConfig {
            tick_duration,
            maximum_duration: snapshot_interval.mul_f32(MAX_REMOTE_EXTRAPOLATION_INTERVALS),
        });
    register_game_protocol(app);
    app.add_systems(
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
    .add_observer(report_client_connected);
}

/// Derives the client-local world composition from replicated authority state.
///
/// This is separate from transport setup because callers must first provide
/// the matching embedded base map, Template catalog, and Placement Ranks.
pub fn configure_client_world_state(app: &mut App) {
    app.add_systems(
        PreUpdate,
        apply_replicated_world_occupancy.after(ReplicationSystems::Receive),
    );
}

fn apply_replicated_world_occupancy(
    replicated: Query<&AnchorOccupancy, Changed<AnchorOccupancy>>,
    templates: Res<WorldTemplateCatalog>,
    ranks: Res<PlacementRanks>,
    mut composition: ResMut<WorldComposition>,
) {
    if replicated.is_empty() {
        return;
    }
    let Ok(occupancy) = replicated.single() else {
        error!(
            "cannot apply replicated world occupancy: expected exactly one changed world-state entity"
        );
        return;
    };
    match composition.bypass_change_detection().apply_newer_occupancy(
        occupancy.clone(),
        &templates,
        &ranks,
    ) {
        Ok(true) => composition.set_changed(),
        Ok(false) => {}
        Err(error) => {
            error!(%error, generation = occupancy.generation(), "cannot apply replicated world occupancy");
        }
    }
}

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
            LocalAddr(std::net::SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0)),
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
    info!(target: "game_console", "Client erfolgreich gestartet");
    Ok(client)
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

fn enable_controlled_input(trigger: On<Add, Controlled>, mut commands: Commands) {
    commands.entity(trigger.entity).insert((
        InputMarker::<PlayerInput>::default(),
        MovementIntent::ZERO,
        GazeIntent::ZERO,
        AttackIntent::RELEASED,
        RunIntent::RELEASED,
        DashIntent::RELEASED,
        DeathConfirmIntent::RELEASED,
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
        character: selection.0.clone(),
    });
    commands.entity(trigger.entity).remove::<PendingJoin>();
}

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

pub fn client_input_timeline_synced(
    clients: Query<(), (With<Client>, With<IsSynced<InputTimeline>>)>,
) -> bool {
    !clients.is_empty()
}

fn report_client_connected(trigger: On<Add, Connected>, clients: Query<(), With<Client>>) {
    if clients.contains(trigger.entity) {
        info!(target: "game_console", "Client erfolgreich eingeloggt");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::CommandQueue;
    use world01_world_data::{WorldMap, WorldTemplateCatalog};

    #[derive(Resource, Default)]
    struct CompositionChangeCount(u32);

    fn count_composition_changes(
        composition: Res<WorldComposition>,
        mut count: ResMut<CompositionChangeCount>,
    ) {
        if composition.is_changed() {
            count.0 += 1;
        }
    }

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
                offset: Vec2::new(3.0, -2.0)
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
    fn client_accepts_late_join_snapshot_and_ignores_stale_generation() {
        let map = WorldMap::load_embedded("overworld01").expect("the embedded Instance is valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("the embedded Template catalog is valid");
        let ranks = PlacementRanks::from_entries([("grass", 10), ("tree", 20), ("ankh", 100)])
            .expect("the current embedded Assets have Placement Ranks");
        let mut authority = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the authority composition is valid");
        authority
            .set_occupant("template_anchor_001", "test_template", &templates, &ranks)
            .expect("the authority assignment is valid");

        let mut app = App::new();
        app.insert_resource(templates.clone())
            .insert_resource(ranks.clone())
            .insert_resource(
                WorldComposition::new(map, &templates, &ranks)
                    .expect("the replica composition is valid"),
            )
            .init_resource::<CompositionChangeCount>()
            .add_systems(
                Update,
                (
                    apply_replicated_world_occupancy,
                    count_composition_changes.after(apply_replicated_world_occupancy),
                ),
            );
        let replicated = app.world_mut().spawn(authority.occupancy().clone()).id();

        app.update();
        assert_eq!(
            app.world().resource::<WorldComposition>().current_map(),
            authority.current_map()
        );
        assert_eq!(
            app.world()
                .resource::<WorldComposition>()
                .occupancy()
                .generation(),
            1
        );
        assert_eq!(app.world().resource::<CompositionChangeCount>().0, 1);

        app.world_mut()
            .entity_mut(replicated)
            .insert(AnchorOccupancy::default());
        app.update();
        assert_eq!(
            app.world()
                .resource::<WorldComposition>()
                .occupancy()
                .generation(),
            1
        );
        assert_eq!(app.world().resource::<CompositionChangeCount>().0, 1);

        authority
            .set_occupant("template_anchor_002", "test_template02", &templates, &ranks)
            .expect("the newer authority assignment is valid");
        app.world_mut()
            .entity_mut(replicated)
            .insert(authority.occupancy().clone());
        app.update();
        assert_eq!(
            app.world().resource::<WorldComposition>().current_map(),
            authority.current_map()
        );
        assert_eq!(
            app.world().resource::<WorldComposition>().occupancy(),
            authority.occupancy()
        );
        assert_eq!(app.world().resource::<CompositionChangeCount>().0, 2);
    }
}
