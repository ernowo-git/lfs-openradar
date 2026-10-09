use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, net::SocketAddr, path::Path};

/// Built-in overlay appearance, independent of the desktop renderer.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HudStyle {
    Classic,
    #[default]
    #[serde(rename = "gt", alias = "gt7-inspired")]
    Gt7Inspired,
}

impl HudStyle {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Gt7Inspired => "GT",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GapSettings {
    pub enabled: bool,
    /// Independent native window position in logical screen pixels.
    pub window_x: Option<f32>,
    pub window_y: Option<f32>,
    pub scale: f32,
    /// Read the earlier shared-canvas layout, but save only window coordinates.
    #[serde(rename = "x", skip_serializing)]
    pub legacy_x: Option<f32>,
    #[serde(rename = "y", skip_serializing)]
    pub legacy_y: Option<f32>,
}
impl Default for GapSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            window_x: None,
            window_y: None,
            scale: 1.0,
            legacy_x: None,
            legacy_y: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CarProfile {
    pub name: String,
    /// Zero is accepted for legacy capacity-only profiles; the RPM bar stays unavailable.
    #[serde(default)]
    pub max_rpm: u32,
    /// Read old settings without exposing or saving the retired litres option.
    #[serde(default, skip_serializing)]
    pub fuel_tank_litres: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub insim_address: SocketAddr,
    pub insim_password: InSimPassword,
    /// Folder containing the user's LFS.exe; only the setup button edits LFS.
    pub lfs_directory: String,
    pub outsim_bind: SocketAddr,
    pub outgauge_bind: SocketAddr,
    /// Zero selects the 92-byte OutGauge packet without an ID.
    pub outgauge_id: i32,
    /// Copy valid OutGauge packets unchanged to other local telemetry apps.
    #[serde(
        deserialize_with = "forward_destinations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub outgauge_forward: Vec<SocketAddr>,
    pub cars: BTreeMap<String, CarProfile>,
    pub speed_dashboard: GapSettings,
    pub fuel: GapSettings,
    /// Percentage of the configured RPM limit that starts the red/blue blink.
    pub rpm_blink_threshold_percent: u8,
    /// Time each RPM blink color is shown, in milliseconds.
    pub rpm_blink_interval_ms: u16,
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
    pub hud_style: HudStyle,
    /// Show diagnostic status and measurement age in gap gadgets for either HUD style.
    pub hud_debug: bool,
    pub hide_when_background: bool,
    /// Key that toggles all overlays; "None" disables the shortcut.
    pub overlay_toggle_key: String,
    /// Follow the viewed car in live single player; multiplayer still uses your own car.
    pub follow_viewed_car: bool,
    pub radar_enabled: bool,
    pub gap_ahead: GapSettings,
    pub gap_behind: GapSettings,
    pub performance_delta: GapSettings,
}

/// Serialize the configured secret, but redact it in diagnostic Debug output.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct InSimPassword(pub String);

fn forward_destinations<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SocketAddr>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Destinations {
        One(SocketAddr),
        Many(Vec<SocketAddr>),
    }
    Ok(match Destinations::deserialize(deserializer)? {
        Destinations::One(address) => vec![address],
        Destinations::Many(addresses) => addresses,
    })
}

impl std::fmt::Debug for InSimPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            insim_address: "127.0.0.1:29999".parse().unwrap(),
            insim_password: InSimPassword::default(),
            lfs_directory: String::new(),
            outsim_bind: "127.0.0.1:30000".parse().unwrap(),
            outgauge_bind: "127.0.0.1:30001".parse().unwrap(),
            outgauge_id: 24602,
            outgauge_forward: Vec::new(),
            cars: BTreeMap::from([(
                "XFG".into(),
                CarProfile {
                    name: "XF GTI".into(),
                    max_rpm: 8000,
                    fuel_tank_litres: None,
                },
            )]),
            speed_dashboard: GapSettings::default(),
            fuel: GapSettings::default(),
            rpm_blink_threshold_percent: 95,
            rpm_blink_interval_ms: 100,
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
            hud_style: HudStyle::default(),
            hud_debug: false,
            hide_when_background: true,
            overlay_toggle_key: "Insert".into(),
            follow_viewed_car: false,
            radar_enabled: true,
            gap_ahead: GapSettings::default(),
            gap_behind: GapSettings::default(),
            performance_delta: GapSettings::default(),
        }
    }
}

