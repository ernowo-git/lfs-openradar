# Lap Records: follow Performance Delta lap validity

Status: proposed; planning only, no runtime changes.
Branch: `plan/lap-records-validity`.
Requested: 20 October 2026.

## Outcome

Only complete, valid laps may improve the local human driver's PB lap or best
sectors. Use Performance Delta's existing invalidation and final-penalty rules.
An invalid lap must leave both record rows and persisted values unchanged, even
if it contains a faster individual sector. The next complete valid lap can
improve records normally.

This request supersedes the earlier decision to ignore invalid-lap flags in
[the original plan](lap-records-gadget-research.md) and the current glossary.
The existing overlay and configuration ADRs do not conflict with this change.

## Findings from current source

- [Recorder](../src/lap_records.rs): already checks complete lap boundaries,
  ordered checkpoints, elapsed times, identity and standard track timing. It
  deliberately ignores `InvalidLap`, `PitStop` and packet penalties.
- [Performance Delta](../src/delta.rs): `invalidate()` disqualifies the current
  and pending completed trace; `confirm_reference()` requires a valid trace,
  positive official lap time and final `penalty == 0`.
- [Engine](../src/radar/mod.rs): sends every packet to the recorder before its
  main dispatch. Delta receives `InvalidLap` and `PitStop` only for the selected
  driver; pit-lane entry alone does not invalidate its lap.
- [Decoder](../src/lfs/insim.rs): already produces `InvalidLap` for HLV incidents
  and nonzero penalty notifications. No new packet type or subscription is
  needed for those existing signals.
- [Recorder tests](../tests/lap_records.rs): explicitly expect flagged laps to
  improve best sectors. Helpers and the TCP fixture use final penalty `2`,
  which Delta would reject. Update clean fixtures to `0` during implementation.
- The recorder also feeds the [Lap-time log](../src/lap_log.rs). Record eligibility
  must be independent of timing coherence so rejected PB candidates still appear
  in the session log under its existing rules.

These are source findings; no live LFS reproduction or fix validation has run
on this planning branch.

## Proposed behavior

| Event or condition | Lap Records behavior |
| --- | --- |
| Local driver's `InvalidLap` or `PitStop` during a candidate lap | Mark that candidate invalid until its finish boundary |
| Final local `Lap` has nonzero penalty | Reject the candidate, matching Delta's `penalty == 0` check |
| Penalty subsequently cleared before finish | Earlier invalidation remains sticky for this candidate |
| Opponent/AI invalidation, penalty or pit event | Does not affect the local human's records |
| Pit-lane entry without a pit stop or invalidation | Does not alone disqualify the lap, matching Delta |
| Complete valid lap | Update PB only if strictly faster; update sector minima from this lap |
| Invalid lap with a faster sector | Update neither PB nor any best sector |
| Invalid finish followed by a clean lap | Advance the boundary, clear candidate validity, accept the next complete clean lap |
| Duplicate finish packet | Ignore it without clearing the new candidate's invalidation |
| Reset, garage, re-entry, disconnect, track/car/session/layout change | Retain existing timing reset rules and clear candidate validity with that context |

Parity here means lap eligibility from shared InSim events. Keep records working
without OutSim and independently of camera selection, gadget visibility and
Follow viewed car. Do not require Delta to establish a sampled reference before
saving scalar lap records. Delta's trace sampling and telemetry continuity
checks remain specific to its continuous comparison.

## Smallest implementation

1. Add a candidate invalidation flag to `Recorder`, separate from `broken`
   timing. Handle matching-driver `InvalidLap` and `PitStop` packets directly
   in the recorder, using its existing local-human identity rather than
   `Engine.selected`, which can follow another car or disappear with the camera.
2. Gate only the PB/best-sector update block on that flag and final
   `penalty == 0`. Continue processing the log and finish boundary even when
   the record candidate is rejected. Clear invalidation on a genuine next-lap
   boundary and existing full resets; preserve it across duplicate events.
3. Update recorder status and desktop help to describe valid-lap recording.
   Update `GLOSSARY.md`, the Lap Records usage section and the original plan's
   status with a supersession link. Remove the Lap-time log text saying its
   permissive behavior is the same as Lap Records; retain its actual behavior.
4. Extend existing recorder and TCP persistence tests. Keep configuration
   storage, renderers, Delta behavior and dependencies unchanged.

Do not reset the whole timing sequence on an invalidation: that would also
discard log entries and unnecessarily delay recovery. Do not connect record
acceptance to Delta's selected-driver state or session-only reference lifetime.

## Existing saved records

Saved records contain no validity provenance or underlying lap history. Existing
PBs and sector minima cannot be reliably classified or rebuilt. A previously
invalid fast PB can also prevent a slower clean lap from replacing it.

Default proposal: preserve saved records; enforce the new policy on future
candidates. For known affected track/car entries, back up the configuration and
clear the entire entry before collecting clean records, because sector minima
may also be contaminated. Decide whether that targeted reset is wanted before
implementation; do not automatically delete all historical records or claim
preserved values have been validated.

## Verification after implementation

Add failing assertions before changing recorder behavior, using the real
`Engine::packet` path in `tests/lap_records.rs`:

- Establish a clean PB, then send a faster invalid lap; assert PB, every sector
  and `take_changes()` remain unchanged. Repeat with no existing record.
- Cover `InvalidLap`, `PitStop` and a nonzero final LAP penalty independently;
  include an invalidation after the last split and a penalty-clear notification
  after an earlier invalidation. Include a slower invalid lap with a faster sector.
- Verify the next valid complete lap records normally, invalidation does not
  leak through resets, opponents do not invalidate the local driver, camera
  changes do not bypass validity, and pit-lane entry alone remains eligible.
- Verify duplicate LAP packets cannot clear invalidation for the next lap;
  preserve existing missing/conflicting checkpoint and partial-lap rejection.
- Verify invalid but coherent laps still enter the Lap-time log, with its
  existing session-best and previous-lap comparison behavior.
- Extend the TCP fixture to prove rejected candidates leave saved records
  unchanged and a subsequent clean improvement survives runtime replacement.

Run focused recorder, Delta and log suites, then repository checks:

```sh
cargo test --locked --no-default-features --test lap_records --test delta --test lap_log
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --no-default-features --locked
```

Live acceptance: on the same track/car, establish a clean record, finish a
faster invalid lap, verify both rows remain unchanged while the log receives
the lap, then complete a faster valid lap and verify the improvement persists
after restart. Compare the invalidation with Performance Delta in the same
session. Automated packet tests do not establish live incident delivery or
visual/runtime acceptance.
