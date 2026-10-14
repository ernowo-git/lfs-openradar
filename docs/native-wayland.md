# Native Wayland HUD on KDE/KWin

OpenRadar automatically selects its native Wayland overlay backend on supported
KDE/KWin Wayland sessions when the backend preference is Auto. It draws
all enabled gadgets on one transparent, click-through Layer Shell surface.
The existing control panel continues through XWayland, so both a working
`DISPLAY` and a native Wayland session are required. Native GPU presentation
currently uses Vulkan.

## Start

Build from the current source with the default features, then preview without LFS:

```bash
cargo run --locked -- --demo
```

For live telemetry, omit `--demo`:

```bash
./target/debug/lfs-openradar --config /path/to/openradar.local.toml
```

**Settings → Overlay backend** defaults to **Auto (recommended)**. At startup,
Auto checks for a Linux KDE Wayland session, connects to Wayland, and checks
that the compositor advertises Layer Shell. It selects Window elsewhere or
when native support is unavailable. Saving Auto retains automatic selection;
both Windows/X11 and Wayland layouts remain separate.

If your config already explicitly selects Window or Native Wayland, that choice
is retained. Select Auto, save, and restart to enable detection, or launch with
`--overlay-backend auto`. Use `--overlay-backend window` or
`--overlay-backend wayland` to choose either presentation explicitly. The CLI
option overrides the saved preference for the launch; saving settings also
saves that choice. Backend changes take effect after restart.

Once native rendering is selected, initialization or rendering failures remain
visible in the panel for retry or a manual switch to Window.

The first arrangement puts stacked gaps upper-left, delta upper-centre,
Fuel lower-left, Speed dashboard lower-centre, and Radar lower-right. It
preserves each gadget's Enabled preference; enable the other gadgets to see them.
The existing HUD style selector, previews, and telemetry controls still work.

## Placement and monitors

Select the target monitor in **Settings → Monitor**. All X/Y values in native
mode are logical pixels from that monitor's top-left corner. Each gadget has
independent placement and size. In **Gadgets**, use X/Y or enable Position mode
and drag **Move gadget** / **Move radar** in the panel. The overlay stays
click-through throughout; direct dragging on the overlay is not supported.

Wayland placement is saved separately from Windows/X11 placement. One common
Wayland layout is reused across monitors. Temporary fitting keeps gadgets
inside the selected monitor and shrinks oversized gadgets only for display.
The chosen gadget scale remains intact. **Save settings** persists the adjusted
displayed positions while preserving the chosen scales.

If the selected monitor is unavailable, the HUD hides and the panel warns.
Reconnect it to restore the HUD, or choose another monitor manually. Display
scale is separate from gadget scale: fractional scaling uses appropriately
sized GPU buffers and the same logical placement.

## Visibility and errors

**Show overlays** controls visibility manually, independently of each gadget's
Enabled preference. Press **Insert** to toggle it globally on KDE/KWin, even
while LFS has focus. Configure `overlay_toggle_key` or use `"None"` to disable
it; see [shortcut settings](usage.md). Game-focus detection is deferred.
Native startup or rendering errors keep the panel open with an error and
**Retry native overlay**. There is no automatic X11 fallback. Closing the panel
stops the native worker and removes its surface.

Diagnostics appear in `openradar.graphics.log` beside the selected config.
The native readiness entry records output, logical and physical sizes, and
scale. This entry proves presentation startup, not fullscreen or input delivery.

## Configuration

These are optional additions to the existing TOML configuration:

```toml
overlay_backend = "auto" # top-level; or "window" / "wayland" to override

[wayland]
monitor = "DP-1" # choose the name shown by the monitor selector

[wayland.radar]
x = 120.0
y = 240.0
scale = 1.25

[wayland.gap_ahead]
x = 24.0
y = 24.0
scale = 1.0
```

The other placement tables are `wayland.gap_behind`,
`wayland.performance_delta`, `wayland.speed_dashboard`, and `wayland.fuel`.
Omitted X/Y values use the initial arrangement. Radar scale accepts
220/320 through 800/320, matching the panel's Radar size control; the other
gadgets accept 0.5 through 2.0. Coordinates and scales must be finite.
Gadget enablement stays in the existing settings, not the placement tables.

## Verification and limits

Local tests have verified native GPU startup with the actual GT radar and
dashboard textures, coexistence with the XWayland panel and all six demo
gadgets, unavailable-monitor selection recovery, hide/show control, Retry,
and clean shutdown. Deterministic tests cover independent layout persistence,
panel move/save routing, requested versus displayed geometry, and fractional
physical sizing.

Focused/fullscreen LFS stacking, actual mouse/scroll/keyboard delivery,
physical monitor unplug/replug, and visual sharpness at 125%/150% still need
live acceptance. Do not infer those results from a successful build or log.
Use the [confirmed acceptance checklist](wayland-gt-integration-plan.md#5-verify-and-document-the-first-supported-setup).
Other compositors, Gamescope, and a native Wayland control panel remain outside
the initial support target.

The real-desktop integration tests are opt-in:

```bash
cargo test --locked --test native_wayland -- --ignored --test-threads=1
```

They briefly display demo overlays and do not change desktop monitor settings.
