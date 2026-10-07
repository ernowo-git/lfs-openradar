//! Embedded fonts used only by the GT7-inspired HUD families.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

pub(super) const BODY: &str = "gt7-body";
pub(super) const DISPLAY: &str = "gt7-display";

pub(super) fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    for (name, bytes) in [
        (
            BODY,
            include_bytes!("../../assets/fonts/Roboto.ttf").as_slice(),
        ),
        (
            DISPLAY,
            include_bytes!("../../assets/fonts/Orbitron.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
        let mut family = vec![name.into()];
        family.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    ctx.set_fonts(fonts);
}
