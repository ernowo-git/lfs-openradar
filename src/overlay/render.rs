//! Telemetry formatting and gadget painting, independent of native window state.

use super::{
    delta_style::DeltaStyle,
    gadget_style::{CellStyle, PanelStyle, RowRule, TextFont, TextStyle},
    gap_style::GapStyle,
    radar_style::{
        MarkerShape, ProximityStyle, RadarCarStyle, RadarStyle, RadialBackgroundStyle,
        SideWarningStyle,
    },
    theme,
};
use crate::{
    config::{Config, GapSettings},
    gaps::GapValue,
    radar::{RadarFrame, Threat},
};
use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke, Vec2};

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
        TextFont::Proportional => FontFamily::Proportional,
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
    let style = theme::resolve(config.hud_style).radar;
    let size = rect.width().min(rect.height());
    let panel = Rect::from_center_size(rect.center(), Vec2::splat(size - 8.0));
    let painter = &painter.with_clip_rect(panel.intersect(painter.clip_rect()));
    if draw_background || style.marker == MarkerShape::Arrow {
        paint_gadget_panel(painter, panel, 1.0, &style.panel);
    }
    let projection = radar_projection(panel, config, style);
    let RadarProjection {
        origin: centre,
        metres_to_pixels: scale,
        ui_scale,
    } = projection;
    if style.center_band != Color32::TRANSPARENT {
        painter.rect_filled(
            Rect::from_center_size(
                centre,
                Vec2::new(
                    config.car_width_m as f32 * scale.x * 1.5,
                    panel.height() - 55.0,
                ),
            ),
            0.0,
            style.center_band,
        );
    }
    if let Some(proximity) = &style.proximity {
        paint_proximity_guides(painter, panel, style, proximity, ui_scale);
        if let Some(warnings) = &proximity.side_warnings {
            paint_side_warnings(painter, panel, proximity, frame, warnings, ui_scale);
        }
    } else {
        let grid = Stroke::new(style.grid_width, style.grid_color);
        for radius in [3.0, 6.0, 9.0] {
            painter.circle_stroke(centre, radius * scale.x, grid);
        }
        painter.line_segment(
            [
                Pos2::new(panel.left() + 15.0, centre.y),
                Pos2::new(panel.right() - 15.0, centre.y),
            ],
            grid,
        );
        painter.line_segment(
            [
                Pos2::new(centre.x, panel.top() + 30.0),
                Pos2::new(centre.x, panel.bottom() - 25.0),
            ],
            grid,
        );
    }
    if let Some(heading) = &style.heading {
        let anchor = Rect::from_min_size(Pos2::new(centre.x, panel.top()), Vec2::ZERO);
        paint_gadget_text(
            painter,
            anchor,
            ui_scale,
            heading,
            style.proximity.as_ref().map_or("RADAR", |p| p.heading),
        );
    }
    if frame.live {
        for car in &frame.cars {
            let car_style = if car.uncertain {
                style.uncertain
            } else {
                match car.threat {
                    Threat::Nearby => style.nearby,
                    Threat::Alongside => style.alongside,
                    Threat::PotentialContact => style.potential_contact,
                }
            };
            let position = projection.position(car.right, car.forward);
            draw_car(
                painter,
                position,
                &projection,
                car.relative_heading,
                config,
                car_style,
                style,
            );
        }
        draw_car(
            painter,
            centre,
            &projection,
            0.0,
            config,
            style.player,
            style,
        );
        if let Some(label) = &style.player_label {
            paint_gadget_text(
                painter,
                Rect::from_min_size(centre, Vec2::ZERO),
                1.0,
                label,
                "YOU",
            );
        }
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

fn side_warning_appearance(
    frame: &RadarFrame,
    style: &SideWarningStyle,
) -> [Option<(Color32, f32)>; 2] {
    let mut sides: [Option<(u8, Color32)>; 2] = [None, None];
    if frame.live {
        for car in &frame.cars {
            let side = if car.right < -style.center_tolerance_m {
                0
            } else if car.right > style.center_tolerance_m {
                1
            } else {
                continue;
            };
            let (priority, color) = if car.uncertain {
                (0, style.uncertain)
            } else {
                match car.threat {
                    Threat::Nearby => (1, style.nearby),
                    Threat::Alongside => (2, style.alongside),
                    Threat::PotentialContact => (3, style.contact),
                }
            };
            if sides[side].is_none_or(|(current, _)| priority > current) {
                sides[side] = Some((priority, color));
            }
        }
    }
    sides.map(|side| {
        side.map(|(priority, color)| {
            let inner_radius = if priority == 3 {
                style.contact_inner_radius
            } else {
                style.inner_radius
            };
            (color, inner_radius)
        })
    })
}

fn paint_side_warnings(
    painter: &egui::Painter,
    panel: Rect,
    proximity: &ProximityStyle,
    frame: &RadarFrame,
    style: &SideWarningStyle,
    ui_scale: f32,
) {
    let painter = painter.with_clip_rect(panel.intersect(painter.clip_rect()));
    let center = panel_point(panel, proximity.guide_center);
    let radius = panel.width().min(panel.height()) * proximity.guide_radius;
    const SEGMENTS: u32 = 32;
    for (side, appearance) in side_warning_appearance(frame, style)
        .into_iter()
        .enumerate()
    {
        let Some((color, inner_radius)) = appearance else {
            continue;
        };
        let angle = if side == 0 { std::f32::consts::PI } else { 0.0 };
        let mut mesh = egui::Mesh::default();
        let mut outer = Vec::new();
        let mut inner = Vec::new();
        for step in 0..=SEGMENTS {
            let theta =
                angle - style.half_angle + 2.0 * style.half_angle * step as f32 / SEGMENTS as f32;
            let direction = Vec2::new(theta.cos(), theta.sin());
            let inside = center + direction * radius * inner_radius;
            let outside = center + direction * radius;
            mesh.colored_vertex(inside, color.gamma_multiply(style.fill_opacity));
            mesh.colored_vertex(outside, color.gamma_multiply(style.fill_opacity));
            inner.push(inside);
            outer.push(outside);
        }
        for step in 0..SEGMENTS {
            let index = step * 2;
            mesh.add_triangle(index, index + 1, index + 3);
            mesh.add_triangle(index, index + 3, index + 2);
        }
        painter.add(Shape::mesh(mesh));
        let mut outline = outer;
        outline.extend(inner.into_iter().rev());
        painter.add(Shape::closed_line(
            outline,
            Stroke::new(style.border_width * ui_scale, color),
        ));
    }
}

fn panel_point(panel: Rect, fraction: Vec2) -> Pos2 {
    panel.min + Vec2::new(fraction.x * panel.width(), fraction.y * panel.height())
}

#[derive(Clone, Copy)]
struct RadarProjection {
    origin: Pos2,
    /// Classic uses uniform zoom; GT7 fits each axis to its configured range.
    metres_to_pixels: Vec2,
    ui_scale: f32,
}

impl RadarProjection {
    fn position(&self, right: f64, forward: f64) -> Pos2 {
        self.origin + Vec2::new(right as f32, -forward as f32) * self.metres_to_pixels
    }
}

fn radar_projection(panel: Rect, config: &Config, style: &RadarStyle) -> RadarProjection {
    let Some(proximity) = &style.proximity else {
        let size = panel.width().min(panel.height()) + 8.0;
        // Side range controls the zoom of the entire Classic radar. Do not cap
        // it by front/rear range or stretch the physical car footprints.
        let scale = (size - 60.0) / (2.0 * config.side_m + config.car_width_m) as f32;
        return RadarProjection {
            origin: panel.center() + Vec2::new(0.0, 10.0),
            metres_to_pixels: Vec2::splat(scale),
            ui_scale: 1.0,
        };
    };
    let ui_scale = panel.width().min(panel.height()) / style.panel.size.x;
    let origin = panel_point(panel, proximity.origin);
    // Reserve symbol room before fitting each axis to its configured range.
    // The rings follow the reference layout and do not represent track geometry.
    let padding = proximity.marker_size.length() * ui_scale * 0.5;
    // Match the engine's conservative range padding, including rotated footprints.
    let physical_padding = config.car_length_m.hypot(config.car_width_m) as f32 * 0.5;
    let front = (origin.y - panel.top() - proximity.top * panel.height() - padding)
        / (config.front_m as f32 + physical_padding);
    let rear = (panel.top() + proximity.bottom * panel.height() - origin.y - padding)
        / (config.rear_m as f32 + physical_padding);
    let side = (panel.width() * (0.5 - proximity.side_inset) - padding)
        / (config.side_m as f32 + physical_padding);
    RadarProjection {
        origin,
        metres_to_pixels: Vec2::new(side.max(0.0), front.min(rear).max(0.0)),
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
    let guide_center = panel_point(panel, proximity.guide_center);
    let top = panel.top() + proximity.top * panel.height();
    let radius = panel.width().min(panel.height()) * proximity.guide_radius;
    if let Some(background) = &proximity.background {
        let painter = painter.with_clip_rect(panel.intersect(painter.clip_rect()));
        paint_radial_background(&painter, guide_center, radius, background);
    }
    let grid = Stroke::new(style.grid_width * ui_scale, style.grid_color);
    for fraction in proximity.ring_fractions {
        let radius = radius * fraction;
        let angle = ((guide_center.y - top) / radius).clamp(0.0, 1.0).asin();
        const SEGMENTS: usize = 96;
        let point = |step: usize| {
            let theta =
                -angle + (std::f32::consts::PI + 2.0 * angle) * step as f32 / SEGMENTS as f32;
            guide_center + Vec2::new(theta.cos(), theta.sin()) * radius
        };
        for step in 0..SEGMENTS {
            let a = point(step);
            let b = point(step + 1);
            let opacity = (((a.y + b.y) * 0.5 - top) / (guide_center.y - top)).clamp(0.0, 1.0);
            painter.line_segment(
                [a, b],
                Stroke::new(grid.width, grid.color.gamma_multiply(opacity)),
            );
        }
    }
    painter.line_segment(
        [
            guide_center - Vec2::new(radius, 0.0),
            guide_center + Vec2::new(radius, 0.0),
        ],
        grid,
    );
}

fn radial_background_color(style: &RadialBackgroundStyle, distance: f32) -> Color32 {
    let distance = distance.clamp(0.0, 1.0);
    let interpolate = |center: f32, edge: f32| center + (edge - center) * distance;
    let center = style.center_color.to_srgba_unmultiplied();
    let edge = style.edge_color.to_srgba_unmultiplied();
    // Interpolate RGB separately from alpha so a transparent gray edge still
    // introduces gray along the fade, rather than premultiplied black throughout.
    Color32::from_rgba_unmultiplied(
        interpolate(center[0] as f32, edge[0] as f32).round() as u8,
        interpolate(center[1] as f32, edge[1] as f32).round() as u8,
        interpolate(center[2] as f32, edge[2] as f32).round() as u8,
        (interpolate(style.center_opacity, style.edge_opacity).clamp(0.0, 1.0) * 255.0).round()
            as u8,
    )
}

fn paint_radial_background(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    style: &RadialBackgroundStyle,
) {
    const RINGS: u32 = 16;
    const SEGMENTS: u32 = 64;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, radial_background_color(style, 0.0));
    for ring in 1..=RINGS {
        let distance = ring as f32 / RINGS as f32;
        let color = radial_background_color(style, distance);
        for segment in 0..SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            mesh.colored_vertex(
                center + Vec2::new(angle.cos(), angle.sin()) * radius * distance,
                color,
            );
        }
    }
    for segment in 0..SEGMENTS {
        mesh.add_triangle(0, 1 + segment, 1 + (segment + 1) % SEGMENTS);
    }
    for ring in 1..RINGS {
        let inner = 1 + (ring - 1) * SEGMENTS;
        let outer = inner + SEGMENTS;
        for segment in 0..SEGMENTS {
            let next = (segment + 1) % SEGMENTS;
            mesh.add_triangle(inner + segment, outer + segment, outer + next);
            mesh.add_triangle(inner + segment, outer + next, inner + next);
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn draw_car(
    painter: &egui::Painter,
    centre: Pos2,
    projection: &RadarProjection,
    angle: f64,
    config: &Config,
    style: &RadarCarStyle,
    radar: &RadarStyle,
) {
    let scale = projection.metres_to_pixels.x;
    let ui_scale = projection.ui_scale;
    let (sin, cos) = angle.sin_cos();
    let width = config.car_width_m as f32 * scale;
    let height = config.car_length_m as f32 * scale;
    if radar.marker == MarkerShape::Arrow {
        draw_arrow(
            painter,
            centre,
            radar
                .proximity
                .as_ref()
                .map_or(Vec2::new(width, height), |p| p.marker_size * ui_scale),
            angle,
            &RadarCarStyle {
                border_width: style.border_width * ui_scale,
                ..*style
            },
            radar.fill_brightness,
        );
        return;
    }
    let border = style.border_width.clamp(0.0, width.min(height) * 0.5);
    let corners = |width: f32, height: f32| {
        [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
            .map(|(x, y)| {
                let x = x * width;
                let y = y * height;
                centre
                    + Vec2::new(
                        x * cos as f32 - y * sin as f32,
                        -(x * sin as f32 + y * cos as f32),
                    )
            })
            .to_vec()
    };
    let fill = style.fill.gamma_multiply(radar.fill_brightness);
    // Two nested polygons keep the border inside the rotated car footprint.
    painter.add(Shape::convex_polygon(
        corners(width, height),
        if border > 0.0 {
            style.border_color
        } else {
            fill
        },
        Stroke::NONE,
    ));
    let inner_width = width - 2.0 * border;
    let inner_height = height - 2.0 * border;
    if border > 0.0 && inner_width > 0.0 && inner_height > 0.0 {
        painter.add(Shape::convex_polygon(
            corners(inner_width, inner_height),
            fill,
            Stroke::NONE,
        ));
    }
    let nose = centre
        + Vec2::new(
            (-sin * config.car_length_m * 0.35) as f32,
            (-cos * config.car_length_m * 0.35) as f32,
        ) * scale;
    painter.circle_filled(nose, 2.0, Color32::WHITE);
}

fn draw_arrow(
    painter: &egui::Painter,
    centre: Pos2,
    size: Vec2,
    angle: f64,
    style: &RadarCarStyle,
    brightness: f32,
) {
    let half_width = size.x * 0.5;
    let height = size.y;
    let side = height.hypot(half_width);
    // Inset the triangle's edges, rather than scaling it, for an even inner border.
    let inradius = half_width * height / (half_width + side);
    let border = style.border_width.clamp(0.0, inradius);
    let (sin, cos) = angle.sin_cos();
    let transform = |points: [(f32, f32); 3]| {
        points
            .map(|(x, y)| {
                centre
                    + Vec2::new(
                        x * cos as f32 - y * sin as f32,
                        -(x * sin as f32 + y * cos as f32),
                    )
            })
            .to_vec()
    };
    let fill = style.fill.gamma_multiply(brightness);
    painter.add(Shape::convex_polygon(
        transform([
            (0.0, height * 0.5),
            (-half_width, -height * 0.5),
            (half_width, -height * 0.5),
        ]),
        if border > 0.0 {
            style.border_color
        } else {
            fill
        },
        Stroke::NONE,
    ));
    if border > 0.0 && border < inradius {
        let inner_half_width = half_width - border * (side + half_width) / height;
        let tip = height * 0.5 - border * side / half_width;
        let base = -height * 0.5 + border;
        painter.add(Shape::convex_polygon(
            transform([
                (0.0, tip),
                (-inner_half_width, base),
                (inner_half_width, base),
            ]),
            fill,
            Stroke::NONE,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HudStyle;

    #[test]
    fn side_range_changes_horizontal_positions_throughout_both_theme_ranges() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let frame = RadarFrame {
            live: true,
            cars: vec![crate::radar::RadarCar {
                plid: 2,
                name: "Opponent".into(),
                right: 0.4,
                forward: 2.0,
                relative_heading: 0.0,
                threat: Threat::Nearby,
                uncertain: false,
            }],
            ..Default::default()
        };
        for hud in [HudStyle::Classic, HudStyle::Gt7Inspired] {
            let style = theme::resolve(hud).radar;
            for size in [220.0, 320.0, 800.0] {
                let mut previous: Option<Vec2> = None;
                for side in [0.0, 0.1, 1.0, 3.0, 5.0, 8.0, 9.0, 12.0] {
                    let config = Config {
                        hud_style: hud,
                        side_m: side,
                        ..Default::default()
                    };
                    config.validate().unwrap();
                    let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(size));
                    let projection = radar_projection(canvas.shrink(4.0), &config, style);
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
                                true,
                            );
                        },
                    );
                    let marker_center = |car: &RadarCarStyle| {
                        output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                Shape::Path(path)
                                    if path.fill
                                        == car.fill.gamma_multiply(style.fill_brightness) =>
                                {
                                    Some(Rect::from_points(&path.points).center())
                                }
                                _ => None,
                            })
                            .unwrap()
                    };
                    let offset = marker_center(style.nearby) - marker_center(style.player);
                    assert!(
                        (offset - (projection.position(0.4, 2.0) - projection.origin)).length()
                            < 0.001
                    );
                    assert!(offset.is_finite());
                    if let Some(previous) = previous {
                        assert!(
                            offset.x < previous.x,
                            "{hud:?}, size {size}, side {side}: horizontal zoom did not change"
                        );
                        if hud == HudStyle::Classic {
                            assert!(offset.y.abs() < previous.y.abs());
                        } else {
                            assert!((offset.y - previous.y).abs() < 0.001);
                        }
                    }
                    previous = Some(offset);
                }
            }
        }
    }

    #[test]
    fn classic_zoom_keeps_circles_round_and_footprints_proportional() {
        let ctx = egui::Context::default();
        let config = Config {
            hud_style: HudStyle::Classic,
            ..Default::default()
        };
        let radar = theme::resolve(HudStyle::Classic).radar;
        for side in [0.0, 0.1, 1.0, 3.0, 5.0, 8.0, 9.0, 12.0] {
            let config = Config {
                side_m: side,
                ..config.clone()
            };
            let panel = Rect::from_min_size(Pos2::ZERO, Vec2::splat(320.0)).shrink(4.0);
            let projection = radar_projection(panel, &config, radar);
            let scale = projection.metres_to_pixels;
            assert_eq!(scale.x, scale.y);
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                paint(
                    &ctx.layer_painter(egui::LayerId::background()),
                    panel.expand(4.0),
                    &RadarFrame {
                        live: true,
                        ..Default::default()
                    },
                    &config,
                    true,
                );
            });
            let radii: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Circle(circle) if circle.stroke.color == radar.grid_color => {
                        Some(circle.radius)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(radii, vec![3.0 * scale.x, 6.0 * scale.x, 9.0 * scale.x]);
            for angle in [0.0, 0.7, std::f64::consts::FRAC_PI_2] {
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    draw_car(
                        &ctx.layer_painter(egui::LayerId::background()),
                        projection.origin,
                        &projection,
                        angle,
                        &config,
                        radar.player,
                        radar,
                    );
                });
                let paths: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Path(path) => Some(&path.points),
                        _ => None,
                    })
                    .collect();
                assert_eq!(paths.len(), 2);
                let (outer, inner) = (paths[0], paths[1]);
                let (sin, cos) = angle.sin_cos();
                for point in outer {
                    let world = (*point - projection.origin) / scale;
                    let right = world.x as f64 * cos - world.y as f64 * sin;
                    let forward = -world.x as f64 * sin - world.y as f64 * cos;
                    assert!((right.abs() - config.car_width_m * 0.5).abs() < 0.00001);
                    assert!((forward.abs() - config.car_length_m * 0.5).abs() < 0.00001);
                }
                for edge in 0..4 {
                    let direction = outer[(edge + 1) % 4] - outer[edge];
                    let inset = inner[edge] - outer[edge];
                    let distance =
                        (direction.x * inset.y - direction.y * inset.x).abs() / direction.length();
                    assert!((distance - radar.player.border_width).abs() < 0.0001);
                }
            }
        }
    }

    #[test]
    fn side_warnings_follow_relative_position_and_highest_live_threat() {
        let radar = theme::resolve(HudStyle::Gt7Inspired).radar;
        let style = radar
            .proximity
            .as_ref()
            .unwrap()
            .side_warnings
            .as_ref()
            .unwrap();
        let car = |right, threat, uncertain| crate::radar::RadarCar {
            plid: 1,
            name: "Opponent".into(),
            right,
            forward: 0.0,
            relative_heading: 0.0,
            threat,
            uncertain,
        };
        let mut frame = RadarFrame {
            live: true,
            ..Default::default()
        };
        assert_eq!(side_warning_appearance(&frame, style), [None, None]);
        frame.cars = vec![car(-2.0, Threat::Nearby, false)];
        assert_eq!(
            side_warning_appearance(&frame, style),
            [Some((style.nearby, style.inner_radius)), None]
        );
        frame.cars.extend([
            car(-1.0, Threat::PotentialContact, false),
            car(-3.0, Threat::Alongside, false),
            car(2.0, Threat::Alongside, false),
            car(0.0, Threat::PotentialContact, false),
            car(2.0, Threat::PotentialContact, true),
        ]);
        assert_eq!(
            side_warning_appearance(&frame, style),
            [
                Some((style.contact, style.contact_inner_radius)),
                Some((style.alongside, style.inner_radius))
            ]
        );
        frame.cars = vec![car(2.0, Threat::PotentialContact, true)];
        assert_eq!(
            side_warning_appearance(&frame, style),
            [None, Some((style.uncertain, style.inner_radius))]
        );
        frame.live = false;
        assert_eq!(side_warning_appearance(&frame, style), [None, None]);
        for opponent in [radar.nearby, radar.alongside, radar.potential_contact] {
            assert_eq!(opponent.border_color, radar.nearby.border_color);
        }
    }

    #[test]
    fn contact_side_warning_expands_inward_on_either_side() {
        let proximity = theme::resolve(HudStyle::Gt7Inspired)
            .radar
            .proximity
            .as_ref()
            .unwrap();
        let style = proximity.side_warnings.as_ref().unwrap();
        assert!(style.contact_inner_radius < style.inner_radius);
        let ctx = egui::Context::default();
        for size in [220.0, 640.0] {
            let panel = Rect::from_min_size(Pos2::ZERO, Vec2::splat(size));
            let center = panel_point(panel, proximity.guide_center);
            let radius = size * proximity.guide_radius;
            for right in [-2.0, 2.0] {
                for (threat, uncertain, inner_radius) in [
                    (Threat::Nearby, false, style.inner_radius),
                    (Threat::Alongside, false, style.inner_radius),
                    (Threat::PotentialContact, false, style.contact_inner_radius),
                    (Threat::PotentialContact, true, style.inner_radius),
                ] {
                    let frame = RadarFrame {
                        live: true,
                        cars: vec![crate::radar::RadarCar {
                            plid: 1,
                            name: "Opponent".into(),
                            right,
                            forward: 0.0,
                            relative_heading: 0.0,
                            threat,
                            uncertain,
                        }],
                        ..Default::default()
                    };
                    let output = ctx.run(egui::RawInput::default(), |ctx| {
                        paint_side_warnings(
                            &ctx.layer_painter(egui::LayerId::background()),
                            panel,
                            proximity,
                            &frame,
                            style,
                            size / 320.0,
                        );
                    });
                    let Shape::Mesh(mesh) = &output.shapes[0].shape else {
                        panic!("expected warning mesh");
                    };
                    for pair in mesh.vertices.as_chunks::<2>().0 {
                        assert!(
                            ((pair[0].pos - center).length() - radius * inner_radius).abs() < 0.001
                        );
                        assert!(((pair[1].pos - center).length() - radius).abs() < 0.001);
                        assert_eq!((pair[0].pos.x - center.x).signum(), (right as f32).signum());
                    }
                }
            }
        }
    }

    #[test]
    fn gap_diagnostics_follow_debug_toggle_in_both_themes() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        for hud in [HudStyle::Classic, HudStyle::Gt7Inspired] {
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
    fn radial_background_fades_through_grey_to_a_clear_edge() {
        let background = theme::resolve(HudStyle::Gt7Inspired)
            .radar
            .proximity
            .as_ref()
            .unwrap()
            .background
            .as_ref()
            .unwrap();
        let center = radial_background_color(background, 0.0);
        let middle = radial_background_color(background, 0.5);
        let edge = radial_background_color(background, 1.0);
        assert_eq!(center.to_srgba_unmultiplied(), [0, 0, 0, 102]);
        assert_eq!(middle.a(), 51);
        let rgb = middle.to_srgba_unmultiplied();
        assert!((45..=51).contains(&rgb[0]));
        assert_eq!(rgb[0], rgb[1]);
        assert_eq!(rgb[1], rgb[2]);
        assert_eq!(edge, Color32::TRANSPARENT);
    }

    #[test]
    fn proximity_radar_keeps_live_coordinates_and_compact_symbols_inside_the_panel() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let style = theme::resolve(HudStyle::Gt7Inspired).radar;
        assert_eq!(style.center_band, Color32::TRANSPARENT);
        for size in [220.0, 320.0, 800.0] {
            for side in [0.0, 0.1, 3.0, 5.0, 12.0] {
                let config = Config {
                    hud_style: HudStyle::Gt7Inspired,
                    side_m: side,
                    overlay_size: size,
                    ..Default::default()
                };
                let frame = RadarFrame {
                    live: true,
                    cars: vec![crate::radar::RadarCar {
                        plid: 2,
                        name: "Opponent".into(),
                        right: 2.0,
                        forward: 3.0,
                        relative_heading: 0.0,
                        threat: Threat::Nearby,
                        uncertain: false,
                    }],
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
                let arrow = |color| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            Shape::Path(path) if path.points.len() == 3 && path.fill == color => {
                                Some(Rect::from_points(&path.points))
                            }
                            _ => None,
                        })
                        .unwrap()
                };
                let player = arrow(style.player.border_color);
                let opponent = arrow(style.nearby.border_color);
                assert!(player.center().distance(projection.origin) < 0.001);
                let offset = opponent.center() - player.center();
                assert!((offset.x / 2.0 - projection.metres_to_pixels.x).abs() < 0.001);
                assert!((-offset.y / 3.0 - projection.metres_to_pixels.y).abs() < 0.001);
                let proximity = style.proximity.as_ref().unwrap();
                assert!(
                    (player.size() - proximity.marker_size * projection.ui_scale).length() < 0.001
                );
                assert!(panel.contains_rect(player) && panel.contains_rect(opponent));
                let caption = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.galley.job.text == "Radar" => {
                            Some(text.visual_bounding_rect())
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(
                    caption.top()
                        > panel.top()
                            + (proximity.guide_center.y + proximity.guide_radius) * panel.height()
                );
                assert!(panel.contains_rect(caption));
                for shape in &output.shapes {
                    if let Shape::Path(path) = &shape.shape {
                        assert!(
                            path.points
                                .iter()
                                .all(|point| panel.expand(0.01).contains(*point))
                        );
                        if path.points.len() == 2 {
                            // The reference has a horizontal guide, with no vertical axis/strip.
                            assert!((path.points[0].x - path.points[1].x).abs() > 0.001);
                        }
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
    fn timing_text_fits_both_themes_at_extreme_scales_with_wide_unicode_names() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        for hud in [HudStyle::Classic, HudStyle::Gt7Inspired] {
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

    #[test]
    fn rotated_arrow_borders_stay_inside_the_car_footprint() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let size = Vec2::new(18.0, 42.0);
        let centre = Pos2::new(100.0, 100.0);
        for angle in [0.0, 0.7, std::f64::consts::FRAC_PI_2, std::f64::consts::PI] {
            for width in [0.0, 1.5, 1000.0] {
                let style = RadarCarStyle {
                    border_width: width,
                    ..*theme::resolve(HudStyle::Gt7Inspired).radar.nearby
                };
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    draw_arrow(
                        &ctx.layer_painter(egui::LayerId::background()),
                        centre,
                        size,
                        angle,
                        &style,
                        1.0,
                    );
                });
                let paths: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Path(path) => Some(path),
                        _ => None,
                    })
                    .collect();
                assert!(!paths.is_empty());
                let outer = &paths[0].points;
                for path in &paths {
                    assert!(path.points.iter().all(|p| p.is_finite()));
                }
                if width == 1.5 {
                    assert_eq!(paths.len(), 2);
                    for point in &paths[1].points {
                        for edge in 0..3 {
                            let a = outer[edge];
                            let b = outer[(edge + 1) % 3];
                            let direction = b - a;
                            let relative = *point - a;
                            let distance = (direction.x * relative.y - direction.y * relative.x)
                                .abs()
                                / direction.length();
                            assert!(
                                distance >= width - 0.001,
                                "inner border escapes rotated triangle"
                            );
                        }
                    }
                } else {
                    assert_eq!(paths.len(), 1);
                }
                for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
                    if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                        assert!(mesh.is_valid());
                        assert!(mesh.vertices.iter().all(|vertex| vertex.pos.is_finite()));
                    }
                }
            }
        }
    }
}
