//! The MOBA's per-map design data: which side an authored Prop belongs to.
//!
//! Deliberately apart from [`crate::GameDesign`]. A side is a rule of one game,
//! and the sandbox loads the shared design without ever learning that teams
//! exist. Nothing in this module is part of the World-01 baseline.

use std::collections::HashSet;

use serde::Deserialize;
use world01_world_data::TeamId;

use crate::DesignError;

const MAP01_DESIGN: &str = include_str!("../games/moba/maps/map01.json");

const SCHEMA_VERSION: u32 = 1;
/// The MOBA is played by two sides, so these are the only teams a map may name.
const TEAMS: [TeamId; 2] = [TeamId(0), TeamId(1)];

/// One authored Prop and the side it belongs to.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MobaPropOwner {
    /// The SceneMaker `instance_id` of a Prop placed in this map.
    ///
    /// Never reused within a Scene, which is what lets this file outlive the
    /// Prop it names: a reference that no longer resolves is refused, and one
    /// that resolves can only be the Prop the author meant.
    pub instance_id: String,
    pub team: TeamId,
}

/// Who owns what in one authored map.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MobaMapDesign {
    pub schema_version: u32,
    pub scene_id: String,
    pub props: Vec<MobaPropOwner>,
}

impl MobaMapDesign {
    /// The side of one Prop, or `None` when this map assigns it to nobody.
    pub fn team_of(&self, instance_id: &str) -> Option<TeamId> {
        self.props
            .iter()
            .find(|owner| owner.instance_id == instance_id)
            .map(|owner| owner.team)
    }

    /// What this file can decide on its own.
    ///
    /// Whether an `instance_id` names a Prop the map actually contains, and
    /// whether every Totem has an owner, are questions for wherever the map is -
    /// this crate parses no map and must not pretend to answer them.
    fn validate(&self) -> Result<(), DesignError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(DesignError(format!(
                "map ownership for '{}' uses unsupported schema {}",
                self.scene_id, self.schema_version
            )));
        }
        if self.scene_id.is_empty() {
            return Err(DesignError("map ownership names no scene".into()));
        }
        if self.props.is_empty() {
            return Err(DesignError(format!(
                "map ownership for '{}' assigns no Prop to a side",
                self.scene_id
            )));
        }
        let mut seen = HashSet::with_capacity(self.props.len());
        for owner in &self.props {
            if owner.instance_id.is_empty() {
                return Err(DesignError(format!(
                    "map ownership for '{}' has an entry without an instance",
                    self.scene_id
                )));
            }
            if !TEAMS.contains(&owner.team) {
                return Err(DesignError(format!(
                    "'{}' in '{}' belongs to team {}, and the MOBA has two",
                    owner.instance_id, self.scene_id, owner.team.0
                )));
            }
            if !seen.insert(owner.instance_id.as_str()) {
                return Err(DesignError(format!(
                    "'{}' is assigned twice in '{}'",
                    owner.instance_id, self.scene_id
                )));
            }
        }
        Ok(())
    }
}

/// Every map the MOBA knows who owns what in.
#[derive(Debug, Clone, PartialEq)]
pub struct MobaMapCatalog {
    maps: Vec<MobaMapDesign>,
}

impl MobaMapCatalog {
    pub fn load_embedded() -> Result<Self, DesignError> {
        Self::parse([MAP01_DESIGN])
    }

    /// Parses and validates a set of map-ownership sources together, refusing
    /// two files that claim the same scene.
    ///
    /// Public beyond `load_embedded` so a consumer can build a catalog from
    /// map-ownership JSON it did not embed itself - a test fixture, today.
    pub fn parse<'a>(sources: impl IntoIterator<Item = &'a str>) -> Result<Self, DesignError> {
        let mut maps = Vec::new();
        let mut scenes = HashSet::new();
        for source in sources {
            let map: MobaMapDesign = serde_json::from_str(source)
                .map_err(|error| DesignError(format!("cannot parse map ownership: {error}")))?;
            map.validate()?;
            if !scenes.insert(map.scene_id.clone()) {
                return Err(DesignError(format!(
                    "two ownership files describe scene '{}'",
                    map.scene_id
                )));
            }
            maps.push(map);
        }
        Ok(Self { maps })
    }

    pub fn map(&self, scene_id: &str) -> Option<&MobaMapDesign> {
        self.maps.iter().find(|map| map.scene_id == scene_id)
    }

    pub fn maps(&self) -> &[MobaMapDesign] {
        &self.maps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(body: &str) -> String {
        format!("{{\"schema_version\":1,\"scene_id\":\"map01\",\"props\":[{body}]}}")
    }

    #[test]
    fn the_embedded_map_assigns_three_totems_to_each_side() {
        let catalog = MobaMapCatalog::load_embedded().expect("embedded map ownership is valid");
        let map = catalog.map("map01").expect("map01 has ownership");

        assert_eq!(map.props.len(), 6);
        for team in TEAMS {
            assert_eq!(
                map.props.iter().filter(|owner| owner.team == team).count(),
                3,
                "team {} should own three Totems",
                team.0
            );
        }
        assert_eq!(
            map.team_of("totem_of_life_0002"),
            Some(TeamId(0)),
            "the western Totem of Life"
        );
        assert_eq!(
            map.team_of("totem_of_life_0001"),
            Some(TeamId(1)),
            "the eastern Totem of Life"
        );
    }

    #[test]
    fn a_prop_nobody_assigned_has_no_side() {
        let catalog = MobaMapCatalog::load_embedded().expect("embedded map ownership is valid");
        let map = catalog.map("map01").expect("map01 has ownership");

        assert_eq!(map.team_of("ankh_0001"), None);
        assert_eq!(map.team_of("no_such_prop_9999"), None);
    }

    #[test]
    fn an_unknown_map_is_not_invented() {
        let catalog = MobaMapCatalog::load_embedded().expect("embedded map ownership is valid");

        assert!(catalog.map("overworld01").is_none());
    }

    #[test]
    fn the_same_instance_cannot_belong_to_both_sides() {
        let both = source(
            "{\"instance_id\":\"totem_of_life_0001\",\"team\":0},\
             {\"instance_id\":\"totem_of_life_0001\",\"team\":1}",
        );

        assert!(MobaMapCatalog::parse([both.as_str()]).is_err());
    }

    #[test]
    fn a_third_team_is_refused() {
        let third = source("{\"instance_id\":\"totem_of_life_0001\",\"team\":2}");

        assert!(MobaMapCatalog::parse([third.as_str()]).is_err());
    }

    #[test]
    fn a_map_that_assigns_nothing_is_refused() {
        assert!(MobaMapCatalog::parse([source("").as_str()]).is_err());
    }

    #[test]
    fn an_unsupported_schema_is_refused() {
        let future = "{\"schema_version\":2,\"scene_id\":\"map01\",\"props\":\
                      [{\"instance_id\":\"totem_of_life_0001\",\"team\":0}]}";

        assert!(MobaMapCatalog::parse([future]).is_err());
    }

    #[test]
    fn two_files_cannot_describe_one_scene() {
        let one = source("{\"instance_id\":\"totem_of_life_0001\",\"team\":0}");
        let two = source("{\"instance_id\":\"totem_of_life_0002\",\"team\":1}");

        assert!(MobaMapCatalog::parse([one.as_str(), two.as_str()]).is_err());
    }
}
