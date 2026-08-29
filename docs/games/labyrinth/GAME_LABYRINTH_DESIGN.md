# The Labyrinth — game-specific design context

> This document is the game-specific design source for The Labyrinth inside
> World 01. Shared characters, weapons, abilities, health foundations, visual
> language, and cross-game design principles live in
> [`../../WORLD_DESIGN.md`](../../WORLD_DESIGN.md). Sandbox-wide contracts live
> in [`../../SANDBOX_VISION.md`](../../SANDBOX_VISION.md) and
> [`../../SANDBOX_TECHNICAL.md`](../../SANDBOX_TECHNICAL.md).

Last updated: 2026-08-29

## Purpose and authority

This is compact continuity context for AI agents after a chat or context
switch. It records the solo developer's current intent for The Labyrinth, not
a polished pitch or a frozen specification.

Interpretation rules:

- Read [`WORLD_DESIGN.md`](../../WORLD_DESIGN.md) first for shared World-01
  concepts.
- This document owns only Labyrinth-specific rules, context, tuning, scope,
  and explicit exceptions to World Design.
- Do not copy a shared World-01 mechanic here as if it were Labyrinth-owned.
  Link to the World-01 source and record only the Labyrinth override.
- Distinguish **confirmed direction**, **provisional first-iteration values**,
  and **open questions**.
- Do not silently turn examples or brainstorming into requirements.
- Do not implement anything unless explicitly requested.
- Update this file when the developer makes a durable Labyrinth-specific
  design decision.

## Identity

- Game name: **The Labyrinth**.
- World: **World 01**.
- The current repository and Rust package namespace still use `game01`; this
  technical rename is tracked separately from the design-document migration.
- The shared theme title is defined in [`WORLD_DESIGN.md`](../../WORLD_DESIGN.md)
  and must not be redefined here.

## Core game

- 2D top-down, room-based Dungeon Battle Royale with Roguelite-style
  persistent meta progression.
- 64 players enter an initially regular `A × A` labyrinth.
- First shipped/developed mode: **Solo only**.
- Future mode: teams of four. Do not design its detailed revive/balance rules
  yet, but do not architecturally exclude team play.
- Players can die to other players, mobs, hazards, or the collapse of a room.
- The winner is the final surviving solo player; a future team mode ends with
  one surviving team.
- The central match experience is: explore rooms, gain power (especially
  through kills), react tactically to collapse, traverse a changing topology,
  fight over increasingly limited space, and finish in a small surviving area.
- The Labyrinth is intentionally **slow-paced**. New Labyrinth movement,
  combat, interaction, and timing values should start from that baseline rather
  than from fast action-game conventions.

## Match progression and desired snowballing

- Kills and other match activities make a character substantially stronger for
  the current match.
- In-match progression is temporary and intentionally stronger than meta
  progression.
- Snowballing is a deliberate design choice. It should matter without becoming
  uncontrollable.
- Meta progression provides many small optimizations and slightly better
  starting conditions. Accumulated micro-advantages are intentionally real and
  may increase a player's win rate.
- Exact balance is not first-iteration scope. The first iteration needs
  identifiable/tunable parameters (“Stellschrauben”); balance should emerge
  iteratively across seasons, match data, and community feedback.
- Future matchmaking may increasingly group similarly strong/progressed
  players, especially late in a season. Population, queue times, brackets, and
  exact filtering are future operational problems.

## Labyrinth-specific reference scale and camera

| Area | Reference | Value | Status |
|---|---|---:|---|
| Camera | Standard viewport | `2880 × 1800 px` | Confirmed |
| Movement | All characters in The Labyrinth | `0.8 m/s` | Confirmed Labyrinth baseline |

- Other displays must be scaled/framed so they reveal neither more nor less
  relevant game world.
- The current camera test uses one **50 × 50 tile** room; room dimensions are
  independent from the camera's visible tile count.
- The camera view is configured separately (`view_width_tiles` /
  `view_height_tiles`) and letterboxes unused native-window area.
