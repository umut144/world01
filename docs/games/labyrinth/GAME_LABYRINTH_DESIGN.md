# The Labyrinth — game-specific design context

> This is a game-specific design document built on the game01 sandbox. It is
> not an active specification for the sandbox. Sandbox-wide contracts live in
> [`../../SANDBOX_VISION.md`](../../SANDBOX_VISION.md) and
> [`../../SANDBOX_TECHNICAL.md`](../../SANDBOX_TECHNICAL.md).

Last updated: 2026-08-29

## Purpose and authority

This is compact continuity context for AI agents after a chat/context switch. It records the solo developer's current intent for The Labyrinth, not a polished pitch or a frozen specification.

Interpretation rules:

- Distinguish **confirmed direction**, **provisional first-iteration values**, and **open questions**.
- Do not silently turn examples or brainstorming into requirements.
- Do not implement anything unless explicitly requested.
- Update this file when the developer makes a durable design decision.

## Normierte Größenentscheidungen

Diese Tabelle enthält bestätigte Referenzgrößen. Einzelne Charaktere, Assets
und UI-Elemente werden relativ zu diesen Referenzen skaliert. Neue
Entscheidungen werden als zusätzliche Zeilen ergänzt.

| Bereich | Referenz | Normwert | Ableitung / Anwendung | Status |
|---|---|---:|---|---|
| Weltmaßstab | Tile / Raumgeometrie | `1 m = 192 px` | Einheitliche Umrechnung zwischen Spielwelt und Darstellung | Bestätigt |
| Kamera | Standard-Viewport | `2880 × 1800 px` | Andere Fensterformate werden über Letterboxing angepasst | Bestätigt |
| Bewegung | Alle Charaktere | `0,8 m/s` | Globaler Slow-Paced-Basiswert | Bestätigt |
| MaxHP | Hammerer | `140 HP` | Referenz für die flächenbasierte HP-Normierung | Bestätigt |
| MaxHP | Alle Charaktere | `Body + optional Feet` | Fläche aus exportierten triangulierten Fill-Meshes; automatisch datengetrieben | Bestätigt |
| HP-Balkenlänge | Mage | Faktor `1,0` | Andere Balken: `MaxHP / MaxHP(Mage)` | Bestätigt, Implementierung offen |
| Pupillenfläche | Jeder Charakter | `26,0 %` | Pupillenfläche relativ zur jeweiligen Augenregion | Bestätigt |
| Pupillenkollision | Hammerer | `35,0 %` | Gemeinsame Kollisionsreferenz für alle Charaktere | Bestätigt |

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
- The Labyrinth is intentionally **slow-paced**. Provisional values for new
  movement, combat, interaction, and timing systems should start from that
  baseline rather than from fast action-game conventions. The current global
  character movement speed is `0.8 m/s`, an 80% reduction from the earlier
  `4.0 m/s` development value, and applies equally to every character.

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
- The current playable implementation remains a single room. The former fixed
  `3 × 3` test grid is retired and does not define current gameplay state;
  multi-room topology returns only with a later explicitly scoped gameplay
  slice.
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

The current catalog-driven presentation client additionally exposes ArcherF,
Barde, Chantres, Hammerer, Monk, and Warrior, for eleven selectable character
assets in total. Their presence in the presentation catalog does not by itself
confirm final gameplay roles, kits, lore, or progression beyond the original
five-character core set.

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

- Every currently catalogued character except Barde has eyes with one shared, round black pupil shape.
- Each pupil covers exactly **26.0%** of its authored eye polygon's area; its radius is derived independently from that eye's geometry.
- Pupil movement uses the Hammerer's **35.0%** pupil-radius collision as its normalization reference. Every other eye uses the same absolute collision radius, capped at its own pupil radius: smaller pupils therefore clip less deeply relative to their size, while larger pupils such as Warrior's clip more deeply. Every closed-region edge remains a collision boundary, but only edges with a visible outline clip the pupil; hidden outline edges may therefore retain a round pupil overlap. The contour remains visually in front, and eyes have no separate visible fill.
- `IJKL` controls the local character's gaze direction: `I` up, `J` left,
  `K` down, and `L` right. Cardinal keys may be combined for diagonal gaze.
  Every character starts looking right; there is no neutral runtime gaze.
  Directional input sets the gaze immediately to one of the eight directions.
  Releasing the keys retains that visible gaze direction.
