# Position/range editing freeze investigation — 2026-10-06

The user reported a black screen and whole-machine freeze requiring a hard
restart while editing the overlay in position mode. The exact slider involved
was uncertain: overlay size, side range, or both.

For current operating steps and symptom-based checks, see
[the troubleshooting guide](troubleshooting.md). This document records the
investigation and the successive changes, including superseded workarounds.

## Evidence from this machine

Read-only Windows Event Log and display adapter registry inspection found:

| Evidence | Time / value |
| --- | --- |
| Watchdog dumps with LiveKernelEvent 141 | `WATCHDOG-20261006-1511.dmp` and `WATCHDOG-20261006-1514.dmp` |
| Matching AMD user-mode driver reports | `AMD_REPORT_UM-20261006-1511.dmp` and `AMD_REPORT_UM-20261006-1514.dmp`; report code `a2000002` |
| Unclean restart events, Kernel-Power 41 | October 6, 15:12:19 and 15:15:01, local Windows time |
| Event reporting time | Both pairs were recorded in Application / Windows Error Reporting, event 1001, at 15:18:09 |
| Adapter | AMD Radeon RX 6700 XT |
| Installed driver registry values | Version `32.0.21045.5002`; date `8-17-2026` |
| Older watchdog reports | LiveKernelEvent 117 dump dated September 30; additional watchdog reports dated September 12 |

[Microsoft documents 0x141](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x141---video-engine-timeout-detected)
as a display engine failing to respond in time. Event 41 establishes an unclean
restart; it does not establish its cause. The watchdog/AMD timestamps correlate
with the two restarts. Older reports show that graphics timeouts have occurred
before this incident, without identifying the causes of those earlier events.

No crash dumps were opened, no driver or Windows settings were changed, and the
freeze was not deliberately reproduced. The selected wgpu backend for the
failed sessions was not logged. The event data alone does not identify the
application submission, API, driver function, or hardware condition responsible.

## Code findings and changes

- The size slider previously changed the native overlay size throughout a drag.
  Each distinct size could trigger swapchain reconfiguration. It now waits for
  no mouse button to be held and 250 ms without a value change, then commits
  one rounded native size. Keyboard changes use the same settling interval.
- Position mode previously switched native decorations on the transparent
  overlay. The initial mitigation kept it borderless and used a native drag
  command. A later workaround removed that path; the final fix below restores
  native decorations for title-bar dragging after patching redraw delivery.
- Observed OS positions were previously written to Config and subsequently
  supplied back to the viewport builder. This could generate unnecessary native
  moves during OS dragging or asynchronous position updates. Observed positions
  are now used for saving; only explicit X/Y control edits request native moves.
  A position-feedback oscillation was not demonstrated on this machine.
- The overlay was omitted whenever hidden, allowing its window and GPU surface
  to be discarded and recreated after focus changes. Once first shown, it now
  stays registered and changes native visibility instead.
- Immediate viewports coupled the two windows' painting. A deferred viewport
  now lets the backend schedule each window separately.
- A bounded graphics log now records adapter/backend/driver information,
  settled native resizes, overlay toggles, and graphics warnings/errors. It
  does not log passwords or network packets.

The side-range slider affects a finite drawing scale and radar configuration;
it does not request native window resizing. Its drawing loop has a fixed three
rings and at most the bounded set of tracked cars. No unbounded allocation or
invalid arithmetic was identified in that path.

