//! The MOBA's own design data: which side an authored Prop belongs to, and
//! what a Totem is worth.
//!
//! Deliberately apart from [`crate::GameDesign`]. A side is a rule of one
//! game, and the sandbox loads the shared design without ever learning that
//! teams - or Totems - exist. Nothing in this module is part of the
//! World-01 baseline.

use std::collections::HashSet;

use serde::Deserialize;
use world01_world_data::TeamId;

use crate::DesignError;

const MAP01_DESIGN: &str = include_str!("../games/moba/maps/map01.json");
const TOTEM_DESIGN: &str = include_str!("../games/moba/totems.json");

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

/// One Totem kind's maximum health, by the PolyTools Asset key it is placed
/// from.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MobaTotemHealth {
    pub asset_key: String,
    pub max_hp: f32,
}

/// Every Totem kind's maximum health.
///
/// A Totem's MaxHP is MOBA design data rather than derived from its authored
/// fill area - the World-01 area-based derivation exists to keep Characters
/// comparable to one another, and a building is not on that scale. This file
/// says nothing about which kinds a map actually places or leaves unowned;
/// that is answered wherever the map and this design meet, not here.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MobaTotemDesign {
    pub schema_version: u32,
    pub totems: Vec<MobaTotemHealth>,
}

impl MobaTotemDesign {
    pub fn load_embedded() -> Result<Self, DesignError> {
        let design: Self = serde_json::from_str(TOTEM_DESIGN)
            .map_err(|error| DesignError(format!("cannot parse Totem design: {error}")))?;
        design.validate()?;
        Ok(design)
    }

    /// A Totem kind's maximum health, or `None` when this file says nothing
    /// about the Asset key.
    pub fn max_hp(&self, asset_key: &str) -> Option<f32> {
        self.totems
            .iter()
            .find(|totem| totem.asset_key == asset_key)
            .map(|totem| totem.max_hp)
    }

    fn validate(&self) -> Result<(), DesignError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(DesignError(format!(
                "Totem design uses unsupported schema {}",
                self.schema_version
            )));
        }
        if self.totems.is_empty() {
            return Err(DesignError("Totem design names no Totem kind".into()));
        }
        let mut seen = HashSet::with_capacity(self.totems.len());
        for totem in &self.totems {
            if totem.asset_key.is_empty() {
                return Err(DesignError("a Totem design entry names no Asset".into()));
            }
            if !totem.max_hp.is_finite() || totem.max_hp <= 0.0 {
                return Err(DesignError(format!(
                    "'{}' has a non-positive or non-finite MaxHP",
                    totem.asset_key
                )));
            }
            if !seen.insert(totem.asset_key.as_str()) {
                return Err(DesignError(format!(
                    "'{}' is named twice in the Totem design",
                    totem.asset_key
                )));
            }
        }
        Ok(())
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

    #[test]
    fn the_embedded_totem_design_names_all_three_kinds() {
        let design = MobaTotemDesign::load_embedded().expect("embedded Totem design is valid");

        for asset_key in ["totem_of_life", "totem_of_mana", "totem_of_time"] {
            assert!(
                design.max_hp(asset_key).is_some_and(|hp| hp > 0.0),
                "{asset_key} should have positive MaxHP"
            );
        }
        assert_eq!(design.max_hp("totem_of_wisdom"), None);
    }

    fn totem_source(body: &str) -> String {
        format!("{{\"schema_version\":1,\"totems\":[{body}]}}")
    }

    #[test]
    fn a_totem_named_twice_is_refused() {
        let design: MobaTotemDesign = serde_json::from_str(&totem_source(
            "{\"asset_key\":\"totem_of_life\",\"max_hp\":10.0},             {\"asset_key\":\"totem_of_life\",\"max_hp\":20.0}",
        ))
        .expect("the JSON itself parses");

        assert!(design.validate().is_err());
    }

    #[test]
    fn a_non_positive_max_hp_is_refused() {
        for max_hp in ["0.0", "-1.0"] {
            let source = totem_source(&format!(
                "{{\"asset_key\":\"totem_of_life\",\"max_hp\":{max_hp}}}"
            ));
            let design: MobaTotemDesign =
                serde_json::from_str(&source).expect("the JSON itself parses");
            assert!(
                design.validate().is_err(),
                "max_hp {max_hp} should be refused"
            );
        }
    }

    #[test]
    fn a_totem_design_naming_nothing_is_refused() {
        let design: MobaTotemDesign =
            serde_json::from_str(&totem_source("")).expect("the JSON itself parses");

        assert!(design.validate().is_err());
    }

    #[test]
    fn an_unsupported_totem_design_schema_is_refused() {
        let design: MobaTotemDesign = serde_json::from_str(
            "{\"schema_version\":2,\"totems\":             [{\"asset_key\":\"totem_of_life\",\"max_hp\":10.0}]}",
        )
        .expect("the JSON itself parses");

        assert!(design.validate().is_err());
    }
}
