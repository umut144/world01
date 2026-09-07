//! One definition of where an Actor is supported and where it may step next.
//!
//! The movement constraint, the Path speed rule, and the server's recovery rule
//! all ask that same question, so they ask it here instead of each carrying its
//! own copy of the rule.

use world01_world_data::{
    GroundSupport, MapColumnSurface, MapRouteSurface, Position, WorldMap, WorldPosition,
};

use super::{CharacterTraversalProfile, TraversalCatalog};

pub(crate) fn resolve_terrain_position(
    map: &WorldMap,
    traversal: &TraversalCatalog,
    character: &world01_world_data::CharacterId,
    candidate: WorldPosition,
) -> Option<WorldPosition> {
    let profile = traversal.character(character)?;
    let horizontal = candidate.horizontal();
    map.terrain_cell_at(horizontal)?;
    let mut surfaces = Vec::new();
    map.terrain_walking_surfaces(horizontal, &mut surfaces);
    let standing_on = nearest_surface(&surfaces, candidate.elevation_meters)?;
    if !profile.permits_surface(standing_on.surface) {
        return None;
    }
    Some(WorldPosition::new(
        candidate.x,
        candidate.y,
        standing_on.elevation_meters,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SampleSupport<'a> {
    Terrain,
    RouteSurface(&'a str),
}

impl SampleSupport<'_> {
    pub(super) fn to_owned(self) -> GroundSupport {
        match self {
            Self::Terrain => GroundSupport::Terrain,
            Self::RouteSurface(route_surface_id) => GroundSupport::RouteSurface {
                route_surface_id: route_surface_id.to_owned(),
            },
        }
    }
}

impl<'a> From<&'a GroundSupport> for SampleSupport<'a> {
    fn from(support: &'a GroundSupport) -> Self {
        match support {
            GroundSupport::Terrain => Self::Terrain,
            GroundSupport::RouteSurface { route_surface_id } => {
                Self::RouteSurface(route_surface_id)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GroundSample<'a> {
    pub(super) support: SampleSupport<'a>,
    pub(super) surface: &'a str,
    pub(super) elevation_meters: f32,
    pub(super) grade_percent: Option<i32>,
}

pub(super) enum CurrentSupport<'a> {
    Resolved(GroundSample<'a>),
    MissingIdentity,
    Unsupported,
}

pub(super) fn resolve_current_support<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    support: &GroundSupport,
    position: WorldPosition,
) -> CurrentSupport<'a> {
    let horizontal = position.horizontal();
    let (sample, excluded_route) = match support {
        GroundSupport::Terrain => (
            sample_terrain(map, horizontal, position.elevation_meters),
            None,
        ),
        GroundSupport::RouteSurface { route_surface_id } => {
            let Some(route) = map.route_surface(route_surface_id) else {
                return CurrentSupport::MissingIdentity;
            };
            (
                sample_route(route, horizontal),
                Some(route_surface_id.as_str()),
            )
        }
    };
    if let Some(sample) = sample {
        return CurrentSupport::Resolved(sample);
    }
    match resolve_detached_support(
        map,
        profile,
        position.elevation_meters,
        horizontal,
        excluded_route,
    ) {
        Some(sample) => CurrentSupport::Resolved(sample),
        None => CurrentSupport::Unsupported,
    }
}

fn sample_terrain(
    map: &WorldMap,
    position: Position,
    reference_elevation_meters: f32,
) -> Option<GroundSample<'_>> {
    map.terrain_cell_at(position)?;
    let mut surfaces = Vec::new();
    map.terrain_walking_surfaces(position, &mut surfaces);
    let standing_on = nearest_surface(&surfaces, reference_elevation_meters)?;
    Some(GroundSample {
        support: SampleSupport::Terrain,
        surface: standing_on.surface,
        elevation_meters: standing_on.elevation_meters,
        grade_percent: None,
    })
}

/// The walking surface an Actor at this height stands on.
///
/// An excavated column offers more than one, and the nearest is the one the
/// Actor is on: the floor of a tunnel for whoever walks through it, the ground
/// above for whoever walks over it. Two equally near surfaces are refused
/// rather than resolved by order, the same way two reachable Routes are.
///
/// Ground under standing water is not offered at all. Whether an Actor may be
/// on the water itself is its profile's business, and it is asked afterwards.
fn nearest_surface<'a>(
    surfaces: &[MapColumnSurface<'a>],
    elevation_meters: f32,
) -> Option<MapColumnSurface<'a>> {
    let mut nearest: Option<(f32, MapColumnSurface<'a>)> = None;
    let mut ambiguous = false;
    for surface in surfaces.iter().filter(|surface| !surface.flooded) {
        let distance = (surface.elevation_meters - elevation_meters).abs();
        match nearest {
            Some((closest, _)) if distance > closest => continue,
            Some((closest, _)) if distance == closest => ambiguous = true,
            _ => {
                nearest = Some((distance, *surface));
                ambiguous = false;
            }
        }
    }
    if ambiguous {
        return None;
    }
    nearest.map(|(_, surface)| surface)
}

fn sample_route(route: &MapRouteSurface, position: Position) -> Option<GroundSample<'_>> {
    let sample = route.sample_at(position)?;
    Some(GroundSample {
        support: SampleSupport::RouteSurface(&route.route_surface_id),
        surface: &route.surface,
        elevation_meters: sample.elevation_meters,
        grade_percent: Some(sample.grade_percent),
    })
}

