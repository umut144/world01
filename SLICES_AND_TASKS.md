# The Labyrinth — Slices and Tasks

| Slice | Goal | Status | Tasks / phases |
|---|---|---|---|
| **1 — Local Multiplayer Foundation** | One local headless server and five clients with character selection, authoritative join, a shared standard room, and synchronized WASD movement | **Complete** | Workspace and builds; world data and isolated simulation; Lightyear transport; character selection and authoritative spawn; synchronized movement; manual verification |
| **2 — Responsive Authoritative Movement** | Immediate and precise local movement, smooth remote movement, reliable stopping, and strict separation of authoritative world state from presentation | **Complete** | Authoritative `Position` and presentation boundary ✓; tick-bound Lightyear native inputs and reliable stop transitions ✓; owned-player prediction and reconciliation ✓; snapshot interpolation for remote players ✓; presentation-only render interpolation and correction smoothing ✓; unified 60 Hz simulation/input/prediction ✓; independent 30 Hz snapshot cadence with interpolation ratio 1.0 ✓; local five-client verification ✓ |
| **3 — Network Resilience Validation** | Evaluate prediction, reconciliation, and remote interpolation under controlled adverse network conditions | **In progress — Phase 2 implemented** | Opt-in symmetric latency/jitter/loss profiles ✓ (`latency-jitter`, `average`); automated configuration checks ✓; initial adverse-network observations recorded ✓; ratio 1.5 removed jitter-only stutters ✓; ratio 2.0 trial implemented ✓; manual five-client comparison pending; record acceptance thresholds and tune only from evidence |

## Test Findings

Not every slice requires a finding entry. Add rows when a manual or automated test produces a meaningful design or implementation decision.

| Slice / phase | Profile and parameters | Observation | Finding / decision |
|---|---|---|---|
| **3 / Phase 2** | `latency-jitter`; 100 ms RTT, 20 ms jitter, 0% loss; interpolation factor 1.5 | Remote movement became smoother and spontaneous strong stutters disappeared. | Jitter alone is sufficiently handled by the current interpolation path and buffer. |
| **3 / Phase 2** | `average`; 100 ms RTT, 20 ms jitter, 2% loss; interpolation factor 1.5 | Movement was generally smoother, but occasional stronger stutters remained. | Snapshot loss is the likely cause of the remaining stutters; the loss-free comparison isolated it from jitter. |
| **3 / Phase 2** | `average`; same network profile; interpolation factor 2.0 | Remote movement felt smoother again, with an acceptable additional delay. | Keep factor 2.0 as the provisional default while continuing adverse-network validation. |
| **3 / Phase 2** | `average`; same network profile | `server_late_input_mismatch` appears; it disappears with `GAME01_NETWORK_SIMULATION=off`. | Expected delayed-input correction under simulated RTT, not currently treated as a gameplay bug. |
