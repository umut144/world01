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

## The MOBA branch

This is `game/moba`, taken from `main` on 2026-09-13. The game's design lives in
[`docs/games/moba/GAME_MOBA_DESIGN.md`](docs/games/moba/GAME_MOBA_DESIGN.md) and
its build order and authoring gates in
[`docs/games/moba/MOBA_ROADMAP.md`](docs/games/moba/MOBA_ROADMAP.md). Only the
current phase appears in the table below; the phases beyond it are described in
the roadmap so the tracker does not become a second plan.

One working rule governs every commit on this branch: **a commit is either
sandbox-generic or MOBA-owned, never both.** A generic commit compiles and
passes its tests without anything MOBA-specific, and cherry-picks onto `main` as
it stands. A mixed commit has to be taken apart later by someone who has
forgotten which half was which.

| ID | Area | Outcome | Status |
|---|---|---|---|
| `SBX-42` | World authoring | Raise the map importer's `FORMAT_VERSION` from 19 to 20 together with a SceneMaker re-export of every map at that version, so the runtime requires the never-reused `instance_id` guarantee rather than assuming it. Neither half works alone: raising it first rejects every map in `assets/`, re-exporting first loads maps whose promise nothing checks. | **Blocked on SceneMaker re-exporting `workspaces/world01/*/exports/` at version 20** |
| `MOBA-02` | MOBA | Phase 2: MOBA design data for Totem ownership and health, a `TotemLayout` derived beside `AnkhLayout`, destructible Totems, team assignment and team-join spawning, and a match that ends when a Totem of Life falls. | Gates A, B and C met. `TotemLayout` is derived in the same fixed-tick transaction as `AnkhLayout` and refuses a Totem the ownership file never names, an ownership entry that names no placed Totem, or a kind with no health entry. Team assignment and join spawn at the team's own Totem of Life are done. Remaining: a `destructible` Region on the Totem Assets (Gate A needs re-authoring - the `hurt` Region it originally asked for is inert on a Prop), Totem entities with health wired into the existing damage resolution, and the win condition |
| `LAB-17` | The Labyrinth | Add the first authoritative server-side implementation of the shared World-01 Hammer impact contract using configured attack Components and server-owned damage. | **Deferred while sandbox work is prioritized** |
| `WORLD-18` | World rendering | Evaluate a Bevy tilemap crate for the SceneMaker-authored world representation, including compatibility with the current engine-neutral map export and PolyTools asset profiles. | **Deferred for later performance work** |
| `WORLD-19` | World rendering | Design and implement chunked tilemap rendering/streaming for large maps, with explicit chunk size, culling, update boundaries, and a migration path from the current repeated `Mesh2d` terrain presentation. | **Blocked on `WORLD-18`** |
| `SBX-25` | Bots | Add an `ai` crate whose data-driven behaviour tree writes only intent components, running in a `SimulationSet::Decision` phase before the gameplay step, with node conditions and actions resolved by name from design files. | Planned — built as MOBA roadmap Phase 4 |
| `SBX-26` | Bots | Replicate bots as server-authoritative and interpolated rather than predicted, so `SimulationAuthority::Predicted` skips them and neither the behaviour tree nor pathfinding has to be deterministic on the client. | **Blocked on `SBX-25`** — built beside it in MOBA roadmap Phase 4 |
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
