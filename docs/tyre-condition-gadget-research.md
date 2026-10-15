# Tyre condition gadget research

Researched 15 October 2026. Scope: a new OpenRadar gadget for each tyre's wear and hottest temperature. This is research and a proposed implementation path; no telemetry decoder or gadget was implemented.

## Finding

**A per-wheel tyre air-temperature gadget is feasible using the existing OutSim connection. Exact wear and hottest tread temperature are not exposed by the documented interfaces inspected.** The installed first-party OutSim specification exports one `AirTemp` byte per wheel, with no wear, tread depth, inner/middle/outer temperature, or surface-temperature fields. Do not represent air temperature as tread temperature, or an accumulated slip estimate as measured wear. [Installed LFS specification](../../../Games/LFS/docs/OutSimPack.txt)

The official download page currently offers **0.7G** as the old-graphics version and **0.8C31** as the public test. The local installation identifies itself as 0.8C31 in `cfg.txt` and `deb.log`; its bundled `docs/OutSimPack.txt` was read directly. The official release notes establish that extended OutSim has existed since 0.6V. This is stronger evidence for the local test build than relying on an older manual copy; it is not a live UDP validation or an independent inspection of a clean 0.7G installation. [Current downloads](https://www.lfs.net/downloads), [0.6V release notes](https://www.lfs.net/patch-6v)

## What the available interfaces contain

| Interface | Relevant data | Consequence |
| --- | --- | --- |
| Extended OutSim, wheel option `0x80` | Four wheel records; air temperature, forces/load, rotation, steering, slip and contact state | Direct air temperature; no direct wear or tread-temperature measurement |
| Legacy OutSim | Motion and pose | No tyre condition |
| OutGauge | Dashboard values including fuel, controls, speed, engine values | No per-wheel tyre condition |
| InSim | `IS_NPL.Tyres` compounds; `IS_PIT.Tyres` changes; player, camera and race events | Useful for identity and resetting history; tyre arrays are not wear |
| RAF / CAR exports | Replay telemetry / static garage car data | No new live tyre-condition channel |

Sources: [installed OutSim specification](../../../Games/LFS/docs/OutSimPack.txt), [official InSim and dashboard structures](https://www.lfs.net/programmer/insim), [programmer export descriptions](https://www.lfs.net/programmer), [RAF specification](https://www.lfs.net/programmer/raf).

OutSim's documented live scope is the user's car in multiplayer, and the viewed car in single player or a single-player replay, using driver/custom view. A gadget must account for that scope rather than promise telemetry for every opponent or spectator view. The 0.6V release note's multiplayer wear/temperature transmission improvement describes LFS's own network synchronisation; it does not establish a public wear packet. [Official InSim documentation](https://www.lfs.net/programmer/insim), [0.6V release notes](https://www.lfs.net/patch-6v)

## Decoder facts verified against the installed specification

`OutSim Opts` is hexadecimal. `0x1ff` enables all documented options and produces 280 bytes. The wheel option is `OSO_WHEELS = 128`; its four records occupy 160 bytes. For each 40-byte `OutSimWheel`, offsets are:

| Offset | Type | Field |
| --- | --- | --- |
| 0–24 | Seven `float`s | Suspension deflection, steering, X force, Y force, vertical load, angular velocity, lean |
| 28 | `byte` | `AirTemp`, degrees Celsius |
| 29 | `byte` | `SlipFraction` |
| 30 | `byte` | `Touching` |
| 31 | `byte` | Spare |
| 32 | `float` | `SlipRatio` |
| 36 | `float` | `TanSlipAngle` |

The wheel array begins at offset 112 when every option is enabled; other combinations require calculating its offset from enabled preceding blocks. The specification does not name the array's corner order, missing-value sentinels, or special puncture semantics for `AirTemp`. Validate these before assigning FL/FR/RL/RR or inventing interpretations of zero/255. The byte representation has whole-degree granularity; extra decimal places would imply precision the stream does not carry. [Installed specification](../../../Games/LFS/docs/OutSimPack.txt)

## Define “hottest” before presenting a number

The F9 display distinguishes tyre air from three tread bands; its normal numeric temperatures describe band averages through the tread, while Ctrl+Shift exposes surface temperature. These are different measurements. [LFS display manual](https://en.lfsmanual.net/wiki/Display#Tyre_temperature/wear_.26_clutch_temperature_.28F9.29)

For an OutSim-only MVP, use these labels and definitions:

- **Tyre air °C:** each wheel's directly reported `AirTemp`.
- **Hottest air:** maximum current valid air temperature across the four wheels, highlighting the corresponding corner(s).
- **Peak air since reset**, if requested: maximum received air temperature for each wheel over the explicitly stated history interval.

Neither maximum means the hottest location on a tyre. A future **hottest tread band** would require actual band readings and take their current maximum. A **peak tread temperature** would additionally track that value over time. Received-sample peaks can miss spikes between samples. History belongs to a car and tyre stint; changing car, replay seeking, reconnecting and changing tyres must not silently mix histories.

## Options for the full wear and tread-temperature request

1. **Preferred supported path:** ask LFS to expose per-wheel wear/depth and band/surface temperatures through a documented telemetry extension. Until available, show wear/tread temperature as unavailable. Existing exports do not supply the missing measurements. [OutSim specification](../../../Games/LFS/docs/OutSimPack.txt), [official packet definitions](https://www.lfs.net/programmer/insim)
2. **Separate experiment:** screen capture and OCR of F9 could observe the three displayed numbers and compute a maximum. Wear would require analysing the diagram's thickness and calibrating it; it is not a supplied numerical wear percentage. This is an engineering inference from the F9 display, with dependencies on resolution, UI scale, visibility, capture permission and graphics version. First compare captured readings with the visible game. [F9 display description](https://en.lfsmanual.net/wiki/Display)
3. **Unsupported research:** process-memory reading might locate internal values, but no official address/layout contract was found. It would require version-specific discovery, Windows/Wine process access and semantic validation; feasibility is unproven here.
4. **Estimated wear:** force/slip/distance could drive a calibrated model, but those inputs do not uniquely measure tread loss. Such a feature must be explicitly labelled an estimate and validated per car, compound and conditions. It should not delay the supported temperature MVP.

## OpenRadar implementation path

Repository inspection found the infrastructure already needed for a supported air-temperature gadget:

- [OutSim decoder](../src/lfs/outsim.rs) recognises the extended packet, accounts for the wheel block, and currently discards it; `Sample` retains time and pose. Decode optional wheel data there while preserving legacy packets and existing pose consumers.
- [Configuration](../src/config.rs) defaults to `0x1ff`; [LFS setup](../src/setup.rs) writes selected options with a backup. A separate socket or a game configuration change is not needed for this installation.
- Extend the existing engine and shared snapshot with a small tyre frame, following the existing gadget modules. A suggested new domain file is `src/tyres.rs`, with unavailable/stale values represented explicitly.
- Add one shared egui painter, for example `src/overlay/tyre_style.rs`; register the gadget with [overlay session](../src/overlay/session.rs), desktop windows/control cards, native Wayland rendering, settings and demo snapshots.
- Respect [ADR 0001](adr/0001-native-wayland-overlay-worker.md) for shared snapshots and connections, [ADR 0002](adr/0002-separate-wayland-hud-layout.md) for distinct placement configurations, and [ADR 0003](adr/0003-automatic-overlay-backend-selection.md) for backend selection.

Build the smallest decoder-and-diagnostics MVP first. Capture actual 0.8C31 packets without changing game settings; check packet lengths/options, identify corner order with steering/load differences and controlled driving, and compare air readings with the appropriate F9 region rather than its tread-band numbers. Check camera changes and packet loss. Only then build the four-corner UI and sampled air peaks.

Meaningful implementation checks: known wheel bytes decode into the correct records, optional wheel absence stays unavailable, truncated packets remain rejected, legacy pose decoding is preserved, stale/wrong-car telemetry is withheld, and history resets on validated lifecycle events. Final acceptance requires Windows and Linux/Wine runtime checks, including native Wayland and windowed overlays. No live readings, wear accuracy, corner mapping, or new UI were validated during this research.
