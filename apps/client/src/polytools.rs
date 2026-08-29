use std::{collections::HashMap, error::Error, fmt};

use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
#[cfg(test)]
use game01_content::RuntimeFrameTransform;
#[cfg(test)]
use game01_content::WEAPON_REACH_LIMIT_ROLE;
use game01_content::{
    AuthoredFacing, HAMMER_ASSET_KEY, RuntimeAttachmentFrame, RuntimeComponent, RuntimeContent,
    RuntimeManifest, RuntimeMesh, WEAPON_ATTACK_POINT_ROLE, WEAPON_GRIP_ROLE,
    WEAPON_SECONDARY_GRIP_ROLE, WEAPON_SOCKET_ROLE,
};
use game01_world_data::CharacterId;

use crate::eyes::{EyeCollider, EyePupil, PupilGeometry};
use crate::hammer::{HammerPresentationMaterial, HammerPresentationState};
use crate::pose::CharacterHead;

#[derive(Component)]
pub struct BodyAnchor;

#[derive(Component, Debug, Clone)]
pub struct HammerVisual {
    pub owner: Entity,
    pub rest_transform: Transform,
    pub attack_point_from_grip: Vec2,
    pub secondary_grip_from_primary: Vec2,
    pub owner_asset_pivot: Vec2,
    pub material_handles: Vec<Handle<HammerPresentationMaterial>>,
    pub flat_visuals: Vec<Entity>,
    pub swing_depth_visuals: Vec<Entity>,
    pub behind_layer: f32,
    pub front_layer: f32,
}

#[derive(Component, Debug, Clone)]
pub struct CharacterVisual {
    pub orientation_root: Entity,
    pub authored_facing: AuthoredFacing,
    pub outline_visuals: Vec<Entity>,
}

#[derive(Component)]
pub struct CharacterVisualOrientation;

#[derive(Resource, Clone)]
pub struct CharacterAssetLibrary {
    content: RuntimeContent,
    pupil_area_ratio: f32,
    pupil_collision_reference_radius: f32,
}

impl CharacterAssetLibrary {
    #[cfg(test)]
    fn load_embedded() -> Result<Self, PolyToolsAssetError> {
        let design = game01_configs::load_embedded().map_err(|error| {
            PolyToolsAssetError::new(format!("cannot load embedded eye design: {error}"))
        })?;
        Self::from_content(
            RuntimeContent::load_embedded()
                .map_err(|error| PolyToolsAssetError::new(error.to_string()))?,
            design.eyes.pupil_area_ratio,
            design.eyes.hammerer_collision_radius_ratio,
        )
    }

    pub fn from_content(
        content: RuntimeContent,
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
        let hammerer = content
            .character(&CharacterId("hammerer".into()))
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
            content,
            pupil_area_ratio,
            pupil_collision_reference_radius,
        })
    }

    pub fn ids(&self) -> Vec<CharacterId> {
        self.content.ids()
    }

    fn character(&self, character: &CharacterId) -> Option<&RuntimeManifest> {
        self.content.character(character)
    }

    pub fn body_pivot(&self, character: &CharacterId) -> Vec2 {
        let Some(manifest) = self.character(character) else {
            return Vec2::ZERO;
        };
        manifest
            .components
            .iter()
            .find(|component| component.name == "body")
            .and_then(|component| component_world_transform(component, &manifest.components))
            .map(|body_transform| {
                Vec2::new(
                    body_transform.translation.x - manifest.asset_pivot[0],
                    body_transform.translation.y - manifest.asset_pivot[1],
                )
            })
            .unwrap_or(Vec2::ZERO)
    }

    /// Highest rendered fill vertex in the character root's local space,
    /// including the asset-pivot correction used by the presentation tree.
    pub fn health_bar_offset_y(&self, character: &CharacterId) -> f32 {
        let Some(manifest) = self.character(character) else {
            return 1.05;
        };
        let mut highest = f32::NEG_INFINITY;
        for component in &manifest.components {
            let world = component_world_transform(component, &manifest.components)
                .unwrap_or_else(|| component_transform(component));
            highest = highest.max(mesh_highest_y(
                component.mesh.as_ref(),
                world,
                component.local_pivot,
            ));
            for referenced in &component.referenced_components {
                let referenced_world = world.mul_transform(component_transform(referenced));
                highest = highest.max(mesh_highest_y(
                    referenced.mesh.as_ref(),
                    referenced_world,
                    referenced.local_pivot,
                ));
            }
        }
        if highest.is_finite() {
            highest - manifest.asset_pivot[1] + 0.08
        } else {
            1.05
        }
    }

    #[cfg(test)]
    fn authored_facing(&self, character: &CharacterId) -> AuthoredFacing {
        self.character(character)
            .map(|manifest| manifest.presentation.authored_facing)
            .unwrap_or_default()
    }
}

