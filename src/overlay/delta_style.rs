//! Appearance settings for the performance delta gadget.

use super::gadget_style::{
    self, CellStyle, PanelStyle, RowRule, TextFont, TextStyle, TimingCellStyle,
};
use eframe::egui::{Align2, Color32, Stroke, Vec2};

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

pub(super) const DELTA: DeltaStyle = DeltaStyle {
    panel: PanelStyle {
        size: Vec2::new(230.0, 100.0),
        ..gadget_style::PANEL
    },
    heading: TextStyle {
        color: Color32::WHITE,
        font_size: 23.0,
        offset: Vec2::new(8.0, 7.0),
        align: Align2::LEFT_TOP,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
    gaining_color: Color32::from_rgb(100, 230, 145),
    losing_color: Color32::from_rgb(255, 120, 110),
    reference: TextStyle {
        color: Color32::WHITE,
        font_size: 12.0,
        offset: Vec2::new(8.0, 34.0),
        align: Align2::LEFT_TOP,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
    estimate: TextStyle {
        color: Color32::WHITE,
        font_size: 13.0,
        offset: Vec2::new(8.0, 54.0),
        align: Align2::LEFT_TOP,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    },
    estimate_label: None,
    status: Some(TextStyle {
        color: Color32::GRAY,
        font_size: 10.0,
        offset: Vec2::new(8.0, 78.0),
        align: Align2::LEFT_TOP,
        font: TextFont::Proportional,
        outline: Stroke::NONE,
    }),
    trend: Some(TrendStyle {
        track_color: Color32::DARK_GRAY,
        gaining_color: Color32::GREEN,
        losing_color: Color32::LIGHT_RED,
        track_width: 3.0,
        bar_width: 4.0,
        half_length: 28.0,
        right_offset: 43.0,
        top_offset: 84.0,
    }),
    label: None,
    value_cell: None,
    rule: None,
    reference_time_strip: None,
};
