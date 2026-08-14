# Agent entry point

Before planning, discussing, or changing this project, read `GAME_DESIGN.md` in full.

`GAME_DESIGN.md` is the persistent source of truth for the current game-design intent, scope boundaries, confirmed decisions, open questions, and reference-art locations. Preserve the distinction between confirmed design and ideas still under discussion. Update it when the user makes a durable design decision.

The user is a solo developer. Prefer iterative, high-leverage work and avoid prematurely solving future-season problems. Do not implement gameplay merely because it is described in the design document; implementation requires an explicit user request.

## Development Workflow & Build Rules

### Scope and efficiency

- Inspect only files relevant to the current task.
- Make targeted, minimal diffs; do not rewrite unrelated files.
- Preserve existing user changes and avoid opportunistic refactors.
- Prefer the simplest implementation that supports the current iteration.
- Do not implement speculative future-season systems.
- Documentation-only and asset-only changes do not require Rust validation.

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
- Keep development feature sets and compiler flags consistent across routine checks and local development where practical to avoid unnecessary recompilation and duplicate artifacts.

### Validation

- After changing Rust code, always run:

  `RUSTFLAGS="-A warnings" cargo check --quiet --features bevy/dynamic_linking`

- Silent output means success; do not rerun with verbose output when the command succeeds.
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

- The direct Cargo validation command above is authoritative until the repository provides a project-local wrapper.
- When validation steps become repetitive or more complex, consolidate them into one lightweight, version-controlled command such as `just check` or `cargo xtask check`.
- Once such a wrapper exists and is documented here, prefer it over duplicating validation logic across instructions or scripts.
- Do not assume or invoke a wrapper command before it actually exists in the repository.

### Git workflow

- After every completed change, create a Git commit with a concise, descriptive message matching the actual change.
- Commit only files that belong to the current task; do not include unrelated or user-owned working-tree changes.
- Do not push. The user handles all pushes to the remote repository.
