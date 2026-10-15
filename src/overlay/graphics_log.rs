use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 1024 * 1024;

struct GraphicsLog {
    output: Mutex<(File, u64)>,
    suboptimal_present_logged: AtomicBool,
}
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
        // Presentation still succeeds; retain one diagnostic without per-frame noise.
        if record.level() == log::Level::Warn
            && record.target() == "wgpu_hal::vulkan"
            && record
                .args()
                .to_string()
                .starts_with("Suboptimal present of frame ")
            && self.suboptimal_present_logged.swap(true, Ordering::Relaxed)
        {
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
        if let Ok(mut output) = self.output.lock() {
            let (file, bytes) = &mut *output;
            if *bytes + line.len() as u64 <= MAX_BYTES && file.write_all(line.as_bytes()).is_ok() {
                *bytes += line.len() as u64;
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut output) = self.output.lock() {
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
            if log::set_boxed_logger(Box::new(GraphicsLog {
                output: Mutex::new((file, bytes)),
                suboptimal_present_logged: AtomicBool::new(false),
            }))
            .is_ok()
            {
                log::set_max_level(log::LevelFilter::Info);
                log::info!(target: "openradar_graphics", "Session started; graphics log {}", path.display());
            }
        }
        Err(error) => eprintln!("Cannot create graphics log {}: {error}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;

    #[test]
    fn repeated_suboptimal_presents_keep_first_and_other_diagnostics() {
        let path = std::env::temp_dir().join(format!(
            "openradar-graphics-log-{}-{}.log",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let logger = GraphicsLog {
            output: Mutex::new((File::create(&path).unwrap(), 0)),
            suboptimal_present_logged: AtomicBool::new(false),
        };
        for frame in [2, 0, 1, 2] {
            logger.log(
                &log::Record::builder()
                    .level(log::Level::Warn)
                    .target("wgpu_hal::vulkan")
                    .args(format_args!("Suboptimal present of frame {frame}"))
                    .build(),
            );
        }
        logger.log(
            &log::Record::builder()
                .level(log::Level::Warn)
                .target("wgpu_hal::vulkan")
                .args(format_args!("surface needs recovery"))
                .build(),
        );
        logger.log(
            &log::Record::builder()
                .level(log::Level::Error)
                .target("wgpu_hal::vulkan")
                .args(format_args!("gpu device lost"))
                .build(),
        );
        logger.flush();
        let output = std::fs::read_to_string(&path).unwrap();
        drop(logger);
        std::fs::remove_file(path).unwrap();
        assert_eq!(output.matches("Suboptimal present of frame").count(), 1);
        assert!(output.contains("Suboptimal present of frame 2"));
        assert!(output.contains("WARN wgpu_hal::vulkan: surface needs recovery"));
        assert!(output.contains("ERROR wgpu_hal::vulkan: gpu device lost"));
    }
}
