# game01 Multiplayer Sandbox — Vision

Last updated: 2026-08-29

## Purpose and audience

This document defines the product and scope boundary of the `game01` sandbox
for developers. The sandbox is the shared, runnable technical foundation for
several **multiplayer games** set in **“Secrets, Room's & Travels'”**. It
provides common runtime capabilities and asset conventions; it does not define
a particular game, genre, ruleset, or player progression.

“Sandbox” means a reusable development and integration base from which concrete
games can grow. It does not mean a disposable prototype, a standalone game, or
an attempt to build a universal engine.

Game-specific rules belong in game plugins and their design documents. The
Labyrinth reference design is deliberately isolated at
[`games/labyrinth/GAME_LABYRINTH_DESIGN.md`](games/labyrinth/GAME_LABYRINTH_DESIGN.md).

`SANDBOX_TECHNICAL.md` specifies the technical contracts. `PLUGIN_GUIDE.md`
defines how a game extends those contracts.

## Scope

The sandbox supports multiple 2D multiplayer games that can share world theme,
authored assets, networking conventions, and presentation infrastructure. A
game may use rooms, larger spaces, characters, or other entities without those
concepts implying a particular genre or gameplay loop.

The sandbox owns stable boundaries rather than genre behavior:

- Bevy application composition and ECS scheduling;
- protocol-neutral shared state and deterministic simulation entry points;
- server-authoritative networking, ownership, prediction, replication, and
  reconciliation boundaries;
- PolyTools runtime-asset import, validation, and caching;
- 2D camera framing and optional room presentation primitives;
- device-independent input actions and bindings;
- character/entity instantiation and presentation attachment.

## Extension and repository model

The repository uses `main` as the canonical shared multiplayer base. A concrete
game or game genre may be developed on a dedicated branch created from that
base. Such a branch may contain its own plugins, rules, assets, configuration,
and documentation without requiring those game-specific concerns to coexist on
`main`.

A generally useful capability may originate while building one game. When the
capability is clearly reusable, its contract is separated from the originating
game's semantics and merged back into `main`. Other game branches can then take
that shared improvement from the common base. Game-specific rules remain on
their owning branch. `main` therefore evolves through concrete needs discovered
by games, not through speculative framework work.

Within a game branch, the game is composed as one or more Bevy plugins. The
sandbox exposes narrow contracts for game plugins to:

- register game-owned components, resources, events, and schedules;
- define game input actions and map them from sandbox device state;
- register replicated state and server-side validation where networking is
  used;
- instantiate game-owned entities from validated asset identities;
- add camera/room policies and presentation systems;
- load game-local content without bypassing the shared asset boundary.

The sandbox remains the owner of shared multiplayer lifecycle, transport,
runtime-asset, and presentation boundaries. Plugins own genre semantics and
must not make client-visible state authoritative. Detailed conventions and a
minimal plugin are in [`PLUGIN_GUIDE.md`](PLUGIN_GUIDE.md).

## Decision principles

- Promote a game-originated capability to `main` only when it has concrete
  cross-game value and a stable contract free of the originating game's rules.
- Do not generalize hypothetical needs in advance; let concrete game work
  reveal useful shared behavior.
- Prefer typed, explicit data crossing boundaries over plugin access to another
  plugin's internal state.
- Keep headless simulation free of rendering, windowing, audio, and device
  APIs.
- Keep plugin-specific configuration separate from sandbox configuration.
- Preserve the existing setting and asset conventions without turning them into
  gameplay requirements.
