# Agent entry point

Before planning, discussing, or changing this project, read
`docs/SANDBOX_VISION.md`, `docs/SANDBOX_TECHNICAL.md`, and `TASKS.md` in full.
Read `docs/PLUGIN_GUIDE.md` in full when a task concerns game plugins or public
sandbox extension contracts. Read
`docs/games/labyrinth/GAME_LABYRINTH_DESIGN.md` in full only when the task
concerns The Labyrinth.

`docs/SANDBOX_VISION.md` is the persistent source of truth for sandbox purpose,
scope boundaries, confirmed decisions, and extension principles.
`docs/SANDBOX_TECHNICAL.md` is the persistent source of truth for the
genre-neutral technical sandbox contracts. `docs/PLUGIN_GUIDE.md` explains how
game implementations extend those contracts.
`docs/games/labyrinth/GAME_LABYRINTH_DESIGN.md` is the persistent source of
truth for The Labyrinth's game-design intent, scope boundaries, confirmed
decisions, open questions, and reference-art locations. Preserve the
distinction between confirmed design and ideas still under discussion. Update
the appropriate document when the user makes a durable decision.

`TASKS.md` is the short, current project tracker. Keep only active, next,
blocked, or deliberately deferred outcomes in it. Completed implementation
history belongs in Git rather than accumulating in the tracker. A task may
still be planned and implemented as a vertical slice without preserving every
finished slice as permanent documentation.

`AGENTS.md`, `docs/SANDBOX_VISION.md`, `docs/SANDBOX_TECHNICAL.md`,
`docs/PLUGIN_GUIDE.md`, `docs/games/labyrinth/GAME_LABYRINTH_DESIGN.md`,
and `TASKS.md` are the complete canonical project-document set. Do not add
further context, planning, design, workflow, test, or architecture documents
unless the user explicitly requests one; extend the appropriate existing
document instead. Keep automated behavior in tests, durable sandbox-wide
technical decisions in `docs/SANDBOX_TECHNICAL.md`, and game-owned decisions in
the corresponding game document.

The user is a solo developer. Prefer iterative, high-leverage work and avoid prematurely solving future-season problems. Do not implement gameplay merely because it is described in the design document; implementation requires an explicit user request.

## Development Workflow & Build Rules

### Internet and data-volume discipline

- The user's connection is currently data-volume constrained. Do not perform web searches, deep internet research, broad repository downloads, or external lookups without the user's explicit permission.
- Prefer local repository files, installed dependency source, Cargo metadata, existing documentation, and direct code inspection before considering the internet.
- If external information is explicitly authorized, use the narrowest possible query and the fewest sources needed; do not open unrelated pages, fetch large assets, or repeatedly refresh equivalent information.
- Do not download dependencies, assets, tools, or updates proactively. Use already available local caches and only fetch what the requested task actually requires.
- Keep command output compact (`--quiet` where the project rules permit it), avoid dumping large logs, and inspect focused file ranges rather than entire trees.
- If current external information would materially improve the answer but no permission was given, state that limitation and ask before browsing.

### Scope and efficiency

- Inspect only files relevant to the current task.
- Make targeted, minimal diffs; do not rewrite unrelated files.
- Preserve existing user changes and avoid opportunistic refactors.
- Prefer the simplest implementation that supports the current iteration.
- Do not implement speculative future-season systems.
- Documentation-only and asset-only changes do not require Rust validation.

### Stateful gameplay planning

- For stateful gameplay work, use a compact state matrix during planning when it materially clarifies Input, Simulation, Presentation, and Constraints. Treat it as a working aid, not persistent documentation; record only resulting durable decisions in the appropriate canonical document.

### Bevy and Rust version

- Target stable Rust and Bevy 0.19 APIs exclusively.
- Do not copy patterns from older Bevy versions without verifying them against Bevy 0.19.
- Avoid deprecated APIs and broad warning suppressions in source code.
- Do not change dependency versions or Cargo features unless explicitly required by the task.

