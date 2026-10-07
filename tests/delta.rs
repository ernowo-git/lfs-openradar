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
    e.select_driver(Some((1, 1, "XRG".into())));
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
#[test]
fn race_records_lap_two_from_the_finish_and_compares_from_lap_three() {
    for reports_first in [true, false] {
        let mut engine = race_setup();
        // Join during lap 1: sector 1 cannot create a partial reference.
        for progress in 5..=81 {
            let time = (progress - 5) * 100;
            let report = match progress {
                40 => Some(5000), // Opening lap has a different timing origin.
                80 => Some(4000),
                _ => None,
            };
            if reports_first && let Some(ms) = report {
                engine.lap_report(ms, 0, time);
            }
            engine.update(&car(progress as f64), time);
            if progress < 40 {
                assert_eq!(
                    engine.frame(time, 250).status,
                    "Cross the finish line to start recording"
                );
            }
            if !reports_first && let Some(ms) = report {
                engine.lap_report(ms, 0, time);
            }
            let frame = engine.frame(time, 250);
            if progress < 80 {
                assert!(frame.best_seconds.is_none());
                assert!(frame.seconds.is_none());
                assert!(frame.estimated_lap_seconds.is_none());
            }
            if progress == 40 || progress == 50 {
                assert_eq!(
                    frame.status,
                    "Complete a valid recorded lap to set a reference"
                );
            }
        }
        let frame = engine.frame(7600, 250);
        assert_eq!(frame.best_seconds, Some(4.0));
        assert!(frame.seconds.unwrap().abs() < 1e-8);
        assert!((frame.estimated_lap_seconds.unwrap() - 4.0).abs() < 1e-8);
        assert_eq!(frame.status, "Estimated vs session best");
        let stale = engine.frame(7851, 250);
        assert!(stale.seconds.is_none());
        assert!(stale.estimated_lap_seconds.is_none());
        assert_eq!(stale.best_seconds, Some(4.0));
    }
}

#[test]
fn opening_lap_with_a_different_start_origin_cannot_supply_a_reference() {
    let mut engine = race_setup();
    for progress in 39..=80 {
        engine.update(&car(progress as f64), (progress - 39) * 100);
    }
    // A finish-to-finish trace does not qualify if official timing disagrees.
    engine.lap_report(6000, 0, 4100);
    assert!(engine.frame(4100, 250).best_seconds.is_none());
    for progress in 81..=120 {
        engine.update(&car(progress as f64), (progress - 39) * 100);
    }
    engine.lap_report(4000, 0, 8100);
    assert_eq!(engine.frame(8100, 250).best_seconds, Some(4.0));
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
fn coalesced_mci_updates_keep_recording_and_live_comparison() {
    for report_first in [true, false] {
        let mut engine = race_setup();
        // The runtime timestamps every packet in a TCP read together. Include
        // advancing and stationary updates in each batch, also at the finish.
        for progress in 39..=81 {
            let time = ((progress - 39) / 2) * 200;
            if progress == 80 && report_first {
                engine.lap_report(4000, 0, time);
            }
            let sample = car(progress as f64);
            engine.update(&sample, time);
            engine.update(&sample, time);
            if progress == 80 && !report_first {
                engine.lap_report(4000, 0, time);
            }
        }
        let frame = engine.frame(4200, 250);
        assert_eq!(frame.best_seconds, Some(4.0));
        assert_eq!(frame.status, "Estimated vs session best");
        assert!(frame.seconds.is_some());
        assert!(frame.estimated_lap_seconds.is_some());
        // Actual clock reversal must still discard the current recording.
        engine.update(&car(82.0), 4199);
        let backwards = engine.frame(4200, 250);
        assert!(backwards.seconds.is_none());
        assert_eq!(backwards.best_seconds, Some(4.0));
    }
}

#[test]
fn short_telemetry_interruptions_preserve_the_trace_but_long_gaps_discard_it() {
    for interrupted_nodes in [1, 6] {
        let mut engine = race_setup();
        for progress in 39..=80 {
            let time = (progress - 39) * 100;
            let mut sample = car(progress as f64);
            if (60..60 + interrupted_nodes).contains(&progress) {
                sample.info |= 32;
                engine.update(&sample, time);
                let suspended = engine.frame(time, 250);
                assert!(suspended.seconds.is_none());
                assert!(suspended.estimated_lap_seconds.is_none());
                continue;
            }
            engine.update(&sample, time);
        }
        engine.lap_report(4000, 0, 4100);
        assert_eq!(
            engine.frame(4100, 250).best_seconds,
            (interrupted_nodes == 1).then_some(4.0)
        );
    }
}

fn timing_pipeline() -> lfs_openradar::radar::Engine {
    use lfs_openradar::lfs::insim::{Player, State};
    let mut engine = lfs_openradar::radar::Engine::default();
    for id in 1..=4 {
        engine
            .packet(
                Packet::Player(Player {
                    plid: id,
                    ucid: id,
                    kind: if id == 1 { 0 } else { 6 },
                    name: format!("Driver {id}"),
                    model: "XRG".into(),
                    in_garage: false,
                }),
                0,
            )
            .unwrap();
    }
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
                    race_laps: 5,
                    ..track()
                },
                requested: false,
            },
            0,
        )
        .unwrap();
    engine
}

