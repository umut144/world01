# The Labyrinth — Slices and Tasks

| Slice | Goal | Status | Tasks / phases |
|---|---|---|---|
| **1 — Local Multiplayer Foundation** | One local headless server and five clients with character selection, authoritative join, a shared standard room, and synchronized WASD movement | **Complete** | Workspace and builds; world data and isolated simulation; Lightyear transport; character selection and authoritative spawn; synchronized movement; manual verification |
| **2 — Responsive Authoritative Movement** | Immediate and precise local movement, smooth remote movement, reliable stopping, and strict separation of authoritative world state from presentation | **In progress — Phase 6 complete** | Authoritative `Position` and presentation boundary ✓; tick-bound Lightyear native inputs and reliable stop transitions ✓; owned-player prediction and reconciliation ✓; snapshot interpolation for remote players ✓; presentation-only render interpolation and correction smoothing ✓; unified 60 Hz simulation, input, prediction, and snapshot cadence ✓; tune remote interpolation delay; verify with five clients and adverse network conditions |
