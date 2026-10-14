# Native Wayland overlay MVP

This standalone example tests whether a native Wayland Layer Shell surface can
keep grouped gadgets above the game without a desktop window rule. It uses
`smithay-client-toolkit` with `Layer::Overlay`, an empty input region, and no
keyboard interactivity. All three mock gadgets belong to one surface.

It deliberately uses shared-memory software drawing to isolate the windowing
protocol from GPU setup. The animated radar and two bars are synthetic; this
does not connect to LFS or change the existing application's overlay backend.
Successful testing here is a prerequisite for integrating the existing egui
and wgpu renderer, not proof that that integration already works.

The next stage is specified in the [confirmed GT integration plan](wayland-gt-integration-plan.md): all six real gadgets on KDE/KWin, reusing the current control panel, with monitor selection, saved independent placement, and fractional scaling.
That integration is now available in the application; see the [native Wayland guide](native-wayland.md).

## Run

Use a native Wayland session whose compositor advertises `zwlr_layer_shell_v1`,
such as KDE Plasma, Sway, or COSMIC. GNOME Wayland does not support this protocol.
The example reports an error and exits if it cannot connect or bind Layer Shell;
it does not fall back to XWayland or require a Keep Above rule.

```bash
cargo run --locked --no-default-features --features wayland-mvp \
  --example wayland-overlay -- --seconds 120
```

The default runtime is 60 seconds; `--seconds` accepts 1..3600. Stop earlier with
Ctrl+C in the launching terminal. The timer also expires while the surface is
hidden and the compositor stops sending frame callbacks.

Expect a cyan radar with a moving yellow car and two animated bars, starting
40 logical pixels from the top and left of a compositor-selected monitor.
There is no opaque rectangle behind the entire group. The radar and panels
have translucent backgrounds, with fully transparent space between them.
The overlay never takes keyboard focus and all pixels are click-through.
It ignores panels' reserved space and reserves no desktop space itself.

For a standalone executable:

```bash
cargo build --release --locked --no-default-features --features wayland-mvp \
  --example wayland-overlay
./target/release/examples/wayland-overlay --seconds 120
```

## Live acceptance check

Do this without creating a Keep Above rule. Record the compositor/version,
GPU/driver, display scale, game launch method (Wine/Proton), and game display
mode. Test each display mode you intend to support, including fullscreen.

| Check | Pass condition |
| --- | --- |
| Native protocol | Startup reports Layer Shell available and the first configured buffer commit. |
| Grouping | One surface contains the radar and both bars; there are no separate gadget windows. |
| Stacking | All gadgets remain visible when the game gains focus and after repeated Alt+Tab. |
| Fullscreen | The group remains above the focused fullscreen game on this compositor. |
| Transparency | The desktop/game shows through the gaps and translucent gadget backgrounds. |
| Pointer input | Clicking and scrolling underneath both painted gadgets and gaps reaches the underlying application. |
| Keyboard input | The focused game/application still receives keyboard input when the overlay appears. |
| Animation | The yellow car and both bars continue moving while the game is focused. |
| Exit | The overlay disappears after the requested time or terminal Ctrl+C. |

Logs establish protocol negotiation and buffer commits only. Visual stacking,
focus, and click-through require the live checks above. An Overlay layer is a
compositor request; fullscreen policy must be verified on each target desktop.
There is no guarantee of displaying above lock screens or other Overlay clients.

### Initial local check (2026-10-14)

Tested on KWin 6.7.5 in native Wayland mode, Mesa 26.2.4, Radeon RX 6700 XT,
one 2560x1440 output at scale 1. The protocol trace recorded exactly one
`wl_surface`, Overlay layer 3, an empty input region, keyboard interactivity 0,
and configure/ack at 640x240. A 60-second debug run submitted 2075 buffers and
exited successfully. A desktop screenshot showed all three gadgets above the
terminal, with the terminal visible through the gaps and translucent panels.

This verifies native surface creation, desktop appearance, and continued
frame submission. Focus switching, actual pointer/keyboard delivery,
fullscreen LFS, and other compositors remain unverified; run the checklist
above before treating this as a production backend.

If these checks pass, the next step is to connect the existing egui/wgpu gadget
rendering to this surface and implement position mode. The MVP has a fixed
layout, compositor-selected output, and a 1x buffer that the compositor scales;
monitor selection, sharp HiDPI rendering, dragging, and live telemetry are deferred.

## Automated checks

```bash
cargo test --locked --no-default-features --features wayland-mvp \
  --example wayland-overlay
cargo clippy --locked --no-default-features --features wayland-mvp \
  --example wayland-overlay -- -D warnings
```

The drawing test checks transparent gaps, premultiplied alpha, three visible
gadgets, and changing animation frames without requiring a compositor.
These checks also run in CI; on Windows, the example builds a Linux-only error
stub and does not compile the Wayland dependencies.

Protocol references: [SCTK Layer Shell example](https://docs.rs/crate/smithay-client-toolkit/0.20.0/source/examples/simple_layer.rs),
[Layer Shell protocol](https://wayland.app/protocols/wlr-layer-shell-unstable-v1),
and [supported compositors](https://github.com/wmww/gtk4-layer-shell#supported-desktops).
