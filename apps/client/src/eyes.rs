use bevy::prelude::*;
use std::collections::HashMap;

const PUPIL_SEGMENTS: u32 = 24;

#[derive(Clone, Debug)]
pub struct EyeCollider {
    center: Vec2,
    boundary: Vec<Vec2>,
    triangles: Vec<[Vec2; 3]>,
    visible_clip_segments: Option<Vec<[Vec2; 2]>>,
    radius: f32,
    collision_radius: f32,
}

#[derive(Clone, Debug)]
pub struct PupilGeometry {
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl EyeCollider {
    #[cfg(test)]
    pub fn from_outline(vertices: &[[f32; 2]], indices: &[u32], radius: f32) -> Option<Self> {
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        Self::from_boundary(outline_boundary(vertices, indices)?, radius, radius)
    }

    pub fn from_region_mesh(
        vertices: &[[f32; 2]],
        indices: &[u32],
        pupil_area_ratio: f32,
        pupil_collision_reference_radius: f32,
    ) -> Option<Self> {
        if !valid_pupil_parameters(pupil_area_ratio, pupil_collision_reference_radius) {
            return None;
        }
        let vertices = vertices
            .iter()
            .copied()
            .map(Vec2::from_array)
            .collect::<Vec<_>>();
        let triangles = region_triangles(&vertices, indices)?;
        let area = triangles
            .iter()
            .map(|triangle| triangle_area(*triangle).abs())
            .sum::<f32>();
        if !area.is_finite() || area <= f32::EPSILON {
            return None;
        }
        let radius = pupil_radius_from_area(area, pupil_area_ratio)?;
        Self::from_geometry(
            region_boundary(&vertices, indices)?,
            triangles,
            radius,
            pupil_collision_reference_radius.min(radius),
        )
    }

    pub fn pupil_radius_from_region_mesh(
        vertices: &[[f32; 2]],
        indices: &[u32],
        pupil_area_ratio: f32,
    ) -> Option<f32> {
        if !pupil_area_ratio.is_finite() || pupil_area_ratio <= 0.0 || pupil_area_ratio >= 1.0 {
            return None;
        }
        let vertices = vertices
            .iter()
            .copied()
            .map(Vec2::from_array)
            .collect::<Vec<_>>();
        let area = region_triangles(&vertices, indices)?
            .iter()
            .map(|triangle| triangle_area(*triangle).abs())
            .sum::<f32>();
        pupil_radius_from_area(area, pupil_area_ratio)
    }

    pub fn from_outline_area_ratio(
        vertices: &[[f32; 2]],
        indices: &[u32],
        pupil_area_ratio: f32,
        pupil_collision_reference_radius: f32,
    ) -> Option<Self> {
        if !valid_pupil_parameters(pupil_area_ratio, pupil_collision_reference_radius) {
            return None;
        }
        let boundary = outline_boundary(vertices, indices)?;
        let radius = pupil_radius_from_area(polygon_area(&boundary).abs(), pupil_area_ratio)?;
        Self::from_boundary(
            boundary,
            radius,
            pupil_collision_reference_radius.min(radius),
        )
    }

    pub fn center(&self) -> Vec2 {
        self.center
    }

    pub fn with_visible_outline(
        mut self,
        vertices: &[[f32; 2]],
        indices: &[u32],
        outline_is_closed: bool,
    ) -> Self {
        if !outline_is_closed {
            self.visible_clip_segments = Some(stroke_centerline_segments(vertices, indices));
        }
        self
    }

    #[cfg(test)]
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
            triangles: self
                .triangles
                .iter()
                .map(|triangle| triangle.map(|vertex| vertex + offset))
                .collect(),
            visible_clip_segments: self.visible_clip_segments.as_ref().map(|segments| {
                segments
                    .iter()
                    .map(|segment| segment.map(|vertex| vertex + offset))
                    .collect()
            }),
            radius: self.radius,
            collision_radius: self.collision_radius,
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
            + self.collision_radius;
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

    pub fn clipped_pupil_geometry(&self, center: Vec2) -> PupilGeometry {
        let circle = (0..PUPIL_SEGMENTS)
            .map(|segment| {
                let angle = std::f32::consts::TAU * segment as f32 / PUPIL_SEGMENTS as f32;
                center + Vec2::new(angle.cos(), angle.sin()) * self.radius
            })
            .collect::<Vec<_>>();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        if let Some(segments) = self.visible_clip_segments.as_ref() {
            let clipped = segments.iter().fold(circle, |polygon, segment| {
                clip_polygon_to_half_plane(&polygon, segment[0], segment[1], self.center)
            });
            append_polygon_geometry(&clipped, center, &mut vertices, &mut indices);
            return PupilGeometry { vertices, indices };
        }

        for triangle in &self.triangles {
            let clipped = clip_polygon_to_triangle(&circle, *triangle);
            append_polygon_geometry(&clipped, center, &mut vertices, &mut indices);
        }

        PupilGeometry { vertices, indices }
    }

    fn from_boundary(boundary: Vec<Vec2>, radius: f32, collision_radius: f32) -> Option<Self> {
        let center = polygon_center(&boundary);
        let triangles = boundary
            .iter()
            .copied()
            .zip(boundary.iter().copied().cycle().skip(1))
            .take(boundary.len())
            .map(|(left, right)| [center, left, right])
            .collect();
        Self::from_geometry(boundary, triangles, radius, collision_radius)
    }

    fn from_geometry(
        boundary: Vec<Vec2>,
        triangles: Vec<[Vec2; 3]>,
        radius: f32,
        collision_radius: f32,
    ) -> Option<Self> {
        let centroid = polygon_center(&boundary);
        let center = if circle_fits(centroid, &boundary, collision_radius) {
            centroid
        } else {
            largest_clearance_center(&boundary, collision_radius)?
        };
        Some(Self {
            center,
            boundary,
            triangles,
            visible_clip_segments: None,
            radius,
            collision_radius,
        })
    }

    fn circle_fits(&self, center: Vec2) -> bool {
        circle_fits(center, &self.boundary, self.collision_radius)
    }
}

fn valid_pupil_parameters(pupil_area_ratio: f32, pupil_collision_reference_radius: f32) -> bool {
    pupil_area_ratio.is_finite()
        && pupil_area_ratio > 0.0
        && pupil_area_ratio < 1.0
        && pupil_collision_reference_radius.is_finite()
        && pupil_collision_reference_radius > 0.0
}

fn pupil_radius_from_area(area: f32, pupil_area_ratio: f32) -> Option<f32> {
    if !area.is_finite() || area <= f32::EPSILON {
        return None;
    }
    Some((area * pupil_area_ratio / std::f32::consts::PI).sqrt())
}

fn region_triangles(vertices: &[Vec2], indices: &[u32]) -> Option<Vec<[Vec2; 3]>> {
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return None;
    }
    indices
        .chunks_exact(3)
        .map(|triangle| {
            let triangle = [
                *vertices.get(triangle[0] as usize)?,
                *vertices.get(triangle[1] as usize)?,
                *vertices.get(triangle[2] as usize)?,
            ];
            (triangle_area(triangle).abs() > f32::EPSILON).then_some(triangle)
        })
        .collect()
}