fn component_world_transform(
    component: &RuntimeComponent,
    components: &[RuntimeComponent],
) -> Option<Transform> {
    let local = component_transform(component);
    component
        .parent_component_id
        .as_ref()
        .and_then(|parent_id| {
            components
                .iter()
                .find(|parent| &parent.component_id == parent_id)
                .and_then(|parent| component_world_transform(parent, components))
                .map(|parent| parent.mul_transform(local))
        })
        .or(Some(local))
}

fn mesh_highest_y(
    mesh: Option<&RuntimeMesh>,
    transform: Transform,
    pivot: Option<[f32; 2]>,
) -> f32 {
    mesh.map(|mesh| {
        let pivot = pivot.unwrap_or([0.0, 0.0]);
        mesh.vertices
            .iter()
            .map(|vertex| {
                transform
                    .transform_point(Vec3::new(vertex[0] - pivot[0], vertex[1] - pivot[1], 0.0))
                    .y
            })
            .fold(f32::NEG_INFINITY, f32::max)
    })
    .unwrap_or(f32::NEG_INFINITY)
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

const ASSET_LOCAL_Z_STEP: f32 = 0.01;
const OUTLINE_Z_OFFSET: f32 = 0.001;
const PUPIL_Z_OFFSET: f32 = 0.002;
const DYNAMIC_EYE_OUTLINE_Z_OFFSET: f32 = 0.003;
const ATTACHED_WEAPON_LAYER_GAP_STEPS: i32 = 1;

fn weapon_key_for_character(character: &CharacterId) -> Option<&'static str> {
    (character.0 == "hammerer").then_some(HAMMER_ASSET_KEY)
}

fn asset_local_z_index_bounds(manifest: &RuntimeManifest) -> Option<(i32, i32)> {
    let mut bounds: Option<(i32, i32)> = None;
    for component in &manifest.components {
        let mut include = |z_index: i32| {
            bounds = Some(match bounds {
                Some((minimum, maximum)) => (minimum.min(z_index), maximum.max(z_index)),
                None => (z_index, z_index),
            });
        };
        include(component.z_index);
        for referenced in &component.referenced_components {
            include(component.z_index.saturating_add(referenced.z_index));
        }
    }
    bounds
}

fn resting_attached_weapon_layer(character: &RuntimeManifest, weapon: &RuntimeManifest) -> f32 {
    let character_minimum = asset_local_z_index_bounds(character)
        .map(|(minimum, _)| minimum)
        .unwrap_or_default();
    let weapon_maximum = asset_local_z_index_bounds(weapon)
        .map(|(_, maximum)| maximum)
        .unwrap_or_default();
    character_minimum
        .saturating_sub(weapon_maximum)
        .saturating_sub(ATTACHED_WEAPON_LAYER_GAP_STEPS) as f32
        * ASSET_LOCAL_Z_STEP
}

fn attacking_attached_weapon_layer(character: &RuntimeManifest, weapon: &RuntimeManifest) -> f32 {
    let character_maximum = asset_local_z_index_bounds(character)
        .map(|(_, maximum)| maximum)
        .unwrap_or_default();
    let weapon_minimum = asset_local_z_index_bounds(weapon)
        .map(|(minimum, _)| minimum)
        .unwrap_or_default();
    character_maximum
        .saturating_sub(weapon_minimum)
        .saturating_add(ATTACHED_WEAPON_LAYER_GAP_STEPS) as f32
        * ASSET_LOCAL_Z_STEP
}

