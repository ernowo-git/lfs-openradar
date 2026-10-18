mod support;
use lfs_openradar::{
    config::{Config, ConfigStore},
    lap_records::{PersonalBest, Record, Recorder, Records},
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
    send(&mut engine, track("BL1", count));
    engine
}

fn complete(engine: &mut Engine, n: u16, start: u32, checkpoints: &[u32], total: u32) {
    for (i, &time) in checkpoints.iter().enumerate() {
        send(engine, split(i as u8 + 1, time, start.wrapping_add(time)));
    }
    send(engine, lap(n, total, start.wrapping_add(total)));
}

#[test]
fn pb_is_one_lap_while_sector_minima_can_come_from_slower_flagged_laps() {
    let mut engine = setup(2);
    // Connecting mid-lap must not record an apparently complete split sequence.
    complete(&mut engine, 1, 0, &[30_000, 65_000], 100_000);
    assert!(engine.lap_records.records().is_empty());
    complete(&mut engine, 2, 100_000, &[30_000, 65_000], 100_000);
    assert_eq!(
        engine.lap_records.records()["BL1"]["XRG"].pb.lap_ms,
        100_000
    );
    send(&mut engine, Packet::InvalidLap(1));
    send(
        &mut engine,
        Packet::PitLane {
            plid: 1,
            entered: true,
        },
    );
    complete(&mut engine, 3, 200_000, &[29_000, 66_000], 101_000);
    let record = &engine.lap_records.records()["BL1"]["XRG"];
    assert_eq!(record.pb.checkpoints_ms, [30_000, 65_000]);
    assert_eq!(record.pb.lap_ms, 100_000);
    assert_eq!(record.best_sectors_ms, [29_000, 35_000, 35_000]);
    // A tie keeps the original PB, even when one of its checkpoints improves.
    complete(&mut engine, 4, 301_000, &[28_000, 64_000], 100_000);
    assert_eq!(
        engine.lap_records.records()["BL1"]["XRG"].pb.checkpoints_ms,
        [30_000, 65_000]
    );
    assert_eq!(
        engine.lap_records.records()["BL1"]["XRG"].best_sectors_ms,
        [28_000, 35_000, 35_000]
    );
}

#[test]
fn missing_conflicting_and_stale_events_do_not_create_partial_records() {
    let mut engine = setup(2);
    send(&mut engine, lap(1, 100_000, 100_000));
    send(&mut engine, split(1, 30_000, 130_000));
    assert!(engine.lap_records.take_changes().is_none());
    // Identical duplicates are harmless, including a duplicate boundary after S1.
    send(&mut engine, split(1, 30_000, 130_000));
    send(&mut engine, lap(1, 100_000, 100_000));
    send(&mut engine, split(2, 65_000, 165_000));
    send(&mut engine, lap(2, 100_000, 200_000));
    let original = engine.lap_records.records().clone();
    engine.lap_records.take_changes().unwrap();
    send(&mut engine, split(2, 20_000, 220_000)); // Missing S1.
    send(&mut engine, lap(3, 70_000, 270_000));
    complete(&mut engine, 4, 270_001, &[10_000, 20_000], 30_000); // Wrong elapsed time.
    send(&mut engine, split(1, 10_000, 310_001));
    send(&mut engine, split(1, 11_000, 311_001)); // Conflicting S1.
    send(&mut engine, split(2, 20_000, 320_001));
    send(&mut engine, lap(5, 30_000, 330_001));
    assert_eq!(engine.lap_records.records(), &original);
    assert!(engine.lap_records.take_changes().is_none());
    complete(&mut engine, 6, 330_001, &[10_000, 20_000], 30_000);
    assert_eq!(engine.lap_records.records()["BL1"]["XRG"].pb.lap_ms, 30_000);
}

