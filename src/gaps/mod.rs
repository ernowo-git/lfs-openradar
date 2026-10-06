//! Estimated race-order gaps from common path-node passage times.
//! Arrival timestamps are approximate; MCI has no physics timestamp.
use crate::lfs::insim::{Car, Player, TrackInfo};
use std::collections::{BTreeMap, VecDeque};

const HISTORY_MS: u64 = 30_000;
const MAX_SAMPLES: usize = 3_100; // Also bounded at the minimum 10 ms interval.
const MAX_POINT_AGE_MS: u64 = 2_000;

#[derive(Clone, Debug, Default)]
pub struct GapValue {
    pub driver: Option<String>,
    pub position: Option<u8>,
    pub seconds: Option<f64>,
    pub laps: Option<u32>,
    pub measured_age_ms: Option<u64>,
    pub status: String,
}
#[derive(Clone, Debug, Default)]
pub struct GapFrame {
    pub ahead: GapValue,
    pub behind: GapValue,
}
impl GapFrame {
    pub fn unavailable(reason: &str) -> Self {
        let value = GapValue {
            status: reason.into(),
            ..Default::default()
        };
        Self {
            ahead: value.clone(),
            behind: value,
        }
    }
}
#[derive(Clone, Copy)]
struct Passage {
    progress: i64,
    time: u64,
}
#[derive(Default)]
struct History {
    passages: VecDeque<Passage>,
    latest: Option<(Car, u64)>,
}
#[derive(Default)]
pub struct GapEngine {
    track: Option<TrackInfo>,
    histories: BTreeMap<u8, History>,
}
impl GapEngine {
    pub fn clear(&mut self) {
        self.histories.clear();
    }
    pub fn set_track(&mut self, track: TrackInfo) {
        if self.track.as_ref() != Some(&track) {
            self.clear();
        }
        self.track = Some(track);
    }
    pub fn forget_track(&mut self) {
        self.clear();
        self.track = None;
    }
    pub fn track_matches(&self, track: &str) -> bool {
        self.track.as_ref().is_some_and(|info| info.track == track)
    }
    pub fn update(&mut self, cars: &[Car], time: u64) {
        let Some(track) = self.track.as_ref().filter(|t| t.supports_gaps()) else {
            return;
        };
        self.histories
            .retain(|id, _| cars.iter().any(|c| c.plid == *id));
        for car in cars {
            let history = self.histories.entry(car.plid).or_default();
            if car.node >= track.nodes || car.info & (4 | 8 | 32) != 0 {
                *history = History::default();
                continue;
            }
            let progress = progress_of(car, track);
            if let Some((old, old_time)) = &history.latest {
                let distance = progress - progress_of(old, track);
                // Never interpolate across silence, backwards driving, or a reset.
                if time <= *old_time
                    || time - old_time > 500
                    || distance < 0
                    || distance > i64::from(track.nodes / 4).max(1)
                    || old.pose.distance(car.pose) > 10.0 + (time - old_time) as f64 * 0.15
                {
                    history.passages.clear();
                } else if distance > 0 {
                    // Record skipped nodes with interpolated passage timestamps.
                    for step in 1..=distance {
                        history.passages.push_back(Passage {
                            progress: progress_of(old, track) + step,
                            time: old_time
                                + ((time - old_time) as f64 * step as f64 / distance as f64) as u64,
                        });
                    }
                }
            }
            // The initial node is deliberately not a known crossing.
            history.latest = Some((car.clone(), time));
            while history
                .passages
                .front()
                .is_some_and(|p| time.saturating_sub(p.time) > HISTORY_MS)
                || history.passages.len() > MAX_SAMPLES
            {
                history.passages.pop_front();
            }
        }
    }
    pub fn frame(
        &self,
        selected: Option<u8>,
        players: &BTreeMap<u8, Player>,
        now: u64,
        stale_ms: u64,
    ) -> GapFrame {
        let Some(track) = &self.track else {
            return GapFrame::unavailable("Waiting for track information");
        };
        if !track.supports_gaps() {
            return GapFrame::unavailable("Unsupported track timing");
        }
        // Practice/qualifying position is a best-lap ranking, not live race order.
        if track.race_laps == 0 || track.race_laps == 255 {
            return GapFrame::unavailable("Race-order gaps require a race");
        }
        let Some(local) = selected.and_then(|id| self.histories.get(&id)) else {
            return GapFrame::unavailable("Waiting for local driver");
        };
        let Some((me, time)) = &local.latest else {
            return GapFrame::unavailable("Unreliable local progress");
        };
        if now.saturating_sub(*time) > stale_ms {
            return GapFrame::unavailable("Telemetry stale");
        }
        if me.position == 0 {
            return GapFrame::unavailable("Race position unknown");
        }
        let current: Vec<_> = self
            .histories
            .values()
            .filter(|h| {
                h.latest
                    .as_ref()
                    .is_some_and(|(c, _)| players.get(&c.plid).is_some_and(|p| !p.in_garage))
            })
            .collect();
        if current
            .iter()
            .filter(|h| h.latest.as_ref().unwrap().0.position == me.position)
            .count()
            != 1
        {
            return GapFrame::unavailable("Ambiguous race order");
        }
        let neighbor = |position: Option<u8>, ahead: bool| {
            let Some(position) = position.filter(|p| *p != 0) else {
                return GapValue {
                    status: "No driver".into(),
                    ..Default::default()
                };
            };
            let matches: Vec<_> = current
                .iter()
                .filter(|h| h.latest.as_ref().unwrap().0.position == position)
                .collect();
            if matches.len() != 1 {
                return GapValue {
                    status: if matches.is_empty() {
                        "No driver"
                    } else {
                        "Ambiguous race order"
                    }
                    .into(),
                    ..Default::default()
                };
            }
            let other = *matches[0];
            let (car, car_time) = other.latest.as_ref().unwrap();
            let mut result = GapValue {
                driver: players.get(&car.plid).map(|p| p.name.clone()),
                position: Some(position),
                status: "Building passage history".into(),
                ..Default::default()
            };
            if now.saturating_sub(*car_time) > stale_ms {
                result.status = "Telemetry stale".into();
                return result;
            }
            let (leader, trailer) = if ahead {
                (other, local)
            } else {
                (local, other)
            };
            let separation = progress_of(&leader.latest.as_ref().unwrap().0, track)
                - progress_of(&trailer.latest.as_ref().unwrap().0, track);
            if separation < 0 {
                result.status = "Race order updating".into();
                return result;
            }
            if separation >= i64::from(track.nodes) {
                result.laps = Some((separation / i64::from(track.nodes)) as u32);
                result.status = "Lap separation".into();
                return result;
            }
            let Some(point) = trailer.passages.back() else {
                return result;
            };
            let age = now.saturating_sub(point.time);
            if age > MAX_POINT_AGE_MS {
                result.status = "Waiting for progress".into();
                return result;
            }
            if let Some(crossing) = leader
                .passages
                .iter()
                .rev()
                .find(|p| p.progress == point.progress)
                && crossing.time <= point.time
            {
                result.seconds = Some((point.time - crossing.time) as f64 / 1000.0);
                result.measured_age_ms = Some(age);
                result.status = "Estimated at common path node".into();
            }
            result
        };
        GapFrame {
            ahead: neighbor(me.position.checked_sub(1), true),
            behind: neighbor(me.position.checked_add(1), false),
        }
    }
}
fn progress_of(car: &Car, track: &TrackInfo) -> i64 {
    i64::from(car.lap) * i64::from(track.nodes)
        + (i64::from(car.node) + i64::from(track.nodes) - i64::from(track.finish))
            % i64::from(track.nodes)
}
