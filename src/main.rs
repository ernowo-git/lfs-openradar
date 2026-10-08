use lfs_openradar::{config::Config, demo::Demo, runtime::Runtime};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Options {
    config: Option<PathBuf>,
    demo: bool,
    headless: bool,
    seconds: Option<u64>,
    screenshot: Option<PathBuf>,
}
fn options() -> Result<Option<Options>, String> {
    let mut result = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "LFS OpenRadar\n\n  --demo                 Synthetic radar preview, no sockets\n  --headless             Console telemetry diagnostics\n  --config PATH          Load a TOML configuration\n  --seconds N            Exit after N seconds (1..3600)\n  --screenshot PATH      Save the demo control-panel preview as PNG\n\nDefault: native control panel; reads openradar.local.toml if present.\nInSim/MCI and OutSim are both required for live radar output."
                );
                return Ok(None);
            }
            "--demo" => result.demo = true,
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
    Ok(Some(result))
}
fn run() -> Result<(), String> {
    let Some(options) = options()? else {
        return Ok(());
    };
    let explicit_config = options.config.is_some();
    let path = options
        .config
        .unwrap_or_else(|| PathBuf::from("openradar.local.toml"));
    let config = if explicit_config || path.exists() {
        Config::load(&path)?
    } else {
        Config::default()
    };
    config.validate()?;
    if options.headless {
        let runtime = if options.demo {
            None
        } else {
            Some(Runtime::start(config.clone())?)
        };
        let mut demo = options.demo.then(Demo::default);
        let start = Instant::now();
        let mut next_print = 0;
        let limit = options.seconds.unwrap_or(10);
        while start.elapsed().as_secs() < limit {
            let now = start.elapsed().as_millis() as u64;
            let snapshot = if let Some(demo) = &mut demo {
                demo.snapshot(now, &config)
            } else {
                runtime.as_ref().unwrap().snapshot()
            };
            if now >= next_print {
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
                if let Some(error) = &snapshot.error {
                    eprintln!("{error}");
                }
                next_print = now + 1000;
            }
            std::thread::sleep(Duration::from_millis(20));
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
