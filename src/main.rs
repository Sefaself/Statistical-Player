//! Statistical Player — Desktop music player in Rust.
//!
//! Entry point wiring settings, logging, crash handling, and eframe native window.

use eframe::egui;
use statistical_player::audio::AudioThread;
use statistical_player::core::{
    PlaybackState, PlayerCommand, PlayerCore, RepeatMode, Settings, TrackItem,
};
use statistical_player::logging::{init_logging, install_panic_hook};
use statistical_player::ui::StatisticalPlayerApp;
use std::time::Duration;

fn main() -> eframe::Result {
    // Check for CLI subcommands before initializing the GUI
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "audit" {
        if args.len() < 3 {
            eprintln!("Usage: cargo run -- audit <folder> [--out <path>]");
            std::process::exit(1);
        }
        let mut folder_path = None;
        let mut out_path = None;
        let mut i = 2;
        while i < args.len() {
            if args[i] == "--out" {
                i += 1;
                if i < args.len() {
                    out_path = Some(std::path::PathBuf::from(&args[i]));
                } else {
                    eprintln!("Missing value for --out");
                    std::process::exit(1);
                }
            } else if folder_path.is_none() {
                folder_path = Some(std::path::PathBuf::from(&args[i]));
            }
            i += 1;
        }

        let folder = match folder_path {
            Some(f) => f,
            None => {
                eprintln!("Usage: cargo run -- audit <folder> [--out <path>]");
                std::process::exit(1);
            }
        };

        let report_path =
            out_path.unwrap_or_else(|| std::path::PathBuf::from("docs/audit-report.md"));
        match statistical_player::library::run_audit(&folder, Some(&report_path)) {
            Ok(report) => {
                println!("{}", report.render_console());
                println!("Report successfully saved to {}\n", report_path.display());
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!(
                    "Error executing library audit on {}: {e:#}",
                    folder.display()
                );
                std::process::exit(1);
            }
        }
    } else if args.len() >= 2 && args[1] == "play" {
        if args.len() < 3 {
            eprintln!("Usage: cargo run -- play <file> [--seek <seconds>]");
            std::process::exit(1);
        }
        let mut file_path = None;
        let mut seek_secs = None;
        let mut i = 2;
        while i < args.len() {
            if args[i] == "--seek" {
                i += 1;
                if i < args.len() {
                    seek_secs = args[i].parse::<f64>().ok();
                } else {
                    eprintln!("Missing value for --seek");
                    std::process::exit(1);
                }
            } else if file_path.is_none() {
                file_path = Some(std::path::PathBuf::from(&args[i]));
            }
            i += 1;
        }

        let file = match file_path {
            Some(f) => f,
            None => {
                eprintln!("Usage: cargo run -- play <file> [--seek <seconds>]");
                std::process::exit(1);
            }
        };

        let seek = seek_secs.map(std::time::Duration::from_secs_f64);
        if let Err(e) = run_play_cli(&file, seek) {
            eprintln!("Playback error on {}: {e:#}", file.display());
            std::process::exit(1);
        }
        std::process::exit(0);
    } else if args.len() >= 2 && args[1] == "queue" {
        if args.len() < 3 {
            eprintln!("Usage: cargo run -- queue <file1> <file2> ... [--repeat off|track|queue]");
            std::process::exit(1);
        }
        let mut files = Vec::new();
        let mut repeat_mode = RepeatMode::Off;
        let mut i = 2;
        while i < args.len() {
            if args[i] == "--repeat" {
                i += 1;
                if i < args.len() {
                    match args[i].to_lowercase().as_str() {
                        "off" => repeat_mode = RepeatMode::Off,
                        "track" => repeat_mode = RepeatMode::Track,
                        "queue" => repeat_mode = RepeatMode::Queue,
                        other => {
                            eprintln!(
                                "Invalid repeat mode: '{other}'. Expected off, track, or queue."
                            );
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!("Missing value for --repeat");
                    std::process::exit(1);
                }
            } else {
                files.push(std::path::PathBuf::from(&args[i]));
            }
            i += 1;
        }

        if files.is_empty() {
            eprintln!("Usage: cargo run -- queue <file1> <file2> ... [--repeat off|track|queue]");
            std::process::exit(1);
        }

        if let Err(e) = run_queue_cli(&files, repeat_mode) {
            eprintln!("Queue error: {e:#}");
            std::process::exit(1);
        }
        std::process::exit(0);
    } else if args.len() >= 2 && args[1] == "scan" {
        if args.len() < 3 {
            eprintln!("Usage: cargo run -- scan <folder>");
            std::process::exit(1);
        }
        let folder = std::path::PathBuf::from(&args[2]);
        if !folder.is_dir() {
            eprintln!("Error: '{}' is not a valid directory.", folder.display());
            std::process::exit(1);
        }
        run_scan_cli(&folder);
        std::process::exit(0);
    }

    // 1. Load settings (or default if absent/unreadable)
    let settings = Settings::load();

    // 2. Setup logging directory in OS application data folder
    let log_dir = Settings::data_dir().map(|d| d.join("logs"));
    let _log_guard = init_logging(log_dir.clone());
    install_panic_hook(log_dir);

    tracing::info!(
        "Starting Statistical Player v{} on {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([settings.window_width, settings.window_height])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Statistical Player"),
        ..Default::default()
    };

    eframe::run_native(
        "Statistical Player",
        native_options,
        Box::new(|cc| {
            let audio = AudioThread::spawn()
                .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(format!("{e:#}")))?;
            let ctx = cc.egui_ctx.clone();
            let wake_ui = Box::new(move || {
                ctx.request_repaint();
            });
            let player = PlayerCore::spawn(audio, wake_ui);
            Ok(Box::new(StatisticalPlayerApp::new(cc, settings, player)))
        }),
    )
}

