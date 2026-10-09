use lfs_openradar::{
    config::{CarProfile, Config},
    dashboard::DashboardTelemetry,
    lfs::{
        insim::{Player, car_name},
        outgauge::{self, Sample},
    },
};

fn packet(id: i32) -> Vec<u8> {
    let mut bytes = vec![0; if id == 0 { 92 } else { 96 }];
    bytes[..4].copy_from_slice(&100_u32.to_le_bytes());
    bytes[4..8].copy_from_slice(b"XFG\0");
    bytes[8..10].copy_from_slice(&16384_u16.to_le_bytes());
    bytes[10] = 6;
    bytes[11] = 1;
    bytes[12..16].copy_from_slice(&20_f32.to_le_bytes());
    bytes[16..20].copy_from_slice(&6400_f32.to_le_bytes());
    bytes[40..44]
        .copy_from_slice(&(outgauge::ABS | outgauge::ENGINE | outgauge::HEADLIGHTS).to_le_bytes());
    bytes[44..48].copy_from_slice(
        &(outgauge::ABS | outgauge::ENGINE | outgauge::ENGINE_SEVERE).to_le_bytes(),
    );
    if id != 0 {
        bytes[92..96].copy_from_slice(&id.to_le_bytes());
    }
    bytes
}

#[test]
fn outgauge_decodes_units_gears_lamps_and_rejects_bad_packets() {
    for id in [0, 24602] {
        let mut bytes = packet(id);
        let sample = outgauge::decode(&bytes, id).unwrap();
        assert_eq!(sample.car, "XFG");
        assert_eq!(sample.gear_label(), "5");
        assert!((sample.speed() - 72.0).abs() < 0.001);
        assert_eq!(sample.lamp(outgauge::ABS), Some(true));
        assert_eq!(sample.lamp(outgauge::HEADLIGHTS), Some(false));
        assert_eq!(sample.lamp(outgauge::TC), None);
        assert_ne!(sample.lights & outgauge::ENGINE_SEVERE, 0);
        bytes[8..10].fill(0);
        bytes[10] = 0;
        let sample = outgauge::decode(&bytes, id).unwrap();
        assert_eq!(sample.gear_label(), "R");
        assert!((sample.speed() - 44.73873).abs() < 0.001);
        bytes[10] = 1;
        assert_eq!(outgauge::decode(&bytes, id).unwrap().gear_label(), "N");
        assert!(outgauge::decode(&bytes[..bytes.len() - 1], id).is_err());
        bytes[16..20].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(outgauge::decode(&bytes, id).is_err());
    }
    assert!(outgauge::decode(&packet(24602), 99).is_err());
    assert_eq!(car_name(&[0x07, 0xd8, 0xa4, 0]), "A4D807");
}

#[test]
fn dashboard_requires_matching_fresh_ordered_samples_and_clears_on_lifecycle() {
    let config = Config::default();
    let player = Player {
        plid: 1,
        ucid: 1,
        kind: 0,
        name: "You".into(),
        model: "XFG".into(),
        abs_enabled: true,
        in_garage: false,
    };
    let mut telemetry = DashboardTelemetry::default();
    let sample = outgauge::decode(&packet(0), 0).unwrap();
    telemetry.receive(
        Sample {
            plid: 2,
            ..sample.clone()
        },
        0,
        Some(&player),
        config.stale_ms,
    );
    telemetry.receive(
        Sample {
            car: "XRG".into(),
            ..sample.clone()
        },
        0,
        Some(&player),
        config.stale_ms,
    );
    assert!(telemetry.frame(0, &config, Some(&player)).sample.is_none());
    telemetry.receive(sample.clone(), 0, Some(&player), config.stale_ms);
    assert_eq!(
        telemetry.frame(0, &config, Some(&player)).abs_enabled,
        Some(true)
    );
    assert_eq!(
        telemetry
            .frame(config.stale_ms + 1, &config, Some(&player))
            .abs_enabled,
        None
    );
    telemetry.receive(
        Sample {
            time_ms: 99,
            rpm: 9999.0,
            ..sample.clone()
        },
        10,
        Some(&player),
        config.stale_ms,
    );
    assert_eq!(
        telemetry
            .frame(10, &config, Some(&player))
            .sample
            .unwrap()
            .rpm,
        6400.0
    );
    assert!(
        telemetry
            .frame(config.stale_ms + 1, &config, Some(&player))
            .sample
            .is_none()
    );
    assert!(telemetry.frame(10, &config, None).sample.is_none());
    telemetry.clear();
    assert!(telemetry.frame(10, &config, Some(&player)).sample.is_none());
    telemetry.receive(
        Sample {
            time_ms: u32::MAX,
            ..sample.clone()
        },
        20,
        Some(&player),
        config.stale_ms,
    );
    telemetry.receive(
        Sample {
            time_ms: 0,
            rpm: 7000.0,
            ..sample.clone()
        },
        21,
        Some(&player),
        config.stale_ms,
    );
    assert_eq!(
        telemetry
            .frame(21, &config, Some(&player))
            .sample
            .unwrap()
            .rpm,
        7000.0
    );
}

