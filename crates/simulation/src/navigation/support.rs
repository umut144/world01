//! One definition of where an Actor is supported and where it may step next.
//!
//! The movement constraint, the Path speed rule, and the server's recovery rule
//! all ask that same question, so they ask it here instead of each carrying its
//! own copy of the rule.

use world01_world_data::{GroundSupport, MapRouteSurface, Position, WorldMap, WorldPosition};

use super::{CharacterTraversalProfile, TraversalCatalog};

pub(crate) fn resolve_terrain_position(
    map: &WorldMap,
    traversal: &TraversalCatalog,
    character: &world01_world_data::CharacterId,
    candidate: WorldPosition,
) -> Option<WorldPosition> {
    let profile = traversal.character(character)?;
    let cell = map.terrain_cell_at(candidate.horizontal())?;
    profile
        .permits_surface(&cell.surface)
        .then(|| WorldPosition::new(candidate.x, candidate.y, cell.elevation_meters))
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
        GroundSupport::Terrain => (sample_terrain(map, horizontal), None),
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

fn sample_terrain(map: &WorldMap, position: Position) -> Option<GroundSample<'_>> {
    let cell = map.terrain_cell_at(position)?;
    Some(GroundSample {
        support: SampleSupport::Terrain,
        surface: &cell.surface,
        elevation_meters: cell.elevation_meters,
        grade_percent: None,
    })
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

    let terrain = sample_terrain(map, position)?;
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
    let terrain = sample_terrain(map, position)?;
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
    select_unique_route(map.route_surfaces().iter().filter_map(|route| {
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
