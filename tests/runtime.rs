mod support;
use lfs_openradar::{config::Config, runtime::Runtime};
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
    mock_lfs(false);
}

#[test]
fn mock_lfs_follows_viewed_ai_and_missing_outsim_pauses() {
    mock_lfs(true);
}

fn mock_lfs(follow: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let udp_reservation = UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = Config {
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
    drop(udp_reservation);
    let outsim_enabled = Arc::new(AtomicBool::new(true));
    let server_stop = Arc::new(AtomicBool::new(false));
    let send_outsim = outsim_enabled.clone();
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
        version[18] = 9;
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
                udp.send_to(&[0; 96], destination).unwrap(); // Shared OutGauge traffic.
            }
            time += 20;
            thread::sleep(Duration::from_millis(20));
        }
    });
    let runtime = Runtime::start(config).unwrap();
    let overlay_reader = runtime.snapshot_reader();
    let start = Instant::now();
    while !overlay_reader.snapshot().frame.live && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(20));
    }
    let live = overlay_reader.snapshot();
    assert!(live.frame.live, "{live:?}");
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
    assert!(paused.gaps.ahead.seconds.is_none() && paused.gaps.behind.seconds.is_none());
    server_stop.store(true, Ordering::Relaxed);
    drop(runtime);
    server.join().unwrap();
}
