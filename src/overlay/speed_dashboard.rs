//! Speed dashboard matching the supplied reference. All coordinates are logical pixels.
use crate::{config::Config, dashboard::DashboardFrame, lfs::outgauge};
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Pos2, Rect, TextureHandle, Vec2};

pub(super) const SIZE: Vec2 = Vec2::new(574.0, 210.0);
const WHITE: Color32 = Color32::from_rgb(240, 240, 240);
const YELLOW: Color32 = Color32::from_rgb(255, 235, 63);
const RED: Color32 = Color32::from_rgb(236, 35, 69);
const BACKGROUND: Color32 = Color32::from_rgba_unmultiplied_const(37, 37, 37, 102);
const RPM_MARKER: Color32 = Color32::from_rgb(37, 37, 37);

#[derive(Clone)]
struct Icons(Vec<TextureHandle>);
fn icons(ctx: &egui::Context, physical_size: f32) -> Icons {
    // Round up to a few reusable resolutions for the preview, overlay, and display DPI.
    let size = (physical_size.ceil() as u32).max(1).next_power_of_two();
    let id = egui::Id::new(("speed-dashboard-icons", size));
    if let Some(icons) = ctx.data(|data| data.get_temp::<Icons>(id)) {
        return icons;
    }
    let images: &[(&str, &[u8])] = &[
        (
            "abs-off",
            include_bytes!("../../assets/indicators/abs-off.svg"),
        ),
        (
            "abs-on",
            include_bytes!("../../assets/indicators/abs-on.svg"),
        ),
        (
            "abs-warning",
            include_bytes!("../../assets/indicators/abs-warning.svg"),
        ),
        (
            "tc-off",
            include_bytes!("../../assets/indicators/tc-off.svg"),
        ),
        ("tc-on", include_bytes!("../../assets/indicators/tc-on.svg")),
        (
            "engine-off",
            include_bytes!("../../assets/indicators/engine-off.svg"),
        ),
        (
            "engine-minor",
            include_bytes!("../../assets/indicators/engine-minor.svg"),
        ),
        (
            "engine-major",
            include_bytes!("../../assets/indicators/engine-major.svg"),
        ),
        (
            "headlights-off",
            include_bytes!("../../assets/indicators/headlights-off.svg"),
        ),
        (
            "headlights-on",
            include_bytes!("../../assets/indicators/headlights-on.svg"),
        ),
        (
            "headlights-highbeam",
            include_bytes!("../../assets/indicators/headlights-highbeam.svg"),
        ),
    ];
    let icons = Icons(
        images
            .iter()
            .map(|(name, bytes)| {
                let color = egui_extras::image::load_svg_bytes_with_size(
                    bytes,
                    egui::SizeHint::Width(size),
                    &Default::default(),
                )
                .expect("valid embedded indicator SVG");
                ctx.load_texture(*name, color, egui::TextureOptions::LINEAR)
            })
            .collect(),
    );
    ctx.data_mut(|data| data.insert_temp(id, icons.clone()));
    icons
}

pub(super) fn rpm_fraction(frame: &DashboardFrame, config: &Config) -> Option<f32> {
    let sample = frame.sample.as_ref()?;
    // The demo reference uses an explicitly synthetic 13,300 RPM scale.
    let max = if sample.car == "DEMO" {
        13300
    } else {
        config.cars.get(&sample.car)?.max_rpm
    };
    (max > 0).then(|| (sample.rpm / max as f32).clamp(0.0, 1.0))
}

// Use UI time so constant RPM still animates.
fn rpm_fill_color(fraction: f32, threshold: f32, interval: f64, time: f64) -> Color32 {
    if fraction >= threshold && time.rem_euclid(2.0 * interval) >= interval {
        super::gt7_style::GAIN_TEXT
    } else {
        RED
    }
}

fn abs_icon(frame: &DashboardFrame) -> Option<usize> {
    let triggered = frame.sample.as_ref()?.lamp(outgauge::ABS)?;
    Some(if frame.abs_enabled? {
        if triggered { 2 } else { 1 }
    } else {
        0
    })
}

fn headlight_icon(frame: &DashboardFrame) -> Option<usize> {
    let sample = frame.sample.as_ref()?;
    if sample.lamp(outgauge::FULLBEAM) == Some(true) {
        Some(10)
    } else if let Some(switch) = frame.headlight_switch {
        Some(match switch {
            0 => 8,
            1 | 2 => 9,
            3 => 10,
            _ => return None,
        })
    } else {
        sample
            .lamp(outgauge::HEADLIGHTS)
            .map(|on| if on { 9 } else { 8 })
    }
}

