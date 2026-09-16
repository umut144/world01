//! Which Totems stand on the composed map, whose each one is, and what it
//! is worth.
//!
//! A Totem's kind comes from the PolyTools Asset it was placed from; whose it
//! is comes from the MOBA's map-ownership design data; what it is worth
//! comes from the MOBA's Totem-health design data. All three have to agree
//! with what the map actually places, which is why building this fails
//! loudly in every direction a stale reference can fail in - see
//! `docs/games/moba/GAME_MOBA_DESIGN.md#how-the-runtime-learns-what-stands-where`.

use std::{collections::HashSet, error::Error, fmt};

use bevy::prelude::{Component, Resource};
use world01_world_data::{TeamId, WorldMap, WorldPosition};

use super::{MobaMapOwnership, MobaTotemHealthDesign};

/// The three kinds of Totem the MOBA places, one PolyTools Asset each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotemKind {
    Life,
    Mana,
    Time,
}

impl TotemKind {
    /// The kind a placed Prop is, or `None` when it is not a Totem at all.
    ///
    /// The Asset key is the only source: nothing about a Totem's kind is
    /// inferred from a name convention, a position, or the file that says
    /// whose it is.
    fn from_asset_key(asset_key: &str) -> Option<Self> {
        match asset_key {
            "totem_of_life" => Some(Self::Life),
            "totem_of_mana" => Some(Self::Mana),
            "totem_of_time" => Some(Self::Time),
            _ => None,
        }
    }
}

/// One placed Totem's identity: which kind it is and which side it defends.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Totem {
    pub kind: TotemKind,
    pub team: TeamId,
}

/// One Totem as the composed map and the MOBA's design data agree it stands:
/// its identity, where it is, and how much it can take.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedTotem {
    pub totem: Totem,
    pub position: WorldPosition,
    pub max_hp: f32,
}

/// Every Totem on the composed map, positioned, assigned to a side, and
/// given its health.
///
/// Empty for a map that places no Totem at all - an ordinary World-01 map
/// carries none, and that is not an error. Only once a map places at least
/// one Totem does this type have anything to check.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct TotemLayout {
    pub totems: Vec<PlacedTotem>,
}

/// Why a map's Totems, its ownership file, or its health file disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TotemLayoutError {
    /// The map places a Totem and the MOBA has no ownership file for its
    /// scene at all.
    NoOwnershipForScene { scene_id: String },
    /// A Totem stands on the map and the ownership file never names it.
    Unowned { instance_id: String },
    /// The ownership file names an instance the map places no Totem at -
    /// deleted, renamed, or never a Totem to begin with.
    Dangling {
        instance_id: String,
        placed_totems: Vec<String>,
    },
    /// A Totem stands on the map and the Totem-health design names no MaxHP
    /// for its Asset key.
    NoHealthForKind {
        instance_id: String,
        asset_key: String,
    },
}

impl fmt::Display for TotemLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOwnershipForScene { scene_id } => write!(
                formatter,
                "'{scene_id}' places a Totem, and the MOBA has no ownership file for it"
            ),
            Self::Unowned { instance_id } => write!(
                formatter,
                "Totem '{instance_id}' stands on the map and the ownership file assigns it to no \
                 side"
            ),
            Self::Dangling {
                instance_id,
                placed_totems,
            } => {
                write!(
                    formatter,
                    "the ownership file assigns '{instance_id}' to a side, but the map places no \
                     Totem with that instance; the map's Totems are: "
                )?;
                if placed_totems.is_empty() {
                    formatter.write_str("none")
                } else {
                    formatter.write_str(&placed_totems.join(", "))
                }
            }
            Self::NoHealthForKind {
                instance_id,
                asset_key,
            } => write!(
                formatter,
                "Totem '{instance_id}' is placed from Asset '{asset_key}', which the Totem-health \
                 design names no MaxHP for"
            ),
        }
    }
}

impl Error for TotemLayoutError {}

