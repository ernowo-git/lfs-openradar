//! Local append-only capture. Socket threads only enqueue; one writer owns disk I/O.
use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const QUEUE_SIZE: usize = 4096;
const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub struct Options {
    pub directory: PathBuf,
    pub track: String,
    pub max_bytes: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            directory: "recordings".into(),
            track: "BL1".into(),
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    Outsim,
    Outgauge,
}
impl Stream {
    fn index(self) -> usize {
        match self {
            Self::Outsim => 0,
            Self::Outgauge => 1,
        }
    }
}

/// Version 1 stores raw packets in hex so decoder additions remain possible.
/// Source and receipt clocks are distinct; neither is an official lap timer.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Header {
        format: String,
        version: u32,
        capture_id: String,
        recorder_version: String,
        started_unix_ms: u64,
        declared_track: String,
        context_verified: bool,
        outsim_options: u16,
        outsim_id: Option<i32>,
        outgauge_id: i32,
    },
    Packet {
        sequence: u64,
        stream: Stream,
        source_time_ms: u32,
        received_us: u64,
        data_hex: String,
    },
    Gap {
        stream: Option<Stream>,
        received_us: u64,
        reason: String,
    },
    Footer {
        complete: bool,
        ended_unix_ms: u64,
        packets: u64,
        error: Option<String>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub active: bool,
    pub path: PathBuf,
    pub elapsed_ms: u64,
    pub packets: [u64; 2],
    pub last_received_ms: [Option<u64>; 2],
    pub gaps: u64,
    pub rejected: u64,
    pub bytes: u64,
    pub error: Option<String>,
}

enum Input {
    Packet(Stream, u32, u64, Vec<u8>),
    Tick(u64),
    Rejected(Stream, u64, String),
    Finish,
}

#[derive(Clone)]
pub(crate) struct Sink {
    sender: SyncSender<Input>,
    status: Arc<Mutex<Status>>,
    started: Instant,
}
impl Sink {
    fn send(&self, input: Input) {
        if self.status.lock().unwrap().error.is_some() {
            return;
        }
        if let Err(e) = self.sender.try_send(input) {
            let error = match e {
                TrySendError::Full(_) => "recording queue overflow; capture interrupted",
                TrySendError::Disconnected(_) => "recording writer stopped",
            };
            self.status
                .lock()
                .unwrap()
                .error
                .get_or_insert_with(|| error.into());
        }
    }
    pub(crate) fn packet(&self, stream: Stream, source_time_ms: u32, bytes: &[u8]) {
        self.send(Input::Packet(
            stream,
            source_time_ms,
            self.now_us(),
            bytes.to_vec(),
        ));
    }
    pub(crate) fn rejected(&self, stream: Stream, reason: String) {
        self.send(Input::Rejected(stream, self.now_us(), reason));
    }
    pub(crate) fn tick(&self) {
        self.send(Input::Tick(self.now_us()));
    }
    fn now_us(&self) -> u64 {
        self.started.elapsed().as_micros() as u64
    }
}

