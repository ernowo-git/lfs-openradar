# Fuel gadget research and implementation plan

Status: implemented; automated desktop and headless checks completed. Live LFS acceptance remains pending.
Date: 8 October 2026.
Reference: C:/Users/winslop/Downloads/Frame 13.svg (534 × 224 logical pixels).

## Feasibility

Yes. Live fuel percentage and estimated laps until empty are possible with the existing connections. Estimated finish margin is possible for standard lap-count races. The user subsequently removed the litres display and capacity editor. Timed-race finish estimates need additional work.

The [official LFS protocol specification](https://www.lfs.net/programmer/insim) documents OutGauge Fuel as a 0–1 fraction, with car identity and timestamp. InSim supplies race length, lap/node progress, lap completion, pit events, and finish notifications. RaceLaps encodes ordinary laps (1–99), longer races (100–190), hours (191–238), practice (0), and no timing (255). InSim fuel fields depend on /showfuel yes and otherwise use 255; the primary implementation will use OutGauge instead.

This is a feasibility conclusion from the documented fields and current code, not a live measurement of accuracy. No host fuel-sharing change should be necessary for the player's own OutGauge stream. Single-player follow mode can use the watched car; multiplayer remains restricted to the user's own car and the existing internal-view checks.

The [CAR_info export specification](https://www.lfs.net/programmer/carinfo) includes fuel litres in a garage export. It is not a live tank-capacity channel and its wording does not establish that the value is maximum capacity. Do not assume it is capacity or build an importer for the first version. The current gadget displays percentage only.

## Design and meaning

The SVG was rendered and visually inspected. It contains:

- Header: LAPS UNTIL EMPTY, a large range value, and a signed finish margin at the right.
- A green horizontal separator.
- USAGE / LAPS / REFUEL columns, with AVG, MAX, MIN rows and a darker AVG highlight.
- A fuel-pump badge and current tank percentage on the right; the original litres row was removed at the user’s request.

Match these positions and proportions using the existing fonts and egui painter. Keep the header transparent as drawn. Replace the body's SVG 70% opacity with the speed dashboard's actual background: RGB (37, 37, 37), alpha 102/255 (40%). Reuse that color in a shared constant so the gadgets stay consistent. Retain the darker AVG highlight, green separator and badge. Show a negative margin in red, positive margin with an explicit + sign, and zero neutrally. The user subsequently supplied fuel_safe.svg, fuel_warning.svg and fuel_danger.svg: choose green above 2 AVG fuel laps, yellow above 1 through 2 laps, and red at 1 lap or less. Use a neutral grey badge before calibration or with stale data; zero fuel is red immediately. A race shortfall is separate from this range warning.

The SVG's illustrative numbers are independent placeholders, not an arithmetic fixture. The implemented values must come from consistent calculations. The user's example is the acceptance fixture: 12.9 fuel laps minus 20.6 required laps equals −7.7 laps of shortfall. It does not mean 7.7 litres or 7.7%.

Agreed table definitions:

| Row/field | Meaning |
| --- | --- |
| USAGE | Tank percentage points consumed per usable complete lap. |
| AVG | Mean consumption over up to the five most recent usable laps. |
| MAX | Highest consumption among those laps; produces the shortest range. |
| MIN | Lowest positive consumption among those laps; produces the longest range. |
| LAPS | Current fuel divided by that row's consumption. |
| REFUEL | Additional tank percentage needed to complete the remaining race using that row's consumption. Confirmed by the user. |
| Header range and margin | Use the AVG consumption estimate. |

Use one decimal for usage, range and margin. Round an additional-fuel recommendation upward to a whole tank percentage, so display rounding does not understate it. If more fuel is required than the available tank space, flag that another stop is needed; do not silently clamp the total deficit into a promise that one stop suffices.

## Calculations

Keep internal values as unrounded tank fractions.

- consumption per lap = start fuel − end fuel, for a complete usable lap.
- range in laps = current fuel / consumption per lap.
- remaining race laps ≈ configured total laps − completed laps − current lap fraction, clamped at zero.
- finish margin = AVG range − remaining race laps.
- additional fuel fraction = max(0, remaining race laps × row consumption − current fuel).

Anchor completed laps to IS_LAP.LapsDone, with a validated MCI lap counter for mid-race connections. Estimate the fractional lap from path position relative to the finish node. Node spacing and the timing-line transition make this approximate; verify the starting-grid convention and crossing order in live LFS before fixing any offset. Handle LAP arriving before or after the matching MCI update once, without a one-lap jump.

Initially estimate enough to cover the selected driver's full configured lap distance. This is conservative for a lapped driver whose race may end earlier after the leader finishes; explain that limitation in the gadget status/help. IS_FIN for the selected driver ends the finish requirement immediately. A later leader-based prediction needs explicit finish-rule validation.

Zero fuel is a valid reading. With a known positive burn rate it means zero range. Missing data, zero/undetectable burn, and lack of completed laps mean an unavailable estimate, not infinity or a fabricated zero.

## Data flow and smallest implementation

At research time, the decoder validated but did not retain the OutGauge fuel float. The implementation now retains it and reuses DashboardTelemetry's identity and timestamp checks. Runtime opens/routes OutGauge when either Speed dashboard or Fuel is enabled. The following steps describe the implemented approach.

1. Extend src/lfs/outgauge.rs Sample with fuel fraction and retain offset 28. Validate the documented range before using it; invalid fuel must not fabricate a level. Preserve packet-size, ID, finite-value, identity and timestamp checks. Add the low-fuel warning mask if using the badge warning.
2. Extend src/lfs/insim.rs Lap with LapsDone. Add finish and the pit events needed to discard pit-contaminated consumption laps. Today ISP_PIT is represented as InvalidLap; preserve delta invalidation when giving it a richer representation. Keep other gadget callers compatible.
3. Add src/fuel.rs with a small FuelEngine and FuelFrame. Store only the selected car's current lap boundaries and a bounded five-lap history. Feed accepted fuel readings and complete assembled MCI sets into it. Use existing Engine selection/lifecycle decisions, rather than implementing another player selector or MCI assembler.
4. Keep fuel tracking in src/radar/mod.rs Engine, beside gaps and delta, where assembled race progress and lifecycle events already meet. Add a method for accepted OutGauge fuel updates and a fresh FuelFrame accessor. Fuel availability must depend on fresh matching OutGauge and relevant InSim progress, without inheriting radar's requirement for an OutSim pose when it is unnecessary.
5. In src/runtime.rs, enable the existing gauge receiver when speed_dashboard.enabled OR fuel.enabled. Reuse one receiver and DashboardTelemetry's acceptance checks; return an acceptance result if necessary so rejected/duplicate samples cannot reach fuel history. Publish Snapshot.fuel. Update both separate-port and shared-port routing and diagnostics. Socket-collision validation must apply when either gadget needs OutGauge.
6. Add fuel: GapSettings to src/config.rs, default disabled; include it in position initialization and validation. Fuel percentage/range must work without a car profile or RPM limit. Legacy fuel_tank_litres settings are accepted when loading old profiles and dropped on save; profiles used only for capacity are retired.
7. Add src/overlay/fuel_style.rs and register it in src/overlay/mod.rs. Add Fuel to the gadget registry in src/overlay/desktop.rs, expand the independent overlay collection, and wire preview, enable/reconnect, position, scale, editing, close feedback and visibility. Reuse GapWindow and existing deferred viewport rendering. No separate process, new socket, or new dependency.
8. Update src/demo.rs with a coherent synthetic shortage/surplus scenario. Update the example configuration and usage/configuration docs when the implementation lands. Correct obsolete roadmap statements that OutGauge is not implemented, limited to the touched telemetry scope.

## Estimate lifecycle

- Before calibration: show live percentage, with — for usage, range and margin and a learning status. Learn from two finish-boundary fuel readings spanning a full usable lap; the partial lap on connection is not a full-lap sample.
- Each completed usable lap updates the five-lap history. AVG, MAX and MIN use the same history. Race pace changes naturally age out older samples.
- Exclude laps with a pit stop/refuel, reset/teleport, missing or stale boundary samples, a discontinuous lap counter, reversed/unsupported progress, or an identity change. A timing penalty alone does not erase physically valid fuel consumption. Do not copy delta's clean-lap requirement blindly.
- Refuelling increases the live level/range immediately. Retain completed consumption history for the same car/session; discard the current incomplete lap and re-anchor it. Detect pit events even if a refuel happens between received UDP samples. Ignore tiny float noise rather than treating it as a stop.
- On stale telemetry, withhold affected live values and invalidate a partial learning lap. Retain completed history only for the same surviving identity/session.
- Pause or an unsupported camera suspends output and discards a partial lap; resuming the same surviving car can retain completed history.
- Genuine car re-entry, driver switch/ownership change, session/race restart, track/layout change, or disconnect clears calibration. Requested roster/track snapshots for unchanged data do not clear it. Opponent events do not disturb the selected car.
- After selected-car finish, show a finished state and no continuing negative race requirement.

## Initial support and follow-up

First implementation: standard circuits with standard lap timing; lap-count race finish estimates including the longer RaceLaps encoding; live fuel level and learned range in practice/qualifying; existing own-car and live single-player follow behavior.

Withhold finish margin/refuel for practice, qualifying, unknown length, timed races, and unsupported/custom timing. Show the reason in the control card/status while keeping independently available fuel level. Timed races are feasible as follow-up using race elapsed time, a recent lap-time estimate and validated leader/final-lap handling. InSim race-clock units differ across protocol versions; the current application requests version 9, so do not assume current version-10 millisecond units.

No automatic /showfuel changes, automatic refuel control, stored cross-session burn profiles, tank-capacity downloads, binary car-info importer, or extra telemetry sockets in the first implementation.

## Validation and acceptance

Automated checks should extend existing packet/config/runtime tests and add one focused fuel suite:

- Fuel float offset/range, malformed packets, wrong IDs, zero fuel, ordering and clock wrap.
- Exact unrounded fixture with AVG range 12.9, requirement 20.6 and displayed margin −7.7; positive and zero margins; MAX usage implies fewer laps than MIN.
- Calibration waits for a complete usable lap; bounded history; zero burn does not divide by zero.
- Fuel-only enablement opens/routes OutGauge; both gadgets use one receiver; disabling both removes the requirement; shared-port ambiguity is rejected.
- Refuel updates range without corrupting burn; pit events, resets, stale data, driver changes, requested roster snapshots and opponent events have the intended history effects.
- Long lap-count encoding, pre-start/mid-race progress, LAP/MCI ordering, finish notifications, and unsupported/timed sessions.
- Old TOML loads unchanged; Fuel settings round-trip; retired capacity entries are removed on save.
- Painter checks for 534 × 224 proportions, dashboard background alpha, dynamic signs/placeholders, and control-card/viewport layout at supported scale and monitor sizes.

Before a code PR run the repository's cargo fmt --check, cargo check --locked, cargo clippy --all-targets --locked -- -D warnings, cargo test --locked and cargo test --no-default-features --locked. Render the demo for visual comparison, then validate live LFS driving, single-player AI follow, own-car multiplayer, start/finish crossings, a pit refuel and reconnect. Synthetic tests establish calculations and lifecycle behavior; live checks establish fuel precision and the lap-counter conventions.

## Confirmed product decision

The user confirmed REFUEL means additional fuel needed to finish, shown as tank percentage for each consumption row, and subsequently authorized implementation. No version bump, commit, push or PR was requested.