pub fn spawn_character_visual(
    commands: &mut Commands,
    root: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    hammer_materials: &mut Assets<HammerPresentationMaterial>,
    library: &CharacterAssetLibrary,
    character: &CharacterId,
) -> Result<(), PolyToolsAssetError> {
    let manifest = library
        .character(character)
        .ok_or_else(|| PolyToolsAssetError::new("missing validated character manifest"))?;
    let body_pivot = library.body_pivot(character);
    let orientation_root = commands
        .spawn((
            CharacterVisualOrientation,
            Transform::from_translation(-body_pivot.extend(0.0)),
            Visibility::default(),
        ))
        .id();
    let status_pivot = commands
        .spawn((
            Transform::from_translation(body_pivot.extend(0.0)),
            Visibility::default(),
        ))
        .id();
    let anchor = commands
        .spawn((
            Transform::from_xyz(-manifest.asset_pivot[0], -manifest.asset_pivot[1], 0.0),
            Visibility::default(),
        ))
        .id();
    commands.entity(root).add_child(status_pivot);
    commands.entity(status_pivot).add_child(orientation_root);
    commands.entity(orientation_root).add_child(anchor);

    let mut component_entities = HashMap::new();
    let mut outline_visuals = Vec::new();
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
            component.z_index as f32 * ASSET_LOCAL_Z_STEP,
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
                        component.z_index as f32 * ASSET_LOCAL_Z_STEP + PUPIL_Z_OFFSET,
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
                    Mesh2d(meshes.add(bevy_mesh(&RuntimeMesh {
                        vertices: stroke_mesh.vertices.clone(),
                        indices: stroke_mesh.indices.clone(),
                    }))),
                    MeshMaterial2d(outline_color),
                    Transform::from_xyz(
                        -component.local_pivot.unwrap_or([0.0, 0.0])[0],
                        -component.local_pivot.unwrap_or([0.0, 0.0])[1],
                        component.z_index as f32 * ASSET_LOCAL_Z_STEP
                            + if is_dynamic_eye {
                                DYNAMIC_EYE_OUTLINE_Z_OFFSET
                            } else {
                                OUTLINE_Z_OFFSET
                            },
                    ),
                ))
                .id();
            commands.entity(component_entity).add_child(outline);
            outline_visuals.push(outline);
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
                            (component.z_index + referenced.z_index) as f32 * ASSET_LOCAL_Z_STEP,
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
                        Mesh2d(meshes.add(bevy_mesh(&RuntimeMesh {
                            vertices: stroke_mesh.vertices.clone(),
                            indices: stroke_mesh.indices.clone(),
                        }))),
                        MeshMaterial2d(outline_color),
                        Transform::from_xyz(
                            -pivot[0],
                            -pivot[1],
                            (component.z_index + referenced.z_index) as f32 * ASSET_LOCAL_Z_STEP
                                + OUTLINE_Z_OFFSET,
                        ),
                    ))
                    .id();
                commands.entity(referenced_entity).add_child(outline);
                outline_visuals.push(outline);
            }
        }
    }

    commands.entity(root).insert(CharacterVisual {
        orientation_root,
        authored_facing: manifest.presentation.authored_facing,
        outline_visuals,
    });

    if weapon_key_for_character(character) == Some(HAMMER_ASSET_KEY) {
        spawn_hammer_visual(
            commands,
            root,
            anchor,
            meshes,
            hammer_materials,
            manifest,
            library.content.hammer(),
        )?;
    }

    Ok(())
}

