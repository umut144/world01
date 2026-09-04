# World 01 Multiplayer Sandbox — Technical Specification

Last updated: 2026-09-04

## Purpose and interpretation

This document records the technical foundation for the World 01 multiplayer
sandbox described in [`SANDBOX_VISION.md`](SANDBOX_VISION.md). Shared
cross-game design intent is recorded in [`WORLD_DESIGN.md`](WORLD_DESIGN.md);
this document distinguishes two
kinds of statement:

- **Current verified baseline** describes behavior and boundaries that exist in
  the repository today.
- **Confirmed shared direction** describes constraints that must hold when a
  capability is extracted into the shared `main` foundation. It does not imply
  that a corresponding public API already exists.

The current codebase grew from The Labyrinth and remains partly game-specific.
The design umbrella, repository, and Rust package namespace now use `world01`.
Physical separation into crates is not by itself proof that their complete
contents are already genre-neutral or reusable. This document is neither an
implementation roadmap nor a promise to build speculative framework APIs.

## Current workspace and dependencies

The workspace currently has these physical boundaries:

```text
apps/
  client/                 graphical current-game application and composition
  server/                 headless authoritative current-game application
crates/
  configs/                typed technical runtime configuration
  design/                 World-01 and game design data with typed design loading
  content/                PolyTools import, validation, runtime derivation
  world_data/             protocol-neutral serializable/domain components
  simulation/             deterministic movement, aim, and Hammer systems
  network/                Lightyear protocol and client/server transport
```

The baseline is stable Rust, Bevy 0.19, and Lightyear 0.28. Bevy default
features are disabled. The server has no direct rendering, windowing, audio, or
input-device features.

Local VS Code server/client tasks show Cargo's build progress and clear their
dedicated terminal before each start. Runtime console output omits timestamps,
levels, and module targets. It shows only global errors plus messages explicitly
sent to the `game_console` target; that target accepts `DEBUG` and higher
levels. The current startup messages report successful server start, client
start, and client login.

The graphical client starts windowed at a physical `1024 × 640 px`. The window
remains resizable without a configured maximum, so maximizing may use the full
available monitor area, including `2880 × 1800 px` where the display permits.

In the following graph, `A -> B` means **B directly depends on A**:

```text
configs --------------------------------> simulation
design ---------------------------------> simulation
world_data ----+------------------------> simulation
               +----> content ----------> simulation
               +----> network

configs + content + world_data + simulation + network ----> client/server
```

There is no direct dependency from `network` to `simulation` or `content`.
Both applications currently depend directly on all five shared crates and own
their final schedule and plugin composition.

### Current implementation coupling

The physical boundaries are useful, but their current contents still include
Labyrinth behavior:

- `configs` contains technical simulation cadence, network cadence, camera
  framing, and the bounded world-overlap recovery rate in `runtime.toml`.
- `design` owns `world01.toml` for shared World-01 baseline values alongside
  game design data such as characters, weapons, abilities, and mass
  assignments.
- `content` requires the current character catalog and Hammer package and
  derives Hammer combat geometry, Mage eye emitters, Character Hurt geometry,
  placed world Collision geometry, and character HP.
- `world_data` is free of Lightyear and asset parsing, but includes
  `SelectedCharacter`, `CharacterHealth`, weapon aim, Hammer attack state, and
  Mage eye-beam state.
- `simulation` contains deterministic general movement alongside gaze, weapon
  aim, Hammer rules, and Mage charge/projectile rules; its canonical step
  currently schedules all of them.
- `network` has separate client/server transport modules, but its internal
  protocol registration is fixed to the current join request, `PlayerInput`,
  character state, health, Hammer state, and Mage eye-beam state.
- `apps/client` and `apps/server` compose a concrete Labyrinth session,
  including character selection, five-player spawning, room presentation,
  health bars, eyes, Hammer presentation, and Mage eye-beam combat and
  presentation.

These are facts about the starting implementation, not requirements that every
future multiplayer game must inherit. Several current components embody
World-01 character and weapon concepts that were first implemented while
building The Labyrinth; their current code location does not make those design
concepts Labyrinth-owned.

## Confirmed dependency and authority boundaries

When behavior is promoted into the shared sandbox foundation, these boundaries
must remain true:

- `world_data` contains protocol-neutral data and must not parse asset formats
  or expose Lightyear/transport types.
- simulation consumes explicit typed state and intent. It must not sample
  hardware, send packets, own application tick-loop orchestration, or create
  presentation entities.
- the network boundary owns Lightyear-specific protocol, transport,
  replication, prediction, interpolation, and connection adapters. Genre rules
  and game-specific spawn composition do not become transport policy.
