//! Reference-based GT7 appearance. All colors, cell geometry and type sizes live here.

use super::{
    delta_style::{DeltaStyle, ReferenceTimeStripStyle},
    gadget_style::{CellStyle, PanelStyle, TextFont, TextStyle, TimingCellStyle},
    gap_style::{DriverRowStyle, GapStyle},
    radar_style::{
        self, MarkerShape, ProximityStyle, RadarCarStyle, RadarStyle, RadialBackgroundStyle,
        SideWarningStyle,
    },
};
use eframe::egui::{Align2, Color32, Stroke, Vec2};

const WHITE: Color32 = Color32::from_rgb(249, 250, 255);
const MUTED: Color32 = Color32::from_rgb(216, 221, 248);
const BACKDROP: Color32 = Color32::TRANSPARENT;
/// Grey-black cell backing at 30% opacity (77 / 255).
const CELL_BACKGROUND: Color32 = Color32::from_rgba_unmultiplied_const(24, 24, 24, 77);
pub(super) const GAIN_TEXT: Color32 = Color32::from_rgb(112, 189, 255);
const LOSS_TEXT: Color32 = Color32::from_rgb(255, 134, 145);
const BLUE: Color32 = Color32::from_rgb(20, 98, 210);
const RED: Color32 = Color32::from_rgb(223, 20, 45);
const POSITION_BORDER: Color32 = Color32::from_rgb(147, 154, 182);
const PANEL: PanelStyle = PanelStyle {
    size: Vec2::new(320.0, 106.0),
    background: BACKDROP,
    border_color: Color32::TRANSPARENT,
    border_width: 0.0,
    corner_radius: 0.0,
};
const CELL: PanelStyle = PanelStyle {
    size: Vec2::new(40.0, 36.0),
    background: CELL_BACKGROUND,
    ..PANEL
};
const BODY: TextStyle = TextStyle {
    color: WHITE,
    font_size: 16.0,
    offset: Vec2::ZERO,
    align: Align2::LEFT_TOP,
    font: TextFont::Body,
    outline: Stroke::NONE,
};
const NUMERIC: TextStyle = TextStyle {
    font: TextFont::Numeric,
    outline: Stroke {
        width: 0.8,
        color: Color32::BLACK,
    },
    ..BODY
};

const PLAYER: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(185, 42, 25),
    border_color: Color32::from_rgb(245, 167, 140),
    border_width: 0.8,
};
const NEARBY: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(43, 115, 159),
    border_color: Color32::from_rgb(152, 208, 239),
    ..PLAYER
};
const ALONGSIDE: RadarCarStyle = RadarCarStyle { ..NEARBY };
const CONTACT: RadarCarStyle = RadarCarStyle { ..NEARBY };
const UNCERTAIN: RadarCarStyle = RadarCarStyle {
    fill: Color32::from_rgb(119, 128, 140),
    border_color: MUTED,
    ..PLAYER
};

pub(super) const RADAR: RadarStyle = RadarStyle {
    player: &PLAYER,
    nearby: &NEARBY,
    alongside: &ALONGSIDE,
    potential_contact: &CONTACT,
    uncertain: &UNCERTAIN,
    marker: MarkerShape::Arrow,
    fill_brightness: 1.0,
    panel: PanelStyle {
        size: Vec2::new(320.0, 320.0),
        ..PANEL
    },
    grid_color: Color32::from_rgba_unmultiplied_const(217, 224, 244, 150),
    grid_width: 0.8,
    center_band: Color32::TRANSPARENT,
    heading: Some(TextStyle {
        font_size: 18.0,
        offset: Vec2::new(0.0, 258.0),
        align: Align2::CENTER_TOP,
        outline: Stroke {
            width: 0.25,
            color: WHITE,
        },
        ..BODY
    }),
    proximity: Some(ProximityStyle {
        origin: Vec2::new(0.5, 0.38),
        guide_center: Vec2::new(0.5, 0.30),
        guide_radius: 0.44,
        ring_fractions: [0.375, 0.68, 1.0],
        top: 0.18,
        bottom: 0.78,
        side_inset: 0.10,
        background: Some(RadialBackgroundStyle {
            center_color: Color32::BLACK,
            edge_color: Color32::from_rgb(96, 96, 96),
            center_opacity: 0.40,
            edge_opacity: 0.0,
        }),
        side_warnings: Some(SideWarningStyle {
            inner_radius: 0.68,
            contact_inner_radius: 0.375,
            half_angle: 0.55,
            fill_opacity: 0.25,
            border_width: 2.0,
            center_tolerance_m: 0.3,
            nearby: Color32::from_rgb(236, 175, 63),
            alongside: Color32::from_rgb(255, 195, 75),
            contact: RED,
            uncertain: MUTED,
        }),
        marker_size: Vec2::new(14.0, 18.0),
        heading: "Radar",
    }),
    player_label: None,
    paused: TextStyle {
        font: TextFont::Body,
        color: WHITE,
        ..radar_style::CLASSIC.paused
    },
    hint: TextStyle {
        font: TextFont::Body,
        color: MUTED,
        ..radar_style::CLASSIC.hint
    },
};

