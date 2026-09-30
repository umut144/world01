# World 01

A server-authoritative multiplayer sandbox in Rust, built on Bevy 0.19 and Lightyear 0.28. A headless server runs the simulation; graphical clients predict their own character and interpolate everyone else. The theme is "Secrets, Room's & Travels'".

This is a one-person project, started in August 2026 and still in active development. It is a foundation for later games, not a finished game.

<!-- TODO: Screenshot or short GIF: one server and two clients on `overworld01`, showing a Hammerer and a Mage in the same world. -->

## Motivation and goal

The goal is to develop the sandbox into a mature framework for multiplayer games, from which several genres (MMORPG, MOBA, RTS, battle royale, shooter and others) can be derived with little effort. The intended route is one shared engine and asset library, where a game is mostly data (rule values, maps, which Realms it uses) and not a separate Rust crate. `TASKS.md` describes that target. None of these genres is implemented yet. It also records that a MOBA and a dungeon battle royale were started and deliberately removed, because they had grown into game-shaped crates.

Nothing of that "many games" layer exists yet. What exists is the engine side: networking, deterministic simulation, content import, and one playable slice with two characters.

## What it does today

- Headless authoritative server and graphical client (windowed, 2D presentation of a world that carries elevation).
- Up to 5 clients (`MAX_CLIENTS`), joining a server on `127.0.0.1:5000`. Clients send input; the server validates and owns all gameplay state.
- Client-side prediction and reconciliation for the owning player; interpolation, bounded extrapolation and correction smoothing for remote players.
- Simulation at 60 ticks per second, state snapshots at 30 Hz (`crates/configs/runtime.toml`).
- Movement with terrain height, path ("route") surfaces with grades, water depth limits, character-to-character and character-to-world collision separation.
- Combat for two characters: the Hammerer (transforming hammer) and the Mage (converging eye beams), with health, damage, status, life and respawn at Ankh points.
- Maps authored in an external editor and imported from JSON exports, with Template composition at runtime (server composes, clients apply the replicated result).
- Ground navigation graph derived on the server, with a path query, meant for future bots.
- Keyboard and controller input (the controller via `gilrs`).

## What it does not do yet

- There is no game on top of it: no rounds, win conditions, progression, lobby or matchmaking.
- Bots do not exist. The navigation graph and a planned behaviour-tree crate (`SBX-25`) are prepared for them.
- No stable plugin API. Protocol registration is fixed to what World 01 replicates today. `docs/PLUGIN_GUIDE.md` is marked there as partly historical.
- No airborne or flying movement rules; collision is planar.
- No persistence, accounts, or remote play. The server address is hard-coded to localhost (`SBX-28`).
- Client and server must be built from the same content. The protocol ID does not fingerprint embedded maps.
- Only the Hammerer and the Mage have gameplay rules. Other characters in `assets/characters/` are authored assets without rules.
- No CI configuration is present in this repository.

## Architecture

```
            PolyTools / SceneMaker exports (assets/, embedded at build time)
                               |
   configs   design            v
      \        |           content  ----+
       \       |              |         |
        +------+---- world_data         |
                   (protocol-neutral    |
                    data, map, composition)
                          |             |
                     simulation <-------+
                (deterministic rules)
                          |
   network (Lightyear protocol + transport)
        |                                   |
   apps/server (headless, authoritative)   apps/client (Bevy window, prediction,
                                            interpolation, presentation)
```

Crate boundaries, taken from `Cargo.toml` and `docs/SANDBOX_TECHNICAL.md`:

- `world_data` holds plain serializable data and the map model. It has no Lightyear and no asset parsing.
- `simulation` holds deterministic systems that take explicit data and intent. It does not read input devices, send packets or create visuals. The server and the predicting client run the same step.
- `network` is the only place that knows Lightyear. `network` does not depend on `simulation` or `content`.
- `content` is the only place that parses PolyTools JSON. Everything else gets validated typed data.
- `design` and `configs` hold tunable design values and technical settings, loaded from TOML and JSON embedded at build time.
- The server has no rendering, windowing, audio or input features. The client owns presentation, which is never authoritative.

### Where a tester can start

- **Pure logic, no engine needed:** map import and validation, Template projection and merging (`world_data`), design data loading (`design`), PolyTools manifest validation (`content`).
- **Deterministic simulation:** movement, collision separation, navigation graph, combat, life and respawn (`simulation`). These are unit tests that build small Bevy worlds.
- **Protocol and transport:** registration and input handling (`network`).
- **Apps:** session logic in `apps/server`, and input, pose and presentation logic in `apps/client`, each with their own tests.
- **Not covered:** the real client-server connection end to end, rendering, and latency behaviour. Latency can be simulated by hand with the `WORLD01_NETWORK_SIMULATION` environment variable; there is no automated test for it.

