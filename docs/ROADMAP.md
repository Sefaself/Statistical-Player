# Statistical Player — Development Roadmap

Oct 22, 2026 · @Sefaself

## 1. Vision, scope and non-goals

Statistical Player is a fast, good-looking, cross-platform desktop music player written in Rust, modeled on its owner's personal foobar2000 layout: a folder-based album library, a 3D-style coverflow, live audio visualizations, and per-track play statistics. It keeps foobar2000's density and feel but ships one fixed, opinionated layout instead of an editable one.

**Primary goals**

- Play MP3, AAC (.m4a) and FLAC reliably, with full source precision (a 24-bit FLAC is decoded without being truncated to 16 bits). WAV, OGG Vorbis and ALAC are welcome if the decoder library handles them.
- One codebase for Windows, macOS and Linux. Windows is the development machine; the code must never become Windows-only, and macOS gets tested early rather than at the end.
- Light on resources: near-zero CPU while idle, minimized, or hidden in the tray/Dock, so it can play for hours on a MacBook battery.
- A library that fits the owner's real folders: one folder is one album, including a single giant compilation folder (see section 4).
- One fixed layout: queue on the left, coverflow on top, tabbed views in the center, and a bottom control bar with art, title, artist, transport, VU meter, time, volume and a full-width waveform seekbar.
- Count plays (at 50% of a track) and keep enough history to build a yearly recap later.

**Non-goals until explicitly promoted**

- Plugins or any extension API, and any layout editor.
- Opus support (dropped on purpose; this removes the only C dependency).
- Gapless playback, crossfade, ReplayGain, remembering the queue between sessions.
- Equalizer, low-pass and high-pass filters (later DSP phase).
- Streaming services, internet radio, YouTube, MilkDrop-style visualizers, scrobbling, tag editing.
- Exclusive-mode, ASIO or other bit-perfect driver work. "Maximum quality" means: decode at full precision and hand it to the operating system's normal audio path.
- Mobile platforms.

**Audience of this document.** It is written for an AI coding agent (most likely Claude Code) working with a developer who knows programming and Python but has never written Rust. It is a roadmap for the whole product, not an instruction to build everything at once: the build proceeds in small phases (section 19), and each phase gets its own short task document.

## 2. Working agreement for the AI agent

Build one small phase at a time, prove it works by running it, then stop and report. This section is the standing instruction set for any AI coding agent working in this repository.

### Read first

1. This document (`docs/ROADMAP.md`).
2. The file for the current phase (`docs/phases/P<n>-<name>.md`). Only that phase is in scope.
3. `docs/DECISIONS.md`, a running log of choices made along the way (create it in Phase 0).

### Standing rules

1. **Stay in the current phase.** If a task needs something from a later phase, stub it or ask the owner. Never build ahead while you are in the neighborhood.
2. **Smallest vertical slice first.** Get something visible and audible end to end, then deepen it. A working ugly version beats a half-finished clean one.
3. **Run it.** A task is not done until the app has been built and started and the phase checklist items for that task have been tried. Say which ones you could not try.
4. **Check dependency docs, do not trust memory.** Crate APIs change between versions. Use `cargo add <crate>` to get the current version and read its docs.rs page before writing code against it. This roadmap deliberately pins no versions.
5. **Justify every new dependency** in one line in `docs/DECISIONS.md`: what it does, why std or an existing crate is not enough, and its maintenance status. Prefer widely used, recently released crates.
6. **No `unsafe` in our own code and no C dependencies**, with two documented exceptions: bundled SQLite (rusqlite) and OS APIs reached through crates.
7. **Read-only on the owner's music.** The player never writes, renames, moves or deletes music files or tags in v1. Cache, database and settings live in the OS app-data directory.
8. **No network access at runtime in v1.** No telemetry, no online lookups.
9. **Never hardcode personal paths.** Library folders come from settings. Test fixtures are small generated files in `tests/fixtures`; never commit music.
10. **No `unwrap()` or `expect()`** outside tests and one-time startup invariants. Use `anyhow` for application errors, `thiserror` for error enums, and `tracing` for logs.
11. **Ask, do not guess,** when a requirement is ambiguous. Section 20 lists the known open questions; add new ones there.
12. **Keep modules small and named after their job.** A file over roughly 500 lines is a signal to split it.

### Real-time audio rules

The audio callback runs on a thread owned by the operating system and has a few milliseconds to finish. Inside it:

- no memory allocation (no `Vec` growth, `String`, `Box::new`, `format!`);
- no locks (`Mutex`, `RwLock`) and no blocking channel calls;
- no file or network I/O and no logging macros;
- no panics, so every slice access must be checked.

It may read from the lock-free ring buffer, apply volume, write to the device buffer, and update atomic counters. Everything else happens on other threads.

### Support for a Rust beginner

The owner knows programming and Python but not Rust. Prefer simple, boring Rust: owned data, channels, and `clone()` outside hot paths. Avoid clever generics and lifetime gymnastics. When a Rust concept first appears (ownership, `Arc<Mutex<_>>`, traits, `Result` and `?`), add a one-or-two-line comment or explain it in the task summary. When a compiler error is fixed, say in one sentence what it meant.

### The loop for every task

1. State the plan in about five lines.
2. Implement the smallest slice.
3. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
4. Run the app and try the checklist.
5. Commit with a clear message, one logical change per commit.
6. Report: what changed, what to try, what was not verified, what comes next.

### Starter CLAUDE.md for the repository root

```markdown
# Statistical Player
Desktop music player in Rust (eframe/egui, symphonia, cpal, rusqlite).
Read docs/ROADMAP.md, then the current phase file in docs/phases/. Work only on that phase.

## Commands
- Run: cargo run
- Check: cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test

## Hard rules
- Never allocate, lock, log or do I/O inside the audio callback.
- Never modify the owner's music files.
- No unsafe code. No new dependency without a note in docs/DECISIONS.md.
- Verify crate APIs against docs.rs for the version you add; do not rely on memory.
- Run the app before saying a task is done, and say what you could not verify.
- The owner is new to Rust: keep code simple and explain new concepts briefly.
```

Other agents read a similar file (for example `AGENTS.md`); the content can be the same.

## 3. Locked decisions and owner context

The owner has fixed the decisions in the first table; the agent must not reopen them. The second table lists proposed defaults that the agent may change, but only with an entry in `docs/DECISIONS.md`.

### Owner decisions (locked)

| Area | Decision |
| --- | --- |
| Language and GUI | Pure Rust with a native GUI. No web view. |
| Platforms | Windows (development machine), macOS and Linux. Cross-platform is mandatory, but ports follow the first stable Windows version. CI builds all three from day one. |
| Formats | MP3, AAC in .m4a, FLAC. WAV, OGG Vorbis and ALAC if the decoder supports them. Opus is excluded. |
| Quality | Decode at full precision (a 24-bit FLAC stays 24-bit through the pipeline) and use the operating system's normal audio output. No exclusive mode, ASIO or driver work. |
| Album | One folder is one album. The giant M4A compilation folder is one giant album. |
| Coverflow | Top of the window, 3D-style within reason, cover of the first track in the folder, double-click replaces the queue. Order is personalized in a settings screen with drag and sort. |
| Layout | Fixed. Queue on the left, coverflow on top, tabbed views in the center, control bar at the bottom with the waveform seekbar beneath it. The old top-right info panel is dropped; its content moves into the control bar. |
| Queue columns | Playing marker, track number, title, artist, year, length, times played. |
| Control bar | Cover, title, artist and a short format line (for example FLAC 16-bit), restart track, previous, stop, play/pause, next, randomize queue, repeat (off, track, queue), settings, horizontal VU meter, elapsed and total time, volume, waveform seekbar. |
| Center tabs | Info, lyrics, spectrogram, spectrum, oscilloscope, spectrogram plus spectrum, DSP manager. |
| Lyrics | Synced lyrics from a .lrc file with the same name next to the track. |
| Statistics | A play counts at 50% of the track. Existing foobar2000 statistics (play count, first played, last played, added) are carried over. A yearly recap and a 'play something' mode come later. |
| DSP | Playback speed also changes pitch (plain resampling). Reverb, equalizer, filters and custom presets come later; equalizer and filters are low priority. |
| Background use | Closing the window keeps music playing from the Dock or system tray. Media keys work. CPU use is minimal when hidden or minimized. |
| Not now | Plugins, layout editor, gapless, crossfade, ReplayGain, remembering the queue between sessions. |
| Process | Slow and one phase at a time. The first version is a very basic beta that proves the core works. |

### Proposed defaults (changeable with a decision note)

| Area | Default | Reason |
| --- | --- | --- |
| GUI | eframe/egui with the wgpu backend | Pure Rust, easy custom painting for visualizers and coverflow, Metal on macOS instead of deprecated OpenGL. |
| Decoding | symphonia | Pure Rust; covers FLAC, MP3, AAC-LC, ALAC, WAV and Vorbis. |
| Audio output | cpal | WASAPI, CoreAudio and ALSA/PipeWire behind one API. |
| Tags and cover art | lofty | Handles ID3, Vorbis comments and MP4 atoms, including custom fields. |
| Database | rusqlite with bundled SQLite | Statistics and recap queries are far easier in SQL. This is the one deliberate C exception to 'pure Rust'. |
| DSP math | rustfft, rubato | FFT for visuals; resampling for rate conversion and speed. |
| Media keys | souvlaki | One API for Windows, macOS and Linux. |
| Tray icon | tray-icon on Windows and macOS; ksni on Linux as the starting point | The Linux tray story differs by desktop; verify during the platform phase. |
| Font | Poppins (confirmed by the owner), with light and dark modes and a slight blue accent | Looks like the font in the owner's current layout. |
| Project shape | One crate with clear modules at first | Simpler for a Rust beginner; split into a workspace later if needed. |

### Owner context

