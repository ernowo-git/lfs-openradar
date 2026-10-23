mod support;
use lfs_openradar::{config::Config, runtime::Runtime};
use std::{
    net::UdpSocket,
    thread,
    time::{Duration, Instant},
};

#[test]
fn outsim_only_inputs_receive_validated_controls_and_expire_when_paused() {
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
    let mut packet = support::full_outsim(100, 0.0, 0.0, 0.0);
    for (at, value) in [(72, 0.8_f32), (76, 0.2), (80, -1.5), (84, 0.6)] {
        packet[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        sender.send_to(&packet, config.outsim_bind).unwrap();
        if runtime.snapshot().inputs.current.is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "no standalone input sample");
        thread::sleep(Duration::from_millis(5));
    }
    let snapshot = runtime.snapshot();
    assert!(!snapshot.connected);
    let inputs = snapshot.inputs.current.unwrap();
    assert_eq!(
        [
            inputs.throttle,
            inputs.brake,
            inputs.clutch,
            inputs.steer_rad
        ],
        [0.8, 0.2, 0.6, -1.5]
    );
    assert_eq!(snapshot.inputs.history[0].1, [0.8, 0.2]);
    // A future timestamp must not make a malformed pedal sample usable.
    packet[8..12].copy_from_slice(&120_u32.to_le_bytes());
    packet[72..76].copy_from_slice(&1.1_f32.to_le_bytes());
    sender.send_to(&packet, config.outsim_bind).unwrap();
    while runtime.snapshot().malformed_packets == 0 {
        assert!(Instant::now() < deadline, "invalid controls not rejected");
        thread::sleep(Duration::from_millis(5));
    }
    packet[8..12].copy_from_slice(&100_u32.to_le_bytes());
    packet[72..76].copy_from_slice(&0.8_f32.to_le_bytes());
    while runtime.snapshot().inputs.current.is_some() {
        // A paused simulation repeats the last timestamp.
        sender.send_to(&packet, config.outsim_bind).unwrap();
        assert!(
            Instant::now() < deadline,
            "paused input sample did not expire"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(runtime.snapshot().inputs.history.is_empty());
}

#[test]
fn inputs_preferences_round_trip_and_validate_both_layouts() {
    assert!(!Config::default().inputs.enabled);
    let mut config: Config = toml::from_str("[inputs]\nenabled=true\nwindow_x=-120.0\nwindow_y=480.0\nscale=0.75\n[wayland.inputs]\nx=40.0\ny=80.0\nscale=1.5\n").unwrap();
    config.validate().unwrap();
    let restored: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert!(restored.inputs.enabled);
    assert_eq!(restored.inputs.window_x, Some(-120.0));
    assert_eq!(restored.inputs.scale, 0.75);
    assert_eq!(restored.wayland.inputs.scale, 1.5);
    config.wayland.inputs.x = Some(f32::NAN);
    assert!(config.validate().is_err());
    config.wayland.inputs.x = None;
    config.inputs.scale = 3.0;
    assert!(config.validate().is_err());
}

#[test]
fn steering_ratios_follow_the_detected_car_and_validate_saved_overrides() {
    use lfs_openradar::inputs::steering_ratio;
    let mut config = Config::default();
    assert_eq!(steering_ratio(&config, Some("XFG")), Some(15.0));
    assert_eq!(steering_ratio(&config, Some("XRG")), None);
    assert_eq!(steering_ratio(&config, Some("ABCDEF")), None);
    assert_eq!(steering_ratio(&config, None), None);
    config.cars.get_mut("XFG").unwrap().steering_ratio = Some(12.0);
    assert_eq!(steering_ratio(&config, Some("XFG")), Some(12.0));
    let restored: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(steering_ratio(&restored, Some("XFG")), Some(12.0));
    for invalid in [f32::NAN, f32::INFINITY, -1.0, 0.0, 101.0] {
        config.cars.get_mut("XFG").unwrap().steering_ratio = Some(invalid);
        assert!(config.validate().is_err());
    }
    // A car can have steering configuration without an RPM limit.
    let mut config: Config =
        toml::from_str("[cars.ABCDEF]\nname='Mod'\nsteering_ratio=10.0\n").unwrap();
    config.validate().unwrap();
    assert_eq!(steering_ratio(&config, Some("ABCDEF")), Some(10.0));
    config.set_car_steering_ratio("ABCDEF", None);
    assert!(!config.cars.contains_key("ABCDEF"));
    config.validate().unwrap();
    config.set_car_steering_ratio("ABCDEF", Some(9.0));
    assert_eq!(steering_ratio(&config, Some("ABCDEF")), Some(9.0));
    let mut config = Config::default();
    config.set_car_steering_ratio("XFG", Some(12.0));
    config.set_car_steering_ratio("XFG", None);
    assert_eq!(config.cars["XFG"].max_rpm, 8000);
    assert_eq!(steering_ratio(&config, Some("XFG")), Some(15.0));
    config.validate().unwrap();
}

#[cfg(feature = "desktop")]
#[test]
fn xfg_steering_wheel_uses_its_fifteen_to_one_ratio() {
    use lfs_openradar::overlay::session::{Monitor, OverlaySession};
    let mut config = Config {
        radar_enabled: false,
        ..Default::default()
    };
    config.inputs.enabled = true;
    let mut session = OverlaySession::new(config, true);
    session.outputs_changed(vec![Monitor {
        name: "DP-1".into(),
        size: [1920.0, 1080.0],
        scale: 1.0,
    }]);
    let context = eframe::egui::Context::default();
    let geometry = session
        .status()
        .gadgets
        .into_iter()
        .find(|g| g.gadget == lfs_openradar::overlay::Gadget::Inputs)
        .unwrap();
    let center = geometry.rect.min + eframe::egui::vec2(70.0, 78.0);
    for (road_degrees, expected, marker_offset) in [
        (-30.0_f32, "-450°", [46.0, 0.0]),
        (-6.0, "-90°", [46.0, 0.0]),
        (0.0, "+0°", [0.0, -46.0]),
        (6.0, "+90°", [-46.0, 0.0]),
        (30.0, "+450°", [-46.0, 0.0]),
    ] {
        let mut snapshot = lfs_openradar::runtime::Snapshot::default();
        snapshot.dashboard.car = Some("XFG".into());
        snapshot.inputs.current = Some(lfs_openradar::lfs::outsim::Inputs {
            throttle: 0.0,
            brake: 0.0,
            clutch: 0.0,
            handbrake: 0.0,
            steer_rad: road_degrees.to_radians(),
        });
        let output = session.paint(&context, &snapshot);
        assert!(
            output.shapes.iter().any(|shape| matches!(&shape.shape,
            eframe::egui::Shape::Text(text) if text.galley.text() == expected)),
            "XFG road angle {road_degrees} must display steering wheel angle {expected}"
        );
        let marker = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                eframe::egui::Shape::LineSegment { points, stroke } if stroke.width == 4.0 => {
                    Some(points[1])
                }
                _ => None,
            })
            .expect("steering wheel marker");
        assert!(
            (marker - (center + eframe::egui::Vec2::from(marker_offset))).length() < 0.01,
            "icon must rotate by the same steering-wheel angle as its number"
        );
    }
}

