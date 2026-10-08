# Dashboard indicator assets

User-supplied SVGs copied without modification from `Downloads/indicators_svg`.
Embedded in the desktop app and rasterized at a resolution matching the dashboard
scale and display DPI. Textures are cached at powers of two to stay sharp without
rerendering every frame.

| Asset | Original |
| --- | --- |
| abs-off.svg | abs_off.svg |
| abs-on.svg | abs_on.svg |
| abs-warning.svg | abs_triggered.svg |
| tc-off.svg | tc_off.svg |
| tc-on.svg | tc_on.svg |
| engine-off.svg | engine_normal.svg |
| engine-minor.svg | engine_minor.svg |
| engine-major.svg | engine_major.svg |
| headlights-off.svg | light_off.svg |
| headlights-on.svg | light_low.svg |
| headlights-highbeam.svg | light_high.svg |

`fuel-pump.svg` contains the white pump path extracted from the user-supplied
`Downloads/Frame 13.svg` Fuel design. Its original coordinates are preserved.
It supplies the neutral badge while the range estimate is unavailable.

`fuel-safe.svg`, `fuel-warning.svg`, and `fuel-danger.svg` are copied unchanged
from `Downloads/fuel_indicator`. Their green, yellow, and red circles indicate
more than 2, more than 1 through 2, and at most 1 estimated fuel laps remaining.
