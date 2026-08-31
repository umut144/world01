use std::{env, error::Error, io};

use bevy::{
    app::ScheduleRunnerPlugin,
    log::{Level, LogPlugin},
    prelude::*,
    state::app::StatesPlugin,
};
use world01_configs::load_embedded;
use world01_content::{
    CharacterHealthCatalog, CharacterHurtGeometryCatalog, CharacterMassGeometryCatalog,
    HammerCombatGeometry, MageEyeGeometry, RuntimeContent, WorldCollisionGeometryCatalog,
};
use world01_design::load_embedded as load_game_design;
use world01_network::{
    NETWORK_SIMULATION_ENV, NetworkSimulationProfile, ServerNetworkSet, configure_server,
};
use world01_simulation::{
    CharacterLifeRules, CharacterMassCatalog, HammerAttackRules, HammerStrikeRules,
    LocomotionRules, MageAttackRules, MovementStep, SimulationSet, WeaponAimRules,
    add_simulation_step, apply_hammer_strike_damage, apply_mage_beam_damage, expire_mage_beams,
    finish_mage_cooldowns, update_character_life,
};
use world01_world_data::{AnkhLayout, WorldMap};

use crate::session::ServerSessionPlugin;

mod session;

fn main() -> Result<(), Box<dyn Error>> {
    let config = load_embedded()?;
    let game_design = load_game_design()?;
    let content = RuntimeContent::load_embedded()?;
    let world_map = WorldMap::load_embedded()?;
    let ankh_layout = AnkhLayout::from_map(&world_map);
    if ankh_layout.positions.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SceneMaker world map requires at least one Ankh placement",
        )
        .into());
    }
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
    let mage_attack_rules = MageAttackRules::from_design(
        config.simulation.ticks_per_second,
        &game_design.mage,
        &game_design.mage_eye_beams,
    )?;
    let hammer_geometry =
        HammerCombatGeometry::from_content(&content, &game_design.hammer.attack_components)?;
    let hurt_geometry = CharacterHurtGeometryCatalog::from_content(&content)?;
    let mage_eye_geometry =
        MageEyeGeometry::from_content(&content, game_design.mage.eye_size_ratio)?;
    let world_collision = WorldCollisionGeometryCatalog::from_content_and_map(&content, &world_map);
    let character_health = CharacterHealthCatalog::from_content(&content)?;
    let mass_geometry = CharacterMassGeometryCatalog::from_content(&content, &game_design.mass)?;
    let character_mass = CharacterMassCatalog::from_geometry(&config, &mass_geometry)?;
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
        game_log_plugin(),
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
    .insert_resource(mage_attack_rules)
    .insert_resource(hammer_geometry)
    .insert_resource(hurt_geometry)
    .insert_resource(mage_eye_geometry)
    .insert_resource(world_collision)
    .insert_resource(character_health)
    .insert_resource(character_mass)
    .insert_resource(world_map)
    .insert_resource(ankh_layout);
    add_simulation_step(&mut app, FixedUpdate);
    app.configure_sets(
        FixedUpdate,
        SimulationSet::GameplayStep.after(ServerNetworkSet::PrepareSimulation),
    );
    app.add_systems(
        FixedUpdate,
        (
            apply_hammer_strike_damage,
            apply_mage_beam_damage,
            expire_mage_beams,
            finish_mage_cooldowns,
            update_character_life,
        )
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

fn game_log_plugin() -> LogPlugin {
    LogPlugin {
        filter: "game_console=debug".to_owned(),
        level: Level::ERROR,
        fmt_layer: |_| {
            Some(Box::new(
                bevy::log::tracing_subscriber::fmt::Layer::default()
                    .without_time()
                    .with_target(false)
                    .with_level(false)
                    .with_writer(std::io::stderr),
            ))
        },
        ..default()
    }
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
