use bevy::prelude::{Component, Reflect, Resource};
use serde::{Deserialize, Serialize};

use crate::{Position, WorldMap};

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
    pub positions: Vec<Position>,
}

impl AnkhLayout {
    pub fn from_map(map: &WorldMap) -> Self {
        Self {
            positions: map
                .placements()
                .iter()
                .filter(|placement| placement.asset_key == "ankh")
                .map(|placement| placement.position)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{TEST_GRASS_CELL, test_export};

    const PLACEMENTS: &str = r#"{
        "instance_id": "ankh_0001",
        "asset_key": "ankh",
        "position_authoring_px": { "x": 64, "y": 96 }
    },
    {
        "instance_id": "tree_0001",
        "asset_key": "tree",
        "position_authoring_px": { "x": 0, "y": 0 }
    },
    {
        "instance_id": "ankh_0002",
        "asset_key": "ankh",
        "position_authoring_px": { "x": 96, "y": 64 }
    }"#;

    #[test]
    fn layout_keeps_only_ankh_placements_in_authored_order() {
        let source = test_export(TEST_GRASS_CELL, PLACEMENTS);
        let map = WorldMap::from_source(&source).expect("the synthetic export is valid");

        let layout = AnkhLayout::from_map(&map);

        assert_eq!(
            layout.positions,
            [Position::new(0.0, 1.0), Position::new(1.0, 0.0)]
        );
    }
}
