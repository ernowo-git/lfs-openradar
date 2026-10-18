mod support;
use lfs_openradar::{
    config::Config,
    lap_records::Recorder,
    lfs::insim::{Packet, State, TrackInfo},
    radar::Engine,
};

fn state(track: &str, flags: u16) -> Packet {
    Packet::State(State {
        session: Default::default(),
        track: track.into(),
        flags,
        camera: 3,
        viewed: 1,
    })
}

fn track(code: &str, count: u8) -> Packet {
    Packet::RaceStart {
        info: TrackInfo {
            track: code.into(),
            nodes: 1000,
            finish: 0,
            split1: 300,
            timing: 0x40 | count,
            race_laps: 10,
        },
        requested: false,
    }
}

fn split(n: u8, time: u32, elapsed: u32) -> Packet {
    Packet::Split {
        plid: 1,
        time_ms: time,
        elapsed_ms: elapsed,
        split: n,
        penalty: 2,
    }
}

fn lap(n: u16, time: u32, elapsed: u32) -> Packet {
    Packet::Lap {
        plid: 1,
        time_ms: time,
        elapsed_ms: elapsed,
        laps_done: n,
        penalty: 2,
    }
}

fn send(engine: &mut Engine, packet: Packet) {
    engine.packet(packet, 0).unwrap();
}

fn setup(count: u8) -> Engine {
    let mut engine = Engine::default();
    send(&mut engine, support::player(1, 0));
    send(&mut engine, support::player(2, 2));
    send(&mut engine, state("BL1", 1));
    let mut metadata = track("BL1", count);
    if let Packet::RaceStart { requested, .. } = &mut metadata {
        *requested = true;
    }
    send(&mut engine, metadata);
    engine
}

fn complete(engine: &mut Engine, n: u16, start: u32, checkpoints: &[u32], total: u32) {
    for (i, &time) in checkpoints.iter().enumerate() {
        send(engine, split(i as u8 + 1, time, start.wrapping_add(time)));
    }
    send(engine, lap(n, total, start.wrapping_add(total)));
}

#[test]
fn log_keeps_every_lap_and_session_best_but_snapshots_only_latest_eight() {
    let mut engine = setup(0);
    send(&mut engine, lap(1, 100_000, 100_000));
    let mut start = 100_000;
    for number in 2..=13 {
        let total = if number == 2 {
            90_000
        } else {
            100_000 + u32::from(number)
        };
        complete(&mut engine, number, start, &[], total);
        start += total;
    }
    let log = engine.lap_records.lap_log.lock().unwrap();
    assert_eq!(log.laps().len(), 12);
    assert_eq!(log.laps()[0].delta_ms, None);
    assert_eq!(log.laps()[1].delta_ms, Some(10_003));
    let frame = log.frame();
    assert_eq!(frame.best_ms, Some(90_000)); // Best survives scrolling out of view.
    assert_eq!(frame.laps.len(), 8);
    assert_eq!(frame.laps[0].number, 13);
    assert_eq!(frame.laps[7].number, 6);
    assert_eq!(frame.laps[0].delta_ms, Some(1));
    assert_eq!(engine.lap_records.records()["BL1"]["XRG"].pb.lap_ms, 90_000);
}

#[test]
fn duplicate_incomplete_opponent_and_replay_laps_never_add_rows() {
    let mut engine = setup(2);
    complete(&mut engine, 1, 0, &[30_000, 65_000], 100_000);
    assert!(engine.lap_records.lap_log.lock().unwrap().laps().is_empty());
    complete(&mut engine, 2, 100_000, &[30_000, 65_000], 100_000);
    send(&mut engine, lap(2, 100_000, 200_000));
    // A missing checkpoint cannot produce a complete recorded lap.
    complete(&mut engine, 3, 200_000, &[30_000], 100_000);
    complete(&mut engine, 4, 300_000, &[29_000, 64_000], 99_000);
    let mut other = lap(5, 1, 399_001);
    if let Packet::Lap { plid, .. } = &mut other {
        *plid = 2;
    }
    send(&mut engine, other);
    send(&mut engine, state("BL1", 3));
    complete(&mut engine, 5, 399_000, &[10_000, 20_000], 30_000);
    let log = engine.lap_records.lap_log.lock().unwrap();
    assert_eq!(log.laps().len(), 2);
    assert_eq!(log.laps()[1].delta_ms, None);
    assert_eq!(log.frame().best_ms, Some(99_000));
}

