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

pub const STANDARD_ROOM_WIDTH_METERS: f32 = 15.0;
pub const STANDARD_ROOM_HEIGHT_METERS: f32 = 9.0;
pub const STANDARD_ROOM_CENTER_Y_METERS: f32 = 0.1875;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartingRoomGrid;

impl StartingRoomGrid {
    const MINIMUM_COORDINATE: i32 = -1;
    const MAXIMUM_COORDINATE: i32 = 1;

    pub fn constrain_position(self, position: Position) -> Position {
        Position::new(
            position.x.clamp(Self::minimum_x(), Self::maximum_x()),
            position.y.clamp(Self::minimum_y(), Self::maximum_y()),
        )
    }

    pub fn room_id_at(self, position: Position) -> RoomId {
        let position = self.constrain_position(position);
        let x = ((position.x - Self::minimum_x()) / STANDARD_ROOM_WIDTH_METERS)
            .floor()
            .clamp(0.0, 2.0) as i32
            + Self::MINIMUM_COORDINATE;
        let y = ((position.y - Self::minimum_y()) / STANDARD_ROOM_HEIGHT_METERS)
            .floor()
            .clamp(0.0, 2.0) as i32
            + Self::MINIMUM_COORDINATE;
        Self::room_id_for_coordinates(x, y)
    }

    pub fn room_floor_minimum(self, room: RoomId) -> Vec2 {
        let (x, y) = Self::coordinates_for_room_id(room);
        Vec2::new(
            (x as f32 - 0.5) * STANDARD_ROOM_WIDTH_METERS,
            STANDARD_ROOM_CENTER_Y_METERS + (y as f32 - 0.5) * STANDARD_ROOM_HEIGHT_METERS,
        )
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

    const fn minimum_x() -> f32 {
        (Self::MINIMUM_COORDINATE as f32 - 0.5) * STANDARD_ROOM_WIDTH_METERS
    }

    const fn maximum_x() -> f32 {
        (Self::MAXIMUM_COORDINATE as f32 + 0.5) * STANDARD_ROOM_WIDTH_METERS
    }

    const fn minimum_y() -> f32 {
        STANDARD_ROOM_CENTER_Y_METERS
            + (Self::MINIMUM_COORDINATE as f32 - 0.5) * STANDARD_ROOM_HEIGHT_METERS
    }

    const fn maximum_y() -> f32 {
        STANDARD_ROOM_CENTER_Y_METERS
            + (Self::MAXIMUM_COORDINATE as f32 + 0.5) * STANDARD_ROOM_HEIGHT_METERS
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
        let grid = StartingRoomGrid;

        assert_eq!(grid.starting_room(), RoomId(4));
        assert_eq!(grid.room_id_at(Position::ZERO), RoomId(4));
        assert_eq!(grid.room_id_at(Position::new(7.5, 4.6875)), RoomId(8));
    }

    #[test]
    fn starting_grid_clamps_positions_to_its_open_neighbor_set() {
        let grid = StartingRoomGrid;
        let position = grid.constrain_position(Position::new(30.0, -20.0));

        assert_eq!(position, Position::new(22.5, -13.3125));
        assert_eq!(grid.room_id_at(position), RoomId(2));
    }
}
