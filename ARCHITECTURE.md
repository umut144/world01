# The Labyrinth — persistent technical architecture

Last updated: 2026-08-28

## Purpose and authority

This is the technical source of truth for architecture, dependency direction, technology choices, system boundaries, and implementation-slice structure.

- `GAME_DESIGN.md` owns player-facing rules, experience, scope, and art direction.
- `ARCHITECTURE.md` owns how confirmed requirements are represented and separated technically.
- `SLICES_AND_TASKS.md` owns the compact tabular slice status and task overview.
- `AGENTS.md` owns code-agent workflow, validation, editing, and Git rules.
- Do not introduce speculative infrastructure for unconfirmed future features.
- Preserve the distinction between confirmed architecture and open implementation details.

## Current technology baseline

- Language: stable Rust.
- Engine: Bevy 0.19 APIs exclusively.
- Networking: Lightyear 0.28.
- First deployment target: local native processes without Docker.
- First mode: one dedicated headless server and five separately running graphical clients.
- Server simulation frequency for the first slice: configurable fixed **30 ticks per second**.

## Authority and replication model

- The dedicated server is authoritative over gameplay state.
- Clients send character selection and movement intent, never authoritative positions.
- The server validates joins, owns player entities, runs simulation, and mutates authoritative player `Position`s.
- The server replicates entity lifecycle, character identity, ownership data, and Position snapshots to clients.
- Bevy `Transform` is presentation state derived from `Position` on graphical clients; it is neither authoritative nor replicated.
- A client may control only the player entity assigned to its connection.
- Disconnecting removes the server-owned player and replicates its despawn.
- A later-joining client must receive the already existing replicated players.
- Prediction and Reconciliation are explicitly excluded from the first slice.
- Prediction, reconciliation, snapshot interpolation, and render interpolation enter in later phases of Slice 2; Phase 1 only establishes their required state boundary.
- Slice 2 Phase 3 predicts only the owning client's `Position`; the server remains authoritative and Lightyear reconciles mismatches through rollback and tick replay.

## Dependency and responsibility boundaries

World data, configuration, simulation, networking, orchestration, input, and presentation are separate concerns.

### `world_data`

Owns shared protocol-neutral domain data:

- `CharacterId`: validated catalog key for a character asset (currently
  ArcherF, Barde, Chantres, Glavier, Hammerer, Mage, Monk, Rogue, Sorcerer,
  Warrior, and Wizard);
- player identity, selected character, and ownership markers;
- standard-room and spawn data;
- the fixed initial `3 × 3` room-grid coordinate mapping and authoritative
  `RoomId` assignment;
- movement intent data passed into simulation;
- combined tick-bound player input containing movement and gaze intent;
- authoritative current movement direction, retained horizontal body facing,
  and retained gaze direction;
- authoritative two-dimensional `Position` in meters;
- replicated gameplay components that are not transport-specific.

Slice 1 used `Transform` directly as provisional movement state. Slice 2 Phase 1 supersedes that choice with the protocol-neutral `Position`; graphical transforms no longer belong to shared world state.

Implemented shared data uses transport-neutral scalar identifiers and
coordinates: `PlayerId`, `PlayerOwner`, `SelectedCharacter`, `Player`, `RoomId`,
`StandardRoom`, `SpawnPoint`, `PlayerInput`, `MovementIntent`, `GazeIntent`,
`MovementDirection`, `BodyFacing`, `GazeDirection`, and `HammerAttackState`.
Network-specific
connection types stay outside `world_data`.

### `configs`

Owns explicitly requested, human-editable game-design parameters:

- dedicated workspace crate/directory named `configs`;
- typed configuration boundary;
- initially backed by `design.toml`;
- exposes the current global `0.8 m/s` movement speed and 60 Hz simulation
  cadence, plus the explicitly requested Hammer charge/swing/recovery timings
  and charging movement multiplier;

Later values such as mass, MaxHP, or attack values are added only when the developer explicitly requests them and the corresponding behavior enters scope. Do not expose every internal constant merely because simulation uses it.

### `simulation`

Owns transport-, input-device-, and presentation-independent game rules:

- consumes explicit movement intent;
- consumes explicit gaze intent;
- applies current movement rules to authoritative state;
- runs when scheduled by fixed-step orchestration but contains no tick-loop orchestration itself;
- does not read keyboard input;
- does not send or receive network messages;
- does not render, animate, play audio, or manage UI.

The simulation mutates protocol-neutral `Position`, `MovementDirection`,
`BodyFacing`, and `GazeDirection`, never presentation `Transform`. Every tick,
finite movement intent becomes a unit-clamped current `MovementDirection` and
zero or invalid intent becomes zero. Horizontal movement intent updates
retained Left/Right body facing; zero horizontal intent preserves it. A finite
non-zero gaze intent is normalized and replaces the retained gaze; zero or
invalid gaze preserves the previous direction. A mass/velocity movement model
will be required soon and does not need to follow real-world physics. Keep the
simulation interface and network/input flow suitable for adding explicit
velocity and mass without rewriting those outer layers.

Collision is not required in the first slice, including room-boundary collision.

Movement is represented by `MovementStep`, constructed from the typed design
configuration. It supplies explicit speed and step duration to the simulation
system, clamps intent to unit length, rejects non-finite intent, and directly
updates `Position` without owning fixed-tick scheduling. The current global
speed is `0.8 m/s` for every character. A `HammerAttackRules` resource applies
the configured Charging multiplier to both authoritative displacement and
replicated `MovementDirection`; the initial multiplier is `0.0`, so Charging
locks movement without rewriting input state.

### `network`

Owns Lightyear-specific concerns:

- transport configuration;
- protocol and message registration;
- connection-to-player ownership mapping;
- reliable character-selection/join messages;
- tick-bound native movement-input buffering, redundancy, and ownership validation;
- spawn, despawn, component, and Position replication.

Networking transports intent and replicated state; it does not own movement rules.

