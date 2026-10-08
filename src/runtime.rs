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
    pub outgauge_error: Option<String>,
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

pub struct Runtime {
    latest: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn start(config: Config) -> Result<Self, String> {
        let password = config.effective_insim_password();
        Self::start_with_password(config, password)
    }

    /// An explicit password overrides the TOML/environment value for this connection.
    pub fn start_with_password(config: Config, password: String) -> Result<Self, String> {
        config.validate()?;
        let initial = insim::init(config.mci_interval_ms, &password)?;
        let udp = UdpSocket::bind(config.outsim_bind).map_err(|e| {
            format!(
                "Cannot bind OutSim {}: {e}. Check other telemetry apps.",
                config.outsim_bind
            )
        })?;
        udp.set_nonblocking(true).map_err(|e| e.to_string())?;
        let latest = Arc::new(Mutex::new(Snapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let output = latest.clone();
        let quit = stop.clone();
        let worker = thread::Builder::new()
            .name("lfs-telemetry".into())
            .spawn(move || {
                run(config, initial, udp, output, quit);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            latest,
            stop,
            worker: Some(worker),
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        self.latest.lock().unwrap().clone()
    }
    pub fn snapshot_reader(&self) -> SnapshotReader {
        SnapshotReader(self.latest.clone())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn request_roster(stream: &mut TcpStream) -> std::io::Result<()> {
    for subtype in [13, 14, 7, 19] {
        stream.write_all(&insim::tiny(subtype, 1))?;
    }
    Ok(())
}

fn receive_outgauge(
    bytes: &[u8],
    config: &Config,
    engine: &mut Engine,
    now: u64,
    protocol_ready: bool,
    dashboard: &mut crate::dashboard::DashboardTelemetry,
    stats: &mut Snapshot,
) {
    match outgauge::decode(bytes, config.outgauge_id) {
        Ok(sample) if protocol_ready => {
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
        Ok(_) => {}
        Err(error) => stats.outgauge_error = Some(error),
    }
}

fn run(
    config: Config,
    initial: [u8; 44],
    udp: UdpSocket,
    output: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
) {
    let start = Instant::now();
    let mut engine = Engine::new(config.follow_viewed_car);
    let mut stats = Snapshot::default();
    let mut dashboard = crate::dashboard::DashboardTelemetry::default();
    let gauge = if config.needs_outgauge() && config.outgauge_bind != config.outsim_bind {
        match UdpSocket::bind(config.outgauge_bind).and_then(|socket| {
            socket.set_nonblocking(true)?;
            Ok(socket)
        }) {
            Ok(socket) => Some(socket),
            Err(error) => {
                stats.outgauge_error = Some(format!(
                    "cannot bind OutGauge {}: {error}",
                    config.outgauge_bind
                ));
                None
            }
        }
    } else {
        None
    };
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
        if connection.is_none() && now >= retry_at {
            engine.clear();
            dashboard.clear();
            framer = Framer::default();
            protocol_ready = false;
            local_lights_supported = false;
            pending_light_request = None;
            let attempt =
                TcpStream::connect_timeout(&config.insim_address, Duration::from_millis(300))
                    .and_then(|mut stream| {
                        stream.set_nodelay(true)?;
                        stream.set_read_timeout(Some(Duration::from_millis(10)))?;
                        stream.set_write_timeout(Some(Duration::from_millis(100)))?;
                        stream.write_all(&initial)?;
                        Ok(stream)
                    });
            match attempt {
                Ok(stream) => {
                    connection = Some(stream);
                    stats.error = None;
                    stats.version.clear();
                    connected_at = now;
                    received_at = now;
                }
                Err(error) => {
                    stats.error = Some(format!(
                        "InSim {}: {error}. In LFS type /insim {}.",
                        config.insim_address,
                        config.insim_address.port()
                    ));
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
                Ok((size, peer)) if peer.ip() == config.insim_address.ip() => {
                    let bytes = &udp_buffer[..size];
                    // A shared socket routes OutGauge separately from radar physics.
                    if matches!(size, 92 | 96) && size != expected_size {
                        if config.needs_outgauge() {
                            if config.outgauge_bind == config.outsim_bind {
                                receive_outgauge(
                                    bytes,
                                    &config,
                                    &mut engine,
                                    start.elapsed().as_millis() as u64,
                                    protocol_ready,
                                    &mut dashboard,
                                    &mut stats,
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
                        Ok(sample) if protocol_ready => {
                            engine.outsim(sample, start.elapsed().as_millis() as u64, &config)
                        }
                        Ok(_) => {}
                        Err(error) => {
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
                    Ok((size, peer)) if peer.ip() == config.insim_address.ip() => {
                        receive_outgauge(
                            &udp_buffer[..size],
                            &config,
                            &mut engine,
                            start.elapsed().as_millis() as u64,
                            protocol_ready,
                            &mut dashboard,
                            &mut stats,
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
        let render_now = start.elapsed().as_millis() as u64;
        stats.dashboard = dashboard.frame(render_now, &config, engine.dashboard_player());
        stats.fuel = engine.fuel(render_now, &config);
        stats.connected = protocol_ready;
        stats.frame = engine.frame(render_now, &config);
        stats.gaps = engine.gaps(render_now, &config, &stats.frame);
        stats.delta = engine.delta(render_now, &config, &stats.frame);
        if !protocol_ready {
            stats.frame.status = "Waiting for local LFS InSim connection".into();
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
}
