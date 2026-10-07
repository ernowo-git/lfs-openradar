use lfs_openradar::{
    config::Config,
    delta::DeltaEngine,
    lfs::insim::{self, Car, Packet, TrackInfo},
    radar::Pose,
};

fn track() -> TrackInfo {
    TrackInfo {
        track: "BL1".into(),
        nodes: 40,
        finish: 7,
        split1: 0,
        timing: 0x40,
        race_laps: 0,
    }
}
fn car(progress: f64) -> Car {
    // A closed path permits meaningful spatial comparison on later laps.
    let angle = progress.rem_euclid(40.0) * std::f64::consts::TAU / 40.0;
    Car {
        node: ((progress.floor() as u64 + 7) % 40) as u16,
        lap: (progress / 40.0).floor() as u16,
        position: 0,
        plid: 1,
        info: 192,
        pose: Pose {
            x: 20.0 * angle.cos(),
            y: 20.0 * angle.sin(),
            ..Default::default()
        },
        speed_mps: 30.0,
        direction: 0.0,
    }
}
fn setup() -> DeltaEngine {
    let mut e = DeltaEngine::default();
    e.set_track(track());
    e.select_driver(Some((1, "XRG".into())));
    e
}
fn race_setup() -> DeltaEngine {
    let mut engine = setup();
    engine.set_track(TrackInfo {
        split1: 17, // Offset 10 from the finish, including a raw-node wrap.
        timing: 0x41,
        race_laps: 5,
        ..track()
    });
    engine
}
fn opening_lap(
    engine: &mut DeltaEngine,
    reports_first: bool,
    invalid_at: Option<u64>,
    split: Option<(u32, u8)>,
    lap: Option<(u32, u8)>,
) {
    // Connected after the start: no initial finish crossing was observed.
    for progress in 5..=40 {
        let time = (progress - 5) * 100;
        if invalid_at == Some(progress) {
            engine.invalidate();
        }
        if reports_first
            && progress == 10
            && let Some((ms, penalty)) = split
        {
            engine.split_report(ms, penalty, time);
        }
        if reports_first
            && progress == 40
            && let Some((ms, penalty)) = lap
        {
            engine.lap_report(ms, penalty, time);
        }
        engine.update(&car(progress as f64), time);
        if !reports_first
            && progress == 10
            && let Some((ms, penalty)) = split
        {
            engine.split_report(ms, penalty, time);
        }
        if !reports_first
            && progress == 40
            && let Some((ms, penalty)) = lap
        {
            engine.lap_report(ms, penalty, time);
        }
    }
}