The `game01-network` crate exposes separate `client` and `server` Cargo features. Both applications disable its default features and select only their respective feature. Both sides retain Lightyear replication, native input, Netcode/UDP, prediction, and interpolation because the server registers predicted/interpolated `Position` state and assigns `PredictionTarget`/`InterpolationTarget` to recipients. The dedicated server deliberately excludes Lightyear's `client` feature and all local client connection, input-collection, prediction-presentation, and interpolation-presentation systems.

Slice 2 Phase 2 uses Lightyear's native input pipeline instead of a custom
movement message. The current pipeline carries one combined `PlayerInput` with
movement and gaze intent. The client samples hardware state into a local
resource during normal frame input collection, writes that state to its
controlled entity in Lightyear's `FixedPreUpdate` input stage, and sends
redundant tick-addressed history. The server validates each input target against
`ControlledBy`, lets Lightyear select the current tick's `ActionState`, and
adapts that state to explicit `MovementIntent` and `GazeIntent` components
before simulation. This is a direct entity-local query with no
connection-to-player scan.

Slice 2 Phase 3 registers `Position` for Lightyear prediction and assigns a `PredictionTarget` only to the controlling peer. Once its input timeline is synchronized, that client adapts the rollback-aware native `ActionState` to `MovementIntent` and runs the same `MovementStep` plus `move_players` system used by the server. Confirmed server positions remain the reconciliation authority. Remote players are not predicted in this phase.

Phase 3 uses Lightyear UDP + Netcode on loopback address `127.0.0.1:5000`. Clients bind an operating-system-selected local UDP port and receive a non-zero Netcode client ID from their first process argument (falling back to the process ID). The server admits at most five unique identities and removes connection-registry entries on disconnect.

Phase 4 adds one ordered-reliable client-to-server `JoinRequest` carrying a
catalog-backed `CharacterId`. Connection begins only after local confirmation.
The server validates the ID against the synced character catalog, rejects
repeated joins per Netcode identity, allocates a stable `PlayerId`, and chooses
one of five separated spawn positions. Slice 2 Phase 1 now replicates
`PlayerId`, `PlayerOwner`, `SelectedCharacter`, `Position`, entity spawn, and
despawn to all clients. Since Slice 2 Phase 6, snapshot publication follows
the configured 60 Hz Lightyear tick.

### `server`

Owns headless authoritative app orchestration:

- starts the local server transport;
- accepts and tracks connections;
- validates join requests;
- allocates player identities and spawn positions;
- schedules the fixed-step simulation;
- publishes replicated state;
- handles disconnect cleanup.

The server must not depend on rendering, audio, windowing, graphical UI, or input-device features.

### `client`

Owns non-authoritative local interaction and presentation:

- character-selection screen;
- connection lifecycle presentation;
- keyboard input collection and conversion to movement intent;
- gamepad input collection and conversion to movement intent;
- rendering provisional characters and the standard room;
- presenting replicated server state.
- deriving visible Bevy `Transform`s from replicated or later predicted/interpolated `Position`s.
- running shared movement simulation only for its controlled predicted player after input-timeline synchronization.

Client presentation state is never gameplay authority.

The initial controller boundary uses the direct `gilrs` native gamepad backend
on the graphical client only because Bevy 0.19.1's matching official backend
crate is unavailable. The connected Xbox controller's left analog stick is the
prioritized movement source; a neutral stick leaves WASD as a development and
accessibility fallback. Both input devices are reduced to the same local
`MovementIntent` before Lightyear's native tick input path, so the server and
simulation remain unaware of input-device choice.
The left stick uses a radial 0.15 deadzone, then linearly maps the remaining
physical travel to the `0.0..=1.0` intent magnitude. Simulation preserves that
magnitude while clamping only values above one, so partial stick deflection
produces proportionally slower movement.
Controller-driven selection, actions, rumble, rebinding, and multi-controller
assignment are outside this slice.

### PolyTools runtime-asset boundary

- PolyTools Runtime Export is the canonical interchange format for character
  presentations; the synced catalog determines which character IDs are valid.
- The game consumes imported copies under `game01/assets/`; it does not read
  the sibling PolyTools project at runtime and does not retain a
  `polytools/world01` path prefix in its asset tree.
- `./scripts/sync_polytools_characters.sh` copies the authoritative
  `catalog.json` and all character packages named by it from PolyTools into
  `assets/characters/`. It validates the catalog, imports every advertised
  character package, the explicitly required Hammer weapon package, and their
  referenced Symbol packages, then replaces the generated destination
  atomically so stale components cannot survive a re-export. The source World
  directory can be overridden with `POLYTOOLS_WORLD_DIR`. The sync requires
  PolyTools Runtime Manifest schema 10 and validates the Hammer's required grip,
  attack point, and polygonal AttackRegion contract before importing it. The
  client accepts schema 10 character packages while temporarily retaining
  loading compatibility with imported schema 5 through 9 packages.
- Client-only loading validates each imported manifest and turns its already
  triangulated fill, closed-region, and contour-stroke geometry into Bevy 2D
  mesh presentation entities. Component transforms, hierarchy, and `z_index`
  remain presentation data; PolyTools geometry never enters simulation,
  networking, or replicated world state.
- The current export contract contains geometry but no material/color data.
  The first integration applies a small client-owned temporary palette by
  character and component name. A future material export is a separate
  PolyTools contract decision.

The Hammerer weapon slices extend this boundary without introducing a general
inventory system:

- PolyTools remains authoritative for the Hammer's visible geometry,
  attachment frames, and polygonal AttackRegion.
- Slice 14 directly associates the catalogued Hammerer with the catalogued
  Hammer. This fixed association is derived from character identity on every
  client and requires no replicated equipment state yet.
- The import/sync path expands from characters plus referenced Symbols to the
  explicitly required `weapons` package. It must not import unrelated catalog
  categories speculatively.