- Experienced with programming and Python, new to Rust. Comfortable in a terminal.
- Development machine: a Windows desktop. Lowest-spec target: an Apple-silicon MacBook with 8 GB of memory, so memory and battery budgets in section 16 assume that machine.
- Current player: foobar2000 with a heavily customized layout. A screenshot of it is the visual reference (section 10).
- Library: FLAC, M4A (AAC) and MP3 albums, two artist folders that each hold many FLAC albums, and one huge mixed M4A compilation folder (section 4).

## 4. Library model and real-world edge cases

The library is a set of root folders, and every folder that directly contains audio files is exactly one album. This rule matches how the owner's music is organized, including the one giant compilation folder.

### How the owner's folders map

| Folder type | What is inside | Result in the app |
| --- | --- | --- |
| FLAC album folder | One album, FLAC files | One album |
| M4A album folder | One album, AAC in .m4a | One album |
| MP3 album folder | One album, MP3 files | One album |
| Artist folder (two of them) | Many FLAC album subfolders, no audio files directly inside | One album per subfolder; the artist folder itself is not an album |
| M4A compilation | One huge folder of mixed albums and artists, all .m4a (AAC) | One giant album: one coverflow sleeve, cover of its first track, double-click queues every track |

The library holds about 4,550 files today (including .lrc files), of which about 2,670 are in the compilation folder, and it keeps growing. Design for at least 10,000 tracks in one album and 50,000 tracks in total.

### Album rules

1. An album is a directory with at least one supported audio file directly inside it. Its identity is its canonical path.
2. Subdirectories are scanned independently. A folder with its own tracks and album subfolders yields one album for its own tracks plus one per subfolder.
3. Multi-disc layouts (`CD1`, `Disc 2`) form one album: their tracks are ordered by disc number and then track number (confirmed by the owner).
4. Mixed formats inside one folder are fine.
5. Files that are not audio are ignored, except cover images and `.lrc` lyrics files.
6. Hidden folders (names starting with a dot) and symbolic links are skipped, to avoid loops and system folders.

### Track order inside an album

- Order by disc number (a missing disc number counts as 1), then by track number. This also holds for the giant compilation folder and for multi-disc albums, where CD 1 comes before CD 2.
- Tracks that tie, or have no track number, use natural filename order: case-insensitive and numeric-aware, so `Track 2` comes before `Track 10`.
- The compilation folder is ordered by track number too, as the owner confirmed.
- The order is computed during the scan and stored, never recomputed on the UI thread.

### Album title, artist and year

- **Title:** the Album tag when at least 80% of the tracks share the same non-empty value; otherwise the folder name. The compilation folder therefore shows its folder name.
- **Artist:** the Album Artist tag when consistent, else the Artist tag when consistent, else 'Various Artists'.
- **Year:** the earliest year among the tracks when they agree within the album; otherwise empty.

### Cover art

The album cover is chosen in this order:

1. The embedded picture of the album's first track (the owner's rule).
2. An image file in the folder named `cover`, `folder`, `front` or `albumart` with extension jpg, jpeg, png or webp, matched case-insensitively.
3. A generated placeholder: the album's first letter on a color derived from a hash of its path.

Covers are decoded and shrunk off the UI thread; see section 8 for the cache.

### Real-world quirks the code must survive

