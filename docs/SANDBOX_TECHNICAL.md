# game01 Sandbox — Technical Specification

Last updated: 2026-08-28

## Purpose and authority

This is the technical contract for the genre-neutral sandbox described in
[`SANDBOX_VISION.md`](SANDBOX_VISION.md). It defines responsibilities and
extension boundaries, not rules for a particular game. Game-specific choices
are documented with the owning game and implemented in its plugins.

## Workspace and dependency direction

The current workspace provides these reusable boundaries:

```text
apps/
  client/                 graphical composition and presentation host
  server/                 headless authoritative composition host
crates/
  configs/                explicitly exposed, typed configuration
  content/                PolyTools import, validation, runtime derivation
  world_data/             protocol-neutral state, identities, input data
  simulation/             deterministic game-step scheduling helpers
  network/                Lightyear protocol, transport, replication adapters
```

The baseline is stable Rust, Bevy 0.19, and Lightyear 0.28. Bevy default
features remain disabled. The server must remain independent of rendering,
windowing, audio, and input-device features.

Dependency direction is deliberately one-way:

```text
content ───────┐
configs ───────┼──> simulation ──> application composition
world_data ────┘          ▲
                           │
network ───────────────> world_data
client/server ──────────> all required shared crates
game plugins ───────────> sandbox public contracts
```

`world_data` must not parse asset formats or expose transport types.
`simulation` must not read hardware input, send packets, or create visual
entities. `network` transports validated intent and state; it does not choose
genre rules. New crates are warranted only for a stable shared dependency
boundary; otherwise use modules.

## Network model

Networking is optional for a game, but networked games use a
server-authoritative model:

- clients submit input/requests, never authoritative gameplay outcomes;
- the server validates ownership and requests, runs simulation, and owns
  authoritative entity lifecycle;
- replicated components are explicit protocol-neutral data; visual `Transform`
  values are derived presentation state;
- an owning client may predict only the state explicitly registered for that
  purpose; confirmed server state reconciles it;
- remote interpolation and any presentation smoothing remain client-only;
- disconnect cleanup and late-join replication are shared lifecycle concerns.

A plugin declares its replicated components, messages, ownership checks, and
whether an entity participates in prediction/interpolation. It must keep its
simulation deterministic for server and owner-prediction reuse. No default
matchmaking, persistence, damage, or team model is part of this protocol.

## Asset pipeline

PolyTools Runtime Export is the canonical authored-asset interchange format.
The repository consumes imported copies below `assets/`, never the sibling
authoring project at runtime.

`content` is the sole parsing and validation boundary. It validates manifests,
resolves referenced runtime content, converts it to typed data, and caches the
validated result for consumers. Client presentation may convert visual geometry
to Bevy meshes; headless plugins consume only semantic data they explicitly
need. Parsed JSON, raw file paths, visual mesh hierarchy, and material policy
must not leak into simulation.

The existing PolyTools synchronization script remains the controlled import
step. A game plugin may add an asset contract and validation rules, but must do
so through `content` and only import its declared packages. Asset identity is a
stable key, not an implicit filename convention.

## Entity and character capability

The sandbox provides a generic entity-instantiation path with explicit identity,
ownership, protocol-neutral position/state, and an optional presentation root.
“Character” is a reusable presentation/capability term, not a promise of a
class system, combatant, or player avatar.

A plugin may define the components that make an entity controllable or
otherwise interactive. It supplies spawn rules, simulation behavior, and
replication registration. The client derives visual transforms and asset
attachments from authoritative, predicted, or interpolated state; no visual
component becomes gameplay authority.

## Camera and room service

The graphical host supplies a configurable orthographic 2D camera, aspect-safe
viewport calculation, and letterboxing. A room/frame service may expose
configured extents and temporary spatial presentation for games that choose to
use them.

This service does not define a grid, room adjacency, transition behavior,
collision, encounter contents, or world topology. A plugin can provide any of
those policies, including choosing not to use rooms at all. Camera-follow
selection is likewise a plugin policy expressed through the public camera
target contract.

## Input abstraction

The client collects keyboard and controller state into device-neutral action
values. The sandbox owns device sampling, binding/configuration plumbing, focus
loss safety, and conversion into tick-bound network input where applicable.

Plugins register named actions and interpret their values in their own systems.
An action can be digital, one-dimensional, or two-dimensional. Bindings must
be replaceable without exposing device APIs to simulation. The same action data
is used by local-only and networked plugins; transport adaptation occurs outside
genre logic.

## Plugin integration requirements

A game plugin must:

1. depend only on documented sandbox public contracts;
2. register game-owned schedules, components, resources, and events in its
   plugin build function;
3. keep authoritative rules in simulation-compatible systems and presentation
   in client-only systems;
4. declare any networked state and validate its server-side inputs;
5. register asset requirements through `content` rather than loading raw
   PolyTools data directly.

The exact Rust shape and a minimal example are in
[`PLUGIN_GUIDE.md`](PLUGIN_GUIDE.md).
