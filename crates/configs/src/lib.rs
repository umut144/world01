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

/// Where [`local_start_map_override`] looks, anchored at compile time to this
/// crate rather than to whatever directory a process happens to be run from.
const RUNTIME_LOCAL_TOML_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime.local.toml");

#[derive(Debug, Deserialize)]
struct LocalRuntimeOverride {
    start_map: String,
}

/// A developer's own choice of [`WorldConfig::start_map`], read from a file
/// `runtime.toml` does not carry and git does not track.
///
/// `start_map` is a deployment choice, not a design one, and the server's
/// value is the one that counts - a client reading it independently and
/// disagreeing is how a client ends up predicting movement through a world
/// the server does not have (see [`WorldConfig`]'s own doc). A per-process
/// environment variable would let exactly that happen: set it in one
/// terminal and forget the other, and the two silently disagree. A file next
/// to `runtime.toml`, read the same way by both apps, cannot disagree with
/// itself - so this is a file, not an environment variable, on purpose.
///
/// Returns `Ok(None)` when the file does not exist, which is the normal case
/// for everyone who has not created one: [`load_embedded`]'s own value
/// stands. When the file exists it must parse and name a real map - the same
/// "no fallbacks" contract `runtime.toml` itself already has - so a typo
/// here fails loudly rather than silently starting the wrong map or falling
/// back to the committed default.
pub fn local_start_map_override() -> Result<Option<String>, Box<dyn std::error::Error>> {
    local_start_map_override_from(Path::new(RUNTIME_LOCAL_TOML_PATH))
}

/// [`local_start_map_override`]'s logic, taking the file's path as an
/// argument so a test can exercise it without ever touching the one real
/// developers put their own override in.
fn local_start_map_override_from(
    path: &Path,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let local: LocalRuntimeOverride = toml::from_str(&contents)?;
    if local.start_map.is_empty() {
        return Err("runtime.local.toml names an empty start_map".into());
    }
    Ok(Some(local.start_map))
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
        assert_eq!(runtime.world.start_map, "map01");
        assert!(runtime.world.is_valid());
    }

    fn override_test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("world01_configs_test_{name}.toml"))
    }

    #[test]
    fn a_missing_local_override_file_is_not_an_error() {
        let path = override_test_path("missing");
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            local_start_map_override_from(&path).expect("a missing file is not an error"),
            None
        );
    }

    #[test]
    fn a_local_override_file_names_the_start_map() {
        let path = override_test_path("names_map");
        std::fs::write(&path, "start_map = \"map01\"\n").expect("the temp file writes");

        let result = local_start_map_override_from(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            result.expect("a valid override file parses"),
            Some("map01".to_owned())
        );
    }

    #[test]
    fn an_empty_start_map_in_the_override_file_is_refused() {
        let path = override_test_path("empty_map");
        std::fs::write(&path, "start_map = \"\"\n").expect("the temp file writes");

        let result = local_start_map_override_from(&path);
        let _ = std::fs::remove_file(&path);

        assert!(result.is_err());
    }

    #[test]
    fn a_malformed_override_file_is_refused_not_ignored() {
        let path = override_test_path("malformed");
        std::fs::write(&path, "not valid toml [[[").expect("the temp file writes");

        let result = local_start_map_override_from(&path);
        let _ = std::fs::remove_file(&path);

        assert!(result.is_err());
    }
}
