//! Time control utilities for testing
//!
//! Provides utilities for controlling time in tests, similar to
//! Python HA's async_fire_time_changed.

use chrono::{DateTime, Duration, Utc};
use std::sync::{Arc, RwLock};

/// A controllable time source for testing
#[derive(Clone)]
pub struct MockTime {
    current: Arc<RwLock<DateTime<Utc>>>,
}

impl MockTime {
    /// Create a new mock time starting at the current time
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(Utc::now())),
        }
    }

    /// Create a new mock time starting at a specific time
    pub fn at(time: DateTime<Utc>) -> Self {
        Self {
            current: Arc::new(RwLock::new(time)),
        }
    }

    /// Get the current mock time
    pub fn now(&self) -> DateTime<Utc> {
        *self.current.read().unwrap()
    }

    /// Set the current mock time
    pub fn set(&self, time: DateTime<Utc>) {
        *self.current.write().unwrap() = time;
    }

    /// Advance time by a duration
    pub fn advance(&self, duration: Duration) {
        let mut current = self.current.write().unwrap();
        *current = *current + duration;
    }

    /// Advance time by seconds
    pub fn advance_seconds(&self, seconds: i64) {
        self.advance(Duration::seconds(seconds));
    }

    /// Advance time by minutes
    pub fn advance_minutes(&self, minutes: i64) {
        self.advance(Duration::minutes(minutes));
    }

    /// Advance time by hours
    pub fn advance_hours(&self, hours: i64) {
        self.advance(Duration::hours(hours));
    }

    /// Advance time by days
    pub fn advance_days(&self, days: i64) {
        self.advance(Duration::days(days));
    }
}

impl Default for MockTime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_time() {
        let time = MockTime::new();
        let initial = time.now();

        time.advance_seconds(60);
        let after = time.now();

        assert!(after > initial);
        assert_eq!((after - initial).num_seconds(), 60);
    }

    #[test]
    fn test_mock_time_at() {
        let fixed = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let time = MockTime::at(fixed);
        assert_eq!(time.now(), fixed);

        time.advance_hours(1);
        assert_eq!(time.now().hour(), 1);
    }

    #[test]
    fn test_mock_time_set() {
        let time = MockTime::new();

        let new_time = DateTime::parse_from_rfc3339("2025-06-15T12:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        time.set(new_time);
        assert_eq!(time.now(), new_time);
    }

    use chrono::Timelike;
}
