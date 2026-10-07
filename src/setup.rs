//! Explicit, backed-up configuration of the selected LFS installation.
use crate::config::Config;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub struct SetupPlan {
    pub script_path: PathBuf,
    pub existing_ports: Vec<u16>,
    pub target_port: u16,
    original: Option<Vec<u8>>,
    replacement: Vec<u8>,
}
impl SetupPlan {
    pub fn has_conflict(&self) -> bool {
        self.existing_ports.iter().any(|p| *p != self.target_port)
    }
    pub fn changes_script(&self) -> bool {
        self.original.as_ref() != Some(&self.replacement)
    }
}
#[derive(Debug)]
pub struct SetupResult {
    pub script_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub changed: bool,
}

fn read_script(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
            if meta.len() > MAX_SCRIPT_BYTES {
                return Err("Startup script is larger than 1 MiB; edit it manually".into());
            }
            fs::read(path)
                .map(Some)
                .map_err(|e| format!("Cannot read {}: {e}", path.display()))
        }
        Ok(_) => Err("autoexec.lfs must be a regular file, not a link or directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot inspect {}: {e}", path.display())),
    }
}

/// Inspection is read-only. Require a recognizable LFS installation rather than
/// silently creating a data/script tree in a mistyped folder.
pub fn prepare(directory: &Path, port: u16) -> Result<SetupPlan, String> {
    if port == 0 {
        return Err("InSim port must be nonzero".into());
    }
    let root = installation_root(directory)?;
    let scripts = root
        .join("data")
        .join("script")
        .canonicalize()
        .map_err(|_| "The selected LFS folder has no data/script directory".to_string())?;
    if !scripts.is_dir() || !scripts.starts_with(&root) {
        return Err("LFS data/script must be a directory inside the selected installation".into());
    }
    let script_path = scripts.join("autoexec.lfs");
    let original = read_script(&script_path)?;
    let bytes = original.as_deref().unwrap_or_default();
    let (replacement, existing_ports) = update_script(bytes, port)?;
    Ok(SetupPlan {
        script_path,
        existing_ports,
        target_port: port,
        original,
        replacement,
    })
}

fn installation_root(directory: &Path) -> Result<PathBuf, String> {
    let root = directory
        .canonicalize()
        .map_err(|e| format!("Cannot open LFS folder: {e}"))?;
    let has_exe = fs::read_dir(&root)
        .map_err(|e| format!("Cannot read LFS folder: {e}"))?
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("LFS.exe")
                && entry.path().is_file()
        });
    if !has_exe {
        return Err("Choose the LFS installation folder containing LFS.exe".into());
    }
    Ok(root)
}

/// Preserve bytes outside the InSim command, including legacy encodings, comments,
/// UTF-8 BOMs, mixed line endings, whitespace, and unrelated startup commands.
fn update_script(bytes: &[u8], port: u16) -> Result<(Vec<u8>, Vec<u16>), String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return Err("Use an ANSI or UTF-8 autoexec.lfs; UTF-16 scripts cannot be updated".into());
    }
    let bom = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    let mut edits = Vec::new();
    let mut ports = Vec::new();
    let mut offset = bom;
    for line in bytes[bom..].split_inclusive(|b| *b == b'\n') {
        let leading = line.iter().take_while(|b| b.is_ascii_whitespace()).count();
        let content = &line[leading..];
        if content.is_empty() || content.starts_with(b"//") {
            offset += line.len();
            continue;
        }
        let lower: Vec<u8> = content.iter().map(u8::to_ascii_lowercase).collect();
        let command = lower.starts_with(b"/insim")
            && lower
                .get(6)
                .is_none_or(|b| b.is_ascii_whitespace() || *b == b'=');
        if command {
            let mut number = 6;
            while content.get(number).is_some_and(u8::is_ascii_whitespace) {
                number += 1;
            }
            if content.get(number) == Some(&b'=') {
                number += 1;
            }
            while content.get(number).is_some_and(u8::is_ascii_whitespace) {
                number += 1;
            }
            let mut end = number;
            while content.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            let old = std::str::from_utf8(&content[number..end])
                .ok()
                .and_then(|n| n.parse::<u16>().ok())
                .ok_or("Cannot read an existing /insim port; edit that command manually")?;
            let trailing = &content[end..];
            let trailing = &trailing[trailing
                .iter()
                .take_while(|b| b.is_ascii_whitespace())
                .count()..];
            if !trailing.is_empty() && !trailing.starts_with(b"//") {
                return Err(
                    "Put each /insim command on its own line before using startup setup".into(),
                );
            }
            ports.push(old);
            if old != port {
                edits.push((offset + leading, offset + leading + end));
            }
        } else if lower.windows(6).any(|w| w == b"/insim") {
            // Text commands can mention /insim without executing it. Other combined
            // command lines require manual separation to avoid editing their meaning.
            let text_command = [
                b"/say".as_slice(),
                b"/echo",
                b"/join",
                b"/rcm",
                b"/pass",
                b"/msg",
                b"/altf",
                b"/ctrlf",
            ]
            .iter()
            .any(|cmd| {
                lower.starts_with(cmd) && lower.get(cmd.len()).is_some_and(u8::is_ascii_whitespace)
            });
            if !text_command {
                return Err(
                    "Put each /insim command on its own line before using startup setup".into(),
                );
            }
        }
        offset += line.len();
    }
    let command = format!("/insim {port}");
    let mut updated = bytes.to_vec();
    for (start, end) in edits.into_iter().rev() {
        updated.splice(start..end, command.bytes());
    }
    if ports.is_empty() {
        let newline: &[u8] =
            if bytes.windows(2).any(|b| b == b"\r\n") || (bytes.is_empty() && cfg!(windows)) {
                b"\r\n"
            } else {
                b"\n"
            };
        if updated.len() > bom && !updated.ends_with(b"\n") {
            updated.extend_from_slice(newline);
        }
        updated.extend_from_slice(command.as_bytes());
        updated.extend_from_slice(newline);
    }
    ports.sort_unstable();
    ports.dedup();
    Ok((updated, ports))
}

