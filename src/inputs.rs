//! Live OutSim controls and a bounded, time-based pedal history.
use crate::lfs::outsim::{Inputs, Telemetry};
use std::collections::VecDeque;

pub const HISTORY_MS: u64 = 5_000;
const SAMPLE_MS: u64 = 20;
const MAX_POINTS: usize = (HISTORY_MS / SAMPLE_MS + 1) as usize;

/// Only apply documented ratios or an explicit per-car override. OutSim keeps
/// its raw road-wheel input angle; the cockpit-wheel conversion is presentation.
pub fn steering_ratio(config: &crate::config::Config, car: Option<&str>) -> Option<f32> {
    let car = car?;
    config
        .cars
        .get(car)
        .and_then(|profile| profile.steering_ratio)
        .or(match car {
            // https://en.lfsmanual.net/wiki/XFG: 30° lock, 900° total wheel travel, 15:1.
            "XFG" | "DEMO" => Some(15.0),
            _ => None,
        })
}

#[derive(Clone, Debug, Default)]
pub struct InputsFrame {
    pub current: Option<Inputs>,
    /// Oldest to newest: age in milliseconds and [throttle, brake], 0..=1.
    pub history: Vec<(u64, [f32; 2])>,
    pub status: String,
}

#[derive(Default)]
pub struct InputsTelemetry {
    latest: Option<(u32, u64, Inputs)>,
    history: VecDeque<(u64, [f32; 2])>,
}

impl InputsTelemetry {
    pub fn clear(&mut self) {
        self.latest = None;
        self.history.clear();
    }

    pub fn receive(&mut self, telemetry: &Telemetry, now: u64, stale_ms: u64) {
        let Some(inputs) = &telemetry.inputs else {
            self.clear();
            return;
        };
        if let Some((time, received, _)) = &self.latest {
            let elapsed = telemetry.sample.time_ms.wrapping_sub(*time) as i32;
            // Duplicate timestamps must not keep paused controls alive.
            if elapsed == 0 || (elapsed < 0 && now.saturating_sub(*received) <= stale_ms) {
                return;
            }
            if now.saturating_sub(*received) > stale_ms {
                self.clear();
            }
        }
        self.latest = Some((telemetry.sample.time_ms, now, inputs.clone()));
        if self
            .history
            .back()
            .is_none_or(|(at, _)| now.saturating_sub(*at) >= SAMPLE_MS)
        {
            self.history
                .push_back((now, [inputs.throttle, inputs.brake]));
        }
        while self.history.len() > MAX_POINTS
            || self
                .history
                .front()
                .is_some_and(|(at, _)| now.saturating_sub(*at) > HISTORY_MS)
        {
            self.history.pop_front();
        }
    }

    pub fn frame(&self, now: u64, stale_ms: u64) -> InputsFrame {
        let current = self
            .latest
            .as_ref()
            .filter(|(_, at, _)| now.saturating_sub(*at) <= stale_ms)
            .map(|(_, _, inputs)| inputs.clone());
        InputsFrame {
            history: if current.is_some() {
                self.history
                    .iter()
                    .filter_map(|(at, value)| {
                        let age = now.saturating_sub(*at);
                        (age <= HISTORY_MS).then_some((age, *value))
                    })
                    .collect()
            } else {
                Vec::new()
            },
            status: if current.is_some() {
                "OutSim inputs connected"
            } else if self.latest.is_some() {
                "OutSim inputs stale"
            } else {
                "Waiting for OutSim inputs · enable INPUTS in OutSim options"
            }
            .into(),
            current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(time: u32) -> Telemetry {
        let mut bytes = [0; 84];
        bytes[..4].copy_from_slice(&time.to_le_bytes());
        for (at, value) in [(64, 0.75_f32), (68, 0.25), (72, -0.5), (76, 1.0)] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        crate::lfs::outsim::decode_telemetry(&bytes, 0x1c, None).unwrap()
    }

    #[test]
    fn history_is_bounded_and_never_bridges_stale_paused_or_missing_inputs() {
        let mut meter = InputsTelemetry::default();
        for time in 0..10_000 {
            meter.receive(&sample(time), u64::from(time), 250);
        }
        let frame = meter.frame(9999, 250);
        assert_eq!(frame.current.unwrap().steer_rad, -0.5);
        assert_eq!(frame.history.len(), 250);
        assert!(
            frame
                .history
                .iter()
                .all(|(age, v)| *age <= HISTORY_MS && *v == [0.75, 0.25])
        );
        // Reordered and duplicate packets cannot refresh the receive time.
        meter.receive(&sample(9998), 10_010, 250);
        meter.receive(&sample(9999), 10_300, 250);
        assert!(meter.frame(10_300, 250).current.is_none());
        assert!(meter.frame(10_300, 250).history.is_empty());
        meter.receive(&sample(0), 10_700, 250);
        assert_eq!(meter.frame(10_700, 250).history.len(), 1);
        let mut missing = sample(20);
        missing.inputs = None;
        meter.receive(&missing, 10_720, 250);
        assert!(meter.frame(10_720, 250).current.is_none());
        assert!(meter.history.is_empty());
        meter.receive(&sample(u32::MAX - 9), 11_000, 250);
        meter.receive(&sample(10), 11_020, 250);
        assert_eq!(meter.frame(11_020, 250).history.len(), 2);
        meter.clear();
        assert!(meter.frame(11_020, 250).current.is_none());
    }
}