- Larger logical spaces can be composed from multiple room units, while each
  room's configured tile dimensions remain independent and are shown whole.
- The camera follows and centers the complete active room; exceptions and
  special rooms are possible.
- Room dimensions are configured before a match and may vary between rooms;
  every configured room is shown completely and centered in its aspect-matched
  camera frame.
- The current camera test intentionally has no neighbor-room or transition
  presentation; the character starts at the center of the single room.
- The current playable implementation remains a single room. The former fixed
  `3 × 3` test grid is retired and does not define current gameplay state;
  multi-room topology returns only with a later explicitly scoped gameplay
  slice.
- Room transitions are intended to be completely seamless at first, without a
  visible door, threshold, or transition moment.

## Collapse: essential Labyrinth design

- The labyrinth progressively collapses toward a core room or connected core
  area chosen deterministically-randomly from the match seed/start state.
- Collapse behaves broadly like a shrinking radius but should produce an
  organic room boundary.
- A small connected group of rooms may remain as the final arena instead of
  collapsing to exactly one room. It must be small enough to force the
  surviving players to meet and finish.
- A room about to collapse shakes/bebt. Players must escape through an
  available cardinal exit.
- Adjacent rooms may already be threatened, forcing continued movement until a
  safe region is reached.
- Remaining in a room when it collapses is fatal and ends that player's match.
- Route replanning is primarily **tactical**, not strategic.

### Graph invariant

Model rooms as nodes and doors/tunnels as edges. After each normal collapse
step:

- Every remaining room must have a path to the surviving core/core area.
- The surviving room graph must therefore stay connected to that core.
- A bottleneck room may be an articulation point and cannot be removed while
  its removal would isolate surviving areas.
- Two large areas can instead have one or more deliberate tunnel/connection
  edges. Redundant tunnels allow routes to disappear gradually without
  immediate isolation.

Portals are an acceptable creative fallback if a generated/collapsing state
would isolate players, for example a temporary escape portal appearing in an
isolated room. The primary algorithm need not be mathematically perfect if
failure is safely detected and turned into a coherent game event.

### Multiple collapse variants

Do not assume one universal collapse algorithm. The intended direction is to
co-design several fun collapse variants/rule sets and draw one per match.
Examples discussed, not all confirmed for iteration 1:

- conventional outside-to-core collapse;
- directionally biased collapse;
- organic/jagged borders;
- protected bottlenecks;
- large regions linked by one room;
- large regions linked by one or several tunnels;
- occasional valid inner collapse;
- different final-area topologies.

The variants should share clear invariants while creating different match
character.

### Collapse communication

Current direction is diegetic communication:

- shaking rooms/walls;
- a central floor crack extending toward cardinal directions;
- longer cracks may warn across several neighboring rooms in a direction;
- the strongest collapse direction may shake more;
- more falling debris/VFX on the threatened side;
- directional and escalating sound cues.

Exact warning timing, cascades, and fairness rules will be designed
iteratively when this system is in scope.

### Offline validation intention

Before relying on a collapse variant in game, visualize generated states as
simple raster/PNG grid images and inspect them manually. Tests should make
connectivity failures, islands, protected regions, bottlenecks, and odd
collapse shapes easy to see.

## Room content

Known room categories include:

- quiet/empty exploration and discovery rooms;
- shops;
- bot/mob combat rooms;
- timed puzzle challenges;
- hazard rooms whose hazard can be disabled via a room button;
- bonus rooms granting a free item or upgrade;
- further room types later.

Room value is contextual. Location, collapse pressure, nearby bottlenecks,
enemies, and player routes should change how attractive or dangerous the same
room type is. Rooms enrich the overarching survival/tactical loop rather than
behaving as disconnected minigames.

## Labyrinth playable roster

Confirmed playable set for The Labyrinth:

1. Wizard
2. Mage
3. Sorcerer
4. Rogue
5. Glavier