- Attachment frames are not ordinary Bezier Guides. They are oriented local
  frames with position and rotation, scoped to and inheriting the transform of
  an authored component (or an authored group when that scope is supported).
  PolyTools presents them with a transform-like gizmo rather than as an
  unoriented point. Parent scale is inherited; no independent non-uniform
  attachment scale is required for the first contract.
- The initial Weapon Guide menu exposes `weapon_socket_primary` for a character,
  `grip_primary` for a weapon, and the planned `attack_point_primary` alignment
  frame. The intended editor route is `Guide -> Weapon -> ...` from the selected
  component or group.
- `weapon_socket_primary` is authored under the Hammerer's body hierarchy.
  `grip_primary` is authored at the held part of the Hammer shaft. The planned
  `attack_point_primary` is authored at the Hammer head, initially at the
  geometric center of the `head` group.
- PolyTools Regions are non-rendering semantic geometry distinct from visible
  Components and from spine-like Guides. The immediate required role is
  `AttackRegion`; additional roles such as physical collision or damage-
  receiving regions enter only with corresponding gameplay scope.
- A Region reuses PolyTools' Bezier topology and Closed Loop drawing workflow,
  but the first runtime contract accepts exactly one valid, closed,
  non-degenerate boundary. Exported gameplay geometry is metric and carries an
  explicit semantic role; simulation must not infer collision from a visible
  fill or contour mesh.
- PolyTools Runtime Manifest schema 9 introduced top-level Asset-local
  `attachment_frames` and triangulated semantic `regions` arrays without
  assigning new meaning to schema 8 fields. Runtime Manifest schema 10 extends
  those frames with `reach_limit_primary`. Hammerer and Hammer schema-10 data
  are authored, exported, validated, synced, and imported into typed client-side
  presentation data. The Hammer contract requires exactly one grip, one attack
  point, one reach limit, and one triangulated AttackRegion; Hammerer requires
  exactly one weapon socket. The Region is retained as semantic data and is not
  rendered.
- PolyTools World schema 54 and Runtime Manifest schema 10 add the
  transform-based Weapon Guide role `reach_limit_primary`. The editor route,
  Component/Group scope, Inspector, Canvas gizmo, persistence, Scale Rebase,
  validation, and export support are implemented. The authored Hammer frame at
  `(0.0, 0.2 m)` is synced into game01. The schema-10 sync and typed importer
  require exactly one reach limit for Hammer; the maximum planted-head reach is
  derived from its distance to `attack_point_primary` rather than from visible
  Component geometry.

The Hammer visible entity is attached by aligning `grip_primary` with
`weapon_socket_primary`. The client derives its local translation, rotation,
scale animation, and presentation layer from the authored frames plus the
semantic attack state. These transforms never become server authority.
For Slice 14, only Hammerer receives the fixed Hammer association. Its
presentation hierarchy places a weapon-pose root at the authored socket and an
inverse grip transform beneath it, so the visible geometry aligns exactly while
future pose rotation and scale occur around the grip. The hierarchy inherits
the character's outer transform and orientation. PolyTools `z_index` is
asset-local semantic ordering, not a global Bevy Z coordinate: the client
currently maps adjacent authored values to `0.01`-spaced local layers beneath
an Asset root. That root places the complete ordered range relative to other
Assets. In the carried pose, the Hammer root is offset so its highest local
visual layer remains below the Hammerer's lowest local visual layer. No attack
state or gameplay equipment component is introduced in Slice 14.

### PolyTools root Asset scale and Rebase slice

PolyTools World schema 53 provides one explicit positive, uniform root Asset
Scale in the root Asset Inspector. It is an authoring
transform around the Asset Pivot, defaults to `1`, and previews the complete
Asset consistently: visible Components, nested Component/Group hierarchy,
References, ordinary Guides, Weapon attachment frames, semantic Regions, and
their Canvas selection/gizmo geometry. Non-uniform root scaling and root-level
mirroring are outside this focused slice; those remain explicit Component-level
operations.

Root Asset Scale is not permitted to leak into Runtime Export. A dedicated
atomic Rebase bakes the factor into all owned coordinate-bearing authoring data,
resets root Scale exactly to `1`, and preserves the complete visible and
semantic result around the unchanged Asset Pivot. In particular, grip,
attack-point, socket, and Region relationships must remain identical after the
Rebase. References retain coherent instance placement without modifying their
source Assets. Existing Component Scale Rebase must still work before or after
the root operation.

The editor reference-image metadata is a calibration backdrop rather than
runtime Asset content and remains unchanged, allowing the scaled drawing to be
judged against it. To keep this slice bounded, a non-default authored Motion
document blocks root Rebase unless its scale-sensitive coordinates are covered
by an exact preservation path and focused tests; motion data is never adjusted
partially or silently.

The operation is one undoable change. Unsupported or invalid content blocks the
entire Rebase with a precise reason; there is no partial bake. Derived Mesh, UV,
SDF, and Runtime Export artifacts are invalidated and rebuilt from the rebased
canonical source rather than numerically scaling stale bakes. Runtime Export
rejects a non-unit root Asset Scale and directs the author to Rebase.

Automated PolyTools acceptance covers nested and rotated hierarchy, primitives
and Bézier geometry, semantic Regions, Component- and Group-scoped
Guides/Weapon frames, References, Asset Pivot behavior, save/load round trips,
undo/redo, post-Rebase Component Scale Rebase, and Runtime Export rejection
before Rebase. The enlarged/rebased Hammer export has been synced and visually
accepted in game with correct size, grip/socket alignment, grip pivot,
AttackRegion, attack point, and carried-behind-character layering.

### Hammer attack and combat-state boundary

- `GazeDirection` is reused as the Hammerer's aim direction for the first
  attack; no separate aim protocol is introduced. Before the first non-zero
  gaze, presentation retains the authored resting pose. Afterwards, Idle and
  Charging rotate the authored grip-to-attack-point vector exactly opposite
  the retained gaze.
