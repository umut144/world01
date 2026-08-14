# The Labyrinth — persistent technical architecture

Last updated: 2026-08-14

## Purpose and authority

This is the technical source of truth for architecture, dependency direction, technology choices, system boundaries, and implementation-slice structure.

- `GAME_DESIGN.md` owns player-facing rules, experience, scope, and art direction.
- `ARCHITECTURE.md` owns how confirmed requirements are represented and separated technically.
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
- The server validates joins, owns player entities, runs simulation, and mutates authoritative player `Transform`s.
- The server replicates entity lifecycle, character identity, ownership data, and Transform snapshots to clients.
- A client may control only the player entity assigned to its connection.
- Disconnecting removes the server-owned player and replicates its despawn.
- A later-joining client must receive the already existing replicated players.
- Prediction and Reconciliation are explicitly excluded from the first slice.
- Snapshot interpolation is an optional isolated presentation improvement only after basic replication works; it is not part of first-slice acceptance.

## Dependency and responsibility boundaries

World data, configuration, simulation, networking, orchestration, input, and presentation are separate concerns.

### `world_data`

Owns shared protocol-neutral domain data:

- `CharacterKind`: Wizard, Mage, Sorcerer, Rogue, Glavier;
- player identity, selected character, and ownership markers;
- standard-room and spawn data;
- movement intent data passed into simulation;
- replicated gameplay components that are not transport-specific.

For the first movement slice, `Transform` is the only required movement-state component. Identity, selection, and ownership components do not count as additional movement-state modeling.

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

The initial implementation may mutate Transform directly. A mass/velocity movement model will be required soon and does not need to follow real-world physics. Keep the simulation interface and network/input flow suitable for adding explicit velocity and mass without rewriting those outer layers.

Collision is not required in the first slice, including room-boundary collision.

Phase 2 movement is represented by `MovementStep`, constructed from the typed design configuration. It supplies explicit speed and step duration to the simulation system, clamps intent to unit length, rejects non-finite intent, and directly updates `Transform` without owning fixed-tick scheduling.

### `network`

Owns Lightyear-specific concerns:

- transport configuration;
- protocol and message registration;
- connection-to-player ownership mapping;
- reliable character-selection/join messages;
- ordered/current movement-intent delivery;
- spawn, despawn, component, and Transform replication.

Networking transports intent and replicated state; it does not own movement rules.

Phase 3 uses Lightyear UDP + Netcode on loopback address `127.0.0.1:5000`. Clients bind an operating-system-selected local UDP port and receive a non-zero Netcode client ID from their first process argument (falling back to the process ID). The server admits at most five unique identities and removes connection-registry entries on disconnect.

Phase 4 adds one ordered-reliable client-to-server `JoinRequest` carrying `CharacterKind`. Connection begins only after local confirmation. The server rejects repeated joins per Netcode identity, allocates a stable `PlayerId`, chooses one of five separated spawn positions, and replicates `PlayerId`, `PlayerOwner`, `SelectedCharacter`, `Transform`, entity spawn, and despawn to all clients. Snapshot publication currently follows the configured 30 Hz Lightyear tick.

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
- rendering provisional characters and the standard room;
- presenting replicated server state.

Client presentation state is never gameplay authority.

## Input-to-simulation flow

```text
WASD / local input device
        │
        ▼
client input collection
        │  MovementIntent
        ▼
Lightyear transport
        │
        ▼
server input/ownership mapping
        │  explicit simulation input
        ▼
simulation system
        │  authoritative Transform mutation
        ▼
Lightyear snapshot replication
        │
        ▼
all client presentations
```

The fixed tick loop schedules and supplies simulation inputs. Game rules must not be embedded in the loop, input adapter, or network handler.

## Character selection and join protocol

- Selection happens locally before joining the gameplay session.
- The client requires a selected `CharacterKind` before enabling confirmation.
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
- The graphical client uses an explicit subset of Bevy's 2D features. This retains the maintained 2D rendering feature collection while excluding scene, picking, UI, audio, and 3D support.
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

- Add the five-value `CharacterKind`.
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

Implemented movement pipeline:

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

VS Code exposes the same commands through `.vscode/tasks.json`: individual `game01: server` and `game01: client 1001`–`1005` tasks, plus `game01: local slice (server + 5 clients)` to start all six processes in parallel with dedicated terminal panels.

```sh
RUSTFLAGS="-A warnings" cargo run --quiet --package game01-server
```

Start clients 1–4 first:

```sh
RUSTFLAGS="-A warnings" cargo run --quiet --package game01-client --features dev -- 1001
RUSTFLAGS="-A warnings" cargo run --quiet --package game01-client --features dev -- 1002
RUSTFLAGS="-A warnings" cargo run --quiet --package game01-client --features dev -- 1003
RUSTFLAGS="-A warnings" cargo run --quiet --package game01-client --features dev -- 1004
```

Verification sequence:

1. In every client, select a different character where practical and confirm joining. Every open client must show the same four characters at the same spawn positions.
2. Focus each client in turn and move its owned character with WASD, including a diagonal. Every open client must show only that character moving and must converge on the same authoritative position after input stops.
3. For genuinely simultaneous manual input, use multiple keyboards/operators; on a single desktop, rapidly alternating focused clients still verifies independent ownership and replication but not simultaneous key presses.
4. Close client `1002`. Its character must disappear from every remaining client without affecting the other players.
5. Start client `1005` with the command below, select a character, and join. As a late joiner it must immediately receive clients `1001`, `1003`, and `1004`; those clients must receive `1005`.

```sh
RUSTFLAGS="-A warnings" cargo run --quiet --package game01-client --features dev -- 1005
```

6. Restart client `1002` with its original command, select again, and join. All five clients must now see the same five-player set, proving reconnect and released server capacity.
7. Move all five owned characters and compare their final positions across every window. No client-side prediction is expected, so visible network latency or unsmoothed motion is acceptable in this slice.
8. Stop the server. Clients must report or otherwise reflect disconnection without crashing. Close the remaining client processes manually.

Expected server evidence includes connection, authoritative spawn, disconnect, and reconnect log entries for the corresponding identities. A failed item should be recorded with the responsible client ID, selected character, observed windows, and relevant server log line before implementation is changed.

## Open technical decisions

- Persistence architecture after the non-persistent first slice.
