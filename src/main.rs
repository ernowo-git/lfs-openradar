use lfs_openradar::{
    config::{Config, OverlayBackend},
    demo::Demo,
    runtime::Runtime,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Options {
    overlay_backend: Option<OverlayBackend>,
    config: Option<PathBuf>,
    demo: bool,
    headless: bool,
    seconds: Option<u64>,
    screenshot: Option<PathBuf>,
    no_insim: bool,
    record: Option<PathBuf>,
    track: String,
    recover: Option<PathBuf>,
}
fn options() -> Result<Option<Options>, String> {
    let mut result = Options {
        track: "BL1".into(),
        ..Default::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "LFS OpenRadar\n\n  --demo                 Synthetic radar preview, no sockets\n  --headless             Console telemetry diagnostics\n  --config PATH          Load a TOML configuration\n  --seconds N            Exit after N seconds (1..3600)\n  --screenshot PATH      Save the demo control-panel preview as PNG\n\nDefault: native control panel.\nInSim/MCI and OutSim are both required for live radar output."
                );
                #[cfg(target_os = "linux")]
                println!(
                    "Default config: $XDG_CONFIG_HOME/lfs-openradar/openradar.local.toml (fallback: ~/.config/lfs-openradar/openradar.local.toml)."
                );
                #[cfg(not(target_os = "linux"))]
                println!("Default config: openradar.local.toml in the working directory.");
                println!(
                    "  --overlay-backend NAME Choose auto (default), window, or wayland (Linux/KDE); overrides the saved choice."
                );
                println!(
                    "  --no-insim             Disable InSim; local recording stays available\n  --record DIRECTORY     Record local UDP telemetry (headless); Enter stops if no --seconds\n  --track CODE           User-declared recording track (default BL1)\n  --recover-recording P  Recover complete lines from an interrupted recording"
                );
                return Ok(None);
            }
            "--no-insim" => result.no_insim = true,
            "--record" => {
                result.record = Some(PathBuf::from(
                    args.next().ok_or("--record needs a directory")?,
                ));
                result.headless = true;
            }
            "--track" => result.track = args.next().ok_or("--track needs a track code")?,
            "--recover-recording" => {
                result.recover = Some(PathBuf::from(
                    args.next().ok_or("--recover-recording needs a path")?,
                ))
            }
            "--demo" => result.demo = true,
            "--overlay-backend" => {
                result.overlay_backend = Some(match args.next().as_deref() {
                    Some("auto") => OverlayBackend::Auto,
                    Some("window") => OverlayBackend::Window,
                    Some("wayland") => OverlayBackend::Wayland,
                    _ => return Err("--overlay-backend requires auto, window, or wayland".into()),
                });
            }
            "--headless" => result.headless = true,
            "--config" => {
                result.config = Some(PathBuf::from(args.next().ok_or("--config needs a path")?))
            }
            "--seconds" => {
                let seconds = args
                    .next()
                    .ok_or("--seconds needs a number")?
                    .parse::<u64>()
                    .map_err(|_| "Invalid seconds")?;
                if !(1..=3600).contains(&seconds) {
                    return Err("Seconds must be 1..3600".into());
                }
                result.seconds = Some(seconds);
            }
            "--screenshot" => {
                result.screenshot = Some(PathBuf::from(
                    args.next().ok_or("--screenshot needs a path")?,
                ))
            }
            _ => return Err(format!("Unknown option: {arg}. Use --help.")),
        }
    }
    if result.screenshot.is_some()
        && (!result.demo || result.headless || result.seconds.is_some_and(|n| n < 3))
    {
        return Err("--screenshot requires a desktop --demo with at least 3 seconds".into());
    }
    if result.record.is_some() && result.demo {
        return Err("--record cannot be used with --demo".into());
    }
    Ok(Some(result))
}

fn config_path(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|home| home.join(".config"))
            })
            .ok_or(
                "cannot locate config directory; set HOME or XDG_CONFIG_HOME, or use --config",
            )?;
        let directory = base.join("lfs-openradar");
        std::fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "cannot create config directory {}: {error}",
                directory.display()
            )
        })?;
        Ok(directory.join("openradar.local.toml"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(PathBuf::from("openradar.local.toml"))
    }
}

