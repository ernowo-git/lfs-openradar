# Native Wayland GT HUD integration plan

Status: confirmed on 2026-10-14; integration implemented with TDD. Native GPU demo and recovery checks pass; live driving and the remaining display/input acceptance checks are pending. See the [native Wayland guide](native-wayland.md).

## Outcome

Add a native Wayland overlay backend to the actual application, targeting KDE/KWin first. Display all six existing gadgets with live LFS data and their current appearance and calculations. Validate GT first; preserve the existing style selector and shared painter code.

Keep the current XWayland control panel, including settings, previews, telemetry controls, Apply / reconnect, Save settings, and gadget enablement. Windows and X11 retain their existing overlay behavior and saved positions.

## Agreed behavior

- One shared transparent overlay on a selected monitor. All gadgets retain independent positions and scales.
- Monitor-relative positions use logical pixels. Display scale and gadget scale remain distinct.
- Auto is the default backend preference after the approved follow-up. It selects native Wayland on supported KDE/KWin Wayland sessions and Window elsewhere; explicit choices remain overrides.
- Show overlays controls visibility manually, including the implemented KDE global show/hide shortcut. Linux game-focus detection is deferred.
- Position mode uses the panel's X/Y and Move gadget controls. The overlay stays click-through and does not request keyboard focus. Direct dragging is deferred.
- Persist one common Wayland layout and the selected monitor separately from existing Windows/X11 positions. Per-monitor layouts are deferred.
- Missing selected monitor: hide the HUD, show a warning in the panel, and restore it when that monitor returns. Allow manual selection of another monitor.
- Support fractional scaling now, including 125% and 150%, with physically sized rendering buffers and sharp fonts/icons.
- Keep each gadget fully on-screen where it fits. If it is too large, temporarily reduce its displayed scale enough to fit while preserving its requested scale.
- Keep requested positions and the saved HUD layout separate from displayed positions. Save settings explicitly persists the adjusted displayed positions; temporary fitting does not overwrite chosen gadget scales.
- Native initialization or rendering failure keeps the panel open with a clear error and Retry. Switching to X11 requires explicit user action.
- Preserve gadget enabled/disabled settings. First-run defaults define placement and do not enable every gadget automatically.

### First-run placement

| Gadget | Location |
| --- | --- |
| Gap Ahead and Gap Behind | Upper-left, stacked |
| Performance Delta | Upper-centre |
| Fuel | Lower-left |
| Speed dashboard | Lower-centre |
| Radar | Lower-right |

Use margins and the actual gadget dimensions to calculate the initial coordinates. Fit them to the selected monitor. Gadgets remain freely positionable; automatic collision avoidance is outside this first version.

### Domain examples

These examples apply the agreed behavior using the glossary's terms.

| Situation | Result |
| --- | --- |
| Show overlays is off while Radar is enabled | Radar is hidden; its enablement and HUD layout remain unchanged. |
| A disabled Fuel gadget has a saved position | Fuel keeps its layout entry and uses that placement when enabled. |
| The selected monitor is disconnected while Show overlays is on | The HUD is hidden and the panel warns; the selected monitor, visibility preference, and saved HUD layout remain unchanged. |
| A gadget at scale 2 fits only at scale 1.5 on a smaller monitor | Its displayed gadget scale is 1.5; its chosen gadget scale remains 2 and becomes usable again when space permits. |
| A requested position is outside the monitor and the user has not saved | The displayed position is constrained; the requested position and saved HUD layout remain unchanged. |
| The user saves after temporary position and scale fitting | The adjusted displayed positions become saved placement; chosen gadget scales are preserved. |

## Architecture

The accepted architecture and coordinate decisions are recorded in [ADR-0001](adr/0001-native-wayland-overlay-worker.md) and [ADR-0002](adr/0002-separate-wayland-hud-layout.md). Domain terminology is defined in the root [glossary](../GLOSSARY.md).

Keep eframe/XWayland on the main thread. A dedicated native Wayland worker owns its SCTK/calloop loop, Wayland connection, Layer Shell surface, egui context, wgpu resources, and egui-wgpu renderer. Its surface covers the selected output; unused pixels are transparent.

Reuse existing gadget painters, theme resolution, embedded fonts, and dashboard/fuel textures. Use the egui-wgpu version already exposed by eframe and resolved in Cargo.lock. Enable the Wayland system backend required for native display/surface handles. Encapsulate the raw-handle lifetime boundary and destroy GPU surface resources before the underlying Wayland surface and connection.

The panel remains the sole owner of Runtime and its LFS connections. The worker reads a cloned SnapshotReader, or the existing demo source, independently of panel repainting. Read one snapshot per grouped frame so all six gadgets display consistent data.

Publish the latest settings, layout, and visibility to the worker, with a wakeup rather than a backlog of slider changes. Report ready/error/closed status back to the panel and request panel repaint when status changes. Process commands while hidden even if frame callbacks have stopped. Panel shutdown wakes, stops, and joins the worker.

Use the selected output's stable name for saved selection on KDE/KWin. Never silently replace a missing saved selection with a different output. On first use, choose an available output and show that choice in the selector.

## Implementation sequence

### 1. Prove the GPU integration seam

