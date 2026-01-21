//! Config Flow Manager
//!
//! Manages configuration flows for Python integrations. This allows users
//! to add and configure integrations through the frontend UI.

use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
use ulid::Ulid;

use ha_api::config_flow::{ConfigFlowHandler, FlowResult, FormField};
use ha_api::ApplicationCredentialsStore;
use ha_event_bus::EventBus;
use ha_registries::Registries;
use ha_service_registry::ServiceRegistry;
use ha_state_machine::StateMachine;

use super::async_bridge::AsyncBridge;
use super::hass_wrapper::create_hass_wrapper_for_config_flow;

/// Active flow state
struct ActiveFlow {
    /// Integration domain
    handler: String,
    /// Python flow handler instance
    flow_instance: PyObject,
    /// Current step ID
    current_step: String,
}

/// Manages active configuration flows
///
/// This struct holds references to Rust core objects and handles all Python
/// interop internally via PyO3. Callers don't need to deal with PyO3 types.
///
/// Run a flow step on a blocking thread to avoid blocking Tokio workers.
/// This is critical for long-running Python async operations like network I/O.
///
/// This function creates a NEW event loop for each blocking call rather than
/// using a shared one. This avoids "event loop already running" errors when
/// called from spawn_blocking.
fn run_flow_step_blocking(
    _async_bridge: &Arc<AsyncBridge>,
    flow_instance: &PyObject,
    step: &str,
    user_input: Option<serde_json::Value>,
    flow_id: &str,
    handler: &str,
    _hass: &PyObject,
) -> Result<FlowResult, String> {
    Python::with_gil(|py| {
        let method_name = format!("async_step_{}", step);
        debug!("Calling {} on flow (blocking thread)", method_name);

        // Convert user_input to Python dict
        let py_input = match user_input {
            Some(input) => json_to_pyobject(py, &input),
            None => None,
        };

        // Get the method
        let flow_bound = flow_instance.bind(py);
        let method = flow_bound
            .getattr(method_name.as_str())
            .map_err(|e| format!("Flow has no method {}: {}", method_name, e))?;

        // Call the method (it's async, so we get a coroutine)
        let coro = if let Some(input) = py_input {
            method
                .call1((input,))
                .map_err(|e| format!("Failed to call {}: {}", method_name, e))?
        } else {
            method
                .call1((py.None(),))
                .map_err(|e| format!("Failed to call {}: {}", method_name, e))?
        };

        // Create a NEW event loop for this blocking call
        // This avoids "event loop already running" errors since spawn_blocking
        // threads don't have an event loop set up, and we can't use a shared one
        let asyncio = py
            .import_bound("asyncio")
            .map_err(|e| format!("Failed to import asyncio: {}", e))?;

        let new_loop = asyncio
            .call_method0("new_event_loop")
            .map_err(|e| format!("Failed to create event loop: {}", e))?;

        // Set it as the current loop for this thread
        asyncio
            .call_method1("set_event_loop", (&new_loop,))
            .map_err(|e| format!("Failed to set event loop: {}", e))?;

        // Run the coroutine to completion
        let result = new_loop
            .call_method1("run_until_complete", (&coro,))
            .map_err(|e| format!("Failed to run coroutine: {}", e))?;

        // Close the loop when done
        let _ = new_loop.call_method0("close");

        // Convert Python result to FlowResult
        convert_flow_result_standalone(py, &result, flow_id, handler)
    })
}