- **Unicode:** all text is UTF-8 and the owner's library has Italian accents. macOS stores file names in decomposed form (NFD), so normalize to NFC for comparisons and sort keys, but keep the original path for opening files.
- **Case:** Windows and macOS file systems are case-insensitive by default and Linux is not. Match `.lrc` and cover files case-insensitively everywhere.
- **Long paths:** Windows paths over 260 characters need testing.
- **Bad files:** a file that fails to open or tag is skipped with a logged warning and counted in a 'skipped files' number in Settings. It never crashes the scan.
- **Missing tags:** title falls back to the file name without extension, artist to 'Unknown Artist'. A leading number in the file name may serve as a track number when the tag is missing.
- **Very short tracks:** a track of a few seconds is legal (the owner's soundtracks have them). Do not special-case it, except that zero-length files are skipped.
- **Huge cover images:** cap decoded size, decode off the UI thread, and never load a full-size cover just to draw a thumbnail.
- **Offline folders:** if a root folder cannot be reached (unplugged drive), keep its rows and mark them offline. Never delete database rows, above all statistics, because a folder is temporarily missing.
- **Files changing during a scan:** tolerate it; the next scan fixes it.

## 5. Tech stack and dependency policy

Use stable Rust (edition 2024) with a small set of well-known crates, each with one clear job. Symphonia 0.6 requires Rust 1.85 or newer, so install the current stable toolchain with `rustup` and keep it updated.

### Crates by job

The agent must verify each crate's current version, feature names and API on crates.io and docs.rs before using it. Names below are starting choices, not guarantees.

| Job | Crate | Notes |
| --- | --- | --- |
| Window and UI | `eframe`, `egui` | Use the wgpu backend. `egui_extras` provides a virtualized table, which suits the queue. |
| Decoding | `symphonia` | Enable codec features explicitly. By default only royalty-free codecs are on, so MP3 and AAC need their flags (or `all-codecs`). |
| Audio output | `cpal` | WASAPI on Windows, CoreAudio on macOS, ALSA/PipeWire on Linux. |
| Ring buffers | `rtrb` | Lock-free single-producer, single-consumer buffer between decoder and audio callback. |
| Resampling | `rubato` | Sample-rate conversion, and speed/pitch change by varying the ratio. |
| FFT | `rustfft` (and `realfft`) | Spectrum, spectrogram. |
| Tags and cover art | `lofty` | ID3, Vorbis comments, MP4 atoms, embedded pictures. |
| Images | `image` | JPEG, PNG, WebP decoding; resize with `image` or `fast_image_resize`. |
| Database | `rusqlite` (feature `bundled`) | SQLite compiled by Cargo, no system install. Deliberate exception to 'pure Rust'. |
| Folder walking | `walkdir` | Parallelize with `rayon` only if scanning proves slow. |
| Text handling | `unicode-normalization`, a natural-sort crate | NFC normalization; numeric-aware sorting. |
| Threads and messages | `crossbeam-channel` | Commands to the player core, events back to the UI. No async runtime. |
| Settings and paths | `serde`, `toml`, `directories` | Settings file in the OS config directory, data in the OS data directory. |
| Logging and errors | `tracing`, `tracing-subscriber`, `anyhow`, `thiserror` |  |
| Media keys | `souvlaki` | Needs the window handle on Windows; see section 15. |
| Tray icon | `tray-icon`; `ksni` on Linux | See section 15. |
| Window handle | `raw-window-handle` | Passed to souvlaki on Windows. |
| Native dialogs | `rfd` | Folder picker for library roots. |
| Shuffle and time | `fastrand`, `time` | Timestamps are stored in UTC. |
| Tests | `tempfile` | Temporary folders for scanner tests. |

Not needed and not to be added without a decision note: `tokio` or any async runtime, web frameworks, game engines.

### Dependency policy

1. Add crates with `cargo add`, never by copying version numbers from this document or from memory.
2. Commit `Cargo.lock`. Update dependencies deliberately, one at a time, in their own commits.
3. Prefer crates with a release in the last 12 months and many dependents. Note the reason for any exception in `docs/DECISIONS.md`.
4. Keep features minimal (`default-features = false` where sensible) to protect build time and binary size.
5. Run `cargo tree --duplicates` now and then and remove avoidable duplicates.

### Codec facts that shape the plan

- Symphonia decodes AAC-LC, ALAC, FLAC, MP3, Vorbis, PCM and ADPCM. Its documentation lists HE-AAC as not supported. The owner's M4A files are described as AAC; Phase 1 includes an audit tool that opens every file in a folder and reports any that fail, so an unsupported profile is found early.
- The same table marks AAC-LC as not gapless, while FLAC, MP3, ALAC and Vorbis are. Gapless is out of scope now, but expect extra work on the M4A files when it is added.
- Symphonia's AAC-LC decoder benchmarks 10 to 27% slower than FFmpeg depending on the CPU. That is irrelevant for playback and visible only if the whole compilation folder is decoded in bulk.

### Build profile (put in Cargo.toml in Phase 0)

```toml
[profile.dev]
opt-level = 1

# Dependencies such as the decoder and FFT must be fast even in debug builds,
# otherwise audio stutters while developing.
[profile.dev.package."*"]
opt-level = 3

[profile.release]
lto = "thin"
strip = true
```

### System packages

- **Windows:** the Visual Studio Build Tools with the 'Desktop development with C++' workload (needed for linking and for compiling bundled SQLite).
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Linux (Debian or Ubuntu names, a starting list that CI will correct):** `build-essential`, `pkg-config`, `libasound2-dev`, `libxkbcommon-dev`, `libwayland-dev`, `libx11-dev`, `libxcb1-dev`, plus Vulkan drivers for wgpu. Prefer souvlaki's `zbus` feature over its libdbus backend to avoid `libdbus-1-dev`.

## 6. Architecture, threading and repository layout

The UI is a thin view over a player core that runs on its own thread, so playback, play counting and media keys keep working when the window is hidden.

&#91;embedded content: threads and data flow · 9 parts\]

Commands flow from the UI and the operating system into the core. The core controls the decoder, the decoder feeds the audio callback through a ring buffer and copies audio to the analysis thread, and snapshots flow back to the UI. The callback reports progress to the core through atomic counters.

### Threads

| Thread | Owns | Must never |
| --- | --- | --- |
| UI | egui frame, view state, textures | block on files, the database or audio; decode |
| Player core | queue, play state, play counter, command routing | draw; run effects |
| Decoder and DSP | open file, decoder, resampler, effects, the writing end of the ring buffer | depend on the UI's frame rate |
| Audio callback | the reading end of the ring buffer, volume, peak atomics | allocate, lock, log, do I/O |
| Analysis | FFT state | run when no visual is visible |
| Library workers | scanning, thumbnails, waveforms, database writes | touch UI objects or wake the UI more than about 10 times a second |
| OS integration | media-key and tray callbacks | do anything except forward messages to the core |

### Messages

- **Commands** (UI or OS to core): Play, Pause, Resume, Toggle, Stop, Next, Previous, Restart, Seek, SetVolume, SetSpeed, ReplaceQueue, Randomize, SetRepeat, Quit.
- **Snapshot** (core to UI): one replaceable value holding state, queue and current index, position, duration, volume, speed, format line and messages to show. The UI redraws when the snapshot's version changes. The core wakes the UI with a stored copy of egui's `Context` and `request_repaint()` only while the window is visible.
- **Events** (one-shot): TrackChanged, PlayCounted, ScanProgress, Error.
- **Plumbing:** `crossbeam-channel` for commands and events, a triple-buffer or single-slot exchange for analysis frames, atomics for peaks and counters. The core loop uses `recv_timeout`.

### Repository layout

```text
statistical-player/
  Cargo.toml  CLAUDE.md  CHANGELOG.md
  assets/            Poppins, icons
  docs/              ROADMAP.md, DECISIONS.md, PERF.md, phases/
  tests/fixtures/    tiny generated audio files
  src/
    main.rs          starts eframe and wires the threads together
    core/            queue, state machine, play counter, commands, snapshot
    audio/           decoder, resampler, DSP, output, ring buffers, analysis, waveform
    library/         scanner, tags, database, art cache, stats, importer
    lyrics/          .lrc parser
    platform/        media keys, tray, window tricks (the only place for cfg(target_os))
    ui/              theme, queue table, coverflow, control bar, visualizers, settings
```

### Module rules

- `core`, `audio`, `library` and `lyrics` never import `egui` or `eframe`. Only `ui` and `main.rs` do. This keeps the logic testable and lets the project split into crates later.
- Dependencies point downward: `ui` to `core` to `audio` and `library`; `platform` to `core`. No cycles.
- Shared plain types (IDs, commands, snapshot) live in `core`.
- Split into a Cargo workspace only when the boundaries have settled or compile times hurt.

## 7. Audio engine specification

The engine turns a file into a continuous stream of f32 samples at the device's sample rate, with predictable latency, fast seeking, and no allocation on the real-time thread. It is plain in the first version, and gapless, DSP and visual taps must slot in later without a rewrite.

### Pipeline

1. **Open.** Probe the file with Symphonia (use the file extension as a hint), pick the first audio track, create its decoder.
2. **Decode.** Convert each decoded packet to interleaved f32. 16-bit and 24-bit integer audio convert exactly, because f32 has a 24-bit mantissa. The pipeline stays f32 from here to the device.
3. **Channel mapping.** Mono is duplicated to left and right, stereo passes through, and more than two channels are downmixed to stereo with standard coefficients (center and surrounds at -3 dB, LFE dropped).
4. **Rate and speed stage.** A `rubato` resampler runs when `source_rate x speed` differs from the device rate. When they are equal and speed is 1.0 the stage is skipped, so the samples pass through untouched.
5. **DSP stage.** Empty in v1; the effect chain of section 14 goes here.
6. **Analysis tap.** Copy samples (after DSP, before volume) into a second ring buffer for the visualizers. If it is full, drop samples. Never block.
7. **Playback ring buffer.** An `rtrb` buffer holding about 200 ms of audio. The decoder thread keeps it topped up.
8. **Audio callback (cpal).** Read samples, apply volume, write them to the device buffer in the device's native sample format (support f32, i16 and i32).

### Output device and sample rate

- Open the system default output device once and keep one stream open for the whole session at the device's default rate. Resample everything to that rate with a high-quality sinc resampler. This avoids clicks from re-opening streams between tracks.
- Use shared mode only. On Windows and macOS the OS mixer may still adjust the rate or mix other apps' sound; that is the accepted meaning of 'maximum quality' here.
- Log the path taken for every track, for example `FLAC 24-bit 96 kHz -> f32 -> resample to 48 kHz`, and keep the string available for the Info tab and control bar.
- If the default device changes or the stream reports an error, rebuild the stream and resume at the same position.
- A later option may reopen the stream at the file's own rate. It is not part of v1.

### Volume and click prevention

- Volume is a linear gain stored as f32 bits in an atomic. The slider maps position `p` (0 to 1) to gain `p x p`.
- Gain changes ramp over about 10 ms. Pause, resume, seek and stop fade out or in over about 10 ms inside the callback, so there are no pops.

### Position and timing

- The callback counts `frames_consumed` (monotonic u64 atomic). The decoder thread counts `frames_written`.
- The decoder also pushes small timeline entries (`output_frame_index`, `source_time`) about every 4096 output frames into a bounded queue. The player core thread reads the latest entry at or before `frames_consumed` and interpolates. This keeps the displayed position correct while speed changes.
- The core thread publishes the position at about 30 Hz while the window is visible and about 1 Hz while hidden.

### Flush on seek and track change

Stale audio must never be heard after a seek. Suggested mechanism (the agent may choose another, recorded in `docs/DECISIONS.md`):

1. The core thread asks the decoder to stop writing.
2. The decoder publishes `discard_target = frames_written` in an atomic, then seeks and starts writing new audio after it.
3. While `frames_consumed < discard_target`, the callback pops and drops samples (bounded per callback) and outputs silence.

### End of track and underruns

- When the decoder reaches the end of a file it records `track_end_frame = frames_written`. When `frames_consumed` passes that value, the core thread sees it by polling and starts the next track. The callback sends no messages.
- If the ring buffer runs empty while playing, output silence and increment an `underruns` atomic. The core thread logs it and the debug overlay shows it.
- Keep the track-boundary markers in the data model even though gapless is out of scope. They make gapless a small change later.

### Player states

- **Stopped:** nothing loaded.
- **Loading:** file opening, buffer filling. Moves to Playing once about 100 ms is buffered, or to Stopped with an error event.
- **Playing:** callback consumes audio.
- **Paused:** callback fades to silence. The decoder fills the buffer once and then idles.

### Queue and modes

- The queue is an ordered list of track IDs with a current index. It lives in the player core thread, not in the UI.
- **Restart** seeks to 0:00. **Previous** always goes to the previous track in play order. They are separate buttons.
- **Repeat** cycles Off, Queue, Track. With Off, the end of the queue stops playback.
- **Shuffle** (the 'randomize queue' button) is a one-shot action, not a mode: the tracks after the current one are put in random order, the current track keeps playing, and the new order simply becomes the queue. There is no toggle and no restore in v1.
- Double-clicking a queue row plays that row and leaves the queue unchanged. Double-clicking a coverflow album replaces the queue with the album's tracks in album order and starts at the first track.

### Errors and resilience

- A corrupt packet is skipped with a log line. After a run of consecutive failures, skip the track and show a short message.
- An unsupported codec shows a message naming the file. During automatic advance it skips to the next track; after an explicit play it stays stopped.
- If the output device disappears, pause and show 'Audio device lost'. Pressing Play retries opening the default device.
- After the computer sleeps and wakes, rebuild the stream if it has died.

### Targets

| Measure | Target |
| --- | --- |
| Press Play to first sound (local file) | under 200 ms |
| Seek, perceived delay | under 150 ms |
| Decoder thread CPU, 44.1 kHz FLAC, release build | under 2% of one core |
| Underruns in a one-hour playback soak | zero |

### Testing hooks

Hide the output behind a small trait (a real cpal sink and a null sink that counts frames) so that decode, resample, buffering and position math can be tested without a sound card.

## 8. Library scanner, database and tags

The UI reads everything from SQLite and never touches music files while drawing. A background scanner fills the database, so the app opens instantly on the last known library and updates as the scan proceeds.

### Where things live

Use the `directories` crate. Settings go in `settings.toml` (config directory); the database in `library.db` (data directory); thumbnails and backups in the cache and data directories. Never write next to the music.

### Scan flow

1. **Roots.** Settings lets the owner add or remove root folders with a native folder picker. The first run starts with none and shows an 'Add your music folder' prompt.
2. **Walk.** The scanner thread walks each root and collects `(path, size, mtime)` for files with a supported extension (`mp3`, `m4a`, `flac`, `wav`, `ogg`).
3. **Diff.** Compare against the database. New or changed files get their tags read; unchanged files are skipped; files no longer present are marked `missing`, not deleted.
4. **Read tags** in parallel batches (cores minus one threads, batches of about 200, one transaction per batch). Send progress to the UI at no more than 10 Hz.
5. **Derive albums.** Group tracks by directory, apply the ordering, title, artist and year rules of section 4, choose the cover, and queue thumbnail jobs.
6. **Triggers.** A manual 'Rescan' button, plus a quiet incremental scan shortly after startup. The scan can be cancelled and is safe to interrupt.

Targets: a cold scan of 20,000 tracks in under 3 minutes on an SSD; an incremental scan with no changes in under 5 seconds; first paint on a 20,000-track library in under 500 ms.

### Tag fields to read (lofty)

Title, artist, album artist, album, track number, disc number, year (from the recording date), duration, codec name, sample rate, bit depth, channel count, bitrate, presence of an embedded picture. Also check whether a `.lrc` file with the same stem sits next to the track and store its path.

Foobar2000 statistics come from an XML export that the owner supplies later; see section 9.

### Schema

Run `PRAGMA foreign_keys = ON` and `PRAGMA journal_mode = WAL`. Track the schema with `PRAGMA user_version` and apply numbered migrations in code. Never edit a migration that has shipped. Times are Unix milliseconds in UTC.

```sql
CREATE TABLE roots (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  online INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE albums (
  id INTEGER PRIMARY KEY,
  root_id INTEGER NOT NULL REFERENCES roots(id),
  path TEXT NOT NULL UNIQUE,        -- the album's directory
  title TEXT NOT NULL,
  artist TEXT,
  year INTEGER,
  track_count INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  cover_key TEXT,                   -- hash naming the cached thumbnail; NULL = placeholder
  added_at INTEGER NOT NULL,
  manual_order INTEGER              -- set by drag and drop; NULL = not placed yet
);

CREATE TABLE tracks (
  id INTEGER PRIMARY KEY,
  album_id INTEGER NOT NULL REFERENCES albums(id),
  album_index INTEGER NOT NULL,     -- position inside the album
  path TEXT NOT NULL UNIQUE,
  file_size INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL,
  title TEXT NOT NULL,
  artist TEXT NOT NULL,
  album_artist TEXT,
  album TEXT,
  track_no INTEGER,
  disc_no INTEGER,
  year INTEGER,
  duration_ms INTEGER NOT NULL,
  codec TEXT NOT NULL,
  sample_rate INTEGER NOT NULL,
  bit_depth INTEGER,
  channels INTEGER NOT NULL,
  bitrate_kbps INTEGER,
  lrc_path TEXT,
  missing INTEGER NOT NULL DEFAULT 0,
  added_at INTEGER NOT NULL         -- first seen, or imported 'Added' date
);
CREATE INDEX tracks_album ON tracks(album_id, album_index);

CREATE TABLE track_stats (
  track_id INTEGER PRIMARY KEY REFERENCES tracks(id),
  play_count INTEGER NOT NULL DEFAULT 0,
  first_played INTEGER,
  last_played INTEGER,
  imported_base INTEGER NOT NULL DEFAULT 0  -- plays imported from foobar2000; play_count = imported_base + number of play_events
);

CREATE TABLE play_events (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL REFERENCES tracks(id),
  played_at INTEGER NOT NULL,       -- moment the 50% threshold was reached
  listened_ms INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL
);
CREATE INDEX play_events_time ON play_events(played_at);

CREATE TABLE waveforms (
  track_id INTEGER PRIMARY KEY REFERENCES tracks(id),
  buckets INTEGER NOT NULL,
  peaks BLOB NOT NULL               -- min and max per bucket, one byte each
);
```

User settings such as window size, library roots and visualizer frame rate live in `settings.toml`, not in the database.

### Protecting irreplaceable data

The library tables can be rebuilt by rescanning. `track_stats`, `play_events` and `manual_order` cannot. Therefore:

- Rescans update rows by `path` and keep their `id`; they never drop a table or truncate it.
- At startup, if the newest backup is older than 24 hours, copy `library.db` to a `backups` folder and keep the five most recent copies.
- A later enhancement can re-link a moved file by matching size, duration, title, artist and album, so that moving a folder does not lose statistics.

### Cover thumbnails

- Decode the chosen cover off the UI thread, crop it to a centered square, scale it to 384 px, and save it as JPEG (quality about 85) in `cache/art/<cover_key>.jpg`. Use at most two worker threads at low priority.
- The UI keeps an LRU cache of at most 96 GPU textures and loads them lazily as sleeves approach the visible range. 96 textures of 384 x 384 RGBA is about 57 MB, which suits an 8 GB laptop.
- The control bar's small cover reuses the same thumbnail.

### How the UI reads data

- One writer connection owned by the scanner and stats code; the UI uses a read connection (WAL allows both at once).
- Keep an in-memory snapshot of album rows (a few thousand rows) for the coverflow.
- When a queue is built, load lightweight track rows (id, title, artist, year, length, play count) into memory. Ten thousand rows is about 2 MB.

### Tests

Generate tiny audio files and folder trees in a temp directory, then check the scanner for: album grouping, ordering rules, the compilation-folder case, missing files, rescans that preserve IDs, and tolerance of corrupt files.

## 9. Playback statistics and foobar2000 import

A play is counted when half of a track has been listened to, and every counted play is stored as a timestamped event so that a yearly recap can be computed later. Existing foobar2000 statistics are imported once, without ever touching the music files.

### Counting rule

- Listened time accumulates only while the player is Playing. Each position update adds the elapsed time since the previous one, and any jump larger than about twice the update interval (a seek) adds nothing.
- Listened time is measured in source time, so changing playback speed does not change what counts as half a track.
- When listened time reaches 50% of the track's duration, count one play for the current play session.
- A **play session** starts whenever a track starts from the beginning (explicit play, automatic advance, restart, or a repeat loop) and ends when another track starts or playback stops. Pausing, resuming and seeking do not start a new session, and a session counts at most once.
- There is no minimum track length. A three-second track counts after 1.5 seconds of listening.
- The tracker runs in the player core thread, so counting keeps working when the window is hidden.
- On a counted play, in one transaction: insert a `play_events` row, set `first_played` if empty, set `last_played` to now, and increase `play_count`.
- `play_count` equals `imported_base` plus the number of `play_events` rows for that track. It is stored for fast display and always updated in the same transaction.

### What the owner's foobar2000 shows

The owner's Playback Statistics panel shows Played (a count), First played, Last played and Added, with timestamps like `2026-10-05 20:36:33`. By default the foobar2000 Playback Statistics component keeps these in its own database, not in the audio files' tags; it writes tags only when synchronization is switched on or run from its context menu. Forum posts by foobar2000 users describe both the database default and the option to write to tags. The owner has confirmed that they live in foobar2000's database and can be exported as XML.

### Import plan (decided)

1. The owner's statistics live in foobar2000's own database, and the owner can export them as XML.
2. The importer is built later, when the owner hands over that XML file. The agent must inspect the real file's structure then and must not guess the layout now.
3. Until then the player counts plays from zero and stores events. A later import adds to them without double counting.

### Import source

The import source is the XML export from foobar2000 with, for each track, its file path, play count, first played, last played and added date. Tags are not used, and the music files are never touched.

Confirm the field names and the timestamp format against the real XML file. The owner's panel shows timestamps like `2026-10-05 20:36:33`.

### Import behavior

- Always run a **dry run** first and show: tracks matched, tracks not found (with a list), and tracks that already have statistics.
- Match on file path. Because the same library may live under different folder prefixes on Windows and macOS, fall back to matching on the path relative to the library root.
- Import sets `imported_base`, `first_played`, `last_played` and the track's `added_at`, then recomputes `play_count`. Running it twice does not double the counts.
- Old plays have no individual timestamps, so no `play_events` rows are invented. The yearly recap is therefore accurate from the first day of use; imported counts feed all-time totals.
- The player never writes statistics back to files in v1.

### Display

- The queue's Times Played column shows `play_count`, blank when zero (as in the owner's layout).
- The Info tab shows Played N times, First played, Last played and Added, in local time as `YYYY-MM-DD HH:MM:SS`.

