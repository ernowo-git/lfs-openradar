use lfs_openradar::config::{Config, HudStyle};

#[test]
fn legacy_styles_migrate_to_gt_without_changing_placement() {
    assert_eq!(HudStyle::default(), HudStyle::Gt7Inspired);
    assert_eq!(Config::default().hud_style, HudStyle::Gt7Inspired);
    assert_eq!(HudStyle::Gt7Inspired.label(), "GT");
    let legacy: Config = toml::from_str("overlay_x = 123.0").unwrap();
    assert_eq!(legacy.hud_style, HudStyle::Gt7Inspired);
    assert!(!legacy.hud_debug);
    let example: Config = toml::from_str(include_str!("../openradar.example.toml")).unwrap();
    assert_eq!(example.hud_style, HudStyle::Gt7Inspired);
    assert!(!example.hud_debug);
    for (name, expected) in [
        ("classic", HudStyle::Gt7Inspired),
        ("gt", HudStyle::Gt7Inspired),
        ("gt7-inspired", HudStyle::Gt7Inspired),
    ] {
        let config: Config = toml::from_str(&format!("hud_style = '{name}'\nhud_debug = true\noverlay_x = 123.0\n[gap_ahead]\nscale = 1.5\nwindow_x = 456.0\nenabled = true")).unwrap();
        config.validate().unwrap();
        assert_eq!(config.hud_style, expected);
        let saved = toml::to_string(&config).unwrap();
        let canonical = "gt";
        assert!(saved.contains(&format!("hud_style = \"{canonical}\"")));
        let restored: Config = toml::from_str(&saved).unwrap();
        assert_eq!(restored.hud_style, expected);
        assert!(restored.hud_debug);
        assert_eq!(restored.overlay_x, 123.0);
        assert_eq!(restored.gap_ahead.scale, 1.5);
        assert_eq!(restored.gap_ahead.window_x, Some(456.0));
        assert!(restored.gap_ahead.enabled);
    }
}

#[test]
fn unknown_hud_styles_are_rejected() {
    assert!(toml::from_str::<Config>("hud_style = 'unknown'").is_err());
}

#[test]
fn relative_configuration_migrates_style_and_keeps_each_backend_placement() {
    let config: Config = toml::from_str("hud_style='classic'\n[relative]\nenabled=true\nwindow_x=-800.0\nwindow_y=100.0\nscale=0.75\n[wayland.relative]\nx=24.0\ny=360.0\nscale=1.25\n").unwrap();
    config.validate().unwrap();
    let saved = toml::to_string(&config).unwrap();
    let restored: Config = toml::from_str(&saved).unwrap();
    assert!(saved.contains("hud_style = \"gt\""));
    assert!(restored.relative.enabled);
    assert_eq!(restored.relative.window_x, Some(-800.0));
    assert_eq!(restored.relative.scale, 0.75);
    assert_eq!(restored.wayland.relative.x, Some(24.0));
    assert_eq!(restored.wayland.relative.scale, 1.25);
    assert!(!Config::default().relative.enabled);
    for input in [
        "[relative]\nscale=3.0",
        "[relative]\nwindow_x=nan",
        "[wayland.relative]\nscale=0.0",
        "[wayland.relative]\ny=inf",
    ] {
        assert!(toml::from_str::<Config>(input).unwrap().validate().is_err());
    }
}
