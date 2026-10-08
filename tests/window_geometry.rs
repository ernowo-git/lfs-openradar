// Exercise the creation-time geometry used by the native backend without a GPU.
#![cfg(feature = "desktop")]
use eframe::egui;
#[path = "../vendor/eframe/src/native/window_geometry.rs"]
mod window_geometry;