- Gaze is gameplay-relevant authoritative state. The controlling client sends
  its currently held direction to the server, which immediately updates and
  replicates the retained gaze. Gaze remains independent from movement, body
  facing, and weapon orientation.
- Each character keeps its own eye geometry, eye positions, and eye pivots. Pupil movement is constrained by that eye's geometry rather than the character's overall pivot; rotated or mirrored eye assets must retain the same visible look direction.
- Barde is temporarily excluded from the eye/pupil implementation.

### Movement-dependent body pose

- Each PolyTools character asset declares its authored initial pose as Left,
  Right, Neutral, Top, or Down.
- Authored Left/Right poses follow the character's server-authoritative body
  facing. Moving horizontally in the authored direction keeps the original
  geometry; moving in the opposite direction mirrors the complete character
  presentation horizontally on every client.
- Neutral characters do not flip. While moving, their complete authored head
  subtree shifts slightly in the current movement direction, including
  diagonals, and smoothly returns to its authored neutral position when they
  stop. Top and Down are retained as authored metadata for later use but have
  no runtime behavior yet.
- Pure vertical movement and stopping retain the last horizontal body pose. At
  spawn, the character uses its authored pose.
- Body pose and eye gaze are independent. A character may move and face right
  while its pupils continue looking left; mirroring the body must not mirror
  the gaze in screen space. Likewise, a neutral character's head may follow
  movement while its pupils continue looking independently in another
  direction.
- Body facing and gaze are retained and replicated independently. A late-joining
  client receives their current authoritative values.

## Hammerer: first weapon and attack direction

The Hammerer is the first character used to establish weapons and combat. This
does not confirm complete kits or roles for the other catalogued characters.

Confirmed first-iteration direction:

- The Hammerer carries the existing Hammer weapon authored in PolyTools.
- The Hammerer's `weapon_socket_primary` and the Hammer's `grip_primary` are
  authored attachment frames. They carry orientation as well as position so
  the Hammer rotates around its grip instead of around its visual center.
- The Hammer has an authored polygonal `AttackRegion`, drawn with PolyTools'
  existing Bezier/closed-loop interaction rather than approximated by a circle.
- The Hammer's complete authored Asset has been further enlarged and rebased in
  PolyTools for Slice 15B. The synced geometry, AttackRegion, and every Weapon
  frame share that authored scale; game01 applies no hard-coded size multiplier.
- `grip_primary` remains the Hammer's carried contact aligned to the Hammerer's
  `weapon_socket_primary`. The attack regrips the same hand to a separately
  authored weapon-local `grip_secondary`; neither existing role is renamed.
  The released strike's full-length impact radius is derived from
  `grip_secondary` to `attack_point_primary` after the new Hammer export is
  synced.
- The Hammerer has a weapon-aim angle independent from gaze. While an IJKL
  direction is held, that angle approaches the immediately selected gaze at a
  constant `60°/s`, exactly `1°` per 60 Hz tick, without acceleration or
  braking. At a remaining difference of at most `1°`, it clamps to the exact
  target. Releasing every IJKL key immediately stops weapon rotation at its
  current angle while the eyes retain their last gaze.
- Weapon-aim speed begins equal for all characters but is individually
  configurable. At an exact `180°` difference it continues in the last non-zero
  weapon-turn direction; the initial fallback is clockwise.
- The carried Hammer is displayed opposite the weapon-aim direction. On attack
  release, the actual weapon aim—not gaze—is frozen as the attack direction.
  For example, a Hammer visually carried at `210°` attacks toward `30°` even
  while the Hammerer's eyes continue looking up at `90°`.
