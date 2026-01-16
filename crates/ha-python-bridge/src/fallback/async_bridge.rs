//! Async bridge between Tokio and Python asyncio
//!
//! Provides utilities for calling Python async functions from Rust
//! and handling the asyncio event loop.

use super::errors::{FallbackError, FallbackResult};
use pyo3::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::oneshot;

/// Bridge for running Python async code from Rust
pub struct AsyncBridge {
    /// Python asyncio event loop (stored as PyObject for thread safety)
    event_loop: Option<PyObject>,
}

impl AsyncBridge {
    /// Create a new async bridge
    pub fn new() -> FallbackResult<Self> {
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
            })
        })
    }

    /// Run a Python coroutine to completion
    pub fn run_coroutine<T>(&self, coro: PyObject) -> FallbackResult<T>
    where
        T: for<'py> FromPyObject<'py>,
    {
        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| FallbackError::AsyncBridge("No event loop".to_string()))?;

            let result = event_loop
                .bind(py)
                .call_method1("run_until_complete", (coro,))?;

            result.extract().map_err(FallbackError::from)
        })
    }

    /// Run a Python coroutine and return the result as PyObject
    pub fn run_coroutine_py(&self, coro: PyObject) -> FallbackResult<PyObject> {
        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| FallbackError::AsyncBridge("No event loop".to_string()))?;

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
    ) -> FallbackResult<PyObject> {
        Python::with_gil(|py| {
            let bound = obj.bind(py);
            let coro = bound.call_method1(method, args)?;
            Ok(coro.unbind())
        })
    }

    /// Schedule a callback to run in the asyncio event loop
    pub fn call_soon(&self, callback: PyObject, args: PyObject) -> FallbackResult<()> {
        Python::with_gil(|py| {
            let event_loop = self
                .event_loop
                .as_ref()
                .ok_or_else(|| FallbackError::AsyncBridge("No event loop".to_string()))?;

            event_loop
                .bind(py)
                .call_method1("call_soon", (callback, args))?;

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
    result: Option<oneshot::Receiver<FallbackResult<PyObject>>>,
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
    fn spawn_blocking(&mut self) -> oneshot::Receiver<FallbackResult<PyObject>> {
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
    type Output = FallbackResult<PyObject>;

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
            Poll::Ready(Err(_)) => Poll::Ready(Err(FallbackError::AsyncBridge(
                "Channel closed".to_string(),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Helper to run Python async code from Tokio
pub async fn run_python_async<F, T>(f: F) -> FallbackResult<T>
where
    F: FnOnce(Python<'_>) -> PyResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || Python::with_gil(|py| f(py).map_err(FallbackError::from)))
        .await
        .map_err(|e| FallbackError::AsyncBridge(e.to_string()))?
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
                        let _ = loop_obj.call_method1("call_soon_threadsafe", (set_result, py_result));
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
