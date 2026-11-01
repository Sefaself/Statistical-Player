# Statistical Player

A fast, opinionated desktop music player written in pure Rust (eframe/egui, Symphonia, CPAL, and rusqlite), modeled on foobar2000's density and responsiveness with a fixed modern layout.

## How to run and test

### 1. Build
Build the debug development executable:
```bash
cargo build
```

### 2. Run GUI
Launch the desktop application window (native GUI with Poppins typography and theme toggle):
```bash
cargo run
```

### 3. Audit a Music Folder
Recursively audit an audio folder, decode ~1 second of every track, inspect AAC audio object types (LC / HE-AAC / HE-AACv2), and report errors or missing tags:
```bash
# Output to docs/audit-report.md by default
cargo run -- audit tests/fixtures

# Output to a custom report path (e.g. *.local.md which is ignored by git)
cargo run -- audit "C:\path\to\your\music" --out audit.local.md
```

### 4. Play an Audio File (CLI)
Play any supported audio file (`.flac`, `.mp3`, `.m4a`, `.wav`, `.ogg`) directly to the system default output device with live position and VU peak levels:
```bash
# Play from start (Ctrl+C to quit)
cargo run -- play tests/fixtures/flac_16bit.flac

# Seek to a specific timestamp in seconds
cargo run -- play tests/fixtures/flac_16bit.flac --seek 1.0
```

### 5. Automated Tests & Code Quality
Run all unit and integration tests (including audio pipeline, real-time zero-allocation verification, and audit tests):
```bash
cargo test
```

Run complete format, lint, and test verification:
```bash
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
```

### 6. Regenerate Test Fixtures
If needed, regenerate the 2-second stereo sine fixtures (440 Hz left, 880 Hz right) using FFmpeg:
```powershell
# Windows PowerShell
powershell -ExecutionPolicy Bypass -File tests/fixtures/generate_fixtures.ps1

# Linux / macOS
bash tests/fixtures/generate_fixtures.sh
```