pub(crate) struct Recorder {
    pub sink: Sink,
    worker: Option<JoinHandle<()>>,
}
impl Recorder {
    pub fn start(config: &Config, options: Options) -> Result<Self, String> {
        if options.track.is_empty()
            || options.track.len() > 6
            || !options.track.bytes().all(|c| c.is_ascii_alphanumeric())
        {
            return Err("recording requires a track code of 1..6 letters or digits".into());
        }
        if options.max_bytes < 4096 {
            return Err("recording storage limit must be at least 4096 bytes".into());
        }
        fs::create_dir_all(&options.directory)
            .map_err(|e| format!("cannot create recordings directory: {e}"))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        let id = format!("{}-{}", stamp.as_nanos(), std::process::id());
        let path = options.directory.join(format!("{id}.jsonl.partial"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("cannot create recording: {e}"))?;
        let mut output = BufWriter::new(file);
        let header = Event::Header {
            format: "openradar-recording".into(),
            version: 1,
            capture_id: id,
            recorder_version: env!("CARGO_PKG_VERSION").into(),
            started_unix_ms: stamp.as_millis() as u64,
            declared_track: options.track,
            context_verified: false,
            outsim_options: config.outsim_options,
            outsim_id: config.expected_outsim_id(),
            outgauge_id: config.outgauge_id,
        };
        let bytes = write_event(&mut output, &header)
            .map_err(|e| format!("cannot write recording header: {e}"))?;
        checkpoint(&mut output).map_err(|e| format!("cannot persist recording header: {e}"))?;
        let status = Arc::new(Mutex::new(Status {
            active: true,
            path,
            bytes,
            ..Default::default()
        }));
        let (sender, receiver) = mpsc::sync_channel(QUEUE_SIZE);
        let sink = Sink {
            sender,
            status: status.clone(),
            started: Instant::now(),
        };
        let timeout_ms = config.stale_ms;
        let worker = thread::Builder::new()
            .name("lfs-recording-writer".into())
            .spawn(move || {
                let result =
                    write_capture(output, receiver, &status, options.max_bytes, timeout_ms);
                let mut state = status.lock().unwrap();
                if let Err(e) = result {
                    state
                        .error
                        .get_or_insert_with(|| format!("recording write failed: {e}"));
                }
                state.active = false;
            })
            .map_err(|e| format!("cannot start recording writer: {e}"))?;
        Ok(Self {
            sink,
            worker: Some(worker),
        })
    }
    pub fn status(&self) -> Status {
        let mut state = self.sink.status.lock().unwrap().clone();
        if state.active {
            state.elapsed_ms = self.sink.started.elapsed().as_millis() as u64;
        }
        state
    }
    pub fn finish(mut self) -> Status {
        self.stop();
        self.status()
    }
    fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = self.sink.sender.send(Input::Finish);
            if worker.join().is_err() {
                let mut state = self.sink.status.lock().unwrap();
                state.active = false;
                state.error = Some("recording writer panicked".into());
            }
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn checkpoint(output: &mut BufWriter<File>) -> std::io::Result<()> {
    output.flush()?;
    output.get_ref().sync_data()
}
fn write_event(output: &mut impl Write, event: &Event) -> std::io::Result<u64> {
    let mut line = serde_json::to_vec(event)?;
    line.push(b'\n');
    output.write_all(&line)?;
    Ok(line.len() as u64)
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        result.push(DIGITS[(b >> 4) as usize] as char);
        result.push(DIGITS[(b & 15) as usize] as char);
    }
    result
}

fn write_capture(
    mut output: BufWriter<File>,
    receiver: mpsc::Receiver<Input>,
    status: &Mutex<Status>,
    max_bytes: u64,
    timeout_ms: u64,
) -> std::io::Result<()> {
    let mut last: [Option<(u32, u64)>; 2] = [None, None];
    let mut timed_out = [false; 2];
    let mut sequence = 0;
    let mut synced = Instant::now();
    loop {
        let input = match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(input) => Some(input),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                status
                    .lock()
                    .unwrap()
                    .error
                    .get_or_insert_with(|| "recording producer disconnected".into());
                break;
            }
        };
        let mut events = Vec::new();
        match input {
            Some(Input::Finish) => break,
            Some(Input::Packet(stream, source, received, bytes)) => {
                let i = stream.index();
                if let Some((previous, previous_received)) = last[i] {
                    let dt = source.wrapping_sub(previous);
                    let reason = if dt == 0 {
                        Some("duplicate source timestamp")
                    } else if dt > u32::MAX / 2 {
                        Some("source clock moved backwards or packets reordered")
                    } else if dt as u64 > timeout_ms
                        || received.saturating_sub(previous_received) > timeout_ms * 1000
                    {
                        Some("unexpected sample gap")
                    } else {
                        None
                    };
                    if let Some(reason) = reason.filter(|_| !timed_out[i]) {
                        events.push(Event::Gap {
                            stream: Some(stream),
                            received_us: received,
                            reason: reason.into(),
                        });
                    }
                }
                last[i] = Some((source, received));
                timed_out[i] = false;
                events.push(Event::Packet {
                    sequence,
                    stream,
                    source_time_ms: source,
                    received_us: received,
                    data_hex: hex(&bytes),
                });
                sequence += 1;
                let mut state = status.lock().unwrap();
                state.last_received_ms[i] = Some(received / 1000);
            }
            Some(Input::Rejected(stream, received, reason)) => {
                status.lock().unwrap().rejected += 1;
                events.push(Event::Gap {
                    stream: Some(stream),
                    received_us: received,
                    reason: format!("rejected packet: {reason}"),
                });
            }
            Some(Input::Tick(now)) => {
                status.lock().unwrap().elapsed_ms = now / 1000;
                for stream in [Stream::Outsim, Stream::Outgauge] {
                    let i = stream.index();
                    if !timed_out[i]
                        && now.saturating_sub(last[i].map_or(0, |(_, t)| t)) > timeout_ms * 1000
                    {
                        events.push(Event::Gap {
                            stream: Some(stream),
                            received_us: now,
                            reason: "stream timeout; data unavailable".into(),
                        });
                        timed_out[i] = true;
                    }
                }
            }
            None => {}
        }
        for event in events {
            // Leave room for a terminal gap/footer while keeping the file bounded.
            if status.lock().unwrap().bytes + 2048 > max_bytes {
                status.lock().unwrap().error =
                    Some("recording storage limit reached; capture interrupted".into());
                break;
            }
            let size = write_event(&mut output, &event)?;
            let mut state = status.lock().unwrap();
            state.bytes += size;
            match event {
                Event::Gap { .. } => state.gaps += 1,
                Event::Packet { stream, .. } => state.packets[stream.index()] += 1,
                _ => {}
            }
        }
        if status.lock().unwrap().error.is_some() {
            break;
        }
        if synced.elapsed() >= Duration::from_secs(1) {
            checkpoint(&mut output)?;
            synced = Instant::now();
        }
    }
    let (error, path, count) = {
        let state = status.lock().unwrap();
        (
            state.error.clone(),
            state.path.clone(),
            state.packets.iter().sum(),
        )
    };
    let size = write_event(
        &mut output,
        &Event::Footer {
            complete: error.is_none(),
            ended_unix_ms: unix_ms(),
            packets: count,
            error,
        },
    )?;
    checkpoint(&mut output)?;
    drop(output);
    let final_path = path.with_extension("");
    // Hard-link publication is atomic and refuses to overwrite an existing file.
    fs::hard_link(&path, &final_path)?;
    fs::remove_file(&path)?;
    let mut state = status.lock().unwrap();
    state.path = final_path;
    state.bytes += size;
    Ok(())
}