#[cfg(feature = "desktop")]
#[test]
fn native_inputs_use_independent_layout_shared_painter_and_temporary_visibility() {
    use lfs_openradar::overlay::{
        Gadget,
        session::{Monitor, OverlaySession},
    };
    let mut config = Config {
        radar_enabled: false,
        ..Default::default()
    };
    config.inputs.enabled = true;
    config.inputs.window_x = Some(-100.0);
    config.inputs.scale = 0.5;
    config.wayland.inputs.x = Some(40.0);
    config.wayland.inputs.y = Some(60.0);
    config.wayland.inputs.scale = 1.5;
    let mut session = OverlaySession::new(config, true);
    session.outputs_changed(vec![Monitor {
        name: "DP-1".into(),
        size: [1920.0, 1080.0],
        scale: 1.25,
    }]);
    let status = session.status();
    let gadget = status
        .gadgets
        .iter()
        .find(|g| g.gadget == Gadget::Inputs)
        .unwrap();
    assert_eq!(gadget.rect.min, eframe::egui::pos2(40.0, 60.0));
    assert_eq!(gadget.scale, 1.5);
    let snapshot = lfs_openradar::runtime::Snapshot {
        dashboard: lfs_openradar::dashboard::DashboardFrame {
            car: Some("XFG".into()),
            ..Default::default()
        },
        inputs: lfs_openradar::demo::inputs_preview(2500),
        ..Default::default()
    };
    let output = session.paint(&eframe::egui::Context::default(), &snapshot);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        eframe::egui::Shape::Text(text) if text.galley.text() == "STEER")));
    assert_eq!(session.config_for_save().inputs.window_x, Some(-100.0));
    session.hidden_gadgets.insert(Gadget::Inputs);
    assert!(!session.status().visible);
    assert!(session.config_for_save().inputs.enabled);
}
