use std::{env, error::Error, io};

use bevy::prelude::*;
use bevy::window::WindowResolution;
use game01_configs::load_embedded;

use crate::presentation::ClientPresentationPlugin;

mod presentation;

const INITIAL_WINDOW_WIDTH: u32 = 1280;
const INITIAL_WINDOW_HEIGHT: u32 = 800;

fn main() -> Result<(), Box<dyn Error>> {
    let client_id = client_id_from_args()?;
    let design = load_embedded()?;
    let tick_duration = design.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
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
    app.add_plugins(ClientPresentationPlugin {
        client_id,
        tick_duration,
    });
    app.run();
    Ok(())
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
