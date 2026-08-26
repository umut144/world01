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

#[derive(Component)]
pub struct BodyAnchor;

#[derive(Resource, Clone)]
pub struct CharacterAssetLibrary {
    characters: HashMap<CharacterId, PolyToolsManifest>,
}

impl CharacterAssetLibrary {
    #[cfg(test)]
    fn load_embedded() -> Result<Self, PolyToolsAssetError> {
        Self::load_from_directory(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/characters")
                .as_path(),
        )
    }

    pub fn load_from_directory(directory: &Path) -> Result<Self, PolyToolsAssetError> {
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

        Ok(Self { characters })
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

#[derive(Clone, Deserialize)]
struct PolyToolsManifest {
    schema_version: u32,
    asset_key: String,
    asset_type: String,
    asset_pivot: [f32; 2],
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
struct PolyToolsStrokeMesh {
    has_outline: bool,
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
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
    let anchor = commands
        .spawn((
            Transform::from_xyz(-manifest.asset_pivot[0], -manifest.asset_pivot[1], 0.0),
            Visibility::default(),
        ))
        .id();
    commands.entity(root).add_child(anchor);

    let mut component_entities = HashMap::new();
    for component in &manifest.components {
        let mut entity_commands =
            commands.spawn((component_transform(component), Visibility::default()));
        if component.name == "body" {
            entity_commands.insert(BodyAnchor);
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
        let mesh_transform = Transform::from_xyz(
            -component.local_pivot.unwrap_or([0.0, 0.0])[0],
            -component.local_pivot.unwrap_or([0.0, 0.0])[1],
            component.z_index as f32 * 0.01,
        );

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
                        component.z_index as f32 * 0.01 + 0.001,
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
                        Transform::from_xyz(-pivot[0], -pivot[1], referenced.z_index as f32 * 0.01),
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
                            referenced.z_index as f32 * 0.01 + 0.001,
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
    let mut bevy_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    bevy_mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        mesh.vertices
            .iter()
            .map(|vertex| [vertex[0], vertex[1], 0.0])
            .collect::<Vec<_>>(),
    );
    bevy_mesh.insert_indices(Indices::U32(mesh.indices.clone()));
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
    if !(5..=6).contains(&manifest.schema_version) {
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
    if mesh.vertices.is_empty()
        || mesh.indices.is_empty()
        || mesh.indices.len() % 3 != 0
        || mesh.vertices.iter().any(|vertex| !finite_pair(*vertex))
        || mesh
            .indices
            .iter()
            .any(|index| *index as usize >= mesh.vertices.len())
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
        assert_eq!(library.ids().len(), 9);
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
}
