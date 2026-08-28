use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use bevy::prelude::{Resource, Vec2};
use game01_world_data::CharacterId;

use crate::manifest::{
    ContentError, RuntimeComponent, RuntimeContent, RuntimeManifest, RuntimeTransform,
    WEAPON_ATTACK_POINT_ROLE, WEAPON_GRIP_ROLE, WEAPON_REACH_LIMIT_ROLE,
    WEAPON_SECONDARY_GRIP_ROLE, WEAPON_SOCKET_ROLE, attachment_frame,
};

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct CharacterHealthCatalog {
    max_hp: HashMap<CharacterId, f32>,
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
            if total.is_finite() && total > 0.0 {
                areas.insert(asset_key.clone(), total);
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CharacterHealthError {}

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct HammerCombatGeometry {
    socket_offset: Vec2,
    primary_grip: Vec2,
    secondary_grip: Vec2,
    attack_point: Vec2,
    reach_limit: Vec2,
}

impl HammerCombatGeometry {
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
        let hammer = HammerCombatGeometry::from_content(&content)
            .expect("embedded Hammer frames define combat geometry");

        assert_eq!(health.max_hp(&CharacterId("hammerer".into())), Some(140.0));
        assert!(hammer.socket_offset().is_finite());
        assert!(hammer.attack_radius(1.0) > hammer.attack_radius(0.0));
        assert!(hammer.maximum_reach() > hammer.attack_radius(1.0));
    }
}
