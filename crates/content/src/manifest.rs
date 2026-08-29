use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
use game01_world_data::CharacterId;
use serde::Deserialize;

pub const HAMMER_ASSET_KEY: &str = "hammer";
pub const WEAPON_SOCKET_ROLE: &str = "weapon_socket_primary";
pub const WEAPON_GRIP_ROLE: &str = "grip_primary";
pub const WEAPON_SECONDARY_GRIP_ROLE: &str = "grip_secondary";
pub const WEAPON_ATTACK_POINT_ROLE: &str = "attack_point_primary";
pub const WEAPON_REACH_LIMIT_ROLE: &str = "reach_limit_primary";

#[derive(Resource, Clone)]
pub struct RuntimeContent {
    characters: HashMap<CharacterId, RuntimeManifest>,
    hammer: RuntimeManifest,
}

impl RuntimeContent {
    pub fn load_embedded() -> Result<Self, ContentError> {
        Self::from_source_loader(
            include_str!("../../../assets/catalog.json"),
            |asset_type, asset_key| {
                embedded_manifest(asset_type, asset_key)
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        ContentError::new(format!(
                            "missing embedded {asset_type} asset {asset_key}"
                        ))
                    })
            },
        )
    }

    fn from_source_loader(
        catalog_source: &str,
        mut load: impl FnMut(&str, &str) -> Result<String, ContentError>,
    ) -> Result<Self, ContentError> {
        let catalog: RuntimeCatalog = serde_json::from_str(catalog_source)
            .map_err(|error| ContentError::new(format!("cannot parse world catalog: {error}")))?;
        let mut source_cache = HashMap::new();
        for asset in &catalog.assets {
            if !matches!(
                asset.asset_type.as_str(),
                "character" | "weapons" | "symbols"
            ) {
                continue;
            }
            source_cache.insert(
                asset.asset_key.clone(),
                load(&asset.asset_type, &asset.asset_key)?,
            );
        }

        let mut characters = HashMap::new();
        let mut hammer = None;
        for asset in &catalog.assets {
            if asset.asset_type != "character" && asset.asset_type != "weapons" {
                continue;
            }
            let source = source_cache.get(&asset.asset_key).ok_or_else(|| {
                ContentError::new(format!("missing loaded source for {}", asset.asset_key))
            })?;
            let mut manifest: RuntimeManifest = serde_json::from_str(source).map_err(|error| {
                ContentError::new(format!("cannot parse {}: {error}", asset.asset_key))
            })?;
            resolve_asset_references(&mut manifest, &source_cache)?;
            if asset.asset_type == "character" {
                validate_character_manifest(&manifest, &asset.asset_key)?;
                let id = CharacterId::new(asset.asset_key.clone()).ok_or_else(|| {
                    ContentError::new("character catalog contains an empty asset key")
                })?;
                characters.insert(id, manifest);
            } else if asset.asset_key == HAMMER_ASSET_KEY {
                validate_hammer_manifest(&manifest)?;
                hammer = Some(manifest);
            }
        }

        if characters.is_empty() {
            return Err(ContentError::new(
                "character catalog contains no loadable character manifests",
            ));
        }
        let hammer = hammer.ok_or_else(|| ContentError::new("catalog is missing Hammer"))?;
        Ok(Self { characters, hammer })
    }

    pub fn ids(&self) -> Vec<CharacterId> {
        let mut ids = self.characters.keys().cloned().collect::<Vec<_>>();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        ids
    }

    pub fn contains_character(&self, character: &CharacterId) -> bool {
        self.characters.contains_key(character)
    }

    pub fn character(&self, character: &CharacterId) -> Option<&RuntimeManifest> {
        self.characters.get(character)
    }

    pub fn characters(&self) -> impl Iterator<Item = (&CharacterId, &RuntimeManifest)> {
        self.characters.iter()
    }

    pub fn hammer(&self) -> &RuntimeManifest {
        &self.hammer
    }
}

#[derive(Deserialize)]
struct RuntimeCatalog {
    assets: Vec<RuntimeCatalogAsset>,
}

