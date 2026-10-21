//! Telemetry formatting and gadget painting, independent of native window state.

use super::{
    delta_style::DeltaStyle,
    gadget_style::{CellStyle, PanelStyle, RowRule, TextFont, TextStyle},
    gap_style::GapStyle,
    radar_style::{ProximityStyle, RadarStyle, WarningStyle},
    theme,
};
use crate::{
    config::{Config, GapSettings},
    gaps::GapValue,
    radar::{RadarFrame, Threat},
};
use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct RadarAutoHide {
    clear_since: Option<Instant>,
}

impl RadarAutoHide {
    fn opacity(&mut self, now: Instant, enabled: bool, warning: bool) -> f32 {
        if !enabled || warning {
            self.clear_since = None;
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(*self.clear_since.get_or_insert(now));
        let progress =
            (elapsed.saturating_sub(Duration::from_secs(2)).as_secs_f32() / 0.3).clamp(0.0, 1.0);
        (1.0 - progress).powi(3)
    }
}

fn radar_auto_hide_id(ctx: &egui::Context) -> egui::Id {
    egui::Id::new(("radar-auto-hide", ctx.viewport_id()))
}

pub(super) struct GapPaintOptions {
    pub scale: f32,
    pub debug: bool,
}

impl GapPaintOptions {
    pub fn new(settings: &GapSettings, debug: bool) -> Self {
        Self {
            scale: settings.scale,
            debug,
        }
    }
}

fn gap_text(value: &GapValue) -> String {
    if let Some(laps) = value.laps {
        format!("{laps} lap{}", if laps == 1 { "" } else { "s" })
    } else if let Some(seconds) = value.seconds {
        format!("~{seconds:.1} s")
    } else {
        "—".into()
    }
}
fn delta_text(value: &crate::delta::DeltaFrame) -> String {
    value
        .seconds
        .map(|s| format!("{:+.2} s", if s.abs() < 0.005 { 0.0 } else { s }))
        .unwrap_or_else(|| "— s".into())
}
fn delta_color(value: &crate::delta::DeltaFrame, style: &DeltaStyle) -> Color32 {
    match value.seconds {
        Some(s) if s < -0.005 => style.gaining_color,
        Some(s) if s > 0.005 => style.losing_color,
        _ => style.heading.color,
    }
}
fn delta_trend(value: &crate::delta::DeltaFrame) -> &'static str {
    match value.trend {
        Some(t) if t < -0.02 => "GAINING",
        Some(t) if t > 0.02 => "LOSING",
        Some(_) => "STEADY",
        None => "",
    }
}
pub(super) fn delta_value(value: &crate::delta::DeltaFrame) -> GapValue {
    GapValue {
        driver: Some(
            value
                .best_seconds
                .map(|s| format!("Session best {}", lap_time_text(s)))
                .unwrap_or_else(|| "No reference lap yet".into()),
        ),
        status: format!("{} {}", delta_trend(value), value.status),
        ..Default::default()
    }
}
pub(super) fn lap_time_text(seconds: f64) -> String {
    let hundredths = (seconds * 100.0).round() as u64;
    format!(
        "{}:{:02}.{:02}",
        hundredths / 6000,
        hundredths / 100 % 60,
        hundredths % 100
    )
}
fn lap_time_millis(seconds: f64) -> String {
    let millis = (seconds.max(0.0) * 1000.0).round() as u64;
    format!(
        "{}:{:02}.{:03}",
        millis / 60000,
        millis / 1000 % 60,
        millis % 1000
    )
}

fn signed_millis(seconds: Option<f64>) -> String {
    seconds
        .map(|s| format!("{:+.3}", if s.abs() < 0.0005 { 0.0 } else { s }))
        .unwrap_or_else(|| "—.---".into())
}
pub(super) fn estimated_lap_text(value: &crate::delta::DeltaFrame) -> String {
    value
        .estimated_lap_seconds
        .map(|s| format!("Estimated lap {}", lap_time_text(s)))
        .unwrap_or_else(|| "Estimated lap —".into())
}
pub(super) fn paint_delta(
    painter: &egui::Painter,
    canvas: Rect,
    value: &crate::delta::DeltaFrame,
    settings: &GapSettings,
    style: &DeltaStyle,
) {
    let scale = settings
        .scale
        .min(canvas.width() / style.panel.size.x)
        .min(canvas.height() / style.panel.size.y);
    let rect = Rect::from_center_size(canvas.center(), style.panel.size * scale);
    paint_gadget_panel(painter, rect, scale, &style.panel);
    if let Some(strip) = &style.reference_time_strip {
        let time = paint_cell(painter, rect, scale, &strip.time_cell);
        paint_cell_text(
            painter,
            rect,
            time,
            scale,
            &style.reference,
            value
                .best_seconds
                .map(lap_time_millis)
                .unwrap_or_else(|| "-:--.---".into()),
        );
        paint_gadget_text(
            painter,
            rect,
            scale,
            &strip.heading,
            "SESSION BEST / LIVE DELTA",
        );
    }
    let value_cell = style.value_cell.as_ref().map(|cell| {
        let (background, foreground) = match value.seconds {
            Some(s) if s < -0.005 => (cell.gaining, cell.gaining_text),
            Some(s) if s > 0.005 => (cell.losing, cell.losing_text),
            _ => (cell.neutral, cell.neutral_text),
        };
        let cell_rect = Rect::from_min_size(rect.min + cell.offset * scale, cell.size * scale);
        painter.rect_filled(cell_rect, cell.corner_radius * scale, background);
        (cell_rect, foreground)
    });
    if let Some(label) = &style.label {
        paint_gadget_text(painter, rect, scale, label, "DELTA");
    }
    let heading = TextStyle {
        color: value_cell.map_or_else(|| delta_color(value, style), |(_, foreground)| foreground),
        ..style.heading
    };
    let text = if style.reference_time_strip.is_some() {
        signed_millis(value.seconds)
    } else if style.label.is_some() {
        delta_text(value)
    } else {
        format!("DELTA  {}", delta_text(value))
    };
    if let Some((cell, _)) = value_cell {
        paint_cell_text(painter, rect, cell, scale, &heading, text);
    } else {
        paint_gadget_text(painter, rect, scale, &heading, text);
    }
    if let Some(rule) = &style.rule {
        paint_row_rule(painter, rect, scale, rule);
    }
    if style.reference_time_strip.is_none() {
        paint_gadget_text(
            painter,
            rect,
            scale,
            &style.reference,
            delta_value(value).driver.unwrap_or_default(),
        );
    }
    if let Some(label) = &style.estimate_label {
        paint_gadget_text(painter, rect, scale, label, "ESTIMATED LAP");
    }
    paint_gadget_text(
        painter,
        rect,
        scale,
        &style.estimate,
        if style.estimate_label.is_some() {
            value
                .estimated_lap_seconds
                .map(lap_time_millis)
                .unwrap_or_else(|| "-:--.---".into())
        } else {
            estimated_lap_text(value)
        },
    );
    if let Some(status) = &style.status {
        paint_gadget_text(
            painter,
            rect,
            scale,
            status,
            if value.seconds.is_some() {
                format!("{} · ESTIMATE", delta_trend(value))
            } else {
                clipped_text(&value.status, 35)
            },
        );
    }
    if let (Some(trend), Some(appearance)) = (value.trend, &style.trend) {
        let center = rect.right() - appearance.right_offset * scale;
        let y = rect.top() + appearance.top_offset * scale;
        let half_length = appearance.half_length * scale;
        painter.line_segment(
            [
                Pos2::new(center - half_length, y),
                Pos2::new(center + half_length, y),
            ],
            Stroke::new(appearance.track_width * scale, appearance.track_color),
        );
        painter.line_segment(
            [
                Pos2::new(center, y),
                Pos2::new(center + trend.clamp(-1.0, 1.0) as f32 * half_length, y),
            ],
            Stroke::new(
                appearance.bar_width * scale,
                if trend < 0.0 {
                    appearance.gaining_color
                } else {
                    appearance.losing_color
                },
            ),
        );
    }
}
fn gap_driver(value: &GapValue) -> String {
    match (&value.driver, value.position) {
        (Some(name), Some(position)) => format!("P{position} · {name}"),
        _ => "—".into(),
    }
}

