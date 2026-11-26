//! Bottom transport control bar.

use std::time::Duration;

use egui::{Align, Layout, RichText, Ui};

use crate::core::{PlaybackState, PlayerCommand, PlayerHandle, PlayerSnapshot, RepeatMode};
use crate::ui::theme::ThemeTokens;

fn format_time(d: Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}

/// Renders the transport buttons, current track title/artist, and `m:ss / m:ss` time display.
pub fn show_control_bar(
    ui: &mut Ui,
    player: &PlayerHandle,
    snapshot: &PlayerSnapshot,
    tokens: ThemeTokens,
) {
    ui.horizontal(|ui| {
        // Left: current track title & artist
        ui.vertical(|ui| {
            if let Some(track) = snapshot.current_track() {
                ui.label(
                    RichText::new(&track.title)
                        .strong()
                        .color(tokens.primary_text),
                );
                ui.label(
                    RichText::new(&track.artist)
                        .size(12.0)
                        .color(tokens.secondary_text),
                );
            } else {
                ui.label(
                    RichText::new("Nothing playing")
                        .italics()
                        .color(tokens.secondary_text),
                );
            }
        });

        ui.add_space(16.0);

        // Center: transport buttons
        if ui.button("Restart").clicked() {
            let _ = player.send(PlayerCommand::Restart);
        }
        if ui.button("Previous").clicked() {
            let _ = player.send(PlayerCommand::Previous);
        }
        if ui.button("Stop").clicked() {
            let _ = player.send(PlayerCommand::Stop);
        }

        let play_pause_label = if snapshot.state == PlaybackState::Playing {
            "Pause"
        } else {
            "Play"
        };
        if ui.button(play_pause_label).clicked() {
            let _ = player.send(PlayerCommand::TogglePlayPause);
        }

        if ui.button("Next").clicked() {
            let _ = player.send(PlayerCommand::Next);
        }
        if ui.button("Randomize").clicked() {
            let _ = player.send(PlayerCommand::RandomizeQueue);
        }

        let repeat_label = match snapshot.repeat_mode {
            RepeatMode::Off => "Repeat: Off",
            RepeatMode::Queue => "Repeat: Queue",
            RepeatMode::Track => "Repeat: Track",
        };
        if ui.button(repeat_label).clicked() {
            let _ = player.send(PlayerCommand::SetRepeat(snapshot.repeat_mode.cycle()));
        }

        // Right: plain m:ss / m:ss time text
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let pos_str = format_time(snapshot.position);
            let dur_str = format_time(snapshot.duration);
            ui.label(
                RichText::new(format!("{pos_str} / {dur_str}"))
                    .monospace()
                    .color(tokens.primary_text),
            );
        });
    });
}
