// Exercise the exact Windows backend request tracker without creating any GPU
// surfaces or relying on Windows delivering a native redraw event.
#[path = "../vendor/eframe/src/native/redraw_ledger.rs"]
mod redraw_ledger;
