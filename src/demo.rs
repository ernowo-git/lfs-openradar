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
            version: "synthetic".into(),
            frame,
            mci_sets: self.engine.mci_sets,
            outsim_samples: self.engine.outsim_samples,
            rejected_outsim: self.engine.rejected_outsim,
            ..Default::default()
        }
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
