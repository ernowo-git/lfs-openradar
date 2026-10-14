use super::*;
use crate::overlay::session::Monitor;
use eframe::{egui, egui_wgpu};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{EventLoop, ping::PingSource},
        calloop_wayland_source::WaylandSource,
        protocols::{
            wp::fractional_scale::v1::client::{
                wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
                wp_fractional_scale_v1::{self, WpFractionalScaleV1},
            },
            wp::viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::{
    ptr::NonNull,
    time::{Duration, Instant},
};
use wayland_client::{
    Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};

pub(super) fn supports_layer_shell(connection: &Connection) -> bool {
    registry_queue_init::<Worker>(connection).is_ok_and(|(globals, _queue)| {
        globals.contents().with_list(|list| {
            ["wl_compositor", "zwlr_layer_shell_v1"]
                .iter()
                .all(|interface| {
                    list.iter()
                        .any(|global| global.interface == *interface && global.version >= 1)
                })
        })
    })
}

pub(super) fn run(
    connection: Connection,
    session: Arc<Mutex<OverlaySession>>,
    source: Arc<Mutex<SnapshotSource>>,
    stop: Arc<AtomicBool>,
    repaint: egui::Context,
    ping: PingSource,
) -> Result<(), String> {
    let (globals, queue) = registry_queue_init::<Worker>(&connection)
        .map_err(|e| format!("cannot enumerate Wayland protocols: {e}"))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| format!("cannot bind Wayland compositor: {e}"))?;
    let shell =
        LayerShell::bind(&globals, &qh).map_err(|e| format!("layer shell unavailable: {e}"))?;
    let mut state = Worker {
        connection: connection.clone(),
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        shell,
        fractional_manager: globals.bind(&qh, 1..=1, ()).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        layer: None,
        fractional: None,
        viewport: None,
        gpu: None,
        selected: None,
        scale: 1.0,
        configured: false,
        frame_pending: false,
        force: false,
        last_visible: false,
        session,
        source,
        repaint,
        started: Instant::now(),
        logical: [0, 0],
        fatal: None,
    };
    let mut event_loop = EventLoop::<Worker>::try_new()
        .map_err(|e| format!("cannot start overlay event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(event_loop.handle())
        .map_err(|e| format!("cannot register Wayland connection: {e}"))?;
    event_loop
        .handle()
        .insert_source(ping, |_, _, state| {
            state.force = true;
        })
        .map_err(|e| format!("cannot register overlay wakeup: {e}"))?;
    while !stop.load(Ordering::Acquire) {
        event_loop
            .dispatch(Duration::from_millis(16), &mut state)
            .map_err(|e| format!("native Wayland dispatch failed: {e}"))?;
        state.sync(&qh)?;
        if let Some(error) = state.fatal.take() {
            return Err(error);
        }
        if state.configured && (state.force || !state.frame_pending) {
            state.draw(&qh)?;
        }
    }
    state.clear_surface();
    Ok(())
}

struct Worker {
    // GPU surface is explicitly dropped before the layer and connection.
    gpu: Option<Gpu>,
    connection: Connection,
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    shell: LayerShell,
    fractional_manager: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    layer: Option<LayerSurface>,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    selected: Option<String>,
    scale: f32,
    logical: [u32; 2],
    configured: bool,
    frame_pending: bool,
    force: bool,
    last_visible: bool,
    session: Arc<Mutex<OverlaySession>>,
    source: Arc<Mutex<SnapshotSource>>,
    repaint: egui::Context,
    started: Instant,
    fatal: Option<String>,
}
impl Worker {
    fn clear_surface(&mut self) {
        self.gpu = None;
        if let Some(fractional) = self.fractional.take() {
            fractional.destroy();
        }
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
        self.layer = None;
        self.configured = false;
        self.frame_pending = false;
    }
    fn sync(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        let outputs: Vec<_> = self
            .outputs
            .outputs()
            .filter_map(|output| {
                let info = self.outputs.info(&output)?;
                let name = info.name?;
                let size = info.logical_size.or_else(|| {
                    info.modes.iter().find(|m| m.current).map(|m| {
                        (
                            m.dimensions.0 / info.scale_factor.max(1),
                            m.dimensions.1 / info.scale_factor.max(1),
                        )
                    })
                })?;
                if size.0 <= 0 || size.1 <= 0 {
                    return None;
                }
                let scale = if self.selected.as_ref() == Some(&name) && self.fractional.is_some() {
                    self.scale
                } else {
                    info.scale_factor.max(1) as f32
                };
                Some((
                    output,
                    Monitor {
                        name,
                        size: [size.0 as f32, size.1 as f32],
                        scale,
                    },
                ))
            })
            .collect();
        let status = {
            let mut session = self.session.lock().unwrap();
            session.outputs_changed(outputs.iter().map(|(_, m)| m.clone()).collect());
            session.status()
        };
        let target = outputs
            .iter()
            .find(|(_, m)| Some(&m.name) == status.selected_monitor.as_ref());
        let available_name = target.map(|(_, m)| m.name.clone());
        if available_name != self.selected {
            self.clear_surface();
            self.selected = available_name;
            if self.selected.is_none() {
                self.fatal = None;
            }
            if let Some((output, monitor)) = target {
                self.scale = monitor.scale;
                let surface = self.compositor.create_surface(qh);
                let region = Region::new(&self.compositor)
                    .map_err(|e| format!("cannot create input region: {e}"))?;
                surface.set_input_region(Some(region.wl_region()));
                let layer = self.shell.create_layer_surface(
                    qh,
                    surface,
                    Layer::Overlay,
                    Some("lfs-openradar-native"),
                    Some(output),
                );
                layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
                layer.set_size(0, 0);
                layer.set_exclusive_zone(-1);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                if let (Some(manager), Some(viewporter)) =
                    (&self.fractional_manager, &self.viewporter)
                {
                    self.fractional =
                        Some(manager.get_fractional_scale(layer.wl_surface(), qh, ()));
                    self.viewport = Some(viewporter.get_viewport(layer.wl_surface(), qh, ()));
                }
                layer.commit();
                self.layer = Some(layer);
            }
            self.repaint.request_repaint();
        }
        if status.visible != self.last_visible {
            self.force = true;
            self.repaint.request_repaint();
        }
        Ok(())
    }
    fn draw(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        let presentation = self.session.lock().unwrap().clone();
        let status = presentation.status();
        if !status.visible && !self.last_visible {
            self.force = false;
            return Ok(());
        }
        let Some(layer) = &self.layer else {
            return Ok(());
        };
        let physical = [
            (self.logical[0] as f32 * self.scale).ceil() as u32,
            (self.logical[1] as f32 * self.scale).ceil() as u32,
        ];
        if physical.contains(&0) {
            return Ok(());
        }
        if let Some(viewport) = &self.viewport {
            layer.wl_surface().set_buffer_scale(1);
            viewport.set_destination(self.logical[0] as i32, self.logical[1] as i32);
        } else {
            layer.wl_surface().set_buffer_scale(self.scale as i32);
        }
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::new(&self.connection, layer.wl_surface(), physical)?);
        }
        let source = self.source.lock().unwrap().clone();
        let snapshot = source.snapshot(
            self.started.elapsed().as_millis() as u64,
            &presentation.config,
        );
        let gpu = self.gpu.as_mut().unwrap();
        let presented = gpu.draw(physical, &presentation, &snapshot)?;
        if !presented {
            self.force = true;
            return Ok(());
        }
        if !self.frame_pending {
            layer.wl_surface().frame(qh, layer.wl_surface().clone());
            self.frame_pending = true;
            layer.commit();
        }
        self.force = false;
        self.last_visible = status.visible;
        let first = !self.session.lock().unwrap().status().ready;
        self.session.lock().unwrap().presented();
        if first {
            log::info!(target: "openradar_graphics", "Native Wayland GPU ready: output {:?}, logical {:?}, physical {:?}, scale {}, click-through", self.selected, self.logical, physical, self.scale);
            self.repaint.request_repaint();
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.clear_surface();
    }
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: egui_wgpu::Renderer,
    context: egui::Context,
}
impl Gpu {
    fn new(
        connection: &Connection,
        surface: &wl_surface::WlSurface,
        size: [u32; 2],
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let display = NonNull::new(connection.backend().display_ptr().cast())
            .ok_or("native Wayland display pointer unavailable")?;
        let window = NonNull::new(surface.id().as_ptr().cast())
            .ok_or("native Wayland surface pointer unavailable")?;
        // SAFETY: Worker owns the connection and layer for the entire GPU lifetime,
        // and clear_surface drops Gpu before destroying either Wayland resource.
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: wgpu::rwh::RawDisplayHandle::Wayland(
                    wgpu::rwh::WaylandDisplayHandle::new(display),
                ),
                raw_window_handle: wgpu::rwh::RawWindowHandle::Wayland(
                    wgpu::rwh::WaylandWindowHandle::new(window),
                ),
            })
        }
        .map_err(|e| format!("cannot create native GPU surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| format!("cannot find native GPU adapter: {e}"))?;
        let capabilities = surface.get_capabilities(&adapter);
        if !capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            return Err("native GPU surface does not support premultiplied transparency".into());
        }
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|e| format!("cannot create native GPU device: {e}"))?;
        let mut config = surface
            .get_default_config(&adapter, size[0], size[1])
            .ok_or("native GPU surface has no supported configuration")?;
        config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        config.format = capabilities
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .unwrap_or(config.format);
        if size
            .iter()
            .any(|v| *v > device.limits().max_texture_dimension_2d)
        {
            return Err("native overlay exceeds GPU texture limits".into());
        }
        surface.configure(&device, &config);
        let renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );
        let context = egui::Context::default();
        context.set_visuals(egui::Visuals::dark());
        Ok(Self {
            surface,
            device,
            queue,
            config,
            renderer,
            context,
        })
    }
    fn draw(
        &mut self,
        size: [u32; 2],
        presentation: &OverlaySession,
        snapshot: &crate::runtime::Snapshot,
    ) -> Result<bool, String> {
        if size != [self.config.width, self.config.height] {
            if size
                .iter()
                .any(|v| *v > self.device.limits().max_texture_dimension_2d)
            {
                return Err("native overlay exceeds GPU texture limits".into());
            }
            self.config.width = size[0];
            self.config.height = size[1];
            self.surface.configure(&self.device, &self.config);
        }
        let output = presentation.paint(&self.context, snapshot);
        let jobs = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        for (id, delta) in &output.textures_delta.set {
            self.renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Timeout) => return Ok(false),
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(false);
            }
            Err(error) => return Err(format!("native GPU presentation failed: {error}")),
        };
        let view = frame.texture.create_view(&Default::default());
        let descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: output.pixels_per_point,
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let commands = self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &jobs,
            &descriptor,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("native HUD"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &jobs, &descriptor);
        }
        self.queue.submit(
            commands
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        frame.present();
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        Ok(true)
    }
}

