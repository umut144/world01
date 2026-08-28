//! Canonical PolyTools Runtime Manifest import and validation boundary.

mod derived;
mod manifest;

pub use derived::{
    CharacterHealthCatalog, CharacterHealthError, HammerCombatGeometry, HammerCombatGeometryError,
};
pub use manifest::{
    AuthoredFacing, ContentError, RuntimeAttachmentFrame, RuntimeComponent, RuntimeContent,
    RuntimeFrameTransform, RuntimeManifest, RuntimeMesh, RuntimePresentation, RuntimeRegionMesh,
    RuntimeSemanticRegion, RuntimeStrokeMesh, RuntimeStrokeRun, RuntimeTransform,
};
