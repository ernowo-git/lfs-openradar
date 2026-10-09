use super::{
    render::{GapPaintOptions, delta_value, paint, paint_delta, paint_gap},
    theme,
    window::{GapWindow, OverlayWindow},
};
use crate::{
    config::{Config, GapSettings, HudStyle},
    demo::Demo,
    runtime::{Runtime, Snapshot, SnapshotReader},
};
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2, ViewportBuilder, ViewportId};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const PANEL_MARGIN: f32 = 20.0;
const CARD_WIDTH: f32 = 300.0;
const CARD_HEIGHT: f32 = 464.0;
const CARD_SPACING: f32 = 12.0;

fn initial_panel_size() -> Vec2 {
    Vec2::new(
        GADGETS.len() as f32 * CARD_WIDTH
            + (GADGETS.len() - 1) as f32 * CARD_SPACING
            + PANEL_MARGIN * 2.0,
        CARD_HEIGHT + 180.0,
    )
}

pub fn run(
    config: Config,
    config_path: PathBuf,
    demo: bool,
    seconds: Option<u64>,
    screenshot: Option<PathBuf>,
) -> Result<(), String> {
    super::graphics_log::init(&config_path);
    let wgpu_setup = eframe::egui_wgpu::WgpuSetupCreateNew::default();
    #[cfg(windows)]
    let wgpu_setup = {
        let mut setup = wgpu_setup;
        // The ordinary DXGI HWND swapchain is opaque. DirectComposition's
        // visual swapchain supports alpha compositing for the radar window.
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
        setup
            .instance_descriptor
            .backend_options
            .dx12
            .presentation_system = wgpu::wgt::Dx12SwapchainKind::DxgiFromVisual;
        log::info!(target: "openradar_graphics", "Windows presentation: DX12 DirectComposition; transparent backbuffers enabled");
        setup
    };
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("LFS OpenRadar · Control panel")
            .with_inner_size(initial_panel_size())
            .with_min_inner_size([380.0, 400.0])
            // eframe derives the shared painter's alpha support from the root
            // viewport, even when only a child needs a transparent background.
            // The control panel still paints its own opaque central frame.
            .with_transparent(true),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: wgpu_setup.into(),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "lfs-openradar",
        options,
        Box::new(move |cc| {
            if let Some(state) = &cc.wgpu_render_state {
                let info = state.adapter.get_info();
                log::info!(target: "openradar_graphics", "GPU {} · backend {:?} · driver {} {}", info.name, info.backend, info.driver, info.driver_info);
            }
            super::fonts::install(&cc.egui_ctx);
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let mut app = App::new(
                config,
                config_path,
                demo,
                seconds,
                screenshot,
            );
            app.start_hotkey(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())
}

struct App {
    config: Config,
    config_path: PathBuf,
    runtime: Option<Runtime>,
    demo: Option<Demo>,
    started: Instant,
    seconds: Option<u64>,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    message: Option<String>,
    setup_message: Option<String>,
    setup_plan: Option<crate::setup::SetupPlan>,
    folder_picker: Option<super::folder_picker::FolderPicker>,
    overlay_enabled: bool,
    hotkey: Option<super::hotkey::OverlayHotkey>,
    hotkey_error: Option<String>,
    edit_overlay: bool,
    overlay_created: bool,
    overlay_window: OverlayWindow,
    overlay_feedback: Arc<Mutex<OverlayFeedback>>,
    overlay_data: Arc<Mutex<OverlayData>>,
    gap_overlays: [GapOverlay; 5],
    tab: ControlTab,
    opening_layout_passes: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlTab {
    Gadgets,
    Settings,
}

#[derive(Clone)]
enum OverlaySource {
    Disconnected,
    Live(SnapshotReader),
    Demo(Arc<Mutex<Demo>>),
}
impl OverlaySource {
    fn snapshot(&self, now: u64, config: &Config) -> Snapshot {
        match self {
            Self::Disconnected => Snapshot::default(),
            Self::Live(reader) => reader.snapshot(),
            Self::Demo(demo) => demo.lock().unwrap().snapshot(now, config),
        }
    }
}

#[derive(Clone)]
struct OverlayData {
    source: OverlaySource,
    config: Config,
    started: Instant,
    control_painted_at: Instant,
    visible: bool,
    editing: bool,
}

#[derive(Default)]
struct OverlayFeedback {
    observed_position: Option<Pos2>,
    moved_to: Option<Pos2>,
    closed: bool,
    panel_stall_reported: bool,
}
struct GapOverlay {
    window: GapWindow,
    data: Arc<Mutex<OverlayData>>,
    feedback: Arc<Mutex<OverlayFeedback>>,
    created: bool,
    editing: bool,
}
impl GapOverlay {
    fn new(settings: &GapSettings, data: &OverlayData) -> Self {
        Self {
            window: GapWindow::new(settings),
            data: Arc::new(Mutex::new(data.clone())),
            feedback: Arc::new(Mutex::new(OverlayFeedback::default())),
            created: false,
            editing: false,
        }
    }
}
impl App {
    fn new(
        mut config: Config,
        config_path: PathBuf,
        demo: bool,
        seconds: Option<u64>,
        screenshot: Option<PathBuf>,
    ) -> Self {
        config.prepare_gap_positions();
        config.insim_password.0 = config.effective_insim_password();
        let started = Instant::now();
        let overlay_data = OverlayData {
            source: if demo {
                OverlaySource::Demo(Arc::new(Mutex::new(Demo::default())))
            } else {
                OverlaySource::Disconnected
            },
            config: config.clone(),
            started,
            control_painted_at: started,
            visible: false,
            editing: false,
        };
        let gap_overlays = [
            GapOverlay::new(&config.gap_ahead, &overlay_data),
            GapOverlay::new(&config.gap_behind, &overlay_data),
            GapOverlay::new(&config.performance_delta, &overlay_data),
            GapOverlay::new(&config.speed_dashboard, &overlay_data),
            GapOverlay::new(&config.fuel, &overlay_data),
        ];
        let mut app = Self {
            overlay_data: Arc::new(Mutex::new(overlay_data)),
            gap_overlays,
            overlay_window: OverlayWindow::new(&config),
            overlay_feedback: Arc::new(Mutex::new(OverlayFeedback::default())),
            overlay_created: false,
            config,
            config_path,
            runtime: None,
            demo: demo.then(Demo::default),
            started,
            seconds,
            screenshot,
            screenshot_requested: false,
            message: None,
            setup_message: None,
            setup_plan: None,
            folder_picker: None,
            overlay_enabled: demo,
            hotkey: None,
            hotkey_error: None,
            edit_overlay: false,
            tab: ControlTab::Gadgets,
            opening_layout_passes: 0,
        };
        if !demo {
            app.connect();
        }
        app
    }
    fn start_hotkey(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        match self.config.overlay_toggle_key() {
            Ok(Some(key)) => {
                match super::hotkey::OverlayHotkey::start(key.virtual_key, ctx.clone()) {
                    Ok(hotkey) => self.hotkey = Some(hotkey),
                    Err(error) => {
                        log::error!(target: "openradar_graphics", "{error}");
                        self.hotkey_error = Some(error);
                    }
                }
            }
            Ok(None) => {}
            Err(error) => self.hotkey_error = Some(error),
        }
        #[cfg(not(windows))]
        let _ = ctx;
    }
    fn toggle_overlays(&mut self) {
        self.overlay_enabled = !self.overlay_enabled;
        log::info!(target: "openradar_graphics", "Show overlay {}", self.overlay_enabled);
    }
    fn process_hotkey(&mut self, ctx: &egui::Context) {
        let configured_key = self
            .config
            .overlay_toggle_key()
            .ok()
            .flatten()
            .and_then(|key| egui::Key::from_name(&key.name));
        let pressed = self
            .hotkey
            .as_ref()
            .is_some_and(|hotkey| hotkey.take_toggle());
        // Other desktop backends can use the shortcut while the panel has focus.
        #[cfg(not(windows))]
        let pressed = pressed
            || configured_key.is_some_and(|key| {
                ctx.input(|input| {
                    input.events.iter().any(|event| {
                        matches!(event,
                egui::Event::Key { key: pressed_key, pressed: true, repeat: false, modifiers, .. }
                    if *pressed_key == key && modifiers.is_none())
                    })
                })
            });
        if let Some(key) = configured_key {
            // A configured Space/Enter shortcut must not also activate the
            // focused checkbox. Only panel input is consumed; LFS keeps its key.
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
        }
        if pressed {
            self.toggle_overlays();
            ctx.request_repaint();
        }
    }
    fn connect(&mut self) {
        // Drop joins the old worker and releases its UDP port before restarting.
        self.set_source(OverlaySource::Disconnected);
        self.runtime = None;
        match Runtime::start_with_password(
            self.config.clone(),
            self.config.insim_password.0.clone(),
        ) {
            Ok(runtime) => {
                self.set_source(OverlaySource::Live(runtime.snapshot_reader()));
                self.runtime = Some(runtime);
                self.message = None;
            }
            Err(error) => self.message = Some(error),
        }
    }
    fn apply_startup_setup(&mut self, plan: crate::setup::SetupPlan) {
        self.setup_message = Some(match crate::setup::apply(&plan) {
            Ok(result) if !result.changed => format!(
                "InSim is already configured on port {} in {}. Restart LFS if the listener is not running.",
                plan.target_port,
                result.script_path.display()
            ),
            Ok(result) => {
                let backup = result
                    .backup_path
                    .map(|p| format!(" Backup: {}.", p.display()))
                    .unwrap_or_default();
                format!(
                    "Enabled InSim on port {} in {}.{} Restart LFS to use it.",
                    plan.target_port,
                    result.script_path.display(),
                    backup
                )
            }
            Err(error) => error,
        });
        self.setup_plan = None;
    }
    fn apply_outsim_setup(&mut self) {
        self.setup_plan = None;
        if self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.snapshot().connected)
        {
            self.setup_message = Some(
                "Close LFS before configuring OutSim, then click Configure OutSim again.".into(),
            );
            return;
        }
        self.setup_message = Some(
            match crate::setup::prepare_outsim(
                std::path::Path::new(self.config.lfs_directory.trim()),
                &self.config,
            )
            .and_then(|plan| crate::setup::apply_outsim(&plan))
            {
                Ok(result) if !result.changed => format!(
                    "OutSim already matches OpenRadar in {}. Start LFS to use it.",
                    result.cfg_path.display()
                ),
                Ok(result) => format!(
                    "Configured OutSim in {}. Backup: {}. Start LFS to use it.",
                    result.cfg_path.display(),
                    result.backup_path.unwrap().display()
                ),
                Err(error) => error,
            },
        );
    }
    fn startup_setup_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("LFS startup setup").default_open(true).show(ui, |ui| {
            ui.label("Select your LFS installation to set up InSim and OutSim for OpenRadar.");
            ui.horizontal(|ui| {
                ui.label("LFS folder");
                if ui.add(egui::TextEdit::singleline(&mut self.config.lfs_directory)
                    .hint_text("Folder containing LFS.exe").desired_width((ui.available_width() - 100.0).clamp(80.0, 360.0))).changed() {
                    self.setup_plan = None;
                    self.setup_message = None;
                }
                if ui.button("Browse…").clicked() {
                    self.setup_plan = None;
                    self.folder_picker = Some(super::folder_picker::FolderPicker::new(
                        PathBuf::from(self.config.lfs_directory.trim())));
                }
            });
            ui.label(format!("Startup command: /insim {}", self.config.insim_address.port()));
            ui.label(egui::RichText::new("Existing commands are preserved. A backup is saved before any edit.").small());
            if ui.add_enabled(!self.config.lfs_directory.trim().is_empty(),
                egui::Button::new("Enable InSim at startup")).clicked() {
                self.setup_message = None;
                self.setup_plan = None;
                match crate::setup::prepare(std::path::Path::new(self.config.lfs_directory.trim()),
                    self.config.insim_address.port()) {
                    Ok(plan) if plan.has_conflict() => self.setup_plan = Some(plan),
                    Ok(plan) => self.apply_startup_setup(plan),
                    Err(error) => self.setup_message = Some(error),
                }
            }
            if let Some(plan) = &self.setup_plan {
                let ports = plan.existing_ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
                let existing = (plan.existing_ports.len() == 1).then(|| plan.existing_ports[0]).filter(|p| *p != 0);
                let target = plan.target_port;
                ui.colored_label(Color32::YELLOW, format!(
                    "{} already configures InSim port(s): {ports}. OpenRadar uses {target}.",
                    plan.script_path.display()));
                let mut replace = false;
                let mut keep = false;
                let mut cancel = false;
                ui.horizontal_wrapped(|ui| {
                    replace = ui.button(format!("Replace with OpenRadar port {target}")).clicked();
                    if let Some(port) = existing { keep = ui.button(format!("Use existing port {port}")).clicked(); }
                    cancel = ui.button("Cancel").clicked();
                });
                if replace {
                    let plan = self.setup_plan.take().unwrap();
                    self.apply_startup_setup(plan);
                } else if keep {
                    self.setup_plan = None;
                    // Validate the inspected script again before adopting its port.
                    match crate::setup::prepare(
                        std::path::Path::new(self.config.lfs_directory.trim()), existing.unwrap()) {
                        Ok(plan) if !plan.changes_script() => {
                            self.config.insim_address.set_port(existing.unwrap());
                            if self.demo.is_none() { self.connect(); }
                            self.setup_message = Some(format!(
                                "OpenRadar now uses port {}. Save settings to keep it. Restart LFS if needed.", existing.unwrap()));
                        }
                        Ok(_) => self.setup_message = Some("The script changed; inspect it again before adopting a port".into()),
                        Err(error) => self.setup_message = Some(error),
                    }
                } else if cancel { self.setup_plan = None; }
            }
            ui.add_space(8.0);
            ui.label("Close LFS before configuring OutSim. Start LFS again after setup.");
            if ui.add_enabled(!self.config.lfs_directory.trim().is_empty(),
                egui::Button::new("Configure OutSim")).clicked() {
                self.apply_outsim_setup();
            }
            ui.label(egui::RichText::new("Updates OutSim in cfg.txt to match OpenRadar. Other settings are preserved; cfg.txt.BAK is saved before changes.").small());
            if ui.add_enabled(!self.config.lfs_directory.trim().is_empty(), egui::Button::new("Configure OutGauge")).clicked() {
                self.setup_message = Some(if self.runtime.as_ref().is_some_and(|runtime| runtime.snapshot().connected) {
                    "Close LFS before configuring OutGauge, then start it again after setup.".into()
                } else {
                    match crate::setup::prepare_outgauge(std::path::Path::new(self.config.lfs_directory.trim()), &self.config)
                        .and_then(|plan| crate::setup::apply_outsim(&plan)) {
                        Ok(result) => format!("OutGauge configured in {}. Start LFS and enable Speed dashboard, Fuel, or OutGauge forwarding.", result.cfg_path.display()),
                        Err(error) => error,
                    }
                });
            }
            ui.label(egui::RichText::new("OutGauge supplies Speed dashboard, Fuel, and telemetry forwarding through one UDP receiver. A backup is saved before changing cfg.txt.").small());
            if let Some(message) = &self.setup_message { ui.label(message); }
            ui.label(egui::RichText::new("Save settings remembers the selected LFS folder.").small());
        });
    }
    fn set_source(&self, source: OverlaySource) {
        self.overlay_data.lock().unwrap().source = source.clone();
        for gap in &self.gap_overlays {
            gap.data.lock().unwrap().source = source.clone();
        }
    }
    fn receive_overlay_feedback(&mut self) {
        if let Ok(mut feedback) = self.overlay_feedback.lock() {
            if let Some(position) = feedback.moved_to.take() {
                self.config.overlay_x = position.x;
                self.config.overlay_y = position.y;
            }
            if feedback.closed {
                self.config.radar_enabled = false;
                self.edit_overlay = false;
                feedback.closed = false;
            }
        }
        for (gap, settings) in self.gap_overlays.iter_mut().zip([
            &mut self.config.gap_ahead,
            &mut self.config.gap_behind,
            &mut self.config.performance_delta,
            &mut self.config.speed_dashboard,
            &mut self.config.fuel,
        ]) {
            if let Ok(mut feedback) = gap.feedback.lock() {
                if let Some(position) = feedback.moved_to.take() {
                    settings.window_x = Some(position.x);
                    settings.window_y = Some(position.y);
                }
                if feedback.closed {
                    settings.enabled = false;
                    gap.editing = false;
                    feedback.closed = false;
                }
            }
        }
    }
    fn show_gap_overlays(&mut self, ctx: &egui::Context, foreground: bool) {
        let theme = theme::resolve(self.config.hud_style);
        for (index, gap) in self.gap_overlays.iter_mut().enumerate() {
            let (kind, settings, id, title) = if index == 0 {
                (
                    Gadget::Ahead,
                    &self.config.gap_ahead,
                    "gap-ahead-overlay",
                    "LFS OpenRadar · Gap ahead",
                )
            } else if index == 1 {
                (
                    Gadget::Behind,
                    &self.config.gap_behind,
                    "gap-behind-overlay",
                    "LFS OpenRadar · Gap behind",
                )
            } else if index == 2 {
                (
                    Gadget::Delta,
                    &self.config.performance_delta,
                    "performance-delta-overlay",
                    "LFS OpenRadar · Performance delta",
                )
            } else if index == 3 {
                (
                    Gadget::Dashboard,
                    &self.config.speed_dashboard,
                    "speed-dashboard-overlay",
                    "LFS OpenRadar · Speed dashboard",
                )
            } else {
                (
                    Gadget::Fuel,
                    &self.config.fuel,
                    "fuel-overlay",
                    "LFS OpenRadar · Fuel",
                )
            };
            let visible = overlay_visible(
                self.overlay_enabled,
                settings.enabled,
                gap.editing,
                self.demo.is_some(),
                self.config.hide_when_background,
                foreground,
            );
            if gap.window.settle_scale(
                settings.scale,
                self.started.elapsed(),
                ctx.input(|input| input.pointer.any_down()),
            ) {
                log::info!(target: "openradar_graphics", "Settled {id} scale {}", settings.scale);
            }
            gap.created |= visible;
            if !gap.created {
                continue;
            }
            let became_visible = {
                let mut data = gap.data.lock().unwrap();
                let became_visible = visible && !data.visible;
                data.config = self.config.clone();
                data.visible = visible;
                data.editing = gap.editing;
                data.control_painted_at = Instant::now();
                became_visible
            };
            let data = gap.data.clone();
            let feedback = gap.feedback.clone();
            let id = ViewportId::from_hash_of(id);
            ctx.show_viewport_deferred(
                id,
                gap.window.builder(
                    title,
                    visible,
                    gap.editing,
                    match kind {
                        Gadget::Dashboard => super::speed_dashboard::SIZE,
                        Gadget::Fuel => super::fuel_style::SIZE,
                        Gadget::Delta => theme.delta.panel.size,
                        Gadget::Ahead => theme.ahead.panel.size,
                        Gadget::Behind => theme.behind.panel.size,
                        Gadget::Radar => unreachable!(),
                    },
                ),
                move |child, _| {
                    render_gap_overlay(child, &data, &feedback, kind);
                },
            );
            if became_visible {
                ctx.request_repaint_of(id);
            }
        }
    }
    fn capture(&mut self, ctx: &egui::Context) {
        let preview_delay: f32 = if self.config.gap_ahead.enabled || self.config.gap_behind.enabled
        {
            6.0
        } else {
            1.0
        };
        let preview_delay = preview_delay.min(
            self.seconds
                .map_or(6.0, |seconds| seconds.saturating_sub(2).max(1) as f32),
        );
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && self.started.elapsed().as_secs_f32() > preview_delay
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.screenshot_requested = true;
        }
        let captured = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = captured
            && let Some(path) = self.screenshot.take()
        {
            let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            let result =
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels)
                    .ok_or_else(|| "Invalid screenshot buffer".to_string())
                    .and_then(|image| image.save(&path).map_err(|e| e.to_string()));
            match result {
                Ok(()) => eprintln!("Saved preview: {}", path.display()),
                Err(error) => {
                    eprintln!("Screenshot: {error}");
                    self.message = Some(error);
                }
            }
        }
    }
    fn control_panel(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        let header = egui::TopBottomPanel::top("control-header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(12, 18, 28))
                    .inner_margin(PANEL_MARGIN),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.heading("LFS OpenRadar");
                        ui.label(
                            egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                                .small()
                                .color(Color32::GRAY),
                        );
                    });
                    ui.add_space(12.0);
                    if ui
                        .add_enabled(self.demo.is_none(), egui::Button::new("Apply / reconnect"))
                        .clicked()
                    {
                        self.connect();
                    }
                    if ui.button("Save settings").clicked() {
                        self.message = Some(match self.config.save(&self.config_path) {
                            Ok(()) => format!("Saved {}", self.config_path.display()),
                            Err(error) => error,
                        });
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.add_space(8.0);
                    project_link(ui, ProjectLink::GitHub);
                    project_link(ui, ProjectLink::KoFi);
                });
                ui.horizontal_wrapped(|ui| {
                    if self.demo.is_some() {
                        ui.colored_label(
                            Color32::from_rgb(92, 204, 222),
                            "DEMO / NO NETWORK CONNECTION",
                        );
                    }
                    if !snapshot.frame.status.is_empty() {
                        ui.label(&snapshot.frame.status);
                    }
                    badge(ui, "MCI", snapshot.frame.mci_age_ms, self.config.stale_ms);
                    badge(
                        ui,
                        "OutSim",
                        snapshot.frame.outsim_age_ms,
                        self.config.stale_ms,
                    );
                    if let Some(driver) = &snapshot.frame.driver {
                        ui.label(driver);
                    }
                });
                if let Some(message) = self.message.as_ref().or(snapshot.error.as_ref()) {
                    ui.colored_label(Color32::from_rgb(255, 180, 120), message);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, ControlTab::Gadgets, "Gadgets");
                    ui.selectable_value(&mut self.tab, ControlTab::Settings, "Settings");
                });
            });
        let content = egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(12, 18, 28))
                    .inner_margin(PANEL_MARGIN),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("control-content")
                    .show(ui, |ui| match self.tab {
                        ControlTab::Gadgets => self.gadget_cards(ui, snapshot),
                        ControlTab::Settings => self.settings_controls(ui, snapshot),
                    })
                    .content_size
            });
        // Measure the real layout after egui's grid has settled. Fit once at
        // startup, leaving later user resizing and tab switches under user control.
        if self.opening_layout_passes == 0 {
            let current = ctx.input(|input| input.content_rect().size());
            let fitted =
                fit_panel_to_monitor(current, ctx.input(|input| input.viewport().monitor_size));
            if fitted != current {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(fitted));
            }
        }
        if self.opening_layout_passes < 4 {
            self.opening_layout_passes += 1;
            if self.opening_layout_passes == 4 {
                let current = ctx.input(|input| input.content_rect().size());
                let desired = Vec2::new(
                    current.x,
                    header.response.rect.height() + content.inner.y + PANEL_MARGIN * 2.0,
                );
                let fitted =
                    fit_panel_to_monitor(desired, ctx.input(|input| input.viewport().monitor_size));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(fitted.ceil()));
            }
        }
    }

    fn settings_controls(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        ui.heading("Overlay visibility");
        if ui
            .checkbox(&mut self.overlay_enabled, "Show overlays")
            .changed()
        {
            log::info!(target: "openradar_graphics", "Show overlay {}", self.overlay_enabled);
        }
        if let Ok(Some(key)) = self.config.overlay_toggle_key() {
            let scope = if cfg!(windows) {
                "also while driving"
            } else {
                "while this window has focus"
            };
            ui.label(format!("Toggle with {} ({scope}).", key.name));
        }
        if let Some(error) = &self.hotkey_error {
            ui.colored_label(Color32::YELLOW, error);
        }
        ui.checkbox(
            &mut self.config.hide_when_background,
            "Hide overlay when LFS is in background",
        );
        if super::game_foreground().is_none() {
            ui.label(
                egui::RichText::new(
                    "Foreground detection unavailable here; overlay visibility is manual.",
                )
                .small()
                .color(Color32::YELLOW),
            );
        }
        ui.add_space(12.0);
        ui.heading("Telemetry");
        if self.demo.is_none() {
            follow_view_controls(ui, &mut self.config);
        }
        ui.add(egui::Slider::new(&mut self.config.interpolation_ms, 0..=200).text("Interpolation (ms)"))
            .on_hover_text("Smooths radar motion by drawing slightly older telemetry between received updates. More delay can reduce jitter but makes the radar respond later. No positions are predicted.");
        ui.label("Interpolation smooths radar motion by delaying the display. Higher values can reduce jitter but add latency; 0 ms uses the latest shared telemetry time.");
        ui.label(
            egui::RichText::new(
                "Default: 60 ms. Use Apply / reconnect after changing this setting in live mode.",
            )
            .small(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("InSim password");
            ui.add(
                egui::TextEdit::singleline(&mut self.config.insim_password.0)
                    .password(true)
                    .hint_text("LFS multiplayer admin password"),
            );
        });
        if snapshot
            .error
            .as_deref()
            .into_iter()
            .chain(self.message.as_deref())
            .any(|error| error.contains("password"))
        {
            ui.colored_label(
                Color32::from_rgb(240, 178, 85),
                "Check \"Game Admin\" in your LFS folder's cfg.txt for the password.",
            );
        }
        ui.label(
            egui::RichText::new(
                "Save settings stores the password in TOML. Apply / reconnect uses it.",
            )
            .small(),
        );
        ui.add_space(12.0);
        ui.heading("OutGauge forwarding");
        let mut forwarding = !self.config.outgauge_forward.is_empty();
        if ui
            .checkbox(&mut forwarding, "Forward OutGauge to other apps")
            .changed()
        {
            self.config.outgauge_forward.clear();
            if forwarding {
                self.config.outgauge_forward.push(std::net::SocketAddr::new(
                    self.config.outgauge_bind.ip(),
                    60000,
                ));
            }
        }
        if forwarding {
            let mut remove = None;
            for (index, destination) in self.config.outgauge_forward.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Destination {} port", destination.ip()));
                        let mut port = destination.port();
                        ui.add(egui::DragValue::new(&mut port).range(1..=65535));
                        destination.set_port(port);
                        if ui.button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                });
            }
            if let Some(index) = remove {
                self.config.outgauge_forward.remove(index);
            }
            if ui.button("Add destination").clicked() {
                let port = self
                    .config
                    .outgauge_forward
                    .last()
                    .and_then(|address| address.port().checked_add(1))
                    .unwrap_or(60000);
                self.config.outgauge_forward.push(std::net::SocketAddr::new(
                    self.config.outgauge_bind.ip(),
                    port,
                ));
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("OutGauge ID");
                ui.add(egui::DragValue::new(&mut self.config.outgauge_id));
            });
            ui.label("All destinations receive the same packets and OutGauge ID.");
            ui.label("For MOZA, use port 60000 and OutGauge ID 0. Close LFS and click Configure OutGauge to match these settings.");
            ui.label("Forwarding works with gadgets disabled. Keep OpenRadar running.");
            if let Some(error) = snapshot
                .outgauge_forward_error
                .as_ref()
                .or(snapshot.outgauge_error.as_ref())
            {
                ui.colored_label(Color32::YELLOW, error);
            }
        }
        ui.label("Apply / reconnect to use forwarding changes, then Save settings to keep them.");
        ui.add_space(12.0);
        self.startup_setup_controls(ui);
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!(
                "MCI sets {} · OutSim {} · rejected {} · malformed {}",
                snapshot.mci_sets,
                snapshot.outsim_samples,
                snapshot.rejected_outsim,
                snapshot.malformed_packets
            ))
            .small(),
        );
    }

    fn gadget_cards(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let previous_style = self.config.hud_style;
        let previous_debug = self.config.hud_debug;
        ui.horizontal_wrapped(|ui| {
            ui.label("HUD style");
            egui::ComboBox::from_id_salt("hud-style")
                .selected_text(self.config.hud_style.label())
                .show_ui(ui, |ui| {
                    for style in [HudStyle::Classic, HudStyle::Gt7Inspired] {
                        ui.selectable_value(&mut self.config.hud_style, style, style.label());
                    }
                });
            ui.checkbox(&mut self.config.hud_debug, "HUD debug information")
                .on_hover_text(
                    "Show gap diagnostic status and measurement age in both HUD styles.",
                );
        });
        if self.config.hud_style != previous_style || self.config.hud_debug != previous_debug {
            for id in [
                "radar-overlay",
                "gap-ahead-overlay",
                "gap-behind-overlay",
                "performance-delta-overlay",
            ] {
                ui.ctx().request_repaint_of(ViewportId::from_hash_of(id));
            }
        }
        ui.add_space(CARD_SPACING);
        let theme = theme::resolve(self.config.hud_style);
        let delta_value = delta_value(&snapshot.delta);
        let spacing = CARD_SPACING;
        let columns = grid_columns(ui.available_width(), CARD_WIDTH, spacing).min(GADGETS.len());
        let width = (ui.available_width() - spacing * (columns - 1) as f32) / columns as f32;
        egui::Grid::new("gadget-cards")
            .num_columns(columns)
            .spacing([spacing, spacing])
            .show(ui, |ui| {
                for (index, gadget) in GADGETS.iter().enumerate() {
                    ui.allocate_ui_with_layout(
                        // egui centers cells vertically; reserve the full card
                        // height so a zero-height allocation cannot add a gap.
                        Vec2::new(width, CARD_HEIGHT),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(20, 29, 43))
                                .corner_radius(8)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.set_width((width - 24.0).max(1.0));
                                    ui.set_min_height(CARD_HEIGHT - 24.0);
                                    ui.heading(gadget.title());
                                    ui.add_space(6.0);
                                    match gadget {
                                        Gadget::Dashboard => {
                                            let previous = self.config.speed_dashboard.enabled;
                                            ui.checkbox(&mut self.config.speed_dashboard.enabled, "Enabled");
                                            if previous != self.config.speed_dashboard.enabled && self.demo.is_none() { self.connect(); }
                                            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 130.0), egui::Sense::hover());
                                            super::speed_dashboard::paint(ui.painter(), rect, &snapshot.dashboard, &self.config);
                                            ui.label(egui::RichText::new(snapshot.outgauge_error.as_deref().unwrap_or(&snapshot.dashboard.status)).small());
                                            if let Some(code) = &snapshot.dashboard.car && code != "DEMO" {
                                                let profile = self.config.cars.get(code);
                                                ui.label(format!("Car: {} ({code})", profile.map_or(code.as_str(), |p| p.name.as_str())));
                                                let has_limit = profile.is_some_and(|p| p.max_rpm > 0);
                                                if !has_limit { ui.label(egui::RichText::new("No saved RPM limit; edit the value below or press Set.").small()); }
                                                let mut max_rpm = profile.filter(|p| p.max_rpm > 0).map_or(8000, |p| p.max_rpm);
                                                let changed = ui.horizontal(|ui| {
                                                    ui.label("Max RPM");
                                                    let changed = ui.add(egui::DragValue::new(&mut max_rpm).range(1..=100000).speed(100)).changed();
                                                    let set = !has_limit && ui.button("Set").clicked();
                                                    changed || set
                                                }).inner;
                                                if changed {
                                                    self.config.cars.entry(code.clone()).or_insert(crate::config::CarProfile {
                                                        name: code.clone(), max_rpm, ..Default::default()
                                                    }).max_rpm = max_rpm;
                                                }
                                            }
                                            ui.add(egui::Slider::new(&mut self.config.rpm_blink_threshold_percent, 1..=100).text("Blink threshold").suffix("%"));
                                            ui.add(egui::Slider::new(&mut self.config.rpm_blink_interval_ms, 50..=500).text("Blink interval").suffix(" ms")).on_hover_text("Time per color. Lower values blink faster.");
                                            let gap = &mut self.gap_overlays[3];
                                            ui.checkbox(&mut gap.editing, "Position mode");
                                            gap_position_controls(ui, &mut self.config.speed_dashboard, &mut gap.window, gap.editing);
                                            ui.add(egui::Slider::new(&mut self.config.speed_dashboard.scale, 0.5..=2.0).text("Scale"));
                                            ui.label(egui::RichText::new("ABS shows enabled/triggered states. TC mirrors its warning lamp. A dash means unavailable.").small());
                                        }
                                        Gadget::Fuel => {
                                            let previous = self.config.fuel.enabled;
                                            ui.checkbox(&mut self.config.fuel.enabled, "Enabled");
                                            if previous != self.config.fuel.enabled && self.demo.is_none() { self.connect(); }
                                            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 130.0), egui::Sense::hover());
                                            super::fuel_style::paint(ui.painter(), rect, &snapshot.fuel);
                                            ui.label(egui::RichText::new(snapshot.outgauge_error.as_deref().unwrap_or(&snapshot.fuel.status)).small());
                                            let gap = &mut self.gap_overlays[4];
                                            ui.checkbox(&mut gap.editing, "Position mode");
                                            gap_position_controls(ui, &mut self.config.fuel, &mut gap.window, gap.editing);
                                            ui.add(egui::Slider::new(&mut self.config.fuel.scale, 0.5..=2.0).text("Scale"));
                                            ui.label(egui::RichText::new("REFUEL is additional tank %. * means more than one stop. Usage uses up to 5 full laps; timed-race finish estimates are unavailable.").small());
                                        }
                                        Gadget::Radar => {
                                            ui.checkbox(&mut self.config.radar_enabled, "Enabled");
                                            let (rect, _) = ui.allocate_exact_size(
                                                Vec2::new(ui.available_width(), 220.0),
                                                egui::Sense::hover(),
                                            );
                                            paint(
                                                ui.painter(),
                                                rect,
                                                &snapshot.frame,
                                                &self.config,
                                                true,
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.config.side_m,
                                                    0.0..=12.0,
                                                )
                                                .text("Side range (m)"),
                                            );
                                            if ui.checkbox(&mut self.edit_overlay, "Radar position mode").changed() {
                                                log::info!(target: "openradar_graphics", "Position mode {}", self.edit_overlay);
                                            }
                                            ui.label(egui::RichText::new(
                                                "Enable Position mode to drag the radar title bar or use Move radar."
                                            ).small());
                                            position_controls(ui, &mut self.config, &mut self.overlay_window, self.edit_overlay);
                                            ui.add(egui::Slider::new(&mut self.config.overlay_size, 220.0..=800.0)
                                                .text("Radar size"));
                                        }
                                        Gadget::Ahead | Gadget::Behind | Gadget::Delta => {
                                            let (settings, value, gap) = if *gadget == Gadget::Ahead
                                            {
                                                (
                                                    &mut self.config.gap_ahead,
                                                    &snapshot.gaps.ahead,
                                                    &mut self.gap_overlays[0],
                                                )
                                            } else if *gadget == Gadget::Behind {
                                                (
                                                    &mut self.config.gap_behind,
                                                    &snapshot.gaps.behind,
                                                    &mut self.gap_overlays[1],
                                                )
                                            } else {
                                                (
                                                    &mut self.config.performance_delta,
                                                    &delta_value,
                                                    &mut self.gap_overlays[2],
                                                )
                                            };
                                            ui.checkbox(&mut settings.enabled, "Enabled");
                                            ui.add_space(16.0);
                                            let (rect, _) = ui.allocate_exact_size(
                                                Vec2::new(ui.available_width(), 120.0),
                                                egui::Sense::hover(),
                                            );
                                            let preview = GapSettings { scale: 1.0, ..settings.clone() };
                                            if *gadget == Gadget::Delta {
                                                paint_delta(ui.painter(), rect, &snapshot.delta, &preview, theme.delta);
                                            } else {
                                                let (title, style) = if *gadget == Gadget::Ahead {
                                                    ("AHEAD", theme.ahead)
                                                } else {
                                                    ("BEHIND", theme.behind)
                                                };
                                                paint_gap(ui.painter(), rect, title, value, &GapPaintOptions::new(&preview, self.config.hud_debug), style);
                                            }
                                            ui.add_space(16.0);
                                            ui.checkbox(&mut gap.editing, "Position mode");
                                            ui.label(
                                                egui::RichText::new(
                                                    "Drag this gadget's title bar to move it.",
                                                )
                                                .small(),
                                            );
                                            gap_position_controls(
                                                ui,
                                                settings,
                                                &mut gap.window,
                                                gap.editing,
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut settings.scale, 0.5..=2.0)
                                                    .text("Scale"),
                                            );
                                        }
                                    }
                                });
                        },
                    );
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    }
}

