use bevy::prelude::{Component, Reflect, Resource};
use serde::{Deserialize, Serialize};

use crate::Position;

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
    pub fn for_room(width_tiles: u32, height_tiles: u32) -> Self {
        let width = width_tiles as f32;
        let height = height_tiles as f32;
        let offsets = [(-0.25, -0.20), (0.20, 0.15), (-0.05, 0.35)];

        Self {
            positions: offsets
                .into_iter()
                .map(|(x, y)| Position::new(width * x, height * y))
                .collect(),
        }
    }
}

impl Default for AnkhLayout {
    fn default() -> Self {
        Self::for_room(16, 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_room_layout_contains_three_distinct_ankhs_around_the_center() {
        let layout = AnkhLayout::for_room(16, 10);

        assert_eq!(layout.positions.len(), 3);
        assert_eq!(layout.positions[0], Position::new(-4.0, -2.0));
        assert_eq!(layout.positions[1], Position::new(3.2, 1.5));
        assert_eq!(layout.positions[2], Position::new(-0.8, 3.5));
    }
}
