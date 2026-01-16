//! Home Assistant API Comparison Testing Library
//!
//! This crate provides infrastructure for comparing our Rust HA implementation
//! against a real Python Home Assistant instance to ensure API compatibility.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐      ┌─────────────────┐
//! │  Python HA      │      │  Rust HA        │
//! │  (Docker)       │      │  (ha-server)    │
//! │  :18123         │      │  :18124         │
//! └────────┬────────┘      └────────┬────────┘
//!          │                        │
//!          └──────────┬─────────────┘
//!                     │
//!              ┌──────▼──────┐
//!              │  Comparison │
//!              │  Test Suite │
//!              └─────────────┘
//! ```

pub mod client;
pub mod compare;
pub mod config;
pub mod harness;
