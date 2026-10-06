# LFS OpenRadar roadmap

Updated 6 October 2026. This is the forward product roadmap after the `v0.1`
release. Features below are planned unless marked released; milestone numbers
describe delivery order rather than promised release dates. The
[original radar research](proximity-radar-plan.md) retains its detailed radar
acceptance criteria.

## Product direction

Grow OpenRadar into a modular Rust overlay for Live for Speed. The proximity
radar becomes one widget alongside split timing, gear, RPM, and gaps ahead and
behind. Drivers can enable only the gadgets they want, position and resize them,
and save their layout from one control panel.

A separate analyzer repository will own the analyzer application, modular
backend, database integration, and frontend UI. It will have its own roadmap,
build/CI pipeline, issues and releases. OpenRadar's product scope is the radar and
widgets. The analyzer will show speed changes, driving lines, and track maps
with split/sector markers using its own telemetry ingestion. OpenRadar does not
serve as its collector, and either application can run without the other.

Within OpenRadar, keep one telemetry connection and session model shared by its
radar and widgets; individual widgets do not open their own sockets. The analyzer
owns a separate connection and session model. Both applications may reuse Rust
protocol/geometry libraries at build time without sharing a required runtime
service. They have separate configuration, process lifecycle and releases.
The analyzer repository has not been created or named yet. Shared library
extraction is optional future work; neither app depends on the other's checkout.

Native Windows remains the first supported overlay deployment; native Linux
beside LFS through Proton remains a separate acceptance target, starting with
X11. Wayland and Gamescope need their own platform work and verification.

## Telemetry and widget scope

| Widget | Primary inputs | Planned behavior |
| --- | --- | --- |
| Proximity radar | InSim `IS_MCI` + required OutSim MAIN/TIME | Preserve opponent footprints, local orientation smoothing, and existing stale/association gating. Released in `v0.1`; broader driving checks remain open. |
| Gear | OutSim2 `OSO_DRIVE` | Show R, N, or the forward gear. Hide the current value when the drive block is absent or stale. |
| RPM | OutSim2 `OSO_DRIVE` | Convert `EngineAngVel` from radians/second to RPM using `value * 60 / (2 * pi)`. Provide numeric and bar presentations; configurable shift cues come later. |
| Split and lap timing | InSim `IS_SPX`, `IS_LAP`, session/player lifecycle and validity information | Show completed splits, sectors, lap time, and delta to the driver's best valid lap. Keep session records as a separate comparison. |
| Gap ahead / behind at timing lines | InSim split/lap events, lap identity and race order | Compare elapsed race times at the same timing line and lap. Identify the compared driver and retain the last measurement with its age. |
| Continuous gap ahead / behind | InSim MCI path node/lap/order/positions + track path/progress model; OutSim local distance where available | Estimate time gaps from matching progress histories. Label them estimated and withhold values when progress or source association is unreliable. |

