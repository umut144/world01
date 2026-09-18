//! Who may join a session, and where they appear when they do.
//!
//! The sandbox owns the lifecycle - a client connected, an Actor entity has to
//! exist, here are the components every Character carries. What it does not own
//! is which side that Actor plays on or where in the world it appears: those
//! are rules of one game, and a sandbox that answered them would be answering
//! for every game at once.
//!
//! Note what does *not* live here: the choice itself. A side is picked before
//! the join, travels with it, and is only ever validated on arrival. The
//! server does not derive an identity the player never stated - a rule that
//! matters more once a lobby lets players pick sides, because a server that
//! can invent a side will quietly disagree with the lobby that assigned one.

use std::{error::Error, fmt};

use world01_world_data::{CharacterId, TeamId, WorldPosition};

use crate::WorldDerivation;

/// What a joining player brings with them.
///
/// Today a developer states it on the client's command line; later a lobby
/// will. Either way it arrives as a claim to be checked, not as a question for
/// the server to answer on the player's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinedIdentity {
    pub character: CharacterId,
    /// The side the player picked, or `None` when nobody picked one.
    pub team: Option<TeamId>,
}

/// Where and as what an admitted Actor enters the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Admission {
    pub position: WorldPosition,
    /// The side the game confirmed. `None` leaves the Actor without one, which
    /// is what a game with no sides admits every player as.
    pub team: Option<TeamId>,
}

/// Why a join was refused.
///
/// Refusal is an outcome rather than a failure: a player who brought no side
/// to a game played in sides is told so, and the session carries on serving
/// everyone already in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRefused(String);

impl JoinRefused {
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }
}

impl fmt::Display for JoinRefused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for JoinRefused {}

/// A game's own answer to who joins and where.
///
/// Sits on [`WorldDerivation`] rather than beside it because admission reads
/// the world the game derived: the MOBA spawns behind the Totem of Life its
/// derivation placed, so the two would have to agree anyway. Making that one
/// type means they cannot be configured apart.
pub trait SessionRules: WorldDerivation {
    fn admit(brought: &JoinedIdentity, derived: &Self::Derived) -> Result<Admission, JoinRefused>;
}
