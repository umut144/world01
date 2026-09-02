use bevy::prelude::Vec2;
use world01_content::{AuthoredFacing, CharacterHurtGeometry, RuntimeComponentGeometry};
use world01_world_data::{BodyFacing, Position};

#[derive(Debug, Clone, Copy)]
pub(crate) struct GeometryTransform {
    pub origin: Vec2,
    pub axis_x: Vec2,
    pub axis_y: Vec2,
}

impl GeometryTransform {
    pub const IDENTITY: Self = Self {
        origin: Vec2::ZERO,
        axis_x: Vec2::X,
        axis_y: Vec2::Y,
    };

    pub fn translated(position: Position) -> Self {
        Self {
            origin: Vec2::new(position.x, position.y),
            ..Self::IDENTITY
        }
    }
}

/// Places geometry authored for one side at a position, mirrored if the actor
/// faces the other way.
///
/// Hurt and collision geometry are separate concerns with separate authoring,
/// but they are posed identically, so they share this.
pub(crate) fn facing_transform(
    authored: AuthoredFacing,
    position: Position,
    facing: BodyFacing,
) -> GeometryTransform {
    let mirrored = matches!(
        (authored, facing),
        (AuthoredFacing::Left, BodyFacing::Right) | (AuthoredFacing::Right, BodyFacing::Left)
    );
    GeometryTransform {
        origin: Vec2::new(position.x, position.y),
        axis_x: if mirrored { -Vec2::X } else { Vec2::X },
        axis_y: Vec2::Y,
    }
}

pub(crate) fn hurt_transform(
    geometry: &CharacterHurtGeometry,
    position: Position,
    facing: BodyFacing,
) -> GeometryTransform {
    facing_transform(geometry.authored_facing, position, facing)
}

pub(crate) fn posed_hurt_transform(
    geometry: &CharacterHurtGeometry,
    position: Position,
    facing: BodyFacing,
    scale: f32,
    rotation_radians: f32,
) -> GeometryTransform {
    let mut transform = hurt_transform(geometry, position, facing);
    let rotation = Vec2::from_angle(rotation_radians) * scale;
    transform.axis_x = rotate(rotation, transform.axis_x);
    transform.axis_y = rotate(rotation, transform.axis_y);
    transform
}

pub(crate) fn rotate(axis_x: Vec2, point: Vec2) -> Vec2 {
    axis_x * point.x + axis_x.perp() * point.y
}

pub(crate) fn components_overlap(
    first: &RuntimeComponentGeometry,
    first_transform: GeometryTransform,
    second: &RuntimeComponentGeometry,
    second_transform: GeometryTransform,
) -> bool {
    for first_triangle in first.indices.chunks_exact(3) {
        let first_points = triangle_points(first, first_transform, first_triangle);
        for second_triangle in second.indices.chunks_exact(3) {
            let second_points = triangle_points(second, second_transform, second_triangle);
            if triangles_overlap(first_points, second_points) {
                return true;
            }
        }
    }
    false
}

/// The deepest contact between two posed Components.
///
/// Deepest rather than first: a rounded body meets a box across several
/// triangles at once, and the pair that is furthest in is the one whose surface
/// the actor is actually running into.
pub(crate) fn components_contact(
    first: &RuntimeComponentGeometry,
    first_transform: GeometryTransform,
    second: &RuntimeComponentGeometry,
    second_transform: GeometryTransform,
) -> Option<Contact> {
    let mut deepest: Option<Contact> = None;
    for first_triangle in first.indices.chunks_exact(3) {
        let first_points = triangle_points(first, first_transform, first_triangle);
        for second_triangle in second.indices.chunks_exact(3) {
            let second_points = triangle_points(second, second_transform, second_triangle);
            let Some(contact) = triangle_contact(first_points, second_points) else {
                continue;
            };
            if deepest.is_none_or(|best| contact.depth > best.depth) {
                deepest = Some(contact);
            }
        }
    }
    deepest
}

