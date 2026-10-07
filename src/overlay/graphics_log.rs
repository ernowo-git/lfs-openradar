use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 1024 * 1024;

struct GraphicsLog(Mutex<(File, u64)>);
impl log::Log for GraphicsLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        matches!(metadata.target(), "openradar_graphics" | "openradar_timing")
            || (metadata.level() <= log::Level::Warn
                && ["wgpu", "eframe", "egui_wgpu", "egui_winit", "winit"]
                    .iter()
                    .any(|prefix| metadata.target().starts_with(prefix)))
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let line = format!(
            "{timestamp} {} {}: {}\n",
            record.level(),
            record.target(),
            record.args()
        );
        eprint!("{line}");
        if let Ok(mut output) = self.0.lock() {
            let (file, bytes) = &mut *output;
            if *bytes + line.len() as u64 <= MAX_BYTES && file.write_all(line.as_bytes()).is_ok() {
                *bytes += line.len() as u64;
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut output) = self.0.lock() {
            let _ = output.0.flush();
        }
    }
}

/// Append across launches so a failed session is retained. Reset only when the
/// bounded file is already full. Timing transitions are included; raw telemetry
/// and passwords are excluded.
pub(super) fn init(config_path: &Path) {
    let path = config_path.with_file_name("openradar.graphics.log");
    let full = path
        .metadata()
        .is_ok_and(|metadata| metadata.len() >= MAX_BYTES - 4096);
    let result = OpenOptions::new()
        .create(true)
        .write(true)
        .append(!full)
        .truncate(full)
        .open(&path);
    match result {
        Ok(file) => {
            let bytes = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            if log::set_boxed_logger(Box::new(GraphicsLog(Mutex::new((file, bytes))))).is_ok() {
                log::set_max_level(log::LevelFilter::Info);
                log::info!(target: "openradar_graphics", "Session started; graphics log {}", path.display());
            }
        }
        Err(error) => eprintln!("Cannot create graphics log {}: {error}", path.display()),
    }
}
