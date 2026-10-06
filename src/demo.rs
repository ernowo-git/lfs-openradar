//! Deterministic synthetic telemetry for previewing without LFS or sockets.
use crate::{
    config::Config,
    lfs::{
        insim::{Car, Packet, Player, State},
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
        ] {
            engine
                .packet(
                    Packet::Player(Player {
                        plid,
                        ucid: plid,
                        kind,
                        name: name.into(),
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
                    track: "DEMO".into(),
                }),
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
            let seconds = tick as f64 / 1000.0;
            let heading = 0.35 * (seconds * 0.4).sin();
            let me = Pose {
                x: 0.0,
                y: seconds * 15.0,
                z: 0.0,
                heading,
            };
            let (sin, cos) = heading.sin_cos();
            let mut cars = vec![Car {
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
            connected: true,
            version: "synthetic".into(),
            frame,
            mci_sets: self.engine.mci_sets,
            outsim_samples: self.engine.outsim_samples,
            rejected_outsim: self.engine.rejected_outsim,
            ..Default::default()
        }
    }
}