fn paint_gadget_panel(painter: &egui::Painter, rect: Rect, scale: f32, style: &PanelStyle) {
    let radius = (style.corner_radius * scale).clamp(0.0, rect.width().min(rect.height()) * 0.5);
    painter.rect_filled(rect, radius, style.background);
    let border = (style.border_width * scale).clamp(0.0, rect.width().min(rect.height()) * 0.5);
    if border > 0.0 {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(border, style.border_color),
            egui::StrokeKind::Inside,
        );
    }
}

fn paint_cell(painter: &egui::Painter, panel: Rect, scale: f32, cell: &CellStyle) -> Rect {
    let rect = Rect::from_min_size(panel.min + cell.offset * scale, cell.panel.size * scale);
    paint_gadget_panel(painter, rect, scale, &cell.panel);
    rect
}

fn paint_cell_text(
    painter: &egui::Painter,
    panel: Rect,
    cell: Rect,
    scale: f32,
    style: &TextStyle,
    text: impl ToString,
) {
    let local = TextStyle {
        offset: style.offset - (cell.min - panel.min) / scale,
        ..*style
    };
    paint_gadget_text(painter, cell, scale, &local, text);
}

fn paint_gadget_text(
    painter: &egui::Painter,
    rect: Rect,
    scale: f32,
    style: &TextStyle,
    text: impl ToString,
) {
    let position = rect.min + style.offset * scale;
    let family = match style.font {
        TextFont::Body => FontFamily::Name(super::fonts::BODY.into()),
        TextFont::Numeric => FontFamily::Name(super::fonts::NUMERIC.into()),
    };
    let font = FontId::new(style.font_size * scale, family);
    let mut text = text.to_string();
    // Use measured glyph width for names/statuses, including wide and Unicode text.
    // Zero-sized rectangles are intentional anchors for the radar's centered labels.
    if rect.width() > 0.0 {
        let width = match style.align.x() {
            egui::Align::Min => rect.right() - position.x,
            egui::Align::Max => position.x - rect.left(),
            egui::Align::Center => 2.0 * (position.x - rect.left()).min(rect.right() - position.x),
        } - 8.0 * scale;
        if painter
            .layout_no_wrap(text.clone(), font.clone(), style.color)
            .size()
            .x
            > width
        {
            let mut shortened = text.clone();
            loop {
                shortened.pop();
                text = format!("{shortened}…");
                if shortened.is_empty()
                    || painter
                        .layout_no_wrap(text.clone(), font.clone(), style.color)
                        .size()
                        .x
                        <= width
                {
                    break;
                }
            }
        }
    }
    let galley = painter.layout_no_wrap(text, font, style.color);
    let origin = style.align.anchor_size(position, galley.size()).min;
    if style.outline.width > 0.0 {
        for direction in [
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, -1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, 1.0),
        ] {
            painter.galley_with_override_text_color(
                origin + direction * style.outline.width * scale,
                galley.clone(),
                style.outline.color,
            );
        }
    }
    painter.galley(origin, galley, style.color);
}

fn paint_row_rule(painter: &egui::Painter, rect: Rect, scale: f32, style: &RowRule) {
    let y = rect.top() + style.top_offset * scale;
    painter.line_segment(
        [
            Pos2::new(rect.left() + style.inset * scale, y),
            Pos2::new(rect.right() - style.inset * scale, y),
        ],
        Stroke::new(style.width * scale, style.color),
    );
}

