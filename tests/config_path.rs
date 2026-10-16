#![cfg(target_os = "linux")]

use std::{path::Path, process::Command};

#[test]
fn linux_default_config_is_independent_of_launch_directory() {
    let root = std::env::temp_dir().join(format!("openradar-config-path-{}", std::process::id()));
    let home = root.join("home");
    let cwd = root.join("launch");
    let xdg = root.join("xdg");
    std::fs::create_dir_all(home.join(".config/lfs-openradar")).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    let home_config = home.join(".config/lfs-openradar/openradar.local.toml");
    std::fs::write(&home_config, "[speed_dashboard]\nenabled = true\n").unwrap();
    // A launch-folder config must never override the Linux default.
    std::fs::write(cwd.join("openradar.local.toml"), "invalid TOML").unwrap();
    let run = |directory: &Path, xdg_home: Option<&Path>, explicit: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lfs-openradar"));
        command
            .current_dir(directory)
            .env("HOME", &home)
            .env_remove("XDG_CONFIG_HOME")
            .args(["--demo", "--headless", "--seconds", "1"]);
        if let Some(xdg_home) = xdg_home {
            command.env("XDG_CONFIG_HOME", xdg_home);
        }
        if let Some(explicit) = explicit {
            command.arg("--config").arg(explicit);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).contains("Dashboard:")
    };
    assert!(run(&cwd, None, None));
    assert!(run(&root, None, None));
    for ignored in [Path::new(""), Path::new("relative-config")] {
        assert!(run(&cwd, Some(ignored), None));
    }
    // First launch creates the default directory so saving and locking work.
    assert!(!run(&cwd, Some(&xdg), None));
    assert!(xdg.join("lfs-openradar").is_dir());
    let xdg_config = xdg.join("lfs-openradar/openradar.local.toml");
    {
        let store = lfs_openradar::config::ConfigStore::open(&xdg_config).unwrap();
        store
            .lock()
            .unwrap()
            .save_settings(&lfs_openradar::config::Config::default())
            .unwrap();
    }
    assert!(!run(&cwd, Some(&xdg), None));
    std::fs::write(
        cwd.join("custom.toml"),
        "[speed_dashboard]\nenabled = true\n",
    )
    .unwrap();
    assert!(run(&cwd, Some(&xdg), Some(Path::new("custom.toml"))));
    assert!(run(&cwd, Some(&xdg), Some(&home_config)));
    let missing = Command::new(env!("CARGO_BIN_EXE_lfs-openradar"))
        .current_dir(&cwd)
        .env("HOME", &home)
        .args(["--demo", "--headless", "--config", "missing.toml"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(!cwd.join("missing.toml").exists());
    let no_home = Command::new(env!("CARGO_BIN_EXE_lfs-openradar"))
        .current_dir(&cwd)
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .args(["--demo", "--headless"])
        .output()
        .unwrap();
    assert!(!no_home.status.success());
    assert!(String::from_utf8_lossy(&no_home.stderr).contains("use --config"));
    let explicit_without_home = Command::new(env!("CARGO_BIN_EXE_lfs-openradar"))
        .current_dir(&cwd)
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .args(["--demo", "--headless", "--seconds", "1", "--config"])
        .arg(&home_config)
        .output()
        .unwrap();
    assert!(explicit_without_home.status.success());
    std::fs::remove_dir_all(root).unwrap();
}
