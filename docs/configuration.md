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
hud_style = "classic" # or "gt7-inspired"
hud_debug = false # show gap diagnostics in either HUD style when true
```

Missing settings default to Classic; unknown style names are rejected. The
**Gadgets** tab's **HUD style** selector applies the choice immediately and
**Save settings** persists it. External TOML edits require restarting the app.
Changing themes retains each gadget's position, scale, and enabled state.
**HUD debug information** in the Gadgets tab controls `hud_debug`. It defaults
to false and reveals passage-history status and estimate measurement age in gap
windows for both Classic and GT7-inspired.

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
See [LFS setup](../README.md#connect-lfs) for the required game settings.

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
uniform zoom in Classic and horizontal placement in GT7. Detection includes
half the footprint diagonal to retain cars touching the region.
Other radar distances and car dimensions must remain positive.

For gadget placement and performance-delta behavior, see the usage guide's
[gap gadgets](usage.md#gap-gadgets-and-control-panel-grid) and
[live performance delta](usage.md#live-performance-delta) sections.
