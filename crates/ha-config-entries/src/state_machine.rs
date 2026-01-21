//! Config Entry State Machine
//!
//! Enforces valid state transitions for ConfigEntry lifecycle.
//! Based on Python HA's config_entries.py state machine:
//!
//! ```text
//! NotLoaded → SetupInProgress → Loaded
//!                            ↘ SetupError → SetupInProgress (retry)
//!                            ↘ SetupRetry → SetupInProgress (auto-retry)
//!                            ↘ MigrationError (terminal)
//!
//! Loaded/SetupError/SetupRetry → UnloadInProgress → NotLoaded
//!                                                 ↘ FailedUnload (terminal)
//! ```

use crate::entry::ConfigEntryState;
use thiserror::Error;

/// Error when an invalid state transition is attempted
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("Invalid state transition from {from:?} to {to:?}: {reason}")]
pub struct InvalidTransition {
    pub from: ConfigEntryState,
    pub to: ConfigEntryState,
    pub reason: &'static str,
}

impl ConfigEntryState {
    /// Attempt a transition to a new state.
    ///
    /// Returns the new state if valid, or an error describing why the transition
    /// is invalid.
    pub fn try_transition(
        self,
        to: ConfigEntryState,
    ) -> Result<ConfigEntryState, InvalidTransition> {
        use ConfigEntryState::*;

        let valid = match (self, to) {
            // From NotLoaded - can only start setup
            (NotLoaded, SetupInProgress) => true,

            // From SetupInProgress - can go to any setup result state
            (SetupInProgress, Loaded) => true,
            (SetupInProgress, SetupError) => true,
            (SetupInProgress, SetupRetry) => true,
            (SetupInProgress, MigrationError) => true,

            // From SetupError - can retry setup or start unload
            (SetupError, SetupInProgress) => true,
            (SetupError, UnloadInProgress) => true,

            // From SetupRetry - can retry setup or start unload
            (SetupRetry, SetupInProgress) => true,
            (SetupRetry, UnloadInProgress) => true,

            // From Loaded - can only start unload
            (Loaded, UnloadInProgress) => true,

            // From UnloadInProgress - can complete or fail
            (UnloadInProgress, NotLoaded) => true,
            (UnloadInProgress, FailedUnload) => true,

            // Terminal states - no transitions allowed
            (MigrationError, _) => false,
            (FailedUnload, _) => false,

            // All other transitions are invalid
            _ => false,
        };

        if valid {
            Ok(to)
        } else {
            Err(InvalidTransition {
                from: self,
                to,
                reason: Self::transition_error_reason(self, to),
            })
        }
    }

    /// Check if a transition is valid without performing it
    pub fn can_transition_to(self, to: ConfigEntryState) -> bool {
        self.try_transition(to).is_ok()
    }

    /// Get a human-readable reason for why a transition is invalid
    fn transition_error_reason(from: ConfigEntryState, to: ConfigEntryState) -> &'static str {
        use ConfigEntryState::*;

        match (from, to) {
            (MigrationError, _) => "MigrationError is terminal - entry cannot recover",
            (FailedUnload, _) => "FailedUnload is terminal - entry cannot recover",
            (SetupInProgress, NotLoaded) => {
                "Setup in progress - must complete before returning to NotLoaded"
            }
            (UnloadInProgress, Loaded) => "Unload in progress - cannot go back to Loaded",
            (NotLoaded, Loaded) => "Cannot jump to Loaded - must go through SetupInProgress",
            (NotLoaded, SetupError) => {
                "Cannot jump to SetupError - must go through SetupInProgress"
            }
            (Loaded, NotLoaded) => "Cannot jump to NotLoaded - must go through UnloadInProgress",
            (Loaded, SetupInProgress) => "Already loaded - unload first before re-setup",
            _ => "Invalid state transition",
        }
    }
}

/// Calculates retry delay with exponential backoff.
///
/// Follows Python HA pattern: 2^min(tries, 4) * 5 + random jitter
/// This gives delays of: 5s, 10s, 20s, 40s, 80s (then stays at 80s)
pub fn calculate_retry_delay(tries: u32) -> f64 {
    let base_delay = 2_u32.pow(tries.min(4)) * 5;
    // Add small jitter (0-100ms) to prevent thundering herd
    let jitter = rand::random::<f64>() * 0.1;
    base_delay as f64 + jitter
}