fn spawn_hammer_visual(
    commands: &mut Commands,
    owner: Entity,
    character_anchor: Entity,
    meshes: &mut Assets<Mesh>,
    hammer_materials: &mut Assets<HammerPresentationMaterial>,
    character: &RuntimeManifest,
    hammer: &RuntimeManifest,
) -> Result<(), PolyToolsAssetError> {
    let socket = attachment_frame(character, WEAPON_SOCKET_ROLE)?;
    let grip = attachment_frame(hammer, WEAPON_GRIP_ROLE)?;
    let secondary_grip = attachment_frame(hammer, WEAPON_SECONDARY_GRIP_ROLE)?;
    let attack_point = attachment_frame(hammer, WEAPON_ATTACK_POINT_ROLE)?;
    let behind_layer = resting_attached_weapon_layer(character, hammer);
    let front_layer = attacking_attached_weapon_layer(character, hammer);
    let (pose_transform, asset_transform) =
        hammer_attachment_transforms(socket, grip, behind_layer);
    let attack_point_from_grip = asset_transform
        .to_matrix()
        .transform_point3(Vec3::new(
            attack_point.asset_transform.position[0],
            attack_point.asset_transform.position[1],
            0.0,
        ))
        .truncate();
    let secondary_grip_from_primary = asset_transform
        .to_matrix()
        .transform_point3(Vec3::new(
            secondary_grip.asset_transform.position[0],
            secondary_grip.asset_transform.position[1],
            0.0,
        ))
        .truncate();
    let pose_root = commands.spawn((pose_transform, Visibility::default())).id();
    let asset_root = commands
        .spawn((asset_transform, Visibility::default()))
        .id();
    commands.entity(character_anchor).add_child(pose_root);
    commands.entity(pose_root).add_child(asset_root);

    let mut component_entities = HashMap::new();
    let mut material_handles = Vec::new();
    let mut flat_visuals = Vec::new();
    let mut swing_depth_visuals = Vec::new();
    for component in &hammer.components {
        let entity = commands
            .spawn((component_transform(component), Visibility::default()))
            .id();
        component_entities.insert(component.component_id.as_str(), entity);
    }

    for component in &hammer.components {
        let component_entity = component_entities[component.component_id.as_str()];
        let parent = component
            .parent_component_id
            .as_deref()
            .and_then(|parent_id| component_entities.get(parent_id).copied())
            .unwrap_or(asset_root);
        commands.entity(parent).add_child(component_entity);

        let pivot = component.local_pivot.unwrap_or([0.0, 0.0]);
        let z = component.z_index as f32 * ASSET_LOCAL_Z_STEP;
        if let Some(mesh) = component.mesh.as_ref() {
            let material = hammer_materials.add(HammerPresentationMaterial::from_color(
                hammer_component_color(&component.name),
                z,
                behind_layer,
                component.projection_depth_meters,
            ));
            material_handles.push(material.clone());
            let fill = commands
                .spawn((
                    Mesh2d(meshes.add(bevy_mesh(mesh))),
                    MeshMaterial2d(material.clone()),
                    Transform::from_xyz(-pivot[0], -pivot[1], 0.0),
                    Visibility::Visible,
                ))
                .id();
            commands.entity(component_entity).add_child(fill);
            flat_visuals.push(fill);

            let depth_fill = commands
                .spawn((
                    Mesh2d(meshes.add(bevy_closed_prism_mesh(
                        mesh,
                        component.projection_depth_meters,
                    ))),
                    MeshMaterial2d(material),
                    Transform::from_xyz(-pivot[0], -pivot[1], 0.0),
                    Visibility::Hidden,
                ))
                .id();
            commands.entity(component_entity).add_child(depth_fill);
            swing_depth_visuals.push(depth_fill);
        }
        if let Some(stroke) = component.contour_stroke_mesh.as_ref()
            && stroke.has_outline
        {
            let material = hammer_materials.add(HammerPresentationMaterial::from_color(
                Color::srgb(0.045, 0.04, 0.055),
                z + OUTLINE_Z_OFFSET,
                behind_layer,
                component.projection_depth_meters,
            ));
            material_handles.push(material.clone());
            let outline = commands
                .spawn((
                    Mesh2d(meshes.add(bevy_mesh(&RuntimeMesh {
                        vertices: stroke.vertices.clone(),
                        indices: stroke.indices.clone(),
                    }))),
                    MeshMaterial2d(material),
                    Transform::from_xyz(-pivot[0], -pivot[1], 0.0),
                    Visibility::Visible,
                ))
                .id();
            commands.entity(component_entity).add_child(outline);
            flat_visuals.push(outline);
        }
    }

    commands.entity(pose_root).insert((
        HammerVisual {
            owner,
            rest_transform: pose_transform,
            attack_point_from_grip,
            secondary_grip_from_primary,
            owner_asset_pivot: Vec2::from_array(character.asset_pivot),
            material_handles,
            flat_visuals,
            swing_depth_visuals,
            behind_layer,
            front_layer,
        },
        HammerPresentationState::default(),
    ));

    Ok(())
}

