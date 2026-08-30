//! Canonical PolyTools Runtime Manifest import and validation boundary.

mod derived;
mod manifest;

pub use derived::{
    CharacterHealthCatalog, CharacterHealthError, CharacterHurtGeometry,
    CharacterHurtGeometryCatalog, CharacterHurtGeometryError, CharacterMassGeometryCatalog,
    CharacterMassGeometryError, DensityAreas, HammerCombatGeometry, HammerCombatGeometryError,
    RuntimeComponentGeometry,
};
pub use manifest::{
    AuthoredFacing, ContentError, HAMMER_ASSET_KEY, RuntimeAttachmentFrame, RuntimeComponent,
    RuntimeContent, RuntimeFrameTransform, RuntimeManifest, RuntimeMesh, RuntimePresentation,
    RuntimeRegionMesh, RuntimeStrokeMesh, RuntimeStrokeRun, RuntimeTransform,
    WEAPON_ATTACK_POINT_ROLE, WEAPON_GRIP_ROLE, WEAPON_REACH_LIMIT_ROLE,
    WEAPON_SECONDARY_GRIP_ROLE, WEAPON_SOCKET_ROLE,
};
