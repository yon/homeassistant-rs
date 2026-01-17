//! Script Engine
//!
//! This crate provides the script execution engine for Home Assistant.
//! Scripts are sequences of actions that can be triggered by automations,
//! called as services, or executed directly.
//!
//! # Action Types
//!
//! - Service calls
//! - Delays
//! - Wait for trigger
//! - Conditionals (choose, if/then/else)
//! - Loops (repeat)
//! - Variables
//! - Parallel/sequential execution
//!
//! # Key Types
//!
//! - [`Action`] - A single action in a script
//! - [`Script`] - A complete script definition
//! - [`ScriptExecutor`] - Executes scripts

pub mod action;
pub mod executor;
pub mod script;

pub use action::{Action, Target};
pub use executor::{ExecutionContext, ScriptExecutor, ScriptExecutorError, ScriptExecutorResult};
pub use script::{Script, ScriptConfig, ScriptMode};