## Tests and quality

The workspace contains roughly 360 `#[test]` functions across the library crates and both apps. The single validation entry point is `scripts/check.sh`:

```sh
./scripts/check.sh               # cargo fmt --check, cargo check for server and client
./scripts/check.sh --tests       # additionally the library crates' tests
./scripts/check.sh --apps-tests  # additionally the tests inside apps/server and apps/client
```

The script also fails if a legacy project name appears anywhere in tracked files. `scripts/check-agent*.sh` is a local helper that lets a sandboxed agent request a `check.sh` run from a watcher on the developer's machine; it is not needed to build or run the project.

There is no CI. Checks are run locally.

## Requirements, building and running

- Rust 1.95 or newer (`rust-version` in `Cargo.toml`), edition 2024.
- Developed and run on macOS. The client's Cargo features also enable X11 and Wayland, but Linux and Windows have not been tried.

Start the server, then one or more clients, each in its own terminal, from the repository root:

```sh
cargo run --package world01-server
```

```sh
cargo run --package world01-client --features dev -- 1001
cargo run --package world01-client --features dev -- 1002
```

The number is the client ID (greater than zero, one per client). `--features dev` enables Bevy dynamic linking and is meant for development builds only. Without it, use `cargo run --package world01-client -- 1001`. `.vscode/tasks.json` has ready-made tasks for a server plus five clients.

To start on a different map without editing `runtime.toml`, create `crates/configs/runtime.local.toml` (git-ignored) containing `start_map = "<scene id>"`. The available scene IDs are the file names in `assets/maps/`.

Controls (from `apps/client/src/input.rs`): `W A S D` move, `I J K L` aim, `Space` primary attack (Hammer strike, Mage eye beams; right trigger on a controller), `Q` secondary attack (left trigger), `Left Shift` run, `Left Ctrl` dash, `E` confirm after death. The secondary attack is sent to the server, but no combat rule reads it yet. The digit keys `0` to `3` are a temporary test control that places a Template on the map.

## Project structure

| Path | Contents |
|---|---|
| `apps/server` | Headless authoritative server |
| `apps/client` | Graphical client: input, prediction, presentation |
| `crates/simulation` | Deterministic movement, collision, navigation, combat, life |
| `crates/world_data` | Protocol-neutral data, map model, Template composition |
| `crates/network` | Lightyear protocol, client and server transport |
| `crates/content` | PolyTools import, validation, derived geometry |
| `crates/design` | Design data: characters, weapons, abilities, mass, traversal |
| `crates/configs` | Technical runtime settings (`runtime.toml`) |
| `assets/` | Imported characters, weapons, props, terrain, maps |
| `docs/` | Vision, technical spec, plugin guide, world design, river roadmap |
| `scripts/` | `check.sh` and export sync scripts |
| `reference_drawings/` | Hand-drawn concept art |
| `TASKS.md` | Open and deferred work |

Start with `docs/SANDBOX_VISION.md` and `docs/SANDBOX_TECHNICAL.md`. The technical document separates what is verified in code from what is only a planned direction.

## Status and next steps

The status is work in progress. The open items are in `TASKS.md`. The next ones are an `ai` crate for data-driven bots (`SBX-25`), a split of the client's `presentation.rs` (`SBX-27`), and cleaning up leftovers from the architecture review (`SBX-28`). Tilemap rendering for large maps (`WORLD-18`, `WORLD-19`) is deferred. No dates are promised.

## Credits and license

- Built with [Bevy](https://bevyengine.org/) 0.19 and Lightyear 0.28, plus `serde`, `serde_json`, `toml` and `gilrs`. These keep their own licenses.
- Characters, weapons, props, terrain and maps in `assets/` are exports from the author's own authoring tools, PolyTools (characters, weapons, props, terrain) and SceneMaker (maps). Both tools are public repositories of their own and are not part of this one. <!-- TODO: add links to the PolyTools and SceneMaker repositories. -->
- `reference_drawings/` contains the author's own concept drawings.
- `assets/textures/white_paper.png` was generated with Codex.

Copyright © 2026 Umut Coşkun. All rights reserved. The source is public for viewing. See [LICENSE](LICENSE): no use, copying or modification is permitted without written permission. Third-party components are excluded from that license.
