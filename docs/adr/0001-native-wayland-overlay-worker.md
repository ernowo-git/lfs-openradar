---
status: accepted
---

# Keep the control panel while adding a native Wayland overlay worker

The first native Wayland integration will keep the existing eframe/XWayland control panel and use a dedicated worker to draw all six independently positioned gadgets on one Layer Shell surface. This preserves the current controls and shared painters while avoiding changes to eframe's native event loop; the panel remains the sole owner of LFS connections, and the worker consumes shared snapshots.

The worker owns its Wayland and GPU resources, reports errors to the panel, and stays responsive to commands while hidden. Native failures keep the panel available for retry; switching to X11 is explicit. A native control panel and direct overlay dragging are deferred.

The worker and retained panel are implemented. Native GPU demo and coexistence checks pass; live game acceptance remains tracked in the [confirmed plan](../wayland-gt-integration-plan.md).