- Hammer layer switching is presentation-only. Carried and Charging keep the
  complete Hammer behind the Hammerer. At the procedural swing's overhead apex
  the Hammer root moves above the Hammerer's complete asset-local layer range,
  remains in front through Impact, and returns behind only when Recovery
  restores the carried hand pose. PolyTools Component order inside each Asset
  is never rewritten for this transition.
- Attack input travels through the existing tick-bound native input path. A
  quick press/release and a held charge are the same action with different held
  durations. The client merges Space and Xbox right trigger into one boolean
  `AttackIntent`; primary pointer/trackpad click is unbound. The native input
  history gives prediction and authoritative server processing the same
  press/release sequence.
- The initial direct gaze assignment is provisional. Slice 15A replaces it with
  a deterministic, server-authoritative and predicted angular-motion model.
  Its state must be suitable for character-specific turning parameters without
  coupling input devices to simulation. Directional input is confirmed to
  represent an absolute target angle. Simulation integrates current angle and
  angular velocity using character-specific maximum speed, acceleration, and
  braking parameters, then derives `GazeDirection` from that angle.
- Slice 15B makes Charging follow the evolving inertial gaze with a
  presentation-level Hammer lag and retains the 300-tick (`5.0 s` at 60 Hz)
  cap; release freezes the attack direction for the resulting swing. That
  refinement removes the current Charging movement lock so character movement
  remains at normal global speed.
- Shared simulation owns the replicated/predicted `HammerAttackState`. Slice
  15B extends its deterministic Idle, Charging, Swing/Impact, Embedded,
  Recovery lifecycle.
  The provisional Swing duration is 27 ticks (`0.45 s`) and Embedded lasts 120
  ticks (`2.0 s`). This does not establish a reusable animation state machine.
- The owner predicts the same deterministic attack transition where needed;
  replicated semantic state lets remote and late-joining clients derive the
  corresponding Hammer pose. Visible weapon transforms are not replicated.
- Slice 15B updates client presentation's procedural clockwise curve. Charge
  progress linearly scales the Hammer from `1.0` to
  `0.8` and pulls it inward by at most 5% of the authored grip-to-attack-point
  distance. The swing reaches scale `1.25` at the overhead apex and `1.0` at
  ground impact. It samples fixed-tick overstep for frame-smooth transforms.
- The planned Embedded Impact requires a presentation anchor containing the
  world-space Hammer-head point at impact. The head remains at that point for
  `2.0 s`, with
  optional bounded `±1.5 cm` shake, while the shaft rotates so its grip stays
  connected to the moving Hammerer. Recovery interpolates directly from this
  constrained pose to the current carried pose; it must not continue the
  former circular path. The authoritative future AttackRegion still uses
  simulation state rather than this presentation anchor.
- Embedded movement is constrained by a circle around the planted head. Inward
  and tangential displacement remain unchanged; only an outward displacement
  crossing the authored maximum reach is projected to the boundary. The current
  Hammer manifest exposes the intended endpoint only as the visual Component
  pivot named `shaft_bevel_bottom` at asset position `(0, 0.03119038)`. A
  semantic Weapon Guide at that position is preferred before shared simulation
  imports it as authoritative reach data.
- `attack_point_primary` is the preferred authored visual alignment reference
  for the Hammer head and AttackRegion. The exact authoritative relationship
  between that point, the grip/socket pair, and a separately configurable
  impact distance is deliberately still open.
- Server-authoritative current/max HP enters before damaging attacks. Clients
  receive the replicated values and temporarily render a simple bar above each
  character solely for multiplayer combat validation.
- At the impact tick, simulation places and evaluates the authored polygonal
  AttackRegion from authoritative player position, locked attack direction,
  and the confirmed impact-placement rule. It does not collide the visible,
  continuously animated render mesh.
- Max HP, base damage, charged-damage curve, impact distance, and other
  explicitly requested balance parameters belong in `configs` when their
  corresponding behavior is implemented. Charge cap, action timings, and the
  Charging movement multiplier are already configured; purely visual easing
  and overswing scale remain presentation constants.

The 2880 × 1800 design window remains a reference size independent from the native window size. The first local client window starts at 2880 × 1800 logical units with the same 16:10 aspect ratio, allowing macOS Retina scaling while matching the full design viewport out of the box. The window is resizable and supports macOS fullscreen; the client derives a camera viewport matching the active room's aspect and centers it in every physical window size. Remaining area is black letterboxing, so resizing or fullscreen never distorts the room or reveals part of another room through the camera frame.

The client now has a local presentation state boundary with `CharacterSelection` and `InGame` states. Phase 1 enables Bevy UI rendering and uses the state boundary to cleanly remove selection entities on transition; actual button-driven layout migration is Phase 2.

The shared authored spatial reference uses `1 m` terrain Tiles at `192 px/m`.
Room dimensions come from `[room]` in the `configs` resource; the current
camera test room is `50 × 50` Tiles. Visible framing is independent and comes
from `[camera]`: preset `0` uses the explicit width/height values, while
presets `1`–`8` select the fixed `8×5`, `16×10`, `21×13`, `24×15`, `32×20`, `37×23`, `40×25`, and `45×28`
views. The active camera currently shows `22 × 20` Tiles; native-window and
reference-window pixel sizes do not change those meter-based dimensions.

The current room presentation uses one client-only `Sprite` per configured
`1 × 1 m` Tile in a subtle light checkerboard. It is a temporary spatial-scale
aid and does not define room collision, world data, or the future floor-
rendering system.

Room dimensions are loaded from the pre-match `configs` resource and inserted
identically into server and client. The client derives its orthographic
projection and viewport aspect from the effective camera view, so exactly the
configured tile count is shown and any native-window remainder is letterboxed.
The current one-room test has no room-boundary or transition logic; movement is
unbounded within the 50×50 presentation floor and the player spawns at origin.
The client watches `crates/configs/design.toml` and hot-reloads valid room and
camera changes during development.

The current presentation renders one client-only checkerboard floor using the
configured room dimensions. Each tile is one `1 × 1 m` Sprite; it is a temporary
scale aid and has no gameplay collision authority.

