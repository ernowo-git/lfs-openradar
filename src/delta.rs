//! Session-best lap comparison at matching track progress, independent of rendering.
//! Node crossings use arrival timestamps; spatial interpolation smooths the display
//! between nodes. This is an estimate, not LFS's physics clock.
use crate::{
    lfs::insim::{Car, TrackInfo},
    radar::Pose,
};

#[derive(Clone, Debug, Default)]
pub struct DeltaFrame {
    pub seconds: Option<f64>,
    /// Change in delta per second: negative means gaining time.
    pub trend: Option<f64>,
    pub best_seconds: Option<f64>,
    /// Duration from sector 1 to the finish; never presented as a full lap best.
    pub sector_reference_seconds: Option<f64>,
    pub estimated_lap_seconds: Option<f64>,
    pub since_sector1: bool,
    pub status: String,
}
impl DeltaFrame {
    pub fn unavailable(reason: &str) -> Self {
        Self {
            status: reason.into(),
            ..Default::default()
        }
    }
}
#[derive(Clone, Copy)]
struct Point {
    time: f64,
    pose: Pose,
}
struct Lap {
    start: f64,
    offset: usize,
    split: Option<SplitAnchor>,
    points: Vec<Point>,
    valid: bool,
}
struct Completed {
    lap: Lap,
    at: u64,
}
struct Reference {
    points: Vec<Point>,
    seconds: f64,
    offset: usize,
}
#[derive(Clone, Copy)]
struct SplitReport {
    time_ms: u32,
    penalty: u8,
    at: u64,
}
struct SplitAnchor {
    time: f64,
    at: u64,
    offset: usize,
    report: Option<SplitReport>,
}

