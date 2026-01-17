//! Python wrapper for EventBus

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ha_core::Event;
use ha_event_bus::EventBus;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tokio::sync::broadcast;
use tracing::{debug, error};

use super::py_types::{py_to_json, PyContext, PyEvent};

/// Python wrapper for EventBus
#[pyclass(name = "EventBus")]
pub struct PyEventBus {
    inner: Arc<EventBus>,
}

#[pymethods]
impl PyEventBus {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(EventBus::new()),
        }
    }

    /// Fire an event
    ///
    /// Args:
    ///     event_type: The type of event to fire
    ///     event_data: Optional event data as a dictionary
    ///     context: Optional context for the event
    #[pyo3(signature = (event_type, event_data=None, context=None))]
    fn fire(
        &self,
        event_type: &str,
        event_data: Option<&Bound<'_, PyDict>>,
        context: Option<PyContext>,
    ) -> PyResult<()> {
        let data = match event_data {
            Some(dict) => py_to_json(dict.as_any())?,
            None => serde_json::Value::Object(Default::default()),
        };

        let ctx = context.map(|c| c.into_inner()).unwrap_or_default();
        let event = Event::new(event_type, data, ctx);
        self.inner.fire(event);

        Ok(())
    }

    /// Get the number of active event type subscriptions
    fn listener_count(&self) -> usize {
        self.inner.listener_count()
    }

    /// Listen for events of a specific type
    ///
    /// Args:
    ///     event_type: The type of event to listen for (e.g., "state_changed")
    ///                 Use "*" or None to listen to all events
    ///     callback: A callable that takes an Event object
    ///
    /// Returns:
    ///     A callable that, when called, will unsubscribe from the event
    ///
    /// Example:
    ///     def on_state_changed(event):
    ///         print(f"State changed: {event.data}")
    ///
    ///     unsubscribe = bus.listen("state_changed", on_state_changed)
    ///     # Later...
    ///     unsubscribe()  # Stop listening
    #[pyo3(signature = (event_type, callback))]
    fn listen(&self, py: Python<'_>, event_type: &str, callback: PyObject) -> PyResult<PyObject> {
        // Validate the callback is callable
        if !callback.bind(py).is_callable() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "callback must be callable",
            ));
        }

        // Subscribe to the event type
        let rx = if event_type == "*" {
            self.inner.subscribe_all()
        } else {
            self.inner.subscribe(event_type)
        };

        // Create a cancellation flag
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_clone = cancelled.clone();

        // Spawn a task to receive events and call the callback
        let py_callback = callback.clone_ref(py);
        tokio::spawn(async move {
            let mut rx = rx;
            loop {
                // Check if cancelled
                if cancelled_clone.load(Ordering::Relaxed) {
                    debug!("Event listener cancelled");
                    break;
                }

                // Wait for the next event
                match rx.recv().await {
                    Ok(event) => {
                        // Clone with GIL for the blocking task
                        let py_callback_clone = Python::with_gil(|py| py_callback.clone_ref(py));
                        // Call the Python callback in a blocking task
                        let result = tokio::task::spawn_blocking(move || {
                            Python::with_gil(|py| {
                                // Convert the event to a Python Event object
                                let py_event = PyEvent::from_inner(event);
                                // Call the callback
                                if let Err(e) = py_callback_clone.call1(py, (py_event,)) {
                                    error!("Error in event callback: {}", e);
                                }
                            })
                        })
                        .await;

                        if let Err(e) = result {
                            error!("Task panicked: {}", e);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        debug!("Event listener lagged by {} events", n);
                        // Continue listening
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("Event bus channel closed");
                        break;
                    }
                }
            }
        });

        // Create and return an unsubscribe function
        let unsubscribe = PyUnsubscribe { cancelled };
        Ok(unsubscribe.into_py(py))
    }

    /// Listen for a single event of a specific type
    ///
    /// Similar to listen(), but automatically unsubscribes after the first event.
    ///
    /// Args:
    ///     event_type: The type of event to listen for
    ///     callback: A callable that takes an Event object
    ///
    /// Returns:
    ///     A callable that, when called, will unsubscribe (if event hasn't fired yet)
    #[pyo3(signature = (event_type, callback))]
    fn listen_once(
        &self,
        py: Python<'_>,
        event_type: &str,
        callback: PyObject,
    ) -> PyResult<PyObject> {
        // Validate the callback is callable
        if !callback.bind(py).is_callable() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "callback must be callable",
            ));
        }

        // Subscribe to the event type
        let rx = if event_type == "*" {
            self.inner.subscribe_all()
        } else {
            self.inner.subscribe(event_type)
        };

        // Create a cancellation flag
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_clone = cancelled.clone();

        // Spawn a task to receive ONE event and call the callback
        let py_callback = callback.clone_ref(py);
        tokio::spawn(async move {
            let mut rx = rx;
            loop {
                // Check if cancelled
                if cancelled_clone.load(Ordering::Relaxed) {
                    debug!("Event listener (once) cancelled");
                    break;
                }

                // Wait for the next event
                match rx.recv().await {
                    Ok(event) => {
                        // Clone with GIL for the blocking task
                        let py_callback_clone = Python::with_gil(|py| py_callback.clone_ref(py));
                        // Call the Python callback in a blocking task
                        let _ = tokio::task::spawn_blocking(move || {
                            Python::with_gil(|py| {
                                let py_event = PyEvent::from_inner(event);
                                if let Err(e) = py_callback_clone.call1(py, (py_event,)) {
                                    error!("Error in event callback (once): {}", e);
                                }
                            })
                        })
                        .await;

                        // Only listen once, so break after first event
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Continue waiting for an event
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("Event bus channel closed");
                        break;
                    }
                }
            }
        });

        // Create and return an unsubscribe function
        let unsubscribe = PyUnsubscribe { cancelled };
        Ok(unsubscribe.into_py(py))
    }

    /// Fire an event (alias for fire, for API compatibility)
    ///
    /// This is equivalent to fire() since event firing is synchronous.
    /// For async Python usage, wrap with asyncio.to_thread() if needed:
    ///     await asyncio.to_thread(bus.async_fire, "my_event", {"key": "value"})
    #[pyo3(signature = (event_type, event_data=None, context=None))]
    fn async_fire(
        &self,
        event_type: &str,
        event_data: Option<&Bound<'_, PyDict>>,
        context: Option<PyContext>,
    ) -> PyResult<()> {
        // Just delegate to fire() since it's already synchronous
        self.fire(event_type, event_data, context)
    }

    fn __repr__(&self) -> String {
        format!("EventBus(listeners={})", self.inner.listener_count())
    }
}

/// A callable object that unsubscribes from an event when called
#[pyclass(name = "Unsubscribe")]
pub struct PyUnsubscribe {
    cancelled: Arc<AtomicBool>,
}

#[pymethods]
impl PyUnsubscribe {
    fn __call__(&self) -> PyResult<()> {
        self.cancelled.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn __repr__(&self) -> String {
        let status = if self.cancelled.load(Ordering::Relaxed) {
            "cancelled"
        } else {
            "active"
        };
        format!("Unsubscribe({})", status)
    }
}

impl PyEventBus {
    pub fn from_arc(inner: Arc<EventBus>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<EventBus> {
        &self.inner
    }
}
