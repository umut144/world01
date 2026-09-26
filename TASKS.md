# World 01 — Current Tasks

This file tracks only current, next, blocked, or deliberately deferred
outcomes. Completed implementation history remains available in Git. Tasks may
use vertical slices when that helps produce a small, testable result; completed
slices are not retained here as a permanent project chronicle.

## One world, one game

World 01 is a single game - an MMORPG - built the way it has been built so far:
the sandbox grows asset by asset and system by system, and the game is what it
grows into. There is no second game and no plugin boundary between a core and a
genre on top of it.

A MOBA was started on 2026-09-13 and removed on 2026-09-26. What it left behind
is deliberate: `TeamId` as an identity type without rules attached,
`PlacementRanks::extended_with`, a publicly constructible `DesignError`, the
gitignored `runtime.local.toml` start-map override, the authored-footprint
overlap check, and SceneMaker export contract v21. What went with it is the
MOBA's own crate, design data, map, documents and Totem assets, along with the
`WorldDerivation` and `SessionRules` seams - a seam with no second
implementation is indirection, not architecture.

The world is divided into **Realms**: a Realm carries the mechanics its Scenes
are played under, so a side-scrolling area is a different Realm from the walked
overworld. Which Scene belongs to which Realm is decided here, from this
project's own design data - SceneMaker authors one flat set of Scenes per
Workspace and its export says nothing about Realms, because a map editor has no
business knowing what physics applies.

No Realm assignment exists in code yet, and that is deliberate: there is one
Realm, every Scene belongs to it, and the data that says otherwise arrives with
the second one.

The Labyrinth went the same way on 2026-09-26. It was a 64-player dungeon
battle royale - the concept this codebase originally grew from, and a second
game by any reading of the decision above. Its design document is out of the
tree and in Git history, and the documents no longer route agents to it. What
it built stays: the characters, weapons, abilities, health and status
foundations were always World-01's own, and `docs/WORLD_DESIGN.md` owns them.

| ID | Area | Outcome | Status |
|---|---|---|---|
| `WORLD-18` | World rendering | Evaluate a Bevy tilemap crate for the SceneMaker-authored world representation, including compatibility with the current engine-neutral map export and PolyTools asset profiles. | **Deferred for later performance work** |
| `WORLD-19` | World rendering | Design and implement chunked tilemap rendering/streaming for large maps, with explicit chunk size, culling, update boundaries, and a migration path from the current repeated `Mesh2d` terrain presentation. | **Blocked on `WORLD-18`** |
| `SBX-25` | Bots | Add an `ai` crate whose data-driven behaviour tree writes only intent components, running in a `SimulationSet::Decision` phase before the gameplay step, with node conditions and actions resolved by name from design files. | Planned |
| `SBX-26` | Bots | Replicate bots as server-authoritative and interpolated rather than predicted, so `SimulationAuthority::Predicted` skips them and neither the behaviour tree nor pathfinding has to be deterministic on the client. | **Blocked on `SBX-25`** |
| `SBX-27` | Client presentation | Split `presentation.rs` into separate input, character-selection and render-reconciliation plugins, and source character colours from content instead of the two hand-maintained palettes in `polytools.rs` and `presentation.rs`. | Planned |
| `SBX-28` | Sandbox architecture | Retire the leftovers the architecture review found: the simulation tests living in `lib.rs` instead of beside their modules, the public `CharacterId` field that bypasses its validating constructor, and the server address hard-coded in the network crate. | Planned |
| `SBX-29` | Sandbox architecture | Move the rules and catalog types out of `simulation` into a `world_rules` crate so they can be queried without the ECS systems. | **Deferred until a second consumer needs the rules without the systems, such as the `ai` crate or a balancing tool** |
| `SBX-32` | Bots | Expose the status action mask and the incoming-damage modifiers as queries on `ActorCondition`, covering the three categories the design names, so that the simulation and a bot's evaluation function read one rule rather than two. `silenced_ticks` currently ticks down and nothing reads it. | **Blocked on `SBX-25`, or on the first shoulder-button ability** |
| `WORLD-21` | Content | Settle where the Asset pivot is subtracted before an Asset needs a non-zero one. `placed_component_geometry` leaves Component-bound Region geometry in raw Asset space, the export already subtracts the pivot from authored Region vertices, and attachment frames stay raw, so the two branches of `region_geometry` disagree and hit surfaces would move. Every Asset places its pivot at the origin today, which is the only reason none of this is visible. | **Deferred until an Asset places its pivot away from the origin** |
| `SBX-41` | World authoring | Let SceneMaker author a switch button - an Asset placed on the map that names the switch it throws - so the pairing stops living in `AUTHORED_BUTTONS` and moving a tile stops meaning a code change. Phase 2 of [`docs/RIVER_ROADMAP.md`](docs/RIVER_ROADMAP.md) left this behind deliberately. | **Blocked on SceneMaker and PolyTools** |
| `WORLD-20` | World collision | Add dirty-flag caching to `WorldColliderGrid` so it rebuilds only when `WorldCollisionGeometryCatalog` changes (via Bevy's `Changed<T>` detection), reducing unnecessary broad-phase index reconstruction when props move but the collision geometry remains static. | Planned |

## Tracker rules

- Keep at most one task **In progress**.
- Describe an observable outcome, not an implementation diary.
- Add acceptance detail only when it is needed to decide whether the task is complete.
- Remove completed rows after the immediate handoff; Git preserves their history.
