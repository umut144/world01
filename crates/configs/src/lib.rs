use serde::Deserialize;
use std::{path::Path, time::Duration};

const DESIGN_TOML: &str = include_str!("../design.toml");

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct DesignConfig {
    pub simulation: SimulationConfig,
    pub network: NetworkConfig,
    pub movement: MovementConfig,
    pub room: RoomConfig,
    pub camera: CameraConfig,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct SimulationConfig {
    pub ticks_per_second: u32,
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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct RoomConfig {
    pub width_tiles: u32,
    pub height_tiles: u32,
}

impl RoomConfig {
    pub const fn is_valid(self) -> bool {
        self.width_tiles > 0 && self.height_tiles > 0
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
            3 => Some((24, 15)),
            4 => Some((32, 20)),
            5 => Some((40, 25)),
            _ => None,
        }
    }

    pub const fn is_valid(self) -> bool {
        matches!(self.effective_view_tiles(), Some((width, height)) if width > 0 && height > 0)
    }
}

pub fn load_embedded() -> Result<DesignConfig, toml::de::Error> {
    toml::from_str(DESIGN_TOML)
}

pub fn load_file(path: &Path) -> Result<DesignConfig, Box<dyn std::error::Error + Send + Sync>> {
    let contents = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&contents)?)
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
        assert_eq!(design.network.snapshot_send_hz, 30);
        assert_eq!(
            design.network.snapshot_interval(),
            Some(Duration::from_secs_f64(1.0 / 30.0))
        );
        assert_eq!(design.network.remote_interpolation_ratio, 2.0);
        assert_eq!(design.room.width_tiles, 50);
        assert_eq!(design.room.height_tiles, 50);
        assert!(design.room.is_valid());
        assert_eq!(design.camera.effective_view_tiles(), Some((8, 5)));
        assert!(design.camera.is_valid());
        assert_eq!(
            design.network.snapshot_interval_for(design.simulation),
            Some(Duration::from_secs_f64(1.0 / 30.0))
        );
        assert_eq!(
            design.network.validated_remote_interpolation_ratio(),
            Some(2.0)
        );
    }

    #[test]
    fn network_cadence_rejects_invalid_profiles() {
        let simulation = SimulationConfig {
            ticks_per_second: 60,
        };

        for snapshot_send_hz in [0, 40, 120] {
            let network = NetworkConfig {
                snapshot_send_hz,
                remote_interpolation_ratio: 1.0,
            };
            assert_eq!(network.snapshot_interval_for(simulation), None);
        }

        for remote_interpolation_ratio in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let network = NetworkConfig {
                snapshot_send_hz: 30,
                remote_interpolation_ratio,
            };
            assert_eq!(network.validated_remote_interpolation_ratio(), None);
        }
    }
}