// Registry order is insertion order. Appending a gadget fills the next cell.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gadget {
    Radar,
    Ahead,
    Behind,
    Delta,
    Dashboard,
    Fuel,
}
const GADGETS: &[Gadget] = &[
    Gadget::Radar,
    Gadget::Dashboard,
    Gadget::Ahead,
    Gadget::Behind,
    Gadget::Delta,
    Gadget::Fuel,
];
impl Gadget {
    fn title(self) -> &'static str {
        match self {
            Self::Radar => "Radar",
            Self::Ahead => "Gap ahead",
            Self::Behind => "Gap behind",
            Self::Delta => "Performance delta",
            Self::Dashboard => "Speed dashboard",
            Self::Fuel => "Fuel",
        }
    }
}
fn grid_columns(width: f32, card_width: f32, spacing: f32) -> usize {
    (((width + spacing) / (card_width + spacing)).floor() as usize).max(1)
}

#[derive(Clone, Copy)]
enum ProjectLink {
    GitHub,
    KoFi,
}
impl ProjectLink {
    fn label(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub repository",
            Self::KoFi => "Ko-fi",
        }
    }
    fn url(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/ernowo-git/lfs-openradar",
            Self::KoFi => "https://ko-fi.com/ernowo",
        }
    }
}
fn project_link(ui: &mut egui::Ui, link: ProjectLink) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Link, ui.is_enabled(), link.label())
    });
    let painter = ui.painter();
    let background = Color32::from_rgb(12, 18, 28);
    let color = if response.hovered() || response.has_focus() {
        Color32::from_rgb(92, 204, 222)
    } else {
        Color32::from_rgb(215, 223, 231)
    };
    let p = |x, y| rect.min + Vec2::new(x, y);
    match link {
        ProjectLink::GitHub => {
            // A cat silhouette cut out of a circular mark, drawn at native scale.
            painter.circle_filled(p(13.0, 13.0), 10.5, color);
            painter.rect_filled(
                Rect::from_min_max(p(7.0, 8.0), p(19.0, 16.0)),
                4.0,
                background,
            );
            for ear in [
                [p(7.0, 10.0), p(7.0, 5.5), p(11.0, 8.0)],
                [p(15.0, 8.0), p(19.0, 5.5), p(19.0, 10.0)],
            ] {
                painter.add(Shape::convex_polygon(
                    ear.to_vec(),
                    background,
                    Stroke::NONE,
                ));
            }
            painter.rect_filled(
                Rect::from_min_max(p(9.5, 14.0), p(16.5, 24.0)),
                2.0,
                background,
            );
            painter.add(Shape::line(
                vec![p(10.0, 20.0), p(7.0, 19.0), p(5.5, 16.0), p(4.0, 15.5)],
                Stroke::new(2.0_f32, background),
            ));
        }
        ProjectLink::KoFi => {
            painter.rect_stroke(
                Rect::from_min_max(p(17.0, 9.0), p(24.0, 17.0)),
                3.0,
                Stroke::new(2.5_f32, color),
                egui::StrokeKind::Middle,
            );
            painter.rect_filled(Rect::from_min_max(p(3.0, 7.0), p(20.0, 21.0)), 4.0, color);
            let heart = Color32::from_rgb(255, 94, 91);
            painter.circle_filled(p(9.3, 12.0), 2.4, heart);
            painter.circle_filled(p(13.7, 12.0), 2.4, heart);
            painter.add(Shape::convex_polygon(
                vec![p(7.0, 12.7), p(16.0, 12.7), p(11.5, 17.5)],
                heart,
                Stroke::NONE,
            ));
        }
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, color),
            egui::StrokeKind::Inside,
        );
    }
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(link.url()));
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(link.label())
}
fn fit_panel_to_monitor(desired: Vec2, monitor: Option<Vec2>) -> Vec2 {
    monitor.map_or(desired, |monitor| {
        desired.min((monitor - Vec2::new(64.0, 100.0)).max(Vec2::splat(1.0)))
    })
}
fn overlay_visible(
    master: bool,
    enabled: bool,
    editing: bool,
    demo: bool,
    hide_in_background: bool,
    foreground: bool,
) -> bool {
    master && enabled && (editing || demo || !hide_in_background || foreground)
}
impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.process_hotkey(ctx);
        self.receive_overlay_feedback();
        let now = self.started.elapsed().as_millis() as u64;
        let snapshot = if let Some(demo) = &mut self.demo {
            demo.snapshot(now, &self.config)
        } else {
            self.runtime
                .as_ref()
                .map(Runtime::snapshot)
                .unwrap_or_default()
        };
        self.control_panel(ctx, &snapshot);
        if let Some(path) = self.folder_picker.as_mut().and_then(|p| p.show(ctx)) {
            self.config.lfs_directory = path.display().to_string();
            self.setup_plan = None;
            self.setup_message = None;
        }
        if self.folder_picker.as_ref().is_some_and(|p| p.closed) {
            self.folder_picker = None;
        }
        let foreground = super::game_foreground().unwrap_or(true);
        let show_overlay = overlay_visible(
            self.overlay_enabled,
            self.config.radar_enabled,
            self.edit_overlay,
            self.demo.is_some(),
            self.config.hide_when_background,
            foreground,
        );
        if self.overlay_window.settle_size(
            self.config.overlay_size,
            self.started.elapsed(),
            ctx.input(|input| input.pointer.any_down()),
        ) {
            log::info!(target: "openradar_graphics", "Settled overlay size {}", self.config.overlay_size.round());
        }
        self.overlay_created |= show_overlay;
        // Keep the native window and GPU surface registered while hidden.
        // Focus changes should change visibility, not destroy/recreate surfaces.
        if self.overlay_created {
            let overlay_id = ViewportId::from_hash_of("radar-overlay");
            let builder = self.overlay_window.builder(show_overlay, self.edit_overlay);
            let became_visible = {
                let mut data = self.overlay_data.lock().unwrap();
                let became_visible = show_overlay && !data.visible;
                data.config = self.config.clone();
                data.visible = show_overlay;
                data.editing = self.edit_overlay;
                data.control_painted_at = Instant::now();
                became_visible
            };
            let data = self.overlay_data.clone();
            let feedback = self.overlay_feedback.clone();
            ctx.show_viewport_deferred(overlay_id, builder, move |child, _| {
                render_overlay(child, &data, &feedback);
            });
            // The child has its own timer. An immediate request every parent
            // pass can flood Windows redraw delivery across the two windows.
            if became_visible {
                ctx.request_repaint_of(overlay_id);
            }
        }
        self.show_gap_overlays(ctx, foreground);
        self.capture(ctx);
        if self
            .seconds
            .is_some_and(|seconds| self.started.elapsed().as_secs() >= seconds)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn render_overlay(
    child: &egui::Context,
    data: &Mutex<OverlayData>,
    feedback: &Mutex<OverlayFeedback>,
) {
    // A deferred callback may remain registered while the control panel stops
    // repainting. Read current settings and telemetry at paint time, never a
    // captured frame. Release all locks before drawing or native commands.
    let data = data.lock().unwrap().clone();
    if !data.visible {
        return;
    }
    let snapshot = data
        .source
        .snapshot(data.started.elapsed().as_millis() as u64, &data.config);
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(child, |ui| {
            paint(
                ui.painter(),
                ui.max_rect(),
                &snapshot.frame,
                &data.config,
                data.editing,
            );
        });
    overlay_painted(child, &data, feedback);
}