pub(super) fn paint_gap(
    painter: &egui::Painter,
    canvas: Rect,
    title: &str,
    value: &GapValue,
    options: &GapPaintOptions,
    style: &GapStyle,
) {
    let scale = options
        .scale
        .min(canvas.width() / style.panel.size.x)
        .min(canvas.height() / style.panel.size.y);
    let size = style.panel.size * scale;
    let rect = Rect::from_center_size(canvas.center(), size);
    paint_gadget_panel(painter, rect, scale, &style.panel);
    if let Some(row) = &style.driver_row {
        let position = paint_cell(painter, rect, scale, &row.position_cell);
        let driver = paint_cell(painter, rect, scale, &row.driver_cell);
        let gap = paint_cell(painter, rect, scale, &row.gap_cell);
        let mut fade = egui::Mesh::default();
        for (point, color) in [
            (gap.left_top(), row.gap_fade_from),
            (gap.right_top(), row.gap_fade_to),
            (gap.right_bottom(), row.gap_fade_to),
            (gap.left_bottom(), row.gap_fade_from),
        ] {
            fade.colored_vertex(point, color);
        }
        fade.add_triangle(0, 1, 2);
        fade.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(fade));
        paint_gadget_text(painter, rect, scale, &style.heading, title);
        paint_cell_text(
            painter,
            rect,
            position,
            scale,
            &row.position,
            value
                .position
                .map(|p| p.to_string())
                .unwrap_or_else(|| "—".into()),
        );
        paint_cell_text(
            painter,
            rect,
            driver,
            scale,
            &style.driver,
            value.driver.as_deref().unwrap_or("—"),
        );
        if let Some(text) = &style.value {
            // Signed intervals follow the leaderboard reference: ahead +, behind −.
            let seconds = value
                .seconds
                .map(|s| if title == "BEHIND" { -s.abs() } else { s.abs() });
            let text_value = if value.laps.is_some() {
                gap_text(value)
            } else {
                signed_millis(seconds)
            };
            paint_cell_text(painter, rect, gap, scale, text, text_value);
        }
        if options.debug {
            let status = value
                .measured_age_ms
                .map(|age| format!("ESTIMATE · {:.1} s ago", age as f64 / 1000.0))
                .unwrap_or_else(|| value.status.clone());
            paint_gadget_text(painter, rect, scale, &style.status, status);
        }
        return;
    }
    paint_gadget_text(
        painter,
        rect,
        scale,
        &style.heading,
        if style.value.is_some() {
            title.to_owned()
        } else {
            format!("{title}  {}", gap_text(value))
        },
    );
    if let Some(value_style) = &style.value {
        paint_gadget_text(painter, rect, scale, value_style, gap_text(value));
    }
    if let Some(rule) = &style.rule {
        paint_row_rule(painter, rect, scale, rule);
    }
    paint_gadget_text(
        painter,
        rect,
        scale,
        &style.driver,
        clipped_text(&gap_driver(value), 28),
    );
    if options.debug {
        let status = value
            .measured_age_ms
            .map(|age| format!("ESTIMATE · {:.1} s ago", age as f64 / 1000.0))
            .unwrap_or_else(|| clipped_text(&value.status, 32));
        paint_gadget_text(painter, rect, scale, &style.status, status);
    }
}
fn clipped_text(text: &str, limit: usize) -> String {
    if text.chars().count() > limit {
        format!("{}…", text.chars().take(limit - 1).collect::<String>())
    } else {
        text.into()
    }
}

pub(super) fn paint(
    painter: &egui::Painter,
    rect: Rect,
    frame: &RadarFrame,
    config: &Config,
    draw_background: bool,
) {
    paint_at(
        painter,
        rect,
        frame,
        config,
        draw_background,
        Instant::now(),
    );
}

fn paint_at(
    painter: &egui::Painter,
    rect: Rect,
    frame: &RadarFrame,
    config: &Config,
    draw_background: bool,
    now: Instant,
) {
    let style = theme::resolve(config.hud_style).radar;
    let size = rect.width().min(rect.height());
    let panel = Rect::from_center_size(rect.center(), Vec2::splat(size - 8.0));
    let ctx = painter.ctx();
    let auto_hide_id = radar_auto_hide_id(ctx);
    let opacity = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<RadarAutoHide>(auto_hide_id)
            .opacity(
                now,
                config.radar_auto_hide && !draw_background,
                frame.live && !frame.cars.is_empty(),
            )
    });
    if config.radar_auto_hide && !draw_background {
        // Keep observing telemetry even when the radar is fully transparent.
        ctx.request_repaint_after(Duration::from_millis(16));
    }
    if opacity == 0.0 {
        return;
    }
    let mut faded = painter.with_clip_rect(panel.intersect(painter.clip_rect()));
    faded.multiply_opacity(opacity);
    let painter = &faded;
    if draw_background || style.proximity.is_some() {
        paint_gadget_panel(painter, panel, 1.0, &style.panel);
    }
    let projection = radar_projection(panel, config, style);
    let RadarProjection {
        origin: centre,
        ui_scale,
        ..
    } = projection;
    if let Some(proximity) = &style.proximity {
        paint_proximity_warnings(painter, panel, proximity, frame, &projection);
        paint_proximity_guides(painter, panel, style, proximity, ui_scale);
    }
    if frame.live {
        let marker_size = projection.marker_size;
        let marker = |position, color| {
            painter.rect(
                Rect::from_center_size(position, marker_size),
                1.5 * ui_scale,
                color,
                Stroke::new(1.5 * ui_scale, Color32::BLACK),
                egui::StrokeKind::Inside,
            );
        };
        for car in &frame.cars {
            marker(projection.position(car.right, car.forward), style.opponent);
        }
        // Keep the player visible when markers overlap.
        marker(centre, style.player);
    } else {
        paint_gadget_text(
            painter,
            Rect::from_min_size(centre, Vec2::ZERO),
            ui_scale,
            &style.paused,
            "RADAR PAUSED",
        );
    }
    if !frame.live {
        paint_gadget_text(
            painter,
            Rect::from_min_size(Pos2::new(centre.x, panel.bottom()), Vec2::ZERO),
            ui_scale,
            &style.hint,
            "MCI + OutSim required",
        );
    }
}

