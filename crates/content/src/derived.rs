use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::{Resource, Vec2};
use world01_world_data::{
    CharacterId, ComponentMassAssignment, ComponentMassClass, DensityClass,
    HealthGeometryDefinition, MassModelDefinition, Position, WorldMap,
};

use crate::manifest::{
    AuthoredFacing, ContentError, HAMMER_ASSET_KEY, RuntimeComponent, RuntimeContent,
    RuntimeManifest, RuntimeRegion, RuntimeTransform, WEAPON_ATTACK_POINT_ROLE, WEAPON_GRIP_ROLE,
    WEAPON_REACH_LIMIT_ROLE, WEAPON_SECONDARY_GRIP_ROLE, WEAPON_SOCKET_ROLE, attachment_frame,
};

/// Shorter collision edges do not define a trustworthy surface axis.
const MIN_COLLISION_EDGE_LENGTH_METERS: f32 = 0.000_01;

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
    /// The hurt surfaces every Character authors, one per Region with the
    /// `hurt` role.
    ///
    /// The Asset decides which parts can be hit; there is no list here and no
    /// fallback to a Component. A Character that authors no hurt Region simply
    /// has no entry and cannot be hit, the same way a Character without a
    /// `collision` Region occupies no space. Adding the Region later is all it
    /// takes, and a new Character needs no change in this repository at all.
    pub fn from_content(content: &RuntimeContent) -> Result<Self, CharacterHurtGeometryError> {
        let mut geometries = HashMap::new();
        for (character_id, manifest) in content.characters() {
            let components = manifest
                .regions
                .iter()
                .filter(|region| region.role == "hurt")
                .map(|region| {
                    let geometry = region_geometry(manifest, region)
                        .map_err(|error| CharacterHurtGeometryError(error.to_string()))?;
                    Ok(RuntimeComponentGeometry {
                        name: region_surface_name(manifest, region)
                            .map_err(CharacterHurtGeometryError)?,
                        ..geometry
                    })
                })
                .collect::<Result<Vec<_>, CharacterHurtGeometryError>>()?;
            if components.is_empty() {
                continue;
            }
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
    pub fn from_content(content: &RuntimeContent) -> Result<Self, MageEyeGeometryError> {
        let mage = content
            .character(&CharacterId("mage".into()))
            .ok_or_else(|| MageEyeGeometryError("content is missing Mage".into()))?;
        Ok(Self {
            authored_facing: mage.presentation.authored_facing,
            left: eye_beam_emitter(mage, "eye_left")?,
            right: eye_beam_emitter(mage, "eye_right")?,
        })
    }
}

/// The space a character occupies, as authored.
///
/// Deliberately not derived from the mesh and deliberately without a fallback.
/// A character whose manifest carries no `collision` Region simply occupies no
/// space: it neither blocks nor is blocked, and it can still be hit, because
/// hurt geometry is a separate authored concern. Adding the Region later is all
/// it takes to make that character solid.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterCollisionGeometry {
    pub authored_facing: AuthoredFacing,
    pub components: Vec<CollisionComponentGeometry>,
}

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct CharacterCollisionGeometryCatalog {
    geometries: HashMap<CharacterId, CharacterCollisionGeometry>,
}

