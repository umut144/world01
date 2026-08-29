use serde::Deserialize;
use std::{collections::HashMap, path::Path, time::Duration};

const DESIGN_TOML: &str = include_str!("../design.toml");

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DesignConfig {
    pub simulation: SimulationConfig,
    pub network: NetworkConfig,
    pub movement: MovementConfig,
    pub locomotion: LocomotionConfig,
    pub health: HealthConfig,
    pub weapon_aim: WeaponAimConfig,
    pub room: RoomConfig,
    pub camera: CameraConfig,
    pub eyes: EyesConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WeaponAimConfig {
    pub default_degrees_per_second: f32,
    #[serde(default)]
    pub character_degrees_per_second: HashMap<String, f32>,
}

impl WeaponAimConfig {
    pub fn is_valid(&self) -> bool {
        self.default_degrees_per_second.is_finite()
            && self.default_degrees_per_second > 0.0
            && self
                .character_degrees_per_second
                .iter()
                .all(|(character, speed)| {
                    !character.is_empty() && speed.is_finite() && *speed > 0.0
                })
    }
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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct LocomotionConfig {
    pub default_max_stamina: f32,
    pub stamina_regeneration_percent_per_second: f32,
    pub run_speed_multiplier: f32,
    pub run_drain_per_second: f32,
    pub dash_cost_percent: f32,
    pub dash_speed_multiplier: f32,
    pub dash_duration_seconds: f32,
    pub dash_invulnerability_seconds: f32,
    pub knockdown_duration_seconds: f32,
    pub knockdown_damage_percent_max_hp: f32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct HealthConfig {
    pub death_confirmation_seconds: f32,
    pub death_confirmation_initial_degrees_per_second: f32,
    pub death_confirmation_max_degrees_per_second: f32,
    pub revival_seconds: f32,
    pub revival_health_percent: f32,
    pub respawn_health_percent: f32,
    pub ankh_respawn_radius_meters: f32,
}

impl HealthConfig {
    pub fn is_valid(self) -> bool {
        self.death_confirmation_seconds.is_finite()
            && self.death_confirmation_seconds > 0.0
            && self
                .death_confirmation_initial_degrees_per_second
                .is_finite()
            && self.death_confirmation_initial_degrees_per_second >= 0.0
            && self.death_confirmation_max_degrees_per_second.is_finite()
            && self.death_confirmation_max_degrees_per_second
                >= self.death_confirmation_initial_degrees_per_second
            && self.revival_seconds.is_finite()
            && self.revival_seconds > 0.0
            && self.revival_health_percent.is_finite()
            && (0.0..=100.0).contains(&self.revival_health_percent)
            && self.respawn_health_percent.is_finite()
            && (0.0..=100.0).contains(&self.respawn_health_percent)
            && self.ankh_respawn_radius_meters.is_finite()
            && self.ankh_respawn_radius_meters >= 0.0
    }
}

impl LocomotionConfig {
    pub fn is_valid(self) -> bool {
        self.default_max_stamina.is_finite()
            && self.default_max_stamina > 0.0
            && self.stamina_regeneration_percent_per_second.is_finite()
            && self.stamina_regeneration_percent_per_second >= 0.0
            && self.run_speed_multiplier.is_finite()
            && self.run_speed_multiplier >= 1.0
            && self.run_drain_per_second.is_finite()
            && self.run_drain_per_second >= 0.0
            && self.dash_cost_percent.is_finite()
            && (0.0..=100.0).contains(&self.dash_cost_percent)
            && self.dash_speed_multiplier.is_finite()
            && self.dash_speed_multiplier > 0.0
            && self.dash_duration_seconds.is_finite()
            && self.dash_duration_seconds > 0.0
            && self.dash_invulnerability_seconds.is_finite()
            && (0.0..=self.dash_duration_seconds).contains(&self.dash_invulnerability_seconds)
            && self.knockdown_duration_seconds.is_finite()
            && self.knockdown_duration_seconds > 0.0
            && self.knockdown_damage_percent_max_hp.is_finite()
            && (0.0..=100.0).contains(&self.knockdown_damage_percent_max_hp)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct EyesConfig {
    pub pupil_area_ratio: f32,
    pub hammerer_collision_radius_ratio: f32,
}

impl EyesConfig {
    pub fn is_valid(self) -> bool {
        self.pupil_area_ratio.is_finite()
            && self.pupil_area_ratio > 0.0
            && self.pupil_area_ratio < 1.0
            && self.hammerer_collision_radius_ratio.is_finite()
            && self.hammerer_collision_radius_ratio > 0.0
            && self.hammerer_collision_radius_ratio <= 1.0
    }
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
        assert_eq!(design.camera.effective_view_tiles(), Some((16, 10)));
        assert!(design.camera.is_valid());
        assert_eq!(design.eyes.pupil_area_ratio, 0.26);
        assert_eq!(design.eyes.hammerer_collision_radius_ratio, 0.35);
        assert!(design.eyes.is_valid());
        assert_eq!(design.movement.speed_meters_per_second, 0.8);
        assert_eq!(design.locomotion.default_max_stamina, 100.0);
        assert_eq!(
            design.locomotion.stamina_regeneration_percent_per_second,
            2.5
        );
        assert_eq!(design.locomotion.run_speed_multiplier, 1.5);
        assert_eq!(design.locomotion.run_drain_per_second, 8.0);
        assert_eq!(design.locomotion.dash_cost_percent, 17.0);
        assert_eq!(design.locomotion.dash_speed_multiplier, 2.0);
        assert_eq!(design.locomotion.dash_duration_seconds, 1.0);
        assert_eq!(design.locomotion.dash_invulnerability_seconds, 0.337);
        assert_eq!(design.locomotion.knockdown_duration_seconds, 2.0);
        assert_eq!(design.locomotion.knockdown_damage_percent_max_hp, 5.0);
        assert!(design.locomotion.is_valid());
        assert_eq!(design.health.death_confirmation_seconds, 4.0);
        assert_eq!(
            design.health.death_confirmation_initial_degrees_per_second,
            144.0
        );
        assert_eq!(
            design.health.death_confirmation_max_degrees_per_second,
            1440.0
        );
        assert_eq!(design.health.revival_seconds, 8.0);
        assert_eq!(design.health.revival_health_percent, 80.0);
        assert_eq!(design.health.respawn_health_percent, 40.0);
        assert_eq!(design.health.ankh_respawn_radius_meters, 4.0);
        assert!(design.health.is_valid());
        assert_eq!(design.weapon_aim.default_degrees_per_second, 60.0);
        assert!(design.weapon_aim.character_degrees_per_second.is_empty());
        assert!(design.weapon_aim.is_valid());
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
