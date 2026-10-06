# lfs-openradar

Standalone Rust proximity radar, gap gadgets, and live performance delta for Live for Speed. The prototype receives
InSim/MCI opponents and required OutSim local telemetry, interpolates their
histories, and draws a radar using egui/winit/wgpu.

## Documentation

- [Configuration guide](docs/configuration.md): config location, `--config`,
  saving settings, InSim passwords, and telemetry options.
- [LFS startup setup](docs/lfs-startup-setup.md): enable InSim automatically,
  resolve port conflicts, and restore a script backup.
- [CI and releases](docs/ci-and-releases.md): PR checks, version bumps,
  automatic tags, release notes, and Windows/Linux downloads.
- [Troubleshooting](docs/troubleshooting.md): connection issues, positioning,
  paused overlays, and graphics logs.
- [Product roadmap](docs/roadmap.md): planned widgets and architecture.
- [v0.2 release notes](docs/release-notes-v0.2.md): platform requirements and
  release limitations.
- [v0.2 release plan](docs/release-v0.2.md): packaging and platform acceptance.
- [Radar implementation plan](docs/proximity-radar-plan.md): telemetry research
  and driving acceptance criteria.
- [Graphics freeze investigation](docs/graphics-freeze-investigation.md):
  evidence, mitigations, and remaining graphics checks.

## Roadmap

OpenRadar will grow into a modular overlay with independently configurable
radar, split/lap timing, gear, RPM, and gap-ahead/gap-behind widgets. These
additional gadgets were not included in `v0.1`. The current feature branch adds
estimated gap-ahead/gap-behind gadgets, a live performance delta, and a wrapping control-panel grid;
split/lap timing, gear, and RPM remain planned. The gadgets share the
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
The [CI and release guide](docs/ci-and-releases.md) covers automatic Windows
and Linux x64 releases. The [v0.2 release plan](docs/release-v0.2.md) records the
earlier packaging and platform acceptance work.

## Gap gadgets and control-panel grid

The control panel presents Radar, Gap ahead, and Gap behind cards in insertion
order. Cards fill each row from left to right, wrap to the left of the next row
when another card cannot fit, and reflow when the window is resized. Narrow
windows use one column; extra rows and global controls are vertically scrollable.

Radar, Gap ahead, and Gap behind each have a separate transparent overlay window.
Enable the gap gadgets from their cards, turn on each card's **Position mode**,
and drag that gadget's title bar anywhere on the desktop. Turn Position mode off
to restore its borderless, mouse click-through display. Each gap also has screen
X/Y controls, a **Move gadget** handle in Position mode, and a Scale control.
**Save settings** persists each window's screen coordinates and scale.
Existing TOML files retain their radar settings and default to radar enabled,
with both new gap gadgets disabled. Earlier normalized gap X/Y values are
migrated to screen coordinates; future saves use `window_x` and `window_y` in
logical screen pixels. Fresh gap windows start to the right of the radar.

**Show overlays** controls all windows, and each card's Enabled setting controls
its own gadget. Closing a gadget window in Position mode disables that gadget.
The radar retains its own size and position when gaps are enabled or moved.
All windows share one telemetry connection, but repaint independently; hidden
windows keep their existing graphics surfaces for reuse. Background hiding
applies to each window, with its own Position mode allowing placement outside LFS.

Gaps compare the immediately preceding/following **race-order** driver, including
AI, rather than the nearest car on the radar. Both show the driver, race position,
an estimate such as `~5.0 s`, and the measurement age. Lapped neighbors show lap
separation. This first implementation supports races with standard track timing;
practice/qualifying rankings and custom/open timing layouts show unavailable.

The existing InSim connection requests track node count and finish-node metadata.
For each complete MCI set, the gap engine keeps up to 30 seconds of node-passage
history (also capped at 3,100 passages per car). It compares both cars' passage
times at the trailing car's latest crossed node, matching lap identity across the
finish line. Skipped-node times are interpolated between updates. No distance
divided by speed or forward prediction is used. Values update as nodes are
crossed; arrival timestamps, node spacing, and network delay limit accuracy.

Missing history, unknown/tied race positions, lagging/out-of-path cars, backwards
progress, resets, and stale or mismatched telemetry withhold seconds. A crossing
older than two seconds is unavailable until progress resumes. Both gadgets share
the current local-driver/OutSim association gate. Live two-car validation against
LFS timing remains required; synthetic checks do not establish live accuracy.

For a native preview with both gaps enabled, create a TOML file with
`[gap_ahead]` and `[gap_behind]` sections containing `enabled = true`, then run:

```text
cargo run -- --demo --config YOUR_CONFIG.toml --seconds 12 --screenshot gadgets-preview.png
```

Gap-enabled screenshots wait up to six seconds for the demo passage history;
shorter timed runs capture earlier. The deterministic demo settles at `~5.0 s`
ahead and `~2.3 s` behind.

## Live performance delta

Enable **Performance delta** and **Show overlays** in the control panel. Its own
**Position mode**, X/Y, and Scale controls work like the gap gadgets. Save settings
to persist placement. Existing configurations default to this gadget disabled.

After crossing the finish line, complete a clean, fully recorded lap to establish
your session-best reference. The first partial lap after connecting or leaving
the pits cannot qualify. The display compares elapsed time at matching track
progress on every complete MCI update (normally every 20 ms), with spatial
interpolation between reference nodes. Negative/green means ahead; positive/red
means behind. **GAINING / LOSING / STEADY** and the small bar show the recent change
in delta, independently of whether you are ahead overall.

