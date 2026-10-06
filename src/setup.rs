//! Explicit, backed-up configuration of the selected LFS installation's startup script.
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
        let name = format!("autoexec.lfs.openradar-{stamp}-{attempt}.{suffix}");
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
    Err("Cannot create a unique startup-script backup".into())
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
