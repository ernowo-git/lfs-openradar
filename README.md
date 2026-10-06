# lfs-openradar

Standalone Rust proximity radar for Live for Speed. The prototype receives
InSim/MCI opponents and required OutSim local telemetry, interpolates their
histories, and draws a radar using egui/winit/wgpu.

## Roadmap

OpenRadar will grow into a modular overlay with independently configurable
radar, split/lap timing, gear, RPM, and gap-ahead/gap-behind widgets. These
additional gadgets are planned, not included in `v0.1`. They will share the
existing Rust telemetry runtime: InSim for opponents and timing, and modern
OutSim for local pose, gear, and engine speed. OutGauge remains optional for
later dashboard fields.

The future analyzer belongs in a separate repository with its own roadmap and
releases. That project will own live speed traces, driving-line visualization,
track maps with splits, telemetry ingestion, storage, analysis, and the frontend
UI. This repository owns the radar and widgets. Each app can run independently;
OpenRadar will not act as the analyzer's centralized collector. Simultaneous
OutSim delivery needs verification, as described in the roadmap's
connection/coexistence gate.

See the [product roadmap](docs/roadmap.md) for milestones, telemetry requirements,
widget architecture, and acceptance criteria.

## Run

From this folder:

```text
cargo run -- --demo
cargo run
```

The demo opens the control panel and an animated overlay without opening any
network sockets. Live mode opens the control panel and connects to the local LFS
client. Enable **Show overlay** when ready. **Position mode** adds a normal
border/title bar to the overlay; drag its title bar to move it. Turn position
mode off to restore the borderless, mouse click-through radar with a transparent
background. Position mode retains the dark radar backing for placement; the
control-panel preview also keeps its backing. The **Move radar**
handle and **X/Y** controls in the control panel remain available as alternatives.
**Overlay size** applies once dragging stops and the value has
settled for 250 ms. **Side range (m)** changes the drawing scale immediately.
Use **Save settings**
to write openradar.local.toml. Use **Apply / reconnect** after changing live
settings, including interpolation and detection range.

Windows foreground detection hides the overlay when LFS.exe is not active,
unless position mode is enabled. Keep the control panel open; closing it exits
the radar and releases its sockets.

Windows rendering uses DX12 with a DirectComposition visual swapchain for
background transparency. The vendor patch disables the ordinary opaque Windows
redirection bitmap when creating transparent DirectComposition windows.
Alpha support is enabled on the shared renderer;
the control panel still paints an opaque background. Other platforms retain
default wgpu backend selection. Windows builds select this path explicitly,
overriding `WGPU_BACKEND` and `WGPU_DX12_PRESENTATION_SYSTEM`.

For paused output, positioning, redraw problems, connection errors, or graphics
freezes, see the [troubleshooting guide](docs/troubleshooting.md).

## LFS setup

The defaults match the OutSim settings inspected on this machine. No LFS or
LFSLapper configuration files were changed.

1. In the local LFS client, type `/insim 29999`. This is the client's listener,
   independent of the server's LFSLapper connection.
2. With LFS closed, verify these values in its cfg.txt if OutSim is not already
   configured. Restart LFS after any manual changes.

```text
OutSim Mode 1
OutSim Delay 2
OutSim IP 127.0.0.1
OutSim Port 30000
OutSim ID 24601
OutSim Opts 1ff
```

3. Drive the local human car in cockpit or custom view. The current prototype
   pauses for spectating, replays, free view, pause, ambiguous local human
   selection, or a mismatch between the OutSim pose and MCI driver.
4. Check that both MCI and OutSim age indicators update. Missing, stale, or
   misassociated OutSim pauses the radar; there is no MCI-only fallback.

Regular players can use OpenRadar while connected to a multiplayer server,
including in its lobby, without server administrator privileges or the remote
server's admin password. OpenRadar connects to the player's own local LFS client
at `127.0.0.1`; each player enables local InSim and configures their own OutSim.
The overlay can remain open in the lobby, but radar output stays paused until
the player drives their own car in cockpit or custom view with both telemetry
sources updating.

The connection requests InSim version 9 for LFS 0.7A+ compatibility and accepts
version 9 or 10 responses. It sends initialization, roster/state requests,
keepalive replies, and a connection-specific close message on exit. It does not
send game controls, race restarts, telemetry reconfiguration, or host messages.

