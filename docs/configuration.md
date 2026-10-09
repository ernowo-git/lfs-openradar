# Configuration guide

OpenRadar reads TOML settings. Start by copying
[openradar.example.toml](../openradar.example.toml) to your own config file.
Fields omitted from older configurations use built-in defaults.

## Where the config lives

Without `--config`, the app looks for `openradar.local.toml` in its **working
directory**. This is the folder it was launched from; it is not automatically
the executable's directory. In a terminal, the working directory is the current
folder. A Windows shortcut's **Start in** field determines that folder.

For simple use, keep the executable and `openradar.local.toml` together, then
launch from that folder. Set a shortcut's **Start in** to the same folder.

The config can also live in any other directory. Pass its path explicitly:

```text
lfs-openradar.exe --config "C:\MySettings\openradar.local.toml"
```

On Linux, for example:

```sh
./lfs-openradar --config "$HOME/.config/openradar/openradar.local.toml"
```

When running from the source repository:

```text
cargo run -- --config openradar.local.toml
cargo run -- --config openradar.local.toml --headless --seconds 10
cargo run -- --config openradar.example.toml --demo --headless --seconds 3
```

Relative `--config` paths resolve from the working directory too. Use an absolute
path if you want to select the same file regardless of the launch directory.
Quote paths containing spaces.

## Loading and saving

- If the default `openradar.local.toml` is missing, the app starts with built-in
  defaults. **Save settings** creates that file in the working directory.
- If you pass `--config`, the file must already exist and contain valid TOML.
  A missing or invalid explicit config produces a startup error.
- **Save settings** writes the current panel settings to the same path selected
  at launch, including an explicit `--config` path. The folder must exist and be
  writable.
- **Apply / reconnect** restarts the live telemetry connection with the panel's
  current settings. Saving a file alone does not reconnect the worker.
- When editing TOML externally, restart the app to load the changes. It does not
  watch the file for changes.

Desktop graphics diagnostics are written to `openradar.graphics.log` beside the
chosen config path. See [troubleshooting](troubleshooting.md) for log details.

## HUD appearance

Set the top-level `hud_style` before any `[section]` headings:

```toml
hud_style = "gt" # default; or "classic"
hud_debug = false # show gap diagnostics in either HUD style when true
```

Missing settings default to GT; unknown style names are rejected. The previous
`gt7-inspired` value is still accepted and saved as `gt`. The
**Gadgets** tab's **HUD style** selector applies the choice immediately and
**Save settings** persists it. External TOML edits require restarting the app.
Changing themes retains each gadget's position, scale, and enabled state.
**HUD debug information** in the Gadgets tab controls `hud_debug`. It defaults
to false and reveals passage-history status and estimate measurement age in gap
windows for both Classic and GT.

For source customization, Classic colors and dimensions remain in
`src/overlay/radar_style.rs`, `gap_style.rs`, and `delta_style.rs`, with common
panel defaults in `gadget_style.rs`. The additional theme is defined in
`src/overlay/gt7_style.rs`. `theme.rs` resolves the selection and `render.rs`
draws the gadgets. The same selected dimensions drive native windows and
canvas fitting.

## InSim password

Place this top-level setting before any `[section]` headings:

```toml
insim_password = "your-local-LFS-password"
```

