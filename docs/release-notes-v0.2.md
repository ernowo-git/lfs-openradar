# LFS OpenRadar v0.2

OpenRadar supports LFS 0.8C29.

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
- Reliable control-panel demo screenshot capture with multiple overlay windows.

## Downloads and startup

The Windows x64 ZIP and experimental Linux x64 tar.gz contain the executable,
example configuration, README, documentation, and third-party license notices.
Raw executables are also available. SHA256SUMS lists the hashes of all four
downloads. Extract a package into a writable folder before saving settings.

The Windows build uses the Microsoft Visual C++ runtime. If startup reports a
missing `VCRUNTIME140.dll`, install the current Microsoft Visual C++ x64
Redistributable from Microsoft's supported-download page:
https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist.

The Linux candidate was built on Ubuntu 22.04 with Rust 1.99.0 and requires
glibc 2.35 or newer, X11 runtime libraries, and a working Vulkan/OpenGL driver.
The tar.gz retains executable permissions. For a raw Linux download, run
`chmod +x lfs-openradar-v0.2-linux-x86_64` before launching it.

Run `lfs-openradar.exe --demo` on Windows or `./lfs-openradar --demo` on Linux
to preview without LFS. For live use, follow the InSim/OutSim instructions in the
README. Enable and position the gap gadgets from their own control-panel cards.

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

Both platforms pass the automated test suite, headless tests, and clippy. Native
Windows DX12 and Linux Xvfb/Vulkan demo previews show all three gadget cards and
the expected 5.0-second ahead and 2.3-second behind estimates. Package startup and
archive contents are checked separately; build details are included in each
package's BUILD-INFO.txt.
