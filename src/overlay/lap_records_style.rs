use crate::lap_records::LapRecordsFrame;
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Rect, Vec2};

pub(super) const SIZE: Vec2 = Vec2::new(640.0, 208.0);

fn time(value: Option<u32>) -> String {
    value.map_or("—".into(), |ms| {
        format!("{}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
    })
}

pub(super) fn paint(painter: &egui::Painter, rect: Rect, frame: &LapRecordsFrame) {
    let scale = (rect.width() / SIZE.x).min(rect.height() / SIZE.y);
    let origin = rect.center() - SIZE * scale / 2.0;
    let point = |x, y| origin + Vec2::new(x, y) * scale;
    let text = |x, y, align, value: String, size, color| {
        painter.text(
            point(x, y),
            align,
            value,
            FontId::new(size * scale, FontFamily::Name(super::fonts::BODY.into())),
            color,
        );
    };
    let white = Color32::from_rgb(243, 243, 243);
    let muted = Color32::from_gray(185);
    let green = Color32::from_rgb(9, 182, 26);
    painter.rect_filled(
        Rect::from_min_size(origin, SIZE * scale),
        0.0,
        super::gadget_style::DASHBOARD_BACKGROUND,
    );
    text(
        16.0,
        24.0,
        Align2::LEFT_CENTER,
        "LAP RECORDS".into(),
        23.0,
        white,
    );
    text(
        624.0,
        24.0,
        Align2::RIGHT_CENTER,
        format!(
            "{} / {}",
            frame.track.as_deref().unwrap_or("—"),
            frame.car.as_deref().unwrap_or("—")
        ),
        21.0,
        white,
    );
    let record = frame.record.as_ref();
    let count = usize::from(record.map_or(frame.checkpoint_count, |r| r.checkpoint_count)).min(3);
    for (row, (label, columns)) in [("PB LAP", count + 1), ("BEST", count + 2)]
        .into_iter()
        .enumerate()
    {
        let y = 65.0 + row as f32 * 66.0;
        text(
            16.0,
            y + 23.0,
            Align2::LEFT_CENTER,
            label.into(),
            20.0,
            if row == 0 { white } else { green },
        );
        for index in 0..columns {
            let x = 102.0 + (index as f32 + 0.5) * 522.0 / columns as f32;
            let is_lap = index == columns - 1;
            let heading = if is_lap {
                if row == 0 { "LAP" } else { "THEORETICAL" }.into()
            } else if row == 0 {
                (1..=index + 1)
                    .map(|i| format!("S{i}"))
                    .collect::<Vec<_>>()
                    .join("+")
            } else {
                format!("S{}", index + 1)
            };
            let value = record.and_then(|record| {
                if is_lap && row == 1 {
                    record
                        .best_sectors_ms
                        .iter()
                        .try_fold(0_u32, |total, &sector| total.checked_add(sector))
                } else if is_lap {
                    Some(record.pb.lap_ms)
                } else if row == 0 {
                    record.pb.checkpoints_ms.get(index).copied()
                } else {
                    record.best_sectors_ms.get(index).copied()
                }
            });
            text(x, y, Align2::CENTER_CENTER, heading, 16.0, muted);
            painter.text(
                point(x, y + 25.0),
                Align2::CENTER_CENTER,
                time(value),
                FontId::new(20.0 * scale, FontFamily::Name(super::fonts::NUMERIC.into())),
                if row == 1 && !is_lap { green } else { white },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_cumulative_pb_and_independent_sectors_without_clipping() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        for count in 0..=3 {
            let mut frame = crate::demo::lap_records_preview();
            let record = frame.record.as_mut().unwrap();
            record.checkpoint_count = count;
            record.pb.checkpoints_ms = (1..=count).map(|i| u32::from(i) * 30_000).collect();
            record.pb.lap_ms = (u32::from(count) + 1) * 30_000;
            record.best_sectors_ms = vec![29_000; usize::from(count) + 1];
            for scale in [0.5, 1.0, 2.0] {
                let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), SIZE * scale);
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    paint(
                        &ctx.layer_painter(egui::LayerId::background()),
                        rect,
                        &frame,
                    );
                });
                assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == "0:29.000"
                        && text.galley.job.sections[0].format.font_id.family == FontFamily::Name(super::super::fonts::NUMERIC.into()))));
                assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == "PB LAP"
                        && text.galley.job.sections[0].format.font_id.family == FontFamily::Name(super::super::fonts::BODY.into()))));
                for label in [
                    "THEORETICAL".to_string(),
                    time(Some((u32::from(count) + 1) * 29_000)),
                    time(Some((u32::from(count) + 1) * 30_000)),
                ] {
                    assert!(
                        output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == label)),
                        "missing {label}"
                    );
                }
                for shape in output.shapes {
                    if let egui::Shape::Text(text) = shape.shape {
                        assert!(
                            rect.expand(1.0).contains_rect(text.visual_bounding_rect()),
                            "clipped: {}",
                            text.galley.text()
                        );
                    }
                }
            }
        }
        assert_eq!(time(Some(100_123)), "1:40.123");
        assert_eq!(time(None), "—");
    }
}
