//! Radar car appearance. Keep drawing and telemetry logic out of this module.

use super::gadget_style::{PanelStyle, TextStyle};
use eframe::egui::{Color32, Vec2};

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
