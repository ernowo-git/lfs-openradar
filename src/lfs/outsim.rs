//! Strict decoders for legacy OutSim and the configurable LFST layout.
use super::insim::{i32_at, u32_at};
use crate::radar::Pose;

#[derive(Clone, Debug)]
pub struct Sample {
    pub time_ms: u32,
    pub pose: Pose,
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
    Ok(Sample {
        time_ms: u32_at(p, time_offset),
        pose: Pose {
            x: f64::from(i32_at(p, main_offset + 48)) / 65536.0,
            y: f64::from(i32_at(p, main_offset + 52)) / 65536.0,
            z: f64::from(i32_at(p, main_offset + 56)) / 65536.0,
            heading: f64::from(f32::from_le_bytes(
                p[main_offset + 12..main_offset + 16].try_into().unwrap(),
            )),
        },
    })
}