If LFS reports "password does not match your multiplayer admin password", enter
the password configured in your **local LFS installation** in the control
panel's masked **InSim password** field, then click **Apply / reconnect**.
You can check this password in your local LFS `cfg.txt`: find the `Game Admin`
entry and use the value after it. Leave the field blank if that entry is empty.
This is the local client's InSim authentication password; you do not need the
multiplayer server's admin password. See the
[LFS InSim password documentation](https://en.lfsmanual.net/wiki/IS_ISI#Admin).
The field stays in memory and is not included in saved TOML settings or logs.
Alternatively, set `LFS_INSIM_ADMIN` in the launch environment (also supported
for headless mode).

OutGauge may share UDP port 30000: its standard packets are ignored. A separate
telemetry app already bound to that port causes a clear startup error. Configure
distinct destinations or use a telemetry relay when sharing consumers; this
prototype does not implement a relay.

## Configuration

Copy openradar.example.toml to openradar.local.toml and edit it, or specify a path:

```text
cargo run -- --config openradar.example.toml
cargo run -- --headless --seconds 10
cargo run -- --demo --headless --seconds 3
```

InSim and OutSim addresses must be loopback addresses with nonzero ports.
OutSim options in TOML are decimal: 511 corresponds to LFS's hexadecimal 1ff.

Supported OutSim formats:

- The configurable OutSim2 layout, including the full 280-byte LFST packet.
  TIME and MAIN fields are required. If ID validation is enabled, the ID field
  must be present.
- Legacy format: set outsim_options = 0. Set outsim_id = 0 for a 64-byte packet
  without ID, or a nonzero ID for a 68-byte packet with ID.
- A zero outsim_id disables ID validation for configurable layouts.

Interpolation defaults to 60 ms. Arrival times align the sources approximately;
MCI does not provide per-car physics timestamps. Extrapolation is disabled.
Telemetry older than 250 ms becomes uncertain; older than 500 ms pauses the
radar. These thresholds are configurable.

Vehicle footprints use approximate configurable dimensions (default 1.8 m by
4.2 m). Blue means nearby, amber means alongside, red means potential contact,
and gray means uncertain telemetry. These are geometric hints, not validated
collision predictions. Model-specific dimensions and origin offsets still need
calibration. A height gate reduces bridge/overpass detections but needs track
testing.

## Platform status

- **Windows:** native build, demo rendering, and mock TCP/UDP telemetry are
  exercised during development. Live LFS alignment, input pass-through over the
  game, DPI behavior, and orientation smoothness still need driving tests.
- **Linux/X11:** native Rust build intended alongside LFS through Proton; not
  verified on this Windows machine. Install your distribution's C/C++ compiler,
  pkg-config, X11/Wayland development libraries, and working Vulkan/OpenGL
  drivers as needed by winit/wgpu.
- **Wayland:** reliable stacking above a game needs compositor-specific work or
  a layer-shell backend. The current ordinary-window prototype does not promise
  this support.
- **Exclusive fullscreen / Gamescope:** unverified.

Foreground detection is implemented on Windows only. Other desktops expose
manual overlay visibility. Windowed/borderless LFS is the initial target.

## Graphics diagnostics

Desktop launches append to `openradar.graphics.log` beside the chosen TOML path.
The log records the selected GPU/backend/driver, overlay toggles, settled size
changes, and graphics warnings/errors. It excludes telemetry and passwords and
is capped at 1 MiB; it resets on a later launch when almost full. Timestamps are
Unix milliseconds. Headless mode does not initialize the graphics stack.
If the overlay keeps painting while the panel has not repainted for five seconds,
the log records that observation once and records when panel painting resumes.
This may also happen when a panel is deliberately minimized or obscured.

After two reported hard freezes during position/range editing on this machine,
Windows recorded matching GPU engine timeouts and AMD driver reports. Native
window handling was changed to avoid resize storms, position feedback commands,
and surface destruction/recreation when hidden. Decorations now change only
when toggling position mode. The control
panel and overlay now render as separate deferred viewports. These are
mitigations; GPU-free automated checks cannot certify driver stability. See
[the investigation](docs/graphics-freeze-investigation.md) for evidence and limits.

The overlay reads the telemetry worker's latest snapshot on every repaint and
schedules its own refresh. It does not rely on control-panel repainting to leave
the paused state or notice missing telemetry. Demo mode also advances its own
dual-source simulation when painting the overlay.

Windows builds use a local eframe 0.33.0 patch to retain missed native redraw
requests and service them when another window paints. This addresses the
reported overlay that updated only while resizing, and repaint loss when focus
changes between windows. The patch has a soft catch-up time budget and changes
no Windows driver settings. Its source/provenance and limits are documented in
[vendor/eframe/OPENRADAR-PATCH.md](vendor/eframe/OPENRADAR-PATCH.md); keep the vendor
directory when copying the project. The user reported success with the redraw
and positioning fixes on this Windows setup on 2026-10-06. Broader driving and
platform acceptance checks below remain outstanding.

## Build and verification

Rust 1.88 or newer is required. Windows builds need Visual Studio C++ Build Tools
and a Windows SDK. Cargo.lock records the dependency graph.

```text
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --locked
```

Headless-only builds avoid the desktop graphics dependencies:

```text
cargo test --no-default-features --locked
cargo run --no-default-features -- --headless --seconds 10
```

The automated tests cover packet validation and conversions, fragmented TCP
reads, sets of 17/48 cars, OutSim layouts, required input gating, wrong-pose
rejection, heading wrap, clock reset/wrap, lifecycle invalidation, and a local
mock LFS TCP/UDP server. The mock test also checks shared OutGauge traffic and
continued MCI reception after OutSim stops.

Save an application-rendered demo preview for visual verification:

```text
cargo run -- --demo --seconds 4 --screenshot demo-preview.png
```

Live acceptance still requires two nearby cars while turning/sliding, AI,
pit/spectate/rejoin, takeover, reconnect, track/car changes, bridges and slopes,
and a comparison of orientation smoothness and latency against an MCI-only
diagnostic baseline. Do not treat passing synthetic tests as completion of those
driving checks.

## Source layout

```text
src/
  main.rs              CLI and application startup
  config.rs            Validated TOML settings
  demo.rs              Synthetic dual-source preview
  runtime.rs           Socket worker, reconnect, latest-frame publication
  lfs/insim.rs         TCP framing, handshake, roster/lifecycle, MCI sets
  lfs/outsim.rs        OutSim layout validation and decoding
  radar/mod.rs         Source association, interpolation, footprint geometry
  overlay/desktop.rs   Control panel and transparent radar window
  overlay/mod.rs       Platform foreground detection
tests/                 Protocol, geometry, and socket integration checks
```

Research and remaining acceptance criteria:
[docs/proximity-radar-plan.md](docs/proximity-radar-plan.md). Future product scope:
[docs/roadmap.md](docs/roadmap.md).
