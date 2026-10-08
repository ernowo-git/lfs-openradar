# Real-time follow viewed car plan

Status: implemented; automated verification complete, live LFS acceptance pending.
Branch: `feat/follow-viewed-car`, based on fetched `origin/main`
at `6d18184`. Scope agreed on 7 October 2026: live follow-view only, with no
replay support.

## Outcome and scope

Add a saved **Follow viewed car** option so a user can watch an AI car in a
single-player race and debug OpenRadar while LFS drives. Radar, race-order
gaps, and performance delta all use the same viewed car. Following also works
when viewing the human car in a live single-player session.

The option defaults to off. Existing own-car behavior remains the default.
In multiplayer, continue using the existing own-human-car rules even when the
option is enabled; following someone else's car is unavailable because the
required OutSim stream belongs to the user's car.

Exclude replay playback, seeking, playback-speed handling, driving controls,
automatic AI creation, automatic camera changes, and per-driver reference caches.
Cockpit and custom view remain required. No MCI-only fallback is added.

The [official InSim specification](https://www.lfs.net/programmer/insim)
documents the viewed-player ID in `IS_STA`, the state flags, and OutSim output
for the viewed car in single player. Multiplayer OutSim belongs to the user's
car. Follow-view therefore needs no new telemetry source or packet format.

## User flow

1. Start a live single-player race with several AI drivers on a standard circuit.
2. Select an AI with Tab and use cockpit or custom view.
3. Enable **Follow viewed car** in OpenRadar and click **Apply / reconnect**.
   Use **Save settings** to persist it, following the existing settings behavior.
4. The panel identifies the followed driver. Radar and gaps become available
   once fresh, matching InSim and OutSim samples arrive.
5. Stay on that driver for a full clean recorded lap to establish its delta
   reference. Changing to another driver starts a new reference.
6. Disable **Hide overlay when LFS is in background** when inspecting the overlay
   while working in another app. This remains an independent preference.

## Implementation sequence

### 1. Configuration and selection

- Add top-level `follow_viewed_car: bool` to `Config`, defaulting to `false`.
  Older TOML files continue loading through existing default handling.
- Initialize the telemetry `Engine` with this policy from `Runtime`. Retain
  `Engine::default()` as own-car mode for existing callers and fixtures. Preserve
  the configured policy when `Engine::clear()` resets telemetry after reconnects
  and session changes.
- Refactor `Engine::select_driver()` into shared view/state checks and two
  selection paths. Replace relevant numeric state masks with named constants
  so multiplayer (`512`) and front end (`256`) are not confused.
- In follow mode during single player, resolve `State.viewed` directly through
  the player roster, allowing AI and human entries outside the garage. Do not
  require exactly one local human. Missing/zero viewed ID yields no target.
- Require an active game and camera 3 or 4. Paused, SPR, free view, and front-end
  states yield no live output. Do not add any replay eligibility path.
- In own-car mode, and in multiplayer regardless of the option, keep the
  existing local-human selection and association rules.

### 2. Target identity and history boundaries

The current delta key is `(UCID, car model)`. Two AI drivers can share both,
so simply relaxing the human filter would incorrectly share reference laps.

- Use a car identity including PLID, UCID, and model for delta selection.
  Explicit lifecycle resets handle re-entry or PLID reuse with identical fields.
- On an actual target change, clear the delta reference and partial lap, gap
  histories, OutSim association and clock, pose interpolation, and threat state.
  Require fresh matching telemetry before rendering the new target.
- Preserve a best reference across a temporary pause or unsupported camera
  only when resuming the same surviving car. Discard the incomplete lap. Track
  the last target identity independently of temporary selection eligibility,
  so switching cars while paused cannot reuse a reference on resume.
- Clear the reference on race/session restart, track or layout change,
  disconnect, ownership/model replacement, leave, or genuine re-entry.
  A car reset discards the current lap and telemetry history; its completed
  clean reference may remain if the car identity and session are unchanged.
- Distinguish requested roster snapshots (`IS_NPL.ReqI != 0`) from genuine
  join/re-entry packets. An unchanged roster refresh must not invalidate the
  followed driver's reference; changed ownership/model/driver kind must.
- Clear the OutSim-to-MCI association whenever its supporting histories are
  cleared. Keep distance, heading, ordering, and freshness checks for all targets.
  OutSim has no car ID, so association remains an approximate spatial check;
  switching waits for fresh matching data and does not guarantee packet-level
  identity when cars overlap.
- Only the selected target's lap reports and invalidations affect its delta.
  An unrelated AI's pit, reset, leave, or roster update must not discard the
  followed driver's lap/reference. Check the existing broad history resets and
  narrow them where needed for this behavior.

### 3. UI and documentation

- Add **Follow viewed car** near the live telemetry controls in the existing
  control panel. Help text explains single-player AI use, required cameras,
  **Apply / reconnect**, and unchanged multiplayer own-car behavior.
- Show the active target name and a clear follow-mode status. Distinguish
  waiting for roster/telemetry, unsupported camera, pause, and missing target.
  Avoid own-human-only instructions when follow mode is active in single player.
- Add the default and comments to `openradar.example.toml`. Update README,
  configuration, usage, and troubleshooting guides with the AI debugging flow
  and background-visibility preference. Do not change LFS OutSim settings:
  `OutSim Mode 1` already fits live-only use.

### 4. Automated verification

Extend existing behavioral tests rather than duplicate the calculations:

- Old config defaults off; the new option round-trips through save/load.
- A viewed AI is rejected by default and accepted in enabled single-player mode,
  including an AI-only race and a race with several local AI drivers.
- Selected AI supplies radar origin, race-order gap reference, and delta events.
- Two AI cars with the same UCID/model have separate reference lifetimes.
  Switch A to B and back: neither partial laps nor completed references leak.
- Switching while paused or in free view also clears the previous car's
  reference on resume. Returning to the same car retains only its clean reference.
- Missing target, garage, pause, replay, front end, unsupported cameras, stale
  telemetry, and incorrect OutSim pose withhold output as appropriate.
- Target pit/leave/re-entry/reused PLID, ownership changes, race restart,
  track changes, reconnect, and unrelated-AI events follow the reset rules.
- Unchanged requested roster snapshots preserve the reference; actual
  replacements reset it. Engine clearing preserves the configured follow policy.
- Multiplayer still requires the user's own human car; viewing a remote car
  never enables this follow path. Demo and default own-car tests still pass.
- Cover runtime policy wiring through existing socket-worker tests, and verify
  the control is visible and saves the setting through existing UI/config tests.

Run the repository Rust checks after implementation: `cargo fmt --check`,
`cargo check --locked`, `cargo clippy --all-targets --locked -- -D warnings`,
`cargo test --locked`, and `cargo test --no-default-features --locked`.

### 5. Live acceptance

On Windows, run an AI-only race on a standard circuit with at least three AI:

1. Follow AI A in cockpit view and confirm both telemetry ages update, nearby
   cars appear relative to A, and gaps identify A's race-order neighbors.
2. Stay on A through sufficient clean laps for a reference and live delta.
3. Switch to AI B of the same model. Old values disappear during synchronization;
   B establishes its own reference. Switch back and confirm a fresh reference
   is required rather than restoring B's or A's old reference.
4. Exercise pause/resume, camera changes, AI pit/reset, race restart, and reconnect.
   Confirm temporary interruptions and lifecycle changes follow the rules above.
5. Disable the option and verify normal own-car driving still works. Verify
   multiplayer own-car behavior if a multiplayer session is available.
6. Turn background hiding off, focus the debugger, and confirm the live AI race
   and overlay continue updating. If LFS pauses on focus loss, adjust the LFS
   setting during the acceptance run and document the requirement.

Record which live checks ran and any remaining validation limits. Passing
synthetic tests alone does not establish live AI telemetry correctness.

## Effort and completion

Estimate: 2-4 hours for a basic prototype; 1-2 working days for implementation,
identity/lifecycle tests, documentation, and Windows live acceptance. The main
uncertainty is target switching and lap-reference validity, rather than rendering.
No replay work is included.

Done means the option is saved and applied, all three gadgets follow the same
live target, references never cross targets, default behavior remains compatible,
the Rust checks pass, and live acceptance results are recorded.

## Implementation record

Implemented on 7 October 2026: saved configuration and live control-panel option,
single-player viewed-car selection, shared gadget target, PLID-based delta
identity, target-specific lifecycle resets, roster snapshot handling, and updated
usage/configuration/troubleshooting documentation. Normal own-car mode retains
its reference when briefly viewing another car and returning to the same car.

The branch was initially created from stale local `main` at `ed86d4d`.
It was corrected to the freshly fetched remote main at `6d18184`, preserving
the feature changes and integrating the current Settings UI, overlay shortcut,
estimated-lap display, roster handling, and telemetry continuity fixes.

Automated coverage includes AI-only races, matching radar/gap/delta targets,
identical AI models with distinct lap references, switching while paused,
temporary camera changes, opponent events, replacement/reconnect, unsupported
views, wrong/stale OutSim, multiplayer fallback, persisted configuration,
checkbox interaction, and TCP/UDP worker wiring.

Verification passed: `cargo fmt --check`, `cargo check --locked`,
`cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`
(104 tests), and `cargo test --no-default-features --locked` (80 tests).
The three-second headless demo smoke check passed with no rejected or malformed
telemetry. `cargo build --locked` produced the Windows debug executable.

Windows live acceptance remains pending: LFS is installed at `C:\Games\LFS`
but was not running during implementation, and native UI control is unavailable
in this session. No live AI-race, focus-loss, or visual overlay results are claimed.

Version files are unchanged. If a pull request is later created or opened, ask
the repository's four-choice version bump question and wait for the answer
before changing version files.
