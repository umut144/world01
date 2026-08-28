# game01 Sandbox — Vision

Last updated: 2026-08-28

## Purpose and audience

This document defines the product and scope boundary of the `game01` sandbox
for developers. The sandbox is a reusable technical foundation for several
games set in **“Secrets, Room's & Travels'”**. It provides shared runtime
capabilities and asset conventions; it does not define a game, genre, ruleset,
or player progression.

Game-specific rules belong in game plugins and their design documents. The
Labyrinth reference design is deliberately isolated at
[`games/labyrinth/GAME_LABYRINTH_DESIGN.md`](games/labyrinth/GAME_LABYRINTH_DESIGN.md).

`SANDBOX_TECHNICAL.md` specifies the technical contracts. `PLUGIN_GUIDE.md`
defines how a game extends those contracts.

## Scope

The sandbox supports multiple 2D games that can share world theme, authored
assets, networking conventions, and presentation infrastructure. A game may
be single-player or networked and may use rooms, larger spaces, characters, or
other entities. None of these optional concepts imply combat, a match loop, or
progression.

The sandbox owns stable boundaries rather than genre behavior:

- Bevy application composition and ECS scheduling;
- protocol-neutral shared state and deterministic simulation entry points;
- optional server-authoritative networking and replication;
- PolyTools runtime-asset import, validation, and caching;
- 2D camera framing and optional room presentation primitives;
- device-independent input actions and bindings;
- character/entity instantiation and presentation attachment.

## Minimal viable sandbox — iteration 1

Iteration 1 must establish a small, working vertical base that plugins can
reuse and test independently:

1. A Cargo workspace with headless server and graphical client entry points.
2. A Bevy ECS boundary that keeps simulation separate from input, networking,
   and presentation.
3. Optional server-authoritative entity lifecycle, ownership, and replicated
   state using the existing Lightyear integration.
4. A validated PolyTools runtime-asset pipeline that loads imported assets from
   `assets/` and exposes typed runtime data without requiring the authoring
   project at runtime.
5. A generic controllable-entity path: instantiate an entity, accept mapped
   local input, and present replicated state. Its behavior is selected by a
   game plugin.
6. A configurable 2D camera and room/frame presentation service that works
   without asserting topology, transitions, or collision semantics.
7. Keyboard and controller input mapping with a single action-level interface.

The goal is a reliable extension point, not a feature-complete engine. Existing
Labyrinth behavior may temporarily exercise this base, but does not become a
sandbox requirement merely because it already exists.

## Explicit exclusions

Iteration 1 does not own or prescribe:

- combat, health, damage, weapons, abilities, targeting, or AI;
- victory conditions, matches, rounds, quests, cards, tactics, or economic
  systems;
- progression, seasons, accounts, persistence, matchmaking, or rankings;
- room-graph generation, collapse, hazards, puzzles, portals, or traversal
  rules;
- character classes, kits, inventories, or an equipment model;
- animation systems, VFX, audio, UI polish, final materials, or final art
  direction;
- physics/collision policy beyond facilities explicitly added by a plugin.

These omissions keep the base useful to Battle Royale, MMORPG, MOBA, card, and
tactics implementations without forcing their assumptions on one another.

## Extension model

A game is composed as one or more Bevy plugins. The sandbox exposes narrow
contracts for game plugins to:

- register game-owned components, resources, events, and schedules;
- define game input actions and map them from sandbox device state;
- register replicated state and server-side validation where networking is
  used;
- instantiate game-owned entities from validated asset identities;
- add camera/room policies and presentation systems;
- load game-local content without bypassing the shared asset boundary.

The sandbox remains the owner of shared lifecycle, transport, runtime-asset,
and presentation boundaries. Plugins own genre semantics and must not make
client-visible state authoritative. Detailed conventions and a minimal plugin
are in [`PLUGIN_GUIDE.md`](PLUGIN_GUIDE.md).

## Decision principles

- Add a shared abstraction only after two current game implementations need
  the same stable contract.
- Prefer typed, explicit data crossing boundaries over plugin access to another
  plugin's internal state.
- Keep headless simulation free of rendering, windowing, audio, and device
  APIs.
- Keep plugin-specific configuration separate from sandbox configuration.
- Preserve the existing setting and asset conventions without turning them into
  gameplay requirements.
