//! Shared input telemetry painter for desktop, previews, and native Wayland.
use super::{
    fonts,
    gadget_style::{DASHBOARD_BACKGROUND, YELLOW},
};
use crate::inputs::{HISTORY_MS, InputsFrame};
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Rect, Shape, Stroke, Vec2};

pub(super) const SIZE: Vec2 = Vec2::new(660.0, 160.0);
const WHITE: Color32 = Color32::from_gray(240);
const MUTED: Color32 = Color32::from_gray(165);
const GREEN: Color32 = Color32::from_rgb(9, 182, 26);
const RED: Color32 = Color32::from_rgb(236, 35, 69);

pub(super) fn paint(
    painter: &egui::Painter,
    rect: Rect,
    frame: &InputsFrame,
    config: &crate::config::Config,
    car: Option<&str>,
) {
    let scale = (rect.width() / SIZE.x).min(rect.height() / SIZE.y);
    let origin = rect.center() - SIZE * scale / 2.0;
    let point = |x, y| origin + Vec2::new(x, y) * scale;
    let text = |x, y, value: &str, size, color, numeric| {
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
    let ratio = crate::inputs::steering_ratio(config, car);
    text(
        70.0,
        15.0,
        if ratio.is_some() {
            "STEER"
        } else {
            "ROAD STEER"
        },
        12.0,
        MUTED,
        false,
    );
    let center = point(70.0, 78.0);
    let angle = frame
        .current
        .as_ref()
        .map(|v| v.steer_rad * ratio.unwrap_or(1.0));
    let wheel_color = if angle.is_some() { WHITE } else { MUTED };
    painter.circle_stroke(center, 42.0 * scale, Stroke::new(6.0 * scale, wheel_color));
    // Screen Y points down, so positive steering rotates anticlockwise.
    let rotate = |v: Vec2| {
        let (sin, cos) = angle.unwrap_or(0.0).sin_cos();
        center + Vec2::new(v.x * cos + v.y * sin, -v.x * sin + v.y * cos) * scale
    };
    for spoke in [
        Vec2::new(-39.0, 0.0),
        Vec2::new(39.0, 0.0),
        Vec2::new(0.0, 39.0),
    ] {
        painter.line_segment(
            [center, rotate(spoke)],
            Stroke::new(5.0 * scale, wheel_color),
        );
    }
    if angle.is_some() {
        painter.line_segment(
            [rotate(Vec2::new(0.0, -38.0)), rotate(Vec2::new(0.0, -46.0))],
            Stroke::new(4.0 * scale, YELLOW),
        );
    }
    let degrees = angle.map_or("—°".into(), |v| format!("{:+.0}°", v.to_degrees()));
    text(70.0, 141.0, &degrees, 20.0, WHITE, true);

    let plot = Rect::from_min_max(point(144.0, 36.0), point(522.0, 126.0));
    let grid = Stroke::new(scale, Color32::from_white_alpha(35));
    for fraction in [0.0, 0.5, 1.0] {
        let y = plot.bottom() - fraction * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            grid,
        );
    }
    for index in 0..=5 {
        let x = plot.left() + plot.width() * index as f32 / 5.0;
        painter.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            grid,
        );
    }
    text(185.0, 15.0, "THROTTLE", 11.0, GREEN, false);
    text(266.0, 15.0, "BRAKE", 11.0, RED, false);
    text(161.0, 143.0, "−5s", 10.0, MUTED, false);
    text(507.0, 143.0, "NOW", 10.0, MUTED, false);
    if frame.current.is_some() {
        let painter = painter.with_clip_rect(plot.expand(2.0 * scale).intersect(rect));
        for (channel, color) in [(0, GREEN), (1, RED)] {
            let points: Vec<_> = frame
                .history
                .iter()
                .map(|(age, value)| {
                    egui::pos2(
                        plot.right()
                            - (*age as f32 / HISTORY_MS as f32).clamp(0.0, 1.0) * plot.width(),
                        plot.bottom() - value[channel].clamp(0.0, 1.0) * plot.height(),
                    )
                })
                .collect();
            if points.len() >= 2 {
                painter.add(Shape::line(points, Stroke::new(2.0 * scale, color)));
            }
        }
    } else {
        text(333.0, 81.0, "NO DATA", 15.0, MUTED, false);
    }
    let inputs = frame.current.as_ref();
    for (index, label, color, value) in [
        (0, "T", GREEN, inputs.map(|v| v.throttle)),
        (1, "B", RED, inputs.map(|v| v.brake)),
        (2, "C", YELLOW, inputs.map(|v| v.clutch)),
    ] {
        let x = 539.0 + index as f32 * 38.0;
        let percent = value.map_or("—".into(), |v| format!("{:.0}", v * 100.0));
        text(x + 13.0, 17.0, &percent, 15.0, WHITE, true);
        let bar = Rect::from_min_max(point(x, 36.0), point(x + 26.0, 126.0));
        painter.rect_filled(bar, 0.0, Color32::from_white_alpha(28));
        if let Some(value) = value {
            let fill = Rect::from_min_max(
                egui::pos2(
                    bar.left(),
                    bar.bottom() - value.clamp(0.0, 1.0) * bar.height(),
                ),
                bar.max,
            );
            painter.rect_filled(fill, 0.0, color);
        }
        text(x + 13.0, 143.0, label, 12.0, color, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painter_shows_real_units_missing_values_and_scales_without_speed_or_gear() {
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        for scale in [0.5, 1.0, 2.0] {
            for frame in [crate::demo::inputs_preview(2500), InputsFrame::default()] {
                let rect = Rect::from_min_size(egui::pos2(12.0, 24.0), SIZE * scale);
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    paint(
                        &ctx.layer_painter(egui::LayerId::background()),
                        rect,
                        &frame,
                        &crate::config::Config::default(),
                        Some("XFG"),
                    );
                });
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect();
                assert!(
                    labels.contains(&"STEER")
                        && labels.contains(&"THROTTLE")
                        && labels.contains(&"BRAKE")
                );
                assert!(!labels.contains(&"KPH") && !labels.contains(&"GEAR"));
                assert_eq!(labels.contains(&"NO DATA"), frame.current.is_none());
                let angle = frame.current.as_ref().map_or("—°".into(), |v| {
                    format!("{:+.0}°", (v.steer_rad * 15.0).to_degrees())
                });
                assert!(labels.contains(&angle.as_str()));
                for shape in &output.shapes {
                    assert!(
                        rect.expand(1.0)
                            .contains_rect(shape.shape.visual_bounding_rect()),
                        "shape outside gadget: {:?}",
                        shape.shape.visual_bounding_rect()
                    );
                }
            }
        }
    }
}
