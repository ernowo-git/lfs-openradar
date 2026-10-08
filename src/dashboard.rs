//! Latest matching dashboard sample; missing and stale data never become 'off'.
use crate::{
    config::Config,
    lfs::{insim::Player, outgauge::Sample},
};

#[derive(Clone, Debug, Default)]
pub struct DashboardFrame {
    pub sample: Option<Sample>,
    pub car: Option<String>,
    pub abs_enabled: Option<bool>,
    /// Local light switch: 0 off, 1 side, 2 low, 3 high.
    pub headlight_switch: Option<u8>,
    pub age_ms: Option<u64>,
    pub status: String,
}
#[derive(Default)]
pub struct DashboardTelemetry {
    latest: Option<(Sample, u64)>,
    local_lights: Option<(u8, String, u8, u64)>,
}
impl DashboardTelemetry {
    pub fn clear(&mut self) {
        self.latest = None;
        self.local_lights = None;
    }
    pub fn receive_local_lights(&mut self, headlights: u8, now: u64, player: &Player) {
        if player.local_human() && headlights <= 3 {
            self.local_lights = Some((player.plid, player.model.clone(), headlights, now));
        }
    }
    pub fn receive(
        &mut self,
        sample: Sample,
        now: u64,
        player: Option<&Player>,
        stale_ms: u64,
    ) -> bool {
        let Some(player) = player else {
            return false;
        };
        if sample.plid == 0 || sample.plid != player.plid || sample.car != player.model {
            return false;
        }
        if let Some((old, received)) = &self.latest
            && old.plid == sample.plid
            && old.car == sample.car
            && now.saturating_sub(*received) <= stale_ms
            && (sample.time_ms.wrapping_sub(old.time_ms) as i32) <= 0
        {
            return false;
        }
        self.latest = Some((sample, now));
        true
    }
    pub fn frame(&self, now: u64, config: &Config, player: Option<&Player>) -> DashboardFrame {
        let mut frame = DashboardFrame::default();
        let Some(player) = player else {
            frame.status = "Use a live cockpit or custom view".into();
            return frame;
        };
        frame.car = Some(player.model.clone());
        frame.status = "Waiting for OutGauge".into();
        if let Some((sample, received)) = &self.latest
            && sample.plid == player.plid
            && sample.car == player.model
        {
            let age = now.saturating_sub(*received);
            frame.age_ms = Some(age);
            if age <= config.stale_ms {
                frame.sample = Some(sample.clone());
                frame.abs_enabled = Some(player.abs_enabled);
                if let Some((plid, car, headlights, received)) = &self.local_lights
                    && *plid == player.plid
                    && car == &player.model
                    && player.local_human()
                    && now.saturating_sub(*received) <= config.stale_ms
                {
                    frame.headlight_switch = Some(*headlights);
                }
                frame.status = if config.cars.get(&sample.car).is_some_and(|p| p.max_rpm > 0) {
                    "OutGauge connected".into()
                } else {
                    "Set this car's RPM limit to enable the RPM bar".into()
                };
            } else {
                frame.status = "OutGauge stale".into();
            }
        }
        frame
    }
}