#[derive(Deserialize)]
struct RuntimeCatalogAsset {
    asset_key: String,
    asset_type: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthoredFacing {
    Left,
    Right,
    #[default]
    Neutral,
    Top,
    Down,
}

#[derive(Clone, Default, Deserialize)]
pub struct RuntimePresentation {
    #[serde(default)]
    pub authored_facing: AuthoredFacing,
}

#[derive(Clone, Deserialize)]
pub struct RuntimeManifest {
    pub schema_version: u32,
    pub asset_key: String,
    pub asset_type: String,
    pub asset_pivot: [f32; 2],
    #[serde(default)]
    pub presentation: RuntimePresentation,
    pub components: Vec<RuntimeComponent>,
    #[serde(default)]
    pub attachment_frames: Vec<RuntimeAttachmentFrame>,
}

#[derive(Clone, Deserialize)]
pub struct RuntimeAttachmentFrame {
    pub frame_id: String,
    pub role: String,
    pub asset_transform: RuntimeFrameTransform,
}

#[derive(Clone, Copy, Deserialize)]
pub struct RuntimeFrameTransform {
    pub position: [f32; 2],
    pub rotation_radians: f32,
}

#[derive(Clone, Deserialize)]
struct RuntimeSymbolManifest {
    #[serde(alias = "asset_key")]
    key: String,
    #[serde(rename = "type", alias = "asset_type")]
    asset_kind: String,
    #[serde(default)]
    components: Vec<RuntimeComponent>,
}

#[derive(Clone, Deserialize)]
pub struct RuntimeComponent {
    pub component_id: String,
    pub name: String,
    pub parent_component_id: Option<String>,
    pub z_index: i32,
    #[serde(default)]
    pub component_pivot: Option<[f32; 2]>,
    #[serde(default)]
    pub local_pivot: Option<[f32; 2]>,
    pub local_transform: RuntimeTransform,
    #[serde(default = "default_projection_depth_meters")]
    pub projection_depth_meters: f32,
    pub mesh: Option<RuntimeMesh>,
    #[serde(default)]
    pub closed_region_mesh: Option<RuntimeRegionMesh>,
    #[serde(default)]
    pub contour_stroke_mesh: Option<RuntimeStrokeMesh>,
    #[serde(default)]
    pub source_asset_key: Option<String>,
    #[serde(skip)]
    pub referenced_components: Vec<RuntimeComponent>,
}

fn default_projection_depth_meters() -> f32 {
    0.1
}

#[derive(Clone, Deserialize)]
pub struct RuntimeTransform {
    pub position: [f32; 2],
    pub rotation_radians: f32,
    pub scale: [f32; 2],
}

#[derive(Clone, Deserialize)]
pub struct RuntimeMesh {
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Deserialize)]
pub struct RuntimeRegionMesh {
    pub role: String,
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Deserialize)]
pub struct RuntimeStrokeMesh {
    pub has_outline: bool,
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    #[serde(default)]
    pub runs: Vec<RuntimeStrokeRun>,
}