#[cfg(test)]
mod tests {
    use super::*;
    use ConfigEntryState::*;

    // ==================== Valid Transitions ====================

    #[test]
    fn test_not_loaded_to_setup_in_progress() {
        assert!(NotLoaded.can_transition_to(SetupInProgress));
        assert_eq!(
            NotLoaded.try_transition(SetupInProgress),
            Ok(SetupInProgress)
        );
    }

    #[test]
    fn test_setup_in_progress_to_loaded() {
        assert!(SetupInProgress.can_transition_to(Loaded));
    }

    #[test]
    fn test_setup_in_progress_to_setup_error() {
        assert!(SetupInProgress.can_transition_to(SetupError));
    }

    #[test]
    fn test_setup_in_progress_to_setup_retry() {
        assert!(SetupInProgress.can_transition_to(SetupRetry));
    }

    #[test]
    fn test_setup_in_progress_to_migration_error() {
        assert!(SetupInProgress.can_transition_to(MigrationError));
    }

    #[test]
    fn test_setup_error_to_setup_in_progress() {
        assert!(SetupError.can_transition_to(SetupInProgress));
    }

    #[test]
    fn test_setup_error_to_unload_in_progress() {
        assert!(SetupError.can_transition_to(UnloadInProgress));
    }

    #[test]
    fn test_setup_retry_to_setup_in_progress() {
        assert!(SetupRetry.can_transition_to(SetupInProgress));
    }

    #[test]
    fn test_setup_retry_to_unload_in_progress() {
        assert!(SetupRetry.can_transition_to(UnloadInProgress));
    }

    #[test]
    fn test_loaded_to_unload_in_progress() {
        assert!(Loaded.can_transition_to(UnloadInProgress));
    }

    #[test]
    fn test_unload_in_progress_to_not_loaded() {
        assert!(UnloadInProgress.can_transition_to(NotLoaded));
    }

    #[test]
    fn test_unload_in_progress_to_failed_unload() {
        assert!(UnloadInProgress.can_transition_to(FailedUnload));
    }

    // ==================== Invalid Transitions ====================

