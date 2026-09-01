//! Geometry queries: what overlaps what, and which pairs are worth testing.
//!
//! The narrow phase in [`overlap`] is exact - it decides triangle against
//! triangle on the authored meshes - and it is deliberately source-agnostic: an
//! authored Region and a mesh component are the same thing to it. The broad
//! phase in [`broadphase`] only narrows down which pairs reach it.

pub mod broadphase;
pub(crate) mod overlap;
