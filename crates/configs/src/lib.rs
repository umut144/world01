use serde::Deserialize;
use std::{path::Path, time::Duration};

const RUNTIME_TOML: &str = include_str!("../runtime.toml");

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RuntimeConfig {
    pub simulation: SimulationConfig,
    pub network: NetworkConfig,
    pub camera: CameraConfig,
    pub world: WorldConfig,
}

/// Which world this process serves.
///
/// A deployment choice rather than a design one: two servers running the same
/// game host different maps, while the rules they play by are identical. What a
/// Hammerer is belongs in the design crate; which world this process is, does
/// not.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct WorldConfig {
    /// The `scene_id` of the authored map to load.
    ///
    /// The server's value is the one that counts. A client is told which map it
    /// joined and checks that it has that one, rather than picking its own -
    /// two processes reading this key independently is how a client ends up
    /// predicting movement through a world the server does not have.
    pub start_map: String,
}

impl WorldConfig {
    pub fn is_valid(&self) -> bool {
        !self.start_map.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct SimulationConfig {
    pub ticks_per_second: u32,
    pub world_separation_meters_per_second: f32,
}

impl SimulationConfig {
    pub fn tick_duration(self) -> Option<Duration> {
        (self.ticks_per_second > 0)
            .then(|| Duration::from_secs_f64(1.0 / f64::from(self.ticks_per_second)))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct NetworkConfig {
    pub snapshot_send_hz: u32,
    pub remote_interpolation_ratio: f32,
}

impl NetworkConfig {
    pub fn snapshot_interval(self) -> Option<Duration> {
        (self.snapshot_send_hz > 0)
            .then(|| Duration::from_secs_f64(1.0 / f64::from(self.snapshot_send_hz)))
    }

    pub fn snapshot_interval_for(self, simulation: SimulationConfig) -> Option<Duration> {
        (self.snapshot_send_hz <= simulation.ticks_per_second
            && simulation.ticks_per_second % self.snapshot_send_hz.max(1) == 0)
            .then(|| self.snapshot_interval())
            .flatten()
    }

    pub fn validated_remote_interpolation_ratio(self) -> Option<f32> {
        (self.remote_interpolation_ratio.is_finite() && self.remote_interpolation_ratio > 0.0)
            .then_some(self.remote_interpolation_ratio)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct CameraConfig {
    pub view_preset: u32,
    pub view_width_tiles: u32,
    pub view_height_tiles: u32,
}

impl CameraConfig {
    pub const fn effective_view_tiles(self) -> Option<(u32, u32)> {
        match self.view_preset {
            0 => Some((self.view_width_tiles, self.view_height_tiles)),
            1 => Some((8, 5)),
            2 => Some((16, 10)),
            3 => Some((21, 13)),
            4 => Some((24, 15)),
            5 => Some((32, 20)),
            6 => Some((37, 23)),
            7 => Some((40, 25)),
            8 => Some((45, 28)),
            _ => None,
        }
    }

    pub const fn is_valid(self) -> bool {
        matches!(self.effective_view_tiles(), Some((width, height)) if width > 0 && height > 0)
    }
}

pub fn load_embedded() -> Result<RuntimeConfig, toml::de::Error> {
    toml::from_str(RUNTIME_TOML)
}

pub fn load_file(path: &Path) -> Result<RuntimeConfig, Box<dyn std::error::Error + Send + Sync>> {
    let contents = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&contents)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_runtime_configuration_is_valid() {
        let runtime = load_embedded().expect("embedded runtime configuration parses");
        assert_eq!(runtime.simulation.ticks_per_second, 60);
        assert_eq!(runtime.simulation.world_separation_meters_per_second, 3.0);
        assert_eq!(runtime.network.snapshot_send_hz, 30);
        assert_eq!(runtime.network.remote_interpolation_ratio, 2.0);
        assert_eq!(runtime.camera.effective_view_tiles(), Some((16, 10)));
        assert!(runtime.camera.is_valid());
        assert_eq!(runtime.world.start_map, "overworld01");
        assert!(runtime.world.is_valid());
    }
}
