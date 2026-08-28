use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    fmt, fs,
    path::Path,
};

use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use game01_world_data::CharacterId;
use serde::Deserialize;

use crate::eyes::{EyeCollider, EyePupil, PupilGeometry};
use crate::pose::CharacterHead;

#[derive(Component)]
pub struct BodyAnchor;

#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterVisual {
    pub orientation_root: Entity,
    pub authored_facing: AuthoredFacing,
}

#[derive(Component)]
pub struct CharacterVisualOrientation;

#[derive(Resource, Clone)]
pub struct CharacterAssetLibrary {
    characters: HashMap<CharacterId, PolyToolsManifest>,
    pupil_area_ratio: f32,
    pupil_collision_reference_radius: f32,
}

impl CharacterAssetLibrary {
    #[cfg(test)]
    fn load_embedded() -> Result<Self, PolyToolsAssetError> {
        let design = game01_configs::load_embedded().map_err(|error| {
            PolyToolsAssetError::new(format!("cannot load embedded eye design: {error}"))
        })?;
        Self::load_from_directory(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/characters")
                .as_path(),
            design.eyes.pupil_area_ratio,
            design.eyes.hammerer_collision_radius_ratio,
        )
    }

    pub fn load_from_directory(
        directory: &Path,
        pupil_area_ratio: f32,
        hammerer_collision_radius_ratio: f32,
    ) -> Result<Self, PolyToolsAssetError> {
        if !pupil_area_ratio.is_finite() || pupil_area_ratio <= 0.0 || pupil_area_ratio >= 1.0 {
            return Err(PolyToolsAssetError::new(
                "pupil area ratio must be finite and between zero and one",
            ));
        }
        if !hammerer_collision_radius_ratio.is_finite()
            || hammerer_collision_radius_ratio <= 0.0
            || hammerer_collision_radius_ratio > 1.0
        {
            return Err(PolyToolsAssetError::new(
                "Hammerer collision radius ratio must be finite, greater than zero, and at most one",
            ));
        }
        let catalog_source =
            fs::read_to_string(directory.join("catalog.json")).map_err(|error| {
                PolyToolsAssetError::new(format!("cannot read character catalog: {error}"))
            })?;
        let catalog =
            game01_world_data::CharacterCatalog::from_json(&catalog_source).map_err(|error| {
                PolyToolsAssetError::new(format!("cannot parse character catalog: {error}"))
            })?;
        let mut characters = HashMap::new();
        for character in catalog.ids() {
            let source = fs::read_to_string(directory.join(&character.0).join("manifest.json"))
                .map_err(|error| {
                    PolyToolsAssetError::new(format!("cannot read {}: {error}", character.0))
                })?;
            let manifest: PolyToolsManifest = serde_json::from_str(&source).map_err(|error| {
                PolyToolsAssetError::new(format!("cannot parse {}: {error}", character.0))
            })?;
            let mut manifest = manifest;
            resolve_asset_references(&mut manifest, directory)?;
            validate_manifest(&manifest, &character.0)?;
            characters.insert(character.clone(), manifest);
        }

        if characters.is_empty() {
            return Err(PolyToolsAssetError::new(
                "character catalog contains no loadable character manifests",
            ));
        }

        let hammerer = characters
            .iter()
            .find_map(|(character, manifest)| (character.0 == "hammerer").then_some(manifest))
            .ok_or_else(|| PolyToolsAssetError::new("character catalog is missing Hammerer"))?;
        let hammerer_eye_region = hammerer
            .components
            .iter()
            .find(|component| component.name == "eye_left" || component.name == "eye_right")
            .and_then(|component| component.closed_region_mesh.as_ref())
            .ok_or_else(|| {
                PolyToolsAssetError::new(
                    "Hammerer is missing a closed eye region for pupil normalization",
                )
            })?;
        let hammerer_pupil_radius = EyeCollider::pupil_radius_from_region_mesh(
            &hammerer_eye_region.vertices,
            &hammerer_eye_region.indices,
            pupil_area_ratio,
        )
        .ok_or_else(|| PolyToolsAssetError::new("Hammerer eye region has invalid geometry"))?;
        let pupil_collision_reference_radius =
            hammerer_pupil_radius * hammerer_collision_radius_ratio;

        Ok(Self {
            characters,
            pupil_area_ratio,
            pupil_collision_reference_radius,
        })
    }

