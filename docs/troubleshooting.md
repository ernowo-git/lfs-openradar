# OpenRadar troubleshooting

Run the commands below from the `lfs-openradar` folder. This guide covers the
current Windows build; Linux/Proton behavior has not been verified here.

## Start with the symptom

| Symptom | Check / action |
| --- | --- |
| Both previews show a paused radar | Read the control-panel status and MCI/OutSim ages. Follow the telemetry checks below. Both sources are required. |
| Panel updates, overlay stays paused or updates only when resizing | Restart with `cargo run --locked` to rebuild/load the current redraw fix. Keep the `vendor/eframe` directory and Cargo patch override. If it recurs, collect the graphics log and note which window had focus. |
| Panel preview and input stop when enabling Position mode, but overlay keeps moving | This was another redraw-delivery symptom. Use the current patched build. Capture the log's panel repaint observation if it recurs; note whether the panel was minimized or obscured. |
| Overlay has a black or white rectangular background outside Position mode | Restart the current build, including the vendor patch. Transparency requires renderer alpha support, an unfilled radar, and Windows windows created without an opaque redirection bitmap. Windows uses DX12 with a DirectComposition visual swapchain; check its startup log entry and the selected `Dx12` backend. Report any alpha-mode warning. |
| Cannot drag the overlay | Enable **Show overlay** and **Position mode**, then drag its native **title bar**. Radar contents are not a drag handle. **Move radar** and **X/Y** in the panel are alternatives. |
| Overlay disappears after switching away from LFS | Windows hides it when **Hide overlay when LFS is in background** is enabled. Position mode overrides that hiding while Show overlay is enabled. |
| Overlay is enabled but cannot be found | Enable Position mode and set X/Y to a visible location, such as 40/160. Check the intended monitor. Windowed/borderless LFS is the initial target; exclusive fullscreen is unverified. |
| Size does not change while dragging its slider | Expected: release the pointer and allow 250 ms without another change. The native window then receives one settled size. |
| Side range changes visually but live detections use the previous range | Click **Apply / reconnect** to update the live telemetry worker's configuration. **Save settings** persists settings for the next launch. |
| InSim reports a password mismatch | Enter LFS's multiplayer admin password in the masked **InSim password** field, then **Apply / reconnect**. Leave it blank only if LFS has no admin password. |
| Have to type `/insim` every time LFS starts | Use **LFS startup setup → Enable InSim at startup**, then restart LFS. See the [startup setup guide](lfs-startup-setup.md) for folder selection, backups, and port conflicts. |
| Startup setup cannot write the script | Check that the selected installation is writable and `autoexec.lfs` is not read-only. The setup reports file errors; an existing script is backed up before changes. |
| Cannot bind OutSim / UDP port already in use | Close any previous radar instance. Check for another telemetry consumer on the same port and configure distinct destinations or an external relay. |
| Whole PC freezes or black-screens | Preserve the next available logs and Windows events. Follow the graphics evidence section; do not repeatedly provoke a hard lock to test a fix. |

Keep the control panel open: closing it exits OpenRadar and releases its sockets.
Disable Position mode after placement to restore the borderless, mouse
click-through overlay with a transparent background. The dark radar backing
appears in Position mode and in the control-panel preview. Save settings after
moving it if the position should survive a restart. **Save settings** also saves
the masked InSim password as plain text in the local TOML file.

## Telemetry checks

MCI means **Multi Car Info**, the InSim packet containing car positions and
headings. OpenRadar uses it for nearby cars. OutSim supplies the local car's
physics pose for smoother orientation. OutGauge is optional and does not replace
either source; its standard packets are ignored if received on the OutSim port.

1. In the local LFS client, type `/insim 29999`, or use the port configured in
   `insim_address`. This listener is separate from the server's LFSLapper connection.
2. With LFS closed, check its `cfg.txt` against the default settings below. Restart
   LFS after editing. Match the radar's `outsim_bind`, `outsim_id`, and
   `outsim_options` if using other values.
3. Drive your own local human car in cockpit or custom view. Pause, replay,
   spectating, free view, or ambiguous local driver selection pauses output.
4. Watch the MCI/OutSim ages and counters. With defaults, telemetry becomes
   uncertain after 250 ms and pauses after 500 ms. Rising rejected/malformed
   counters suggest a packet layout, ID, or source mismatch.

