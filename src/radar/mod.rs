//! Timestamped telemetry fusion and renderer-independent radar geometry.
use crate::{
    config::Config,
    gaps::{GapEngine, GapFrame},
    lfs::{
        insim::{
            Car, ISS_FRONT_END, ISS_GAME, ISS_MULTI, ISS_PAUSED, ISS_REPLAY, ISS_SHIFTU,
            MciAssembler, Packet, Player, State,
        },
        outsim::Sample,
    },
};
use std::{
    collections::{BTreeMap, VecDeque},
    f64::consts::{PI, TAU},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub heading: f64,
}
impl Pose {
    pub fn distance(self, other: Self) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2) + (self.z - other.z).powi(2))
            .sqrt()
    }
    fn interpolate(self, other: Self, amount: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * amount,
            y: self.y + (other.y - self.y) * amount,
            z: self.z + (other.z - self.z) * amount,
            heading: self.heading + wrap_angle(other.heading - self.heading) * amount,
        }
    }
}
pub fn wrap_angle(angle: f64) -> f64 {
    (angle + PI).rem_euclid(TAU) - PI
}
pub fn local_offset(me: Pose, other: Pose) -> [f64; 2] {
    let (sin, cos) = me.heading.sin_cos();
    let (dx, dy) = (other.x - me.x, other.y - me.y);
    [dx * cos + dy * sin, -dx * sin + dy * cos]
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Threat {
    Nearby,
    Alongside,
    PotentialContact,
}
#[derive(Clone, Debug)]
pub struct RadarCar {
    pub plid: u8,
    pub name: String,
    pub right: f64,
    pub forward: f64,
    pub relative_heading: f64,
    pub threat: Threat,
    pub uncertain: bool,
}
#[derive(Clone, Debug, Default)]
pub struct RadarFrame {
    pub status: String,
    pub live: bool,
    pub uncertain: bool,
    pub cars: Vec<RadarCar>,
    pub driver: Option<String>,
    pub mci_age_ms: Option<u64>,
    pub outsim_age_ms: Option<u64>,
}
#[derive(Clone)]
struct Snapshot {
    time: u64,
    cars: Vec<Car>,
}
#[derive(Clone, Copy)]
struct TimedPose {
    time: u64,
    pose: Pose,
}
#[derive(Default)]
pub struct Engine {
    follow_viewed_car: bool,
    delta: crate::delta::DeltaEngine,
    gaps: GapEngine,
    players: BTreeMap<u8, Player>,
    state: Option<State>,
    selected: Option<u8>,
    /// Retained while paused or in another camera to protect the reference lifetime.
    reference_target: Option<u8>,
    assembler: MciAssembler,
    snapshots: VecDeque<Snapshot>,
    poses: VecDeque<TimedPose>,
    outsim_clock: Option<u32>,
    delta_association: Option<TimedPose>,
    association_stale_ms: u64,
    previous_threats: BTreeMap<u8, Threat>,
    pub mci_sets: u64,
    pub outsim_samples: u64,
    pub rejected_outsim: u64,
}
impl Engine {
    pub fn new(follow_viewed_car: bool) -> Self {
        Self {
            follow_viewed_car,
            ..Self::default()
        }
    }
    pub fn clear(&mut self) {
        self.delta.clear();
        self.delta_association = None;
        self.gaps.forget_track();
        self.players.clear();
        self.state = None;
        self.selected = None;
        self.reference_target = None;
        self.clear_histories();
    }
    fn clear_histories(&mut self) {
        self.delta_association = None;
        self.gaps.clear();
        self.clear_radar_histories();
    }
    fn clear_radar_histories(&mut self) {
        self.assembler.clear();
        self.snapshots.clear();
        self.poses.clear();
        self.outsim_clock = None;
        self.previous_threats.clear();
    }
    fn forget_driver_history(&mut self, id: u8, replace: bool, reason: &str) {
        if self.reference_target == Some(id) {
            if replace {
                self.delta.reset_reference();
            } else {
                self.delta.reset_lap_with_reason(reason);
            }
            self.clear_histories();
        } else {
            self.gaps.remove_driver(id);
            self.clear_radar_histories();
        }
    }
    fn follows_view(&self) -> bool {
        self.follow_viewed_car
            && self
                .state
                .as_ref()
                .is_some_and(|s| s.flags & ISS_MULTI == 0)
    }
    fn select_driver(&self) -> Option<u8> {
        let state = self.state.as_ref()?;
        if !matches!(state.camera, 3 | 4)
            || state.flags & ISS_GAME == 0
            || state.flags & (ISS_REPLAY | ISS_PAUSED | ISS_SHIFTU | ISS_FRONT_END) != 0
        {
            return None;
        }
        if self.follows_view() {
            return self
                .players
                .get(&state.viewed)
                .filter(|p| p.plid != 0 && !p.in_garage && p.kind & 4 == 0)
                .map(|p| p.plid);
        }
        let mut humans = self.players.values().filter(|p| p.local_human());
        let first = humans.next()?;
        if humans.next().is_some() {
            return None;
        }
        (state.viewed == first.plid).then_some(first.plid)
    }
    fn inactive_status(&self) -> &'static str {
        let Some(state) = &self.state else {
            return "Waiting for LFS state and player roster";
        };
        if !self.follows_view() {
            return "Drive your local human car in cockpit or custom view";
        }
        if state.flags & ISS_REPLAY != 0 {
            "Replay unsupported — start a live single-player session"
        } else if state.flags & ISS_PAUSED != 0 {
            "LFS paused — resume to follow the viewed car"
        } else if state.flags & (ISS_FRONT_END | ISS_GAME) != ISS_GAME {
            "Start a live single-player session to follow a car"
        } else if state.flags & ISS_SHIFTU != 0 || !matches!(state.camera, 3 | 4) {
            "Use cockpit or custom view to follow a car"
        } else {
            "Waiting for a viewed car on track and its player roster"
        }
    }
    pub fn packet(&mut self, packet: Packet, time: u64) -> Result<(), String> {
        match packet {
            Packet::State(state) => {
                if self.state.as_ref().is_some_and(|old| {
                    (self.follow_viewed_car
                        && old.flags & ISS_MULTI == 0
                        && state.flags & ISS_MULTI == 0
                        && old.viewed != state.viewed)
                        || (old.flags ^ state.flags) & ISS_MULTI != 0
                }) {
                    self.delta.reset_reference();
                }
                if !self.delta.track_matches(&state.track) {
                    self.delta.clear();
                }
                if !self.gaps.track_matches(&state.track) {
                    self.gaps.forget_track();
                }
                if self.state.as_ref().is_some_and(|old| {
                    old.track != state.track
                        || old.viewed != state.viewed
                        || old.camera != state.camera
                        || (old.flags ^ state.flags)
                            & (ISS_GAME
                                | ISS_REPLAY
                                | ISS_PAUSED
                                | ISS_SHIFTU
                                | ISS_FRONT_END
                                | ISS_MULTI)
                            != 0
                }) {
                    self.delta.reset_lap();
                    self.clear_histories();
                }
                self.state = Some(state);
            }
            Packet::Player(player) => {
                // NPL for an existing PLID means re-entry or car replacement.
                if self.players.contains_key(&player.plid) {
                    self.forget_driver_history(
                        player.plid,
                        true,
                        "selected car re-entered or was replaced",
                    );
                }
                self.players.insert(player.plid, player);
            }
            Packet::PlayerSnapshot(player) => {
                // Refreshing the roster must not look like re-entering the race.
                if self.players.get(&player.plid).is_some_and(|old| {
                    old.ucid != player.ucid
                        || old.model != player.model
                        || old.kind != player.kind
                        || old.in_garage
                }) {
                    self.forget_driver_history(
                        player.plid,
                        true,
                        "driver ownership or car changed",
                    );
                }
                self.players.insert(player.plid, player);
            }
            Packet::Pit(id) => {
                self.forget_driver_history(id, true, "selected driver pitted");
                if let Some(p) = self.players.get_mut(&id) {
                    p.in_garage = true;
                }
            }
            Packet::Leave(id) => {
                self.forget_driver_history(id, true, "selected driver left the race");
                self.players.remove(&id);
            }
            Packet::ConnectionLeft(id) => {
                let leaving: Vec<_> = self
                    .players
                    .values()
                    .filter(|p| p.ucid == id)
                    .map(|p| p.plid)
                    .collect();
                for plid in leaving {
                    self.forget_driver_history(plid, true, "selected connection left");
                }
                self.players.retain(|_, p| p.ucid != id);
            }
            Packet::Reset(id) => {
                self.forget_driver_history(id, false, "selected car reset");
            }
            Packet::Session => {
                self.delta.reset_reference();
                self.clear_histories();
            }
            Packet::LayoutChanged => self.delta.reset_reference(),
            Packet::Lap {
                plid,
                time_ms,
                penalty,
            } => {
                if self.selected == Some(plid) {
                    self.delta.lap_report(time_ms, penalty, time);
                }
            }
            Packet::InvalidLap(id) => {
                if self.selected == Some(id) {
                    self.delta.invalidate();
                }
            }
            Packet::RaceStart { info, requested } => {
                if !requested {
                    self.delta.reset_reference();
                    self.clear_histories();
                }
                self.delta.set_track(info.clone());
                self.gaps.set_track(info);
            }
            Packet::Takeover(id) => {
                self.forget_driver_history(id, true, "driver ownership changed");
                // Re-request roster before trusting ownership again.
                self.players.remove(&id);
            }
            Packet::Camera(id) => {
                if self.selected == Some(id) {
                    self.delta.reset_lap();
                    self.state = None;
                    self.clear_histories();
                }
            }
            Packet::Tiny(10..=12) => self.clear(),
            Packet::Mci(cars) => {
                if let Some(cars) = self.assembler.push(cars)? {
                    let teleported: Vec<_> = self
                        .snapshots
                        .back()
                        .map(|last| {
                            let dt = time.saturating_sub(last.time) as f64 / 1000.0;
                            cars.iter()
                                .filter(|c| {
                                    last.cars.iter().find(|old| old.plid == c.plid).is_some_and(
                                        |old| old.pose.distance(c.pose) > 10.0 + dt * 150.0,
                                    )
                                })
                                .map(|c| c.plid)
                                .collect()
                        })
                        .unwrap_or_default();
                    if self.selected.is_some_and(|id| teleported.contains(&id)) {
                        self.delta
                            .reset_lap_with_reason("local car position jumped");
                    }
                    if !teleported.is_empty() {
                        // Each gap history validates its own car. An opponent
                        // teleport must not erase everyone else's passages.
                        self.clear_radar_histories();
                    }
                    self.gaps.update(&cars, time);
                    if let Some(car) = self
                        .selected
                        .and_then(|id| cars.iter().find(|c| c.plid == id))
                    {
                        if self.delta_association.is_some_and(|pose| {
                            time.saturating_sub(pose.time) <= self.association_stale_ms
                                && pose.pose.distance(car.pose) <= 12.0
                                && wrap_angle(pose.pose.heading - car.pose.heading).abs() <= 1.0
                        }) {
                            self.delta.update(car, time);
                        } else {
                            self.delta.suspend("Waiting for matching OutSim telemetry");
                        }
                    } else {
                        self.delta.suspend("Waiting for local track progress");
                    }
                    self.snapshots.push_back(Snapshot { time, cars });
                    while self.snapshots.len() > 64 {
                        self.snapshots.pop_front();
                    }
                    self.mci_sets += 1;
                }
            }
            _ => {}
        }
        let selected = self.select_driver();
        self.delta.select_driver(
            selected
                .and_then(|id| self.players.get(&id))
                .map(|p| (p.plid, p.ucid, p.model.clone())),
        );
        if selected.is_some() {
            self.reference_target = selected;
        }
        if self.selected != selected {
            self.clear_histories();
            self.selected = selected;
        }
        Ok(())
    }
    pub fn outsim(&mut self, sample: Sample, time: u64, config: &Config) {
        self.association_stale_ms = config.stale_ms;
        self.outsim_samples += 1;
        let Some(id) = self.selected else {
            self.rejected_outsim += 1;
            return;
        };
        let Some(latest) = self.snapshots.back() else {
            self.rejected_outsim += 1;
            return;
        };
        let Some(local) = latest.cars.iter().find(|c| c.plid == id) else {
            self.rejected_outsim += 1;
            return;
        };
        if time.saturating_sub(latest.time) > config.stale_ms
            || local.pose.distance(sample.pose) > 12.0
            || wrap_angle(local.pose.heading - sample.pose.heading).abs() > 1.0
        {
            self.delta.suspend("Waiting for matching OutSim telemetry");
            self.delta_association = None;
            self.poses.clear();
            self.outsim_clock = None;
            self.rejected_outsim += 1;
            return;
        }
        if let Some(previous) = self.outsim_clock {
            let delta = sample.time_ms.wrapping_sub(previous);
            if delta == 0 || delta >= 0x8000_0000 {
                // Session events normally reset this clock. Recover after silence too.
                if self
                    .poses
                    .back()
                    .is_none_or(|p| time.saturating_sub(p.time) <= config.hide_ms)
                {
                    self.rejected_outsim += 1;
                    return;
                }
                self.poses.clear();
                self.delta
                    .reset_lap_with_reason("OutSim clock restarted after silence");
            }
        }
        if self.poses.back().is_some_and(|last| {
            last.pose.distance(sample.pose) > 10.0 + time.saturating_sub(last.time) as f64 * 0.15
        }) {
            self.delta.reset_lap_with_reason("OutSim position jumped");
            self.poses.clear();
        }
        self.outsim_clock = Some(sample.time_ms);
        self.delta_association = Some(TimedPose {
            time,
            pose: sample.pose,
        });
        self.poses.push_back(TimedPose {
            time,
            pose: sample.pose,
        });
        while self.poses.len() > 128 {
            self.poses.pop_front();
        }
    }
    pub fn gaps(&self, now: u64, config: &Config, radar: &RadarFrame) -> GapFrame {
        if self
            .state
            .as_ref()
            .is_none_or(|state| !self.gaps.track_matches(&state.track))
        {
            return GapFrame::unavailable("Waiting for matching track information");
        }
        if !radar.live || radar.uncertain {
            return GapFrame::unavailable(&radar.status);
        }
        self.gaps
            .frame(self.selected, &self.players, now, config.stale_ms)
    }
    pub fn delta(&self, now: u64, config: &Config, radar: &RadarFrame) -> crate::delta::DeltaFrame {
        let delta = self.delta.frame(now, config.stale_ms);
        if !radar.live || radar.uncertain {
            return crate::delta::DeltaFrame {
                best_seconds: delta.best_seconds,
                ..crate::delta::DeltaFrame::unavailable(&radar.status)
            };
        }
        if self
            .state
            .as_ref()
            .is_none_or(|s| !self.delta.track_matches(&s.track))
        {
            return crate::delta::DeltaFrame::unavailable("Waiting for matching track information");
        }
        delta
    }
    pub fn frame(&mut self, now: u64, config: &Config) -> RadarFrame {
        let mut frame = RadarFrame {
            mci_age_ms: self.snapshots.back().map(|s| now.saturating_sub(s.time)),
            outsim_age_ms: self.poses.back().map(|p| now.saturating_sub(p.time)),
            ..Default::default()
        };
        let Some(id) = self.selected else {
            frame.status = self.inactive_status().into();
            return frame;
        };
        frame.driver = self.players.get(&id).map(|p| p.name.clone());
        let Some(snapshot) = self.snapshots.back() else {
            frame.status = "Waiting for a complete MCI update".into();
            return frame;
        };
        let Some(local) = self.poses.back() else {
            frame.status = "OutSim required — waiting for a matching car pose".into();
            return frame;
        };
        if now
            .saturating_sub(snapshot.time)
            .max(now.saturating_sub(local.time))
            > config.hide_ms
        {
            frame.status = "Telemetry stale — radar paused".into();
            return frame;
        }
        let display_time = now
            .saturating_sub(config.interpolation_ms)
            .min(snapshot.time)
            .min(local.time);
        let Some(me) = pose_at(&self.poses, display_time) else {
            frame.status = "Synchronizing MCI and OutSim".into();
            return frame;
        };
        let Some(cars) = cars_at(&self.snapshots, display_time) else {
            frame.status = "Synchronizing MCI and OutSim".into();
            return frame;
        };
        let Some(reference) = cars.iter().find(|c| c.plid == id) else {
            frame.status = "Selected car missing from MCI — radar paused".into();
            return frame;
        };
        if reference.pose.distance(me) > 12.0
            || wrap_angle(reference.pose.heading - me.heading).abs() > 1.0
        {
            frame.status = "MCI / OutSim association mismatch — radar paused".into();
            return frame;
        }
        frame.live = true;
        frame.uncertain = now
            .saturating_sub(snapshot.time)
            .max(now.saturating_sub(local.time))
            > config.stale_ms;
        frame.status = if frame.uncertain {
            "Delayed telemetry"
        } else {
            "MCI + OutSim connected"
        }
        .into();
        if self.follows_view() {
            frame.status = format!(
                "Following {} · {}",
                frame.driver.as_deref().unwrap_or("viewed car"),
                frame.status
            );
        }
        let mut next_threats = BTreeMap::new();
        for car in cars.iter().filter(|c| c.plid != id) {
            if self.players.get(&car.plid).is_none_or(|p| p.in_garage)
                || (car.pose.z - me.z).abs() > config.height_m
            {
                continue;
            }
            let [right, forward] = local_offset(me, car.pose);
            let radius = config.car_length_m.hypot(config.car_width_m) * 0.5;
            if right.abs() > config.side_m + radius
                || forward > config.front_m + radius
                || forward < -config.rear_m - radius
            {
                continue;
            }
            let relative_heading = wrap_angle(car.pose.heading - me.heading);
            let uncertain = frame.uncertain || car.info & 32 != 0 || reference.info & 32 != 0;
            let hysteresis =
                if self.previous_threats.get(&car.plid) == Some(&Threat::PotentialContact) {
                    0.15
                } else {
                    0.0
                };
            let threat = if !uncertain
                && footprints_intersect(
                    right,
                    forward,
                    relative_heading,
                    config.car_width_m,
                    config.car_length_m,
                    0.25 + hysteresis,
                ) {
                Threat::PotentialContact
            } else if !uncertain
                && forward.abs() < config.car_length_m
                && right.abs() < config.car_width_m + 2.0
            {
                Threat::Alongside
            } else {
                Threat::Nearby
            };
            next_threats.insert(car.plid, threat);
            frame.cars.push(RadarCar {
                plid: car.plid,
                name: self.players[&car.plid].name.clone(),
                right,
                forward,
                relative_heading,
                threat,
                uncertain,
            });
        }
        self.previous_threats = next_threats;
        frame
    }
}
fn pose_at(history: &VecDeque<TimedPose>, time: u64) -> Option<Pose> {
    if time < history.front()?.time {
        return None;
    }
    let mut before = *history.front()?;
    for after in history.iter().skip(1) {
        if after.time >= time {
            let fraction =
                (time - before.time) as f64 / after.time.saturating_sub(before.time).max(1) as f64;
            return Some(before.pose.interpolate(after.pose, fraction));
        }
        before = *after;
    }
    Some(before.pose)
}
fn cars_at(history: &VecDeque<Snapshot>, time: u64) -> Option<Vec<Car>> {
    if time < history.front()?.time {
        return None;
    }
    let mut before = history.front()?;
    for after in history.iter().skip(1) {
        if after.time >= time {
            let fraction =
                (time - before.time) as f64 / after.time.saturating_sub(before.time).max(1) as f64;
            return Some(
                after
                    .cars
                    .iter()
                    .filter_map(|car| {
                        let old = before.cars.iter().find(|old| old.plid == car.plid)?;
                        let mut result = car.clone();
                        result.pose = old.pose.interpolate(car.pose, fraction);
                        Some(result)
                    })
                    .collect(),
            );
        }
        before = after;
    }
    Some(before.cars.clone())
}
/// Separating-axis test for two approximate oriented vehicle footprints.
pub fn footprints_intersect(
    x: f64,
    y: f64,
    angle: f64,
    width: f64,
    length: f64,
    margin: f64,
) -> bool {
    let (sin, cos) = angle.sin_cos();
    let hw = width * 0.5 + margin;
    let hl = length * 0.5 + margin;
    let opponent_axes = [[cos, sin], [-sin, cos]];
    for axis in [[1.0, 0.0], [0.0, 1.0], opponent_axes[0], opponent_axes[1]] {
        let own_extent = hw * axis[0].abs() + hl * axis[1].abs();
        let other_extent = hw * (axis[0] * cos + axis[1] * sin).abs()
            + hl * (-axis[0] * sin + axis[1] * cos).abs();
        if (x * axis[0] + y * axis[1]).abs() > own_extent + other_extent {
            return false;
        }
    }
    true
}
