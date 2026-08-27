use std::{env, error::Error, io, path::Path};

use bevy::prelude::*;
use bevy::window::WindowResolution;
use game01_configs::load_embedded;
use game01_network::{NETWORK_SIMULATION_ENV, NetworkSimulationProfile};
use game01_simulation::MovementStep;
use game01_world_data::{CharacterCatalog, StartingRoomGrid};

use crate::controller::ControllerInput;
use crate::polytools::CharacterAssetLibrary;
use crate::prediction::ClientPredictionPlugin;
use crate::presentation::{CameraView, ClientPresentationPlugin};

mod controller;
mod eyes;
mod input;
mod polytools;
mod prediction;
mod presentation;

const INITIAL_WINDOW_WIDTH: u32 = 2880;
const INITIAL_WINDOW_HEIGHT: u32 = 1800;

fn main() -> Result<(), Box<dyn Error>> {
    let client_id = client_id_from_args()?;
    let network_simulation = network_simulation_from_env()?;
    let character_assets =
        CharacterAssetLibrary::load_from_directory(Path::new("assets/characters"))?;
    let controller_input = ControllerInput::new()?;
    let design = load_embedded()?;
    let tick_duration = design.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
        )
    })?;
    let movement_step = MovementStep::from_design(&design)?;
    let camera_view = design.camera.effective_view_tiles().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "camera view preset or dimensions are invalid",
        )
    })?;
    let snapshot_interval = design
        .network
        .snapshot_interval_for(design.simulation)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "snapshot send rate must be positive and an integer divisor of the simulation tick rate",
            )
        })?;
    let remote_interpolation_ratio = design
        .network
        .validated_remote_interpolation_ratio()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "remote interpolation ratio must be finite and greater than zero",
            )
        })?;

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Labyrinth — Secrets, Room's & Travels'".into(),
            resolution: WindowResolution::new(INITIAL_WINDOW_WIDTH, INITIAL_WINDOW_HEIGHT),
            resizable: true,
            ..default()
        }),
        ..default()
    }));
    app.insert_resource(movement_step);
    app.insert_resource(CharacterCatalog::from_json(include_str!(
        "../../../assets/characters/catalog.json"
    ))?);
    app.insert_resource(CameraView::new(camera_view.0, camera_view.1));
    app.insert_resource(
        StartingRoomGrid::from_tiles(design.room.width_tiles, design.room.height_tiles)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "room dimensions must be greater than zero",
                )
            })?,
    );
    app.insert_non_send(controller_input);
    app.add_plugins(ClientPredictionPlugin);
    app.add_plugins(ClientPresentationPlugin {
        client_id,
        tick_duration,
        snapshot_interval,
        remote_interpolation_ratio,
        network_simulation,
        character_assets,
    });
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