pub(super) fn paint(painter: &egui::Painter, rect: Rect, frame: &DashboardFrame, config: &Config) {
    let scale = (rect.width() / SIZE.x).min(rect.height() / SIZE.y);
    let origin = rect.center() - SIZE * scale * 0.5;
    let point = |x, y| origin + Vec2::new(x, y) * scale;
    let panel = |x, y, w, h| Rect::from_min_size(point(x, y), Vec2::new(w, h) * scale);
    let display = FontFamily::Name(super::fonts::DISPLAY.into());
    let body = FontFamily::Name(super::fonts::BODY.into());
    let numeric = FontFamily::Name(super::fonts::NUMERIC.into());
    painter.rect_filled(panel(0.0, 48.0, 574.0, 162.0), 0.0, BACKGROUND);
    painter.rect_filled(panel(121.0, 0.0, 332.0, 48.0), 0.0, BACKGROUND);
    painter.text(
        point(130.0, 26.0),
        Align2::LEFT_CENTER,
        "RPM x1000",
        FontId::new(20.0 * scale, display.clone()),
        WHITE,
    );
    let rpm = frame.sample.as_ref().map_or("—".into(), |s| {
        let value = format!("{:.0}", s.rpm);
        if value.len() > 3 {
            format!(
                "{}.{}",
                &value[..value.len() - 3],
                &value[value.len() - 3..]
            )
        } else {
            value
        }
    });
    painter.text(
        point(441.0, 26.0),
        Align2::RIGHT_CENTER,
        rpm,
        FontId::new(26.0 * scale, numeric.clone()),
        WHITE,
    );
    let fraction = rpm_fraction(frame, config);
    let time = painter.ctx().input(|input| input.time);
    let threshold = f32::from(config.rpm_blink_threshold_percent) / 100.0;
    let interval = f64::from(config.rpm_blink_interval_ms) / 1000.0;
    let fill_color = fraction.map(|value| rpm_fill_color(value, threshold, interval, time));
    if fraction.is_some_and(|value| value >= threshold) {
        painter
            .ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(
                interval - time.rem_euclid(interval),
            ));
    }
    painter.rect_filled(
        panel(0.0, 48.0, SIZE.x, 23.0),
        0.0,
        if fraction.is_some() {
            Color32::from_rgb(215, 215, 215)
        } else {
            Color32::from_rgb(80, 80, 80)
        },
    );
    if let Some(value) = fraction {
        painter.rect_filled(
            panel(0.0, 48.0, SIZE.x * value, 23.0),
            0.0,
            fill_color.unwrap(),
        );
    }
    // Nine fixed markers divide the full scale into ten equal percentage intervals.
    for index in 1..10 {
        let x = SIZE.x * index as f32 / 10.0;
        painter.line_segment(
            [point(x, 48.0), point(x, 71.0)],
            egui::Stroke::new(scale, RPM_MARKER),
        );
    }
    let speed = frame
        .sample
        .as_ref()
        .map_or("—".into(), |s| format!("{:.0}", s.speed()));
    let gear = frame.sample.as_ref().map_or("—".into(), |s| s.gear_label());
    let units = frame
        .sample
        .as_ref()
        .map_or("—", |s| if s.kmh { "km/h" } else { "mph" });
    painter.text(
        point(111.0, 126.0),
        Align2::CENTER_CENTER,
        speed,
        FontId::new(54.0 * scale, numeric),
        WHITE,
    );
    painter.text(
        point(147.0, 179.0),
        Align2::CENTER_CENTER,
        units,
        FontId::new(23.0 * scale, body),
        WHITE,
    );
    let shift = frame.sample.as_ref().and_then(|s| s.lamp(outgauge::SHIFT)) == Some(true);
    painter.text(
        point(288.0, 133.0),
        Align2::CENTER_CENTER,
        gear,
        FontId::new(82.0 * scale, display),
        if shift { RED } else { YELLOW },
    );
    let icons = icons(
        painter.ctx(),
        47.0 * scale * painter.ctx().pixels_per_point(),
    );
    let sample = frame.sample.as_ref();
    let lamp = |mask| sample.and_then(|s| s.lamp(mask));
    let engine = match lamp(outgauge::ENGINE) {
        Some(true) if sample.is_some_and(|s| s.lights & outgauge::ENGINE_SEVERE != 0) => Some(7),
        Some(true) => Some(6),
        Some(false) => Some(5),
        None => None,
    };
    let states = [
        (476.0, 109.0, abs_icon(frame)),
        (
            535.0,
            109.0,
            lamp(outgauge::TC).map(|on| if on { 4 } else { 3 }),
        ),
        (476.0, 169.0, engine),
        (535.0, 169.0, headlight_icon(frame)),
    ];
    for (x, y, index) in states {
        let center = point(x, y);
        if let Some(index) = index {
            painter.image(
                icons.0[index].id(),
                Rect::from_center_size(center, Vec2::splat(47.0 * scale)),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.circle_filled(center, 23.5 * scale, Color32::from_rgb(44, 44, 44));
            painter.text(
                center,
                Align2::CENTER_CENTER,
                "—",
                FontId::proportional(20.0 * scale),
                Color32::GRAY,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn svg_icons_render_at_display_resolution_and_reuse_cached_textures() {
        let ctx = egui::Context::default();
        for (physical_size, resolution) in [(23.5, 32), (47.0, 64), (188.0, 256)] {
            let loaded = icons(&ctx, physical_size);
            assert_eq!(loaded.0.len(), 11);
            for texture in &loaded.0 {
                assert_eq!(texture.size(), [resolution, resolution]);
            }
            let cached = icons(&ctx, physical_size);
            assert_eq!(loaded.0[0].id(), cached.0[0].id());
        }
    }

    #[test]
    fn indicators_distinguish_abs_setup_and_full_beam_priority() {
        let mut frame = DashboardFrame {
            sample: Some(outgauge::Sample {
                available: outgauge::ABS | outgauge::HEADLIGHTS,
                ..Default::default()
            }),
            abs_enabled: Some(true),
            ..Default::default()
        };
        assert_eq!(abs_icon(&frame), Some(1)); // Enabled, not intervening.
        frame.sample.as_mut().unwrap().lights = outgauge::ABS;
        assert_eq!(abs_icon(&frame), Some(2)); // Triggered.
        frame.abs_enabled = Some(false);
        assert_eq!(abs_icon(&frame), Some(0)); // Disabled, even with warning lamp on.
        frame.abs_enabled = None;
        assert_eq!(abs_icon(&frame), None);
        assert_eq!(headlight_icon(&frame), Some(8));
        frame.sample.as_mut().unwrap().lights = 1 << 14;
        assert_eq!(headlight_icon(&frame), Some(9));
        frame.sample.as_mut().unwrap().lights |= outgauge::FULLBEAM;
        assert_eq!(headlight_icon(&frame), Some(10));
        frame.sample.as_mut().unwrap().lights = outgauge::FULLBEAM;
        assert_eq!(headlight_icon(&frame), Some(10));
        frame.sample.as_mut().unwrap().available = 0;
        assert_eq!(headlight_icon(&frame), None);
        assert_eq!(abs_icon(&frame), None);
        assert_eq!(headlight_icon(&DashboardFrame::default()), None);
        frame.sample = None;
        assert_eq!(abs_icon(&frame), None);
    }

    #[test]
    fn local_low_beam_works_without_a_dashboard_symbol_and_flash_still_wins() {
        let mut frame = DashboardFrame {
            sample: Some(outgauge::Sample {
                // Actual FXO packet: full beam exists, dipped/side symbols do not.
                available: 0x10008f66,
                lights: 4,
                ..Default::default()
            }),
            headlight_switch: Some(2),
            ..Default::default()
        };
        assert_eq!(headlight_icon(&frame), Some(9));
        frame.sample.as_mut().unwrap().lights |= outgauge::FULLBEAM;
        assert_eq!(headlight_icon(&frame), Some(10));
        frame.sample.as_mut().unwrap().lights = 4;
        assert_eq!(headlight_icon(&frame), Some(9));
        for (switch, icon) in [(0, 8), (1, 9), (3, 10)] {
            frame.headlight_switch = Some(switch);
            assert_eq!(headlight_icon(&frame), Some(icon));
        }
        frame.sample = None;
        assert_eq!(headlight_icon(&frame), None);
    }

    #[test]
    fn rpm_bar_respects_the_selected_threshold_and_blink_interval() {
        let blue = super::super::gt7_style::GAIN_TEXT;
        for interval in [0.05, 0.1, 0.5] {
            assert_eq!(rpm_fill_color(0.949, 0.95, interval, interval), RED);
            assert_eq!(rpm_fill_color(0.95, 0.95, interval, 0.0), RED);
            assert_eq!(rpm_fill_color(0.95, 0.95, interval, interval - 0.001), RED);
            assert_eq!(rpm_fill_color(0.95, 0.95, interval, interval), blue);
            assert_eq!(
                rpm_fill_color(1.0, 0.95, interval, 2.0 * interval - 0.001),
                blue
            );
            assert_eq!(rpm_fill_color(1.0, 0.95, interval, 2.0 * interval), RED);
            assert_eq!(rpm_fill_color(0.849, 0.85, interval, interval), RED);
            assert_eq!(rpm_fill_color(0.85, 0.85, interval, interval), blue);
        }
    }

    #[test]
    fn rpm_bar_uses_car_limit_and_clamps_without_guessing() {
        let mut config = Config::default();
        config.cars.clear();
        let frame = DashboardFrame {
            sample: Some(outgauge::Sample {
                car: "XFG".into(),
                rpm: 6400.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(rpm_fraction(&frame, &config), None);
        config.cars.insert(
            "XFG".into(),
            crate::config::CarProfile {
                name: "XF GTI".into(),
                max_rpm: 8000,
            },
        );
        assert_eq!(rpm_fraction(&frame, &config), Some(0.8));
        config.cars.get_mut("XFG").unwrap().max_rpm = 6000;
        assert_eq!(rpm_fraction(&frame, &config), Some(1.0));
        assert_eq!(rpm_fraction(&DashboardFrame::default(), &config), None);
    }
}