- client and server applications compose shared facilities with the active game
  branch; presentation never becomes gameplay authority.
- introduce another crate only when a stable shared dependency boundary needs
  independent dependencies or reuse. Otherwise prefer a module.

## Multiplayer network model

### Current verified baseline

The existing application uses a server-authoritative model:

- clients send input and join requests, never authoritative gameplay outcomes;
- the server validates ownership and joins, owns entity lifecycle, runs the
  authoritative simulation, and publishes replicated state;
- existing overlap is corrected only by the server before movement blocking,
  and the existing predicted `Position` replication reconciles and smooths the
  result for the owner;
- Character pairs are read from one snapshot enumerated in stable `ActorId`
  order, share their correction inversely to movement mass, and are not
  speed-clamped. Positive corrections include `0.001 m` rounding clearance;
- Character/world correction runs after Character-pair correction. Static
  geometry never moves, while each Character receives the complete accumulated
  correction from the world Regions its current bounds overlap, limited by the
  configured `3.0 m/s` recovery rate (`0.05 m` at the current 60 Hz). It needs no
  `ActorId` order because Characters are resolved independently. Mass, movement
  speed, RUN, DASH, and life state do not participate. The Character's multiple
  Regions form one projected hull against each world Region, preventing a
  capped correction from alternating between body and feet;
- separating Characters beside a wall can still leave one temporarily inside
  world collision when the required recovery exceeds the per-tick limit.
  Movement blocking permits it to leave but not to move deeper. A non-convex
  authored Region receives a valid but potentially non-minimal convex-hull
  escape, with the configured limit bounding each tick rather than promising a
  shortest route;
- replicated gameplay components use protocol-neutral types from
  `world_data`; Bevy `Transform` remains derived client presentation;
- the owning client predicts explicitly registered state and reconciles to
  confirmed server state;
- remote interpolation, correction smoothing, and bounded extrapolation are
  presentation-only;
- disconnect cleanup and late-join replication are implemented.

Protocol registration is currently internal and Labyrinth-specific. A game
plugin cannot yet register arbitrary replicated components, messages,
ownership checks, or prediction/interpolation policies through a stable public
sandbox extension point.

The current transport baseline uses a remote interpolation ratio of `2.0` and
presentation-only extrapolation of at most two snapshot intervals. Local
five-client trials at approximately 100 ms round-trip latency, 20 ms jitter,
and 2% packet loss found this smoother than the earlier `1.5` ratio while
retaining acceptable delay. Authoritative state, prediction, and reconciliation
never consume the extrapolated presentation position.

### Confirmed shared direction

Multiplayer networking is a foundational sandbox capability. A reusable game
extension must be able to declare its own protocol state and server-side input
validation without placing its genre rules inside the transport layer. The
shape of that registration API is not yet established and should emerge from a
concrete extraction slice. The sandbox does not prescribe matchmaking,
persistence, combat, damage, or team rules merely because the current game has
related domain state.

## PolyTools content boundary

### Current verified baseline

PolyTools Runtime Export is the current authored-asset interchange format. The
repository consumes synchronized copies below `assets/` and does not read the
sibling authoring project at runtime.

`content` is the sole current JSON parsing, manifest validation, reference
resolution, and typed conversion boundary. The applications construct a
validated `RuntimeContent` value and either retain it or derive their runtime
resources from it. Loading uses a temporary source map while resolving
references; there is not yet a generic persistent asset-cache service.

The existing loader is deliberately specific: it recognizes the character
catalog, requires Hammer content, validates Hammer attachment contracts, and
exposes Labyrinth-derived health and combat geometry. Asset keys provide
stable identity inside this contract, while the set of supported package roles
is not yet plugin-extensible.

World-01 and game-specific design data are loaded through `design`, not
`configs`. The design crate owns typed shared baseline values, character and
weapon data, including the mapping from gameplay roles to stable Component
names. It does not parse PolyTools manifests;
`content` resolves those names against validated Component geometry before the
simulation receives the resulting typed data. `configs` remains the boundary
for technical runtime settings such as simulation and network cadence, camera
framing, and bounded recovery from invalid world overlap.

### Confirmed shared direction

Raw PolyTools JSON and schema handling remain confined to `content`. Consumers
receive validated typed data; simulation consumes only semantic data it
explicitly needs, while visible mesh hierarchy and material policy remain in
presentation. A future game-owned asset contract must extend this boundary
rather than parse raw manifests elsewhere. No general asset-contract
registration API exists yet.

PolyTools schema 16 exposes authored and Component-bound Regions with `attack`,
`hurt`, or `collision` roles. The content boundary validates both variants and
converts them into typed geometry. Current Character Hurt and Hammer attack
derivation retains its earlier Component-based fallback when the corresponding
authored Regions are absent. Placed props participate in Mage projectile
collision only when they declare a `collision` Region; visible Component
geometry is not silently treated as collision geometry.

