#![cfg(all(target_os = "linux", feature = "native-wayland"))]
use lfs_openradar::{
    config::Config,
    overlay::{native::NativeOverlay, session::SnapshotSource},
};

#[test]
#[ignore = "requires a real XWayland panel, KDE/KWin Wayland session, and GPU"]
fn auto_backend_uses_native_on_supported_kde_and_respects_window_override() {
    let path = std::env::temp_dir().join(format!(
        "openradar-auto-backend-{}.toml",
        std::process::id()
    ));
    // Test automatic defaults, saved choices, and CLI override precedence.
    for (saved, override_arg, expected_preference, expected_active) in [
        (None, None, "Auto", "Wayland"),
        (None, Some("window"), "Window", "Window"),
        (Some("window"), None, "Window", "Window"),
        (Some("window"), Some("auto"), "Auto", "Wayland"),
    ] {
        let mut input = "radar_enabled = true\noverlay_toggle_key = 'None'\n".to_string();
        if let Some(saved) = saved {
            input.push_str(&format!("overlay_backend = '{saved}'\n"));
        }
        std::fs::write(&path, &input).unwrap();
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_lfs-openradar"));
        command
            .args(["--demo", "--seconds", "1", "--config"])
            .arg(&path)
            .env("XDG_CURRENT_DESKTOP", "Plasma:KDE")
            .env("XDG_SESSION_TYPE", "wayland");
        if let Some(backend) = override_arg {
            command.args(["--overlay-backend", backend]);
        }
        let output = command.output().unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{diagnostics}");
        assert!(
            diagnostics.contains(&format!(
                "Overlay backend {expected_preference} · active {expected_active}"
            )),
            "{diagnostics}"
        );
        assert_eq!(
            diagnostics.contains("Native Wayland GPU ready"),
            expected_active == "Wayland",
            "{diagnostics}"
        );
        // Startup must not rewrite the saved preference or either layout.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), input);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "requires a working XWayland panel and GPU"]
fn auto_backend_uses_window_for_x11_other_desktops_and_missing_wayland_socket() {
    let path =
        std::env::temp_dir().join(format!("openradar-auto-window-{}.toml", std::process::id()));
    std::fs::write(
        &path,
        "overlay_backend = 'auto'\noverlay_toggle_key = 'None'\n",
    )
    .unwrap();
    for (desktop, session, display) in [
        ("KDE", "x11", None),
        ("GNOME", "wayland", None),
        (
            "KDE",
            "wayland",
            Some("/missing-openradar-test-wayland-socket"),
        ),
    ] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_lfs-openradar"));
        command
            .args(["--demo", "--seconds", "1", "--config"])
            .arg(&path)
            .env("XDG_CURRENT_DESKTOP", desktop)
            .env("XDG_SESSION_TYPE", session);
        if let Some(display) = display {
            command
                .env("WAYLAND_DISPLAY", display)
                .env_remove("WAYLAND_SOCKET");
        }
        let output = command.output().unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{diagnostics}");
        assert!(
            diagnostics.contains("Overlay backend Auto · active Window"),
            "{diagnostics}"
        );
        assert!(
            !diagnostics.contains("Native Wayland GPU ready"),
            "{diagnostics}"
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "requires a real Wayland compositor and GPU"]
fn native_presentation_recovers_missing_selection_show_and_retry() {
    let mut config = Config::default();
    let source = SnapshotSource::Demo(std::sync::Arc::new(std::sync::Mutex::new(
        lfs_openradar::demo::Demo::default(),
    )));
    let mut overlay = NativeOverlay::start(
        config.clone(),
        true,
        source.clone(),
        eframe::egui::Context::default(),
    );
    let wait_ready = |overlay: &NativeOverlay| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !overlay.status().ready
            && overlay.status().error.is_none()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(overlay.status().ready, "{:?}", overlay.status().error);
    };
    wait_ready(&overlay);
    let selected = overlay.status().selected_monitor;
    config.wayland.monitor = Some("missing-test-output".into());
    overlay.update(config.clone(), false, source.clone());
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        !overlay.status().ready,
        "an unavailable output must not report a ready presentation"
    );
    assert!(!overlay.status().visible);
    config.wayland.monitor = selected;
    overlay.update(config.clone(), false, source.clone());
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!overlay.status().visible);
    overlay.update(config, true, source);
    wait_ready(&overlay);
    overlay.retry();
    wait_ready(&overlay);
    overlay.shutdown();
    assert!(!overlay.status().ready);
    assert!(!overlay.status().visible);
}

#[test]
#[ignore = "requires real XWayland and Wayland GPU presentation"]
fn actual_panel_and_native_eight_gadget_overlay_coexist_in_demo_mode() {
    let path =
        std::env::temp_dir().join(format!("openradar-native-app-{}.toml", std::process::id()));
    let config = Config {
        overlay_toggle_key: "None".into(),
        gap_ahead: lfs_openradar::config::GapSettings {
            enabled: true,
            ..Default::default()
        },
        gap_behind: lfs_openradar::config::GapSettings {
            enabled: true,
            ..Default::default()
        },
        performance_delta: lfs_openradar::config::GapSettings {
            enabled: true,
            ..Default::default()
        },
        speed_dashboard: lfs_openradar::config::GapSettings {
            enabled: true,
            ..Default::default()
        },
        fuel: lfs_openradar::config::GapSettings {
            enabled: true,
            ..Default::default()
        },
        lap_log: lfs_openradar::config::GapSettings {
            enabled: true,
            ..Default::default()
        },
        lap_records: lfs_openradar::config::GapSettings {
            enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };
    config.save(&path).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lfs-openradar"))
        .args([
            "--demo",
            "--overlay-backend",
            "wayland",
            "--seconds",
            "3",
            "--config",
        ])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{diagnostics}");
    assert!(
        diagnostics.contains("Native Wayland GPU ready"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires a real KDE/KWin Wayland session and GPU"]
fn native_gpu_presents_reused_gt_text_and_dashboard_and_exits_cleanly() {
    let mut config = Config::default();
    config.speed_dashboard.enabled = true;
    let source = SnapshotSource::Demo(std::sync::Arc::new(std::sync::Mutex::new(
        lfs_openradar::demo::Demo::default(),
    )));
    let mut overlay = NativeOverlay::start(config, true, source, eframe::egui::Context::default());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !overlay.status().ready
        && overlay.status().error.is_none()
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let status = overlay.status();
    overlay.shutdown();
    assert!(
        status.ready,
        "GPU overlay never became ready: {:?}",
        status.error
    );
    assert!(status.physical_size[0] > 0);
}

#[test]
fn failed_display_connection_keeps_controller_available_for_update_and_shutdown() {
    let mut overlay = NativeOverlay::with_connection(
        Config::default(),
        true,
        SnapshotSource::Disconnected,
        eframe::egui::Context::default(),
        Err("cannot connect to native Wayland".into()),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while overlay.status().error.is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        overlay.status().error.as_deref(),
        Some("cannot connect to native Wayland")
    );
    overlay.update(Config::default(), false, SnapshotSource::Disconnected);
    assert!(!overlay.status().visible);
    overlay.shutdown();
}
