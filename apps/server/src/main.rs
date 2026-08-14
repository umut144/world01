use std::{error::Error, io};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use game01_configs::load_embedded;
use game01_network::configure_server;

fn main() -> Result<(), Box<dyn Error>> {
    let design = load_embedded()?;
    let tick_duration = design.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
        )
    })?;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_duration)),
        LogPlugin::default(),
    ));
    configure_server(&mut app, tick_duration);
    app.run();
    Ok(())
}
