use bevy::{
    ecs::entity::{EntityMapper, MapEntities},
    prelude::{Component, Reflect},
};
use serde::{Deserialize, Serialize};

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

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct AttackIntent {
    pub pressed: bool,
}

impl AttackIntent {
    pub const RELEASED: Self = Self { pressed: false };
    pub const PRESSED: Self = Self { pressed: true };

    pub const fn new(pressed: bool) -> Self {
        Self { pressed }
    }
}

/// The second attack input, beside [`AttackIntent`].
///
/// Which ability either intent starts is not decided here and is not sent: a
/// system runs on the Actors carrying the state its ability needs, so the
/// archetype decides who hears a press.
#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct AttackSecondaryIntent {
    pub pressed: bool,
}

impl AttackSecondaryIntent {
    pub const RELEASED: Self = Self { pressed: false };
    pub const PRESSED: Self = Self { pressed: true };

    pub const fn new(pressed: bool) -> Self {
        Self { pressed }
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct RunIntent {
    pub pressed: bool,
}

impl RunIntent {
    pub const RELEASED: Self = Self { pressed: false };
    pub const PRESSED: Self = Self { pressed: true };

    pub const fn new(pressed: bool) -> Self {
        Self { pressed }
    }
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct DashIntent {
    pub pressed: bool,
}

#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize,
)]
pub struct DeathConfirmIntent {
    pub pressed: bool,
}

impl DeathConfirmIntent {
    pub const RELEASED: Self = Self { pressed: false };
    pub const PRESSED: Self = Self { pressed: true };

    pub const fn new(pressed: bool) -> Self {
        Self { pressed }
    }
}

impl DashIntent {
    pub const RELEASED: Self = Self { pressed: false };
    pub const PRESSED: Self = Self { pressed: true };

    pub const fn new(pressed: bool) -> Self {
        Self { pressed }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct PlayerInput {
    pub movement: MovementIntent,
    pub gaze: GazeIntent,
    pub attack: AttackIntent,
    pub attack_secondary: AttackSecondaryIntent,
    pub run: RunIntent,
    pub dash: DashIntent,
    pub death_confirm: DeathConfirmIntent,
}

impl PlayerInput {
    pub const ZERO: Self = Self {
        movement: MovementIntent::ZERO,
        gaze: GazeIntent::ZERO,
        attack: AttackIntent::RELEASED,
        attack_secondary: AttackSecondaryIntent::RELEASED,
        run: RunIntent::RELEASED,
        dash: DashIntent::RELEASED,
        death_confirm: DeathConfirmIntent::RELEASED,
    };
}

impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}