fn region_boundary(vertices: &[Vec2], indices: &[u32]) -> Option<Vec<Vec2>> {
    let mut edge_counts = HashMap::<(u32, u32), u32>::new();
    for triangle in indices.chunks_exact(3) {
        for (left, right) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let edge = if left < right {
                (left, right)
            } else {
                (right, left)
            };
            *edge_counts.entry(edge).or_default() += 1;
        }
    }
    let boundary_edges = edge_counts
        .into_iter()
        .filter_map(|(edge, count)| (count == 1).then_some(edge))
        .collect::<Vec<_>>();
    let mut neighbors = HashMap::<u32, Vec<u32>>::new();
    for (left, right) in &boundary_edges {
        neighbors.entry(*left).or_default().push(*right);
        neighbors.entry(*right).or_default().push(*left);
    }
    if neighbors.len() < 3 || neighbors.values().any(|adjacent| adjacent.len() != 2) {
        return None;
    }

    let start = *neighbors.keys().min()?;
    let mut ordered = vec![start];
    let mut previous = None;
    let mut current = start;
    loop {
        let adjacent = neighbors.get(&current)?;
        let next = adjacent
            .iter()
            .copied()
            .find(|candidate| Some(*candidate) != previous)?;
        if next == start {
            break;
        }
        if ordered.contains(&next) {
            return None;
        }
        ordered.push(next);
        previous = Some(current);
        current = next;
    }
    if ordered.len() != neighbors.len() {
        return None;
    }
    ordered
        .into_iter()
        .map(|index| vertices.get(index as usize).copied())
        .collect()
}