```text
OutSim Mode 1
OutSim Delay 2
OutSim IP 127.0.0.1
OutSim Port 30000
OutSim ID 24601
OutSim Opts 1ff
```

LFS writes the options as hexadecimal `1ff`; OpenRadar TOML uses decimal `511`.
See [configuration and supported layouts](../README.md#configuration) for legacy
OutSim packets. Addresses must be loopback addresses with nonzero ports.

| Control-panel status | Interpretation / next check |
| --- | --- |
| `Waiting for local LFS InSim connection` | Confirm LFS is running, its local InSim listener is enabled, and the address/port match. Check any accompanying connection/password error. |
| `Drive your local human car in cockpit or custom view` | No eligible local driver/view is selected. Return to your own car and an eligible camera. |
| `Waiting for a complete MCI update` | InSim has not supplied a complete car update. Check connection state and whether cars are on track. |
| `OutSim required — waiting for a matching local pose` | No accepted local pose is available. Check cfg.txt, destination port, options/layout, and ID. |
| `Telemetry stale — radar paused` | At least one source stopped updating. Compare the two ages to identify it. |
| `Synchronizing MCI and OutSim` | The interpolation histories do not yet cover a common display time. A brief wait after joining/reconnecting is expected; persistent status needs ages/counters in the report. |
| `Local car missing from MCI — radar paused` | The selected driver is absent from the current car set. Check joining, spectating, and takeover state. |
| `MCI / OutSim association mismatch — radar paused` | Sources disagree on the local pose. Ensure both come from the same local LFS instance and driver; report persistent mismatches rather than disabling validation. |
| `MCI + OutSim connected` | Both sources passed the current gating. No nearby cars can still be a valid result. |

To inspect telemetry without initializing graphics, close the desktop radar first
so it releases the UDP port, then run:

```powershell
cargo run --no-default-features --locked -- --headless --seconds 10
```

Desktop and headless live mode load `insim_password` from TOML. The
`LFS_INSIM_ADMIN` launch environment variable overrides that value when present.
Do not include the password or variable's value in diagnostic output.

A synthetic check needs neither LFS nor sockets and also avoids graphics:

```powershell
cargo run --no-default-features --locked -- --demo --headless --seconds 3
```

Expect `MCI + OutSim connected` and three nearby synthetic cars after startup.
A passing headless check verifies the telemetry path, not native window painting.

## Graphics evidence

Desktop launches append to `openradar.graphics.log` beside the chosen TOML path
(normally this project folder). It records session starts, GPU/backend/driver,
overlay and Position mode toggles, settled sizes, and graphics warnings/errors.
It excludes telemetry and passwords. Preserve it before relaunching if it is
almost full: the file is bounded to 1 MiB and resets at a later launch when
within 4 KiB of that limit. Headless mode does not create a graphics session.

`Control panel has not repainted ...; overlay still repainting` means the child
window kept painting while the panel had no paint for at least five seconds.
Minimizing or obscuring the panel can also produce this observation; pair it
with what you saw. A later entry records when panel repainting resumes.

Timestamps are Unix milliseconds. For example, convert a logged value to
Bangkok time with:

```powershell
[DateTimeOffset]::FromUnixTimeMilliseconds(1791275952645).ToOffset([TimeSpan]::FromHours(7))
```

The earlier log's missing `VK_LAYER_KHRONOS_validation` warning and
`Suboptimal present` warnings do not establish the cause of a freeze. Record
their timing and the selected backend; installing a validation layer is not a
documented fix for the reported redraw problem.

The transparency follow-up found that the shared wgpu painter was initialized
with transparency disabled, because the root control-panel viewport had no
transparent flag. Enabling only the child window and removing its drawn backing
therefore left the presentation path opaque. The root now enables alpha support
while its panel still paints an opaque frame. Windows builds also select DX12
with [`DxgiFromVisual`](https://docs.rs/wgpu/latest/wgpu/enum.Dx12SwapchainKind.html#variant.DxgiFromVisual),
the DirectComposition presentation path that supports
transparent swapchains, instead of relying on the previous Vulkan selection or
DX12's opaque HWND swapchain. This changes the graphics backend on Windows;
other platforms retain their default backend selection. The redraw patch and
settled resize behavior remain in place. Confirm compositing and Position mode
on the live desktop after rebuilding; automated checks do not verify them.

The graphics log records `Windows presentation: DX12 DirectComposition;
transparent backbuffers enabled` before adapter selection. If the new build
still presents a solid rectangle, include the whole session and any alpha-mode warning.
Windows startup deliberately selects this presentation path, overriding
`WGPU_BACKEND` and `WGPU_DX12_PRESENTATION_SYSTEM` for this application.

A later screenshot showed a white rectangle after DX12 was confirmed active.
The native window still had its ordinary opaque redirection bitmap. The local
eframe patch now creates transparent DirectComposition windows with
`WS_EX_NOREDIRECTIONBITMAP`, disabling that extra bitmap. This applies to the
root and child windows at creation and remains stable when changing Position
mode. A complete restart is required to pick up these creation attributes.
See the [vendor patch notes](../vendor/eframe/OPENRADAR-PATCH.md) for scope and
the Microsoft reference. This fix still requires a live visual check.

After a whole-machine freeze, use Event Viewer or these read-only queries for
the last three hours. Adjust the start time to include the incident:

```powershell
$radarIncidentStart = (Get-Date).AddHours(-3)
Get-WinEvent -FilterHashtable @{ LogName = 'System'; StartTime = $radarIncidentStart } |
    Where-Object { $_.ProviderName -match 'Display|AMD|amdw|nvlddmkm|Kernel-Power|WHEA|BugCheck' } |
    Select-Object -First 50 TimeCreated, ProviderName, Id, Message |
    Format-List

Get-WinEvent -FilterHashtable @{ LogName = 'Application'; StartTime = $radarIncidentStart } |
    Where-Object {
        $_.ProviderName -match 'Windows Error Reporting|Application Hang|Application Error' -and
        $_.Message -match 'LiveKernelEvent|AMD_REPORT|WATCHDOG|lfs-openradar'
    } |
    Select-Object -First 50 TimeCreated, ProviderName, Id, Message |
    Format-List
```

Kernel-Power event 41 records an unclean restart, not its cause. Microsoft
[defines 0x141 as a video engine timeout](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x141---video-engine-timeout-detected).
The original incidents had matching watchdog and AMD reports, but the evidence
did not identify the exact failing API/driver path. Keep any referenced dump
files for targeted analysis; they can contain private process data.

## Fix history and regression checks

On 2026-10-06, successive reports exposed several separate problems:

- Continuous native resizing and unnecessary move/surface commands were reduced:
  sizes now settle, observed OS positions do not feed back as move requests, and
  the overlay window stays registered while hidden.
- The overlay now reads fresh worker snapshots and schedules its own repaint,
  so it can leave the paused state without a control-panel paint.
- A local Windows eframe patch retains unanswered native redraw requests and
  services older ones when another window paints. This addresses the overlay
  updating only during resizing and the panel stopping at Position mode entry.
- Position mode restores native borders/title-bar dragging, with panel movement
  controls still available.

The user reported success after the final build. That is live feedback on this
Windows setup, not proof that the earlier GPU hard lock cannot recur. The build
passed 30 automated tests, formatting, Clippy, desktop build, and headless build
checks. See [the investigation](graphics-freeze-investigation.md) for evidence
and [the local patch notes](../vendor/eframe/OPENRADAR-PATCH.md) for provenance.

Keep `vendor/eframe`, the `[patch.crates-io]` override in `Cargo.toml`, and
`Cargo.lock` when copying the project. Before replacing the patch or upgrading
the graphics dependencies, check these behaviors in normal use:

1. With nearby cars, the overlay animates without touching the size slider.
2. Switching focus between LFS and the panel preserves animation and panel input.
3. Position mode permits title-bar dragging; disabling it restores click-through.
4. One ordinary size adjustment settles without continuous native resizing.
5. Missing OutSim pauses both displays rather than leaving stale live positions.

Do not deliberately repeat whole-PC hard locks as an acceptance test. Linux/X11,
Wayland stacking, exclusive fullscreen, and broad DPI/monitor behavior remain
outside the verified scope.

## What to include in a bug report

- Incident time and timezone; whether only one window, the app, or the whole PC
  stopped responding; whether its preview still moved.
- The control-panel status, source ages/counters, LFS camera/driver state, focus,
  and the exact control being used. Say whether demo and headless modes differ.
- The relevant graphics-log session, selected GPU/backend/driver, and matching
  Windows event details if a hard freeze occurred.
- Relevant TOML settings, Windows/LFS versions, and whether the current vendor
  patch was present. Exclude passwords and review logs for private paths before
  sharing. Record which build was run and restart after rebuilding it.