### Bevy feature discipline

- Disable Bevy default features.
- Start from Bevy 0.19's smallest suitable official feature profile; prefer the official `2d` profile for the initial graphical client unless the current implementation demonstrably needs a smaller explicit feature set.
- Enable engine features only when current implemented behavior requires them.
- Do not enable 3D, Bevy UI, audio, picking, scene, development-tool, or additional asset-format features speculatively.
- When using the `2d` profile, periodically review whether included capabilities such as scene or picking are actually needed before replacing the maintained profile with a lower-level manual feature list.
- Removing a runtime plugin is not a substitute for disabling its compile-time Cargo feature.
- Keep headless simulation/server code independent of rendering, audio, windowing, and input dependencies.
- Keep development feature sets and compiler flags consistent across routine checks and local development. Routine development commands must not add ad-hoc `RUSTFLAGS`, because each distinct flag set creates another Cargo artifact family.

### Validation

- After changing Rust code, always run the project-local validation wrapper:

  `./scripts/check.sh`

- The wrapper checks the headless server and graphical client separately, uses quiet Cargo output, and enables dynamic linking only for the client development check.
- The workspace `dev` profile is the single source of truth for development compilation settings. It disables incremental compilation to keep `target/debug/incremental` empty while retaining normal dependency artifacts for warm checks.
- Use the wrapper or its exact package/feature combinations for routine development. In particular, client development checks use `--features dev`; do not create an additional debug client variant without that feature.
- Silent output means success; do not rerun with verbose output when the wrapper succeeds.
- If the check fails, use the existing error output first. Run more verbose or targeted commands only when needed to diagnose the failure.
- Run relevant targeted tests when behavior covered by tests was changed.
- Do not automatically run the complete test suite unless the change scope justifies it.
- Never run `cargo build`, `cargo run`, release builds, benchmarks, or graphical/manual tests unless explicitly requested.
- Use dynamic linking for development validation only, never as a release requirement.

### Formatting

- Keep touched Rust code compatible with `rustfmt`.
- Avoid formatting or modifying unrelated files.
- Run `cargo fmt --check` when Rust formatting may have changed.
- If formatting is required, format only the affected scope where practical.

### Error handling

- Do not use `unwrap()`, `expect()`, or `panic!()` in ECS systems, network handlers, persistence paths, or other recoverable runtime logic.
- Handle recoverable failures with `Result`, guarded control flow, or appropriate Bevy logging.
- `unwrap()`, `expect()`, and `panic!()` are acceptable in focused tests or for genuine startup invariants whose violation makes execution impossible.
- Do not silently ignore errors.

### Architecture boundaries

- Keep gameplay/simulation rules separate from rendering, VFX, audio, input, and other presentation concerns.
- Keep simulation systems independent from the input collection, network transport, and tick-loop orchestration that invokes them; pass explicit data into simulation rather than embedding game rules in orchestration code.
- Do not make client-visible presentation state the authority for gameplay state.
- Keep player ownership and character identity explicit; do not assume there can only ever be one local or solo player.
- Solo mode is the current implementation scope, but core data structures must not inherently prevent future teams of four.
- Prefer configurable resources/components over scattered magic numbers so balancing parameters remain discoverable and tunable.
- Store explicitly requested game-design parameters in the dedicated `configs` crate. Expose only parameters the user has explicitly requested; do not automatically mirror every simulation constant into public design configuration.
- Avoid premature abstractions; introduce shared infrastructure only when required by current behavior or a confirmed architectural constraint.

### Consolidated project check command

- `./scripts/check.sh` is the authoritative routine Rust validation command.
- Keep validation logic consolidated in this lightweight wrapper rather than duplicating Cargo commands across instructions.
- Extend the wrapper only when the project gains another target that must always compile.

### Git workflow

- After every completed change, create a Git commit with a concise, descriptive message matching the actual change.
- Commit only files that belong to the current task; do not include unrelated or user-owned working-tree changes.
- Do not push. The user handles all pushes to the remote repository.
