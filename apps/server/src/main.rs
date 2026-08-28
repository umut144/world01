use std::{env, error::Error, io};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*, state::app::StatesPlugin};
use game01_configs::load_embedded;
use game01_network::{
    NETWORK_SIMULATION_ENV, NetworkSimulationProfile, ServerNetworkSet, configure_server,
};
use game01_simulation::{
    HammerAttackRules, MovementStep, WeaponAimRules, advance_hammer_attacks, move_players,
    update_character_orientation, update_gaze_direction, update_weapon_aim,
};
use game01_world_data::{
    CharacterCatalog, CharacterHealthCatalog, HammerCombatGeometry, StartingRoomGrid,
};

fn main() -> Result<(), Box<dyn Error>> {
    let design = load_embedded()?;
    let network_simulation = network_simulation_from_env()?;
    let tick_duration = design.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
        )
    })?;
    let movement_step = MovementStep::from_design(&design)?;
    let weapon_aim_rules = WeaponAimRules::from_design(&design)?;
    let hammer_attack_rules = HammerAttackRules::from_design(&design)?;
    let hammer_geometry = HammerCombatGeometry::from_runtime_manifests(
        include_str!("../../../assets/characters/hammerer/manifest.json"),
        include_str!("../../../assets/characters/hammer/manifest.json"),
    )?;
    let character_health = CharacterHealthCatalog::from_manifests([
        include_str!("../../../assets/characters/archerf/manifest.json"),
        include_str!("../../../assets/characters/barde/manifest.json"),
        include_str!("../../../assets/characters/chantres/manifest.json"),
        include_str!("../../../assets/characters/glavier/manifest.json"),
        include_str!("../../../assets/characters/hammerer/manifest.json"),
        include_str!("../../../assets/characters/mage/manifest.json"),
        include_str!("../../../assets/characters/monk/manifest.json"),
        include_str!("../../../assets/characters/rogue/manifest.json"),
        include_str!("../../../assets/characters/sorcerer/manifest.json"),
        include_str!("../../../assets/characters/warrior/manifest.json"),
        include_str!("../../../assets/characters/wizard/manifest.json"),
    ])?;
    let snapshot_interval = design
        .network
        .snapshot_interval_for(design.simulation)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "snapshot send rate must be positive and an integer divisor of the simulation tick rate",
            )
        })?;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_duration)),
        LogPlugin::default(),
        StatesPlugin,
    ))
    .insert_resource(CharacterCatalog::from_json(include_str!(
        "../../../assets/characters/catalog.json"
    ))?)
    .insert_resource(Time::<Fixed>::from_duration(tick_duration))
    .insert_resource(movement_step)
    .insert_resource(weapon_aim_rules)
    .insert_resource(hammer_attack_rules)
    .insert_resource(hammer_geometry)
    .insert_resource(character_health)
    .insert_resource(
        StartingRoomGrid::from_tiles(design.room.width_tiles, design.room.height_tiles)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "room dimensions must be greater than zero",
                )
            })?,
    )
    .add_systems(
        FixedUpdate,
        (
            update_gaze_direction,
            update_weapon_aim,
            advance_hammer_attacks,
            move_players,
            update_character_orientation,
        )
            .chain()
            .after(ServerNetworkSet::PrepareSimulation),
    );
    configure_server(
        &mut app,
        tick_duration,
        snapshot_interval,
        network_simulation,
    );
    app.run();
    Ok(())
}

fn network_simulation_from_env() -> Result<NetworkSimulationProfile, Box<dyn Error>> {
    let value = env::var(NETWORK_SIMULATION_ENV).unwrap_or_else(|_| "off".to_owned());
    NetworkSimulationProfile::from_name(&value).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{NETWORK_SIMULATION_ENV} must be 'off', 'latency-jitter', or 'average', got '{value}'"
            ),
        )
        .into()
    })
}