fn warning_appearance(
    car: &crate::radar::RadarCar,
    frame_uncertain: bool,
    style: &WarningStyle,
) -> (u8, Color32) {
    if frame_uncertain || car.uncertain {
        (0, style.uncertain)
    } else {
        match car.threat {
            Threat::Nearby => (1, style.nearby),
            Threat::Alongside => (2, style.alongside),
            Threat::PotentialContact => (3, style.contact),
        }
    }
}

fn paint_proximity_warnings(
    painter: &egui::Painter,
    panel: Rect,
    proximity: &ProximityStyle,
    frame: &RadarFrame,
    projection: &RadarProjection,
) {
    if !frame.live {
        return;
    }
    let style = &proximity.warnings;
    let center = panel_point(panel, proximity.guide_center);
    let radius = panel.width().min(panel.height()) * proximity.guide_radius;
    // Draw stronger warnings last so a nearby car cannot obscure contact.
    for priority in 0..=3 {
        for car in frame
            .cars
            .iter()
            .filter(|car| warning_appearance(car, frame.uncertain, style).0 == priority)
        {
            let (_, color) = warning_appearance(car, frame.uncertain, style);
            // The diagonal guides divide the sensor into four 90-degree sectors.
            let offset = projection.position(car.right, car.forward) - center;
            let quarter_turn = std::f32::consts::FRAC_PI_2;
            let direction = (offset.y.atan2(offset.x) / quarter_turn).round() * quarter_turn;
            const ANGLE_STEPS: u32 = 24;
            const RADIAL_STEPS: u32 = 8;
            let mut mesh = egui::Mesh::default();
            for angle_step in 0..=ANGLE_STEPS {
                let angle = direction - std::f32::consts::FRAC_PI_4
                    + quarter_turn * angle_step as f32 / ANGLE_STEPS as f32;
                let axis = Vec2::angled(angle);
                for radial_step in 0..=RADIAL_STEPS {
                    let distance = radial_step as f32 / RADIAL_STEPS as f32;
                    let fade = ((1.0 - distance) / style.edge_fade).clamp(0.0, 1.0);
                    mesh.colored_vertex(
                        center + axis * radius * distance,
                        color.gamma_multiply(style.fill_opacity * fade),
                    );
                }
            }
            for angle_step in 0..ANGLE_STEPS {
                for radial_step in 0..RADIAL_STEPS {
                    let index = angle_step * (RADIAL_STEPS + 1) + radial_step;
                    let next = index + RADIAL_STEPS + 1;
                    mesh.add_triangle(index, index + 1, next);
                    mesh.add_triangle(index + 1, next + 1, next);
                }
            }
            painter.add(Shape::mesh(mesh));
        }
    }
}

fn panel_point(panel: Rect, fraction: Vec2) -> Pos2 {
    panel.min + Vec2::new(fraction.x * panel.width(), fraction.y * panel.height())
}

#[derive(Clone, Copy)]
struct RadarProjection {
    origin: Pos2,
    /// Shared distance scale for positions and hitboxes on both axes.
    metres_to_pixels: f32,
    /// Hitbox dimensions using a uniform scale set by the lateral range.
    marker_size: Vec2,
    ui_scale: f32,
}

impl RadarProjection {
    fn position(&self, right: f64, forward: f64) -> Pos2 {
        self.origin + Vec2::new(right as f32, -forward as f32) * self.metres_to_pixels
    }
}

fn radar_projection(panel: Rect, config: &Config, style: &RadarStyle) -> RadarProjection {
    let proximity = style.proximity.as_ref().expect("GT radar geometry");
    let ui_scale = panel.width().min(panel.height()) / style.panel.size.x;
    let origin = panel_point(panel, proximity.origin);
    // Match the engine's conservative range padding, including rotated footprints.
    let physical_padding = config.car_length_m.hypot(config.car_width_m) as f32 * 0.5;
    // Fit both the admitted car centre and its marker within the side boundary.
    // The marker's padding scales with the same pixels per metre as its position.
    let side = panel.width() * (0.5 - proximity.side_inset)
        / (config.side_m as f32 + 2.0 * physical_padding);
    RadarProjection {
        origin,
        metres_to_pixels: side,
        marker_size: Vec2::new(config.car_width_m as f32, config.car_length_m as f32) * side,
        ui_scale,
    }
}

