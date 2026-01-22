//! Python wrappers for core types

use ha_core::{Context, EntityId, Event, State};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;

/// Python wrapper for EntityId
#[pyclass(name = "EntityId")]
#[derive(Clone)]
pub struct PyEntityId {
    inner: EntityId,
}

#[pymethods]
impl PyEntityId {
    #[new]
    fn new(entity_id: &str) -> PyResult<Self> {
        let inner: EntityId = entity_id
            .parse()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;
        Ok(Self { inner })
    }

    #[getter]
    fn domain(&self) -> &str {
        self.inner.domain()
    }

    #[getter]
    fn object_id(&self) -> &str {
        self.inner.object_id()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("EntityId('{}')", self.inner)
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.to_string().hash(&mut hasher);
        hasher.finish()
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl PyEntityId {
    pub fn into_inner(self) -> EntityId {
        self.inner
    }

    pub fn from_inner(inner: EntityId) -> Self {
        Self { inner }
    }
}

/// Python wrapper for Context
#[pyclass(name = "Context")]
#[derive(Clone)]
pub struct PyContext {
    inner: Context,
}

#[pymethods]
impl PyContext {
    #[new]
    #[pyo3(signature = (id=None, user_id=None, parent_id=None))]
    fn new(id: Option<String>, user_id: Option<String>, parent_id: Option<String>) -> Self {
        let mut ctx = if let Some(id) = id {
            Context::with_id(id)
        } else {
            Context::new()
        };
        ctx.user_id = user_id;
        ctx.parent_id = parent_id;
        Self { inner: ctx }
    }

    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn user_id(&self) -> Option<&str> {
        self.inner.user_id.as_deref()
    }

    #[getter]
    fn parent_id(&self) -> Option<&str> {
        self.inner.parent_id.as_deref()
    }

    /// Return dictionary representation of context
    fn as_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new_bound(py);
        dict.set_item("id", &self.inner.id)?;
        dict.set_item("parent_id", self.inner.parent_id.as_deref())?;
        dict.set_item("user_id", self.inner.user_id.as_deref())?;
        Ok(dict.into_any().unbind())
    }

    fn __repr__(&self) -> String {
        format!(
            "Context(id='{}', user_id={:?})",
            self.inner.id, self.inner.user_id
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner.id == other.inner.id
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.id.hash(&mut hasher);
        hasher.finish()
    }
}

impl PyContext {
    pub fn into_inner(self) -> Context {
        self.inner
    }

    pub fn from_inner(inner: Context) -> Self {
        Self { inner }
    }
}

impl Default for PyContext {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}

/// Python wrapper for State
#[pyclass(name = "State", subclass)]
#[derive(Clone)]
pub struct PyState {
    inner: State,
}

#[pymethods]
impl PyState {
    #[getter]
    fn entity_id(&self) -> String {
        self.inner.entity_id.to_string()
    }

    #[getter]
    fn domain(&self) -> &str {
        self.inner.entity_id.domain()
    }

    #[getter]
    fn object_id(&self) -> &str {
        self.inner.entity_id.object_id()
    }

    #[getter]
    fn state(&self) -> &str {
        &self.inner.state
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new_bound(py);
        for (key, value) in &self.inner.attributes {
            let py_value = json_to_py(py, value)?;
            dict.set_item(key, py_value)?;
        }
        Ok(dict.unbind())
    }

    #[getter]
    fn last_changed(&self) -> String {
        self.inner.last_changed.to_rfc3339()
    }

    #[getter]
    fn last_updated(&self) -> String {
        self.inner.last_updated.to_rfc3339()
    }

    /// Return the friendly name of the entity
    #[getter]
    fn name(&self) -> Option<String> {
        self.inner
            .attributes
            .get("friendly_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Return dictionary representation of state
    fn as_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new_bound(py);
        dict.set_item("entity_id", self.inner.entity_id.to_string())?;
        dict.set_item("state", &self.inner.state)?;

        // Convert attributes
        let attrs = PyDict::new_bound(py);
        for (key, value) in &self.inner.attributes {
            let py_value = json_to_py(py, value)?;
            attrs.set_item(key, py_value)?;
        }
        dict.set_item("attributes", attrs)?;

        dict.set_item("last_changed", self.inner.last_changed.to_rfc3339())?;
        dict.set_item("last_updated", self.inner.last_updated.to_rfc3339())?;

        Ok(dict.into_any().unbind())
    }

    fn __repr__(&self) -> String {
        format!(
            "<state {}={} @ {}>",
            self.inner.entity_id,
            self.inner.state,
            self.inner.last_changed.to_rfc3339()
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner.entity_id == other.inner.entity_id
            && self.inner.state == other.inner.state
            && self.inner.attributes == other.inner.attributes
    }
}

impl PyState {
    pub fn from_inner(inner: State) -> Self {
        Self { inner }
    }
}

/// Python wrapper for Event
///
/// Events are stored as Arc<Event> to avoid cloning event data.
/// This matches the EventBus optimization where events are broadcast as Arc.
#[pyclass(name = "Event")]
#[derive(Clone)]
pub struct PyEvent {
    inner: std::sync::Arc<Event<serde_json::Value>>,
}

#[pymethods]
impl PyEvent {
    #[getter]
    fn event_type(&self) -> &str {
        self.inner.event_type.as_str()
    }

    #[getter]
    fn data(&self, py: Python<'_>) -> PyResult<PyObject> {
        json_to_py(py, &self.inner.data)
    }

    #[getter]
    fn time_fired(&self) -> String {
        self.inner.time_fired.to_rfc3339()
    }

    #[getter]
    fn context(&self) -> PyContext {
        PyContext::from_inner(self.inner.context.clone())
    }

    /// Return dictionary representation of event
    fn as_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new_bound(py);
        dict.set_item("event_type", self.inner.event_type.as_str())?;
        dict.set_item("data", json_to_py(py, &self.inner.data)?)?;
        dict.set_item("origin", "LOCAL")?; // Default origin
        dict.set_item("time_fired", self.inner.time_fired.to_rfc3339())?;

        // Add context as dict
        let ctx_dict = PyDict::new_bound(py);
        ctx_dict.set_item("id", &self.inner.context.id)?;
        ctx_dict.set_item("parent_id", self.inner.context.parent_id.as_deref())?;
        ctx_dict.set_item("user_id", self.inner.context.user_id.as_deref())?;
        dict.set_item("context", ctx_dict)?;

        Ok(dict.into_any().unbind())
    }

    fn __repr__(&self) -> String {
        format!("<Event {}>", self.inner.event_type)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner.event_type == other.inner.event_type
            && self.inner.data == other.inner.data
            && self.inner.context.id == other.inner.context.id
    }
}

impl PyEvent {
    /// Create a PyEvent from an Arc<Event> (cheap, no clone)
    pub fn from_arc(inner: std::sync::Arc<Event<serde_json::Value>>) -> Self {
        Self { inner }
    }

    /// Create a PyEvent from an owned Event (wraps in Arc)
    pub fn from_inner(inner: Event<serde_json::Value>) -> Self {
        Self {
            inner: std::sync::Arc::new(inner),
        }
    }
}

/// Convert serde_json::Value to Python object
pub fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    use pyo3::IntoPy;

    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.into_py(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => Ok(s.into_py(py)),
        serde_json::Value::Array(arr) => {
            let list: Vec<PyObject> = arr
                .iter()
                .map(|item| json_to_py(py, item))
                .collect::<PyResult<_>>()?;
            Ok(list.into_py(py))
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in obj {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

/// Convert Python object to serde_json::Value
pub fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::json!(i))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(serde_json::json!(f))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else if let Ok(list) = obj.downcast::<pyo3::types::PyList>() {
        let arr: Result<Vec<_>, _> = list.iter().map(|item| py_to_json(&item)).collect();
        Ok(serde_json::Value::Array(arr?))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            map.insert(key, py_to_json(&v)?);
        }
        Ok(serde_json::Value::Object(map))
    } else {
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "Cannot convert Python object to JSON",
        ))
    }
}

/// Convert Python dict to HashMap<String, serde_json::Value>
pub fn py_dict_to_hashmap(
    dict: &Bound<'_, PyDict>,
) -> PyResult<HashMap<String, serde_json::Value>> {
    let mut map = HashMap::new();
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        map.insert(key, py_to_json(&v)?);
    }
    Ok(map)
}
