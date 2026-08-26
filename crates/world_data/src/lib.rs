//! Shared protocol-neutral domain data.

use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::{Component, Reflect, Resource, Vec2},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CharacterKind {
    Wizard,
    Mage,
    Sorcerer,
    Rogue,
    Glavier,
}

impl CharacterKind {
    pub const ALL: [Self; 5] = [
        Self::Wizard,
        Self::Mage,
        Self::Sorcerer,
        Self::Rogue,
        Self::Glavier,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Wizard => "Wizard",
            Self::Mage => "Mage",
            Self::Sorcerer => "Sorcerer",
            Self::Rogue => "Rogue",
            Self::Glavier => "Glavier",
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u64);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerOwner(pub u64);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedCharacter(pub CharacterKind);

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
