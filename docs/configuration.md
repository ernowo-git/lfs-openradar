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

`lfs_directory` remembers the folder containing `LFS.exe` for the control panel's
**LFS startup setup** section. Setting a path or saving TOML does not edit LFS;
click **Enable InSim at startup** to update its startup script. See the
[startup setup guide](lfs-startup-setup.md) for folder selection and port conflicts.

InSim and OutSim addresses must be loopback addresses with nonzero ports.
OutSim options in TOML are decimal: `511` corresponds to LFS's hexadecimal `1ff`.
See [LFS setup](../README.md#lfs-setup) for the required game settings.

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

For gadget placement and performance-delta behavior, see the README's
[gap gadgets](../README.md#gap-gadgets-and-control-panel-grid) and
[live performance delta](../README.md#live-performance-delta) sections.
