# The Labyrinth — Slices and Tasks

| Slice | Goal | Status | Tasks / phases |
|---|---|---|---|
| **1 — Local Multiplayer Foundation** | One local headless server and five clients with character selection, authoritative join, a shared standard room, and synchronized WASD movement | **Complete** | Workspace and builds; world data and isolated simulation; Lightyear transport; character selection and authoritative spawn; synchronized movement; manual verification |
| **2 — Responsive Authoritative Movement** | Immediate and precise local movement, smooth remote movement, reliable stopping, and strict separation of authoritative world state from presentation | **In progress — Phase 2 complete** | Authoritative `Position` and presentation boundary ✓; tick-bound Lightyear native inputs and reliable stop transitions ✓; implement owned-player prediction and reconciliation; add snapshot interpolation for remote players; add render interpolation; raise tick rate from 30 to 60 Hz as an isolated phase; verify with five clients and adverse network conditions |