    pub fn ids(&self) -> Vec<CharacterId> {
        let mut ids = self.characters.keys().cloned().collect::<Vec<_>>();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        ids
    }

    fn character(&self, character: &CharacterId) -> Option<&PolyToolsManifest> {
        self.characters.get(character)
    }

    pub fn body_pivot(&self, character: &CharacterId) -> Vec2 {
        self.character(character)
            .and_then(|manifest| {
                manifest
                    .components
                    .iter()
                    .find(|component| component.name == "body")
            })
            .and_then(|component| component.component_pivot.or(component.local_pivot))
            .map(|pivot| Vec2::new(pivot[0], pivot[1]))
            .unwrap_or(Vec2::ZERO)
    }

    #[cfg(test)]
    fn authored_facing(&self, character: &CharacterId) -> AuthoredFacing {
        self.character(character)
            .map(|manifest| manifest.presentation.authored_facing)
            .unwrap_or_default()
    }
}

#[derive(Debug)]
pub struct PolyToolsAssetError(String);

impl PolyToolsAssetError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for PolyToolsAssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PolyToolsAssetError {}

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
struct PolyToolsPresentation {
    #[serde(default)]
    authored_facing: AuthoredFacing,
}

#[derive(Clone, Deserialize)]
struct PolyToolsManifest {
    schema_version: u32,
    asset_key: String,
    asset_type: String,
    asset_pivot: [f32; 2],
    #[serde(default)]
    presentation: PolyToolsPresentation,
    components: Vec<PolyToolsComponent>,
}

#[derive(Clone, Deserialize)]
struct PolyToolsSymbolManifest {
    #[serde(alias = "asset_key")]
    key: String,
    #[serde(rename = "type", alias = "asset_type")]
    asset_kind: String,
    #[serde(default)]
    components: Vec<PolyToolsComponent>,
}

#[derive(Clone, Deserialize)]
struct PolyToolsComponent {
    component_id: String,
    name: String,
    parent_component_id: Option<String>,
    z_index: i32,
    #[serde(default)]
    component_pivot: Option<[f32; 2]>,
    #[serde(default)]
    local_pivot: Option<[f32; 2]>,
    local_transform: PolyToolsTransform,
    mesh: Option<PolyToolsMesh>,
    #[serde(default)]
    closed_region_mesh: Option<PolyToolsRegionMesh>,
    #[serde(default)]
    contour_stroke_mesh: Option<PolyToolsStrokeMesh>,
    #[serde(default)]
    source_asset_key: Option<String>,
    #[serde(skip)]
    referenced_components: Vec<PolyToolsComponent>,
}

#[derive(Clone, Deserialize)]
struct PolyToolsTransform {
    position: [f32; 2],
    rotation_radians: f32,
    scale: [f32; 2],
}

