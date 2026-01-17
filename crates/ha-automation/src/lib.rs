//! Automation Engine
//!
//! This crate provides the automation system for Home Assistant.
//! Automations are event-driven rules that execute actions when triggers fire
//! and conditions are met.
//!
//! # Architecture
//!
//! ```text
//! AUTOMATION = TRIGGER → CONDITIONS → ACTIONS
//! ```
//!
//! - **Triggers**: Event detectors that initiate the automation
//! - **Conditions**: State-based tests evaluated at trigger time
//! - **Actions**: Sequence of tasks to execute (handled by ha-script)
//!
//! # Key Types
//!
//! - [`Trigger`] - Event that starts an automation
//! - [`Condition`] - State check that must pass
//! - [`Automation`] - Complete automation definition
//! - [`AutomationManager`] - Manages all automations

pub mod automation;
pub mod condition;
pub mod eval;
pub mod trigger;
pub mod trigger_eval;

pub use automation::{
    Automation, AutomationConfig, AutomationError, AutomationManager, AutomationResult,
    ExecutionMode,
};
pub use condition::{Condition, ConditionError, ConditionResult};
pub use eval::{ConditionEvaluator, EvalContext};
pub use trigger::{Trigger, TriggerData, TriggerError, TriggerResult};
pub use trigger_eval::{TriggerEvalContext, TriggerEvaluator};