/// Convert a Python flow result dict to FlowResult (standalone version)
fn convert_flow_result_standalone(
    py: Python<'_>,
    result: &pyo3::Bound<'_, pyo3::PyAny>,
    flow_id: &str,
    handler: &str,
) -> Result<FlowResult, String> {
    let result_type = result
        .get_item("type")
        .ok()
        .and_then(|t| {
            if let Ok(val) = t.getattr("value") {
                val.extract::<String>().ok()
            } else {
                t.extract::<String>().ok()
            }
        })
        .unwrap_or_else(|| "form".to_string());

    debug!("Flow result type: {}", result_type);

    let step_id = result
        .get_item("step_id")
        .ok()
        .and_then(|s| s.extract::<String>().ok());

    let errors = result.get_item("errors").ok().and_then(|e| {
        if e.is_none() {
            return None;
        }
        let dict = e.downcast::<PyDict>().ok()?;
        let mut map = HashMap::new();
        for (k, v) in dict.iter() {
            if let (Ok(key), Ok(val)) = (k.extract::<String>(), v.extract::<String>()) {
                map.insert(key, val);
            }
        }
        if map.is_empty() {
            None
        } else {
            Some(map)
        }
    });

    let description_placeholders = result
        .get_item("description_placeholders")
        .ok()
        .and_then(|e| {
            if e.is_none() {
                return None;
            }
            let dict = e.downcast::<PyDict>().ok()?;
            let mut map = HashMap::new();
            for (k, v) in dict.iter() {
                if let (Ok(key), Ok(val)) = (k.extract::<String>(), v.extract::<String>()) {
                    map.insert(key, val);
                }
            }
            if map.is_empty() {
                None
            } else {
                Some(map)
            }
        });

    let data_schema = result
        .get_item("data_schema")
        .ok()
        .and_then(|schema| {
            if schema.is_none() {
                return None;
            }
            convert_schema_to_fields_standalone(&schema)
        })
        .unwrap_or_default();

    let title = result
        .get_item("title")
        .ok()
        .and_then(|t| t.extract::<String>().ok());

    let reason = result
        .get_item("reason")
        .ok()
        .and_then(|r| r.extract::<String>().ok());

    let entry_result = if result_type == "create_entry" {
        if let Ok(data) = result.get_item("data") {
            if let Ok(dict) = data.downcast::<PyDict>() {
                let mut map = serde_json::Map::new();
                for (k, v) in dict.iter() {
                    if let Ok(key) = k.extract::<String>() {
                        let val = pyobject_to_json(py, &v);
                        map.insert(key, val);
                    }
                }
                Some(serde_json::Value::Object(map))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let last_step = result.get_item("last_step").ok().and_then(|v| {
        if v.is_none() {
            None
        } else {
            v.extract::<bool>().ok()
        }
    });

    let preview = result.get_item("preview").ok().and_then(|v| {
        if v.is_none() {
            None
        } else {
            v.extract::<String>().ok()
        }
    });

    Ok(FlowResult {
        flow_id: flow_id.to_string(),
        handler: handler.to_string(),
        result_type,
        step_id,
        data_schema,
        errors,
        description_placeholders,
        title,
        reason,
        version: Some(1),
        minor_version: Some(1),
        result: entry_result,
        last_step,
        preview,
    })
}

/// Convert a voluptuous schema to form fields (standalone version)
fn convert_schema_to_fields_standalone(
    schema: &pyo3::Bound<'_, pyo3::PyAny>,
) -> Option<Vec<FormField>> {
    let schema_dict = schema.getattr("schema").ok()?;
    let dict = schema_dict.downcast::<PyDict>().ok()?;

    let mut fields = Vec::new();

    for (key, _value) in dict.iter() {
        let field_name = if let Ok(schema_attr) = key.getattr("schema") {
            schema_attr.extract::<String>().ok()
        } else {
            key.extract::<String>().ok()
        };

        if let Some(name) = field_name {
            let type_name = key
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_default();
            let required = type_name.contains("Required");

            fields.push(FormField {
                name,
                field_type: "string".to_string(),
                required: Some(required),
                default: None,
            });
        }
    }

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

pub struct ConfigFlowManager {
    /// Active flows: flow_id -> flow state
    flows: RwLock<HashMap<String, ActiveFlow>>,
    /// Event bus reference
    event_bus: Arc<EventBus>,
    /// State machine reference
    state_machine: Arc<StateMachine>,
    /// Service registry reference
    service_registry: Arc<ServiceRegistry>,
    /// Registries reference
    registries: Arc<Registries>,
    /// Config directory path
    config_dir: Option<PathBuf>,
    /// Async bridge for running Python coroutines
    async_bridge: Arc<AsyncBridge>,
    /// Application credentials store for OAuth integrations
    application_credentials: ApplicationCredentialsStore,
}

impl ConfigFlowManager {
    /// Create a new ConfigFlowManager
    pub fn new(
        event_bus: Arc<EventBus>,
        state_machine: Arc<StateMachine>,
        service_registry: Arc<ServiceRegistry>,
        registries: Arc<Registries>,
        config_dir: Option<PathBuf>,
        async_bridge: Arc<AsyncBridge>,
        application_credentials: ApplicationCredentialsStore,
    ) -> Self {
        Self {
            flows: RwLock::new(HashMap::new()),
            event_bus,
            state_machine,
            service_registry,
            registries,
            config_dir,
            async_bridge,
            application_credentials,
        }
    }

    /// Create a hass wrapper for config flows
    fn create_hass(&self) -> Result<PyObject, String> {
        Python::with_gil(|py| {
            create_hass_wrapper_for_config_flow(
                py,
                self.event_bus.clone(),
                self.state_machine.clone(),
                self.service_registry.clone(),
                self.registries.clone(),
                self.config_dir.as_deref(),
                self.application_credentials.clone(),
            )
            .map_err(|e| format!("Failed to create hass wrapper: {}", e))
        })
    }

    /// Create a Python config flow instance and call async_step_user
    fn create_flow_instance(
        &self,
        py: Python<'_>,
        handler: &str,
        flow_id: &str,
        _show_advanced_options: bool,
        hass: &PyObject,
    ) -> Result<(PyObject, FlowResult), String> {
        // Import the config_flow module for this integration
        let module_path = format!("homeassistant.components.{}.config_flow", handler);
        debug!("Importing config flow module: {}", module_path);

        let module = py
            .import_bound(module_path.as_str())
            .map_err(|e| format!("Failed to import {}: {}", module_path, e))?;

        // Find the ConfigFlow class (it's usually named {Domain}FlowHandler or ConfigFlow)
        // Try common naming patterns
        let flow_class = self.find_flow_class(py, &module, handler)?;

        // Instantiate the flow
        let flow_instance = flow_class
            .call0()
            .map_err(|e| format!("Failed to instantiate config flow: {}", e))?;

        // Set hass attribute on the flow
        flow_instance
            .setattr("hass", hass)
            .map_err(|e| format!("Failed to set hass on flow: {}", e))?;

        // Set flow_id - HA sets this internally
        flow_instance
            .setattr("flow_id", flow_id)
            .map_err(|e| format!("Failed to set flow_id: {}", e))?;

        // Set context - used by flows for discovery info, etc.
        let context = PyDict::new_bound(py);
        context.set_item("source", "user").unwrap();
        flow_instance
            .setattr("context", context)
            .map_err(|e| format!("Failed to set context: {}", e))?;

        // Call async_step_user(None) to get the initial form
        let result = self.call_flow_step(
            py,
            &flow_instance.clone().unbind(),
            "user",
            None,
            flow_id,
            handler,
            hass,
        )?;

        Ok((flow_instance.unbind(), result))
    }

    /// Find the ConfigFlow class for the given handler/domain
    ///
    /// Native HA uses a registry-based approach:
    /// 1. Config flow classes use `class MyFlow(ConfigFlow, domain="mydomain")`
    /// 2. When the module is imported, `ConfigFlow.__init_subclass__` registers the class
    /// 3. The class is stored in `homeassistant.config_entries.HANDLERS`
    ///
    /// We replicate this by importing the module (which triggers registration) and then
    /// looking up the handler from the HANDLERS registry.
    fn find_flow_class<'py>(
        &self,
        py: Python<'py>,
        _module: &pyo3::Bound<'py, pyo3::types::PyModule>,
        handler: &str,
    ) -> Result<pyo3::Bound<'py, pyo3::PyAny>, String> {
        debug!("Looking up config flow handler for domain: {}", handler);

        // The module import already happened, which triggered __init_subclass__
        // and registered the handler in the HANDLERS registry.
        // Now we just need to look it up from the registry.
        let config_entries = py
            .import_bound("homeassistant.config_entries")
            .map_err(|e| format!("Failed to import homeassistant.config_entries: {}", e))?;

        let handlers = config_entries
            .getattr("HANDLERS")
            .map_err(|e| format!("Failed to get HANDLERS registry: {}", e))?;

        // HANDLERS.get(domain) returns the registered class or None
        let handler_class = handlers
            .call_method1("get", (handler,))
            .map_err(|e| format!("Failed to call HANDLERS.get: {}", e))?;

        if handler_class.is_none() {
            return Err(format!(
                "No config flow handler registered for domain '{}'",
                handler
            ));
        }

        info!("Found registered config flow handler for {}", handler);
        Ok(handler_class)
    }

    /// Call a flow step and convert the result
    ///
    /// Creates a new event loop for each call to avoid "event loop already running"
    /// errors. This is needed because we may be called from various contexts.
    fn call_flow_step(
        &self,
        py: Python<'_>,
        flow_instance: &PyObject,
        step: &str,
        user_input: Option<serde_json::Value>,
        flow_id: &str,
        handler: &str,
        _hass: &PyObject,
    ) -> Result<FlowResult, String> {
        let method_name = format!("async_step_{}", step);
        debug!("Calling {} on flow", method_name);

        // Convert user_input to Python dict
        let py_input = match user_input {
            Some(input) => json_to_pyobject(py, &input),
            None => None,
        };

        // Get the method
        let flow_bound = flow_instance.bind(py);
        let method = flow_bound
            .getattr(method_name.as_str())
            .map_err(|e| format!("Flow has no method {}: {}", method_name, e))?;

        // Call the method (it's async, so we get a coroutine)
        let coro = if let Some(input) = py_input {
            method
                .call1((input,))
                .map_err(|e| format!("Failed to call {}: {}", method_name, e))?
        } else {
            method
                .call1((py.None(),))
                .map_err(|e| format!("Failed to call {}: {}", method_name, e))?
        };

        // Create a NEW event loop for this call
        // This avoids "event loop already running" errors
        let asyncio = py
            .import_bound("asyncio")
            .map_err(|e| format!("Failed to import asyncio: {}", e))?;

        let new_loop = asyncio
            .call_method0("new_event_loop")
            .map_err(|e| format!("Failed to create event loop: {}", e))?;

        // Set it as the current loop for this thread
        asyncio
            .call_method1("set_event_loop", (&new_loop,))
            .map_err(|e| format!("Failed to set event loop: {}", e))?;

        // Run the coroutine to completion
        let result = new_loop
            .call_method1("run_until_complete", (&coro,))
            .map_err(|e| format!("Failed to run coroutine: {}", e))?;

        // Close the loop when done
        let _ = new_loop.call_method0("close");

        // Convert Python result to FlowResult
        self.convert_flow_result(py, &result, flow_id, handler)
    }

    /// Convert a Python flow result dict to FlowResult
    fn convert_flow_result(
        &self,
        py: Python<'_>,
        result: &pyo3::Bound<'_, pyo3::PyAny>,
        flow_id: &str,
        handler: &str,
    ) -> Result<FlowResult, String> {
        // Result is a FlowResult TypedDict with type, step_id, data_schema, errors, etc.
        let result_type = result
            .get_item("type")
            .ok()
            .and_then(|t| {
                // FlowResultType is an enum, get its value
                if let Ok(val) = t.getattr("value") {
                    val.extract::<String>().ok()
                } else {
                    t.extract::<String>().ok()
                }
            })
            .unwrap_or_else(|| "form".to_string());

        debug!("Flow result type: {}", result_type);

        let step_id = result
            .get_item("step_id")
            .ok()
            .and_then(|s| s.extract::<String>().ok());

        let errors = result.get_item("errors").ok().and_then(|e| {
            if e.is_none() {
                return None;
            }
            let dict = e.downcast::<PyDict>().ok()?;
            let mut map = HashMap::new();
            for (k, v) in dict.iter() {
                if let (Ok(key), Ok(val)) = (k.extract::<String>(), v.extract::<String>()) {
                    map.insert(key, val);
                }
            }
            if map.is_empty() {
                None
            } else {
                Some(map)
            }
        });

        let description_placeholders =
            result
                .get_item("description_placeholders")
                .ok()
                .and_then(|e| {
                    if e.is_none() {
                        return None;
                    }
                    let dict = e.downcast::<PyDict>().ok()?;
                    let mut map = HashMap::new();
                    for (k, v) in dict.iter() {
                        if let (Ok(key), Ok(val)) = (k.extract::<String>(), v.extract::<String>()) {
                            map.insert(key, val);
                        }
                    }
                    if map.is_empty() {
                        None
                    } else {
                        Some(map)
                    }
                });

        // Convert data_schema to form fields (always returns a Vec, empty if None)
        let data_schema = result
            .get_item("data_schema")
            .ok()
            .and_then(|schema| {
                if schema.is_none() {
                    return None;
                }
                self.convert_schema_to_fields(&schema)
            })
            .unwrap_or_default();

        let title = result
            .get_item("title")
            .ok()
            .and_then(|t| t.extract::<String>().ok());

        let reason = result
            .get_item("reason")
            .ok()
            .and_then(|r| r.extract::<String>().ok());

        // For create_entry, extract the data and create the entry
        let entry_result = if result_type == "create_entry" {
            // Get the data from the result
            if let Ok(data) = result.get_item("data") {
                if let Ok(dict) = data.downcast::<PyDict>() {
                    let mut map = serde_json::Map::new();
                    for (k, v) in dict.iter() {
                        if let Ok(key) = k.extract::<String>() {
                            let val = pyobject_to_json(py, &v);
                            map.insert(key, val);
                        }
                    }
                    Some(serde_json::Value::Object(map))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Extract last_step - controls whether frontend shows "Submit" or "Next" button
        let last_step = result.get_item("last_step").ok().and_then(|v| {
            if v.is_none() {
                None
            } else {
                v.extract::<bool>().ok()
            }
        });

        // Extract preview - component name for preview in frontend
        let preview = result.get_item("preview").ok().and_then(|v| {
            if v.is_none() {
                None
            } else {
                v.extract::<String>().ok()
            }
        });

        Ok(FlowResult {
            flow_id: flow_id.to_string(),
            handler: handler.to_string(),
            result_type,
            step_id,
            data_schema,
            errors,
            description_placeholders,
            title,
            reason,
            version: Some(1),
            minor_version: Some(1),
            result: entry_result,
            last_step,
            preview,
        })
    }

    /// Convert a voluptuous schema to form fields
    ///
    /// Note: This is a simplified implementation. The full schema conversion
    /// is complex because voluptuous uses various marker types (Required, Optional)
    /// and validator types. For now, we extract basic field information.
    fn convert_schema_to_fields(
        &self,
        schema: &pyo3::Bound<'_, pyo3::PyAny>,
    ) -> Option<Vec<FormField>> {
        // Get the schema dict from the vol.Schema object
        let schema_dict = schema.getattr("schema").ok()?;

        // Try to downcast to PyDict
        let dict = schema_dict.downcast::<PyDict>().ok()?;

        let mut fields = Vec::new();

        for (key, _value) in dict.iter() {
            // The key is usually vol.Required("name") or vol.Optional("name")
            // Try to extract the field name from the marker
            let field_name = if let Ok(schema_attr) = key.getattr("schema") {
                schema_attr.extract::<String>().ok()
            } else {
                // Maybe it's just a string key
                key.extract::<String>().ok()
            };

            if let Some(name) = field_name {
                // Check if required by looking at the marker type name
                let type_name = key
                    .get_type()
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                let required = type_name.contains("Required");

                fields.push(FormField {
                    name,
                    field_type: "string".to_string(), // Default to string for now
                    required: Some(required),
                    default: None,
                });
            }
        }

        if fields.is_empty() {
            None
        } else {
            Some(fields)
        }
    }
}

#[async_trait]
impl ConfigFlowHandler for ConfigFlowManager {
    async fn start_flow(
        &self,
        handler: &str,
        show_advanced_options: bool,
    ) -> Result<FlowResult, String> {
        let flow_id = Ulid::new().to_string().to_lowercase();
        info!(
            "Starting config flow for {} with flow_id {}",
            handler, flow_id
        );

        let hass = self.create_hass()?;

        // Import and instantiate the Python config flow
        let (flow_instance, result) = Python::with_gil(|py| {
            self.create_flow_instance(py, handler, &flow_id, show_advanced_options, &hass)
        })?;

        // Store the active flow
        {
            let mut flows = self.flows.write().await;
            flows.insert(
                flow_id.clone(),
                ActiveFlow {
                    handler: handler.to_string(),
                    flow_instance,
                    current_step: result.step_id.clone().unwrap_or_else(|| "user".to_string()),
                },
            );
        }

        Ok(result)
    }

    async fn progress_flow(
        &self,
        flow_id: &str,
        user_input: Option<serde_json::Value>,
    ) -> Result<FlowResult, String> {
        let (handler, flow_instance, current_step) = {
            let flows = self.flows.read().await;
            let flow = flows
                .get(flow_id)
                .ok_or_else(|| format!("Flow {} not found", flow_id))?;
            (
                flow.handler.clone(),
                Python::with_gil(|py| flow.flow_instance.clone_ref(py)),
                flow.current_step.clone(),
            )
        };

        info!(
            "Progressing flow {} for {} at step {}",
            flow_id, handler, current_step
        );

        let hass = self.create_hass()?;
        let async_bridge = self.async_bridge.clone();
        let flow_id_owned = flow_id.to_string();
        let handler_owned = handler.clone();

        // Run the Python code on a blocking thread to avoid blocking Tokio workers
        // This is critical for long-running async operations like network I/O (e.g. async_pair())
        let result = tokio::task::spawn_blocking(move || {
            run_flow_step_blocking(
                &async_bridge,
                &flow_instance,
                &current_step,
                user_input,
                &flow_id_owned,
                &handler_owned,
                &hass,
            )
        })
        .await
        .map_err(|e| format!("Task panicked: {}", e))??;

        // Update flow state or clean up if done
        if result.result_type == "create_entry" || result.result_type == "abort" {
            let mut flows = self.flows.write().await;
            flows.remove(flow_id);
            info!(
                "Flow {} completed with result type: {}",
                flow_id, result.result_type
            );
        } else if let Some(step_id) = &result.step_id {
            let mut flows = self.flows.write().await;
            if let Some(flow) = flows.get_mut(flow_id) {
                flow.current_step = step_id.clone();
            }
        }

        Ok(result)
    }

    async fn list_flows(&self) -> Vec<serde_json::Value> {
        let flows = self.flows.read().await;
        flows
            .iter()
            .map(|(flow_id, flow)| {
                serde_json::json!({
                    "flow_id": flow_id,
                    "handler": flow.handler,
                    "step_id": flow.current_step,
                    "context": {
                        "source": "user"
                    }
                })
            })
            .collect()
    }
}

/// Convert JSON value to Python object
fn json_to_pyobject(py: Python<'_>, value: &serde_json::Value) -> Option<PyObject> {
    match value {
        serde_json::Value::Null => Some(py.None()),
        serde_json::Value::Bool(b) => Some(b.into_py(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Some(f.into_py(py))
            } else {
                None
            }
        }
        serde_json::Value::String(s) => Some(s.into_py(py)),
        serde_json::Value::Array(arr) => {
            let list = pyo3::types::PyList::empty_bound(py);
            for item in arr {
                if let Some(py_item) = json_to_pyobject(py, item) {
                    list.append(py_item).ok()?;
                }
            }
            Some(list.into())
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in obj {
                if let Some(py_val) = json_to_pyobject(py, v) {
                    dict.set_item(k, py_val).ok()?;
                }
            }
            Some(dict.into())
        }
    }
}

/// Convert Python object to JSON value
fn pyobject_to_json(py: Python<'_>, obj: &pyo3::Bound<'_, pyo3::PyAny>) -> serde_json::Value {
    if obj.is_none() {
        return serde_json::Value::Null;
    }
    if let Ok(b) = obj.extract::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(i) = obj.extract::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Ok(f) = obj.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    if let Ok(s) = obj.extract::<String>() {
        return serde_json::Value::String(s);
    }
    if let Ok(list) = obj.downcast::<pyo3::types::PyList>() {
        let arr: Vec<serde_json::Value> = list
            .iter()
            .map(|item| pyobject_to_json(py, &item))
            .collect();
        return serde_json::Value::Array(arr);
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            if let Ok(key) = k.extract::<String>() {
                map.insert(key, pyobject_to_json(py, &v));
            }
        }
        return serde_json::Value::Object(map);
    }
    // Default to string representation
    serde_json::Value::String(obj.to_string())
}
