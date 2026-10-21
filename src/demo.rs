//! Deterministic synthetic telemetry for previewing without LFS or sockets.
use crate::{
    config::Config,
    lfs::{
        insim::{Car, Packet, Player, State, TrackInfo},
        outsim::Sample,
    },
    radar::{Engine, Pose},
    runtime::Snapshot,
};

pub struct Demo {
    engine: Engine,
    next_tick: u64,
}
impl Default for Demo {
    fn default() -> Self {
        let mut engine = Engine::default();
        for (plid, name, kind) in [
            (1, "YOU", 0),
            (2, "LEFT", 2),
            (3, "AHEAD", 6),
            (4, "RIGHT", 6),
            (5, "Driver A", 6),
            (6, "Driver B", 6),
        ] {
            engine
                .packet(
                    Packet::Player(Player {
                        plid,
                        ucid: plid,
                        kind,
                        name: name.into(),
                        model: "XRG".into(),
                        in_pits: false,
                        abs_enabled: true,
                        in_garage: false,
                    }),
                    0,
                )
                .unwrap();
        }
        engine
            .packet(
                Packet::State(State {
                    session: Default::default(),
                    flags: 1,
                    camera: 3,
                    viewed: 1,
                    track: "DEMO".into(),
                }),
                0,
            )
            .unwrap();
        engine
            .packet(
                Packet::RaceStart {
                    info: TrackInfo {
                        track: "DEMO".into(),
                        nodes: 1000,
                        finish: 0,
                        split1: 0,
                        timing: 0x40,
                        race_laps: 10,
                    },
                    requested: false,
                },
                0,
            )
            .unwrap();
        Self {
            engine,
            next_tick: 0,
        }
    }
}
impl Demo {
    pub fn snapshot(&mut self, now: u64, config: &Config) -> Snapshot {
        while self.next_tick <= now {
            let tick = self.next_tick;
            if tick > 0 && tick.is_multiple_of(100_000) {
                self.engine
                    .packet(
                        Packet::Lap {
                            plid: 1,
                            time_ms: 100_000,
                            elapsed_ms: 0,
                            penalty: 0,
                            laps_done: (tick / 100_000) as u16,
                        },
                        tick,
                    )
                    .unwrap();
            }
            let seconds = tick as f64 / 1000.0;
            let angle = seconds * std::f64::consts::TAU / 100.0;
            let heading = angle - std::f64::consts::FRAC_PI_2;
            let radius = 1500.0 / std::f64::consts::TAU;
            let me = Pose {
                x: radius * angle.sin(),
                y: radius * (1.0 - angle.cos()),
                z: 0.0,
                heading,
            };
            let (sin, cos) = heading.sin_cos();
            let mut cars = vec![Car {
                node: (tick / 100 % 1000) as u16,
                lap: (tick / 100_000 + 1) as u16,
                position: 2,
                plid: 1,
                info: 64,
                pose: me,
                speed_mps: 15.0,
                direction: heading,
            }];
            for (plid, right, forward, relative) in [
                (2, -2.6, 1.2 * seconds.sin(), 0.0),
                (3, 0.4, 6.2, 0.15 * seconds.sin()),
                (4, 3.0, -4.0 + 2.0 * (seconds * 0.5).sin(), 0.15),
            ] {
                cars.push(Car {
                    node: (tick / 100 % 1000) as u16,
                    lap: (tick / 100_000 + 1) as u16,
                    position: 0,
                    plid,
                    info: if plid == 4 { 128 } else { 0 },
                    pose: Pose {
                        x: me.x + right * cos - forward * sin,
                        y: me.y + right * sin + forward * cos,
                        z: 0.0,
                        heading: heading + relative,
                    },
                    speed_mps: 15.0,
                    direction: heading,
                });
            }
            // Separate race-order neighbors from the nearby radar cars.
            for (plid, offset, position) in [(5, 5_000_i64, 1), (6, -2_300_i64, 3)] {
                let progress_time = (tick as i64 + 100_000 + offset) as u64;
                cars.push(Car {
                    node: (progress_time / 100 % 1000) as u16,
                    lap: (progress_time / 100_000) as u16,
                    position,
                    plid,
                    info: if plid == 6 { 128 } else { 0 },
                    pose: Pose {
                        y: me.y + offset as f64 * 0.015,
                        ..me
                    },
                    speed_mps: 15.0,
                    direction: heading,
                });
            }
            cars.iter_mut().find(|c| c.plid == 4).unwrap().info = 0;
            self.engine.packet(Packet::Mci(cars), tick).unwrap();
            self.engine.outsim(
                Sample {
                    time_ms: tick as u32,
                    pose: me,
                },
                tick,
                config,
            );
            self.next_tick += 20;
        }
        let mut frame = self.engine.frame(now, config);
        frame.status = format!("DEMO · {}", frame.status);
        Snapshot {
            delta: self.engine.delta(now, config, &frame),
            gaps: self.engine.gaps(now, config, &frame),
            dashboard: crate::dashboard::DashboardFrame {
                sample: Some(crate::lfs::outgauge::Sample {
                    time_ms: now as u32,
                    car: "DEMO".into(),
                    plid: 1,
                    kmh: true,
                    gear: 6,
                    speed_mps: 248.0 / 3.6,
                    rpm: 6400.0,
                    fuel: 0.14,
                    available: crate::lfs::outgauge::ABS
                        | crate::lfs::outgauge::TC
                        | crate::lfs::outgauge::ENGINE
                        | crate::lfs::outgauge::HEADLIGHTS,
                    lights: crate::lfs::outgauge::ABS | crate::lfs::outgauge::ENGINE,
                }),
                car: Some("DEMO".into()),
                abs_enabled: Some(true),
                headlight_switch: None,
                age_ms: Some(0),
                status: "Synthetic dashboard preview".into(),
            },
            connected: true,
            fuel: fuel_preview(),
            lap_records: lap_records_preview(),
            lap_log: lap_log_preview(),
            relative: relative_preview(),
            g_force: g_force_preview(now),
            version: "synthetic".into(),
            frame,
            mci_sets: self.engine.mci_sets,
            outsim_samples: self.engine.outsim_samples,
            rejected_outsim: self.engine.rejected_outsim,
            ..Default::default()
        }
    }
}

