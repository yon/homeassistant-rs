//! HassWrapper - hashable Home Assistant object for Python integrations

use super::bus::BusWrapper;
use super::config::ConfigWrapper;
use super::services::ServicesWrapper;
use super::states::StatesWrapper;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::sync::atomic::{AtomicU64, Ordering};

static HASS_INSTANCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Python wrapper for the Home Assistant object
///
/// This provides a hashable HomeAssistant-like object that can be used as
/// a dictionary key or set element in Python code. SimpleNamespace isn't
/// hashable, so we need this custom class.
#[pyclass(name = "HomeAssistant")]
pub struct HassWrapper {
    /// Unique instance ID for hashing
    instance_id: u64,
    /// Event bus
    #[pyo3(get)]
    pub bus: Py<BusWrapper>,
    /// State machine
    #[pyo3(get)]
    pub states: Py<StatesWrapper>,
    /// Service registry
    #[pyo3(get)]
    pub services: Py<ServicesWrapper>,
    /// Configuration
    #[pyo3(get)]
    pub config: Py<ConfigWrapper>,
    /// Data storage dict
    data: Py<PyDict>,
    /// Config entries wrapper
    config_entries: PyObject,
    /// Helpers namespace
    helpers: PyObject,
    /// Event loop
    loop_: PyObject,
    /// Loop thread ID
    loop_thread_id: PyObject,
    /// async_create_task function
    async_create_task: PyObject,
    /// timeout context manager factory
    timeout: PyObject,
}

impl HassWrapper {
    pub fn new(
        py: Python<'_>,
        bus: Py<BusWrapper>,
        states: Py<StatesWrapper>,
        services: Py<ServicesWrapper>,
        config: Py<ConfigWrapper>,
        config_entries: PyObject,
        helpers: PyObject,
        loop_: PyObject,
        loop_thread_id: PyObject,
        async_create_task: PyObject,
        timeout: PyObject,
    ) -> PyResult<Self> {
        let data = PyDict::new_bound(py);
        // Add integrations dict that entities expect
        let integrations = PyDict::new_bound(py);
        data.set_item("integrations", &integrations)?;

        Ok(Self {
            instance_id: HASS_INSTANCE_COUNTER.fetch_add(1, Ordering::SeqCst),
            bus,
            states,
            services,
            config,
            data: data.unbind(),
            config_entries,
            helpers,
            loop_,
            loop_thread_id,
            async_create_task,
            timeout,
        })
    }
}

#[pymethods]
impl HassWrapper {
    /// Get the data dict
    #[getter]
    fn data(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        Ok(self.data.clone_ref(py))
    }

    /// Get config_entries
    #[getter]
    fn config_entries(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.config_entries.clone_ref(py))
    }

    /// Get helpers
    #[getter]
    fn helpers(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.helpers.clone_ref(py))
    }

    /// Get the event loop
    #[pyo3(name = "loop")]
    #[getter]
    fn get_loop(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.loop_.clone_ref(py))
    }

    /// Get the loop thread ID
    #[getter]
    fn get_loop_thread_id(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.loop_thread_id.clone_ref(py))
    }

    /// Get async_create_task
    #[getter]
    fn get_async_create_task(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.async_create_task.clone_ref(py))
    }

    /// Get timeout factory
    #[getter]
    fn get_timeout(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.timeout.clone_ref(py))
    }

    /// Verify we're running in the event loop thread
    ///
    /// In HA, this raises an error if called from wrong thread.
    /// We just no-op since we're always in the same thread context.
    fn verify_event_loop_thread(&self, _func_name: &str) {
        // No-op - we're always running in the right thread context
    }

    /// Hash based on instance ID (identity-based hashing)
    fn __hash__(&self) -> u64 {
        self.instance_id
    }

    /// Equality based on instance ID (identity-based equality)
    fn __eq__(&self, other: &HassWrapper) -> bool {
        self.instance_id == other.instance_id
    }

    /// String representation
    fn __repr__(&self) -> String {
        format!("<HomeAssistant instance_id={}>", self.instance_id)
    }

    /// Run a blocking function in the executor thread pool
    ///
    /// This is the key method that config flows need to run blocking I/O
    /// (like network requests, file operations, etc.) without blocking the event loop.
    ///
    /// # Arguments
    /// * `func` - The blocking function to run
    /// * `args` - Optional positional arguments to pass to the function
    ///
    /// # Returns
    /// A coroutine that will return the result of the function when awaited.
    #[pyo3(signature = (func, *args))]
    fn async_add_executor_job<'py>(
        &self,
        py: Python<'py>,
        func: PyObject,
        args: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Create a coroutine that runs the function in the executor
        let code = r#"
import asyncio
import concurrent.futures

# Create a module-level executor if not already created
if not hasattr(asyncio, '_ha_executor'):
    asyncio._ha_executor = concurrent.futures.ThreadPoolExecutor(max_workers=8)

async def _run_in_executor(func, *args):
    """Run a blocking function in the executor."""
    loop = asyncio.get_running_loop()
    return await loop.run_in_executor(asyncio._ha_executor, func, *args)
"#;
        let globals = pyo3::types::PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;

        let run_fn = globals.get_item("_run_in_executor")?.unwrap();

        // Build the argument tuple: (func, *args)
        // Collect into a Vec first since chain() doesn't implement ExactSizeIterator
        let call_args: Vec<_> = std::iter::once(func.bind(py).clone())
            .chain(args.iter())
            .collect();
        let call_args = PyTuple::new_bound(py, call_args);

        // Call the async function to get the coroutine
        let coro = run_fn.call1(call_args)?;
        Ok(coro)
    }

    /// Run a blocking function in the executor (alternate signature with target)
    ///
    /// Some code passes the function as target=func, so we support that too.
    #[pyo3(signature = (target, *args))]
    fn add_executor_job<'py>(
        &self,
        py: Python<'py>,
        target: PyObject,
        args: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Delegate to async_add_executor_job
        self.async_add_executor_job(py, target, args)
    }

    /// Run an import in the executor
    ///
    /// This is used by HA's loader to import Python modules in a thread pool.
    /// It's identical to async_add_executor_job but named specifically for imports.
    ///
    /// # Arguments
    /// * `func` - The import function to run (typically importlib.import_module)
    /// * `args` - Arguments to pass to the function
    ///
    /// # Returns
    /// A coroutine that will return the imported module when awaited.
    #[pyo3(signature = (func, *args))]
    fn async_add_import_executor_job<'py>(
        &self,
        py: Python<'py>,
        func: PyObject,
        args: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Delegate to async_add_executor_job - imports are just another blocking operation
        self.async_add_executor_job(py, func, args)
    }
}
