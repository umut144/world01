# The Labyrinth — persistent technical architecture

Last updated: 2026-08-14

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
  ArcherF, Barde, Chantres, Glavier, Hammerer, Mage, Rogue, Sorcerer, Warrior,
  and Wizard);
- player identity, selected character, and ownership markers;
- standard-room and spawn data;
- the fixed initial `3 × 3` room-grid coordinate mapping and authoritative
  `RoomId` assignment;
- movement intent data passed into simulation;
- authoritative two-dimensional `Position` in meters;
- replicated gameplay components that are not transport-specific.

Slice 1 used `Transform` directly as provisional movement state. Slice 2 Phase 1 supersedes that choice with the protocol-neutral `Position`; graphical transforms no longer belong to shared world state.

Implemented shared Phase 2 data uses transport-neutral scalar identifiers and coordinates: `PlayerId`, `PlayerOwner`, `SelectedCharacter`, `Player`, `RoomId`, `StandardRoom`, `SpawnPoint`, and `MovementIntent`. Network-specific connection types stay outside `world_data`.

### `configs`

Owns explicitly requested, human-editable game-design parameters:

- dedicated workspace crate/directory named `configs`;
- typed configuration boundary;
- initially backed by `design.toml`;
- initially exposes movement speed and the confirmed 30 Hz simulation tick rate if tick rate is represented as design configuration.

Later values such as mass, MaxHP, or attack values are added only when the developer explicitly requests them and the corresponding behavior enters scope. Do not expose every internal constant merely because simulation uses it.

### `simulation`

Owns transport-, input-device-, and presentation-independent game rules:

- consumes explicit movement intent;
- applies current movement rules to authoritative state;
- runs when scheduled by fixed-step orchestration but contains no tick-loop orchestration itself;
- does not read keyboard input;
- does not send or receive network messages;
- does not render, animate, play audio, or manage UI.

The simulation mutates protocol-neutral `Position`, never presentation `Transform`. A mass/velocity movement model will be required soon and does not need to follow real-world physics. Keep the simulation interface and network/input flow suitable for adding explicit velocity and mass without rewriting those outer layers.

Collision is not required in the first slice, including room-boundary collision.

Movement is represented by `MovementStep`, constructed from the typed design configuration. It supplies explicit speed and step duration to the simulation system, clamps intent to unit length, rejects non-finite intent, and directly updates `Position` without owning fixed-tick scheduling.

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

Slice 2 Phase 2 uses Lightyear's native input pipeline instead of a custom movement message. The client samples hardware state into a local resource during normal frame input collection, writes that state to its controlled entity in Lightyear's `FixedPreUpdate` input stage, and sends redundant tick-addressed history. The server validates each input target against `ControlledBy`, lets Lightyear select the current tick's `ActionState`, and adapts that state to `MovementIntent` before simulation. This is a direct entity-local query with no connection-to-player scan.

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

### PolyTools character-asset boundary

- PolyTools Runtime Export is the canonical interchange format for character
  presentations; the synced catalog determines which character IDs are valid.
- The game consumes imported copies under `game01/assets/`; it does not read
  the sibling PolyTools project at runtime and does not retain a
  `polytools/world01` path prefix in its asset tree.
- `./scripts/sync_polytools_characters.sh` copies the authoritative
  `catalog.json` and all character packages named by it from PolyTools into
  `assets/characters/`. It validates the catalog, imports every advertised
  character package plus referenced Symbol packages, and replaces the generated
  destination atomically so stale components cannot survive a re-export. The
  source World directory can be overridden with `POLYTOOLS_WORLD_DIR`. New
  syncs require PolyTools Runtime Manifest schema 8; the client temporarily
  retains loading compatibility with the already imported schema 5 through 7
  packages.
- Client-only loading validates each imported manifest and turns its already
  triangulated fill, closed-region, and contour-stroke geometry into Bevy 2D
  mesh presentation entities. Component transforms, hierarchy, and `z_index`
  remain presentation data; PolyTools geometry never enters simulation,
  networking, or replicated world state.
- The current export contract contains geometry but no material/color data.
  The first integration applies a small client-owned temporary palette by
  character and component name. A future material export is a separate
  PolyTools contract decision.

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
into local movement/gaze presentation inputs live in `apps/client/src/input.rs`;
`IJKL` is not part of movement intent or network state. Barde is intentionally
excluded for now.
The configured `[eyes].pupil_area_ratio` is currently `0.26`; each visible pupil
radius is derived from the schema-8 `closed_region_mesh` area. Movement uses
`[eyes].pupil_collision_radius_ratio = 0.35`. The client intersects the pupil
polygon with the exported region triangles, renders no separate eye fill, and
keeps the eye contour in front of the resulting clipped mesh. Legacy schema
5–7 packages may reconstruct the region boundary from their stroke geometry.

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
WASD / local input device
        │
        ▼
client input collection
        │  latest MovementIntent resource
        ▼
Lightyear native tick buffer
        │
        ▼
server-owned entity ActionState
        │  explicit simulation input
        ▼
simulation system
        │  authoritative Position mutation
        ▼
Lightyear Position replication
        │
        ▼
client presentation derives Transform
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

- The canonical `configs/design.toml` simulation rate increases from 30 to 60 ticks per second; movement speed remains 4 meters per second and therefore each tick applies half the former displacement.
- Server `ScheduleRunner`, server and client `Time<Fixed>`, Lightyear client/server timelines, native input buffering, prediction replay, and `ReplicationMetadata` all derive the same 16.67 ms tick duration from that design value.
- Changed replicated positions can consequently publish at up to 60 snapshots per second. No separate snapshot-rate throttle is introduced in this phase.
- Local render interpolation now carries at most one 60 Hz simulation step of intentional presentation delay, and Lightyear's unchanged default remote interpolation ratio operates on the shorter 60 Hz send interval.
- Render frame rate remains independent, while movement distance per second and the Phase 5 correction half-life remain unchanged.

Acceptance: server authority, client prediction, input ticks, and snapshot tick metadata advance at one shared 60 Hz cadence; one simulated second still moves a character exactly four meters.

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
