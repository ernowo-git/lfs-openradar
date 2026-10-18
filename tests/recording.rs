mod support;
use lfs_openradar::{
    config::Config,
    lfs::{outgauge, outsim},
    recording::{self, Event, Stream},
    runtime::Runtime,
};
use std::{
    fs,
    net::UdpSocket,
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "openradar-recording-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn events(path: &std::path::Path) -> Vec<Event> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn gauge(time: u32) -> Vec<u8> {
    let mut bytes = vec![0; 96];
    bytes[..4].copy_from_slice(&time.to_le_bytes());
    bytes[4..8].copy_from_slice(b"XFG\0");
    bytes[10] = 3;
    bytes[28..32].copy_from_slice(&0.5_f32.to_le_bytes());
    bytes[48..52].copy_from_slice(&0.7_f32.to_le_bytes());
    bytes[92..96].copy_from_slice(&24602_i32.to_le_bytes());
    bytes
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !condition() {
        assert!(Instant::now() < deadline, "condition timed out");
        thread::sleep(Duration::from_millis(10));
    }
}
fn config(shared: bool, insim: bool) -> Config {
    let reserve = UdpSocket::bind("127.0.0.1:0").unwrap();
    let second = UdpSocket::bind("127.0.0.1:0").unwrap();
    Config {
        insim_enabled: insim,
        outsim_bind: reserve.local_addr().unwrap(),
        outgauge_bind: if shared {
            reserve.local_addr().unwrap()
        } else {
            second.local_addr().unwrap()
        },
        ..Default::default()
    }
}

#[test]
fn local_capture_survives_disabled_or_missing_insim_without_dashboard_gadgets() {
    for shared in [false, true] {
        for insim in [false, true] {
            let dir = Directory::new();
            let mut config = config(shared, insim);
            let no_handshake = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            config.insim_address = no_handshake.local_addr().unwrap();
            no_handshake.set_nonblocking(true).unwrap();
            assert!(!config.speed_dashboard.enabled && !config.fuel.enabled);
            let mut runtime = Runtime::start(config.clone()).unwrap();
            runtime
                .start_recording(recording::Options {
                    directory: dir.0.clone(),
                    ..Default::default()
                })
                .unwrap();
            assert!(runtime.start_recording(Default::default()).is_err());
            let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
            wait(|| {
                sender
                    .send_to(
                        &support::full_outsim(100, -10.0, 20.0, 0.0),
                        config.outsim_bind,
                    )
                    .unwrap();
                sender.send_to(&gauge(105), config.outgauge_bind).unwrap();
                runtime
                    .recording_status()
                    .is_some_and(|s| s.packets[0] > 0 && s.packets[1] > 0)
            });
            assert!(!runtime.snapshot().connected);
            if !insim {
                assert_eq!(
                    no_handshake.accept().unwrap_err().kind(),
                    std::io::ErrorKind::WouldBlock
                );
            }
            // Corrupt data must be visible but not become a valid packet event.
            let mut invalid = support::full_outsim(110, 0.0, 0.0, 0.0);
            invalid[72..76].copy_from_slice(&f32::NAN.to_le_bytes());
            sender.send_to(&invalid, config.outsim_bind).unwrap();
            wait(|| runtime.recording_status().unwrap().rejected > 0);
            wait(|| {
                runtime.recording_status().unwrap().last_received_ms[1].is_some_and(|t| {
                    runtime.recording_status().unwrap().elapsed_ms > t + config.stale_ms + 20
                })
            });
            let status = runtime.stop_recording().unwrap();
            assert!(!status.active && status.error.is_none());
            assert!(
                status
                    .path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("XFG_BL1_")
            );
            assert!(status.gaps >= 2);
            let saved = events(&status.path);
            assert!(
                matches!(&saved[0], Event::Header { context_verified: false, declared_track, version: 1, .. } if declared_track == "BL1")
            );
            assert!(saved.iter().any(|e| matches!(e, Event::Packet { stream: Stream::Outsim, data_hex, .. } if data_hex.starts_with("4c465354") && data_hex.len() == 560)));
            assert!(saved.iter().any(|e| matches!(e, Event::Packet { stream: Stream::Outgauge, source_time_ms: 105, data_hex, .. } if data_hex.len() == 192)));
            assert!(
                matches!(saved.last(), Some(Event::Footer { complete: true, packets, .. }) if *packets == status.packets.iter().sum::<u64>())
            );
            assert_eq!(status.bytes, fs::metadata(&status.path).unwrap().len());
            assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        }
    }
}

