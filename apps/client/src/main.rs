use std::{env, error::Error, io};

use bevy::prelude::*;
use bevy::window::WindowResolution;
use game01_configs::load_embedded;
use game01_network::{NETWORK_SIMULATION_ENV, NetworkSimulationProfile};
use game01_simulation::MovementStep;

use crate::prediction::ClientPredictionPlugin;
use crate::presentation::ClientPresentationPlugin;

mod prediction;
mod presentation;

const INITIAL_WINDOW_WIDTH: u32 = 1280;
const INITIAL_WINDOW_HEIGHT: u32 = 800;

fn main() -> Result<(), Box<dyn Error>> {
    let client_id = client_id_from_args()?;
    let network_simulation = network_simulation_from_env()?;
    let design = load_embedded()?;
    let tick_duration = design.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
        )
    })?;
    let movement_step = MovementStep::from_design(&design)?;
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
            resizable: false,
            ..default()
        }),
        ..default()
    }));
    app.insert_resource(movement_step);
    app.add_plugins(ClientPredictionPlugin);
    app.add_plugins(ClientPresentationPlugin {
        client_id,
        tick_duration,
        remote_interpolation_ratio,
        network_simulation,
    });
    app.run();
    Ok(())
}

fn network_simulation_from_env() -> Result<NetworkSimulationProfile, Box<dyn Error>> {
    let value = env::var(NETWORK_SIMULATION_ENV).unwrap_or_else(|_| "off".to_owned());
    NetworkSimulationProfile::from_name(&value).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{NETWORK_SIMULATION_ENV} must be 'off' or 'average', got '{value}'"),
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
