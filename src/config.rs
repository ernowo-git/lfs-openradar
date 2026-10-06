use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub insim_address: SocketAddr,
    pub outsim_bind: SocketAddr,
    /// Zero selects the legacy layout; otherwise OutSim Opts bitmask.
    pub outsim_options: u16,
    /// Zero disables ID validation and selects a legacy packet without an ID.
    pub outsim_id: i32,
    pub mci_interval_ms: u16,
    pub interpolation_ms: u64,
    pub stale_ms: u64,
    pub hide_ms: u64,
    pub side_m: f64,
    pub front_m: f64,
    pub rear_m: f64,
    pub height_m: f64,
    pub car_width_m: f64,
    pub car_length_m: f64,
    pub overlay_x: f32,
    pub overlay_y: f32,
    pub overlay_size: f32,
    pub hide_when_background: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            insim_address: "127.0.0.1:29999".parse().unwrap(),
            outsim_bind: "127.0.0.1:30000".parse().unwrap(),
            outsim_options: 0x1ff,
            outsim_id: 24601,
            mci_interval_ms: 20,
            interpolation_ms: 60,
            stale_ms: 250,
            hide_ms: 500,
            side_m: 5.0,
            front_m: 8.0,
            rear_m: 7.0,
            height_m: 3.0,
            car_width_m: 1.8,
            car_length_m: 4.2,
            overlay_x: 40.0,
            overlay_y: 160.0,
            overlay_size: 320.0,
            hide_when_background: true,
        }
    }
}

impl Config {
    pub fn expected_outsim_id(&self) -> Option<i32> {
        (self.outsim_id != 0).then_some(self.outsim_id)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let config: Self = toml::from_str(&raw).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let raw = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, raw).map_err(|e| e.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.insim_address.ip().is_loopback()
            || !self.outsim_bind.ip().is_loopback()
            || self.insim_address.port() == 0
            || self.outsim_bind.port() == 0
        {
            return Err("Use nonzero ports on loopback addresses for the local LFS client".into());
        }
        if !(10..=8000).contains(&self.mci_interval_ms) || !self.mci_interval_ms.is_multiple_of(10)
        {
            return Err("MCI interval must be 10..8000 ms in steps of 10".into());
        }
        if self.interpolation_ms > 200
            || self.stale_ms <= self.interpolation_ms
            || self.hide_ms <= self.stale_ms
            || self.hide_ms > 2000
        {
            return Err("Require interpolation <= 200 ms < stale < hide <= 2000 ms".into());
        }
        if self.outsim_options & !0x1ff != 0
            || (self.outsim_options != 0 && self.outsim_options & 0xc != 0xc)
            || (self.outsim_options != 0 && self.outsim_id != 0 && self.outsim_options & 2 == 0)
        {
            return Err("OutSim options must include TIME and MAIN, or be zero for legacy".into());
        }
        for v in [
            self.side_m,
            self.front_m,
            self.rear_m,
            self.height_m,
            self.car_width_m,
            self.car_length_m,
        ] {
            if !v.is_finite() || !(0.1..=100.0).contains(&v) {
                return Err("Radar distances must be finite, positive, and <= 100 metres".into());
            }
        }
        if !self.overlay_x.is_finite()
            || !self.overlay_y.is_finite()
            || !self.overlay_size.is_finite()
            || !(220.0..=800.0).contains(&self.overlay_size)
        {
            return Err(
                "Overlay size must be 220..800 logical pixels; position must be finite".into(),
            );
        }
        Ok(())
    }
}
