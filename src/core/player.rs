//! Player core thread: manages playlist queue, repeat mode, state transitions, and UI snapshots.
//!
//! Enforces architectural rules:
//! - Core never imports `egui`.
//! - Wakes UI via a `Box<dyn Fn() + Send>`.
//! - Communicates with the audio thread via `AudioCommand`, `AudioEvent`, and atomic `AudioStatus`.
//! - Publishes immutable `PlayerSnapshot` with a monotonic version number.

use std::sync::Arc;
use std::time::Duration;

use crate::audio::{AudioCommand, AudioEvent, AudioThread};
use crate::core::types::{PlaybackState, PlayerCommand, PlayerSnapshot, RepeatMode, TrackItem};

/// Handle for communicating with the player core thread.
pub struct PlayerHandle {
    cmd_tx: crossbeam_channel::Sender<PlayerCommand>,
    snapshot_slot: Arc<std::sync::Mutex<Arc<PlayerSnapshot>>>,
    status: Arc<crate::audio::AudioStatus>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl PlayerHandle {
    /// Sends a command to the player core.
    pub fn send(
        &self,
        cmd: PlayerCommand,
    ) -> Result<(), crossbeam_channel::SendError<PlayerCommand>> {
        self.cmd_tx.send(cmd)
    }

    /// Reads the latest atomic snapshot of player state.
    pub fn snapshot(&self) -> Arc<PlayerSnapshot> {
        Arc::clone(&*self.snapshot_slot.lock().unwrap())
    }

    /// Accesses the underlying real-time audio status (underruns, peak meters, volume).
    pub fn status(&self) -> &Arc<crate::audio::AudioStatus> {
        &self.status
    }
}

impl Drop for PlayerHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(PlayerCommand::Quit);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Spawner for the player core thread.
pub struct PlayerCore;

impl PlayerCore {
    /// Spawns the player core thread.
    ///
    /// Takes ownership of `audio` and calls `wake_ui` whenever a new snapshot is published.
    pub fn spawn(audio: AudioThread, wake_ui: Box<dyn Fn() + Send>) -> PlayerHandle {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let initial_snapshot = Arc::new(PlayerSnapshot::default());
        let snapshot_slot = Arc::new(std::sync::Mutex::new(initial_snapshot));
        let slot_for_thread = Arc::clone(&snapshot_slot);
        let audio_status = Arc::clone(audio.status());

        let handle = std::thread::Builder::new()
            .name("player-core".to_string())
            .spawn(move || {
                run_core_loop(audio, wake_ui, cmd_rx, slot_for_thread);
            })
            .expect("Failed to spawn player core thread");

        PlayerHandle {
            cmd_tx,
            snapshot_slot,
            status: audio_status,
            handle: Some(handle),
        }
    }
}

struct CoreState {
    audio: AudioThread,
    wake_ui: Box<dyn Fn() + Send>,
    cmd_rx: crossbeam_channel::Receiver<PlayerCommand>,
    snapshot_slot: Arc<std::sync::Mutex<Arc<PlayerSnapshot>>>,
    queue: Vec<TrackItem>,
    current_index: Option<usize>,
    state: PlaybackState,
    repeat_mode: RepeatMode,
    volume: f32,
    speed: f32,
    current_generation: u32,
    is_auto_advancing: bool,
    consecutive_failures: usize,
    format_line: String,
    current_duration: Duration,
    status_message: Option<String>,
    error_message: Option<String>,
    version: u64,
    last_published: Option<PlayerSnapshot>,
}

impl CoreState {
    fn new(
        audio: AudioThread,
        wake_ui: Box<dyn Fn() + Send>,
        cmd_rx: crossbeam_channel::Receiver<PlayerCommand>,
        snapshot_slot: Arc<std::sync::Mutex<Arc<PlayerSnapshot>>>,
    ) -> Self {
        Self {
            audio,
            wake_ui,
            cmd_rx,
            snapshot_slot,
            queue: Vec::new(),
            current_index: None,
            state: PlaybackState::Stopped,
            repeat_mode: RepeatMode::Off,
            volume: 1.0,
            speed: 1.0,
            current_generation: 0,
            is_auto_advancing: false,
            consecutive_failures: 0,
            format_line: String::new(),
            current_duration: Duration::ZERO,
            status_message: None,
            error_message: None,
            version: 0,
            last_published: None,
        }
    }

