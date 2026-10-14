#![cfg(feature = "desktop")]
use lfs_openradar::{
    config::Config,
    overlay::session::{Monitor, OverlaySession},
};

#[test]
fn first_monitor_selection_is_retained_when_it_disappears_before_saving() {
    let a = Monitor {
        name: "DP-1".into(),
        size: [1920.0, 1080.0],
        scale: 1.0,
    };
    let b = Monitor {
        name: "HDMI-A-1".into(),
        ..a.clone()
    };
    let mut session = OverlaySession::new(Config::default(), true);
    session.outputs_changed(vec![a, b.clone()]);
    assert_eq!(session.status().selected_monitor.as_deref(), Some("DP-1"));
    session.update(Config::default(), true);
    session.outputs_changed(vec![b]);
    assert_eq!(session.status().selected_monitor.as_deref(), Some("DP-1"));
    assert!(!session.status().visible);
}

#[test]
fn grouped_gt_paint_uses_native_scales_instead_of_window_scales() {
    let mut config = Config::default();
    config.performance_delta.enabled = true;
    config.gap_ahead.enabled = true;
    config.performance_delta.scale = 0.5;
    config.gap_ahead.scale = 0.5;
    config.wayland.performance_delta.scale = 2.0;
    config.wayland.gap_ahead.scale = 2.0;
    let mut session = OverlaySession::new(config, true);
    session.outputs_changed(vec![Monitor {
        name: "DP-1".into(),
        size: [2560.0, 1440.0],
        scale: 1.0,
    }]);
    let mut snapshot = lfs_openradar::runtime::Snapshot::default();
    snapshot.delta.best_seconds = Some(62.123);
    let output = session.paint(&eframe::egui::Context::default(), &snapshot);
    let font_size = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                eframe::egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.galley.job.sections[0].format.font_id.size)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {label}"))
    };
    assert_eq!(font_size("1:02.123"), 54.0);
    assert_eq!(font_size("AHEAD"), 22.0);
}

#[test]
fn explicit_save_keeps_requested_scales_but_persists_displayed_positions() {
    let mut config = Config::default();
    config.wayland.radar.x = Some(1000.0);
    config.wayland.radar.scale = 2.0;
    config.gap_ahead.enabled = true;
    let mut session = OverlaySession::new(config, true);
    session.outputs_changed(vec![Monitor {
        name: "DP-1".into(),
        size: [800.0, 600.0],
        scale: 1.25,
    }]);
    let saved = session.config_for_save();
    assert_eq!(saved.wayland.radar.x, Some(200.0));
    assert_eq!(saved.wayland.radar.scale, 2.0);
    assert_eq!(saved.wayland.monitor.as_deref(), Some("DP-1"));
    assert!(saved.gap_ahead.enabled);
    assert_eq!(session.status().gadgets.len(), 6);
}

#[test]
fn displayed_geometry_fits_the_monitor_without_replacing_requested_position_or_scale() {
    let mut config = Config::default();
    config.wayland.radar.x = Some(1000.0);
    config.wayland.radar.y = Some(-20.0);
    config.wayland.radar.scale = 2.0;
    let mut session = OverlaySession::new(config, true);
    session.outputs_changed(vec![Monitor {
        name: "DP-1".into(),
        size: [800.0, 600.0],
        scale: 1.5,
    }]);
    let status = session.status();
    assert_eq!(status.physical_size, [1200, 900]);
    let radar = &status.gadgets[0];
    assert_eq!([radar.rect.min.x, radar.rect.min.y], [200.0, 0.0]);
    assert_eq!([radar.rect.width(), radar.rect.height()], [600.0, 600.0]);
    assert_eq!(radar.scale, 1.875);
    session.outputs_changed(vec![Monitor {
        name: "DP-1".into(),
        size: [2560.0, 1440.0],
        scale: 1.0,
    }]);
    let restored = session.status();
    assert_eq!(restored.gadgets[0].rect.min.x, 1000.0);
    assert_eq!(restored.gadgets[0].scale, 2.0);
}

#[test]
fn missing_selected_monitor_hides_hud_and_reconnect_respects_manual_visibility() {
    let mut config = Config::default();
    config.wayland.monitor = Some("DP-1".into());
    let monitor = Monitor {
        name: "DP-1".into(),
        size: [1920.0, 1080.0],
        scale: 1.25,
    };
    let other = Monitor {
        name: "HDMI-A-1".into(),
        ..monitor.clone()
    };
    let mut session = OverlaySession::new(config.clone(), true);
    session.outputs_changed(vec![monitor.clone(), other.clone()]);
    assert!(session.status().visible);
    session.outputs_changed(vec![other.clone()]);
    assert!(!session.status().visible);
    assert!(session.status().warning.is_some());
    assert_eq!(session.status().selected_monitor.as_deref(), Some("DP-1"));
    session.outputs_changed(vec![other, monitor]);
    assert!(session.status().visible);
    session.update(config, false);
    assert!(!session.status().visible);
}

#[test]
fn native_failure_is_reported_without_losing_user_settings_and_retry_clears_it() {
    let mut config = Config::default();
    config.wayland.monitor = Some("DP-1".into());
    let mut session = OverlaySession::new(config.clone(), true);
    session.failure("cannot connect to native Wayland".into());
    assert_eq!(
        session.status().error.as_deref(),
        Some("cannot connect to native Wayland")
    );
    assert!(!session.status().visible);
    session.update(config, false);
    assert_eq!(session.status().selected_monitor.as_deref(), Some("DP-1"));
    session.retry();
    assert!(session.status().error.is_none());
}
