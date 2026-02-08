//! Home Assistant Python Bridge
//!
//! Embeds the Python interpreter to run Python integrations from Rust.
//! This crate provides the PyO3-based bridge that allows the Rust HA server
//! to load and execute Python config flows, entity platforms, and services.

// PyO3 macros trigger false positive clippy warnings about useless conversions
#![allow(clippy::useless_conversion)]

#[cfg(feature = "py_bridge")]
pub mod py_bridge;

#[cfg(feature = "py_bridge")]
pub use py_bridge::{load_allowlist_from_config, PyBridge, PyBridgeError, PyBridgeResult};