fn run() -> Result<(), String> {
    let Some(options) = options()? else {
        return Ok(());
    };
    if let Some(source) = options.recover {
        let mut name = source.as_os_str().to_os_string();
        name.push(".recovered.jsonl");
        let destination = PathBuf::from(name);
        lfs_openradar::recording::recover(&source, &destination)?;
        println!("Recovered interrupted recording: {}", destination.display());
        return Ok(());
    }
    let explicit_config = options.config.is_some();
    let path = config_path(options.config)?;
    let mut config = if explicit_config || path.exists() {
        Config::load(&path)?
    } else {
        Config::default()
    };
    if let Some(backend) = options.overlay_backend {
        config.overlay_backend = backend;
    }
    if options.no_insim {
        config.insim_enabled = false;
    }
    config.validate()?;
    if options.headless {
        let mut runtime = if options.demo {
            None
        } else {
            Some(Runtime::start_with_store(
                config.clone(),
                config.effective_insim_password(),
                Some(lfs_openradar::config::ConfigStore::open(&path)?),
            )?)
        };
        let recording = options.record.is_some();
        if let Some(directory) = options.record {
            runtime
                .as_mut()
                .unwrap()
                .start_recording(lfs_openradar::recording::Options {
                    directory,
                    track: options.track,
                    ..Default::default()
                })?;
            println!(
                "Recording local telemetry; track is user-declared. No official timing or identity verification."
            );
        }
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if recording && options.seconds.is_none() {
            println!("Press Enter to stop and finalize the recording.");
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }
        let mut demo = options.demo.then(Demo::default);
        let start = Instant::now();
        let mut next_print = 0;
        let limit = options.seconds.or((!recording).then_some(10));
        while limit.is_none_or(|limit| start.elapsed().as_secs() < limit)
            && !stop.load(std::sync::atomic::Ordering::Relaxed)
        {
            let now = start.elapsed().as_millis() as u64;
            let snapshot = if let Some(demo) = &mut demo {
                demo.snapshot(now, &config)
            } else {
                runtime.as_ref().unwrap().snapshot()
            };
            if let Some(status) = runtime.as_ref().and_then(|r| r.recording_status())
                && let Some(error) = status.error
            {
                return Err(error);
            }
            if now >= next_print {
                if let Some(status) = runtime.as_ref().and_then(|r| r.recording_status()) {
                    println!(
                        "Recording: {}ms OutSim={} OutGauge={} gaps={} rejected={} bytes={} | {}",
                        status.elapsed_ms,
                        status.packets[0],
                        status.packets[1],
                        status.gaps,
                        status.rejected,
                        status.bytes,
                        status.path.display()
                    );
                }
                println!(
                    "{} | MCI sets={} OutSim={} rejected={} malformed={} nearby={} ahead={:?}s behind={:?}s",
                    snapshot.frame.status,
                    snapshot.mci_sets,
                    snapshot.outsim_samples,
                    snapshot.rejected_outsim,
                    snapshot.malformed_packets,
                    snapshot.frame.cars.len(),
                    snapshot.gaps.ahead.seconds,
                    snapshot.gaps.behind.seconds
                );
                if config.speed_dashboard.enabled {
                    println!(
                        "Dashboard: {} | sample={:?} age={:?}ms headlights={:?}",
                        snapshot.dashboard.status,
                        snapshot.dashboard.sample,
                        snapshot.dashboard.age_ms,
                        snapshot.dashboard.headlight_switch
                    );
                    if let Some(error) = &snapshot.outgauge_error {
                        eprintln!("outgauge: {error}");
                    }
                }
                if config.fuel.enabled {
                    println!(
                        "Fuel: {} | range={:?} laps margin={:?} laps refuel={:?}%",
                        snapshot.fuel.status,
                        snapshot.fuel.rows[0].laps,
                        snapshot.fuel.margin,
                        snapshot.fuel.rows[0].refuel.map(|value| value * 100.0)
                    );
                    if let Some(error) = &snapshot.outgauge_error {
                        eprintln!("outgauge: {error}");
                    }
                }
                if config.lap_log.enabled {
                    println!(
                        "Lap-time log: best={:?}ms latest={:?}",
                        snapshot.lap_log.best_ms,
                        snapshot.lap_log.laps.first()
                    );
                }
                if config.lap_records.enabled {
                    println!(
                        "Lap records: {} | {} / {} PB={:?}ms sectors={:?}ms",
                        snapshot.lap_records.status,
                        snapshot.lap_records.track.as_deref().unwrap_or("—"),
                        snapshot.lap_records.car.as_deref().unwrap_or("—"),
                        snapshot.lap_records.record.as_ref().map(|r| r.pb.lap_ms),
                        snapshot
                            .lap_records
                            .record
                            .as_ref()
                            .map(|r| &r.best_sectors_ms)
                    );
                }
                if let Some(error) = &snapshot.lap_records_error {
                    eprintln!("{error}");
                }
                if let Some(error) = &snapshot.error {
                    eprintln!("{error}");
                }
                next_print = now + 1000;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(status) = runtime.as_mut().and_then(|r| r.stop_recording()) {
            println!(
                "Saved recording: {} (OutSim={}, OutGauge={}, gaps={})",
                status.path.display(),
                status.packets[0],
                status.packets[1],
                status.gaps
            );
            if let Some(error) = status.error {
                return Err(error);
            }
        }
        return Ok(());
    }
    #[cfg(feature = "desktop")]
    {
        lfs_openradar::overlay::run(
            config,
            path,
            options.demo,
            options.seconds,
            options.screenshot,
        )
    }
    #[cfg(not(feature = "desktop"))]
    {
        Err("Desktop feature disabled. Use --headless or build with default features.".into())
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("lfs-openradar: {error}");
        std::process::exit(1);
    }
}