Character eyes are client-only presentation entities. PolyTools `eye_left` and
`eye_right` contours provide the per-character bounds and local pivots; the
client reconstructs the inner closed contour from each eye's exported stroke
mesh, uses it as the white eye surface, and constrains the round black pupil
with circle-versus-polygon collision. The pupil uses the polygon centroid as
its neutral point, or a maximum-clearance fallback when that centroid cannot
fit the complete circle. The gaze vector is transformed through the inverse
full eye transform hierarchy before collision, so mirrored or rotated asset
components retain the same screen-space look direction. This presentation-only
geometry lives in `apps/client/src/eyes.rs`. Device sampling and conversion
into the combined local movement/gaze input live in `apps/client/src/input.rs`.
`IJKL` updates only non-zero gaze intent, so releasing the keys retains the
previous direction. Barde is intentionally excluded for now.
The configured `[eyes].pupil_area_ratio` is currently `0.26`; each visible pupil
radius is derived from the schema-8 `closed_region_mesh` area. At asset-library
load time, `[eyes].hammerer_collision_radius_ratio = 0.35` is applied to the
Hammerer pupil radius to derive one absolute collision-reference radius. Every
eye uses that radius capped at its own visible pupil radius. The client intersects the pupil
polygon with the exported region triangles when the complete outline is
visible. For partial outlines, it clips only against reconstructed visible
stroke centerlines while retaining the complete closed region as the collision
boundary. It renders no separate eye fill and keeps the eye contour in front of
the resulting mesh. Legacy schema 5–7 packages may reconstruct the region
boundary from their stroke geometry.

PolyTools schema-8 manifests now expose
`presentation.authored_facing` with the values `left`, `right`, `neutral`,
`top`, and `down`; older compatible manifests default to `neutral` when the
presentation metadata is absent. The client retains that asset-authored value
separately from runtime movement and gaze state. Character geometry is parented
under a client-only visual-orientation root between the replicated player
entity and the existing asset-pivot hierarchy. Horizontal mirroring changes
only that orientation root, never the player `Transform`, authoritative
`Position`, simulation state, or network state.

The server derives current `MovementDirection` and retained `BodyFacing` from
movement intent plus retained `GazeDirection` from gaze intent. All three
components replicate to remote clients and are predicted for the controlling
client using the same shared simulation system. Client-only `pose.rs` owns all
movement-dependent visual transforms. Authored Left and Right poses convert
`BodyFacing` into an orientation-root x scale of `1` or `-1`; Top and Down
remain visual no-ops. For Neutral assets, `pose.rs` shifts the exported `head`
component and its complete child hierarchy up to 0.06 meters along the current
eight-directional movement vector, smoothing both entry and return with a
provisional 0.05-second half-life. The body remains unflipped and zero movement
returns the head to its authored transform. These values live together in the
client-only `PoseSettings` resource for focused visual tuning.

Eye gaze is read from the independently replicated/predicted `GazeDirection`.
Before normal transform propagation, the eye system computes each pupil's
current full hierarchy transform, including body orientation and neutral-head
motion, and converts the unchanged screen-space gaze through its inverse. A
body flip or head shift therefore does not reverse or replace visible gaze.
Initial `BodyFacing::Authored`, zero movement, and zero gaze preserve the
exported pose and neutral pupils until corresponding input arrives.

For the initial room-transition slice, `StartingRoomGrid` is shared
protocol-neutral domain data: its nine room coordinates map to stable
`RoomId`s, all internal cardinal boundaries are open, and its outer perimeter
blocks movement. Shared simulation constrains `Position` to that grid then
derives `RoomId` after each movement step. The server is authoritative over
both values; `RoomId` is replicated and predicted alongside the owner’s
`Position` so predicted movement and eventual server confirmation agree. On
each client, only the locally controlled player's current room anchors the
presentation camera to the complete configured room. The room-sized camera
viewport is centered inside the native window; any remaining area is black
letterboxing. This camera response is seamless and remains presentation-only.

Phase 2 now uses five real Bevy UI `Button` entities and one UI confirmation button. Their percentage-based layout owns hit testing and interaction state; the previous window-coordinate click calculation is removed. Polygon preview entities remain world-space presentation content until the later preview-composition phase.

Phase 3 gives selection previews a shared scale and bounded composition independent from authoritative in-game transforms. Reference-driven headwear proportions, face treatment, and compact class identifiers improve silhouette distinction while keeping the deliberately minimal closed-polygon language.

Phase 4 separates character labels into bounded, centered UI nodes and gives selection, hover, press, and disabled confirmation distinct visual states. Mouse input and keyboard input (`1`–`5`, arrow keys, and `Enter`) feed the same local selection and join path; neither path changes simulation or network protocol data.

## Input-to-simulation flow

```text
WASD / controller + IJKL
        │
        ▼
client input collection
        │  latest PlayerInput resource
        ▼
Lightyear native tick buffer
        │
        ▼
server-owned entity ActionState<PlayerInput>
        │  MovementIntent + GazeIntent
        ▼
simulation system
        │  authoritative Position + MovementDirection + BodyFacing + GazeDirection
        ▼
Lightyear state replication / owner prediction
        │
        ▼
client `pose.rs` and eye presentation derive body/head transforms and pupils
```

The fixed tick loop schedules and supplies simulation inputs. Game rules must not be embedded in the loop, input adapter, or network handler.

## Character selection and join protocol

- Selection happens locally before joining the gameplay session.
- The client requires a selected catalog-backed `CharacterId` before enabling confirmation.
- Confirming starts the connection and sends the join request once connected.
- The server rejects repeated join attempts from a connection that already owns a player.
- Character choices are not exclusive in the first slice; multiple players may choose the same character unless game design later changes this.
- Successful validation spawns the authoritative player at a server-selected spawn position.

## First-slice workspace intent

Intended logical structure:

```text
game01/
├── crates/
│   ├── world_data/
│   ├── simulation/
│   ├── network/
│   └── configs/
│       └── design.toml
└── apps/
    ├── server/
    └── client/
```

Cargo packages use the `game01-` prefix (`game01-client`, `game01-server`, `game01-configs`, `game01-network`, `game01-simulation`, and `game01-world-data`). The project-local validation entry point is `./scripts/check.sh`.

## Bevy dependency strategy

- Disable Bevy default features.
- The graphical client uses an explicit subset of Bevy's 2D features. It includes the currently implemented Bevy UI selection screen, while excluding scene, picking, audio, and 3D support.
- The headless server directly enables only `std`, `multi_threaded`, `bevy_log`, and `bevy_state`; it does not enable Bevy's `default_app` feature. Lightyear replication still transitively requires Bevy asset/serialization types, so removing a direct feature does not imply removing every related transitive crate.
- The server and headless shared crates use only the minimal non-rendering Bevy capabilities they need.
- Do not add Bevy UI, 3D, audio, scene, picking, development-tool, or extra asset-format features speculatively.
- Routine development and agent validation should use consistent feature sets and compiler flags where practical to avoid duplicate artifacts.
- Dynamic linking is for development only and is not a release/deployment requirement.

## First-slice implementation phases

### Phase 1 — workspace and build foundation

- Create the Cargo workspace and the confirmed crate/app boundaries.
- Configure minimal Bevy features separately for graphical and headless targets.
- Integrate Lightyear 0.28.
- Add the typed `configs` crate and initial `design.toml`.
- Provide one lightweight project-local validation wrapper and update `AGENTS.md` to use it.
- Ignore generated `target/` and `.DS_Store` files.
- Verify client and server independently with checks only; do not automatically run them.

Acceptance: client and server targets compile, and the server does not depend on graphical Bevy features.

### Phase 2 — world data and isolated simulation

- Add catalog-backed `CharacterId` selection and validation.
- Add identity, selection, ownership, room, spawn, and movement-intent data.
- Implement configurable direct-Transform movement at 30 Hz.
- Normalize diagonal intent.
- Add focused simulation tests.
- Do not add collision or velocity/mass yet.

Acceptance: simulation changes Transform deterministically from explicit intent without input, network, or presentation dependencies.

### Phase 3 — local transport and connections

- Start the headless local server.
- Connect up to five client processes.
- Track connection lifecycle and unique player ownership.
- Handle disconnects cleanly.

Acceptance: five clients can connect concurrently and the server distinguishes them.

### Phase 4 — character selection and authoritative spawn

- Render five provisional recognizable character choices.
- Implement selection and confirmation interaction.
- Send and validate join requests.
- Spawn server-owned player entities at separated positions.
- Replicate existing and newly joined players.

Acceptance: each client selects, joins, and sees the same set of spawned players with the correct character kinds.

### Phase 5 — synchronized movement

- Convert WASD into normalized movement intent.
- Send intent to the server and associate it with the owned player.
- Run server simulation at 30 Hz.
- Replicate authoritative Transforms to all clients.
- Stop movement safely on missing/stale input.

Acceptance: five clients move concurrently and all clients observe the same authoritative positions.

Slice 1 movement pipeline, superseded by Slice 2 Phase 2:

- The client presentation converts WASD state into `MovementIntent`; it does not move gameplay entities locally.
- The client network boundary sends the latest intent over a sequenced-unreliable channel at 30 Hz, so delayed input cannot overtake newer input.
- A movement message contains no player identifier. The server derives ownership from the authenticated connection and only writes to that connection's player.
- The server expires an intent to zero after three simulation ticks without a fresh message (about 100 ms).
- The independent simulation system consumes `MovementIntent` and the configured `MovementStep` in `FixedUpdate` at 30 Hz.
- Lightyear replicates the resulting authoritative `Transform` changes to every client.

### Phase 6 — manual slice verification

- Provide explicit commands for one server and five clients.
- The developer manually tests selection, join, simultaneous movement, disconnect/despawn, reconnect, late join, and server shutdown behavior.
- Prediction, Reconciliation, and optional interpolation remain outside acceptance.

Manual verification procedure (run each command in a separate terminal from the repository root):

VS Code exposes corresponding non-quiet commands through `.vscode/tasks.json`, keeping Cargo and runtime output visible: individual `game01: server` and `game01: client 1001`–`1005` tasks, plus `game01: local slice (server + 5 clients)` to start all six processes in parallel with dedicated terminal panels.
For a single-client movement/room test, `game01: local room test (server + client 1001)` starts the existing server and client `1001` tasks in parallel in separate terminals. It adds no readiness polling or startup wrapper; the client uses its normal connection behavior while the server starts independently.

```sh
cargo run --quiet --package game01-server
```

Start clients 1–4 first:

```sh
cargo run --quiet --package game01-client --features dev -- 1001
cargo run --quiet --package game01-client --features dev -- 1002
cargo run --quiet --package game01-client --features dev -- 1003
cargo run --quiet --package game01-client --features dev -- 1004
```

Verification sequence:

1. In every client, select a different character where practical and confirm joining. Every open client must show the same four characters at the same spawn positions.
2. Focus each client in turn and move its owned character with WASD, including a diagonal. Every open client must show only that character moving and must converge on the same authoritative position after input stops.
3. For genuinely simultaneous manual input, use multiple keyboards/operators; on a single desktop, rapidly alternating focused clients still verifies independent ownership and replication but not simultaneous key presses.
4. Close client `1002`. Its character must disappear from every remaining client without affecting the other players.
5. Start client `1005` with the command below, select a character, and join. As a late joiner it must immediately receive clients `1001`, `1003`, and `1004`; those clients must receive `1005`.

```sh
cargo run --quiet --package game01-client --features dev -- 1005
```

