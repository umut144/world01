use serde::Deserialize;
use std::time::Duration;

const DESIGN_TOML: &str = include_str!("../design.toml");

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct DesignConfig {
    pub simulation: SimulationConfig,
    pub movement: MovementConfig,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct SimulationConfig {
    pub ticks_per_second: u32,
}

impl SimulationConfig {
    pub fn tick_duration(self) -> Option<Duration> {
        (self.ticks_per_second > 0)
            .then(|| Duration::from_secs_f64(1.0 / f64::from(self.ticks_per_second)))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct MovementConfig {
    pub speed_meters_per_second: f32,
}

pub fn load_embedded() -> Result<DesignConfig, toml::de::Error> {
    toml::from_str(DESIGN_TOML)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_simulation_runs_at_sixty_hertz() {
        let design = load_embedded().expect("embedded design configuration parses");

        assert_eq!(design.simulation.ticks_per_second, 60);
        assert_eq!(
            design.simulation.tick_duration(),
            Some(Duration::from_secs_f64(1.0 / 60.0))
        );
    }
}
