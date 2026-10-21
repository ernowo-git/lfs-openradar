mod support;
use lfs_openradar::{config::Config, runtime::Runtime};
use std::{
    net::UdpSocket,
    thread,
    time::{Duration, Instant},
};

#[test]
fn outsim_only_meter_receives_validated_physics_without_insim_or_outgauge() {
    let reserved = UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = Config {
        insim_enabled: false,
        outsim_bind: reserved.local_addr().unwrap(),
        stale_ms: 100,
        ..Default::default()
    };
    drop(reserved);
    let runtime = Runtime::start(config.clone()).unwrap();
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut physics = support::full_outsim(100, 0.0, 0.0, 0.0);
    physics[40..44].copy_from_slice(&(-9.80665_f32).to_le_bytes());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        sender.send_to(&physics, config.outsim_bind).unwrap();
        if runtime.snapshot().g_force.current.is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "no standalone G-force sample");
        thread::sleep(Duration::from_millis(5));
    }
    let snapshot = runtime.snapshot();
    assert!(!snapshot.connected);
    assert!(snapshot.dashboard.sample.is_none());
    assert!((snapshot.g_force.current.unwrap()[1] + 1.0).abs() < 0.0001);
    physics[8..12].copy_from_slice(&120_u32.to_le_bytes());
    physics[36..40].copy_from_slice(&f32::NAN.to_le_bytes());
    sender.send_to(&physics, config.outsim_bind).unwrap();
    while runtime.snapshot().malformed_packets == 0 {
        assert!(Instant::now() < deadline, "invalid telemetry not rejected");
        thread::sleep(Duration::from_millis(5));
    }
    while runtime.snapshot().g_force.current.is_some() {
        assert!(Instant::now() < deadline, "G-force did not become stale");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(runtime.snapshot().g_force.peak_g > 0.99);
}

#[test]
fn meter_preferences_round_trip_and_validate_both_layouts() {
    assert_eq!(Config::default().g_force_range_g, 3);
    let mut config: Config = toml::from_str("g_force_range_g = 3\n[g_force]\nenabled = true\nwindow_x = -120.0\nscale = 0.75\n[wayland.g_force]\nx = 40.0\ny = 80.0\nscale = 1.5\n").unwrap();
    config.validate().unwrap();
    let restored: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(restored.g_force_range_g, 3);
    assert!(restored.g_force.enabled);
    assert_eq!(restored.g_force.window_x, Some(-120.0));
    assert_eq!(restored.wayland.g_force.scale, 1.5);
    for range in [0, 5, 255] {
        config.g_force_range_g = range;
        assert!(config.validate().is_err());
    }
    config.g_force_range_g = 2;
    config.wayland.g_force.x = Some(f32::NAN);
    assert!(config.validate().is_err());
    config.wayland.g_force.x = None;
    config.g_force.scale = 3.0;
    assert!(config.validate().is_err());
    assert!(!Config::default().g_force.enabled);
}

#[cfg(feature = "desktop")]
#[test]
fn native_meter_uses_its_own_layout_and_shared_painter() {
    use lfs_openradar::overlay::{
        Gadget,
        session::{Monitor, OverlaySession},
    };
    let mut config = Config {
        radar_enabled: false,
        ..Default::default()
    };
    config.g_force.enabled = true;
    config.g_force.scale = 0.5;
    config.g_force.window_x = Some(-100.0);
    config.wayland.g_force.x = Some(40.0);
    config.wayland.g_force.y = Some(60.0);
    config.wayland.g_force.scale = 1.5;
    let mut session = OverlaySession::new(config, true);
    session.outputs_changed(vec![Monitor {
        name: "DP-1".into(),
        size: [1920.0, 1080.0],
        scale: 1.0,
    }]);
    let status = session.status();
    let meter = status
        .gadgets
        .iter()
        .find(|g| g.gadget == Gadget::GForce)
        .unwrap();
    assert_eq!(meter.rect.min, eframe::egui::pos2(40.0, 60.0));
    assert_eq!(meter.scale, 1.5);
    let snapshot = lfs_openradar::runtime::Snapshot {
        g_force: lfs_openradar::demo::g_force_preview(1000),
        ..Default::default()
    };
    let output = session.paint(&eframe::egui::Context::default(), &snapshot);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        eframe::egui::Shape::Text(text) if text.galley.text() == "G-FORCE")));
    assert_eq!(session.config_for_save().g_force.window_x, Some(-100.0));
    session.hidden_gadgets.insert(Gadget::GForce);
    assert!(!session.status().visible);
    assert!(session.config_for_save().g_force.enabled);
}