    fn load_track(&mut self, index: usize, autoplay: bool, is_auto_advance: bool) {
        if index >= self.queue.len() {
            return;
        }
        self.current_index = Some(index);
        self.current_generation = self.current_generation.wrapping_add(1);
        let generation = self.current_generation;
        self.is_auto_advancing = is_auto_advance;
        self.state = PlaybackState::Loading;
        self.error_message = None;

        let track = &self.queue[index];
        self.format_line = format!("{} • {}", track.artist, track.title);
        self.current_duration = track.duration;

        let _ = self.audio.send(AudioCommand::Load {
            path: track.path.clone(),
            generation,
            start: Duration::ZERO,
            autoplay,
        });
    }

    fn handle_command(&mut self, cmd: PlayerCommand) -> bool {
        match cmd {
            PlayerCommand::Play | PlayerCommand::Resume => match self.state {
                PlaybackState::Paused => {
                    let _ = self.audio.send(AudioCommand::Play);
                    self.state = PlaybackState::Playing;
                }
                PlaybackState::Stopped => {
                    if let Some(idx) = self.current_index {
                        self.load_track(idx, true, false);
                    } else if !self.queue.is_empty() {
                        self.load_track(0, true, false);
                    }
                }
                PlaybackState::Loading | PlaybackState::Playing => {}
            },
            PlayerCommand::PlayIndex(index) => {
                if index < self.queue.len() {
                    self.consecutive_failures = 0;
                    self.load_track(index, true, false);
                }
            }
            PlayerCommand::Pause => {
                if self.state == PlaybackState::Playing || self.state == PlaybackState::Loading {
                    let _ = self.audio.send(AudioCommand::Pause);
                    self.state = PlaybackState::Paused;
                }
            }
            PlayerCommand::TogglePlayPause => {
                if self.state == PlaybackState::Playing {
                    let _ = self.audio.send(AudioCommand::Pause);
                    self.state = PlaybackState::Paused;
                } else {
                    self.handle_command(PlayerCommand::Play);
                }
            }
            PlayerCommand::Stop => {
                let _ = self.audio.send(AudioCommand::Stop);
                self.state = PlaybackState::Stopped;
                self.is_auto_advancing = false;
                self.consecutive_failures = 0;
            }
            PlayerCommand::Next => {
                if !self.queue.is_empty() {
                    let next_idx = match self.repeat_mode {
                        RepeatMode::Queue => {
                            (self.current_index.unwrap_or(0) + 1) % self.queue.len()
                        }
                        _ => self.current_index.map(|i| i + 1).unwrap_or(0),
                    };
                    if next_idx < self.queue.len() {
                        self.consecutive_failures = 0;
                        self.load_track(next_idx, true, false);
                    } else {
                        let _ = self.audio.send(AudioCommand::Stop);
                        self.state = PlaybackState::Stopped;
                    }
                }
            }
            PlayerCommand::Previous => {
                if !self.queue.is_empty() {
                    let pos = self.audio.status().position();
                    if pos > Duration::from_secs(3) {
                        let _ = self.audio.send(AudioCommand::Seek(Duration::ZERO));
                    } else if let Some(idx) = self.current_index {
                        if idx > 0 {
                            self.consecutive_failures = 0;
                            self.load_track(idx - 1, true, false);
                        } else if self.repeat_mode == RepeatMode::Queue {
                            self.consecutive_failures = 0;
                            self.load_track(self.queue.len() - 1, true, false);
                        } else {
                            let _ = self.audio.send(AudioCommand::Seek(Duration::ZERO));
                        }
                    }
                }
            }
            PlayerCommand::Restart => {
                let _ = self.audio.send(AudioCommand::Seek(Duration::ZERO));
                let _ = self.audio.send(AudioCommand::Play);
                self.state = PlaybackState::Playing;
            }
            PlayerCommand::Seek(target) => {
                let _ = self.audio.send(AudioCommand::Seek(target));
            }
            PlayerCommand::SetVolume(vol) => {
                self.volume = vol.clamp(0.0, 1.0);
                let _ = self.audio.send(AudioCommand::SetVolume(self.volume));
            }
            PlayerCommand::SetSpeed(spd) => {
                self.speed = spd;
            }
            PlayerCommand::ReplaceQueue(new_queue) => {
                self.queue = new_queue;
                self.consecutive_failures = 0;
                if !self.queue.is_empty() {
                    self.load_track(0, true, false);
                } else {
                    self.current_index = None;
                    self.state = PlaybackState::Stopped;
                    let _ = self.audio.send(AudioCommand::Stop);
                }
            }
            PlayerCommand::RandomizeQueue => {
                randomize_after_current(&mut self.queue, self.current_index);
            }
            PlayerCommand::SetRepeat(mode) => {
                self.repeat_mode = mode;
            }
            PlayerCommand::OpenFolder(_) => {
                // Handled at library scan level
            }
            PlayerCommand::Quit => {
                let _ = self.audio.send(AudioCommand::Shutdown);
                return false;
            }
        }
        true
    }