Before broad integration, run the existing XWayland panel alongside a worker-owned Layer Shell surface rendered through wgpu. Reuse representative GT text and a dashboard texture, instead of extending the mock software renderer.

Gate completion requires transparent GPU output, correct initial configure/commit ordering, click-through, no focus stealing, hide/show recovery, clean shutdown, and correct 100%/125%/150% sizing on KDE/KWin. Verify focused/fullscreen stacking with GPU presentation; the working software prototype is supporting evidence, not a substitute for this check.

If GPU alpha or mixed event-loop behavior fails, resolve it before implementing the full feature. Report the concrete failure instead of silently reverting to X11.

### 2. Add backend selection and the six-gadget renderer

Add a saved Auto/Window/Native Wayland preference and a CLI override. Auto resolves at startup without replacing the saved preference. Display the active backend in the panel. Applying a backend change can require an application restart; live backend switching is outside this first version.

Route native mode to the worker and suppress the existing six child overlay windows. Reuse all six painters, settings, fonts, textures, and enablement rules. Preserve existing demo and live telemetry calculations, stale/unavailable states, and style selection.

Completion: all six gadgets render from one snapshot on one native surface in demo mode, and the control panel remains responsive.

### 3. Add monitor selection and saved Wayland layout

Add a backward-compatible Wayland configuration section for selected monitor and independent gadget positions/scales. Keep existing Windows/X11 fields and values intact. Route the same panel controls to the active backend's layout.

Implement first-run placement, monitor-relative coordinates, temporary position constraints, effective fit-to-monitor scaling, explicit saving, and fractional buffer scaling. Keep requested scale separate from displayed scale, including the radar's existing size control.

Completion: independently move/scale each gadget, save/restart, and switch backends without losing either layout. Fractional scaling does not alter the logical placement or requested gadget size.

### 4. Complete visibility, monitor lifecycle, and failure recovery

Handle Show overlays, enabled gadgets, monitor changes/removal/return, output resolution/scale changes, Apply / reconnect, renderer errors, Retry, and application shutdown. Preserve the single telemetry connection owner and keep repainting when the panel is idle or minimized.

Completion: hidden overlays respond to controls, missing-monitor recovery restores the intended output, failed workers can be retried without duplicate workers/connections, and closing the panel removes the overlay cleanly.

### 5. Verify and document the first supported setup

Add focused checks for layout round trips and backend isolation, logical-to-physical sizing, requested versus displayed geometry, monitor disappearance, and relevant worker lifecycle transitions. Reuse existing telemetry tests instead of duplicating their calculations. Run formatting, appropriate feature builds, existing tests, and clippy, including Windows compilation coverage where available.

Live acceptance on KDE/KWin must cover:

- Actual LFS data in all six gadgets, including unavailable/stale transitions and reconnect.
- Current GT visuals, embedded typography/icons, and transparent gaps/backgrounds.
- Fullscreen and borderless LFS, repeated Alt+Tab, mouse/scroll delivery through painted gadgets and gaps, and retained game keyboard focus.
- Independent panel placement/scale edits, gadget enablement, Show overlays, save/restart, and unchanged Windows/X11 configuration.
- Monitor selection, unplug/replug or equivalent output disable/enable, resolution changes, and 100%/125%/150% scaling.
- Oversized and off-screen gadgets, preservation of requested scales, and explicit saving of constrained positions.
- Continued animation while the panel is idle/minimized, error/Retry behavior, and clean exit.

Record compositor, GPU/driver, display scale/resolution, game launch method, display mode, and results. Update usage, configuration, development, and troubleshooting docs around the verified behavior. Broad compositor support is a later effort.

## Evidence and remaining uncertainty

Source inspection supports the worker seam: the panel already shares SnapshotReader with child renders; painters are separable from native windows; eframe exposes the matching egui-wgpu renderer; Wayland and wgpu provide native raw-handle interoperation. Fractional-scale and viewporter protocols are available through the existing SCTK dependency tree.

Runtime uncertainty remains in GPU alpha capabilities, initial buffer presentation ordering, fractional-scale resize timing, fullscreen stacking, and recovery when frame callbacks are suspended. The first gate exists to resolve those risks on the target machine.

## Deferred work

Native Wayland control panel, direct on-overlay dragging, automatic game-focus visibility, per-monitor layout profiles, automatic runtime failover, live backend switching, and support claims for other compositors.

## Follow-up: Linux show/hide shortcut

The user subsequently requested a Linux hotkey. KDE/KWin now registers the
existing `overlay_toggle_key` (Insert by default) globally for both presentation
backends. Conflicts or unavailable services keep the panel shortcut available
with a warning. This follow-up expands the original shortcut deferral; game-focus
detection remains deferred. See [usage](usage.md) and the [live test](development.md#linux-global-shortcut-checks).

## Follow-up: automatic backend selection

The user approved automatic backend selection after the initial implementation.
Auto is now the default preference. Linux KDE/KWin Wayland sessions with a
reachable connection and advertised Layer Shell use native presentation;
Windows, Linux X11, other desktops, and builds without native support use Window.
Explicit saved and CLI choices remain overrides, and saving Auto preserves that
preference. The probed Wayland connection is reused for startup. Native rendering
failures remain visible for retry or an explicit switch; runtime failover is
still deferred. See [ADR 0003](adr/0003-automatic-overlay-backend-selection.md).
