//! Shared protocol-neutral domain data.

use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::{Component, Reflect, Resource, Vec2},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterId(pub String);

impl CharacterId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()).then_some(Self(value))
    }

    pub fn label(&self) -> String {
        self.0
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct CharacterCatalog {
    ids: HashSet<CharacterId>,
}

impl CharacterCatalog {
    pub fn from_json(source: &str) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct Catalog {
            assets: Vec<Asset>,
        }
        #[derive(Deserialize)]
        struct Asset {
            asset_key: String,
            asset_type: String,
        }
        let catalog: Catalog = serde_json::from_str(source)?;
        let ids = catalog
            .assets
            .into_iter()
            .filter(|asset| asset.asset_type == "character")
            .filter_map(|asset| CharacterId::new(asset.asset_key))
            .collect();
        Ok(Self { ids })
    }

    pub fn contains(&self, id: &CharacterId) -> bool {
        self.ids.contains(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &CharacterId> {
        self.ids.iter()
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u64);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerOwner(pub u64);

#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedCharacter(pub CharacterId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Player;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(pub u32);

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct StartingRoomGrid {
    width_meters: f32,
    height_meters: f32,
}

impl Default for StartingRoomGrid {
    fn default() -> Self {
        Self::from_tiles(15, 9).expect("default room dimensions are valid")
    }
}

impl StartingRoomGrid {
    const MINIMUM_COORDINATE: i32 = -1;
    const MAXIMUM_COORDINATE: i32 = 1;

    pub fn from_tiles(width_tiles: u32, height_tiles: u32) -> Option<Self> {
        (width_tiles > 0 && height_tiles > 0).then_some(Self {
            width_meters: width_tiles as f32,
            height_meters: height_tiles as f32,
        })
    }

    pub const fn width_meters(self) -> f32 {
        self.width_meters
    }

    pub const fn height_meters(self) -> f32 {
        self.height_meters
    }

    pub fn constrain_position(self, position: Position) -> Position {
        Position::new(
            position.x.clamp(self.minimum_x(), self.maximum_x()),
            position.y.clamp(self.minimum_y(), self.maximum_y()),
        )
    }

    pub fn room_id_at(self, position: Position) -> RoomId {
        let position = self.constrain_position(position);
        let x = ((position.x - self.minimum_x()) / self.width_meters)
            .floor()
            .clamp(0.0, 2.0) as i32
            + Self::MINIMUM_COORDINATE;
        let y = ((position.y - self.minimum_y()) / self.height_meters)
            .floor()
            .clamp(0.0, 2.0) as i32
            + Self::MINIMUM_COORDINATE;
        Self::room_id_for_coordinates(x, y)
    }

    pub fn room_floor_minimum(self, room: RoomId) -> Vec2 {
        let (x, y) = Self::coordinates_for_room_id(room);
        Vec2::new(
            (x as f32 - 0.5) * self.width_meters,
            (y as f32 - 0.5) * self.height_meters,
        )
    }

    pub fn room_center(self, room: RoomId) -> Vec2 {
        self.room_floor_minimum(room) + Vec2::new(self.width_meters, self.height_meters) * 0.5
    }

    pub const fn starting_room(self) -> RoomId {
        Self::room_id_for_coordinates(0, 0)
    }

    const fn room_id_for_coordinates(x: i32, y: i32) -> RoomId {
        RoomId(((y + 1) * 3 + (x + 1)) as u32)
    }

    fn coordinates_for_room_id(room: RoomId) -> (i32, i32) {
        let index = room.0.min(8) as i32;
        (index % 3 - 1, index / 3 - 1)
    }

    const fn minimum_x(self) -> f32 {
        (Self::MINIMUM_COORDINATE as f32 - 0.5) * self.width_meters
    }

    const fn maximum_x(self) -> f32 {
        (Self::MAXIMUM_COORDINATE as f32 + 0.5) * self.width_meters
    }

    const fn minimum_y(self) -> f32 {
        (Self::MINIMUM_COORDINATE as f32 - 0.5) * self.height_meters
    }

    const fn maximum_y(self) -> f32 {
        (Self::MAXIMUM_COORDINATE as f32 + 0.5) * self.height_meters
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StandardRoom;

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub room: RoomId,
    pub x_meters: f32,
    pub y_meters: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct MovementIntent {
    pub x: f32,
    pub y: f32,
}

impl MapEntities for MovementIntent {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

impl MovementIntent {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct GazeIntent {
    pub x: f32,
    pub y: f32,
}

impl GazeIntent {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct GazeDirection {
    pub x: f32,
    pub y: f32,
}

impl GazeDirection {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub enum BodyFacing {
    #[default]
    Authored,
    Left,
    Right,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct PlayerInput {
    pub movement: MovementIntent,
    pub gaze: GazeIntent,
}

impl PlayerInput {
    pub const ZERO: Self = Self {
        movement: MovementIntent::ZERO,
        gaze: GazeIntent::ZERO,
    };

    pub const fn new(movement: MovementIntent, gaze: GazeIntent) -> Self {
        Self { movement, gaze }
    }
}

impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_grid_uses_the_same_room_for_origin_and_diagonals() {
        let grid = StartingRoomGrid::default();

        assert_eq!(grid.starting_room(), RoomId(4));
        assert_eq!(grid.room_id_at(Position::ZERO), RoomId(4));
        assert_eq!(grid.room_id_at(Position::new(7.5, 4.5)), RoomId(8));
    }

    #[test]
    fn starting_grid_clamps_positions_to_its_open_neighbor_set() {
        let grid = StartingRoomGrid::default();
        let position = grid.constrain_position(Position::new(30.0, -20.0));

        assert_eq!(position, Position::new(22.5, -13.5));
        assert_eq!(grid.room_id_at(position), RoomId(2));
    }
}
