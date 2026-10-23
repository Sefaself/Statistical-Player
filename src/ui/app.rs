//! Main application shell and UI window management.

use crate::core::{PlayerSnapshot, Settings, ThemeSetting};
use crate::ui::theme::{ThemeTokens, apply_theme_preference, configure_theming};
use egui::{CentralPanel, RichText, containers::panel::Panel};

pub struct StatisticalPlayerApp {
    settings: Settings,
    snapshot: PlayerSnapshot,
}

impl StatisticalPlayerApp {
    pub fn new(cc: &eframe::CreationContext, settings: Settings) -> Self {
        // Register custom fonts
        crate::ui::fonts::configure_fonts(&cc.egui_ctx);

        // Configure theme tokens and preference
        configure_theming(&cc.egui_ctx, settings.theme);

        Self {
            settings,
            snapshot: PlayerSnapshot::default(),
        }
    }
}

impl eframe::App for StatisticalPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let current_theme = ui.ctx().theme();
        let tokens = ThemeTokens::for_theme(current_theme);

        // Top bar: App title and Theme toggle
        Panel::top("top_panel").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("Statistical Player")
                        .size(16.0)
                        .strong()
                        .color(tokens.primary_text),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let next_theme = match self.settings.theme {
                        ThemeSetting::System => "Theme: System",
                        ThemeSetting::Dark => "Theme: Dark",
                        ThemeSetting::Light => "Theme: Light",
                    };

                    if ui.button(next_theme).clicked() {
                        self.settings.theme = match self.settings.theme {
                            ThemeSetting::System => ThemeSetting::Dark,
                            ThemeSetting::Dark => ThemeSetting::Light,
                            ThemeSetting::Light => ThemeSetting::System,
                        };
                        apply_theme_preference(ui.ctx(), self.settings.theme);
                        let _ = self.settings.save();
                    }
                });
            });
            ui.add_space(4.0);
        });

        // Bottom control bar placeholder
        Panel::bottom("bottom_bar").min_size(56.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new("Nothing playing")
                        .color(tokens.secondary_text)
                        .italics(),
                );
            });
        });

        // Central content area (Phase 0 Foundation Shell)
        CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new("Statistical Player — Foundation (v0.0)")
                        .size(18.0)
                        .color(tokens.accent)
                        .strong(),
                );
                ui.add_space(12.0);
                ui.label(
                    RichText::new(
                        "Phase 0 foundation is active. Ready to build Phase 1 Core Playback Beta.",
                    )
                    .size(13.0)
                    .color(tokens.secondary_text),
                );

                ui.add_space(24.0);
                ui.group(|ui| {
                    ui.set_width(420.0);
                    ui.label(
                        RichText::new("Environment Status")
                            .strong()
                            .color(tokens.primary_text),
                    );
                    ui.separator();
                    ui.label(format!("• Active Theme: {:?}", self.settings.theme));
                    ui.label(format!(
                        "• Target Visualizer FPS: {}",
                        self.settings.visualizer_fps
                    ));
                    ui.label(format!(
                        "• Background Playback: {}",
                        self.settings.keep_playing_in_background
                    ));
                    ui.label(format!("• Engine State: {:?}", self.snapshot.state));
                    ui.label(format!("• Queued Tracks: {}", self.snapshot.queue.len()));
                });
            });
        });
    }

    fn on_exit(&mut self) {
        // Persist settings on normal exit
        let _ = self.settings.save();
    }
}