fn to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.00001 {
        -100.0
    } else {
        20.0 * amplitude.log10()
    }
}

fn format_duration(d: std::time::Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins:02}:{secs:02}")
}

fn run_play_cli(file: &std::path::Path, seek: Option<std::time::Duration>) -> anyhow::Result<()> {
    println!("Initializing audio output device...");
    let mut engine = statistical_player::audio::AudioEngine::new()?;
    let meta = engine.load_file(file)?;

    let dev_rate = engine.sink_sample_rate();
    let signal_path = if meta.sample_rate == dev_rate {
        format!(
            "{} -> f32 (direct {:.1} kHz)",
            meta.format_line(),
            dev_rate as f32 / 1000.0
        )
    } else {
        format!(
            "{} -> f32 -> resample to {:.1} kHz",
            meta.format_line(),
            dev_rate as f32 / 1000.0
        )
    };

    tracing::info!("Signal path: {signal_path}");
    println!("Signal path: {signal_path}");

    if let Some(seek_dur) = seek {
        println!("Seeking to {}...", format_duration(seek_dur));
        engine.seek(seek_dur)?;
    }

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = std::sync::Arc::clone(&running);
    if let Err(e) = ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    }) {
        tracing::warn!("Failed to install Ctrl+C signal handler: {e}");
    }

    println!("Playing: {} (Press Ctrl+C to quit)\n", file.display());
    engine.play()?;
    let playback_start = std::time::Instant::now();

    let mut last_print = playback_start - std::time::Duration::from_secs(1);
    let total_dur = meta.duration.unwrap_or_default();

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        engine.fill_buffer();

        if last_print.elapsed() >= std::time::Duration::from_secs(1) {
            let pos = engine.position();
            let total_str = if total_dur.as_secs() > 0 {
                format_duration(total_dur)
            } else {
                "??:??".to_string()
            };
            print!(
                "\rPosition: {} / {} | Peak [L: {:>5.1} dB, R: {:>5.1} dB] ",
                format_duration(pos),
                total_str,
                to_db(engine.peak_left()),
                to_db(engine.peak_right()),
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            last_print = std::time::Instant::now();
        }

        if engine.is_track_ended() {
            if engine.frames_written() == 0 {
                let err_msg = engine
                    .last_error()
                    .map(|e| format!("Track produced zero audio frames: {e}"))
                    .unwrap_or_else(|| {
                        "Track produced zero audio frames (file may be empty, corrupt, or unsupported)"
                            .to_string()
                    });
                eprintln!("\nError: {err_msg}");
                anyhow::bail!("{err_msg}");
            }
            println!("\nEnd of track reached.");
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    if !running.load(std::sync::atomic::Ordering::SeqCst) {
        println!("\nPlayback stopped by user (Ctrl+C).");
    }

    let wall_clock = playback_start.elapsed();
    let audio_length = engine
        .position()
        .saturating_sub(seek.unwrap_or(Duration::ZERO));

    println!(
        "\nPlayback summary:\n  - Audio length:    {} ({:.2} s)\n  - Wall-clock time: {} ({:.2} s)\n  - Device rate:     {} Hz\n  - Underrun count:  {}",
        format_duration(audio_length),
        audio_length.as_secs_f64(),
        format_duration(wall_clock),
        wall_clock.as_secs_f64(),
        dev_rate,
        engine.underruns(),
    );

    Ok(())
}

fn run_queue_cli(files: &[std::path::PathBuf], repeat_mode: RepeatMode) -> anyhow::Result<()> {
    println!("Initializing audio output device...");
    let audio = AudioThread::spawn()?;
    let (wake_tx, wake_rx) = crossbeam_channel::unbounded();
    let wake_ui = Box::new(move || {
        let _ = wake_tx.send(());
    });

    let player = PlayerCore::spawn(audio, wake_ui);

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = std::sync::Arc::clone(&running);
    if let Err(e) = ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    }) {
        tracing::warn!("Failed to install Ctrl+C signal handler: {e}");
    }

    player.send(PlayerCommand::SetRepeat(repeat_mode))?;

    let tracks: Vec<TrackItem> = files
        .iter()
        .enumerate()
        .map(|(id, path)| TrackItem::simple(id as u64, path.clone()))
        .collect();

    println!(
        "Queued {} track(s) (Repeat mode: {:?})",
        tracks.len(),
        repeat_mode
    );
    for (i, t) in tracks.iter().enumerate() {
        println!("  {}. {}", i + 1, t.path.display());
    }
    println!("Press Ctrl+C to stop.\n");

    player.send(PlayerCommand::ReplaceQueue(tracks))?;

    let mut last_state = None;
    let mut last_index = None;
    let mut last_generation = 0_u32;
    let mut loops_completed = 0;
    let total_tracks = files.len();

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        let _ = wake_rx.recv_timeout(Duration::from_millis(50));
        let snap = player.snapshot();
        let current_gen = player.status().generation();

        // Print track change
        if current_gen != last_generation && current_gen > 0 {
            if let Some(idx) = snap.current_index {
                // If we wrapped around to track 0 from the last track
                if last_index == Some(total_tracks.saturating_sub(1))
                    && idx == 0
                    && last_generation > 0
                {
                    loops_completed += 1;
                    println!("\n--- Completed loop {} of queue ---", loops_completed);
                    if repeat_mode == RepeatMode::Queue && loops_completed >= 2 {
                        println!("Queue mode reached limit of 2 full loops. Stopping.");
                        let _ = player.send(PlayerCommand::Stop);
                        break;
                    }
                }

                let track_title = snap
                    .current_track()
                    .map(|t| t.path.display().to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                println!("[Track #{}] Loading: {}", idx + 1, track_title);
                last_index = Some(idx);
            }
            last_generation = current_gen;
        }

        // Print state change
        if last_state != Some(snap.state) {
            println!("[State] {:?}", snap.state);
            last_state = Some(snap.state);

            if snap.state == PlaybackState::Stopped && last_generation > 0 {
                if let Some(ref err) = snap.error_message {
                    eprintln!("[Error] {err}");
                } else if let Some(ref status) = snap.status_message {
                    println!("[Status] {status}");
                }
                break;
            }
        }
    }

    if !running.load(std::sync::atomic::Ordering::SeqCst) {
        println!("\nPlayback stopped by user (Ctrl+C).");
        let _ = player.send(PlayerCommand::Stop);
    }

    let underruns = player.status().underruns();
    println!("\nPlayback finished. Final underrun count: {}", underruns);

    Ok(())
}

