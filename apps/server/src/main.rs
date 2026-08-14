use std::{error::Error, io};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*, state::app::StatesPlugin};
use game01_configs::load_embedded;
use game01_network::{ServerNetworkSet, configure_server};
use game01_simulation::{MovementStep, move_players};

fn main() -> Result<(), Box<dyn Error>> {
    let design = load_embedded()?;
    let tick_duration = design.simulation.tick_duration().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "simulation tick rate must be greater than zero",
        )
    })?;
    let movement_step = MovementStep::from_design(&design)?;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_duration)),
        LogPlugin::default(),
        StatesPlugin,
    ))
    .insert_resource(Time::<Fixed>::from_duration(tick_duration))
    .insert_resource(movement_step)
    .add_systems(
        FixedUpdate,
        move_players.after(ServerNetworkSet::PrepareSimulation),
    );
    configure_server(&mut app, tick_duration);
    app.run();
    Ok(())
}
