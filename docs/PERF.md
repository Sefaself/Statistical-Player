# Performance & Resource Measurement Log

Budget definitions and measured values per phase, following Section 16 of `docs/ROADMAP.md`.

## Budgets

| Scenario | Budget |
| --- | --- |
| Cold start to first paint (SSD, 20k tracks) | < 500 ms |
| Window visible, paused, idle | 0 repaints, < 0.5% CPU (1 core) |
| Playing, window hidden | < 1.5% CPU, <= 20 thread wakeups/sec |
| Playing, window visible, no visualizer tab | < 4% CPU (VU & waveform @ 30 fps) |
| Playing, spectrogram tab visible (30 fps) | < 8% CPU |
| Steady state memory (5,000 albums) | < 250 MB; textures <= 96 (~57 MB) |
| Audio underruns (1 hour soak) | 0 underruns |

## Phase Benchmarks & Measurements

### Phase 0: Foundation
- **Idle CPU (Paused/Window visible):** Verified 0 repaints at rest when not hovering/interacting.
- **Memory footprint (Debug/Release):** Initial shell: ~35 MB working set.