#[test]
fn sector_one_reference_starts_lap_two_delta_and_projects_the_whole_lap() {
    for reports_first in [true, false] {
        for node_ms in [90_u64, 110] {
            let mut engine = race_setup();
            // Opening lap contains two seconds before S1 and three after it.
            opening_lap(
                &mut engine,
                reports_first,
                None,
                Some((2000, 0)),
                Some((5000, 0)),
            );
            let reference = engine.frame(3500, 250);
            assert_eq!(reference.sector_reference_seconds, Some(3.0));
            assert_eq!(reference.best_seconds, None);
            for progress in 41..=50 {
                let time = 3500 + (progress - 40) * 80;
                if reports_first && progress == 50 {
                    engine.split_report(800, 0, time);
                }
                engine.update(&car(progress as f64), time);
                if progress < 50 {
                    let waiting = engine.frame(time, 250);
                    assert!(waiting.seconds.is_none());
                    assert!(waiting.estimated_lap_seconds.is_none());
                }
                if reports_first && progress == 50 {
                    let start = engine.frame(time, 250);
                    assert!(start.seconds.unwrap().abs() < 1e-8);
                    assert!((start.estimated_lap_seconds.unwrap() - 3.8).abs() < 1e-8);
                }
                if !reports_first && progress == 50 {
                    engine.split_report(800, 0, time);
                }
            }
            let now = 4300 + node_ms;
            engine.update(&car(51.0), now);
            let delta = engine.frame(now, 250);
            let expected = (node_ms as f64 - 100.0) / 1000.0;
            assert!(delta.since_sector1);
            assert_eq!(delta.status, "Delta since sector 1");
            assert!((delta.seconds.unwrap() - expected).abs() < 1e-8);
            // Includes lap TWO's faster first sector, not the opening lap's.
            assert!((delta.estimated_lap_seconds.unwrap() - (3.8 + expected)).abs() < 1e-8);
            let stale = engine.frame(now + 251, 250);
            assert!(stale.estimated_lap_seconds.is_none());
            assert_eq!(stale.sector_reference_seconds, Some(3.0));
            for progress in 52..=80 {
                engine.update(&car(progress as f64), now + (progress - 51) * 100);
            }
            let finish = now + 2900;
            engine.lap_report((finish - 3500) as u32, 0, finish);
            engine.update(&car(81.0), finish + 100);
            let full = engine.frame(finish + 100, 250);
            assert!(!full.since_sector1);
            assert!(full.sector_reference_seconds.is_none());
            assert!((full.best_seconds.unwrap() - (3.8 + expected)).abs() < 1e-8);
            // Lap 3 takes 100 ms to node 1; the full reference took 80 ms.
            assert!((full.seconds.unwrap() - 0.02).abs() < 1e-8);
            assert!(
                (full.estimated_lap_seconds.unwrap() - full.best_seconds.unwrap() - 0.02).abs()
                    < 1e-8
            );
        }
    }
}

#[test]
fn partial_reference_requires_clean_continuous_progress_and_official_timing() {
    for invalid_at in [None, Some(7), Some(20)] {
        for (split, lap) in [
            (Some((2000, 0)), Some((5000, 0))),
            (None, Some((5000, 0))),
            (Some((2000, 1)), Some((5000, 0))),
            (Some((2000, 0)), Some((5000, 1))),
            (Some((6000, 0)), Some((5000, 0))),
            (Some((2000, 0)), Some((8000, 0))),
            (Some((2000, 0)), None),
        ] {
            let mut engine = race_setup();
            opening_lap(&mut engine, false, invalid_at, split, lap);
            assert_eq!(
                engine.frame(3500, 250).sector_reference_seconds.is_some(),
                invalid_at.is_none() && split == Some((2000, 0)) && lap == Some((5000, 0))
            );
        }
    }
    let mut engine = race_setup();
    for progress in 5..=40 {
        let time = (progress - 5) * 100;
        engine.update(&car(progress as f64), time);
        if progress == 10 {
            engine.split_report(2000, 0, time);
        }
        if progress == 20 {
            engine.reset_lap();
        }
    }
    engine.lap_report(5000, 0, 3500);
    assert!(engine.frame(3500, 250).sector_reference_seconds.is_none());
}

#[test]
fn partial_reference_survives_interruption_but_session_reset_clears_it() {
    let mut engine = race_setup();
    opening_lap(&mut engine, false, None, Some((2000, 0)), Some((5000, 0)));
    engine.reset_lap();
    let reset = engine.frame(3500, 250);
    assert_eq!(reset.sector_reference_seconds, Some(3.0));
    assert!(reset.seconds.is_none());
    assert!(reset.estimated_lap_seconds.is_none());
    engine.reset_reference();
    assert!(engine.frame(3500, 250).sector_reference_seconds.is_none());
}

#[test]
fn opening_lap_with_a_different_start_origin_can_still_supply_post_split_reference() {
    let mut engine = race_setup();
    for progress in 39..=80 {
        let time = (progress - 39) * 100;
        engine.update(&car(progress as f64), time);
        if progress == 50 {
            engine.split_report(3000, 0, time);
        }
    }
    // The recorded finish-to-finish span was 4s, but LFS's opening lap was 6s.
    // Both agree that the portion after S1 lasted 3s.
    engine.lap_report(6000, 0, 4100);
    let delta = engine.frame(4100, 250);
    assert_eq!(delta.best_seconds, None);
    assert_eq!(delta.sector_reference_seconds, Some(3.0));
}