fn triangle_points(
    component: &RuntimeComponentGeometry,
    transform: GeometryTransform,
    indices: &[u32],
) -> [Vec2; 3] {
    [
        transform_point(transform, component.vertices[indices[0] as usize]),
        transform_point(transform, component.vertices[indices[1] as usize]),
        transform_point(transform, component.vertices[indices[2] as usize]),
    ]
}

fn transform_point(transform: GeometryTransform, point: Vec2) -> Vec2 {
    transform.origin + transform.axis_x * point.x + transform.axis_y * point.y
}

/// Every vertex of a component, posed. The broad phase bounds geometry with
/// this rather than reaching into the transform itself.
pub(crate) fn transformed_points(
    component: &RuntimeComponentGeometry,
    transform: GeometryTransform,
) -> impl Iterator<Item = Vec2> + '_ {
    component
        .vertices
        .iter()
        .map(move |point| transform_point(transform, *point))
}

pub(crate) fn component_projection_minimum(
    component: &RuntimeComponentGeometry,
    transform: GeometryTransform,
    origin: Vec2,
    direction: Vec2,
) -> f32 {
    component
        .vertices
        .iter()
        .map(|point| (transform_point(transform, *point) - origin).dot(direction))
        .fold(f32::INFINITY, f32::min)
        .max(0.0)
}

/// How two shapes touch, rather than whether they touch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Contact {
    /// Unit vector pointing out of the second shape towards the first.
    pub normal: Vec2,
    /// How far they overlap along that normal; near zero when they only touch.
    pub depth: f32,
}

/// The shallowest axis that separates two overlapping triangles, which is the
/// surface one of them ran into.
///
/// Deliberately not the same code as [`triangles_overlap`]. That one answers a
/// combat question - did the Hammer reach this body - with an epsilon in the
/// units of an unnormalised edge normal, and it has been answering it correctly
/// for a while. This one measures a distance in meters, so its axes are
/// normalised and its epsilon means something else. Merging them would have to
/// change one of the two, and the one that must not change is the one already
/// deciding damage.
pub(crate) fn triangle_contact(first: [Vec2; 3], second: [Vec2; 3]) -> Option<Contact> {
    const EPSILON: f32 = 0.000_01;
    let mut best: Option<Contact> = None;
    for triangle in [first, second] {
        for index in 0..3 {
            let edge = triangle[(index + 1) % 3] - triangle[index];
            let axis = edge.perp();
            let length = axis.length();
            if length <= EPSILON {
                continue;
            }
            let axis = axis / length;
            let first_projection = first.map(|point| point.dot(axis));
            let second_projection = second.map(|point| point.dot(axis));
            let first_minimum = min_value(first_projection);
            let first_maximum = max_value(first_projection);
            let second_minimum = min_value(second_projection);
            let second_maximum = max_value(second_projection);
            let depth = first_maximum.min(second_maximum) - first_minimum.max(second_minimum);
            if depth < 0.0 {
                return None;
            }
            if best.is_none_or(|contact| depth < contact.depth) {
                let outwards = (first_minimum + first_maximum) >= (second_minimum + second_maximum);
                best = Some(Contact {
                    normal: if outwards { axis } else { -axis },
                    depth,
                });
            }
        }
    }
    best
}

pub(crate) fn triangles_overlap(first: [Vec2; 3], second: [Vec2; 3]) -> bool {
    const EPSILON: f32 = 0.000_01;
    for triangle in [first, second] {
        for index in 0..3 {
            let edge = triangle[(index + 1) % 3] - triangle[index];
            let axis = edge.perp();
            let first_projection = first.map(|point| point.dot(axis));
            let second_projection = second.map(|point| point.dot(axis));
            if max_value(second_projection) < min_value(first_projection) - EPSILON
                || max_value(first_projection) < min_value(second_projection) - EPSILON
            {
                return false;
            }
        }
    }
    true
}

fn min_value(values: [f32; 3]) -> f32 {
    values.into_iter().fold(f32::INFINITY, f32::min)
}

fn max_value(values: [f32; 3]) -> f32 {
    values.into_iter().fold(f32::NEG_INFINITY, f32::max)
}
