use std::{env, error::Error, io};

use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use bevy::window::WindowResolution;
use world01_configs::{load_embedded, local_start_map_override};
use world01_content::{
    CharacterCollisionGeometryCatalog, CharacterHurtGeometryCatalog, HammerCombatGeometry,
    MageEyeGeometry, RuntimeContent, WorldCollisionGeometryCatalog,
};
use world01_design::{load_embedded as load_game_design, load_world01_embedded};
use world01_moba::{MobaPlacementRanksDesign, MobaWorldDerivation, MobaWorldSource};
use world01_network::{NETWORK_SIMULATION_ENV, NetworkSimulationProfile};
use world01_simulation::{
    CharacterLifeRules, ExertionRules, HammerAttackRules, MageAttackRules, MovementStep,
    TraversalCatalog, WeaponAimRules, WorldColliderGrid, WorldDerivation,
};
use world01_world_data::{AnkhLayout, TeamId, WorldComposition, WorldMap, WorldTemplateCatalog};

use crate::controller::ControllerInput;
use crate::hammer::HammerPresentationRules;
use crate::polytools::CharacterAssetLibrary;
use crate::prediction::ClientPredictionPlugin;
use crate::presentation::{CameraView, ClientPresentationPlugin};
use crate::projection::ProjectionDepthPresentationPlugin;
use crate::session::ClientSessionPlugin;

mod controller;
mod eyes;
mod hammer;
mod input;
mod mage;
mod polytools;
mod pose;
mod prediction;
mod presentation;
mod projection;
mod session;

const INITIAL_WINDOW_PHYSICAL_WIDTH: u32 = 1024;
const INITIAL_WINDOW_PHYSICAL_HEIGHT: u32 = 640;

