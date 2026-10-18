//! Socket workers publish a bounded latest frame; rendering never reads sockets.
use crate::{
    config::Config,
    gaps::GapFrame,
    lfs::{
        insim::{self, Framer, Packet},
        outgauge, outsim,
    },
    radar::{Engine, RadarFrame},
};
use std::{
    io::{Read, Write},
    net::{TcpStream, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub connected: bool,
    pub version: String,
    pub frame: RadarFrame,
    pub gaps: GapFrame,
    pub delta: crate::delta::DeltaFrame,
    pub dashboard: crate::dashboard::DashboardFrame,
    pub fuel: crate::fuel::FuelFrame,
    pub lap_records: crate::lap_records::LapRecordsFrame,
    pub lap_log: crate::lap_log::LapLogFrame,
    pub lap_records_error: Option<String>,
    pub outgauge_error: Option<String>,
    pub outgauge_forward_error: Option<String>,
    pub error: Option<String>,
    pub mci_sets: u64,
    pub outsim_samples: u64,
    pub rejected_outsim: u64,
    pub malformed_packets: u64,
}

/// Read-only access to the worker's latest frame for independently painted
/// windows. Cloning this handle does not create another telemetry connection.
#[derive(Clone)]
pub struct SnapshotReader(Arc<Mutex<Snapshot>>);
impl SnapshotReader {
    pub fn snapshot(&self) -> Snapshot {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Clone, Copy)]
enum VisibilityCommand {
    Set(bool),
    Toggle,
}

pub struct Runtime {
    latest: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    config: Config,
    recording_sink: Arc<Mutex<Option<crate::recording::Sink>>>,
    recorder: Option<crate::recording::Recorder>,
    last_recording: Option<crate::recording::Status>,
    overlay_visibility: Arc<Mutex<Option<VisibilityCommand>>>,
}

impl Runtime {
    pub fn start(config: Config) -> Result<Self, String> {
        let password = config.effective_insim_password();
        Self::start_with_password(config, password)
    }

    /// An explicit password overrides the TOML/environment value for this connection.
    pub fn start_with_password(config: Config, password: String) -> Result<Self, String> {
        Self::start_with_store(config, password, None)
    }

    pub fn start_with_store(
        config: Config,
        password: String,
        store: Option<crate::config::SharedConfigStore>,
    ) -> Result<Self, String> {
        Self::start_with_session(config, password, store, Default::default())
    }

    pub fn start_with_session(
        config: Config,
        password: String,
        store: Option<crate::config::SharedConfigStore>,
        lap_log: crate::lap_log::SharedLapLog,
    ) -> Result<Self, String> {
        config.validate()?;
        let initial = if config.insim_enabled {
            Some(insim::init(config.mci_interval_ms, &password)?)
        } else {
            None
        };
        let udp = UdpSocket::bind(config.outsim_bind).map_err(|e| {
            format!(
                "Cannot bind OutSim {}: {e}. Check other telemetry apps.",
                config.outsim_bind
            )
        })?;
        udp.set_nonblocking(true).map_err(|e| e.to_string())?;
        let latest = Arc::new(Mutex::new(Snapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let recording_sink = Arc::new(Mutex::new(None));
        let worker_sink = recording_sink.clone();
        let saved_config = config.clone();
        let output = latest.clone();
        let quit = stop.clone();
        let overlay_visibility = Arc::new(Mutex::new(None));
        let worker_visibility = overlay_visibility.clone();
        let worker = thread::Builder::new()
            .name("lfs-telemetry".into())
            .spawn(move || {
                run(
                    config,
                    initial,
                    udp,
                    output,
                    quit,
                    store,
                    lap_log,
                    worker_sink,
                    worker_visibility,
                );
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            latest,
            stop,
            worker: Some(worker),
            config: saved_config,
            recording_sink,
            recorder: None,
            last_recording: None,
            overlay_visibility,
        })
    }
    pub fn start_recording(&mut self, options: crate::recording::Options) -> Result<(), String> {
        if self.recorder.is_some() {
            return Err("a recording is already open; stop it first".into());
        }
        if self.config.outsim_bind == self.config.outgauge_bind
            && matches!(
                outsim::packet_size(self.config.outsim_options, self.config.expected_outsim_id())?,
                92 | 96
            )
        {
            return Err(
                "recording needs separate UDP ports for ambiguous OutSim/OutGauge packet sizes"
                    .into(),
            );
        }
        let recorder = crate::recording::Recorder::start(&self.config, options)?;
        *self.recording_sink.lock().unwrap() = Some(recorder.sink.clone());
        self.recorder = Some(recorder);
        Ok(())
    }
    pub fn stop_recording(&mut self) -> Option<crate::recording::Status> {
        self.recording_sink.lock().unwrap().take();
        if let Some(recorder) = self.recorder.take() {
            self.last_recording = Some(recorder.finish());
        }
        self.last_recording.clone()
    }
    pub fn recording_status(&self) -> Option<crate::recording::Status> {
        self.recorder
            .as_ref()
            .map(|r| r.status())
            .or_else(|| self.last_recording.clone())
    }
    pub fn recording_open(&self) -> bool {
        self.recorder.is_some()
    }
    pub fn snapshot(&self) -> Snapshot {
        self.latest.lock().unwrap().clone()
    }
    pub fn snapshot_reader(&self) -> SnapshotReader {
        SnapshotReader(self.latest.clone())
    }
    /// Apply pending `/o show`, `/o hide`, and `/o hud` requests once to the
    /// panel's current visibility. Multiple requests retain their combined effect.
    pub fn apply_overlay_visibility(&self, visible: bool) -> bool {
        match self.overlay_visibility.lock().unwrap().take() {
            Some(VisibilityCommand::Set(value)) => value,
            Some(VisibilityCommand::Toggle) => !visible,
            None => visible,
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.stop_recording();
    }
}

fn request_roster(stream: &mut TcpStream) -> std::io::Result<()> {
    for subtype in [13, 14, 7, 19] {
        stream.write_all(&insim::tiny(subtype, 1))?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn receive_outgauge(
    bytes: &[u8],
    config: &Config,
    engine: &mut Engine,
    now: u64,
    socket: &UdpSocket,
    dashboard: &mut crate::dashboard::DashboardTelemetry,
    stats: &mut Snapshot,
    recording: &Mutex<Option<crate::recording::Sink>>,
) {
    match outgauge::decode(bytes, config.outgauge_id) {
        Ok(sample) => {
            if let Some(sink) = recording.lock().unwrap().as_ref() {
                sink.packet(crate::recording::Stream::Outgauge, sample.time_ms, bytes);
            }
            stats.outgauge_forward_error = None;
            for destination in &config.outgauge_forward {
                if let Err(error) = socket.send_to(bytes, destination) {
                    stats.outgauge_forward_error.get_or_insert_with(|| {
                        format!("outgauge forwarding to {destination}: {error}")
                    });
                }
            }
            // The engine has no dashboard player until InSim is ready.
            if dashboard.receive(
                sample.clone(),
                now,
                engine.dashboard_player(),
                config.stale_ms,
            ) {
                engine.fuel_sample(&sample, now, config);
            }
            stats.outgauge_error = None;
        }
        Err(error) => {
            if let Some(sink) = recording.lock().unwrap().as_ref() {
                sink.rejected(crate::recording::Stream::Outgauge, error.clone());
            }
            stats.outgauge_error = Some(error);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    config: Config,
    initial: Option<[u8; 44]>,
    udp: UdpSocket,
    output: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    store: Option<crate::config::SharedConfigStore>,
    lap_log: crate::lap_log::SharedLapLog,
    recording: Arc<Mutex<Option<crate::recording::Sink>>>,
    overlay_visibility: Arc<Mutex<Option<VisibilityCommand>>>,
) {
    let start = Instant::now();
    let mut engine = Engine::new(config.follow_viewed_car);
    engine.lap_records = crate::lap_records::Recorder::with_records(
        store
            .as_ref()
            .map(|s| s.lock().unwrap().records.clone())
            .unwrap_or_else(|| config.track_records.clone()),
    );
    engine.lap_records.lap_log = lap_log;
    let mut retry_records_at = 0;
    let mut stats = Snapshot::default();
    let mut dashboard = crate::dashboard::DashboardTelemetry::default();
    let mut gauge = None;
    let mut gauge_retry_at = 0;
    let mut connection_attempt: Option<std::sync::mpsc::Receiver<Result<TcpStream, String>>> = None;
    let mut connection: Option<TcpStream> = None;
    let mut framer = Framer::default();
    let mut retry_at = 0;
    let mut received_at = 0;
    let mut connected_at = 0;
    let mut protocol_ready = false;
    let mut local_lights_supported = false;
    let mut light_request = 0_u8;
    let mut pending_light_request: Option<(u8, u8, String, u64)> = None;
    let mut lights_requested_at = 0;
    let mut tcp_buffer = [0; 4096];
    let mut udp_buffer = [0; 1024];
    let expected_size =
        outsim::packet_size(config.outsim_options, config.expected_outsim_id()).unwrap();
    while !stop.load(Ordering::Relaxed) {
        let now = start.elapsed().as_millis() as u64;
        let needs_gauge = config.needs_outgauge() || recording.lock().unwrap().is_some();
        if needs_gauge
            && gauge.is_none()
            && config.outgauge_bind != config.outsim_bind
            && now >= gauge_retry_at
        {
            match UdpSocket::bind(config.outgauge_bind).and_then(|socket| {
                socket.set_nonblocking(true)?;
                Ok(socket)
            }) {
                Ok(socket) => {
                    gauge = Some(socket);
                    stats.outgauge_error = None;
                }
                Err(error) => {
                    stats.outgauge_error = Some(format!(
                        "cannot bind OutGauge {}: {error}",
                        config.outgauge_bind
                    ));
                    gauge_retry_at = now + 2000;
                }
            }
        }
        if let Some(initial) = initial
            && connection.is_none()
            && connection_attempt.is_none()
            && now >= retry_at
        {
            engine.clear();
            dashboard.clear();
            framer = Framer::default();
            protocol_ready = false;
            local_lights_supported = false;
            pending_light_request = None;
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            let address = config.insim_address;
            // Connect/handshake attempts must not block local UDP capture.
            match thread::Builder::new()
                .name("lfs-insim-connect".into())
                .spawn(move || {
                    let result = TcpStream::connect_timeout(&address, Duration::from_millis(300))
                        .and_then(|mut stream| {
                            stream.set_nodelay(true)?;
                            stream.set_read_timeout(Some(Duration::from_millis(10)))?;
                            stream.set_write_timeout(Some(Duration::from_millis(100)))?;
                            stream.write_all(&initial)?;
                            Ok(stream)
                        })
                        .map_err(|e| {
                            format!(
                                "insim {address}: {e}. In LFS type /insim {}.",
                                address.port()
                            )
                        });
                    let _ = sender.send(result);
                }) {
                Ok(_) => connection_attempt = Some(receiver),
                Err(e) => {
                    stats.error = Some(format!("cannot start InSim connector: {e}"));
                    retry_at = now + 2000;
                }
            }
        }
        if let Some(result) = connection_attempt.as_ref().and_then(|r| r.try_recv().ok()) {
            connection_attempt = None;
            match result {
                Ok(stream) => {
                    connection = Some(stream);
                    stats.error = None;
                    stats.version.clear();
                    connected_at = now;
                    received_at = now;
                }
                Err(error) => {
                    stats.error = Some(error);
                    retry_at = now + 2000;
                }
            }
        }
        let mut disconnected = None;
        if let Some(stream) = connection.as_mut() {
            match stream.read(&mut tcp_buffer) {
                Ok(0) => {
                    disconnected = Some(if protocol_ready {
                        "LFS closed the InSim connection".into()
                    } else {
                        "LFS rejected the InSim handshake. If LFS reports a password mismatch, enter its multiplayer admin password and Apply / reconnect.".into()
                    })
                }
                Ok(size) => {
                    let packet_time = start.elapsed().as_millis() as u64;
                    received_at = packet_time;
                    match framer.push(&tcp_buffer[..size]) {
                        Err(error) => disconnected = Some(error),
                        Ok(packets) => {
                            for bytes in packets {
                                match insim::decode(&bytes) {
                                    Err(error) => {
                                        stats.malformed_packets += 1;
                                        disconnected = Some(error);
                                        break;
                                    }
                                    Ok(Packet::Version { version, protocol }) => {
                                        if !matches!(protocol, 9 | 10) {
                                            disconnected = Some(format!(
                                                "Unsupported InSim version {protocol}; need 9 or 10"
                                            ));
                                            break;
                                        }
                                        protocol_ready = true;
                                        local_lights_supported = protocol >= 10;
                                        stats.version = version;
                                        if let Err(error) = request_roster(stream) {
                                            disconnected = Some(error.to_string());
                                            break;
                                        }
                                    }
                                    Ok(Packet::LocalLights {
                                        request,
                                        headlights,
                                    }) if protocol_ready => {
                                        if let Some((expected, plid, car, sent)) =
                                            &pending_light_request
                                            && request == *expected
                                        {
                                            if let Some(player) = engine.dashboard_player()
                                                && player.plid == *plid
                                                && player.model == *car
                                                && packet_time.saturating_sub(*sent)
                                                    <= config.stale_ms
                                            {
                                                dashboard.receive_local_lights(
                                                    headlights,
                                                    packet_time,
                                                    player,
                                                );
                                            }
                                            pending_light_request = None;
                                        }
                                    }
                                    Ok(Packet::LocalCommand(command)) if protocol_ready => {
                                        use VisibilityCommand::{Set, Toggle};
                                        let command = command.trim();
                                        let request = if command.eq_ignore_ascii_case("show") {
                                            Some(Set(true))
                                        } else if command.eq_ignore_ascii_case("hide") {
                                            Some(Set(false))
                                        } else if command.eq_ignore_ascii_case("hud") {
                                            Some(Toggle)
                                        } else {
                                            None
                                        };
                                        if let Some(request) = request {
                                            let mut pending = overlay_visibility.lock().unwrap();
                                            // Collapse the ordered commands without losing toggle parity.
                                            *pending = match (request, *pending) {
                                                (Toggle, Some(Set(value))) => Some(Set(!value)),
                                                (Toggle, Some(Toggle)) => None,
                                                _ => Some(request),
                                            };
                                        }
                                    }
                                    Ok(Packet::Tiny(0)) => {
                                        if let Err(error) = stream.write_all(&insim::tiny(0, 0)) {
                                            disconnected = Some(error.to_string());
                                            break;
                                        }
                                    }
                                    Ok(packet) if protocol_ready => {
                                        let player_id = engine.dashboard_player().map(|p| p.plid);
                                        let invalidate = match &packet {
                                            Packet::State(_)
                                            | Packet::Camera(_)
                                            | Packet::Session
                                            | Packet::Tiny(10..=12)
                                            | Packet::RaceStart {
                                                requested: false, ..
                                            } => true,
                                            Packet::Player(p) => Some(p.plid) == player_id,
                                            Packet::Pit(id)
                                            | Packet::Leave(id)
                                            | Packet::Reset(id)
                                            | Packet::Takeover(id) => Some(*id) == player_id,
                                            Packet::ConnectionLeft(_) => true,
                                            _ => false,
                                        };
                                        if invalidate {
                                            dashboard.clear();
                                            pending_light_request = None;
                                        }
                                        let refresh = matches!(
                                            packet,
                                            Packet::Takeover(_)
                                                | Packet::Session
                                                | Packet::RaceStart {
                                                    requested: false,
                                                    ..
                                                }
                                                | Packet::Camera(_)
                                                | Packet::Tiny(10..=12)
                                        );
                                        if let Err(error) = engine.packet(packet, packet_time) {
                                            stats.malformed_packets += 1;
                                            stats.error = Some(error);
                                        }
                                        if refresh && let Err(error) = request_roster(stream) {
                                            disconnected = Some(error.to_string());
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => disconnected = Some(error.to_string()),
            }
            // OutGauge reports dashboard symbols, which may omit low beam on stock cars.
            // TINY_LCL reads the local human's actual light switch (InSim 10+).
            if protocol_ready
                && local_lights_supported
                && config.speed_dashboard.enabled
                && disconnected.is_none()
                && now.saturating_sub(lights_requested_at) >= 100
                && pending_light_request
                    .as_ref()
                    .is_none_or(|(_, _, _, sent)| now.saturating_sub(*sent) > config.stale_ms)
                && let Some(player) = engine.dashboard_player().filter(|p| p.local_human())
            {
                light_request = light_request.wrapping_add(1).max(1);
                if let Err(error) = stream.write_all(&insim::tiny(30, light_request)) {
                    disconnected = Some(error.to_string());
                } else {
                    pending_light_request =
                        Some((light_request, player.plid, player.model.clone(), now));
                    lights_requested_at = now;
                }
            }
            if !protocol_ready && now.saturating_sub(connected_at) > 3000 {
                disconnected =
                    Some("No InSim version response — check the local admin password".into());
            } else if now.saturating_sub(received_at) > 40000 {
                disconnected = Some("InSim connection timed out".into());
            }
        }
        if let Some(error) = disconnected {
            connection = None;
            protocol_ready = false;
            pending_light_request = None;
            engine.clear();
            dashboard.clear();
            stats.error = Some(error);
            retry_at = now + 2000;
        }
        // Bound work per tick so a flood cannot starve rendering or shutdown.
        for _ in 0..256 {
            match udp.recv_from(&mut udp_buffer) {
                Ok((size, peer)) if peer.ip() == config.outsim_bind.ip() => {
                    let bytes = &udp_buffer[..size];
                    // A shared socket routes OutGauge separately from radar physics.
                    if matches!(size, 92 | 96) && size != expected_size {
                        if needs_gauge {
                            if config.outgauge_bind == config.outsim_bind {
                                receive_outgauge(
                                    bytes,
                                    &config,
                                    &mut engine,
                                    start.elapsed().as_millis() as u64,
                                    &udp,
                                    &mut dashboard,
                                    &mut stats,
                                    &recording,
                                );
                            } else {
                                let id = if size == 96 {
                                    i32::from_le_bytes(bytes[92..96].try_into().unwrap())
                                } else {
                                    0
                                };
                                stats.outgauge_error = Some(format!(
                                    "outgauge arrives at {} (ID {id}); gadgets expect {} (ID {}). Match the telemetry configuration or use Configure OutGauge with LFS closed.",
                                    config.outsim_bind, config.outgauge_bind, config.outgauge_id
                                ));
                            }
                        }
                        continue;
                    }
                    match outsim::decode(bytes, config.outsim_options, config.expected_outsim_id())
                    {
                        Ok(sample) => {
                            if let Some(sink) = recording.lock().unwrap().as_ref() {
                                sink.packet(
                                    crate::recording::Stream::Outsim,
                                    sample.time_ms,
                                    bytes,
                                );
                            }
                            if protocol_ready {
                                engine.outsim(sample, start.elapsed().as_millis() as u64, &config);
                            }
                        }
                        Err(error) => {
                            if let Some(sink) = recording.lock().unwrap().as_ref() {
                                sink.rejected(crate::recording::Stream::Outsim, error.clone());
                            }
                            stats.malformed_packets += 1;
                            stats.error = Some(error);
                        }
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    stats.error = Some(format!("OutSim receive: {error}"));
                    break;
                }
            }
        }
        if let Some(socket) = &gauge {
            for _ in 0..256 {
                match socket.recv_from(&mut udp_buffer) {
                    Ok((size, peer)) if peer.ip() == config.outgauge_bind.ip() => {
                        receive_outgauge(
                            &udp_buffer[..size],
                            &config,
                            &mut engine,
                            start.elapsed().as_millis() as u64,
                            socket,
                            &mut dashboard,
                            &mut stats,
                            &recording,
                        );
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        stats.outgauge_error = Some(format!("outgauge receive: {error}"));
                        break;
                    }
                }
            }
        }
        if let Some(sink) = recording.lock().unwrap().as_ref() {
            sink.tick();
        }
        let render_now = start.elapsed().as_millis() as u64;
        stats.dashboard = dashboard.frame(render_now, &config, engine.dashboard_player());
        stats.fuel = engine.fuel(render_now, &config);
        if let Some(store) = &store {
            // ponytail: occasional config writes run here; move I/O off-thread if slow storage stalls telemetry.
            let mut store = store.lock().unwrap();
            if let Some(records) = engine.lap_records.take_changes() {
                store.update_records(records);
                retry_records_at = render_now + 2000;
            } else if render_now >= retry_records_at {
                store.flush();
                retry_records_at = render_now + 2000;
            }
            if stats.lap_records_error != store.error {
                if let Some(error) = &store.error {
                    #[cfg(feature = "desktop")]
                    log::error!("{error}");
                    #[cfg(not(feature = "desktop"))]
                    eprintln!("{error}");
                }
                stats.lap_records_error = store.error.clone();
            }
        }
        stats.lap_records = engine.lap_records.frame();
        stats.lap_log = engine.lap_records.lap_log.lock().unwrap().frame();
        stats.connected = protocol_ready;
        stats.frame = engine.frame(render_now, &config);
        stats.gaps = engine.gaps(render_now, &config, &stats.frame);
        stats.delta = engine.delta(render_now, &config, &stats.frame);
        if !protocol_ready {
            stats.frame.status = if config.insim_enabled {
                "Waiting for local LFS InSim connection"
            } else {
                "InSim disabled; local recording available"
            }
            .into();
        }
        stats.mci_sets = engine.mci_sets;
        stats.outsim_samples = engine.outsim_samples;
        stats.rejected_outsim = engine.rejected_outsim;
        *output.lock().unwrap() = stats.clone();
        thread::sleep(Duration::from_millis(5));
    }
    if let Some(mut stream) = connection {
        let _ = stream.write_all(&insim::tiny(2, 0));
    }
    if let Some(store) = &store {
        let mut store = store.lock().unwrap();
        if let Some(records) = engine.lap_records.take_changes() {
            store.update_records(records);
        }
        store.flush();
    }
}