### Later: recap and 'play something'

- **Yearly recap** is a full-window page built from `play_events`. Top tracks, artists and albums are `GROUP BY` counts over a year's range; hours listened is the sum of `listened_ms`. Design the page with the owner when the time comes.
- **Play something** picks a track by weighted random choice, favoring tracks never or rarely played and avoiding recently played ones. The owner will define the weights later.

## 10. UI specification: layout, theme, components

The window has one fixed layout of five regions, taken from the owner's foobar2000 screenshot minus its top-right info panel, whose content now lives in the control bar.

&#91;embedded content: window layout · 5 regions\]

### Regions

| Region | Content | Size |
| --- | --- | --- |
| Queue | The track list | About 29% of the window width, resizable with a splitter, at least 320 px |
| Coverflow | Album sleeves | About 28% of the right-hand height, at least 200 px |
| Tab bar and viewer | Info, Lyrics, Spectrogram, Spectrum, Oscilloscope, Spectrum + Spectrogram, DSP | The rest of the right-hand area |
| Control bar | Now playing, transport, VU, time, volume | 56 to 64 px high |
| Waveform seekbar | Full-width waveform | 72 px high, full window width |

Splitter positions and window size are remembered. Minimum window size is about 900 x 600.

### Control bar, left to right

1. **Now playing:** cover (56 px square), title in semibold, artist below it, and a short format line in quiet text, for example `FLAC 16-bit`. For lossy files the planned line is the codec and bitrate, such as `AAC 256 kbps`; the owner has not confirmed that.
2. **Buttons:** restart track, previous, stop, play/pause, next, randomize queue, repeat (cycles Off, Queue, Track, with the icon showing the state), settings. Stop returns to 0:00 and keeps the queue.
3. **VU meter:** horizontal, labeled FL and FR (section 12).
4. **Time:** elapsed and total, for example `0:10 / 2:35`, in a fixed-width cell so it does not jitter.
5. **Volume:** slider; clicking its icon mutes.

### Queue

- Columns: playing marker, track number, title, artist, year, length, times played. Times played is blank when zero.
- Double-click plays that row. The playing row is highlighted and scrolled into view when the track changes, unless the owner scrolled in the last 5 seconds.
- Compact rows (about 24 px), virtualized: 10,000 rows scroll at 60 fps. Column widths are resizable and remembered.
- The queue order is the play order, so header clicks do not sort in v1. Drag-to-reorder, multi-select, removal and a context menu come later. There is no search or filter bar in v1.

### Center tabs

- **Info:** large cover, title, artist, album, year, track and disc, format details (codec, bit depth, sample rate, bitrate), file path and size, the signal-path line from section 7, and the statistics: played N times, first played, last played, added.
- **Lyrics, Spectrogram, Spectrum, Oscilloscope, Spectrum + Spectrogram:** sections 12 and 13.
- **DSP:** section 14.
- The last selected tab is remembered.

### Theme

Light and dark modes with a slight blue accent. A setting offers System, Light and Dark, defaulting to System. The values below are starting points estimated by eye from the owner's screenshot; refine them against the running app.

