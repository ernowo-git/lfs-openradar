use lfs_openradar::{
    config::{CarProfile, Config},
    fuel::{FuelEngine, FuelFrame, race_laps},
    lfs::{
        insim::{self, Car, Packet, Player, State, TrackInfo},
        outgauge,
    },
    radar::{Engine, Pose},
};

fn track() -> TrackInfo {
    TrackInfo {
        track: "BL1".into(),
        nodes: 1000,
        finish: 100,
        split1: 400,
        timing: 0x40,
        race_laps: 30,
    }
}
fn car(now: u64) -> Car {
    Car {
        node: ((now % 1000 + 100) % 1000) as u16,
        lap: (now / 1000 + 1) as u16,
        plid: 1,
        position: 1,
        info: 64 | 128,
        pose: Pose::default(),
        speed_mps: 30.0,
        direction: 0.0,
    }
}
fn sample(now: u64, fuel: f32) -> outgauge::Sample {
    outgauge::Sample {
        time_ms: now as u32,
        plid: 1,
        car: "XFG".into(),
        fuel,
        available: outgauge::FUEL_WARNING,
        ..Default::default()
    }
}
fn tracker() -> FuelEngine {
    let mut engine = FuelEngine::default();
    engine.select_driver(Some((1, 1, "XFG".into())));
    engine.set_track(track());
    engine
}
fn lap(engine: &mut FuelEngine, number: u16, start: f32, end: f32, before_mci: bool) {
    let from = u64::from(number - 1) * 1000;
    for time in (from..=from + 1000).step_by(20) {
        let fuel = start + (end - start) * (time - from) as f32 / 1000.0;
        engine.receive(&sample(time, fuel), time, 250);
        if time == from + 1000 && before_mci {
            engine.lap_report(number, time, 250);
        }
        engine.update(&car(time), time, 250);
        if time == from + 1000 && !before_mci {
            engine.lap_report(number, time, 250);
        }
    }
}
fn close(actual: Option<f64>, expected: f64) {
    assert!(
        (actual.unwrap() - expected).abs() < 0.0001,
        "{actual:?} != {expected}"
    );
}

#[test]
fn example_and_row_definitions_use_consistent_unrounded_fuel() {
    let frame = lfs_openradar::demo::fuel_preview();
    close(frame.rows[0].laps, 12.9);
    close(frame.remaining_race_laps, 20.6);
    close(frame.margin, -7.7);
    assert!(frame.rows[1].laps < frame.rows[2].laps);
    close(frame.rows[0].refuel, 20.6 * (0.14 / 12.9) - 0.14);
    let mut empty = FuelFrame {
        fraction: Some(0.0),
        ..Default::default()
    };
    empty.estimates([0.01, 0.02, 0.005], Some(20.0));
    close(empty.rows[0].laps, 0.0);
    close(empty.margin, -20.0);
    empty.estimates([0.0; 3], None);
    assert!(empty.rows[0].laps.is_none() && empty.margin.is_none());
    assert!(FuelFrame::default().margin.is_none());
    let mut multi = FuelFrame {
        fraction: Some(0.1),
        ..Default::default()
    };
    multi.estimates([0.01; 3], Some(150.0));
    assert!(multi.rows[0].multiple_stops);
    close(multi.rows[0].refuel, 1.4);
}

#[test]
fn learns_only_full_laps_and_uses_the_latest_five_for_both_packet_orders() {
    let config = Config::default();
    for before in [false, true] {
        let mut engine = tracker();
        lap(&mut engine, 1, 0.9, 0.89, before);
        assert!(engine.frame(1000, &config, true).rows[0].usage.is_none());
        let mut fuel = 0.89;
        for (index, burn) in [0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.07]
            .into_iter()
            .enumerate()
        {
            lap(&mut engine, index as u16 + 2, fuel, fuel - burn, before);
            fuel -= burn;
        }
        let frame = engine.frame(8000, &config, true);
        close(frame.rows[0].usage, 0.05);
        close(frame.rows[1].usage, 0.07);
        close(frame.rows[2].usage, 0.03);
        close(frame.remaining_race_laps, 22.0);
        engine.lap_report(8, 8000, 250); // Duplicate cannot become another sample.
        close(engine.frame(8000, &config, true).rows[0].usage, 0.05);
        assert!(engine.frame(8300, &config, true).fraction.is_none());
        assert!(engine.frame(8000, &config, false).fraction.is_none());
    }
}

