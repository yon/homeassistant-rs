//! Config Entries
//!
//! This crate provides the configuration entry system for Home Assistant.
//! Config entries represent individual integration instances and manage
//! their lifecycle (setup, unload, reload).
//!
//! # Key Types
//!
//! - [`ConfigEntry`] - A single integration configuration
//! - [`ConfigEntryState`] - Lifecycle state of an entry
//! - [`ConfigEntries`] - Manager for all config entries
//!
//! # Storage
//!
//! Config entries are persisted in `.storage/core.config_entries` with
//! version tracking for migrations.

pub mod entry;
pub mod manager;

// Re-export main types
pub use entry::{
    ConfigEntry, ConfigEntryDisabledBy, ConfigEntrySource, ConfigEntryState, ConfigEntryUpdate,
};

pub use manager::{
    ConfigEntries, ConfigEntriesData, ConfigEntriesError, ConfigEntriesResult, SetupHandler,
    STORAGE_KEY, STORAGE_MINOR_VERSION, STORAGE_VERSION,
};
