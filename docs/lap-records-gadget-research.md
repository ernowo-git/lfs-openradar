# Lap records gadget research and implementation plan

Researched and implemented 15 October 2026. Recorder, automatic persistence and both overlay backends are implemented. Automated desktop/headless checks and a real KDE Wayland GPU demo passed. Live LFS timing comparison and Windows runtime acceptance remain pending.

Tracked in [RAC-3](https://plane.ctrlchsr.space/sampingan/projects/4f36a711-f98d-48a3-9227-5d674915f6d4/issues/d3f02f43-e31e-4b21-93c2-31eb59b4d53a), In Progress, labeled `ready-for-agent`.

Implementation: [recorder](../src/lap_records.rs), [configuration writer](../src/config.rs), [shared painter](../src/overlay/lap_records_style.rs), and [recorder/persistence/TCP checks](../tests/lap_records.rs). Recording runs independently of gadget visibility. The sections below retain the research-time findings and implementation rationale; validity detection was explicitly removed from scope.

## Outcome and agreed behavior

The existing InSim connection provides the events needed for this feature. Add a Lap records gadget and persist records in the existing TOML configuration, grouped by **track code and car** (for example BL1 / XFG). The user confirmed this key and that independent best sectors count **only after a complete lap**. Subsequent scope decision: **ignore invalid-lap status**; HLV incidents and penalties do not disqualify timing records. Complete, coherent timing data is still required.

Maintain two distinct sets:

- **PB lap:** the fastest accepted complete lap, retaining its own cumulative splits together: S1, S1+S2, lap time on a three-sector track. Replace this entire set only when a strictly faster complete lap is accepted.
- **Best sectors:** the minimum individual S1, S2 and S3 across accepted complete laps. These can come from different laps. The BEST row displays THEORETICAL: the sum of the independent best sectors. The PB row retains the recorded PB lap time. Calculate the theoretical total from stored sectors without persisting a second total.

This is a records feature, not an archive of every completed lap. The proposed configuration retains a bounded set of records for each encountered track/car combination. Display missing values as unavailable, rather than zero. Timing display should use minutes, seconds and milliseconds; storage uses integer milliseconds.

## Primary-source protocol findings

The [official InSim specification](https://www.lfs.net/programmer/insim) defines `IS_SPX.STime` and `IS_LAP.LTime` in milliseconds, with player IDs; splits are numbered 1–3. Both packets include total elapsed time and current penalty. `IS_RST` supplies track code, timing mode and checkpoint count (`Timing & 3`). Zero through three checkpoints imply one through four sectors. Its high timing bits distinguish standard, custom and absent timing. `IS_NPL` supplies car identity and local/remote/AI type.

The current v9 initialization can remain for this feature. Invalid-lap detection, HLV delivery and protocol upgrades are outside scope by explicit user decision. Records are local timing bests and do not imply clean-lap or LFSWorld eligibility. [Current decoder](../src/lfs/insim.rs)

**Interpretation to validate:** the specification calls `STime` “split time” without explicitly stating cumulative versus individual duration. The planned cumulative interpretation must be checked against a live three-sector lap and the in-game split display before implementation is accepted. Do not substitute packet arrival timestamps for measured lap times.

## Current repository integration points

These findings come from current source inspection:

| Area | Existing behavior | Proposed change |
| --- | --- | --- |
| [InSim decoder](../src/lfs/insim.rs) | Already decodes `Packet::Lap`, `Packet::Split`, track metadata and invalidation events. Drops both events' elapsed time and RST Split2/Split3. Init requests v9 despite requesting HLV. | Preserve elapsed time for boundary/deduplication checks. Add a checkpoint-count accessor; retain additional nodes only if needed. Keep current protocol compatibility; do not add validity gating. |
| [Engine](../src/radar/mod.rs) | Routes LAP to delta/fuel; SPLIT reaches the catch-all. Existing driver selection depends on camera and can follow another car in single player. | Feed a separate personal recorder from authoritative events and local-human identity. Reuse roster/lifecycle data, but do not record an AI/viewed opponent as the user's PB. |
| [Performance Delta](../src/delta.rs) | Maintains a session reference with sampled progress/pose information. | Keep its current meaning. Scalar saved splits cannot reconstruct its continuous reference trace. |
| [Configuration](../src/config.rs) | Defaults missing top-level fields, rejects unknown fields and serializes the complete config with `fs::write`. | Add default-empty records plus separate gadget settings. Introduce coordinated, safe writes before automatic persistence. |
| [Runtime](../src/runtime.rs) | Owns telemetry worker and publishes shared snapshots; reconnect clears Engine history. No config path is passed to it. | Publish a lap-record frame and preserve committed records across reconnect/runtime replacement. Pass an application-owned records/persistence handle. |
| [Desktop panel](../src/overlay/desktop.rs) | Holds editable config and path; explicitly saves settings. Apply/reconnect replaces Runtime. | Share the authoritative record store, so reconnect and Save settings retain newly earned records. |
| [Headless entry point](../src/main.rs) | Already knows the selected config path. | Use the same record loading/writing behavior as desktop. |
| [Overlay session](../src/overlay/session.rs) | Registers gadget settings, geometry, enablement and shared painting. | Add Lap records throughout desktop and native Wayland registration. |

## Timing model and acceptance rules

Suggested module: `src/lap_records.rs`, with a small recorder, serializable record types and a read-only gadget frame. Its inputs are identity, track/session metadata, split/lap events and lifecycle events. It does not require OutGauge, OutSim pose samples or interpolated MCI timing.

For a verified cumulative three-sector feed, let `c1` and `c2` be the checkpoint times and `L` the lap time:

```text
S1 = c1
S2 = c2 - c1
S3 = L - c2
require 0 < c1 < c2 < L
PB checkpoints = [c1, c2]; PB lap = L
```

Example: lap A has checkpoints 30,000 / 65,000 ms and finishes in 100,000 ms, giving sectors 30,000 / 35,000 / 35,000. Lap B has 29,000 / 66,000 and finishes in 101,000, giving 29,000 / 37,000 / 35,000. PB stays entirely from A; independent best sectors become 29,000 / 35,000 / 35,000. Their sum, 99,000 ms, is the theoretical best shown in the BEST row; the recorded PB remains 100,000 ms.

Proposed conservative policy:

1. Record only one unambiguous local human, with known car and matching current track/session metadata. Reject replay input. Camera changes should not change record ownership; display hiding is separate from recording.
2. After startup, reconnect, car/driver replacement, reset, session restart or unknown continuity, discard pending splits and wait for an authoritative lap boundary before starting a candidate. A mid-lap connection must not produce a partial PB. Existing committed records survive all these events.
3. Collect the expected checkpoints exactly once in order. Reject unexpected, conflicting, missing or non-increasing values. Use elapsed time and completed-lap count to reject duplicate/stale events; SPX has no lap counter, so maintain its association within the current timing sequence.
4. Ignore HLV/invalid-lap events and penalties when deciding whether to accept a completed lap. Pit-lane entry alone does not disqualify a lap. Reset, garage re-entry, identity/layout changes and interrupted timing continuity discard the candidate because complete-lap association is lost.
5. At LAP, require complete checkpoint coverage, valid ordering and intact event continuity. Derive sectors, compare records and commit PB plus improved sectors in one update. No sectors are persisted provisionally at a checkpoint. Ties retain the existing PB.
6. Clear the candidate for the next lap regardless of whether the completed lap was accepted.

Use checkpoint count rather than hard-coded three-sector assumptions: zero checkpoints means lap only; one means two sectors; two gives the requested S1/S2/S3; three needs S4 and another cumulative checkpoint. Recommended UI adapts its columns to actual count. Do not label the third checkpoint as the finish on a four-sector track.

Initial scope should accept standard timing only. Custom layouts can share a base track code while changing the course; retain existing records but suspend new recording on custom/no timing or a changed layout until a sufficiently strong layout identity is implemented. Preserve the full received track code, including reverse/config suffixes. Built-in car codes and existing hexadecimal mod identifiers can reuse `Player.model`; never use transient PLID as a persisted key. Records belong to this local installation; multi-account profiles and mod-revision comparison are deferred.

## Proposed TOML representation

The outer key is always the track code, with car nested underneath. Names below are proposed, not currently accepted configuration fields:

```toml
[lap_records]
enabled = true
scale = 1.0

[track_records.BL1.XFG]
checkpoint_count = 2
best_sectors_ms = [29000, 35000, 35000]

[track_records.BL1.XFG.pb]
checkpoints_ms = [30000, 65000]
lap_ms = 100000
```

Store checkpoint count with each set and validate array lengths, positive durations, increasing PB checkpoints, final checkpoint below lap time and sector minima no greater than the PB's corresponding sectors. Derive the PB lap from `pb.lap_ms` and the theoretical best from the sum of `best_sectors_ms`. An empty map loads old configs without migration. Keep records separate from existing RPM profiles: `Config::save` currently removes capacity-only entries from `cars`, which must not prune lap data. [Current configuration code](../src/config.rs)

## Persistence ownership

**Automatic persistence is required when records improve.** Requiring Save settings would lose an earned PB on exit. Conversely, automatically serializing the panel's current config would silently save unsaved HUD placement, contrary to [ADR 0002](adr/0002-separate-wayland-hud-layout.md).

Use one application-owned config/record store shared by the telemetry producer and settings controller. It retains the last persisted settings baseline, current records and dirty/retry state. Serialize every automatic record write and explicit settings save through this owner:

- On record improvement, merge records into persisted settings, leaving current unsaved panel edits in memory.
- On explicit Save settings, merge the latest records into the chosen panel settings, then advance the persisted baseline only after a successful write.
- Give Runtime restarts the same store. Reconnect must not seed from an older cloned config. Native rendering only reads snapshots and never writes records.
- Write a complete sibling temporary file and replace the destination safely; verify replacement behavior on both Windows and Linux. Keep the prior valid file on failure. Validate before writing; avoid concurrent writers inside the process. Separate simultaneous app instances need a lock or an explicit unsupported-state error.
- Retain improvements in memory after a write failure, show a concise persistence error and retry without waiting for another PB. Log errors beginning with lowercase, for example `cannot save lap records: ...`. Do not echo configuration contents or credentials.
- Use the exact path selected by startup/`--config`. If it does not exist yet, the baseline is the application's default saved settings. Keep environment/connection credential overrides out of automatic writes. Demo mode never writes records.

The current plain `Config::save` cannot safely coexist with an independent telemetry file writer. Centralizing both paths is part of this feature, not optional follow-up work. File replacement alone does not prevent stale settings saves from erasing records.

## Staged implementation and validation

1. **Prove split semantics first.** Capture a three-sector lap on the installed LFS build, comparing checkpoints/LAP against the game. Verify cumulative checkpoint semantics before deriving sectors. Invalid-lap detection and protocol upgrades are not prerequisites.
2. **Build the recorder without UI.** Add deterministic packet-sequence tests for the example above, a sector improvement on a slower lap, ties, incomplete startup laps, acceptance despite HLV/penalty flags, reset/garage events, missing/duplicate checkpoints, reconnection and track/car switches. Include local human versus AI/opponent/replay, variable sector counts and custom/no timing.
3. **Add config persistence.** Exercise old-config loading, round trips, restart recovery, malformed record validation, saving settings after automatic updates, automatic updates with unsaved HUD edits, reconnect, write failure/retry and demo isolation. Use temporary config files; never overwrite the real user config during tests.
4. **Add the gadget.** Suggested presentation: track/car header, PB cumulative checkpoint row, best individual-sector row with a calculated theoretical best lap. Label cumulative `S1+S2` explicitly. Add a shared painter, demo data, enablement, desktop position/scale and separate Wayland placement. Preserve [ADR 0001](adr/0001-native-wayland-overlay-worker.md)'s single telemetry owner and [ADR 0003](adr/0003-automatic-overlay-backend-selection.md)'s backend preference behavior.
5. **Accept live behavior.** Drive faster/slower laps, including laps flagged invalid, restart OpenRadar, switch track and car, and verify persisted values against LFS. Check Windows and Linux/Wine, including native Wayland. Automated tests establish state transitions; they do not establish live packet semantics.

No new socket, external service or lap database is needed. The main implementation risks are associating events with complete laps and coordinating automatic records with explicit settings saves. The exact visual styling remains an implementation choice; record semantics and storage grouping above reflect the user's confirmed choices.