#[test]
fn car_profiles_round_trip_and_validate_without_inventing_rpm_limits() {
    let mut config: Config = toml::from_str(
        "[cars.XFG]\nname='XF GTI'\nmax_rpm=8000\n[speed_dashboard]\nenabled=true\nscale=1.5",
    )
    .unwrap();
    config.validate().unwrap();
    let roundtrip: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(roundtrip.cars["XFG"].max_rpm, 8000);
    assert!(roundtrip.speed_dashboard.enabled);
    assert_eq!(Config::default().cars["XFG"].max_rpm, 8000);
    config.cars.insert(
        "XFG".into(),
        CarProfile {
            name: "XF GTI".into(),
            max_rpm: 0,
            fuel_tank_litres: None,
        },
    );
    assert!(config.validate().is_err());
}

#[test]
fn switching_drivers_accepts_the_new_cars_clock_immediately() {
    let config = Config::default();
    let mut player = Player {
        plid: 1,
        ucid: 1,
        kind: 0,
        name: "You".into(),
        model: "XFG".into(),
        abs_enabled: true,
        in_garage: false,
    };
    let mut telemetry = DashboardTelemetry::default();
    let sample = outgauge::decode(&packet(0), 0).unwrap();
    telemetry.receive(sample.clone(), 0, Some(&player), config.stale_ms);
    player.plid = 2;
    assert!(telemetry.frame(1, &config, Some(&player)).sample.is_none());
    telemetry.receive(
        Sample {
            plid: 2,
            time_ms: 1,
            rpm: 2000.0,
            ..sample
        },
        1,
        Some(&player),
        config.stale_ms,
    );
    assert_eq!(
        telemetry
            .frame(1, &config, Some(&player))
            .sample
            .unwrap()
            .rpm,
        2000.0
    );
}

#[test]
fn shared_telemetry_port_requires_distinguishable_packet_sizes() {
    let mut config = Config::default();
    config.speed_dashboard.enabled = true;
    config.outgauge_bind = config.outsim_bind;
    config.validate().unwrap();
    config.outsim_options = 31; // 92-byte configurable OutSim conflicts with OutGauge.
    assert!(config.validate().unwrap_err().contains("separate port"));
    config.outgauge_bind = "127.0.0.1:30001".parse().unwrap();
    config.validate().unwrap();
}

#[test]
fn outgauge_forwarding_round_trips_and_rejects_invalid_or_looping_destinations() {
    let legacy: Config = toml::from_str("").unwrap();
    assert!(legacy.outgauge_forward.is_empty());
    assert!(!legacy.needs_outgauge());
    let mut config: Config = toml::from_str("outgauge_forward = '127.0.0.1:60000'").unwrap();
    assert!(config.needs_outgauge());
    config.validate().unwrap();
    let saved: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(saved.outgauge_forward, config.outgauge_forward);
    let multiple: Config =
        toml::from_str("outgauge_forward = ['127.0.0.1:60000', '127.0.0.1:60001']").unwrap();
    multiple.validate().unwrap();
    assert_eq!(multiple.outgauge_forward.len(), 2);
    let saved: Config = toml::from_str(&toml::to_string(&multiple).unwrap()).unwrap();
    assert_eq!(saved.outgauge_forward, multiple.outgauge_forward);
    for destination in [
        "127.0.0.1:0".parse().unwrap(),
        "192.168.1.1:60000".parse().unwrap(),
        "[::1]:60000".parse().unwrap(),
        config.outgauge_bind,
        config.outsim_bind,
    ] {
        config.outgauge_forward = vec!["127.0.0.1:60000".parse().unwrap(), destination];
        assert!(config.validate().is_err(), "{destination}");
    }
    config.outgauge_forward = vec!["127.0.0.1:60000".parse().unwrap()];
    config.outgauge_bind = config.outsim_bind;
    config.outsim_options = 31;
    assert!(config.validate().unwrap_err().contains("separate port"));
}

