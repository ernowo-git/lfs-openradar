//! Shared G-meter painter for independent windows and the grouped Wayland HUD.
use super::{
    fonts,
    gadget_style::{DASHBOARD_BACKGROUND, YELLOW},
};
use crate::g_force::GForceFrame;
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Rect, Stroke, Vec2};

pub(super) const SIZE: Vec2 = Vec2::new(320.0, 360.0);

pub(super) fn paint(painter: &egui::Painter, rect: Rect, frame: &GForceFrame, range_g: u8) {
    let scale = (rect.width() / SIZE.x).min(rect.height() / SIZE.y);
    let origin = rect.center() - SIZE * scale / 2.0;
    let point = |x, y| origin + Vec2::new(x, y) * scale;
    let center = point(160.0, 172.0);
    let radius = 126.0 * scale;
    let range = f32::from(range_g).max(1.0);
    let plot = |value: [f32; 2]| {
        // Show the felt horizontal load opposite vehicle acceleration.
        // Screen Y grows downward: braking goes up, acceleration goes down.
        let offset = Vec2::new(-value[0], value[1]) / range;
        center + offset / offset.length().max(1.0) * radius
    };
    let white = Color32::from_gray(242);
    let muted = Color32::from_gray(165);
    let accent = YELLOW;
    let text = |x, y, value: String, size, color, numeric| {
        painter.text(
            point(x, y),
            Align2::CENTER_CENTER,
            value,
            FontId::new(
                size * scale,
                FontFamily::Name(if numeric { fonts::NUMERIC } else { fonts::BODY }.into()),
            ),
            color,
        );
    };
    painter.rect_filled(
        Rect::from_min_size(origin, SIZE * scale),
        0.0,
        DASHBOARD_BACKGROUND,
    );
    text(160.0, 17.0, "G-FORCE".into(), 17.0, white, false);
    for ring in 1..=4 {
        painter.circle_stroke(
            center,
            radius * ring as f32 / 4.0,
            Stroke::new(scale, Color32::from_gray(if ring == 4 { 100 } else { 62 })),
        );
    }
    let grid = Stroke::new(scale, Color32::from_gray(70));
    painter.line_segment([center - Vec2::X * radius, center + Vec2::X * radius], grid);
    painter.line_segment([center - Vec2::Y * radius, center + Vec2::Y * radius], grid);
    text(160.0, 37.0, "BRAKING".into(), 12.0, muted, false);
    text(160.0, 308.0, "ACCELERATION".into(), 11.0, muted, false);
    // Draw only observed sectors. Empty directions are never filled in as grip.
    for (index, value) in frame.outline.iter().enumerate() {
        let Some(value) = value else { continue };
        let current = plot(*value);
        painter.circle_filled(current, 1.2 * scale, white);
        if let Some(Some(next)) = frame.outline.get((index + 1) % frame.outline.len()) {
            painter.line_segment([current, plot(*next)], Stroke::new(1.5 * scale, white));
        }
    }
    for (value, age) in &frame.trail {
        let opacity = (1.0 - *age as f32 / 1500.0).clamp(0.0, 1.0);
        painter.circle_filled(
            plot(*value),
            2.0 * scale,
            accent.gamma_multiply(opacity * 0.55),
        );
    }
    if let Some(value) = frame.current {
        painter.circle_filled(plot(value), 4.5 * scale, accent);
    } else {
        text(160.0, 172.0, "NO DATA".into(), 15.0, muted, false);
    }
    let value = frame
        .current
        .map_or("— G".into(), |v| format!("{:.2} G", v[0].hypot(v[1])));
    text(100.0, 337.0, value, 24.0, accent, true);
    text(246.0, 329.0, "PEAK".into(), 11.0, muted, false);
    text(
        246.0,
        347.0,
        format!("{:.2} G", frame.peak_g),
        15.0,
        white,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_plots_felt_load_opposite_vehicle_acceleration() {
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, SIZE);
        let center = egui::pos2(160.0, 172.0);
        let accent = YELLOW;
        // Right turn -> left, left turn -> right, acceleration -> down,
        // braking -> up, regardless of the car's world heading.
        for heading in [0.0_f32, std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
            for (acceleration, expected_offset) in [
                ([1.0, 0.0], Vec2::new(-63.0, 0.0)),
                ([-1.0, 0.0], Vec2::new(63.0, 0.0)),
                ([0.0, 1.0], Vec2::new(0.0, 63.0)),
                ([0.0, -1.0], Vec2::new(0.0, -63.0)),
            ] {
                let (sin, cos) = heading.sin_cos();
                let [right, forward] = acceleration;
                let world = [
                    (right * cos - forward * sin) * 9.80665,
                    (right * sin + forward * cos) * 9.80665,
                ];
                let mut packet = [0; 64];
                packet[16..20].copy_from_slice(&heading.to_le_bytes());
                packet[28..32].copy_from_slice(&world[0].to_le_bytes());
                packet[32..36].copy_from_slice(&world[1].to_le_bytes());
                let telemetry = crate::lfs::outsim::decode_telemetry(&packet, 0, None).unwrap();
                let mut meter = crate::g_force::GForceTelemetry::default();
                meter.receive(&telemetry, 0, 250);
                let frame = meter.frame(0, 250);
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    paint(
                        &ctx.layer_painter(egui::LayerId::background()),
                        rect,
                        &frame,
                        2,
                    );
                });
                for (radius, fill) in [
                    (4.5, accent),
                    (2.0, accent.gamma_multiply(0.55)),
                    (1.2, Color32::from_gray(242)),
                ] {
                    let plotted = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Circle(circle)
                                if circle.radius == radius && circle.fill == fill =>
                            {
                                Some(circle.center)
                            }
                            _ => None,
                        })
                        .expect("current dot, trail, and peak must all render");
                    assert!(
                        plotted.distance(center + expected_offset) < 0.001,
                        "heading {heading}, acceleration {acceleration:?}: plotted {plotted:?}, expected {:?}",
                        center + expected_offset
                    );
                }
                assert!((frame.peak_g - 1.0).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn shared_meter_fits_at_all_scales_and_ranges_with_and_without_data() {
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        for scale in [0.5, 1.0, 2.0] {
            for range in [1, 2, 3, 4] {
                for frame in [crate::demo::g_force_preview(1000), GForceFrame::default()] {
                    let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), SIZE * scale);
                    let output = ctx.run(egui::RawInput::default(), |ctx| {
                        paint(
                            &ctx.layer_painter(egui::LayerId::background()),
                            rect,
                            &frame,
                            range,
                        )
                    });
                    for shape in &output.shapes {
                        if let egui::Shape::Text(text) = &shape.shape {
                            assert!(
                                rect.expand(1.0).contains_rect(text.visual_bounding_rect()),
                                "clipped: {}",
                                text.galley.text()
                            );
                        }
                    }
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == "G-FORCE")));
                    assert_eq!(
                        output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == "NO DATA")),
                        frame.current.is_none()
                    );
                }
            }
        }
    }
}
