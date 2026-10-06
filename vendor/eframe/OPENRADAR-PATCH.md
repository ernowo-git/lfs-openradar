# Local Windows graphics patches

This directory contains the eframe 0.33.0 source and assets from the installed
Cargo registry package, used through `[patch.crates-io]`. The registry cache is
unchanged. Upstream source is licensed MIT OR Apache-2.0; the MIT license and
copyright notice are included here.

Changes cover `src/native/run.rs`, `src/native/wgpu_integration.rs`, and the added
`src/native/redraw_ledger.rs`.

## Redraw delivery

The request-tracking approach is adapted from
https://github.com/emilk/egui/pull/8650, retrieved on 2026-10-06. That proposal is
open, not an upstream release. The local adaptation also marks direct
`RepaintNow` paints as completed so their requests are not left outstanding.

On Windows, `request_redraw()` does not guarantee delivery of a corresponding
`RedrawRequested` event for each window. The original runner removes scheduled
requests when issuing them. This patch records their order until painting
actually happens. After a delivered redraw, older outstanding requests are
painted directly, under a 5 ms soft budget. A paint already started may exceed
the budget. Outstanding requests remain available for subsequent redraws.
Non-Windows platforms keep their existing redraw-request behavior.

The tracker is CPU-only and covered by four tests included in OpenRadar's
`tests/windows_redraw.rs`. These use the exact source compiled into the patched
backend and simulate lost native redraws, both focus directions, duplicate
requests, delayed work after budget exhaustion, and preservation of newer work.
The tests do not claim GPU/driver stability or live Windows acceptance.

## DirectComposition transparency

When Windows options explicitly select only DX12 with `DxgiFromVisual`, the
wgpu window creation path applies `with_no_redirection_bitmap(true)` to
transparent root and child windows before creating them. It preserves egui's
remaining window attributes and post-creation initialization. Ordinary DXGI
HWND swapchains, other backends, and other platforms retain their previous
window attributes. Existing/custom wgpu setups are not changed.

The user's screenshot after enabling DirectComposition showed a solid white
client-area rectangle underneath the radar. The missing
`WS_EX_NOREDIRECTIONBITMAP` style left the window's normal opaque redirection
surface present beneath the alpha-composited visual. Microsoft explains this
requirement in [its DirectComposition window-layering example](https://learn.microsoft.com/en-us/archive/msdn-magazine/2014/june/windows-with-c-high-performance-window-layering-using-the-windows-composition-engine).
This local change follows that requirement; live compositing still needs
verification after rebuilding.

The style is set at creation and stays stable through position-mode,
visibility, and size changes. The application keeps the zero-alpha clear color
and paints no radar backing outside position mode; it keeps a backing in
position mode and an opaque control-panel frame.

Retain this directory when copying/building OpenRadar. When upgrading eframe,
check whether a released upstream fix replaces this patch and remove the local
override only after rechecking focus changes, position mode, and continuous
overlay animation without resizing, plus background transparency over the game
in both normal and position modes.