pub(super) fn resolve_target<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current: GroundSample<'a>,
    position: Position,
) -> Option<GroundSample<'a>> {
    if let SampleSupport::RouteSurface(route_surface_id) = current.support {
        let route = map.route_surface(route_surface_id)?;
        if let Some(target) = sample_route(route, position) {
            return sample_is_reachable(profile, current.elevation_meters, target)
                .then_some(target);
        }
    }

    let excluded_route = match current.support {
        SampleSupport::Terrain => None,
        SampleSupport::RouteSurface(route_surface_id) => Some(route_surface_id),
    };
    match unique_reachable_route(
        map,
        profile,
        current.elevation_meters,
        position,
        excluded_route,
    ) {
        ReachableRoute::One(route) => return Some(route),
        ReachableRoute::Ambiguous => return None,
        ReachableRoute::None => {}
    }

    let terrain = sample_terrain(map, position, current.elevation_meters)?;
    sample_is_reachable(profile, current.elevation_meters, terrain).then_some(terrain)
}

fn resolve_detached_support<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    position: Position,
    excluded_route: Option<&str>,
) -> Option<GroundSample<'a>> {
    match unique_reachable_route(
        map,
        profile,
        current_elevation_meters,
        position,
        excluded_route,
    ) {
        ReachableRoute::One(route) => return Some(route),
        ReachableRoute::Ambiguous => return None,
        ReachableRoute::None => {}
    }
    let terrain = sample_terrain(map, position, current_elevation_meters)?;
    sample_is_reachable(profile, current_elevation_meters, terrain).then_some(terrain)
}

#[derive(Debug, Clone, Copy)]
enum ReachableRoute<'a> {
    None,
    One(GroundSample<'a>),
    Ambiguous,
}

fn unique_reachable_route<'a>(
    map: &'a WorldMap,
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    position: Position,
    excluded_route: Option<&str>,
) -> ReachableRoute<'a> {
    select_unique_route(map.walked_surfaces().filter_map(|route| {
        if excluded_route == Some(route.route_surface_id.as_str()) {
            return None;
        }
        let sample = sample_route(route, position)?;
        if !sample_is_reachable(profile, current_elevation_meters, sample) {
            return None;
        }
        Some(sample)
    }))
}

fn select_unique_route<'a>(
    candidates: impl IntoIterator<Item = GroundSample<'a>>,
) -> ReachableRoute<'a> {
    let mut candidates = candidates.into_iter();
    let Some(first) = candidates.next() else {
        return ReachableRoute::None;
    };
    if candidates.next().is_some() {
        ReachableRoute::Ambiguous
    } else {
        ReachableRoute::One(first)
    }
}

fn sample_is_reachable(
    profile: &CharacterTraversalProfile,
    current_elevation_meters: f32,
    sample: GroundSample<'_>,
) -> bool {
    sample_is_usable(profile, sample)
        && profile.permits_step(current_elevation_meters, sample.elevation_meters)
}

pub(super) fn sample_is_usable(
    profile: &CharacterTraversalProfile,
    sample: GroundSample<'_>,
) -> bool {
    profile.permits_surface(sample.surface)
        && sample
            .grade_percent
            .is_none_or(|grade| profile.speed_for_grade(grade).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(elevations: &[f32]) -> Vec<MapColumnSurface<'static>> {
        elevations
            .iter()
            .map(|elevation_meters| MapColumnSurface {
                elevation_meters: *elevation_meters,
                surface: "land",
                flooded: false,
            })
            .collect()
    }

    fn standing_at(surfaces: &[MapColumnSurface<'_>], elevation_meters: f32) -> Option<f32> {
        nearest_surface(surfaces, elevation_meters).map(|surface| surface.elevation_meters)
    }

    #[test]
    fn an_actor_stands_on_the_surface_nearest_its_own_height() {
        assert_eq!(standing_at(&column(&[1.0, 10.0]), 1.2), Some(1.0));
        assert_eq!(standing_at(&column(&[1.0, 10.0]), 9.6), Some(10.0));
        assert_eq!(standing_at(&column(&[4.0]), 0.0), Some(4.0));
        assert_eq!(standing_at(&column(&[]), 1.0), None);
    }

    #[test]
    fn two_equally_near_surfaces_are_refused_rather_than_ordered() {
        assert_eq!(standing_at(&column(&[1.0, 3.0]), 2.0), None);
        assert_eq!(standing_at(&column(&[3.0, 1.0]), 2.0), None);
        assert_eq!(
            standing_at(&column(&[1.0, 3.0, 3.1]), 2.0),
            None,
            "a third surface further away does not resolve the tie"
        );
    }

    #[test]
    fn ground_under_standing_water_is_not_offered_at_all() {
        let flooded = [
            MapColumnSurface {
                elevation_meters: 0.5,
                surface: "land",
                flooded: true,
            },
            MapColumnSurface {
                elevation_meters: 1.0,
                surface: "water",
                flooded: false,
            },
        ];

        assert_eq!(
            nearest_surface(&flooded, 0.5).map(|surface| surface.surface),
            Some("water"),
            "an Actor at the height of the bed is offered the water over it, not the bed"
        );
    }

    #[test]
    fn multiple_reachable_routes_are_explicitly_ambiguous() {
        let samples = [
            GroundSample {
                support: SampleSupport::RouteSurface("first"),
                surface: "land",
                elevation_meters: 1.0,
                grade_percent: Some(0),
            },
            GroundSample {
                support: SampleSupport::RouteSurface("second"),
                surface: "land",
                elevation_meters: 1.0,
                grade_percent: Some(0),
            },
        ];

        assert!(matches!(
            select_unique_route(samples),
            ReachableRoute::Ambiguous
        ));
    }
}
