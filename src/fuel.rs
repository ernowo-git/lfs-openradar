//! Fuel estimates from accepted OutGauge samples and the selected car's race progress.
use crate::{
    config::Config,
    lfs::{
        insim::{Car, TrackInfo},
        outgauge,
    },
};
use std::collections::VecDeque;

#[derive(Clone, Debug, Default)]
pub struct FuelRow {
    pub usage: Option<f64>,
    pub laps: Option<f64>,
    pub refuel: Option<f64>,
    pub multiple_stops: bool,
}

#[derive(Clone, Debug, Default)]
pub struct FuelFrame {
    pub car: Option<String>,
    pub fraction: Option<f64>,
    pub low_fuel: Option<bool>,
    pub rows: [FuelRow; 3],
    pub remaining_race_laps: Option<f64>,
    pub margin: Option<f64>,
    pub status: String,
}
impl FuelFrame {
    pub fn estimates(&mut self, usage: [f64; 3], remaining: Option<f64>) {
        self.rows = Default::default();
        self.margin = None;
        self.remaining_race_laps = remaining;
        let Some(fuel) = self.fraction else {
            return;
        };
        for (row, burn) in self.rows.iter_mut().zip(usage) {
            if burn > 0.0 && burn.is_finite() {
                row.usage = Some(burn);
                row.laps = Some(fuel / burn);
                row.refuel = remaining.map(|laps| (laps * burn - fuel).max(0.0));
                row.multiple_stops = row.refuel.is_some_and(|amount| amount > 1.0 - fuel);
            }
        }
        self.margin = self.rows[0]
            .laps
            .zip(remaining)
            .map(|(range, need)| range - need);
    }
}

