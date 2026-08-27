use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Clone, Debug)]
pub struct EyeCollider {
    center: Vec2,
    boundary: Vec<Vec2>,
    radius: f32,
}

impl EyeCollider {
    pub fn from_outline(vertices: &[[f32; 2]], indices: &[u32], radius: f32) -> Option<Self> {
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        let boundary = boundary_loops(vertices, indices)
            .into_iter()
            .filter(|loop_vertices| loop_vertices.len() >= 3)
            .min_by(|left, right| {
                polygon_area(left)
                    .abs()
                    .total_cmp(&polygon_area(right).abs())
            })?;
        Self::from_boundary(boundary, radius)
    }

    pub fn center(&self) -> Vec2 {
        self.center
    }

    pub fn boundary(&self) -> &[Vec2] {
        &self.boundary
    }

    pub fn translated(&self, offset: Vec2) -> Self {
        Self {
            center: self.center + offset,
            boundary: self
                .boundary
                .iter()
                .map(|vertex| *vertex + offset)
                .collect(),
            radius: self.radius,
        }
    }

    pub fn position_for_local_gaze(&self, gaze: Vec2) -> Vec2 {
        let direction = gaze.normalize_or_zero();
        if direction == Vec2::ZERO {
            return self.center;
        }

        let mut minimum = 0.0;
        let mut maximum = self
            .boundary
            .iter()
            .map(|vertex| vertex.distance(self.center))
            .fold(0.0, f32::max)
            + self.radius;
        for _ in 0..20 {
            let distance = (minimum + maximum) * 0.5;
            if self.circle_fits(self.center + direction * distance) {
                minimum = distance;
            } else {
                maximum = distance;
            }
        }
        self.center + direction * minimum
    }

    fn from_boundary(boundary: Vec<Vec2>, radius: f32) -> Option<Self> {
        let centroid = polygon_center(&boundary);
        let center = if circle_fits(centroid, &boundary, radius) {
            centroid
        } else {
            largest_clearance_center(&boundary, radius)?
        };
        Some(Self {
            center,
            boundary,
            radius,
        })
    }

    fn circle_fits(&self, center: Vec2) -> bool {
        circle_fits(center, &self.boundary, self.radius)
    }
}

#[derive(Component, Debug, Clone)]
pub struct EyePupil {
    pub owner: Entity,
    collider: EyeCollider,
}

impl EyePupil {
    pub fn new(owner: Entity, collider: EyeCollider) -> Self {
        Self { owner, collider }
    }

    pub fn position_for_world_gaze(
        &self,
        world_gaze: Vec2,
        global_transform: &GlobalTransform,
    ) -> Vec2 {
        self.collider
            .position_for_local_gaze(gaze_in_local_space(world_gaze, global_transform))
    }
}

fn gaze_in_local_space(world_gaze: Vec2, global_transform: &GlobalTransform) -> Vec2 {
    if !world_gaze.is_finite() {
        return Vec2::ZERO;
    }
    global_transform
        .to_matrix()
        .inverse()
        .transform_vector3(world_gaze.extend(0.0))
        .truncate()
        .normalize_or_zero()
}

fn boundary_loops(vertices: &[[f32; 2]], indices: &[u32]) -> Vec<Vec<Vec2>> {
    let mut edges = HashMap::new();
    for triangle in indices.chunks_exact(3) {
        for (start, end) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = (start.min(end), start.max(end));
            if edges.remove(&key).is_none() {
                edges.insert(key, (start, end));
            }
        }
    }

    let mut outgoing = edges.into_values().collect::<HashMap<_, _>>();
    let mut loops = Vec::new();
    while let Some(start) = outgoing.keys().next().copied() {
        let mut vertex_ids = vec![start];
        let mut current = start;
        while let Some(next) = outgoing.remove(&current) {
            if next == start {
                break;
            }
            vertex_ids.push(next);
            current = next;
        }
        if vertex_ids.len() >= 3 {
            loops.push(
                vertex_ids
                    .into_iter()
                    .filter_map(|index| vertices.get(index as usize))
                    .map(|vertex| Vec2::from_array(*vertex))
                    .collect(),
            );
        }
    }
    loops
}

