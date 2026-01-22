//! Async bridge between Tokio and Python asyncio
//!
//! Provides utilities for calling Python async functions from Rust
//! and handling the asyncio event loop.

use super::errors::{PyBridgeError, PyBridgeResult};
use pyo3::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread::JoinHandle;
use tokio::sync::oneshot;

/// Bridge for running Python async code from Rust
pub struct AsyncBridge {
    /// Python asyncio event loop (stored as PyObject for thread safety)
    event_loop: Option<PyObject>,
    /// Flag to signal background loop to stop
    stop_flag: Arc<AtomicBool>,
    /// Handle to the background loop thread (Mutex for interior mutability)
    background_thread: Mutex<Option<JoinHandle<()>>>,
}

impl AsyncBridge {
    /// Create a new async bridge
    pub fn new() -> PyBridgeResult<Self> {
        Python::with_gil(|py| {
            // Get or create an asyncio event loop
            let asyncio = py.import_bound("asyncio")?;

            // Try to get the running loop, or create a new one
            let loop_result = asyncio.call_method0("get_event_loop");
            let event_loop: PyObject = match loop_result {
                Ok(loop_obj) => loop_obj.unbind(),
                Err(_) => {
                    // No running loop, create a new one
                    let new_loop = asyncio.call_method0("new_event_loop")?;
                    asyncio.call_method1("set_event_loop", (&new_loop,))?;
                    new_loop.unbind()
                }
            };

            Ok(Self {
                event_loop: Some(event_loop),
                stop_flag: Arc::new(AtomicBool::new(false)),
                background_thread: Mutex::new(None),
            })
        })
    }

    /// Start running the event loop in a background thread
    ///
    /// This allows scheduled tasks (like `async_call_later`) to execute.
    /// The loop runs until `stop_background_loop` is called.
    pub fn start_background_loop(&self) -> PyBridgeResult<()> {
        let mut thread_guard = self.background_thread.lock().unwrap();
        if thread_guard.is_some() {
            return Ok(()); // Already running
        }

        let event_loop = self
            .event_loop
            .as_ref()
            .ok_or_else(|| PyBridgeError::AsyncBridge("No event loop".to_string()))?;

        let loop_clone = Python::with_gil(|py| event_loop.clone_ref(py));
        let stop_flag = self.stop_flag.clone();

        // Reset stop flag
        stop_flag.store(false, Ordering::SeqCst);

        let handle = std::thread::spawn(move || {
            Python::with_gil(|py| {
                // Set this thread's event loop
                if let Ok(asyncio) = py.import_bound("asyncio") {
                    let _ = asyncio.call_method1("set_event_loop", (&loop_clone,));
                }

                // Run the event loop using run_forever() which properly processes all tasks
                // We'll stop it from another thread using call_soon_threadsafe(loop.stop)
                let run_code = r#"
import asyncio
import sys

def _run_event_loop(loop, stop_flag_checker):
    """Run the event loop, processing all scheduled tasks.

    Uses a coroutine that periodically checks the stop flag and stops the loop.
    This ensures all tasks (including those scheduled with call_later) are processed.
    """
    async def _stop_checker():
        """Check stop flag every 100ms and stop loop if set."""
        while True:
            await asyncio.sleep(0.1)
            if stop_flag_checker():
                loop.stop()
                return

    # Schedule the stop checker
    loop.create_task(_stop_checker())

    # Run forever - this properly processes all scheduled tasks
    try:
        loop.run_forever()
    except Exception as e:
        print(f"Event loop error: {e}", file=sys.stderr)
    finally:
        # Clean up pending tasks
        try:
            pending = asyncio.all_tasks(loop)
            for task in pending:
                task.cancel()
            loop.run_until_complete(asyncio.gather(*pending, return_exceptions=True))
        except Exception:
            pass
"#;
                let globals = pyo3::types::PyDict::new_bound(py);
                let _ = py.run_bound(run_code, Some(&globals), None);

                // Create a Python function that checks the stop flag
                let stop_flag_for_py = stop_flag.clone();
                let stop_checker = pyo3::types::PyCFunction::new_closure_bound(
                    py,
                    None,
                    None,
                    move |_args: &Bound<'_, pyo3::types::PyTuple>,
                          _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>|
                          -> bool { stop_flag_for_py.load(Ordering::SeqCst) },
                )
                .expect("Failed to create stop checker closure");

                // Get the run function and call it
                if let Some(run_fn) = globals.get_item("_run_event_loop").ok().flatten() {
                    if let Err(e) = run_fn.call1((loop_clone.bind(py), stop_checker)) {
                        tracing::warn!("Event loop run error: {:?}", e);
                    }
                }

                tracing::debug!("Background event loop thread exiting");
            });
        });

        *thread_guard = Some(handle);
        tracing::info!("Started Python event loop background thread");
        Ok(())
    }

