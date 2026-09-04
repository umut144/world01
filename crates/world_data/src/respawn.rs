use bevy::prelude::{Component, Reflect, Resource};
use serde::{Deserialize, Serialize};

use crate::{WorldMap, WorldPosition};

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct Ankh {
    pub index: u32,
}

impl Ankh {
    pub const fn new(index: u32) -> Self {
        Self { index }
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Reflect, Serialize, Deserialize)]
pub struct AnkhLayout {
    pub positions: Vec<WorldPosition>,
}

impl AnkhLayout {
    pub fn from_map(map: &WorldMap) -> Self {
        Self {
            positions: map
                .props()
                .iter()
                .filter(|placement| placement.asset_key == "ankh")
                .map(|placement| {
                    WorldPosition::new(
                        placement.position.x,
                        placement.position.y,
                        placement.elevation_meters,
                    )
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{TEST_GRASS_CELL, TEST_SCENE_ID, test_export};

    const PROPS: &str = r#"{
        "instance_id": "ankh_0001",
        "asset_key": "ankh",
        "position_authoring_px": { "x": 64, "y": 96 },
        "elevation_meters": 1.0
    },
    {
        "instance_id": "tree_0001",
        "asset_key": "tree",
        "position_authoring_px": { "x": 0, "y": 0 },
        "elevation_meters": 1.0
    },
    {
        "instance_id": "ankh_0002",
        "asset_key": "ankh",
        "position_authoring_px": { "x": 96, "y": 64 },
        "elevation_meters": 1.0
    }"#;

    #[test]
    fn layout_keeps_only_ankh_placements_in_authored_order() {
        let source = test_export(TEST_GRASS_CELL, PROPS);
        let map =
            WorldMap::from_source(&source, TEST_SCENE_ID).expect("the synthetic export is valid");

        let layout = AnkhLayout::from_map(&map);

        assert_eq!(
            layout.positions,
            [
                WorldPosition::new(0.0, 1.0, 1.0),
                WorldPosition::new(1.0, 0.0, 1.0),
            ]
        );
    }
}