The reference stays fixed during a lap and updates after a faster clean lap is
confirmed by LFS's lap-completion packet. Track-limit/wall/pit-speed violations,
pit stops, penalties, resets, backwards/discontinuous progress, and missing or
misassociated telemetry prevent a lap becoming a reference. Pitting or changing
views retains the best reference for the same driver and car, but restarts
recording. Track, layout, car, session changes and reconnects clear the reference.
References are session-only and are not written to disk.

Standard circuit timing is supported in practice, qualifying, and races. Open
and custom timing layouts show unavailable. Values are explicitly estimates:
MCI has no per-car physics timestamp, and node spacing, racing-line differences,
and network arrival timing affect accuracy. Stale telemetry withholds the delta.
Live driving validation remains required. The demo records a 100-second reference
lap after its first finish crossing; the delta becomes available after 200 seconds.

```toml
[performance_delta]
enabled = true
window_x = 376.0
window_y = 360.0
scale = 1.0
```

## Run

From this folder:

```text
cargo run -- --demo
cargo run
```

The demo opens the control panel and an animated overlay without opening any
network sockets. Live mode opens the control panel and connects to the local LFS
client. Enable **Show overlays** when ready. **Radar position mode** adds a normal
border/title bar to the radar; drag its title bar to move it. Turn position
mode off to restore the borderless, mouse click-through radar with a transparent
background. Position mode retains the dark radar backing for placement; the
control-panel preview also keeps its backing. The **Move radar**
handle and **X/Y** controls in the control panel remain available as alternatives.
**Radar size** applies once dragging stops and the value has
settled for 250 ms. **Side range (m)** changes the drawing scale immediately.
Use **Save settings**
to write the configuration file used at launch (see [Configuration](#configuration)). Use **Apply / reconnect** after changing live
settings, including interpolation and detection range.

Windows foreground detection hides each overlay when LFS.exe is not active,
unless that gadget's position mode is enabled. Keep the control panel open; closing it exits
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

The defaults match the OutSim settings inspected on this machine. The startup
setup button changes LFS's `autoexec.lfs` only when requested; OutSim and
LFSLapper settings are configured separately.

1. Open **LFS startup setup** in OpenRadar, select the folder containing `LFS.exe`,
   and click **Enable InSim at startup**. Restart LFS to activate it. Alternatively,
   type `/insim 29999` in the local LFS client for the current session. This is the
   client's listener, independent of the server's LFSLapper connection. See the
   [startup setup guide](docs/lfs-startup-setup.md) for backups and port conflicts.
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

If LFS reports a password mismatch, enter your local LFS `Game Admin` password
in the masked **InSim password** field, then click **Apply / reconnect**.
**Save settings** stores it as plain text in the chosen TOML file. See the
[password configuration guide](docs/configuration.md#insim-password) for TOML
settings and environment overrides.

OutGauge may share UDP port 30000: its standard packets are ignored. A separate
telemetry app already bound to that port causes a clear startup error. Configure
distinct destinations or use a telemetry relay when sharing consumers; this
prototype does not implement a relay.

## Configuration

By default, OpenRadar loads `openradar.local.toml` from its **working directory**:
the folder it was launched from, which can differ from the executable's folder.
For simple use, copy [openradar.example.toml](openradar.example.toml) to
`openradar.local.toml` beside the executable and launch from that folder. A
Windows shortcut's **Start in** field controls its working directory.

You can keep the config in any directory by passing its path:

```text
lfs-openradar.exe --config "C:\MySettings\openradar.local.toml"
```

**Save settings** writes to that same path. Without `--config`, a missing default
file uses built-in defaults, and **Save settings** creates `openradar.local.toml`
in the working directory. An explicitly supplied config must already exist.

See the [configuration guide](docs/configuration.md) for relative paths, Linux
and source-build examples, password settings, and telemetry options.

## Platform status

- **Windows:** native build, demo rendering, and mock TCP/UDP telemetry are
  exercised during development. Live LFS alignment, input pass-through over the
  game, DPI behavior, and orientation smoothness still need driving tests.
  Release executables require the Microsoft Visual C++ x64 runtime; see the
  [v0.2 release notes](docs/release-notes-v0.2.md) for the official download.
- **Linux/X11:** the x64 release build, automated tests, and an Xvfb/Vulkan
  software-rendered demo pass in an Ubuntu 22.04 environment under WSL. The
  candidate requires glibc 2.35 or newer. Live LFS through Wine/Proton, overlay
  transparency, stacking, positioning, and click-through over a game still need
  acceptance testing. Running the binary needs X11 libraries and working
  Vulkan/OpenGL drivers. Building from source additionally needs a C/C++
  compiler, pkg-config, and X11/Wayland development libraries.
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

The vendored wgpu integration also routes screenshot readbacks to their
originating viewport on every platform. This prevents another overlay from
consuming the control-panel screenshot while multiple gadgets are enabled.

## Build and verification

Rust 1.88 or newer is required. Windows builds need Visual Studio C++ Build Tools
and a Windows SDK. Cargo.lock records the dependency graph.

GitHub Actions runs formatting, clippy, compilation, desktop/headless tests,
and release-automation tests on Windows and Linux for every opened or updated PR.
Merging a higher Cargo version into `main` triggers verified release builds,
a `vMAJOR.MINOR.PATCH` tag, generated release notes, and downloads for both OSes.
An unchanged version skips release creation. See [CI and releases](docs/ci-and-releases.md)
for the version-bump procedure and workflow recovery.

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
