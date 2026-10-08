//! Resolve a saved HUD selection into one immutable bundle for all viewports.

use super::{delta_style, gap_style, gt7_style, radar_style};
use crate::config::HudStyle;

pub(super) struct HudTheme {
    pub radar: &'static radar_style::RadarStyle,
    pub ahead: &'static gap_style::GapStyle,
    pub behind: &'static gap_style::GapStyle,
    pub delta: &'static delta_style::DeltaStyle,
}

const CLASSIC: HudTheme = HudTheme {
    radar: &radar_style::CLASSIC,
    ahead: &gap_style::AHEAD,
    behind: &gap_style::BEHIND,
    delta: &delta_style::DELTA,
};

const GT7: HudTheme = HudTheme {
    radar: &gt7_style::RADAR,
    ahead: &gt7_style::AHEAD,
    behind: &gt7_style::BEHIND,
    delta: &gt7_style::DELTA,
};

pub(super) fn resolve(style: HudStyle) -> &'static HudTheme {
    match style {
        HudStyle::Classic => &CLASSIC,
        HudStyle::Gt7Inspired => &GT7,
    }
}