## Entity and character state

### Current verified baseline

Reusable pieces already exist for numeric player identity, ownership,
protocol-neutral position, and selected asset identity. However, there is no
generic sandbox entity-instantiation path today. Server session code currently
turns a character join request into a Labyrinth player with movement, gaze,
health, and optional Hammer components at one of five fixed spawn positions.
The presentation root and asset attachment flow are private client-app
implementation details rather than a public sandbox contract.

### Confirmed shared direction

A future shared instantiation boundary may assemble explicit identity,
ownership, protocol-neutral state, and an optional presentation attachment.
The active game branch must continue to own spawn rules and game-specific
components. “Character” does not by itself imply a combatant, class, inventory,
or fixed player-avatar model. Visual components never become authoritative
state. The exact public API is not yet established.

## Camera and room presentation

### Current verified baseline

The graphical client implements a configurable orthographic camera,
aspect-safe viewport calculation, letterboxing, camera follow, and a temporary
tile-based room presentation. World 01's maps are authored in SceneMaker and
exported as engine-neutral snapshots. `scripts/sync_scenemaker_world.sh`
validates and synchronizes the complete export directory, and the
protocol-neutral `world_data` crate embeds its files as a deterministic catalog.
Its build script rejects unreadable scene headers, unknown scene kinds, and
duplicate scene IDs before compiling that catalog.
Callers select an Instance by its scene ID. Template exports are imported into a
typed catalog grouped by `group_number`; authoritative occupancy can choose and
compose them into a map while the game runs. A pure geometry projection can
place one explicitly chosen Template at one matching Instance Anchor: it
requires identical Terrain-cell sizes, uses a signed integer grid offset,
rejects a mask or Prop origin outside the Instance (the boundary itself is
valid), and returns translated cells and Props without merging them. A Prop's
full footprint is not part of this boundary check.
Template Prop IDs remain local during projection; merging assigns each one the
deterministic ID
`template.<anchor_id>.<template_scene_id>.<local_prop_id>` and rejects even a
collision of those namespaced IDs. Instance positions are
centered in world space; Template cells, Props, and insertion anchors
deliberately remain in the Template's own bottom-left coordinate frame until
that projection. Instance Anchors and Template insertion anchors also retain
their validated integer Terrain-grid coordinates, so projection never has to
recover grid positions from floats.
A second pure operation merges one projection into a new `WorldMap` without
mutating either input. Placement ranks enter `world_data` as an explicit typed
parameter loaded by `design`, preserving the dependency boundary. Template
Terrain replaces equal- or lower-ranked Terrain. Prop replacement uses the same
axis-aligned placement footprints that SceneMaker exports and shows with its red
placement highlight: a footprint's lower-left corner is the Prop position minus
its exported Asset anchor, height is ignored, and strict interval overlap means
touching edges are allowed. A Template Terrain cell removes every overlapping
existing Prop of equal or lower rank. Each incoming Template Prop is dropped as
a whole if any overlapping existing Prop has a higher rank; otherwise it removes
all overlapping equal- or lower-ranked Props and is appended in source order.
Existing ordering is otherwise retained, new cells follow in Template order,
and generated Prop IDs use the namespace above. Missing ranks, missing or
invalid Prop footprints, overlapping Props within one authored scene, and
identity collisions are errors. A valid Prop profile has `footprint_meters`
with finite positive dimensions and `anchor_meters` with finite coordinates in
the inclusive range from zero through that footprint's size. The export sync
gate enforces the same profile requirements before replacing embedded maps.
These placement footprints are authoring geometry, not gameplay collision
regions.
Maps and projected Templates require Prop origins inside or on the world
boundary; their full placement footprints may extend beyond it.
Collision, navigation, presentation, and other derived state are not rebuilt by
this pure operation and must be rebuilt by the authoritative runtime after an
occupant changes.
Each `WorldMap` retains its Instance scene ID, and each projection is bound to
that ID so it cannot be merged into another Instance merely because dimensions
or Anchor names happen to match. A protocol-neutral composition owns an
unchanged base map, compact serializable Anchor occupancy with a generation,
and its current derived map. Every accepted occupancy change rebuilds from the
base and folds occupied Anchors in authored SceneMaker order; assignment order
cannot affect the result, an empty Anchor is normal, and invalid changes leave
the composition untouched.
`AnchorOccupancy` is now a normally replicated Lightyear component on one
persistent server-owned world-state entity. It has no owner, prediction target,
or interpolation target. A separately replicated marker makes this singleton
identity explicit to the client. Delivery of its current complete occupancy to
a late joiner relies on Lightyear's normal semantics for a persistent
`Replicate` entity; the project structurally verifies that configuration but
does not yet carry a transport-level late-join test. Both applications load the
same embedded base map, Template catalog, and Placement Ranks. After
replication, the client stages only the newest received occupancy. The shared
fixed-tick world transaction composes that candidate and builds its `WorldMap`,
world collision catalog, collision broad-phase grid, and `AnkhLayout` before
committing any of them. It runs before `SimulationSet::Collision`, so server
separation reads the new catalog and grid in the accepting tick. A candidate
without an Ankh, with invalid composition data, or with invalid collision
geometry leaves the accepted composition and all derived resources unchanged.
Repeated and older generations are ignored. The applied generation is stored
beside the runtime resources, making repeated fixed ticks and rollback replays
idempotent rather than dependent on Bevy change detection. Respawn derives its
indexed Ankh candidates from the current `AnkhLayout`. The server composition
is authoritative, and its occupancy is mirrored to the replicated component
only after that generation has become the applied runtime world.
The current fixed protocol ID does not fingerprint embedded maps, Templates,
or Placement Ranks. Deterministic world derivation therefore currently assumes
that client and server come from the same content build; a compatibility
fingerprint or handshake is required before heterogeneous builds are allowed to
connect.
Navigation is not implemented yet. Client terrain, Prop, and Ankh presentation
entities are rebuilt together exactly once for each applied runtime generation;
they never render an unaccepted occupancy snapshot. For manual acceptance, the
temporary in-game digit controls send an ordered reliable request rather than
mutating the client world: `0` empties both test Anchors, `1` assigns
`test_template02` only to `template_anchor_001`, `2` assigns it only to
`template_anchor_002`, and `3` assigns it to both. The server accepts such a
request only from a client with a joined Character, validates the full
composition, applies it through the same fixed-tick world transaction, and then
replicates the accepted occupancy. For the first version, a rare occupancy
change during a client prediction rollback may replay buffered input against
the newest world until normal server reconciliation; retaining historical
worlds for every rollback tick is deliberately deferred unless playtesting
shows that short discrepancy to be material.
The current strict importer accepts export schema 9 and embedded scene schema
10. It requires their water fields so an older snapshot cannot masquerade as
current, but `WorldMap` does not yet model water and Templates carrying water
are rejected. The headless server and graphical client therefore consume the
same selected Instance dimensions and gameplay placements. SceneMaker stores only
PolyTools asset keys and authoring geometry; the runtime resolves those keys
through its synchronized PolyTools content and renders the real runtime
geometry. These facilities live inside the Labyrinth client application. There
is no public reusable camera service, room service, map service, or
camera-target contract today.

