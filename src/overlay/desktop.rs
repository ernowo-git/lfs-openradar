use super::window::{GapWindow, OverlayWindow};
use crate::{
    config::{Config, GapSettings},
    demo::Demo,
    gaps::GapValue,
    radar::{RadarFrame, Threat},
    runtime::{Runtime, Snapshot, SnapshotReader},
};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2, ViewportBuilder, ViewportId,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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
            .with_inner_size([1020.0, 760.0])
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
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(App::new(
                config,
                config_path,
                demo,
                seconds,
                screenshot,
            )))
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
    edit_overlay: bool,
    overlay_created: bool,
    overlay_window: OverlayWindow,
    overlay_feedback: Arc<Mutex<OverlayFeedback>>,
    overlay_data: Arc<Mutex<OverlayData>>,
    gap_overlays: [GapOverlay; 3],
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
            edit_overlay: false,
        };
        if !demo {
            app.connect();
        }
        app
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
    fn startup_setup_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("LFS startup setup").default_open(true).show(ui, |ui| {
            ui.label("Select your LFS installation to enable InSim automatically on future launches.");
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
            } else {
                (
                    Gadget::Delta,
                    &self.config.performance_delta,
                    "performance-delta-overlay",
                    "LFS OpenRadar · Performance delta",
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
                gap.window.builder(title, visible, gap.editing),
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
    fn gadget_cards(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let delta_value = delta_value(&snapshot.delta);
        let spacing = 12.0;
        let columns = grid_columns(ui.available_width(), 300.0, spacing);
        let width = (ui.available_width() - spacing * (columns - 1) as f32) / columns as f32;
        egui::Grid::new("gadget-cards")
            .num_columns(columns)
            .spacing([spacing, spacing])
            .show(ui, |ui| {
                for (index, gadget) in GADGETS.iter().enumerate() {
                    ui.allocate_ui_with_layout(
                        // egui centers cells vertically; reserve the full card
                        // height so a zero-height allocation cannot add a gap.
                        Vec2::new(width, 354.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(20, 29, 43))
                                .corner_radius(8)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.set_width((width - 24.0).max(1.0));
                                    ui.set_min_height(330.0);
                                    ui.heading(gadget.title());
                                    ui.add_space(6.0);
                                    match gadget {
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
                                                    3.0..=12.0,
                                                )
                                                .text("Side range (m)"),
                                            );
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
                                            ui.label(
                                                egui::RichText::new(if *gadget == Gadget::Delta {
                                                    delta_text(&snapshot.delta)
                                                } else {
                                                    gap_text(value)
                                                })
                                                .size(28.0)
                                                .color(if *gadget == Gadget::Delta {
                                                    delta_color(&snapshot.delta)
                                                } else {
                                                    Color32::from_rgb(92, 204, 222)
                                                }),
                                            );
                                            ui.label(gap_driver(value));
                                            ui.label(&value.status);
                                            if let Some(age) = value.measured_age_ms {
                                                ui.label(format!(
                                                    "Measured {:.1} s ago",
                                                    age as f64 / 1000.0
                                                ));
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
}
const GADGETS: &[Gadget] = &[Gadget::Radar, Gadget::Ahead, Gadget::Behind, Gadget::Delta];
impl Gadget {
    fn title(self) -> &'static str {
        match self {
            Self::Radar => "Radar",
            Self::Ahead => "Gap ahead",
            Self::Behind => "Gap behind",
            Self::Delta => "Performance delta",
        }
    }
}
fn grid_columns(width: f32, card_width: f32, spacing: f32) -> usize {
    (((width + spacing) / (card_width + spacing)).floor() as usize).max(1)
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
fn gap_text(value: &GapValue) -> String {
    if let Some(laps) = value.laps {
        format!("{laps} lap{}", if laps == 1 { "" } else { "s" })
    } else if let Some(seconds) = value.seconds {
        format!("~{seconds:.1} s")
    } else {
        "—".into()
    }
}
fn delta_text(value: &crate::delta::DeltaFrame) -> String {
    value
        .seconds
        .map(|s| format!("{:+.2} s", if s.abs() < 0.005 { 0.0 } else { s }))
        .unwrap_or_else(|| "— s".into())
}
fn delta_color(value: &crate::delta::DeltaFrame) -> Color32 {
    match value.seconds {
        Some(s) if s < -0.005 => Color32::from_rgb(100, 230, 145),
        Some(s) if s > 0.005 => Color32::from_rgb(255, 120, 110),
        _ => Color32::WHITE,
    }
}
fn delta_trend(value: &crate::delta::DeltaFrame) -> &'static str {
    match value.trend {
        Some(t) if t < -0.02 => "GAINING",
        Some(t) if t > 0.02 => "LOSING",
        Some(_) => "STEADY",
        None => "",
    }
}
fn delta_value(value: &crate::delta::DeltaFrame) -> GapValue {
    GapValue {
        driver: Some(
            value
                .best_seconds
                .map(|s| format!("Session best {:.0}:{:05.2}", (s / 60.0).floor(), s % 60.0))
                .unwrap_or_else(|| "No reference lap yet".into()),
        ),
        status: format!("{} {}", delta_trend(value), value.status),
        ..Default::default()
    }
}
fn paint_delta(
    painter: &egui::Painter,
    canvas: Rect,
    value: &crate::delta::DeltaFrame,
    settings: &GapSettings,
) {
    let scale = settings
        .scale
        .min(canvas.width() / 230.0)
        .min(canvas.height() / 76.0);
    let rect = Rect::from_center_size(canvas.center(), Vec2::new(230.0, 76.0) * scale);
    painter.rect_filled(
        rect,
        6.0 * scale,
        Color32::from_rgba_unmultiplied(12, 18, 28, 210),
    );
    painter.text(
        rect.min + Vec2::new(8.0, 7.0) * scale,
        Align2::LEFT_TOP,
        format!("DELTA  {}", delta_text(value)),
        FontId::proportional(23.0 * scale),
        delta_color(value),
    );
    painter.text(
        rect.min + Vec2::new(8.0, 34.0) * scale,
        Align2::LEFT_TOP,
        delta_value(value).driver.unwrap_or_default(),
        FontId::proportional(12.0 * scale),
        Color32::WHITE,
    );
    painter.text(
        rect.min + Vec2::new(8.0, 55.0) * scale,
        Align2::LEFT_TOP,
        if value.seconds.is_some() {
            format!("{} · ESTIMATE", delta_trend(value))
        } else {
            clipped_text(&value.status, 35)
        },
        FontId::proportional(10.0 * scale),
        Color32::GRAY,
    );
    if let Some(trend) = value.trend {
        let center = rect.right() - 43.0 * scale;
        let y = rect.top() + 42.0 * scale;
        painter.line_segment(
            [
                Pos2::new(center - 28.0 * scale, y),
                Pos2::new(center + 28.0 * scale, y),
            ],
            Stroke::new(3.0 * scale, Color32::DARK_GRAY),
        );
        painter.line_segment(
            [
                Pos2::new(center, y),
                Pos2::new(center + trend.clamp(-1.0, 1.0) as f32 * 28.0 * scale, y),
            ],
            Stroke::new(
                4.0 * scale,
                if trend < 0.0 {
                    Color32::GREEN
                } else {
                    Color32::LIGHT_RED
                },
            ),
        );
    }
}
fn gap_driver(value: &GapValue) -> String {
    match (&value.driver, value.position) {
        (Some(name), Some(position)) => format!("P{position} · {name}"),
        _ => "—".into(),
    }
}
impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
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
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(12, 18, 28))
                    .inner_margin(20.0),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("LFS OpenRadar");
                ui.label(
                    egui::RichText::new(if self.demo.is_some() {
                        "DEMO / NO NETWORK CONNECTION"
                    } else {
                        "LOCAL PROXIMITY RADAR"
                    })
                    .color(Color32::from_rgb(92, 204, 222))
                    .small(),
                );
                ui.add_space(8.0);
                ui.label(&snapshot.frame.status);
                ui.horizontal(|ui| {
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
                ui.add_space(6.0);
                self.gadget_cards(ui, &snapshot);
                ui.add_space(8.0);
                self.startup_setup_controls(ui);
                if self.demo.is_none() {
                    ui.horizontal(|ui| {
                        ui.label("InSim password");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.config.insim_password.0)
                                .password(true)
                                .hint_text("LFS multiplayer admin password"),
                        );
                    });
                    ui.label(
                        egui::RichText::new(
                            "Save settings stores the password in TOML. Apply / reconnect uses it.",
                        )
                        .small(),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.overlay_enabled, "Show overlays").changed() {
                        log::info!(target: "openradar_graphics", "Show overlay {}", self.overlay_enabled);
                    }
                    if ui.checkbox(&mut self.edit_overlay, "Radar position mode").changed() {
                        log::info!(target: "openradar_graphics", "Position mode {}", self.edit_overlay);
                    }
                });
                if self.edit_overlay {
                    ui.label(egui::RichText::new("Drag the radar title bar, use Move radar, or edit X/Y.").small());
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
                position_controls(ui, &mut self.config, &mut self.overlay_window, self.edit_overlay);
                ui.add(
                    egui::Slider::new(&mut self.config.overlay_size, 220.0..=800.0)
                        .text("Radar size"),
                );
                ui.add(
                    egui::Slider::new(&mut self.config.interpolation_ms, 0..=200)
                        .text("Interpolation (ms)"),
                );
                ui.horizontal(|ui| {
                    if ui.button("Save settings").clicked() {
                        self.message = Some(match self.config.save(&self.config_path) {
                            Ok(()) => format!("Saved {}", self.config_path.display()),
                            Err(error) => error,
                        });
                    }
                    if self.demo.is_none() && ui.button("Apply / reconnect").clicked() {
                        self.connect();
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
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
                if let Some(message) = self.message.as_ref().or(snapshot.error.as_ref()) {
                    ui.colored_label(Color32::from_rgb(255, 180, 120), message);
                }
                });
            });
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
    if kind == Gadget::Delta {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(child, |ui| {
                paint_delta(
                    ui.painter(),
                    ui.max_rect(),
                    &snapshot.delta,
                    &data.config.performance_delta,
                );
            });
        overlay_painted(child, &data, feedback);
        return;
    }
    let (title, value, settings) = if kind == Gadget::Ahead {
        ("AHEAD", &snapshot.gaps.ahead, &data.config.gap_ahead)
    } else {
        ("BEHIND", &snapshot.gaps.behind, &data.config.gap_behind)
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(child, |ui| {
            paint_gap(ui.painter(), ui.max_rect(), title, value, settings);
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

fn paint_gap(
    painter: &egui::Painter,
    canvas: Rect,
    title: &str,
    value: &GapValue,
    settings: &GapSettings,
) {
    let scale = settings
        .scale
        .min(canvas.width() / 230.0)
        .min(canvas.height() / 76.0);
    let size = Vec2::new(230.0, 76.0) * scale;
    let rect = Rect::from_center_size(canvas.center(), size);
    painter.rect_filled(
        rect,
        6.0 * scale,
        Color32::from_rgba_unmultiplied(12, 18, 28, 210),
    );
    painter.text(
        Pos2::new(rect.left() + 8.0 * scale, rect.top() + 7.0 * scale),
        Align2::LEFT_TOP,
        format!("{title}  {}", gap_text(value)),
        FontId::proportional(20.0 * scale),
        Color32::from_rgb(92, 204, 222),
    );
    painter.text(
        Pos2::new(rect.left() + 8.0 * scale, rect.top() + 32.0 * scale),
        Align2::LEFT_TOP,
        clipped_text(&gap_driver(value), 28),
        FontId::proportional(13.0 * scale),
        Color32::WHITE,
    );
    let status = value
        .measured_age_ms
        .map(|age| format!("ESTIMATE · {:.1} s ago", age as f64 / 1000.0))
        .unwrap_or_else(|| clipped_text(&value.status, 32));
    painter.text(
        Pos2::new(rect.left() + 8.0 * scale, rect.top() + 53.0 * scale),
        Align2::LEFT_TOP,
        status,
        FontId::proportional(10.0 * scale),
        Color32::GRAY,
    );
}
fn clipped_text(text: &str, limit: usize) -> String {
    if text.chars().count() > limit {
        format!("{}…", text.chars().take(limit - 1).collect::<String>())
    } else {
        text.into()
    }
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
        let drag = editing.then(|| {
            ui.add(egui::Button::new("Move radar").sense(egui::Sense::drag()))
                .on_hover_cursor(egui::CursorIcon::Grab)
        });
        let delta = drag.as_ref().map_or(Vec2::ZERO, egui::Response::drag_delta);
        if delta != Vec2::ZERO {
            config.overlay_x = (config.overlay_x + delta.x).clamp(-8000.0, 8000.0);
            config.overlay_y = (config.overlay_y + delta.y).clamp(-8000.0, 8000.0);
        }
        if x.changed() || y.changed() || delta != Vec2::ZERO {
            window.set_position(config);
        }
        drag
    })
    .inner
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

fn paint(
    painter: &egui::Painter,
    rect: Rect,
    frame: &RadarFrame,
    config: &Config,
    draw_background: bool,
) {
    let size = rect.width().min(rect.height());
    let panel = Rect::from_center_size(rect.center(), Vec2::splat(size - 8.0));
    if draw_background {
        painter.rect_filled(
            panel,
            16.0,
            Color32::from_rgba_unmultiplied(13, 23, 35, 225),
        );
    }
    let centre = panel.center() + Vec2::new(0.0, 10.0);
    let scale = ((size - 60.0) / (config.front_m + config.rear_m + config.car_length_m) as f32)
        .min((size - 60.0) / (2.0 * config.side_m + config.car_width_m) as f32);
    let grid = Stroke::new(1.0_f32, Color32::from_rgb(37, 57, 74));
    for radius in [3.0, 6.0, 9.0] {
        painter.circle_stroke(centre, radius * scale, grid);
    }
    painter.line_segment(
        [
            Pos2::new(panel.left() + 15.0, centre.y),
            Pos2::new(panel.right() - 15.0, centre.y),
        ],
        grid,
    );
    painter.line_segment(
        [
            Pos2::new(centre.x, panel.top() + 30.0),
            Pos2::new(centre.x, panel.bottom() - 25.0),
        ],
        grid,
    );
    painter.text(
        Pos2::new(centre.x, panel.top() + 12.0),
        Align2::CENTER_TOP,
        "FRONT",
        FontId::proportional(10.0),
        Color32::from_rgb(119, 146, 163),
    );
    if frame.live {
        for car in &frame.cars {
            let color = if car.uncertain {
                Color32::from_rgb(109, 123, 142)
            } else {
                match car.threat {
                    Threat::Nearby => Color32::from_rgb(121, 185, 235),
                    Threat::Alongside => Color32::from_rgb(252, 193, 94),
                    Threat::PotentialContact => Color32::from_rgb(255, 99, 99),
                }
            };
            let position =
                centre + Vec2::new(car.right as f32 * scale, -car.forward as f32 * scale);
            draw_car(
                painter,
                position,
                scale,
                car.relative_heading,
                config,
                color,
            );
        }
        draw_car(
            painter,
            centre,
            scale,
            0.0,
            config,
            Color32::from_rgb(95, 231, 202),
        );
        painter.text(
            centre,
            Align2::CENTER_CENTER,
            "YOU",
            FontId::proportional(9.0),
            Color32::from_rgb(9, 32, 34),
        );
    } else {
        painter.text(
            centre,
            Align2::CENTER_CENTER,
            "RADAR PAUSED",
            FontId::proportional(14.0),
            Color32::from_rgb(160, 177, 192),
        );
    }
    painter.text(
        Pos2::new(centre.x, panel.bottom() - 10.0),
        Align2::CENTER_BOTTOM,
        if frame.live {
            "Approximate footprints"
        } else {
            "MCI + OutSim required"
        },
        FontId::proportional(10.0),
        Color32::from_rgb(119, 146, 163),
    );
}

fn draw_car(
    painter: &egui::Painter,
    centre: Pos2,
    scale: f32,
    angle: f64,
    config: &Config,
    color: Color32,
) {
    let (sin, cos) = angle.sin_cos();
    let points = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)].map(|(x, y)| {
        let x = x * config.car_width_m;
        let y = y * config.car_length_m;
        centre
            + Vec2::new(
                (x * cos - y * sin) as f32 * scale,
                -(x * sin + y * cos) as f32 * scale,
            )
    });
    painter.add(Shape::convex_polygon(
        points.to_vec(),
        color.gamma_multiply(0.85),
        Stroke::new(1.5_f32, color),
    ));
    let nose = centre
        + Vec2::new(
            (-sin * config.car_length_m * 0.35) as f32 * scale,
            (-cos * config.car_length_m * 0.35) as f32 * scale,
        );
    painter.circle_filled(nose, 2.0, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radar::RadarCar;

    #[test]
    fn cards_wrap_left_to_right_and_reflow_when_resized() {
        let ctx = egui::Context::default();
        let mut app = App::new(
            Config::default(),
            PathBuf::from("unused.toml"),
            true,
            None,
            None,
        );
        let snapshot = Demo::default().snapshot(7_000, &app.config);
        for (width, expected_columns) in [(340.0, 1), (680.0, 2), (1020.0, 3), (340.0, 1)] {
            let mut output = egui::FullOutput::default();
            // Give egui's remembered grid measurements time to settle after reflow.
            for _ in 0..3 {
                output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(width, 1500.0),
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
            assert_eq!(cards.len(), 4);
            assert!(
                cards[0].top() < 20.0,
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
    fn gap_panels_fit_their_own_viewport_at_extreme_scales() {
        let ctx = egui::Context::default();
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
                                    &GapSettings {
                                        enabled: true,
                                        scale,
                                        ..Default::default()
                                    },
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
        ctx.set_embed_viewports(false);
        let mut config = Config {
            radar_enabled: false,
            ..Default::default()
        };
        config.gap_ahead.enabled = true;
        config.gap_behind.enabled = true;
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
        let callbacks: Vec<_> = ["gap-ahead-overlay", "gap-behind-overlay"]
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
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(230.0, 76.0))),
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
    fn startup_setup_conflict_requires_a_choice_and_adopting_port_leaves_script_untouched() {
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
        let config = Config {
            lfs_directory: folder.display().to_string(),
            ..Default::default()
        };
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        let ctx = egui::Context::default();
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
    fn delta_viewport_refreshes_and_closes_independently() {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut config = Config::default();
        config.performance_delta.enabled = true;
        config.gap_ahead.enabled = true;
        let mut app = App::new(config, PathBuf::from("unused.toml"), true, None, None);
        app.gap_overlays[2].editing = true;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.show_gap_overlays(ctx, true)
        });
        let id = ViewportId::from_hash_of("performance-delta-overlay");
        assert_eq!(output.viewport_output[&id].builder.decorations, Some(true));
        let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
        app.set_source(OverlaySource::Demo(Arc::new(Mutex::new(Demo::default()))));
        app.gap_overlays[2].data.lock().unwrap().started =
            Instant::now() - Duration::from_secs(201);
        let parent_passes = ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        let mut input = egui::RawInput {
            viewport_id: id,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(230.0, 76.0))),
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
    fn panel_drag_fallback_preserves_clicks_without_a_native_drag_loop() {
        let ctx = egui::Context::default();
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
        ctx.set_embed_viewports(false);
        let config = Config::default();
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
        assert_eq!(ctx.cumulative_pass_nr_for(ViewportId::ROOT), parent_passes);

        data.lock().unwrap().visible = false;
        assert!(draw_child().shapes.is_empty());
    }

    #[test]
    fn range_and_size_extremes_have_finite_bounded_meshes_without_a_gpu() {
        let ctx = egui::Context::default();
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