fn triangle_area(triangle: [Vec2; 3]) -> f32 {
    (triangle[1] - triangle[0]).perp_dot(triangle[2] - triangle[0]) * 0.5
}

fn stroke_centerline_segments(vertices: &[[f32; 2]], indices: &[u32]) -> Vec<[Vec2; 2]> {
    indices
        .chunks_exact(6)
        .filter_map(|quad| {
            if quad[2] != quad[3] || quad[1] != quad[4] {
                return None;
            }
            let start_outer = Vec2::from_array(*vertices.get(quad[0] as usize)?);
            let start_inner = Vec2::from_array(*vertices.get(quad[1] as usize)?);
            let end_outer = Vec2::from_array(*vertices.get(quad[2] as usize)?);
            let end_inner = Vec2::from_array(*vertices.get(quad[5] as usize)?);
            Some([
                (start_outer + start_inner) * 0.5,
                (end_outer + end_inner) * 0.5,
            ])
        })
        .collect()
}

fn append_polygon_geometry(
    polygon: &[Vec2],
    center: Vec2,
    vertices: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    if polygon.len() < 3 {
        return;
    }
    let first = vertices.len() as u32;
    vertices.extend(polygon.iter().map(|vertex| (*vertex - center).to_array()));
    for index in 1..polygon.len() as u32 - 1 {
        indices.extend_from_slice(&[first, first + index, first + index + 1]);
    }
}

