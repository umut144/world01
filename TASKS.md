# World 01 — Current Tasks

This file tracks only current, next, blocked, or deliberately deferred
outcomes. Completed implementation history remains available in Git. Tasks may
use vertical slices when that helps produce a small, testable result; completed
slices are not retained here as a permanent project chronicle.

## Documentation routing

Until this slice, no separate World Design document existed and `AGENTS.md`
identified the Labyrinth document as the only game-design source. As a result,
Codex naturally used Labyrinth context for shared characters, weapons,
abilities, and mechanics, even when a request was not Labyrinth-specific.

Shared World 01 design now belongs in `docs/WORLD_DESIGN.md`. The Labyrinth
document contains only Labyrinth-specific context, tuning, scope, and explicit
overrides. Future agents must read World Design for cross-game work and the
active game document for that game's deviations.

| ID | Area | Outcome | Status |
|---|---|---|---|
| `LAB-17` | The Labyrinth | Add the first authoritative server-side implementation of the shared World-01 Hammer impact contract using configured attack Components and server-owned damage. | **Deferred while sandbox work is prioritized** |
| `WORLD-18` | World rendering | Evaluate a Bevy tilemap crate for the SceneMaker-authored world representation, including compatibility with the current engine-neutral map export and PolyTools asset profiles. | **Deferred for later performance work** |
| `WORLD-19` | World rendering | Design and implement chunked tilemap rendering/streaming for large maps, with explicit chunk size, culling, update boundaries, and a migration path from the current repeated `Mesh2d` terrain presentation. | **Blocked on `WORLD-18`** |
| `SBX-22` | Sandbox architecture | Resolve every source of damage through one event: Hammer impacts, eye beams and stamina exhaustion emit it, and a single system is the only writer of `CharacterHealth`. | **Next** |
| `SBX-21` | Sandbox architecture | Replace `update_locomotion` with one system per concern - status timers, exertion (stamina, run, dash), movement velocity and position integration - with a `SimulationSet::Collision` phase between velocity and integration, and the exertion rules exposed as queries on `LocomotionRules` rather than predicates inlined in the system. `locomotion.rs` disappears so the name is free for a later animation layer; the tick a dash ends keeps moving at dash speed as it does today. | **Blocked on `SBX-22`** |
| `SBX-23` | Sandbox architecture | Move line-of-sight and overlap queries out of `combat::mage` into a shared spatial module with a broadphase over the map, so combat and future perception use one implementation. | **Blocked on `SBX-21`** |
| `SBX-24` | Bots | Derive a navigation representation from `WorldMap` and the static collision geometry at startup, alongside the existing content catalogs, with deterministic pathfinding on it. | **Blocked on `SBX-23`** |
| `SBX-25` | Bots | Add an `ai` crate whose data-driven behaviour tree writes only intent components, running in a `SimulationSet::Decision` phase before the gameplay step, with node conditions and actions resolved by name from design files. | **Blocked on `SBX-20`, `SBX-24`** |
| `SBX-26` | Bots | Replicate bots as server-authoritative and interpolated rather than predicted, so `SimulationAuthority::Predicted` skips them and neither the behaviour tree nor pathfinding has to be deterministic on the client. | **Blocked on `SBX-25`** |
| `SBX-27` | Client presentation | Split `presentation.rs` into separate input, character-selection and render-reconciliation plugins, and source character colours from content instead of the two hand-maintained palettes in `polytools.rs` and `presentation.rs`. | Planned |
| `SBX-28` | Sandbox architecture | Retire the leftovers the architecture review found: the unused `move_players`, the simulation tests living in `lib.rs` instead of beside their modules, the public `CharacterId` field that bypasses its validating constructor, and the server address hard-coded in the network crate. | Planned |
| `SBX-29` | Sandbox architecture | Move the rules and catalog types out of `simulation` into a `world_rules` crate so they can be queried without the ECS systems. | **Deferred until a second consumer needs the rules without the systems, such as the `ai` crate or a balancing tool** |
| `SBX-30` | Sandbox architecture | Decide whether a root stops a dash. `rooted_ticks` only sets `blocks_movement`, which is applied to the normal velocity, while a dash moves on its own frozen vector and is cancelled only by `blocks_all_input` - so a rooted actor keeps dashing at full speed. | **Deferred until an ability applies root** |

## Tracker rules

- Keep at most one task **In progress**.
- Describe an observable outcome, not an implementation diary.
- Add acceptance detail only when it is needed to decide whether the task is complete.
- Remove completed rows after the immediate handoff; Git preserves their history.