6. Restart client `1002` with its original command, select again, and join. All five clients must now see the same five-player set, proving reconnect and released server capacity.
7. Move all five owned characters and compare their final positions across every window. No client-side prediction is expected, so visible network latency or unsmoothed motion is acceptable in this slice.
8. Stop the server. Clients must report or otherwise reflect disconnection without crashing. Close the remaining client processes manually.

Expected server evidence includes connection, authoritative spawn, disconnect, and reconnect log entries for the corresponding identities. A failed item should be recorded with the responsible client ID, selected character, observed windows, and relevant server log line before implementation is changed.

## Second-slice implementation phases

### Phase 1 — authoritative state and presentation boundary

- `world_data::Position` is the sole authoritative two-dimensional player location in meters.
- The server spawns `Position`; simulation mutates it; Lightyear replicates it.
- The headless simulation and network state no longer require or replicate Bevy `Transform`.
- The graphical client creates a presentation `Transform` when a replicated player becomes renderable and synchronizes its x/y translation from `Position` during `Update`.
- Input transport, prediction, interpolation, and the 30 Hz tick rate remain unchanged in this phase so later effects can be evaluated independently.

Acceptance: synchronized movement still follows the server, while simulation and replicated world state contain no presentation transform authority.

### Phase 2 — tick-bound native input pipeline

- The custom sequenced-unreliable `MovementInput` message and movement channel are removed.
- The graphical client still collects keyboard state outside simulation and exposes the latest normalized `MovementIntent` through a local adapter resource.
- In Lightyear's `FixedPreUpdate` input stage, that value is written to the locally controlled entity's native `ActionState` and buffered for the associated tick.
- Lightyear sends redundant recent input history, allowing later packets to recover an earlier lost direction or stop transition.
- `ControlledBy` establishes the server-side connection/entity relationship and causes only the owning client to receive the local `Controlled` marker.
- The server rejects native input targets not owned by the sending connection.
- The current tick's native `ActionState` is copied directly to the same entity's protocol-neutral `MovementIntent` before simulation; no per-message player search is required.
- The old three-tick `InputAge` timeout is removed. A received neutral input applies on its addressed simulation tick instead of waiting for a local expiry counter.
- Simulation remains input-transport-independent and the tick rate remains 30 Hz for isolated evaluation.

Acceptance: movement and neutral stop input use Lightyear's tick-addressed redundant pipeline, are applied only to the authenticated controlled entity, and reach simulation as explicit `MovementIntent`.

### Phase 3 — owned-position prediction and reconciliation

- Lightyear prediction support is enabled and `Position` is registered as a predicted replicated component.
- Every server-owned player still replicates to all clients, but `PredictionTarget` names only its controlling peer.
- The owning client receives `Controlled` and the native input marker, then carries a local `MovementIntent` adapter component used solely to feed shared simulation.
- Client prediction begins only after Lightyear reports the input timeline synchronized.
- Client and server construct `MovementStep` from the same embedded design configuration and execute the same `move_players` system in `FixedUpdate`.
- Lightyear stores predicted position history, compares it with confirmed server snapshots, restores mismatches, and replays buffered tick inputs.
- Other players remain plain replicated entities until snapshot interpolation is added in Phase 4.
- Visible correction smoothing remains a presentation concern for the later render-interpolation phase; Phase 3 keeps reconciliation state exact.
- Tick rate remains 30 Hz so the responsiveness improvement comes from prediction rather than a frequency change.

Acceptance: the controlled character responds from local tick input without waiting for a server round trip, while confirmed server `Position` remains authoritative and can reconcile the prediction.

### Phase 4 — remote snapshot interpolation

- `Position` remains the only replicated and authoritative location, with a protocol-registered linear interpolation function.
- Each server player uses mutually exclusive delivery roles: its owner receives `Predicted`, while every other client receives `Interpolated`.
- Lightyear buffers confirmed remote `Position` snapshots and evaluates them on its delayed, synchronized interpolation timeline. Remote entities are not extrapolated or locally simulated.
- The owning client remains on the prediction and reconciliation path introduced in Phase 3; snapshot interpolation never delays local control.
- Presentation copies the resolved `Position` into graphical `Transform` during `PostUpdate`, after Lightyear's `Update` interpolation work.
- Tick rate remains 30 Hz and no frame-level render interpolation is introduced, keeping both later improvements independently measurable.

Acceptance: the local player remains immediately responsive and authoritative corrections remain possible, while other players move between received server snapshots rather than snapping directly from one snapshot to the next.

### Phase 5 — presentation-only render interpolation

- Only the locally controlled predicted player records its previous and current fixed-tick `Position`; remote players remain exclusively on Lightyear's snapshot-interpolation path and receive no second smoothing delay.
- During `PostUpdate`, the local graphical `Transform` is linearly sampled from that two-tick presentation history using Bevy's fixed-time overstep fraction.
- The interpolation history and `Transform` are client-only presentation data. They are neither replicated nor read by simulation, and authoritative/predicted `Position` is never overwritten with a visual value.
- Prediction tracks the pre-rollback predicted `Position`. After reconciliation, the network adapter exposes only the resulting positional error as `ClientPositionCorrection`; presentation anchors it to the last visible `Transform` and decays it with a frame-rate-independent 200 ms half-life.
- Remote snapshot delay, simulation tick rate, movement parameters, and network protocol payloads remain unchanged for isolated evaluation.

Acceptance: local fixed-tick movement is visually continuous between simulation steps, remote players retain exactly one snapshot-interpolation pass, reconciliation corrections converge smoothly, and simulation observes only exact `Position` state.

### Phase 6 — unified 60 Hz cadence

- The canonical `configs/design.toml` simulation rate increased from 30 to 60
  ticks per second; movement speed remained 4 meters per second during this
  historical cadence slice and each tick therefore applied half the former
  displacement. Slice 15 later superseded the global speed with `0.8 m/s` as a
  separate slow-paced game-design decision.
