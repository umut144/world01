use std::cmp::Ordering;

use bevy::prelude::Vec2;
use world01_content::{
    AuthoredFacing, CharacterHurtGeometry, CollisionComponentGeometry, RuntimeComponentGeometry,
};
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

/// The exact translation that brings the first Component to touching the
/// second along one authored boundary axis.
///
/// This is deliberately independent from triangle winding and from the
/// triangle-pair contact heuristic used by current movement blocking. It first
/// asks the existing narrow phase whether the Components overlap, then finds
/// the smallest translation that makes their complete projections touch on a
/// boundary axis from either Component. For a non-convex Component that is a
/// conservative convex-hull separation and may move farther than the locally
/// shortest way out. Clearance beyond exact touching belongs to the system
/// that eventually applies this result, not to this geometry calculation.
/// Equal escape distances are oriented from the posed point sets rather than
/// triangle winding. Two geometrically indistinguishable Components at the
/// same pose cannot supply an antisymmetric answer, so they use the canonical
/// positive axis deterministically.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ComponentSeparation {
    /// Unit direction in which the first Component moves.
    pub normal: Vec2,
    /// Distance to exact projected contact, before any application clearance.
    pub separation_distance: f32,
}

#[derive(Debug, Clone, Copy)]
struct SeparationCandidate {
    separation: ComponentSeparation,
    axis: Vec2,
}

pub(crate) fn component_separation(
    first: &CollisionComponentGeometry,
    first_transform: GeometryTransform,
    second: &CollisionComponentGeometry,
    second_transform: GeometryTransform,
) -> Option<ComponentSeparation> {
    if !components_overlap(
        first.geometry(),
        first_transform,
        second.geometry(),
        second_transform,
    ) {
        return None;
    }

    let first_points = transformed_points(first.geometry(), first_transform).collect::<Vec<_>>();
    let second_points = transformed_points(second.geometry(), second_transform).collect::<Vec<_>>();
    let shape_order = compare_point_sets(&first_points, &second_points);
    let projection_origin = first_points[0];
    let mut best: Option<SeparationCandidate> = None;

    for (edge, transform) in first
        .boundary_edges()
        .iter()
        .map(|edge| (*edge, first_transform))
        .chain(
            second
                .boundary_edges()
                .iter()
                .map(|edge| (*edge, second_transform)),
        )
    {
        let start = transform_point(transform, edge[0]);
        let end = transform_point(transform, edge[1]);
        let axis = canonical_axis((end - start).perp())?;
        let first_projection = projection_bounds(&first_points, projection_origin, axis);
        let second_projection = projection_bounds(&second_points, projection_origin, axis);
        let candidate = separation_on_axis(first_projection, second_projection, axis, shape_order);
        if best.is_none_or(|current| compare_candidates(candidate, current).is_lt()) {
            best = Some(candidate);
        }
    }

    best.map(|candidate| candidate.separation)
}

fn canonical_axis(axis: Vec2) -> Option<Vec2> {
    let length = axis.length();
    if !length.is_finite() || length == 0.0 {
        return None;
    }
    let mut axis = axis / length;
    if axis.x < 0.0 || (axis.x == 0.0 && axis.y < 0.0) {
        axis = -axis;
    }
    if axis.x == 0.0 {
        axis.x = 0.0;
    }
    if axis.y == 0.0 {
        axis.y = 0.0;
    }
    Some(axis)
}

fn projection_bounds(points: &[Vec2], origin: Vec2, axis: Vec2) -> (f32, f32) {
    points
        .iter()
        .map(|point| (*point - origin).dot(axis))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |bounds, value| {
            (bounds.0.min(value), bounds.1.max(value))
        })
}