#[test]
fn recording_is_personal_and_independent_of_camera_but_resets_with_identity_and_connection() {
    let mut engine = setup(0);
    send(&mut engine, lap(1, 10_000, 10_000));
    // Camera changes alter the radar's selector, but not record ownership.
    send(&mut engine, Packet::Camera(1));
    send(&mut engine, lap(2, 10_000, 20_000));
    assert_eq!(engine.lap_records.records()["BL1"]["XRG"].pb.lap_ms, 10_000);
    let mut other = lap(3, 1, 20_001);
    if let Packet::Lap { plid, .. } = &mut other {
        *plid = 2;
    }
    send(&mut engine, other);
    send(&mut engine, Packet::Reset(1));
    send(&mut engine, lap(3, 1, 20_001));
    assert_eq!(engine.lap_records.records()["BL1"]["XRG"].pb.lap_ms, 10_000);
    send(&mut engine, state("BL1", 3)); // Replay cannot update personal records.
    complete(&mut engine, 4, 20_001, &[], 1);
    send(&mut engine, state("BL1", 1));
    engine.clear();
    assert_eq!(engine.lap_records.records()["BL1"]["XRG"].pb.lap_ms, 10_000);
    send(&mut engine, support::player(1, 0));
    send(&mut engine, state("BL1R", 1));
    send(&mut engine, track("BL1R", 0));
    complete(&mut engine, 1, 0, &[], 11_000);
    complete(&mut engine, 2, 11_000, &[], 11_000);
    assert_eq!(
        engine.lap_records.records()["BL1R"]["XRG"].pb.lap_ms,
        11_000
    );
    let mut changed = support::player(1, 0);
    if let Packet::Player(player) = &mut changed {
        player.model = "ABC123".into();
    }
    send(&mut engine, changed);
    complete(&mut engine, 3, 22_000, &[], 12_000);
    complete(&mut engine, 4, 34_000, &[], 12_000);
    assert_eq!(
        engine.lap_records.records()["BL1R"]["ABC123"].pb.lap_ms,
        12_000
    );
    send(&mut engine, support::player(1, 2)); // Local AI.
    complete(&mut engine, 5, 46_000, &[], 1);
    complete(&mut engine, 6, 46_001, &[], 1);
    assert!(engine.lap_records.frame().car.is_none());
    assert_eq!(
        engine.lap_records.records()["BL1R"]["XRG"].pb.lap_ms,
        11_000
    );
}

#[test]
fn variable_sector_counts_wraparound_and_custom_timing() {
    for count in 0..=3 {
        let mut engine = setup(count);
        let checkpoints: Vec<u32> = (1..=count).map(|i| u32::from(i) * 10_000).collect();
        let total = (u32::from(count) + 1) * 10_000;
        let start = u32::MAX - 999;
        send(&mut engine, lap(1, 10_000, start));
        complete(&mut engine, 2, start, &checkpoints, total);
        let record = &engine.lap_records.records()["BL1"]["XRG"];
        assert_eq!(record.best_sectors_ms, vec![10_000; usize::from(count) + 1]);
        record.validate().unwrap();
        let original = engine.lap_records.records().clone();
        let mut custom = track("BL1", count);
        if let Packet::RaceStart { info, .. } = &mut custom {
            info.timing = 0x80 | count;
        }
        send(&mut engine, custom);
        complete(
            &mut engine,
            3,
            start.wrapping_add(total),
            &checkpoints,
            total,
        );
        assert_eq!(engine.lap_records.records(), &original);
        send(&mut engine, Packet::LayoutChanged);
        assert!(
            engine
                .lap_records
                .frame()
                .status
                .contains("standard track timing")
        );
    }
}

fn records() -> Records {
    let mut engine = setup(2);
    send(&mut engine, lap(1, 100_000, 100_000));
    complete(&mut engine, 2, 100_000, &[30_000, 65_000], 100_000);
    engine.lap_records.take_changes().unwrap()
}

struct TemporaryConfig(std::path::PathBuf);
impl TemporaryConfig {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Concurrent Windows tests can receive the same wall-clock timestamp.
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "openradar-records-{}-{unique}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory.join("config.toml"))
    }
}
impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}

#[test]
fn config_writer_preserves_unsaved_settings_and_latest_records_across_save_and_restart() {
    let temp = TemporaryConfig::new();
    let mut saved = Config {
        overlay_x: 20.0,
        ..Config::default()
    };
    saved.insim_password.0 = "saved-password".into();
    saved.save(&temp.0).unwrap();
    let store = ConfigStore::open(&temp.0).unwrap();
    assert!(
        ConfigStore::open(&temp.0).is_err(),
        "second writer must not open the same config"
    );
    // Runtime/panel edits must not enter the automatic save's baseline.
    let mut edited = saved.clone();
    edited.overlay_x = 500.0;
    edited.insim_password.0 = "runtime-secret".into();
    store.lock().unwrap().update_records(records());
    let loaded = Config::load(&temp.0).unwrap();
    assert_eq!(loaded.overlay_x, 20.0);
    assert_eq!(loaded.insim_password.0, "saved-password");
    assert_eq!(loaded.track_records, records());
    store.lock().unwrap().save_settings(&edited).unwrap();
    let loaded = Config::load(&temp.0).unwrap();
    assert_eq!(loaded.overlay_x, 500.0);
    assert_eq!(
        loaded.track_records,
        records(),
        "stale panel clone cannot erase records"
    );
    drop(store);
    let store = ConfigStore::open(&temp.0).unwrap();
    let recorder = Recorder::with_records(store.lock().unwrap().records.clone());
    assert_eq!(recorder.records(), &records());
}

