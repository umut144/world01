use bevy::prelude::{Component, Reflect, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
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

/// Authoritative position of an actor in World 01's logical 2.5D space.
///
/// `x` and `y` locate the actor on the current support surface. Elevation is a
/// gameplay coordinate rather than Bevy presentation depth.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
pub struct WorldPosition {
    pub x: f32,
    pub y: f32,
    pub elevation_meters: f32,
}

/// The authored surface that currently determines a grounded Actor's height.
///
/// Terrain is one continuous support layer; its cells supply local surface and
/// elevation data without becoming replicated Actor state. Route-surface IDs
/// distinguish overlapping floors such as a bridge above the Terrain below.
#[derive(Debug, Clone, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub enum GroundSupport {
    Terrain,
    RouteSurface { route_surface_id: String },
}

/// Whether an Actor currently follows a ground surface or moves vertically by
/// another rule set.
///
/// Airborne and flying behavior is intentionally not implemented yet. Keeping
/// them explicit prevents a missing ground surface from becoming an implicit
/// Terrain fallback.
#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub enum MovementMedium {
    Grounded(GroundSupport),
    Airborne,
    Flying,
}

impl MovementMedium {
    pub const GROUNDED_TERRAIN: Self = Self::Grounded(GroundSupport::Terrain);
}

impl WorldPosition {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        elevation_meters: 0.0,
    };

    pub const fn new(x: f32, y: f32, elevation_meters: f32) -> Self {
        Self {
            x,
            y,
            elevation_meters,
        }
    }

    pub const fn horizontal(self) -> Position {
        Position::new(self.x, self.y)
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct MovementDirection {
    pub x: f32,
    pub y: f32,
}

impl MovementDirection {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct MovementVelocity {
    pub x: f32,
    pub y: f32,
}

impl MovementVelocity {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        Vec2::new(self.x, self.y).length()
    }

    pub fn normalized(self) -> Self {
        let direction = Vec2::new(self.x, self.y).normalize_or_zero();
        Self::new(direction.x, direction.y)
    }

    pub fn scaled(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct StaminaState {
    pub current: f32,
    pub maximum: f32,
}

impl StaminaState {
    pub fn full(maximum: f32) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct RunState {
    pub toggled: bool,
    pub active: bool,
    pub input_pressed: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect, Serialize, Deserialize)]
pub struct DashState {
    pub active: bool,
    pub elapsed_seconds: f32,
    pub velocity: MovementVelocity,
    pub invulnerable: bool,
    pub input_pressed: bool,
}

impl Default for DashState {
    fn default() -> Self {
        Self {
            active: false,
            elapsed_seconds: 0.0,
            velocity: MovementVelocity::ZERO,
            invulnerable: false,
            input_pressed: false,
        }
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
