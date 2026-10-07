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
    pub estimated_lap_seconds: Option<f64>,
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
}

#[derive(Default)]
pub struct DeltaEngine {
    track: Option<TrackInfo>,
    driver: Option<(u8, u8, String)>,
    latest: Option<(Car, u64)>,
    current: Option<Lap>,
    completed: Option<Completed>,
    report: Option<(u32, u8, u64)>,
    best: Option<Reference>,
    previous_delta: Option<(f64, u64)>,
    value: DeltaFrame,
}
impl DeltaEngine {
    pub fn reset_lap(&mut self) {
        self.reset_lap_with_reason("recording interrupted");
    }
    pub fn reset_lap_with_reason(&mut self, reason: &str) {
        if self.current.is_some() || self.completed.is_some() {
            timing_log(&format!("lap recording reset: {reason}"));
        }
        self.latest = None;
        self.current = None;
        self.completed = None;
        self.report = None;
        self.previous_delta = None;
        self.value = DeltaFrame::unavailable("Cross the finish line to start recording");
    }
    /// Keep the trusted trace during a brief telemetry interruption. The next
    /// update still has to pass the time, position and progress continuity checks.
    pub fn suspend(&mut self, reason: &str) {
        if self.value.status != reason && self.current.is_some() {
            timing_log(&format!("lap recording suspended: {reason}"));
        }
        self.previous_delta = None;
        self.value = DeltaFrame::unavailable(reason);
    }
    pub fn reset_reference(&mut self) {
        self.reset_lap();
        if self.best.is_some() {
            timing_log("session reference cleared");
        }
        self.best = None;
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
    pub fn select_driver(&mut self, driver: Option<(u8, u8, String)>) {
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
        let measured = completed.lap.points.last().unwrap().time;
        if completed.lap.valid
            && penalty == 0
            && official_ms > 0
            && measured > 0.0
            && (measured - f64::from(official_ms)).abs() <= 1_000.0
            && self
                .best
                .as_ref()
                .is_none_or(|b| f64::from(official_ms) / 1000.0 < b.seconds)
        {
            let mut points = completed.lap.points.clone();
            // Anchor the trace to LFS's reported lap duration rather than accumulate
            // network timing error across successive finish crossings.
            for point in &mut points {
                point.time *= f64::from(official_ms) / measured;
            }
            self.best = Some(Reference {
                points,
                seconds: f64::from(official_ms) / 1000.0,
            });
            timing_log(&format!("session reference recorded: {official_ms} ms"));
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
        let offset_of = |c: &Car| (i64::from(c.node) + nodes - finish) % nodes;
        if car.info & 32 != 0 {
            self.suspend("Track progress delayed");
            return;
        }
        if car.node >= track.nodes || car.info & (4 | 8) != 0 {
            self.reset_lap_with_reason("invalid track progress");
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
            // Multiple MCI sets can arrive in the same TCP read and share its
            // timestamp. They are ordered samples, not a backwards clock.
            let interruption = if time < *old_time {
                Some("arrival clock moved backwards")
            } else if time - old_time > 500 {
                Some("telemetry gap exceeded 500 ms")
            } else if distance > (nodes / 4).max(1) {
                Some("track progress moved backwards or jumped")
            } else if !matches!(lap_change, 0 | 1)
                || (lap_change == 1 && !crossed_finish && !near_finish)
            {
                Some("lap counter changed away from the finish")
            } else if old.pose.distance(car.pose) > 10.0 + (time - old_time) as f64 * 0.15 {
                Some("car position jumped")
            } else {
                None
            };
            if let Some(reason) = interruption {
                self.reset_lap_with_reason(reason);
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
                            && lap.points.len() == nodes as usize + 1
                        {
                            self.completed = Some(Completed { lap, at: time });
                        }
                        self.current = Some(Lap {
                            start: crossing,
                            points: vec![Point { time: 0.0, pose }],
                            valid: true,
                        });
                        timing_log(&format!("lap recording started: lap {}", car.lap));
                    }
                }
            }
        }
        self.latest = Some((car.clone(), time));
        self.confirm_reference();
        self.value = DeltaFrame {
            best_seconds: self.best.as_ref().map(|b| b.seconds),
            status: "Complete a valid recorded lap to set a reference".into(),
            ..Default::default()
        };
        let Some(lap) = &self.current else {
            self.value.status = "Cross the finish line to start recording".into();
            return;
        };
        if !lap.valid {
            self.value.status = "Lap invalid — reference retained".into();
            return;
        }
        let Some(best) = &self.best else {
            return;
        };
        let offset = offset_of(car) as usize;
        let (a, b) = (best.points[offset], best.points[offset + 1]);
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
        let seconds = (time as f64 - lap.start - reference_ms) / 1000.0;
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
        self.value.estimated_lap_seconds = Some(best.seconds + seconds);
        self.value.status = "Estimated vs session best".into();
    }
    pub fn frame(&self, now: u64, stale_ms: u64) -> DeltaFrame {
        if self
            .latest
            .as_ref()
            .is_some_and(|(_, t)| now.saturating_sub(*t) > stale_ms)
        {
            return DeltaFrame {
                best_seconds: self.best.as_ref().map(|b| b.seconds),
                ..DeltaFrame::unavailable("Telemetry stale")
            };
        }
        // Retain the session reference when recording is interrupted.
        DeltaFrame {
            best_seconds: self.best.as_ref().map(|b| b.seconds),
            ..self.value.clone()
        }
    }
}

fn timing_log(message: &str) {
    #[cfg(feature = "desktop")]
    log::info!(target: "openradar_timing", "{message}");
    #[cfg(not(feature = "desktop"))]
    let _ = message;
}
