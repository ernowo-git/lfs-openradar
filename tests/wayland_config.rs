use lfs_openradar::config::Config;
#[test]
fn cli_accepts_auto_as_an_override() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lfs-openradar"))
        .args([
            "--overlay-backend",
            "auto",
            "--demo",
            "--headless",
            "--seconds",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn backend_defaults_to_auto_and_explicit_window_stays_an_override() {
    let path = std::env::temp_dir().join(format!(
        "openradar-backend-default-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, "overlay_x = -900.0\n").unwrap();
    let config = Config::load(&path).unwrap();
    config.save(&path).unwrap();
    let saved: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["overlay_backend"].as_str(), Some("auto"));
    assert_eq!(Config::load(&path).unwrap().overlay_x, -900.0);
    std::fs::write(&path, "overlay_backend = 'window'\n").unwrap();
    Config::load(&path).unwrap().save(&path).unwrap();
    let saved: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["overlay_backend"].as_str(), Some("window"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn native_layout_rejects_nonfinite_coordinates_and_invalid_chosen_scales() {
    for input in ["x = nan", "y = inf", "scale = 0.0", "scale = 3.0"] {
        let config: Config = toml::from_str(&format!("[wayland.gap_ahead]\n{input}\n")).unwrap();
        assert!(config.validate().is_err(), "accepted {input}");
    }
}

#[test]
fn wayland_layout_round_trips_without_replacing_window_positions_or_enablement() {
    let path = std::env::temp_dir().join(format!("openradar-wayland-{}.toml", std::process::id()));
    std::fs::write(&path, "overlay_backend = 'wayland'\noverlay_x = -900.0\noverlay_y = 120.0\nradar_enabled = false\n[gap_ahead]\nwindow_x = -600.0\nwindow_y = 80.0\nenabled = true\nscale = 1.5\n[wayland]\nmonitor = 'DP-1'\n[wayland.radar]\nx = 120.0\ny = 240.0\nscale = 1.25\n[wayland.gap_ahead]\nx = 20.0\ny = 40.0\nscale = 0.75\n").unwrap();
    let config = Config::load(&path)
        .expect("native layout should load through the normal configuration interface");
    config.save(&path).unwrap();
    let restored = Config::load(&path).unwrap();
    let saved = toml::Value::try_from(&restored).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(saved["overlay_backend"].as_str(), Some("wayland"));
    assert_eq!(saved["wayland"]["monitor"].as_str(), Some("DP-1"));
    assert_eq!(saved["wayland"]["radar"]["x"].as_float(), Some(120.0));
    assert_eq!(
        saved["wayland"]["gap_ahead"]["scale"].as_float(),
        Some(0.75)
    );
    assert_eq!(restored.overlay_x, -900.0);
    assert_eq!(restored.gap_ahead.window_x, Some(-600.0));
    assert_eq!(restored.gap_ahead.scale, 1.5);
    assert!(restored.gap_ahead.enabled);
    assert!(!restored.radar_enabled);
}