fn run_scan_cli(folder: &std::path::Path) {
    println!("Scanning folder: {}", folder.display());
    let start = std::time::Instant::now();
    let (tx, rx) = crossbeam_channel::unbounded();
    let handle = statistical_player::library::load_folder(folder.to_path_buf(), tx);

    let mut time_to_first_batch: Option<Duration> = None;
    let mut batch_count = 0usize;

    while let Ok(ev) = rx.recv() {
        match ev {
            statistical_player::library::LoaderEvent::Batch(batch) => {
                batch_count += 1;
                if time_to_first_batch.is_none() {
                    let t = start.elapsed();
                    time_to_first_batch = Some(t);
                    println!(
                        "First batch received in {:.3} s ({} tracks)",
                        t.as_secs_f64(),
                        batch.len()
                    );
                }
            }
            statistical_player::library::LoaderEvent::Done {
                total,
                skipped,
                elapsed,
                ..
            } => {
                let first_str = time_to_first_batch
                    .map(|d| format!("{:.3} s", d.as_secs_f64()))
                    .unwrap_or_else(|| "N/A (0 batches)".to_string());
                println!(
                    "\nScan complete:\n  - Time to first batch: {}\n  - Total time:          {:.3} s\n  - Batches emitted:     {}\n  - Track count:         {}\n  - Skipped count:       {}",
                    first_str,
                    elapsed.as_secs_f64(),
                    batch_count,
                    total,
                    skipped
                );
                break;
            }
        }
    }

    let _ = handle.join();
}