#[derive(Clone, Deserialize)]
struct PolyToolsMesh {
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

#[derive(Clone, Deserialize)]
struct PolyToolsRegionMesh {
    role: String,
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

#[derive(Clone, Deserialize)]
struct PolyToolsStrokeMesh {
    has_outline: bool,
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
    #[serde(default)]
    runs: Vec<PolyToolsStrokeRun>,
}

#[derive(Clone, Deserialize)]
struct PolyToolsStrokeRun {
    closed: bool,
}

pub fn spawn_character_visual(
    commands: &mut Commands,
    root: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    library: &CharacterAssetLibrary,
    character: &CharacterId,
) -> Result<(), PolyToolsAssetError> {
    let manifest = library
        .character(character)
        .ok_or_else(|| PolyToolsAssetError::new("missing validated character manifest"))?;
    let orientation_root = commands
        .spawn((
            CharacterVisualOrientation,
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let anchor = commands
        .spawn((
            Transform::from_xyz(-manifest.asset_pivot[0], -manifest.asset_pivot[1], 0.0),
            Visibility::default(),
        ))
        .id();
    commands.entity(root).insert(CharacterVisual {
        orientation_root,
        authored_facing: manifest.presentation.authored_facing,
    });
    commands.entity(root).add_child(orientation_root);
    commands.entity(orientation_root).add_child(anchor);

    let mut component_entities = HashMap::new();
    for component in &manifest.components {
        let transform = component_transform(component);
        let mut entity_commands = commands.spawn((transform, Visibility::default()));
        if component.name == "body" {
            entity_commands.insert(BodyAnchor);
        }
        if component.name == "head" {
            entity_commands.insert(CharacterHead::new(root, transform.translation));
        }
        let entity = entity_commands.id();
        component_entities.insert(component.component_id.as_str(), entity);
    }

    for component in &manifest.components {
        let component_entity = component_entities[component.component_id.as_str()];
        let parent = component
            .parent_component_id
            .as_deref()
            .and_then(|parent_id| component_entities.get(parent_id).copied())
            .unwrap_or(anchor);
        commands.entity(parent).add_child(component_entity);

        let fill_color = materials.add(component_color(character, &component.name));
        let outline_color = materials.add(Color::srgb(0.045, 0.04, 0.055));
        let is_dynamic_eye = character.0 != "barde"
            && (component.name == "eye_left" || component.name == "eye_right");
        let eye_collider = is_dynamic_eye
            .then(|| {
                let collider = component
                    .closed_region_mesh
                    .as_ref()
                    .and_then(|mesh| {
                        EyeCollider::from_region_mesh(
                            &mesh.vertices,
                            &mesh.indices,
                            library.pupil_area_ratio,
                            library.pupil_collision_reference_radius,
                        )
                    })
                    .or_else(|| {
                        (manifest.schema_version < 8)
                            .then(|| {
                                component.contour_stroke_mesh.as_ref().and_then(|mesh| {
                                    EyeCollider::from_outline_area_ratio(
                                        &mesh.vertices,
                                        &mesh.indices,
                                        library.pupil_area_ratio,
                                        library.pupil_collision_reference_radius,
                                    )
                                })
                            })
                            .flatten()
                    });
                collider.map(|collider| match component.contour_stroke_mesh.as_ref() {
                    Some(stroke) => collider.with_visible_outline(
                        &stroke.vertices,
                        &stroke.indices,
                        stroke.runs.iter().any(|run| run.closed),
                    ),
                    None => collider,
                })
            })
            .flatten();
        if is_dynamic_eye && eye_collider.is_none() {
            warn!(character = %character.0, eye = %component.name, "cannot build eye collider from contour");
        }
        let mesh_transform = Transform::from_xyz(
            -component.local_pivot.unwrap_or([0.0, 0.0])[0],
            -component.local_pivot.unwrap_or([0.0, 0.0])[1],
            component.z_index as f32 * 0.01,
        );

        if let Some(eye_collider) = eye_collider {
            let pivot = component.local_pivot.unwrap_or([0.0, 0.0]);
            let pivot = Vec2::from_array(pivot);
            let local_collider = eye_collider.translated(-pivot);
            let pupil_geometry = local_collider.clipped_pupil_geometry(local_collider.center());
            let pupil = commands
                .spawn((
                    EyePupil::new(root, local_collider.clone()),
                    Mesh2d(meshes.add(bevy_pupil_mesh(&pupil_geometry))),
                    MeshMaterial2d(materials.add(Color::srgb(0.01, 0.008, 0.01))),
                    Transform::from_xyz(
                        local_collider.center().x,
                        local_collider.center().y,
                        component.z_index as f32 * 0.01 + 0.002,
                    ),
                ))
                .id();
            commands.entity(component_entity).add_child(pupil);
        }

        if let Some(mesh) = component.mesh.as_ref() {
            let fill = commands
                .spawn((
                    Mesh2d(meshes.add(bevy_mesh(mesh))),
                    MeshMaterial2d(fill_color),
                    mesh_transform,
                ))
                .id();
            commands.entity(component_entity).add_child(fill);
        }

        if let Some(stroke_mesh) = component.contour_stroke_mesh.as_ref()
            && stroke_mesh.has_outline
        {
            let outline = commands
                .spawn((
                    Mesh2d(meshes.add(bevy_mesh(&PolyToolsMesh {
                        vertices: stroke_mesh.vertices.clone(),
                        indices: stroke_mesh.indices.clone(),
                    }))),
                    MeshMaterial2d(outline_color),
                    Transform::from_xyz(
                        -component.local_pivot.unwrap_or([0.0, 0.0])[0],
                        -component.local_pivot.unwrap_or([0.0, 0.0])[1],
                        component.z_index as f32 * 0.01
                            + if is_dynamic_eye { 0.003 } else { 0.001 },
                    ),
                ))
                .id();
            commands.entity(component_entity).add_child(outline);
        }

        for referenced in &component.referenced_components {
            let referenced_entity = commands
                .spawn((component_transform(referenced), Visibility::default()))
                .id();
            commands
                .entity(component_entity)
                .add_child(referenced_entity);
            let fill_color = materials.add(component_color(character, &component.name));
            let outline_color = materials.add(Color::srgb(0.045, 0.04, 0.055));
            let pivot = referenced.local_pivot.unwrap_or([0.0, 0.0]);
            if let Some(mesh) = referenced.mesh.as_ref() {
                let fill = commands
                    .spawn((
                        Mesh2d(meshes.add(bevy_mesh(mesh))),
                        MeshMaterial2d(fill_color.clone()),
                        Transform::from_xyz(
                            -pivot[0],
                            -pivot[1],
                            (component.z_index + referenced.z_index) as f32 * 0.01,
                        ),
                    ))
                    .id();
                commands.entity(referenced_entity).add_child(fill);
            }
            if let Some(stroke_mesh) = referenced.contour_stroke_mesh.as_ref()
                && stroke_mesh.has_outline
            {
                let outline = commands
                    .spawn((
                        Mesh2d(meshes.add(bevy_mesh(&PolyToolsMesh {
                            vertices: stroke_mesh.vertices.clone(),
                            indices: stroke_mesh.indices.clone(),
                        }))),
                        MeshMaterial2d(outline_color),
                        Transform::from_xyz(
                            -pivot[0],
                            -pivot[1],
                            (component.z_index + referenced.z_index) as f32 * 0.01 + 0.001,
                        ),
                    ))
                    .id();
                commands.entity(referenced_entity).add_child(outline);
            }
        }
    }

    Ok(())
}

fn component_transform(component: &PolyToolsComponent) -> Transform {
    Transform::from_xyz(
        component.local_transform.position[0],
        component.local_transform.position[1],
        0.0,
    )
    .with_rotation(Quat::from_rotation_z(
        component.local_transform.rotation_radians,
    ))
    .with_scale(Vec3::new(
        component.local_transform.scale[0],
        component.local_transform.scale[1],
        1.0,
    ))
}

fn bevy_mesh(mesh: &PolyToolsMesh) -> Mesh {
    bevy_mesh_from_parts(&mesh.vertices, &mesh.indices)
}

pub(crate) fn bevy_pupil_mesh(geometry: &PupilGeometry) -> Mesh {
    bevy_mesh_from_parts(&geometry.vertices, &geometry.indices)
}

fn bevy_mesh_from_parts(vertices: &[[f32; 2]], indices: &[u32]) -> Mesh {
    let mut bevy_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    bevy_mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vertices
            .iter()
            .map(|vertex| [vertex[0], vertex[1], 0.0])
            .collect::<Vec<_>>(),
    );
    bevy_mesh.insert_indices(Indices::U32(indices.to_vec()));
    bevy_mesh
}

fn component_color(character: &CharacterId, component_name: &str) -> Color {
    match component_name {
        "head" | "belly" => Color::srgb(0.82, 0.63, 0.48),
        name if name.starts_with("eye") || name.starts_with("eyebrow") => {
            Color::srgb(0.035, 0.03, 0.04)
        }
        _ => character_color(character),
    }
}

fn character_color(character: &CharacterId) -> Color {
    match character.0.as_str() {
        "wizard" => Color::srgb(0.31, 0.18, 0.58),
        "mage" => Color::srgb(0.12, 0.55, 0.62),
        "sorcerer" => Color::srgb(0.62, 0.12, 0.16),
        "rogue" => Color::srgb(0.16, 0.17, 0.21),
        "glavier" => Color::srgb(0.63, 0.43, 0.11),
        "barde" => Color::srgb(0.55, 0.30, 0.12),
        "chantres" => Color::srgb(0.42, 0.20, 0.55),
        "hammerer" => Color::srgb(0.48, 0.31, 0.18),
        "monk" => Color::srgb(0.58, 0.32, 0.10),
        _ => Color::srgb(0.30, 0.34, 0.40),
    }
}

fn resolve_asset_references(
    manifest: &mut PolyToolsManifest,
    directory: &Path,
) -> Result<(), PolyToolsAssetError> {
    for component in &mut manifest.components {
        if component.mesh.is_some() || component.contour_stroke_mesh.is_some() {
            continue;
        }
        let Some(source_key) = component.source_asset_key.as_deref() else {
            continue;
        };
        let source = fs::read_to_string(directory.join(source_key).join("manifest.json")).map_err(
            |error| {
                PolyToolsAssetError::new(format!(
                    "cannot read referenced asset {source_key}: {error}"
                ))
            },
        )?;
        let symbol: PolyToolsSymbolManifest = serde_json::from_str(&source).map_err(|error| {
            PolyToolsAssetError::new(format!(
                "cannot parse referenced asset {source_key}: {error}"
            ))
        })?;
        if symbol.key != source_key || symbol.asset_kind != "symbols" {
            return Err(PolyToolsAssetError::new(format!(
                "referenced asset {source_key} is not a symbols manifest"
            )));
        }
        component.referenced_components = symbol.components;
    }
    Ok(())
}

fn validate_manifest(
    manifest: &PolyToolsManifest,
    expected_key: &str,
) -> Result<(), PolyToolsAssetError> {
    if !(5..=9).contains(&manifest.schema_version) {
        return Err(PolyToolsAssetError::new(format!(
            "{} uses unsupported schema {}",
            manifest.asset_key, manifest.schema_version
        )));
    }
    if manifest.asset_key != expected_key || manifest.asset_type != "character" {
        return Err(PolyToolsAssetError::new(
            "manifest asset key does not match its package",
        ));
    }
    if manifest.asset_type != "character" || !finite_pair(manifest.asset_pivot) {
        return Err(PolyToolsAssetError::new(format!(
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
        return Err(PolyToolsAssetError::new(format!(
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
            || component
                .local_transform
                .scale
                .iter()
                .any(|scale| *scale == 0.0)
        {
            return Err(PolyToolsAssetError::new(format!(
                "{} has invalid transform data",
                component.component_id
            )));
        }
        if let Some(parent) = component.parent_component_id.as_deref()
            && !component_ids.contains(parent)
        {
            return Err(PolyToolsAssetError::new(format!(
                "{} has an unknown parent",
                component.component_id
            )));
        }
        if let Some(mesh) = component.mesh.as_ref() {
            validate_mesh(mesh, &component.component_id)?;
        }
        if let Some(region) = component.closed_region_mesh.as_ref() {
            if region.role != "closed_contour_region" {
                return Err(PolyToolsAssetError::new(format!(
                    "{} has an invalid closed region role",
                    component.component_id
                )));
            }
            validate_mesh_parts(&region.vertices, &region.indices, &component.component_id)?;
        }
        if manifest.schema_version >= 8
            && expected_key != "barde"
            && (component.name == "eye_left" || component.name == "eye_right")
            && component.closed_region_mesh.is_none()
        {
            return Err(PolyToolsAssetError::new(format!(
                "{} is missing its schema-8+ closed eye region",
                component.component_id
            )));
        }
        if let Some(contour_stroke_mesh) = component.contour_stroke_mesh.as_ref() {
            let stroke = PolyToolsMesh {
                vertices: contour_stroke_mesh.vertices.clone(),
                indices: contour_stroke_mesh.indices.clone(),
            };
            if contour_stroke_mesh.has_outline {
                validate_mesh(&stroke, &component.component_id)?;
            } else if !stroke.vertices.is_empty() || !stroke.indices.is_empty() {
                return Err(PolyToolsAssetError::new(format!(
                    "{} has a disabled outline with geometry",
                    component.component_id
                )));
            }
        }
    }
    Ok(())
}

fn validate_mesh(mesh: &PolyToolsMesh, component_id: &str) -> Result<(), PolyToolsAssetError> {
    validate_mesh_parts(&mesh.vertices, &mesh.indices, component_id)
}

fn validate_mesh_parts(
    vertices: &[[f32; 2]],
    indices: &[u32],
    component_id: &str,
) -> Result<(), PolyToolsAssetError> {
    if vertices.is_empty()
        || indices.is_empty()
        || indices.len() % 3 != 0
        || vertices.iter().any(|vertex| !finite_pair(*vertex))
        || indices
            .iter()
            .any(|index| *index as usize >= vertices.len())
    {
        return Err(PolyToolsAssetError::new(format!(
            "{component_id} has invalid mesh geometry"
        )));
    }
    Ok(())
}

fn finite_pair(values: [f32; 2]) -> bool {
    values.into_iter().all(f32::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifests_cover_current_catalog_characters() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        let ids = library.ids();

        assert_eq!(ids.len(), 11);
        assert!(ids.iter().any(|character| character.0 == "monk"));
        assert!(ids.iter().any(|character| character.0 == "warrior"));
    }

    #[test]
    fn embedded_hammer_has_slice_14_authoring_contract() {
        let hammer: serde_json::Value = serde_json::from_str(include_str!(
            "../../../assets/characters/hammer/manifest.json"
        ))
        .expect("embedded Hammer manifest is valid JSON");
        let frame_roles = hammer["attachment_frames"]
            .as_array()
            .expect("Hammer attachment frames are present")
            .iter()
            .filter_map(|frame| frame["role"].as_str())
            .collect::<BTreeSet<_>>();
        let regions = hammer["regions"]
            .as_array()
            .expect("Hammer regions are present");

        assert_eq!(hammer["schema_version"], 9);
        assert_eq!(hammer["asset_type"], "weapons");
        assert!(frame_roles.contains("grip_primary"));
        assert!(frame_roles.contains("attack_point_primary"));
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0]["role"], "attack");
        assert!(
            regions[0]["vertices"]
                .as_array()
                .is_some_and(|v| v.len() >= 3)
        );
        assert!(
            regions[0]["indices"]
                .as_array()
                .is_some_and(|i| { !i.is_empty() && i.len() % 3 == 0 })
        );
    }

    #[test]
    fn embedded_manifests_expose_confirmed_authored_facing() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        let expected = [
            ("archerf", AuthoredFacing::Left),
            ("barde", AuthoredFacing::Neutral),
            ("chantres", AuthoredFacing::Left),
            ("glavier", AuthoredFacing::Neutral),
            ("hammerer", AuthoredFacing::Neutral),
            ("mage", AuthoredFacing::Right),
            ("monk", AuthoredFacing::Neutral),
            ("rogue", AuthoredFacing::Left),
            ("sorcerer", AuthoredFacing::Left),
            ("warrior", AuthoredFacing::Neutral),
            ("wizard", AuthoredFacing::Right),
        ];

        for (character, facing) in expected {
            assert_eq!(
                library.authored_facing(&CharacterId(character.to_owned())),
                facing,
                "{character} has the wrong authored facing",
            );
        }
    }

    #[test]
    fn missing_presentation_metadata_defaults_to_neutral() {
        let manifest: PolyToolsManifest = serde_json::from_str(
            r#"{
                "schema_version": 8,
                "asset_key": "legacy",
                "asset_type": "character",
                "asset_pivot": [0.0, 0.0],
                "components": []
            }"#,
        )
        .expect("legacy manifest remains readable");

        assert_eq!(
            manifest.presentation.authored_facing,
            AuthoredFacing::Neutral
        );
    }

    #[test]
    fn authored_facing_accepts_every_exported_value() {
        for (serialized, expected) in [
            (r#""left""#, AuthoredFacing::Left),
            (r#""right""#, AuthoredFacing::Right),
            (r#""neutral""#, AuthoredFacing::Neutral),
            (r#""top""#, AuthoredFacing::Top),
            (r#""down""#, AuthoredFacing::Down),
        ] {
            assert_eq!(
                serde_json::from_str::<AuthoredFacing>(serialized)
                    .expect("exported authored facing is valid"),
                expected,
            );
        }
    }

    #[test]
    fn imported_manifests_use_only_finite_indexed_geometry() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        for manifest in library.characters.values() {
            for component in &manifest.components {
                if let Some(mesh) = component.mesh.as_ref() {
                    validate_mesh(mesh, &component.component_id).expect("fill mesh is valid");
                }
            }
        }
    }

    #[test]
    fn imported_eye_regions_build_colliders() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        for (character, manifest) in &library.characters {
            if character.0 == "barde" {
                continue;
            }
            for component in &manifest.components {
                if component.name != "eye_left" && component.name != "eye_right" {
                    continue;
                }
                let region = component
                    .closed_region_mesh
                    .as_ref()
                    .expect("eye has a closed region");
                assert!(
                    EyeCollider::from_region_mesh(
                        &region.vertices,
                        &region.indices,
                        library.pupil_area_ratio,
                        library.pupil_collision_reference_radius,
                    )
                    .is_some(),
                    "{} {} must produce an eye collider",
                    character.0,
                    component.name,
                );
            }
        }
    }
}