fn render_gap_overlay(
    child: &egui::Context,
    data: &Mutex<OverlayData>,
    feedback: &Mutex<OverlayFeedback>,
    kind: Gadget,
) {
    let data = data.lock().unwrap().clone();
    if !data.visible {
        return;
    }
    let snapshot = data
        .source
        .snapshot(data.started.elapsed().as_millis() as u64, &data.config);
    let theme = theme::resolve(data.config.hud_style);
    if kind == Gadget::Fuel {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(child, |ui| {
                super::fuel_style::paint(ui.painter(), ui.max_rect(), &snapshot.fuel);
            });
        overlay_painted(child, &data, feedback);
        return;
    }
    if kind == Gadget::Dashboard {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(child, |ui| {
                super::speed_dashboard::paint(
                    ui.painter(),
                    ui.max_rect(),
                    &snapshot.dashboard,
                    &data.config,
                );
            });
        overlay_painted(child, &data, feedback);
        return;
    }
    if kind == Gadget::Delta {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(child, |ui| {
                paint_delta(
                    ui.painter(),
                    ui.max_rect(),
                    &snapshot.delta,
                    &data.config.performance_delta,
                    theme.delta,
                );
            });
        overlay_painted(child, &data, feedback);
        return;
    }
    let (title, value, settings, style) = if kind == Gadget::Ahead {
        (
            "AHEAD",
            &snapshot.gaps.ahead,
            &data.config.gap_ahead,
            theme.ahead,
        )
    } else {
        (
            "BEHIND",
            &snapshot.gaps.behind,
            &data.config.gap_behind,
            theme.behind,
        )
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(child, |ui| {
            paint_gap(
                ui.painter(),
                ui.max_rect(),
                title,
                value,
                &GapPaintOptions::new(settings, data.config.hud_debug),
                style,
            );
        });
    overlay_painted(child, &data, feedback);
}

fn overlay_painted(child: &egui::Context, data: &OverlayData, feedback: &Mutex<OverlayFeedback>) {
    if let Ok(mut feedback) = feedback.lock() {
        let panel_age = data.control_painted_at.elapsed();
        let stalled = panel_age >= Duration::from_secs(5);
        if stalled && !feedback.panel_stall_reported {
            log::info!(target: "openradar_graphics", "Control panel has not repainted for {} ms; overlay still repainting", panel_age.as_millis());
        } else if !stalled && feedback.panel_stall_reported {
            log::info!(target: "openradar_graphics", "Control panel repainting resumed");
        }
        feedback.panel_stall_reported = stalled;
        if let Some(rect) = child.input(|i| i.viewport().outer_rect)
            && feedback.observed_position != Some(rect.min)
        {
            feedback.observed_position = Some(rect.min);
            feedback.moved_to = Some(rect.min);
        }
        if child.input(|i| i.viewport().close_requested()) {
            feedback.closed = true;
            child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            child.request_repaint_of(ViewportId::ROOT);
        }
    }
    child.request_repaint_after(Duration::from_millis(16));
}

fn gap_position_controls(
    ui: &mut egui::Ui,
    settings: &mut GapSettings,
    window: &mut GapWindow,
    editing: bool,
) {
    ui.horizontal(|ui| {
        let x = ui.add(
            egui::DragValue::new(settings.window_x.get_or_insert(0.0))
                .prefix("X ")
                .range(-32000.0..=32000.0),
        );
        let y = ui.add(
            egui::DragValue::new(settings.window_y.get_or_insert(0.0))
                .prefix("Y ")
                .range(-32000.0..=32000.0),
        );
        if x.changed() || y.changed() {
            window.set_position(settings);
        }
    });
    if editing {
        let drag = ui
            .add(egui::Button::new("Move gadget").sense(egui::Sense::drag()))
            .on_hover_cursor(egui::CursorIcon::Grab);
        let delta = drag.drag_delta();
        if delta != Vec2::ZERO {
            *settings.window_x.get_or_insert(0.0) += delta.x;
            *settings.window_y.get_or_insert(0.0) += delta.y;
            window.set_position(settings);
        }
    }
}

fn position_controls(
    ui: &mut egui::Ui,
    config: &mut Config,
    window: &mut OverlayWindow,
    editing: bool,
) -> Option<egui::Response> {
    ui.horizontal(|ui| {
        ui.label("Position");
        let x = ui.add(
            egui::DragValue::new(&mut config.overlay_x)
                .prefix("X ")
                .range(-8000.0..=8000.0),
        );
        let y = ui.add(
            egui::DragValue::new(&mut config.overlay_y)
                .prefix("Y ")
                .range(-8000.0..=8000.0),
        );
        if x.changed() || y.changed() {
            window.set_position(config);
        }
    });
    editing.then(|| {
        let drag = ui
            .add(egui::Button::new("Move radar").sense(egui::Sense::drag()))
            .on_hover_cursor(egui::CursorIcon::Grab);
        let delta = drag.drag_delta();
        if delta != Vec2::ZERO {
            config.overlay_x = (config.overlay_x + delta.x).clamp(-8000.0, 8000.0);
            config.overlay_y = (config.overlay_y + delta.y).clamp(-8000.0, 8000.0);
            window.set_position(config);
        }
        drag
    })
}

fn badge(ui: &mut egui::Ui, name: &str, age: Option<u64>, stale: u64) {
    let color = if age.is_some_and(|age| age <= stale) {
        Color32::from_rgb(100, 225, 166)
    } else {
        Color32::from_rgb(240, 178, 85)
    };
    let text = age
        .map(|age| format!("{name} {age} ms"))
        .unwrap_or_else(|| format!("{name} waiting"));
    ui.label(egui::RichText::new(text).color(color).small());
}

fn follow_view_controls(ui: &mut egui::Ui, config: &mut Config) -> egui::Response {
    let response = ui.checkbox(&mut config.follow_viewed_car, "Follow viewed car")
        .on_hover_text("Follow the car you watch in live single player, including AI. Use cockpit or custom view. Multiplayer still requires your own car.");
    ui.label(
        egui::RichText::new("Apply / reconnect to change following. Save settings to remember it.")
            .small(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{
        gap_style,
        render::{estimated_lap_text, lap_time_text},
    };
    use crate::{
        gaps::GapValue,
        radar::{RadarCar, RadarFrame, Threat},
    };

    #[test]
    fn hotkey_toggles_shared_visibility_without_changing_enabled_gadgets() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        ctx.set_embed_viewports(false);
        let mut config = Config::default();
        config.gap_ahead.enabled = true;
        config.gap_behind.enabled = true;
        config.performance_delta.enabled = true;
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        let saved = toml::to_string(&app.config).unwrap();
        let queued = Arc::new(AtomicUsize::new(0));
        app.hotkey = Some(super::super::hotkey::OverlayHotkey::queued(queued.clone()));
        for (presses, visible) in [(0, true), (1, false), (0, false), (1, true)] {
            queued.store(presses, Ordering::Relaxed);
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                app.process_hotkey(ctx);
                app.show_gap_overlays(ctx, true);
            });
            assert_eq!(app.overlay_enabled, visible);
            for id in [
                "gap-ahead-overlay",
                "gap-behind-overlay",
                "performance-delta-overlay",
            ] {
                assert_eq!(
                    output.viewport_output[&ViewportId::from_hash_of(id)]
                        .builder
                        .visible,
                    Some(visible)
                );
            }
            assert_eq!(toml::to_string(&app.config).unwrap(), saved);
        }
    }

    #[test]
    fn follow_view_checkbox_changes_the_saved_config() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let mut config = Config::default();
        let mut rect = Rect::NOTHING;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = follow_view_controls(ui, &mut config).rect;
            });
        });
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,
            Shape::Text(t) if t.galley.text() == "Follow viewed car")));
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(rect.center()),
                        egui::Event::PointerButton {
                            pos: rect.center(),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        follow_view_controls(ui, &mut config);
                    });
                },
            );
        }
        assert!(config.follow_viewed_car);
        let restored: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert!(restored.follow_viewed_car);
    }

    #[test]
    fn cards_wrap_left_to_right_and_reflow_when_resized() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let mut app = App::new(
            Config::default(),
            PathBuf::from("unused.toml"),
            true,
            None,
            None,
        );
        let snapshot = Demo::default().snapshot(7_000, &app.config);
        for (width, expected_columns) in
            [(340.0, 1), (680.0, 2), (1020.0, 3), (1276.0, 4), (340.0, 1)]
        {
            let mut output = egui::FullOutput::default();
            // Give egui's remembered grid measurements time to settle after reflow.
            for _ in 0..3 {
                output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(width, GADGETS.len() as f32 * CARD_HEIGHT + 300.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            app.gadget_cards(ui, &snapshot);
                        });
                    },
                );
            }
            let cards: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(rect) if rect.fill == Color32::from_rgb(20, 29, 43) => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(cards.len(), GADGETS.len());
            let selector = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text)
                        if matches!(
                            text.galley.job.text.as_str(),
                            "GT" | "HUD debug information"
                        ) =>
                    {
                        Some(text.visual_bounding_rect())
                    }
                    _ => None,
                })
                .max_by(|a, b| a.bottom().total_cmp(&b.bottom()))
                .unwrap();
            assert!(
                cards[0].top() > selector.bottom() && cards[0].top() - selector.bottom() < 20.0,
                "unexpected space before first row: {cards:?}"
            );
            for (index, card) in cards.iter().enumerate() {
                assert!(card.right() <= width + 0.1, "card exceeds width: {cards:?}");
                if index % expected_columns == 0 {
                    assert!((card.left() - cards[0].left()).abs() < 0.1);
                }
                if index > 0 {
                    if index % expected_columns == 0 {
                        assert!(card.top() > cards[index - 1].bottom());
                    } else {
                        assert!(card.left() > cards[index - 1].right());
                        assert!((card.top() - cards[index - 1].top()).abs() < 0.1);
                    }
                }
            }
        }
        assert_eq!(grid_columns(611.9, 300.0, 12.0), 1);
        assert_eq!(grid_columns(612.0, 300.0, 12.0), 2);
    }

    #[test]
    fn opening_panel_fits_cards_and_tabs_keep_actions_in_the_header() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let mut app = App::new(
            Config::default(),
            PathBuf::from("unused.toml"),
            true,
            None,
            None,
        );
        let snapshot = Demo::default().snapshot(7_000, &app.config);
        let mut size = initial_panel_size();
        let mut output = egui::FullOutput::default();
        for _ in 0..6 {
            output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| app.control_panel(ctx, &snapshot),
            );
            for command in &output.viewport_output[&ViewportId::ROOT].commands {
                if let egui::ViewportCommand::InnerSize(fitted) = command {
                    size = *fitted;
                }
            }
        }
        let texts = |output: &egui::FullOutput| -> Vec<(String, Rect)> {
            output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let Shape::Text(text) = &shape.shape {
                        Some((
                            text.galley.job.text.clone(),
                            text.galley.rect.translate(text.pos.to_vec2()),
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        };
        let gadget_texts = texts(&output);
        let radar_card = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Rect(rect) if rect.fill == Color32::from_rgb(20, 29, 43) => Some(rect.rect),
                _ => None,
            })
            .unwrap();
        for label in [
            "Radar position mode",
            "Position",
            "Radar size",
            "Side range (m)",
        ] {
            let (_, rect) = gadget_texts.iter().find(|(text, _)| text == label).unwrap();
            assert!(
                radar_card.contains_rect(*rect),
                "{label} must fit inside the Radar card"
            );
        }
        for shape in &output.shapes {
            if let Shape::Rect(rect) = &shape.shape
                && rect.fill == Color32::from_rgb(20, 29, 43)
            {
                assert!(rect.rect.bottom() <= size.y, "opening panel clips a card");
            }
        }
        assert!(!gadget_texts.iter().any(|(text, _)| text == "Show overlays"));
        app.tab = ControlTab::Settings;
        let settings = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(380.0, 900.0))),
                ..Default::default()
            },
            |ctx| app.control_panel(ctx, &snapshot),
        );
        let settings_texts = texts(&settings);
        for label in ["Apply / reconnect", "Save settings", "Quit"] {
            let (_, gadget_rect) = gadget_texts.iter().find(|(text, _)| text == label).unwrap();
            assert!(gadget_rect.bottom() < radar_card.top());
            let (_, settings_rect) = settings_texts
                .iter()
                .find(|(text, _)| text == label)
                .unwrap();
            assert!(
                settings_rect.bottom() < 150.0,
                "{label} must remain in the narrow header"
            );
        }
        for label in [
            "Show overlays",
            "LFS startup setup",
            "Interpolation (ms)",
            "InSim password",
        ] {
            assert!(settings_texts.iter().any(|(text, _)| text == label));
        }
        assert!(
            settings_texts
                .iter()
                .any(|(text, _)| text.contains("reduce jitter but add latency"))
        );
        assert!(
            !settings_texts
                .iter()
                .any(|(text, _)| text == "Radar position mode")
        );
        assert!(
            settings.viewport_output[&ViewportId::ROOT]
                .commands
                .iter()
                .all(|command| { !matches!(command, egui::ViewportCommand::InnerSize(_)) }),
            "tab switches must preserve the user's window size"
        );
        let constrained =
            fit_panel_to_monitor(initial_panel_size(), Some(Vec2::new(1024.0, 768.0)));
        assert!(constrained.x <= 960.0 && constrained.y <= 668.0);

        // Save through the header button, using the same config field edited by
        // the masked password input, and verify persistence to the launch path.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let config_path = std::env::temp_dir().join(format!("openradar-password-ui-{stamp}.toml"));
        app.config_path = config_path.clone();
        app.config.insim_password.0 = "test-password".into();
        let save_position = settings_texts
            .iter()
            .find(|(text, _)| text == "Save settings")
            .unwrap()
            .1
            .center();
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(380.0, 400.0))),
                    events: vec![
                        egui::Event::PointerMoved(save_position),
                        egui::Event::PointerButton {
                            pos: save_position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| app.control_panel(ctx, &snapshot),
            );
        }
        let restored = Config::load(&config_path).unwrap();
        assert_eq!(restored.insim_password.0, "test-password");
        assert!(app.message.as_ref().unwrap().starts_with("Saved "));
        std::fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn gap_panels_fit_their_own_viewport_at_extreme_scales() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let value = GapValue {
            seconds: Some(5.0),
            ..Default::default()
        };
        for size in [220.0, 800.0] {
            for scale in [2.0, 0.5] {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(size))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ctx, |ui| {
                                paint_gap(
                                    ui.painter(),
                                    ui.max_rect(),
                                    "AHEAD",
                                    &value,
                                    &GapPaintOptions {
                                        scale,
                                        debug: false,
                                    },
                                    &gap_style::AHEAD,
                                );
                            });
                    },
                );
                for shape in output.shapes {
                    if let Shape::Rect(rect) = shape.shape {
                        assert!(rect.rect.left() >= 0.0 && rect.rect.top() >= 0.0);
                        assert!(rect.rect.right() <= size && rect.rect.bottom() <= size);
                    }
                }
            }
        }
    }

    #[test]
    fn gap_windows_register_independently_and_feedback_does_not_move_or_close_neighbors() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        ctx.set_embed_viewports(false);
        let mut config = Config {
            radar_enabled: false,
            ..Default::default()
        };
        config.gap_ahead.enabled = true;
        config.gap_behind.enabled = true;
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        app.gap_overlays[0].editing = true;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.show_gap_overlays(ctx, true)
        });
        let ahead_id = ViewportId::from_hash_of("gap-ahead-overlay");
        let behind_id = ViewportId::from_hash_of("gap-behind-overlay");
        assert!(
            !output
                .viewport_output
                .contains_key(&ViewportId::from_hash_of("radar-overlay"))
        );
        assert!(output.viewport_output[&ahead_id].viewport_ui_cb.is_some());
        assert!(output.viewport_output[&behind_id].viewport_ui_cb.is_some());
        assert_eq!(
            output.viewport_output[&ahead_id].builder.decorations,
            Some(true)
        );
        assert_eq!(
            output.viewport_output[&behind_id].builder.decorations,
            Some(false)
        );
        let initial_behind = output.viewport_output[&behind_id].builder.clone();
        let mut initial_ahead = output.viewport_output[&ahead_id].builder.clone();
        app.gap_overlays[0].feedback.lock().unwrap().moved_to = Some(Pos2::new(-900.0, 350.0));
        app.receive_overlay_feedback();
        assert_eq!(app.config.gap_ahead.window_x, Some(-900.0));
        // An observed OS drag is saved, without a native move feedback loop.
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.show_gap_overlays(ctx, true)
        });
        assert!(
            initial_ahead
                .patch(output.viewport_output[&ahead_id].builder.clone())
                .0
                .is_empty()
        );
        assert_eq!(
            output.viewport_output[&behind_id].builder.position,
            initial_behind.position
        );
        app.gap_overlays[0].feedback.lock().unwrap().closed = true;
        app.receive_overlay_feedback();
        assert!(!app.config.gap_ahead.enabled);
        assert!(app.config.gap_behind.enabled);
        assert!(app.overlay_enabled);
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.show_gap_overlays(ctx, true)
        });
        assert_eq!(
            output.viewport_output[&ahead_id].builder.visible,
            Some(false)
        );
        assert_eq!(
            output.viewport_output[&behind_id].builder.visible,
            Some(true)
        );
        // Retain the hidden window and callback for reuse.
        assert!(output.viewport_output[&ahead_id].viewport_ui_cb.is_some());
    }

    #[test]
    fn separate_gap_callbacks_refresh_without_the_panel_or_radar() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        ctx.set_embed_viewports(false);
        let mut config = Config {
            radar_enabled: false,
            hud_style: HudStyle::Classic,
            ..Default::default()
        };
        config.gap_ahead.enabled = true;
        config.gap_behind.enabled = true;
        config.performance_delta.enabled = true;
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        // Use the same deterministic telemetry source for both independent windows.
        app.set_source(OverlaySource::Disconnected);
        for gap in &app.gap_overlays {
            gap.data.lock().unwrap().started = Instant::now() - Duration::from_secs(7);
        }
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.show_gap_overlays(ctx, true)
        });
        let parent_passes = ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        let callbacks: Vec<_> = [
            "gap-ahead-overlay",
            "gap-behind-overlay",
            "performance-delta-overlay",
        ]
        .iter()
        .map(|name| {
            let id = ViewportId::from_hash_of(name);
            (
                id,
                output.viewport_output[&id].viewport_ui_cb.clone().unwrap(),
            )
        })
        .collect();
        let draw = |id, callback: &egui::DeferredViewportUiCallback| {
            let mut input = egui::RawInput {
                viewport_id: id,
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    if id == ViewportId::from_hash_of("performance-delta-overlay") {
                        Vec2::new(230.0, 100.0)
                    } else {
                        Vec2::new(230.0, 76.0)
                    },
                )),
                ..Default::default()
            };
            input.viewports.insert(
                id,
                egui::ViewportInfo {
                    parent: Some(ViewportId::ROOT),
                    ..Default::default()
                },
            );
            ctx.run(input, |child| callback(child))
        };
        let text_present = |output: &egui::FullOutput, expected: &str| {
            output.shapes.iter().any(|shape| {
            matches!(&shape.shape, Shape::Text(text) if text.galley.job.text == expected)
        })
        };
        assert!(text_present(
            &draw(callbacks[0].0, callbacks[0].1.as_ref()),
            "AHEAD  —"
        ));
        app.set_source(OverlaySource::Demo(Arc::new(Mutex::new(Demo::default()))));
        for ((id, callback), expected) in callbacks.iter().zip(["AHEAD  ~5.0 s", "BEHIND  ~2.3 s"])
        {
            let output = draw(*id, callback.as_ref());
            assert!(text_present(&output, expected));
            assert!(output.viewport_output[id].repaint_delay <= Duration::from_millis(16));
        }
        app.set_source(OverlaySource::Disconnected);
        assert!(text_present(
            &draw(callbacks[1].0, callbacks[1].1.as_ref()),
            "BEHIND  —"
        ));
        // The same retained callbacks must observe a changed theme in either direction.
        for (style, expected) in [
            (
                HudStyle::Gt7Inspired,
                ["AHEAD", "BEHIND", "SESSION BEST / LIVE DELTA"],
            ),
            (HudStyle::Classic, ["AHEAD  —", "BEHIND  —", "DELTA  — s"]),
        ] {
            for gap in &app.gap_overlays {
                gap.data.lock().unwrap().config.hud_style = style;
            }
            for ((id, callback), label) in callbacks.iter().zip(expected) {
                assert!(text_present(&draw(*id, callback.as_ref()), label));
            }
        }
        // Retained gap callbacks also observe diagnostics changes without a parent pass.
        app.set_source(OverlaySource::Demo(Arc::new(Mutex::new(Demo::default()))));
        for style in [HudStyle::Classic, HudStyle::Gt7Inspired] {
            for debug in [false, true, false] {
                for gap in &app.gap_overlays {
                    let mut data = gap.data.lock().unwrap();
                    data.config.hud_style = style;
                    data.config.hud_debug = debug;
                }
                for (id, callback) in callbacks.iter().take(2) {
                    let output = draw(*id, callback.as_ref());
                    assert_eq!(
                        output.shapes.iter().any(|shape| matches!(&shape.shape,
                    Shape::Text(text) if text.galley.job.text.starts_with("ESTIMATE"))),
                        debug
                    );
                }
            }
        }
        assert_eq!(ctx.cumulative_pass_nr_for(ViewportId::ROOT), parent_passes);
    }

    #[test]
    fn each_position_mode_overrides_background_hiding_for_only_its_window() {
        assert!(!overlay_visible(true, true, false, false, true, false));
        assert!(overlay_visible(true, true, true, false, true, false));
        assert!(!overlay_visible(true, false, true, false, true, true));
        assert!(!overlay_visible(false, true, true, false, true, true));
    }

    #[test]
    fn startup_setup_handles_insim_conflicts_and_configures_outsim_in_one_click() {
        use std::fs;
        let temp_root = std::env::temp_dir().canonicalize().unwrap();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let folder = temp_root.join(format!("openradar-setup-ui-test-{stamp}"));
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("LFS.exe"), b"test fixture").unwrap();
        fs::create_dir_all(folder.join("data/script")).unwrap();
        let script = folder.join("data/script/autoexec.lfs");
        let original = b"// keep this\r\n/insim 29998\r\n/ff 80\r\n";
        fs::write(&script, original).unwrap();
        let cfg_path = folder.join("cfg.txt");
        let cfg_original = b"Game Admin preserve-me\r\nOutSim Mode 0\r\n";
        fs::write(&cfg_path, cfg_original).unwrap();
        let config = Config {
            lfs_directory: folder.display().to_string(),
            ..Default::default()
        };
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        fn draw(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> egui::FullOutput {
            ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 600.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.startup_setup_controls(ui));
                },
            )
        }
        fn click(ctx: &egui::Context, app: &mut App, label: &str) {
            let output = draw(ctx, app, vec![]);
            let position = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Button not found: {label}"));
            for pressed in [true, false] {
                draw(
                    ctx,
                    app,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        }
        draw(&ctx, &mut app, vec![]);
        assert_eq!(fs::read(&script).unwrap(), original);
        assert_eq!(fs::read(&cfg_path).unwrap(), cfg_original);
        click(&ctx, &mut app, "Enable InSim at startup");
        assert!(app.setup_plan.is_some());
        assert_eq!(fs::read(&script).unwrap(), original);
        click(&ctx, &mut app, "Cancel");
        assert!(app.setup_plan.is_none());
        click(&ctx, &mut app, "Enable InSim at startup");
        click(&ctx, &mut app, "Use existing port 29998");
        assert_eq!(app.config.insim_address.port(), 29998);
        assert_eq!(fs::read(&script).unwrap(), original);
        assert_eq!(fs::read_dir(script.parent().unwrap()).unwrap().count(), 1);
        app.config.insim_address.set_port(29999);
        click(&ctx, &mut app, "Enable InSim at startup");
        click(&ctx, &mut app, "Replace with OpenRadar port 29999");
        assert_eq!(
            fs::read(&script).unwrap(),
            b"// keep this\r\n/insim 29999\r\n/ff 80\r\n"
        );
        assert_eq!(fs::read_dir(script.parent().unwrap()).unwrap().count(), 2);
        assert!(app.setup_plan.is_none());
        assert!(app.setup_message.as_ref().unwrap().contains("Restart LFS"));
        click(&ctx, &mut app, "Configure OutSim");
        assert_eq!(fs::read(folder.join("cfg.txt.BAK")).unwrap(), cfg_original);
        assert_eq!(fs::read(&cfg_path).unwrap(),
            b"Game Admin preserve-me\r\nOutSim Mode 1\r\nOutSim Delay 2\r\nOutSim IP 127.0.0.1\r\nOutSim Port 30000\r\nOutSim ID 24601\r\nOutSim Opts 1ff\r\n");
        assert!(
            app.setup_message
                .as_ref()
                .unwrap()
                .contains("Configured OutSim")
        );
        let files = fs::read_dir(&folder).unwrap().count();
        click(&ctx, &mut app, "Configure OutSim");
        assert!(
            app.setup_message
                .as_ref()
                .unwrap()
                .contains("already matches")
        );
        assert_eq!(fs::read_dir(&folder).unwrap().count(), files);
        assert_eq!(folder.parent(), Some(temp_root.as_path()));
        assert!(
            folder
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("openradar-setup-ui-test-")
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn header_link_icons_open_the_requested_pages_in_a_new_tab() {
        for link in [ProjectLink::GitHub, ProjectLink::KoFi] {
            let ctx = egui::Context::default();
            super::super::fonts::install(&ctx);
            let mut rect = Rect::NOTHING;
            let mut draw = |events| {
                ctx.run(
                    egui::RawInput {
                        events,
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            rect = project_link(ui, link).rect;
                        });
                    },
                )
            };
            draw(vec![]);
            let position = rect.center();
            // End the first draw borrow before sending the click.
            let mut output = egui::FullOutput::default();
            for pressed in [true, false] {
                output = ctx.run(
                    egui::RawInput {
                        events: vec![
                            egui::Event::PointerMoved(position),
                            egui::Event::PointerButton {
                                pos: position,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            project_link(ui, link);
                        });
                    },
                );
            }
            assert!(
                output
                    .platform_output
                    .commands
                    .iter()
                    .any(|command| matches!(command,
                egui::OutputCommand::OpenUrl(open) if open.url == link.url() && open.new_tab))
            );
        }
    }

    #[test]
    fn delta_estimate_and_reference_labels_fit_the_overlay() {
        assert_eq!(lap_time_text(59.999), "1:00.00");
        for scale in [0.6, 1.0, 2.5] {
            let ctx = egui::Context::default();
            super::super::fonts::install(&ctx);
            let value = crate::delta::DeltaFrame {
                seconds: Some(-0.15),
                trend: Some(-0.1),
                best_seconds: Some(83.6),
                estimated_lap_seconds: Some(83.45),
                status: "Estimated vs session best".into(),
            };
            let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(230.0, 100.0) * scale);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ctx| {
                    paint_delta(
                        &ctx.layer_painter(egui::LayerId::background()),
                        rect,
                        &value,
                        &GapSettings {
                            scale,
                            ..Default::default()
                        },
                        theme::resolve(HudStyle::Classic).delta,
                    );
                },
            );
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(text) => Some(text),
                    _ => None,
                })
                .collect();
            assert!(
                texts
                    .iter()
                    .any(|t| t.galley.job.text == "Estimated lap 1:23.45")
            );
            assert!(texts.iter().any(|t| t.galley.job.text.contains("ESTIMATE")));
            for text in texts {
                assert!(
                    rect.expand(scale)
                        .contains_rect(text.visual_bounding_rect()),
                    "{}",
                    text.galley.job.text
                );
            }
            assert_eq!(
                estimated_lap_text(&crate::delta::DeltaFrame::default()),
                "Estimated lap —"
            );
        }
    }

    #[test]
    fn delta_viewport_refreshes_and_closes_independently() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        ctx.set_embed_viewports(false);
        let mut config = Config {
            hud_style: HudStyle::Classic,
            ..Default::default()
        };
        config.performance_delta.enabled = true;
        config.gap_ahead.enabled = true;
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        app.gap_overlays[2].editing = true;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.show_gap_overlays(ctx, true)
        });
        let id = ViewportId::from_hash_of("performance-delta-overlay");
        assert_eq!(output.viewport_output[&id].builder.decorations, Some(true));
        assert_eq!(
            output.viewport_output[&id].builder.inner_size,
            Some(Vec2::new(230.0, 100.0))
        );
        let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
        app.set_source(OverlaySource::Demo(Arc::new(Mutex::new(Demo::default()))));
        app.gap_overlays[2].data.lock().unwrap().started =
            Instant::now() - Duration::from_secs(201);
        let parent_passes = ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        let mut input = egui::RawInput {
            viewport_id: id,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(230.0, 100.0))),
            ..Default::default()
        };
        input.viewports.insert(
            id,
            egui::ViewportInfo {
                parent: Some(ViewportId::ROOT),
                ..Default::default()
            },
        );
        let output = ctx.run(input, |child| callback(child));
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,
            Shape::Text(t) if t.galley.job.text.starts_with("DELTA  +0.00 s"))));
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,
            Shape::Text(t) if t.galley.job.text == "Estimated lap 1:40.00")));
        assert!(output.viewport_output[&id].repaint_delay <= Duration::from_millis(16));
        assert_eq!(ctx.cumulative_pass_nr_for(ViewportId::ROOT), parent_passes);
        app.gap_overlays[2].feedback.lock().unwrap().moved_to = Some(Pos2::new(-800.0, 440.0));
        app.gap_overlays[2].feedback.lock().unwrap().closed = true;
        app.receive_overlay_feedback();
        assert_eq!(app.config.performance_delta.window_x, Some(-800.0));
        assert!(!app.config.performance_delta.enabled);
        assert!(app.config.gap_ahead.enabled && app.config.radar_enabled);
    }

    #[test]
    fn fuel_viewport_refreshes_and_closes_independently() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        ctx.set_embed_viewports(false);
        let mut config = Config::default();
        config.fuel.enabled = true;
        config.speed_dashboard.enabled = true;
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        app.gap_overlays[4].editing = true;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.show_gap_overlays(ctx, true)
        });
        let id = ViewportId::from_hash_of("fuel-overlay");
        assert_eq!(output.viewport_output[&id].builder.decorations, Some(true));
        assert_eq!(
            output.viewport_output[&id].builder.inner_size,
            Some(super::super::fuel_style::SIZE)
        );
        let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
        let parent_passes = ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        let mut input = egui::RawInput {
            viewport_id: id,
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                super::super::fuel_style::SIZE,
            )),
            ..Default::default()
        };
        input.viewports.insert(
            id,
            egui::ViewportInfo {
                parent: Some(ViewportId::ROOT),
                ..Default::default()
            },
        );
        let output = ctx.run(input, |child| callback(child));
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,
            Shape::Text(t) if t.galley.text() == "−7.7")));
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,
            Shape::Text(t) if t.galley.text() == "12.9")));
        assert!(output.viewport_output[&id].repaint_delay <= Duration::from_millis(16));
        assert_eq!(ctx.cumulative_pass_nr_for(ViewportId::ROOT), parent_passes);
        app.gap_overlays[4].feedback.lock().unwrap().moved_to = Some(Pos2::new(-800.0, 440.0));
        app.gap_overlays[4].feedback.lock().unwrap().closed = true;
        app.receive_overlay_feedback();
        assert_eq!(app.config.fuel.window_x, Some(-800.0));
        assert!(!app.config.fuel.enabled);
        assert!(app.config.speed_dashboard.enabled && app.config.radar_enabled);
    }

    #[test]
    fn panel_drag_fallback_preserves_clicks_without_a_native_drag_loop() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let mut config = Config::default();
        let initial_position = Pos2::new(config.overlay_x, config.overlay_y);
        let mut window = OverlayWindow::new(&config);
        let mut native = window.builder(true, false);
        let mut draw = |events: Vec<egui::Event>, editing| {
            let mut handle = None;
            let mut quit = None;
            let output = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        handle = position_controls(ui, &mut config, &mut window, editing);
                        quit = Some(ui.button("Quit"));
                    });
                },
            );
            (output, handle, quit.unwrap(), window.builder(true, false))
        };
        draw(vec![], false);
        let (_, handle, _, builder) = draw(vec![], true);
        let (commands, recreate) = native.patch(builder);
        assert!(
            commands.is_empty(),
            "Enabling position mode must not change native window styles"
        );
        assert!(!recreate);
        assert_eq!(native.mouse_passthrough, Some(true));
        let origin = handle.unwrap().rect.center();
        let button = |position, pressed| egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw(
            vec![egui::Event::PointerMoved(origin), button(origin, true)],
            true,
        );
        let moved = origin + Vec2::new(40.0, 25.0);
        let (output, _, _, builder) = draw(vec![egui::Event::PointerMoved(moved)], true);
        assert_eq!(
            builder.position,
            Some(initial_position + Vec2::new(40.0, 25.0))
        );
        let (commands, recreate) = native.patch(builder);
        assert!(matches!(
            commands.as_slice(),
            [egui::ViewportCommand::OuterPosition(_)]
        ));
        assert!(!recreate);
        assert!(
            output
                .viewport_output
                .values()
                .all(|viewport| viewport.commands.is_empty()),
            "Dragging the panel must not enter a native drag loop"
        );
        draw(vec![button(moved, false)], true);
        let (_, _, quit, builder) = draw(vec![], false);
        assert!(
            native.patch(builder).0.is_empty(),
            "Leaving position mode must not change native styles"
        );
        let quit_pos = quit.rect.center();
        draw(
            vec![egui::Event::PointerMoved(quit_pos), button(quit_pos, true)],
            false,
        );
        let (_, _, quit, _) = draw(vec![button(quit_pos, false)], false);
        assert!(
            quit.clicked(),
            "Control panel clicks must still work after dragging"
        );
    }

    #[test]
    fn retained_overlay_callback_updates_without_control_panel_repaints() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        ctx.set_embed_viewports(false);
        let config = Config {
            hud_style: HudStyle::Classic,
            ..Default::default()
        };
        let window = OverlayWindow::new(&config);
        let data = Arc::new(Mutex::new(OverlayData {
            source: OverlaySource::Disconnected,
            config,
            started: Instant::now() - Duration::from_secs(1),
            control_painted_at: Instant::now(),
            visible: true,
            editing: false,
        }));
        let feedback = Arc::new(Mutex::new(OverlayFeedback::default()));
        let id = ViewportId::from_hash_of("regression-overlay");
        let output = ctx.run(egui::RawInput::default(), |parent| {
            let data = data.clone();
            let feedback = feedback.clone();
            parent.show_viewport_deferred(id, window.builder(true, false), move |child, _| {
                render_overlay(child, &data, &feedback);
            });
        });
        let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
        let parent_passes = ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        let draw_child = || {
            let mut input = egui::RawInput {
                viewport_id: id,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(320.0))),
                ..Default::default()
            };
            input.viewports.insert(
                id,
                egui::ViewportInfo {
                    parent: Some(ViewportId::ROOT),
                    ..Default::default()
                },
            );
            ctx.run(input, |child| callback(child))
        };
        let contains_text = |output: &egui::FullOutput, expected: &str| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, Shape::Text(text) if text.galley.job.text == expected)
            })
        };
        assert!(contains_text(&draw_child(), "RADAR PAUSED"));

        // Change the telemetry source without replacing the registered callback
        // or running another control-panel pass.
        data.lock().unwrap().source = OverlaySource::Demo(Arc::new(Mutex::new(Demo::default())));
        let live = draw_child();
        assert!(contains_text(&live, "YOU"));
        assert!(!contains_text(&live, "RADAR PAUSED"));

        // A disconnect must propagate just as promptly, without bypassing the
        // required-input pause gate.
        data.lock().unwrap().source = OverlaySource::Disconnected;
        let paused = draw_child();
        assert!(contains_text(&paused, "RADAR PAUSED"));
        assert!(paused.viewport_output[&id].repaint_delay <= Duration::from_millis(16));
        data.lock().unwrap().config.hud_style = HudStyle::Gt7Inspired;
        assert!(contains_text(&draw_child(), "Radar"));
        data.lock().unwrap().config.hud_style = HudStyle::Classic;
        assert!(!contains_text(&draw_child(), "Radar"));
        assert_eq!(ctx.cumulative_pass_nr_for(ViewportId::ROOT), parent_passes);

        data.lock().unwrap().visible = false;
        assert!(draw_child().shapes.is_empty());
    }

    #[test]
    fn range_and_size_extremes_have_finite_bounded_meshes_without_a_gpu() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let frame = RadarFrame {
            live: true,
            cars: (1..=48)
                .map(|plid| RadarCar {
                    plid,
                    name: String::new(),
                    right: (plid % 7) as f64 - 3.0,
                    forward: (plid % 13) as f64 - 6.0,
                    relative_heading: plid as f64 * 0.15,
                    threat: Threat::Alongside,
                    uncertain: false,
                })
                .collect(),
            ..Default::default()
        };
        for size in [220.0, 320.0, 800.0] {
            for side in [3.0, 5.0, 12.0] {
                let config = Config {
                    side_m: side,
                    overlay_size: size,
                    ..Default::default()
                };
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(size))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            paint(ui.painter(), ui.max_rect(), &frame, &config, true);
                        });
                    },
                );
                let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                let mut vertices = 0;
                for primitive in primitives {
                    if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                        vertices += mesh.vertices.len();
                        assert!(mesh.is_valid());
                        assert!(mesh.vertices.iter().all(|vertex| vertex.pos.is_finite()));
                    }
                }
                assert!(
                    vertices > 0 && vertices < 20_000,
                    "size {size}, side {side}: {vertices} vertices"
                );
            }
        }
    }
}