#[derive(Default)]
pub struct FuelEngine {
    track: Option<TrackInfo>,
    identity: Option<(u8, u8, String)>,
    latest: Option<(f64, Option<bool>, u64)>,
    progress: Option<(f64, u64)>,
    previous: Option<Car>,
    completed: Option<(u16, u64)>,
    boundary: Option<(u16, f64)>,
    consumption: VecDeque<f64>,
    in_pit_lane: bool,
    finished: bool,
}
impl FuelEngine {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn reset_history(&mut self) {
        self.suspend();
        self.consumption.clear();
        self.completed = None;
        self.in_pit_lane = false;
        self.finished = false;
    }
    pub fn suspend(&mut self) {
        self.latest = None;
        self.progress = None;
        self.previous = None;
        self.boundary = None;
    }
    pub fn invalidate_lap(&mut self) {
        self.boundary = None;
    }
    pub fn select_driver(&mut self, identity: Option<(u8, u8, String)>) {
        let Some(identity) = identity else {
            self.suspend();
            return;
        };
        if self.identity.as_ref() != Some(&identity) {
            self.reset_history();
            self.identity = Some(identity);
        }
    }
    pub fn set_track(&mut self, track: TrackInfo) {
        if self.track.as_ref() != Some(&track) {
            self.reset_history();
        }
        self.track = Some(track);
    }
    pub fn track_matches(&self, name: &str) -> bool {
        self.track.as_ref().is_some_and(|t| t.track == name)
    }
    pub fn forget_track(&mut self) {
        self.track = None;
        self.reset_history();
    }
    pub fn pit_lane(&mut self, entered: bool) {
        self.in_pit_lane = entered;
        self.invalidate_lap();
    }
    pub fn finish(&mut self) {
        self.finished = true;
        self.invalidate_lap();
    }
    /// Called only after shared dashboard identity and packet-order validation.
    pub fn receive(&mut self, sample: &outgauge::Sample, now: u64, stale_ms: u64) {
        let fuel = f64::from(sample.fuel);
        if !(0.0..=1.0).contains(&fuel) {
            return;
        }
        if self.latest.is_some_and(|(old, _, time)| {
            now.saturating_sub(time) > stale_ms || fuel > old + 0.000_001
        }) {
            self.invalidate_lap();
        }
        self.latest = Some((fuel, sample.lamp(outgauge::FUEL_WARNING), now));
    }
    pub fn update(&mut self, car: &Car, now: u64, stale_ms: u64) {
        let Some(track) = self.track.as_ref().filter(|t| t.supports_gaps()) else {
            self.progress = None;
            self.previous = None;
            self.invalidate_lap();
            return;
        };
        if car.node >= track.nodes || car.info & (4 | 8 | 32) != 0 {
            self.progress = None;
            self.previous = None;
            self.invalidate_lap();
            return;
        }
        let nodes = u32::from(track.nodes);
        let offset_of = |c: &Car| (u32::from(c.node) + nodes - u32::from(track.finish)) % nodes;
        let offset = offset_of(car);
        let mut progress =
            f64::from(car.lap.saturating_sub(1)) + f64::from(offset) / f64::from(nodes);
        let mut interrupted = false;
        if let Some((old, (old_progress, received))) = self.previous.as_ref().zip(self.progress) {
            let distance = (u32::from(car.node) + nodes - u32::from(old.node)) % nodes;
            let old_offset = offset_of(old);
            let lap_change = i32::from(car.lap) - i32::from(old.lap);
            let crossed_finish = old_offset + distance >= nodes;
            let near_finish =
                old_offset.min(nodes - old_offset) <= 2 || offset.min(nodes - offset) <= 2;
            // Like delta tracking, unwrap path movement independently of the
            // timed lap counter: their finish crossings occur in different MCI packets.
            interrupted = now < received
                || now.saturating_sub(received) > stale_ms
                || distance > (nodes / 4).max(1)
                || !matches!(lap_change, 0 | 1)
                || (lap_change == 1 && !crossed_finish && !near_finish);
            if !interrupted {
                progress = old_progress + f64::from(distance) / f64::from(nodes);
            }
        }
        if interrupted {
            self.invalidate_lap();
        }
        self.progress = Some((progress, now));
        self.previous = Some(car.clone());
        if self
            .latest
            .is_none_or(|(_, _, time)| now.saturating_sub(time) > stale_ms)
        {
            self.invalidate_lap();
        }
    }
    pub fn lap_report(&mut self, laps_done: u16, now: u64, stale_ms: u64) {
        if self.completed.is_some_and(|(done, _)| laps_done <= done) {
            return;
        }
        self.completed = Some((laps_done, now));
        if let (Some(track), Some(car), Some((progress, received))) =
            (&self.track, &self.previous, &mut self.progress)
            && track.supports_gaps()
            && now.saturating_sub(*received) <= stale_ms
        {
            let nodes = u32::from(track.nodes);
            let offset = (u32::from(car.node) + nodes - u32::from(track.finish)) % nodes;
            // Anchor to authoritative LapsDone on either side of the nearest
            // finish node, without adding a second lap when MCI catches up.
            let phase = f64::from(offset) / f64::from(nodes);
            *progress = f64::from(laps_done) + if phase > 0.5 { phase - 1.0 } else { phase };
        }
        let sample = self
            .latest
            .filter(|(_, _, time)| now.saturating_sub(*time) <= stale_ms);
        if self.in_pit_lane
            || self.finished
            || self.track.as_ref().is_none_or(|t| !t.supports_gaps())
            || self
                .progress
                .is_none_or(|(_, time)| now.saturating_sub(time) > stale_ms)
        {
            self.invalidate_lap();
            return;
        }
        let Some((fuel, _, _)) = sample else {
            self.invalidate_lap();
            return;
        };
        if let Some((lap, start)) = self.boundary
            && lap.checked_add(1) == Some(laps_done)
        {
            let used = start - fuel;
            if used > 0.000_001 {
                self.consumption.push_back(used);
                if self.consumption.len() > 5 {
                    self.consumption.pop_front();
                }
            }
        }
        self.boundary = Some((laps_done, fuel));
    }
    pub fn frame(&self, now: u64, config: &Config, active: bool) -> FuelFrame {
        let mut frame = FuelFrame {
            car: self.identity.as_ref().map(|(_, _, car)| car.clone()),
            status: "Use a live cockpit or custom view".into(),
            ..Default::default()
        };
        if !active {
            return frame;
        }
        let Some((fuel, low, received)) = self.latest else {
            frame.status = "Waiting for OutGauge".into();
            return frame;
        };
        if now.saturating_sub(received) > config.stale_ms {
            frame.status = "OutGauge stale".into();
            return frame;
        }
        frame.fraction = Some(fuel);
        frame.low_fuel = low;
        let remaining = if self.finished {
            Some(0.0)
        } else {
            self.track
                .as_ref()
                .filter(|t| t.supports_gaps())
                .and_then(|t| race_laps(t.race_laps))
                .zip(
                    self.progress
                        .filter(|(_, time)| now.saturating_sub(*time) <= config.stale_ms),
                )
                .map(|(total, (progress, _))| (f64::from(total) - progress).max(0.0))
        };
        let count = self.consumption.len();
        if count == 0 {
            frame.status = if self.finished {
                "Race finished"
            } else if self.track.as_ref().is_none_or(|t| !t.supports_gaps()) {
                "Standard circuit timing required to learn fuel usage"
            } else if self.boundary.is_some() {
                "Fuel reading anchored — finish another uninterrupted lap"
            } else {
                "Learning fuel usage — complete a full lap between readings"
            }
            .into();
        } else {
            let usage = [
                self.consumption.iter().sum::<f64>() / count as f64,
                self.consumption.iter().copied().fold(0.0, f64::max),
                self.consumption
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min),
            ];
            frame.estimates(usage, remaining);
            frame.status = if self.finished {
                "Race finished"
            } else if self.track.as_ref().is_none_or(|t| !t.supports_gaps()) {
                "Unsupported track timing — finish estimate unavailable"
            } else if self
                .track
                .as_ref()
                .is_some_and(|t| (191..=238).contains(&t.race_laps))
            {
                "Timed race — finish estimate unavailable"
            } else if self
                .track
                .as_ref()
                .is_some_and(|t| race_laps(t.race_laps).is_none())
            {
                "Practice / qualifying — finish estimate unavailable"
            } else if remaining.is_none() {
                "Track progress stale — finish estimate unavailable"
            }
            // ponytail: full personal race distance is conservative for lapped cars;
            // add leader-based finish prediction after validating LFS finish rules.
            else {
                "Estimated for full race distance; conservative when lapped"
            }
            .into();
            if frame.rows.iter().any(|r| r.multiple_stops) {
                frame
                    .status
                    .push_str(" · refuel exceeds tank space: more than one stop needed");
            }
        }
        frame
    }
}

pub fn race_laps(encoded: u8) -> Option<u16> {
    match encoded {
        1..=99 => Some(u16::from(encoded)),
        100..=190 => Some((u16::from(encoded) - 100) * 10 + 100),
        _ => None,
    }
}
