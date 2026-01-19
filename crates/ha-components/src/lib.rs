//! Home Assistant Built-in Components
//!
//! This crate contains implementations of Home Assistant's built-in components
//! (integrations) that don't require Python.

mod input_helpers;

pub use input_helpers::{
    load_input_booleans, load_input_numbers, register_input_boolean_services,
    register_input_number_services, InputBooleanConfig, InputNumberConfig,
};
