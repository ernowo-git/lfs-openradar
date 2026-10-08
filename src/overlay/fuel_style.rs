//! Fuel layout from Frame 13.svg, with the speed dashboard's panel background.
use crate::fuel::FuelFrame;
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Rect, Stroke, TextureHandle, Vec2};

pub(super) const SIZE: Vec2 = Vec2::new(534.0, 224.0);
const WHITE: Color32 = Color32::from_rgb(243, 243, 243);
const GREEN: Color32 = Color32::from_rgb(9, 182, 26);
const RED: Color32 = Color32::from_rgb(236, 35, 69);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Indicator {
    Safe,
    Warning,
    Danger,
    Unknown,
}

fn indicator(frame: &FuelFrame) -> Indicator {
    if frame.fraction.is_none() {
        Indicator::Unknown
    } else if frame.fraction == Some(0.0) {
        Indicator::Danger
    } else {
        match frame.rows[0]
            .laps
            .filter(|laps| laps.is_finite() && *laps >= 0.0)
        {
            Some(laps) if laps <= 1.0 => Indicator::Danger,
            Some(laps) if laps <= 2.0 => Indicator::Warning,
            Some(_) => Indicator::Safe,
            None => Indicator::Unknown,
        }
    }
}

fn badge_texture(ctx: &egui::Context, state: Indicator, pixels: f32) -> TextureHandle {
    let size = (pixels.ceil() as u32).max(1).next_power_of_two();
    let id = egui::Id::new(("fuel-indicator", state as u8, size));
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let svg: &[u8] = match state {
        Indicator::Safe => include_bytes!("../../assets/indicators/fuel-safe.svg"),
        Indicator::Warning => include_bytes!("../../assets/indicators/fuel-warning.svg"),
        Indicator::Danger => include_bytes!("../../assets/indicators/fuel-danger.svg"),
        Indicator::Unknown => include_bytes!("../../assets/indicators/fuel-pump.svg"),
    };
    let image = egui_extras::image::load_svg_bytes_with_size(
        svg,
        egui::SizeHint::Width(size),
        &Default::default(),
    )
    .expect("valid embedded fuel indicator SVG");
    let texture = ctx.load_texture("fuel-indicator", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

fn decimal(value: Option<f64>) -> String {
    value.map_or("—".into(), |v| {
        if v > 999.9 {
            ">999".into()
        } else {
            format!("{v:.1}")
        }
    })
}
fn margin(value: Option<f64>) -> String {
    value.map_or("—".into(), |v| {
        if v.abs() < 0.05 {
            "0.0".into()
        } else if v > 999.9 {
            ">+999".into()
        } else if v < -999.9 {
            "<−999".into()
        } else if v < 0.0 {
            format!("−{:.1}", -v)
        } else {
            format!("+{v:.1}")
        }
    })
}
fn refuel(value: Option<f64>, multiple: bool) -> String {
    value.map_or("—".into(), |v| {
        format!(
            "{:.0}%{}",
            (v * 100.0 - 1e-6).max(0.0).ceil(),
            if multiple { "*" } else { "" }
        )
    })
}

pub(super) fn paint(painter: &egui::Painter, rect: Rect, frame: &FuelFrame) {
    let scale = (rect.width() / SIZE.x).min(rect.height() / SIZE.y);
    let origin = rect.center() - SIZE * scale * 0.5;
    let point = |x, y| origin + Vec2::new(x, y) * scale;
    let panel = |x, y, w, h| Rect::from_min_size(point(x, y), Vec2::new(w, h) * scale);
    let body = FontFamily::Name(super::fonts::BODY.into());
    let numeric = FontFamily::Name(super::fonts::NUMERIC.into());
    let text = |x, y, align, value: String, size, family: FontFamily, color| {
        painter.text(
            point(x, y),
            align,
            value,
            FontId::new(size * scale, family),
            color,
        );
    };
    painter.rect_filled(
        panel(0.0, 66.5, SIZE.x, 157.5),
        0.0,
        super::gadget_style::DASHBOARD_BACKGROUND,
    );
    painter.rect_filled(
        panel(20.4, 111.3, 374.7, 36.7),
        0.0,
        Color32::from_rgba_unmultiplied_const(15, 15, 15, 178),
    );
    painter.line_segment(
        [point(0.0, 66.5), point(SIZE.x, 66.5)],
        Stroke::new(scale, GREEN),
    );
    text(
        16.0,
        34.0,
        Align2::LEFT_CENTER,
        "LAPS UNTIL EMPTY".into(),
        20.0,
        body.clone(),
        WHITE,
    );
    text(
        328.0,
        34.0,
        Align2::CENTER_CENTER,
        decimal(frame.rows[0].laps),
        32.0,
        numeric.clone(),
        WHITE,
    );
    text(
        513.0,
        34.0,
        Align2::RIGHT_CENTER,
        margin(frame.margin),
        22.0,
        numeric.clone(),
        if frame.margin.is_some_and(|v| v <= -0.05) {
            RED
        } else if frame.margin.is_some_and(|v| v >= 0.05) {
            GREEN
        } else {
            WHITE
        },
    );
    for (x, label) in [(137.0, "USAGE"), (238.0, "LAPS"), (342.0, "REFUEL")] {
        text(
            x,
            95.0,
            Align2::CENTER_CENTER,
            label.into(),
            21.0,
            body.clone(),
            WHITE,
        );
    }
    for (index, (label, row)) in ["AVG", "MAX", "MIN"]
        .into_iter()
        .zip(&frame.rows)
        .enumerate()
    {
        let y = [130.0, 164.0, 199.0][index];
        text(
            27.0,
            y,
            Align2::LEFT_CENTER,
            label.into(),
            21.0,
            body.clone(),
            WHITE,
        );
        text(
            173.0,
            y,
            Align2::RIGHT_CENTER,
            row.usage
                .map_or("—".into(), |v| format!("{:.1}%", v * 100.0)),
            20.0,
            numeric.clone(),
            WHITE,
        );
        text(
            265.0,
            y,
            Align2::RIGHT_CENTER,
            decimal(row.laps),
            22.0,
            numeric.clone(),
            WHITE,
        );
        text(
            378.0,
            y,
            Align2::RIGHT_CENTER,
            refuel(row.refuel, row.multiple_stops),
            20.0,
            numeric.clone(),
            WHITE,
        );
    }
    let state = indicator(frame);
    let badge_rect = if state == Indicator::Unknown {
        painter.circle_filled(point(480.6, 111.3), 21.7 * scale, Color32::GRAY);
        panel(469.7, 100.5, 21.7, 21.7)
    } else {
        panel(458.9, 89.6, 43.4, 43.4)
    };
    let icon = badge_texture(
        painter.ctx(),
        state,
        badge_rect.width() * painter.ctx().pixels_per_point(),
    );
    painter.image(
        icon.id(),
        badge_rect,
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    text(
        480.6,
        161.0,
        Align2::CENTER_CENTER,
        frame
            .fraction
            .map_or("—".into(), |v| format!("{:.0}%", v * 100.0)),
        21.0,
        numeric,
        WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indicator_thresholds_and_supplied_assets_follow_average_range() {
        let mut frame = crate::demo::fuel_preview();
        frame.low_fuel = Some(true); // Badge uses lap range, not LFS's warning lamp.
        for (laps, expected) in [
            (12.9, Indicator::Safe),
            (2.0001, Indicator::Safe),
            (2.0, Indicator::Warning),
            (1.0001, Indicator::Warning),
            (1.0, Indicator::Danger),
            (0.0, Indicator::Danger),
        ] {
            frame.rows[0].laps = Some(laps);
            assert_eq!(indicator(&frame), expected);
        }
        for laps in [None, Some(f64::NAN), Some(f64::INFINITY)] {
            frame.rows[0].laps = laps;
            assert_eq!(indicator(&frame), Indicator::Unknown);
        }
        frame.fraction = Some(0.0);
        assert_eq!(indicator(&frame), Indicator::Danger);
        frame.fraction = None;
        frame.rows[0].laps = Some(0.5);
        assert_eq!(indicator(&frame), Indicator::Unknown);

        let ctx = egui::Context::default();
        for (state, color) in [
            (Indicator::Safe, Color32::from_rgb(9, 182, 26)),
            (Indicator::Warning, Color32::from_rgb(254, 234, 81)),
            (Indicator::Danger, Color32::from_rgb(233, 48, 74)),
        ] {
            let mut id = None;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                let texture = badge_texture(ctx, state, 32.0);
                assert_eq!(texture.id(), badge_texture(ctx, state, 32.0).id());
                id = Some(texture.id());
            });
            let (_, delta) = output
                .textures_delta
                .set
                .iter()
                .find(|(texture_id, _)| Some(*texture_id) == id)
                .unwrap();
            let egui::ImageData::Color(image) = &delta.image;
            assert_eq!(image.size, [32, 32]);
            assert_eq!(image.pixels[16 * 32 + 2], color);
        }
    }

    #[test]
    fn layout_matches_reference_and_formats_shortage_and_refuel() {
        assert_eq!(margin(Some(12.9 - 20.6)), "−7.7");
        assert_eq!(margin(Some(2.3)), "+2.3");
        assert_eq!(margin(Some(-0.01)), "0.0");
        assert_eq!(refuel(Some(0.08001), false), "9%");
        assert_eq!(refuel(Some(0.08), true), "8%*");
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let frame = crate::demo::fuel_preview();
        for scale in [0.5, 1.0, 2.0] {
            let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), SIZE * scale);
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                paint(
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &frame,
                );
            });
            assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Text(t) if t.galley.text() == "−7.7")));
            assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Text(t) if t.galley.text() == "14%")));
            assert!(!output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Text(t) if t.galley.text().ends_with(" lt"))));
            assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Rect(r) if r.fill == super::super::gadget_style::DASHBOARD_BACKGROUND
                    && (r.rect.height() - 157.5 * scale).abs() < 0.01)));
            for shape in output.shapes {
                if let egui::Shape::Text(text) = shape.shape {
                    assert!(
                        rect.expand(1.0).contains_rect(text.visual_bounding_rect()),
                        "text clips: {}",
                        text.galley.text()
                    );
                }
            }
        }
    }
}