fn clip_polygon_to_half_plane(
    subject: &[Vec2],
    clip_start: Vec2,
    clip_end: Vec2,
    inside_reference: Vec2,
) -> Vec<Vec2> {
    let reference_distance = (clip_end - clip_start).perp_dot(inside_reference - clip_start);
    if reference_distance.abs() <= f32::EPSILON {
        return subject.to_vec();
    }
    let orientation = reference_distance.signum();
    let mut output = Vec::new();
    let Some(mut previous) = subject.last().copied() else {
        return output;
    };
    let mut previous_distance =
        orientation * (clip_end - clip_start).perp_dot(previous - clip_start);
    for current in subject.iter().copied() {
        let current_distance = orientation * (clip_end - clip_start).perp_dot(current - clip_start);
        let previous_inside = previous_distance >= -f32::EPSILON;
        let current_inside = current_distance >= -f32::EPSILON;
        if previous_inside != current_inside {
            let denominator = previous_distance - current_distance;
            if denominator.abs() > f32::EPSILON {
                output.push(previous + (current - previous) * (previous_distance / denominator));
            }
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_distance = current_distance;
    }
    output
}

fn clip_polygon_to_triangle(subject: &[Vec2], triangle: [Vec2; 3]) -> Vec<Vec2> {
    let mut output = subject.to_vec();
    let inside_reference = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
    for (clip_start, clip_end) in triangle
        .iter()
        .copied()
        .zip(triangle.iter().copied().cycle().skip(1))
        .take(3)
    {
        output = clip_polygon_to_half_plane(&output, clip_start, clip_end, inside_reference);
    }
    output
}

fn outline_boundary(vertices: &[[f32; 2]], indices: &[u32]) -> Option<Vec<Vec2>> {
    centerline_loops(vertices, indices)
        .into_iter()
        .filter(|loop_vertices| loop_vertices.len() >= 3)
        .max_by(|left, right| {
            polygon_area(left)
                .abs()
                .total_cmp(&polygon_area(right).abs())
        })
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

    pub fn clipped_geometry(&self, position: Vec2) -> PupilGeometry {
        self.collider.clipped_pupil_geometry(position)
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

fn centerline_loops(vertices: &[[f32; 2]], indices: &[u32]) -> Vec<Vec<Vec2>> {
    const JOIN_EPSILON_SQUARED: f32 = 0.000_000_01;
    let mut loops = Vec::new();
    let mut current = Vec::new();

    for quad in indices.chunks_exact(6) {
        if quad[2] != quad[3] || quad[1] != quad[4] {
            continue;
        }
        let Some(start_outer) = vertices
            .get(quad[0] as usize)
            .copied()
            .map(Vec2::from_array)
        else {
            continue;
        };
        let Some(start_inner) = vertices
            .get(quad[1] as usize)
            .copied()
            .map(Vec2::from_array)
        else {
            continue;
        };
        let Some(end_outer) = vertices
            .get(quad[2] as usize)
            .copied()
            .map(Vec2::from_array)
        else {
            continue;
        };
        let Some(end_inner) = vertices
            .get(quad[5] as usize)
            .copied()
            .map(Vec2::from_array)
        else {
            continue;
        };
        let start = (start_outer + start_inner) * 0.5;
        let end = (end_outer + end_inner) * 0.5;

        if current
            .last()
            .is_some_and(|previous: &Vec2| previous.distance_squared(start) > JOIN_EPSILON_SQUARED)
        {
            finish_centerline_loop(&mut loops, &mut current, JOIN_EPSILON_SQUARED);
        }
        if current.is_empty() {
            current.push(start);
        }
        current.push(end);
    }
    finish_centerline_loop(&mut loops, &mut current, JOIN_EPSILON_SQUARED);
    loops
}

fn finish_centerline_loop(
    loops: &mut Vec<Vec<Vec2>>,
    current: &mut Vec<Vec2>,
    epsilon_squared: f32,
) {
    if current.len() >= 2
        && current[0].distance_squared(*current.last().expect("length checked")) <= epsilon_squared
    {
        current.pop();
    }
    if current.len() >= 3 {
        loops.push(std::mem::take(current));
    } else {
        current.clear();
    }
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
    fn disconnected_stroke_quads_reconstruct_the_authored_boundary() {
        let vertices = [
            [-1.0, -1.1],
            [-1.0, -0.9],
            [1.0, -1.1],
            [1.0, -0.9],
            [0.9, -1.0],
            [1.1, -1.0],
            [0.9, 1.0],
            [1.1, 1.0],
            [1.0, 0.9],
            [1.0, 1.1],
            [-1.0, 0.9],
            [-1.0, 1.1],
            [-0.9, 1.0],
            [-1.1, 1.0],
            [-0.9, -1.0],
            [-1.1, -1.0],
        ];
        let indices = [
            0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7, 8, 9, 10, 10, 9, 11, 12, 13, 14, 14, 13, 15,
        ];

        let collider = EyeCollider::from_outline(&vertices, &indices, 0.1)
            .expect("disconnected quads form a valid square eye");

        assert_eq!(collider.boundary().len(), 4);
        assert!(collider.circle_fits(Vec2::ZERO));
    }

    #[test]
    fn world_gaze_is_unmirrored_by_the_full_eye_transform() {
        let transform = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_z(
            std::f32::consts::PI,
        )));

        assert!(gaze_in_local_space(Vec2::X, &transform).distance(-Vec2::X) < 0.000_1);
    }

    #[test]
    fn region_mesh_clips_pupil_vertices_to_its_boundary() {
        let vertices = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let collider = EyeCollider::from_region_mesh(&vertices, &[0, 1, 2, 0, 2, 3], 0.26, 0.35)
            .expect("square region fits a pupil");
        let position = collider.position_for_local_gaze(Vec2::X);
        let geometry = collider.clipped_pupil_geometry(position);

        assert!(!geometry.indices.is_empty());
        assert!(geometry.vertices.iter().all(|vertex| {
            let point = Vec2::from_array(*vertex) + position;
            point_is_inside_polygon(point, collider.boundary())
                || collider
                    .boundary()
                    .iter()
                    .copied()
                    .zip(collider.boundary().iter().copied().cycle().skip(1))
                    .take(collider.boundary().len())
                    .any(|(start, end)| point_segment_distance(point, start, end) < 0.000_1)
        }));
    }

    #[test]
    fn invisible_edge_collides_without_clipping_the_pupil() {
        let vertices = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let mut collider =
            EyeCollider::from_region_mesh(&vertices, &[0, 1, 2, 0, 2, 3], 0.26, 0.35)
                .expect("square region fits a pupil");
        collider.visible_clip_segments = Some(vec![
            [Vec2::new(-1.0, 1.0), Vec2::new(1.0, 1.0)],
            [Vec2::new(1.0, 1.0), Vec2::new(1.0, -1.0)],
            [Vec2::new(-1.0, -1.0), Vec2::new(-1.0, 1.0)],
        ]);
        let position = collider.position_for_local_gaze(-Vec2::Y);
        let geometry = collider.clipped_pupil_geometry(position);

        assert!(collider.circle_fits(position));
        assert!(
            geometry
                .vertices
                .iter()
                .any(|vertex| Vec2::from_array(*vertex).y + position.y < -1.0)
        );
    }
}
