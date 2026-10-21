//! Bounded, session-only horizontal acceleration history. No external cache.
use crate::lfs::outsim::Telemetry;
use std::collections::VecDeque;

const GRAVITY: f64 = 9.80665;
const TRAIL_MS: u64 = 1500;
const MAX_POINTS: usize = 96;
const SECTORS: usize = 72;

#[derive(Clone, Debug, Default)]
pub struct GForceFrame {
    /// Right and forward acceleration in G; braking is negative forward.
    pub current: Option<[f32; 2]>,
    pub trail: Vec<([f32; 2], u64)>,
    /// Measured maxima per angular sector; unobserved directions stay empty.
    pub outline: Vec<Option<[f32; 2]>>,
    pub peak_g: f32,
    pub status: String,
}

pub struct GForceTelemetry {
    latest: Option<(u32, u64, [f32; 2])>,
    trail: VecDeque<([f32; 2], u64)>,
    outline: [Option<[f32; 2]>; SECTORS],
    peak_g: f32,
}

impl Default for GForceTelemetry {
    fn default() -> Self {
        Self {
            latest: None,
            trail: VecDeque::new(),
            outline: [None; SECTORS],
            peak_g: 0.0,
        }
    }
}

impl GForceTelemetry {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Keep measured peaks across a pause or temporary camera change.
    pub fn suspend(&mut self) {
        self.latest = None;
        self.trail.clear();
    }

    pub fn receive(&mut self, telemetry: &Telemetry, now: u64, stale_ms: u64) {
        let time = telemetry.sample.time_ms;
        let mut dt = None;
        if let Some((previous, received, _)) = self.latest {
            let elapsed = time.wrapping_sub(previous) as i32;
            if elapsed == 0 {
                return;
            }
            if now.saturating_sub(received) > stale_ms {
                self.suspend();
                // A restarted simulation clock begins a new driving session.
                if elapsed < 0 {
                    self.clear();
                }
            } else if elapsed < 0 {
                return;
            } else {
                dt = Some(elapsed as f64);
            }
        }
        // OutSim Accel is world-space. Project the horizontal motion onto the
        // heading's right/forward axes; vertical acceleration is excluded.
        let (sin, cos) = telemetry.sample.pose.heading.sin_cos();
        let [x, y, _] = telemetry.acceleration.map(f64::from);
        let raw = [
            (x * cos + y * sin) / GRAVITY,
            (-x * sin + y * cos) / GRAVITY,
        ];
        if raw.iter().any(|v| !v.is_finite()) {
            return;
        }
        let mut current = raw.map(|v| v as f32);
        if let (Some(dt), Some((_, _, previous))) = (dt, self.latest) {
            let amount = (1.0 - (-dt / 80.0).exp()) as f32;
            for i in 0..2 {
                current[i] = previous[i] + (current[i] - previous[i]) * amount;
            }
        }
        self.latest = Some((time, now, current));
        if self
            .trail
            .back()
            .is_none_or(|(_, at)| now.saturating_sub(*at) >= 20)
        {
            self.trail.push_back((current, now));
        }
        while self.trail.len() > MAX_POINTS
            || self
                .trail
                .front()
                .is_some_and(|(_, at)| now.saturating_sub(*at) > TRAIL_MS)
        {
            self.trail.pop_front();
        }
        let magnitude = current[0].hypot(current[1]);
        self.peak_g = self.peak_g.max(magnitude);
        if magnitude > 0.02 {
            let angle = current[1]
                .atan2(current[0])
                .rem_euclid(std::f32::consts::TAU);
            let index = (angle / std::f32::consts::TAU * SECTORS as f32) as usize % SECTORS;
            if self.outline[index].is_none_or(|old| old[0].hypot(old[1]) < magnitude) {
                self.outline[index] = Some(current);
            }
        }
    }

