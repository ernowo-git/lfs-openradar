//! Personal timing records from complete InSim lap/checkpoint sequences.
use crate::lfs::insim::{
    ISS_FRONT_END, ISS_GAME, ISS_MULTI, ISS_REPLAY, Packet, Player, State, TrackInfo,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Records = BTreeMap<String, BTreeMap<String, Record>>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersonalBest {
    pub checkpoints_ms: Vec<u32>,
    pub lap_ms: u32,
}

impl PersonalBest {
    fn sectors(&self) -> Option<Vec<u32>> {
        let mut previous = 0;
        self.checkpoints_ms
            .iter()
            .copied()
            .chain([self.lap_ms])
            .map(|time| {
                let duration = time.checked_sub(previous).filter(|duration| *duration > 0);
                previous = time;
                duration
            })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub checkpoint_count: u8,
    pub best_sectors_ms: Vec<u32>,
    pub pb: PersonalBest,
}

impl Record {
    pub fn validate(&self) -> Result<(), String> {
        let sectors = self
            .pb
            .sectors()
            .ok_or("lap records require increasing positive times")?;
        if self.checkpoint_count > 3
            || self.pb.checkpoints_ms.len() != usize::from(self.checkpoint_count)
            || self.best_sectors_ms.len() != sectors.len()
            || self
                .best_sectors_ms
                .iter()
                .zip(sectors)
                .any(|(&best, pb)| best == 0 || best > pb)
        {
            return Err("lap record sector count or best times are inconsistent".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct LapRecordsFrame {
    pub track: Option<String>,
    pub car: Option<String>,
    pub checkpoint_count: u8,
    pub record: Option<Record>,
    pub status: String,
}

#[derive(Default)]
pub struct Recorder {
    records: Records,
    pub lap_log: crate::lap_log::SharedLapLog,
    changed: bool,
    // Keep timing context across camera changes that suspend the radar's selector.
    state: Option<State>,
    track: Option<TrackInfo>,
    driver: Option<(u8, u8, String)>,
    boundary: Option<(u32, u16)>,
    checkpoints: Vec<u32>,
    broken: bool,
    session_start: bool,
}

impl Recorder {
    pub fn with_records(records: Records) -> Self {
        Self {
            records,
            ..Self::default()
        }
    }

    pub fn records(&self) -> &Records {
        &self.records
    }

    pub fn take_changes(&mut self) -> Option<Records> {
        std::mem::take(&mut self.changed).then(|| self.records.clone())
    }

    fn reset_lap(&mut self) {
        self.lap_log.lock().unwrap().break_continuity();
        self.session_start = false;
        self.boundary = None;
        self.checkpoints.clear();
        self.broken = false;
    }

    pub fn disconnect(&mut self) {
        self.reset_lap();
        self.lap_log.lock().unwrap().disconnected();
        self.state = None;
        self.track = None;
        self.driver = None;
    }

    pub fn select_driver<'a>(&mut self, players: impl Iterator<Item = &'a Player>) {
        let mut humans = players.filter(|p| p.plid != 0 && p.local_human());
        let first = humans.next();
        let driver = first
            .filter(|_| humans.next().is_none())
            .map(|p| (p.plid, p.ucid, p.model.clone()));
        if self.driver != driver {
            self.reset_lap();
            if let Some((_, _, car)) = &driver {
                self.lap_log.lock().unwrap().car(car);
            }
            self.driver = driver;
        }
    }

    fn timing(&self) -> Option<&TrackInfo> {
        let state = self.state.as_ref()?;
        (state.flags & ISS_GAME != 0 && state.flags & (ISS_REPLAY | ISS_FRONT_END) == 0)
            .then_some(())?;
        self.driver.as_ref()?;
        self.track.as_ref().filter(|track| {
            !track.track.is_empty()
                && track.track == state.track
                && track.timing & 0xc0 == 0x40
                && track.race_laps != 255
        })
    }

    pub fn packet(&mut self, packet: &Packet) {
        match packet {
            Packet::State(state) => {
                self.lap_log.lock().unwrap().track(&state.track);
                if self.state.as_ref().is_none_or(|old| {
                    old.track != state.track
                        || (old.flags ^ state.flags)
                            & (ISS_GAME | ISS_REPLAY | ISS_FRONT_END | ISS_MULTI)
                            != 0
                }) {
                    self.reset_lap();
                }
                self.state = Some(state.clone());
            }
            Packet::RaceStart { info, requested } => {
                if !requested || self.track.as_ref().is_some_and(|old| old != info) {
                    self.lap_log.lock().unwrap().clear();
                }
                self.lap_log.lock().unwrap().track(&info.track);
                if !requested || self.track.as_ref() != Some(info) {
                    self.reset_lap();
                }
                if !requested {
                    self.session_start = true;
                }
                self.track = Some(info.clone());
            }
            Packet::LayoutChanged => {
                self.lap_log.lock().unwrap().clear();
                self.reset_lap();
                self.track = None;
            }
            Packet::Session | Packet::Tiny(9 | 11 | 12) => {
                self.lap_log.lock().unwrap().clear();
                self.reset_lap();
            }
            Packet::Player(p) if self.driver.as_ref().is_some_and(|d| d.0 == p.plid) => {
                self.reset_lap()
            }
            Packet::Reset(id) | Packet::Pit(id) | Packet::Leave(id) | Packet::Takeover(id)
                if self.driver.as_ref().is_some_and(|d| d.0 == *id) =>
            {
                self.reset_lap()
            }
            Packet::ConnectionLeft(id) if self.driver.as_ref().is_some_and(|d| d.1 == *id) => {
                self.reset_lap()
            }
            Packet::Split {
                plid,
                time_ms,
                elapsed_ms,
                split,
                ..
            } if self.driver.as_ref().is_some_and(|d| d.0 == *plid) => {
                let Some(track) = self.timing() else {
                    self.reset_lap();
                    return;
                };
                let count = track.timing & 3;
                let Some((start, _)) = self.boundary.or(self.session_start.then_some((0, 0)))
                else {
                    return;
                };
                let index = usize::from(*split).wrapping_sub(1);
                // TCP can replay a duplicate event; only identical checkpoints are harmless.
                if index < self.checkpoints.len()
                    && self.checkpoints[index] == *time_ms
                    && elapsed_ms.wrapping_sub(start) == *time_ms
                {
                    return;
                }
                if *split == 0
                    || *split > count
                    || index != self.checkpoints.len()
                    || *time_ms <= self.checkpoints.last().copied().unwrap_or(0)
                    || elapsed_ms.wrapping_sub(start) != *time_ms
                {
                    self.broken = true;
                } else {
                    self.checkpoints.push(*time_ms);
                }
            }
            Packet::Lap {
                plid,
                time_ms,
                elapsed_ms,
                laps_done,
                ..
            } if self.driver.as_ref().is_some_and(|d| d.0 == *plid) => {
                let Some(track) = self.timing() else {
                    self.reset_lap();
                    return;
                };
                let count = track.timing & 3;
                let track = track.track.clone();
                if self.boundary == Some((*elapsed_ms, *laps_done)) {
                    return;
                }
                self.lap_log
                    .lock()
                    .unwrap()
                    .boundary(*elapsed_ms, *laps_done);
                let complete = self.boundary.is_some_and(|(start, laps)| {
                    laps.wrapping_add(1) == *laps_done && elapsed_ms.wrapping_sub(start) == *time_ms
                });
                let pb = PersonalBest {
                    checkpoints_ms: self.checkpoints.clone(),
                    lap_ms: *time_ms,
                };
                let sectors = pb.sectors();
                let coherent = !self.broken
                    && self.checkpoints.len() == usize::from(count)
                    && sectors.is_some();
                // A witnessed race start also establishes a boundary for the log's first lap.
                let log_complete =
                    complete || (self.session_start && *laps_done == 1 && elapsed_ms == time_ms);
                if coherent && log_complete {
                    self.lap_log.lock().unwrap().record(*laps_done, *time_ms);
                } else {
                    self.lap_log.lock().unwrap().break_continuity();
                }
                if coherent
                    && complete
                    && let Some(sectors) = sectors
                {
                    let car = self.driver.as_ref().unwrap().2.clone();
                    let entry = self.records.entry(track).or_default().entry(car);
                    match entry {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(Record {
                                checkpoint_count: count,
                                best_sectors_ms: sectors,
                                pb,
                            });
                            self.changed = true;
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            let record = entry.get_mut();
                            // A different checkpoint layout must never overwrite incompatible records.
                            if record.checkpoint_count == count {
                                for (best, sector) in record.best_sectors_ms.iter_mut().zip(sectors)
                                {
                                    if sector < *best {
                                        *best = sector;
                                        self.changed = true;
                                    }
                                }
                                if pb.lap_ms < record.pb.lap_ms {
                                    record.pb = pb;
                                    self.changed = true;
                                }
                            }
                        }
                    }
                }
                self.session_start = false;
                self.boundary = Some((*elapsed_ms, *laps_done));
                self.checkpoints.clear();
                self.broken = false;
            }
            // HLV and penalty flags deliberately do not disqualify personal timing records.
            _ => {}
        }
    }

    pub fn frame(&self) -> LapRecordsFrame {
        let track = self
            .state
            .as_ref()
            .map(|state| state.track.clone())
            .filter(|t| !t.is_empty());
        let car = self.driver.as_ref().map(|driver| driver.2.clone());
        let record = track
            .as_ref()
            .zip(car.as_ref())
            .and_then(|(track, car)| self.records.get(track)?.get(car))
            .cloned();
        let timing = self.timing();
        let count = timing
            .map(|t| t.timing & 3)
            .or_else(|| record.as_ref().map(|r| r.checkpoint_count))
            .unwrap_or(2);
        let status = if timing.is_none() {
            "Waiting for a local human car and standard track timing"
        } else if record.as_ref().is_some_and(|r| r.checkpoint_count != count) {
            "Checkpoint layout differs from saved records"
        } else if self.boundary.is_none() {
            "Cross the finish line to start recording"
        } else if self.broken {
            "Incomplete timing sequence — waiting for the next lap"
        } else {
            "Recording complete laps · invalid-lap flags ignored"
        };
        LapRecordsFrame {
            track,
            car,
            checkpoint_count: count,
            record,
            status: status.into(),
        }
    }
}