#[test]
fn asynchronous_finish_nodes_counters_and_reports_keep_full_lap_samples() {
    let config = Config::default();
    for one_based in [false, true] {
        for counter_early in [false, true] {
            for report_early in [false, true] {
                let mut engine = tracker();
                for now in (0..=30040).step_by(20) {
                    let mut position = car(now / 10);
                    let phase = now % 10000;
                    // The nearest finish node and LFS's timed lap counter can
                    // advance in separate MCI packets on either side of the line.
                    if counter_early && phase == 9980 {
                        position.lap += 1;
                    } else if !counter_early && now > 0 && phase == 0 {
                        position.lap -= 1;
                    }
                    if !one_based {
                        position.lap -= 1;
                    }
                    engine.receive(&sample(now, 0.5 - now as f32 * 0.000001), now, 250);
                    let report = if report_early && phase == 9980 {
                        Some((now / 10000 + 1) as u16)
                    } else if !report_early && now >= 10020 && phase == 20 {
                        Some((now / 10000) as u16)
                    } else {
                        None
                    };
                    engine.update(&position, now, 250);
                    if let Some(done) = report {
                        engine.lap_report(done, now, 250);
                    }
                }
                let frame = engine.frame(30040, &config, true);
                close(frame.rows[0].usage, 0.01);
                close(frame.remaining_race_laps, 26.996);
            }
        }
    }
}

#[test]
fn refuel_pit_and_stale_samples_keep_history_but_discard_partial_laps() {
    let config = Config::default();
    let mut engine = tracker();
    lap(&mut engine, 1, 0.51, 0.50, false);
    lap(&mut engine, 2, 0.50, 0.49, false);
    close(engine.frame(2000, &config, true).rows[0].usage, 0.01);
    let old_range = engine.frame(2000, &config, true).rows[0].laps.unwrap();
    engine.pit_lane(true);
    lap(&mut engine, 3, 0.8, 0.75, false);
    assert!(engine.frame(3000, &config, true).rows[0].laps.unwrap() > old_range);
    close(engine.frame(3000, &config, true).rows[0].usage, 0.01);
    engine.pit_lane(false);
    lap(&mut engine, 4, 0.75, 0.70, false); // First new boundary, not a complete sampled lap.
    close(engine.frame(4000, &config, true).rows[0].usage, 0.01);
    lap(&mut engine, 5, 0.70, 0.68, false);
    close(engine.frame(5000, &config, true).rows[0].usage, 0.015);
    // A refuel without a pit event also invalidates the incomplete lap.
    lap(&mut engine, 6, 0.9, 0.8, false);
    close(engine.frame(6000, &config, true).rows[0].usage, 0.015);
    engine.receive(&sample(6500, 0.79), 6500, 250); // Gap in UDP reception.
    lap(&mut engine, 7, 0.79, 0.74, false);
    close(engine.frame(7000, &config, true).rows[0].usage, 0.015);
}

#[test]
fn zero_burn_identity_changes_track_snapshots_and_finish_are_safe() {
    let config = Config::default();
    let mut engine = tracker();
    lap(&mut engine, 1, 0.5, 0.5, false);
    lap(&mut engine, 2, 0.5, 0.5, false);
    assert!(engine.frame(2000, &config, true).rows[0].laps.is_none());
    lap(&mut engine, 3, 0.5, 0.49, false);
    engine.set_track(track());
    close(engine.frame(3000, &config, true).rows[0].usage, 0.01);
    engine.finish();
    close(engine.frame(3000, &config, true).remaining_race_laps, 0.0);
    close(engine.frame(3000, &config, true).rows[0].refuel, 0.0);
    assert_eq!(engine.frame(3000, &config, true).status, "Race finished");
    engine.select_driver(Some((2, 1, "XFG".into())));
    assert!(engine.frame(3000, &config, true).fraction.is_none());
    engine.receive(&sample(3000, 0.49), 3000, 250);
    assert!(engine.frame(3000, &config, true).rows[0].usage.is_none());
    engine.clear();
    assert!(engine.frame(3000, &config, true).car.is_none());
    for encoded in [0, 191, 238, 255] {
        let mut engine = tracker();
        engine.set_track(TrackInfo {
            race_laps: encoded,
            ..track()
        });
        lap(&mut engine, 1, 0.5, 0.49, false);
        lap(&mut engine, 2, 0.49, 0.48, false);
        let frame = engine.frame(2000, &config, true);
        assert!(frame.rows[0].laps.is_some());
        assert!(frame.margin.is_none() && frame.rows[0].refuel.is_none());
    }
    let mut engine = tracker();
    engine.set_track(TrackInfo {
        timing: 0x80,
        ..track()
    });
    lap(&mut engine, 1, 0.5, 0.49, false);
    lap(&mut engine, 2, 0.49, 0.48, false);
    let frame = engine.frame(2000, &config, true);
    assert!(frame.fraction.is_some() && frame.rows[0].laps.is_none());
    assert!(frame.status.starts_with("Standard circuit"));
}

