use std::{env, error::Error, io};

use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use bevy::window::WindowResolution;
use world01_configs::load_embedded;
use world01_content::{CharacterHurtGeometryCatalog, HammerCombatGeometry, RuntimeContent};
use world01_design::load_embedded as load_game_design;
use world01_network::{NETWORK_SIMULATION_ENV, NetworkSimulationProfile};
use world01_simulation::{
    CharacterLifeRules, HammerAttackRules, LocomotionRules, MovementStep, WeaponAimRules,
};
use world01_world_data::AnkhLayout;

use crate::controller::ControllerInput;
use crate::hammer::HammerPresentationRules;
use crate::polytools::CharacterAssetLibrary;
use crate::prediction::ClientPredictionPlugin;
use crate::presentation::{CameraView, ClientPresentationPlugin, RoomDimensions};
use crate::projection::ProjectionDepthPresentationPlugin;
use crate::session::ClientSessionPlugin;

mod controller;
mod eyes;
mod hammer;
mod input;
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
    let network_simulation = network_simulation_from_env()?;
    let config = load_embedded()?;
    let game_design = load_game_design()?;
    let content = RuntimeContent::load_embedded()?;
    let hammer_geometry =
        HammerCombatGeometry::from_content(&content, &game_design.hammer.attack_components)?;
    let hurt_geometry = CharacterHurtGeometryCatalog::from_content(&content)?;
    let character_assets = CharacterAssetLibrary::from_content(
        content,
        config.eyes.pupil_area_ratio,
        config.eyes.hammerer_collision_radius_ratio,
    )?;
    let controller_input = ControllerInput::new()?;
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
    app.insert_resource(locomotion_rules);
    app.insert_resource(character_life_rules);
    app.insert_resource(weapon_aim_rules);
    app.insert_resource(hammer_attack_rules);
    app.insert_resource(hammer_presentation_rules);
    app.insert_resource(hammer_geometry);
    app.insert_resource(hurt_geometry);
    app.insert_resource(CameraView::new(camera_view.0, camera_view.1));
    app.insert_resource(
        RoomDimensions::new(config.room.width_tiles, config.room.height_tiles).ok_or_else(
            || {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "room dimensions must be greater than zero",
                )
            },
        )?,
    );
    app.insert_resource(AnkhLayout::for_room(
        config.room.width_tiles,
        config.room.height_tiles,
    ));
    app.insert_non_send(controller_input);
    app.add_plugins(ClientPredictionPlugin);
    app.add_plugins(ClientSessionPlugin {
        client_id,
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
