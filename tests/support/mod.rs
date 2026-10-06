#![allow(dead_code)]
use lfs_openradar::{
    config::Config,
    lfs::{
        insim::{Car, Packet, Player, State},
        outsim::Sample,
    },
    radar::{Engine, Pose},
};

pub fn packet(kind: u8, size: usize) -> Vec<u8> {
    let mut p = vec![0; size];
    p[0] = (size / 4) as u8;
    p[1] = kind;
    p
}
pub fn mci(ids: &[u8], first: bool, last: bool) -> Vec<u8> {
    let mut p = packet(38, 4 + 28 * ids.len());
    p[3] = ids.len() as u8;
    for (i, id) in ids.iter().enumerate() {
        let base = 4 + i * 28;
        p[base + 4] = *id;
        p[base + 6] =
            if i == 0 && first { 64 } else { 0 } | if i + 1 == ids.len() && last { 128 } else { 0 };
        p[base + 8..base + 12].copy_from_slice(&(-65536_i32).to_le_bytes());
        p[base + 12..base + 16].copy_from_slice(&(131072_i32).to_le_bytes());
        p[base + 20..base + 22].copy_from_slice(&(32768_u16).to_le_bytes());
        p[base + 24..base + 26].copy_from_slice(&(16384_u16).to_le_bytes());
    }
    p
}
pub fn full_outsim(time: u32, x: f64, y: f64, heading: f32) -> Vec<u8> {
    let mut p = vec![0; 280];
    p[..4].copy_from_slice(b"LFST");
    p[4..8].copy_from_slice(&24601_i32.to_le_bytes());
    p[8..12].copy_from_slice(&time.to_le_bytes());
    p[24..28].copy_from_slice(&heading.to_le_bytes());
    p[60..64].copy_from_slice(&((x * 65536.0) as i32).to_le_bytes());
    p[64..68].copy_from_slice(&((y * 65536.0) as i32).to_le_bytes());
    p
}
pub fn player(id: u8, kind: u8) -> Packet {
    Packet::Player(Player {
        plid: id,
        ucid: id,
        kind,
        name: format!("Driver {id}"),
        model: "XRG".into(),
        in_garage: false,
    })
}
pub fn setup() -> Engine {
    let mut engine = Engine::default();
    engine.packet(player(1, 0), 0).unwrap();
    engine.packet(player(2, 2), 0).unwrap();
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
}
pub fn mci_tick(engine: &mut Engine, time: u64, heading: f64) {
    let me = Pose {
        heading,
        ..Default::default()
    };
    engine
        .packet(
            Packet::Mci(vec![
                Car {
                    plid: 1,
                    info: 64,
                    pose: me,
                    speed_mps: 0.0,
                    direction: heading,
                },
                Car {
                    plid: 2,
                    info: 128,
                    pose: Pose {
                        x: 3.0,
                        y: 0.0,
                        z: 0.0,
                        heading,
                    },
                    speed_mps: 0.0,
                    direction: heading,
                },
            ]),
            time,
        )
        .unwrap();
}
pub fn tick(engine: &mut Engine, time: u64, heading: f64) {
    mci_tick(engine, time, heading);
    engine.outsim(
        Sample {
            time_ms: time as u32,
            pose: Pose {
                heading,
                ..Default::default()
            },
        },
        time,
        &Config::default(),
    );
}