    fn process_audio_events(&mut self) {
        while let Ok(event) = self.audio.events().try_recv() {
            match event {
                AudioEvent::Loaded {
                    generation,
                    metadata,
                } => {
                    if generation == self.current_generation {
                        self.consecutive_failures = 0;
                        self.format_line = metadata.format_line();
                        if let Some(dur) = metadata.duration {
                            self.current_duration = dur;
                        }
                        if self.state == PlaybackState::Loading {
                            self.state = PlaybackState::Playing;
                        }
                        self.error_message = None;
                    }
                }
                AudioEvent::LoadFailed { generation, error } => {
                    if generation == self.current_generation {
                        if self.is_auto_advancing {
                            self.consecutive_failures += 1;
                            if self.consecutive_failures >= self.queue.len()
                                || self.queue.is_empty()
                            {
                                self.state = PlaybackState::Stopped;
                                let _ = self.audio.send(AudioCommand::Stop);
                                self.error_message =
                                    Some(format!("All tracks failed to load: {error}"));
                                self.is_auto_advancing = false;
                            } else {
                                let next_idx = match self.repeat_mode {
                                    RepeatMode::Track => self.current_index.unwrap_or(0),
                                    RepeatMode::Queue => {
                                        (self.current_index.unwrap_or(0) + 1) % self.queue.len()
                                    }
                                    RepeatMode::Off => self.current_index.unwrap_or(0) + 1,
                                };
                                if next_idx < self.queue.len() {
                                    self.load_track(next_idx, true, true);
                                } else {
                                    self.state = PlaybackState::Stopped;
                                    let _ = self.audio.send(AudioCommand::Stop);
                                    self.error_message =
                                        Some(format!("Failed to load track: {error}"));
                                    self.is_auto_advancing = false;
                                }
                            }
                        } else {
                            // Explicit play: stay stopped with error message
                            self.state = PlaybackState::Stopped;
                            let _ = self.audio.send(AudioCommand::Stop);
                            self.error_message = Some(format!("Failed to load track: {error}"));
                            self.is_auto_advancing = false;
                        }
                    }
                }
                AudioEvent::Error(err) => {
                    self.error_message = Some(err);
                }
            }
        }
    }

