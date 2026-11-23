# Phase 1, Task 1.2: audio thread + player core thread

Overrides the Task 1.2 text in P0-P1.md: decoding does NOT run on the core thread.

## Threads
- **UI**: sends PlayerCommand, reads PlayerSnapshot. Never touches the engine.
- **Core** (src/core/player.rs): owns queue, current index, state, repeat mode, volume.
  Loop: `recv_timeout` 50 ms while Playing, 250 ms otherwise. Never imports egui.
- **Audio thread** (src/audio/thread.rs): creates and owns the whole `AudioEngine`
  (cpal Stream is likely !Send, so the engine is built ON this thread and never moves).
  Loop: drain AudioCommand with `recv_timeout` (>= 10 ms), call `fill_buffer` while
  playing, publish status.
- **cpal callback**: unchanged.

## Messages
- Core -> Audio (`AudioCommand`, crossbeam): `Load { path, generation, start, autoplay }`,
  `Play`, `Pause`, `Stop`, `Seek(Duration)`, `SetVolume(f32)`, `Shutdown`.
- Audio -> Core, continuous (`Arc<AudioStatus>`, atomics): `generation`, position,
  `track_ended`, underruns, peak_l, peak_r. If position needs engine-owned fields,
  the audio thread copies them into atomics each loop.
- Audio -> Core, one-shot (`AudioEvent`, channel): `Loaded { generation, metadata }`,
  `LoadFailed { generation, error }`, `Error(String)`.
- Core -> UI: `PlayerSnapshot` in one replaceable slot (`Arc<Mutex<Arc<PlayerSnapshot>>>`
  is fine, this is not the audio callback) + a `version: u64`. The core wakes the UI
  through a `Box<dyn Fn() + Send>` passed in at spawn (main.rs wraps egui's
  `request_repaint`), so core stays egui-free.

## Coordination (the race-prone part)
- Every `Load` gets a new `generation`. Status and events carry the generation they
  belong to. The core ignores anything from an older generation, so a late
  `track_ended` from the previous track can never skip the next one.
- **Next/Previous/Play(index)/auto-advance**: bump generation, send `Load`.
- **Seek**: same generation, send `Seek`; the engine's existing flush/discard handles stale audio.
- **Track end**: core sees `track_ended` for the current generation (the drain tail
  is already inside `is_track_ended`), then applies repeat: Track -> reload same;
  Queue -> next or wrap; Off -> next or Stop at the end.
- **Load failure**: auto-advance skips to the next track (at most queue-length
  failures in a row, then Stop + message). Explicit play stays Stopped + message.
- **Stop**: audio Stop, position 0, queue kept. **Randomize**: one-shot shuffle of the
  tracks AFTER the current one; the current track is untouched.
- **ReplaceQueue**: replaces queue, index 0, loads and plays track 0.

## Files
- `src/audio/thread.rs` (new): `AudioThread::spawn()` and `spawn_null(rate)` for tests,
  handle with `send()`, `status()`, `events()`, `Drop` = Shutdown + join.
- `src/core/player.rs` (new): `PlayerCore::spawn(audio, wake_ui) -> PlayerHandle`.
- `src/core/types.rs`: extend existing types only as needed (snapshot gets version, error message).
- Engine changes: only what is needed to expose the status. No behavior changes.

## Tests (NullSink, generated fixtures)
Audio thread: load, play, reach end -> `track_ended`; stale generation ignored; seek.
Core: 3-track queue advances; repeat Off/Track/Queue at both boundaries; randomize
keeps current; failed track is skipped on auto-advance; Stop keeps the queue.

## Amendments
- Audio thread wakeups. The plan says recv_timeout of at least 10 ms, which allows 100 wakeups a second. With a 1.5 s buffer there's no need. Block on commands with no timeout while paused or stopped, and wake every ~100 ms while playing (refill when the buffer is below about half). Section 15 of the roadmap budgets about 20 wakeups a second for hidden playback.
- Status consistency. The core reads several atomics (generation, position, track_ended) one by one, and a load in between could mix old and new values. Read generation first and last and discard the reading if it changed, or pack generation and track_ended into one AtomicU64. Also clear track_ended as soon as a Load or Seek is handled.
- Reuse what exists. The engine already has atomics for position, peaks and underruns in CallbackShared. AudioStatus should hand out that same Arc, not copy values into a second set.