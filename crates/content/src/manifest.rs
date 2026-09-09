use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    fmt,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use world01_world_data::CharacterId;

include!(concat!(env!("OUT_DIR"), "/embedded_asset_manifests.rs"));

pub const HAMMER_ASSET_KEY: &str = "hammer";
pub const WEAPON_SOCKET_ROLE: &str = "weapon_socket_primary";
pub const WEAPON_GRIP_ROLE: &str = "grip_primary";
pub const WEAPON_SECONDARY_GRIP_ROLE: &str = "grip_secondary";
pub const WEAPON_ATTACK_POINT_ROLE: &str = "attack_point_primary";
pub const WEAPON_REACH_LIMIT_ROLE: &str = "reach_limit_primary";
pub const RUNTIME_MANIFEST_SCHEMA_VERSION: u32 = 21;
pub const REGION_GEOMETRY_AUTHORED: &str = "authored";
pub const REGION_GEOMETRY_COMPONENT: &str = "component";

#[derive(Resource, Clone)]
pub struct RuntimeContent {
    characters: HashMap<CharacterId, RuntimeManifest>,
    props: HashMap<String, RuntimeManifest>,
    terrain: HashMap<String, RuntimeManifest>,
    weapons: HashMap<String, RuntimeManifest>,
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
                "character" | "props" | "weapons" | "terrain" | "symbols"
            ) {
                continue;
            }
            source_cache.insert(
                asset.asset_key.clone(),
                load(&asset.asset_type, &asset.asset_key)?,
            );
        }

        let mut characters = HashMap::new();
        let mut props = HashMap::new();
        let mut terrain = HashMap::new();
        let mut weapons = HashMap::new();
        let mut hammer = None;
        for asset in &catalog.assets {
            if !matches!(
                asset.asset_type.as_str(),
                "character" | "props" | "weapons" | "terrain"
            ) {
                continue;
            }
            let source = source_cache.get(&asset.asset_key).ok_or_else(|| {
                ContentError::new(format!("missing loaded source for {}", asset.asset_key))
            })?;
            let mut manifest: RuntimeManifest = serde_json::from_str(source).map_err(|error| {
                ContentError::new(format!("cannot parse {}: {error}", asset.asset_key))
            })?;
            if manifest.schema_version != RUNTIME_MANIFEST_SCHEMA_VERSION {
                return Err(ContentError::new(format!(
                    "{} uses unsupported schema {}",
                    manifest.asset_key, manifest.schema_version
                )));
            }
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
            } else if asset.asset_type == "props" {
                validate_asset_contents(&manifest)?;
                props.insert(asset.asset_key.clone(), manifest);
            } else if asset.asset_type == "terrain" {
                validate_asset_contents(&manifest)?;
                terrain.insert(asset.asset_key.clone(), manifest);
            } else if asset.asset_type == "weapons" {
                validate_asset_contents(&manifest)?;
                weapons.insert(asset.asset_key.clone(), manifest);
            }
        }

        if characters.is_empty() {
            return Err(ContentError::new(
                "character catalog contains no loadable character manifests",
            ));
        }
        validate_palette_variants(&props)?;
        validate_palette_variants(&terrain)?;
        let hammer = hammer.ok_or_else(|| ContentError::new("catalog is missing Hammer"))?;
        Ok(Self {
            characters,
            props,
            terrain,
            weapons,
            hammer,
        })
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

    pub fn prop(&self, asset_key: &str) -> Option<&RuntimeManifest> {
        self.props.get(asset_key)
    }

    pub fn terrain(&self, asset_key: &str) -> Option<&RuntimeManifest> {
        self.terrain.get(asset_key)
    }

    /// A catalogued weapon other than the Hammer, which has its own accessor
    /// because the sandbox binds it by name.
    pub fn weapon(&self, asset_key: &str) -> Option<&RuntimeManifest> {
        self.weapons.get(asset_key)
    }

    /// The terrain Asset that a cell actually draws.
    ///
    /// A Single draws itself. A Palette draws one of the interchangeable
    /// Singles it names, and which one is a plain function of the cell's
    /// coordinates: every client derives the same choice, nobody sends it and
    /// nothing stores it, so the world looks the same everywhere and again
    /// after a rebuild.
    pub fn terrain_variant(&self, asset_key: &str, x: u32, y: u32) -> Option<&RuntimeManifest> {
        let manifest = self.terrain.get(asset_key)?;
        let RuntimeComposition::Palette { variants } = &manifest.composition else {
            return Some(manifest);
        };
        let count = u32::try_from(variants.len()).ok()?.max(1);
        let variant = variants.get((cell_variant_mix(x, y) % count) as usize)?;
        self.terrain
            .get(variant.as_str())
            .filter(|chosen| matches!(chosen.composition, RuntimeComposition::Single))
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

/// What an Asset is made of.
///
/// PolyTools schema 19 sorts every Asset into one of three kinds, and only a
/// Palette carries `variants`. Keeping the two together in one value makes a
/// Single that names variants, and a Palette that names none, unrepresentable
/// past this boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeComposition {
    /// One Asset that draws itself.
    Single,
    /// One Asset assembled from other Assets, each named by an Asset Reference.
    Set,
    /// A choice between interchangeable Single Assets. Carries no geometry.
    Palette { variants: Vec<String> },
}

