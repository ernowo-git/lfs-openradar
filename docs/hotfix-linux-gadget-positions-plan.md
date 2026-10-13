# Hotfix plan: persist gadget positions on Linux

Research date: 2026-10-13. Baseline: `main` at `c8ca600`, version `0.4.1`.
Reported task: persist gadget positions after Save settings; enable/disable and
scale already persist. Source supplied by the user: `[[Journal/2026-10-08]]`.
The journal entry is not in this repository. The user confirmed **KDE + Wayland**;
the distro, movement method, and actual OpenRadar window backend are unconfirmed.

## Finding

Native Wayland cannot support the application's current absolute-coordinate
save/restore design. This is the leading diagnosis for the reported session,
supported by the exact locked dependency sources; it has not been reproduced
on the user's desktop.

- `Cargo.toml:13` enables both `x11` and `wayland` on eframe, with default features
  disabled. `src/overlay/desktop.rs:55` uses default event-loop selection.
- Cargo.lock resolves egui-winit `0.33.3` and winit `0.30.13`.
  [winit's Linux backend selection](https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/linux/mod.rs#L734-L775)
  prefers Wayland when its display/socket variables are present, even with X11
  available. A KDE Wayland session alone does not prove which backend was used.
- [winit's position API](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.outer_position)
  cannot query outer or inner positions on Wayland.
  [Setting a position](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.set_outer_position)
  is also unsupported. The cached Wayland implementation returns errors for both
  queries and makes `set_outer_position` a no-op.
- `overlay_painted` (`src/overlay/desktop.rs:1415`) records moves only when
  egui's viewport `outer_rect` is available. egui-winit derives that rectangle
  from the native outer-position query, so it is absent on native Wayland.
- `receive_overlay_feedback` (`src/overlay/desktop.rs:457`) copies observed moves
  to radar `overlay_x`/`overlay_y` or each other gadget's `window_x`/`window_y`.
  `App::update` drains feedback before drawing the Save settings button.
- Save settings (`src/overlay/desktop.rs:651`) calls `Config::save`
  (`src/config.rs:254`), which already serializes coordinates, enabled states,
  and scales. Load and `prepare_gap_positions` retain explicit saved coordinates.
  `src/overlay/window.rs` supplies them to each native window builder.

This explains both failure modes: title-bar movement never updates the config
on native Wayland, and manually entered X/Y values can be saved but cannot place
the native Wayland window on restart. Changing TOML serialization or falling
back to `inner_rect` would not resolve the backend limitation.

## Proposed change

1. Reproduce using a temporary config on KDE Wayland. Move one gadget by its
   title bar, inspect panel X/Y, save, inspect TOML, then restart with the same
   config path. Repeat with explicit panel X/Y. Record whether the app is native
   Wayland or XWayland; compare with an X11 launch before changing code.
2. Remove `wayland` from eframe's feature list in `Cargo.toml`, retaining
   `default_fonts`, `wgpu`, and `x11`. Add a short manifest comment explaining
   that absolute gadget positions require X11/XWayland. eframe's
   [feature forwarding](https://github.com/emilk/egui/blob/0.33.0/crates/eframe/Cargo.toml)
   already propagates backend selection to its windowing dependencies.
3. Refresh Cargo.lock only if required by the feature change, preserving resolved
   package versions. Verify the Linux feature graph with
   `cargo tree --locked --target x86_64-unknown-linux-gnu -e features -i winit`:
   `winit/x11` must be enabled and `winit/wayland` disabled. Wayland-related
   clipboard dependencies may remain; their presence alone is not a failure.
4. Update usage/platform status, installation, and troubleshooting docs to explain
   that KDE Wayland uses XWayland, and that XWayland plus a working `DISPLAY` is
   required. Native Wayland positioning remains unsupported. Do not silently
   fall back to a backend that cannot restore positions.
5. Add one focused regression test in `src/overlay/desktop.rs` using existing
   egui RawInput/deferred callback patterns. Supply native `outer_rect` values
   for all six gadget viewports, drain feedback through the normal update path,
   activate Save settings, load the temporary TOML, and verify each independent
   X/Y, enabled state, scale (radar size), and restored builder position. Include
   a valid negative coordinate. Also cover an absent rectangle preserving the
   previous config. Keep the existing guarantee that observing an OS position
   does not issue another native move command.

The intended behavior is that the user's KDE session stays on Wayland while
OpenRadar uses XWayland. Existing configuration fields and migration remain
sufficient. No new dependency, compositor integration, or vendor patch is needed.

## Validation and acceptance

Run the repository's Windows and Linux checks:

```text
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --no-default-features --locked
```

CI also runs the existing Python release-automation tests. WSL inspection found
`Ubuntu-26.04`, which is available for Linux compilation/checks. WSL or an Xvfb
demo alone cannot validate KDE window-manager behavior.

On KDE Wayland with XWayland available, use a disposable config and demo mode:

- Enable radar, dashboard, gap ahead, gap behind, delta, and fuel. Place each
  independently by title-bar dragging; check that the corresponding panel X/Y
  follows. Turn Position mode off, save, quit, and restart with the same file.
  All six must restore their final borderless positions.
- Repeat using panel X/Y and Move gadget/Move radar controls. Test Save settings
  immediately after completing a move and after toggling Position mode.
- Repeat several save/restart cycles to detect title-bar/frame-offset drift.
  Test a secondary monitor, negative coordinates, and fractional display scale
  where available. Keep expected off-screen recovery when a monitor disconnects.
- Confirm enabled states, gadget scales/radar size, transparency, overlay
  visibility, and click-through still work. Repeat placement on an X11 session
  and run a Windows smoke check.

If reproduction shows OpenRadar was already using XWayland, investigate its
reported outer rectangle, decoration transitions, DPI conversion, and
creation-time monitor fitting before applying any additional geometry change.
An XWayland window can still be affected by KWin placement rules; backend
selection is necessary for this design, but desktop acceptance is the proof.

## Delivery

Suggested branch: `hotfix/linux-gadget-positions`.
Suggested commit: `fix(overlay): restore Linux gadget positions through XWayland`.
Implementation is applied on `hotfix/linux-gadget-positions`; release work is
deferred. The original research and proposed steps above describe the rationale.
After creating/opening the implementation PR, ask for **Yes - major**, **Yes -
minor**, **Yes - patch**, or **No** before changing the version. A patch bump to
`0.4.2` would fit this fix if approved. For a release, build a Windows `.exe` and
a Linux amd64 binary using an available WSL distribution; tags and release names
must both use `vMAJOR.MINOR.PATCH`.

## Implementation validation (2026-10-13)

- Removed eframe's `wayland` feature and refreshed Cargo.lock offline. No package
  versions changed; unused dependency entries were removed.
- Verified the Linux winit feature graph enables X11 and disables Wayland.
- Added `native_gadget_positions_survive_save_settings_and_restart`, covering
  all six gadgets, negative coordinates, missing position feedback, the actual
  Save settings button, restored builders, enabled states, and scales/radar size.
  Observed moves leave the running builders' requested positions unchanged.
- Updated installation, usage, and troubleshooting documentation for XWayland.
- Windows and Ubuntu-26.04 WSL both passed formatting, compilation, Clippy with
  warnings denied, desktop tests, and headless tests. Cargo checks used `--offline`
  and `--locked` after the lockfile refresh.
- Version/lockfile validation and all 14 release-automation tests passed under
  WSL Python. Windows' `python` command was an unavailable Store alias.
- The first Linux test link exhausted the 3.9 GB `/tmp` filesystem. Rerunning
  with the build cache on WSL's persistent filesystem passed both test suites.

KDE/XWayland live placement, repeated restart drift, fractional scaling, and
over-game overlay behavior remain pending desktop acceptance. The user approved
a patch bump to `0.4.2` for the implementation PR. Release publication remains
pending merge.