/// Recover complete JSON lines from an interrupted file, preserving the original.
/// A truncated/malformed tail is ignored; the output is explicitly incomplete.
pub fn recover(source: &Path, destination: &Path) -> Result<(), String> {
    let input = File::open(source).map_err(|e| e.to_string())?;
    let mut input = BufReader::new(input);
    let mut header = false;
    let mut packets = 0;
    let mut output: Option<BufWriter<File>> = None;
    loop {
        let mut line = Vec::new();
        use std::io::Read;
        let size = input
            .by_ref()
            .take(MAX_LINE_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if size == 0 || size > MAX_LINE_BYTES || !line.ends_with(b"\n") {
            break;
        }
        let Ok(event) = serde_json::from_slice::<Event>(&line) else {
            break;
        };
        if !header {
            if !matches!(&event, Event::Header { format, version: 1, .. } if format == "openradar-recording")
            {
                return Err("unsupported recording header".into());
            }
            header = true;
            output = Some(BufWriter::new(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(destination)
                    .map_err(|e| e.to_string())?,
            ));
        } else if matches!(event, Event::Header { .. } | Event::Footer { .. }) {
            break;
        }
        if matches!(event, Event::Packet { .. }) {
            packets += 1;
        }
        write_event(output.as_mut().unwrap(), &event).map_err(|e| e.to_string())?;
    }
    let Some(mut output) = output else {
        return Err("no complete recording header".into());
    };
    write_event(
        &mut output,
        &Event::Footer {
            complete: false,
            ended_unix_ms: unix_ms(),
            packets,
            error: Some("recovered interrupted recording; tail may be missing".into()),
        },
    )
    .map_err(|e| e.to_string())?;
    checkpoint(&mut output).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_queue_fails_explicitly_without_waiting_for_disk() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let status = Arc::new(Mutex::new(Status::default()));
        let sink = Sink {
            sender,
            status: status.clone(),
            started: Instant::now(),
        };
        sink.tick();
        sink.packet(Stream::Outsim, 1, &[1, 2, 3]);
        assert!(
            status
                .lock()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("overflow")
        );
    }

    #[test]
    fn rollover_is_continuous_but_reordered_and_duplicate_times_are_marked() {
        let directory = std::env::temp_dir().join(format!(
            "openradar-clock-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let recorder = Recorder::start(
            &Config::default(),
            Options {
                directory: directory.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        for source in [u32::MAX - 5, 4, 4, 1] {
            recorder.sink.packet(Stream::Outsim, source, &[0; 4]);
        }
        let state = recorder.finish();
        assert_eq!(state.gaps, 2);
        assert_eq!(state.packets, [4, 0]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn write_failure_is_returned_instead_of_publishing_success() {
        let path = std::env::temp_dir().join(format!(
            "openradar-readonly-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"").unwrap();
        let output = BufWriter::new(File::open(&path).unwrap());
        let (sender, receiver) = mpsc::sync_channel(2);
        sender
            .send(Input::Packet(Stream::Outsim, 1, 1, vec![0; 4]))
            .unwrap();
        sender.send(Input::Finish).unwrap();
        let state = Mutex::new(Status {
            path: path.clone(),
            ..Default::default()
        });
        assert!(write_capture(output, receiver, &state, DEFAULT_MAX_BYTES, 250).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"");
        fs::remove_file(path).unwrap();
    }
}