    fn check_track_ended(&mut self) {
        let (status_gen, track_ended) = self.audio.status().generation_and_track_ended();
        if track_ended
            && status_gen == self.current_generation
            && (self.state == PlaybackState::Playing || self.state == PlaybackState::Loading)
        {
            self.consecutive_failures = 0;
            match self.repeat_mode {
                RepeatMode::Track => {
                    if let Some(idx) = self.current_index {
                        self.load_track(idx, true, true);
                    }
                }
                RepeatMode::Queue => {
                    if !self.queue.is_empty() {
                        let next_idx = (self.current_index.unwrap_or(0) + 1) % self.queue.len();
                        self.load_track(next_idx, true, true);
                    }
                }
                RepeatMode::Off => {
                    let next_idx = self.current_index.map(|i| i + 1).unwrap_or(0);
                    if next_idx < self.queue.len() {
                        self.load_track(next_idx, true, true);
                    } else {
                        self.state = PlaybackState::Stopped;
                        let _ = self.audio.send(AudioCommand::Stop);
                        self.status_message = Some("Queue finished".to_string());
                    }
                }
            }
        }
    }

    fn publish_snapshot(&mut self) {
        let position = if self.state == PlaybackState::Stopped {
            Duration::ZERO
        } else {
            self.audio.status().position()
        };

        let current_snap = PlayerSnapshot {
            version: self.version,
            state: self.state,
            queue: self.queue.clone(),
            current_index: self.current_index,
            position,
            duration: self.current_duration,
            volume: self.volume,
            speed: self.speed,
            repeat_mode: self.repeat_mode,
            format_line: self.format_line.clone(),
            status_message: self.status_message.clone(),
            error_message: self.error_message.clone(),
        };

        let changed = match &self.last_published {
            Some(prev) => {
                prev.state != current_snap.state
                    || prev.current_index != current_snap.current_index
                    || prev.position.as_millis() != current_snap.position.as_millis()
                    || (prev.volume - current_snap.volume).abs() > 1e-4
                    || (prev.speed - current_snap.speed).abs() > 1e-4
                    || prev.repeat_mode != current_snap.repeat_mode
                    || prev.format_line != current_snap.format_line
                    || prev.status_message != current_snap.status_message
                    || prev.error_message != current_snap.error_message
                    || prev.queue != current_snap.queue
            }
            None => true,
        };

        if changed {
            self.version = self.version.wrapping_add(1);
            let snap = Arc::new(PlayerSnapshot {
                version: self.version,
                ..current_snap
            });
            *self.snapshot_slot.lock().unwrap() = Arc::clone(&snap);
            self.last_published = Some((*snap).clone());
            (self.wake_ui)();
        }
    }
}

/// Shuffles elements in `queue` after `current_idx`.
///
/// If `current_idx` is `Some(i)`, elements at `0..=i` remain untouched.
/// If `current_idx` is `None`, the whole slice is shuffled.
fn randomize_after_current(queue: &mut [TrackItem], current_idx: Option<usize>) {
    let start = match current_idx {
        Some(idx) => idx + 1,
        None => 0,
    };
    if start >= queue.len() {
        return;
    }
    let slice = &mut queue[start..];
    if slice.len() <= 1 {
        return;
    }

    let mut rng = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x853c49e6748fea9b);

    for i in (1..slice.len()).rev() {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let j = (rng as usize) % (i + 1);
        slice.swap(i, j);
    }
}

fn run_core_loop(
    audio: AudioThread,
    wake_ui: Box<dyn Fn() + Send>,
    cmd_rx: crossbeam_channel::Receiver<PlayerCommand>,
    snapshot_slot: Arc<std::sync::Mutex<Arc<PlayerSnapshot>>>,
) {
    let mut state = CoreState::new(audio, wake_ui, cmd_rx, snapshot_slot);

    // Initial snapshot publish
    state.publish_snapshot();

    loop {
        let timeout = if state.state == PlaybackState::Playing {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(250)
        };

        match state.cmd_rx.recv_timeout(timeout) {
            Ok(cmd) => {
                if !state.handle_command(cmd) {
                    break;
                }
                while let Ok(next_cmd) = state.cmd_rx.try_recv() {
                    if !state.handle_command(next_cmd) {
                        return;
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }

        state.process_audio_events();
        state.check_track_ended();
        state.publish_snapshot();
    }
}