Upstream egui has also reported
[Windows resize hangs](https://github.com/emilk/egui/issues/8123) and released
[surface lifecycle recovery changes](https://github.com/emilk/egui/pull/8171).
Those concern different dependency versions and symptoms and do **not** prove
that this incident has the same cause. No speculative driver settings or
unreleased upstream patches were applied at this initial stage.

## Follow-up: control panel freezes on entering position mode

After the deferred overlay was changed to read live telemetry independently,
the user reported that the control-panel preview and input stopped immediately
on enabling position mode, while the overlay continued updating. No radar drag
was needed. The pasted log ends at `Position mode true`, with no new graphics
error there. The active backend is confirmed as Vulkan on the RX 6700 XT.

Entering position mode changed `MousePassthrough` from true to false. Inspection
of winit 0.30.13's Windows implementation shows this changes
`WS_EX_TRANSPARENT` / `WS_EX_LAYERED`, calls `ShowWindow`, and refreshes the native
frame. This is a plausible focus/redraw trigger, not proof of the exact failure.
The earlier native drag handler is not an explanation for a freeze before
dragging.

Upstream has reported
[Windows deferred-viewport repaint delivery failures](https://github.com/emilk/egui/issues/8466),
including interaction stopping when repaint delivery fails. An
[open redraw-ledger proposal](https://github.com/emilk/egui/pull/8650) addresses
that class of problem. Its direction of focus loss differs from this report;
it is supporting context, not confirmation of an identical cause. No unreleased
backend patch was added at this stage.

The interim workaround kept the overlay permanently click-through and moved all
position input into the control panel. Position mode exposed a **Move radar**
drag handle alongside **X/Y** controls; dragging it requests only an outer
position change. Entering/leaving position mode changed no native input or frame
style. The overlay could not intercept clicks intended for the panel underneath.
The root also stops issuing an immediate child repaint request every frame;
the overlay uses its own refresh timer and is explicitly woken when first shown
or shown again.

The graphics log now reports once if the overlay is repainting while the panel
has not repainted for five seconds, and reports when panel painting resumes.
That observation can also occur for a deliberately minimized/obscured panel.
This provides evidence if redraw delivery still fails after the workaround.

## Follow-up: overlay refreshes only while resizing

The user confirmed the prior control-panel input workaround stopped its freeze,
but also reported that the overlay still updated only when moving the size
slider. They requested direct window dragging with a border in position mode.
This demonstrates that a timer and fresh snapshot access alone did not resolve
native redraw delivery. The previous automated callback checks covered frame
freshness after a callback runs, not whether Windows delivers that callback.

OpenRadar now uses a vendored eframe 0.33.0 with a Windows redraw-request tracker
adapted from the open upstream proposal linked above. It retains the oldest
unanswered request for each native window. After a delivered redraw is painted,
the backend directly paints older outstanding requests, within a 5 ms soft
budget; a paint already started may exceed that budget. Work that is not reached
remains pending. This addresses both overlay and panel starvation without
depending on an extra resize event. Linux keeps its existing scheduling behavior.

Position mode now restores OS decorations and mouse input so the native title
bar can move the overlay. The custom `StartDrag` callback remains removed.
Leaving position mode restores borderless click-through behavior. Settled size
changes, retained hidden surfaces, and fresh worker snapshot reads remain.
The panel drag handle and X/Y remain alternatives for moving the radar.

The local patch is described in
[vendor/eframe/OPENRADAR-PATCH.md](../vendor/eframe/OPENRADAR-PATCH.md). The source
is reviewed and tested locally; it is not claimed to be an upstream released
fix or proof that the earlier GPU hard lock cannot recur.

## Verification and remaining limits

Regression checks exercise egui's actual `ViewportBuilder::patch` commands
without a native window or GPU. They check that slider dragging emits no native
resize, settling emits one resize, returning to the original size cancels it,
keyboard edits wait for idle, position mode/hiding preserve geometry, and an
explicit position edit emits one move. Existing protocol, geometry, and socket
tests continue to exercise the radar independently of graphics.
CPU tessellation also checks minimum/maximum overlay sizes and side ranges with
48 opponents, requiring valid finite meshes and fewer than 20,000 vertices.

Completed verification for the current mitigation build:

- `cargo fmt --check`: passed.
- `cargo test --locked --offline`: all 30 tests passed. New checks exercise a
  retained overlay callback transitioning paused/live/paused without a root
  repaint, and real egui pointer events for the control-panel drag handle. The
  latter requires one position command during panel-handle dragging and a
  working Quit click afterward. Mode-toggle checks require decorations and
  pass-through changes without window recreation or explicit GPU size commands.
  Four backend tracker tests simulate missing native redraw events in both
  directions, duplicate requests, budget-delayed work, and newer requests.
- `cargo clippy --all-targets --locked --offline -- -D warnings`: passed.
- `cargo build --locked --offline`: passed; the normal debug executable rebuilt.
- `cargo check --no-default-features --locked --offline`: passed.
- Rebuilt executable with `--demo --headless --seconds 1`: exited successfully
  with connected synthetic MCI/OutSim, three nearby cars, and no rejected or
  malformed packets. No GPU window was opened.

After the final build, the user reported success on 2026-10-06. This provides
live feedback that the redraw and positioning fixes worked on this Windows
setup. Passing automated checks and this feedback do not establish prevention
of a driver hard lock or complete the broader driving, DPI, and Linux acceptance
checks. If another failure occurs, the graphics log and matching Windows event
reports would be the next diagnostic inputs; repeated hard-lock reproduction
is not part of this investigation.