fn timing_cars(progress: u64) -> Vec<Car> {
    [
        (1, 2, progress, 64),
        (2, 1, progress + 2, 0),
        (3, 3, progress - 3, 128),
    ]
    .into_iter()
    .map(|(id, position, progress, info)| {
        let mut sample = car(progress as f64);
        sample.plid = id;
        sample.position = position;
        sample.info = info;
        sample
    })
    .collect()
}

#[test]
fn opponent_events_and_roster_replies_preserve_local_delta_and_neighbor_gaps() {
    use lfs_openradar::lfs::{insim::Player, outsim::Sample};
    let config = Config {
        interpolation_ms: 0,
        ..Default::default()
    };
    for event in [
        Packet::Pit(4),
        Packet::Reset(4),
        Packet::Leave(4),
        Packet::ConnectionLeft(4),
        Packet::Camera(4),
        Packet::Takeover(4),
        Packet::PlayerSnapshot(Player {
            plid: 1,
            ucid: 1,
            kind: 0,
            name: "Driver 1".into(),
            model: "XRG".into(),
            in_garage: false,
        }),
    ] {
        let mut engine = timing_pipeline();
        for progress in 38..=90 {
            let time = (progress - 38) * 100;
            if progress == 60 || progress == 84 {
                engine.packet(event.clone(), time).unwrap();
            }
            let cars = timing_cars(progress);
            let pose = cars[0].pose;
            engine.packet(Packet::Mci(cars), time).unwrap();
            engine.outsim(
                Sample {
                    time_ms: time as u32,
                    pose,
                },
                time,
                &config,
            );
            if progress == 80 {
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
            if progress == 61 || progress == 85 {
                let radar = engine.frame(time, &config);
                assert!(radar.live, "{event:?}: {}", radar.status);
                let gaps = engine.gaps(time, &config, &radar);
                assert!(
                    gaps.ahead.seconds.is_some(),
                    "{event:?}: {}",
                    gaps.ahead.status
                );
                assert!(
                    gaps.behind.seconds.is_some(),
                    "{event:?}: {}",
                    gaps.behind.status
                );
            }
        }
        let radar = engine.frame(5200, &config);
        let delta = engine.delta(5200, &config, &radar);
        assert_eq!(delta.best_seconds, Some(4.0), "{event:?}");
        assert!(delta.seconds.unwrap().abs() < 1e-8, "{event:?}");
    }
}

#[test]
fn transient_pose_mismatch_or_missing_local_mci_recovers_without_restarting_the_lap() {
    use lfs_openradar::lfs::outsim::Sample;
    let config = Config {
        interpolation_ms: 0,
        ..Default::default()
    };
    for missing_car in [false, true] {
        let mut engine = timing_pipeline();
        for progress in 38..=90 {
            let time = (progress - 38) * 100;
            let mut cars = timing_cars(progress);
            let mut pose = cars[0].pose;
            if progress == 60 || progress == 84 {
                if missing_car {
                    cars.remove(0);
                    cars[0].info |= 64;
                } else {
                    pose.x += 1000.0;
                }
            }
            engine.packet(Packet::Mci(cars), time).unwrap();
            engine.outsim(
                Sample {
                    time_ms: time as u32,
                    pose,
                },
                time,
                &config,
            );
            if progress == 60 || progress == 84 {
                let radar = engine.frame(time, &config);
                let delta = engine.delta(time, &config, &radar);
                assert!(delta.seconds.is_none());
                assert!(delta.estimated_lap_seconds.is_none());
            }
            if progress == 80 {
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
        let radar = engine.frame(5200, &config);
        let delta = engine.delta(5200, &config, &radar);
        assert_eq!(delta.best_seconds, Some(4.0));
        assert!(delta.seconds.unwrap().abs() < 1e-8);
        let gaps = engine.gaps(5200, &config, &radar);
        assert!(gaps.ahead.seconds.is_some());
        assert!(gaps.behind.seconds.is_some());
    }
}

#[test]
fn local_resets_and_teleports_discard_recording_but_opponent_teleports_do_not() {
    use lfs_openradar::lfs::outsim::Sample;
    let config = Config {
        interpolation_ms: 0,
        ..Default::default()
    };
    for mode in 0..3 {
        let mut engine = timing_pipeline();
        for progress in 38..=81 {
            let time = (progress - 38) * 100;
            let mut cars = timing_cars(progress);
            cars[2].info = 0;
            let mut other = car((progress - 8) as f64);
            other.plid = 4;
            other.position = 4;
            other.info = 128;
            cars.push(other);
            if progress == 60 {
                match mode {
                    0 => engine.packet(Packet::Reset(1), time).unwrap(),
                    1 => cars[0].pose.x += 1000.0,
                    _ => cars[3].pose.x += 1000.0,
                }
            }
            let pose = cars[0].pose;
            engine.packet(Packet::Mci(cars), time).unwrap();
            engine.outsim(
                Sample {
                    time_ms: time as u32,
                    pose,
                },
                time,
                &config,
            );
            if progress == 80 {
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
        let delta = engine.delta(4300, &config, &radar);
        assert_eq!(delta.best_seconds, (mode == 2).then_some(4.0));
        if mode == 2 {
            let gaps = engine.gaps(4300, &config, &radar);
            assert!(gaps.ahead.seconds.is_some());
            assert!(gaps.behind.seconds.is_some());
        }
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
fn race_pipeline_ignores_split_reports_and_hides_estimate_when_paused() {
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
    for progress in 5..=81 {
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
        if progress == 40 || progress == 80 {
            engine
                .packet(
                    Packet::Lap {
                        plid: 1,
                        time_ms: if progress == 40 { 5000 } else { 4000 },
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
        if progress < 80 {
            let radar = engine.frame(time, &config);
            let delta = engine.delta(time, &config, &radar);
            assert!(delta.best_seconds.is_none());
            assert!(delta.seconds.is_none());
            assert!(delta.estimated_lap_seconds.is_none());
        }
    }
    let radar = engine.frame(7600, &config);
    assert!(radar.live);
    let delta = engine.delta(7600, &config, &radar);
    assert_eq!(delta.best_seconds, Some(4.0));
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
            7600,
        )
        .unwrap();
    let paused = engine.frame(7600, &config);
    let delta = engine.delta(7600, &config, &paused);
    assert!(delta.seconds.is_none());
    assert!(delta.estimated_lap_seconds.is_none());
    assert_eq!(delta.best_seconds, Some(4.0));
    engine.packet(Packet::Session, 7600).unwrap();
    let reset = engine.frame(7600, &config);
    assert!(engine.delta(7600, &config, &reset).best_seconds.is_none());
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
    e.select_driver(Some((1, 1, "XRG".into())));
    e.update(&car(81.0), end + 300);
    assert_eq!(e.frame(end + 300, 250).best_seconds, Some(4.0));
    e.select_driver(Some((1, 1, "XFG".into())));
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
