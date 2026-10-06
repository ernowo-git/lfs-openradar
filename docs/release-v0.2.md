# v0.2 release plan

Prepared 6 October 2026. Status: planned, not tagged or published.
Feature branch: `codex/live-gap-gadgets`.

## Scope

Ship the wrapping control-panel gadget grid, estimated race-order gaps ahead
and behind, independent radar/ahead/behind overlay windows, saved gap window
positions and scales, migration from the earlier shared-canvas layout, and plain
driver names with LFS color codes removed. Keep the existing radar-only settings
as the default. Gear, RPM, split/lap timing, continuous sub-node estimates, and
custom/open-layout gaps remain outside v0.2.

Windows x64 is the primary release platform. Ship a native Linux x64 binary as
experimental, initially targeting an X11 desktop with LFS running through
Wine/Proton in windowed or borderless mode. Native Wayland, Gamescope, exclusive
fullscreen, ARM64, and Linux automatic foreground detection are not acceptance
targets for this release. Linux currently exposes manual overlay visibility.

## Downloads

Build both platforms from the same reviewed release commit with Cargo.lock.

| Asset | Contents |
| --- | --- |
| `lfs-openradar-v0.2-windows-x86_64.exe` | Standalone Windows executable. |
| `lfs-openradar-v0.2-windows-x86_64.zip` | The executable named `lfs-openradar.exe`, example TOML, README, license notices, and relevant setup/troubleshooting docs. |
| `lfs-openradar-v0.2-linux-x86_64` | Native Linux ELF executable. |
| `lfs-openradar-v0.2-linux-x86_64.tar.gz` | Executable named `lfs-openradar` with its executable permission retained, example TOML, README, license notices, and relevant setup/troubleshooting docs. |
| `SHA256SUMS` | SHA-256 checksums for all four downloads. |

Exclude local TOML settings, passwords, logs, recordings, preview images, and
debug/build caches. The bundled example keeps the new gadgets disabled.
Inspect linked runtime dependencies on each platform and document any required
system libraries. These are portable application packages, not fully static or
dependency-free binaries. Check third-party notices, including the local eframe
patch, before packaging.

## Build and verification work

1. Add GitHub Actions checks for Windows and Linux. Start with explicit
   `windows-2022` and `ubuntu-22.04` x64 runners, both currently listed by GitHub.
   Use the older Linux build environment as the intended compatibility baseline;
   confirm the actual ELF/glibc requirements instead of assuming them.
2. Pin one tested Rust toolchain at or above the project's Rust 1.88 requirement
   for both jobs. Include rustfmt and clippy. Install the Linux compiler,
   pkg-config, and the X11/Wayland development dependencies needed by the locked
   eframe/winit graph. Keep the vendored eframe source in the build checkout.
3. Run the existing repository checks on both platforms:

   ```text
   cargo fmt --check
   cargo check --locked
   cargo test --locked
   cargo test --no-default-features --locked
   cargo clippy --all-targets --locked -- -D warnings
   ```

4. Build release-mode desktop executables for `x86_64-pc-windows-msvc` and
   `x86_64-unknown-linux-gnu`, using `cargo build --release --locked --target ...`.
   Upload candidate binaries as workflow artifacts. Build natively on each OS;
   adding a Linux Rust target on Windows alone does not provide its linker and
   native libraries. WSL can supplement local compilation and diagnostics.
5. Smoke-test each actual release executable in headless demo mode. Run an X11
   demo under Xvfb/software rendering in Linux CI where supported, with all three
   windows enabled. Capture a preview and retain failure logs. This exercises
   rendering but does not certify overlay behavior over a game.
6. Inspect the packaged Linux executable with `file`, `readelf`, and its linked
   dependencies. Test startup in a fresh environment matching the chosen Linux
   baseline and a newer distribution. Test the Windows package on a clean
   machine without developer tools; record any runtime prerequisite.
7. Generate packages and checksums from the verified release executables, then
   unpack each package and repeat a startup smoke test on its contents.

## Manual acceptance before release

Record OS, desktop/session type, GPU/driver, LFS/Wine/Proton versions, and game
display mode with the results. A WSL build alone is not Linux gaming acceptance.

- Windows: preserve radar transparency and click-through. Enable both gaps,
  position and scale all three overlays independently, save/reload placement,
  close/re-enable one gadget, and confirm the others keep updating. Exercise
  Alt-Tab, background hiding, visibility toggles, DPI and monitor transitions,
  and settled resizing without redraw stalls or graphics freezes.
- Linux/X11: run the native app beside LFS in the same Linux environment. Verify
  local TCP/UDP telemetry, three independent transparent windows, stacking above
  the game, click-through, title-bar positioning, saved screen coordinates,
  continued redraw, and shutdown. Document manual visibility and compositor
  requirements. An XWayland session needs its own result rather than inheriting
  an X11 claim.
- Gap calculations on both: compare live values with LFS timing using another
  car or AI. Exercise overtakes, finish-line crossing, lapped drivers, stopped
  cars, pit/rejoin, resets, reconnects, and stale/missing telemetry. Confirm
  estimates and measurement ages remain visible and unsupported modes show an
  unavailable value. Verify colored names display as plain text.
- Upgrade: load v0.1 radar-only TOML and the feature branch's earlier normalized
  gap layout. Save, restart, and confirm placement and enabled state survive.

Track failures as release blockers for the claimed Windows or experimental X11
behavior. Linux need not support every desktop, but its advertised X11 setup
must pass live checks. Do not publish a claimed dual-platform release with an
untested Linux binary substituted for these checks.

## Release sequence

1. Finish the build workflow and acceptance work on the feature branch, and
   update the README with exact Linux dependencies and observed support limits.
2. Set Cargo.toml to `0.2.0` and update this package's entry in Cargo.lock. Prepare
   release notes covering the gadgets, separate windows, name cleanup, settings
   migration, and experimental Linux support.
3. Review and merge the feature branch into main. Build and test the exact commit
   intended for the release; create a candidate or prerelease if feedback is
   needed before final acceptance.
4. Create tag `v0.2`, following the existing `v0.1` tag naming. Build the tagged
   commit and prepare a draft GitHub Release with both OS packages, raw binaries,
   checksums, and the recorded platform limitations.
5. Verify download names, checksums, archive contents, Linux permissions, and
   package startup against the tagged commit. Publish v0.2 only after the stated
   gates pass. Preserve the v0.1 release for rollback and comparison.

At the time this plan was written, Windows automated checks and native demo
rendering have passed for the feature work. Linux build/CI setup, packaged
release binaries, and live platform acceptance are still outstanding.

## References

- [GitHub hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Rust cross-compilation](https://rust-lang.github.io/rustup/cross-compilation.html)
- [Managing GitHub releases](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository)
- [Winit window positioning restrictions](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.set_outer_position)
