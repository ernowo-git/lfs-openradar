mod support;
use lfs_openradar::{
    config::{Config, GapSettings},
    lfs::outgauge,
    runtime::Runtime,
};
use std::{
    io::{Read, Write},
    net::{TcpListener, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use support::*;

#[test]
fn mock_lfs_tcp_udp_connects_and_missing_outsim_pauses() {
    mock_lfs(false, false, 9, false);
}

#[test]
fn mock_lfs_follows_viewed_ai_and_missing_outsim_pauses() {
    mock_lfs(true, false, 9, false);
}

#[test]
fn mock_lfs_shared_outsim_outgauge_port_updates_both_gadgets() {
    mock_lfs(true, true, 10, false);
}

#[test]
fn mock_lfs_reads_local_low_beam_when_outgauge_has_no_dipped_symbol() {
    mock_lfs(false, false, 10, false);
}

#[test]
fn mock_lfs_fuel_only_receives_outgauge_on_separate_and_shared_ports() {
    mock_lfs(false, false, 9, true);
    mock_lfs(true, true, 10, true);
}

fn mock_lfs(follow: bool, shared: bool, protocol: u8, fuel_only: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let udp_reservation = UdpSocket::bind("127.0.0.1:0").unwrap();
    let gauge_reservation = UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = Config {
        outgauge_bind: if shared {
            udp_reservation.local_addr().unwrap()
        } else {
            gauge_reservation.local_addr().unwrap()
        },
        outgauge_id: if shared { 1 } else { 24602 },
        speed_dashboard: GapSettings {
            enabled: !fuel_only,
            ..Default::default()
        },
        fuel: GapSettings {
            enabled: true,
            ..Default::default()
        },
        insim_address: listener.local_addr().unwrap(),
        outsim_bind: udp_reservation.local_addr().unwrap(),
        insim_password: lfs_openradar::config::InSimPassword("test-secret".into()),
        interpolation_ms: 20,
        stale_ms: 100,
        hide_ms: 200,
        follow_viewed_car: follow,
        ..Default::default()
    };
    let destination = config.outsim_bind;
    let gauge_destination = config.outgauge_bind;
    let gauge_id = config.outgauge_id;
    drop(gauge_reservation);
    drop(udp_reservation);
    let outsim_enabled = Arc::new(AtomicBool::new(true));
    let server_stop = Arc::new(AtomicBool::new(false));
    let send_outsim = outsim_enabled.clone();
    let gauge_enabled = Arc::new(AtomicBool::new(true));
    let send_gauge = gauge_enabled.clone();
    let stop = server_stop.clone();
    let server = thread::spawn(move || {
        let (mut tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut init = [0; 44];
        tcp.read_exact(&mut init).unwrap();
        assert_eq!(init[8], 9);
        assert_eq!(&init[12..24], b"test-secret\0");
        assert_eq!(&init[4..6], &[0, 0]);
        let mut version = packet(2, 20);
        version[4..8].copy_from_slice(b"0.7G");
        version[18] = protocol;
        tcp.write_all(&version).unwrap();
        let mut requests = [0; 16];
        tcp.read_exact(&mut requests).unwrap();
        assert_eq!(
            &requests,
            &[1, 3, 1, 13, 1, 3, 1, 14, 1, 3, 1, 7, 1, 3, 1, 19]
        );
        for (id, kind) in [(1, 0), (2, if follow { 2 } else { 6 })] {
            let mut npl = packet(21, 76);
            npl[2] = 1;
            npl[3] = id;
            npl[4] = id;
            npl[5] = kind;
            npl[73] = 2;
            npl[72] = lfs_openradar::lfs::insim::SETF_ABS_ENABLE;
            npl[40..44].copy_from_slice(b"XFG\0");
            npl[8..11].copy_from_slice(if id == 2 { b"AI2" } else { b"You" });
            tcp.write_all(&npl).unwrap();
        }
        let mut state = packet(5, 28);
        state[8] = 1;
        state[10] = 3;
        state[11] = if follow { 2 } else { 1 };
        state[20..23].copy_from_slice(b"BL1");
        tcp.write_all(&state).unwrap();
        let mut track = packet(17, 28);
        track[2] = 1;
        track[4] = 10;
        track[7] = 0x40;
        track[8..11].copy_from_slice(b"BL1");
        track[18..20].copy_from_slice(&1000_u16.to_le_bytes());
        tcp.write_all(&track).unwrap();
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut time = 20;
        let mut framer = lfs_openradar::lfs::insim::Framer::default();
        let mut incoming = [0; 128];
        tcp.set_read_timeout(Some(Duration::from_millis(1)))
            .unwrap();
        while !stop.load(Ordering::Relaxed) {
            let mut mci = mci(&[1, 2], true, true);
            mci[9] = 1;
            mci[37] = 2;
            // Deliberately split the TCP packet header across writes.
            if tcp.write_all(&mci[..2]).is_err() {
                break;
            }
            if tcp.write_all(&mci[2..]).is_err() {
                break;
            }
            if send_outsim.load(Ordering::Relaxed) {
                udp.send_to(
                    &full_outsim(time, -1.0, 2.0, std::f32::consts::FRAC_PI_2),
                    destination,
                )
                .unwrap();
                if !shared {
                    udp.send_to(&[0; 96], destination).unwrap();
                } // Unconfigured OutGauge traffic.
            }
            if send_gauge.load(Ordering::Relaxed) {
                let mut gauge = vec![0; 96];
                gauge[..4].copy_from_slice(&time.to_le_bytes());
                gauge[4..8].copy_from_slice(b"XFG\0");
                gauge[8..10].copy_from_slice(&16384_u16.to_le_bytes());
                gauge[10] = 6;
                gauge[11] = if follow { 2 } else { 1 };
                gauge[12..16].copy_from_slice(&20_f32.to_le_bytes());
                gauge[16..20].copy_from_slice(&6400_f32.to_le_bytes());
                gauge[28..32].copy_from_slice(&0.14_f32.to_le_bytes());
                gauge[40..44].copy_from_slice(&outgauge::ENGINE.to_le_bytes());
                gauge[44..48]
                    .copy_from_slice(&(outgauge::ENGINE | outgauge::ENGINE_SEVERE).to_le_bytes());
                gauge[92..96].copy_from_slice(&gauge_id.to_le_bytes());
                udp.send_to(&gauge, gauge_destination).unwrap();
            }
            if protocol == 10
                && let Ok(size) = tcp.read(&mut incoming)
            {
                for request in framer.push(&incoming[..size]).unwrap() {
                    if request[1] == 3 && request[3] == 30 {
                        assert!(
                            !follow,
                            "must not apply the human's local lights to a followed AI"
                        );
                        assert_ne!(request[2], 0);
                        let mut reply = packet(4, 8);
                        reply[2] = request[2];
                        reply[3] = 10;
                        reply[4..8].copy_from_slice(&(2_u32 << 18).to_le_bytes());
                        tcp.write_all(&reply).unwrap();
                    }
                }
            }
            time += 20;
            thread::sleep(Duration::from_millis(20));
        }
    });
    let runtime = Runtime::start(config).unwrap();
    let overlay_reader = runtime.snapshot_reader();
    let start = Instant::now();
    while (!overlay_reader.snapshot().frame.live
        || overlay_reader.snapshot().dashboard.sample.is_none()
        || overlay_reader.snapshot().fuel.fraction.is_none()
        || (protocol == 10
            && !follow
            && !fuel_only
            && overlay_reader.snapshot().dashboard.headlight_switch != Some(2)))
        && start.elapsed() < Duration::from_secs(3)
    {
        thread::sleep(Duration::from_millis(20));
    }
    let live = overlay_reader.snapshot();
    assert!(live.frame.live, "{live:?}");
    let dashboard = live
        .dashboard
        .sample
        .as_ref()
        .expect("fresh matching OutGauge");
    assert_eq!(live.dashboard.abs_enabled, Some(true));
    assert_eq!(
        live.dashboard.headlight_switch,
        if protocol == 10 && !follow && !fuel_only {
            Some(2)
        } else {
            None
        }
    );
    assert_eq!(dashboard.gear_label(), "5");
    assert_eq!(dashboard.rpm, 6400.0);
    assert!((live.fuel.fraction.unwrap() - 0.14).abs() < 0.00001);
    assert_ne!(dashboard.lights & outgauge::ENGINE_SEVERE, 0);
    assert_eq!(live.frame.cars.len(), 1);
    assert_eq!(live.version, "0.7G");
    assert_eq!(live.malformed_packets, 0);
    if follow {
        assert_eq!(live.frame.driver.as_deref(), Some("AI2"));
        assert_eq!(live.gaps.ahead.position, Some(1));
        assert_eq!(live.gaps.ahead.status, "Building passage history");
        assert_eq!(live.gaps.behind.status, "No driver");
    } else {
        assert_eq!(live.gaps.ahead.status, "No driver");
        assert_eq!(live.gaps.behind.position, Some(2));
        assert_eq!(live.gaps.behind.status, "Building passage history");
    }
    outsim_enabled.store(false, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(350));
    let paused = overlay_reader.snapshot();
    assert!(paused.connected);
    assert!(!paused.frame.live);
    assert!(paused.frame.status.contains("stale"));
    assert!(
        paused.dashboard.sample.is_some(),
        "dashboard does not depend on OutSim"
    );
    assert!(
        paused.fuel.fraction.is_some(),
        "fuel does not depend on OutSim"
    );
    gauge_enabled.store(false, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(200));
    assert!(overlay_reader.snapshot().dashboard.sample.is_none());
    assert!(overlay_reader.snapshot().fuel.fraction.is_none());
    assert!(
        overlay_reader
            .snapshot()
            .dashboard
            .headlight_switch
            .is_none()
    );
    assert!(paused.gaps.ahead.seconds.is_none() && paused.gaps.behind.seconds.is_none());
    server_stop.store(true, Ordering::Relaxed);
    drop(runtime);
    server.join().unwrap();
}
