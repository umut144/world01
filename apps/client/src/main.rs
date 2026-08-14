use std::{env, error::Error, io};

use bevy::prelude::*;
use game01_configs::load_embedded;
use game01_network::configure_client;

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
    app.add_plugins(DefaultPlugins);
    configure_client(&mut app, tick_duration, client_id);
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