#[derive(Clone, Deserialize)]
#[serde(try_from = "RawRuntimeManifest")]
pub struct RuntimeManifest {
    pub schema_version: u32,
    /// The identity PolyTools keeps across a rename. Opaque: it is compared,
    /// never parsed, and world01 stores nothing by it today.
    pub asset_id: String,
    pub asset_key: String,
    pub asset_type: String,
    pub asset_pivot: [f32; 2],
    pub composition: RuntimeComposition,
    pub presentation: RuntimePresentation,
    pub components: Vec<RuntimeComponent>,
    pub attachment_frames: Vec<RuntimeAttachmentFrame>,
    pub regions: Vec<RuntimeRegion>,
}

impl RuntimeManifest {
    /// The Singles a Palette chooses between; empty for every other Asset.
    pub fn variants(&self) -> &[String] {
        match &self.composition {
            RuntimeComposition::Palette { variants } => variants,
            RuntimeComposition::Single | RuntimeComposition::Set => &[],
        }
    }

    pub fn is_palette(&self) -> bool {
        matches!(self.composition, RuntimeComposition::Palette { .. })
    }
}

#[derive(Deserialize)]
struct RawRuntimeManifest {
    schema_version: u32,
    asset_id: String,
    asset_key: String,
    asset_type: String,
    asset_category: String,
    asset_pivot: [f32; 2],
    #[serde(default)]
    presentation: RuntimePresentation,
    components: Vec<RuntimeComponent>,
    #[serde(default)]
    variants: Option<Vec<String>>,
    #[serde(default)]
    attachment_frames: Vec<RuntimeAttachmentFrame>,
    #[serde(default)]
    regions: Vec<RuntimeRegion>,
}

impl TryFrom<RawRuntimeManifest> for RuntimeManifest {
    type Error = String;

