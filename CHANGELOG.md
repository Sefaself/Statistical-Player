# Changelog

All notable changes to Statistical Player are documented here, organized by phase.

## [v0.0.0] - Phase 0: Foundation - 2026-10-23

### Added
- Git repository setup with `.gitignore`, `.gitattributes`, `CLAUDE.md`, and `AGENTS.md`.
- Documentation foundation (`docs/ROADMAP.md`, `docs/DECISIONS.md`, `docs/PERF.md`, `docs/phases/P0-P1.md`).
- Multi-platform GitHub Actions CI workflow for Windows, macOS, and Linux.
- Embedded Poppins fonts (`Regular`, `Medium`, `SemiBold`) with SIL OFL license.
- Clean application architecture modules: `ui`, `core`, `audio`, `library`, `lyrics`, `platform`.
- Theming engine supporting Light and Dark themes with the roadmap token palette.
- Cross-platform configuration persistence in OS config directory (`settings.toml`).
- Structured logging with `tracing` and `tracing-appender` to rotating log files and stderr.
- Crash reporting via a custom panic hook writing backtraces to the log directory.
