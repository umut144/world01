# The Labyrinth — persistent AI design context

Last updated: 2026-08-14

## Purpose and authority

This is compact continuity context for AI agents after a chat/context switch. It records the solo developer's current intent, not a polished pitch or a frozen specification.

Interpretation rules:

- Distinguish **confirmed direction**, **provisional first-iteration values**, and **open questions**.
- Do not silently turn examples or brainstorming into requirements.
- Do not implement anything unless explicitly requested.
- Update this file when the developer makes a durable design decision.

## Identity

- Project/workspace: `game01`
- Codename/game name: **The Labyrinth**
- Theme title: **“Secrets, Room's & Travels'”**. The apostrophes are deliberately incorrect/unusual for marketing and must not be “corrected” automatically.
- World/theme: **High Fantasy**
- The setting is part of a larger world intended to be used by a later MMORPG.
- That world currently contains 23 classes. The Labyrinth initially exposes only five of them.

## Core game

- 2D top-down, room-based Dungeon Battle Royale with Roguelite-style persistent meta progression.
- 64 players enter an initially regular `A × A` labyrinth.
- First shipped/developed mode: **Solo only**.
- Future mode: teams of four. Do not design its detailed revive/balance rules yet, but do not architecturally exclude team play.
- Players can die to other players, mobs, hazards, or the collapse of a room.
- The winner is the final surviving solo player; a future team mode ends with one surviving team.
- The central match experience is: explore rooms, gain power (especially through kills), react tactically to collapse, traverse a changing topology, fight over increasingly limited space, and finish in a small surviving area.

## Match progression and desired snowballing

- Kills and other match activities make a character substantially stronger for the current match.
- In-match progression is temporary and intentionally stronger than meta progression.
- Snowballing is a deliberate design choice. It should matter without becoming uncontrollable.
- Meta progression provides many small optimizations and slightly better starting conditions. Accumulated micro-advantages are intentionally real and may increase a player's win rate.
- Exact balance is not first-iteration scope. The first iteration needs identifiable/tunable parameters (“Stellschrauben”); balance should emerge iteratively across seasons, match data, and community feedback.
- Future matchmaking may increasingly group similarly strong/progressed players, especially late in a season. Population, queue times, brackets, and exact filtering are future operational problems.

## Labyrinth geometry and camera

- Rooms use four-way cardinal connectivity: north/east/south/west (N/O/S/W in German notation).
- Standard rooms normally have up to four exits, with explicit exceptions possible.
- Initial topology is an `A × A` box/grid. Collapse quickly turns it into a unique shape.
- Standard design viewport: **2880 × 1800 px**.
- Other displays must be scaled/framed so they reveal neither more nor less relevant game world.
- Scale: **1 m = 192 px**.
- Tile size: **1 m = 192 px**.
- The current camera test uses one **50 × 50 tile** room; room dimensions are
  independent from the camera's visible tile count.
- The camera view is configured separately (`view_width_tiles` /
  `view_height_tiles`) and letterboxes any unused native-window area.
- Larger logical spaces can be composed from multiple room units, while each
  room's configured tile dimensions remain independent and are shown whole.
- The camera follows and centers the complete active room; exceptions and
  special rooms are possible.
- Room dimensions are configured before a match and may vary between rooms;
  every configured room is shown completely and centered in its own aspect-
  matched camera frame.
- The current camera test intentionally has no neighbor-room or transition
  presentation; the character starts at the center of the single room.
- Room transitions are intended to be completely seamless at first, without a
  visible door, threshold, or transition moment.

## Collapse: essential design

- The labyrinth progressively collapses toward a core room or connected core area chosen deterministically-randomly from the match seed/start state.
- Collapse behaves broadly like a shrinking radius but should produce an organic room boundary.
- A small connected group of rooms may remain as the final arena instead of collapsing to exactly one room. It must be small enough to force the surviving players to meet and finish.
- A room about to collapse shakes/bebt. Players must escape through an available cardinal exit.
- Adjacent rooms may also already be threatened, forcing continued movement until a safe region is reached.
- Remaining in a room when it collapses is fatal and ends that player's match.
- Route replanning is primarily **tactical**, not strategic.

### Graph invariant

Model rooms as nodes and doors/tunnels as edges. After each normal collapse step:

- Every remaining room must have a path to the surviving core/core area.
- The surviving room graph must therefore stay connected to that core.
- A bottleneck room may be an articulation point and cannot be removed while its removal would isolate surviving areas.
- Two large areas can instead have one or more deliberate tunnel/connection edges. Redundant tunnels allow routes to disappear gradually without immediate isolation.

Portals are an acceptable creative fallback if a generated/collapsing state would isolate players (for example, a temporary escape portal appearing in an isolated room). The primary algorithm need not be mathematically perfect if failure is safely detected and turned into a coherent game event.

### Multiple collapse variants

Do not assume one universal collapse algorithm. The intended direction is to co-design several fun collapse variants/rule sets and draw one per match. Examples discussed, not all confirmed for iteration 1:

- conventional outside-to-core collapse;
- directionally biased collapse;
- organic/jagged borders;
- protected bottlenecks;
- large regions linked by one room;
- large regions linked by one or several tunnels;
- occasional valid inner collapse;
- different final-area topologies.

The variants should share clear invariants while creating different match character.

### Collapse communication

The developer has multiple ideas to refine later. Current direction is diegetic communication:

- shaking rooms/walls;
- a central floor crack extending toward cardinal directions;
- longer cracks may warn across several neighboring rooms in a direction;
- the strongest collapse direction may shake more;
- more falling debris/VFX on the threatened side;
- directional and escalating sound cues.

Exact warning timing, cascades, and fairness rules will be designed iteratively when this system is in scope.

### Offline validation intention

Before relying on a collapse variant in game, visualize generated states as simple raster/PNG grid images and inspect them manually. The developer wants to review images and report conflicts. Tests should make connectivity failures, islands, protected regions, bottlenecks, and odd collapse shapes easy to see.

## Room content (“spice”, not isolated minigames)

Known room categories include:

- quiet/empty exploration and discovery rooms;
- shops;
- bot/mob combat rooms;
- timed puzzle challenges;
- hazard rooms whose hazard can be disabled via a room button;
- bonus rooms granting a free item or upgrade;
- further room types later.

Room value is contextual. Location, collapse pressure, nearby bottlenecks, enemies, and player routes should change how attractive or dangerous the same room type is. Rooms enrich the overarching survival/tactical loop rather than behaving as disconnected minigames.

## Five playable characters/classes

Confirmed playable set for The Labyrinth:

1. Wizard
2. Mage
3. Sorcerer
4. Rogue
5. Glavier

There are **five**, not four. Each account is intended to own all five, each with its own character progression. The developer already has concrete concepts for them; do not invent final roles, kits, lore, or visual replacements without discussion.

Reference drawings:

- `reference_drawings/5 chars/wizard01.JPG`
- `reference_drawings/5 chars/wizard02.JPG`
- `reference_drawings/5 chars/mage01.JPG`
- `reference_drawings/5 chars/mage02.JPG`
- `reference_drawings/5 chars/sorcerer01.JPG`
- `reference_drawings/5 chars/sorcerer02.JPG`
- `reference_drawings/5 chars/rogue01.JPG`
- `reference_drawings/5 chars/rogue02.JPG`
- `reference_drawings/5 chars/glavier01.JPG`
- `reference_drawings/5 chars/glavier02.JPG`

## Visual language

- Art direction: **2D polygon style**.
- Characters are intentionally archetypal and minimalist.
- Designs should be constructible from a small number of simple closed polygons.
- Example from the developer: Wizard is essentially three major geometries—hat, head, body—excluding eyes and mouth.
- Character readability should come from strong primary silhouette and a few high-signal geometric identifiers, not surface detail.
- The drawings demonstrate related but distinguishable silhouettes: characteristic hat/hood contours, face openings/eye treatment, class symbols, and Glavier's prominent weapon/form motifs.
- Preserve the charming, readable abstraction; do not “improve” it into anatomically complex or detail-heavy fantasy art by default.

### Eyes and gaze

- Every currently playable character except Barde has eyes with one shared, round black pupil shape.
- `IJKL` controls the local character's gaze direction: `I` up, `J` left, `K` down, and `L` right. This is presentation-only and does not change movement.
- Each character keeps its own eye geometry, eye positions, and eye pivots. The complete pupil circle must remain inside that eye's geometry rather than being constrained by the character's overall pivot; rotated or mirrored eye assets must retain the same visible look direction.
- Barde is temporarily excluded from the eye/pupil implementation.

## Diegetic / minimal UI philosophy

