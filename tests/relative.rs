mod support;
use lfs_openradar::{
    config::Config,
    lfs::insim::{self, Car, Packet, Player, SessionInfo, State, TrackInfo},
    radar::{Engine, Pose},
    relative::RelativeEngine,
};
use std::collections::BTreeMap;

fn state(kind: u8) -> State {
    State {
        session: SessionInfo {
            kind,
            race_laps: if kind == 1 { 191 } else { 0 },
            qual_mins: 20,
        },
        flags: 1,
        camera: 3,
        viewed: 1,
        track: "BL1".into(),
    }
}
fn track(kind: u8) -> TrackInfo {
    TrackInfo {
        track: "BL1".into(),
        nodes: 1000,
        finish: 200,
        split1: 500,
        timing: 0x40,
        race_laps: if kind == 1 { 191 } else { 0 },
    }
}
fn players() -> BTreeMap<u8, Player> {
    (1..=7)
        .map(|id| {
            let Packet::Player(p) = support::player(id, if id == 1 { 0 } else { 6 }) else {
                unreachable!()
            };
            (id, p)
        })
        .collect()
}
fn car(id: u8, progress: i64) -> Car {
    Car {
        plid: id,
        position: if id == 2 { 1 } else { 10 + id },
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
fn cars(now: u64) -> Vec<Car> {
    let progress = 2940 + now as i64 / 100;
    [0, 1050, -1023, -1, 2, 7, -10]
        .into_iter()
        .enumerate()
        .map(|(i, offset)| car(i as u8 + 1, progress + offset))
        .collect()
}
fn setup(kind: u8, observed_start: bool) -> RelativeEngine {
    let mut engine = RelativeEngine::default();
    let players = players();
    engine.packet(&Packet::State(state(kind)), 0, &players);
    engine.packet(
        &Packet::RaceStart {
            info: track(kind),
            requested: !observed_start,
        },
        0,
        &players,
    );
    for now in (0..=10_000).step_by(100) {
        engine.update(&cars(now), now);
    }
    engine
}
fn lap(id: u8, number: u16, time: u32) -> Packet {
    Packet::Lap {
        plid: id,
        time_ms: time,
        elapsed_ms: u32::from(number) * time,
        penalty: 0,
        laps_done: number,
    }
}
#[test]
fn all_session_types_use_track_order_and_common_passages_across_finish_and_lap_wrap() {
    for kind in 0..=2 {
        let engine = setup(kind, true);
        let frame = engine.frame(Some(1), &players(), 10_000, 250);
        assert_eq!(
            frame
                .rows
                .iter()
                .flatten()
                .map(|r| r.plid)
                .collect::<Vec<_>>(),
            [2, 6, 5, 1, 4, 7, 3]
        );
        let ahead = frame.rows[0].as_ref().unwrap();
        let behind = frame.rows[6].as_ref().unwrap();
        assert_eq!(ahead.seconds, Some(5.0));
        assert_eq!(behind.seconds, Some(2.3));
        assert_eq!(ahead.lap_difference, if kind == 1 { 1 } else { 0 });
        assert_eq!(behind.lap_difference, if kind == 1 { -1 } else { 0 });
        assert_eq!(frame.current_lap, Some(3));
        assert_eq!(frame.rows[3].as_ref().unwrap().seconds, Some(0.0));
    }
}
#[test]
fn pits_garages_sparse_fields_and_stalls_do_not_fabricate_gaps_or_duplicate_rows() {
    let mut engine = setup(1, true);
    let mut players = players();
    engine.packet(
        &Packet::PitLane {
            plid: 5,
            entered: true,
        },
        10_000,
        &players,
    );
    let mut samples = cars(10_100);
    samples.iter_mut().find(|c| c.plid == 5).unwrap().info = 4;
    engine.update(&samples, 10_100);
    let frame = engine.frame(Some(1), &players, 10_100, 250);
    let pitting = frame.rows[2].as_ref().unwrap();
    assert!(pitting.pit);
    assert_eq!(pitting.seconds, None);
    players.get_mut(&5).unwrap().in_garage = true;
    assert!(
        !engine
            .frame(Some(1), &players, 10_100, 250)
            .rows
            .iter()
            .flatten()
            .any(|r| r.plid == 5)
    );
    assert!(
        engine
            .frame(Some(1), &players, 11_000, 250)
            .rows
            .iter()
            .all(Option::is_none)
    );
    engine.update(&cars(11_000)[..2], 11_000);
    let frame = engine.frame(Some(1), &players, 11_000, 250);
    assert_eq!(frame.rows.iter().flatten().count(), 2);
    assert_eq!(frame.rows[2].as_ref().unwrap().seconds, None);
}
#[test]
fn fastest_badge_requires_session_coverage_and_keeps_departed_drivers_records() {
    for observed in [false, true] {
        let mut engine = setup(1, observed);
        let mut players = players();
        engine.packet(&lap(2, 1, 59_000), 10_000, &players);
        assert_eq!(
            engine.frame(Some(1), &players, 10_000, 250).rows[0]
                .as_ref()
                .unwrap()
                .fastest,
            observed
        );
        players.get_mut(&2).unwrap().name = "Replacement driver".into();
        engine.packet(&Packet::Player(players[&2].clone()), 10_000, &players);
        engine.update(&cars(10_100), 10_100);
        engine.packet(&lap(3, 1, 60_000), 10_100, &players);
        assert!(
            engine
                .frame(Some(1), &players, 10_100, 250)
                .rows
                .iter()
                .flatten()
                .all(|r| !r.fastest)
        );
        engine.packet(&lap(3, 2, 58_000), 10_100, &players);
        assert_eq!(
            engine.frame(Some(1), &players, 10_100, 250).rows[6]
                .as_ref()
                .unwrap()
                .fastest,
            observed
        );
    }
}
#[test]
fn a_complete_qualifying_result_table_can_restore_the_fastest_badge_after_late_join() {
    let mut engine = setup(2, false);
    let players = players();
    let result = |id, index, best| Packet::Result {
        plid: id,
        name: players[&id].name.clone(),
        best_ms: best,
        index,
        total: 2,
        requested: true,
    };
    engine.packet(&result(2, 0, 59_000), 10_000, &players);
    assert!(
        !engine.frame(Some(1), &players, 10_000, 250).rows[0]
            .as_ref()
            .unwrap()
            .fastest
    );
    engine.packet(&result(3, 1, 60_000), 10_000, &players);
    assert!(
        engine.frame(Some(1), &players, 10_000, 250).rows[0]
            .as_ref()
            .unwrap()
            .fastest
    );
    engine.clear();
    assert!(
        engine
            .frame(Some(1), &players, 10_000, 250)
            .rows
            .iter()
            .all(Option::is_none)
    );
}
#[test]
fn native_duration_and_recent_complete_leader_laps_produce_an_explicit_projection() {
    let mut engine = setup(1, true);
    let players = players();
    engine.packet(&Packet::RaceTime(200_000), 10_000, &players);
    for number in 1..=2 {
        engine.packet(&lap(2, number, 60_000), 10_000, &players);
    }
    assert_eq!(
        engine.frame(Some(1), &players, 10_000, 250).projected_laps,
        None
    );
    engine.packet(&lap(2, 3, 60_000), 10_000, &players);
    let frame = engine.frame(Some(1), &players, 10_000, 250);
    assert_eq!(frame.duration_ms, Some(3_600_000));
    assert_eq!(frame.elapsed_ms, Some(200_000));
    // Leader: current lap 4, 9% through; plus remaining time divided by median pace.
    assert!((frame.projected_laps.unwrap() - (3.09 + 3_400_000.0 / 60_000.0)).abs() < 0.00001);
    engine.packet(
        &Packet::PitLane {
            plid: 2,
            entered: true,
        },
        10_000,
        &players,
    );
    assert_eq!(
        engine.frame(Some(1), &players, 10_000, 250).projected_laps,
        None
    );
}
#[test]
fn unsupported_paths_and_pauses_leave_no_misleading_traffic_order() {
    let players = players();
    let mut engine = setup(0, true);
    let mut unsupported = track(0);
    unsupported.timing = 0x80;
    engine.packet(
        &Packet::RaceStart {
            info: unsupported,
            requested: true,
        },
        10_000,
        &players,
    );
    engine.update(&cars(10_100), 10_100);
    assert!(
        engine
            .frame(Some(1), &players, 10_100, 250)
            .rows
            .iter()
            .all(Option::is_none)
    );
    let mut engine = setup(1, true);
    let mut paused = state(1);
    paused.flags |= 4;
    engine.packet(&Packet::State(paused), 10_000, &players);
    assert!(
        engine
            .frame(Some(1), &players, 10_000, 250)
            .rows
            .iter()
            .all(Option::is_none)
    );
}
#[test]
fn relative_is_live_without_outsim_while_radar_remains_paused() {
    let config = Config::default();
    let mut engine = Engine::default();
    for player in players().into_values() {
        engine.packet(Packet::Player(player), 0).unwrap();
    }
    engine.packet(Packet::State(state(1)), 0).unwrap();
    engine
        .packet(
            Packet::RaceStart {
                info: track(1),
                requested: false,
            },
            0,
        )
        .unwrap();
    let mut samples = cars(0);
    samples.first_mut().unwrap().info = 64;
    samples.last_mut().unwrap().info = 128;
    engine.packet(Packet::Mci(samples), 0).unwrap();
    assert!(!engine.frame(0, &config).live);
    assert!(engine.relative(0, &config).rows[3].is_some());
}
#[test]
fn session_clock_results_and_initial_pit_flags_decode_with_protocol_nine_units() {
    let mut clock = support::packet(4, 8);
    clock[3] = 6;
    clock[4..8].copy_from_slice(&12345_u32.to_le_bytes());
    assert!(matches!(
        insim::decode(&clock).unwrap(),
        Packet::RaceTime(123450)
    ));
    let mut result = support::packet(35, 84);
    result[2] = 1;
    result[3] = 2;
    result[28..36].copy_from_slice(b"Driver 2");
    result[68..72].copy_from_slice(&59000_u32.to_le_bytes());
    result[81] = 1;
    assert!(matches!(
        insim::decode(&result).unwrap(),
        Packet::Result {
            plid: 2,
            best_ms: 59000,
            index: 0,
            total: 1,
            requested: true,
            ..
        }
    ));
    assert!(insim::decode(&support::packet(35, 80)).is_err());
    let mut player = support::packet(21, 76);
    player[6..8].copy_from_slice(&0x100_u16.to_le_bytes());
    player[73] = 1;
    assert!(matches!(insim::decode(&player).unwrap(),Packet::Player(p) if p.in_pits));
}
