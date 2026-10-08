# Installation

## Download

Download the Windows x64 ZIP or experimental Linux x64 tar.gz from
[GitHub Releases](https://github.com/ernowo-git/lfs-openradar/releases), then
extract it into a writable folder. Archives include the example configuration
and documentation. Raw executables are also available.

Windows requires the Microsoft Visual C++ x64 runtime. Linux requires glibc 2.35
or newer, X11 libraries, and suitable Vulkan/OpenGL drivers. See the
[runtime requirements](release-notes-v0.2.md#downloads-and-startup) and
[platform status](usage.md#platform-status). Wayland, Gamescope, and exclusive
fullscreen overlay behavior remain unverified.

## First configuration

Copy [openradar.example.toml](../openradar.example.toml) to `openradar.local.toml`
in your extracted application folder:

```powershell
Copy-Item openradar.example.toml openradar.local.toml
```

On Linux, use `cp openradar.example.toml openradar.local.toml`.
Launch from this folder, or set a Windows shortcut's **Start in** to it.
The default config location is the **working directory**, which can differ
from the executable's folder.

You can store the config elsewhere by passing an existing file explicitly:

```powershell
.\lfs-openradar.exe --config "C:\MySettings\openradar.local.toml"
```

**Save settings** writes to that same path. Without `--config`, a missing default
file uses built-in defaults and is created when you save. See the
[configuration guide](configuration.md) for all settings and overrides.

## Connect LFS

1. Open **Settings → LFS startup setup** in OpenRadar, select the folder containing
   `LFS.exe`, and click **Enable InSim at startup**. Restart LFS to activate it.
   For the current session only, type `/insim 29999` in LFS instead.
2. Close LFS. In **Settings → LFS startup setup**, click **Configure OutSim**.
   This updates `cfg.txt` to match OpenRadar and saves `cfg.txt.BAK` before editing.
   Existing backups are kept. Start LFS after setup. You can also configure it
   manually with the defaults below:

```text
OutSim Mode 1
OutSim Delay 2
OutSim IP 127.0.0.1
OutSim Port 30000
OutSim ID 24601
OutSim Opts 1ff
```

3. If your local LFS client has a **Game Admin** password, enter it in the masked
   **InSim password** field and click **Apply / reconnect**. Saving settings
   stores the password as plain text in your TOML file.
4. Drive your own car and confirm both telemetry sources are updating.
5. For **Speed dashboard**, close LFS and click **Configure OutGauge**. Enable
   the gadget, restart LFS, and set the car’s **Max RPM**. See the
   [dashboard configuration](configuration.md#speed-dashboard).

See [LFS startup setup](lfs-startup-setup.md) for backups and port conflicts,
or [troubleshooting](troubleshooting.md) if the overlay stays paused.