impl CharacterCollisionGeometryCatalog {
    /// A character without a collision Region is absent from the catalog, which
    /// is a content state the design allows. A Region that cannot be resolved
    /// is something else entirely - a broken manifest - and fails.
    pub fn from_content(content: &RuntimeContent) -> Result<Self, RegionGeometryError> {
        let mut geometries = HashMap::new();
        for (character_id, manifest) in content.characters() {
            let components = manifest
                .regions
                .iter()
                .filter(|region| region.role == "collision")
                .map(|region| {
                    region_geometry(manifest, region)
                        .and_then(CollisionComponentGeometry::from_geometry)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if components.is_empty() {
                continue;
            }
            geometries.insert(
                character_id.clone(),
                CharacterCollisionGeometry {
                    authored_facing: manifest.presentation.authored_facing,
                    components,
                },
            );
        }
        Ok(Self { geometries })
    }

    /// Builds a catalog from geometry that did not come from a manifest, so a
    /// test can pin collision behaviour without depending on authored content.
    pub fn from_geometries(
        geometries: impl IntoIterator<Item = (CharacterId, CharacterCollisionGeometry)>,
    ) -> Self {
        Self {
            geometries: geometries.into_iter().collect(),
        }
    }

    pub fn character(&self, character: &CharacterId) -> Option<&CharacterCollisionGeometry> {
        self.geometries.get(character)
    }

    /// True while no character occupies space at all.
    pub fn is_empty(&self) -> bool {
        self.geometries.is_empty()
    }
}

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct WorldCollisionGeometryCatalog {
    pub regions: Vec<PlacedCollisionGeometry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedCollisionGeometry {
    pub instance_id: String,
    pub position: Position,
    pub component: CollisionComponentGeometry,
}

impl WorldCollisionGeometryCatalog {
    pub fn from_content_and_map(
        content: &RuntimeContent,
        map: &WorldMap,
    ) -> Result<Self, RegionGeometryError> {
        let mut regions = Vec::new();
        for placement in map.props() {
            // A placement whose Asset is not here is a broken world, not an
            // empty one. Passing over it would leave the Prop standing in the
            // picture while it blocks nothing, which is exactly the failure a
            // rename upstream produces.
            let Some(manifest) = content
                .prop(&placement.asset_key)
                .or_else(|| content.terrain(&placement.asset_key))
            else {
                return Err(RegionGeometryError(format!(
                    "'{}' places Asset '{}', which this content does not carry",
                    placement.instance_id, placement.asset_key
                )));
            };
            for region in &manifest.regions {
                if region.role != "collision" {
                    continue;
                }
                regions.push(PlacedCollisionGeometry {
                    instance_id: placement.instance_id.clone(),
                    position: placement.position,
                    component: CollisionComponentGeometry::from_geometry(region_geometry(
                        manifest, region,
                    )?)?,
                });
            }
        }
        Ok(Self { regions })
    }
}

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct WorldDestructibleGeometryCatalog {
    pub regions: Vec<PlacedDestructibleGeometry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedDestructibleGeometry {
    pub instance_id: String,
    pub position: Position,
    pub component: RuntimeComponentGeometry,
}

impl WorldDestructibleGeometryCatalog {
    /// The destructible surfaces every placed Prop authors, one per Region
    /// with the `destructible` role.
    ///
    /// Mirrors [`WorldCollisionGeometryCatalog::from_content_and_map`] in
    /// shape - translation-only, sourced from the map's placed Props, no
    /// facing applied, because a placed Prop does not turn - but keeps the
    /// same [`RuntimeComponentGeometry`] shape a Character's `hurt` Region
    /// already produces, since a destructible Prop is hit-tested the same
    /// way a Character is, not run through a physics query the way
    /// `collision` geometry is.
    ///
    /// A placed Prop that authors no `destructible` Region simply has no
    /// entry and cannot be hit, the same graceful absence `collision`
    /// already has.
    pub fn from_content_and_map(
        content: &RuntimeContent,
        map: &WorldMap,
    ) -> Result<Self, RegionGeometryError> {
        let mut regions = Vec::new();
        for placement in map.props() {
            // A placement whose Asset is not here is a broken world, not an
            // empty one, the same reasoning collision geometry uses above.
            let Some(manifest) = content
                .prop(&placement.asset_key)
                .or_else(|| content.terrain(&placement.asset_key))
            else {
                return Err(RegionGeometryError(format!(
                    "'{}' places Asset '{}', which this content does not carry",
                    placement.instance_id, placement.asset_key
                )));
            };
            for region in &manifest.regions {
                if region.role != "destructible" {
                    continue;
                }
                let geometry = region_geometry(manifest, region)?;
                regions.push(PlacedDestructibleGeometry {
                    instance_id: placement.instance_id.clone(),
                    position: placement.position,
                    component: RuntimeComponentGeometry {
                        name: region_surface_name(manifest, region).map_err(RegionGeometryError)?,
                        ..geometry
                    },
                });
            }
        }
        Ok(Self { regions })
    }
}

impl CharacterHealthCatalog {
    /// Maximum health per Character, from the Component areas the design names.
    ///
    /// A Character the definition does not name, or names with no Components,
    /// derives no area and gets [`Self::FALLBACK_MAX_HP`]. That is deliberate: a
    /// newly authored Character loads and plays without a design edit, and it
    /// gains real health once someone decides which of its parts count. Only
    /// closed shapes carry an area, so a named Component that draws an open
    /// Contour is an error rather than a silent zero.
    pub const FALLBACK_MAX_HP: f32 = 1.0;
    const HAMMERER_MAX_HP: f32 = 140.0;

    pub fn from_content(
        content: &RuntimeContent,
        definition: &HealthGeometryDefinition,
    ) -> Result<Self, CharacterHealthError> {
        let mut areas = HashMap::new();
        for (asset_key, manifest) in content.characters() {
            let Some(assignment) = definition.character(&asset_key.0) else {
                continue;
            };
            let mut total = 0.0;
            for name in &assignment.components {
                let component = manifest
                    .components
                    .iter()
                    .find(|component| component.name == *name)
                    .ok_or_else(|| {
                        CharacterHealthError(format!(
                            "{} health geometry names unknown Component {name}",
                            asset_key.0
                        ))
                    })?;
                if component.mesh.is_none() {
                    return Err(CharacterHealthError(format!(
                        "{} health geometry names Component {name}, which draws an open Contour and has no area",
                        asset_key.0
                    )));
                }
                total += transformed_mesh_area(component, &manifest.components)?;
            }
            if assignment.components.is_empty() {
                continue;
            }
            if !total.is_finite() || total <= 0.0 {
                return Err(CharacterHealthError(format!(
                    "{} health geometry encloses no positive area",
                    asset_key.0
                )));
            }
            areas.insert(asset_key.clone(), total);
        }
        for assignment in &definition.characters {
            if content
                .character(&CharacterId(assignment.asset_key.clone()))
                .is_none()
            {
                return Err(CharacterHealthError(format!(
                    "health geometry names unknown Character {}",
                    assignment.asset_key
                )));
            }
        }
        let hammerer = areas
            .get(&CharacterId("hammerer".into()))
            .copied()
            .ok_or_else(|| CharacterHealthError("missing Hammerer health area".into()))?;
        let mut max_hp: HashMap<CharacterId, f32> = areas
            .into_iter()
            .map(|(id, area)| (id, area / hammerer * Self::HAMMERER_MAX_HP))
            .collect();
        for (asset_key, _) in content.characters() {
            max_hp
                .entry(asset_key.clone())
                .or_insert(Self::FALLBACK_MAX_HP);
        }
        Ok(Self { max_hp })
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
            // A Character the definition does not name carries the default
            // classification on every one of its Components, so a newly
            // authored Character loads without a design edit.
            let assignment = definition
                .characters
                .iter()
                .find(|assignment| assignment.asset_key == character_id.0);
            let assigned_components = assignment
                .map(|assignment| assignment.components.as_slice())
                .unwrap_or_default();
            let body_areas = density_areas_for_manifest(manifest, assigned_components)?;
            let mut weapon_areas = DensityAreas::ZERO;
            let equipped = assignment
                .map(|assignment| assignment.equipped_weapon_asset_keys.as_slice())
                .unwrap_or_default();
            for weapon_key in equipped {
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
        for assignment in &definition.characters {
            if !body.contains_key(&CharacterId(assignment.asset_key.clone())) {
                return Err(CharacterMassGeometryError(format!(
                    "mass definition names unknown Character {}",
                    assignment.asset_key
                )));
            }
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

/// The class a Component carries when the design names no exception for it.
///
/// Medium, because it is the only default that keeps a Character's mass
/// positive: `weightless` derives density zero, and a Character whose parts
/// all weigh nothing derives no movement mass and no speed.
const DEFAULT_COMPONENT_MASS_CLASS: ComponentMassClass = ComponentMassClass::Medium;

fn density_areas_for_manifest(
    manifest: &RuntimeManifest,
    assignments: &[ComponentMassAssignment],
) -> Result<DensityAreas, CharacterMassGeometryError> {
    let mut assignments_by_name = HashMap::new();
    for assignment in assignments {
        assignments_by_name.insert(assignment.component_name.as_str(), assignment);
    }
    for assignment in assignments {
        let component = manifest
            .components
            .iter()
            .find(|component| component.name == assignment.component_name)
            .ok_or_else(|| {
                CharacterMassGeometryError(format!(
                    "{} mass assignments reference unknown Component {}",
                    manifest.asset_key, assignment.component_name
                ))
            })?;
        if component.mesh.is_none() && assignment.classification.density_class().is_some() {
            return Err(CharacterMassGeometryError(format!(
                "{} contour-only Component {} must be excluded from mass",
                manifest.asset_key, component.name
            )));
        }
    }
    let mut areas = DensityAreas::ZERO;
    for component in manifest
        .components
        .iter()
        .filter(|component| component.mesh.is_some())
    {
        // No entry means the default class rather than an error: the design
        // file states the exceptions, not an inventory of every drawn part.
        let classification = assignments_by_name
            .get(component.name.as_str())
            .map(|assignment| assignment.classification)
            .unwrap_or(DEFAULT_COMPONENT_MASS_CLASS);
        let Some(class) = classification.density_class() else {
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
    /// The attack surfaces come only from Regions the Asset authors with the
    /// `attack` role. There is deliberately no Component-name list in design:
    /// the Asset is the single place that decides what a weapon hits with, so
    /// a Hammer without an authored attack Region fails here instead of
    /// falling back to names a design file would have to enumerate.
    pub fn from_content(content: &RuntimeContent) -> Result<Self, HammerCombatGeometryError> {
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
        let attack_components = hammer
            .regions
            .iter()
            .filter(|region| region.role == "attack")
            .map(|region| {
                let geometry = region_geometry(hammer, region)
                    .map_err(|error| HammerCombatGeometryError::new(error.to_string()))?;
                Ok(RuntimeComponentGeometry {
                    name: region_surface_name(hammer, region)
                        .map_err(HammerCombatGeometryError::new)?,
                    ..geometry
                })
            })
            .collect::<Result<Vec<_>, HammerCombatGeometryError>>()?;
        if attack_components.is_empty() {
            return Err(HammerCombatGeometryError::new(
                "Hammer authors no attack Region",
            ));
        }
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

/// Geometry prepared specifically for collision queries.
///
/// Fill meshes are triangulated, but a triangulation edge shared by two
/// triangles is not part of the authored surface. Boundary edges are derived
/// once when a collision catalog is built so simulation never has to recover
/// that topology per contact. Endpoint positions, rather than vertex indices,
/// identify an edge because a triangulator may duplicate an exact position at
/// a seam. No proximity tolerance is applied; only equal exported positions
/// are welded.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionComponentGeometry {
    geometry: RuntimeComponentGeometry,
    boundary_edges: Vec<[Vec2; 2]>,
}

impl CollisionComponentGeometry {
    pub fn from_geometry(geometry: RuntimeComponentGeometry) -> Result<Self, RegionGeometryError> {
        if geometry.vertices.is_empty()
            || geometry.indices.is_empty()
            || geometry.indices.len() % 3 != 0
            || geometry.vertices.iter().any(|vertex| !vertex.is_finite())
            || geometry
                .indices
                .iter()
                .any(|index| *index as usize >= geometry.vertices.len())
        {
            return Err(RegionGeometryError(format!(
                "collision geometry '{}' is invalid",
                geometry.name
            )));
        }
        let mut referenced_vertices = vec![false; geometry.vertices.len()];
        for index in &geometry.indices {
            referenced_vertices[*index as usize] = true;
        }
        if referenced_vertices.iter().any(|referenced| !referenced) {
            return Err(RegionGeometryError(format!(
                "collision geometry '{}' contains an unreferenced vertex",
                geometry.name
            )));
        }
        let mut edge_counts = BTreeMap::<[[u32; 2]; 2], (u8, [Vec2; 2])>::new();
        for triangle in geometry.indices.chunks_exact(3) {
            for edge in [
                [triangle[0], triangle[1]],
                [triangle[1], triangle[2]],
                [triangle[2], triangle[0]],
            ] {
                let points = [
                    geometry.vertices[edge[0] as usize],
                    geometry.vertices[edge[1] as usize],
                ];
                let keys = [position_key(points[0]), position_key(points[1])];
                if keys[0] == keys[1] {
                    return Err(RegionGeometryError(format!(
                        "collision geometry '{}' contains a degenerate edge",
                        geometry.name
                    )));
                }
                let (key, points) = if keys[0] < keys[1] {
                    (keys, points)
                } else {
                    ([keys[1], keys[0]], [points[1], points[0]])
                };
                let (count, _) = edge_counts.entry(key).or_insert((0, points));
                *count = count.saturating_add(1);
                if *count > 2 {
                    return Err(RegionGeometryError(format!(
                        "collision geometry '{}' has a non-manifold edge",
                        geometry.name
                    )));
                }
            }
        }
        let boundary_edges = edge_counts
            .into_iter()
            .filter_map(|(_, (count, points))| (count == 1).then_some(points))
            .collect::<Vec<_>>();
        if boundary_edges
            .iter()
            .any(|edge| edge[0].distance(edge[1]) <= MIN_COLLISION_EDGE_LENGTH_METERS)
        {
            return Err(RegionGeometryError(format!(
                "collision geometry '{}' contains a boundary edge shorter than {MIN_COLLISION_EDGE_LENGTH_METERS} m",
                geometry.name
            )));
        }
        if boundary_edges.is_empty() {
            return Err(RegionGeometryError(format!(
                "collision geometry '{}' has no boundary edges",
                geometry.name
            )));
        }
        Ok(Self {
            geometry,
            boundary_edges,
        })
    }

    pub fn geometry(&self) -> &RuntimeComponentGeometry {
        &self.geometry
    }

    pub fn boundary_edges(&self) -> &[[Vec2; 2]] {
        &self.boundary_edges
    }
}

fn position_key(point: Vec2) -> [u32; 2] {
    [coordinate_key(point.x), coordinate_key(point.y)]
}

fn coordinate_key(value: f32) -> u32 {
    if value == 0.0 { 0 } else { value.to_bits() }
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

/// The effect key of one attack surface: the name of the Component its Region
/// is attached to.
///
/// A Region carries its own name too, but that is an editor label PolyTools
/// generates when the Region is created. The Component name is the one the
/// Asset author curated and reads in the Outliner, so a per-surface design
/// override keys on it and no second set of names has to be maintained.
fn region_surface_name(
    manifest: &RuntimeManifest,
    region: &RuntimeRegion,
) -> Result<String, String> {
    manifest
        .components
        .iter()
        .find(|component| component.component_id == region.source_component_id)
        .map(|component| component.name.clone())
        .ok_or_else(|| {
            format!(
                "{} {} Region '{}' is attached to Component {}, which the manifest does not have",
                manifest.asset_key, region.role, region.name, region.source_component_id
            )
        })
}

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

/// A Component's Fill Mesh in Asset space.
///
/// The Asset pivot is not subtracted here. Authored Region vertices arrive in
/// this same space, and a Region that borrows a Component has to land where the
/// authored one would, so the two paths must agree with each other. Every
/// Asset's pivot is the origin today, which is why the question of what a
/// non-zero pivot would mean for a Region has not had to be answered yet.
/// The Fill Meshes of the Asset an outer Reference instances, in the host
/// Asset's space and merged into one surface.
///
/// A Reference owns no Mesh: it names another Asset, whose Components the
/// content boundary resolves into `referenced_components` on load. Every one
/// of them is placed by its own transform inside that Asset and then by the
/// Reference's transform inside this one. They merge into a single geometry
/// because a Region is one surface — the Barde's belly is an Orb, not a list
/// of the Orb's parts — and overlap tests run per triangle, so a merged set
/// of several parts answers exactly like the parts would.
fn referenced_component_geometry(
    manifest: &RuntimeManifest,
    reference: &RuntimeComponent,
) -> Result<RuntimeComponentGeometry, String> {
    let reference_world =
        component_world_transform(reference, &manifest.components, &mut HashSet::new())
            .map_err(|error| error.to_string())?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for referenced in &reference.referenced_components {
        let Some(mesh) = referenced.mesh.as_ref() else {
            continue;
        };
        let inner = component_world_transform(
            referenced,
            &reference.referenced_components,
            &mut HashSet::new(),
        )
        .map_err(|error| error.to_string())?;
        let world = compose(reference_world, inner);
        let base = vertices.len() as u32;
        vertices.extend(
            mesh.vertices
                .iter()
                .map(|vertex| transform_point(world, *vertex)),
        );
        indices.extend(mesh.indices.iter().map(|index| index + base));
    }
    if vertices.is_empty() {
        return Err(format!(
            "Reference {} instances an Asset with no Fill Mesh to take geometry from",
            reference.name
        ));
    }
    Ok(RuntimeComponentGeometry {
        component_id: reference.component_id.clone(),
        name: reference.name.clone(),
        vertices,
        indices,
    })
}

fn placed_component_geometry(
    manifest: &RuntimeManifest,
    component: &RuntimeComponent,
) -> Result<RuntimeComponentGeometry, String> {
    let mesh = component.mesh.as_ref().ok_or_else(|| {
        format!(
            "Component {} has no Fill Mesh to take geometry from",
            component.name
        )
    })?;
    let transform = component_world_transform(component, &manifest.components, &mut HashSet::new())
        .map_err(|error| error.to_string())?;
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

fn eye_beam_emitter(
    manifest: &RuntimeManifest,
    name: &str,
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
    let width = maximum.x - minimum.x;
    let offset = (minimum + maximum) * 0.5;
    if vertices.is_empty() || !offset.is_finite() || !width.is_finite() || width <= 0.0 {
        return Err(MageEyeGeometryError(format!(
            "Mage {name} does not define a valid eye span"
        )));
    }
    Ok(EyeBeamEmitterGeometry { offset, width })
}

/// Components or Regions, whichever the Character declared.
fn region_geometry(
    manifest: &RuntimeManifest,
    region: &RuntimeRegion,
) -> Result<RuntimeComponentGeometry, RegionGeometryError> {
    if region.is_authored_geometry() {
        return Ok(RuntimeComponentGeometry {
            component_id: region.region_id.clone(),
            name: region.name.clone(),
            vertices: region
                .vertices
                .iter()
                .map(|vertex| Vec2::from_array(*vertex))
                .collect(),
            indices: region.indices.clone(),
        });
    }
    let component = manifest
        .components
        .iter()
        .find(|component| component.component_id == region.source_component_id)
        .ok_or_else(|| {
            RegionGeometryError(format!(
                "{} Region '{}' names Component {}, which the manifest does not have",
                manifest.asset_key, region.name, region.source_component_id
            ))
        })?;
    let geometry = if component.source_asset_key.is_some() {
        referenced_component_geometry(manifest, component)
    } else {
        placed_component_geometry(manifest, component)
    }
    .map_err(|message| {
        RegionGeometryError(format!(
            "{} Region '{}': {message}",
            manifest.asset_key, region.name
        ))
    })?;
    Ok(RuntimeComponentGeometry {
        component_id: region.region_id.clone(),
        name: region.name.clone(),
        ..geometry
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionGeometryError(String);

impl fmt::Display for RegionGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for RegionGeometryError {}

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
    use world01_world_data::{
        CharacterHealthAssignment, ComponentMassAssignment, ComponentMassClass,
    };

    #[test]
    fn embedded_content_derives_health_and_hammer_geometry() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let health = CharacterHealthCatalog::from_content(&content, &health_geometry())
            .expect("embedded character geometry defines health");
        let hammer = HammerCombatGeometry::from_content(&content)
            .expect("embedded Hammer frames define combat geometry");

        assert_eq!(health.max_hp(&CharacterId("hammerer".into())), Some(140.0));
        assert!(
            content
                .ids()
                .iter()
                .all(|character| health.max_hp(character).is_some())
        );
        assert!(hammer.socket_offset().is_finite());
        let mut attack_surfaces = hammer
            .attack_components()
            .iter()
            .map(|component| component.name.as_str())
            .collect::<Vec<_>>();
        attack_surfaces.sort_unstable();
        assert_eq!(attack_surfaces, ["head_left", "head_mid", "head_right"]);
        assert!(hammer.attack_components().iter().all(|component| {
            !component.name.is_empty()
                && !component.vertices.is_empty()
                && !component.indices.is_empty()
        }));
        assert!(hammer.attack_radius(1.0) > hammer.attack_radius(0.0));
        assert!(hammer.maximum_reach() > hammer.attack_radius(1.0));

        // Hurt surfaces are authored, so a Character has an entry exactly when it
        // authors hurt Regions, and every surface it does author resolves.
        let hurt = CharacterHurtGeometryCatalog::from_content(&content)
            .expect("every authored hurt Region resolves");
        assert!(content.ids().iter().all(|character| {
            let authored = content
                .character(character)
                .map(|manifest| {
                    manifest
                        .regions
                        .iter()
                        .filter(|region| region.role == "hurt")
                        .count()
                })
                .unwrap_or_default();
            match hurt.character(character) {
                Some(geometry) => geometry.components.len() == authored,
                None => authored == 0,
            }
        }));

        let character_collision = CharacterCollisionGeometryCatalog::from_content(&content)
            .expect("embedded Character collision topology is valid");
        assert!(content.ids().iter().all(|character| {
            character_collision
                .character(character)
                .is_some_and(|geometry| {
                    geometry
                        .components
                        .iter()
                        .all(|component| !component.boundary_edges().is_empty())
                })
        }));

        let map =
            WorldMap::load_embedded("sandbox", "overworld01").expect("embedded world map is valid");
        let world_collision = WorldCollisionGeometryCatalog::from_content_and_map(&content, &map)
            .expect("embedded world collision topology is valid");
        assert!(!world_collision.regions.is_empty());
        assert!(
            world_collision
                .regions
                .iter()
                .all(|region| !region.component.boundary_edges().is_empty())
        );

        // Whether any shipped Prop carries a `destructible` Region depends on
        // the last content sync, so this holds either way: the catalog loads,
        // and whatever it found has usable geometry. A Prop without the Region
        // has no entry, like one without `collision`.
        let world_destructible =
            WorldDestructibleGeometryCatalog::from_content_and_map(&content, &map)
                .expect("embedded world destructible topology is valid");
        assert!(world_destructible.regions.iter().all(|region| {
            !region.component.vertices.is_empty() && !region.component.indices.is_empty()
        }));
    }

    /// What the shipped design also declares: the body and, where a Character
    /// draws one, the feet. Built here rather than read from the design crate so
    /// the importer is tested against a definition this test controls. Not every
    /// Character draws feet — the Chantres and the Sorcerer do not — and naming a
    /// part a Character does not have is an error rather than a skipped zero.
    fn health_geometry() -> HealthGeometryDefinition {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        HealthGeometryDefinition {
            schema_version: 1,
            characters: content
                .ids()
                .iter()
                .map(|character| {
                    let manifest = content
                        .character(character)
                        .expect("the roster lists only Characters the content has");
                    CharacterHealthAssignment {
                        asset_key: character.0.clone(),
                        components: ["body", "feet"]
                            .into_iter()
                            .filter(|name| {
                                manifest.components.iter().any(|component| {
                                    component.name == *name && component.mesh.is_some()
                                })
                            })
                            .map(str::to_owned)
                            .collect(),
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn a_character_the_health_geometry_does_not_name_falls_back_to_one_hit_point() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let mut definition = health_geometry();
        let dropped = definition
            .characters
            .iter()
            .position(|assignment| assignment.asset_key != "hammerer")
            .expect("the roster has a Character beside the Hammerer");
        let dropped = definition.characters.remove(dropped);
        let health = CharacterHealthCatalog::from_content(&content, &definition)
            .expect("an unnamed Character is not an error");

        assert_eq!(
            health.max_hp(&CharacterId(dropped.asset_key.clone())),
            Some(CharacterHealthCatalog::FALLBACK_MAX_HP)
        );
        assert_eq!(
            health.max_hp(&CharacterId("hammerer".into())),
            Some(140.0),
            "and the Hammerer stays the reference the others scale against"
        );
    }

    #[test]
    fn health_geometry_rejects_a_component_the_character_does_not_draw() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let mut definition = health_geometry();
        definition.characters[0].components = vec!["shoulder_pad".to_owned()];

        assert!(CharacterHealthCatalog::from_content(&content, &definition).is_err());
    }

    fn collision_geometry(vertices: Vec<Vec2>, indices: Vec<u32>) -> RuntimeComponentGeometry {
        RuntimeComponentGeometry {
            component_id: "test_collision".into(),
            name: "test_collision".into(),
            vertices,
            indices,
        }
    }

    fn square_vertices() -> Vec<Vec2> {
        vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]
    }

    fn contains_edge(edges: &[[Vec2; 2]], first: Vec2, second: Vec2) -> bool {
        edges
            .iter()
            .any(|edge| *edge == [first, second] || *edge == [second, first])
    }

    #[test]
    fn collision_boundary_excludes_the_shared_triangulation_edge() {
        let source = collision_geometry(square_vertices(), vec![0, 1, 2, 0, 2, 3]);
        let geometry = CollisionComponentGeometry::from_geometry(source.clone())
            .expect("two triangles form valid collision geometry");
        let repeated = CollisionComponentGeometry::from_geometry(source)
            .expect("the same geometry remains valid");

        assert_eq!(geometry.boundary_edges().len(), 4);
        assert!(!contains_edge(
            geometry.boundary_edges(),
            Vec2::ZERO,
            Vec2::ONE
        ));
        assert_eq!(geometry.boundary_edges(), repeated.boundary_edges());
    }

    #[test]
    fn collision_boundary_welds_equal_positions_with_different_indices() {
        let vertices = vec![
            Vec2::ZERO,
            Vec2::X,
            Vec2::ONE,
            Vec2::ZERO,
            Vec2::ONE,
            Vec2::Y,
        ];
        let geometry = CollisionComponentGeometry::from_geometry(collision_geometry(
            vertices,
            vec![0, 1, 2, 3, 4, 5],
        ))
        .expect("an indexed seam still forms valid collision geometry");

        assert_eq!(geometry.boundary_edges().len(), 4);
        assert!(!contains_edge(
            geometry.boundary_edges(),
            Vec2::ZERO,
            Vec2::ONE
        ));
    }

    #[test]
    fn a_non_manifold_collision_edge_is_rejected() {
        let vertices = vec![
            Vec2::ZERO,
            Vec2::X,
            Vec2::Y,
            Vec2::NEG_Y,
            Vec2::new(0.5, 1.0),
        ];
        let error = CollisionComponentGeometry::from_geometry(collision_geometry(
            vertices,
            vec![0, 1, 2, 1, 0, 3, 0, 1, 4],
        ))
        .expect_err("three triangles cannot share one collision edge");

        assert!(error.to_string().contains("non-manifold edge"));
    }

    #[test]
    fn an_unreferenced_collision_vertex_is_rejected() {
        let mut vertices = square_vertices();
        vertices.push(Vec2::splat(100.0));
        let error = CollisionComponentGeometry::from_geometry(collision_geometry(
            vertices,
            vec![0, 1, 2, 0, 2, 3],
        ))
        .expect_err("unreferenced vertices must not enlarge collision bounds");

        assert!(error.to_string().contains("unreferenced vertex"));
    }

    #[test]
    fn a_collision_edge_below_the_surface_threshold_is_rejected() {
        let vertices = vec![
            Vec2::ZERO,
            Vec2::new(MIN_COLLISION_EDGE_LENGTH_METERS * 0.5, 0.0),
            Vec2::Y,
        ];
        let error =
            CollisionComponentGeometry::from_geometry(collision_geometry(vertices, vec![0, 1, 2]))
                .expect_err("a near-degenerate edge cannot define a surface axis");

        assert!(error.to_string().contains("edge shorter than"));
    }

    #[test]
    fn contour_only_components_are_automatically_excluded_from_mass() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let mage = content
            .character(&CharacterId("mage".into()))
            .expect("embedded content contains Mage");
        let assignments = [
            ("body", ComponentMassClass::Medium),
            ("head", ComponentMassClass::Medium),
            ("hat", ComponentMassClass::Light),
            ("hat_tip", ComponentMassClass::Light),
        ]
        .map(|(component_name, classification)| ComponentMassAssignment {
            component_name: component_name.into(),
            classification,
        });

        let areas = density_areas_for_manifest(mage, &assignments)
            .expect("unassigned contour-only Components do not affect mass");

        assert!(areas.area(DensityClass::Medium) > 0.0);
        assert!(areas.area(DensityClass::Light) > 0.0);
    }

    #[test]
    fn contour_only_components_cannot_receive_density() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let mage = content
            .character(&CharacterId("mage".into()))
            .expect("embedded content contains Mage");
        let assignments = [ComponentMassAssignment {
            component_name: "arm_line".into(),
            classification: ComponentMassClass::Medium,
        }];

        let error = density_areas_for_manifest(mage, &assignments)
            .expect_err("contour-only Components cannot contribute mass");

        assert!(error.to_string().contains("must be excluded from mass"));
    }
}
