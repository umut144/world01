//! Narrowing which colliders are worth an exact test.
//!
//! Everything here is an accelerator and never an authority. A query returns a
//! superset of what the narrow phase would accept, and the narrow phase still
//! decides triangle by triangle. Losing a pair here would silently change
//! combat, so the superset property is asserted by a differential test rather
//! than merely documented.

use std::collections::HashMap;

use bevy::prelude::{Resource, Vec2};
use world01_content::WorldCollisionGeometryCatalog;

/// The edge length of one grid cell, in meters.
///
/// Large enough that a typical prop lands in one or two cells, small enough
/// that a character's query touches few of them.
const CELL_SIZE_METERS: f32 = 4.0;

/// An axis-aligned box. Used only to decide what to test exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub minimum: Vec2,
    pub maximum: Vec2,
}

impl Aabb {
    /// Returns `None` when the points are empty or carry a non-finite value,
    /// so that unusable geometry drops out of the broad phase instead of
    /// poisoning the grid with NaN.
    pub fn around(points: impl IntoIterator<Item = Vec2>) -> Option<Self> {
        let mut bounds: Option<Self> = None;
        for point in points {
            if !point.x.is_finite() || !point.y.is_finite() {
                return None;
            }
            bounds = Some(match bounds {
                Some(current) => Self {
                    minimum: current.minimum.min(point),
                    maximum: current.maximum.max(point),
                },
                None => Self {
                    minimum: point,
                    maximum: point,
                },
            });
        }
        bounds
    }

    pub fn expanded(self, margin: f32) -> Self {
        Self {
            minimum: self.minimum - Vec2::splat(margin),
            maximum: self.maximum + Vec2::splat(margin),
        }
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.minimum.x <= other.maximum.x
            && other.minimum.x <= self.maximum.x
            && self.minimum.y <= other.maximum.y
            && other.minimum.y <= self.maximum.y
    }
}

/// A uniform grid over the static world colliders, built once at startup.
///
/// Indices refer to `WorldCollisionGeometryCatalog::regions`, so the catalog
/// stays the single owner of the geometry.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldColliderGrid {
    cells: HashMap<(i32, i32), Vec<u32>>,
    /// One entry per catalog region, so an index into the catalog indexes
    /// here too. `None` marks geometry the broad phase cannot place.
    bounds: Vec<Option<Aabb>>,
}

impl WorldColliderGrid {
    pub fn from_catalog(catalog: &WorldCollisionGeometryCatalog) -> Self {
        let mut grid = Self::default();
        for (index, region) in catalog.regions.iter().enumerate() {
            let placement = Vec2::new(region.position.x, region.position.y);
            let bounds = Aabb::around(
                region
                    .component
                    .vertices
                    .iter()
                    .map(|vertex| placement + *vertex),
            );
            grid.bounds.push(bounds);
            let Some(bounds) = bounds else {
                continue;
            };
            let index = index as u32;
            for cell in cells_covering(bounds) {
                grid.cells.entry(cell).or_default().push(index);
            }
        }
        grid
    }

    /// Fills `out` with every collider index that might overlap `query`.
    ///
    /// The result is a superset: an index appears at most once, and no index
    /// whose collider truly overlaps is ever missing.
    pub fn candidates(&self, query: Aabb, out: &mut Vec<u32>) {
        out.clear();
        for cell in cells_covering(query) {
            let Some(indices) = self.cells.get(&cell) else {
                continue;
            };
            for index in indices {
                if self.bounds[*index as usize].is_some_and(|bounds| bounds.overlaps(query)) {
                    out.push(*index);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

fn cells_covering(bounds: Aabb) -> impl Iterator<Item = (i32, i32)> {
    let first = cell_of(bounds.minimum);
    let last = cell_of(bounds.maximum);
    (first.1..=last.1).flat_map(move |y| (first.0..=last.0).map(move |x| (x, y)))
}

fn cell_of(point: Vec2) -> (i32, i32) {
    (
        (point.x / CELL_SIZE_METERS).floor() as i32,
        (point.y / CELL_SIZE_METERS).floor() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_content::{PlacedCollisionGeometry, RuntimeComponentGeometry};
    use world01_world_data::Position;

    fn square(name: &str, x: f32, y: f32, size: f32) -> PlacedCollisionGeometry {
        PlacedCollisionGeometry {
            instance_id: name.to_owned(),
            position: Position::new(x, y),
            component: RuntimeComponentGeometry {
                component_id: name.to_owned(),
                name: name.to_owned(),
                vertices: vec![
                    Vec2::ZERO,
                    Vec2::new(size, 0.0),
                    Vec2::splat(size),
                    Vec2::new(0.0, size),
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
            },
        }
    }

    fn world_bounds(region: &PlacedCollisionGeometry) -> Option<Aabb> {
        let placement = Vec2::new(region.position.x, region.position.y);
        Aabb::around(
            region
                .component
                .vertices
                .iter()
                .map(|vertex| placement + *vertex),
        )
    }

    fn catalog() -> WorldCollisionGeometryCatalog {
        WorldCollisionGeometryCatalog {
            regions: vec![
                square("far", -20.0, -20.0, 2.0),
                square("small", 0.5, 0.5, 3.0),
                square("wide", 12.0, -7.0, 10.0),
                square("inside_wide", 12.5, -6.5, 1.0),
            ],
        }
    }

    /// The property the narrow phase depends on: the grid may hand over more
    /// than necessary, never less.
    #[test]
    fn the_grid_never_loses_a_collider_a_brute_force_scan_would_find() {
        let catalog = catalog();
        let grid = WorldColliderGrid::from_catalog(&catalog);
        let mut candidates = Vec::new();

        for x in -30..30 {
            for y in -30..30 {
                let query = Aabb {
                    minimum: Vec2::new(x as f32, y as f32),
                    maximum: Vec2::new(x as f32 + 1.5, y as f32 + 2.5),
                };
                grid.candidates(query, &mut candidates);

                for (index, region) in catalog.regions.iter().enumerate() {
                    let bounds = world_bounds(region).expect("test colliders are finite");
                    if bounds.overlaps(query) {
                        assert!(
                            candidates.contains(&(index as u32)),
                            "collider {index} overlaps the query but was not offered"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_collider_spanning_several_cells_is_offered_once() {
        let catalog = catalog();
        let grid = WorldColliderGrid::from_catalog(&catalog);
        let mut candidates = Vec::new();

        grid.candidates(
            Aabb {
                minimum: Vec2::new(-40.0, -40.0),
                maximum: Vec2::new(40.0, 40.0),
            },
            &mut candidates,
        );

        let mut unique = candidates.clone();
        unique.dedup();
        assert_eq!(candidates, unique, "candidates are free of duplicates");
        assert_eq!(candidates.len(), catalog.regions.len());
    }

    #[test]
    fn geometry_the_broad_phase_cannot_place_is_never_offered() {
        let mut catalog = catalog();
        let mut broken = square("broken", 0.0, 0.0, 1.0);
        broken.component.vertices[2] = Vec2::new(f32::NAN, 0.0);
        catalog.regions.push(broken);
        let broken_index = (catalog.regions.len() - 1) as u32;
        let grid = WorldColliderGrid::from_catalog(&catalog);
        let mut candidates = Vec::new();

        grid.candidates(
            Aabb {
                minimum: Vec2::new(-40.0, -40.0),
                maximum: Vec2::new(40.0, 40.0),
            },
            &mut candidates,
        );

        assert!(!candidates.contains(&broken_index));
    }
}
