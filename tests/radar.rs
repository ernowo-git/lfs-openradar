mod support;
use lfs_openradar::{
    config::Config,
    lfs::{
        insim::{Packet, State},
        outsim::Sample,
    },
    radar::{Pose, footprints_intersect, local_offset},
};
use support::*;

#[test]
fn cardinal_transform_and_rotated_footprints() {
    for heading in [
        0.0,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
        3.0 * std::f64::consts::FRAC_PI_2,
        0.71,
    ] {
        let (sin, cos) = heading.sin_cos();
        let me = Pose {
            heading,
            ..Default::default()
        };
        let ahead = local_offset(
            me,
            Pose {
                x: -sin * 5.0,
                y: cos * 5.0,
                ..Default::default()
            },
        );
        let right = local_offset(
            me,
            Pose {
                x: cos * 3.0,
                y: sin * 3.0,
                ..Default::default()
            },
        );
        assert!(ahead[0].abs() < 1e-9 && (ahead[1] - 5.0).abs() < 1e-9);
        assert!((right[0] - 3.0).abs() < 1e-9 && right[1].abs() < 1e-9);
    }
    assert!(footprints_intersect(0.0, 4.1, 0.0, 1.8, 4.2, 0.0));
    assert!(!footprints_intersect(2.5, 0.0, 0.0, 1.8, 4.2, 0.0));
    assert!(footprints_intersect(
        2.5,
        0.0,
        std::f64::consts::FRAC_PI_2,
        1.8,
        4.2,
        0.0
    ));
}
#[test]
fn outsim_is_required_and_staleness_pauses() {
    let config = Config::default();
    let mut engine = setup();
    assert!(!engine.frame(0, &config).live);
    tick(&mut engine, 20, 0.0);
    tick(&mut engine, 40, 0.0);
    assert!(engine.frame(100, &config).live);
    assert!(engine.frame(350, &config).uncertain);
    assert!(!engine.frame(600, &config).live);
    engine.packet(Packet::Reset(1), 601).unwrap();
    assert!(!engine.frame(602, &config).live);
}
#[test]
fn local_ai_is_not_selected_and_wrong_pose_is_rejected() {
    let config = Config::default();
    let mut engine = setup();
    tick(&mut engine, 20, 0.0);
    tick(&mut engine, 40, 0.0);
    let frame = engine.frame(100, &config);
    assert_eq!(frame.driver.as_deref(), Some("Driver 1"));
    engine.outsim(
        Sample {
            time_ms: 60,
            pose: Pose {
                x: 50.0,
                ..Default::default()
            },
        },
        60,
        &config,
    );
    assert!(!engine.frame(100, &config).live);
    assert_eq!(engine.rejected_outsim, 1);
}
#[test]
fn camera_track_takeover_and_reused_plid_clear_history() {
    let config = Config::default();
    for packet in [
        Packet::Leave(1),
        Packet::Pit(1),
        Packet::Takeover(1),
        Packet::Camera(1),
        player(1, 0),
        Packet::State(State {
            flags: 1,
            camera: 3,
            viewed: 1,
            track: "WE1".into(),
        }),
    ] {
        let mut engine = setup();
        tick(&mut engine, 20, 0.0);
        tick(&mut engine, 40, 0.0);
        engine.packet(packet, 50).unwrap();
        assert!(!engine.frame(100, &config).live);
    }
}
#[test]
fn reordered_outsim_does_not_replace_newer_pose() {
    let config = Config::default();
    let mut engine = setup();
    tick(&mut engine, 20, 0.0);
    tick(&mut engine, 40, 0.0);
    engine.outsim(
        Sample {
            time_ms: 30,
            pose: Pose {
                heading: 0.2,
                ..Default::default()
            },
        },
        50,
        &config,
    );
    assert_eq!(engine.rejected_outsim, 1);
    assert!(engine.frame(100, &config).live);
}
#[test]
fn config_rejects_invalid_boundaries_and_roundtrips() {
    let mut config = Config {
        outsim_options: 12,
        ..Default::default()
    };
    assert!(config.validate().is_err());
    config.outsim_id = 0;
    assert!(config.validate().is_ok());
    config.hide_ms = config.stale_ms;
    assert!(config.validate().is_err());
    let encoded = toml::to_string(&Config::default()).unwrap();
    let decoded: Config = toml::from_str(&encoded).unwrap();
    assert!(decoded.validate().is_ok());
}

#[test]
fn interpolation_uses_shortest_arc_across_heading_wrap() {
    let config = Config {
        interpolation_ms: 30,
        ..Default::default()
    };
    let mut engine = setup();
    tick(&mut engine, 20, 359.0_f64.to_radians());
    tick(&mut engine, 40, 1.0_f64.to_radians());
    let frame = engine.frame(60, &config); // Render at 30 ms, halfway across wrap.
    assert!(frame.live);
    assert!(frame.cars[0].right > 2.99);
    assert!(frame.cars[0].forward.abs() < 0.001);
}

#[test]
fn outsim_clock_recovers_after_reset_and_silence() {
    let config = Config::default();
    let mut engine = setup();
    tick(&mut engine, 1000, 0.0);
    tick(&mut engine, 1020, 0.0);
    mci_tick(&mut engine, 1600, 0.0);
    mci_tick(&mut engine, 1620, 0.0);
    engine.outsim(
        Sample {
            time_ms: 20,
            pose: Pose::default(),
        },
        1620,
        &config,
    );
    engine.outsim(
        Sample {
            time_ms: 40,
            pose: Pose::default(),
        },
        1640,
        &config,
    );
    assert!(engine.frame(1700, &config).live);
    assert_eq!(engine.rejected_outsim, 0);
    engine.packet(Packet::Reset(1), 1710).unwrap();
    mci_tick(&mut engine, 1720, 0.0);
    engine.outsim(
        Sample {
            time_ms: 0,
            pose: Pose::default(),
        },
        1720,
        &config,
    );
    assert_eq!(engine.rejected_outsim, 0);
}

#[test]
fn outsim_clock_wrap_is_a_forward_update() {
    let config = Config {
        interpolation_ms: 0,
        ..Default::default()
    };
    let mut engine = setup();
    mci_tick(&mut engine, 20, 0.0);
    engine.outsim(
        Sample {
            time_ms: u32::MAX - 10,
            pose: Pose::default(),
        },
        20,
        &config,
    );
    mci_tick(&mut engine, 40, 0.0);
    engine.outsim(
        Sample {
            time_ms: 5,
            pose: Pose::default(),
        },
        40,
        &config,
    );
    assert!(engine.frame(40, &config).live);
    assert_eq!(engine.rejected_outsim, 0);
}