fn attachment_frame<'a>(
    manifest: &'a RuntimeManifest,
    role: &str,
) -> Result<&'a RuntimeAttachmentFrame, PolyToolsAssetError> {
    let mut matches = manifest
        .attachment_frames
        .iter()
        .filter(|frame| frame.role == role);
    let Some(frame) = matches.next() else {
        return Err(PolyToolsAssetError::new(format!(
            "{} is missing attachment frame {role}",
            manifest.asset_key
        )));
    };
    if matches.next().is_some() {
        return Err(PolyToolsAssetError::new(format!(
            "{} has duplicate attachment frame {role}",
            manifest.asset_key
        )));
    }
    Ok(frame)
}

fn hammer_attachment_transforms(
    socket: &RuntimeAttachmentFrame,
    grip: &RuntimeAttachmentFrame,
    layer: f32,
) -> (Transform, Transform) {
    let socket_transform = Transform::from_xyz(
        socket.asset_transform.position[0],
        socket.asset_transform.position[1],
        layer,
    )
    .with_rotation(Quat::from_rotation_z(
        socket.asset_transform.rotation_radians,
    ));
    let inverse_grip_rotation = -grip.asset_transform.rotation_radians;
    let inverse_grip_position = Vec2::from_angle(inverse_grip_rotation)
        .rotate(-Vec2::from_array(grip.asset_transform.position));
    let asset_transform =
        Transform::from_xyz(inverse_grip_position.x, inverse_grip_position.y, 0.0)
            .with_rotation(Quat::from_rotation_z(inverse_grip_rotation));
    (socket_transform, asset_transform)
}

fn component_transform(component: &RuntimeComponent) -> Transform {
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

fn bevy_mesh(mesh: &RuntimeMesh) -> Mesh {
    bevy_mesh_from_parts(&mesh.vertices, &mesh.indices)
}

fn bevy_closed_prism_mesh(mesh: &RuntimeMesh, depth_meters: f32) -> Mesh {
    let (vertices, indices) = closed_prism_parts(mesh, depth_meters);
    bevy_mesh_from_parts_3d(&vertices, &indices)
}

fn closed_prism_parts(mesh: &RuntimeMesh, depth_meters: f32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let half_depth = depth_meters.max(0.0) * 0.5;
    let mut vertices = Vec::with_capacity(mesh.vertices.len() * 2);
    for vertex in &mesh.vertices {
        vertices.push([vertex[0], vertex[1], -half_depth]);
    }
    for vertex in &mesh.vertices {
        vertices.push([vertex[0], vertex[1], half_depth]);
    }

    let offset = mesh.vertices.len() as u32;
    let mut indices = Vec::with_capacity(mesh.indices.len() * 2);
    for triangle in mesh.indices.chunks_exact(3) {
        indices.extend_from_slice(triangle);
        indices.extend_from_slice(&[
            triangle[0] + offset,
            triangle[2] + offset,
            triangle[1] + offset,
        ]);
    }

    let mut boundary_edges = HashMap::<(u32, u32), (u32, u32, u32)>::new();
    for triangle in mesh.indices.chunks_exact(3) {
        for (start, end) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = if start < end {
                (start, end)
            } else {
                (end, start)
            };
            boundary_edges
                .entry(key)
                .and_modify(|edge| edge.2 += 1)
                .or_insert((start, end, 1));
        }
    }
    for (_, (start, end, count)) in boundary_edges {
        if count == 1 {
            indices.extend_from_slice(&[
                start,
                end,
                end + offset,
                start,
                end + offset,
                start + offset,
            ]);
        }
    }

    (vertices, indices)
}

pub(crate) fn bevy_pupil_mesh(geometry: &PupilGeometry) -> Mesh {
    bevy_mesh_from_parts(&geometry.vertices, &geometry.indices)
}

fn bevy_mesh_from_parts(vertices: &[[f32; 2]], indices: &[u32]) -> Mesh {
    let vertices = vertices
        .iter()
        .map(|vertex| [vertex[0], vertex[1], 0.0])
        .collect::<Vec<_>>();
    bevy_mesh_from_parts_3d(&vertices, indices)
}

fn bevy_mesh_from_parts_3d(vertices: &[[f32; 3]], indices: &[u32]) -> Mesh {
    let mut bevy_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices.to_vec());
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

