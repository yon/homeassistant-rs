//! HassWrapper - hashable Home Assistant object for Python integrations

use super::bus::BusWrapper;
use super::config::ConfigWrapper;
use super::services::ServicesWrapper;
use super::states::StatesWrapper;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static HASS_INSTANCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Persistent hass.data dict
/// This ensures data set by async_setup survives to async_setup_entry
/// because integrations like Wemo store state in hass.data during async_setup
/// and expect it to still be there during async_setup_entry.
static HASS_DATA: OnceLock<Py<PyDict>> = OnceLock::new();

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
        // Use persistent data dict so data set by async_setup survives to async_setup_entry
        // This is critical for integrations like Wemo that store WemoData in hass.data["wemo"]
        // during async_setup and expect it to be there during async_setup_entry.
        let data_py = HASS_DATA.get_or_init(|| {
            Python::with_gil(|py| {
                let dict = PyDict::new_bound(py);
                // Add integrations dict that entities expect
                let integrations = PyDict::new_bound(py);
                dict.set_item("integrations", &integrations)
                    .expect("Failed to set integrations");
                dict.unbind()
            })
        });
        // Clone the Py<PyDict> so we can store it in this wrapper while the static keeps its reference
        let data = data_py.clone_ref(py);
        let data_bound = data.bind(py);

        // Initialize the network singleton - many components expect this to exist
        // The network component stores a Network object in hass.data["network"]
        // We need to initialize it so components can access network adapters
        // We create a minimal Network object with real adapter info from ifaddr
        // Only runs once - subsequent calls will skip if network already exists
        let init_network_code = r#"
def init_network(hass_data):
    # Skip if already initialized (persistent data dict is reused)
    if "network" in hass_data:
        return

    try:
        import ifaddr
        from ipaddress import ip_address

        # Get source IP for default route detection
        def get_source_ip(target_ip):
            import socket
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                s.connect((target_ip, 80))
                source = s.getsockname()[0]
                s.close()
                return source
            except Exception:
                return None

        source_ip = get_source_ip("224.0.0.251")  # mDNS target
        source_ip_address = ip_address(source_ip) if source_ip else None

        # Convert ifaddr adapters to HA format
        adapters = []
        for adapter in ifaddr.get_adapters():
            ipv4_list = []
            ipv6_list = []
            for ip in adapter.ips:
                if isinstance(ip.ip, str):
                    # IPv4
                    ipv4_list.append({
                        "address": ip.ip,
                        "network_prefix": ip.network_prefix,
                    })
                elif isinstance(ip.ip, tuple):
                    # IPv6
                    ipv6_list.append({
                        "address": ip.ip[0],
                        "flowinfo": ip.ip[1],
                        "scope_id": ip.ip[2],
                        "network_prefix": ip.network_prefix,
                    })

            is_default = False
            if source_ip_address and ipv4_list:
                for ipv4 in ipv4_list:
                    if ipv4["address"] == str(source_ip_address):
                        is_default = True
                        break

            adapters.append({
                "name": adapter.nice_name,
                "index": getattr(adapter, 'index', None),
                "enabled": is_default,  # Only enable the default adapter
                "auto": is_default,
                "default": is_default,
                "ipv4": ipv4_list,
                "ipv6": ipv6_list,
            })

        # Create a minimal Network-like object
        class MinimalNetwork:
            def __init__(self, adapters_list):
                self.adapters = adapters_list
                self._data = {}

            @property
            def configured_adapters(self):
                return []

        hass_data["network"] = MinimalNetwork(adapters)
    except Exception as e:
        import sys
        print(f"Warning: Failed to initialize network component: {e}", file=sys.stderr)
        # Fallback: create empty network
        class MinimalNetwork:
            def __init__(self):
                self.adapters = []
                self._data = {}
            @property
            def configured_adapters(self):
                return []
        hass_data["network"] = MinimalNetwork()
