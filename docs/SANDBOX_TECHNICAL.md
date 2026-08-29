# game01 Multiplayer Sandbox — Technical Specification

Last updated: 2026-08-29

## Purpose and interpretation

This document records the technical foundation for the multiplayer sandbox
described in [`SANDBOX_VISION.md`](SANDBOX_VISION.md). It distinguishes two
kinds of statement:

- **Current verified baseline** describes behavior and boundaries that exist in
  the repository today.
- **Confirmed shared direction** describes constraints that must hold when a
  capability is extracted into the shared `main` foundation. It does not imply
  that a corresponding public API already exists.

The current codebase grew from The Labyrinth and remains partly game-specific.
Physical separation into crates is not by itself proof that their complete
contents are already genre-neutral or reusable. This document is neither an
implementation roadmap nor a promise to build speculative framework APIs.

## Current workspace and dependencies

The workspace currently has these physical boundaries:

```text
apps/
  client/                 graphical Labyrinth application and composition
  server/                 headless authoritative Labyrinth application
crates/
  configs/                typed configuration
  content/                PolyTools import, validation, runtime derivation
  world_data/             protocol-neutral serializable/domain components
  simulation/             deterministic movement, aim, and Hammer systems
  network/                Lightyear protocol and client/server transport
```

The baseline is stable Rust, Bevy 0.19, and Lightyear 0.28. Bevy default
features are disabled. The server has no direct rendering, windowing, audio, or
input-device features.

In the following graph, `A -> B` means **B directly depends on A**:

```text
configs --------------------------------> simulation
world_data ----+------------------------> simulation
               +----> content ----------> simulation
               +----> network

configs + content + world_data + simulation + network ----> client/server
```

There is no direct dependency from `network` to `simulation` or `content`.
Both applications currently depend directly on all five shared crates and own
their final schedule and plugin composition.

### Current game-specific coupling

The physical boundaries are useful, but their current contents still include
Labyrinth behavior:

- `configs` contains movement, weapon aim, Hammer attack, room, camera, and eye
  values in one design configuration.
- `content` requires the current character catalog and Hammer package and
  derives Hammer combat geometry and character HP.
- `world_data` is free of Lightyear and asset parsing, but includes
  `SelectedCharacter`, `CharacterHealth`, weapon aim, and Hammer attack state.
- `simulation` contains deterministic general movement alongside gaze, weapon
  aim, and Hammer rules; its canonical step currently schedules all of them.
- `network` has separate client/server transport modules, but its internal
  protocol registration is fixed to the current join request, `PlayerInput`,
  character state, health, and Hammer state.
- `apps/client` and `apps/server` compose a concrete Labyrinth session,
  including character selection, five-player spawning, room presentation,
  health bars, eyes, and Hammer presentation.

These are facts about the starting implementation, not requirements that every
future multiplayer game must inherit.

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

### Confirmed shared direction

Raw PolyTools JSON and schema handling remain confined to `content`. Consumers
receive validated typed data; simulation consumes only semantic data it
explicitly needs, while visible mesh hierarchy and material policy remain in
presentation. A future game-owned asset contract must extend this boundary
rather than parse raw manifests elsewhere. No general asset-contract
registration API exists yet.

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
tile-based room presentation. These facilities live inside the Labyrinth client
application. There is no public reusable camera service, room service, or
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