fn create_unique(
    path: &Path,
    suffix: &str,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..100 {
        let name = format!(
            "{}.openradar-{stamp}-{attempt}.{suffix}",
            path.file_name().unwrap().to_string_lossy()
        );
        let target = path.with_file_name(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                let result = (|| {
                    if let Some(permissions) = &permissions {
                        file.set_permissions(permissions.clone())?;
                    }
                    file.write_all(bytes)?;
                    file.sync_all()
                })();
                if let Err(e) = result {
                    drop(file);
                    let _ = fs::remove_file(&target);
                    return Err(format!("Cannot write {}: {e}", target.display()));
                }
                return Ok(target);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create {}: {e}", target.display())),
        }
    }
    Err("Cannot create a unique setup file".into())
}

#[derive(Debug)]
pub struct OutSimPlan {
    pub cfg_path: PathBuf,
    original: Vec<u8>,
    replacement: Vec<u8>,
}
#[derive(Debug)]
pub struct OutSimResult {
    pub cfg_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub changed: bool,
}

fn read_cfg(path: &Path) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "cfg.txt is missing. Run LFS once, close it, then configure OutSim.".into()
        } else {
            format!("Cannot inspect cfg.txt: {e}")
        }
    })?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("cfg.txt must be a regular file, not a link or directory".into());
    }
    if meta.len() > MAX_SCRIPT_BYTES {
        return Err("cfg.txt is larger than 1 MiB; edit it manually".into());
    }
    fs::read(path).map_err(|e| format!("Cannot read cfg.txt: {e}"))
}

/// Read-only inspection. The GUI prepares and applies this plan in one click.
pub fn prepare_outsim(directory: &Path, config: &Config) -> Result<OutSimPlan, String> {
    if !config.outsim_bind.is_ipv4()
        || !config.outsim_bind.ip().is_loopback()
        || config.outsim_bind.port() == 0
    {
        return Err("OutSim setup requires an IPv4 loopback address and nonzero UDP port".into());
    }
    crate::lfs::outsim::packet_size(config.outsim_options, config.expected_outsim_id())?;
    if config.outsim_options != 0 && config.outsim_id != 0 && config.outsim_options & 2 == 0 {
        return Err("OutSim options must include ID when an OutSim ID is configured".into());
    }
    let cfg_path = installation_root(directory)?.join("cfg.txt");
    let original = read_cfg(&cfg_path)?;
    let replacement = update_outsim_cfg(&original, config)?;
    Ok(OutSimPlan {
        cfg_path,
        original,
        replacement,
    })
}

fn update_outsim_cfg(bytes: &[u8], config: &Config) -> Result<Vec<u8>, String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return Err("Use an ANSI or UTF-8 cfg.txt; UTF-16 files cannot be updated".into());
    }
    let settings = [
        (b"Mode".as_slice(), "1".to_string()),
        (b"Delay".as_slice(), "2".to_string()),
        (b"IP".as_slice(), config.outsim_bind.ip().to_string()),
        (b"Port".as_slice(), config.outsim_bind.port().to_string()),
        (b"ID".as_slice(), config.outsim_id.to_string()),
        (b"Opts".as_slice(), format!("{:x}", config.outsim_options)),
    ];
    let bom = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    let mut seen = [false; 6];
    let mut updated = bytes[..bom].to_vec();
    for line in bytes[bom..].split_inclusive(|b| *b == b'\n') {
        // Match two complete key tokens, retaining indentation, separators, and
        // trailing comments. All duplicate active entries receive the same value.
        let mut cursor = 0;
        let mut token = || {
            while line.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            let start = cursor;
            while line.get(cursor).is_some_and(|b| !b.is_ascii_whitespace()) {
                cursor += 1;
            }
            &line[start..cursor]
        };
        let prefix = token();
        let key = token();
        if prefix.eq_ignore_ascii_case(b"OutSim")
            && let Some(index) = settings
                .iter()
                .position(|(name, _)| key.eq_ignore_ascii_case(name))
        {
            seen[index] = true;
            let separator_start = cursor;
            while line.get(cursor).is_some_and(|b| *b == b' ' || *b == b'\t') {
                cursor += 1;
            }
            let start = cursor;
            let comment = line[start..].starts_with(b"//")
                || line[start..].starts_with(b";")
                || line[start..].starts_with(b"#");
            if !comment {
                while line
                    .get(cursor)
                    .is_some_and(|b| !b.is_ascii_whitespace() && *b != b';' && *b != b'#')
                    && !line[cursor..].starts_with(b"//")
                {
                    cursor += 1;
                }
            }
            updated.extend_from_slice(&line[..start]);
            if separator_start == start {
                updated.push(b' ');
            }
            updated.extend_from_slice(settings[index].1.as_bytes());
            if comment {
                updated.push(b' ');
            }
            updated.extend_from_slice(&line[cursor..]);
        } else {
            updated.extend_from_slice(line);
        }
    }
    let newline: &[u8] =
        if bytes.windows(2).any(|b| b == b"\r\n") || (bytes.is_empty() && cfg!(windows)) {
            b"\r\n"
        } else {
            b"\n"
        };
    for (index, (key, value)) in settings.iter().enumerate() {
        if !seen[index] {
            if updated.len() > bom && !updated.ends_with(b"\n") {
                updated.extend_from_slice(newline);
            }
            updated.extend_from_slice(b"OutSim ");
            updated.extend_from_slice(key);
            updated.push(b' ');
            updated.extend_from_slice(value.as_bytes());
            updated.extend_from_slice(newline);
        }
    }
    Ok(updated)
}