Use the password configured in your **local LFS installation**. In LFS's
`cfg.txt`, find the `Game Admin` entry and use the value after it. Leave the
password blank if that entry is empty. This authenticates the connection to the
local client; players do not need the multiplayer server's admin password. See
the [LFS InSim password documentation](https://en.lfsmanual.net/wiki/IS_ISI#Admin).

You can also enter the password in the control panel's masked **InSim password**
field, then click **Apply / reconnect**. **Save settings** stores the displayed
field's value as plain text in the selected TOML file. The repository's default
local config is ignored by Git. Passwords are redacted from configuration
diagnostics and excluded from logs.

`LFS_INSIM_ADMIN`, when present, overrides the TOML password at launch in both
desktop and headless modes. An empty environment value selects a blank password.
Panel edits followed by **Apply / reconnect** override the launch value for the
current connection; **Save settings** persists the panel field's value.

## Telemetry options

`follow_viewed_car = false` is the default. Set it to `true`, or enable
**Follow viewed car** in the control panel, to follow the watched AI or human
car in a live single-player session. Use cockpit or custom view. Radar, gaps,
and delta share that target; switching cars discards the previous reference lap.
Multiplayer continues requiring your own human car. Replay following is unsupported.

Click **Apply / reconnect** after changing this setting in the panel;
**Save settings** persists it. The existing `OutSim Mode 1` configuration is
sufficient. See [the AI debugging flow](usage.md#follow-viewed-car).

`lfs_directory` remembers the folder containing `LFS.exe` for the control panel's
**LFS startup setup** section. Setting a path or saving TOML does not edit LFS;
click **Enable InSim at startup** to update its startup script. See the
[startup setup guide](lfs-startup-setup.md) for folder selection and port conflicts.

InSim and OutSim addresses must be loopback addresses with nonzero ports.
OutSim options in TOML are decimal: `511` corresponds to LFS's hexadecimal `1ff`.
See [LFS setup](installation.md#connect-lfs) for the required game settings.

Supported OutSim formats:

- The configurable OutSim2 layout, including the full 280-byte LFST packet.
  TIME and MAIN fields are required. If ID validation is enabled, the ID field
  must be present.
- Legacy format: set `outsim_options = 0`. Set `outsim_id = 0` for a 64-byte packet
  without ID, or a nonzero ID for a 68-byte packet with ID.
- A zero `outsim_id` disables ID validation for configurable layouts.

Interpolation defaults to 60 ms. Arrival times align the sources approximately;
MCI does not provide per-car physics timestamps. Extrapolation is disabled.
Telemetry older than 250 ms becomes uncertain; older than 500 ms pauses the
radar. These thresholds are configurable.

Vehicle footprints use approximate configurable dimensions (default 1.8 m by
4.2 m). Blue means nearby, amber means alongside, red means potential contact,
and gray means uncertain telemetry. These are geometric hints, not validated
collision predictions. Model-specific dimensions and origin offsets still need
calibration. A height gate reduces bridge/overpass detections but needs track
testing.

`side_m` accepts 0–100 metres; the desktop slider covers 0–12. Side range controls
uniform zoom in Classic and horizontal placement in GT. Detection includes
half the footprint diagonal to retain cars touching the region.
Other radar distances and car dimensions must remain positive.

For gadget placement and performance-delta behavior, see the usage guide's
[gap gadgets](usage.md#control-panel-and-gadget-grid) and
[live performance delta](usage.md#live-performance-delta) sections.

## Speed dashboard

Enable **Speed dashboard** in Gadgets. It has its own position mode, scale,
and overlay window. Its appearance follows the supplied speed dashboard reference
in either HUD style. Its body and RPM header use #252525 at 40% opacity.
The dashboard uses OutGauge independently of radar OutSim.
OutGauge can use its own UDP port or share the OutSim port. Match
`outgauge_bind` and `outgauge_id` to LFS’s existing OutGauge destination and ID.
A shared port is supported when OutSim packets are not 92 or 96 bytes (the
default full OutSim format is 280 bytes).

In Settings, select the LFS installation and click **Configure OutGauge** while
LFS is closed. This backed-up action changes only OutGauge entries in cfg.txt.
Enable the gadget, start LFS, and drive in cockpit or custom view.
External TOML edits require restarting OpenRadar; use **Save settings** to persist
changes made in the panel. Enabling/disabling the gadget reconnects the worker
so its optional OutGauge socket can be opened/released. If the two sources share
a port, the existing OutSim socket receives both.

Top-level OpenRadar settings, before any table headings:

~~~toml
outgauge_bind = "127.0.0.1:30001"
outgauge_id = 24602 # zero selects packets without an ID
~~~

Matching LFS cfg.txt entries (not TOML):

~~~text
OutGauge Mode 1
OutGauge Delay 2
OutGauge IP 127.0.0.1
OutGauge Port 30001
OutGauge ID 24602
~~~

XFG uses a predefined 8,000 RPM dashboard scale. Save other cars' display names
and full-scale RPM in profiles, or override the XFG scale:

~~~toml
[cars.XFG]
name = "XF GTI"
max_rpm = 8000
~~~

The current car identifier appears in the dashboard card. You can adjust **Max RPM**
there and save settings, or edit the profiles directly. Profile keys are uppercase
built-in car codes or six-digit uppercase vehicle mod IDs. Missing profiles leave
the RPM bar grey while numeric RPM remains available. Nine evenly spaced markers
show 10% intervals across the continuous bar. The bar runs from zero to
the configured limit and clamps at 100%. Set **Blink threshold** in the Speed
dashboard card (1–100%, default 95%). At or above the selected percentage, the
filled portion alternates red and blue (#70BDFF). Below the threshold, the fill stays red.
Set **Blink interval** to choose the time per color (50–500 ms, default 100 ms).
Lower values blink faster; a complete red/blue cycle takes twice that interval. Changes apply immediately; **Save settings** persists the choice.
The top-level TOML settings are `rpm_blink_threshold_percent = 95` and
`rpm_blink_interval_ms = 100`. The demo has a synthetic 13,300 RPM limit.
Speed units follow LFS's km/h/mph preference; gear shows R, N, or the forward gear.

ABS uses the selected car's InSim setup flag: white when enabled, red when
intervening, and grey when disabled. The red state requires both ABS enabled
and its OutGauge warning lamp active. TC mirrors its warning lamp, indicating
intervention or a disabled system. Engine damage uses the game's minor/severe
flags. Headlights are white for sidelights/dipped beam and blue for high beam
or flashing; high beam takes priority when multiple light flags are active. On
InSim 10+ the local human car also uses its actual light switch, so low beam
works on cars whose dashboard has no dipped-beam symbol. Older LFS versions
and followed AI cars use the available OutGauge symbols. The gear turns red while
the game's shift lamp is on. Unsupported lamps and missing/stale/mismatched
telemetry show dashes. Samples must match the selected local/viewed player's ID
and car identifier; the existing live-view restrictions apply. Dashboard updates
remain available when OutSim is missing. OutGauge bind failures do not stop radar.

For connection diagnostics, run OpenRadar with `--headless --seconds 10`
(using the same `--config` path as the desktop app). With Speed dashboard enabled,
this prints its status, latest sample, age, and any OutGauge errors. Close the
desktop app first so the diagnostic process can bind the telemetry socket.

## OutGauge forwarding

OpenRadar can copy OutGauge packets to other local telemetry apps while using
the same stream for Speed dashboard and Fuel. Forwarding also works with both
gadgets disabled and does not require an InSim connection.

For a MOZA wheel, open **Settings**, enable **Forward OutGauge to other apps**,
and use destination port `60000` and **OutGauge ID** `0`. Close LFS and click
**Configure OutGauge** to send its telemetry to OpenRadar. Click **Apply /
reconnect** and **Save settings**, then start LFS and MOZA Pit House.
Keep OpenRadar running while driving.

The equivalent top-level TOML settings are:

~~~toml
outgauge_bind = "127.0.0.1:30001"
outgauge_id = 0
outgauge_forward = ["127.0.0.1:60000"]
~~~

Click **Add destination** for each additional app and set its listening port.
For example, two destinations can use
`outgauge_forward = ["127.0.0.1:60000", "127.0.0.1:60001"]`.
Existing single-address settings such as
`outgauge_forward = "127.0.0.1:60000"` still load; saving writes an array.
An empty array disables forwarding. Every destination receives the same packet
bytes and ID; one send failure does not skip the other destinations.

LFS's `cfg.txt` must use `OutGauge Port 30001`, `OutGauge IP 127.0.0.1`,
`OutGauge Mode 1`, and `OutGauge ID 0`. MOZA receives the forwarded packets on
port `60000`. Its [LFS setup guide](https://support.mozaracing.com/en/support/solutions/articles/70000628538-live-for-speed)
requires ID `0`, which omits the optional identifier from each packet.

Packets retain their original bytes, including an ID when configured. The
destination must be a nonzero loopback endpoint with the same IP family as the
OutGauge receiver and must differ from OpenRadar's UDP receivers. Remove
`outgauge_forward` or clear the checkbox to disable forwarding. OutSim setup
continues to use its own configured destination.

## Fuel

Enable **Fuel** in Gadgets after configuring [OutGauge](#speed-dashboard). Fuel
uses the same receiver as Speed dashboard and works with that gadget disabled,
or with OutSim unavailable. InSim supplies the selected car and race progress.
It follows your own car in multiplayer or the watched car in supported live
single-player follow mode. Enabling/disabling Fuel reconnects the worker.

~~~toml
[fuel]
enabled = true
window_x = 376.0
window_y = 746.0
scale = 1.0
~~~

The Fuel card provides Position mode, X/Y, and scale. Fuel is shown as tank
percentage; no tank-capacity setting is needed. Old `fuel_tank_litres` entries
are accepted for compatibility and removed when settings are saved.

Fuel defaults to disabled in older configurations. It shares the dashboard's
#252525 body at 40% opacity and retains its supplied design in either HUD style.
See [Fuel usage](usage.md#fuel) for the range, margin, and REFUEL meanings.
