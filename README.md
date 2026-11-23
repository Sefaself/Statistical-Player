# Statistical Player

A fast, (yet-to-be)good-looking, cross-platform desktop music player written in pure Rust, inspired by my own heavily customized foobar2000 layout :3

> **Status: very early development (Phase 1, pre-v0.1).** The audio engine and player core work and are tested, but there is no graphical interface yet. As of right now you can only drive it from the command line.

## Goals

- One fixed layout: queue on the left, coverflow-like interface on top, tabbed visualizers in the center, control bar with waveform seekbar at the bottom.
- Folder-based library: one folder is one album (as of right now).
- Full-precision decoding (FLAC, MP3, AAC in M4A, WAV, OGG Vorbis) in pure Rust, no C audio dependencies.
- Low CPU and battery use, even when minimized.
- Exact Windows, macOS and Linux support (CI builds all three).

The full developement plan is in [`docs/ROADMAP.md`](docs/ROADMAP.md).

## What works as of right now (11/23/2026)

- Decoding with [symphonia](https://github.com/pdeljanov/Symphonia), output with [cpal](https://github.com/RustAudio/cpal), through a lock-free ring buffer.
- Resampling to the device sample rate, seeking, end-of-track drain.
- A dedicated audio thread and a player core thread (queue, repeat modes, next/previous, one-shot randomize, skip-on-failure).
- A library audit tool that checks every file in a folder.
- Real-time safety test: the audio callback is checked for zero allocations.

Not yet: GUI, library database, coverflow, visualizers, lyrics, statistics, gapless playback.

## Build and run

Requires a current stable Rust toolchain. On Windows you also need the Visual Studio C++ Build Tools. On Linux you need ALSA development headers (`libasound2-dev`).

```bash
cargo run                                   # themed window (placeholder UI test)
cargo run -- audit <folder>                 # check every audio file in a folder
cargo run -- play <file>                    # play one file with position and peak readout
cargo run -- queue <f1> <f2> ... [--repeat off|track|queue]
cargo test                                  # full test suite (uses a null audio sink)
```

## Project layout

```text
src/core/      player core thread, queue, commands, snapshot, settings
src/audio/     decoder, resampler, audio thread, sinks
src/library/   folder loader, audit tool
src/ui/        egui window, theme, fonts
docs/          roadmap, decisions log, phase plans
```

## Contributing

This is a personal learning project and development is slow and deliberate (one small phase at a time). Issues and suggestions are welcome, but do expect slow replies...

## License

The bundled Poppins font is under the SIL Open Font License (see `assets/fonts/OFL.txt`).3

## Why!?

I was sick and tired of having 2 different music players between Windows, macOS and Linux, i wanted something that is exactly whan i need, consistent across all platforms.
I know there are plenty of other good music players out there but none seem satisfy my stupid specific needs and quirks.

I'm building this for me, honestly I don't expect anyone to actually use this, but if you find it useful you may as well use it.