fn cfg_backup(
    path: &Path,
    original: &[u8],
    permissions: fs::Permissions,
) -> Result<PathBuf, String> {
    let target = path.with_file_name("cfg.txt.BAK");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target);
    match file {
        Ok(mut file) => {
            let result = (|| {
                file.set_permissions(permissions)?;
                file.write_all(original)?;
                file.sync_all()
            })();
            if let Err(e) = result {
                drop(file);
                let _ = fs::remove_file(&target);
                return Err(format!("Cannot write cfg.txt.BAK: {e}"));
            }
            Ok(target)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            create_unique(path, "bak", original, Some(permissions))
        }
        Err(e) => Err(format!("Cannot create cfg.txt.BAK: {e}")),
    }
}

/// Apply only after the user's Configure OutSim button action. Never overwrite
/// an existing backup, a stale cfg.txt, or unrelated configuration bytes.
pub fn apply_outsim(plan: &OutSimPlan) -> Result<OutSimResult, String> {
    if read_cfg(&plan.cfg_path)? != plan.original {
        return Err("cfg.txt changed since inspection. Click Configure OutSim again.".into());
    }
    if plan.original == plan.replacement {
        return Ok(OutSimResult {
            cfg_path: plan.cfg_path.clone(),
            backup_path: None,
            changed: false,
        });
    }
    let permissions = fs::metadata(&plan.cfg_path)
        .map_err(|e| format!("Cannot inspect cfg.txt: {e}"))?
        .permissions();
    if permissions.readonly() {
        return Err("cfg.txt is read-only; make it writable before configuring OutSim.".into());
    }
    let backup_path = cfg_backup(&plan.cfg_path, &plan.original, permissions.clone())?;
    let temporary = create_unique(&plan.cfg_path, "tmp", &plan.replacement, Some(permissions))?;
    let result = (|| {
        if read_cfg(&plan.cfg_path)? != plan.original {
            return Err("cfg.txt changed during setup; no changes were applied. Try again.".into());
        }
        replace_script(&temporary, &plan.cfg_path)
            .map_err(|e| format!("Cannot update cfg.txt: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(OutSimResult {
        cfg_path: plan.cfg_path.clone(),
        backup_path: Some(backup_path),
        changed: true,
    })
}

/// Call only after an explicit setup-button action, including conflict resolution.
/// Refuse stale plans so user edits between inspection and application survive.
pub fn apply(plan: &SetupPlan) -> Result<SetupResult, String> {
    if read_script(&plan.script_path)? != plan.original {
        return Err(
            "autoexec.lfs changed since inspection. Click Enable InSim at startup again".into(),
        );
    }
    if !plan.changes_script() {
        return Ok(SetupResult {
            script_path: plan.script_path.clone(),
            backup_path: None,
            changed: false,
        });
    }
    let permissions = fs::metadata(&plan.script_path)
        .ok()
        .map(|m| m.permissions());
    let backup_path = plan
        .original
        .as_ref()
        .map(|bytes| create_unique(&plan.script_path, "bak", bytes, permissions.clone()))
        .transpose()?;
    let temporary = create_unique(&plan.script_path, "tmp", &plan.replacement, permissions)?;
    let result = (|| {
        if read_script(&plan.script_path)? != plan.original {
            return Err(
                "autoexec.lfs changed during setup; no changes were applied. Try again".into(),
            );
        }
        replace_script(&temporary, &plan.script_path)
            .map_err(|e| format!("Cannot update startup script: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(SetupResult {
        script_path: plan.script_path.clone(),
        backup_path,
        changed: true,
    })
}

#[cfg(not(windows))]
fn replace_script(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn replace_script(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both paths are NUL-terminated UTF-16 buffers valid for this call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
