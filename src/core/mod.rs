//! Core domain logic: settings, commands, state machine, and snapshot types.

pub mod settings;
pub mod types;

pub use settings::{Settings, ThemeSetting};
pub use types::{PlaybackState, PlayerCommand, PlayerSnapshot, RepeatMode, TrackItem};