    #[test]
    fn test_not_loaded_cannot_jump_to_loaded() {
        let result = NotLoaded.try_transition(Loaded);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.from, NotLoaded);
        assert_eq!(err.to, Loaded);
    }

    #[test]
    fn test_loaded_cannot_jump_to_not_loaded() {
        assert!(!Loaded.can_transition_to(NotLoaded));
    }

    #[test]
    fn test_loaded_cannot_go_to_setup_in_progress() {
        assert!(!Loaded.can_transition_to(SetupInProgress));
    }

    #[test]
    fn test_setup_in_progress_cannot_go_to_not_loaded() {
        assert!(!SetupInProgress.can_transition_to(NotLoaded));
    }

    #[test]
    fn test_unload_in_progress_cannot_go_to_loaded() {
        assert!(!UnloadInProgress.can_transition_to(Loaded));
    }

    // ==================== Terminal States ====================

    #[test]
    fn test_migration_error_is_terminal() {
        // MigrationError cannot transition to any state
        assert!(!MigrationError.can_transition_to(NotLoaded));
        assert!(!MigrationError.can_transition_to(SetupInProgress));
        assert!(!MigrationError.can_transition_to(Loaded));
        assert!(!MigrationError.can_transition_to(SetupError));
        assert!(!MigrationError.can_transition_to(SetupRetry));
        assert!(!MigrationError.can_transition_to(UnloadInProgress));
        assert!(!MigrationError.can_transition_to(FailedUnload));
    }

    #[test]
    fn test_failed_unload_is_terminal() {
        // FailedUnload cannot transition to any state
        assert!(!FailedUnload.can_transition_to(NotLoaded));
        assert!(!FailedUnload.can_transition_to(SetupInProgress));
        assert!(!FailedUnload.can_transition_to(Loaded));
        assert!(!FailedUnload.can_transition_to(SetupError));
        assert!(!FailedUnload.can_transition_to(SetupRetry));
        assert!(!FailedUnload.can_transition_to(UnloadInProgress));
        assert!(!FailedUnload.can_transition_to(MigrationError));
    }

    // ==================== Error Messages ====================

    #[test]
    fn test_error_message_for_terminal_state() {
        let result = MigrationError.try_transition(NotLoaded);
        let err = result.unwrap_err();
        assert!(err.reason.contains("terminal"));
    }

    #[test]
    fn test_error_display() {
        let err = InvalidTransition {
            from: NotLoaded,
            to: Loaded,
            reason: "test reason",
        };
        let msg = format!("{}", err);
        assert!(msg.contains("NotLoaded"));
        assert!(msg.contains("Loaded"));
        assert!(msg.contains("test reason"));
    }

    // ==================== Retry Delay ====================

    #[test]
    fn test_retry_delay_exponential_backoff() {
        // Base delays: 5, 10, 20, 40, 80 (then caps at 80)
        let delay0 = calculate_retry_delay(0);
        let delay1 = calculate_retry_delay(1);
        let delay2 = calculate_retry_delay(2);
        let delay3 = calculate_retry_delay(3);
        let delay4 = calculate_retry_delay(4);
        let delay5 = calculate_retry_delay(5);

        // Check base values (with tolerance for jitter)
        assert!((5.0..5.2).contains(&delay0));
        assert!((10.0..10.2).contains(&delay1));
        assert!((20.0..20.2).contains(&delay2));
        assert!((40.0..40.2).contains(&delay3));
        assert!((80.0..80.2).contains(&delay4));
        // Capped at 80
        assert!((80.0..80.2).contains(&delay5));
    }

    // ==================== Complete State Graph Coverage ====================

    #[test]
    fn test_full_setup_success_path() {
        // NotLoaded -> SetupInProgress -> Loaded -> UnloadInProgress -> NotLoaded
        let state = NotLoaded;
        let state = state.try_transition(SetupInProgress).unwrap();
        let state = state.try_transition(Loaded).unwrap();
        let state = state.try_transition(UnloadInProgress).unwrap();
        let state = state.try_transition(NotLoaded).unwrap();
        assert_eq!(state, NotLoaded);
    }

    #[test]
    fn test_setup_retry_path() {
        // NotLoaded -> SetupInProgress -> SetupRetry -> SetupInProgress -> Loaded
        let state = NotLoaded;
        let state = state.try_transition(SetupInProgress).unwrap();
        let state = state.try_transition(SetupRetry).unwrap();
        let state = state.try_transition(SetupInProgress).unwrap();
        let state = state.try_transition(Loaded).unwrap();
        assert_eq!(state, Loaded);
    }

    #[test]
    fn test_setup_error_recovery_path() {
        // NotLoaded -> SetupInProgress -> SetupError -> SetupInProgress -> Loaded
        let state = NotLoaded;
        let state = state.try_transition(SetupInProgress).unwrap();
        let state = state.try_transition(SetupError).unwrap();
        let state = state.try_transition(SetupInProgress).unwrap();
        let state = state.try_transition(Loaded).unwrap();
        assert_eq!(state, Loaded);
    }

    #[test]
    fn test_unload_from_setup_error() {
        // NotLoaded -> SetupInProgress -> SetupError -> UnloadInProgress -> NotLoaded
        let state = NotLoaded;
        let state = state.try_transition(SetupInProgress).unwrap();
        let state = state.try_transition(SetupError).unwrap();
        let state = state.try_transition(UnloadInProgress).unwrap();
        let state = state.try_transition(NotLoaded).unwrap();
        assert_eq!(state, NotLoaded);
    }

    #[test]
    fn test_failed_unload_path() {
        // NotLoaded -> SetupInProgress -> Loaded -> UnloadInProgress -> FailedUnload (terminal)
        let state = NotLoaded;
        let state = state.try_transition(SetupInProgress).unwrap();
        let state = state.try_transition(Loaded).unwrap();
        let state = state.try_transition(UnloadInProgress).unwrap();
        let state = state.try_transition(FailedUnload).unwrap();
        assert_eq!(state, FailedUnload);
        // Cannot recover from FailedUnload
        assert!(state.try_transition(NotLoaded).is_err());
    }
}