fn separation_on_axis(
    first: (f32, f32),
    second: (f32, f32),
    axis: Vec2,
    shape_order: Ordering,
) -> SeparationCandidate {
    let (negative_distance, positive_distance) = axis_translation_distances(first, second);
    let (normal, separation_distance) = match negative_distance.total_cmp(&positive_distance) {
        Ordering::Less => (-axis, negative_distance),
        Ordering::Greater => (axis, positive_distance),
        Ordering::Equal => match shape_order {
            Ordering::Less => (-axis, negative_distance),
            Ordering::Greater | Ordering::Equal => (axis, positive_distance),
        },
    };
    SeparationCandidate {
        separation: ComponentSeparation {
            normal,
            separation_distance,
        },
        axis,
    }
}

fn axis_translation_distances(first: (f32, f32), second: (f32, f32)) -> (f32, f32) {
    ((first.1 - second.0).max(0.0), (second.1 - first.0).max(0.0))
}

fn compare_candidates(first: SeparationCandidate, second: SeparationCandidate) -> Ordering {
    first
        .separation
        .separation_distance
        .total_cmp(&second.separation.separation_distance)
        .then_with(|| compare_vec2(first.axis, second.axis))
        .then_with(|| compare_vec2(first.separation.normal, second.separation.normal))
}

fn compare_point_sets(first: &[Vec2], second: &[Vec2]) -> Ordering {
    let mut first = first.to_vec();
    let mut second = second.to_vec();
    first.sort_by(|left, right| compare_vec2(*left, *right));
    second.sort_by(|left, right| compare_vec2(*left, *right));
    first
        .iter()
        .zip(&second)
        .map(|(left, right)| compare_vec2(*left, *right))
        .find(|order| !order.is_eq())
        .unwrap_or_else(|| first.len().cmp(&second.len()))
}

