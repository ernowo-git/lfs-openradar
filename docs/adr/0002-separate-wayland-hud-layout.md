---
status: accepted
---

# Save Wayland placement separately from desktop window positions

The native Wayland HUD will store one common layout in logical coordinates relative to the selected monitor, separately from the existing Windows/X11 desktop positions. Reinterpreting the existing coordinates would change their meaning and risk losing a working layout when switching backends; separate placement preserves both arrangements while sharing gadget enablement, appearance, and telemetry settings.

Temporary position constraints and fit-to-monitor scaling affect displayed geometry without silently overwriting saved placement or requested gadget scales. Explicit saving persists adjusted positions. A missing selected monitor hides the HUD until it returns or the user selects another output; per-monitor layouts are deferred.

Separate placement, fitting, and persistence are implemented and covered by configuration, session, and panel tests. See the [confirmed plan](../wayland-gt-integration-plan.md) for the remaining live monitor acceptance checks.