/// Synthetic horizontal acceleration for the G-meter; never used for live data.
pub fn g_force_preview(now: u64) -> crate::g_force::GForceFrame {
    let value = |at: u64| {
        let t = at as f32 / 1000.0;
        [1.1 * t.sin(), 0.8 * (t * 1.7).cos()]
    };
    crate::g_force::GForceFrame {
        current: Some(value(now)),
        trail: (0..=60)
            .map(|i| {
                let age = i * 25;
                (value(now.saturating_sub(age)), age)
            })
            .collect(),
        outline: (0..72)
            .map(|i| {
                let angle = i as f32 * std::f32::consts::TAU / 72.0;
                Some([
                    1.4 * angle.cos(),
                    if angle.sin() > 0.0 { 1.1 } else { 0.65 } * angle.sin(),
                ])
            })
            .collect(),
        peak_g: 1.4,
        status: "Synthetic G-meter preview".into(),
    }
}

/// Coherent synthetic example: 12.9 laps of fuel, 20.6 required, −7.7 margin.
pub fn fuel_preview() -> crate::fuel::FuelFrame {
    let mut frame = crate::fuel::FuelFrame {
        car: Some("DEMO".into()),
        fraction: Some(0.14),
        low_fuel: Some(false),
        status: "Synthetic fuel preview".into(),
        ..Default::default()
    };
    frame.estimates([0.14 / 12.9, 0.016, 0.009], Some(20.6));
    frame
}

/// PB and independent sectors deliberately come from different complete laps.
pub fn lap_records_preview() -> crate::lap_records::LapRecordsFrame {
    use crate::lap_records::{LapRecordsFrame, PersonalBest, Record};
    LapRecordsFrame {
        track: Some("BL1".into()),
        car: Some("XFG".into()),
        checkpoint_count: 2,
        record: Some(Record {
            checkpoint_count: 2,
            best_sectors_ms: vec![29_000, 35_000, 35_000],
            pb: PersonalBest {
                checkpoints_ms: vec![30_000, 65_000],
                lap_ms: 100_000,
            },
        }),
        status: "Synthetic lap records preview".into(),
    }
}

/// Reference contents with previous-lap deltas and GT appearance supplied by the painter.
pub fn lap_log_preview() -> crate::lap_log::LapLogFrame {
    use crate::lap_log::{Lap, LapLogFrame};
    let values = [
        (17, 136_949, Some(-202)),
        (16, 137_151, Some(447)),
        (15, 136_704, Some(-149)),
        (14, 136_853, Some(-43_312)),
        (13, 180_165, None),
        (11, 142_291, Some(5_271)),
        (10, 137_020, Some(180)),
        (9, 136_840, Some(482)),
    ];
    LapLogFrame {
        best_ms: Some(102_089),
        laps: values
            .into_iter()
            .map(|(number, time_ms, delta_ms)| Lap {
                number,
                time_ms,
                delta_ms,
            })
            .collect(),
    }
}

/// Synthetic panel sample; never presented as live telemetry.
pub fn relative_preview() -> crate::relative::RelativeFrame {
    use crate::relative::{RelativeFrame, RelativeRow};
    RelativeFrame {
        rows: std::array::from_fn(|i| {
            Some(RelativeRow {
                plid: i as u8 + 1,
                position: Some([10, 1, 11, 12, 2, 13, 14][i]),
                name: [
                    "Driver ahead",
                    "Race leader",
                    "Pitting driver",
                    "YOU",
                    "Lapped traffic",
                    "Driver behind",
                    "Following driver",
                ][i]
                    .into(),
                seconds: if i == 2 {
                    None
                } else {
                    Some([12.0, 7.4, 2.7, 0.0, 1.8, 4.8, 6.3][i])
                },
                lap_difference: if i == 1 {
                    1
                } else if i == 4 {
                    -1
                } else {
                    0
                },
                pit: i == 2,
                fastest: i == 1,
            })
        }),
        session: "RACE".into(),
        elapsed_ms: Some(1_201_000),
        duration_ms: Some(3_600_000),
        current_lap: Some(12),
        projected_laps: Some(36.2),
        status: "DEMO · Estimated on-track gaps".into(),
        ..Default::default()
    }
}
