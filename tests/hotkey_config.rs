use lfs_openradar::config::Config;

#[test]
fn existing_configs_default_to_insert_and_custom_keys_round_trip() {
    let legacy: Config = toml::from_str("side_m = 7.0").unwrap();
    let key = legacy.overlay_toggle_key().unwrap().unwrap();
    assert_eq!((key.virtual_key, key.name.as_str()), (0x2d, "Insert"));
    let custom: Config = toml::from_str("overlay_toggle_key = 'f8'").unwrap();
    custom.validate().unwrap();
    let saved = toml::to_string(&custom).unwrap();
    let restored: Config = toml::from_str(&saved).unwrap();
    assert_eq!(restored.overlay_toggle_key, "f8");
    let key = restored.overlay_toggle_key().unwrap().unwrap();
    assert_eq!((key.virtual_key, key.name.as_str()), (0x77, "F8"));
    let example: Config = toml::from_str(include_str!("../openradar.example.toml")).unwrap();
    example.validate().unwrap();
    assert_eq!(example.overlay_toggle_key, "Insert");
}

#[test]
fn hotkey_names_are_case_insensitive_and_none_disables_the_shortcut() {
    for name in [
        " insert ",
        "Delete",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Space",
        "Enter",
        "Escape",
        "Tab",
        "Backspace",
        "F1",
        "F24",
        "a",
        "Z",
        "0",
        "9",
    ] {
        let config = Config {
            overlay_toggle_key: name.into(),
            ..Default::default()
        };
        config.validate().unwrap();
        assert!(config.overlay_toggle_key().unwrap().is_some());
    }
    for name in ["", "None", " none "] {
        let config = Config {
            overlay_toggle_key: name.into(),
            ..Default::default()
        };
        config.validate().unwrap();
        assert!(config.overlay_toggle_key().unwrap().is_none());
    }
}

#[test]
fn unsupported_keys_fail_validation_instead_of_silently_disabling() {
    for name in ["F0", "F25", "F256", "Ctrl+Insert", "unknown", "1.0", "💡"] {
        let config = Config {
            overlay_toggle_key: name.into(),
            ..Default::default()
        };
        assert!(
            config
                .validate()
                .unwrap_err()
                .starts_with("overlay_toggle_key")
        );
    }
}
