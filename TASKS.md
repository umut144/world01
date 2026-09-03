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
| `SBX-24` | Bots | Derive a navigation representation from `WorldMap` and the collision geometry, with deterministic pathfinding on it. Built at startup but rebuildable for the region an Anchor covers, because a Template swap changes the world mid-session; walkability comes from intersecting a cell's `surface` and the height step against the Actor's traversal profile. | **Next** |
| `SBX-25` | Bots | Add an `ai` crate whose data-driven behaviour tree writes only intent components, running in a `SimulationSet::Decision` phase before the gameplay step, with node conditions and actions resolved by name from design files. | **Blocked on `SBX-24`** |
| `SBX-26` | Bots | Replicate bots as server-authoritative and interpolated rather than predicted, so `SimulationAuthority::Predicted` skips them and neither the behaviour tree nor pathfinding has to be deterministic on the client. | **Blocked on `SBX-25`** |
| `SBX-27` | Client presentation | Split `presentation.rs` into separate input, character-selection and render-reconciliation plugins, and source character colours from content instead of the two hand-maintained palettes in `polytools.rs` and `presentation.rs`. | Planned |
| `SBX-28` | Sandbox architecture | Retire the leftovers the architecture review found: the simulation tests living in `lib.rs` instead of beside their modules, the public `CharacterId` field that bypasses its validating constructor, and the server address hard-coded in the network crate. | Planned |
| `SBX-29` | Sandbox architecture | Move the rules and catalog types out of `simulation` into a `world_rules` crate so they can be queried without the ECS systems. | **Deferred until a second consumer needs the rules without the systems, such as the `ai` crate or a balancing tool** |
| `SBX-32` | Bots | Expose the status action mask and the incoming-damage modifiers as queries on `ActorCondition`, covering the three categories the design names, so that the simulation and a bot's evaluation function read one rule rather than two. `silenced_ticks` currently ticks down and nothing reads it. | **Blocked on `SBX-25`, or on the first shoulder-button ability** |
| `SBX-34` | World data | Place Templates at Anchors and let the server exchange an occupant while the game runs, so it can drive events. Which Template a place receives is a deliberate game decision; an Anchor with none stays empty. Geometry follows SceneMaker's `EXPORT_CONTRACT.md`: translate by anchor minus insertion anchor, the translated Terrain cells are the mask, heights travel unchanged. What happens to what was already there is ours: a Template replaces its own footprint - surface, height and Props - except where an existing thing outranks it, resolved on the server from ranks in the design data and replicated as the outcome. The complete SceneMaker export directory is synchronized and embedded as a deterministic catalog; Instances load by scene ID, Templates are typed and grouped, and a pure geometry projection validates and translates one explicit Template/Anchor pair without merging it. Projected Prop origins stay within the Instance; merging will namespace their IDs as `template.<anchor_id>.<template_scene_id>.<local_prop_id>` and reject collisions. Next: add the design-owned placement ranks and pure merge policy before runtime exchange. | **In progress** |

## Tracker rules

- Keep at most one task **In progress**.
- Describe an observable outcome, not an implementation diary.
- Add acceptance detail only when it is needed to decide whether the task is complete.
- Remove completed rows after the immediate handoff; Git preserves their history.
