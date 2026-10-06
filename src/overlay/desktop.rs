use super::window::OverlayWindow;
use crate::{
    config::Config,
    demo::Demo,
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
            .with_inner_size([480.0, 700.0])
            .with_min_inner_size([460.0, 660.0])
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
    insim_password: String,
    config_path: PathBuf,
    runtime: Option<Runtime>,
    demo: Option<Demo>,
    started: Instant,
    seconds: Option<u64>,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    message: Option<String>,
    overlay_enabled: bool,
    edit_overlay: bool,
    overlay_created: bool,
    overlay_window: OverlayWindow,
    overlay_feedback: Arc<Mutex<OverlayFeedback>>,
    overlay_data: Arc<Mutex<OverlayData>>,
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
impl App {
    fn new(
        config: Config,
        config_path: PathBuf,
        demo: bool,
        seconds: Option<u64>,
        screenshot: Option<PathBuf>,
    ) -> Self {
        let started = Instant::now();
        let mut app = Self {
            overlay_data: Arc::new(Mutex::new(OverlayData {
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
            })),
            overlay_window: OverlayWindow::new(&config),
            overlay_feedback: Arc::new(Mutex::new(OverlayFeedback::default())),
            overlay_created: false,
            config,
            insim_password: std::env::var("LFS_INSIM_ADMIN").unwrap_or_default(),
            config_path,
            runtime: None,
            demo: demo.then(Demo::default),
            started,
            seconds,
            screenshot,
            screenshot_requested: false,
            message: None,
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
        self.overlay_data.lock().unwrap().source = OverlaySource::Disconnected;
        self.runtime = None;
        match Runtime::start_with_password(self.config.clone(), self.insim_password.clone()) {
            Ok(runtime) => {
                self.overlay_data.lock().unwrap().source =
                    OverlaySource::Live(runtime.snapshot_reader());
                self.runtime = Some(runtime);
                self.message = None;
            }
            Err(error) => self.message = Some(error),
        }
    }
    fn capture(&mut self, ctx: &egui::Context) {
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && self.started.elapsed().as_secs_f32() > 1.0
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
}
impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Ok(mut feedback) = self.overlay_feedback.lock() {
            if let Some(position) = feedback.moved_to.take() {
                // Save the observed position, without commanding an OS move
                // back to it during the next frame.
                self.config.overlay_x = position.x;
                self.config.overlay_y = position.y;
            }
            if feedback.closed {
                self.overlay_enabled = false;
                feedback.closed = false;
            }
        }
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
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 280.0),
                    egui::Sense::hover(),
                );
                paint(ui.painter(), rect, &snapshot.frame, &self.config, true);
                ui.add_space(8.0);
                if self.demo.is_none() {
                    ui.horizontal(|ui| {
                        ui.label("InSim password");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.insim_password)
                                .password(true)
                                .hint_text("LFS multiplayer admin password"),
                        );
                    });
                    ui.label(
                        egui::RichText::new(
                            "Password stays in memory. Enter it, then Apply / reconnect.",
                        )
                        .small(),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.overlay_enabled, "Show overlay").changed() {
                        log::info!(target: "openradar_graphics", "Show overlay {}", self.overlay_enabled);
                    }
                    if ui.checkbox(&mut self.edit_overlay, "Position mode").changed() {
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
                        .text("Overlay size"),
                );
                ui.add(
                    egui::Slider::new(&mut self.config.side_m, 3.0..=12.0).text("Side range (m)"),
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
        let foreground = super::game_foreground().unwrap_or(true);
        let show_overlay = self.overlay_enabled
            && (self.edit_overlay
                || self.demo.is_some()
                || !self.config.hide_when_background
                || foreground);
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
        }
    }
    child.request_repaint_after(Duration::from_millis(16));
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
