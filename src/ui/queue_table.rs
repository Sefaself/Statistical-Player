//! Virtualized queue table displaying loaded or queued tracks.

use std::time::Duration;

use egui::{Align, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::core::TrackItem;
use crate::ui::theme::ThemeTokens;

const ROW_HEIGHT: f32 = 24.0;

fn format_length(d: Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}

/// Renders the virtualized track queue table.
///
/// Returns `Some(index)` if the user double-clicked a row.
pub fn show_queue_table(
    ui: &mut Ui,
    tracks: &[TrackItem],
    current_index: Option<usize>,
    tokens: ThemeTokens,
) -> Option<usize> {
    let mut double_clicked_row = None;

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(28.0)) // Playing marker
        .column(Column::exact(44.0)) // Track #
        .column(Column::remainder().at_least(160.0).clip(true)) // Title
        .column(Column::initial(180.0).at_least(100.0).clip(true)) // Artist
        .column(Column::exact(52.0)) // Year
        .column(Column::exact(60.0)) // Length
        .header(24.0, |mut header| {
            header.col(|ui| {
                ui.label(RichText::new("").strong().color(tokens.secondary_text));
            });
            header.col(|ui| {
                ui.label(RichText::new("#").strong().color(tokens.secondary_text));
            });
            header.col(|ui| {
                ui.label(RichText::new("Title").strong().color(tokens.secondary_text));
            });
            header.col(|ui| {
                ui.label(
                    RichText::new("Artist")
                        .strong()
                        .color(tokens.secondary_text),
                );
            });
            header.col(|ui| {
                ui.label(RichText::new("Year").strong().color(tokens.secondary_text));
            });
            header.col(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("Length")
                            .strong()
                            .color(tokens.secondary_text),
                    );
                });
            });
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, tracks.len(), |mut row| {
                let row_idx = row.index();
                let track = &tracks[row_idx];
                let is_playing = current_index == Some(row_idx);

                row.set_selected(is_playing);

                let text_color = if is_playing {
                    tokens.accent
                } else {
                    tokens.primary_text
                };
                let sub_color = if is_playing {
                    tokens.accent
                } else {
                    tokens.secondary_text
                };

                row.col(|ui| {
                    if is_playing {
                        ui.label(RichText::new("▶").color(tokens.accent).strong());
                    }
                });
                row.col(|ui| {
                    let trk_str = track.track_no.map(|n| n.to_string()).unwrap_or_default();
                    ui.label(RichText::new(trk_str).color(sub_color));
                });
                row.col(|ui| {
                    let mut text = RichText::new(&track.title).color(text_color);
                    if is_playing {
                        text = text.strong();
                    }
                    ui.label(text);
                });
                row.col(|ui| {
                    ui.label(RichText::new(&track.artist).color(sub_color));
                });
                row.col(|ui| {
                    let year_str = track.year.map(|y| y.to_string()).unwrap_or_default();
                    ui.label(RichText::new(year_str).color(sub_color));
                });
                row.col(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format_length(track.duration)).color(sub_color));
                    });
                });

                if row.response().double_clicked() {
                    double_clicked_row = Some(row_idx);
                }
            });
        });

    double_clicked_row
}