| Token | Dark | Light |
| --- | --- | --- |
| Window background | `#181818` | `#f4f5f7` |
| Panel | `#1f1f1f` | `#ffffff` |
| Raised (hover, selection) | `#2a2a2a` | `#e9ebef` |
| Primary text | `#e8e8e8` | `#1b1d21` |
| Secondary text | `#a0a0a0` | `#5b6270` |
| Accent | `#4fa3ff` | `#1a73e8` |
| Waveform, unplayed | `#8a8a8a` | `#9aa0aa` |

Spectrum bars and the played part of the waveform use the accent, with a lighter tint at the top of the bars.

### Typography, icons, spacing

- **Font:** Poppins (Regular, Medium, SemiBold), bundled in the executable. Body and queue rows 13 px, title 15 px semibold, time readout about 20 px, active lyric line about 28 px.
- **Icons:** one open-license outline icon set (Phosphor or Material Symbols, for example), bundled.
- **Spacing and shape:** 4 px grid, 6 px corner radius, no shadows, an accent focus ring.
- **Scaling:** follow the operating system's display scale; test 100% to 200%.

### Settings window

Sections: Library (roots, rescan, skipped-file count), Album order (Alphabetical or Manual, with the drag list), Appearance (theme), Playback (closing behavior, visual frame rate, battery saver), About (version, licenses).

### Keyboard shortcuts (inside the app)

Space plays or pauses. Left and Right seek by 5 seconds. Up and Down change volume by 5%. Ctrl or Cmd+Q quits. F12 toggles the debug overlay. Media keys come from section 15.

### Empty and busy states

- No library yet: a centered prompt, 'Add your music folder'.
- Scanning: a thin progress line at the bottom of the queue and a count in Settings.
- Errors: a small message above the control bar that fades after a few seconds.
- Nothing playing: the control bar shows 'Nothing playing' and its buttons are disabled where meaningless.

## 11. Coverflow specification

The coverflow is a horizontal strip of album sleeves. The selected album faces the viewer in the center, the others fan out on both sides in perspective, and scrolling is smooth. Phase 2 ships a flat version of the strip; Phase 8 adds the 3D look of the owner's current layout.

### Behavior

- One `selected` position (a float, so it can animate). It is independent of the playing album.
- **Mouse wheel** (vertical or horizontal) moves the selection one album per notch with an eased animation. **Click and drag** scrolls with a little inertia. **Left and Right** step by one, **Home and End** jump to the ends.
- **Single-click** a side sleeve centers it. **Double-click** the center sleeve, or press Enter, replaces the queue with the album's tracks in album order and starts the first track.
- A caption under the center sleeve shows the album title, with artist and year in smaller text below.
- A thin draggable scrubber under the strip lets the owner cross thousands of albums quickly.
- Hovering a sleeve shows a tooltip: title, artist, track count, total duration.
- Later: type-a-letter jump, a 'show playing album' command, a context menu.

### Geometry (starting values; tune by eye against the owner's layout)

Let `S` be the sleeve size and `d` an album's offset from the selection (negative on the left).

- The center sleeve is square with `S` about 80% of the strip height, leaving room for the caption.
- Rotation about the vertical axis grows with `|d|` and saturates: about 65 degrees at `|d| >= 1`, interpolated between 0 and 1.
- The first neighbors sit about 0.6 S from the center. Further sleeves step outward by about 0.2 S each so only a slice of each shows.
- Sleeves farther from the center are pushed back in depth and project smaller. Use a simple perspective divide with the camera at about 3 S from the center plane.
- Brightness falls with the angle (about 55% at the extremes), which gives the stacked-cards look.
- Draw from the outermost sleeves inward so the center is on top.
- Draw about 7 sleeves on each side in full. Beyond that, optionally draw 1 to 3 pixel slivers in the cover's average color, like the dense stack at the edges of the owner's current coverflow.

### Drawing technique

Start with egui meshes: every sleeve is a set of 8 vertical quads whose corners are projected from 3D positions, with linear texture coordinates and a brightness tint. Eight strips keep the distortion small for this amount of tilt. Only if the result is visibly warped, move to a wgpu paint callback with a small shader that is perspective-correct. Do not start there.

### Animation and repaint

- The selection moves toward its target with an ease-out curve over about 250 ms, or a critically damped spring. Several wheel notches in a row update the target.
- Ask egui to repaint only while something is moving. At rest the strip must cause zero repaints.

### Textures

- Request thumbnails for the visible sleeves plus about 6 on each side. Cancel requests for sleeves that scrolled away.
- While a thumbnail loads, draw the placeholder (section 4). Never decode or read files on the UI thread.
- Keep at most 96 textures (LRU), as in section 8. Cover-fit cropping to a square happens when the thumbnail is made, not while drawing.

### Album order

The owner wants the order of albums to be personal, edited in a settings menu by dragging.

- The order is stored in `albums.manual_order`. Albums without a position go after the placed ones, sorted by title.
- **Settings, Album order** shows a virtualized list: thumbnail, title, artist, drag handle. Drag to reorder, with automatic scrolling near the edges.
- An Order option switches between Alphabetical (by album title, ignoring case and accents) and Manual. Choosing Manual starts from the current alphabetical order and lets the owner drag albums. Further presets (by year, most played) can come later.
- Renumber positions in one transaction after each drop. With thousands of albums, add 'move to top' and 'move to position' actions later because long drags are tiring.
- The owner confirmed both options. Phase 2 ships Alphabetical; Phase 8 adds Manual.

### Acceptance criteria

| Measure | Target |
| --- | --- |
| Scrolling frame rate with 5,000 albums | 60 fps on the Windows dev machine, at least 30 fps on the 8 GB MacBook |
| Repaints while idle | none |
| File or image decoding on the UI thread | none |
| GPU textures alive | 96 or fewer |
| Double-click to sound, including the 10,000-track compilation | under 200 ms |
| First frame of a new thumbnail after scrolling stops | under 300 ms |

## 12. Visualizations, waveform seekbar and VU meter

Every visual is computed from audio the engine already produces, runs only while it is visible, and shares one analysis pass. The goal is the look of the owner's foobar2000 views at a fraction of their CPU cost.

### Shared rules

- **Taps.** The audio callback computes per-block peak levels for left and right (before volume) into atomics. That alone drives the VU meter, with no analysis thread. The decoder thread also copies samples (after DSP, before volume) into the analysis ring buffer for the FFT and oscilloscope.
- **Analysis thread.** Runs only while a spectrum, spectrogram or oscilloscope is visible, the window is visible, and playback is running. Otherwise it sleeps.
- **Publishing.** The analysis thread hands the UI its newest frame through a single-slot or triple-buffer exchange. The UI never waits for it and never copies large buffers per frame.
- **Frame rate.** Visuals redraw at 30 fps while the window is focused, 15 fps while visible but unfocused, and not at all while minimized or hidden. A setting offers 15, 30 or 60 fps and a battery-saver mode (15 fps, spectrogram paused).
- **Pause.** When paused, bars decay to zero and then all visual repaints stop.
- **Colors.** Blue accent and light-blue gradients as in the owner's layout; exact values in section 10.

### Parameters at a glance

| Visual | Window and update | Scale and look |
| --- | --- | --- |
| Spectrum | 4096-sample Hann window over the latest mono mix, once per frame | About 64 log-spaced bars from 30 Hz to 18 kHz (capped at Nyquist); 0 to -60 dB with grid lines every 10 dB and labels on the right |
| Spectrogram | 2048-sample window, one new column per frame | Log frequency axis, grayscale, about 70 dB of dynamic range, scrolls right to left |
| Oscilloscope | Latest 2048 frames, aligned on a rising zero crossing | Left and right overlaid in two colors, fixed full-scale amplitude |
| VU meter | Per-block peaks from the callback | Horizontal bars labeled FL and FR, -60 to 0 dB in 10 dB steps |

### Spectrum

1. Mix left and right to mono, apply a Hann window, run a real FFT.
2. Normalize so a full-scale sine reads 0 dB, then convert magnitudes to dB.
3. Map FFT bins to the bars. Where several bars share one bin at low frequencies, interpolate between bins. Where a bar covers many bins, take the maximum.
4. Smooth over time: instant rise, fall of about 30 dB per second. An optional peak-hold cap falls slowly.
5. Draw filled rectangles with a vertical gradient, plus the dB grid.

### Spectrogram

- Each frame appends one column: compute the FFT, map bins to rows with a precomputed bin-to-row table (log frequency, max or linear interpolation), convert dB to a byte, and write it into a circular column buffer.
- **Efficient drawing.** Keep the image in a GPU texture as wide as the visible area. Update only the new columns each frame (a partial texture update) and draw the texture as two quads with a moving UV offset, instead of shifting the whole image.
- Clear the image on track change and rebuild it on resize. A seek may clear it too.
- A whole-track mode (like a precomputed image of the full song) is not in v1. The owner confirmed that real-time scrolling is what he wants.

### Oscilloscope

Take the latest 2048 stereo frames (about 43 ms at 48 kHz). Search the first 1024 samples of the mono mix for a rising zero crossing and start drawing there, so the trace stays steady. Draw polylines with 1.5 px lines.

### Spectrogram plus spectrum tab

One tab shows both: the spectrogram on the left (about 60% of the width) and the spectrum on the right. They share one FFT per frame.

### VU meter (control bar)

- Two horizontal bars, left over right, labeled FL and FR, with a -60 to 0 dB scale.
- The audio callback writes the peak of each block into `peak_l` and `peak_r` atomics (f32 bits). The UI reads them each frame.
- Ballistics happen in the UI: instant attack, release of about 20 dB per second, and a peak-hold tick that stays about 1 second.

### Waveform seekbar

**Data.** A fixed number of buckets per track (4096), each holding the minimum and maximum sample scaled to one byte each. That is about 8 KB per track.

**Computation.** A worker decodes the track once, ignoring volume and DSP, and fills the buckets. Compute the current track as soon as it starts and the next queued track in the background. Never precompute the whole library. Cache results in the `waveforms` table. A 3-minute FLAC should finish in well under a second in a release build.