impl Config {
    pub fn overlay_toggle_key(&self) -> Result<Option<OverlayToggleKey>, String> {
        let name = self.overlay_toggle_key.trim().to_ascii_uppercase();
        let (virtual_key, canonical) = match name.as_str() {
            "" | "NONE" => return Ok(None),
            "INSERT" => (0x2d, "Insert"),
            "DELETE" => (0x2e, "Delete"),
            "HOME" => (0x24, "Home"),
            "END" => (0x23, "End"),
            "PAGEUP" => (0x21, "PageUp"),
            "PAGEDOWN" => (0x22, "PageDown"),
            "SPACE" => (0x20, "Space"),
            "ENTER" => (0x0d, "Enter"),
            "ESCAPE" => (0x1b, "Escape"),
            "TAB" => (0x09, "Tab"),
            "BACKSPACE" => (0x08, "Backspace"),
            _ => {
                let code = if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() {
                    Some(name.as_bytes()[0])
                } else {
                    name.strip_prefix('F')
                        .and_then(|n| n.parse::<u8>().ok())
                        .filter(|n| (1..=24).contains(n))
                        .map(|n| 0x70 + n - 1)
                };
                let virtual_key = code.ok_or_else(||
                    "overlay_toggle_key must be a supported key name or None; see the usage guide".to_string())?;
                return Ok(Some(OverlayToggleKey {
                    virtual_key,
                    name: if (0x70..=0x87).contains(&virtual_key) {
                        format!("F{}", virtual_key - 0x70 + 1)
                    } else {
                        name
                    },
                }));
            }
        };
        Ok(Some(OverlayToggleKey {
            virtual_key,
            name: canonical.into(),
        }))
    }
    pub fn effective_insim_password(&self) -> String {
        std::env::var("LFS_INSIM_ADMIN").unwrap_or_else(|_| self.insim_password.0.clone())
    }
    pub fn expected_outsim_id(&self) -> Option<i32> {
        (self.outsim_id != 0).then_some(self.outsim_id)
    }
    pub fn needs_outgauge(&self) -> bool {
        self.speed_dashboard.enabled || self.fuel.enabled || !self.outgauge_forward.is_empty()
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // TOML errors can echo source lines containing the password.
        let mut config: Self = toml::from_str(&raw)
            .map_err(|_| "Invalid TOML configuration; check field names and values".to_string())?;
        config.validate()?;
        config.prepare_gap_positions();
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let mut saved = self.clone();
        // These legacy profiles only configured the retired litres display.
        saved.cars.retain(|_, profile| profile.max_rpm > 0);
        saved.prepare_gap_positions();
        let raw = toml::to_string_pretty(&saved).map_err(|e| e.to_string())?;
        std::fs::write(path, raw).map_err(|e| e.to_string())
    }

    pub fn prepare_gap_positions(&mut self) {
        for (settings, legacy_y, offset_y) in [
            (&mut self.gap_ahead, 0.12, 0.0),
            (&mut self.gap_behind, 0.88, 100.0),
            (&mut self.performance_delta, 0.5, 200.0),
            (&mut self.speed_dashboard, 0.5, 360.0),
            (&mut self.fuel, 0.5, 586.0),
        ] {
            let legacy = settings.legacy_x.is_some() || settings.legacy_y.is_some();
            settings.window_x.get_or_insert(if legacy {
                self.overlay_x + settings.legacy_x.unwrap_or(0.5) * self.overlay_size
                    - 115.0 * settings.scale
            } else {
                self.overlay_x + self.overlay_size + 16.0
            });
            settings.window_y.get_or_insert(if legacy {
                self.overlay_y + settings.legacy_y.unwrap_or(legacy_y) * self.overlay_size
                    - 38.0 * settings.scale
            } else {
                self.overlay_y + offset_y
            });
            settings.legacy_x = None;
            settings.legacy_y = None;
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=100).contains(&self.rpm_blink_threshold_percent) {
            return Err("rpm_blink_threshold_percent must be 1..100".into());
        }
        if !(50..=500).contains(&self.rpm_blink_interval_ms) {
            return Err("rpm_blink_interval_ms must be 50..500".into());
        }
        for (code, profile) in &self.cars {
            if code.trim().is_empty()
                || profile.name.trim().is_empty()
                || (profile.max_rpm == 0 && profile.fuel_tank_litres.is_none())
            {
                return Err(
                    "car profiles require a car code, a name, and an RPM limit or fuel capacity"
                        .into(),
                );
            }
            if profile
                .fuel_tank_litres
                .is_some_and(|v| !v.is_finite() || v <= 0.0)
            {
                return Err("fuel tank capacity must be finite and positive".into());
            }
        }
        if !self.outgauge_bind.ip().is_loopback() || self.outgauge_bind.port() == 0 {
            return Err("OutGauge requires a nonzero loopback UDP endpoint".into());
        }
        for destination in &self.outgauge_forward {
            if !destination.ip().is_loopback()
                || destination.port() == 0
                || destination.is_ipv4() != self.outgauge_bind.is_ipv4()
            {
                return Err(
                    "outgauge forwarding requires a nonzero loopback destination with the receiver's IP family".into(),
                );
            }
            if *destination == self.outgauge_bind || *destination == self.outsim_bind {
                return Err(
                    "outgauge forwarding destination must differ from OpenRadar's UDP receivers"
                        .into(),
                );
            }
        }
        self.overlay_toggle_key()?;
        crate::lfs::insim::init(self.mci_interval_ms, &self.insim_password.0)?;
        for settings in [
            &self.gap_ahead,
            &self.gap_behind,
            &self.performance_delta,
            &self.speed_dashboard,
            &self.fuel,
        ] {
            if [settings.window_x, settings.window_y]
                .iter()
                .flatten()
                .any(|v| !v.is_finite())
                || [settings.legacy_x, settings.legacy_y]
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || !settings.scale.is_finite()
                || !(0.5..=2.0).contains(&settings.scale)
            {
                return Err("Gap screen positions must be finite and scale 0.5..2".into());
            }
        }
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
        if self.needs_outgauge()
            && self.outgauge_bind == self.outsim_bind
            && matches!(
                crate::lfs::outsim::packet_size(self.outsim_options, self.expected_outsim_id())?,
                92 | 96
            )
        {
            return Err(
                "OutGauge needs a separate port when OutSim packets are 92 or 96 bytes".into(),
            );
        }
        if !self.side_m.is_finite() || !(0.0..=100.0).contains(&self.side_m) {
            return Err("Radar side range must be finite and 0..100 metres".into());
        }
        for v in [
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayToggleKey {
    pub virtual_key: u8,
    pub name: String,
}
