# Development

Use Rust 1.88 or newer and keep the checked-in Cargo.lock and `vendor/eframe`
directory. Windows needs Visual Studio C++ Build Tools and a Windows SDK.

## Linux

<details>
<summary>Ubuntu/Debian</summary>

Install the native development dependencies before building:

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config \
  libx11-dev libxrandr-dev libxcursor-dev libxi-dev libxinerama-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libgl1-mesa-dev libegl1-mesa-dev libudev-dev
```

`build-essential` provides the C/C++ compiler and build tools. The remaining
packages provide the native libraries needed by the desktop app. These match
the Ubuntu build dependencies in the [CI workflow](../.github/workflows/ci.yml).

</details>

<details>
<summary>Arch Linux</summary>

Update the system and install the native development dependencies from the
official repositories before building:

```bash
sudo pacman -Syu --needed \
  base-devel pkgconf \
  libx11 libxrandr libxcursor libxi libxinerama \
  libxkbcommon libxkbcommon-x11 wayland \
  libglvnd systemd-libs
```

[`base-devel`](https://archlinux.org/packages/core/any/base-devel/) provides the
C/C++ compiler and build tools. Arch packages include their development headers;
[`libglvnd`](https://archlinux.org/packages/extra/x86_64/libglvnd/files/) supplies
OpenGL/EGL headers, and
[`systemd-libs`](https://archlinux.org/packages/core/x86_64/systemd-libs/files/)
supplies libudev.

</details>

## Build from source

```text
cargo build --release --locked
cargo run --locked -- --demo
```

The executable is placed in `target/release/`.

For the standalone native Wayland Layer Shell experiment, see the
[MVP run instructions and live acceptance checks](wayland-mvp.md).
The integrated GPU backend is included by default and selected automatically
on supported KDE/KWin Wayland sessions when the preference is Auto;
see the [native Wayland guide](native-wayland.md) for launch and live-test commands.

## Testing

Run the local Rust checks before submitting code changes:

```text
cargo fmt --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --no-default-features --locked
```

The tests cover telemetry packets, socket behavior, geometry, lap-reference
validity, and startup-script changes. For a demo check without graphics or LFS:

```text
cargo run --no-default-features --locked -- --demo --headless --seconds 3
```

Release automation has a separate Python 3.11+ test suite:

```text
python -m unittest discover -s .github/scripts -p "test_*.py" -v
```

GitHub Actions checks PRs on Windows and Linux. Synthetic tests do not replace
live driving checks for timing accuracy, transparency, click-through, monitor
changes, and Linux/Wine/Proton behavior. See the
[acceptance criteria](proximity-radar-plan.md).

## Linux global shortcut checks

Desktop builds include a small D-Bus client with a vendored libdbus, so no
additional system development package is required. Linux KDE/KWin registers
the configured key with KGlobalAccel and listens only to its pressed signal;
held-key repeats use a separate signal. Registration is removed at shutdown.
The implementation follows [KGlobalAccel's client registration](https://github.com/KDE/kglobalaccel/blob/master/src/kglobalaccel.cpp)
and [setter flags](https://github.com/KDE/kglobalaccel/blob/master/src/kglobalaccel_p.h).

Run the opt-in integration test in a real KDE session with Vulkan available.
It temporarily binds F24 and Insert, exercises both overlay backends through
the panel's update interface, checks second-instance fallback, and releases the
bindings. Close other OpenRadar instances first and run it serially:

```bash
cargo test --lib kde_shortcut_toggles -- --ignored --test-threads=1
```

The test invokes KDE's action over D-Bus. Also check the physical key once
with LFS focused to confirm the compositor delivers it during gameplay.

Automatic backend selection is also exercised through the real executable:

```bash
cargo test --test native_wayland auto_backend -- --ignored --test-threads=1
```

These checks cover native startup on KDE, the explicit Window override, X11,
other desktop markers, an unreachable Wayland socket, and unchanged saved
preferences. The ordinary config and panel tests check Auto persistence. Builds
without `native-wayland` select Window automatically.
