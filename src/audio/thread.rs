//! Dedicated audio thread managing AudioEngine lifecycle, command processing, and status publication.
//!
//! Enforces thread boundaries:
//! - AudioEngine is constructed and confined to this thread.
//! - The core thread sends commands (`AudioCommand`) and receives one-shot events (`AudioEvent`).
//! - Continuous status is observed via lock-free atomics in `AudioStatus`.

use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::audio::engine::AudioEngine;
use crate::audio::sink::{CallbackShared, NullSink};
use crate::audio::source::AudioMetadata;

/// Commands sent from the player core to the audio thread.
#[derive(Debug)]
pub enum AudioCommand {
    /// Loads a track from disk, seeking to `start` if specified, and optionally starts playing.
    Load {
        path: PathBuf,
        generation: u32,
        start: Duration,
        autoplay: bool,
    },
    /// Resumes audio playback.
    Play,
    /// Pauses audio playback (fades to silence).
    Pause,
    /// Stops playback and flushes buffered audio.
    Stop,
    /// Seeks to a specific duration in the current track.
    Seek(Duration),
    /// Sets volume with a 0.0..=1.0 slider position (maps quadratically to gain).
    SetVolume(f32),
    /// Signals the audio thread to terminate cleanly.
    Shutdown,
}

/// One-shot events sent from the audio thread back to the player core.
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Track was successfully probed, opened, and buffered.
    Loaded {
        generation: u32,
        metadata: AudioMetadata,
    },
    /// Track failed to load or decode.
    LoadFailed { generation: u32, error: String },
    /// Non-fatal error during playback or decoding.
    Error(String),
}

/// Helper function to pack a 32-bit generation and a boolean track_ended flag into a single 64-bit word.
#[inline]
pub fn pack_gen_ended(generation: u32, track_ended: bool) -> u64 {
    ((generation as u64) << 32) | (if track_ended { 1 } else { 0 })
}

/// Helper function to unpack generation and track_ended from a 64-bit word.
#[inline]
pub fn unpack_gen_ended(packed: u64) -> (u32, bool) {
    ((packed >> 32) as u32, (packed & 1) != 0)
}

/// Continuous lock-free status observed by the player core and UI.
///
/// Hands out the engine's existing `CallbackShared` atomics directly, adding only the
/// necessary fields (`gen_ended` and `position_micros`) that are not present on `CallbackShared`.
#[derive(Debug)]
pub struct AudioStatus {
    /// Upper 32 bits: generation; Bit 0: track_ended flag.
    gen_ended: AtomicU64,
    /// Current playback position in microseconds.
    position_micros: AtomicU64,
    /// Engine callback shared atomics (underruns, peaks, volume, paused).
    shared: Arc<CallbackShared>,
}

impl AudioStatus {
    /// Creates a new `AudioStatus` wrapping an existing `CallbackShared`.
    pub fn new(shared: Arc<CallbackShared>) -> Self {
        Self {
            gen_ended: AtomicU64::new(pack_gen_ended(0, false)),
            position_micros: AtomicU64::new(0),
            shared,
        }
    }

    /// Current track generation.
    pub fn generation(&self) -> u32 {
        unpack_gen_ended(self.gen_ended.load(Ordering::Acquire)).0
    }

    /// Whether the current track has fully played out including device drain.
    pub fn track_ended(&self) -> bool {
        unpack_gen_ended(self.gen_ended.load(Ordering::Acquire)).1
    }

    /// Reads generation and track_ended atomically in a single atomic instruction.
    pub fn generation_and_track_ended(&self) -> (u32, bool) {
        unpack_gen_ended(self.gen_ended.load(Ordering::Acquire))
    }

    /// Current playback position.
    pub fn position(&self) -> Duration {
        Duration::from_micros(self.position_micros.load(Ordering::Relaxed))
    }

    /// Counter of underrun occurrences.
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    /// Left channel peak level in 0.0..=1.0.
    pub fn peak_left(&self) -> f32 {
        self.shared.peak_left()
    }

