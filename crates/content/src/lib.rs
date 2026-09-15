//! Canonical PolyTools Runtime Manifest import and validation boundary.

mod derived;
mod manifest;

pub use derived::{
    CharacterCollisionGeometry, CharacterCollisionGeometryCatalog, CharacterHealthCatalog,
    CharacterHealthError, CharacterHurtGeometry, CharacterHurtGeometryCatalog,
    CharacterHurtGeometryError, CharacterMassGeometryCatalog, CharacterMassGeometryError,
    CollisionComponentGeometry, DensityAreas, EyeBeamEmitterGeometry, HammerCombatGeometry,
    HammerCombatGeometryError, MageEyeGeometry, MageEyeGeometryError, PlacedCollisionGeometry,
    PlacedDestructibleGeometry, RegionGeometryError, RuntimeComponentGeometry,
    WorldCollisionGeometryCatalog, WorldDestructibleGeometryCatalog,
};
pub use manifest::{
    AuthoredFacing, ContentError, HAMMER_ASSET_KEY, RUNTIME_MANIFEST_SCHEMA_VERSION,
    RuntimeAttachmentFrame, RuntimeComponent, RuntimeComposition, RuntimeContent,
    RuntimeFrameTransform, RuntimeManifest, RuntimeMesh, RuntimePresentation,
    RuntimeProjectionDepthCorner, RuntimeRegion, RuntimeRegionMesh, RuntimeStrokeMesh,
    RuntimeStrokeRun, RuntimeTransform, WEAPON_ATTACK_POINT_ROLE, WEAPON_GRIP_ROLE,
    WEAPON_REACH_LIMIT_ROLE, WEAPON_SECONDARY_GRIP_ROLE, WEAPON_SOCKET_ROLE,
};
