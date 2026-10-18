# Tyre stress prototype

Prototype branch: `prototype/tyre-stress-index`. The question is whether a
per-wheel load, motion and combined-slip index behaves usefully enough to test
against observed LFS tyre wear. This is an offline experiment, with no HUD,
installed-binary replacement, configuration edits or additional dependencies.

## Open a report

Generate a standalone HTML report from an existing local recording:

```sh
cargo run --no-default-features --locked --example tyre-stress-prototype -- \
  recordings/CAPTURE.jsonl target/tyre-stress-report.html
```

Open `target/tyre-stress-report.html` in a browser. It contains the decoded data,
chart, sample scrubber, editable wheel radii, telemetry table and synthetic guided
experiments. It needs no server or network connection. It refuses to overwrite
an existing report; use another filename or explicitly remove your old output.
Reports contain local driving telemetry, so review that before sharing them.

To try the synthetic experiments without a recording:

```sh
cargo run --no-default-features --locked --example tyre-stress-prototype -- \
  --demo target/tyre-stress-demo.html
```

The exporter reuses OpenRadar's recording schema and production OutSim/OutGauge
decoders. It requires the extended wheel block, such as `OutSim Opts 1ff`, and
validates packet sequence, decoded source clocks and the footer count. Recover
interrupted recordings with `--recover-recording` first. A recovered/incomplete
file remains marked incomplete in its report. No recorded packet is resampled
before integration; only the displayed chart is reduced for browser drawing.

## Index definition

For wheel index `i`:

```text
motion_i = max(length(car_velocity), abs(wheel_angular_velocity_i) * radius_i)
rate_i   = max(vertical_load_i, 0) / 1000 * motion_i * SlipFraction_i / 255
stress_i = integral(rate_i dt)
```

`radius_i` is a user-supplied nominal radius in metres. The initial `0.300 m`
values are illustrative assumptions, not detected car dimensions. Wheel names
remain W0–W3 until corner order has been checked against live observations.

This is a bounded, load-weighted motion/slip exposure proxy. Its stress units
are defined by this formula (kN·m weighted by the slip fraction), **not measured
frictional energy, rubber volume, wear percentage, remaining life or grip loss**.
Zero reported slip produces zero index; ordinary rolling wear is not modeled.
The formula intentionally handles both stationary burnouts and locked-wheel
slides without trying to invert an undocumented longitudinal-slip convention.

LFS's [slip-fraction definition](https://www.lfs.net/programmer/raf) describes
combined slip relative to the slip producing greatest force. Values 0–254
approach the peak-force slip; 255 means beyond it and saturates. The prototype
therefore cannot distinguish increasingly severe slides once this field reaches
255. Raw `SlipRatio`, `TanSlipAngle`, forces and rotation remain inspectable.

The integral uses source-clock seconds and trapezoidal integration, with both
interval endpoints required to have contact and positive load. Internal tyre air
temperature is displayed as reported; no ambient/surface temperature, thermal
state or wear multiplier is inferred. A later slip-energy model needs validated
per-wheel slip velocities and compatible force coordinates before it can improve
on this deliberately simple proxy.

## Boundaries and validation

Recorded OutSim/global gaps, observed car/PLID identity changes, duplicate or
backwards source clocks, and source/receipt intervals over 250 ms start a new
segment at zero. Receipt-clock regression also splits segments; normal source
clock rollover is supported. Segments are never summed into a claimed stint
total. Unknown contact transitions and missing data can undercount stress.

Record one known tyre stint at a time. OutSim/OutGauge cannot reliably identify
tyre replacement, every driver/session change or every teleport. A recorder
header's track is user-declared. Do not join runs across fresh tyres or treat
the first received sample as proof that a tyre is new. Air temperature cannot
describe tread heat, and one scalar per wheel cannot reproduce local flat spots.

The standalone model has a runnable `modelSelfCheck()` that executes when the
report opens. It covers zero slip, contact/load gating, load scaling, burnout,
lockup, radius sensitivity, unchanged response to temperature, clock rollover
and discontinuities. These checks verify arithmetic and state handling only.

For the next experiment:

1. Keep car, compound, track, setup, nominal radii and recording coverage fixed.
   Start each capture with a known tyre state; stop before changing tyres/car.
2. Compare gentle and aggressive runs. Inspect stress trends and synchronized
   F9 observations, keeping the F9 display scale consistent. F9 thickness is a
   graphical reference requiring its own calibration, not numerical ground truth.
3. Investigate exceptions, especially lockups, prolonged saturated slip and
   uneven wear. Fit a wear conversion only if an independent wear reference is
   available, then validate it on separate runs.

Current verdict: the prototype makes these comparisons possible. Predictive wear
accuracy and wheel corner mapping remain unvalidated. Keep it on the prototype
branch until the experiment answers those questions; the production HUD does
not yet have a tyre-wear or tyre-stress gadget.

Creation checks processed three existing recordings (2,006, 14,222 and 88,677
OutSim samples) and the checked-in protocol fixture. Numerical checks found
finite, nonnegative, nondecreasing integrals within each segment. A separate
headless Firefox session exercised the recorded chart, segment selection,
sample navigation, radius validation, guided experiments and playback controls.
Malformed/truncated inputs and existing output paths were also checked. These
results demonstrate a runnable prototype; they do not establish wear accuracy.
