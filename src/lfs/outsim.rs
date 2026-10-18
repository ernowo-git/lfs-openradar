//! Strict decoders for legacy OutSim and the configurable LFST layout.
use super::insim::{i32_at, u32_at};
use crate::radar::Pose;

#[derive(Clone, Debug)]
pub struct Sample {
    pub time_ms: u32,
    pub pose: Pose,
}

/// Values retain the protocol's units. Wheel indices intentionally have no
/// corner labels until their order has been checked against a live capture.
#[derive(Clone, Debug)]
pub struct Telemetry {
    pub sample: Sample,
    pub angular_velocity: [f32; 3],
    pub acceleration: [f32; 3],
    pub velocity: [f32; 3],
    pub pitch: f32,
    pub roll: f32,
    pub inputs: Option<Inputs>,
    pub drive: Option<Drive>,
    pub distance: Option<Distance>,
    pub wheels: Option<[Wheel; 4]>,
    pub steering_torque_nm: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct Inputs {
    pub throttle: f32,
    pub brake: f32,
    pub steer_rad: f32,
    pub clutch: f32,
    pub handbrake: f32,
}

#[derive(Clone, Debug)]
pub struct Drive {
    pub gear: u8,
    pub engine_rad_s: f32,
    pub max_torque_nm: f32,
}

#[derive(Clone, Debug)]
pub struct Distance {
    pub driven_m: f32,
    pub indexed_m: f32,
}

#[derive(Clone, Debug)]
pub struct Wheel {
    pub suspension_m: f32,
    pub steer_rad: f32,
    pub force_x_n: f32,
    pub force_y_n: f32,
    pub vertical_load_n: f32,
    pub angular_velocity_rad_s: f32,
    pub lean_rad: f32,
    pub air_temp_c: u8,
    pub slip_fraction: u8,
    pub touching: u8,
    pub slip_ratio: f32,
    pub tan_slip_angle: f32,
}

pub fn packet_size(options: u16, id: Option<i32>) -> Result<usize, String> {
    if options == 0 {
        return Ok(if id.is_some() { 68 } else { 64 });
    }
    if options & !0x1ff != 0 || options & 0xc != 0xc {
        return Err("OutSim TIME and MAIN fields are required".into());
    }
    Ok([
        (1, 4),
        (2, 4),
        (4, 4),
        (8, 60),
        (16, 20),
        (32, 12),
        (64, 8),
        (128, 160),
        (256, 8),
    ]
    .into_iter()
    .filter(|(flag, _)| options & flag != 0)
    .map(|(_, size)| size)
    .sum())
}
pub fn decode(p: &[u8], options: u16, id: Option<i32>) -> Result<Sample, String> {
    Ok(decode_telemetry(p, options, id)?.sample)
}

pub fn decode_telemetry(p: &[u8], options: u16, id: Option<i32>) -> Result<Telemetry, String> {
    let size = packet_size(options, id)?;
    if p.len() != size {
        return Err(format!("Expected {size} OutSim bytes; got {}", p.len()));
    }
    let (time_offset, main_offset, id_offset) = if options == 0 {
        (0, 4, id.map(|_| 64))
    } else {
        let mut offset = 0;
        if options & 1 != 0 {
            if &p[0..4] != b"LFST" {
                return Err("Missing LFST OutSim header".into());
            }
            offset += 4;
        }
        let id_offset = if options & 2 != 0 {
            let at = offset;
            offset += 4;
            Some(at)
        } else {
            None
        };
        (offset, offset + 4, id_offset)
    };
    if let Some(expected) = id {
        let at = id_offset.ok_or("OutSim configuration requires an ID field")?;
        if i32_at(p, at) != expected {
            return Err("OutSim ID does not match configuration".into());
        }
    }
    for at in (main_offset..main_offset + 48).step_by(4) {
        if !f32::from_le_bytes(p[at..at + 4].try_into().unwrap()).is_finite() {
            return Err("OutSim contains non-finite physics values".into());
        }
    }
    let sample = Sample {
        time_ms: u32_at(p, time_offset),
        pose: Pose {
            x: f64::from(i32_at(p, main_offset + 48)) / 65536.0,
            y: f64::from(i32_at(p, main_offset + 52)) / 65536.0,
            z: f64::from(i32_at(p, main_offset + 56)) / 65536.0,
            heading: f64::from(f32::from_le_bytes(
                p[main_offset + 12..main_offset + 16].try_into().unwrap(),
            )),
        },
    };
    let f = |at| f32::from_le_bytes(p[at..at + 4].try_into().unwrap());
    let vector = |at| [f(at), f(at + 4), f(at + 8)];
    let mut offset = main_offset + 60;
    let mut block = |flag, size| {
        if options & flag != 0 {
            let at = offset;
            offset += size;
            Some(at)
        } else {
            None
        }
    };
    let inputs = block(16, 20).map(|at| Inputs {
        throttle: f(at),
        brake: f(at + 4),
        steer_rad: f(at + 8),
        clutch: f(at + 12),
        handbrake: f(at + 16),
    });
    let drive = block(32, 12).map(|at| Drive {
        gear: p[at],
        engine_rad_s: f(at + 4),
        max_torque_nm: f(at + 8),
    });
    let distance = block(64, 8).map(|at| Distance {
        driven_m: f(at),
        indexed_m: f(at + 4),
    });
    let wheels = block(128, 160).map(|at| {
        std::array::from_fn(|i| {
            let at = at + i * 40;
            Wheel {
                suspension_m: f(at),
                steer_rad: f(at + 4),
                force_x_n: f(at + 8),
                force_y_n: f(at + 12),
                vertical_load_n: f(at + 16),
                angular_velocity_rad_s: f(at + 20),
                lean_rad: f(at + 24),
                air_temp_c: p[at + 28],
                slip_fraction: p[at + 29],
                touching: p[at + 30],
                slip_ratio: f(at + 32),
                tan_slip_angle: f(at + 36),
            }
        })
    });
    let steering_torque_nm = block(256, 8).map(f);
    // Validate all defined floating-point values, excluding spare bytes.
    let mut at = main_offset + 60;
    let fields: [(u16, usize, usize, &[usize]); 5] = [
        (16, 20, 1, &[0, 4, 8, 12, 16]),
        (32, 12, 1, &[4, 8]),
        (64, 8, 1, &[0, 4]),
        (128, 40, 4, &[0, 4, 8, 12, 16, 20, 24, 32, 36]),
        (256, 8, 1, &[0]),
    ];
    for (flag, stride, count, float_offsets) in fields {
        if options & flag != 0 {
            for i in 0..count {
                if float_offsets
                    .iter()
                    .any(|o| !f(at + i * stride + o).is_finite())
                {
                    return Err("outsim contains non-finite telemetry values".into());
                }
            }
            at += stride * count;
        }
    }
    if inputs.as_ref().is_some_and(|v| {
        [v.throttle, v.brake, v.clutch, v.handbrake]
            .iter()
            .any(|v| !(0.0..=1.0).contains(v))
    }) {
        return Err("outsim pedal inputs must be between zero and one".into());
    }
    Ok(Telemetry {
        sample,
        angular_velocity: vector(main_offset),
        acceleration: vector(main_offset + 24),
        velocity: vector(main_offset + 36),
        pitch: f(main_offset + 16),
        roll: f(main_offset + 20),
        inputs,
        drive,
        distance,
        wheels,
        steering_torque_nm,
    })
}
