mod support;

use lfs_openradar::{
    config::Config,
    lfs::{
        insim::{self, Car, Packet, Player, State, TrackInfo},
        outsim::Sample,
    },
    radar::{Engine, Pose},
};

fn driver(id: u8) -> Player {
    Player {
        plid: id,
        ucid: 0,
        kind: 2,
        name: format!("AI {id}"),
        model: "XRG".into(),
        abs_enabled: false,
        in_garage: false,
    }
}
fn state(viewed: u8, flags: u16, camera: u8) -> Packet {
    Packet::State(State {
        flags,
        camera,
        viewed,
        track: "BL1".into(),
    })
}
fn setup(follow: bool) -> Engine {
    let mut e = Engine::new(follow);
    for id in 1..=3 {
        e.packet(Packet::Player(driver(id)), 0).unwrap();
    }
    e.packet(state(2, 1, 3), 0).unwrap();
    e.packet(
        Packet::RaceStart {
            info: TrackInfo {
                track: "BL1".into(),
                nodes: 40,
                finish: 7,
                split1: 0,
                timing: 0x40,
                race_laps: 10,
            },
            requested: false,
        },
        0,
    )
    .unwrap();
    e
}
fn car(id: u8, progress: u64) -> Car {
    let angle = progress as f64 * std::f64::consts::TAU / 40.0;
    Car {
        node: ((progress + 7) % 40) as u16,
        lap: (progress / 40) as u16,
        position: id,
        plid: id,
        info: match id {
            1 => 64,
            3 => 128,
            _ => 0,
        },
        pose: Pose {
            x: 20.0 * angle.cos(),
            y: 20.0 * angle.sin(),
            ..Default::default()
        },
        speed_mps: 30.0,
        direction: 0.0,
    }
}
fn tick(e: &mut Engine, viewed: u8, p: u64, time: u64) {
    let cars = vec![car(1, p + 2), car(2, p), car(3, p - 2)];
    let pose = cars.iter().find(|c| c.plid == viewed).unwrap().pose;
    e.packet(Packet::Mci(cars), time).unwrap();
    e.outsim(
        Sample {
            time_ms: time as u32,
            pose,
        },
        time,
        &Config::default(),
    );
}
fn record(e: &mut Engine, event: Option<Packet>) {
    for p in 38..=82 {
        let time = (p - 38) * 100;
        if p == 60
            && let Some(event) = &event
        {
            e.packet(event.clone(), time).unwrap();
        }
        tick(e, 2, p, time);
        if p == 80 {
            e.packet(
                Packet::Lap {
                    plid: 2,
                    time_ms: 4000,
                    penalty: 0,
                },
                time,
            )
            .unwrap();
        }
    }
}
fn best(e: &mut Engine, now: u64) -> Option<f64> {
    let config = Config::default();
    let radar = e.frame(now, &config);
    e.delta(now, &config, &radar).best_seconds
}

#[test]
fn ai_only_race_uses_the_viewed_ai_for_all_three_gadgets() {
    let config = Config {
        interpolation_ms: 0,
        ..Default::default()
    };
    let mut e = setup(true);
    record(&mut e, None);
    let frame = e.frame(4400, &config);
    assert!(frame.live, "{}", frame.status);
    assert_eq!(frame.driver.as_deref(), Some("AI 2"));
    assert!(frame.status.starts_with("Following AI 2"));
    let ahead = frame.cars.iter().find(|c| c.plid == 1).unwrap();
    assert!((ahead.right - (car(1, 84).pose.x - car(2, 82).pose.x)).abs() < 0.01);
    let gaps = e.gaps(4400, &config, &frame);
    assert_eq!(gaps.ahead.driver.as_deref(), Some("AI 1"));
    assert_eq!(gaps.behind.driver.as_deref(), Some("AI 3"));
    assert!((gaps.ahead.seconds.unwrap() - 0.2).abs() < 0.001);
    assert!((gaps.behind.seconds.unwrap() - 0.2).abs() < 0.001);
    let delta = e.delta(4400, &config, &frame);
    assert_eq!(delta.best_seconds, Some(4.0));
    assert!(delta.seconds.unwrap().abs() < 0.001);
    let mut default = setup(false);
    record(&mut default, None);
    assert!(!default.frame(4400, &config).live);
}

