use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use bevy::prelude::*;
use lightyear::prelude::{
    AppChannelExt, AppComponentExt, AppMessageExt, ChannelMode, ChannelSettings,
    InterpolationRegistrationExt, LinkConditionerConfig, NetworkDirection, PredictionBuilderExt,
    RecvLinkConditioner, ReliableSettings, input::native::ActionState,
};
use serde::{Deserialize, Serialize};
use world01_world_data::{
    AttackIntent, BodyFacing, CharacterHealth, CharacterId, CharacterLifeState, CharacterMass,
    DashIntent, DashState, DeathConfirmIntent, DeathConfirmationState, GazeDirection, GazeIntent,
    HammerAttackState, MageAttackState, MovementDirection, MovementIntent, MovementVelocity,
    PlayerId, PlayerInput, PlayerOwner, Position, RespawnState, RevivalState, RunIntent, RunState,
    SelectedCharacter, StaminaState, StatusEffectState, WeaponAimState,
};

pub const MAX_CLIENTS: usize = 5;
pub(crate) const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);
pub const NETWORK_SIMULATION_ENV: &str = "WORLD01_NETWORK_SIMULATION";
pub(crate) const PROTOCOL_ID: u64 = 0x47_41_4d_45_30_32;

pub(crate) struct JoinChannel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JoinRequest {
    pub character: CharacterId,
}

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

    pub(crate) fn receive_config(self) -> Option<LinkConditionerConfig> {
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

    pub(crate) fn receive_conditioner(self) -> Option<RecvLinkConditioner> {
        self.receive_config().map(RecvLinkConditioner::new)
    }
}

pub(crate) fn register_game_protocol(app: &mut App) {
    app.register_message::<JoinRequest>()
        .add_direction(NetworkDirection::ClientToServer);
    app.add_channel::<JoinChannel>(ChannelSettings {
        mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
        ..default()
    })
    .add_direction(NetworkDirection::ClientToServer);
    app.component::<PlayerId>().replicate_once();
    app.component::<PlayerOwner>().replicate_once();
    app.component::<SelectedCharacter>().replicate_once();
    app.component::<CharacterMass>().replicate_once().predict();
    app.component::<MovementDirection>().replicate().predict();
    app.component::<MovementVelocity>().replicate().predict();
    app.component::<StaminaState>().replicate().predict();
    app.component::<RunState>().replicate().predict();
    app.component::<DashState>().replicate().predict();
    app.component::<StatusEffectState>().replicate().predict();
    app.component::<BodyFacing>().replicate().predict();
    app.component::<GazeDirection>().replicate().predict();
    app.component::<WeaponAimState>().replicate().predict();
    app.component::<HammerAttackState>().replicate().predict();
    app.component::<MageAttackState>().replicate().predict();
    app.component::<CharacterHealth>().replicate().predict();
    app.component::<CharacterLifeState>().replicate().predict();
    app.component::<DeathConfirmationState>()
        .replicate()
        .predict();
    app.component::<RevivalState>().replicate().predict();
    app.component::<RespawnState>().replicate().predict();
    app.component::<Position>()
        .replicate()
        .predict()
        .enable_correction()
        .into_component_registration()
        .add_interpolation_with(interpolate_position);
}

fn interpolate_position(start: Position, end: Position, t: f32) -> Position {
    Position::new(
        start.x + (end.x - start.x) * t,
        start.y + (end.y - start.y) * t,
    )
}

pub fn apply_tick_player_input(
    mut players: Query<(
        &ActionState<PlayerInput>,
        &mut MovementIntent,
        &mut GazeIntent,
        &mut AttackIntent,
        &mut RunIntent,
        &mut DashIntent,
        &mut DeathConfirmIntent,
    )>,
) {
    for (action_state, mut movement, mut gaze, mut attack, mut run, mut dash, mut death_confirm) in
        &mut players
    {
        *movement = action_state.0.movement;
        *gaze = action_state.0.gaze;
        *attack = action_state.0.attack;
        *run = action_state.0.run;
        *dash = action_state.0.dash;
        *death_confirm = action_state.0.death_confirm;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightyear::prelude::input::native::InputPlugin as NativeInputPlugin;
    use lightyear::prelude::{Link, ReplicationMetadata, server::ServerPlugins};

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

    #[cfg(feature = "server")]
    #[test]
    fn dedicated_server_protocol_updates_without_client_timeline() {
        let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
        let snapshot_interval = Duration::from_secs_f64(1.0 / 30.0);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin));
        app.add_plugins(ServerPlugins { tick_duration })
            .add_plugins(NativeInputPlugin::<PlayerInput>::default())
            .insert_resource(ReplicationMetadata::new(snapshot_interval));
        register_game_protocol(&mut app);
        app.finish();
        app.cleanup();
        app.update();
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
                    run: RunIntent::PRESSED,
                    dash: DashIntent::PRESSED,
                    death_confirm: DeathConfirmIntent::PRESSED,
                }),
                MovementIntent::ZERO,
                GazeIntent::ZERO,
                AttackIntent::RELEASED,
                RunIntent::RELEASED,
                DashIntent::RELEASED,
                DeathConfirmIntent::RELEASED,
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
        assert_eq!(
            app.world().get::<RunIntent>(player),
            Some(&RunIntent::RELEASED)
        );
        assert_eq!(
            app.world().get::<DashIntent>(player),
            Some(&DashIntent::RELEASED)
        );
        assert_eq!(
            app.world().get::<DeathConfirmIntent>(player),
            Some(&DeathConfirmIntent::RELEASED)
        );
    }
}