#[test]
fn absent_invalid_or_non_race_split_metadata_keeps_finish_line_recording() {
    for info in [
        track(),
        TrackInfo {
            timing: 0x41,
            race_laps: 5,
            split1: 99,
            ..track()
        },
        TrackInfo {
            timing: 0x41,
            race_laps: 5,
            split1: 7,
            ..track()
        },
        TrackInfo {
            timing: 0x41,
            split1: 17,
            ..track()
        },
    ] {
        let mut engine = setup();
        engine.set_track(info);
        opening_lap(&mut engine, false, None, Some((2000, 0)), Some((5000, 0)));
        assert!(engine.frame(3500, 250).sector_reference_seconds.is_none());
    }
}
fn record(
    e: &mut DeltaEngine,
    start: u64,
    ms_per_node: u64,
    invalid: bool,
    report_first: bool,
) -> u64 {
    for p in 39..=80 {
        let time = start + (p - 39) * ms_per_node;
        if p == 60 && invalid {
            e.invalidate();
        }
        if p == 80 && report_first {
            e.lap_report((40 * ms_per_node) as u32, 0, time);
        }
        e.update(&car(p as f64), time);
        if p == 80 && !report_first {
            e.lap_report((40 * ms_per_node) as u32, 0, time);
        }
    }
    start + 41 * ms_per_node
}

#[test]
fn confirms_complete_reference_with_lap_packet_before_or_after_mci() {
    for first in [true, false] {
        let mut e = setup();
        let end = record(&mut e, 0, 100, false, first);
        e.update(&car(81.0), end + 100);
        assert_eq!(e.frame(end + 100, 250).best_seconds, Some(4.0));
        assert!(e.frame(end + 100, 250).seconds.unwrap().abs() < 1e-8);
    }
}

#[test]
fn finish_crossing_does_not_require_lap_counter_and_node_to_change_together() {
    // The nearest path node and the timed finish line are separate telemetry
    // fields. The lap counter can change just before or after the finish node.
    for counter_offset in [-1_i64, 1] {
        let mut engine = setup();
        for progress in 39..=81_i64 {
            let time = (progress - 39) as u64 * 100;
            let mut sample = car(progress as f64);
            sample.lap = ((progress + counter_offset) / 40) as u16;
            engine.update(&sample, time);
            if progress == 80 {
                engine.lap_report(4000, 0, time);
            }
        }
        let delta = engine.frame(4200, 250);
        assert_eq!(
            delta.best_seconds,
            Some(4.0),
            "counter offset {counter_offset}"
        );
        assert!(delta.seconds.unwrap().abs() < 1e-8);
    }
}

#[test]
fn lap_counter_jumps_away_from_finish_still_interrupt_recording() {
    for counter_change in [-1_i32, 1, 2] {
        let mut engine = setup();
        let end = record(&mut engine, 0, 100, false, false);
        for progress in 81..=85 {
            engine.update(&car(progress as f64), end + (progress - 80) * 100);
        }
        let mut jumped = car(86.0);
        jumped.lap = (i32::from(jumped.lap) + counter_change) as u16;
        engine.update(&jumped, end + 600);
        let delta = engine.frame(end + 600, 250);
        assert!(delta.seconds.is_none());
        assert_eq!(delta.best_seconds, Some(4.0));
    }
}
#[test]
fn live_delta_interpolates_within_node_and_reports_gain_and_loss() {
    let mut e = setup();
    let end = record(&mut e, 0, 100, false, false);
    // Half way along the reference segment, before the next node update.
    let mut half = car(80.0);
    let next = car(81.0);
    let mut early = half.clone();
    early.pose.x += (next.pose.x - early.pose.x) * 0.1;
    early.pose.y += (next.pose.y - early.pose.y) * 0.1;
    e.update(&early, end + 10);
    half.pose.x = (half.pose.x + next.pose.x) * 0.5;
    half.pose.y = (half.pose.y + next.pose.y) * 0.5;
    e.update(&half, end + 25);
    assert!((e.frame(end + 25, 250).seconds.unwrap() + 0.025).abs() < 1e-8);
    assert!(e.frame(end + 25, 250).trend.unwrap() < 0.0);
    e.update(&car(81.0), end + 150);
    assert!((e.frame(end + 150, 250).seconds.unwrap() - 0.05).abs() < 1e-8);
    assert!(e.frame(end + 150, 250).trend.unwrap() > 0.0);
}

