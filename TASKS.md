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

## Tracker rules

- Keep at most one task **In progress**.
- Describe an observable outcome, not an implementation diary.
- Add acceptance detail only when it is needed to decide whether the task is complete.
- Remove completed rows after the immediate handoff; Git preserves their history.