- While charging, the Hammer follows the changing weapon aim, is held opposite
  it, and appears smaller. On release it swings across the Hammerer, grows while
  passing over the head, and reaches the ground in the locked attack direction
  at normal scale.
- Outside an attack, the carried Hammer points exactly behind the Hammerer,
  opposite its current weapon-aim direction. Both gaze and weapon aim start
  right, so the Hammer initially appears left/behind. This opposite placement
  is Hammerer-specific rather than a general weapon rule. Other characters may
  present their weapons differently; ArcherF's Bow is intended to be held in
  front in her weapon-aim direction.
- In its carried and charging presentation the Hammer is layered behind the
  Hammerer. At the released swing's overhead apex, the complete Hammer switches
  in front of the Hammerer and remains there through ground impact. Once it
  returns to the carried hand pose, it switches behind the Hammerer again.
- PolyTools `z_index` values express only ordering among parts of the same
  authored Asset. They are not absolute game-world Z coordinates; gameplay
  presentation places each Asset into a contextual layer range while
  preserving its internal authored order.
- Pressing and quickly releasing the attack input produces the basic strike;
  holding it charges the same strike and releasing executes it.
- Charging follows the current weapon aim and caps at `5.0 s`. Three independent
  progress curves run during it:
  - From `0.0–2.0 s`, the held contact moves linearly from `grip_primary` to
    `grip_secondary`, selecting attack reach. Releasing freezes the exact
    intermediate grip; a `1.0 s` release therefore uses half the authored reach
    extension, while reach remains capped at `grip_secondary` from `2.0 s` on.
  - Visible Hammer scale moves linearly from `1.0` to `0.8` during `0.0–2.0 s`,
    then linearly from `0.8` to `0.5` during `2.0–5.0 s`.
  - The visible inward pull begins only at `2.0 s` and grows linearly to at most
    5% of the authored secondary-grip-to-attack-point distance at `5.0 s`.
- Charge scale and inward pull are presentation-only anticipation. They do not
  change the locked attack grip, authoritative impact length, polygonal
  AttackRegion size, or damage area. Swing restores the Hammer to its confirmed
  apex/Impact scale curve, and authoritative Impact always evaluates the
  authored AttackRegion at normal scale.
  Continued IJKL input may keep rotating the weapon aim while Charging.
- The Hammerer retains normal global movement speed during Charging, Swing,
  embedded Impact, and Recovery. Releasing freezes the attack direction,
  charge duration, and current interpolated attack grip.
- The first procedural swing crosses over the Hammerer's body through depth
  rather than circling around it in the screen plane. From a left-carried
  `180°` pose toward a rightward `0°` impact, it is projected like an exact
  `180°` rotation around the screen-space Y axis. For arbitrary attack angles,
  that depth-rotation axis follows the screen-space tangent perpendicular to
  the locked attack direction. PolyTools authors a Component-local projection
  depth in centimeters. During Swing, the visible Hammer uses that depth as a
  closed side silhouette, so the `90°` midpoint shows the authored thickness
  rather than disappearing. This contourless depth form remains active through
  the complete Embedded shake and visually distinguishes the Hammer's attack
  state. Idle, Charging, and Recovery retain the normal flat polygon
  presentation with authored contours. The authored depth is independent of
  component and asset scale; its visible contribution follows
  `|sin(θ)|`, growing naturally to its maximum at `90°` and returning to zero
  at `180°`, while the original 2D geometry follows `|cos(θ)|`.
- Release-to-impact duration remains `1.15 s`. The authoritative impact occurs
  only on the fixed-tick `Swing -> Embedded` transition; the visible depth
  swing can never produce an earlier hit. On impact, the Hammer head remains
  embedded at its world-space impact point for `2.0 s` with a subtle,
  deterministic presentation-only shake of at most `±1.5 cm`. The first custom
  Hammer vertex shader applies this displacement with zero weight at the held
  grip and increasing weight toward the head. Simulation state and the
  authoritative impact point never shake. The shaft keeps rotating toward the
  moving Hammerer so the grip remains visibly held. After the embedded interval,
  Recovery lasts
  exactly `1.0 s`: the Hammer returns smoothly and directly to the current
  behind-the-Hammerer shoulder pose while regripping from the attack contact
  reached at release back to `grip_primary` on `weapon_socket_primary`, instead
  of completing a circular rotation. The Hammer uses scale `1.25` at the
  overhead apex and `1.0` at impact.
