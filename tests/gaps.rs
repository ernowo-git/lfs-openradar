use lfs_openradar::{
    config::Config,
    demo::Demo,
    gaps::GapEngine,
    lfs::insim::{Car, Player, TrackInfo},
    radar::Pose,
};
use std::collections::BTreeMap;

fn track() -> TrackInfo {
    TrackInfo {
        track: "BL1".into(),
        nodes: 1000,
        finish: 200,
        timing: 0x40,
        race_laps: 10,
    }
}
fn car(id: u8, position: u8, progress: i64) -> Car {
    Car {
        plid: id,
        position,
        node: ((progress + 200) % 1000) as u16,
        lap: (progress / 1000) as u16,
        info: 0,
        pose: Pose {
            y: progress as f64,
            ..Default::default()
        },
        speed_mps: 10.0,
        direction: 0.0,
    }
}
fn players() -> BTreeMap<u8, Player> {
    (1..=4)
        .map(|id| {
            (
                id,
                Player {
                    plid: id,
                    ucid: id,
                    kind: if id == 1 { 0 } else { 6 },
                    name: format!("Driver {id}"),
                    model: "XRG".into(),
                    in_garage: false,
                },
            )
        })
        .collect()
}
fn history() -> GapEngine {
    let mut engine = GapEngine::default();
    engine.set_track(track());
    // Cross both the raw node wrap and finish-line/lap boundary.
    for t in (0..=10_000).step_by(100) {
        let progress = 1940 + t as i64 / 100;
        engine.update(
            &[
                car(1, 2, progress),
                car(2, 1, progress + 50),
                car(3, 3, progress - 23),
                car(4, 4, progress - 1),
            ],
            t,
        );
    }
    engine
}
#[test]
fn five_second_gap_and_behind_cross_wrap_with_race_neighbors() {
    let frame = history().frame(Some(1), &players(), 10_000, 250);
    assert_eq!(frame.ahead.seconds, Some(5.0));
    assert_eq!(frame.behind.seconds, Some(2.3));
    assert_eq!(frame.ahead.driver.as_deref(), Some("Driver 2"));
    assert_eq!(frame.behind.driver.as_deref(), Some("Driver 3"));
    assert_eq!(frame.ahead.laps, None);
    assert_eq!(frame.behind.measured_age_ms, Some(0));
}
#[test]
fn changing_speed_still_compares_the_same_point_and_expired_history_is_unavailable() {
    let mut engine = GapEngine::default();
    engine.set_track(track());
    let progress = |time: i64| {
        if time < 5_000 {
            1940 + time / 100
        } else {
            1990 + (time - 5_000) / 50
        }
    };
    for time in (0..=12_000).step_by(50) {
        engine.update(
            &[
                car(1, 2, progress(time as i64)),
                car(2, 1, progress(time as i64 + 5_000)),
                car(3, 3, progress(time as i64 - 2_300)),
            ],
            time,
        );
    }
    let frame = engine.frame(Some(1), &players(), 12_000, 250);
    assert_eq!(frame.ahead.seconds, Some(5.0));
    assert_eq!(frame.behind.seconds, Some(2.3));
    let mut engine = GapEngine::default();
    engine.set_track(track());
    for time in (0..=40_000).step_by(100) {
        let progress = 1940 + time as i64 / 100;
        engine.update(&[car(1, 2, progress), car(2, 1, progress + 350)], time);
    }
    assert!(
        engine
            .frame(Some(1), &players(), 40_000, 250)
            .ahead
            .seconds
            .is_none()
    );
}
#[test]
fn insufficient_history_stale_and_stopped_cars_do_not_invent_values() {
    let mut engine = GapEngine::default();
    engine.set_track(track());
    engine.update(&[car(1, 2, 1940), car(2, 1, 1990), car(3, 3, 1917)], 0);
    assert!(
        engine
            .frame(Some(1), &players(), 0, 250)
            .ahead
            .seconds
            .is_none()
    );
    let mut engine = history();
    assert!(
        engine
            .frame(Some(1), &players(), 10_251, 250)
            .ahead
            .seconds
            .is_none()
    );
    for t in (10_100..=12_100).step_by(100) {
        engine.update(&[car(1, 2, 2040), car(2, 1, 2090), car(3, 3, 2017)], t);
    }
    let stopped = engine.frame(Some(1), &players(), 12_100, 250);
    assert_eq!(stopped.ahead.status, "Waiting for progress");
    assert!(stopped.ahead.seconds.is_none());
}
#[test]
fn order_changes_ties_unknown_positions_and_lapped_drivers() {
    let mut engine = history();
    engine.update(&[car(1, 2, 2041), car(2, 3, 2091), car(3, 1, 2018)], 10_100);
    let frame = engine.frame(Some(1), &players(), 10_100, 250);
    assert_eq!(frame.ahead.driver.as_deref(), Some("Driver 3"));
    assert!(frame.ahead.seconds.is_none()); // Order has not caught up with progress.
    engine.update(&[car(1, 2, 2042), car(2, 1, 2092), car(3, 1, 2019)], 10_200);
    assert_eq!(
        engine.frame(Some(1), &players(), 10_200, 250).ahead.status,
        "Ambiguous race order"
    );
    engine.update(&[car(1, 0, 2043), car(2, 1, 2093)], 10_300);
    assert_eq!(
        engine.frame(Some(1), &players(), 10_300, 250).ahead.status,
        "Race position unknown"
    );
    engine.update(&[car(1, 2, 2044), car(2, 1, 3100)], 10_400);
    assert_eq!(
        engine.frame(Some(1), &players(), 10_400, 250).ahead.laps,
        Some(1)
    );
}
#[test]
fn resets_lag_reverse_progress_and_unsupported_timing_clear_estimates() {
    for flag in [4, 8, 32] {
        let mut engine = history();
        let mut leader = car(2, 1, 2091);
        leader.info = flag;
        engine.update(&[car(1, 2, 2041), leader], 10_100);
        assert!(
            engine
                .frame(Some(1), &players(), 10_100, 250)
                .ahead
                .seconds
                .is_none()
        );
    }
    let mut engine = history();
    engine.update(&[car(1, 2, 2039), car(2, 1, 2091)], 10_100);
    assert!(
        engine
            .frame(Some(1), &players(), 10_100, 250)
            .ahead
            .seconds
            .is_none()
    );
    engine.clear();
    assert!(
        engine
            .frame(Some(1), &players(), 10_100, 250)
            .ahead
            .seconds
            .is_none()
    );
    engine.set_track(TrackInfo {
        timing: 0x80,
        ..track()
    });
    assert_eq!(
        engine.frame(Some(1), &players(), 10_100, 250).ahead.status,
        "Unsupported track timing"
    );
    engine.set_track(TrackInfo {
        race_laps: 0,
        ..track()
    });
    assert_eq!(
        engine.frame(Some(1), &players(), 10_100, 250).ahead.status,
        "Race-order gaps require a race"
    );
}
#[test]
fn demo_and_config_migration_supply_gap_widgets() {
    let config: Config = toml::from_str("overlay_x = 123.0\nside_m = 7.0").unwrap();
    assert_eq!(config.overlay_x, 123.0);
    assert!(config.radar_enabled);
    assert!(!config.gap_ahead.enabled);
    let mut demo = Demo::default();
    let snapshot = demo.snapshot(7_000, &config);
    assert_eq!(snapshot.gaps.ahead.seconds, Some(5.0));
    assert_eq!(snapshot.gaps.behind.seconds, Some(2.3));
    let mut configured = config;
    configured.gap_ahead.enabled = true;
    configured.gap_behind.window_y = Some(800.0);
    let serialized = toml::to_string(&configured).unwrap();
    let restored: Config = toml::from_str(&serialized).unwrap();
    assert!(restored.gap_ahead.enabled);
    assert_eq!(restored.gap_behind.window_y, Some(800.0));
    configured.gap_ahead.scale = f32::NAN;
    assert!(configured.validate().is_err());
}