    fn try_from(raw: RawRuntimeManifest) -> Result<Self, Self::Error> {
        if raw.asset_id.is_empty() {
            return Err(format!("{} carries no asset identity", raw.asset_key));
        }
        let composition = match (raw.asset_category.as_str(), raw.variants) {
            ("single", None) => RuntimeComposition::Single,
            ("set", None) => RuntimeComposition::Set,
            ("palette", Some(variants)) => RuntimeComposition::Palette { variants },
            ("palette", None) => {
                return Err(format!("Palette {} names no variants", raw.asset_key));
            }
            ("single" | "set", Some(_)) => {
                return Err(format!(
                    "{} is a {} and must not name variants",
                    raw.asset_key, raw.asset_category
                ));
            }
            (category, _) => {
                return Err(format!(
                    "{} has unknown asset category {category}",
                    raw.asset_key
                ));
            }
        };
        Ok(Self {
            schema_version: raw.schema_version,
            asset_id: raw.asset_id,
            asset_key: raw.asset_key,
            asset_type: raw.asset_type,
            asset_pivot: raw.asset_pivot,
            composition,
            presentation: raw.presentation,
            components: raw.components,
            attachment_frames: raw.attachment_frames,
            regions: raw.regions,
        })
    }
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
    #[serde(default)]
    pub projection_depth_corners: Option<Vec<RuntimeProjectionDepthCorner>>,
    pub mesh: Option<RuntimeMesh>,
    #[serde(default)]
    pub closed_region_mesh: Option<RuntimeRegionMesh>,
    #[serde(default)]
    pub contour_stroke_mesh: Option<RuntimeStrokeMesh>,
    #[serde(default)]
    pub source_asset_key: Option<String>,
    #[serde(default)]
    pub source_asset_id: Option<String>,
    #[serde(skip)]
    pub referenced_components: Vec<RuntimeComponent>,
}

fn default_projection_depth_meters() -> f32 {
    0.1
}

#[derive(Clone, Deserialize)]
pub struct RuntimeProjectionDepthCorner {
    pub point_id: String,
    pub position: [f32; 2],
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

/// An authored Region: a named piece of an Asset with a gameplay role.
///
/// Since schema 16 a Region does not have to carry its own shape.
/// `geometry_source` says where the shape comes from - `authored` means the
/// `vertices` and `indices` below, `component` means the mesh of
/// `source_component_id`, in that Component's own frame.
#[derive(Clone, Deserialize)]
pub struct RuntimeRegion {
    pub region_id: String,
    pub name: String,
    pub role: String,
    pub geometry_source: String,
    pub source_component_id: String,
    #[serde(default)]
    pub vertices: Vec<[f32; 2]>,
    #[serde(default)]
    pub indices: Vec<u32>,
}

impl RuntimeRegion {
    pub fn is_authored_geometry(&self) -> bool {
        self.geometry_source == REGION_GEOMETRY_AUTHORED
    }
}

#[derive(Clone, Deserialize)]
pub struct RuntimeStrokeMesh {
    pub has_outline: bool,
    pub stroke_width_meters: f32,
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
        let referenced: RuntimeManifest = serde_json::from_str(source).map_err(|error| {
            ContentError::new(format!(
                "cannot parse referenced asset {source_key}: {error}"
            ))
        })?;
        if referenced.schema_version != RUNTIME_MANIFEST_SCHEMA_VERSION {
            return Err(ContentError::new(format!(
                "referenced asset {source_key} uses unsupported schema {}",
                referenced.schema_version
            )));
        }
        if referenced.asset_key != source_key
            || !matches!(referenced.composition, RuntimeComposition::Single)
        {
            return Err(ContentError::new(format!(
                "referenced asset {source_key} is not a Single Asset"
            )));
        }
        // The Key reads well, the identity is what points. A Reference that
        // names one and misses the other has survived a rename only halfway.
        if component.source_asset_id.as_deref() != Some(referenced.asset_id.as_str()) {
            return Err(ContentError::new(format!(
                "referenced asset {source_key} does not carry the identity the reference names"
            )));
        }
        component.referenced_components = referenced.components;
    }
    Ok(())
}

fn validate_character_manifest(
    manifest: &RuntimeManifest,
    expected_key: &str,
) -> Result<(), ContentError> {
    if manifest.asset_key != expected_key
        || manifest.asset_type != "character"
        || !matches!(manifest.composition, RuntimeComposition::Single)
    {
        return Err(ContentError::new(
            "manifest asset key does not match its package",
        ));
    }
    validate_asset_contents(manifest)?;
    if expected_key == "hammerer" {
        attachment_frame(manifest, WEAPON_SOCKET_ROLE)?;
    }
    Ok(())
}

