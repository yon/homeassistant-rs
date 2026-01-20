//! Utility functions for Python ↔ JSON conversion

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Convert a Python value to serde_json::Value
pub fn py_to_json(value: &Bound<'_, PyAny>) -> serde_json::Value {
    if value.is_none() {
        return serde_json::Value::Null;
    }
    if let Ok(b) = value.extract::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(i) = value.extract::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Ok(f) = value.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    if let Ok(s) = value.extract::<String>() {
        return serde_json::Value::String(s);
    }
    if let Ok(list) = value.downcast::<PyList>() {
        let arr: Vec<serde_json::Value> = list.iter().map(|item| py_to_json(&item)).collect();
        return serde_json::Value::Array(arr);
    }
    if let Ok(dict) = value.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            if let Ok(key) = k.extract::<String>() {
                map.insert(key, py_to_json(&v));
            }
        }
        return serde_json::Value::Object(map);
    }
    // Default to string representation
    serde_json::Value::String(value.to_string())
}

/// Convert serde_json::Value to Python object
pub fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
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
            let list = PyList::empty_bound(py);
            for item in arr {
                list.append(json_to_py(py, item)?)?;
            }
            Ok(list.into())
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in obj {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into())
        }
    }
}
