//! Theme color tokens, typography settings, and style applications.
//!
//! Directly implements Section 10 of `docs/ROADMAP.md`.

use crate::core::ThemeSetting;
use egui::{Color32, Stroke, Theme, ThemePreference, Visuals};

/// Theme color tokens as defined in Section 10 of ROADMAP.md.
#[derive(Debug, Clone, Copy)]
pub struct ThemeTokens {
    pub window_background: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub primary_text: Color32,
    pub secondary_text: Color32,
    pub accent: Color32,
    pub waveform_unplayed: Color32,
}

impl ThemeTokens {
    /// Dark mode color palette.
    pub const DARK: Self = Self {
        window_background: Color32::from_rgb(0x18, 0x18, 0x18),
        panel: Color32::from_rgb(0x1f, 0x1f, 0x1f),
        raised: Color32::from_rgb(0x2a, 0x2a, 0x2a),
        primary_text: Color32::from_rgb(0xe8, 0xe8, 0xe8),
        secondary_text: Color32::from_rgb(0xa0, 0xa0, 0xa0),
        accent: Color32::from_rgb(0x4f, 0xa3, 0xff),
        waveform_unplayed: Color32::from_rgb(0x8a, 0x8a, 0x8a),
    };

    /// Light mode color palette.
    pub const LIGHT: Self = Self {
        window_background: Color32::from_rgb(0xf4, 0xf5, 0xf7),
        panel: Color32::from_rgb(0xff, 0xff, 0xff),
        raised: Color32::from_rgb(0xe9, 0xeb, 0xef),
        primary_text: Color32::from_rgb(0x1b, 0x1d, 0x21),
        secondary_text: Color32::from_rgb(0x5b, 0x62, 0x70),
        accent: Color32::from_rgb(0x1a, 0x73, 0xe8),
        waveform_unplayed: Color32::from_rgb(0x9a, 0xa0, 0xaa),
    };

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::DARK,
            Theme::Light => Self::LIGHT,
        }
    }
}

fn create_visuals(tokens: ThemeTokens, is_dark: bool) -> Visuals {
    let mut visuals = if is_dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.dark_mode = is_dark;
    visuals.panel_fill = tokens.panel;
    visuals.window_fill = tokens.window_background;
    visuals.extreme_bg_color = tokens.window_background;

    // Widgets styling
    visuals.widgets.noninteractive.bg_fill = tokens.panel;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, tokens.primary_text);

    visuals.widgets.inactive.bg_fill = tokens.panel;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, tokens.primary_text);

    visuals.widgets.hovered.bg_fill = tokens.raised;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, tokens.primary_text);

    visuals.widgets.active.bg_fill = tokens.raised;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, tokens.accent);

    visuals.selection.bg_fill = tokens.accent.gamma_multiply(0.25);
    visuals.selection.stroke = Stroke::new(1.0, tokens.accent);

    visuals
}

/// Sets up visuals and spacing for both dark and light modes, and applies the preference.
pub fn configure_theming(ctx: &egui::Context, setting: ThemeSetting) {
    ctx.set_visuals_of(Theme::Dark, create_visuals(ThemeTokens::DARK, true));
    ctx.set_visuals_of(Theme::Light, create_visuals(ThemeTokens::LIGHT, false));

    ctx.all_styles_mut(|style| {
        // Spacing: 4px grid
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
    });

    apply_theme_preference(ctx, setting);
}

/// Updates the active theme preference (System, Dark, Light).
pub fn apply_theme_preference(ctx: &egui::Context, setting: ThemeSetting) {
    let pref = match setting {
        ThemeSetting::System => ThemePreference::System,
        ThemeSetting::Dark => ThemePreference::Dark,
        ThemeSetting::Light => ThemePreference::Light,
    };
    ctx.set_theme(pref);
}
