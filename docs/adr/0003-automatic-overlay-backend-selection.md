---
status: accepted
---

# Select the overlay backend automatically at startup

The user approved automatic selection after the original explicit-selection
implementation. The saved backend preference now defaults to Auto. On Linux,
Auto requires a KDE Wayland session, a reachable Wayland connection, and the
compositor's Layer Shell support before choosing the native HUD. Other sessions,
Windows, and builds without native support choose Window. The probed connection
is reused for native startup, including inherited Wayland sockets.

The active backend and saved preference are separate: saving Auto retains Auto
for detection on the next launch, and explicit Window or Native Wayland choices
remain overrides. The existing CLI and panel allow either override. Placement
continues to follow [ADR 0002](0002-separate-wayland-hud-layout.md).

Automatic selection is limited to startup. Once native rendering is selected,
errors stay visible for retry or an explicit switch, preserving the failure
handling in [ADR 0001](0001-native-wayland-overlay-worker.md). This expands the
original automatic-selection deferral without adding runtime failover or live
backend switching.