#[test]
fn engine_publishes_delta_only_with_associated_outsim_and_resets_session() {
    use lfs_openradar::{
        lfs::{
            insim::{Player, State},
            outsim::Sample,
        },
        radar::Engine,
    };
    let config = Config::default();
    let mut engine = Engine::default();
    engine
        .packet(
            Packet::Player(Player {
                plid: 1,
                ucid: 1,
                kind: 0,
                name: "Driver".into(),
                model: "XRG".into(),
                in_garage: false,
            }),
            0,
        )
        .unwrap();
    engine
        .packet(
            Packet::State(State {
                flags: 1,
                camera: 3,
                viewed: 1,
                track: "BL1".into(),
            }),
            0,
        )
        .unwrap();
    engine
        .packet(
            Packet::RaceStart {
                info: track(),
                requested: false,
            },
            0,
        )
        .unwrap();
    for p in 38..=81 {
        let time = (p - 38) * 100;
        if p == 60 {
            // An opponent pitting must not discard our local reference recording.
            engine.packet(Packet::Pit(2), time).unwrap();
        }
        let car = car(p as f64);
        engine.packet(Packet::Mci(vec![car.clone()]), time).unwrap();
        engine.outsim(
            Sample {
                time_ms: time as u32,
                pose: car.pose,
            },
            time,
            &config,
        );
        if p == 80 {
            engine
                .packet(
                    Packet::Lap {
                        plid: 1,
                        time_ms: 4000,
                        penalty: 0,
                    },
                    time,
                )
                .unwrap();
        }
    }
    let radar = engine.frame(4300, &config);
    assert!(radar.live);
    let delta = engine.delta(4300, &config, &radar);
    assert_eq!(delta.best_seconds, Some(4.0));
    assert!(delta.seconds.unwrap().abs() < 1e-8);
    let stale = engine.frame(5000, &config);
    let stale_delta = engine.delta(5000, &config, &stale);
    assert!(stale_delta.seconds.is_none());
    assert_eq!(stale_delta.best_seconds, Some(4.0));
    engine
        .packet(
            Packet::State(State {
                flags: 5,
                camera: 3,
                viewed: 1,
                track: "BL1".into(),
            }),
            5000,
        )
        .unwrap();
    let paused = engine.frame(5000, &config);
    let paused_delta = engine.delta(5000, &config, &paused);
    assert!(paused_delta.seconds.is_none());
    assert_eq!(paused_delta.best_seconds, Some(4.0));
    engine.packet(Packet::Session, 5000).unwrap();
    let reset = engine.frame(5000, &config);
    assert!(engine.delta(5000, &config, &reset).best_seconds.is_none());
}

