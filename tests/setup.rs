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