There are **five**, not four. Each account is intended to own all five, each
with its own character progression. Their shared character identity and
abilities are defined in [`WORLD_DESIGN.md`](../../WORLD_DESIGN.md); this list
only defines the Labyrinth selection.

The current catalog-driven presentation client additionally exposes ArcherF,
Barde, Chantres, Hammerer, Monk, and Warrior, for eleven selectable character
assets in total. Their presence in the presentation catalog does not by itself
confirm final Labyrinth gameplay roles, kits, lore, or progression.

## Labyrinth-specific presentation and UI application

- The Labyrinth follows the World-01 diegetic/minimal UI philosophy.
- A simple health bar above each character is temporarily allowed as a
  development visualization so multi-client damage and shared Hammerer damage
  behavior can be verified.
- That health bar is not the intended final HP presentation and does not
  replace the shared World-01 direction toward diegetic damage communication.
- Exact collision interpretation of the drawn character geometry remains open;
  the lower trapezoid/body concept is referenced from World Design but is not a
  frozen Labyrinth-specific technical specification.

## Persistent and seasonal progression in The Labyrinth

The shared Magic Coin and seasonal direction is defined in World Design. The
following are Labyrinth-specific or currently scoped here:

- Provisional first-iteration placement rewards: Top 8 receive Magic Coins,
  with increasing rewards toward first place.
- Exact amounts and distribution remain tunable.
- The first Labyrinth implementation does not include account persistence.

## Likely first-iteration focus

The first Labyrinth iteration should prove that these elements create fun
together:

- room-based movement and transitions;
- core combat/encounters using shared World-01 character and weapon concepts;
- temporary power growth, especially through kills;
- shrinking/collapsing connected room topology;
- readable escape pressure;
- forced player convergence and a decisive finish;
- tunable parameters for later balance.

Do not assume every listed room type, all five polished kits, seasonal history,
team play, advanced matchmaking, final art, or every collapse variant belongs
in the first playable iteration.

## First vertical slice: confirmed plan

The first vertical slice proves the multiplayer foundation before implementing
the larger Battle Royale loop.

### Target outcome

- Run one dedicated headless server locally without Docker.
- Connect five separately running graphical clients.
- Each client first sees all five Labyrinth characters in a selection screen.
- Each character occupies one fifth of the 2880 px viewport width (576 px) and
  the upper three quarters of its height (1350 px).
- A shared lower-area button confirms the selected character and joins the
  server.
- The server spawns joined players in one standard room.
- Each player uses WASD to move their own character.
- Movement is server-authoritative and synchronized to all clients.
- No collision is required in this slice, including no room-boundary collision
  requirement.
- No animation state machine, primitive motion animation, actions, final mesh,
  final styling, account persistence, Prediction, or Reconciliation is
  required.
- A simple provisional polygon representation and tinting are allowed.
- Mage is female; Wizard, Sorcerer, Rogue, and Glavier are male.

Shared networking, configuration, content, and dependency boundaries are
specified in [`../../SANDBOX_TECHNICAL.md`](../../SANDBOX_TECHNICAL.md).
Labyrinth-specific implementation behavior is preserved by the code, focused
tests, this document's durable game decisions, and Git history rather than a
separate implementation chronicle.

## Open Labyrinth design questions

- Exact value/range of `A` and initial spawn distribution.
- Exact core-room/core-area selection and final-area size.
- Which collapse variant(s) enter iteration 1.
- Collapse wave timing, warning semantics, and escape constraints.
- Which additional game-specific combat abilities complement the shared
  World-01 abilities.
- Exact DASH collision/slide behavior at walls, doors, and room boundaries in
  the Labyrinth topology.
- Full status-effect interaction in Labyrinth encounters beyond the shared
  KNOCKDOWNED foundation.
- Detailed in-match upgrade system and snowball controls.
- Exact Labyrinth meta-upgrades and Magic Coin economy/reward amounts.
- Persistence design beyond the non-persistent first slice.
- Later matchmaking strength metric/filtering.
