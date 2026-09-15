//! Which Totems stand on the composed map, and whose each one is.
//!
//! A Totem's kind comes from the PolyTools Asset it was placed from; whose it
//! is comes from the MOBA's map-ownership design data. Both facts have to
//! agree with what the map actually places, which is why building this fails
//! loudly in both directions - see
//! `docs/games/moba/GAME_MOBA_DESIGN.md#how-the-runtime-learns-what-stands-where`.

use std::{collections::HashSet, error::Error, fmt};

use bevy::prelude::{Component, Resource};
use world01_world_data::{TeamId, WorldMap, WorldPosition};

use super::MobaMapOwnership;

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

/// One placed Totem: which kind it is and which side it defends.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Totem {
    pub kind: TotemKind,
    pub team: TeamId,
}

/// Every Totem on the composed map, positioned and assigned to a side.
///
/// Empty for a map that places no Totem at all - an ordinary World-01 map
/// carries none, and that is not an error. Only once a map places at least
/// one Totem does this type have anything to check.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct TotemLayout {
    pub totems: Vec<(Totem, WorldPosition)>,
}

/// Why a map's Totems and its ownership file disagree.
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
        }
    }
}

impl Error for TotemLayoutError {}

impl TotemLayout {
    /// Derives the Totems standing on a composed map, in the same
    /// fixed-tick world transaction that already derives `AnkhLayout`, so a
    /// Totem survives a recomposition the way an Ankh does.
    ///
    /// Fails in both directions a stale reference can fail in: a Totem the
    /// map places that the ownership file never names, and an ownership
    /// entry that names no Totem the map actually places.
    pub fn from_map(
        map: &WorldMap,
        ownership: &MobaMapOwnership,
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
            placed_instances.insert(placement.instance_id.as_str());
            totems.push((
                Totem { kind: *kind, team },
                WorldPosition::new(
                    placement.position.x,
                    placement.position.y,
                    placement.elevation_meters,
                ),
            ));
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
                "version": 19,
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

    #[test]
    fn a_map_without_a_totem_needs_no_ownership_file_at_all() {
        let source = synthetic_map(SCENE_ID, "");
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = MobaMapOwnership::from_sources([
            "{\"schema_version\":1,\"scene_id\":\"some_other_map\",\"props\":             [{\"instance_id\":\"totem_of_life_0001\",\"team\":0}]}",
        ])
        .expect("the synthetic ownership file is valid");

        let layout = TotemLayout::from_map(&map, &ownership).expect("no Totem, nothing to check");

        assert!(layout.totems.is_empty());
    }

    #[test]
    fn a_totem_the_file_owns_carries_its_kind_team_and_position() {
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

        let layout = TotemLayout::from_map(&map, &ownership).expect("the Totem is owned");

        assert_eq!(
            layout.totems,
            [(
                Totem {
                    kind: TotemKind::Life,
                    team: TeamId(0)
                },
                expected_position,
            )]
        );
    }

    #[test]
    fn a_totem_the_file_never_names_is_refused() {
        let entries = totem("totem_of_mana_0001", "totem_of_mana", 64, 96);
        let source = synthetic_map(SCENE_ID, &entries);
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = ownership("{\"instance_id\":\"totem_of_time_0001\",\"team\":0}");

        let error = TotemLayout::from_map(&map, &ownership).expect_err("the Totem is unowned");

        assert_eq!(
            error,
            TotemLayoutError::Unowned {
                instance_id: "totem_of_mana_0001".to_owned()
            }
        );
    }

    #[test]
    fn a_file_entry_naming_no_placed_totem_is_refused() {
        let entries = totem("totem_of_time_0001", "totem_of_time", 64, 96);
        let source = synthetic_map(SCENE_ID, &entries);
        let map = WorldMap::from_source(&source, SCENE_ID).expect("the synthetic export is valid");
        let ownership = ownership(
            "{\"instance_id\":\"totem_of_time_0001\",\"team\":0},             {\"instance_id\":\"totem_of_life_0002\",\"team\":1}",
        );

        let error = TotemLayout::from_map(&map, &ownership).expect_err("one entry dangles");

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
            "{\"schema_version\":1,\"scene_id\":\"some_other_map\",\"props\":             [{\"instance_id\":\"totem_of_life_0001\",\"team\":0}]}",
        ])
        .expect("the synthetic ownership file is valid");

        let error = TotemLayout::from_map(&map, &ownership).expect_err("no ownership file");

        assert_eq!(
            error,
            TotemLayoutError::NoOwnershipForScene {
                scene_id: SCENE_ID.to_owned()
            }
        );
    }
}
