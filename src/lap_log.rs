//! Session-only completed laps. Only the latest eight rows enter render snapshots.
use std::sync::{Arc, Mutex};

pub const VISIBLE_ROWS: usize = 8;
pub type SharedLapLog = Arc<Mutex<LapLog>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lap {
    pub number: u16,
    pub time_ms: u32,
    pub delta_ms: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct LapLogFrame {
    pub best_ms: Option<u32>,
    /// Newest completed lap first.
    pub laps: Vec<Lap>,
}

#[derive(Default)]
pub struct LapLog {
    track: Option<String>,
    car: Option<String>,
    laps: Vec<Lap>,
    best_ms: Option<u32>,
    boundary: Option<(u32, u16)>,
    compare_previous: bool,
    verify_session: bool,
}

impl LapLog {
    pub fn clear(&mut self) {
        self.laps.clear();
        self.best_ms = None;
        self.boundary = None;
        self.verify_session = false;
        self.break_continuity();
    }

    pub(crate) fn track(&mut self, track: &str) {
        if !track.is_empty() && self.track.as_deref() != Some(track) {
            self.clear();
            self.track = Some(track.into());
        }
    }

    pub(crate) fn car(&mut self, car: &str) {
        if self.car.as_deref() != Some(car) {
            self.clear();
            self.car = Some(car.into());
        }
    }

    pub(crate) fn break_continuity(&mut self) {
        self.compare_previous = false;
    }

    pub(crate) fn disconnected(&mut self) {
        self.break_continuity();
        self.verify_session = true;
    }

    /// Recognise a session clock/lap counter restart missed while disconnected.
    pub(crate) fn boundary(&mut self, elapsed_ms: u32, number: u16) {
        if self.verify_session
            && let Some((elapsed, lap)) = self.boundary
            && number <= lap
            && elapsed_ms < elapsed
            && elapsed.wrapping_sub(elapsed_ms) < u32::MAX / 2
        {
            self.clear();
        }
        self.verify_session = false;
        self.boundary = Some((elapsed_ms, number));
    }

    pub(crate) fn record(&mut self, number: u16, time_ms: u32) {
        let previous = self.laps.last().filter(|_| self.compare_previous);
        self.laps.push(Lap {
            number,
            time_ms,
            delta_ms: previous.map(|lap| i64::from(time_ms) - i64::from(lap.time_ms)),
        });
        self.best_ms = Some(self.best_ms.map_or(time_ms, |best| best.min(time_ms)));
        self.compare_previous = true;
    }

    pub fn frame(&self) -> LapLogFrame {
        LapLogFrame {
            best_ms: self.best_ms,
            laps: self.laps.iter().rev().take(VISIBLE_ROWS).cloned().collect(),
        }
    }

    pub fn laps(&self) -> &[Lap] {
        &self.laps
    }
}