The installed LFS developer file `docs/OutSimPack.txt` defines `OSO_DRIVE` as
option bit `0x20`, with Gear and EngineAngVel, and `OSO_DISTANCE` as `0x40`, with
CurrentLapDist and IndexedDistance. OpenRadar's default `OutSim Opts 1ff` already
includes these blocks. The current decoder validates the configurable packet
size but only exposes time and pose; extracting the extra fields is future work.
Legacy OutSim has no drive block, so it can support the radar without supplying
gear/RPM widgets. The [official InSim specification](https://www.lfs.net/programmer/insim)
points to that developer file and documents timing events and MCI progress fields.

OutGauge remains an optional later dashboard adapter for fields such as fuel and
dashboard lights. Gear and RPM do not require it when the modern OutSim drive
block is enabled. Future OutGauge support must validate its own layout and
driver association, and coexist with other UDP consumers. Existing behavior
ignores standard OutGauge packets; it does not decode them.

## OpenRadar internal architecture

```text
Local LFS
  InSim TCP -> roster, lifecycle, MCI, split/lap/validity events
  OutSim UDP -> pose, drive data, inputs, distance when configured
                         |
                shared telemetry/session model
                         |
            radar / dashboard / timing / gap calculations
                         |
                 latest immutable UI snapshot
                         |
         widget registry -> overlay host + control panel
```

Start with built-in Rust modules behind a small widget interface: stable widget
ID, settings, required telemetry capabilities, presentation state, and render
method. Additional widgets should fit this interface without changing network
ownership. Runtime-loaded plugins or scripting can be evaluated after the
built-in contract is stable.

Process every timing/lifecycle event in the telemetry worker before publishing
the latest snapshot; a latest-frame handoff must not drop lap or split events.
Keep histories bounded. Widgets never block on sockets, and rendering reads a
consistent snapshot rather than combining unrelated update times.

Track availability and age per input and widget. Missing OutSim must still pause
the radar, as required by its MVP, while an InSim-only timing widget can retain
its last measured split. A missing gear value is unavailable rather than neutral
or zero RPM. Clear live values on disconnect or driver changes; keep deliberately
historical results visibly distinct from live measurements.

The overlay host owns transparency, placement, foreground visibility, DPI,
position mode, and repaint scheduling. Radar, Gap ahead, and Gap behind use
independent native overlay windows so each can be positioned freely on the
desktop. Each owns its saved geometry, enable state, positioning mode, resize
debounce, and repaint timer while sharing the telemetry runtime. Hide/show keeps
each window and graphics surface registered rather than recreating them.
Retain the working radar-only layout as the default during migration. Native
multi-window stability, DPI, monitor transitions, and live driving remain
acceptance targets as more gadgets are added.

The control panel presents built-in gadgets as cards in a responsive grid.
Cards follow a stable insertion order, filling each row from left to right.
A newly added gadget occupies the next space to the right; when another card
cannot fit within the available window width, it starts the next row at the
left. Resizing the control panel recalculates the column count while preserving
card order. Use consistent card widths, top-aligned rows, and spacing; narrow
windows fall back to one column, and extra rows remain accessible through
vertical scrolling. Each card contains its gadget preview, enabled state, and
gadget-specific controls. Connection, global overlay controls, and save actions
stay outside the card grid. This control-panel arrangement is independent of
the gadgets' saved native window positions over the game.

Save versioned TOML settings with widget IDs, enabled state, position, size,
units, style, and widget-specific options. Migrate existing radar settings without
losing placement or ranges. Add layout profiles and import/export once the base
format is stable; exclude passwords from persisted or exported settings.

## Delivery milestones

Feature branch `codex/live-gap-gadgets` implements the control-panel wrapping
card grid and the first common-node race-order gap estimates, including saved
enable/position/scale settings, separate radar/ahead/behind overlay windows, and
deterministic demo coverage. These changes
are unreleased. Continuous sub-node progress, measured split/finish gaps, the
full widget-layout architecture, and live driving acceptance remain outstanding.

### 0. Radar baseline — released as v0.1

Native Windows executable, transparent click-through radar, control panel,
position mode, settings, diagnostics, and setup/troubleshooting documentation.
Continue the driving and graphics acceptance checks in the README; a published
release does not complete the Linux, DPI, fullscreen, or driver stability matrix.

### 1. Modular overlay foundation

- Extract a built-in widget registry and overlay host from the radar-specific UI.
- Add widget enable/disable, selection, move/resize, and saved layout settings.
- Arrange control-panel gadget cards in a responsive grid: append to the right,
  wrap to the left of the next row when full, and preserve order during resize.
- Add shared telemetry capability/age reporting and deterministic demo data.
- Keep the radar's geometry and required OutSim behavior intact.

Acceptance: the default radar behaves as before; independent widget repainting
does not depend on moving a slider or repainting the control panel. Exercise
grid insertion, exact row boundaries, narrow-window single-column layout,
resize reflow, and vertical scrolling with enough cards to exceed the window.
Also exercise position mode, rapid visibility toggles, settled resizing, Alt-Tab, and multiple
monitors without repeating the previous redraw and freeze regressions.

### 2. Gear and RPM gadgets

- Extend OutSim decoding to expose the optional drive block and its timestamps.
- Add gear and RPM widgets, individual visibility, scale, and visual settings.
- Support default full OutSim2 packets and other valid configured layouts;
  report missing capabilities for legacy packets or disabled drive fields.

Acceptance: verify R/N/forward gear mapping, RPM conversion and plausible values,
malformed/non-finite inputs, time reset/wrap, stale data, and driver association.
Compare displayed values against LFS while driving and shifting. Enabling these
widgets must not require another telemetry socket or OutGauge configuration.

### 3. Split, sector, and lap timing gadget

- Decode split/lap and relevant session, reset, takeover, penalty and validity
  events. Store millisecond values; format them only when rendering.
- Distinguish cumulative split times from individual sector times, including the
  final sector calculated from lap time minus the last cumulative split.
- Keep the driver's best valid lap reference separate from session-wide records.
  Do not assemble a personal reference by combining best sectors from different
  laps. Show an unavailable reference until a valid complete lap is observed.
- Reset references on session/track/car changes and current-lap state on rejoin;
  reject incomplete/invalid laps. Define validity per supported LFS mode and
  show unknown status when the available events cannot establish it.

Acceptance: compare timing with LFS through practice, qualifying, races,
different split counts, invalid laps, restarts, pit/rejoin, takeover and reconnect.
Joining mid-lap must not invent a lap start or historical personal best.
Continuous delta against a recorded distance-based lap trace is a later step;
completed split delta is the first deliverable.

### 4. Gap ahead and behind gadgets

- Start with measured split/finish-line gaps and last-measurement age.
- Preserve MCI node, lap, and race-position fields currently omitted from the
  radar's car model. Define race-order neighbors separately from nearest cars
  along the track; show lapped status and handle ties or unknown positions.
- Add continuous estimates after validating track progress and common-point
  crossing histories. Straight-line distance divided by current speed is not a
  reliable race time gap, especially around corners or stopped cars.
- Use local OutSim distance as an additional progress input where available;
  opponent progress still needs InSim and a track model. Match clocks and
  identities explicitly; packet arrival is an approximation for continuous
  estimates, while timing-line event elapsed times provide measured anchors.

Acceptance: test lap-boundary wrap, lapped traffic, overtakes, stopped/spinning
cars, pit lanes, alternate layouts, reconnect, latency and missing path data.
Never compare different laps as though they were the same timing-line gap.
Expose unknown or estimated status instead of a falsely precise number.

### 5. Expanded gadgets and platform delivery

Consider speed, throttle/brake/clutch, acceleration, race position, flags,
fuel/dashboard lights through optional OutGauge, and layout profiles. Add widgets
only when their inputs and usefulness are established. Shift thresholds need
configurable or validated car/mod-specific values rather than one universal RPM.

Package and exercise the modular overlay on native Linux/X11 beside LFS through
Proton. Record desktop, compositor, GPU/driver, Proton/LFS version and display
mode. Advance Wayland or Gamescope support only after its overlay behavior is
implemented and verified. Continue shipping Windows builds independently of
these platform experiments.

## External analyzer project — separate repository

The analyzer is outside OpenRadar's delivery milestones. The initial design
notes below preserve the discussion until its repository exists; its detailed
roadmap and implementation will belong there. OpenRadar retains only the
cross-project telemetry coexistence requirements. These notes do not add an
analyzer backend, database or web frontend deliverable to this repository.

Deliver a live speed trace, a map of the driver's actual line colored by speed,
and track split/sector markers, followed by saved-lap comparisons and replay of
recorded telemetry. Treat real-time presentation and asynchronous recording/
analysis as complementary paths rather than competing architectures.

```text
Local LFS
  -> OpenRadar-owned telemetry inputs
       -> local session model -> radar and widgets
  -> analyzer-owned telemetry inputs
       -> analyzer backend
            -> rolling live state -> stream -> frontend
            -> recorder -> database -> analysis jobs -> query API -> frontend
```

These are independent logical inputs; simultaneous OutSim delivery is a
prototype gate, not a verified capability of the current setup.

**Connection/coexistence gate:** InSim supports up to eight external programs;
each application can open its own TCP connection to the same local listener,
with its own subscriptions, keepalive, roster and lifecycle state. Each consumes
a connection slot. The cfg.txt OutSim setup defines one IP/port destination;
opening another UDP receiver does not establish a second copy of that stream.
Do not assume binding both apps to the same unicast UDP port duplicates packets.

Investigate connection-requested OutSim through `SMALL_SSP` and each connection's
`IS_ISI.UDPPort`. The specification documents this when OutSim is not configured
in cfg.txt; it does not establish concurrent per-connection delivery for our
current cfg-driven setup. Verify independent destinations, packet layout/rate,
starting/stopping either app, and coexistence with existing telemetry consumers
on the supported LFS version before promising simultaneous operation. If direct
delivery cannot satisfy this, decide explicitly on a transport-only datagram
fan-out adapter or another verified source; do not make OpenRadar the analyzer's
collector by default. Do not reconfigure LFS or its telemetry automatically.
Source: [official InSim/OutSim specification](https://www.lfs.net/programmer/insim).

**Live path:** collect validated source samples, maintain short histories, and
incrementally calculate speed changes, current line, and sector progress. Push
bounded updates to the frontend over a stream, initially WebSocket, rather than
polling the database per frame. A reasonable prototype target is a 10–20 Hz web
display feed with a measured end-to-end delay budget of 100–200 ms; these are
targets, not current performance claims. Retain telemetry at its received rate
when recording is enabled. Rendering can interpolate independently, but should
not imply fresh physics samples at its frame rate.

**Recording and analysis path:** batch telemetry writes on a recorder worker;
trigger completed-lap processing from timing events after the required samples
are committed. Compute distance-aligned speed comparisons, braking/acceleration
zones, line differences and sector summaries outside the live socket/render
path. Jobs may run during driving as well as after a session. Expose progress and
completed results through the backend API, and make job retries idempotent.

The analyzer records its own validated source telemetry before display
interpolation/downsampling and does not ingest rendered radar frames.
Use a versioned contract carrying session, stream/sequence, driver/car identity,
track/config/layout identity, lap identity, source timestamps when present,
monotonic receive time, units and quality flags. Retain source clocks separately
and record their alignment; a backend's network receive time must not replace
the original capture timeline. Start a new clock/identity epoch on reset or
reconnect, and distinguish missing samples from interpolation.

For speed, expose OutSim velocity and define whether each display uses ground
speed or full 3D speed. Derive speed change over a timestamped window, with
explicit smoothing rather than a noisy single-sample difference. OutSim
positions provide the local driving trace; position alone is not a track mesh
or an optimal reference line. Compare traces against a selected recorded lap,
resampled by validated track progress, not merely equal elapsed time.

Use version-matched track/path assets where available for map geometry and
InSim `IS_RST` timing metadata for finish/split node indices. `IS_SPX` and
`IS_LAP` establish timing crossings. Standard circuits, reverse configurations,
pit paths and custom checkpoint layouts require explicit mapping rules; show
unsupported/missing geometry rather than fabricated split locations. Sources:
[LFS programmer assets](https://www.lfs.net/programmer) and
[InSim specification](https://www.lfs.net/programmer/insim).

The analyzer's Rust service can use async socket/API handling, with its own
ingestion, storage and CPU-heavy analysis isolated behind interfaces. OpenRadar
retains its existing telemetry worker model; changing the analyzer does not
require an overlay async rewrite.
Database and frontend framework selection remain open until local-only versus
shared/hosted deployment is decided. Begin with one modular service containing
ingest, live analysis, recording, historical analysis and API modules.

Use separate delivery policies. A live consumer can skip superseded display
frames and resynchronize after lag. A recorder needs ordered samples, durable
acknowledgment, deduplication, and bounded retry/spool storage. Never let a slow
database or remote upload block the analyzer's live feed. If recording capacity
is exhausted, report an explicit recording gap or stopped recording while keeping
live output responsive. OpenRadar has no backend dependency. Recording/upload is
opt-in within the analyzer, with retention and disk-limit settings.

If Tokio is adopted, `watch` fits latest display state, while bounded `mpsc`
fits work queues with an explicit overflow/backpressure policy. Neither provides
durability by itself. Broadcast receivers can lag and lose messages, so they
must not be the only recording mechanism. CPU-heavy comparisons need a bounded
compute worker pool rather than running on async I/O tasks. References:
[watch](https://docs.rs/tokio/latest/tokio/sync/watch/index.html),
[mpsc](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html),
[broadcast lag](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html), and
[blocking/CPU work](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

Acceptance: verify live latency, timestamp ordering/reset/wrap, identical results
from replaying a saved input, lap/split alignment, reverse/custom maps, invalid or
partial laps, bounded storage, backend failure/recovery, retry deduplication and
slow frontend consumers. Verify each app with the other stopped, both startup
orders, separate reconnection/shutdown, and simultaneous OutSim delivery.
Demonstrate that recording and analysis do not cause radar stalls through shared
machine resource contention or repeat the previous GUI/redraw failures.

## Validation approach

Use deterministic decoder/calculation fixtures and a demo snapshot for every
widget, including missing, stale, reset and partial-capability states. Test config
migration and bounded event/history handling. Measure CPU/GPU/frame-time impact
with all first-wave widgets enabled, and keep updates bounded so one widget
cannot stall the panel or the radar. Live acceptance should include ordinary
multiplayer participants connecting to their local client without remote admin
credentials, and coexistence with LFSLapper and other telemetry applications.

Do not change the released `v0.1` assets to add planned functionality. Each
implemented milestone gets its own release notes and verified binary when ready.
