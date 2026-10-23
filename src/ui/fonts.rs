//! Font registration and typography configuration.
//!
//! Embeds Poppins (Regular, Medium, SemiBold) directly into the executable.

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "Poppins-Regular".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Poppins-Regular.ttf"
        ))),
    );

    fonts.font_data.insert(
        "Poppins-Medium".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Poppins-Medium.ttf"
        ))),
    );

    fonts.font_data.insert(
        "Poppins-SemiBold".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Poppins-SemiBold.ttf"
        ))),
    );

    // Prioritize Poppins for proportional text
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "Poppins-Regular".to_owned());
    }

    ctx.set_fonts(fonts);
}
