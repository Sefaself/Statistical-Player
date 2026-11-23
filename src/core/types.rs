//! Core types: commands, playback state, and UI snapshots.
//!
//! Defined here so that UI, audio, and player core can share them without cycles.

use std::path::PathBuf;
use std::time::Duration;

/// Playback state of the audio engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Loading,
    Playing,
    Paused,
}

/// Repeat modes supported by the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    Track,
    Queue,
}

impl RepeatMode {
    /// Cycles to the next repeat mode: Off -> Queue -> Track -> Off.
    pub fn cycle(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::Queue,
            RepeatMode::Queue => RepeatMode::Track,
            RepeatMode::Track => RepeatMode::Off,
        }
    }
}

/// Commands sent from the UI or OS integration to the player core thread.
#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Play,
    PlayIndex(usize),
    Pause,
    Resume,
    TogglePlayPause,
    Stop,
    Next,
    Previous,
    Restart,
    Seek(Duration),
    SetVolume(f32),
    SetSpeed(f32),
    ReplaceQueue(Vec<TrackItem>),
    RandomizeQueue,
    SetRepeat(RepeatMode),
    OpenFolder(PathBuf),
    Quit,
}

/// Lightweight track entry displayed in the queue table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackItem {
    pub id: u64,
    pub path: PathBuf,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<u32>,
    pub duration: Duration,
    pub play_count: u32,
    pub codec: String,
    pub sample_rate: u32,
    pub bit_depth: Option<u32>,
    pub channels: u16,
    pub bitrate_kbps: Option<u32>,
}

impl TrackItem {
    /// Creates a minimal `TrackItem` from path.
    pub fn simple(id: u64, path: PathBuf) -> Self {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();
        Self {
            id,
            path,
            track_no: None,
            disc_no: None,
            title,
            artist: "Unknown Artist".to_string(),
            album: "Unknown Album".to_string(),
            year: None,
            duration: Duration::ZERO,
            play_count: 0,
            codec: String::new(),
            sample_rate: 0,
            bit_depth: None,
            channels: 2,
            bitrate_kbps: None,
        }
    }
}

/// Snapshot published from the player core thread to the UI thread.
///
/// Immutable and cheap to clone (or swap via atomic/arc). The UI redraws
/// when `version` increments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerSnapshot {
    pub version: u64,
    pub state: PlaybackState,
    pub queue: Vec<TrackItem>,
    pub current_index: Option<usize>,
    pub position: Duration,
    pub duration: Duration,
    pub volume: f32,
    pub speed: f32,
    pub repeat_mode: RepeatMode,
    pub format_line: String,
    pub status_message: Option<String>,
    pub error_message: Option<String>,
}

impl PlayerSnapshot {
    /// Returns the currently playing track item, if any.
    pub fn current_track(&self) -> Option<&TrackItem> {
        self.current_index.and_then(|idx| self.queue.get(idx))
    }
}
