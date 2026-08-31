use bevy::prelude::Vec2;
use world01_content::{CharacterHurtGeometry, RuntimeComponentGeometry};
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

pub(crate) fn hurt_transform(
    geometry: &CharacterHurtGeometry,
    position: Position,
    facing: BodyFacing,
) -> GeometryTransform {
    let mirrored = matches!(
        (geometry.authored_facing, facing),
        (world01_content::AuthoredFacing::Left, BodyFacing::Right)
            | (world01_content::AuthoredFacing::Right, BodyFacing::Left)
    );
    GeometryTransform {
        origin: Vec2::new(position.x, position.y),
        axis_x: if mirrored { -Vec2::X } else { Vec2::X },
        axis_y: Vec2::Y,
    }
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