#[test]
fn race_pipeline_uses_only_the_local_first_split_and_hides_estimate_when_paused() {
    use lfs_openradar::{
        lfs::{
            insim::{Player, State},
            outsim::Sample,
        },
        radar::Engine,
    };
    let config = Config::default();
    let mut engine = Engine::default();
    engine
        .packet(
            Packet::Player(Player {
                plid: 1,
                ucid: 1,
                kind: 0,
                name: "Driver".into(),
                model: "XRG".into(),
                in_garage: false,
            }),
            0,
        )
        .unwrap();
    engine
        .packet(
            Packet::State(State {
                flags: 1,
                camera: 3,
                viewed: 1,
                track: "BL1".into(),
            }),
            0,
        )
        .unwrap();
    engine
        .packet(
            Packet::RaceStart {
                info: TrackInfo {
                    split1: 17,
                    timing: 0x41,
                    race_laps: 5,
                    ..track()
                },
                requested: false,
            },
            0,
        )
        .unwrap();
    for progress in 5..=51 {
        let time = (progress - 5) * 100;
        let sample = car(progress as f64);
        engine
            .packet(Packet::Mci(vec![sample.clone()]), time)
            .unwrap();
        engine.outsim(
            Sample {
                time_ms: time as u32,
                pose: sample.pose,
            },
            time,
            &config,
        );
        if progress == 10 || progress == 50 {
            engine
                .packet(
                    Packet::Split {
                        plid: 1,
                        time_ms: if progress == 10 { 2000 } else { 1000 },
                        split: 1,
                        penalty: 0,
                    },
                    time,
                )
                .unwrap();
        }
        if progress == 40 {
            engine
                .packet(
                    Packet::Lap {
                        plid: 1,
                        time_ms: 5000,
                        penalty: 0,
                    },
                    time,
                )
                .unwrap();
        }
        if progress == 49 {
            engine
                .packet(
                    Packet::Split {
                        plid: 2,
                        time_ms: 1,
                        split: 1,
                        penalty: 1,
                    },
                    time,
                )
                .unwrap();
            engine
                .packet(
                    Packet::Split {
                        plid: 1,
                        time_ms: 1,
                        split: 2,
                        penalty: 1,
                    },
                    time,
                )
                .unwrap();
        }
    }
    let radar = engine.frame(4600, &config);
    assert!(radar.live);
    let delta = engine.delta(4600, &config, &radar);
    assert_eq!(delta.sector_reference_seconds, Some(3.0));
    assert_eq!(delta.best_seconds, None);
    assert!(delta.seconds.unwrap().abs() < 1e-8);
    assert!((delta.estimated_lap_seconds.unwrap() - 4.0).abs() < 1e-8);
    engine
        .packet(
            Packet::State(State {
                flags: 5,
                camera: 3,
                viewed: 1,
                track: "BL1".into(),
            }),
            4600,
        )
        .unwrap();
    let paused = engine.frame(4600, &config);
    let delta = engine.delta(4600, &config, &paused);
    assert!(delta.seconds.is_none());
    assert!(delta.estimated_lap_seconds.is_none());
    assert_eq!(delta.sector_reference_seconds, Some(3.0));
    engine.packet(Packet::Session, 4600).unwrap();
    let reset = engine.frame(4600, &config);
    assert!(
        engine
            .delta(4600, &config, &reset)
            .sector_reference_seconds
            .is_none()
    );
}