#[test]
fn all_supported_outsim_blocks_decode_without_assuming_optional_offsets() {
    let mut full = support::full_outsim(4321, -1.0, 2.0, 0.5);
    for (at, value) in [
        (72, 0.75_f32),
        (76, 0.2),
        (80, -0.1),
        (96, 500.0),
        (104, 1234.0),
        (108, 1200.0),
        (128, 4500.0),
        (144, 0.15),
        (272, -8.0),
    ] {
        full[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    full[92] = 4;
    full[140] = 83;
    full[141] = 255;
    full[142] = 1;
    let t = outsim::decode_telemetry(&full, 0x1ff, Some(24601)).unwrap();
    assert_eq!(t.sample.time_ms, 4321);
    assert_eq!(t.sample.pose.x, -1.0);
    assert_eq!(t.inputs.unwrap().throttle, 0.75);
    assert_eq!(t.drive.unwrap().gear, 4);
    assert_eq!(t.distance.unwrap().indexed_m, 1200.0);
    let wheels = t.wheels.unwrap();
    assert_eq!(
        (
            wheels[0].air_temp_c,
            wheels[0].slip_fraction,
            wheels[0].touching
        ),
        (83, 255, 1)
    );
    assert_eq!(wheels[0].vertical_load_n, 4500.0);
    assert_eq!(wheels[0].slip_ratio, 0.15);
    assert_eq!(t.steering_torque_nm, Some(-8.0));
    // Exercise every legal optional block combination, checking its actual stride.
    for blocks in 0..32_u16 {
        let options = 15 | (blocks << 4);
        let mut packet = full[..72].to_vec();
        for (flag, range) in [
            (16, 72..92),
            (32, 92..104),
            (64, 104..112),
            (128, 112..272),
            (256, 272..280),
        ] {
            if options & flag != 0 {
                packet.extend_from_slice(&full[range]);
            }
        }
        let t = outsim::decode_telemetry(&packet, options, Some(24601)).unwrap();
        assert_eq!(t.wheels.is_some(), options & 128 != 0);
        assert_eq!(
            t.distance.map(|d| d.indexed_m),
            (options & 64 != 0).then_some(1200.0)
        );
        assert_eq!(t.steering_torque_nm, (options & 256 != 0).then_some(-8.0));
        assert!(outsim::decode(&packet[..packet.len() - 1], options, Some(24601)).is_err());
    }
    full[144..148].copy_from_slice(&f32::INFINITY.to_le_bytes());
    assert!(outsim::decode(&full, 0x1ff, Some(24601)).is_err());
    let t = outgauge::decode_telemetry(&gauge(5), 24602).unwrap();
    assert_eq!(t.sample.time_ms, 5);
    assert_eq!(t.throttle, 0.7);
}

#[test]
fn storage_cap_finalizes_incomplete_and_recovery_preserves_valid_prefix() {
    let dir = Directory::new();
    let config = config(false, false);
    let mut runtime = Runtime::start(config.clone()).unwrap();
    runtime
        .start_recording(recording::Options {
            directory: dir.0.clone(),
            max_bytes: 4096,
            ..Default::default()
        })
        .unwrap();
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    wait(|| {
        sender
            .send_to(
                &support::full_outsim(100, 0.0, 0.0, 0.0),
                config.outsim_bind,
            )
            .unwrap();
        runtime.recording_status().unwrap().error.is_some()
    });
    let status = runtime.stop_recording().unwrap();
    assert!(status.error.unwrap().contains("storage limit"));
    assert!(status.packets[0] > 0);
    assert!(fs::metadata(&status.path).unwrap().len() <= 4096);
    assert!(matches!(
        events(&status.path).last(),
        Some(Event::Footer {
            complete: false,
            ..
        })
    ));
    let original = fs::read_to_string(&status.path).unwrap();
    let prefix = original
        .lines()
        .take_while(|line| !line.contains("\"footer\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let interrupted = dir.0.join("interrupted.partial");
    let damaged = prefix + "{\"type\":\"packet\",\"data_hex\":\"truncated";
    fs::write(&interrupted, &damaged).unwrap();
    let recovered = dir.0.join("recovered.jsonl");
    recording::recover(&interrupted, &recovered).unwrap();
    assert_eq!(fs::read_to_string(&interrupted).unwrap(), damaged);
    assert!(
        matches!(events(&recovered).last(), Some(Event::Footer { complete: false, error: Some(e), .. }) if e.contains("recovered"))
    );
    assert!(recording::recover(&interrupted, &recovered).is_err());
}

#[test]
fn disabled_insim_does_not_validate_an_unused_password_or_mci_interval() {
    let config = Config {
        insim_enabled: false,
        mci_interval_ms: 0,
        insim_password: lfs_openradar::config::InSimPassword(
            "unused password longer than sixteen characters".into(),
        ),
        ..Default::default()
    };
    config.validate().unwrap();
    let old: Config = toml::from_str("").unwrap();
    assert!(old.insim_enabled);
}

#[test]
fn version_one_golden_fixture_preserves_protocol_values() {
    let fixture: Vec<Event> = include_str!("fixtures/recording-v1.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let Event::Header {
        version: 1,
        outsim_options,
        outsim_id,
        outgauge_id,
        ..
    } = &fixture[0]
    else {
        panic!("unexpected fixture header")
    };
    for event in &fixture[1..3] {
        let Event::Packet {
            stream,
            source_time_ms,
            data_hex,
            ..
        } = event
        else {
            panic!("expected packet")
        };
        let bytes: Vec<u8> = data_hex
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        match stream {
            Stream::Outsim => {
                let sample = outsim::decode_telemetry(&bytes, *outsim_options, *outsim_id).unwrap();
                assert_eq!(sample.sample.time_ms, *source_time_ms);
                assert_eq!((sample.sample.pose.x, sample.sample.pose.y), (-1.0, 2.0));
                assert_eq!(sample.velocity, [0.0, 30.0, 0.0]);
                assert_eq!(sample.wheels.unwrap()[3].air_temp_c, 83);
            }
            Stream::Outgauge => {
                let sample = outgauge::decode_telemetry(&bytes, *outgauge_id).unwrap();
                assert_eq!(sample.sample.car, "XFG");
                assert_eq!(sample.sample.time_ms, *source_time_ms);
                assert_eq!(sample.throttle, 0.7);
            }
        }
    }
    assert!(matches!(
        fixture.last(),
        Some(Event::Footer {
            packets: 2,
            complete: true,
            ..
        })
    ));
}