impl TotemLayout {
    /// Derives the Totems standing on a composed map, in the same
    /// fixed-tick world transaction that already derives `AnkhLayout`, so a
    /// Totem survives a recomposition the way an Ankh does.
    ///
    /// Fails in every direction a stale or incomplete design can fail in: a
    /// Totem the map places that the ownership file never names, an
    /// ownership entry that names no Totem the map actually places, and a
    /// Totem whose Asset the Totem-health design says nothing about.
    pub fn from_map(
        map: &WorldMap,
        ownership: &MobaMapOwnership,
        health: &MobaTotemHealthDesign,
    ) -> Result<Self, TotemLayoutError> {
        let totem_placements: Vec<_> = map
            .props()
            .iter()
            .filter_map(|placement| {
                TotemKind::from_asset_key(&placement.asset_key).map(|kind| (placement, kind))
            })
            .collect();

        if totem_placements.is_empty() {
            return Ok(Self::default());
        }

        let design =
            ownership
                .map(map.scene_id())
                .ok_or_else(|| TotemLayoutError::NoOwnershipForScene {
                    scene_id: map.scene_id().to_owned(),
                })?;

        let mut totems = Vec::with_capacity(totem_placements.len());
        let mut placed_instances = HashSet::with_capacity(totem_placements.len());
        for (placement, kind) in &totem_placements {
            let Some(team) = design.team_of(&placement.instance_id) else {
                return Err(TotemLayoutError::Unowned {
                    instance_id: placement.instance_id.clone(),
                });
            };
            let Some(max_hp) = health.max_hp(&placement.asset_key) else {
                return Err(TotemLayoutError::NoHealthForKind {
                    instance_id: placement.instance_id.clone(),
                    asset_key: placement.asset_key.clone(),
                });
            };
            placed_instances.insert(placement.instance_id.as_str());
            totems.push(PlacedTotem {
                totem: Totem { kind: *kind, team },
                position: WorldPosition::new(
                    placement.position.x,
                    placement.position.y,
                    placement.elevation_meters,
                ),
                max_hp,
            });
        }

        if let Some(dangling) = design
            .props
            .iter()
            .find(|owner| !placed_instances.contains(owner.instance_id.as_str()))
        {
            let mut placed_totems: Vec<String> = placed_instances
                .iter()
                .map(|instance_id| (*instance_id).to_owned())
                .collect();
            placed_totems.sort();
            return Err(TotemLayoutError::Dangling {
                instance_id: dangling.instance_id.clone(),
                placed_totems,
            });
        }

        Ok(Self { totems })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE_ID: &str = "totem_layout_test";

    fn synthetic_map(scene_id: &str, extra_props: &str) -> String {
        let comma = if extra_props.is_empty() { "" } else { "," };
        format!(
            r#"{{
                "format": "scene_maker_scene_export",
                "version": 20,
                "workspace_key": "world01",
                "grid": {{
                    "terrain_cell_meters": 1.0,
                    "authoring_pixels_per_meter": 32.0,
                    "game_pixels_per_meter": 192.0,
                    "water_cell_meters": 0.5
                }},
                "asset_profiles": [
                    {{ "asset_key": "grass", "surface": "land", "footprint_meters": null, "anchor_meters": null }},
                    {{ "asset_key": "ankh", "surface": null, "footprint_meters": {{ "width": 1.0625, "height": 1.71875 }}, "anchor_meters": {{ "x": 0.53125, "y": 0.3125 }} }},
                    {{ "asset_key": "totem_of_life", "surface": null, "footprint_meters": {{ "width": 1.28, "height": 1.28 }}, "anchor_meters": {{ "x": 0.64, "y": 0.64 }} }},
                    {{ "asset_key": "totem_of_mana", "surface": null, "footprint_meters": {{ "width": 1.28, "height": 1.28 }}, "anchor_meters": {{ "x": 0.64, "y": 0.64 }} }},
                    {{ "asset_key": "totem_of_time", "surface": null, "footprint_meters": {{ "width": 1.28, "height": 1.28 }}, "anchor_meters": {{ "x": 0.64, "y": 0.64 }} }}
                ],
                "water_raster": [],
                "route_surface_bakes": [],
                "route_surface_cut_raster": [],
                "bridge_bakes": [],
                "water_bakes": [],
                "scene": {{
                    "schema": "srt.scene_maker_scene",
                    "version": 17,
                    "scene_id": "{scene_id}",
                    "scene_kind": "instance",
                    "size_cells": {{ "width": 16, "height": 16 }},
                    "coordinate_space": "scene_local_bottom_left_y_up",
                    "terrain_cells": [
                        {{ "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }}
                    ],
                    "props": [
                        {{
                            "instance_id": "ankh_0001",
                            "asset_key": "ankh",
                            "position_authoring_px": {{ "x": 288, "y": 256 }},
                            "elevation_meters": 1.0
                        }}{comma}{extra_props}
                    ],
                    "switches": [],
                    "water_bodies": [],
                    "route_surfaces": [],
                    "bridges": [],
                    "template_definition": null,
                    "template_anchors": [],
                    "default_elevation_meters": 1.0
                }}
            }}"#,
            scene_id = scene_id,
            comma = comma,
            extra_props = extra_props,
        )
    }

    fn totem(instance_id: &str, asset_key: &str, x: i64, y: i64) -> String {
        format!(
            r#"{{
                "instance_id": "{instance_id}",
                "asset_key": "{asset_key}",
                "position_authoring_px": {{ "x": {x}, "y": {y} }},
                "elevation_meters": 1.0
            }}"#
        )
    }

    fn ownership(body: &str) -> MobaMapOwnership {
        let source =
            format!("{{\"schema_version\":1,\"scene_id\":\"{SCENE_ID}\",\"props\":[{body}]}}");
        MobaMapOwnership::from_sources([source.as_str()])
            .expect("the synthetic ownership file is valid")
    }

    fn health(body: &str) -> MobaTotemHealthDesign {
        let source = format!("{{\"schema_version\":1,\"totems\":[{body}]}}");
        MobaTotemHealthDesign::from_sources(&source).expect("the synthetic health file is valid")
    }

    fn full_health() -> MobaTotemHealthDesign {
        health(
            "{\"asset_key\":\"totem_of_life\",\"max_hp\":2000.0},\
             {\"asset_key\":\"totem_of_mana\",\"max_hp\":1000.0},\
             {\"asset_key\":\"totem_of_time\",\"max_hp\":1000.0}",
        )
    }

    #[test]
    fn a_map_without_a_totem_needs_no_ownership_or_health_file_at_all() {
        let source = synthetic_map(SCENE_ID, "");
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = MobaMapOwnership::from_sources([
            "{\"schema_version\":1,\"scene_id\":\"some_other_map\",\"props\":\
             [{\"instance_id\":\"totem_of_life_0001\",\"team\":0}]}",
        ])
        .expect("the synthetic ownership file is valid");

        let layout = TotemLayout::from_map(&map, &ownership, &full_health())
            .expect("no Totem, nothing to check");

        assert!(layout.totems.is_empty());
    }

    #[test]
    fn a_totem_the_files_own_carries_its_kind_team_position_and_health() {
        let entries = totem("totem_of_life_0001", "totem_of_life", 64, 96);
        let source = synthetic_map(SCENE_ID, &entries);
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let placed = map
            .props()
            .iter()
            .find(|prop| prop.instance_id == "totem_of_life_0001")
            .expect("the Totem is placed");
        let expected_position = WorldPosition::new(
            placed.position.x,
            placed.position.y,
            placed.elevation_meters,
        );
        let ownership = ownership("{\"instance_id\":\"totem_of_life_0001\",\"team\":0}");

        let layout = TotemLayout::from_map(&map, &ownership, &full_health())
            .expect("the Totem is owned and priced");

        assert_eq!(
            layout.totems,
            [PlacedTotem {
                totem: Totem {
                    kind: TotemKind::Life,
                    team: TeamId(0)
                },
                position: expected_position,
                max_hp: 2000.0,
            }]
        );
    }

    #[test]
    fn a_totem_the_ownership_file_never_names_is_refused() {
        let entries = totem("totem_of_mana_0001", "totem_of_mana", 64, 96);
        let source = synthetic_map(SCENE_ID, &entries);
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = ownership("{\"instance_id\":\"totem_of_time_0001\",\"team\":0}");

        let error = TotemLayout::from_map(&map, &ownership, &full_health())
            .expect_err("the Totem is unowned");

        assert_eq!(
            error,
            TotemLayoutError::Unowned {
                instance_id: "totem_of_mana_0001".to_owned()
            }
        );
    }

    #[test]
    fn a_totem_the_health_file_never_names_is_refused() {
        let entries = totem("totem_of_mana_0001", "totem_of_mana", 64, 96);
        let source = synthetic_map(SCENE_ID, &entries);
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = ownership("{\"instance_id\":\"totem_of_mana_0001\",\"team\":0}");
        let health = health("{\"asset_key\":\"totem_of_life\",\"max_hp\":2000.0}");

        let error = TotemLayout::from_map(&map, &ownership, &health)
            .expect_err("the Totem has no listed MaxHP");

        assert_eq!(
            error,
            TotemLayoutError::NoHealthForKind {
                instance_id: "totem_of_mana_0001".to_owned(),
                asset_key: "totem_of_mana".to_owned(),
            }
        );
    }

    #[test]
    fn a_file_entry_naming_no_placed_totem_is_refused() {
        let entries = totem("totem_of_time_0001", "totem_of_time", 64, 96);
        let source = synthetic_map(SCENE_ID, &entries);
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = ownership(
            "{\"instance_id\":\"totem_of_time_0001\",\"team\":0},\
             {\"instance_id\":\"totem_of_life_0002\",\"team\":1}",
        );

        let error =
            TotemLayout::from_map(&map, &ownership, &full_health()).expect_err("one entry dangles");

        assert_eq!(
            error,
            TotemLayoutError::Dangling {
                instance_id: "totem_of_life_0002".to_owned(),
                placed_totems: vec!["totem_of_time_0001".to_owned()],
            }
        );
    }

    #[test]
    fn a_map_with_a_totem_and_no_ownership_file_is_refused() {
        let entries = totem("totem_of_life_0001", "totem_of_life", 64, 96);
        let source = synthetic_map(SCENE_ID, &entries);
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = MobaMapOwnership::from_sources([
            "{\"schema_version\":1,\"scene_id\":\"some_other_map\",\"props\":\
             [{\"instance_id\":\"totem_of_life_0001\",\"team\":0}]}",
        ])
        .expect("the synthetic ownership file is valid");

        let error =
            TotemLayout::from_map(&map, &ownership, &full_health()).expect_err("no ownership file");

        assert_eq!(
            error,
            TotemLayoutError::NoOwnershipForScene {
                scene_id: SCENE_ID.to_owned()
            }
        );
    }

    #[test]
    fn the_embedded_map01_places_six_totems_three_per_side_with_every_kind() {
        let map = WorldMap::load_embedded("map01").expect("the embedded map01 export is valid");
        let templates = world01_world_data::WorldTemplateCatalog::load_embedded()
            .expect("embedded Templates are valid");
        let ranks = world01_design::load_world01_embedded()
            .expect("embedded World 01 design parses")
            .placement_ranks()
            .expect("embedded Placement Ranks are valid")
            .extended_with(
                world01_design::moba::MobaPlacementRanksDesign::load_embedded()
                    .expect("embedded Placement Rank overlay is valid")
                    .entries(),
            )
            .expect("the overlay only adds Assets the sandbox has not ranked");
        world01_world_data::WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("map01 composes with the sandbox's ranks extended by the MOBA overlay");

        let ownership = MobaMapOwnership::load_embedded().expect("embedded map ownership is valid");
        let health =
            MobaTotemHealthDesign::load_embedded().expect("embedded Totem design is valid");
        let layout = TotemLayout::from_map(&map, &ownership, &health)
            .expect("map01's Totems, ownership, and health all agree");

        assert_eq!(layout.totems.len(), 6, "three Totems per side");
        for team in [TeamId(0), TeamId(1)] {
            let team_totems: Vec<_> = layout
                .totems
                .iter()
                .filter(|placed| placed.totem.team == team)
                .collect();
            assert_eq!(
                team_totems.len(),
                3,
                "team {} should have three Totems",
                team.0
            );
            for kind in [TotemKind::Life, TotemKind::Mana, TotemKind::Time] {
                assert!(
                    team_totems.iter().any(|placed| placed.totem.kind == kind),
                    "team {} should have a {kind:?}",
                    team.0
                );
            }
        }
    }
}
