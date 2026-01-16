//! Python wrapper for the HomeAssistant struct

use super::{PyEventBus, PyServiceRegistry, PyStateMachine};
use ha_event_bus::EventBus;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;
use pyo3::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Notify;

/// Tracks pending background tasks for async_block_till_done
#[derive(Default)]
pub struct TaskTracker {
    /// Number of pending tasks
    pending_count: AtomicUsize,
    /// Notification for when tasks complete
    notify: Notify,
}

impl TaskTracker {
    pub fn new() -> Self {
        Self {
            pending_count: AtomicUsize::new(0),
            notify: Notify::new(),
        }
    }

    /// Increment the pending task count
    pub fn task_started(&self) {
        self.pending_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement the pending task count and notify waiters if zero
    pub fn task_completed(&self) {
        let prev = self.pending_count.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            // Was 1, now 0 - notify waiters
            self.notify.notify_waiters();
        }
    }

    /// Get the current pending task count
    pub fn pending_count(&self) -> usize {
        self.pending_count.load(Ordering::SeqCst)
    }

    /// Wait for all pending tasks to complete
    pub async fn wait_for_completion(&self) {
        // If no pending tasks, return immediately
        if self.pending_count.load(Ordering::SeqCst) == 0 {
            return;
        }

        // Wait for notification that tasks completed
        // Use a loop in case of spurious wakeups
        loop {
            self.notify.notified().await;
            if self.pending_count.load(Ordering::SeqCst) == 0 {
                break;
            }
        }
    }
}

/// Python wrapper for the central HomeAssistant instance
///
/// This provides access to all core components:
/// - bus: The event bus for pub/sub
/// - states: The state machine for entity states
/// - services: The service registry
#[pyclass(name = "HomeAssistant")]
pub struct PyHomeAssistant {
    bus: Arc<EventBus>,
    states: Arc<StateMachine>,
    services: Arc<ServiceRegistry>,
    task_tracker: Arc<TaskTracker>,
}

#[pymethods]
impl PyHomeAssistant {
    #[new]
    fn new() -> Self {
        let bus = Arc::new(EventBus::new());
        let states = Arc::new(StateMachine::new(bus.clone()));
        let services = Arc::new(ServiceRegistry::new());
        let task_tracker = Arc::new(TaskTracker::new());

        Self {
            bus,
            states,
            services,
            task_tracker,
        }
    }

    /// Get the event bus
    #[getter]
    fn bus(&self) -> PyEventBus {
        PyEventBus::from_arc(self.bus.clone())
    }

    /// Get the state machine
    #[getter]
    fn states(&self) -> PyStateMachine {
        PyStateMachine::from_arc(self.states.clone())
    }

    /// Get the service registry
    #[getter]
    fn services(&self) -> PyServiceRegistry {
        PyServiceRegistry::from_arc(self.services.clone())
    }

    /// Wait for all pending background tasks to complete
    ///
    /// This is essential for tests to ensure all async operations have finished
    /// before making assertions.
    ///
    /// Args:
    ///     wait_background_tasks: If True, also wait for background tasks (default: False)
    ///
    /// Example:
    ///     hass.states.set("light.test", "on")
    ///     hass.block_till_done()  # Wait for state_changed event to propagate
    ///     # Now safe to check event listeners received the event
    #[pyo3(signature = (wait_background_tasks=false))]
    fn async_block_till_done(&self, wait_background_tasks: bool) -> PyResult<()> {
        // Get the current Tokio runtime handle
        let handle = Handle::try_current().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "No Tokio runtime available: {}",
                e
            ))
        })?;

        let task_tracker = self.task_tracker.clone();

        // Block on waiting for tasks to complete
        tokio::task::block_in_place(|| {
            handle.block_on(async {
                // Always yield to let pending micro-tasks run
                tokio::task::yield_now().await;

                if wait_background_tasks {
                    // Wait for tracked background tasks
                    task_tracker.wait_for_completion().await;
                }

                // Yield again to ensure all handlers have processed
                tokio::task::yield_now().await;
            })
        });

        Ok(())
    }

    /// Synchronous version of async_block_till_done
    ///
    /// Waits for all pending tasks to complete. This is a blocking call.
    ///
    /// Example:
    ///     hass.states.set("light.test", "on")
    ///     hass.block_till_done()
    fn block_till_done(&self) -> PyResult<()> {
        self.async_block_till_done(false)
    }

    /// Check if the Home Assistant instance is running
    #[getter]
    fn is_running(&self) -> bool {
        // For now, always return true since we don't have a stop mechanism
        true
    }

    /// Check if the Home Assistant instance is stopping
    #[getter]
    fn is_stopping(&self) -> bool {
        // For now, always return false since we don't have a stop mechanism
        false
    }

    /// Get the number of pending background tasks
    fn pending_task_count(&self) -> usize {
        self.task_tracker.pending_count()
    }

    fn __repr__(&self) -> String {
        format!(
            "HomeAssistant(entities={}, services={}, pending_tasks={})",
            self.states.entity_count(),
            self.services.service_count(),
            self.task_tracker.pending_count()
        )
    }
}

impl PyHomeAssistant {
    pub fn bus_arc(&self) -> &Arc<EventBus> {
        &self.bus
    }

    pub fn states_arc(&self) -> &Arc<StateMachine> {
        &self.states
    }

    pub fn services_arc(&self) -> &Arc<ServiceRegistry> {
        &self.services
    }

    pub fn task_tracker(&self) -> &Arc<TaskTracker> {
        &self.task_tracker
    }
}