fn validate_hammer_manifest(manifest: &RuntimeManifest) -> Result<(), ContentError> {
    if manifest.asset_key != HAMMER_ASSET_KEY
        || manifest.asset_type != "weapons"
        || !matches!(manifest.composition, RuntimeComposition::Single)
    {
        return Err(ContentError::new("Hammer must be a single weapons Asset"));
    }
    validate_asset_contents(manifest)?;
    attachment_frame(manifest, WEAPON_GRIP_ROLE)?;
    attachment_frame(manifest, WEAPON_SECONDARY_GRIP_ROLE)?;
    attachment_frame(manifest, WEAPON_ATTACK_POINT_ROLE)?;
    attachment_frame(manifest, WEAPON_REACH_LIMIT_ROLE)?;
    if !manifest
        .regions
        .iter()
        .any(|region| region.role == "attack")
    {
        return Err(ContentError::new(
            "Hammer must author at least one attack Region",
        ));
    }
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
    if !finite_pair(manifest.asset_pivot) {
        return Err(ContentError::new(format!(
            "{} has invalid asset metadata",
            manifest.asset_key
        )));
    }
    if let RuntimeComposition::Palette { variants } = &manifest.composition {
        return validate_palette_contents(manifest, variants);
    }
    if manifest.components.is_empty() {
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

    let region_ids = manifest
        .regions
        .iter()
        .map(|region| region.region_id.as_str())
        .collect::<BTreeSet<_>>();
    let region_names = manifest
        .regions
        .iter()
        .map(|region| region.name.as_str())
        .collect::<BTreeSet<_>>();
    if region_ids.len() != manifest.regions.len()
        || region_names.len() != manifest.regions.len()
        || manifest.regions.iter().any(|region| {
            region.region_id.is_empty()
                || region.name.is_empty()
                || !matches!(region.role.as_str(), "attack" | "hurt" | "collision")
                || !matches!(
                    region.geometry_source.as_str(),
                    REGION_GEOMETRY_AUTHORED | REGION_GEOMETRY_COMPONENT
                )
                || region.source_component_id.is_empty()
                || (region.is_authored_geometry() && !valid_region_mesh(region))
        })
    {
        return Err(ContentError::new(format!(
            "{} has invalid authored Regions",
            manifest.asset_key
        )));
    }

    for component in &manifest.components {
        if component.source_asset_key.is_none() && component.projection_depth_corners.is_none() {
            return Err(ContentError::new(format!(
                "{} is missing projection-depth Corner Points",
                component.component_id
            )));
        }
        if let Some(corners) = component.projection_depth_corners.as_ref() {
            let corner_ids = corners
                .iter()
                .map(|corner| corner.point_id.as_str())
                .collect::<BTreeSet<_>>();
            if corner_ids.len() != corners.len()
                || corners
                    .iter()
                    .any(|corner| corner.point_id.is_empty() || !finite_pair(corner.position))
            {
                return Err(ContentError::new(format!(
                    "{} has invalid projection-depth Corner Points",
                    component.component_id
                )));
            }
        }
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
            if !stroke.stroke_width_meters.is_finite() || stroke.stroke_width_meters <= 0.0 {
                return Err(ContentError::new(format!(
                    "{} has an invalid contour stroke width",
                    component.component_id
                )));
            }
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

/// A Palette is a choice, not a drawing: it names Singles and carries nothing
/// that could be drawn, attached to, or collided with.
fn validate_palette_contents(
    manifest: &RuntimeManifest,
    variants: &[String],
) -> Result<(), ContentError> {
    let unique = variants.iter().collect::<BTreeSet<_>>();
    if variants.is_empty()
        || unique.len() != variants.len()
        || variants.iter().any(|variant| variant.is_empty())
        || !manifest.components.is_empty()
        || !manifest.attachment_frames.is_empty()
        || !manifest.regions.is_empty()
    {
        return Err(ContentError::new(format!(
            "{} is not a usable Palette",
            manifest.asset_key
        )));
    }
    Ok(())
}

/// Every variant a Palette names has to be a loaded Single of the same Asset
/// type, so choosing one can never end in an empty drawing.
fn validate_palette_variants(
    assets: &HashMap<String, RuntimeManifest>,
) -> Result<(), ContentError> {
    for manifest in assets.values() {
        for variant in manifest.variants() {
            let usable = assets.get(variant.as_str()).is_some_and(|candidate| {
                matches!(candidate.composition, RuntimeComposition::Single)
                    && candidate.asset_type == manifest.asset_type
            });
            if !usable {
                return Err(ContentError::new(format!(
                    "Palette {} names {variant}, which is not a loaded single {} Asset",
                    manifest.asset_key, manifest.asset_type
                )));
            }
        }
    }
    Ok(())
}

/// Mixes a cell's coordinates into the index of the Palette member it shows.
///
/// Plain arithmetic on the coordinates alone - no randomness, no session salt,
/// nothing kept - so two clients looking at the same cell see the same Asset.
fn cell_variant_mix(x: u32, y: u32) -> u32 {
    let mut mixed = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA6B);
    mixed ^= mixed >> 15;
    mixed = mixed.wrapping_mul(0x2545_F491);
    mixed ^ (mixed >> 13)
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

/// Only an `authored` Region carries a mesh here. A `component` Region is
/// checked where its Component is resolved, because that is where the
/// Component is within reach.
fn valid_region_mesh(region: &RuntimeRegion) -> bool {
    !region.vertices.iter().any(|vertex| !finite_pair(*vertex))
        && !region.indices.is_empty()
        && region.indices.len() % 3 == 0
        && !region
            .indices
            .iter()
            .any(|index| *index as usize >= region.vertices.len())
}

/// Looks up one embedded Asset package.
///
/// The table it reads is generated from the world catalog at build time, so a
/// newly catalogued Asset arrives with the sync rather than with an edit here,
/// and a catalogued Asset without a package fails the build naming the file.
fn embedded_manifest(asset_type: &str, asset_key: &str) -> Option<&'static str> {
    EMBEDDED_ASSET_MANIFESTS
        .iter()
        .find(|(embedded_type, embedded_key, _)| {
            *embedded_type == asset_type && *embedded_key == asset_key
        })
        .map(|(_, _, source)| *source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_content_loads_every_catalogued_character_and_hammer() {
        let content = RuntimeContent::load_embedded().expect("embedded PolyTools content is valid");

        // Counted from the catalogue rather than written down: a Character
        // added in PolyTools should make this test load one more, not fail.
        let catalogued = EMBEDDED_ASSET_MANIFESTS
            .iter()
            .filter(|(asset_type, _, _)| *asset_type == "character")
            .count();
        assert!(
            catalogued >= 11,
            "the world carries its authored Characters"
        );
        assert_eq!(content.ids().len(), catalogued);
        assert!(content.contains_character(&CharacterId("hammerer".into())));
        assert_eq!(content.hammer().asset_key, HAMMER_ASSET_KEY);
        assert_eq!(
            content.prop("ankh").map(|prop| prop.asset_key.as_str()),
            Some("ankh")
        );
        assert_eq!(
            content
                .terrain("grass")
                .map(|terrain| terrain.asset_key.as_str()),
            Some("grass")
        );
    }

    #[test]
    fn embedded_ankh_body_exposes_its_authored_projection_depth_corners() {
        let content = RuntimeContent::load_embedded().expect("embedded PolyTools content is valid");
        let ankh = content.prop("ankh").expect("embedded Ankh prop is present");
        let body = ankh
            .components
            .iter()
            .find(|component| component.name == "body")
            .expect("Ankh has a body Component");
        let corners = body
            .projection_depth_corners
            .as_ref()
            .expect("ordinary Components export projection-depth Corner Points");

        assert_eq!(corners.len(), 12);
        assert!(
            corners
                .iter()
                .all(|corner| { !corner.point_id.is_empty() && finite_pair(corner.position) })
        );
    }

    #[test]
    fn embedded_grass_is_a_palette_over_loaded_single_assets() {
        let content = RuntimeContent::load_embedded().expect("embedded PolyTools content is valid");
        let grass = content.terrain("grass").expect("grass is catalogued");

        assert!(grass.is_palette());
        assert!(grass.components.is_empty());
        assert!(grass.regions.is_empty());
        assert!(!grass.variants().is_empty());
        for variant in grass.variants() {
            let single = content
                .terrain(variant)
                .expect("a Palette only names loaded Assets");
            assert_eq!(single.composition, RuntimeComposition::Single);
            assert_eq!(single.asset_type, grass.asset_type);
        }
    }

    #[test]
    fn a_palette_cell_resolves_to_one_single_and_keeps_resolving_to_it() {
        let content = RuntimeContent::load_embedded().expect("embedded PolyTools content is valid");
        let grass = content.terrain("grass").expect("grass is catalogued");
        let mut chosen_keys = BTreeSet::new();

        for y in 0..16 {
            for x in 0..16 {
                let chosen = content
                    .terrain_variant("grass", x, y)
                    .expect("a Palette cell resolves to a Single");
                assert_eq!(chosen.composition, RuntimeComposition::Single);
                assert!(!chosen.components.is_empty());
                assert!(grass.variants().contains(&chosen.asset_key));
                let again = content
                    .terrain_variant("grass", x, y)
                    .expect("the same cell resolves again");
                assert_eq!(chosen.asset_key, again.asset_key);
                chosen_keys.insert(chosen.asset_key.clone());
            }
        }

        assert_eq!(chosen_keys.len(), grass.variants().len());
    }

    #[test]
    fn an_asset_reference_points_at_an_identity_and_not_only_at_a_name() {
        let content = RuntimeContent::load_embedded().expect("embedded PolyTools content is valid");
        let bridge = content.prop("bridge").expect("Bridge is catalogued");
        let post = content.prop("post").expect("Post is catalogued");

        assert_eq!(bridge.composition, RuntimeComposition::Set);
        assert!(!post.asset_id.is_empty());
        let member = bridge
            .components
            .iter()
            .find(|component| component.source_asset_key.as_deref() == Some("post"))
            .expect("the Bridge names a Post member");

        assert_eq!(
            member.source_asset_id.as_deref(),
            Some(post.asset_id.as_str())
        );
        assert!(
            !member.referenced_components.is_empty(),
            "a resolved Reference carries the Components of the Asset it instances"
        );
    }

    #[test]
    fn a_single_terrain_cell_resolves_to_itself() {
        let content = RuntimeContent::load_embedded().expect("embedded PolyTools content is valid");

        assert_eq!(
            content
                .terrain_variant("grass01", 7, 3)
                .map(|manifest| manifest.asset_key.as_str()),
            Some("grass01")
        );
    }

    #[test]
    fn a_single_cannot_name_variants_and_a_palette_cannot_omit_them() {
        assert!(
            serde_json::from_str::<RuntimeManifest>(&probe_manifest(
                "single",
                ",\"variants\":[\"grass01\"]"
            ))
            .is_err()
        );
        assert!(serde_json::from_str::<RuntimeManifest>(&probe_manifest("palette", "")).is_err());

        let palette: RuntimeManifest =
            serde_json::from_str(&probe_manifest("palette", ",\"variants\":[\"grass01\"]"))
                .expect("a Palette that names variants parses");

        assert_eq!(
            palette.composition,
            RuntimeComposition::Palette {
                variants: vec!["grass01".to_owned()],
            }
        );
    }

    fn probe_manifest(asset_category: &str, variants: &str) -> String {
        format!(
            "{{\"schema_version\":{RUNTIME_MANIFEST_SCHEMA_VERSION},\
             \"asset_id\":\"asset_probe\",\"asset_key\":\"probe\",\
             \"asset_type\":\"terrain\",\"asset_category\":\"{asset_category}\",\
             \"asset_pivot\":[0.0,0.0],\"components\":[]{variants}}}"
        )
    }
}