#[test]
fn failed_save_keeps_prior_file_and_retries_without_another_pb() {
    let temp = TemporaryConfig::new();
    Config::default().save(&temp.0).unwrap();
    let original = std::fs::read(&temp.0).unwrap();
    let store = ConfigStore::open(&temp.0).unwrap();
    let original_path = temp.0.with_extension("original");
    std::fs::rename(&temp.0, &original_path).unwrap();
    std::fs::create_dir(&temp.0).unwrap(); // Prevent replacement, even under privileged test execution.
    store.lock().unwrap().update_records(records());
    assert!(
        store
            .lock()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .starts_with("cannot save lap records")
    );
    assert_eq!(std::fs::read(&original_path).unwrap(), original);
    assert_eq!(store.lock().unwrap().records, records());
    std::fs::remove_dir(&temp.0).unwrap();
    std::fs::rename(&original_path, &temp.0).unwrap();
    store.lock().unwrap().flush();
    assert!(store.lock().unwrap().error.is_none());
    assert_eq!(Config::load(&temp.0).unwrap().track_records, records());
}

#[test]
fn legacy_configuration_loads_and_malformed_records_are_rejected() {
    let temp = TemporaryConfig::new();
    std::fs::write(&temp.0, "overlay_x = 15.0\n").unwrap();
    assert!(Config::load(&temp.0).unwrap().track_records.is_empty());
    let valid = Record {
        checkpoint_count: 2,
        best_sectors_ms: vec![29_000, 35_000, 35_000],
        pb: PersonalBest {
            checkpoints_ms: vec![30_000, 65_000],
            lap_ms: 100_000,
        },
    };
    for malformed in [
        Record {
            checkpoint_count: 3,
            ..valid.clone()
        },
        Record {
            best_sectors_ms: vec![0, 1, 1],
            ..valid.clone()
        },
        Record {
            best_sectors_ms: vec![31_000, 35_000, 35_000],
            ..valid.clone()
        },
        Record {
            pb: PersonalBest {
                checkpoints_ms: vec![30_000, 20_000],
                lap_ms: 100_000,
            },
            ..valid
        },
    ] {
        let mut config = Config::default();
        config
            .track_records
            .entry("BL1".into())
            .or_default()
            .insert("XFG".into(), malformed);
        assert!(config.validate().is_err());
        assert!(config.save(&temp.0).is_err());
    }
    assert_eq!(
        std::fs::read_to_string(&temp.0).unwrap(),
        "overlay_x = 15.0\n"
    );
}

#[test]
fn tcp_lap_events_save_without_outsim_and_runtime_replacement_keeps_records() {
    use lfs_openradar::runtime::Runtime;
    use std::{
        io::{Read, Write},
        net::{TcpListener, UdpSocket},
        time::{Duration, Instant},
    };
    let temp = TemporaryConfig::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = Config {
        insim_address: listener.local_addr().unwrap(),
        outsim_bind: udp.local_addr().unwrap(),
        ..Config::default()
    };
    drop(udp);
    config.save(&temp.0).unwrap();
    let store = ConfigStore::open(&temp.0).unwrap();
    for attempt in 0..2 {
        let runtime =
            Runtime::start_with_store(config.clone(), String::new(), Some(store.clone())).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
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
        npl[3] = 1;
        npl[4] = 1;
        npl[73] = 1;
        npl[40..43].copy_from_slice(b"XRG");
        let mut rst = support::packet(17, 28);
        rst[4] = 10;
        rst[7] = 0x42;
        rst[8..11].copy_from_slice(b"BL1");
        let mut bytes = [version, sta, npl, rst].concat();
        if attempt == 0 {
            for (kind, time, elapsed, n) in [
                (24, 100_000_u32, 100_000_u32, 1),
                (25, 30_000, 130_000, 1),
                (25, 65_000, 165_000, 2),
                (24, 100_000, 200_000, 2),
            ] {
                let mut packet = support::packet(kind, if kind == 24 { 20 } else { 16 });
                packet[3] = 1;
                packet[4..8].copy_from_slice(&time.to_le_bytes());
                packet[8..12].copy_from_slice(&elapsed.to_le_bytes());
                if kind == 24 {
                    packet[12..14].copy_from_slice(&(n as u16).to_le_bytes());
                    packet[17] = 2;
                } else {
                    packet[12] = n;
                    packet[13] = 2;
                }
                bytes.extend(packet);
            }
        }
        stream.write_all(&bytes).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let snapshot = runtime.snapshot();
            if let Some(record) = snapshot.lap_records.record {
                assert_eq!(record.pb.lap_ms, 100_000);
                assert!(snapshot.lap_records_error.is_none());
                assert_eq!(snapshot.outsim_samples, 0);
                assert_eq!(Config::load(&temp.0).unwrap().track_records, records());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "no lap record from TCP stream: {:?}",
                snapshot.error
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(runtime);
    }
}

#[test]
fn headless_demo_does_not_save_synthetic_records() {
    let temp = TemporaryConfig::new();
    let original = "[lap_records]\nenabled = true\n";
    std::fs::write(&temp.0, original).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lfs-openradar"))
        .args(["--demo", "--headless", "--seconds", "1", "--config"])
        .arg(&temp.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Lap records:"));
    assert_eq!(std::fs::read_to_string(&temp.0).unwrap(), original);
    assert_eq!(
        std::fs::read_dir(temp.0.parent().unwrap()).unwrap().count(),
        1
    );
}
