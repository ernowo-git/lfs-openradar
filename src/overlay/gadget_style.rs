//! Shared appearance types for the gap and delta gadgets.

use eframe::egui::{Align2, Color32, Stroke, Vec2};

#[derive(Clone, Copy)]
pub(super) enum TextFont {
    Proportional,
    Body,
    Display,
}

#[derive(Clone, Copy)]
pub(super) struct PanelStyle {
    /// Base window size in logical pixels, before the user's scale is applied.
    pub size: Vec2,
    pub background: Color32,
    pub border_color: Color32,
    /// Inner border thickness at scale 1.0. Zero disables the border.
    pub border_width: f32,
    pub corner_radius: f32,
}

#[derive(Clone, Copy)]
pub(super) struct TextStyle {
    pub color: Color32,
    pub font_size: f32,
    /// Offset from the panel's top-left corner at scale 1.0.
    pub offset: Vec2,
    pub align: Align2,
    pub font: TextFont,
    pub outline: Stroke,
}

#[derive(Clone, Copy)]
pub(super) struct CellStyle {
    pub offset: Vec2,
    pub panel: PanelStyle,
}

#[derive(Clone, Copy)]
pub(super) struct RowRule {
    pub color: Color32,
    pub width: f32,
    pub top_offset: f32,
    pub inset: f32,
}

pub(super) struct TimingCellStyle {
    pub size: Vec2,
    pub offset: Vec2,
    pub neutral: Color32,
    pub gaining: Color32,
    pub losing: Color32,
    pub neutral_text: Color32,
    pub gaining_text: Color32,
    pub losing_text: Color32,
    pub corner_radius: f32,
}

pub(super) const PANEL: PanelStyle = PanelStyle {
    size: Vec2::new(230.0, 76.0),
    background: Color32::from_rgba_unmultiplied_const(12, 18, 28, 210),
    border_color: Color32::WHITE,
    border_width: 0.0,
    corner_radius: 6.0,
};