fn compare_vec2(first: Vec2, second: Vec2) -> Ordering {
    first
        .x
        .total_cmp(&second.x)
        .then_with(|| first.y.total_cmp(&second.y))
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

#[cfg(test)]
mod tests {
    use super::*;
    use world01_content::RuntimeComponentGeometry;

    const ASSERT_EPSILON: f32 = 0.000_01;
    const TEST_CLEARANCE: f32 = 0.001;

    fn collision_geometry(
        name: &str,
        vertices: Vec<Vec2>,
        indices: Vec<u32>,
    ) -> CollisionComponentGeometry {
        CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: name.into(),
            name: name.into(),
            vertices,
            indices,
        })
        .expect("test collision geometry is valid")
    }

    fn rectangle(name: &str, minimum: Vec2, maximum: Vec2) -> CollisionComponentGeometry {
        collision_geometry(
            name,
            vec![
                minimum,
                Vec2::new(maximum.x, minimum.y),
                maximum,
                Vec2::new(minimum.x, maximum.y),
            ],
            vec![0, 1, 2, 0, 2, 3],
        )
    }

    fn l_shape(name: &str) -> CollisionComponentGeometry {
        collision_geometry(
            name,
            vec![
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 1.0),
                Vec2::ONE,
                Vec2::new(1.0, 2.0),
                Vec2::new(0.0, 2.0),
            ],
            vec![0, 1, 3, 1, 2, 3, 0, 3, 5, 3, 4, 5],
        )
    }

    fn translated(x: f32, y: f32) -> GeometryTransform {
        GeometryTransform {
            origin: Vec2::new(x, y),
            ..GeometryTransform::IDENTITY
        }
    }

    fn moved(transform: GeometryTransform, offset: Vec2) -> GeometryTransform {
        GeometryTransform {
            origin: transform.origin + offset,
            ..transform
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= ASSERT_EPSILON,
            "{actual} != {expected}"
        );
    }

    fn assert_vec2(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    fn assert_separates(
        first: &CollisionComponentGeometry,
        first_transform: GeometryTransform,
        second: &CollisionComponentGeometry,
        second_transform: GeometryTransform,
        separation: ComponentSeparation,
    ) {
        let first_transform = moved(
            first_transform,
            separation.normal * (separation.separation_distance + TEST_CLEARANCE),
        );
        assert!(!components_overlap(
            first.geometry(),
            first_transform,
            second.geometry(),
            second_transform,
        ));
    }

    #[test]
    fn shallow_and_deep_frontal_contacts_choose_the_surface_normal() {
        let actor = rectangle("actor", Vec2::ZERO, Vec2::ONE);
        let blocker = rectangle("blocker", Vec2::ZERO, Vec2::ONE);

        for (blocker_x, expected_distance) in [(0.9, 0.1), (0.2, 0.8)] {
            let separation = component_separation(
                &actor,
                GeometryTransform::IDENTITY,
                &blocker,
                translated(blocker_x, 0.0),
            )
            .expect("the rectangles overlap");

            assert_vec2(separation.normal, Vec2::NEG_X);
            assert_close(separation.separation_distance, expected_distance);
            assert_separates(
                &actor,
                GeometryTransform::IDENTITY,
                &blocker,
                translated(blocker_x, 0.0),
                separation,
            );
        }
    }

    #[test]
    fn containment_is_finite_and_antisymmetric() {
        let inner = rectangle("inner", Vec2::splat(-0.2), Vec2::splat(0.2));
        let outer = rectangle("outer", Vec2::splat(-1.0), Vec2::ONE);
        let inner_from_outer = component_separation(
            &inner,
            GeometryTransform::IDENTITY,
            &outer,
            GeometryTransform::IDENTITY,
        )
        .expect("the inner rectangle is contained");
        let outer_from_inner = component_separation(
            &outer,
            GeometryTransform::IDENTITY,
            &inner,
            GeometryTransform::IDENTITY,
        )
        .expect("containment is symmetric as an overlap");

        assert_close(inner_from_outer.separation_distance, 1.2);
        assert_close(
            outer_from_inner.separation_distance,
            inner_from_outer.separation_distance,
        );
        assert_vec2(outer_from_inner.normal, -inner_from_outer.normal);
        assert_separates(
            &inner,
            GeometryTransform::IDENTITY,
            &outer,
            GeometryTransform::IDENTITY,
            inner_from_outer,
        );
        assert_separates(
            &outer,
            GeometryTransform::IDENTITY,
            &inner,
            GeometryTransform::IDENTITY,
            outer_from_inner,
        );
    }

    #[test]
    fn identical_coincident_components_choose_one_documented_direction() {
        let first = rectangle("first", Vec2::ZERO, Vec2::ONE);
        let second = rectangle("second", Vec2::ZERO, Vec2::ONE);
        let separation = component_separation(
            &first,
            GeometryTransform::IDENTITY,
            &second,
            GeometryTransform::IDENTITY,
        )
        .expect("identical Components overlap");
        let repeated = component_separation(
            &first,
            GeometryTransform::IDENTITY,
            &second,
            GeometryTransform::IDENTITY,
        )
        .expect("the same query remains valid");

        assert_eq!(separation, repeated);
        assert!(separation.normal.is_finite());
        assert_close(separation.separation_distance, 1.0);
        assert_separates(
            &first,
            GeometryTransform::IDENTITY,
            &second,
            GeometryTransform::IDENTITY,
            separation,
        );
    }

    #[test]
    fn exact_touch_has_zero_separation_and_disjoint_components_have_none() {
        let first = rectangle("first", Vec2::ZERO, Vec2::ONE);
        let second = rectangle("second", Vec2::ZERO, Vec2::ONE);
        let touching = component_separation(
            &first,
            GeometryTransform::IDENTITY,
            &second,
            translated(1.0, 0.0),
        )
        .expect("the existing narrow phase includes exact contact");

        assert_close(touching.separation_distance, 0.0);
        assert!(touching.normal.is_finite());
        assert!(
            component_separation(
                &first,
                GeometryTransform::IDENTITY,
                &second,
                translated(2.0, 0.0),
            )
            .is_none()
        );
    }

    #[test]
    fn argument_order_reverses_a_non_tied_contact() {
        let first = rectangle("first", Vec2::ZERO, Vec2::ONE);
        let second = rectangle("second", Vec2::ZERO, Vec2::ONE);
        let first_from_second = component_separation(
            &first,
            GeometryTransform::IDENTITY,
            &second,
            translated(0.9, 0.0),
        )
        .expect("the rectangles overlap");
        let second_from_first = component_separation(
            &second,
            translated(0.9, 0.0),
            &first,
            GeometryTransform::IDENTITY,
        )
        .expect("argument order does not change overlap");

        assert_close(
            first_from_second.separation_distance,
            second_from_first.separation_distance,
        );
        assert_vec2(first_from_second.normal, -second_from_first.normal);
    }

    #[test]
    fn translating_both_components_does_not_change_the_result() {
        let first = rectangle("first", Vec2::ZERO, Vec2::ONE);
        let second = rectangle("second", Vec2::ZERO, Vec2::ONE);
        let baseline = component_separation(
            &first,
            GeometryTransform::IDENTITY,
            &second,
            translated(0.9, 0.0),
        )
        .expect("the rectangles overlap");
        let shifted = component_separation(
            &first,
            translated(3.0, -2.0),
            &second,
            translated(3.9, -2.0),
        )
        .expect("translation preserves overlap");

        assert_vec2(shifted.normal, baseline.normal);
        assert_close(shifted.separation_distance, baseline.separation_distance);
    }

    #[test]
    fn a_mirrored_deep_overlap_keeps_the_same_surface_normal() {
        let first = rectangle("first", Vec2::ZERO, Vec2::ONE);
        let second = rectangle("second", Vec2::ZERO, Vec2::ONE);
        let mirrored = GeometryTransform {
            origin: Vec2::X,
            axis_x: Vec2::NEG_X,
            axis_y: Vec2::Y,
        };
        let separation = component_separation(&first, mirrored, &second, translated(0.2, 0.0))
            .expect("the mirrored rectangle overlaps deeply");

        assert_vec2(separation.normal, Vec2::NEG_X);
        assert_close(separation.separation_distance, 0.8);
        assert_separates(&first, mirrored, &second, translated(0.2, 0.0), separation);
    }

    #[test]
    fn a_non_convex_component_gets_a_conservative_separating_translation() {
        let actor = rectangle("actor", Vec2::splat(-0.2), Vec2::splat(0.2));
        let blocker = l_shape("blocker");
        let actor_transform = translated(1.0, 1.0);
        let separation = component_separation(
            &actor,
            actor_transform,
            &blocker,
            GeometryTransform::IDENTITY,
        )
        .expect("the actor overlaps the inner corner of the L");

        assert!(separation.normal.is_finite());
        assert!(separation.separation_distance > 0.0);
        assert_separates(
            &actor,
            actor_transform,
            &blocker,
            GeometryTransform::IDENTITY,
            separation,
        );
    }

    #[test]
    fn a_long_thin_wall_uses_its_shallow_face() {
        let actor = rectangle("actor", Vec2::ZERO, Vec2::splat(0.4));
        let wall = rectangle("wall", Vec2::new(0.0, -2.0), Vec2::new(0.1, 2.0));
        let separation = component_separation(
            &actor,
            GeometryTransform::IDENTITY,
            &wall,
            translated(0.35, 0.0),
        )
        .expect("the actor overlaps the wall by five centimeters");

        assert_vec2(separation.normal, Vec2::NEG_X);
        assert_close(separation.separation_distance, 0.05);
    }

    #[test]
    fn axis_translation_distances_sum_to_both_interval_widths() {
        let first = (-0.5, 1.5);
        let second = (1.0, 4.0);
        let (negative_distance, positive_distance) = axis_translation_distances(first, second);

        assert_close(
            negative_distance + positive_distance,
            (first.1 - first.0) + (second.1 - second.0),
        );
    }
}
