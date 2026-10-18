//! Throwaway offline report: reuse the production decoders, leave the HUD alone.
use lfs_openradar::{
    lfs::{outgauge, outsim},
    recording::{Event, Stream},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::Path,
};

#[derive(Serialize)]
struct Capture {
    source: String,
    track: String,
    context_verified: bool,
    complete: bool,
    cars: BTreeSet<String>,
    // ponytail: keep reports in memory; chunk only if long captures exceed browser memory.
    frames: Vec<Frame>,
}

#[derive(Serialize)]
struct Frame {
    time_ms: u32,
    received_us: u64,
    speed_mps: f64,
    // Load N, rotation rad/s, air °C, slip fraction, touching, slip ratio,
    // tan(slip angle), force right N, force forward N. No corner assumptions.
    wheels: [[f64; 9]; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    boundary: Option<String>,
}

fn hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() > 2048 || !value.len().is_multiple_of(2) || !value.is_ascii() {
        return Err("invalid packet hex length or encoding".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                .map_err(|_| "invalid packet hex digit".into())
        })
        .collect()
}

fn read_capture(path: &Path) -> Result<Capture, String> {
    let mut input = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let mut capture = Capture {
        source: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        track: String::new(),
        context_verified: false,
        complete: false,
        cars: BTreeSet::new(),
        frames: Vec::new(),
    };
    let mut layout = None;
    let mut ended = false;
    let mut packet_count = 0;
    let mut car = None;
    let mut boundary = None;
    let mut line_number = 0;
    loop {
        let mut line = Vec::new();
        let size = input
            .by_ref()
            .take(16_385)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        line_number += 1;
        if size > 16_384 || !line.ends_with(b"\n") {
            return Err(format!(
                "invalid or truncated recording line {line_number}; recover the recording first"
            ));
        }
        if ended {
            return Err("recording contains data after its footer".into());
        }
        let event: Event = serde_json::from_slice(&line)
            .map_err(|e| format!("invalid recording line {line_number}: {e}"))?;
        if layout.is_none() {
            let Event::Header {
                format,
                version: 1,
                declared_track,
                context_verified,
                outsim_options,
                outsim_id,
                outgauge_id,
                ..
            } = event
            else {
                return Err("expected a version 1 recording header".into());
            };
            if format != "openradar-recording" || outsim_options & 128 == 0 {
                return Err(
                    "prototype requires an OpenRadar recording with OutSim wheel data (Opts 1ff)"
                        .into(),
                );
            }
            outsim::packet_size(outsim_options, outsim_id)?;
            capture.track = declared_track;
            capture.context_verified = context_verified;
            layout = Some((outsim_options, outsim_id, outgauge_id));
            continue;
        }
        let (options, outsim_id, outgauge_id) = layout.unwrap();
        match event {
            Event::Header { .. } => return Err("recording contains a second header".into()),
            Event::Footer {
                complete,
                error,
                packets,
                ..
            } => {
                if packets != packet_count {
                    return Err(
                        "recording footer packet count does not match packet sequence".into(),
                    );
                }
                capture.complete = complete && error.is_none();
                ended = true;
            }
            Event::Gap {
                stream: None | Some(Stream::Outsim),
                reason,
                ..
            } => {
                boundary = Some(format!("recorded gap: {reason}"));
            }
            Event::Gap { .. } => {}
            Event::Packet {
                sequence: next,
                stream,
                source_time_ms,
                received_us,
                data_hex,
            } => {
                if next != packet_count {
                    return Err("recording packet sequence is discontinuous".into());
                }
                packet_count += 1;
                let bytes = hex(&data_hex)?;
                match stream {
                    Stream::Outgauge => {
                        let sample = outgauge::decode(&bytes, outgauge_id)?;
                        if sample.time_ms != source_time_ms {
                            return Err("OutGauge clock disagrees with recording metadata".into());
                        }
                        let identity = (sample.car.clone(), sample.plid);
                        if car.as_ref() != Some(&identity) && !capture.frames.is_empty() {
                            boundary = Some("car identity changed or became known".into());
                        }
                        car = Some(identity);
                        capture.cars.insert(sample.car);
                    }
                    Stream::Outsim => {
                        let telemetry = outsim::decode_telemetry(&bytes, options, outsim_id)?;
                        if telemetry.sample.time_ms != source_time_ms {
                            return Err("OutSim clock disagrees with recording metadata".into());
                        }
                        let speed_mps = telemetry
                            .velocity
                            .iter()
                            .map(|v| f64::from(*v).powi(2))
                            .sum::<f64>()
                            .sqrt();
                        let wheels = telemetry.wheels.ok_or("missing OutSim wheels")?.map(|w| {
                            [
                                f64::from(w.vertical_load_n),
                                f64::from(w.angular_velocity_rad_s),
                                f64::from(w.air_temp_c),
                                f64::from(w.slip_fraction),
                                f64::from(w.touching),
                                f64::from(w.slip_ratio),
                                f64::from(w.tan_slip_angle),
                                f64::from(w.force_x_n),
                                f64::from(w.force_y_n),
                            ]
                        });
                        capture.frames.push(Frame {
                            time_ms: source_time_ms,
                            received_us,
                            speed_mps,
                            wheels,
                            boundary: boundary.take(),
                        });
                    }
                }
            }
        }
    }
    if !ended {
        return Err("recording has no footer; recover it first".into());
    }
    if capture.frames.is_empty() {
        return Err("recording has no OutSim wheel samples".into());
    }
    Ok(capture)
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: cargo run --no-default-features --locked --example tyre-stress-prototype -- CAPTURE.jsonl REPORT.html (or --demo REPORT.html)".into());
    }
    let capture = if args[0] == "--demo" {
        Capture {
            source: "synthetic experiments".into(),
            track: "DEMO".into(),
            context_verified: false,
            complete: false,
            cars: BTreeSet::new(),
            frames: Vec::new(),
        }
    } else {
        read_capture(Path::new(&args[0]))?
    };
    // Escape '<' even inside filenames and gap text: JSON is embedded in HTML.
    let json = serde_json::to_string(&capture)
        .map_err(|e| e.to_string())?
        .replace('<', "\\u003c");
    let (before, after) = include_str!("tyre-stress-prototype.html")
        .split_once("__CAPTURE_DATA__")
        .unwrap();
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| format!("cannot create report (existing files are preserved): {e}"))?;
    let mut output = BufWriter::new(output);
    write!(output, "{before}{json}{after}").map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    eprintln!(
        "created {} with {} OutSim samples; open it in a browser",
        Path::new(&args[1]).display(),
        capture.frames.len()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
