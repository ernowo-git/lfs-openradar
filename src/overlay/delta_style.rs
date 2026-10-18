//! Appearance settings for the performance delta gadget.

use super::gadget_style::{CellStyle, PanelStyle, RowRule, TextStyle, TimingCellStyle};
use eframe::egui::Color32;

pub(super) struct ReferenceTimeStripStyle {
    pub time_cell: CellStyle,
    pub heading: TextStyle,
}

pub(super) struct TrendStyle {
    pub track_color: Color32,
    pub gaining_color: Color32,
    pub losing_color: Color32,
    pub track_width: f32,
    pub bar_width: f32,
    pub half_length: f32,
    pub right_offset: f32,
    pub top_offset: f32,
}

pub(super) struct DeltaStyle {
    pub panel: PanelStyle,
    pub heading: TextStyle,
    pub gaining_color: Color32,
    pub losing_color: Color32,
    pub reference: TextStyle,
    pub estimate: TextStyle,
    pub estimate_label: Option<TextStyle>,
    pub status: Option<TextStyle>,
    pub trend: Option<TrendStyle>,
    pub label: Option<TextStyle>,
    pub value_cell: Option<TimingCellStyle>,
    pub rule: Option<RowRule>,
    pub reference_time_strip: Option<ReferenceTimeStripStyle>,
}
