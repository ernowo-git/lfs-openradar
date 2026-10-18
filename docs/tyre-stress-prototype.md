# Tyre stress and wear estimation prototype

Prototype branch: `prototype/tyre-stress-index`. The question is whether a
per-wheel load, motion and combined-slip index, converted through a configurable
wear coefficient, behaves usefully enough to test against observed LFS tyre wear.
This is an offline HTML experiment, generated separately from the main
application. It adds no HUD gadget or additional dependencies.

## Open a report

Generate a standalone HTML report from an existing local recording:

```sh
cargo run --no-default-features --locked --example tyre-stress-prototype -- \
  recordings/CAPTURE.jsonl target/tyre-stress-report.html
```

Open `target/tyre-stress-report.html` in a browser. It contains the decoded data,
stress and estimated-wear charts, per-wheel remaining-tread cards, sample scrubber,
editable wheel radii, calibration controls, telemetry table and synthetic guided
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

## Slip-angle telemetry

Extended OutSim supplies a signed, dimensionless `TanSlipAngle` float for each
of the four wheels: the **tangent of the tyre's lateral slip angle**. Slip angle
describes the difference between the direction the wheel points and its motion
over the road. It is separate from wheel steering angle and whole-car drift
angle. The decoder preserves it as `Wheel::tan_slip_angle`.
[Installed LFS specification](../../../Games/LFS/docs/OutSimPack.txt),
[OpenRadar decoder](../src/lfs/outsim.rs).

Convert the reported tangent to an angle using:

```text
slip_angle_rad = atan(TanSlipAngle)
slip_angle_deg = atan(TanSlipAngle) * 180 / pi
```

| Reported tangent | Calculated principal angle |
| --- | --- |
| 0 | 0° |
| ±0.1 | ±5.71° |
| ±0.2 | ±11.31° |
| ±0.5 | ±26.57° |
| ±1.0 | ±45° |

`atan` returns the principal angle between −90° and +90°. Tangent alone cannot
distinguish directions separated by 180°, so it does not reconstruct a full
wheel travel direction during reversing or spins. Positive/negative left-right
meaning and wheel corner mapping still require live validation. Low-speed and
airborne readings need care; do not treat them as evidence of meaningful tyre
scrub without checking motion, ground contact and positive load.

The prototype currently shows the **raw tangent** under **More telemetry at
this sample**, alongside longitudinal `SlipRatio`, wheel rotation and forces.
It does not yet display the converted angle in degrees. The stress/wear model
uses the separately reported combined `SlipFraction`; it does not currently
use `TanSlipAngle` as an independent wear term or infer tyre wear directly from
the lateral slip angle.

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

## Tyre-wear estimation

Wear estimation is part of the prototype scope. For each wheel and uninterrupted
segment, the linear empirical model is:

```text
loss_pp       = min(starting_remaining_percent, stress * wear_coefficient)
remaining_pct = max(0, starting_remaining_percent - loss_pp)
```

The coefficient is in **percentage points of original tread per stress unit**.
For example, 1,000 stress units at 0.01 pp/unit predict 10 pp of tread loss;
starting at 80% then predicts 70% remaining. Loss is limited to the specified
starting tread. If starting tread is unknown, the model can show loss (capped
at 100 pp) while remaining tread stays unknown. A missing coefficient keeps
both wear outputs unknown; zero is not accepted as a calibrated wear coefficient.

The report's **Estimated tyre wear** section provides:

- A wear coefficient and four independently editable starting-tread percentages.
- **Use illustrative values**: an arbitrary demonstration coefficient of
  `0.001 pp/unit` and an assumed 100% starting tread for all wheels in the selected
  segment. The resulting percentages remain explicitly marked **ILLUSTRATIVE**,
  including when they are applied to real recorded telemetry.
- **Assume fresh tyres: 100%**, an explicit starting-state assumption which does
  not supply a wear coefficient or assert that the game actually fitted tyres.
- Per-wheel estimated loss and remaining tread at the sample cursor, plus chart
  choices for remaining tread, tread loss and the underlying stress index.
- A **Wear estimation** guided experiment, preloaded with illustrative estimates
  in demo-only reports.

Real recordings begin uncalibrated. Enter a coefficient fitted to independent
wear observations, or click **Use illustrative values** to explore behavior.
For a simple fit, divide observed tread loss in percentage points by the
corresponding accumulated stress, then test on separate runs. User-supplied
coefficients remain labeled unverified; typing a value cannot establish accuracy.

Calibration inputs belong only to the selected segment. Moving across a gap or
identity boundary does not carry a coefficient or assume fresh tyres. Returning
to a previously configured segment restores its inputs. Changing nominal radii
clears wear coefficients because it changes the stress scale; starting-tread
observations are retained. Inputs live in page memory only and are not saved.

This model is a first conversion of the exposure index, not an independently
validated LFS wear law. It inherits zero-slip rolling wear omission, saturated
slip severity and missing thermal/local-wear effects. A predicted 0% is a model
limit and does not predict the game's puncture time.

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
and discontinuities. Wear checks cover conversion units, starting tread,
missing calibration, unknown starting tread, per-wheel load response, depletion
and invalid inputs. These checks verify arithmetic and state handling only.

For the next experiment:

1. Keep car, compound, track, setup, nominal radii and recording coverage fixed.
   Start each capture with a known tyre state; stop before changing tyres/car.
2. Compare gentle and aggressive runs. Inspect stress trends and synchronized
   F9 observations, keeping the F9 display scale consistent. F9 thickness is a
   graphical reference requiring its own calibration, not numerical ground truth.
3. Investigate exceptions, especially lockups, prolonged saturated slip and
   uneven wear. Fit a wear conversion only if an independent wear reference is
   available, then validate it on separate runs.

Current verdict: the prototype provides configurable tyre-wear estimates and
makes these comparisons possible. Predictive wear accuracy and wheel corner
mapping remain unvalidated. Keep it on the prototype
branch until the experiment answers those questions; the production HUD does
not yet have a tyre-wear or tyre-stress gadget.

Creation checks processed three existing recordings (2,006, 14,222 and 88,677
OutSim samples) and the checked-in protocol fixture. Numerical checks found
finite, nonnegative, nondecreasing integrals within each segment. A separate
headless Firefox session exercised the recorded chart, segment selection,
sample navigation, radius validation, guided experiments and playback controls.
Malformed/truncated inputs and existing output paths were also checked. These
results demonstrate a runnable prototype; they do not establish wear accuracy.