#[test]
fn identical_ai_cars_never_share_references_even_when_switching_while_paused() {
    for flags in [1, 1 | insim::ISS_PAUSED, 1 | insim::ISS_SHIFTU] {
        let mut e = setup(true);
        record(&mut e, None);
        assert_eq!(best(&mut e, 4400), Some(4.0));
        e.packet(state(2, flags, 3), 4500).unwrap();
        e.packet(state(1, flags, 3), 4500).unwrap();
        assert!(best(&mut e, 4500).is_none());
        e.packet(state(2, 1, 3), 4600).unwrap();
        tick(&mut e, 2, 84, 4600);
        tick(&mut e, 2, 85, 4700);
        assert!(best(&mut e, 4700).is_none());
    }
}

#[test]
fn temporary_view_interruptions_preserve_only_the_same_cars_clean_reference() {
    for (flags, camera) in [
        (1 | insim::ISS_PAUSED, 3),
        (1 | insim::ISS_SHIFTU, 3),
        (1, 0),
    ] {
        let mut e = setup(true);
        record(&mut e, None);
        e.packet(state(2, flags, camera), 4500).unwrap();
        assert!(!e.frame(4500, &Config::default()).live);
        e.packet(state(2, 1, 4), 4600).unwrap();
        tick(&mut e, 2, 84, 4600);
        tick(&mut e, 2, 85, 4700);
        assert_eq!(best(&mut e, 4700), Some(4.0));
        let frame = e.frame(4700, &Config::default());
        assert!(e.delta(4700, &Config::default(), &frame).seconds.is_none());
    }
}

#[test]
fn another_viewed_ai_builds_its_own_reference_after_switching() {
    let mut e = setup(true);
    record(&mut e, None);
    e.packet(state(1, 1, 3), 4500).unwrap();
    // AI 1 crosses the finish at p=78 and p=118; use a different lap time.
    for p in 76..=120 {
        let time = 5000 + (p - 76) * 125;
        tick(&mut e, 1, p, time);
        if p == 118 {
            e.packet(
                Packet::Lap {
                    plid: 1,
                    time_ms: 5000,
                    penalty: 0,
                },
                time,
            )
            .unwrap();
        }
    }
    assert_eq!(best(&mut e, 10500), Some(5.0));
    e.packet(state(2, 1, 3), 10600).unwrap();
    tick(&mut e, 2, 121, 10600);
    tick(&mut e, 2, 122, 10700);
    assert!(e.frame(10700, &Config::default()).live);
    assert!(best(&mut e, 10700).is_none());
}

#[test]
fn default_own_car_mode_keeps_reference_when_returning_from_spectating() {
    let mut e = setup(false);
    let mut human = driver(2);
    human.kind = 0;
    e.packet(Packet::Player(human), 0).unwrap();
    record(&mut e, None);
    assert_eq!(best(&mut e, 4400), Some(4.0));
    e.packet(state(1, 1, 3), 4500).unwrap();
    assert!(!e.frame(4500, &Config::default()).live);
    e.packet(state(2, 1, 3), 4600).unwrap();
    tick(&mut e, 2, 84, 4600);
    tick(&mut e, 2, 85, 4700);
    assert_eq!(best(&mut e, 4700), Some(4.0));
}

#[test]
fn opponent_lifecycle_and_roster_refresh_do_not_break_reference_recording() {
    for event in [
        Packet::Pit(1),
        Packet::Leave(1),
        Packet::Reset(1),
        Packet::Takeover(1),
        Packet::Player(driver(1)),
        Packet::PlayerSnapshot(driver(2)),
        Packet::Lap {
            plid: 1,
            time_ms: 1,
            penalty: 1,
        },
        Packet::InvalidLap(1),
    ] {
        let mut e = setup(true);
        record(&mut e, Some(event));
        assert_eq!(best(&mut e, 4400), Some(4.0));
    }
}

