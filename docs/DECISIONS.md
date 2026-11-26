# Architectural & Dependency Decisions

This file records key technical choices, library additions, and justifications as required by the roadmap.

## Dependency Log

| Crate | Purpose | Why not std / alternative | Status & Justification | Added In |
| --- | --- | --- | --- | --- |
| `eframe` / `egui` | Native GUI windowing and immediate mode UI | std has no GUI; pure Rust native GUI with wgpu backend | Actively maintained, de-facto standard pure-Rust GUI | Phase 0 |
| `directories` | OS-standard directory paths (config, data, cache) | std doesn't resolve XDG / Windows Roaming / macOS Library dirs cleanly | Actively maintained, cross-platform standard | Phase 0 |
| `serde` & `serde_derive` | Serialization/deserialization for config & snapshot | std has no serde; required for TOML and state persistence | Universal Rust standard | Phase 0 |
| `toml` | Parsing and serializing `settings.toml` | Standard human-readable config format | Official TOML parser for Rust | Phase 0 |
| `tracing` | Structured application logging | std print doesn't offer levels, filters, or thread spans | Standard structured logging in Rust | Phase 0 |
| `tracing-subscriber` | Formatting & filtering tracing events to stdout & file | Needed to pipe tracing to console and file appender | Official tracing subscriber | Phase 0 |
| `tracing-appender` | Non-blocking file writer for persistent debug logs | Needed to write logs without stalling the main thread | Official tracing companion | Phase 0 |
| `anyhow` | Application-level error handling | Ergonomic error reporting with context across layers | Universal Rust standard | Phase 0 |
| `thiserror` | Strongly typed domain error enums | std error boilerplate is verbose and error-prone | Universal Rust standard | Phase 0 |
| `symphonia` | Audio decoding (FLAC, MP3, AAC/M4A, WAV, Vorbis) | std has no audio decoding; pure Rust with zero C dependencies | Actively maintained, standard pure-Rust multimedia library | Phase 1 |
| `cpal` | Low-level cross-platform audio I/O callback | std has no audio device access; WASAPI/CoreAudio/ALSA backend | De-facto standard Rust audio I/O crate | Phase 1 |
| `rtrb` | Lock-free single-producer single-consumer ring buffer | std mutex/channels allocate or lock; strictly forbidden in audio callback | Lightweight, realtime-safe wait-free SPSC queue | Phase 1 |
| `crossbeam-channel` | Multi-producer multi-consumer command/event messaging | std channels lack cross-thread select and bounded performance | Industry standard concurrent channels | Phase 1 |
| `lofty` | Audio tag and metadata parsing (ID3, Vorbis comments, MP4 atoms) | std has no tag parser; pure Rust with broad format support | Actively maintained, high-performance tagging library | Phase 1 |
| `walkdir` | Recursive folder directory walking | std::fs lacks ergonomic recursive directory walking | Standard Rust recursive directory iterator | Phase 1 |
| `ctrlc` | Cross-platform Ctrl+C signal handling for CLI subcommands | std has no cross-platform signal handling APIs | De-facto standard signal handler crate | Phase 1 |
| `rubato` | High-quality audio sample rate conversion | std has no sample rate conversion or DSP sinc/FFT resampling algorithms | Realtime-safe sinc/FFT audio resampler specified in roadmap Section 5 & 7 | Phase 1 |
| `egui_extras` | Virtualized table (`TableBuilder`) for large track queues | Core `egui` does not include a virtualized multi-column table widget | Official companion crate for `egui`, required to render 2,670+ rows at 24px smoothly | Phase 1 |
| `rfd` | Native OS folder picker dialog (`FileDialog::pick_folder`) | `std` and `egui` have no native OS file/folder dialog | Lightweight pure-Rust / native COM-backed dialog crate; invoked off the UI thread | Phase 1 |

## Architecture Decisions

- **2026-10-23 (Phase 0): Thread boundary separation.** Audio callback, decoder, player core, and UI threads are strictly separated. The UI thread never touches audio devices, decoders, or blocking file I/O.
- **2026-10-23 (Phase 0): Fixed layout with Poppins typography.** Embedded SIL OFL Poppins font with egui custom font definitions. Light and Dark themes defined via explicit color tokens matching Section 10 of the roadmap.
- **2026-10-23 (Phase 0): Settings persistence.** Stored in OS config directory (`directories::ProjectDirs`) as `settings.toml` using `serde` and `toml`.
- **2026-11-15 (Phase 1): Lock-free flush on seek and track change.** Implemented via `discard_target` atomic counter in `CallbackShared`. When `flush()` is called, `discard_target` is set to `frames_written`. The audio callback pops and drops buffered old samples until `frames_consumed == discard_target` without allocating, locking, or doing I/O. The timeline base (`timeline_base_frame`, `timeline_base_time`) maintains monotonic position calculation and continuous end-of-track detection without race conditions.
- **2026-11-15 (Phase 1): Fixed-ratio FFT resampler vs future variable-ratio resampler.** The chosen `rubato::Fft` resampler operates with a fixed sample rate ratio determined at stream creation. It provides high performance and low distortion for matching file sample rates to device hardware output rates. For Phase 9 (playback speed / pitch control: 0.5x – 2.0x in 0.05 increments), a dynamic asynchronous resampler (e.g. `rubato::Async` or polynomial interpolation) that can smoothly modulate its resampling ratio during active playback without stream reconstruction will be required.
- **2026-11-23 (Phase 1): Audio thread architecture and status synchronization.**
  1. *Status reuse & position atomic*: `AudioStatus` directly hands out the engine's `CallbackShared` Arc for peak levels, underruns, volume, and paused state (no duplication). Because `position()` requires engine-owned non-atomic fields (`timeline_base_frame`, `timeline_base_time`, device sample rate, and track duration clamp), `AudioStatus` adds an atomic `position_micros: AtomicU64` updated by the audio thread each loop.
  2. *Atomic generation & track_ended packing*: Generation (`u32`) and `track_ended` (`bool`) are packed into a single `AtomicU64` (`(generation as u64) << 32 | (if track_ended { 1 } else { 0 })`). This prevents the player core from ever reading a torn or mismatched generation/ended pair. `track_ended` is cleared immediately when a `Load` or `Seek` command is handled.
  3. *Refill observation & thread confinement*: Added `is_buffer_below_half()` and `shared()` to `AudioEngine` to expose buffer fullness without exposing internal ring buffer primitives. While `cpal::Stream` is `Send` in modern CPAL on Windows, `AudioEngine` is constructed directly on the audio thread inside `AudioThread::spawn()` and `spawn_null()` to guarantee platform-independent thread safety.
  4. *Wakeup efficiency*: While paused or stopped, the audio thread blocks on the crossbeam command channel with no timeout (0% CPU). While playing, it wakes up every 100 ms to check `is_buffer_below_half()`, refill if needed, publish position, and detect `is_track_ended()`.