    pub fn frame(&self, now: u64, stale_ms: u64) -> GForceFrame {
        let current = self
            .latest
            .filter(|(_, at, _)| now.saturating_sub(*at) <= stale_ms);
        GForceFrame {
            current: current.map(|(_, _, value)| value),
            trail: if current.is_some() {
                self.trail
                    .iter()
                    .filter_map(|(value, at)| {
                        let age = now.saturating_sub(*at);
                        (age <= TRAIL_MS).then_some((*value, age))
                    })
                    .collect()
            } else {
                Vec::new()
            },
            outline: self.outline.to_vec(),
            peak_g: self.peak_g,
            status: if current.is_some() {
                "OutSim connected · horizontal G"
            } else if self.latest.is_some() {
                "OutSim stale"
            } else {
                "Waiting for matching OutSim telemetry"
            }
            .into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(time: u32, heading: f64, acceleration: [f32; 3]) -> Telemetry {
        let mut bytes = vec![0; 64];
        bytes[..4].copy_from_slice(&time.to_le_bytes());
        bytes[16..20].copy_from_slice(&(heading as f32).to_le_bytes());
        for (i, value) in acceleration.iter().enumerate() {
            bytes[28 + i * 4..32 + i * 4].copy_from_slice(&value.to_le_bytes());
        }
        crate::lfs::outsim::decode_telemetry(&bytes, 0, None).unwrap()
    }

    #[test]
    fn projects_world_axes_and_excludes_vertical_g() {
        for heading in [0.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI] {
            let (sin, cos) = heading.sin_cos();
            let mut meter = GForceTelemetry::default();
            meter.receive(
                &sample(
                    10,
                    heading,
                    [(GRAVITY * cos) as f32, (GRAVITY * sin) as f32, 98.0],
                ),
                10,
                250,
            );
            let [right, forward] = meter.frame(10, 250).current.unwrap();
            assert!((right - 1.0).abs() < 0.0001);
            assert!(forward.abs() < 0.0001);
            meter.clear();
            meter.receive(
                &sample(
                    10,
                    heading,
                    [(GRAVITY * sin) as f32, (-GRAVITY * cos) as f32, 0.0],
                ),
                10,
                250,
            );
            let [right, forward] = meter.frame(10, 250).current.unwrap();
            assert!(right.abs() < 0.0001);
            assert!((forward + 1.0).abs() < 0.0001);
        }
    }

    #[test]
    fn bounds_history_rejects_reordering_and_preserves_peaks_when_suspended() {
        let mut meter = GForceTelemetry::default();
        for time in (0..10_000).step_by(10) {
            meter.receive(
                &sample(time, 0.0, [9.80665, 0.0, 0.0]),
                u64::from(time),
                250,
            );
        }
        let before = meter.frame(9990, 250);
        assert!(before.trail.len() <= MAX_POINTS);
        assert!(before.trail.iter().all(|(_, age)| *age <= TRAIL_MS));
        assert_eq!(before.outline.iter().flatten().count(), 1);
        for time in [9990, 9980] {
            meter.receive(&sample(time, 0.0, [1000.0, 1000.0, 0.0]), 9995, 250);
        }
        assert_eq!(meter.frame(9995, 250).peak_g, before.peak_g);
        assert!(meter.frame(10_300, 250).current.is_none());
        assert!(meter.frame(10_300, 250).trail.is_empty());
        meter.receive(&sample(9990, 0.0, [1000.0, 1000.0, 0.0]), 10_300, 250);
        assert!(meter.frame(10_300, 250).current.is_none());
        assert_eq!(meter.frame(10_300, 250).peak_g, before.peak_g);
        meter.suspend();
        assert_eq!(meter.frame(11_000, 250).peak_g, before.peak_g);
        meter.clear();
        assert_eq!(meter.frame(11_000, 250).peak_g, 0.0);
    }

    #[test]
    fn clock_wrap_is_forward_but_a_restart_after_silence_resets_peaks() {
        let mut meter = GForceTelemetry::default();
        meter.receive(&sample(u32::MAX - 9, 0.0, [9.80665, 0.0, 0.0]), 0, 250);
        meter.receive(&sample(10, 0.0, [0.0; 3]), 20, 250);
        assert!(meter.frame(20, 250).current.unwrap()[0] < 1.0);
        assert!(meter.frame(20, 250).peak_g > 0.99);
        meter.receive(&sample(0, 0.0, [0.0; 3]), 1000, 250);
        assert_eq!(meter.frame(1000, 250).peak_g, 0.0);
    }
}
