//! Strict OutGauge decoding. Dashboard samples are independent of radar poses.
use super::insim::{car_name, i32_at, u32_at};

pub const SHIFT: u32 = 1;
pub const TC: u32 = 1 << 4;
pub const ABS: u32 = 1 << 10;
pub const ENGINE: u32 = 1 << 11;
pub const FULLBEAM: u32 = 1 << 1;
pub const HEADLIGHTS: u32 = FULLBEAM | (1 << 14) | (1 << 16);
pub const ENGINE_SEVERE: u32 = 0x10000000;
pub const FUEL_WARNING: u32 = 1 << 15;

#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub time_ms: u32,
    pub car: String,
    pub plid: u8,
    pub kmh: bool,
    pub gear: u8,
    pub speed_mps: f32,
    pub rpm: f32,
    pub fuel: f32,
    pub available: u32,
    pub lights: u32,
}
impl Sample {
    pub fn lamp(&self, mask: u32) -> Option<bool> {
        (self.available & mask != 0).then_some(self.lights & mask != 0)
    }
    pub fn speed(&self) -> f32 {
        self.speed_mps * if self.kmh { 3.6 } else { 2.236_936_3 }
    }
    pub fn gear_label(&self) -> String {
        match self.gear {
            0 => "R".into(),
            1 => "N".into(),
            gear => (gear - 1).to_string(),
        }
    }
}

pub fn decode(p: &[u8], id: i32) -> Result<Sample, String> {
    let expected = if id == 0 { 92 } else { 96 };
    if p.len() != expected {
        return Err(format!(
            "expected {expected} OutGauge bytes; got {}",
            p.len()
        ));
    }
    if id != 0 && i32_at(p, 92) != id {
        return Err(format!(
            "outgauge ID {} does not match configured ID {id}",
            i32_at(p, 92)
        ));
    }
    let read_float = |at| f32::from_le_bytes(p[at..at + 4].try_into().unwrap());
    for at in (12..40).step_by(4).chain((48..60).step_by(4)) {
        if !read_float(at).is_finite() {
            return Err("OutGauge contains non-finite values".into());
        }
    }
    let speed_mps = read_float(12);
    let rpm = read_float(16);
    let fuel = read_float(28);
    if !(0.0..=1.0).contains(&fuel) {
        return Err("outgauge fuel must be between zero and one".into());
    }
    if speed_mps < 0.0 || rpm < 0.0 || p[10] > 31 {
        return Err("OutGauge contains invalid speed, RPM, or gear".into());
    }
    Ok(Sample {
        time_ms: u32_at(p, 0),
        car: car_name(&p[4..8]),
        plid: p[11],
        kmh: u16::from_le_bytes(p[8..10].try_into().unwrap()) & 16384 != 0,
        gear: p[10],
        speed_mps,
        rpm,
        fuel,
        available: u32_at(p, 40),
        lights: u32_at(p, 44),
    })
}