- During Embedded Impact, the Hammerer may move freely inside the Hammer's
  authored reach. At maximum reach, only movement farther away from the planted
  head is blocked; tangential and inward movement remain available. The desired
  reach endpoint is slightly above the current `shaft_bevel_bottom` position.
  PolyTools provides the semantic Weapon Guide `reach_limit_primary`; its
  further-scaled authored Hammer position is approximately `(0.0, 0.20 m)`.
  Relative to `attack_point_primary`, the further-enlarged schema-11 Hammer now
  defines a maximum planted-head reach of roughly `2.1674 m`. Its carried
  grip-to-head distance is roughly `1.3674 m`, while the authored secondary
  attack grip produces a full impact radius of roughly `1.7674 m`.
- Space invokes the primary attack during keyboard development. The Xbox right
  trigger invokes the same action when a controller is available. Primary
  pointer/trackpad click is deliberately not bound.
- The first swing uses a focused procedural 3D transform curve and a
  Hammer-specific presentation shader rather than a general-purpose animation
  state machine.
- Simulation and presentation remain separate: the visible Hammer transform is
  not authoritative collision state, while the server evaluates the authored
  attack geometry at the authoritative impact pose.

Confirmed authoring aids:

- `attack_point_primary` is placed at the Hammer head's authored center and
  gives the swing and AttackRegion an explicit alignment reference.
- `grip_secondary` is authored on the Hammer shaft as the attack contact. The
  impact radius is derived from this frame to `attack_point_primary`; no
  duplicate impact-distance balance value is introduced for this iteration.

The first combat-value slice introduces server-authoritative current/max HP.
Each character's maximum HP is derived at runtime from the summed area of its
current triangulated PolyTools fill meshes for the semantic `body` component
and, when present, the semantic `feet` component. The Hammerer is the
normalization reference at `140 HP`; all other values use the same
HP-per-square-meter ratio. This is intentionally manifest-driven: resizing or
reauthoring a character's `body`/`feet` geometry automatically changes its
derived MaxHP after the next asset sync/export, without a second HP table.
Contours,
outlines, eyes, clothing, weapons, and auxiliary overlays such as Warrior's
`body_side_*` components are excluded. No MaxHP value is duplicated in
`design.toml`. A simple health bar above each character is deliberately allowed
as a temporary development visualization so multi-client damage and
charged-damage behavior can be verified. It is not the intended final HP
presentation and does not replace the diegetic/minimal-UI direction below.

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

Shared networking, configuration, content, and dependency boundaries are
specified in [`../../SANDBOX_TECHNICAL.md`](../../SANDBOX_TECHNICAL.md).
Labyrinth-specific implementation behavior is preserved by the code, focused
tests, this document's durable game decisions, and Git history rather than a
separate implementation chronicle.

## Open design questions

- Exact value/range of `A` and initial spawn distribution.
- Exact core-room/core-area selection and final-area size.
- Which collapse variant(s) enter iteration 1.
- Collapse wave timing, warning semantics, and escape constraints.
- Exact combat and abilities of all five characters.
- Exact AttackRegion placement/overlap rule, base damage, and charged-damage
  curve. Hammer impact distance is authored by `grip_secondary` and
  `attack_point_primary`; current action timings remain balanceable.
- Detailed in-match upgrade system and snowball controls.
- Exact meta-upgrades and Magic Coin economy/reward amounts.
- Persistence design beyond the non-persistent first slice.
- Later matchmaking strength metric/filtering.
- Exact presentation/collision interpretation of the drawn character geometry.
