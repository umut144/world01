# game01 Sandbox — Plugin Guide

Last updated: 2026-08-28

## Purpose and audience

This guide is for developers adding a new game implementation to the sandbox.
Read [`SANDBOX_VISION.md`](SANDBOX_VISION.md) and
[`SANDBOX_TECHNICAL.md`](SANDBOX_TECHNICAL.md) first. A game plugin owns its
genre rules; it reuses sandbox services without changing their authority
boundaries.

## Set up a game plugin

1. Add a game crate or application-local module, depending on whether it needs
   an independently reusable dependency graph.
2. Define a root Bevy plugin for the game and, where useful, separate server,
   client, and shared simulation plugins.
3. Register only game-owned components, resources, events, action definitions,
   asset requirements, and systems.
4. Compose the plugin from `apps/server` and/or `apps/client`; do not make a
   game crate the owner of shared transport or asset parsing.
5. Add focused deterministic tests for game simulation and use the project
   validation wrapper after Rust changes.

Use explicit plugin names, for example `LabyrinthPlugin` or
`TacticsPlugin`; do not name a genre implementation `CorePlugin` or
`SandboxPlugin`.

## Required contracts

There is no mandatory gameplay trait or event set. A plugin implements only
the contracts needed by its game:

| Need | Plugin responsibility | Sandbox boundary |
|---|---|---|
| Simulation | Register deterministic systems that consume explicit data. | `simulation`, `world_data` |
| Input | Register action names and map action values to game intent. | client input abstraction |
| Networking | Declare replicated components/messages and validate ownership on the server. | `network` |
| Assets | Declare asset identities and typed semantic needs. | `content` |
| Presentation | Derive visuals from authoritative/predicted state. | client host and camera service |
| Camera/rooms | Provide follow, extent, topology, or transition policy when needed. | camera/room service |

For a networked game, authoritative systems must run on the server and use the
same deterministic state transition on an owning predicted client only when
that state is intentionally registered for prediction. Do not let input
collection, rendering, UI, or mesh transforms define game state.

## Minimal example: movement capability

This example intentionally demonstrates movement as a game capability, not a
sandbox rule. Names below are illustrative until the sandbox API is finalized.

```rust
use bevy::prelude::*;

#[derive(Component, Default)]
pub struct MovementIntent(pub Vec2);

#[derive(Component)]
pub struct MoveSpeed(pub f32);

pub struct ExampleMovementPlugin;

impl Plugin for ExampleMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, apply_movement);
    }
}

fn apply_movement(
    mut entities: Query<(&MovementIntent, &MoveSpeed, &mut SandboxPosition)>,
    fixed_time: Res<Time<Fixed>>,
) {
    for (intent, speed, mut position) in &mut entities {
        let direction = intent.0.normalize_or_zero();
        position.0 += direction * speed.0 * fixed_time.delta_secs();
    }
}
```

In a real networked plugin, input adaptation writes `MovementIntent`; server
validation and replication registration are added at the network boundary; the
client renders `SandboxPosition`. The system itself neither reads a key nor
sends a packet.

## Events and lifecycle

Define game-local events only where asynchronous communication is necessary,
for example `RequestSpawn`, `TurnStarted`, or `CardPlayed`. Events do not grant
authority: a server validates requests before emitting authoritative state
changes. Use components/resources for durable state and events for transient
facts.

Plugins must clean up game-owned entities and resources when their session or
state ends. Shared connection lifecycle is owned by the sandbox host; games
react to it through documented lifecycle events or components rather than
reaching into transport internals.

## Checklist

- Does the plugin contain no assumptions about another game's combat,
  progression, or room rules?
- Is every authoritative value server-owned for networked play?
- Can headless simulation compile without client/presentation dependencies?
- Are raw PolyTools files confined to `content`?
- Are device bindings outside game simulation?
- Are replicated state, messages, and ownership validation explicit?

If a required shared facility does not exist, keep the implementation local to
the game until a second concrete game proves a stable common contract.