    /// Stop the background event loop
    pub fn stop_background_loop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);

        let mut thread_guard = self.background_thread.lock().unwrap();
        if let Some(handle) = thread_guard.take() {
            // Give the thread time to stop gracefully
            std::thread::sleep(std::time::Duration::from_millis(200));
            // If it's still running, it will stop on next iteration
            if !handle.is_finished() {
                tracing::debug!("Waiting for background loop thread to finish");
            }
            let _ = handle.join();
            tracing::info!("Stopped Python event loop background thread");
        }
    }

    /// Run a Python coroutine to completion
    pub fn run_coroutine<T>(&self, coro: PyObject) -> PyBridgeResult<T>
    where
        T: for<'py> FromPyObject<'py>,
    {
        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| PyBridgeError::AsyncBridge("No event loop".to_string()))?;

            // Ensure our event loop is set as the current loop before running
            // This is critical for libraries that create Futures during setup
            let asyncio = py.import_bound("asyncio")?;
            asyncio.call_method1("set_event_loop", (event_loop,))?;

            let result = event_loop
                .bind(py)
                .call_method1("run_until_complete", (coro,))?;

            result.extract().map_err(PyBridgeError::from)
        })
    }

    /// Run a Python coroutine and return the result as PyObject
    pub fn run_coroutine_py(&self, coro: PyObject) -> PyBridgeResult<PyObject> {
        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| PyBridgeError::AsyncBridge("No event loop".to_string()))?;

            // Ensure our event loop is set as the current loop before running
            let asyncio = py.import_bound("asyncio")?;
            asyncio.call_method1("set_event_loop", (event_loop,))?;

            let result = event_loop
                .bind(py)
                .call_method1("run_until_complete", (coro,))?;

            Ok(result.unbind())
        })
    }

    /// Create a Python coroutine from an async function call
    pub fn call_async(
        &self,
        obj: &PyObject,
        method: &str,
        args: impl IntoPy<Py<pyo3::types::PyTuple>>,
    ) -> PyBridgeResult<PyObject> {
        Python::with_gil(|py| {
            let bound = obj.bind(py);
            let coro = bound.call_method1(method, args)?;
            Ok(coro.unbind())
        })
    }

    /// Schedule a callback to run in the asyncio event loop
    pub fn call_soon(&self, callback: PyObject, args: PyObject) -> PyBridgeResult<()> {
        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| PyBridgeError::AsyncBridge("No event loop".to_string()))?;

            event_loop
                .bind(py)
                .call_method1("call_soon", (callback, args))?;

            Ok(())
        })
    }

    /// Process pending tasks on the event loop
    ///
    /// This runs `asyncio.sleep(0)` to yield to any pending tasks that were
    /// scheduled but haven't had a chance to run yet. It also runs a short
    /// timeout to allow blocking operations in the executor to complete.
    ///
    /// Call this after running coroutines that may have scheduled background tasks.
    pub fn run_pending_tasks(&self, timeout_secs: f64) -> PyBridgeResult<()> {
        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| PyBridgeError::AsyncBridge("No event loop".to_string()))?;

            // Run sleep(timeout) to allow pending tasks to execute
            // This gives background tasks time to make progress
            let code = format!(
                r#"
import asyncio

async def _process_pending(timeout):
    """Run pending tasks and give them time to execute."""
    # Yield to other tasks first
    await asyncio.sleep(0)

    # Get all tasks
    tasks = [t for t in asyncio.all_tasks() if not t.done()]

    if tasks:
        # Give tasks some time to run
        try:
            await asyncio.wait(tasks, timeout=timeout)
        except asyncio.TimeoutError:
            pass

    # Final yield
    await asyncio.sleep(0)
"#
            );

            let globals = pyo3::types::PyDict::new_bound(py);
            py.run_bound(&code, Some(&globals), None)?;

            let process_fn = globals.get_item("_process_pending")?.unwrap();
            let coro = process_fn.call1((timeout_secs,))?;

            // Set our event loop as current and run
            let asyncio = py.import_bound("asyncio")?;
            asyncio.call_method1("set_event_loop", (event_loop,))?;

            event_loop
                .bind(py)
                .call_method1("run_until_complete", (coro,))?;

            Ok(())
        })
    }

    /// Check if the event loop is running
    pub fn is_running(&self) -> bool {
        Python::with_gil(|py| {
            self.event_loop
                .as_ref()
                .and_then(|loop_obj| {
                    loop_obj
                        .bind(py)
                        .call_method0("is_running")
                        .ok()
                        .and_then(|r| r.extract().ok())
                })
                .unwrap_or(false)
        })
    }

    /// Get a reference to the event loop (cloned for Python use)
    pub fn get_event_loop(&self, py: Python<'_>) -> Option<PyObject> {
        self.event_loop
            .as_ref()
            .map(|loop_obj| loop_obj.clone_ref(py))
    }

    /// Check if background loop is running
    pub fn is_background_loop_running(&self) -> bool {
        self.background_thread
            .lock()
            .unwrap()
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }
}

