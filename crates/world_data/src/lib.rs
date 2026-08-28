//! Shared protocol-neutral domain data.

use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::{Component, Reflect, Resource, Vec2},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    f32::consts::TAU,
    fmt,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterId(pub String);

impl CharacterId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()).then_some(Self(value))
    }

    pub fn label(&self) -> String {
        self.0
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct CharacterCatalog {
    ids: HashSet<CharacterId>,
}

impl CharacterCatalog {
    pub fn from_json(source: &str) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct Catalog {
            assets: Vec<Asset>,
        }
        #[derive(Deserialize)]
        struct Asset {
            asset_key: String,
            asset_type: String,
        }
        let catalog: Catalog = serde_json::from_str(source)?;
        let ids = catalog
            .assets
            .into_iter()
            .filter(|asset| asset.asset_type == "character")
            .filter_map(|asset| CharacterId::new(asset.asset_key))
            .collect();
        Ok(Self { ids })
    }

    pub fn contains(&self, id: &CharacterId) -> bool {
        self.ids.contains(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &CharacterId> {
        self.ids.iter()
    }
}

/// Runtime-derived health values. Areas come from the current exported meshes;
/// no character-specific HP table is stored in design configuration.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CharacterHealthCatalog {
    max_hp: HashMap<CharacterId, f32>,
}

impl CharacterHealthCatalog {
    pub fn from_manifests<'a>(
        manifests: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, CharacterHealthError> {
        let mut areas = HashMap::new();
        for source in manifests {
            let manifest: HealthManifest = serde_json::from_str(source).map_err(|error| {
                CharacterHealthError(format!("invalid character manifest: {error}"))
            })?;
            let Some(asset_key) = CharacterId::new(manifest.asset_key) else {
                continue;
            };
            let mut total = 0.0;
            for component in &manifest.components {
                if component.name != "body" && component.name != "feet" {
                    continue;
                }
                total += transformed_mesh_area(component, &manifest.components)?;
            }
            if total.is_finite() && total > 0.0 {
                areas.insert(asset_key, total);
            }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterHealthError(String);
impl fmt::Display for CharacterHealthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for CharacterHealthError {}

#[derive(Deserialize)]
struct HealthManifest {
    asset_key: String,
    components: Vec<HealthComponent>,
}
#[derive(Deserialize)]
struct HealthComponent {
    component_id: String,
    name: String,
    parent_component_id: Option<String>,
    local_transform: HealthTransform,
    mesh: Option<HealthMesh>,
}
#[derive(Deserialize)]
struct HealthTransform {
    position: [f32; 2],
    rotation_radians: f32,
    scale: [f32; 2],
}
#[derive(Deserialize)]
struct HealthMesh {
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

fn transformed_mesh_area(
    component: &HealthComponent,
    components: &[HealthComponent],
) -> Result<f32, CharacterHealthError> {
    let Some(mesh) = &component.mesh else {
        return Ok(0.0);
    };
    if mesh.indices.len() % 3 != 0
        || mesh
            .indices
            .iter()
            .any(|&i| i as usize >= mesh.vertices.len())
    {
        return Err(CharacterHealthError(format!(
            "invalid mesh for component {}",
            component.name
        )));
    }
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

type Affine2 = (Vec2, Vec2, Vec2); // columns and translation
fn component_world_transform(
    component: &HealthComponent,
    components: &[HealthComponent],
    visiting: &mut HashSet<String>,
) -> Result<Affine2, CharacterHealthError> {
    if !visiting.insert(component.component_id.clone()) {
        return Err(CharacterHealthError("component transform cycle".into()));
    }
    let local = local_affine(&component.local_transform);
    let world = if let Some(parent_id) = &component.parent_component_id {
        let parent = components
            .iter()
            .find(|c| &c.component_id == parent_id)
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
fn local_affine(t: &HealthTransform) -> Affine2 {
    let (s, c) = t.rotation_radians.sin_cos();
    (
        Vec2::new(c * t.scale[0], s * t.scale[0]),
        Vec2::new(-s * t.scale[1], c * t.scale[1]),
        Vec2::from_array(t.position),
    )
}
fn compose(a: Affine2, b: Affine2) -> Affine2 {
    (
        a.0 * b.0.x + a.1 * b.0.y,
        a.0 * b.1.x + a.1 * b.1.y,
        a.0 * b.2.x + a.1 * b.2.y + a.2,
    )
}
fn transform_point(t: Affine2, p: [f32; 2]) -> Vec2 {
    t.0 * p[0] + t.1 * p[1] + t.2
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
pub struct CharacterHealth {
    pub current: f32,
    pub maximum: f32,
}
impl CharacterHealth {
    pub fn full(maximum: f32) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u64);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerOwner(pub u64);

#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedCharacter(pub CharacterId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Player;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(pub u32);

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct StartingRoomGrid {
    width_meters: f32,
    height_meters: f32,
}

impl Default for StartingRoomGrid {
    fn default() -> Self {
        Self::from_tiles(15, 9).expect("default room dimensions are valid")
    }
}

impl StartingRoomGrid {
    const MINIMUM_COORDINATE: i32 = -1;
    const MAXIMUM_COORDINATE: i32 = 1;

    pub fn from_tiles(width_tiles: u32, height_tiles: u32) -> Option<Self> {
        (width_tiles > 0 && height_tiles > 0).then_some(Self {
            width_meters: width_tiles as f32,
            height_meters: height_tiles as f32,
        })
    }

    pub const fn width_meters(self) -> f32 {
        self.width_meters
    }

    pub const fn height_meters(self) -> f32 {
        self.height_meters
    }

    pub fn constrain_position(self, position: Position) -> Position {
        Position::new(
            position.x.clamp(self.minimum_x(), self.maximum_x()),
            position.y.clamp(self.minimum_y(), self.maximum_y()),
        )
    }

    pub fn room_id_at(self, position: Position) -> RoomId {
        let position = self.constrain_position(position);
        let x = ((position.x - self.minimum_x()) / self.width_meters)
            .floor()
            .clamp(0.0, 2.0) as i32
            + Self::MINIMUM_COORDINATE;
        let y = ((position.y - self.minimum_y()) / self.height_meters)
            .floor()
            .clamp(0.0, 2.0) as i32
            + Self::MINIMUM_COORDINATE;
        Self::room_id_for_coordinates(x, y)
    }

    pub fn room_floor_minimum(self, room: RoomId) -> Vec2 {
        let (x, y) = Self::coordinates_for_room_id(room);
        Vec2::new(
            (x as f32 - 0.5) * self.width_meters,
            (y as f32 - 0.5) * self.height_meters,
        )
    }

    pub fn room_center(self, room: RoomId) -> Vec2 {
        self.room_floor_minimum(room) + Vec2::new(self.width_meters, self.height_meters) * 0.5
    }

    pub const fn starting_room(self) -> RoomId {
        Self::room_id_for_coordinates(0, 0)
    }

    const fn room_id_for_coordinates(x: i32, y: i32) -> RoomId {
        RoomId(((y + 1) * 3 + (x + 1)) as u32)
    }

    fn coordinates_for_room_id(room: RoomId) -> (i32, i32) {
        let index = room.0.min(8) as i32;
        (index % 3 - 1, index / 3 - 1)
    }

    const fn minimum_x(self) -> f32 {
        (Self::MINIMUM_COORDINATE as f32 - 0.5) * self.width_meters
    }

    const fn maximum_x(self) -> f32 {
        (Self::MAXIMUM_COORDINATE as f32 + 0.5) * self.width_meters
    }

    const fn minimum_y(self) -> f32 {
        (Self::MINIMUM_COORDINATE as f32 - 0.5) * self.height_meters
    }

    const fn maximum_y(self) -> f32 {
        (Self::MAXIMUM_COORDINATE as f32 + 0.5) * self.height_meters
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StandardRoom;

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub room: RoomId,
    pub x_meters: f32,
    pub y_meters: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct MovementIntent {
    pub x: f32,
    pub y: f32,
}

impl MapEntities for MovementIntent {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

impl MovementIntent {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct MovementDirection {
    pub x: f32,
    pub y: f32,
}

impl MovementDirection {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct GazeIntent {
    pub x: f32,
    pub y: f32,
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct AttackIntent {
    pub pressed: bool,
}

impl AttackIntent {
    pub const RELEASED: Self = Self { pressed: false };
    pub const PRESSED: Self = Self { pressed: true };

    pub const fn new(pressed: bool) -> Self {
        Self { pressed }
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub enum HammerAttackPhase {
    #[default]
    Idle,
    Charging,
    Swing,
    Embedded,
    Recovery,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct HammerAttackState {
    pub phase: HammerAttackPhase,
    pub direction: GazeDirection,
    pub phase_ticks: u32,
    pub charge_ticks: u32,
    pub impact_point: Position,
}

impl HammerAttackState {
    pub const IDLE: Self = Self {
        phase: HammerAttackPhase::Idle,
        direction: GazeDirection::ZERO,
        phase_ticks: 0,
        charge_ticks: 0,
        impact_point: Position::ZERO,
    };
}

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct HammerCombatGeometry {
    socket_offset: Vec2,
    primary_grip: Vec2,
    secondary_grip: Vec2,
    attack_point: Vec2,
    reach_limit: Vec2,
}

impl HammerCombatGeometry {
    pub fn from_runtime_manifests(
        hammerer_source: &str,
        hammer_source: &str,
    ) -> Result<Self, HammerCombatGeometryError> {
        let hammerer: CombatManifest = serde_json::from_str(hammerer_source).map_err(|error| {
            HammerCombatGeometryError::new(format!("invalid Hammerer manifest: {error}"))
        })?;
        let hammer: CombatManifest = serde_json::from_str(hammer_source).map_err(|error| {
            HammerCombatGeometryError::new(format!("invalid Hammer manifest: {error}"))
        })?;
        if !matches!(hammerer.schema_version, 11 | 12) || hammerer.asset_key != "hammerer" {
            return Err(HammerCombatGeometryError::new(
                "Hammerer combat geometry requires its schema-11 manifest",
            ));
        }
        if !matches!(hammer.schema_version, 11 | 12) || hammer.asset_key != "hammer" {
            return Err(HammerCombatGeometryError::new(
                "Hammer combat geometry requires its schema-11 manifest",
            ));
        }
        let hammerer_pivot = finite_vec2(hammerer.asset_pivot, "Hammerer asset pivot")?;
        let socket = unique_frame(&hammerer, "weapon_socket_primary")?;
        let primary_grip = unique_frame(&hammer, "grip_primary")?;
        let secondary_grip = unique_frame(&hammer, "grip_secondary")?;
        let attack_point = unique_frame(&hammer, "attack_point_primary")?;
        let reach_limit = unique_frame(&hammer, "reach_limit_primary")?;
        let geometry = Self {
            socket_offset: socket - hammerer_pivot,
            primary_grip,
            secondary_grip,
            attack_point,
            reach_limit,
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

    pub fn socket_offset(self) -> Vec2 {
        self.socket_offset
    }

    pub fn attack_radius(self, grip_progress: f32) -> f32 {
        let grip = self
            .primary_grip
            .lerp(self.secondary_grip, grip_progress.clamp(0.0, 1.0));
        self.attack_point.distance(grip)
    }

    pub fn maximum_reach(self) -> f32 {
        self.attack_point.distance(self.reach_limit)
    }
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

#[derive(Deserialize)]
struct CombatManifest {
    schema_version: u32,
    asset_key: String,
    asset_pivot: [f32; 2],
    attachment_frames: Vec<CombatAttachmentFrame>,
}

#[derive(Deserialize)]
struct CombatAttachmentFrame {
    role: String,
    asset_transform: CombatFrameTransform,
}

#[derive(Deserialize)]
struct CombatFrameTransform {
    position: [f32; 2],
}

fn unique_frame(manifest: &CombatManifest, role: &str) -> Result<Vec2, HammerCombatGeometryError> {
    let mut frames = manifest
        .attachment_frames
        .iter()
        .filter(|frame| frame.role == role);
    let frame = frames.next().ok_or_else(|| {
        HammerCombatGeometryError::new(format!(
            "{} is missing attachment frame {role}",
            manifest.asset_key
        ))
    })?;
    if frames.next().is_some() {
        return Err(HammerCombatGeometryError::new(format!(
            "{} contains duplicate attachment frame {role}",
            manifest.asset_key
        )));
    }
    finite_vec2(
        frame.asset_transform.position,
        &format!("{} attachment frame {role}", manifest.asset_key),
    )
}

fn finite_vec2(values: [f32; 2], label: &str) -> Result<Vec2, HammerCombatGeometryError> {
    let value = Vec2::from_array(values);
    value.is_finite().then_some(value).ok_or_else(|| {
        HammerCombatGeometryError::new(format!("{label} must contain finite coordinates"))
    })
}

impl GazeIntent {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct GazeDirection {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub enum WeaponTurnDirection {
    #[default]
    Clockwise,
    CounterClockwise,
}

impl WeaponTurnDirection {
    pub const fn angle_sign(self) -> f32 {
        match self {
            Self::Clockwise => -1.0,
            Self::CounterClockwise => 1.0,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
pub struct WeaponAimState {
    pub angle_radians: f32,
    pub last_turn_direction: WeaponTurnDirection,
}

impl WeaponAimState {
    pub const RIGHT: Self = Self {
        angle_radians: 0.0,
        last_turn_direction: WeaponTurnDirection::Clockwise,
    };

    pub fn new(angle_radians: f32, last_turn_direction: WeaponTurnDirection) -> Self {
        Self {
            angle_radians: angle_radians.rem_euclid(TAU),
            last_turn_direction,
        }
    }

    pub fn direction(self) -> GazeDirection {
        GazeDirection::new(self.angle_radians.cos(), self.angle_radians.sin())
    }
}

impl Default for WeaponAimState {
    fn default() -> Self {
        Self::RIGHT
    }
}

impl GazeDirection {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const RIGHT: Self = Self { x: 1.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub enum BodyFacing {
    #[default]
    Authored,
    Left,
    Right,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct PlayerInput {
    pub movement: MovementIntent,
    pub gaze: GazeIntent,
    pub attack: AttackIntent,
}

impl PlayerInput {
    pub const ZERO: Self = Self {
        movement: MovementIntent::ZERO,
        gaze: GazeIntent::ZERO,
        attack: AttackIntent::RELEASED,
    };

    pub const fn new(movement: MovementIntent, gaze: GazeIntent) -> Self {
        Self {
            movement,
            gaze,
            attack: AttackIntent::RELEASED,
        }
    }
}

impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_grid_uses_the_same_room_for_origin_and_diagonals() {
        let grid = StartingRoomGrid::default();

        assert_eq!(grid.starting_room(), RoomId(4));
        assert_eq!(grid.room_id_at(Position::ZERO), RoomId(4));
        assert_eq!(grid.room_id_at(Position::new(7.5, 4.5)), RoomId(8));
    }

    #[test]
    fn starting_grid_clamps_positions_to_its_open_neighbor_set() {
        let grid = StartingRoomGrid::default();
        let position = grid.constrain_position(Position::new(30.0, -20.0));

        assert_eq!(position, Position::new(22.5, -13.5));
        assert_eq!(grid.room_id_at(position), RoomId(2));
    }

    #[test]
    fn embedded_hammer_combat_geometry_comes_from_schema_eleven_frames() {
        let geometry = HammerCombatGeometry::from_runtime_manifests(
            include_str!("../../../assets/characters/hammerer/manifest.json"),
            include_str!("../../../assets/characters/hammer/manifest.json"),
        )
        .expect("synced Hammer manifests define valid combat geometry");

        assert!(geometry.socket_offset().is_finite());
        assert!(geometry.attack_radius(0.0) > 0.0);
        assert!(geometry.attack_radius(0.5) > geometry.attack_radius(0.0));
        assert!(geometry.attack_radius(1.0) > geometry.attack_radius(0.5));
        assert!(geometry.maximum_reach() > geometry.attack_radius(1.0));
    }
}
