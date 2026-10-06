//! LFS protocol boundaries. Decode network data here; keep windowing out.
//!
//! Receivers will publish bounded, timestamped telemetry histories. Use a
//! monotonic arrival clock and keep raw protocol units at the decoder boundary.

pub mod insim;
pub mod outsim;