#[test]
fn blink_threshold_defaults_to_ninety_five_and_round_trips_with_valid_bounds() {
    let legacy: Config = toml::from_str("[speed_dashboard]\nenabled=true").unwrap();
    assert_eq!(legacy.rpm_blink_threshold_percent, 95);
    let mut config: Config = toml::from_str("rpm_blink_threshold_percent = 85").unwrap();
    config.validate().unwrap();
    let saved: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(saved.rpm_blink_threshold_percent, 85);
    for threshold in [0, 101] {
        config.rpm_blink_threshold_percent = threshold;
        assert!(config.validate().is_err());
    }
    for threshold in [1, 100] {
        config.rpm_blink_threshold_percent = threshold;
        config.validate().unwrap();
    }
}

#[test]
fn blink_interval_defaults_to_one_hundred_and_saves_with_valid_bounds() {
    let legacy: Config = toml::from_str("[speed_dashboard]\nenabled=true").unwrap();
    assert_eq!(legacy.rpm_blink_interval_ms, 100);
    let mut config: Config = toml::from_str("rpm_blink_interval_ms = 50").unwrap();
    config.validate().unwrap();
    let saved: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(saved.rpm_blink_interval_ms, 50);
    config.rpm_blink_interval_ms = 500;
    config.validate().unwrap();
    for interval in [0, 49, 501] {
        config.rpm_blink_interval_ms = interval;
        assert!(config.validate().is_err());
    }
}

#[test]
fn local_lights_expire_and_never_transfer_to_an_ai_or_another_car() {
    let config = Config::default();
    let mut player = Player {
        plid: 1,
        ucid: 1,
        kind: 0,
        name: "You".into(),
        model: "XFG".into(),
        abs_enabled: true,
        in_garage: false,
    };
    let mut telemetry = DashboardTelemetry::default();
    let sample = outgauge::decode(&packet(0), 0).unwrap();
    telemetry.receive(sample.clone(), 0, Some(&player), config.stale_ms);
    telemetry.receive_local_lights(2, 0, &player);
    assert_eq!(
        telemetry.frame(0, &config, Some(&player)).headlight_switch,
        Some(2)
    );
    player.kind = 2; // Followed AI: local human's switch does not describe this car.
    telemetry.receive_local_lights(3, 1, &player);
    assert_eq!(
        telemetry.frame(1, &config, Some(&player)).headlight_switch,
        None
    );
    player.kind = 0;
    player.model = "XRG".into();
    telemetry.receive(
        Sample {
            car: "XRG".into(),
            time_ms: 101,
            ..sample.clone()
        },
        2,
        Some(&player),
        config.stale_ms,
    );
    assert_eq!(
        telemetry.frame(2, &config, Some(&player)).headlight_switch,
        None
    );
    player.model = "XFG".into();
    telemetry.receive(
        Sample {
            time_ms: 102,
            ..sample
        },
        config.stale_ms + 1,
        Some(&player),
        config.stale_ms,
    );
    assert_eq!(
        telemetry
            .frame(config.stale_ms + 1, &config, Some(&player))
            .headlight_switch,
        None
    );
    telemetry.receive_local_lights(2, config.stale_ms + 1, &player);
    assert_eq!(
        telemetry
            .frame(config.stale_ms + 1, &config, Some(&player))
            .headlight_switch,
        Some(2)
    );
    telemetry.clear();
    assert_eq!(
        telemetry
            .frame(config.stale_ms + 1, &config, Some(&player))
            .headlight_switch,
        None
    );
}