impl Drop for AsyncBridge {
    fn drop(&mut self) {
        self.stop_background_loop();
    }
}

impl Default for AsyncBridge {
    fn default() -> Self {
        Self::new().expect("Failed to create async bridge")
    }
}

/// A future that wraps a Python coroutine
pub struct PyFuture {
    coro: PyObject,
    bridge: Arc<AsyncBridge>,
    result: Option<oneshot::Receiver<PyBridgeResult<PyObject>>>,
}

impl PyFuture {
    /// Create a new PyFuture from a Python coroutine
    pub fn new(coro: PyObject, bridge: Arc<AsyncBridge>) -> Self {
        Self {
            coro,
            bridge,
            result: None,
        }
    }

    /// Spawn the coroutine on a blocking thread
    fn spawn_blocking(&mut self) -> oneshot::Receiver<PyBridgeResult<PyObject>> {
        let (tx, rx) = oneshot::channel();
        let coro = Python::with_gil(|py| self.coro.clone_ref(py));
        let bridge = self.bridge.clone();

        std::thread::spawn(move || {
            let result = bridge.run_coroutine_py(coro);
            let _ = tx.send(result);
        });

        rx
    }
}

impl Future for PyFuture {
    type Output = PyBridgeResult<PyObject>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // If we haven't spawned yet, do so now
        if self.result.is_none() {
            let rx = self.spawn_blocking();
            self.result = Some(rx);
        }

        // Check if the result is ready
        let rx = self.result.as_mut().unwrap();
        match Pin::new(rx).poll(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => Poll::Ready(Err(PyBridgeError::AsyncBridge(
                "Channel closed".to_string(),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Helper to run Python async code from Tokio
pub async fn run_python_async<F, T>(f: F) -> PyBridgeResult<T>
where
    F: FnOnce(Python<'_>) -> PyResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || Python::with_gil(|py| f(py).map_err(PyBridgeError::from)))
        .await
        .map_err(|e| PyBridgeError::AsyncBridge(e.to_string()))?
}

/// Create a Python awaitable that resolves when a Rust future completes
pub fn rust_future_to_python<F, T>(py: Python<'_>, future: F) -> PyResult<PyObject>
where
    F: Future<Output = T> + Send + 'static,
    T: IntoPy<PyObject> + Send + 'static,
{
    let asyncio = py.import_bound("asyncio")?;
    let loop_obj = asyncio.call_method0("get_event_loop")?;

    // Create a Python Future object
    let py_future = loop_obj.call_method0("create_future")?;
    let py_future_clone = py_future.clone().unbind();

    // Spawn the Rust future on Tokio
    let handle = tokio::runtime::Handle::try_current()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    handle.spawn(async move {
        let result = future.await;

        // Set the result on the Python future
        Python::with_gil(|py| {
            let py_future = py_future_clone.bind(py);
            let py_result = result.into_py(py);

            // Get the event loop and schedule setting the result
            if let Ok(asyncio) = py.import_bound("asyncio") {
                if let Ok(loop_obj) = asyncio.call_method0("get_event_loop") {
                    if let Ok(set_result) = py_future.getattr("set_result") {
                        let _ =
                            loop_obj.call_method1("call_soon_threadsafe", (set_result, py_result));
                    }
                }
            }
        });
    });

    Ok(py_future.unbind())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_bridge_creation() {
        let bridge = AsyncBridge::new();
        assert!(bridge.is_ok());
    }

    #[tokio::test]
    async fn test_run_python_async() {
        let result = run_python_async(|py| {
            let sys = py.import_bound("sys")?;
            let version: String = sys.getattr("version")?.extract()?;
            Ok(version)
        })
        .await;

        assert!(result.is_ok());
        assert!(result.unwrap().starts_with("3."));
    }
}
