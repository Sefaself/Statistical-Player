//! Application settings loading, saving, and persistence.
//!
//! Stores user preferences in the OS-standard config directory as `settings.toml`.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

/// User theme preference: System, Dark, or Light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSetting {
    #[default]
    System,
    Dark,
    Light,
}

/// Persistent user settings saved in `settings.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Active theme setting.
    pub theme: ThemeSetting,
    /// Last window width in logical points.
    pub window_width: f32,
    /// Last window height in logical points.
    pub window_height: f32,
    /// Whether closing the main window leaves playback running in the background.
    pub keep_playing_in_background: bool,
    /// Target frame rate for live visualizers (15, 30, or 60).
    pub visualizer_fps: u32,
    /// Battery saver mode for laptops (15 fps cap, paused spectrogram).
    pub battery_saver: bool,
    /// Configured library root paths.
    pub library_roots: Vec<PathBuf>,
    /// Last opened folder (remembered in Phase 1 before SQLite library is active).
    pub last_opened_folder: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeSetting::System,
            window_width: 1024.0,
            window_height: 680.0,
            keep_playing_in_background: true,
            visualizer_fps: 30,
            battery_saver: false,
            library_roots: Vec::new(),
            last_opened_folder: None,
        }
    }
}

impl Settings {
    /// Returns project directory resolver (`directories::ProjectDirs`).
    pub fn project_dirs() -> Option<ProjectDirs> {
        ProjectDirs::from("com", "statistical-player", "StatisticalPlayer")
    }

    /// Path to `settings.toml` in the OS config directory.
    pub fn config_path() -> Option<PathBuf> {
        Self::project_dirs().map(|dirs| dirs.config_dir().join("settings.toml"))
    }

    /// Path to data directory (where database and logs reside).
    pub fn data_dir() -> Option<PathBuf> {
        Self::project_dirs().map(|dirs| dirs.data_dir().to_path_buf())
    }

    /// Path to cache directory (where cover thumbnails reside).
    pub fn cache_dir() -> Option<PathBuf> {
        Self::project_dirs().map(|dirs| dirs.cache_dir().to_path_buf())
    }

    /// Load settings from disk, falling back to defaults if not found or corrupted.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            warn!("Could not determine OS config directory; using default settings");
            return Self::default();
        };

        if !path.exists() {
            info!(
                "No settings file found at {}; using defaults",
                path.display()
            );
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(settings) => {
                    info!("Loaded settings from {}", path.display());
                    settings
                }
                Err(err) => {
                    warn!(
                        "Failed to parse {}: {}; using default settings",
                        path.display(),
                        err
                    );
                    Self::default()
                }
            },
            Err(err) => {
                warn!(
                    "Failed to read {}: {}; using default settings",
                    path.display(),
                    err
                );
                Self::default()
            }
        }
    }

    /// Save current settings to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()
            .context("Could not determine OS config directory to save settings")?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config dir {}", parent.display()))?;
        }

        let content =
            toml::to_string_pretty(self).context("Failed to serialize settings to TOML")?;

        fs::write(&path, content)
            .with_context(|| format!("Failed to write settings to {}", path.display()))?;

        info!("Saved settings to {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_serialize_and_deserialize() {
        let settings = Settings::default();
        let toml_str = toml::to_string(&settings).expect("serialization failed");
        let decoded: Settings = toml::from_str(&toml_str).expect("deserialization failed");

        assert_eq!(decoded.theme, ThemeSetting::System);
        assert_eq!(decoded.window_width, 1024.0);
        assert_eq!(decoded.window_height, 680.0);
        assert!(decoded.keep_playing_in_background);
        assert_eq!(decoded.visualizer_fps, 30);
    }
}