#[test]
fn race_length_encoding_and_retired_capacity_preserve_old_configuration() {
    for (encoded, laps) in [(1, 1), (99, 99), (100, 100), (101, 110), (190, 1000)] {
        assert_eq!(race_laps(encoded), Some(laps));
    }
    for encoded in [0, 191, 238, 239, 255] {
        assert_eq!(race_laps(encoded), None);
    }
    let mut config: Config = toml::from_str("[cars.XFG]\nname='XF GTI'\nmax_rpm=8000").unwrap();
    assert!(!config.fuel.enabled);
    assert_eq!(config.cars["XFG"].fuel_tank_litres, None);
    config.fuel.enabled = true;
    config.cars.insert(
        "A4D807".into(),
        CarProfile {
            name: "Mod".into(),
            fuel_tank_litres: Some(100.0),
            ..Default::default()
        },
    );
    config.validate().unwrap();
    config.cars.get_mut("XFG").unwrap().fuel_tank_litres = Some(100.0);
    let path = std::env::temp_dir().join(format!(
        "openradar-retired-capacity-{}.toml",
        std::process::id()
    ));
    config.save(&path).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("fuel_tank_litres"));
    let restored = Config::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(restored.fuel.enabled && restored.cars["XFG"].max_rpm == 8000);
    assert!(!restored.cars.contains_key("A4D807"));
    config.outgauge_bind = config.outsim_bind;
    config.outsim_options = 31; // Ambiguous 92-byte physics packet, even with dashboard off.
    assert!(config.validate().is_err());
    config.outgauge_bind = "127.0.0.1:30001".parse().unwrap();
    for value in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        config.cars.get_mut("XFG").unwrap().fuel_tank_litres = Some(value);
        assert!(config.validate().is_err());
    }
}

#[test]
fn fuel_and_lifecycle_packets_validate_their_wire_layouts() {
    let mut gauge = vec![0; 96];
    gauge[4..8].copy_from_slice(b"XFG\0");
    gauge[11] = 1;
    gauge[28..32].copy_from_slice(&0.14_f32.to_le_bytes());
    gauge[92..96].copy_from_slice(&24602_i32.to_le_bytes());
    assert_eq!(outgauge::decode(&gauge, 24602).unwrap().fuel, 0.14);
    for invalid in [-0.01, 1.01, f32::INFINITY, f32::NAN] {
        gauge[28..32].copy_from_slice(&invalid.to_le_bytes());
        assert!(outgauge::decode(&gauge, 24602).is_err());
    }
    for fact in 0..=4 {
        let mut packet = vec![0; 8];
        packet[..4].copy_from_slice(&[2, 28, 0, 1]);
        packet[4] = fact;
        assert!(
            matches!(insim::decode(&packet).unwrap(), Packet::PitLane { plid: 1, entered } if entered == (fact != 0))
        );
    }
    assert!(matches!(
        insim::decode(&[
            6, 26, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ])
        .unwrap(),
        Packet::PitStop(1)
    ));
    assert!(insim::decode(&[1, 34, 0, 1]).is_err());
}

#[test]
fn engine_fuel_works_without_outsim_and_preserves_only_the_same_drivers_history() {
    let config = Config::default();
    let player = Player {
        plid: 1,
        ucid: 1,
        kind: 0,
        name: "You".into(),
        model: "XFG".into(),
        in_garage: false,
        abs_enabled: true,
    };
    let state = State {
        flags: 1,
        camera: 3,
        viewed: 1,
        track: "BL1".into(),
    };
    let mut engine = Engine::default();
    engine.packet(Packet::Player(player.clone()), 0).unwrap();
    engine.packet(Packet::State(state.clone()), 0).unwrap();
    engine
        .packet(
            Packet::RaceStart {
                info: track(),
                requested: false,
            },
            0,
        )
        .unwrap();
    for now in (0..=3000).step_by(20) {
        engine.fuel_sample(&sample(now, 0.5 - now as f32 * 0.00001), now, &config);
        engine.packet(Packet::Mci(vec![car(now)]), now).unwrap();
        if now > 0 && now % 1000 == 0 {
            engine
                .packet(
                    Packet::Lap {
                        plid: 1,
                        time_ms: 1000,
                        penalty: 0,
                        laps_done: (now / 1000) as u16,
                    },
                    now,
                )
                .unwrap();
        }
    }
    close(engine.fuel(3000, &config).rows[0].usage, 0.01);
    assert!(!engine.frame(3000, &config).live);
    for packet in [
        Packet::PlayerSnapshot(player.clone()),
        Packet::RaceStart {
            info: track(),
            requested: true,
        },
        Packet::PitStop(2),
        Packet::Reset(2),
        Packet::InvalidLap(1),
    ] {
        engine.packet(packet, 3000).unwrap();
        close(engine.fuel(3000, &config).rows[0].usage, 0.01);
    }
    engine
        .packet(
            Packet::State(State {
                flags: 5,
                ..state.clone()
            }),
            3000,
        )
        .unwrap();
    assert!(engine.fuel(3000, &config).fraction.is_none());
    engine.packet(Packet::State(state), 3000).unwrap();
    engine.fuel_sample(&sample(3020, 0.47), 3020, &config);
    close(engine.fuel(3020, &config).rows[0].usage, 0.01);
    engine.packet(Packet::Player(player), 3020).unwrap();
    engine.fuel_sample(&sample(3040, 0.47), 3040, &config);
    assert!(engine.fuel(3040, &config).rows[0].usage.is_none());
}
