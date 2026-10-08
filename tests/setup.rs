use lfs_openradar::{config::Config, setup};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Installation {
    root: PathBuf,
    temp_root: PathBuf,
}
impl Installation {
    fn new(script: Option<&[u8]>) -> Self {
        let temp_root = std::env::temp_dir().canonicalize().unwrap();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = temp_root.join(format!(
            "openradar-setup-test-{stamp}-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("LFS.exe"), b"test fixture, not executable").unwrap();
        fs::create_dir_all(root.join("data/script")).unwrap();
        let fixture = Self { root, temp_root };
        if let Some(bytes) = script {
            fs::write(fixture.script(), bytes).unwrap();
        }
        fixture
    }
    fn script(&self) -> PathBuf {
        self.root.join("data/script/autoexec.lfs")
    }
    fn files(&self) -> Vec<PathBuf> {
        fs::read_dir(self.root.join("data/script"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect()
    }
}
impl Drop for Installation {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(self.temp_root.as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("openradar-setup-test-")
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn appends_command_preserves_original_bytes_and_backs_up_once() {
    let original =
        b"\xef\xbb\xbf// Existing settings\r\n/ff 70\r\n// Legacy byte: \xe9\r\n/view custom";
    let fixture = Installation::new(Some(original));
    let plan = setup::prepare(&fixture.root, 29999).unwrap();
    assert!(plan.changes_script() && !plan.has_conflict());
    assert_eq!(fs::read(fixture.script()).unwrap(), original);
    let result = setup::apply(&plan).unwrap();
    let backup = result.backup_path.unwrap();
    assert_eq!(fs::read(&backup).unwrap(), original);
    let mut expected = original.to_vec();
    expected.extend_from_slice(b"\r\n/insim 29999\r\n");
    assert_eq!(fs::read(fixture.script()).unwrap(), expected);
    let plan = setup::prepare(&fixture.root, 29999).unwrap();
    let again = setup::apply(&plan).unwrap();
    assert!(!again.changed && again.backup_path.is_none());
    assert_eq!(fixture.files().len(), 2);
}
#[test]
fn existing_port_is_detected_without_edits_then_replaced_explicitly() {
    let original = b"// /insim 9000 is a comment\n/echo remember /insim 2000\n  /InSim = 29998  // startup\n/ff 80\n";
    let fixture = Installation::new(Some(original));
    let plan = setup::prepare(&fixture.root, 29999).unwrap();
    assert!(plan.has_conflict());
    assert_eq!(plan.existing_ports, [29998]);
    assert_eq!(fs::read(fixture.script()).unwrap(), original);
    assert_eq!(fixture.files().len(), 1);
    let keep = setup::prepare(&fixture.root, 29998).unwrap();
    assert!(!keep.changes_script());
    assert!(!setup::apply(&keep).unwrap().changed);
    setup::apply(&plan).unwrap();
    assert_eq!(fs::read(fixture.script()).unwrap(),
        b"// /insim 9000 is a comment\n/echo remember /insim 2000\n  /insim 29999  // startup\n/ff 80\n");
}
#[test]
fn missing_autoexec_is_created_without_a_backup() {
    let fixture = Installation::new(None);
    let plan = setup::prepare(&fixture.root, 30001).unwrap();
    let result = setup::apply(&plan).unwrap();
    assert!(result.changed);
    assert!(result.backup_path.is_none());
    let bytes = fs::read(fixture.script()).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap().trim(), "/insim 30001");
    assert_eq!(fixture.files().len(), 1);
}
#[test]
fn stale_plan_never_overwrites_a_users_edit_or_new_script() {
    for initial in [Some(b"/ff 50\n".as_slice()), None] {
        let fixture = Installation::new(initial);
        let plan = setup::prepare(&fixture.root, 29999).unwrap();
        fs::write(fixture.script(), b"/ff 80\n/insim 29998\n").unwrap();
        assert!(
            setup::apply(&plan)
                .unwrap_err()
                .contains("changed since inspection")
        );
        assert_eq!(
            fs::read(fixture.script()).unwrap(),
            b"/ff 80\n/insim 29998\n"
        );
        assert_eq!(fixture.files().len(), 1);
    }
}
#[test]
fn multiple_port_commands_are_preserved_and_backups_are_never_overwritten() {
    let original = b"/insim 0\n/ff 70\n/insim 29998\n/insim 29999\n";
    let fixture = Installation::new(Some(original));
    let plan = setup::prepare(&fixture.root, 29999).unwrap();
    assert_eq!(plan.existing_ports, [0, 29998, 29999]);
    let first = setup::apply(&plan).unwrap().backup_path.unwrap();
    let replacement = fs::read(fixture.script()).unwrap();
    assert_eq!(
        replacement,
        b"/insim 29999\n/ff 70\n/insim 29999\n/insim 29999\n"
    );
    let plan = setup::prepare(&fixture.root, 30001).unwrap();
    let second = setup::apply(&plan).unwrap().backup_path.unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read(first).unwrap(), original);
    assert_eq!(fs::read(second).unwrap(), replacement);
    assert_eq!(fixture.files().len(), 3);
}
#[test]
fn invalid_or_ambiguous_scripts_are_left_untouched() {
    for script in [
        b"/insim banana\n".as_slice(),
        b"/insim 65536\n",
        b"/insim 29999 /ff 70\n",
        b"/ff 70 /insim 29999\n",
        b"\xff\xfe/\0i\0n\0s\0i\0m\0",
        b"/insim\n",
    ] {
        let fixture = Installation::new(Some(script));
        assert!(setup::prepare(&fixture.root, 29999).is_err());
        assert_eq!(fs::read(fixture.script()).unwrap(), script);
        assert_eq!(fixture.files().len(), 1);
    }
}
#[test]
fn validates_installation_and_accepts_case_insensitive_executable_name() {
    let fixture = Installation::new(None);
    assert!(setup::prepare(&fixture.root, 0).is_err());
    // Rename via an intermediate name to work on case-insensitive filesystems too.
    fs::rename(fixture.root.join("LFS.exe"), fixture.root.join("temp.exe")).unwrap();
    fs::rename(fixture.root.join("temp.exe"), fixture.root.join("lfs.exe")).unwrap();
    assert!(setup::prepare(&fixture.root, 29999).is_ok());
    fs::remove_file(fixture.root.join("lfs.exe")).unwrap();
    assert!(setup::prepare(&fixture.root, 29999).is_err());
    assert!(setup::prepare(&fixture.root.join("data"), 29999).is_err());
    fs::write(fixture.root.join("LFS.exe"), b"fixture").unwrap();
    fs::create_dir(fixture.script()).unwrap();
    assert!(setup::prepare(&fixture.root, 29999).is_err());
}
#[test]
fn selected_lfs_folder_round_trips_with_legacy_config_defaults() {
    let legacy: Config = toml::from_str("side_m = 7.0").unwrap();
    assert!(legacy.lfs_directory.is_empty());
    let config = Config {
        lfs_directory: "C:\\Games\\Live for Speed".into(),
        ..legacy
    };
    let restored: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(restored.lfs_directory, config.lfs_directory);
}

#[test]
fn outsim_setup_preserves_other_settings_and_creates_exact_backup_once() {
    let fixture = Installation::new(None);
    let cfg = fixture.root.join("cfg.txt");
    let original = b"\xef\xbb\xbfGame Admin fixture-pass\r\nPlayer Name Andr\xe9\r\nOutGauge Port 30001\n  outSIM\tMode   0  // disabled\r\nOutSim Delay 10\r\nOutSim IP 0.0.0.0\r\nOutSim Port 0\r\nOutSim ID 0\r\nOutSim Opts 0\r\n";
    fs::write(&cfg, original).unwrap();
    let plan = setup::prepare_outsim(&fixture.root, &Config::default()).unwrap();
    assert_eq!(fs::read(&cfg).unwrap(), original);
    let result = setup::apply_outsim(&plan).unwrap();
    assert!(result.changed);
    assert_eq!(result.cfg_path, cfg);
    assert_eq!(result.backup_path, Some(fixture.root.join("cfg.txt.BAK")));
    assert_eq!(fs::read(result.backup_path.unwrap()).unwrap(), original);
    assert_eq!(fs::read(&cfg).unwrap(),
        b"\xef\xbb\xbfGame Admin fixture-pass\r\nPlayer Name Andr\xe9\r\nOutGauge Port 30001\n  outSIM\tMode   1  // disabled\r\nOutSim Delay 2\r\nOutSim IP 127.0.0.1\r\nOutSim Port 30000\r\nOutSim ID 24601\r\nOutSim Opts 1ff\r\n");
    let again =
        setup::apply_outsim(&setup::prepare_outsim(&fixture.root, &Config::default()).unwrap())
            .unwrap();
    assert!(!again.changed && again.backup_path.is_none());
    assert_eq!(
        fs::read(fixture.root.join("cfg.txt.BAK")).unwrap(),
        original
    );
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 4);
}

#[test]
fn outsim_setup_appends_missing_keys_and_matches_custom_and_legacy_formats() {
    for (options, id) in [(0x1ff, 99), (0xc, 0), (0, 77)] {
        let fixture = Installation::new(None);
        let cfg = fixture.root.join("cfg.txt");
        let original = b"OutGauge Mode 1\n// OutSim Mode 0\nGame Admin keep-me";
        fs::write(&cfg, original).unwrap();
        let config = Config {
            outsim_bind: "127.0.0.2:30123".parse().unwrap(),
            outsim_options: options,
            outsim_id: id,
            ..Default::default()
        };
        let result =
            setup::apply_outsim(&setup::prepare_outsim(&fixture.root, &config).unwrap()).unwrap();
        assert_eq!(fs::read(result.backup_path.unwrap()).unwrap(), original);
        assert_eq!(
            fs::read_to_string(cfg).unwrap(),
            format!(
                "OutGauge Mode 1\n// OutSim Mode 0\nGame Admin keep-me\nOutSim Mode 1\nOutSim Delay 2\nOutSim IP 127.0.0.2\nOutSim Port 30123\nOutSim ID {id}\nOutSim Opts {options:x}\n"
            )
        );
    }
}

#[test]
fn outsim_duplicate_keys_empty_values_and_attached_comments_are_updated() {
    let fixture = Installation::new(None);
    let cfg = fixture.root.join("cfg.txt");
    fs::write(&cfg, b"OutSim Mode 0;disabled\nOutSim Mode 2 # replay\nOutSim Delay\nOutSim ID   // missing\nOutSim Opts 0//legacy\nOutSim ModeExtra keep\nOutSim Port 12345\n").unwrap();
    setup::apply_outsim(&setup::prepare_outsim(&fixture.root, &Config::default()).unwrap())
        .unwrap();
    assert_eq!(
        fs::read_to_string(cfg).unwrap(),
        "OutSim Mode 1;disabled\nOutSim Mode 1 # replay\nOutSim Delay 2\nOutSim ID   24601 // missing\nOutSim Opts 1ff//legacy\nOutSim ModeExtra keep\nOutSim Port 30000\nOutSim IP 127.0.0.1\n"
    );
}

#[test]
fn existing_cfg_backup_is_preserved_and_later_edits_have_their_own_backup() {
    let fixture = Installation::new(None);
    let cfg = fixture.root.join("cfg.txt");
    let backup = fixture.root.join("cfg.txt.BAK");
    fs::write(&backup, b"older user backup").unwrap();
    fs::write(&cfg, b"Game Admin current\n").unwrap();
    let first =
        setup::apply_outsim(&setup::prepare_outsim(&fixture.root, &Config::default()).unwrap())
            .unwrap();
    let first_backup = first.backup_path.unwrap();
    assert_ne!(first_backup, backup);
    assert_eq!(fs::read(&first_backup).unwrap(), b"Game Admin current\n");
    let before = fs::read(&cfg).unwrap();
    let config = Config {
        outsim_bind: "127.0.0.1:30002".parse().unwrap(),
        ..Default::default()
    };
    let second =
        setup::apply_outsim(&setup::prepare_outsim(&fixture.root, &config).unwrap()).unwrap();
    let second_backup = second.backup_path.unwrap();
    assert_ne!(first_backup, second_backup);
    assert_eq!(fs::read(backup).unwrap(), b"older user backup");
    assert_eq!(fs::read(second_backup).unwrap(), before);
}

#[test]
fn outsim_stale_plan_and_readonly_cfg_leave_the_file_untouched() {
    let fixture = Installation::new(None);
    let cfg = fixture.root.join("cfg.txt");
    fs::write(&cfg, b"OutSim Mode 0\n").unwrap();
    let plan = setup::prepare_outsim(&fixture.root, &Config::default()).unwrap();
    fs::write(&cfg, b"user changed this\n").unwrap();
    assert!(
        setup::apply_outsim(&plan)
            .unwrap_err()
            .contains("changed since inspection")
    );
    assert_eq!(fs::read(&cfg).unwrap(), b"user changed this\n");
    assert!(!fixture.root.join("cfg.txt.BAK").exists());
    let permissions = fs::metadata(&cfg).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&cfg, readonly).unwrap();
    let result =
        setup::apply_outsim(&setup::prepare_outsim(&fixture.root, &Config::default()).unwrap());
    fs::set_permissions(&cfg, permissions).unwrap();
    assert!(result.unwrap_err().contains("read-only"));
    assert_eq!(fs::read(cfg).unwrap(), b"user changed this\n");
    assert!(!fixture.root.join("cfg.txt.BAK").exists());
}

#[test]
fn outsim_missing_invalid_files_installation_and_configuration_fail_before_writing() {
    let fixture = Installation::new(None);
    let cfg = fixture.root.join("cfg.txt");
    assert!(
        setup::prepare_outsim(&fixture.root, &Config::default())
            .unwrap_err()
            .contains("Run LFS once")
    );
    fs::create_dir(&cfg).unwrap();
    assert!(setup::prepare_outsim(&fixture.root, &Config::default()).is_err());
    fs::remove_dir(&cfg).unwrap();
    fs::write(&cfg, b"\xff\xfeG\0a\0m\0e\0").unwrap();
    assert!(setup::prepare_outsim(&fixture.root, &Config::default()).is_err());
    fs::write(&cfg, b"Game Admin original\n").unwrap();
    for config in [
        Config {
            outsim_bind: "192.168.0.1:30000".parse().unwrap(),
            ..Default::default()
        },
        Config {
            outsim_bind: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        },
        Config {
            outsim_options: 1,
            ..Default::default()
        },
        Config {
            outsim_options: 0xc,
            outsim_id: 99,
            ..Default::default()
        },
    ] {
        assert!(setup::prepare_outsim(&fixture.root, &config).is_err());
    }
    assert_eq!(fs::read(&cfg).unwrap(), b"Game Admin original\n");
    assert!(!fixture.root.join("cfg.txt.BAK").exists());
    fs::remove_file(fixture.root.join("LFS.exe")).unwrap();
    assert!(setup::prepare_outsim(&fixture.root, &Config::default()).is_err());
}

#[test]
fn outgauge_setup_preserves_outsim_and_other_settings_with_backup() {
    let fixture = Installation::new(None);
    let cfg = fixture.root.join("cfg.txt");
    let original = b"Game Admin unchanged\r\nOutSim Port 30000\r\nOutGauge Mode 0 // keep comment\r\nOutGauge Port 1234\r\n";
    fs::write(&cfg, original).unwrap();
    let plan = setup::prepare_outgauge(&fixture.root, &Config::default()).unwrap();
    let result = setup::apply_outsim(&plan).unwrap();
    assert_eq!(fs::read(result.backup_path.unwrap()).unwrap(), original);
    let bytes = fs::read(&cfg).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("OutSim Port 30000"));
    assert!(text.contains("Game Admin unchanged"));
    assert!(text.contains("OutGauge Mode 1 // keep comment"));
    assert!(text.contains("OutGauge Port 30001"));
    assert!(text.contains("OutGauge ID 24602"));
    assert!(
        !setup::apply_outsim(&setup::prepare_outgauge(&fixture.root, &Config::default()).unwrap())
            .unwrap()
            .changed
    );
}
