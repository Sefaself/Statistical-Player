//! Main application shell and UI window management.

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{Receiver, unbounded};
use egui::{CentralPanel, RichText, containers::panel::Panel};

use crate::core::{PlayerCommand, PlayerHandle, PlayerSnapshot, Settings, ThemeSetting, TrackItem};
use crate::library::{LoaderEvent, load_folder};
use crate::ui::control_bar::show_control_bar;
use crate::ui::queue_table::show_queue_table;
use crate::ui::theme::{ThemeTokens, apply_theme_preference, configure_theming};

pub struct StatisticalPlayerApp {
    settings: Settings,
    player: PlayerHandle,
    snapshot: Arc<PlayerSnapshot>,
    ui_tracks: Vec<TrackItem>,
    is_loading: bool,
    display_core_queue: bool,
    folder_pick_rx: Option<Receiver<Option<PathBuf>>>,
    loader_rx: Option<Receiver<LoaderEvent>>,
    loader_handle: Option<std::thread::JoinHandle<()>>,
}

impl StatisticalPlayerApp {
    pub fn new(cc: &eframe::CreationContext, settings: Settings, player: PlayerHandle) -> Self {
        // Register custom fonts
        crate::ui::fonts::configure_fonts(&cc.egui_ctx);

        // Configure theme tokens and preference
        configure_theming(&cc.egui_ctx, settings.theme);

        let snapshot = player.snapshot();

        Self {
            settings,
            player,
            snapshot,
            ui_tracks: Vec::new(),
            is_loading: false,
            display_core_queue: false,
            folder_pick_rx: None,
            loader_rx: None,
            loader_handle: None,
        }
    }

    fn start_folder_picker(&mut self, ctx: &egui::Context) {
        if self.folder_pick_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        let ctx_clone = ctx.clone();
        std::thread::Builder::new()
            .name("folder-picker".to_string())
            .spawn(move || {
                let picked = rfd::FileDialog::new().pick_folder();
                let _ = tx.send(picked);
                ctx_clone.request_repaint();
            })
            .ok();
        self.folder_pick_rx = Some(rx);
    }

    fn start_loading_folder(&mut self, folder: PathBuf, ctx: &egui::Context) {
        self.settings.last_opened_folder = Some(folder.clone());
        let _ = self.settings.save();

        self.ui_tracks.clear();
        self.is_loading = true;
        self.display_core_queue = false;

        let (raw_tx, raw_rx) = unbounded::<LoaderEvent>();
        let (ui_tx, ui_rx) = unbounded::<LoaderEvent>();
        let ctx_clone = ctx.clone();

        // Forwarder thread wakes egui only when loader events arrive, keeping loader.rs egui-free
        std::thread::Builder::new()
            .name("loader-ui-wake".to_string())
            .spawn(move || {
                while let Ok(ev) = raw_rx.recv() {
                    let is_done = matches!(ev, LoaderEvent::Done { .. });
                    if ui_tx.send(ev).is_err() {
                        break;
                    }
                    ctx_clone.request_repaint();
                    if is_done {
                        break;
                    }
                }
            })
            .ok();

        let handle = load_folder(folder, raw_tx);
        self.loader_rx = Some(ui_rx);
        self.loader_handle = Some(handle);
    }

    fn poll_background_events(&mut self, ctx: &egui::Context) {
        // 1. Poll native folder picker result
        if let Some(ref rx) = self.folder_pick_rx
            && let Ok(picked_opt) = rx.try_recv()
        {
            self.folder_pick_rx = None;
            if let Some(folder) = picked_opt {
                self.start_loading_folder(folder, ctx);
            }
        }

        // 2. Poll progressive folder loader batches
        let mut finished = false;
        if let Some(ref rx) = self.loader_rx {
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    LoaderEvent::Batch(mut batch) => {
                        self.ui_tracks.append(&mut batch);
                    }
                    LoaderEvent::Done { tracks, .. } => {
                        self.ui_tracks = tracks;
                        self.is_loading = false;
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.loader_rx = None;
            if let Some(h) = self.loader_handle.take() {
                let _ = h.join();
            }
        }
    }
}

impl eframe::App for StatisticalPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Poll loader/picker channels and read latest snapshot (no request_repaint here!)
        self.poll_background_events(ui.ctx());

        let latest = self.player.snapshot();
        if latest.version != self.snapshot.version {
            self.snapshot = latest;
        }

        let current_theme = ui.ctx().theme();
        let tokens = ThemeTokens::for_theme(current_theme);

        // Top bar: App title, Open folder button, and Theme toggle
        Panel::top("top_panel").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("Statistical Player")
                        .size(16.0)
                        .strong()
                        .color(tokens.primary_text),
                );

                ui.add_space(12.0);
                if ui.button("Open folder").clicked() {
                    self.start_folder_picker(ui.ctx());
                }

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

            if self.is_loading {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!("Loading {} tracks...", self.ui_tracks.len()))
                        .size(12.0)
                        .color(tokens.secondary_text),
                );
            }
            ui.add_space(4.0);
        });

        // Bottom control bar
        Panel::bottom("bottom_bar").min_size(56.0).show(ui, |ui| {
            ui.add_space(4.0);
            show_control_bar(ui, &self.player, &self.snapshot, tokens);
            ui.add_space(4.0);
        });

        // Central content area: virtualized queue table
        CentralPanel::default().show(ui, |ui| {
            let tracks_to_show = if self.display_core_queue {
                &self.snapshot.queue
            } else {
                &self.ui_tracks
            };

            if tracks_to_show.is_empty() && !self.is_loading {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("Click \"Open folder\" to load a music folder.")
                            .size(14.0)
                            .color(tokens.secondary_text),
                    );
                });
                return;
            }

            let playing_index = if self.display_core_queue {
                self.snapshot.current_index
            } else {
                None
            };

            if let Some(clicked_row) = show_queue_table(ui, tracks_to_show, playing_index, tokens) {
                if !self.display_core_queue {
                    let _ = self
                        .player
                        .send(PlayerCommand::ReplaceQueue(self.ui_tracks.clone()));
                    if clicked_row != 0 {
                        let _ = self.player.send(PlayerCommand::PlayIndex(clicked_row));
                    }
                    self.display_core_queue = true;
                } else {
                    let _ = self.player.send(PlayerCommand::PlayIndex(clicked_row));
                }
            }
        });
    }

    fn on_exit(&mut self) {
        let _ = self.settings.save();
    }
}