**While computing,** draw a flat dim line. No animation, to save power.

**Drawing.** Render the waveform once into a texture (white on transparent) whenever the data or window width changes, then draw it twice per frame with clip rectangles: gray for the unplayed part and the accent blue for the played part. Add a thin playhead line. This costs almost nothing per frame.

**Layout.** Full window width, 64 to 80 px high, centered mirrored shape, directly under the control bar.

**Interaction.** Click or drag to seek. During a drag, move a preview line and send at most about 10 seek commands per second, with a final one on release. Hovering shows a vertical line and a time tooltip.

**Re-bucketing.** For width `W`, combine neighboring buckets with min and max when there are more than `W`, and stretch when there are fewer. Cache the result per width.

### Acceptance criteria

| Measure | Target |
| --- | --- |
| Spectrogram tab visible, 30 fps, release build, Windows dev machine | under 8% of one core |
| Paused, window visible | no repaints and under 0.5% CPU |
| Window minimized or hidden while playing | no visual work at all; analysis thread asleep |
| Waveform of a 3-minute FLAC ready after the track starts | under 1 second |
| Seek by clicking the waveform | sound resumes at the new position within 150 ms |

## 13. Lyrics (.lrc)

Lyrics come from a synced `.lrc` file next to the track with the same name. The Lyrics tab highlights the current line and scrolls along with the music.

### Finding the file

- For `Song.flac`, look for `Song.lrc` in the same folder. Compare the file name without extension case-insensitively.
- Check during the scan (store `lrc_path`) and again when a track starts, in case the file was added later.
- Decode as UTF-8 and strip a BOM. If the bytes are not valid UTF-8, try UTF-16 when a BOM says so, and otherwise Windows-1252, so Italian accents survive (`encoding_rs` is the likely crate).

### Format to parse

- A line is one or more timestamps followed by text: `[mm:ss.xx]`, `[mm:ss.xxx]`, `[mm:ss]` and `[m:ss.x]` all occur. Minutes may exceed 59.
- Several timestamps can share a text: `[00:12.00][01:30.50]Chorus`.
- Header tags `[ar:]`, `[ti:]`, `[al:]`, `[by:]`, `[length:]` are read and ignored for display. `[offset:+/-ms]` is applied. Players disagree on its sign, so test with one of the owner's files that uses it and keep the sign in a single constant.
- Word-level tags like `<mm:ss.xx>` are stripped in v1.
- Blank text after a timestamp is valid (an instrumental gap) and is kept as an empty line.
- Handle CRLF line endings. Sort lines by time with a stable sort. Ignore malformed lines without failing.
- If the file has no timestamps at all, show it as plain scrolling text without highlighting.

### Display

- The active line is the last line whose time is at or before the current position, found by binary search.
- Show a vertical list. The active line is large and bright, neighbors are dimmer, and the active line eases to the vertical center in about 200 ms.
- If the owner scrolls manually, pause auto-follow for 3 seconds, then re-center. Clicking a line seeks to its time.
- Long lines wrap. Sizes scale with the tab height.
- With no file, show 'No lyrics file for this track' and the expected file name.
- Update at 30 Hz only while the Lyrics tab is visible and playing. The scroll animation runs only when the active line changes.

### Later

Embedded lyrics tags, online lookup, word-by-word highlighting.

### Tests

Parser tests for multiple timestamps, offset, BOM, CRLF, missing decimals, blank lines, malformed lines and a file without timestamps.

## 14. DSP: speed and pitch first, effects later

Version 1 has exactly one DSP feature, playback speed with pitch following it. Everything else (reverb, echo, equalizer, filters, presets) arrives later through a fixed chain that is designed now but not built.

### Playback speed (first DSP phase)

- **Control:** slider from 0.50x to 2.00x in steps of 0.01x. Double-click resets to 1.00x. Preset buttons for 0.75x, 1.00x, 1.25x and 1.50x. Show the pitch change in semitones (`12 x log2(speed)`) next to it.
- **Implementation:** vary the resampling ratio in the rate stage of section 7: `ratio = device_rate / (source_rate x speed)`. Check the current `rubato` documentation for which resampler types allow changing the ratio while running, and what relative range is allowed.
- **No clicks:** change the ratio smoothly, or fade the output for 10 to 20 ms around the change.
- **Latency:** the new speed is heard after the already-buffered audio plays out (about 200 ms). If that feels slow, flush and refill the buffer on a speed change with a short fade. Record the choice in `docs/DECISIONS.md`.
- **Position:** all displayed time (position, waveform, lyrics, play counting) stays in source time, as in sections 7 and 9.
- **Persistence:** speed returns to 1.00x at every launch. Confirm with the owner later whether it should persist.
- **DSP Manager tab (v1):** the speed slider, a reset button, and a status line showing the signal path, for example `FLAC 24-bit 96 kHz -> f32 -> resample 48 kHz at 1.00x`.

### The chain (designed now, built later)

Source, channel mapping, **speed resample**, equalizer, filters, echo, reverb, limiter, analysis tap, ring buffer.

- The chain runs on the decoder thread in blocks of 1024 to 4096 frames, never in the audio callback, so a slow effect can only delay buffering and never glitch the output.
- Parameter changes arrive over a channel and apply at block boundaries with short smoothing.
- Buffers are allocated when an effect is created, not while processing.
- With DSP switched off, the stages are bypassed completely.
- Flush denormal numbers to zero so filters and reverb tails do not slow the CPU.

### Later effects, in this order