#[test]
fn demo_completes_a_closed_reference_lap() {
    let config = Config::default();
    let snapshot = lfs_openradar::demo::Demo::default().snapshot(201_000, &config);
    assert_eq!(snapshot.delta.best_seconds, Some(100.0));
    assert!(snapshot.delta.seconds.unwrap().abs() < 0.001);
    assert!((snapshot.gaps.ahead.seconds.unwrap() - 5.0).abs() < 0.001);
}
#[test]
fn rejects_invalid_incomplete_penalized_and_unconfirmed_laps() {
    let mut e = setup();
    let end = record(&mut e, 0, 100, true, false);
    e.update(&car(81.0), end + 100);
    assert_eq!(e.frame(end + 100, 250).best_seconds, None);
    for mode in 0..3 {
        let mut e = setup();
        for p in 39..=80 {
            if mode == 0 && p == 60 {
                e.reset_lap();
            }
            e.update(&car(p as f64), (p - 39) * 100);
        }
        if mode != 2 {
            e.lap_report(4000, if mode == 1 { 1 } else { 0 }, 4100);
        }
        e.update(&car(81.0), 4200);
        assert_eq!(e.frame(4200, 250).best_seconds, None);
    }
}
#[test]
fn reference_is_frozen_until_a_faster_valid_completed_lap() {
    let mut e = setup();
    let end = record(&mut e, 0, 100, false, false);
    for p in 81..=120 {
        let time = end + (p - 80) * 80;
        e.update(&car(p as f64), time);
        assert_eq!(e.frame(time, 250).best_seconds, Some(4.0));
    }
    e.lap_report(3200, 0, end + 3200);
    e.update(&car(121.0), end + 3280);
    assert_eq!(e.frame(end + 3280, 250).best_seconds, Some(3.2));
}
#[test]
fn stale_backwards_reset_and_car_or_track_change_are_safe() {
    let mut e = setup();
    let end = record(&mut e, 0, 100, false, false);
    e.update(&car(81.0), end + 100);
    assert!(e.frame(end + 351, 250).seconds.is_none());
    e.update(&car(80.0), end + 200);
    assert!(e.frame(end + 200, 250).seconds.is_none());
    assert_eq!(e.frame(end + 200, 250).best_seconds, Some(4.0));
    e.reset_lap();
    assert_eq!(e.frame(end + 200, 250).best_seconds, Some(4.0));
    e.select_driver(None);
    e.select_driver(Some((1, "XRG".into())));
    e.update(&car(81.0), end + 300);
    assert_eq!(e.frame(end + 300, 250).best_seconds, Some(4.0));
    e.select_driver(Some((1, "XFG".into())));
    e.update(&car(81.0), end + 400);
    assert_eq!(e.frame(end + 400, 250).best_seconds, None);
    let mut info = track();
    info.track = "BL1R".into();
    e.set_track(info);
    assert!(e.frame(end + 400, 250).seconds.is_none());
}
#[test]
fn lap_and_validity_packets_validate_lengths_and_request_hlvc() {
    let mut lap = vec![0; 20];
    lap[0] = 5;
    lap[1] = 24;
    lap[3] = 1;
    lap[4..8].copy_from_slice(&45678_u32.to_le_bytes());
    lap[17] = 2;
    assert!(matches!(
        insim::decode(&lap).unwrap(),
        Packet::Lap {
            plid: 1,
            time_ms: 45678,
            penalty: 2
        }
    ));
    for size in [16, 20] {
        let mut hlv = vec![0; size];
        hlv[0] = (size / 4) as u8;
        hlv[1] = 52;
        hlv[3] = 1;
        assert!(matches!(
            insim::decode(&hlv).unwrap(),
            Packet::InvalidLap(1)
        ));
    }
    assert!(insim::decode(&[1, 24, 0, 1]).is_err());
    assert!(insim::decode(&[1, 52, 0, 1]).is_err());
    assert_eq!(insim::init(20, "").unwrap()[7] & 1, 1);
}
#[test]
fn toml_password_round_trips_is_redacted_and_missing_fields_keep_defaults() {
    let config: Config = toml::from_str("insim_password = 'secret123'\n[performance_delta]\nenabled = true\nwindow_x = -400.0\nscale = 1.5").unwrap();
    config.validate().unwrap();
    assert!(!format!("{config:?}").contains("secret123"));
    let saved = toml::to_string(&config).unwrap();
    let restored: Config = toml::from_str(&saved).unwrap();
    assert_eq!(restored.insim_password.0, "secret123");
    assert!(restored.performance_delta.enabled);
    assert_eq!(restored.performance_delta.window_x, Some(-400.0));
    let legacy: Config = toml::from_str("side_m = 7.0").unwrap();
    assert!(legacy.insim_password.0.is_empty());
    assert!(!legacy.performance_delta.enabled);
    let invalid: Config = toml::from_str("insim_password = 'long-invalid-password'").unwrap();
    assert!(invalid.validate().is_err());
}
