use std::{env, error::Error, io};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*, state::app::StatesPlugin};
use game01_configs::load_embedded;
use game01_content::{
    CharacterHealthCatalog, CharacterHurtGeometryCatalog, HammerCombatGeometry, RuntimeContent,
};
use game01_design::load_embedded as load_game_design;
use game01_network::{
    NETWORK_SIMULATION_ENV, NetworkSimulationProfile, ServerNetworkSet, configure_server,
};
use game01_simulation::{
    CharacterLifeRules, HammerAttackRules, HammerStrikeRules, LocomotionRules, MovementStep,
    SimulationSet, WeaponAimRules, add_simulation_step, apply_hammer_strike_damage,
    update_character_life,
};

use crate::session::ServerSessionPlugin;

mod session;

fn main() -> Result<(), Box<dyn Error>> {
    let config = load_embedded()?;
    let game_design = load_game_design()?;
    let content = RuntimeContent::load_embedded()?;
    let network_simulation = network_simulation_from_env()?;
    let tick_duration = config.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
        )
    })?;
    let movement_step = MovementStep::from_design(&config)?;
    let locomotion_rules = LocomotionRules::from_design(&config)?;
    let character_life_rules = CharacterLifeRules::from_design(&config)?;
    let weapon_aim_rules = WeaponAimRules::from_design(&config)?;
    let hammer_attack_rules =
        HammerAttackRules::from_design(config.simulation.ticks_per_second, &game_design.hammer)?;
    let hammer_strike_rules = HammerStrikeRules::from_design(
        config.simulation.ticks_per_second,
        &game_design.hammer,
        &game_design.hammer_strike,
    )?;
    let hammer_geometry =
        HammerCombatGeometry::from_content(&content, &game_design.hammer.attack_components)?;
    let hurt_geometry = CharacterHurtGeometryCatalog::from_content(&content)?;
    let character_health = CharacterHealthCatalog::from_content(&content)?;
    let snapshot_interval = config
        .network
        .snapshot_interval_for(config.simulation)
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
    .insert_resource(locomotion_rules)
    .insert_resource(character_life_rules)
    .insert_resource(weapon_aim_rules)
    .insert_resource(hammer_attack_rules)
    .insert_resource(hammer_strike_rules)
    .insert_resource(hammer_geometry)
    .insert_resource(hurt_geometry)
    .insert_resource(character_health);
    add_simulation_step(&mut app, FixedUpdate);
    app.configure_sets(
        FixedUpdate,
        SimulationSet::GameplayStep.after(ServerNetworkSet::PrepareSimulation),
    );
    app.add_systems(
        FixedUpdate,
        (apply_hammer_strike_damage, update_character_life)
            .chain()
            .after(SimulationSet::GameplayStep),
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
