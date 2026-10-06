# LFS OpenRadar v0.2

## Changes

- Separate gap-ahead and gap-behind overlay windows, each with its own visibility,
  position, and scale settings, alongside the existing radar.
- A control-panel grid that fills each row from left to right and wraps as the
  window narrows.
- Estimated gaps between adjacent race positions using recent track-node passage
  times. Missing, stale, or unsupported telemetry shows an unavailable value.
- Saved independent gap placements and migration of earlier shared-canvas gap
  settings. New gadgets remain disabled by default.
- Plain driver names with LFS color codes removed.

## Downloads and startup

The Windows x64 ZIP and experimental Linux x64 tar.gz contain the executable,
example configuration, README, documentation, and third-party license notices.
Raw executables are also available. SHA256SUMS lists the hashes of all four
downloads. Extract a package into a writable folder before saving settings.

Run `lfs-openradar.exe --demo` on Windows or `./lfs-openradar --demo` on Linux
to preview without LFS. For live use, follow the InSim/OutSim instructions in the
README. Enable and position the gap gadgets from their own control-panel cards.
Do not copy another driver's local configuration or password.

## Platform scope and remaining validation

Windows x64 is the primary platform. Linux x64 is experimental and targets X11
with LFS through Wine/Proton in windowed or borderless mode. Linux uses manual
overlay visibility. XWayland requires its own compositor-specific test results;
native Wayland overlay positioning and stacking are not supported by this
release. ARM64, Gamescope, and exclusive fullscreen are outside this release's
acceptance scope.

The draft release is for review and testing. Live race-gap comparisons and Linux
gaming overlay behavior must be checked before publication; a headless test or
software-rendered demo does not establish live platform acceptance. See the
release plan for the manual checks.