- Server `ScheduleRunner`, server and client `Time<Fixed>`, Lightyear client/server timelines, native input buffering, prediction replay, and `ReplicationMetadata` all derive the same 16.67 ms tick duration from that design value.
- Changed replicated positions can consequently publish at up to 60 snapshots per second. No separate snapshot-rate throttle is introduced in this phase.
- Local render interpolation now carries at most one 60 Hz simulation step of intentional presentation delay, and Lightyear's unchanged default remote interpolation ratio operates on the shorter 60 Hz send interval.
- Render frame rate remains independent, while movement distance per second and the Phase 5 correction half-life remain unchanged.

Acceptance at the time of this cadence change: server authority, client
prediction, input ticks, and snapshot tick metadata advanced at one shared
60 Hz cadence; the later Slice 15 balance change does not alter that cadence.

### Phase 7 — joint snapshot-rate and buffer tuning

- The simulation, native input, and prediction/reconciliation timelines remain at 60 Hz.
- Two explicitly requested design parameters are added under `[network]`: `snapshot_send_hz = 30` and `remote_interpolation_ratio = 1.0`.
- `ReplicationMetadata` uses the independent 33.33 ms snapshot interval, so continuously changed replicated state can publish at up to 30 snapshots per second rather than 60.
- The server accepts only a positive snapshot rate that is an integer divisor of, and no greater than, the simulation rate. This keeps publication cadence deterministic and evenly aligned with simulation ticks.
- Each client link receives an explicit Lightyear `InterpolationConfig` with ratio 1.0. Lightyear's fixed one-simulation-tick safety margin and dynamic measured-jitter margin remain unchanged.
- At 60 Hz simulation and negligible jitter, the expected remote presentation offset is approximately one 33.33 ms send interval plus one 16.67 ms safety tick, or roughly 50 ms.
- Lowering the sender-dependent rate reduces position/snapshot traffic substantially, but does not imply that total network traffic is reduced by exactly 50 percent.

Acceptance: authoritative simulation and owned-player response retain their 60 Hz behavior, while remote state publishes at 30 Hz and uses a 1.0-interval interpolation buffer without removing Lightyear's adaptive jitter protection.

## Third-slice implementation phases

### Phase 1 — controlled adverse-network profile

- Lightyear's receive-side link conditioner simulates imperfect transport without entering world data or simulation code.
- Network simulation is disabled by default. Set `GAME01_NETWORK_SIMULATION=average` on both the server and every client to enable it.
- The `latency-jitter` profile represents an end-to-end target of approximately 100 ms round-trip latency and 20 ms jitter with no packet loss. The `average` profile adds 2 percent packet loss to the same target. Half of each value is applied independently to each receive direction.
- The profile therefore tests client input delivery and server snapshot delivery together. Lightyear's redundant native-input history, owned-player prediction/reconciliation, and remote snapshot interpolation remain unchanged.
- The randomized packet sequence is intentionally nondeterministic; automated tests verify profile activation, while gameplay quality is evaluated manually.

Run the server and each client from separate terminals with the same environment value:

```sh
GAME01_NETWORK_SIMULATION=average cargo run --package game01-server
GAME01_NETWORK_SIMULATION=average cargo run --package game01-client --features dev -- 1001
```

Use `GAME01_NETWORK_SIMULATION=latency-jitter` for the loss-free comparison run.

Repeat the client command for IDs `1002` through `1005`. Startup logs must report `network_simulation="average"`. Compare against the existing VS Code tasks, which deliberately retain the default `off` profile.

Manual acceptance checks:

1. The owned player responds immediately through prediction and stops promptly after releasing WASD.
2. Reconciliation may visibly correct inaccurate prediction but must not teleport continuously or diverge permanently.
3. Remote players remain delayed but visually continuous; temporary degradation is acceptable during random loss, persistent freezing is not.
4. All five clients converge on the same stopped positions, and no client or server panics during join, movement, disconnect, or reconnect.

Initial manual result: owned-player response, stopping, convergence, and lifecycle stability pass under the `average` profile. Remote-player delay remains acceptable, but motion becomes visibly less fluid. Join presentation is delayed by less than one second and ungraceful disconnect presentation by roughly four seconds; lifecycle latency is tracked separately from movement interpolation.

### Phase 2 — adverse-network interpolation-buffer trial

- The snapshot rate remains 30 Hz and simulation/input/prediction remain 60 Hz.
- The remote interpolation ratio increases in isolation from `1.5` to `2.0`.
- At negligible jitter this adds half a snapshot interval, approximately 16.67 ms, to the prior remote presentation offset. The expected base offset becomes roughly 83.33 ms before Lightyear's dynamic jitter allowance.
- The additional history is intended to reduce interpolation-buffer underruns caused by the Phase 1 jitter and packet loss. It deliberately trades a small amount of remote delay for smoother motion.
- No extrapolation, senderate change, prediction change, or lifecycle-timeout adjustment is included, preserving a clean A/B comparison under the same `average` profile.

Acceptance: under the unchanged adverse-network profile, remote motion is materially smoother than with ratio `1.5`, while its additional delay remains acceptable and owned-player response is unchanged.

### Phase 3 — bounded remote presentation extrapolation

- Lightyear remains responsible for authoritative snapshot history and interpolation. When its interpolation timeline advances beyond the newest confirmed remote `Position`, the network adapter exposes a presentation-only extrapolation offset.
- Velocity is derived from the two newest confirmed position samples and never becomes authoritative world state.
- Extrapolation is limited to two 30 Hz snapshot intervals (approximately 66.67 ms). After that limit the remote visual stops rather than drifting indefinitely.
- A confirmed unchanged sample produces zero velocity, so an authoritative stop is not intentionally extrapolated as packet loss.
- Only remote interpolated entities receive the offset. The locally controlled predicted player, server simulation, replicated `Position`, reconciliation, input path, and snapshot cadence are unchanged.

Acceptance: isolated lost snapshots under the `average` profile produce shorter or less visible remote stalls, without changing owned-player response or allowing remote visuals to drift for more than two snapshot intervals.

## Open technical decisions

- Persistence architecture after the non-persistent first slice.