#[test]
fn target_replacement_session_and_disconnect_discard_references() {
    let mut changed = driver(2);
    changed.model = "XFG".into();
    for event in [
        Packet::Pit(2),
        Packet::Leave(2),
        Packet::Takeover(2),
        Packet::Player(driver(2)),
        Packet::PlayerSnapshot(changed),
        Packet::ConnectionLeft(0),
        Packet::Session,
        Packet::LayoutChanged,
        Packet::Tiny(10),
        state(2, 1 | insim::ISS_MULTI, 3),
    ] {
        let mut e = setup(true);
        record(&mut e, None);
        e.packet(event.clone(), 4500).unwrap();
        assert!(best(&mut e, 4500).is_none());
        if matches!(
            event,
            Packet::Pit(_)
                | Packet::Leave(_)
                | Packet::Takeover(_)
                | Packet::ConnectionLeft(_)
                | Packet::Tiny(_)
        ) {
            for id in 1..=3 {
                e.packet(Packet::PlayerSnapshot(driver(id)), 4600).unwrap();
            }
        }
        // Restore only state/track metadata, then prove the reference is still
        // absent when telemetry becomes live again (not merely hidden).
        e.packet(state(2, 1, 3), 4600).unwrap();
        e.packet(
            Packet::RaceStart {
                info: TrackInfo {
                    track: "BL1".into(),
                    nodes: 40,
                    finish: 7,
                    split1: 0,
                    timing: 0x40,
                    race_laps: 10,
                },
                requested: true,
            },
            4600,
        )
        .unwrap();
        tick(&mut e, 2, 84, 4600);
        tick(&mut e, 2, 85, 4700);
        assert!(e.frame(4700, &Config::default()).live);
        assert!(best(&mut e, 4700).is_none(), "{event:?}");
    }
    let mut e = setup(true);
    record(&mut e, None);
    e.packet(Packet::Reset(2), 4500).unwrap();
    tick(&mut e, 2, 84, 4600);
    tick(&mut e, 2, 85, 4700);
    assert_eq!(best(&mut e, 4700), Some(4.0));
    e.clear();
    for id in 1..=3 {
        e.packet(Packet::Player(driver(id)), 4600).unwrap();
    }
    e.packet(state(2, 1, 3), 4600).unwrap();
    tick(&mut e, 2, 84, 4600);
    tick(&mut e, 2, 85, 4700);
    assert!(e.frame(4700, &Config::default()).live);
    assert!(best(&mut e, 4700).is_none());
}

#[test]
fn invalid_views_missing_car_wrong_pose_and_stale_data_withhold_output() {
    let config = Config::default();
    for (viewed, flags, camera) in [
        (0, 1, 3),
        (9, 1, 3),
        (2, 1 | insim::ISS_REPLAY, 3),
        (2, 1 | insim::ISS_FRONT_END, 3),
        (2, 0, 3),
        (2, 1 | insim::ISS_MULTI, 3),
    ] {
        let mut e = setup(true);
        e.packet(state(viewed, flags, camera), 0).unwrap();
        tick(&mut e, 2, 38, 0);
        tick(&mut e, 2, 39, 100);
        assert!(!e.frame(100, &config).live);
    }
    let mut e = setup(true);
    tick(&mut e, 2, 38, 0);
    tick(&mut e, 2, 39, 100);
    e.outsim(
        Sample {
            time_ms: 200,
            pose: Pose {
                x: 100.0,
                ..Default::default()
            },
        },
        200,
        &config,
    );
    assert!(!e.frame(200, &config).live);
    assert_eq!(e.rejected_outsim, 1);
    tick(&mut e, 2, 41, 300);
    assert!(!e.frame(1000, &config).live);
}

#[test]
fn multiplayer_keeps_own_human_rules_and_single_player_allows_human_targets() {
    let config = Config::default();
    let mut e = setup(true);
    let mut human = driver(2);
    human.kind = 0;
    e.packet(Packet::Player(human), 0).unwrap();
    for flags in [1, 1 | insim::ISS_MULTI] {
        e.packet(state(2, flags, 3), 0).unwrap();
        tick(&mut e, 2, 38, 100);
        tick(&mut e, 2, 39, 200);
        assert!(e.frame(200, &config).live);
    }
    e.packet(state(1, 1 | insim::ISS_MULTI, 3), 300).unwrap();
    tick(&mut e, 1, 40, 300);
    assert!(!e.frame(300, &config).live);
}

#[test]
fn roster_decoder_distinguishes_snapshots_from_reentry() {
    let mut bytes = support::packet(21, 76);
    bytes[3] = 2;
    bytes[73] = 2;
    assert!(matches!(insim::decode(&bytes).unwrap(), Packet::Player(_)));
    bytes[2] = 1;
    assert!(matches!(
        insim::decode(&bytes).unwrap(),
        Packet::PlayerSnapshot(_)
    ));
}

#[test]
fn configuration_defaults_off_and_round_trips_saved_preference() {
    let old: Config = toml::from_str("hide_when_background = false").unwrap();
    assert!(!old.follow_viewed_car);
    let config = Config {
        follow_viewed_car: true,
        ..Default::default()
    };
    let path = std::env::temp_dir().join(format!("openradar-follow-{}.toml", std::process::id()));
    config.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(loaded.follow_viewed_car);
}