impl LayerShellHandler for Worker {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.layer.as_ref() == Some(layer) {
            self.fatal = Some("native layer surface was closed; retry presentation".into());
        }
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if self.layer.as_ref() != Some(layer) {
            return;
        }
        self.logical = [configure.new_size.0, configure.new_size.1];
        self.configured = true;
        self.force = true;
    }
}
impl CompositorHandler for Worker {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if self.fractional.is_none() {
            self.scale = scale.max(1) as f32;
            self.force = true;
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self
            .layer
            .as_ref()
            .is_some_and(|l| l.wl_surface() == surface)
        {
            self.frame_pending = false;
        }
    }
}
impl OutputHandler for Worker {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.force = true;
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.force = true;
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.force = true;
    }
}
impl ProvidesRegistryState for Worker {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}
impl Dispatch<WpFractionalScaleV1, ()> for Worker {
    fn event(
        state: &mut Self,
        proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.fractional.as_ref() == Some(proxy)
            && let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
        {
            state.scale = (scale as f32 / 120.0).max(0.1);
            state.force = true;
        }
    }
}
delegate_noop!(Worker: ignore WpFractionalScaleManagerV1);
delegate_noop!(Worker: ignore WpViewporter);
delegate_noop!(Worker: ignore WpViewport);
delegate_compositor!(Worker);
delegate_layer!(Worker);
delegate_output!(Worker);
delegate_registry!(Worker);