fn main() -> Result<(), Box<dyn Error>> {
    let client_id = client_id_from_args()?;
    let team = team_from_args()?;
    let network_simulation = network_simulation_from_env()?;
    let mut config = load_embedded()?;
    // A developer's own runtime.local.toml, gitignored, overrides which map
    // this process starts with - see world01_configs::local_start_map_override
    // for why this is a file both apps read identically rather than a
    // per-process environment variable the two could disagree on.
    if let Some(start_map) = local_start_map_override()? {
        config.world.start_map = start_map;
    }
    let world_design = load_world01_embedded()?;
    let game_design = load_game_design()?;
    let content = RuntimeContent::load_embedded()?;
    let world_map = WorldMap::load_embedded(&config.world.start_map)?;
    let world_templates = WorldTemplateCatalog::load_embedded()?;
    // A Totem outranks every Terrain and Prop `world01.toml` already ranks -
    // no Template may ever paint over an objective - so the MOBA's own
    // overlay adds its ranks to the sandbox's table rather than the sandbox
    // ever naming a Totem.
    let placement_ranks = world_design
        .placement_ranks()?
        .extended_with(MobaPlacementRanksDesign::load_embedded()?.entries())?;
    let world_composition =
        WorldComposition::new(world_map.clone(), &world_templates, &placement_ranks)?;
    let ankh_layout = AnkhLayout::from_map(&world_map);
    if ankh_layout.positions.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SceneMaker world map requires at least one Ankh placement",
        )
        .into());
    }
    // The same rule the world rebuild will call again on every recomposition,
    // used here to derive the first world - see MobaWorldDerivation.
    let moba_source = MobaWorldSource::load_embedded()?;
    let totem_layout = MobaWorldDerivation::derive(&world_map, &moba_source)?;
    let hammer_geometry = HammerCombatGeometry::from_content(&content)?;
    let hurt_geometry = CharacterHurtGeometryCatalog::from_content(&content)?;
    let mage_eye_geometry = MageEyeGeometry::from_content(&content)?;
    let world_collision =
        WorldCollisionGeometryCatalog::from_content_and_map(&content, &world_map)?;
    let world_collider_grid = WorldColliderGrid::from_catalog(&world_collision);
    let collision_geometry = CharacterCollisionGeometryCatalog::from_content(&content)?;
    let traversal_catalog = TraversalCatalog::from_design(&game_design.traversal)?;
    let character_assets = CharacterAssetLibrary::from_content(
        content.clone(),
        world_design.eyes.pupil_area_ratio,
        world_design.eyes.hammerer_collision_radius_ratio,
        game_design.mage.pupil_size_ratio,
        game_design.mage.pupil_edge_clearance_ratio,
    )?;
    let controller_input = ControllerInput::new()?;
    let tick_duration = config.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
        )
    })?;
    let movement_step = MovementStep::from_runtime(&config)?;
    let exertion_rules =
        ExertionRules::from_design(config.simulation.ticks_per_second, &world_design.locomotion)?;
    let character_life_rules =
        CharacterLifeRules::from_design(config.simulation.ticks_per_second, &world_design.health)?;
    let weapon_aim_rules =
        WeaponAimRules::from_design(config.simulation.ticks_per_second, &world_design.weapon_aim)?;
    let hammer_attack_rules =
        HammerAttackRules::from_design(config.simulation.ticks_per_second, &game_design.hammer)?;
    let mage_attack_rules = MageAttackRules::from_design(
        config.simulation.ticks_per_second,
        &game_design.mage,
        &game_design.mage_eye_beams,
    )?;
    let hammer_presentation_rules = HammerPresentationRules::from_design(
        &game_design.hammer,
        config.simulation.ticks_per_second,
        hammer_attack_rules,
    )
    .ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Hammer presentation values must be finite and within their valid ranges",
        )
    })?;
    let camera_view = config.camera.effective_view_tiles().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "camera view preset or dimensions are invalid",
        )
    })?;
    let snapshot_interval = config
        .network
        .snapshot_interval_for(config.simulation)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "snapshot send rate must be positive and an integer divisor of the simulation tick rate",
            )
        })?;
    let remote_interpolation_ratio = config
        .network
        .validated_remote_interpolation_ratio()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "remote interpolation ratio must be finite and greater than zero",
            )
        })?;

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(game_log_plugin()).set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Labyrinth — Secrets, Room's & Travels'".into(),
            resolution: WindowResolution::new(
                INITIAL_WINDOW_PHYSICAL_WIDTH,
                INITIAL_WINDOW_PHYSICAL_HEIGHT,
            ),
            resizable: true,
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(ProjectionDepthPresentationPlugin);
    app.insert_resource(movement_step);
    app.insert_resource(exertion_rules);
    app.insert_resource(character_life_rules);
    app.insert_resource(weapon_aim_rules);
    app.insert_resource(hammer_attack_rules);
    app.insert_resource(mage_attack_rules);
    app.insert_resource(hammer_presentation_rules);
    app.insert_resource(hammer_geometry);
    app.insert_resource(hurt_geometry);
    app.insert_resource(mage_eye_geometry);
    app.insert_resource(content);
    app.insert_resource(world_collision);
    app.insert_resource(world_collider_grid);
    app.insert_resource(collision_geometry);
    app.insert_resource(traversal_catalog);
    app.insert_resource(CameraView::new(camera_view.0, camera_view.1));
    app.insert_resource(world_composition);
    app.insert_resource(world_templates);
    app.insert_resource(placement_ranks);
    app.insert_resource(world_map);
    app.insert_resource(ankh_layout);
    app.insert_resource(moba_source);
    app.insert_resource(totem_layout);
    app.insert_non_send(controller_input);
    app.add_plugins(ClientPredictionPlugin::<MobaWorldDerivation>::default());
    app.add_plugins(ClientSessionPlugin {
        client_id,
        team,
        tick_duration,
        snapshot_interval,
        remote_interpolation_ratio,
        network_simulation,
    });
    app.add_plugins(ClientPresentationPlugin { character_assets });
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

/// The side this client plays on, as `--team <number>` anywhere in the args.
///
/// Absent means absent: no side was picked, and the value travels to the
/// server as `None` rather than as a guess. Whether a join without a side is
/// admissible is the game's decision, not this function's - a sandbox has no
/// sides at all, while the MOBA refuses the join. This is where a lobby will
/// eventually put the player's own choice; until it exists, a developer says
/// it on the command line so that nothing in between has to invent one.
fn team_from_args() -> Result<Option<TeamId>, Box<dyn Error>> {
    let args = env::args().collect::<Vec<_>>();
    let Some(flag) = args.iter().position(|argument| argument == "--team") else {
        return Ok(None);
    };
    let Some(value) = args.get(flag + 1) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--team must be followed by a side number",
        )
        .into());
    };
    Ok(Some(TeamId(value.parse::<u8>()?)))
}

fn client_id_from_args() -> Result<u64, Box<dyn Error>> {
    let Some(value) = env::args().nth(1) else {
        return Ok(u64::from(std::process::id()));
    };
    let client_id = value.parse::<u64>()?;
    if client_id == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "client id must be greater than zero",
        )
        .into());
    }
    Ok(client_id)
}