"#;
        let globals = PyDict::new_bound(py);
        py.run_bound(init_network_code, Some(&globals), None)?;

        let init_fn = globals.get_item("init_network")?.unwrap();
        let _ = init_fn.call1((data_bound.clone(),));

        Ok(Self {
            instance_id: HASS_INSTANCE_COUNTER.fetch_add(1, Ordering::SeqCst),
            bus,
            states,
            services,
            config,
            data,
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

    /// Check if Home Assistant is running
    ///
    /// Returns true when HA is in starting or running state.
    /// We're always running while serving requests.
    #[getter]
    fn is_running(&self) -> bool {
        true
    }

    /// Check if Home Assistant is stopping
    ///
    /// Returns true when HA is in stopping or final_write state.
    /// We return false since we're serving requests.
    #[getter]
    fn is_stopping(&self) -> bool {
        false
    }

    /// Get the current state of Home Assistant
    ///
    /// Returns CoreState.running since we're serving requests.
    #[getter]
    fn state(&self, py: Python<'_>) -> PyResult<PyObject> {
        // Import CoreState enum from homeassistant.core
        let ha_core = py.import_bound("homeassistant.core")?;
        let core_state = ha_core.getattr("CoreState")?;
        let running = core_state.getattr("running")?;
        Ok(running.unbind())
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

    /// Create a task from a coroutine
    ///
    /// This wraps asyncio.create_task to match HA's API.
    ///
    /// # Arguments
    /// * `target` - The coroutine to wrap in a task
    /// * `name` - Optional name for the task
    /// * `eager_start` - Whether to start the task eagerly
    ///
    /// # Returns
    /// The created asyncio task
    #[pyo3(signature = (target, name=None, eager_start=false))]
    fn async_create_task<'py>(
        &self,
        py: Python<'py>,
        target: PyObject,
        name: Option<String>,
        eager_start: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let code = r#"
import asyncio
import logging

_LOGGER = logging.getLogger(__name__)

def create_task(coro, name=None, eager_start=False):
    """Create an async task."""
    try:
        loop = asyncio.get_running_loop()
        task = loop.create_task(coro, name=name)
        if eager_start and hasattr(task, '__await__'):
            # Try to start it eagerly by stepping through the coroutine once
            try:
                task.__await__().__next__()
            except StopIteration:
                pass
            except Exception:
                pass  # Task will complete on its own
        _LOGGER.debug(f"Created task: {name or 'unnamed'}")
        return task
    except RuntimeError:
        # No running loop - schedule it for later
        _LOGGER.warning(f"No running loop for task: {name or 'unnamed'}")
        return asyncio.ensure_future(coro)
"#;
        let globals = pyo3::types::PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;

        let create_fn = globals.get_item("create_task")?.unwrap();

        // Build kwargs
        let kwargs = pyo3::types::PyDict::new_bound(py);
        if let Some(n) = name {
            kwargs.set_item("name", n)?;
        }
        kwargs.set_item("eager_start", eager_start)?;

        let task = create_fn.call((target,), Some(&kwargs))?;
        Ok(task)
    }

    /// Create a background task tied to the HomeAssistant lifecycle
    ///
    /// Background tasks:
    /// - Will not block startup
    /// - Will be automatically cancelled on shutdown
    /// - Calls to async_block_till_done will not wait for completion
    ///
    /// # Arguments
    /// * `target` - The coroutine to wrap in a background task
    /// * `name` - Name for the task
    /// * `eager_start` - Whether to start the task eagerly (default true)
    ///
    /// # Returns
    /// The created asyncio task
    #[pyo3(signature = (target, name, eager_start=true))]
    fn async_create_background_task<'py>(
        &self,
        py: Python<'py>,
        target: PyObject,
        name: String,
        eager_start: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Log for debugging
        tracing::info!("Creating background task: {}", name);

        // For now, background tasks are just regular tasks
        // In a full implementation, we'd track them and cancel on shutdown
        let result = self.async_create_task(py, target, Some(name.clone()), eager_start);

        match &result {
            Ok(_) => tracing::info!("Background task '{}' created successfully", name),
            Err(e) => tracing::error!("Failed to create background task '{}': {:?}", name, e),
        }

        result
    }

    /// Run a HassJob from within the event loop
    ///
    /// HassJob is a wrapper around a callable with job_type indicating how to run it:
    /// - Callback: Run synchronously
    /// - Coroutinefunction: Create a task for the coroutine
    /// - Executor: Run in executor (not implemented yet)
    ///
    /// # Arguments
    /// * `hassjob` - The HassJob to run
    /// * `args` - Arguments to pass to the job target
    /// * `background` - Whether to run as a background task (default false)
    ///
    /// # Returns
    /// The task if created, or None for callbacks
    #[pyo3(signature = (hassjob, *args, background=false))]
    fn async_run_hass_job<'py>(
        &self,
        py: Python<'py>,
        hassjob: PyObject,
        args: &Bound<'py, PyTuple>,
        background: bool,
    ) -> PyResult<PyObject> {
        // Import HassJobType enum
        let core_module = py.import_bound("homeassistant.core")?;
        let hass_job_type = core_module.getattr("HassJobType")?;
        let callback_type = hass_job_type.getattr("Callback")?;

        // Get job type and target from hassjob
        let job_type = hassjob.getattr(py, "job_type")?;
        let target = hassjob.getattr(py, "target")?;

        // Check if it's a callback type - run synchronously
        if job_type.bind(py).eq(&callback_type)? {
            // Call the target directly with args
            let _ = target.call_bound(py, args, None)?;
            return Ok(py.None());
        }

        // For coroutine types, create a task
        // Call the target to get the coroutine
        let coro = target.call_bound(py, args, None)?;

        // Create a task for the coroutine
        if background {
            self.async_create_background_task(py, coro, "hass_job".to_string(), true)
                .map(|t| t.unbind())
        } else {
            self.async_create_task(py, coro, Some("hass_job".to_string()), false)
                .map(|t| t.unbind())
        }
    }
}
