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

    #[test]
    fn layout_uses_authored_ankh_placements() {
        let map = WorldMap::load_embedded().expect("embedded SceneMaker map is valid");
        let layout = AnkhLayout::from_map(&map);

        assert_eq!(layout.positions, [Position::new(4.0, 0.0)]);
    }
}
