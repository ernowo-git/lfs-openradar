# LFS proximity radar: research and implementation plan

Researched 6 October 2026. This historical plan records the research behind
OpenRadar. [The project README](../README.md) tracks the implemented setup and
verification limits. Workspace findings below refer to the original LFSLapper
checkout; those LFSLapper files are outside this standalone repository.

The [product roadmap](roadmap.md) extends this original radar plan after `v0.1`
with modular split/lap timing, gear, RPM, and gap-ahead/gap-behind gadgets. It
records the current delivery sequence and shared widget architecture. Modern
OutSim's optional drive block supplies gear and engine speed; OutGauge remains
optional for other dashboard fields.

## Recommendation and assumptions

Build a separate application running beside the driver's LFS client, with native Windows and Linux builds. The MVP requires both InSim/MCI and OutSim: MCI supplies the opponent field and roster, while OutSim supplies the local car's position and orientation for a smoother radar reference. Include a simple radar and configurable warning zones. OutGauge remains optional for future dashboard widgets. Rust is the user's preferred language. At the time of the initial research, the original workspace contained C# LFSLapper source and LPR configuration, with no Rust project yet.

An external overlay requires installation on each driver's computer. If the intended feature is available to every server participant without installing anything, choose a separate LFSLapper button-based design instead. That changes the rendering capabilities, update budget, and implementation scope.

The recommended application connects to the local LFS client, independently of the host's LFSLapper connection. Leave the existing timing add-on and executable untouched. Verify coexistence during the initial prototype.

## Verified interface facts

