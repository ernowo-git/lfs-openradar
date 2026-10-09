use lfs_openradar::config::{Config, HudStyle};

#[test]
fn default_and_example_configs_use_gt_and_both_themes_round_trip() {
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
        ("classic", HudStyle::Classic),
        ("gt", HudStyle::Gt7Inspired),
        ("gt7-inspired", HudStyle::Gt7Inspired),
    ] {
        let config: Config = toml::from_str(&format!("hud_style = '{name}'\nhud_debug = true\noverlay_x = 123.0\n[gap_ahead]\nscale = 1.5\nwindow_x = 456.0\nenabled = true")).unwrap();
        config.validate().unwrap();
        assert_eq!(config.hud_style, expected);
        let saved = toml::to_string(&config).unwrap();
        let canonical = if expected == HudStyle::Classic {
            "classic"
        } else {
            "gt"
        };
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
