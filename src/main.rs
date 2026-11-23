//! Statistical Player — Desktop music player in Rust.
//!
//! Entry point wiring settings, logging, crash handling, and eframe native window.

use eframe::egui;
use statistical_player::core::Settings;
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
        Box::new(|cc| Ok(Box::new(StatisticalPlayerApp::new(cc, settings)))),
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
