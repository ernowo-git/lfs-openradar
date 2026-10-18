//! Track-relative traffic and native session context. No OutSim or saved PB dependency.
use crate::{
    gaps::GapEngine,
    lfs::insim::{
        Car, ISS_FRONT_END, ISS_GAME, ISS_PAUSED, ISS_REPLAY, Packet, Player, State, TrackInfo,
    },
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Default)]
pub struct RelativeRow {
    pub plid: u8,
    pub position: Option<u8>,
    pub name: String,
    pub seconds: Option<f64>,
    pub lap_difference: i32,
    pub pit: bool,
    pub fastest: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RelativeFrame {
    /// Fixed slots: three ahead (furthest first), selected driver, three behind.
    pub rows: [Option<RelativeRow>; 7],
    pub session: String,
    pub elapsed_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub current_lap: Option<u16>,
    pub total_laps: Option<u32>,
    pub projected_laps: Option<f64>,
    pub status: String,
}

#[derive(Default)]
pub struct RelativeEngine {
    gaps: GapEngine,
    state: Option<State>,
    track: Option<TrackInfo>,
    pit: BTreeSet<u8>,
    clock: Option<(u64, u64)>,
    pace: BTreeMap<u8, VecDeque<u32>>,
    lap_boundaries: BTreeMap<u8, (u32, u16)>,
    interrupted: BTreeSet<u8>,
    best: Option<(u32, Option<(u8, String)>)>,
    complete_timing: bool,
    result_count: Option<u8>,
    result_indices: BTreeSet<u8>,
    leader: Option<Car>,
    awaiting_start_state: bool,
}

impl RelativeEngine {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    fn reset_session(&mut self) {
        self.gaps.clear();
        self.pit.clear();
        self.clock = None;
        self.pace.clear();
        self.lap_boundaries.clear();
        self.interrupted.clear();
        self.best = None;
        self.complete_timing = false;
        self.result_count = None;
        self.result_indices.clear();
        self.leader = None;
    }
    fn best_lap(&mut self, ms: u32, identity: Option<(u8, String)>) {
        if ms > 0 && self.best.as_ref().is_none_or(|best| ms < best.0) {
            self.best = Some((ms, identity));
        }
    }
    pub fn packet(&mut self, packet: &Packet, now: u64, players: &BTreeMap<u8, Player>) {
        match packet {
            Packet::State(state) => {
                if self.state.as_ref().is_some_and(|old| {
                    old.track != state.track
                        || old.session.kind != state.session.kind
                        || (old.flags ^ state.flags) & (ISS_GAME | ISS_REPLAY | ISS_FRONT_END) != 0
                }) {
                    if !self.awaiting_start_state {
                        self.reset_session();
                    }
                    if self.track.as_ref().is_some_and(|t| t.track != state.track) {
                        self.track = None;
                        self.gaps.forget_track();
                    }
                }
                // Wait for a fresh clock reply after either pause transition.
                if self
                    .state
                    .as_ref()
                    .is_some_and(|old| (old.flags ^ state.flags) & ISS_PAUSED != 0)
                {
                    self.clock = None;
                    self.gaps.clear();
                }
                self.awaiting_start_state = false;
                self.state = Some(state.clone());
            }
            Packet::RaceStart { info, requested } => {
                if !requested {
                    self.reset_session();
                    self.complete_timing = true;
                    self.awaiting_start_state = true;
                    self.clock = Some((0, now));
                }
                self.track = Some(info.clone());
                self.gaps.set_track(info.clone());
            }
            Packet::RaceTime(ms) => self.clock = Some((u64::from(*ms), now)),
            Packet::PitLane { plid, entered } => {
                if *entered {
                    self.pit.insert(*plid);
                    self.interrupted.insert(*plid);
                    self.pace.remove(plid);
                } else {
                    self.pit.remove(plid);
                }
            }
            Packet::PitStop(plid) => {
                self.pit.insert(*plid);
                self.interrupted.insert(*plid);
                self.pace.remove(plid);
            }
            Packet::Pit(plid)
            | Packet::Leave(plid)
            | Packet::Reset(plid)
            | Packet::Takeover(plid) => self.remove(*plid),
            Packet::Player(player) => {
                self.remove(player.plid);
                if player.in_pits {
                    self.pit.insert(player.plid);
                }
            }
            Packet::PlayerSnapshot(player) => {
                if player.in_pits {
                    self.pit.insert(player.plid);
                } else {
                    self.pit.remove(&player.plid);
                }
                if players.get(&player.plid).is_some_and(|old| {
                    old.ucid != player.ucid
                        || old.name != player.name
                        || old.model != player.model
                        || old.in_garage != player.in_garage
                }) {
                    self.remove(player.plid);
                    if player.in_pits {
                        self.pit.insert(player.plid);
                    }
                }
            }
            Packet::ConnectionLeft(ucid) => {
                for player in players.values().filter(|p| p.ucid == *ucid) {
                    self.remove(player.plid);
                }
            }
            Packet::Lap {
                plid,
                time_ms,
                elapsed_ms,
                laps_done,
                ..
            } => {
                let coherent = self
                    .lap_boundaries
                    .get(plid)
                    .is_some_and(|&(elapsed, laps)| {
                        laps.checked_add(1) == Some(*laps_done)
                            && elapsed_ms.wrapping_sub(elapsed) == *time_ms
                    });
                let duplicate = self.lap_boundaries.get(plid) == Some(&(*elapsed_ms, *laps_done));
                if !duplicate {
                    self.lap_boundaries.insert(*plid, (*elapsed_ms, *laps_done));
                    if coherent
                        && !self.pit.contains(plid)
                        && !self.interrupted.remove(plid)
                        && *time_ms > 0
                    {
                        let pace = self.pace.entry(*plid).or_default();
                        pace.push_back(*time_ms);
                        while pace.len() > 5 {
                            pace.pop_front();
                        }
                    } else {
                        self.pace.remove(plid);
                        self.interrupted.remove(plid);
                    }
                    self.best_lap(
                        *time_ms,
                        players.get(plid).map(|p| (p.ucid, p.name.clone())),
                    );
                }
            }
            Packet::Result {
                plid,
                name,
                best_ms,
                index,
                total,
                requested,
            } if self.state.as_ref().is_some_and(|s| s.session.kind == 2) => {
                let identity = players
                    .get(plid)
                    .filter(|p| p.name == *name)
                    .map(|p| (p.ucid, p.name.clone()));
                self.best_lap(*best_ms, identity);
                // Results recover a complete qualifying table, not an ongoing race's lap history.
                if *requested
                    && *total > 0
                    && *index < *total
                    && self.state.as_ref().is_some_and(|s| s.session.kind == 2)
                {
                    if self.result_count != Some(*total) {
                        self.result_indices.clear();
                        self.result_count = Some(*total);
                    }
                    self.result_indices.insert(*index);
                    if self.result_indices.len() == usize::from(*total) {
                        self.complete_timing = true;
                    }
                }
            }
            Packet::Session => self.reset_session(),
            Packet::LayoutChanged => {
                self.reset_session();
                self.track = None;
                self.gaps.forget_track();
            }
            _ => {}
        }
    }
    fn remove(&mut self, plid: u8) {
        self.gaps.remove_driver(plid);
        self.pit.remove(&plid);
        self.pace.remove(&plid);
        self.lap_boundaries.remove(&plid);
        self.interrupted.insert(plid);
        if self.leader.as_ref().is_some_and(|c| c.plid == plid) {
            self.leader = None;
        }
    }
    pub fn update(&mut self, cars: &[Car], now: u64) {
        self.gaps.update(cars, now);
        self.leader = cars
            .iter()
            .find(|c| c.position == 1 && c.info & (4 | 8 | 32) == 0)
            .cloned();
        if cars.iter().filter(|c| c.position == 1).count() != 1 {
            self.leader = None;
        }
    }
    pub fn frame(
        &self,
        selected: Option<u8>,
        players: &BTreeMap<u8, Player>,
        now: u64,
        stale_ms: u64,
    ) -> RelativeFrame {
        let mut frame = RelativeFrame::default();
        let Some(state) = &self.state else {
            frame.status = "Waiting for LFS session".into();
            return frame;
        };
        frame.session = match state.session.kind {
            1 => "RACE",
            2 => "QUALIFY",
            _ => "PRACTICE",
        }
        .into();
        if state.flags & ISS_GAME == 0 || state.flags & (ISS_REPLAY | ISS_FRONT_END) != 0 {
            frame.status = "Live session required".into();
            return frame;
        }
        let racing = state.session.kind == 1;
        let race_laps = state.session.race_laps;
        if racing {
            frame.total_laps = match race_laps {
                1..=99 => Some(u32::from(race_laps)),
                100..=190 => Some(u32::from(race_laps - 100) * 10 + 100),
                _ => None,
            };
            if (191..=238).contains(&race_laps) {
                frame.duration_ms = Some(u64::from(race_laps - 190) * 3_600_000);
            }
        } else if state.session.kind == 2 && state.session.qual_mins > 0 {
            frame.duration_ms = Some(u64::from(state.session.qual_mins) * 60_000);
        }
        frame.elapsed_ms = self
            .clock
            .filter(|(_, at)| now.saturating_sub(*at) <= 2500)
            .map(|(elapsed, at)| {
                elapsed
                    + if state.flags & ISS_PAUSED != 0 {
                        0
                    } else {
                        now.saturating_sub(at)
                    }
            });
        if state.flags & ISS_PAUSED != 0 {
            frame.status = "LFS paused".into();
            return frame;
        }
        if self
            .track
            .as_ref()
            .is_none_or(|t| t.track != state.track || !t.supports_gaps())
        {
            frame.status = "Track path unavailable for relative order".into();
            return frame;
        }
        frame.rows = self
            .gaps
            .on_track(selected, players, &self.pit, now, stale_ms);
        if let Some(me) = &frame.rows[3] {
            frame.current_lap = self.gaps.current_lap(me.plid);
            frame.status = "Estimated on-track gaps".into();
        } else {
            frame.status = "Waiting for reliable driver and track progress".into();
        }
        for row in frame.rows.iter_mut().flatten() {
            if !racing {
                row.lap_difference = 0;
            }
            row.fastest = self.complete_timing
                && self
                    .best
                    .as_ref()
                    .and_then(|b| b.1.as_ref())
                    .is_some_and(|(ucid, name)| {
                        players
                            .get(&row.plid)
                            .is_some_and(|p| p.ucid == *ucid && p.name == *name)
                            && players
                                .values()
                                .filter(|p| p.ucid == *ucid && p.name == *name)
                                .count()
                                == 1
                    });
        }
        if frame.rows[3].is_some()
            && racing
            && let (Some(duration), Some(elapsed), Some(leader), Some(track)) = (
                frame.duration_ms,
                frame.elapsed_ms,
                &self.leader,
                &self.track,
            )
            && elapsed < duration
            && !self.pit.contains(&leader.plid)
            && let Some(pace) = self.pace.get(&leader.plid).filter(|p| p.len() >= 2)
        {
            let mut times: Vec<_> = pace.iter().copied().collect();
            times.sort_unstable();
            let median = f64::from(times[times.len() / 2]);
            let fraction = f64::from(
                (u32::from(leader.node) + u32::from(track.nodes) - u32::from(track.finish))
                    % u32::from(track.nodes),
            ) / f64::from(track.nodes);
            // Predict the leader's distance at time expiry, not an official finish lap.
            frame.projected_laps = Some(
                f64::from(leader.lap.saturating_sub(1))
                    + fraction
                    + duration.saturating_sub(elapsed) as f64 / median,
            );
        }
        frame
    }
}