- The developer categorically dislikes conventional camera-lens HUD: text, numbers, floating bars, and overlays should appear only when absolutely necessary or genuinely useful.
- HP should not default to a health bar. Damage can appear as cracks, injuries, or visible degradation on the character mesh/polygons.
- World danger should be communicated through geometry, animation, shader/material state, VFX, and sound.
- Magic Coins/meta screens are an example where explicit UI may be useful.
- Related concept reference: `reference_drawings/design_concepts/hit_region_is_bottom_trapez_and_hp_example_for_UIless_visualization.JPG`.
- That drawing also proposes a lower trapezoid/body region as the gameplay hit region. Treat exact collision implementation as a concept to confirm, not yet a frozen technical specification.

## Persistent and seasonal progression

- Currency: **Magic Coins**, stored persistently on player accounts.
- Magic Coins can improve future starting conditions and/or purchase character upgrades during the current season.
- Coins can be found in match chests; found coins become persistent immediately and remain even after early death.
- Provisional first-iteration placement rewards: Top 8 receive Magic Coins, with increasing rewards toward first place. Exact amounts/distribution remain tunable.
- Seasons are envisioned as roughly three months.
- Gameplay-affecting seasonal progression may reset completely each season and be rebuilt alongside new content.
- Desired future feature: a beautiful **Season History Book** retaining emotional ownership—history, statistics, achievements/trophies, victories, and other memories—after power resets.
- Season History Book and its extra art are explicitly not first-iteration scope. Once live, there would be a season to develop/refine it.

## Development philosophy (high priority)

The developer is a solo developer and follows an iterative philosophy described metaphorically as “SLAM transferred to game design”: while building, discover detailed design questions and close them progressively.

- First the coarse structure, then the fine detail; always think iteratively.
- Optimize scope around mechanics with the greatest leverage, where leverage means **fun**.
- Simulation/gameplay and presentation/art are separate iteration layers.
- Simulation iteration 1 implements only the most important, high-fun-leverage game principles.
- Art, shaders, VFX, and sound initially receive only a first sufficient iteration.
- Expand and refine both layers season by season.
- Build tunable systems before trying to discover final balance values.
- Do not spend current scope on future problems merely because they can already be imagined.
- Do not architecturally block plausible future needs (notably four-player teams), but avoid implementing speculative systems.
- Creative fallbacks are valid engineering/game-design tools; correctness can include detecting a bad state and converting it into a coherent event.

## Likely first-iteration focus (direction, not an implementation order)

The foundation should prove that these elements create fun together:

- room-based movement and transitions;
- core combat/encounters;
- temporary power growth, especially through kills;
- shrinking/collapsing connected room topology;
- readable escape pressure;
- forced player convergence and a decisive finish;
- tunable parameters for later balance.

Do not assume every listed room type, all five polished kits, seasonal history, team play, advanced matchmaking, final art, or every collapse variant belongs in the first playable iteration.

## First vertical slice: confirmed plan

The first vertical slice proves the multiplayer foundation before implementing the larger Battle Royale loop.

### Target outcome

- Run one dedicated headless server locally without Docker.
- Connect five separately running graphical clients.
- Each client first sees all five characters in a selection screen.
- Each character occupies one fifth of the 2880 px viewport width (576 px) and the upper three quarters of its height (1350 px).
- A shared lower-area button confirms the selected character and joins the server.
- The server spawns joined players in one standard room.
- Each player uses WASD to move their own character.
- Movement is server-authoritative and synchronized to all clients.
- No collision is required in this slice, including no room-boundary collision requirement.
- No animation state machine, primitive motion animation, actions, final mesh, final styling, account persistence, Prediction, or Reconciliation is required.
- A simple provisional polygon representation and tinting are allowed.
- Mage is female; Wizard, Sorcerer, Rogue, and Glavier are male.

The technical realization, networking model, configuration boundary, and crate responsibilities for this slice are specified in `ARCHITECTURE.md`.

## Open design questions

- Exact value/range of `A` and initial spawn distribution.
- Exact core-room/core-area selection and final-area size.
- Which collapse variant(s) enter iteration 1.
- Collapse wave timing, warning semantics, and escape constraints.
- Exact combat and abilities of all five characters.
- Detailed in-match upgrade system and snowball controls.
- Exact meta-upgrades and Magic Coin economy/reward amounts.
- Persistence design beyond the non-persistent first slice.
- Later matchmaking strength metric/filtering.
- Exact presentation/collision interpretation of the drawn character geometry.
