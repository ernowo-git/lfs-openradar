# Usage guide

For installation and a quick start, see [the README](../README.md).

## Control panel and gadget grid

The **Gadgets** tab presents Radar, Gap ahead, Gap behind, and Performance delta
cards in insertion order. Cards fill each row from left to right, wrap to the left of the next row
when another card cannot fit, and reflow when the window is resized. Narrow
windows use one column; extra rows are vertically scrollable. The panel opens
with space for all four cards and fits its height to the rendered controls,
within the monitor's available size. You can resize it afterward.

The header keeps **Apply / reconnect**, **Save settings**, **Quit**, and connection
status visible on either tab, including when the content is scrolled. Apply is
disabled in the demo, which has no network connection. The **Settings** tab holds
**Show overlays**, background hiding, interpolation, the InSim password, and
**LFS startup setup**. Radar position mode, X/Y, size, and side range are inside
the Radar card.

**Interpolation (ms)** smooths radar motion by displaying slightly older
telemetry between received updates. Higher values can reduce jitter but add
display delay. The default is 60 ms; 0 ms uses the latest time shared by MCI
and OutSim. It does not predict future positions. Click **Apply / reconnect**
after changing interpolation in live mode.

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

In a race with sector timing, OpenRadar can record a clean trace from the
sector-1 checkpoint to the finish on lap 1. On lap 2, delta starts at zero when
you cross sector 1 and compares the remainder of the lap with that trace.
**SINCE SECTOR 1** identifies this partial comparison: gains or losses in the
first sector are excluded from the delta. A reference requires uninterrupted
matching telemetry and LFS's split and lap reports; connecting after sector 1
means waiting for a later valid recording.

After a clean, fully recorded finish-to-finish lap is confirmed, the gadget uses
your session-best full-lap reference. Tracks without a valid sector-1 checkpoint,
practice, and qualifying use this full-lap recording path from the beginning.
The display compares elapsed time at matching track
progress on every complete MCI update (normally every 20 ms), with spatial
interpolation between reference nodes. Negative/green means ahead; positive/red
means behind. **GAINING / LOSING / STEADY** and the small bar show the recent change
in delta, independently of whether you are ahead overall.

**Estimated lap** shows the projected current lap time in minutes and seconds.
With a full reference it is the reference lap time plus the live delta. With a
partial reference it is the current lap's official first-sector time plus the
reference time from sector 1 to the finish, plus the live delta since sector 1.
It assumes the remaining track is driven at the reference pace. It shows a dash
until comparison is available, and while paused, invalid, or telemetry is stale.

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
to write the configuration file used at launch (see [Configuration](configuration.md)). Use **Apply / reconnect** after changing live
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
freezes, see the [troubleshooting guide](troubleshooting.md).

## Platform status

- **Windows:** native build, demo rendering, and mock TCP/UDP telemetry are
  exercised during development. Live LFS alignment, input pass-through over the
  game, DPI behavior, and orientation smoothness still need driving tests.
  Release executables require the Microsoft Visual C++ x64 runtime; see the
  [v0.2 release notes](release-notes-v0.2.md) for the official download.
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
[the investigation](graphics-freeze-investigation.md) for evidence and limits.

The overlay reads the telemetry worker's latest snapshot on every repaint and
schedules its own refresh. It does not rely on control-panel repainting to leave
the paused state or notice missing telemetry. Demo mode also advances its own
dual-source simulation when painting the overlay.

Windows builds use a local eframe 0.33.0 patch to retain missed native redraw
requests and service them when another window paints. This addresses the
reported overlay that updated only while resizing, and repaint loss when focus
changes between windows. The patch has a soft catch-up time budget and changes
no Windows driver settings. Its source/provenance and limits are documented in
[vendor/eframe/OPENRADAR-PATCH.md](../vendor/eframe/OPENRADAR-PATCH.md); keep the vendor
directory when copying the project. The user reported success with the redraw
and positioning fixes on this Windows setup on 2026-10-06. Broader driving and
platform acceptance checks below remain outstanding.

The vendored wgpu integration also routes screenshot readbacks to their
originating viewport on every platform. This prevents another overlay from
consuming the control-panel screenshot while multiple gadgets are enabled.

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
[docs/proximity-radar-plan.md](proximity-radar-plan.md). Future product scope:
[docs/roadmap.md](roadmap.md).