    /// Right channel peak level in 0.0..=1.0.
    pub fn peak_right(&self) -> f32 {
        self.shared.peak_right()
    }

    /// Current volume gain in 0.0..=1.0.
    pub fn volume_gain(&self) -> f32 {
        self.shared.volume_gain()
    }

    /// Whether playback is currently paused.
    pub fn paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    /// Reference to the underlying `CallbackShared` Arc.
    pub fn shared(&self) -> &Arc<CallbackShared> {
        &self.shared
    }

    pub(crate) fn set_generation_and_track_ended(&self, generation: u32, track_ended: bool) {
        self.gen_ended
            .store(pack_gen_ended(generation, track_ended), Ordering::Release);
    }

    pub(crate) fn set_track_ended(&self, generation: u32, track_ended: bool) {
        self.gen_ended
            .store(pack_gen_ended(generation, track_ended), Ordering::Release);
    }

    pub(crate) fn set_position(&self, pos: Duration) {
        self.position_micros
            .store(pos.as_micros() as u64, Ordering::Relaxed);
    }
}

/// Handle to the background audio thread.
pub struct AudioThread {
    cmd_tx: crossbeam_channel::Sender<AudioCommand>,
    event_rx: crossbeam_channel::Receiver<AudioEvent>,
    status: Arc<AudioStatus>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioThread {
    /// Spawns the audio thread with real CPAL hardware output.
    pub fn spawn() -> Result<Self> {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let (init_tx, init_rx) = crossbeam_channel::bounded(1);

        let handle = std::thread::Builder::new()
            .name("audio-thread".to_string())
            .spawn(move || {
                let engine = match AudioEngine::new() {
                    Ok(e) => {
                        let status = Arc::new(AudioStatus::new(Arc::clone(e.shared())));
                        let _ = init_tx.send(Ok(Arc::clone(&status)));
                        (e, status)
                    }
                    Err(err) => {
                        let _ = init_tx.send(Err(err));
                        return;
                    }
                };

                run_audio_thread_loop(engine.0, engine.1, cmd_rx, event_tx);
            })
            .context("Failed to spawn audio thread")?;

        let status = init_rx
            .recv()
            .map_err(|_| anyhow!("Audio thread terminated during initialization"))??;

        Ok(Self {
            cmd_tx,
            event_rx,
            status,
            handle: Some(handle),
        })
    }

    /// Spawns the audio thread with a headless NullSink for automated tests.
    pub fn spawn_null(sample_rate: u32) -> Result<(Self, Arc<NullSink>)> {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let (init_tx, init_rx) = crossbeam_channel::bounded(1);

        let handle = std::thread::Builder::new()
            .name("audio-thread-null".to_string())
            .spawn(move || {
                let (engine, null_sink) = AudioEngine::new_null(sample_rate);
                let status = Arc::new(AudioStatus::new(Arc::clone(engine.shared())));
                let _ = init_tx.send((Arc::clone(&status), null_sink));

                run_audio_thread_loop(engine, status, cmd_rx, event_tx);
            })
            .context("Failed to spawn null audio thread")?;

        let (status, null_sink) = init_rx
            .recv()
            .map_err(|_| anyhow!("Audio thread terminated during initialization"))?;

        let thread = Self {
            cmd_tx,
            event_rx,
            status,
            handle: Some(handle),
        };

        Ok((thread, null_sink))
    }

    /// Sends an `AudioCommand` to the audio thread.
    pub fn send(
        &self,
        cmd: AudioCommand,
    ) -> Result<(), crossbeam_channel::SendError<AudioCommand>> {
        self.cmd_tx.send(cmd)
    }

    /// Returns a reference to the command sender channel.
    pub fn sender(&self) -> &crossbeam_channel::Sender<AudioCommand> {
        &self.cmd_tx
    }

    /// Returns a reference to the one-shot event receiver channel.
    pub fn events(&self) -> &crossbeam_channel::Receiver<AudioEvent> {
        &self.event_rx
    }

    /// Returns a reference to the continuous `AudioStatus` atomics.
    pub fn status(&self) -> &Arc<AudioStatus> {
        &self.status
    }
}

impl Drop for AudioThread {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(AudioCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_command(
    cmd: AudioCommand,
    engine: &mut AudioEngine,
    status: &AudioStatus,
    event_tx: &crossbeam_channel::Sender<AudioEvent>,
    is_playing: &mut bool,
    current_generation: &mut u32,
) -> bool {
    match cmd {
        AudioCommand::Load {
            path,
            generation,
            start,
            autoplay,
        } => {
            *current_generation = generation;
            // Clear track_ended as soon as Load is handled
            status.set_generation_and_track_ended(generation, false);

            match engine.load_file(&path) {
                Ok(metadata) => {
                    if start > Duration::ZERO {
                        let _ = engine.seek(start);
                    }
                    status.set_position(engine.position());
                    let _ = event_tx.send(AudioEvent::Loaded {
                        generation,
                        metadata,
                    });
                    if autoplay {
                        let _ = engine.play();
                        *is_playing = true;
                    } else {
                        let _ = engine.pause();
                        *is_playing = false;
                    }
                }
                Err(err) => {
                    let _ = engine.pause();
                    *is_playing = false;
                    let _ = event_tx.send(AudioEvent::LoadFailed {
                        generation,
                        error: err.to_string(),
                    });
                }
            }
        }
        AudioCommand::Play => {
            let _ = engine.play();
            *is_playing = true;
        }
        AudioCommand::Pause => {
            let _ = engine.pause();
            *is_playing = false;
        }
        AudioCommand::Stop => {
            let _ = engine.pause();
            engine.flush();
            *is_playing = false;
            status.set_generation_and_track_ended(*current_generation, false);
            status.set_position(Duration::ZERO);
        }
        AudioCommand::Seek(target) => {
            // Clear track_ended as soon as Seek is handled
            status.set_generation_and_track_ended(*current_generation, false);
            let _ = engine.seek(target);
            status.set_position(engine.position());
        }
        AudioCommand::SetVolume(vol) => {
            engine.set_volume(vol);
        }
        AudioCommand::Shutdown => {
            return false;
        }
    }
    true
}

fn run_audio_thread_loop(
    mut engine: AudioEngine,
    status: Arc<AudioStatus>,
    cmd_rx: crossbeam_channel::Receiver<AudioCommand>,
    event_tx: crossbeam_channel::Sender<AudioEvent>,
) {
    let mut is_playing = false;
    let mut current_generation = 0_u32;

    loop {
        // Wakeup strategy:
        // While paused or stopped: block on command queue with NO timeout (0% CPU).
        // While playing: wait up to 100 ms for commands before executing refill/status tick.
        let cmd_res = if is_playing {
            cmd_rx.recv_timeout(Duration::from_millis(100))
        } else {
            cmd_rx
                .recv()
                .map_err(|_| crossbeam_channel::RecvTimeoutError::Disconnected)
        };

        match cmd_res {
            Ok(cmd) => {
                if !handle_command(
                    cmd,
                    &mut engine,
                    &status,
                    &event_tx,
                    &mut is_playing,
                    &mut current_generation,
                ) {
                    break;
                }
                // Drain any additional commands that arrived in the channel
                while let Ok(next_cmd) = cmd_rx.try_recv() {
                    if !handle_command(
                        next_cmd,
                        &mut engine,
                        &status,
                        &event_tx,
                        &mut is_playing,
                        &mut current_generation,
                    ) {
                        return;
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // Periodic wakeup during playback
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                break;
            }
        }

        // Active playback handling
        if is_playing {
            // Refill buffer when below about half
            if engine.is_buffer_below_half() {
                engine.fill_buffer();
            }

            status.set_position(engine.position());

            if engine.is_track_ended() {
                status.set_track_ended(current_generation, true);
                is_playing = false;
            }
        }
    }
}
