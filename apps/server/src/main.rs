use std::{env, error::Error, io};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*, state::app::StatesPlugin};
use game01_configs::load_embedded;
use game01_content::{CharacterHealthCatalog, HammerCombatGeometry, RuntimeContent};
use game01_network::{
    NETWORK_SIMULATION_ENV, NetworkSimulationProfile, ServerNetworkSet, configure_server,
};
use game01_simulation::{
    HammerAttackRules, MovementStep, SimulationSet, WeaponAimRules, add_simulation_step,
};

use crate::session::ServerSessionPlugin;

mod session;

fn main() -> Result<(), Box<dyn Error>> {
    let design = load_embedded()?;
    let content = RuntimeContent::load_embedded()?;
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
    let hammer_geometry = HammerCombatGeometry::from_content(&content)?;
    let character_health = CharacterHealthCatalog::from_content(&content)?;
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
    .insert_resource(content)
    .insert_resource(Time::<Fixed>::from_duration(tick_duration))
    .insert_resource(movement_step)
    .insert_resource(weapon_aim_rules)
    .insert_resource(hammer_attack_rules)
    .insert_resource(hammer_geometry)
    .insert_resource(character_health);
    add_simulation_step(&mut app, FixedUpdate);
    app.configure_sets(
        FixedUpdate,
        SimulationSet::GameplayStep.after(ServerNetworkSet::PrepareSimulation),
    );
    app.add_plugins(ServerSessionPlugin);
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
