//! Native Wayland Layer Shell proof of concept; see docs/wayland-mvp.md.

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = linux::run() {
        eprintln!("wayland overlay: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("wayland overlay: this prototype requires Linux with a Layer Shell compositor");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod linux {
    use std::time::{Duration, Instant};

    use smithay_client_toolkit::{
        compositor::{CompositorHandler, CompositorState, Region},
        delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
        output::{OutputHandler, OutputState},
        reexports::{calloop::EventLoop, calloop_wayland_source::WaylandSource},
        registry::{ProvidesRegistryState, RegistryState},
        registry_handlers,
        shell::{
            WaylandSurface,
            wlr_layer::{
                Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
                LayerSurfaceConfigure,
            },
        },
        shm::{Shm, ShmHandler, slot::SlotPool},
    };
    use wayland_client::{
        Connection, QueueHandle,
        globals::registry_queue_init,
        protocol::{wl_output, wl_shm, wl_surface},
    };

    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 240;

    pub fn run() -> Result<(), String> {
        let mut seconds = 60;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!(
                        "Wayland overlay MVP\n\n  --seconds N    Run for 1..3600 seconds (default: 60)\n\nThree animated mock gadgets on one transparent, click-through Overlay surface.\nNo LFS connection or window rules needed. Stop with Ctrl+C in the terminal."
                    );
                    return Ok(());
                }
                "--seconds" => {
                    seconds = args
                        .next()
                        .ok_or("--seconds needs a number")?
                        .parse::<u64>()
                        .map_err(|_| "--seconds must be 1..3600")?;
                    if !(1..=3600).contains(&seconds) {
                        return Err("--seconds must be 1..3600".into());
                    }
                }
                _ => return Err(format!("unknown option: {arg}; use --help")),
            }
        }

        let connection = Connection::connect_to_env()
            .map_err(|e| format!("cannot connect to the native Wayland session: {e}"))?;
        let (globals, queue) = registry_queue_init(&connection)
            .map_err(|e| format!("cannot enumerate Wayland protocols: {e}"))?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh).map_err(|e| e.to_string())?;
        let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| {
            format!(
                "zwlr_layer_shell_v1 is unavailable: {e}; use a compositor with Layer Shell support (GNOME Wayland is unsupported)"
            )
        })?;
        let shm = Shm::bind(&globals, &qh).map_err(|e| e.to_string())?;
        let surface = compositor.create_surface(&qh);
        // Some(empty), rather than None, makes even painted pixels click-through.
        let input_region = Region::new(&compositor).map_err(|e| e.to_string())?;
        surface.set_input_region(Some(input_region.wl_region()));
        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Overlay,
            Some("lfs-openradar-wayland-mvp"),
            None,
        );
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(40, 0, 0, 40);
        layer.set_size(WIDTH, HEIGHT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        // Ignore panels' exclusive zones and reserve no desktop space ourselves.
        layer.set_exclusive_zone(-1);
        // The first commit must have no buffer; draw only after configure/ack.
        layer.commit();

        let mut state = Overlay {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            pool: SlotPool::new((WIDTH * HEIGHT * 4) as usize, &shm).map_err(|e| e.to_string())?,
            shm,
            layer,
            width: WIDTH,
            height: HEIGHT,
            configured: false,
            closed: false,
            error: None,
            start: Instant::now(),
            frames: 0,
        };
        let mut event_loop = EventLoop::<Overlay>::try_new().map_err(|e| e.to_string())?;
        WaylandSource::new(connection, queue)
            .insert(event_loop.handle())
            .map_err(|e| e.to_string())?;
        println!(
            "Layer Shell available; requesting one 640x240 Overlay surface at (40,40), click-through, no keyboard focus; running for {seconds}s"
        );
        while !state.closed && state.error.is_none() && state.start.elapsed().as_secs() < seconds {
            // A timeout also exits while hidden, when frame callbacks may stop.
            event_loop
                .dispatch(Duration::from_millis(100), &mut state)
                .map_err(|e| format!("Wayland event dispatch failed: {e}"))?;
        }
        if let Some(error) = state.error {
            return Err(error);
        }
        if state.frames == 0 {
            return Err("compositor did not configure/present the overlay before exit".into());
        }
        println!(
            "finished: {} buffer commits in {:.1}s (visual stacking and input need manual verification)",
            state.frames,
            state.start.elapsed().as_secs_f32()
        );
        Ok(())
    }

    struct Overlay {
        registry: RegistryState,
        outputs: OutputState,
        shm: Shm,
        pool: SlotPool,
        layer: LayerSurface,
        width: u32,
        height: u32,
        configured: bool,
        closed: bool,
        error: Option<String>,
        start: Instant,
        frames: u64,
    }

    impl Overlay {
        fn draw(&mut self, qh: &QueueHandle<Self>) {
            if self.closed || self.error.is_some() {
                return;
            }
            if let Err(error) = self.present(qh) {
                self.error = Some(format!("cannot draw overlay: {error}"));
            }
        }

        fn present(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
            let width = self.width as i32;
            let height = self.height as i32;
            // SlotPool reclaims allocations once the compositor releases them.
            let (buffer, canvas) = self
                .pool
                .create_buffer(width, height, width * 4, wl_shm::Format::Argb8888)
                .map_err(|e| e.to_string())?;
            paint(
                canvas,
                self.width,
                self.height,
                self.start.elapsed().as_secs_f32(),
            );
            let surface = self.layer.wl_surface();
            surface.damage_buffer(0, 0, width, height);
            surface.frame(qh, surface.clone());
            buffer.attach_to(surface).map_err(|e| e.to_string())?;
            self.layer.commit();
            self.frames += 1;
            if self.frames == 1 {
                println!(
                    "configured {}x{}; first transparent buffer committed",
                    width, height
                );
            }
            Ok(())
        }
    }

    // Mock graphics isolate the protocol proof from telemetry and GPU setup.
    // ARGB8888 uses native-endian, premultiplied channels (RGB <= alpha).
    fn paint(canvas: &mut [u8], width: u32, height: u32, seconds: f32) {
        let marker = (120.0 + seconds.cos() * 65.0, 120.0 + seconds.sin() * 65.0);
        let bars = [
            290.0 + (seconds.sin() + 1.0) * 150.0,
            290.0 + ((seconds * 0.7).cos() + 1.0) * 150.0,
        ];
        for (index, pixel) in canvas.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let x = (index as u32 % width) as f32 * WIDTH as f32 / width as f32;
            let y = (index as u32 / width) as f32 * HEIGHT as f32 / height as f32;
            let distance = (x - 120.0).powi(2) + (y - 120.0).powi(2);
            let color: u32 = if (x - marker.0).abs() < 7.0 && (y - marker.1).abs() < 12.0 {
                0xffffcc44
            } else if (x - 120.0).abs() < 7.0 && (y - 120.0).abs() < 12.0 {
                0xffffffff
            } else if (distance - 95.0_f32.powi(2)).abs() < 200.0
                || (distance - 55.0_f32.powi(2)).abs() < 100.0
            {
                0xff51d6e8
            } else if distance < 95.0_f32.powi(2) {
                0x60101820
            } else if (280.0..620.0).contains(&x)
                && ((40.0..104.0).contains(&y) || (136.0..200.0).contains(&y))
            {
                let panel = usize::from(y >= 136.0);
                let local_y = y - [40.0, 136.0][panel];
                if (290.0..bars[panel]).contains(&x) && (20.0..44.0).contains(&local_y) {
                    [0xff51d6e8, 0xff70df90][panel]
                } else {
                    0xa0121c24
                }
            } else {
                0
            };
            *pixel = color.to_ne_bytes();
        }
    }

    impl LayerShellHandler for Overlay {
        fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
            self.closed = true;
        }

        fn configure(
            &mut self,
            _: &Connection,
            qh: &QueueHandle<Self>,
            _: &LayerSurface,
            configure: LayerSurfaceConfigure,
            _: u32,
        ) {
            // SCTK acknowledges configure before calling this handler.
            self.width = if configure.new_size.0 == 0 {
                WIDTH
            } else {
                configure.new_size.0
            };
            self.height = if configure.new_size.1 == 0 {
                HEIGHT
            } else {
                configure.new_size.1
            };
            if self.width > 4096 || self.height > 4096 {
                self.error =
                    Some("configured overlay dimensions exceed the MVP limit (4096)".into());
                return;
            }
            if !self.configured {
                self.configured = true;
                self.draw(qh);
            }
        }
    }

    impl CompositorHandler for Overlay {
        // ponytail: compositor scales a 1x buffer; add HiDPI rendering after the stacking proof.
        fn scale_factor_changed(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            _: &wl_surface::WlSurface,
            _: i32,
        ) {
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
            qh: &QueueHandle<Self>,
            _: &wl_surface::WlSurface,
            _: u32,
        ) {
            self.draw(qh);
        }
    }

    impl OutputHandler for Overlay {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.outputs
        }
        fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
        fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        }
        fn output_destroyed(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            _: wl_output::WlOutput,
        ) {
        }
    }

    impl ShmHandler for Overlay {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }

    impl ProvidesRegistryState for Overlay {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry
        }
        registry_handlers![OutputState];
    }

    delegate_compositor!(Overlay);
    delegate_output!(Overlay);
    delegate_shm!(Overlay);
    delegate_layer!(Overlay);
    delegate_registry!(Overlay);

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn mock_gadgets_have_transparent_gaps_premultiplied_alpha_and_animation() {
            let mut first = vec![0; (WIDTH * HEIGHT * 4) as usize];
            let mut later = first.clone();
            paint(&mut first, WIDTH, HEIGHT, 0.0);
            paint(&mut later, WIDTH, HEIGHT, 1.0);
            let pixel = |x, y| {
                let offset = ((y * WIDTH + x) * 4) as usize;
                u32::from_ne_bytes(first[offset..offset + 4].try_into().unwrap())
            };
            assert_eq!(pixel(0, 0), 0);
            assert_eq!(pixel(260, 120), 0); // Gap between gadgets.
            assert_eq!(pixel(120, 120), 0xffffffff);
            assert_ne!(pixel(300, 60), 0);
            assert_ne!(pixel(300, 160), 0);
            assert_ne!(first, later);
            for bytes in first.as_chunks::<4>().0 {
                let color = u32::from_ne_bytes(*bytes);
                let alpha = color >> 24;
                for shift in [0, 8, 16] {
                    assert!((color >> shift) & 255 <= alpha);
                }
            }
        }
    }
}