fn hammer_component_color(component_name: &str) -> Color {
    if component_name.starts_with("head") {
        Color::srgb(0.42, 0.46, 0.52)
    } else {
        Color::srgb(0.34, 0.18, 0.08)
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
    fn body_pivot_is_derived_from_the_rendered_body_anchor() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");

        let hammerer_pivot = library.body_pivot(&CharacterId("hammerer".to_owned()));
        let rogue_pivot = library.body_pivot(&CharacterId("rogue".to_owned()));
        assert!((hammerer_pivot - Vec2::new(0.0, 0.695)).length() < 0.000_001);
        assert!((rogue_pivot - Vec2::new(0.0, 0.15)).length() < 0.000_001);
    }

    #[test]
    fn embedded_hammer_has_current_combat_authoring_contract() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        let hammer = library.content.hammer();
        let grip = attachment_frame(hammer, WEAPON_GRIP_ROLE).expect("Hammer grip is valid");
        let secondary_grip = attachment_frame(hammer, WEAPON_SECONDARY_GRIP_ROLE)
            .expect("Hammer secondary grip is valid");
        let attack_point = attachment_frame(hammer, WEAPON_ATTACK_POINT_ROLE)
            .expect("Hammer attack point is valid");
        let reach_limit =
            attachment_frame(hammer, WEAPON_REACH_LIMIT_ROLE).expect("Hammer reach limit is valid");
        assert_eq!(hammer.schema_version, 13);
        assert_eq!(hammer.asset_type, "weapons");
        assert!(Vec2::from_array(grip.asset_transform.position).is_finite());
        assert!(Vec2::from_array(secondary_grip.asset_transform.position).is_finite());
        assert!(Vec2::from_array(attack_point.asset_transform.position).is_finite());
        assert!(Vec2::from_array(reach_limit.asset_transform.position).is_finite());
        assert_ne!(
            grip.asset_transform.position,
            attack_point.asset_transform.position
        );
        assert_ne!(
            grip.asset_transform.position,
            secondary_grip.asset_transform.position
        );
        assert_ne!(
            secondary_grip.asset_transform.position,
            attack_point.asset_transform.position
        );
        assert_ne!(
            reach_limit.asset_transform.position,
            attack_point.asset_transform.position
        );
        let authored_reach = Vec2::from_array(attack_point.asset_transform.position)
            .distance(Vec2::from_array(reach_limit.asset_transform.position));
        let held_attack_distance = Vec2::from_array(attack_point.asset_transform.position)
            .distance(Vec2::from_array(grip.asset_transform.position));
        let secondary_attack_distance = Vec2::from_array(attack_point.asset_transform.position)
            .distance(Vec2::from_array(secondary_grip.asset_transform.position));
        assert!(authored_reach > secondary_attack_distance);
        assert!(secondary_attack_distance > held_attack_distance);
    }

    #[test]
    fn projection_depth_builds_closed_side_walls_from_fill_boundary() {
        let mesh = RuntimeMesh {
            vertices: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
        };
        let (vertices, indices) = closed_prism_parts(&mesh, 0.4);

        assert_eq!(vertices.len(), 8);
        assert!(vertices[..4].iter().all(|vertex| vertex[2] == -0.2));
        assert!(vertices[4..].iter().all(|vertex| vertex[2] == 0.2));
        assert_eq!(indices.len(), 36);
    }

    #[test]
    fn only_hammerer_receives_the_fixed_hammer_association() {
        assert_eq!(
            weapon_key_for_character(&CharacterId("hammerer".to_owned())),
            Some("hammer")
        );
        assert_eq!(
            weapon_key_for_character(&CharacterId("mage".to_owned())),
            None
        );
    }

    #[test]
    fn resting_hammer_is_layered_behind_the_complete_hammerer_asset() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        let hammerer = library
            .character(&CharacterId("hammerer".to_owned()))
            .expect("Hammerer manifest is present");
        let hammer = library.content.hammer();
        let layer = resting_attached_weapon_layer(hammerer, hammer);
        let hammerer_minimum = asset_local_z_index_bounds(hammerer)
            .map(|(minimum, _)| minimum)
            .expect("Hammerer has visual components") as f32
            * ASSET_LOCAL_Z_STEP;

        for component in &hammer.components {
            let hammer_outline_z =
                layer + component.z_index as f32 * ASSET_LOCAL_Z_STEP + OUTLINE_Z_OFFSET;
            assert!(hammer_outline_z < hammerer_minimum);
        }
    }

    #[test]
    fn attacking_hammer_is_layered_in_front_of_the_complete_hammerer_asset() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        let hammerer = library
            .character(&CharacterId("hammerer".to_owned()))
            .expect("Hammerer manifest is present");
        let hammer = library.content.hammer();
        let layer = attacking_attached_weapon_layer(hammerer, hammer);
        let hammerer_maximum = asset_local_z_index_bounds(hammerer)
            .map(|(_, maximum)| maximum)
            .expect("Hammerer has visual components") as f32
            * ASSET_LOCAL_Z_STEP;
        let hammer_minimum = asset_local_z_index_bounds(hammer)
            .map(|(minimum, _)| minimum)
            .expect("Hammer has visual components") as f32
            * ASSET_LOCAL_Z_STEP;

        assert!(layer + hammer_minimum > hammerer_maximum);
    }

    #[test]
    fn hammer_grip_aligns_to_socket_and_remains_the_pose_pivot() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        let hammerer = library
            .character(&CharacterId("hammerer".to_owned()))
            .expect("Hammerer manifest is present");
        let authored_socket = attachment_frame(hammerer, WEAPON_SOCKET_ROLE)
            .expect("Hammerer weapon socket is valid");
        let authored_grip = attachment_frame(library.content.hammer(), WEAPON_GRIP_ROLE)
            .expect("Hammer grip is valid");
        let socket = RuntimeAttachmentFrame {
            frame_id: authored_socket.frame_id.clone(),
            role: authored_socket.role.clone(),
            asset_transform: RuntimeFrameTransform {
                position: authored_socket.asset_transform.position,
                rotation_radians: 0.6,
            },
        };
        let grip = RuntimeAttachmentFrame {
            frame_id: authored_grip.frame_id.clone(),
            role: authored_grip.role.clone(),
            asset_transform: RuntimeFrameTransform {
                position: authored_grip.asset_transform.position,
                rotation_radians: -0.25,
            },
        };
        let (pose, asset) = hammer_attachment_transforms(&socket, &grip, 0.05);
        let grip_position = Vec3::new(
            grip.asset_transform.position[0],
            grip.asset_transform.position[1],
            0.0,
        );
        let grip_at_pose_origin = asset.to_matrix().transform_point3(grip_position);
        let aligned_grip = pose.to_matrix().transform_point3(grip_at_pose_origin);

        assert!(grip_at_pose_origin.length() < 0.000_001);
        assert!((aligned_grip.x - socket.asset_transform.position[0]).abs() < 0.000_001);
        assert!((aligned_grip.y - socket.asset_transform.position[1]).abs() < 0.000_001);
        assert!((aligned_grip.z - 0.05).abs() < 0.000_001);
        let grip_direction = Vec3::new(
            grip.asset_transform.rotation_radians.cos(),
            grip.asset_transform.rotation_radians.sin(),
            0.0,
        );
        let aligned_direction = pose
            .to_matrix()
            .transform_vector3(asset.to_matrix().transform_vector3(grip_direction));
        let socket_direction = Vec3::new(
            socket.asset_transform.rotation_radians.cos(),
            socket.asset_transform.rotation_radians.sin(),
            0.0,
        );
        assert!(aligned_direction.distance(socket_direction) < 0.000_001);

        let parent = Transform::from_xyz(2.0, -1.0, 3.0)
            .with_rotation(Quat::from_rotation_z(0.4))
            .with_scale(Vec3::new(-1.5, 1.5, 1.0));
        let inherited_grip = parent.to_matrix().transform_point3(aligned_grip);
        let inherited_socket = parent.to_matrix().transform_point3(Vec3::new(
            socket.asset_transform.position[0],
            socket.asset_transform.position[1],
            0.05,
        ));
        assert!(inherited_grip.distance(inherited_socket) < 0.000_001);
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
    fn imported_eye_regions_build_colliders() {
        let library =
            CharacterAssetLibrary::load_embedded().expect("embedded PolyTools exports are valid");
        for (character, manifest) in library.content.characters() {
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
