//! Statistical Player — Desktop music player in Rust.
//!
//! Entry point wiring settings, logging, crash handling, and eframe native window.

use eframe::egui;
use statistical_player::core::Settings;
use statistical_player::logging::{init_logging, install_panic_hook};
use statistical_player::ui::StatisticalPlayerApp;

fn main() -> eframe::Result {
    // Check for CLI subcommands before initializing the GUI
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "audit" {
        if args.len() < 3 {
            eprintln!("Usage: cargo run -- audit <folder>");
            std::process::exit(1);
        }
        let folder = std::path::PathBuf::from(&args[2]);
        let report_path = std::path::PathBuf::from("docs/audit-report.md");
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