#[test]
fn pits_disconnects_and_runtime_replacement_keep_rows_without_crossing_timing_gaps() {
    let mut engine = setup(0);
    send(&mut engine, lap(1, 100_000, 100_000));
    complete(&mut engine, 2, 100_000, &[], 100_000);
    send(&mut engine, Packet::Pit(1));
    send(&mut engine, support::player(1, 0));
    complete(&mut engine, 3, 200_000, &[], 110_000);
    complete(&mut engine, 4, 310_000, &[], 99_000);
    let shared = engine.lap_records.lap_log.clone();
    assert_eq!(shared.lock().unwrap().laps().len(), 2);
    assert_eq!(shared.lock().unwrap().laps()[1].delta_ms, None);
    engine.clear();
    // Runtime replacement uses the same app-owned memory, with fresh timing context.
    engine.lap_records = Recorder::default();
    engine.lap_records.lap_log = shared.clone();
    send(&mut engine, support::player(1, 0));
    send(&mut engine, state("BL1", 1));
    let mut metadata = track("BL1", 0);
    if let Packet::RaceStart { requested, .. } = &mut metadata {
        *requested = true;
    }
    send(&mut engine, metadata);
    complete(&mut engine, 5, 409_000, &[], 101_000);
    complete(&mut engine, 6, 510_000, &[], 98_000);
    complete(&mut engine, 7, 608_000, &[], 97_000);
    let log = shared.lock().unwrap();
    assert_eq!(log.laps().len(), 4);
    assert_eq!(log.laps()[2].delta_ms, None);
    assert_eq!(log.laps()[3].delta_ms, Some(-1000));
    assert_eq!(log.frame().best_ms, Some(97_000));
    assert!(
        Recorder::default()
            .lap_log
            .lock()
            .unwrap()
            .laps()
            .is_empty()
    );
}

#[test]
fn race_restart_track_car_and_wire_session_end_events_clear_history() {
    for event in [
        track("BL1", 0),
        state("BL1R", 1),
        Packet::Session,
        Packet::Tiny(9),
        Packet::Tiny(11),
        Packet::Tiny(12),
        Packet::LayoutChanged,
    ] {
        let mut engine = setup(0);
        send(&mut engine, lap(1, 100_000, 100_000));
        complete(&mut engine, 2, 100_000, &[], 100_000);
        send(&mut engine, event);
        assert!(engine.lap_records.lap_log.lock().unwrap().laps().is_empty());
        assert!(
            engine
                .lap_records
                .lap_log
                .lock()
                .unwrap()
                .frame()
                .best_ms
                .is_none()
        );
    }
    let mut engine = setup(0);
    send(&mut engine, lap(1, 100_000, 100_000));
    complete(&mut engine, 2, 100_000, &[], 100_000);
    let mut changed = support::player(1, 0);
    if let Packet::Player(player) = &mut changed {
        player.model = "XFG".into();
    }
    send(&mut engine, changed);
    assert!(engine.lap_records.lap_log.lock().unwrap().laps().is_empty());
}

#[test]
fn requested_metadata_preserves_history_and_clock_restart_after_disconnect_clears_it() {
    let mut engine = setup(0);
    send(&mut engine, lap(1, 100_000, 100_000));
    complete(&mut engine, 2, 100_000, &[], 100_000);
    let mut metadata = track("BL1", 0);
    if let Packet::RaceStart { requested, .. } = &mut metadata {
        *requested = true;
    }
    send(&mut engine, metadata.clone());
    complete(&mut engine, 3, 200_000, &[], 99_000);
    assert_eq!(engine.lap_records.lap_log.lock().unwrap().laps().len(), 2);
    engine.clear();
    send(&mut engine, support::player(1, 0));
    send(&mut engine, state("BL1", 1));
    send(&mut engine, metadata);
    complete(&mut engine, 1, 0, &[], 101_000);
    assert!(engine.lap_records.lap_log.lock().unwrap().laps().is_empty());
    complete(&mut engine, 2, 101_000, &[], 102_000);
    assert_eq!(
        engine.lap_records.lap_log.lock().unwrap().frame().best_ms,
        Some(102_000)
    );
}

#[test]
fn log_preferences_round_trip_without_serializing_session_history() {
    let mut config: Config = toml::from_str("").unwrap();
    assert!(!config.lap_log.enabled);
    config.lap_log.enabled = true;
    config.lap_log.window_x = Some(-200.0);
    config.wayland.lap_log.x = Some(400.0);
    config.wayland.lap_log.scale = 1.5;
    let text = toml::to_string(&config).unwrap();
    let restored: Config = toml::from_str(&text).unwrap();
    restored.validate().unwrap();
    assert!(restored.lap_log.enabled);
    assert_eq!(restored.lap_log.window_x, Some(-200.0));
    assert_eq!(restored.wayland.lap_log.x, Some(400.0));
    assert_eq!(restored.wayland.lap_log.scale, 1.5);
    assert!(!text.contains("delta_ms"));
    let mut bad = restored;
    bad.wayland.lap_log.scale = 0.0;
    assert!(bad.validate().is_err());
}

