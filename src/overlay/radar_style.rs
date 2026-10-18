//! Radar sensor and warning appearance. Keep drawing and telemetry logic out of this module.

use super::gadget_style::{PanelStyle, TextStyle};
use eframe::egui::{Color32, Vec2};

pub(super) struct WarningStyle {
    /// Band height as a fraction of the sensor radius.
    pub band_height: f32,
    pub contact_band_height: f32,
    pub fill_opacity: f32,
    /// Fraction of the radius over which the circular edge fades out.
    pub edge_fade: f32,
    /// Cars within this distance of the centre line activate an ahead/rear cap.
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
    pub top: f32,
    pub bottom: f32,
    pub side_inset: f32,
    pub warnings: WarningStyle,
    /// White player marker size at the theme's base panel size.
    pub marker_size: Vec2,
}

pub(super) struct RadarStyle {
    pub player: Color32,
    pub panel: PanelStyle,
    pub grid_color: Color32,
    pub grid_width: f32,
    pub paused: TextStyle,
    pub hint: TextStyle,
    pub proximity: Option<ProximityStyle>,
}
