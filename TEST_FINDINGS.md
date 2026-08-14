# The Labyrinth — Test Findings

This document records meaningful observations from manual or automated tests. Not every slice needs an entry. Findings should capture the tested conditions, the observed behavior, and the resulting implementation or design decision.

| Slice / phase | Profile and parameters | Observation | Finding / decision |
|---|---|---|---|
| **3 / Phase 2** | `latency-jitter`; 100 ms RTT, 20 ms jitter, 0% loss; interpolation factor 1.5 | Remote movement became smoother and spontaneous strong stutters disappeared. | Jitter alone is sufficiently handled by the current interpolation path and buffer. |
| **3 / Phase 2** | `average`; 100 ms RTT, 20 ms jitter, 2% loss; interpolation factor 1.5 | Movement was generally smoother, but occasional stronger stutters remained. | Snapshot loss is the likely cause of the remaining stutters; the loss-free comparison isolated it from jitter. |
| **3 / Phase 2** | `average`; same network profile; interpolation factor 2.0 | Remote movement felt smoother again, with an acceptable additional delay. | Keep factor 2.0 as the provisional default while continuing adverse-network validation. |
| **3 / Phase 2** | `average`; same network profile | `server_late_input_mismatch` appears; it disappears with `GAME01_NETWORK_SIMULATION=off`. | Expected delayed-input correction under simulated RTT, not currently treated as a gameplay bug. |
| **3 / Phase 3** | `average`; 100 ms RTT, 20 ms jitter, 2% loss; interpolation factor 2.0; bounded extrapolation of at most two snapshot intervals; five clients | Remote movement became clearly smoother than without extrapolation. Owned movement, convergence, and five-client operation remained correct. | Bounded presentation-only extrapolation is accepted as the current packet-loss treatment; Slice 3 is complete. |
