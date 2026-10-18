//! Independent appearance settings for the ahead and behind gap gadgets.

use super::gadget_style::{CellStyle, PanelStyle, RowRule, TextStyle};
use eframe::egui::Color32;

pub(super) struct DriverRowStyle {
    pub position_cell: CellStyle,
    pub driver_cell: CellStyle,
    pub gap_cell: CellStyle,
    pub gap_fade_from: Color32,
    pub gap_fade_to: Color32,
    pub position: TextStyle,
}

pub(super) struct GapStyle {
    pub panel: PanelStyle,
    pub heading: TextStyle,
    pub driver: TextStyle,
    pub status: TextStyle,
    /// Separate value placement for compact timing-row layouts.
    pub value: Option<TextStyle>,
    pub rule: Option<RowRule>,
    pub driver_row: Option<DriverRowStyle>,
}
