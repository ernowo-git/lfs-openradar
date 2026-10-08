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