#[test]
fn independent_window_positions_migrate_and_save_without_legacy_coordinates() {
    let mut config: Config = toml::from_str("overlay_x = 100.0\noverlay_y = 200.0\noverlay_size = 320.0\n[gap_ahead]\nenabled = true\nx = 0.5\ny = 0.12\n[gap_behind]\nenabled = true\nwindow_x = -1200.0\nwindow_y = 440.0").unwrap();
    config.validate().unwrap();
    config.prepare_gap_positions();
    assert_eq!(config.gap_ahead.window_x, Some(145.0));
    assert!((config.gap_ahead.window_y.unwrap() - 200.4).abs() < 0.001);
    assert_eq!(config.gap_behind.window_x, Some(-1200.0));
    assert_eq!(config.gap_behind.window_y, Some(440.0));
    assert_eq!(config.gap_ahead.legacy_x, None);
    let path =
        std::env::temp_dir().join(format!("openradar-gap-layout-{}.toml", std::process::id()));
    config.save(&path).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        !saved
            .lines()
            .any(|line| line.starts_with("x =") || line.starts_with("y ="))
    );
    assert_eq!(loaded.gap_ahead.window_x, config.gap_ahead.window_x);
    assert_eq!(loaded.gap_behind.window_x, Some(-1200.0));
    let mut fresh = Config::default();
    fresh.prepare_gap_positions();
    assert!(fresh.gap_ahead.window_x.unwrap() > fresh.overlay_x + fresh.overlay_size);
    assert_eq!(
        fresh.gap_behind.window_y.unwrap() - fresh.gap_ahead.window_y.unwrap(),
        100.0
    );
    fresh.gap_ahead.window_x = Some(f32::NAN);
    assert!(fresh.validate().is_err());
}
