//! Compact GT traffic panel shared by the desktop and native Wayland overlays.
use super::{fonts, gt7_style as gt};
use crate::relative::RelativeFrame;
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Rect, Stroke, Vec2};

pub(super) const SIZE: Vec2 = Vec2::new(520.0, 360.0);
fn clock(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60
    )
}

pub(super) fn paint(painter: &egui::Painter, rect: Rect, frame: &RelativeFrame) {
    let scale = (rect.width() / SIZE.x).min(rect.height() / SIZE.y);
    let origin = rect.center() - SIZE * scale / 2.0;
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let point = |x, y| origin + Vec2::new(x, y) * scale;
    let text = |x, y, align, value: String, size, numeric, color, width| {
        let mut font = FontId::new(
            size * scale,
            FontFamily::Name(if numeric { fonts::NUMERIC } else { fonts::BODY }.into()),
        );
        let galley = painter.layout_no_wrap(value.clone(), font.clone(), color);
        if galley.size().x > width * scale {
            font.size *= width * scale / galley.size().x;
        }
        painter.text(point(x, y), align, value, font, color);
    };
    painter.rect_filled(
        Rect::from_min_size(origin, SIZE * scale),
        0.0,
        gt::CELL_BACKGROUND,
    );
    text(
        12.0,
        17.0,
        Align2::LEFT_CENTER,
        "RELATIVE".into(),
        17.0,
        false,
        gt::WHITE,
        220.0,
    );
    text(
        508.0,
        17.0,
        Align2::RIGHT_CENTER,
        "EST. GAP".into(),
        13.0,
        false,
        gt::MUTED,
        150.0,
    );
    for (index, row) in frame.rows.iter().enumerate() {
        let top = 34.0 + index as f32 * 38.0;
        let background = if index == 3 {
            Color32::from_rgba_unmultiplied(157, 130, 45, 160)
        } else {
            gt::CELL_BACKGROUND
        };
        painter.rect_filled(
            Rect::from_min_max(point(0.0, top), point(SIZE.x, top + 37.0)),
            0.0,
            background,
        );
        let Some(row) = row else {
            continue;
        };
        let color = if row.pit {
            Color32::from_gray(155)
        } else {
            gt::WHITE
        };
        text(
            46.0,
            top + 19.0,
            Align2::RIGHT_CENTER,
            row.position.map_or("—".into(), |p| p.to_string()),
            22.0,
            true,
            color,
            40.0,
        );
        if row.fastest {
            let center = point(66.0, top + 19.0);
            let stroke = Stroke::new(1.5 * scale, Color32::from_rgb(199, 113, 255));
            painter.circle_stroke(center, 8.0 * scale, stroke);
            painter.line_segment([center, center + Vec2::new(0.0, -5.0) * scale], stroke);
            painter.line_segment([point(62.0, top + 8.0), point(70.0, top + 8.0)], stroke);
        }
        text(
            84.0,
            top + 19.0,
            Align2::LEFT_CENTER,
            row.name.clone(),
            22.0,
            false,
            color,
            230.0,
        );
        if row.pit {
            text(
                352.0,
                top + 19.0,
                Align2::RIGHT_CENTER,
                "PIT".into(),
                14.0,
                false,
                color,
                34.0,
            );
        }
        if row.lap_difference != 0 {
            text(
                415.0,
                top + 19.0,
                Align2::RIGHT_CENTER,
                format!(
                    "{}{}L",
                    if row.lap_difference > 0 { "+" } else { "−" },
                    row.lap_difference.unsigned_abs()
                ),
                14.0,
                false,
                color,
                56.0,
            );
        }
        text(
            508.0,
            top + 19.0,
            Align2::RIGHT_CENTER,
            row.seconds.map_or("—".into(), |s| {
                let sign = if index < 3 {
                    "+"
                } else if index > 3 {
                    "-"
                } else {
                    ""
                };
                format!("{sign}{s:.1}")
            }),
            25.0,
            true,
            color,
            84.0,
        );
    }
    let elapsed = frame.elapsed_ms.map_or("—".into(), clock);
    let duration = frame
        .duration_ms
        .map_or(String::new(), |ms| format!(" / {}", clock(ms)));
    text(
        12.0,
        317.0,
        Align2::LEFT_CENTER,
        format!(
            "{}  {elapsed}{duration}",
            if frame.session.is_empty() {
                "SESSION"
            } else {
                &frame.session
            }
        ),
        16.0,
        false,
        gt::WHITE,
        310.0,
    );
    let lap = frame.current_lap.map_or("—".into(), |lap| lap.to_string());
    let total = frame
        .total_laps
        .map(|n| format!(" / {n}"))
        .or_else(|| frame.projected_laps.map(|n| format!(" / ~{n:.1}")))
        .unwrap_or_default();
    text(
        508.0,
        317.0,
        Align2::RIGHT_CENTER,
        format!("Lap {lap}{total}"),
        17.0,
        false,
        gt::WHITE,
        175.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seven_rows_and_long_names_fit_at_supported_scales() {
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        for scale in [0.5, 1.0, 2.0] {
            let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), SIZE * scale);
            let mut frame = crate::demo::relative_preview();
            frame.rows[0].as_mut().unwrap().name =
                "a very long driver name that must stay within its column".into();
            frame.rows[0].as_mut().unwrap().seconds = Some(0.8);
            frame.rows[4].as_mut().unwrap().seconds = Some(0.8);
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                paint(
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &frame,
                )
            });
            for expected in ["+0.8", "+7.4", "—", "0.0", "-0.8", "-4.8", "-6.3"] {
                assert!(
                    output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == expected)),
                    "missing {expected}"
                );
            }
            for shape in output.shapes {
                if let egui::Shape::Text(text) = shape.shape {
                    assert!(
                        rect.expand(1.0).contains_rect(text.visual_bounding_rect()),
                        "{}",
                        text.galley.text()
                    );
                }
            }
        }
    }
}