1. **Reverb.** A classic algorithmic design (parallel comb filters into series all-pass filters) with room size, damping and wet/dry mix. No convolution.
2. **Echo.** A feedback delay with delay time 50 ms to 1 s, feedback 0 to 90%, wet/dry mix.
3. **Equalizer (low priority).** Ten graphic bands at 31, 62, 125, 250, 500 Hz and 1, 2, 4, 8, 16 kHz, plus or minus 12 dB, built from biquad peaking filters, with a preamp.
4. **High-pass and low-pass filters (low priority).** Butterworth biquads from 20 Hz to 20 kHz, 12 or 24 dB per octave.
5. **Limiter.** A soft limiter at the end of the chain whenever any effect can add gain.
6. **Presets.** Named presets stored in `dsp_presets.toml` in the config directory. Each lists which effects are on and their parameters. The DSP tab has a preset dropdown, Save As, Delete, and one master on/off switch (like 'DSP Disabled' in the owner's layout).

### Targets and tests

- All effects together under 3% of one core at 48 kHz stereo.
- Tests: filter frequency response against known values, reverb and echo tails that decay to silence, no NaN or infinity under silence or full-scale input, bypass producing bit-identical output.

## 15. Platform integration: media keys, tray and Dock, background mode

The player keeps playing when its window is closed, answers the keyboard's media keys, and uses almost no CPU while hidden. This forces one architectural rule from the first phase: the player's brain must live outside the UI.

### The rule

A hidden or minimized window usually stops running its draw loop. So the queue, the play state machine, automatic track advance, play counting, media-key handling and tray handling all live in the player core thread and other non-UI threads. The UI is a view: it sends commands and renders snapshots. Nothing the music needs may wait for the UI's `update()` function to run.

### Media keys and OS media widgets (souvlaki)

- **Windows:** the system media overlay and media keys. souvlaki needs the window's HWND, so create the controls after eframe has made its window and take the raw window handle from the creation context.
- **macOS:** Control Center, Now Playing and media keys. souvlaki's current documentation says it needs an AppDelegate or winit event loop but not an open window (older notes said a window was required). Verify on the installed version, and verify the keys still work while the window is hidden.
- **Linux:** MPRIS over D-Bus, shown in GNOME and KDE media controls and testable with `playerctl`. Prefer souvlaki's `zbus` backend.
- **Events handled:** Play, Pause, Toggle, Next, Previous, Stop, and Seek or SetPosition where the OS offers them. Each becomes a command to the core thread.
- **Metadata:** on every track change set title, artist, album, duration and cover (the cached thumbnail; MPRIS wants a `file://` URL). Push playback state on change and position about once per second, not at 30 Hz.
- souvlaki's callback runs on its own thread. Forward events through a channel to the core thread and never touch UI objects from it.

### Closing the window

- Intercept the close request: cancel the close and hide the window. Quit only from the tray menu, the app menu (Cmd+Q on macOS), or an explicit Quit item. A setting 'Closing the window keeps playing in the background' is on by default. Check the current egui viewport command names (`CancelClose`, `Visible`, `Focus`, `Minimized`).
- **Windows and Linux:** a tray icon. Menu: Show or hide window, Play/Pause, Previous, Next, Quit. Tooltip shows `Artist - Title`. A left click toggles the window.
- **macOS:** the Dock icon is the handle. Closing the window hides it while playback continues, and clicking the Dock icon must bring it back. winit may not report the Dock 'reopen' event; if so, minimize the window to the Dock on close instead, which is the normal Mac behavior. A menu-bar icon can come later.
- **Linux without a tray:** GNOME hides tray icons unless an extension is installed. If no tray host is found, minimize instead of hiding, and tell the owner once.
- **Waking from the tray:** tray events arrive on non-UI threads. Give the tray thread a clone of egui's `Context` and call `request_repaint()` plus the show and focus viewport commands so the hidden window's event loop wakes. Test this on every OS, because a hidden window may not process events.
- When the window is shown again, it renders the latest snapshot immediately.

### Power rules while hidden

- Share `window_visible` and `window_focused` as atomics. Visual code, the analysis thread and thumbnail or waveform workers read them.
- Hidden or minimized: no visuals, analysis thread asleep, position updates once per second, background image and waveform jobs paused. A scan the owner started keeps running.
- No busy loops. The core loop blocks on its channel with a timeout of about 50 ms while playing and 250 ms while idle.
- Later option: if paused for more than 30 seconds, close the output stream so the audio hardware can sleep, and reopen it on Play.

### Per-OS notes

| Topic | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Media controls | System media overlay via souvlaki with HWND | Now Playing and keys via souvlaki | MPRIS via souvlaki (zbus) |
| Hidden-window handle | Tray icon (`tray-icon`) | Dock icon, minimize if reopen is not reported | Tray via `ksni`, fallback minimize |
| Audio | WASAPI shared mode through cpal | CoreAudio through cpal | ALSA through cpal; PipeWire and PulseAudio supply an ALSA layer |
| Distribution | Plain exe plus installer later | `.app` bundle, code signing and notarization to share with others | AppImage or similar later |
| Main risk | Long paths, antivirus false positives | Dock reopen, signing | Tray availability, Wayland quirks |

## 16. Performance and battery budget

These budgets come from the owner's goal of running quietly on a MacBook battery. They are starting hypotheses to measure and revise after each phase, not promises. Record every measurement in `docs/PERF.md`.

### Budgets

| Scenario | Budget |
| --- | --- |
| Cold start to first paint, 20,000-track library, SSD | under 500 ms (the UI shows the last saved snapshot) |
| Window visible, paused, nothing animating | zero repaints, under 0.5% of one core |
| Playing, window hidden | under 1.5% of one core on the Windows dev machine, at most about 20 thread wakeups per second |
| Playing, window visible, no visualizer tab (VU and waveform at 30 fps) | under 4% of one core |
| Playing, spectrogram tab visible at 30 fps | under 8% of one core |
| Memory at steady state with 5,000 albums | under 250 MB; textures 96 or fewer (about 57 MB); a 10,000-row queue about 2 MB |
| Audio underruns | zero in a one-hour soak |
| MacBook battery test: one hour of hidden playback of the same FLAC | not noticeably worse than a mainstream native player playing the same file |

### Techniques

1. **Reactive drawing.** Never request a repaint unconditionally every frame. Repaint on input, or ask for a repaint after a delay (`request_repaint_after`) while something animates.
2. **Capped visual frame rates** as in section 12.
3. **No per-frame allocations in draw code.** Reuse vectors and mesh buffers. A debug counter should show allocations per frame near zero.
4. **Cache static drawings as textures** (waveform, thumbnails).
5. **Thread hygiene.** Block on channels with timeouts; no sleep loops shorter than 10 ms; no busy waiting.
6. **Decode only what is needed.** The ring buffer holds about 200 ms and the decoder sleeps while it is full.
7. **Database off the UI thread.** The UI reads in-memory snapshots, as in section 8.
8. **GPU settings.** Choose the low-power GPU preference on laptops and a vsynced present mode, so no frames are wasted. Check the current eframe and wgpu option names.
9. **Build profile** as in section 5 (optimized dependencies in debug builds, thin LTO in release).

### Built-in instrumentation (from Phase 1)

A debug overlay toggled with F12 showing: frame time (average and worst), repaints per second, decoder buffer fill, underrun count, analysis frames per second, texture count, and slow database queries. Log any frame over 16 ms at debug level with `tracing`.

### How to measure

- **Windows:** Task Manager and Resource Monitor, with the app minimized.
- **macOS:** Activity Monitor, Energy tab (Energy Impact and Idle Wake Ups), and Instruments if needed.
- **Linux:** `top` and `powertop`.
- Compare against a baseline: the same file playing in another player on the same machine.

## 17. Testing and quality

Test automatically the logic that can break silently (parsing, ordering, counting, position math, scanning) and test sound and feel by hand with a checklist per phase. CI cannot play audio, so automated tests use a null output.

### Automated tests

| Layer | What to cover | How |
| --- | --- | --- |
| Unit | LRC parser, natural sort and track ordering, album title and artist rules, play-count tracker (pauses, seeks, repeats), volume curve, resample-ratio math, spectrum bin mapping, waveform re-bucketing, foobar import parser and path matching | Plain `#[test]` functions beside the code |
| Scanner and database | Album grouping, the compilation-folder case, missing files, rescans that keep IDs, migrations from empty and from the previous version, corrupt files | Generated folder trees in a temp directory |
| Audio pipeline | Decode, resample and buffer into the null sink: frame counts and durations within one frame, channel mapping, no clipping; seek accuracy (decoding from 10.000 s matches the same samples of a full decode) | Generated fixture files |
| Real-time safety | The callback function performs no heap allocation | Run the callback on a fake buffer under an allocation-counting allocator and assert the count stays zero |
| UI smoke | Every panel draws without panicking for empty, small and huge data | Run egui headless with `Context::run` |
| Robustness | Parsers and the scanner never panic on junk input | Random bytes saved as `.mp3`, `.flac`, `.m4a`, `.lrc`; optionally `proptest` |

### Fixtures

Use tiny files generated by a script, never copyrighted music: one to three seconds of sine tones, with distinct tones in the left and right channels, as FLAC 16-bit, FLAC 24-bit, MP3, AAC in M4A, WAV, plus one truncated file and one empty file. Each stays under about 100 KB. Commit the generator script (ffmpeg or sox, owner's choice) and the results. If the owner can produce a short HE-AAC sample, add it to confirm it fails gracefully.

### Design for testability

- Keep UI state changes in plain functions that do not touch egui, so they can be tested without a window.
- Hide the audio output behind a trait (section 7) and the clock behind a small trait where timing matters.

### CI

On every push: format check, clippy with warnings as errors, and the full test suite on Windows, macOS and Linux (section 18).

### Manual regression checklist (every phase adds items)

1. Play MP3, AAC M4A, FLAC 16-bit 44.1 kHz, FLAC 24-bit 96 kHz and WAV. Durations are right and there are no clicks at start or end.
2. Seek to the start, middle and end; seek rapidly; seek past the end.
3. Next, previous, restart, shuffle and every repeat mode, including at the queue's boundaries.
4. Replace the queue from the coverflow; a 10,000-track queue scrolls smoothly.
5. Resize the window to extremes; test 125%, 150% and 200% display scaling and a second monitor with a different scale.
6. Minimize, close to tray or Dock, press media keys while hidden, restore.
7. Switch the output device or unplug headphones during playback; sleep and wake the laptop.
8. Put a corrupt file and an unsupported file in the queue.
9. Hidden playback for one hour: no underruns, flat memory, CPU within budget.
10. Rename a folder, add an album, delete an album, rescan: statistics survive.
11. Scan, load and play from the giant compilation folder; open the waveform of a long track.
12. First run with no config, and an upgrade from the previous database version.

### Quality bars

- Zero compiler or clippy warnings.
- No panic on bad input. A panic hook writes the message and a backtrace to a log file in the data directory (use `tracing-appender` with rotation).
- Every bug that is fixed gets a regression test when feasible and a line in `CHANGELOG.md`.

## 18. Build, packaging and CI for Windows, macOS and Linux

Build and run on the owner's Windows machine first, check all three operating systems in CI from Phase 0, and have the owner smoke-test on the MacBook at the end of every phase from Phase 1. Cross-platform problems found late are expensive.

### First-time setup on Windows

1. Install Rust with `rustup`. The installer asks for the Visual Studio C++ Build Tools; install the 'Desktop development with C++' workload.
2. Install Git and an editor (for example VS Code with the rust-analyzer extension) and the agent tool.
3. Check with `cargo --version`, then run the empty project from Phase 0 with `cargo run`.

### Repository hygiene

- Git from the first commit. `.gitignore` covers `target/`, databases, caches and local settings. `.gitattributes` sets `* text=auto`.
- Folders: `src/`, `assets/` (fonts, icons), `tests/fixtures/`, `docs/` (this roadmap, `phases/`, `DECISIONS.md`, `PERF.md`), plus `CHANGELOG.md`.
- Licenses: Symphonia is MPL-2.0, which allows distributing a binary but asks that changes to its own files stay open. Fonts such as Poppins are under the SIL Open Font License and ship with their license file. Generate a third-party license list (for example with `cargo-about` or `cargo-license`) before sharing builds. Choose the project's own license before the code is public.

### Cross-platform discipline

- Always use `Path` and `PathBuf`, never paths joined as strings. Use the `directories` crate for locations.
- Keep operating-system code (tray, media controls, window tricks) in one `platform` module behind small traits. Outside it, avoid `cfg(target_os)`.
- Never assume case sensitivity, path separators or normalization of file names (section 4).

### CI starter (GitHub Actions)

Check the current versions of these actions before use.

```yaml
name: ci
on: [push, pull_request]
jobs:
  check:
    strategy:
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Linux system packages
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libasound2-dev libxkbcommon-dev libwayland-dev libx11-dev
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all
```

### Release builds

- `cargo build --release`. Embed fonts and icons with `include_bytes!` so the program is one self-contained executable.
- **Windows:** a zip of the `.exe` first; an installer (`cargo-wix` or Inno Setup) and an application icon later.
- **macOS:** build for Apple silicon (`aarch64-apple-darwin`); Intel builds can wait. Package an `.app` with a current bundling tool such as `cargo-bundle` or `cargo-packager`. For the owner's own Mac an ad-hoc signature is enough. Sharing with other people needs Developer ID signing and notarization through a paid Apple Developer membership, or they will see Gatekeeper warnings.
- **Linux:** a plain binary plus a `.desktop` file first; AppImage later.
- **Versioning:** beta tags `v0.1.0`, `v0.2.0` and so on, one per phase, each with a `CHANGELOG.md` entry.

## 19. Phased development roadmap

Eleven small phases take the project from an empty window to a cross-platform 1.0. Phase 1 is the first beta, and every phase ends with a working program.

&#91;embedded content: development phases · 11 phases, v0.0 to v1.0\]

**A phase is finished only when** its acceptance criteria are met, CI is green on Windows, macOS and Linux, the owner has smoke-tested it (Windows always; the MacBook from Phase 1), `CHANGELOG.md` and `docs/PERF.md` are updated, and the commit is tagged. The next document to write is `docs/phases/P0-P1.md`: a task-by-task plan for Phases 0 and 1 with exact commands, file lists and checkpoints.

### Phase 0: Foundation (v0.0)

- **Build:** install the toolchain; git repository with `.gitignore`, `CLAUDE.md`, `docs/`; `Cargo.toml` with the profiles of section 5; an eframe window with the light and dark theme tokens and Poppins; `tracing` to a log file; `settings.toml` load and save; a panic hook; CI on three systems.
- **Done when:** `cargo run` opens the themed window on Windows, the theme switches between light and dark, and CI is green everywhere.

### Phase 1: Core playback beta (v0.1)

The essential: pick a folder, see its tracks, play them well.

- **Build:**
  1. A library audit tool, `cargo run -- audit <folder>`: walk a folder, open every audio file, decode about a second, and report counts by codec, sample rate and bit depth plus every file that fails or lacks tags. Run it on the compilation folder.
  2. The audio engine of section 7 with tests on the null sink.
  3. The player core thread with commands and snapshot (section 6).
  4. A minimal UI: open-folder button and drag-and-drop, tags read in the background with lofty, the queue table, and a control bar with cover, title, artist, format line, buttons, a plain seek slider, time and volume. VU and waveform are placeholders. F12 shows the debug overlay.
  5. No database yet. The last opened folder is remembered in `settings.toml`.
- **Out of scope:** database, coverflow, tabs, lyrics, statistics, tray, media keys, visualizers, DSP.
- **Done when:** the 2,670-track compilation folder shows its first rows within 2 seconds and the full list within 30 seconds without freezing the UI; MP3, AAC M4A, FLAC 16-bit 44.1 kHz and FLAC 24-bit 96 kHz play correctly with the right format line; seek, next, previous, restart, stop, repeat and randomize work without clicks; a one-hour playback soak shows zero underruns and flat memory; CPU stays under 4% of one core while playing; the audit report is saved in `docs/` with unsupported files listed; the owner builds and plays a file on the MacBook.

### Phase 2: Library and flat coverflow (v0.2)

- **Build:** SQLite schema and migrations; library roots in Settings; the incremental scanner and album rules of section 4, including track order; cover choice and the thumbnail worker with the texture cache; a flat coverflow strip with eased scrolling, caption and double-click to replace the queue; Alphabetical order; backups.
- **Done when:** the owner's library scans cold in under a minute and rescans unchanged in under 2 seconds; the compilation is one album of about 2,670 tracks in track-number order; the strip scrolls at 60 fps with a synthetic 5,000-album library; double-click to sound takes under 200 ms; restart shows the library instantly.

### Phase 3: Control bar, waveform and VU (v0.3)

- **Build:** waveform computation, cache, drawing and click-to-seek; VU meter from callback peaks; the final control bar; a Settings window skeleton.
- **Done when:** the waveform and VU criteria of section 12 pass; a paused window causes no repaints; CPU stays within budget with the VU at 30 fps.

### Phase 4: Media keys, tray and Dock (v0.4)

- **Build:** souvlaki integration; close-to-background with tray or Dock; the hidden-mode power rules; window and splitter persistence; in-app shortcuts; recovery from device changes and sleep; error messages.
- **Done when:** media keys work while the window is hidden on Windows and on the Mac; the queue advances and plays are counted while hidden; hidden CPU is under 1.5%; Linux without a tray falls back to minimizing.

### Phase 5: Statistics and XML import (v0.5)

- **Build:** the play counter with the 50% rule, the statistics tables, the Times Played column and the Info tab. When the owner supplies the foobar2000 XML export: the importer with dry run, path matching and an idempotent import.
- **Done when:** unit tests cover pauses, seeks and repeats; real listening increments counts once; hidden playback counts; the dry-run report matches the owner's expectations; importing twice changes nothing.

### Phase 6: Visualizers (v0.6)

- **Build:** the analysis thread and tap; spectrum, real-time scrolling spectrogram, oscilloscope and the combined tab; visibility gating, the frame-rate setting and battery saver.
- **Done when:** the criteria and CPU budgets of section 12 pass and the analysis thread sleeps while the window is hidden.

### Phase 7: Lyrics (v0.7)

- **Build:** the `.lrc` parser, matching and encoding handling; the Lyrics tab with follow, manual scroll and click-to-seek.
- **Done when:** parser tests pass, the owner's tracks show synced lines, and CPU stays within budget.

### Phase 8: 3D coverflow and manual order (v0.8)

- **Build:** perspective strip meshes, sliver stack and scrubber; Manual album order with the drag list stored in `manual_order`.
- **Done when:** the criteria of section 11 pass, the owner approves the look against his foobar2000, and the manual order survives rescans.

### Phase 9: DSP (v0.9)

- **Build:** the speed and pitch control and DSP tab; later in the phase reverb and echo; equalizer, filters, limiter and presets only if the owner promotes them.
- **Done when:** speed changes cause no clicks; position, lyrics and play counting stay right at any speed; each effect passes its tests within the CPU target.

### Phase 10: macOS and Linux release (v1.0)

- **Build:** fix the platform issues found by CI and smoke tests; packaging for each system; the MacBook battery test; the third-party license list; a user-facing README.
- **Done when:** every manual checklist passes on Windows and macOS, the Linux build runs, the battery test meets section 16, and the release is tagged.

### After 1.0

Pick from the future ideas in section 20.

## 20. Risks, open questions and future ideas

The risks that hurt most are the ones a Rust beginner meets late: audio glitches, hidden-window behavior and differences between operating systems. Each has an early test in the phases of section 19.

### Risks

| Risk | Why it matters | Mitigation and where it is tested |
| --- | --- | --- |
| Real-time rule violations in the audio callback | Crackles that are hard to diagnose | Rules in section 2, allocation-counting test in section 17, underrun counter, soak test in Phase 1 |
| Player logic tied to the UI | Playback stops advancing or counting when the window is hidden | Player core thread from Phase 1; hidden-mode test in Phase 4 |
| HE-AAC or unusual M4A files in the compilation | Some tracks would not play | Library audit tool in Phase 1; unsupported files skip with a message |
| Coverflow looks warped or feels slow | It is the app's visual identity | Strip meshes first, shader fallback, texture cache; flat in Phase 2, 3D in Phase 8 |
| Dock, tray and hidden-window differences | Closing the window must keep music playing on every OS | Phase 4, fallbacks in section 15, testing on the MacBook |
| Unreviewed AI-written code piling up | A beginner cannot judge it by reading | Small phases, tests, CI, plain-language task summaries, the 500-line file guideline |
| Dependency API drift | The agent writes against outdated APIs | Rule 4 of section 2: check docs.rs for the version actually added |
| foobar2000 statistics exist only in its database | Importer needs the real XML layout | Importer built after the owner supplies the XML export (section 9) |
| Memory pressure on an 8 GB laptop | Large textures and queues | 96-texture cache, 384 px thumbnails, in-memory snapshots, measurements in `docs/PERF.md` |
| wgpu problems on older Linux GPUs | The app might not start there | Keep a fallback to the OpenGL backend; test in the Linux phase |
| Scope creep | The project never reaches a usable state | Phases with acceptance criteria and the non-goals list in section 1 |
| Licensing | MPL-2.0 (Symphonia) and font licenses | Section 18 |

### The owner's answers, and what is still open

The owner answered the eleven original questions on 2026-10-23. The answers are folded into the sections above and recorded in the table below, followed by what is still open.

| Question | Answer |
| --- | --- |
| Where are play counts stored? | In foobar2000's database. The owner will export XML later (section 9). |
| Spectrogram behavior | Scrolls in real time (section 12). |
| Font and theme | Poppins, slight blue accent, light and dark modes (section 10). |
| Album order | Alphabetical and Manual (section 11). |
| Track order in the compilation folder | By track number (section 4). |
| Left block of the control bar | Cover, title, artist and a short format line such as FLAC 16-bit (section 10). |
| Multi-disc albums | One album, sorted by disc and then track number (section 4). |
| Shuffle | A one-shot 'randomize queue' (section 7). |
| Bundled SQLite | Accepted (section 3). |
| Library size | About 4,550 files, 2,670 of them in the compilation folder, growing (section 4). |
| Stop button | Included (section 10). |

**Still open.** Use the planned default until the owner decides.

- The XML layout of the foobar2000 export. Inspect the real file when it arrives.
- The format line for lossy files. Planned: `AAC 256 kbps`, `MP3 320 kbps`.
- Exact color values. The tokens in section 10 are estimates the owner can refine against screenshots of the running app.
- Where newly scanned albums land in Manual order. Planned: at the end.
- Whether playback speed persists across launches. Planned: reset to 1.00x.

### Future ideas (not scheduled)

Gapless playback and crossfade, ReplayGain, restoring the queue and position between sessions, equalizer and filters, yearly recap page, 'play something' mode, embedded lyrics and online lookup, whole-track spectrogram, plugin API, layout editor, tag editing, scrobbling, search and filter bar, multiple playlists, CUE sheet support, match-the-track sample rate output, global hotkeys, mini player, Opus through libopus if the owner ever wants it.

## Sources

Taken from search results on 2026-10-22 and 2026-10-23, not from fully read pages. Treat them as pointers and re-verify details against each project's current documentation.

- [Symphonia: supported codecs and formats](https://github.com/pdeljanov/Symphonia)
- [Symphonia 0.6 on docs.rs (minimum Rust version)](https://docs.rs/crate/symphonia/0.6.1)
- [souvlaki: platform notes](https://docs.rs/souvlaki)
- [foobar2000 Playback Statistics: database by default, tags optional (Hydrogenaudio)](https://hydrogenaudio.org/index.php?msg=664332)
- [Synchronizing statistics with file tags (Hydrogenaudio)](https://hydrogenaudio.org/index.php?msg=493340)
