//! Shared modules for the native Windows and Linux LFS radar application.
//!
//! OutSim is required for the MVP. Live radar output must never silently fall
//! back to MCI-only orientation when OutSim is missing or stale.

pub mod config;
pub mod dashboard;
pub mod delta;
pub mod demo;
pub mod gaps;
pub mod lfs;
pub mod overlay;
pub mod radar;
pub mod runtime;
pub mod setup;

pub const MVP_INPUTS: &str = "InSim/MCI + OutSim (OutGauge is optional)";