### Confirmed shared direction

If extracted, shared camera/frame presentation may provide orthographic
framing, aspect handling, and explicit follow targets. It must not silently
define grids, room adjacency, transitions, collision, encounters, or world
topology. Those policies remain game-owned. Extraction should follow a concrete
reuse need rather than create a speculative service in advance.

## Input boundary

### Current verified baseline

Keyboard and controller sampling are client-only and are converted into the
protocol-neutral `PlayerInput` sent through Lightyear's tick-bound native input
path. The current input shape is fixed to movement, gaze, and attack. Keyboard
bindings and controller mappings are hard-coded in the client. There is no
named-action registry, one-dimensional action type, configurable binding
service, or explicit generic focus-loss contract yet.

### Confirmed shared direction

Simulation must remain independent of device APIs. A reusable input layer may
eventually expose replaceable digital, one-dimensional, and two-dimensional
actions that feed both owner prediction and authoritative server input. Games
interpret those actions as their own intent. This is a confirmed boundary
direction, not a description of an existing registration API.

## Game-plugin integration status

The repository does not yet expose a complete stable sandbox plugin API. The
existing Bevy plugins primarily organize the current client, server, prediction,
session, and Hammer presentation code. The example in
[`PLUGIN_GUIDE.md`](PLUGIN_GUIDE.md) is design guidance and uses illustrative
names; it is not guaranteed to compile against current public APIs.

When a concrete game branch is separated from the shared foundation, its
plugins must:

1. keep game-owned simulation rules deterministic and independent of input
   devices, transport, and presentation;
2. keep presentation client-only and derived from authoritative, predicted, or
   interpolated state;
3. validate game-owned requests and ownership on the server;
4. obtain authored content through the shared `content` boundary;
5. depend only on public shared contracts rather than application-private
   implementation details.

These constraints are stable. The specific traits, registries, events, and
plugin split should be introduced only by a concrete implementation need.