fn paint_proximity_guides(
    painter: &egui::Painter,
    panel: Rect,
    style: &RadarStyle,
    proximity: &ProximityStyle,
    ui_scale: f32,
) {
    let center = panel_point(panel, proximity.origin);
    let radius = panel.width().min(panel.height()) * proximity.guide_radius;
    // Diagonal guides frame the front, rear and side warning sectors.
    for axis in [Vec2::new(1.0, 1.0), Vec2::new(1.0, -1.0)] {
        let axis = axis.normalized();
        let mut mesh = egui::Mesh::default();
        let half_width = axis.rot90() * style.grid_width * ui_scale * 0.5;
        for (distance, opacity) in [(-0.78, 0.0), (-0.4, 1.0), (0.4, 1.0), (0.78, 0.0)] {
            let point = center + axis * radius * distance;
            let color = style.grid_color.gamma_multiply(opacity);
            mesh.colored_vertex(point - half_width, color);
            mesh.colored_vertex(point + half_width, color);
        }
        for index in [0, 2, 4] {
            mesh.add_triangle(index, index + 1, index + 2);
            mesh.add_triangle(index + 1, index + 3, index + 2);
        }
        painter.add(Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HudStyle;

    #[test]
    fn auto_hide_waits_two_seconds_eases_out_and_shows_immediately() {
        let now = Instant::now();
        let mut state = RadarAutoHide::default();
        for (millis, enabled, warning, expected) in [
            (0, true, false, 1.0),
            (1000, true, false, 1.0),
            (1999, true, false, 1.0),
            (2000, true, false, 1.0),
            (2150, true, false, 0.125),
            (2300, true, false, 0.0),
            (8000, true, true, 1.0),
            (9000, true, false, 1.0),
            (10000, true, false, 1.0),
            (12000, true, false, 0.0),
            (12000, false, false, 1.0),
            (13000, true, false, 1.0),
            (14000, true, false, 1.0),
        ] {
            let opacity = state.opacity(now + Duration::from_millis(millis), enabled, warning);
            assert!(
                (opacity - expected).abs() < 0.001,
                "at {millis} ms: {opacity}"
            );
        }
    }

    #[test]
    fn auto_hide_fades_the_entire_radar_and_all_warning_types_restore_it() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let config = Config {
            radar_auto_hide: true,
            ..Default::default()
        };
        let quiet = RadarFrame {
            live: true,
            ..Default::default()
        };
        let now = Instant::now();
        let seed = |age| {
            let key = radar_auto_hide_id(&ctx);
            ctx.data_mut(|data| {
                data.insert_temp(
                    key,
                    RadarAutoHide {
                        clear_since: Some(now - age),
                    },
                )
            });
        };
        let draw = |frame: &RadarFrame, preview| {
            ctx.run(egui::RawInput::default(), |ctx| {
                paint_at(
                    &ctx.layer_painter(egui::LayerId::background()),
                    Rect::from_min_size(Pos2::ZERO, Vec2::splat(320.0)),
                    frame,
                    &config,
                    preview,
                    now,
                );
            })
        };
        seed(Duration::from_secs(3));
        assert!(draw(&quiet, false).shapes.is_empty());
        seed(Duration::from_millis(2150));
        let fading = draw(&quiet, false);
        assert!(fading.shapes.iter().any(|s| matches!(&s.shape,
            Shape::Rect(rect) if rect.fill.a() > 0 && rect.fill.a() < 100)));
        assert!(fading.shapes.iter().all(|s| match &s.shape {
            Shape::Rect(rect) => rect.fill.a() < 100 && rect.stroke.color.a() < 100,
            Shape::Mesh(mesh) => mesh.vertices.iter().all(|v| v.color.a() < 100),
            Shape::Noop => true,
            _ => false,
        }));
        for threat in [Threat::Nearby, Threat::Alongside, Threat::PotentialContact] {
            for uncertain in [false, true] {
                seed(Duration::from_secs(3));
                let frame = RadarFrame {
                    live: true,
                    cars: vec![crate::radar::RadarCar {
                        plid: 2,
                        name: "Opponent".into(),
                        right: 2.0,
                        forward: 0.0,
                        relative_heading: 0.0,
                        threat,
                        uncertain,
                    }],
                    ..Default::default()
                };
                let output = draw(&frame, false);
                assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                    Shape::Rect(rect) if rect.fill == theme::resolve(config.hud_style).radar.player)));
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|s| matches!(&s.shape, Shape::Mesh(_)))
                );
            }
        }
        seed(Duration::from_secs(3));
        assert!(
            !draw(&quiet, true).shapes.is_empty(),
            "preview and position mode stay visible"
        );
        seed(Duration::from_secs(3));
        assert!(draw(&RadarFrame::default(), false).shapes.is_empty());
    }

    #[test]
    fn native_auto_hide_preserves_editing_and_manual_visibility_overrides() {
        use super::super::session::{Gadget, Monitor, OverlaySession};
        let ctx = egui::Context::default();
        let config = Config {
            radar_auto_hide: true,
            ..Default::default()
        };
        let mut session = OverlaySession::new(config.clone(), true);
        session.outputs_changed(vec![Monitor {
            name: "test".into(),
            size: [1920.0, 1080.0],
            scale: 1.0,
        }]);
        let key = radar_auto_hide_id(&ctx);
        ctx.data_mut(|data| {
            data.insert_temp(
                key,
                RadarAutoHide {
                    clear_since: Some(Instant::now() - Duration::from_secs(3)),
                },
            )
        });
        let mut snapshot = crate::runtime::Snapshot::default();
        snapshot.frame.live = true;
        assert!(session.paint(&ctx, &snapshot).shapes.is_empty());
        session.radar_editing = true;
        assert!(!session.paint(&ctx, &snapshot).shapes.is_empty());
        assert!(session.config_for_save().radar_auto_hide);

        snapshot.frame.cars.push(crate::radar::RadarCar {
            plid: 2,
            name: "Opponent".into(),
            right: 2.0,
            forward: 0.0,
            relative_heading: 0.0,
            threat: Threat::PotentialContact,
            uncertain: false,
        });
        session.update(config.clone(), false);
        assert!(
            session.paint(&ctx, &snapshot).shapes.is_empty(),
            "master hide takes priority"
        );
        session.update(config.clone(), true);
        session.hidden_gadgets.insert(Gadget::Radar);
        assert!(
            session.paint(&ctx, &snapshot).shapes.is_empty(),
            "command hide takes priority"
        );
        session.hidden_gadgets.clear();
        let mut disabled = config.clone();
        disabled.radar_enabled = false;
        session.update(disabled, true);
        assert!(
            session.paint(&ctx, &snapshot).shapes.is_empty(),
            "disabled radar stays hidden"
        );
        session.update(config, true);
        session.radar_editing = false;
        assert!(!session.paint(&ctx, &snapshot).shapes.is_empty());
    }

    #[test]
    fn proximity_wedges_follow_direction_threat_and_fade_at_the_circle() {
        let radar = theme::resolve(HudStyle::Gt7Inspired).radar;
        let proximity = radar.proximity.as_ref().unwrap();
        let style = &proximity.warnings;
        let ctx = egui::Context::default();
        let config = Config::default();
        for size in [220.0, 640.0] {
            let panel = Rect::from_min_size(Pos2::ZERO, Vec2::splat(size));
            let projection = radar_projection(panel, &config, radar);
            let center = panel_point(panel, proximity.guide_center);
            let radius = size * proximity.guide_radius;
            for (right, forward, sector) in [
                (-2.0, 0.0, -Vec2::X),
                (2.0, 0.0, Vec2::X),
                (0.0, 6.0, -Vec2::Y),
                (0.0, -6.0, Vec2::Y),
                (0.0, 0.1, -Vec2::Y),
                (0.0, -0.1, Vec2::Y),
                (6.0, 1.0, Vec2::X),
                (-6.0, -1.0, -Vec2::X),
            ] {
                for (threat, uncertain, frame_uncertain, color) in [
                    (Threat::Nearby, false, false, style.nearby),
                    (Threat::Alongside, false, false, style.alongside),
                    (Threat::PotentialContact, false, false, style.contact),
                    (Threat::PotentialContact, true, false, style.uncertain),
                    (Threat::PotentialContact, false, true, style.uncertain),
                ] {
                    let car = crate::radar::RadarCar {
                        plid: 1,
                        name: "Opponent".into(),
                        right,
                        forward,
                        relative_heading: 0.0,
                        threat,
                        uncertain,
                    };
                    let (priority, actual_color) = warning_appearance(&car, frame_uncertain, style);
                    assert_eq!(actual_color, color);
                    assert_eq!(priority == 0, uncertain || frame_uncertain);
                    let mut frame = RadarFrame {
                        live: true,
                        uncertain: frame_uncertain,
                        cars: vec![car],
                        ..Default::default()
                    };
                    let draw = |frame: &RadarFrame| {
                        ctx.run(egui::RawInput::default(), |ctx| {
                            paint_proximity_warnings(
                                &ctx.layer_painter(egui::LayerId::background()),
                                panel,
                                proximity,
                                frame,
                                &projection,
                            );
                        })
                    };
                    let output = draw(&frame);
                    let Shape::Mesh(mesh) = &output.shapes[0].shape else {
                        panic!("expected warning mesh");
                    };
                    assert!(mesh.is_valid());
                    assert!(
                        mesh.vertices.iter().all(|vertex| {
                            let offset = vertex.pos - center;
                            let along = offset.dot(sector);
                            let across = offset.dot(sector.rot90());
                            offset.length() <= radius + 0.001
                                && along >= -0.001
                                && across.abs() <= along + 0.001
                        }),
                        "warning must fit its diagonal-bounded sector: {right}, {forward}"
                    );
                    let outer: Vec<_> = mesh
                        .vertices
                        .iter()
                        .filter(|vertex| (vertex.pos.distance(center) - radius).abs() < 0.001)
                        .collect();
                    assert_eq!(outer.len(), 25);
                    let expected = center + sector * radius;
                    assert!(outer.iter().any(|v| v.pos.distance(expected) < 0.001));
                    assert!(mesh.vertices.iter().any(|v| v.color.a() == 0));
                    assert!(mesh.vertices.iter().any(|v| v.color.a() > 100));
                    frame.live = false;
                    assert!(draw(&frame).shapes.is_empty());
                    frame.live = true;
                    frame.cars.clear();
                    assert!(draw(&frame).shapes.is_empty());
                }
            }
        }
    }

    #[test]
    fn gt_gap_diagnostics_follow_debug_toggle() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        for hud in [HudStyle::Gt7Inspired] {
            let theme = theme::resolve(hud);
            for debug in [false, true] {
                for age in [None, Some(100)] {
                    let value = GapValue {
                        status: "Building passage history".into(),
                        measured_age_ms: age,
                        ..Default::default()
                    };
                    let output = ctx.run(egui::RawInput::default(), |ctx| {
                        paint_gap(
                            &ctx.layer_painter(egui::LayerId::background()),
                            Rect::from_min_size(Pos2::ZERO, theme.ahead.panel.size),
                            "AHEAD",
                            &value,
                            &GapPaintOptions { scale: 1.0, debug },
                            theme.ahead,
                        );
                    });
                    let diagnostic = output.shapes.iter().any(|shape| matches!(&shape.shape,
                        Shape::Text(text) if text.galley.job.text.contains("Building passage history") ||
                            text.galley.job.text.starts_with("ESTIMATE")));
                    assert_eq!(diagnostic, debug);
                }
            }
        }
    }

    #[test]
    fn empty_live_radar_has_only_diagonal_guides_and_a_white_player_marker() {
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            paint(
                &ctx.layer_painter(egui::LayerId::background()),
                Rect::from_min_size(Pos2::ZERO, Vec2::splat(320.0)),
                &RadarFrame {
                    live: true,
                    ..Default::default()
                },
                &Config::default(),
                true,
            );
        });
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, Shape::Text(_)))
        );
        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|s| matches!(&s.shape, Shape::Mesh(_)))
                .count(),
            2
        );
        let center = Pos2::new(160.0, 160.0);
        for mesh in output.shapes.iter().filter_map(|s| match &s.shape {
            Shape::Mesh(mesh) => Some(mesh),
            _ => None,
        }) {
            let start = mesh.vertices[0].pos.lerp(mesh.vertices[1].pos, 0.5);
            let offset = start - center;
            assert!((offset.x.abs() - offset.y.abs()).abs() < 0.001);
            assert!(offset.length() > 1.0);
        }
        assert!(output.shapes.iter().all(|s| match &s.shape {
            Shape::Mesh(mesh) => mesh.vertices.iter().all(|v| {
                v.color == Color32::TRANSPARENT
                    || v.color == theme::resolve(HudStyle::Gt7Inspired).radar.grid_color
            }),
            Shape::Rect(rect) =>
                rect.fill == Color32::TRANSPARENT
                    || rect.fill == theme::resolve(HudStyle::Gt7Inspired).radar.player,
            Shape::Noop => true,
            _ => false,
        }));
    }

    #[test]
    fn side_range_zooms_car_markers_and_positions_at_the_same_scale() {
        let style = theme::resolve(HudStyle::Gt7Inspired).radar;
        for size in [220.0, 320.0, 800.0] {
            let panel = Rect::from_min_size(Pos2::ZERO, Vec2::splat(size)).shrink(4.0);
            let mut previous = f32::INFINITY;
            for side in [0.0, 0.1, 3.0, 5.0, 12.0, 100.0] {
                let config = Config {
                    side_m: side,
                    ..Default::default()
                };
                let projection = radar_projection(panel, &config, style);
                assert!(projection.marker_size.is_finite());
                assert!(projection.marker_size.x > 0.0);
                assert!(projection.marker_size.x < previous);
                previous = projection.marker_size.x;
                let across = projection.position(config.car_width_m, 0.0);
                let ahead = projection.position(0.0, config.car_length_m);
                assert!(
                    (across.distance(projection.origin) - projection.marker_size.x).abs() < 0.001
                );
                assert!(
                    (ahead.distance(projection.origin) - projection.marker_size.y).abs() < 0.001
                );
                let larger = radar_projection(
                    Rect::from_min_size(Pos2::ZERO, panel.size() * 2.0),
                    &config,
                    style,
                );
                assert!((larger.marker_size - projection.marker_size * 2.0).length() < 0.001);
            }
        }
    }

    #[test]
    fn live_radar_shows_warnings_and_matching_contrasting_car_markers_at_all_sizes() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let style = theme::resolve(HudStyle::Gt7Inspired).radar;
        let luminance = |color| {
            let rgb = egui::Rgba::from(color);
            0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
        };
        // Contrast distinguishes the fills by brightness as well as hue.
        assert!((luminance(style.player) + 0.05) / (luminance(style.opponent) + 0.05) >= 3.0);
        assert!((luminance(style.opponent) + 0.05) / 0.05 >= 3.0);
        for (width, length) in [(1.8, 4.2), (2.0, 5.0), (4.0, 2.0)] {
            for size in [220.0, 320.0, 800.0] {
                for side in [0.0, 0.1, 3.0, 5.0, 12.0] {
                    let config = Config {
                        hud_style: HudStyle::Gt7Inspired,
                        side_m: side,
                        car_width_m: width,
                        car_length_m: length,
                        overlay_size: size,
                        ..Default::default()
                    };
                    let frame = RadarFrame {
                        live: true,
                        cars: [(2.0, 3.0), (-2.0, 3.0), (-2.0, -3.0), (2.0, -3.0)]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (right, forward))| crate::radar::RadarCar {
                                plid: index as u8 + 2,
                                name: "Opponent".into(),
                                right,
                                forward,
                                relative_heading: 0.0,
                                threat: Threat::Nearby,
                                uncertain: false,
                            })
                            .collect(),
                        ..Default::default()
                    };
                    let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(size));
                    let panel = canvas.shrink(4.0);
                    let projection = radar_projection(panel, &config, style);
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(canvas),
                            ..Default::default()
                        },
                        |ctx| {
                            paint(
                                &ctx.layer_painter(egui::LayerId::background()),
                                canvas,
                                &frame,
                                &config,
                                false,
                            );
                        },
                    );
                    let markers: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Rect(rect) if rect.fill != Color32::TRANSPARENT => Some(rect),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(markers.len(), frame.cars.len() + 1);
                    let player = markers.last().unwrap();
                    for (opponent, car) in markers.iter().zip(&frame.cars) {
                        assert_eq!(opponent.fill, style.opponent);
                        assert!((opponent.rect.size() - player.rect.size()).length() < 0.001);
                        assert_eq!(opponent.corner_radius, player.corner_radius);
                        assert_eq!(opponent.stroke, player.stroke);
                        assert_eq!(opponent.stroke.color, Color32::BLACK);
                        assert!(
                            opponent
                                .rect
                                .center()
                                .distance(projection.position(car.right, car.forward))
                                < 0.001
                        );
                        assert!(panel.contains_rect(opponent.rect));
                    }
                    assert_eq!(player.fill, style.player);
                    assert!(player.rect.center().distance(projection.origin) < 0.001);
                    assert!((player.rect.size() - projection.marker_size).length() < 0.001);
                    let ratio = player.rect.width() / player.rect.height();
                    assert!(
                        (ratio - (config.car_width_m / config.car_length_m) as f32).abs() < 0.001
                    );
                    assert!(
                        (player.rect.width() / config.car_width_m as f32
                            - projection.metres_to_pixels)
                            .abs()
                            < 0.001
                    );
                    assert!(
                        (player.rect.height() / config.car_length_m as f32
                            - projection.metres_to_pixels)
                            .abs()
                            < 0.001
                    );
                    assert!(panel.contains_rect(player.rect));
                    assert!(
                        !output.shapes.iter().any(|shape| matches!(
                            &shape.shape,
                            Shape::Path(_) | Shape::Circle(_) | Shape::Ellipse(_)
                        )),
                        "all cars use the same rounded rectangle"
                    );
                    // Two diagonal guide meshes plus each opponent's warning wedge.
                    assert_eq!(
                        output
                            .shapes
                            .iter()
                            .filter(|shape| matches!(&shape.shape, Shape::Mesh(_)))
                            .count(),
                        2 + frame.cars.len()
                    );
                    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                        Shape::Text(text) if text.galley.job.text == "Radar"
                    )));
                    for shape in &output.shapes {
                        if let Shape::Path(path) = &shape.shape {
                            assert!(
                                path.points
                                    .iter()
                                    .all(|point| panel.expand(0.01).contains(*point))
                            );
                        }
                    }
                    let mut vertices = 0;
                    for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
                        if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                            assert!(mesh.is_valid());
                            assert!(mesh.vertices.iter().all(|vertex| vertex.pos.is_finite()));
                            vertices += mesh.vertices.len();
                        }
                    }
                    assert!(vertices < 20_000);
                }
            }
        }
    }

    #[test]
    fn reference_rows_show_real_times_positions_and_lap_gaps() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let theme = theme::resolve(HudStyle::Gt7Inspired);
        let settings = GapSettings::default();
        let texts = |output: egui::FullOutput| -> Vec<String> {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text) => Some(text.galley.job.text.clone()),
                    _ => None,
                })
                .collect()
        };
        for seconds in [-6.456, 6.456] {
            let value = crate::delta::DeltaFrame {
                seconds: Some(seconds),
                best_seconds: Some(55.244),
                estimated_lap_seconds: Some(55.244 + seconds),
                trend: Some(0.5),
                status: "estimate".into(),
            };
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                paint_delta(
                    &ctx.layer_painter(egui::LayerId::background()),
                    Rect::from_min_size(Pos2::ZERO, theme.delta.panel.size),
                    &value,
                    &settings,
                    theme.delta,
                );
            });
            let expected = if seconds < 0.0 { "-6.456" } else { "+6.456" };
            let cell = theme.delta.value_cell.as_ref().unwrap();
            let (background, foreground) = if seconds < 0.0 {
                (cell.gaining, cell.gaining_text)
            } else {
                (cell.losing, cell.losing_text)
            };
            assert!(
                output.shapes.iter().any(
                    |shape| matches!(&shape.shape, Shape::Rect(rect) if rect.fill == background)
                )
            );
            assert_eq!(
                background,
                if seconds < 0.0 {
                    super::super::gt7_style::GAIN_TEXT
                } else {
                    Color32::from_rgb(255, 134, 145)
                }
            );
            assert_eq!(foreground, Color32::WHITE);
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                Shape::Text(text) if text.galley.job.text == expected &&
                    text.galley.job.sections[0].format.color == foreground)));
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                Shape::Text(text) if text.galley.job.text == lap_time_millis(55.244 + seconds) &&
                    text.galley.job.sections[0].format.font_id.size == 32.0 &&
                    text.galley.job.sections[0].format.font_id.family == FontFamily::Name(super::super::fonts::NUMERIC.into()))));
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                Shape::Text(text) if text.galley.job.text == "0:55.244" &&
                    text.galley.job.sections[0].format.font_id.family == FontFamily::Name(super::super::fonts::NUMERIC.into()))));
            assert!(
                !output
                    .shapes
                    .iter()
                    .any(|shape| matches!(&shape.shape, Shape::LineSegment { .. }))
            );
            let text = texts(output);
            assert!(
                !text
                    .iter()
                    .any(|t| matches!(t.as_str(), "BEST" | "LOSING" | "GAINING" | "estimate"))
            );
            for expected in ["SESSION BEST / LIVE DELTA", "0:55.244", expected] {
                assert!(text.iter().any(|t| t == expected));
            }
        }
        for (title, seconds, laps, expected) in [
            ("AHEAD", Some(0.224), None, "+0.224"),
            ("BEHIND", Some(2.131), None, "-2.131"),
            ("AHEAD", Some(2.131), Some(2), "2 laps"),
            ("BEHIND", None, None, "—.---"),
        ] {
            let value = GapValue {
                driver: Some("Inori".into()),
                position: Some(12),
                seconds,
                laps,
                ..Default::default()
            };
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                paint_gap(
                    &ctx.layer_painter(egui::LayerId::background()),
                    Rect::from_min_size(Pos2::ZERO, theme.ahead.panel.size),
                    title,
                    &value,
                    &GapPaintOptions::new(&settings, false),
                    theme.ahead,
                );
            });
            let text = texts(output);
            for expected in [title, "12", "Inori", expected] {
                assert!(text.iter().any(|t| t == expected));
            }
        }
        assert_eq!(lap_time_millis(59.9996), "1:00.000");
        assert_eq!(signed_millis(Some(-0.0001)), "+0.000");
    }

    #[test]
    fn gt_timing_text_fits_at_extreme_scales_with_wide_unicode_names() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        for hud in [HudStyle::Gt7Inspired] {
            let theme = theme::resolve(hud);
            for scale in [0.5, 1.0, 2.0] {
                let settings = GapSettings {
                    scale,
                    ..Default::default()
                };
                for available in [false, true] {
                    for delta in [false, true] {
                        let size = if delta {
                            theme.delta.panel.size
                        } else {
                            theme.ahead.panel.size
                        } * scale;
                        let rect = Rect::from_min_size(Pos2::ZERO, size);
                        let output = ctx.run(
                            egui::RawInput {
                                screen_rect: Some(rect),
                                ..Default::default()
                            },
                            |ctx| {
                                let painter = ctx.layer_painter(egui::LayerId::background());
                                if delta {
                                    let value = crate::delta::DeltaFrame {
                                        seconds: available.then_some(-123.45),
                                        trend: available.then_some(0.8),
                                        best_seconds: available.then_some(83.456),
                                        estimated_lap_seconds: available.then_some(82.345),
                                        status: "Waiting for matching track information".into(),
                                    };
                                    paint_delta(&painter, rect, &value, &settings, theme.delta);
                                } else {
                                    let value = GapValue {
                                        driver: Some("WWWWWWWWWWWWWWWWWWWWWWWWW界界界".into()),
                                        position: Some(255),
                                        seconds: available.then_some(123.4),
                                        measured_age_ms: available.then_some(350),
                                        status: "Waiting for matching track information".into(),
                                        ..Default::default()
                                    };
                                    paint_gap(
                                        &painter,
                                        rect,
                                        "AHEAD",
                                        &value,
                                        &GapPaintOptions::new(&settings, true),
                                        theme.ahead,
                                    );
                                }
                            },
                        );
                        for shape in output.shapes {
                            if let Shape::Text(text) = shape.shape {
                                assert!(
                                    rect.expand(scale)
                                        .contains_rect(text.visual_bounding_rect()),
                                    "{hud:?}: {}",
                                    text.galley.job.text
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
