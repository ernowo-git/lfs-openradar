//! Caller-facing state of the grouped HUD; compositor observations enter here.
use crate::config::{Config, GadgetPlacement};
use crate::{
    demo::Demo,
    runtime::{Snapshot, SnapshotReader},
};
use eframe::egui::{Pos2, Rect, Vec2};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub enum SnapshotSource {
    Disconnected,
    Live(SnapshotReader),
    Demo(Arc<Mutex<Demo>>),
}
impl SnapshotSource {
    pub(crate) fn snapshot(&self, now: u64, config: &Config) -> Snapshot {
        match self {
            Self::Disconnected => Snapshot::default(),
            Self::Live(reader) => reader.snapshot(),
            Self::Demo(demo) => demo.lock().unwrap().snapshot(now, config),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gadget {
    Radar,
    Ahead,
    Behind,
    Delta,
    Dashboard,
    Fuel,
}

impl Gadget {
    pub(crate) fn window_settings_mut(
        self,
        config: &mut Config,
    ) -> Option<&mut crate::config::GapSettings> {
        match self {
            Self::Radar => None,
            Self::Ahead => Some(&mut config.gap_ahead),
            Self::Behind => Some(&mut config.gap_behind),
            Self::Delta => Some(&mut config.performance_delta),
            Self::Dashboard => Some(&mut config.speed_dashboard),
            Self::Fuel => Some(&mut config.fuel),
        }
    }
    pub fn placement(self, config: &Config) -> &GadgetPlacement {
        match self {
            Self::Radar => &config.wayland.radar,
            Self::Ahead => &config.wayland.gap_ahead,
            Self::Behind => &config.wayland.gap_behind,
            Self::Delta => &config.wayland.performance_delta,
            Self::Dashboard => &config.wayland.speed_dashboard,
            Self::Fuel => &config.wayland.fuel,
        }
    }
    pub fn placement_mut(self, config: &mut Config) -> &mut GadgetPlacement {
        match self {
            Self::Radar => &mut config.wayland.radar,
            Self::Ahead => &mut config.wayland.gap_ahead,
            Self::Behind => &mut config.wayland.gap_behind,
            Self::Delta => &mut config.wayland.performance_delta,
            Self::Dashboard => &mut config.wayland.speed_dashboard,
            Self::Fuel => &mut config.wayland.fuel,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GadgetGeometry {
    pub gadget: Gadget,
    pub rect: Rect,
    pub scale: f32,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Monitor {
    pub name: String,
    pub size: [f32; 2],
    pub scale: f32,
}

#[derive(Clone, Debug, Default)]
pub struct OverlayStatus {
    pub ready: bool,
    pub selected_monitor: Option<String>,
    pub visible: bool,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub monitors: Vec<Monitor>,
    pub logical_size: Vec2,
    pub physical_size: [u32; 2],
    pub gadgets: Vec<GadgetGeometry>,
}

#[derive(Clone)]
pub struct OverlaySession {
    pub(crate) config: Config,
    pub(crate) show: bool,
    error: Option<String>,
    outputs: Vec<Monitor>,
    ready: bool,
}

impl OverlaySession {
    /// Paint the current grouped HUD through the same presentation interface
    /// used by the GPU adapter, with native displayed scales and one snapshot.
    pub fn paint(
        &self,
        context: &eframe::egui::Context,
        snapshot: &Snapshot,
    ) -> eframe::egui::FullOutput {
        use super::{
            fonts, fuel_style,
            render::{GapPaintOptions, paint, paint_delta, paint_gap},
            speed_dashboard, theme,
        };
        use eframe::egui;
        let key = egui::Id::new("openradar-native-fonts");
        if !context.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) {
            fonts::install(context);
            context.set_visuals(egui::Visuals::dark());
            context.data_mut(|d| d.insert_temp(key, true));
        }
        let status = self.status();
        let scale = status
            .monitors
            .iter()
            .find(|m| Some(&m.name) == status.selected_monitor.as_ref())
            .map_or(1.0, |m| m.scale);
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, status.logical_size)),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(scale);
        let mut config = self.config.clone();
        for geometry in &status.gadgets {
            if let Some(settings) = geometry.gadget.window_settings_mut(&mut config) {
                settings.scale = geometry.scale;
            }
        }
        context.run(input, |ctx| {
            if !status.visible {
                return;
            }
            let painter = ctx.layer_painter(egui::LayerId::background());
            let theme = theme::resolve(config.hud_style);
            for geometry in &status.gadgets {
                if !geometry.enabled {
                    continue;
                }
                let painter = painter.with_clip_rect(geometry.rect);
                match geometry.gadget {
                    Gadget::Radar => {
                        paint(&painter, geometry.rect, &snapshot.frame, &config, false)
                    }
                    Gadget::Ahead => paint_gap(
                        &painter,
                        geometry.rect,
                        "AHEAD",
                        &snapshot.gaps.ahead,
                        &GapPaintOptions::new(&config.gap_ahead, config.hud_debug),
                        theme.ahead,
                    ),
                    Gadget::Behind => paint_gap(
                        &painter,
                        geometry.rect,
                        "BEHIND",
                        &snapshot.gaps.behind,
                        &GapPaintOptions::new(&config.gap_behind, config.hud_debug),
                        theme.behind,
                    ),
                    Gadget::Delta => paint_delta(
                        &painter,
                        geometry.rect,
                        &snapshot.delta,
                        &config.performance_delta,
                        theme.delta,
                    ),
                    Gadget::Dashboard => speed_dashboard::paint(
                        &painter,
                        geometry.rect,
                        &snapshot.dashboard,
                        &config,
                    ),
                    Gadget::Fuel => fuel_style::paint(&painter, geometry.rect, &snapshot.fuel),
                }
            }
        })
    }
    pub fn new(config: Config, show: bool) -> Self {
        Self {
            config,
            show,
            error: None,
            outputs: Vec::new(),
            ready: false,
        }
    }
    pub fn update(&mut self, mut config: Config, show: bool) {
        if config.wayland.monitor.is_none() {
            config.wayland.monitor = self.config.wayland.monitor.clone();
        }
        if self.config.wayland.monitor != config.wayland.monitor {
            self.ready = false;
        }
        self.config = config;
        self.show = show;
    }
    pub fn failure(&mut self, error: String) {
        self.error = Some(error);
        self.ready = false;
    }
    pub fn retry(&mut self) {
        self.error = None;
        self.ready = false;
    }
    #[cfg(all(target_os = "linux", feature = "native-wayland"))]
    pub(crate) fn presented(&mut self) {
        self.ready = true;
    }
    #[cfg(all(target_os = "linux", feature = "native-wayland"))]
    pub(crate) fn closed(&mut self) {
        self.ready = false;
        self.show = false;
    }
    pub fn outputs_changed(&mut self, outputs: Vec<Monitor>) {
        if self.config.wayland.monitor.is_none() {
            self.config.wayland.monitor = outputs.first().map(|output| output.name.clone());
        }
        self.outputs = outputs;
    }
    pub fn config_for_save(&self) -> Config {
        let status = self.status();
        let mut config = self.config.clone();
        config.wayland.monitor = status.selected_monitor;
        for geometry in status.gadgets {
            let placement = geometry.gadget.placement_mut(&mut config);
            placement.x = Some(geometry.rect.min.x);
            placement.y = Some(geometry.rect.min.y);
        }
        config
    }
    pub fn status(&self) -> OverlayStatus {
        let selected = self
            .config
            .wayland
            .monitor
            .clone()
            .or_else(|| self.outputs.first().map(|m| m.name.clone()));
        let monitor = self
            .outputs
            .iter()
            .find(|m| Some(&m.name) == selected.as_ref());
        let available = monitor.is_some();
        let logical_size = monitor.map_or(Vec2::ZERO, |m| Vec2::from(m.size));
        let scale = monitor.map_or(1.0, |m| m.scale);
        let theme = super::theme::resolve(self.config.hud_style);
        let sizes = [
            theme.radar.panel.size,
            theme.ahead.panel.size,
            theme.behind.panel.size,
            theme.delta.panel.size,
            super::speed_dashboard::SIZE,
            super::fuel_style::SIZE,
        ];
        let enabled_flags = [
            self.config.radar_enabled,
            self.config.gap_ahead.enabled,
            self.config.gap_behind.enabled,
            self.config.performance_delta.enabled,
            self.config.speed_dashboard.enabled,
            self.config.fuel.enabled,
        ];
        let mut gadgets = Vec::new();
        if available {
            for (index, gadget) in [
                Gadget::Radar,
                Gadget::Ahead,
                Gadget::Behind,
                Gadget::Delta,
                Gadget::Dashboard,
                Gadget::Fuel,
            ]
            .into_iter()
            .enumerate()
            {
                let placement = gadget.placement(&self.config);
                let chosen = if gadget == Gadget::Radar {
                    Vec2::splat(320.0)
                } else {
                    sizes[index]
                };
                let fitted_scale = placement
                    .scale
                    .min(logical_size.x / chosen.x)
                    .min(logical_size.y / chosen.y);
                let size = chosen * fitted_scale;
                let defaults = match gadget {
                    Gadget::Radar => logical_size - size - Vec2::splat(24.0),
                    Gadget::Ahead => Vec2::splat(24.0),
                    Gadget::Behind => Vec2::new(
                        24.0,
                        24.0 + sizes[1].y * self.config.wayland.gap_ahead.scale + 8.0,
                    ),
                    Gadget::Delta => Vec2::new((logical_size.x - size.x) / 2.0, 24.0),
                    Gadget::Dashboard => Vec2::new(
                        (logical_size.x - size.x) / 2.0,
                        logical_size.y - size.y - 24.0,
                    ),
                    Gadget::Fuel => Vec2::new(24.0, logical_size.y - size.y - 24.0),
                };
                let position = Pos2::new(
                    placement
                        .x
                        .unwrap_or(defaults.x)
                        .clamp(0.0, (logical_size.x - size.x).max(0.0)),
                    placement
                        .y
                        .unwrap_or(defaults.y)
                        .clamp(0.0, (logical_size.y - size.y).max(0.0)),
                );
                gadgets.push(GadgetGeometry {
                    gadget,
                    rect: Rect::from_min_size(position, size),
                    scale: fitted_scale,
                    enabled: enabled_flags[index],
                });
            }
        }
        let enabled = self.config.radar_enabled
            || self.config.gap_ahead.enabled
            || self.config.gap_behind.enabled
            || self.config.performance_delta.enabled
            || self.config.speed_dashboard.enabled
            || self.config.fuel.enabled;
        OverlayStatus {
            ready: self.ready && available && self.error.is_none(),
            selected_monitor: selected,
            visible: self.show && enabled && available && self.error.is_none(),
            warning: (!available)
                .then(|| "selected monitor unavailable; choose a connected monitor".into()),
            error: self.error.clone(),
            monitors: self.outputs.clone(),
            logical_size,
            physical_size: [
                (logical_size.x * scale).ceil() as u32,
                (logical_size.y * scale).ceil() as u32,
            ],
            gadgets,
        }
    }
}