#[test]
fn witnessed_race_start_records_the_first_full_lap_and_ties_stay_neutral() {
    let mut engine = setup(2);
    send(&mut engine, track("BL1", 2));
    complete(&mut engine, 1, 0, &[30_000, 65_000], 100_000);
    complete(&mut engine, 2, 100_000, &[30_000, 65_000], 100_000);
    let log = engine.lap_records.lap_log.lock().unwrap();
    assert_eq!(log.laps().len(), 2);
    assert_eq!(log.laps()[0].number, 1);
    assert_eq!(log.laps()[0].delta_ms, None);
    assert_eq!(log.laps()[1].delta_ms, Some(0));
}

#[test]
fn tcp_snapshots_publish_the_same_session_log_after_runtime_replacement() {
    use lfs_openradar::runtime::Runtime;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let udp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = Config {
        insim_address: server.local_addr().unwrap(),
        outsim_bind: udp.local_addr().unwrap(),
        ..Default::default()
    };
    drop(udp);
    let shared = Default::default();
    for attempt in 0..2 {
        let runtime = Runtime::start_with_session(
            config.clone(),
            String::new(),
            None,
            std::sync::Arc::clone(&shared),
        )
        .unwrap();
        let (mut stream, _) = server.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut handshake = [0; 44];
        stream.read_exact(&mut handshake).unwrap();
        let mut version = support::packet(2, 20);
        version[18] = 9;
        let mut sta = support::packet(5, 28);
        sta[8..10].copy_from_slice(&1_u16.to_le_bytes());
        sta[10] = 3;
        sta[11] = 1;
        sta[20..23].copy_from_slice(b"BL1");
        let mut npl = support::packet(21, 76);
        npl[2] = 1; // Requested roster refresh, rather than re-entry.
        npl[3] = 1;
        npl[4] = 1;
        npl[73] = 1;
        npl[40..43].copy_from_slice(b"XRG");
        let mut rst = support::packet(17, 28);
        rst[2] = 1; // Metadata reply must preserve an existing session log.
        rst[4] = 10;
        rst[7] = 0x40;
        rst[8..11].copy_from_slice(b"BL1");
        let mut bytes = [version, sta, npl, rst].concat();
        let laps = if attempt == 0 {
            [(1_u16, 100_000_u32, 100_000_u32), (2, 100_000, 200_000)]
        } else {
            [(3, 101_000, 301_000), (4, 99_000, 400_000)]
        };
        for (number, time, elapsed) in laps {
            let mut packet = support::packet(24, 20);
            packet[3] = 1;
            packet[4..8].copy_from_slice(&time.to_le_bytes());
            packet[8..12].copy_from_slice(&elapsed.to_le_bytes());
            packet[12..14].copy_from_slice(&number.to_le_bytes());
            bytes.extend(packet);
        }
        stream.write_all(&bytes).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let frame = runtime.snapshot().lap_log;
            if frame.laps.len() == attempt + 1 {
                assert_eq!(frame.laps[0].number, if attempt == 0 { 2 } else { 4 });
                assert_eq!(frame.laps[0].delta_ms, None);
                assert_eq!(
                    frame.best_ms,
                    Some(if attempt == 0 { 100_000 } else { 99_000 })
                );
                break;
            }
            assert!(Instant::now() < deadline, "missing log rows: {frame:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(runtime);
    }
    assert_eq!(shared.lock().unwrap().laps().len(), 2);
}

#[test]
fn returning_from_pits_can_restart_driver_counters_without_erasing_session_history() {
    let mut engine = setup(0);
    send(&mut engine, lap(1, 100_000, 100_000));
    complete(&mut engine, 2, 100_000, &[], 100_000);
    send(&mut engine, Packet::Pit(1));
    send(&mut engine, support::player(1, 0));
    complete(&mut engine, 1, 0, &[], 101_000);
    complete(&mut engine, 2, 101_000, &[], 99_000);
    let log = engine.lap_records.lap_log.lock().unwrap();
    assert_eq!(log.laps().len(), 2);
    assert_eq!(log.laps()[1].delta_ms, None);
    assert_eq!(log.frame().best_ms, Some(99_000));
}