MCI carries multi-car positions, speed, motion direction, body heading, and PLID. Each packet holds at most 16 cars; FIRST/LAST flags delimit a set. Positions use 65536 units/metre, speed 32768 units/100 m/s, and heading 65536 units/turn, anticlockwise from +Y. Initial intervals support 10 ms increments. LOCAL concerns local operation/buttons; it does not identify your car. NCN/NPL provide connection/player mapping; STA reports the viewed PLID. OutSim is view-dependent and OutGauge supplies dashboard telemetry. Keepalive replies are required. The current published protocol is version 10. [Official InSim specification](https://www.lfs.net/programmer/insim)

Do not interpret a requested 50 Hz MCI stream as guaranteed fresh remote physics at 50 Hz. Measure actual motion, arrival spacing, and lag before selecting smoothing settings.

## Corrections to the pasted proposal

The coordinate transform needs different signs. Derived from the documented heading convention, with `h` in radians and offsets in metres:

```text
right   =  dx*cos(h) + dy*sin(h)
forward = -dx*sin(h) + dy*cos(h)
screenX = centreX + right * pixelsPerMetre
screenY = centreY - forward * pixelsPerMetre
```

At heading zero, +Y is ahead and +X is right. At heading 90 degrees, -X is ahead and +Y is right. Test all four cardinal headings and an arbitrary angle. Widen signed position integers before subtracting, then convert to floating point.

Use body heading for radar orientation and motion direction for velocity. Derive velocity as `vx = -speed*sin(direction)` and `vy = speed*cos(direction)`. Render opponent rotation using explicit transformed corner vectors, avoiding ambiguity about graphics-library rotation signs.

The suggested fixed overlap thresholds measure centre proximity, not physical overlap. Distinguish nearby, alongside, and potential contact. Approximate contact with oriented footprints and a configurable margin; do not label all side-by-side cars as overlapping.

Prediction should begin disabled. A fixed 150 ms lead can move symbols ahead of observed positions and worsen warnings during braking or spinning. Later extrapolate both cars to the same render time, cap the horizon, and disable extrapolation after resets or suspect updates.

## Workspace findings

- `src/InSim4.cs:1483` already defines and decodes CompCar fields.
- `src/InSim4.cs:1000` hardcodes a 10 ms interval; its `mciSeconds` parameter is misleading and does not set that interval in the inspected implementation.
- `src/LFSClient/managePacket.cs:1207` converts MCI coordinates into metres and headings into degrees, then rounds stored positions to two decimal places. An LPR implementation must not apply raw-packet conversions again.
- `doc/Player Vars.txt` exposes X/Y/Z, Heading, Direction, and OnTrack. Existing LFS buttons can support a simpler server-side display; they do not expose arbitrary rotated vehicle graphics.

These are source observations, not confirmation that the installed executable was built from this source.

## Proposed design

```text
Local LFS -> InSim receiver -> roster + complete MCI snapshots
          -> OutSim receiver -> local pose + telemetry time
                                      |
                            target/time alignment
                                      |
                               radar geometry
                                      |
                        latest render state -> overlay
```

Use a receiver thread and a bounded latest-state handoff. Rendering should never block network processing or consume a growing queue of old frames. Record receive times with a monotonic clock.

OutSim is a required MVP input. Receive its UDP telemetry separately from the InSim decoder and validate the configured packet layout. Use its local position and heading together as the radar reference; avoid mixing a newer heading with an unrelated older position. Verify heading conventions against MCI and interpolate angles across the wrap boundary using the shortest arc.

Pair OutSim with the selected local driver only when the active view and telemetry availability establish that association. Clear buffered poses on target, camera, session, track, or connection changes. Validate supported camera modes in the initial spike. If OutSim is unavailable, stale, or cannot be associated reliably, suspend the live radar and show an OutSim status; an MCI-only fallback does not satisfy the MVP requirement.

Keep short timestamped histories and align both sources to a common display time. MCI has no per-car physics timestamp, so use monotonic arrival times as an approximation and measure the residual mismatch. Handle OutSim telemetry time resets and reordered datagrams. Begin with interpolation rather than a fixed prediction lead. Measure orientation smoothness and end-to-end delay against an MCI-only diagnostic baseline; a higher packet rate alone is not evidence of improvement.

Start with TCP for simpler ordered roster and snapshot handling; evaluate UDP MCI only if measured backlog justifies it. Validate packet lengths and counts, stream fragmentation/coalescing, byte order, and version-specific size encoding against captures. Never assume a socket read is one packet.

Maintain connection and player maps. Select the local human driver explicitly, excluding local AI; do not assume UCID zero identifies the player. Default to driving mode. Offer a separate follow-view mode for spectating/replays, clearing history on target changes. If identification is ambiguous, hide warnings and show a setup status.

Assemble complete MCI sets before publishing. Retain the last complete set briefly during an incomplete update; clear on disconnect, player removal, session/track change, and target change. Reset interpolation on teleport, reset, or implausible jumps. Initial proposed stale limits: mark uncertain after 250 ms and hide after 500 ms, then tune using captures. These are design defaults, not LFS guarantees.

Use 12 m broad-phase range, a 5 m lateral / 8 m forward / 7 m rear display zone, and an adjustable height gate initially around 3 m. Expand broad-phase bounds for vehicle footprints. Test bridges, banking, slopes, and jumps before trusting the height gate. Unknown vehicle sizes use an explicitly approximate 1.8 m by 4.2 m footprint; add validated per-model and mod overrides later. Calibrate the reported position relative to the vehicle footprint.

Keep warning thresholds configurable and add hysteresis to prevent flicker. Lagging data must lower confidence rather than produce stronger predictions. Show stationary opponents; skip players in the garage. Audio warnings are deferred.

## Overlay choice and prototype gate

For Rust, prototype `winit` for the window and a small renderer. Winit exposes transparency, decorations, and window-level settings; verify renderer alpha support rather than assuming the window flag is sufficient. [Winit window attributes](https://docs.rs/winit/latest/winit/window/struct.WindowAttributes.html)

Test a native Windows layered-window implementation if needed. Microsoft documents per-pixel alpha and mouse pass-through for layered windows, plus a NOACTIVATE style. [Window features](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features), [Extended styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)

The gate is a transparent, input-pass-through rectangle over LFS that does not steal focus. Target windowed/borderless operation first; exclusive fullscreen remains unverified. Test Alt-Tab, multiple monitors, DPI changes, minimize/restore, and foreground detection. Hide the radar when LFS is not foreground. Provide a separate settings window for moving/resizing it and saving preferences.

## Delivery sequence and acceptance

1. **Connection and overlay spike:** confirm installed LFS version, local InSim and OutSim endpoints, supported camera modes, car selection with AI, and coexistence with LFSLapper and other telemetry consumers. Verify overlay alpha and input behavior. Capture both telemetry streams on Windows and Linux/Proton.
2. **MCI + OutSim radar MVP:** roster, complete MCI sets, OutSim local pose, target/time alignment, angle interpolation, corrected geometry, rotated approximate footprints, adjustable placement/range, and stale-data handling. Accept only after left/right/ahead/behind match physical cars while driving and turning, orientation is measurably smoother than the MCI-only diagnostic baseline, and missing or misassociated OutSim safely suspends the radar.
3. **Warning quality:** footprint distance/intersection, alongside detection, height filtering, hysteresis, and lag confidence. Accept using side-by-side, nose-to-tail, spinning, bridge, and unknown-mod scenarios.
4. **Smoothing refinement:** tune the MVP's dual-source interpolation and compare it with capped extrapolation using recorded traces. Select settings from measured latency, orientation jitter, and positional error.
5. **Packaging:** build a standalone executable, configuration defaults, setup/troubleshooting guide, and telemetry diagnostics with recording opt-in.

Automated checks should cover packet boundaries and malformed data; sets spanning 17 and 48 cars; local-human versus AI selection; lifecycle removal and PLID reuse; cardinal transforms; angle wrapping; footprint geometry; and stale/reset behavior. Include OutSim layout validation, datagram reordering, telemetry time resets, source alignment, camera/target changes, and missing OutSim suspension. Use fixtures and deterministic clocks.

Live acceptance must include multiplayer with AI, a grid exceeding 16 entrants, pit/spectate/rejoin, takeover, reconnect, pause/replay, packet stalls, car/track changes, and telemetry consumers already running. Verify the existing timing UI still works during coexistence testing. No live-server tests were run during this research task.

Before implementation, resolve the deployment choice (per-driver overlay or server-distributed button UI), installed LFS version, and display mode. The recommended default is a separate Rust client overlay, with MCI and OutSim required for the MVP, for windowed/borderless LFS.

## Rust stack and Linux/Proton support

Recommended stack: Rust for decoding and geometry, standard TCP sockets with a receiver thread, `winit` for ordinary windows, `wgpu` for rendering, and `egui` for settings. Store configuration with serde/TOML. Use small platform modules for overlay placement, focus behavior, and game-window tracking. Pin mutually compatible crate releases during the prototype.

Wgpu supports Windows and Linux graphics backends; egui provides a native Rust UI. This establishes a portable rendering base, not a guarantee of identical desktop overlay behavior. [Wgpu](https://wgpu.rs/), [Egui](https://github.com/emilk/egui)

On Linux, the intended deployment is a native Linux radar beside LFS running through Proton. Test local InSim connectivity in the actual Steam/Proton environment, including any sandbox boundaries. Running the Windows overlay itself through Proton is an optional experiment, not the primary Linux support strategy. [Proton project](https://github.com/ValveSoftware/Proton)

Support targets:

| Environment | Planned support |
| --- | --- |
| Windows, windowed/borderless LFS | Initial production target; verify alpha, pass-through, focus, DPI. |
| Linux X11, LFS through Proton | Initial Linux target; verify compositor transparency and window-manager stacking. |
| Linux Wayland, LFS through Proton | Separate platform effort; support specific tested compositors rather than promising all desktops. |
| Exclusive fullscreen or nested Gamescope | Separate acceptance gate; do not promise external overlay visibility before testing. |

Winit's window-level setting is an OS hint. Wayland needs a suitable surface role and compositor support for reliable overlay stacking; investigate a dedicated layer-shell backend instead of relying only on an ordinary application window. The layer-shell protocol defines an overlay layer. [Winit window level](https://docs.rs/winit/0.30.13/winit/window/enum.WindowLevel.html), [Layer-shell protocol](https://github.com/swaywm/wlr-protocols/blob/master/unstable/wlr-layer-shell-unstable-v1.xml)

Extend milestone 1 with native Windows and native Linux/X11 prototypes. Record the tested Linux desktop, display server, GPU, Proton version, and LFS display mode. Wayland and Gamescope support require their own tested matrix. Shared radar logic should remain independent of these windowing backends.