fn polygon_area(vertices: &[Vec2]) -> f32 {
    vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(left, right)| left.x * right.y - right.x * left.y)
        .sum::<f32>()
        * 0.5
}

fn polygon_center(vertices: &[Vec2]) -> Vec2 {
    let area = polygon_area(vertices);
    if area.abs() <= f32::EPSILON {
        return vertices.iter().copied().sum::<Vec2>() / vertices.len() as f32;
    }
    let weighted = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(left, right)| {
            let cross = left.x * right.y - right.x * left.y;
            (*left + *right) * cross
        })
        .sum::<Vec2>();
    weighted / (6.0 * area)
}

fn largest_clearance_center(boundary: &[Vec2], radius: f32) -> Option<Vec2> {
    const GRID_STEPS: u32 = 32;
    let minimum = boundary.iter().copied().reduce(Vec2::min)?;
    let maximum = boundary.iter().copied().reduce(Vec2::max)?;
    let extent = maximum - minimum;
    let mut best = None;
    let mut best_clearance = f32::NEG_INFINITY;

    for x in 0..=GRID_STEPS {
        for y in 0..=GRID_STEPS {
            let candidate = minimum
                + Vec2::new(
                    extent.x * x as f32 / GRID_STEPS as f32,
                    extent.y * y as f32 / GRID_STEPS as f32,
                );
            let clearance = clearance(candidate, boundary);
            if clearance > best_clearance {
                best = Some(candidate);
                best_clearance = clearance;
            }
        }
    }

    let mut center = best?;
    let mut step = extent.x.max(extent.y) * 0.25;
    for _ in 0..16 {
        let mut next = center;
        let mut next_clearance = clearance(center, boundary);
        for direction in [
            Vec2::ZERO,
            Vec2::X,
            -Vec2::X,
            Vec2::Y,
            -Vec2::Y,
            Vec2::ONE.normalize(),
            Vec2::new(1.0, -1.0).normalize(),
            Vec2::new(-1.0, 1.0).normalize(),
            -Vec2::ONE.normalize(),
        ] {
            let candidate = center + direction * step;
            let candidate_clearance = clearance(candidate, boundary);
            if candidate_clearance > next_clearance {
                next = candidate;
                next_clearance = candidate_clearance;
            }
        }
        if next == center {
            step *= 0.5;
        } else {
            center = next;
        }
    }

    (clearance(center, boundary) >= radius).then_some(center)
}

fn circle_fits(center: Vec2, boundary: &[Vec2], radius: f32) -> bool {
    clearance(center, boundary) >= radius
}

fn clearance(point: Vec2, boundary: &[Vec2]) -> f32 {
    point_is_inside_polygon(point, boundary)
        .then(|| {
            boundary
                .iter()
                .zip(boundary.iter().cycle().skip(1))
                .take(boundary.len())
                .map(|(start, end)| point_segment_distance(point, *start, *end))
                .fold(f32::INFINITY, f32::min)
        })
        .unwrap_or(f32::NEG_INFINITY)
}

fn point_is_inside_polygon(point: Vec2, boundary: &[Vec2]) -> bool {
    boundary
        .iter()
        .zip(boundary.iter().cycle().skip(1))
        .take(boundary.len())
        .fold(false, |inside, (start, end)| {
            let crosses = (start.y > point.y) != (end.y > point.y)
                && point.x < (end.x - start.x) * (point.y - start.y) / (end.y - start.y) + start.x;
            inside ^ crosses
        })
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let projection = (point - start).dot(segment) / length_squared;
    point.distance(start + segment * projection.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_reaches_but_does_not_cross_a_triangle_edge() {
        let collider = EyeCollider::from_boundary(
            vec![
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(0.0, 1.0),
            ],
            0.1,
        )
        .expect("triangle fits the pupil");
        let position = collider.position_for_local_gaze(Vec2::Y);

        assert!(collider.circle_fits(position));
        assert!(
            clearance(position, collider.boundary()) - 0.1 < 0.000_1,
            "pupil should touch the boundary"
        );
    }

    #[test]
    fn world_gaze_is_unmirrored_by_the_full_eye_transform() {
        let transform = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_z(
            std::f32::consts::PI,
        )));

        assert!(gaze_in_local_space(Vec2::X, &transform).distance(-Vec2::X) < 0.000_1);
    }
}
