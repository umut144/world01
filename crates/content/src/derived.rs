use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::{Resource, Vec2};
use world01_world_data::{
    CharacterId, ComponentMassAssignment, DensityClass, MassModelDefinition, Position, WorldMap,
};

use crate::manifest::{
    AuthoredFacing, ContentError, HAMMER_ASSET_KEY, RuntimeComponent, RuntimeContent,
    RuntimeManifest, RuntimeRegion, RuntimeTransform, WEAPON_ATTACK_POINT_ROLE, WEAPON_GRIP_ROLE,
    WEAPON_REACH_LIMIT_ROLE, WEAPON_SECONDARY_GRIP_ROLE, WEAPON_SOCKET_ROLE, attachment_frame,
};

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CharacterHealthCatalog {
    max_hp: HashMap<CharacterId, f32>,
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CharacterMassGeometryCatalog {
    body: HashMap<CharacterId, DensityAreas>,
    equipped_weapon: HashMap<CharacterId, DensityAreas>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensityAreas {
    areas: [f32; 6],
}

impl DensityAreas {
    pub const ZERO: Self = Self { areas: [0.0; 6] };

    pub fn area(self, class: DensityClass) -> f32 {
        self.areas[class.index()]
    }

    fn add(&mut self, class: DensityClass, area: f32) {
        self.areas[class.index()] += area;
    }

    fn combined(self, other: Self) -> Self {
        let mut result = self;
        for class in DensityClass::ALL {
            result.areas[class.index()] += other.areas[class.index()];
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterMassGeometryError(String);

impl fmt::Display for CharacterMassGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CharacterMassGeometryError {}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CharacterHurtGeometryCatalog {
    geometries: HashMap<CharacterId, CharacterHurtGeometry>,
}

impl CharacterHurtGeometryCatalog {
    pub fn from_content(content: &RuntimeContent) -> Result<Self, CharacterHurtGeometryError> {
        let mut geometries = HashMap::new();
        for (character_id, manifest) in content.characters() {
            let authored_regions = manifest
                .regions
                .iter()
                .filter(|region| region.role == "hurt")
                .map(region_geometry)
                .collect::<Vec<_>>();
            let components = if authored_regions.is_empty() {
                ["body", "head"]
                    .into_iter()
                    .map(|name| character_component_geometry(manifest, name))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                authored_regions
            };
            geometries.insert(
                character_id.clone(),
                CharacterHurtGeometry {
                    authored_facing: manifest.presentation.authored_facing,
                    components,
                },
            );
        }
        Ok(Self { geometries })
    }

    pub fn character(&self, character: &CharacterId) -> Option<&CharacterHurtGeometry> {
        self.geometries.get(character)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharacterHurtGeometry {
    pub authored_facing: AuthoredFacing,
    pub components: Vec<RuntimeComponentGeometry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterHurtGeometryError(String);

impl fmt::Display for CharacterHurtGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CharacterHurtGeometryError {}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MageEyeGeometry {
    pub authored_facing: AuthoredFacing,
    pub left: EyeBeamEmitterGeometry,
    pub right: EyeBeamEmitterGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeBeamEmitterGeometry {
    pub offset: Vec2,
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MageEyeGeometryError(String);

impl fmt::Display for MageEyeGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for MageEyeGeometryError {}

impl MageEyeGeometry {
    pub fn from_content(
        content: &RuntimeContent,
        eye_size_ratio: f32,
    ) -> Result<Self, MageEyeGeometryError> {
        if !eye_size_ratio.is_finite() || eye_size_ratio <= 0.0 {
            return Err(MageEyeGeometryError(
                "Mage eye size ratio must be finite and greater than zero".into(),
            ));
        }
        let mage = content
            .character(&CharacterId("mage".into()))
            .ok_or_else(|| MageEyeGeometryError("content is missing Mage".into()))?;
        Ok(Self {
            authored_facing: mage.presentation.authored_facing,
            left: eye_beam_emitter(mage, "eye_left", eye_size_ratio)?,
            right: eye_beam_emitter(mage, "eye_right", eye_size_ratio)?,
        })
    }
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct WorldCollisionGeometryCatalog {
    pub regions: Vec<PlacedCollisionGeometry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCollisionGeometry {
    pub instance_id: String,
    pub position: Position,
    pub component: RuntimeComponentGeometry,
}

impl WorldCollisionGeometryCatalog {
    pub fn from_content_and_map(content: &RuntimeContent, map: &WorldMap) -> Self {
        let regions = map
            .placements()
            .iter()
            .chain(map.transitions())
            .flat_map(|placement| {
                content
                    .prop(&placement.asset_key)
                    .or_else(|| content.terrain(&placement.asset_key))
                    .into_iter()
                    .flat_map(move |manifest| {
                        manifest
                            .regions
                            .iter()
                            .filter(|region| region.role == "collision")
                            .map(move |region| PlacedCollisionGeometry {
                                instance_id: placement.instance_id.clone(),
                                position: placement.position,
                                component: region_geometry(region),
                            })
                    })
            })
            .collect();
        Self { regions }
    }
}

impl CharacterHealthCatalog {
    pub fn from_content(content: &RuntimeContent) -> Result<Self, CharacterHealthError> {
        let mut areas = HashMap::new();
        for (asset_key, manifest) in content.characters() {
            let mut total = 0.0;
            for component in &manifest.components {
                if component.name != "body" && component.name != "feet" {
                    continue;
                }
                total += transformed_mesh_area(component, &manifest.components)?;
            }
            if !total.is_finite() || total <= 0.0 {
                return Err(CharacterHealthError(format!(
                    "{} is missing a positive body/feet area",
                    asset_key.0
                )));
            }
            areas.insert(asset_key.clone(), total);
        }
        let hammerer = areas
            .get(&CharacterId("hammerer".into()))
            .copied()
            .ok_or_else(|| CharacterHealthError("missing Hammerer body area".into()))?;
        Ok(Self {
            max_hp: areas
                .into_iter()
                .map(|(id, area)| (id, area / hammerer * 140.0))
                .collect(),
        })
    }

    pub fn max_hp(&self, character: &CharacterId) -> Option<f32> {
        self.max_hp.get(character).copied()
    }
}

impl CharacterMassGeometryCatalog {
    pub fn from_content(
        content: &RuntimeContent,
        definition: &MassModelDefinition,
    ) -> Result<Self, CharacterMassGeometryError> {
        if !definition.is_valid() {
            return Err(CharacterMassGeometryError(
                "mass definition is invalid".into(),
            ));
        }
        let hammer_definition = definition
            .weapons
            .iter()
            .find(|assignment| assignment.asset_key == HAMMER_ASSET_KEY)
            .ok_or_else(|| {
                CharacterMassGeometryError("mass definition is missing Hammer".into())
            })?;
        let hammer = density_areas_for_manifest(content.hammer(), &hammer_definition.components)?;
        let mut body = HashMap::new();
        let mut equipped_weapon = HashMap::new();
        for (character_id, manifest) in content.characters() {
            let assignment = definition
                .characters
                .iter()
                .find(|assignment| assignment.asset_key == character_id.0)
                .ok_or_else(|| {
                    CharacterMassGeometryError(format!(
                        "mass definition is missing Character {}",
                        character_id.0
                    ))
                })?;
            let body_areas = density_areas_for_manifest(manifest, &assignment.components)?;
            let mut weapon_areas = DensityAreas::ZERO;
            for weapon_key in &assignment.equipped_weapon_asset_keys {
                if weapon_key != HAMMER_ASSET_KEY {
                    return Err(CharacterMassGeometryError(format!(
                        "{} references unsupported equipped weapon {weapon_key}",
                        character_id.0
                    )));
                }
                weapon_areas = weapon_areas.combined(hammer);
            }
            body.insert(character_id.clone(), body_areas);
            equipped_weapon.insert(character_id.clone(), weapon_areas);
        }
        if definition.characters.len() != body.len() {
            return Err(CharacterMassGeometryError(
                "mass definition contains an unknown Character assignment".into(),
            ));
        }
        Ok(Self {
            body,
            equipped_weapon,
        })
    }

    pub fn body(&self, character: &CharacterId) -> Option<DensityAreas> {
        self.body.get(character).copied()
    }

    pub fn equipped_weapon(&self, character: &CharacterId) -> Option<DensityAreas> {
        self.equipped_weapon.get(character).copied()
    }

    pub fn character_ids(&self) -> impl Iterator<Item = &CharacterId> {
        self.body.keys()
    }
}

fn density_areas_for_manifest(
    manifest: &RuntimeManifest,
    assignments: &[ComponentMassAssignment],
) -> Result<DensityAreas, CharacterMassGeometryError> {
    let mut assignments_by_name = HashMap::new();
    for assignment in assignments {
        assignments_by_name.insert(assignment.component_name.as_str(), assignment);
    }
    if assignments_by_name.len() != manifest.components.len() {
        return Err(CharacterMassGeometryError(format!(
            "{} mass assignments do not cover its Components exactly once",
            manifest.asset_key
        )));
    }
    let mut areas = DensityAreas::ZERO;
    for component in &manifest.components {
        let assignment = assignments_by_name
            .get(component.name.as_str())
            .ok_or_else(|| {
                CharacterMassGeometryError(format!(
                    "{} mass assignments are missing Component {}",
                    manifest.asset_key, component.name
                ))
            })?;
        let Some(class) = assignment.classification.density_class() else {
            continue;
        };
        let area = transformed_mesh_area(component, &manifest.components).map_err(|error| {
            CharacterMassGeometryError(format!(
                "cannot derive mass area for {} Component {}: {error}",
                manifest.asset_key, component.name
            ))
        })?;
        if !area.is_finite() || area < 0.0 {
            return Err(CharacterMassGeometryError(format!(
                "{} Component {} has invalid mass area {area}",
                manifest.asset_key, component.name
            )));
        }
        areas.add(class, area);
    }
    Ok(areas)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterHealthError(String);

impl fmt::Display for CharacterHealthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CharacterHealthError {}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct HammerCombatGeometry {
    socket_offset: Vec2,
    primary_grip: Vec2,
    secondary_grip: Vec2,
    attack_point: Vec2,
    reach_limit: Vec2,
    attack_components: Vec<RuntimeComponentGeometry>,
}

impl HammerCombatGeometry {
    pub fn from_content(
        content: &RuntimeContent,
        attack_component_names: &[String],
    ) -> Result<Self, HammerCombatGeometryError> {
        let hammerer = content
            .character(&CharacterId("hammerer".into()))
            .ok_or_else(|| HammerCombatGeometryError::new("content is missing Hammerer"))?;
        let hammer = content.hammer();
        let hammerer_pivot = finite_vec2(hammerer.asset_pivot, "Hammerer asset pivot")?;
        let socket = unique_frame(hammerer, WEAPON_SOCKET_ROLE)?;
        let primary_grip = unique_frame(hammer, WEAPON_GRIP_ROLE)?;
        let secondary_grip = unique_frame(hammer, WEAPON_SECONDARY_GRIP_ROLE)?;
        let attack_point = unique_frame(hammer, WEAPON_ATTACK_POINT_ROLE)?;
        let reach_limit = unique_frame(hammer, WEAPON_REACH_LIMIT_ROLE)?;
        let authored_regions = hammer
            .regions
            .iter()
            .filter(|region| region.role == "attack")
            .map(region_geometry)
            .collect::<Vec<_>>();
        let attack_components = if authored_regions.is_empty() {
            attack_component_names
                .iter()
                .map(|name| component_geometry(hammer, name))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            authored_regions
        };
        let geometry = Self {
            socket_offset: socket - hammerer_pivot,
            primary_grip,
            secondary_grip,
            attack_point,
            reach_limit,
            attack_components,
        };
        let primary_radius = geometry.attack_radius(0.0);
        let secondary_radius = geometry.attack_radius(1.0);
        if primary_radius <= f32::EPSILON
            || secondary_radius <= primary_radius
            || geometry.maximum_reach() <= secondary_radius
        {
            return Err(HammerCombatGeometryError::new(
                "Hammer grips, attack point, and reach limit do not define increasing valid reaches",
            ));
        }
        Ok(geometry)
    }

    pub fn socket_offset(&self) -> Vec2 {
        self.socket_offset
    }

    pub fn attack_radius(&self, grip_progress: f32) -> f32 {
        let grip = self
            .primary_grip
            .lerp(self.secondary_grip, grip_progress.clamp(0.0, 1.0));
        self.attack_point.distance(grip)
    }

    pub fn secondary_grip(&self) -> Vec2 {
        self.secondary_grip
    }

    pub fn attack_point(&self) -> Vec2 {
        self.attack_point
    }

    pub fn maximum_reach(&self) -> f32 {
        self.attack_point.distance(self.reach_limit)
    }

    pub fn attack_components(&self) -> &[RuntimeComponentGeometry] {
        &self.attack_components
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeComponentGeometry {
    pub component_id: String,
    pub name: String,
    pub vertices: Vec<Vec2>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HammerCombatGeometryError(String);

impl HammerCombatGeometryError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for HammerCombatGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for HammerCombatGeometryError {}

fn unique_frame(manifest: &RuntimeManifest, role: &str) -> Result<Vec2, HammerCombatGeometryError> {
    attachment_frame(manifest, role)
        .map_err(content_to_geometry_error)
        .and_then(|frame| {
            finite_vec2(
                frame.asset_transform.position,
                &format!("{} attachment frame {role}", manifest.asset_key),
            )
        })
}

fn content_to_geometry_error(error: ContentError) -> HammerCombatGeometryError {
    HammerCombatGeometryError::new(error.to_string())
}

fn finite_vec2(values: [f32; 2], label: &str) -> Result<Vec2, HammerCombatGeometryError> {
    let value = Vec2::from_array(values);
    value.is_finite().then_some(value).ok_or_else(|| {
        HammerCombatGeometryError::new(format!("{label} must contain finite coordinates"))
    })
}

fn component_geometry(
    manifest: &RuntimeManifest,
    name: &str,
) -> Result<RuntimeComponentGeometry, HammerCombatGeometryError> {
    let component = manifest
        .components
        .iter()
        .find(|component| component.name == name)
        .ok_or_else(|| {
            HammerCombatGeometryError::new(format!("Hammer is missing attack Component {name}"))
        })?;
    let mesh = component.mesh.as_ref().ok_or_else(|| {
        HammerCombatGeometryError::new(format!("Hammer attack Component {name} has no Fill Mesh"))
    })?;
    let transform = component_world_transform(component, &manifest.components, &mut HashSet::new())
        .map_err(|error| HammerCombatGeometryError::new(error.to_string()))?;
    Ok(RuntimeComponentGeometry {
        component_id: component.component_id.clone(),
        name: component.name.clone(),
        vertices: mesh
            .vertices
            .iter()
            .map(|vertex| transform_point(transform, *vertex))
            .collect(),
        indices: mesh.indices.clone(),
    })
}

fn character_component_geometry(
    manifest: &RuntimeManifest,
    name: &str,
) -> Result<RuntimeComponentGeometry, CharacterHurtGeometryError> {
    let component = manifest
        .components
        .iter()
        .find(|component| component.name == name)
        .ok_or_else(|| {
            CharacterHurtGeometryError(format!(
                "{} is missing hurt Component {name}",
                manifest.asset_key
            ))
        })?;
    let mesh = component.mesh.as_ref().ok_or_else(|| {
        CharacterHurtGeometryError(format!(
            "{} hurt Component {name} has no Fill Mesh",
            manifest.asset_key
        ))
    })?;
    let transform = component_world_transform(component, &manifest.components, &mut HashSet::new())
        .map_err(|error| CharacterHurtGeometryError(error.to_string()))?;
    let pivot = finite_vec2(manifest.asset_pivot, "character asset pivot")
        .map_err(|error| CharacterHurtGeometryError(error.to_string()))?;
    Ok(RuntimeComponentGeometry {
        component_id: component.component_id.clone(),
        name: component.name.clone(),
        vertices: mesh
            .vertices
            .iter()
            .map(|vertex| transform_point(transform, *vertex) - pivot)
            .collect(),
        indices: mesh.indices.clone(),
    })
}

fn eye_beam_emitter(
    manifest: &RuntimeManifest,
    name: &str,
    eye_size_ratio: f32,
) -> Result<EyeBeamEmitterGeometry, MageEyeGeometryError> {
    let component = manifest
        .components
        .iter()
        .find(|component| component.name == name)
        .ok_or_else(|| MageEyeGeometryError(format!("Mage is missing {name}")))?;
    let region = component
        .closed_region_mesh
        .as_ref()
        .ok_or_else(|| MageEyeGeometryError(format!("Mage {name} has no closed eye region")))?;
    let transform = component_world_transform(component, &manifest.components, &mut HashSet::new())
        .map_err(|error| MageEyeGeometryError(error.to_string()))?;
    let pivot = Vec2::from_array(manifest.asset_pivot);
    let vertices = region
        .vertices
        .iter()
        .map(|vertex| transform_point(transform, *vertex) - pivot)
        .collect::<Vec<_>>();
    let minimum = vertices
        .iter()
        .copied()
        .fold(Vec2::splat(f32::INFINITY), Vec2::min);
    let maximum = vertices
        .iter()
        .copied()
        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
    let width = (maximum.x - minimum.x) * eye_size_ratio;
    let authored_offset = (minimum + maximum) * 0.5;
    let scale_origin = transform_point(transform, [0.0, 0.0]) - pivot;
    let offset = scale_origin + (authored_offset - scale_origin) * eye_size_ratio;
    if vertices.is_empty() || !offset.is_finite() || !width.is_finite() || width <= 0.0 {
        return Err(MageEyeGeometryError(format!(
            "Mage {name} does not define a valid eye span"
        )));
    }
    Ok(EyeBeamEmitterGeometry { offset, width })
}

fn region_geometry(region: &RuntimeRegion) -> RuntimeComponentGeometry {
    RuntimeComponentGeometry {
        component_id: region.region_id.clone(),
        name: region.name.clone(),
        vertices: region
            .vertices
            .iter()
            .map(|vertex| Vec2::from_array(*vertex))
            .collect(),
        indices: region.indices.clone(),
    }
}

fn transformed_mesh_area(
    component: &RuntimeComponent,
    components: &[RuntimeComponent],
) -> Result<f32, CharacterHealthError> {
    let Some(mesh) = &component.mesh else {
        return Ok(0.0);
    };
    let transform = component_world_transform(component, components, &mut HashSet::new())?;
    let mut area = 0.0;
    for triangle in mesh.indices.chunks_exact(3) {
        let a = transform_point(transform, mesh.vertices[triangle[0] as usize]);
        let b = transform_point(transform, mesh.vertices[triangle[1] as usize]);
        let c = transform_point(transform, mesh.vertices[triangle[2] as usize]);
        area += ((b - a).perp_dot(c - a)).abs() * 0.5;
    }
    Ok(area)
}

type Affine2 = (Vec2, Vec2, Vec2);

fn component_world_transform(
    component: &RuntimeComponent,
    components: &[RuntimeComponent],
    visiting: &mut HashSet<String>,
) -> Result<Affine2, CharacterHealthError> {
    if !visiting.insert(component.component_id.clone()) {
        return Err(CharacterHealthError("component transform cycle".into()));
    }
    let local = local_affine(&component.local_transform);
    let world = if let Some(parent_id) = &component.parent_component_id {
        let parent = components
            .iter()
            .find(|candidate| &candidate.component_id == parent_id)
            .ok_or_else(|| CharacterHealthError(format!("missing parent component {parent_id}")))?;
        compose(
            component_world_transform(parent, components, visiting)?,
            local,
        )
    } else {
        local
    };
    visiting.remove(&component.component_id);
    Ok(world)
}

fn local_affine(transform: &RuntimeTransform) -> Affine2 {
    let (sine, cosine) = transform.rotation_radians.sin_cos();
    (
        Vec2::new(cosine * transform.scale[0], sine * transform.scale[0]),
        Vec2::new(-sine * transform.scale[1], cosine * transform.scale[1]),
        Vec2::from_array(transform.position),
    )
}

fn compose(parent: Affine2, child: Affine2) -> Affine2 {
    (
        parent.0 * child.0.x + parent.1 * child.0.y,
        parent.0 * child.1.x + parent.1 * child.1.y,
        parent.0 * child.2.x + parent.1 * child.2.y + parent.2,
    )
}

fn transform_point(transform: Affine2, point: [f32; 2]) -> Vec2 {
    transform.0 * point[0] + transform.1 * point[1] + transform.2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_content_derives_health_and_hammer_geometry() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let health = CharacterHealthCatalog::from_content(&content)
            .expect("embedded character geometry defines health");
        let hammer = HammerCombatGeometry::from_content(
            &content,
            &["head_mid", "head_left", "head_right"].map(str::to_owned),
        )
        .expect("embedded Hammer frames define combat geometry");

        assert_eq!(health.max_hp(&CharacterId("hammerer".into())), Some(140.0));
        assert!(
            content
                .ids()
                .iter()
                .all(|character| health.max_hp(character).is_some())
        );
        assert!(hammer.socket_offset().is_finite());
        assert_eq!(
            hammer
                .attack_components()
                .iter()
                .map(|component| component.name.as_str())
                .collect::<Vec<_>>(),
            ["head_mid", "head_left", "head_right"]
        );
        assert!(
            hammer
                .attack_components()
                .iter()
                .all(|component| !component.vertices.is_empty() && !component.indices.is_empty())
        );
        assert!(hammer.attack_radius(1.0) > hammer.attack_radius(0.0));
        assert!(hammer.maximum_reach() > hammer.attack_radius(1.0));

        let hurt = CharacterHurtGeometryCatalog::from_content(&content)
            .expect("all playable characters define body and head hurt Components");
        assert!(content.ids().iter().all(|character| {
            hurt.character(character)
                .is_some_and(|geometry| geometry.components.len() == 2)
        }));
    }
}
