# Local telemetry recording

Tracked in [RAC-4](https://plane.ctrlchsr.space/sampingan/projects/4f36a711-f98d-48a3-9227-5d674915f6d4/issues/e82e0bb0-e030-44ba-9fb5-0d270fb83863). This is the recorder milestone; map calibration, calculated laps, RaceHub analysis and uploads follow separately.

OpenRadar can save OutSim and OutGauge without InSim, a multiplayer server connection, RaceHub login or internet access. The existing radar and timing gadgets still need their usual InSim context. Recording does not verify a driver account, track, session type, lap legality or official times.

## Record from the panel

1. With LFS closed, configure OutSim and OutGauge using the existing Settings setup buttons. Extended OutSim (`OutSim Opts 1ff`) supplies the detailed channels. Do not change the game's configuration while LFS is running.
2. Open **Recording**. Turn off **Enable InSim HUD connection** for recording without InSim.
3. Enter the actual track code, such as `BL1`, and the output directory. Track selection is your declaration, not automatic detection. Relative directories are relative to the application's working directory.
4. Start LFS and drive in a supported view. Click **Start recording**. The panel applies the connection settings and uses one UDP receiver; dashboard gadgets can remain disabled.
5. Check that both stream counters increase. Camera changes, incompatible output settings, another application holding a port, and silence can prevent samples. Use the displayed stream ages, gaps, rejected count and path to check recording health.
6. Click **Stop recording** before changing track or car. The file is finalized and its path is displayed. Quit also finalizes an open recording. Apply/reconnect is disabled during a recording.

The default file cap is 512 MiB. Reaching it or overflowing the bounded write queue interrupts the recording and reports an error. Existing recordings are never silently deleted. A finished capture can still contain gaps or no samples: inspect its counters and quality events before analysis.

## Record without the panel

```sh
cargo run --no-default-features --locked -- --no-insim --record recordings --track BL1 --seconds 60
```

For an installed executable, replace the Cargo prefix with `lfs-openradar`. Omit `--seconds` to record until Enter. A forced termination can leave an interrupted file; Ctrl+C does not perform graceful finalization in this first version.

Configuration also accepts `insim_enabled = false`. It defaults to true for existing HUD users. The recording command does not edit LFS configuration or connect to RaceHub.

## Data and recovery

Files begin as `<TRACK>_<UTC-start-time>.jsonl.partial`. On stop, the detected car code is added, for example `XFG_BL1_2026-10-17_13-30-52.123456789Z.jsonl`. `Z` means UTC; fractional seconds distinguish captures started close together. The car comes from OutGauge (vehicle mods use their hexadecimal identifier), while the track remains user-declared. Files with no valid OutGauge car use `UNKNOWN-CAR`; captures containing different cars use `MULTI-CAR`.

The header is persisted immediately; the writer flushes and requests file synchronization at least every second when it can make progress. Abrupt power/process loss may lose data after the last successful checkpoint. Stop drains the queue, writes a footer and synchronizes the file, then atomically publishes the finished `.jsonl` file without replacing an existing file. A disk failure leaves an error and may leave a partial file.

Recover complete lines from an interrupted file:

```sh
lfs-openradar --recover-recording recordings/CAPTURE.jsonl.partial
```

This creates `CAPTURE.jsonl.partial.recovered.jsonl`, preserves the original and refuses to overwrite an existing destination. The recovered footer always says incomplete. Only complete, supported event lines preceding a damaged tail are retained; recovery cannot restore missing samples or certify telemetry accuracy.

Version 1 is JSON Lines: `header`, `packet`, `gap`, `footer`. Packets preserve the exact datagram as lowercase hex with stream name, original source time in milliseconds, monotonic receipt time in microseconds since capture start, and a recording-wide sequence. The header includes the selected track, OutSim options and IDs; it includes no passwords or access tokens. OutSim and OutGauge remain separate streams rather than being silently resampled together.

The detailed decoders expose optional motion, controls, drivetrain, distance, wheel and steering-torque channels in protocol units. Missing blocks remain absent. Tyre temperature is air temperature; wheel indices remain unlabeled pending live corner mapping. No tread wear or surface-temperature channel is invented. Raw packets preserve all received bytes for future decoding.

An experimental [tyre stress and wear report](tyre-stress-prototype.md) can replay
extended wheel recordings as a standalone HTML file. It converts accumulated
stress into estimated tread loss and remaining tread using explicit calibration
inputs. Illustrative values can demonstrate the model; wear accuracy remains
unvalidated.

`gap` events record timeout/rejection, duplicate/backwards source clocks and long discontinuities. The initial timeout threshold uses `stale_ms`; short dropped intervals are not guaranteed to be detected, and UDP supplies no packet sequence number to count exact losses. Header UTC is client-reported. A footer's `complete` means clean capture finalization, not a complete lap, gap-free trace or official result.

Before map/UI work, capture real laps with InSim disabled, verify both streams and decoded fields, inspect camera gaps and compare with the actual car. Software fixtures do not verify LFS/Wine driving behavior.