#[derive(Clone, Deserialize)]
pub struct RuntimeStrokeRun {
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentError(String);

impl ContentError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ContentError {}

fn resolve_asset_references(
    manifest: &mut RuntimeManifest,
    sources: &HashMap<String, String>,
) -> Result<(), ContentError> {
    for component in &mut manifest.components {
        if component.mesh.is_some() || component.contour_stroke_mesh.is_some() {
            continue;
        }
        let Some(source_key) = component.source_asset_key.as_deref() else {
            continue;
        };
        let source = sources
            .get(source_key)
            .ok_or_else(|| ContentError::new(format!("missing referenced asset {source_key}")))?;
        let symbol: RuntimeSymbolManifest = serde_json::from_str(source).map_err(|error| {
            ContentError::new(format!(
                "cannot parse referenced asset {source_key}: {error}"
            ))
        })?;
        if symbol.key != source_key || symbol.asset_kind != "symbols" {
            return Err(ContentError::new(format!(
                "referenced asset {source_key} is not a symbols manifest"
            )));
        }
        component.referenced_components = symbol.components;
    }
    Ok(())
}

fn validate_character_manifest(
    manifest: &RuntimeManifest,
    expected_key: &str,
) -> Result<(), ContentError> {
    if manifest.schema_version != 13 {
        return Err(ContentError::new(format!(
            "{} uses unsupported schema {}",
            manifest.asset_key, manifest.schema_version
        )));
    }
    if manifest.asset_key != expected_key || manifest.asset_type != "character" {
        return Err(ContentError::new(
            "manifest asset key does not match its package",
        ));
    }
    validate_asset_contents(manifest)?;
    if manifest.schema_version >= 9 && expected_key == "hammerer" {
        attachment_frame(manifest, WEAPON_SOCKET_ROLE)?;
    }
    for component in &manifest.components {
        if manifest.schema_version >= 8
            && expected_key != "barde"
            && (component.name == "eye_left" || component.name == "eye_right")
            && component.closed_region_mesh.is_none()
        {
            return Err(ContentError::new(format!(
                "{} is missing its schema-8+ closed eye region",
                component.component_id
            )));
        }
    }
    Ok(())
}

fn validate_hammer_manifest(manifest: &RuntimeManifest) -> Result<(), ContentError> {
    if manifest.schema_version != 13
        || manifest.asset_key != HAMMER_ASSET_KEY
        || manifest.asset_type != "weapons"
    {
        return Err(ContentError::new(
            "Hammer must be a schema-11 or schema-12 weapons manifest",
        ));
    }
    validate_asset_contents(manifest)?;
    attachment_frame(manifest, WEAPON_GRIP_ROLE)?;
    attachment_frame(manifest, WEAPON_SECONDARY_GRIP_ROLE)?;
    attachment_frame(manifest, WEAPON_ATTACK_POINT_ROLE)?;
    attachment_frame(manifest, WEAPON_REACH_LIMIT_ROLE)?;
    Ok(())
}

pub(crate) fn attachment_frame<'a>(
    manifest: &'a RuntimeManifest,
    role: &str,
) -> Result<&'a RuntimeAttachmentFrame, ContentError> {
    let mut matches = manifest
        .attachment_frames
        .iter()
        .filter(|frame| frame.role == role);
    let Some(frame) = matches.next() else {
        return Err(ContentError::new(format!(
            "{} is missing attachment frame {role}",
            manifest.asset_key
        )));
    };
    if matches.next().is_some() {
        return Err(ContentError::new(format!(
            "{} has duplicate attachment frame {role}",
            manifest.asset_key
        )));
    }
    Ok(frame)
}

fn validate_asset_contents(manifest: &RuntimeManifest) -> Result<(), ContentError> {
    if !finite_pair(manifest.asset_pivot) || manifest.components.is_empty() {
        return Err(ContentError::new(format!(
            "{} has invalid asset metadata",
            manifest.asset_key
        )));
    }
    let component_ids = manifest
        .components
        .iter()
        .map(|component| component.component_id.as_str())
        .collect::<BTreeSet<_>>();
    let component_names = manifest
        .components
        .iter()
        .map(|component| component.name.as_str())
        .collect::<BTreeSet<_>>();
    if component_ids.len() != manifest.components.len()
        || component_names.len() != manifest.components.len()
    {
        return Err(ContentError::new(format!(
            "{} has duplicate component identities",
            manifest.asset_key
        )));
    }

    for component in &manifest.components {
        let pivot = component
            .component_pivot
            .or(component.local_pivot)
            .unwrap_or([0.0, 0.0]);
        if !finite_pair(pivot)
            || !finite_pair(component.local_transform.position)
            || !component.local_transform.rotation_radians.is_finite()
            || !finite_pair(component.local_transform.scale)
            || component.local_transform.scale.contains(&0.0)
        {
            return Err(ContentError::new(format!(
                "{} has invalid transform data",
                component.component_id
            )));
        }
        if let Some(parent) = component.parent_component_id.as_deref()
            && !component_ids.contains(parent)
        {
            return Err(ContentError::new(format!(
                "{} has an unknown parent",
                component.component_id
            )));
        }
        if let Some(mesh) = component.mesh.as_ref() {
            validate_mesh_parts(&mesh.vertices, &mesh.indices, &component.component_id)?;
        }
        if let Some(region) = component.closed_region_mesh.as_ref() {
            if region.role != "closed_contour_region" {
                return Err(ContentError::new(format!(
                    "{} has an invalid closed region role",
                    component.component_id
                )));
            }
            validate_mesh_parts(&region.vertices, &region.indices, &component.component_id)?;
        }
        if let Some(stroke) = component.contour_stroke_mesh.as_ref() {
            if stroke.has_outline {
                validate_mesh_parts(&stroke.vertices, &stroke.indices, &component.component_id)?;
            } else if !stroke.vertices.is_empty() || !stroke.indices.is_empty() {
                return Err(ContentError::new(format!(
                    "{} has a disabled outline with geometry",
                    component.component_id
                )));
            }
        }
    }

    let frame_ids = manifest
        .attachment_frames
        .iter()
        .map(|frame| frame.frame_id.as_str())
        .collect::<BTreeSet<_>>();
    let frame_roles = manifest
        .attachment_frames
        .iter()
        .map(|frame| frame.role.as_str())
        .collect::<BTreeSet<_>>();
    if frame_ids.len() != manifest.attachment_frames.len()
        || frame_roles.len() != manifest.attachment_frames.len()
        || manifest.attachment_frames.iter().any(|frame| {
            frame.frame_id.is_empty()
                || frame.role.is_empty()
                || !finite_pair(frame.asset_transform.position)
                || !frame.asset_transform.rotation_radians.is_finite()
        })
    {
        return Err(ContentError::new(format!(
            "{} has invalid attachment frames",
            manifest.asset_key
        )));
    }

    Ok(())
}

