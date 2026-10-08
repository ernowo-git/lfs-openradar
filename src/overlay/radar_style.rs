//! Radar car appearance. Keep drawing and telemetry logic out of this module.

use super::gadget_style::{PanelStyle, TextFont, TextStyle};
use eframe::egui::{Align2, Color32, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkerShape {
    Car,
    Arrow,
}

pub(super) struct RadialBackgroundStyle {
    pub center_color: Color32,
    pub edge_color: Color32,
    /// Opacity from 0.0 (transparent) to 1.0 (opaque).
    pub center_opacity: f32,
    pub edge_opacity: f32,
}

pub(super) struct SideWarningStyle {
    /// Fraction of the outer circle radius where the warning sector starts.
    pub inner_radius: f32,
    /// Contact warnings extend inward to this fraction of the outer radius.
    pub contact_inner_radius: f32,
    pub half_angle: f32,
    pub fill_opacity: f32,
    pub border_width: f32,
    /// Cars within this distance of the centre line do not activate a side.
    pub center_tolerance_m: f64,
    pub nearby: Color32,
    pub alongside: Color32,
    pub contact: Color32,
    pub uncertain: Color32,
}

/// Proximity-display layout in fractions of the radar panel, except marker pixels.
/// Guide geometry is decorative; car coordinates still come from live telemetry.
pub(super) struct ProximityStyle {
    pub origin: Vec2,
    pub guide_center: Vec2,
    pub guide_radius: f32,
    pub ring_fractions: [f32; 3],
    pub top: f32,
    pub bottom: f32,
    pub side_inset: f32,
    /// None disables the radial background, which follows the outer guide circle.
    pub background: Option<RadialBackgroundStyle>,
    pub side_warnings: Option<SideWarningStyle>,
    /// Directional symbol size at the theme's base panel size.
    pub marker_size: Vec2,
    pub heading: &'static str,
}

pub(super) struct RadarStyle {
    pub player: &'static RadarCarStyle,
    pub nearby: &'static RadarCarStyle,
    pub alongside: &'static RadarCarStyle,
    pub potential_contact: &'static RadarCarStyle,
    pub uncertain: &'static RadarCarStyle,
    pub marker: MarkerShape,
    pub fill_brightness: f32,
    pub panel: PanelStyle,
    pub grid_color: Color32,
    pub grid_width: f32,
    pub center_band: Color32,
    pub heading: Option<TextStyle>,
    pub player_label: Option<TextStyle>,
    pub paused: TextStyle,
    pub hint: TextStyle,
    pub proximity: Option<ProximityStyle>,
}

pub(super) struct RadarCarStyle {
    pub fill: Color32,
    pub border_color: Color32,
    /// Inner border thickness in logical pixels, independent of radar zoom.
    /// Zero disables the border; rendering clamps it to fit the car body.
    pub border_width: f32,
}

const BORDER_COLOR: Color32 = Color32::from_rgb(255, 255, 255);
const BORDER_WIDTH: f32 = 1.5;
/// Retain the radar's existing shading of the base fill colors.
pub(super) const FILL_BRIGHTNESS: f32 = 0.85;

pub(super) const PLAYER: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(95, 231, 202),
    border_color: BORDER_COLOR,
    border_width: BORDER_WIDTH,
};

pub(super) const NEARBY: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(121, 185, 235),
    ..PLAYER
};

pub(super) const ALONGSIDE: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(252, 193, 94),
    ..PLAYER
};

pub(super) const POTENTIAL_CONTACT: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(255, 99, 99),
    border_color: Color32::WHITE,
    border_width: 2.0,
};

pub(super) const UNCERTAIN: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(109, 123, 142),
    ..PLAYER
};

pub(super) const CLASSIC: RadarStyle = RadarStyle {
    player: &PLAYER,
    nearby: &NEARBY,
    alongside: &ALONGSIDE,
    potential_contact: &POTENTIAL_CONTACT,
    uncertain: &UNCERTAIN,
    marker: MarkerShape::Car,
    fill_brightness: FILL_BRIGHTNESS,
    panel: PanelStyle {
        size: Vec2::new(320.0, 320.0),
        background: Color32::from_rgba_unmultiplied_const(13, 23, 35, 225),
        corner_radius: 16.0,
        border_color: Color32::WHITE,
        border_width: 0.0,
    },
    grid_color: Color32::from_rgb(37, 57, 74),
    grid_width: 1.0,
    center_band: Color32::TRANSPARENT,
    proximity: None,
    heading: None,
    player_label: Some(TextStyle {
        color: Color32::from_rgb(9, 32, 34),
        font_size: 9.0,
        offset: Vec2::ZERO,
        align: Align2::CENTER_CENTER,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    }),
    paused: TextStyle {
        color: Color32::from_rgb(160, 177, 192),
        font_size: 14.0,
        offset: Vec2::ZERO,
        align: Align2::CENTER_CENTER,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
    hint: TextStyle {
        color: Color32::from_rgb(119, 146, 163),
        font_size: 10.0,
        offset: Vec2::new(0.0, -10.0),
        align: Align2::CENTER_BOTTOM,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
};
