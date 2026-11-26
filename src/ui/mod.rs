//! UI module: themes, fonts, components, panels, and eframe application.

pub mod app;
pub mod control_bar;
pub mod fonts;
pub mod queue_table;
pub mod theme;

pub use app::StatisticalPlayerApp;
pub use theme::{ThemeTokens, apply_theme_preference, configure_theming};
