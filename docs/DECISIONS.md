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

## Architecture Decisions

- **2026-10-23 (Phase 0): Thread boundary separation.** Audio callback, decoder, player core, and UI threads are strictly separated. The UI thread never touches audio devices, decoders, or blocking file I/O.
- **2026-10-23 (Phase 0): Fixed layout with Poppins typography.** Embedded SIL OFL Poppins font with egui custom font definitions. Light and Dark themes defined via explicit color tokens matching Section 10 of the roadmap.
- **2026-10-23 (Phase 0): Settings persistence.** Stored in OS config directory (`directories::ProjectDirs`) as `settings.toml` using `serde` and `toml`.
