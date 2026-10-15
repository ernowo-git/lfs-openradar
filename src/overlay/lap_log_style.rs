//! GT presentation of the session best and eight most recent completed laps.
use super::{fonts, gt7_style as gt};
use crate::lap_log::{LapLogFrame, VISIBLE_ROWS};
use eframe::egui::{self, Align2, FontFamily, FontId, Rect, Vec2};

pub(super) const SIZE: Vec2 = Vec2::new(440.0, 398.0);

fn time(ms: u32) -> String {
    format!("{:02}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
}

pub(super) fn paint(painter: &egui::Painter, rect: Rect, frame: &LapLogFrame) {
    let scale = (rect.width() / SIZE.x).min(rect.height() / SIZE.y);
    let origin = rect.center() - SIZE * scale / 2.0;
    let point = |x, y| origin + Vec2::new(x, y) * scale;
    let text = |x, y, align, value: String, size, family: &str, color, width| {
        let mut font = FontId::new(size * scale, FontFamily::Name(family.into()));
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
        220.0,
        18.0,
        Align2::CENTER_CENTER,
        "LAPTIME LOG".into(),
        18.0,
        fonts::BODY,
        gt::WHITE,
        416.0,
    );
    painter.rect_filled(
        Rect::from_min_max(point(0.0, 36.0), point(440.0, 94.0)),
        0.0,
        gt::CELL_BACKGROUND,
    );
    text(
        12.0,
        65.0,
        Align2::LEFT_CENTER,
        "SESSION BEST".into(),
        17.0,
        fonts::BODY,
        gt::MUTED,
        145.0,
    );
    text(
        428.0,
        65.0,
        Align2::RIGHT_CENTER,
        frame.best_ms.map_or("—".into(), time),
        30.0,
        fonts::NUMERIC,
        gt::WHITE,
        250.0,
    );
    for (x, align, label) in [
        (16.0, Align2::LEFT_CENTER, "LAP"),
        (278.0, Align2::RIGHT_CENTER, "TIME"),
        (424.0, Align2::RIGHT_CENTER, "DELTA"),
    ] {
        text(
            x,
            113.0,
            align,
            label.into(),
            16.0,
            fonts::BODY,
            gt::MUTED,
            120.0,
        );
    }
    for index in 0..VISIBLE_ROWS {
        let top = 134.0 + index as f32 * 32.0;
        if index % 2 == 0 {
            painter.rect_filled(
                Rect::from_min_max(point(0.0, top), point(440.0, top + 32.0)),
                0.0,
                gt::CELL_BACKGROUND,
            );
        }
        let Some(lap) = frame.laps.get(index) else {
            continue;
        };
        let delta = lap.delta_ms.map_or("—".into(), |ms| {
            format!(
                "{}{:.3}",
                if ms < 0 { "−" } else { "+" },
                ms.unsigned_abs() as f64 / 1000.0
            )
        });
        let color = match lap.delta_ms {
            Some(ms) if ms < 0 => gt::GAIN_TEXT,
            Some(ms) if ms > 0 => gt::LOSS_TEXT,
            Some(_) => gt::WHITE,
            None => gt::MUTED,
        };
        text(
            16.0,
            top + 16.0,
            Align2::LEFT_CENTER,
            lap.number.to_string(),
            22.0,
            fonts::NUMERIC,
            gt::WHITE,
            72.0,
        );
        text(
            278.0,
            top + 16.0,
            Align2::RIGHT_CENTER,
            time(lap.time_ms),
            22.0,
            fonts::NUMERIC,
            gt::WHITE,
            184.0,
        );
        text(
            424.0,
            top + 16.0,
            Align2::RIGHT_CENTER,
            delta,
            22.0,
            fonts::NUMERIC,
            color,
            132.0,
        );
    }
    if frame.laps.is_empty() {
        text(
            220.0,
            250.0,
            Align2::CENTER_CENTER,
            "Waiting for a complete lap".into(),
            18.0,
            fonts::BODY,
            gt::MUTED,
            410.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gt_log_has_eight_rows_signed_deltas_and_no_temperature_at_all_scales() {
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        for scale in [0.5, 1.0, 2.0] {
            for mut frame in [crate::demo::lap_log_preview(), LapLogFrame::default()] {
                // Exercise a ninth input row and unusually wide values too.
                if let Some(last) = frame.laps.last().cloned() {
                    frame.laps.push(last);
                    frame.laps[7].number = u16::MAX;
                    frame.laps[7].time_ms = u32::MAX;
                    frame.laps[7].delta_ms = Some(i64::from(u32::MAX));
                }
                let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), SIZE * scale);
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    paint(
                        &ctx.layer_painter(egui::LayerId::background()),
                        rect,
                        &frame,
                    )
                });
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect();
                assert!(texts.iter().any(|t| t.galley.text() == "SESSION BEST"));
                assert!(!texts.iter().any(|t| t.galley.text().contains("TEMP")));
                for text in &texts {
                    let bounds = text.galley.rect.translate(text.pos.to_vec2());
                    assert!(
                        rect.expand(1.0).contains_rect(bounds),
                        "{}: {bounds:?} outside {rect:?}",
                        text.galley.text()
                    );
                }
                if !frame.laps.is_empty() {
                    assert_eq!(texts.len(), 6 + VISIBLE_ROWS * 3);
                    let faster = texts.iter().find(|t| t.galley.text() == "−0.202").unwrap();
                    assert_eq!(faster.galley.job.sections[0].format.color, gt::GAIN_TEXT);
                    assert_eq!(
                        faster.galley.job.sections[0].format.font_id.family,
                        FontFamily::Name(fonts::NUMERIC.into())
                    );
                    let slower = texts.iter().find(|t| t.galley.text() == "+0.447").unwrap();
                    assert_eq!(slower.galley.job.sections[0].format.color, gt::LOSS_TEXT);
                }
            }
        }
    }
}
