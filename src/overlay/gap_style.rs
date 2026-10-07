//! Independent appearance settings for the ahead and behind gap gadgets.

use super::gadget_style::{self, CellStyle, PanelStyle, RowRule, TextFont, TextStyle};
use eframe::egui::{Align2, Color32, Stroke, Vec2};

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

pub(super) const AHEAD: GapStyle = GapStyle {
    panel: gadget_style::PANEL,
    heading: TextStyle {
        color: Color32::from_rgb(92, 204, 222),
        font_size: 20.0,
        offset: Vec2::new(8.0, 7.0),
        align: Align2::LEFT_TOP,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
    driver: TextStyle {
        color: Color32::WHITE,
        font_size: 13.0,
        offset: Vec2::new(8.0, 32.0),
        align: Align2::LEFT_TOP,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
    status: TextStyle {
        color: Color32::GRAY,
        font_size: 10.0,
        offset: Vec2::new(8.0, 53.0),
        align: Align2::LEFT_TOP,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
    value: None,
    rule: None,
    driver_row: None,
};

// Override any field here to customize the behind gadget independently.
pub(super) const BEHIND: GapStyle = GapStyle { ..AHEAD };
