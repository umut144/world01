# World 01 — Current Tasks

This file tracks only current, next, blocked, or deliberately deferred
outcomes. Completed implementation history remains available in Git. Tasks may
use vertical slices when that helps produce a small, testable result; completed
slices are not retained here as a permanent project chronicle.

## One sandbox, many games

World 01 is a sandbox: an engine plus one asset library, from which many games
are built. The ones foreseen - an MMORPG, a MOBA, a battle royale, and above
all funmaps - share most of their mechanics and differ in data. The model is
Warcraft III: one fixed engine, one editor, and the map is the unit that
carries the game.

Three levels, and the boundary between them is what this section exists to
record:

- The **engine** is the mechanics: the tick, collision, movement, damage,
  health, navigation, the content pipeline. Rust, shared by everything, and
  never aware of any game.
- A **game** is data: which rule values apply, which maps belong to it, which
  Realms it contains, how a session starts and ends. Its unit is
  `design/games/<key>/` plus its maps. Never a crate.
- A **Realm** is a camera model and a movement model together - what a player
  feels as "this part plays like this". Realms are shared vocabulary rather
  than a game's property: a game selects the ones it uses, and two games may
  select the same one.

A mechanic belongs in the engine; a configuration of mechanics is data. When
something cannot be expressed as data, the engine is missing a mechanic, and
that mechanic belongs in the engine as a generic one - not beside it in a
game-shaped crate.

**A Realm is one id with two consumers.** Server and client derive the same
Realm from the same design data; the server reads only its movement model, the
client reads both. That leaves exactly one value the two sides have to agree
on, which matters because the halves are unalike: a camera model is pure client
presentation - no determinism, no replication, no protocol - while a movement
model sits inside `add_simulation_step` and runs on the server and the
predicting client alike. A disagreement there shows as rubber-banding, not as
an error.

The models are engine code and the Realms are data rows that pick one value per
axis, so two camera models and four movement models are six pieces of code and
eight Realms - and a further Realm after that costs no code at all.

What this means concretely, using the MMORPG being built: its caves are
room-centred camera with atomic free movement, flight is follow-player camera
with analog forced movement, and the cloud layer is follow-player with atomic
forced. *Free* means the player decides whether to move; *forced* means the
Actor keeps moving along its last movement vector whether input arrives or not.
*Analog* means continuous positions, *atomic* discrete steps. A side-scrolling
jump-and-run is a separate game and uses analog free movement with either
camera - the same Realm its overworld would use. A game with several Realms is
the normal case, not a special one.

One axis is deliberately left open: perspective. Side-scrolling is not another
value beside the models above. `WorldPosition` spans its plane with `x` and `y`
and carries elevation as a third, gameplay-owned coordinate, while a
side-scroller wants `x` and elevation as its plane - which reaches collision,
navigation and ground support. That is a question to answer when it arrives,
not a cell in the same table.

SceneMaker authors one flat set of Scenes and its export says nothing about
Realms, because a map editor has no business knowing what physics applies.

None of this exists in code, and none of it is being built ahead of need. Work
continues on the sandbox itself, mechanic by mechanic, exactly as before. This
section is the target the work is aimed at, not a plan to execute.

When it is taken up, the intended order is: a second camera model first,
because it is client-only and proves the data path from map to Realm without
touching the deterministic step; then the Realm assignment itself; then a
second movement model, the first real test of the seam; then `SBX-25`, whose
conditions and actions resolved by name from design files are the same machine
a funmap's triggers will need.

A MOBA was started on 2026-09-13 and removed on 2026-09-26. The removal stands,
and the reason is sharper now than it was then: that MOBA was a crate full of
Rust types - `Totem`, `TotemKind`, `TotemLayout`, its own admission rules -
which is exactly the shape the picture above rejects. Taken apart, a Totem is a
destructible Prop with health (engine), owned by a side (`TeamId`, kept), whose
destruction ends the round (data, plus one generic system). None of it needed a
game crate. We built one because we thought of games as code.

What the MOBA left behind is deliberate: `TeamId` as an identity type without
rules attached, `PlacementRanks::extended_with`, a publicly constructible
`DesignError`, the gitignored `runtime.local.toml` start-map override, and the
authored-footprint overlap check. What went with it is the crate, its design
data, its map, its documents and the Totem assets, along with the
`WorldDerivation` and `SessionRules` seams - a seam with no second
implementation is indirection, not architecture. When a second real case
exists, the seam gets cut to two examples instead of one imagined one.

The Labyrinth went the same day and for the same reason: a 64-player dungeon
battle royale is a game, and a game is data and maps, not a design document
standing beside the engine's. What it built stays - the characters, weapons,
abilities, health and status foundations were always the engine's own, and
`docs/WORLD_DESIGN.md` owns them.

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