fn validate_mesh_parts(
    vertices: &[[f32; 2]],
    indices: &[u32],
    component_id: &str,
) -> Result<(), ContentError> {
    if vertices.is_empty()
        || indices.is_empty()
        || indices.len() % 3 != 0
        || vertices.iter().any(|vertex| !finite_pair(*vertex))
        || indices
            .iter()
            .any(|index| *index as usize >= vertices.len())
    {
        return Err(ContentError::new(format!(
            "{component_id} has invalid mesh geometry"
        )));
    }
    Ok(())
}

fn finite_pair(values: [f32; 2]) -> bool {
    values.into_iter().all(f32::is_finite)
}

fn embedded_manifest(asset_type: &str, asset_key: &str) -> Option<&'static str> {
    match (asset_type, asset_key) {
        ("character", "archerf") => Some(include_str!(
            "../../../assets/characters/archerf/manifest.json"
        )),
        ("character", "barde") => Some(include_str!(
            "../../../assets/characters/barde/manifest.json"
        )),
        ("character", "chantres") => Some(include_str!(
            "../../../assets/characters/chantres/manifest.json"
        )),
        ("character", "glavier") => Some(include_str!(
            "../../../assets/characters/glavier/manifest.json"
        )),
        ("character", "hammerer") => Some(include_str!(
            "../../../assets/characters/hammerer/manifest.json"
        )),
        ("character", "mage") => Some(include_str!(
            "../../../assets/characters/mage/manifest.json"
        )),
        ("character", "monk") => Some(include_str!(
            "../../../assets/characters/monk/manifest.json"
        )),
        ("character", "rogue") => Some(include_str!(
            "../../../assets/characters/rogue/manifest.json"
        )),
        ("character", "sorcerer") => Some(include_str!(
            "../../../assets/characters/sorcerer/manifest.json"
        )),
        ("character", "warrior") => Some(include_str!(
            "../../../assets/characters/warrior/manifest.json"
        )),
        ("character", "wizard") => Some(include_str!(
            "../../../assets/characters/wizard/manifest.json"
        )),
        ("props", "ankh") => Some(include_str!("../../../assets/props/ankh/manifest.json")),
        ("props", "tree") => Some(include_str!("../../../assets/props/tree/manifest.json")),
        ("weapons", "hammer") => Some(include_str!("../../../assets/weapons/hammer/manifest.json")),
        ("symbols", "heart") => Some(include_str!("../../../assets/symbols/heart/manifest.json")),
        ("symbols", "orb") => Some(include_str!("../../../assets/symbols/orb/manifest.json")),
        ("symbols", "plus") => Some(include_str!("../../../assets/symbols/plus/manifest.json")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_content_loads_every_catalogued_character_and_hammer() {
        let content = RuntimeContent::load_embedded().expect("embedded PolyTools content is valid");

        assert_eq!(content.ids().len(), 11);
        assert!(content.contains_character(&CharacterId("hammerer".into())));
        assert_eq!(content.hammer().asset_key, HAMMER_ASSET_KEY);
    }
}