#[derive(Default)]
pub struct DeltaEngine {
    track: Option<TrackInfo>,
    driver: Option<(u8, String)>,
    latest: Option<(Car, u64)>,
    current: Option<Lap>,
    completed: Option<Completed>,
    report: Option<(u32, u8, u64)>,
    best: Option<Reference>,
    sector_reference: Option<Reference>,
    split_report: Option<SplitReport>,
    unrecorded_invalid: bool,
    previous_delta: Option<(f64, u64)>,
    value: DeltaFrame,
}
impl DeltaEngine {
    pub fn reset_lap(&mut self) {
        self.latest = None;
        self.current = None;
        self.completed = None;
        self.report = None;
        self.split_report = None;
        self.unrecorded_invalid = false;
        self.previous_delta = None;
        self.value = DeltaFrame::unavailable("Cross the finish line to start recording");
    }
    pub fn reset_reference(&mut self) {
        self.reset_lap();
        self.best = None;
        self.sector_reference = None;
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn set_track(&mut self, info: TrackInfo) {
        if self.track.as_ref() != Some(&info) {
            self.reset_reference();
        }
        self.track = Some(info);
    }
    pub fn track_matches(&self, track: &str) -> bool {
        self.track.as_ref().is_some_and(|t| t.track == track)
    }
    pub fn select_driver(&mut self, driver: Option<(u8, String)>) {
        if let Some(driver) = driver {
            if self.driver.as_ref() != Some(&driver) {
                self.reset_reference();
                self.driver = Some(driver);
            }
        } else {
            self.reset_lap();
        }
    }
    pub fn invalidate(&mut self) {
        self.unrecorded_invalid = true;
        if let Some(lap) = &mut self.current {
            lap.valid = false;
        }
        if let Some(lap) = &mut self.completed {
            lap.lap.valid = false;
        }
    }
    pub fn lap_report(&mut self, time_ms: u32, penalty: u8, at: u64) {
        self.report = Some((time_ms, penalty, at));
        self.confirm_reference();
    }
    pub fn split_report(&mut self, time_ms: u32, penalty: u8, at: u64) {
        self.split_report = Some(SplitReport {
            time_ms,
            penalty,
            at,
        });
        self.match_split_report();
        self.confirm_reference();
    }
    fn match_split_report(&mut self) {
        let Some(report) = self.split_report else {
            return;
        };
        // SPX can arrive on either side of the corresponding MCI node crossing.
        for lap in self
            .current
            .iter_mut()
            .chain(self.completed.iter_mut().map(|c| &mut c.lap))
        {
            if let Some(split) = &mut lap.split
                && split.at.abs_diff(report.at) <= 1_000
            {
                split.report = Some(report);
                lap.valid &= report.penalty == 0 && report.time_ms > 0;
                self.split_report = None;
                break;
            }
        }
    }
    fn confirm_reference(&mut self) {
        let (Some(completed), Some((official_ms, penalty, report_at))) =
            (&self.completed, self.report)
        else {
            return;
        };
        // LAP may precede or follow the MCI finish crossing. Match only nearby
        // events with a consistent duration; incomplete/out laps cannot qualify.
        if completed.at.abs_diff(report_at) > 1_000 {
            return;
        }
        let full_measured = completed.lap.points.last().unwrap().time;
        let full_lap =
            completed.lap.offset == 0 && (full_measured - f64::from(official_ms)).abs() <= 1_000.0;
        let (offset, duration_ms) = if full_lap {
            (0, official_ms)
        } else {
            let Some(anchor) = completed.lap.split.as_ref() else {
                return;
            };
            let Some(split) = anchor.report else {
                return;
            };
            (anchor.offset, official_ms.saturating_sub(split.time_ms))
        };
        // A race opening may have a different start origin from a flying lap.
        // Even if a finish crossing was observed, its post-S1 trace can qualify.
        let mut points = completed.lap.points[offset - completed.lap.offset..].to_vec();
        let origin = points[0].time;
        let measured = points.last().unwrap().time - origin;
        let reference = if full_lap {
            &mut self.best
        } else {
            &mut self.sector_reference
        };
        if completed.lap.valid
            && penalty == 0
            && duration_ms > 0
            && measured > 0.0
            && (measured - f64::from(duration_ms)).abs() <= 1_000.0
            && reference
                .as_ref()
                .is_none_or(|b| f64::from(duration_ms) / 1000.0 < b.seconds)
        {
            // Anchor the trace to LFS's reported lap duration rather than accumulate
            // network timing error across successive finish crossings.
            for point in &mut points {
                point.time = (point.time - origin) * f64::from(duration_ms) / measured;
            }
            *reference = Some(Reference {
                points,
                seconds: f64::from(duration_ms) / 1000.0,
                offset,
            });
            if full_lap {
                self.sector_reference = None;
            }
            self.previous_delta = None;
        }
        self.completed = None;
        self.report = None;
    }
    pub fn update(&mut self, car: &Car, time: u64) {
        let Some(track) = self.track.as_ref().filter(|t| t.supports_gaps()) else {
            self.value = DeltaFrame::unavailable("Standard track timing required");
            return;
        };
        let nodes = i64::from(track.nodes);
        let finish = i64::from(track.finish);
        let split_offset = (track.race_laps > 0
            && track.timing & 3 != 0
            && track.split1 < track.nodes
            && track.split1 != track.finish)
            .then(|| (i64::from(track.split1) + nodes - finish) % nodes);
        let offset_of = |c: &Car| (i64::from(c.node) + nodes - finish) % nodes;
        if car.node >= track.nodes || car.info & (4 | 8 | 32) != 0 {
            self.reset_lap();
            self.value.status = "Unreliable track progress".into();
            return;
        }
        if let Some((old, old_time)) = &self.latest {
            // The nearest path node and the actual timed finish crossing do
            // not necessarily advance in the same MCI update. Track the local
            // trace through node wrap independently of the race lap counter.
            let distance = (i64::from(car.node) + nodes - i64::from(old.node)) % nodes;
            let old_offset = offset_of(old);
            let new_offset = offset_of(car);
            let lap_change = i32::from(car.lap) - i32::from(old.lap);
            let crossed_finish = old_offset + distance >= nodes;
            let near_finish =
                old_offset.min(nodes - old_offset) <= 2 || new_offset.min(nodes - new_offset) <= 2;
            if time <= *old_time
                || time - old_time > 500
                || distance > (nodes / 4).max(1)
                || !matches!(lap_change, 0 | 1)
                || (lap_change == 1 && !crossed_finish && !near_finish)
                || old.pose.distance(car.pose) > 10.0 + (time - old_time) as f64 * 0.15
            {
                self.reset_lap();
            } else {
                for step in 1..=distance {
                    let fraction = step as f64 / distance as f64;
                    let crossing = *old_time as f64 + (time - old_time) as f64 * fraction;
                    let pose = Pose {
                        x: old.pose.x + (car.pose.x - old.pose.x) * fraction,
                        y: old.pose.y + (car.pose.y - old.pose.y) * fraction,
                        z: old.pose.z + (car.pose.z - old.pose.z) * fraction,
                        heading: car.pose.heading,
                    };
                    let offset = (old_offset + step) % nodes;
                    if let Some(lap) = &mut self.current {
                        lap.points.push(Point {
                            time: crossing - lap.start,
                            pose,
                        });
                    }
                    if offset == 0 {
                        if let Some(lap) = self.current.take()
                            && lap.points.len() == nodes as usize - lap.offset + 1
                        {
                            self.completed = Some(Completed { lap, at: time });
                        }
                        self.current = Some(Lap {
                            start: crossing,
                            offset: 0,
                            split: None,
                            points: vec![Point { time: 0.0, pose }],
                            valid: true,
                        });
                        self.unrecorded_invalid = false;
                    }
                    if Some(offset) == split_offset {
                        if self.current.is_none() && self.best.is_none() {
                            self.current = Some(Lap {
                                start: crossing,
                                offset: offset as usize,
                                split: None,
                                points: vec![Point { time: 0.0, pose }],
                                valid: !self.unrecorded_invalid,
                            });
                        }
                        if let Some(lap) = &mut self.current {
                            lap.split = Some(SplitAnchor {
                                time: crossing,
                                at: time,
                                offset: offset as usize,
                                report: None,
                            });
                        }
                        // A new comparison starts at this checkpoint in partial mode.
                        if self.best.is_none() {
                            self.previous_delta = None;
                        }
                    }
                }
            }
        }
        self.latest = Some((car.clone(), time));
        self.match_split_report();
        self.confirm_reference();
        self.value = DeltaFrame {
            best_seconds: self.best.as_ref().map(|b| b.seconds),
            sector_reference_seconds: self.sector_reference.as_ref().map(|b| b.seconds),
            status: "Complete a valid recorded lap to set a reference".into(),
            ..Default::default()
        };
        let Some(lap) = &self.current else {
            self.value.status = if split_offset.is_some() && self.best.is_none() {
                "Cross sector 1 to start recording"
            } else {
                "Cross the finish line to start recording"
            }
            .into();
            return;
        };
        if !lap.valid {
            self.value.status = "Lap invalid — reference retained".into();
            return;
        }
        let offset = offset_of(car) as usize;
        let (best, start, first_sector_ms) = if let Some(best) = &self.best {
            (best, lap.start, None)
        } else if let Some(best) = &self.sector_reference {
            self.value.since_sector1 = true;
            let Some(split) = &lap.split else {
                self.value.status = "Cross sector 1 to start delta".into();
                return;
            };
            if offset < best.offset {
                self.value.status = "Cross sector 1 to start delta".into();
                return;
            }
            let Some(report) = split.report else {
                self.value.status = "Waiting for sector 1 timing".into();
                return;
            };
            (best, split.time, Some(report.time_ms))
        } else {
            return;
        };
        let index = offset - best.offset;
        let (a, b) = (best.points[index], best.points[index + 1]);
        let dx = b.pose.x - a.pose.x;
        let dy = b.pose.y - a.pose.y;
        let dz = b.pose.z - a.pose.z;
        let length2 = dx * dx + dy * dy + dz * dz;
        if length2 < 0.01 {
            self.value.status = "Reference progress unavailable".into();
            return;
        }
        let fraction = (((car.pose.x - a.pose.x) * dx
            + (car.pose.y - a.pose.y) * dy
            + (car.pose.z - a.pose.z) * dz)
            / length2)
            .clamp(0.0, 1.0);
        let reference_ms = a.time + (b.time - a.time) * fraction;
        let seconds = (time as f64 - start - reference_ms) / 1000.0;
        let trend = self.previous_delta.and_then(|(old, at)| {
            (time > at).then(|| (seconds - old) * 1000.0 / (time - at) as f64)
        });
        // Keep a half-second trend window to avoid noisy per-packet indicators.
        if self
            .previous_delta
            .is_none_or(|(_, at)| time.saturating_sub(at) >= 500)
        {
            self.previous_delta = Some((seconds, time));
        }
        self.value.seconds = Some(seconds);
        self.value.trend = trend;
        self.value.estimated_lap_seconds =
            Some(best.seconds + seconds + first_sector_ms.map_or(0.0, |ms| f64::from(ms) / 1000.0));
        self.value.status = if first_sector_ms.is_some() {
            "Delta since sector 1"
        } else {
            "Estimated vs session best"
        }
        .into();
    }
    pub fn frame(&self, now: u64, stale_ms: u64) -> DeltaFrame {
        if self
            .latest
            .as_ref()
            .is_some_and(|(_, t)| now.saturating_sub(*t) > stale_ms)
        {
            return DeltaFrame {
                best_seconds: self.best.as_ref().map(|b| b.seconds),
                sector_reference_seconds: self.sector_reference.as_ref().map(|b| b.seconds),
                ..DeltaFrame::unavailable("Telemetry stale")
            };
        }
        // Pausing, changing view, or losing telemetry resets the current lap,
        // but the recorded session reference remains available for display.
        DeltaFrame {
            best_seconds: self.best.as_ref().map(|b| b.seconds),
            sector_reference_seconds: self.sector_reference.as_ref().map(|b| b.seconds),
            ..self.value.clone()
        }
    }
}