pub(super) const AHEAD: GapStyle = GapStyle {
    panel: PANEL,
    heading: TextStyle {
        font_size: 11.0,
        offset: Vec2::new(10.0, 5.0),
        ..BODY
    },
    driver: TextStyle {
        offset: Vec2::new(60.0, 32.0),
        ..BODY
    },
    status: TextStyle {
        font_size: 10.0,
        color: MUTED,
        offset: Vec2::new(10.0, 89.0),
        ..BODY
    },
    value: Some(TextStyle {
        offset: Vec2::new(302.0, 64.0),
        align: Align2::RIGHT_TOP,
        ..BODY
    }),
    rule: None,
    driver_row: Some(DriverRowStyle {
        position_cell: CellStyle {
            offset: Vec2::new(10.0, 22.0),
            panel: PanelStyle {
                border_color: POSITION_BORDER,
                border_width: 1.5,
                ..CELL
            },
        },
        driver_cell: CellStyle {
            offset: Vec2::new(52.0, 22.0),
            panel: PanelStyle {
                size: Vec2::new(258.0, 36.0),
                ..CELL
            },
        },
        gap_cell: CellStyle {
            offset: Vec2::new(52.0, 60.0),
            panel: PanelStyle {
                size: Vec2::new(258.0, 24.0),
                background: Color32::TRANSPARENT,
                ..CELL
            },
        },
        gap_fade_from: BACKDROP,
        gap_fade_to: CELL_BACKGROUND,
        position: TextStyle {
            offset: Vec2::new(30.0, 40.0),
            align: Align2::CENTER_CENTER,
            ..BODY
        },
    }),
};

pub(super) const BEHIND: GapStyle = GapStyle { ..AHEAD };

pub(super) const DELTA: DeltaStyle = DeltaStyle {
    panel: PanelStyle {
        size: Vec2::new(440.0, 132.0),
        ..PANEL
    },
    label: None,
    heading: TextStyle {
        font_size: 20.0,
        offset: Vec2::new(375.0, 47.0),
        align: Align2::CENTER_CENTER,
        ..BODY
    },
    gaining_color: BLUE,
    losing_color: RED,
    value_cell: Some(TimingCellStyle {
        size: Vec2::new(110.0, 44.0),
        offset: Vec2::new(320.0, 25.0),
        neutral: CELL_BACKGROUND,
        gaining: GAIN_TEXT,
        losing: LOSS_TEXT,
        neutral_text: Color32::WHITE,
        gaining_text: Color32::WHITE,
        losing_text: Color32::WHITE,
        corner_radius: 3.0,
    }),
    reference: TextStyle {
        font_size: 27.0,
        offset: Vec2::new(163.5, 47.0),
        align: Align2::CENTER_CENTER,
        ..NUMERIC
    },
    estimate: TextStyle {
        font_size: 32.0,
        offset: Vec2::new(10.0, 94.0),
        ..NUMERIC
    },
    estimate_label: Some(TextStyle {
        font_size: 11.0,
        color: MUTED,
        offset: Vec2::new(10.0, 77.0),
        ..BODY
    }),
    status: None,
    trend: None,
    rule: None,
    reference_time_strip: Some(ReferenceTimeStripStyle {
        time_cell: CellStyle {
            offset: Vec2::new(10.0, 25.0),
            panel: PanelStyle {
                size: Vec2::new(307.0, 44.0),
                background: CELL_BACKGROUND,
                corner_radius: 3.0,
                ..CELL
            },
        },
        heading: TextStyle {
            font_size: 11.0,
            offset: Vec2::new(10.0, 6.0),
            ..BODY
        },
    }),
};
